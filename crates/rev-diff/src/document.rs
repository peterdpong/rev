use std::{ops::Range, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Original,
    Modified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    Binary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Added,
    Removed,
}

#[derive(Clone, Debug)]
pub struct Line {
    pub kind: LineKind,
    pub old: Option<usize>,
    pub new: Option<usize>,
    pub(crate) bytes: Range<usize>,
    pub(crate) newline: bool,
}

impl Line {
    pub fn number(&self, side: Side) -> Option<usize> {
        match side {
            Side::Original => self.old,
            Side::Modified => self.new,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Hunk {
    pub header: String,
    pub lines: Range<usize>,
}

/// Source is shared once per parsed patch, including when a file is cloned.
#[derive(Clone, Debug)]
pub struct File {
    pub(crate) path: String,
    pub(crate) status: Status,
    pub(crate) source: Arc<str>,
    pub(crate) lines: Vec<Line>,
    pub(crate) hunks: Vec<Hunk>,
    pub(crate) message: Option<String>,
    pub(crate) additions: usize,
    pub(crate) deletions: usize,
    pub(crate) old_lines: Vec<(usize, usize)>,
    pub(crate) new_lines: Vec<(usize, usize)>,
}

impl File {
    pub fn parse(patch: &str) -> Result<Vec<Self>, crate::ParseError> {
        crate::parser::parse(patch)
    }

    pub fn placeholder(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            status: Status::Modified,
            source: Arc::from(""),
            lines: Vec::new(),
            hunks: Vec::new(),
            message: Some(message.into()),
            additions: 0,
            deletions: 0,
            old_lines: Vec::new(),
            new_lines: Vec::new(),
        }
    }

    pub fn path(&self) -> &String {
        &self.path
    }
    pub fn status(&self) -> Status {
        self.status
    }
    pub fn additions(&self) -> usize {
        self.additions
    }
    pub fn deletions(&self) -> usize {
        self.deletions
    }
    pub fn lines(&self) -> &[Line] {
        &self.lines
    }
    pub fn hunks(&self) -> &[Hunk] {
        &self.hunks
    }
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
    pub fn text(&self, line: usize) -> &str {
        &self.source[self.lines[line].bytes.clone()]
    }

    pub fn line_index(&self, side: Side, number: usize) -> Option<usize> {
        let map = self.source_map(side);
        map.binary_search_by_key(&number, |(number, _)| *number)
            .ok()
            .map(|index| map[index].1)
    }

    pub fn source_map(&self, side: Side) -> &[(usize, usize)] {
        match side {
            Side::Original => &self.old_lines,
            Side::Modified => &self.new_lines,
        }
    }

    /// Never pretend omitted context is available. Copying a sparse range fails
    /// instead of joining non-adjacent lines into misleading source code.
    pub fn copy(&self, selection: &Selection) -> Result<String, &'static str> {
        if selection.path != self.path || selection.side != selection.end_side {
            return Err("Select lines on one side of one file.");
        }
        let map = self.source_map(selection.side);
        let start = selection.start.min(selection.end);
        let end = selection.start.max(selection.end);
        let index = map
            .binary_search_by_key(&start, |(number, _)| *number)
            .map_err(|_| "The selected source is not in this patch.")?;
        let mut output = String::new();
        let mut next = start;
        for &(number, line) in &map[index..] {
            if number != next {
                return Err("This selection includes context not supplied by Git.");
            }
            output.push_str(self.text(line));
            if self.lines[line].newline {
                output.push('\n');
            }
            if number == end {
                return Ok(output);
            }
            next = next.checked_add(1).ok_or("Source line number overflow.")?;
        }
        Err("The selected source is not in this patch.")
    }

    pub(crate) fn index_sources(&mut self) {
        for (index, line) in self.lines.iter().enumerate() {
            if let Some(number) = line.old {
                self.old_lines.push((number, index));
            }
            if let Some(number) = line.new {
                self.new_lines.push((number, index));
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Position {
    path: String,
    side: Side,
    line: usize,
}

impl Position {
    pub fn new(path: impl Into<String>, side: Side, line: usize) -> Self {
        Self {
            path: path.into(),
            side,
            line,
        }
    }
    pub fn path(&self) -> &String {
        &self.path
    }
    pub fn side(&self) -> Side {
        self.side
    }
    pub fn line(&self) -> usize {
        self.line
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    path: String,
    side: Side,
    end_side: Side,
    start: usize,
    end: usize,
}

impl Selection {
    pub fn new(path: impl Into<String>, side: Side, start: usize, end: usize) -> Self {
        Self {
            path: path.into(),
            side,
            end_side: side,
            start,
            end,
        }
    }
    pub fn with_end_side(mut self, side: Side) -> Self {
        self.end_side = side;
        self
    }
    pub fn path(&self) -> &String {
        &self.path
    }
    pub fn side(&self) -> Side {
        self.side
    }
    pub fn end_side(&self) -> Side {
        self.end_side
    }
    pub fn start(&self) -> usize {
        self.start
    }
    pub fn end(&self) -> usize {
        self.end
    }
    pub fn contains(&self, file: &str, side: Side, number: usize) -> bool {
        self.path == file
            && self.side == side
            && (self.start.min(self.end)..=self.start.max(self.end)).contains(&number)
    }
}
