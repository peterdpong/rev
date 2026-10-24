export type Provider = 'codex' | 'claude';
export type ReviewRequest = { repo: string; base?: string; head?: string; mode?: 'branch' | 'working' };
export type ChangedFile = { path: string; oldPath?: string; status: string; added: number; removed: number; binary: boolean };
export type Review = Required<ReviewRequest> & {
  name: string; branch: string; baseSha: string; headSha: string; mergeBase: string;
  branches: string[]; files: ChangedFile[]; dirty: boolean; key: string;
};
export type Selection = { path: string; side: 'additions' | 'deletions'; start: number; end: number };
export type FilePatch = { patch: string; digest: string; tooLarge: boolean };
export type Comment = Selection & { id: string; body: string; headSha: string; mergeBase: string; digest: string; createdAt: string; status: 'queued' | 'sent' };
export type ReviewState = { comments: Comment[]; viewed: Record<string, string> };
export type AgentRequest = { review: ReviewRequest; provider: Provider; mode: 'ask' | 'apply'; question?: string; selection?: Selection; commentIds?: string[] };
export type AgentEvent = { kind: 'message' | 'activity' | 'error'; text: string };
export type AgentRun = { id: string; status: 'running' | 'completed' | 'failed' | 'cancelled'; events: AgentEvent[]; startedAt: string; mode: 'ask' | 'apply' };
export type ReviewTab = { id: string; label: string; kind: 'branch' | 'pr'; request: ReviewRequest; file?: string };
export type Project = { repo: string; name: string; collapsed: boolean; tabs: ReviewTab[] };
export type Workspace = { projects: Project[]; activeTabId?: string };
export type WorkspaceAction =
  | { type: 'closeTab'; id: string }
  | { type: 'collapseProject'; repo: string; collapsed: boolean }
  | { type: 'removeProject'; repo: string }
  | { type: 'setFile'; id: string; file: string };
