import { test, type TestContext } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
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

test('viewer reviews recover stale commit selections after a squash, including missing objects', async (t) => {
  const f = await fixture(t);
  const service = new HostService();
  t.after(() => service.close());
  const request = {
    ...f.request,
    commitRange: { start: f.first, end: f.last },
    recoverStaleRange: true,
    patches: false,
  };
  const selected = await service.call('review', request);
  assert.deepEqual(selected.review.commitRange, request.commitRange);
  assert.equal(selected.notice, undefined);

  await git(f.repo, ['reset', '--soft', 'main']);
  await git(f.repo, ['commit', '-m', 'Squash feature changes']);
  const squashed = await resolveRef(f.repo, 'HEAD');
  await assert.rejects(
    service.call('review', { ...request, recoverStaleRange: false }),
    /not in this branch’s first-parent history/,
  );
  const recovered = await service.call('review', request);
  assert.equal(recovered.review.head, 'feature');
  assert.equal(recovered.review.headSha, squashed);
  assert.equal(recovered.review.commitRange, undefined);
  assert.equal(recovered.review.scopeKey, recovered.review.key);
  assert.match(recovered.notice!, /Showing all commits/);

  const clone = await mkdtemp(path.join(tmpdir(), 'rev-squashed-clone-'));
  t.after(() => rm(clone, { recursive: true, force: true }));
  await git(f.repo, [
    'clone', '--no-local', '--single-branch', '--branch', 'feature', f.repo, clone,
  ]);
  await assert.rejects(resolveRef(clone, f.first));
  const restored = await service.call('review', {
    ...request,
    repo: clone,
    base: f.root,
  });
  assert.equal(restored.review.headSha, squashed);
  assert.equal(restored.review.commitRange, undefined);
  assert.match(restored.notice!, /Showing all commits/);
  await assert.rejects(
    service.call('review', {
      ...request,
      repo: clone,
      base: f.root,
      head: f.after,
    }),
    /Cannot find head/,
    'A missing commit-only review must not silently switch to another branch',
  );
});

test('stale-range recovery preserves unrelated validation errors', async (t) => {
  const f = await fixture(t);
  const service = new HostService();
  t.after(() => service.close());
  const request = { ...f.request, recoverStaleRange: true, patches: false };
  await assert.rejects(
    service.call('review', {
      ...request,
      commitRange: { start: f.last, end: f.first },
    }),
    /start commit must be older/,
  );
  await assert.rejects(
    service.call('review', {
      ...request,
      head: 'missing-branch',
      commitRange: { start: f.first, end: f.last },
    }),
    /Cannot find head/,
  );
  await assert.rejects(
    service.call('review', {
      ...request,
      mode: 'working',
      commitRange: { start: f.first, end: f.last },
    }),
    /Commit ranges are only supported in committed mode/,
  );
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

test('the branch picker can load only local branches from the requested repo', async (t) => {
  const f = await fixture(t);
  const other = await fixture(t);
  const service = new HostService(f.request);
  t.after(() => service.close());
  await git(f.repo, ['branch', 'origin/local-feature']);
  await git(f.repo, ['tag', 'feature']); // Ambiguous short ref names must still display cleanly.
  await git(f.repo, ['update-ref', 'refs/remotes/origin/remote-only', f.after]);
  await git(f.repo, ['update-ref', 'refs/rev/owner/repo/42/head', f.after]);
  await git(other.repo, ['branch', 'only-in-other-repo']);
  assert.deepEqual(
    await service.call('branches', { repo: f.repo, localOnly: true }),
    ['feature', 'main', 'origin/local-feature'],
  );
  assert.deepEqual(
    await service.call('branches', { repo: other.repo, localOnly: true }),
    ['feature', 'main', 'only-in-other-repo'],
  );
  assert.ok(
    (await service.call('branches', { repo: f.repo })).includes(
      'origin/remote-only',
    ),
  );
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

test('snapshot patches stay pinned to their committed comparison and enforce repository/path identity', async (t) => {
  const f = await fixture(t);
  const other = await fixture(t);
  const service = new HostService();
  t.after(() => service.close());
  const opened = await service.call('review', { ...f.request, patches: false });
  const params = {
    review: f.request,
    snapshotId: opened.snapshotId,
    path: 'app.ts',
  };
  await f.commit('app.ts', 'export const answer = 3;\n', 'New commit');
  const patch = await service.call('patch', params);
  assert.match(patch.patch, /answer = 2/);
  assert.doesNotMatch(patch.patch, /answer = 3/);
  await assert.rejects(
    service.call('patch', { ...params, review: other.request }),
    /Invalid review repository/,
  );
  await assert.rejects(
    service.call('patch', { ...params, path: '../outside' }),
    /not in this comparison/,
  );
  await assert.rejects(
    service.call('patch', {
      ...params,
      headSha: f.root,
      mergeBase: opened.review.mergeBase,
    }),
    /comparison changed/,
  );
  await assert.rejects(
    service.call('patch', { ...params, snapshotId: 'unknown' }),
    /review expired/,
  );
  const refreshed = await service.call('review', {
    ...f.request,
    patches: false,
  });
  assert.match(
    (
      await service.call('patch', {
        ...params,
        snapshotId: refreshed.snapshotId,
      })
    ).patch,
    /answer = 3/,
  );
});

test('working snapshot patches load fresh contents using only path-scoped scans', async (t) => {
  const f = await fixture(t);
  const service = new HostService();
  t.after(() => service.close());
  const request: ReviewRequest = {
    repo: f.repo,
    mode: 'working',
    head: 'HEAD',
  };
  await writeFile(path.join(f.repo, 'scratch.txt'), 'initial untracked\n');
  const opened = await service.call('workingReview', { repo: f.repo });
  const params = {
    review: request,
    snapshotId: opened.snapshotId,
    path: 'app.ts',
  };
  await writeFile(path.join(f.repo, 'app.ts'), 'fresh tracked content\n');
  const traceFile = path.join(f.repo, '.git/patch-trace');
  const previousTrace = process.env.GIT_TRACE;
  process.env.GIT_TRACE = traceFile;
  try {
    assert.match(
      (await service.call('patch', params)).patch,
      /\+fresh tracked content/,
    );
  } finally {
    if (previousTrace === undefined) delete process.env.GIT_TRACE;
    else process.env.GIT_TRACE = previousTrace;
  }
  const commands = (await readFile(traceFile, 'utf8'))
    .split('\n')
    .filter((line) => line.includes('built-in: git'));
  assert.ok(commands.length > 0);
  assert.ok(
    commands.every(
      (line) => !/\b(status|for-each-ref|merge-base|log)\b/.test(line),
    ),
    commands.join('\n'),
  );
  const scans = commands.filter((line) => /\b(diff|ls-files)\b/.test(line));
  assert.ok(scans.length >= 2);
  assert.ok(
    scans.every((line) => line.endsWith(' -- app.ts')),
    scans.join('\n'),
  );

  const scratch = { ...params, path: 'scratch.txt' };
  await writeFile(
    path.join(f.repo, 'scratch.txt'),
    'fresh untracked content\n',
  );
  assert.match(
    (await service.call('patch', scratch)).patch,
    /\+fresh untracked content/,
  );
  await git(f.repo, ['add', 'scratch.txt']);
  assert.match((await service.call('patch', scratch)).patch, /new file mode/);
  await git(f.repo, ['rm', '--cached', 'scratch.txt']);
  await writeFile(path.join(f.repo, '.gitignore'), 'scratch.txt\n');
  await assert.rejects(
    service.call('patch', scratch),
    /not in this comparison/,
  );
  await git(f.repo, ['switch', '-c', 'same-commit-new-branch']);
  await assert.rejects(service.call('patch', params), /checkout changed/);
});

test('working snapshot patches preserve renames and staged deletions with untracked copies', async (t) => {
  const f = await fixture(t);
  const service = new HostService();
  t.after(() => service.close());
  await git(f.repo, ['mv', 'root.txt', 'renamed.txt']);
  const request: ReviewRequest = {
    repo: f.repo,
    mode: 'working',
    head: 'HEAD',
  };
  const opened = await service.call('workingReview', {
    repo: f.repo,
    path: 'renamed.txt',
  });
  const params = {
    review: request,
    snapshotId: opened.snapshotId,
    path: 'renamed.txt',
  };
  assert.match(
    (await service.call('patch', params)).patch,
    /rename from root.txt/,
  );
  await git(f.repo, ['reset', '--hard', 'HEAD']);
  await git(f.repo, ['rm', '--cached', 'root.txt']);
  const deleted = await service.call('workingReview', {
    repo: f.repo,
    path: 'root.txt',
  });
  assert.match(
    (
      await service.call('patch', {
        ...params,
        snapshotId: deleted.snapshotId,
        path: 'root.txt',
      })
    ).patch,
    /deleted file mode/,
  );
});
