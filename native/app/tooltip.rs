use super::controls::{HEADER_PILL_HEIGHT, HEADER_PILL_LINE_HEIGHT, HEADER_PILL_PADDING};
use super::popover::POPOVER_RADIUS;
use crate::theme::*;
use gpui_kit::base::{
    ElementExt as _, Placement, Root, RootPlugin, TooltipOverlay, TooltipRequest,
};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::{cell::Cell, rc::Rc};

pub(super) fn init(cx: &mut App) {
    Root::register_plugin::<TooltipLayer>(cx, TooltipLayer::new);
}

struct TooltipLayer(Entity<TooltipOverlay>);

impl TooltipLayer {
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self(cx.new(|_| {
            TooltipOverlay::new().render_with(|view, _, _, _| {
                div()
                    .debug_selector(|| "tooltip".into())
                    .child(view)
                    .into_any_element()
            })
        }))
    }
}

impl Render for TooltipLayer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.0.clone()
    }
}

impl RootPlugin for TooltipLayer {}

fn overlay(window: &Window, cx: &App) -> Option<Entity<TooltipOverlay>> {
    let root = window.root::<Root>().flatten()?;
    let layer = root.read(cx).plugin::<TooltipLayer>()?;
    Some(layer.read(cx).0.clone())
}

fn tooltip(label: SharedString, window: &mut Window, cx: &mut App) -> AnyView {
    Tooltip::new(label)
        .font_family(".AppleSystemUIFont")
        .text_size(px(12.))
        .line_height(px(HEADER_PILL_LINE_HEIGHT))
        .text_color(rgb(TEXT))
        .bg(rgb(SURFACE))
        .border_color(rgb(CONTROL_BORDER))
        .rounded(px(POPOVER_RADIUS))
        .m(px(UI_SPACING))
        .px(px(HEADER_PILL_PADDING))
        .py(px((HEADER_PILL_HEIGHT - HEADER_PILL_LINE_HEIGHT) / 2. - 1.))
        .build(window, cx)
}

pub(super) trait TooltipExt: StatefulInteractiveElement + ParentElement + Sized {
    fn with_tooltip(self, label: impl Into<SharedString>) -> Self {
        let label = label.into();
        let bounds = Rc::new(Cell::new(Bounds::default()));
        let trigger = bounds.clone();
        self.on_prepaint(move |bounds, _, _| trigger.set(bounds))
            .on_hover(move |hovered, window, cx| {
                let Some(overlay) = overlay(window, cx) else {
                    return;
                };
                overlay.update(cx, |overlay, cx| {
                    if *hovered {
                        let label = label.clone();
                        let request = TooltipRequest::new(bounds.get(), move |window, cx| {
                            tooltip(label.clone(), window, cx)
                        })
                        .with_placement(Placement::Bottom);
                        overlay.request_show(request, window, cx);
                    } else {
                        overlay.request_hide(window, cx);
                    }
                });
            })
            .on_mouse_down(MouseButton::Left, hide)
            .on_scroll(hide)
    }
}

impl<E: StatefulInteractiveElement + ParentElement> TooltipExt for E {}

fn hide<E>(_: &E, window: &mut Window, cx: &mut App) {
    if let Some(overlay) = overlay(window, cx) {
        overlay.update(cx, |overlay, cx| overlay.hide(cx));
    }
}

#[cfg(test)]
mod tests {
    use super::{HEADER_PILL_HEIGHT, Root, init, tooltip};
    use crate::app::controls::icon_button;
    use crate::theme::UI_SPACING;
    use gpui_kit::assets::IconName;
    use gpui_kit::{
        AnyView, AppContext as _, Context, IntoElement, Modifiers, ParentElement as _, Render,
        TestAppContext, VisualTestContext, Window, div, point, prelude::*, px,
    };
    use std::time::Duration;

    struct TooltipHost {
        tooltip: AnyView,
    }

    impl Render for TooltipHost {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().flex().items_start().child(
                div()
                    .debug_selector(|| "tooltip-host".into())
                    .child(self.tooltip.clone()),
            )
        }
    }

    #[gpui::test]
    fn tooltips_are_as_tall_as_a_header_pill(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::theme::init(cx);
        });
        let (_, cx) = cx.add_window_view(|window, cx| TooltipHost {
            tooltip: tooltip("Open repository".into(), window, cx),
        });
        cx.update(|window, cx| {
            window.refresh();
            _ = window.draw(cx);
        });
        let host = cx.debug_bounds("tooltip-host").unwrap();
        assert_eq!(host.size.height, px(HEADER_PILL_HEIGHT + UI_SPACING * 2.));
    }

    struct TriggerHost;

    impl Render for TriggerHost {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().pl(px(200.)).pt(px(40.)).child(
                icon_button("trigger", "Open repository", IconName::Copy)
                    .debug_selector(|| "trigger".into()),
            )
        }
    }

    #[gpui::test]
    fn tooltips_sit_under_their_trigger(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::theme::init(cx);
            init(cx);
        });
        let window = cx.add_window(|window, cx| {
            let view = cx.new(|_| TriggerHost);
            Root::new(view, window, cx)
        });
        let cx = &mut VisualTestContext::from_window(*window, cx);
        cx.run_until_parked();
        let trigger = cx.debug_bounds("trigger").unwrap();
        cx.simulate_mouse_move(
            trigger.origin + point(px(2.), px(2.)),
            None,
            Modifiers::default(),
        );
        cx.executor().advance_clock(Duration::from_millis(600));
        cx.run_until_parked();
        let tooltip = cx.debug_bounds("tooltip").expect("tooltip is shown");
        assert!(tooltip.top() <= trigger.bottom());
        assert!(tooltip.top() + px(UI_SPACING) > trigger.bottom());
        assert!((tooltip.center().x - trigger.center().x).abs() <= px(1.));
    }
}
