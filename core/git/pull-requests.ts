import { mkdir, readFile, rename, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { hash } from './util';
import { runGitText as git, runCommandText as command } from './command';
import { rootOf, resolveRef, defaultBase } from './refs';
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
    remote,
    `+refs/pull/${details.number}/head:${namespace}/head`,
    `+refs/heads/${details.baseRefName}:${namespace}/base`,
  ]);
  const metadata: PullRequest = {
    ...details,
    author: details.author?.login || 'Unknown author',
  };
  const file = await pullRequestPath(root, `${namespace}/head`);
  await mkdir(path.dirname(file), { recursive: true });
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
