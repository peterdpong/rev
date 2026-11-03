import { parseArgs } from 'node:util';
import { inspect, filePatch } from '../core/git';

async function main() {
  const [command = 'help', ...args] = process.argv.slice(2);
  if (command === 'help') {
    console.log(
      'rev diff|inspect [repo] [--base main] [--head feature] [--working] [--file path]',
    );
    return;
  }
  if (!['diff', 'inspect'].includes(command)) {
    throw new Error('Use diff, inspect, or help.');
  }
  const { values, positionals } = parseArgs({
    args,
    allowPositionals: true,
    options: {
      base: { type: 'string' },
      head: { type: 'string' },
      working: { type: 'boolean' },
      file: { type: 'string' },
    },
  });
  if (positionals.length > 1) {
    throw new Error('Pass one repository folder.');
  }
  const review = await inspect({
    repo: positionals[0] || process.cwd(),
    base: values.base,
    head: values.head,
    mode: values.working ? 'working' : 'branch',
  });
  if (command === 'inspect') {
    console.log(JSON.stringify(review, null, 2));
    return;
  }
  for (const path of values.file
    ? [values.file]
    : review.files.map((file) => file.path)) {
    const patch = await filePatch(review, path);
    if (patch.tooLarge) {
      throw new Error(`Diff exceeds the inline limit: ${path}`);
    }
    process.stdout.write(patch.patch);
  }
}

main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
