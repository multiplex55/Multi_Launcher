use multi_launcher::file_search::coordinator::CancellationToken;
use multi_launcher::file_search::model::{
    ContentMatchMode, FileTypeFilter, FilenameMatchMode, SearchEvent, SearchId, SearchKind,
    SearchRequest, SearchResult, SearchScope,
};
use multi_launcher::file_search::native::search_content_native_summary;
use multi_launcher::file_search::settings::FileSearchSettings;
use multi_launcher::file_search::walkdir::search_filenames_in_directory;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::mpsc,
};

fn req(kind: SearchKind, roots: Vec<PathBuf>, text: &str) -> SearchRequest {
    SearchRequest {
        kind,
        scope: SearchScope::Roots { roots },
        text: text.into(),
        case_sensitive: false,
        include_hidden_files: false,
        max_results: 100,
        max_file_size_bytes: 1024 * 1024,
        included_extensions: vec![],
        excluded_extensions: vec![],
        excluded_directory_names: vec![],
        filename_match_mode: FilenameMatchMode::RankedSubstring,
        content_match_mode: ContentMatchMode::ExactPhrase,
        whole_word: false,
        file_type_filter: FileTypeFilter::FilesAndDirectories,
    }
}
fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}
fn filenames(request: SearchRequest, settings: &FileSearchSettings) -> (Vec<String>, Vec<String>) {
    let (tx, rx) = mpsc::channel();
    let summary = search_filenames_in_directory(
        request,
        settings,
        &CancellationToken::new(),
        &tx,
        SearchId(1),
    )
    .unwrap();
    let names = rx
        .try_iter()
        .filter_map(|e| match e {
            SearchEvent::Result {
                result: SearchResult::Filename(r),
                ..
            } => Some(r.file_name),
            _ => None,
        })
        .collect();
    (names, summary.root_errors)
}

#[test]
fn global_roots_resolve_and_search_only_configured_roots() {
    let d = tempfile::tempdir().unwrap();
    let configured = d.path().join("configured");
    let outside = d.path().join("outside");
    write(&configured.join("needle.txt"), "");
    write(&outside.join("needle.txt"), "");
    let settings = FileSearchSettings {
        global_search_roots: vec![configured.clone()],
        ..Default::default()
    };
    let (names, _) = filenames(
        req(
            SearchKind::Filename,
            settings.global_search_roots.clone(),
            "needle",
        ),
        &settings,
    );
    assert_eq!(names, vec!["needle.txt"]);
}

#[test]
fn multi_root_custom_searches_include_each_root() {
    let d = tempfile::tempdir().unwrap();
    let a = d.path().join("a");
    let b = d.path().join("b");
    write(&a.join("needle-a.txt"), "");
    write(&b.join("needle-b.txt"), "");
    let (mut names, _) = filenames(
        req(SearchKind::Filename, vec![a, b], "needle"),
        &Default::default(),
    );
    names.sort();
    assert_eq!(names, vec!["needle-a.txt", "needle-b.txt"]);
}

#[test]
fn overlapping_roots_are_deduplicated() {
    let d = tempfile::tempdir().unwrap();
    let nested = d.path().join("nested");
    write(&nested.join("needle.txt"), "");
    let (names, _) = filenames(
        req(
            SearchKind::Filename,
            vec![d.path().into(), nested],
            "needle",
        ),
        &Default::default(),
    );
    assert_eq!(names, vec!["needle.txt"]);
}

#[test]
fn invalid_roots_reported_while_valid_roots_still_search() {
    let d = tempfile::tempdir().unwrap();
    write(&d.path().join("needle.txt"), "");
    let (names, errors) = filenames(
        req(
            SearchKind::Filename,
            vec![d.path().into(), d.path().join("missing")],
            "needle",
        ),
        &Default::default(),
    );
    assert_eq!(names, vec!["needle.txt"]);
    assert_eq!(errors.len(), 1);
}

#[test]
fn filename_search_honors_include_and_exclude_extensions() {
    let d = tempfile::tempdir().unwrap();
    write(&d.path().join("needle.rs"), "");
    write(&d.path().join("needle.txt"), "");
    write(&d.path().join("needle.md"), "");
    let mut r = req(SearchKind::Filename, vec![d.path().into()], "needle");
    r.included_extensions = vec!["rs".into(), ".txt".into()];
    r.excluded_extensions = vec!["txt".into()];
    let (names, _) = filenames(r, &Default::default());
    assert_eq!(names, vec!["needle.rs"]);
}

#[test]
fn content_search_uses_native_fallback_without_external_tools() {
    let d = tempfile::tempdir().unwrap();
    write(&d.path().join("a.txt"), "alpha needle\n");
    write(&d.path().join("b.txt"), "nothing\n");
    let summary = search_content_native_summary(
        req(SearchKind::Content, vec![d.path().into()], "needle"),
        &Default::default(),
        &CancellationToken::new(),
    )
    .unwrap();
    assert_eq!(summary.results.len(), 1);
    assert_eq!(summary.results[0].file_name, "a.txt");
}

#[test]
fn hidden_file_behavior_is_configurable() {
    let d = tempfile::tempdir().unwrap();
    write(&d.path().join(".needle.txt"), "");
    let (names, _) = filenames(
        req(SearchKind::Filename, vec![d.path().into()], "needle"),
        &Default::default(),
    );
    assert!(names.is_empty());
    let mut r = req(SearchKind::Filename, vec![d.path().into()], "needle");
    r.include_hidden_files = true;
    let (names, _) = filenames(r, &Default::default());
    assert_eq!(names, vec![".needle.txt"]);
}

#[test]
fn excluded_directory_overrides_are_request_scoped() {
    let d = tempfile::tempdir().unwrap();
    write(&d.path().join("target/needle.txt"), "");
    write(&d.path().join("keep/needle.txt"), "");
    let mut r = req(SearchKind::Filename, vec![d.path().into()], "needle");
    r.excluded_directory_names = vec!["target".into()];
    let (names, _) = filenames(r, &Default::default());
    assert_eq!(names, vec!["needle.txt"]);
}
