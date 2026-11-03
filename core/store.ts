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
import type { Review, ReviewState, Selection, Comment } from './types';
import { filePatch, git, reviewDigest, hash } from './git';

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
    const state: ReviewState = {
      comments: [],
      ...JSON.parse(await readPersistedState(review)),
    };
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
      return { viewed: {}, comments: [] };
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

export async function addComment(
  review: Review,
  selection: Selection | undefined,
  body: string,
  expectedDigest?: string,
): Promise<ReviewState> {
  selection ||= {
    scope: 'review',
    path: '',
    side: 'additions',
    start: 0,
    end: 0,
  };
  if (
    selection.scope &&
    !['lines', 'file', 'review'].includes(selection.scope)
  ) {
    throw new Error('Unknown review scope.');
  }
  if (selection.scope === 'review' || selection.scope === 'file') {
    const patch =
      selection.scope === 'file'
        ? await filePatch(review, selection.path)
        : undefined;
    if (patch && expectedDigest && patch.digest !== expectedDigest) {
      throw new Error('This file changed. Refresh before sending feedback.');
    }
    if (!body?.trim() || body.length > 20_000) {
      throw new Error('Write a comment between 1 and 20,000 characters.');
    }
    const comment: Comment = {
      ...selection,
      path: patch ? selection.path : '',
      start: 0,
      end: 0,
      id: randomUUID(),
      body: body.trim(),
      headSha: review.headSha,
      mergeBase: review.mergeBase,
      digest: patch?.digest || (await reviewDigest(review)),
      createdAt: new Date().toISOString(),
      status: 'queued',
    };
    return mutateState(review, (state) => {
      state.comments.push(comment);
    });
  }
  if (!body?.trim() || body.length > 20_000) {
    throw new Error('Write a comment between 1 and 20,000 characters.');
  }
  if (
    !['additions', 'deletions'].includes(selection.side) ||
    !Number.isInteger(selection.start) ||
    !Number.isInteger(selection.end) ||
    selection.start < 1 ||
    selection.end < selection.start
  ) {
    throw new Error('Select a valid line range first.');
  }
  const patch = await filePatch(review, selection.path);
  if (patch.tooLarge) {
    throw new Error('This file is too large for inline comments.');
  }
  if (expectedDigest && patch.digest !== expectedDigest) {
    throw new Error(
      'This file changed since you selected it. Refresh and select the lines again.',
    );
  }
  const valid = new Set<number>();
  let oldLine = 0;
  let newLine = 0;
  let inHunk = false;
  for (const line of patch.patch.split('\n')) {
    const hunk = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(line);
    if (hunk) {
      oldLine = Number(hunk[1]);
      newLine = Number(hunk[2]);
      inHunk = true;
      continue;
    }
    if (!inHunk) {
      continue;
    }
    if (line.startsWith(' ')) {
      valid.add(selection.side === 'deletions' ? oldLine : newLine);
      oldLine++;
      newLine++;
    } else if (line.startsWith('+')) {
      if (selection.side === 'additions') {
        valid.add(newLine);
      }
      newLine++;
    } else if (line.startsWith('-')) {
      if (selection.side === 'deletions') {
        valid.add(oldLine);
      }
      oldLine++;
    }
  }
  if (!valid.has(selection.start) || !valid.has(selection.end)) {
    throw new Error(
      'The selected lines are not present on that side of this diff. Refresh and select the visible lines.',
    );
  }
  const comment: Comment = {
    ...selection,
    id: randomUUID(),
    body: body.trim(),
    headSha: review.headSha,
    mergeBase: review.mergeBase,
    digest: patch.digest,
    createdAt: new Date().toISOString(),
    status: 'queued',
  };
  return mutateState(review, (state) => {
    state.comments.push(comment);
  });
}
