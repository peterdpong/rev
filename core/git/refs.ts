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
