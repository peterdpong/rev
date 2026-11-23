use super::*;
use std::time::Duration;

#[derive(Default)]
pub(super) struct FileSheet {
    pub(super) open: bool,
    pub(super) floating: bool,
    hovered: [bool; 3],
    epoch: u64,
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
        self.file_sheet.open = true;
        self.file_list.update(cx, |list, cx| list.focus_filter(window, cx));
        cx.notify();
    }

    pub(super) fn hide_file_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.file_sheet.epoch += 1;
        self.file_sheet.hovered = [false; 3];
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

    pub(super) fn hover_file_sheet(
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
}
