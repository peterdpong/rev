import {
  mkdir,
  readFile,
  writeFile,
  rename,
  open,
  unlink,
} from 'node:fs/promises';
import path from 'node:path';
import { randomUUID } from 'node:crypto';
import type { Review, ReviewState } from './types';
import { filePatch, git, hash } from './git';

async function statePath(review: Review) {
  const gitDir = (
    await git(review.repo, ['rev-parse', '--absolute-git-dir'])
  ).trim();
  const dir = path.join(gitDir, 'revue');
  await mkdir(dir, { recursive: true });
  return path.join(dir, `${review.scopeKey}.json`);
}

export async function readState(
  review: Review,
  validateViewed = true,
): Promise<ReviewState> {
  try {
    const state: ReviewState = JSON.parse(await readPersistedState(review));
    const names = validateViewed ? Object.keys(state.viewed) : [];
    for (let i = 0; i < names.length; i += 8) {
      await Promise.all(
        names.slice(i, i + 8).map(async (name) => {
          try {
            if ((await filePatch(review, name)).digest !== state.viewed[name]) {
              delete state.viewed[name];
            }
          } catch {
            delete state.viewed[name];
          }
        }),
      );
    }
    return state;
  } catch (e: any) {
    if (e.code === 'ENOENT') {
      return { viewed: {} };
    }
    throw e;
  }
}

export async function mutateState(
  review: Review,
  change: (state: ReviewState) => void,
  validateViewed = true,
) {
  const file = await statePath(review);
  const lockPath = file + '.lock';
  let lock;
  for (let attempt = 0; attempt < 50; attempt++) {
    try {
      lock = await open(lockPath, 'wx');
      break;
    } catch (error: any) {
      if (error.code !== 'EEXIST') {
        throw error;
      }
      await new Promise((r) => setTimeout(r, 40));
    }
  }
  if (!lock) {
    throw new Error(
      `Review data is busy. If no other Revue is running, remove ${lockPath}.`,
    );
  }
  const temp = file + '.' + randomUUID();
  try {
    const state = await readState(review, validateViewed);
    change(state);
    await writeFile(temp, JSON.stringify(state, null, 2), { mode: 0o600 });
    await rename(temp, file);
    return state;
  } finally {
    await lock.close();
    await unlink(lockPath);
    await unlink(temp).catch(() => {});
  }
}

/** Read scopes saved before review identity included the comparison mode. */
async function readPersistedState(review: Review): Promise<string> {
  const file = await statePath(review);
  try {
    return await readFile(file, 'utf8');
  } catch (error: any) {
    if (error.code !== 'ENOENT') throw error;
    const head =
      review.head === 'HEAD'
        ? review.branch === 'detached HEAD'
          ? review.branchHeadSha
          : review.branch
        : review.head;
    const key = hash([review.repo, review.base, head].join('\0')).slice(0, 24);
    const scope = review.commitRange
      ? hash(
          [key, review.commitRange.start, review.commitRange.end].join('\0'),
        ).slice(0, 24)
      : key;
    return readFile(path.join(path.dirname(file), `${scope}.json`), 'utf8');
  }
}
