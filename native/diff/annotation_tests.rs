use std::{cell::RefCell, rc::Rc};

use gpui::{
    AppContext as _, Context, Entity, IntoElement, Modifiers, ParentElement as _, Render,
    Styled as _, TestAppContext, VisualTestContext, Window, div, px,
};
use gpui_kit::test::TestWindowExt as _;

use super::{Diff, DiffLinePosition, DiffLineRange, DiffSide, DiffState, document::fixture};

struct Host {
    state: Entity<DiffState>,
    annotations: Rc<RefCell<Vec<DiffLineRange>>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let annotations = self.annotations.clone();
        div().w(px(400.)).h(px(200.)).child(
            Diff::new(&self.state)
                .size_full()
                .header_visible(false)
                .on_add_annotation(move |range, _, _| {
                    annotations.borrow_mut().push(range.clone());
                }),
        )
    }
}

fn range(start: usize, end: usize) -> DiffLineRange {
    DiffLineRange::new("review.txt", DiffSide::Modified, start, end)
}

fn setup(cx: &mut TestAppContext) -> (Entity<Host>, &mut VisualTestContext) {
    cx.update(super::test_init);
    let state = cx.new(|cx| {
        DiffState::new(
            [fixture::document(
                None,
                Some(("review.txt", "one\ntwo\nthree\nfour\n")),
            )],
            cx,
        )
    });
    let (host, cx) = cx.add_window_view(move |_, _| Host {
        state,
        annotations: Rc::default(),
    });
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, cx| window.render_frame(cx));
    }
    (host, cx)
}

#[gpui::test]
fn annotation_click_inside_selection_resets_to_that_line(cx: &mut TestAppContext) {
    let (host, cx) = setup(cx);
    cx.update(|window, cx| {
        let state = host.read(cx).state.clone();
        state.update(cx, |state, cx| {
            state.set_selected_lines(Some(range(1, 3)), cx)
        });
        window.render_frame(cx);
        window.within(("new", 1usize)).hover("source", cx);
        window.click(("add-annotation", 1usize), cx);
        assert_eq!(state.read(cx).selected_lines(), Some(range(2, 2)));
        assert_eq!(*host.read(cx).annotations.borrow(), [range(2, 2)]);
    });
}

#[gpui::test]
fn annotation_drag_starts_at_pressed_line_and_opens_on_release(cx: &mut TestAppContext) {
    let (host, cx) = setup(cx);
    cx.update(|window, cx| {
        let state = host.read(cx).state.clone();
        for selection in [None, Some(range(1, 3))] {
            state.update(cx, |state, cx| state.set_selected_lines(selection, cx));
            window.render_frame(cx);
            let from = window.find(("add-annotation", 2usize)).bounds().center();
            let to = window.find_all("source")[3].bounds().center();
            window.drag(from, to, cx);
            assert_eq!(state.read(cx).selected_lines(), Some(range(3, 4)));
        }
        // Release reports the dragged range.
        assert_eq!(
            *host.read(cx).annotations.borrow(),
            [range(3, 4), range(3, 4)]
        );
    });
}

#[gpui::test]
fn unified_drag_from_deletion_follows_pointer_onto_its_paired_addition(cx: &mut TestAppContext) {
    let (host, cx) = setup(cx);
    cx.update(|window, cx| {
        let state = host.read(cx).state.clone();
        state.update(cx, |state, cx| {
            state.set_files(
                [fixture::modified("review.txt", "one\nold\nthree\n", "one\nnew\nthree\n")],
                cx,
            )
        });
        window.render_frame(cx);
        // Rows: one, old (deleted), new (added), three.
        let to = window.find_all("source")[2].bounds().center();
        state.update(cx, |state, cx| {
            let old = DiffLinePosition::new("review.txt", DiffSide::Original, 2);
            state.begin_line_selection(old, false, window, cx);
            state.drag_line_selection_at(to, cx);
        });
        assert_eq!(
            state.read(cx).selected_lines(),
            Some(
                DiffLineRange::new("review.txt", DiffSide::Original, 2, 2)
                    .with_end_side(DiffSide::Modified)
            )
        );
    });
}

#[gpui::test]
fn annotation_bar_spans_the_selection_during_and_after_a_drag(cx: &mut TestAppContext) {
    use gpui::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput};
    let (host, cx) = setup(cx);
    cx.update(|window, cx| {
        let state = host.read(cx).state.clone();
        let bars = |window: &Window| {
            (0..4usize)
                .filter(|ix| !window.find_all(("annotation-bar", *ix)).is_empty())
                .collect::<Vec<_>>()
        };
        window.within(("new", 0usize)).hover("source", cx);
        let from = window.find(("add-annotation", 0usize)).bounds().center();
        let to = window.find_all("source")[2].bounds().center();
        let modifiers = Modifiers::default();
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: from,
                modifiers,
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: to,
                pressed_button: Some(MouseButton::Left),
                modifiers,
            }),
            cx,
        );
        window.render_frame(cx);
        // Held at line 3: the bar spans lines 1–3.
        assert_eq!(state.read(cx).selected_lines(), Some(range(1, 3)));
        assert_eq!(bars(window), [0, 1, 2]);

        window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: to,
                modifiers,
                click_count: 1,
            }),
            cx,
        );
        window.render_frame(cx);
        // The bar stays until the selection is cleared.
        assert_eq!(bars(window), [0, 1, 2]);
        assert_eq!(*host.read(cx).annotations.borrow(), [range(1, 3)]);
        state.update(cx, |state, cx| state.set_selected_lines(None, cx));
        window.render_frame(cx);
        assert!(bars(window).is_empty());
    });
}

#[gpui::test]
fn line_number_drag_reports_its_range_on_release(cx: &mut TestAppContext) {
    let (host, cx) = setup(cx);
    cx.update(|window, cx| {
        let from = window.find(("line", 0usize)).bounds().center();
        let to = window.find(("line", 2usize)).bounds().center();
        window.drag(from, to, cx);
        assert_eq!(*host.read(cx).annotations.borrow(), [range(1, 3)]);
    });
}
