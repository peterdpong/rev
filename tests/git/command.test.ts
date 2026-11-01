import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import {
  CommandError,
  runCommandText,
  runGitText,
  inspect,
  parseDiffSummary,
  rootOf,
} from '../../core/git';

const failure = (reason: string, message: RegExp) => (error: unknown) =>
  error instanceof CommandError &&
  error.reason === reason &&
  message.test(error.message);

test('a missing folder is not reported as a missing git', async () => {
  await assert.rejects(
    rootOf('/does/not/exist'),
    failure('missing-folder', /Folder not found/),
  );
  await assert.rejects(
    runCommandText('definitely-not-a-real-tool', []),
    failure('missing-executable', /not installed/),
  );
});

test('oversized output is reported as too-large even when stderr has text', async (t) => {
  const repo = await mkdtemp(path.join(tmpdir(), 'rev-command-'));
  t.after(() => rm(repo, { recursive: true, force: true }));
  await runGitText(repo, ['init', '-b', 'main']);
  await writeFile(path.join(repo, 'big.txt'), 'x'.repeat(10_000));
  await assert.rejects(
    runCommandText('sh', ['-c', 'echo warning >&2; cat big.txt'], repo, 1_000),
    failure('too-large', /more output/),
  );
});

test('a non-folder repo path is rejected clearly', async (t) => {
  const dir = await mkdtemp(path.join(tmpdir(), 'rev-command-'));
  t.after(() => rm(dir, { recursive: true, force: true }));
  await writeFile(path.join(dir, 'file'), '');
  await assert.rejects(
    inspect({ repo: path.join(dir, 'file') }),
    /Folder not found/,
  );
});

test('parseDiffSummary combines raw records with numstat counts', () => {
  const raw = [
    ':100644 100644 aaaaaaa bbbbbbb M',
    'b.bin',
    ':000000 100644 0000000 ccccccc A',
    'tab\there.txt',
    ':100644 100644 ddddddd ddddddd R100',
    'old.txt',
    'new\nname.txt',
    '-\t-\tb.bin',
    '3\t0\ttab\there.txt',
    '0\t0\t',
    'old.txt',
    'new\nname.txt',
    '',
  ].join('\0');
  assert.deepEqual(parseDiffSummary(raw), [
    { status: 'M', path: 'b.bin', added: 0, removed: 0, binary: true },
    { status: 'A', path: 'tab\there.txt', added: 3, removed: 0, binary: false },
    {
      status: 'R',
      oldPath: 'old.txt',
      path: 'new\nname.txt',
      added: 0,
      removed: 0,
      binary: false,
    },
  ]);
});
