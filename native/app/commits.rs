use super::controls::*;
use super::*;
use crate::theme::*;
use gpui_kit::{
    assets::IconName,
    base::{Button as BaseButton, Popover},
    component::Icon,
};

const MENU_WIDTH: f32 = 420.;
const MAX_PILL_WIDTH: f32 = 280.;
const MENU_VISIBLE_ROWS: usize = 8;
const MENU_ROW_HEIGHT: f32 = CHOICE_ROW_HEIGHT + 2.;

#[derive(Default)]
pub(super) struct CommitPicker {
    pub open: bool,
    pub commits: Vec<Commit>,
    pub has_more: bool,
    pub loading: bool,
    pub range: Option<CommitRange>,
    anchor: Option<usize>,
    head: Option<String>,
    /// The repo, base and head the loaded commits were listed for, so reopening the
    /// menu on the same branch reuses them instead of loading again.
    source: Option<(String, Option<String>, String)>,
    serial: u64,
    /// Shared by the open menu and its exiting copy, so the copy keeps the
    /// list's scroll position while it fades out.
    scroll: UniformListScrollHandle,
}

impl CommitPicker {
    /// The menu stays closed until its first page arrives, so it opens at full height
    /// rather than growing when the list loads in.
    pub fn ready(&self) -> bool {
        self.open && (!self.loading || !self.commits.is_empty())
    }

    pub fn selected(&self, index: usize) -> bool {
        let Some(range) = &self.range else {
            return false;
        };
        let Some(newest) = self.commits.iter().position(|c| c.sha == range.end) else {
            return false;
        };
        let oldest = self
            .commits
            .iter()
            .position(|c| c.sha == range.start)
            .unwrap_or(self.commits.len());
        index >= newest && index <= oldest
    }

    fn choose(&mut self, index: usize, extend: bool) {
        if index >= self.commits.len() {
            return;
        }
        let anchor = if extend {
            self.anchor.unwrap_or(index)
        } else {
            index
        };
        let newest = anchor.min(index);
        let oldest = anchor.max(index);
        self.anchor = Some(anchor);
        self.range = Some(CommitRange {
            start: self.commits[oldest].sha.clone(),
            end: self.commits[newest].sha.clone(),
        });
    }

    /// The trigger label, following GitHub's "All commits" / commit / "N commits" wording.
    fn label(review: &Review) -> SharedString {
        match &review.request.commit_range {
            None => "All commits".into(),
            Some(range) if range.start == range.end => {
                // The review carries its own head commit, so the label does not
                // depend on the dropdown's list, which reloads whenever it opens.
                let short = short_sha(&range.end);
                match review.head_commit.as_ref().filter(|c| c.sha == range.end) {
                    Some(commit) => format!("{short} {}", commit.subject).into(),
                    None => format!("Commit {short}").into(),
                }
            }
            Some(_) if review.commit_count == 1 => "1 commit".into(),
            Some(_) => format!("{} commits", review.commit_count).into(),
        }
    }
}

fn short_sha(sha: &str) -> &str {
    sha.get(..7).unwrap_or(sha)
}

impl Rev {
    pub(super) fn open_commit_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(review) = self.review.as_ref() else {
            return;
        };
        if self.loading || self.patch_loading || self.submitting || self.folder_picker_open {
            cx.notify();
            return;
        }
        let source = (
            review.request.repo.clone(),
            review.request.base.clone(),
            review.branch_head_sha.clone(),
        );
        let picker = &mut self.commit_picker;
        let reuse = picker.source.as_ref() == Some(&source) && !picker.commits.is_empty();
        picker.open = true;
        picker.serial += 1;
        picker.loading = false;
        picker.range = review.request.commit_range.clone();
        picker.head = Some(review.branch_head_sha.clone());
        picker.anchor = picker.range.as_ref().and_then(|range| {
            picker.commits.iter().position(|c| c.sha == range.end)
        });
        if reuse {
            cx.notify();
            return;
        }
        picker.source = Some(source);
        picker.commits.clear();
        picker.has_more = false;
        picker.anchor = None;
        self.load_more_commits(window, cx);
    }

    pub(super) fn close_commit_picker(&mut self, cx: &mut Context<Self>) {
        if self.commit_picker.open {
            self.commit_picker.open = false;
            self.commit_picker.serial += 1;
            self.commit_picker.loading = false;
            cx.notify();
        }
    }

    pub(super) fn load_more_commits(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(review) = self.review.as_ref() else {
            return;
        };
        if !self.commit_picker.open || self.commit_picker.loading {
            return;
        }
        let request = ReviewRequest {
            head: self.commit_picker.head.clone(),
            ..review.request.clone()
        };
        let serial = self.commit_picker.serial;
        self.commit_picker.loading = true;
        self.rpc(
            "commits",
            json!({ "review": request, "offset": self.commit_picker.commits.len() }),
            window,
            cx,
            move |this, result, _, _| {
                let picker = &mut this.commit_picker;
                if !picker.open || picker.serial != serial {
                    return;
                }
                picker.loading = false;
                match result.and_then(decode::<CommitPage>) {
                    Ok(page) => {
                        picker.has_more = page.has_more;
                        picker.commits.extend(page.commits);
                        if picker.anchor.is_none()
                            && let Some(range) = &picker.range
                        {
                            picker.anchor = picker.commits.iter().position(|c| c.sha == range.end);
                        }
                    }
                    Err(error) => {
                        this.error = Some(error);
                        // Give up quietly rather than opening an empty menu.
                        if picker.commits.is_empty() {
                            picker.open = false;
                            picker.source = None;
                        }
                    }
                }
            },
        );
        cx.notify();
    }

    fn choose_commit(
        &mut self,
        index: usize,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_picker.choose(index, extend);
        self.apply_commit_selection(window, cx);
    }

    fn choose_all_commits(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_picker.range = None;
        self.commit_picker.anchor = None;
        self.apply_commit_selection(window, cx);
    }

    fn apply_commit_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_commit_picker(cx);
        if self.loading || self.patch_loading {
            return;
        }
        let Some(review) = self.review.as_ref() else {
            return;
        };
        if review.request.commit_range == self.commit_picker.range {
            return;
        }
        let range = self.commit_picker.range.clone();
        let in_checkout_tab = self
            .workspace
            .active_tab()
            .is_some_and(|tab| tab.kind == crate::workspace_state::TabKind::Current);
        let request = if in_checkout_tab {
            // Stay in the working tree tab; "All commits" returns to the live working tree.
            ReviewRequest::working_tree(review.request.repo.clone()).checkout_commits(range)
        } else {
            ReviewRequest {
                mode: Some("branch".into()),
                commit_range: range,
                ..review.request.clone()
            }
        };
        self.load_review(request, self.file.clone(), window, cx);
    }
}

fn commit_selection_mark(selected: bool) -> Div {
    div().size(px(14.)).flex_shrink_0().when(selected, |d| {
        d.child(
            Icon::new(IconName::Check)
                .size(px(14.))
                .text_color(rgb(TEXT)),
        )
    })
}

impl Rev {
    /// The header pill showing which commits are in the diff; it opens the commit dropdown.
    pub(super) fn commit_picker_pill(
        &self,
        review: &Review,
        disabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = CommitPicker::label(review);
        // Requested opens highlight the pill straight away; the menu itself waits
        // for its commits.
        let requested = self.commit_picker.open;
        let open = self.commit_picker.ready();
        let this = cx.entity().downgrade();
        let trigger = BaseButton::new("commit-picker-trigger")
            .debug_selector(|| "commit-picker-pill".into())
            .h(px(HEADER_PILL_HEIGHT))
            .max_w(px(MAX_PILL_WIDTH))
            .min_w_0()
            .px(px(HEADER_PILL_PADDING))
            .gap(px(6.))
            .rounded(px(CONTROL_RADIUS))
            .bg(rgb(SURFACE))
            .border_1()
            .border_color(rgb(CONTROL_BORDER))
            .text_size(px(12.))
            .line_height(px(HEADER_PILL_LINE_HEIGHT))
            .text_color(rgb(TEXT))
            .when(!disabled, |d| {
                d.hover(|d| d.bg(rgb(HOVER)).border_color(rgb(SELECTED_BORDER)))
            })
            .styles(|styles| {
                styles
                    .selected(|d| d.bg(rgb(HOVER)).border_color(rgb(SELECTED_BORDER)))
                    .disabled(|d| d.opacity(0.5))
            })
            .focus_visible(|d| d.border_color(rgb(FOCUS)))
            .disabled(disabled)
            .selected(requested)
            .aria_expanded(open)
            .accessibility_label(format!("Commits: {label}"))
            .child(div().flex_1().min_w_0().truncate().child(label))
            .child(
                Icon::new(IconName::ChevronDown)
                    .size(px(12.))
                    .flex_shrink_0()
                    .text_color(rgb(MUTED)),
            );

        let popover = AnimatedPopover::new("commit-picker", open, window, cx);
        let menu = popover.visible().then(|| self.commit_menu(review, cx));
        let commit_picker = Popover::new("commit-picker")
            .flex_shrink_0()
            .min_w_0()
            .on_open_change(move |open, window, cx| {
                let open = *open;
                _ = this.update(cx, |this, cx| {
                    // Clicking while the commits are still loading cancels the open.
                    if open && !this.commit_picker.open {
                        this.open_commit_picker(window, cx);
                    } else {
                        this.close_commit_picker(cx);
                    }
                });
            })
            .trigger(trigger);
        popover.render(commit_picker, menu)
    }

    fn commit_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let commit = &self.commit_picker.commits[index];
        let selected = self.commit_picker.selected(index);
        let label = format!("{} {}", short_sha(&commit.sha), commit.subject);
        div()
            .h(px(MENU_ROW_HEIGHT))
            .pb(px(2.))
            .child(
                choice_button(("commit", index), selected)
                    .accessibility_label(label)
                    .child(commit_selection_mark(selected))
                    .child(
                        column()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(12.))
                                    .line_height(px(16.))
                                    .child(commit.subject.clone()),
                            )
                            .child(
                                row()
                                    .gap(px(UI_SPACING))
                                    .text_size(px(11.))
                                    .line_height(px(14.))
                                    .text_color(rgb(MUTED))
                                    .child(
                                        div()
                                            .font_family("Menlo")
                                            .child(short_sha(&commit.sha).to_owned()),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .child(commit.author.clone()),
                                    )
                                    .child(
                                        commit.date.get(..10).unwrap_or(&commit.date).to_owned(),
                                    ),
                            ),
                    )
                    .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                        this.choose_commit(index, event.modifiers().shift, window, cx)
                    })),
            )
            .into_any_element()
    }

    fn commit_menu(&self, review: &Review, cx: &mut Context<Self>) -> AnyElement {
        let picker = &self.commit_picker;
        let all = picker.range.is_none();
        let total = (all && review.request.mode.as_deref() != Some("working"))
            .then_some(review.commit_count)
            .filter(|count| *count > 0);
        column()
            .id("commit-menu")
            .debug_selector(|| "commit-menu".into())
            .w(px(MENU_WIDTH))
            .p(px(UI_SPACING))
            .gap(px(UI_SPACING))
            .bg(rgb(SURFACE))
            .border_1()
            .border_color(rgb(CONTROL_BORDER))
            .rounded(px(8.))
            .shadow_lg()
            .text_color(rgb(TEXT))
            .child(
                choice_button("all-commits", all)
                    .h(px(CONTROL_HEIGHT))
                    .accessibility_label("All commits — full branch diff")
                    .child(commit_selection_mark(all))
                    .child(div().flex_1().child("All commits"))
                    .when_some(total, |d, total| {
                        d.child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(MUTED))
                                .child(if total == 1 {
                                    "1 commit".to_owned()
                                } else {
                                    format!("{total} commits")
                                }),
                        )
                    })
                    .on_click(
                        cx.listener(|this, _, window, cx| this.choose_all_commits(window, cx)),
                    ),
            )
            .child(div().h(px(1.)).flex_shrink_0().bg(rgb(BORDER)))
            .when(!picker.commits.is_empty(), |body| {
                body.child(
                    uniform_list(
                        "commit-list",
                        picker.commits.len(),
                        cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                            range
                                .map(|index| this.commit_row(index, cx))
                                .collect::<Vec<_>>()
                        }),
                    )
                    .track_scroll(&picker.scroll)
                    .h(px(
                        picker.commits.len().min(MENU_VISIBLE_ROWS) as f32 * MENU_ROW_HEIGHT
                    ))
                    .w_full(),
                )
            })
            .when(picker.loading, |body| {
                body.child(
                    div()
                        .px(px(CONTROL_PADDING))
                        .py(px(UI_SPACING))
                        .text_size(px(12.))
                        .text_color(rgb(MUTED))
                        .child("Loading commits…"),
                )
            })
            .when(
                !picker.loading && !picker.has_more && picker.commits.is_empty(),
                |body| {
                    body.child(
                        div()
                            .px(px(CONTROL_PADDING))
                            .py(px(UI_SPACING))
                            .text_size(px(12.))
                            .text_color(rgb(MUTED))
                            .child("No commits on this branch."),
                    )
                },
            )
            .when(picker.has_more && !picker.loading, |body| {
                body.child(
                    button("older-commits", "Load older commits")
                        .w_full()
                        .on_click(
                            cx.listener(|this, _, window, cx| this.load_more_commits(window, cx)),
                        ),
                )
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::CommitPicker;
    use crate::model::{Commit, CommitRange, HeadCommit, Review, ReviewRequest};

    fn commit(sha: &str, subject: &str) -> Commit {
        Commit {
            sha: sha.into(),
            subject: subject.into(),
            author: "a".into(),
            date: "2026-11-01".into(),
        }
    }

    fn review(range: Option<(&str, &str)>, count: usize) -> Review {
        Review {
            request: ReviewRequest {
                commit_range: range.map(|(start, end)| CommitRange {
                    start: start.into(),
                    end: end.into(),
                }),
                ..Default::default()
            },
            commit_count: count,
            ..Default::default()
        }
    }

    #[test]
    fn pill_label_describes_the_selected_commits() {
        assert_eq!(CommitPicker::label(&review(None, 5)), "All commits");
        assert_eq!(CommitPicker::label(&review(Some(("a", "c")), 3)), "3 commits");
        let mut single = review(Some(("abcdef123", "abcdef123")), 1);
        assert_eq!(CommitPicker::label(&single), "Commit abcdef1");
        single.head_commit = Some(HeadCommit {
            sha: "abcdef123".into(),
            subject: "Fix the thing".into(),
        });
        assert_eq!(CommitPicker::label(&single), "abcdef1 Fix the thing");
    }

    #[test]
    fn shift_click_extends_from_the_last_chosen_commit() {
        let mut picker = CommitPicker {
            commits: vec![commit("c", "3"), commit("b", "2"), commit("a", "1")],
            ..Default::default()
        };
        picker.choose(1, false);
        assert_eq!(
            picker.range,
            Some(CommitRange {
                start: "b".into(),
                end: "b".into()
            })
        );
        picker.choose(0, true);
        assert_eq!(
            picker.range,
            Some(CommitRange {
                start: "b".into(),
                end: "c".into()
            })
        );
        assert!(picker.selected(0) && picker.selected(1) && !picker.selected(2));
    }
}
