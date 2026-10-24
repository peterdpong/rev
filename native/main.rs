extern crate gpui_kit as gpui;
mod app;
mod backend;
mod model;
use app::*;
use gpui_kit::*;
use model::{CommitRange, ReviewRequest};

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
    let mut commit = None;
    let mut from = None;
    let mut to = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--repo" | "--base" | "--head" | "--commit" | "--from" | "--to" => {
                let flag = args[index].clone();
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| anyhow::anyhow!("Missing value for {flag}"))?
                    .clone();
                match flag.as_str() {
                    "--repo" => request.repo = value,
                    "--base" => request.base = Some(value),
                    "--commit" => commit = Some(value),
                    "--from" => from = Some(value),
                    "--to" => to = Some(value),
                    _ => request.head = Some(value),
                }
            }
            "--working" => request.mode = Some("working".into()),
            "--help" | "-h" => {
                println!(
                    "Patch\n\n  patch open [repo] [--base main] [--head feature] [--working]\n  patch open [repo] --commit <ref> | --from <oldest> --to <newest>\n  patch diff | commits | ask | comment | comments | apply | fetch | pr | doctor | demo\n\nRun patch help for CLI options."
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
    anyhow::ensure!(
        commit.is_none() || (from.is_none() && to.is_none()),
        "Use --commit or --from/--to, not both."
    );
    anyhow::ensure!(
        from.is_some() == to.is_some(),
        "Use --from and --to together."
    );
    request.commit_range = commit
        .map(|sha: String| CommitRange {
            start: sha.clone(),
            end: sha,
        })
        .or_else(|| from.zip(to).map(|(start, end)| CommitRange { start, end }));
    anyhow::ensure!(
        request.commit_range.is_none() || request.mode.as_deref() != Some("working"),
        "Commit selections cannot include the working tree."
    );
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
            app::bind_shortcuts(cx);
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
                    traffic_light_position: Some(point(px(16.), px(15.))),
                }),
                app_owns_titlebar_drag: true,
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
