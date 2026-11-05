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

// Git directory lookups, keyed by repository root.
const stateDirs = new Map<string, Promise<string>>();

function stateDir(repo: string) {
  let dir = stateDirs.get(repo);
  if (!dir) {
    dir = (async () => {
      const gitDir = (
        await git(repo, ['rev-parse', '--absolute-git-dir'])
      ).trim();
      const dir = path.join(gitDir, 'rev');
      await mkdir(dir, { recursive: true });
      return dir;
    })();
    dir.catch(() => stateDirs.delete(repo));
    stateDirs.set(repo, dir);
  }
  return dir;
}

async function statePath(review: Review) {
  return path.join(await stateDir(review.repo), `${review.scopeKey}.json`);
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

export async function readReviewState(review: Review): Promise<ReviewState> {
  return dropStaleMarkers(
    review,
    await readStoredState(await statePath(review)),
  );
}

/**
 * Apply `change` to the saved state. Updates to the same review apply in call
 * order. The result checks only the markers this update set or changed; the
 * rest were checked by the last read.
 */
export async function updateReviewState(
  review: Review,
  change: (state: ReviewState) => void,
): Promise<ReviewState> {
  // `review.repo` is the repo root, so it identifies the state file with `scopeKey`.
  let before: ReviewState['viewed'] = {};
  const state = await serialize(
    `${review.repo}\0${review.scopeKey}`,
    async () => {
      const file = await statePath(review);
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
  return dropStaleMarkers(review, state, changed);
}
