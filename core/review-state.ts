import { mkdir } from 'node:fs/promises';
import path from 'node:path';
import type { Review } from './git';
import { filePatch, runGitText as git } from './git';
import { readJson, serialize, writeJson } from './json-file';

const VIEWED_VALIDATION_CONCURRENCY = 8;

/**
 * Per-review state kept in the repository's Git directory.
 * Viewed markers map a path to the digest of the patch that was viewed.
 */
export type ReviewState = { viewed: Record<string, string> };

// Git directory lookups, keyed by repository root.
const reviewStateDirs = new Map<string, Promise<string>>();

function reviewStateDir(repo: string) {
  let dir = reviewStateDirs.get(repo);
  if (!dir) {
    dir = (async () => {
      const gitDir = (
        await git(repo, ['rev-parse', '--absolute-git-dir'])
      ).trim();
      const dir = path.join(gitDir, 'rev');
      await mkdir(dir, { recursive: true });
      return dir;
    })();

    dir.catch(() => reviewStateDirs.delete(repo));
    reviewStateDirs.set(repo, dir);
  }

  return dir;
}

async function reviewStatePath(review: Review) {
  return path.join(
    await reviewStateDir(review.repo),
    `${review.scopeKey}.json`,
  );
}

/** Read the saved state as-is, without checking viewed markers. */
async function readStoredState(file: string): Promise<ReviewState> {
  const state = await readJson<ReviewState>(file, () => ({ viewed: {} }));
  state.viewed ??= {};
  return state;
}

/** Drop markers for `names` whose file changed or left the review. */
async function dropStaleMarkers(
  review: Review,
  state: ReviewState,
  names = Object.keys(state.viewed),
): Promise<ReviewState> {
  for (let i = 0; i < names.length; i += VIEWED_VALIDATION_CONCURRENCY) {
    await Promise.all(
      names.slice(i, i + VIEWED_VALIDATION_CONCURRENCY).map(async (name) => {
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

export async function readReviewState(
  review: Review,
  validateViewed = true,
): Promise<ReviewState> {
  const state = await readStoredState(await reviewStatePath(review));
  return validateViewed ? dropStaleMarkers(review, state) : state;
}

/**
 * Apply `change` to the saved state. Only check markers added or changed
 * by this update.
 */
export async function updateReviewState(
  review: Review,
  change: (state: ReviewState) => void,
  validateViewed = true,
): Promise<ReviewState> {
  let before: ReviewState['viewed'] = {};
  const state = await serialize(
    `${review.repo}\0${review.scopeKey}`,
    async () => {
      const file = await reviewStatePath(review);
      const state = await readStoredState(file);
      before = { ...state.viewed };
      change(state);
      await writeJson(file, state);
      return state;
    },
  );

  // Check markers after the write so the git diffs don't hold up the queue.
  const changed = Object.keys(state.viewed).filter(
    (name) => state.viewed[name] !== before[name],
  );

  return validateViewed ? dropStaleMarkers(review, state, changed) : state;
}
