use super::controls::*;
use super::popover::*;
use super::*;
use crate::theme::*;
use gpui_kit::base::motion;
use std::time::Duration;

pub(super) const FILE_LIST_WIDTH: f32 = 210.;
pub(super) const FILE_SHEET_WIDTH: f32 = 272.;
pub(super) const FILE_SHEET_MAX_HEIGHT: f32 = 0.8;
pub(super) const FILE_SHEET_EDGE_WIDTH: f32 = 24.;

const EDGE: usize = 0;
const SHEET: usize = 1;

#[derive(Default)]
pub(super) struct FileSheet {
    pub(super) open: bool,
    pub(super) floating: bool,
    hovered: [bool; 2],
    epoch: u64,
    anchor: Option<Pixels>,
    panel: Bounds<Pixels>,
}

impl Rev {
    fn can_show_file_sheet(&self, cx: &App) -> bool {
        self.file_sheet.floating
            && self.review.is_some()
            && !self.loading
            && !self.submitting
            && !self.diff.read(cx).is_selecting(cx)
            && !self.branch_popover.is_open()
    }

    pub(super) fn show_file_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.review.is_none() || self.loading || self.submitting {
            return;
        }
        if !self.file_sheet.floating {
            self.file_list
                .update(cx, |list, cx| list.focus_filter(window, cx));
            return;
        }
        if self.branch_popover.is_open() {
            self.close_branch_popover(window, cx);
        }
        self.file_sheet.epoch += 1;
        if !self.file_sheet.open {
            self.file_sheet.anchor = None;
        }
        self.file_sheet.open = true;
        self.file_list.update(cx, |list, cx| {
            list.scroll_to_selected(ScrollStrategy::Center);
            list.focus_filter(window, cx);
        });
        cx.notify();
    }

    pub(super) fn hide_file_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.file_sheet.epoch += 1;
        self.file_sheet.hovered = [false; 2];
        if !self.file_sheet.open {
            return;
        }
        self.file_sheet.open = false;
        if self.file_list.read(cx).filter_focused(window, cx) {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }

    pub(super) fn toggle_file_list_floating(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.hide_file_sheet(window, cx);
        self.file_sheet.floating = !self.file_sheet.floating;
        cx.notify();
    }

    fn hover_file_sheet(
        &mut self,
        region: usize,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.file_sheet.hovered[region] = hovered;
        self.file_sheet.epoch += 1;
        let epoch = self.file_sheet.epoch;
        let entering = self.file_sheet.hovered.iter().any(|hovered| *hovered);
        if entering && !self.can_show_file_sheet(cx) {
            return;
        }
        let delay = cx
            .background_executor()
            .timer(Duration::from_millis(if entering { 100 } else { 180 }));
        cx.spawn_in(window, async move |this, cx| {
            delay.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.file_sheet.epoch != epoch {
                    return;
                }
                if entering {
                    if this.can_show_file_sheet(cx) && !this.file_sheet.open {
                        if this.file_sheet.hovered[EDGE] {
                            this.file_sheet.anchor = Some(window.mouse_position().y);
                        }
                        this.file_list.update(cx, |list, _| {
                            list.scroll_to_selected(ScrollStrategy::Center)
                        });
                        this.file_sheet.open = true;
                        cx.notify();
                    }
                } else if !this.file_list.read(cx).filter_focused(window, cx) {
                    this.hide_file_sheet(window, cx);
                }
            });
        })
        .detach();
    }

    fn file_sheet_top(&self, height: Pixels) -> Pixels {
        let panel = self.file_sheet.panel;
        let center = self
            .file_sheet
            .anchor
            .map_or(panel.size.height / 2., |y| y - panel.top());
        (center - height / 2.)
            .min(panel.size.height - height)
            .max(px(0.))
    }

    pub(super) fn docked_file_list(&self, openness: f32, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("docked-file-list")
            .debug_selector(|| "docked-file-list".into())
            .w(px(FILE_LIST_WIDTH * openness))
            .h_full()
            .flex_shrink_0()
            .overflow_hidden()
            .child(
                div()
                    .relative()
                    .left(px(-FILE_LIST_WIDTH * (1. - openness)))
                    .w(px(FILE_LIST_WIDTH))
                    .h_full()
                    .bg(rgb(SUBPANEL))
                    .border_r_1()
                    .border_color(rgb(BORDER))
                    .key_context("RevFiles")
                    .on_action(cx.listener(|this, _: &ClosePopover, window, cx| {
                        this.focus.focus(window, cx);
                    }))
                    .child(self.file_list.clone()),
            )
            .into_any_element()
    }

    pub(super) fn diff_panel(
        &mut self,
        floating: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sheet = motion::transition(
            "file-sheet-reveal",
            if self.file_sheet.open {
                1.0_f32
            } else {
                0.0_f32
            },
            motion_transition(),
            window,
            cx,
        );
        let rows = self.file_list.update(cx, |list, _| list.row_count());
        let sheet_height = px((40. + rows as f32 * (LIST_ROW_HEIGHT + 2.)).max(140.))
            .min(self.file_sheet.panel.size.height * FILE_SHEET_MAX_HEIGHT);
        let this = cx.entity().downgrade();
        column()
            .id("diff-review")
            .debug_selector(|| "diff-review".into())
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .child(
                div()
                    .id("diff-surface")
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(self.diff.clone()),
            )
            .when(floating, |d| {
                d.child(
                    canvas(
                        move |bounds, _, cx| {
                            _ = this.update(cx, |this, cx| {
                                if this.file_sheet.panel != bounds {
                                    this.file_sheet.panel = bounds;
                                    cx.notify();
                                }
                            });
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .child(
                    div()
                        .id("file-sheet-edge")
                        .debug_selector(|| "file-sheet-edge".into())
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(px(FILE_SHEET_EDGE_WIDTH))
                        .on_hover(cx.listener(|this, hovered, window, cx| {
                            this.hover_file_sheet(EDGE, *hovered, window, cx);
                        })),
                )
            })
            .when(floating && (self.file_sheet.open || sheet > 0.), |d| {
                d.child(
                    popover_surface()
                        .id("file-sheet")
                        .debug_selector(|| "file-sheet".into())
                        .absolute()
                        .left(px(UI_SPACING - (FILE_SHEET_WIDTH + UI_SPACING) * (1. - sheet)))
                        .top(self.file_sheet_top(sheet_height))
                        .w(px(FILE_SHEET_WIDTH))
                        .h(sheet_height)
                        .opacity(sheet)
                        .overflow_hidden()
                        .occlude()
                        .key_context("RevFiles")
                        .on_action(cx.listener(|this, _: &ClosePopover, window, cx| {
                            this.hide_file_sheet(window, cx);
                        }))
                        .on_hover(cx.listener(|this, hovered, window, cx| {
                            this.hover_file_sheet(SHEET, *hovered, window, cx);
                        }))
                        .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                            this.hide_file_sheet(window, cx);
                        }))
                        .child(self.file_list.clone()),
                )
            })
            .into_any_element()
    }
}
