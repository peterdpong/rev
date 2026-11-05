import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import path from 'node:path';
import { tmpdir } from 'node:os';
import { runGitText as git, inspect, filePatch } from '../core/git';
import { readReviewState, updateReviewState } from '../core/review-state';
import type { ReviewRequest } from '../core/types';

async function fixture(t: any) {
  const repo = await mkdtemp(path.join(tmpdir(), 'rev-test-'));
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

test('viewed status expires when the reviewed code changes', async (t) => {
  const r = { ...(await fixture(t)), mode: 'working' as const };
  const review = await inspect(r);
  const patch = await filePatch(review, 'app.ts');

  await updateReviewState(review, (state) => {
    state.viewed['app.ts'] = patch.digest;
  });

  assert.equal(Object.keys((await readReviewState(review)).viewed).length, 1);

  await writeFile(path.join(r.repo, 'app.ts'), 'changed since review\n');

  assert.equal(
    Object.keys((await readReviewState(await inspect(r))).viewed).length,
    0,
  );
});

test('updates return state without stale markers', async (t) => {
  const review = await inspect(await fixture(t));
  const patch = await filePatch(review, 'app.ts');

  const state = await updateReviewState(review, (state) => {
    state.viewed['app.ts'] = patch.digest;
    state.viewed['gone.ts'] = 'stale-digest';
  });

  assert.deepEqual(state.viewed, { 'app.ts': patch.digest });
});

test('viewed status expires when a too-large file changes', async (t) => {
  const request = await fixture(t);
  const large = (fill: string) =>
    writeFile(
      path.join(request.repo, 'large.txt'),
      `${fill}\n`.repeat(1_100_000),
    );

  await large('a');
  await git(request.repo, ['add', '.']);
  await git(request.repo, ['commit', '-m', 'large']);
  const review = await inspect(request);
  const patch = await filePatch(review, 'large.txt');
  assert.equal(patch.tooLarge, true);
  assert.notEqual(patch.digest, '');

  await updateReviewState(review, (state) => {
    state.viewed['large.txt'] = patch.digest;
  });
  assert.equal(
    (await readReviewState(review)).viewed['large.txt'],
    patch.digest,
  );

  await large('b');
  await git(request.repo, ['commit', '-am', 'large again']);
  assert.deepEqual(
    Object.keys((await readReviewState(await inspect(request))).viewed),
    [],
  );
});
