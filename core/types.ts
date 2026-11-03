export type * from './git/types';

import type { AgentReviewState } from './agent-types';
export type ReviewState = { viewed: Record<string, string> } & AgentReviewState;

import type { ReviewRequest } from './git/types';
export type ReviewTab = {
  id: string;
  scopeKey?: string;
  label: string;
  kind: 'current' | 'branch' | 'pr';
  currentHeadSha?: string;
  request: ReviewRequest;
  file?: string;
};

export type Project = {
  repo: string;
  name: string;
  collapsed: boolean;
  tabs: ReviewTab[];
};

export type Workspace = { projects: Project[]; activeTabId?: string };

export type WorkspaceAction =
  | { type: 'closeTab'; id: string }
  | { type: 'collapseProject'; repo: string; collapsed: boolean }
  | { type: 'removeProject'; repo: string }
  | { type: 'setFile'; id: string; file: string };

export type * from './agent-types';
