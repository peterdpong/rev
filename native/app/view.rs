use super::*;
use crate::theme::*;
use gpui_kit::{
    assets::IconName,
    base::{Button as BaseButton, Popup, motion},
    component::{Icon, TitleBar, tooltip::Tooltip},
};

const SIDEBAR_WIDTH: f32 = 225.;
const HEADER_HEIGHT: f32 = 38.;
const LIST_ROW_HEIGHT: f32 = 26.;
const HEADER_PILL_HEIGHT: f32 = 26.;
// Shared inset matches the centered header pills, excluding the bottom border.
const UI_SPACING: f32 = (HEADER_HEIGHT - 1. - HEADER_PILL_HEIGHT) / 2.;
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

fn row() -> Div {
    div().flex().items_center().gap_2()
}

fn column() -> Div {
    div().flex().flex_col()
}

fn selected_surface<T: Styled>(element: T) -> T {
    element
        .bg(rgb(SELECTED))
        .border_1()
        .border_color(rgb(SELECTED_BORDER))
        .text_color(rgb(TEXT))
        .shadow_sm()
}

fn button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id).label(label).small().ghost()
}

fn icon_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    icon: IconName,
) -> BaseButton {
    let label = label.into();
    BaseButton::new(id)
        .size(px(24.))
        .flex_shrink_0()
        .rounded_md()
        .border_1()
        .border_color(rgba(0x00000000))
        .text_color(rgb(MUTED))
        .hover(|d| d.text_color(rgb(TEXT)))
        .focus_visible(|d| d.border_color(rgb(FOCUS)))
        .styles(|styles| styles.disabled(|d| d.opacity(0.5)))
        .child(Icon::new(icon).size(px(14.)))
        .accessibility_label(label.clone())
        .tooltip(move |window, cx| Tooltip::new(label.clone()).build(window, cx))
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
        .rounded_md()
        .border_1()
        .border_color(rgba(0x00000000))
        .focus_visible(|d| d.border_color(rgb(FOCUS)))
        .styles(|styles| styles.disabled(|d| d.opacity(0.5)))
        .accessibility_label(label.clone())
        .child(icon)
        .child(div().flex_1().min_w_0().truncate().child(label))
}

fn sidebar_row(id: impl Into<ElementId>, selected: bool, disabled: bool) -> Stateful<Div> {
    row()
        .id(id)
        .h(px(LIST_ROW_HEIGHT))
        .flex_shrink_0()
        .gap_1()
        .pr_1()
        .rounded_md()
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
    fn review_header(&self, _: &mut Context<Self>) -> TitleBar {
        TitleBar::new()
            .h(px(HEADER_HEIGHT))
            .pl_0()
            .bg(rgb(BG))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(div().w(px(SIDEBAR_WIDTH)).h_full().bg(rgb(PANEL)))
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
                        row()
                            .gap_0()
                            .flex_shrink_0()
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
                    .h(px(30.))
                    .flex_shrink_0()
                    .pl(px(UI_SPACING + 8.))
                    .pr(px(UI_SPACING + 4.))
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

    fn diff_view_header(&self, _: &mut Context<Self>) -> AnyElement {
        div().into_any_element()
    }

    fn file_list(&mut self, _: &mut Context<Self>) -> AnyElement {
        div().into_any_element()
    }

    fn diff_view(&mut self, _: &mut Context<Self>) -> AnyElement {
        Diff::new(&self.diff)
            .size_full()
            .header_visible(false)
            .soft_wrap(self.wrap)
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
        let mut body = column().gap_4();
        let mut footer = row().justify_end().gap_2();
        let mut choices = column().gap(px(2.));
        for (i, branch) in modal.local_branches.iter().enumerate() {
            let branch = branch.clone();
            let serial = modal.serial;
            choices = choices.child(
                BaseButton::new(("local-branch", i))
                    .w_full()
                    .h(px(44.))
                    .flex_shrink_0()
                    .min_w_0()
                    .px_2()
                    .gap_2()
                    .justify_start()
                    .text_left()
                    .rounded_md()
                    .border_1()
                    .border_color(rgba(0x00000000))
                    .hover(|d| d.bg(rgb(HOVER)))
                    .focus_visible(|d| d.border_color(rgb(FOCUS)))
                    .styles(|styles| styles.disabled(|d| d.opacity(0.5)))
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
                            .gap(px(4.))
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
                    .px_2()
                    .py_3()
                    .text_color(rgb(MUTED))
                    .child("Loading local branches…"),
            );
        } else if let Some(error) = &modal.branches_error {
            choices = choices.child(
                div()
                    .px_2()
                    .py_3()
                    .text_color(rgb(DANGER))
                    .child(error.clone()),
            );
        } else if modal.local_branches.is_empty() {
            choices = choices.child(
                div()
                    .px_2()
                    .py_3()
                    .text_color(rgb(MUTED))
                    .child("No local branches yet."),
            );
        }
        body = body
            .child(
                column()
                    .gap_2()
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
                    .gap_3()
                    .pt_4()
                    .border_t_1()
                    .border_color(rgb(BORDER))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Manual diff"))
                    .child(
                        row()
                            .items_start()
                            .gap_3()
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
                Button::new("submit-modal")
                    .primary()
                    .label(label)
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
                    .rounded_lg()
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
                            .h(px(44.))
                            .flex_shrink_0()
                            .px_4()
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
                                    row()
                                        .h(px(26.))
                                        .max_w(px(200.))
                                        .min_w_0()
                                        .px_2()
                                        .gap(px(6.))
                                        .rounded_full()
                                        .bg(rgb(HOVER))
                                        .border_1()
                                        .border_color(rgb(CONTROL_BORDER))
                                        .text_color(rgb(TEXT))
                                        .text_size(px(12.))
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
                            .p_4()
                            .child(body),
                    )
                    .child(
                        div()
                            .px_4()
                            .py_3()
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
                        Button::new("welcome-open").label("Open folder").on_click(
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
                        .px_4()
                        .py_2()
                        .border_t_1()
                        .border_color(rgb(BORDER))
                        .child(div().flex_1().text_color(rgb(DANGER)).child(error))
                        .child(button("dismiss-error", "×").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.error = None;
                                cx.notify();
                            },
                        ))),
                )
            })
            .when(self.modal.is_some(), |d| d.child(self.dialog(window, cx)))
    }
}
