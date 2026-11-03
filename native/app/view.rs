use super::*;
use super::controls::*;
use crate::theme::*;
use gpui_kit::{
    assets::IconName,
    base::{Button as BaseButton, Popup, motion},
    component::{Icon, TitleBar},
};

const SIDEBAR_WIDTH: f32 = 225.;
const PR_DESCRIPTION_GAP: f32 = 2.;

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
    fn popover_button(
        &self,
        id: &str,
        trigger: impl IntoElement,
        corner: Anchor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let bounds = self
            .modal
            .as_ref()
            .filter(|modal| modal.anchor.as_deref() == Some(id))
            .map(|modal| modal.anchor_bounds.clone());
        gpui_kit::base::Popup::new(SharedString::from(format!("popover-{id}")), trigger)
            .flex_shrink_0()
            .anchor(corner)
            .offset(px(UI_SPACING))
            .when_some(bounds, |popup, bounds| {
                popup
                    .on_position(move |_, trigger| bounds.set(trigger))
                    .content(self.popover(window, cx))
            })
            .into_any_element()
    }

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

    fn review_header(&self, sidebar: f32, window: &mut Window, cx: &mut Context<Self>) -> TitleBar {
        let busy = self.loading || self.submitting;
        TitleBar::new()
            .h(px(HEADER_HEIGHT))
            .pl_0()
            .bg(rgb(BG))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .w(px(112. + (SIDEBAR_WIDTH - 112.) * sidebar))
                    .h_full()
                    .flex_shrink_0()
                    .bg(rgb(PANEL))
                    .border_r_1()
                    .border_color(rgb(BORDER))
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
                                .child(
                                    self.popover_button(
                                        "review-details",
                                        icon_button(
                                            "review-details",
                                            "Review details",
                                            IconName::Info,
                                        )
                                        .on_click(
                                            cx.listener(|this, _, window, cx| {
                                                this.open_review_details(window, cx)
                                            }),
                                        ),
                                        Anchor::TopRight,
                                        window,
                                        cx,
                                    ),
                                )
                                .child(
                                    self.popover_button(
                                        "choose-commits",
                                        icon_button(
                                            "choose-commits",
                                            "Choose commits",
                                            IconName::GitCommitHorizontal,
                                        )
                                        .disabled(busy)
                                        .on_click(
                                            cx.listener(|this, _, window, cx| {
                                                this.open_commit_picker(window, cx)
                                            }),
                                        ),
                                        Anchor::TopRight,
                                        window,
                                        cx,
                                    ),
                                )
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

    fn sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
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
                                self.popover_button(&format!("add-{repo}"),
                                icon_button(
                                    SharedString::from(format!("add-{repo}")),
                                    format!("Open a branch or PR in {}", project.name),
                                    IconName::Plus,
                                )
                                .disabled(blocked)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_branch_modal(repo.clone(), window, cx)
                                }))
, Anchor::TopLeft, window, cx),
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
            .id("project-sidebar")
            .track_focus(&self.sidebar_focus)
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
        let split = self.diff.read(cx).is_split(cx);
        let wrap = self.diff.read(cx).wraps_lines();

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
                        let (added, removed) = self.file_list.read(cx).counts();
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

impl Render for Rev {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sidebar = super::motion::reveal("sidebar-visibility", !self.sidebar_collapsed, window, cx);
        self.file_list.update(cx, |list, cx| {
            list.sync_status(self.file.as_deref(), &self.state.viewed, self.loading, cx)
        });

        let counts = self
            .file
            .as_deref()
            .and_then(|path| self.file_list.read(cx).file_counts(path));

        self.diff.update(cx, |diff, cx| {
            diff.sync_status(
                DiffViewStatus {
                    path: self.file.clone(),
                    counts,
                    viewed: self
                        .file
                        .as_ref()
                        .is_some_and(|path| self.state.viewed.contains_key(path)),
                    viewed_disabled: self.loading
                        || self.patch_loading
                        || self.submitting
                        || self.patch.digest.is_empty(),
                    loading: self.loading || self.patch_loading,
                    notice: self.notice.clone(),
                },
                cx,
            )
        });

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
                            .child(self.file_list.clone())
                            .child(self.diff.clone()),
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
                cx.listener(|this, _: &ToggleSidebar, window, cx| this.toggle_sidebar(window, cx)),
            )
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
            .child(self.review_header(sidebar, window, cx))
            .child(
                row()
                    .items_start()
                    .gap_0()
                    .flex_1()
                    .min_h_0()
                    .when(sidebar > 0., |row| {
                        row.child(
                            div()
                                .w(px(SIDEBAR_WIDTH * sidebar))
                                .h_full()
                                .flex_shrink_0()
                                .overflow_hidden()
                                .opacity(sidebar)
                                .child(self.sidebar(window, cx)),
                        )
                    })
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

    }
}

#[cfg(test)]
mod tests {
    use super::Rev;
    use crate::app::{FileList, DiffView};
    use crate::{
        model::{File, FilePatch, Review, ReviewState},
        workspace_state::WorkspaceState,
    };
    use gpui_kit::{
        Context, Entity, IntoElement, Render, TestAppContext, VisualTestContext,
        Window, div, prelude::*, px,
    };

    struct FileListHost {
        view: Entity<Rev>,
    }

    struct HeaderHost {
        view: Entity<Rev>,
    }

    impl Render for HeaderHost {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let header = self
                .view
                .update(cx, |view, cx| view.review_header(1.0, window, cx));
            div().w(px(1000.)).h(px(400.)).child(header)
        }
    }

    impl Render for FileListHost {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let list = self.view.read(cx).file_list.clone();
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
        let file_list = cx.new(|cx| FileList::new(window, cx));
        file_list.update(cx, |list, cx| list.set_review("", &files, window, cx));
        let diff = cx.new(DiffView::new);
        let subscriptions = Rev::subscribe_to_views(&file_list, &diff, window, cx);
        Rev {
            sidebar_collapsed: false,
            sidebar_focus: cx.focus_handle(),
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
            diff,
            file_list,
            commit_picker: Default::default(),
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
            working_tree_poll_inflight: false,
            working_tree_refreshing: false,
            working_tree_revisions: Default::default(),
            _working_tree_poll: None,
            _subscriptions: subscriptions,
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
    fn reopening_branch_dialog_starts_with_empty_inputs(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::diff::init(cx);
            crate::theme::init(cx);
        });
        let (view, cx) = cx.add_window_view(fixture);
        cx.update(|window, cx| {
            view.update(cx, |app, cx| {
                app.open_branch_modal("/first-repo".into(), window, cx);
                let modal = app.modal.as_ref().unwrap();
                modal.base_input.update(cx, |input, cx| {
                    input.set_value("main", window, cx)
                });
                modal.head_input.update(cx, |input, cx| {
                    input.set_value("feature", window, cx)
                });
            });
        });
        draw(cx);

        cx.update(|window, cx| {
            view.update(cx, |app, cx| {
                app.close_modal(window, cx);
                assert!(app.modal.is_none());
                app.open_branch_modal("/second-repo".into(), window, cx);
                let modal = app.modal.as_ref().unwrap();
                assert!(modal.base_input.read(cx).value().is_empty());
                assert!(modal.head_input.read(cx).value().is_empty());
                let request = modal.manual_request(cx);
                assert_eq!(request.repo, "/second-repo");
                assert_eq!(request.base, None);
                assert_eq!(request.head, None);
            });
        });
        draw(cx);
    }

    #[gpui::test]
    fn discarding_a_pending_patch_does_not_block_later_reviews(cx: &mut TestAppContext) {
        use serde_json::json;

        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::diff::init(cx);
            crate::theme::init(cx);
        });
        let data_dir =
            std::env::temp_dir().join(format!("rev-pending-patch-{}", std::process::id()));
        let (view, cx) = cx.add_window_view(|window, cx| Rev {
            data_dir: data_dir.clone(),
            ..fixture(window, cx)
        });

        cx.update(|window, cx| {
            view.update(cx, |app, cx| {
                let review = app.review.as_mut().unwrap();
                review.request = crate::model::ReviewRequest::working_tree("/test-repo");
                app.workspace.remember_review(review, &review.request);

                // Removing the only project discards the patch that is still loading.
                app.patch_loading = true;
                app.workspace_action(
                    json!({ "type": "removeProject", "repo": "/test-repo" }),
                    true,
                    window,
                    cx,
                );
                assert!(app.review.is_none());
                assert!(!app.patch_loading);

                // Opening a review while a patch loads replaces it instead of being skipped.
                app.patch_loading = true;
                app.load_review(
                    crate::model::ReviewRequest::working_tree("/test-repo"),
                    None,
                    window,
                    cx,
                );
                assert!(app.loading);
                assert!(!app.patch_loading);
            });
        });

        let _ = std::fs::remove_dir_all(&data_dir);
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
                    ..Default::default()
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
