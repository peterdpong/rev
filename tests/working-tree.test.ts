import { test, type TestContext } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, realpath, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { HostService } from '../core/host';
import { runGitText as git } from '../core/git';

async function fixture(t: TestContext) {
  const repo = await realpath(
    await mkdtemp(path.join(tmpdir(), 'rev-working-')),
  );
  t.after(() => rm(repo, { recursive: true, force: true }));
  await git(repo, ['init', '-b', 'main']);
  await git(repo, ['config', 'user.name', 'Review Test']);
  await git(repo, ['config', 'user.email', 'review@example.invalid']);
  await writeFile(path.join(repo, 'file.txt'), 'base\n');
  await writeFile(path.join(repo, '.gitignore'), 'ignored.txt\n');
  await git(repo, ['add', '.']);
  await git(repo, ['commit', '-m', 'Base']);
  await git(repo, ['switch', '-c', 'feature']);
  const service = new HostService();
  t.after(() => service.close());
  const poll = async () => {
    const [state] = await service.call('workingTrees', {
      repos: [repo],
      activeRepo: repo,
    });
    assert.ok(state);
    return state;
  };
  return { repo, service, poll };
}

test('working-tree revisions detect repeated edits, new files, staging and deletions', async (t) => {
  const { repo, poll } = await fixture(t);
  const initial = await poll();
  assert.deepEqual(
    await poll(),
    initial,
    'unchanged checkouts do not trigger refreshes',
  );
  await writeFile(path.join(repo, 'file.txt'), 'edit\n');
  const edited = await poll();
  assert.notEqual(edited.revision, initial.revision);
  await writeFile(path.join(repo, 'file.txt'), 'more\n');
  const editedAgain = await poll();
  assert.notEqual(
    editedAgain.revision,
    edited.revision,
    'same status and length, different content',
  );
  await git(repo, ['add', 'file.txt']);
  const staged = await poll();
  assert.notEqual(staged.revision, editedAgain.revision);
  const name = 'new file\nwith newline.txt';
  await writeFile(path.join(repo, name), 'one\n');
  const added = await poll();
  assert.notEqual(added.revision, staged.revision);
  await writeFile(path.join(repo, name), 'two\n');
  const changedUntracked = await poll();
  assert.notEqual(changedUntracked.revision, added.revision);
  await rm(path.join(repo, name));
  assert.notEqual((await poll()).revision, changedUntracked.revision);
  await git(repo, ['mv', 'file.txt', 'renamed file.txt']);
  const renamed = await poll();
  await writeFile(path.join(repo, 'renamed file.txt'), 'next\n');
  assert.notEqual((await poll()).revision, renamed.revision);
  await rm(path.join(repo, 'renamed file.txt'));
  assert.ok(await poll(), 'deleted tracked files still produce a revision');
});

test('working-tree refreshes exclude ignored files and follow ignore rule changes', async (t) => {
  const { repo, service, poll } = await fixture(t);
  const initial = await poll();
  await writeFile(path.join(repo, 'ignored.txt'), 'local only\n');
  assert.equal((await poll()).revision, initial.revision);
  await writeFile(path.join(repo, 'ignored.txt'), 'changed ignored content\n');
  assert.equal((await poll()).revision, initial.revision);
  const ignored = await service.call('workingReview', {
    repo,
    path: 'ignored.txt',
  });
  assert.deepEqual(ignored.review.files, []);
  assert.equal(ignored.path, undefined);
  assert.equal(ignored.patch, undefined);
  await rm(path.join(repo, 'ignored.txt'));
  assert.equal((await poll()).revision, initial.revision);

  await writeFile(path.join(repo, 'scratch.tmp'), 'visible\n');
  const visible = await poll();
  assert.notEqual(visible.revision, initial.revision);
  await writeFile(path.join(repo, '.git/info/exclude'), '*.tmp\n');
  const excluded = await poll();
  assert.notEqual(excluded.revision, visible.revision);
  assert.deepEqual(
    (await service.call('workingReview', { repo })).review.files,
    [],
  );
  await writeFile(path.join(repo, 'scratch.tmp'), 'changed while ignored\n');
  assert.equal((await poll()).revision, excluded.revision);
  await writeFile(path.join(repo, '.git/info/exclude'), '');
  assert.notEqual((await poll()).revision, excluded.revision);
  const restored = await service.call('workingReview', {
    repo,
    path: 'scratch.tmp',
  });
  assert.equal(restored.path, 'scratch.tmp');
  assert.match(restored.patch!.patch, /\+changed while ignored/);
});

test('working-tree snapshots follow checkout changes and tolerate removed repositories', async (t) => {
  const { repo, service, poll } = await fixture(t);
  const initial = await poll();
  await git(repo, ['switch', '-c', 'another-branch']);
  const switched = await poll();
  assert.equal(switched.branch, 'another-branch');
  assert.equal(switched.sha, initial.sha);
  assert.notEqual(switched.revision, initial.revision);
  await writeFile(path.join(repo, 'file.txt'), 'commit\n');
  await git(repo, ['commit', '-am', 'Local commit']);
  assert.notEqual((await poll()).sha, switched.sha);
  await git(repo, ['switch', '--detach', 'HEAD']);
  assert.equal((await poll()).branch, 'detached HEAD');
  const states = await service.call('workingTrees', {
    repos: [path.join(repo, 'missing'), repo],
  });
  assert.equal(states[0], null);
  assert.equal(states[1]?.branch, 'detached HEAD');
  const before = states[1]?.revision;
  await writeFile(path.join(repo, 'file.txt'), 'disk edit\n');
  assert.equal(
    (await service.call('workingTrees', { repos: [repo] }))[0]?.revision,
    before,
    'inactive repos only need checkout metadata until opened',
  );
});

test('background working reviews refresh the selected patch while branch reviews stay committed', async (t) => {
  const { repo, service } = await fixture(t);
  await writeFile(path.join(repo, 'file.txt'), 'committed\n');
  await git(repo, ['commit', '-am', 'Feature']);
  const initial = await service.call('workingReview', {
    repo,
    path: 'file.txt',
  });
  assert.equal(initial.review.mode, 'working');
  assert.equal(initial.review.head, 'HEAD');
  assert.equal(initial.path, 'file.txt');
  assert.match(initial.patch!.patch, /\+committed/);
  await service.call('viewed', {
    review: { repo, mode: 'working', head: 'HEAD' },
    path: 'file.txt',
    viewed: true,
    digest: initial.patch!.digest,
    preview: true,
  });
  await writeFile(path.join(repo, 'file.txt'), 'local edits\n');
  await writeFile(path.join(repo, 'new.txt'), 'new file\n');
  const updated = await service.call('workingReview', {
    repo,
    path: initial.path,
  });
  assert.equal(updated.path, initial.path);
  assert.match(updated.patch!.patch, /\+local edits/);
  assert.notEqual(updated.patch!.digest, initial.patch!.digest);
  assert.equal(updated.state.viewed['file.txt'], undefined);
  assert.ok(updated.review.files.some((file) => file.path === 'new.txt'));
  const committed = await service.call('review', {
    repo,
    head: 'feature',
    mode: 'branch',
    patches: true,
  });
  assert.match(committed.patches![0].patch, /\+committed/);
  assert.doesNotMatch(committed.patches![0].patch, /local edits/);
  assert.ok(!committed.review.files.some((file) => file.path === 'new.txt'));
  await rm(path.join(repo, 'new.txt'));
  assert.equal(
    (await service.call('workingReview', { repo, path: 'new.txt' })).path,
    'file.txt',
  );
  await git(repo, ['reset', '--hard', 'HEAD']);
  await git(repo, ['switch', 'main']);
  const clean = await service.call('workingReview', { repo, path: 'file.txt' });
  assert.equal(clean.review.branch, 'main');
  assert.equal(clean.path, undefined);
  assert.equal(clean.patch, undefined);
  assert.deepEqual(clean.review.files, []);
});
