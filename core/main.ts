// Private stdio transport. Only the parent desktop process owns these pipes.
import { createInterface } from 'node:readline';
import { HostService } from './host';
import { hasPendingHistories } from './threads';

const host = new HostService();
let closing = false;
let pending = 0;
const lines = createInterface({ input: process.stdin, crlfDelay: Infinity });

const respond = (id: number | null, result?: unknown, error?: string) => {
  if (!closing) {
    process.stdout.write(JSON.stringify({ id, result, error }) + '\n');
  }
};

lines.on('line', (line) => {
  let id: number | null = null;
  try {
    if (line.length > 1_000_000) {
      throw new Error('Request too large.');
    }
    const request = JSON.parse(line);
    if (
      !Number.isSafeInteger(request.id) ||
      typeof request.method !== 'string'
    ) {
      throw new Error('Invalid request.');
    }
    id = request.id;
    pending++;
    void host
      .call(request.method, request.params)
      .then(
        (result) => respond(id, result),
        (error) => respond(id, undefined, error.message),
      )
      .finally(() => {
        pending--;
      });
  } catch (error: any) {
    respond(id, undefined, error.message);
  }
});

function shutdown() {
  if (closing) {
    return;
  }
  closing = true;
  host.close();
  // Give provider process groups time to terminate and remove clean worktrees.
  const deadline = Date.now() + 8000;
  const timer = setInterval(() => {
    if (
      (!pending &&
        !hasPendingHistories() &&
        ![...host.runner.runs.values()].some(
          (run) => run.status === 'running',
        )) ||
      Date.now() > deadline
    ) {
      clearInterval(timer);
      process.exit(0);
    }
  }, 100);
}

lines.on('close', shutdown);
process.on('SIGTERM', shutdown);
process.on('SIGINT', shutdown);
process.stdout.on('error', shutdown);
