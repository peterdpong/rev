import {
  mkdir,
  lstat,
  cp,
  open,
  unlink,
  writeFile,
  readFile,
} from 'node:fs/promises';
import { homedir } from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import type { AgentWorktree, Review } from './types';
import { git, resolveRef, filePatch } from './git';

function applyPatch(cwd: string, patch: string) {
  return new Promise<void>((resolve, reject) => {
    const child = spawn('git', ['apply', '--binary', '-'], {
      cwd,
      stdio: ['pipe', 'ignore', 'pipe'],
    });
    let error = '';
    child.stderr.on('data', (data) => {
      error += data;
    });
    child.on('error', reject);
    child.stdin.on('error', reject);
    child.on('close', (code) =>
      code === 0
        ? resolve()
        : reject(new Error(error || 'Could not copy the working changes.')),
    );
    child.stdin.end(patch);
  });
}

/** Persistent, isolated edits. Never reset, clean, or switch the user's checkout. */
export async function changeWorktree(
  review: Review,
  root = path.join(
    process.env.REV_DATA_DIR ||
      process.env.REVUE_DATA_DIR ||
      path.join(homedir(), '.revue'),
    'worktrees',
  ),
  previous?: AgentWorktree,
): Promise<AgentWorktree> {
  const common = (
    await git(review.repo, [
      'rev-parse',
      '--path-format=absolute',
      '--git-common-dir',
    ])
  ).trim();
  const snapshot =
    !previous && review.mode === 'working'
      ? (
          await Promise.all(
            review.files.map(
              async (file) =>
                `${file.path}:${(await filePatch(review, file.path)).digest}`,
            ),
          )
        ).join('\n')
      : '';
  const key = previous
    ? /^(?:rev|patch)\/changes-([a-f0-9]{16})$/.exec(previous.branch)?.[1]
    : createHash('sha256')
        .update(`${common}:${review.scopeKey}:${review.headSha}:${snapshot}`)
        .digest('hex')
        .slice(0, 16);
  if (!key) {
    throw new Error('The retained agent worktree is invalid.');
  }
  const directory = path.join(root, key);
  // Retained conversations keep their existing branch after the app rename.
  const branch = previous?.branch ?? `rev/changes-${key}`;
  if (previous && path.resolve(previous.path) !== path.resolve(directory)) {
    throw new Error('The retained agent worktree location changed.');
  }
  await mkdir(root, { recursive: true });
  try {
    await lstat(directory);
    if (
      (
        await git(directory, [
          'rev-parse',
          '--path-format=absolute',
          '--git-common-dir',
        ])
      ).trim() !== common
    ) {
      throw new Error(
        `The agent worktree belongs to a different repository: ${directory}`,
      );
    }
    if (
      (await readFile(path.join(root, `${key}.ready`), 'utf8').catch(
        () => '',
      )) !== review.headSha
    ) {
      throw new Error(
        `The agent worktree snapshot is incomplete. Review the retained files at ${directory} before retrying.`,
      );
    }
    // Explicitly requested commits/squashes belong to this conversation too.
    await resolveRef(directory, 'HEAD');
    if (
      (await git(directory, ['symbolic-ref', '--short', 'HEAD'])).trim() !==
      branch
    ) {
      throw new Error(`The agent worktree changed branches: ${directory}`);
    }
    return { path: directory, branch };
  } catch (error: any) {
    if (error.code !== 'ENOENT') {
      throw error;
    }
    if (previous) {
      throw new Error(
        `The agent worktree is no longer available: ${directory}`,
      );
    }
  }
  await git(review.repo, [
    'worktree',
    'add',
    '-b',
    branch,
    directory,
    review.headSha,
  ]);
  // A Working tree review includes local edits; snapshot them once, then retain
  // subsequent agent edits in this checkout for the reviewer to inspect.
  if (review.mode === 'working') {
    const patch = await git(review.repo, [
      'diff',
      '--binary',
      '--no-ext-diff',
      'HEAD',
      '--',
    ]);
    if (patch) {
      await applyPatch(directory, patch);
    }
    const names = (
      await git(review.repo, [
        'ls-files',
        '--others',
        '--exclude-standard',
        '-z',
      ])
    )
      .split('\0')
      .filter(Boolean);
    for (const name of names) {
      const destination = path.join(directory, name);
      await mkdir(path.dirname(destination), { recursive: true });
      await cp(path.join(review.repo, name), destination, {
        dereference: false,
        verbatimSymlinks: true,
      });
    }
  }
  await writeFile(path.join(root, `${key}.ready`), review.headSha, {
    mode: 0o600,
  });
  return { path: directory, branch };
}

/** Prevent separate Rev windows/CLI processes from editing the same worktree. */
export async function lockWorktree(directory: string) {
  const file = directory + '.lock';
  let handle;
  try {
    handle = await open(file, 'wx', 0o600);
  } catch (error: any) {
    if (error.code === 'EEXIST') {
      throw new Error(
        `This agent worktree is in use by another Rev process. Wait for it to finish. If that process exited, remove ${file} to retry.`,
      );
    }
    throw error;
  }
  await handle.writeFile(String(process.pid));
  return async () => {
    await handle.close();
    await unlink(file);
  };
}
