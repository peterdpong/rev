import { test, type TestContext } from 'node:test';
import assert from 'node:assert/strict';
import {
  access,
  chmod,
  mkdtemp,
  mkdir,
  writeFile,
  rename,
  rm,
  symlink,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import {
  runGitText,
  inspect,
  filePatch,
  reviewPatches,
  resolveRef,
} from '../../core/git';
import { MAX_PATCH_BYTES } from '../../core/git/constants';

async function fixture(t: TestContext) {
  const repo = await mkdtemp(path.join(tmpdir(), 'rev-git-'));
  t.after(() => rm(repo, { recursive: true, force: true }));
  await runGitText(repo, ['init', '-b', 'main']);
  await runGitText(repo, ['config', 'user.name', 'Rev Test']);
  await runGitText(repo, ['config', 'user.email', 'test@example.invalid']);
  await writeFile(path.join(repo, 'file.txt'), 'one\ntwo\nthree\n');
  await runGitText(repo, ['add', '.']);
  await runGitText(repo, ['commit', '-m', 'base']);
  await runGitText(repo, ['switch', '-c', 'feature']);
  return repo;
}

async function commit(repo: string) {
  await runGitText(repo, ['add', '.']);
  await runGitText(repo, ['commit', '-m', 'change']);
}

test('branch review uses the merge base and leaves the checkout untouched', async (t) => {
  const repo = await fixture(t);
  await writeFile(path.join(repo, 'file.txt'), 'one\nchanged\nthree\n');
  await commit(repo);
  const head = await resolveRef(repo, 'HEAD');
  await runGitText(repo, ['switch', 'main']);
  await writeFile(path.join(repo, 'base-only.txt'), 'base change\n');
  await commit(repo);
  const review = await inspect({ repo, base: 'main', head: 'feature' });
  assert.equal(review.headSha, head);
  assert.notEqual(review.mergeBase, review.baseSha);
  assert.deepEqual(
    review.files.map((file) => file.path),
    ['file.txt'],
  );
  assert.match((await filePatch(review, 'file.txt')).patch, /\+changed/);
  assert.equal(
    (await runGitText(repo, ['branch', '--show-current'])).trim(),
    'main',
  );
});

test('renamed and unusual paths retain exact identities', async (t) => {
  const repo = await fixture(t);
  await rename(path.join(repo, 'file.txt'), path.join(repo, 'line\nbreak.txt'));
  await commit(repo);
  const review = await inspect({ repo, base: 'main' });
  assert.equal(review.files[0].oldPath, 'file.txt');
  assert.equal(review.files[0].path, 'line\nbreak.txt');
  const patches = await reviewPatches(review);
  assert.equal(patches.length, 1);
  assert.equal(patches[0].error, undefined);
  assert.match(patches[0].patch, /rename/);
});

test('working review includes tracked and untracked changes; unrelated paths are rejected', async (t) => {
  const repo = await fixture(t);
  await writeFile(path.join(repo, 'file.txt'), 'modified\n');
  await writeFile(path.join(repo, 'new file.txt'), 'new\n');
  const review = await inspect({ repo, base: 'main', mode: 'working' });
  assert.deepEqual(review.files.map((file) => file.path).sort(), [
    'file.txt',
    'new file.txt',
  ]);
  assert.match(
    (await filePatch(review, 'new file.txt')).patch,
    /new file mode/,
  );
  await assert.rejects(
    filePatch(review, '../outside'),
    /not in this comparison/,
  );
});

test('untracked symlinks never read outside the repository and large files are bounded', async (t) => {
  const repo = await fixture(t);
  const outside = await mkdtemp(path.join(tmpdir(), 'rev-outside-'));
  t.after(() => rm(outside, { recursive: true, force: true }));
  await writeFile(path.join(outside, 'private.txt'), 'secret contents');
  await symlink(path.join(outside, 'private.txt'), path.join(repo, 'link.txt'));
  await symlink(outside, path.join(repo, 'linkdir'));
  await mkdir(path.join(repo, 'src'));
  await writeFile(
    path.join(repo, 'src/large.txt'),
    'x'.repeat(MAX_PATCH_BYTES + 1),
  );
  const review = await inspect({ repo, base: 'main', mode: 'working' });

  // Like Git, a symlink is shown as its link text, not its target's contents.
  const link = (await filePatch(review, 'link.txt')).patch;
  assert.match(link, /new file mode 120000/);
  assert.ok(link.includes(path.join(outside, 'private.txt')));
  assert.doesNotMatch(link, /secret contents/);

  // A path through a symlinked folder is refused even if a client lists it.
  review.files.push({
    path: 'linkdir/private.txt',
    status: '?',
    added: 0,
    removed: 0,
    binary: false,
  });
  await assert.rejects(
    filePatch(review, 'linkdir/private.txt'),
    /outside this repository/,
  );
  assert.equal((await filePatch(review, 'src/large.txt')).tooLarge, true);
});

test('untracked files get Git-shaped patches, modes and line counts', async (t) => {
  const repo = await fixture(t);
  await writeFile(path.join(repo, 'empty.txt'), '');
  await writeFile(path.join(repo, 'run.sh'), '#!/bin/sh\necho hi\n');
  await chmod(path.join(repo, 'run.sh'), 0o755);
  await writeFile(path.join(repo, 'data.bin'), Buffer.from([1, 0, 2]));
  const review = await inspect({ repo, base: 'main', mode: 'working' });
  const byPath = (name: string) => review.files.find((f) => f.path === name)!;

  assert.deepEqual(
    [
      byPath('run.sh').added,
      byPath('empty.txt').added,
      byPath('data.bin').binary,
    ],
    [2, 0, true],
  );
  assert.equal(
    (await filePatch(review, 'empty.txt')).patch,
    'diff --git a/empty.txt b/empty.txt\nnew file mode 100644\n',
  );
  assert.match(
    (await filePatch(review, 'run.sh')).patch,
    /new file mode 100755/,
  );
  assert.match(
    (await filePatch(review, 'data.bin')).patch,
    /Binary files \/dev\/null and b\/data.bin differ/,
  );
});

test('client-supplied reviews cannot inject Git options or switch repositories', async (t) => {
  const repo = await fixture(t);
  await writeFile(path.join(repo, 'file.txt'), 'changed\n');
  await commit(repo);
  const review = await inspect({ repo, base: 'main' });
  const target = path.join(repo, 'PWNED');

  for (const evil of [
    { ...review, mergeBase: `--output=${target}` },
    { ...review, headSha: `--output=${target}` },
    { ...review, mode: 'other' as never },
  ]) {
    await assert.rejects(filePatch(evil, 'file.txt'), /Invalid|Unknown/);
    await assert.rejects(reviewPatches(evil), /Invalid|Unknown/);
  }
  await assert.rejects(
    filePatch({ ...review, repo: path.join(repo, '..') }, 'file.txt'),
  );
  await assert.rejects(access(target));
});

test('review keys, branch lists and default bases', async (t) => {
  const repo = await fixture(t);
  await writeFile(path.join(repo, 'file.txt'), 'changed\n');
  await commit(repo);
  await runGitText(repo, ['update-ref', 'refs/remotes/origin/main', 'main']);
  await runGitText(repo, [
    'symbolic-ref',
    'refs/remotes/origin/HEAD',
    'refs/remotes/origin/main',
  ]);
  const branch = await inspect({ repo });
  const working = await inspect({ repo, mode: 'working' });
  assert.equal(branch.base, 'origin/main');
  assert.equal(branch.baseSha, await resolveRef(repo, 'main'));
  assert.equal(working.baseSha, branch.baseSha);
  assert.equal(working.mergeBase, branch.mergeBase);
  assert.equal(working.headSha, branch.headSha);
  assert.equal(branch.commitCount, 1);
  assert.equal(working.commitCount, branch.commitCount);
  assert.equal(working.commitRange, undefined);
  assert.equal(working.scopeKey, working.key);
  assert.notEqual(branch.key, working.key);
  assert.equal(branch.key, (await inspect({ repo })).key);
  assert.deepEqual(branch.branches, ['feature', 'main', 'origin/main']);
});

test('binary changes are flagged in the file list', async (t) => {
  const repo = await fixture(t);
  await writeFile(path.join(repo, 'image.bin'), Buffer.from([0, 1, 2, 3]));
  await commit(repo);
  const review = await inspect({ repo, base: 'main' });
  assert.deepEqual(review.files, [
    {
      status: 'A',
      path: 'image.bin',
      added: 0,
      removed: 0,
      binary: true,
    },
  ]);
});

for (const tracked of [true, false]) {
  test(`${tracked ? 'tracked' : 'untracked'} patches share an exact byte limit, including UTF-8 and diff headers`, async (t) => {
    const repo = await fixture(t);
    const name = 'multibyte.txt';
    let content = 'é'.repeat(500_000);
    const patchForContent = async () => {
      await writeFile(path.join(repo, name), content);
      if (tracked) {
        await commit(repo);
      }
      return filePatch(
        await inspect({
          repo,
          base: 'main',
          mode: tracked ? 'branch' : 'working',
        }),
        name,
      );
    };

    const initial = await patchForContent();
    assert.equal(initial.tooLarge, false);
    // Keep the same one-line diff shape and fill the remaining byte budget.
    content += 'x'.repeat(MAX_PATCH_BYTES - Buffer.byteLength(initial.patch));
    const boundary = await patchForContent();
    assert.equal(boundary.tooLarge, false);
    assert.equal(Buffer.byteLength(boundary.patch), MAX_PATCH_BYTES);
    assert.ok(boundary.patch.length < MAX_PATCH_BYTES);

    content += 'x';
    // The source still fits; its diff headers take the output over the limit.
    assert.ok(Buffer.byteLength(content) < MAX_PATCH_BYTES);
    const over = await patchForContent();
    assert.equal(over.tooLarge, true);
    assert.equal(over.patch, '');
    // The digest still identifies the content so viewed markers can expire.
    assert.match(over.digest, /^[0-9a-f]{64}$/);
  });
}
