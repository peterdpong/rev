use crate::navigation::Navigation;
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const FILE: &str = "client.json";

/// App state for the client, saved as `client.json` in the app data directory.
/// Anything an agent reviewing the repo would need belongs in the core's
/// review state instead.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ClientState {
    #[serde(default)]
    pub navigation: Navigation,
}

/// `REV_DATA_DIR`, or `~/.rev`.
pub fn data_dir() -> PathBuf {
    std::env::var_os("REV_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".rev"))
}

impl ClientState {
    /// Read saved state, or the defaults when there is none. A file that no
    /// longer parses is moved aside so the next save doesn't replace it.
    pub fn load(dir: &Path) -> io::Result<Self> {
        let file = dir.join(FILE);
        let text = match fs::read_to_string(&file) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(error),
        };
        match serde_json::from_str(&text) {
            Ok(state) => Ok(state),
            Err(_) => {
                let millis = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis());
                fs::rename(&file, dir.join(format!("{FILE}.corrupt-{millis}")))?;
                Ok(Self::default())
            }
        }
    }

    /// Replace the saved state atomically: write a temporary file, then rename it.
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        let file = dir.join(FILE);
        let temporary = dir.join(format!("{FILE}.{}.tmp", std::process::id()));
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
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::navigation::Project;

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
    fn saved_state_loads_back() {
        let dir = temp_dir("round-trip");
        let mut state = ClientState::default();
        state.navigation.projects.push(Project {
            repo: "/a".into(),
            name: "a".into(),
            collapsed: true,
            tabs: vec![],
        });
        state.navigation.active_tab_id = Some("tab".into());
        state.save(&dir).unwrap();

        assert_eq!(ClientState::load(&dir).unwrap(), state);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_corrupt_file_is_moved_aside() {
        let dir = temp_dir("corrupt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(FILE), "{ not json").unwrap();

        assert_eq!(ClientState::load(&dir).unwrap(), ClientState::default());
        let names: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            names
                .iter()
                .any(|name| name.starts_with("client.json.corrupt-"))
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}
