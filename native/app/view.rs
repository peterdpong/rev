use super::*;
use gpui_kit::component::{Icon, IconName, text::TextView};

const BG: u32 = 0x17181b;
const PANEL: u32 = 0x111215;
const BORDER: u32 = 0x2c2e33;
const MUTED: u32 = 0x9298a3;
const BLUE: u32 = 0x8ab4ff;
const SELECTED: u32 = 0x253148;

fn row() -> Div {
    div().flex().items_center().gap_2()
}
fn column() -> Div {
    div().flex().flex_col()
}
fn button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
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
    fn sidebar_toggle(&self, cx: &mut Context<Self>) -> Button {
        let label = if self.sidebar_collapsed {
            "Show sidebar"
        } else {
            "Hide sidebar"
        };
        navigation_action(
            "toggle-sidebar",
            if self.sidebar_collapsed {
                IconName::PanelLeftOpen
            } else {
                IconName::PanelLeftClose
            },
            label,
        )
        .tooltip(format!("{label} (⌘B)"))
        .on_click(cx.listener(|this, _, window, cx| this.toggle_sidebar(window, cx)))
    }

    fn sidebar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let blocked = self.loading || self.workspace_busy;
        let mut projects = column().gap_3().min_w_0();
        for project in self.workspace.projects.clone() {
            let repo = project.repo.clone();
            let collapse_repo = repo.clone();
            let collapsed = project.collapsed;
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
            let add_review = navigation_action(
                SharedString::from(format!("add-{repo}")),
                IconName::Plus,
                "Open branch or PR",
            )
            .disabled(blocked)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_modal(ModalKind::Review, None, Some(repo.clone()), window, cx)
            }));
            let mut group = column().gap_1().min_w_0().child(
                row()
                    .mx_2()
                    .gap_0()
                    .min_w_0()
                    .child(project_button)
                    .child(add_review),
            );
            if !collapsed {
                for tab in project.tabs {
                    let id = tab.id.clone();
                    let close = id.clone();
                    let active = self.review.as_ref().is_some_and(|r| r.key == tab.id);
                    let request = tab.request.clone();
                    let file = tab.file.clone();

                    group = group.child(
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
                                .disabled(blocked)
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
                            ),
                    );
                }
            }
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
                    .h(px(52.))
                    .px_2()
                    .justify_between()
                    .child(
                        div()
                            .pl_2()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(16.))
                            .child("Rev"),
                    )
                    .child(
                        navigation_action("add-project", IconName::Plus, "Open repository")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_modal(ModalKind::Open, None, None, window, cx)
                            })),
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
            .child(row().p_3().border_t_1().border_color(rgb(BORDER)).child(
                button("open-project", "Open folder").on_click(cx.listener(
                    |this, _, window, cx| this.open_modal(ModalKind::Open, None, None, window, cx),
                )),
            ))
            .into_any_element()
    }

    fn toolbar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(review) = self.review.clone() else {
            return div().into_any_element();
        };

        let total_added: usize = review.files.iter().map(|f| f.added).sum();
        let total_removed: usize = review.files.iter().map(|f| f.removed).sum();
        let working = review.request.mode.as_deref() == Some("working");
        let mut request = review.request.clone();
        request.mode = Some(if working { "branch" } else { "working" }.into());
        request.commit_range = None;
        let busy = self.loading || self.submitting;

        column()
            .child(
                row()
                    .h(px(52.))
                    .px_4()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(self.sidebar_toggle(cx))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(review.name))
                    .child(
                        div()
                            .text_color(rgb(MUTED))
                            .flex_1()
                            .truncate()
                            .child(format!(
                                "{}  →  {}",
                                review.request.base.as_deref().unwrap_or("main"),
                                review.request.head.as_deref().unwrap_or("HEAD")
                            )),
                    ),
            )
            .child(
                row()
                    .h(px(42.))
                    .px_3()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .text_color(rgb(MUTED))
                            .child(format!("{} files", review.files.len())),
                    )
                    .child(
                        div()
                            .text_color(rgb(0x88c99b))
                            .child(format!("+{total_added}")),
                    )
                    .child(
                        div()
                            .text_color(rgb(0xe29595))
                            .child(format!("−{total_removed}")),
                    )
                    .child(div().flex_1())
                    .child(
                        button(
                            "choose-commits",
                            if let Some(range) = &review.request.commit_range {
                                if review.commit_count == 1 {
                                    range.end[..8].to_string()
                                } else {
                                    format!("{} commits", review.commit_count)
                                }
                            } else {
                                "All commits".into()
                            },
                        )
                        .dropdown_caret(true)
                        .disabled(busy || self.patch_loading)
                        .tooltip("Review one commit or a range of commits")
                        .on_click(
                            cx.listener(|this, _, window, cx| this.open_commit_picker(window, cx)),
                        ),
                    )
                    .child(
                        button(
                            "scope",
                            if working {
                                "Working tree"
                            } else if review.dirty {
                                "Committed · local edits"
                            } else {
                                "Committed"
                            },
                        )
                        .disabled(busy)
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.load_review(request.clone(), this.file.clone(), window, cx)
                            },
                        )),
                    )
                    .child(
                        button("refresh", "Refresh")
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, window, cx| this.refresh(window, cx))),
                    )
                    .child(
                        button("fetch", "Fetch")
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, window, cx| this.fetch(window, cx))),
                    ),
            )
            .into_any_element()
    }

    fn file_list(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let files = self.visible_files(cx);
        let mut list = column().gap_1().p_2().min_w_0();
        for (index, file) in files.iter().enumerate() {
            let path = file.path.clone();
            let selected = self.file.as_ref() == Some(&path);
            let viewed = self.state.viewed.contains_key(&path);
            list = list.child(
                div()
                    .id(("file", index))
                    .cursor_pointer()
                    .rounded_md()
                    .px_2()
                    .h(px(32.))
                    .min_w_0()
                    .flex()
                    .gap_2()
                    .items_center()
                    .when(selected, |d| d.bg(rgb(SELECTED)))
                    .hover(|d| d.bg(rgb(0x25272d)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if !this.loading {
                            this.load_file(path.clone(), window, cx);
                        }
                    }))
                    .child(
                        div()
                            .w(px(16.))
                            .flex_shrink_0()
                            .text_size(px(12.))
                            .text_center()
                            .text_color(if viewed { rgb(0x88c99b) } else { rgb(MUTED) })
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
                            .child(file.path.clone()),
                    ),
            );
        }
        column()
            .w(px(210.))
            .h_full()
            .flex_shrink_0()
            .border_r_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .p_2()
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
            .h(px(40.))
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

    fn dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(modal) = self.modal.clone() else {
            return div().into_any_element();
        };

        let title = match modal.kind {
            ModalKind::Open => "Open repository",
            ModalKind::Review => "Open branch or PR",

            ModalKind::Commits => "Commits",
        };
        let mut body = column().gap_4();
        let mut footer = row().justify_end().gap_2();
        if modal.kind == ModalKind::Commits {
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
        {
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
                    .bg(rgb(BG))
                    .border_1()
                    .border_color(rgb(BORDER))
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
                            .p_4()
                            .border_b_1()
                            .border_color(rgb(BORDER))
                            .child(
                                div()
                                    .flex_1()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .child(button("close-modal", "×").on_click(
                                cx.listener(|this, _, window, cx| this.close_modal(window, cx)),
                            )),
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
                            .p_4()
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
        let content = if self.review.is_some() {
            column()
                .flex_1()
                .min_w_0()
                .h_full()
                .child(self.toolbar(cx))
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
                .relative()
                .flex_1()
                .h_full()
                .items_center()
                .justify_center()
                .gap_4()
                .child(
                    div()
                        .absolute()
                        .top(px(12.))
                        .left(px(16.))
                        .child(self.sidebar_toggle(cx)),
                )
                .child(
                    div()
                        .text_size(px(24.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Open a review"),
                )
                .child(
                    row()
                        .child(Button::new("welcome-open").label("Open folder").on_click(
                            cx.listener(|this, _, window, cx| {
                                this.open_modal(ModalKind::Open, None, None, window, cx)
                            }),
                        )),
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
                this.open_modal(ModalKind::Open, None, None, window, cx)
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
            .child(
                row()
                    .items_start()
                    .gap_0()
                    .flex_1()
                    .min_h_0()
                    .when(!self.sidebar_collapsed, |row| row.child(self.sidebar(cx)))
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
            .when(self.modal.is_some(), |d| d.child(self.dialog(window, cx)))
    }
}
