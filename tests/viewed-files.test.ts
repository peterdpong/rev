import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, readFile, mkdir, rm } from 'node:fs/promises';
import path from 'node:path';
import { tmpdir } from 'node:os';
import { git, inspect, filePatch, hash } from '../core/git';
import { readState, mutateState } from '../core/store';
import type { ReviewRequest } from '../core/types';
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

test('older review identities retain saved markers and unknown state during migration', async (t) => {
  const request = await fixture(t);
  const review = await inspect(request);
  const patch = await filePatch(review, 'app.ts');
  const key = hash([review.repo, review.base, review.branch].join('\0')).slice(
    0,
    24,
  );
  const folder = path.join(
    (await git(review.repo, ['rev-parse', '--absolute-git-dir'])).trim(),
    'revue',
  );
  await mkdir(folder, { recursive: true });
  const legacy = {
    viewed: { 'app.ts': patch.digest },
    preservedMetadata: 'keep',
  };
  const legacyFile = path.join(folder, `${key}.json`);
  await writeFile(legacyFile, JSON.stringify(legacy));
  const loaded = await readState(review);
  assert.equal(loaded.viewed['app.ts'], patch.digest);
  await mutateState(review, (state) => {
    state.viewed = {};
  });
  const migrated = JSON.parse(
    await readFile(path.join(folder, `${review.scopeKey}.json`), 'utf8'),
  );
  assert.equal(migrated.preservedMetadata, 'keep');
  assert.deepEqual(migrated.viewed, {});
  assert.deepEqual(JSON.parse(await readFile(legacyFile, 'utf8')), legacy);
});

test('agent builds can add comments to viewer-only saved state', async (t) => {
  const review = await inspect(await fixture(t));
  const patch = await filePatch(review, 'app.ts');
  const folder = path.join(
    (await git(review.repo, ['rev-parse', '--absolute-git-dir'])).trim(),
    'revue',
  );
  await mkdir(folder, { recursive: true });
  await writeFile(
    path.join(folder, `${review.scopeKey}.json`),
    JSON.stringify({ viewed: { 'app.ts': patch.digest } }),
  );
  const state = await readState(review);
  assert.deepEqual(state.comments, []);
  const { addComment } = await import('../core/store');
  await addComment(
    review,
    { path: 'app.ts', side: 'additions', start: 1, end: 1 },
    'Explain this change',
    patch.digest,
  );
  const updated = await readState(review);
  assert.equal(updated.comments.length, 1);
  assert.equal(updated.viewed['app.ts'], patch.digest);
});
