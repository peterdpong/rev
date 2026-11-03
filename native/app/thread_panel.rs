use super::view::{BLUE, BORDER, MUTED, PANEL, SELECTED, button, column, row};
use super::*;
use gpui_kit::component::text::TextView;

impl Rev {
    fn thread_changes(&self, thread: &ReviewThread) -> Vec<Comment> {
        if !thread.changes.is_empty() {
            return thread.changes.clone();
        }
        self.state
            .comments
            .iter()
            .filter(|comment| thread.comment_ids.contains(&comment.id))
            .cloned()
            .collect()
    }

    fn thread_run(&self, thread: &ReviewThread) -> Option<(&String, &AgentRun)> {
        self.runs
            .iter()
            .filter(|(_, run)| {
                run.thread
                    .as_ref()
                    .is_some_and(|saved| saved.id == thread.id)
            })
            .max_by_key(|(_, run)| {
                (
                    if !run.approvals.is_empty() {
                        4
                    } else if run.status == "running" {
                        3
                    } else if run.status == "queued" {
                        2
                    } else {
                        1
                    },
                    thread.turns.iter().position(|turn| turn.id == run.id),
                )
            })
    }

    fn thread_status(&self, thread: &ReviewThread) -> &'static str {
        let status = self
            .thread_run(thread)
            .map(|(_, run)| run.status.as_str())
            .or_else(|| thread.turns.last().map(|turn| turn.status.as_str()))
            .unwrap_or("");
        if self
            .thread_run(thread)
            .is_some_and(|(_, run)| !run.approvals.is_empty())
        {
            return "Needs approval";
        }
        match status {
            "queued" if self.thread_run(thread).is_some() => "Queued",
            "queued" => "Interrupted",
            "running" if self.thread_run(thread).is_some() => "Running",
            "running" => "Interrupted",
            "completed" if thread.kind == "question" => "Answered",
            "completed" => "Completed",
            "failed" => "Failed",
            "cancelled" => "Stopped",
            _ => "Saved",
        }
    }

    fn source_reference(
        &self,
        id: String,
        selection: &Selection,
        digest: &str,
        snippet: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if selection.scope.as_deref() == Some("review") {
            return div()
                .text_size(px(11.))
                .text_color(rgb(MUTED))
                .child("Entire review")
                .into_any_element();
        }
        if selection.scope.as_deref() == Some("file") {
            let path = selection.path.clone();
            return button(SharedString::from(id), path.clone())
                .tooltip("Jump to file")
                .on_click(
                    cx.listener(move |this, _, window, cx| {
                        this.jump_file(path.clone(), window, cx)
                    }),
                )
                .into_any_element();
        }
        let current = self
            .patches
            .get(&selection.path)
            .is_some_and(|patch| patch.digest == digest);
        let source = selection.clone();
        let digest = digest.to_owned();
        let label = format!("{}:{}–{}", source.path, source.start, source.end);
        let mut view = column().gap_1().min_w_0().child(
            Button::new(SharedString::from(id.clone()))
                .small()
                .ghost()
                .accessibility_label(label.clone())
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .truncate()
                        .text_size(px(11.))
                        .child(label),
                )
                .h(px(24.))
                .px_0()
                .w_full()
                .text_color(rgb(BLUE))
                .tooltip(selection.label())
                .disabled(!current)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.jump_to_code(source.clone(), digest.clone(), window, cx)
                })),
        );
        if !current {
            view = view.child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(MUTED))
                    .child("Earlier revision"),
            );
        } else if snippet
            && let Some(file) = self
                .diff
                .read(cx)
                .files()
                .iter()
                .find(|file| file.path().as_str() == selection.path)
        {
            let mut lines = column()
                .id(SharedString::from(format!("snippet-{id}")))
                .px_2()
                .py_1()
                .rounded_md()
                .bg(rgb(0x0d0f12))
                .min_w_0();
            for number in selection.start..=selection.end.min(selection.start.saturating_add(2)) {
                let text = file.text_for_lines(selection.diff_side(), number, number);
                lines = lines.child(
                    row()
                        .gap_2()
                        .text_size(px(11.))
                        .font_family("Menlo")
                        .child(
                            div()
                                .w(px(30.))
                                .text_color(rgb(MUTED))
                                .child(number.to_string()),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .truncate()
                                .child(text.trim_end().to_string()),
                        ),
                );
            }
            if selection.end.saturating_sub(selection.start) >= 3 {
                lines = lines.child(div().text_size(px(11.)).text_color(rgb(MUTED)).child("…"));
            }
            view = view.child(lines);
        }
        view.into_any_element()
    }

    fn conversation(
        &self,
        thread: &ReviewThread,
        active: bool,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut body = column().gap_2().min_w_0();
        let turns: Vec<_> = if active {
            thread.turns.iter().collect()
        } else {
            thread.turns.last().into_iter().collect()
        };
        for turn in turns {
            if active
                && thread
                    .turns
                    .first()
                    .is_some_and(|first| first.id != turn.id)
            {
                body = body.child(
                    div()
                        .pt_2()
                        .border_t_1()
                        .border_color(rgb(BORDER))
                        .text_size(px(12.))
                        .child(turn.question.clone()),
                );
            }
            let events = self
                .runs
                .get(&turn.id)
                .map_or(&turn.events, |run| &run.events);
            let messages: Vec<_> = events
                .iter()
                .enumerate()
                .filter(|(_, event)| event.kind == "message" || event.kind == "error")
                .collect();
            if messages.is_empty() {
                if matches!(turn.status.as_str(), "running" | "queued") {
                    let activity = events
                        .iter()
                        .rev()
                        .find(|event| event.kind == "activity")
                        .map_or(
                            if turn.status == "queued" {
                                "Queued"
                            } else {
                                "Working…"
                            },
                            |event| event.text.as_str(),
                        );
                    body = body.child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(MUTED))
                            .truncate()
                            .child(activity.chars().take(160).collect::<String>()),
                    );
                }
                continue;
            }
            let messages = if active {
                messages
            } else {
                messages.into_iter().rev().take(1).collect()
            };
            for (index, event) in messages {
                let content = if active {
                    event.text.clone()
                } else {
                    let mut text: String = event.text.chars().take(240).collect();
                    if event.text.chars().count() > 240 {
                        text.push('…');
                    }
                    text
                };
                body = body.child(
                    div()
                        .min_w_0()
                        .text_size(px(12.))
                        .when(event.kind == "error", |d| d.text_color(rgb(0xee9292)))
                        .child(
                            TextView::markdown(
                                SharedString::from(format!("{}-answer-{index}", turn.id)),
                                content,
                            )
                            .selectable(true),
                        ),
                );
            }
        }
        body.into_any_element()
    }

    fn thread_card(&self, thread: &ReviewThread, cx: &mut Context<Self>) -> AnyElement {
        let active = self.active_thread.as_ref() == Some(&thread.id);
        let status = self.thread_status(thread);
        let question = thread.kind == "question";
        let stale = self.review.as_ref().is_none_or(|review| {
            thread.head_sha != review.head_sha || thread.merge_base != review.merge_base
        });
        let id = thread.id.clone();
        let archive_id = id.clone();
        let archived = thread.archived;
        let mut card = column()
            .min_w_0()
            .flex_shrink_0()
            .rounded_lg()
            .border_1()
            .border_color(rgb(if active { 0x46546b } else { BORDER }))
            .bg(rgb(PANEL))
            .overflow_hidden()
            .child(
                div()
                    .id(SharedString::from(format!("thread-{}", thread.id)))
                    .cursor_pointer()
                    .px_3()
                    .py_2()
                    .hover(|d| d.bg(rgb(SELECTED).opacity(0.35)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select_thread(id.clone(), window, cx);
                        if active {
                            this.active_thread = None;
                            this.thread_scroll_target = None;
                            cx.notify();
                        }
                    }))
                    .child(
                        row()
                            .text_size(px(11.))
                            .text_color(rgb(MUTED))
                            .child(div().flex_1().text_color(rgb(BLUE)).child(if question {
                                "Ask agent"
                            } else {
                                "Agent change"
                            }))
                            .child(
                                div()
                                    .id(SharedString::from(format!("status-{}", thread.id)))
                                    .text_color(rgb(if status == "Failed" {
                                        0xee9292
                                    } else if status == "Running" {
                                        BLUE
                                    } else {
                                        MUTED
                                    }))
                                    .child(status),
                            )
                            .when(status == "Running", |d| {
                                d.child(gpui_kit::component::spinner::Spinner::new().small())
                            })
                            .when(question, |d| {
                                d.child(
                                    Button::new(SharedString::from(format!(
                                        "archive-{}",
                                        thread.id
                                    )))
                                    .ghost()
                                    .small()
                                    .icon(if archived {
                                        gpui_kit::assets::IconName::ArchiveRestore
                                    } else {
                                        gpui_kit::assets::IconName::Archive
                                    })
                                    .accessibility_label(if archived {
                                        "Restore question"
                                    } else {
                                        "Archive question"
                                    })
                                    .tooltip(if archived {
                                        "Restore question"
                                    } else {
                                        "Archive question"
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.archive_thread(
                                            archive_id.clone(),
                                            !archived,
                                            window,
                                            cx,
                                        );
                                    })),
                                )
                            })
                            .child(if active { "⌄" } else { "›" }),
                    )
                    .when(question, |d| {
                        d.child(div().mt_1().text_size(px(13.)).child(thread.body.clone()))
                    }),
            );
        let mut content = column().px_3().pb_2().gap_2().min_w_0();
        if question {
            if let (Some(selection), Some(digest)) = (&thread.selection, &thread.digest) {
                content = content.child(self.source_reference(
                    format!("code-{}", thread.id),
                    selection,
                    digest,
                    false,
                    cx,
                ));
            } else {
                content = content.child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(MUTED))
                        .child("Entire review"),
                );
            }
        } else {
            let changes = self.thread_changes(thread);
            if changes.is_empty() {
                content = content.child(div().text_size(px(12.)).child(thread.body.clone()));
                if let (Some(selection), Some(digest)) = (&thread.selection, &thread.digest) {
                    content = content.child(self.source_reference(
                        format!("code-{}", thread.id),
                        selection,
                        digest,
                        true,
                        cx,
                    ));
                }
            }
            for change in changes.iter().take(if active { usize::MAX } else { 3 }) {
                let removable = self
                    .state
                    .comments
                    .iter()
                    .any(|comment| comment.id == change.id && comment.status == "queued");
                let id = change.id.clone();
                content = content.child(
                    column()
                        .gap_1()
                        .child(
                            row()
                                .items_start()
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex_1()
                                        .text_size(px(12.))
                                        .child(change.body.clone()),
                                )
                                .when(removable, |d| {
                                    d.child(
                                        button(
                                            SharedString::from(format!(
                                                "remove-tag-{}-{id}",
                                                thread.id
                                            )),
                                            "×",
                                        )
                                        .tooltip("Remove queued request")
                                        .disabled(
                                            self.submitting || self.repository_agent_running(),
                                        )
                                        .on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                this.remove_queued_change(id.clone(), window, cx)
                                            }),
                                        ),
                                    )
                                }),
                        )
                        .child(self.source_reference(
                            format!("change-source-{}-{}", thread.id, change.id),
                            &change.selection,
                            &change.digest,
                            true,
                            cx,
                        )),
                );
            }
            if !active && changes.len() > 3 {
                content = content.child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(
                    format!("{} more requests · expand to view", changes.len() - 3),
                ));
            }
        }
        content = content.child(self.conversation(thread, active, cx));
        // Approval requests remain visible even when the conversation is collapsed.
        for run in self
            .runs
            .values()
            .filter(|run| run.thread.as_ref().is_some_and(|t| t.id == thread.id))
        {
            for approval in &run.approvals {
                let run_id = run.id.clone();
                let allow_id = approval.id.clone();
                let deny_id = approval.id.clone();
                let deny_run = run_id.clone();
                content = content.child(
                    column()
                        .gap_1()
                        .p_2()
                        .rounded_md()
                        .bg(rgb(SELECTED))
                        .child(div().text_size(px(12.)).child(approval.title.clone()))
                        .child(
                            div()
                                .id(SharedString::from(format!(
                                    "approval-detail-{}",
                                    approval.id
                                )))
                                .max_h(px(120.))
                                .overflow_y_scroll()
                                .text_size(px(11.))
                                .child(approval.detail.clone()),
                        )
                        .child(
                            row()
                                .child(
                                    button(
                                        SharedString::from(format!("allow-{allow_id}")),
                                        "Allow",
                                    )
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            this.respond_approval(
                                                run_id.clone(),
                                                allow_id.clone(),
                                                true,
                                                window,
                                                cx,
                                            );
                                        },
                                    )),
                                )
                                .child(
                                    button(SharedString::from(format!("deny-{deny_id}")), "Deny")
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.respond_approval(
                                                deny_run.clone(),
                                                deny_id.clone(),
                                                false,
                                                window,
                                                cx,
                                            );
                                        })),
                                ),
                        ),
                );
            }
        }
        let worktree = self
            .thread_run(thread)
            .and_then(|(_, run)| run.worktree.as_ref())
            .or_else(|| {
                thread
                    .turns
                    .iter()
                    .rev()
                    .find_map(|turn| turn.worktree.as_ref())
            });
        if let Some(worktree) = worktree {
            let repo = worktree.path.clone();
            let location = repo.clone();
            let branch = worktree.branch.clone();
            let base = thread.head_sha.clone();
            content = content.child(
                row()
                    .child(
                        button(
                            SharedString::from(format!("open-changes-{}", thread.id)),
                            "Open changes",
                        )
                        .tooltip(format!("{} · {}", worktree.branch, worktree.path))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.load_review(
                                    ReviewRequest {
                                        repo: repo.clone(),
                                        base: Some(base.clone()),
                                        head: Some("HEAD".into()),
                                        mode: Some("working".into()),
                                        commit_range: None,
                                    },
                                    None,
                                    window,
                                    cx,
                                )
                            },
                        )),
                    )
                    .child(
                        button(
                            SharedString::from(format!("copy-worktree-{}", thread.id)),
                            "Copy path",
                        )
                        .tooltip(branch)
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(location.clone()))
                        }),
                    ),
            );
        }
        if active {
            if let Some((scope, run)) = self.thread_run(thread) {
                let scope = scope.clone();
                let run_id = run.id.clone();
                content = content.child(
                    row()
                        .child(
                            button(
                                SharedString::from(format!("run-details-{}", thread.id)),
                                "Run details",
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| this.open_run(scope.clone(), window, cx),
                            )),
                        )
                        .when(run.is_active(), |d| {
                            d.child(
                                button(
                                    SharedString::from(format!("stop-thread-{}", thread.id)),
                                    "Stop",
                                )
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.rpc(
                                            "cancel",
                                            json!({ "id": run_id }),
                                            window,
                                            cx,
                                            |this, result, _, _| {
                                                this.result(result);
                                            },
                                        );
                                    },
                                )),
                            )
                        }),
                );
            }
            if (!question || !stale) && !thread.archived {
                content = content.child(
                    column()
                        .key_context("RevThread")
                        .gap_2()
                        .pt_1()
                        .on_action(cx.listener(|this, _: &ReplyThread, window, cx| {
                            this.reply_thread(window, cx)
                        }))
                        .child(
                            Textarea::new(&self.thread_reply)
                                .accessibility_id("thread-reply")
                                .aria_label(if question {
                                    "Ask a follow-up"
                                } else {
                                    "Follow up on changes"
                                })
                                .h(px(72.))
                                .w_full(),
                        )
                        .child(
                            Button::new("reply-thread")
                                .small()
                                .label(
                                    if matches!(status, "Running" | "Queued" | "Needs approval") {
                                        "Queue follow-up"
                                    } else {
                                        "Send"
                                    },
                                )
                                .disabled(self.submitting)
                                .on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.reply_thread(window, cx)
                                    }),
                                ),
                        ),
                );
            }
        }
        card = card.child(content);
        card.into_any_element()
    }

    fn queued_card(&self, comment: &Comment, cx: &mut Context<Self>) -> AnyElement {
        let id = comment.id.clone();
        column()
            .id(SharedString::from(format!("queued-{}", comment.id)))
            .min_w_0()
            .flex_shrink_0()
            .gap_1()
            .px_3()
            .py_2()
            .rounded_lg()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(PANEL))
            .child(
                row()
                    .text_size(px(11.))
                    .text_color(rgb(MUTED))
                    .child(div().flex_1().text_color(rgb(BLUE)).child("Agent change"))
                    .child(if comment.status == "queued" {
                        "Queued"
                    } else {
                        "Sent"
                    })
                    .when(comment.status == "queued", |d| {
                        d.child(
                            button(SharedString::from(format!("remove-change-{id}")), "×")
                                .tooltip("Remove change request")
                                .disabled(self.submitting || self.repository_agent_running())
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.remove_queued_change(id.clone(), window, cx)
                                })),
                        )
                    }),
            )
            .child(div().text_size(px(13.)).child(comment.body.clone()))
            .child(self.source_reference(
                format!("comment-code-{}", comment.id),
                &comment.selection,
                &comment.digest,
                true,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn threads_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target = self.thread_scroll_target.take();
        let filter = self.thread_filter.clone();
        let mut threads: Vec<_> = self
            .state
            .threads
            .iter()
            .enumerate()
            .filter(|(_, thread)| {
                thread.archived == self.show_archived
                    && (!self.changes_only || thread.kind == "change")
                    && filter
                        .as_ref()
                        .is_none_or(|file| self.thread_matches_file(thread, file))
            })
            .collect();
        threads.sort_by(|(_, a), (_, b)| b.updated_at.cmp(&a.updated_at));
        let represented: HashSet<_> = self
            .state
            .threads
            .iter()
            .flat_map(|thread| &thread.comment_ids)
            .collect();
        let queued: Vec<_> = self
            .state
            .comments
            .iter()
            .enumerate()
            .filter(|(_, comment)| {
                !self.show_archived
                    && filter
                        .as_ref()
                        .is_none_or(|file| comment.selection.path == *file)
                    && !represented.contains(&comment.id)
            })
            .collect();
        let count = queued.len() + threads.len();
        let keys: Vec<_> = queued
            .iter()
            .map(|(_, comment)| (comment.id.clone(), false))
            .chain(threads.iter().map(|(_, thread)| {
                (
                    thread.id.clone(),
                    self.active_thread.as_ref() == Some(&thread.id),
                )
            }))
            .collect();
        if self
            .thread_items
            .iter()
            .map(|(id, _)| id)
            .ne(keys.iter().map(|(id, _)| id))
        {
            let top = self.thread_scroll.logical_scroll_top();
            let anchor = self.thread_items.get(top.item_ix).map(|(id, _)| id);
            self.thread_scroll.reset(count);
            self.thread_scroll.splice_focusable(
                0..count,
                keys.iter()
                    .map(|(_, active)| active.then(|| self.thread_reply.focus_handle(cx))),
            );
            self.thread_scroll
                .clone()
                .with_uniform_item_height(px(180.));
            // New answers can reorder the list. Keep the row being read in
            // place unless an explicit thread jump below chooses another one.
            if let Some(index) =
                anchor.and_then(|anchor| keys.iter().position(|(id, _)| id == anchor))
            {
                self.thread_scroll.scroll_to(ListOffset {
                    item_ix: index,
                    offset_in_item: top.offset_in_item,
                });
            }
        } else {
            for (index, ((_, before), (_, after))) in
                self.thread_items.iter().zip(&keys).enumerate()
            {
                if before != after {
                    self.thread_scroll.splice_focusable(
                        index..index + 1,
                        [after.then(|| self.thread_reply.focus_handle(cx))],
                    );
                }
            }
        }
        if let Some(target) = target
            && let Some(index) = keys.iter().position(|(id, _)| *id == target)
        {
            self.thread_scroll.scroll_to(ListOffset {
                item_ix: index,
                offset_in_item: px(0.),
            });
        }
        self.thread_items = keys;
        let entries: Vec<_> = queued
            .iter()
            .map(|(index, _)| (true, *index))
            .chain(threads.iter().map(|(index, _)| (false, *index)))
            .collect();
        let owner = cx.weak_entity();
        let list = if count == 0 {
            div()
                .p_3()
                .text_size(px(12.))
                .text_color(rgb(MUTED))
                .child(if self.changes_only {
                    "No change requests"
                } else {
                    "No threads yet"
                })
                .into_any_element()
        } else {
            list(self.thread_scroll.clone(), move |index, _, cx| {
                owner
                    .update(cx, |this, cx| {
                        let (queued, index) = entries[index];
                        let card = if queued {
                            this.queued_card(&this.state.comments[index], cx)
                        } else {
                            this.thread_card(&this.state.threads[index], cx)
                        };
                        div().px_2().pb_2().min_w_0().child(card).into_any_element()
                    })
                    .unwrap_or_else(|_| div().into_any_element())
            })
            .size_full()
            .into_any_element()
        };
        let pending = self
            .state
            .comments
            .iter()
            .filter(|comment| comment.status == "queued")
            .count();
        let available = self
            .providers
            .iter()
            .any(|provider| provider.id == self.provider && provider.available);
        let mut panel = column()
            .id("thread-sidebar")
            .w_full()
            .h_full()
            .flex_shrink_0()
            .min_h_0()
            .bg(rgb(PANEL))
            .border_l_1()
            .border_color(rgb(BORDER))
            .child(
                row()
                    .h(px(34.))
                    .flex_shrink_0()
                    .px_3()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::MEDIUM)
                            .child(format!("Threads  {}", self.thread_count(None))),
                    )
                    .child(
                        button("close-threads", "×")
                            .tooltip("Close threads (⌘⇧B)")
                            .on_click(
                                cx.listener(|this, _, window, cx| this.hide_threads(window, cx)),
                            ),
                    ),
            )
            .child(
                row()
                    .px_2()
                    .py_1()
                    .flex_shrink_0()
                    .child(
                        button("threads-all", "All")
                            .selected(!self.changes_only && !self.show_archived)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.changes_only = false;
                                this.show_archived = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        button("threads-changes", "Changes")
                            .selected(self.changes_only && !self.show_archived)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.changes_only = true;
                                this.show_archived = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        button("threads-archived", "Archived")
                            .selected(self.show_archived)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_archived = !this.show_archived;
                                this.changes_only = false;
                                cx.notify();
                            })),
                    ),
            )
            .when_some(filter, |d, path| {
                d.child(
                    row()
                        .px_3()
                        .pb_1()
                        .text_size(px(11.))
                        .text_color(rgb(MUTED))
                        .child(div().min_w_0().flex_1().truncate().child(path))
                        .child(
                            button("all-threads", "×")
                                .tooltip("Show all files")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.thread_filter = None;
                                    cx.notify();
                                })),
                        ),
                )
            })
            .child(
                div()
                    .id("review-threads")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .pt_2()
                    .overflow_hidden()
                    .child(list)
                    .child({
                        let owner = cx.weak_entity();
                        canvas(
                            move |_, _, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    let width = this.thread_scroll.viewport_bounds().size.width;
                                    if this.thread_hinted_width != Some(width) {
                                        // GPUI discards all height hints on the first layout
                                        // and width changes. Restore them after list prepaint
                                        // so scrolling can reach rows not measured yet.
                                        this.thread_hinted_width = Some(width);
                                        this.thread_scroll
                                            .clone()
                                            .with_uniform_item_height(px(180.));
                                        this.threads_pane.update(cx, |_, cx| cx.notify());
                                    }
                                });
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full()
                    }),
            );
        if pending > 0 {
            let mut providers = row().gap_1();
            for provider in &self.providers {
                let id = provider.id.clone();
                providers = providers.child(
                    button(
                        SharedString::from(format!("changes-provider-{id}")),
                        if id == "codex" { "Codex" } else { "Claude" },
                    )
                    .selected(self.provider == id)
                    .disabled(!provider.available || self.submitting)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.provider = id.clone();
                        cx.notify();
                    })),
                );
            }
            panel = panel.child(
                column()
                    .id("changes-footer")
                    .flex_shrink_0()
                    .p_2()
                    .gap_2()
                    .border_t_1()
                    .border_color(rgb(BORDER))
                    .child(providers)
                    .child(
                        Button::new("apply-changes")
                            .small()
                            .primary()
                            .w_full()
                            .label(format!(
                                "Apply all {pending} {}",
                                if pending == 1 { "change" } else { "changes" }
                            ))
                            .disabled(
                                !available
                                    || self.loading
                                    || self.patch_loading
                                    || self.submitting
                                    || self.repository_agent_running(),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.apply_queued_changes(window, cx)
                            })),
                    ),
            );
        }
        let _ = window;
        panel.into_any_element()
    }
}
