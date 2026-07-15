use crate::file_search::coordinator::{CancellationToken, SearchExecutor};
use crate::file_search::matching::filename_highlight_match;
use crate::file_search::model::{
    FileKind, FilenameResult, SearchEvent, SearchId, SearchKind, SearchProgress, SearchRequest,
    SearchResult, SearchScope, SearchStatus,
};
use crate::file_search::settings::FileSearchSettings;
use std::cell::Cell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use walkdir::{DirEntry, WalkDir};

#[derive(Debug, Clone)]
pub struct WalkDirSearchExecutor {
    settings: FileSearchSettings,
}

impl WalkDirSearchExecutor {
    pub fn new(settings: FileSearchSettings) -> Self {
        Self { settings }
    }
}

impl SearchExecutor for WalkDirSearchExecutor {
    fn execute(
        &self,
        id: SearchId,
        request: SearchRequest,
        token: CancellationToken,
        events: mpsc::Sender<SearchEvent>,
    ) {
        if let Err(error) =
            search_filenames_in_directory(request, &self.settings, &token, &events, id)
        {
            let _ = events.send(SearchEvent::Failed { id, error });
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WalkDirSearchSummary {
    pub results_found: usize,
    pub files_scanned: u64,
    pub directories_scanned: u64,
    pub skipped_entries: u64,
    pub inaccessible_entries: u64,
    pub cancelled: bool,
    pub root_errors: Vec<String>,
}

pub fn search_filenames_in_directory(
    request: SearchRequest,
    settings: &FileSearchSettings,
    cancellation: &CancellationToken,
    event_sender: &mpsc::Sender<SearchEvent>,
    search_id: SearchId,
) -> Result<WalkDirSearchSummary, String> {
    let roots = match &request.scope {
        SearchScope::Roots { roots } if roots.is_empty() => {
            return Err("walkdir filename search requires at least one root".to_owned());
        }
        SearchScope::Roots { roots } => roots.clone(),
        SearchScope::Files { files } => files.clone(),
    };
    if request.kind != SearchKind::Filename {
        return Err("walkdir filename search only supports filename requests".to_owned());
    }

    let needle = if request.case_sensitive {
        request.text.clone()
    } else {
        request.text.to_lowercase()
    };
    let excluded_names = excluded_directory_names(&request, settings);
    let include_hidden = request.include_hidden_files;
    let mut summary = WalkDirSearchSummary::default();
    let skipped_before_descent = Cell::new(0_u64);
    let mut ranked_results = Vec::new();
    let mut seen_results = HashSet::new();
    let resolved_roots: Vec<PathBuf> = roots
        .into_iter()
        .filter_map(|root| match root.canonicalize() {
            Ok(resolved) if resolved.is_dir() => Some(resolved),
            Ok(resolved) => {
                summary
                    .root_errors
                    .push(format!("{} is not a directory", resolved.display()));
                None
            }
            Err(err) => {
                summary
                    .root_errors
                    .push(format!("{}: {err}", root.display()));
                None
            }
        })
        .collect();
    let roots = dedup_resolved_roots(resolved_roots);
    if roots.is_empty() {
        return Err("walkdir filename search requires at least one usable root".to_owned());
    }

    for root in roots {
        if cancellation.is_cancelled() {
            summary.cancelled = true;
            break;
        }
        let iter = WalkDir::new(&root).into_iter().filter_entry(|entry| {
            if cancellation.is_cancelled() {
                return false;
            }
            let descend = should_descend(entry, &root, &excluded_names, include_hidden);
            if !descend {
                skipped_before_descent.set(skipped_before_descent.get().saturating_add(1));
            }
            descend
        });

        for entry in iter {
            if cancellation.is_cancelled() {
                summary.cancelled = true;
                break;
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    summary.inaccessible_entries += 1;
                    continue;
                }
            };
            if entry.path() != root && should_skip_entry(&entry, &excluded_names, include_hidden) {
                summary.skipped_entries += 1;
                continue;
            }
            if entry.file_type().is_file() && !extension_allowed(entry.path(), &request) {
                summary.skipped_entries += 1;
                continue;
            }
            let metadata = match entry.metadata() {
                Ok(metadata) => Some(metadata),
                Err(_) => {
                    summary.inaccessible_entries += 1;
                    None
                }
            };
            if metadata.as_ref().is_some_and(|metadata| metadata.is_dir()) {
                summary.directories_scanned += 1;
            } else {
                summary.files_scanned += 1;
            }
            let file_name = entry.file_name().to_string_lossy().to_string();
            if let Some(highlight) = filename_highlight_match(
                &file_name,
                entry.path(),
                &needle,
                request.case_sensitive,
                request.filename_match_mode,
            ) {
                let rank = highlight.rank;
                let identity = crate::file_search::model::normalize_path_for_identity(entry.path());
                if !seen_results.insert(identity) {
                    continue;
                }
                let result = FilenameResult {
                    path: entry.path().to_path_buf(),
                    file_name,
                    parent_directory: entry.path().parent().map(Path::to_path_buf),
                    kind: file_kind(metadata.as_ref()),
                    size: metadata.as_ref().filter(|m| m.is_file()).map(|m| m.len()),
                    modified: metadata.and_then(|m| m.modified().ok()),
                    rank,
                    match_quality: rank,
                    filename_match_ranges: highlight.filename_match_ranges,
                    path_match_ranges: highlight.path_match_ranges,
                    arrival_index: ranked_results.len(),
                };
                if event_sender
                    .send(SearchEvent::Result {
                        id: search_id,
                        result: SearchResult::Filename(result.clone()),
                    })
                    .is_err()
                {
                    break;
                }
                ranked_results.push(result);
                summary.results_found += 1;
                if summary.results_found >= request.max_results {
                    break;
                }
            }
        }
        if summary.results_found >= request.max_results {
            break;
        }
    }
    if cancellation.is_cancelled() {
        summary.cancelled = true;
    }
    summary.skipped_entries = summary
        .skipped_entries
        .saturating_add(skipped_before_descent.get());
    let status = if summary.cancelled {
        SearchStatus::Cancelled
    } else {
        SearchStatus::Completed
    };
    let _ = event_sender.send(SearchEvent::Progress {
        id: search_id,
        progress: SearchProgress {
            files_scanned: summary.files_scanned,
            directories_scanned: summary.directories_scanned,
            results_found: summary.results_found,
            status,
            global_truncated: false,
        },
    });
    let _ = if summary.cancelled {
        event_sender.send(SearchEvent::Cancelled { id: search_id })
    } else {
        event_sender.send(SearchEvent::Completed { id: search_id })
    };
    Ok(summary)
}

fn dedup_resolved_roots(roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut result: Vec<PathBuf> = Vec::new();
    'outer: for root in roots {
        for existing in &result {
            if root == *existing || root.starts_with(existing) {
                continue 'outer;
            }
        }
        result.retain(|existing| !existing.starts_with(&root));
        result.push(root);
    }
    result
}

fn should_descend(
    entry: &DirEntry,
    root: &Path,
    excluded_names: &HashSet<String>,
    include_hidden: bool,
) -> bool {
    if entry.path() == root || !entry.file_type().is_dir() {
        return true;
    }
    !should_skip_entry(entry, excluded_names, include_hidden)
}

fn should_skip_entry(
    entry: &DirEntry,
    excluded_names: &HashSet<String>,
    include_hidden: bool,
) -> bool {
    if entry.file_type().is_dir()
        && excluded_names.contains(&entry.file_name().to_string_lossy().to_string())
    {
        return true;
    }
    !include_hidden && is_hidden(entry)
}

fn is_hidden(entry: &DirEntry) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        if entry
            .metadata()
            .map(|m| m.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0)
            .unwrap_or(false)
        {
            return true;
        }
    }
    #[cfg(not(windows))]
    {
        entry.file_name().to_string_lossy().starts_with('.')
    }
    #[cfg(windows)]
    {
        entry.file_name().to_string_lossy().starts_with('.')
    }
}

fn extension_allowed(path: &Path, request: &SearchRequest) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let norm = |s: &String| s.trim_start_matches('.').to_ascii_lowercase();
    if !request.included_extensions.is_empty()
        && !request
            .included_extensions
            .iter()
            .map(norm)
            .any(|e| e == ext)
    {
        return false;
    }
    !request
        .excluded_extensions
        .iter()
        .map(norm)
        .any(|e| e == ext)
}

fn file_kind(metadata: Option<&std::fs::Metadata>) -> FileKind {
    match metadata {
        Some(metadata) if metadata.is_file() => FileKind::File,
        Some(metadata) if metadata.is_dir() => FileKind::Directory,
        _ => FileKind::Other,
    }
}

fn excluded_directory_names(
    request: &SearchRequest,
    _settings: &FileSearchSettings,
) -> HashSet<String> {
    request.excluded_directory_names.iter().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_search::model::SearchScope;
    use std::fs;
    use std::path::PathBuf;

    fn request(root: PathBuf, text: &str, max_results: usize) -> SearchRequest {
        SearchRequest {
            kind: SearchKind::Filename,
            scope: SearchScope::Roots { roots: vec![root] },
            text: text.to_owned(),
            case_sensitive: false,
            include_hidden_files: false,
            max_results,
            max_file_size_bytes: 1024,
            included_extensions: vec![],
            excluded_extensions: vec![],
            excluded_directory_names: vec![],
            filename_match_mode: crate::file_search::model::FilenameMatchMode::RankedSubstring,
            content_match_mode: crate::file_search::model::ContentMatchMode::ExactPhrase,
            whole_word: false,
            file_type_filter: crate::file_search::model::FileTypeFilter::FilesAndDirectories,
        }
    }

    fn run(req: SearchRequest) -> (WalkDirSearchSummary, Vec<SearchEvent>) {
        let (tx, rx) = mpsc::channel();
        let summary = search_filenames_in_directory(
            req,
            &FileSearchSettings::default(),
            &CancellationToken::new(),
            &tx,
            SearchId(7),
        )
        .unwrap();
        (summary, rx.try_iter().collect())
    }

    #[test]
    fn finds_nested_matching_files_directories_unicode_and_duplicates() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("nested/target_name_dir")).unwrap();
        fs::write(temp.path().join("nested/target_name.txt"), "a").unwrap();
        fs::write(
            temp.path().join("nested/target_name_dir/target_name.txt"),
            "b",
        )
        .unwrap();
        fs::write(temp.path().join("nested/ユニコード_target.txt"), "c").unwrap();

        let (summary, events) = run(request(temp.path().to_path_buf(), "target", 20));

        assert_eq!(summary.results_found, 4);
        let results: Vec<_> = events
            .into_iter()
            .filter_map(|event| match event {
                SearchEvent::Result {
                    result: SearchResult::Filename(result),
                    ..
                } => Some(result),
                _ => None,
            })
            .collect();
        assert!(results.iter().any(|r| r.kind == FileKind::Directory));
        assert!(results.iter().any(|r| r.file_name.contains("ユニコード")));
        assert_eq!(
            results
                .iter()
                .filter(|r| r.file_name == "target_name.txt")
                .count(),
            2
        );
        assert!(results.iter().all(|r| r.parent_directory.is_some()));
    }

    #[test]
    fn excludes_configured_and_hidden_directories_before_descent() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("node_modules")).unwrap();
        fs::create_dir_all(temp.path().join(".secret")).unwrap();
        fs::write(temp.path().join("node_modules/match.txt"), "a").unwrap();
        fs::write(temp.path().join(".secret/match.txt"), "b").unwrap();
        fs::write(temp.path().join("match.txt"), "c").unwrap();

        let mut req = request(temp.path().to_path_buf(), "match", 20);
        req.excluded_directory_names = vec!["node_modules".into()];
        let (summary, events) = run(req);
        let count = events
            .iter()
            .filter(|e| matches!(e, SearchEvent::Result { .. }))
            .count();
        assert_eq!(count, 1);
        assert_eq!(summary.results_found, 1);
    }

    #[test]
    fn invalid_roots_are_reported_while_valid_roots_are_searched() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("match.txt"), "a").unwrap();
        let mut req = request(temp.path().to_path_buf(), "match", 20);
        req.scope = SearchScope::Roots {
            roots: vec![temp.path().to_path_buf(), temp.path().join("missing")],
        };
        let (summary, events) = run(req);
        assert_eq!(summary.results_found, 1);
        assert_eq!(summary.root_errors.len(), 1);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, SearchEvent::Result { .. }))
        );
    }

    #[test]
    fn repeated_and_overlapping_roots_do_not_duplicate_results() {
        let temp = tempfile::tempdir().unwrap();
        let nested = temp.path().join("nested");
        fs::create_dir(&nested).unwrap();
        fs::write(nested.join("match.txt"), "a").unwrap();
        let mut req = request(temp.path().to_path_buf(), "match", 20);
        req.scope = SearchScope::Roots {
            roots: vec![temp.path().to_path_buf(), nested, temp.path().to_path_buf()],
        };
        let (summary, events) = run(req);
        assert_eq!(summary.results_found, 1);
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, SearchEvent::Result { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn empty_or_invalid_roots_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let mut req = request(temp.path().join("missing"), "match", 20);
        req.scope = SearchScope::Roots { roots: vec![] };
        let (tx, _) = mpsc::channel();
        assert!(
            search_filenames_in_directory(
                req,
                &FileSearchSettings::default(),
                &CancellationToken::new(),
                &tx,
                SearchId(1)
            )
            .is_err()
        );
    }

    #[test]
    fn includes_hidden_entries_when_requested() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join(".match.txt"), "hidden").unwrap();
        let mut req = request(temp.path().to_path_buf(), "match", 20);
        req.include_hidden_files = true;

        let (summary, events) = run(req);

        assert_eq!(summary.results_found, 1);
        assert!(events.iter().any(|event| matches!(
            event,
            SearchEvent::Result {
                result: SearchResult::Filename(result),
                ..
            } if result.file_name == ".match.txt"
        )));
    }

    #[test]
    fn respects_result_limits_and_cancellation() {
        let temp = tempfile::tempdir().unwrap();
        for i in 0..10 {
            fs::write(temp.path().join(format!("match-{i}.txt")), "a").unwrap();
        }
        let (summary, _) = run(request(temp.path().to_path_buf(), "match", 3));
        assert_eq!(summary.results_found, 3);

        let (tx, _rx) = mpsc::channel();
        let token = CancellationToken::new();
        token.cancel();
        let summary = search_filenames_in_directory(
            request(temp.path().to_path_buf(), "match", 20),
            &FileSearchSettings::default(),
            &token,
            &tx,
            SearchId(8),
        )
        .unwrap();
        assert!(summary.cancelled);
        assert_eq!(summary.results_found, 0);
    }

    #[test]
    fn invalid_scope_and_kind_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let mut req = request(temp.path().to_path_buf(), "x", 10);
        req.kind = SearchKind::Content;
        assert!(
            search_filenames_in_directory(
                req,
                &FileSearchSettings::default(),
                &CancellationToken::new(),
                &mpsc::channel().0,
                SearchId(1)
            )
            .is_err()
        );

        let mut req = request(temp.path().to_path_buf(), "x", 10);
        req.scope = SearchScope::Roots { roots: Vec::new() };
        assert!(
            search_filenames_in_directory(
                req,
                &FileSearchSettings::default(),
                &CancellationToken::new(),
                &mpsc::channel().0,
                SearchId(1)
            )
            .is_err()
        );
    }
}
