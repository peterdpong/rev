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

test('CLI supports a single commit, a range, and history listing', async (t) => {
  const f = await fixture(t);
  const cli = (...args: string[]) =>
    command(
      process.execPath,
      ['--import', 'tsx', 'cli/main.ts', ...args, '--repo', f.repo],
      process.cwd(),
    );
  const single = await cli('diff', '--commit', f.last, '--no-pager');
  assert.match(single, /-export const answer = 1;/);
  const range = await cli(
    'diff',
    '--from',
    f.first,
    '--to',
    f.last,
    '--no-pager',
  );
  assert.match(range, /2 commits, inclusive/);
  assert.doesNotMatch(range, /before\.txt|after\.txt/);
  assert.match(await cli('commits'), /Update answer/);
  await assert.rejects(cli('diff', '--from', f.first), /together/);
});
