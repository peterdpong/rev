use super::controls::*;
use super::*;
use crate::theme::*;
use gpui_kit::{
    assets::IconName,
    base::{Button as BaseButton, Popover, motion},
    component::{
        Icon, Sizable,
        input::{Input, InputEvent, InputState},
    },
};

const MENU_WIDTH: f32 = 320.;
const MENU_VISIBLE_ROWS: usize = 10;
const MENU_ROW_HEIGHT: f32 = CONTROL_HEIGHT + 2.;
/// Matches the PR description card, so one fades out as the other fades in.
const MENU_REVEAL: std::time::Duration = std::time::Duration::from_millis(110);

#[derive(Default)]
pub(super) struct BranchPicker {
    pub open: bool,
    pub branches: Vec<String>,
    pub loading: bool,
    pub(super) query: Option<Entity<InputState>>,
    /// The repository the loaded branches belong to, so reopening the menu shows
    /// them at once while a fresh list loads.
    repo: Option<String>,
    serial: u64,
    _query_events: Option<Subscription>,
}

impl BranchPicker {
    /// Like the commit menu, this stays closed until it has branches to show.
    pub fn ready(&self) -> bool {
        self.open && (!self.loading || !self.branches.is_empty())
    }

    /// Branches matching the search, case-insensitively, with the current one first.
    fn visible(&self, current: &str, cx: &App) -> Vec<String> {
        let query = self
            .query
            .as_ref()
            .map(|query| query.read(cx).value().trim().to_lowercase())
            .unwrap_or_default();
        let mut visible: Vec<String> = self
            .branches
            .iter()
            .filter(|branch| branch.to_lowercase().contains(&query))
            .cloned()
            .collect();
        if let Some(index) = visible.iter().position(|branch| branch == current) {
            let branch = visible.remove(index);
            visible.insert(0, branch);
        }
        visible
    }
}

impl Rev {
    pub(super) fn open_branch_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(review) = self.review.as_ref() else {
            return;
        };
        if self.loading || self.submitting || self.folder_picker_open {
            cx.notify();
            return;
        }
        let repo = review.request.repo.clone();
        match self.branch_picker.query.clone() {
            Some(query) => query.update(cx, |query, cx| query.set_value("", window, cx)),
            None => {
                let query =
                    cx.new(|cx| InputState::new(window, cx).placeholder("Find a branch…"));
                self.branch_picker._query_events = Some(cx.subscribe_in(
                    &query,
                    window,
                    |this, _, event: &InputEvent, window, cx| match event {
                        InputEvent::Change => cx.notify(),
                        InputEvent::PressEnter { .. } => this.choose_first_branch(window, cx),
                        _ => {}
                    },
                ));
                self.branch_picker.query = Some(query);
            }
        }

        let picker = &mut self.branch_picker;
        if picker.repo.as_ref() != Some(&repo) {
            picker.branches.clear();
            picker.repo = Some(repo.clone());
        }
        picker.open = true;
        picker.serial += 1;
        picker.loading = true;
        let serial = picker.serial;
        self.rpc(
            "branches",
            json!({ "repo": repo, "localOnly": true }),
            window,
            cx,
            move |this, result, _, _| {
                let picker = &mut this.branch_picker;
                if !picker.open || picker.serial != serial {
                    return;
                }
                picker.loading = false;
                match result.and_then(decode::<Vec<String>>) {
                    Ok(branches) => picker.branches = branches,
                    Err(error) => {
                        this.error = Some(error);
                        // Give up quietly rather than opening an empty menu.
                        if picker.branches.is_empty() {
                            picker.open = false;
                            picker.repo = None;
                        }
                    }
                }
            },
        );
        cx.notify();
    }

    pub(super) fn close_branch_picker(&mut self, cx: &mut Context<Self>) {
        if self.branch_picker.open {
            self.branch_picker.open = false;
            self.branch_picker.serial += 1;
            self.branch_picker.loading = false;
            cx.notify();
        }
    }

    fn choose_branch(&mut self, branch: String, window: &mut Window, cx: &mut Context<Self>) {
        self.close_branch_picker(cx);
        let Some(review) = self.review.as_ref() else {
            return;
        };
        if self.loading {
            return;
        }
        let request = ReviewRequest::local_branch(review.request.repo.clone(), &branch);
        self.load_review(request, None, window, cx);
    }

    /// Enter in the search field opens the best match, the first listed branch.
    fn choose_first_branch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(review) = self.review.as_ref() else {
            return;
        };
        if !self.branch_picker.ready() {
            return;
        }
        let first = self
            .branch_picker
            .visible(&review.branch_label(), cx)
            .into_iter()
            .next();
        if let Some(branch) = first {
            self.choose_branch(branch, window, cx);
        }
    }

    /// Wraps the header's title pill so clicking it opens the branch dropdown.
    pub(super) fn branch_picker(
        &self,
        pill: AnyElement,
        review: &Review,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let requested = self.branch_picker.open;
        let open = self.branch_picker.ready();
        // Fade and drop in like the PR description card, which fades out as this opens.
        let reveal = motion::transition(
            "branch-menu-reveal",
            if open { 1.0_f32 } else { 0.0_f32 },
            motion::Transition::new(MENU_REVEAL).ease(ease_out_quint()),
            window,
            cx,
        );
        let this = cx.entity().downgrade();
        let trigger = BaseButton::new("branch-picker-trigger")
            .debug_selector(|| "branch-picker-trigger".into())
            .min_w_0()
            .max_w_full()
            .justify_start()
            .rounded(px(CONTROL_RADIUS))
            .selected(requested)
            .aria_expanded(open)
            .accessibility_label(format!("Switch branch: {}", review.branch_label()))
            .child(pill);

        let content = open.then(|| self.branch_menu(review.branch_label(), reveal, cx));
        let focus = self
            .branch_picker
            .query
            .as_ref()
            .map(|query| query.read(cx).focus_handle(cx));
        Popover::new("branch-picker")
            .flex_shrink_1()
            .min_w_0()
            .anchor(Anchor::TopLeft)
            .offset(px(UI_SPACING))
            .open(open)
            // Type to filter as soon as the menu opens.
            .when_some(focus, |popover, focus| popover.track_focus(&focus))
            .on_open_change(move |open, window, cx| {
                let open = *open;
                _ = this.update(cx, |this, cx| {
                    // Clicking while the branches are still loading cancels the open.
                    if open && !this.branch_picker.open {
                        this.open_branch_picker(window, cx);
                    } else {
                        this.close_branch_picker(cx);
                    }
                });
            })
            .trigger(trigger)
            .when_some(content, |popover, content| {
                popover.content(move |_, _, _| content)
            })
            .into_any_element()
    }

    fn branch_row(
        &self,
        index: usize,
        branch: &str,
        current: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let branch = branch.to_owned();
        let selected = branch == current;
        div()
            .h(px(MENU_ROW_HEIGHT))
            .pb(px(2.))
            .child(
                choice_button(("branch", index), selected)
                    .h(px(CONTROL_HEIGHT))
                    .accessibility_label(format!("Review branch {branch}"))
                    .child(div().size(px(14.)).flex_shrink_0().when(selected, |d| {
                        d.child(
                            Icon::new(IconName::Check)
                                .size(px(14.))
                                .text_color(rgb(TEXT)),
                        )
                    }))
                    .child(div().flex_1().min_w_0().truncate().child(branch.clone()))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.choose_branch(branch.clone(), window, cx)
                    })),
            )
            .into_any_element()
    }

    fn branch_menu(&self, current: String, reveal: f32, cx: &mut Context<Self>) -> AnyElement {
        let visible = self.branch_picker.visible(&current, cx);
        let empty = if self.branch_picker.branches.is_empty() {
            "No local branches yet."
        } else {
            "No branches match."
        };
        column()
            .id("branch-menu")
            .debug_selector(|| "branch-menu".into())
            .relative()
            .top(px(-4. * (1. - reveal)))
            .opacity(reveal)
            .w(px(MENU_WIDTH))
            .p(px(UI_SPACING))
            .gap(px(UI_SPACING))
            .bg(rgb(SURFACE))
            .border_1()
            .border_color(rgb(CONTROL_BORDER))
            .rounded(px(8.))
            .shadow_lg()
            .text_color(rgb(TEXT))
            .when_some(self.branch_picker.query.as_ref(), |menu, query| {
                menu.child(
                    Input::new(query)
                        .id("filter-branches")
                        .focus_bordered(false)
                        .small()
                        .min_h(px(CONTROL_HEIGHT))
                        .max_h(px(CONTROL_HEIGHT))
                        .px(px(CONTROL_PADDING))
                        .py_0()
                        .gap(px(6.))
                        .rounded(px(CONTROL_RADIUS))
                        .bg(rgb(BG))
                        .border_1()
                        .border_color(rgb(BORDER))
                        .text_size(px(12.))
                        .line_height(px(HEADER_PILL_LINE_HEIGHT))
                        .prefix(
                            Icon::new(IconName::Search)
                                .size(px(14.))
                                .text_color(rgb(MUTED)),
                        )
                        .aria_label("Find a branch"),
                )
            })
            .when(visible.is_empty(), |menu| {
                menu.child(
                    div()
                        .px(px(CONTROL_PADDING))
                        .py(px(UI_SPACING))
                        .text_size(px(12.))
                        .text_color(rgb(MUTED))
                        .child(empty),
                )
            })
            .when(!visible.is_empty(), |menu| {
                let rows = visible.len();
                menu.child(
                    uniform_list(
                        "branch-list",
                        rows,
                        cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                            range
                                .map(|index| this.branch_row(index, &visible[index], &current, cx))
                                .collect::<Vec<_>>()
                        }),
                    )
                    .h(px(rows.min(MENU_VISIBLE_ROWS) as f32 * MENU_ROW_HEIGHT))
                    .w_full(),
                )
            })
            .into_any_element()
    }
}
