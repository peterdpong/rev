import { createInterface } from 'node:readline';
import { HostService } from './host';
const host = new HostService();
const lines = createInterface({ input: process.stdin, crlfDelay: Infinity });
let closing = false;

function respond(id: number | null, result?: unknown, error?: string) {
  if (!closing) {
    process.stdout.write(JSON.stringify({ id, result, error }) + '\n');
  }
}

lines.on('line', (line) => {
  let id: number | null = null;
  try {
    if (line.length > 1_000_000) {
      throw new Error('Request too large.');
    }

    const request = JSON.parse(line);
    if (
      !request ||
      !Number.isSafeInteger(request.id) ||
      typeof request.method !== 'string'
    ) {
      throw new Error('Invalid request.');
    }
    
    id = request.id;
    void host.call(request.method, request.params).then(
      (result) => respond(id, result),
      (error) => respond(id, undefined, error.message),
    );
  } catch (error: any) {
    respond(id, undefined, error.message);
  }
});

function shutdown() {
  closing = true;
  host.close();
  process.exit(0);
}

lines.on('close', shutdown);
process.on('SIGTERM', shutdown);
process.on('SIGINT', shutdown);
process.stdout.on('error', shutdown);
