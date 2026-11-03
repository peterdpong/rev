use crate::diff::*;
use crate::{
    backend::{Backend, Reply},
    client_state::{self, ClientState},
    model::*,
    workspace_state::WorkspaceState,
};
use gpui_kit::prelude::*;
use gpui_kit::*;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::PathBuf,
};

mod commits;
use commits::CommitPicker;
mod controls;
mod diff_view;
mod file_list;
mod file_tree;
mod modal;
mod view;
mod working_tree;
use diff_view::{DiffView, DiffViewEvent, DiffViewStatus};
use file_list::{FileList, FileListEvent};
use modal::{Modal, ModalKind};
gpui_kit::actions!(
    rev,
    [
        Quit,
        OpenProject,
        Refresh,
        CloseModal,
        Submit,
        ToggleSidebar,
        NextFile,
        PreviousFile,
        MarkViewed
    ]
);

pub fn bind_shortcuts(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-o", OpenProject, Some("Rev")),
        KeyBinding::new("cmd-r", Refresh, Some("Rev")),
        KeyBinding::new("cmd-b", ToggleSidebar, Some("Rev")),
        KeyBinding::new("alt-down", NextFile, Some("Rev")),
        KeyBinding::new("alt-up", PreviousFile, Some("Rev")),
        KeyBinding::new("cmd-shift-v", MarkViewed, Some("Rev")),
        KeyBinding::new("escape", CloseModal, Some("PatchModal")),
        KeyBinding::new("cmd-enter", Submit, Some("PatchModal")),
        KeyBinding::new("escape", CloseModal, Some("Rev")),
        KeyBinding::new("cmd-enter", Submit, Some("PatchModal > Input")),
        KeyBinding::new("escape", CloseModal, Some("PatchModal > Input")),
    ]);
}

pub struct Rev {
    backend: Option<Backend>,
    workspace: WorkspaceState,
    data_dir: PathBuf,
    sidebar_collapsed: bool,
    sidebar_focus: FocusHandle,
    review: Option<Review>,
    review_snapshot_id: Option<String>,
    state: ReviewState,
    file: Option<String>,
    patch: FilePatch,
    diff: Entity<DiffView>,
    file_list: Entity<FileList>,
    commit_picker: CommitPicker,
    focus: FocusHandle,
    modal_focus: FocusHandle,
    modal: Option<Modal>,
    modal_serial: u64,
    folder_picker_open: bool,
    loading: bool,
    patch_loading: bool,
    submitting: bool,
    workspace_busy: bool,
    review_epoch: u64,
    file_epoch: u64,
    error: Option<String>,
    notice: Option<String>,
    working_tree_poll_inflight: bool,
    working_tree_refreshing: bool,
    working_tree_revisions: HashMap<String, String>,
    _working_tree_poll: Option<Task<()>>,
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
        let (backend, mut error) = match backend {
            Ok(b) => (Some(b), None),
            Err(e) => (None, Some(e)),
        };

        let data_dir = client_state::data_dir();
        let workspace = match ClientState::load(&data_dir) {
            Ok(client) => client.workspace,
            Err(e) => {
                error = Some(format!("Could not load saved tabs: {e}"));
                WorkspaceState::default()
            }
        };

        let file_list = cx.new(|cx| FileList::new(window, cx));
        let diff = cx.new(DiffView::new);
        let subscriptions = Self::subscribe_to_views(&file_list, &diff, window, cx);

        let mut app = Self {
            backend,
            workspace,
            data_dir,
            sidebar_collapsed: false,
            sidebar_focus: cx.focus_handle(),
            review: None,
            review_snapshot_id: None,
            state: ReviewState::default(),

            file: None,
            patch: FilePatch::default(),
            diff,
            file_list,
            commit_picker: CommitPicker::default(),

            focus: cx.focus_handle(),
            modal_focus: cx.focus_handle(),
            modal: None,
            modal_serial: 0,
            folder_picker_open: false,

            loading: false,
            patch_loading: false,
            submitting: false,
            workspace_busy: false,
            review_epoch: 0,
            file_epoch: 0,

            error,
            notice: None,

            working_tree_poll_inflight: false,
            working_tree_refreshing: false,
            working_tree_revisions: HashMap::new(),
            _working_tree_poll: None,

            _subscriptions: subscriptions,
        };

        app.focus.focus(window, cx);

        if app.backend.is_some() {
            let tab = app.workspace.active_tab().cloned();

            if let Some(request) = initial {
                app.load_review(request, None, window, cx);
            } else if let Some(tab) = tab {
                app.load_review(tab.request, tab.file, window, cx);
            }

            app.start_working_tree_updates(window, cx);
        }

        app
    }

    fn subscribe_to_views(
        file_list: &Entity<FileList>,
        diff: &Entity<DiffView>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<Subscription> {
        vec![
            cx.subscribe_in(file_list, window, |this, _, event, window, cx| {
                let FileListEvent::Selected(path) = event;
                if !this.loading {
                    this.load_file(path.clone(), window, cx);
                }
            }),
            cx.subscribe_in(diff, window, |this, _, event, window, cx| {
                let DiffViewEvent::ToggleViewed = event;
                this.mark_viewed(window, cx);
            }),
        ]
    }

    fn rpc(
        &mut self,
        method: &'static str,
        params: Value,
        window: &Window,
        cx: &mut Context<Self>,
        done: impl FnOnce(&mut Self, Reply, &mut Window, &mut Context<Self>) + 'static,
    ) {
        self.rpc_quiet(
            method,
            params,
            window,
            cx,
            move |this, result, window, cx| {
                done(this, result, window, cx);
                cx.notify();
            },
        );
    }

    fn rpc_quiet(
        &mut self,
        method: &'static str,
        params: Value,
        window: &Window,
        cx: &mut Context<Self>,
        done: impl FnOnce(&mut Self, Reply, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let Some(backend) = self.backend.clone() else {
            let error = "The local service is unavailable. Restart Rev.";
            if self.error.as_deref() != Some(error) {
                self.error = Some(error.into());
                cx.notify();
            }
            return;
        };
        let task = cx
            .background_executor()
            .spawn(async move { backend.call(method, params) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                done(this, result, window, cx);
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

        let request = request.for_tab();
        self.loading = true;
        self.error = None;
        self.notice = None;

        // Responses from earlier loads are now stale. Bumping the file epoch also discards
        // any pending patch, so its loading flag must clear here.
        self.review_epoch += 1;
        self.file_epoch += 1;
        self.patch_loading = false;
        let epoch = self.review_epoch;

        // Patches are fetched per file by `load_file`, so skip them here.
        let mut params = json!(request);
        params["patches"] = json!(false);

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
                let review = decode::<Review>(value["review"].clone());
                let state = decode::<ReviewState>(value["state"].clone());
                let (mut review, state) = match (review, state) {
                    (Ok(review), Ok(state)) => (review, state),
                    (Err(e), _) | (_, Err(e)) => {
                        this.error = Some(e);
                        return;
                    }
                };
                this.review_snapshot_id = value["snapshotId"].as_str().map(str::to_owned);

                // Committed tabs keep their branch; working trees continue following HEAD.
                if review.request.mode.as_deref() != Some("working")
                    && review.request.head.as_deref() == Some("HEAD")
                {
                    review.request.head = Some(if review.branch == "detached HEAD" {
                        review.branch_head_sha.clone()
                    } else {
                        review.branch.clone()
                    });
                }

                this.workspace.remember_review(&review, &request);
                this.save_workspace();
                this.working_tree_revisions.remove(&review.request.repo);

                // Open the requested file, else the tab's last file, if it's still in the
                // review. Otherwise open the first file.
                let previous = file.or_else(|| this.workspace.active_tab()?.file.clone());
                let selected = previous
                    .filter(|path| review.files.iter().any(|f| &f.path == path))
                    .or_else(|| review.files.first().map(|f| f.path.clone()));

                // Show the new review with no diff until the selected file loads.
                this.state = state;
                this.file_list.update(cx, |list, cx| {
                    list.set_review(&review.request.repo, &review.files, window, cx)
                });
                this.review = Some(review);
                this.file = None;
                this.patch = FilePatch::default();
                this.diff.update(cx, |diff, cx| diff.set_files([], cx));

                if let Some(path) = selected {
                    this.load_file(path, window, cx);
                }
                this.load_pull_request(window, cx);
            },
        );
        cx.notify();
    }

    fn load_pull_request(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(review) = &self.review else { return };
        if review.pull_request.is_some() {
            return;
        }
        let epoch = self.review_epoch;
        // Optional GitHub details load separately so they never delay the diff.
        self.rpc(
            "pullRequest",
            json!(review.request),
            window,
            cx,
            move |this, result, _, _| {
                if this.review_epoch == epoch
                    && let Ok(value) = result
                    && let Ok(pr) = decode::<Option<PullRequest>>(value)
                    && let Some(review) = &mut this.review
                {
                    review.pull_request = pr;
                }
            },
        );
    }

    fn load_file(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(review) = self.review.as_ref() else {
            return;
        };
        let request = review.request.clone();
        self.file_list
            .update(cx, |list, cx| list.select_file(&path, cx));

        // Responses from earlier file loads are now stale.
        self.file_epoch += 1;
        let epoch = self.file_epoch;

        // Clear the previous file's diff while this one loads.
        self.file = Some(path.clone());
        self.patch_loading = true;
        self.patch = FilePatch::default();
        self.error = None;
        self.notice = None;
        self.diff.update(cx, |diff, cx| diff.set_files([], cx));

        self.rpc(
            "patch",
            json!({ "review": request, "path": path, "snapshotId": self.review_snapshot_id }),
            window,
            cx,
            move |this, result, _, cx| {
                if this.file_epoch != epoch {
                    return;
                }
                this.patch_loading = false;

                let Some(value) = this.result(result) else {
                    return;
                };
                let patch = match decode::<FilePatch>(value) {
                    Ok(patch) => patch,
                    Err(e) => {
                        this.error = Some(e);
                        return;
                    }
                };

                // Large and binary patches show a notice instead of a diff.
                this.notice = patch.notice();
                if this.notice.is_none() {
                    match DiffFile::parse(&patch.patch) {
                        Ok(files) => this.diff.update(cx, |diff, cx| diff.set_files(files, cx)),
                        Err(e) => this.error = Some(format!("Could not display this patch: {e}")),
                    }
                }
                this.patch = patch;

                // Remember the file so the tab reopens on it.
                if let Some(id) = this.workspace.active_tab_id.clone() {
                    this.workspace.set_file(&id, path);
                    this.save_workspace();
                }
            },
        );
        cx.notify();
    }

    fn toggle_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
        if self.sidebar_collapsed && self.sidebar_focus.contains_focused(window, cx) {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }

    fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(review) = &self.review {
            let request = self
                .workspace
                .active_tab()
                .map(|tab| tab.request.clone())
                .unwrap_or_else(|| review.request.clone());
            self.load_review(request, self.file.clone(), window, cx);
        }
    }

    fn save_workspace(&mut self) {
        let client = ClientState {
            workspace: self.workspace.clone(),
        };
        if let Err(error) = client.save(&self.data_dir) {
            self.error = Some(format!("Could not save tabs: {error}"));
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
        match action["type"].as_str() {
            Some("collapseProject") => {
                if let (Some(repo), Some(collapsed)) =
                    (action["repo"].as_str(), action["collapsed"].as_bool())
                {
                    self.workspace.collapse_project(repo, collapsed);
                }
            }
            Some("closeTab") => {
                if let Some(id) = action["id"].as_str() {
                    self.workspace.close_tab(id);
                }
            }
            Some("removeProject") => {
                if let Some(repo) = action["repo"].as_str() {
                    self.workspace.remove_project(repo);
                    self.file_list
                        .update(cx, |list, _| list.forget_repository(repo));
                    self.working_tree_revisions.remove(repo);
                }
            }
            _ => return,
        }
        self.save_workspace();
        if navigate {
            if let Some(tab) = self.workspace.active_tab().cloned() {
                self.load_review(tab.request, tab.file, window, cx);
            } else {
                self.review = None;
                self.review_snapshot_id = None;
                self.file_list
                    .update(cx, |list, cx| list.set_review("", &[], window, cx));
                self.state = ReviewState::default();
                self.file = None;
                self.patch = FilePatch::default();
                self.file_epoch += 1;
                self.patch_loading = false;
                self.diff.update(cx, |diff, cx| diff.set_files([], cx));
            }
        }
        cx.notify();
    }

    fn navigate_file(&mut self, direction: isize, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal.is_some() || self.loading {
            return;
        }
        let next = self.file_list.update(cx, |list, _| {
            list.adjacent_file(self.file.as_deref(), direction)
        });
        if let Some(path) = next {
            self.load_file(path, window, cx);
        }
    }

    fn mark_viewed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.patch_loading || self.loading || self.submitting || self.patch.digest.is_empty() {
            return;
        }
        let (Some(review), Some(path)) = (self.review.clone(), self.file.clone()) else {
            return;
        };
        let epoch = self.review_epoch;
        let viewed = !self.state.viewed.contains_key(&path);
        self.file_epoch += 1;
        self.submitting = true;
        self.rpc("viewed", json!({ "review": review.request, "path": path, "viewed": viewed, "digest": self.patch.digest }), window, cx,
            move |this, result, _, _| {
                this.submitting = false;
                if this.review_epoch != epoch { return; }
                if let Some(value) = this.result(result) { match decode(value) { Ok(state) => this.state = state, Err(e) => this.error = Some(e) } }
            });
    }

    fn pick_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.folder_picker_open
            || self.loading
            || self.patch_loading
            || self.submitting
            || self.workspace_busy
        {
            return;
        }
        if self.modal.is_some() {
            self.close_modal(window, cx);
        }
        self.folder_picker_open = true;
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open repository".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = paths.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.folder_picker_open = false;
                match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.first() {
                            this.load_review(
                                ReviewRequest::working_tree(path.to_string_lossy().to_string()),
                                None,
                                window,
                                cx,
                            );
                        }
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => {
                        this.error = Some(format!("Could not open the folder picker: {error}"));
                        cx.notify();
                    }
                    Err(_) => {
                        this.error =
                            Some("The folder picker closed unexpectedly. Try again.".into());
                        cx.notify();
                    }
                }
            });
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

}
