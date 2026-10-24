use super::*;

#[derive(Default)]
pub(super) struct CommitPicker {
    pub commits: Vec<Commit>,
    pub has_more: bool,
    pub loading: bool,
    pub range: Option<CommitRange>,
    pub count: usize,
    anchor: Option<usize>,
    request: ReviewRequest,
}

impl CommitPicker {
    pub fn selected(&self, index: usize) -> bool {
        let Some(range) = &self.range else {
            return false;
        };
        let Some(newest) = self.commits.iter().position(|c| c.sha == range.end) else {
            return false;
        };
        let oldest = self
            .commits
            .iter()
            .position(|c| c.sha == range.start)
            .unwrap_or(self.commits.len());
        index >= newest && index <= oldest
    }

    fn choose(&mut self, index: usize, extend: bool) {
        if index >= self.commits.len() {
            return;
        }
        let anchor = if extend {
            self.anchor.unwrap_or(index)
        } else {
            index
        };
        let newest = anchor.min(index);
        let oldest = anchor.max(index);
        self.anchor = Some(anchor);
        self.range = Some(CommitRange {
            start: self.commits[oldest].sha.clone(),
            end: self.commits[newest].sha.clone(),
        });
        self.count = oldest - newest + 1;
    }
}

impl Patch {
    pub(super) fn open_commit_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(review) = self.review.clone() else {
            return;
        };
        self.open_modal(ModalKind::Commits, None, None, "choose-commits", window, cx);
        if !self
            .modal
            .as_ref()
            .is_some_and(|modal| modal.kind == ModalKind::Commits)
        {
            return;
        }
        self.commit_picker = CommitPicker {
            range: review.request.commit_range.clone(),
            count: review.commit_count,
            request: ReviewRequest {
                head: Some(review.branch_head_sha),
                ..review.request
            },
            ..Default::default()
        };
        self.load_more_commits(window, cx);
    }

    pub(super) fn load_more_commits(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(modal) = self.modal.as_ref().filter(|m| m.kind == ModalKind::Commits) else {
            return;
        };
        if self.commit_picker.loading {
            return;
        }
        let serial = modal.serial;
        self.commit_picker.loading = true;
        self.rpc("commits", json!({ "review": self.commit_picker.request, "offset": self.commit_picker.commits.len() }), window, cx, move |this, result, _, cx| {
            if this.modal_serial != serial { return; }
            this.commit_picker.loading = false;
            if let Some(value) = this.result(result) {
                match decode::<CommitPage>(value) {
                    Ok(page) => {
                        this.commit_picker.has_more = page.has_more;
                        this.commit_picker.commits.extend(page.commits);
                        if this.commit_picker.anchor.is_none() && let Some(range) = &this.commit_picker.range {
                            this.commit_picker.anchor = this.commit_picker.commits.iter().position(|c| c.sha == range.end);
                        }
                    }
                    Err(error) => this.error = Some(error),
                }
            }
            cx.notify();
        });
        cx.notify();
    }

    pub(super) fn choose_commit(&mut self, index: usize, extend: bool, cx: &mut Context<Self>) {
        self.commit_picker.choose(index, extend);
        cx.notify();
    }

    pub(super) fn choose_all_commits(&mut self, cx: &mut Context<Self>) {
        self.commit_picker.range = None;
        self.commit_picker.anchor = None;
        self.commit_picker.count = 0;
        cx.notify();
    }

    pub(super) fn apply_commit_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.commit_picker.loading || self.loading || self.patch_loading {
            return;
        }
        let Some(modal) = self.modal.as_ref().filter(|m| m.kind == ModalKind::Commits) else {
            return;
        };
        let request = ReviewRequest {
            mode: Some("branch".into()),
            commit_range: self.commit_picker.range.clone(),
            ..modal.request.clone()
        };
        self.close_modal(window, cx);
        self.load_review(request, self.file.clone(), window, cx);
    }
}
