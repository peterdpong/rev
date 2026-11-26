import { execFile } from 'node:child_process';
import { access, constants } from 'node:fs/promises';
import path from 'node:path';

export type ConnectionState = 'ready' | 'signedOut' | 'missing' | 'error';

export type ConnectionStatus = {
  id: string;
  name: string;
  description: string;
  state: ConnectionState;
  path?: string;
  version?: string;
  account?: string;
  detail?: string;
};

type Output = { code: number; stdout: string; stderr: string };

type Auth = { signedIn: boolean; account?: string; detail?: string };

export type ConnectionDefinition = {
  id: string;
  name: string;
  description: string;
  executable: string;
  versionArgs: string[];
  parseVersion: (output: string) => string | undefined;
  installHint: string;
  auth?: (executable: string) => Promise<Auth>;
};

const TIMEOUT_MS = 5000;

function run(executable: string, args: string[]): Promise<Output> {
  return new Promise((resolve, reject) => {
    execFile(
      executable,
      args,
      {
        timeout: TIMEOUT_MS,
        encoding: 'utf8',
        env: { ...process.env, NO_COLOR: '1' },
      },
      (error, stdout, stderr) => {
        if (error && typeof error.code !== 'number') {
          reject(error);
          return;
        }
        resolve({
          code: typeof error?.code === 'number' ? error.code : 0,
          stdout,
          stderr,
        });
      },
    );
  });
}

export async function findExecutable(
  name: string,
  searchPath = process.env.PATH || '',
): Promise<string | undefined> {
  for (const dir of searchPath.split(path.delimiter)) {
    if (!dir) continue;
    const candidate = path.join(dir, name);
    try {
      await access(candidate, constants.X_OK);
      return candidate;
    } catch {}
  }
  return undefined;
}

export function parseGhVersion(output: string) {
  return /gh version (\S+)/.exec(output)?.[1];
}

export function parseGhAuth(output: Output): Auth {
  const text = `${output.stdout}\n${output.stderr}`;
  const account = /Logged in to \S+ (?:account|as) ([^\s(]+)/.exec(text)?.[1];
  if (output.code === 0 && account) {
    return { signedIn: true, account };
  }
  return { signedIn: false, detail: 'Run gh auth login in a terminal.' };
}

export const CONNECTIONS: ConnectionDefinition[] = [
  {
    id: 'gh',
    name: 'GitHub CLI',
    description: 'Pull requests',
    executable: 'gh',
    versionArgs: ['--version'],
    parseVersion: parseGhVersion,
    installHint: 'Install it with brew install gh.',
    auth: async (executable) =>
      parseGhAuth(
        await run(executable, ['auth', 'status', '--hostname', 'github.com']),
      ),
  },
];

export async function checkConnection(
  definition: ConnectionDefinition,
): Promise<ConnectionStatus> {
  const base = {
    id: definition.id,
    name: definition.name,
    description: definition.description,
  };
  const executable = await findExecutable(definition.executable);
  if (!executable) {
    return { ...base, state: 'missing', detail: definition.installHint };
  }
  try {
    const output = await run(executable, definition.versionArgs);
    const version = definition.parseVersion(
      `${output.stdout}\n${output.stderr}`,
    );
    if (!definition.auth) {
      return { ...base, state: 'ready', path: executable, version };
    }
    const auth = await definition.auth(executable);
    return {
      ...base,
      state: auth.signedIn ? 'ready' : 'signedOut',
      path: executable,
      version,
      account: auth.account,
      detail: auth.detail,
    };
  } catch (error) {
    return {
      ...base,
      state: 'error',
      path: executable,
      detail: error instanceof Error ? error.message : String(error),
    };
  }
}

export function checkConnections(definitions = CONNECTIONS) {
  return Promise.all(definitions.map(checkConnection));
}
