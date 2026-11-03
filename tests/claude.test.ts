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
import { claude, claudeAdapter, claudeOptions } from '../core/providers/claude';
import type { ProviderTurn } from '../core/providers/types';

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
