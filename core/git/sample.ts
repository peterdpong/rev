import { mkdir, writeFile, mkdtemp, access } from 'node:fs/promises';
import path from 'node:path';
import { tmpdir } from 'node:os';
import { runGitText } from './command';

export async function createSampleRepo(destination?: string) {
  const repo =
    destination || (await mkdtemp(path.join(tmpdir(), 'rev-sample-')));
  if (destination) {
    try {
      await access(path.join(repo, '.git'));
      return repo;
    } catch {
      /* new sample */
    }
  }
  await mkdir(path.join(repo, 'src/lib'), { recursive: true });
  await mkdir(path.join(repo, 'src/api'), { recursive: true });
  await mkdir(path.join(repo, 'tests'), { recursive: true });
  await runGitText(repo, ['init', '-b', 'main']);
  const write = async (file: string, text: string) =>
    writeFile(path.join(repo, file), text);
  await write(
    'src/lib/retry.ts',
    `export type RetryOptions = {
  attempts: number;
  delayMs: number;
};

const sleep = (ms: number) =>
  new Promise(resolve => setTimeout(resolve, ms));

export async function retry<T>(
  operation: () => Promise<T>,
  options: RetryOptions,
): Promise<T> {
  let lastError: unknown;

  for (let attempt = 0; attempt < options.attempts; attempt++) {
    try {
      return await operation();
    } catch (error) {
      lastError = error;
      await sleep(options.delayMs);
    }
  }

  throw lastError;
}
`,
  );
  await write(
    'src/api/client.ts',
    `import { retry } from '../lib/retry';

export async function fetchProject(id: string) {
  return retry(async () => {
    const response = await fetch(\`/api/projects/\${id}\`);
    if (!response.ok) throw new Error('Request failed');
    return response.json();
  }, { attempts: 3, delayMs: 1000 });
}
`,
  );
  await write(
    'README.md',
    '# Orbit API client\n\nA small typed client for the Orbit project API.\n',
  );
  await write(
    'package.json',
    JSON.stringify(
      {
        name: 'orbit-client',
        private: true,
        scripts: { test: 'node --test tests/*.test.mjs' },
      },
      null,
      2,
    ) + '\n',
  );
  await runGitText(repo, ['add', '.']);
  await runGitText(repo, [
    '-c',
    'user.name=Rev Sample',
    '-c',
    'user.email=sample@example.invalid',
    'commit',
    '-m',
    'Add API client',
  ]);
  await runGitText(repo, ['switch', '-c', 'feat/resilient-requests']);
  await write(
    'src/lib/retry.ts',
    `export type RetryOptions = {
  attempts: number;
  delayMs: number;
  maxDelayMs?: number;
  signal?: AbortSignal;
  shouldRetry?: (error: unknown) => boolean;
};

const sleep = (ms: number) =>
  new Promise(resolve => setTimeout(resolve, ms));

export async function retry<T>(
  operation: () => Promise<T>,
  options: RetryOptions,
): Promise<T> {
  let lastError: unknown;

  for (let attempt = 0; attempt < options.attempts; attempt++) {
    options.signal?.throwIfAborted();

    try {
      return await operation();
    } catch (error) {
      lastError = error;
      if (options.shouldRetry && !options.shouldRetry(error)) {
        throw error;
      }

      const backoff = options.delayMs * 2 ** attempt;
      const delay = Math.min(backoff, options.maxDelayMs ?? 30_000);
      await sleep(delay);
    }
  }

  throw lastError;
}
`,
  );
  await write(
    'src/api/client.ts',
    `import { retry } from '../lib/retry';

export class HttpError extends Error {
  constructor(public status: number) {
    super(\`Request failed with status \${status}\`);
  }
}

export async function fetchProject(id: string, signal?: AbortSignal) {
  return retry(async () => {
    const response = await fetch(\`/api/projects/\${encodeURIComponent(id)}\`, { signal });
    if (!response.ok) throw new HttpError(response.status);
    return response.json();
  }, {
    attempts: 3,
    delayMs: 250,
    maxDelayMs: 5000,
    signal,
    shouldRetry: error => error instanceof HttpError && error.status >= 500,
  });
}
`,
  );
  await write(
    'tests/retry.test.mjs',
    `import { test } from 'node:test';
import assert from 'node:assert/strict';
import { retry } from '../src/lib/retry.ts';

test('retries a transient failure', async () => {
  let calls = 0;
  const result = await retry(async () => {
    if (++calls < 3) throw new Error('unavailable');
    return 'ok';
  }, { attempts: 3, delayMs: 1 });
  assert.equal(result, 'ok');
  assert.equal(calls, 3);
});
`,
  );
  await write(
    'README.md',
    '# Orbit API client\n\nA small typed client for the Orbit project API.\n\n## Resilient requests\n\nRequests use exponential backoff, retry server errors, and accept an AbortSignal.\n',
  );
  await runGitText(repo, ['add', '.']);
  await runGitText(repo, [
    '-c',
    'user.name=Rev Sample',
    '-c',
    'user.email=sample@example.invalid',
    'commit',
    '-m',
    'Add backoff and cancellation to API requests',
  ]);
  return repo;
}
