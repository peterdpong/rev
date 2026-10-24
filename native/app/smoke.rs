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

// New viewports get a fresh offscreen renderer: retained GPU paint caches from
// a resized test window are not representative of a presented macOS frame.
fn compact_preview(
    cx: &mut HeadlessAppContext,
    source: &Entity<Patch>,
    collapsed: bool,
    threads: bool,
    output: &str,
) -> anyhow::Result<()> {
    let (review, workspace, state, file, patch, providers) = cx.update(|cx| {
        let app = source.read(cx);
        (
            app.review.clone(),
            app.workspace.clone(),
            app.state.clone(),
            app.file.clone(),
            app.patch.clone(),
            app.providers.clone(),
        )
    });
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Default::default(),
                    size: size(px(940.), px(600.)),
                })),
                titlebar: Some(TitlebarOptions {
                    title: Some("Patch".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(16.), px(15.))),
                }),
                app_owns_titlebar_drag: true,
                focus: false,
                show: false,
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    let mut app = Patch::new(Err("presentation fixture".into()), None, window, cx);
                    app.error = None;
                    app.review = review;
                    app.workspace = workspace;
                    app.state = state;
                    app.file = file;
                    app.patch = patch;
                    app.providers = providers;
                    app.sidebar_collapsed = collapsed;
                    app.threads_visible = threads;
                    app.active_thread = threads.then(|| "question-preview".into());
                    app.wrap = true;
                    app.diff.update(cx, |diff, cx| {
                        diff.set_files(DiffFile::parse(&app.patch.patch).unwrap(), cx)
                    });
                    app
                })
            },
        )
    })?;
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        for id in ["ask-review", "review-details", "choose-commits", "refresh"] {
            let bounds = window.find(id).bounds();
            assert!(
                bounds.size.width >= px(12.) && bounds.right() <= window.viewport_size().width,
                "{id}: {bounds:?}"
            );
        }
    })?;
    cx.capture_screenshot(handle)?.save(output)?;
    Ok(())
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
        bind_shortcuts(cx);
    });
    let (handle, app) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Default::default(),
                    size: size(px(1320.), px(860.)),
                })),
                titlebar: Some(TitlebarOptions {
                    title: Some("Patch".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(16.), px(15.))),
                }),
                app_owns_titlebar_drag: true,
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
    // Popovers use their trigger's geometry and leave the rest of the review interactive.
    cx.update_window(handle, |_, window, cx| window.click("ask-review", cx))?;
    cx.run_until_parked();
    cx.capture_screenshot(handle)?
        .save(format!("{output}/gpui-general-ask.png"))?;
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let trigger = window.find("ask-review").bounds();
        let panel = window.find("review-popover").bounds();
        assert_eq!(panel.top(), trigger.bottom() + px(6.));
        assert_eq!(panel.right(), trigger.right());
        window.input("Keep this question", cx);
        window.click("ask-review", cx);
        assert!(
            app.read(cx).modal.is_none(),
            "clicking the trigger again closes its popover"
        );
        window.click("ask-review", cx);
        assert_eq!(
            app.read(cx).prompt.read(cx).value().as_ref(),
            "Keep this question"
        );
        window.click("filter-files", cx);
        assert!(
            app.read(cx).modal.is_none(),
            "outside clicks dismiss without a blocking backdrop"
        );
        window.input("retry", cx);
        assert_eq!(app.read(cx).filter.read(cx).value().as_ref(), "retry");
        app.update(cx, |app, cx| {
            app.filter
                .update(cx, |filter, cx| filter.set_value("", window, cx))
        });
        window.click("ask-review", cx);
        window.click("choose-commits", cx);
        assert!(
            app.read(cx).modal.as_ref().unwrap().kind == ModalKind::Ask,
            "a popover must block controls it covers"
        );
        window.click("add-project", cx);
        assert!(
            app.read(cx).modal.as_ref().unwrap().kind == ModalKind::Open,
            "an exposed trigger switches popovers with one click"
        );
        window.press("escape", cx);
        assert!(app.read(cx).modal.is_none());
        window.click("open-project", cx);
        window.render_frame(cx);
        let trigger = window.find("open-project").bounds();
        let panel = window.find("review-popover").bounds();
        assert!(
            panel.bottom() >= trigger.top() - px(7.) && panel.bottom() <= trigger.top() - px(5.)
        );
        assert!(
            panel.size.width > px(225.),
            "the sidebar must not clip its dropdown"
        );
    })?;
    cx.capture_screenshot(handle)?
        .save(format!("{output}/gpui-open.png"))?;
    cx.update_window(handle, |_, window, cx| {
        window.press("escape", cx);
        window.press("cmd-b", cx);
        window.press("cmd-o", cx);
        window.render_frame(cx);
        assert!(app.read(cx).modal.as_ref().unwrap().kind == ModalKind::Open);
        assert_eq!(
            window.find("review-popover").bounds().left(),
            window.find("toggle-sidebar").bounds().left()
        );
        window.press("escape", cx);
        window.press("cmd-b", cx);
        app.update(cx, |app, _| app.drafts.clear());
    })?;
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
                        window.within("prompt-mode-tabs").click(if gutter { 1usize } else { 0usize }, cx);
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
        assert!(window.find("threads-tab").visible());
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
        window.within("prompt-mode-tabs").click(0usize, cx);
        app.update(cx, |app, _| {
            assert_eq!(app.modal.as_ref().unwrap().kind as u8, ModalKind::Ask as u8);
            let selection = app.modal.as_ref().unwrap().selection.as_ref().unwrap();
            assert_eq!((selection.start, selection.end), (4, 6));
        });
    })?;
    // Switch modes from inside the input using production key bindings.
    cx.update_window(handle, |_, window, cx| {
        let bound = app.read(cx).modal.as_ref().unwrap().clone();
        window.press("cmd-2", cx);
        assert!(app.read(cx).modal.as_ref().unwrap().kind == ModalKind::Change);
        window.input("Handle cancellation", cx);
        window.press("cmd-1", cx);
        assert!(app.read(cx).modal.as_ref().unwrap().kind == ModalKind::Ask);
        window.press("ctrl-tab", cx);
        assert!(app.read(cx).modal.as_ref().unwrap().kind == ModalKind::Change);
        window.press("ctrl-shift-tab", cx);
        assert!(app.read(cx).modal.as_ref().unwrap().kind == ModalKind::Ask);
        assert_eq!(
            app.read(cx).prompt.read(cx).value().as_ref(),
            "Handle cancellation"
        );
        let modal = app.read(cx).modal.as_ref().unwrap();
        assert_eq!(modal.selection, bound.selection);
        assert_eq!(modal.digest, bound.digest);
        assert_eq!(modal.serial, bound.serial);
        assert!(
            window
                .within("prompt-mode-tabs")
                .find(0usize)
                .bounds()
                .bottom()
                <= window
                    .find(("input", app.read(cx).prompt.entity_id()))
                    .bounds()
                    .top(),
            "prompt input belongs below the tabs"
        );
        window.press("cmd-2", cx);
        app.update(cx, |app, cx| {
            app.prompt
                .update(cx, |input, cx| input.set_value("", window, cx))
        });
        window.press("cmd-enter", cx);
        assert_eq!(
            app.read(cx).error.as_deref(),
            Some("Write a change request.")
        );
        app.update(cx, |app, _| app.submitting = true);
        window.press("cmd-1", cx);
        assert!(app.read(cx).modal.as_ref().unwrap().kind == ModalKind::Change);
        app.update(cx, |app, _| app.submitting = false);
        window.press("cmd-1", cx);
        window.input("Explain the retry behavior", cx);
    })?;
    cx.advance_clock(Duration::from_millis(300));
    cx.run_until_parked();
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
        window.press("cmd-2", cx);
        assert!(
            app.read(cx).modal.as_ref().unwrap().kind == ModalKind::Ask,
            "general questions must not become changes without selected code"
        );
        window.click("close-modal", cx);
        window
            .within(("new", 3usize))
            .within("new-gutter")
            .click(("line", 3usize), cx);
    })?;
    cx.update_window(handle, |_, window, cx| {
        window.within("prompt-mode-tabs").click(1usize, cx);
        assert!(app.read(cx).modal.as_ref().unwrap().selection.is_some());
        let bound = app.read(cx).modal.as_ref().unwrap().clone();
        app.update(cx, |app, cx| {
            app.prompt.update(cx, |input, cx| {
                input.set_value("Handle cancellation", window, cx)
            });
            app.close_modal(window, cx);
            app.open_modal(
                ModalKind::Change,
                bound.selection.clone(),
                None,
                "ask-review",
                window,
                cx,
            );
            assert_eq!(app.prompt.read(cx).value().as_ref(), "Handle cancellation");
        });
        window.click("close-modal", cx);
        window.click("diff-mode", cx);
        assert_eq!(app.read(cx).diff.read(cx).mode(), DiffMode::Split);
        window.click("wrap", cx);
        assert!(app.read(cx).wrap);
        window.click("threads-tab", cx);
        assert!(app.read(cx).threads_visible);
        window.click("files-tab", cx);
        assert!(!app.read(cx).threads_visible);
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

    // Exercise commit selection through the real local service, including
    // applying a historical scope and restoring the existing branch tab.
    let service = Backend::start().map_err(anyhow::Error::msg)?;
    cx.update_window(handle, |_, window, cx| {
        app.update(cx, |app, _| app.backend = Some(service.clone()));
        window.click("choose-commits", cx);
    })?;
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        assert!(!app.read(cx).commit_picker.loading);
        assert!(app.read(cx).commit_picker.commits.len() >= 2);
        window.click(("commit", 0usize), cx);
        assert_eq!(app.read(cx).commit_picker.count, 1);
        let position = window.find(("commit", 1usize)).bounds().center();
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position,
                click_count: 1,
                modifiers: Modifiers {
                    shift: true,
                    ..Default::default()
                },
                ..Default::default()
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position,
                click_count: 1,
                modifiers: Modifiers {
                    shift: true,
                    ..Default::default()
                },
            }),
            cx,
        );
        assert_eq!(app.read(cx).commit_picker.count, 2);
        let picker = &app.read(cx).commit_picker;
        assert_eq!(picker.range.as_ref().unwrap().start, picker.commits[1].sha);
        assert_eq!(picker.range.as_ref().unwrap().end, picker.commits[0].sha);
    })?;
    cx.capture_screenshot(handle)?
        .save(format!("{output}/gpui-commits.png"))?;
    cx.update_window(handle, |_, window, cx| {
        let position = window.find(("commit", 1usize)).bounds().center();
        pointer_down(window, position, cx);
        pointer_up(window, position, cx);
        assert_eq!(app.read(cx).commit_picker.count, 1);
        window.click("submit-modal", cx);
    })?;
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        assert!(app.read(cx).error.is_none(), "{:?}", app.read(cx).error);
        assert_eq!(app.read(cx).review.as_ref().unwrap().commit_count, 1);
        assert!(
            app.read(cx)
                .review
                .as_ref()
                .unwrap()
                .request
                .commit_range
                .is_some()
        );
        assert_eq!(app.read(cx).workspace.projects[0].tabs.len(), 1);
        app.update(cx, |app, _| {
            let review = app.review.as_ref().unwrap();
            app.run_requests.insert(review.scope_key.clone(), review.request.clone());
            app.runs.insert(review.scope_key.clone(), decode(json!({
                "id": "scope-fixture", "status": "running", "mode": "ask", "events": [], "approvals": []
            })).unwrap());
        });
        window.click("choose-commits", cx);
    })?;
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.click("all-commits", cx);
        assert_eq!(app.read(cx).commit_picker.count, 0);
        window.press("cmd-enter", cx);
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
                .is_none()
        );
        assert_eq!(app.read(cx).review.as_ref().unwrap().files.len(), 4);
        assert_eq!(app.read(cx).workspace.projects[0].tabs.len(), 1);
        window.click("agent-output", cx);
        assert!(
            app.read(cx)
                .modal
                .as_ref()
                .unwrap()
                .request
                .commit_range
                .is_some(),
            "a running agent must keep its original comparison when the visible scope changes"
        );
        assert_ne!(
            app.read(cx).modal.as_ref().unwrap().tab,
            app.read(cx).review.as_ref().unwrap().scope_key
        );
        window.click("close-modal", cx);
        app.update(cx, |app, _| {
            app.runs.clear();
            app.run_requests.clear();
        });
    })?;

    // Presentation fixtures stay in memory; no sample conversations are saved to the repo.
    cx.update_window(handle, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.load_file(path.clone(), window, cx);
            app.filter
                .update(cx, |input, cx| input.set_value("", window, cx));
            app.split = false;
            app.diff
                .update(cx, |diff, cx| diff.set_mode(DiffMode::Unified, cx));
        });
    })?;
    cx.run_until_parked();
    cx.capture_screenshot(handle)?
        .save(format!("{output}/gpui-branch.png"))?;
    cx.update_window(handle, |_, window, cx| {
        app.update(cx, |app, cx| {
            let review = app.review.as_mut().unwrap();
            review.pull_request = Some(PullRequest {
                number: 42, title: "Make request retries resilient to transient failures and cancellation".into(),
                body: "Adds exponential backoff, a delay cap, and per-request cancellation.\n\n## Review notes\nCheck cancellation during the delay and the maximum number of attempts.".into(),
                url: "https://github.com/example/patch/pull/42".into(), author: "reviewer".into(), state: "OPEN".into(), is_draft: false,
                head_ref_name: "feat/resilient-requests".into(), base_ref_name: "main".into(),
            });
            let source = Selection { path: path.clone(), side: "additions".into(), start: 4, end: 6 };
            app.state.threads = vec![
                decode(json!({ "id": "question-preview", "kind": "question", "body": "Does cancellation interrupt the delay between retries?",
                    "selection": source, "digest": app.patch.digest, "headSha": review.head_sha, "mergeBase": review.merge_base,
                    "updatedAt": "2026-10-24T12:00:00Z", "commentIds": [], "turns": [{ "id": "turn-preview", "question": "Does cancellation interrupt the delay between retries?", "provider": "codex", "status": "completed",
                        "events": [{ "kind": "message", "text": "Cancellation is checked before each attempt, but the delay does not observe the signal. Cancelling during backoff waits for the timer to finish.\n\nPass the `AbortSignal` to the delay and reject when it aborts. Add a test for cancellation while waiting." }] }] })).unwrap(),
                decode(json!({ "id": "general-preview", "kind": "question", "body": "What should I check before merging?", "headSha": review.head_sha, "mergeBase": review.merge_base,
                    "updatedAt": "2026-10-24T11:00:00Z", "commentIds": [], "turns": [{ "id": "general-turn", "question": "What should I check before merging?", "provider": "codex", "status": "completed",
                        "events": [{ "kind": "message", "text": "Check cancellation while waiting, the maximum retry delay, and the final failed attempt." }] }] })).unwrap(),
            ];
            app.state.comments = vec![Comment { selection: source, id: "change-preview".into(), body: "Add a regression test for cancelling a request during backoff.".into(), status: "queued".into(), digest: app.patch.digest.clone() }];
            app.threads_visible = false;
            app.active_thread = None;
            window.focus(&app.focus, cx);
            cx.notify();
        });
    })?;
    cx.run_until_parked();
    cx.capture_screenshot(handle)?
        .save(format!("{output}/gpui-pr-review.png"))?;
    cx.update_window(handle, |_, window, cx| {
        let index = app
            .read(cx)
            .visible_files(cx)
            .iter()
            .position(|file| file.path == path)
            .unwrap();
        window.click(SharedString::from(format!("file-threads-{index}")), cx);
        assert!(
            app.read(cx).threads_visible,
            "file discussion badges must not activate the underlying file row"
        );
        assert_eq!(app.read(cx).thread_filter.as_ref(), Some(&path));
        window.click("thread-question-preview", cx);
        assert_eq!(
            app.read(cx).active_thread.as_deref(),
            Some("question-preview")
        );
    })?;
    cx.run_until_parked();
    cx.capture_screenshot(handle)?
        .save(format!("{output}/gpui-threads.png"))?;
    cx.update_window(handle, |_, window, cx| {
        window.click("code-question-preview", cx);
        assert!(!app.read(cx).threads_visible);
        assert_eq!(app.read(cx).file.as_ref(), Some(&path));
        window.click("review-details", cx);
        assert!(
            app.read(cx)
                .modal
                .as_ref()
                .is_some_and(|m| m.kind == ModalKind::Details)
        );
    })?;
    cx.run_until_parked();
    cx.capture_screenshot(handle)?
        .save(format!("{output}/gpui-details.png"))?;
    cx.update_window(handle, |_, window, cx| {
        window.press("escape", cx);
        window.resize(size(px(940.), px(600.)));
        window.bounds_changed(cx);
    })?;
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.viewport_size(), size(px(940.), px(600.)));
        assert!(
            window.find("choose-commits").bounds().left()
                > window.find("threads-tab").bounds().right()
        );
        assert!(window.find("refresh").bounds().right() <= window.viewport_size().width);
        assert!(window.find("ask-review").bounds().right() <= window.viewport_size().width);
        assert!(window.find("toggle-sidebar").bounds().left() >= px(80.));
    })?;
    cx.capture_screenshot(handle)?
        .save(format!("{output}/gpui-narrow.png"))?;
    cx.update_window(handle, |_, window, cx| {
        window.press("cmd-b", cx);
        assert!(app.read(cx).sidebar_collapsed);
    })?;
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("ask-review").bounds().right() <= window.viewport_size().width);
        assert!(window.find("review-details").bounds().right() <= window.viewport_size().width);
        assert!(window.find("choose-commits").bounds().right() <= window.viewport_size().width);
    })?;
    cx.capture_screenshot(handle)?
        .save(format!("{output}/gpui-collapsed.png"))?;
    cx.update_window(handle, |_, window, cx| {
        window.click("threads-tab", cx);
        if app.read(cx).active_thread.as_deref() != Some("question-preview") {
            window.click("thread-question-preview", cx);
        }
    })?;
    cx.run_until_parked();
    cx.capture_screenshot(handle)?
        .save(format!("{output}/gpui-threads-narrow.png"))?;
    compact_preview(
        &mut cx,
        &app,
        false,
        false,
        &format!("{output}/gpui-narrow.png"),
    )?;
    compact_preview(
        &mut cx,
        &app,
        true,
        false,
        &format!("{output}/gpui-collapsed.png"),
    )?;
    compact_preview(
        &mut cx,
        &app,
        true,
        true,
        &format!("{output}/gpui-threads-narrow.png"),
    )?;
    service.close();
    println!(
        "Native UI checks passed: PR/commit details, saved threads and source links, file badges, compact/collapsed layouts, anchored popovers, trigger toggle, outside dismissal, draft restoration, keyboard open/close, single/range commit selection, historical comparison, branch restoration, tabbed prompt, keyboard mode switching, text/context preservation, submit shortcut, forward/reverse code and gutter drags, original/modified split sides, stable selection layout, general ask, change draft, modal Escape, split view, wrap, feedback, file filter."
    );
    Ok(())
}
