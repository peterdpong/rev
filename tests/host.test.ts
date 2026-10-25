import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createInterface } from 'node:readline';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { createSampleRepo } from '../core/git';

test(
  'private host handles concurrent request IDs, errors, and parent shutdown',
  { timeout: 15000 },
  async (t) => {
    const root = await mkdtemp(path.join(tmpdir(), 'rev-host-'));
    const repo = await createSampleRepo(path.join(root, 'repo'));
    const child = spawn(process.execPath, ['--import', 'tsx', 'core/main.ts'], {
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    const exited = once(child, 'exit');
    const lines = createInterface({ input: child.stdout });
    const pending = new Map<number | null, (value: any) => void>();
    lines.on('line', (line) => {
      const value = JSON.parse(line);
      pending.get(value.id)?.(value);
    });
    t.after(async () => {
      child.kill();
      await exited;
      lines.close();
      await rm(root, { recursive: true, force: true });
    });
    const call = (id: number | null, value: string) =>
      new Promise<any>((resolve) => {
        pending.set(id, resolve);
        child.stdin.write(value + '\n');
      });
    const [review, patch, invalid] = await Promise.all([
      call(
        1,
        JSON.stringify({
          id: 1,
          method: 'review',
          params: { repo, base: 'main' },
        }),
      ),
      call(
        2,
        JSON.stringify({
          id: 2,
          method: 'patch',
          params: { repo, base: 'main', path: 'src/lib/retry.ts' },
        }),
      ),
      call(null, 'not json'),
    ]);
    assert.ok(review.result.patches.length > 0);
    assert.match(patch.result.patch, /retry/);
    assert.equal(typeof invalid.error, 'string');
    const unknown = await call(
      3,
      JSON.stringify({ id: 3, method: 'unknown', params: { repo } }),
    );
    assert.match(unknown.error, /Unknown action/);
    child.stdin.end();
    const [code] = await exited;
    assert.equal(code, 0);
  },
);
