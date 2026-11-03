import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  mkdtemp,
  mkdir,
  readFile,
  rm,
  stat,
  writeFile,
} from 'node:fs/promises';
import path from 'node:path';
import { tmpdir } from 'node:os';
import { git, inspect } from '../core/git';
import { HostService } from '../core/host';
import { rememberReview, updateWorkspace } from '../core/workspace';

async function fixture(t: any) {
  const root = await mkdtemp(path.join(tmpdir(), 'rev-projects-'));
  const repo = path.join(root, 'repo');
  await mkdir(repo);
  const data = path.join(root, 'data');
  await git(repo, ['init', '-b', 'main']);
  await git(repo, ['config', 'user.name', 'Test']);
  await git(repo, ['config', 'user.email', 'test@example.invalid']);
  await writeFile(path.join(repo, 'file.ts'), 'one\n');
  await git(repo, ['add', '.']);
  await git(repo, ['commit', '-m', 'base']);
  await git(repo, ['switch', '-c', 'feature']);
  await writeFile(path.join(repo, 'file.ts'), 'two\n');
  await git(repo, ['commit', '-am', 'feature']);
  const service = new HostService(undefined, data);
  const call = (method: string, params: unknown = {}) =>
    service.call(method, params) as Promise<any>;
  t.after(async () => {
    await service.close();
    await rm(root, { recursive: true, force: true });
  });
  const current = (input = {}) =>
    call('review', { repo, current: true, remember: true, ...input });
  return { repo, root, data, call, current };
}

test('adding a folder follows its checkout while explicit reviews and feedback stay pinned', async (t) => {
  const { repo, call, current } = await fixture(t);
  const opened = await current();
  const tab = opened.workspace.projects[0].tabs[0];
  assert.equal(tab.kind, 'current');
  assert.equal(tab.label, 'feature');
  assert.equal(tab.request.head, 'HEAD');
  assert.equal(tab.request.base, undefined);
  assert.notEqual(tab.id, opened.review.key);
  await call('workspaceAction', {
    type: 'setFile',
    id: tab.id,
    file: 'file.ts',
  });
  const again = await current();
  assert.equal(again.workspace.projects[0].tabs.length, 1);
  assert.equal(again.workspace.activeTabId, tab.id);
  assert.equal(again.workspace.projects[0].tabs[0].file, 'file.ts');
  const pinned = { ...opened.review, head: opened.review.branch };
  const patch = await call('patch', { review: pinned, path: 'file.ts' });
  await call('comment', {
    review: pinned,
    selection: { path: 'file.ts', side: 'additions', start: 1, end: 1 },
    body: 'Keep this feedback on feature.',
    digest: patch.digest,
  });
  const explicit = await call('review', { ...pinned, remember: true });
  assert.equal(explicit.workspace.projects[0].tabs.length, 2);
  assert.equal(explicit.workspace.activeTabId, opened.review.key);
  await current({
    commitRange: { start: opened.review.headSha, end: opened.review.headSha },
  });
  await git(repo, ['switch', 'main']);
  const changed = await call('workspace');
  const updated = changed.projects[0].tabs[0];
  assert.equal(updated.id, tab.id);
  assert.equal(updated.label, 'main');
  assert.equal(updated.request.commitRange, undefined);
  assert.equal(updated.file, undefined);
  assert.equal(updated.scopeKey, undefined);
  const followed = await current();
  assert.equal(followed.review.branch, 'main');
  assert.equal(followed.state.comments.length, 0);
  assert.equal(
    (await call('review', pinned)).state.comments[0].body,
    'Keep this feedback on feature.',
  );
  const pinnedTab = changed.projects[0].tabs[1];
  assert.equal(pinnedTab.request.head, 'feature');
  assert.equal(pinnedTab.id, opened.review.key);
  await call('review', { ...pinnedTab.request, remember: true });
  await git(repo, ['switch', '-c', 'same-commit']);
  const sibling = await call('workspace');
  assert.equal(sibling.projects[0].tabs[0].label, 'same-commit');
  assert.equal(
    sibling.activeTabId,
    pinnedTab.id,
    'checkout polling must not navigate pinned reviews',
  );
  const closed = await call('workspaceAction', {
    type: 'closeTab',
    id: pinnedTab.id,
  });
  assert.equal(closed.activeTabId, tab.id);
  const retained = await call('workspaceAction', {
    type: 'closeTab',
    id: tab.id,
  });
  assert.equal(retained.projects[0].tabs.length, 1);
  await git(repo, ['switch', '--detach', 'main']);
  const detached = await call('workspace');
  assert.equal(detached.projects[0].tabs[0].id, tab.id);
  assert.match(detached.projects[0].tabs[0].label, /^Detached [a-f0-9]{7}$/);
});

test('saved projects gain Current once, retaining navigation and avoiding unchanged writes', async (t) => {
  const { repo, data, call } = await fixture(t);
  const review = await inspect({ repo });
  await updateWorkspace(data, (workspace) => {
    const pinned = rememberReview(workspace, review);
    pinned.file = 'file.ts';
    workspace.projects[0].collapsed = true;
  });
  const migrated = (await call('init')).workspace;
  assert.equal(migrated.projects[0].tabs.length, 2);
  assert.equal(migrated.projects[0].tabs[0].kind, 'current');
  assert.equal(migrated.projects[0].tabs[1].file, 'file.ts');
  assert.equal(migrated.projects[0].collapsed, true);
  assert.equal(migrated.activeTabId, review.key);
  const before = await stat(path.join(data, 'workspace.json'));
  assert.deepEqual(await call('workspace'), migrated);
  assert.equal(
    (await stat(path.join(data, 'workspace.json'))).mtimeMs,
    before.mtimeMs,
  );
  await call('workspaceAction', { type: 'removeProject', repo: review.repo });
  assert.deepEqual((await call('workspace')).projects, []);
});

test('folder validation and canonicalization do not leave duplicate or invalid projects', async (t) => {
  const { repo, root, call, current } = await fixture(t);
  await assert.rejects(current({ repo: root }), /not a git repository/i);
  assert.deepEqual((await call('workspace')).projects, []);
  const first = await current();
  await mkdir(path.join(repo, 'nested'));
  const nested = await current({ repo: path.join(repo, 'nested') });
  assert.equal(nested.workspace.projects.length, 1);
  assert.equal(nested.workspace.activeTabId, first.workspace.activeTabId);
  await assert.rejects(current({ head: 'main' }), /must follow HEAD/);
});

test('branch lists and fetch stay scoped to their folder and leave the checkout intact', async (t) => {
  const first = await fixture(t);
  const second = await fixture(t);
  await git(second.repo, ['branch', 'remote-only']);
  assert.ok(
    !(await first.call('branches', { repo: first.repo })).includes(
      'remote-only',
    ),
  );
  assert.ok(
    (await first.call('branches', { repo: second.repo })).includes(
      'remote-only',
    ),
  );
  await git(first.repo, ['remote', 'add', 'origin', second.repo]);
  const before = await git(first.repo, ['rev-parse', 'HEAD']);
  await writeFile(path.join(first.repo, 'file.ts'), 'local edits\n');
  await first.call('fetch', { repo: first.repo });
  await git(first.repo, [
    'symbolic-ref',
    'refs/remotes/origin/HEAD',
    'refs/remotes/origin/main',
  ]);
  const branches = await first.call('branches', { repo: first.repo });
  assert.ok(branches.includes('origin/remote-only'));
  assert.ok(
    !branches.includes('origin') && !branches.includes('origin/HEAD'),
    'remote HEAD aliases are not branch choices',
  );
  assert.equal(await git(first.repo, ['rev-parse', 'HEAD']), before);
  assert.equal(
    (await git(first.repo, ['branch', '--show-current'])).trim(),
    'feature',
  );
  assert.equal(
    await readFile(path.join(first.repo, 'file.ts'), 'utf8'),
    'local edits\n',
  );
});
