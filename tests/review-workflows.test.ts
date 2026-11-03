import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { AgentRunner, preparePrompt } from '../core/agents';
import { inspect, git, filePatch } from '../core/git';
import { addComment, readState } from '../core/store';
import { flushRunHistory } from '../core/threads';
import { HostService } from '../core/host';
import { createDemo } from '../core/demo';
import { changeWorktree, lockWorktree } from '../core/agent-worktree';
import { writeFakeCodex } from './fake-codex';
import type { AgentRun } from '../core/types';
import type { ProviderTurn } from '../core/providers/types';

async function waitFor(check: () => boolean) {
  for (let i = 0; i < 400; i++) {
    if (check()) {
      return;
    }
    await new Promise((r) => setTimeout(r, 15));
  }
  assert.fail('Timed out');
}

const finished = (run: AgentRun) => !['running', 'queued'].includes(run.status);

async function fixture(t: any) {
  const root = await mkdtemp(path.join(tmpdir(), 'rev-workflows-'));
  const repo = await createDemo(path.join(root, 'repo'));
  const binary = path.join(root, 'codex');
  await writeFakeCodex(binary, "text('OK'); complete();");
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
  const review = {
    repo,
    base: 'main',
    head: (await git(repo, ['branch', '--show-current'])).trim(),
  };
  return {
    root,
    repo,
    review,
    request: {
      review,
      provider: 'codex' as const,
      mode: 'ask' as const,
      question: 'First question',
    },
  };
}

test('independent questions run concurrently; follow-ups queue, resume their own session, and include the previous answer', async (t) => {
  const f = await fixture(t);
  const starts: ProviderTurn[] = [];
  const release: (() => void)[] = [];
  const runner = new AgentRunner(
    () => ({
      id: 'codex',
      transport: 'app-server',
      async run(turn) {
        const index = starts.push(turn) - 1;
        await turn.saveSession(turn.sessionId || `session-${index}`);
        await new Promise<void>((resolve) => {
          release[index] = resolve;
        });
        turn.emit({ kind: 'message', text: `Answer ${index}` });
      },
    }),
    path.join(f.root, 'worktrees'),
  );
  t.after(() => {
    runner.stopAll();
    release.forEach((r) => r());
  });
  const first = await runner.start(f.request);
  await waitFor(() => starts.length === 1);
  const other = await runner.start({
    ...f.request,
    question: 'Independent question',
  });
  await waitFor(() => starts.length === 2);
  const next = await runner.start({
    ...f.request,
    threadId: first.thread!.id,
    question: 'Follow-up',
  });
  assert.equal(next.status, 'queued');
  assert.notEqual(first.thread!.id, other.thread!.id);
  release[0]();
  await waitFor(() => typeof release[2] === 'function');
  assert.equal(starts[2].sessionId, 'session-0');
  assert.match(starts[2].prompt, /Answer 0/);
  assert.doesNotMatch(starts[2].prompt, /Independent question/);
  assert.equal(starts[1].sessionId, undefined);
  release[1]();
  release[2]();
  await waitFor(() => [first, other, next].every(finished));
  await flushRunHistory(first.id);
  const threads = (await readState(await inspect(f.review))).threads!;
  assert.equal(threads.length, 2);
  assert.deepEqual(
    threads.find((t) => t.id === first.thread!.id)!.turns.map((t) => t.status),
    ['completed', 'completed'],
  );
});

test('cancelling a queued message does not start its successor before the running turn finishes', async (t) => {
  const f = await fixture(t);
  let entered = 0;
  let release!: () => void;
  const runner = new AgentRunner(() => ({
    id: 'codex',
    transport: 'app-server',
    async run() {
      if (++entered === 1) {
        await new Promise<void>((r) => {
          release = r;
        });
      }
    },
  }));
  const first = await runner.start(f.request);
  await waitFor(() => entered === 1);
  const cancelled = await runner.start({
    ...f.request,
    threadId: first.thread!.id,
    question: 'Cancel me',
  });
  const last = await runner.start({
    ...f.request,
    threadId: first.thread!.id,
    question: 'Last',
  });
  runner.cancel(cancelled.id);
  await waitFor(() => cancelled.status === 'cancelled');
  assert.equal(entered, 1);
  assert.equal(last.status, 'queued');
  release();
  await waitFor(() => finished(last));
  assert.equal(entered, 2);
  assert.equal(last.status, 'completed');
});

test('applies feedback in a retained worktree while the original checkout is on another branch with local edits', async (t) => {
  const f = await fixture(t);
  const review = await inspect(f.review);
  await addComment(review, undefined, 'Improve this PR.');
  await git(f.repo, ['switch', 'main']);
  await writeFile(
    path.join(f.repo, 'src/lib/retry.ts'),
    'my unrelated local work\n',
  );
  const before = await git(f.repo, ['status', '--porcelain']);
  const runner = new AgentRunner(
    () => ({
      id: 'codex',
      transport: 'app-server',
      async run(turn) {
        assert.notEqual(turn.cwd, f.repo);
        assert.match(turn.prompt, /Entire review/);
        await writeFile(path.join(turn.cwd, 'agent-result.txt'), 'implemented');
        turn.emit({ kind: 'message', text: 'Changes are ready.' });
      },
    }),
    path.join(f.root, 'worktrees'),
  );
  const run = await runner.start({ ...f.request, mode: 'apply' });
  await waitFor(() => finished(run));
  assert.equal(run.status, 'completed', JSON.stringify(run.events));
  assert.ok(run.worktree);
  assert.equal(
    await readFile(path.join(run.worktree.path, 'agent-result.txt'), 'utf8'),
    'implemented',
  );
  assert.equal(
    await readFile(path.join(f.repo, 'src/lib/retry.ts'), 'utf8'),
    'my unrelated local work\n',
  );
  assert.equal(await git(f.repo, ['status', '--porcelain']), before);
  assert.equal(
    (await git(f.repo, ['branch', '--show-current'])).trim(),
    'main',
  );
  assert.equal(
    (await git(run.worktree.path, ['rev-parse', 'HEAD'])).trim(),
    review.headSha,
  );
  await flushRunHistory(run.id);
  assert.deepEqual(
    (await readState(review)).threads![0].turns[0].worktree,
    run.worktree,
  );
  assert.deepEqual(
    await changeWorktree(review, path.join(f.root, 'worktrees')),
    run.worktree,
  );
  const unlock = await lockWorktree(run.worktree.path);
  await assert.rejects(lockWorktree(run.worktree.path), /another Rev process/);
  await unlock();
});

test('working review snapshots staged, unstaged, and untracked content without altering the original index', async (t) => {
  const f = await fixture(t);
  await writeFile(path.join(f.repo, 'staged.txt'), 'staged');
  await git(f.repo, ['add', 'staged.txt']);
  await writeFile(path.join(f.repo, 'src/lib/retry.ts'), 'unstaged');
  await writeFile(path.join(f.repo, 'untracked.txt'), 'untracked');
  const before = await git(f.repo, ['status', '--porcelain']);
  const worktree = await changeWorktree(
    await inspect({ ...f.review, mode: 'working' }),
    path.join(f.root, 'worktrees'),
  );
  for (const [name, text] of [
    ['staged.txt', 'staged'],
    ['src/lib/retry.ts', 'unstaged'],
    ['untracked.txt', 'untracked'],
  ]) {
    assert.equal(await readFile(path.join(worktree.path, name), 'utf8'), text);
  }
  assert.equal(await git(f.repo, ['status', '--porcelain']), before);
  await writeFile(path.join(f.repo, 'untracked.txt'), 'new local changes');
  const next = await changeWorktree(
    await inspect({ ...f.review, mode: 'working' }),
    path.join(f.root, 'worktrees'),
  );
  assert.notEqual(next.path, worktree.path);
  assert.equal(
    await readFile(path.join(next.path, 'untracked.txt'), 'utf8'),
    'new local changes',
  );
});

test('file and whole-review requests keep their scope; archived questions persist and can be restored', async (t) => {
  const f = await fixture(t);
  const review = await inspect(f.review);
  const selection = {
    scope: 'file' as const,
    path: 'src/lib/retry.ts',
    side: 'additions' as const,
    start: 0,
    end: 0,
  };
  const patch = await filePatch(review, selection.path);
  await addComment(review, selection, 'Simplify this file', patch.digest);
  await addComment(review, undefined, 'Review-wide request');
  const prepared = await preparePrompt({ ...f.request, mode: 'apply' });
  assert.match(prepared.prompt, /entire file/);
  assert.match(prepared.prompt, /Entire review/);
  assert.equal(prepared.ids.length, 2);
  const ask = await preparePrompt({
    ...f.request,
    selection,
    selectionDigest: patch.digest,
  });
  assert.match(ask.prompt, /entire file/);
  assert.doesNotMatch(ask.prompt, /lines 0/);
  const runner = new AgentRunner(() => ({
    id: 'codex',
    transport: 'app-server',
    async run() {},
  }));
  const run = await runner.start(f.request);
  await waitFor(() => finished(run));
  const host = new HostService(undefined, path.join(f.root, 'data'));
  t.after(() => host.close());
  await host.call('archiveThread', { review: f.review, id: run.thread!.id });
  assert.equal((await readState(review)).threads![0].archived, true);
  await host.call('archiveThread', {
    review: f.review,
    id: run.thread!.id,
    archived: false,
  });
  assert.equal((await readState(review)).threads![0].archived, false);
});

test('a whole-review change rejects working content changed after the request was queued', async (t) => {
  const f = await fixture(t);
  const request = { ...f.review, mode: 'working' as const };
  await writeFile(path.join(f.repo, 'local.txt'), 'before');
  await addComment(
    await inspect(request),
    undefined,
    'Review these local changes',
  );
  await writeFile(path.join(f.repo, 'local.txt'), 'after');
  await assert.rejects(
    preparePrompt({ ...f.request, review: request, mode: 'apply' }),
    /stale/,
  );
});

test('change follow-ups queue in the same conversation and retain edits, session, and original requests', async (t) => {
  const f = await fixture(t);
  await addComment(await inspect(f.review), undefined, 'Handle cancellation');
  const turns: ProviderTurn[] = [];
  const releases: (() => void)[] = [];
  const runner = new AgentRunner(
    () => ({
      id: 'codex',
      transport: 'app-server',
      async run(turn) {
        const index = turns.push(turn) - 1;
        await turn.saveSession(turn.sessionId || 'change-session');
        await writeFile(path.join(turn.cwd, `turn-${index}.txt`), 'agent edit');
        await new Promise<void>((resolve) => {
          releases[index] = resolve;
        });
        turn.emit({ kind: 'message', text: `Implemented turn ${index}` });
      },
    }),
    path.join(f.root, 'worktrees'),
  );
  t.after(() => {
    runner.stopAll();
    releases.forEach((release) => release());
  });
  const first = await runner.start({
    ...f.request,
    mode: 'apply',
    question: undefined,
  });
  await waitFor(() => !!releases[0]);
  const reply = await runner.start({
    ...f.request,
    mode: 'apply',
    threadId: first.thread!.id,
    question: 'Also cover timeout cancellation',
  });
  assert.equal(reply.status, 'queued');
  releases[0]();
  await waitFor(() => !!releases[1]);
  assert.equal(turns[1].cwd, turns[0].cwd);
  assert.equal(turns[1].sessionId, 'change-session');
  assert.match(turns[1].prompt, /Handle cancellation/);
  assert.match(turns[1].prompt, /Implemented turn 0/);
  assert.match(turns[1].prompt, /Also cover timeout cancellation/);
  assert.equal(
    await readFile(path.join(turns[1].cwd, 'turn-0.txt'), 'utf8'),
    'agent edit',
  );
  releases[1]();
  await waitFor(() => finished(reply));
  const state = await readState(await inspect(f.review));
  assert.equal(state.threads!.length, 1);
  assert.equal(state.threads![0].kind, 'change');
  assert.equal(state.threads![0].changes!.length, 1);
  assert.deepEqual(
    state.threads![0].turns.map((turn) => turn.status),
    ['completed', 'completed'],
  );
  assert.deepEqual(state.threads![0].turns[1].worktree, first.worktree);
  assert.equal(state.comments[0].status, 'sent');
  await assert.rejects(
    runner.start({
      ...f.request,
      mode: 'apply',
      threadId: first.thread!.id,
      question: ' ',
    }),
    /Enter a follow-up/,
  );
  await assert.rejects(
    runner.start({ ...f.request, threadId: first.thread!.id }),
    /unavailable/,
  );
});

test('change threads resume after restart, explicit commits and squashes, and original branch advancement', async (t) => {
  const f = await fixture(t);
  const original = await inspect(f.review);
  await addComment(original, undefined, 'Implement this request and commit it');
  const root = path.join(f.root, 'worktrees');
  const firstRunner = new AgentRunner(
    () => ({
      id: 'codex',
      transport: 'app-server',
      async run(turn) {
        await turn.saveSession('persisted-change-session');
        assert.doesNotMatch(
          turn.prompt,
          /Never commit|Do not commit or push|must remain uncommitted/,
        );
        assert.match(turn.prompt, /only when the human explicitly requests/);
        await writeFile(path.join(turn.cwd, 'agent.txt'), 'first edit');
        await git(turn.cwd, ['add', 'agent.txt']);
        await git(turn.cwd, ['commit', '-m', 'requested edit']);
        turn.emit({ kind: 'message', text: 'Committed the requested edit.' });
      },
    }),
    root,
  );
  const first = await firstRunner.start({
    ...f.request,
    mode: 'apply',
    question: undefined,
  });
  await waitFor(() => finished(first));
  assert.equal(first.status, 'completed');
  await git(f.repo, ['commit', '--allow-empty', '-m', 'updated review']);
  const sourceHead = await git(f.repo, ['rev-parse', 'HEAD']);
  const nextRunner = new AgentRunner(
    () => ({
      id: 'codex',
      transport: 'app-server',
      async run(turn) {
        assert.equal(turn.sessionId, 'persisted-change-session');
        assert.equal(turn.cwd, first.worktree!.path);
        assert.match(turn.prompt, /Committed the requested edit/);
        assert.match(turn.prompt, /Please squash/);
        await git(turn.cwd, ['reset', '--soft', original.mergeBase]);
        await git(turn.cwd, ['commit', '-m', 'squashed requested changes']);
        turn.emit({ kind: 'message', text: 'Squashed the requested changes.' });
      },
    }),
    root,
  );
  const next = await nextRunner.start({
    ...f.request,
    mode: 'apply',
    threadId: first.thread!.id,
    question: 'Please squash the changes',
  });
  await waitFor(() => finished(next));
  assert.equal(next.status, 'completed', JSON.stringify(next.events));
  const lastRunner = new AgentRunner(
    () => ({
      id: 'codex',
      transport: 'app-server',
      async run(turn) {
        assert.equal(turn.cwd, first.worktree!.path);
        assert.match(turn.prompt, /Squashed the requested changes/);
        assert.equal(
          await readFile(path.join(turn.cwd, 'agent.txt'), 'utf8'),
          'first edit',
        );
      },
    }),
    root,
  );
  const last = await lastRunner.start({
    ...f.request,
    mode: 'apply',
    threadId: first.thread!.id,
    question: 'Double-check the result',
  });
  await waitFor(() => finished(last));
  assert.equal(last.status, 'completed', JSON.stringify(last.events));
  assert.equal(await git(f.repo, ['rev-parse', 'HEAD']), sourceHead);
  const saved = (await readState(await inspect(f.review))).threads![0];
  assert.equal(saved.headSha, original.headSha);
  assert.equal(saved.turns.length, 3);
});

test('working change follow-ups keep their original worktree when local edits change', async (t) => {
  const f = await fixture(t);
  const review = { ...f.review, mode: 'working' as const };
  await writeFile(path.join(f.repo, 'local.txt'), 'original local edit');
  await addComment(await inspect(review), undefined, 'Improve these changes');
  const turns: ProviderTurn[] = [];
  const runner = new AgentRunner(
    () => ({
      id: 'codex',
      transport: 'app-server',
      async run(turn) {
        turns.push(turn);
      },
    }),
    path.join(f.root, 'worktrees'),
  );
  const first = await runner.start({ ...f.request, review, mode: 'apply' });
  await waitFor(() => finished(first));
  assert.equal(first.status, 'completed');
  await writeFile(path.join(f.repo, 'local.txt'), 'new user edit');
  const next = await runner.start({
    ...f.request,
    review,
    mode: 'apply',
    threadId: first.thread!.id,
    question: 'Continue',
  });
  await waitFor(() => finished(next));
  assert.equal(next.status, 'completed', JSON.stringify(next.events));
  assert.equal(turns[1].cwd, turns[0].cwd);
  assert.equal(
    await readFile(path.join(turns[1].cwd, 'local.txt'), 'utf8'),
    'original local edit',
  );
  assert.equal(
    await readFile(path.join(f.repo, 'local.txt'), 'utf8'),
    'new user edit',
  );
  await rm(first.worktree!.path, { recursive: true, force: true });
  const missing = await runner.start({
    ...f.request,
    review,
    mode: 'apply',
    threadId: first.thread!.id,
    question: 'Continue again',
  });
  await waitFor(() => finished(missing));
  assert.equal(missing.status, 'failed');
  assert.match(
    missing.events.map((event) => event.text).join('\n'),
    /no longer available/,
  );
  assert.equal(
    turns.length,
    2,
    'missing worktrees must not be silently recreated',
  );
});

test('retained worktrees from before the rename keep their branch and edits', async (t) => {
  const f = await fixture(t);
  const review = await inspect(f.review);
  const root = path.join(f.root, 'worktrees');
  const current = await changeWorktree(review, root);
  assert.match(current.branch, /^rev\/changes-/);
  const previous = {
    ...current,
    branch: current.branch.replace(/^rev\//, 'patch/'),
  };
  await git(current.path, ['branch', '-m', previous.branch]);
  await writeFile(
    path.join(current.path, 'retained.txt'),
    'existing agent edits',
  );
  assert.deepEqual(await changeWorktree(review, root, previous), previous);
  assert.equal(
    await readFile(path.join(current.path, 'retained.txt'), 'utf8'),
    'existing agent edits',
  );
});
