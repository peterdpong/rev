import { randomUUID } from 'node:crypto';
import type {
  AgentRequest,
  AgentRun,
  Review,
  ReviewThread,
  Selection,
} from './types';
import { mutateState, readState } from './store';

const active = new Map<string, { flush(): Promise<void> }>();

export async function threadContext(
  review: Review,
  id?: string,
  kind: ReviewThread['kind'] = 'question',
) {
  if (!id) {
    return '';
  }
  const thread = (await readState(review, false)).threads?.find(
    (t) => t.id === id,
  );
  if (!thread || thread.archived || (kind && thread.kind !== kind)) {
    throw new Error('This conversation is unavailable for this action.');
  }
  if (
    thread.kind === 'question' &&
    (thread.headSha !== review.headSha || thread.mergeBase !== review.mergeBase)
  ) {
    throw new Error(
      'This thread belongs to an earlier revision. Start a new question for the current code.',
    );
  }
  return (
    '\nPrevious messages in this discussion:\n' +
    thread.turns
      .filter((turn) => turn.status !== 'queued' && turn.status !== 'running')
      .slice(-6)
      .map(
        (turn) =>
          `Human: ${turn.question.slice(0, 4000)}\nAgent: ${turn.events
            .filter((e) => e.kind === 'message')
            .map((e) => e.text)
            .join('\n')
            .slice(0, 8000)}`,
      )
      .join('\n\n') +
    '\n'
  );
}

export async function createRunHistory(
  review: Review,
  request: AgentRequest,
  run: AgentRun,
  commentIds: string[],
  newThread = false,
) {
  const state = await readState(review, false);
  const comments = state.comments.filter((c) => commentIds.includes(c.id));
  const selected = comments.length === 1 ? comments[0] : undefined;
  const selection: Selection | undefined =
    request.selection ||
    (selected
      ? {
          scope: selected.scope,
          path: selected.path,
          side: selected.side,
          start: selected.start,
          end: selected.end,
        }
      : undefined);
  const question =
    request.mode === 'ask' || (request.threadId && !newThread)
      ? request.question!.trim()
      : comments
          .map(
            (c) =>
              `${c.scope === 'review' ? 'Entire review' : c.scope === 'file' ? c.path : `${c.path}:${c.start}–${c.end}`}\n${c.body}`,
          )
          .join('\n\n');
  const id = request.threadId || randomUUID();
  if (request.threadId && !newThread) {
    await threadContext(
      review,
      request.threadId,
      request.mode === 'ask' ? 'question' : 'change',
    );
  }
  const initial: ReviewThread = {
    id,
    kind: request.mode === 'ask' ? 'question' : 'change',
    body: question,
    selection,
    digest: request.selectionDigest || selected?.digest,
    headSha: review.headSha,
    mergeBase: review.mergeBase,
    createdAt: run.startedAt,
    updatedAt: run.startedAt,
    commentIds,
    changes: structuredClone(comments),
    turns: [],
  };
  let last = '';
  let finalStatus: AgentRun['status'] | undefined;
  let writes = Promise.resolve();
  const flush = () => {
    const turn = {
      id: run.id,
      question,
      provider: request.provider,
      startedAt: run.startedAt,
      status: finalStatus || run.status,
      events: structuredClone(run.events),
      worktree: run.worktree,
    };
    const signature = JSON.stringify(turn);
    if (signature === last) {
      return writes.then(async () => {
        run.thread = (await readState(review, false)).threads?.find(
          (t) => t.id === id,
        );
      });
    }
    writes = writes
      .catch(() => {})
      .then(async () => {
        const saved = await mutateState(
          review,
          (state) => {
            state.threads ||= [];
            let thread = state.threads.find((t) => t.id === id);
            if (!thread) {
              thread = structuredClone(initial);
              state.threads.push(thread);
            }
            const index = thread.turns.findIndex((t) => t.id === run.id);
            if (index < 0) {
              thread.turns.push(turn);
            } else {
              thread.turns[index] = turn;
            }
            thread.updatedAt = new Date().toISOString();
          },
          false,
        );
        run.thread = saved.threads!.find((t) => t.id === id);
        last = signature;
      });
    return writes;
  };
  await flush();
  active.set(run.id, { flush });
  const timer = setInterval(() => {
    void flush().catch(() => {});
  }, 750);
  timer.unref();
  return {
    async finish(status?: AgentRun['status']) {
      clearInterval(timer);
      finalStatus = status;
      try {
        await flush();
      } catch (error: any) {
        run.events.push({
          kind: 'error',
          text: `Could not save the conversation: ${error.message}`,
        });
      } finally {
        active.delete(run.id);
      }
    },
  };
}

export async function flushRunHistory(id: string) {
  await active.get(id)?.flush();
}

export function hasPendingHistories() {
  return active.size > 0;
}
