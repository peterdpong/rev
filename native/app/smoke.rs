//! An offscreen Metal render of the production view, with real Git fixture data.
//! This does not automate or capture the user's desktop.
use super::*;
use gpui_kit::{HeadlessAppContext, test::TestWindowExt};
use std::sync::Arc;

fn pointer_down(window: &mut Window, position: Point<Pixels>, cx: &mut App) {
    window.simulate_mouse_move(position, cx);
    window.render_frame(cx);
    window.dispatch_event(
        PlatformInput::MouseDown(MouseDownEvent {
            button: MouseButton::Left,
            position,
            click_count: 1,
            ..Default::default()
        }),
        cx,
    );
}

fn pointer_move(window: &mut Window, position: Point<Pixels>, cx: &mut App) {
    window.dispatch_event(
        PlatformInput::MouseMove(MouseMoveEvent {
            position,
            pressed_button: Some(MouseButton::Left),
            ..Default::default()
        }),
        cx,
    );
    window.render_frame(cx);
}

fn pointer_up(window: &mut Window, position: Point<Pixels>, cx: &mut App) {
    window.dispatch_event(
        PlatformInput::MouseUp(MouseUpEvent {
            button: MouseButton::Left,
            position,
            click_count: 1,
            ..Default::default()
        }),
        cx,
    );
}

fn line_point_on_side(window: &mut Window, line: usize, gutter: bool, old: bool) -> Point<Pixels> {
    let side = if old { "old" } else { "new" };
    let gutter_id = if old { "old-gutter" } else { "new-gutter" };
    let bounds = if gutter {
        window
            .within((side, line - 1))
            .within(gutter_id)
            .find(("line", line - 1))
            .bounds()
    } else {
        window.within((side, line - 1)).find("source").bounds()
    };
    point(
        bounds.left()
            + if gutter {
                bounds.size.width / 2.
            } else {
                px(40.)
            },
        bounds.center().y,
    )
}

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
    // Drive real pointer gestures through production handlers, without Shift.
    for split in [false, true] {
        if split {
            cx.update_window(handle, |_, window, cx| window.click("diff-mode", cx))?;
        }
        for old in [false, true] {
            if old && !split {
                continue;
            }
            for gutter in [false, true] {
                for (start, end) in if old {
                    [(1, 3), (3, 1)]
                } else {
                    [(4, 6), (6, 4)]
                } {
                    // Leave a previous range in place: replacing it must not
                    // insert/remove an action row under the moving pointer.
                    cx.update_window(handle, |_, window, cx| {
                        window.render_frame(cx);
                        let start = line_point_on_side(window, start, gutter, old);
                        pointer_down(window, start, cx);
                    })?;
                    let target = cx.update_window(handle, |_, window, cx| {
                        let origin = line_point_on_side(window, start, gutter, old);
                        let target = line_point_on_side(window, end, gutter, old);
                        pointer_move(window, target, cx);
                        assert_eq!(
                            line_point_on_side(window, start, gutter, old),
                            origin,
                            "dragging must not move source rows"
                        );
                        target
                    })?;
                    let copied = cx.update_window(handle, |_, window, cx| {
                        let copied = app.read(cx).diff.read(cx).selected_text(cx);
                        pointer_up(window, target, cx);
                        copied
                    })?;
                    cx.update_window(handle, |_, window, cx| {
                        let expected = (start.min(end), start.max(end), if old { "deletions" } else { "additions" });
                        let selection = app.read(cx).selection.as_ref().unwrap_or_else(|| panic!("missing drag selection: split={split}, old={old}, gutter={gutter}, {start}–{end}"));
                        assert_eq!((selection.start, selection.end, selection.side.as_str()), expected, "split={split}, old={old}, gutter={gutter}, {start}–{end}");
                        assert_eq!(app.read(cx).diff.read(cx).selected_text(cx), copied, "the action menu must preserve native copy text");
                        window.render_frame(cx);
                        window.click(if gutter { "change-selection" } else { "ask-selection" }, cx);
                        let modal = app.read(cx).modal.as_ref().unwrap().clone();
                        let selection = modal.selection.as_ref().unwrap();
                        assert_eq!((selection.start, selection.end, selection.side.as_str()), expected, "split={split}, old={old}, gutter={gutter}, {start}–{end}");
                        window.click("close-modal", cx);
                        // Restore the previous range through the real gutter.
                        let previous = line_point_on_side(window, end, true, old);
                        pointer_down(window, previous, cx);
                        pointer_up(window, previous, cx);
                    })?;
                }
            }
        }
        cx.update_window(handle, |_, window, cx| {
            app.update(cx, |app, cx| app.close_modal(window, cx));
        })?;
    }
    cx.update_window(handle, |_, window, cx| window.click("diff-mode", cx))?;
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
        "Native UI checks passed: forward/reverse code and gutter drags, original/modified split sides, stable selection layout, general ask, change draft, modal Escape, split view, wrap, feedback, file filter."
    );
    Ok(())
}
