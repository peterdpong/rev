use super::*;
use gpui_kit::component::text::TextView;

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

impl Patch {
    fn sidebar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let blocked = self.loading || self.workspace_busy;
        let mut projects = column().gap_3();
        for project in self.workspace.projects.clone() {
            let repo = project.repo.clone();
            let collapse_repo = repo.clone();
            let collapsed = project.collapsed;
            let mut group = column().gap_1().child(
                row().px_2()
                    .child(button(SharedString::from(format!("project-{repo}")), format!("{}  {}", if collapsed { "›" } else { "⌄" }, project.name)).flex_1().justify_start().disabled(blocked)
                        .on_click(cx.listener(move |this, _, window, cx| this.workspace_action(json!({ "type": "collapseProject", "repo": collapse_repo, "collapsed": !collapsed }), false, window, cx))))
                    .child(button(SharedString::from(format!("add-{repo}")), "+").disabled(blocked).on_click(cx.listener(move |this, _, window, cx| this.open_modal(ModalKind::Review, None, Some(repo.clone()), window, cx))))
            );
            if !collapsed {
                for tab in project.tabs {
                    let id = tab.id.clone();
                    let close = id.clone();
                    let active = self.review.as_ref().is_some_and(|r| r.key == tab.id);
                    let request = tab.request.clone();
                    let file = tab.file.clone();
                    let running = self.runs.get(&id).is_some_and(|r| r.status == "running");
                    group = group.child(
                        row()
                            .mx_2()
                            .pl_3()
                            .rounded_md()
                            .when(active, |d| d.bg(rgb(SELECTED)))
                            .child(
                                button(
                                    SharedString::from(format!("tab-{id}")),
                                    format!("{}{}", if running { "◌  " } else { "⑂  " }, tab.label),
                                )
                                .flex_1()
                                .min_w_0()
                                .justify_start()
                                .disabled(blocked)
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.load_review(request.clone(), file.clone(), window, cx)
                                    },
                                )),
                            )
                            .child(
                                button(SharedString::from(format!("close-{id}")), "×")
                                    .disabled(blocked || running)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.workspace_action(
                                            json!({ "type": "closeTab", "id": close }),
                                            active,
                                            window,
                                            cx,
                                        )
                                    })),
                            ),
                    );
                }
            }
            projects = projects.child(group);
        }
        column()
            .w(px(225.))
            .h_full()
            .flex_shrink_0()
            .bg(rgb(PANEL))
            .border_r_1()
            .border_color(rgb(BORDER))
            .child(
                row()
                    .h(px(52.))
                    .px_4()
                    .justify_between()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(16.))
                            .child("Patch"),
                    )
                    .child(button("add-project", "+").on_click(cx.listener(
                        |this, _, window, cx| {
                            this.open_modal(ModalKind::Open, None, None, window, cx)
                        },
                    ))),
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
                row()
                    .p_3()
                    .border_t_1()
                    .border_color(rgb(BORDER))
                    .child(button("open-project", "Open folder").on_click(cx.listener(
                        |this, _, window, cx| {
                            this.open_modal(ModalKind::Open, None, None, window, cx)
                        },
                    )))
                    .child(button("clone-project", "Clone").on_click(cx.listener(
                        |this, _, window, cx| {
                            this.open_modal(ModalKind::Clone, None, None, window, cx)
                        },
                    ))),
            )
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
        let busy = self.loading || self.submitting;
        column()
            .child(
                row()
                    .h(px(52.))
                    .px_4()
                    .border_b_1()
                    .border_color(rgb(BORDER))
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
                    .when(self.runs.contains_key(&review.key), |d| {
                        d.child(
                            button(
                                "agent-output",
                                self.runs
                                    .get(&review.key)
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
                                |this, _, window, cx| {
                                    this.open_modal(ModalKind::Run, None, None, window, cx)
                                },
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
        let mut list = column().gap_1().p_2();
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
                    .py_2()
                    .flex()
                    .gap_2()
                    .items_start()
                    .when(selected, |d| d.bg(rgb(SELECTED)))
                    .hover(|d| d.bg(rgb(0x25272d)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if !this.loading {
                            this.load_file(path.clone(), window, cx);
                        }
                    }))
                    .child(
                        div()
                            .w(px(13.))
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

    fn annotation(&mut self, annotation: &DiffAnnotation, cx: &mut Context<Self>) -> AnyElement {
        if annotation.id() == &ElementId::Name("selection-actions".into()) {
            return row()
                .p_2()
                .gap_2()
                .bg(rgb(SELECTED))
                .child(button("ask-selection", "Ask agent").on_click(cx.listener(
                    |this, _, window, cx| {
                        this.open_modal(ModalKind::Ask, this.selection.clone(), None, window, cx)
                    },
                )))
                .child(
                    button("change-selection", "Agent change").on_click(cx.listener(
                        |this, _, window, cx| {
                            this.open_modal(
                                ModalKind::Change,
                                this.selection.clone(),
                                None,
                                window,
                                cx,
                            )
                        },
                    )),
                )
                .into_any_element();
        }
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
        let mut annotations: Vec<_> = self
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
        if let Some(selection) = &self.selection {
            annotations.push(DiffAnnotation::line(
                "selection-actions",
                DiffLinePosition::new(selection.path.clone(), selection.diff_side(), selection.end),
            ));
        }
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
                .on_add_annotation(move |range, _, cx| {
                    let _ = add_owner.update(cx, |this, cx| {
                        this.selection = Selection::from_range(range);
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
            .child(div().flex_1().min_h_0().overflow_hidden().child(content))
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
            ModalKind::Clone => "Clone repository",
            ModalKind::Review => "Open branch or PR",
            ModalKind::Ask => "Ask agent",
            ModalKind::Change => "Agent change",
            ModalKind::Changes => "Changes",
            ModalKind::Run => "Agent response",
        };
        let mut body = column().gap_4();
        let mut footer = row().justify_end().gap_2();
        if matches!(modal.kind, ModalKind::Open | ModalKind::Review) {
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
        } else if modal.kind == ModalKind::Clone {
            body = body
                .child(self.form_field(0, "Git URL", "HTTPS or SSH repository URL"))
                .child(self.form_field(1, "Destination", "New local folder"));
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
                    ModalKind::Clone => "Clone",
                    _ => "Open",
                }
            };
            footer = footer.child(
                Button::new("submit-modal")
                    .primary()
                    .label(label)
                    .disabled(
                        self.submitting
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
