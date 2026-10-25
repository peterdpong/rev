use super::*;

pub(super) fn check(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<Patch>,
    output: &str,
) -> anyhow::Result<()> {
    let service = Backend::start().map_err(anyhow::Error::msg)?;
    let (original, paths, epoch, old_backend) = cx.update(|cx| {
        app.update(cx, |app, _| {
            (
                app.file.clone().unwrap(),
                app.review
                    .as_ref()
                    .unwrap()
                    .files
                    .iter()
                    .map(|file| file.path.clone())
                    .collect::<Vec<_>>(),
                app.file_epoch,
                app.backend.replace(service.clone()),
            )
        })
    });
    assert_eq!(paths.len(), 4);
    for (index, path) in paths.iter().enumerate() {
        cx.update_window(handle, |_, window, cx| {
            window.press("cmd-p", cx);
            window.click(("file", index), cx);
        })?;
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let app = app.read(cx);
            assert_eq!(app.file.as_ref(), Some(path));
            assert_eq!(app.diff.read(cx).files().len(), paths.len());
            assert_eq!(
                app.file_epoch, epoch,
                "jumping must not reload or replace the diff"
            );
            assert!(!app.patch_loading);
            let header = window
                .find(SharedString::from(format!("diff-heading-{path}")))
                .bounds();
            assert!(header.bottom() > window.find("diff-surface").bounds().top());
            let viewed = window
                .find(SharedString::from(format!("viewed-{path}")))
                .bounds();
            assert!(
                viewed.right() <= window.find("diff-surface").bounds().right(),
                "header controls remain on screen"
            );
        })?;
    }
    cx.update_window(handle, |_, window, cx| {
        window.press("cmd-p", cx);
        window.click(("file", 0usize), cx);
    })?;
    cx.run_until_parked();
    cx.capture_screenshot(handle)?
        .save(format!("{output}/gpui-continuous.png"))?;
    for index in [0usize, 1] {
        let path = &paths[index];
        cx.update_window(handle, |_, window, cx| {
            window.press("cmd-p", cx);
            window.click(("file", index), cx);
        })?;
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.click(SharedString::from(format!("viewed-{path}")), cx);
        })?;
        cx.run_until_parked();
        cx.update(|cx| {
            let app = app.read(cx);
            assert_eq!(app.state.viewed.get(path), Some(&app.patches[path].digest));
        });
        cx.update_window(handle, |_, window, cx| {
            window.click(SharedString::from(format!("viewed-{path}")), cx);
        })?;
        cx.run_until_parked();
    }
    // Start at README, then scroll into the next file without choosing it in the sidebar.
    cx.update_window(handle, |_, window, cx| {
        window.press("cmd-p", cx);
        window.click(("file", 0usize), cx);
    })?;
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let surface = window.find("diff-surface").bounds();
        let next = window
            .find(SharedString::from(format!("diff-heading-{}", paths[1])))
            .bounds();
        let position = surface.center();
        window.simulate_mouse_move(position, cx);
        window.render_frame(cx);
        window.dispatch_event(
            PlatformInput::ScrollWheel(ScrollWheelEvent {
                position,
                delta: ScrollDelta::Pixels(point(
                    px(0.),
                    -(next.bottom() - surface.top() + px(4.)),
                )),
                touch_phase: TouchPhase::Moved,
                ..Default::default()
            }),
            cx,
        );
    })?;
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(
            app.read(cx).file.as_ref(),
            Some(&paths[1]),
            "scrolling tracks the visible file"
        );
        // Selection context must come from the actual code even if navigation points elsewhere.
        app.update(cx, |app, cx| app.set_active_file(paths[0].clone(), cx));
        let start = line_point_in_file(window, 1, 4, true, false);
        let end = line_point_in_file(window, 1, 6, false, false);
        pointer_down(window, start, cx);
        pointer_move(window, end, cx);
        pointer_up(window, end, cx);
    })?;
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        let modal = app.read(cx).modal.as_ref().unwrap();
        assert_eq!(modal.selection.as_ref().unwrap().path, paths[1]);
        assert_eq!(modal.digest, app.read(cx).patches[&paths[1]].digest);
        app.update(cx, |app, cx| {
            app.switch_prompt_mode(ModalKind::Change, window, cx);
            app.prompt.update(cx, |input, cx| {
                input.set_value("Continuous diff selection fixture", window, cx)
            });
        });
        window.press("cmd-enter", cx);
    })?;
    cx.run_until_parked();
    let (request, comment_id) = cx.update(|cx| {
        let app = app.read(cx);
        assert!(app.error.is_none(), "{:?}", app.error);
        let comment = app
            .state
            .comments
            .iter()
            .find(|comment| comment.body == "Continuous diff selection fixture")
            .unwrap();
        assert_eq!(comment.selection.path, paths[1]);
        (
            app.review.as_ref().unwrap().request.clone(),
            comment.id.clone(),
        )
    });
    let state: ReviewState = decode(
        service
            .call(
                "deleteComment",
                json!({ "review": request, "id": comment_id }),
            )
            .map_err(anyhow::Error::msg)?,
    )
    .map_err(anyhow::Error::msg)?;
    cx.update_window(handle, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.state = state;
            app.close_modal(window, cx);
            app.diff
                .update(cx, |diff, cx| diff.set_file_collapsed(&original, true, cx));
            app.filter
                .update(cx, |input, cx| input.set_value("retry.ts", window, cx));
        })
    })?;
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.press("cmd-p", cx);
        window.click(("file", 0usize), cx);
    })?;
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        app.update(cx, |app, cx| {
            assert_eq!(app.file.as_ref(), Some(&original));
            assert!(!app.diff.read(cx).is_file_collapsed(&original));
            assert_eq!(
                app.diff.read(cx).files().len(),
                paths.len(),
                "file filtering changes navigation only"
            );
            app.filter
                .update(cx, |input, cx| input.set_value("", window, cx));
            app.backend = old_backend;
            app.prompt_mode = ModalKind::Ask;
        })
    })?;
    service.close();
    println!(
        "Continuous diff checks passed: all files, jump-only navigation, scrolling, per-file viewed controls, cross-file selection context, filtering, collapse/reveal."
    );
    Ok(())
}
