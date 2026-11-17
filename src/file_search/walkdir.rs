use crate::file_search::coordinator::{CancellationToken, SearchEventSender, SearchExecutor};
use crate::file_search::matching::filename_highlight_match;
use crate::file_search::model::{
    FileKind, FileTypeFilter, FilenameResult, SearchDiagnostic, SearchEvent, SearchId, SearchKind, SearchProgress,
    SearchRequest, SearchResult, SearchScope, SearchStatus,
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
        events: SearchEventSender,
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
    pub metadata_unavailable: u64,
    pub global_truncated: bool,
}

pub fn search_filenames_in_directory(
    request: SearchRequest,
    settings: &FileSearchSettings,
    cancellation: &CancellationToken,
    event_sender: &SearchEventSender,
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
            Ok(resolved) if resolved.is_dir() || (matches!(&request.scope, SearchScope::Files { .. }) && resolved.is_file()) => Some(resolved),
            Ok(resolved) => {
                summary
                    .root_errors
                    .push(format!("{} is not a directory", resolved.display()));
                None
            }
            Err(err) => {
                if request.metadata.active() { summary.metadata_unavailable = summary.metadata_unavailable.saturating_add(1); }
                summary
                    .root_errors
                    .push(format!("{}: {err}", root.display()));
                None
            }
        })
        .collect();
    let roots = dedup_resolved_roots(resolved_roots);
    if roots.is_empty() {
        if summary.metadata_unavailable > 0 { return Err(format!("No usable search roots; metadata unavailable for {} entries.", summary.metadata_unavailable)); }
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
            if (entry.path() != root || entry.file_type().is_file()) && should_skip_entry(&entry, &excluded_names, include_hidden) {
                summary.skipped_entries += 1;
                continue;
            }
            if entry.file_type().is_file() && !extension_allowed(entry.path(), &request) {
                summary.skipped_entries += 1;
                continue;
            }
            if entry.file_type().is_dir() { summary.directories_scanned += 1; } else { summary.files_scanned += 1; }
            let allowed_type = match request.file_type_filter {
                FileTypeFilter::FilesOnly => entry.file_type().is_file(),
                FileTypeFilter::DirectoriesOnly => entry.file_type().is_dir(),
                FileTypeFilter::FilesAndDirectories => true,
            };
            if !allowed_type { continue; }
            let file_name = entry.file_name().to_string_lossy().to_string();
            let Some(highlight) = filename_highlight_match(&file_name, entry.path(), &needle, request.case_sensitive, request.filename_match_mode) else { continue; };
            let metadata = match entry.metadata() {
                Ok(metadata) => Some(metadata),
                Err(_) => {
                    summary.inaccessible_entries += 1;
                    None
                }
            };
            match crate::file_search::predicate::metadata_matches(&request.metadata, metadata.as_ref()) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(_) => { summary.metadata_unavailable = summary.metadata_unavailable.saturating_add(1); continue; }
            }
            {
                let rank = highlight.rank;
                let identity = crate::file_search::model::normalize_path_for_identity(entry.path());
                if !seen_results.insert(identity) {
                    continue;
                }
                if ranked_results.len() >= request.max_results { summary.global_truncated = true; break; }
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
                if !request.metadata.active() && summary.results_found >= request.max_results { break; }
            }
        }
        if summary.global_truncated || (!request.metadata.active() && summary.results_found >= request.max_results) {
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
            global_truncated: summary.global_truncated,
        },
    });
    if summary.metadata_unavailable > 0 {
        let _ = event_sender.send(SearchEvent::Diagnostic { id: search_id, diagnostic: SearchDiagnostic::Warning(format!("Metadata unavailable for {} entries; they were excluded by the active filters.", summary.metadata_unavailable)) });
    }
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
            metadata_preferences: Default::default(), metadata: Default::default(),
        }
    }

    fn run(req: SearchRequest) -> (WalkDirSearchSummary, Vec<SearchEvent>) {
        let (tx, rx) = mpsc::channel();
        let event_sender: SearchEventSender = tx.into();
        let summary = search_filenames_in_directory(
            req,
            &FileSearchSettings::default(),
            &CancellationToken::new(),
            &event_sender,
            SearchId(7),
        )
        .unwrap();
        (summary, rx.try_iter().collect())
    }

    #[test]
    fn filename_contains_filters_leaf_and_type_before_the_result_budget() {
        use crate::file_search::model::FilenameMatchMode;

        let temp = tempfile::tempdir().unwrap();
        let path_only = temp.path().join("gcc");
        let valid = temp.path().join("valid");
        fs::create_dir_all(path_only.join("skipme")).unwrap();
        fs::create_dir_all(&valid).unwrap();
        fs::write(path_only.join("readme.txt"), "path only").unwrap();
        fs::write(path_only.join("skipme/gcc-excluded.txt"), "excluded").unwrap();
        fs::write(valid.join("gcc.exe"), "match").unwrap();
        let mut req = request(path_only.clone(), "gcc", 1);
        req.scope = SearchScope::Roots {
            roots: vec![path_only.clone(), valid.clone()],
        };
        req.filename_match_mode = FilenameMatchMode::FilenameContains;
        req.file_type_filter = FileTypeFilter::FilesOnly;
        req.excluded_directory_names = vec!["skipme".into()];
        let (summary, events) = run(req);
        let results = events
            .into_iter()
            .filter_map(|event| match event {
                SearchEvent::Result {
                    result: SearchResult::Filename(result),
                    ..
                } => Some(result),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(summary.results_found, 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_name, "gcc.exe");
        assert_eq!(results[0].kind, FileKind::File);
        assert!(results[0].path_match_ranges.is_empty());

        let file_first = temp.path().join("file-first");
        let dir_second = temp.path().join("dir-second");
        fs::create_dir_all(dir_second.join("gcc-directory")).unwrap();
        fs::create_dir_all(&file_first).unwrap();
        fs::write(file_first.join("gcc-file"), "match").unwrap();
        let mut req = request(file_first.clone(), "gcc", 1);
        req.scope = SearchScope::Roots {
            roots: vec![file_first, dir_second],
        };
        req.filename_match_mode = FilenameMatchMode::FilenameContains;
        req.file_type_filter = FileTypeFilter::DirectoriesOnly;
        let (summary, events) = run(req);
        let results = events
            .into_iter()
            .filter_map(|event| match event {
                SearchEvent::Result {
                    result: SearchResult::Filename(result),
                    ..
                } => Some(result),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(summary.results_found, 1);
        assert_eq!(results[0].file_name, "gcc-directory");
        assert_eq!(results[0].kind, FileKind::Directory);
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
        let event_sender: SearchEventSender = tx.into();
        assert!(
            search_filenames_in_directory(
                req,
                &FileSearchSettings::default(),
                &CancellationToken::new(),
                &event_sender,
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
        let event_sender: SearchEventSender = tx.into();
        let token = CancellationToken::new();
        token.cancel();
        let summary = search_filenames_in_directory(
            request(temp.path().to_path_buf(), "match", 20),
            &FileSearchSettings::default(),
            &token,
            &event_sender,
            SearchId(8),
        )
        .unwrap();
        assert!(summary.cancelled);
        assert_eq!(summary.results_found, 0);
    }

    #[test]
    fn explicit_metadata_filtered_files_count_only_eligible_matches() {
        use crate::file_search::predicate::SizePredicate;
        let temp = tempfile::tempdir().unwrap();
        let small = temp.path().join("match-small.txt"); let large = temp.path().join("match-large.txt");
        fs::write(&small, "x").unwrap(); fs::write(&large, "large").unwrap();
        let mut request = request(temp.path().into(), "match", 1);
        request.scope = SearchScope::Files { files: vec![small, large.clone()] };
        request.metadata.size = Some(SizePredicate::LargerThan(1));
        let (summary, events) = run(request.clone());
        assert_eq!(summary.results_found, 1);
        assert!(!summary.global_truncated);
        assert!(events.iter().any(|event| matches!(event, SearchEvent::Result { result: SearchResult::Filename(result), .. } if result.path == large.canonicalize().unwrap())));
        let extra = temp.path().join("match-extra.txt"); fs::write(&extra, "also large").unwrap();
        request.scope = SearchScope::Files { files: vec![large, extra] };
        assert!(run(request).0.global_truncated);
    }

    #[test]
    fn unfiltered_cap_does_not_visit_remaining_explicit_files() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("match.txt"); let second = temp.path().join("other.txt");
        fs::write(&first, "x").unwrap(); fs::write(&second, "x").unwrap();
        let mut request = request(temp.path().into(), "match", 1);
        request.scope = SearchScope::Files { files: vec![first, second] };
        let (unfiltered, _) = run(request.clone());
        assert_eq!(unfiltered.files_scanned, 1);
        assert!(!unfiltered.global_truncated);
        request.metadata.size = Some(crate::file_search::predicate::SizePredicate::LargerThan(0));
        let (filtered, _) = run(request);
        assert_eq!(filtered.files_scanned, 2);
        assert!(!filtered.global_truncated);
    }

    #[test]
    fn rejected_directory_does_not_prune_eligible_descendant() {
        use crate::file_search::predicate::DateWindow;
        let temp = tempfile::tempdir().unwrap();
        let sub = temp.path().join("match-directory"); fs::create_dir(&sub).unwrap();
        let child = sub.join("match-child.txt"); fs::write(&child, "").unwrap();
        let target = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
        fs::OpenOptions::new().write(true).open(&child).unwrap().set_times(fs::FileTimes::new().set_modified(target)).unwrap();
        let mut request = request(temp.path().into(), "match", 5);
        request.metadata.date = Some(DateWindow { start: target - std::time::Duration::from_secs(1), end: target + std::time::Duration::from_secs(1), end_inclusive: true });
        let (summary, events) = run(request);
        assert_eq!(summary.results_found, 1);
        assert!(events.iter().any(|event| matches!(event, SearchEvent::Result { result: SearchResult::Filename(result), .. } if result.file_name == "match-child.txt")));
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
                &SearchEventSender::from(mpsc::channel().0),
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
                &SearchEventSender::from(mpsc::channel().0),
                SearchId(1)
            )
            .is_err()
        );
    }
}
