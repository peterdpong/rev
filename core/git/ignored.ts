import { runGit } from './command';

/**
 * Find untracked local files excluded by Git's current ignore rules.
 */
export async function ignoredPaths(repo: string, paths: string[]) {
  if (!paths.length) {
    return new Set<string>();
  }

  const output = await runGit(repo, ['check-ignore', '--stdin', '-z'], {
    input: paths.join('\0') + '\0',
    literalPathspecs: false,
    okExitCodes: [1],
  });

  return new Set(output.toString('utf8').split('\0').filter(Boolean));
}
