import { createServer, type IncomingMessage } from 'node:http';
import { randomBytes } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { homedir } from 'node:os';
import { inspect, filePatch, fetchRepo, cloneRepo, importPR } from './core/git';
import { readState, addComment, mutateState } from './core/store';
import { AgentRunner, preparePrompt, providers } from './core/agents';
import { createDemo } from './core/demo';
import { readWorkspace, rememberReview, updateWorkspace, applyWorkspaceAction } from './core/workspace';
import type { ReviewRequest } from './shared';

async function body(req: IncomingMessage) {
  let text = ''; for await (const chunk of req) { text += chunk.toString(); if (text.length > 1_000_000) throw new Error('Request too large.'); }
  return JSON.parse(text);
}
export async function startServer(options: { uiDir: string; initial?: ReviewRequest; port?: number; dataDir?: string }) {
  const token = randomBytes(32).toString('hex'); const runner = new AgentRunner();
  const dataDir = options.dataDir || process.env.REVUE_DATA_DIR || path.join(homedir(), '.revue');
  let origin = ''; let sample: string | undefined;
  const server = createServer(async (req, res) => {
    const json = (status: number, data: unknown) => { res.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' }); res.end(JSON.stringify(data)); };
    if (req.headers.host !== origin.replace('http://', '')) return json(403, { error: 'Invalid host.' });
    if (req.headers.origin && req.headers.origin !== origin) return json(403, { error: 'Invalid origin.' });
    try {
      if (req.url === '/api') {
        if (req.method !== 'POST' || req.headers['x-revue-token'] !== token) return json(403, { error: 'Reopen Revue from its launcher to connect.' });
        const { method, params: p = {} } = await body(req);
        let result: unknown;
        switch (method) {
          case 'init': result = { initial: options.initial, providers: await providers(), workspace: await readWorkspace(dataDir) }; break;
          case 'workspaceAction': result = await updateWorkspace(dataDir, workspace => applyWorkspaceAction(workspace, p)); break;
          case 'review': { const review = await inspect(p); const workspace = p.remember ? await updateWorkspace(dataDir, workspace => { rememberReview(workspace, review); }) : undefined; result = { review, state: await readState(review), workspace }; break; }
          case 'patch': result = await filePatch(await inspect(p.review), p.path); break;
          case 'comment': result = await addComment(await inspect(p.review), p.selection, p.body, p.digest); break;
          case 'deleteComment': result = await mutateState(await inspect(p.review), s => { s.comments = s.comments.filter(c => c.id !== p.id); }); break;
          case 'viewed': { const review = await inspect(p.review); const patch = await filePatch(review, p.path); if (p.digest && patch.digest !== p.digest) throw new Error('This file changed while you were reviewing it. Refresh before marking it viewed.'); result = await mutateState(review, s => { if (p.viewed) s.viewed[p.path] = patch.digest; else delete s.viewed[p.path]; }); break; }
          case 'fetch': await fetchRepo(p.repo); result = true; break;
          case 'clone': result = { repo: await cloneRepo(p.url, p.destination) }; break;
          case 'pr': result = await importPR(p.repo, p.value); break;
          case 'demo': sample ||= await createDemo(); result = { repo: sample, base: 'main', head: 'HEAD' }; break;
          case 'prompt': result = { prompt: (await preparePrompt(p)).prompt }; break;
          case 'run': result = await runner.start(p); break;
          case 'runStatus': result = runner.runs.get(p.id); if (!result) throw new Error('This run is no longer available.'); break;
          case 'cancel': runner.cancel(p.id); result = true; break;
          default: return json(404, { error: 'Unknown action.' });
        }
        return json(200, result);
      }
      if (req.method !== 'GET') return json(405, { error: 'Method not allowed.' });
      const pathname = decodeURIComponent(new URL(req.url || '/', origin).pathname);
      const file = path.resolve(options.uiDir, '.' + (pathname === '/' ? '/index.html' : pathname));
      if (!file.startsWith(path.resolve(options.uiDir) + path.sep)) return json(403, { error: 'Invalid path.' });
      const mime: Record<string, string> = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.svg': 'image/svg+xml', '.woff2': 'font/woff2', '.wasm': 'application/wasm' };
      const data = await readFile(file);
      res.writeHead(200, { 'Content-Type': mime[path.extname(file)] || 'application/octet-stream', 'Content-Security-Policy': "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; worker-src 'self' blob:; frame-ancestors 'none'; object-src 'none'; base-uri 'none'", 'X-Content-Type-Options': 'nosniff', 'Referrer-Policy': 'no-referrer' }); res.end(data);
    } catch (error: any) { json(error.code === 'ENOENT' ? 404 : 400, { error: error.message }); }
  });
  await new Promise<void>((resolve, reject) => { server.once('error', reject); server.listen(options.port || 0, '127.0.0.1', () => resolve()); });
  const address = server.address(); if (!address || typeof address === 'string') throw new Error('Could not start the local service.');
  origin = `http://127.0.0.1:${address.port}`;
  return { url: `${origin}/#${token}`, origin, token, server, runner, close: () => { runner.stopAll(); server.close(); server.closeAllConnections(); } };
}
