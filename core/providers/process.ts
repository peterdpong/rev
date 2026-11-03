import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';

export function spawnProvider(
  command: string,
  args: string[],
  cwd: string,
  env = process.env,
) {
  return spawn(command, args, {
    cwd,
    env: { ...env, NO_COLOR: '1' },
    stdio: ['pipe', 'pipe', 'pipe'],
    detached: process.platform !== 'win32',
  });
}

export function processLifetime(child: ChildProcessWithoutNullStreams) {
  let closed = false;
  const done = new Promise<void>((resolve) =>
    child.once('close', () => {
      closed = true;
      resolve();
    }),
  );
  let stopping: Promise<void> | undefined;
  const kill = (signal: NodeJS.Signals) => {
    try {
      if (child.pid && process.platform !== 'win32') {
        process.kill(-child.pid, signal);
      } else {
        child.kill(signal);
      }
    } catch {
      /* already gone */
    }
  };
  return {
    done,
    stop(): Promise<void> {
      if (stopping) {
        return stopping;
      }
      stopping = (async () => {
        if (closed) {
          return;
        }
        kill('SIGTERM');
        const timer = setTimeout(() => kill('SIGKILL'), 2000);
        try {
          await done;
        } finally {
          clearTimeout(timer);
        }
      })();
      return stopping;
    },
  };
}
