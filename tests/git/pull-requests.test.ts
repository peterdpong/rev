import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  realpath,
  rm,
  writeFile,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

import { importPR, inspect, resolveRef, runGitText } from '../../core/git';
import { readPullRequest } from '../../core/git/pull-requests';

test('PR imports use the rev namespace and metadata without changing the checkout', async (t) => {
  const root = await realpath(await mkdtemp(path.join(tmpdir(), 'rev-pr-')));
  t.after(() => rm(root, { recursive: true, force: true }));

  const source = path.join(root, 'source');
  const repo = path.join(root, 'checkout');
  await mkdir(source);
  await runGitText(source, ['init', '-b', 'main']);
  await runGitText(source, ['config', 'user.name', 'Rev Test']);
  await runGitText(source, ['config', 'user.email', 'test@example.invalid']);

  await writeFile(path.join(source, 'file.txt'), 'before\n');
  await runGitText(source, ['add', '.']);
  await runGitText(source, ['commit', '-m', 'Base']);
  const base = await resolveRef(source, 'HEAD');

  await runGitText(source, ['switch', '-c', 'feature']);
  await writeFile(path.join(source, 'file.txt'), 'after\n');
  await runGitText(source, ['commit', '-am', 'PR change']);
  const head = await resolveRef(source, 'HEAD');

  await runGitText(source, ['update-ref', 'refs/pull/42/head', head]);
  await runGitText(root, ['clone', '--branch', 'main', source, repo]);

  // Redirect the GitHub fetch to this test's local repository and forbid network protocols.
  await runGitText(repo, [
    'config',
    `url.${pathToFileURL(source).href}.insteadOf`,
    'https://github.com/rev-test/widget.git',
  ]);
  await runGitText(repo, ['config', 'protocol.allow', 'never']);
  await runGitText(repo, ['config', 'protocol.file.allow', 'always']);

  const details = {
    number: 42,
    title: 'Update file',
    body: 'A test pull request.',
    url: 'https://github.com/rev-test/widget/pull/42',
    author: { login: 'reviewer' },
    state: 'OPEN',
    isDraft: false,
    headRefName: 'feature',
    baseRefName: 'main',
    updatedAt: '2026-01-01T00:00:00Z',
  };

  const bin = path.join(root, 'bin');
  await mkdir(bin);

  // Supply predictable gh output without requiring a GitHub account or network access.
  await writeFile(
    path.join(bin, 'gh'),
    `#!${process.execPath}\nprocess.stdout.write(${JSON.stringify(JSON.stringify(details))});\n`,
    { mode: 0o755 },
  );

  const previousPath = process.env.PATH;
  process.env.PATH = `${bin}${path.delimiter}${previousPath || ''}`;
  t.after(() => {
    if (previousPath === undefined) delete process.env.PATH;
    else process.env.PATH = previousPath;
  });

  await writeFile(path.join(repo, 'file.txt'), 'keep my edits\n');
  await writeFile(path.join(repo, 'untracked.txt'), 'keep this too\n');
  const status = await runGitText(repo, ['status', '--porcelain=v1', '-z']);

  const metadata = { ...details, author: details.author.login };
  for (const value of ['42', details.url]) {
    const request = await importPR(repo, value);

    assert.deepEqual(request, {
      repo,
      base: 'refs/rev/rev-test/widget/42/base',
      head: 'refs/rev/rev-test/widget/42/head',
      mode: 'branch',
    });

    const review = await inspect(request);

    assert.equal(review.baseSha, base);
    assert.equal(review.headSha, head);
    assert.deepEqual(review.pullRequest, metadata);
    assert.ok(review.branches.includes('rev/rev-test/widget/42/head'));
    assert.ok(review.branches.includes('rev/rev-test/widget/42/base'));
    assert.deepEqual(
      review.files.map((file) => file.path),
      ['file.txt'],
    );
  }

  const metadataDir = path.join(repo, '.git', 'rev');
  const files = await readdir(metadataDir);

  assert.equal(files.length, 1);
  assert.deepEqual(
    JSON.parse(await readFile(path.join(metadataDir, files[0]), 'utf8')),
    metadata,
  );

  assert.equal(
    (await runGitText(repo, ['branch', '--show-current'])).trim(),
    'main',
  );
  assert.equal(await resolveRef(repo, 'HEAD'), base);
  assert.equal(
    await runGitText(repo, ['status', '--porcelain=v1', '-z']),
    status,
  );

  assert.equal(
    await readFile(path.join(repo, 'file.txt'), 'utf8'),
    'keep my edits\n',
  );
  assert.equal(
    await readFile(path.join(repo, 'untracked.txt'), 'utf8'),
    'keep this too\n',
  );

  assert.equal(await readPullRequest(repo, 'HEAD'), undefined);
  assert.equal(
    await readPullRequest(repo, 'refs/rev/rev-test/widget/99/head'),
    undefined,
  );

  await assert.rejects(importPR(repo, '--help'), /Enter a PR number/);
});
