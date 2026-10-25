import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { mkdtemp, rm, writeFile, chmod } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { createDemo } from '../src/core/demo';

async function fixture(t: any, env: Record<string, string> = {}) {
  const root = await mkdtemp(path.join(tmpdir(), 'patch-host-'));
  const repo = await createDemo(path.join(root, 'repo'));
  const child = spawn(process.execPath, ['--import', 'tsx', 'src/host.ts'], {
    cwd: process.cwd(), env: { ...process.env, PATCH_DATA_DIR: path.join(root, 'data'), ...env }, stdio: ['pipe', 'pipe', 'pipe'],
  });
  let serial = 0;
  const pending = new Map<number, { resolve: (value: any) => void; reject: (error: Error) => void }>();
  const lines = createInterface({ input: child.stdout });
  lines.on('line', line => {
    const reply = JSON.parse(line); const listener = pending.get(reply.id);
    if (!listener) return;
    pending.delete(reply.id);
    if (reply.error) listener.reject(new Error(reply.error)); else listener.resolve(reply.result);
  });
  const closed = new Promise<void>(resolve => child.once('close', () => resolve()));
  const call = (method: string, params: unknown = {}) => new Promise<any>((resolve, reject) => {
    const id = ++serial; pending.set(id, { resolve, reject }); child.stdin.write(JSON.stringify({ id, method, params }) + '\n');
  });
  t.after(async () => { child.stdin.end(); await closed; lines.close(); await rm(root, { recursive: true, force: true }); });
  return { call, repo, root, child, closed };
}

test('stdio transport keeps requests correlated and persists the native review workflow', { timeout: 15000 }, async t => {
  const { call, repo } = await fixture(t);
  const request = { repo, base: 'main', head: 'HEAD' };
  const [init, result] = await Promise.all([call('init'), call('review', { ...request, remember: true })]);
  assert.ok(Array.isArray(init.providers));
  assert.equal(result.review.files.length, 4);
  assert.equal(result.workspace.projects[0].tabs[0].request.head, 'feat/resilient-requests');
  const patch = await call('patch', { review: request, path: 'src/lib/retry.ts' });
  const state = await call('comment', { review: request, selection: { path: 'src/lib/retry.ts', side: 'additions', start: 4, end: 6 }, body: 'Handle cancellation', digest: patch.digest });
  const prompt = await call('prompt', { review: request, provider: 'codex', mode: 'apply', commentIds: [state.comments[0].id] });
  assert.match(prompt.prompt, /Handle cancellation/);
  const viewed = await call('viewed', { review: request, path: 'src/lib/retry.ts', digest: patch.digest, viewed: true });
  assert.equal(viewed.viewed['src/lib/retry.ts'], patch.digest);
  await call('workspaceAction', { type: 'setFile', id: result.review.key, file: 'src/lib/retry.ts' });
  assert.equal((await call('init')).workspace.projects[0].tabs.find((tab: any) => tab.id === result.review.key).file, 'src/lib/retry.ts');
  await call('deleteComment', { review: request, id: state.comments[0].id });
  assert.equal((await call('review', request)).state.comments.length, 0);
  const working = { ...request, mode: 'working' };
  const captured = await call('patch', { review: working, path: 'src/lib/retry.ts' });
  await writeFile(path.join(repo, 'src/lib/retry.ts'), 'export const changed = true;\n');
  await assert.rejects(call('prompt', { review: working, provider: 'codex', mode: 'ask', question: 'Explain', selection: { path: 'src/lib/retry.ts', side: 'additions', start: 4, end: 6 }, selectionDigest: captured.digest }), /changed since you selected/);
  await assert.rejects(call('unknown'), /Unknown action/);
  assert.ok((await call('init')).workspace.projects.length, 'an invalid method must not kill the host');
});

test('closing the desktop pipe cancels its running agent cleanly', { timeout: 15000 }, async t => {
  const agentDir = await mkdtemp(path.join(tmpdir(), 'patch-agent-'));
  t.after(() => rm(agentDir, { recursive: true, force: true }));
  const fake = path.join(agentDir, 'codex');
  await writeFile(fake, '#!/usr/bin/env node\nprocess.stdin.resume(); process.stdin.on("end", () => { console.log(JSON.stringify({type:"item.started",item:{type:"command_execution",command:"fixture waiting"}})); setInterval(() => {}, 1000); });\n');
  await chmod(fake, 0o755);
  const { call, repo, child, closed } = await fixture(t, { REVUE_CODEX_PATH: fake });
  const review = { repo, base: 'main', head: 'HEAD' };
  const run = await call('run', { review, provider: 'codex', mode: 'ask', question: 'Explain the retry change' });
  assert.equal(run.status, 'running');
  let status;
  for (let i = 0; i < 50; i++) {
    status = await call('runStatus', { id: run.id });
    if (status.events.some((event: any) => event.text === 'fixture waiting')) break;
    await new Promise(resolve => setTimeout(resolve, 25));
  }
  assert.ok(status.events.some((event: any) => event.text === 'fixture waiting'));
  child.stdin.end(); await closed;
  assert.equal(child.exitCode, 0);
});
