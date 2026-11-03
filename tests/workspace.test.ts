import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { tmpdir } from 'node:os';
import { runGitText as git, inspect, hash } from '../core/git';
import {
  rememberReview,
  applyWorkspaceAction,
  readWorkspace,
  updateWorkspace,
} from '../core/workspace';
import type { Workspace } from '../core/types';

async function fixture(t: any) {
  const repo = await mkdtemp(path.join(tmpdir(), 'rev-tabs-'));
  t.after(() => rm(repo, { recursive: true, force: true }));
  await git(repo, ['init', '-b', 'main']);
  await git(repo, ['config', 'user.name', 'Test']);
  await git(repo, ['config', 'user.email', 'test@example.invalid']);
  await writeFile(path.join(repo, 'file.ts'), 'one\n');
  await git(repo, ['add', '.']);
  await git(repo, ['commit', '-m', 'base']);
  await git(repo, ['switch', '-c', 'feature']);
  await writeFile(path.join(repo, 'file.ts'), 'two\n');
  await git(repo, ['commit', '-am', 'feature']);
  return { repo, base: 'main', head: 'HEAD' };
}

test('review tabs group by repository, pin HEAD, and retain the selected file on reopen', async (t) => {
  const first = await fixture(t);
  const second = await fixture(t);
  const workspace: Workspace = { projects: [] };
  const review = await inspect(first);
  const tab = rememberReview(workspace, review);
  assert.equal(tab.request.head, 'feature');
  applyWorkspaceAction(workspace, {
    type: 'setFile',
    id: tab.id,
    file: 'file.ts',
  });
  rememberReview(
    workspace,
    await inspect({ ...first, head: 'feature', mode: 'working' }),
  );
  assert.equal(workspace.projects[0].tabs.length, 1);
  assert.equal(workspace.projects[0].tabs[0].file, 'file.ts');
  assert.equal(workspace.projects[0].tabs[0].request.mode, 'working');
  rememberReview(workspace, await inspect(second));
  assert.equal(workspace.projects.length, 2);
  await git(first.repo, ['switch', 'main']);
  const pinned = await inspect({ ...tab.request, mode: 'branch' });
  assert.equal(pinned.headSha, review.headSha);
  assert.equal(pinned.files[0].path, 'file.ts');
  await assert.rejects(inspect(tab.request), /working tree belongs to main/);
});

test('PR tabs have readable identities and closing active tabs preserves other reviews', async (t) => {
  const input = await fixture(t);
  const workspace: Workspace = { projects: [] };
  const first = rememberReview(workspace, await inspect(input));
  const ref = 'refs/rev/owner/project/42/head';
  await git(input.repo, ['update-ref', ref, 'HEAD']);
  const pr = rememberReview(workspace, await inspect({ ...input, head: ref }));
  assert.equal(pr.label, 'PR #42');
  assert.equal(pr.kind, 'pr');
  assert.equal(workspace.activeTabId, pr.id);
  applyWorkspaceAction(workspace, { type: 'closeTab', id: pr.id });
  assert.equal(workspace.activeTabId, first.id);
  assert.equal(workspace.projects[0].tabs.length, 1);
  applyWorkspaceAction(workspace, { type: 'closeTab', id: first.id });
  assert.equal(workspace.activeTabId, undefined);
  assert.equal(workspace.projects.length, 1);
  applyWorkspaceAction(workspace, {
    type: 'removeProject',
    repo: first.request.repo,
  });
  assert.equal(workspace.projects.length, 0);
});

test('opening a saved tab with an older identity migrates it without duplication', async (t) => {
  const request = await fixture(t);
  const review = await inspect(request);
  const workspace: Workspace = { projects: [] };
  const tab = rememberReview(workspace, review);
  tab.id = hash([review.repo, review.base, review.branch].join('\0')).slice(
    0,
    24,
  );
  tab.file = 'file.ts';
  workspace.activeTabId = tab.id;
  const reopened = rememberReview(workspace, review);
  assert.equal(workspace.projects[0].tabs.length, 1);
  assert.equal(reopened.file, 'file.ts');
  assert.equal(workspace.activeTabId, reopened.id);
  assert.equal(reopened.id, review.key);
});
