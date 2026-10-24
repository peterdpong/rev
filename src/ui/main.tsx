import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import type { DiffLineAnnotation } from '@pierre/diffs';
import { PatchDiff, Virtualizer, type SelectedLineRange } from '@pierre/diffs/react';
import { ArrowDownToLine, ArrowRight, Check, ChevronDown, ChevronRight, Columns2, FileCode2, Folder, GitBranch, GitPullRequest, Layers2, Loader2, MessageSquare, PanelLeft, Plus, RefreshCw, Search, Send, Sparkles, Square, Trash2, X } from 'lucide-react';
import type { AgentRequest, AgentRun, Comment, FilePatch, Provider, Review, ReviewRequest, ReviewState, ReviewTab, Selection, Workspace, WorkspaceAction } from '../shared';
import './styles.css';

declare global { interface Window { revueDesktop?: { chooseRepo(): Promise<string | null> } } }
const token = location.hash.slice(1) || sessionStorage.getItem('revue-token') || '';
if (location.hash) { sessionStorage.setItem('revue-token', token); history.replaceState(null, '', location.pathname); }
async function api<T>(method: string, params?: unknown): Promise<T> {
  const response = await fetch('/api', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Revue-Token': token }, body: JSON.stringify({ method, params }) });
  const data = await response.json();
  if (!response.ok) throw new Error(data.error || 'Could not connect to Revue.');
  return data;
}
const emptyState: ReviewState = { comments: [], viewed: {} };
const shortRef = (ref: string) => ref.replace(/^refs\/(heads|remotes)\//, '').replace(/^refs\/revue\/[^/]+\/[^/]+\/(\d+)\/(head|base)$/, 'PR #$1 / $2');
const filename = (p: string) => p.split('/').at(-1);
const locationLabel = (s: Selection) => `${s.path}:${s.start}${s.end !== s.start ? `–${s.end}` : ''}${s.side === 'deletions' ? ' (old)' : ''}`;
type LoadedReview = { review: Review; state: ReviewState; workspace?: Workspace };
type Modal = { type: 'project' | 'review' | 'queue' | 'result' } | { type: 'ask' | 'change'; selection?: Selection; digest?: string; draftKey: string };
type Annotation = { type: 'comment'; comment: Comment } | { type: 'selection' };
type Job = { run: AgentRun; request: ReviewRequest; question: string; provider: Provider };

function Dialog({ title, close, children, wide = false }: { title: string; close: () => void; children: React.ReactNode; wide?: boolean }) {
  const element = useRef<HTMLElement>(null);
  const closeRef = useRef(close); closeRef.current = close;
  useEffect(() => {
    const previous = document.activeElement as HTMLElement;
    const dialog = element.current!;
    const focusable = () => [...dialog.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), textarea:not(:disabled), select:not(:disabled), summary, [tabindex="0"]')].filter(el => el.getClientRects().length);
    (dialog.querySelector<HTMLElement>('[data-autofocus]') || focusable()[0])?.focus();
    const keydown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') { e.preventDefault(); closeRef.current(); }
      if (e.key === 'Tab') {
        const items = focusable(); const first = items[0]; const last = items.at(-1);
        if (e.shiftKey && (document.activeElement === first || !dialog.contains(document.activeElement))) { e.preventDefault(); last?.focus(); }
        else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first?.focus(); }
      }
    };
    document.addEventListener('keydown', keydown);
    return () => { document.removeEventListener('keydown', keydown); if (previous?.isConnected) previous.focus(); };
  }, []);
  return <div className="modal-backdrop" onMouseDown={e => { if (e.target === e.currentTarget) close(); }}><section ref={element} className={`dialog ${wide ? 'wide-dialog' : ''}`} role="dialog" aria-modal="true" aria-label={title}><header className="dialog-header"><h2>{title}</h2><button className="icon-button" aria-label="Close dialog" onClick={close}><X size={18}/></button></header>{children}</section></div>;
}

function App() {
  const [workspace, setWorkspace] = useState<Workspace>({ projects: [] });
  const [review, setReview] = useState<Review>();
  const [request, setRequest] = useState<ReviewRequest>();
  const [state, setState] = useState<ReviewState>(emptyState);
  const [active, setActive] = useState('');
  const [patch, setPatch] = useState<FilePatch>();
  const [selection, setSelection] = useState<Selection>();
  const [layout, setLayout] = useState<'split' | 'unified'>(() => innerWidth < 1150 ? 'unified' : 'split');
  const [showFiles, setShowFiles] = useState(true);
  const [search, setSearch] = useState('');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [provider, setProvider] = useState<Provider>('codex');
  const [availability, setAvailability] = useState<{ id: Provider; available: boolean }[]>([]);
  const [jobs, setJobs] = useState<Record<string, Job>>({});
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [included, setIncluded] = useState<Record<string, boolean>>({});
  const [modal, setModal] = useState<Modal | null>(null);
  const [projectMode, setProjectMode] = useState<'local' | 'clone'>('local');
  const [repoInput, setRepoInput] = useState('');
  const [urlInput, setUrlInput] = useState('');
  const [reviewMode, setReviewMode] = useState<'branch' | 'pr'>('branch');
  const [target, setTarget] = useState<Review>();
  const [baseInput, setBaseInput] = useState('');
  const [headInput, setHeadInput] = useState('');
  const [prInput, setPrInput] = useState('');
  const [prompt, setPrompt] = useState('');
  const [refresh, setRefresh] = useState(0);
  const loadSequence = useRef(0);
  const currentKey = useRef<string | undefined>(undefined); currentKey.current = review?.key;
  const currentRequest = useRef(request); currentRequest.current = request;
  const searchBox = useRef<HTMLInputElement>(null);
  const queued = state.comments.filter(c => c.status === 'queued');
  const batch = queued.filter(c => included[c.id] !== false);
  const job = review ? jobs[review.key] : undefined;
  const running = job?.run.status === 'running';
  const filtered = useMemo(() => review?.files.filter(f => f.path.toLowerCase().includes(search.toLowerCase())) || [], [review, search]);
  const activeFile = review?.files.find(f => f.path === active);
  const checked = patch && state.viewed[active] === patch.digest;
  const agentAvailable = availability.some(p => p.id === provider && p.available);
  const draft = modal && 'draftKey' in modal ? drafts[modal.draftKey] || '' : '';
  const tab = workspace.projects.flatMap(p => p.tabs).find(t => t.id === review?.key);

  const load = useCallback(async (input: ReviewRequest, file?: string) => {
    const seq = ++loadSequence.current; setBusy(true); setError('');
    try {
      const result = await api<LoadedReview>('review', { ...input, remember: true });
      if (seq !== loadSequence.current) return;
      const saved = result.workspace!.projects.flatMap(p => p.tabs).find(t => t.id === result.review.key)!;
      setModal(null); setWorkspace(result.workspace!); setReview(result.review); setRequest(saved.request); setState(result.state);
      setPatch(undefined); setSelection(undefined); setSearch(''); setRefresh(n => n + 1);
      const preferred = file || saved.file;
      setActive(result.review.files.some(f => f.path === preferred) ? preferred! : result.review.files[0]?.path || '');
    } catch (e: any) { if (seq === loadSequence.current) setError(e.message); }
    finally { if (seq === loadSequence.current) setBusy(false); }
  }, []);

  useEffect(() => {
    api<{ initial?: ReviewRequest; providers: typeof availability; workspace: Workspace }>('init').then(data => {
      setAvailability(data.providers); setWorkspace(data.workspace);
      if (!data.providers.some(p => p.id === 'codex' && p.available) && data.providers.some(p => p.id === 'claude' && p.available)) setProvider('claude');
      const saved = data.workspace.projects.flatMap(p => p.tabs).find(t => t.id === data.workspace.activeTabId);
      if (data.initial) void load(data.initial);
      else if (saved) void load(saved.request, saved.file);
    }).catch(e => setError(e.message));
  }, [load]);

  useEffect(() => {
    setPatch(undefined); setSelection(undefined);
    if (!request || !active) return;
    let obsolete = false;
    api<FilePatch>('patch', { review: request, path: active }).then(p => { if (!obsolete) setPatch(p); }).catch(e => { if (!obsolete) setError(e.message); });
    return () => { obsolete = true; };
  }, [request, active, refresh]);

  const runningIds = Object.entries(jobs).filter(([, j]) => j.run.status === 'running').map(([key, j]) => `${key}:${j.run.id}`).join(',');
  useEffect(() => {
    if (!runningIds) return;
    let stopped = false; let polling = false;
    const poll = async () => {
      if (polling) return; polling = true;
      try {
        await Promise.all(Object.entries(jobs).filter(([, j]) => j.run.status === 'running').map(async ([key, j]) => {
          const next = await api<AgentRun>('runStatus', { id: j.run.id });
          if (stopped) return;
          if (next.status !== 'running' && currentKey.current === key) {
            try {
              const result = await api<LoadedReview>('review', currentRequest.current);
              if (!stopped && currentKey.current === key) { setState(result.state); setReview(result.review); setRefresh(n => n + 1); }
            } catch (e: any) { if (!stopped) setError(e.message); }
          }
          if (!stopped) setJobs(prev => ({ ...prev, [key]: { ...j, run: next } }));
        }));
      } catch (e: any) { if (!stopped) setError(e.message); }
      finally { polling = false; }
    };
    const timer = setInterval(poll, 700);
    return () => { stopped = true; clearInterval(timer); };
  }, [runningIds]);

  const perform = async (fn: () => Promise<void>) => {
    setBusy(true); setError('');
    try { await fn(); } catch (e: any) { setError(e.message); } finally { setBusy(false); }
  };
  const workspaceAction = async (action: WorkspaceAction) => {
    const next = await api<Workspace>('workspaceAction', action); setWorkspace(next); return next;
  };
  const selectFile = (file: string) => {
    setActive(file);
    if (review) {
      setWorkspace(prev => ({ ...prev, projects: prev.projects.map(p => ({ ...p, tabs: p.tabs.map(t => t.id === review.key ? { ...t, file } : t) })) }));
      void api('workspaceAction', { type: 'setFile', id: review.key, file }).catch(e => setError(e.message));
    }
  };
  const closeTab = (item: ReviewTab) => perform(async () => {
    const next = await workspaceAction({ type: 'closeTab', id: item.id });
    if (item.id !== review?.key) return;
    const replacement = next.projects.flatMap(p => p.tabs).find(t => t.id === next.activeTabId);
    if (replacement) await load(replacement.request, replacement.file);
    else { ++loadSequence.current; setReview(undefined); setRequest(undefined); setState(emptyState); setActive(''); }
  });
  const openReview = (repo: string) => perform(async () => {
    const result = await api<LoadedReview>('review', { repo });
    setTarget(result.review); setBaseInput(result.review.base); setHeadInput(result.review.branch === 'detached HEAD' ? result.review.headSha : result.review.branch);
    setReviewMode('branch'); setPrInput(''); setModal({ type: 'review' });
  });
  const openAgent = (type: 'ask' | 'change', context?: Selection) => {
    if (!review || (type === 'change' && !context)) return;
    setError('');
    setModal({ type, selection: context ? { ...context } : undefined, digest: patch?.digest, draftKey: `${review.key}:${type}:${context ? locationLabel(context) : 'all'}` });
  };
  const toggleViewed = () => request && patch && perform(async () => {
    const key = review!.key;
    const next = await api<ReviewState>('viewed', { review: request, path: active, digest: patch.digest, viewed: !checked });
    if (currentKey.current === key) setState(next);
  });
  const navigate = (offset: number) => { const next = filtered[filtered.findIndex(f => f.path === active) + offset]; if (next) selectFile(next.path); };
  const agentRequest = (mode: 'ask' | 'apply'): AgentRequest => ({ review: request!, provider, mode, question: draft, selection: modal && 'selection' in modal ? modal.selection : undefined, commentIds: batch.map(c => c.id) });
  const startAgent = (mode: 'ask' | 'apply') => perform(async () => {
    const key = review!.key; const origin = request!;
    const next = await api<AgentRun>('run', agentRequest(mode));
    setJobs(prev => ({ ...prev, [key]: { run: next, request: origin, provider, question: mode === 'ask' ? draft : `Apply ${batch.length} change request${batch.length === 1 ? '' : 's'}` } }));
    if (modal && 'draftKey' in modal) setDrafts(prev => ({ ...prev, [modal.draftKey]: '' }));
    setModal({ type: 'result' });
  });
  const saveComment = () => perform(async () => {
    if (!modal || modal.type !== 'change') return;
    setState(await api('comment', { review: request, selection: modal.selection, body: draft, digest: modal.digest }));
    setDrafts(prev => ({ ...prev, [modal.draftKey]: '' })); setSelection(undefined); setModal(null);
  });
  const previewPrompt = () => perform(async () => { const result = await api<{ prompt: string }>('prompt', agentRequest('apply')); setPrompt(result.prompt); });
  const chooseRepo = () => perform(async () => { const repo = await window.revueDesktop?.chooseRepo(); if (repo) await load({ repo }); });
  const openProject = () => { setError(''); setProjectMode('local'); setRepoInput(''); setModal({ type: 'project' }); };

  useEffect(() => {
    const keydown = (e: KeyboardEvent) => {
      if (modal || (e.target as HTMLElement).closest('input, textarea, select, [contenteditable]') || busy || e.metaKey || e.ctrlKey || e.altKey) return;
      if (e.key === 'Escape') setSelection(undefined);
      if (e.key === 'j') { e.preventDefault(); navigate(1); }
      if (e.key === 'k') { e.preventDefault(); navigate(-1); }
      if (e.key === 'v') { e.preventDefault(); void toggleViewed(); }
      if (e.key === '/') { e.preventDefault(); setShowFiles(true); searchBox.current?.focus(); }
      if (e.key === 'a') { e.preventDefault(); openAgent('ask'); }
      if (e.key === 'c' && selection) { e.preventDefault(); openAgent('change', selection); }
    };
    window.addEventListener('keydown', keydown); return () => window.removeEventListener('keydown', keydown);
  });

  const selectLines = useCallback((range: SelectedLineRange | null) => {
    if (!range) { setSelection(undefined); return; }
    if (range.endSide && range.side && range.endSide !== range.side) return;
    setSelection({ path: active, start: Math.min(range.start, range.end), end: Math.max(range.start, range.end), side: range.side || 'additions' });
  }, [active]);
  const diffOptions = useMemo(() => ({ theme: 'pierre-dark' as const, themeType: 'dark' as const, diffStyle: layout, diffIndicators: 'bars' as const, disableFileHeader: true, enableLineSelection: true, onLineSelected: selectLines, overflow: 'scroll' as const, unsafeCSS: ':host { --diffs-font-size: 12px; --diffs-line-height: 25px; --diffs-font-family: "SFMono-Regular", Consolas, "Liberation Mono", monospace; --diffs-dark-bg: #111315; }' }), [layout, selectLines]);
  const annotations = useMemo(() => {
    const items: DiffLineAnnotation<Annotation>[] = state.comments.filter(c => c.path === active && c.digest === patch?.digest).map(c => ({ side: c.side, lineNumber: c.end, metadata: { type: 'comment', comment: c } }));
    if (selection?.path === active) items.push({ side: selection.side, lineNumber: selection.end, metadata: { type: 'selection' } });
    return items;
  }, [state.comments, active, patch, selection]);
  const providerSelect = <select aria-label="Local agent" value={provider} disabled={busy || running} onChange={e => setProvider(e.target.value as Provider)}><option value="codex">Codex{availability.find(p => p.id === 'codex')?.available ? '' : ' · not installed'}</option><option value="claude">Claude Code{availability.find(p => p.id === 'claude')?.available ? '' : ' · not installed'}</option></select>;
  const added = review?.files.reduce((sum, f) => sum + f.added, 0) || 0;
  const removed = review?.files.reduce((sum, f) => sum + f.removed, 0) || 0;

  return <div className={`app ${window.revueDesktop ? 'native' : ''}`}>
    <aside className="projects-sidebar" aria-label="Projects">
      <header className="brand-row"><span className="brand"><Layers2 size={18}/>revue</span><button className="icon-button" title="Open project" aria-label="Open project" onClick={openProject} disabled={busy}><Plus size={17}/></button></header>
      <nav className="projects-list" aria-label="Review tabs">
        {workspace.projects.map(project => <div className="project" key={project.repo}>
          <div className="project-heading"><button className="project-toggle" aria-expanded={!project.collapsed} title={project.repo} onClick={() => void perform(async () => { await workspaceAction({ type: 'collapseProject', repo: project.repo, collapsed: !project.collapsed }); })} disabled={busy}>{project.collapsed ? <ChevronRight size={13}/> : <ChevronDown size={13}/>}<Folder size={15}/><span>{project.name}</span></button><button className="icon-button project-add" title={`Open review in ${project.name}`} aria-label={`Open review in ${project.name}`} onClick={() => openReview(project.repo)} disabled={busy}><Plus size={14}/></button></div>
          {!project.collapsed && <div className="project-tabs" role="tablist" aria-label={`${project.name} reviews`} aria-orientation="vertical">
            {project.tabs.map(item => <div className={`review-tab ${review?.key === item.id ? 'active' : ''}`} key={item.id}><button role="tab" aria-selected={review?.key === item.id} aria-controls="review-content" title={`${item.label} ← ${shortRef(item.request.base || '')}`} disabled={busy} onClick={() => load(item.request, item.file)}>{item.kind === 'pr' ? <GitPullRequest size={14}/> : <GitBranch size={14}/>}<span>{item.label}</span>{jobs[item.id]?.run.status === 'running' && <Loader2 size={12} className="spin"/>}</button><button className="tab-close" title={`Close ${item.label}`} aria-label={`Close ${item.label}`} disabled={busy} onClick={() => closeTab(item)}><X size={12}/></button></div>)}
            {!project.tabs.length && <button className="add-review" disabled={busy} onClick={() => openReview(project.repo)}><Plus size={13}/>Open review</button>}
          </div>}
        </div>)}
      </nav>
      <button className="add-project" onClick={openProject} disabled={busy}><Plus size={15}/>Add project</button>
    </aside>

    <main className="main" id="review-content">
      {error && !modal && <div className="error-banner" role="alert"><span>{error}</span><button className="icon-button" aria-label="Dismiss error" onClick={() => setError('')}><X size={15}/></button></div>}
      {!review ? <div className="welcome"><Layers2 size={30}/><h1>Open a project</h1><div className="welcome-actions"><button className="primary" onClick={openProject} disabled={busy}><Folder size={15}/>Open project</button><button onClick={() => perform(async () => { await load(await api<ReviewRequest>('demo')); })} disabled={busy}>Try sample</button></div>{busy && <Loader2 size={18} className="spin"/>}</div> : <>
        <header className="review-header"><div className="review-breadcrumb"><span>{review.name}</span><span className="slash">/</span>{tab?.kind === 'pr' ? <GitPullRequest size={15}/> : <GitBranch size={15}/>}<button className="breadcrumb-branch" title="Open another branch or PR" onClick={() => openReview(review.repo)} disabled={busy}><span className="breadcrumb-base">{shortRef(review.base)}</span><ArrowRight size={12}/><strong>{tab?.label || shortRef(review.head)}</strong><ChevronDown size={12}/></button></div><div className="header-actions">{job && <button className="icon-button" title={running ? 'Agent running' : 'Agent response'} aria-label={running ? 'Agent running' : 'Agent response'} onClick={() => setModal({ type: 'result' })}>{running ? <Loader2 size={16} className="spin"/> : <MessageSquare size={16}/>}</button>}<button className="ask-button" onClick={() => openAgent('ask')} disabled={busy}><Sparkles size={14}/>Ask agent</button></div></header>
        <div className="files-toolbar"><button className={`icon-button file-toggle ${showFiles ? 'pressed' : ''}`} aria-label="Toggle changed files" aria-pressed={showFiles} onClick={() => setShowFiles(v => !v)}><PanelLeft size={15}/></button><span>Files changed <b className="count">{review.files.length}</b></span><span className="diff-stat"><span>+{added}</span><span>−{removed}</span></span><div className="toolbar-spacer"/><select className="mode-select" aria-label="Comparison scope" value={review.mode} onChange={e => load({ ...request!, mode: e.target.value as ReviewRequest['mode'] }, active)} disabled={busy}><option value="branch">Committed</option><option value="working">Working tree{review.dirty ? ' •' : ''}</option></select><button className="icon-button fetch-button" title="Fetch remotes" aria-label="Fetch remotes" disabled={busy} onClick={() => perform(async () => { await api('fetch', { repo: review.repo }); await load(request!, active); })}><ArrowDownToLine size={15}/></button><button className="icon-button" title="Refresh diff" aria-label="Refresh diff" disabled={busy} onClick={() => load(request!, active)}><RefreshCw size={15} className={busy ? 'spin' : ''}/></button>{state.comments.length > 0 && <button className="changes-button" onClick={() => { setPrompt(''); setModal({ type: 'queue' }); }}><MessageSquare size={14}/>Changes <b className="count">{queued.length}</b></button>}</div>
        <div className={`review-body ${showFiles ? '' : 'files-hidden'}`}>
          <aside className="files-sidebar" aria-label="Changed files"><div className="file-search"><Search size={13}/><input ref={searchBox} aria-label="Filter changed files" placeholder="Filter files" value={search} onChange={e => setSearch(e.target.value)}/></div><div className="file-list">{filtered.map(f => <button className={`file-row ${f.path === active ? 'active' : ''}`} key={f.path} title={f.path} onClick={() => selectFile(f.path)}><FileCode2 size={14}/><span>{f.path}</span>{state.viewed[f.path] ? <Check size={13} className="viewed-check"/> : <span className={`file-status status-${f.status}`}>{f.status}</span>}</button>)}{!filtered.length && <div className="empty-label">{search ? 'No matching files' : 'No changes'}</div>}</div></aside>
          <section className="diff-pane" aria-label="Diff"><div className="diff-toolbar"><div className="diff-filename"><FileCode2 size={14}/><span title={active}>{active || 'No changes'}</span><select className="file-jump" aria-label="Changed file" value={active} onChange={e => selectFile(e.target.value)}>{review.files.map(f => <option key={f.path} value={f.path}>{f.path}</option>)}</select></div><div className="diff-actions"><div className="segmented icon-segments"><button className={layout === 'split' ? 'selected' : ''} title="Split diff" aria-label="Split diff" aria-pressed={layout === 'split'} onClick={() => setLayout('split')}><Columns2 size={14}/></button><button className={layout === 'unified' ? 'selected' : ''} title="Unified diff" aria-label="Unified diff" aria-pressed={layout === 'unified'} onClick={() => setLayout('unified')}><Layers2 size={14}/></button></div><button className={`viewed-button ${checked ? 'is-viewed' : ''}`} onClick={toggleViewed} disabled={!patch || busy} title="Mark file viewed (v)" aria-pressed={!!checked}>{checked ? <Check size={13}/> : <Square size={13}/>}<span>Viewed</span></button></div></div>
            {activeFile?.oldPath && <div className="rename-banner">{activeFile.oldPath}<ArrowRight size={12}/>{active}</div>}
            {!active ? <div className="diff-empty">No changes</div> : !patch ? <div className="diff-empty" aria-label="Loading diff"><Loader2 size={20} className="spin"/></div> : patch.tooLarge ? <div className="diff-empty">Patch exceeds the 2 MB preview limit.</div> : !patch.patch.includes('@@') ? <div className="diff-empty"><span>{activeFile?.binary || patch.patch.includes('Binary') ? 'Binary file changed' : 'Metadata change'}</span><pre>{patch.patch || 'No text changes'}</pre></div> : <Virtualizer key={`${review.key}-${active}-${refresh}`} className="diff-scroll"><PatchDiff<Annotation> patch={patch.patch} options={diffOptions} lineAnnotations={annotations} selectedLines={selection ? { start: selection.start, end: selection.end, side: selection.side } : null} renderAnnotation={({ metadata }) => metadata.type === 'selection' ? <div className="selection-actions" onMouseDown={e => e.stopPropagation()}><button onClick={() => openAgent('ask', selection)}><Sparkles size={14}/>Ask agent</button><button onClick={() => openAgent('change', selection)}><MessageSquare size={14}/>Agent change</button><button className="icon-button" aria-label="Clear selection" onClick={() => setSelection(undefined)}><X size={14}/></button></div> : <div className="inline-comment"><MessageSquare size={14}/><p>{metadata.comment.body}</p><span>{metadata.comment.status === 'sent' ? 'Sent' : 'Queued'}</span></div>}/></Virtualizer>}
          </section>
        </div>
      </>}
    </main>

    {modal && <Dialog key={modal.type} title={modal.type === 'project' ? 'Open project' : modal.type === 'review' ? 'Open review' : modal.type === 'ask' ? 'Ask agent' : modal.type === 'change' ? 'Agent change' : modal.type === 'queue' ? 'Changes' : 'Agent response'} close={() => setModal(null)} wide={modal.type === 'result' || modal.type === 'queue'}>
      {modal.type === 'project' && <><div className="segmented dialog-tabs"><button className={projectMode === 'local' ? 'selected' : ''} onClick={() => setProjectMode('local')}>Local</button><button className={projectMode === 'clone' ? 'selected' : ''} onClick={() => setProjectMode('clone')}>Clone repository</button></div><form onSubmit={e => { e.preventDefault(); void perform(async () => { const repo = projectMode === 'clone' ? (await api<{ repo: string }>('clone', { url: urlInput, destination: repoInput })).repo : repoInput; await load({ repo }); }); }}>{projectMode === 'clone' && <label>Git URL<input data-autofocus placeholder="https://github.com/owner/repo.git" value={urlInput} onChange={e => setUrlInput(e.target.value)}/></label>}<label>{projectMode === 'clone' ? 'Destination folder' : 'Project folder'}<input data-autofocus={projectMode === 'local' ? true : undefined} placeholder="/Users/you/dev/project" value={repoInput} onChange={e => setRepoInput(e.target.value)}/></label><footer className="dialog-footer">{window.revueDesktop && projectMode === 'local' && <button type="button" onClick={chooseRepo} disabled={busy}><Folder size={14}/>Browse</button>}<div className="toolbar-spacer"/><button className="primary" type="submit" disabled={busy || !repoInput.trim() || (projectMode === 'clone' && !urlInput.trim())}>{busy && <Loader2 className="spin" size={14}/>}Open project</button></footer></form></>}
      {modal.type === 'review' && target && <><div className="segmented dialog-tabs"><button className={reviewMode === 'branch' ? 'selected' : ''} onClick={() => setReviewMode('branch')}>Branch</button><button className={reviewMode === 'pr' ? 'selected' : ''} onClick={() => setReviewMode('pr')}>Pull request</button></div><form onSubmit={e => { e.preventDefault(); void perform(async () => { if (reviewMode === 'pr') await load(await api<ReviewRequest>('pr', { repo: target.repo, value: prInput.trim() })); else await load({ repo: target.repo, base: baseInput, head: headInput }); }); }}>{reviewMode === 'branch' ? <><label>Branch<select data-autofocus aria-label="Review branch" value={headInput} onChange={e => setHeadInput(e.target.value)}>{Array.from(new Set([headInput, ...target.branches.filter(ref => !ref.startsWith('revue/'))])).map(ref => <option key={ref} value={ref}>{shortRef(ref)}</option>)}</select></label><label>Base<select aria-label="Base branch" value={baseInput} onChange={e => setBaseInput(e.target.value)}>{Array.from(new Set([baseInput, ...target.branches.filter(ref => !ref.startsWith('revue/'))])).map(ref => <option key={ref} value={ref}>{shortRef(ref)}</option>)}</select></label></> : <label>PR number or URL<input data-autofocus aria-label="PR number or URL" placeholder="42 or https://github.com/owner/repo/pull/42" value={prInput} onChange={e => setPrInput(e.target.value)}/></label>}<footer className="dialog-footer"><span className="muted project-context"><Folder size={14}/>{target.name}</span><div className="toolbar-spacer"/><button className="primary" type="submit" disabled={busy || (reviewMode === 'pr' && !prInput.trim())}>{busy && <Loader2 size={14} className="spin"/>}Open review</button></footer></form></>}
      {(modal.type === 'ask' || modal.type === 'change') && <form onSubmit={e => { e.preventDefault(); void (modal.type === 'ask' ? startAgent('ask') : saveComment()); }}><div className="prompt-context">{modal.selection ? <><FileCode2 size={14}/><span>{locationLabel(modal.selection)}</span></> : <><GitPullRequest size={14}/><span>{tab?.label} · Entire review</span></>}</div><textarea data-autofocus aria-label={modal.type === 'ask' ? 'Ask your agent' : 'Write review feedback'} placeholder={modal.type === 'ask' ? 'Ask a question…' : 'What should change?'} value={draft} onChange={e => setDrafts(prev => ({ ...prev, [modal.draftKey]: e.target.value }))} onKeyDown={e => { if ((e.metaKey || e.ctrlKey) && e.key === 'Enter' && draft.trim() && !busy && (modal.type === 'change' || (!running && agentAvailable))) { e.preventDefault(); void (modal.type === 'ask' ? startAgent('ask') : saveComment()); } }}/><footer className="dialog-footer">{modal.type === 'ask' && providerSelect}<div className="toolbar-spacer"/><button className="primary" type="submit" disabled={!draft.trim() || busy || (modal.type === 'ask' && (running || !agentAvailable))}>{busy ? <Loader2 size={14} className="spin"/> : modal.type === 'ask' ? <ArrowRight size={14}/> : <Plus size={14}/>} {modal.type === 'ask' ? 'Ask agent' : 'Queue change'}</button></footer></form>}
      {modal.type === 'queue' && <><div className="change-list">{state.comments.map(c => <article className="change-card" key={c.id}><div className="change-heading">{c.status === 'queued' ? <input aria-label={`Include change on ${c.path}:${c.start}`} type="checkbox" checked={included[c.id] !== false} onChange={e => { setIncluded(prev => ({ ...prev, [c.id]: e.target.checked })); setPrompt(''); }}/> : <Check size={14}/>}<button className="comment-location" onClick={() => { selectFile(c.path); setModal(null); }}>{locationLabel(c)}</button><span className="muted">{c.status === 'sent' ? 'Sent' : ''}</span><button className="icon-button" aria-label={`Remove change on ${c.path}:${c.start}`} disabled={busy || running} onClick={() => perform(async () => { setState(await api('deleteComment', { review: request, id: c.id })); setPrompt(''); })}><Trash2 size={14}/></button></div><p>{c.body}</p></article>)}{!state.comments.length && <div className="empty-label">No queued changes</div>}</div>{prompt && <details className="prompt-preview" open><summary>Agent prompt</summary><pre>{prompt}</pre></details>}<footer className="dialog-footer">{providerSelect}<button className="preview-button" onClick={previewPrompt} disabled={!batch.length || busy}>Preview prompt</button><div className="toolbar-spacer"/><button className="primary" disabled={!batch.length || busy || running || !agentAvailable} onClick={() => startAgent('apply')}><Send size={14}/>Send {batch.length || ''} to agent</button></footer></>}
      {modal.type === 'result' && job && <><div className="result-question">{job.question}</div><div className="result-heading"><Sparkles size={15}/><strong>{job.provider === 'codex' ? 'Codex' : 'Claude Code'}</strong><span className="muted">{job.run.status === 'running' ? 'Working' : job.run.status}</span></div><div className="agent-output">{job.run.events.filter(e => e.kind !== 'activity').map((e, i) => <div key={i} className={`agent-copy ${e.kind}`}>{e.text}</div>)}{running && <div className="activity"><Loader2 size={14} className="spin"/>{[...job.run.events].reverse().find(e => e.kind === 'activity')?.text || 'Connecting…'}</div>}{job.run.events.some(e => e.kind === 'activity') && <details className="activity-details"><summary>Activity</summary>{job.run.events.filter(e => e.kind === 'activity').map((e, i) => <p key={i}>{e.text}</p>)}</details>}</div><footer className="dialog-footer"><div className="toolbar-spacer"/>{running ? <button onClick={() => perform(async () => { await api('cancel', { id: job.run.id }); })}><Square size={12}/>Stop agent</button> : job.run.mode === 'apply' ? <button className="primary" onClick={() => load({ ...job.request, mode: 'working' }, active)}>Review local changes<ArrowRight size={14}/></button> : <button onClick={() => openAgent('ask')}>Ask another question</button>}</footer></>}
      {error && <div className="modal-error" role="alert">{error}</div>}
    </Dialog>}
  </div>;
}

createRoot(document.getElementById('root')!).render(<App/>);
