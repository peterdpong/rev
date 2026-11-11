use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::Arc,
};

use crate::{File, LineKind, Position, Side};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Unified,
    Split,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    Header,
    Hunk(usize),
    Code {
        left: Option<usize>,
        right: Option<usize>,
    },
    Fold(Range<usize>),
    Notice,
}

impl Row {
    pub fn line(&self, side: Side) -> Option<usize> {
        match (self, side) {
            (Self::Code { left, .. }, Side::Original) => *left,
            (Self::Code { right, .. }, Side::Modified) => *right,
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Anchor {
    pub path: String,
    pub position: Option<Position>,
    pub local_row: usize,
}

/// The one interval a viewport needs to invalidate after a file changes.
#[derive(Clone, Debug)]
pub struct RowEdit {
    pub old: Range<usize>,
    pub new_len: usize,
}

struct Section {
    file: Arc<File>,
    rows: Arc<[Row]>,
    source_rows: Vec<usize>,
    collapsed: bool,
    expanded: HashSet<usize>,
}

impl Section {
    fn new(file: File, mode: Mode) -> Self {
        let mut section = Self {
            file: Arc::new(file),
            rows: Arc::from([]),
            source_rows: Vec::new(),
            collapsed: false,
            expanded: HashSet::new(),
        };
        section.layout(mode);
        section
    }

    fn layout(&mut self, mode: Mode) {
        let mut rows = vec![Row::Header];
        self.source_rows = vec![0; self.file.lines().len()];
        if !self.collapsed {
            if self.file.message().is_some() {
                rows.push(Row::Notice);
            }
            for (hunk_index, hunk) in self.file.hunks().iter().enumerate() {
                rows.push(Row::Hunk(hunk_index));
                let mut i = hunk.lines.start;
                while i < hunk.lines.end {
                    let start = i;
                    let context = self.file.lines()[i].kind == LineKind::Context;
                    while i < hunk.lines.end
                        && (self.file.lines()[i].kind == LineKind::Context) == context
                    {
                        i += 1;
                    }
                    if context && i - start > 12 && !self.expanded.contains(&start) {
                        for line in start..start + 3 {
                            rows.push(Row::Code {
                                left: Some(line),
                                right: Some(line),
                            });
                        }
                        rows.push(Row::Fold(start + 3..i - 3));
                        for line in i - 3..i {
                            rows.push(Row::Code {
                                left: Some(line),
                                right: Some(line),
                            });
                        }
                    } else if context || mode == Mode::Unified {
                        for line in start..i {
                            rows.push(Row::Code {
                                left: self.file.lines()[line].old.map(|_| line),
                                right: self.file.lines()[line].new.map(|_| line),
                            });
                        }
                    } else {
                        // Pair changed runs in source order. No second diff algorithm or
                        // hidden similarity heuristic determines the review's alignment.
                        let old: Vec<_> = (start..i)
                            .filter(|&line| self.file.lines()[line].old.is_some())
                            .collect();
                        let new: Vec<_> = (start..i)
                            .filter(|&line| self.file.lines()[line].new.is_some())
                            .collect();
                        for pair in 0..old.len().max(new.len()) {
                            rows.push(Row::Code {
                                left: old.get(pair).copied(),
                                right: new.get(pair).copied(),
                            });
                        }
                    }
                }
            }
        }
        for (index, row) in rows.iter().enumerate() {
            match row {
                Row::Code { left, right } => {
                    for &line in left.iter().chain(right.iter()) {
                        self.source_rows[line] = index;
                    }
                }
                Row::Fold(range) => {
                    for line in range.clone() {
                        self.source_rows[line] = index;
                    }
                }
                _ => {}
            }
        }
        self.rows = rows.into();
    }
}

/// File-length prefix sums. Updating one file and locating a screen row both
/// cost O(log files); rows belonging to other files are never copied or rebuilt.
#[derive(Default)]
struct Counts {
    tree: Vec<usize>,
}

impl Counts {
    fn new(lengths: impl Iterator<Item = usize>) -> Self {
        let lengths: Vec<_> = lengths.collect();
        let mut result = Self {
            tree: vec![0; lengths.len() + 1],
        };
        for (i, length) in lengths.into_iter().enumerate() {
            result.change(i, 0, length);
        }
        result
    }
    fn change(&mut self, index: usize, old: usize, new: usize) {
        let mut i = index + 1;
        while i < self.tree.len() {
            self.tree[i] = self.tree[i] - old + new;
            i += i & i.wrapping_neg();
        }
    }
    fn before(&self, index: usize) -> usize {
        let mut i = index;
        let mut result = 0;
        while i > 0 {
            result += self.tree[i];
            i &= i - 1;
        }
        result
    }
    fn locate(&self, row: usize) -> Option<(usize, usize)> {
        if self.tree.len() <= 1 || row >= self.before(self.tree.len() - 1) {
            return None;
        }
        let mut file = 0;
        let mut preceding = 0;
        let mut step = self.tree.len().next_power_of_two() >> 1;
        while step > 0 {
            let next = file + step;
            if next < self.tree.len() && preceding + self.tree[next] <= row {
                preceding += self.tree[next];
                file = next;
            }
            step >>= 1;
        }
        Some((file, row - preceding))
    }
}

#[derive(Default)]
pub struct Review {
    sections: Vec<Section>,
    paths: HashMap<String, usize>,
    counts: Counts,
    mode: Mode,
}

impl Review {
    pub fn new(files: impl IntoIterator<Item = File>, mode: Mode) -> Self {
        let sections: Vec<_> = files
            .into_iter()
            .map(|file| Section::new(file, mode))
            .collect();
        let paths = sections
            .iter()
            .enumerate()
            .map(|(index, section)| (section.file.path().clone(), index))
            .collect();
        let counts = Counts::new(sections.iter().map(|section| section.rows.len()));
        Self {
            sections,
            paths,
            counts,
            mode,
        }
    }
    pub fn len(&self) -> usize {
        self.counts.before(self.sections.len())
    }
    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }
    pub fn mode(&self) -> Mode {
        self.mode
    }
    pub fn file(&self, index: usize) -> &Arc<File> {
        &self.sections[index].file
    }
    pub fn file_count(&self) -> usize {
        self.sections.len()
    }
    pub fn file_index(&self, path: &str) -> Option<usize> {
        self.paths.get(path).copied()
    }
    pub fn file_start(&self, index: usize) -> usize {
        self.counts.before(index)
    }
    pub fn rows(&self, index: usize) -> &Arc<[Row]> {
        &self.sections[index].rows
    }
    pub fn is_collapsed(&self, path: &str) -> bool {
        self.file_index(path)
            .is_some_and(|index| self.sections[index].collapsed)
    }
    pub fn row(&self, row: usize) -> Option<(usize, &Row)> {
        let (file, local) = self.counts.locate(row)?;
        Some((file, &self.sections[file].rows[local]))
    }
    fn rebuild(&mut self, file: usize) -> RowEdit {
        let start = self.file_start(file);
        let old = self.sections[file].rows.len();
        self.sections[file].layout(self.mode);
        let new_len = self.sections[file].rows.len();
        self.counts.change(file, old, new_len);
        RowEdit {
            old: start..start + old,
            new_len,
        }
    }
    pub fn replace(&mut self, file: File) -> Option<RowEdit> {
        let index = self.file_index(file.path())?;
        self.sections[index].file = Arc::new(file);
        self.sections[index].expanded.clear();
        Some(self.rebuild(index))
    }
    pub fn collapse(&mut self, path: &str, collapsed: bool) -> Option<RowEdit> {
        let index = self.file_index(path)?;
        if self.sections[index].collapsed == collapsed {
            return None;
        }
        self.sections[index].collapsed = collapsed;
        Some(self.rebuild(index))
    }
    pub fn expand_fold(&mut self, file: usize, hidden_start: usize) -> RowEdit {
        self.sections[file]
            .expanded
            .insert(hidden_start.saturating_sub(3));
        self.rebuild(file)
    }
    pub fn set_mode(&mut self, mode: Mode) -> bool {
        if mode == self.mode {
            return false;
        }
        self.mode = mode;
        for section in &mut self.sections {
            section.layout(mode);
        }
        self.counts = Counts::new(self.sections.iter().map(|section| section.rows.len()));
        true
    }
    pub fn position(&self, row: usize, side: Side) -> Option<Position> {
        let (file, row) = self.row(row)?;
        let line = row.line(side)?;
        Some(Position::new(
            self.file(file).path().clone(),
            side,
            self.file(file).lines()[line].number(side)?,
        ))
    }
    pub fn source_row(&self, position: &Position) -> Option<usize> {
        let file = self.file_index(position.path())?;
        let line = self
            .file(file)
            .line_index(position.side(), position.line())?;
        Some(self.file_start(file) + self.sections[file].source_rows[line])
    }
    pub fn anchor(&self, row: usize) -> Option<Anchor> {
        let (file, _) = self.row(row)?;
        Some(Anchor {
            path: self.file(file).path().clone(),
            position: self
                .position(row, Side::Modified)
                .or_else(|| self.position(row, Side::Original)),
            local_row: row - self.file_start(file),
        })
    }
    pub fn resolve(&self, anchor: &Anchor) -> Option<usize> {
        let file = self.file_index(&anchor.path)?;
        if let Some(row) = anchor
            .position
            .as_ref()
            .and_then(|position| self.source_row(position))
        {
            return Some(row);
        }
        Some(self.file_start(file) + anchor.local_row.min(self.sections[file].rows.len() - 1))
    }

    /// Clamp a drag to the anchor's file and side, including when the pointer
    /// crosses another file, a blank split lane, or a hunk separator.
    pub fn drag_target(&self, anchor: &Position, target: usize) -> Option<Position> {
        let file = self.file_index(anchor.path())?;
        let section = &self.sections[file];
        let start = self.file_start(file);
        let local = target.saturating_sub(start).min(section.rows.len() - 1);
        let map = section.file.source_map(anchor.side());
        let insertion = map.partition_point(|&(_, line)| section.source_rows[line] < local);
        let &(number, _) = map.get(insertion).or_else(|| map.last())?;
        Some(Position::new(anchor.path().clone(), anchor.side(), number))
    }
}
