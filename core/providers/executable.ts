import { access } from 'node:fs/promises';
import { constants } from 'node:fs';
import { homedir } from 'node:os';
import path from 'node:path';
import type { Provider } from '../types';

export async function executable(provider: Provider) {
  if (!['codex', 'claude'].includes(provider)) {
    throw new Error('Choose Codex or Claude Code.');
  }
  const override =
    process.env[`REV_${provider.toUpperCase()}_PATH`] ||
    process.env[`REVUE_${provider.toUpperCase()}_PATH`];
  const dirs = (process.env.PATH || '')
    .split(path.delimiter)
    .filter(Boolean)
    .concat([
      path.join(homedir(), '.local/bin'),
      '/opt/homebrew/bin',
      '/usr/local/bin',
      '/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS',
    ]);
  for (const candidate of override
    ? [override]
    : dirs.map((dir) => path.join(dir, provider))) {
    try {
      await access(candidate, constants.X_OK);
      return path.resolve(candidate);
    } catch {
      /* try next */
    }
  }
  throw new Error(
    `${provider === 'codex' ? 'Codex' : 'Claude Code'} was not found. Install its CLI, then restart Rev, or set REV_${provider.toUpperCase()}_PATH.`,
  );
}
