import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, mkdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { git, inspect, filePatch, importPR, hash } from '../src/core/git';
import { HostService } from '../src/core/host';
import { addComment, readState, mutateState } from '../src/core/store';
import { createRunHistory, flushRunHistory, threadContext, hasPendingHistories } from '../src/core/threads';
import type { AgentRequest, AgentRun } from '../src/shared';

async function fixture(t: any) {
  const repo = await mkdtemp(path.join(tmpdir(), 'patch-threads-'));
  t.after(() => rm(repo, { recursive: true, force: true }));
  await git(repo, ['init', '-b', 'main']);
  await git(repo, ['config', 'user.name', 'Review Test']);
  await git(repo, ['config', 'user.email', 'review@example.invalid']);
  await writeFile(path.join(repo, 'app.ts'), 'export const retries = 1;\n');
  await git(repo, ['add', '.']); await git(repo, ['commit', '-m', 'Initial']);
  await git(repo, ['switch', '-c', 'feature']);
  await writeFile(path.join(repo, 'app.ts'), 'export const retries = 3;\n');
  await git(repo, ['add', '.']); await git(repo, ['commit', '-m', 'Handle transient failures', '-m', 'Retry the request before giving up.\nPreserve the original error.']);
  return { repo, request: { repo, base: 'main', head: 'feature' } };
}

function run(id: string): AgentRun {
  return { id, status: 'running', events: [], approvals: [], mode: 'ask', startedAt: new Date().toISOString() } as AgentRun;
}

test('review metadata includes full commit context and cached PR details without switching branches', async t => {
  const f = await fixture(t);
  const review = await inspect(f.request);
  assert.equal(review.commitCount, 1);
  assert.equal(review.headCommit?.subject, 'Handle transient failures');
  assert.equal(review.headCommit?.body, 'Retry the request before giving up.\nPreserve the original error.');
  assert.equal(review.headCommit?.author, 'Review Test');
  const bin = path.join(f.repo, 'fake-bin'); await mkdir(bin);
  const details = { number: 42, title: 'Improve request reliability', body: '## Context\nRetry transient failures.', author: { login: 'reviewer' }, state: 'OPEN', isDraft: true, headRefName: 'feature', baseRefName: 'main', updatedAt: new Date().toISOString(), url: 'https://github.com/acme/patch/pull/42' };
  await writeFile(path.join(bin, 'gh'), `#!/bin/sh\ncat <<'PATCH_PR_JSON'\n${JSON.stringify(details)}\nPATCH_PR_JSON\n`, { mode: 0o755 });
  await git(f.repo, ['update-ref', 'refs/pull/42/head', 'HEAD']);
  await git(f.repo, ['config', `url.${pathToFileURL(f.repo).href}.insteadOf`, 'https://github.com/acme/patch.git']);
  const previousPath = process.env.PATH;
  process.env.PATH = `${bin}:${previousPath}`;
  let request;
  try { request = await importPR(f.repo, '42'); } finally { process.env.PATH = previousPath; }
  const imported = await inspect(request);
  assert.equal(imported.pullRequest?.title, details.title);
  assert.equal(imported.pullRequest?.body, details.body);
  assert.equal(imported.pullRequest?.author, 'reviewer');
  assert.equal(imported.pullRequest?.isDraft, true);
  assert.equal((await git(f.repo, ['branch', '--show-current'])).trim(), 'feature');
  assert.equal((await inspect(f.request)).pullRequest, undefined);
  // Tabs imported before metadata was supported can refresh their details too.
  await rm(path.join(f.repo, '.git', 'revue', `pr-${hash(request.head!).slice(0, 24)}.json`));
  assert.equal((await inspect(request)).pullRequest, undefined);
  const host = new HostService();
  process.env.PATH = `${bin}:${previousPath}`;
  try {
    const refreshed = await host.call('fetch', { repo: f.repo, review: { ...request, mode: 'working' } }) as { request: typeof request };
    assert.equal(refreshed.request.head, request.head);
    assert.equal(refreshed.request.mode, 'working');
    assert.equal((await inspect(request)).pullRequest?.title, details.title);
  } finally { process.env.PATH = previousPath; host.close(); }
});

test('questions, streaming responses, and follow-ups persist as one conversation', async t => {
  const f = await fixture(t), review = await inspect(f.request);
  const patch = await filePatch(review, 'app.ts');
  const request: AgentRequest = { review: f.request, provider: 'codex', mode: 'ask', question: 'Why retry three times?', selection: { path: 'app.ts', side: 'additions', start: 1, end: 1 }, selectionDigest: patch.digest };
  const first = run('first');
  const journal = await createRunHistory(review, request, first, []);
  t.after(() => journal.finish());
  assert.equal(hasPendingHistories(), true);
  first.events.push({ kind: 'message', text: 'It tolerates temporary failures.' });
  await flushRunHistory(first.id);
  let state = await readState(review);
  assert.equal(state.threads?.length, 1);
  assert.equal(state.threads![0].turns[0].events[0].text, first.events[0].text);
  first.status = 'completed'; await journal.finish();
  assert.equal(hasPendingHistories(), false);
  const id = first.thread!.id;
  assert.match(await threadContext(review, id), /Human: Why retry three times\?[\s\S]*Agent: It tolerates temporary failures/);
  const second = run('second');
  const followup = await createRunHistory(review, { ...request, question: 'Is it bounded?', threadId: id }, second, []);
  t.after(() => followup.finish());
  second.events.push({ kind: 'message', text: 'Yes, three attempts maximum.' }); second.status = 'completed';
  await followup.finish();
  state = await readState(await inspect(f.request));
  assert.equal(state.threads?.length, 1);
  assert.equal(state.threads![0].turns.length, 2);
  assert.equal(state.threads![0].turns[1].status, 'completed');
  assert.equal(state.threads![0].digest, patch.digest);
  await git(f.repo, ['commit', '--allow-empty', '-m', 'New revision']);
  await assert.rejects(threadContext(await inspect(f.request), id), /earlier revision/);
  await assert.rejects(threadContext(review, 'missing'), /unavailable/);
});

test('change runs retain the requested feedback alongside the agent response', async t => {
  const f = await fixture(t), review = await inspect(f.request);
  const patch = await filePatch(review, 'app.ts');
  const state = await addComment(review, { path: 'app.ts', side: 'additions', start: 1, end: 1 }, 'Make this configurable.', patch.digest);
  const comment = state.comments[0], change = run('change'); change.mode = 'apply';
  const history = await createRunHistory(review, { review: f.request, provider: 'claude', mode: 'apply', commentIds: [comment.id] }, change, [comment.id]);
  t.after(() => history.finish());
  change.events.push({ kind: 'message', text: 'Added a retries option.' }); change.status = 'completed'; await history.finish();
  const saved = (await readState(review)).threads![0];
  assert.equal(saved.kind, 'change');
  assert.deepEqual(saved.commentIds, [comment.id]);
  assert.equal(saved.selection?.path, 'app.ts');
  assert.match(saved.body, /Make this configurable/);
  assert.equal(saved.turns[0].events[0].text, 'Added a retries option.');
  assert.deepEqual(saved.changes, [comment]);
  await mutateState(review, state => { state.comments = []; });
  const retained = (await readState(review)).threads![0];
  assert.equal(retained.changes?.[0].body, 'Make this configurable.');
  assert.equal(retained.changes?.[0].digest, patch.digest);
  assert.equal(retained.changes?.[0].start, 1);
  await assert.rejects(threadContext(review, saved.id), /unavailable/);
});
