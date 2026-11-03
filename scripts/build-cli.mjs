import { build } from 'esbuild';
await build({
  entryPoints: { cli: 'cli/main.ts' },
  outdir: 'dist',
  outExtension: { '.js': '.mjs' },
  bundle: true,
  platform: 'node',
  target: 'node22',
  format: 'esm',
  packages: 'external',
});
