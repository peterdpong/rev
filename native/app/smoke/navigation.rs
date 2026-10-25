use super::*;

fn scroll(window: &mut Window, cx: &mut App, phase: TouchPhase, x: f32, y: f32) {
    let position = window.find("diff-surface").bounds().center();
    window.dispatch_event(
        PlatformInput::ScrollWheel(ScrollWheelEvent {
            position,
            delta: ScrollDelta::Pixels(point(px(x), px(y))),
            touch_phase: phase,
            ..Default::default()
        }),
        cx,
    );
    window.render_frame(cx);
}

pub(super) fn check(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<Patch>,
    output: &str,
) -> anyhow::Result<()> {
    let (paths, request, tab) = cx.update(|cx| {
        let app = app.read(cx);
        (
            app.review
                .as_ref()
                .unwrap()
                .files
                .iter()
                .map(|f| f.path.clone())
                .collect::<Vec<_>>(),
            app.navigation_request().unwrap(),
            app.workspace.active_tab_id.clone(),
        )
    });
    assert!(paths.len() >= 3);
    cx.update_window(handle, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.navigation = super::super::navigation::Navigation::default();
            app.navigation.loaded(
                None,
                request.clone(),
                &app.review.as_ref().unwrap().scope_key,
                None,
            );
            app.wrap = true;
            app.close_modal(window, cx);
            app.reveal_file(paths[0].clone(), window, cx);
        });
        window.render_frame(cx);
    })?;
    let first = cx.update(|cx| app.read(cx).diff.read(cx).scroll_position());
    cx.update_window(handle, |_, window, cx| {
        app.update(cx, |app, cx| app.jump_file(paths[2].clone(), window, cx));
        window.render_frame(cx);
        assert!(app.read(cx).can_navigate(NavigationDirection::Back));
        assert!(!app.read(cx).can_navigate(NavigationDirection::Forward));
        scroll(window, cx, TouchPhase::Started, 0., 0.);
        scroll(window, cx, TouchPhase::Moved, 120., 0.);
        assert_eq!(
            app.read(cx).file.as_ref(),
            Some(&paths[2]),
            "wait for finger release"
        );
    })?;
    cx.capture_screenshot(handle)?
        .save(format!("{output}/gpui-navigation-swipe.png"))?;
    cx.update_window(handle, |_, window, cx| {
        scroll(window, cx, TouchPhase::Ended, 0., 0.);
        assert_eq!(app.read(cx).file.as_ref(), Some(&paths[0]));
        assert_eq!(app.read(cx).diff.read(cx).scroll_position(), first);
        scroll(window, cx, TouchPhase::Moved, 300., 0.);
        assert_eq!(app.read(cx).file.as_ref(), Some(&paths[0]));
        // Unwrapped, wide code must pan horizontally, never consume Forward.
        app.update(cx, |app, cx| {
            app.wrap = false;
            app.split = true;
            app.diff
                .update(cx, |diff, cx| diff.set_mode(DiffMode::Split, cx));
            app.reveal_file(paths[2].clone(), window, cx);
        });
        window.resize(size(px(940.), px(760.)));
        window.bounds_changed(cx);
        window.render_frame(cx);
        assert!(app.read(cx).diff.read(cx).can_scroll_horizontally(-1.));
        let before_pan = app.read(cx).diff.read(cx).scroll_position();
        scroll(window, cx, TouchPhase::Started, 0., 0.);
        scroll(window, cx, TouchPhase::Moved, -120., 0.);
        scroll(window, cx, TouchPhase::Ended, 0., 0.);
        assert!(app.read(cx).can_navigate(NavigationDirection::Forward));
        assert_ne!(app.read(cx).diff.read(cx).scroll_position(), before_pan);
        app.update(cx, |app, cx| {
            app.wrap = true;
            app.split = false;
            app.diff
                .update(cx, |diff, cx| diff.set_mode(DiffMode::Unified, cx));
            app.reveal_file(paths[0].clone(), window, cx);
        });
        window.resize(size(px(1320.), px(860.)));
        window.bounds_changed(cx);
        window.render_frame(cx);
        window.press("cmd-]", cx);
        assert_eq!(app.read(cx).file.as_ref(), Some(&paths[2]));
        scroll(window, cx, TouchPhase::Started, 0., 0.);
        scroll(window, cx, TouchPhase::Moved, 120., 0.);
        scroll(window, cx, TouchPhase::Cancelled, 0., 0.);
        assert_eq!(app.read(cx).file.as_ref(), Some(&paths[2]));
        assert!(!app.read(cx).can_navigate(NavigationDirection::Forward));
        window.click("navigate-back", cx);
        scroll(window, cx, TouchPhase::Started, 0., 0.);
        scroll(window, cx, TouchPhase::Moved, -120., 0.);
        scroll(window, cx, TouchPhase::Ended, 0., 0.);
        assert_eq!(app.read(cx).file.as_ref(), Some(&paths[2]));
        window.press("cmd-[", cx);
        app.update(cx, |app, cx| app.jump_file(paths[1].clone(), window, cx));
        assert!(
            !app.read(cx).can_navigate(NavigationDirection::Forward),
            "a new visit clears the forward stack"
        );
        window.press("alt-tab", cx);
    })?;
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        assert_ne!(app.read(cx).workspace.active_tab_id, tab);
        window.press("cmd-[", cx);
    })?;
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(app.read(cx).workspace.active_tab_id, tab);
        assert_eq!(app.read(cx).file.as_ref(), Some(&paths[1]));
        // Comparison changes are navigation even though the branch stays the same.
        let sha = app.read(cx).review.as_ref().unwrap().head_sha.clone();
        app.update(cx, |app, cx| {
            app.load_review(
                ReviewRequest {
                    commit_range: Some(CommitRange {
                        start: sha.clone(),
                        end: sha,
                    }),
                    ..request.clone()
                },
                app.file.clone(),
                window,
                cx,
            )
        });
    })?;
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        assert!(app.read(cx).error.is_none(), "{:?}", app.read(cx).error);
        assert!(
            app.read(cx)
                .review
                .as_ref()
                .unwrap()
                .request
                .commit_range
                .is_some()
        );
        window.press("cmd-[", cx);
    })?;
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        assert!(
            app.read(cx)
                .review
                .as_ref()
                .unwrap()
                .request
                .commit_range
                .is_none()
        );
        assert_eq!(app.read(cx).file.as_ref(), Some(&paths[1]));
        let can_forward = app.read(cx).can_navigate(NavigationDirection::Forward);
        window.press("cmd-r", cx);
        assert!(can_forward);
    })?;
    cx.run_until_parked();
    cx.update(|cx| {
        assert!(
            app.read(cx).can_navigate(NavigationDirection::Forward),
            "refresh preserves history"
        )
    });
    // History retains the reviewed branch when Current follows a new checkout.
    let reviewed_head = cx.update(|cx| app.read(cx).review.as_ref().unwrap().request.head.clone());
    let switched = std::process::Command::new("git")
        .args(["-C", &request.repo, "switch", "-c", "navigation-checkout"])
        .output()?;
    anyhow::ensure!(
        switched.status.success(),
        "{}",
        String::from_utf8_lossy(&switched.stderr)
    );
    cx.update_window(handle, |_, window, cx| {
        app.update(cx, |app, cx| app.poll_workspace(window, cx))
    })?;
    for _ in 0..3 {
        cx.advance_clock(Duration::from_secs(2));
        cx.run_until_parked();
    }
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(
            app.read(cx)
                .review
                .as_ref()
                .unwrap()
                .request
                .head
                .as_deref(),
            Some("navigation-checkout")
        );
        window.press("cmd-[", cx);
    })?;
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(
            app.read(cx).review.as_ref().unwrap().request.head,
            reviewed_head
        )
    });
    println!(
        "Navigation checks passed: swipe release and cancellation, momentum, back/forward buttons and shortcuts, file position, branch and commit-range restoration, forward-stack truncation, refresh."
    );
    Ok(())
}
