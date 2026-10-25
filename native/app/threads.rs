use super::*;

impl Patch {
    pub(super) fn thread_matches_file(&self, thread: &ReviewThread, file: &str) -> bool {
        thread.selection.as_ref().is_some_and(|s| s.path == file)
            || thread
                .changes
                .iter()
                .any(|change| change.selection.path == file)
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
            if run.mode == "apply" && run.status == "completed" {
                for comment in &mut self.state.comments {
                    if thread.comment_ids.contains(&comment.id) {
                        comment.status = "sent".into();
                    }
                }
            }
            if let Some(saved) = self.state.threads.iter_mut().find(|t| t.id == thread.id) {
                *saved = thread.clone();
            } else {
                self.state.threads.push(thread.clone());
            }
            cx.notify();
        }
    }

    pub(super) fn repository_agent_running(&self) -> bool {
        self.review.as_ref().is_some_and(|review| {
            self.runs.iter().any(|(scope, run)| {
                run.status == "running"
                    && self
                        .run_requests
                        .get(scope)
                        .is_some_and(|request| request.repo == review.request.repo)
            })
        })
    }

    pub(super) fn track_thread_run(
        &mut self,
        scope: String,
        request: ReviewRequest,
        run: AgentRun,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = run.id.clone();
        let thread_id = run.thread.as_ref().map(|thread| thread.id.clone());
        let ask = run.mode == "ask";
        self.remember_thread(&scope, &run, cx);
        self.run_requests.insert(scope.clone(), request);
        self.runs.insert(scope.clone(), run);
        if self
            .review
            .as_ref()
            .is_some_and(|review| review.scope_key == scope)
        {
            self.hide_file_sheet(window, cx);
            self.threads_visible = true;
            self.thread_filter = None;
            if ask {
                self.changes_only = false;
            }
            if let Some(id) = thread_id {
                self.select_thread(id, window, cx);
            }
        }
        self.poll_run(scope, id, window, cx);
        cx.notify();
    }

    pub(super) fn apply_queued_changes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(review) = self.review.clone() else {
            return;
        };
        let ids: Vec<_> = self
            .state
            .comments
            .iter()
            .filter(|comment| comment.status == "queued")
            .map(|comment| comment.id.clone())
            .collect();
        if ids.is_empty()
            || self.loading
            || self.patch_loading
            || self.submitting
            || self.repository_agent_running()
        {
            return;
        }
        if !self
            .providers
            .iter()
            .any(|provider| provider.id == self.provider && provider.available)
        {
            self.error = Some("Select an available agent to apply changes.".into());
            cx.notify();
            return;
        }
        self.submitting = true;
        self.rpc("run", json!({ "review": review.request, "provider": self.provider, "mode": "apply", "commentIds": ids }), window, cx, move |this, result, window, cx| {
            this.submitting = false;
            if let Some(value) = this.result(result) {
                match decode(value) {
                    Ok(run) => this.track_thread_run(review.scope_key, review.request, run, window, cx),
                    Err(error) => this.error = Some(error),
                }
            }
        });
    }

    pub(super) fn remove_queued_change(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(review) = self.review.clone() else {
            return;
        };
        if self.submitting || self.repository_agent_running() {
            return;
        }
        self.submitting = true;
        self.rpc(
            "deleteComment",
            json!({ "review": review.request, "id": id }),
            window,
            cx,
            move |this, result, _, _| {
                this.submitting = false;
                if this
                    .review
                    .as_ref()
                    .is_some_and(|current| current.scope_key == review.scope_key)
                    && let Some(value) = this.result(result)
                {
                    match decode(value) {
                        Ok(state) => this.state = state,
                        Err(error) => this.error = Some(error),
                    }
                }
            },
        );
    }

    pub(super) fn toggle_threads(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.review.is_none() {
            return;
        }
        if self.modal.is_some() {
            self.close_modal(window, cx);
        }
        if self.threads_visible {
            self.hide_threads(window, cx);
        } else {
            self.hide_file_sheet(window, cx);
            self.threads_visible = true;
            cx.notify();
        }
    }

    pub(super) fn hide_threads(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.threads_visible = false;
        if self
            .thread_reply
            .focus_handle(cx)
            .contains_focused(window, cx)
        {
            self.focus.focus(window, cx);
        }
        cx.notify();
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
        self.changes_only = false;
        if let Some(id) = id {
            self.select_thread(id, window, cx);
        }
        cx.notify();
    }

    pub(super) fn select_thread(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        self.jump_file(selection.path.clone(), window, cx);
        if self
            .patches
            .get(&selection.path)
            .is_some_and(|patch| patch.digest == digest)
        {
            self.selection = Some(selection.clone());
            self.diff.update(cx, |diff, cx| {
                diff.set_selected_lines(
                    Some(DiffLineRange::new(
                        selection.path.clone(),
                        selection.diff_side(),
                        selection.start,
                        selection.end,
                    )),
                    cx,
                );
                diff.scroll_to_line(
                    DiffLinePosition::new(
                        selection.path.clone(),
                        selection.diff_side(),
                        selection.start,
                    ),
                    cx,
                )
            });
        } else {
            self.error = Some("This conversation refers to an earlier version of the file.".into());
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
            || self.loading
            || self.patch_loading
            || self.submitting
            || self.repository_agent_running()
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
                            this.track_thread_run(
                                review.scope_key,
                                review.request,
                                run,
                                window,
                                cx,
                            );
                            this.drafts.remove(&format!("thread-{}", thread.id));
                            if this.active_thread.as_ref() == Some(&thread.id) {
                                this.thread_reply
                                    .update(cx, |input, cx| input.set_value("", window, cx));
                            }
                        }
                        Err(error) => this.error = Some(error),
                    }
                }
            },
        );
    }
}
