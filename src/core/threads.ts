import { randomUUID } from 'node:crypto';
import type { AgentRequest, AgentRun, Review, ReviewThread, Selection } from '../shared';
import { mutateState, readState } from './store';

const active = new Map<string, { flush(): Promise<void> }>();

export async function threadContext(review: Review, id?: string) {
  if (!id) return '';
  const thread = (await readState(review, false)).threads?.find(t => t.id === id);
  if (!thread || thread.kind !== 'question') throw new Error('This question thread is unavailable.');
  if (thread.headSha !== review.headSha || thread.mergeBase !== review.mergeBase) throw new Error('This thread belongs to an earlier revision. Start a new question for the current code.');
  return '\nPrevious messages in this discussion:\n' + thread.turns.slice(-6).map(turn =>
    `Human: ${turn.question.slice(0, 4000)}\nAgent: ${turn.events.filter(e => e.kind === 'message').map(e => e.text).join('\n').slice(0, 8000)}`
  ).join('\n\n') + '\n';
}

export async function createRunHistory(review: Review, request: AgentRequest, run: AgentRun, commentIds: string[]) {
  if (request.threadId && request.mode !== 'ask') throw new Error('Only questions can continue an existing thread.');
  const state = await readState(review, false);
  const comments = state.comments.filter(c => commentIds.includes(c.id));
  const selected = comments.length === 1 ? comments[0] : undefined;
  const selection: Selection | undefined = request.selection || (selected ? { path: selected.path, side: selected.side, start: selected.start, end: selected.end } : undefined);
  const question = request.mode === 'ask' ? request.question!.trim() : comments.map(c => `${c.path}:${c.start}–${c.end}\n${c.body}`).join('\n\n');
  const id = request.threadId || randomUUID();
  if (request.threadId) await threadContext(review, request.threadId);
  const initial: ReviewThread = {
    id, kind: request.mode === 'ask' ? 'question' : 'change', body: question,
    selection, digest: request.selectionDigest || selected?.digest,
    headSha: review.headSha, mergeBase: review.mergeBase, createdAt: run.startedAt, updatedAt: run.startedAt,
    commentIds, turns: [],
  };
  let last = '';
  let writes = Promise.resolve();
  const flush = () => {
    const turn = { id: run.id, question, provider: request.provider, startedAt: run.startedAt, status: run.status, events: structuredClone(run.events) };
    const signature = JSON.stringify(turn);
    if (signature === last) return writes;
    writes = writes.catch(() => {}).then(async () => {
      const saved = await mutateState(review, state => {
        state.threads ||= [];
        let thread = state.threads.find(t => t.id === id);
        if (!thread) { thread = structuredClone(initial); state.threads.push(thread); }
        const index = thread.turns.findIndex(t => t.id === run.id);
        if (index < 0) thread.turns.push(turn); else thread.turns[index] = turn;
        thread.updatedAt = new Date().toISOString();
      }, false);
      run.thread = saved.threads!.find(t => t.id === id);
      last = signature;
    });
    return writes;
  };
  await flush();
  active.set(run.id, { flush });
  const timer = setInterval(() => { void flush().catch(() => {}); }, 750);
  timer.unref();
  return {
    async finish() {
      clearInterval(timer);
      try { await flush(); }
      catch (error: any) { run.events.push({ kind: 'error', text: `Could not save the conversation: ${error.message}` }); }
      finally { active.delete(run.id); }
    },
  };
}

export async function flushRunHistory(id: string) { await active.get(id)?.flush(); }
export function hasPendingHistories() { return active.size > 0; }
