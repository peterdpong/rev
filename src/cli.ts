#!/usr/bin/env node
import { parseArgs } from 'node:util';
import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { inspect, git, diffArgs, cloneRepo, fetchRepo, importPR, filePatch } from './core/git';
import { addComment, readState, mutateState } from './core/store';
import { AgentRunner, preparePrompt, providers } from './core/agents';
import { createDemo } from './core/demo';
import { startServer } from './server';
import type { ReviewRequest, Provider, Selection, AgentRequest } from './shared';

const help = `Revue · a local review desk for humans and agents

  revue open [repo]                         Open the desktop app
  revue serve [repo]                        Open a local browser endpoint
  revue diff [repo]                         Read the branch diff in your terminal
  revue ask "question"                      Ask about the review (read only)
  revue comment "change request"            Queue feedback on selected lines
  revue comments                           List queued and sent feedback
  revue remove <comment-id>                 Remove a comment
  revue apply [--dry-run]                    Send queued feedback to a local agent
  revue fetch [repo]                        Fetch remotes without changing checkout
  revue clone <url> <directory>              Clone a repository
  revue pr <number-or-url>                   Fetch PR refs; print review command
  revue doctor                             Check local agent executables
  revue demo                               Create a small sample repository

  --repo <path>    Repository (defaults to current directory)
  --base <ref>     Base (auto-detect origin/HEAD, main, master)
  --head <ref>     Head (defaults to HEAD)
  --working       Include staged, unstaged, and untracked changes
  --file <path>   File for selection or terminal diff
  --lines <n-m>   Line range, e.g. 12-24
  --side <side>   additions (default) or deletions
  --agent <name>  codex (default) or claude
  --no-pager      Print the diff directly
  --port <n>      Browser endpoint port (default: random free port)

  Examples:
  revue open ~/dev/my-repo --base main
  revue diff --base main --head feature/auth
  revue ask "What can fail here?" --file src/auth.ts --lines 20-35
  revue comment "Handle expired tokens" --file src/auth.ts --lines 29-32
  revue apply --dry-run

Feedback stays in Git metadata. Apply edits the current checkout only when it
matches the reviewed head; stale comments are rejected. No automatic commit/push.
`;

async function main() {
  const { values: flags, positionals } = parseArgs({ allowPositionals: true, options: { repo: { type: 'string' }, base: { type: 'string' }, head: { type: 'string' }, file: { type: 'string' }, lines: { type: 'string' }, side: { type: 'string' }, agent: { type: 'string' }, port: { type: 'string' }, working: { type: 'boolean' }, 'dry-run': { type: 'boolean' }, 'no-pager': { type: 'boolean' }, help: { type: 'boolean', short: 'h' } } });
  const [action = 'help', ...args] = positionals;
  if (action === 'help' || flags.help) return console.log(help);
  const request: ReviewRequest = { repo: path.resolve(flags.repo || (['open', 'serve', 'diff', 'fetch'].includes(action) ? args[0] : undefined) || process.cwd()), base: flags.base, head: flags.head, mode: flags.working ? 'working' : 'branch' };
  const packageRoot = fileURLToPath(new URL('../', import.meta.url));
  const selection = (): Selection | undefined => {
    if (!flags.file) return undefined;
    const [start, end = start] = (flags.lines || '1').split('-').map(Number);
    if (!Number.isInteger(start) || !Number.isInteger(end) || start < 1 || end < start) throw new Error('Use --lines with a valid range such as 12-24.');
    const side = flags.side || 'additions'; if (side !== 'additions' && side !== 'deletions') throw new Error('--side must be additions or deletions.');
    return { path: flags.file, start, end, side };
  };
  if (action === 'doctor') { console.table(await providers()); return; }
  if (action === 'demo') return console.log(await createDemo(path.resolve('.revue-demo')));
  if (action === 'clone') { if (args.length !== 2) throw new Error('Use: revue clone <url> <directory>'); return console.log(await cloneRepo(args[0], args[1])); }
  if (action === 'fetch') { await fetchRepo(request.repo); return console.log('Remotes fetched. Your checkout is unchanged.'); }
  if (action === 'pr') { const r = await importPR(request.repo, args[0] || ''); return console.log(`PR fetched.\nrevue open ${JSON.stringify(r.repo)} --base ${r.base} --head ${r.head}`); }
  if (action === 'serve') {
    const service = await startServer({ uiDir: path.join(packageRoot, 'dist/ui'), initial: request, port: flags.port ? Number(flags.port) : undefined });
    console.log(`Revue is ready:\n${service.url}\n\nPress Ctrl+C to stop.`);
    process.on('SIGINT', () => { service.close(); }); process.on('SIGTERM', () => { service.close(); }); return;
  }
  if (action === 'open') {
    const require = createRequire(import.meta.url);
    const child = spawn(process.execPath, [require.resolve('electron/cli.js'), packageRoot, '--repo', request.repo, ...(flags.base ? ['--base', flags.base] : []), ...(flags.head ? ['--head', flags.head] : []), ...(flags.working ? ['--working'] : [])], { stdio: 'ignore', detached: true });
    child.on('error', error => { console.error(error.message); process.exitCode = 1; }); child.unref(); console.log('Opening Revue…'); return;
  }
  const review = await inspect(request);
  if (action === 'diff') {
    const color = process.stdout.isTTY && !process.env.NO_COLOR;
    let patch = flags.file ? (await filePatch(review, flags.file)).patch : await git(review.repo, [...diffArgs(review), color ? '--color=always' : '--color=never', '--patch', '--']);
    if (!flags.file && review.mode === 'working') for (const f of review.files.filter(f => f.status === '?')) patch += (await filePatch(review, f.path)).patch;
    const title = `${review.name} · ${review.base} → ${review.head}${review.mode === 'working' ? ' + working tree' : ''}\n${review.files.length} changed files\n\n`;
    if (process.stdout.isTTY && !flags['no-pager']) {
      const pager = spawn('less', ['-R', '-F', '-X'], { stdio: ['pipe', 'inherit', 'inherit'] });
      pager.stdin.on('error', () => {}); pager.stdin.end(title + patch);
      pager.on('error', () => process.stdout.write(title + patch));
    } else process.stdout.write(title + (patch || 'No changes.\n'));
    return;
  }
  if (action === 'comment') {
    const s = selection(); if (!s) throw new Error('Add --file and --lines to attach your feedback.');
    const state = await addComment(review, s, args.join(' ')); console.log(`Queued ${state.comments.at(-1)!.id}`); return;
  }
  if (action === 'comments') { const state = await readState(review); for (const c of state.comments) console.log(`[${c.status}] ${c.id}\n  ${c.path}:${c.start}-${c.end} (${c.side})\n  ${c.body}\n`); if (!state.comments.length) console.log('No review comments yet.'); return; }
  if (action === 'remove') { await mutateState(review, s => { s.comments = s.comments.filter(c => c.id !== args[0]); }); return console.log('Comment removed.'); }
  if (action === 'ask' || action === 'apply') {
    const provider = (flags.agent || 'codex') as Provider;
    if (!['codex', 'claude'].includes(provider)) throw new Error('--agent must be codex or claude.');
    const agentRequest: AgentRequest = { review: request, provider, mode: action, question: args.join(' '), selection: selection() };
    if (flags['dry-run']) return console.log((await preparePrompt(agentRequest)).prompt);
    const runner = new AgentRunner(); const run = await runner.start(agentRequest, e => (e.kind === 'message' ? process.stdout : process.stderr).write(e.text + '\n'));
    process.on('SIGINT', () => runner.cancel(run.id));
    const poll = setInterval(() => { if (run.status !== 'running') { clearInterval(poll); process.exitCode = run.status === 'completed' ? 0 : 1; } }, 200);
    return;
  }
  throw new Error(`Unknown command: ${action}. Run revue --help.`);
}
main().catch(error => { console.error(`Revue: ${error.message}`); process.exitCode = 1; });
