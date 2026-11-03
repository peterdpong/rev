use std::{cell::Cell, rc::Rc};
use super::*;
use crate::theme::{CONTROL_RADIUS, SELECTED, SELECTED_BORDER, TEXT, UI_SPACING};
use gpui_kit::component::{Icon, IconName};

const HISTORY_LIMIT: usize = 100;
const SWIPE_DISTANCE: f32 = 100.;

#[derive(Clone)]
pub(super) struct Location {
    request: ReviewRequest,
    resolved_head: Option<String>,
    scope: String,
    file: Option<String>,
    position: Option<DiffScrollPosition>,
}

#[derive(Default)]
pub(super) struct Navigation {
    back: Vec<Location>,
    forward: Vec<Location>,
    request: Option<ReviewRequest>,
    pub(super) travel: Option<NavigationDirection>,
    pub(super) restore: Option<Location>,
    pub(super) diff_bounds: Rc<Cell<Bounds<Pixels>>>,
    swipe: Swipe,
}

#[derive(Default)]
struct Swipe {
    active: bool,
    claimed: bool,
    x: f32,
    y: f32,
    blocked_back: bool,
    blocked_forward: bool,
}

impl Swipe {
    // A fresh touch phase is required. Momentum and mouse-wheel ticks cannot
    // start navigation; direction and the initial scroll boundary stay locked.
    fn update(&mut self, event: &ScrollWheelEvent) -> (bool, Option<NavigationDirection>) {
        if !self.active {
            return (false, None);
        }
        if event.touch_phase == TouchPhase::Cancelled {
            let claimed = self.claimed;
            *self = Self::default();
            return (claimed, None);
        }
        let ScrollDelta::Pixels(delta) = event.delta else {
            *self = Self::default();
            return (false, None);
        };
        self.x += f32::from(delta.x);
        self.y += f32::from(delta.y).abs();
        if !self.claimed && (self.x.abs() > 8. || self.y > 8.) {
            if self.x.abs() < self.y * 2.
                || (self.x > 0. && self.blocked_back)
                || (self.x < 0. && self.blocked_forward)
            {
                *self = Self::default();
                return (false, None);
            }
            self.claimed = true;
        }
        let claimed = self.claimed;
        if event.touch_phase == TouchPhase::Ended {
            let direction =
                (claimed && self.x.abs() >= SWIPE_DISTANCE && self.x.abs() > self.y * 2.)
                    .then_some(if self.x > 0. {
                        NavigationDirection::Back
                    } else {
                        NavigationDirection::Forward
                    });
            *self = Self::default();
            (claimed, direction)
        } else {
            (claimed, None)
        }
    }
}

impl Navigation {
    fn push(stack: &mut Vec<Location>, location: Location) {
        stack.push(location);
        if stack.len() > HISTORY_LIMIT {
            stack.remove(0);
        }
    }

    fn record(&mut self, location: Location) {
        Self::push(&mut self.back, location);
        self.forward.clear();
    }

    pub(super) fn loaded(
        &mut self,
        previous: Option<Location>,
        request: ReviewRequest,
        scope: &str,
        travel: Option<NavigationDirection>,
    ) {
        self.restore = None;
        self.swipe = Swipe::default();
        if let Some(direction) = travel {
            let (from, to) = match direction {
                NavigationDirection::Back => (&mut self.back, &mut self.forward),
                NavigationDirection::Forward => (&mut self.forward, &mut self.back),
            };
            self.restore = from.pop();
            if let Some(previous) = previous {
                Self::push(to, previous);
            }
        } else if let Some(previous) = previous
            && (previous.scope != scope || previous.request != request)
        {
            self.record(previous);
        }
        self.request = Some(request);
    }
}

impl Rev {
    pub(super) fn navigation_location(&self, cx: &App) -> Option<Location> {
        let review = self.review.as_ref()?;
        Some(Location {
            request: self
                .navigation
                .request
                .clone()
                .unwrap_or_else(|| review.request.clone()),
            resolved_head: if review.request.mode.as_deref() == Some("working") {
                Some(if review.branch == "detached HEAD" {
                    review.branch_head_sha.clone()
                } else {
                    review.branch.clone()
                })
            } else {
                review.request.head.clone()
            },
            scope: review.scope_key.clone(),
            file: self.file.clone(),
            position: self.diff_state(cx).read(cx).scroll_position(),
        })
    }

    pub(super) fn record_file_navigation(&mut self, previous: Option<Location>, cx: &App) {
        if !self.loading
            && !self.patch_loading
            && self.navigation.restore.is_none()
            && let Some(location) = previous
            && let Some(current) = self.navigation_location(cx)
            && (location.file != current.file || location.position != current.position)
        {
            self.navigation.record(location);
        }
    }

    pub(super) fn can_navigate(&self, direction: NavigationDirection) -> bool {
        if self.loading || self.patch_loading || self.workspace_busy || self.submitting {
            return false;
        }
        match direction {
            NavigationDirection::Back => !self.navigation.back.is_empty(),
            NavigationDirection::Forward => !self.navigation.forward.is_empty(),
        }
    }

    pub(super) fn navigate_history(
        &mut self,
        direction: NavigationDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigation.swipe = Swipe::default();
        if !self.can_navigate(direction) {
            return;
        }
        let mut target = match direction {
            NavigationDirection::Back => self.navigation.back.last(),
            NavigationDirection::Forward => self.navigation.forward.last(),
        }
        .unwrap()
        .clone();
        // Current follows the checkout. History must still reach the branch
        // that was reviewed if the checkout has since moved elsewhere.
        if target.request.head.as_deref() == Some("HEAD")
            && self
                .workspace
                .projects
                .iter()
                .find(|project| project.repo == target.request.repo)
                .and_then(|project| {
                    project
                        .tabs
                        .iter()
                        .find(|tab| tab.kind == crate::workspace_state::TabKind::Current)
                })
                .is_none_or(|tab| Some(&tab.label) != target.resolved_head.as_ref())
        {
            target.request.head = target.resolved_head.clone();
            target.request.mode = Some("branch".into());
        }
        let previous = self.navigation_location(cx);
        self.close_modal(window, cx);
        if self
            .review
            .as_ref()
            .is_some_and(|review| review.scope_key == target.scope)
            && self.navigation.request.as_ref() == Some(&target.request)
        {
            self.navigation.loaded(
                previous,
                target.request.clone(),
                &target.scope,
                Some(direction),
            );
            self.restore_navigation(window, cx);
        } else {
            self.navigation.travel = Some(direction);
            self.load_review(target.request, target.file, window, cx);
        }
        cx.notify();
    }

    pub(super) fn restore_navigation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(location) = self.navigation.restore.take() {
            if let Some(path) = location.file {
                self.reveal_file(path, window, cx);
            }
            if let Some(position) = location.position {
                self.diff_state(cx)
                    .update(cx, |diff, cx| diff.restore_scroll_position(&position, cx));
            }
        }
    }

    fn navigation_scroll(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.modal.is_some()
            || self.file_sheet.open
            || self.diff.read(cx).is_selecting(cx)
            || self.loading
            || self.patch_loading
            || self.submitting
            || self.workspace_busy
        {
            self.navigation.swipe = Swipe::default();
            return;
        }
        if event.touch_phase == TouchPhase::Started {
            let in_diff = self.navigation.diff_bounds.get().contains(&event.position);
            let state = self.diff_state(cx);
            let diff = state.read(cx);
            self.navigation.swipe = Swipe {
                active: event.modifiers == Modifiers::default(),
                blocked_back: !self.can_navigate(NavigationDirection::Back)
                    || (in_diff && !self.diff.read(cx).wraps_lines() && diff.can_scroll_horizontally(1.)),
                blocked_forward: !self.can_navigate(NavigationDirection::Forward)
                    || (in_diff && !self.diff.read(cx).wraps_lines() && diff.can_scroll_horizontally(-1.)),
                ..Default::default()
            };
        }
        let (claimed, direction) = self.navigation.swipe.update(event);
        if claimed {
            cx.stop_propagation();
        }
        if let Some(direction) = direction {
            self.navigate_history(direction, window, cx);
        }
        cx.notify();
    }

    pub(super) fn navigation_hint(&self) -> Option<AnyElement> {
        let swipe = &self.navigation.swipe;
        if !swipe.claimed {
            return None;
        }
        let progress = (swipe.x.abs() / SWIPE_DISTANCE).min(1.);
        Some(
            div()
                .absolute()
                .top(px(UI_SPACING))
                .size(px(30.))
                .rounded(px(CONTROL_RADIUS))
                .bg(rgb(SELECTED))
                .border_1()
                .border_color(rgb(SELECTED_BORDER))
                .shadow_sm()
                .text_color(rgb(TEXT))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(20.))
                .opacity(0.4 + progress * 0.6)
                .when(swipe.x > 0., |d| {
                    d.left(px(UI_SPACING + UI_SPACING * 2. * progress))
                })
                .when(swipe.x < 0., |d| {
                    d.right(px(UI_SPACING + UI_SPACING * 2. * progress))
                })
                .child(
                    Icon::new(if swipe.x > 0. {
                        IconName::ChevronLeft
                    } else {
                        IconName::ChevronRight
                    })
                    .size(px(14.)),
                )
                .into_any_element(),
        )
    }

    pub(super) fn navigation_events(&self, cx: &Context<Self>) -> impl IntoElement {
        let app = cx.entity().downgrade();
        canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                let app = app.clone();
                window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        let _ =
                            app.update(cx, |this, cx| this.navigation_scroll(event, window, cx));
                    }
                });
            },
        )
        .absolute()
        .size_full()
    }
}

#[cfg(test)]
mod tests {
    use super::{HISTORY_LIMIT, Location, Navigation, Swipe};
    use crate::model::ReviewRequest;
    use gpui_kit::{NavigationDirection, ScrollDelta, ScrollWheelEvent, TouchPhase, point, px};

    fn event(phase: TouchPhase, x: f32, y: f32) -> ScrollWheelEvent {
        ScrollWheelEvent {
            touch_phase: phase,
            delta: ScrollDelta::Pixels(point(px(x), px(y))),
            ..Default::default()
        }
    }

    #[test]
    fn swipe_commits_once_on_release_and_ignores_momentum() {
        let mut swipe = Swipe {
            active: true,
            ..Default::default()
        };
        assert_eq!(
            swipe.update(&event(TouchPhase::Moved, 120., 2.)),
            (true, None)
        );
        assert_eq!(
            swipe.update(&event(TouchPhase::Ended, 0., 0.)),
            (true, Some(NavigationDirection::Back))
        );
        assert_eq!(
            swipe.update(&event(TouchPhase::Moved, 500., 0.)),
            (false, None)
        );
        assert_eq!(
            swipe.update(&event(TouchPhase::Ended, 0., 0.)),
            (false, None)
        );
    }

    #[test]
    fn short_reversed_cancelled_and_vertical_gestures_do_not_navigate() {
        for events in [
            vec![
                event(TouchPhase::Moved, 40., 0.),
                event(TouchPhase::Ended, 0., 0.),
            ],
            vec![
                event(TouchPhase::Moved, 140., 0.),
                event(TouchPhase::Moved, -110., 0.),
                event(TouchPhase::Ended, 0., 0.),
            ],
            vec![
                event(TouchPhase::Moved, 140., 0.),
                event(TouchPhase::Cancelled, 0., 0.),
            ],
            vec![
                event(TouchPhase::Moved, 10., 20.),
                event(TouchPhase::Moved, 200., 0.),
                event(TouchPhase::Ended, 0., 0.),
            ],
        ] {
            let mut swipe = Swipe {
                active: true,
                ..Default::default()
            };
            for event in events {
                assert!(swipe.update(&event).1.is_none());
            }
        }
    }

    #[test]
    fn code_panning_owns_the_whole_gesture_even_after_reaching_an_edge() {
        let mut swipe = Swipe {
            active: true,
            blocked_forward: true,
            ..Default::default()
        };
        assert_eq!(
            swipe.update(&event(TouchPhase::Moved, -20., 0.)),
            (false, None)
        );
        assert_eq!(
            swipe.update(&event(TouchPhase::Moved, -300., 0.)),
            (false, None)
        );
        assert_eq!(
            swipe.update(&event(TouchPhase::Ended, 0., 0.)),
            (false, None)
        );
        let mut swipe = Swipe {
            active: true,
            ..Default::default()
        };
        swipe.update(&event(TouchPhase::Moved, -120., 0.));
        assert_eq!(
            swipe.update(&event(TouchPhase::Ended, 0., 0.)).1,
            Some(NavigationDirection::Forward)
        );
    }

    fn location(head: &str) -> Location {
        Location {
            request: ReviewRequest {
                head: Some(head.into()),
                ..Default::default()
            },
            scope: head.into(),
            resolved_head: Some(head.into()),
            file: None,
            position: None,
        }
    }

    #[test]
    fn history_replays_without_duplicates_refresh_is_ignored_and_new_visits_clear_forward() {
        let mut nav = Navigation::default();
        let a = location("a");
        let b = location("b");
        let c = location("c");
        nav.loaded(None, a.request.clone(), &a.scope, None);
        nav.loaded(Some(a.clone()), b.request.clone(), &b.scope, None);
        nav.loaded(Some(b.clone()), b.request.clone(), &b.scope, None);
        assert_eq!(nav.back.len(), 1);
        nav.loaded(
            Some(b),
            a.request.clone(),
            &a.scope,
            Some(NavigationDirection::Back),
        );
        assert!(nav.back.is_empty());
        assert_eq!(nav.forward.len(), 1);
        nav.loaded(Some(a), c.request.clone(), &c.scope, None);
        assert!(nav.forward.is_empty());
        for _ in 0..150 {
            nav.record(c.clone());
        }
        assert_eq!(nav.back.len(), HISTORY_LIMIT);
    }
}
