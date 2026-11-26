import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  mkdir,
  mkdtemp,
  readFile,
  realpath,
  rm,
  writeFile,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';

import {
  mergeMethods,
  pullRequestActionArgs,
  runGitText,
  runPullRequestAction,
} from '../../core/git';
import { HostService } from '../../core/host';

const URL = 'https://github.com/rev-test/widget/pull/42';

test('pull request actions map to gh commands for the PR URL', () => {
  assert.deepEqual(pullRequestActionArgs(URL, { type: 'ready' }), [
    'pr',
    'ready',
    URL,
  ]);
  assert.deepEqual(pullRequestActionArgs(URL, { type: 'draft' }), [
    'pr',
    'ready',
    '--undo',
    URL,
  ]);
  for (const method of ['squash', 'merge', 'rebase'] as const) {
    assert.deepEqual(pullRequestActionArgs(URL, { type: 'merge', method }), [
      'pr',
      'merge',
      `--${method}`,
      URL,
    ]);
  }
  assert.throws(
    () =>
      pullRequestActionArgs(URL, {
        type: 'merge',
        method: 'admin' as 'squash',
      }),
    /Unknown merge method/,
  );
  assert.throws(
    () => pullRequestActionArgs(URL, { type: 'close' } as never),
    /Unknown pull request action/,
  );
  assert.throws(
    () => pullRequestActionArgs('--admin', { type: 'ready' }),
    /github\.com pull request URL/,
  );
});

test('merge methods and actions run gh in the repository', async (t) => {
  const root = await realpath(
    await mkdtemp(path.join(tmpdir(), 'rev-pr-actions-')),
  );
  t.after(() => rm(root, { recursive: true, force: true }));
  const repo = path.join(root, 'repo');
  await mkdir(repo);
  await runGitText(repo, ['init', '-b', 'main']);

  const bin = path.join(root, 'bin');
  const log = path.join(root, 'gh.log');
  await mkdir(bin);
  const details = {
    number: 42,
    title: 'Update file',
    body: '',
    url: URL,
    author: { login: 'reviewer' },
    state: 'OPEN',
    isDraft: false,
    headRefName: 'feature',
    baseRefName: 'main',
    updatedAt: '2026-01-01T00:00:00Z',
  };
  await writeFile(
    path.join(bin, 'gh'),
    `#!${process.execPath}
const fs = require('node:fs');
const args = process.argv.slice(2);
fs.appendFileSync(${JSON.stringify(log)}, JSON.stringify(args) + '\\n');
if (args[0] === 'repo') {
  process.stdout.write(JSON.stringify({ squashMergeAllowed: true, mergeCommitAllowed: false, rebaseMergeAllowed: true }));
} else if (args[1] === 'view') {
  process.stdout.write(${JSON.stringify(JSON.stringify(details))});
}
`,
    { mode: 0o755 },
  );
  const previousPath = process.env.PATH;
  process.env.PATH = `${bin}${path.delimiter}${previousPath || ''}`;
  t.after(() => {
    if (previousPath === undefined) delete process.env.PATH;
    else process.env.PATH = previousPath;
  });

  assert.deepEqual(await mergeMethods(repo, URL), ['squash', 'rebase']);

  const pr = await runPullRequestAction(repo, URL, {
    type: 'merge',
    method: 'squash',
  });
  assert.equal(pr.author, 'reviewer');
  assert.equal(pr.number, 42);

  const host = new HostService();
  await host.call('pullRequestAction', {
    repo,
    url: URL,
    action: { type: 'draft' },
  });

  const calls = (await readFile(log, 'utf8'))
    .trim()
    .split('\n')
    .map((line) => JSON.parse(line));
  assert.deepEqual(calls, [
    [
      'repo',
      'view',
      'rev-test/widget',
      '--json',
      'squashMergeAllowed,mergeCommitAllowed,rebaseMergeAllowed',
    ],
    ['pr', 'merge', '--squash', URL],
    [
      'pr',
      'view',
      URL,
      '--json',
      'number,title,body,author,state,isDraft,headRefName,baseRefName,updatedAt,url',
    ],
    ['pr', 'ready', '--undo', URL],
    [
      'pr',
      'view',
      URL,
      '--json',
      'number,title,body,author,state,isDraft,headRefName,baseRefName,updatedAt,url',
    ],
  ]);
});
