use super::*;
use crate::theme::*;
use gpui_kit::{
    assets::IconName,
    base::{Button as BaseButton, Popup, motion},
    component::{Icon, TitleBar, tooltip::Tooltip},
};

const SIDEBAR_WIDTH: f32 = 225.;
const LIST_ROW_HEIGHT: f32 = 26.;
const ICON_BUTTON_SIZE: f32 = 24.;
const HEADER_PILL_HEIGHT: f32 = 26.;
// Include the shared outer inset above/below the pills and the bottom border.
const HEADER_HEIGHT: f32 = HEADER_PILL_HEIGHT + UI_SPACING * 2. + 1.;
const FILE_HEADER_HEIGHT: f32 = CONTROL_HEIGHT + UI_SPACING * 2. + 1.;
const HEADER_PILL_PADDING: f32 = 10.;
const HEADER_PILL_LINE_HEIGHT: f32 = 16.;
const PR_DESCRIPTION_GAP: f32 = 2.;

#[derive(IntoElement)]
struct FolderIcon {
    id: SharedString,
    expanded: bool,
}

impl RenderOnce for FolderIcon {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let openness = motion::transition(
            (self.id, "folder-openness"),
            if self.expanded { 1.0_f32 } else { 0.0_f32 },
            motion::Transition::new(std::time::Duration::from_millis(160)),
            window,
            cx,
        );
        div()
            .relative()
            .size(px(14.))
            .flex_shrink_0()
            .text_color(rgb(MUTED))
            .child(
                Icon::new(IconName::FolderClosed)
                    .absolute()
                    .top_0()
                    .left_0()
                    .size(px(14.))
                    .opacity(1.0 - openness),
            )
            .child(
                Icon::new(IconName::FolderOpen)
                    .absolute()
                    .top_0()
                    .left_0()
                    .size(px(14.))
                    .opacity(openness),
            )
    }
}

#[derive(IntoElement)]
struct PrDescription {
    id: SharedString,
    title: String,
    description: String,
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
        // Sample even while closed, so both entrance and exit share a continuous
        // transition and reversing a fade never restarts from an endpoint.
        let opacity = motion::transition(
            (self.id.clone(), "visibility"),
            if open { 1.0_f32 } else { 0.0_f32 },
            motion::Transition::new(std::time::Duration::from_millis(110)).ease(ease_out_quint()),
            window,
            cx,
        );
        let popup = Popup::new(
            self.id,
            div()
                .id("pr-title-text")
                .debug_selector(|| "pr-title-text".into())
                .min_w_0()
                .truncate()
                .child(self.title.clone())
                .on_hover(window.listener_for(&state, |state, hovered, window, cx| {
                    state.trigger_hovered = *hovered;
                    state.update_hover(window, cx);
                })),
        )
        .anchor(Anchor::TopLeft)
        .offset(px(
            (HEADER_PILL_HEIGHT - HEADER_PILL_LINE_HEIGHT) / 2. + PR_DESCRIPTION_GAP
        ));
        if !open && opacity <= 0.0 {
            return popup;
        }
        popup.content(
            column()
                .id("pr-description-card")
                .debug_selector(|| "pr-description-content".into())
                .relative()
                // The text-only trigger is inset from the surrounding pill.
                .left(px(-HEADER_PILL_PADDING - 1.))
                .top(px(-4. * (1. - opacity)))
                .w(px(444.))
                .px(px(HEADER_PILL_PADDING))
                .py_3()
                .gap(px(10.))
                .bg(rgb(SURFACE))
                .border_1()
                .border_color(rgb(CONTROL_BORDER))
                .rounded(px(8.))
                .shadow_lg()
                .opacity(opacity)
                .whitespace_normal()
                .on_hover(window.listener_for(&state, |state, hovered, window, cx| {
                    state.content_hovered = *hovered;
                    state.update_hover(window, cx);
                }))
                .child(div().font_weight(FontWeight::SEMIBOLD).child(self.title))
                .child(
                    div()
                        .id("pull-request-description")
                        .max_h(px(300.))
                        .overflow_y_scroll()
                        .text_color(rgb(MUTED))
                        .child(self.description),
                ),
        )
    }
}

fn row() -> Div {
    div().flex().items_center().gap_2()
}
fn icon_actions() -> Div {
    row().gap(px(UI_SPACING)).flex_shrink_0()
}
fn column() -> Div {
    div().flex().flex_col()
}
fn file_header_row() -> Div {
    row()
        .h(px(FILE_HEADER_HEIGHT))
        .flex_shrink_0()
        .px(px(UI_SPACING))
        .gap(px(UI_SPACING))
        .bg(rgb(SUBPANEL))
        .border_b_1()
        .border_color(rgb(BORDER))
        .text_size(px(12.))
        .line_height(px(HEADER_PILL_LINE_HEIGHT))
}
fn header_pill() -> Div {
    row()
        .h(px(HEADER_PILL_HEIGHT))
        .min_w_0()
        .px(px(HEADER_PILL_PADDING))
        .gap(px(6.))
        .rounded(px(CONTROL_RADIUS))
        .bg(rgb(SURFACE))
        .border_1()
        .border_color(rgb(CONTROL_BORDER))
        .text_size(px(12.))
        .line_height(px(HEADER_PILL_LINE_HEIGHT))
}
fn selected_surface<T: Styled>(element: T) -> T {
    element
        .bg(rgb(SELECTED))
        .border_1()
        .border_color(rgb(SELECTED_BORDER))
        .text_color(rgb(TEXT))
        .shadow_sm()
}
fn button_base(id: impl Into<ElementId>, label: impl Into<SharedString>) -> BaseButton {
    let label = label.into();
    BaseButton::new(id)
        .h(px(CONTROL_HEIGHT))
        .px(px(CONTROL_PADDING))
        .gap(px(UI_SPACING))
        .flex_shrink_0()
        .rounded(px(CONTROL_RADIUS))
        .border_1()
        .border_color(rgb(CONTROL_BORDER))
        .bg(rgb(SURFACE))
        .text_color(rgb(TEXT))
        .text_size(px(12.))
        .focus_visible(|d| d.border_color(rgb(FOCUS)))
        .styles(|styles| styles.disabled(|d| d.opacity(0.5)))
        .accessibility_label(label.clone())
        .child(label)
}

fn primary_button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> BaseButton {
    button_base(id, label)
        .bg(rgb(TEXT))
        .text_color(rgb(BG))
        .border_color(rgb(TEXT))
        .hover(|d| d.bg(rgb(0xffffff)).border_color(rgb(0xffffff)))
}

fn choice_button(id: impl Into<ElementId>, selected: bool) -> BaseButton {
    BaseButton::new(id)
        .w_full()
        .h(px(CHOICE_ROW_HEIGHT))
        .flex_shrink_0()
        .min_w_0()
        .px(px(CONTROL_PADDING))
        .gap(px(UI_SPACING))
        .justify_start()
        .text_left()
        .text_size(px(12.))
        .rounded(px(CONTROL_RADIUS))
        .border_1()
        .border_color(rgba(0x00000000))
        .when(selected, selected_surface)
        .hover(move |d| d.bg(rgb(if selected { SELECTED } else { HOVER })))
        .focus_visible(|d| d.border_color(rgb(FOCUS)))
        .styles(|styles| styles.disabled(|d| d.opacity(0.5)))
        .aria_selected(selected)
}

fn sidebar_item(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    icon: impl IntoElement,
) -> BaseButton {
    let label = label.into();
    BaseButton::new(id)
        .h_full()
        .flex_1()
        .flex_shrink_1()
        .min_w_0()
        .px_2()
        .justify_start()
        .gap_2()
        .rounded(px(CONTROL_RADIUS))
        .border_1()
        .border_color(rgba(0x00000000))
        .focus_visible(|d| d.border_color(rgb(FOCUS)))
        .styles(|styles| styles.disabled(|d| d.opacity(0.5)))
        .accessibility_label(label.clone())
        .child(icon)
        .child(div().flex_1().min_w_0().truncate().child(label))
}

fn icon_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    icon: IconName,
) -> BaseButton {
    let label = label.into();
    BaseButton::new(id)
        .size(px(ICON_BUTTON_SIZE))
        .flex_shrink_0()
        .rounded(px(CONTROL_RADIUS))
        .border_1()
        .border_color(rgba(0x00000000))
        .text_color(rgb(MUTED))
        .hover(|d| d.bg(rgb(HOVER)).text_color(rgb(TEXT)))
        .focus_visible(|d| d.border_color(rgb(FOCUS)))
        .styles(|styles| styles.disabled(|d| d.opacity(0.5)))
        .child(Icon::new(icon).size(px(14.)))
        .accessibility_label(label.clone())
        .tooltip(move |window, cx| Tooltip::new(label.clone()).build(window, cx))
}

fn file_status_icon(id: impl Into<ElementId>, status: &str) -> Stateful<Div> {
    let (icon, label, color) = match status {
        "A" => (IconName::FilePlus, "Added", SUCCESS),
        "M" => (IconName::FilePenLine, "Modified", WARNING),
        "D" => (IconName::FileMinus, "Deleted", DANGER),
        "R" => (IconName::FileSymlink, "Renamed", BLUE),
        "C" => (IconName::Copy, "Copied", BLUE),
        "T" => (IconName::FileType, "File type changed", PURPLE),
        "U" => (IconName::FileExclamationPoint, "Unmerged", DANGER),
        "?" => (IconName::FileQuestionMark, "Untracked", BLUE),
        _ => (IconName::FileQuestionMark, "Unknown status", MUTED),
    };
    div()
        .id(id)
        .size(px(14.))
        .flex_shrink_0()
        .role(accesskit::Role::Image)
        .aria_label(label)
        .child(Icon::new(icon).size(px(14.)).text_color(rgb(color)))
}

fn viewed_button(viewed: bool, disabled: bool) -> BaseButton {
    BaseButton::new("viewed")
        .h(px(CONTROL_HEIGHT))
        .px(px(CONTROL_PADDING))
        .gap(px(UI_SPACING))
        .flex_shrink_0()
        .rounded(px(CONTROL_RADIUS))
        .border_1()
        .border_color(rgb(CONTROL_BORDER))
        .bg(rgb(SURFACE))
        .text_color(rgb(TEXT))
        .text_size(px(13.))
        .font_weight(FontWeight::SEMIBOLD)
        .when(viewed, selected_surface)
        .when(!disabled, |d| {
            d.hover(move |d| d.bg(rgb(if viewed { SELECTED } else { HOVER })))
        })
        .focus_visible(|d| d.border_color(rgb(FOCUS)))
        .styles(|styles| styles.disabled(|d| d.opacity(0.5)))
        .disabled(disabled)
        .role(accesskit::Role::CheckBox)
        .aria_toggled(if viewed {
            accesskit::Toggled::True
        } else {
            accesskit::Toggled::False
        })
        .accessibility_label("Viewed")
        .child(
            div()
                .size(px(14.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(2.))
                .border_1()
                .border_color(rgb(MUTED))
                .when(viewed, |d| {
                    d.bg(rgb(0x737373)).border_color(rgb(FOCUS)).child(
                        Icon::new(IconName::Check)
                            .size(px(12.))
                            .text_color(rgb(0xffffff)),
                    )
                }),
        )
        .child("Viewed")
}

fn sidebar_row(id: impl Into<ElementId>, selected: bool, disabled: bool) -> Stateful<Div> {
    row()
        .id(id)
        .h(px(LIST_ROW_HEIGHT))
        .flex_shrink_0()
        .gap(px(UI_SPACING))
        .pr(px(UI_SPACING))
        .rounded(px(CONTROL_RADIUS))
        .border_1()
        .border_color(rgba(0x00000000))
        .when(selected, selected_surface)
        .hover(move |d| {
            if selected || disabled {
                d
            } else {
                d.bg(rgb(HOVER))
            }
        })
}

impl Rev {
    fn review_header(&self, cx: &mut Context<Self>) -> TitleBar {
        let busy = self.loading || self.submitting;
        TitleBar::new()
            .h(px(HEADER_HEIGHT))
            .pl_0()
            .bg(rgb(BG))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .w(px(SIDEBAR_WIDTH))
                    .h_full()
                    .flex_shrink_0()
                    .bg(rgb(PANEL))
                    .border_r_1()
                    .border_color(rgb(BORDER)),
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
                                        .text_color(rgb(MUTED)),
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
                                    if let Some(pr) = &review.pull_request {
                                        let title = format!("{}  #{}", pr.title, pr.number);
                                        let description = if pr.body.trim().is_empty() {
                                            "No description provided.".to_owned()
                                        } else {
                                            pr.body.trim_end().to_owned()
                                        };
                                        d.child(
                                            header_pill()
                                                .id("pr-title-pill")
                                                .debug_selector(|| "pr-title-pill".into())
                                                .max_w_full()
                                                .hover(|d| {
                                                    d.bg(rgb(HOVER))
                                                        .border_color(rgb(SELECTED_BORDER))
                                                })
                                                .child(
                                                    div().min_w_0().child(PrDescription {
                                                        id: format!(
                                                            "pr-description-{}-{}",
                                                            review.request.repo, pr.number
                                                        )
                                                        .into(),
                                                        title,
                                                        description,
                                                    }),
                                                ),
                                        )
                                    } else {
                                        d.child(header_pill().text_color(rgb(MUTED)).child(
                                            if review.request.mode.as_deref() == Some("working") {
                                                "Working tree"
                                            } else {
                                                "Branch review"
                                            },
                                        ))
                                    }
                                }),
                        )
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
                                        .disabled(busy)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.refresh(window, cx)
                                        })),
                                )
                                .child(
                                    icon_button("fetch", "Fetch", IconName::CloudDownload)
                                        .disabled(busy)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.fetch(window, cx)
                                        })),
                                ),
                        )
                    }),
            )
    }

    fn sidebar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let blocked = self.loading || self.workspace_busy;
        let mut projects = column().gap_1();
        for project in self.workspace.projects.clone() {
            let repo = project.repo.clone();
            let collapse_repo = repo.clone();
            let remove_repo = repo.clone();
            let hover_group = SharedString::from(format!("project-row-{repo}"));
            let active = self
                .workspace
                .active_tab()
                .is_some_and(|tab| tab.request.repo == repo);
            let collapsed = project.collapsed;
            let mut group = column().gap(px(2.)).child(
                sidebar_row(hover_group.clone(), false, blocked)
                    .group(hover_group.clone())
                    .child(
                        sidebar_item(
                            SharedString::from(format!("project-{repo}")),
                            project.name.clone(),
                            FolderIcon {
                                id: repo.clone().into(),
                                expanded: !collapsed,
                            },
                        )
                        .font_weight(FontWeight::MEDIUM)
                        .aria_expanded(!collapsed)
                        .disabled(blocked)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.workspace_action(
                                json!({ "type": "collapseProject", "repo": collapse_repo, "collapsed": !collapsed }),
                                false, window, cx,
                            )
                        })),
                    )
                    .child(
                        icon_actions()
                            .invisible()
                            .group_hover(hover_group, |d| d.visible())
                            .child(
                                icon_button(
                                    SharedString::from(format!("remove-{repo}")),
                                    format!("Remove {} from sidebar", project.name),
                                    IconName::Close,
                                )
                                .disabled(blocked)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.workspace_action(
                                        json!({ "type": "removeProject", "repo": remove_repo }),
                                        active, window, cx,
                                    )
                                })),
                            )
                            .child(
                                icon_button(
                                    SharedString::from(format!("add-{repo}")),
                                    format!("Open a branch or PR in {}", project.name),
                                    IconName::Plus,
                                )
                                .disabled(blocked)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_branch_modal(repo.clone(), window, cx)
                                })),
                            ),
                    ),
            );
            if !collapsed {
                for tab in project.tabs {
                    let working_tree = tab.kind == crate::workspace_state::TabKind::Current;
                    let id = tab.id.clone();
                    let close = id.clone();
                    let active = self.workspace.active_tab_id.as_ref() == Some(&tab.id);
                    let request = tab.request.clone();
                    let file = tab.file.clone();
                    let hover_group = SharedString::from(format!("review-row-{id}"));

                    group = group.child(
                        sidebar_row(hover_group.clone(), active, blocked)
                            .group(hover_group.clone())
                            .pl(px(16.))
                            .child(
                                sidebar_item(
                                    SharedString::from(format!("tab-{id}")),
                                    tab.label.clone(),
                                    Icon::new(if working_tree {
                                        IconName::MonitorDot
                                    } else {
                                        IconName::GitBranch
                                    })
                                    .size(px(14.))
                                    .flex_shrink_0()
                                    .text_color(rgb(MUTED)),
                                )
                                .aria_selected(active)
                                .disabled(blocked)
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.load_review(request.clone(), file.clone(), window, cx)
                                    },
                                )),
                            )
                            .when(!working_tree, |row| {
                                row.child(
                                    icon_button(
                                        SharedString::from(format!("close-{id}")),
                                        format!("Close {}", tab.label),
                                        IconName::Close,
                                    )
                                    .invisible()
                                    .group_hover(hover_group, |button| button.visible())
                                    .disabled(blocked)
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            this.workspace_action(
                                                json!({ "type": "closeTab", "id": close }),
                                                active,
                                                window,
                                                cx,
                                            )
                                        },
                                    )),
                                )
                            }),
                    );
                }
            }
            projects = projects.child(group);
        }
        column()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .flex_shrink_0()
            .bg(rgb(PANEL))
            .border_r_1()
            .border_color(rgb(BORDER))
            .child(
                row()
                    .h(px(ICON_BUTTON_SIZE + UI_SPACING * 2.))
                    .flex_shrink_0()
                    .pl(px(UI_SPACING + 8.))
                    .pr(px(UI_SPACING))
                    .justify_between()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(12.))
                            .text_color(rgb(MUTED))
                            .child("Repos"),
                    )
                    .child(
                        icon_button("add-project", "Open repository", IconName::Plus)
                            .disabled(blocked)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.pick_folder(window, cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .id("projects")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(UI_SPACING))
                    .pb(px(UI_SPACING))
                    .child(projects),
            )
            .into_any_element()
    }

    fn diff_view_header(&self, cx: &mut Context<Self>) -> AnyElement {
        row()
            .id("diff-view-header")
            .bg(rgb(SUBPANEL))
            .h(px(42.))
            .flex_shrink_0()
            .px(px(UI_SPACING))
            .gap(px(UI_SPACING))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                column()
                    .flex_1()
                    .min_w_0()
                    // Match the content inset inside the repo pill and search field.
                    .pl(px(HEADER_PILL_PADDING + 1.))
                    .gap(px(2.))
                    .line_height(px(16.))
                    .when_some(self.review.as_ref(), |d, review| {
                        let comparison = review.comparison_label();
                        let added = self.file_tree.added;
                        let removed = self.file_tree.removed;
                        d.child(
                            div()
                                .id("branch-comparison")
                                .min_w_0()
                                .truncate()
                                .text_color(rgb(MUTED))
                                .child(comparison),
                        )
                        .child(
                            row()
                                .flex_shrink_0()
                                .text_size(px(12.))
                                .line_height(px(14.))
                                .child(
                                    div()
                                        .text_color(rgb(MUTED))
                                        .child(format!("{} files", review.files.len())),
                                )
                                .child(div().text_color(rgb(SUCCESS)).child(format!("+{added}")))
                                .child(div().text_color(rgb(DANGER)).child(format!("−{removed}"))),
                        )
                    }),
            )
            .child(
                icon_button(
                    "diff-mode",
                    if self.split {
                        "Switch to unified diff"
                    } else {
                        "Switch to split diff"
                    },
                    if self.split {
                        IconName::Rows2
                    } else {
                        IconName::Columns2
                    },
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.split = !this.split;
                    let mode = if this.split {
                        DiffMode::Split
                    } else {
                        DiffMode::Unified
                    };
                    this.diff.update(cx, |diff, cx| diff.set_mode(mode, cx));
                    cx.notify();
                })),
            )
            .child(
                icon_button(
                    "wrap",
                    if self.wrap {
                        "Disable line wrapping"
                    } else {
                        "Enable line wrapping"
                    },
                    IconName::TextWrap,
                )
                .selected(self.wrap)
                .aria_toggled(if self.wrap {
                    accesskit::Toggled::True
                } else {
                    accesskit::Toggled::False
                })
                .when(self.wrap, selected_surface)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.wrap = !this.wrap;
                    cx.notify();
                })),
            )
            .into_any_element()
    }

    fn file_tree_row(&self, entry: &TreeEntry, cx: &mut Context<Self>) -> AnyElement {
        #[cfg(test)]
        self.file_rows_rendered
            .borrow_mut()
            .push(entry.path.clone());
        let repo = self
            .review
            .as_ref()
            .map(|review| review.request.repo.as_str())
            .unwrap_or_default();
        let loading = self.loading;
        let path = entry.path.clone();
        let selected =
            matches!(&entry.kind, EntryKind::File(_)) && self.file.as_ref() == Some(&path);
        let viewed = self.state.viewed.contains_key(&path);
        let label = div()
            .flex_1()
            .min_w_0()
            .truncate()
            .child(entry.label.clone());
        let item = BaseButton::new(SharedString::from(format!("file-tree-{repo}-{path}")))
            .w_full()
            .min_w_0()
            .h(px(LIST_ROW_HEIGHT))
            .flex_shrink_0()
            .justify_start()
            .gap(px(6.))
            .pl(px(8. + entry.depth as f32 * 12.))
            .pr_2()
            .rounded(px(CONTROL_RADIUS))
            .border_1()
            .border_color(rgba(0x00000000))
            .text_size(px(12.))
            .when(selected, selected_surface)
            .hover(move |d| {
                if selected || loading {
                    d
                } else {
                    d.bg(rgb(HOVER))
                }
            })
            .focus_visible(|d| d.border_color(rgb(FOCUS)))
            .styles(|styles| styles.disabled(|d| d.opacity(0.5)))
            .disabled(loading)
            .accessibility_label(path.clone());
        let item = match &entry.kind {
            EntryKind::Folder { expanded } => item
                .aria_expanded(*expanded)
                .child(
                    Icon::new(if *expanded {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .size(px(12.))
                    .flex_shrink_0()
                    .text_color(rgb(MUTED)),
                )
                .child(FolderIcon {
                    id: format!("file-folder-{repo}-{path}").into(),
                    expanded: *expanded,
                })
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| this.toggle_file_folder(&path, cx))),
            EntryKind::File(file) => {
                item.aria_selected(selected)
                    .child(div().size(px(12.)).flex_shrink_0())
                    .child(file_status_icon(
                        SharedString::from(format!("file-status-{repo}-{path}")),
                        &file.status,
                    ))
                    .child(label)
                    .when(viewed, |item| {
                        item.child(
                            Icon::new(IconName::Check)
                                .size(px(14.))
                                .flex_shrink_0()
                                .text_color(rgb(SUCCESS)),
                        )
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.load_file(path.clone(), window, cx)
                    }))
            }
        };
        // Include the gap in the fixed row height so viewport calculations stay exact.
        div()
            .w_full()
            .h(px(LIST_ROW_HEIGHT + 2.))
            .pb(px(2.))
            .child(item)
            .into_any_element()
    }

    fn file_list(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let entries = self.file_tree_entries();
        let empty = entries.is_empty();
        let count = entries.len();
        let body = div()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .p(px(UI_SPACING))
            .when(empty, |d| {
                d.child(div().p_2().text_size(px(12.)).text_color(rgb(MUTED)).child(
                    if self.file_tree_filter.is_empty() {
                        "No changed files"
                    } else {
                        "No matching files"
                    },
                ))
            })
            .when(!empty, |d| {
                d.child(
                    uniform_list(
                        "files-scroll",
                        count,
                        cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                            range
                                .map(|index| this.file_tree_row(&entries[index], cx))
                                .collect::<Vec<_>>()
                        }),
                    )
                    .size_full()
                    .track_scroll(&self.file_scroll),
                )
            });
        column()
            .w(px(210.))
            .bg(rgb(SUBPANEL))
            .h_full()
            .flex_shrink_0()
            .border_r_1()
            .border_color(rgb(BORDER))
            .child(
                file_header_row().child(
                    Input::new(&self.filter)
                        .id("filter-files")
                        .focus_bordered(false)
                        .small()
                        .min_h(px(CONTROL_HEIGHT))
                        .max_h(px(CONTROL_HEIGHT))
                        .px(px(CONTROL_PADDING))
                        .py_0()
                        .gap(px(6.))
                        .rounded(px(CONTROL_RADIUS))
                        .bg(rgb(SURFACE))
                        .border_1()
                        .border_color(rgb(BORDER))
                        .text_size(px(12.))
                        .line_height(px(HEADER_PILL_LINE_HEIGHT))
                        .prefix(
                            Icon::new(IconName::Search)
                                .size(px(14.))
                                .text_color(rgb(MUTED)),
                        )
                        .cleanable(true)
                        .aria_label("Find a file"),
                ),
            )
            .child(body)
            .into_any_element()
    }

    fn diff_view(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let owner = cx.weak_entity();
        let add_owner = owner.clone();
        let viewed = self
            .file
            .as_ref()
            .is_some_and(|p| self.state.viewed.contains_key(p));
        let file_counts = self
            .file
            .as_deref()
            .and_then(|path| self.file_tree.file(path))
            .map(|file| (file.added, file.removed));
        let file_header = file_header_row()
            .child(
                row()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(12.))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(self.file.clone().unwrap_or_default()),
                    )
                    .when_some(file_counts, |d, (added, removed)| {
                        d.child(
                            row()
                                .flex_shrink_0()
                                .child(div().text_color(rgb(SUCCESS)).child(format!("+{added}")))
                                .child(div().text_color(rgb(DANGER)).child(format!("−{removed}"))),
                        )
                    }),
            )
            .child(
                viewed_button(
                    viewed,
                    self.loading
                        || self.patch_loading
                        || self.submitting
                        || self.patch.digest.is_empty(),
                )
                .on_click(cx.listener(|this, _, window, cx| this.mark_viewed(window, cx))),
            );
        let content = if self.loading || self.patch_loading {
            div()
                .p_6()
                .text_color(rgb(MUTED))
                .child("Loading…")
                .into_any_element()
        } else if let Some(notice) = &self.notice {
            div()
                .p_6()
                .text_color(rgb(MUTED))
                .child(notice.clone())
                .into_any_element()
        } else if self.file.is_none() {
            div()
                .p_6()
                .text_color(rgb(MUTED))
                .child("No changes")
                .into_any_element()
        } else {
            Diff::new(&self.diff)
                .size_full()
                .border_0()
                .header_visible(false)
                .hunk_separator(DiffHunkSeparator::Simple)
                .soft_wrap(self.wrap)
                .text_size(px(12.))
                .on_add_annotation(move |range, window, cx| {
                    let _ = add_owner.update(cx, |this, cx| {
                        this.selection = Selection::from_range(range);
                        this.selection_menu = window.mouse_position() + point(px(12.), px(18.));
                        this.diff.update(cx, |diff, cx| {
                            diff.set_selected_lines(Some(range.clone()), cx)
                        });
                        cx.notify();
                    });
                })
                .into_any_element()
        };
        column()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(file_header)
            .child(
                div()
                    .id("diff-surface")
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(content),
            )
            .into_any_element()
    }

    fn form_field(&self, index: usize, title: &str, placeholder: &str) -> AnyElement {
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
                Input::new(&self.fields[index])
                    .id(("form-field", index))
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
                    .aria_label(placeholder.to_string()),
            )
            .into_any_element()
    }

    fn dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(modal) = self.modal.clone() else {
            return div().into_any_element();
        };
        let title = "Open branch or PR in";
        let mut body = column().gap(px(POPOVER_PADDING));
        let mut footer = icon_actions().justify_end();
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
                            .child(self.form_field(0, "Base", "Base branch; leave blank to detect"))
                            .child(div().flex().w(relative(0.65)).min_w_0().child(
                                self.form_field(
                                    1,
                                    "Branch or PR",
                                    "Branch, PR number or URL; leave blank for HEAD",
                                ),
                            )),
                    ),
            );

        if let Some(error) = &self.error {
            body = body.child(div().text_color(rgb(DANGER)).child(error.clone()));
        }
        {
            let label = if self.submitting {
                "Working…"
            } else {
                "Open diff"
            };
            footer = footer.child(
                primary_button("submit-modal", label)
                    .disabled(self.submitting)
                    .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
            );
        }
        let max_height = (window.viewport_size().height - px(110.)).max(px(220.));
        div()
            .absolute()
            .inset_0()
            .bg(rgba(0x00000099))
            .flex()
            .items_center()
            .justify_center()
            .occlude()
            .child(
                column()
                    .w(px(650.))
                    .max_w_full()
                    .max_h(max_height)
                    .m_6()
                    .bg(rgb(SURFACE))
                    .border_1()
                    .border_color(rgb(CONTROL_BORDER))
                    .rounded(px(CONTROL_RADIUS))
                    .shadow_lg()
                    .track_focus(&self.modal_focus)
                    .key_context("PatchModal")
                    .on_action(
                        cx.listener(|this, _: &CloseModal, window, cx| {
                            this.close_modal(window, cx)
                        }),
                    )
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
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.close_modal(window, cx)
                                    })),
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
                    .child(
                        div()
                            .px(px(POPOVER_PADDING))
                            .py(px(UI_SPACING))
                            .flex_shrink_0()
                            .border_t_1()
                            .border_color(rgb(BORDER))
                            .child(footer),
                    )
                    .focus_trap("modal-trap", &self.modal_focus),
            )
            .into_any_element()
    }
}

impl Render for Rev {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content =
            if self.review.is_some() {
                column()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(self.diff_view_header(cx))
                    .child(
                        row()
                            .items_start()
                            .gap_0()
                            .flex_1()
                            .min_h_0()
                            .child(self.file_list(cx))
                            .child(self.diff_view(cx)),
                    )
                    .into_any_element()
            } else {
                column()
                    .flex_1()
                    .h_full()
                    .items_center()
                    .justify_center()
                    .gap_4()
                    .child(
                        div()
                            .text_size(px(24.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Open a review"),
                    )
                    .child(row().child(
                        primary_button("welcome-open", "Open folder").on_click(
                            cx.listener(|this, _, window, cx| this.pick_folder(window, cx)),
                        ),
                    ))
                    .into_any_element()
            };
        let status = if self.modal.is_none() {
            self.error.clone()
        } else {
            None
        };
        column()
            .id("rev")
            .relative()
            .size_full()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .text_size(px(13.))
            .font_family(".AppleSystemUIFont")
            .track_focus(&self.focus)
            .key_context("Rev")
            .on_action(
                cx.listener(|this, _: &OpenProject, window, cx| this.pick_folder(window, cx)),
            )
            .on_action(cx.listener(|this, _: &Refresh, window, cx| this.refresh(window, cx)))
            .on_action(
                cx.listener(|this, _: &NextFile, window, cx| this.navigate_file(1, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &PreviousFile, window, cx| {
                    this.navigate_file(-1, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &MarkViewed, window, cx| {
                if this.modal.is_none() {
                    this.mark_viewed(window, cx);
                }
            }))
            .child(self.review_header(cx))
            .child(
                row()
                    .items_start()
                    .gap_0()
                    .flex_1()
                    .min_h_0()
                    .child(self.sidebar(cx))
                    .child(content),
            )
            .when_some(status, |d, error| {
                d.child(
                    row()
                        .p(px(UI_SPACING))
                        .gap(px(UI_SPACING))
                        .border_t_1()
                        .border_color(rgb(BORDER))
                        .child(div().flex_1().text_color(rgb(DANGER)).child(error))
                        .child(
                            icon_button("dismiss-error", "Dismiss error", IconName::Close)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.error = None;
                                    cx.notify();
                                })),
                        ),
                )
            })
            .when(
                self.modal.is_none() && self.selection.is_some() && !self.selecting,
                |d| {
                    d.child(deferred(
                        anchored()
                            .position(self.selection_menu)
                            .snap_to_window_with_margin(px(8.)),
                    ))
                },
            )
            .when(self.modal.is_some(), |d| d.child(self.dialog(window, cx)))
    }
}

#[cfg(test)]
mod performance_tests {
    use super::{FileTreeCache, Rev};
    use crate::{
        diff::DiffState,
        model::{File, FilePatch, Review, ReviewState},
        workspace_state::WorkspaceState,
    };
    use gpui_kit::component::input::InputState;
    use gpui_kit::{
        Context, Entity, IntoElement, Render, ScrollStrategy, TestAppContext, VisualTestContext,
        Window, div, prelude::*, px,
    };

    struct FileListHost {
        view: Entity<Rev>,
    }

    struct HeaderHost {
        view: Entity<Rev>,
    }

    impl Render for HeaderHost {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let header = self.view.update(cx, |view, cx| view.review_header(cx));
            div().w(px(1000.)).h(px(400.)).child(header)
        }
    }

    impl Render for FileListHost {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let list = self.view.update(cx, |view, cx| view.file_list(cx));
            div().w(px(210.)).h(px(300.)).child(list)
        }
    }

    // Keep UI tests independent of the user's persisted workspace and backend.
    fn fixture(window: &mut Window, cx: &mut Context<Rev>) -> Rev {
        let files: Vec<_> = (0..20_000)
            .map(|index| File {
                path: format!("file{index:05}.rs"),
                status: "M".into(),
                added: 20,
                removed: 0,
            })
            .collect();
        let mut tree = FileTreeCache::default();
        tree.set_files(&files);
        Rev {
            backend: None,
            workspace: WorkspaceState::default(),
            data_dir: Default::default(),
            review: Some(Review {
                files,
                ..Default::default()
            }),
            review_snapshot_id: None,
            state: ReviewState::default(),
            file: None,
            patch: FilePatch::default(),
            diff: cx.new(|cx| DiffState::new([], cx)),
            selection: None,
            selecting: false,
            selection_menu: Default::default(),
            filter: cx.new(|cx| InputState::new(window, cx)),
            collapsed_file_folders: Default::default(),
            filtered_file_folders: Default::default(),
            file_tree_filter: String::new(),
            file_tree: tree,
            file_scroll: Default::default(),
            file_rows_rendered: Default::default(),
            fields: Vec::new(),
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
            error: None,
            notice: None,
            split: false,
            wrap: false,
            working_tree_poll_inflight: false,
            working_tree_refreshing: false,
            working_tree_revisions: Default::default(),
            _working_tree_poll: None,
            _subscriptions: Vec::new(),
        }
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            _ = window.draw(cx);
        });
    }

    #[gpui::test]
    fn welcome_screen_opens_and_can_choose_a_repository(cx: &mut TestAppContext) {
        use gpui_kit::test::TestQueryExt;

        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::diff::init(cx);
            crate::theme::init(cx);
        });
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut app = fixture(window, cx);
            app.review = None;
            app
        });
        draw(cx);
        let open_folder = cx.update(|window, _| window.find_by_label("Open folder").bounds());
        cx.simulate_mouse_move(open_folder.center(), None, Default::default());
        draw(cx);
        cx.simulate_click(open_folder.center(), Default::default());
        assert!(cx.did_prompt_for_paths());
        cx.simulate_path_prompt_response(|options| {
            assert!(options.directories);
            assert!(!options.files);
            None
        });
        draw(cx);
        cx.update(|_, cx| {
            let app = view.read(cx);
            assert!(!app.folder_picker_open);
            assert!(app.review.is_none());
            assert!(app.error.is_none());
        });
    }

    #[gpui::test]
    fn twenty_thousand_files_render_only_the_viewport_and_scroll_to_the_last_file(
        cx: &mut TestAppContext,
    ) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::diff::init(cx);
            crate::theme::init(cx);
        });
        let (host, cx) = cx.add_window_view(|window, cx| FileListHost {
            view: cx.new(|cx| fixture(window, cx)),
        });
        let view = cx.update(|_, cx| host.read(cx).view.clone());
        draw(cx);
        cx.update(|_, cx| {
            let app = view.read(cx);
            let rendered = app.file_rows_rendered.take();
            assert!(
                !rendered.is_empty() && rendered.len() < 100,
                "rendered {} rows",
                rendered.len()
            );
            assert_eq!(
                app.file_scroll
                    .0
                    .borrow()
                    .last_item_size
                    .unwrap()
                    .contents
                    .height,
                px(28. * 20_000.)
            );
        });
        view.update(cx, |app, cx| {
            app.file_scroll
                .scroll_to_item(19_999, ScrollStrategy::Nearest);
            cx.notify();
        });
        draw(cx);
        cx.update(|_, cx| {
            let app = view.read(cx);
            let rendered = app.file_rows_rendered.take();
            assert!(
                !rendered.is_empty() && rendered.len() < 100,
                "rendered {} rows after scroll",
                rendered.len()
            );
            assert!(
                rendered.iter().any(|path| path == "file19999.rs"),
                "rendered {rendered:?}"
            );
        });
        view.update(cx, |app, cx| {
            app.file_tree_filter = "file19999".into();
            app.file_tree.set_filter(&app.file_tree_filter);
            app.file_scroll.scroll_to_item(0, ScrollStrategy::Top);
            cx.notify();
        });
        draw(cx);
        view.update(cx, |app, _| {
            assert_eq!(app.file_tree_entries().len(), 1);
            assert!(app.file_rows_rendered.borrow().len() < 10);
            assert_eq!(app.file_scroll.0.borrow().base_handle.offset().y, px(0.));
        });
    }

    #[gpui::test]
    fn unchanged_working_tree_polls_do_not_notify_the_view(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::diff::init(cx);
        });
        let (host, cx) = cx.add_window_view(|window, cx| FileListHost {
            view: cx.new(|cx| fixture(window, cx)),
        });
        let view = cx.update(|_, cx| host.read(cx).view.clone());
        view.update(cx, |app, _| {
            let review = app.review.as_mut().unwrap();
            review.request = crate::model::ReviewRequest::working_tree("/test-repo");
            review.branch = "main".into();
            review.branch_head_sha = "abc".into();
            app.workspace.remember_review(review, &review.request);
            app.working_tree_revisions
                .insert("/test-repo".into(), "same".into());
        });
        cx.run_until_parked();
        let notifications = std::rc::Rc::new(std::cell::Cell::new(0));
        let _subscription = cx.update(|_, cx| {
            let notifications = notifications.clone();
            cx.observe(&view, move |_, _| {
                notifications.set(notifications.get() + 1)
            })
        });
        for _ in 0..3 {
            cx.update(|window, cx| {
                view.update(cx, |app, cx| {
                    app.apply_working_tree_states(
                        Some("/test-repo"),
                        vec![Some(crate::model::WorkingTreeState {
                            repo: "/test-repo".into(),
                            branch: "main".into(),
                            sha: "abc".into(),
                            revision: "same".into(),
                        })],
                        window,
                        cx,
                    );
                })
            });
        }
        assert_eq!(notifications.get(), 0);
        assert!(!cx.update(|_, cx| view.read(cx).working_tree_refreshing));
    }

    #[gpui::test]
    fn opening_a_repository_prompts_once_and_cancelling_preserves_the_review(
        cx: &mut TestAppContext,
    ) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::diff::init(cx);
        });
        let (host, cx) = cx.add_window_view(|window, cx| FileListHost {
            view: cx.new(|cx| fixture(window, cx)),
        });
        let view = cx.update(|_, cx| host.read(cx).view.clone());
        let before = cx.update(|_, cx| view.read(cx).review.clone());
        // Repeated clicks must not queue multiple native dialogs.
        for _ in 0..2 {
            cx.update(|window, cx| view.update(cx, |app, cx| app.pick_folder(window, cx)));
        }
        assert!(cx.did_prompt_for_paths());
        cx.update(|_, cx| {
            let app = view.read(cx);
            assert!(app.folder_picker_open);
            assert!(app.modal.is_none());
        });
        cx.simulate_path_prompt_response(|options| {
            assert!(options.directories);
            assert!(!options.files);
            assert!(!options.multiple);
            None
        });
        cx.run_until_parked();
        assert!(!cx.did_prompt_for_paths());
        cx.update(|_, cx| {
            let app = view.read(cx);
            assert!(!app.folder_picker_open);
            assert!(!app.loading);
            assert!(app.modal.is_none());
            assert_eq!(app.review, before);
            assert!(app.error.is_none());
        });
        // Cancelling releases the guard so the picker can be opened again.
        cx.update(|window, cx| view.update(cx, |app, cx| app.pick_folder(window, cx)));
        assert!(cx.did_prompt_for_paths());
        cx.simulate_path_prompt_response(|_| None);
        cx.run_until_parked();
    }

    #[gpui::test]
    fn pr_description_opens_only_over_title_text(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::diff::init(cx);
            crate::theme::init(cx);
            // Check final geometry without depending on animation wall time.
            cx.set_reduce_motion(true);
        });
        let (host, cx) = cx.add_window_view(|window, cx| HeaderHost {
            view: cx.new(|cx| {
                let mut app = fixture(window, cx);
                let review = app.review.as_mut().unwrap();
                review.name = "patch".into();
                review.pull_request = Some(crate::model::PullRequest {
                    number: 14,
                    title: "Refine the UI".into(),
                    body: "The PR description.".into(),
                    head_ref_name: "feature".into(),
                    base_ref_name: "main".into(),
                    comparison_base_ref_name: None,
                });
                app
            }),
        });
        let view = cx.update(|_, cx| host.read(cx).view.clone());
        draw(cx);
        let pill = cx.debug_bounds("pr-title-pill").unwrap();
        let text = cx.debug_bounds("pr-title-text").unwrap();
        let actions = cx.debug_bounds("review-header-actions").unwrap();
        assert!(
            pill.right() + px(20.) < actions.left(),
            "title pill should hug its text"
        );
        for position in [
            gpui_kit::point((pill.right() + actions.left()) / 2., pill.center().y),
            gpui_kit::point(pill.left() + px(3.), pill.center().y),
            actions.center(),
        ] {
            cx.simulate_mouse_move(position, None, Default::default());
            cx.executor()
                .advance_clock(std::time::Duration::from_millis(200));
            draw(cx);
            assert!(cx.debug_bounds("pr-description-content").is_none());
        }
        cx.simulate_mouse_move(text.center(), None, Default::default());
        draw(cx);
        draw(cx);
        let settled = cx.debug_bounds("pr-description-content").unwrap();
        assert_eq!(settled.left(), pill.left(), "align with the title pill");
        assert!(
            (settled.top() - pill.bottom() - px(2.)).abs() <= px(1.),
            "leave a 2px gap, allowing for pixel rounding: {pill:?}, {settled:?}"
        );

        // Crossing the gap onto the card keeps the description available.
        cx.simulate_mouse_move(settled.center(), None, Default::default());
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(200));
        draw(cx);
        assert!(cx.debug_bounds("pr-description-content").is_some());

        cx.simulate_mouse_move(
            gpui_kit::point(px(950.), px(350.)),
            None,
            Default::default(),
        );
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(200));
        draw(cx);
        assert!(cx.debug_bounds("pr-description-content").is_none());

        view.update(cx, |app, cx| {
            app.review
                .as_mut()
                .unwrap()
                .pull_request
                .as_mut()
                .unwrap()
                .title = "A long pull request title ".repeat(30);
            cx.notify();
        });
        draw(cx);
        let pill = cx.debug_bounds("pr-title-pill").unwrap();
        let actions = cx.debug_bounds("review-header-actions").unwrap();
        assert!(
            pill.right() <= actions.left(),
            "long titles must leave room for actions"
        );
    }

    #[gpui::test]
    fn pr_description_fades_out_and_reentry_cancels_exit(cx: &mut TestAppContext) {
        struct Host;
        impl Render for Host {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div()
                    .size_full()
                    .p_4()
                    .child(
                        super::header_pill()
                            .w(px(200.))
                            .child(super::PrDescription {
                                id: "animated-pr-description".into(),
                                title: "Refine the UI #14".into(),
                                description: "The PR description.".into(),
                            }),
                    )
            }
        }
        let (_, cx) = cx.add_window_view(|_, _| Host);
        draw(cx);
        assert!(cx.debug_bounds("pr-description-content").is_none());
        let title = cx.debug_bounds("pr-title-text").unwrap();
        let outside = gpui_kit::point(px(700.), px(400.));
        cx.simulate_mouse_move(title.center(), None, Default::default());
        draw(cx);
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(120));
        draw(cx);
        let settled = cx.debug_bounds("pr-description-content").unwrap();

        cx.simulate_mouse_move(outside, None, Default::default());
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(150));
        draw(cx);
        assert!(
            cx.debug_bounds("pr-description-content").is_some(),
            "retain the card for its exit"
        );
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        draw(cx);
        assert!(
            cx.debug_bounds("pr-description-content").is_some(),
            "retain it midway through fading"
        );

        // Re-enter during the fade; the pending close must not remove it later.
        cx.simulate_mouse_move(title.center(), None, Default::default());
        draw(cx);
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(120));
        draw(cx);
        assert_eq!(cx.debug_bounds("pr-description-content"), Some(settled));
        cx.simulate_mouse_move(settled.center(), None, Default::default());
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(200));
        draw(cx);
        assert!(cx.debug_bounds("pr-description-content").is_some());

        cx.simulate_mouse_move(outside, None, Default::default());
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(150));
        draw(cx);
        assert!(cx.debug_bounds("pr-description-content").is_some());
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(120));
        draw(cx);
        assert!(
            cx.debug_bounds("pr-description-content").is_none(),
            "unmount after the fade finishes"
        );
    }
}
