import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { tmpdir } from 'node:os';
import { runGitText as git, inspect } from '../core/git';
import {
  rememberReview,
  applyWorkspaceAction,
  readWorkspace,
  updateWorkspace,
} from '../core/workspace';
import { HostService } from '../core/host';
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

test('workspace updates survive concurrent writers and service restarts', async (t) => {
  const input = await fixture(t);
  const dir = path.join(input.repo, '.git', 'workspace');
  const review = await inspect(input);
  await Promise.all(
    Array.from({ length: 8 }, (_, index) =>
      updateWorkspace(dir, (workspace) => {
        rememberReview(workspace, {
          ...review,
          key: `tab-${index}`,
          head: `branch-${index}`,
        });
      }),
    ),
  );
  assert.equal((await readWorkspace(dir)).projects[0].tabs.length, 8);
  await updateWorkspace(dir, (workspace) =>
    applyWorkspaceAction(workspace, {
      type: 'collapseProject',
      repo: review.repo,
      collapsed: true,
    }),
  );
  assert.equal((await readWorkspace(dir)).projects[0].collapsed, true);
  const service = new HostService(undefined, dir);
  t.after(() => service.close());
  const call = async (method: string, params?: unknown) =>
    (await service.call(method, params)) as any;
  const result = await call('review', { ...input, remember: true });
  assert.equal(result.workspace.activeTabId, review.key);
  await call('workspaceAction', {
    type: 'setFile',
    id: review.key,
    file: 'file.ts',
  });
  service.close();
  const restarted = new HostService(undefined, dir);
  t.after(() => restarted.close());
  const restored = (await restarted.call('init')) as { workspace: Workspace };
  assert.equal(
    restored.workspace.projects[0].tabs.find((t: any) => t.id === review.key)!
      .file,
    'file.ts',
  );
  assert.equal(restored.workspace.projects[0].collapsed, false);
});
