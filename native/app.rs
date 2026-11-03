#![allow(dead_code, unused_imports)]
use crate::diff::*;
use crate::{
    backend::{Backend, Reply},
    model::*,
};
use gpui_kit::component::{
    Disableable, FocusTrapElement, Selectable, Sizable,
    button::{Button, ButtonVariants},
    input::{Input, InputState},
};
use gpui_kit::prelude::*;
use gpui_kit::*;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

mod view;
gpui_kit::actions!(
    rev,
    [
        Quit,
        OpenProject,
        Refresh,
        CloseModal,
        Submit,
        NextFile,
        PreviousFile,
        MarkViewed
    ]
);

#[derive(Clone, Copy, PartialEq)]
enum ModalKind {
    Open,
    Review,
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

pub struct Rev {
    backend: Option<Backend>,
    workspace: Workspace,
    review: Option<Review>,
    state: ReviewState,
    file: Option<String>,
    patch: FilePatch,
    diff: Entity<DiffState>,
    selection: Option<Selection>,
    filter: Entity<InputState>,
    fields: Vec<Entity<InputState>>,
    focus: FocusHandle,
    modal_focus: FocusHandle,
    modal: Option<Modal>,
    modal_serial: u64,
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
        let fields = (0..3)
            .map(|_| cx.new(|cx| InputState::new(window, cx)))
            .collect();

        let diff = cx.new(|cx| DiffState::new([], cx));
        let subscriptions = vec![
            cx.observe(&filter, |_, _, cx| cx.notify()),
            cx.subscribe_in(&diff, window, |this, _, event: &DiffEvent, _, cx| {
                match event {
                    DiffEvent::SelectionEnded(range) | DiffEvent::SelectionChanged(Some(range)) => {
                        this.selection = Selection::from_range(range);
                        if this.selection.is_none() {
                            this.error = Some("Select lines on one side of the diff.".into());
                        }
                    }
                    DiffEvent::SelectionChanged(None) => this.selection = None,
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
            filter,
            fields,
            focus: cx.focus_handle(),
            modal_focus: cx.focus_handle(),
            modal: None,
            modal_serial: 0,
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
                    match decode(value["workspace"].clone()) {
                        Ok(workspace) => this.workspace = workspace,
                        Err(error) => {
                            this.error = Some(error);
                            return;
                        }
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
        if self.loading || self.workspace_busy || self.patch_loading {
            return;
        }
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
        self.file_epoch += 1;
        let epoch = self.file_epoch;
        self.file = Some(path.clone());
        self.patch_loading = true;
        self.selection = None;
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

    fn close_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.modal = None;

        self.modal_serial += 1;
        self.selection = None;
        self.diff
            .update(cx, |diff, cx| diff.set_selected_lines(None, cx));
        self.focus.focus(window, cx);
        cx.notify();
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

        self.modal = Some(modal);
        self.modal_focus.focus(window, cx);
        if matches!(kind, ModalKind::Open | ModalKind::Review) {
            self.fields[0].update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    fn open_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open repository".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(path) = paths.first()
            {
                let request = ReviewRequest {
                    repo: path.to_string_lossy().into_owned(),
                    ..Default::default()
                };
                let _ = this.update_in(cx, |this, window, cx| {
                    this.load_review(request, None, window, cx);
                });
            }
        })
        .detach();
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
        }
        cx.notify();
    }
}
