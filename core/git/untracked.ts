import { lstat, readFile, readlink, realpath } from 'node:fs/promises';
import path from 'node:path';
import { MAX_PATCH_BYTES } from './constants';

export type UntrackedFile =
  | { kind: 'text'; content: string; mode: '100644' | '100755' }
  | { kind: 'binary'; content: Buffer }
  | { kind: 'symlink'; target: string }
  | { kind: 'too-large' };

/**
 * Read an untracked file within `repo` (the canonical checkout root).
 * Reject parent folders outside it and return symlink text without following links.
 */
export async function readUntracked(
  repo: string,
  name: string,
  maxBytes = MAX_PATCH_BYTES,
): Promise<UntrackedFile> {
  const full = path.join(repo, name);
  const parent = await realpath(path.dirname(full));
  if (parent !== repo && !parent.startsWith(repo + path.sep)) {
    throw new Error('The untracked file points outside this repository.');
  }

  const file = path.join(parent, path.basename(full));
  const info = await lstat(file);
  if (info.isSymbolicLink()) {
    return { kind: 'symlink', target: await readlink(file) };
  }

  if (!info.isFile()) {
    // `git ls-files --others` lists an untracked nested repository as "dir/".
    throw new Error('The untracked path is not a regular file.');
  }

  if (info.size > maxBytes) {
    return { kind: 'too-large' };
  }

  const bytes = await readFile(file);

  // The file may have grown since lstat.
  if (bytes.length > maxBytes) {
    return { kind: 'too-large' };
  }

  if (bytes.includes(0)) {
    return { kind: 'binary', content: bytes };
  }

  return {
    kind: 'text',
    content: bytes.toString('utf8'),
    mode: info.mode & 0o111 ? '100755' : '100644',
  };
}

/**
 * Line counts for the file list, matching what `git diff --numstat` would
 * report once the file is added.
 */
export function untrackedCounts(file: UntrackedFile) {
  if (file.kind === 'binary') {
    return { added: 0, removed: 0, binary: true };
  }

  const added = splitLines(textOf(file)).length;
  return { added, removed: 0, binary: false };
}

/**
 * Build a Git-style addition patch for the viewer without staging the file.
 * Ordinary `git diff` omits untracked files, so we describe their creation
 * from scratch. This display patch omits Git's `index <old>..<new>` blob hashes.
 *
 * Example: notes.txt containing "hello\nworld\n" produces:
 *   diff --git a/notes.txt b/notes.txt
 *   new file mode 100644
 *   --- /dev/null
 *   +++ b/notes.txt
 *   @@ -0,0 +1,2 @@
 *   +hello
 *   +world
 *
 * Git adds file headers to the standard unified-diff format:
 * @see https://git-scm.com/docs/diff-format#_generating_patch_text_with_p
 * @see https://www.gnu.org/software/diffutils/manual/html_node/Detailed-Unified.html
 */
export function untrackedPatch(
  name: string,
  file: Exclude<UntrackedFile, { kind: 'too-large' }>,
) {
  // a/ and b/ label the before/after paths; they are not actual directories.
  const a = patchPath(`a/${name}`);
  const b = patchPath(`b/${name}`);
  // Git modes: 100644 = regular file, 100755 = executable, 120000 = symlink.
  // https://git-scm.com/book/en/v2/Git-Internals-Git-Objects
  const mode =
    file.kind === 'symlink'
      ? '120000'
      : file.kind === 'text'
        ? file.mode
        : '100644';
  // Identify the file, then declare its creation and file type/permissions.
  // Even for additions, `diff --git` names both paths; /dev/null appears below.
  const header = `diff --git ${a} ${b}\nnew file mode ${mode}\n`;

  if (file.kind === 'binary') {
    // A display notice instead of text hunks; this carries no binary payload.
    return `${header}Binary files /dev/null and ${b} differ\n`;
  }

  const text = textOf(file);
  const added = splitLines(text);
  if (added.length === 0) {
    // Git prints no hunk for an empty file.
    return header;
  }

  // --- names the old file (/dev/null means it did not exist); +++ names the new.
  // A hunk header is @@ -oldStart,oldCount +newStart,newCount @@:
  // -0,0 means no old lines; +1,N means N new lines starting at line 1.
  // For a single line, +1 is shorthand for +1,1. Each added line starts with +.
  let patch = `${header}--- /dev/null\n+++ ${b}\n@@ -0,0 +1${
    added.length === 1 ? '' : `,${added.length}`
  } @@\n${added.map((line) => '+' + line).join('\n')}\n`;

  if (!text.endsWith('\n')) {
    // The patch's own lines end in \n and this marker records that the source did not.
    // https://www.gnu.org/software/diffutils/manual/diffutils.html#Incomplete-Lines
    patch += '\\ No newline at end of file\n';
  }

  return patch;
}

// Git stores a symlink's target path as its content, not the target file's bytes.
const textOf = (file: Exclude<UntrackedFile, { kind: 'binary' }>) =>
  file.kind === 'symlink'
    ? file.target
    : file.kind === 'text'
      ? file.content
      : '';

// A trailing newline ends the last line; it does not add another empty line.
const splitLines = (text: string) =>
  text ? text.replace(/\n$/, '').split('\n') : [];

/**
 * Quote a filename for a patch header the way Git does, with C-style escapes
 * for quotes, backslashes, and control characters.
 * This keeps a filename's tabs/newlines from becoming patch syntax. Ordinary
 * spaces and non-ASCII characters stay literal, matching core.quotePath=false.
 * @see https://git-scm.com/docs/diff-format#_generating_patch_text_with_p
 */
export function patchPath(name: string) {
  if (!/[\x00-\x1f\x7f"\\]/.test(name)) {
    return name;
  }
  return (
    '"' +
    name.replace(/[\x00-\x1f\x7f"\\]/g, (char) =>
      char === '"' || char === '\\'
        ? '\\' + char
        : '\\' + char.charCodeAt(0).toString(8).padStart(3, '0'),
    ) +
    '"'
  );
}
