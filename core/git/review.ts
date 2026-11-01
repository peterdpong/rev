import path from 'node:path';
import { hash, mapLimit } from './util';
import type {
  ChangeStatus,
  ChangedFile,
  Review,
  ReviewMode,
  ReviewRequest,
} from './types';
import { runGitText } from './command';
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
 */
export async function inspect(input: ReviewRequest): Promise<Review> {
  const repo = await rootOf(input.repo);
  const mode = input.mode || 'branch';
  if (!REVIEW_MODES.includes(mode)) {
    throw new Error('Unknown comparison mode.');
  }

  const requestedBase = input.base;
  const head = input.head || 'HEAD';
  const [{ base, baseSha }, headSha, current, branches, status] =
    await Promise.all([
      requestedBase
        ? resolveRef(repo, requestedBase).then((baseSha) => ({
            base: requestedBase,
            baseSha,
          }))
        : defaultBase(repo),
      resolveRef(repo, mode === 'working' ? 'HEAD' : head),
      runGitText(repo, ['branch', '--show-current']).then((out) => out.trim()),
      refNames(repo),
      runGitText(repo, ['status', '--porcelain=v1', '-z']),
    ]);

  if (
    mode === 'working' &&
    head !== 'HEAD' &&
    (await resolveRef(repo, head)) !== headSha
  ) {
    throw new Error(
      `The working tree belongs to ${current || 'a detached HEAD'}. Open that branch to review local changes.`,
    );
  }

  const mergeBase = (
    await runGitText(repo, ['merge-base', baseSha, headSha])
  ).trim();

  const review: Review = {
    repo,
    name: path.basename(repo),
    base,
    head,
    mode,
    branch: current || 'detached HEAD',
    baseSha,
    headSha,
    mergeBase,
    branches,
    files: [],
    dirty: Boolean(status),
    key: reviewKey(
      repo,
      mode,
      base,
      head === 'HEAD' ? current || headSha : head,
    ),
  };

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
