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

import {
  importPR,
  inspect,
  lookupPullRequest,
  refreshPR,
  resolveRef,
  runGitText,
} from '../../core/git';
import { readPullRequest } from '../../core/git/pull-requests';
import { HostService } from '../../core/host';

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

  const metadata = {
    ...details,
    author: details.author.login,
    comparisonBaseRefName: 'main',
  };
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
    assert.deepEqual(await lookupPullRequest(request), metadata);
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

  // Refreshing keeps the review's mode and commit range, and recovers the PR
  // from its refs when the cached metadata is gone.
  const imported = await importPR(repo, '42');
  const scope = { start: head, end: head };
  const expected = { ...imported, commitRange: scope };
  assert.deepEqual(
    await refreshPR(await inspect({ ...imported, commitRange: scope })),
    expected,
  );
  await rm(path.join(metadataDir, files[0]));
  const uncached = await inspect({ ...imported, commitRange: scope });
  assert.equal(uncached.pullRequest, undefined);
  assert.deepEqual(await refreshPR(uncached), expected);
  assert.equal(await refreshPR(await inspect({ repo })), undefined);

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

test('branch PR headers load independently and tolerate unavailable metadata', async (t) => {
  const root = await realpath(
    await mkdtemp(path.join(tmpdir(), 'rev-pr-header-')),
  );
  t.after(() => rm(root, { recursive: true, force: true }));
  const repo = path.join(root, 'repo');
  const bin = path.join(root, 'bin');
  await mkdir(repo);
  await mkdir(bin);
  await runGitText(repo, ['init', '-b', 'feature']);
  await runGitText(repo, ['config', 'user.name', 'Rev Test']);
  await runGitText(repo, ['config', 'user.email', 'test@example.invalid']);
  await runGitText(repo, ['commit', '--allow-empty', '-m', 'Initial']);
  const head = await resolveRef(repo, 'HEAD');
  await runGitText(repo, ['tag', 'release']);
  await runGitText(repo, [
    'remote',
    'add',
    'origin',
    'https://github.com/rev-test/widget.git',
  ]);
  await runGitText(repo, ['update-ref', 'refs/remotes/origin/feature', head]);
  const details = {
    number: 42,
    title: 'Improve the header',
    body: 'A detailed description.',
    url: 'https://github.com/rev-test/widget/pull/42',
    author: { login: 'reviewer' },
    state: 'OPEN',
    isDraft: false,
    headRefName: 'feature',
    baseRefName: 'main',
    updatedAt: '2026-01-01T00:00:00Z',
  };
  const response = path.join(root, 'response.json');
  const calls = path.join(root, 'calls.jsonl');
  await writeFile(response, JSON.stringify(details));
  await writeFile(
    path.join(bin, 'gh'),
    `#!${process.execPath}
const fs = require('node:fs');
fs.appendFileSync(${JSON.stringify(calls)}, JSON.stringify(process.argv.slice(2)) + '\\n');
const response = fs.readFileSync(${JSON.stringify(response)}, 'utf8');
if (response === 'unavailable') process.exit(1);
process.stdout.write(response);
`,
    { mode: 0o755 },
  );
  const previousPath = process.env.PATH;
  process.env.PATH = `${bin}${path.delimiter}${previousPath || ''}`;
  t.after(() => {
    if (previousPath === undefined) delete process.env.PATH;
    else process.env.PATH = previousPath;
  });
  const service = new HostService();
  t.after(() => service.close());
  for (const selected of [
    'HEAD',
    'feature',
    'refs/heads/feature',
    'origin/feature',
  ]) {
    assert.deepEqual(
      await service.call('pullRequest', { repo, head: selected }),
      {
        ...details,
        author: 'reviewer',
      },
    );
  }
  const recorded = await readFile(calls, 'utf8');
  for (const line of recorded.trim().split('\n')) {
    assert.deepEqual(JSON.parse(line).slice(-2), ['--', 'feature']);
  }
  // Don't infer branch associations for a commit, tag or detached checkout.
  for (const selected of [head, 'release']) {
    assert.equal(
      await service.call('pullRequest', { repo, head: selected }),
      null,
    );
  }
  await runGitText(repo, ['switch', '--detach', head]);
  assert.equal(await service.call('pullRequest', { repo }), null);
  assert.equal(await readFile(calls, 'utf8'), recorded);

  // Failed queries, malformed output and a different branch's PR are optional.
  for (const output of [
    'unavailable',
    'invalid JSON',
    'null',
    JSON.stringify({ ...details, headRefName: 'another-feature' }),
  ]) {
    await writeFile(response, output);
    assert.equal(
      await service.call('pullRequest', { repo, head: 'feature' }),
      null,
    );
  }
  await assert.rejects(
    service.call('pullRequest', { repo: '' }),
    /Pass a repository folder/,
  );
});
