//! Shared, interruptible motion. GPUI settles these immediately for Reduce Motion
//! and requests frames only while a transition is running.
use gpui_kit::base::{Presence, Transition, transition};
use gpui_kit::component::ActiveTheme;
use gpui_kit::{App, ElementId, Window};

pub(super) const SIDEBAR_WIDTH: f32 = 225.;

pub(super) fn reveal(
    id: impl Into<ElementId>,
    expanded: bool,
    window: &mut Window,
    cx: &mut App,
) -> f32 {
    let motion = cx.theme().motion_tokens();
    let policy = Transition::new(motion.duration_normal).easing(motion.easing_move.clone());
    transition(
        id.into(),
        if expanded { 1_f32 } else { 0. },
        policy,
        window,
        cx,
    )
    .clamp(0., 1.)
}

pub(super) fn enter(id: impl Into<ElementId>, window: &mut Window, cx: &mut App) -> f32 {
    let motion = cx.theme().motion_tokens();
    let policy = Transition::new(motion.duration_fast).easing(motion.easing_enter.clone());
    Presence::new(id.into(), true)
        .transition(policy)
        .sample(window, cx)
        .progress
        .clamp(0., 1.)
}
