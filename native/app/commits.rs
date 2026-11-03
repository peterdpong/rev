use super::controls::*;
use crate::theme::*;
use gpui_kit::{assets::IconName, component::{Icon, input::InputState}};
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

impl Rev {
    pub(super) fn open_commit_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(review) = self.review.clone() else {
            return;
        };
        if self.loading || self.patch_loading || self.submitting || self.folder_picker_open {
            return;
        }
        if self
            .modal
            .as_ref()
            .is_some_and(|m| m.kind == ModalKind::Commits)
        {
            self.close_modal(window, cx);
            return;
        }
        self.close_modal(window, cx);
        self.modal = Some(Modal {
            kind: ModalKind::Commits,
            base_input: cx.new(|cx| InputState::new(window, cx)),
            head_input: cx.new(|cx| InputState::new(window, cx)),
            request: self.navigation_request().unwrap_or_else(|| review.request.clone()),
            serial: self.modal_serial,
            anchor: Some("choose-commits".into()),
            anchor_bounds: Default::default(),
            local_branches: Vec::new(),
            branches_loading: false,
            branches_error: None,
        });
        self.modal_focus.focus(window, cx);
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

fn commit_selection_mark(selected: bool) -> Div {
    div().size(px(14.)).flex_shrink_0().when(selected, |d| {
        d.child(
            Icon::new(IconName::Check)
                .size(px(14.))
                .text_color(rgb(TEXT)),
        )
    })
}

impl Rev {
    fn commit_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let commit = &self.commit_picker.commits[index];
        let selected = self.commit_picker.selected(index);
        let label = format!("{} {}", &commit.sha[..8], commit.subject);
        div()
            .h(px(CHOICE_ROW_HEIGHT + 2.))
            .pb(px(2.))
            .child(
                choice_button(("commit", index), selected)
                    .accessibility_label(label)
                    .child(commit_selection_mark(selected))
                    .child(
                        column()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(12.))
                                    .line_height(px(16.))
                                    .child(commit.subject.clone()),
                            )
                            .child(
                                row()
                                    .gap(px(UI_SPACING))
                                    .text_size(px(11.))
                                    .line_height(px(14.))
                                    .text_color(rgb(MUTED))
                                    .child(
                                        div()
                                            .font_family("Menlo")
                                            .child(commit.sha[..8].to_string()),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .child(commit.author.clone()),
                                    )
                                    .child(
                                        commit.date.get(..10).unwrap_or(&commit.date).to_owned(),
                                    ),
                            ),
                    )
                    .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                        this.choose_commit(index, event.modifiers().shift, cx)
                    })),
            )
            .into_any_element()
    }

    pub(super) fn commit_choices(&self, cx: &mut Context<Self>) -> AnyElement {
        let all = self.commit_picker.range.is_none();
        column()
            .gap(px(UI_SPACING))
            .child(
                choice_button("all-commits", all)
                    .h(px(CONTROL_HEIGHT))
                    .accessibility_label("All commits — full branch diff")
                    .child(commit_selection_mark(all))
                    .child("All commits")
                    .on_click(cx.listener(|this, _, _, cx| this.choose_all_commits(cx))),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(MUTED))
                    .child("Click a commit. Shift-click to select a range."),
            )
            .when(!self.commit_picker.commits.is_empty(), |body| {
                body.child(
                    uniform_list(
                        "commit-list",
                        self.commit_picker.commits.len(),
                        cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                            range
                                .map(|index| this.commit_row(index, cx))
                                .collect::<Vec<_>>()
                        }),
                    )
                    .h(px(
                        self.commit_picker.commits.len().min(7) as f32 * (CHOICE_ROW_HEIGHT + 2.)
                    ))
                    .w_full(),
                )
            })
            .when(self.commit_picker.loading, |body| {
                body.child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(MUTED))
                        .child("Loading commits…"),
                )
            })
            .when(
                self.commit_picker.has_more && !self.commit_picker.loading,
                |body| {
                    body.child(button("older-commits", "Load older commits").on_click(
                        cx.listener(|this, _, window, cx| this.load_more_commits(window, cx)),
                    ))
                },
            )
            .into_any_element()
    }
}
