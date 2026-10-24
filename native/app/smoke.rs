//! An offscreen Metal render of the production view, with real Git fixture data.
//! This does not automate or capture the user's desktop.
use super::*;
use gpui_kit::{HeadlessAppContext, test::TestWindowExt};
use std::sync::Arc;

pub fn run() -> anyhow::Result<()> {
    let repo = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("Pass a sample repository"))?;
    let output = std::env::args()
        .nth(2)
        .unwrap_or_else(|| ".test-artifacts".into());
    std::fs::create_dir_all(&output)?;
    let backend = Backend::start().map_err(anyhow::Error::msg)?;
    let request = json!({ "repo": repo, "base": "main", "head": "HEAD", "remember": true });
    let value = backend
        .call("review", request)
        .map_err(anyhow::Error::msg)?;
    let mut review: Review = decode(value["review"].clone()).map_err(anyhow::Error::msg)?;
    let state: ReviewState = decode(value["state"].clone()).map_err(anyhow::Error::msg)?;
    let workspace: Workspace = decode(value["workspace"].clone()).map_err(anyhow::Error::msg)?;
    if let Some(tab) = workspace.active() {
        review.request = tab.request.clone();
    }
    let path = review
        .files
        .iter()
        .find(|f| f.path.ends_with("retry.ts"))
        .or(review.files.first())
        .unwrap()
        .path
        .clone();
    let patch: FilePatch = decode(
        backend
            .call("patch", json!({"review": review.request, "path": path}))
            .map_err(anyhow::Error::msg)?,
    )
    .map_err(anyhow::Error::msg)?;
    backend.close();
    let mut cx = HeadlessAppContext::with_platform(
        gpui_kit::platform::current_platform(true).text_system(),
        Arc::new(gpui_kit::assets::Assets),
        gpui_kit::platform::current_headless_renderer,
    );
    cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, None, cx);
        cx.bind_keys([KeyBinding::new("escape", CloseModal, Some("PatchModal"))]);
    });
    let (handle, app) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Default::default(),
                    size: size(px(1320.), px(860.)),
                })),
                focus: false,
                show: false,
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    let mut app = Patch::new(Err("fixture mode".into()), None, window, cx);
                    app.error = None;
                    app.review = Some(review);
                    app.workspace = workspace;
                    app.state = state;
                    app.file = Some(path.clone());
                    app.patch = patch;
                    app.diff.update(cx, |diff, cx| {
                        diff.set_files(DiffFile::parse(&app.patch.patch).unwrap(), cx)
                    });
                    app.providers = vec![
                        Provider {
                            id: "codex".into(),
                            available: true,
                        },
                        Provider {
                            id: "claude".into(),
                            available: false,
                        },
                    ];
                    app
                })
            },
        )
    })?;

    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("ask-review").visible());
        assert!(window.find("changes").visible());
        assert!(window.find("filter-files").bounds().size.width > px(150.));
        window
            .within(("new", 3usize))
            .within("new-gutter")
            .click(("line", 3usize), cx);
        window.press("shift-down", cx);
        window.press("shift-down", cx);
        window.render_frame(cx);
    })?;
    cx.capture_screenshot(handle)?
        .save(format!("{output}/gpui-review.png"))?;

    cx.update_window(handle, |_, window, cx| {
        window.click("ask-selection", cx);
        app.update(cx, |app, _| {
            assert_eq!(app.modal.as_ref().unwrap().kind as u8, ModalKind::Ask as u8);
            let selection = app.modal.as_ref().unwrap().selection.as_ref().unwrap();
            assert_eq!((selection.start, selection.end), (4, 6));
        });
    })?;
    cx.capture_screenshot(handle)?
        .save(format!("{output}/gpui-ask.png"))?;
    cx.update_window(handle, |_, window, cx| {
        window.press("escape", cx);
        assert!(app.read(cx).modal.is_none());
        window
            .within(("new", 3usize))
            .within("new-gutter")
            .click(("line", 3usize), cx);
    })?;
    cx.update_window(handle, |_, window, cx| {
        assert!(app.read(cx).selection.is_some());
        window.click("ask-review", cx);
        assert!(
            app.read(cx).modal.as_ref().unwrap().selection.is_none(),
            "top-level ask must ignore the highlighted range"
        );
        window.click("close-modal", cx);
        window
            .within(("new", 3usize))
            .within("new-gutter")
            .click(("line", 3usize), cx);
    })?;
    cx.update_window(handle, |_, window, cx| {
        window.click("change-selection", cx);
        assert!(app.read(cx).modal.as_ref().unwrap().selection.is_some());
        let bound = app.read(cx).modal.as_ref().unwrap().clone();
        app.update(cx, |app, cx| {
            app.prompt.update(cx, |input, cx| {
                input.set_value("Handle cancellation", window, cx)
            });
            app.close_modal(window, cx);
            app.open_modal(ModalKind::Change, bound.selection.clone(), None, window, cx);
            assert_eq!(app.prompt.read(cx).value().as_ref(), "Handle cancellation");
        });
        window.click("close-modal", cx);
        window.click("diff-mode", cx);
        assert_eq!(app.read(cx).diff.read(cx).mode(), DiffMode::Split);
        window.click("wrap", cx);
        assert!(app.read(cx).wrap);
        window.click("changes", cx);
        assert!(
            app.read(cx)
                .modal
                .as_ref()
                .is_some_and(|m| m.kind == ModalKind::Changes)
        );
        window.click("close-modal", cx);
        window.click("filter-files", cx);
        window.input("retry", cx);
        assert!(
            app.read(cx)
                .visible_files(cx)
                .iter()
                .all(|f| f.path.contains("retry"))
        );
    })?;
    cx.capture_screenshot(handle)?
        .save(format!("{output}/gpui-split.png"))?;
    println!(
        "Native UI checks passed: selection, general ask, change draft, modal Escape, split view, wrap, feedback, file filter."
    );
    Ok(())
}
