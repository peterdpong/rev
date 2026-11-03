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
import { AgentRunner, preparePrompt } from '../core/agents';
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

async function finish(run: AgentRun) {
  for (let i = 0; i < 300 && run.status === 'running'; i++) {
    await new Promise((r) => setTimeout(r, 20));
  }
  assert.notEqual(run.status, 'running');
}

test('real child-process transport sends all feedback and marks sent only on explicit completion', async (t) => {
  const r = await fixture(t);
  const review = await inspect(r);
  await addComment(review, selection, 'Make it 3.');
  const binary = path.join(r.repo, '.git', 'fake-codex.cjs');
  await writeFakeCodex(
    binary,
    `assert.ok(prompt.includes('Make it 3.')); text('Changed.'); complete();`,
  );
  const old = process.env.REVUE_CODEX_PATH;
  process.env.REVUE_CODEX_PATH = binary;
  t.after(() => {
    if (old) {
      process.env.REVUE_CODEX_PATH = old;
    } else {
      delete process.env.REVUE_CODEX_PATH;
    }
  });
  const runner = new AgentRunner();
  const run = await runner.start({
    review: r,
    provider: 'codex',
    mode: 'apply',
  });
  await finish(run);
  assert.equal(run.status, 'completed');
  assert.ok(run.events.some((e) => e.text === 'Changed.'));
  assert.equal((await readState(review)).comments[0].status, 'sent');
  await flushRunHistory(run.id);
  const discussion = (await readState(review)).threads![0];
  assert.equal(discussion.kind, 'change');
  assert.equal(discussion.turns[0].status, 'completed');
  assert.ok(
    discussion.turns[0].events.some((event) => event.text === 'Changed.'),
  );
  await mutateState(review, (s) => {
    s.comments[0].status = 'queued';
  });
  await writeFakeCodex(binary, `complete('failed');`);
  const failed = await runner.start({
    review: r,
    provider: 'codex',
    mode: 'apply',
  });
  await finish(failed);
  assert.equal(failed.status, 'failed');
  assert.equal((await readState(review)).comments[0].status, 'queued');
});

test('questions against a committed snapshot read an isolated checkout when the original is dirty', async (t) => {
  const r = await fixture(t);
  await writeFile(path.join(r.repo, 'app.ts'), 'dirty unrelated local edit\n');
  const binary = path.join(r.repo, '.git', 'snapshot-agent.cjs');
  await writeFakeCodex(
    binary,
    `text(fs.readFileSync('app.ts', 'utf8')); complete();`,
  );
  const old = process.env.REVUE_CODEX_PATH;
  process.env.REVUE_CODEX_PATH = binary;
  t.after(() => {
    if (old) {
      process.env.REVUE_CODEX_PATH = old;
    } else {
      delete process.env.REVUE_CODEX_PATH;
    }
  });
  const runner = new AgentRunner();
  const run = await runner.start({
    review: r,
    provider: 'codex',
    mode: 'ask',
    question: 'What changed?',
  });
  await finish(run);
  assert.equal(run.status, 'completed');
  assert.ok(
    run.events.some(
      (e) => e.kind === 'message' && e.text.includes('answer = 2'),
    ),
  );
  assert.match(
    await readFile(path.join(r.repo, 'app.ts'), 'utf8'),
    /dirty unrelated/,
  );
  for (let i = 0; i < 40; i++) {
    if (
      !(await git(r.repo, ['worktree', 'list', '--porcelain'])).includes(
        'rev-context-',
      )
    ) {
      break;
    }
    await new Promise((r) => setTimeout(r, 25));
  }
  assert.doesNotMatch(
    await git(r.repo, ['worktree', 'list', '--porcelain']),
    /rev-context-/,
  );
});

test('cancellation stops the provider and preserves queued comments', async (t) => {
  const r = await fixture(t);
  const review = await inspect(r);
  await addComment(review, selection, 'Change this.');
  const binary = path.join(r.repo, '.git', 'slow-agent.cjs');
  await writeFakeCodex(binary, `text('Ready');`);
  const old = process.env.REVUE_CODEX_PATH;
  process.env.REVUE_CODEX_PATH = binary;
  t.after(() => {
    if (old) {
      process.env.REVUE_CODEX_PATH = old;
    } else {
      delete process.env.REVUE_CODEX_PATH;
    }
  });
  const runner = new AgentRunner();
  const run = await runner.start({
    review: r,
    provider: 'codex',
    mode: 'apply',
  });
  for (let i = 0; i < 100 && !run.events.some((e) => e.text === 'Ready'); i++) {
    await new Promise((r) => setTimeout(r, 20));
  }
  runner.cancel(run.id);
  await finish(run);
  assert.equal(run.status, 'cancelled');
  assert.equal((await readState(review)).comments[0].status, 'queued');
});
