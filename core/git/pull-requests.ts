import { mkdir, readFile, rename, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { hash } from './util';
import { runGitText as git, runCommandText as command } from './command';
import {
  rootOf,
  resolveRef,
  defaultBase,
  resolveReviewRef,
  stackBase,
  type BaseCandidate,
} from './refs';
import type { ReviewRequest, PullRequest } from './types';

export async function fetchRepo(repo: string) {
  const root = await rootOf(repo);
  await git(root, ['fetch', '--all', '--prune']);
}

async function pullRequestPath(repo: string, head: string) {
  const dir = (await git(repo, ['rev-parse', '--absolute-git-dir'])).trim();
  return path.join(dir, 'revue', `pr-${hash(head).slice(0, 24)}.json`);
}

export async function readPullRequest(
  repo: string,
  head: string,
): Promise<PullRequest | undefined> {
  if (!head.startsWith('refs/revue/')) {
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
  const details = JSON.parse(
    await command(
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
  const match = /^https:\/\/github\.com\/([^/]+\/[^/]+)\/pull\/(\d+)$/.exec(
    details.url,
  );
  if (!match) {
    throw new Error('Only github.com PRs are supported in this prototype.');
  }
  const remote = `https://github.com/${match[1]}.git`;
  const namespace = `refs/revue/${match[1]}/${details.number}`;
  // Fetch exact PR refs into a private namespace; never switch the user's branch.
  await git(root, [
    'fetch',
    '--atomic',
    '--no-tags',
    '--no-write-fetch-head',
    remote,
    `+refs/pull/${details.number}/head:${namespace}/head`,
    `+refs/heads/${details.baseRefName}:${namespace}/base`,
  ]);
  let comparisonBaseRefName = details.baseRefName;
  // Use advertised PR tips present in the fetched first-parent history.
  let candidates: BaseCandidate[] = [];
  try {
    const pulls = JSON.parse(
      await command(
        'gh',
        [
          'pr',
          'list',
          '--repo',
          match[1],
          '--state',
          'open',
          '--limit',
          '1000',
          '--json',
          'number,headRefName,headRefOid',
        ],
        root,
      ),
    );
    if (Array.isArray(pulls)) {
      candidates = pulls
        .filter(
          (pr) =>
            pr.number !== details.number &&
            pr.headRefName !== details.headRefName &&
            typeof pr.headRefOid === 'string' &&
            typeof pr.headRefName === 'string',
        )
        .map((pr) => ({
          ref: `refs/pull/${pr.number}/head`,
          sha: pr.headRefOid,
          name: pr.headRefName,
        }));
    }
  } catch {
    /* PR listing may be unavailable; retain GitHub's declared base. */
  }
  const [headSha, baseSha] = await Promise.all([
    resolveRef(root, `${namespace}/head`),
    resolveRef(root, `${namespace}/base`),
  ]);
  const parent = await stackBase(root, headSha, baseSha, candidates);
  if (parent) {
    await git(root, ['update-ref', `${namespace}/base`, parent.sha]);
    comparisonBaseRefName = parent.name;
  }
  const comparison = base?.trim()
    ? await resolveReviewRef(root, base, 'base', remote)
    : { ref: `${namespace}/base` };
  if (base?.trim()) {
    // Refresh a remote comparison too, while leaving explicitly selected local
    // branches alone. An import in another tab must not reset the automatic base.
    const remotes = (await git(root, ['remote']))
      .trim()
      .split('\n')
      .filter(Boolean);
    const comparisonRef = (
      await git(root, [
        'rev-parse',
        '--symbolic-full-name',
        '--verify',
        '--end-of-options',
        comparison.ref,
      ])
    ).trim();
    const source = remotes
      .sort((a, b) => b.length - a.length)
      .find((name) => comparisonRef.startsWith(`refs/remotes/${name}/`));
    const privatePrefix = `refs/revue/bases/${hash(remote).slice(0, 16)}/`;
    const branch = source
      ? comparisonRef.slice(`refs/remotes/${source}/`.length)
      : comparisonRef.startsWith(privatePrefix)
        ? comparisonRef.slice(privatePrefix.length)
        : undefined;
    if (branch) {
      await git(root, [
        'fetch',
        '--no-tags',
        '--no-write-fetch-head',
        '--',
        source || remote,
        `+refs/heads/${branch}:${comparisonRef}`,
      ]);
    }
  }
  const metadata: PullRequest = {
    ...details,
    author: details.author?.login || 'Unknown author',
    comparisonBaseRefName,
  };
  const file = await pullRequestPath(root, `${namespace}/head`);
  await mkdir(path.dirname(file), { recursive: true });
  const temporary = `${file}.${process.pid}.${Date.now()}`;
  await writeFile(temporary, JSON.stringify(metadata), { mode: 0o600 });
  await rename(temporary, file);
  return {
    repo: root,
    base: comparison.ref,
    head: `${namespace}/head`,
    mode: 'branch',
  };
}
