import { realpath } from 'node:fs/promises';
import { homedir } from 'node:os';
import path from 'node:path';
import { runGitText } from './command';
import type { Checkout } from './types';

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
export async function listBranches(repo: string): Promise<string[]> {
  const root = await rootOf(repo);
  return (
    (
      await runGitText(root, [
        'for-each-ref',
        '--format=%(refname)%09%(refname:short)', // Full and short ref names, separated by a tab (%09).
        'refs/heads', // Branches in this repository.
        'refs/remotes', // Locally cached remote branches; no network request is made.
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
      .map(([, name]) => name)
  );
}

/** The checked-out branch and HEAD of each repo, or null when unavailable. */
export async function checkouts(repos: string[]): Promise<(Checkout | null)[]> {
  if (!Array.isArray(repos) || repos.some((repo) => typeof repo !== 'string')) {
    throw new Error('Pass a list of repository folders.');
  }
  return Promise.all(
    repos.map(async (repo): Promise<Checkout | null> => {
      try {
        const [branch, sha] = await Promise.all([
          runGitText(repo, ['branch', '--show-current']),
          resolveRef(repo, 'HEAD'),
        ]);
        return { repo, branch: branch.trim() || 'detached HEAD', sha };
      } catch {
        return null;
      }
    }),
  );
}
