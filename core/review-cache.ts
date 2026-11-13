import { randomUUID } from 'node:crypto';
import {
  diffArgs,
  filePatch,
  parseDiffSummary,
  resolveRef,
  rootOf,
  runGitText,
  type Review,
} from './git';

export class ReviewCache {
  private snapshots = new Map<string, Review>();

  storeSnapshot(review: Review): string {
    const id = randomUUID();
    this.snapshots.set(id, review);

    // Retain recent snapshots for in-flight requests without growing with every refresh.
    while (this.snapshots.size > 8) {
      this.snapshots.delete(this.snapshots.keys().next().value!);
    }

    return id;
  }

  async patch(
    id: string,
    repo: string,
    fileName: string,
    expected?: { headSha?: string; mergeBase?: string },
  ) {
    const review = this.snapshots.get(id);
    if (!review) {
      throw new Error('This review expired. Refresh the review.');
    }

    if ((await rootOf(repo)) !== review.repo) {
      throw new Error('Invalid review repository.');
    }

    if (
      (expected?.headSha !== undefined || expected?.mergeBase !== undefined) &&
      (expected.headSha !== review.headSha ||
        expected.mergeBase !== review.mergeBase)
    ) {
      throw new Error(
        'The comparison changed while loading. Refresh the review.',
      );
    }

    const file = review.files.find((file) => file.path === fileName);
    if (!file) {
      throw new Error('That file is not in this comparison.');
    }

    // Mark snapshot recently used.
    this.snapshots.delete(id);
    this.snapshots.set(id, review);

    // Committed comparisons stay pinned to the commits that produced the file list.
    if (review.mode === 'branch') {
      return filePatch(review, fileName);
    }

    const [sha, branch] = await Promise.all([
      resolveRef(review.repo, 'HEAD'),
      runGitText(review.repo, ['branch', '--show-current']),
    ]);
    if (
      sha !== review.branchHeadSha ||
      (branch.trim() || 'detached HEAD') !== review.branch
    ) {
      throw new Error(
        'The checkout changed while loading. Refresh the review.',
      );
    }

    // Refresh only the selected path's status: it may have been staged, removed,
    // renamed, or ignored since the snapshot.
    const [summary, untracked] = await Promise.all([
      runGitText(review.repo, [
        ...diffArgs(review, ['--raw', '--numstat', '-z']),
        '--',
        ...(file.oldPath ? [file.oldPath] : []),
        fileName,
      ]),
      runGitText(review.repo, [
        'ls-files',
        '--others',
        '--exclude-standard',
        '-z',
        '--',
        fileName,
      ]),
    ]);
    
    const files = parseDiffSummary(summary);

    // If the file is untracked add the entry.
    if (
      !files.some((file) => file.path === fileName) &&
      untracked.split('\0').includes(fileName)
    ) {
      files.push({ ...file, status: '?' });
    }
    
    return filePatch({ ...review, files }, fileName);
  }

  clear() {
    this.snapshots.clear();
  }
}
