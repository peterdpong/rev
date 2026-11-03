import {
  mkdir,
  open,
  readFile,
  rename,
  unlink,
  writeFile,
} from 'node:fs/promises';
import { randomUUID } from 'node:crypto';
import path from 'node:path';
import type {
  Project,
  Review,
  ReviewRequest,
  ReviewTab,
  Workspace,
  WorkspaceAction,
} from './types';
import { git, hash, resolveRef, reviewKey } from './git';

function currentTab(project: Project, branch: string, sha: string): ReviewTab {
  const label =
    branch === 'detached HEAD' ? `Detached ${sha.slice(0, 7)}` : branch;
  let tab = project.tabs.find((t) => t.kind === 'current');
  if (!tab) {
    tab = {
      id: `current-${hash(project.repo).slice(0, 24)}`,
      kind: 'current',
      label,
      currentHeadSha: sha,
      request: { repo: project.repo, head: 'HEAD', mode: 'branch' },
    };
    project.tabs.unshift(tab);
  } else {
    if (tab.label !== label) {
      delete tab.request.commitRange;
      delete tab.scopeKey;
      delete tab.file;
    }
    tab.label = label;
    tab.currentHeadSha = sha;
  }
  return tab;
}

// Upgrade saved projects and refresh only their checkout metadata. Diff/history
// requests remain pinned to the reviewed branch, not this navigation entry.
export async function refreshWorkspace(dir: string): Promise<Workspace> {
  const workspace = await readWorkspace(dir);
  const checkouts = await Promise.all(
    workspace.projects.map(async (project) => {
      try {
        const [branch, sha] = await Promise.all([
          git(project.repo, ['branch', '--show-current']),
          resolveRef(project.repo, 'HEAD'),
        ]);
        return {
          repo: project.repo,
          branch: branch.trim() || 'detached HEAD',
          sha,
        };
      } catch {
        return undefined;
      } // Keep unavailable projects and their saved reviews.
    }),
  );
  const apply = (value: Workspace) => {
    for (const checkout of checkouts) {
      if (!checkout) {
        continue;
      }
      const project = value.projects.find((p) => p.repo === checkout.repo);
      if (project) {
        currentTab(project, checkout.branch, checkout.sha);
      }
    }
  };
  const updated = structuredClone(workspace);
  apply(updated);
  if (JSON.stringify(updated) === JSON.stringify(workspace)) {
    return workspace;
  }
  return updateWorkspace(dir, apply);
}

export function rememberReview(
  workspace: Workspace,
  review: Review,
  current?: ReviewRequest,
): ReviewTab {
  let project = workspace.projects.find((p) => p.repo === review.repo);
  if (!project) {
    project = {
      repo: review.repo,
      name: review.name,
      collapsed: false,
      tabs: [],
    };
    workspace.projects.push(project);
  }
  if (current) {
    const tab = currentTab(project, review.branch, review.branchHeadSha);
    tab.scopeKey = review.scopeKey;
    tab.request = {
      repo: review.repo,
      head: 'HEAD',
      mode: review.mode,
      ...(current.base ? { base: current.base } : {}),
      ...(review.commitRange ? { commitRange: review.commitRange } : {}),
    };
    project.collapsed = false;
    workspace.activeTabId = tab.id;
    return tab;
  }
  const head =
    review.head === 'HEAD'
      ? review.branch === 'detached HEAD'
        ? review.branchHeadSha
        : review.branch
      : review.head;
  const pr = /^refs\/revue\/[^/]+\/[^/]+\/(\d+)\/head$/.exec(head);
  // Branch and working-tree views share a navigation tab but keep separate review state.
  const tabId = reviewKey(review.repo, 'branch', review.base, head);
  const legacyId = hash([review.repo, review.base, head].join('\0')).slice(
    0,
    24,
  );
  const existing = project.tabs.find(
    (t) => t.id === tabId || t.id === legacyId,
  );
  const tab: ReviewTab = {
    id: tabId,
    scopeKey: review.scopeKey,
    kind: pr ? 'pr' : 'branch',
    label: pr ? `PR #${pr[1]}` : head.replace(/^refs\/(heads|remotes)\//, ''),
    request: {
      repo: review.repo,
      base: review.base,
      head,
      mode: review.mode,
      ...(review.commitRange ? { commitRange: review.commitRange } : {}),
    },
    file: existing?.file,
  };
  if (existing) {
    Object.assign(existing, tab);
  } else {
    project.tabs.push(tab);
  }
  project.collapsed = false;
  workspace.activeTabId = tab.id;
  return tab;
}

export function applyWorkspaceAction(
  workspace: Workspace,
  action: WorkspaceAction,
) {
  if (action.type === 'closeTab') {
    if (
      workspace.projects.some((p) =>
        p.tabs.some((t) => t.id === action.id && t.kind === 'current'),
      )
    ) {
      return;
    }
    const before = workspace.projects.flatMap((p) => p.tabs);
    const index = before.findIndex((t) => t.id === action.id);
    workspace.projects.forEach((p) => {
      p.tabs = p.tabs.filter((t) => t.id !== action.id);
    });
    if (workspace.activeTabId === action.id) {
      const remaining = workspace.projects.flatMap((p) => p.tabs);
      workspace.activeTabId =
        remaining[Math.min(Math.max(index - 1, 0), remaining.length - 1)]?.id;
    }
  } else if (action.type === 'removeProject') {
    workspace.projects = workspace.projects.filter(
      (p) => p.repo !== action.repo,
    );
    if (
      !workspace.projects.some((p) =>
        p.tabs.some((t) => t.id === workspace.activeTabId),
      )
    ) {
      workspace.activeTabId = workspace.projects.flatMap((p) => p.tabs)[0]?.id;
    }
  } else if (action.type === 'collapseProject') {
    const project = workspace.projects.find((p) => p.repo === action.repo);
    if (project) {
      project.collapsed = Boolean(action.collapsed);
    }
  } else if (action.type === 'setFile') {
    if (typeof action.file !== 'string' || action.file.length > 4096) {
      throw new Error('Invalid file path.');
    }
    const tab = workspace.projects
      .flatMap((p) => p.tabs)
      .find((t) => t.id === action.id);
    if (tab) {
      tab.file = action.file;
    }
  } else {
    throw new Error('Unknown workspace action.');
  }
}

export async function readWorkspace(dir: string): Promise<Workspace> {
  try {
    return JSON.parse(await readFile(path.join(dir, 'workspace.json'), 'utf8'));
  } catch (e: any) {
    if (e.code === 'ENOENT') {
      return { projects: [] };
    }
    throw e;
  }
}

export async function updateWorkspace(
  dir: string,
  change: (workspace: Workspace) => void,
): Promise<Workspace> {
  await mkdir(dir, { recursive: true });
  const file = path.join(dir, 'workspace.json');
  const lockPath = file + '.lock';
  let lock;
  for (let i = 0; i < 100; i++) {
    try {
      lock = await open(lockPath, 'wx');
      break;
    } catch (error: any) {
      if (error.code !== 'EEXIST') {
        throw error;
      }
      await new Promise((r) => setTimeout(r, 20));
    }
  }
  if (!lock) {
    throw new Error('The workspace is busy. Try again.');
  }
  const temporary = `${file}.${randomUUID()}`;
  try {
    const workspace = await readWorkspace(dir);
    change(workspace);
    await writeFile(temporary, JSON.stringify(workspace, null, 2), {
      mode: 0o600,
    });
    await rename(temporary, file);
    return workspace;
  } finally {
    await lock.close();
    await unlink(lockPath);
    await unlink(temporary).catch(() => {});
  }
}
