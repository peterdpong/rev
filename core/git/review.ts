import path from 'node:path';
import { hash, mapLimit } from './util';
import type {
  ChangeStatus,
  ChangedFile,
  Review,
  ReviewMode,
  ReviewRequest,
} from './types';
import { runGitText, runGitText as git } from './command';
import { readPullRequest } from './pull-requests';
import {
  assertCommitId,
  CommitIdLabel,
  defaultBase,
  resolveRef,
  rootOf,
} from './refs';
import { ignoredPaths } from './ignored';
import { readUntracked, untrackedCounts } from './untracked';

export const REVIEW_MODES: readonly ReviewMode[] = ['branch', 'working'];

type Comparison = Pick<Review, 'mode' | 'mergeBase' | 'headSha'>;

/**
 * Check the fields that become Git arguments. Reviews can come back from
 * clients, so never assume they were produced by inspect().
 */
export function assertComparison(review: Comparison) {
  if (!REVIEW_MODES.includes(review.mode)) {
    throw new Error('Unknown comparison mode.');
  }

  assertCommitId(review.mergeBase, CommitIdLabel.MergeBase);

  if (review.mode === 'branch') {
    assertCommitId(review.headSha, CommitIdLabel.HeadCommit);
  }
}

/**
 * Compare from the merge base (the base and head branches' shared ancestor).
 * Branch mode compares with headSha: committed changes only.
 * Working mode compares with files on disk: committed changes plus staged
 * and unstaged edits. inspect() adds untracked files separately.
 *
 * Pass flags in `options`, before `--end-of-options`; append paths after `--`.
 */
export function diffArgs(review: Comparison, options: string[] = []) {
  assertComparison(review);
  return [
    'diff',
    '--no-ext-diff', // Use Git's built-in diff instead of external helpers.
    '--no-textconv', // Compare contents without text conversion filters.
    '--find-renames', // Recognize renames instead of separate deletes and adds.
    ...options,
    '--end-of-options', // Stop parsing flags before the commit IDs.
    review.mergeBase,
    ...(review.mode === 'branch' ? [review.headSha] : []),
  ];
}

type Counts = Pick<ChangedFile, 'added' | 'removed' | 'binary'>;
const NO_COUNTS: Counts = { added: 0, removed: 0, binary: false };

/**
 * Split Git's NUL-separated output, read each file's status and paths,
 * then attach added/removed line counts by path. Preserve rename/copy
 * source paths and flag binary files.
 *
 * Example input: concatenate these records (\0 = NUL, \t = tab):
 *   ":100644 100644 aaaaaaa bbbbbbb M\0src/app.ts\0"
 *   ":100644 100644 ccccccc ccccccc R100\0old.ts\0new.ts\0"
 *   "12\t3\tsrc/app.ts\0"
 *   "0\t0\t\0old.ts\0new.ts\0"
 *
 * Parsed result:
 *   [
 *     { status: 'M', path: 'src/app.ts', added: 12, removed: 3, binary: false },
 *     { status: 'R', oldPath: 'old.ts', path: 'new.ts',
 *       added: 0, removed: 0, binary: false },
 *   ]
 */
export function parseDiffSummary(raw: string): ChangedFile[] {
  // NUL separators preserve tabs and newlines inside filenames.
  const parts = raw.split('\0');
  const files: ChangedFile[] = [];
  const counts = new Map<string, Counts>();
  for (let index = 0; index < parts.length - 1;) {
    const record = parts[index++];
    if (record.startsWith(':')) {
      // Raw metadata ends with a status such as M or R100; keep its letter.
      const status = record.slice(
        record.lastIndexOf(' ') + 1,
      )[0] as ChangeStatus;

      // Renames/copies have old and new paths; other statuses have one path.
      const first = parts[index++];
      files.push(
        status === 'R' || status === 'C'
          ? { status, oldPath: first, path: parts[index++], ...NO_COUNTS }
          : { status, path: first, ...NO_COUNTS },
      );
      continue;
    }

    // Numstat format: <added>\t<removed>\t<path>. The path may contain tabs.
    const firstTab = record.indexOf('\t'); // Ends the added-line count.
    const secondTab = record.indexOf('\t', firstTab + 1); // Ends the removed-line count.
    let name = record.slice(secondTab + 1);
    if (!name) {
      // Rename/copy records leave this field empty; two NUL-separated paths follow.
      index++; // Skip the old path; associate counts with the new path.
      name = parts[index++];
    }
    const added = record.slice(0, firstTab);
    const removed = record.slice(firstTab + 1, secondTab);
    // Binary files use "-" instead of counts; report zero lines and flag them.
    counts.set(name, {
      added: Number(added) || 0,
      removed: Number(removed) || 0,
      binary: added === '-',
    });
  }

  // Join raw metadata with numstat counts.
  return files.map((file) => ({ ...file, ...counts.get(file.path) }));
}

/**
 * Review identity key for a given checkout, mode, base name,
 * and head name (the current branch for HEAD)
 */
export const reviewKey = (
  repo: string,
  mode: ReviewMode,
  base: string,
  head: string,
) => hash([repo, mode, base, head].join('\0')).slice(0, 24);

/**
 * Branch and remote names for the ref picker, without symbolic refs such as
 * origin/HEAD.
 */
async function refNames(repo: string) {
  const out = await runGitText(repo, [
    'for-each-ref',
    '--format=%(if)%(symref)%(then)%(else)%(refname:short)%(end)',
    'refs/heads',
    'refs/remotes',
    'refs/revue',
  ]);
  return out.split('\n').filter(Boolean);
}

/**
 * Resolve the requested comparison and assemble its Review metadata.
 * The viewer uses the changed paths, counts, ignore flags, refs, and checkout
 * status to populate its review surface. Working mode also lists untracked
 * files; patch text is loaded separately by filePatch() or reviewPatches().
 */
export async function inspect(input: ReviewRequest): Promise<Review> {
  const repo = await rootOf(input.repo);
  const base = input.base || (await defaultBase(repo)).base;
  const head = input.head || 'HEAD';
  const mode = input.mode || 'branch';
  if (!['branch', 'working'].includes(mode)) {
    throw new Error('Unknown comparison mode.');
  }
  if (
    input.commitRange &&
    (mode === 'working' ||
      typeof input.commitRange.start !== 'string' ||
      typeof input.commitRange.end !== 'string')
  ) {
    throw new Error('Choose a valid commit range in committed mode.');
  }
  const [baseSha, branchHeadSha, branch, refs, status] = await Promise.all([
    resolveRef(repo, base),
    resolveRef(repo, mode === 'working' ? 'HEAD' : head),
    git(repo, ['branch', '--show-current']),
    git(repo, [
      'for-each-ref',
      '--format=%(if)%(symref)%(then)%(else)%(refname:short)%(end)',
      'refs/heads',
      'refs/remotes',
      'refs/revue',
    ]),
    git(repo, ['status', '--porcelain=v1', '-z']),
  ]);
  if (
    mode === 'working' &&
    head !== 'HEAD' &&
    (await resolveRef(repo, head)) !== branchHeadSha
  ) {
    throw new Error(
      `The working tree belongs to ${branch.trim() || 'a detached HEAD'}. Open that branch to review local changes.`,
    );
  }
  let mergeBase = input.commitRange
    ? ''
    : (await git(repo, ['merge-base', baseSha, branchHeadSha])).trim();
  let headSha = branchHeadSha;
  let commitRange;
  let commitCount = 0;
  if (input.commitRange) {
    const [start, end, history] = await Promise.all([
      resolveRef(repo, input.commitRange.start),
      resolveRef(repo, input.commitRange.end),
      git(repo, ['rev-list', '--first-parent', branchHeadSha, '--']),
    ]);
    const shas = history.trim().split('\n');
    const first = shas.indexOf(start);
    const last = shas.indexOf(end);
    if (first < 0 || last < 0) {
      throw new Error(
        'Selected commits are not in this branch’s first-parent history. Choose Full branch diff or select commits from the current branch.',
      );
    }
    if (first < last) {
      throw new Error(
        'The start commit must be older than or equal to the end commit.',
      );
    }
    // Read the stored header: rev-list hides parents at shallow boundaries,
    // which must not be mistaken for a root commit and diffed from empty.
    const header = (await git(repo, ['cat-file', 'commit', start])).split(
      '\n\n',
      1,
    )[0];
    const parent = header
      .split('\n')
      .find((line) => line.startsWith('parent '))
      ?.slice(7);
    if (parent) {
      try {
        mergeBase = await resolveRef(repo, parent);
      } catch {
        throw new Error(
          'The parent commit is unavailable locally. Fetch more history before reviewing this commit.',
        );
      }
    } else {
      mergeBase = (
        await git(repo, ['hash-object', '-t', 'tree', '--stdin'])
      ).trim();
    }
    headSha = end;
    commitRange = { start, end };
    commitCount = first - last + 1;
  }
  const key = reviewKey(
    repo,
    mode,
    base,
    head === 'HEAD' ? branch.trim() || branchHeadSha : head,
  );
  const scopeKey = commitRange
    ? hash(`${key}\0${commitRange.start}\0${commitRange.end}`).slice(0, 24)
    : key;
  const review: Review = {
    repo,
    name: path.basename(repo),
    base,
    head,
    mode,
    branch: branch.trim() || 'detached HEAD',
    baseSha,
    headSha,
    branchHeadSha,
    mergeBase,
    commitRange,
    commitCount,
    scopeKey,
    branches: refs.trim().split('\n').filter(Boolean),
    files: [],
    dirty: Boolean(status),
    key,
  };
  const [commit, pullRequest] = await Promise.all([
    git(repo, [
      'log',
      '-1',
      '--format=%H%x00%P%x00%s%x00%b%x00%an%x00%aI',
      headSha,
      '--',
    ]),
    readPullRequest(repo, head),
  ]);
  const [sha, parents, subject, body, author, date] = commit
    .trimEnd()
    .split('\0');
  review.headCommit = {
    sha,
    parents: parents.split(' ').filter(Boolean),
    subject,
    body: body.trim(),
    author,
    date,
  };
  review.pullRequest = pullRequest;
  if (!commitRange) {
    review.commitCount = Number(
      (
        await git(repo, [
          'rev-list',
          '--count',
          `${mergeBase}..${headSha}`,
          '--',
        ])
      ).trim(),
    );
  }
  const [summary, untracked] = await Promise.all([
    runGitText(repo, diffArgs(review, ['--raw', '--numstat', '-z'])),
    mode === 'working' ? runGitText(repo, ['ls-files', '--others', '-z']) : '',
  ]);
  review.files = parseDiffSummary(summary);

  // A staged deletion can leave an untracked copy on disk. Keep its deletion
  // diff rather than adding a second entry for the same path.
  const changedPaths = new Set(review.files.map((file) => file.path));
  const untrackedPaths = untracked
    .split('\0')
    .filter((name) => name && !changedPaths.has(name));

  review.files.push(
    ...(await mapLimit(
      untrackedPaths,
      8,
      async (name): Promise<ChangedFile> => {
        const counts = await readUntracked(repo, name)
          .then(untrackedCounts)
          .catch(() => NO_COUNTS);
        return { path: name, status: '?', ...counts };
      },
    )),
  );

  const ignored = await ignoredPaths(repo, untrackedPaths);
  for (const file of review.files) {
    file.ignored = file.status === '?' && ignored.has(file.path);
  }

  return review;
}
