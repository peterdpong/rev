import { mkdir, readFile, writeFile, rename, open, unlink } from 'node:fs/promises';
import path from 'node:path';
import { randomUUID } from 'node:crypto';
import type { Comment, Review, ReviewState, Selection } from '../shared';
import { filePatch, git } from './git';

async function statePath(review: Review) {
  const gitDir = (await git(review.repo, ['rev-parse', '--absolute-git-dir'])).trim();
  const dir = path.join(gitDir, 'revue'); await mkdir(dir, { recursive: true });
  return path.join(dir, `${review.scopeKey}.json`);
}
export async function readState(review: Review): Promise<ReviewState> {
  try {
    const state: ReviewState = JSON.parse(await readFile(await statePath(review), 'utf8'));
    const names = Object.keys(state.viewed);
    for (let i = 0; i < names.length; i += 8) await Promise.all(names.slice(i, i + 8).map(async name => {
      try { if ((await filePatch(review, name)).digest !== state.viewed[name]) delete state.viewed[name]; }
      catch { delete state.viewed[name]; }
    }));
    return state;
  }
  catch (e: any) { if (e.code === 'ENOENT') return { comments: [], viewed: {} }; throw e; }
}
export async function mutateState(review: Review, change: (state: ReviewState) => void) {
  const file = await statePath(review); const lockPath = file + '.lock';
  let lock;
  for (let attempt = 0; attempt < 50; attempt++) {
    try { lock = await open(lockPath, 'wx'); break; }
    catch (error: any) { if (error.code !== 'EEXIST') throw error; await new Promise(r => setTimeout(r, 40)); }
  }
  if (!lock) throw new Error(`Review data is busy. If no other Revue is running, remove ${lockPath}.`);
  const temp = file + '.' + randomUUID();
  try {
    const state = await readState(review); change(state);
    await writeFile(temp, JSON.stringify(state, null, 2), { mode: 0o600 }); await rename(temp, file); return state;
  } finally { await lock.close(); await unlink(lockPath); await unlink(temp).catch(() => {}); }
}
export async function addComment(review: Review, selection: Selection, body: string, expectedDigest?: string): Promise<ReviewState> {
  if (!body?.trim() || body.length > 20_000) throw new Error('Write a comment between 1 and 20,000 characters.');
  if (!['additions', 'deletions'].includes(selection.side) || !Number.isInteger(selection.start) || !Number.isInteger(selection.end) || selection.start < 1 || selection.end < selection.start) throw new Error('Select a valid line range first.');
  const patch = await filePatch(review, selection.path);
  if (patch.tooLarge) throw new Error('This file is too large for inline comments.');
  if (expectedDigest && patch.digest !== expectedDigest) throw new Error('This file changed since you selected it. Refresh and select the lines again.');
  const valid = new Set<number>(); let oldLine = 0; let newLine = 0; let inHunk = false;
  for (const line of patch.patch.split('\n')) {
    const hunk = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(line);
    if (hunk) { oldLine = Number(hunk[1]); newLine = Number(hunk[2]); inHunk = true; continue; }
    if (!inHunk) continue;
    if (line.startsWith(' ')) { valid.add(selection.side === 'deletions' ? oldLine : newLine); oldLine++; newLine++; }
    else if (line.startsWith('+')) { if (selection.side === 'additions') valid.add(newLine); newLine++; }
    else if (line.startsWith('-')) { if (selection.side === 'deletions') valid.add(oldLine); oldLine++; }
  }
  if (!valid.has(selection.start) || !valid.has(selection.end)) throw new Error('The selected lines are not present on that side of this diff. Refresh and select the visible lines.');
  const comment: Comment = { ...selection, id: randomUUID(), body: body.trim(), headSha: review.headSha, mergeBase: review.mergeBase, digest: patch.digest, createdAt: new Date().toISOString(), status: 'queued' };
  return mutateState(review, state => { state.comments.push(comment); });
}
