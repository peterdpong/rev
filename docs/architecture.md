# Patch GPUI architecture

## Branch strategy

`main` contains only the empty README init commit. `electron-mvp` preserves the original working application. `gpui-mvp` is derived from it and replaces the desktop and diff rendering layers while retaining the review core and its tests. Nothing is merged into main.

## Components

| Layer | Location | Responsibility |
| --- | --- | --- |
| Native launcher | `native/main.rs` | macOS window, keyboard shortcuts, desktop/CLI routing, lifecycle |
| App state | `native/app.rs` | repositories, tabs, selected file, bounded requests, popovers, drafts, agent jobs |
| Native views | `native/app/view.rs` | project folders, files, GPUI Kit Diff, inline actions/comments, anchored prompt and response popovers |
| Review context | `native/app/review.rs` | shared title bar, commit/PR summary, Files / Threads navigation |
| Conversations | `native/app/threads.rs`, `src/core/threads.ts` | persisted question/change threads, responses, follow-ups and source links |
| Rust transport | `native/backend.rs` | private child-process pipes, request IDs, response correlation, timeouts, shutdown |
| JSONL entrypoint | `src/host.ts` | parse requests, respond by ID, cancel agents on parent disconnect |
| Shared host API | `src/core/host.ts` | dispatch Git, review storage, workspace and agent operations |
| Git core | `src/core/git.ts` | merge-base diffs, lazy file patches, fetch, PR import |
| Review store | `src/core/store.ts` | comments, patch hashes, viewed status, lock + atomic rename |
| Workspace store | `src/core/workspace.ts` | project folders, Current checkout entries, pinned branch/PR tabs, selected files |
| Agent adapters | `src/core/agents.ts` | executable discovery, prompt construction, permissions, output events, cancellation |
| CLI | `src/cli.ts` | terminal diff, questions, queued feedback, apply and native launcher |

The legacy authenticated HTTP adapter remains only for regression coverage and potential transport reuse. The native app does not start it or load HTML.

## Why retain the Node core?

This MVP tests the GPUI interaction model without simultaneously rewriting Git, storage, and agent integration. One native drawing process plus one Node service avoids the Chromium runtime while preserving reviewed behavior. The `.app` bundles Node so it launches without a shell environment. It is not the smallest possible all-Rust architecture and no Electron-vs-GPUI memory claim has been benchmarked.

The JSON request boundary allows the host to be replaced with Rust later. The Rust UI never imports provider-specific protocol details. The CLI calls the same core directly.

## Review and feedback flow

1. Inspect a repository, resolve base/head commits, and compute the merge base. The working checkout is not switched.
2. Persist a project and tab. Each project has a stable Current entry whose navigation request follows `HEAD`; additional branch/PR tabs persist an explicit ref. Review requests used for comments and agents always pin the inspected branch or detached commit.
3. Load the comparison's patches through one `patches` request, validating the inspected endpoints and limiting concurrent Git reads to four. Parse them off the UI thread into one retained `DiffState` with virtualized rows across all files. Each file retains its own digest for feedback and Viewed actions. Binary, oversized, and unavailable files keep a header and an inline notice. Sidebar links reveal and scroll to a file; filtering never filters the diff itself.
4. Selection actions capture the review, path, side, line range, and patch digest before opening the popover. Cross-side ranges are rejected rather than converted to inaccurate source coordinates.
5. A change request is saved in Git metadata. Inline comments are only attached when their digest matches the currently displayed patch; older feedback remains in Changes and is rejected on apply.
6. A selected feedback batch is revalidated immediately before dispatch. The local agent receives the requests and code context. Comments become sent only after explicit successful provider completion.
7. Review agent edits in Working tree mode. Committing and pushing remain separate user actions.

The top-level Ask button explicitly supplies no line selection. Questions about selected code also validate its captured digest. General questions include bounded patch context and tell the agent when context was truncated.

## Native state and responsiveness

Git, process I/O, and request waits run on GPUI's background executor. Requests are tagged and can complete independently. File generations prevent stale asynchronous responses from overwriting a newer selection. Review navigation is serialized while a review is loading; the previously loaded review is retained if the new comparison fails.

Diff parsing is bounded by the 2 MB file limit. The diff entity survives ordinary renders. Project/file navigation uses standard scrolling; the expensive code surface is virtualized. This should be profiled with real large reviews before any performance guarantee.

Popover context is captured separately from current navigation. GPUI Kit’s popup host anchors each surface to its trigger and keeps it inside the window. There is no modal backdrop or focus trap. Outside clicks and Escape dismiss the surface; selected/general prompts have different draft keys. Runs are associated with their originating review tab; closing the response popover does not cancel a run; Stop does. Closing the application closes its private pipes and terminates active provider process groups.

## Local agents and code context

Codex uses `exec --json` with read-only or workspace-write sandbox mode. Claude uses print/stream-json with a restricted tool list and no configured MCP servers for these runs. The installed tools keep their own authentication and configuration. An executable path override is available for each provider.

Questions about a committed snapshot run in a temporary detached worktree when the current checkout is dirty or points elsewhere. Cleanup only removes a clean temporary checkout. Apply requires the reviewed HEAD to match the current checkout, rejects dirty committed-mode reviews, and rejects stale comment anchors. Working-tree mode includes staged, unstaged, and untracked files.

Live model calls, login flows, and authenticated PR fetching require a signed-in installation and are not covered by fixture-based verification. The test suite does exercise the real child-process transport with fake agents, failure/completion handling, cancellation, worktree selection, and persistent state.

## Persistence

To keep the two branches comparable, review files retain the existing `<git-dir>/revue/` format. Workspace tabs default to `~/.revue/workspace.json`. `PATCH_DATA_DIR` (or the older `REVUE_DATA_DIR`) can isolate workspace navigation. Closing a tab removes navigation only, not its feedback. Both stores use filesystem locks and atomic replacement.

Opening a folder creates or reuses its Current entry. `init` upgrades existing projects without replacing pinned tabs; the `workspace` RPC refreshes checkout metadata and writes only when it changes. The desktop polls every two seconds, updates branch labels, and reloads an active Current comparison when its branch or tip changes. A checkout change clears its old commit range and selected file. Open prompts and reply drafts defer automatic navigation, retaining their original source context. The project's branch picker calls `branches` and `fetch` for that folder independently of the active review.

## Future SSH host

The host service owns Git, files, worktrees, state, and agent subprocesses. An SSH adapter can launch that same versioned service remotely and exchange JSONL through SSH stdio. Repository paths and agent working directories must refer to the same host. Add host IDs, protocol negotiation, reconnect behavior, and remote cancellation before enabling remote sessions. Provider credentials should remain on the remote host.

## Dependencies and packaging

GPUI Kit is pinned to 0.8.0 and Cargo.lock pins its matching GPUI packages. The optional `ui-tests` feature enables the upstream offscreen test facilities; it is excluded from release builds. The macOS bundle uses the release Rust binary, two esbuild outputs, and a standalone Node executable (automatically downloaded with checksum verification when the local Node depends on external libraries). The bundle is locally signed, not notarized. Liquid Glass and an all-Rust host are separate future changes.

## Commit scope

`ReviewRequest.commitRange` holds inclusive oldest/newest commit IDs. Inspection validates both against the branch’s first-parent history and diffs the oldest commit’s first parent (or the empty tree for a root) against the newest commit. The symbolic branch remains the navigation identity; normalized commit IDs pin the comparison. `scopeKey` isolates feedback, viewed markers, drafts, and agent runs while `key` keeps one branch tab. The workspace persists the range on that tab.

The `commits` RPC pages first-parent history in batches of 200. The native picker pins its history to the loaded branch head and renders visible rows with GPUI’s virtual list. Selecting a range does not check out, cherry-pick, or write Git objects. CLI `--commit`, `--from`, and `--to` use the same inspection and patch pipeline.

## Review context and conversations

Inspection reads the selected endpoint's commit subject, body, author and date. Importing a PR caches its GitHub title, description, author, state and branch names next to the review store; Fetch updates that metadata and the private PR refs. Ordinary review inspection stays local and works offline.

The desktop shares a 44px draggable title bar across project navigation and the review area, with a reserved inset for native macOS traffic lights. A compact summary and Files / Threads controls stay visible above either the diff or the discussion list. Full descriptions open in an anchored popover. The threads view occupies the main area; it does not add a right sidebar.

Each agent run journals its question or requested changes, original revision and line anchor, provider, response events and completion status into the existing review state. Updates use the store's lock and atomic replacement. Polling flushes pending events; shutdown waits for final history writes. History writes skip expensive viewed-file revalidation. Queued comments appear alongside saved runs and are represented by their conversation once sent.

Follow-up questions carry the thread ID and bounded previous turns. The host rejects follow-ups to another revision or a change-run thread. File/line links retain the original patch digest; changed anchors open the file without scrolling to stale line numbers. Historical messages remain readable. CLI runs use the same journal, so their conversations appear in the desktop after refreshing the review.
