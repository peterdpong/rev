use super::controls::row;
use super::controls::{HEADER_PILL_HEIGHT, HEADER_PILL_LINE_HEIGHT, HEADER_PILL_PADDING};
use super::popover::POPOVER_RADIUS;
use crate::theme::*;
use gpui_kit::base::{
    ElementExt as _, Placement, Root, RootPlugin, TooltipOverlay, TooltipRequest,
};
use gpui_kit::component::{kbd::Kbd, tooltip::Tooltip};
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

const KEYCAP_SIZE: f32 = 16.;
const TOOLTIP_INSET: f32 = (HEADER_PILL_HEIGHT - HEADER_PILL_LINE_HEIGHT) / 2. - 1.;

fn tooltip(
    label: SharedString,
    shortcut: Option<Keystroke>,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    let has_shortcut = shortcut.is_some();
    Tooltip::element(move |_, _| {
        row()
            .gap(px(8.))
            .child(label.clone())
            .when_some(shortcut.as_ref(), |d, keystroke| {
                d.child(keycaps(keystroke))
            })
    })
    .font_family(".AppleSystemUIFont")
    .text_size(px(12.))
    .line_height(px(HEADER_PILL_LINE_HEIGHT))
    .text_color(rgb(TEXT))
    .bg(rgb(SURFACE))
    .border_color(rgb(CONTROL_BORDER))
    .rounded(px(POPOVER_RADIUS))
    .m(px(UI_SPACING))
    .pl(px(HEADER_PILL_PADDING))
    .pr(px(if has_shortcut {
        TOOLTIP_INSET
    } else {
        HEADER_PILL_PADDING
    }))
    .py(px(TOOLTIP_INSET))
    .build(window, cx)
}

fn shortcut_keys(keystroke: &Keystroke) -> Vec<SharedString> {
    let modifiers = &keystroke.modifiers;
    let mut keys: Vec<SharedString> = [
        (modifiers.control, "⌃"),
        (modifiers.alt, "⌥"),
        (modifiers.shift, "⇧"),
        (modifiers.platform, "⌘"),
    ]
    .into_iter()
    .filter(|(held, _)| *held)
    .map(|(_, symbol)| symbol.into())
    .collect();
    let key = Keystroke {
        modifiers: Modifiers::default(),
        key: keystroke.key.clone(),
        key_char: None,
    };
    keys.push(Kbd::format(&key).into());
    keys
}

fn keycaps(keystroke: &Keystroke) -> Div {
    row()
        .debug_selector(|| "tooltip-keycaps".into())
        .gap(px(2.))
        .flex_shrink_0()
        .children(shortcut_keys(keystroke).into_iter().map(keycap))
}

fn keycap(key: SharedString) -> Div {
    div()
        .h(px(KEYCAP_SIZE))
        .min_w(px(KEYCAP_SIZE))
        .px(px(3.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(POPOVER_RADIUS - TOOLTIP_INSET - 1.))
        .bg(rgb(HOVER))
        .border_1()
        .border_color(rgb(CONTROL_BORDER))
        .text_size(px(11.))
        .line_height(px(KEYCAP_SIZE - 2.))
        .text_color(rgb(MUTED))
        .child(key)
}

fn shortcut(action: &dyn Action, context: &str, window: &Window) -> Option<Keystroke> {
    let context = KeyContext::parse(context).ok()?;
    let binding = window.highest_precedence_binding_for_action_in_context(action, context)?;
    Some(binding.keystrokes().first()?.as_keystroke().clone())
}

type Shortcut = Rc<(Box<dyn Action>, &'static str)>;

pub(super) trait TooltipExt: StatefulInteractiveElement + ParentElement + Sized {
    fn with_tooltip(self, label: impl Into<SharedString>) -> Self {
        self.tooltip_with(label.into(), None)
    }

    fn with_shortcut_tooltip(
        self,
        label: impl Into<SharedString>,
        action: &dyn Action,
        context: &'static str,
    ) -> Self {
        self.tooltip_with(label.into(), Some(Rc::new((action.boxed_clone(), context))))
    }

    fn tooltip_with(self, label: SharedString, shortcut_action: Option<Shortcut>) -> Self {
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
                        let shortcut_action = shortcut_action.clone();
                        let request = TooltipRequest::new(bounds.get(), move |window, cx| {
                            let keystroke = shortcut_action
                                .as_ref()
                                .and_then(|entry| shortcut(entry.0.as_ref(), entry.1, window));
                            tooltip(label.clone(), keystroke, window, cx)
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
    use super::{HEADER_PILL_HEIGHT, Root, init, shortcut_keys, tooltip};
    use crate::app::controls::{icon_button, shortcut_icon_button};
    use crate::theme::UI_SPACING;
    use gpui_kit::assets::IconName;
    use gpui_kit::{
        AnyView, AppContext as _, Context, IntoElement, KeyBinding, Keystroke, Modifiers,
        ParentElement as _, Render, TestAppContext, VisualTestContext, Window, div, point,
        prelude::*, px,
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
            tooltip: tooltip("Open repository".into(), None, window, cx),
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

    #[test]
    fn shortcuts_split_into_one_keycap_per_key() {
        let keys = |source: &'static str| {
            shortcut_keys(&Keystroke::parse(source).unwrap())
                .into_iter()
                .map(|key| key.to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(keys("cmd-b"), ["⌘", "B"]);
        assert_eq!(keys("cmd-shift-v"), ["⇧", "⌘", "V"]);
        assert_eq!(keys("alt-down"), ["⌥", "↓"]);
        assert_eq!(keys("escape"), ["⎋"]);
    }

    struct ShortcutHost;

    impl Render for ShortcutHost {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .key_context("Rev")
                .pl(px(200.))
                .pt(px(40.))
                .child(
                    shortcut_icon_button(
                        "trigger",
                        "Refresh",
                        IconName::RefreshCw,
                        &crate::app::Refresh,
                        "Rev",
                    )
                    .debug_selector(|| "trigger".into()),
                )
        }
    }

    #[gpui::test]
    fn shortcut_tooltips_show_keycaps(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::theme::init(cx);
            init(cx);
            cx.bind_keys([KeyBinding::new("cmd-r", crate::app::Refresh, Some("Rev"))]);
        });
        let window = cx.add_window(|window, cx| {
            let view = cx.new(|_| ShortcutHost);
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
        let keycaps = cx
            .debug_bounds("tooltip-keycaps")
            .expect("keycaps are shown");
        assert!(tooltip.contains(&keycaps.origin));
    }
}
