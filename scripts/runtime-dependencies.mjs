import { execFileSync } from 'node:child_process';
import { cp, mkdir } from 'node:fs/promises';
import path from 'node:path';

// esbuild leaves packages external. Keep the SDK's relative resources and its
// runtime dependency graph intact in the .app; never bundle another agent CLI.
export async function copyRuntimeDependencies(
  destination,
  root = process.cwd(),
) {
  const packages = execFileSync(
    'npm',
    ['ls', '--omit=dev', '--all', '--parseable'],
    { cwd: root, encoding: 'utf8' },
  )
    .trim()
    .split('\n');
  const selected = packages.filter(
    (file) =>
      file !== root &&
      !/[/\\]@anthropic-ai[/\\]claude-agent-sdk-[^/\\]+$/.test(file),
  );
  for (const file of selected) {
    const relative = path.relative(root, file);
    if (!relative.startsWith('node_modules' + path.sep)) {
      throw new Error(`Dependency is outside the project: ${file}`);
    }
    const target = path.join(destination, relative);
    await mkdir(path.dirname(target), { recursive: true });
    await cp(file, target, {
      recursive: true,
      filter: (source) =>
        source === file ||
        !path.relative(file, source).split(path.sep).includes('node_modules'),
    });
  }
}
