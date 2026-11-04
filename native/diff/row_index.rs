//! File-sized row segments let one file change without copying every code row.
use std::{ops::Index, rc::Rc};

#[derive(Clone)]
pub(crate) struct RowIndex<T> {
    segments: Vec<Rc<[T]>>,
    ends: Vec<usize>,
}

impl<T> Default for RowIndex<T> {
    fn default() -> Self {
        Self {
            segments: Vec::new(),
            ends: Vec::new(),
        }
    }
}

impl<T> RowIndex<T> {
    pub fn new(segments: Vec<Vec<T>>) -> Self {
        let mut end = 0;
        let mut ends = Vec::with_capacity(segments.len());
        let segments = segments
            .into_iter()
            .map(|segment| {
                end += segment.len();
                ends.push(end);
                Rc::from(segment)
            })
            .collect();
        Self { segments, ends }
    }

    pub fn len(&self) -> usize {
        self.ends.last().copied().unwrap_or(0)
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn start(&self, file: usize) -> usize {
        file.checked_sub(1).map(|ix| self.ends[ix]).unwrap_or(0)
    }
    pub fn segment(&self, file: usize) -> &[T] {
        &self.segments[file]
    }
    pub fn file_at(&self, row: usize) -> Option<usize> {
        let file = self.ends.partition_point(|end| *end <= row);
        (file < self.segments.len()).then_some(file)
    }
    pub fn get(&self, row: usize) -> Option<&T> {
        let file = self.file_at(row)?;
        self.segments[file].get(row - self.start(file))
    }
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &T> {
        self.segments.iter().flat_map(|segment| segment.iter())
    }

    pub fn replace(&mut self, file: usize, rows: Vec<T>) {
        self.segments[file] = Rc::from(rows);
        // Offsets scale with file count, never with the number of source lines.
        let mut end = self.start(file);
        for ix in file..self.segments.len() {
            end += self.segments[ix].len();
            self.ends[ix] = end;
        }
    }
}

impl<T> Index<usize> for RowIndex<T> {
    type Output = T;
    fn index(&self, row: usize) -> &Self::Output {
        self.get(row).expect("diff row index out of bounds")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_matches_flat_rows_and_retains_old_render_snapshots() {
        let mut seed = 7u64;
        let mut segments = vec![vec![]; 40];
        let mut rows = RowIndex::new(segments.clone());
        for _ in 0..200 {
            let old = rows.clone();
            let old_flat = segments.concat();
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let file = (seed >> 32) as usize % segments.len();
            let count = seed as usize % 20;
            segments[file] = (0..count).map(|line| file * 100 + line).collect();
            rows.replace(file, segments[file].clone());
            let expected = segments.concat();
            assert_eq!(rows.len(), expected.len());
            assert_eq!(rows.iter().copied().collect::<Vec<_>>(), expected);
            for (ix, value) in expected.iter().enumerate() {
                assert_eq!(rows.get(ix), Some(value));
            }
            assert_eq!(rows.get(expected.len()), None);
            assert_eq!(old.iter().copied().collect::<Vec<_>>(), old_flat);
            for ix in 0..segments.len() {
                if ix != file {
                    assert!(Rc::ptr_eq(&old.segments[ix], &rows.segments[ix]));
                }
            }
        }
    }
}
