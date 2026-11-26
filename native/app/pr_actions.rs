use super::controls::*;
use super::popover::*;
use super::*;
use crate::model::{PullRequest, PullRequestStatus};
use crate::theme::*;
use gpui_kit::{
    assets::IconName,
    base::{Button as BaseButton, Popover},
    component::Icon,
};
use serde::Deserialize;

const PR_MENU_WIDTH: f32 = 260.;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum MergeMethod {
    Squash,
    Merge,
    Rebase,
}

impl MergeMethod {
    fn key(self) -> &'static str {
        match self {
            Self::Squash => "squash",
            Self::Merge => "merge",
            Self::Rebase => "rebase",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Squash => "Squash and merge",
            Self::Merge => "Create a merge commit",
            Self::Rebase => "Rebase and merge",
        }
    }

    fn confirm_label(self) -> &'static str {
        match self {
            Self::Squash => "Squash and merge",
            Self::Merge => "Merge",
            Self::Rebase => "Rebase and merge",
        }
    }
}

#[derive(Default)]
pub(super) struct PrActions {
    pub(super) open: bool,
    pub(super) url: String,
    pub(super) methods: Option<Result<Vec<MergeMethod>, String>>,
    pub(super) confirm: Option<MergeMethod>,
    busy: bool,
}

impl Rev {
    fn open_pr_actions(&mut self, url: String, window: &Window, cx: &mut Context<Self>) {
        let Some(review) = &self.review else { return };
        let repo = review.request.repo.clone();
        let actions = &mut self.pr_actions;
        actions.open = true;
        actions.confirm = None;
        if actions.url != url {
            actions.url = url.clone();
            actions.methods = None;
        }
        if actions.methods.as_ref().is_some_and(Result::is_ok) {
            cx.notify();
            return;
        }
        actions.methods = None;
        self.rpc_quiet(
            "mergeMethods",
            json!({ "repo": repo, "url": url }),
            window,
            cx,
            move |this, result, _, cx| {
                if this.pr_actions.url != url {
                    return;
                }
                this.pr_actions.methods = Some(result.and_then(decode::<Vec<MergeMethod>>));
                cx.notify();
            },
        );
    }

    fn close_pr_actions(&mut self, cx: &mut Context<Self>) {
        if self.pr_actions.open {
            self.pr_actions.open = false;
            self.pr_actions.confirm = None;
            cx.notify();
        }
    }

    fn confirm_merge(&mut self, method: Option<MergeMethod>, cx: &mut Context<Self>) {
        self.pr_actions.confirm = method;
        cx.notify();
    }

    fn run_pr_action(&mut self, action: Value, window: &Window, cx: &mut Context<Self>) {
        let Some(review) = &self.review else { return };
        if self.pr_actions.busy {
            return;
        }
        let key = review.key.clone();
        let repo = review.request.repo.clone();
        let url = self.pr_actions.url.clone();
        self.pr_actions.busy = true;
        cx.notify();
        self.rpc(
            "pullRequestAction",
            json!({ "repo": repo, "url": url, "action": action }),
            window,
            cx,
            move |this, result, _, cx| {
                this.pr_actions.busy = false;
                this.pr_actions.open = false;
                this.pr_actions.confirm = None;
                if let Some(value) = this.result(result) {
                    match decode::<PullRequest>(value) {
                        Ok(pr) => {
                            this.pull_requests.insert(key.clone(), pr.clone());
                            if let Some(review) = this.review.as_mut().filter(|r| r.key == key) {
                                review.pull_request = Some(pr);
                            }
                        }
                        Err(error) => this.error = Some(error),
                    }
                }
                cx.notify();
            },
        );
    }

    pub(super) fn pr_status_pill(
        &self,
        pr: &PullRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let status = pr.status()?;
        let (label, icon, color) = match status {
            PullRequestStatus::Open => ("Open", IconName::GitPullRequest, SUCCESS),
            PullRequestStatus::Draft => ("Draft", IconName::GitPullRequestDraft, MUTED),
            PullRequestStatus::Merged => ("Merged", IconName::GitMerge, PURPLE),
            PullRequestStatus::Closed => ("Closed", IconName::GitPullRequestClosed, DANGER),
        };
        let status_icon = Icon::new(icon)
            .size(px(14.))
            .flex_shrink_0()
            .text_color(rgb(color));
        if pr.url.is_empty()
            || matches!(
                status,
                PullRequestStatus::Merged | PullRequestStatus::Closed
            )
        {
            return Some(
                header_pill()
                    .flex_shrink_0()
                    .debug_selector(|| "pr-status-pill".into())
                    .child(status_icon)
                    .child(label)
                    .into_any_element(),
            );
        }

        let open = self.pr_actions.open && self.pr_actions.url == pr.url;
        let trigger = BaseButton::new("pr-actions-trigger")
            .debug_selector(|| "pr-status-pill".into())
            .h(px(HEADER_PILL_HEIGHT))
            .flex_shrink_0()
            .px(px(HEADER_PILL_PADDING))
            .gap(px(6.))
            .rounded(px(CONTROL_RADIUS))
            .bg(rgb(SURFACE))
            .border_1()
            .border_color(rgb(CONTROL_BORDER))
            .text_size(px(12.))
            .line_height(px(HEADER_PILL_LINE_HEIGHT))
            .text_color(rgb(TEXT))
            .hover(|d| d.bg(rgb(HOVER)).border_color(rgb(SELECTED_BORDER)))
            .styles(|styles| {
                styles.selected(|d| d.bg(rgb(HOVER)).border_color(rgb(SELECTED_BORDER)))
            })
            .focus_visible(|d| d.border_color(rgb(FOCUS)))
            .selected(open)
            .aria_expanded(open)
            .accessibility_label(format!("Pull request actions: {label}"))
            .child(status_icon)
            .child(label)
            .child(
                Icon::new(IconName::ChevronDown)
                    .size(px(12.))
                    .flex_shrink_0()
                    .text_color(rgb(MUTED)),
            );

        let popover = AnimatedPopover::new("pr-actions", open, window, cx);
        let menu = popover
            .visible()
            .then(|| self.pr_actions_menu(pr, status, cx));
        let this = cx.entity().downgrade();
        let url = pr.url.clone();
        let picker = Popover::new("pr-actions")
            .flex_shrink_0()
            .on_open_change(move |open, window, cx| {
                let open = *open;
                let url = url.clone();
                _ = this.update(cx, |this, cx| {
                    if open && !this.pr_actions.open {
                        this.open_pr_actions(url, window, cx);
                    } else {
                        this.close_pr_actions(cx);
                    }
                });
            })
            .trigger(trigger);
        Some(popover.render(picker, menu))
    }

    fn pr_actions_menu(
        &self,
        pr: &PullRequest,
        status: PullRequestStatus,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let actions = &self.pr_actions;
        let busy = actions.busy;
        let surface = popover_surface()
            .id("pr-actions-menu")
            .debug_selector(|| "pr-actions-menu".into())
            .w(px(PR_MENU_WIDTH))
            .p(px(UI_SPACING))
            .gap(px(2.))
            .text_size(px(12.))
            .text_color(rgb(TEXT));

        if let Some(method) = actions.confirm {
            return surface
                .gap(px(UI_SPACING))
                .child(
                    div()
                        .px(px(UI_SPACING))
                        .pt(px(2.))
                        .line_height(px(16.))
                        .child(format!(
                            "{} #{} into {}?",
                            method.confirm_label(),
                            pr.number,
                            pr.base_ref_name
                        )),
                )
                .child(
                    row()
                        .justify_end()
                        .gap(px(UI_SPACING))
                        .child(
                            button("cancel-merge", "Cancel")
                                .debug_selector(|| "cancel-merge".into())
                                .disabled(busy)
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.confirm_merge(None, cx)),
                                ),
                        )
                        .child(
                            primary_button(
                                "confirm-merge",
                                if busy {
                                    "Merging…"
                                } else {
                                    method.confirm_label()
                                },
                            )
                            .debug_selector(|| "confirm-merge".into())
                            .disabled(busy)
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.run_pr_action(
                                        json!({ "type": "merge", "method": method.key() }),
                                        window,
                                        cx,
                                    )
                                },
                            )),
                        ),
                )
                .into_any_element();
        }

        let (draft_label, draft_icon, draft_action) = if status == PullRequestStatus::Draft {
            ("Ready for review", IconName::GitPullRequest, "ready")
        } else {
            ("Convert to draft", IconName::GitPullRequestDraft, "draft")
        };
        surface
            .child(
                menu_item("pr-draft-toggle", draft_icon, draft_label)
                    .disabled(busy)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.run_pr_action(json!({ "type": draft_action }), window, cx)
                    })),
            )
            .child(div().h(px(1.)).my(px(3.)).flex_shrink_0().bg(rgb(BORDER)))
            .map(|d| match &actions.methods {
                None => d.child(menu_note("Loading merge options…")),
                Some(Err(_)) => d.child(menu_note("Couldn't load merge options")),
                Some(Ok(methods)) if methods.is_empty() => {
                    d.child(menu_note("This repository allows no merge methods"))
                }
                Some(Ok(methods)) => d.children(methods.iter().map(|&method| {
                    menu_item(
                        SharedString::from(format!("pr-merge-{}", method.key())),
                        IconName::GitMerge,
                        method.label(),
                    )
                    .disabled(busy || status == PullRequestStatus::Draft)
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.confirm_merge(Some(method), cx)),
                    )
                })),
            })
            .into_any_element()
    }
}

fn menu_item(id: impl Into<SharedString>, icon: IconName, label: &'static str) -> BaseButton {
    let id = id.into();
    let selector = id.clone();
    choice_button(id, false)
        .debug_selector(move || selector.to_string())
        .h(px(CONTROL_HEIGHT))
        .accessibility_label(label)
        .child(
            Icon::new(icon)
                .size(px(14.))
                .flex_shrink_0()
                .text_color(rgb(MUTED)),
        )
        .child(label)
}

fn menu_note(text: &'static str) -> Div {
    div()
        .px(px(CONTROL_PADDING))
        .py(px(UI_SPACING))
        .text_color(rgb(MUTED))
        .child(text)
}
