use crate::{
    backend::{Backend, Reply},
    model::*,
};
use gpui_kit::component::{
    Disableable, FocusTrapElement, Selectable, Sizable,
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
    collections::{HashMap, HashSet},
    time::Duration,
};

#[cfg(feature = "ui-tests")]
pub mod smoke;
mod view;
gpui_kit::actions!(
    patch,
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
        NextFile,
        PreviousFile,
        MarkViewed
    ]
);

pub fn bind_shortcuts(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-o", OpenProject, Some("Patch")),
        KeyBinding::new("cmd-r", Refresh, Some("Patch")),
        KeyBinding::new("cmd-k", AskReview, Some("Patch")),
        KeyBinding::new("alt-down", NextFile, Some("Patch")),
        KeyBinding::new("alt-up", PreviousFile, Some("Patch")),
        KeyBinding::new("cmd-shift-v", MarkViewed, Some("Patch")),
        KeyBinding::new("escape", CloseModal, Some("PatchModal")),
        KeyBinding::new("cmd-enter", Submit, Some("PatchModal")),
        KeyBinding::new("cmd-1", PromptAsk, Some("Patch")),
        KeyBinding::new("cmd-2", PromptChange, Some("Patch")),
        KeyBinding::new("ctrl-tab", TogglePromptMode, Some("PatchPrompt")),
        KeyBinding::new("ctrl-shift-tab", TogglePromptMode, Some("PatchPrompt")),
        KeyBinding::new("escape", CloseModal, Some("Patch")),
        KeyBinding::new("cmd-enter", Submit, Some("PatchPrompt")),
        KeyBinding::new("cmd-enter", Submit, Some("PatchPrompt > Input")),
        KeyBinding::new("cmd-enter", Submit, Some("PatchModal > Input")),
        KeyBinding::new("escape", CloseModal, Some("PatchPrompt > Input")),
        KeyBinding::new("escape", CloseModal, Some("PatchModal > Input")),
    ]);
}

#[derive(Clone, Copy, PartialEq)]
enum ModalKind {
    Open,
    Clone,
    Review,
    Ask,
    Change,
    Changes,
    Run,
}
#[derive(Clone)]
struct Modal {
    kind: ModalKind,
    request: ReviewRequest,
    tab: String,
    selection: Option<Selection>,
    digest: String,
    serial: u64,
}

pub struct Patch {
    backend: Option<Backend>,
    workspace: Workspace,
    review: Option<Review>,
    state: ReviewState,
    file: Option<String>,
    patch: FilePatch,
    diff: Entity<DiffState>,
    selection: Option<Selection>,
    selecting: bool,
    selection_menu: Point<Pixels>,
    prompt_mode: ModalKind,
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
    runs: HashMap<String, AgentRun>,
    comment_ids: HashSet<String>,
    loading: bool,
    patch_loading: bool,
    submitting: bool,
    workspace_busy: bool,
    review_epoch: u64,
    file_epoch: u64,
    error: Option<String>,
    notice: Option<String>,
    split: bool,
    wrap: bool,
    _subscriptions: Vec<Subscription>,
}

fn decode<T: DeserializeOwned>(value: Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|e| format!("Invalid local service response: {e}"))
}

impl Patch {
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
        let fields = (0..3)
            .map(|_| cx.new(|cx| InputState::new(window, cx)))
            .collect();
        let prompt = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(5)
                .placeholder("Write a question or change request…")
        });
        let diff = cx.new(|cx| DiffState::new([], cx));
        let subscriptions = vec![
            cx.observe(&filter, |_, _, cx| cx.notify()),
            cx.observe_in(&diff, window, |this, diff, window, cx| {
                let state = diff.read(cx);
                if state.selected_lines().is_some() {
                    return;
                } // Gutter/keyboard events own this range.
                let selecting = state.is_selecting_text(cx);
                let range = state.selected_text_lines(cx);
                if selecting {
                    this.selecting = true;
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
                }
                cx.notify();
            }),
            cx.subscribe_in(&diff, window, |this, _, event: &DiffEvent, window, cx| {
                match event {
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
            review: None,
            state: ReviewState::default(),
            file: None,
            patch: FilePatch::default(),
            diff,
            selection: None,
            selecting: false,
            selection_menu: Point::default(),
            prompt_mode: ModalKind::Ask,
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
            runs: HashMap::new(),
            comment_ids: HashSet::new(),
            loading: false,
            patch_loading: false,
            submitting: false,
            workspace_busy: false,
            review_epoch: 0,
            file_epoch: 0,
            error,
            notice: None,
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
            self.error = Some("The local service is unavailable. Restart Patch.".into());
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
        if self.loading || self.workspace_busy || self.patch_loading {
            return;
        }
        self.close_selection_prompt(window, cx);
        self.loading = true;
        self.error = None;
        self.notice = None;
        self.review_epoch += 1;
        self.file_epoch += 1;
        let epoch = self.review_epoch;
        let mut params = json!(request);
        params["remember"] = json!(true);
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
                        // Use the persisted symbolic ref, never allow HEAD to follow another tab's checkout.
                        if let Some(tab) = workspace.active() {
                            review.request = tab.request.clone();
                        }
                        let selected = file
                            .or_else(|| workspace.active().and_then(|t| t.file.clone()))
                            .filter(|file| review.files.iter().any(|f| &f.path == file))
                            .or_else(|| review.files.first().map(|f| f.path.clone()));
                        this.workspace = workspace;
                        this.state = state;
                        this.review = Some(review);
                        this.file = None;
                        this.patch = FilePatch::default();
                        this.selection = None;
                        this.diff.update(cx, |diff, cx| diff.set_files([], cx));
                        this.filter
                            .update(cx, |input, cx| input.set_value("", window, cx));
                        if let Some(path) = selected {
                            this.load_file(path, window, cx);
                        }
                    }
                    Err(e) => this.error = Some(e),
                }
            },
        );
        cx.notify();
    }

    fn load_file(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(review) = self.review.clone() else {
            return;
        };
        self.close_selection_prompt(window, cx);
        self.file_epoch += 1;
        let epoch = self.file_epoch;
        self.file = Some(path.clone());
        self.patch_loading = true;
        self.selection = None;
        self.selecting = false;
        self.patch = FilePatch::default();
        self.error = None;
        self.notice = None;
        self.diff.update(cx, |diff, cx| diff.set_files([], cx));
        self.rpc("patch", json!({ "review": review.request, "path": path }), window, cx, move |this, result, window, cx| {
            if this.file_epoch != epoch { return; }
            this.patch_loading = false;
            let Some(value) = this.result(result) else { return };
            match decode::<FilePatch>(value) {
                Ok(patch) => {
                    if patch.too_large { this.notice = Some("This patch exceeds the 2 MB inline limit. Use patch diff in the terminal.".into()); }
                    else if patch.patch == "Binary file (untracked)" { this.notice = Some("Binary file".into()); }
                    else {
                        match DiffFile::parse(&patch.patch) {
                            Ok(files) => this.diff.update(cx, |diff, cx| diff.set_files(files, cx)),
                            Err(e) => this.error = Some(format!("Could not display this patch: {e}")),
                        }
                    }
                    this.patch = patch;
                    this.rpc("workspaceAction", json!({ "type": "setFile", "id": review.key, "file": path }), window, cx, |this, result, _, _| { this.result(result); });
                }
                Err(e) => this.error = Some(e),
            }
        });
        cx.notify();
    }

    fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(review) = &self.review {
            self.load_review(review.request.clone(), self.file.clone(), window, cx);
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
                                this.file_epoch += 1;
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
        self.load_file(files[next].path.clone(), window, cx);
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
        if self.patch_loading || self.loading || self.patch.digest.is_empty() {
            return;
        }
        let (Some(review), Some(path)) = (self.review.clone(), self.file.clone()) else {
            return;
        };
        let epoch = self.review_epoch;
        let viewed = !self.state.viewed.contains_key(&path);
        self.rpc("viewed", json!({ "review": review.request, "path": path, "viewed": viewed, "digest": self.patch.digest }), window, cx,
            move |this, result, _, _| {
                if this.review_epoch != epoch { return; }
                if let Some(value) = this.result(result) { match decode(value) { Ok(state) => this.state = state, Err(e) => this.error = Some(e) } }
            });
    }

    fn draft_key(modal: &Modal) -> String {
        format!(
            "{}:{}:{}",
            modal.tab,
            modal.kind as u8,
            modal
                .selection
                .as_ref()
                .map(Selection::label)
                .unwrap_or_default()
        )
    }
    fn close_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        let Some(review) = self.review.as_ref() else {
            return;
        };
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
                && m.tab == review.key
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
            tab: review.key.clone(),
            selection: Some(selection),
            digest: self.patch.digest.clone(),
            serial: self.modal_serial,
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
        // Preserve diff focus so Shift-selection and copying keep working.
        cx.notify();
    }

    fn is_selection_prompt(modal: &Modal) -> bool {
        matches!(modal.kind, ModalKind::Ask | ModalKind::Change) && modal.selection.is_some()
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
        if self.submitting || self.selecting || !Self::is_selection_prompt(modal) {
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.loading || self.patch_loading || self.submitting {
            return;
        }
        self.close_modal(window, cx);
        self.error = None;
        let mut request = self
            .review
            .as_ref()
            .map(|r| r.request.clone())
            .unwrap_or_default();
        if let Some(repo) = repo {
            request = ReviewRequest {
                repo,
                ..Default::default()
            };
        }
        let tab = self
            .review
            .as_ref()
            .map(|r| r.key.clone())
            .unwrap_or_default();
        let modal = Modal {
            kind,
            request: request.clone(),
            tab,
            selection,
            digest: self.patch.digest.clone(),
            serial: self.modal_serial,
        };
        let values: Vec<String> = match kind {
            ModalKind::Review => vec![
                request.repo.clone(),
                request.base.clone().unwrap_or_default(),
                "HEAD".into(),
            ],
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
        } else if matches!(kind, ModalKind::Open | ModalKind::Clone | ModalKind::Review) {
            self.fields[0].update(cx, |input, cx| input.focus(window, cx));
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
            json!({ "repo": request.repo }),
            window,
            cx,
            move |this, result, window, cx| {
                this.submitting = false;
                if this.result(result).is_some() && this.review_epoch == epoch {
                    this.refresh(window, cx);
                }
            },
        );
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.notify();
        if self.submitting {
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
            ModalKind::Open | ModalKind::Review => {
                if values[0].is_empty() {
                    self.error = Some("Choose a repository folder.".into());
                    return;
                }
                let request = ReviewRequest {
                    repo: values[0].clone(),
                    base: (!values[1].is_empty()).then(|| values[1].clone()),
                    head: (!values[2].is_empty()).then(|| values[2].clone()),
                    mode: Some("branch".into()),
                };
                if values[2].starts_with("https://github.com/")
                    || values[2].starts_with('#')
                    || values[2].parse::<u64>().is_ok()
                {
                    self.submitting = true;
                    self.rpc(
                        "pr",
                        json!({ "repo": request.repo, "value": values[2].trim_start_matches('#') }),
                        window,
                        cx,
                        move |this, result, window, cx| {
                            this.submitting = false;
                            if let Some(value) = this.result(result) {
                                match decode(value) {
                                    Ok(request) => {
                                        if this.modal_serial == modal.serial {
                                            this.close_modal(window, cx);
                                        }
                                        this.load_review(request, None, window, cx);
                                    }
                                    Err(e) => this.error = Some(e),
                                }
                            }
                        },
                    );
                } else {
                    self.close_modal(window, cx);
                    self.load_review(request, None, window, cx);
                }
            }
            ModalKind::Clone => {
                if values[0].is_empty() || values[1].is_empty() {
                    self.error = Some("Enter a Git URL and destination folder.".into());
                    return;
                }
                self.submitting = true;
                self.rpc(
                    "clone",
                    json!({ "url": values[0], "destination": values[1] }),
                    window,
                    cx,
                    move |this, result, window, cx| {
                        this.submitting = false;
                        if let Some(value) = this.result(result) {
                            if this.modal_serial == modal.serial {
                                this.close_modal(window, cx);
                            }
                            this.load_review(
                                ReviewRequest {
                                    repo: value["repo"].as_str().unwrap_or_default().into(),
                                    ..Default::default()
                                },
                                None,
                                window,
                                cx,
                            );
                        }
                    },
                );
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
                        match decode(value) { Ok(state) if this.review.as_ref().map(|r| &r.key) == Some(&modal.tab) => this.state = state, Ok(_) => {}, Err(e) => { this.error = Some(e); return } }
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
                        "Install and sign in to the selected agent CLI, then restart Patch.".into(),
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
                                    let id = run.id.clone();
                                    this.runs.insert(modal.tab.clone(), run);
                                    if this.modal_serial == modal.serial {
                                        this.modal = Some(Modal {
                                            kind: ModalKind::Run,
                                            ..modal.clone()
                                        });
                                        this.preview = None;
                                    }
                                    this.poll_run(modal.tab, id, window, cx);
                                }
                                Err(e) => this.error = Some(e),
                            }
                        }
                    },
                );
            }
            ModalKind::Run => {}
        }
        cx.notify();
    }

    fn agent_request(&self, modal: &Modal, body: &str) -> Value {
        json!({ "review": modal.request, "provider": self.provider, "mode": if modal.kind == ModalKind::Changes { "apply" } else { "ask" },
            "question": body, "selection": modal.selection, "selectionDigest": modal.digest, "commentIds": self.comment_ids.iter().collect::<Vec<_>>() })
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

    fn poll_run(&mut self, tab: String, id: String, window: &Window, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
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
                        let running = run.status == "running";
                        let id = run.id.clone();
                        this.runs.insert(tab.clone(), run);
                        if running {
                            this.poll_run(tab, id, window, cx);
                        } else if this.review.as_ref().is_some_and(|r| r.key == tab) {
                            this.refresh(window, cx);
                        }
                    }
                    Err(e) => {
                        if let Some(run) = this.runs.get_mut(&tab) {
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
                    && this.review.as_ref().is_some_and(|r| r.key == modal.tab)
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
