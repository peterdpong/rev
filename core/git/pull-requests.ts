import { mkdir, readFile, rename, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { hash } from './util';
import { runGitText, runCommandText } from './command';
import { rootOf } from './refs';
import type { PullRequest, Review, ReviewRequest } from './types';

export async function fetchRepo(repo: string) {
  const root = await rootOf(repo);
  await runGitText(root, ['fetch', '--all', '--prune']);
}

async function pullRequestPath(repo: string, head: string) {
  const dir = (
    await runGitText(repo, ['rev-parse', '--absolute-git-dir'])
  ).trim();
  return path.join(dir, 'rev', `pr-${hash(head).slice(0, 24)}.json`);
}

export async function readPullRequest(
  repo: string,
  head: string,
): Promise<PullRequest | undefined> {
  if (!head.startsWith('refs/rev/')) {
    return;
  }

  try {
    return JSON.parse(
      await readFile(await pullRequestPath(repo, head), 'utf8'),
    );
  } catch (error: any) {
    if (error.code !== 'ENOENT') {
      throw error;
    }
  }
}

/** Import a GitHub PR's commits and metadata into a local repo for review. */
export async function importPR(
  repo: string,
  value: string,
): Promise<ReviewRequest> {
  const root = await rootOf(repo);
  if (
    !/^(\d+|https:\/\/github\.com\/[^/]+\/[^/]+\/pull\/\d+\/?(?:\?.*)?)$/.test(
      value,
    )
  ) {
    throw new Error('Enter a PR number or a github.com pull request URL.');
  }

  // Resolve PR’s title, description, author, branch names, status, and URL.
  const details = JSON.parse(
    await runCommandText(
      'gh',
      [
        'pr',
        'view',
        value,
        '--json',
        'number,title,body,author,state,isDraft,headRefName,baseRefName,updatedAt,url',
      ],
      root,
    ),
  );

  // Use the canonical URL returned by gh to identify the PR's owner/repository.
  const match = /^https:\/\/github\.com\/([^/]+\/[^/]+)\/pull\/(\d+)$/.exec(
    details.url,
  );

  if (!match) {
    throw new Error('Only github.com PRs are supported in this prototype.');
  }

  const remote = `https://github.com/${match[1]}.git`;
  // App-owned refs, e.g. refs/rev/owner/repo/42, keep imports separate from branches.
  const namespace = `refs/rev/${match[1]}/${details.number}`;

  // Fetch exact PR refs into a private namespace.
  // Each refspec is +<remote ref>:<local ref>; + allows updates after force pushes.
  await runGitText(root, [
    'fetch',
    remote,
    // GitHub exposes the PR's head here, including PRs originating from forks.
    `+refs/pull/${details.number}/head:${namespace}/head`,
    // Fetch the target branch so the review can compute its merge base with the PR.
    `+refs/heads/${details.baseRefName}:${namespace}/base`,
  ]);

  // Store the author's login as a string instead of gh's nested author object.
  const metadata: PullRequest = {
    ...details,
    author: details.author?.login || 'Unknown author',
  };

  // Cache PR details under .git/rev/, keyed by the head ref readPullRequest uses.
  const file = await pullRequestPath(root, `${namespace}/head`);
  await mkdir(path.dirname(file), { recursive: true });

  // Write a temporary file, then rename it so readers never see partially written JSON.
  const temporary = `${file}.${process.pid}.${Date.now()}`;
  await writeFile(temporary, JSON.stringify(metadata), { mode: 0o600 });
  await rename(temporary, file);

  return {
    repo: root,
    base: `${namespace}/base`,
    head: `${namespace}/head`,
    mode: 'branch',
  };
}

/**
 * Re-import the PR a review was opened from so it picks up new pushes,
 * keeping the review's mode and commit range.
 */
export async function refreshPR(
  review: Review,
): Promise<ReviewRequest | undefined> {
  const ref = /^refs\/rev\/([^/]+\/[^/]+)\/(\d+)\/head$/.exec(review.head);
  const url =
    review.pullRequest?.url ||
    (ref ? `https://github.com/${ref[1]}/pull/${ref[2]}` : undefined);
  if (!url) {
    return undefined;
  }

  return {
    ...(await importPR(review.repo, url)),
    mode: review.mode,
    commitRange: review.commitRange,
  };
}
