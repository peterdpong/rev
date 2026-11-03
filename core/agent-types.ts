import type { ReviewRequest } from './git/types';

export type Provider = 'codex' | 'claude';

export type Selection = {
  scope?: 'lines' | 'file' | 'review';
  path: string;
  side: 'additions' | 'deletions';
  start: number;
  end: number;
};

export type Comment = Selection & {
  id: string;
  body: string;
  headSha: string;
  mergeBase: string;
  digest: string;
  createdAt: string;
  status: 'queued' | 'sent';
};

export type AgentSession = {
  id: string;
  headSha: string;
  mergeBase: string;
  updatedAt: string;
};

export type AgentRequest = {
  review: ReviewRequest;
  provider: Provider;
  mode: 'ask' | 'apply';
  question?: string;
  selection?: Selection;
  selectionDigest?: string;
  commentIds?: string[];
  threadId?: string;
  newSession?: boolean;
};

export type AgentEvent = {
  kind: 'message' | 'activity' | 'error';
  text: string;
  id?: string;
};

export type AgentApproval = { id: string; title: string; detail: string };

export type AgentRun = {
  id: string;
  status: 'queued' | 'running' | 'completed' | 'failed' | 'cancelled';
  events: AgentEvent[];
  approvals: AgentApproval[];
  sessionId?: string;
  startedAt: string;
  mode: 'ask' | 'apply';
  thread?: ReviewThread;
  scopeKey?: string;
  worktree?: AgentWorktree;
};

export type ThreadTurn = {
  id: string;
  question: string;
  provider: Provider;
  startedAt: string;
  status: AgentRun['status'];
  events: AgentEvent[];
  worktree?: AgentWorktree;
};

export type AgentWorktree = { path: string; branch: string };

export type ReviewThread = {
  id: string;
  archived?: boolean;
  kind: 'question' | 'change';
  body: string;
  selection?: Selection;
  digest?: string;
  headSha: string;
  mergeBase: string;
  createdAt: string;
  updatedAt: string;
  commentIds: string[];
  changes?: Comment[];
  turns: ThreadTurn[];
};

export type AgentReviewState = {
  comments: Comment[];
  threads?: ReviewThread[];
  sessions?: Record<string, AgentSession>;
};
