import { readFile, rename, unlink, writeFile } from 'node:fs/promises';
import { randomUUID } from 'node:crypto';

// Only one client process writes these files, so an in-process queue is
// enough to keep read-modify-write updates from overwriting each other.
const queues = new Map<string, Promise<unknown>>();

/**
 * Run `task` after earlier tasks with the same key. A failed task doesn't
 * block later ones. Callers take their queue position synchronously, so
 * tasks run in call order.
 */
export async function serialize<T>(
  key: string,
  task: () => Promise<T>,
): Promise<T> {
  const run = (queues.get(key) ?? Promise.resolve()).then(task, task);
  queues.set(key, run);
  try {
    return await run;
  } finally {
    if (queues.get(key) === run) {
      queues.delete(key);
    }
  }
}

/**
 * Read a JSON file, or `fallback()` when it doesn't exist. A file that no
 * longer parses is moved aside to `<file>.corrupt-<time>` so the next write
 * can't silently replace what was in it.
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
export async function writeJson(file: string, value: unknown) {
  const temporary = `${file}.${randomUUID()}`;
  try {
    await writeFile(temporary, JSON.stringify(value, null, 2), { mode: 0o600 });
    await rename(temporary, file);
  } finally {
    await unlink(temporary).catch(() => {});
  }
}
