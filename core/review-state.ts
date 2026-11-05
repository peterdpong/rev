import { mkdir } from 'node:fs/promises';
import path from 'node:path';
import type { Review } from './git';
import { filePatch, runGitText as git } from './git';
import { readJson, serialize, writeJson } from './json-file';

/**
 * Per-review state kept in the repository's Git directory, keyed by review
 * scope. Viewed markers map a path to the digest of the patch that was viewed.
 */
export type ReviewState = { viewed: Record<string, string> };

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
  const state = await readJson<ReviewState>(file, () => ({ viewed: {} }));
  state.viewed ??= {};
  return state;
}

/** Drop markers whose file changed or left the review. */
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

export async function readReviewState(
  review: Review,
  validateViewed = true,
): Promise<ReviewState> {
  const state = await readStoredState(await statePath(review));
  return validateViewed ? dropStaleMarkers(review, state) : state;
}

/** Apply `change` to the saved state. Updates to the same review apply in call order. */
export async function updateReviewState(
  review: Review,
  change: (state: ReviewState) => void,
  validateViewed = true,
): Promise<ReviewState> {
  // `review.repo` is the repo root, so it identifies the state file with `scopeKey`.
  const state = await serialize(
    `${review.repo}\0${review.scopeKey}`,
    async () => {
      const file = await statePath(review);
      const state = await readStoredState(file);
      change(state);
      await writeJson(file, state);
      return state;
    },
  );

  // Check markers after the write so the git diffs don't hold up the queue.
  return validateViewed ? dropStaleMarkers(review, state) : state;
}
