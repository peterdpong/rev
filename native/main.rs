mod app;
use gpui_kit::*;

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, None, cx);
            cx.bind_keys([KeyBinding::new("cmd-q", app::Quit, None)]);
            cx.on_action(|_: &app::Quit, cx| cx.quit());
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            gpui_kit::open_window(app::window_options(cx), cx, |window, cx| {
                cx.new(|cx| app::Patch::new(window, cx))
            })
            .expect("open Patch window");
            cx.activate(true);
        });
}
