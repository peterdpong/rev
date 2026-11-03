#!/usr/bin/env node
import { parseArgs } from 'node:util';
import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { createInterface } from 'node:readline/promises';
import path from 'node:path';
import {
  inspect,
  git,
  diffArgs,
  fetchRepo,
  importPR,
  filePatch,
  listCommits,
} from '../core/git';
import { addComment, readState, mutateState } from '../core/store';
import { AgentRunner, preparePrompt, providers } from '../core/agents';
import { createDemo } from '../core/demo';
import type {
  ReviewRequest,
  Provider,
  Selection,
  AgentRequest,
} from '../core/types';

const help = `Rev · a local review desk for humans and agents

  rev open [repo]                         Open the desktop app
  rev diff [repo]                         Read the branch diff in your terminal
  rev commits [repo]                      List commits to review
  rev ask "question"                      Ask about the review (read only)
  rev comment "change request"            Queue feedback on selected lines
  rev comments                           List queued and sent feedback
  rev remove <comment-id>                 Remove a comment
  rev apply [--dry-run]                    Send queued feedback to a local agent
  rev fetch [repo]                        Fetch remotes without changing checkout
  rev pr <number-or-url>                   Fetch PR refs; print review command
  rev doctor                             Check local agent executables
  rev demo                               Create a small sample repository

  --repo <path>    Repository (defaults to current directory)
  --base <ref>     Base (auto-detect origin/HEAD, main, master)
  --head <ref>     Head (defaults to HEAD)
  --commit <ref>   Review one commit
  --from <ref>     Oldest selected commit (inclusive; use with --to)
  --to <ref>       Newest selected commit (inclusive; use with --from)
  --offset <n>     Skip commits when listing older history
  --working       Include staged, unstaged, and untracked changes
  --file <path>   File for selection or terminal diff
  --lines <n-m>   Line range, e.g. 12-24
  --side <side>   additions (default) or deletions
  --agent <name>  codex (default) or claude
  --new-session   Start a fresh agent conversation for this review
  --no-pager      Print the diff directly

  Examples:
  rev open ~/dev/my-repo --base main
  rev diff --base main --head feature/auth
  rev diff --commit HEAD~1
  rev diff --from HEAD~3 --to HEAD~1
  rev ask "What can fail here?" --file src/auth.ts --lines 20-35
  rev comment "Handle expired tokens" --file src/auth.ts --lines 29-32
  rev apply --dry-run

Feedback stays in Git metadata. Apply edits an isolated worktree of the reviewed
branch; stale comments are rejected. No automatic commit/push.
`;

async function main() {
  const { values: flags, positionals } = parseArgs({
    allowPositionals: true,
    options: {
      commit: { type: 'string' },
      from: { type: 'string' },
      to: { type: 'string' },
      offset: { type: 'string' },
      repo: { type: 'string' },
      base: { type: 'string' },
      head: { type: 'string' },
      file: { type: 'string' },
      lines: { type: 'string' },
      side: { type: 'string' },
      agent: { type: 'string' },
      working: { type: 'boolean' },
      'dry-run': { type: 'boolean' },
      'no-pager': { type: 'boolean' },
      'new-session': { type: 'boolean' },
      help: { type: 'boolean', short: 'h' },
    },
  });
  const [action = 'help', ...args] = positionals;
  if (action === 'help' || flags.help) {
    return console.log(help);
  }
  if (flags.commit && (flags.from || flags.to)) {
    throw new Error('Use --commit or --from/--to, not both.');
  }
  if (Boolean(flags.from) !== Boolean(flags.to)) {
    throw new Error('Use --from and --to together.');
  }
  const commitRange = flags.commit
    ? { start: flags.commit, end: flags.commit }
    : flags.from
      ? { start: flags.from, end: flags.to! }
      : undefined;
  if (flags.working && commitRange) {
    throw new Error('Commit selections cannot include the working tree.');
  }
  const request: ReviewRequest = {
    repo: path.resolve(
      flags.repo ||
        (['open', 'diff', 'fetch', 'commits'].includes(action)
          ? args[0]
          : undefined) ||
        process.cwd(),
    ),
    base: flags.base,
    head: flags.head,
    mode: flags.working ? 'working' : 'branch',
    commitRange,
  };
  const packageRoot = fileURLToPath(new URL('../', import.meta.url));
  const selection = (): Selection | undefined => {
    if (!flags.file) {
      return undefined;
    }
    const [start, end = start] = (flags.lines || '1').split('-').map(Number);
    if (
      !Number.isInteger(start) ||
      !Number.isInteger(end) ||
      start < 1 ||
      end < start
    ) {
      throw new Error('Use --lines with a valid range such as 12-24.');
    }
    const side = flags.side || 'additions';
    if (side !== 'additions' && side !== 'deletions') {
      throw new Error('--side must be additions or deletions.');
    }
    return { path: flags.file, start, end, side };
  };
  if (action === 'doctor') {
    console.table(await providers());
    return;
  }
  if (action === 'demo') {
    return console.log(await createDemo(path.resolve('.rev-demo')));
  }
  if (action === 'fetch') {
    await fetchRepo(request.repo);
    return console.log('Remotes fetched. Your checkout is unchanged.');
  }
  if (action === 'pr') {
    const r = await importPR(request.repo, args[0] || '', flags.base);
    return console.log(
      `PR fetched.\nrev open ${JSON.stringify(r.repo)} --base ${r.base} --head ${r.head}`,
    );
  }
  if (action === 'commits') {
    const offset = Number(flags.offset || 0);
    const page = await listCommits(request, offset);
    for (const commit of page.commits) {
      console.log(
        `${commit.sha.slice(0, 10)}  ${commit.subject}  · ${commit.author}`,
      );
    }
    if (page.hasMore) {
      console.log(
        `More commits: repeat with --offset ${offset + page.commits.length}`,
      );
    }
    return;
  }
  if (action === 'open') {
    const binary =
      process.env.REV_DESKTOP_BINARY ||
      [
        path.join(packageRoot, 'target/release/rev'),
        path.join(packageRoot, 'target/debug/rev'),
      ].find(existsSync);
    if (!binary) {
      throw new Error('Build the desktop app with npm run build first.');
    }
    const child = spawn(
      binary,
      [
        'open',
        '--repo',
        request.repo,
        ...(flags.base ? ['--base', flags.base] : []),
        ...(flags.head ? ['--head', flags.head] : []),
        ...(flags.working ? ['--working'] : []),
        ...(commitRange
          ? ['--from', commitRange.start, '--to', commitRange.end]
          : []),
      ],
      { stdio: 'ignore', detached: true },
    );
    await new Promise<void>((resolve, reject) => {
      child.once('spawn', resolve);
      child.once('error', reject);
    });
    child.unref();
    console.log('Opening Rev…');
    return;
  }
  const review = await inspect(request);
  if (action === 'diff') {
    const color = process.stdout.isTTY && !process.env.NO_COLOR;
    let patch = flags.file
      ? (await filePatch(review, flags.file)).patch
      : await git(review.repo, [
          ...diffArgs(review, [
            color ? '--color=always' : '--color=never',
            '--patch',
          ]),
          '--',
        ]);
    if (!flags.file && review.mode === 'working') {
      for (const f of review.files.filter((f) => f.status === '?')) {
        patch += (await filePatch(review, f.path)).patch;
      }
    }
    const comparison = review.commitRange
      ? `${review.commitRange.start.slice(0, 8)} → ${review.commitRange.end.slice(0, 8)} (${review.commitCount} commit${review.commitCount === 1 ? '' : 's'}, inclusive)`
      : `${review.base} → ${review.head}`;
    const title = `${review.name} · ${comparison}${review.mode === 'working' ? ' + working tree' : ''}\n${review.files.length} changed files\n\n`;
    if (process.stdout.isTTY && !flags['no-pager']) {
      const pager = spawn('less', ['-R', '-F', '-X'], {
        stdio: ['pipe', 'inherit', 'inherit'],
      });
      pager.stdin.on('error', () => {});
      pager.stdin.end(title + patch);
      pager.on('error', () => process.stdout.write(title + patch));
    } else {
      process.stdout.write(title + (patch || 'No changes.\n'));
    }
    return;
  }
  if (action === 'comment') {
    const s = selection();
    if (!s) {
      throw new Error('Add --file and --lines to attach your feedback.');
    }
    const state = await addComment(review, s, args.join(' '));
    console.log(`Queued ${state.comments.at(-1)!.id}`);
    return;
  }
  if (action === 'comments') {
    const state = await readState(review);
    for (const c of state.comments) {
      console.log(
        `[${c.status}] ${c.id}\n  ${c.path}:${c.start}-${c.end} (${c.side})\n  ${c.body}\n`,
      );
    }
    if (!state.comments.length) {
      console.log('No review comments yet.');
    }
    return;
  }
  if (action === 'remove') {
    await mutateState(review, (s) => {
      s.comments = s.comments.filter((c) => c.id !== args[0]);
    });
    return console.log('Comment removed.');
  }
  if (action === 'ask' || action === 'apply') {
    const provider = (flags.agent || 'codex') as Provider;
    if (!['codex', 'claude'].includes(provider)) {
      throw new Error('--agent must be codex or claude.');
    }
    const agentRequest: AgentRequest = {
      review: request,
      provider,
      mode: action,
      question: args.join(' '),
      selection: selection(),
      newSession: flags['new-session'],
    };
    if (flags['dry-run']) {
      return console.log((await preparePrompt(agentRequest)).prompt);
    }
    const rendered = new Map<string, string>();
    const runner = new AgentRunner();
    const run = await runner.start(agentRequest, (e) => {
      if (e.kind !== 'message') {
        process.stderr.write(e.text + '\n');
        return;
      }
      if (!e.id) {
        process.stdout.write(e.text + '\n');
        return;
      }
      const old = rendered.get(e.id) || '';
      if (!rendered.has(e.id) && rendered.size) {
        process.stdout.write('\n');
      }
      process.stdout.write(
        e.text.startsWith(old) ? e.text.slice(old.length) : '\n' + e.text,
      );
      rendered.set(e.id, e.text);
    });
    const cancel = () => runner.cancel(run.id);
    process.on('SIGINT', cancel);
    process.on('SIGTERM', cancel);
    try {
      while (['running', 'queued'].includes(run.status)) {
        const approval = run.approvals[0];
        if (approval) {
          let allow = false;
          if (process.stdin.isTTY && process.stderr.isTTY) {
            const terminal = createInterface({
              input: process.stdin,
              output: process.stderr,
            });
            const controller = new AbortController();
            const watch = setInterval(() => {
              if (!run.approvals.some((a) => a.id === approval.id)) {
                controller.abort();
              }
            }, 100);
            terminal.on('SIGINT', () => {
              cancel();
              controller.abort();
            });
            try {
              allow = /^y(es)?$/i.test(
                (
                  await terminal.question(
                    `\n${approval.title}\n${approval.detail}\nAllow once? [y/N] `,
                    { signal: controller.signal },
                  )
                ).trim(),
              );
            } catch {
              /* cancellation denies */
            } finally {
              clearInterval(watch);
              terminal.close();
            }
          } else {
            process.stderr.write(
              `Denied ${approval.title} An interactive terminal is required for approval.\n`,
            );
          }
          if (run.approvals.some((a) => a.id === approval.id)) {
            runner.respondToApproval(run.id, approval.id, allow);
          }
        }
        await new Promise((resolve) => setTimeout(resolve, 100));
      }
    } finally {
      process.off('SIGINT', cancel);
      process.off('SIGTERM', cancel);
      runner.stopAll();
    }
    if (rendered.size) {
      process.stdout.write('\n');
    }
    process.exitCode = run.status === 'completed' ? 0 : 1;
    return;
  }
  throw new Error(`Unknown command: ${action}. Run rev --help.`);
}

main().catch((error) => {
  console.error(`Rev: ${error.message}`);
  process.exitCode = 1;
});
