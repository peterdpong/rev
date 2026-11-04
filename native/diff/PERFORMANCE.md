# Diff renderer prototype

This branch builds on the extracted GPUI diff component. It replaces the row
index and preparation scheduler while retaining the parser, source coordinates,
selection, wrapping, annotations and native row rendering. It is an incremental
renderer rebuild, with the existing behavior as the compatibility baseline.

The design borrows from [Diffs' CodeView](https://diffs.com/docs#codeview),
[virtualization](https://diffs.com/docs#virtualization), and
[background workers](https://diffs.com/docs#worker-pool): one continuous viewport,
measured rows backed by height estimates, immediate plain text, and preparation
that follows the reader. Browser-specific DOM and worker code is not used.

## Changes

- `RowIndex` stores immutable, shared row segments per file. Replacing, folding
  or collapsing a file rebuilds that file's rows and source maps. Only the file
  offsets after it change; unrelated source lines and measured heights survive.
- Source and change locations are local to a file. Navigation resolves their
  global row through the file offset, including after earlier files grow or
  shrink. Lookup costs O(log files); single-file updates cost O(changed file
  lines + file count), instead of O(all review lines).
- Highlighting prepares the visible files first, with one neighboring file on
  each side. The next job is chosen from the current viewport after every
  completion. Only one CPU job runs at a time; compiled language queries are
  reused between jobs and viewport changes.
- Plain text is always renderable. Collapsed files and source-less placeholders
  are skipped. A generation counter rejects stale jobs after replacement or
  changed presentation options.
- Prepared highlighting has an approximate 8 MiB buffer budget. The active
  viewport and prefetch window are pinned; their data may exceed the budget.
  Offscreen entries farthest from the viewport are evicted first. This is not
  a limit on total app memory: parsed source, grammars and GPUI allocations are
  separate. Global layout changes, such as switching unified/split mode, still
  rebuild all rows.

## Reproduce the measurements

```sh
cargo test --locked --bin rev benchmark_layout_updates -- --ignored --nocapture
```

The benchmark is a separate commit immediately before the renderer changes, so
it can also run against the baseline. Each configuration contains 100,000 source
lines. Fixtures are parsed before timing; the first iteration is discarded and
results are medians of nine iterations. Collapse/expand averages 20 operations
per iteration; replacement averages ten. Times include synchronous state update
and preparation scheduling, but exclude Git, parsing, background highlighting,
painting and GPU work. No timing threshold is used in tests.

Measured on the same machine with Rust's unoptimized test profile:

| Files × lines/file | Create, before → after | Collapse/expand, before → after | Replace file, before → after |
| --- | --- | --- | --- |
| 250 × 400 | 15.491 → 14.528 ms | 5.919 → 0.063 ms | 19.839 → 0.148 ms |
| 1,000 × 100 | 16.154 → 14.936 ms | 6.186 → 0.049 ms | 20.310 → 0.098 ms |
| 20 × 5,000 | 15.341 → 14.431 ms | 5.858 → 0.387 ms | 19.623 → 0.996 ms |

These measurements demonstrate reduced single-file update work. They are not
release-build latency, scrolling FPS, or an end-to-end comparison with Diffs.
Creating the initial layout still visits all supplied source lines.

## Verification and remaining work

Run `cargo test --locked --bin rev` for native regression coverage. In addition
to existing selection, copying, syntax, wrapping and scroll tests, this branch
checks segmented rows against flattened rows under repeated growth/shrinkage,
retains old render snapshots, verifies later-file selection and navigation after
earlier-file changes, and exercises highlighting across jumps, collapse, cache
eviction and an in-flight review replacement.

The backend still returns all preview patches before the app installs them.
Large files retain the existing explicit load action and size limits. Progressive
patch delivery, viewport-first parsing, and release-build frame/memory profiling
are follow-up work; this prototype does not claim to solve cold-load latency.
