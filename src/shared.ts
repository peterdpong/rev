export type Provider = 'codex' | 'claude';
export type CommitRange = { start: string; end: string };
export type Commit = { sha: string; parents: string[]; subject: string; author: string; date: string };
export type CommitPage = { commits: Commit[]; hasMore: boolean };
export type CommitDetails = Commit & { body: string };
export type PullRequest = { number: number; title: string; body: string; url: string; author: string; state: string; isDraft: boolean; headRefName: string; baseRefName: string; updatedAt: string };
export type ReviewRequest = { repo: string; base?: string; head?: string; mode?: 'branch' | 'working'; commitRange?: CommitRange };
export type ChangedFile = { path: string; oldPath?: string; status: string; added: number; removed: number; binary: boolean };
export type Review = Required<Omit<ReviewRequest, 'commitRange'>> & {
  commitRange?: CommitRange; commitCount: number; branchHeadSha: string; scopeKey: string;
  name: string; branch: string; baseSha: string; headSha: string; mergeBase: string;
  branches: string[]; files: ChangedFile[]; dirty: boolean; key: string;
  headCommit?: CommitDetails; pullRequest?: PullRequest;
};
export type Selection = { path: string; side: 'additions' | 'deletions'; start: number; end: number };
export type FilePatch = { patch: string; digest: string; tooLarge: boolean };
export type Comment = Selection & { id: string; body: string; headSha: string; mergeBase: string; digest: string; createdAt: string; status: 'queued' | 'sent' };
export type ReviewState = { comments: Comment[]; viewed: Record<string, string>; threads?: ReviewThread[] };
export type AgentRequest = { review: ReviewRequest; provider: Provider; mode: 'ask' | 'apply'; question?: string; selection?: Selection; selectionDigest?: string; commentIds?: string[]; threadId?: string };
export type AgentEvent = { kind: 'message' | 'activity' | 'error'; text: string };
export type AgentRun = { id: string; status: 'running' | 'completed' | 'failed' | 'cancelled'; events: AgentEvent[]; startedAt: string; mode: 'ask' | 'apply'; thread?: ReviewThread };
export type ThreadTurn = { id: string; question: string; provider: Provider; startedAt: string; status: AgentRun['status']; events: AgentEvent[] };
export type ReviewThread = { id: string; kind: 'question' | 'change'; body: string; selection?: Selection; digest?: string; headSha: string; mergeBase: string; createdAt: string; updatedAt: string; commentIds: string[]; turns: ThreadTurn[] };
export type ReviewTab = { id: string; scopeKey?: string; label: string; kind: 'branch' | 'pr'; request: ReviewRequest; file?: string };
export type Project = { repo: string; name: string; collapsed: boolean; tabs: ReviewTab[] };
export type Workspace = { projects: Project[]; activeTabId?: string };
export type WorkspaceAction =
  | { type: 'closeTab'; id: string }
  | { type: 'collapseProject'; repo: string; collapsed: boolean }
  | { type: 'removeProject'; repo: string }
  | { type: 'setFile'; id: string; file: string };
