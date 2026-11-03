import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import {
  command,
  git,
  inspect,
  filePatch,
  listCommits,
  resolveRef,
} from '../core/git';
import type { ReviewRequest, Review } from '../core/types';

async function fixture(t: any) {
  const repo = await mkdtemp(path.join(tmpdir(), 'rev-commits-'));
  t.after(() => rm(repo, { recursive: true, force: true }));
  await git(repo, ['init', '-b', 'main']);
  await git(repo, ['config', 'user.name', 'Review Test']);
  await git(repo, ['config', 'user.email', 'review@example.invalid']);
  const commit = async (name: string, content: string, message: string) => {
    await writeFile(path.join(repo, name), content);
    await git(repo, ['add', '--', name]);
    await git(repo, ['commit', '-m', message]);
    return resolveRef(repo, 'HEAD');
  };
  const root = await commit('root.txt', 'root\n', 'Initial commit');
  await git(repo, ['switch', '-c', 'feature']);
  const before = await commit(
    'before.txt',
    'earlier change\n',
    'Before the selection',
  );
  const first = await commit(
    'app.ts',
    'export const answer = 1;\n',
    'Add answer',
  );
  const last = await commit(
    'app.ts',
    'export const answer = 2;\n',
    'Update answer',
  );
  const after = await commit(
    'after.txt',
    'later change\n',
    'After the selection',
  );
  const request: ReviewRequest = { repo, base: 'main', head: 'feature' };
  return { repo, request, root, before, first, last, after, commit };
}

import { addComment, readState, mutateState } from '../core/store';
import { preparePrompt } from '../core/agents';
test('feedback and viewed markers stay with the selected range; agent prompts use the selected endpoints', async (t) => {
  const f = await fixture(t);
  const request = {
    ...f.request,
    commitRange: { start: f.first, end: f.last },
  };
  const review = await inspect(request);
  const selection = {
    path: 'app.ts',
    side: 'additions' as const,
    start: 1,
    end: 1,
  };
  const patch = await filePatch(review, 'app.ts');
  await addComment(review, selection, 'Explain this value', patch.digest);
  await mutateState(review, (state) => {
    state.viewed['app.ts'] = patch.digest;
  });
  assert.equal((await readState(review)).comments.length, 1);
  const full = await readState(await inspect(f.request));
  assert.equal(full.comments.length, 0);
  assert.deepEqual(full.viewed, {});
  assert.equal(
    (
      await readState(
        await inspect({
          ...f.request,
          commitRange: { start: f.last, end: f.last },
        }),
      )
    ).comments.length,
    0,
  );
  const prompt = await preparePrompt({
    review: request,
    provider: 'codex',
    mode: 'ask',
    question: 'Explain the selected commits',
  });
  assert.ok(
    prompt.prompt.includes(`Commit range (inclusive): ${f.first} → ${f.last}`),
  );
  assert.doesNotMatch(prompt.prompt, /before\.txt|after\.txt/);
  const changes = await preparePrompt({
    review: request,
    provider: 'codex',
    mode: 'apply',
  });
  assert.equal(changes.review.headSha, f.last);
  assert.match(changes.prompt, /Explain this value/);
});
