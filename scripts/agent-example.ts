import { parseArgs } from 'node:util';
import { rootOf } from '../core/git';
import { codex } from '../core/providers/codex';
import { executable } from '../core/providers/executable';

// A connection example, deliberately separate from the PR review runner.
async function main() {
  const { values, positionals } = parseArgs({
    allowPositionals: true,
    options: {
      repo: { type: 'string' },
      session: { type: 'string' },
    },
  });
  const question = positionals.join(' ').trim();
  if (!values.repo || !question) {
    throw new Error('Pass --repo <folder> and a question.');
  }
  const controller = new AbortController();
  const abort = () => controller.abort();
  process.once('SIGINT', abort);
  const timer = setTimeout(abort, 120_000);
  const messages = new Map<string, string>();
  try {
    await codex.run({
      executable: await executable('codex'),
      cwd: await rootOf(values.repo),
      mode: 'ask',
      prompt: `Answer only. Do not edit files, commit, push, or switch branches.\nHuman question: ${question}`,
      sessionId: values.session,
      signal: controller.signal,
      emit(event) {
        if (event.kind === 'message') {
          messages.set(event.id || String(messages.size), event.text);
        } else {
          console.error(event.text);
        }
      },
      async saveSession(id) {
        console.error(`Session: ${id}`);
      },
      async approve() {
        return false;
      },
    });
    for (const text of messages.values()) {
      console.log(text);
    }
  } finally {
    clearTimeout(timer);
    process.removeListener('SIGINT', abort);
  }
}

main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
