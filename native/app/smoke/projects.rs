use super::*;

struct Fixture(std::path::PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn git(repo: &str, args: &[&str]) -> anyhow::Result<()> {
    let result = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()?;
    anyhow::ensure!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(())
}

pub(super) fn check_projects(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    app: &Entity<Patch>,
    output: &str,
) -> anyhow::Result<()> {
    let source = cx.update(|cx| app.read(cx).review.as_ref().unwrap().request.repo.clone());
    let root = Fixture(std::env::temp_dir().join(format!(
            "patch-project-ui-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        )));
    std::fs::create_dir_all(&root.0)?;
    let repo = root.0.join("current-project").to_string_lossy().to_string();
    git(
        &source,
        &["clone", "--local", "--no-hardlinks", &source, &repo],
    )?;
    git(&repo, &["branch", "main", "origin/main"])?;
    git(
        &repo,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    )?;
    cx.update_window(handle, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.close_modal(window, cx);
            app.sidebar_collapsed = false;
            app.threads_visible = false;
            app.thread_reply
                .update(cx, |input, cx| input.set_value("", window, cx));
        });
        window.resize(size(px(1320.), px(860.)));
        window.bounds_changed(cx);
        window.render_frame(cx);
        window.press("cmd-o", cx);
        app.update(cx, |app, cx| {
            app.fields[0].update(cx, |input, cx| input.set_value(repo.clone(), window, cx))
        });
        window.click("submit-modal", cx);
    })?;
    cx.run_until_parked();
    let (current_id, branch, canonical_repo) = cx.update(|cx| {
        let app = app.read(cx);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.modal.is_none());
        let tab = app.workspace.active().unwrap();
        assert!(tab.is_current());
        assert_eq!(tab.request.head.as_deref(), Some("HEAD"));
        assert_eq!(
            app.review.as_ref().unwrap().request.head.as_ref(),
            Some(&tab.label)
        );
        (tab.id.clone(), tab.label.clone(), tab.request.repo.clone())
    });
    cx.update_window(handle, |_, window, cx| {
        window.click(SharedString::from(format!("add-{canonical_repo}")), cx);
    })?;
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(
            app.read(cx).modal.as_ref().unwrap().request.repo,
            canonical_repo
        );
        assert!(app.read(cx).branch_options.iter().any(|b| b == "main"));
        assert!(window.find("fetch-branches").bounds().right() <= window.viewport_size().width);
    })?;
    cx.capture_screenshot(handle)?
        .save(format!("{output}/gpui-branches.png"))?;
    cx.update_window(handle, |_, window, cx| {
        let index = app
            .read(cx)
            .branch_options
            .iter()
            .position(|b| b == "main")
            .unwrap();
        window.click(("branch", index), cx);
    })?;
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        assert!(!app.read(cx).workspace.active().unwrap().is_current());
        window.click(SharedString::from(format!("tab-{current_id}")), cx);
    })?;
    cx.run_until_parked();
    cx.capture_screenshot(handle)?
        .save(format!("{output}/gpui-current-project.png"))?;
    cx.update_window(handle, |_, window, cx| {
        app.update(cx, |app, cx| app.poll_workspace(window, cx))
    })?;
    git(&repo, &["switch", "main"])?;
    for _ in 0..3 {
        cx.advance_clock(Duration::from_secs(2));
        cx.run_until_parked();
    }
    cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(app.workspace.active_tab_id.as_ref(), Some(&current_id));
        assert_eq!(
            app.review.as_ref().unwrap().request.head.as_deref(),
            Some("main")
        );
        assert_eq!(app.workspace.active().unwrap().label, "main");
    });
    // A checkout can change without changing the commit, including detached HEAD.
    for args in [
        &["switch", "--detach", "main"][..],
        &["switch", "-c", "same-commit"][..],
    ] {
        git(&repo, args)?;
        for _ in 0..3 {
            cx.advance_clock(Duration::from_secs(2));
            cx.run_until_parked();
        }
    }
    cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(
            app.review.as_ref().unwrap().request.head.as_deref(),
            Some("same-commit")
        );
        assert_eq!(app.workspace.active_tab_id.as_ref(), Some(&current_id));
    });
    // Keep the question's source branch intact while a prompt is being written.
    cx.update_window(handle, |_, window, cx| {
        window.click("ask-review", cx);
    })?;
    git(&repo, &["switch", &branch])?;
    for _ in 0..3 {
        cx.advance_clock(Duration::from_secs(2));
        cx.run_until_parked();
    }
    cx.update_window(handle, |_, window, cx| {
        assert_eq!(app.read(cx).workspace.active().unwrap().label, branch);
        assert_eq!(
            app.read(cx).modal.as_ref().unwrap().request.head.as_deref(),
            Some("same-commit")
        );
        window.press("escape", cx);
    })?;
    for _ in 0..3 {
        cx.advance_clock(Duration::from_secs(2));
        cx.run_until_parked();
    }
    cx.update(|cx| {
        assert_eq!(
            app.read(cx).review.as_ref().unwrap().request.head.as_ref(),
            Some(&branch)
        )
    });
    // Cycle real review tabs across both projects, including a collapsed project,
    // with focus in a prompt. Returning to that review must restore its draft.
    let tabs = cx.update(|cx| {
        app.read(cx)
            .workspace
            .projects
            .iter()
            .flat_map(|p| &p.tabs)
            .map(|tab| tab.id.clone())
            .collect::<Vec<_>>()
    });
    let start = tabs.iter().position(|id| id == &current_id).unwrap();
    cx.update_window(handle, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.workspace.projects[0].collapsed = true;
            app.active_thread = Some("navigation-draft".into());
            app.thread_reply.update(cx, |input, cx| {
                input.set_value("Keep this thread reply", window, cx)
            });
        });
        window.press("cmd-b", cx);
        assert!(app.read(cx).sidebar_collapsed);
        window.press("cmd-k", cx);
        window.input("Keep this review question", cx);
    })?;
    for (shortcut, direction) in [("alt-tab", 1_isize), ("alt-shift-tab", -1)] {
        for step in 1..=tabs.len() {
            cx.update_window(handle, |_, window, cx| window.press(shortcut, cx))?;
            cx.run_until_parked();
            cx.update(|cx| {
                let app = app.read(cx);
                let expected = (start as isize + direction * step as isize)
                    .rem_euclid(tabs.len() as isize) as usize;
                assert!(app.error.is_none(), "{:?}", app.error);
                assert_eq!(app.workspace.active_tab_id.as_ref(), Some(&tabs[expected]));
                assert!(app.modal.is_none());
                assert!(app.sidebar_collapsed);
                assert!(app.thread_reply.read(cx).value().is_empty());
                assert_eq!(
                    app.drafts
                        .get("thread-navigation-draft")
                        .map(String::as_str),
                    Some("Keep this thread reply")
                );
            });
        }
    }
    cx.update_window(handle, |_, window, cx| {
        window.press("cmd-k", cx);
        assert_eq!(
            app.read(cx).prompt.read(cx).value(),
            "Keep this review question"
        );
        window.press("escape", cx);
    })?;
    println!(
        "Project checks passed: folder-only add, project branch picker, pinned tabs, automatic checkout following, detached/attached HEAD, prompt preservation, keyboard tab cycling in both directions across projects with wraparound and a hidden sidebar."
    );
    Ok(())
}
