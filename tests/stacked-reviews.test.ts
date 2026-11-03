import { test, type TestContext } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import {
  command,
  git,
  inspect,
  resolveRef,
  importPR,
  filePatch,
} from '../core/git';
import { HostService } from '../core/host';
import type { ReviewRequest } from '../core/types';

async function fixture(t: TestContext) {
  const root = await mkdtemp(path.join(tmpdir(), 'rev-stack-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const remote = path.join(root, 'remote'),
    repo = path.join(root, 'checkout');
  await mkdir(remote);
  await git(remote, ['init', '-b', 'main']);
  await git(remote, ['config', 'user.name', 'Stack Test']);
  await git(remote, ['config', 'user.email', 'stack@example.invalid']);
  const commit = async (name: string) => {
    await writeFile(path.join(remote, name), name + '\n');
    await git(remote, ['add', '--', name]);
    await git(remote, ['commit', '-m', name]);
    return resolveRef(remote, 'HEAD');
  };
  const main = await commit('root.txt');
  await git(remote, ['switch', '-c', 'parent']);
  const parent = await commit('parent.txt');
  await git(remote, ['switch', '-c', 'child']);
  const child = await commit('child.txt');
  await git(remote, ['switch', '-c', 'sibling', 'main']);
  const sibling = await commit('sibling.txt');
  await git(remote, ['switch', 'main']);
  await git(root, ['clone', pathToFileURL(remote).href, repo]);
  return { root, repo, remote, main, parent, child, sibling, commit };
}

test('automatic branch comparisons exclude the parent layer and explicit main shows the full stack', async (t) => {
  const f = await fixture(t);
  await writeFile(path.join(f.repo, 'root.txt'), 'keep my edits\n');
  await writeFile(path.join(f.repo, 'untracked.txt'), 'keep my file\n');
  const status = await git(f.repo, ['status', '--porcelain=v1', '-z']);
  const local = await git(f.repo, [
    'for-each-ref',
    '--format=%(refname)',
    'refs/heads',
  ]);
  const review = await inspect({ repo: f.repo, head: 'child' });
  assert.equal(review.head, 'refs/remotes/origin/child');
  assert.equal(review.base, 'refs/remotes/origin/parent');
  assert.equal(review.mergeBase, f.parent);
  assert.equal(review.commitCount, 1);
  assert.deepEqual(
    review.files.map((file) => file.path),
    ['child.txt'],
  );
  const full = await inspect({ repo: f.repo, head: 'child', base: 'main' });
  assert.deepEqual(
    full.files.map((file) => file.path),
    ['child.txt', 'parent.txt'],
  );
  assert.equal(await git(f.repo, ['status', '--porcelain=v1', '-z']), status);
  assert.equal(
    await git(f.repo, ['for-each-ref', '--format=%(refname)', 'refs/heads']),
    local,
  );
  assert.equal(
    (await git(f.repo, ['branch', '--show-current'])).trim(),
    'main',
  );
});

test('local and remote aliases of the same parent are one candidate; ambiguous branch names do not hide changes', async (t) => {
  const f = await fixture(t);
  await git(f.repo, ['branch', 'parent', 'origin/parent']);
  assert.equal(
    (await inspect({ repo: f.repo, head: 'origin/child' })).base,
    'refs/heads/parent',
  );
  await git(f.repo, ['branch', 'another-parent', 'parent']);
  assert.equal(
    (await inspect({ repo: f.repo, head: 'origin/child' })).mergeBase,
    f.main,
  );
  await git(f.repo, ['branch', '-D', 'parent', 'another-parent']);
  await git(f.repo, ['update-ref', '-d', 'refs/remotes/origin/parent']);
  // A sibling and descendants are not a parent, even if their commits exist locally.
  await git(f.repo, ['update-ref', 'refs/heads/descendant', f.child]);
  assert.equal(
    (await inspect({ repo: f.repo, head: 'origin/child' })).mergeBase,
    f.main,
  );
});

test('typed bases resolve remote-only branches and fetch a missing branch without creating a checkout', async (t) => {
  const f = await fixture(t);
  for (const base of [
    'parent',
    'origin/parent',
    'refs/remotes/origin/parent',
    ' parent ',
  ]) {
    assert.equal(
      (await inspect({ repo: f.repo, head: 'origin/child', base })).mergeBase,
      f.parent,
    );
  }
  await git(f.repo, ['update-ref', '-d', 'refs/remotes/origin/parent']);
  const review = await inspect({
    repo: f.repo,
    head: 'origin/child',
    base: 'parent',
  });
  assert.equal(review.base, 'refs/remotes/origin/parent');
  assert.equal(review.mergeBase, f.parent);
  await assert.rejects(resolveRef(f.repo, 'refs/heads/parent'));
  assert.equal(
    (await git(f.repo, ['branch', '--show-current'])).trim(),
    'main',
  );
  assert.match((await filePatch(review, 'child.txt')).patch, /\+child.txt/);
  await assert.rejects(
    inspect({ repo: f.repo, head: 'child', base: 'does-not-exist' }),
    /Could not fetch base branch "does-not-exist"/,
  );
  await assert.rejects(
    inspect({ repo: f.repo, head: 'child', base: '--help' }),
    /Cannot find base/,
  );
});

test('an unpushed branch still compares against its parent, not its own remote tip', async (t) => {
  const f = await fixture(t);
  await git(f.repo, ['switch', '-c', 'child', 'origin/child']);
  await git(f.repo, ['config', 'user.name', 'Stack Test']);
  await git(f.repo, ['config', 'user.email', 'stack@example.invalid']);
  await writeFile(path.join(f.repo, 'unpushed.txt'), 'extra change\n');
  await git(f.repo, ['add', 'unpushed.txt']);
  await git(f.repo, ['commit', '-m', 'Unpushed change']);
  const review = await inspect({ repo: f.repo });
  assert.equal(review.mergeBase, f.parent);
  assert.equal(review.commitCount, 2);
  assert.deepEqual(
    review.files.map((file) => file.path),
    ['child.txt', 'unpushed.txt'],
  );
});

test('non-origin and ambiguous remotes resolve explicitly instead of guessing', async (t) => {
  const f = await fixture(t);
  await git(f.repo, ['remote', 'rename', 'origin', 'upstream']);
  assert.equal(
    (await inspect({ repo: f.repo, head: 'upstream/child', base: 'parent' }))
      .mergeBase,
    f.parent,
  );
  await git(f.repo, ['remote', 'add', 'other', pathToFileURL(f.remote).href]);
  await git(f.repo, ['fetch', 'other']);
  await assert.rejects(
    inspect({ repo: f.repo, head: 'upstream/child', base: 'parent' }),
    /remote-qualified/,
  );
  assert.equal(
    (
      await inspect({
        repo: f.repo,
        head: 'upstream/child',
        base: 'upstream/parent',
      })
    ).mergeBase,
    f.parent,
  );
});

async function pullFixture(t: TestContext) {
  const f = await fixture(t);
  await git(f.remote, ['update-ref', 'refs/pull/2/head', f.child]);
  await git(f.repo, [
    'config',
    `url.${pathToFileURL(f.remote).href}.insteadOf`,
    'https://github.com/acme/stack.git',
  ]);
  const details = {
    number: 2,
    title: 'Child change',
    body: 'Second layer',
    author: { login: 'reviewer' },
    state: 'OPEN',
    isDraft: false,
    headRefName: 'child',
    baseRefName: 'main',
    updatedAt: '2026-10-27T12:00:00Z',
    url: 'https://github.com/acme/stack/pull/2',
  };
  const pulls = [
    { number: 1, headRefName: 'parent', headRefOid: f.parent },
    { number: 2, headRefName: 'child', headRefOid: f.child },
    { number: 3, headRefName: 'sibling', headRefOid: f.sibling },
  ];
  const bin = path.join(f.root, 'bin');
  await mkdir(bin);
  const detailsFile = path.join(bin, 'details.json'),
    pullsFile = path.join(bin, 'pulls.json');
  await writeFile(detailsFile, JSON.stringify(details));
  await writeFile(pullsFile, JSON.stringify(pulls));
  // Keep test responses in files so both view and list exercise real Git fetches.
  await writeFile(
    path.join(bin, 'gh'),
    `#!/bin/sh\ncase "$2" in\nview) cat "$(dirname "$0")/details.json";;\nlist) cat "$(dirname "$0")/pulls.json";;\n*) exit 1;;\nesac\n`,
    { mode: 0o755 },
  );
  const previous = process.env.PATH;
  process.env.PATH = `${bin}:${previous}`;
  t.after(() => {
    process.env.PATH = previous;
  });
  return { ...f, details, pulls, detailsFile, pullsFile };
}

test('PRs targeting main automatically compare to the nearest parent PR and retain GitHub metadata', async (t) => {
  const f = await pullFixture(t);
  const request = await importPR(f.repo, '2');
  const review = await inspect(request);
  assert.equal(review.mergeBase, f.parent);
  assert.equal(review.commitCount, 1);
  assert.deepEqual(
    review.files.map((file) => file.path),
    ['child.txt'],
  );
  assert.equal(review.pullRequest?.baseRefName, 'main');
  assert.equal(review.pullRequest?.comparisonBaseRefName, 'parent');
  assert.equal(
    (await inspect({ repo: f.repo, head: request.head })).mergeBase,
    f.parent,
  );
  const host = new HostService(undefined, path.join(f.root, 'data'));
  t.after(() => host.close());
  const refreshed = (await host.call('fetch', {
    repo: f.repo,
    review: request,
  })) as { request: ReviewRequest };
  assert.equal((await inspect(refreshed.request)).mergeBase, f.parent);
});

test('explicit PR bases survive import, host refresh, and CLI import', async (t) => {
  const f = await pullFixture(t);
  const host = new HostService(undefined, path.join(f.root, 'data'));
  t.after(() => host.close());
  const request = (await host.call('pr', {
    repo: f.repo,
    value: '2',
    base: 'main',
  })) as ReviewRequest;
  assert.equal(request.base, 'main');
  // A manual comparison must not change the automatic comparison in another tab.
  assert.equal(
    (await inspect({ repo: f.repo, head: request.head })).mergeBase,
    f.parent,
  );
  const review = await inspect(request);
  assert.equal(review.mergeBase, f.main);
  const scope = { start: f.child, end: f.child };
  const refreshed = (await host.call('fetch', {
    repo: f.repo,
    review: { ...request, commitRange: scope },
  })) as { request: ReviewRequest };
  assert.equal(refreshed.request.base, 'main');
  assert.deepEqual(refreshed.request.commitRange, scope);
  assert.equal(
    (await inspect({ ...refreshed.request, commitRange: undefined })).mergeBase,
    f.main,
  );
  const cli = await command(
    process.execPath,
    [
      '--import',
      'tsx',
      'cli/main.ts',
      'pr',
      '2',
      '--repo',
      f.repo,
      '--base',
      'parent',
    ],
    process.cwd(),
  );
  assert.match(cli, /--base refs\/remotes\/origin\/parent /);
  const remoteRequest = await importPR(f.repo, '2', 'origin/parent');
  await git(f.remote, ['switch', 'parent']);
  const advanced = await f.commit('parent-later.txt');
  const remoteRefresh = (await host.call('fetch', {
    repo: f.repo,
    review: remoteRequest,
  })) as { request: ReviewRequest };
  assert.equal((await inspect(remoteRefresh.request)).baseSha, advanced);
});

test('a three-layer PR stack uses the immediate parent, not the bottom layer', async (t) => {
  const f = await pullFixture(t);
  await git(f.remote, ['switch', '-c', 'leaf', 'child']);
  const leaf = await f.commit('leaf.txt');
  await git(f.remote, ['update-ref', 'refs/pull/4/head', leaf]);
  await writeFile(
    f.detailsFile,
    JSON.stringify({
      ...f.details,
      number: 4,
      headRefName: 'leaf',
      url: 'https://github.com/acme/stack/pull/4',
    }),
  );
  await writeFile(
    f.pullsFile,
    JSON.stringify([
      ...f.pulls,
      { number: 4, headRefName: 'leaf', headRefOid: leaf },
    ]),
  );
  const review = await inspect(await importPR(f.repo, '4'));
  assert.equal(review.mergeBase, f.child);
  assert.equal(review.pullRequest?.comparisonBaseRefName, 'child');
  assert.deepEqual(
    review.files.map((file) => file.path),
    ['leaf.txt'],
  );
});

test('GitHub-declared stack bases work without local parent branches, including non-ancestor parents', async (t) => {
  const f = await pullFixture(t);
  await git(f.remote, ['switch', 'parent']);
  await f.commit('parent-later.txt');
  await writeFile(
    f.detailsFile,
    JSON.stringify({ ...f.details, baseRefName: 'parent' }),
  );
  const review = await inspect(await importPR(f.repo, '2'));
  assert.equal(review.mergeBase, f.parent);
  assert.deepEqual(
    review.files.map((file) => file.path),
    ['child.txt'],
  );
  assert.notEqual(review.baseSha, f.parent);
});

test('unverified and ambiguous PR parents keep the declared base', async (t) => {
  const f = await pullFixture(t);
  await writeFile(f.pullsFile, JSON.stringify([f.pulls[1], f.pulls[2]]));
  assert.equal((await inspect(await importPR(f.repo, '2'))).mergeBase, f.main);
  await writeFile(
    f.pullsFile,
    JSON.stringify([
      ...f.pulls,
      { number: 4, headRefName: 'same-tip', headRefOid: f.parent },
    ]),
  );
  assert.equal((await inspect(await importPR(f.repo, '2'))).mergeBase, f.main);
  await writeFile(f.pullsFile, 'unavailable');
  assert.equal((await inspect(await importPR(f.repo, '2'))).mergeBase, f.main);
});
