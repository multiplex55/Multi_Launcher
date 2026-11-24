//! Frontend-neutral construction of File Search requests.
use super::{
    model::{FileTypeFilter, SearchKind, SearchRequest, SearchScope},
    predicate::SizeMode,
    settings::{FileSearchSettings, FileSearchUiPreferences},
};
use std::{collections::HashSet, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootScope {
    Global,
    Directory,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestError {
    EmptySearchText,
    EmptyGlobalRoots,
    EmptyDirectoryRoots,
    InvalidMetadata(String),
}

impl RequestError {
    pub fn user_message(&self) -> &str {
        match self {
            Self::EmptySearchText => "Enter search text before searching.",
            Self::EmptyGlobalRoots => {
                "Configure at least one valid global search root in File Search settings."
            }
            Self::EmptyDirectoryRoots => "Choose at least one valid root directory first.",
            Self::InvalidMetadata(message) => message,
        }
    }
}

/// Borrowed criteria for request construction. A frontend can move its owned
/// snapshot to a worker, then borrow it there: root validation accesses the filesystem.
#[derive(Clone, Debug)]
pub struct RequestInput<'a> {
    pub text: &'a str,
    pub kind: SearchKind,
    pub scope: RootScope,
    pub custom_roots: Vec<PathBuf>,
    pub case_sensitive: bool,
    pub include_hidden: bool,
    pub settings: &'a FileSearchSettings,
    pub preferences: &'a FileSearchUiPreferences,
    pub excluded_directory_names: Vec<String>,
}

/// Resolve only existing directories, retaining first-seen canonical identity order.
/// This performs filesystem I/O and should not run on a UI thread.
pub fn resolve_valid_roots(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    dedup_search_paths(paths.into_iter().filter_map(|path| {
        let trimmed = path.to_string_lossy().trim().to_owned();
        if trimmed.is_empty() {
            return None;
        }
        PathBuf::from(trimmed)
            .canonicalize()
            .ok()
            .filter(|path| path.is_dir())
    }))
}

pub fn dedup_search_paths(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter(|path| seen.insert(super::sorting::path_identity(path)))
        .collect()
}

/// Build authored criteria without capturing the metadata submission clock.
pub fn build_semantic_request(input: &RequestInput<'_>) -> Result<SearchRequest, RequestError> {
    let text = input.text.trim();
    if text.is_empty() {
        return Err(RequestError::EmptySearchText);
    }
    let roots = match input.scope {
        RootScope::Global => {
            resolve_valid_roots(input.settings.global_search_roots.iter().cloned())
        }
        RootScope::Directory => resolve_valid_roots(input.custom_roots.iter().cloned()),
    };
    if roots.is_empty() {
        return Err(match input.scope {
            RootScope::Global => RequestError::EmptyGlobalRoots,
            RootScope::Directory => RequestError::EmptyDirectoryRoots,
        });
    }
    let preferences = input.preferences;
    Ok(SearchRequest {
        kind: input.kind,
        scope: SearchScope::Roots { roots },
        text: text.to_owned(),
        case_sensitive: input.case_sensitive,
        include_hidden_files: input.include_hidden,
        max_results: input.settings.max_search_results.max(1),
        max_file_size_bytes: input.settings.max_content_search_file_size_bytes.max(1),
        included_extensions: preferences.included_extensions.clone(),
        excluded_extensions: preferences.excluded_extensions.clone(),
        excluded_directory_names: input.excluded_directory_names.clone(),
        filename_match_mode: preferences.filename_match_mode,
        content_match_mode: preferences.content_match_mode,
        whole_word: preferences.whole_word,
        file_type_filter: if preferences.metadata.size.mode != SizeMode::Any
            || input.kind == SearchKind::Content
        {
            FileTypeFilter::FilesOnly
        } else {
            preferences.file_type_filter
        },
        metadata_preferences: preferences.metadata.clone(),
        metadata: Default::default(),
    })
}

/// Build a backend-ready request, capturing metadata at submission time. Pass
/// its result to `SearchCoordinator::start_search` on the coordinator owner.
pub fn build_request(input: &RequestInput<'_>) -> Result<SearchRequest, RequestError> {
    let mut request = build_semantic_request(input)?;
    request.metadata = request
        .metadata_preferences
        .capture(request.kind, request.max_file_size_bytes)
        .map_err(RequestError::InvalidMetadata)?;
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_search::{
        coordinator::SearchCoordinator,
        model::{FilenameMatchMode, SearchEvent, SearchResult},
        predicate::{DateMode, SizeMode},
    };
    use std::time::{Duration, Instant};

    fn input<'a>(
        root: PathBuf,
        settings: &'a FileSearchSettings,
        preferences: &'a FileSearchUiPreferences,
    ) -> RequestInput<'a> {
        RequestInput {
            text: "  needle  ",
            kind: SearchKind::Filename,
            scope: RootScope::Directory,
            custom_roots: vec![root],
            case_sensitive: true,
            include_hidden: true,
            settings,
            preferences,
            excluded_directory_names: vec!["skip".into()],
        }
    }

    #[test]
    fn request_normalizes_roots_and_preserves_form_rules() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let mut settings = FileSearchSettings::default();
        settings.max_search_results = 0;
        settings.max_content_search_file_size_bytes = 0;
        settings.global_search_roots.clear();
        let mut preferences = FileSearchUiPreferences::default();
        preferences.filename_match_mode = FilenameMatchMode::FilenameContains;
        preferences.file_type_filter = FileTypeFilter::DirectoriesOnly;
        preferences.metadata.size.mode = SizeMode::Empty;
        let mut form = input(root.clone(), &settings, &preferences);
        form.custom_roots
            .extend([root.clone(), root.join("missing")]);
        let request = build_request(&form).unwrap();
        assert_eq!(
            request.scope,
            SearchScope::Roots {
                roots: vec![root.canonicalize().unwrap()]
            }
        );
        assert_eq!(request.text, "needle");
        assert_eq!(request.max_results, 1);
        assert_eq!(request.max_file_size_bytes, 1);
        assert_eq!(request.file_type_filter, FileTypeFilter::FilesOnly);
        assert!(request.case_sensitive && request.include_hidden_files);
        assert_eq!(request.excluded_directory_names, vec!["skip"]);
        let mut invalid_preferences = preferences.clone();
        invalid_preferences.metadata.date.mode = DateMode::Custom;
        invalid_preferences.metadata.date.start = "invalid".into();
        form.preferences = &invalid_preferences;
        assert!(matches!(
            build_request(&form),
            Err(RequestError::InvalidMetadata(_))
        ));
        form.text = "";
        assert_eq!(build_request(&form), Err(RequestError::EmptySearchText));
        form.text = "needle";
        form.custom_roots.clear();
        assert_eq!(build_request(&form), Err(RequestError::EmptyDirectoryRoots));
        form.scope = RootScope::Global;
        assert_eq!(build_request(&form), Err(RequestError::EmptyGlobalRoots));
    }

    #[test]
    fn request_built_off_thread_runs_through_real_coordinator() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("needle.txt");
        std::fs::write(&target, "fixture").unwrap();
        let mut settings = FileSearchSettings::default();
        settings.max_search_results = 10;
        let mut preferences = FileSearchUiPreferences::default();
        preferences.filename_match_mode = FilenameMatchMode::FilenameContains;
        let worker_settings = settings.clone();
        let root = temp.path().to_path_buf();
        let request =
            std::thread::spawn(move || build_request(&input(root, &worker_settings, &preferences)))
                .join()
                .unwrap()
                .unwrap();
        let mut coordinator = SearchCoordinator::from_settings(settings);
        let id = coordinator.start_search(request);
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut found = false;
        let mut completed = false;
        while Instant::now() < deadline && !completed {
            for event in coordinator.drain_current_events() {
                match event {
                    SearchEvent::Result {
                        id: event_id,
                        result: SearchResult::Filename(result),
                    } if event_id == id => {
                        found |= result.path.file_name() == target.file_name();
                    }
                    SearchEvent::Completed { id: event_id } if event_id == id => completed = true,
                    SearchEvent::Failed { error, .. } => panic!("file search failed: {error}"),
                    _ => {}
                }
            }
            if !completed {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        assert!(
            completed && found,
            "coordinator should return the fixture and complete"
        );
    }
}
