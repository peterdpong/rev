import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, rm, symlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { git, inspect, filePatch, reviewPatches } from '../core/git';
import { HostService } from '../core/host';
import type { ReviewPatch } from '../core/types';
import { mutateState } from '../core/store';

async function fixture(t: any) {
  const root = await mkdtemp(path.join(tmpdir(), 'rev-continuous-'));
  const repo = path.join(root, 'repo');
  await mkdir(repo);
  await git(repo, ['init', '-b', 'main']);
  await git(repo, ['config', 'user.name', 'Test']);
  await git(repo, ['config', 'user.email', 'test@example.invalid']);
  for (const file of ['first.ts', 'second.ts', 'rename.ts', 'deleted.ts']) {
    await writeFile(path.join(repo, file), `${file} before\n`);
  }
  await git(repo, ['add', '.']);
  await git(repo, ['commit', '-m', 'base']);
  await git(repo, ['switch', '-c', 'feature']);
  await writeFile(path.join(repo, 'first.ts'), 'first change\n');
  await writeFile(path.join(repo, 'second.ts'), 'second change\n');
  await git(repo, ['mv', 'rename.ts', 'renamed file.ts']);
  await git(repo, ['rm', 'deleted.ts']);
  await writeFile(path.join(repo, 'binary.bin'), Buffer.from([0, 255, 1]));
  await writeFile(path.join(repo, 'empty.ts'), '');
  await git(repo, ['add', '.']);
  await git(repo, ['commit', '-m', 'changes']);
  const request = { repo, base: 'main', head: 'feature' };
  const service = new HostService(undefined, path.join(root, 'data'));
  t.after(async () => {
    await service.close();
    await rm(root, { recursive: true, force: true });
  });
  return { root, repo, request, service };
}

test('continuous patches retain Git order and per-file digests across text, rename, delete, empty and binary files', async (t) => {
  const { request, service } = await fixture(t);
  const review = await inspect(request);
  const patches = (await service.call('patches', {
    review: request,
    headSha: review.headSha,
    mergeBase: review.mergeBase,
  })) as ReviewPatch[];
  assert.deepEqual(
    patches.map((p) => p.path),
    review.files.map((f) => f.path),
  );
  for (const entry of patches) {
    assert.equal(entry.error, undefined);
    assert.deepEqual(
      { patch: entry.patch, digest: entry.digest, tooLarge: entry.tooLarge },
      await filePatch(review, entry.path),
    );
  }
  assert.match(
    patches.find((p) => p.path === 'renamed file.ts')!.patch,
    /rename from rename.ts/,
  );

  const second = patches.find((p) => p.path === 'second.ts')!;
  const state: any = await service.call('comment', {
    review: request,
    selection: { path: 'second.ts', side: 'additions', start: 1, end: 1 },
    digest: second.digest,
    body: 'Feedback on the second file',
  });
  assert.equal(state.comments[0].path, 'second.ts');
  await assert.rejects(
    service.call('comment', {
      review: request,
      selection: { path: 'second.ts', side: 'additions', start: 1, end: 1 },
      digest: patches.find((p) => p.path === 'first.ts')!.digest,
      body: 'Wrong digest',
    }),
    /changed/i,
  );
});

test('a moved branch is rejected instead of mixing patches with an older review', async (t) => {
  const { request, service, repo } = await fixture(t);
  const review = await inspect(request);
  await writeFile(path.join(repo, 'first.ts'), 'new commit\n');
  await git(repo, ['commit', '-am', 'advanced']);
  await assert.rejects(
    service.call('patches', {
      review: request,
      headSha: review.headSha,
      mergeBase: review.mergeBase,
    }),
    /comparison changed/i,
  );
  const original = await reviewPatches(review);
  assert.match(
    original.find((p) => p.path === 'first.ts')!.patch,
    /\+first change/,
  );
  assert.doesNotMatch(
    original.find((p) => p.path === 'first.ts')!.patch,
    /new commit/,
  );
});

test('unavailable working files do not hide other files, and oversized content is not transferred', async (t) => {
  const { request, repo, root } = await fixture(t);
  await writeFile(path.join(repo, 'large.txt'), 'x'.repeat(2_000_001));
  await writeFile(path.join(repo, 'untracked.bin'), Buffer.from([0, 1]));
  await writeFile(path.join(repo, 'line\nbreak.ts'), 'literal filename\n');
  await writeFile(path.join(root, 'outside'), 'private\n');
  await symlink(path.join(root, 'outside'), path.join(repo, 'outside-link'));
  const review = await inspect({ ...request, mode: 'working' });
  const patches = await reviewPatches(review);
  assert.deepEqual(
    patches.map((p) => p.path),
    review.files.map((f) => f.path),
  );
  const large = patches.find((p) => p.path === 'large.txt')!;
  assert.equal(large.tooLarge, true);
  assert.equal(large.patch, '');
  const link = patches.find((p) => p.path === 'outside-link')!;
  assert.match(link.patch, /new file mode 120000/);
  assert.doesNotMatch(link.patch, /\+private/);
  assert.equal(link.error, undefined);
  assert.match(
    patches.find((p) => p.path === 'line\nbreak.ts')!.patch,
    /line\\012break.ts/,
  );
  assert.match(
    patches.find((p) => p.path === 'first.ts')!.patch,
    /\+first change/,
  );
  assert.match(
    patches.find((p) => p.path === 'untracked.bin')!.patch,
    /Binary files.*differ/,
  );
});

test('desktop previews defer large diffs and expansion loads only the requested file', async (t) => {
  const { request, service, repo } = await fixture(t);
  await writeFile(path.join(repo, 'wide.txt'), 'é'.repeat(150_000));
  await writeFile(path.join(repo, 'many-lines.txt'), 'line\n'.repeat(5_001));
  await writeFile(path.join(repo, 'over-limit.txt'), 'x'.repeat(2_000_001));
  await git(repo, ['add', '.']);
  await git(repo, ['commit', '-m', 'large diffs']);
  const review = await inspect(request);
  const params = {
    review: request,
    headSha: review.headSha,
    mergeBase: review.mergeBase,
  };
  const previews = (await service.call('patches', {
    ...params,
    preview: true,
  })) as ReviewPatch[];
  assert.deepEqual(
    previews.map((p) => p.path),
    review.files.map((f) => f.path),
  );
  for (const name of ['wide.txt', 'many-lines.txt', 'over-limit.txt']) {
    assert.deepEqual(
      previews.find((p) => p.path === name),
      {
        path: name,
        patch: '',
        digest: '',
        tooLarge: false,
        deferred: true,
      },
    );
  }
  assert.match(
    previews.find((p) => p.path === 'first.ts')!.patch,
    /first change/,
  );
  for (const name of ['wide.txt', 'many-lines.txt']) {
    const expanded = (await service.call('patch', {
      ...params,
      path: name,
    })) as ReviewPatch;
    assert.equal(expanded.tooLarge, false);
    assert.ok(expanded.patch.length > 0);
    assert.equal(expanded.digest, (await filePatch(review, name)).digest);
  }
  assert.deepEqual(
    await service.call('patch', { ...params, path: 'over-limit.txt' }),
    {
      patch: '',
      digest: '',
      tooLarge: true,
    },
  );
  await assert.rejects(
    service.call('patch', { ...params, path: '../outside' }),
    /not in this comparison/,
  );
  await writeFile(path.join(repo, 'first.ts'), 'changed again\n');
  await git(repo, ['commit', '-am', 'advance while a file is collapsed']);
  await assert.rejects(
    service.call('patch', { ...params, path: 'wide.txt' }),
    /comparison changed/i,
  );
});

test('working previews defer oversized untracked files and allow retry after local edits', async (t) => {
  const { request, service, repo } = await fixture(t);
  const working = { ...request, mode: 'working' };
  await writeFile(path.join(repo, 'local.txt'), 'x'.repeat(300_000));
  await writeFile(path.join(repo, 'many-lines.txt'), 'line\n'.repeat(5_001));
  const review = await inspect({ ...request, mode: 'working' });
  const params = {
    review: working,
    headSha: review.headSha,
    mergeBase: review.mergeBase,
  };
  const previews = (await service.call('patches', {
    ...params,
    preview: true,
  })) as ReviewPatch[];
  assert.equal(previews.find((p) => p.path === 'local.txt')!.deferred, true);
  assert.equal(
    previews.find((p) => p.path === 'many-lines.txt')!.deferred,
    true,
  );
  await rm(path.join(repo, 'local.txt'));
  await assert.rejects(
    service.call('patch', { ...params, path: 'local.txt' }),
    /not in this comparison/,
  );
  await writeFile(path.join(repo, 'local.txt'), 'now available\n');
  const retry = (await service.call('patch', {
    ...params,
    path: 'local.txt',
  })) as ReviewPatch;
  assert.match(retry.patch, /now available/);
  assert.equal(retry.tooLarge, false);
});

test('a high line count defers a patch without reading its content', async (t) => {
  const { request, repo } = await fixture(t);
  await writeFile(path.join(repo, 'many-lines.txt'), 'line\n'.repeat(5_001));
  await git(repo, ['add', 'many-lines.txt']);
  const review = await inspect({ ...request, mode: 'working' });
  // Keep the merged Git core's repository validation, even for previews.
  await assert.rejects(
    reviewPatches(
      { ...review, repo: path.join(repo, 'missing-checkout') },
      true,
    ),
  );
  await rm(path.join(repo, 'many-lines.txt'));
  const previews = await reviewPatches(review, true);
  assert.deepEqual(
    previews.find((p) => p.path === 'many-lines.txt'),
    {
      path: 'many-lines.txt',
      patch: '',
      digest: '',
      tooLarge: false,
      deferred: true,
    },
  );
  assert.match(
    previews.find((p) => p.path === 'first.ts')!.patch,
    /first change/,
  );
});

test('desktop metadata and viewed updates leave deferred markers for lazy validation', async (t) => {
  const { request, service, repo } = await fixture(t);
  const working = { ...request, mode: 'working' as const };
  await writeFile(path.join(repo, 'local.txt'), 'x'.repeat(300_000));
  const review = await inspect(working);
  const original = await filePatch(review, 'local.txt');
  await mutateState(review, (state) => {
    state.viewed['local.txt'] = original.digest;
  });
  await writeFile(path.join(repo, 'local.txt'), 'changed\n'.repeat(40_000));
  const desktop: any = await service.call('review', {
    ...working,
    preview: true,
  });
  assert.equal(desktop.state.viewed['local.txt'], original.digest);
  const updated: any = await service.call('viewed', {
    review: working,
    path: 'first.ts',
    viewed: true,
    preview: true,
    digest: (await filePatch(review, 'first.ts')).digest,
  });
  assert.equal(updated.viewed['local.txt'], original.digest);
  const loaded = await filePatch(review, 'local.txt');
  assert.notEqual(loaded.digest, original.digest);
  // Non-preview clients retain eager validation of all persisted markers.
  const standard: any = await service.call('review', working);
  assert.equal(standard.state.viewed['local.txt'], undefined);
});
