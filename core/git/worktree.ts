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

export async function workingTrees(
  repos: string[],
  activeRepo?: string,
): Promise<(WorkingTreeState | null)[]> {
  if (!Array.isArray(repos) || repos.some((repo) => typeof repo !== 'string')) {
    throw new Error('Pass a list of repository folders.');
  }

  // Check and updated all open repositories.
  return mapLimit(repos, 4, async (repo) => {
    try {
      // Track branch switches and new commits in every repository.
      const [branch, sha] = await Promise.all([
        runGitText(repo, ['branch', '--show-current']),
        resolveRef(repo, 'HEAD'),
      ]);

      let fingerprint = '';
      // Only inspect local file changes for the active repository.
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
          
          // Detect changes to branch tips used in the comparison.
          runGitText(repo, [
            'for-each-ref',
            '--format=%(refname)%00%(objectname)',
            'refs/heads',
            'refs/remotes',
          ]),
        ]);
        
        // Combine untracked files with changed tracked paths.
        const paths = new Set(untracked.split('\0').filter(Boolean));

        // Status records use "XY path": X is staged, Y is unstaged.
        const entries = status.split('\0');
        let skipOriginalPath = false;
        entries.forEach((entry) => {
          if (skipOriginalPath) {
            skipOriginalPath = false;
            return;
          }

          // Ignore the empty record after the final NUL.
          if (!entry) {
            return; 
          } 

          paths.add(entry.slice(3)); // Strip "XY " to keep the current path.

          // Renames (R) and copies (C) include the original path in the next record.
          skipOriginalPath = /[RC]/.test(entry.slice(0, 2));
        });

        // Keep path order stable so fingerprint stays stable.
        const fileMetadata = await mapLimit([...paths].sort(), 16, async (name) => {
          try {
            // Read metadata for the file itself, without following symlinks.
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
            if (error.code === 'ENOENT' || error.code === 'ENOTDIR') {
              return [name, 'missing'];
            }
              
            throw error;
          }
        });

        // Status alone misses repeated edits to an already modified/untracked file.
        fingerprint = JSON.stringify([status, refs, fileMetadata]);
      }

      return {
        repo,
        branch: branch.trim() || 'detached HEAD',
        sha,
        // Local-state fingerprint used to decide whether the review needs refreshing.
        revision: hash(JSON.stringify([branch, sha, fingerprint])),
      };
    } catch {
      // Don't block other repos from updating on error.
      return null;
    }
  });
}
