import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, rm, symlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { git, inspect, filePatch, reviewPatches } from '../src/core/git';
import { HostService } from '../src/core/host';
import type { ReviewPatch } from '../src/shared';

async function fixture(t: any) {
  const root = await mkdtemp(path.join(tmpdir(), 'patch-continuous-'));
  const repo = path.join(root, 'repo'); await mkdir(repo);
  await git(repo, ['init', '-b', 'main']);
  await git(repo, ['config', 'user.name', 'Test']); await git(repo, ['config', 'user.email', 'test@example.invalid']);
  for (const file of ['first.ts', 'second.ts', 'rename.ts', 'deleted.ts']) await writeFile(path.join(repo, file), `${file} before\n`);
  await git(repo, ['add', '.']); await git(repo, ['commit', '-m', 'base']);
  await git(repo, ['switch', '-c', 'feature']);
  await writeFile(path.join(repo, 'first.ts'), 'first change\n');
  await writeFile(path.join(repo, 'second.ts'), 'second change\n');
  await git(repo, ['mv', 'rename.ts', 'renamed file.ts']); await git(repo, ['rm', 'deleted.ts']);
  await writeFile(path.join(repo, 'binary.bin'), Buffer.from([0, 255, 1]));
  await writeFile(path.join(repo, 'empty.ts'), '');
  await git(repo, ['add', '.']); await git(repo, ['commit', '-m', 'changes']);
  const request = { repo, base: 'main', head: 'feature' };
  const service = new HostService(undefined, path.join(root, 'data'));
  t.after(async () => { await service.close(); await rm(root, { recursive: true, force: true }); });
  return { root, repo, request, service };
}

test('continuous patches retain Git order and per-file digests across text, rename, delete, empty and binary files', async t => {
  const { request, service } = await fixture(t);
  const review = await inspect(request);
  const patches = await service.call('patches', { review: request, headSha: review.headSha, mergeBase: review.mergeBase }) as ReviewPatch[];
  assert.deepEqual(patches.map(p => p.path), review.files.map(f => f.path));
  for (const entry of patches) {
    assert.equal(entry.error, undefined);
    assert.deepEqual({ patch: entry.patch, digest: entry.digest, tooLarge: entry.tooLarge }, await filePatch(review, entry.path));
  }
  assert.match(patches.find(p => p.path === 'renamed file.ts')!.patch, /rename from rename.ts/);
  const second = patches.find(p => p.path === 'second.ts')!;
  const state: any = await service.call('comment', { review: request, selection: { path: 'second.ts', side: 'additions', start: 1, end: 1 }, digest: second.digest, body: 'Feedback on the second file' });
  assert.equal(state.comments[0].path, 'second.ts');
  await assert.rejects(service.call('comment', { review: request, selection: { path: 'second.ts', side: 'additions', start: 1, end: 1 }, digest: patches.find(p => p.path === 'first.ts')!.digest, body: 'Wrong digest' }), /changed/i);
});

test('a moved branch is rejected instead of mixing patches with an older review', async t => {
  const { request, service, repo } = await fixture(t);
  const review = await inspect(request);
  await writeFile(path.join(repo, 'first.ts'), 'new commit\n'); await git(repo, ['commit', '-am', 'advanced']);
  await assert.rejects(service.call('patches', { review: request, headSha: review.headSha, mergeBase: review.mergeBase }), /comparison changed/i);
  const original = await reviewPatches(review);
  assert.match(original.find(p => p.path === 'first.ts')!.patch, /\+first change/);
  assert.doesNotMatch(original.find(p => p.path === 'first.ts')!.patch, /new commit/);
});

test('unavailable working files do not hide other files, and oversized content is not transferred', async t => {
  const { request, repo, root } = await fixture(t);
  await writeFile(path.join(repo, 'large.txt'), 'x'.repeat(2_000_001));
  await writeFile(path.join(repo, 'untracked.bin'), Buffer.from([0, 1]));
  await writeFile(path.join(repo, 'line\nbreak.ts'), 'literal filename\n');
  await writeFile(path.join(root, 'outside'), 'private\n');
  await symlink(path.join(root, 'outside'), path.join(repo, 'outside-link'));
  const review = await inspect({ ...request, mode: 'working' });
  const patches = await reviewPatches(review);
  assert.deepEqual(patches.map(p => p.path), review.files.map(f => f.path));
  const large = patches.find(p => p.path === 'large.txt')!;
  assert.equal(large.tooLarge, true); assert.equal(large.patch, '');
  assert.match(patches.find(p => p.path === 'outside-link')!.error!, /outside this repository/);
  assert.equal(patches.find(p => p.path === 'outside-link')!.digest, '');
  assert.match(patches.find(p => p.path === 'line\nbreak.ts')!.patch, /line\\012break.ts/);
  assert.match(patches.find(p => p.path === 'first.ts')!.patch, /\+first change/);
  assert.equal(patches.find(p => p.path === 'untracked.bin')!.patch, 'Binary file (untracked)');
});
