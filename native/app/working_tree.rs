use super::*;
use std::time::Duration;

impl Rev {
    pub(super) fn start_working_tree_updates(&mut self, window: &Window, cx: &mut Context<Self>) {
        self._working_tree_poll = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                if this
                    .update_in(cx, |this, window, cx| this.poll_working_trees(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    fn can_refresh_working_tree(&self) -> bool {
        !self.loading
            && !self.folder_picker_open
            && !self.patch_loading
            && !self.submitting
            && !self.workspace_busy
            && self.modal.is_none()
            && self
                .workspace
                .active_tab()
                .is_some_and(|tab| tab.kind == crate::workspace_state::TabKind::Current)
            && self
                .review
                .as_ref()
                .is_some_and(|review| review.request.mode.as_deref() == Some("working"))
    }

    fn poll_working_trees(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.working_tree_poll_inflight
            || self.working_tree_refreshing
            || self.workspace.projects.is_empty()
        {
            return;
        }
        let repos: Vec<_> = self
            .workspace
            .projects
            .iter()
            .map(|project| project.repo.clone())
            .collect();
        let active_repo = self
            .review
            .as_ref()
            .filter(|review| review.request.mode.as_deref() == Some("working"))
            .map(|review| review.request.repo.clone());
        self.working_tree_poll_inflight = true;
        self.rpc_quiet(
            "workingTrees",
            json!({ "repos": repos, "activeRepo": active_repo }),
            window,
            cx,
            move |this, result, window, cx| {
                this.working_tree_poll_inflight = false;
                let Ok(states) = result.and_then(decode::<Vec<Option<WorkingTreeState>>>) else {
                    return;
                };
                this.apply_working_tree_states(active_repo.as_deref(), states, window, cx);
            },
        );
    }

    pub(super) fn apply_working_tree_states(
        &mut self,
        active_repo: Option<&str>,
        states: Vec<Option<WorkingTreeState>>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;
        let mut refresh = None;
        for state in states.into_iter().flatten() {
            changed |= self
                .workspace
                .update_checkout(&state.repo, &state.branch, &state.sha);
            if active_repo == Some(state.repo.as_str())
                && self
                    .review
                    .as_ref()
                    .is_some_and(|review| review.request.repo == state.repo)
                && self.working_tree_revisions.get(&state.repo) != Some(&state.revision)
            {
                refresh = Some(state);
            }
        }
        if changed {
            self.save_workspace();
            cx.notify();
        }
        if let Some(state) = refresh
            && self.can_refresh_working_tree()
            && !self.working_tree_refreshing
        {
            self.refresh_working_tree(state, window, cx);
        }
    }

    fn refresh_working_tree(
        &mut self,
        snapshot: WorkingTreeState,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let review_epoch = self.review_epoch;
        let file_epoch = self.file_epoch;
        self.working_tree_refreshing = true;
        self.rpc_quiet(
            "workingReview",
            json!({ "repo": snapshot.repo, "path": self.file }),
            window,
            cx,
            move |this, result, window, cx| {
                this.working_tree_refreshing = false;
                // Navigation, file selection and dialogs win over an older background response.
                if this.review_epoch != review_epoch
                    || this.file_epoch != file_epoch
                    || !this.can_refresh_working_tree()
                {
                    return;
                }
                let Ok(mut update) = result.and_then(decode::<WorkingReview>) else {
                    return;
                };
                let Some(previous) = &this.review else { return };
                if previous.request.repo != snapshot.repo {
                    return;
                }
                let same_checkout = previous.branch == update.review.branch
                    && previous.branch_head_sha == update.review.branch_head_sha;
                if same_checkout {
                    update.review.pull_request = previous.pull_request.clone();
                }
                let review_changed = previous != &update.review;
                let same_file = this.file == update.path;
                let patch_changed = !same_file
                    || !same_checkout
                    || this.patch.digest != update.patch.digest
                    || this.patch.patch != update.patch.patch
                    || this.patch.too_large != update.patch.too_large;
                let notice = if update.patch.too_large {
                    Some("This patch exceeds the 2 MB inline limit.".into())
                } else if update.patch.patch == "Binary file (untracked)" {
                    Some("Binary file".into())
                } else {
                    None
                };
                let visible_changed = review_changed
                    || patch_changed
                    || this.state != update.state
                    || this.notice != notice;
                if patch_changed {
                    let files = if notice.is_some() || update.path.is_none() {
                        Vec::new()
                    } else {
                        let Ok(files) = DiffFile::parse(&update.patch.patch) else {
                            return;
                        };
                        files
                    };
                    this.selection = None;
                    this.file_epoch += 1;
                    this.diff.update(cx, |diff, cx| {
                        let scroll = diff.list().logical_scroll_top();
                        let horizontal = diff.horizontal_scroll().offset();
                        diff.set_files(files, cx);
                        if same_file && same_checkout {
                            diff.list().scroll_to(scroll);
                            diff.horizontal_scroll().set_offset(horizontal);
                        }
                    });
                }
                if !same_checkout {
                    this.review_epoch += 1;
                }
                this.workspace
                    .refresh_working_tree(&update.review, update.path.clone());
                this.file_tree.set_files(&update.review.files);
                this.review = Some(update.review);
                this.review_snapshot_id = Some(update.snapshot_id);
                this.state = update.state;
                this.file = update.path;
                this.patch = update.patch;
                this.notice = notice;
                this.working_tree_revisions
                    .insert(snapshot.repo, snapshot.revision);
                if review_changed || !same_file {
                    this.save_workspace();
                }
                if visible_changed {
                    cx.notify();
                }
                if !same_checkout {
                    this.load_pull_request(window, cx);
                }
            },
        );
    }
}
