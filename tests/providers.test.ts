import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  mkdtemp,
  rm,
  writeFile,
  readFile,
  symlink,
  chmod,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { randomUUID } from 'node:crypto';
import type { Options, SDKMessage } from '@anthropic-ai/claude-agent-sdk';
import { AgentRunner } from '../core/agents';
import { createDemo } from '../core/demo';
import { git } from '../core/git';
import { claude, claudeAdapter, claudeOptions } from '../core/providers/claude';
import type { ProviderTurn } from '../core/providers/types';
import { writeFakeCodex } from './fake-codex';

async function until(condition: () => boolean) {
  for (let i = 0; i < 250; i++) {
    if (condition()) {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
  assert.fail('Timed out waiting for provider');
}

async function fixture(
  t: any,
  source = "text('Hello'); complete();",
  setup = '',
) {
  const root = await mkdtemp(path.join(tmpdir(), 'rev-provider-'));
  const repo = await createDemo(path.join(root, 'repo'));
  const binary = path.join(root, 'codex');
  await writeFakeCodex(binary, source, setup);
  const old = process.env.REV_CODEX_PATH;
  process.env.REV_CODEX_PATH = binary;
  t.after(async () => {
    if (old === undefined) {
      delete process.env.REV_CODEX_PATH;
    } else {
      process.env.REV_CODEX_PATH = old;
    }
    await rm(root, { recursive: true, force: true });
  });
  const request = {
    review: { repo, base: 'main', head: 'HEAD' },
    provider: 'codex' as const,
    mode: 'ask' as const,
    question: 'Explain this diff',
  };
  return { root, repo, binary, request };
}

test('Codex resumes saved conversations across runners, resets on request and on new commits', async (t) => {
  const { repo, request } = await fixture(t);
  const run = await new AgentRunner(
    undefined,
    path.join(request.review.repo, '.git', 'agent-worktrees'),
  ).start(request);
  await until(() => run.status !== 'running');
  assert.equal(run.status, 'completed');
  assert.ok(run.sessionId);
  const resumed = await new AgentRunner(
    undefined,
    path.join(request.review.repo, '.git', 'agent-worktrees'),
  ).start({
    ...request,
    threadId: run.thread!.id,
  });
  await until(() => resumed.status !== 'running');
  assert.equal(resumed.sessionId, run.sessionId);
  const fresh = await new AgentRunner(
    undefined,
    path.join(request.review.repo, '.git', 'agent-worktrees'),
  ).start({ ...request, newSession: true });
  await until(() => fresh.status !== 'running');
  assert.equal(fresh.status, 'completed');
  assert.notEqual(fresh.sessionId, run.sessionId);
  const working = await new AgentRunner(
    undefined,
    path.join(request.review.repo, '.git', 'agent-worktrees'),
  ).start({
    ...request,
    review: { ...request.review, mode: 'working' },
  });
  await until(() => working.status !== 'running');
  assert.notEqual(working.sessionId, fresh.sessionId);
  await git(repo, [
    '-c',
    'user.name=Test',
    '-c',
    'user.email=test@example.invalid',
    'commit',
    '--allow-empty',
    '-m',
    'new revision',
  ]);
  const moved = await new AgentRunner(
    undefined,
    path.join(request.review.repo, '.git', 'agent-worktrees'),
  ).start(request);
  await until(() => moved.status !== 'running');
  assert.notEqual(moved.sessionId, fresh.sessionId);
});

test('Codex handles early deltas, ignores other turns, and replaces partial messages', async (t) => {
  const { request } = await fixture(
    t,
    `
    event('turn/completed', { turn: { id: 'other-turn', status: 'failed' } });
    event('item/agentMessage/delta', { itemId: 'message-fixture', delta: 'Hel' });
    event('item/agentMessage/delta', { itemId: 'message-fixture', delta: 'lo' });
    text('Hello!'); complete();`,
    'globalThis.early = true;',
  );
  const run = await new AgentRunner(
    undefined,
    path.join(request.review.repo, '.git', 'agent-worktrees'),
  ).start(request);
  await until(() => run.status !== 'running');
  assert.equal(run.status, 'completed');
  assert.deepEqual(
    run.events.filter((e) => e.kind === 'message').map((e) => e.text),
    ['Hello!'],
  );
});

test('Ask denies permission escalation, and unknown interactions receive an error', async (t) => {
  const { request } = await fixture(
    t,
    `
    globalThis.onReply = reply => {
      if (reply.id === 'approval') { assert.equal(reply.result.decision, 'decline'); send({ id: 'unknown', method: 'future/interaction', params: { threadId, turnId } }); }
      else { assert.equal(reply.error.code, -32601); text('Denied safely'); complete(); }
    };
    send({ id: 'approval', method: 'item/commandExecution/requestApproval', params: { threadId, turnId, command: 'touch forbidden' } });`,
  );
  const run = await new AgentRunner(
    undefined,
    path.join(request.review.repo, '.git', 'agent-worktrees'),
  ).start(request);
  await until(() => run.status !== 'running');
  assert.equal(run.status, 'completed');
  assert.equal(run.approvals.length, 0);
});

test('Apply approvals are scoped to their run, wait for a decision, and cancel cleanly', async (t) => {
  const { request, repo } = await fixture(
    t,
    `
    globalThis.onReply = reply => { text(reply.result.decision); complete(); };
    send({ id: 'approval', method: 'item/commandExecution/requestApproval', params: { threadId, turnId, command: 'npm test', reason: 'Needs access' } });`,
  );
  const { inspect } = await import('../core/git');
  const { addComment, readState } = await import('../core/store');
  await addComment(
    await inspect(request.review),
    { path: 'src/lib/retry.ts', side: 'additions', start: 4, end: 6 },
    'Handle cancellation',
  );
  const runner = new AgentRunner(
    undefined,
    path.join(request.review.repo, '.git', 'agent-worktrees'),
  );
  const run = await runner.start({ ...request, mode: 'apply' });
  await until(() => run.approvals.length > 0);
  assert.equal(run.status, 'running');
  assert.throws(
    () => runner.respondToApproval('another-run', run.approvals[0].id, true),
    /no longer pending/,
  );
  const id = run.approvals[0].id;
  runner.respondToApproval(run.id, id, true);
  assert.match(run.events.at(-1)!.text, /Approved/);
  assert.equal(run.approvals.length, 0);
  assert.throws(
    () => runner.respondToApproval(run.id, id, true),
    /no longer pending/,
  );
  await until(() => run.status !== 'running');
  assert.equal(run.status, 'completed');
  assert.ok(run.events.some((e) => e.text === 'accept'));
  await addComment(
    await inspect(request.review),
    { path: 'src/lib/retry.ts', side: 'additions', start: 4, end: 6 },
    'Second request',
  );
  const stopped = await runner.start({ ...request, mode: 'apply' });
  await until(() => stopped.approvals.length > 0);
  runner.cancel(stopped.id);
  await until(() => stopped.status !== 'running');
  assert.equal(stopped.status, 'cancelled');
  assert.equal(stopped.approvals.length, 0);
  assert.equal(
    (await readState(await inspect({ repo, base: 'main' }))).comments.at(-1)
      ?.status,
    'queued',
  );
});

test('provider exit without completion fails the turn', async (t) => {
  const { request } = await fixture(t, "text('Partial'); process.exit(0);");
  const run = await new AgentRunner(
    undefined,
    path.join(request.review.repo, '.git', 'agent-worktrees'),
  ).start(request);
  await until(() => run.status !== 'running');
  assert.equal(run.status, 'failed');
  assert.ok(run.events.some((e) => e.kind === 'error'));
});

function context(
  cwd: string,
  overrides: Partial<ProviderTurn> = {},
): ProviderTurn {
  return {
    cwd,
    executable: '/unused/claude',
    mode: 'ask',
    prompt: 'Explain',
    signal: new AbortController().signal,
    emit() {},
    async saveSession() {},
    async approve() {
      return false;
    },
    ...overrides,
  };
}

test('Claude SDK receives resume options, streams one message, and requires successful result', async () => {
  let options: Options | undefined;
  let closed = false;
  let saved = '';
  const events: any[] = [];
  const session = randomUUID();
  const adapter = claudeAdapter((args) => {
    options = args.options;
    return {
      close() {
        closed = true;
      },
      async *[Symbol.asyncIterator]() {
        yield {
          type: 'assistant',
          session_id: session,
          message: {
            id: 'message',
            content: [{ type: 'text', text: 'Answer' }],
          },
        } as SDKMessage;
        yield {
          type: 'result',
          subtype: 'success',
          is_error: false,
          session_id: session,
        } as SDKMessage;
      },
    };
  });
  await adapter.run(
    context('/tmp', {
      sessionId: session,
      emit(e) {
        events.push(e);
      },
      async saveSession(id) {
        saved = id;
      },
    }),
  );
  assert.equal(options?.resume, session);
  assert.equal(options?.pathToClaudeCodeExecutable, '/unused/claude');
  assert.equal(saved, session);
  assert.equal(closed, true);
  assert.equal(events[0].text, 'Answer');
  const incomplete = claudeAdapter(() => ({
    close() {},
    async *[Symbol.asyncIterator]() {},
  }));
  await assert.rejects(incomplete.run(context('/tmp')), /without completing/);
  const failed = claudeAdapter(() => ({
    close() {},
    async *[Symbol.asyncIterator]() {
      yield {
        type: 'result',
        subtype: 'error_during_execution',
        is_error: true,
        errors: ['provider rejected'],
        session_id: session,
      } as SDKMessage;
    },
  }));
  await assert.rejects(failed.run(context('/tmp')), /provider rejected/);
});

test('Claude tool policy rejects writes in Ask and symlink/outside/Git writes in Apply', async (t) => {
  const root = await mkdtemp(path.join(tmpdir(), 'rev-policy-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  await symlink(tmpdir(), path.join(root, 'outside'));
  await symlink(
    path.join(tmpdir(), `missing-${randomUUID()}`),
    path.join(root, 'dangling'),
  );
  const ask = claudeOptions(context(root));
  const apply = claudeOptions(context(root, { mode: 'apply' }));
  assert.deepEqual(ask.tools, ['Read', 'Glob', 'Grep']);
  assert.equal(ask.strictMcpConfig, true);
  assert.deepEqual(apply.mcpServers, {});
  const hook = async (options: Options, file: string) =>
    options.hooks!.PreToolUse![0].hooks[0](
      {
        hook_event_name: 'PreToolUse',
        tool_name: 'Write',
        tool_input: { file_path: file },
        session_id: 's',
        cwd: root,
        transcript_path: '',
        tool_use_id: 't',
      },
      't',
      { signal: new AbortController().signal },
    );
  for (const file of [
    '../escape',
    'outside/new-file',
    'dangling/new-file',
    '.git/config',
  ]) {
    assert.equal(
      ((await hook(apply, file)) as any).hookSpecificOutput.permissionDecision,
      'deny',
    );
  }
  assert.deepEqual(await hook(apply, 'src/new.ts'), {});
  assert.equal(
    ((await hook(ask, 'src/new.ts')) as any).hookSpecificOutput
      .permissionDecision,
    'deny',
  );
});

test('Claude commands always request review approval, including explicit Git operations', async () => {
  const signal = new AbortController().signal;
  let allowed = false;
  const requests: string[] = [];
  const options = claudeOptions(
    context('/tmp', {
      mode: 'apply',
      async approve(request) {
        requests.push(request.detail);
        return allowed;
      },
    }),
  );
  assert.ok((options.tools as string[]).includes('Bash'));
  const input = { command: 'git commit -am "Requested change"' };
  const hook = await options.hooks!.PreToolUse![0].hooks[0](
    {
      hook_event_name: 'PreToolUse',
      tool_name: 'Bash',
      tool_input: input,
      session_id: 's',
      cwd: '/tmp',
      transcript_path: '',
      tool_use_id: 't',
    },
    't',
    { signal },
  );
  assert.equal((hook as any).hookSpecificOutput.permissionDecision, 'ask');
  assert.equal(
    (
      await options.canUseTool!('Bash', input, {
        signal,
        toolUseID: 't',
        requestId: 't',
      })
    )?.behavior,
    'deny',
  );
  allowed = true;
  assert.equal(
    (
      await options.canUseTool!('Bash', input, {
        signal,
        toolUseID: 'u',
        requestId: 'u',
      })
    )?.behavior,
    'allow',
  );
  assert.equal(requests.length, 2);
  const ask = claudeOptions(context('/tmp'));
  assert.equal(
    (
      await ask.canUseTool!('Bash', input, {
        signal,
        toolUseID: 'v',
        requestId: 'v',
      })
    )?.behavior,
    'deny',
  );
});

test(
  'official Claude SDK talks to the configured local executable over its real protocol',
  { timeout: 15000 },
  async (t) => {
    const root = await mkdtemp(path.join(tmpdir(), 'rev-sdk-'));
    t.after(() => rm(root, { recursive: true, force: true }));
    const binary = path.join(root, 'claude');
    const session = randomUUID();
    await writeFile(
      binary,
      `#!${process.execPath}
const fs = require('node:fs');
fs.writeFileSync(${JSON.stringify(path.join(root, 'args.json'))}, JSON.stringify(process.argv));
const send = value => console.log(JSON.stringify(value));
require('node:readline').createInterface({ input: process.stdin }).on('line', line => {
 const m = JSON.parse(line);
 if (m.type === 'control_request') send({ type: 'control_response', response: { subtype: 'success', request_id: m.request_id, response: { commands: [], models: [], account: {} } } });
 if (m.type === 'user') {
  send({ type: 'assistant', uuid: 'a', session_id: ${JSON.stringify(session)}, parent_tool_use_id: null, message: { id: 'message', type: 'message', role: 'assistant', model: 'fixture', content: [{ type: 'text', text: 'SDK connected' }], stop_reason: 'end_turn', stop_sequence: null, usage: { input_tokens: 0, output_tokens: 0 } } });
  send({ type: 'result', subtype: 'success', is_error: false, result: 'SDK connected', session_id: ${JSON.stringify(session)}, uuid: 'b', duration_ms: 1, duration_api_ms: 0, num_turns: 1, total_cost_usd: 0, usage: {}, modelUsage: {}, permission_denials: [] });
 }
});`,
    );
    await chmod(binary, 0o755);
    const events: string[] = [];
    let saved = '';
    await claude.run(
      context(root, {
        executable: binary,
        emit(e) {
          events.push(e.text);
        },
        async saveSession(id) {
          saved = id;
        },
      }),
    );
    assert.equal(saved, session);
    assert.ok(events.includes('SDK connected'));
    const args: string[] = JSON.parse(
      await readFile(path.join(root, 'args.json'), 'utf8'),
    );
    assert.ok(args.includes('--strict-mcp-config'));
    assert.ok(
      args.includes('--tools=Read,Glob,Grep') ||
        (args.includes('--tools') && args.includes('Read,Glob,Grep')),
      JSON.stringify(args),
    );
  },
);
