use gpui::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, ScrollDelta,
    ScrollEvent, Styled as _, TestAppContext, VisualTestContext, Window, div, point, px,
};

use super::{Diff, DiffState, document::fixture};
use gpui::{ListOffset, TouchPhase};
use std::time::Duration;

/// Every line changed, so the unified patch is two rows per line.
const LINES: usize = 300;

struct Host {
    state: Entity<DiffState>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(400.))
            .h(px(200.))
            .child(Diff::new(&self.state).size_full())
    }
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
}

fn long_patch(cx: &mut TestAppContext) -> (Entity<DiffState>, &mut VisualTestContext) {
    cx.update(super::test_init);
    let original = (0..LINES)
        .map(|ix| format!("old {ix}\n"))
        .collect::<String>();
    let modified = (0..LINES)
        .map(|ix| format!("new {ix}\n"))
        .collect::<String>();
    let state =
        cx.new(|cx| DiffState::new([fixture::modified("long.txt", &original, &modified)], cx));
    let (_, cx) = cx.add_window_view({
        let state = state.clone();
        move |_, _| Host { state }
    });
    // The body records its size on the first paint and lays out again.
    for _ in 0..3 {
        draw(cx);
    }
    (state, cx)
}

#[gpui::test]
fn the_whole_patch_keeps_its_height_after_the_first_layout(cx: &mut TestAppContext) {
    let (state, cx) = long_patch(cx);
    let (max, viewport) = cx.update(|_, cx| {
        let list = state.read(cx).list();
        (
            list.max_offset_for_scrollbar().y,
            list.viewport_bounds().size.height,
        )
    });
    // Rows past the viewport have not been rendered; without their hinted
    // height the patch would read as barely taller than one screen.
    assert!(
        max > viewport * 10.,
        "max offset {max:?} for a {viewport:?} viewport"
    );
}

#[gpui::test]
fn the_wheel_scrolls_past_the_rows_rendered_so_far(cx: &mut TestAppContext) {
    let (state, cx) = long_patch(cx);
    for _ in 0..50 {
        cx.simulate_event(ScrollEvent {
            position: point(px(100.), px(120.)),
            delta: ScrollDelta::Pixels(point(px(0.), px(-40.))),
            ..Default::default()
        });
        draw(cx);
    }
    let top = cx.update(|_, cx| state.read(cx).list().logical_scroll_top().item_ix);
    // 2000 px of wheel travel crosses far more than the first screen of rows.
    assert!(top > 40, "scrolled to row {top}");
}

fn swipe(cx: &mut VisualTestContext, dy: f32, packets: usize) {
    let wheel = |cx: &mut VisualTestContext, dy: f32, touch_phase| {
        cx.simulate_event(ScrollEvent {
            position: point(px(100.), px(120.)),
            delta: ScrollDelta::Pixels(point(px(0.), px(dy))),
            touch_phase,
            ..Default::default()
        });
    };
    wheel(cx, 0., TouchPhase::Started);
    for _ in 0..packets {
        wheel(cx, dy, TouchPhase::Moved);
        frame(cx);
    }
    wheel(cx, 0., TouchPhase::Ended);
}

fn frame(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(16));
    draw(cx);
}

#[gpui::test]
fn a_flick_released_mid_patch_scrolls_without_dragging_the_viewport(cx: &mut TestAppContext) {
    let (state, cx) = long_patch(cx);
    let list = cx.update(|_, cx| state.read(cx).list().clone());
    let top = list.viewport_bounds().top();
    swipe(cx, -40., 30);
    for _ in 0..60 {
        frame(cx);
    }

    swipe(cx, -60., 5);
    for _ in 0..6 {
        frame(cx);
        assert_eq!(list.viewport_bounds().top(), top, "the viewport stays put");
    }

    // A swipe that starts straight away scrolls the patch.
    let before = list.scroll_px_offset_for_scrollbar().y;
    swipe(cx, -30., 5);
    assert_eq!(list.scroll_px_offset_for_scrollbar().y, before - px(150.));
    assert_eq!(list.viewport_bounds().top(), top);
}

struct StickyHost {
    state: Entity<DiffState>,
}

impl Render for StickyHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(400.))
            .h(px(200.))
            .child(Diff::new(&self.state).sticky_header(true).size_full())
    }
}

#[gpui::test]
fn the_current_file_header_sticks_until_the_next_one_pushes_it_out(cx: &mut TestAppContext) {
    cx.update(super::test_init);
    let lines = |prefix: &str| {
        (0..40)
            .map(|ix| format!("{prefix} {ix}\n"))
            .collect::<String>()
    };
    let files = [
        fixture::modified("first.txt", &lines("old"), &lines("new")),
        fixture::modified("second.txt", &lines("old"), &lines("new")),
    ];
    let state = cx.new(|cx| DiffState::new(files, cx));
    let (_, cx) = cx.add_window_view({
        let state = state.clone();
        move |_, _| StickyHost { state }
    });
    for _ in 0..3 {
        draw(cx);
    }
    assert!(
        cx.debug_bounds("sticky-file-header").is_none(),
        "the first header is already at the top"
    );

    let scroll_to = |cx: &mut VisualTestContext, item_ix| {
        cx.update(|_, cx| {
            state.read(cx).list().scroll_to(ListOffset {
                item_ix,
                offset_in_item: px(0.),
            })
        });
        draw(cx);
    };
    let (second, viewport) = cx.update(|_, cx| {
        let state = state.read(cx);
        (state.header_row(1).unwrap(), state.list().viewport_bounds())
    });

    scroll_to(cx, 10);
    let sticky = cx.debug_bounds("sticky-file-header").unwrap();
    assert_eq!(sticky.top(), viewport.top());

    scroll_to(cx, second);
    cx.update(|_, cx| state.read(cx).list().scroll_by(px(-10.)));
    draw(cx);
    let next = cx
        .update(|_, cx| state.read(cx).list().bounds_for_item(second))
        .unwrap();
    assert!(next.top() > viewport.top() + px(5.));
    let sticky = cx.debug_bounds("sticky-file-header").unwrap();
    assert!(
        (sticky.bottom() - next.top()).abs() < px(0.5),
        "pushed up by the next header: {sticky:?} above {next:?}"
    );

    scroll_to(cx, second + 5);
    let sticky = cx.debug_bounds("sticky-file-header").unwrap();
    assert_eq!(sticky.top(), viewport.top());
    assert_eq!(
        cx.update(|_, cx| state.read(cx).visible_file().cloned()),
        Some("second.txt".into())
    );
}

#[gpui::test]
fn loading_a_file_above_the_viewport_keeps_the_reading_position(cx: &mut TestAppContext) {
    use super::{DiffFile, DiffLinePosition, DiffLineRange, DiffSide};
    let (state, cx) = long_patch(cx);
    cx.update(|_, cx| {
        state.update(cx, |state, cx| {
            let long = state.files()[0].clone();
            state.set_files([DiffFile::placeholder("large.txt", "Loading…"), long], cx);
            state.scroll_to_line(
                DiffLinePosition::new("long.txt", DiffSide::Modified, 200),
                cx,
            );
            state.set_selected_lines(
                Some(DiffLineRange::new("long.txt", DiffSide::Modified, 200, 201)),
                cx,
            );
        })
    });
    draw(cx);
    cx.update(|_, cx| {
        let list = state.read(cx).list();
        list.scroll_to(gpui::ListOffset {
            offset_in_item: px(7.),
            ..list.logical_scroll_top()
        });
    });
    draw(cx);
    let before = cx.update(|_, cx| state.read(cx).scroll_position().unwrap());
    cx.update(|_, cx| {
        state.update(cx, |state, cx| {
            let old = "before\n".repeat(100);
            let new = "after\n".repeat(100);
            assert!(state.replace_file(fixture::modified("large.txt", &old, &new), cx));
        })
    });
    draw(cx);
    cx.update(|_, cx| {
        let state = state.read(cx);
        let after = state.scroll_position().unwrap();
        assert_eq!(after, before);
        assert_eq!(state.selected_lines().unwrap().start(), 200);
    });
}
