use super::*;
use super::branch_popover::BranchForm;
use super::controls::*;
use super::popover::*;
use crate::theme::*;
use gpui_kit::{
    assets::IconName,
    base::{Button as BaseButton, Popover, motion},
    component::Icon,
};

pub(super) const SIDEBAR_WIDTH: f32 = 225.;

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

}

impl Render for Rev {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.file_list.update(cx, |list, cx| {
            list.sync_status(self.file.as_deref(), &self.state.viewed, self.loading, cx)
        });

        let files = self.review.iter().flat_map(|review| &review.files).map(|file| {
            let patch = self.patches.get(&file.path);
            (file.path.clone(), DiffFileStatus {
                counts: (file.added, file.removed),
                viewed: patch.is_some_and(|patch| self.state.viewed.get(&file.path) == Some(&patch.digest)),
                disabled: !self.loading && !self.patch_loading && patch.is_none_or(|patch| patch.too_large || patch.digest.is_empty()),
            })
        }).collect();
        self.diff.update(cx, |diff, cx| diff.sync_status(DiffViewStatus {
            path: self.file.clone(), loading: self.loading || self.patch_loading, files,
        }, cx));

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
            .on_action(cx.listener(|this, _: &FindInDiff, window, cx| {
                if this.review.is_some() && !this.branch_popover.is_open() {
                    this.diff.update(cx, |diff, cx| diff.open_find(window, cx));
                }
            }))
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
            patches: Default::default(),
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
    fn diff_header_shows_branch_pills_and_copies_the_head(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::diff::init(cx);
            crate::theme::init(cx);
        });
        let (_, cx) = cx.add_window_view(|window, cx| {
            let mut app = fixture(window, cx);
            let review = app.review.as_mut().unwrap();
            review.request.base = Some("refs/heads/main".into());
            review.request.head = Some("refs/heads/fix/align-review-summary".into());
            app
        });
        draw(cx);
        let base = cx.debug_bounds("branch-base").unwrap();
        let head = cx.debug_bounds("branch-head").unwrap();
        let copy = cx.debug_bounds("copy-branch").unwrap();
        let height = px(crate::app::branch_comparison::BRANCH_PILL_HEIGHT);
        assert_eq!((base.size.height, head.size.height), (height, height));
        assert_eq!(base.top(), head.top(), "keep both pills on one line");
        assert!(base.right() < head.left(), "base before head");
        let header = cx.debug_bounds("diff-view-header").unwrap();
        let counts = cx.debug_bounds("diff-line-counts").unwrap();
        assert_eq!(
            base.left() - header.left(),
            px(crate::theme::UI_SPACING),
            "start the base pill at the header's inset, like the boxes around it"
        );
        assert_eq!(
            counts.top() - base.bottom(),
            px(crate::theme::UI_SPACING),
            "space the counts below the pills"
        );
        assert_eq!(
            counts.left(),
            base.left(),
            "align the counts with the pill's left edge"
        );
        assert!(
            copy.left() > head.left() && copy.right() <= head.right(),
            "join the copy button to the head pill: {head:?}, {copy:?}"
        );

        cx.simulate_click(copy.center(), Default::default());
        draw(cx);
        let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
        assert_eq!(copied.as_deref(), Some("fix/align-review-summary"));
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
                    body: "The PR description.\n\nA second paragraph.".into(),
                    head_ref_name: "feature".into(),
                    base_ref_name: "main".into(),
                    comparison_base_ref_name: None,
                    url: "https://github.com/owner/repo/pull/14".into(),
                    author: "octocat".into(),
                    state: "OPEN".into(),
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
        let link = cx.debug_bounds("open-pr-browser").unwrap();
        let inset = px(crate::app::header::PR_CARD_LINK_INSET);
        assert_eq!(
            (link.top() - settled.top(), settled.right() - link.right()),
            (inset, inset),
            "inset the GitHub link equally from the card's top and right edges"
        );
        let title = cx.debug_bounds("pr-card-title").unwrap();
        let meta = cx.debug_bounds("pr-card-meta").unwrap();
        let description = cx.debug_bounds("pr-card-description").unwrap();
        let line = px(crate::app::header::PR_CARD_LINE_HEIGHT);
        let gap = px(crate::app::header::PR_CARD_GAP);
        assert_eq!((title.size.height, meta.size.height), (line, line));
        assert_eq!(meta.top() - title.bottom(), gap, "title to meta");
        assert_eq!(
            description.top() - meta.bottom(),
            gap,
            "meta to description"
        );
        assert_eq!(
            description.size.height,
            line * 2. + gap,
            "two one-line paragraphs, one gap apart"
        );
        assert_eq!(settled.left(), pill.left(), "align with the title pill");
        assert!(
            (settled.top() - pill.bottom() - px(crate::app::header::PR_DESCRIPTION_GAP)).abs() <= px(1.),
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
            ..Default::default()
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
                            .child(crate::app::header::PrDescription {
                                id: "animated-pr-description".into(),
                                pull_request: crate::model::PullRequest {
                                    number: 14,
                                    title: "Refine the UI".into(),
                                    body: "The PR description.".into(),
                                    ..Default::default()
                                },
                            }),
                    )
            }
        }
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::theme::init(cx);
        });
        let (_, cx) = cx.add_window_view(|_, _| Host);
        draw(cx);
        assert!(cx.debug_bounds("pr-description-content").is_none());
        let title = cx.debug_bounds("pr-title-text").unwrap();
        let outside = gpui_kit::point(px(700.), px(400.));
        cx.simulate_mouse_move(title.center(), None, Default::default());
        draw(cx);
        cx.executor().advance_clock(MOTION_DURATION);
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
        cx.executor().advance_clock(MOTION_DURATION);
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
        cx.executor().advance_clock(MOTION_DURATION);
        draw(cx);
        assert!(
            cx.debug_bounds("pr-description-content").is_none(),
            "unmount after the fade finishes"
        );
    }
}
