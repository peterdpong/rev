import type { ProviderAdapter, ProviderTurn } from './types';
import { StdioRpc, type RpcMessage } from './rpc';

export const codex: ProviderAdapter = {
  id: 'codex',
  transport: 'app-server',
  run: runCodex,
};

async function runCodex(turn: ProviderTurn) {
  turn.signal.throwIfAborted();
  const rpc = new StdioRpc(turn.executable, ['app-server'], turn.cwd);
  let threadId: string | undefined;
  let turnId: string | undefined;
  let starting = false;
  const early: RpcMessage[] = [];
  let resolveDone!: () => void;
  let rejectDone!: (error: Error) => void;
  const done = new Promise<void>((resolve, reject) => {
    resolveDone = resolve;
    rejectDone = reject;
  });
  // A child can fail while we are still awaiting initialization.
  void done.catch(() => {});
  const texts = new Map<string, string>();
  rpc.onFailure = rejectDone;
  const reply = (message: RpcMessage, result: unknown) => {
    try {
      rpc.send({ id: message.id, result });
    } catch {
      /* cancellation/exit owns the failure */
    }
  };
  rpc.onMessage = (message) => {
    const p = message.params || {};
    if (starting && !turnId) {
      if (early.length > 1000) {
        throw new Error('Too many Codex events before turn startup.');
      }
      early.push(message);
      return;
    }
    if (message.id !== undefined) {
      const supported = [
        'item/commandExecution/requestApproval',
        'item/fileChange/requestApproval',
      ].includes(message.method!);
      if (!supported) {
        // Unknown interactions fail closed instead of hanging the provider.
        rpc.send({
          id: message.id,
          error: {
            code: -32601,
            message: 'Rev does not support this interaction.',
          },
        });
        turn.emit({
          kind: 'activity',
          text: `Unsupported agent interaction: ${message.method}`,
        });
      } else if (
        turn.mode !== 'apply' ||
        p.threadId !== threadId ||
        (turnId && p.turnId !== turnId)
      ) {
        reply(message, { decision: 'decline' });
      } else {
        void turn
          .approve({
            title:
              message.method === 'item/fileChange/requestApproval'
                ? 'Allow file changes?'
                : 'Allow command?',
            detail:
              [
                p.command,
                p.reason,
                p.cwd,
                p.grantRoot,
                p.networkApprovalContext &&
                  JSON.stringify(p.networkApprovalContext),
              ]
                .filter(Boolean)
                .join('\n') || JSON.stringify(p),
          })
          .then(
            (allow) =>
              reply(message, { decision: allow ? 'accept' : 'decline' }),
            () => reply(message, { decision: 'decline' }),
          );
      }
      return;
    }
    const eventTurn = p.turn?.id || p.turnId;
    if (
      !turnId ||
      !threadId ||
      p.threadId !== threadId ||
      (eventTurn && eventTurn !== turnId)
    ) {
      return;
    }
    if (message.method === 'item/agentMessage/delta') {
      const text = ((texts.get(p.itemId) || '') + p.delta).slice(0, 50_000);
      texts.set(p.itemId, text);
      turn.emit({ kind: 'message', id: p.itemId, text });
    }
    if (
      message.method === 'item/completed' &&
      p.item?.type === 'agentMessage'
    ) {
      turn.emit({ kind: 'message', id: p.item.id, text: p.item.text });
    }
    if (
      message.method === 'item/started' &&
      p.item?.type === 'commandExecution'
    ) {
      turn.emit({ kind: 'activity', text: p.item.command });
    }
    if (message.method === 'item/completed' && p.item?.type === 'fileChange') {
      turn.emit({
        kind: 'activity',
        text: `Updated ${(p.item.changes || []).map((c: any) => c.path).join(', ')}`,
      });
    }
    if (message.method === 'error') {
      turn.emit({
        kind: p.willRetry ? 'activity' : 'error',
        text: p.error?.message || 'Codex failed.',
      });
    }
    if (message.method === 'turn/completed') {
      if (p.turn?.status === 'completed') {
        resolveDone();
      } else {
        rejectDone(
          new Error(
            p.turn?.error?.message ||
              `Codex turn ${p.turn?.status || 'did not complete'}.`,
          ),
        );
      }
    }
  };
  const abort = () => {
    rejectDone(new Error('Agent cancelled.'));
    void rpc.close();
  };
  turn.signal.addEventListener('abort', abort, { once: true });
  try {
    await rpc.request('initialize', {
      clientInfo: { name: 'rev', title: 'Rev', version: '0.1.0' },
    });
    rpc.send({ method: 'initialized', params: {} });
    const response = await rpc.request(
      turn.sessionId ? 'thread/resume' : 'thread/start',
      {
        ...(turn.sessionId ? { threadId: turn.sessionId } : {}),
        cwd: turn.cwd,
        approvalPolicy: turn.mode === 'ask' ? 'never' : 'on-request',
        approvalsReviewer: 'user',
        sandbox: turn.mode === 'ask' ? 'read-only' : 'workspace-write',
      },
    );
    threadId = response?.thread?.id;
    if (!threadId) {
      throw new Error('Codex did not return a conversation ID.');
    }
    await turn.saveSession(threadId);
    turn.signal.throwIfAborted();
    starting = true;
    const started = await rpc.request('turn/start', {
      threadId,
      input: [{ type: 'text', text: turn.prompt }],
    });
    turnId = started?.turn?.id;
    if (!turnId) {
      throw new Error('Codex did not return a turn ID.');
    }
    for (const message of early) {
      rpc.onMessage(message);
    }
    await done;
  } finally {
    turn.signal.removeEventListener('abort', abort);
    await rpc.close();
  }
}
