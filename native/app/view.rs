use super::*;
use super::branch_popover::BranchForm;
use super::controls::*;
use super::popover::*;
use crate::theme::*;
use gpui_kit::{
    assets::IconName,
    base::{Button as BaseButton, Popover, Popup, motion},
    component::{Icon, TitleBar},
};

const SIDEBAR_WIDTH: f32 = 225.;
const COLLAPSED_HEADER_WIDTH: f32 = 112.;
// Match the spacing between header pills.
const PR_DESCRIPTION_GAP: f32 = UI_SPACING;

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
                .bg(rgb(SELECTED))
                .with_animation(
                    "pr-title-pulse",
                    Animation::new(std::time::Duration::from_millis(1200))
                        .repeat()
                        .with_easing(pulsating_between(0.45, 1.)),
                    |bar, delta| bar.opacity(delta),
                ),
        )
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

fn sidebar_row(id: impl Into<ElementId>, selected: bool) -> Stateful<Div> {
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
        .hover(move |d| if selected { d } else { d.bg(rgb(HOVER)) })
}

impl Rev {
    fn render_branch_popover(
        &self,
        repo: &str,
        project_name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = format!("add-{repo}");
        let anchored = |form: &&BranchForm| form.anchor.as_deref() == Some(id.as_str());
        let open = self
            .branch_popover
            .form
            .as_ref()
            .filter(|form| form.ready)
            .filter(anchored);

        let animated = AnimatedPopover::new(format!("popover-{id}"), open.is_some(), window, cx);
        let content = open
            .or(self.branch_popover.closing.as_ref().filter(anchored))
            .filter(|_| animated.visible())
            .map(|form| self.branch_popover_content(form, animated.open(), window, cx));

        let this = cx.entity().downgrade();
        let repo = repo.to_string();

        let base = Popover::new(SharedString::from(format!("popover-{id}")))
            .flex_shrink_0()
            .track_focus(&self.branch_popover.focus)
            .on_open_change(move |open, window, cx| {
                _ = this.update(cx, |this, cx| {
                    if *open {
                        this.open_branch_popover(repo.clone(), window, cx);
                    } else if this
                        .branch_popover
                        .form
                        .as_ref()
                        .is_some_and(|form| form.request.repo == repo)
                    {
                        this.close_branch_popover(window, cx);
                    }
                });
            })
            .trigger(
                icon_button(
                    SharedString::from(id.clone()),
                    format!("Open a branch or PR in {project_name}"),
                    IconName::Plus,
                )
                .debug_selector({
                    let id = id.clone();
                    move || id.clone()
                }),
            );

        animated.render(base, content)
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

    fn review_header(
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
            .bg(rgb(BG))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .w(px(COLLAPSED_HEADER_WIDTH
                        + (SIDEBAR_WIDTH - COLLAPSED_HEADER_WIDTH)
                            * sidebar_openness))
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
                                    let width = title_pill_width.flatten();
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
                                                .when_some(width, |d, w| d.w(w))
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
                                                        .text_color(rgb(MUTED)),
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
                                ),
                        )
                    }),
            )
    }

    fn sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
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
                sidebar_row(hover_group.clone(), false)
                    .debug_selector({
                        let id = hover_group.to_string();
                        move || id.clone()
                    })
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
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.workspace_action(
                                        json!({ "type": "removeProject", "repo": remove_repo }),
                                        active, window, cx,
                                    )
                                })),
                            )
                            .child(
                                self.render_branch_popover(&repo, &project.name, window, cx),
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
                        sidebar_row(hover_group.clone(), active)
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
                    viewed_disabled: !self.loading
                        && !self.patch_loading
                        && self.patch.digest.is_empty(),
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

        let sidebar_openness = motion::transition(
            "sidebar-openness",
            if self.sidebar_collapsed {
                0.0_f32
            } else {
                1.0_f32
            },
            motion_transition(),
            window,
            cx,
        );

        let status = if !self.branch_popover.is_open() {
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
                if !this.branch_popover.is_open() {
                    this.mark_viewed(window, cx);
                }
            }))
            .child(self.review_header(sidebar_openness, window, cx))
            .child(
                row()
                    .items_start()
                    .gap_0()
                    .flex_1()
                    .min_h_0()
                    .when(sidebar_openness > 0., |row| {
                        // Clip a full-width sidebar and slide it with the
                        // closing edge so its contents never reflow.
                        row.child(
                            div()
                                .w(px(SIDEBAR_WIDTH * sidebar_openness))
                                .h_full()
                                .flex_shrink_0()
                                .overflow_hidden()
                                .child(
                                    div()
                                        .relative()
                                        .left(px(-SIDEBAR_WIDTH * (1. - sidebar_openness)))
                                        .h_full()
                                        .child(self.sidebar(window, cx)),
                                ),
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
    use crate::app::controls::MOTION_DURATION;
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
                .update(cx, |view, cx| view.review_header(1., window, cx));
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
            pull_requests: Default::default(),
            without_pull_request: Default::default(),
            pull_request_loading: false,
            revalidate_pull_request: false,
            focus: cx.focus_handle(),
            branch_popover: super::BranchPopover::new(cx),
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
    fn plus_button_toggles_an_animated_branch_popover(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::diff::init(cx);
            crate::theme::init(cx);
        });
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut app = fixture(window, cx);
            app.workspace.projects = vec![crate::workspace_state::Project {
                repo: "/repo".into(),
                name: "repo".into(),
                collapsed: false,
                tabs: Vec::new(),
            }];
            app
        });
        let settle = |cx: &mut VisualTestContext| {
            draw(cx);
            cx.executor().advance_clock(MOTION_DURATION);
            draw(cx);
        };
        draw(cx);
        let row = cx.debug_bounds("project-row-/repo").unwrap();
        cx.simulate_mouse_move(row.center(), None, Default::default());
        draw(cx);
        let plus = cx.debug_bounds("add-/repo").unwrap();
        assert!(cx.debug_bounds("review-popover").is_none());

        cx.simulate_click(plus.center(), Default::default());
        settle(cx);
        let popover = cx.debug_bounds("review-popover").unwrap();
        assert!(popover.top() >= plus.bottom());
        assert!(cx.update(|window, cx| view.read(cx).branch_popover.focus.is_focused(window)));

        cx.simulate_click(gpui_kit::point(px(900.), px(500.)), Default::default());
        draw(cx);
        assert!(cx.update(|_, cx| !view.read(cx).branch_popover.is_open()));
        assert!(cx.debug_bounds("review-popover").is_some());
        settle(cx);
        assert!(cx.debug_bounds("review-popover").is_none());

        cx.simulate_mouse_move(plus.center(), None, Default::default());
        draw(cx);
        cx.simulate_click(plus.center(), Default::default());
        settle(cx);
        assert!(cx.update(|_, cx| view.read(cx).branch_popover.is_open()));
        cx.simulate_click(plus.center(), Default::default());
        settle(cx);
        assert!(cx.update(|_, cx| !view.read(cx).branch_popover.is_open()));
        assert!(cx.debug_bounds("review-popover").is_none());
    }

    #[gpui::test]
    fn closing_keeps_only_a_shown_popover_for_its_exit(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::diff::init(cx);
            crate::theme::init(cx);
        });
        let (view, cx) = cx.add_window_view(fixture);
        cx.update(|window, cx| {
            view.update(cx, |app, cx| {
                app.open_branch_popover("/first-repo".into(), window, cx);
                assert!(app.branch_popover.form.as_ref().unwrap().ready);
                app.close_branch_popover(window, cx);
                let closing = app.branch_popover.closing.as_ref().unwrap();
                assert_eq!(closing.anchor.as_deref(), Some("add-/first-repo"));

                app.open_branch_popover("/second-repo".into(), window, cx);
                app.branch_popover.form.as_mut().unwrap().ready = false;
                app.close_branch_popover(window, cx);
                let closing = app.branch_popover.closing.as_ref().unwrap();
                assert_eq!(closing.anchor.as_deref(), Some("add-/first-repo"));
            });
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
                app.open_branch_popover("/first-repo".into(), window, cx);
                let form = app.branch_popover.form.as_ref().unwrap();
                form.base_input.update(cx, |input, cx| {
                    input.set_value("main", window, cx)
                });
                form.head_input.update(cx, |input, cx| {
                    input.set_value("feature", window, cx)
                });
            });
        });
        draw(cx);

        cx.update(|window, cx| {
            view.update(cx, |app, cx| {
                app.close_branch_popover(window, cx);
                assert!(!app.branch_popover.is_open());
                app.open_branch_popover("/second-repo".into(), window, cx);
                let form = app.branch_popover.form.as_ref().unwrap();
                assert!(form.base_input.read(cx).value().is_empty());
                assert!(form.head_input.read(cx).value().is_empty());
                let request = form.manual_request(cx);
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
            assert!(!app.branch_popover.is_open());
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
            assert!(!app.branch_popover.is_open());
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
            (settled.top() - pill.bottom() - px(super::PR_DESCRIPTION_GAP)).abs() <= px(1.),
            "leave the pill gap, allowing for pixel rounding: {pill:?}, {settled:?}"
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
    fn pull_request_title_is_cached_across_commit_selections(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::diff::init(cx);
            crate::theme::init(cx);
        });
        let (host, cx) = cx.add_window_view(|window, cx| HeaderHost {
            view: cx.new(|cx| {
                let mut app = fixture(window, cx);
                app.review.as_mut().unwrap().key = "review".into();
                app.pull_request_loading = true;
                app
            }),
        });
        let view = cx.update(|_, cx| host.read(cx).view.clone());
        draw(cx);
        assert!(cx.debug_bounds("pr-title-loading").is_some());
        assert!(cx.debug_bounds("pr-title-pill").is_none());

        let pr = crate::model::PullRequest {
            number: 14,
            title: "Refine the UI".into(),
            body: String::new(),
            head_ref_name: "feature".into(),
            base_ref_name: "main".into(),
            comparison_base_ref_name: None,
        };
        view.update(cx, |app, cx| {
            app.pull_requests.insert("review".into(), pr.clone());

            // A new commit selection keeps the review key, so it reuses the title
            // without fetching it again.
            let mut selection = Review {
                key: "review".into(),
                ..Default::default()
            };
            assert!(!app.apply_cached_pull_request(&mut selection));
            assert_eq!(selection.pull_request.as_ref(), Some(&pr));

            // A review whose last lookup found no PR is looked up again, since one may
            // have been opened since, but keeps its branch pill instead of a placeholder.
            app.without_pull_request.insert("plain".into());
            let mut plain = Review {
                key: "plain".into(),
                ..Default::default()
            };
            assert!(app.apply_cached_pull_request(&mut plain));
            assert_eq!(plain.pull_request, None);

            // Unknown reviews fetch, and Refresh revalidates a cached one.
            let mut other = Review {
                key: "other".into(),
                ..Default::default()
            };
            assert!(app.apply_cached_pull_request(&mut other));
            app.revalidate_pull_request = true;
            let mut refreshed = Review {
                key: "review".into(),
                ..Default::default()
            };
            assert!(app.apply_cached_pull_request(&mut refreshed));
            assert_eq!(refreshed.pull_request.as_ref(), Some(&pr));

            app.review = Some(selection);
            app.pull_request_loading = false;
            cx.notify();
        });
        draw(cx);
        assert!(cx.debug_bounds("pr-title-loading").is_none());
        assert!(cx.debug_bounds("pr-title-pill").is_some());
    }

    #[gpui::test]
    fn title_pill_slides_between_loading_and_loaded_widths(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::diff::init(cx);
            crate::theme::init(cx);
        });
        let (host, cx) = cx.add_window_view(|window, cx| HeaderHost {
            view: cx.new(|cx| {
                let mut app = fixture(window, cx);
                app.review.as_mut().unwrap().branch = "a".into();
                app.pull_request_loading = true;
                app
            }),
        });
        let view = cx.update(|_, cx| host.read(cx).view.clone());
        draw(cx);
        let loading = cx.debug_bounds("pr-title-loading").unwrap().size.width;

        view.update(cx, |app, cx| {
            app.pull_request_loading = false;
            cx.notify();
        });
        draw(cx);
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(60));
        draw(cx);
        let midway = cx.debug_bounds("branch-pill").unwrap().size.width;

        cx.executor()
            .advance_clock(std::time::Duration::from_millis(400));
        draw(cx);
        draw(cx);
        let settled = cx.debug_bounds("branch-pill").unwrap().size.width;
        assert!(settled < loading, "a one-letter branch is narrower: {settled:?}");
        assert!(
            midway < loading && midway > settled,
            "slide rather than snap: {loading:?} → {midway:?} → {settled:?}"
        );
    }

    #[gpui::test]
    fn commit_pill_opens_a_dropdown_beside_the_title(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::diff::init(cx);
            crate::theme::init(cx);
        });
        let (host, cx) = cx.add_window_view(|window, cx| HeaderHost {
            view: cx.new(|cx| {
                let mut app = fixture(window, cx);
                let review = app.review.as_mut().unwrap();
                review.name = "patch".into();
                review.request.mode = Some("branch".into());
                review.commit_count = 4;
                app
            }),
        });
        let view = cx.update(|_, cx| host.read(cx).view.clone());
        draw(cx);
        let pill = cx.debug_bounds("commit-picker-pill").unwrap();
        let actions = cx.debug_bounds("review-header-actions").unwrap();
        assert!(pill.right() < actions.left());
        assert!(cx.debug_bounds("commit-menu").is_none());

        // The menu waits for its commits rather than opening empty and growing.
        cx.simulate_click(pill.center(), Default::default());
        draw(cx);
        draw(cx);
        assert!(cx.update(|_, cx| view.read(cx).commit_picker.open));
        assert!(cx.debug_bounds("commit-menu").is_none());

        // Clicking again while they load cancels the open.
        cx.simulate_click(pill.center(), Default::default());
        draw(cx);
        assert!(!cx.update(|_, cx| view.read(cx).commit_picker.open));

        cx.simulate_click(pill.center(), Default::default());
        draw(cx);
        view.update(cx, |app, cx| {
            let picker = &mut app.commit_picker;
            picker.loading = false;
            picker.commits = vec![crate::model::Commit {
                sha: "0123456789".into(),
                subject: "Add the picker".into(),
                author: "a".into(),
                date: "2026-11-01".into(),
            }];
            cx.notify();
        });
        draw(cx);
        draw(cx);
        cx.executor().advance_clock(MOTION_DURATION);
        draw(cx);
        let menu = cx.debug_bounds("commit-menu").unwrap();
        assert_eq!(menu.left(), pill.left(), "align the menu with its pill");
        assert!(menu.top() >= pill.bottom());

        // Clicking the pill again closes it rather than reopening.
        cx.simulate_click(pill.center(), Default::default());
        draw(cx);
        assert!(!cx.update(|_, cx| view.read(cx).commit_picker.open));
        assert!(cx.debug_bounds("commit-menu").is_some());
        cx.executor().advance_clock(MOTION_DURATION);
        draw(cx);
        assert!(cx.debug_bounds("commit-menu").is_none());

        // Reopening on the same branch reuses the loaded commits, so it opens at once.
        cx.simulate_click(pill.center(), Default::default());
        draw(cx);
        draw(cx);
        assert!(!cx.update(|_, cx| view.read(cx).commit_picker.loading));
        assert!(cx.debug_bounds("commit-menu").is_some());

        // Clicking elsewhere dismisses an open menu.
        cx.simulate_click(gpui_kit::point(px(950.), px(350.)), Default::default());
        draw(cx);
        assert!(!cx.update(|_, cx| view.read(cx).commit_picker.open));
        cx.executor().advance_clock(MOTION_DURATION);
        draw(cx);
        assert!(cx.debug_bounds("commit-menu").is_none());
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
