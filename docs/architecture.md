# Revue architecture

## Scope of the first version

Optimize for a reviewer moving quickly through a branch, asking precise questions, and giving an agent a coherent next pass. The unit of work is a comparison plus local feedback, not a chat thread. The diff remains the primary surface.

This deliberately leaves PR discovery, hosted collaboration, GitHub review publishing, and remote execution for later. Fetching a PR is implemented; posting its comments is a separate product decision.

## Components

| Layer | Implementation | Responsibility |
| --- | --- | --- |
| Desktop | Electron main + isolated preload | Window lifecycle, repository picker, local-service lifecycle |
| UI | React, TypeScript, `@pierre/diffs` | File navigation, virtualized diff, line selections, inline selection actions, project/review tabs, agent dialogs |
| Local API | Node HTTP, random port/token | Same-origin transport for UI; no public listener |
| Git core | `execFile` with argument arrays | Resolve commits, merge base, file metadata, lazy patches, clone/fetch, PR refs |
| Workspace store | Atomic JSON in `~/.revue/workspace.json` | Project folders, stable branch/PR tab identities, active tab, last file |
| Review store | JSON in Git metadata | Anchored comments, sent/queued state, viewed patch hashes |
| Agent adapters | Child processes + JSONL readers | Discover executable, choose tool/sandbox mode, send prompt, normalize events, stop |
| CLI | Same core called directly | Terminal diff/pager, questions, comment queue, batch apply, desktop launcher |

Relevant source: `src/core/git.ts`, `src/core/store.ts`, `src/core/workspace.ts`, `src/core/agents.ts`, `src/server.ts`, `src/cli.ts`, `src/desktop/`, `src/ui/`.

## Data flow

1. Normalize the repository path and resolve both refs to commit hashes. Resolve the merge base once for a comparison.
2. Get changed-file status and line counts with NUL-delimited Git output. Working tree mode includes untracked paths.
3. Fetch the selected file's patch. The diff library parses, highlights, and virtualizes it. Other file bodies stay out of the UI payload.
4. A selection identifies a path, side, and line range. A comment also stores head SHA, merge-base SHA, and a hash of the exact patch.
5. Queue feedback locally. A filesystem lock plus atomic rename keeps concurrent feedback mutations from losing data.
6. Before dispatch, re-read Git and validate each selected comment against current code. Stale feedback remains available but cannot be applied until the reviewer replaces it.
7. Send the batch through the selected provider adapter. Stream normalized messages/activity/errors into the UI through short polling. A provider's explicit completion event plus successful exit is required to mark the comments sent.
8. Inspect the resulting working-tree changes. Committing and pushing remain normal Git operations outside this MVP.

## Navigation and agent interaction

The left sidebar groups reviews under canonical repository paths. Each review tab is keyed by repository, base, and head ref. Opening `HEAD` pins the tab to its symbolic branch (or commit when detached), so switching the checkout cannot silently repoint that tab. Committed/working-tree mode and the selected file belong to the tab. Workspace changes use a lock and atomic rename, shared by desktop and browser launches without relying on a random-port browser origin. Closing a tab only removes navigation state; feedback stays in Git metadata.

A diff selection reveals **Ask agent** and **Agent change** directly beneath the selected lines. Either opens a compact dialog with a captured file/side/range; the top-level **Ask agent** always uses the entire review. Change requests remain a local queue accessed through **Changes**, with an optional full-prompt preview before dispatch. Agent jobs and in-memory drafts are associated with their originating review, so switching tabs cannot show another review's output as the current one. Agent output opens in a dialog rather than occupying a persistent sidebar.

## Correct code context

The reviewer can inspect a branch without switching the working checkout. A question against another commit—or a committed snapshot with local dirty files—runs in a temporary detached worktree at the reviewed SHA. Cleanup removes that worktree only if it is safe; a modified worktree is retained and its location reported.

For working-tree reviews, the agent sees the actual checkout, including local edits. A pinned tab refuses working-tree mode when its head differs from the current checkout. Apply requires its HEAD to equal the reviewed head. This avoids sending feedback about one branch to an agent operating on another. It does not lock the checkout against manual Git operations or independent processes; that is a next-stage concurrency concern.

## Rendering and performance

`@pierre/diffs` owns syntax highlighting, split/unified alignment, selection, and diff virtualization. Revue loads one file patch at a time, so opening a thousand-file review does not immediately request and render all thousand file bodies. Metadata is loaded in one comparison request. A 2 MB patch limit bounds pathological files.

This is a starting design rather than a measured performance guarantee. Before scaling up: benchmark real large PRs; cache comparisons by repo/head/base plus working-tree fingerprints; watch Git/index changes; virtualize the file tree; move heavier diff work to the library's worker pool; and profile large minified/generated files. Keep invalidation tied to content identity, not just branch names.

## Agent integration choice

For this prototype, one-shot CLI jobs keep provider adapters small and reproducible. The UI can ask about exact source ranges without implementing an agent harness. The installed provider handles its own login, model inference, tools, and policies.

For the next version:

- **Codex:** replace the `exec` adapter with app-server. Initialize once, start/resume threads, start turns, translate agent-message/tool events, surface approval requests, and interrupt turns.
- **Claude:** use the Agent SDK with an explicit executable path and tool approval callbacks. Enable a controlled test-running capability after designing those approvals.
- **Pi or ACP:** add separate adapters if they meet a specific need. Do not insert another harness simply to wrap the first two providers.

Keep the UI dependent on a small interface such as `start(request)`, `events(runId)`, `cancel(runId)`, and eventually `respondToApproval(...)`. Keep provider-specific protocol details outside React.

## Moving to SSH

The useful boundary is the **host service**, encompassing Git, files, worktrees, state, and agent processes. The model's working directory and the diff's repository must describe the same host filesystem.

```text
Local UI
  └─ HostTransport
       ├─ LocalHost: current in-process/loopback service
       └─ SshHost: authenticated SSH stdio or forwarded loopback connection
            └─ small Revue host service on the remote machine
                 ├─ remote Git checkout and worktrees
                 ├─ remote review storage
                 └─ remotely installed/authenticated Codex or Claude
```

Start with an existing SSH host alias and standard SSH authentication. Launch a versioned host service over SSH and exchange framed JSON, or forward an authenticated loopback endpoint. Keep provider credentials on that host. Stream patches/events; do not copy entire repositories through the UI. Add connection state, version negotiation, reconnect/resume, cancellation, and per-host workspace locks before relying on it for long tasks.

Do not expose the current local HTTP service to a LAN/public interface. SSH is a transport extension, not a reason to weaken its authentication boundary. Remote support is not implemented in this version.

## Suggested next experiments

1. Review five real agent-generated branches. Measure navigation friction, whether questions need follow-up, and whether batched feedback produces better edits than one-off requests.
2. Test project tabs with several simultaneous PRs; add discovery/search only if finding reviews becomes the bottleneck.
3. Add persistent Codex threads if follow-up questions repeatedly need the same context.
4. Add checkpoints and an agent-edit comparison to separate the agent's latest pass from the original branch diff.
5. Add GitHub review publishing only after defining which local notes should remain private.

Local review ownership and the agent's next-pass workflow should stay clear as these capabilities grow.
