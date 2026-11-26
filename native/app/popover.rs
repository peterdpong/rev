use super::controls::*;
use crate::theme::*;
use gpui_kit::base::{Popover, Popup, motion};
use gpui_kit::prelude::*;
use gpui_kit::*;

pub(super) const POPOVER_RADIUS: f32 = 8.;
const POPOVER_RISE: f32 = 6.;

pub(super) fn popover_surface() -> Div {
    column()
        .bg(rgb(SURFACE))
        .border_1()
        .border_color(rgb(CONTROL_BORDER))
        .rounded(px(POPOVER_RADIUS))
        .shadow_lg()
}

#[derive(Clone, Copy)]
pub(super) struct PopoverMotion {
    progress: f32,
    visible: bool,
}

impl PopoverMotion {
    pub(super) fn new(
        id: impl Into<SharedString>,
        open: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let sample = motion::Presence::new(id.into(), open)
            .transition(motion_transition())
            .sample(window, cx);

        Self {
            progress: sample.progress,
            visible: sample.should_render() && (open || sample.progress > 0.),
        }
    }

    pub(super) fn visible(&self) -> bool {
        self.visible
    }

    pub(super) fn apply(&self, content: impl IntoElement) -> Div {
        self.surface(content.into_any_element(), true)
    }

    pub(super) fn surface(&self, content: AnyElement, interactive: bool) -> Div {
        div()
            .relative()
            .top(px(-POPOVER_RISE * (1. - self.progress)))
            .opacity(self.progress)
            .child(content)
            .when(!interactive, |d| {
                d.child(
                    div()
                        .id("popover-exit-shield")
                        .absolute()
                        .inset_0()
                        .occlude(),
                )
            })
    }
}

pub(super) struct AnimatedPopover {
    id: SharedString,
    open: bool,
    motion: PopoverMotion,
}

impl AnimatedPopover {
    pub(super) fn new(
        id: impl Into<SharedString>,
        open: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let id = id.into();
        let motion = PopoverMotion::new(format!("{id}-motion"), open, window, cx);

        Self { id, open, motion }
    }

    pub(super) fn visible(&self) -> bool {
        self.motion.visible()
    }

    pub(super) fn open(&self) -> bool {
        self.open
    }

    pub(super) fn render(self, popover: Popover, content: Option<impl IntoElement>) -> AnyElement {
        let Self { id, open, motion } = self;
        let content = content
            .filter(|_| motion.visible())
            .map(IntoElement::into_any_element);

        let (live, closing) = if open {
            (content, None)
        } else {
            (None, content)
        };

        let popover = popover
            .anchor(Anchor::TopLeft)
            .offset(px(UI_SPACING))
            .open(open)
            .when_some(live, |popover, content| {
                popover.content(move |_, _, _| motion.surface(content, true))
            });

        Popup::new(SharedString::from(format!("{id}-closing")), popover)
            .flex_shrink_0()
            .min_w_0()
            .anchor(Anchor::TopLeft)
            .offset(px(UI_SPACING))
            .when_some(closing, |popup, content| {
                popup.content(motion.surface(content, false))
            })
            .into_any_element()
    }
}
