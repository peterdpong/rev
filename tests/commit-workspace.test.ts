import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import {
  runCommandText as command,
  runGitText as git,
  inspect,
  filePatch,
  listCommits,
  resolveRef,
} from '../core/git';
import type { ReviewRequest, Review, Workspace } from '../core/types';

async function fixture(t: any) {
  const repo = await mkdtemp(path.join(tmpdir(), 'rev-commits-'));
  t.after(() => rm(repo, { recursive: true, force: true }));
  await git(repo, ['init', '-b', 'main']);
  await git(repo, ['config', 'user.name', 'Review Test']);
  await git(repo, ['config', 'user.email', 'review@example.invalid']);
  const commit = async (name: string, content: string, message: string) => {
    await writeFile(path.join(repo, name), content);
    await git(repo, ['add', '--', name]);
    await git(repo, ['commit', '-m', message]);
    return resolveRef(repo, 'HEAD');
  };
  const root = await commit('root.txt', 'root\n', 'Initial commit');
  await git(repo, ['switch', '-c', 'feature']);
  const before = await commit(
    'before.txt',
    'earlier change\n',
    'Before the selection',
  );
  const first = await commit(
    'app.ts',
    'export const answer = 1;\n',
    'Add answer',
  );
  const last = await commit(
    'app.ts',
    'export const answer = 2;\n',
    'Update answer',
  );
  const after = await commit(
    'after.txt',
    'later change\n',
    'After the selection',
  );
  const request: ReviewRequest = { repo, base: 'main', head: 'feature' };
  return { repo, request, root, before, first, last, after, commit };
}

import { HostService } from '../core/host';
test('the host remembers commit scope in the existing branch tab and can restore the full branch', async (t) => {
  const f = await fixture(t);
  const service = new HostService(undefined, path.join(f.repo, '.host-data'));
  t.after(() => service.close());
  const scope = { start: f.first, end: f.last };
  const result = (await service.call('review', {
    ...f.request,
    commitRange: scope,
    remember: true,
  })) as { review: Review; workspace: Workspace };
  assert.deepEqual(
    result.workspace.projects[0].tabs[0].request.commitRange,
    scope,
  );
  const restored = await inspect(result.workspace.projects[0].tabs[0].request);
  assert.equal(restored.scopeKey, result.review.scopeKey);
  const page = (await service.call('commits', { review: f.request })) as {
    commits: { sha: string }[];
  };
  assert.equal(page.commits[0].sha, f.after);
  const full = (await service.call('review', {
    ...f.request,
    remember: true,
  })) as { workspace: Workspace };
  assert.equal(full.workspace.projects[0].tabs.length, 1);
  assert.equal(
    full.workspace.projects[0].tabs[0].request.commitRange,
    undefined,
  );
});

test('detached HEAD tabs retain the branch tip when the selected range ends earlier', async (t) => {
  const f = await fixture(t);
  await git(f.repo, ['switch', '--detach', f.after]);
  const service = new HostService(undefined, path.join(f.repo, '.host-data'));
  t.after(() => service.close());
  const result = (await service.call('review', {
    ...f.request,
    head: 'HEAD',
    commitRange: { start: f.first, end: f.last },
    remember: true,
  })) as { review: Review; workspace: Workspace };
  const tab = result.workspace.projects[0].tabs[0];
  assert.equal(tab.request.head, f.after);
  assert.equal(tab.scopeKey, result.review.scopeKey);
  assert.equal((await inspect(tab.request)).key, result.review.key);
});
