use crate::file_search::model::{
    ContentFileResult, ContentMatch, FilenameResult, PathIdentity, SearchResult,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub const FILENAME_SORT_LABEL: &str = "Filename";
pub const CONTENT_SORT_LABEL: &str = "Content";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FilenameSort {
    Relevance,
    FilenameAscending,
    FilenameDescending,
    FullPathAscending,
    ModifiedNewest,
    ModifiedOldest,
    SizeLargest,
    SizeSmallest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentSort {
    DiscoveryOrder,
    PathThenLine,
    MatchCountDescending,
    ModifiedNewest,
    FilenameRelevance,
    LineNumber,
}

pub fn path_identity(path: &Path) -> PathIdentity {
    if let Ok(canonical) = path.canonicalize() {
        return PathIdentity::from_path(&canonical);
    }
    if path.is_absolute() {
        return PathIdentity::from_path(path);
    }
    if let Ok(cwd) = std::env::current_dir() {
        return PathIdentity::from_path(&cwd.join(path));
    }
    PathIdentity::from_path(path)
}

fn normalized_path(path: &Path) -> String {
    path_identity(path).normalized_path
}

fn lower(s: &str) -> String {
    s.to_lowercase()
}
fn file_tie(a: &FilenameResult, b: &FilenameResult) -> std::cmp::Ordering {
    lower(&a.file_name)
        .cmp(&lower(&b.file_name))
        .then_with(|| normalized_path(&a.path).cmp(&normalized_path(&b.path)))
        .then_with(|| a.arrival_index.cmp(&b.arrival_index))
}
fn content_file_tie(a: &ContentFileResult, b: &ContentFileResult) -> std::cmp::Ordering {
    lower(&normalized_path(&a.path))
        .cmp(&lower(&normalized_path(&b.path)))
        .then_with(|| a.arrival_index.cmp(&b.arrival_index))
}

pub fn sort_filename_results_by(results: &mut [FilenameResult], sort: FilenameSort) {
    results.sort_by(|a, b| match sort {
        FilenameSort::Relevance => a.rank.cmp(&b.rank).then_with(|| file_tie(a, b)),
        FilenameSort::FilenameAscending => file_tie(a, b),
        FilenameSort::FilenameDescending => file_tie(b, a),
        FilenameSort::FullPathAscending => normalized_path(&a.path)
            .cmp(&normalized_path(&b.path))
            .then_with(|| a.arrival_index.cmp(&b.arrival_index)),
        FilenameSort::ModifiedNewest => b.modified.cmp(&a.modified).then_with(|| file_tie(a, b)),
        FilenameSort::ModifiedOldest => a.modified.cmp(&b.modified).then_with(|| file_tie(a, b)),
        FilenameSort::SizeLargest => b.size.cmp(&a.size).then_with(|| file_tie(a, b)),
        FilenameSort::SizeSmallest => a.size.cmp(&b.size).then_with(|| file_tie(a, b)),
    });
}

pub fn sort_filename_results(results: &mut [FilenameResult]) {
    sort_filename_results_by(results, FilenameSort::Relevance);
}

fn match_key(m: &ContentMatch, occurrence: usize) -> (usize, usize, usize, usize) {
    (
        m.line_number,
        m.column.unwrap_or(usize::MAX),
        m.byte_end,
        occurrence,
    )
}

pub fn sort_content_matches(matches: &mut [ContentMatch]) {
    let mut seen: HashMap<(usize, usize, usize), usize> = HashMap::new();
    let keys: Vec<_> = matches
        .iter()
        .map(|m| {
            let base = (m.line_number, m.byte_start, m.byte_end);
            let occ = *seen.entry(base).or_insert(0);
            *seen.get_mut(&base).unwrap() += 1;
            match_key(m, occ)
        })
        .collect();
    let mut indexed: Vec<_> = matches.iter().cloned().zip(keys).collect();
    indexed.sort_by_key(|(_, k)| *k);
    for (slot, (m, _)) in matches.iter_mut().zip(indexed) {
        *slot = m;
    }
}

pub fn sort_content_results_by(results: &mut [ContentFileResult], sort: ContentSort) {
    for r in results.iter_mut() {
        sort_content_matches(&mut r.matches);
    }
    results.sort_by(|a, b| match sort {
        ContentSort::DiscoveryOrder => a
            .arrival_index
            .cmp(&b.arrival_index)
            .then_with(|| content_file_tie(a, b)),
        ContentSort::PathThenLine => content_file_tie(a, b),
        ContentSort::MatchCountDescending => b
            .total_matches
            .cmp(&a.total_matches)
            .then_with(|| content_file_tie(a, b)),
        ContentSort::ModifiedNewest => b
            .modified
            .cmp(&a.modified)
            .then_with(|| content_file_tie(a, b)),
        ContentSort::FilenameRelevance => a
            .filename_relevance
            .cmp(&b.filename_relevance)
            .then_with(|| content_file_tie(a, b)),
        ContentSort::LineNumber => a
            .matches
            .first()
            .map(|m| m.line_number)
            .cmp(&b.matches.first().map(|m| m.line_number))
            .then_with(|| content_file_tie(a, b)),
    });
}

pub fn dedup_filename_results(results: Vec<FilenameResult>) -> Vec<FilenameResult> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for r in results {
        if seen.insert(path_identity(&r.path)) {
            out.push(r);
        }
    }
    out
}

pub fn dedup_content_results(results: Vec<ContentFileResult>) -> Vec<ContentFileResult> {
    let mut order = Vec::<PathIdentity>::new();
    let mut groups: HashMap<PathIdentity, ContentFileResult> = HashMap::new();
    for mut r in results {
        let id = path_identity(&r.path);
        if let Some(existing) = groups.get_mut(&id) {
            existing.total_matches = existing.total_matches.max(r.total_matches);
            existing.truncated |= r.truncated;
            let mut keys: HashSet<_> = existing
                .matches
                .iter()
                .map(|m| (m.line_number, m.byte_start, m.byte_end, m.line.clone()))
                .collect();
            for m in r.matches.drain(..) {
                if keys.insert((m.line_number, m.byte_start, m.byte_end, m.line.clone())) {
                    existing.matches.push(m);
                }
            }
        } else {
            order.push(id.clone());
            groups.insert(id, r);
        }
    }
    order
        .into_iter()
        .filter_map(|id| groups.remove(&id))
        .collect()
}

pub fn sort_and_dedup_results(
    results: Vec<SearchResult>,
    filename_sort: FilenameSort,
    content_sort: ContentSort,
) -> Vec<SearchResult> {
    let mut files = Vec::new();
    let mut contents = Vec::new();
    for r in results {
        match r {
            SearchResult::Filename(f) => files.push(f),
            SearchResult::ContentFile(c) => contents.push(c),
        }
    }
    files = dedup_filename_results(files);
    contents = dedup_content_results(contents);
    sort_filename_results_by(&mut files, filename_sort);
    sort_content_results_by(&mut contents, content_sort);
    files
        .into_iter()
        .map(SearchResult::Filename)
        .chain(contents.into_iter().map(SearchResult::ContentFile))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_search::model::{FileKind, FilenameRank};
    fn result(path: &str, rank: FilenameRank) -> FilenameResult {
        let path = PathBuf::from(path);
        FilenameResult {
            file_name: path.file_name().unwrap().to_string_lossy().to_string(),
            parent_directory: path.parent().map(Path::to_path_buf),
            path,
            kind: FileKind::File,
            size: None,
            modified: None,
            rank,
            match_quality: rank,
            filename_match_ranges: vec![],
            path_match_ranges: vec![],
            arrival_index: 0,
        }
    }
    #[test]
    fn every_filename_sort_mode_is_deterministic() {
        let mut rs = vec![
            result("b/z.txt", FilenameRank::FilenameContains),
            result("a/a.txt", FilenameRank::ExactFilename),
        ];
        for s in [
            FilenameSort::Relevance,
            FilenameSort::FilenameAscending,
            FilenameSort::FilenameDescending,
            FilenameSort::FullPathAscending,
            FilenameSort::ModifiedNewest,
            FilenameSort::ModifiedOldest,
            FilenameSort::SizeLargest,
            FilenameSort::SizeSmallest,
        ] {
            sort_filename_results_by(&mut rs, s);
            let once = rs.clone();
            sort_filename_results_by(&mut rs, s);
            assert_eq!(rs, once);
        }
    }
    fn cf(path: &str, arrival: usize, total: usize) -> ContentFileResult {
        ContentFileResult {
            path: path.into(),
            file_name: path.into(),
            modified: None,
            filename_relevance: None,
            arrival_index: arrival,
            total_matches: total,
            matches: vec![ContentMatch::new(arrival + 1, "x".into(), 0, 1)],
            truncated: false,
        }
    }
    #[test]
    fn every_content_sort_mode_is_deterministic() {
        let mut rs = vec![cf("b", 1, 2), cf("a", 0, 1)];
        for s in [
            ContentSort::DiscoveryOrder,
            ContentSort::PathThenLine,
            ContentSort::MatchCountDescending,
            ContentSort::ModifiedNewest,
            ContentSort::FilenameRelevance,
            ContentSort::LineNumber,
        ] {
            sort_content_results_by(&mut rs, s);
            let once = rs.clone();
            sort_content_results_by(&mut rs, s);
            assert_eq!(rs, once);
        }
    }
    #[test]
    fn null_size_and_modified_sort_consistently() {
        let mut rs = vec![
            result("b", FilenameRank::ExactFilename),
            result("a", FilenameRank::ExactFilename),
        ];
        rs[0].size = Some(1);
        sort_filename_results_by(&mut rs, FilenameSort::SizeLargest);
        assert_eq!(rs[0].path, PathBuf::from("b"));
        sort_filename_results_by(&mut rs, FilenameSort::ModifiedNewest);
        assert_eq!(rs[0].path, PathBuf::from("a"));
    }
    #[test]
    fn duplicate_content_matches_merge_to_one() {
        let mut a = cf("same", 0, 1);
        let mut b = cf("same", 1, 3);
        b.matches[0] = a.matches[0].clone();
        a.truncated = true;
        let out = dedup_content_results(vec![a, b]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].total_matches, 3);
        assert!(out[0].truncated);
        assert_eq!(out[0].matches.len(), 1);
    }
}
