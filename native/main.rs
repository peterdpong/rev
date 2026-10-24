extern crate gpui_kit as gpui;
mod app;
mod backend;
mod model;
use app::*;
use gpui_kit::*;
use model::ReviewRequest;

fn main() -> anyhow::Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args
        .first()
        .is_some_and(|arg| arg != "open" && !arg.starts_with('-'))
    {
        let status = std::process::Command::new(backend::node_binary())
            .arg(backend::resource_dir().join("cli.mjs"))
            .env("PATH", backend::host_path())
            .env("PATCH_DESKTOP_BINARY", std::env::current_exe()?)
            .args(args)
            .status()?;
        std::process::exit(status.code().unwrap_or(1));
    }
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
                println!(
                    "Patch\n\n  patch open [repo] [--base main] [--head feature] [--working]\n  patch diff | ask | comment | comments | apply | fetch | clone | pr | doctor | demo\n\nRun patch help for CLI options."
                );
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
            gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, None, cx);
            cx.bind_keys([
                KeyBinding::new("cmd-q", Quit, None),
                KeyBinding::new("cmd-o", OpenProject, Some("Patch")),
                KeyBinding::new("cmd-r", Refresh, Some("Patch")),
                KeyBinding::new("cmd-k", AskReview, Some("Patch")),
                KeyBinding::new("alt-down", NextFile, Some("Patch")),
                KeyBinding::new("alt-up", PreviousFile, Some("Patch")),
                KeyBinding::new("cmd-shift-v", MarkViewed, Some("Patch")),
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
                    title: Some("Patch".into()),
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
                cx.new(|cx| Patch::new(backend, initial, window, cx))
            })
            .expect("open Patch window");
            cx.activate(true);
        });
    Ok(())
}
