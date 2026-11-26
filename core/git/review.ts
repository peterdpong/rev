import path from 'node:path';
import { hash, mapLimit } from './util';
import type {
  ChangeStatus,
  ChangedFile,
  CommitRange,
  Review,
  ReviewMode,
  ReviewRequest,
} from './types';
import { runGitText } from './command';
import { readPullRequest } from './pull-requests';
import { prBaseOf, prRemoteOf, resolveReviewRef, stackBase } from './refs';
import {
  assertCommitId,
  CommitIdLabel,
  defaultBase,
  resolveRef,
  rootOf,
} from './refs';
import {
  listUntrackedFiles,
  readUntracked,
  untrackedCounts,
} from './untracked';

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
    'refs/rev', // Include PR refs imported by Rev.
  ]);
  return out.split('\n').filter(Boolean);
}

function validateReviewRequest(input: ReviewRequest, mode: ReviewMode) {
  if (!REVIEW_MODES.includes(mode)) {
    throw new Error('Unknown comparison mode.');
  }

  if (input.commitRange) {
    // A range selects committed history, not working-tree changes.
    if (mode === 'working') {
      throw new Error('Commit ranges are only supported in committed mode.');
    }

    const { start, end } = input.commitRange;
    if (typeof start !== 'string' || typeof end !== 'string') {
      throw new Error('Choose a start and end commit for the range.');
    }
  }
}

type ResolvedComparison = Pick<
  Review,
  'mergeBase' | 'headSha' | 'commitRange' | 'commitCount' | 'scopeKey'
>;

/** Full-branch and working reviews share the same merge base and commit count. */
async function resolveBranchComparison(
  repo: string,
  baseSha: string,
  headSha: string,
  key: string,
): Promise<ResolvedComparison> {
  const mergeBase = (
    await runGitText(repo, ['merge-base', baseSha, headSha])
  ).trim();

  // Count commits reachable from head but not the merge base.
  const commitCount = Number(
    (
      await runGitText(repo, [
        'rev-list',
        '--count',
        `${mergeBase}..${headSha}`,
        '--',
      ])
    ).trim(),
  );

  return {
    mergeBase,
    headSha,
    commitRange: undefined,
    commitCount,
    scopeKey: key,
  };
}

/** Resolve an inclusive range on the branch's first-parent history. */
async function resolveRangeComparison(
  repo: string,
  branchHeadSha: string,
  range: CommitRange,
  key: string,
): Promise<ResolvedComparison> {
  const [start, end, history] = await Promise.all([
    resolveRef(repo, range.start),
    resolveRef(repo, range.end),
    runGitText(repo, ['rev-list', '--first-parent', branchHeadSha, '--']),
  ]);

  const shas = history.trim().split('\n');
  const first = shas.indexOf(start);
  const last = shas.indexOf(end);
  if (first < 0 || last < 0) {
    throw new Error(
      'Selected commits are not in this branch’s first-parent history. Choose Full branch diff or select commits from the current branch.',
    );
  }

  // History is newest first, so the older start must have the larger index.
  if (first < last) {
    throw new Error(
      'The start commit must be older than or equal to the end commit.',
    );
  }

  // Read the stored header: rev-list hides parents at shallow boundaries,
  // which must not be mistaken for a root commit and diffed from empty.
  const header = (await runGitText(repo, ['cat-file', 'commit', start])).split(
    '\n\n',
    1,
  )[0];

  const parent = header
    .split('\n')
    .find((line) => line.startsWith('parent '))
    ?.slice(7);

  let mergeBase: string;
  if (parent) {
    try {
      // Start before the first selected commit so its changes are included.
      mergeBase = await resolveRef(repo, parent);
    } catch {
      throw new Error(
        'The parent commit is unavailable locally. Fetch more history before reviewing this commit.',
      );
    }
  } else {
    // A root commit has no parent so compare it against Git's empty tree.
    mergeBase = (
      await runGitText(repo, ['hash-object', '-t', 'tree', '--stdin'])
    ).trim();
  }

  return {
    mergeBase,
    headSha: end, // Exclude commits newer than the selected endpoint.
    commitRange: { start, end },
    commitCount: first - last + 1, // Include both endpoints.
    scopeKey: hash(`${key}\0${start}\0${end}`).slice(0, 24),
  };
}

/**
 * Validate the request, resolve its comparison, then load review metadata and files.
 */
export async function inspect(input: ReviewRequest): Promise<Review> {
  const repo = await rootOf(input.repo);
  const head = input.head?.trim() || 'HEAD';
  const mode = input.mode || 'branch';
  validateReviewRequest(input, mode);

  // Set for an imported PR head, so missing branches can be fetched from the repo.
  const remote = prRemoteOf(head);
  const explicitBase = input.base?.trim();

  const resolvedHead = await resolveReviewRef(
    repo,
    mode === 'working' ? 'HEAD' : head,
    'head',
    remote,
  );
  const branchHeadSha = resolvedHead.sha;
  const resolvedHeadRef = mode === 'working' ? head : resolvedHead.ref;

  let resolvedBase = await resolveReviewRef(
    repo,
    explicitBase || (remote ? prBaseOf(head) : (await defaultBase(repo)).base),
    'base',
    remote,
  );

  // With no chosen base, compare a branch to its stack parent if it has one.
  // Imported PRs skip this as importPR already stored their parent as the base.
  if (!explicitBase && !remote && mode === 'branch') {
    resolvedBase =
      (await stackBase(repo, branchHeadSha, resolvedBase.sha)) ?? resolvedBase;
  }

  const { ref: base, sha: baseSha } = resolvedBase;
  const [current, branches, status] = await Promise.all([
    runGitText(repo, ['branch', '--show-current']).then((out) => out.trim()),
    refNames(repo),
    runGitText(repo, ['status', '--porcelain=v1', '-z']),
  ]);

  // Local changes belong to the checked-out branch, so refuse any other head.
  if (
    mode === 'working' &&
    head !== 'HEAD' &&
    (await resolveReviewRef(repo, head, 'head', remote)).sha !== branchHeadSha
  ) {
    throw new Error(
      `The working tree belongs to ${current || 'a detached HEAD'}. Open that branch to review local changes.`,
    );
  }

  // Keep the review identity stable when the selected commit range changes.
  const key = reviewKey(
    repo,
    mode,
    base,
    resolvedHeadRef === 'HEAD' ? current || branchHeadSha : resolvedHeadRef,
  );

  const comparison = input.commitRange
    ? await resolveRangeComparison(repo, branchHeadSha, input.commitRange, key)
    : await resolveBranchComparison(repo, baseSha, branchHeadSha, key);

  const review: Review = {
    repo,
    name: path.basename(repo),
    base,
    head: resolvedHeadRef,
    mode,
    branch: current || 'detached HEAD',
    baseSha,
    branchHeadSha,
    ...comparison,
    branches,
    files: [],
    dirty: Boolean(status),
    key,
  };

  // Load the compared commit's details and any locally cached PR metadata.
  const [commit, pullRequest] = await Promise.all([
    runGitText(repo, [
      'log',
      '-1',
      '--format=%H%x00%P%x00%s%x00%b%x00%an%x00%aI', // NUL-separated ID, parents, subject, body, author, date.
      review.headSha,
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

  const [summary, untracked] = await Promise.all([
    runGitText(repo, diffArgs(review, ['--raw', '--numstat', '-z'])),
    mode === 'working' ? listUntrackedFiles(repo) : [],
  ]);
  review.files = parseDiffSummary(summary);

  // A staged deletion can leave an untracked copy on disk. Keep its deletion
  // diff rather than adding a second entry for the same path.
  const changedPaths = new Set(review.files.map((file) => file.path));
  const untrackedPaths = untracked.filter((name) => !changedPaths.has(name));

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

  return review;
}
