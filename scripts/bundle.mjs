import { execFileSync } from 'node:child_process';
import { mkdir, copyFile, writeFile, chmod } from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { copyRuntimeDependencies } from './runtime-dependencies.mjs';

if (process.platform !== 'darwin') {
  throw new Error(
    'The app bundler currently targets macOS. Use cargo build --release on other platforms.',
  );
}
execFileSync('cargo', ['build', '--release', '--locked'], { stdio: 'inherit' });
const app = path.resolve('dist/Rev.app');
const macos = path.join(app, 'Contents/MacOS');
const resources = path.join(app, 'Contents/Resources');
await mkdir(macos, { recursive: true });
await mkdir(resources, { recursive: true });
const metadata = JSON.parse(
  execFileSync('cargo', ['metadata', '--no-deps', '--format-version', '1'], {
    encoding: 'utf8',
  }),
);
await copyFile(
  path.join(metadata.target_directory, 'release/rev'),
  path.join(macos, 'rev'),
);
// Homebrew's small node launcher depends on dylibs outside the app. Use an
// official standalone distribution if the current executable is not portable.
let node = process.env.REV_BUNDLE_NODE || process.execPath;

const isPortable = (binary) =>
  execFileSync('otool', ['-L', binary], { encoding: 'utf8' })
    .split('\n')
    .slice(1)
    .filter((line) => line.trim())
    .every((line) => /^\s*\/(usr\/lib|System\/Library)\//.test(line));

if (!isPortable(node)) {
  if (process.env.REV_BUNDLE_NODE) {
    throw new Error(
      'REV_BUNDLE_NODE must point to a standalone Node distribution.',
    );
  }
  const version = 'v24.19.0';
  const name = `node-${version}-darwin-${process.arch}`;
  const file = `${name}.tar.gz`;
  const runtime = path.resolve('dist/runtime');
  await mkdir(runtime, { recursive: true });
  const fetchText = async (url) => {
    const response = await fetch(url);
    if (!response.ok) {
      throw new Error(`Download failed: ${response.status} ${url}`);
    }
    return response;
  };
  const manifest = await (
    await fetchText(`https://nodejs.org/dist/${version}/SHASUMS256.txt`)
  ).text();
  const expected = manifest
    .split('\n')
    .find((line) => line.trim().endsWith(`  ${file}`))
    ?.split(/\s+/)[0];
  if (!expected) {
    throw new Error('Official Node checksum not found.');
  }
  const archive = path.join(runtime, file);
  let bytes;
  try {
    bytes = await readFile(archive);
  } catch {
    /* first build */
  }
  if (!bytes || createHash('sha256').update(bytes).digest('hex') !== expected) {
    console.log(`Downloading standalone Node ${version}…`);
    bytes = Buffer.from(
      await (
        await fetchText(`https://nodejs.org/dist/${version}/${file}`)
      ).arrayBuffer(),
    );
    if (createHash('sha256').update(bytes).digest('hex') !== expected) {
      throw new Error('Node archive checksum mismatch.');
    }
    await writeFile(archive, bytes);
  }
  execFileSync('tar', ['-xzf', archive, '-C', runtime]);
  node = path.join(runtime, name, 'bin/node');
  if (!isPortable(node)) {
    throw new Error('The downloaded Node binary has external dependencies.');
  }
  await copyFile(
    path.join(runtime, name, 'LICENSE'),
    path.join(resources, 'NODE-LICENSE'),
  );
}
await copyFile(node, path.join(resources, 'node'));
for (const file of ['host.mjs', 'cli.mjs']) {
  await copyFile(path.join('dist', file), path.join(resources, file));
}
await copyRuntimeDependencies(resources);
await chmod(path.join(macos, 'rev'), 0o755);
await chmod(path.join(resources, 'node'), 0o755);
execFileSync(path.join(resources, 'node'), ['--version'], { stdio: 'inherit' });
await writeFile(
  path.join(app, 'Contents/Info.plist'),
  `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Rev</string>
<key>CFBundleDisplayName</key><string>Rev</string>
<key>CFBundleIdentifier</key><string>dev.peterdpong.rev</string>
<key>CFBundleExecutable</key><string>rev</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundleVersion</key><string>1</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSSupportsAutomaticGraphicsSwitching</key><true/>
<key>LSMinimumSystemVersion</key><string>13.5</string>
</dict></plist>`,
);
execFileSync('codesign', ['--force', '--deep', '--sign', '-', app], {
  stdio: 'inherit',
});
console.log(`Built ${app}`);
