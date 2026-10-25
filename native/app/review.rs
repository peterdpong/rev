use super::view::{BG, BLUE, BORDER, MUTED, PANEL, SELECTED, button, column, row};
use super::*;
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::{IconName, TitleBar, text::TextView};

impl Patch {
    pub(super) fn top_nav(
        &mut self,
        sidebar: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let navigation_width = px(112. + (motion::SIDEBAR_WIDTH - 112.) * sidebar);
        let leading = row()
            .id("navigation-chrome")
            .test_support()
            .w(navigation_width)
            .gap_0()
            .h_full()
            .flex_shrink_0()
            .pl(px(80.))
            .pr_2()
            .border_r(px(sidebar))
            .border_color(rgb(BORDER).opacity(sidebar))
            .child(
                div()
                    .id("navigation-title")
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .opacity(((sidebar - 0.4) / 0.6).clamp(0., 1.))
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Patch"),
            )
            .child(self.sidebar_toggle(window, cx));
        let mut main = row()
            .w((window.viewport_size().width - navigation_width).max(px(0.)))
            .flex_shrink_0()
            .min_w_0()
            .px_3()
            .gap_2()
            .text_size(px(12.))
            .child(
                row()
                    .gap_0()
                    .child(
                        Button::new("navigate-back")
                            .small()
                            .ghost()
                            .icon(IconName::ChevronLeft)
                            .w(px(24.))
                            .tooltip("Back (⌘[)")
                            .accessibility_label("Back")
                            .disabled(!self.can_navigate(NavigationDirection::Back))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.navigate_history(NavigationDirection::Back, window, cx)
                            })),
                    )
                    .child(
                        Button::new("navigate-forward")
                            .small()
                            .ghost()
                            .icon(IconName::ChevronRight)
                            .w(px(24.))
                            .tooltip("Forward (⌘])")
                            .accessibility_label("Forward")
                            .disabled(!self.can_navigate(NavigationDirection::Forward))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.navigate_history(NavigationDirection::Forward, window, cx)
                            })),
                    ),
            );
        if let Some(review) = self.review.clone() {
            let label = review
                .pull_request
                .as_ref()
                .map(|pr| format!("#{}", pr.number))
                .unwrap_or_else(|| review.request.head.clone().unwrap_or_else(|| "HEAD".into()));
            main = main
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .child(review.name.clone()),
                )
                .child(div().text_color(rgb(MUTED)).child("/"))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(MUTED))
                        .child(label),
                );
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
            if let Some(scope) = run_scope {
                let label = if self
                    .runs
                    .get(&scope)
                    .is_some_and(|run| run.status == "running")
                {
                    "Agent running…"
                } else {
                    "Agent response"
                };
                main = main.child(self.popover_button(
                    "agent-output",
                    button("agent-output", label).on_click(cx.listener(
                        move |this, _, window, cx| {
                            let thread = this
                                .runs
                                .get(&scope)
                                .and_then(|run| run.thread.as_ref())
                                .map(|thread| thread.id.clone());
                            if this
                                .review
                                .as_ref()
                                .is_some_and(|review| review.scope_key == scope)
                                && thread.is_some()
                            {
                                this.show_threads(None, thread, window, cx);
                            } else {
                                this.open_run(scope.clone(), window, cx);
                            }
                        },
                    )),
                    Anchor::TopRight,
                    window,
                    cx,
                ));
            }
            main = main.child(
                self.popover_button(
                    "ask-review",
                    Button::new("ask-review")
                        .small()
                        .label("Ask agent")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_modal(ModalKind::Ask, None, None, "ask-review", window, cx)
                        })),
                    Anchor::TopRight,
                    window,
                    cx,
                ),
            );
        } else {
            main = main.child(div().text_color(rgb(MUTED)).child("Review"));
        }
        TitleBar::new()
            .h(px(44.))
            .pl_0()
            .pr_0()
            .bg(rgb(PANEL))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(row().gap_0().w_full().h_full().child(leading).child(main))
            .into_any_element()
    }

    pub(super) fn toolbar(
        &mut self,
        sidebar: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(review) = self.review.clone() else {
            return div().into_any_element();
        };
        let pr = review.pull_request.as_ref();
        let commit = review.head_commit.as_ref();
        let working = review.request.mode.as_deref() == Some("working");
        let title = if working {
            "Uncommitted changes".to_string()
        } else {
            pr.map(|p| p.title.clone())
                .or_else(|| commit.map(|c| c.subject.clone()))
                .unwrap_or_else(|| review.name.clone())
        };
        let description = if working {
            ""
        } else {
            pr.map(|p| p.body.as_str())
                .or_else(|| commit.map(|c| c.body.as_str()))
                .unwrap_or("")
        };
        let excerpt = description
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("")
            .trim()
            .trim_start_matches('#')
            .trim()
            .to_string();
        let (status, color) = if let Some(pr) = pr {
            if pr.is_draft {
                ("Draft", MUTED)
            } else {
                match pr.state.as_str() {
                    "MERGED" => ("Merged", 0xbd9afa),
                    "CLOSED" => ("Closed", 0xee9292),
                    "OPEN" => ("Open", 0x8fd3a5),
                    _ => ("Pull request", MUTED),
                }
            }
        } else if working {
            ("Working tree", BLUE)
        } else if review.request.commit_range.is_some() {
            ("Commits", BLUE)
        } else {
            ("Branch", MUTED)
        };
        let source = pr
            .map(|p| p.head_ref_name.clone())
            .or_else(|| review.request.head.clone())
            .unwrap_or_default();
        let base = pr
            .map(|p| p.base_ref_name.clone())
            .or_else(|| review.request.base.clone())
            .unwrap_or_default();
        let author = pr
            .map(|p| p.author.as_str())
            .or_else(|| commit.map(|c| c.author.as_str()))
            .unwrap_or("");
        let added: usize = review.files.iter().map(|f| f.added).sum();
        let removed: usize = review.files.iter().map(|f| f.removed).sum();
        let mut summary = column()
            .gap_1()
            .px_4()
            .py_2()
            .flex_shrink_0()
            .bg(rgb(BG))
            .child(
                row()
                    .gap_2()
                    .min_w_0()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(18.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .px_2()
                            .py(px(2.))
                            .rounded_md()
                            .bg(rgb(color).opacity(0.10))
                            .text_color(rgb(color))
                            .text_size(px(11.))
                            .child(status),
                    )
                    .child(self.popover_button(
                        "review-details",
                        button("review-details", "Details").on_click(cx.listener(
                            |this, _, window, cx| {
                                this.open_modal(
                                    ModalKind::Details,
                                    None,
                                    None,
                                    "review-details",
                                    window,
                                    cx,
                                )
                            },
                        )),
                        Anchor::TopRight,
                        window,
                        cx,
                    )),
            )
            .when(!excerpt.is_empty(), |d| {
                d.child(
                    div()
                        .truncate()
                        .text_size(px(12.))
                        .text_color(rgb(MUTED))
                        .child(excerpt),
                )
            })
            .child(
                row()
                    .min_w_0()
                    .gap_2()
                    .text_size(px(11.))
                    .text_color(rgb(MUTED))
                    .child(
                        div()
                            .max_w(px(320.))
                            .min_w_0()
                            .truncate()
                            .child(format!("{source} → {base}")),
                    )
                    .child("·")
                    .child(div().max_w(px(180.)).truncate().child(author.to_owned()))
                    .child(div().flex_1())
                    .child(div().text_color(rgb(0x8fd3a5)).child(format!("+{added}")))
                    .child(div().text_color(rgb(0xe39494)).child(format!("−{removed}"))),
            );
        if (pr.is_some() || working)
            && let Some(commit) = commit
        {
            summary = summary.child(
                row()
                    .min_w_0()
                    .gap_2()
                    .text_size(px(11.))
                    .text_color(rgb(MUTED))
                    .child(
                        div()
                            .font_family("Menlo")
                            .text_color(rgb(BLUE))
                            .child(commit.sha.chars().take(8).collect::<String>()),
                    )
                    .child(div().min_w_0().truncate().child(commit.subject.clone())),
            );
        }
        let mut request = self
            .navigation_request()
            .unwrap_or_else(|| review.request.clone());
        request.mode = Some(if working { "branch" } else { "working" }.into());
        request.commit_range = None;
        let count = self.thread_count(None);
        let busy = self.loading || self.submitting;
        let commits = if review.request.commit_range.is_some() {
            format!("{} selected ▾", review.commit_count)
        } else {
            format!(
                "{} {} ▾",
                review.commit_count,
                if review.commit_count == 1 {
                    "commit"
                } else {
                    "commits"
                }
            )
        };
        let controls = row()
            .h(px(36.))
            .px_3()
            .gap_1()
            .border_b_1()
            .border_t_1()
            .border_color(rgb(BORDER))
            .flex_shrink_0()
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .child(format!("{} files", review.files.len())),
            )
            .child(
                button("toggle-threads", format!("Threads  {count}"))
                    .selected(self.threads_visible)
                    .tooltip(if self.threads_visible {
                        "Hide threads (⌘⇧B)"
                    } else {
                        "Show threads (⌘⇧B)"
                    })
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_threads(window, cx))),
            )
            .child(div().flex_1())
            .child(self.popover_button(
                "choose-commits",
                button("choose-commits", commits).disabled(busy).on_click(
                    cx.listener(|this, _, window, cx| this.open_commit_picker(window, cx)),
                ),
                Anchor::TopRight,
                window,
                cx,
            ))
            .child(
                button(
                    "scope",
                    if review.dirty && !working {
                        "Working tree ·"
                    } else {
                        "Working tree"
                    },
                )
                .when(working, |b| b.bg(rgb(SELECTED)))
                .disabled(busy)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.load_review(request.clone(), this.file.clone(), window, cx)
                })),
            )
            .child(
                button("fetch", "↓")
                    .tooltip("Fetch updates")
                    .accessibility_label("Fetch updates")
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, window, cx| this.fetch(window, cx))),
            )
            .child(
                button("refresh", "↻")
                    .tooltip("Refresh review (⌘R)")
                    .accessibility_label("Refresh review")
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, window, cx| this.refresh(window, cx))),
            );
        column()
            .w((window.viewport_size().width - px(motion::SIDEBAR_WIDTH * sidebar)).max(px(0.)))
            .flex_shrink_0()
            .child(summary)
            .child(controls)
            .into_any_element()
    }

    pub(super) fn review_details(&self) -> AnyElement {
        let Some(review) = &self.review else {
            return div().into_any_element();
        };
        let mut content = column().gap_3().text_size(px(12.));
        if let Some(pr) = &review.pull_request {
            content = content
                .child(
                    div()
                        .text_size(px(15.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(pr.title.clone()),
                )
                .child(div().text_color(rgb(MUTED)).child(format!(
                    "#{} · {} · {}",
                    pr.number,
                    pr.author,
                    pr.state.to_lowercase()
                )))
                .when(!pr.body.is_empty(), |d| {
                    d.child(TextView::markdown("pr-description", pr.body.clone()))
                })
                .child(button("open-pr-browser", "Open on GitHub ↗").on_click({
                    let url = pr.url.clone();
                    move |_, _, cx| cx.open_url(&url)
                }));
        }
        if let Some(commit) = &review.head_commit {
            content = content.child(
                column()
                    .gap_2()
                    .when(review.pull_request.is_some(), |d| {
                        d.pt_3().border_t_1().border_color(rgb(BORDER))
                    })
                    .child(div().text_color(rgb(MUTED)).text_size(px(11.)).child(
                        if review.request.commit_range.is_some() {
                            "Selected commit"
                        } else {
                            "Latest commit"
                        },
                    ))
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child(commit.subject.clone()),
                    )
                    .child(div().text_color(rgb(MUTED)).child(format!(
                        "{} · {} · {}",
                        commit.sha.chars().take(8).collect::<String>(),
                        commit.author,
                        commit.date.get(..10).unwrap_or(&commit.date)
                    )))
                    .when(!commit.body.is_empty(), |d| {
                        d.child(TextView::markdown(
                            "commit-description",
                            commit.body.clone(),
                        ))
                    }),
            );
        }
        content.into_any_element()
    }
}
