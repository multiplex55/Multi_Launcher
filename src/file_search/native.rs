use crate::file_search::coordinator::{CancellationToken, SearchExecutor};
use crate::file_search::model::{
    ContentFileResult, ContentFileResultBuilder, ContentMatch, ContentMatchMode, ContentMatchRange,
    SearchEvent, SearchId, SearchKind, SearchProgress, SearchRequest, SearchResult, SearchScope,
    SearchStatus,
};
use crate::file_search::settings::FileSearchSettings;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use walkdir::{DirEntry, WalkDir};

#[derive(Debug, Clone)]
pub struct NativeSearchExecutor {
    settings: FileSearchSettings,
}

impl NativeSearchExecutor {
    pub fn new(settings: FileSearchSettings) -> Self {
        Self { settings }
    }
}

impl SearchExecutor for NativeSearchExecutor {
    fn execute(
        &self,
        id: SearchId,
        request: SearchRequest,
        token: CancellationToken,
        events: mpsc::Sender<SearchEvent>,
    ) {
        if let Err(error) = search_content_native(id, request, &self.settings, &token, &events) {
            let _ = events.send(SearchEvent::Failed { id, error });
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NativeSearchSummary {
    pub results: Vec<ContentFileResult>,
    pub results_found: usize,
    pub files_scanned: u64,
    pub directories_scanned: u64,
    pub cancelled: bool,
    pub global_truncated: bool,
}

pub fn search_content_native(
    id: SearchId,
    request: SearchRequest,
    settings: &FileSearchSettings,
    cancellation: &CancellationToken,
    events: &mpsc::Sender<SearchEvent>,
) -> Result<(), String> {
    let summary = search_content_native_summary(request, settings, cancellation)?;
    for result in summary.results.iter().cloned() {
        if cancellation.is_cancelled() {
            let mut cancelled_summary = summary.clone();
            cancelled_summary.cancelled = true;
            send_terminal(id, &cancelled_summary, events);
            return Ok(());
        }
        if events
            .send(SearchEvent::Result {
                id,
                result: SearchResult::ContentFile(result),
            })
            .is_err()
        {
            return Ok(());
        }
    }
    send_terminal(id, &summary, events);
    Ok(())
}

fn send_terminal(id: SearchId, summary: &NativeSearchSummary, events: &mpsc::Sender<SearchEvent>) {
    let status = if summary.cancelled {
        SearchStatus::Cancelled
    } else {
        SearchStatus::Completed
    };
    let _ = events.send(SearchEvent::Progress {
        id,
        progress: SearchProgress {
            files_scanned: summary.files_scanned,
            directories_scanned: summary.directories_scanned,
            results_found: summary.results_found,
            status,
            global_truncated: summary.global_truncated,
        },
    });
    let _ = if summary.cancelled {
        events.send(SearchEvent::Cancelled { id })
    } else {
        events.send(SearchEvent::Completed { id })
    };
}

pub fn search_content_native_summary(
    request: SearchRequest,
    settings: &FileSearchSettings,
    cancellation: &CancellationToken,
) -> Result<NativeSearchSummary, String> {
    if request.kind != SearchKind::Content {
        return Err("native search only supports content requests".to_owned());
    }
    let matcher = ContentMatcher::new(&request)?;
    let mut summary = NativeSearchSummary::default();
    let mut seen = HashSet::new();
    let paths = collect_paths(&request, settings, cancellation, &mut summary, &mut seen);
    for path in paths {
        if cancellation.is_cancelled() {
            summary.cancelled = true;
            break;
        }
        if summary.results.len() >= request.max_results {
            summary.global_truncated = true;
            break;
        }
        if let Some(result) = search_file(
            &path,
            &request,
            settings,
            cancellation,
            &matcher,
            &mut summary,
        ) {
            summary.results_found += 1;
            summary.results.push(result);
        }
    }
    if cancellation.is_cancelled() {
        summary.cancelled = true;
    }
    Ok(summary)
}

fn collect_paths(
    request: &SearchRequest,
    settings: &FileSearchSettings,
    cancellation: &CancellationToken,
    summary: &mut NativeSearchSummary,
    seen: &mut HashSet<String>,
) -> Vec<PathBuf> {
    match &request.scope {
        SearchScope::Files { files } => files
            .iter()
            .filter(|p| accept_file_path(p, request, seen))
            .cloned()
            .collect(),
        SearchScope::Roots { roots } => {
            let roots = if roots.is_empty() {
                &settings.global_search_roots
            } else {
                roots
            };
            let mut out = Vec::new();
            for root in roots {
                if cancellation.is_cancelled() {
                    summary.cancelled = true;
                    break;
                }
                if root.is_file() {
                    if accept_file_path(root, request, seen) {
                        out.push(root.clone());
                    }
                } else if root.is_dir() {
                    let walker = WalkDir::new(root)
                        .follow_links(false)
                        .into_iter()
                        .filter_entry(|e| should_descend(e, request));
                    for entry in walker {
                        if cancellation.is_cancelled() {
                            summary.cancelled = true;
                            break;
                        }
                        let Ok(entry) = entry else { continue };
                        if entry.file_type().is_dir() {
                            summary.directories_scanned += 1;
                            continue;
                        }
                        if !entry.file_type().is_file() {
                            continue;
                        }
                        let path = entry.into_path();
                        if accept_file_path(&path, request, seen) {
                            out.push(path);
                        }
                    }
                }
            }
            out
        }
    }
}

fn should_descend(entry: &DirEntry, request: &SearchRequest) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    if !request.include_hidden_files && is_hidden(entry.path()) {
        return false;
    }
    if entry.file_type().is_dir() {
        if let Some(name) = entry.file_name().to_str() {
            return !request.excluded_directory_names.iter().any(|d| d == name);
        }
    }
    true
}

fn accept_file_path(path: &Path, request: &SearchRequest, seen: &mut HashSet<String>) -> bool {
    if !request.include_hidden_files && is_hidden(path) {
        return false;
    }
    if !extension_allowed(path, request) {
        return false;
    }
    let id = crate::file_search::model::normalize_path_for_identity(path);
    seen.insert(id)
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

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'))
}

fn search_file(
    path: &Path,
    request: &SearchRequest,
    settings: &FileSearchSettings,
    cancellation: &CancellationToken,
    matcher: &ContentMatcher,
    summary: &mut NativeSearchSummary,
) -> Option<ContentFileResult> {
    if cancellation.is_cancelled() {
        summary.cancelled = true;
        return None;
    }
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > request.max_file_size_bytes {
        return None;
    }
    summary.files_scanned += 1;
    if cancellation.is_cancelled() {
        summary.cancelled = true;
        return None;
    }
    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    if reader.fill_buf().ok().is_some_and(|b| b.contains(&0)) {
        return None;
    }
    let display_limit = match &request.scope {
        SearchScope::Files { .. } => usize::MAX,
        _ => settings.max_matches_per_content_file,
    };
    let mut builder = ContentFileResultBuilder::new(path.to_path_buf(), display_limit);
    let mut buf = Vec::new();
    let mut line_number = 0;
    loop {
        if cancellation.is_cancelled() {
            summary.cancelled = true;
            break;
        }
        buf.clear();
        let read = reader.read_until(b'\n', &mut buf).ok()?;
        if read == 0 {
            break;
        }
        line_number += 1;
        if buf.ends_with(b"\n") {
            buf.pop();
            if buf.ends_with(b"\r") {
                buf.pop();
            }
        }
        let line = String::from_utf8_lossy(&buf).into_owned();
        let ranges = matcher.find(&line);
        if !ranges.is_empty() {
            let byte_start = ranges[0].byte_start;
            let byte_end = ranges[0].byte_end;
            builder.push_match(ContentMatch {
                line_number,
                column: Some(byte_start),
                line,
                byte_start,
                byte_end,
                ranges,
            });
        }
    }
    let result = builder.finish();
    (result.total_matches > 0).then_some(result)
}

#[derive(Debug, Clone)]
struct ContentMatcher {
    terms: Vec<String>,
    case_sensitive: bool,
    whole_word: bool,
}
impl ContentMatcher {
    fn new(request: &SearchRequest) -> Result<Self, String> {
        let mut terms = match request.content_match_mode {
            ContentMatchMode::ExactPhrase => vec![request.text.trim().to_owned()],
            ContentMatchMode::AnyTerm => {
                request.text.split_whitespace().map(str::to_owned).collect()
            }
        };
        terms.retain(|t| !t.is_empty());
        if terms.is_empty() {
            return Err("content search query cannot be empty".into());
        }
        let mut seen = HashSet::new();
        terms.retain(|t| {
            seen.insert(if request.case_sensitive {
                t.clone()
            } else {
                t.to_lowercase()
            })
        });
        Ok(Self {
            terms,
            case_sensitive: request.case_sensitive,
            whole_word: request.whole_word,
        })
    }
    fn find(&self, line: &str) -> Vec<ContentMatchRange> {
        let hay = if self.case_sensitive {
            line.to_owned()
        } else {
            line.to_lowercase()
        };
        let mut candidates = Vec::new();
        for (term_idx, term) in self.terms.iter().enumerate() {
            let needle = if self.case_sensitive {
                term.clone()
            } else {
                term.to_lowercase()
            };
            let mut pos = 0;
            while let Some(rel) = hay[pos..].find(&needle) {
                let start = pos + rel;
                let end = start + needle.len();
                if !self.whole_word || whole_word_at(line, start, end) {
                    candidates.push((start, end, term_idx));
                }
                pos = start.saturating_add(1);
            }
        }
        candidates.sort_by_key(|&(s, e, i)| (s, e, i));
        let mut ranges = Vec::new();
        let mut last_end = 0;
        for (s, e, _) in candidates {
            if s >= last_end {
                ranges.push(ContentMatchRange {
                    byte_start: s,
                    byte_end: e,
                });
                last_end = e;
            }
        }
        ranges
    }
}

fn whole_word_at(line: &str, start: usize, end: usize) -> bool {
    let before = line[..start].chars().next_back();
    let after = line[end..].chars().next();
    !before.is_some_and(is_word) && !after.is_some_and(is_word)
}
fn is_word(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_search::model::{FileTypeFilter, FilenameMatchMode};
    use tempfile::tempdir;

    fn req(root: PathBuf, text: &str) -> SearchRequest {
        SearchRequest {
            kind: SearchKind::Content,
            scope: SearchScope::Roots { roots: vec![root] },
            text: text.into(),
            case_sensitive: true,
            include_hidden_files: false,
            max_results: 100,
            max_file_size_bytes: 1024 * 1024,
            included_extensions: vec![],
            excluded_extensions: vec![],
            excluded_directory_names: vec![],
            filename_match_mode: FilenameMatchMode::RankedSubstring,
            content_match_mode: ContentMatchMode::ExactPhrase,
            whole_word: false,
            file_type_filter: FileTypeFilter::FilesOnly,
        }
    }
    fn run(request: SearchRequest) -> NativeSearchSummary {
        search_content_native_summary(
            request,
            &FileSearchSettings::default(),
            &CancellationToken::new(),
        )
        .unwrap()
    }
    fn write(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn literal_phrase_matching() {
        let d = tempdir().unwrap();
        write(&d.path().join("a.txt"), b"one two\n");
        assert_eq!(
            run(req(d.path().into(), "one two")).results[0].total_matches,
            1
        );
    }
    #[test]
    fn any_term_matching() {
        let d = tempdir().unwrap();
        write(&d.path().join("a.txt"), b"alpha beta alpha\n");
        let mut r = req(d.path().into(), "beta alpha alpha");
        r.content_match_mode = ContentMatchMode::AnyTerm;
        let m = &run(r).results[0].matches[0];
        assert_eq!(m.ranges.len(), 3);
    }
    #[test]
    fn whole_word_boundaries() {
        let d = tempdir().unwrap();
        write(&d.path().join("a.txt"), b"cat scatter cat_ cat\n");
        let mut r = req(d.path().into(), "cat");
        r.whole_word = true;
        let m = &run(r).results[0].matches[0];
        assert_eq!(m.ranges.len(), 2);
    }
    #[test]
    fn case_sensitivity() {
        let d = tempdir().unwrap();
        write(&d.path().join("a.txt"), b"Needle needle\n");
        assert_eq!(
            run(req(d.path().into(), "needle")).results[0].matches[0]
                .ranges
                .len(),
            1
        );
        let mut r = req(d.path().into(), "needle");
        r.case_sensitive = false;
        assert_eq!(run(r).results[0].matches[0].ranges.len(), 2);
    }
    #[test]
    fn multiple_matches_on_one_line() {
        let d = tempdir().unwrap();
        write(&d.path().join("a.txt"), b"x x x\n");
        assert_eq!(
            run(req(d.path().into(), "x")).results[0].matches[0]
                .ranges
                .len(),
            3
        );
    }
    #[test]
    fn multiple_matching_files() {
        let d = tempdir().unwrap();
        write(&d.path().join("a.txt"), b"x\n");
        write(&d.path().join("b.txt"), b"x\n");
        assert_eq!(run(req(d.path().into(), "x")).results.len(), 2);
    }
    #[test]
    fn included_extensions() {
        let d = tempdir().unwrap();
        write(&d.path().join("a.rs"), b"x\n");
        write(&d.path().join("b.txt"), b"x\n");
        let mut r = req(d.path().into(), "x");
        r.included_extensions = vec!["rs".into()];
        assert_eq!(run(r).results[0].file_name, "a.rs");
    }
    #[test]
    fn excluded_extensions() {
        let d = tempdir().unwrap();
        write(&d.path().join("a.rs"), b"x\n");
        write(&d.path().join("b.txt"), b"x\n");
        let mut r = req(d.path().into(), "x");
        r.excluded_extensions = vec!["rs".into()];
        assert_eq!(run(r).results[0].file_name, "b.txt");
    }
    #[test]
    fn excluded_directories() {
        let d = tempdir().unwrap();
        write(&d.path().join("skip/a.txt"), b"x\n");
        write(&d.path().join("keep/b.txt"), b"x\n");
        let mut r = req(d.path().into(), "x");
        r.excluded_directory_names = vec!["skip".into()];
        assert_eq!(run(r).results[0].file_name, "b.txt");
    }
    #[test]
    fn hidden_file_behavior() {
        let d = tempdir().unwrap();
        write(&d.path().join(".hidden.txt"), b"x\n");
        assert!(run(req(d.path().into(), "x")).results.is_empty());
        let mut r = req(d.path().into(), "x");
        r.include_hidden_files = true;
        assert_eq!(run(r).results.len(), 1);
    }
    #[test]
    fn file_size_limits() {
        let d = tempdir().unwrap();
        write(&d.path().join("a.txt"), b"xxxx\n");
        let mut r = req(d.path().into(), "x");
        r.max_file_size_bytes = 2;
        assert!(run(r).results.is_empty());
    }
    #[test]
    fn binary_file_skipping() {
        let d = tempdir().unwrap();
        write(&d.path().join("a.txt"), b"x\0x\n");
        assert!(run(req(d.path().into(), "x")).results.is_empty());
    }
    #[test]
    fn per_file_truncation() {
        let d = tempdir().unwrap();
        write(&d.path().join("a.txt"), b"x\nx\nx\n");
        let settings = FileSearchSettings {
            max_matches_per_content_file: 1,
            ..Default::default()
        };
        let s = search_content_native_summary(
            req(d.path().into(), "x"),
            &settings,
            &CancellationToken::new(),
        )
        .unwrap();
        assert_eq!(s.results[0].total_matches, 3);
        assert!(s.results[0].truncated);
    }
    #[test]
    fn global_truncation() {
        let d = tempdir().unwrap();
        write(&d.path().join("a.txt"), b"x\n");
        write(&d.path().join("b.txt"), b"x\n");
        let mut r = req(d.path().into(), "x");
        r.max_results = 1;
        let s = run(r);
        assert_eq!(s.results.len(), 1);
        assert!(s.global_truncated);
    }
    #[test]
    fn cancellation() {
        let d = tempdir().unwrap();
        write(&d.path().join("a.txt"), b"x\n");
        let t = CancellationToken::new();
        t.cancel();
        assert!(
            search_content_native_summary(
                req(d.path().into(), "x"),
                &FileSearchSettings::default(),
                &t
            )
            .unwrap()
            .cancelled
        );
    }
    #[test]
    fn duplicate_roots() {
        let d = tempdir().unwrap();
        write(&d.path().join("a.txt"), b"x\n");
        let mut r = req(d.path().into(), "x");
        r.scope = SearchScope::Roots {
            roots: vec![d.path().into(), d.path().into()],
        };
        assert_eq!(run(r).results.len(), 1);
    }
    #[test]
    fn explicit_file_scopes() {
        let d = tempdir().unwrap();
        let a = d.path().join("a.txt");
        let b = d.path().join("b.txt");
        write(&a, b"x\n");
        write(&b, b"x\n");
        let mut r = req(d.path().into(), "x");
        r.scope = SearchScope::Files { files: vec![a] };
        assert_eq!(run(r).results.len(), 1);
    }
    #[test]
    fn unicode_paths_and_content() {
        let d = tempdir().unwrap();
        write(&d.path().join("unicodé/雪.txt"), "café ☕\n".as_bytes());
        let s = run(req(d.path().into(), "fé"));
        assert_eq!(s.results[0].matches[0].byte_start, 2);
    }
}
