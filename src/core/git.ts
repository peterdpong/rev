import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { createHash } from 'node:crypto';
import { realpath, readFile, stat, mkdir, writeFile, rename } from 'node:fs/promises';
import path from 'node:path';
import type { ReviewRequest, Review, ChangedFile, FilePatch, ReviewPatch, CommitPage, PullRequest } from '../shared';

const exec = promisify(execFile);
export const hash = (value: string) => createHash('sha256').update(value).digest('hex');
export async function command(executable: string, args: string[], cwd?: string, maxBuffer = 32 * 1024 * 1024) {
  try {
    const result = exec(executable, args, { cwd, maxBuffer, timeout: 120_000, encoding: 'utf8', env: { ...process.env, GIT_TERMINAL_PROMPT: '0', GIT_OPTIONAL_LOCKS: '0' } });
    result.child.stdin?.end();
    return (await result).stdout;
  }
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
export async function listBranches(repo: string): Promise<string[]> {
  const root = await rootOf(repo);
  return (await git(root, ['for-each-ref', '--format=%(refname)%09%(refname:short)', 'refs/heads', 'refs/remotes']))
    .trim().split('\n').filter(Boolean).map(line => line.split('\t'))
    .filter(([ref]) => !ref.startsWith('refs/remotes/') || !ref.endsWith('/HEAD')).map(([, name]) => name);
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
export async function listCommits(input: ReviewRequest, offset = 0): Promise<CommitPage> {
  if (!Number.isSafeInteger(offset) || offset < 0) throw new Error('Invalid commit offset.');
  const repo = await rootOf(input.repo);
  const head = await resolveRef(repo, input.head || 'HEAD');
  const parts = (await git(repo, ['log', '--first-parent', '-z', '--format=%H%x00%P%x00%s%x00%an%x00%aI', '--max-count=201', `--skip=${offset}`, head, '--'])).split('\0');
  const commits = [];
  for (let i = 0; i + 4 < parts.length; i += 5) {
    commits.push({ sha: parts[i], parents: parts[i + 1].split(' ').filter(Boolean), subject: parts[i + 2], author: parts[i + 3], date: parts[i + 4] });
  }
  return { commits: commits.slice(0, 200), hasMore: commits.length > 200 };
}
export async function inspect(input: ReviewRequest): Promise<Review> {
  const repo = await rootOf(input.repo);
  const base = input.base || await defaultBase(repo);
  const head = input.head || 'HEAD';
  const mode = input.mode || 'branch';
  if (!['branch', 'working'].includes(mode)) throw new Error('Unknown comparison mode.');
  if (input.commitRange && (mode === 'working' || typeof input.commitRange.start !== 'string' || typeof input.commitRange.end !== 'string')) throw new Error('Choose a valid commit range in committed mode.');
  const [baseSha, branchHeadSha, branch, refs, status] = await Promise.all([
    resolveRef(repo, base), resolveRef(repo, mode === 'working' ? 'HEAD' : head),
    git(repo, ['branch', '--show-current']), git(repo, ['for-each-ref', '--format=%(refname:short)', 'refs/heads', 'refs/remotes', 'refs/revue']),
    git(repo, ['status', '--porcelain=v1', '-z'])
  ]);
  if (mode === 'working' && head !== 'HEAD' && await resolveRef(repo, head) !== branchHeadSha) {
    throw new Error(`The working tree belongs to ${branch.trim() || 'a detached HEAD'}. Open that branch to review local changes.`);
  }
  let mergeBase = input.commitRange ? '' : (await git(repo, ['merge-base', baseSha, branchHeadSha])).trim();
  let headSha = branchHeadSha;
  let commitRange;
  let commitCount = 0;
  if (input.commitRange) {
    const [start, end, history] = await Promise.all([
      resolveRef(repo, input.commitRange.start), resolveRef(repo, input.commitRange.end),
      git(repo, ['rev-list', '--first-parent', branchHeadSha, '--']),
    ]);
    const shas = history.trim().split('\n');
    const first = shas.indexOf(start); const last = shas.indexOf(end);
    if (first < 0 || last < 0) throw new Error('Selected commits are not in this branch’s first-parent history. Choose Full branch diff or select commits from the current branch.');
    if (first < last) throw new Error('The start commit must be older than or equal to the end commit.');
    // Read the stored header: rev-list hides parents at shallow boundaries,
    // which must not be mistaken for a root commit and diffed from empty.
    const header = (await git(repo, ['cat-file', 'commit', start])).split('\n\n', 1)[0];
    const parent = header.split('\n').find(line => line.startsWith('parent '))?.slice(7);
    if (parent) {
      try { mergeBase = await resolveRef(repo, parent); }
      catch { throw new Error('The parent commit is unavailable locally. Fetch more history before reviewing this commit.'); }
    } else mergeBase = (await git(repo, ['hash-object', '-t', 'tree', '--stdin'])).trim();
    headSha = end; commitRange = { start, end }; commitCount = first - last + 1;
  }
  const key = hash(`${repo}\0${base}\0${head === 'HEAD' ? branch.trim() || branchHeadSha : head}`).slice(0, 24);
  const scopeKey = commitRange ? hash(`${key}\0${commitRange.start}\0${commitRange.end}`).slice(0, 24) : key;
  const review: Review = { repo, name: path.basename(repo), base, head, mode, branch: branch.trim() || 'detached HEAD', baseSha, headSha, branchHeadSha, mergeBase, commitRange, commitCount, scopeKey, branches: refs.trim().split('\n').filter(Boolean), files: [], dirty: Boolean(status), key };
  const [commit, pullRequest] = await Promise.all([
    git(repo, ['log', '-1', '--format=%H%x00%P%x00%s%x00%b%x00%an%x00%aI', headSha, '--']),
    readPullRequest(repo, head),
  ]);
  const [sha, parents, subject, body, author, date] = commit.trimEnd().split('\0');
  review.headCommit = { sha, parents: parents.split(' ').filter(Boolean), subject, body: body.trim(), author, date };
  review.pullRequest = pullRequest;
  if (!commitRange) review.commitCount = Number((await git(repo, ['rev-list', '--count', `${mergeBase}..${headSha}`, '--'])).trim());
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
function patchPath(name: string) {
  if (!/[\x00-\x20\x7f"\\]/.test(name)) return name;
  return '"' + name.replace(/[\x00-\x1f\x7f"\\]/g, char => char === '"' || char === '\\' ? '\\' + char : '\\' + char.charCodeAt(0).toString(8).padStart(3, '0')) + '"';
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
      patch = `diff --git ${patchPath(`a/${name}`)} ${patchPath(`b/${name}`)}\nnew file mode 100644\n--- /dev/null\n+++ ${patchPath(`b/${name}`)}\n@@ -0,0 +1,${lines.length} @@\n${lines.map(l => '+' + l).join('\n')}\n`;
      if (content && !content.endsWith('\n')) patch += '\\ No newline at end of file\n';
    } else patch = await git(review.repo, [...diffArgs(review), '--patch', '--unified=5', '--', ...(file.oldPath ? [file.oldPath] : []), file.path], 3_000_000);
  } catch (error: any) {
    if (/maxBuffer|ENOBUFS/i.test(error.message)) return { patch: '', digest: '', tooLarge: true };
    throw error;
  }
  return { patch, digest: hash(patch), tooLarge: patch.length > 2_000_000 };
}
/** Load a continuous review against one resolved comparison, with bounded Git concurrency. */
export async function reviewPatches(review: Review): Promise<ReviewPatch[]> {
  const patches: ReviewPatch[] = new Array(review.files.length);
  let next = 0;
  await Promise.all(Array.from({ length: Math.min(4, review.files.length) }, async () => {
    while (next < review.files.length) {
      const index = next++; const path = review.files[index].path;
      try {
        const patch = await filePatch(review, path);
        patches[index] = { ...patch, path, patch: patch.tooLarge ? '' : patch.patch };
      } catch (error) {
        patches[index] = { path, patch: '', digest: '', tooLarge: false, error: error instanceof Error ? error.message : String(error) };
      }
    }
  }));
  return patches;
}

export async function fetchRepo(repo: string) {
  const root = await rootOf(repo);
  await git(root, ['fetch', '--all', '--prune']);
}
async function pullRequestPath(repo: string, head: string) {
  const dir = (await git(repo, ['rev-parse', '--absolute-git-dir'])).trim();
  return path.join(dir, 'revue', `pr-${hash(head).slice(0, 24)}.json`);
}
async function readPullRequest(repo: string, head: string): Promise<PullRequest | undefined> {
  if (!head.startsWith('refs/revue/')) return;
  try { return JSON.parse(await readFile(await pullRequestPath(repo, head), 'utf8')); }
  catch (error: any) { if (error.code !== 'ENOENT') throw error; }
}
export async function importPR(repo: string, value: string): Promise<ReviewRequest> {
  const root = await rootOf(repo);
  if (!/^(\d+|https:\/\/github\.com\/[^/]+\/[^/]+\/pull\/\d+\/?(?:\?.*)?)$/.test(value)) throw new Error('Enter a PR number or a github.com pull request URL.');
  const details = JSON.parse(await command('gh', ['pr', 'view', value, '--json', 'number,title,body,author,state,isDraft,headRefName,baseRefName,updatedAt,url'], root));
  const match = /^https:\/\/github\.com\/([^/]+\/[^/]+)\/pull\/(\d+)$/.exec(details.url);
  if (!match) throw new Error('Only github.com PRs are supported in this prototype.');
  const remote = `https://github.com/${match[1]}.git`;
  const namespace = `refs/revue/${match[1]}/${details.number}`;
  // Fetch exact PR refs into a private namespace; never switch the user's branch.
  await git(root, ['fetch', remote, `+refs/pull/${details.number}/head:${namespace}/head`, `+refs/heads/${details.baseRefName}:${namespace}/base`]);
  const metadata: PullRequest = { ...details, author: details.author?.login || 'Unknown author' };
  const file = await pullRequestPath(root, `${namespace}/head`);
  await mkdir(path.dirname(file), { recursive: true });
  const temporary = `${file}.${process.pid}.${Date.now()}`;
  await writeFile(temporary, JSON.stringify(metadata), { mode: 0o600 });
  await rename(temporary, file);
  return { repo: root, base: `${namespace}/base`, head: `${namespace}/head`, mode: 'branch' };
}
