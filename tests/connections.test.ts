import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdir, mkdtemp, realpath, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';

import {
  CONNECTIONS,
  checkConnection,
  checkConnections,
  parseGhAuth,
  parseGhVersion,
} from '../core/connections';
import { HostService } from '../core/host';

test('gh version and both auth status formats are parsed', () => {
  assert.equal(
    parseGhVersion(
      'gh version 2.62.0 (2024-11-14)\nhttps://github.com/cli/cli/releases/tag/v2.62.0\n',
    ),
    '2.62.0',
  );
  assert.deepEqual(
    parseGhAuth({
      code: 0,
      stdout:
        'github.com\n  ✓ Logged in to github.com account octocat (keyring)\n',
      stderr: '',
    }),
    { signedIn: true, account: 'octocat' },
  );
  assert.deepEqual(
    parseGhAuth({
      code: 0,
      stdout: '',
      stderr:
        'github.com\n  ✓ Logged in to github.com as hubot (oauth_token)\n',
    }),
    { signedIn: true, account: 'hubot' },
  );
  assert.equal(
    parseGhAuth({
      code: 1,
      stdout: '',
      stderr: 'You are not logged into any GitHub hosts.',
    }).signedIn,
    false,
  );
});

async function fakeGh(t: test.TestContext, signedIn: boolean) {
  const root = await realpath(await mkdtemp(path.join(tmpdir(), 'rev-conn-')));
  t.after(() => rm(root, { recursive: true, force: true }));
  const bin = path.join(root, 'bin');
  await mkdir(bin);
  await writeFile(
    path.join(bin, 'gh'),
    `#!${process.execPath}
const args = process.argv.slice(2);
if (args[0] === '--version') {
  process.stdout.write('gh version 9.9.9 (2026-01-01)\\n');
} else if (args[0] === 'auth') {
  if (${signedIn}) {
    process.stdout.write('github.com\\n  ✓ Logged in to github.com account octocat (keyring)\\n');
  } else {
    process.stderr.write('You are not logged into any GitHub hosts.\\n');
    process.exit(1);
  }
}
`,
    { mode: 0o755 },
  );
  const previous = process.env.PATH;
  process.env.PATH = bin;
  t.after(() => {
    if (previous === undefined) delete process.env.PATH;
    else process.env.PATH = previous;
  });
  return path.join(bin, 'gh');
}

test('a signed-in gh is ready and reports its version and account', async (t) => {
  const gh = await fakeGh(t, true);
  const [status] = await checkConnections();
  assert.deepEqual(status, {
    id: 'gh',
    name: 'GitHub CLI',
    description: CONNECTIONS[0].description,
    state: 'ready',
    path: gh,
    version: '9.9.9',
    account: 'octocat',
    detail: undefined,
  });
  const host = new HostService();
  assert.equal((await host.call('connections'))[0].state, 'ready');
});

test('a signed-out gh asks for gh auth login', async (t) => {
  await fakeGh(t, false);
  const [status] = await checkConnections();
  assert.equal(status.state, 'signedOut');
  assert.equal(status.version, '9.9.9');
  assert.match(status.detail ?? '', /gh auth login/);
});

test('a missing executable reports how to install it', async () => {
  const status = await checkConnection({
    ...CONNECTIONS[0],
    executable: 'rev-test-missing-cli',
  });
  assert.equal(status.state, 'missing');
  assert.equal(status.path, undefined);
  assert.match(status.detail ?? '', /brew install gh/);
});
