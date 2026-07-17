use crate::file_search::model::{FilenameMatchMode, FilenameRank, TextMatchRange};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use std::path::Path;

/// Returns true when `needle` occurs literally in `haystack`, ignoring case.
pub fn literal_contains_case_insensitive(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let haystack: String = haystack.chars().flat_map(|c| c.to_lowercase()).collect();
    let needle: String = needle.chars().flat_map(|c| c.to_lowercase()).collect();
    haystack.contains(&needle)
}

pub fn rank_filename_match(
    file_name: &str,
    path: &Path,
    search_text: &str,
    case_sensitive: bool,
) -> Option<FilenameRank> {
    ranked_substring_match_ranges(file_name, path, search_text, case_sensitive).map(|m| m.rank)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilenameHighlightMatch {
    pub rank: FilenameRank,
    pub filename_match_ranges: Vec<TextMatchRange>,
    pub path_match_ranges: Vec<TextMatchRange>,
}

pub fn filename_highlight_match(
    file_name: &str,
    path: &Path,
    search_text: &str,
    case_sensitive: bool,
    mode: FilenameMatchMode,
) -> Option<FilenameHighlightMatch> {
    match mode {
        FilenameMatchMode::RankedSubstring => {
            ranked_substring_match_ranges(file_name, path, search_text, case_sensitive)
        }
        FilenameMatchMode::Fuzzy => {
            fuzzy_filename_match_ranges(file_name, search_text, case_sensitive).map(
                |filename_match_ranges| FilenameHighlightMatch {
                    rank: FilenameRank::FilenameContains,
                    filename_match_ranges,
                    path_match_ranges: Vec::new(),
                },
            )
        }
    }
}

pub fn ranked_substring_match_ranges(
    file_name: &str,
    path: &Path,
    search_text: &str,
    case_sensitive: bool,
) -> Option<FilenameHighlightMatch> {
    exact_filename_match_range(file_name, search_text, case_sensitive)
        .map(|filename_match_ranges| FilenameHighlightMatch {
            rank: FilenameRank::ExactFilename,
            filename_match_ranges,
            path_match_ranges: Vec::new(),
        })
        .or_else(|| {
            filename_prefix_match_range(file_name, search_text, case_sensitive).map(
                |filename_match_ranges| FilenameHighlightMatch {
                    rank: FilenameRank::FilenameStartsWith,
                    filename_match_ranges,
                    path_match_ranges: Vec::new(),
                },
            )
        })
        .or_else(|| {
            filename_substring_match_ranges(file_name, search_text, case_sensitive).map(
                |filename_match_ranges| FilenameHighlightMatch {
                    rank: FilenameRank::FilenameContains,
                    filename_match_ranges,
                    path_match_ranges: Vec::new(),
                },
            )
        })
        .or_else(|| {
            path_substring_match_ranges(&path.to_string_lossy(), search_text, case_sensitive).map(
                |path_match_ranges| FilenameHighlightMatch {
                    rank: FilenameRank::FullPathContains,
                    filename_match_ranges: Vec::new(),
                    path_match_ranges,
                },
            )
        })
}

pub fn exact_filename_match_range(
    file_name: &str,
    query: &str,
    case_sensitive: bool,
) -> Option<Vec<TextMatchRange>> {
    if query.is_empty() || !eq_for_match(file_name, query, case_sensitive) {
        return None;
    }
    Some(vec![TextMatchRange {
        byte_start: 0,
        byte_end: file_name.len(),
    }])
}

pub fn filename_prefix_match_range(
    file_name: &str,
    query: &str,
    case_sensitive: bool,
) -> Option<Vec<TextMatchRange>> {
    substring_ranges(file_name, query, case_sensitive).and_then(|ranges| {
        ranges
            .first()
            .filter(|r| r.byte_start == 0)
            .map(|r| vec![*r])
    })
}

pub fn filename_substring_match_ranges(
    file_name: &str,
    query: &str,
    case_sensitive: bool,
) -> Option<Vec<TextMatchRange>> {
    substring_ranges(file_name, query, case_sensitive)
}

pub fn path_substring_match_ranges(
    path: &str,
    query: &str,
    case_sensitive: bool,
) -> Option<Vec<TextMatchRange>> {
    substring_ranges(path, query, case_sensitive)
}

pub fn fuzzy_filename_match_ranges(
    file_name: &str,
    query: &str,
    case_sensitive: bool,
) -> Option<Vec<TextMatchRange>> {
    if query.is_empty() {
        return None;
    }
    let matcher = SkimMatcherV2::default();
    let (hay, needle, char_to_original) = if case_sensitive {
        (
            file_name.to_owned(),
            query.to_owned(),
            file_name.char_indices().map(|(i, _)| i).collect::<Vec<_>>(),
        )
    } else {
        let (folded, map) = folded_chars_with_original_char_starts(file_name);
        let needle = query.chars().flat_map(|c| c.to_lowercase()).collect();
        (folded, needle, map)
    };
    let indices = matcher
        .fuzzy_indices(&hay, &needle)
        .or_else(|| {
            let compact_needle: String = needle.chars().filter(|ch| ch.is_alphanumeric()).collect();
            (!compact_needle.is_empty() && compact_needle != needle)
                .then(|| matcher.fuzzy_indices(&hay, &compact_needle))
                .flatten()
        })?
        .1;
    Some(fuzzy_char_indices_to_byte_ranges(
        file_name,
        &char_to_original,
        &indices,
    ))
}

fn eq_for_match(a: &str, b: &str, case_sensitive: bool) -> bool {
    if case_sensitive {
        a == b
    } else {
        a.to_lowercase() == b.to_lowercase()
    }
}

fn substring_ranges(text: &str, query: &str, case_sensitive: bool) -> Option<Vec<TextMatchRange>> {
    if query.is_empty() {
        return None;
    }
    if case_sensitive {
        return find_byte_ranges(text, query);
    }
    let (folded, byte_to_original) = folded_bytes_with_original_offsets(text);
    let needle: String = query.chars().flat_map(|c| c.to_lowercase()).collect();
    find_byte_ranges(&folded, &needle)
        .map(|ranges| {
            ranges
                .into_iter()
                .filter_map(|r| {
                    let start = *byte_to_original.get(r.byte_start)?;
                    let end = *byte_to_original.get(r.byte_end)?;
                    (start < end).then_some(TextMatchRange {
                        byte_start: start,
                        byte_end: end,
                    })
                })
                .collect()
        })
        .filter(|ranges: &Vec<_>| !ranges.is_empty())
}

fn find_byte_ranges(text: &str, needle: &str) -> Option<Vec<TextMatchRange>> {
    if needle.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    let mut offset = 0;
    while let Some(rel) = text[offset..].find(needle) {
        let start = offset + rel;
        let end = start + needle.len();
        if text.is_char_boundary(start) && text.is_char_boundary(end) {
            out.push(TextMatchRange {
                byte_start: start,
                byte_end: end,
            });
        }
        offset = end;
    }
    (!out.is_empty()).then_some(out)
}

fn folded_bytes_with_original_offsets(text: &str) -> (String, Vec<usize>) {
    let mut folded = String::new();
    let mut map = Vec::new();
    for (start, ch) in text.char_indices() {
        for lower in ch.to_lowercase() {
            let s = lower.to_string();
            map.extend(std::iter::repeat(start).take(s.len()));
            folded.push(lower);
        }
    }
    map.push(text.len());
    (folded, map)
}

fn folded_chars_with_original_char_starts(text: &str) -> (String, Vec<usize>) {
    let mut folded = String::new();
    let mut starts = Vec::new();
    for (start, ch) in text.char_indices() {
        for lower in ch.to_lowercase() {
            folded.push(lower);
            starts.push(start);
        }
    }
    (folded, starts)
}

pub fn fuzzy_char_indices_to_byte_ranges(
    original: &str,
    char_to_original_start: &[usize],
    indices: &[usize],
) -> Vec<TextMatchRange> {
    let original_starts: Vec<_> = original
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(original.len()))
        .collect();
    let mut byte_indices: Vec<_> = indices
        .iter()
        .filter_map(|&i| char_to_original_start.get(i).copied())
        .collect();
    byte_indices.sort_unstable();
    byte_indices.dedup();
    let mut ranges: Vec<TextMatchRange> = Vec::new();
    for start in byte_indices {
        if let Ok(char_pos) = original_starts.binary_search(&start) {
            let end = original_starts[char_pos + 1];
            if let Some(last) = ranges.last_mut() {
                if last.byte_end == start {
                    last.byte_end = end;
                    continue;
                }
            }
            ranges.push(TextMatchRange {
                byte_start: start,
                byte_end: end,
            });
        }
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_search::model::FilenameRank;

    #[test]
    fn literal_refinement_matching_is_case_insensitive() {
        assert!(literal_contains_case_insensitive("Src/Main.RS", "main.rs"));
        assert!(literal_contains_case_insensitive("Line 42: Needle", "42"));
        assert!(!literal_contains_case_insensitive("alpha", "beta"));
    }

    #[test]
    fn ranks_filename_before_path_matches() {
        assert_eq!(
            rank_filename_match("needle.txt", "src/needle.txt".as_ref(), "needle.txt", false),
            Some(FilenameRank::ExactFilename)
        );
        assert_eq!(
            rank_filename_match("needle.txt", "src/needle.txt".as_ref(), "need", false),
            Some(FilenameRank::FilenameStartsWith)
        );
        assert_eq!(
            rank_filename_match(
                "my-needle.txt",
                "src/my-needle.txt".as_ref(),
                "needle",
                false
            ),
            Some(FilenameRank::FilenameContains)
        );
        assert_eq!(
            rank_filename_match("main.rs", "needle/main.rs".as_ref(), "needle", false),
            Some(FilenameRank::FullPathContains)
        );
    }

    #[test]
    fn ascii_and_case_insensitive_substring_ranges() {
        assert_eq!(
            filename_substring_match_ranges("abc abc", "abc", true).unwrap(),
            vec![
                TextMatchRange {
                    byte_start: 0,
                    byte_end: 3
                },
                TextMatchRange {
                    byte_start: 4,
                    byte_end: 7
                }
            ]
        );
        assert_eq!(
            filename_substring_match_ranges("FileSearchDialog", "search", false).unwrap(),
            vec![TextMatchRange {
                byte_start: 4,
                byte_end: 10
            }]
        );
    }

    #[test]
    fn unicode_filename_ranges_respect_char_boundaries() {
        let ranges = filename_substring_match_ranges("café.rs", "CAFÉ", false).unwrap();
        assert_eq!(
            ranges,
            vec![TextMatchRange {
                byte_start: 0,
                byte_end: 5
            }]
        );
        assert!("café.rs".is_char_boundary(ranges[0].byte_start));
        assert!("café.rs".is_char_boundary(ranges[0].byte_end));
    }

    #[test]
    fn fuzzy_ranges_group_multiple_separated_runs() {
        let ranges = fuzzy_filename_match_ranges("FileSearchDialog", "f_s_dlg", false).unwrap();
        assert!(
            ranges.len() >= 4,
            "expected separated fuzzy groups: {ranges:?}"
        );
        assert_eq!(
            ranges.first().copied(),
            Some(TextMatchRange {
                byte_start: 0,
                byte_end: 1
            })
        );
        assert!(ranges.iter().any(|range| *range
            == TextMatchRange {
                byte_start: 4,
                byte_end: 5
            }));
        assert!(ranges.iter().any(|range| *range
            == TextMatchRange {
                byte_start: 10,
                byte_end: 11
            }));
        assert!(ranges.iter().all(
            |range| "FileSearchDialog".is_char_boundary(range.byte_start)
                && "FileSearchDialog".is_char_boundary(range.byte_end)
        ));
    }

    #[test]
    fn fuzzy_indices_survive_unicode_case_folding() {
        let ranges = fuzzy_filename_match_ranges("ÅngströmReport.md", "ång", false).unwrap();
        assert_eq!(
            ranges.first().copied(),
            Some(TextMatchRange {
                byte_start: 0,
                byte_end: "Ång".len()
            })
        );
        assert!(
            ranges
                .iter()
                .all(|r| "ÅngströmReport.md".is_char_boundary(r.byte_start)
                    && "ÅngströmReport.md".is_char_boundary(r.byte_end))
        );
    }

    #[test]
    fn ranked_highlights_report_filename_and_path_ranges_separately() {
        let filename_hit = filename_highlight_match(
            "notes.txt",
            "archive/notes.txt".as_ref(),
            "note",
            false,
            FilenameMatchMode::RankedSubstring,
        )
        .unwrap();
        assert_eq!(filename_hit.rank, FilenameRank::FilenameStartsWith);
        assert!(!filename_hit.filename_match_ranges.is_empty());
        assert!(filename_hit.path_match_ranges.is_empty());

        let path_hit = filename_highlight_match(
            "notes.txt",
            "archive/project/notes.txt".as_ref(),
            "project",
            false,
            FilenameMatchMode::RankedSubstring,
        )
        .unwrap();
        assert_eq!(path_hit.rank, FilenameRank::FullPathContains);
        assert!(path_hit.filename_match_ranges.is_empty());
        assert!(!path_hit.path_match_ranges.is_empty());
    }
}
