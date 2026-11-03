export type ReviewMode = 'branch' | 'working';
export type ChangeStatus = 'A' | 'M' | 'D' | 'R' | 'C' | 'T' | 'U' | 'X' | '?';

export type CommitRange = { start: string; end: string };

export type Commit = {
  sha: string;
  parents: string[];
  subject: string;
  author: string;
  date: string;
};

export type CommitPage = { commits: Commit[]; hasMore: boolean };

export type CommitDetails = Commit & { body: string };

export type PullRequest = {
  number: number;
  title: string;
  body: string;
  url: string;
  author: string;
  state: string;
  isDraft: boolean;
  headRefName: string;
  baseRefName: string;
  comparisonBaseRefName?: string;
  updatedAt: string;
};

export type ReviewRequest = {
  repo: string;
  base?: string;
  head?: string;
  mode?: 'branch' | 'working';
  commitRange?: CommitRange;
};

export type ChangedFile = {
  path: string;
  oldPath?: string;
  status: ChangeStatus;
  added: number;
  removed: number;
  binary: boolean;
  ignored?: boolean;
};

export type Review = Required<Omit<ReviewRequest, 'commitRange'>> & {
  commitRange?: CommitRange;
  commitCount: number;
  branchHeadSha: string;
  scopeKey: string;
  name: string;
  branch: string;
  baseSha: string;
  headSha: string;
  mergeBase: string;
  branches: string[];
  files: ChangedFile[];
  dirty: boolean;
  key: string;
  headCommit?: CommitDetails;
  pullRequest?: PullRequest;
};

export type FilePatch = {
  patch: string;
  digest: string;
  tooLarge: boolean;
  deferred?: boolean;
};
export type ReviewPatch = FilePatch & { path: string; error?: string };
