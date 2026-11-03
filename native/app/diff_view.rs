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
    LoadFile(String),
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
    pub deferred: bool,
    pub loading: bool,
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
            cx.subscribe(&state, |_, _, event, cx| match event {
                DiffEvent::FileExpanded(path) => cx.emit(DiffViewEvent::LoadFile(path.to_string())),
                DiffEvent::VisibleFileChanged(path) => cx.emit(DiffViewEvent::VisibleFileChanged(path.to_string())),
                _ => {},
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
        let load_path = path.clone();
        let viewed = status.is_some_and(|status| status.viewed);
        let (added, removed) = status.map_or((file.additions(), file.deletions()), |status| status.counts);
        row()
            .id(SharedString::from(format!("diff-heading-{path}")))
            .min_w_0()
            .gap(px(UI_SPACING))
            .h(px(CONTROL_HEIGHT))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.))
                    .child(path.clone()),
            )
            .when(status.is_some_and(|status| status.deferred), |d| {
                d.child(div().text_size(px(11.)).text_color(rgb(MUTED)).child("Large diff"))
                    .child(button(SharedString::from(format!("load-diff-{path}")),
                        if status.is_some_and(|status| status.loading) { "Loading…" } else { "Load diff" })
                        .disabled(status.is_some_and(|status| status.loading))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.state.update(cx, |state, cx| state.set_file_collapsed(&load_path, false, cx));
                            cx.emit(DiffViewEvent::LoadFile(load_path.clone()));
                        })))
            })
            .when(added > 0, |d| {
                d.child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(SUCCESS))
                        .child(format!("+{added}")),
                )
            })
            .when(removed > 0, |d| {
                d.child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(DANGER))
                        .child(format!("−{removed}")),
                )
            })
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

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.state.update(cx, |state, cx| {
                    state.begin_line_selection(
                        crate::diff::DiffLinePosition::new("file.rs", DiffSide::Modified, 2),
                        false, window, cx,
                    );
                });
                assert!(view.is_selecting(cx));
                view.state.update(cx, |state, cx| state.set_files([], cx));
                assert!(!view.is_selecting(cx));
            });
        });
    }

    #[gpui::test]
    fn deferred_file_buttons_emit_the_path_and_expand_the_file(cx: &mut TestAppContext) {
        use gpui_kit::test::TestQueryExt;
        use super::{DiffFileStatus, DiffViewEvent};
        use std::cell::RefCell;

        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::diff::init(cx);
            crate::theme::init(cx);
        });
        let (host, cx) = cx.add_window_view(|_, cx| Host {
            view: cx.new(|cx| {
                let mut view = DiffView::new(cx);
                view.set_files([DiffFile::placeholder("large.rs", "Large diff")], cx);
                view.state.update(cx, |state, cx| state.set_file_collapsed("large.rs", true, cx));
                view.sync_status(DiffViewStatus {
                    path: Some("large.rs".into()),
                    files: [("large.rs".into(), DiffFileStatus {
                        deferred: true, disabled: true, ..Default::default()
                    })].into(),
                    ..Default::default()
                }, cx);
                view
            }),
        });
        let view = cx.update(|_, cx| host.read(cx).view.clone());
        let paths = Rc::new(RefCell::new(Vec::new()));
        let _subscription = cx.update(|_, cx| {
            let paths = paths.clone();
            cx.subscribe(&view, move |_, event, _| {
                if let DiffViewEvent::LoadFile(path) = event {
                    paths.borrow_mut().push(path.clone());
                }
            })
        });
        cx.run_until_parked();
        let button = cx.update(|window, cx| {
            window.refresh();
            _ = window.draw(cx);
            window.find_by_label("Load diff").bounds()
        });
        cx.simulate_click(button.center(), Default::default());
        cx.run_until_parked();
        assert!(!paths.borrow().is_empty());
        assert!(paths.borrow().iter().all(|path| path == "large.rs"));
        cx.update(|_, cx| assert!(!view.read(cx).state.read(cx).is_file_collapsed("large.rs")));
    }

}
