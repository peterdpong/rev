use super::controls::*;
use super::*;
use crate::theme::*;
use gpui_kit::{
    assets::IconName,
    base::{Button as BaseButton, Popover},
    component::Icon,
};

const MENU_WIDTH: f32 = 320.;
const MENU_VISIBLE_ROWS: usize = 10;
const MENU_ROW_HEIGHT: f32 = CONTROL_HEIGHT + 2.;

#[derive(Default)]
pub(super) struct BranchPicker {
    pub open: bool,
    pub branches: Vec<String>,
    pub loading: bool,
    /// The repository the loaded branches belong to, so reopening the menu shows
    /// them at once while a fresh list loads.
    repo: Option<String>,
    serial: u64,
}

impl BranchPicker {
    /// Like the commit menu, this stays closed until it has branches to show.
    pub fn ready(&self) -> bool {
        self.open && (!self.loading || !self.branches.is_empty())
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

    fn open_branch_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_branch_picker(cx);
        if let Some(review) = self.review.as_ref() {
            self.open_branch_modal(review.request.repo.clone(), window, cx);
        }
    }

    /// Wraps the header's title pill so clicking it opens the branch dropdown.
    pub(super) fn branch_picker(
        &self,
        pill: AnyElement,
        review: &Review,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let requested = self.branch_picker.open;
        let open = self.branch_picker.ready();
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

        let current = review.branch_label();
        let content = open.then(|| self.branch_menu(current, cx));
        Popover::new("branch-picker")
            .flex_shrink_1()
            .min_w_0()
            .anchor(Anchor::TopLeft)
            .offset(px(UI_SPACING))
            .open(open)
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

    fn branch_row(&self, index: usize, current: &str, cx: &mut Context<Self>) -> AnyElement {
        let branch = self.branch_picker.branches[index].clone();
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

    fn branch_menu(&self, current: String, cx: &mut Context<Self>) -> AnyElement {
        let branches = &self.branch_picker.branches;
        column()
            .id("branch-menu")
            .debug_selector(|| "branch-menu".into())
            .w(px(MENU_WIDTH))
            .p(px(UI_SPACING))
            .gap(px(UI_SPACING))
            .bg(rgb(SURFACE))
            .border_1()
            .border_color(rgb(CONTROL_BORDER))
            .rounded(px(8.))
            .shadow_lg()
            .text_color(rgb(TEXT))
            .when(branches.is_empty(), |menu| {
                menu.child(
                    div()
                        .px(px(CONTROL_PADDING))
                        .py(px(UI_SPACING))
                        .text_size(px(12.))
                        .text_color(rgb(MUTED))
                        .child("No local branches yet."),
                )
            })
            .when(!branches.is_empty(), |menu| {
                menu.child(
                    uniform_list(
                        "branch-list",
                        branches.len(),
                        cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                            range
                                .map(|index| this.branch_row(index, &current, cx))
                                .collect::<Vec<_>>()
                        }),
                    )
                    .h(px(branches.len().min(MENU_VISIBLE_ROWS) as f32 * MENU_ROW_HEIGHT))
                    .w_full(),
                )
            })
            .child(div().h(px(1.)).flex_shrink_0().bg(rgb(BORDER)))
            .child(
                choice_button("open-branch-dialog", false)
                    .h(px(CONTROL_HEIGHT))
                    .accessibility_label("Open a branch or PR")
                    .child(
                        Icon::new(IconName::Plus)
                            .size(px(14.))
                            .flex_shrink_0()
                            .text_color(rgb(MUTED)),
                    )
                    .child("Open branch or PR…")
                    .on_click(
                        cx.listener(|this, _, window, cx| this.open_branch_dialog(window, cx)),
                    ),
            )
            .into_any_element()
    }
}
