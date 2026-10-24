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

impl Patch {
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
                    let running = self
                        .runs
                        .get(tab.scope_key.as_ref().unwrap_or(&id))
                        .is_some_and(|r| r.status == "running");
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
                                    if running {
                                        Icon::new(IconName::LoaderCircle)
                                            .size(px(14.))
                                            .into_any_element()
                                    } else {
                                        div().child("⑂").into_any_element()
                                    },
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
                                .disabled(blocked || running)
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
                            .child("Patch"),
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
        let queued = self
            .state
            .comments
            .iter()
            .filter(|c| c.status == "queued")
            .count();
        let total_added: usize = review.files.iter().map(|f| f.added).sum();
        let total_removed: usize = review.files.iter().map(|f| f.removed).sum();
        let working = review.request.mode.as_deref() == Some("working");
        let mut request = review.request.clone();
        request.mode = Some(if working { "branch" } else { "working" }.into());
        request.commit_range = None;
        let busy = self.loading || self.submitting;
        let run_scope = self
            .runs
            .iter()
            .find(|(scope, run)| {
                run.status == "running"
                    && self.run_requests.get(*scope).is_some_and(|request| {
                        request.repo == review.request.repo
                            && request.base == review.request.base
                            && request.head == review.request.head
                    })
            })
            .map(|(scope, _)| scope.clone())
            .or_else(|| {
                self.runs
                    .contains_key(&review.scope_key)
                    .then(|| review.scope_key.clone())
            });
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
                    )
                    .when_some(run_scope, |d, scope| {
                        d.child(
                            button(
                                "agent-output",
                                self.runs
                                    .get(&scope)
                                    .map(|r| {
                                        if r.status == "running" {
                                            "Agent running…"
                                        } else {
                                            "Agent response"
                                        }
                                    })
                                    .unwrap_or_default(),
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| this.open_run(scope.clone(), window, cx),
                            )),
                        )
                    })
                    .child(
                        Button::new("ask-review")
                            .small()
                            .label("Ask agent")
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_modal(ModalKind::Ask, None, None, window, cx)
                            })),
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
                    )
                    .child(
                        button("changes", format!("Changes ({queued})"))
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_modal(ModalKind::Changes, None, None, window, cx)
                            })),
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

    fn prompt_tabs(&self, kind: ModalKind, cx: &mut Context<Self>) -> AnyElement {
        TabBar::new("prompt-mode-tabs")
            .segmented()
            .small()
            .selected_index(usize::from(kind == ModalKind::Change))
            .child(
                Tab::new()
                    .label("Ask agent")
                    .aria_label("Ask agent, Command 1")
                    .disabled(self.submitting),
            )
            .child(
                Tab::new()
                    .label("Agent change")
                    .aria_label("Agent change, Command 2")
                    .disabled(self.submitting),
            )
            .on_click(cx.listener(|this, index, window, cx| {
                this.switch_prompt_mode(
                    if *index == 0 {
                        ModalKind::Ask
                    } else {
                        ModalKind::Change
                    },
                    window,
                    cx,
                );
            }))
            .into_any_element()
    }

    fn provider_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = column().gap_2();
        let mut providers = row().gap_2();
        for provider in &self.providers {
            let id = provider.id.clone();
            let label = if id == "codex" {
                "Codex"
            } else {
                "Claude Code"
            };
            providers = providers.child(
                button(SharedString::from(format!("provider-{id}")), label)
                    .selected(self.provider == id)
                    .disabled(!provider.available || self.submitting)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.provider = id.clone();
                        cx.notify();
                    })),
            );
        }
        body = body.child(providers);
        if !self.providers.iter().any(|p| p.available) {
            body = body.child(
                div()
                    .text_color(rgb(MUTED))
                    .child("Install and sign in to Codex or Claude Code, then restart Patch."),
            );
        }
        body.into_any_element()
    }

    fn selection_prompt(&self, modal: &Modal, cx: &mut Context<Self>) -> AnyElement {
        let pane = column()
            .id("selection-prompt")
            .w(px(440.))
            .gap_2()
            .p_3()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(BORDER))
            .rounded_lg()
            .shadow_lg()
            .occlude()
            .track_focus(&self.modal_focus)
            .key_context("PatchPrompt")
            .child(
                row()
                    .child(self.prompt_tabs(modal.kind, cx))
                    .child(div().flex_1())
                    .child(
                        button("close-modal", "×").on_click(
                            cx.listener(|this, _, window, cx| this.close_modal(window, cx)),
                        ),
                    ),
            )
            .child(
                Textarea::new(&self.prompt)
                    .accessibility_id("agent-prompt")
                    .aria_label("Agent prompt")
                    .h(px(110.))
                    .disabled(self.submitting),
            )
            .when(modal.kind == ModalKind::Ask, |pane| {
                pane.child(self.provider_controls(cx))
            })
            .when_some(self.error.clone(), |pane, error| {
                pane.child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(0xe29595))
                        .child(error),
                )
            })
            .child(
                row()
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_size(px(11.))
                            .text_color(rgb(MUTED))
                            .child(
                                modal
                                    .selection
                                    .as_ref()
                                    .map(Selection::label)
                                    .unwrap_or_default(),
                            ),
                    )
                    .child(
                        Button::new("submit-modal")
                            .primary()
                            .small()
                            .label(if self.submitting {
                                "Working…"
                            } else if modal.kind == ModalKind::Change {
                                "Queue change"
                            } else {
                                "Ask agent"
                            })
                            .disabled(self.submitting)
                            .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                    ),
            );
        deferred(
            anchored()
                .position(self.selection_menu)
                .snap_to_window_with_margin(px(8.))
                .child(pane),
        )
        .into_any_element()
    }

    fn annotation(&mut self, annotation: &DiffAnnotation, _: &mut Context<Self>) -> AnyElement {
        let comment = self
            .state
            .comments
            .iter()
            .find(|c| annotation.id() == &ElementId::Name(c.id.clone().into()));
        match comment {
            Some(comment) => column()
                .p_3()
                .gap_1()
                .bg(rgb(0x202329))
                .border_l_2()
                .border_color(rgb(BLUE))
                .child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(
                    if comment.digest != self.patch.digest {
                        "Outdated feedback"
                    } else if comment.status == "sent" {
                        "Sent"
                    } else {
                        "Queued change"
                    },
                ))
                .child(div().text_size(px(13.)).child(comment.body.clone()))
                .into_any_element(),
            None => div().into_any_element(),
        }
    }

    fn diff_view(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let annotations: Vec<_> = self
            .state
            .comments
            .iter()
            .filter(|c| {
                Some(&c.selection.path) == self.file.as_ref() && c.digest == self.patch.digest
            })
            .map(|c| {
                DiffAnnotation::line(
                    SharedString::from(c.id.clone()),
                    DiffLinePosition::new(
                        c.selection.path.clone(),
                        c.selection.diff_side(),
                        c.selection.end,
                    ),
                )
            })
            .collect();
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
                .annotations(annotations)
                .render_annotation(move |annotation, _, cx| {
                    owner
                        .update(cx, |this, cx| this.annotation(annotation, cx))
                        .unwrap_or_else(|_| div().into_any_element())
                })
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
        if Self::is_selection_prompt(&modal) {
            return if self.selecting {
                div().into_any_element()
            } else {
                self.selection_prompt(&modal, cx)
            };
        }
        let title = match modal.kind {
            ModalKind::Open => "Open repository",
            ModalKind::Review => "Open branch or PR",
            ModalKind::Ask => "Ask agent",
            ModalKind::Change => "Agent change",
            ModalKind::Changes => "Changes",
            ModalKind::Run => "Agent response",
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
        } else if matches!(modal.kind, ModalKind::Ask | ModalKind::Change) {
            body = body
                .child(
                    div().text_size(px(12.)).text_color(rgb(MUTED)).child(
                        modal
                            .selection
                            .as_ref()
                            .map(Selection::label)
                            .unwrap_or_else(|| {
                                format!(
                                    "{} → {}",
                                    modal.request.base.as_deref().unwrap_or("base"),
                                    modal.request.head.as_deref().unwrap_or("HEAD")
                                )
                            }),
                    ),
                )
                .child(
                    Textarea::new(&self.prompt)
                        .accessibility_id("agent-prompt")
                        .aria_label("Agent prompt")
                        .h(px(140.))
                        .disabled(self.submitting),
                );
        } else if modal.kind == ModalKind::Changes {
            let mut comments = column().gap_3();
            if self.state.comments.is_empty() {
                comments =
                    comments.child(div().text_color(rgb(MUTED)).child("No feedback queued."));
            }
            for comment in self.state.comments.clone() {
                let id = comment.id.clone();
                let remove = id.clone();
                let selected = self.comment_ids.contains(&id);
                comments = comments.child(
                    column()
                        .gap_2()
                        .p_3()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .child(
                            row()
                                .child(
                                    button(
                                        SharedString::from(format!("select-{id}")),
                                        if comment.status == "sent" {
                                            "Sent"
                                        } else if selected {
                                            "☑"
                                        } else {
                                            "☐"
                                        },
                                    )
                                    .disabled(comment.status == "sent" || self.submitting)
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            if !this.comment_ids.remove(&id) {
                                                this.comment_ids.insert(id.clone());
                                            }
                                            this.preview = None;
                                            cx.notify();
                                        },
                                    )),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .text_color(rgb(MUTED))
                                        .text_size(px(11.))
                                        .child(comment.selection.label()),
                                )
                                .child(
                                    button(
                                        SharedString::from(format!("remove-{remove}")),
                                        "Remove",
                                    )
                                    .disabled(self.submitting)
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            this.delete_comment(remove.clone(), window, cx)
                                        },
                                    )),
                                ),
                        )
                        .child(div().child(comment.body)),
                );
            }
            body = body.child(comments);
            footer = footer.child(
                button("preview-prompt", "Preview prompt")
                    .disabled(self.submitting || self.comment_ids.is_empty())
                    .on_click(cx.listener(|this, _, window, cx| this.preview_prompt(window, cx))),
            );
        } else if modal.kind == ModalKind::Run {
            if let Some(run) = self.runs.get(&modal.tab) {
                body = body.child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(MUTED))
                        .child(format!(
                            "{} · {}",
                            if run.mode == "apply" {
                                "Agent changes"
                            } else {
                                "Question"
                            },
                            run.status
                        )),
                );
                for (index, event) in run.events.iter().enumerate() {
                    body = body.child(if event.kind == "message" {
                        TextView::markdown(("agent-message", index), event.text.clone())
                            .selectable(true)
                            .into_any_element()
                    } else {
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(if event.kind == "error" {
                                0xe29595
                            } else {
                                MUTED
                            }))
                            .child(event.text.clone())
                            .into_any_element()
                    });
                }
                if run.events.is_empty() {
                    body = body.child(div().text_color(rgb(MUTED)).child("Starting agent…"));
                }
                if run.status == "running" {
                    footer =
                        footer.child(button("stop-agent", "Stop").on_click(
                            cx.listener(|this, _, window, cx| this.cancel_run(window, cx)),
                        ));
                }
                let transcript = run
                    .events
                    .iter()
                    .map(|event| event.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n");
                footer = footer.child(button("copy-response", "Copy").on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(transcript.clone()))
                }));
            }
            footer = footer.child(
                button("close-response", "Close")
                    .on_click(cx.listener(|this, _, window, cx| this.close_modal(window, cx))),
            );
        }
        if matches!(modal.kind, ModalKind::Ask | ModalKind::Changes) {
            body = body.child(self.provider_controls(cx));
        }
        if let Some(preview) = &self.preview {
            let text = preview.clone();
            body = body
                .child(row().child(div().flex_1().child("Prompt preview")).child(
                    button("copy-prompt", "Copy").on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()))
                    }),
                ))
                .child(
                    div()
                        .p_3()
                        .bg(rgb(PANEL))
                        .text_size(px(11.))
                        .font_family("Menlo")
                        .child(preview.clone()),
                );
        }
        if let Some(error) = &self.error {
            body = body.child(div().text_color(rgb(0xe29595)).child(error.clone()));
        }
        if modal.kind != ModalKind::Run {
            let label = if self.submitting {
                "Working…"
            } else {
                match modal.kind {
                    ModalKind::Ask => "Ask agent",
                    ModalKind::Change => "Queue change",
                    ModalKind::Changes => "Send to agent",
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
                            || (modal.kind == ModalKind::Commits && self.commit_picker.loading)
                            || (modal.kind == ModalKind::Changes && self.comment_ids.is_empty()),
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

impl Render for Patch {
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
                        ))
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
            .id("patch")
            .relative()
            .size_full()
            .bg(rgb(BG))
            .text_color(rgb(0xe4e6eb))
            .text_size(px(13.))
            .font_family(".AppleSystemUIFont")
            .track_focus(&self.focus)
            .key_context("Patch")
            .on_action(cx.listener(|this, _: &OpenProject, window, cx| {
                this.open_modal(ModalKind::Open, None, None, window, cx)
            }))
            .on_action(cx.listener(|this, _: &AskReview, window, cx| {
                if this.review.is_some() {
                    this.open_modal(ModalKind::Ask, None, None, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &PromptAsk, window, cx| {
                this.switch_prompt_mode(ModalKind::Ask, window, cx)
            }))
            .on_action(cx.listener(|this, _: &PromptChange, window, cx| {
                this.switch_prompt_mode(ModalKind::Change, window, cx)
            }))
            .on_action(cx.listener(|this, _: &TogglePromptMode, window, cx| {
                this.toggle_prompt_mode(window, cx)
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
