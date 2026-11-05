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

test('concurrent updates preserve all viewed markers', async (t) => {
  const request = { ...(await fixture(t)), mode: 'working' as const };
  const names = ['first.ts', 'second.ts', 'third.ts'];

  await Promise.all(
    names.map((name) =>
      writeFile(path.join(request.repo, name), 'export const added = true;\n'),
    ),
  );

  const review = await inspect(request);
  const patches = await Promise.all(
    names.map((name) => filePatch(review, name)),
  );
  const viewed = Object.fromEntries(
    names.map((name, i) => [name, patches[i].digest]),
  );

  const updates = await Promise.all(
    names.map((name) =>
      updateReviewState(review, (state) => {
        state.viewed[name] = viewed[name];
      }),
    ),
  );

  assert.deepEqual(
    updates.map((state) => Object.keys(state.viewed).length).sort(),
    [1, 2, 3],
  );
  assert.deepEqual((await readReviewState(review)).viewed, viewed);
});

test('a failed update does not block later updates', async (t) => {
  const review = await inspect(await fixture(t));
  const patch = await filePatch(review, 'app.ts');
  const failure = new Error('Update failed');
  let attempts = 0;

  const results = await Promise.allSettled(
    Array.from({ length: 3 }, () =>
      updateReviewState(review, (state) => {
        if (attempts++ === 0) {
          throw failure;
        }

        state.viewed['app.ts'] = patch.digest;
      }),
    ),
  );

  assert.equal(attempts, 3);
  assert.deepEqual(
    results.filter((result) => result.status === 'rejected'),
    [{ status: 'rejected', reason: failure }],
  );
  assert.deepEqual((await readReviewState(review)).viewed, {
    'app.ts': patch.digest,
  });
});

test('updates to the same file apply in call order', async (t) => {
  const review = await inspect(await fixture(t));
  const patch = await filePatch(review, 'app.ts');

  await Promise.all([
    updateReviewState(review, (state) => {
      state.viewed['app.ts'] = patch.digest;
    }),
    updateReviewState(review, (state) => {
      delete state.viewed['app.ts'];
    }),
  ]);

  assert.deepEqual((await readReviewState(review)).viewed, {});
});
