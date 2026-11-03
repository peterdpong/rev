import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  mkdtemp,
  writeFile,
  readFile,
  rm,
  mkdir,
  chmod,
  symlink,
} from 'node:fs/promises';
import path from 'node:path';
import { tmpdir } from 'node:os';
import { git, inspect, filePatch, resolveRef } from '../core/git';
import { readState, addComment, mutateState } from '../core/store';
import { preparePrompt } from '../core/agents';
import { flushRunHistory } from '../core/threads';
import { writeFakeCodex } from './fake-codex';
import type { AgentRun, ReviewRequest } from '../core/types';

async function fixture(t: any) {
  const repo = await mkdtemp(path.join(tmpdir(), 'revue-test-'));
  t.after(() => rm(repo, { recursive: true, force: true }));
  await git(repo, ['init', '-b', 'main']);
  await git(repo, ['config', 'user.name', 'Test']);
  await git(repo, ['config', 'user.email', 'test@example.invalid']);
  await writeFile(path.join(repo, 'app.ts'), 'export const answer = 1;\n');
  await git(repo, ['add', '.']);
  await git(repo, ['commit', '-m', 'base']);
  await git(repo, ['switch', '-c', 'feature']);
  await writeFile(path.join(repo, 'app.ts'), 'export const answer = 2;\n');
  await git(repo, ['commit', '-am', 'change']);
  return { repo, base: 'main', head: 'HEAD' } satisfies ReviewRequest;
}

const selection = {
  path: 'app.ts',
  side: 'additions' as const,
  start: 1,
  end: 1,
};

test('feedback persists, concurrent mutations do not lose comments, and stale feedback is rejected', async (t) => {
  const r = await fixture(t);
  const review = await inspect(r);
  await Promise.all(
    Array.from({ length: 6 }, (_, i) =>
      addComment(review, selection, `Request ${i}`),
    ),
  );
  assert.equal((await readState(review)).comments.length, 6);
  const prompt = await preparePrompt({
    review: r,
    provider: 'codex',
    mode: 'apply',
  });
  assert.equal(prompt.ids.length, 6);
  assert.match(prompt.prompt, /Request 5/);
  await writeFile(path.join(r.repo, 'app.ts'), 'export const answer = 99;\n');
  await git(r.repo, ['commit', '-am', 'changed meanwhile']);
  await assert.rejects(
    preparePrompt({ review: r, provider: 'codex', mode: 'apply' }),
    /stale/,
  );
});

test('blocks edits against another checkout and against unseen dirty changes', async (t) => {
  const r = await fixture(t);
  await addComment(await inspect(r), selection, 'Change this.');
  await writeFile(path.join(r.repo, 'app.ts'), 'dirty\n');
  await assert.rejects(
    preparePrompt({ review: r, provider: 'codex', mode: 'apply' }),
    /local changes/,
  );
  await git(r.repo, ['restore', 'app.ts']);
  await git(r.repo, ['switch', 'main']);
  const other = { ...r, head: 'feature' };
  await assert.rejects(
    preparePrompt({ review: other, provider: 'codex', mode: 'apply' }),
    /checkout of the reviewed branch/,
  );
});

test('a general question includes actual patches, not only filenames', async (t) => {
  const r = await fixture(t);
  const prepared = await preparePrompt({
    review: r,
    provider: 'claude',
    mode: 'ask',
    question: 'What changed?',
  });
  assert.match(prepared.prompt, /\+export const answer = 2/);
  assert.match(prepared.prompt, /Answer only/);
});

async function finish(run: AgentRun) {
  for (let i = 0; i < 300 && run.status === 'running'; i++) {
    await new Promise((r) => setTimeout(r, 20));
  }
  assert.notEqual(run.status, 'running');
}
