use std::{cell::Cell, rc::Rc};
use super::controls::*;
use super::{CloseModal, Rev, Submit, decode};
use crate::{model::ReviewRequest, theme::*};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    FocusTrapElement, Icon, Sizable,
    input::{Input, InputState},
};
use gpui_kit::prelude::*;
use gpui_kit::*;
use serde_json::json;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum ModalKind {
    Review,
    Commits,
    Details,
}

#[derive(Clone)]
pub(super) struct Modal {
    pub(super) kind: ModalKind,
    pub(super) request: ReviewRequest,
    pub(super) base_input: Entity<InputState>,
    pub(super) head_input: Entity<InputState>,
    pub(super) serial: u64,
    pub(super) anchor: Option<SharedString>,
    pub(super) anchor_bounds: Rc<Cell<Bounds<Pixels>>>,
    pub(super) local_branches: Vec<String>,
    pub(super) branches_loading: bool,
    pub(super) branches_error: Option<String>,
}

impl Modal {
    pub(super) fn manual_request(&self, cx: &App) -> ReviewRequest {
        let base = self.base_input.read(cx).value().trim().to_string();
        let head = self.head_input.read(cx).value().trim().to_string();

        ReviewRequest {
            repo: self.request.repo.clone(),
            base: (!base.is_empty()).then_some(base),
            head: (!head.is_empty()).then_some(head),
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

impl Rev {
    pub(super) fn close_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.modal = None;

        self.modal_serial += 1;
        self.diff.update(cx, |diff, cx| diff.clear_selection(cx));
        gpui_kit::base::TextSelection::clear(window, cx);
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn open_branch_modal(
        &mut self,
        repo: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.loading || self.patch_loading || self.submitting || self.folder_picker_open {
            return;
        }
        if self.modal.as_ref().is_some_and(|m| m.kind == ModalKind::Review && m.request.repo == repo) {
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
            base_input: cx.new(|cx| InputState::new(window, cx).placeholder("Auto-detect base")),
            head_input: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Branch, #123 or PR URL (default: HEAD)")
            }),
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

    pub(super) fn open_local_branch(
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

    pub(super) fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal.as_ref().is_some_and(|m| m.kind == ModalKind::Details) {
            return;
        }
        if self.modal.as_ref().is_some_and(|m| m.kind == ModalKind::Commits) {
            self.apply_commit_selection(window, cx);
            return;
        }
        if self.submitting {
            return;
        }
        let Some(modal) = self.modal.clone() else {
            return;
        };
        self.error = None;
        let request = modal.manual_request(cx);
        if request.repo.is_empty() {
            self.error = Some("Choose a repository folder.".into());
            return;
        }
        let head = request.head.as_deref().unwrap_or_default();
        if head.starts_with("https://github.com/")
            || head.starts_with('#')
            || head.parse::<u64>().is_ok()
        {
            let serial = modal.serial;
            self.submitting = true;
            self.rpc(
                "pr",
                json!({
                    "repo": request.repo,
                    "base": request.base,
                    "value": head.trim_start_matches('#'),
                }),
                window,
                cx,
                move |this, result, window, cx| {
                    this.submitting = false;
                    if this.modal.as_ref().is_none_or(|open| open.serial != serial) {
                        return;
                    }
                    if let Some(value) = this.result(result) {
                        match decode(value) {
                            Ok(request) => {
                                if this.modal_serial == serial {
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

    fn form_field(
        &self,
        input: &Entity<InputState>,
        id: &'static str,
        title: &str,
        description: &str,
    ) -> AnyElement {
        column()
            .gap_1()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .text_color(rgb(MUTED))
                    .text_size(px(12.))
                    .child(title.to_string()),
            )
            .child(
                Input::new(input)
                    .id(id)
                    .small()
                    .min_h(px(CONTROL_HEIGHT))
                    .max_h(px(CONTROL_HEIGHT))
                    .px(px(CONTROL_PADDING))
                    .rounded(px(CONTROL_RADIUS))
                    .bg(rgb(BG))
                    .border_1()
                    .border_color(rgb(CONTROL_BORDER))
                    .text_size(px(12.))
                    .disabled(self.submitting)
                    .aria_label(description.to_string()),
            )
            .into_any_element()
    }

    pub(super) fn popover(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(modal) = self.modal.clone() else {
            return div().into_any_element();
        };
        let is_commits = modal.kind == ModalKind::Commits;
        let is_details = modal.kind == ModalKind::Details;
        let title = if is_details {
            "Review details in"
        } else if is_commits {
            "Review commits in"
        } else {
            "Open branch or PR in"
        };
        let mut body = column().gap(px(POPOVER_PADDING));
        let mut footer = icon_actions().justify_end();
        if is_commits {
            body = body.child(self.commit_choices(cx));
        } else if is_details {
            body = body.child(self.review_details());
        } else {
            let mut choices = column().gap(px(2.));
            for (i, branch) in modal.local_branches.iter().enumerate() {
                let branch = branch.clone();
                let serial = modal.serial;
                choices = choices.child(
                    choice_button(("local-branch", i), false)
                        .disabled(self.submitting)
                        .accessibility_label(format!("Open local branch {branch}"))
                        .child(
                            Icon::new(IconName::GitBranch)
                                .size(px(14.))
                                .flex_shrink_0()
                                .text_color(rgb(MUTED)),
                        )
                        .child(
                            column()
                                .flex_1()
                                .min_w_0()
                                .gap_0()
                                .line_height(px(16.))
                                .child(div().truncate().child(branch.clone()))
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(rgb(MUTED))
                                        .child("Local branch"),
                                ),
                        )
                        .child(
                            Icon::new(IconName::ChevronRight)
                                .size(px(12.))
                                .text_color(rgb(MUTED)),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_local_branch(&branch, serial, window, cx)
                        })),
                );
            }
            if modal.branches_loading {
                choices = choices.child(
                    div()
                        .px(px(CONTROL_PADDING))
                        .py(px(UI_SPACING))
                        .text_color(rgb(MUTED))
                        .child("Loading local branches…"),
                );
            } else if let Some(error) = &modal.branches_error {
                choices = choices.child(
                    div()
                        .px(px(CONTROL_PADDING))
                        .py(px(UI_SPACING))
                        .text_color(rgb(DANGER))
                        .child(error.clone()),
                );
            } else if modal.local_branches.is_empty() {
                choices = choices.child(
                    div()
                        .px(px(CONTROL_PADDING))
                        .py(px(UI_SPACING))
                        .text_color(rgb(MUTED))
                        .child("No local branches yet."),
                );
            }
            body = body
                .child(
                    column()
                        .gap(px(UI_SPACING))
                        .child(
                            div()
                                .text_size(px(12.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Branches"),
                        )
                        .child(
                            div()
                                .id("branch-options")
                                .max_h(px(220.))
                                .overflow_y_scroll()
                                .child(choices),
                        ),
                )
                .child(
                    column()
                        .gap(px(POPOVER_PADDING))
                        .pt(px(POPOVER_PADDING))
                        .border_t_1()
                        .border_color(rgb(BORDER))
                        .child(div().font_weight(FontWeight::SEMIBOLD).child("Manual diff"))
                        .child(
                            row()
                                .items_start()
                                .gap(px(POPOVER_PADDING))
                                .child(self.form_field(&modal.base_input, "base-input",
                                    "Base",
                                    "Base branch; leave blank to detect",
                                ))
                                .child(div().flex().w(relative(0.65)).min_w_0().child(
                                    self.form_field(&modal.head_input, "head-input",
                                        "Branch or PR",
                                        "Branch, PR number or URL; leave blank for HEAD",
                                    ),
                                )),
                        ),
                );
        }
        if let Some(error) = &self.error {
            body = body.child(div().text_color(rgb(DANGER)).child(error.clone()));
        }
        if !is_details {
            let label = if self.submitting {
                "Working…"
            } else {
                if is_commits {
                    "Review selection"
                } else {
                    "Open diff"
                }
            };
            footer = footer.child(
                primary_button("submit-modal", label)
                    .disabled(self.submitting)
                    .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
            );
        }
        let max_height = (window.viewport_size().height - px(80.)).max(px(220.));
        let serial = modal.serial;
        let reveal = super::motion::enter("review-popover-enter", window, cx);
        column()
            .opacity(reveal)
            .id("review-popover")
            .on_mouse_down_out(cx.listener(move |this, event, window, cx| {
                this.dismiss_popover(serial, event, window, cx)
            }))
            .w(px(650.))
            .max_w_full()
            .max_h(max_height)
            .bg(rgb(SURFACE))
            .border_1()
            .border_color(rgb(CONTROL_BORDER))
            .rounded(px(CONTROL_RADIUS))
            .shadow_lg()
            .track_focus(&self.modal_focus)
            .key_context("RevPopover")
            .on_action(cx.listener(|this, _: &CloseModal, window, cx| this.close_modal(window, cx)))
            .on_action(cx.listener(|this, _: &Submit, window, cx| this.submit(window, cx)))
            .child(
                row()
                    .h(px(HEADER_HEIGHT))
                    .flex_shrink_0()
                    .pl(px(POPOVER_PADDING))
                    .pr(px(UI_SPACING))
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .flex_shrink_0()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .map(|d| {
                        let name = std::path::Path::new(&modal.request.repo)
                            .file_name()
                            .map(|name| name.to_string_lossy().to_string())
                            .unwrap_or_else(|| modal.request.repo.clone());
                        d.child(
                            header_pill()
                                .max_w(px(200.))
                                .text_color(rgb(TEXT))
                                .child(
                                    Icon::new(IconName::Folder)
                                        .size(px(14.))
                                        .text_color(rgb(MUTED))
                                        .flex_shrink_0(),
                                )
                                .child(div().min_w_0().truncate().child(name)),
                        )
                    })
                    .child(div().flex_1().min_w_0())
                    .child(
                        icon_button("close-modal", "Close", IconName::Close)
                            .flex_shrink_0()
                            .on_click(
                                cx.listener(|this, _, window, cx| this.close_modal(window, cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .id("modal-body")
                    .min_h_0()
                    .flex_1()
                    .overflow_y_scroll()
                    .p(px(POPOVER_PADDING))
                    .child(body),
            )
            .when(!is_details, |dialog| {
                dialog.child(
                    div()
                        .px(px(POPOVER_PADDING))
                        .py(px(UI_SPACING))
                        .flex_shrink_0()
                        .border_t_1()
                        .border_color(rgb(BORDER))
                        .child(footer),
                )
            })
            .focus_trap("modal-trap", &self.modal_focus)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{Modal, ModalKind};
    use crate::model::{CommitRange, ReviewRequest};
    use gpui_kit::{App, AppContext, TestAppContext, Window, component::input::InputState};

    fn repo_modal(window: &mut Window, cx: &mut App) -> Modal {
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
            base_input: cx.new(|cx| InputState::new(window, cx)),
            head_input: cx.new(|cx| InputState::new(window, cx)),
            serial: 1,
            anchor: None,
            anchor_bounds: Default::default(),
            local_branches: vec!["feature".into()],
            branches_loading: false,
            branches_error: None,
        }
    }

    #[gpui::test]
    fn manual_diff_uses_the_repo_context_and_explicit_comparison(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.add_empty_window().update(|window, cx| {
            let modal = repo_modal(window, cx);
            modal
                .base_input
                .update(cx, |input, cx| input.set_value(" custom-base ", window, cx));
            modal
                .head_input
                .update(cx, |input, cx| input.set_value(" #123 ", window, cx));

            let request = modal.manual_request(cx);
            assert_eq!(request.repo, "/selected-repo");
            assert_eq!(request.base.as_deref(), Some("custom-base"));
            assert_eq!(request.head.as_deref(), Some("#123"));
            assert_eq!(request.mode.as_deref(), Some("branch"));
            assert_eq!(request.commit_range, None);
        });
    }

    #[gpui::test]
    fn local_branch_opens_independently_of_manual_inputs_and_previous_scope(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        cx.add_empty_window().update(|window, cx| {
            let modal = repo_modal(window, cx);
            let defaults = modal.manual_request(cx);
            assert_eq!(defaults.base, None);
            assert_eq!(defaults.head, None);
            assert_eq!(defaults.mode.as_deref(), Some("branch"));

            modal
                .base_input
                .update(cx, |input, cx| input.set_value("custom-base", window, cx));
            modal
                .head_input
                .update(cx, |input, cx| input.set_value("#123", window, cx));
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
        });
    }
}

impl Rev {
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
}
