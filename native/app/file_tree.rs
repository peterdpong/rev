use crate::model::File;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

/// Rebuild structure only when files/filter change, and rows only on expansion changes.
#[derive(Default)]
pub(super) struct FileTreeCache {
    files: Vec<File>,
    file_indices: HashMap<String, usize>,
    tree: FileTree,
    filter: String,
    rows: Rc<Vec<TreeEntry>>,
    dirty: bool,
    pub added: usize,
    pub removed: usize,
}

impl FileTreeCache {
    pub fn set_files(&mut self, files: &[File]) {
        if self.files == files {
            return;
        }
        self.files = files.to_vec();
        self.file_indices = files
            .iter()
            .enumerate()
            .map(|(index, file)| (file.path.clone(), index))
            .collect();
        self.added = files.iter().map(|file| file.added).sum();
        self.removed = files.iter().map(|file| file.removed).sum();
        self.rebuild();
    }

    pub fn file(&self, path: &str) -> Option<&File> {
        self.file_indices.get(path).map(|&index| &self.files[index])
    }

    pub fn set_filter(&mut self, filter: &str) {
        let filter = filter.to_lowercase();
        if self.filter != filter {
            self.filter = filter;
            self.rebuild();
        }
    }

    fn rebuild(&mut self) {
        let files: Vec<_> = self
            .files
            .iter()
            .filter(|file| {
                self.filter.is_empty() || file.path.to_lowercase().contains(&self.filter)
            })
            .cloned()
            .collect();
        self.tree = FileTree::new(&files);
        self.invalidate_rows();
    }

    pub fn invalidate_rows(&mut self) {
        self.dirty = true;
    }

    pub fn entries(&mut self, collapsed: Option<&HashSet<String>>) -> Rc<Vec<TreeEntry>> {
        if self.dirty {
            self.rows = Rc::new(self.tree.entries(collapsed));
            self.dirty = false;
        }
        self.rows.clone()
    }
}

#[derive(Default)]
pub(super) struct FileTree {
    folders: BTreeMap<String, FileTree>,
    files: BTreeMap<String, File>,
}

pub(super) struct TreeEntry {
    pub path: String,
    pub label: String,
    pub depth: usize,
    pub kind: EntryKind,
}

pub(super) enum EntryKind {
    Folder { expanded: bool },
    File(File),
}

impl FileTree {
    pub fn new(files: &[File]) -> Self {
        let mut root = Self::default();
        for file in files {
            // Git paths always use '/', including when a filename contains '\\'.
            let mut parts = file.path.split('/').peekable();
            let mut folder = &mut root;
            while let Some(part) = parts.next() {
                if parts.peek().is_some() {
                    folder = folder.folders.entry(part.to_owned()).or_default();
                } else {
                    folder.files.insert(part.to_owned(), file.clone());
                }
            }
        }
        root
    }

    pub fn entries(&self, collapsed: Option<&HashSet<String>>) -> Vec<TreeEntry> {
        let mut entries = Vec::new();
        self.append_entries("", 0, collapsed, &mut entries);
        entries
    }

    fn append_entries(
        &self,
        parent: &str,
        depth: usize,
        collapsed: Option<&HashSet<String>>,
        entries: &mut Vec<TreeEntry>,
    ) {
        for (name, folder) in &self.folders {
            let path = if parent.is_empty() {
                name.clone()
            } else {
                format!("{parent}/{name}")
            };
            let expanded = !collapsed.is_some_and(|paths| paths.contains(&path));
            entries.push(TreeEntry {
                path: path.clone(),
                label: name.clone(),
                depth,
                kind: EntryKind::Folder { expanded },
            });
            if expanded {
                folder.append_entries(&path, depth + 1, collapsed, entries);
            }
        }
        for (name, file) in &self.files {
            entries.push(TreeEntry {
                path: file.path.clone(),
                label: name.clone(),
                depth,
                kind: EntryKind::File(file.clone()),
            });
        }
    }
}

pub(super) fn reveal_file(collapsed: &mut HashSet<String>, path: &str) -> bool {
    let mut changed = false;
    for (index, _) in path.match_indices('/') {
        changed |= collapsed.remove(&path[..index]);
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str) -> File {
        File {
            path: path.into(),
            status: "M".into(),
            added: 3,
            removed: 1,
        }
    }

    #[test]
    fn folders_come_first_and_keep_full_paths_for_duplicate_basenames() {
        let tree = FileTree::new(&[
            file("README.md"),
            file("tests/unit/app.rs"),
            file("src/z.rs"),
            file("src/app.rs"),
        ]);
        let entries = tree.entries(None);
        let paths: Vec<_> = entries
            .iter()
            .map(|entry| (entry.path.as_str(), entry.depth))
            .collect();
        assert_eq!(
            paths,
            [
                ("src", 0),
                ("src/app.rs", 1),
                ("src/z.rs", 1),
                ("tests", 0),
                ("tests/unit", 1),
                ("tests/unit/app.rs", 2),
                ("README.md", 0),
            ]
        );
        assert_eq!(entries[1].label, "app.rs");
        assert_eq!(entries[5].label, "app.rs");
        let EntryKind::File(file) = &entries[1].kind else {
            panic!("expected file")
        };
        assert_eq!(
            (file.status.as_str(), file.added, file.removed),
            ("M", 3, 1)
        );
    }

    #[test]
    fn collapsing_a_folder_hides_only_its_descendants() {
        let tree = FileTree::new(&[file("src/nested/a.rs"), file("src2/b.rs"), file("root.rs")]);
        let collapsed = HashSet::from(["src".into()]);
        let entries = tree.entries(Some(&collapsed));
        assert!(matches!(
            entries[0].kind,
            EntryKind::Folder { expanded: false }
        ));
        let paths: Vec<_> = entries.iter().map(|entry| entry.path.as_str()).collect();
        assert_eq!(paths, ["src", "src2", "src2/b.rs", "root.rs"]);
    }

    #[test]
    fn filtered_files_keep_their_ancestor_folders() {
        let files = [file("src/nested/match.rs"), file("tests/other.rs")];
        let matches: Vec<_> = files
            .into_iter()
            .filter(|file| file.path.contains("match"))
            .collect();
        let entries = FileTree::new(&matches).entries(None);
        let paths: Vec<_> = entries.iter().map(|entry| entry.path.as_str()).collect();
        assert_eq!(paths, ["src", "src/nested", "src/nested/match.rs"]);
        assert!(FileTree::new(&[]).entries(None).is_empty());
    }

    #[test]
    fn revealing_a_file_opens_ancestors_without_changing_other_folders() {
        let mut collapsed = HashSet::from(["src".into(), "src/nested".into(), "src2".into()]);
        reveal_file(&mut collapsed, "src/nested/a.rs");
        assert_eq!(collapsed, HashSet::from(["src2".into()]));
    }

    #[test]
    fn filenames_with_spaces_unicode_and_backslashes_are_preserved() {
        let entries = FileTree::new(&[file("données/my file\\name.rs")]).entries(None);
        assert_eq!(entries[1].label, "my file\\name.rs");
        assert_eq!(entries[1].path, "données/my file\\name.rs");
    }

    #[test]
    fn large_tree_reuses_cached_rows_until_files_filter_or_expansion_change() {
        let mut files: Vec<_> = (0..20_000)
            .map(|i| file(&format!("src/group{:03}/file{i:05}.rs", i / 100)))
            .collect();
        let mut cache = FileTreeCache::default();
        cache.set_files(&files);
        let all = cache.entries(None);
        assert_eq!(all.len(), 20_201);
        assert_eq!((cache.added, cache.removed), (60_000, 20_000));
        for _ in 0..100 {
            assert!(Rc::ptr_eq(&all, &cache.entries(None)));
        }
        cache.set_files(&files);
        assert!(Rc::ptr_eq(&all, &cache.entries(None)));

        let collapsed = HashSet::from(["src/group000".into()]);
        cache.invalidate_rows();
        let folded = cache.entries(Some(&collapsed));
        assert_eq!(folded.len(), all.len() - 100);
        assert!(!Rc::ptr_eq(&all, &folded));
        assert!(Rc::ptr_eq(&folded, &cache.entries(Some(&collapsed))));

        cache.set_filter("FILE00042");
        let filtered = cache.entries(None);
        assert_eq!(filtered.len(), 3);
        assert_eq!(filtered[2].path, "src/group000/file00042.rs");
        cache.set_filter("file00042");
        assert!(Rc::ptr_eq(&filtered, &cache.entries(None)));

        files[42].added = 9;
        cache.set_files(&files);
        let updated = cache.entries(None);
        let EntryKind::File(updated_file) = &updated[2].kind else {
            panic!("expected file")
        };
        assert_eq!(updated_file.added, 9);
        assert_eq!(cache.added, 60_006);
        cache.set_filter("");
        assert_eq!(cache.entries(Some(&collapsed)).len(), folded.len());
        cache.set_files(&[]);
        assert!(cache.entries(None).is_empty());
        assert_eq!((cache.added, cache.removed), (0, 0));
    }
}
