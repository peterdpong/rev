use crate::workspace_state::WorkspaceState;
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

const CLIENT_STATE_FILE_NAME: &str = "client.json";

/// Client state saved as client.json in the app data directory.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ClientState {
    #[serde(default)]
    pub workspace: WorkspaceState,
}

/// `REV_DATA_DIR`, or `~/.rev`.
pub fn data_dir() -> PathBuf {
    std::env::var_os("REV_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".rev"))
}

impl ClientState {
    pub fn load(dir: &Path) -> io::Result<Self> {
        let file = dir.join(CLIENT_STATE_FILE_NAME);
        let text = match fs::read_to_string(&file) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(error),
        };

        match serde_json::from_str::<Self>(&text) {
            Ok(mut state) => {
                state.workspace.normalize();
                Ok(state)
            }
            Err(_) => {
                fs::remove_file(&file)?;

                Ok(Self::default())
            }
        }
    }

    pub fn save(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        let file = dir.join(CLIENT_STATE_FILE_NAME);
        let temporary = dir.join(format!(
            "{CLIENT_STATE_FILE_NAME}.{}.tmp",
            std::process::id()
        ));

        let result = write_private(&temporary, &serde_json::to_vec_pretty(self)?)
            .and_then(|()| fs::rename(&temporary, &file));

        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }

        result
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);

    // New files are owner-only on Unix, for other platforms use default permissions.
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model::{CommitRange, ReviewRequest},
        workspace_state::{Project, ReviewTab, TabKind},
    };

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rev-client-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_missing_file_loads_as_defaults() {
        let dir = temp_dir("missing");
        assert_eq!(ClientState::load(&dir).unwrap(), ClientState::default());
    }

    #[test]
    fn saved_workspace_restores_the_active_review() {
        let dir = temp_dir("round-trip");
        let request = ReviewRequest {
            repo: "/a".into(),
            base: Some("main".into()),
            head: Some("feature".into()),
            mode: Some("branch".into()),
            commit_range: Some(CommitRange {
                start: "abc123".into(),
                end: "def456".into(),
            }),
        };
        let mut state = ClientState::default();
        state.workspace.projects.push(Project {
            repo: "/a".into(),
            name: "a".into(),
            collapsed: true,
            tabs: vec![ReviewTab {
                id: "tab".into(),
                kind: TabKind::Branch,
                label: "feature".into(),
                request: request.clone(),
                scope_key: Some("a:feature:branch".into()),
                head_sha: None,
                file: Some("src/main.rs".into()),
            }],
        });
        state.workspace.active_tab_id = Some("tab".into());
        state.workspace.normalize();
        state.save(&dir).unwrap();

        let loaded = ClientState::load(&dir).unwrap();
        assert_eq!(loaded, state);
        assert_eq!(loaded.workspace.active_tab().unwrap().request, request);
        let saved: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join(CLIENT_STATE_FILE_NAME)).unwrap()).unwrap();
        assert!(saved.get("workspace").is_some());
        assert_eq!(saved.as_object().unwrap().len(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn legacy_repositories_gain_a_permanent_working_tree_on_load() {
        let dir = temp_dir("legacy-working-tree");
        fs::create_dir_all(&dir).unwrap();
        let json = serde_json::json!({ "workspace": {
            "projects": [{ "repo": "/a", "name": "a", "collapsed": true, "tabs": [] }]
        }});
        fs::write(
            dir.join(CLIENT_STATE_FILE_NAME),
            serde_json::to_vec(&json).unwrap(),
        )
        .unwrap();
        let state = ClientState::load(&dir).unwrap();
        let project = &state.workspace.projects[0];
        assert!(project.collapsed);
        assert_eq!(project.tabs.len(), 1);
        assert_eq!(project.tabs[0].kind, TabKind::Current);
        assert_eq!(project.tabs[0].request, ReviewRequest::working_tree("/a"));
        assert_eq!(
            state.workspace.active_tab_id.as_deref(),
            Some(project.tabs[0].id.as_str())
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_corrupt_file_is_deleted_and_loads_as_defaults() {
        let dir = temp_dir("corrupt");
        fs::create_dir_all(&dir).unwrap();
        for text in ["{ not json", r#"{"workspace":{"projects":false}}"#] {
            fs::write(dir.join(CLIENT_STATE_FILE_NAME), text).unwrap();

            assert_eq!(ClientState::load(&dir).unwrap(), ClientState::default());
            assert!(fs::read_dir(&dir).unwrap().next().is_none());
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_read_error_is_returned_without_deleting_state() {
        let dir = temp_dir("read-error");
        let file = dir.join(CLIENT_STATE_FILE_NAME);
        fs::create_dir_all(&file).unwrap();

        assert!(ClientState::load(&dir).is_err());
        assert!(file.is_dir());
        fs::remove_dir_all(&dir).unwrap();
    }
}
