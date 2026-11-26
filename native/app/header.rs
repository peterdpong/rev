use super::*;
use super::branch_comparison::branch_comparison;
use super::controls::*;
use super::popover::*;
use super::view::SIDEBAR_WIDTH;
use crate::model::{PullRequest, PullRequestStatus};
use crate::theme::*;
use gpui_kit::{
    assets::IconName,
    base::{Button as BaseButton, Popup},
    component::{
        Icon, TitleBar,
        text::{TextView, TextViewStyle},
    },
};

const COLLAPSED_HEADER_WIDTH: f32 = 112.;
// Match the spacing between header pills.
pub(super) const PR_DESCRIPTION_GAP: f32 = UI_SPACING;
pub(super) const PR_CARD_LINE_HEIGHT: f32 = 16.;
pub(super) const PR_CARD_GAP: f32 = 2.;
const PR_CARD_PADDING_Y: f32 = 8.;
pub(super) const PR_CARD_LINK_INSET: f32 = 4.;

#[derive(IntoElement)]
pub(super) struct PrDescription {
    pub(super) id: SharedString,
    pub(super) pull_request: PullRequest,
}

#[derive(Default)]
struct PrDescriptionState {
    open: bool,
    trigger_hovered: bool,
    content_hovered: bool,
    close_task: Option<Task<()>>,
}

impl PrDescriptionState {
    fn update_hover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_task = None;
        if self.trigger_hovered || self.content_hovered {
            if !self.open {
                self.open = true;
                cx.notify();
            }
        } else if self.open {
            // Allow the pointer to cross the gap before starting the exit fade.
            self.close_task = Some(cx.spawn_in(window, async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(150))
                    .await;
                let _ = this.update_in(cx, |state, _, cx| {
                    if !state.trigger_hovered && !state.content_hovered {
                        state.open = false;
                        cx.notify();
                    }
                });
            }));
        }
    }
}

impl RenderOnce for PrDescription {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state =
            window.use_keyed_state((ElementId::from(self.id.clone()), "hover"), cx, |_, _| {
                PrDescriptionState::default()
            });
        let open = state.read(cx).open;
        let motion = PopoverMotion::new(format!("{}-motion", self.id), open, window, cx);
        let pr = self.pull_request;

        let popup = Popup::new(
            self.id,
            div()
                .id("pr-title-text")
                .debug_selector(|| "pr-title-text".into())
                .min_w_0()
                .truncate()
                .child(format!("{}  #{}", pr.title, pr.number))
                .on_hover(window.listener_for(&state, |state, hovered, window, cx| {
                    state.trigger_hovered = *hovered;
                    state.update_hover(window, cx);
                })),
        )
        .anchor(Anchor::TopLeft)
        .offset(px(
            (HEADER_PILL_HEIGHT - HEADER_PILL_LINE_HEIGHT) / 2. + PR_DESCRIPTION_GAP
        ));

        if !motion.visible() {
            return popup;
        }

        popup.content(
            motion.apply(
                popover_surface()
                    .id("pr-description-card")
                    .debug_selector(|| "pr-description-content".into())
                    .relative()
                    // The text-only trigger is inset from the surrounding pill.
                    .left(px(-HEADER_PILL_PADDING - 1.))
                    .w(px(444.))
                    .px(px(HEADER_PILL_PADDING))
                    .py(px(PR_CARD_PADDING_Y))
                    .gap(px(PR_CARD_GAP))
                    .text_size(px(12.))
                    .line_height(px(PR_CARD_LINE_HEIGHT))
                    .whitespace_normal()
                    .on_hover(window.listener_for(&state, |state, hovered, window, cx| {
                        state.content_hovered = *hovered;
                        state.update_hover(window, cx);
                    }))
                    .child(
                        row()
                            .items_start()
                            .gap(px(UI_SPACING))
                            .child(
                                column()
                                    .flex_1()
                                    .min_w_0()
                                    .gap(px(PR_CARD_GAP))
                                    .child(
                                        div()
                                            .debug_selector(|| "pr-card-title".into())
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(pr.title.clone()),
                                    )
                                    .child(
                                        pull_request_meta(&pr)
                                            .debug_selector(|| "pr-card-meta".into()),
                                    ),
                            )
                            .when(!pr.url.is_empty(), |d| {
                                let url = pr.url.clone();
                                d.child(
                                    button("open-pr-browser", "GitHub ↗")
                                        .debug_selector(|| "open-pr-browser".into())
                                        .h(px(22.))
                                        .px(px(8.))
                                        .rounded(px(POPOVER_RADIUS - PR_CARD_LINK_INSET))
                                        .mt(px(PR_CARD_LINK_INSET - PR_CARD_PADDING_Y - 1.))
                                        .mr(px(PR_CARD_LINK_INSET - HEADER_PILL_PADDING - 1.))
                                        .on_click(move |_, _, cx| cx.open_url(&url)),
                                )
                            }),
                    )
                    .child(
                        div()
                            .id("pull-request-description")
                            .debug_selector(|| "pr-card-description".into())
                            .max_h(px(300.))
                            .overflow_y_scroll()
                            .map(|d| {
                                if pr.body.trim().is_empty() {
                                    d.text_color(rgb(palette().muted)).child("No description provided.")
                                } else {
                                    let body = pr.body.trim_end().to_owned();
                                    d.child(
                                        TextView::markdown("pr-description-markdown", body).style(
                                            TextViewStyle::default()
                                                .paragraph_gap(rems(PR_CARD_GAP / 16.)),
                                        ),
                                    )
                                }
                            }),
                    ),
            ),
        )
    }
}

fn pull_request_meta(pr: &PullRequest) -> Div {
    let status = pr.status().map(|status| match status {
        PullRequestStatus::Open => ("Open", palette().success),
        PullRequestStatus::Draft => ("Draft", palette().muted),
        PullRequestStatus::Merged => ("Merged", palette().purple),
        PullRequestStatus::Closed => ("Closed", palette().danger),
    });
    row()
        .gap(px(4.))
        .text_color(rgb(palette().muted))
        .child(format!("#{}", pr.number))
        .when(!pr.author.is_empty(), |d| {
            d.child("·").child(pr.author.clone())
        })
        .when_some(status, |d, (label, color)| {
            d.child("·")
                .child(div().text_color(rgb(color)).child(label))
        })
}

const PR_SKELETON_WIDTH: f32 = 220.;

fn pull_request_skeleton(width: Option<Pixels>) -> impl IntoElement {
    header_pill()
        .id("pr-title-loading")
        .debug_selector(|| "pr-title-loading".into())
        .w(width.unwrap_or(px(PR_SKELETON_WIDTH)))
        .max_w_full()
        .flex_shrink_1()
        .role(accesskit::Role::ProgressIndicator)
        .aria_label("Loading pull request")
        .child(
            div()
                .flex_1()
                .h(px(8.))
                .rounded_full()
                .bg(rgb(palette().selected))
                .with_animation(
                    "pr-title-pulse",
                    Animation::new(std::time::Duration::from_millis(1200))
                        .repeat()
                        .with_easing(pulsating_between(0.45, 1.)),
                    |bar, delta| bar.opacity(delta),
                ),
        )
}

impl Rev {
    fn sidebar_toggle(&self, cx: &mut Context<Self>) -> BaseButton {
        icon_button(
            "toggle-sidebar",
            if self.sidebar_collapsed {
                "Show sidebar (⌘B)"
            } else {
                "Hide sidebar (⌘B)"
            },
            if self.sidebar_collapsed {
                IconName::PanelLeftOpen
            } else {
                IconName::PanelLeftClose
            },
        )
        .on_click(cx.listener(|this, _, window, cx| this.toggle_sidebar(window, cx)))
    }

    pub(super) fn review_header(
        &self,
        sidebar_openness: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> TitleBar {
        const BRANCH_FIXED_WIDTH: f32 = HEADER_PILL_PADDING * 2. + 2. + 14. + 6.;
        let title_pill_width = self.review.as_ref().map(|review| {
            let pr_title = review
                .pull_request
                .as_ref()
                .map(|pr| format!("{}  #{}", pr.title, pr.number));
            let (label, fixed_width) = match pr_title {
                Some(title) => (title, HEADER_PILL_PADDING * 2. + 2.),
                None if self.pull_request_loading => (String::new(), PR_SKELETON_WIDTH),
                None => (review.branch_label(), BRANCH_FIXED_WIDTH),
            };
            sliding_pill_width("title-pill-width", &label, fixed_width, None, window, cx)
        });
        TitleBar::new()
            .h(px(HEADER_HEIGHT))
            .pl_0()
            .bg(rgb(palette().bg))
            .border_b_1()
            .border_color(rgb(palette().border))
            .child(
                div()
                    .w(px(COLLAPSED_HEADER_WIDTH
                        + (SIDEBAR_WIDTH - COLLAPSED_HEADER_WIDTH)
                            * sidebar_openness))
                    .h_full()
                    .flex_shrink_0()
                    .bg(rgb(palette().panel))
                    .border_r_1()
                    .border_color(rgb(palette().border))
                    .flex()
                    .items_center()
                    .justify_end()
                    .pr(px(UI_SPACING))
                    .child(self.sidebar_toggle(cx)),
            )
            .child(
                row()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .pl(px(UI_SPACING))
                    .pr(px(UI_SPACING))
                    .gap(px(UI_SPACING))
                    .when_some(self.review.as_ref(), |d, review| {
                        d.child(
                            header_pill()
                                .max_w(px(200.))
                                .flex_shrink_0()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(
                                    Icon::new(IconName::Folder)
                                        .size(px(14.))
                                        .flex_shrink_0()
                                        .text_color(rgb(palette().muted)),
                                )
                                .child(div().min_w_0().truncate().child(review.name.clone())),
                        )
                        .child(
                            div()
                                .id("pull-request-title")
                                .flex_shrink_1()
                                .min_w_0()
                                .flex()
                                .items_center()
                                .map(|d| {
                                    let width = title_pill_width.flatten();
                                    if let Some(pr) = &review.pull_request {
                                        d.child(
                                            header_pill()
                                                .id("pr-title-pill")
                                                .debug_selector(|| "pr-title-pill".into())
                                                .max_w_full()
                                                .when_some(width, |d, w| d.w(w))
                                                .hover(|d| {
                                                    d.bg(rgb(palette().hover))
                                                        .border_color(rgb(palette().selected_border))
                                                })
                                                .child(
                                                    div().min_w_0().child(PrDescription {
                                                        id: format!(
                                                            "pr-description-{}-{}",
                                                            review.request.repo, pr.number
                                                        )
                                                        .into(),
                                                        pull_request: pr.clone(),
                                                    }),
                                                ),
                                        )
                                    } else if self.pull_request_loading {
                                        d.child(pull_request_skeleton(width))
                                    } else {
                                        d.child(
                                            header_pill()
                                                .id("branch-pill")
                                                .debug_selector(|| "branch-pill".into())
                                                .max_w_full()
                                                .when_some(width, |d, w| d.w(w))
                                                .child(
                                                    Icon::new(IconName::GitBranch)
                                                        .size(px(14.))
                                                        .flex_shrink_0()
                                                        .text_color(rgb(palette().muted)),
                                                )
                                                .child(
                                                    div()
                                                        .min_w_0()
                                                        .truncate()
                                                        .child(review.branch_label()),
                                                ),
                                        )
                                    }
                                }),
                        )
                        .child(self.commit_picker_pill(review, window, cx))
                        .child(div().flex_1().min_w_0())
                        .child(
                            icon_actions()
                                .id("review-header-actions")
                                .debug_selector(|| "review-header-actions".into())
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(|_, _, cx| cx.stop_propagation())
                                .child(
                                    icon_button("refresh", "Refresh", IconName::RefreshCw)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.refresh(window, cx)
                                        })),
                                )
                                .child(
                                    icon_button("fetch", "Fetch", IconName::CloudDownload)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.fetch(window, cx)
                                        })),
                                )
                                .child(self.settings_button(cx)),
                        )
                    })
                    .when(self.review.is_none(), |d| {
                        d.child(div().flex_1()).child(self.settings_button(cx))
                    }),
            )
    }

    pub(super) fn diff_view_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let split = self.diff.read(cx).is_split(cx);
        let wrap = self.diff.read(cx).wraps_lines();
        let single_file = self.diff.read(cx).is_single_file();

        row()
            .id("diff-view-header")
            .debug_selector(|| "diff-view-header".into())
            .bg(rgb(palette().subpanel))
            .flex_shrink_0()
            .p(px(UI_SPACING))
            .gap(px(UI_SPACING))
            .border_b_1()
            .border_color(rgb(palette().border))
            .child(
                column()
                    .flex_1()
                    .min_w_0()
                    .gap(px(UI_SPACING))
                    .line_height(px(16.))
                    .when_some(self.review.as_ref(), |d, review| {
                        let (base, head) = review.comparison_refs();
                        let (added, removed) = self.file_list.read(cx).counts();
                        d.child(branch_comparison(base, head, cx)).child(
                            row()
                                .debug_selector(|| "diff-line-counts".into())
                                .flex_shrink_0()
                                .text_size(px(12.))
                                .line_height(px(14.))
                                .child(
                                    div()
                                        .text_color(rgb(palette().muted))
                                        .child(format!("{} files", review.files.len())),
                                )
                                .child(div().text_color(rgb(palette().success)).child(format!("+{added}")))
                                .child(div().text_color(rgb(palette().danger)).child(format!("−{removed}"))),
                        )
                    }),
            )
            .child(
                icon_button(
                    "file-view-mode",
                    if single_file {
                        "Show all files"
                    } else {
                        "Show one file at a time"
                    },
                    if single_file {
                        IconName::Files
                    } else {
                        IconName::File
                    },
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.diff.update(cx, |diff, cx| diff.toggle_single_file(cx));
                    cx.notify();
                })),
            )
            .child(
                icon_button(
                    "diff-mode",
                    if split {
                        "Switch to unified diff"
                    } else {
                        "Switch to split diff"
                    },
                    if split {
                        IconName::Rows2
                    } else {
                        IconName::Columns2
                    },
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.diff.update(cx, |diff, cx| diff.toggle_mode(cx));
                    cx.notify();
                })),
            )
            .child(
                icon_button(
                    "wrap",
                    if wrap {
                        "Disable line wrapping"
                    } else {
                        "Enable line wrapping"
                    },
                    IconName::TextWrap,
                )
                .selected(wrap)
                .aria_toggled(if wrap {
                    accesskit::Toggled::True
                } else {
                    accesskit::Toggled::False
                })
                .when(wrap, selected_surface)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.diff.update(cx, |diff, cx| diff.toggle_wrap(cx));
                    cx.notify();
                })),
            )
            .into_any_element()
    }
}
