import { AgentRunner, preparePrompt, providers } from './agents';
import { flushRunHistory } from './threads';
import path from 'node:path';
import { homedir } from 'node:os';
import {
  inspect,
  filePatch,
  reviewPatches,
  fetchRepo,
  importPR,
  listCommits,
  listBranches,
} from './git';
import { readState, addComment, mutateState } from './store';
import { createDemo } from './demo';
import {
  refreshWorkspace,
  rememberReview,
  updateWorkspace,
  applyWorkspaceAction,
} from './workspace';
import type { ReviewRequest } from './types';

export class HostService {
  readonly runner: AgentRunner;
  private sample: string | undefined;
  private closed = false;
  readonly dataDir: string;
  constructor(
    readonly initial?: ReviewRequest,
    dataDir?: string,
  ) {
    this.dataDir =
      dataDir ||
      process.env.REV_DATA_DIR ||
      process.env.REVUE_DATA_DIR ||
      path.join(homedir(), '.revue');
    this.runner = new AgentRunner(
      undefined,
      path.join(this.dataDir, 'worktrees'),
    );
  }
  async call(method: string, p: any = {}): Promise<unknown> {
    if (this.closed) {
      throw new Error('The local service is closing.');
    }
    const dataDir = this.dataDir;
    const runner = this.runner;
    const options = { initial: this.initial };
    let result: unknown;
    switch (method) {
      case 'init':
        result = {
          initial: options.initial,
          providers: await providers(),
          workspace: await refreshWorkspace(dataDir),
        };
        break;
      case 'workspace':
        result = await refreshWorkspace(dataDir);
        break;
      case 'branches':
        result = await listBranches(p.repo);
        break;
      case 'workspaceAction':
        result = await updateWorkspace(dataDir, (workspace) =>
          applyWorkspaceAction(workspace, p),
        );
        break;
      case 'commits':
        result = await listCommits(p.review, p.offset);
        break;
      case 'review': {
        if (p.current && p.head && p.head !== 'HEAD') {
          throw new Error('The Current branch tab must follow HEAD.');
        }
        const review = await inspect(p);
        const workspace = p.remember
          ? await updateWorkspace(dataDir, (workspace) => {
              rememberReview(workspace, review, p.current ? p : undefined);
            })
          : undefined;
        // The desktop validates viewed markers against patches as they load.
        // Revalidating every marker here would eagerly read deferred diffs.
        result = {
          review,
          ...(p.remember === undefined && p.preview !== true
            ? { patches: await reviewPatches(review) }
            : {}),
          state: await readState(review, p.preview !== true),
          workspace,
        };
        break;
      }
      case 'patch': {
        const review = await inspect(p.review || p);
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
        const review = await inspect(p.review || p);
        if (p.headSha !== review.headSha || p.mergeBase !== review.mergeBase) {
          throw new Error(
            'The comparison changed while loading. Refresh the review.',
          );
        }
        result = await reviewPatches(review, p.preview === true);
        break;
      }
      case 'comment':
        result = await addComment(
          await inspect(p.review),
          p.selection,
          p.body,
          p.digest,
        );
        break;
      case 'archiveThread':
        result = await mutateState(
          await inspect(p.review),
          (s) => {
            const thread = s.threads?.find(
              (t) => t.id === p.id && t.kind === 'question',
            );
            if (!thread) {
              throw new Error('Question thread not found.');
            }
            thread.archived = p.archived !== false;
          },
          false,
        );
        break;
      case 'deleteComment':
        result = await mutateState(await inspect(p.review), (s) => {
          s.comments = s.comments.filter((c) => c.id !== p.id);
        });
        break;
      case 'viewed': {
        const review = await inspect(p.review || p);
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
        if (p.review) {
          const review = await inspect(p.review || p);
          const imported = /^refs\/revue\/([^/]+\/[^/]+)\/(\d+)\/head$/.exec(
            review.head,
          );
          const url =
            review.pullRequest?.url ||
            (imported
              ? `https://github.com/${imported[1]}/pull/${imported[2]}`
              : undefined);
          if (url) {
            const automaticBase = review.head.replace(/\/head$/, '/base');
            const base =
              p.review.base && p.review.base !== automaticBase
                ? p.review.base
                : undefined;
            result = {
              request: {
                ...(await importPR(review.repo, url, base)),
                mode: p.review.mode || 'branch',
                commitRange: p.review.commitRange,
              },
            };
            break;
          }
        }
        await fetchRepo(p.repo);
        result = true;
        break;
      }
      case 'pr':
        result = await importPR(p.repo, p.value, p.base);
        break;
      case 'demo':
        this.sample ||= await createDemo();
        result = { repo: this.sample, base: 'main', head: 'HEAD' };
        break;
      case 'prompt':
        result = { prompt: (await preparePrompt(p)).prompt };
        break;
      case 'run': {
        const run = await runner.start(p);
        if (this.closed) {
          runner.cancel(run.id);
        }
        result = run;
        break;
      }
      case 'runStatus':
        await flushRunHistory(p.id);
        result = runner.runs.get(p.id);
        if (!result) {
          throw new Error('This run is no longer available.');
        }
        break;
      case 'cancel':
        runner.cancel(p.id);
        result = true;
        break;
      case 'approve':
        runner.respondToApproval(p.id, p.approvalId, p.allow);
        result = true;
        break;
      default:
        throw new Error('Unknown action.');
    }

    return result;
  }
  close() {
    this.closed = true;
    this.runner.stopAll();
  }
}
