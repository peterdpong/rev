use std::collections::HashMap;
use super::controls::*;
use crate::{
    diff::{Diff, DiffEvent, DiffFile, DiffHunkSeparator, DiffMode, DiffState},
    theme::*,
};
use gpui_kit::prelude::*;
use gpui_kit::*;

pub(super) enum DiffViewEvent {
    ToggleViewed(String),
    VisibleFileChanged(String),
}

#[derive(Default, PartialEq)]
pub(super) struct DiffViewStatus {
    pub path: Option<String>,
    pub loading: bool,
    pub files: HashMap<String, DiffFileStatus>,
}

#[derive(Default, PartialEq)]
pub(super) struct DiffFileStatus {
    pub counts: (usize, usize),
    pub viewed: bool,
    pub disabled: bool,
}

pub(super) struct DiffView {
    state: Entity<DiffState>,
    status: DiffViewStatus,
    wrap: bool,
    _state_subscriptions: Vec<Subscription>,
}

impl EventEmitter<DiffViewEvent> for DiffView {}

impl DiffView {
    pub(super) fn new(cx: &mut Context<Self>) -> Self {
        let state = cx.new(|cx| DiffState::new([], cx));
        let subscriptions = vec![
            cx.observe(&state, |_, _, cx| cx.notify()),
            cx.subscribe(&state, |_, _, event, cx| {
                if let DiffEvent::VisibleFileChanged(path) = event {
                    cx.emit(DiffViewEvent::VisibleFileChanged(path.to_string()));
                }
            }),
        ];

        Self {
            state,
            status: DiffViewStatus::default(),
            wrap: false,
            _state_subscriptions: subscriptions,
        }
    }

    pub(super) fn state(&self) -> &Entity<DiffState> {
        &self.state
    }

    pub(super) fn is_selecting(&self, cx: &App) -> bool {
        let state = self.state.read(cx);
        state.is_selecting_lines() || state.is_selecting_text(cx)
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

impl DiffView {
    fn diff_file_header(&self, file: &DiffFile, cx: &mut Context<Self>) -> AnyElement {
        let path = file.path().to_string();
        let status = self.status.files.get(&path);
        let viewed = status.is_some_and(|status| status.viewed);
        let (added, removed) = status.map_or((file.additions(), file.deletions()), |status| status.counts);
        row()
            .id(SharedString::from(format!("diff-heading-{path}")))
            .debug_selector({
                let path = path.clone();
                move || format!("diff-heading-{path}")
            })
            .min_w_0()
            .gap(px(UI_SPACING))
            .h(px(CONTROL_HEIGHT))
            .text_size(px(12.))
            .line_height(px(HEADER_PILL_LINE_HEIGHT))
            .child(
                row()
                    .flex_1()
                    .min_w_0()
                    .child(div().min_w_0().truncate().child(path.clone()))
                    .child(
                        row()
                            .flex_shrink_0()
                            .child(div().text_color(rgb(SUCCESS)).child(format!("+{added}")))
                            .child(div().text_color(rgb(DANGER)).child(format!("−{removed}"))),
                    ),
            )
            .child(
                viewed_button(
                    viewed,
                    status.is_none_or(|status| status.disabled),
                )
                .on_click(cx.listener(move |_, _, _, cx| cx.emit(DiffViewEvent::ToggleViewed(path.clone())))),
            )
            .into_any_element()
    }
}

impl Render for DiffView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.clone();
        let header_owner = cx.weak_entity();
        let content = if self.status.loading && self.state.read(cx).files().is_empty() {
            div()
                .p_6()
                .text_color(rgb(MUTED))
                .child("Loading…")
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
                .render_header(move |file, _, cx| {
                    header_owner.update(cx, |this, cx| this.diff_file_header(file, cx))
                        .unwrap_or_else(|_| div().into_any_element())
                })
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

    struct ResizableHost {
        view: Entity<DiffView>,
        width: f32,
    }

    impl Render for ResizableHost {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().w(px(self.width)).h(px(300.)).child(self.view.clone())
        }
    }

    #[gpui::test]
    fn file_header_controls_follow_a_resize_in_the_same_frame(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::diff::init(cx);
            crate::theme::init(cx);
        });
        let (host, cx) = cx.add_window_view(|_, cx| ResizableHost {
            width: 700.,
            view: cx.new(|cx| {
                let mut view = DiffView::new(cx);
                let files = DiffFile::parse("diff --git a/file.rs b/file.rs\n--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-old\n+new\n").unwrap();
                view.set_files(files, cx);
                view.sync_status(DiffViewStatus { path: Some("file.rs".into()), ..Default::default() }, cx);
                view
            }),
        });
        let draw = |cx: &mut gpui_kit::VisualTestContext| {
            cx.run_until_parked();
            cx.update(|window, cx| {
                window.refresh();
                _ = window.draw(cx);
            });
        };
        for _ in 0..3 {
            draw(cx);
        }
        let inset = px(crate::theme::UI_SPACING);
        let heading = cx.debug_bounds("diff-heading-file.rs").unwrap();
        assert_eq!(heading.right(), px(700.) - inset);

        for width in [520., 760.] {
            cx.update(|window, cx| {
                host.update(cx, |host, cx| {
                    host.width = width;
                    cx.notify();
                });
                _ = window.draw(cx);
            });
            let heading = cx.debug_bounds("diff-heading-file.rs").unwrap();
            assert_eq!(heading.right(), px(width) - inset, "at {width}px");
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
                view.sync_status(DiffViewStatus { path: Some("file.rs".into()), files: [("file.rs".into(), Default::default())].into(), ..Default::default() }, cx);
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
                cx.subscribe(&view, move |_, event, _| {
                    if matches!(event, super::DiffViewEvent::ToggleViewed(_)) { actions.set(actions.get() + 1); }
                }),
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
