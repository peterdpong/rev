import { realpath } from 'node:fs/promises';
import { homedir } from 'node:os';
import path from 'node:path';
import { runGitText } from './command';
import type { Checkout } from './types';
import { hash } from './util';

const COMMIT_ID = /^(?:[0-9a-f]{40}|[0-9a-f]{64})$/;

export enum CommitIdLabel {
  Commit = 'commit',
  MergeBase = 'merge base',
  HeadCommit = 'head commit',
}

export const isCommitId = (value: unknown): value is string =>
  typeof value === 'string' && COMMIT_ID.test(value);

/**
 * Accept only a full SHA-1 or SHA-256 commit ID.
 */
export function assertCommitId(
  value: string,
  label: CommitIdLabel = CommitIdLabel.Commit,
): string {
  if (!isCommitId(value)) {
    throw new Error(`Invalid ${label} ID.`);
  }

  return value;
}

/**
 * Resolve a branch, tag, or revision expression to an exact commit ID.
 */
export async function resolveRef(repo: string, ref: string) {
  if (!ref || ref.includes('\0') || ref.length > 500) {
    throw new Error('Invalid Git reference.');
  }

  return (
    await runGitText(repo, [
      'rev-parse',
      '--verify',
      '--end-of-options',
      `${ref}^{commit}`,
    ])
  ).trim();
}

/**
 * Find the canonical checkout root for a folder opened in the viewer.
 */
export async function rootOf(repo: string) {
  const directory = path.resolve(repo.replace(/^~(?=\/|$)/, homedir()));
  const root = (
    await runGitText(directory, ['rev-parse', '--show-toplevel'])
  ).trim();
  return realpath(root);
}

/**
 * Choose a comparison base and resolve its commit when none was supplied.
 * Prefer the remote default branch, then main/master, then recent HEAD
 * history.
 */
export async function defaultBase(repo: string) {
  for (const ref of [
    'refs/remotes/origin/HEAD',
    'main',
    'master',
    'HEAD~1',
    'HEAD',
  ]) {
    try {
      const baseSha = await resolveRef(repo, ref);
      const base =
        ref === 'refs/remotes/origin/HEAD'
          ? (await runGitText(repo, ['symbolic-ref', '--short', ref])).trim()
          : ref;
      return { base, baseSha };
    } catch {
      /* try next */
    }
  }

  throw new Error(
    'This repository has no commits yet. Make an initial commit first.',
  );
}

/** List local and locally cached remote branches, excluding remote HEAD aliases. */
export async function listBranches(
  repo: string,
  localOnly = false,
): Promise<string[]> {
  const root = await rootOf(repo);
  return (
    (
      await runGitText(root, [
        'for-each-ref',
        '--format=%(refname)%09%(refname:short)', // Full and short ref names, separated by a tab (%09).
        'refs/heads', // Branches in this repository.
        ...(!localOnly ? ['refs/remotes'] : []), // Cached remote branches; no network request.
      ])
    )
      .trim()
      .split('\n')
      .filter(Boolean) // Filter out blank branch entries.
      .map((line) => line.split('\t'))
      // Use full names to distinguish remote HEAD aliases from local branches.
      .filter(
        ([ref]) => !ref.startsWith('refs/remotes/') || !ref.endsWith('/HEAD'),
      )
      // Return short names such as main or origin/main.
      .map(([ref, name]) =>
        localOnly ? ref.slice('refs/heads/'.length) : name,
      )
  );
}

/** Read the current branch and HEAD commit, including detached checkouts. */
export async function readCheckout(
  repo: string,
): Promise<Pick<Checkout, 'branch' | 'sha'>> {
  const [branch, sha] = await Promise.all([
    runGitText(repo, ['branch', '--show-current']),
    resolveRef(repo, 'HEAD'),
  ]);
  return { branch: branch.trim() || 'detached HEAD', sha };
}

/** The checked-out branch and HEAD of each repo, or null when unavailable. */
export async function checkouts(repos: string[]): Promise<(Checkout | null)[]> {
  if (!Array.isArray(repos) || repos.some((repo) => typeof repo !== 'string')) {
    throw new Error('Pass a list of repository folders.');
  }
  return Promise.all(
    repos.map(async (repo): Promise<Checkout | null> => {
      try {
        return { repo, ...(await readCheckout(repo)) };
      } catch {
        return null;
      }
    }),
  );
}

async function listRemotes(repo: string) {
  return (await runGitText(repo, ['remote']))
    .trim()
    .split('\n')
    .filter(Boolean);
}

/** The remote whose name prefixes `shortRef` (e.g. origin in origin/main) */
function remoteOf(remotes: string[], shortRef: string) {
  return [...remotes]
    .sort((a, b) => b.length - a.length)
    .find((name) => shortRef.startsWith(`${name}/`));
}

/** Where branches fetched from a URL that is not a configured remote are kept. */
const privateBasePrefix = (source: string) =>
  `refs/rev/bases/${hash(source).slice(0, 16)}/`;

type ResolvedRef = { ref: string; sha: string };

/**
 * Turn a typed base or head (e.g. `main`, `origin/parent`, `parent`) into a ref
 * and commit, fetching a remote branch if needed. Never changes the checkout or
 * creates a local branch.
 */
export async function resolveReviewRef(
  repo: string,
  name: string,
  role: 'base' | 'head',
  prRemote?: string,
): Promise<ResolvedRef> {
  const ref = name.trim();
  if (!ref || ref.includes('\0') || ref.length > 500) {
    throw new Error(`Invalid ${role} reference.`);
  }

  // 1. Use the ref as-is if it exists locally.
  try {
    return { ref, sha: await resolveRef(repo, ref) };
  } catch {
    /* try remote branches */
  }

  const missing = () =>
    new Error(
      `Cannot find ${role} "${ref}". Choose an existing branch or fetch its history.`,
    );

  const remotes = await listRemotes(repo);
  const shortRef = ref.replace(/^refs\/remotes\//, '');
  const namedRemote = remoteOf(remotes, shortRef);
  const branch = namedRemote ? shortRef.slice(namedRemote.length + 1) : ref;

  // Only plain branch names are fetched. A commit hash or refs/... path that
  // isn't local is reported as missing.
  if (branch.startsWith('refs/') || /^[a-f0-9]{7,64}$/i.test(branch)) {
    throw missing();
  }

  try {
    // Reject anything git won't accept as a branch name, e.g. `--help` or `HEAD~1`.
    await runGitText(repo, ['check-ref-format', '--branch', branch]);
  } catch {
    throw missing();
  }

  // 2. Look for an already-fetched remote branch (origin wins a tie).
  const searchRemotes = namedRemote ? [namedRemote] : remotes;
  const fetched: ResolvedRef[] = [];

  for (const remoteName of searchRemotes) {
    const trackingRef = `refs/remotes/${remoteName}/${branch}`;
    try {
      fetched.push({
        ref: trackingRef,
        sha: await resolveRef(repo, trackingRef),
      });
    } catch {
      /* not fetched */
    }
  }

  const fetchedMatch =
    fetched.length === 1
      ? fetched[0]
      : fetched.find((f) => f.ref === `refs/remotes/origin/${branch}`);

  if (fetchedMatch) {
    return fetchedMatch;
  }

  if (fetched.length > 1) {
    throw new Error(
      `More than one remote has ${role} "${branch}". Choose a remote-qualified branch, such as ${fetched[0].ref.slice('refs/remotes/'.length)}.`,
    );
  }

  // 3. Not available locally, fetch just this branch.
  const fetchFrom =
    namedRemote ||
    prRemote ||
    (remotes.includes('origin')
      ? 'origin'
      : remotes.length === 1
        ? remotes[0]
        : undefined);

  if (!fetchFrom) {
    throw missing();
  }

  const fetchInto = remotes.includes(fetchFrom)
    ? `refs/remotes/${fetchFrom}/${branch}`
    : `${privateBasePrefix(fetchFrom)}${branch}`;

  try {
    await fetchBranch(repo, fetchFrom, branch, fetchInto);
    return { ref: fetchInto, sha: await resolveRef(repo, fetchInto) };
  } catch (error) {
    throw new Error(
      `Could not fetch ${role} branch "${branch}" from ${fetchFrom}: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
}

function fetchBranch(
  repo: string,
  source: string,
  branch: string,
  destination: string,
) {
  return runGitText(repo, [
    'fetch',
    '--no-tags', // Skip the remote's tags; only the one branch is needed.
    '--no-write-fetch-head', // Leave .git/FETCH_HEAD for the user's own fetches.
    '--', // End options so the remote cannot be read as a flag.
    source,
    `+refs/heads/${branch}:${destination}`,
  ]);
}

/**
 * Re-fetch `ref` when it tracks a remote branch (a remote-tracking ref or one
 * fetched from `prRemote` by resolveReviewRef). Local branches are left alone.
 */
export async function refreshRemoteRef(
  repo: string,
  ref: string,
  prRemote?: string,
) {
  const full = (
    await runGitText(repo, [
      'rev-parse',
      '--symbolic-full-name', // Print the full ref name, e.g. refs/remotes/origin/main.
      '--verify', // Require exactly one valid ref.
      '--end-of-options', // Stop parsing flags before the ref.
      ref,
    ])
  ).trim();
  const remote = full.startsWith('refs/remotes/')
    ? remoteOf(await listRemotes(repo), full.slice('refs/remotes/'.length))
    : undefined;
  const prefix = remote
    ? `refs/remotes/${remote}/`
    : prRemote && privateBasePrefix(prRemote);
  const source = remote || prRemote;
  if (source && prefix && full.startsWith(prefix)) {
    await fetchBranch(repo, source, full.slice(prefix.length), full);
  }
}

export type BaseCandidate = { ref: string; sha: string; name: string };

/**
 * Local and remote-tracking branch tips in head's history, minus head's own
 * branch and any names in `exclude`.
 */
export async function branchTips(
  repo: string,
  head: string,
  exclude: string[] = [],
): Promise<BaseCandidate[]> {
  const refs = await runGitText(repo, [
    'for-each-ref',
    `--merged=${head}`, // Only refs whose tips are in head's history.
    '--format=%(refname)%09%(objectname)%09%(symref)', // Ref, tip commit, and symbolic target (set for aliases such as origin/HEAD), tab-separated.
    'refs/heads', // Local branches.
    'refs/remotes', // Locally cached remote branches; no network request is made.
  ]);
  const tips = refs
    .trim()
    .split('\n')
    .filter(Boolean)
    .map((line) => line.split('\t'))
    .filter(([, , symbolic]) => !symbolic)
    .map(([ref, sha]) => ({
      ref,
      sha,
      name: ref.replace(/^refs\/(heads\/|remotes\/[^/]+\/)/, ''),
    }));
  // The remote tip of this same branch is not a stack parent when local
  // commits have not been pushed yet.
  const ownNames = new Set([
    ...exclude,
    ...tips.filter((tip) => tip.sha === head).map((tip) => tip.name),
  ]);
  return tips.filter((tip) => !ownNames.has(tip.name));
}

/**
 * The nearest candidate on head's first-parent history above base.
 */
export async function stackBase(
  repo: string,
  head: string,
  base: string,
  candidates?: BaseCandidate[],
): Promise<BaseCandidate | undefined> {
  if (head === base) {
    return;
  }
  const history = (
    await runGitText(repo, [
      'rev-list',
      '--first-parent', // Follow only the mainline of merges, newest first.
      head,
      `^${base}`, // Stop at commits already in the target base.
      '--', // End revision arguments.
    ])
  )
    .trim()
    .split('\n');
  const distance = new Map(history.map((sha, index) => [sha, index]));
  candidates ??= await branchTips(repo, head);
  const ancestors = candidates
    .filter((candidate) => (distance.get(candidate.sha) ?? 0) > 0)
    .sort(
      (a, b) =>
        distance.get(a.sha)! - distance.get(b.sha)! ||
        Number(b.ref.startsWith('refs/heads/')) -
          Number(a.ref.startsWith('refs/heads/')) ||
        a.ref.localeCompare(b.ref),
    );
  const nearest = ancestors[0];
  if (!nearest) {
    return;
  }
  // Multiple names at one commit do not establish a unique parent branch.
  if (
    new Set(
      ancestors
        .filter((candidate) => candidate.sha === nearest.sha)
        .map((candidate) => candidate.name),
    ).size !== 1
  ) {
    return;
  }
  return nearest;
}

/** The GitHub remote of an imported PR head (refs/rev/<owner>/<repo>/<n>/head). */
export function prRemoteOf(head: string) {
  const match = /^refs\/rev\/([^/]+\/[^/]+)\/\d+\/head$/.exec(head);
  return match ? `https://github.com/${match[1]}.git` : undefined;
}

/** The automatically chosen base importPR stores beside an imported PR head. */
export const prBaseOf = (head: string) => head.replace(/\/head$/, '/base');
