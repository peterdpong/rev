import { createInterface } from 'node:readline';
import { processLifetime, spawnProvider } from './process';

export type RpcMessage = {
  id?: number | string;
  method?: string;
  params?: any;
  result?: any;
  error?: { code?: number; message?: string };
};

// Codex uses JSONL JSON-RPC without a jsonrpc header. All pending requests fail
// when the child dies; stdout noise must never become assistant output.
export class StdioRpc {
  private serial = 0;
  private pending = new Map<
    number,
    {
      resolve(value: any): void;
      reject(error: Error): void;
      timer: NodeJS.Timeout;
    }
  >();
  private child;
  private life;
  private lines;
  private failure?: Error;
  private stderr = '';
  onMessage: (message: RpcMessage) => void = () => {};
  onFailure: (error: Error) => void = () => {};

  constructor(command: string, args: string[], cwd: string) {
    this.child = spawnProvider(command, args, cwd);
    this.life = processLifetime(this.child);
    this.lines = createInterface({
      input: this.child.stdout,
      crlfDelay: Infinity,
    });
    this.child.stderr.on('data', (data) => {
      this.stderr = (this.stderr + data).slice(-8000);
    });
    this.child.on('error', (error) => this.fail(error));
    this.child.stdin.on('error', (error) => this.fail(error));
    this.child.once('close', (code) =>
      this.fail(
        new Error(`Codex app-server exited (${code}). ${this.stderr}`.trim()),
      ),
    );
    this.lines.on('line', (line) => {
      try {
        if (line.length > 8_000_000) {
          throw new Error('Codex sent an oversized protocol message.');
        }
        const message: RpcMessage = JSON.parse(line);
        if (!message || typeof message !== 'object') {
          throw new Error('Invalid Codex protocol message.');
        }
        if (message.method) {
          this.onMessage(message);
        } else if (typeof message.id === 'number') {
          const request = this.pending.get(message.id);
          if (!request) {
            return;
          }
          this.pending.delete(message.id);
          clearTimeout(request.timer);
          if (message.error) {
            request.reject(
              new Error(message.error.message || 'Codex request failed.'),
            );
          } else {
            request.resolve(message.result);
          }
        }
      } catch (error) {
        this.fail(error instanceof Error ? error : new Error(String(error)));
      }
    });
  }
  private fail(error: Error) {
    if (this.failure) {
      return;
    }
    this.failure = error;
    for (const request of this.pending.values()) {
      clearTimeout(request.timer);
      request.reject(error);
    }
    this.pending.clear();
    this.onFailure(error);
  }
  send(message: RpcMessage) {
    if (this.failure) {
      throw this.failure;
    }
    this.child.stdin.write(JSON.stringify(message) + '\n');
  }
  request(method: string, params: unknown): Promise<any> {
    if (this.failure) {
      return Promise.reject(this.failure);
    }
    const id = ++this.serial;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(
          new Error(`Codex did not respond to ${method} within 60 seconds.`),
        );
      }, 60_000);
      this.pending.set(id, { resolve, reject, timer });
      try {
        this.send({ id, method, params });
      } catch (error) {
        this.pending.delete(id);
        clearTimeout(timer);
        reject(error);
      }
    });
  }
  async close() {
    this.fail(new Error('Codex connection closed.'));
    await this.life.stop();
    this.lines.close();
  }
}
