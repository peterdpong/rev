import { mkdir, open, readFile, rename, unlink, writeFile } from 'node:fs/promises';
import { randomUUID } from 'node:crypto';
import path from 'node:path';
import type { Review, ReviewTab, Workspace, WorkspaceAction } from '../shared';

export function rememberReview(workspace: Workspace, review: Review): ReviewTab {
  let project = workspace.projects.find(p => p.repo === review.repo);
  if (!project) {
    project = { repo: review.repo, name: review.name, collapsed: false, tabs: [] };
    workspace.projects.push(project);
  }
  const head = review.head === 'HEAD' ? (review.branch === 'detached HEAD' ? review.headSha : review.branch) : review.head;
  const pr = /^refs\/revue\/[^/]+\/[^/]+\/(\d+)\/head$/.exec(head);
  const existing = project.tabs.find(t => t.id === review.key);
  const tab: ReviewTab = {
    id: review.key, kind: pr ? 'pr' : 'branch', label: pr ? `PR #${pr[1]}` : head.replace(/^refs\/(heads|remotes)\//, ''),
    request: { repo: review.repo, base: review.base, head, mode: review.mode }, file: existing?.file,
  };
  if (existing) Object.assign(existing, tab); else project.tabs.push(tab);
  project.collapsed = false;
  workspace.activeTabId = tab.id;
  return tab;
}

export function applyWorkspaceAction(workspace: Workspace, action: WorkspaceAction) {
  if (action.type === 'closeTab') {
    const before = workspace.projects.flatMap(p => p.tabs); const index = before.findIndex(t => t.id === action.id);
    workspace.projects.forEach(p => { p.tabs = p.tabs.filter(t => t.id !== action.id); });
    if (workspace.activeTabId === action.id) {
      const remaining = workspace.projects.flatMap(p => p.tabs);
      workspace.activeTabId = remaining[Math.min(Math.max(index - 1, 0), remaining.length - 1)]?.id;
    }
  } else if (action.type === 'removeProject') {
    workspace.projects = workspace.projects.filter(p => p.repo !== action.repo);
    if (!workspace.projects.some(p => p.tabs.some(t => t.id === workspace.activeTabId))) workspace.activeTabId = workspace.projects.flatMap(p => p.tabs)[0]?.id;
  } else if (action.type === 'collapseProject') {
    const project = workspace.projects.find(p => p.repo === action.repo); if (project) project.collapsed = Boolean(action.collapsed);
  } else if (action.type === 'setFile') {
    if (typeof action.file !== 'string' || action.file.length > 4096) throw new Error('Invalid file path.');
    const tab = workspace.projects.flatMap(p => p.tabs).find(t => t.id === action.id); if (tab) tab.file = action.file;
  } else throw new Error('Unknown workspace action.');
}

export async function readWorkspace(dir: string): Promise<Workspace> {
  try { return JSON.parse(await readFile(path.join(dir, 'workspace.json'), 'utf8')); }
  catch (e: any) { if (e.code === 'ENOENT') return { projects: [] }; throw e; }
}

export async function updateWorkspace(dir: string, change: (workspace: Workspace) => void): Promise<Workspace> {
  await mkdir(dir, { recursive: true });
  const file = path.join(dir, 'workspace.json'); const lockPath = file + '.lock';
  let lock;
  for (let i = 0; i < 100; i++) {
    try { lock = await open(lockPath, 'wx'); break; }
    catch (error: any) { if (error.code !== 'EEXIST') throw error; await new Promise(r => setTimeout(r, 20)); }
  }
  if (!lock) throw new Error('The workspace is busy. Try again.');
  const temporary = `${file}.${randomUUID()}`;
  try {
    const workspace = await readWorkspace(dir); change(workspace);
    await writeFile(temporary, JSON.stringify(workspace, null, 2), { mode: 0o600 }); await rename(temporary, file);
    return workspace;
  } finally { await lock.close(); await unlink(lockPath); await unlink(temporary).catch(() => {}); }
}
