import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { createHash } from 'node:crypto';
import { realpath, readFile, stat } from 'node:fs/promises';
import path from 'node:path';
import type { ReviewRequest, Review, ChangedFile, FilePatch } from '../shared';

const exec = promisify(execFile);
export const hash = (value: string) => createHash('sha256').update(value).digest('hex');
export async function command(executable: string, args: string[], cwd?: string, maxBuffer = 32 * 1024 * 1024) {
  try { return (await exec(executable, args, { cwd, maxBuffer, timeout: 120_000, encoding: 'utf8', env: { ...process.env, GIT_TERMINAL_PROMPT: '0', GIT_OPTIONAL_LOCKS: '0' } })).stdout; }
  catch (error: any) { throw new Error(error.code === 'ENOENT' ? `${executable} is not installed or is not on PATH.` : (error.stderr || error.message).trim()); }
}
export const git = (repo: string, args: string[], maxBuffer?: number) => command('git', ['-c', 'core.quotePath=false', '--no-pager', '--literal-pathspecs', ...args], repo, maxBuffer);
export async function resolveRef(repo: string, ref: string) {
  if (!ref || ref.includes('\0') || ref.length > 500) throw new Error('Invalid Git reference.');
  return (await git(repo, ['rev-parse', '--verify', '--end-of-options', `${ref}^{commit}`])).trim();
}
export async function rootOf(repo: string) {
  return realpath((await git(path.resolve(repo.replace(/^~(?=\/|$)/, process.env.HOME || '~')), ['rev-parse', '--show-toplevel'])).trim());
}
export async function defaultBase(repo: string) {
  for (const ref of ['refs/remotes/origin/HEAD', 'main', 'master', 'HEAD~1', 'HEAD']) {
    try { await resolveRef(repo, ref); return ref === 'refs/remotes/origin/HEAD' ? (await git(repo, ['symbolic-ref', '--short', ref])).trim() : ref; } catch { /* try next */ }
  }
  throw new Error('This repository has no commits yet. Make an initial commit first.');
}
export function parseNameStatus(raw: string): Pick<ChangedFile, 'status' | 'path' | 'oldPath'>[] {
  const parts = raw.split('\0'); const files = [];
  for (let i = 0; i < parts.length - 1;) {
    const status = parts[i++]; const first = parts[i++];
    if (status.startsWith('R') || status.startsWith('C')) files.push({ status: status[0], oldPath: first, path: parts[i++] });
    else files.push({ status: status[0], path: first });
  }
  return files;
}
function parseNumstat(raw: string) {
  const parts = raw.split('\0'); const counts = new Map<string, { added: number; removed: number; binary: boolean }>();
  for (let i = 0; i < parts.length - 1;) {
    const record = parts[i++]; const first = record.indexOf('\t'); const second = record.indexOf('\t', first + 1);
    let name = record.slice(second + 1);
    if (!name) { i++; name = parts[i++]; }
    const a = record.slice(0, first); const d = record.slice(first + 1, second);
    counts.set(name, { added: Number(a) || 0, removed: Number(d) || 0, binary: a === '-' });
  }
  return counts;
}
export function diffArgs(review: Review) {
  return ['diff', '--no-ext-diff', '--no-textconv', '--find-renames', review.mergeBase, ...(review.mode === 'branch' ? [review.headSha] : [])];
}
export async function inspect(input: ReviewRequest): Promise<Review> {
  const repo = await rootOf(input.repo);
  const base = input.base || await defaultBase(repo);
  const head = input.head || 'HEAD';
  const mode = input.mode || 'branch';
  if (!['branch', 'working'].includes(mode)) throw new Error('Unknown comparison mode.');
  const [baseSha, headSha, branch, refs, status] = await Promise.all([
    resolveRef(repo, base), resolveRef(repo, mode === 'working' ? 'HEAD' : head),
    git(repo, ['branch', '--show-current']), git(repo, ['for-each-ref', '--format=%(refname:short)', 'refs/heads', 'refs/remotes', 'refs/revue']),
    git(repo, ['status', '--porcelain=v1', '-z'])
  ]);
  if (mode === 'working' && head !== 'HEAD' && await resolveRef(repo, head) !== headSha) {
    throw new Error(`The working tree belongs to ${branch.trim() || 'a detached HEAD'}. Open that branch to review local changes.`);
  }
  const mergeBase = (await git(repo, ['merge-base', baseSha, headSha])).trim();
  const review: Review = { repo, name: path.basename(repo), base, head, mode, branch: branch.trim() || 'detached HEAD', baseSha, headSha, mergeBase, branches: refs.trim().split('\n').filter(Boolean), files: [], dirty: Boolean(status), key: hash(`${repo}\0${base}\0${head === 'HEAD' ? branch.trim() || headSha : head}`).slice(0, 24) };
  const [names, nums, untracked] = await Promise.all([
    git(repo, [...diffArgs(review), '--name-status', '-z']),
    git(repo, [...diffArgs(review), '--numstat', '-z']),
    mode === 'working' ? git(repo, ['ls-files', '--others', '--exclude-standard', '-z']) : Promise.resolve('')
  ]);
  const counts = parseNumstat(nums);
  review.files = parseNameStatus(names).map(f => ({ ...f, ...(counts.get(f.path) || { added: 0, removed: 0, binary: false }) }));
  for (const name of untracked.split('\0').filter(Boolean)) review.files.push({ path: name, status: '?', added: 0, removed: 0, binary: false });
  return review;
}
export async function filePatch(review: Review, name: string): Promise<FilePatch> {
  const file = review.files.find(f => f.path === name);
  if (!file) throw new Error('That file is not in this comparison. Refresh the review.');
  let patch = '';
  try {
    if (file.status === '?') {
      const full = await realpath(path.join(review.repo, file.path));
      if (!full.startsWith(review.repo + path.sep)) throw new Error('The untracked file points outside this repository.');
      if ((await stat(full)).size > 2_000_000) return { patch: '', digest: '', tooLarge: true };
      const content = await readFile(full, 'utf8');
      if (content.includes('\0')) return { patch: 'Binary file (untracked)', digest: hash(content), tooLarge: false };
      const lines = content ? content.replace(/\n$/, '').split('\n') : [];
      patch = `diff --git a/${name} b/${name}\nnew file mode 100644\n--- /dev/null\n+++ b/${name}\n@@ -0,0 +1,${lines.length} @@\n${lines.map(l => '+' + l).join('\n')}\n`;
      if (content && !content.endsWith('\n')) patch += '\\ No newline at end of file\n';
    } else patch = await git(review.repo, [...diffArgs(review), '--patch', '--unified=5', '--', ...(file.oldPath ? [file.oldPath] : []), file.path], 3_000_000);
  } catch (error: any) {
    if (/maxBuffer|ENOBUFS/i.test(error.message)) return { patch: '', digest: '', tooLarge: true };
    throw error;
  }
  return { patch, digest: hash(patch), tooLarge: patch.length > 2_000_000 };
}
export async function fetchRepo(repo: string) {
  const root = await rootOf(repo);
  await git(root, ['fetch', '--all', '--prune']);
}
export async function cloneRepo(url: string, destination: string) {
  if (!/^(https:\/\/|ssh:\/\/|git@)/.test(url) || /[\r\n\0]/.test(url)) throw new Error('Use an HTTPS or SSH Git URL.');
  const target = path.resolve(destination.replace(/^~(?=\/|$)/, process.env.HOME || '~'));
  await command('git', ['clone', '--', url, target]);
  return rootOf(target);
}
export async function importPR(repo: string, value: string): Promise<ReviewRequest> {
  const root = await rootOf(repo);
  if (!/^(\d+|https:\/\/github\.com\/[^/]+\/[^/]+\/pull\/\d+\/?(?:\?.*)?)$/.test(value)) throw new Error('Enter a PR number or a github.com pull request URL.');
  const details = JSON.parse(await command('gh', ['pr', 'view', value, '--json', 'number,baseRefName,baseRefOid,headRefOid,url'], root));
  const match = /^https:\/\/github\.com\/([^/]+\/[^/]+)\/pull\/(\d+)$/.exec(details.url);
  if (!match) throw new Error('Only github.com PRs are supported in this prototype.');
  const remote = `https://github.com/${match[1]}.git`;
  const namespace = `refs/revue/${match[1]}/${details.number}`;
  // Fetch exact PR refs into a private namespace; never switch the user's branch.
  await git(root, ['fetch', remote, `+refs/pull/${details.number}/head:${namespace}/head`, `+refs/heads/${details.baseRefName}:${namespace}/base`]);
  return { repo: root, base: `${namespace}/base`, head: `${namespace}/head`, mode: 'branch' };
}
