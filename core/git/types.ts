export type ReviewMode = 'branch' | 'working';

export type ReviewRequest = {
  repo: string;
  base?: string;
  head?: string;
  mode?: ReviewMode;
  commitRange?: CommitRange;
};

/** Git's change letter, or '?' for an untracked file in a working review. */
export type ChangeStatus = 'A' | 'M' | 'D' | 'R' | 'C' | 'T' | 'U' | 'X' | '?';

export type ChangedFile = {
  path: string;
  oldPath?: string;
  status: ChangeStatus;
  added: number;
  removed: number;
  binary: boolean;
  /** Only untracked local files excluded by Git's ignore rules are marked. */
  ignored?: boolean;
};

export type Review = Required<Omit<ReviewRequest, 'commitRange'>> & {
  name: string;
  branch: string;
  baseSha: string;
  headSha: string;
  mergeBase: string;
  branches: string[];
  files: ChangedFile[];
  dirty: boolean;
  key: string;
  commitRange?: CommitRange;
  commitCount: number;
  branchHeadSha: string;
  scopeKey: string;
  headCommit?: CommitDetails;
  pullRequest?: PullRequest;
};

/** When `tooLarge` is true, `patch` is empty and `digest` covers the content IDs. */
export type FilePatch = { patch: string; digest: string; tooLarge: boolean };

export type ReviewPatch = FilePatch & { path: string; error?: string };

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
  updatedAt: string;
};
