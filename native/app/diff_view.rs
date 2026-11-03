use super::controls::*;
use crate::{
    diff::{Diff, DiffFile, DiffHunkSeparator, DiffMode, DiffState},
    theme::*,
};
use gpui_kit::prelude::*;
use gpui_kit::*;

pub(super) enum DiffViewEvent {
    ToggleViewed,
}

#[derive(Default, PartialEq)]
pub(super) struct DiffViewStatus {
    pub path: Option<String>,
    pub counts: Option<(usize, usize)>,
    pub viewed: bool,
    pub viewed_disabled: bool,
    pub loading: bool,
    pub notice: Option<String>,
}

pub(super) struct DiffView {
    state: Entity<DiffState>,
    status: DiffViewStatus,
    wrap: bool,
    _state_subscription: Subscription,
}

impl EventEmitter<DiffViewEvent> for DiffView {}

impl DiffView {
    pub(super) fn new(cx: &mut Context<Self>) -> Self {
        let state = cx.new(|cx| DiffState::new([], cx));
        let subscription = cx.observe(&state, |_, _, cx| cx.notify());

        Self {
            state,
            status: DiffViewStatus::default(),
            wrap: false,
            _state_subscription: subscription,
        }
    }

    pub(super) fn sync_status(&mut self, status: DiffViewStatus, cx: &mut Context<Self>) {
        if self.status != status {
            self.status = status;
            cx.notify();
        }
    }

    pub(super) fn set_files(
        &mut self,
        files: impl IntoIterator<Item = DiffFile>,
        cx: &mut Context<Self>,
    ) {
        self.state
            .update(cx, |state, cx| state.set_files(files, cx));
    }

    pub(super) fn refresh_files(
        &mut self,
        files: Vec<DiffFile>,
        preserve_scroll: bool,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |state, cx| {
            let scroll = state.list().logical_scroll_top();
            let horizontal = state.horizontal_scroll().offset();
            state.set_files(files, cx);
            if preserve_scroll {
                state.list().scroll_to(scroll);
                state.horizontal_scroll().set_offset(horizontal);
            }
        });
    }

    pub(super) fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.state
            .update(cx, |state, cx| state.set_selected_lines(None, cx));
    }

    pub(super) fn is_split(&self, cx: &App) -> bool {
        self.state.read(cx).mode() == DiffMode::Split
    }

    pub(super) fn toggle_mode(&mut self, cx: &mut Context<Self>) {
        let mode = if self.is_split(cx) {
            DiffMode::Unified
        } else {
            DiffMode::Split
        };
        self.state.update(cx, |state, cx| state.set_mode(mode, cx));
    }

    pub(super) fn wraps_lines(&self) -> bool {
        self.wrap
    }

    pub(super) fn toggle_wrap(&mut self, cx: &mut Context<Self>) {
        self.wrap = !self.wrap;
        cx.notify();
    }
}

impl Render for DiffView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.clone();
        let file_header = file_header_row()
            .child(
                row()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(12.))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(self.status.path.clone().unwrap_or_default()),
                    )
                    .when_some(self.status.counts, |d, (added, removed)| {
                        d.child(
                            row()
                                .flex_shrink_0()
                                .child(div().text_color(rgb(SUCCESS)).child(format!("+{added}")))
                                .child(div().text_color(rgb(DANGER)).child(format!("−{removed}"))),
                        )
                    }),
            )
            .child(
                viewed_button(self.status.viewed, self.status.viewed_disabled)
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(DiffViewEvent::ToggleViewed))),
            );

        let content = if self.status.loading {
            div()
                .p_6()
                .text_color(rgb(MUTED))
                .child("Loading…")
                .into_any_element()
        } else if let Some(notice) = &self.status.notice {
            div()
                .p_6()
                .text_color(rgb(MUTED))
                .child(notice.clone())
                .into_any_element()
        } else if self.status.path.is_none() {
            div()
                .p_6()
                .text_color(rgb(MUTED))
                .child("No changes")
                .into_any_element()
        } else {
            Diff::new(&self.state)
                .size_full()
                .border_0()
                .header_visible(false)
                .hunk_separator(DiffHunkSeparator::Simple)
                .soft_wrap(self.wrap)
                .text_size(px(12.))
                .on_add_annotation(move |range, _, cx| {
                    state.update(cx, |diff, cx| {
                        diff.set_selected_lines(Some(range.clone()), cx)
                    });
                })
                .into_any_element()
        };

        column()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(file_header)
            .child(div().id("diff-surface").flex_1().min_h_0().overflow_hidden().child(content))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{DiffView, DiffViewStatus};
    use crate::diff::{DiffFile, DiffLineRange, DiffSide};
    use gpui_kit::{
        AppContext, Context, Entity, IntoElement, Render, TestAppContext, Window, div, prelude::*,
        px,
    };
    use std::{cell::Cell, rc::Rc};

    struct Host {
        view: Entity<DiffView>,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().w(px(700.)).h(px(300.)).child(self.view.clone())
        }
    }

    #[gpui::test]
    fn selections_redraw_the_diff_and_viewed_clicks_emit_an_action(cx: &mut TestAppContext) {
        use gpui_kit::test::TestQueryExt;

        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::diff::init(cx);
            crate::theme::init(cx);
        });

        let (host, cx) = cx.add_window_view(|_, cx| Host {
            view: cx.new(|cx| {
                let mut view = DiffView::new(cx);
                let files = DiffFile::parse("diff --git a/file.rs b/file.rs\n--- a/file.rs\n+++ b/file.rs\n@@ -1,3 +1,3 @@\n before\n-old\n+new\n after\n").unwrap();
                view.set_files(files, cx);
                view.sync_status(DiffViewStatus { path: Some("file.rs".into()), ..Default::default() }, cx);
                view
            }),
        });

        let view = cx.update(|_, cx| host.read(cx).view.clone());
        cx.run_until_parked();

        let notifications = Rc::new(Cell::new(0));
        let actions = Rc::new(Cell::new(0));
        let _subscriptions = cx.update(|_, cx| {
            let notifications = notifications.clone();
            let actions = actions.clone();
            [
                cx.observe(&view, move |_, _| {
                    notifications.set(notifications.get() + 1)
                }),
                cx.subscribe(&view, move |_, _, _| actions.set(actions.get() + 1)),
            ]
        });

        let range = DiffLineRange::new("file.rs", DiffSide::Original, 2, 2)
            .with_end_side(DiffSide::Modified);
        view.update(cx, |view, cx| {
            view.state.update(cx, |state, cx| {
                state.set_selected_lines(Some(range.clone()), cx)
            });
        });
        cx.run_until_parked();
        assert!(notifications.get() > 0);
        assert_eq!(actions.get(), 0);

        cx.update(|window, cx| {
            window.refresh();
            _ = window.draw(cx);
            assert_eq!(view.read(cx).state.read(cx).selected_lines(), Some(range));
        });

        let button = cx.update(|window, _| window.find_by_label("Viewed").bounds());
        cx.simulate_click(button.center(), Default::default());
        cx.run_until_parked();
        assert_eq!(actions.get(), 1);

        view.update(cx, |view, cx| {
            view.clear_selection(cx);
            assert!(view.state.read(cx).selected_lines().is_none());
        });
    }
}
