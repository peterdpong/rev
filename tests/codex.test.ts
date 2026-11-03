import { test, type TestContext } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { codex } from '../core/providers/codex';
import type { ProviderTurn } from '../core/providers/types';
import { writeFakeCodex } from './fake-codex';

async function fixture(t: TestContext, source: string, setup = '') {
  const cwd = await mkdtemp(path.join(tmpdir(), 'rev-codex-'));
  t.after(() => rm(cwd, { recursive: true, force: true }));
  const executable = path.join(cwd, 'codex');
  await writeFakeCodex(executable, source, setup);
  const turn: ProviderTurn = {
    cwd,
    executable,
    mode: 'ask',
    prompt: 'Explain',
    signal: new AbortController().signal,
    emit() {},
    async saveSession() {},
    async approve() {
      return false;
    },
  };
  return turn;
}

test('Codex starts and resumes threads, handles early events, and ignores unrelated turns', async (t) => {
  const turn = await fixture(
    t,
    `
    event('item/agentMessage/delta', { itemId: 'message-fixture', delta: 'Hel' });
    event('item/agentMessage/delta', { itemId: 'message-fixture', delta: 'lo' });
    event('item/agentMessage/delta', { threadId: 'unrelated', itemId: 'wrong', delta: 'Ignore me' });
    text('Hello'); complete();`,
    `globalThis.early = true;
    globalThis.onThread = r => { assert.equal(r.params.sandbox, 'read-only'); assert.equal(r.params.approvalPolicy, 'never'); };`,
  );
  let saved = '';
  const output: string[] = [];
  turn.saveSession = async (id) => {
    saved = id;
  };
  turn.emit = (event) => output.push(event.text);
  await codex.run(turn);
  assert.ok(saved);
  assert.ok(output.includes('Hello'));
  assert.ok(!output.includes('Ignore me'));
  const first = saved;
  await codex.run({ ...turn, sessionId: first });
  assert.equal(saved, first);
});

test('Ask declines approvals; Apply delegates approval to its caller', async (t) => {
  const turn = await fixture(
    t,
    `
    send({ id: 900, method: 'item/fileChange/requestApproval', params: { threadId, turnId, reason: 'Fixture' } });
    globalThis.onReply = response => { text(response.result.decision); complete(); };`,
  );
  let calls = 0;
  const output: string[] = [];
  turn.approve = async () => {
    calls++;
    return true;
  };
  turn.emit = (event) => output.push(event.text);
  await codex.run(turn);
  assert.equal(calls, 0);
  assert.ok(output.includes('decline'));
  await codex.run({ ...turn, mode: 'apply' });
  assert.equal(calls, 1);
  assert.ok(output.includes('accept'));
});

test('provider exit without completion rejects instead of reporting success', async (t) => {
  const turn = await fixture(t, "text('Partial'); process.exit(0);");
  await assert.rejects(codex.run(turn), /exited/);
});

test(
  'cancellation terminates the provider and settles the outstanding turn',
  { timeout: 10000 },
  async (t) => {
    const turn = await fixture(t, 'setInterval(() => {}, 1000);');
    const controller = new AbortController();
    const pending = codex.run({ ...turn, signal: controller.signal });
    const timer = setTimeout(() => controller.abort(), 100);
    try {
      await assert.rejects(pending, /cancelled|aborted|closed|exited/i);
    } finally {
      clearTimeout(timer);
    }
  },
);
