import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createInterface } from 'node:readline';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { runGitText } from '../core/git';
import { HostService } from '../core/host';

test('the host requires an explicit local repository folder', async (t) => {
  const host = new HostService();
  t.after(() => host.close());
  for (const repo of [undefined, '', '   ']) {
    await assert.rejects(
      // @ts-expect-error Untyped callers can still omit the folder.
      host.call('review', { repo }),
      /Pass a repository folder/,
    );
  }
  const folder = await mkdtemp(path.join(tmpdir(), 'rev-not-a-repo-'));
  t.after(() => rm(folder, { recursive: true, force: true }));
  await assert.rejects(
    host.call('review', { repo: folder }),
    /not a git repository/i,
  );
});

test(
  'private host handles concurrent request IDs, errors, and parent shutdown',
  { timeout: 15000 },
  async (t) => {
    const repo = await mkdtemp(path.join(tmpdir(), 'rev-host-'));
    t.after(() => rm(repo, { recursive: true, force: true }));
    await runGitText(repo, ['init', '-b', 'main']);
    await runGitText(repo, ['config', 'user.name', 'Rev Test']);
    await runGitText(repo, ['config', 'user.email', 'test@example.invalid']);
    await writeFile(path.join(repo, 'file.txt'), 'before\n');
    await runGitText(repo, ['add', '.']);
    await runGitText(repo, ['commit', '-m', 'base']);
    await runGitText(repo, ['switch', '-c', 'feature']);
    await writeFile(path.join(repo, 'file.txt'), 'after\n');
    await runGitText(repo, ['commit', '-am', 'change']);
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
          params: { review: { repo, base: 'main' }, path: 'file.txt' },
        }),
      ),
      call(null, 'not json'),
    ]);
    assert.ok(review.result.patches.length > 0);
    assert.match(patch.result.patch, /\+after/);
    assert.equal(typeof invalid.error, 'string');
    const unknown = await call(
      3,
      JSON.stringify({ id: 3, method: 'unknown', params: { repo } }),
    );
    assert.match(unknown.error, /Unknown action/);
    const malformed = await call(
      4,
      JSON.stringify({
        id: 4,
        method: 'checkouts',
        params: { repos: [repo, 42] },
      }),
    );
    assert.match(malformed.error, /Pass a list of repository folders/);
    const malformedReview = await call(
      5,
      JSON.stringify({
        id: 5,
        method: 'patch',
        params: { review: { repo, mode: 42 }, path: 'file.txt' },
      }),
    );
    assert.match(malformedReview.error, /Unknown comparison mode/);
    child.stdin.end();
    const [code] = await exited;
    assert.equal(code, 0);
  },
);
