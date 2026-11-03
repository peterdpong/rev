use super::controls::*;
use crate::theme::*;
use gpui_kit::base::{Popover, Popup, motion};
use gpui_kit::prelude::*;
use gpui_kit::*;

const POPOVER_RADIUS: f32 = 8.;

pub(super) fn popover_surface() -> Div {
    column()
        .bg(rgb(SURFACE))
        .border_1()
        .border_color(rgb(CONTROL_BORDER))
        .rounded(px(POPOVER_RADIUS))
        .shadow_lg()
}

pub(super) struct AnimatedPopover {
    id: SharedString,
    open: bool,
    progress: f32,
    visible: bool,
}

impl AnimatedPopover {
    pub(super) fn new(
        id: impl Into<SharedString>,
        open: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let id = id.into();
        let sample = motion::Presence::new(SharedString::from(format!("{id}-motion")), open)
            .transition(motion_transition())
            .sample(window, cx);

        Self {
            id,
            open,
            progress: sample.progress,
            visible: sample.should_render() && (open || sample.progress > 0.),
        }
    }

    pub(super) fn visible(&self) -> bool {
        self.visible
    }

    pub(super) fn open(&self) -> bool {
        self.open
    }

    pub(super) fn render(self, popover: Popover, content: Option<impl IntoElement>) -> AnyElement {
        let Self {
            id, open, progress, ..
        } = self;
        let content = content
            .filter(|_| self.visible)
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
                popover.content(move |_, _, _| surface_motion(content, progress, true))
            });

        Popup::new(SharedString::from(format!("{id}-closing")), popover)
            .flex_shrink_0()
            .min_w_0()
            .anchor(Anchor::TopLeft)
            .offset(px(UI_SPACING))
            .when_some(closing, |popup, content| {
                popup.content(surface_motion(content, progress, false))
            })
            .into_any_element()
    }
}

fn surface_motion(content: AnyElement, progress: f32, interactive: bool) -> Div {
    div()
        .relative()
        .top(px(-6. * (1. - progress)))
        .opacity(progress)
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
