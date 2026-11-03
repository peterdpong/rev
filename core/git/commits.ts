import { runGitText } from './command';
import { rootOf, resolveRef } from './refs';
import type { ReviewRequest, CommitPage } from './types';

export async function listCommits(
  input: ReviewRequest,
  offset = 0,
): Promise<CommitPage> {
  if (!Number.isSafeInteger(offset) || offset < 0) {
    throw new Error('Invalid commit offset.');
  }

  const repo = await rootOf(input.repo);
  const head = await resolveRef(repo, input.head || 'HEAD');
  const parts = (
    await runGitText(repo, [
      'log', 
      '--first-parent', // Follow only the first parent of merges to keep history on the mainline.
      '-z', // Separate commits with NUL bytes, matching the field separator below.
      '--format=%H%x00%P%x00%s%x00%an%x00%aI', // Emit SHA, parent SHAs, subject, author name, and author date (strict ISO 8601). %x00 separates fields with NUL bytes for parsing below.
      '--max-count=201', // Fetch one extra commit beyond the 200-item page to determine hasMore.
      `--skip=${offset}`, 
      head, // Start history at the resolved head commit.
      '--', // End revision arguments.
    ])
  ).split('\0');

  const commits = [];
  for (let i = 0; i + 4 < parts.length; i += 5) {
    commits.push({
      sha: parts[i],
      parents: parts[i + 1].split(' ').filter(Boolean),
      subject: parts[i + 2],
      author: parts[i + 3],
      date: parts[i + 4],
    });
  }

  return { commits: commits.slice(0, 200), hasMore: commits.length > 200 };
}
