import path from 'node:path';
import { hash, mapLimit } from './util';
import type { ChangedFile, FilePatch, Review, ReviewPatch } from './types';
import { CommandError, runGitText } from './command';
import { rootOf } from './refs';
import { assertComparison, diffArgs } from './review';
import { readUntracked, untrackedPatch, untrackedCounts } from './untracked';
import {
  MAX_PATCH_BYTES,
  MAX_INLINE_PATCH_BYTES,
  MAX_INLINE_CHANGED_LINES,
} from './constants';

/**
 * For too-large files, the patch isn't read so we calculate
 * the digest from the content IDs before and after.
 */
async function tooLarge(review: Review, file: ChangedFile): Promise<FilePatch> {
  const id = (args: string[]) =>
    runGitText(review.repo, args).then(
      (out) => out.trim(),
      () => '', // The file doesn't exist on this side.
    );

  // Look up a committed file's content hash without reading its contents.
  const blob = (commit: string, name: string) =>
    id([
      'rev-parse',
      '--verify',
      '--quiet',
      '--end-of-options',
      `${commit}:${name}`,
    ]);

  const [before, after] = await Promise.all([
    // Untracked files have no content on the base side.
    file.status === '?'
      ? ''
      : blob(review.mergeBase, file.oldPath ?? file.path),
    // Use the committed version for branch reviews, or hash the local file.
    review.mode === 'branch'
      ? blob(review.headSha, file.path)
      : id(['hash-object', '--no-filters', '--', file.path]),
  ]);

  return {
    patch: '',
    // Fingerprint both sides so viewed status expires when either changes.
    digest: hash(`large\0${before}\0${after}`),
    tooLarge: true,
  };
}

/**
 * Check that Review has a valid mode, commit IDs, and a repo that
 * is a real checkout root (inspect() should always store the canonical root.
 */
async function assertReview(review: Review) {
  if (
    typeof review?.repo !== 'string' ||
    !path.isAbsolute(review.repo) ||
    !Array.isArray(review.files)
  ) {
    throw new Error('Invalid review.');
  }

  assertComparison(review);

  if ((await rootOf(review.repo)) !== review.repo) {
    throw new Error('Invalid review repository.');
  }
}

/**
 * Load one file's patch.
 */
async function patchFor(
  review: Review,
  file: ChangedFile,
  maxBytes = MAX_PATCH_BYTES,
  maxChangedLines = Infinity,
): Promise<FilePatch> {
  try {
    if (file.status === '?') {
      const untracked = await readUntracked(review.repo, file.path, maxBytes);

      if (
        untracked.kind === 'too-large' ||
        untrackedCounts(untracked).added > maxChangedLines
      ) {
        return tooLarge(review, file);
      }

      const patch = untrackedPatch(file.path, untracked);
      // Diff headers and added-line prefixes count toward the same byte limit.
      if (Buffer.byteLength(patch, 'utf8') > maxBytes) {
        return tooLarge(review, file);
      }
      return {
        patch,
        // A binary placeholder never changes, so digest the bytes instead.
        digest: hash(untracked.kind === 'binary' ? untracked.content : patch),
        tooLarge: false,
      };
    }

    const patch = await runGitText(
      review.repo,
      [
        ...diffArgs(review, ['--patch', '--unified=5']),
        '--',
        ...(file.oldPath ? [file.oldPath] : []),
        file.path,
      ],
      maxBytes,
    );

    return { patch, digest: hash(patch), tooLarge: false };
  } catch (error) {
    if (error instanceof CommandError && error.reason === 'too-large') {
      return tooLarge(review, file);
    }
    throw error;
  }
}

/**
 * Load one changed file's patch and content digest for the viewer.
 */
export async function filePatch(
  review: Review,
  fileName: string,
): Promise<FilePatch> {
  await assertReview(review);

  const file = review.files.find((f) => f.path === fileName);
  if (!file) {
    throw new Error('That file is not in this comparison.');
  }

  return patchFor(review, file);
}

/**
 * Load the continuous diff with up to four concurrent patch reads, in the
 * Review's file order. Errors are reported per file so one unreadable file.
 */
export async function reviewPatches(
  review: Review,
  preview = false,
): Promise<ReviewPatch[]> {
  await assertReview(review);

  return mapLimit(review.files, 4, async (file) => {
    const deferred: ReviewPatch = {
      path: file.path,
      patch: '',
      digest: '',
      tooLarge: false,
      deferred: true,
    };
    if (preview && file.added + file.removed > MAX_INLINE_CHANGED_LINES)
      return deferred;
    try {
      const patch = await patchFor(
        review,
        file,
        preview ? MAX_INLINE_PATCH_BYTES : MAX_PATCH_BYTES,
        preview ? MAX_INLINE_CHANGED_LINES : Infinity,
      );
      return preview && patch.tooLarge
        ? deferred
        : { ...patch, path: file.path };
    } catch (error) {
      return {
        path: file.path,
        patch: '',
        digest: '',
        tooLarge: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  });
}
