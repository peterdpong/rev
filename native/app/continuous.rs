use super::*;

pub(super) enum PendingFileNavigation {
    History {
        path: String,
        position: DiffScrollPosition,
    },
}

impl PendingFileNavigation {
    fn path(&self) -> &str {
        match self {
            Self::History { path, .. } => path,
        }
    }
}

pub(super) fn prepare_patches(
    entries: Vec<ReviewPatch>,
) -> (HashMap<String, FilePatch>, Vec<DiffFile>) {
    let mut patches = HashMap::new();
    let mut files = Vec::with_capacity(entries.len());
    for entry in entries {
        let path = entry.path;
        let mut patch = entry.patch;
        let file = if let Some(error) = entry.error {
            DiffFile::placeholder(path.clone(), format!("Could not load this diff: {error}"))
        } else if patch.deferred {
            DiffFile::placeholder(path.clone(), "Large diff. Expand this file to load it.")
        } else if let Some(notice) = patch.notice() {
            DiffFile::placeholder(path.clone(), notice)
        } else {
            match DiffFile::parse(&patch.patch) {
                Ok(files) => files
                    .into_iter()
                    .find(|file| file.path().as_str() == path)
                    .unwrap_or_else(|| DiffFile::placeholder(path.clone(), "No textual changes")),
                Err(error) => DiffFile::placeholder(
                    path.clone(),
                    format!("Could not display this diff: {error}"),
                ),
            }
        };
        // DiffFile owns the parsed source. Keep only the digest and load status in the cache.
        patch.patch = String::new();
        patches.insert(path, patch);
        files.push(file);
    }
    (patches, files)
}

#[cfg(test)]
mod tests {
    use super::prepare_patches;
    use crate::model::{FilePatch, ReviewPatch};

    #[test]
    fn prepares_real_source_and_unavailable_files_without_losing_paths_or_digests() {
        let entries = vec![
            ReviewPatch { path: "line\nbreak.ts".into(), patch: FilePatch {
                patch: "diff --git \"a/line\\012break.ts\" \"b/line\\012break.ts\"\nnew file mode 100644\n--- /dev/null\n+++ \"b/line\\012break.ts\"\n@@ -0,0 +1 @@\n+hello\n".into(),
                digest: "text-digest".into(), too_large: false, deferred: false,
            }, error: None },
            ReviewPatch { path: "binary.bin".into(), patch: FilePatch { patch: "Binary file (untracked)".into(), digest: "binary-digest".into(), too_large: false, deferred: false }, error: None },
            ReviewPatch { path: "deferred.txt".into(), patch: FilePatch { deferred: true, ..Default::default() }, error: None },
            ReviewPatch { path: "large.txt".into(), patch: FilePatch { too_large: true, ..Default::default() }, error: None },
            ReviewPatch { path: "unavailable.txt".into(), patch: FilePatch::default(), error: Some("File is unavailable".into()) },
        ];
        let (patches, files) = prepare_patches(entries);
        assert_eq!(
            files
                .iter()
                .map(|file| file.path().as_str())
                .collect::<Vec<_>>(),
            [
                "line\nbreak.ts",
                "binary.bin",
                "deferred.txt",
                "large.txt",
                "unavailable.txt"
            ]
        );
        assert_eq!(files[0].additions(), 1);
        assert!(
            files[1..]
                .iter()
                .all(|file| file.additions() == 0 && file.deletions() == 0)
        );
        assert_eq!(patches["line\nbreak.ts"].digest, "text-digest");
        assert_eq!(patches["binary.bin"].digest, "binary-digest");
        assert!(patches["deferred.txt"].deferred);
        assert!(!patches["deferred.txt"].too_large);
        assert!(patches["large.txt"].too_large);
        assert!(patches.values().all(|patch| patch.patch.is_empty()));
    }
}

impl Rev {
    pub(super) fn is_file_viewed(&self, path: &str) -> bool {
        self.patches.get(path).is_some_and(|patch| {
            !patch.digest.is_empty() && self.state.viewed.get(path) == Some(&patch.digest)
        })
    }

    pub(super) fn set_active_file(&mut self, path: String, cx: &mut Context<Self>) {
        if self.file.as_ref() == Some(&path) {
            return;
        }
        self.pending_file_navigation = None;
        self.patch = self.patches.get(&path).cloned().unwrap_or_default();
        self.file = Some(path);
        cx.notify();
    }

    pub(super) fn load_patches(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(review), Some(backend)) = (self.review.as_ref(), self.backend.clone()) else {
            return;
        };
        let params = json!({ "review": review.request, "headSha": review.head_sha, "mergeBase": review.merge_base, "preview": true });
        let preserve_scroll = !self.diff_state(cx).read(cx).files().is_empty();
        self.patch_loading = true;
        self.loading_patches.clear();
        self.pending_file_navigation = None;
        self.file_epoch += 1;
        let epoch = self.file_epoch;
        let task = cx.background_executor().spawn(async move {
            backend
                .call("patches", params)
                .and_then(decode::<Vec<ReviewPatch>>)
                .map(prepare_patches)
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.file_epoch != epoch {
                    return;
                }
                this.patch_loading = false;
                match result {
                    Ok((patches, files)) => {
                        this.patches = patches;
                        this.diff_state(cx).update(cx, |diff, cx| {
                            let scroll = preserve_scroll.then(|| diff.scroll_position()).flatten();
                            diff.set_files(files, cx);
                            diff.set_files_collapsed(
                                this.patches.iter().filter_map(|(path, patch)| {
                                    (patch.deferred || this.is_file_viewed(path))
                                        .then_some(path.as_str())
                                }),
                                true,
                                cx,
                            );
                            if let Some(scroll) = scroll {
                                diff.restore_scroll_position(&scroll, cx);
                            }
                        });
                        if let Some(path) = this.file.clone() {
                            this.patch = this.patches.get(&path).cloned().unwrap_or_default();
                            if !preserve_scroll {
                                if this.patches.get(&path).is_some_and(|patch| patch.deferred)
                                    || this.is_file_viewed(&path)
                                {
                                    this.diff_state(cx).update(cx, |diff, cx| diff.scroll_to_file(&path, cx));
                                } else {
                                    this.reveal_file(path, window, cx);
                                }
                            }
                        }
                        this.restore_navigation(window, cx);
                    }
                    Err(error) => {
                        this.navigation.restore = None;
                        this.error = Some(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn load_file_patch(
        &mut self,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.loading
            || self.patch_loading
            || !self.patches.get(&path).is_some_and(|patch| patch.deferred)
            || self.loading_patches.contains(&path)
        {
            return;
        }
        let (Some(review), Some(backend)) = (self.review.as_ref(), self.backend.clone()) else {
            return;
        };
        let params = json!({ "review": review.request, "path": path,
            "headSha": review.head_sha, "mergeBase": review.merge_base,
            "snapshotId": self.review_snapshot_id });
        self.loading_patches.insert(path.clone());
        let epoch = self.file_epoch;
        let requested = path.clone();
        let task = cx.background_executor().spawn(async move {
            let reply = backend.call("patch", params).and_then(decode::<FilePatch>);
            let entry = match reply {
                Ok(patch) => ReviewPatch {
                    path: requested,
                    patch,
                    error: None,
                },
                Err(error) => ReviewPatch {
                    path: requested,
                    patch: FilePatch {
                        deferred: true,
                        ..Default::default()
                    },
                    error: Some(error),
                },
            };
            prepare_patches(vec![entry])
        });
        self.diff_state(cx).update(cx, |diff, cx| {
            diff.replace_file(DiffFile::placeholder(path.clone(), "Loading diff…"), cx);
        });
        cx.spawn_in(window, async move |this, cx| {
            let (mut patches, mut files) = task.await;
            let _ = this.update_in(cx, |this, _, cx| {
                // Switching reviews or refreshing invalidates in-flight file loads.
                if this.file_epoch != epoch {
                    return;
                }
                this.loading_patches.remove(&path);
                if let (Some(patch), Some(file)) = (patches.remove(&path), files.pop()) {
                    let deferred = patch.deferred;
                    let loaded = !patch.digest.is_empty();
                    if this.file.as_ref() == Some(&path) {
                        this.patch = patch.clone();
                    }
                    this.patches.insert(path.clone(), patch);
                    // Update just this file; preserve other folds and the viewport.
                    this.diff_state(cx).update(cx, |diff, cx| {
                        diff.replace_file(file, cx);
                    });
                    if !deferred
                        && this
                            .pending_file_navigation
                            .as_ref()
                            .is_some_and(|pending| pending.path() == path)
                    {
                        let pending = this.pending_file_navigation.take().unwrap();
                        if loaded {
                            match pending {
                                PendingFileNavigation::History { position, .. } => {
                                    this.diff_state(cx).update(cx, |diff, cx| {
                                        diff.restore_scroll_position(&position, cx)
                                    });
                                }
                            }
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn jump_file(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        let previous = self.navigation_location(cx);
        self.reveal_file(path, window, cx);
        self.record_file_navigation(previous, cx);
    }

    pub(super) fn reveal_file(
        &mut self,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(review) = self.review.as_ref() else {
            return;
        };
        if self.file_list.read(cx).file_counts(&path).is_none() {
            return;
        }
        let tab_id = self
            .workspace
            .active_tab_id
            .clone()
            .unwrap_or_else(|| review.key.clone());

        self.file_list.update(cx, |list, cx| list.select_file(&path, cx));

        self.hide_file_sheet(window, cx);
        self.set_active_file(path.clone(), cx);
        self.pending_file_navigation = None;
        self.load_file_patch(path.clone(), window, cx);
        gpui_kit::base::TextSelection::clear(window, cx);
        self.diff_state(cx).update(cx, |diff, cx| {
            diff.set_selected_lines(None, cx);
            diff.set_file_collapsed(&path, false, cx);
            diff.scroll_to_file(&path, cx);
        });
        self.workspace.set_file(&tab_id, path);
        self.save_workspace();
    }

    pub(super) fn mark_file_viewed(
        &mut self,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(review), Some(patch)) = (self.review.as_ref(), self.patches.get(&path)) else {
            return;
        };
        if self.loading || self.patch_loading || patch.too_large || patch.digest.is_empty() {
            return;
        }
        let epoch = self.review_epoch;
        let viewed = self.state.viewed.get(&path) != Some(&patch.digest);
        self.rpc("viewed", json!({ "review": review.request, "path": path, "viewed": viewed, "digest": patch.digest, "preview": true }), window, cx,
            move |this, result, _, cx| {
                if this.review_epoch != epoch { return; }
                if let Some(value) = this.result(result) {
                    match decode(value) {
                        Ok(state) => {
                            this.state = state;
                            this.diff_state(cx).update(cx, |diff, cx| diff.set_file_collapsed(&path, viewed, cx));
                        },
                        Err(error) => this.error = Some(error)
                    }
                }
            });
    }
}
