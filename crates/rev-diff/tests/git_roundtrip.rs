//! Exercise the parser against Git's actual wire format, not only hand-written
//! patches. Everything lives in a disposable repository with local config.
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use rev_diff::{File, Selection, Side, Status};

struct Repository(PathBuf);
impl Repository {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let repo =
            Self(std::env::temp_dir().join(format!("rev-diff-{}-{nonce}", std::process::id())));
        fs::create_dir_all(&repo.0).unwrap();
        repo.git(&["init", "-q"]);
        repo.git(&["config", "core.autocrlf", "false"]);
        repo.git(&["config", "core.quotePath", "true"]);
        repo
    }
    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.0)
            .args([
                "-c",
                "user.name=Renderer Test",
                "-c",
                "user.email=renderer@example.invalid",
                "-c",
                "commit.gpgSign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }
    fn write(&self, path: &str, bytes: &[u8]) {
        fs::write(self.0.join(path), bytes).unwrap();
    }
}
impl Drop for Repository {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn real_git_patches_reconstruct_both_sources_exactly() {
    let repo = Repository::new();
    let mut sources = BTreeMap::new();
    for i in 0..30 {
        let name = format!("café {i}\t.rs");
        let old: String = (0..40)
            .map(|line| {
                format!(
                    "old_{i}_{line} λ\t{}",
                    if line == 39 {
                        ""
                    } else if line % 2 == 0 {
                        "\r\n"
                    } else {
                        "\n"
                    }
                )
            })
            .collect();
        let new: String = (0..40)
            .filter(|line| (line + i) % 7 != 0)
            .map(|line| {
                if (line + i) % 3 == 0 {
                    format!("new_{i}_{line} 👍\n")
                } else {
                    format!(
                        "old_{i}_{line} λ\t{}",
                        if line == 39 {
                            ""
                        } else if line % 2 == 0 {
                            "\r\n"
                        } else {
                            "\n"
                        }
                    )
                }
            })
            .collect();
        repo.write(&name, old.as_bytes());
        sources.insert(name, (old, new));
    }
    repo.write("deleted.txt", b"removed\nsecond");
    repo.write("before name", b"unchanged rename\n");
    repo.write("binary", b"before\0bytes");
    repo.git(&["add", "--all"]);
    repo.git(&["commit", "-qm", "base"]);
    for (name, (_, new)) in &sources {
        repo.write(name, new.as_bytes());
    }
    fs::remove_file(repo.0.join("deleted.txt")).unwrap();
    fs::rename(repo.0.join("before name"), repo.0.join("after name")).unwrap();
    repo.write("binary", b"after\0bytes");
    repo.write("added.txt", b"added\nno final newline");
    sources.insert(
        "added.txt".into(),
        ("".into(), "added\nno final newline".into()),
    );
    sources.insert("deleted.txt".into(), ("removed\nsecond".into(), "".into()));
    repo.git(&["add", "--all"]);
    let patch = repo.git(&[
        "diff",
        "--cached",
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
        "--unified=999999",
        "--find-renames",
    ]);
    let files = File::parse(&patch).unwrap();
    assert_eq!(files.len(), sources.len() + 2);
    assert!(
        files
            .iter()
            .any(|file| file.path() == "after name" && file.status() == Status::Renamed)
    );
    assert!(
        files
            .iter()
            .any(|file| file.path() == "binary" && file.status() == Status::Binary)
    );
    for file in &files {
        let Some((old, new)) = sources.get(file.path()) else {
            continue;
        };
        for (side, expected) in [(Side::Original, old), (Side::Modified, new)] {
            let map = file.source_map(side);
            if expected.is_empty() {
                assert!(map.is_empty());
                continue;
            }
            let selection = Selection::new(
                file.path().clone(),
                side,
                map.first().unwrap().0,
                map.last().unwrap().0,
            );
            assert_eq!(
                &file.copy(&selection).unwrap(),
                expected,
                "{} {side:?}",
                file.path()
            );
        }
    }
}
