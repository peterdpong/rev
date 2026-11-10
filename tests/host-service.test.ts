import { test, type TestContext } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { runGitText as git, inspect, resolveRef } from '../core/git';
import type { ReviewRequest } from '../core/types';
import { HostService } from '../core/host';

async function fixture(t: TestContext) {
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

test('the host reviews a commit scope and the full branch', async (t) => {
  const f = await fixture(t);
  const service = new HostService();
  t.after(() => service.close());
  const scope = { start: f.first, end: f.last };
  const result = await service.call('review', {
    ...f.request,
    commitRange: scope,
    patches: false,
  });
  assert.deepEqual(result.review.commitRange, scope);
  assert.equal(result.patches, undefined);
  const restored = await inspect({ ...f.request, commitRange: scope });
  assert.equal(restored.scopeKey, result.review.scopeKey);

  const page = await service.call('commits', { review: f.request });
  assert.equal(page.commits[0].sha, f.after);
  const full = await service.call('review', f.request);
  assert.equal(full.review.commitRange, undefined);
  assert.ok(full.patches && full.patches.length > 0);
});

test('the host reports each checkout and the launch request', async (t) => {
  const f = await fixture(t);
  const service = new HostService(f.request);
  t.after(() => service.close());
  assert.deepEqual(await service.call('init'), { initial: f.request });

  const missing = path.join(f.repo, 'missing');
  assert.deepEqual(
    await service.call('checkouts', { repos: [f.repo, missing] }),
    [{ repo: f.repo, branch: 'feature', sha: f.after }, null],
  );
  await assert.rejects(
    // @ts-expect-error Repository lists must contain only strings.
    service.call('checkouts', { repos: [f.repo, 42] }),
    /Pass a list of repository folders/,
  );
  await git(f.repo, ['switch', '--detach', f.last]);
  assert.deepEqual(await service.call('checkouts', { repos: [f.repo] }), [
    { repo: f.repo, branch: 'detached HEAD', sha: f.last },
  ]);
  // @ts-expect-error Unknown actions are rejected by TypeScript and at runtime.
  await assert.rejects(service.call('workspace'), /Unknown action/);
});

test('the host persists viewed markers and can defer validation to the viewer', async (t) => {
  const f = await fixture(t);
  const service = new HostService();
  t.after(() => service.close());
  const request: ReviewRequest = { ...f.request, mode: 'working' };
  const patch = await service.call('patch', {
    review: request,
    path: 'app.ts',
  });

  const marked = await service.call('viewed', {
    review: request,
    path: 'app.ts',
    digest: patch.digest,
    viewed: true,
    preview: true,
  });
  assert.equal(marked.viewed['app.ts'], patch.digest);

  await writeFile(f.repo + '/app.ts', 'changed since review\n');
  const restarted = new HostService();
  t.after(() => restarted.close());
  const preview = await restarted.call('review', {
    ...request,
    patches: false,
    preview: true,
  });
  assert.equal(preview.state.viewed['app.ts'], patch.digest);

  const validated = await restarted.call('review', {
    ...request,
    patches: false,
  });
  assert.deepEqual(validated.state.viewed, {});
  await assert.rejects(
    restarted.call('viewed', {
      review: request,
      path: 'app.ts',
      digest: patch.digest,
      viewed: true,
    }),
    /This file changed/,
  );
});
