import {
  query,
  type Options,
  type Query,
  type SDKUserMessage,
} from '@anthropic-ai/claude-agent-sdk';
import { realpath, lstat } from 'node:fs/promises';
import path from 'node:path';
import type { ProviderAdapter, ProviderTurn } from './types';
import { processLifetime, spawnProvider } from './process';

async function writablePath(cwd: string, file: unknown) {
  if (typeof file !== 'string' || !file) {
    return false;
  }
  const root = await realpath(cwd);
  let current = path.resolve(cwd, file);
  // Resolve the existing ancestor too, so a new file beneath a symlink cannot
  // escape the checkout. Refuse .git even when it is an in-checkout directory.
  const relative = path.relative(cwd, current);
  if (relative.split(path.sep).includes('.git')) {
    return false;
  }
  for (;;) {
    try {
      const actual = await realpath(current);
      const rel = path.relative(root, actual);
      return (
        rel !== '..' &&
        !rel.startsWith('..' + path.sep) &&
        !path.isAbsolute(rel) &&
        !rel.split(path.sep).includes('.git')
      );
    } catch (error: any) {
      if (error.code !== 'ENOENT' || path.dirname(current) === current) {
        return false;
      }
      try {
        if ((await lstat(current)).isSymbolicLink()) {
          return false;
        }
      } catch (missing: any) {
        if (missing.code !== 'ENOENT') {
          return false;
        }
      }
      current = path.dirname(current);
    }
  }
}

export function claudeOptions(turn: ProviderTurn): Options {
  const tools =
    turn.mode === 'ask'
      ? ['Read', 'Glob', 'Grep']
      : ['Read', 'Glob', 'Grep', 'Edit', 'Write'];
  return {
    cwd: turn.cwd,
    pathToClaudeCodeExecutable: turn.executable,
    systemPrompt: { type: 'preset', preset: 'claude_code' },
    ...(turn.sessionId ? { resume: turn.sessionId } : {}),
    persistSession: true,
    includePartialMessages: true,
    permissionMode: turn.mode === 'ask' ? 'default' : 'acceptEdits',
    tools,
    settingSources: ['user'],
    mcpServers: {},
    strictMcpConfig: true,
    // Enforce the tool boundary even when user settings contain allow rules.
    hooks: {
      PreToolUse: [
        {
          hooks: [
            async (input) => {
              if (input.hook_event_name !== 'PreToolUse') {
                return {};
              }
              const writing =
                input.tool_name === 'Edit' || input.tool_name === 'Write';
              const allowed =
                tools.includes(input.tool_name) &&
                (!writing ||
                  (await writablePath(
                    turn.cwd,
                    (input.tool_input as any)?.file_path,
                  )));
              return allowed
                ? {}
                : {
                    hookSpecificOutput: {
                      hookEventName: 'PreToolUse',
                      permissionDecision: 'deny',
                      permissionDecisionReason:
                        'Rev only allows review tools and edits inside this checkout, excluding Git metadata.',
                    },
                  };
            },
          ],
        },
      ],
    },
    canUseTool: async (name, input, options) => {
      // Ask may never grant expanded permissions. Apply can approve a request
      // only within the existing restricted tool surface.
      if (turn.mode === 'ask' || !tools.includes(name)) {
        return {
          behavior: 'deny',
          message: 'Not permitted in this Rev review.',
        };
      }
      if (
        (name === 'Edit' || name === 'Write') &&
        !(await writablePath(turn.cwd, input.file_path))
      ) {
        return {
          behavior: 'deny',
          message: 'Edits must stay inside the checkout.',
        };
      }
      const allow = await turn.approve(
        { title: `Allow ${name}?`, detail: JSON.stringify(input, null, 2) },
        options.signal,
      );
      return allow
        ? { behavior: 'allow', updatedInput: input }
        : { behavior: 'deny', message: 'The reviewer declined this action.' };
    },
  };
}

type QueryFactory = (
  args: Parameters<typeof query>[0],
) => Pick<Query, typeof Symbol.asyncIterator | 'close'>;

export function claudeAdapter(
  makeQuery: QueryFactory = query,
): ProviderAdapter {
  return {
    id: 'claude',
    transport: 'agent-sdk',
    async run(turn) {
      turn.signal.throwIfAborted();
      const controller = new AbortController();
      let releaseInput!: () => void;
      const inputDone = new Promise<void>((resolve) => {
        releaseInput = resolve;
      });
      async function* input(): AsyncGenerator<SDKUserMessage> {
        yield {
          type: 'user',
          session_id: turn.sessionId || '',
          parent_tool_use_id: null,
          message: { role: 'user', content: turn.prompt },
        };
        await inputDone;
      }
      let life: ReturnType<typeof processLifetime> | undefined;
      let stream: ReturnType<QueryFactory> | undefined;
      const abort = () => {
        controller.abort();
        releaseInput();
        stream?.close();
        void life?.stop();
      };
      turn.signal.addEventListener('abort', abort, { once: true });
      let stderr = '';
      let sessionId: string | undefined;
      let messageId = 'response';
      const texts = new Map<string, string>();
      try {
        stream = makeQuery({
          prompt: input(),
          options: {
            ...claudeOptions(turn),
            abortController: controller,
            spawnClaudeCodeProcess: (options) => {
              const child = spawnProvider(
                options.command,
                options.args,
                options.cwd || turn.cwd,
                options.env,
              );
              life = processLifetime(child);
              child.stderr.on('data', (data) => {
                stderr = (stderr + data).slice(-8000);
              });
              return child;
            },
          },
        });
        for await (const message of stream) {
          turn.signal.throwIfAborted();
          if (
            'session_id' in message &&
            message.session_id &&
            message.session_id !== sessionId
          ) {
            sessionId = message.session_id;
            await turn.saveSession(sessionId);
          }
          if (message.type === 'stream_event') {
            const e = message.event;
            if (e.type === 'message_start') {
              messageId = e.message.id;
            }
            if (
              e.type === 'content_block_delta' &&
              e.delta.type === 'text_delta'
            ) {
              const id = `${messageId}:${e.index}`;
              const text = ((texts.get(id) || '') + e.delta.text).slice(
                0,
                50_000,
              );
              texts.set(id, text);
              turn.emit({ kind: 'message', id, text });
            }
          }
          if (message.type === 'assistant') {
            message.message.content.forEach((block, index) => {
              if (block.type === 'text') {
                turn.emit({
                  kind: 'message',
                  id: `${message.message.id}:${index}`,
                  text: block.text,
                });
              }
              if (block.type === 'tool_use') {
                turn.emit({
                  kind: 'activity',
                  text: `${block.name}${(block.input as any)?.file_path ? ` · ${(block.input as any).file_path}` : ''}`,
                });
              }
            });
          }
          if (message.type === 'result') {
            if (message.is_error || message.subtype !== 'success') {
              throw new Error(
                'errors' in message
                  ? message.errors.join('\n')
                  : 'Claude did not complete the turn.',
              );
            }
            if (!sessionId) {
              throw new Error('Claude did not return a conversation ID.');
            }
            return;
          }
        }
        throw new Error(
          `Claude exited without completing the turn.${stderr ? `\n${stderr}` : ''}`,
        );
      } finally {
        turn.signal.removeEventListener('abort', abort);
        releaseInput();
        stream?.close();
        await life?.stop();
      }
    },
  };
}

export const claude = claudeAdapter();
