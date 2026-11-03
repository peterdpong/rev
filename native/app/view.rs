use super::*;
use gpui_kit::component::{Icon, IconName, text::TextView};

pub(super) const BG: u32 = 0x17191d;
pub(super) const PANEL: u32 = 0x131519;
pub(super) const BORDER: u32 = 0x2b2e34;
pub(super) const MUTED: u32 = 0x929aa7;
pub(super) const BLUE: u32 = 0x92b8ff;
pub(super) const SELECTED: u32 = 0x263449;

pub(super) fn row() -> Div {
    div().flex().items_center().gap_2()
}
pub(super) fn column() -> Div {
    div().flex().flex_col()
}
pub(super) fn button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id).label(label).small().ghost()
}

fn navigation_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    icon: impl IntoElement,
) -> Button {
    let label = label.into();
    Button::new(id)
        .small()
        .ghost()
        .flex_1()
        .min_w_0()
        .h(px(28.))
        .px_2()
        .accessibility_label(label.clone())
        .tooltip(label.clone())
        // Button's built-in label is centered. A full-width content row keeps
        // short and truncated names on the same leading edge.
        .child(
            row()
                .id("navigation-content")
                .w_full()
                .min_w_0()
                .text_size(px(13.))
                .child(
                    div()
                        .size(px(14.))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(MUTED))
                        .child(icon),
                )
                .child(
                    div()
                        .id("navigation-label")
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(label),
                ),
        )
}

fn navigation_action(id: impl Into<ElementId>, icon: IconName, label: &'static str) -> Button {
    Button::new(id)
        .small()
        .ghost()
        .icon(icon)
        .w(px(24.))
        .h(px(28.))
        .flex_shrink_0()
        .accessibility_label(label)
        .tooltip(label)
}

impl Rev {
    pub(super) fn popover_button(
        &mut self,
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
            .offset(px(6.))
            .when_some(bounds, |popup, bounds| {
                popup
                    .on_position(move |_, trigger| bounds.set(trigger))
                    .content(self.popover(window, cx))
            })
            .into_any_element()
    }

    pub(super) fn sidebar_toggle(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = if self.sidebar_collapsed {
            "Show sidebar"
        } else {
            "Hide sidebar"
        };
        let trigger = navigation_action(
            "toggle-sidebar",
            if self.sidebar_collapsed {
                IconName::PanelLeftOpen
            } else {
                IconName::PanelLeftClose
            },
            label,
        )
        .tooltip(format!("{label} (⌘B)"))
        .on_click(cx.listener(|this, _, window, cx| this.toggle_sidebar(window, cx)));
        self.popover_button("toggle-sidebar", trigger, Anchor::TopLeft, window, cx)
    }

    fn sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let blocked = self.loading || self.workspace_busy || self.sidebar_collapsed;
        let mut projects = column().gap_3().min_w_0();
        for project in self.workspace.projects.clone() {
            let repo = project.repo.clone();
            let collapse_repo = repo.clone();
            let collapsed = project.collapsed;
            let expanded = motion::reveal(
                SharedString::from(format!("project-reveal-{repo}")),
                !collapsed,
                window,
                cx,
            );
            let tabs_height = px(project.tabs.len() as f32 * 32. * expanded);
            let project_button = navigation_button(
                SharedString::from(format!("project-{repo}")),
                project.name,
                Icon::new(if collapsed { IconName::ChevronRight } else { IconName::ChevronDown })
                    .size(px(14.)),
            )
            .disabled(blocked)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.workspace_action(
                    json!({ "type": "collapseProject", "repo": collapse_repo, "collapsed": !collapsed }),
                    false,
                    window,
                    cx,
                )
            }));
            let review_anchor = format!("add-{repo}");
            let add_review = navigation_action(
                SharedString::from(format!("add-{repo}")),
                IconName::Plus,
                "Open branch or PR",
            )
            .disabled(blocked)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_modal(
                    ModalKind::Review,
                    None,
                    Some(repo.clone()),
                    &format!("add-{repo}"),
                    window,
                    cx,
                )
            }));
            let add_review =
                self.popover_button(&review_anchor, add_review, Anchor::TopLeft, window, cx);
            let mut group = column().min_w_0().child(
                row()
                    .mx_2()
                    .gap_0()
                    .min_w_0()
                    .child(project_button)
                    .child(add_review),
            );
            let mut tabs = column().gap_1().pt_1().min_w_0();
            if expanded > 0. {
                for tab in project.tabs {
                    let id = tab.id.clone();
                    let close = id.clone();
                    let active = self.review.as_ref().is_some_and(|r| r.key == tab.id);
                    let request = tab.request.clone();
                    let file = tab.file.clone();

                    tabs = tabs.child(
                        row()
                            .mx_2()
                            .pl(px(20.))
                            .gap_0()
                            .min_w_0()
                            .rounded_md()
                            .when(active, |d| d.bg(rgb(SELECTED)))
                            .child(
                                navigation_button(
                                    SharedString::from(format!("tab-{id}")),
                                    tab.label,
                                    { div().child("⑂").into_any_element() },
                                )
                                .disabled(blocked || collapsed)
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.load_review(request.clone(), file.clone(), window, cx)
                                    },
                                )),
                            )
                            .child(
                                navigation_action(
                                    SharedString::from(format!("close-{id}")),
                                    IconName::Close,
                                    "Close review",
                                )
                                .disabled(blocked || collapsed)
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
                            ),
                    );
                }
            }
            group = group.child(
                div()
                    .id(SharedString::from(format!("project-tabs-{}", project.repo)))
                    .h(tabs_height)
                    .overflow_hidden()
                    .opacity(expanded)
                    .child(tabs),
            );
            projects = projects.child(group);
        }
        column()
            .id("project-sidebar")
            .track_focus(&self.sidebar_focus)
            .w(px(225.))
            .h_full()
            .flex_shrink_0()
            .overflow_hidden()
            .bg(rgb(PANEL))
            .border_r_1()
            .border_color(rgb(BORDER))
            .child(
                row()
                    .h(px(36.))
                    .px_2()
                    .justify_between()
                    .child(
                        div()
                            .pl_2()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(11.))
                            .text_color(rgb(MUTED))
                            .child("Projects"),
                    )
                    .child(
                        self.popover_button(
                            "add-project",
                            navigation_action("add-project", IconName::Plus, "Open repository")
                                .disabled(self.sidebar_collapsed)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_modal(
                                        ModalKind::Open,
                                        None,
                                        None,
                                        "add-project",
                                        window,
                                        cx,
                                    )
                                })),
                            Anchor::TopLeft,
                            window,
                            cx,
                        ),
                    ),
            )
            .child(
                div()
                    .id("projects")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .py_2()
                    .child(projects),
            )
            .child(
                row().p_3().border_t_1().border_color(rgb(BORDER)).child(
                    self.popover_button(
                        "open-project",
                        button("open-project", "Open folder")
                            .disabled(self.sidebar_collapsed)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_modal(
                                    ModalKind::Open,
                                    None,
                                    None,
                                    "open-project",
                                    window,
                                    cx,
                                )
                            })),
                        Anchor::BottomLeft,
                        window,
                        cx,
                    ),
                ),
            )
            .into_any_element()
    }

    fn file_list(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let files = self.visible_files(cx);
        let mut list = column().gap(px(2.)).p_2().min_w_0();
        let mut last_folder = String::new();
        for (index, file) in files.iter().enumerate() {
            let path = file.path.clone();
            let (folder, name) = path.rsplit_once('/').unwrap_or(("", &path));
            if folder != last_folder && !folder.is_empty() {
                list = list.child(
                    row()
                        .gap_1()
                        .pt_2()
                        .pb_1()
                        .px_2()
                        .text_size(px(11.))
                        .text_color(rgb(MUTED))
                        .child(div().min_w_0().truncate().child(folder.to_string())),
                );
            }
            last_folder = folder.to_string();
            let selected = self.file.as_ref() == Some(&path);
            let viewed = self.state.viewed.contains_key(&path);

            let mut item = row()
                .id(("file", index))
                .cursor_pointer()
                .rounded_md()
                .px_2()
                .h(px(28.))
                .min_w_0()
                .gap_1()
                .when(selected, |d| d.bg(rgb(SELECTED)))
                .hover(|d| d.bg(rgb(0x252a33)))
                .child(
                    div()
                        .w(px(14.))
                        .flex_shrink_0()
                        .text_size(px(11.))
                        .text_center()
                        .text_color(if viewed { rgb(0x8fd3a5) } else { rgb(MUTED) })
                        .child(if viewed {
                            "✓".into()
                        } else {
                            file.status.clone()
                        }),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(12.))
                        .truncate()
                        .child(name.to_string()),
                );

            list = list.child(item.on_click(cx.listener(move |this, _, window, cx| {
                if !this.loading {
                    this.load_file(path.clone(), window, cx);
                }
            })));
        }
        let total = self.review.as_ref().map(|r| r.files.len()).unwrap_or(0);
        let viewed = self
            .review
            .as_ref()
            .map(|r| {
                r.files
                    .iter()
                    .filter(|f| self.state.viewed.contains_key(&f.path))
                    .count()
            })
            .unwrap_or(0);
        column()
            .w(px(200.))
            .h_full()
            .flex_shrink_0()
            .border_r_1()
            .border_color(rgb(BORDER))
            .bg(rgb(PANEL))
            .child(
                row()
                    .h(px(34.))
                    .px_3()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .text_size(px(11.))
                    .text_color(rgb(MUTED))
                    .child(div().flex_1().child("Changed files"))
                    .child(format!("{viewed}/{total} viewed")),
            )
            .child(
                div()
                    .px_2()
                    .pt_2()
                    .child(Input::new(&self.filter).id("filter-files").small()),
            )
            .child(
                div()
                    .id("files-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(list),
            )
            .into_any_element()
    }

    fn commit_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let commit = &self.commit_picker.commits[index];
        let selected = self.commit_picker.selected(index);
        let label = format!("{} {}", &commit.sha[..8], commit.subject);
        Button::new(("commit", index))
            .ghost()
            .w_full()
            .h(px(48.))
            .px_2()
            .accessibility_label(label)
            .tooltip(format!(
                "{} · {} · Shift-click to select a range",
                commit.author, commit.date
            ))
            .selected(selected)
            .child(
                row()
                    .w_full()
                    .min_w_0()
                    .child(div().w(px(16.)).flex_shrink_0().child(if selected {
                        "✓"
                    } else {
                        ""
                    }))
                    .child(
                        column()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(13.))
                                    .child(commit.subject.clone()),
                            )
                            .child(
                                row()
                                    .text_size(px(11.))
                                    .text_color(rgb(MUTED))
                                    .child(
                                        div()
                                            .font_family("Menlo")
                                            .child(commit.sha[..8].to_string()),
                                    )
                                    .child(div().truncate().child(commit.author.clone())),
                            ),
                    ),
            )
            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                this.choose_commit(index, event.modifiers().shift, cx)
            }))
            .into_any_element()
    }

    fn commit_choices(&self, cx: &mut Context<Self>) -> AnyElement {
        column()
            .gap_2()
            .child(
                Button::new("all-commits")
                    .ghost()
                    .w_full()
                    .h(px(32.))
                    .px_2()
                    .accessibility_label("Full branch diff")
                    .child(
                        row()
                            .w_full()
                            .child(div().w(px(16.)).flex_shrink_0().child(
                                if self.commit_picker.range.is_none() {
                                    "✓"
                                } else {
                                    ""
                                },
                            ))
                            .child("Full branch diff"),
                    )
                    .selected(self.commit_picker.range.is_none())
                    .on_click(cx.listener(|this, _, _, cx| this.choose_all_commits(cx))),
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
                    .h(px((self.commit_picker.commits.len().min(7) * 48) as f32))
                    .w_full(),
                )
            })
            .when(self.commit_picker.loading, |body| {
                body.child(div().text_color(rgb(MUTED)).child("Loading commits…"))
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

    fn diff_view(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let owner = cx.weak_entity();
        let add_owner = owner.clone();
        let viewed = self
            .file
            .as_ref()
            .is_some_and(|p| self.state.viewed.contains_key(p));
        let header = row()
            .h(px(34.))
            .px_3()
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .text_size(px(12.))
                    .child(self.file.clone().unwrap_or_default()),
            )
            .child(
                button("diff-mode", if self.split { "Split" } else { "Unified" }).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.split = !this.split;
                        let mode = if this.split {
                            DiffMode::Split
                        } else {
                            DiffMode::Unified
                        };
                        this.diff.update(cx, |diff, cx| diff.set_mode(mode, cx));
                        cx.notify();
                    }),
                ),
            )
            .child(
                button("wrap", "Wrap")
                    .selected(self.wrap)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.wrap = !this.wrap;
                        cx.notify();
                    })),
            )
            .child(
                button("viewed", if viewed { "✓ Viewed" } else { "Viewed" })
                    .disabled(self.patch_loading || self.patch.digest.is_empty())
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
                .header_visible(false)
                .soft_wrap(self.wrap)
                .text_size(px(12.))
                .on_add_annotation(move |range, window, cx| {
                    let _ = add_owner.update(cx, |this, cx| {
                        this.selection_menu = window.mouse_position() + point(px(12.), px(18.));
                        this.accept_selection(range, window, cx);
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
            .child(header)
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
            .gap_2()
            .child(
                div()
                    .text_color(rgb(MUTED))
                    .text_size(px(12.))
                    .child(title.to_string()),
            )
            .child(
                Input::new(&self.fields[index])
                    .id(("form-field", index))
                    .aria_label(placeholder.to_string()),
            )
            .into_any_element()
    }

    fn popover(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(modal) = self.modal.clone() else {
            return div().into_any_element();
        };

        let title = match modal.kind {
            ModalKind::Open => "Open repository",
            ModalKind::Review => "Open branch or PR",

            ModalKind::Commits => "Commits",
            ModalKind::Details => "Review details",
        };
        let mut body = column().gap_3();
        let mut footer = row().justify_end().gap_2();
        if modal.kind == ModalKind::Details {
            body = body.child(self.review_details());
        } else if modal.kind == ModalKind::Commits {
            body = body.child(self.commit_choices(cx));
            footer = footer.child(
                div()
                    .flex_1()
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .child(if self.commit_picker.range.is_none() {
                        "Full branch diff".to_string()
                    } else if self.commit_picker.count == 1 {
                        "1 commit".to_string()
                    } else {
                        format!("{} commits", self.commit_picker.count)
                    }),
            );
        } else if matches!(modal.kind, ModalKind::Open | ModalKind::Review) {
            body = body
                .child(self.form_field(0, "Repository", "Repository folder"))
                .child(
                    button("browse", "Choose folder…")
                        .on_click(cx.listener(|this, _, window, cx| this.pick_folder(window, cx))),
                )
                .child(self.form_field(1, "Base", "Base branch; leave blank to detect"))
                .child(self.form_field(
                    2,
                    "Branch / PR",
                    "Branch, PR number or URL; leave blank for HEAD",
                ));
            if modal.kind == ModalKind::Review {
                let branches = self
                    .review
                    .as_ref()
                    .filter(|r| r.request.repo == modal.request.repo)
                    .map(|r| r.branches.clone())
                    .unwrap_or_default();
                let mut choices = column().gap_1();
                for (i, branch) in branches.into_iter().enumerate() {
                    choices = choices.child(
                        button(("branch", i), branch.clone())
                            .justify_start()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.fields[2].update(cx, |input, cx| {
                                    input.set_value(branch.clone(), window, cx)
                                })
                            })),
                    );
                }
                body = body.child(
                    div()
                        .id("branch-options")
                        .max_h(px(140.))
                        .overflow_y_scroll()
                        .child(choices),
                );
            }
        }

        if let Some(error) = &self.error {
            body = body.child(div().text_color(rgb(0xe29595)).child(error.clone()));
        }
        if modal.kind != ModalKind::Details {
            let label = if self.submitting {
                "Working…"
            } else {
                match modal.kind {
                    ModalKind::Commits => "View diff",
                    _ => "Open",
                }
            };
            footer = footer.child(
                Button::new("submit-modal")
                    .primary()
                    .label(label)
                    .disabled(
                        self.submitting
                            || (modal.kind == ModalKind::Commits && self.commit_picker.loading),
                    )
                    .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
            );
        }
        let max_height = (window.viewport_size().height - px(110.)).clamp(px(120.), px(620.));
        let width = px(match modal.kind {
            ModalKind::Open | ModalKind::Review => 420.,
            ModalKind::Commits => 480.,

            ModalKind::Details => 560.,
        })
        .min((window.viewport_size().width - px(16.)).max(px(120.)));
        let serial = modal.serial;
        let progress = motion::enter(("popover-enter", serial), window, cx);
        column()
            .id("review-popover")
            .relative()
            .top(px(-4. * (1. - progress)))
            .opacity(progress)
            .role(Role::Dialog)
            .w(width)
            .max_h(max_height)
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(BORDER))
            .rounded_lg()
            .shadow_lg()
            .occlude()
            .tab_group()
            .track_focus(&self.modal_focus)
            .key_context("RevPopover")
            .on_mouse_down_out(cx.listener(move |this, event, window, cx| {
                this.dismiss_popover(serial, event, window, cx)
            }))
            .on_action(cx.listener(|this, _: &CloseModal, window, cx| this.close_modal(window, cx)))
            .on_action(cx.listener(|this, _: &Submit, window, cx| this.submit(window, cx)))
            .child(
                row()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(
                        button("close-modal", "×").on_click(
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
                    .p_3()
                    .child(body),
            )
            .when(modal.kind != ModalKind::Details, |d| {
                d.child(
                    div()
                        .p_3()
                        .border_t_1()
                        .border_color(rgb(BORDER))
                        .child(footer),
                )
            })
            .into_any_element()
    }
}

impl Render for Rev {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Sample once so the titlebar, sidebar and review controls move together.
        let sidebar = motion::reveal("sidebar-reveal", !self.sidebar_collapsed, window, cx);
        let content = if self.review.is_some() {
            let body_progress = motion::enter(("review-body-enter", 0_usize), window, cx);
            let body = {
                row()
                    .items_start()
                    .gap_0()
                    .flex_1()
                    .min_h_0()
                    .child(self.file_list(cx))
                    .child(self.diff_view(cx))
                    .into_any_element()
            };
            column()
                .flex_1()
                .min_w_0()
                .h_full()
                .child(self.toolbar(sidebar, window, cx))
                .child(
                    column()
                        .flex_1()
                        .min_h_0()
                        .opacity(body_progress)
                        .child(body),
                )
                .into_any_element()
        } else {
            column()
                .relative()
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
                .child(
                    row()
                        .child(
                            self.popover_button(
                                "welcome-open",
                                Button::new("welcome-open").label("Open folder").on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.open_modal(
                                            ModalKind::Open,
                                            None,
                                            None,
                                            "welcome-open",
                                            window,
                                            cx,
                                        )
                                    }),
                                ),
                                Anchor::TopLeft,
                                window,
                                cx,
                            ),
                        )
                        .child(
                            button("welcome-demo", "Try a sample")
                                .disabled(self.submitting)
                                .on_click(cx.listener(|this, _, window, cx| this.demo(window, cx))),
                        ),
                )
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
            .text_color(rgb(0xe4e6eb))
            .text_size(px(13.))
            .font_family(".AppleSystemUIFont")
            .track_focus(&self.focus)
            .key_context("Rev")
            .on_action(cx.listener(|this, _: &OpenProject, window, cx| {
                let anchor = if !this.sidebar_collapsed {
                    "add-project"
                } else if this.review.is_some() {
                    "toggle-sidebar"
                } else {
                    "welcome-open"
                };
                this.open_modal(ModalKind::Open, None, None, anchor, window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &ToggleSidebar, window, cx| this.toggle_sidebar(window, cx)),
            )
            .on_action(cx.listener(|this, _: &CloseModal, window, cx| this.close_modal(window, cx)))
            .on_action(cx.listener(|this, _: &Submit, window, cx| this.submit(window, cx)))
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
            .child(self.top_nav(sidebar, window, cx))
            .child(
                row()
                    .items_start()
                    .gap_0()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .id("sidebar-clip")
                            .w(px(motion::SIDEBAR_WIDTH * sidebar))
                            .h_full()
                            .flex_shrink_0()
                            .overflow_hidden()
                            .opacity(sidebar)
                            .when(sidebar > 0., |d| d.child(self.sidebar(window, cx))),
                    )
                    .child(content),
            )
            .when_some(status, |d, error| {
                d.child(
                    row()
                        .px_4()
                        .py_2()
                        .border_t_1()
                        .border_color(rgb(BORDER))
                        .child(div().flex_1().text_color(rgb(0xe29595)).child(error))
                        .child(button("dismiss-error", "×").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.error = None;
                                cx.notify();
                            },
                        ))),
                )
            })
            .when(
                self.modal.as_ref().is_some_and(|m| m.anchor.is_none()),
                |d| {
                    let selection_prompt = false;
                    let panel = self.popover(window, cx);
                    if selection_prompt {
                        d.child(panel)
                    } else {
                        d.child(deferred(
                            anchored()
                                .position(self.selection_menu)
                                .snap_to_window_with_margin(px(8.))
                                .child(panel),
                        ))
                    }
                },
            )
    }
}
