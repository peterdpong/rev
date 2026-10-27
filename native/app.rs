use gpui_kit::component::TitleBar;
use gpui_kit::prelude::*;
use gpui_kit::*;

gpui_kit::actions!(patch, [Quit]);

pub fn window_options(cx: &App) -> WindowOptions {
    WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some("Patch".into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(16.), px(15.))),
        }),
        app_owns_titlebar_drag: true,
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(1100.), px(760.)),
            cx,
        ))),
        window_min_size: Some(size(px(760.), px(500.))),
        ..Default::default()
    }
}

pub struct Patch {
    focus: FocusHandle,
}
impl Patch {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        Self { focus }
    }
}
impl Render for Patch {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("patch")
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x17191d))
            .text_color(rgb(0xe4e6eb))
            .text_size(px(13.))
            .font_family(".AppleSystemUIFont")
            .track_focus(&self.focus)
            .key_context("Patch")
            .child(
                TitleBar::new()
                    .h(px(44.))
                    .pl(px(80.))
                    .bg(rgb(0x131519))
                    .border_b_1()
                    .border_color(rgb(0x2b2e34))
                    .child("Patch"),
            )
            .child(
                div()
                    .id("review-surface")
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(0x929aa7))
                    .child("Open a review"),
            )
    }
}
