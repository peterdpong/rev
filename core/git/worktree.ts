import { lstat } from 'node:fs/promises';
import path from 'node:path';
import { runGitText } from './command';
import { resolveRef } from './refs';
import { hash, mapLimit } from './util';

export type WorkingTreeState = {
  repo: string;
  branch: string;
  sha: string;
  revision: string;
};

/** Local-only polling: check every checkout, inspect files only for the visible working tree. */
export async function workingTrees(
  repos: string[],
  activeRepo?: string,
): Promise<(WorkingTreeState | null)[]> {
  if (!Array.isArray(repos) || repos.some((repo) => typeof repo !== 'string')) {
    throw new Error('Pass a list of repository folders.');
  }
  return mapLimit(repos, 4, async (repo) => {
    try {
      const [branch, sha] = await Promise.all([
        runGitText(repo, ['branch', '--show-current']),
        resolveRef(repo, 'HEAD'),
      ]);
      let fingerprint = '';
      if (repo === activeRepo) {
        const [status, untracked, refs] = await Promise.all([
          runGitText(repo, [
            'status',
            '--porcelain=v1',
            '-z',
            '--untracked-files=no',
          ]),
          // Match the review's committable untracked files.
          runGitText(repo, ['ls-files', '--others', '--exclude-standard', '-z']),
          runGitText(repo, [
            'for-each-ref',
            '--format=%(refname)%00%(objectname)',
            'refs/heads',
            'refs/remotes',
          ]),
        ]);
        const paths = new Set(untracked.split('\0').filter(Boolean));
        const entries = status.split('\0');
        for (let i = 0; i < entries.length; i++) {
          const entry = entries[i];
          if (!entry) continue;
          paths.add(entry.slice(3));
          if (/[RC]/.test(entry.slice(0, 2))) i++; // The next NUL field is the old name.
        }
        const stamps = await mapLimit([...paths].sort(), 16, async (name) => {
          try {
            const stat = await lstat(path.join(repo, name), { bigint: true });
            return [
              name,
              stat.size,
              stat.mtimeNs,
              stat.ctimeNs,
              stat.mode,
              stat.ino,
            ].map(String);
          } catch (error: any) {
            if (error.code === 'ENOENT' || error.code === 'ENOTDIR')
              return [name, 'missing'];
            throw error;
          }
        });
        // Status alone misses repeated edits to an already modified/untracked file.
        fingerprint = JSON.stringify([status, refs, stamps]);
      }
      return {
        repo,
        branch: branch.trim() || 'detached HEAD',
        sha,
        revision: hash(JSON.stringify([branch, sha, fingerprint])),
      };
    } catch {
      // Removed folders and transient checkout operations must not stop other repos updating.
      return null;
    }
  });
}
