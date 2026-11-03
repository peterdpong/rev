import { createServer, type IncomingMessage } from 'node:http';
import { randomBytes } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { HostService } from './host';
import type { ReviewRequest } from './types';

async function body(req: IncomingMessage) {
  let text = '';
  for await (const chunk of req) {
    text += chunk.toString();
    if (text.length > 1_000_000) {
      throw new Error('Request too large.');
    }
  }
  return JSON.parse(text);
}

export async function startServer(options: {
  uiDir: string;
  initial?: ReviewRequest;
  port?: number;
  dataDir?: string;
}) {
  const token = randomBytes(32).toString('hex');
  const host = new HostService(options.initial, options.dataDir);
  const runner = host.runner;
  let origin = '';
  const server = createServer(async (req, res) => {
    const json = (status: number, data: unknown) => {
      res.writeHead(status, {
        'Content-Type': 'application/json',
        'Cache-Control': 'no-store',
      });
      res.end(JSON.stringify(data));
    };
    if (req.headers.host !== origin.replace('http://', '')) {
      return json(403, { error: 'Invalid host.' });
    }
    if (req.headers.origin && req.headers.origin !== origin) {
      return json(403, { error: 'Invalid origin.' });
    }
    try {
      if (req.url === '/api') {
        if (req.method !== 'POST' || req.headers['x-revue-token'] !== token) {
          return json(403, {
            error: 'Reopen Revue from its launcher to connect.',
          });
        }
        const { method, params: p = {} } = await body(req);
        let result: unknown;
        result = await host.call(method, p);
        return json(200, result);
      }
      if (req.method !== 'GET') {
        return json(405, { error: 'Method not allowed.' });
      }
      const pathname = decodeURIComponent(
        new URL(req.url || '/', origin).pathname,
      );
      const file = path.resolve(
        options.uiDir,
        '.' + (pathname === '/' ? '/index.html' : pathname),
      );
      if (!file.startsWith(path.resolve(options.uiDir) + path.sep)) {
        return json(403, { error: 'Invalid path.' });
      }
      const mime: Record<string, string> = {
        '.html': 'text/html',
        '.js': 'text/javascript',
        '.css': 'text/css',
        '.svg': 'image/svg+xml',
        '.woff2': 'font/woff2',
        '.wasm': 'application/wasm',
      };
      const data = await readFile(file);
      res.writeHead(200, {
        'Content-Type': mime[path.extname(file)] || 'application/octet-stream',
        'Content-Security-Policy':
          "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; worker-src 'self' blob:; frame-ancestors 'none'; object-src 'none'; base-uri 'none'",
        'X-Content-Type-Options': 'nosniff',
        'Referrer-Policy': 'no-referrer',
      });
      res.end(data);
    } catch (error: any) {
      json(error.code === 'ENOENT' ? 404 : 400, { error: error.message });
    }
  });
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(options.port || 0, '127.0.0.1', () => resolve());
  });
  const address = server.address();
  if (!address || typeof address === 'string') {
    throw new Error('Could not start the local service.');
  }
  origin = `http://127.0.0.1:${address.port}`;
  return {
    url: `${origin}/#${token}`,
    origin,
    token,
    server,
    runner,
    close: () => {
      runner.stopAll();
      server.close();
      server.closeAllConnections();
    },
  };
}
