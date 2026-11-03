import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, mkdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { git, inspect, filePatch, importPR, hash } from '../core/git';
import { addComment, readState, mutateState } from '../core/store';
import {
  createRunHistory,
  flushRunHistory,
  threadContext,
  hasPendingHistories,
} from '../core/threads';
import type { AgentRequest, AgentRun } from '../core/types';

async function fixture(t: any) {
  const repo = await mkdtemp(path.join(tmpdir(), 'rev-threads-'));
  t.after(() => rm(repo, { recursive: true, force: true }));
  await git(repo, ['init', '-b', 'main']);
  await git(repo, ['config', 'user.name', 'Review Test']);
  await git(repo, ['config', 'user.email', 'review@example.invalid']);
  await writeFile(path.join(repo, 'app.ts'), 'export const retries = 1;\n');
  await git(repo, ['add', '.']);
  await git(repo, ['commit', '-m', 'Initial']);
  await git(repo, ['switch', '-c', 'feature']);
  await writeFile(path.join(repo, 'app.ts'), 'export const retries = 3;\n');
  await git(repo, ['add', '.']);
  await git(repo, [
    'commit',
    '-m',
    'Handle transient failures',
    '-m',
    'Retry the request before giving up.\nPreserve the original error.',
  ]);
  return { repo, request: { repo, base: 'main', head: 'feature' } };
}

function run(id: string): AgentRun {
  return {
    id,
    status: 'running',
    events: [],
    approvals: [],
    mode: 'ask',
    startedAt: new Date().toISOString(),
  } as AgentRun;
}

test('questions, streaming responses, and follow-ups persist as one conversation', async (t) => {
  const f = await fixture(t),
    review = await inspect(f.request);
  const patch = await filePatch(review, 'app.ts');
  const request: AgentRequest = {
    review: f.request,
    provider: 'codex',
    mode: 'ask',
    question: 'Why retry three times?',
    selection: { path: 'app.ts', side: 'additions', start: 1, end: 1 },
    selectionDigest: patch.digest,
  };
  const first = run('first');
  const journal = await createRunHistory(review, request, first, []);
  t.after(() => journal.finish());
  assert.equal(hasPendingHistories(), true);
  first.events.push({
    kind: 'message',
    text: 'It tolerates temporary failures.',
  });
  await flushRunHistory(first.id);
  let state = await readState(review);
  assert.equal(state.threads?.length, 1);
  assert.equal(state.threads![0].turns[0].events[0].text, first.events[0].text);
  first.status = 'completed';
  await journal.finish();
  assert.equal(hasPendingHistories(), false);
  const id = first.thread!.id;
  assert.match(
    await threadContext(review, id),
    /Human: Why retry three times\?[\s\S]*Agent: It tolerates temporary failures/,
  );
  const second = run('second');
  const followup = await createRunHistory(
    review,
    { ...request, question: 'Is it bounded?', threadId: id },
    second,
    [],
  );
  t.after(() => followup.finish());
  second.events.push({ kind: 'message', text: 'Yes, three attempts maximum.' });
  second.status = 'completed';
  await followup.finish();
  state = await readState(await inspect(f.request));
  assert.equal(state.threads?.length, 1);
  assert.equal(state.threads![0].turns.length, 2);
  assert.equal(state.threads![0].turns[1].status, 'completed');
  assert.equal(state.threads![0].digest, patch.digest);
  await git(f.repo, ['commit', '--allow-empty', '-m', 'New revision']);
  await assert.rejects(
    threadContext(await inspect(f.request), id),
    /earlier revision/,
  );
  await assert.rejects(threadContext(review, 'missing'), /unavailable/);
});

test('change runs retain the requested feedback alongside the agent response', async (t) => {
  const f = await fixture(t),
    review = await inspect(f.request);
  const patch = await filePatch(review, 'app.ts');
  const state = await addComment(
    review,
    { path: 'app.ts', side: 'additions', start: 1, end: 1 },
    'Make this configurable.',
    patch.digest,
  );
  const comment = state.comments[0],
    change = run('change');
  change.mode = 'apply';
  const history = await createRunHistory(
    review,
    {
      review: f.request,
      provider: 'claude',
      mode: 'apply',
      commentIds: [comment.id],
    },
    change,
    [comment.id],
  );
  t.after(() => history.finish());
  change.events.push({ kind: 'message', text: 'Added a retries option.' });
  change.status = 'completed';
  await history.finish();
  const saved = (await readState(review)).threads![0];
  assert.equal(saved.kind, 'change');
  assert.deepEqual(saved.commentIds, [comment.id]);
  assert.equal(saved.selection?.path, 'app.ts');
  assert.match(saved.body, /Make this configurable/);
  assert.equal(saved.turns[0].events[0].text, 'Added a retries option.');
  assert.deepEqual(saved.changes, [comment]);
  await mutateState(review, (state) => {
    state.comments = [];
  });
  const retained = (await readState(review)).threads![0];
  assert.equal(retained.changes?.[0].body, 'Make this configurable.');
  assert.equal(retained.changes?.[0].digest, patch.digest);
  assert.equal(retained.changes?.[0].start, 1);
  await assert.rejects(threadContext(review, saved.id), /unavailable/);
});
