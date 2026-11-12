import { test, type TestContext } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, rm, symlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import {
  runGitText,
  inspect,
  filePatch,
  listUntrackedFiles,
} from '../../core/git';

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

test('untracked file lists respect ignore rules and literal path selections', async (t) => {
  const repo = await fixture(t);
  await writeFile(path.join(repo, '.gitignore'), '*.log\n');
  await writeFile(path.join(repo, 'tracked.txt'), 'tracked\n');
  await save(repo);
  const names = ['*.txt', 'ordinary.txt', '-option.txt', 'tab\tline\n.txt'];
  for (const name of [...names, 'ignored.log']) {
    await writeFile(path.join(repo, name), 'untracked\n');
  }

  assert.deepEqual((await listUntrackedFiles(repo)).sort(), [...names].sort());
  assert.deepEqual(await listUntrackedFiles(repo, ['*.txt']), ['*.txt']);
  assert.deepEqual(
    (await listUntrackedFiles(repo, ['-option.txt', 'tab\tline\n.txt'])).sort(),
    ['-option.txt', 'tab\tline\n.txt'],
  );
  assert.deepEqual(
    await listUntrackedFiles(repo, ['tracked.txt', 'ignored.log', 'missing']),
    [],
  );
  assert.deepEqual(await listUntrackedFiles(repo, []), []);
});

test('committed files remain visible despite ignore rules in another checkout', async (t) => {
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
  assert.equal(await runGitText(repo, ['status', '--porcelain']), before);
  assert.equal(
    (await runGitText(repo, ['branch', '--show-current'])).trim(),
    'main',
  );
});

test('working reviews exclude ignored untracked files but retain tracked and force-added files', async (t) => {
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
    review.files.map((file) => file.path).sort(),
    ['.gitignore', 'ordinary.md', 'staged.txt', 'tracked.txt'],
  );
  await assert.rejects(
    filePatch(review, 'hidden.txt'),
    /That file is not in this comparison/,
  );
  const committed = await inspect({ repo, base: 'main', head: 'feature' });
  assert.deepEqual(
    committed.files.map((file) => file.path),
    ['tracked.txt'],
  );
  assert.equal(await runGitText(repo, ['status', '--porcelain']), before);
  assert.equal(await runGitText(repo, ['diff', '--cached']), index);
});

test('untracked files respect nested rules, negation, ignored directories and unusual paths', async (t) => {
  const repo = await fixture(t);
  await mkdir(path.join(repo, 'nested'));
  await mkdir(path.join(repo, 'cache/deep'), { recursive: true });
  await writeFile(path.join(repo, '.gitignore'), '*.log\ncache/\n');
  await writeFile(path.join(repo, 'nested/.gitignore'), '!keep.log\n*.tmp\n');
  for (const file of [
    'keep.log',
    'ignored.log',
    'a\tb\nc.tmp',
    'visible\tfile\n.md',
  ]) {
    await writeFile(path.join(repo, 'nested', file), 'data');
  }
  await writeFile(path.join(repo, 'cache/deep/result'), 'generated');
  const review = await inspect({ repo, base: 'main', mode: 'working' });
  assert.deepEqual(
    review.files.map((file) => file.path).sort(),
    [
      '.gitignore',
      'nested/.gitignore',
      'nested/keep.log',
      'nested/visible\tfile\n.md',
    ],
  );
});

test('local and global excludes hide untracked files; symlink ignore files are not followed', async (t) => {
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
  assert.deepEqual(
    review.files.map((file) => file.path).sort(),
    ['.gitignore', 'rules', 'visible.txt'],
  );
});

test('untracked files follow local ignore edits and reappear when force-added', async (t) => {
  const repo = await fixture(t);
  await writeFile(path.join(repo, 'scratch.tmp'), 'data');
  const request = { repo, base: 'main', mode: 'working' as const };
  const scratch = async () =>
    (await inspect(request)).files.find((file) => file.path === 'scratch.tmp');
  assert.equal((await scratch())?.status, '?');
  await writeFile(path.join(repo, '.gitignore'), '*.tmp\n');
  assert.equal(await scratch(), undefined);
  await writeFile(path.join(repo, '.gitignore'), '*.tmp\n!scratch.tmp\n');
  assert.equal((await scratch())?.status, '?');
  await writeFile(path.join(repo, '.gitignore'), '*.tmp\n');
  await runGitText(repo, ['add', '-f', 'scratch.tmp']);
  const staged = await scratch();
  assert.ok(staged);
  assert.equal(staged.status, 'A');
});

test('a staged deletion remains visible when an ignored untracked copy is left on disk', async (t) => {
  const repo = await fixture(t);
  await writeFile(path.join(repo, 'removed.txt'), 'tracked\n');
  await save(repo);
  await runGitText(repo, ['rm', '--cached', 'removed.txt']);
  await writeFile(path.join(repo, '.gitignore'), '*.txt\n');
  const review = await inspect({ repo, base: 'HEAD', mode: 'working' });
  const matches = review.files.filter((file) => file.path === 'removed.txt');
  assert.equal(matches.length, 1);
  assert.equal(matches[0].status, 'D');
  assert.match(
    (await filePatch(review, 'removed.txt')).patch,
    /deleted file mode/,
  );
});
