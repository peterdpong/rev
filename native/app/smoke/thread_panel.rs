use super::*;
use gpui_kit::test::TestQueryExt;

fn present(window: &Window, id: &str) -> bool {
    let id = ElementId::from(SharedString::from(id.to_string()));
    window
        .elements()
        .iter()
        .any(|element| element.path().contains(&id))
}

pub(super) fn check(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<Patch>,
    output: &str,
) -> anyhow::Result<()> {
    let root = std::fs::canonicalize(output)?.join("thread-agent-fixture");
    std::fs::create_dir_all(&root)?;
    std::fs::write(root.join("requests.jsonl"), "")?;
    let (review, changes, original) = cx.update(|cx| app.update(cx, |app, _| {
        let review = app.review.clone().unwrap();
        let changes = ["src/lib/retry.ts", "src/api/client.ts"].iter().enumerate().map(|(i, path)| json!({
            "id": format!("queued-{i}"), "path": path, "side": "additions", "start": 4, "end": 6,
            "body": if i == 0 { "Add a regression test for cancellation during backoff." } else { "Make the error status available to callers." },
            "status": "queued", "digest": app.patches[*path].digest,
        })).collect::<Vec<_>>();
        let original = (app.state.clone(), app.backend.take(), std::mem::take(&mut app.runs), std::mem::take(&mut app.run_requests), app.file.clone(), app.wrap);
        (review, changes, original)
    }));
    let script = root.join("host.mjs");
    let data = json!({ "changes": changes, "head": review.head_sha, "base": review.merge_base, "capture": root.join("requests.jsonl") });
    std::fs::write(
        &script,
        format!(
            "const fixture = {data};\n{}",
            r#"
import readline from 'node:readline';
import { appendFileSync } from 'node:fs';
const runs = new Map();
for await (const line of readline.createInterface({ input: process.stdin })) {
  const { id, method, params } = JSON.parse(line);
  appendFileSync(fixture.capture, JSON.stringify({ method, params }) + '\n');
  let result = true;
  if (method === 'run') {
    const apply = params.mode === 'apply', key = apply ? 'apply-fixture' : 'ask-fixture';
    const thread = { id: key, kind: apply ? 'change' : 'question', body: apply ? 'Apply review feedback' : params.question,
      selection: params.selection, digest: params.selectionDigest, headSha: fixture.head, mergeBase: fixture.base,
      updatedAt: '2026-10-26T12:00:00Z', commentIds: apply ? params.commentIds : [],
      changes: apply ? fixture.changes.filter(change => params.commentIds.includes(change.id)) : [], turns: [] };
    const run = { id: key, status: 'running', mode: params.mode, events: [], approvals: [], thread };
    thread.turns.push({ id: key, question: thread.body, provider: params.provider, status: run.status, events: run.events });
    runs.set(key, run); result = run;
  } else if (method === 'runStatus') {
    result = runs.get(params.id);
    if (result.mode === 'ask') {
      result.status = 'completed'; result.events = [{ kind: 'message', text: 'The retry loop is bounded and cancellation stops the pending delay.' }];
      result.thread.turns[0].status = result.status; result.thread.turns[0].events = result.events;
    }
  } else if (method === 'cancel') { runs.get(params.id).status = 'cancelled'; }
  else if (method === 'deleteComment') { result = { comments: fixture.changes.filter(change => change.id !== params.id), viewed: {}, threads: [] }; }
  process.stdout.write(JSON.stringify({ id, result }) + '\n');
}
"#
        ),
    )?;
    let backend = Backend::fixture(script).map_err(anyhow::Error::msg)?;
    cx.update_window(handle, |_, window, cx| app.update(cx, |app, cx| {
        app.backend = Some(backend.clone());
        app.state.comments = decode(json!(changes)).unwrap();
        app.state.threads = vec![decode(json!({ "id": "source-fixture", "kind": "question", "body": "Why does this retry path need cancellation?", "selection": changes[0], "digest": changes[0]["digest"], "headSha": review.head_sha, "mergeBase": review.merge_base, "updatedAt": "2026-10-25T12:00:00Z", "commentIds": [], "turns": [{ "id": "source-turn", "question": "Why cancellation?", "provider": "codex", "status": "completed", "events": [{ "kind": "message", "text": "It avoids doing work after the request is cancelled." }] }] })).unwrap()];
        app.show_threads(None, None, window, cx);
        cx.set_reduce_motion(true);
    }))?;
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let diff = window.find("diff-surface").bounds();
        let panel = window.find("thread-sidebar").bounds();
        assert!(diff.size.width > px(600.));
        assert_eq!(diff.right(), panel.left());
        assert!(window.find("apply-changes").bounds().bottom() <= panel.bottom());
        window.click("threads-changes", cx);
        window.render_frame(cx);
        assert!(!present(window, "thread-source-fixture"));
        assert!(present(window, "queued-queued-0"));
        assert!(present(window, "snippet-comment-code-queued-0"));
        window.click("comment-code-queued-1", cx);
    })?;
    cx.run_until_parked();
    cx.update_window(handle, |_, _window, cx| {
        assert!(app.read(cx).threads_visible);
        assert_eq!(app.read(cx).file.as_deref(), Some("src/api/client.ts"));
        assert_eq!(
            app.read(cx).diff.read(cx).selected_lines().unwrap().start(),
            4
        );
        assert!(app.read(cx).modal.is_none());
    })?;
    cx.capture_screenshot(handle)?
        .save(format!("{output}/gpui-change-sidebar.png"))?;
    cx.update_window(handle, |_, window, cx| {
        window.click("threads-all", cx);
        window.click("ask-review", cx);
        window.input("What happens when the request is cancelled?", cx);
        window.press("cmd-enter", cx);
    })?;
    cx.run_until_parked();
    let epoch = cx.update(|cx| {
        let app = app.read(cx);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.modal.is_none());
        assert!(app.threads_visible);
        assert_eq!(app.active_thread.as_deref(), Some("ask-fixture"));
        assert_eq!(app.runs[&review.scope_key].status, "running");
        app.file_epoch
    });
    cx.capture_screenshot(handle)?
        .save(format!("{output}/gpui-ask-running.png"))?;
    cx.advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(app.read(cx).runs[&review.scope_key].status, "completed");
        assert_eq!(
            app.read(cx).file_epoch,
            epoch,
            "answers must not reload or displace the diff"
        );
        assert!(present(window, "status-ask-fixture"));
    })?;
    cx.capture_screenshot(handle)?
        .save(format!("{output}/gpui-ask-sidebar.png"))?;
    cx.update_window(handle, |_, window, cx| {
        window.click("ask-review", cx);
        assert!(
            app.read(cx).prompt.read(cx).value().is_empty(),
            "sent questions must not become new drafts"
        );
        window.press("escape", cx);
        window.click("threads-changes", cx);
        window.click("apply-changes", cx);
    })?;
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let app = app.read(cx);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_eq!(app.runs[&review.scope_key].mode, "apply");
        assert!(app.repository_agent_running());
        assert_eq!(app.active_thread.as_deref(), Some("apply-fixture"));
        assert_eq!(
            app.state
                .threads
                .iter()
                .find(|thread| thread.id == "apply-fixture")
                .unwrap()
                .changes
                .len(),
            2
        );
        assert!(present(window, "change-source-apply-fixture-queued-0"));
    })?;
    let requests = std::fs::read_to_string(root.join("requests.jsonl"))?
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let apply = requests
        .iter()
        .find(|request| request["method"] == "run" && request["params"]["mode"] == "apply")
        .unwrap();
    assert_eq!(
        apply["params"]["commentIds"],
        json!(["queued-0", "queued-1"])
    );
    assert_eq!(apply["params"]["review"], json!(review.request));
    cx.update_window(handle, |_, _window, cx| {
        app.update(cx, |app, cx| {
            let mut run = app.runs[&review.scope_key].clone();
            run.status = "completed".into();
            run.events = vec![AgentEvent {
                kind: "message".into(),
                text: "Added the regression test and exposed the error status.".into(),
            }];
            let turn = run.thread.as_mut().unwrap().turns.last_mut().unwrap();
            turn.status = run.status.clone();
            turn.events = run.events.clone();
            app.remember_thread(&review.scope_key, &run, cx);
            app.runs.insert(review.scope_key.clone(), run);
        })
    })?;
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(!present(window, "changes-footer"));
        window.resize(size(px(940.), px(600.)));
        window.bounds_changed(cx);
        app.update(cx, |app, cx| {
            app.wrap = true;
            cx.notify();
        });
    })?;
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("diff-surface").bounds().size.width >= px(400.));
        assert!(window.find("thread-sidebar").bounds().right() <= window.viewport_size().width);
    })?;
    cx.capture_screenshot(handle)?
        .save(format!("{output}/gpui-thread-sidebar-narrow.png"))?;
    backend.close();
    cx.update_window(handle, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.state = original.0;
            app.backend = original.1;
            app.runs = original.2;
            app.run_requests = original.3;
            app.wrap = original.5;
            app.changes_only = false;
            app.thread_filter = None;
            app.active_thread = None;
            app.close_modal(window, cx);
            app.hide_threads(window, cx);
            window.resize(size(px(1320.), px(860.)));
            window.bounds_changed(cx);
            if let Some(path) = original.4 {
                app.jump_file(path, window, cx);
            }
            app.error = None;
        })
    })?;
    cx.run_until_parked();
    println!(
        "Thread sidebar checks passed: persistent diff, source links and highlights, change filter and snippets, ask submission/status/answer without a diff reload, batched apply payload, retained source tags, completed queue, compact layout."
    );
    Ok(())
}
