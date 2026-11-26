import { runCommandText } from './command';
import { rootOf } from './refs';
import type { PullRequest } from './types';

export const MERGE_METHODS = ['squash', 'merge', 'rebase'] as const;
export type MergeMethod = (typeof MERGE_METHODS)[number];

export type PullRequestAction =
  | { type: 'ready' }
  | { type: 'draft' }
  | { type: 'merge'; method: MergeMethod };

const PULL_REQUEST_URL = /^https:\/\/github\.com\/([^/]+\/[^/]+)\/pull\/\d+$/;
const PULL_REQUEST_FIELDS =
  'number,title,body,author,state,isDraft,headRefName,baseRefName,updatedAt,url';

function repositoryOf(url: string) {
  const match = PULL_REQUEST_URL.exec(url);
  if (!match) {
    throw new Error('Pull request actions need a github.com pull request URL.');
  }
  return match[1];
}

export function pullRequestActionArgs(
  url: string,
  action: PullRequestAction,
): string[] {
  repositoryOf(url);
  switch (action?.type) {
    case 'ready':
      return ['pr', 'ready', url];
    case 'draft':
      return ['pr', 'ready', '--undo', url];
    case 'merge':
      if (!MERGE_METHODS.includes(action.method)) {
        throw new Error('Unknown merge method.');
      }
      return ['pr', 'merge', `--${action.method}`, url];
    default:
      throw new Error('Unknown pull request action.');
  }
}

export async function mergeMethods(
  repo: string,
  url: string,
): Promise<MergeMethod[]> {
  const root = await rootOf(repo);
  const settings = JSON.parse(
    await runCommandText(
      'gh',
      [
        'repo',
        'view',
        repositoryOf(url),
        '--json',
        'squashMergeAllowed,mergeCommitAllowed,rebaseMergeAllowed',
      ],
      root,
    ),
  );
  const allowed: Record<MergeMethod, unknown> = {
    squash: settings.squashMergeAllowed,
    merge: settings.mergeCommitAllowed,
    rebase: settings.rebaseMergeAllowed,
  };
  return MERGE_METHODS.filter((method) => allowed[method] === true);
}

export async function runPullRequestAction(
  repo: string,
  url: string,
  action: PullRequestAction,
): Promise<PullRequest> {
  const root = await rootOf(repo);
  await runCommandText('gh', pullRequestActionArgs(url, action), root);
  const details = JSON.parse(
    await runCommandText(
      'gh',
      ['pr', 'view', url, '--json', PULL_REQUEST_FIELDS],
      root,
    ),
  );
  return { ...details, author: details.author?.login || 'Unknown author' };
}
