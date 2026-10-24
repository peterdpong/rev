import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { command, git, inspect, filePatch, listCommits, resolveRef } from '../src/core/git';
import { addComment, readState, mutateState } from '../src/core/store';
import { preparePrompt } from '../src/core/agents';
import { HostService } from '../src/core/host';
import type { ReviewRequest, Review, Workspace } from '../src/shared';

async function fixture(t: any) {
  const repo = await mkdtemp(path.join(tmpdir(), 'patch-commits-'));
  t.after(() => rm(repo, { recursive: true, force: true }));
  await git(repo, ['init', '-b', 'main']);
  await git(repo, ['config', 'user.name', 'Review Test']);
  await git(repo, ['config', 'user.email', 'review@example.invalid']);
  const commit = async (name: string, content: string, message: string) => {
    await writeFile(path.join(repo, name), content);
    await git(repo, ['add', '--', name]); await git(repo, ['commit', '-m', message]);
    return resolveRef(repo, 'HEAD');
  };
  const root = await commit('root.txt', 'root\n', 'Initial commit');
  await git(repo, ['switch', '-c', 'feature']);
  const before = await commit('before.txt', 'earlier change\n', 'Before the selection');
  const first = await commit('app.ts', 'export const answer = 1;\n', 'Add answer');
  const last = await commit('app.ts', 'export const answer = 2;\n', 'Update answer');
  const after = await commit('after.txt', 'later change\n', 'After the selection');
  const request: ReviewRequest = { repo, base: 'main', head: 'feature' };
  return { repo, request, root, before, first, last, after, commit };
}

test('single commits and inclusive ranges exclude changes outside the selection without touching the checkout', async t => {
  const f = await fixture(t);
  await writeFile(path.join(f.repo, 'dirty.txt'), 'keep my untracked work\n');
  const status = await git(f.repo, ['status', '--porcelain=v1', '-z']);
  const branch = await inspect(f.request);
  const review = await inspect({ ...f.request, commitRange: { start: f.first, end: f.last } });
  assert.equal(review.key, branch.key);
  assert.notEqual(review.scopeKey, branch.scopeKey);
  assert.equal(review.commitCount, 2);
  assert.equal(review.mergeBase, f.before);
  assert.equal(review.headSha, f.last);
  assert.equal(review.branchHeadSha, f.after);
  assert.deepEqual(review.files.map(f => f.path), ['app.ts']);
  assert.match((await filePatch(review, 'app.ts')).patch, /\+export const answer = 2;/);
  const single = await inspect({ ...f.request, commitRange: { start: f.last, end: f.last } });
  const patch = (await filePatch(single, 'app.ts')).patch;
  assert.equal(single.commitCount, 1);
  assert.match(patch, /-export const answer = 1;/);
  assert.match(patch, /\+export const answer = 2;/);
  assert.equal(await resolveRef(f.repo, 'HEAD'), f.after);
  assert.equal(await git(f.repo, ['status', '--porcelain=v1', '-z']), status);
});

test('root and empty commits are valid comparisons', async t => {
  const f = await fixture(t);
  const root = await inspect({ ...f.request, commitRange: { start: f.root, end: f.root } });
  assert.deepEqual(root.files.map(f => [f.path, f.status]), [['root.txt', 'A']]);
  assert.match((await filePatch(root, 'root.txt')).patch, /\+root/);
  await git(f.repo, ['commit', '--allow-empty', '-m', 'Empty commit']);
  const empty = await inspect({ ...f.request, commitRange: { start: 'HEAD', end: 'HEAD' } });
  assert.equal(empty.commitCount, 1);
  assert.deepEqual(empty.files, []);
});

test('a shallow boundary is not mistaken for a root commit', async t => {
  const f = await fixture(t);
  const shallow = await mkdtemp(path.join(tmpdir(), 'patch-shallow-'));
  t.after(() => rm(shallow, { recursive: true, force: true }));
  await git(f.repo, ['clone', '--depth=1', '--branch=feature', pathToFileURL(f.repo).href, shallow]);
  await assert.rejects(inspect({ repo: shallow, base: 'HEAD', head: 'HEAD', commitRange: { start: 'HEAD', end: 'HEAD' } }), /Fetch more history/);
});

test('merge commits compare to the first parent and history excludes side-parent duplicates', async t => {
  const f = await fixture(t);
  await git(f.repo, ['switch', '-c', 'side']);
  const side = await f.commit('side.txt', 'merged change\n', 'Side change');
  await git(f.repo, ['switch', 'feature']);
  const parent = await f.commit('feature.txt', 'mainline change\n', 'Mainline change');
  await git(f.repo, ['merge', '--no-ff', 'side', '-m', 'Merge side']);
  const head = await resolveRef(f.repo, 'HEAD');
  const review = await inspect({ ...f.request, commitRange: { start: head, end: head } });
  assert.equal(review.mergeBase, parent);
  assert.deepEqual(review.files.map(f => f.path), ['side.txt']);
  const page = await listCommits(f.request);
  assert.equal(page.commits[0].sha, head);
  assert.equal(page.commits[0].parents.length, 2);
  assert.ok(!page.commits.some(c => c.sha === side));
  assert.equal(page.commits[0].author, 'Review Test');
  assert.match(page.commits[0].date, /^\d{4}-/);
});

test('range validation rejects reversed, foreign, invalid, and working-tree selections', async t => {
  const f = await fixture(t);
  await assert.rejects(inspect({ ...f.request, commitRange: { start: f.last, end: f.first } }), /older/);
  await assert.rejects(inspect({ ...f.request, commitRange: { start: '--help', end: f.last } }));
  await assert.rejects(inspect({ ...f.request, mode: 'working', commitRange: { start: f.first, end: f.last } }), /committed mode/);
  await git(f.repo, ['switch', '-c', 'other', 'main']);
  const other = await f.commit('other.txt', 'other\n', 'Other branch');
  await assert.rejects(inspect({ ...f.request, commitRange: { start: other, end: other } }), /first-parent history/);
});

test('feedback and viewed markers stay with the selected range; agent prompts use the selected endpoints', async t => {
  const f = await fixture(t);
  const request = { ...f.request, commitRange: { start: f.first, end: f.last } };
  const review = await inspect(request);
  const selection = { path: 'app.ts', side: 'additions' as const, start: 1, end: 1 };
  const patch = await filePatch(review, 'app.ts');
  await addComment(review, selection, 'Explain this value', patch.digest);
  await mutateState(review, state => { state.viewed['app.ts'] = patch.digest; });
  assert.equal((await readState(review)).comments.length, 1);
  const full = await readState(await inspect(f.request));
  assert.equal(full.comments.length, 0); assert.deepEqual(full.viewed, {});
  assert.equal((await readState(await inspect({ ...f.request, commitRange: { start: f.last, end: f.last } }))).comments.length, 0);
  const prompt = await preparePrompt({ review: request, provider: 'codex', mode: 'ask', question: 'Explain the selected commits' });
  assert.ok(prompt.prompt.includes(`Commit range (inclusive): ${f.first} → ${f.last}`));
  assert.doesNotMatch(prompt.prompt, /before\.txt|after\.txt/);
  await assert.rejects(preparePrompt({ review: request, provider: 'codex', mode: 'apply' }), /selected end commit/);
});

test('the host remembers commit scope in the existing branch tab and can restore the full branch', async t => {
  const f = await fixture(t);
  const service = new HostService(undefined, path.join(f.repo, '.host-data'));
  t.after(() => service.close());
  const scope = { start: f.first, end: f.last };
  const result = await service.call('review', { ...f.request, commitRange: scope, remember: true }) as { review: Review; workspace: Workspace };
  assert.deepEqual(result.workspace.projects[0].tabs[0].request.commitRange, scope);
  const restored = await inspect(result.workspace.projects[0].tabs[0].request);
  assert.equal(restored.scopeKey, result.review.scopeKey);
  const page = await service.call('commits', { review: f.request }) as { commits: { sha: string }[] };
  assert.equal(page.commits[0].sha, f.after);
  const full = await service.call('review', { ...f.request, remember: true }) as { workspace: Workspace };
  assert.equal(full.workspace.projects[0].tabs.length, 1);
  assert.equal(full.workspace.projects[0].tabs[0].request.commitRange, undefined);
});

test('CLI supports a single commit, a range, and history listing', async t => {
  const f = await fixture(t);
  const cli = (...args: string[]) => command(process.execPath, ['--import', 'tsx', 'src/cli.ts', ...args, '--repo', f.repo], process.cwd());
  const single = await cli('diff', '--commit', f.last, '--no-pager');
  assert.match(single, /-export const answer = 1;/);
  const range = await cli('diff', '--from', f.first, '--to', f.last, '--no-pager');
  assert.match(range, /2 commits, inclusive/);
  assert.doesNotMatch(range, /before\.txt|after\.txt/);
  assert.match(await cli('commits'), /Update answer/);
  await assert.rejects(cli('diff', '--from', f.first), /together/);
});

test('detached HEAD tabs retain the branch tip when the selected range ends earlier', async t => {
  const f = await fixture(t);
  await git(f.repo, ['switch', '--detach', f.after]);
  const service = new HostService(undefined, path.join(f.repo, '.host-data'));
  t.after(() => service.close());
  const result = await service.call('review', { ...f.request, head: 'HEAD', commitRange: { start: f.first, end: f.last }, remember: true }) as { review: Review; workspace: Workspace };
  const tab = result.workspace.projects[0].tabs[0];
  assert.equal(tab.request.head, f.after);
  assert.equal(tab.scopeKey, result.review.scopeKey);
  assert.equal((await inspect(tab.request)).key, result.review.key);
});

test('history pagination has no gaps or duplicate commits', async t => {
  const f = await fixture(t);
  const tree = (await git(f.repo, ['rev-parse', 'HEAD^{tree}'])).trim();
  let head = f.after;
  for (let i = 0; i < 201; i++) head = (await git(f.repo, ['commit-tree', tree, '-p', head, '-m', `History ${i}`])).trim();
  await git(f.repo, ['update-ref', 'refs/heads/history', head]);
  const request = { ...f.request, head: 'history' };
  const first = await listCommits(request); const second = await listCommits(request, 200);
  assert.equal(first.commits.length, 200); assert.equal(first.hasMore, true);
  assert.equal(second.commits.length, 6); assert.equal(second.hasMore, false);
  assert.equal(new Set([...first.commits, ...second.commits].map(c => c.sha)).size, 206);
  await assert.rejects(listCommits(request, -1), /offset/);
});
