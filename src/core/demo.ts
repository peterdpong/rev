import { mkdir, writeFile, mkdtemp, access } from 'node:fs/promises';
import path from 'node:path';
import { tmpdir } from 'node:os';
import { git } from './git';

export async function createDemo(destination?: string) {
  const repo = destination || await mkdtemp(path.join(tmpdir(), 'revue-sample-'));
  if (destination) { try { await access(path.join(repo, '.git')); return repo; } catch { /* new sample */ } }
  await mkdir(path.join(repo, 'src/lib'), { recursive: true }); await mkdir(path.join(repo, 'src/api'), { recursive: true }); await mkdir(path.join(repo, 'tests'), { recursive: true });
  await git(repo, ['init', '-b', 'main']);
  const write = async (file: string, text: string) => writeFile(path.join(repo, file), text);
  await write('src/lib/retry.ts', `export type RetryOptions = {\n  attempts: number;\n  delayMs: number;\n};\n\nconst sleep = (ms: number) =>\n  new Promise(resolve => setTimeout(resolve, ms));\n\nexport async function retry<T>(\n  operation: () => Promise<T>,\n  options: RetryOptions,\n): Promise<T> {\n  let lastError: unknown;\n\n  for (let attempt = 0; attempt < options.attempts; attempt++) {\n    try {\n      return await operation();\n    } catch (error) {\n      lastError = error;\n      await sleep(options.delayMs);\n    }\n  }\n\n  throw lastError;\n}\n`);
  await write('src/api/client.ts', `import { retry } from '../lib/retry';\n\nexport async function fetchProject(id: string) {\n  return retry(async () => {\n    const response = await fetch(\x60/api/projects/\x24{id}\x60);\n    if (!response.ok) throw new Error('Request failed');\n    return response.json();\n  }, { attempts: 3, delayMs: 1000 });\n}\n`);
  await write('README.md', '# Orbit API client\n\nA small typed client for the Orbit project API.\n');
  await write('package.json', JSON.stringify({ name: 'orbit-client', private: true, scripts: { test: 'node --test tests/*.test.mjs' } }, null, 2) + '\n');
  await git(repo, ['add', '.']); await git(repo, ['-c', 'user.name=Revue Sample', '-c', 'user.email=sample@example.invalid', 'commit', '-m', 'Add API client']);
  await git(repo, ['switch', '-c', 'feat/resilient-requests']);
  await write('src/lib/retry.ts', `export type RetryOptions = {\n  attempts: number;\n  delayMs: number;\n  maxDelayMs?: number;\n  signal?: AbortSignal;\n  shouldRetry?: (error: unknown) => boolean;\n};\n\nconst sleep = (ms: number) =>\n  new Promise(resolve => setTimeout(resolve, ms));\n\nexport async function retry<T>(\n  operation: () => Promise<T>,\n  options: RetryOptions,\n): Promise<T> {\n  let lastError: unknown;\n\n  for (let attempt = 0; attempt < options.attempts; attempt++) {\n    options.signal?.throwIfAborted();\n\n    try {\n      return await operation();\n    } catch (error) {\n      lastError = error;\n      if (options.shouldRetry && !options.shouldRetry(error)) {\n        throw error;\n      }\n\n      const backoff = options.delayMs * 2 ** attempt;\n      const delay = Math.min(backoff, options.maxDelayMs ?? 30_000);\n      await sleep(delay);\n    }\n  }\n\n  throw lastError;\n}\n`);
  await write('src/api/client.ts', `import { retry } from '../lib/retry';\n\nexport class HttpError extends Error {\n  constructor(public status: number) {\n    super(\x60Request failed with status \x24{status}\x60);\n  }\n}\n\nexport async function fetchProject(id: string, signal?: AbortSignal) {\n  return retry(async () => {\n    const response = await fetch(\x60/api/projects/\x24{encodeURIComponent(id)}\x60, { signal });\n    if (!response.ok) throw new HttpError(response.status);\n    return response.json();\n  }, {\n    attempts: 3,\n    delayMs: 250,\n    maxDelayMs: 5000,\n    signal,\n    shouldRetry: error => error instanceof HttpError && error.status >= 500,\n  });\n}\n`);
  await write('tests/retry.test.mjs', `import { test } from 'node:test';\nimport assert from 'node:assert/strict';\nimport { retry } from '../src/lib/retry.ts';\n\ntest('retries a transient failure', async () => {\n  let calls = 0;\n  const result = await retry(async () => {\n    if (++calls < 3) throw new Error('unavailable');\n    return 'ok';\n  }, { attempts: 3, delayMs: 1 });\n  assert.equal(result, 'ok');\n  assert.equal(calls, 3);\n});\n`);
  await write('README.md', '# Orbit API client\n\nA small typed client for the Orbit project API.\n\n## Resilient requests\n\nRequests use exponential backoff, retry server errors, and accept an AbortSignal.\n');
  await git(repo, ['add', '.']); await git(repo, ['-c', 'user.name=Revue Sample', '-c', 'user.email=sample@example.invalid', 'commit', '-m', 'Add backoff and cancellation to API requests']);
  return repo;
}
