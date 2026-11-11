extern crate gpui_kit as gpui;
mod diff;
rust_i18n::i18n!("native/diff/locales", fallback = "en");
mod app;
mod backend;
mod model;
mod client_state;
mod workspace_state;
mod theme;
use app::*;
use gpui_kit::*;
use model::ReviewRequest;

fn initial_review() -> anyhow::Result<Option<ReviewRequest>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.is_empty() {
        return Ok(None);
    }
    let mut request = ReviewRequest::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--repo" | "--base" | "--head" => {
                let flag = &args[i];
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| anyhow::anyhow!("Missing value for {flag}"))?
                    .clone();
                match flag.as_str() {
                    "--repo" => request.repo = value,
                    "--base" => request.base = Some(value),
                    _ => request.head = Some(value),
                }
            }
            "--working" => request.mode = Some("working".into()),
            other => anyhow::bail!(
                "Unknown option: {other}. Use --repo <folder> [--base main] [--head branch] [--working]."
            ),
        }
        i += 1;
    }
    anyhow::ensure!(!request.repo.is_empty(), "Pass --repo <folder>.");
    Ok(Some(request))
}

fn main() -> anyhow::Result<()> {
    let initial = initial_review()?;
    let backend = backend::Backend::start();
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            diff::init(cx);
            crate::theme::init(cx);
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
                    traffic_light_position: Some(point(px(12.), px(12.))),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1320.), px(860.)),
                    cx,
                ))),
                window_min_size: Some(size(px(940.), px(600.))),
                ..gpui_kit::component::TitleBar::window_options()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| Rev::new(backend, initial, window, cx))
            })
            .expect("open Rev window");
            cx.activate(true);
        });
    Ok(())
}
