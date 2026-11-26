use std::collections::HashMap;
use super::{CloseFind, FindNext, FindPrevious, controls::*, popover::*};
use crate::{
    diff::{Diff, DiffEvent, DiffFile, DiffHunkSeparator, DiffMode, DiffState},
    theme::*,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::prelude::*;
use gpui_kit::*;

const COPY_PATH_BUTTON_SIZE: f32 = 20.;
const FIND_PADDING: f32 = 4.;
const FIND_TOP: f32 = 52.;
const FIND_RIGHT: f32 = UI_SPACING;

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
    files: Vec<DiffFile>,
    single_file: bool,
    find: Option<Entity<InputState>>,
    find_open: bool,
    find_status: (SharedString, usize),
    _find_subscription: Option<Subscription>,
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
            files: Vec::new(),
            single_file: false,
            find: None,
            find_open: false,
            find_status: Default::default(),
            _find_subscription: None,
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
        self.files = files.into_iter().collect();
        let path = self.status.path.clone();
        self.show_files(path.as_deref(), false, cx);
    }

    pub(super) fn refresh_files(
        &mut self,
        files: Vec<DiffFile>,
        preserve_scroll: bool,
        cx: &mut Context<Self>,
    ) {
        self.files = files;
        let path = self.status.path.clone();
        self.show_files(path.as_deref(), preserve_scroll, cx);
    }

    fn show_files(&mut self, path: Option<&str>, preserve_scroll: bool, cx: &mut Context<Self>) {
        let files = if self.single_file {
            path.and_then(|path| self.files.iter().find(|file| file.path().as_str() == path))
                .cloned()
                .into_iter()
                .collect()
        } else {
            self.files.clone()
        };

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

    pub(super) fn focus_file(&mut self, path: &str, cx: &mut Context<Self>) {
        let shown = self.state.read(cx).files().first().map(|file| file.path().to_string());
        if self.single_file && shown.as_deref() != Some(path) {
            self.show_files(Some(path), false, cx);
        }

        self.state.update(cx, |state, cx| {
            state.set_selected_lines(None, cx);
            state.set_file_collapsed(path, false, cx);
            state.scroll_to_file(path, cx);
        });
    }

    /// Opens the find bar and selects its query, replacing the query with
    /// the selected text when the selection is within one line.
    pub(super) fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.state.read(cx).selected_text(cx);
        let selected = selected.trim_end_matches(['\n', '\r']);
        let input = self.find_input(window, cx);
        input.update(cx, |input, cx| {
            if !selected.is_empty() && !selected.contains('\n') {
                input.set_value(selected.to_owned(), window, cx);
            }
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        self.find_open = true;
        let query = input.read(cx).value();
        self.state
            .update(cx, |state, cx| state.set_search_query(&query, cx));
        cx.notify();
    }

    pub(super) fn toggle_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.find_open {
            self.close_find(window, cx);
        } else {
            self.open_find(window, cx);
        }
    }

    pub(super) fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.find_open {
            return;
        }
        self.find_open = false;
        self.state.update(cx, |state, cx| {
            state.set_search_query("", cx);
            window.focus(&state.focus_handle(cx), cx);
        });
        cx.notify();
    }

    fn find_input(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<InputState> {
        if let Some(input) = &self.find {
            return input.clone();
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Find"));
        let subscription =
            cx.subscribe_in(&input, window, |this, input, event: &InputEvent, _, cx| {
                if let InputEvent::Change = event {
                    let query = input.read(cx).value();
                    this.state
                        .update(cx, |state, cx| state.set_search_query(&query, cx));
                }
            });
        self._find_subscription = Some(subscription);
        self.find = Some(input.clone());
        input
    }

    fn find_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let input = self.find.clone()?;
        let open = self.find_open;
        let motion = PopoverMotion::new("diff-find-motion", open, window, cx);
        if !motion.visible() {
            return None;
        }
        if open {
            let state = self.state.read(cx);
            let count = state.search_match_count();
            let status = match (state.current_search_match(), count) {
                _ if state.search_query().is_empty() => String::new(),
                (_, 0) => "No results".to_owned(),
                (Some(current), count) => format!("{} of {count}", current + 1),
                (None, count) => format!("{count} results"),
            };
            self.find_status = (status.into(), count);
        }
        let (status, count) = self.find_status.clone();
        let step = |forward: bool| {
            cx.listener(move |this: &mut Self, _, _, cx| {
                this.state
                    .update(cx, |state, cx| state.select_search_match(forward, cx));
            })
        };
        let bar = popover_surface()
            .id("diff-find")
            .key_context("RevFind")
            .on_action(cx.listener(|this, _: &CloseFind, window, cx| this.close_find(window, cx)))
            .on_action(cx.listener(|this, _: &FindNext, _, cx| {
                this.state
                    .update(cx, |state, cx| state.select_search_match(true, cx));
            }))
            .on_action(cx.listener(|this, _: &FindPrevious, _, cx| {
                this.state
                    .update(cx, |state, cx| state.select_search_match(false, cx));
            }))
            .flex_row()
            .items_center()
            .gap(px(2.))
            .p(px(FIND_PADDING))
            .text_size(px(12.))
            .line_height(px(HEADER_PILL_LINE_HEIGHT))
            .child(
                Input::new(&input)
                    .id("diff-find-input")
                    .focus_bordered(false)
                    .small()
                    .w(px(220.))
                    .min_h(px(CONTROL_HEIGHT))
                    .max_h(px(CONTROL_HEIGHT))
                    .px(px(CONTROL_PADDING))
                    .py_0()
                    .gap(px(6.))
                    .rounded(px(CONTROL_RADIUS))
                    .bg(rgb(SUBPANEL))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .text_size(px(12.))
                    .line_height(px(HEADER_PILL_LINE_HEIGHT))
                    .prefix(
                        Icon::new(IconName::Search)
                            .size(px(14.))
                            .text_color(rgb(MUTED)),
                    ),
            )
            .child(
                div()
                    .id("diff-find-status")
                    .debug_selector(|| "diff-find-status".into())
                    .min_w(px(68.))
                    .px(px(6.))
                    .text_color(rgb(MUTED))
                    .child(status),
            )
            .child(
                icon_button(
                    "diff-find-previous",
                    "Previous match (⇧↩)",
                    IconName::ArrowUp,
                )
                .disabled(count == 0)
                .on_click(step(false)),
            )
            .child(
                icon_button("diff-find-next", "Next match (↩)", IconName::ArrowDown)
                    .disabled(count == 0)
                    .on_click(step(true)),
            )
            .child(
                icon_button("diff-find-close", "Close (esc)", IconName::Close)
                    .on_click(cx.listener(|this, _, window, cx| this.close_find(window, cx))),
            );
        Some(
            div()
                .absolute()
                .top(px(FIND_TOP))
                .right(px(FIND_RIGHT))
                .child(motion.surface(bar.into_any_element(), open))
                .into_any_element(),
        )
    }

    pub(super) fn is_single_file(&self) -> bool {
        self.single_file
    }

    pub(super) fn toggle_single_file(&mut self, cx: &mut Context<Self>) {
        self.single_file = !self.single_file;
        let path = self.status.path.clone();
        self.show_files(path.as_deref(), false, cx);
        if let Some(path) = path {
            self.state
                .update(cx, |state, cx| state.scroll_to_file(&path, cx));
        }
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
                        CopyButton::new(format!("copy-path-{path}"), path.clone(), "Copy file path")
                            .size(px(COPY_PATH_BUTTON_SIZE))
                            .rounded(px(CONTROL_RADIUS))
                            .border_1()
                            .border_color(rgba(0x00000000)),
                    )
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let find_bar = self.find_bar(window, cx);
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
                .sticky_header(true)
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
            .relative()
            .child(div().id("diff-surface").flex_1().min_h_0().overflow_hidden().child(content))
            .children(find_bar)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{DiffView, DiffViewStatus};
    use crate::app::controls::MOTION_DURATION;
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
    fn single_file_mode_shows_only_the_current_file(cx: &mut TestAppContext) {
        let view = cx.new(DiffView::new);
        let shown = |cx: &mut TestAppContext| {
            view.read_with(cx, |view, cx| {
                view.state()
                    .read(cx)
                    .files()
                    .iter()
                    .map(|file| file.path().to_string())
                    .collect::<Vec<_>>()
            })
        };
        view.update(cx, |view, cx| {
            let files = DiffFile::parse(
                "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-a\n+b\n\
                 diff --git a/b.rs b/b.rs\n--- a/b.rs\n+++ b/b.rs\n@@ -1 +1 @@\n-a\n+b\n",
            )
            .unwrap();
            view.refresh_files(files, false, cx);
            view.focus_file("b.rs", cx);
            view.sync_status(DiffViewStatus { path: Some("b.rs".into()), ..Default::default() }, cx);
        });
        assert_eq!(shown(cx), ["a.rs", "b.rs"]);

        view.update(cx, |view, cx| view.toggle_single_file(cx));
        assert_eq!(shown(cx), ["b.rs"]);
        view.update(cx, |view, cx| view.focus_file("a.rs", cx));
        assert_eq!(shown(cx), ["a.rs"]);

        view.update(cx, |view, cx| view.toggle_single_file(cx));
        assert_eq!(shown(cx), ["a.rs", "b.rs"]);
    }

    #[gpui::test]
    fn file_header_copies_its_path(cx: &mut TestAppContext) {
        use gpui_kit::test::TestQueryExt;

        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::diff::init(cx);
            crate::theme::init(cx);
        });
        let (host, cx) = cx.add_window_view(|_, cx| Host {
            view: cx.new(|cx| {
                let mut view = DiffView::new(cx);
                let files = DiffFile::parse("diff --git a/src/app/view.rs b/src/app/view.rs\n--- a/src/app/view.rs\n+++ b/src/app/view.rs\n@@ -1 +1 @@\n-old\n+new\n").unwrap();
                view.set_files(files, cx);
                view.sync_status(DiffViewStatus { path: Some("src/app/view.rs".into()), ..Default::default() }, cx);
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
        draw(cx);
        let heading = cx.debug_bounds("diff-heading-src/app/view.rs").unwrap();
        let copy = cx.debug_bounds("copy-path-src/app/view.rs").unwrap();
        assert!(
            copy.left() > heading.left() && copy.right() < heading.right(),
            "sit inside the file header: {heading:?}, {copy:?}"
        );
        assert_eq!(copy.size.height, px(super::COPY_PATH_BUTTON_SIZE));
        let labelled = cx.update(|window, _| window.find_by_label("Copy file path").bounds());
        assert_eq!(labelled, copy);

        cx.simulate_click(copy.center(), Default::default());
        draw(cx);
        let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
        assert_eq!(copied.as_deref(), Some("src/app/view.rs"));
        let collapsed = cx.update(|_, cx| {
            let view = host.read(cx).view.clone();
            let state = view.read(cx).state.clone();
            state.read(cx).is_file_collapsed("src/app/view.rs")
        });
        assert!(!collapsed, "copying leaves the file expanded");
        let copied_label = cx.update(|window, _| window.find_by_label("Copied").bounds());
        assert_eq!(copied_label, copy, "relabel the button after copying");
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

    #[gpui::test]
    fn find_prefills_a_one_line_selection_and_steps_with_enter(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::diff::init(cx);
            crate::theme::init(cx);
            super::super::bind_shortcuts(cx);
        });
        let (host, cx) = cx.add_window_view(|_, cx| Host {
            view: cx.new(|cx| {
                let mut view = DiffView::new(cx);
                let files = DiffFile::parse(
                    "diff --git a/file.rs b/file.rs\n--- a/file.rs\n+++ b/file.rs\n@@ -1,2 +1,2 @@\n needle\n-old needle\n+new needle\n",
                )
                .unwrap();
                view.set_files(files, cx);
                view.sync_status(DiffViewStatus { path: Some("file.rs".into()), ..Default::default() }, cx);
                view
            }),
        });
        let view = cx.update(|_, cx| host.read(cx).view.clone());
        let draw = |cx: &mut gpui_kit::VisualTestContext| {
            cx.run_until_parked();
            cx.update(|window, cx| {
                window.refresh();
                _ = window.draw(cx);
            });
        };
        let status = |cx: &mut gpui_kit::VisualTestContext| {
            view.read_with(cx, |view, cx| {
                let state = view.state.read(cx);
                (
                    state.search_query().to_owned(),
                    state.current_search_match(),
                    state.search_match_count(),
                )
            })
        };
        draw(cx);

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.state.update(cx, |state, cx| {
                    state.set_selected_lines(
                        Some(DiffLineRange::new("file.rs", DiffSide::Modified, 1, 1)),
                        cx,
                    )
                });
                view.open_find(window, cx);
            })
        });
        cx.executor().advance_clock(MOTION_DURATION);
        draw(cx);
        assert_eq!(status(cx), ("needle".into(), Some(0), 3));
        assert!(cx.debug_bounds("diff-find-status").is_some());

        cx.simulate_keystrokes("enter");
        assert_eq!(status(cx).1, Some(1));
        cx.simulate_keystrokes("shift-enter shift-enter");
        assert_eq!(status(cx).1, Some(2), "wraps back to the last match");

        cx.simulate_keystrokes("escape");
        draw(cx);
        assert_eq!(status(cx), (String::new(), None, 0));
        assert!(
            cx.debug_bounds("diff-find-status").is_some(),
            "stays on screen while it animates out"
        );
        cx.executor().advance_clock(MOTION_DURATION);
        draw(cx);
        assert!(cx.debug_bounds("diff-find-status").is_none());

        cx.update(|window, cx| view.update(cx, |view, cx| view.toggle_find(window, cx)));
        assert_eq!(
            status(cx).0,
            "needle",
            "toggling reopens with the last query"
        );
        cx.update(|window, cx| view.update(cx, |view, cx| view.toggle_find(window, cx)));
        assert_eq!(status(cx).0, "", "toggling again closes it");

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.state.update(cx, |state, cx| {
                    state.set_selected_lines(
                        Some(DiffLineRange::new("file.rs", DiffSide::Modified, 1, 2)),
                        cx,
                    )
                });
                view.open_find(window, cx);
            })
        });
        assert_eq!(
            status(cx).0,
            "needle",
            "a multi-line selection keeps the last query"
        );

        cx.simulate_input("old");
        assert_eq!(status(cx), ("old".into(), Some(0), 1));
    }
}
