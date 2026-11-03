import { realpath } from 'node:fs/promises';
import { homedir } from 'node:os';
import path from 'node:path';
import { runGitText, runGitText as git } from './command';
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

export async function listBranches(repo: string): Promise<string[]> {
  const root = await rootOf(repo);
  return (
    await git(root, [
      'for-each-ref',
      '--format=%(refname)%09%(refname:short)',
      'refs/heads',
      'refs/remotes',
    ])
  )
    .trim()
    .split('\n')
    .filter(Boolean)
    .map((line) => line.split('\t'))
    .filter(
      ([ref]) => !ref.startsWith('refs/remotes/') || !ref.endsWith('/HEAD'),
    )
    .map(([, name]) => name);
}

/** Resolve a comparison endpoint without checking out or creating a local branch. */
export async function resolveReviewRef(
  repo: string,
  value: string,
  role: 'base' | 'head',
  prRemote?: string,
): Promise<{ ref: string; sha: string }> {
  const ref = value.trim();
  if (!ref || ref.includes('\0') || ref.length > 500) {
    throw new Error(`Invalid ${role} reference.`);
  }
  try {
    return { ref, sha: await resolveRef(repo, ref) };
  } catch {
    /* try remote branches */
  }
  const missing = () =>
    new Error(
      `Cannot find ${role} "${ref}". Choose an existing branch or fetch its history.`,
    );
  const remotes = (await git(repo, ['remote']))
    .trim()
    .split('\n')
    .filter(Boolean);
  const qualified = ref.replace(/^refs\/remotes\//, '');
  const remote = remotes
    .sort((a, b) => b.length - a.length)
    .find((name) => qualified.startsWith(`${name}/`));
  const branch = remote ? qualified.slice(remote.length + 1) : ref;
  // Revision expressions and private refs must resolve exactly; never turn an
  // unavailable commit or an option into a fetch refspec.
  if (branch.startsWith('refs/') || /^[a-f0-9]{7,64}$/i.test(branch)) {
    throw missing();
  }
  try {
    await git(repo, ['check-ref-format', '--branch', branch]);
  } catch {
    throw missing();
  }
  const candidates = remote
    ? [remote]
    : remotes.includes('origin')
      ? ['origin', ...remotes.filter((r) => r !== 'origin')]
      : remotes;
  const found = [];
  for (const name of candidates) {
    const candidate = `refs/remotes/${name}/${branch}`;
    try {
      found.push({ ref: candidate, sha: await resolveRef(repo, candidate) });
    } catch {
      /* not fetched */
    }
  }
  if (found.length === 1 || found[0]?.ref === `refs/remotes/origin/${branch}`) {
    return found[0];
  }
  if (found.length > 1) {
    throw new Error(
      `More than one remote has ${role} "${branch}". Choose a remote-qualified branch, such as ${candidates[0]}/${branch}.`,
    );
  }
  const source =
    remote ||
    prRemote ||
    (remotes.includes('origin')
      ? 'origin'
      : remotes.length === 1
        ? remotes[0]
        : undefined);
  if (!source) {
    throw missing();
  }
  const destination = remotes.includes(source)
    ? `refs/remotes/${source}/${branch}`
    : `refs/revue/bases/${hash(source).slice(0, 16)}/${branch}`;
  try {
    await git(repo, [
      'fetch',
      '--no-tags',
      '--no-write-fetch-head',
      '--',
      source,
      `+refs/heads/${branch}:${destination}`,
    ]);
    return { ref: destination, sha: await resolveRef(repo, destination) };
  } catch (error) {
    throw new Error(
      `Could not fetch ${role} branch "${branch}" from ${source}: ${(error as Error).message}`,
    );
  }
}

export type BaseCandidate = { ref: string; sha: string; name: string };

/** Only a strict first-parent ancestor above the target base is evidence of a stack. */
export async function stackBase(
  repo: string,
  head: string,
  base: string,
  candidates?: BaseCandidate[],
) {
  if (head === base) {
    return;
  }
  const history = (
    await git(repo, ['rev-list', '--first-parent', head, `^${base}`, '--'])
  )
    .trim()
    .split('\n');
  const distance = new Map(history.map((sha, index) => [sha, index]));
  if (!candidates) {
    const refs = await git(repo, [
      'for-each-ref',
      `--merged=${head}`,
      '--format=%(refname)%09%(objectname)%09%(symref)',
      'refs/heads',
      'refs/remotes',
    ]);
    candidates = refs
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
    const headNames = new Set(
      candidates
        .filter((candidate) => candidate.sha === head)
        .map((candidate) => candidate.name),
    );
    candidates = candidates.filter(
      (candidate) => !headNames.has(candidate.name),
    );
  }
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

export function prNamespace(head: string) {
  return /^refs\/revue\/([^/]+\/[^/]+)\/(\d+)\/head$/.exec(head);
}
