use crate::diff::*;
use crate::{
    backend::{Backend, Reply},
    client_state::{self, ClientState},
    model::*,
    workspace_state::WorkspaceState,
};
use gpui_kit::component::{
    FocusTrapElement, Sizable,
    input::{Input, InputState},
};
use gpui_kit::prelude::*;
use gpui_kit::*;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    path::PathBuf,
    rc::Rc,
};

mod file_tree;
mod view;
mod working_tree;
use file_tree::{EntryKind, FileTreeCache, TreeEntry};
mod commits;
mod motion;
mod review;
use commits::CommitPicker;
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
        KeyBinding::new("escape", CloseModal, Some("RevPopover")),
        KeyBinding::new("cmd-enter", Submit, Some("RevPopover")),
        KeyBinding::new("escape", CloseModal, Some("Rev")),
        KeyBinding::new("cmd-enter", Submit, Some("RevPopover > Input")),
        KeyBinding::new("escape", CloseModal, Some("RevPopover > Input")),
    ]);
}

#[derive(Clone, Copy, PartialEq)]
enum ModalKind {
    Review,
    Commits,
    Details,
}
#[derive(Clone)]
struct Modal {
    kind: ModalKind,
    request: ReviewRequest,
    serial: u64,
    anchor: Option<SharedString>,
    anchor_bounds: Rc<Cell<Bounds<Pixels>>>,
    local_branches: Vec<String>,
    branches_loading: bool,
    branches_error: Option<String>,
}

impl Modal {
    fn manual_request(&self, values: &[String]) -> ReviewRequest {
        ReviewRequest {
            repo: self.request.repo.clone(),
            base: (!values[0].is_empty()).then(|| values[0].clone()),
            head: (!values[1].is_empty()).then(|| values[1].clone()),
            mode: Some("branch".into()),
            commit_range: None,
        }
    }

    fn local_branch_request(&self, branch: &str) -> ReviewRequest {
        ReviewRequest {
            repo: self.request.repo.clone(),
            head: Some(format!("refs/heads/{branch}")),
            mode: Some("branch".into()),
            ..Default::default()
        }
    }
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
    diff: Entity<DiffState>,
    selection: Option<Selection>,
    selecting: bool,
    selection_menu: Point<Pixels>,
    commit_picker: CommitPicker,
    filter: Entity<InputState>,
    collapsed_file_folders: HashMap<String, HashSet<String>>,
    filtered_file_folders: HashSet<String>,
    file_tree_filter: String,
    file_tree: FileTreeCache,
    file_scroll: UniformListScrollHandle,
    #[cfg(test)]
    file_rows_rendered: std::cell::RefCell<Vec<String>>,
    fields: Vec<Entity<InputState>>,
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
    split: bool,
    wrap: bool,
    working_tree_poll_inflight: bool,
    working_tree_refreshing: bool,
    working_tree_revisions: HashMap<String, String>,
    _working_tree_poll: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

fn decode<T: DeserializeOwned>(value: Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|e| format!("Invalid local service response: {e}"))
}

#[cfg(test)]
mod modal_tests {
    use super::{Modal, ModalKind};
    use crate::model::{CommitRange, ReviewRequest};

    fn repo_modal() -> Modal {
        Modal {
            kind: ModalKind::Review,
            request: ReviewRequest {
                repo: "/selected-repo".into(),
                base: Some("old-base".into()),
                head: Some("old-branch".into()),
                mode: Some("working".into()),
                commit_range: Some(CommitRange {
                    start: "a".into(),
                    end: "b".into(),
                }),
            },
            serial: 1,
            anchor: None,
            anchor_bounds: Default::default(),
            local_branches: vec!["feature".into()],
            branches_loading: false,
            branches_error: None,
        }
    }

    #[test]
    fn manual_diff_uses_the_repo_context_and_explicit_comparison() {
        let modal = repo_modal();
        let values = ["custom-base".into(), "#123".into()];
        let request = modal.manual_request(&values);
        assert_eq!(request.repo, "/selected-repo");
        assert_eq!(request.base.as_deref(), Some("custom-base"));
        assert_eq!(request.head.as_deref(), Some("#123"));
        assert_eq!(request.mode.as_deref(), Some("branch"));
        assert_eq!(request.commit_range, None);
    }

    #[test]
    fn local_branch_opens_independently_of_manual_fields_and_previous_scope() {
        let modal = repo_modal();
        let request = modal.local_branch_request("feature");
        assert_eq!(
            request,
            ReviewRequest {
                repo: "/selected-repo".into(),
                head: Some("refs/heads/feature".into()),
                mode: Some("branch".into()),
                ..Default::default()
            }
        );
        let defaults = modal.manual_request(&[String::new(), String::new()]);
        assert_eq!(defaults.base, None);
        assert_eq!(defaults.head, None);
        assert_eq!(defaults.mode.as_deref(), Some("branch"));
    }
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
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Find a file…"));
        let fields = ["Auto-detect base", "Branch, #123 or PR URL (default: HEAD)"]
            .into_iter()
            .map(|placeholder| cx.new(|cx| InputState::new(window, cx).placeholder(placeholder)))
            .collect();

        let diff = cx.new(|cx| DiffState::new([], cx));
        let subscriptions = vec![
            cx.observe(&filter, |this, filter, cx| {
                let value = filter.read(cx).value().to_string();
                if value != this.file_tree_filter {
                    this.filtered_file_folders.clear();
                    this.file_tree.set_filter(&value);
                    this.file_tree.invalidate_rows();
                    this.file_scroll.scroll_to_item(0, ScrollStrategy::Top);
                    this.file_tree_filter = value;
                    cx.notify();
                }
            }),
            cx.observe_in(&diff, window, |this, diff, window, cx| {
                let state = diff.read(cx);
                if state.selected_lines().is_some() {
                    return;
                } // Gutter/keyboard events own this range.
                let previous = (this.selecting, this.selection.clone());
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
                }
                if previous != (this.selecting, this.selection.clone()) {
                    cx.notify();
                }
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
            selection: None,
            collapsed_file_folders: HashMap::new(),
            filtered_file_folders: HashSet::new(),
            file_tree_filter: String::new(),
            file_tree: FileTreeCache::default(),
            file_scroll: UniformListScrollHandle::default(),
            #[cfg(test)]
            file_rows_rendered: Default::default(),
            selecting: false,
            selection_menu: Point::default(),
            commit_picker: CommitPicker::default(),
            filter,
            fields,
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
            split: false,
            wrap: false,
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

    /// Background checks opt in to redraws only when user-visible state changes.
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
        if self.loading || self.workspace_busy || self.patch_loading {
            return;
        }
        let request = request.for_tab();
        self.loading = true;
        self.error = None;
        self.notice = None;
        self.review_epoch += 1;
        self.file_epoch += 1;
        let epoch = self.review_epoch;
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
                let parsed = decode::<Review>(value["review"].clone()).and_then(|review| {
                    Ok((review, decode::<ReviewState>(value["state"].clone())?))
                });
                match parsed {
                    Ok((mut review, state)) => {
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
                        let selected = file
                            .or_else(|| this.workspace.active_tab().and_then(|t| t.file.clone()))
                            .filter(|file| review.files.iter().any(|f| &f.path == file))
                            .or_else(|| review.files.first().map(|f| f.path.clone()));
                        this.state = state;
                        this.file_tree_filter.clear();
                        this.filtered_file_folders.clear();
                        this.file_tree.set_filter("");
                        this.file_tree.set_files(&review.files);
                        this.file_tree.invalidate_rows();
                        this.file_scroll = UniformListScrollHandle::default();
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
                        this.load_pull_request(window, cx);
                    }
                    Err(e) => this.error = Some(e),
                }
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
        let mut revealed = false;
        if let Some(collapsed) = self.collapsed_file_folders.get_mut(&review.request.repo) {
            revealed |= file_tree::reveal_file(collapsed, &path);
        }
        revealed |= file_tree::reveal_file(&mut self.filtered_file_folders, &path);
        if revealed {
            self.file_tree.invalidate_rows();
        }
        if let Some(index) = self
            .file_tree_entries()
            .iter()
            .position(|entry| entry.path == path)
        {
            self.file_scroll
                .scroll_to_item(index, ScrollStrategy::Nearest);
        }
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
                match decode::<FilePatch>(value) {
                    Ok(patch) => {
                        if patch.too_large {
                            this.notice = Some("This patch exceeds the 2 MB inline limit.".into());
                        } else if patch.patch == "Binary file (untracked)" {
                            this.notice = Some("Binary file".into());
                        } else {
                            match DiffFile::parse(&patch.patch) {
                                Ok(files) => {
                                    this.diff.update(cx, |diff, cx| diff.set_files(files, cx))
                                }
                                Err(e) => {
                                    this.error = Some(format!("Could not display this patch: {e}"))
                                }
                            }
                        }
                        this.patch = patch;
                        if let Some(id) = this.workspace.active_tab_id.clone() {
                            this.workspace.set_file(&id, path);
                            this.save_workspace();
                        }
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
                    self.collapsed_file_folders.remove(repo);
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
                self.file_tree.set_files(&[]);
                self.state = ReviewState::default();
                self.file = None;
                self.selection = None;
                self.patch = FilePatch::default();
                self.file_epoch += 1;
                self.diff.update(cx, |diff, cx| diff.set_files([], cx));
            }
        }
        cx.notify();
    }

    fn navigate_file(&mut self, direction: isize, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal.is_some() || self.loading {
            return;
        }
        let entries = self.file_tree_entries();
        let files: Vec<_> = entries
            .iter()
            .filter_map(|entry| match &entry.kind {
                EntryKind::File(file) => Some(file),
                EntryKind::Folder { .. } => None,
            })
            .collect();
        if files.is_empty() {
            return;
        }
        let index = files
            .iter()
            .position(|f| Some(&f.path) == self.file.as_ref());
        let next = match index {
            Some(index) => (index as isize + direction).clamp(0, files.len() as isize - 1) as usize,
            None if direction < 0 => files.len() - 1,
            None => 0,
        };
        self.load_file(files[next].path.clone(), window, cx);
    }

    fn file_tree_entries(&mut self) -> std::rc::Rc<Vec<TreeEntry>> {
        let collapsed = if self.file_tree_filter.is_empty() {
            self.review
                .as_ref()
                .and_then(|review| self.collapsed_file_folders.get(&review.request.repo))
        } else {
            Some(&self.filtered_file_folders)
        };
        self.file_tree.entries(collapsed)
    }

    fn toggle_file_folder(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(review) = &self.review else {
            return;
        };
        let collapsed = if self.filter.read(cx).value().is_empty() {
            self.collapsed_file_folders
                .entry(review.request.repo.clone())
                .or_default()
        } else {
            &mut self.filtered_file_folders
        };
        if !collapsed.remove(path) {
            collapsed.insert(path.to_owned());
        }
        self.file_tree.invalidate_rows();
        cx.notify();
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

    fn close_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.modal = None;

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
    }

    fn open_branch_modal(&mut self, repo: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading || self.patch_loading || self.submitting || self.folder_picker_open {
            return;
        }
        if self
            .modal
            .as_ref()
            .is_some_and(|m| m.kind == ModalKind::Review && m.request.repo == repo)
        {
            self.close_modal(window, cx);
            return;
        }
        self.close_modal(window, cx);
        self.error = None;
        let request = ReviewRequest {
            repo,
            ..Default::default()
        };
        let modal = Modal {
            kind: ModalKind::Review,
            request: request.clone(),
            serial: self.modal_serial,
            anchor: Some(format!("add-{}", request.repo).into()),
            anchor_bounds: Default::default(),
            local_branches: Vec::new(),
            branches_loading: self.backend.is_some(),
            branches_error: self
                .backend
                .is_none()
                .then(|| "The local service is unavailable. Restart Rev.".into()),
        };
        for input in &self.fields {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }

        self.modal = Some(modal);
        self.modal_focus.focus(window, cx);
        if self.backend.is_some() {
            let serial = self.modal_serial;
            self.rpc(
                "branches",
                json!({ "repo": request.repo, "localOnly": true }),
                window,
                cx,
                move |this, result, _, _| {
                    let Some(modal) = this.modal.as_mut().filter(|modal| modal.serial == serial)
                    else {
                        return;
                    };
                    modal.branches_loading = false;
                    match result.and_then(decode::<Vec<String>>) {
                        Ok(branches) => modal.local_branches = branches,
                        Err(error) => modal.branches_error = Some(error),
                    }
                },
            );
        }
        cx.notify();
    }

    fn open_local_branch(
        &mut self,
        branch: &str,
        serial: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.submitting || self.loading || self.patch_loading || self.workspace_busy {
            return;
        }
        let Some(modal) = self.modal.as_ref().filter(|modal| {
            modal.serial == serial && modal.local_branches.iter().any(|name| name == branch)
        }) else {
            return;
        };
        let request = modal.local_branch_request(branch);
        self.close_modal(window, cx);
        self.load_review(request, None, window, cx);
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
        if self
            .modal
            .as_ref()
            .is_some_and(|m| m.kind == ModalKind::Details)
        {
            return;
        }
        if self
            .modal
            .as_ref()
            .is_some_and(|m| m.kind == ModalKind::Commits)
        {
            self.apply_commit_selection(window, cx);
            return;
        }
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

        self.error = None;
        let request = modal.manual_request(&values);
        if request.repo.is_empty() {
            self.error = Some("Choose a repository folder.".into());
            return;
        }
        if values[1].starts_with("https://github.com/")
            || values[1].starts_with('#')
            || values[1].parse::<u64>().is_ok()
        {
            self.submitting = true;
            self.rpc(
                "pr",
                json!({
                    "repo": request.repo,
                    "base": request.base,
                    "value": values[1].trim_start_matches('#'),
                }),
                window,
                cx,
                move |this, result, window, cx| {
                    this.submitting = false;
                    if this
                        .modal
                        .as_ref()
                        .is_none_or(|open| open.serial != modal.serial)
                    {
                        return;
                    }
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
        cx.notify();
    }
}
