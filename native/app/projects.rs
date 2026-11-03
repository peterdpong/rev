use super::*;

impl Rev {
    pub(super) fn navigate_tab(
        &mut self,
        direction: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.loading || self.workspace_busy || self.submitting {
            return;
        }
        let tabs: Vec<_> = self
            .workspace
            .projects
            .iter()
            .flat_map(|p| &p.tabs)
            .collect();
        if tabs.len() < 2 {
            return;
        }
        let index = tabs
            .iter()
            .position(|tab| Some(&tab.id) == self.workspace.active_tab_id.as_ref());
        let next = index.map_or_else(
            || if direction < 0 { tabs.len() - 1 } else { 0 },
            |index| (index as isize + direction).rem_euclid(tabs.len() as isize) as usize,
        );
        let tab = tabs[next].clone();
        self.close_modal(window, cx);
        self.load_review(tab.request, tab.file, window, cx);
    }

    pub(super) fn navigation_request(&self) -> Option<ReviewRequest> {
        self.workspace
            .active()
            .map(|tab| tab.request.clone())
            .or_else(|| self.review.as_ref().map(|review| review.request.clone()))
    }

    pub(super) fn poll_workspace(&self, window: &Window, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        let epoch = (self.review_epoch, self.workspace_epoch, self.file_epoch);
        let delay = cx.background_executor().timer(Duration::from_secs(2));
        let task = cx.background_executor().spawn(async move {
            delay.await;
            backend
                .call("workspace", json!({}))
                .and_then(decode::<Workspace>)
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if epoch == (this.review_epoch, this.workspace_epoch, this.file_epoch)
                    && !this.loading
                    && !this.patch_loading
                    && !this.workspace_busy
                    && let Ok(mut workspace) = result
                {
                    // Another window's active tab must not navigate this window.
                    if let Some(id) = this.workspace.active_tab_id.as_ref()
                        && workspace
                            .projects
                            .iter()
                            .any(|p| p.tabs.iter().any(|t| &t.id == id))
                    {
                        workspace.active_tab_id = Some(id.clone());
                    }
                    this.workspace = workspace;
                    if this.modal.is_none()
                        && !this.selecting
                        && !this.submitting
                        && this.thread_reply.read(cx).value().trim().is_empty()
                        && let Some(tab) = this
                            .workspace
                            .active()
                            .filter(|tab| tab.is_current())
                            .cloned()
                        && this.review.as_ref().is_none_or(|review| {
                            let label = if review.branch == "detached HEAD" {
                                format!("Detached {}", &review.branch_head_sha[..7])
                            } else {
                                review.branch.clone()
                            };
                            review.request.repo != tab.request.repo
                                || Some(&review.branch_head_sha) != tab.current_head_sha.as_ref()
                                || label != tab.label
                        })
                    {
                        this.load_review(tab.request, tab.file, window, cx);
                    }
                    cx.notify();
                }
                this.poll_workspace(window, cx);
            });
        })
        .detach();
    }

    pub(super) fn load_branches(&mut self, fetch: bool, window: &Window, cx: &mut Context<Self>) {
        let Some(modal) = self.modal.as_ref().filter(|m| m.kind == ModalKind::Review) else {
            return;
        };
        let repo = modal.request.repo.clone();
        let serial = modal.serial;
        let Some(backend) = self.backend.clone() else {
            return;
        };
        self.branches_loading = true;
        self.error = None;
        let task = cx.background_executor().spawn(async move {
            if fetch {
                backend.call("fetch", json!({ "repo": repo }))?;
            }
            backend
                .call("branches", json!({ "repo": repo }))
                .and_then(decode::<Vec<String>>)
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, _, cx| {
                if this.modal_serial != serial {
                    return;
                }
                this.branches_loading = false;
                match result {
                    Ok(branches) => this.branch_options = branches,
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
