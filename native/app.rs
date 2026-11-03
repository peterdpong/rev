use crate::{
    backend::{Backend, Reply},
    model::*,
};
use gpui_kit::component::{
    Disableable, Selectable, Sizable,
    button::{Button, ButtonVariants},
    diff::*,
    input::{Input, InputState, Textarea, TextareaState},
    tab::{Tab, TabBar},
};
use gpui_kit::prelude::*;
use gpui_kit::*;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    rc::Rc,
    time::Duration,
};

mod commits;
mod continuous;
mod file_sheet;
mod motion;
mod navigation;
mod panes;
mod projects;
mod review;
mod thread_panel;
mod threads;
mod view;
use commits::CommitPicker;
gpui_kit::actions!(
    rev,
    [
        Quit,
        OpenProject,
        AskReview,
        Refresh,
        CloseModal,
        Submit,
        PromptAsk,
        PromptChange,
        TogglePromptMode,
        ToggleSidebar,
        ToggleThreads,
        NextTab,
        PreviousTab,
        GoBack,
        GoForward,
        ReplyThread,
        NextFile,
        PreviousFile,
        MarkViewed,
        BrowseFiles,
        ChooseFile
    ]
);

pub fn bind_shortcuts(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-o", OpenProject, Some("Rev")),
        KeyBinding::new("cmd-r", Refresh, Some("Rev")),
        KeyBinding::new("cmd-k", AskReview, Some("Rev")),
        KeyBinding::new("cmd-b", ToggleSidebar, Some("Rev")),
        KeyBinding::new("cmd-shift-b", ToggleThreads, Some("Rev")),
        KeyBinding::new("alt-tab", NextTab, Some("Rev")),
        KeyBinding::new("alt-shift-tab", PreviousTab, Some("Rev")),
        KeyBinding::new("cmd-[", GoBack, Some("Rev")),
        KeyBinding::new("cmd-]", GoForward, Some("Rev")),
        KeyBinding::new("cmd-p", BrowseFiles, Some("Rev")),
        KeyBinding::new("enter", ChooseFile, Some("RevFiles > Input")),
        KeyBinding::new("escape", CloseModal, Some("RevFiles > Input")),
        KeyBinding::new("enter", ReplyThread, Some("RevThread > Input")),
        KeyBinding::new("enter", Submit, Some("RevPrompt > Input")),
        KeyBinding::new("enter", Submit, Some("RevAgent > Input")),
        KeyBinding::new("cmd-enter", ReplyThread, Some("RevThread > Input")),
        KeyBinding::new("cmd-enter", ReplyThread, Some("RevThread")),
        KeyBinding::new("alt-down", NextFile, Some("Rev")),
        KeyBinding::new("alt-up", PreviousFile, Some("Rev")),
        KeyBinding::new("cmd-shift-v", MarkViewed, Some("Rev")),
        KeyBinding::new("escape", CloseModal, Some("RevPopover")),
        KeyBinding::new("cmd-enter", Submit, Some("RevPopover")),
        KeyBinding::new("cmd-1", PromptAsk, Some("Rev")),
        KeyBinding::new("cmd-2", PromptChange, Some("Rev")),
        KeyBinding::new("ctrl-tab", TogglePromptMode, Some("RevPrompt")),
        KeyBinding::new("ctrl-shift-tab", TogglePromptMode, Some("RevPrompt")),
        KeyBinding::new("escape", CloseModal, Some("Rev")),
        KeyBinding::new("cmd-enter", Submit, Some("RevPrompt")),
        KeyBinding::new("cmd-enter", Submit, Some("RevPrompt > Input")),
        KeyBinding::new("cmd-enter", Submit, Some("RevPopover > Input")),
        KeyBinding::new("escape", CloseModal, Some("RevPrompt > Input")),
        KeyBinding::new("escape", CloseModal, Some("RevPopover > Input")),
    ]);
}

#[derive(Clone, Copy, PartialEq)]
enum ModalKind {
    Open,
    Review,
    Ask,
    Change,
    Changes,
    Run,
    Commits,
    Details,
}
#[derive(Clone)]
struct Modal {
    kind: ModalKind,
    request: ReviewRequest,
    tab: String,
    selection: Option<Selection>,
    digest: String,
    serial: u64,
    anchor: Option<SharedString>,
    anchor_bounds: Rc<Cell<Bounds<Pixels>>>,
}

pub struct Rev {
    backend: Option<Backend>,
    workspace: Workspace,
    navigation: navigation::Navigation,
    sidebar_collapsed: bool,
    sidebar_focus: FocusHandle,
    review: Option<Review>,
    state: ReviewState,
    threads_visible: bool,
    thread_filter: Option<String>,
    changes_only: bool,
    show_archived: bool,
    active_thread: Option<String>,
    thread_reply: Entity<TextareaState>,
    thread_scroll: ListState,
    thread_items: Vec<(String, bool)>,
    thread_hinted_width: Option<Pixels>,
    thread_scroll_target: Option<String>,
    file: Option<String>,
    file_sheet: file_sheet::FileSheet,
    patch: FilePatch,
    patches: HashMap<String, FilePatch>,
    loading_patches: HashSet<String>,
    pending_file_navigation: Option<continuous::PendingFileNavigation>,
    diff: Entity<DiffState>,
    diff_pane: Entity<panes::Pane>,
    files_pane: Entity<panes::Pane>,
    threads_pane: Entity<panes::Pane>,
    selection: Option<Selection>,
    selecting: bool,
    selection_menu: Point<Pixels>,
    prompt_mode: ModalKind,
    commit_picker: CommitPicker,
    branch_options: Vec<String>,
    branches_loading: bool,
    filter: Entity<InputState>,
    fields: Vec<Entity<InputState>>,
    prompt: Entity<TextareaState>,
    focus: FocusHandle,
    modal_focus: FocusHandle,
    modal: Option<Modal>,
    modal_serial: u64,
    drafts: HashMap<String, String>,
    preview: Option<String>,
    providers: Vec<Provider>,
    provider: String,
    new_session: bool,
    runs: HashMap<String, AgentRun>,
    run_requests: HashMap<String, ReviewRequest>,
    comment_ids: HashSet<String>,
    loading: bool,
    patch_loading: bool,
    submitting: bool,
    workspace_busy: bool,
    workspace_epoch: u64,
    review_epoch: u64,
    file_epoch: u64,
    error: Option<String>,
    split: bool,
    wrap: bool,
    _subscriptions: Vec<Subscription>,
}

fn decode<T: DeserializeOwned>(value: Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|e| format!("Invalid local service response: {e}"))
}

impl Rev {
    pub fn new(
        backend: Result<Backend, String>,
        initial: Option<ReviewRequest>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (backend, error) = match backend {
            Ok(b) => (Some(b), None),
            Err(e) => (None, Some(e)),
        };
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Find a file…"));
        let fields: Vec<Entity<InputState>> = (0..3)
            .map(|index| {
                cx.new(|cx| {
                    InputState::new(window, cx).placeholder(match index {
                        0 => "Repository folder",
                        1 => "Automatic",
                        _ => "Search branches or enter a PR number or URL",
                    })
                })
            })
            .collect();
        let prompt = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(5)
                .placeholder("Write a question or change request…")
        });
        let diff = cx.new(|cx| DiffState::new([], cx));
        let owner = cx.entity();
        let diff_pane =
            cx.new(|cx| panes::Pane::new(owner.clone(), panes::PaneKind::Diff, &diff, cx));
        let files_pane =
            cx.new(|cx| panes::Pane::new(owner.clone(), panes::PaneKind::Files, &diff, cx));
        let threads_pane =
            cx.new(|cx| panes::Pane::new(owner.clone(), panes::PaneKind::Threads, &diff, cx));
        let thread_reply = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(2)
                .placeholder("Send a follow-up…")
        });
        let subscriptions = vec![
            cx.observe(&filter, |_, _, cx| cx.notify()),
            cx.observe(&fields[2], |_, _, cx| cx.notify()),
            cx.observe_in(&diff, window, |this, diff, window, cx| {
                let state = diff.read(cx);
                if state.selected_lines().is_some() {
                    return;
                } // Gutter/keyboard events own this range.
                let selecting = state.is_selecting_text(cx);
                let range = state.selected_text_lines(cx);
                if selecting {
                    if !this.selecting {
                        this.selecting = true;
                        cx.notify();
                    }
                } else if let Some(range) = range {
                    this.selecting = false;
                    if this.selection != Selection::from_range(&range) {
                        this.selection_menu = window.mouse_position() + point(px(12.), px(18.));
                        this.accept_selection(&range, window, cx);
                    }
                } else if this.selecting {
                    this.selecting = false;
                    this.selection = None;
                    this.close_selection_prompt(window, cx);
                    cx.notify();
                }
            }),
            cx.subscribe_in(&diff, window, |this, _, event: &DiffEvent, window, cx| {
                match event {
                    DiffEvent::VisibleFileChanged(path) => {
                        this.set_active_file(path.to_string(), cx)
                    }
                    DiffEvent::FileExpanded(path) => {
                        this.load_file_patch(path.to_string(), window, cx);
                    }
                    DiffEvent::SelectionStarted(_) => this.selecting = true,
                    DiffEvent::SelectionEnded(range) => {
                        this.selecting = false;
                        this.selection_menu = window.mouse_position() + point(px(12.), px(18.));
                        this.accept_selection(range, window, cx);
                    }
                    DiffEvent::SelectionChanged(None) if !this.selecting => this.selection = None,
                    _ => {}
                }
                cx.notify();
            }),
        ];
        let mut app = Self {
            backend,
            workspace: Workspace::default(),
            navigation: navigation::Navigation::default(),
            sidebar_collapsed: false,
            sidebar_focus: cx.focus_handle(),
            review: None,
            state: ReviewState::default(),
            threads_visible: false,
            thread_filter: None,
            changes_only: false,
            show_archived: false,
            active_thread: None,
            thread_reply,
            thread_scroll: ListState::new(0, ListAlignment::Top, px(0.)),
            thread_items: Vec::new(),
            thread_hinted_width: None,
            thread_scroll_target: None,
            file: None,
            file_sheet: file_sheet::FileSheet::default(),
            patch: FilePatch::default(),
            patches: HashMap::new(),
            loading_patches: HashSet::new(),
            pending_file_navigation: None,
            diff,
            diff_pane,
            files_pane,
            threads_pane,
            selection: None,
            selecting: false,
            selection_menu: Point::default(),
            prompt_mode: ModalKind::Ask,
            commit_picker: CommitPicker::default(),
            branch_options: vec![],
            branches_loading: false,
            filter,
            fields,
            prompt,
            focus: cx.focus_handle(),
            modal_focus: cx.focus_handle(),
            modal: None,
            modal_serial: 0,
            drafts: HashMap::new(),
            preview: None,
            providers: vec![],
            provider: "codex".into(),
            new_session: false,
            runs: HashMap::new(),
            run_requests: HashMap::new(),
            comment_ids: HashSet::new(),
            loading: false,
            patch_loading: false,
            submitting: false,
            workspace_busy: false,
            workspace_epoch: 0,
            review_epoch: 0,
            file_epoch: 0,
            error,
            split: false,
            wrap: false,
            _subscriptions: subscriptions,
        };
        app.focus.focus(window, cx);
        if app.backend.is_some() {
            app.rpc(
                "init",
                json!({}),
                window,
                cx,
                move |this, result, window, cx| {
                    let Some(value) = this.result(result) else {
                        return;
                    };
                    match (
                        decode(value["workspace"].clone()),
                        decode(value["providers"].clone()),
                    ) {
                        (Ok(workspace), Ok(providers)) => {
                            this.workspace = workspace;
                            this.providers = providers;
                        }
                        (Err(e), _) | (_, Err(e)) => {
                            this.error = Some(e);
                            return;
                        }
                    }
                    if let Some(provider) = this.providers.iter().find(|p| p.available) {
                        this.provider = provider.id.clone();
                    }
                    this.poll_workspace(window, cx);
                    let tab = this.workspace.active().cloned();
                    if let Some(request) = initial {
                        this.load_review(request, None, window, cx);
                    } else if let Some(tab) = tab {
                        this.load_review(tab.request, tab.file, window, cx);
                    }
                },
            );
        }
        app
    }

    fn rpc(
        &mut self,
        method: &'static str,
        params: Value,
        window: &Window,
        cx: &mut Context<Self>,
        done: impl FnOnce(&mut Self, Reply, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let Some(backend) = self.backend.clone() else {
            self.error = Some("The local service is unavailable. Restart Rev.".into());
            return;
        };
        let task = cx
            .background_executor()
            .spawn(async move { backend.call(method, params) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                done(this, result, window, cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn result(&mut self, result: Reply) -> Option<Value> {
        match result {
            Ok(value) => Some(value),
            Err(error) => {
                self.error = Some(error);
                None
            }
        }
    }

    fn load_review(
        &mut self,
        request: ReviewRequest,
        file: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.loading || self.workspace_busy {
            return;
        }
        let previous = self.navigation_location(cx);
        let travel = self.navigation.travel.take();
        self.close_selection_prompt(window, cx);
        self.hide_file_sheet(window, cx);
        self.loading = true;
        self.patch_loading = false;
        self.error = None;
        self.review_epoch += 1;
        self.file_epoch += 1;
        self.loading_patches.clear();
        self.pending_file_navigation = None;
        let epoch = self.review_epoch;
        let mut params = json!(request);
        params["remember"] = json!(true);
        params["preview"] = json!(true);
        params["current"] = json!(request.head.as_deref().is_none_or(|head| head == "HEAD"));
        let modal_serial = self.modal_serial;
        self.rpc(
            "review",
            params,
            window,
            cx,
            move |this, result, window, cx| {
                if this.review_epoch != epoch {
                    return;
                }
                this.loading = false;
                let Some(value) = this.result(result) else {
                    return;
                };
                let parsed = decode::<Review>(value["review"].clone()).and_then(|review| {
                    Ok((
                        review,
                        decode::<ReviewState>(value["state"].clone())?,
                        decode::<Workspace>(value["workspace"].clone())?,
                    ))
                });
                match parsed {
                    Ok((mut review, state, workspace)) => {
                        // Navigation follows HEAD; comments and agent requests keep the reviewed branch.
                        review.pin_current_request();
                        if this.modal_serial == modal_serial
                            && this.modal.as_ref().is_some_and(|m| {
                                matches!(m.kind, ModalKind::Open | ModalKind::Review)
                            })
                        {
                            this.close_modal(window, cx);
                        }
                        let selected = file
                            .or_else(|| workspace.active().and_then(|t| t.file.clone()))
                            .filter(|file| review.files.iter().any(|f| &f.path == file))
                            .or_else(|| review.files.first().map(|f| f.path.clone()));
                        if this
                            .review
                            .as_ref()
                            .is_none_or(|old| old.scope_key != review.scope_key)
                        {
                            this.threads_visible =
                                !state.threads.is_empty() || !state.comments.is_empty();
                            this.changes_only = false;
                            this.thread_filter = None;
                            if let Some(id) = this.active_thread.take() {
                                this.drafts.insert(
                                    format!("thread-{id}"),
                                    this.thread_reply.read(cx).value().to_string(),
                                );
                            }
                            this.thread_reply
                                .update(cx, |input, cx| input.set_value("", window, cx));
                        }
                        this.workspace = workspace;
                        this.navigation.loaded(
                            previous,
                            this.workspace
                                .active()
                                .map(|tab| tab.request.clone())
                                .unwrap_or_else(|| review.request.clone()),
                            &review.scope_key,
                            travel,
                        );
                        this.state = state;
                        this.review = Some(review);
                        this.file = selected;
                        this.patch = FilePatch::default();
                        this.patches.clear();
                        this.selection = None;
                        this.diff.update(cx, |diff, cx| diff.set_files([], cx));
                        this.filter
                            .update(cx, |input, cx| input.set_value("", window, cx));
                        this.load_patches(window, cx);
                    }
                    Err(e) => this.error = Some(e),
                }
            },
        );
        cx.notify();
    }

    fn toggle_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal.is_some() {
            self.close_modal(window, cx);
        }
        self.sidebar_collapsed = !self.sidebar_collapsed;
        if self.sidebar_collapsed && self.sidebar_focus.contains_focused(window, cx) {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }

    fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(request) = self.navigation_request() {
            self.load_review(request, self.file.clone(), window, cx);
        }
    }

    fn workspace_action(
        &mut self,
        action: Value,
        navigate: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_busy || self.loading {
            return;
        }
        self.workspace_busy = true;
        self.workspace_epoch += 1;
        self.rpc(
            "workspaceAction",
            action,
            window,
            cx,
            move |this, result, window, cx| {
                this.workspace_busy = false;
                let Some(value) = this.result(result) else {
                    return;
                };
                match decode::<Workspace>(value) {
                    Ok(workspace) => {
                        this.workspace = workspace;
                        if navigate {
                            if let Some(tab) = this.workspace.active().cloned() {
                                this.load_review(tab.request, tab.file, window, cx);
                            } else {
                                this.review = None;
                                this.state = ReviewState::default();
                                this.file = None;
                                this.selection = None;
                                this.patch = FilePatch::default();
                                this.patches.clear();
                                this.file_epoch += 1;
                                this.loading_patches.clear();
                                this.pending_file_navigation = None;
                                this.diff.update(cx, |diff, cx| diff.set_files([], cx));
                            }
                        }
                    }
                    Err(e) => this.error = Some(e),
                }
            },
        );
        cx.notify();
    }

    fn navigate_file(&mut self, direction: isize, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal.is_some() || self.loading {
            return;
        }
        let files = self.visible_files(cx);
        if files.is_empty() {
            return;
        }
        let index = files
            .iter()
            .position(|f| Some(&f.path) == self.file.as_ref())
            .unwrap_or(0);
        let next = (index as isize + direction).clamp(0, files.len() as isize - 1) as usize;
        self.jump_file(files[next].path.clone(), window, cx);
    }

    fn visible_files(&self, cx: &App) -> Vec<File> {
        let filter = self.filter.read(cx).value().to_lowercase();
        self.review
            .as_ref()
            .map(|r| {
                r.files
                    .iter()
                    .filter(|f| f.path.to_lowercase().contains(&filter))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    fn mark_viewed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(path) = self.file.clone() {
            self.mark_file_viewed(path, window, cx);
        }
    }

    fn draft_key(modal: &Modal) -> String {
        format!(
            "{}:{}:{}:{}",
            modal.tab,
            modal.kind as u8,
            serde_json::to_string(&modal.request).unwrap_or_default(),
            modal
                .selection
                .as_ref()
                .map(Selection::label)
                .unwrap_or_default()
        )
    }
    fn close_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.hide_file_sheet(window, cx);
        if let Some(modal) = self.modal.take()
            && matches!(modal.kind, ModalKind::Ask | ModalKind::Change)
        {
            self.drafts.insert(
                Self::draft_key(&modal),
                self.prompt.read(cx).value().to_string(),
            );
        }
        self.preview = None;
        self.modal_serial += 1;
        self.selection = None;
        self.selecting = false;
        gpui_kit::base::TextSelection::clear(window, cx);
        self.diff
            .update(cx, |diff, cx| diff.set_selected_lines(None, cx));
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn dismiss_popover(
        &mut self,
        serial: u64,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(modal) = &self.modal else {
            return;
        };
        if modal.serial != serial
            || (modal.anchor.is_some() && modal.anchor_bounds.get().contains(&event.position))
            || (Self::is_selection_prompt(modal) && event.modifiers.shift)
        {
            return;
        }
        self.close_modal(window, cx);
    }

    fn accept_selection(
        &mut self,
        range: &DiffLineRange,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selection = Selection::from_range(range);
        const CROSS_SIDE: &str = "Select lines on one side of the diff.";
        if self.selection.is_none() {
            self.error = Some(CROSS_SIDE.into());
        } else if self.error.as_deref() == Some(CROSS_SIDE) {
            self.error = None;
        }
        let Some(selection) = self.selection.clone() else {
            if self.modal.as_ref().is_some_and(Self::is_selection_prompt)
                && let Some(modal) = self.modal.take()
            {
                self.drafts.insert(
                    Self::draft_key(&modal),
                    self.prompt.read(cx).value().to_string(),
                );
                self.modal_serial += 1;
            }
            return;
        };
        self.set_active_file(selection.path.clone(), cx);
        let Some(review) = self.review.as_ref() else {
            return;
        };
        if self.patch.digest.is_empty() || self.patch_loading {
            return;
        }
        if self.submitting
            || self
                .modal
                .as_ref()
                .is_some_and(|m| !Self::is_selection_prompt(m))
        {
            return;
        }
        if self.modal.as_ref().is_some_and(|m| {
            m.selection.as_ref() == Some(&selection)
                && m.digest == self.patch.digest
                && m.tab == review.scope_key
        }) {
            return;
        }
        if let Some(previous) = &self.modal {
            self.drafts.insert(
                Self::draft_key(previous),
                self.prompt.read(cx).value().to_string(),
            );
        }
        self.modal_serial += 1;
        let modal = Modal {
            kind: self.prompt_mode,
            request: review.request.clone(),
            tab: review.scope_key.clone(),
            selection: Some(selection),
            digest: self.patch.digest.clone(),
            serial: self.modal_serial,
            anchor: None,
            anchor_bounds: Default::default(),
        };
        let draft = self
            .drafts
            .get(&Self::draft_key(&modal))
            .cloned()
            .unwrap_or_default();
        self.prompt
            .update(cx, |input, cx| input.set_value(draft, window, cx));
        self.modal = Some(modal);
        self.preview = None;
        self.new_session = false;
        // Preserve diff focus so Shift-selection and copying keep working.
        cx.notify();
    }

    fn is_selection_prompt(modal: &Modal) -> bool {
        matches!(modal.kind, ModalKind::Ask | ModalKind::Change)
            && modal.selection.as_ref().is_some_and(|s| s.start > 0)
    }

    fn close_selection_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal.as_ref().is_some_and(Self::is_selection_prompt) {
            self.close_modal(window, cx);
        }
    }

    fn switch_prompt_mode(&mut self, kind: ModalKind, window: &mut Window, cx: &mut Context<Self>) {
        let Some(modal) = &mut self.modal else {
            return;
        };
        if self.submitting
            || self.selecting
            || !matches!(modal.kind, ModalKind::Ask | ModalKind::Change)
        {
            return;
        }
        modal.kind = kind;
        self.prompt_mode = kind;
        self.error = None;
        self.preview = None;
        self.prompt.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    fn toggle_prompt_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let kind = if self
            .modal
            .as_ref()
            .is_some_and(|m| m.kind == ModalKind::Ask)
        {
            ModalKind::Change
        } else {
            ModalKind::Ask
        };
        self.switch_prompt_mode(kind, window, cx);
    }

    fn open_modal(
        &mut self,
        kind: ModalKind,
        selection: Option<Selection>,
        repo: Option<String>,
        anchor: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.loading || self.patch_loading || self.submitting {
            return;
        }
        if self
            .modal
            .as_ref()
            .is_some_and(|m| m.kind == kind && m.anchor.as_deref() == Some(anchor))
        {
            self.close_modal(window, cx);
            return;
        }
        self.close_modal(window, cx);
        self.error = None;
        self.new_session = false;
        let mut request = self
            .review
            .as_ref()
            .map(|r| r.request.clone())
            .unwrap_or_default();
        if kind == ModalKind::Commits {
            request = self.navigation_request().unwrap_or(request);
        }
        if let Some(repo) = repo {
            request = ReviewRequest {
                repo,
                ..Default::default()
            };
        }
        let tab = self
            .review
            .as_ref()
            .map(|r| r.scope_key.clone())
            .unwrap_or_default();
        let modal = Modal {
            kind,
            request: request.clone(),
            tab,
            anchor: selection
                .as_ref()
                .is_none_or(|s| s.start == 0)
                .then(|| SharedString::from(anchor.to_owned())),
            anchor_bounds: Default::default(),
            selection,
            digest: self.patch.digest.clone(),
            serial: self.modal_serial,
        };
        let values: Vec<String> = match kind {
            ModalKind::Review => vec![request.repo.clone(), String::new(), String::new()],
            _ => vec![String::new(); 3],
        };
        for (input, value) in self.fields.iter().zip(values) {
            input.update(cx, |input, cx| input.set_value(value, window, cx));
        }
        let draft = self
            .drafts
            .get(&Self::draft_key(&modal))
            .cloned()
            .unwrap_or_default();
        self.prompt
            .update(cx, |input, cx| input.set_value(draft, window, cx));
        self.comment_ids = self
            .state
            .comments
            .iter()
            .filter(|c| c.status == "queued")
            .map(|c| c.id.clone())
            .collect();
        self.modal = Some(modal);
        self.modal_focus.focus(window, cx);
        if matches!(kind, ModalKind::Ask | ModalKind::Change) {
            self.prompt.update(cx, |input, cx| input.focus(window, cx));
        } else if kind == ModalKind::Open {
            self.fields[0].update(cx, |input, cx| input.focus(window, cx));
        } else if kind == ModalKind::Review {
            self.fields[2].update(cx, |input, cx| input.focus(window, cx));
            self.branch_options.clear();
            self.load_branches(false, window, cx);
        }
        cx.notify();
    }

    fn pick_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open repository".into()),
        });
        let serial = self.modal_serial;
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(path) = paths.first()
            {
                let path = path.to_string_lossy().to_string();
                let _ = this.update_in(cx, |this, window, cx| {
                    if this.modal_serial == serial {
                        this.fields[0].update(cx, |input, cx| input.set_value(path, window, cx));
                    }
                });
            }
        })
        .detach();
    }

    fn demo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.submitting = true;
        self.rpc("demo", json!({}), window, cx, |this, result, window, cx| {
            this.submitting = false;
            if let Some(value) = this.result(result) {
                match decode(value) {
                    Ok(request) => this.load_review(request, None, window, cx),
                    Err(e) => this.error = Some(e),
                }
            }
        });
    }

    fn fetch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(review) = &self.review else { return };
        let request = review.request.clone();
        let epoch = self.review_epoch;
        self.submitting = true;
        self.rpc(
            "fetch",
            json!({ "repo": request.repo, "review": request }),
            window,
            cx,
            move |this, result, window, cx| {
                this.submitting = false;
                if let Some(value) = this.result(result)
                    && this.review_epoch == epoch
                {
                    if value.get("request").is_some() {
                        match decode(value["request"].clone()) {
                            Ok(request) => this.load_review(request, this.file.clone(), window, cx),
                            Err(error) => this.error = Some(error),
                        }
                    } else {
                        this.refresh(window, cx);
                    }
                }
            },
        );
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.notify();
        if self.submitting || self.loading {
            return;
        }
        let Some(modal) = self.modal.clone() else {
            return;
        };
        let values: Vec<String> = self
            .fields
            .iter()
            .map(|input| input.read(cx).value().trim().to_string())
            .collect();
        let body = self.prompt.read(cx).value().trim().to_string();
        self.error = None;
        match modal.kind {
            ModalKind::Open => {
                if values[0].is_empty() {
                    self.error = Some("Choose a repository folder.".into());
                    return;
                }
                self.load_review(
                    ReviewRequest {
                        repo: values[0].clone(),
                        ..Default::default()
                    },
                    None,
                    window,
                    cx,
                );
            }
            ModalKind::Review => {
                if values[2].is_empty() {
                    self.error = Some("Choose a branch or enter a pull request.".into());
                    return;
                }
                let request = ReviewRequest {
                    repo: modal.request.repo.clone(),
                    base: (!values[1].is_empty()).then(|| values[1].clone()),
                    head: (!values[2].is_empty()).then(|| values[2].clone()),
                    mode: Some("branch".into()),
                    commit_range: None,
                };
                if values[2].starts_with("https://github.com/")
                    || values[2].starts_with('#')
                    || values[2].parse::<u64>().is_ok()
                {
                    self.submitting = true;
                    self.rpc(
                        "pr",
                        json!({ "repo": request.repo, "value": values[2].trim_start_matches('#'), "base": request.base }),
                        window,
                        cx,
                        move |this, result, window, cx| {
                            this.submitting = false;
                            if let Some(value) = this.result(result) {
                                match decode(value) {
                                    Ok(request) => {
                                        if this.modal_serial == modal.serial {
                                            this.load_review(request, None, window, cx);
                                        }
                                    }
                                    Err(e) => this.error = Some(e),
                                }
                            }
                        },
                    );
                } else {
                    self.load_review(request, None, window, cx);
                }
            }
            ModalKind::Change => {
                if body.is_empty() {
                    self.error = Some("Write a change request.".into());
                    return;
                }
                self.submitting = true;
                self.rpc("comment", json!({ "review": modal.request, "selection": modal.selection, "body": body, "digest": modal.digest }), window, cx, move |this, result, window, cx| {
                    this.submitting = false;
                    if let Some(value) = this.result(result) {
                        match decode::<ReviewState>(value) {
                            Ok(state) if this.review.as_ref().map(|r| &r.scope_key) == Some(&modal.tab) => {
                                let id = state.comments.last().map(|comment| comment.id.clone());
                                this.state = state;
                                this.threads_visible = true;
                                this.thread_filter = None;
                                this.thread_scroll_target = id;
                            },
                            Ok(_) => {}, Err(e) => { this.error = Some(e); return }
                        }
                        this.drafts.remove(&Self::draft_key(&modal));
                        if this.modal_serial == modal.serial {
                            this.prompt.update(cx, |input, cx| input.set_value("", window, cx));
                            this.close_modal(window, cx);
                            this.selection = None;
                            this.diff.update(cx, |diff, cx| diff.set_selected_lines(None, cx));
                        }
                    }
                });
            }
            ModalKind::Ask | ModalKind::Changes => {
                if modal.kind == ModalKind::Ask && body.is_empty() {
                    self.error = Some("Write a question.".into());
                    return;
                }
                if modal.kind == ModalKind::Changes && self.comment_ids.is_empty() {
                    self.error = Some("Select feedback to send.".into());
                    return;
                }
                if !self
                    .providers
                    .iter()
                    .any(|p| p.id == self.provider && p.available)
                {
                    self.error = Some(
                        "Install and sign in to the selected agent CLI, then restart Rev.".into(),
                    );
                    return;
                }
                self.submitting = true;
                let params = self.agent_request(&modal, &body);
                self.rpc(
                    "run",
                    params,
                    window,
                    cx,
                    move |this, result, window, cx| {
                        this.submitting = false;
                        if let Some(value) = this.result(result) {
                            match decode::<AgentRun>(value) {
                                Ok(run) => {
                                    if this.modal_serial == modal.serial {
                                        this.close_modal(window, cx);
                                        this.drafts.remove(&Self::draft_key(&modal));
                                        this.prompt.update(cx, |input, cx| {
                                            input.set_value("", window, cx)
                                        });
                                    }
                                    this.track_thread_run(
                                        modal.tab,
                                        modal.request,
                                        run,
                                        window,
                                        cx,
                                    );
                                }
                                Err(e) => this.error = Some(e),
                            }
                        }
                    },
                );
            }
            ModalKind::Commits => self.apply_commit_selection(window, cx),
            ModalKind::Run | ModalKind::Details => {}
        }
        cx.notify();
    }

    fn agent_request(&self, modal: &Modal, body: &str) -> Value {
        let comment_ids: Vec<_> = if modal.kind == ModalKind::Changes {
            self.comment_ids.iter().collect()
        } else {
            Vec::new()
        };
        json!({ "review": modal.request, "provider": self.provider, "mode": if modal.kind == ModalKind::Changes { "apply" } else { "ask" },
            "question": body, "selection": modal.selection, "selectionDigest": modal.digest, "commentIds": comment_ids, "newSession": self.new_session })
    }

    fn preview_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(modal) = self.modal.clone() else {
            return;
        };
        let params = self.agent_request(&modal, &self.prompt.read(cx).value());
        self.submitting = true;
        self.rpc("prompt", params, window, cx, move |this, result, _, _| {
            this.submitting = false;
            if this.modal_serial != modal.serial {
                return;
            }
            if let Some(value) = this.result(result) {
                this.preview = value["prompt"].as_str().map(str::to_string);
            }
        });
    }

    fn open_run(&mut self, scope: String, window: &mut Window, cx: &mut Context<Self>) {
        self.open_modal(ModalKind::Run, None, None, "agent-output", window, cx);
        if let Some(modal) = &mut self.modal {
            if let Some(request) = self.run_requests.get(&scope) {
                modal.request = request.clone();
            }
            modal.tab = scope;
        }
    }

    fn poll_run(&mut self, tab: String, id: String, window: &Window, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let polled_id = id.clone();
        let delay = cx.background_executor().timer(Duration::from_millis(500));
        let task = cx.background_executor().spawn(async move {
            delay.await;
            backend.call("runStatus", json!({ "id": id }))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await.and_then(decode::<AgentRun>);
            let _ = this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(run) => {
                        let running = run.is_active();
                        let id = run.id.clone();
                        this.remember_thread(&tab, &run, cx);
                        this.runs.insert(id.clone(), run);
                        if running {
                            this.poll_run(tab, id, window, cx);
                        }
                    }
                    Err(e) => {
                        if let Some(run) = this.runs.get_mut(&polled_id) {
                            run.status = "failed".into();
                            run.events.push(AgentEvent {
                                kind: "error".into(),
                                text: e,
                            });
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn cancel_run(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(run) = self.modal.as_ref().and_then(|m| self.runs.get(&m.tab)) {
            self.rpc(
                "cancel",
                json!({ "id": run.id }),
                window,
                cx,
                |this, result, _, _| {
                    this.result(result);
                },
            );
        }
    }

    fn approve_run(
        &mut self,
        approval_id: String,
        allow: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(run) = self.modal.as_ref().and_then(|m| self.runs.get(&m.tab)) {
            self.rpc(
                "approve",
                json!({ "id": run.id, "approvalId": approval_id, "allow": allow }),
                window,
                cx,
                |this, result, _, _| {
                    this.result(result);
                },
            );
        }
    }

    fn delete_comment(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(modal) = self.modal.clone() else {
            return;
        };
        self.submitting = true;
        self.rpc(
            "deleteComment",
            json!({ "review": modal.request, "id": id }),
            window,
            cx,
            move |this, result, _, _| {
                this.submitting = false;
                if let Some(value) = this.result(result)
                    && this
                        .review
                        .as_ref()
                        .is_some_and(|r| r.scope_key == modal.tab)
                {
                    match decode(value) {
                        Ok(state) => this.state = state,
                        Err(e) => this.error = Some(e),
                    }
                    this.comment_ids.remove(&id);
                    this.preview = None;
                }
            },
        );
    }
}
