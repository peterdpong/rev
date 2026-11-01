export type ReviewMode = 'branch' | 'working';

export type ReviewRequest = {
  repo: string;
  base?: string;
  head?: string;
  mode?: ReviewMode;
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

export type Review = Required<ReviewRequest> & {
  name: string;
  branch: string;
  baseSha: string;
  headSha: string;
  mergeBase: string;
  branches: string[];
  files: ChangedFile[];
  dirty: boolean;
  key: string;
};

/** When `tooLarge` is true, `patch` and `digest` are empty. */
export type FilePatch = { patch: string; digest: string; tooLarge: boolean };

export type ReviewPatch = FilePatch & { path: string; error?: string };
