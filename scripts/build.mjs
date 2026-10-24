import { build } from 'esbuild';
await build({
  entryPoints: { cli: 'src/cli.ts', host: 'src/host.ts' }, outdir: 'dist', outExtension: { '.js': '.mjs' },
  bundle: true, platform: 'node', target: 'node22', format: 'esm', packages: 'external',
});
