import { execFile, type ExecFileException } from 'node:child_process';
import { statSync } from 'node:fs';

import {
  DEFAULT_COMMAND_MAX_BUFFER_BYTES,
  DEFAULT_COMMAND_TIMEOUT_MS,
} from './constants';

export type CommandFailure =
  'missing-executable' | 'missing-folder' | 'too-large' | 'timeout' | 'failed';

export class CommandError extends Error {
  constructor(
    message: string,
    readonly reason: CommandFailure,
    readonly exitCode?: number,
  ) {
    super(message);
    this.name = 'CommandError';
  }
}

export type RunOptions = {
  cwd?: string;
  input?: string | Buffer;
  maxBuffer?: number;
  timeout?: number;
  okExitCodes?: number[];
};

export type GitOptions = Omit<RunOptions, 'cwd'> & {
  /** Most commands take pathspecs; check-ignore rejects this flag. */
  literalPathspecs?: boolean;
};

/**
 * Run a program and return its stdout as bytes.
 */
export function runCommand(
  executable: string,
  args: string[],
  {
    cwd,
    input,
    maxBuffer = DEFAULT_COMMAND_MAX_BUFFER_BYTES,
    timeout = DEFAULT_COMMAND_TIMEOUT_MS,
    okExitCodes = [],
  }: RunOptions = {},
): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    let child;
    try {
      child = execFile(
        executable,
        args,
        {
          cwd,
          encoding: 'buffer',
          maxBuffer,
          timeout,
          env: {
            ...process.env,
            GIT_TERMINAL_PROMPT: '0',
            GIT_OPTIONAL_LOCKS: '0',
          },
        },
        (error, stdout, stderr) => {
          if (
            !error ||
            (typeof error.code === 'number' && okExitCodes.includes(error.code))
          ) {
            resolve(stdout);
          } else {
            reject(
              parseCommandError(error, stderr.toString(), executable, cwd),
            );
          }
        },
      );
    } catch (error) {
      // Some spawn failures (such as a cwd that is a file) throw synchronously.
      reject(
        parseCommandError(error as ExecFileException, '', executable, cwd),
      );
      return;
    }

    child.stdin?.on('error', () => {});
    child.stdin?.end(input);
  });
}

/**
 * Run a program and return its stdout as UTF-8 text.
 */
export async function runCommandText(
  executable: string,
  args: string[],
  cwd?: string,
  maxBuffer?: number,
) {
  return (await runCommand(executable, args, { cwd, maxBuffer })).toString(
    'utf8',
  );
}

export function runGit(
  repo: string,
  args: string[],
  { literalPathspecs = true, ...options }: GitOptions = {},
) {
  return runCommand(
    'git',
    [
      // Apply this setting only to this command; leave saved Git config unchanged.
      '-c',
      // Keep non-ASCII filenames readable; control characters remain escaped.
      'core.quotePath=false',
      // Send output directly to us instead of an interactive pager such as less.
      '--no-pager',
      ...(literalPathspecs ? ['--literal-pathspecs'] : []),
      ...args,
    ],
    { ...options, cwd: repo },
  );
}

export const runGitText = async (
  repo: string,
  args: string[],
  maxBuffer?: number,
) => {
  return (await runGit(repo, args, { maxBuffer })).toString('utf8');
};

function parseCommandError(
  error: ExecFileException,
  stderr: string,
  executable: string,
  cwd: string | undefined,
) {
  if (
    error.code === 'ERR_CHILD_PROCESS_STDIO_MAXBUFFER' ||
    error.code === 'ENOBUFS'
  ) {
    return new CommandError(
      `${executable} produced more output than allowed.`,
      'too-large',
    );
  }

  // A missing working directory also surfaces as a spawn ENOENT.
  if (error.code === 'ENOENT' || error.code === 'ENOTDIR') {
    if (cwd && !isDirectory(cwd)) {
      return new CommandError(`Folder not found: ${cwd}`, 'missing-folder');
    }

    if (error.code === 'ENOENT') {
      return new CommandError(
        `${executable} is not installed or is not on PATH.`,
        'missing-executable',
      );
    }
  }

  if (error.killed) {
    return new CommandError(`${executable} timed out.`, 'timeout');
  }

  return new CommandError(
    stderr.trim() || error.message,
    'failed',
    typeof error.code === 'number' ? error.code : undefined,
  );
}

function isDirectory(folder: string) {
  try {
    return statSync(folder).isDirectory();
  } catch {
    return false;
  }
}
