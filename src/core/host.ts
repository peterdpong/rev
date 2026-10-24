import path from 'node:path';
import { homedir } from 'node:os';
import { inspect, filePatch, fetchRepo, importPR, listCommits } from './git';
import { readState, addComment, mutateState } from './store';
import { AgentRunner, preparePrompt, providers } from './agents';
import { createDemo } from './demo';
import { readWorkspace, rememberReview, updateWorkspace, applyWorkspaceAction } from './workspace';
import type { ReviewRequest } from '../shared';
import { flushRunHistory } from './threads';

export class HostService {
  readonly runner = new AgentRunner();
  private sample: string | undefined;
  private closed = false;
  readonly dataDir: string;
  constructor(readonly initial?: ReviewRequest, dataDir?: string) {
    this.dataDir = dataDir || process.env.PATCH_DATA_DIR || process.env.REVUE_DATA_DIR || path.join(homedir(), '.revue');
  }
  async call(method: string, p: any = {}): Promise<unknown> {
    if (this.closed) throw new Error('The local service is closing.');
    const runner = this.runner; const dataDir = this.dataDir; const options = { initial: this.initial };
    let result: unknown;
    switch (method) {
      case 'init': result = { initial: options.initial, providers: await providers(), workspace: await readWorkspace(dataDir) }; break;
      case 'workspaceAction': result = await updateWorkspace(dataDir, workspace => applyWorkspaceAction(workspace, p)); break;
      case 'commits': result = await listCommits(p.review, p.offset); break;
      case 'review': { const review = await inspect(p); const workspace = p.remember ? await updateWorkspace(dataDir, workspace => { rememberReview(workspace, review); }) : undefined; result = { review, state: await readState(review), workspace }; break; }
      case 'patch': result = await filePatch(await inspect(p.review), p.path); break;
      case 'comment': result = await addComment(await inspect(p.review), p.selection, p.body, p.digest); break;
      case 'deleteComment': result = await mutateState(await inspect(p.review), s => { s.comments = s.comments.filter(c => c.id !== p.id); }); break;
      case 'viewed': { const review = await inspect(p.review); const patch = await filePatch(review, p.path); if (p.digest && patch.digest !== p.digest) throw new Error('This file changed while you were reviewing it. Refresh before marking it viewed.'); result = await mutateState(review, s => { if (p.viewed) s.viewed[p.path] = patch.digest; else delete s.viewed[p.path]; }); break; }
      case 'fetch': {
        if (p.review) {
          const review = await inspect(p.review);
          const imported = /^refs\/revue\/([^/]+\/[^/]+)\/(\d+)\/head$/.exec(review.head);
          const url = review.pullRequest?.url || (imported ? `https://github.com/${imported[1]}/pull/${imported[2]}` : undefined);
          if (url) { result = { request: { ...await importPR(review.repo, url), mode: p.review.mode || "branch", commitRange: p.review.commitRange } }; break; }
        }
        await fetchRepo(p.repo); result = true; break;
      }
      case 'pr': result = await importPR(p.repo, p.value); break;
      case 'demo': this.sample ||= await createDemo(); result = { repo: this.sample, base: 'main', head: 'HEAD' }; break;
      case 'prompt': result = { prompt: (await preparePrompt(p)).prompt }; break;
      case 'run': {
        const run = await runner.start(p);
        if (this.closed) runner.cancel(run.id);
        result = run; break;
      }
      case 'runStatus': await flushRunHistory(p.id); result = runner.runs.get(p.id); if (!result) throw new Error('This run is no longer available.'); break;
      case 'cancel': runner.cancel(p.id); result = true; break;
      default: throw new Error('Unknown action.');
    }

    return result;
  }
  close() { this.closed = true; this.runner.stopAll(); }
}
