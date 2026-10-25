import { test, type TestContext } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, rm, symlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { runGitText, inspect, filePatch, ignoredPaths } from '../../core/git';

async function fixture(t: TestContext) {
  const repo = await mkdtemp(path.join(tmpdir(), 'rev-ignored-test-'));
  t.after(() => rm(repo, { recursive: true, force: true }));
  await runGitText(repo, ['init', '-b', 'main']);
  await runGitText(repo, ['config', 'user.name', 'Test']);
  await runGitText(repo, ['config', 'user.email', 'test@example.invalid']);
  await runGitText(repo, ['commit', '--allow-empty', '-m', 'base']);
  await runGitText(repo, ['switch', '-c', 'feature']);
  return repo;
}

async function save(repo: string) {
  await runGitText(repo, ['add', '-f', '.']);
  await runGitText(repo, ['commit', '-m', 'files']);
}

test('committed files are never marked ignored, even when reviewing another checkout', async (t) => {
  const repo = await fixture(t);
  await mkdir(path.join(repo, 'docs'));
  await writeFile(path.join(repo, 'docs/notes.md'), 'notes');
  await writeFile(path.join(repo, '.gitignore'), 'docs/\n');
  await save(repo);
  await runGitText(repo, ['switch', 'main']);
  await writeFile(path.join(repo, '.git/info/exclude'), 'docs/\n');
  const before = await runGitText(repo, ['status', '--porcelain']);
  const review = await inspect({ repo, base: 'main', head: 'feature' });
  assert.deepEqual(review.files.map((file) => file.path).sort(), [
    '.gitignore',
    'docs/notes.md',
  ]);
  assert.ok(review.files.every((file) => file.ignored === false));
  assert.equal(await runGitText(repo, ['status', '--porcelain']), before);
  assert.equal(
    (await runGitText(repo, ['branch', '--show-current'])).trim(),
    'main',
  );
});

test('working reviews mark only ignored untracked files and can read their patches', async (t) => {
  const repo = await fixture(t);
  await writeFile(path.join(repo, 'tracked.txt'), 'base');
  await save(repo);
  await writeFile(path.join(repo, '.gitignore'), '*.txt\n');
  await writeFile(path.join(repo, 'tracked.txt'), 'changed');
  await writeFile(path.join(repo, 'hidden.txt'), 'local only\n');
  await writeFile(path.join(repo, 'ordinary.md'), 'untracked\n');
  await writeFile(path.join(repo, 'staged.txt'), 'staged\n');
  await runGitText(repo, ['add', '-f', 'staged.txt']);
  const before = await runGitText(repo, ['status', '--porcelain']);
  const index = await runGitText(repo, ['diff', '--cached']);
  const review = await inspect({ repo, base: 'main', mode: 'working' });
  assert.deepEqual(
    review.files.filter((file) => file.ignored).map((file) => file.path),
    ['hidden.txt'],
  );
  for (const name of ['tracked.txt', 'staged.txt', 'ordinary.md']) {
    assert.equal(
      review.files.find((file) => file.path === name)?.ignored,
      false,
    );
  }
  assert.equal(
    review.files.find((file) => file.path === 'hidden.txt')?.status,
    '?',
  );
  assert.match((await filePatch(review, 'hidden.txt')).patch, /\+local only/);
  assert.deepEqual(
    [
      ...(await ignoredPaths(repo, [
        'tracked.txt',
        'staged.txt',
        'hidden.txt',
        'ordinary.md',
      ])),
    ],
    ['hidden.txt'],
  );
  const committed = await inspect({ repo, base: 'main', head: 'feature' });
  assert.deepEqual(
    committed.files.map((file) => file.path),
    ['tracked.txt'],
  );
  assert.equal(committed.files[0].ignored, false);
  assert.equal(await runGitText(repo, ['status', '--porcelain']), before);
  assert.equal(await runGitText(repo, ['diff', '--cached']), index);
});

test('untracked labels respect nested rules, negation, ignored directories and unusual paths', async (t) => {
  const repo = await fixture(t);
  await mkdir(path.join(repo, 'nested'));
  await mkdir(path.join(repo, 'cache/deep'), { recursive: true });
  await writeFile(path.join(repo, '.gitignore'), '*.log\ncache/\n');
  await writeFile(path.join(repo, 'nested/.gitignore'), '!keep.log\n*.tmp\n');
  for (const file of ['keep.log', 'ignored.log', 'a\tb\nc.tmp']) {
    await writeFile(path.join(repo, 'nested', file), 'data');
  }
  await writeFile(path.join(repo, 'cache/deep/result'), 'generated');
  const review = await inspect({ repo, base: 'main', mode: 'working' });
  assert.equal(
    review.files.find((file) => file.path === 'nested/keep.log')?.ignored,
    false,
  );
  assert.deepEqual(
    review.files
      .filter((file) => file.ignored)
      .map((file) => file.path)
      .sort(),
    ['cache/deep/result', 'nested/a\tb\nc.tmp', 'nested/ignored.log'],
  );
});

test('local and global excludes label untracked files; symlink ignore files are not followed', async (t) => {
  const repo = await fixture(t);
  await writeFile(path.join(repo, 'rules'), '*.txt\n');
  await symlink('rules', path.join(repo, '.gitignore'));
  await writeFile(path.join(repo, 'visible.txt'), 'data');
  await writeFile(path.join(repo, 'local.log'), 'data');
  await writeFile(path.join(repo, 'global.cache'), 'data');
  await writeFile(path.join(repo, '.git/info/exclude'), '*.log\n');
  const globalRules = path.join(repo, '.git/global-ignore');
  await writeFile(globalRules, '*.cache\n');
  await runGitText(repo, ['config', 'core.excludesFile', globalRules]);
  const review = await inspect({ repo, base: 'main', mode: 'working' });
  assert.equal(
    review.files.find((file) => file.path === 'visible.txt')?.ignored,
    false,
  );
  assert.deepEqual(
    review.files
      .filter((file) => file.ignored)
      .map((file) => file.path)
      .sort(),
    ['global.cache', 'local.log'],
  );
});

test('labels follow local ignore edits and disappear when a file is force-added', async (t) => {
  const repo = await fixture(t);
  await writeFile(path.join(repo, 'scratch.tmp'), 'data');
  const request = { repo, base: 'main', mode: 'working' as const };
  const scratch = async () =>
    (await inspect(request)).files.find((file) => file.path === 'scratch.tmp')!;
  assert.equal((await scratch()).ignored, false);
  await writeFile(path.join(repo, '.gitignore'), '*.tmp\n');
  assert.equal((await scratch()).ignored, true);
  await writeFile(path.join(repo, '.gitignore'), '*.tmp\n!scratch.tmp\n');
  assert.equal((await scratch()).ignored, false);
  await writeFile(path.join(repo, '.gitignore'), '*.tmp\n');
  await runGitText(repo, ['add', '-f', 'scratch.tmp']);
  const staged = await scratch();
  assert.equal(staged.status, 'A');
  assert.equal(staged.ignored, false);
});

test('a staged deletion remains unmarked when an ignored untracked copy is left on disk', async (t) => {
  const repo = await fixture(t);
  await writeFile(path.join(repo, 'removed.txt'), 'tracked\n');
  await save(repo);
  await runGitText(repo, ['rm', '--cached', 'removed.txt']);
  await writeFile(path.join(repo, '.gitignore'), '*.txt\n');
  const review = await inspect({ repo, base: 'HEAD', mode: 'working' });
  const matches = review.files.filter((file) => file.path === 'removed.txt');
  assert.equal(matches.length, 1);
  assert.equal(matches[0].status, 'D');
  assert.equal(matches[0].ignored, false);
  assert.match(
    (await filePatch(review, 'removed.txt')).patch,
    /deleted file mode/,
  );
});
