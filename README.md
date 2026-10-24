# Revue

A small local desktop app and CLI for reviewing code written by humans and agents.

**Review → understand → collect feedback → ask your local agent for another pass.**

## Try it

Requires Node.js 22+, Git, and an installed/authenticated Codex or Claude Code CLI for agent actions. GitHub PR import additionally requires `gh auth login`. Browsing diffs and queuing feedback work without an agent.

```sh
cd /Users/peter/dev/revue
npm install
npm run build
npm start
```

Dependencies are already installed and the app is built in this workspace. Open a local repository, clone an HTTPS/SSH Git URL, or choose **Try sample**. The sample is a real Git repository containing an intentionally imperfect retry implementation.

Launch directly into the supplied sample:

```sh
npm run demo
npm start -- --repo /Users/peter/dev/revue/.revue-demo --base main
```

An unsigned development app runs through Electron; there is no installer or signed release yet. `npm run dev` rebuilds and launches (it does not hot reload).

## What works

- Open and clone repositories; keep branch and PR tabs grouped under project folders in the left sidebar.
- Restore open tabs and each review’s last selected file after restarting. Add a review with the project’s **+** button.
- Fetch remotes without merging into or switching your checkout.
- Fetch a GitHub PR by URL or number into private `refs/revue/…` refs.
- Compare a branch against its **merge base**, the normal PR comparison semantics.
- Include staged, unstaged, and untracked changes with **Working tree**.
- Split/unified syntax-highlighted diffs using `@pierre/diffs`, virtualized rendering, and lazy per-file patch loading. Renames, deletion, and binary metadata are handled.
- Select lines by clicking/dragging line numbers (Shift-click extends a range).
- Select code to reveal **Ask agent** and **Agent change** inline. The top-level **Ask agent** opens a question about the entire review. Both providers reuse the CLI’s existing sign-in.
- Queue private inline feedback with old/new side and line range. Open **Changes** to choose requests, optionally preview the prompt, and send one batch to an agent. Prompts and responses open in dialogs; there is no permanent agent panel.
- Stop an agent, see execution activity/errors, and inspect its local edits through Working tree.
- Mark files viewed; the mark expires when the patch changes. Search files and use `j`/`k`, `v`, `a`, `c`, `/`, and Cmd/Ctrl+Enter.
- Persist feedback and viewed hashes in `<git-dir>/revue/`. Desktop and CLI share this state. Project/tab navigation is stored in `~/.revue/workspace.json` (override its directory with `REVUE_DATA_DIR`).

## A good first test

1. Open the sample; select `src/lib/retry.ts` from the file list (or the file selector in a narrow window).
2. Select the added `signal` option, then choose **Agent change** beside the selection.
3. Click **Queue change** after writing: “Make the backoff wait abortable, and avoid sleeping after the final failed attempt. Add tests for both cases.”
4. Open **Changes** in the toolbar, optionally **Preview prompt**, then **Send to agent**.
5. Click **Review local changes** to inspect the result. A sent comment is not automatically considered resolved.

The app does not commit, push, or post GitHub review comments. For other people's PRs, questions can run in an isolated temporary checkout of the reviewed commit. To apply feedback, your current checkout must match the reviewed head. A committed-only review refuses to edit a dirty checkout; switch to Working tree and queue feedback on the current code instead.

## CLI

No global installation is necessary:

```sh
node /Users/peter/dev/revue/bin/revue.mjs --help
```

During development, use `npm run cli -- <command>`. Optionally run `npm link` yourself to install the `revue` command on PATH.

```sh
revue open ~/dev/project --base main
revue diff --repo ~/dev/project --base main --head feature/auth
revue diff --working --no-pager
revue ask "What can fail here?" --file src/auth.ts --lines 20-35
revue comment "Handle expired tokens" --file src/auth.ts --lines 29-32
revue comments
revue apply --dry-run
revue apply --agent codex
revue remove <comment-id>
revue fetch
revue clone https://github.com/owner/repo.git ~/dev/repo
revue pr 42
revue doctor
```

`diff` uses a colored terminal pager on a TTY and plain output when piped. This is a minimal CLI, not a full-screen TUI with interactive comments. `--side deletions` anchors a comment to old lines. `--base`, `--head`, and `--working` must describe the same comparison when sharing comments between commands and the desktop UI.

For the same interface in a browser:

```sh
npm run preview -- /path/to/repo --base main
```

Open the URL printed by the command. The fragment contains a per-process capability token. The service binds only to `127.0.0.1` and rejects other origins/hosts. Restarting it creates a new token.

## How desktop apps use installed agents

They usually launch a provider's local executable and communicate over structured stdin/stdout. The executable owns authentication, model access, tools, sandboxing, and the agent loop. The desktop app owns the UI, review context, workspaces, and lifecycle. A local agent process normally still calls cloud-hosted models; “local” does not imply offline inference.

Revue's first adapters are intentionally small:

| Provider | Transport in this prototype | Questions | Change requests |
| --- | --- | --- | --- |
| Codex | `codex exec --json`, prompt on stdin | `read-only` sandbox | `workspace-write` sandbox |
| Claude Code | `claude -p --output-format stream-json --verbose`, prompt on stdin | Read/Glob/Grep tools | Read/Glob/Grep/Edit/Write tools |

Codex's approval policy is `never`: work requiring escalation fails instead of silently obtaining more access. Claude uses `dontAsk` with an explicit tool list and no MCP servers. Claude's adapter currently has no shell tool, so it can edit tests but cannot run them. These are provider policies, not a new OS sandbox implemented by Revue. Use repositories you trust; provider configuration and instruction files can affect agent behavior. Revue does not read or copy authentication tokens.

CLI discovery checks PATH, common macOS installation locations, and `~/.local/bin`. Override with `REVUE_CODEX_PATH` or `REVUE_CLAUDE_PATH` if needed. A green availability dot means the executable was found; it does not prove authentication or model access.

For a richer app, use **Codex app-server** (JSON-RPC over stdio) and the **Claude Agent SDK**. Those support persistent conversations, richer tool events, and approval interactions. Revue uses one fresh process per question/batch to keep the prototype small. The provider boundary is in `src/core/agents.ts`.

**bb is not universally powered by Pi.** Its current source has separate providers: [Codex app-server transport](https://github.com/get-bb/bb/blob/main/plugins/provider-codex/src/bridge/app-server-connection.ts), [Claude Agent SDK integration](https://github.com/get-bb/bb/blob/main/plugins/provider-claude-code/src/bridge/session-options.ts), and an optional [Pi provider](https://github.com/get-bb/bb/blob/main/plugins/provider-pi/package.json). You do not need to add Pi just to talk to an installed Codex or Claude Code CLI.

## Architecture

See [the architecture notes](docs/architecture.md) for boundaries, data flow, and the SSH evolution path.

```text
Electron window ─┐
                ├── React + @pierre/diffs ── loopback API ─┐
Browser preview ┘                                         │
CLI ───────────────────────────────────────────── shared TypeScript core
                                                         ├── Git / gh subprocesses
                                                         ├── Review JSON in Git metadata
                                                         └── Codex / Claude subprocesses
```

There is no backend account, hosted database, provider key store, or custom agent harness. Electron keeps the desktop, CLI, and agent adapters in one language/runtime. Tauri would reduce distribution size, but introduces a Rust boundary or a bundled Node sidecar before we know whether the review workflow is right.

## Current limits

- No SSH hosts, persistent chat threads, GitHub comment publishing, PR inbox, CI/checks display, or hosted review inbox.
- No signed/packaged installer; validated on this macOS environment. Windows process handling and paths need explicit work before claiming support.
- Patches above 2 MB show a fallback. Whole-review agent prompts include at most 40 file patches/120k characters and disclose omissions. Selected-file prompts also have bounded context.
- Large repository scale has not been benchmarked. The file list is not virtualized; the diff is. The prototype validates the workflow, not production-scale throughput.
- Agent output is kept in memory (last 30 runs, bounded events), not persisted across app restarts. Review comments survive restarts.
- Stop sends termination to the provider process group. Files already edited are retained for review; there is no automatic rollback.
- One run per repository is enforced within an app process, not across independent CLI/app processes. Avoid concurrent editing runs against the same checkout.
- GitHub PR import uses `gh` for metadata and Git for transport. Public or private access must work in both tools; set up `gh auth setup-git` or your normal Git credentials if needed.

## Validation

```sh
npm test
npm run check
npm run build
```

The integration suite exercises actual temporary Git repositories, merge-base semantics, renames/binaries/deletions, literal pathspecs, working-tree and untracked content, feedback persistence, concurrent writes, stale-anchor protection, viewed invalidation, line validation, provider flags/events, real child-process transport with controlled fixtures, and loopback API access checks.

The sample UI was manually exercised through project/branch tabs, selected-file restoration, line selection, contextual and whole-review prompts, comment creation, batch preview, viewed tracking, and CLI reading the same feedback. The integration suite also verifies project grouping, branch pinning, PR tab identities, concurrent navigation writes, and persistence across service restarts. Electron was launched successfully; macOS computer-control permission was unavailable for native UI automation, so interaction verification used the shared browser UI. No live provider inference or authenticated GitHub fetch has been verified yet; executable detection and provider CLI flags were checked locally.

References: [Hunk](https://www.hunk.dev/), [Pierre Diffs](https://diffs.com/), [Codex non-interactive mode](https://developers.openai.com/codex/noninteractive/), [Codex app-server](https://developers.openai.com/codex/app-server/), [Claude Code programmatic usage](https://code.claude.com/docs/en/headless).
