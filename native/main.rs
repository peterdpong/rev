extern crate gpui_kit as gpui;
mod diff;
rust_i18n::i18n!("native/diff/locales", fallback = "en");
mod app;
mod backend;
mod model;
use app::*;
use gpui_kit::*;
use model::ReviewRequest;

fn main() -> anyhow::Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let explicit_open = args.first().is_some_and(|a| a == "open");
    if explicit_open {
        args.remove(0);
    }
    let mut request = ReviewRequest::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--repo" | "--base" | "--head" => {
                let flag = args[index].clone();
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| anyhow::anyhow!("Missing value for {flag}"))?
                    .clone();
                match flag.as_str() {
                    "--repo" => request.repo = value,
                    "--base" => request.base = Some(value),
                    _ => request.head = Some(value),
                }
            }
            "--working" => request.mode = Some("working".into()),
            "--help" | "-h" => {
                println!("Rev\n\n  rev open [repo] [--base main] [--head feature] [--working]");
                return Ok(());
            }
            value if !value.starts_with('-') && request.repo.is_empty() => {
                request.repo = value.into()
            }
            other => anyhow::bail!("Unknown argument: {other}"),
        }
        index += 1;
    }
    if request.repo.is_empty() && (explicit_open || !args.is_empty()) {
        request.repo = std::env::current_dir()?.to_string_lossy().to_string();
    }
    let initial = (!request.repo.is_empty()).then_some(request);
    let backend = backend::Backend::start();
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            diff::init(cx);
            gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, None, cx);
            cx.bind_keys([
                KeyBinding::new("cmd-q", Quit, None),
                KeyBinding::new("cmd-o", OpenProject, Some("Rev")),
                KeyBinding::new("cmd-r", Refresh, Some("Rev")),
                KeyBinding::new("alt-down", NextFile, Some("Rev")),
                KeyBinding::new("alt-up", PreviousFile, Some("Rev")),
                KeyBinding::new("cmd-shift-v", MarkViewed, Some("Rev")),
                KeyBinding::new("escape", CloseModal, Some("PatchModal")),
                KeyBinding::new("cmd-enter", Submit, Some("PatchModal")),
            ]);
            cx.on_action(|_: &Quit, cx| cx.quit());
            if let Ok(service) = &backend {
                let service = service.clone();
                cx.on_app_quit(move |_| {
                    service.close();
                    async {}
                })
                .detach();
            }
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let options = WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("Rev".into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1320.), px(860.)),
                    cx,
                ))),
                window_min_size: Some(size(px(940.), px(600.))),
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| Rev::new(backend, initial, window, cx))
            })
            .expect("open Rev window");
            cx.activate(true);
        });
    Ok(())
}
