use gpui::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, ScrollDelta,
    ScrollEvent, Styled as _, TestAppContext, VisualTestContext, Window, div, point, px,
};

use super::{Diff, DiffState, document::fixture};
use gpui::TouchPhase;
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
