use super::*;

#[derive(Default)]
pub(super) struct FileSheet {
    pub(super) open: bool,
    hovered: [bool; 3],
    epoch: u64,
}

impl Patch {
    fn can_show_file_sheet(&self) -> bool {
        self.review.is_some()
            && !self.loading
            && !self.submitting
            && !self.threads_visible
            && !self.selecting
            && self.modal.is_none()
    }

    pub(super) fn show_file_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.review.is_none() || self.loading || self.threads_visible || self.submitting {
            return;
        }
        if self.modal.is_some() {
            self.close_modal(window, cx);
        }
        self.file_sheet.epoch += 1;
        self.file_sheet.open = true;
        self.filter.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    pub(super) fn hide_file_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.file_sheet.epoch += 1;
        self.file_sheet.hovered = [false; 3];
        if !self.file_sheet.open {
            return;
        }
        self.file_sheet.open = false;
        if self.filter.focus_handle(cx).contains_focused(window, cx) {
            self.focus.focus(window, cx);
        }
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
        if entering && !self.can_show_file_sheet() {
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
                    if this.can_show_file_sheet() && !this.file_sheet.open {
                        this.file_sheet.open = true;
                        cx.notify();
                    }
                } else if !this.filter.focus_handle(cx).contains_focused(window, cx) {
                    this.hide_file_sheet(window, cx);
                }
            });
        })
        .detach();
    }
}
