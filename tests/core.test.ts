import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  mkdtemp,
  writeFile,
  readFile,
  rm,
  mkdir,
  chmod,
  symlink,
} from 'node:fs/promises';
import path from 'node:path';
import { tmpdir } from 'node:os';
import { git, inspect, filePatch, resolveRef } from '../core/git';
import { readState, addComment, mutateState } from '../core/store';
import { AgentRunner, preparePrompt } from '../core/agents';
import { flushRunHistory } from '../core/threads';
import { writeFakeCodex } from './fake-codex';
import { startServer } from '../core/server';
import type { AgentRun, ReviewRequest } from '../core/types';

async function fixture(t: any) {
  const repo = await mkdtemp(path.join(tmpdir(), 'revue-test-'));
  t.after(() => rm(repo, { recursive: true, force: true }));
  await git(repo, ['init', '-b', 'main']);
  await git(repo, ['config', 'user.name', 'Test']);
  await git(repo, ['config', 'user.email', 'test@example.invalid']);
  await writeFile(path.join(repo, 'app.ts'), 'export const answer = 1;\n');
  await git(repo, ['add', '.']);
  await git(repo, ['commit', '-m', 'base']);
  await git(repo, ['switch', '-c', 'feature']);
  await writeFile(path.join(repo, 'app.ts'), 'export const answer = 2;\n');
  await git(repo, ['commit', '-am', 'change']);
  return { repo, base: 'main', head: 'HEAD' } satisfies ReviewRequest;
}

const selection = {
  path: 'app.ts',
  side: 'additions' as const,
  start: 1,
  end: 1,
};

test('uses the merge base, excluding changes only on the base branch', async (t) => {
  const r = await fixture(t);
  await git(r.repo, ['switch', 'main']);
  await writeFile(path.join(r.repo, 'base-only.ts'), 'base branch change\n');
  await git(r.repo, ['add', '.']);
  await git(r.repo, ['commit', '-m', 'base moves']);
  const review = await inspect({ ...r, head: 'feature' });
  assert.deepEqual(
    review.files.map((f) => f.path),
    ['app.ts'],
  );
  assert.notEqual(review.mergeBase, review.baseSha);
  assert.match(
    (await filePatch(review, 'app.ts')).patch,
    /\+export const answer = 2/,
  );
  assert.equal(
    (await git(r.repo, ['branch', '--show-current'])).trim(),
    'main',
  );
});

test('reports renamed, binary, deleted files and preserves literal filenames', async (t) => {
  const r = await fixture(t);
  await writeFile(path.join(r.repo, 'file[1].ts'), 'literal\n');
  await writeFile(path.join(r.repo, 'file1.ts'), 'different\n');
  await writeFile(path.join(r.repo, 'image.bin'), Buffer.from([0, 1, 2, 0]));
  await git(r.repo, ['mv', 'app.ts', 'renamed file.ts']);
  await git(r.repo, ['add', '.']);
  await git(r.repo, ['commit', '-m', 'rename and binary']);
  const review = await inspect({ ...r, base: 'HEAD~1' });
  assert.equal(
    review.files.find((f) => f.path === 'renamed file.ts')?.oldPath,
    'app.ts',
  );
  assert.equal(review.files.find((f) => f.path === 'image.bin')?.binary, true);
  const patch = (await filePatch(review, 'file[1].ts')).patch;
  assert.match(patch, /literal/);
  assert.doesNotMatch(patch, /different/);
  await git(r.repo, ['rm', 'renamed file.ts']);
  await git(r.repo, ['commit', '-m', 'delete']);
  assert.equal(
    (await inspect({ ...r, base: 'HEAD~1' })).files.find(
      (f) => f.path === 'renamed file.ts',
    )?.status,
    'D',
  );
});

test('working tree includes staged, unstaged and untracked changes', async (t) => {
  const r = await fixture(t);
  await writeFile(path.join(r.repo, 'staged.ts'), 'staged\n');
  await git(r.repo, ['add', 'staged.ts']);
  await writeFile(path.join(r.repo, 'app.ts'), 'export const answer = 3;\n');
  await writeFile(path.join(r.repo, 'new.ts'), 'new file');
  const review = await inspect({ ...r, mode: 'working' });
  assert.deepEqual(review.files.map((f) => f.path).sort(), [
    'app.ts',
    'new.ts',
    'staged.ts',
  ]);
  assert.match(
    (await filePatch(review, 'app.ts')).patch,
    /\+export const answer = 3/,
  );
  assert.match(
    (await filePatch(review, 'new.ts')).patch,
    /No newline at end of file/,
  );
  assert.equal(review.dirty, true);
});

test('untracked symlinks expose only the link target and arbitrary files are rejected', async (t) => {
  const r = await fixture(t);
  await symlink('/etc/hosts', path.join(r.repo, 'external'));
  const review = await inspect({ ...r, mode: 'working' });
  const link = await filePatch(review, 'external');
  assert.match(link.patch, /new file mode 120000/);
  assert.match(link.patch, /\+\/etc\/hosts/);
  assert.doesNotMatch(link.patch, /localhost/);
  await assert.rejects(
    filePatch(review, '../outside'),
    /not in this comparison/,
  );
  await assert.rejects(resolveRef(r.repo, '--help'), /./);
});

test('feedback persists, concurrent mutations do not lose comments, and stale feedback is rejected', async (t) => {
  const r = await fixture(t);
  const review = await inspect(r);
  await Promise.all(
    Array.from({ length: 6 }, (_, i) =>
      addComment(review, selection, `Request ${i}`),
    ),
  );
  assert.equal((await readState(review)).comments.length, 6);
  const prompt = await preparePrompt({
    review: r,
    provider: 'codex',
    mode: 'apply',
  });
  assert.equal(prompt.ids.length, 6);
  assert.match(prompt.prompt, /Request 5/);
  await writeFile(path.join(r.repo, 'app.ts'), 'export const answer = 99;\n');
  await git(r.repo, ['commit', '-am', 'changed meanwhile']);
  await assert.rejects(
    preparePrompt({ review: r, provider: 'codex', mode: 'apply' }),
    /stale/,
  );
});

test('prepares branch edits despite local changes or a different checked-out branch', async (t) => {
  const r = await fixture(t);
  await addComment(await inspect(r), selection, 'Change this.');
  await writeFile(path.join(r.repo, 'app.ts'), 'dirty\n');
  assert.match(
    (await preparePrompt({ review: r, provider: 'codex', mode: 'apply' }))
      .prompt,
    /Change this/,
  );
  await git(r.repo, ['restore', 'app.ts']);
  await git(r.repo, ['switch', 'main']);
  const other = { ...r, head: 'feature' };
  assert.match(
    (await preparePrompt({ review: other, provider: 'codex', mode: 'apply' }))
      .prompt,
    /Change this/,
  );
});

test('rejects stale selection while queuing a working-tree comment', async (t) => {
  const r = { ...(await fixture(t)), mode: 'working' as const };
  const review = await inspect(r);
  const old = await filePatch(review, 'app.ts');
  await writeFile(path.join(r.repo, 'app.ts'), 'changed\n');
  await assert.rejects(
    addComment(await inspect(r), selection, 'Fix', old.digest),
    /changed since you selected/,
  );
});

test('viewed status expires when its code changes and invalid line ranges are rejected', async (t) => {
  const r = { ...(await fixture(t)), mode: 'working' as const };
  const review = await inspect(r);
  const patch = await filePatch(review, 'app.ts');
  await mutateState(review, (state) => {
    state.viewed['app.ts'] = patch.digest;
  });
  assert.equal(Object.keys((await readState(review)).viewed).length, 1);
  await assert.rejects(
    addComment(review, { ...selection, start: 999, end: 1000 }, 'No such line'),
    /not present/,
  );
  await writeFile(path.join(r.repo, 'app.ts'), 'changed since review\n');
  assert.equal(
    Object.keys((await readState(await inspect(r))).viewed).length,
    0,
  );
});

test('a general question includes actual patches, not only filenames', async (t) => {
  const r = await fixture(t);
  const prepared = await preparePrompt({
    review: r,
    provider: 'claude',
    mode: 'ask',
    question: 'What changed?',
  });
  assert.match(prepared.prompt, /\+export const answer = 2/);
  assert.match(prepared.prompt, /Answer only/);
});

async function finish(run: AgentRun) {
  for (let i = 0; i < 300 && run.status === 'running'; i++) {
    await new Promise((r) => setTimeout(r, 20));
  }
  assert.notEqual(run.status, 'running');
}

test('real child-process transport sends all feedback and marks sent only on explicit completion', async (t) => {
  const r = await fixture(t);
  const review = await inspect(r);
  await addComment(review, selection, 'Make it 3.');
  const binary = path.join(r.repo, '.git', 'fake-codex.cjs');
  await writeFakeCodex(
    binary,
    `assert.ok(prompt.includes('Make it 3.')); text('Changed.'); complete();`,
  );
  const old = process.env.REVUE_CODEX_PATH;
  process.env.REVUE_CODEX_PATH = binary;
  t.after(() => {
    if (old) {
      process.env.REVUE_CODEX_PATH = old;
    } else {
      delete process.env.REVUE_CODEX_PATH;
    }
  });
  const runner = new AgentRunner(
    undefined,
    path.join(r.repo, '.git', 'agent-worktrees'),
  );
  const run = await runner.start({
    review: r,
    provider: 'codex',
    mode: 'apply',
  });
  await finish(run);
  assert.equal(run.status, 'completed');
  assert.ok(run.events.some((e) => e.text === 'Changed.'));
  assert.equal((await readState(review)).comments[0].status, 'sent');
  await flushRunHistory(run.id);
  const discussion = (await readState(review)).threads![0];
  assert.equal(discussion.kind, 'change');
  assert.equal(discussion.turns[0].status, 'completed');
  assert.ok(
    discussion.turns[0].events.some((event) => event.text === 'Changed.'),
  );
  await mutateState(review, (s) => {
    s.comments[0].status = 'queued';
  });
  await writeFakeCodex(binary, `complete('failed');`);
  const failed = await runner.start({
    review: r,
    provider: 'codex',
    mode: 'apply',
  });
  await finish(failed);
  assert.equal(failed.status, 'failed');
  assert.equal((await readState(review)).comments[0].status, 'queued');
});

test('local service requires token and rejects cross-origin requests', async (t) => {
  const r = await fixture(t);
  const ui = path.join(r.repo, '.git', 'test-ui');
  await mkdir(ui);
  await writeFile(path.join(ui, 'index.html'), '<h1>Revue</h1>');
  const service = await startServer({ uiDir: ui, initial: r });
  t.after(() => service.close());
  const call = (headers: Record<string, string> = {}) =>
    fetch(`${service.origin}/api`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', ...headers },
      body: JSON.stringify({ method: 'review', params: r }),
    });
  assert.equal((await call()).status, 403);
  assert.equal(
    (
      await call({
        'X-Revue-Token': service.token,
        Origin: 'https://evil.example',
      })
    ).status,
    403,
  );
  const response = await call({ 'X-Revue-Token': service.token });
  assert.equal(response.status, 200);
  const payload = (await response.json()) as {
    review: { files: Array<{ path: string }> };
  };
  assert.equal(payload.review.files[0].path, 'app.ts');
  const html = await fetch(service.origin);
  assert.equal(html.status, 200);
  assert.match(
    html.headers.get('content-security-policy')!,
    /frame-ancestors 'none'/,
  );
});

test('questions against a committed snapshot read an isolated checkout when the original is dirty', async (t) => {
  const r = await fixture(t);
  await writeFile(path.join(r.repo, 'app.ts'), 'dirty unrelated local edit\n');
  const binary = path.join(r.repo, '.git', 'snapshot-agent.cjs');
  await writeFakeCodex(
    binary,
    `text(fs.readFileSync('app.ts', 'utf8')); complete();`,
  );
  const old = process.env.REVUE_CODEX_PATH;
  process.env.REVUE_CODEX_PATH = binary;
  t.after(() => {
    if (old) {
      process.env.REVUE_CODEX_PATH = old;
    } else {
      delete process.env.REVUE_CODEX_PATH;
    }
  });
  const runner = new AgentRunner(
    undefined,
    path.join(r.repo, '.git', 'agent-worktrees'),
  );
  const run = await runner.start({
    review: r,
    provider: 'codex',
    mode: 'ask',
    question: 'What changed?',
  });
  await finish(run);
  assert.equal(run.status, 'completed');
  assert.ok(
    run.events.some(
      (e) => e.kind === 'message' && e.text.includes('answer = 2'),
    ),
  );
  assert.match(
    await readFile(path.join(r.repo, 'app.ts'), 'utf8'),
    /dirty unrelated/,
  );
  for (let i = 0; i < 40; i++) {
    if (
      !(await git(r.repo, ['worktree', 'list', '--porcelain'])).includes(
        'rev-context-',
      )
    ) {
      break;
    }
    await new Promise((r) => setTimeout(r, 25));
  }
  assert.doesNotMatch(
    await git(r.repo, ['worktree', 'list', '--porcelain']),
    /rev-context-/,
  );
});

test('cancellation stops the provider and preserves queued comments', async (t) => {
  const r = await fixture(t);
  const review = await inspect(r);
  await addComment(review, selection, 'Change this.');
  const binary = path.join(r.repo, '.git', 'slow-agent.cjs');
  await writeFakeCodex(binary, `text('Ready');`);
  const old = process.env.REVUE_CODEX_PATH;
  process.env.REVUE_CODEX_PATH = binary;
  t.after(() => {
    if (old) {
      process.env.REVUE_CODEX_PATH = old;
    } else {
      delete process.env.REVUE_CODEX_PATH;
    }
  });
  const runner = new AgentRunner(
    undefined,
    path.join(r.repo, '.git', 'agent-worktrees'),
  );
  const run = await runner.start({
    review: r,
    provider: 'codex',
    mode: 'apply',
  });
  for (let i = 0; i < 100 && !run.events.some((e) => e.text === 'Ready'); i++) {
    await new Promise((r) => setTimeout(r, 20));
  }
  runner.cancel(run.id);
  await finish(run);
  assert.equal(run.status, 'cancelled');
  assert.equal((await readState(review)).comments[0].status, 'queued');
});
