import { writeFile, chmod } from 'node:fs/promises';

// A real subprocess speaking the app-server protocol; no inference or login.
export async function writeFakeCodex(file: string, turn: string, setup = '') {
  await writeFile(
    file,
    `#!${process.execPath}
const fs = require('node:fs');
const { randomUUID } = require('node:crypto');
const assert = require('node:assert/strict');
assert.equal(process.argv[2], 'app-server');
let initialized = false, threadId, turnId = 'turn-fixture';
const send = value => process.stdout.write(JSON.stringify(value) + '\\n');
const event = (method, params) => send({ method, params: { threadId, turnId, ...params } });
const text = text => event('item/completed', { item: { id: 'message-fixture', type: 'agentMessage', text } });
const complete = (status = 'completed') => event('turn/completed', { turn: { id: turnId, status, error: status === 'failed' ? { message: 'provider failed' } : null } });
${setup}
require('node:readline').createInterface({ input: process.stdin }).on('line', line => {
 const request = JSON.parse(line);
 if (!request.method) { if (globalThis.onReply) globalThis.onReply(request); return; }
 if (request.method === 'initialize') { send({ id: request.id, result: {} }); return; }
 if (request.method === 'initialized') { initialized = true; return; }
 assert.ok(initialized);
 if (request.method === 'thread/start' || request.method === 'thread/resume') {
   threadId = request.params.threadId || randomUUID();
   assert.ok(['read-only', 'workspace-write'].includes(request.params.sandbox));
   if (globalThis.onThread) globalThis.onThread(request);
   send({ id: request.id, result: { thread: { id: threadId } } }); return;
 }
 if (request.method === 'turn/start') {
   assert.equal(request.params.threadId, threadId);
   if (!globalThis.early) send({ id: request.id, result: { turn: { id: turnId, status: 'inProgress' } } });
   const prompt = request.params.input[0].text;
   ${turn}
   if (globalThis.early) send({ id: request.id, result: { turn: { id: turnId, status: 'inProgress' } } });
   return;
 }
 send({ id: request.id, error: { code: -32601, message: 'Unexpected method: ' + request.method } });
});
`,
  );
  await chmod(file, 0o755);
}
