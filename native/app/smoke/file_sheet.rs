use super::*;

fn settle(cx: &mut HeadlessAppContext, handle: AnyWindowHandle, ms: u64) -> anyhow::Result<()> {
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))?;
    for step in (0..ms).step_by(16) {
        cx.advance_clock(Duration::from_millis((ms - step).min(16)));
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))?;
    }
    Ok(())
}

pub(super) fn check(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<Patch>,
    output: &str,
) -> anyhow::Result<()> {
    cx.update(|cx| cx.set_reduce_motion(false));
    let (surface, edge, away) = cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(!app.read(cx).file_sheet.open);
        let surface = window.find("diff-surface").bounds();
        assert_eq!(surface.left(), window.find("sidebar-clip").bounds().right());
        (
            surface,
            window.find("file-sheet-edge").bounds().center(),
            surface.center(),
        )
    })?;
    cx.capture_screenshot(handle)?
        .save(format!("{output}/gpui-file-sheet-closed.png"))?;
    cx.update_window(handle, |_, window, cx| window.simulate_mouse_move(edge, cx))?;
    settle(cx, handle, 50)?;
    assert!(
        !cx.update(|cx| app.read(cx).file_sheet.open),
        "brief passes must not open the sheet"
    );
    settle(cx, handle, 70)?;
    assert!(cx.update(|cx| app.read(cx).file_sheet.open));
    settle(cx, handle, 220)?;
    cx.update_window(handle, |_, window, cx| {
        let panel = window.find("file-sheet").bounds();
        assert!(panel.size.height > px(200.));
        assert_eq!(panel.size.width, px(272.));
        assert_eq!(
            window.find("diff-surface").bounds(),
            surface,
            "hovering must not shift the diff"
        );
        window.simulate_mouse_move(panel.center(), cx);
    })?;
    settle(cx, handle, 250)?;
    assert!(
        cx.update(|cx| app.read(cx).file_sheet.open),
        "moving into the sheet keeps it open"
    );
    cx.capture_screenshot(handle)?
        .save(format!("{output}/gpui-file-sheet.png"))?;
    cx.update_window(handle, |_, window, cx| window.simulate_mouse_move(away, cx))?;
    settle(cx, handle, 70)?;
    assert!(
        cx.update(|cx| app.read(cx).file_sheet.open),
        "a small pointer gap has a grace period"
    );
    cx.update_window(handle, |_, window, cx| {
        window.simulate_mouse_move(window.find("file-sheet").bounds().center(), cx);
    })?;
    settle(cx, handle, 220)?;
    assert!(cx.update(|cx| app.read(cx).file_sheet.open));
    cx.update_window(handle, |_, window, cx| window.simulate_mouse_move(away, cx))?;
    settle(cx, handle, 220)?;
    assert!(!cx.update(|cx| app.read(cx).file_sheet.open));
    settle(cx, handle, 220)?;
    // Keyboard access keeps search alive even with the pointer outside the sheet.
    cx.update_window(handle, |_, window, cx| {
        window.press("cmd-p", cx);
        window.input("retry.ts", cx);
        assert_eq!(app.read(cx).filter.read(cx).value().as_ref(), "retry.ts");
    })?;
    settle(cx, handle, 400)?;
    assert!(cx.update(|cx| app.read(cx).file_sheet.open));
    cx.update_window(handle, |_, window, cx| {
        window.press("enter", cx);
        assert_eq!(app.read(cx).file.as_deref(), Some("src/lib/retry.ts"));
        assert!(
            !app.read(cx).file_sheet.open,
            "choosing a file dismisses the sheet"
        );
        app.update(cx, |app, cx| {
            app.filter
                .update(cx, |input, cx| input.set_value("", window, cx))
        });
    })?;
    settle(cx, handle, 220)?;
    // A source selection may pass through the reveal edge without opening navigation.
    let start = cx.update_window(handle, |_, window, cx| {
        let start = line_point_in_file(window, 2, 4, true, false);
        pointer_down(window, start, cx);
        start
    })?;
    cx.run_until_parked();
    assert!(cx.update(|cx| app.read(cx).selecting));
    cx.update_window(handle, |_, window, cx| {
        pointer_move(window, point(edge.x, start.y), cx);
    })?;
    settle(cx, handle, 250)?;
    assert!(
        !cx.update(|cx| app.read(cx).file_sheet.open),
        "drag selection must not summon the sheet"
    );
    cx.update_window(handle, |_, window, cx| {
        pointer_up(window, edge, cx);
        app.update(cx, |app, cx| app.close_modal(window, cx));
        window.simulate_mouse_move(away, cx);
        window.click("browse-files", cx);
        assert!(app.read(cx).file_sheet.open);
        window.press("escape", cx);
        assert!(!app.read(cx).file_sheet.open);
        cx.set_reduce_motion(true);
        window.render_frame(cx);
        window.press("cmd-p", cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("file-sheet").bounds().left(),
            surface.left() + px(8.)
        );
        window.click("diff-mode", cx);
        assert!(
            !app.read(cx).file_sheet.open,
            "outside clicks dismiss and still activate their target"
        );
        window.click("diff-mode", cx);
    })?;
    cx.run_until_parked();
    println!(
        "Floating file sheet checks passed: full-width diff, hover delay, pointer transfer and return, auto-dismiss, focused search, keyboard jump, Escape, outside click, drag safety, Reduce Motion."
    );
    Ok(())
}
