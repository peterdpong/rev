use std::{collections::HashMap, ops::Range};

use super::{DiffFile, DiffMode, DiffSide, state::DisplayRow};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SearchMatch {
    pub(crate) row: usize,
    pub(crate) file: usize,
    pub(crate) side: DiffSide,
    pub(crate) line: usize,
    pub(crate) range: Range<usize>,
}

#[derive(Default)]
pub(crate) struct SearchResults {
    matches: Vec<SearchMatch>,
    lines: HashMap<(usize, DiffSide, usize), Range<usize>>,
}

impl SearchResults {
    pub(crate) fn find(
        files: &[DiffFile],
        rows: &[DisplayRow],
        mode: DiffMode,
        query: &str,
    ) -> Self {
        let mut results = Self::default();
        if query.is_empty() {
            return results;
        }
        for (row, display) in rows.iter().enumerate() {
            for (file, side, line) in shown_lines(files, display, mode) {
                let start = results.matches.len();
                let text = files[file].lines(side)[line].display();
                for range in find_in(text, query) {
                    results.matches.push(SearchMatch {
                        row,
                        file,
                        side,
                        line,
                        range,
                    });
                }
                if results.matches.len() > start {
                    results
                        .lines
                        .insert((file, side, line), start..results.matches.len());
                }
            }
        }
        results
    }

    pub(crate) fn matches(&self) -> &[SearchMatch] {
        &self.matches
    }

    pub(crate) fn on_line(
        &self,
        file: usize,
        side: DiffSide,
        line: usize,
    ) -> impl Iterator<Item = (usize, &SearchMatch)> {
        self.lines
            .get(&(file, side, line))
            .cloned()
            .unwrap_or_default()
            .map(|ix| (ix, &self.matches[ix]))
    }
}

fn shown_lines(
    files: &[DiffFile],
    row: &DisplayRow,
    mode: DiffMode,
) -> impl Iterator<Item = (usize, DiffSide, usize)> {
    let DisplayRow::Code {
        file,
        original,
        modified,
        ..
    } = *row
    else {
        return [None, None].into_iter().flatten();
    };
    let original = original.map(|ix| (file, DiffSide::Original, ix));
    let modified = modified.map(|ix| (file, DiffSide::Modified, ix));
    if mode == DiffMode::Split && !files[file].is_single_column() {
        [original, modified].into_iter().flatten()
    } else {
        [modified.or(original), None].into_iter().flatten()
    }
}

fn find_in<'a>(text: &'a str, query: &'a str) -> impl Iterator<Item = Range<usize>> + 'a {
    let (haystack, needle) = (text.as_bytes(), query.as_bytes());
    let mut start = 0;
    std::iter::from_fn(move || {
        while !needle.is_empty() && start + needle.len() <= haystack.len() {
            let end = start + needle.len();
            let found = text.is_char_boundary(start)
                && text.is_char_boundary(end)
                && haystack[start..end].eq_ignore_ascii_case(needle);
            if found {
                let range = start..end;
                start = end;
                return Some(range);
            }
            start += 1;
        }
        None
    })
}

#[cfg(test)]
mod tests {
    use super::find_in;

    fn ranges(text: &str, query: &str) -> Vec<std::ops::Range<usize>> {
        find_in(text, query).collect()
    }

    #[test]
    fn matches_ignore_ascii_case_and_do_not_overlap() {
        assert_eq!(ranges("Foo foo FOO", "foo"), [0..3, 4..7, 8..11]);
        assert_eq!(ranges("aaaa", "aa"), [0..2, 2..4]);
        assert!(ranges("abc", "").is_empty());
        assert!(ranges("ab", "abc").is_empty());
    }

    #[test]
    fn matches_non_ascii_text_exactly() {
        assert_eq!(ranges("é É é", "é"), [0..2, 6..8]);
    }
}
