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

                if this.review_epoch != review_epoch
                    || this.file_epoch != file_epoch
                    || !this.can_refresh_working_tree()
                {
                    return;
                }

                let Ok(mut update) = result.and_then(decode::<WorkingReview>) else {
                    return;
                };

                let Some(previous) = &this.review else {
                    return;
                };
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

                // The digest covers the patch text and the too-large flag.
                let patch_changed =
                    !same_file || !same_checkout || this.patch.digest != update.patch.digest;
                let notice = update.patch.notice();

                if patch_changed {
                    let files = if notice.is_some() || update.path.is_none() {
                        Vec::new()
                    } else {
                        let Ok(files) = DiffFile::parse(&update.patch.patch) else {
                            return;
                        };
                        files
                    };

                    this.file_epoch += 1;
                    this.diff.update(cx, |diff, cx| {
                        diff.refresh_files(files, same_file && same_checkout, cx);
                    });
                }

                if !same_checkout {
                    this.review_epoch += 1;
                }

                this.workspace
                    .refresh_working_tree(&update.review, update.path.clone());
                this.file_list
                    .update(cx, |list, cx| list.update_files(&update.review.files, cx));
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

                cx.notify();

                // Update pull request information if checkout changed.
                if !same_checkout {
                    this.load_pull_request(window, cx);
                }
            },
        );
    }
}
