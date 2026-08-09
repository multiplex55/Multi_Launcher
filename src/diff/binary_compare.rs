//! File-backed, block-oriented binary comparison.
use crate::diff::model::{BinaryCompareState, validated_splitter};
use crate::diff::text_compare::NavigationDirection;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub const COMPARISON_BLOCK_SIZE: usize = 64 * 1024;

#[derive(Debug)]
pub struct BinaryDocument {
    pub path: Option<PathBuf>,
    pub len: u64,
    file: Option<File>,
}
impl BinaryDocument {
    pub fn open(path: Option<&Path>) -> Result<Self, String> {
        match path {
            Some(path) => {
                let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
                let len = file
                    .metadata()
                    .map_err(|e| format!("{}: {e}", path.display()))?
                    .len();
                Ok(Self {
                    path: Some(path.to_owned()),
                    len,
                    file: Some(file),
                })
            }
            None => Ok(Self {
                path: None,
                len: 0,
                file: None,
            }),
        }
    }
    pub fn read_at(&mut self, offset: u64, limit: usize) -> Result<Vec<u8>, String> {
        let Some(file) = &mut self.file else {
            return Ok(vec![]);
        };
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| e.to_string())?;
        let mut out = vec![0; limit.min(self.len.saturating_sub(offset) as usize)];
        file.read_exact(&mut out).map_err(|e| e.to_string())?;
        Ok(out)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BinaryDiffRange {
    pub start: u64,
    pub left_len: u64,
    pub right_len: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BinaryDifferenceIndex {
    pub ranges: Vec<BinaryDiffRange>,
}

impl BinaryDifferenceIndex {
    pub fn build(left: &mut BinaryDocument, right: &mut BinaryDocument) -> Result<Self, String> {
        let total = left.len.max(right.len);
        let mut ranges = Vec::<BinaryDiffRange>::new();
        let mut offset = 0u64;
        while offset < total {
            let count = COMPARISON_BLOCK_SIZE.min((total - offset) as usize);
            let l = left.read_at(offset, count)?;
            let r = right.read_at(offset, count)?;
            let mut i = 0;
            while i < l.len().max(r.len()) {
                if l.get(i) == r.get(i) {
                    i += 1;
                    continue;
                }
                let start = i;
                while i < l.len().max(r.len()) && l.get(i) != r.get(i) {
                    i += 1;
                }
                let candidate = BinaryDiffRange {
                    start: offset + start as u64,
                    left_len: (i.min(l.len()) - start.min(l.len())) as u64,
                    right_len: (i.min(r.len()) - start.min(r.len())) as u64,
                };
                if let Some(last) = ranges.last_mut().filter(|last| {
                    last.start + last.left_len.max(last.right_len) == candidate.start
                }) {
                    last.left_len += candidate.left_len;
                    last.right_len += candidate.right_len;
                } else {
                    ranges.push(candidate);
                }
            }
            offset += count as u64;
        }
        Ok(Self { ranges })
    }
    pub fn contains(&self, offset: u64) -> bool {
        self.ranges
            .iter()
            .any(|r| offset >= r.start && offset < r.start + r.left_len.max(r.right_len))
    }
}

#[derive(Debug)]
pub struct BinaryViewModel {
    pub left: BinaryDocument,
    pub right: BinaryDocument,
    pub differences: BinaryDifferenceIndex,
    pub current_difference: Option<usize>,
    pub bytes_per_row: usize,
    pub splitter: f32,
    pub visible_byte_offset: u64,
    pub pending_scroll_offset: Option<u64>,
    pub generation: u64,
    pub stale: bool,
}
impl BinaryViewModel {
    pub fn load(state: &BinaryCompareState, splitter: f32) -> Result<Self, String> {
        let mut left = BinaryDocument::open(state.left.as_deref())?;
        let mut right = BinaryDocument::open(state.right.as_deref())?;
        let differences = BinaryDifferenceIndex::build(&mut left, &mut right)?;
        Ok(Self {
            left,
            right,
            differences,
            current_difference: None,
            bytes_per_row: 16,
            splitter: validated_splitter(splitter),
            visible_byte_offset: 0,
            pending_scroll_offset: None,
            generation: 1,
            stale: false,
        })
    }
    /// Reopens both files and atomically publishes a completely rebuilt index.
    pub fn refresh_external(&mut self, state: &BinaryCompareState) -> Result<(), String> {
        self.stale = true;
        let mut replacement = Self::load(state, self.splitter)?;
        replacement.generation = self.generation.wrapping_add(1);
        *self = replacement;
        Ok(())
    }
    pub fn navigate(&mut self, direction: NavigationDirection) {
        let n = self.differences.ranges.len();
        if n == 0 {
            self.current_difference = None;
            self.pending_scroll_offset = None;
            return;
        }
        let next = match direction {
            NavigationDirection::First => 0,
            NavigationDirection::Last => n - 1,
            NavigationDirection::Next => self.current_difference.map_or(0, |i| (i + 1) % n),
            NavigationDirection::Previous => {
                self.current_difference.map_or(n - 1, |i| (i + n - 1) % n)
            }
        };
        self.current_difference = Some(next);
        self.pending_scroll_offset = Some(self.differences.ranges[next].start);
    }
    pub fn row(&mut self, offset: u64) -> Result<BinaryRow, String> {
        let left = self.left.read_at(offset, self.bytes_per_row)?;
        let right = self.right.read_at(offset, self.bytes_per_row)?;
        Ok(BinaryRow::new(
            offset,
            left,
            right,
            self.bytes_per_row,
            &self.differences,
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryCell {
    pub byte: Option<u8>,
    pub changed: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryRow {
    pub offset: u64,
    pub left: Vec<BinaryCell>,
    pub right: Vec<BinaryCell>,
}
impl BinaryRow {
    fn new(
        offset: u64,
        left: Vec<u8>,
        right: Vec<u8>,
        width: usize,
        index: &BinaryDifferenceIndex,
    ) -> Self {
        let cells = |side: &[u8]| {
            (0..width)
                .map(|i| BinaryCell {
                    byte: side.get(i).copied(),
                    changed: index.contains(offset + i as u64),
                })
                .collect()
        };
        Self {
            offset,
            left: cells(&left),
            right: cells(&right),
        }
    }
    pub fn hex(cells: &[BinaryCell]) -> String {
        cells
            .iter()
            .map(|c| c.byte.map_or("--".into(), |b| format!("{b:02X}")))
            .collect::<Vec<_>>()
            .join(" ")
    }
    pub fn ascii(cells: &[BinaryCell]) -> String {
        cells
            .iter()
            .map(|c| {
                c.byte.map_or(' ', |b| {
                    if b.is_ascii_graphic() || b == b' ' {
                        b as char
                    } else {
                        '.'
                    }
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn docs(a: &[u8], b: &[u8]) -> (tempfile::TempDir, BinaryDocument, BinaryDocument) {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a"), a).unwrap();
        std::fs::write(d.path().join("b"), b).unwrap();
        let l = BinaryDocument::open(Some(&d.path().join("a"))).unwrap();
        let r = BinaryDocument::open(Some(&d.path().join("b"))).unwrap();
        (d, l, r)
    }
    #[test]
    fn ranges_and_boundaries() {
        let (_d, mut l, mut r) = docs(&vec![1; COMPARISON_BLOCK_SIZE + 2], &{
            let mut x = vec![1; COMPARISON_BLOCK_SIZE + 2];
            x[COMPARISON_BLOCK_SIZE - 1] = 2;
            x[COMPARISON_BLOCK_SIZE] = 2;
            x
        });
        let i = BinaryDifferenceIndex::build(&mut l, &mut r).unwrap();
        assert_eq!(
            i.ranges,
            [BinaryDiffRange {
                start: COMPARISON_BLOCK_SIZE as u64 - 1,
                left_len: 2,
                right_len: 2
            }]
        );
    }
    #[test]
    fn equal_middle_separate_and_tail() {
        for (a, b, n) in [
            (b"abc".as_slice(), b"abc".as_slice(), 0),
            (b"abc", b"axc", 1),
            (b"abcde", b"axcye", 2),
            (b"abc", b"abcde", 1),
        ] {
            let (_d, mut l, mut r) = docs(a, b);
            assert_eq!(
                BinaryDifferenceIndex::build(&mut l, &mut r)
                    .unwrap()
                    .ranges
                    .len(),
                n
            );
        }
    }
    #[test]
    fn first_byte_is_one_contiguous_range() {
        let (_d, mut left, mut right) = docs(b"abc", b"xbc");
        assert_eq!(
            BinaryDifferenceIndex::build(&mut left, &mut right)
                .unwrap()
                .ranges,
            [BinaryDiffRange {
                start: 0,
                left_len: 1,
                right_len: 1
            }]
        );
    }
    #[test]
    fn formatting_missing() {
        let (_d, mut l, mut r) = docs(b"A\0", b"B");
        let i = BinaryDifferenceIndex::build(&mut l, &mut r).unwrap();
        let row = BinaryRow::new(
            0,
            l.read_at(0, 16).unwrap(),
            r.read_at(0, 16).unwrap(),
            2,
            &i,
        );
        assert_eq!(BinaryRow::hex(&row.left), "41 00");
        assert_eq!(BinaryRow::hex(&row.right), "42 --");
        assert_eq!(BinaryRow::ascii(&row.left), "A.");
    }
    #[test]
    fn navigation_wraps_and_scrolls_to_range_start() {
        let d = tempfile::tempdir().unwrap();
        let left = d.path().join("left");
        let right = d.path().join("right");
        std::fs::write(&left, b"abcde").unwrap();
        std::fs::write(&right, b"axcye").unwrap();
        let mut model = BinaryViewModel::load(
            &BinaryCompareState {
                left: Some(left),
                right: Some(right),
                relative_path: None,
            },
            0.5,
        )
        .unwrap();
        model.navigate(NavigationDirection::Previous);
        assert_eq!(model.current_difference, Some(1));
        assert_eq!(model.pending_scroll_offset, Some(3));
        model.navigate(NavigationDirection::Next);
        assert_eq!(model.current_difference, Some(0));
        assert_eq!(model.pending_scroll_offset, Some(1));
        model.navigate(NavigationDirection::Last);
        assert_eq!(model.current_difference, Some(1));
        model.navigate(NavigationDirection::First);
        assert_eq!(model.current_difference, Some(0));
    }
    #[test]
    fn external_refresh_rebuilds_index_and_advances_generation() {
        let d = tempfile::tempdir().unwrap();
        let left = d.path().join("left");
        let right = d.path().join("right");
        std::fs::write(&left, [1, 2, 3]).unwrap();
        std::fs::write(&right, [1, 2, 3]).unwrap();
        let state = BinaryCompareState {
            left: Some(left),
            right: Some(right.clone()),
            relative_path: None,
        };
        let mut model = BinaryViewModel::load(&state, 0.5).unwrap();
        let generation = model.generation;
        assert!(model.differences.ranges.is_empty());
        std::fs::write(right, [1, 9, 3]).unwrap();
        model.refresh_external(&state).unwrap();
        assert_eq!(model.generation, generation + 1);
        assert!(!model.stale);
        assert!(model.differences.contains(1));
    }
}
