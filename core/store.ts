import { mkdir, readFile, writeFile, rename, unlink } from 'node:fs/promises';
import path from 'node:path';
import { randomUUID } from 'node:crypto';
import type { Review, ReviewState } from './types';
import { filePatch, runGitText as git } from './git';

// Serialize updates to each state file within this process.
const pendingWrites = new Map<string, Promise<unknown>>();

async function statePath(review: Review) {
  const gitDir = (
    await git(review.repo, ['rev-parse', '--absolute-git-dir'])
  ).trim();
  
  const dir = path.join(gitDir, 'rev');
  await mkdir(dir, { recursive: true });

  return path.join(dir, `${review.scopeKey}.json`);
}

/** Read the saved state as-is, without checking viewed markers. */
async function readStoredState(file: string): Promise<ReviewState> {
  try {
    return JSON.parse(await readFile(file, 'utf8'));
  } catch (e: any) {
    if (e.code === 'ENOENT') {
      return { viewed: {} };
    }
    throw e;
  }
}

async function dropStaleMarkers(
  review: Review,
  state: ReviewState,
): Promise<ReviewState> {
  const names = Object.keys(state.viewed);

  for (let i = 0; i < names.length; i += 8) {
    await Promise.all(
      names.slice(i, i + 8).map(async (name) => {
        const viewed = state.viewed[name];
        try {
          if (!viewed || (await filePatch(review, name)).digest !== viewed) {
            delete state.viewed[name];
          }
        } catch {
          delete state.viewed[name];
        }
      }),
    );
  }

  return state;
}

export async function readState(review: Review): Promise<ReviewState> {
  return dropStaleMarkers(
    review,
    await readStoredState(await statePath(review)),
  );
}

export async function mutateState(
  review: Review,
  change: (state: ReviewState) => void,
) {
  // Take the queue position synchronously so updates apply in call order.
  // `review.repo` is the repo root, so it identifies the state file with `scopeKey`.
  const key = `${review.repo}\0${review.scopeKey}`;

  const run = async () => {
    const file = await statePath(review);
    const temp = file + '.' + randomUUID();
    try {
      const state = await readStoredState(file);
      change(state);
      await writeFile(temp, JSON.stringify(state, null, 2), { mode: 0o600 });
      await rename(temp, file);
      return state;
    } finally {
      await unlink(temp).catch(() => {});
    }
  };

  // Run after the previous update whether it succeeded or failed, so one
  // failure doesn't block later updates.
  const write = (pendingWrites.get(key) ?? Promise.resolve()).then(run, run);
  pendingWrites.set(key, write);

  let state: ReviewState;
  try {
    state = await write;
  } finally {
    if (pendingWrites.get(key) === write) {
      pendingWrites.delete(key);
    }
  }

  // Check markers after the write so the git diffs don't hold up the queue.
  return dropStaleMarkers(review, state);
}
