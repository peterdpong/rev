import {
  inspect,
  filePatch,
  reviewPatches,
  fetchRepo,
  importPR,
  refreshPR,
  listCommits,
  listBranches,
  checkouts,
} from './git';
import { readState, mutateState } from './store';
import type {
  Checkout,
  CommitPage,
  FilePatch,
  Review,
  ReviewPatch,
  ReviewRequest,
  ReviewState,
} from './types';

type HostParams = {
  init: void;
  checkouts: { repos: string[] };
  branches: { repo: string };
  commits: { review: ReviewRequest; offset?: number };
  review: ReviewRequest & { patches?: boolean; preview?: boolean };
  patch: {
    review: ReviewRequest;
    path: string;
    headSha?: string;
    mergeBase?: string;
  };
  patches: { review: ReviewRequest; headSha: string; mergeBase: string };
  viewed: {
    review: ReviewRequest;
    path: string;
    viewed: boolean;
    digest?: string;
    preview?: boolean;
  };
  fetch: { repo: string; review?: ReviewRequest };
  pr: { repo: string; value: string };
};

type HostResults = {
  init: { initial: ReviewRequest | undefined };
  checkouts: (Checkout | null)[];
  branches: string[];
  commits: CommitPage;
  review: { review: Review; patches?: ReviewPatch[]; state: ReviewState };
  patch: FilePatch;
  patches: ReviewPatch[];
  viewed: ReviewState;
  fetch: { request: ReviewRequest } | true;
  pr: ReviewRequest;
};

type HostMethod = keyof HostParams;

type HostCall =
  | { [M in HostMethod]: [method: M, params: HostParams[M]] }[HostMethod]
  | [method: 'init'];

const REPO_METHODS: ReadonlySet<string> = new Set<HostMethod>([
  'review',
  'branches',
  'commits',
  'patch',
  'patches',
  'viewed',
  'fetch',
  'pr',
]);

function repoOf(params: unknown): unknown {
  if (typeof params !== 'object' || params === null) {
    return undefined;
  }
  if ('review' in params && params.review) {
    return repoOf(params.review);
  }
  return 'repo' in params ? params.repo : undefined;
}

export class HostService {
  private closed = false;
  constructor(readonly initial?: ReviewRequest) {}
  call(method: 'init'): Promise<HostResults['init']>;
  call<M extends HostMethod>(
    method: M,
    params: HostParams[M],
  ): Promise<HostResults[M]>;
  async call(...[method, p]: HostCall): Promise<HostResults[HostMethod]> {
    if (this.closed) {
      throw new Error('The local service is closing.');
    }

    // Repository operations require an explicit folder.
    if (REPO_METHODS.has(method)) {
      const repo = repoOf(p);
      if (typeof repo !== 'string' || !repo.trim()) {
        throw new Error('Pass a repository folder.');
      }
    }

    let result: HostResults[HostMethod];
    switch (method) {
      case 'init':
        result = { initial: this.initial };
        break;
      case 'checkouts':
        result = await checkouts(p.repos);
        break;
      case 'branches':
        result = await listBranches(p.repo);
        break;
      case 'commits':
        result = await listCommits(p.review, p.offset);
        break;
      case 'review': {
        const review = await inspect(p);
        // The desktop validates viewed markers against patches as they load.
        // Revalidating every marker here would eagerly read deferred diffs.
        result = {
          review,
          ...(p.patches !== false
            ? { patches: await reviewPatches(review) }
            : {}),
          state: await readState(review, p.preview !== true),
        };
        break;
      }
      case 'patch': {
        const review = await inspect(p.review);
        if (
          (p.headSha !== undefined || p.mergeBase !== undefined) &&
          (p.headSha !== review.headSha || p.mergeBase !== review.mergeBase)
        ) {
          throw new Error(
            'The comparison changed while loading. Refresh the review.',
          );
        }

        result = await filePatch(review, p.path);
        break;
      }
      case 'patches': {
        const review = await inspect(p.review);
        if (p.headSha !== review.headSha || p.mergeBase !== review.mergeBase) {
          throw new Error(
            'The comparison changed while loading. Refresh the review.',
          );
        }

        result = await reviewPatches(review);
        break;
      }
      case 'viewed': {
        const review = await inspect(p.review);
        const patch = await filePatch(review, p.path);
        if (p.digest && patch.digest !== p.digest) {
          throw new Error(
            'This file changed while you were reviewing it. Refresh before marking it viewed.',
          );
        }

        result = await mutateState(
          review,
          (s) => {
            if (p.viewed) {
              s.viewed[p.path] = patch.digest;
            } else {
              delete s.viewed[p.path];
            }
          },
          p.preview !== true,
        );
        break;
      }
      case 'fetch': {
        const request = p.review && (await refreshPR(await inspect(p.review)));
        if (request) {
          result = { request };
          break;
        }
        await fetchRepo(p.repo);
        result = true;
        break;
      }
      case 'pr':
        result = await importPR(p.repo, p.value);
        break;
      default:
        throw new Error('Unknown action.');
    }

    return result;
  }
  close() {
    this.closed = true;
  }
}
