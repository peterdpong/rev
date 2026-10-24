import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { tmpdir } from 'node:os';
import { git, inspect } from '../src/core/git';
import { rememberReview, applyWorkspaceAction, readWorkspace, updateWorkspace } from '../src/core/workspace';
import { startServer } from '../src/server';
import type { Workspace } from '../src/shared';

async function fixture(t: any) {
  const repo = await mkdtemp(path.join(tmpdir(), 'revue-tabs-'));
  t.after(() => rm(repo, { recursive: true, force: true }));
  await git(repo, ['init', '-b', 'main']);
  await git(repo, ['config', 'user.name', 'Test']); await git(repo, ['config', 'user.email', 'test@example.invalid']);
  await writeFile(path.join(repo, 'file.ts'), 'one\n'); await git(repo, ['add', '.']); await git(repo, ['commit', '-m', 'base']);
  await git(repo, ['switch', '-c', 'feature']); await writeFile(path.join(repo, 'file.ts'), 'two\n'); await git(repo, ['commit', '-am', 'feature']);
  return { repo, base: 'main', head: 'HEAD' };
}

test('review tabs group by repository, pin HEAD, and retain the selected file on reopen', async t => {
  const first = await fixture(t); const second = await fixture(t);
  const workspace: Workspace = { projects: [] };
  const review = await inspect(first); const tab = rememberReview(workspace, review);
  assert.equal(tab.request.head, 'feature');
  applyWorkspaceAction(workspace, { type: 'setFile', id: tab.id, file: 'file.ts' });
  rememberReview(workspace, await inspect({ ...first, head: 'feature', mode: 'working' }));
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

test('PR tabs have readable identities and closing active tabs preserves other reviews', async t => {
  const input = await fixture(t); const workspace: Workspace = { projects: [] };
  const first = rememberReview(workspace, await inspect(input));
  const ref = 'refs/revue/owner/project/42/head'; await git(input.repo, ['update-ref', ref, 'HEAD']);
  const pr = rememberReview(workspace, await inspect({ ...input, head: ref }));
  assert.equal(pr.label, 'PR #42'); assert.equal(pr.kind, 'pr'); assert.equal(workspace.activeTabId, pr.id);
  applyWorkspaceAction(workspace, { type: 'closeTab', id: pr.id });
  assert.equal(workspace.activeTabId, first.id); assert.equal(workspace.projects[0].tabs.length, 1);
  applyWorkspaceAction(workspace, { type: 'closeTab', id: first.id });
  assert.equal(workspace.activeTabId, undefined); assert.equal(workspace.projects.length, 1);
  applyWorkspaceAction(workspace, { type: 'removeProject', repo: first.request.repo });
  assert.equal(workspace.projects.length, 0);
});

test('workspace updates survive concurrent writers and service restarts', async t => {
  const input = await fixture(t); const dir = path.join(input.repo, '.git', 'workspace');
  const review = await inspect(input);
  await Promise.all(Array.from({ length: 8 }, (_, index) => updateWorkspace(dir, workspace => { rememberReview(workspace, { ...review, key: `tab-${index}`, head: `branch-${index}` }); })));
  assert.equal((await readWorkspace(dir)).projects[0].tabs.length, 8);
  await updateWorkspace(dir, workspace => applyWorkspaceAction(workspace, { type: 'collapseProject', repo: review.repo, collapsed: true }));
  assert.equal((await readWorkspace(dir)).projects[0].collapsed, true);
  const service = await startServer({ uiDir: input.repo, dataDir: dir }); t.after(() => service.close());
  const call = async (method: string, params?: unknown) => {
    const response = await fetch(`${service.origin}/api`, { method: 'POST', headers: { 'X-Revue-Token': service.token, 'Content-Type': 'application/json' }, body: JSON.stringify({ method, params }) });
    assert.equal(response.status, 200); return response.json();
  };
  const result = await call('review', { ...input, remember: true });
  assert.equal(result.workspace.activeTabId, review.key);
  await call('workspaceAction', { type: 'setFile', id: review.key, file: 'file.ts' });
  service.close();
  const restarted = await startServer({ uiDir: input.repo, dataDir: dir }); t.after(() => restarted.close());
  const response = await fetch(`${restarted.origin}/api`, { method: 'POST', headers: { 'X-Revue-Token': restarted.token, 'Content-Type': 'application/json' }, body: JSON.stringify({ method: 'init' }) });
  assert.equal(response.status, 200);
  const restored = await response.json();
  assert.equal(restored.workspace.projects[0].tabs.find((t: any) => t.id === review.key).file, 'file.ts');
  assert.equal(restored.workspace.projects[0].collapsed, false);
});
