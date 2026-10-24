use super::view::{BLUE, BORDER, MUTED, PANEL, SELECTED, button, column, row};
use super::*;
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::text::TextView;

impl Patch {
    fn thread_matches_file(&self, thread: &ReviewThread, file: &str) -> bool {
        thread.selection.as_ref().is_some_and(|s| s.path == file)
            || self
                .state
                .comments
                .iter()
                .any(|c| c.selection.path == file && thread.comment_ids.contains(&c.id))
    }

    pub(super) fn thread_count(&self, file: Option<&str>) -> usize {
        self.state
            .threads
            .iter()
            .filter(|t| file.is_none_or(|f| self.thread_matches_file(t, f)))
            .count()
            + self
                .state
                .comments
                .iter()
                .filter(|c| {
                    file.is_none_or(|f| c.selection.path == f)
                        && !self
                            .state
                            .threads
                            .iter()
                            .any(|t| t.comment_ids.contains(&c.id))
                })
                .count()
    }

    pub(super) fn remember_thread(&mut self, scope: &str, run: &AgentRun, cx: &mut Context<Self>) {
        if self.review.as_ref().is_none_or(|r| r.scope_key != scope) {
            return;
        }
        if let Some(thread) = &run.thread {
            if let Some(saved) = self.state.threads.iter_mut().find(|t| t.id == thread.id) {
                *saved = thread.clone();
            } else {
                self.state.threads.push(thread.clone());
            }
            cx.notify();
        }
    }

    pub(super) fn show_threads(
        &mut self,
        file: Option<String>,
        id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_modal(window, cx);
        self.threads_visible = true;
        self.thread_filter = file;
        if let Some(id) = id {
            self.select_thread(id, window, cx);
        }
        cx.notify();
    }

    fn select_thread(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(previous) = &self.active_thread {
            self.drafts.insert(
                format!("thread-{previous}"),
                self.thread_reply.read(cx).value().to_string(),
            );
        }
        let draft = self
            .drafts
            .get(&format!("thread-{id}"))
            .cloned()
            .unwrap_or_default();
        self.thread_scroll_target = Some(id.clone());
        self.active_thread = Some(id);
        self.thread_reply
            .update(cx, |input, cx| input.set_value(draft, window, cx));
        cx.notify();
    }

    pub(super) fn jump_to_code(
        &mut self,
        selection: Selection,
        digest: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_modal(window, cx);
        self.threads_visible = false;
        if self.file.as_ref() == Some(&selection.path) && self.patch.digest == digest {
            let side = selection.diff_side();
            self.diff.update(cx, |diff, cx| {
                diff.scroll_to_line(
                    DiffLinePosition::new(selection.path, side, selection.start),
                    cx,
                )
            });
        } else {
            let path = selection.path.clone();
            self.thread_target = Some((selection, digest));
            self.load_file(path, window, cx);
        }
        cx.notify();
    }

    pub(super) fn reply_thread(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(review) = self.review.clone() else {
            return;
        };
        let Some(thread) = self
            .state
            .threads
            .iter()
            .find(|t| Some(&t.id) == self.active_thread.as_ref())
            .cloned()
        else {
            return;
        };
        let question = self.thread_reply.read(cx).value().trim().to_string();
        if question.is_empty()
            || self.submitting
            || thread.kind != "question"
            || thread.head_sha != review.head_sha
            || thread.merge_base != review.merge_base
        {
            return;
        }
        let provider = thread
            .turns
            .last()
            .map(|t| t.provider.clone())
            .unwrap_or_else(|| self.provider.clone());
        let params = json!({ "review": review.request, "provider": provider, "mode": "ask", "question": question,
            "threadId": thread.id, "selection": thread.selection, "selectionDigest": thread.digest });
        self.submitting = true;
        self.rpc(
            "run",
            params,
            window,
            cx,
            move |this, result, window, cx| {
                this.submitting = false;
                if let Some(value) = this.result(result) {
                    match decode::<AgentRun>(value) {
                        Ok(run) => {
                            let id = run.id.clone();
                            this.remember_thread(&review.scope_key, &run, cx);
                            this.runs.insert(review.scope_key.clone(), run);
                            this.run_requests
                                .insert(review.scope_key.clone(), review.request);
                            this.drafts.remove(&format!("thread-{}", thread.id));
                            if this.active_thread.as_ref() == Some(&thread.id) {
                                this.thread_reply
                                    .update(cx, |input, cx| input.set_value("", window, cx));
                            }
                            this.poll_run(review.scope_key, id, window, cx);
                        }
                        Err(error) => this.error = Some(error),
                    }
                }
            },
        );
    }

    pub(super) fn threads_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(review) = self.review.clone() else {
            return div().into_any_element();
        };
        let mut list = column().gap_2().w_full().max_w(px(920.)).mx_auto();
        let filter = self.thread_filter.clone();
        let target = self.thread_scroll_target.take();
        let mut index = usize::from(filter.is_some());
        if let Some(file) = &filter {
            list = list.child(
                row()
                    .pb_1()
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .child(div().min_w_0().flex_1().truncate().child(file.clone()))
                    .child(button("all-threads", "All threads").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.thread_filter = None;
                            cx.notify();
                        },
                    ))),
            );
        }
        let mut threads = self.state.threads.clone();
        threads.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        for thread in threads {
            if filter
                .as_ref()
                .is_some_and(|f| !self.thread_matches_file(&thread, f))
            {
                continue;
            }
            if target.as_ref() == Some(&thread.id) {
                self.thread_scroll.scroll_to_top_of_item(index);
            }
            index += 1;
            let active = self.active_thread.as_ref() == Some(&thread.id);
            let id = thread.id.clone();
            let stale =
                thread.head_sha != review.head_sha || thread.merge_base != review.merge_base;
            let running = self.runs.values().any(|r| {
                r.status == "running" && r.thread.as_ref().is_some_and(|t| t.id == thread.id)
            });
            let status = if running {
                "Running"
            } else {
                match thread.turns.last().map(|t| t.status.as_str()).unwrap_or("") {
                    "completed" => {
                        if thread.kind == "question" {
                            "Answered"
                        } else {
                            "Completed"
                        }
                    }
                    "failed" => "Failed",
                    "cancelled" => "Cancelled",
                    "running" => "Unfinished",
                    _ => "Saved",
                }
            };
            let kind = if thread.kind == "question" {
                "Question"
            } else {
                "Agent change"
            };
            let label = thread
                .selection
                .as_ref()
                .map(Selection::label)
                .unwrap_or_else(|| "Entire review".into());
            let mut card = column()
                .flex_shrink_0()
                .border_1()
                .border_color(rgb(if active { 0x46546b } else { BORDER }))
                .rounded_lg()
                .bg(rgb(PANEL))
                .overflow_hidden()
                .child(
                    div()
                        .id(SharedString::from(format!("thread-{}", thread.id)))
                        .test_support()
                        .cursor_pointer()
                        .px_3()
                        .py_2()
                        .hover(|s| s.bg(rgb(SELECTED).opacity(0.35)))
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
                                .gap_2()
                                .text_size(px(11.))
                                .text_color(rgb(MUTED))
                                .child(div().text_color(rgb(BLUE)).child(kind))
                                .child(div().min_w_0().flex_1().truncate().child(label))
                                .when(stale, |d| d.child("Earlier revision"))
                                .child(status)
                                .child(if active { "⌄" } else { "›" }),
                        )
                        .child(
                            div()
                                .mt_1()
                                .when(!active, |d| d.truncate())
                                .text_size(px(13.))
                                .child(thread.body.clone()),
                        ),
                );
            if active {
                let mut conversation = column().px_3().pb_3().gap_3();
                if let (Some(selection), Some(digest)) =
                    (thread.selection.clone(), thread.digest.clone())
                    && review.files.iter().any(|f| f.path == selection.path)
                {
                    conversation = conversation.child(
                        row().child(
                            button(
                                SharedString::from(format!("code-{}", thread.id)),
                                "View code ↗",
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.jump_to_code(selection.clone(), digest.clone(), window, cx)
                                },
                            )),
                        ),
                    );
                }
                for (index, turn) in thread.turns.iter().enumerate() {
                    let mut turn_view = column().gap_2();
                    if index > 0 {
                        turn_view = turn_view.child(
                            column()
                                .gap_1()
                                .pt_3()
                                .border_t_1()
                                .border_color(rgb(BORDER))
                                .child(div().text_size(px(11.)).text_color(rgb(MUTED)).child("You"))
                                .child(div().text_size(px(13.)).child(turn.question.clone())),
                        );
                    }
                    turn_view =
                        turn_view.child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(
                            if turn.provider == "claude" {
                                "Claude"
                            } else {
                                "Codex"
                            },
                        ));
                    let mut messages = 0;
                    for (event_index, event) in turn.events.iter().enumerate() {
                        if event.kind == "message" || event.kind == "error" {
                            messages += 1;
                            turn_view = turn_view.child(
                                div()
                                    .text_size(px(13.))
                                    .when(event.kind == "error", |d| d.text_color(rgb(0xee9292)))
                                    .child(TextView::markdown(
                                        SharedString::from(format!("{}-{event_index}", turn.id)),
                                        event.text.clone(),
                                    )),
                            );
                        }
                    }
                    if messages == 0 {
                        turn_view =
                            turn_view.child(div().text_size(px(12.)).text_color(rgb(MUTED)).child(
                                if running {
                                    "Working…"
                                } else {
                                    "No response was saved."
                                },
                            ));
                    }
                    conversation = conversation.child(turn_view);
                }
                if thread.kind == "question" && !stale && !running {
                    conversation = conversation.child(
                        column()
                            .key_context("PatchThread")
                            .gap_2()
                            .pt_2()
                            .on_action(cx.listener(|this, _: &ReplyThread, window, cx| {
                                this.reply_thread(window, cx)
                            }))
                            .child(Textarea::new(&self.thread_reply).w_full())
                            .child(
                                row()
                                    .justify_end()
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgb(MUTED))
                                            .child("⌘Enter"),
                                    )
                                    .child(
                                        Button::new("reply-thread")
                                            .small()
                                            .primary()
                                            .label("Ask follow-up")
                                            .disabled(self.submitting)
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.reply_thread(window, cx)
                                            })),
                                    ),
                            ),
                    );
                }
                card = card.child(conversation);
            }
            list = list.child(card);
        }
        for comment in self.state.comments.clone() {
            if filter
                .as_ref()
                .is_some_and(|f| &comment.selection.path != f)
                || self
                    .state
                    .threads
                    .iter()
                    .any(|t| t.comment_ids.contains(&comment.id))
            {
                continue;
            }
            let selection = comment.selection.clone();
            let digest = comment.digest.clone();
            if target.as_ref() == Some(&comment.id) {
                self.thread_scroll.scroll_to_top_of_item(index);
            }
            index += 1;
            list = list.child(
                column()
                    .flex_shrink_0()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .rounded_lg()
                    .bg(rgb(PANEL))
                    .child(
                        row()
                            .text_size(px(11.))
                            .text_color(rgb(MUTED))
                            .child(div().text_color(rgb(BLUE)).child("Agent change"))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .child(comment.selection.label()),
                            )
                            .child(if comment.status == "queued" {
                                "Queued"
                            } else {
                                "Sent"
                            }),
                    )
                    .child(div().text_size(px(13.)).child(comment.body))
                    .child(
                        row().child(
                            button(
                                SharedString::from(format!("comment-code-{}", comment.id)),
                                "View code ↗",
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.jump_to_code(selection.clone(), digest.clone(), window, cx)
                                },
                            )),
                        ),
                    ),
            );
        }
        if self.thread_count(filter.as_deref()) == 0 {
            list =
                list.child(
                    column()
                        .items_center()
                        .pt(px(80.))
                        .gap_2()
                        .child(
                            div()
                                .text_size(px(15.))
                                .font_weight(FontWeight::MEDIUM)
                                .child("No conversations yet"),
                        )
                        .child(button("start-thread", "Ask about this review").on_click(
                            cx.listener(|this, _, window, cx| {
                                this.open_modal(
                                    ModalKind::Ask,
                                    None,
                                    None,
                                    "ask-review",
                                    window,
                                    cx,
                                )
                            }),
                        )),
                );
        }
        let _ = window;
        list.id("review-threads")
            .test_support()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.thread_scroll)
            .p_3()
            .into_any_element()
    }
}
