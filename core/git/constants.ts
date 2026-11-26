export const DEFAULT_COMMAND_MAX_BUFFER_BYTES = 32 * 1024 * 1024;
export const DEFAULT_COMMAND_TIMEOUT_MS = 120_000;

/** Maximum bytes captured for a file diff or read from an untracked file. */
export const MAX_PATCH_BYTES = 2_000_000;

/** Bounds for the first paint; larger patches load when expanded. */
export const MAX_INLINE_PATCH_BYTES = 200_000;
export const MAX_INLINE_CHANGED_LINES = 5_000;
