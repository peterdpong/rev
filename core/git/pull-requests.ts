import { mkdir, readFile, rename, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { hash } from './util';
import { runGitText, runCommandText } from './command';
import {
  branchTips,
  prBaseOf,
  refreshRemoteRef,
  resolveRef,
  resolveReviewRef,
  rootOf,
  stackBase,
  type BaseCandidate,
} from './refs';
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

/** One entry of `gh pr list --json number,headRefName,headRefOid`. */
type PullTip = { number: number; headRefName: string; headRefOid: string };

function isPullTip(value: unknown): value is PullTip {
  if (typeof value !== 'object' || value === null) {
    return false;
  }
  const pull = value as Record<string, unknown>;
  return (
    Number.isInteger(pull.number) &&
    typeof pull.headRefName === 'string' &&
    typeof pull.headRefOid === 'string'
  );
}

/**
 * Tips of the repository's other active PRs (open, including drafts).
 */
async function activePullTips(
  repository: string,
  pr: { number: number; headRefName: string },
  cwd: string,
): Promise<BaseCandidate[]> {
  // A failed gh call or unexpected output contributes no PR tips.
  const pulls = await runCommandText(
    'gh',
    [
      'pr',
      'list',
      '--repo', // List PRs in the PR's repository, not the checkout's default remote.
      repository,
      '--state', // Open PRs, drafts included; merged and closed PRs are not stack parents.
      'open',
      '--limit', // Raise gh's default of 30 so busy repositories still find the parent.
      '1000',
      '--json', // Each PR's number, branch name, and tip commit.
      'number,headRefName,headRefOid',
    ],
    cwd,
  )
    .then((output): unknown[] => {
      const parsed: unknown = JSON.parse(output);
      return Array.isArray(parsed) ? parsed : [];
    })
    .catch((): unknown[] => []);

  return pulls
    .filter(isPullTip)
    .filter(
      (other) =>
        other.number !== pr.number && other.headRefName !== pr.headRefName,
    )
    .map((other) => ({
      ref: `refs/pull/${other.number}/head`,
      sha: other.headRefOid,
      name: other.headRefName,
    }));
}

/** Import a GitHub PR's commits and metadata into a local repo for review. */
export async function importPR(
  repo: string,
  value: string,
  base?: string,
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
    '--atomic', // Update both refs or neither, so head and base never mismatch.
    '--no-tags', // Skip the remote's tags
    '--no-write-fetch-head', // Leave .git/FETCH_HEAD for the user's own fetches.
    remote,
    // GitHub exposes the PR's head here, including PRs originating from forks.
    `+refs/pull/${details.number}/head:${namespace}/head`,
    // Fetch the target branch so the review can compute its merge base with the PR.
    `+refs/heads/${details.baseRefName}:${namespace}/base`,
  ]);

  // Compare against the nearest open PR or fetched branch whose tip is on this
  // PR's first-parent history.
  let comparisonBaseRefName = details.baseRefName;
  const [headSha, baseSha] = await Promise.all([
    resolveRef(root, `${namespace}/head`),
    resolveRef(root, `${namespace}/base`),
  ]);

  const [pullTips, branches] = await Promise.all([
    activePullTips(match[1], details, root),
    // A stale copy of the PR's own branch sits in its history but it cannot be the parent so exclude it.
    branchTips(root, headSha, [details.headRefName]),
  ]);

  const parent = await stackBase(root, headSha, baseSha, [
    ...pullTips,
    ...branches,
  ]);

  if (parent) {
    await runGitText(root, ['update-ref', `${namespace}/base`, parent.sha]);
    comparisonBaseRefName = parent.name;
  }

  // Diff against the user's chosen base if given, otherwise the automatic one.
  let comparisonBase = `${namespace}/base`;
  if (base?.trim()) {
    comparisonBase = (await resolveReviewRef(root, base, 'base', remote)).ref;
    await refreshRemoteRef(root, comparisonBase, remote);
  }

  // Store the author's login as a string instead of gh's nested author object.
  const metadata: PullRequest = {
    ...details,
    author: details.author?.login || 'Unknown author',
    comparisonBaseRefName,
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
    base: comparisonBase,
    head: `${namespace}/head`,
    mode: 'branch',
  };
}

/**
 * Re-import the PR a review was opened from so it picks up new pushes,
 * keeping the review's explicit base, mode and commit range.
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

  const base = review.base !== prBaseOf(review.head) ? review.base : undefined;
  return {
    ...(await importPR(review.repo, url, base)),
    mode: review.mode,
    commitRange: review.commitRange,
  };
}
