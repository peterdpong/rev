import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import path from 'node:path';
import { tmpdir } from 'node:os';
import { runGitText as git, inspect, filePatch } from '../core/git';
import { readState, mutateState } from '../core/store';
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

  await mutateState(review, (state) => {
    state.viewed['app.ts'] = patch.digest;
  });

  assert.equal(Object.keys((await readState(review)).viewed).length, 1);

  await writeFile(path.join(r.repo, 'app.ts'), 'changed since review\n');

  assert.equal(
    Object.keys((await readState(await inspect(r))).viewed).length,
    0,
  );
});

test('updates return state without stale markers', async (t) => {
  const review = await inspect(await fixture(t));
  const patch = await filePatch(review, 'app.ts');

  const state = await mutateState(review, (state) => {
    state.viewed['app.ts'] = patch.digest;
    state.viewed['gone.ts'] = 'stale-digest';
  });

  assert.deepEqual(state.viewed, { 'app.ts': patch.digest });
});
