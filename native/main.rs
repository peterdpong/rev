mod app;
mod backend;
mod client_state;
mod model;
mod navigation;
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
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, None, cx);
            cx.bind_keys([
                KeyBinding::new("cmd-q", app::Quit, None),
                KeyBinding::new("cmd-o", app::OpenFolder, Some("Rev")),
            ]);
            cx.on_action(|_: &app::Quit, cx| cx.quit());

            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            gpui_kit::open_window(app::window_options(cx), cx, |window, cx| {
                cx.new(|cx| match initial {
                    Some(request) => app::Rev::from_repo(request, window, cx),
                    None => app::Rev::reopen_last(window, cx),
                })
            })
            .expect("open Rev window");
            cx.activate(true);
        });
    Ok(())
}
