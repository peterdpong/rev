import { readFile, rename, unlink, writeFile } from 'node:fs/promises';
import { randomUUID } from 'node:crypto';

type JsonValue =
  string | number | boolean | null | JsonValue[] | { [key: string]: JsonValue };

const queues = new Map<string, Promise<void>>();

export async function serialize<T>(
  key: string,
  task: () => Promise<T>,
): Promise<T> {
  const run = (queues.get(key) ?? Promise.resolve()).then(task);
  // Let the next task start after either success or failure.
  const done = run.then(
    () => {},
    () => {},
  );
  queues.set(key, done);

  try {
    return await run;
  } finally {
    if (queues.get(key) === done) {
      queues.delete(key);
    }
  }
}

/**
 * Read a JSON file, or `fallback()` when it doesn't exist. A file that no
 * longer parses is moved aside to `<file>.corrupt-<time>`.
 */
export async function readJson<T>(file: string, fallback: () => T): Promise<T> {
  let text: string;
  try {
    text = await readFile(file, 'utf8');
  } catch (e: any) {
    if (e.code === 'ENOENT') {
      return fallback();
    }
    throw e;
  }

  try {
    return JSON.parse(text);
  } catch {
    await rename(file, `${file}.corrupt-${Date.now()}`).catch(() => {});
    return fallback();
  }
}

/** Replace a JSON file atomically: write a temporary file, then rename it. */
export async function writeJson(file: string, value: JsonValue) {
  const temporary = `${file}.${randomUUID()}`;

  try {
    await writeFile(temporary, JSON.stringify(value, null, 2), { mode: 0o600 });
    await rename(temporary, file);
  } finally {
    await unlink(temporary).catch(() => {});
  }
}
