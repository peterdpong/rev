import { build } from 'esbuild';
await build({ entryPoints: ['src/cli.ts'], outfile: 'dist/cli.mjs', bundle: true, platform: 'node', target: 'node22', format: 'esm', packages: 'external' });
await build({ entryPoints: ['src/desktop/main.ts'], outfile: 'dist/desktop/main.cjs', bundle: true, platform: 'node', target: 'node22', format: 'cjs', external: ['electron'] });
await build({ entryPoints: ['src/desktop/preload.ts'], outfile: 'dist/desktop/preload.cjs', bundle: true, platform: 'node', target: 'node22', format: 'cjs', external: ['electron'] });
