# Independent diff renderer prototype

This experiment starts at `ui-diff-performance` (`388a385`). The default build
replaces the extracted diff component with Rev-owned parsing, row layout,
selection, and GPUI rendering. The previous renderer remains available with
`--features legacy-diff` for comparison against the same application shell.

It is a viewer experiment. It inherits that branch's desktop and Git service;
it does not include the separate agent stack or move the restacked UI branches.

## Ownership and dependency boundary

| Layer | Location | Responsibility |
| --- | --- | --- |
| Existing Git service | `core/git/` | Ask Git for two-way patches, review metadata, and deferred file contents |
| New document core | `crates/rev-diff/src/document.rs`, `parser.rs` | Parse patch bytes, keep original/modified source coordinates, copy exact source |
| New review index | `crates/rev-diff/src/index.rs` | Project unified/split rows, fold supplied context, replace/collapse files, resolve source anchors |
| New line tinting | `crates/rev-diff/src/lexer.rs` | Bounded, line-local keyword/string/comment coloring |
| New GPUI adapter | `native/review_diff/state.rs`, `view.rs` | Visible-row rendering, input, selection, scrolling, and callbacks to the desktop shell |
| GPUI | Existing `gpui-kit` dependency | Element layout, text shaping, virtualization, event delivery, and GPU drawing |

`rev-diff` has **zero dependencies**, including zero development dependencies.
Check it with `cargo tree -p rev-diff`. It can be tested and benchmarked without
starting a window or Node.

The new adapter uses GPUI's primitive `list`, `div`, `canvas`, `StyledText`, and
input APIs. It never imports the extracted component's parser, renderer,
selection, highlighting, or scroll logic. The old files under `native/diff/`
are not compiled in the default build. They and their licenses remain intact
for the comparison build.

The whole application is **not** dependency free: the existing shell still uses
GPUI Kit controls and a Node Git service. GPUI Kit also has its own transitive
dependencies. This experiment removes the diff implementation's dependence on
a third-party diff component, a similarity library, grammar packages, and
component-specific text-selection behavior. Git remains the diff algorithm;
this does not implement Myers or replace Git's comparison semantics.

## Choices for review

### Source coordinates are the contract

A selection is `(path, side, first source line, last source line)`, not a set of
screen-row offsets. Split alignment, folding, and file updates cannot silently
change which source a selection describes. The app's existing selection
callbacks use these values; future agent attachments can use the same contract.

Source bytes are shared in an `Arc<str>`, with line spans pointing into the
patch. Display expands tabs and visualizes control characters, while copying
preserves the original tabs, Unicode, CRLF, and missing final newline. Copying
across context omitted by Git returns an explicit error instead of concatenating
unrelated lines into apparent source.

### Continuous review, file-local changes

The core keeps immutable row arrays per file. A Fenwick tree indexes their
lengths so row lookup and file-length updates cost `O(log file count)`. Replacing
or collapsing one file regenerates that file's projection and returns the one
row interval to splice into GPUI's list. Other files retain their row arrays.
Layout within the changed file still costs `O(file lines)`; this is not
incremental editing within a single file.

There is an additional adapter cost: GPUI's public list API cannot seed height
hints for only inserted rows. Reapplying its uniform hint walks the full list,
retains previous heights as hints, and invalidates measurements. The core is
file-local; end-to-end file updates are not yet wholly file-local. Ordinary
wheel scrolling does not run that reseeding pass. This is a concrete reason to
consider a Rev-owned viewport/height index in the next experiment, rather than
claim the core benchmark represents complete UI update time.

Before a splice, the adapter captures the top source line and pixel offset.
Afterward it resolves that anchor in the updated index. It falls back to a local
row when the source disappeared. Expanding a file above the viewport should
therefore preserve the reader's location.

Only visible rows and GPUI's small overscan are materialized as elements. Source
formatting and tinting happen on demand for those rows; there is no whole-review
highlight pass or unbounded presentation cache. The inherited pane boundaries
keep ordinary scrolling from rebuilding unrelated sidebar content.

### Deliberately simple projection

Unified mode follows Git's patch order. Split mode pairs removed and added runs
in source order with blank cells where needed. There is no second similarity
heuristic that can introduce a different interpretation of Git's changes.
Long runs of context already supplied by Git fold to three lines at each end.
Expanding a fold reveals real supplied source; missing context is not invented
or fetched implicitly.

File collapse, viewed-file collapse, file jumps, deferred loading, wrapping, and
navigation anchors go through the existing shell's interfaces. The adapter adds
its own vertical scrollbar and horizontal code panning.

### Line-first input

Dragging the code, gutter, or add-selection button selects whole source lines.
It does not activate native text highlighting. A drag stays on the starting
file and side, including across blank split cells. In unified mode, each line
number column can start a selection on its corresponding source side. Holding
at the viewport edge schedules time-based auto-scroll until release.

Copy uses the selected source range. Shift-Up/Down extends it. Cmd-A selects
available source in the active file, including an all-deleted file. Arrow keys,
Page Up/Down, Home, and End move through the continuous review. The shell keeps
ownership of the resulting selection popover.

## Run and compare

Run these commands from this branch's worktree after installing the existing
Node dependencies:

```sh
npm run build:host
cargo run --locked --bin rev -- open /path/to/repo --base main --head feature
```

The old renderer in the same shell:

```sh
cargo run --locked --features legacy-diff --bin rev -- open /path/to/repo --base main --head feature
```

Use a temporary repository and `REV_DATA_DIR=/path/to/temporary-state` to compare
without changing saved projects. Viewed-file state is still stored under the
reviewed repository's Git directory, so use a disposable repository when testing
viewed actions. Neither command replaces an installed app.

## Validation and measurements

```sh
cargo test --locked --workspace
cargo test --locked --features legacy-diff --bin rev
cargo clippy --locked -p rev-diff --all-targets -- -D warnings
cargo bench --locked -p rev-diff --bench review
npm run build:host
npm test
```

Core tests cover exact source copying, quoted paths, sparse context, malformed
hunks, source-side selection, folding, stable anchors, and repeated file-index
updates. An integration test creates a disposable Git repository with 30 edited
Unicode/whitespace filenames plus addition, deletion, rename, and binary cases,
then checks that the real Git patch reconstructs both text sources exactly.
No new UI automation test suite is introduced.

Local optimized CPU benchmark, November 10, 2026, nine measured runs after one
warm-up, with 100,000 added source lines in each case:

| Files × lines | Build index | Collapse/expand operation | Replace one file | Row lookup |
| --- | ---: | ---: | ---: | ---: |
| 250 × 400 | 0.505 ms | 0.0009 ms | 0.0022 ms | 7.4 ns |
| 1,000 × 100 | 0.590 ms | 0.0003 ms | 0.0007 ms | 10.1 ns |
| 20 × 5,000 | 0.445 ms | 0.0123 ms | 0.0287 ms | 4.1 ns |

These are medians of per-run timings. Index construction excludes input cloning
and parsing; replacement includes cloning the replacement file. Collapse timing
averages alternating collapse and expand operations. Measurements exclude Git
I/O, GPUI layout, font shaping, GPU paint, and frame pacing. They establish a
small indexing cost, not a scrolling FPS claim or a measured speedup over the
legacy component. The benchmark source is `crates/rev-diff/benches/review.rs`.

The default desktop tests, core tests, 54 backend tests, legacy comparison tests,
TypeScript check, and native build pass. A separate prototype app started on a
203-file temporary review with over 100,000 changed lines and persisted its
workspace without an error log. Visual inspection was unavailable because
Computer Use was not approved for that window; interaction and frame-pacing
checks remain to be done manually.

## Prototype limits and next experiments

- Tinting is deliberately line-local, not language-aware syntax parsing. There
  is no multiline grammar state, intraline word highlighting, or merge-conflict
  editing. Combined merge patches return an explicit unsupported-format error.
- Source input and paths must be UTF-8. Git's binary changes appear as notices.
  Only two-way unified patches are supported; this is not a general patch applier.
- Painting caps each displayed line at roughly 32 KiB. Selection/copy still use
  the complete source. The cap and inherited file-load limits bound work but are
  visible prototype tradeoffs for pathological generated lines.
- Wrapped rows use GPUI measurement with estimates for unvisited rows. Scrollbar
  proportions can settle as those rows are measured. Horizontal width is an
  estimate, not a full-file font-shaping pass. Tabs display as four spaces.
- A mode switch rebuilds every file's row projection. Large single-file changes
  still require rebuilding that file. Per-line updates and background projection
  would be separate experiments if profiling warrants them.
- There is no minimap, sticky file header, accessibility audit, rich text
  selection, or selection spanning multiple files/sides. Full-file and PR agent
  actions belong to the separate agent stack, not this renderer prototype.

Next compare both renderers on the same real reviews, and profile the adapter's
whole-list height reseeding separately from its file-local core: warm/cold scrolling,
wrapping, long lines, file-sheet interactions, original-side gutter drags,
selection at viewport edges, and loading/collapsing a file above the viewport.
Measure frame time and memory before choosing a replacement for the main stack.
