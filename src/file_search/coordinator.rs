use crate::file_search::executor::FileSearchExecutor;
use crate::file_search::model::{
    SearchBackend, SearchEvent, SearchId, SearchKind, SearchRequest, SearchResult, SearchScope,
    SearchStatus,
};
use crate::file_search::settings::FileSearchSettings;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;

#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchDiagnostics {
    pub started: u64,
    pub completed: u64,
    pub cancelled: u64,
    pub failed: u64,
    pub stale_events_ignored: u64,
    pub last_error: Option<String>,
}

/// Backend worker contract used by the coordinator. In-process implementations
/// should poll `CancellationToken::is_cancelled` cooperatively, while future
/// external-tool implementations must terminate their child process when the
/// token becomes cancelled.
pub trait SearchExecutor: Send + Sync + 'static {
    fn execute(
        &self,
        id: SearchId,
        request: SearchRequest,
        token: CancellationToken,
        events: mpsc::Sender<SearchEvent>,
    );
}

pub struct SearchCoordinator {
    next_id: u64,
    active_search_id: Option<SearchId>,
    active_token: Option<CancellationToken>,
    event_sender: mpsc::Sender<SearchEvent>,
    event_receiver: mpsc::Receiver<SearchEvent>,
    active_status: SearchStatus,
    last_backend: Option<SearchBackend>,
    diagnostics: SearchDiagnostics,
    executor: Arc<dyn SearchExecutor>,
    production_settings: Option<FileSearchSettings>,
}

impl Default for SearchCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchCoordinator {
    pub fn new() -> Self {
        Self::from_settings(FileSearchSettings::default())
    }

    pub fn from_settings(settings: FileSearchSettings) -> Self {
        let executor = Arc::new(FileSearchExecutor::new(settings.clone()));
        Self::with_executor_and_settings(executor, Some(settings))
    }

    pub fn with_settings(settings: FileSearchSettings) -> Self {
        Self::from_settings(settings)
    }

    pub fn with_executor(executor: Arc<dyn SearchExecutor>) -> Self {
        Self::with_executor_and_settings(executor, None)
    }

    fn with_executor_and_settings(
        executor: Arc<dyn SearchExecutor>,
        production_settings: Option<FileSearchSettings>,
    ) -> Self {
        let (event_sender, event_receiver) = mpsc::channel();
        Self {
            next_id: 1,
            active_search_id: None,
            active_token: None,
            event_sender,
            event_receiver,
            active_status: SearchStatus::Pending,
            last_backend: None,
            diagnostics: SearchDiagnostics::default(),
            executor,
            production_settings,
        }
    }

    pub fn reconfigure_from_settings(&mut self, settings: FileSearchSettings) {
        self.cancel_active();
        self.executor = Arc::new(FileSearchExecutor::new(settings.clone()));
        self.production_settings = Some(settings);
    }

    pub fn production_settings(&self) -> Option<&FileSearchSettings> {
        self.production_settings.as_ref()
    }

    pub fn start_search(&mut self, request: SearchRequest) -> SearchId {
        self.cancel_active_token();

        let id = SearchId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        let backend =
            Self::select_backend_with_settings(&request, self.production_settings.as_ref());
        let token = CancellationToken::new();

        self.active_search_id = Some(id);
        self.active_token = Some(token.clone());
        self.active_status = SearchStatus::Running;
        self.last_backend = Some(backend);
        self.diagnostics.started += 1;

        let events = self.event_sender.clone();
        let executor = Arc::clone(&self.executor);
        let worker_request = request;
        let worker_token = token;
        thread::spawn(move || {
            if events.send(SearchEvent::Started { id, backend }).is_err() {
                return;
            }
            executor.execute(id, worker_request, worker_token, events);
        });

        id
    }

    pub fn cancel_active(&mut self) {
        self.cancel_active_token();
        if let Some(id) = self.active_search_id {
            let _ = self.event_sender.send(SearchEvent::Cancelled { id });
        }
    }

    pub fn drain_current_events(&mut self) -> Vec<SearchEvent> {
        self.drain_events(false)
    }

    pub fn drain_events_including_stale(&mut self) -> Vec<SearchEvent> {
        self.drain_events(true)
    }

    pub fn active_search_id(&self) -> Option<SearchId> {
        self.active_search_id
    }

    pub fn active_status(&self) -> SearchStatus {
        self.active_status
    }

    pub fn last_backend(&self) -> Option<SearchBackend> {
        self.last_backend
    }

    pub fn diagnostics(&self) -> &SearchDiagnostics {
        &self.diagnostics
    }

    pub fn select_backend(request: &SearchRequest) -> SearchBackend {
        Self::select_backend_with_settings(request, None)
    }

    pub fn select_backend_with_settings(
        request: &SearchRequest,
        settings: Option<&FileSearchSettings>,
    ) -> SearchBackend {
        match (&request.kind, &request.scope) {
            (SearchKind::Filename, SearchScope::Roots { roots })
                if settings.is_some_and(|settings| {
                    !settings.everything_enabled && roots_match_global_search_roots(roots, settings)
                }) =>
            {
                SearchBackend::Ripgrep
            }
            (SearchKind::Filename, SearchScope::Roots { roots })
                if roots.len() == 1
                    && !settings.is_some_and(|settings| {
                        roots_match_global_search_roots(roots, settings)
                    }) =>
            {
                SearchBackend::WalkDir
            }
            (SearchKind::Filename, SearchScope::Roots { .. }) => SearchBackend::Everything,
            (SearchKind::Filename, SearchScope::Files { .. }) => SearchBackend::WalkDir,
            (SearchKind::Content, _) => SearchBackend::Ripgrep,
        }
    }

    fn cancel_active_token(&mut self) {
        if let Some(token) = &self.active_token {
            token.cancel();
        }
    }

    fn drain_events(&mut self, include_stale: bool) -> Vec<SearchEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.event_receiver.try_recv() {
            let is_current = Some(event_id(&event)) == self.active_search_id;
            if include_stale || is_current {
                self.apply_event(&event);
                events.push(event);
            } else {
                self.diagnostics.stale_events_ignored += 1;
            }
        }
        events
    }

    fn apply_event(&mut self, event: &SearchEvent) {
        match event {
            SearchEvent::Started { backend, .. } => {
                self.active_status = SearchStatus::Running;
                self.last_backend = Some(*backend);
            }
            SearchEvent::Result { .. } | SearchEvent::Progress { .. } => {}
            SearchEvent::Completed { .. } => {
                self.active_status = SearchStatus::Completed;
                self.diagnostics.completed += 1;
                self.active_token = None;
            }
            SearchEvent::Cancelled { .. } => {
                self.active_status = SearchStatus::Cancelled;
                self.diagnostics.cancelled += 1;
                self.active_token = None;
            }
            SearchEvent::Failed { error, .. } => {
                self.active_status = SearchStatus::Failed;
                self.diagnostics.failed += 1;
                self.diagnostics.last_error = Some(error.clone());
                self.active_token = None;
            }
        }
    }
}

fn roots_match_global_search_roots(
    roots: &[std::path::PathBuf],
    settings: &FileSearchSettings,
) -> bool {
    if roots.is_empty() {
        return true;
    }
    if roots.len() != settings.global_search_roots.len() {
        return false;
    }
    let mut request_roots: Vec<_> = roots
        .iter()
        .map(|path| crate::file_search::model::normalize_path_for_identity(path))
        .collect();
    let mut global_roots: Vec<_> = settings
        .global_search_roots
        .iter()
        .map(|path| crate::file_search::model::normalize_path_for_identity(path))
        .collect();
    request_roots.sort();
    global_roots.sort();
    request_roots == global_roots
}

pub fn event_id(event: &SearchEvent) -> SearchId {
    match event {
        SearchEvent::Started { id, .. }
        | SearchEvent::Result { id, .. }
        | SearchEvent::Progress { id, .. }
        | SearchEvent::Completed { id }
        | SearchEvent::Cancelled { id }
        | SearchEvent::Failed { id, .. } => *id,
    }
}

pub fn send_result_limited(
    events: &mpsc::Sender<SearchEvent>,
    id: SearchId,
    result: SearchResult,
    emitted: &mut usize,
    max_results: usize,
) -> bool {
    if *emitted >= max_results {
        return false;
    }
    *emitted += 1;
    events.send(SearchEvent::Result { id, result }).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_search::model::{FileKind, FilenameRank, FilenameResult};
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::Duration;

    #[derive(Clone)]
    enum FakeMode {
        Success(usize),
        Empty,
        Failure,
        Wait,
    }

    struct FakeExecutor {
        modes: Mutex<Vec<FakeMode>>,
        tokens: Mutex<Vec<CancellationToken>>,
    }

    impl FakeExecutor {
        fn new(modes: Vec<FakeMode>) -> Arc<Self> {
            Arc::new(Self {
                modes: Mutex::new(modes),
                tokens: Mutex::new(Vec::new()),
            })
        }

        fn token(&self, index: usize) -> CancellationToken {
            self.tokens.lock().unwrap()[index].clone()
        }
    }

    impl SearchExecutor for FakeExecutor {
        fn execute(
            &self,
            id: SearchId,
            request: SearchRequest,
            token: CancellationToken,
            events: mpsc::Sender<SearchEvent>,
        ) {
            self.tokens.lock().unwrap().push(token.clone());
            let mode = self.modes.lock().unwrap().remove(0);
            match mode {
                FakeMode::Success(count) => {
                    let mut emitted = 0;
                    for i in 0..count {
                        if token.is_cancelled() {
                            let _ = events.send(SearchEvent::Cancelled { id });
                            return;
                        }
                        let result = SearchResult::Filename(FilenameResult {
                            path: PathBuf::from(format!("result-{i}")),
                            file_name: format!("result-{i}"),
                            parent_directory: None,
                            kind: FileKind::File,
                            size: None,
                            modified: None,
                            rank: FilenameRank::ExactFilename,
                            match_quality: FilenameRank::ExactFilename,
                            filename_match_ranges: Vec::new(),
                            path_match_ranges: Vec::new(),
                            arrival_index: i,
                        });
                        if !send_result_limited(
                            &events,
                            id,
                            result,
                            &mut emitted,
                            request.max_results,
                        ) {
                            break;
                        }
                    }
                    let _ = events.send(SearchEvent::Completed { id });
                }
                FakeMode::Empty => {
                    let _ = events.send(SearchEvent::Completed { id });
                }
                FakeMode::Failure => {
                    let _ = events.send(SearchEvent::Failed {
                        id,
                        error: "fake failure".to_string(),
                    });
                }
                FakeMode::Wait => {
                    for _ in 0..100 {
                        if token.is_cancelled() {
                            let _ = events.send(SearchEvent::Cancelled { id });
                            return;
                        }
                        thread::sleep(Duration::from_millis(2));
                    }
                }
            }
        }
    }

    fn request(kind: SearchKind, scope: SearchScope, max_results: usize) -> SearchRequest {
        SearchRequest {
            kind,
            scope,
            text: "needle".to_string(),
            case_sensitive: false,
            include_hidden_files: false,
            max_results,
            max_file_size_bytes: 1024,
            included_extensions: Vec::new(),
            excluded_extensions: Vec::new(),
            excluded_directory_names: Vec::new(),
            filename_match_mode: crate::file_search::model::FilenameMatchMode::RankedSubstring,
            content_match_mode: crate::file_search::model::ContentMatchMode::ExactPhrase,
            whole_word: false,
            file_type_filter: crate::file_search::model::FileTypeFilter::FilesAndDirectories,
        }
    }

    fn drain_after(coordinator: &mut SearchCoordinator) -> Vec<SearchEvent> {
        thread::sleep(Duration::from_millis(20));
        coordinator.drain_current_events()
    }

    #[test]
    fn selects_backend_from_request_shape() {
        assert_eq!(
            SearchCoordinator::select_backend(&request(
                SearchKind::Filename,
                SearchScope::Roots {
                    roots: vec![".".into()]
                },
                10,
            )),
            SearchBackend::WalkDir
        );
        assert_eq!(
            SearchCoordinator::select_backend(&request(
                SearchKind::Filename,
                SearchScope::Roots { roots: Vec::new() },
                10
            )),
            SearchBackend::Everything
        );
        assert_eq!(
            SearchCoordinator::select_backend(&request(
                SearchKind::Filename,
                SearchScope::Files {
                    files: vec!["file.txt".into()]
                },
                10
            )),
            SearchBackend::WalkDir
        );
        assert_eq!(
            SearchCoordinator::select_backend(&request(
                SearchKind::Content,
                SearchScope::Roots { roots: Vec::new() },
                10
            )),
            SearchBackend::Ripgrep
        );
    }

    #[test]
    fn everything_disabled_selects_ripgrep_for_global_filename_search() {
        let settings = FileSearchSettings {
            everything_enabled: false,
            ..FileSearchSettings::default()
        };

        assert_eq!(
            SearchCoordinator::select_backend_with_settings(
                &request(
                    SearchKind::Filename,
                    SearchScope::Roots { roots: Vec::new() },
                    10
                ),
                Some(&settings),
            ),
            SearchBackend::Ripgrep
        );
    }

    #[test]
    fn assigns_monotonic_search_ids() {
        let exec = FakeExecutor::new(vec![FakeMode::Empty, FakeMode::Empty]);
        let mut coordinator = SearchCoordinator::with_executor(exec);
        assert_eq!(
            coordinator.start_search(request(
                SearchKind::Filename,
                SearchScope::Roots { roots: Vec::new() },
                10
            )),
            SearchId(1)
        );
        assert_eq!(
            coordinator.start_search(request(
                SearchKind::Filename,
                SearchScope::Roots { roots: Vec::new() },
                10
            )),
            SearchId(2)
        );
    }

    #[test]
    fn starting_new_search_cancels_previous_token() {
        let exec = FakeExecutor::new(vec![FakeMode::Wait, FakeMode::Empty]);
        let mut coordinator = SearchCoordinator::with_executor(exec.clone());
        coordinator.start_search(request(
            SearchKind::Filename,
            SearchScope::Roots { roots: Vec::new() },
            10,
        ));
        thread::sleep(Duration::from_millis(10));
        coordinator.start_search(request(
            SearchKind::Filename,
            SearchScope::Roots { roots: Vec::new() },
            10,
        ));
        assert!(exec.token(0).is_cancelled());
    }

    #[test]
    fn stale_events_are_ignored_by_current_drain() {
        let exec = FakeExecutor::new(vec![FakeMode::Wait, FakeMode::Empty]);
        let mut coordinator = SearchCoordinator::with_executor(exec);
        let old = coordinator.start_search(request(
            SearchKind::Filename,
            SearchScope::Roots { roots: Vec::new() },
            10,
        ));
        let new = coordinator.start_search(request(
            SearchKind::Filename,
            SearchScope::Roots { roots: Vec::new() },
            10,
        ));
        let events = drain_after(&mut coordinator);
        assert!(events.iter().all(|event| event_id(event) == new));
        assert!(events.iter().all(|event| event_id(event) != old));
        assert!(coordinator.diagnostics().stale_events_ignored > 0);
    }

    #[test]
    fn successful_completion_updates_status() {
        let exec = FakeExecutor::new(vec![FakeMode::Success(1)]);
        let mut coordinator = SearchCoordinator::with_executor(exec);
        let id = coordinator.start_search(request(
            SearchKind::Filename,
            SearchScope::Roots { roots: Vec::new() },
            10,
        ));
        let events = drain_after(&mut coordinator);
        assert!(events.contains(&SearchEvent::Completed { id }));
        assert_eq!(coordinator.active_status(), SearchStatus::Completed);
    }

    #[test]
    fn empty_completion_updates_status() {
        let exec = FakeExecutor::new(vec![FakeMode::Empty]);
        let mut coordinator = SearchCoordinator::with_executor(exec);
        let id = coordinator.start_search(request(
            SearchKind::Filename,
            SearchScope::Roots { roots: Vec::new() },
            10,
        ));
        let events = drain_after(&mut coordinator);
        assert_eq!(
            events,
            vec![
                SearchEvent::Started {
                    id,
                    backend: SearchBackend::Everything
                },
                SearchEvent::Completed { id }
            ]
        );
    }

    #[test]
    fn failure_updates_status_and_diagnostics() {
        let exec = FakeExecutor::new(vec![FakeMode::Failure]);
        let mut coordinator = SearchCoordinator::with_executor(exec);
        coordinator.start_search(request(
            SearchKind::Filename,
            SearchScope::Roots { roots: Vec::new() },
            10,
        ));
        drain_after(&mut coordinator);
        assert_eq!(coordinator.active_status(), SearchStatus::Failed);
        assert_eq!(
            coordinator.diagnostics().last_error.as_deref(),
            Some("fake failure")
        );
    }

    #[test]
    fn explicit_cancellation_sets_status_without_blocking() {
        let exec = FakeExecutor::new(vec![FakeMode::Wait]);
        let mut coordinator = SearchCoordinator::with_executor(exec);
        let id = coordinator.start_search(request(
            SearchKind::Filename,
            SearchScope::Roots { roots: Vec::new() },
            10,
        ));
        coordinator.cancel_active();
        let events = drain_after(&mut coordinator);
        assert!(events.contains(&SearchEvent::Cancelled { id }));
        assert_eq!(coordinator.active_status(), SearchStatus::Cancelled);
    }

    #[test]
    fn result_limit_is_enforced() {
        let exec = FakeExecutor::new(vec![FakeMode::Success(5)]);
        let mut coordinator = SearchCoordinator::with_executor(exec);
        coordinator.start_search(request(
            SearchKind::Filename,
            SearchScope::Roots { roots: Vec::new() },
            2,
        ));
        let events = drain_after(&mut coordinator);
        let results = events
            .iter()
            .filter(|event| matches!(event, SearchEvent::Result { .. }))
            .count();
        assert_eq!(results, 2);
    }
    fn drain_until_terminal(coordinator: &mut SearchCoordinator) -> Vec<SearchEvent> {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut all_events = Vec::new();
        loop {
            let events = coordinator.drain_current_events();
            let terminal = events.iter().any(|event| {
                matches!(
                    event,
                    SearchEvent::Completed { .. }
                        | SearchEvent::Cancelled { .. }
                        | SearchEvent::Failed { .. }
                )
            });
            all_events.extend(events);
            if terminal || std::time::Instant::now() >= deadline {
                return all_events;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn assert_no_unwired_placeholder(events: &[SearchEvent]) {
        assert!(
            !events.iter().any(|event| matches!(
                event,
                SearchEvent::Failed { error, .. }
                    if error.contains("Search backend execution is not wired yet")
                        || error.contains("not wired yet")
            )),
            "events: {events:?}"
        );
    }

    #[test]
    fn from_settings_installs_production_dispatcher() {
        let temp = tempfile::tempdir().expect("tempdir");
        let expected = "configured-dispatcher-filename.txt";
        std::fs::write(temp.path().join(expected), "contents").expect("write file");
        let settings = FileSearchSettings {
            max_search_results: 3,
            ..FileSearchSettings::default()
        };
        let mut coordinator = SearchCoordinator::from_settings(settings.clone());

        assert_eq!(coordinator.production_settings(), Some(&settings));
        coordinator.start_search(SearchRequest {
            kind: SearchKind::Filename,
            scope: SearchScope::Roots {
                roots: vec![temp.path().to_path_buf()],
            },
            text: expected.to_owned(),
            case_sensitive: settings.case_sensitive,
            include_hidden_files: settings.include_hidden_files,
            max_results: settings.max_search_results,
            max_file_size_bytes: settings.max_content_search_file_size_bytes,
            included_extensions: Vec::new(),
            excluded_extensions: Vec::new(),
            excluded_directory_names: settings.excluded_directory_names.clone(),
            filename_match_mode: crate::file_search::model::FilenameMatchMode::RankedSubstring,
            content_match_mode: crate::file_search::model::ContentMatchMode::ExactPhrase,
            whole_word: false,
            file_type_filter: crate::file_search::model::FileTypeFilter::FilesAndDirectories,
        });

        let events = drain_until_terminal(&mut coordinator);
        assert!(events.iter().any(|event| matches!(
            event,
            SearchEvent::Result {
                result: SearchResult::Filename(result),
                ..
            } if result.file_name == expected
        )));
        assert_no_unwired_placeholder(&events);
    }

    #[test]
    fn production_directory_filename_search_uses_walkdir_and_returns_result() {
        let temp = tempfile::tempdir().expect("tempdir");
        let expected = "known-production-filename.txt";
        std::fs::write(temp.path().join(expected), "contents").expect("write file");
        let mut coordinator = SearchCoordinator::new();

        coordinator.start_search(SearchRequest {
            kind: SearchKind::Filename,
            scope: SearchScope::Roots {
                roots: vec![temp.path().to_path_buf()],
            },
            text: expected.to_owned(),
            case_sensitive: false,
            include_hidden_files: false,
            max_results: 10,
            max_file_size_bytes: 1024,
            included_extensions: Vec::new(),
            excluded_extensions: Vec::new(),
            excluded_directory_names: Vec::new(),
            filename_match_mode: crate::file_search::model::FilenameMatchMode::RankedSubstring,
            content_match_mode: crate::file_search::model::ContentMatchMode::ExactPhrase,
            whole_word: false,
            file_type_filter: crate::file_search::model::FileTypeFilter::FilesAndDirectories,
        });

        let events = drain_until_terminal(&mut coordinator);
        assert_eq!(coordinator.last_backend(), Some(SearchBackend::WalkDir));
        assert!(events.iter().any(|event| matches!(
            event,
            SearchEvent::Result {
                result: SearchResult::Filename(result),
                ..
            } if result.file_name == expected
        )));
        assert!(events
            .iter()
            .any(|event| matches!(event, SearchEvent::Completed { .. })));
        assert_no_unwired_placeholder(&events);
    }

    #[test]
    fn production_content_search_uses_ripgrep_and_returns_result_when_available() {
        let Ok(ripgrep) =
            crate::file_search::ripgrep::resolve_ripgrep_executable(std::path::Path::new("rg"))
        else {
            return;
        };
        let temp = tempfile::tempdir().expect("tempdir");
        let expected = temp.path().join("content-hit.txt");
        std::fs::write(&expected, "alpha needle omega\n").expect("write file");
        std::fs::write(temp.path().join("miss.txt"), "alpha omega\n").expect("write miss file");
        let settings = FileSearchSettings {
            ripgrep_executable_path: ripgrep,
            max_search_results: 10,
            ..FileSearchSettings::default()
        };
        let mut coordinator = SearchCoordinator::from_settings(settings.clone());

        coordinator.start_search(SearchRequest {
            kind: SearchKind::Content,
            scope: SearchScope::Roots {
                roots: vec![temp.path().to_path_buf()],
            },
            text: "needle".to_owned(),
            case_sensitive: settings.case_sensitive,
            include_hidden_files: settings.include_hidden_files,
            max_results: settings.max_search_results,
            max_file_size_bytes: settings.max_content_search_file_size_bytes,
            included_extensions: Vec::new(),
            excluded_extensions: Vec::new(),
            excluded_directory_names: settings.excluded_directory_names.clone(),
            filename_match_mode: crate::file_search::model::FilenameMatchMode::RankedSubstring,
            content_match_mode: crate::file_search::model::ContentMatchMode::ExactPhrase,
            whole_word: false,
            file_type_filter: crate::file_search::model::FileTypeFilter::FilesAndDirectories,
        });

        let events = drain_until_terminal(&mut coordinator);
        assert_eq!(coordinator.last_backend(), Some(SearchBackend::Ripgrep));
        assert!(
            events.iter().any(|event| matches!(
                event,
                SearchEvent::Result {
                    result: SearchResult::ContentFile(result),
                    ..
                } if result.path == expected
                    && result.matches.iter().any(|m| m.line.contains("needle"))
            )),
            "events: {events:?}"
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, SearchEvent::Completed { .. })),
            "events: {events:?}"
        );
        assert_no_unwired_placeholder(&events);
    }

    #[test]
    fn production_content_search_with_missing_ripgrep_falls_back_to_detected_ripgrep() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp.path().join("haystack.txt"), "needle").expect("write file");
        let missing_rg = temp.path().join("missing").join("rg");
        assert!(missing_rg.is_absolute());
        let settings = FileSearchSettings {
            ripgrep_executable_path: missing_rg.clone(),
            ..FileSearchSettings::default()
        };
        let mut coordinator = SearchCoordinator::with_settings(settings);

        coordinator.start_search(SearchRequest {
            kind: SearchKind::Content,
            scope: SearchScope::Roots {
                roots: vec![temp.path().to_path_buf()],
            },
            text: "needle".to_owned(),
            case_sensitive: false,
            include_hidden_files: false,
            max_results: 10,
            max_file_size_bytes: 1024,
            included_extensions: Vec::new(),
            excluded_extensions: Vec::new(),
            excluded_directory_names: Vec::new(),
            filename_match_mode: crate::file_search::model::FilenameMatchMode::RankedSubstring,
            content_match_mode: crate::file_search::model::ContentMatchMode::ExactPhrase,
            whole_word: false,
            file_type_filter: crate::file_search::model::FileTypeFilter::FilesAndDirectories,
        });

        let events = drain_until_terminal(&mut coordinator);
        assert_eq!(coordinator.last_backend(), Some(SearchBackend::Ripgrep));
        if crate::file_search::ripgrep::resolve_ripgrep_executable(&missing_rg).is_err() {
            let error = events
                .iter()
                .find_map(|event| match event {
                    SearchEvent::Failed { error, .. } => Some(error.as_str()),
                    _ => None,
                })
                .expect("failed event");
            assert!(error.contains("ripgrep"), "{error}");
        } else {
            assert!(
                events.iter().any(|event| matches!(
                    event,
                    SearchEvent::Result {
                        result: SearchResult::ContentFile(result),
                        ..
                    } if result.path == PathBuf::from("haystack.txt")
                )),
                "events: {events:?}"
            );
            assert!(
                events
                    .iter()
                    .any(|event| matches!(event, SearchEvent::Completed { .. })),
                "events: {events:?}"
            );
        }
        assert_no_unwired_placeholder(&events);
    }

    #[test]
    fn everything_disabled_global_filename_search_returns_ripgrep_results_when_available() {
        let Ok(ripgrep) =
            crate::file_search::ripgrep::resolve_ripgrep_executable(std::path::Path::new("rg"))
        else {
            return;
        };
        let temp = tempfile::tempdir().expect("tempdir");
        let expected = "global-ripgrep-hit.txt";
        std::fs::write(temp.path().join(expected), "contents").expect("write file");
        let settings = FileSearchSettings {
            global_search_roots: vec![temp.path().to_path_buf()],
            everything_enabled: false,
            ripgrep_executable_path: ripgrep,
            ..FileSearchSettings::default()
        };
        let mut coordinator = SearchCoordinator::with_settings(settings);

        coordinator.start_search(SearchRequest {
            kind: SearchKind::Filename,
            scope: SearchScope::Roots {
                roots: vec![temp.path().to_path_buf()],
            },
            text: "global-ripgrep-hit".to_owned(),
            case_sensitive: false,
            include_hidden_files: false,
            max_results: 10,
            max_file_size_bytes: 1024,
            included_extensions: Vec::new(),
            excluded_extensions: Vec::new(),
            excluded_directory_names: Vec::new(),
            filename_match_mode: crate::file_search::model::FilenameMatchMode::RankedSubstring,
            content_match_mode: crate::file_search::model::ContentMatchMode::ExactPhrase,
            whole_word: false,
            file_type_filter: crate::file_search::model::FileTypeFilter::FilesAndDirectories,
        });

        let events = drain_until_terminal(&mut coordinator);
        assert_eq!(coordinator.last_backend(), Some(SearchBackend::Ripgrep));
        assert!(
            events.iter().any(|event| matches!(
                event,
                SearchEvent::Result {
                    result: SearchResult::Filename(result),
                    ..
                } if result.file_name == expected
            )),
            "events: {events:?}"
        );
        assert_no_unwired_placeholder(&events);
    }

    #[test]
    fn production_global_filename_search_with_everything_disabled_uses_detected_ripgrep_fallback() {
        let temp = tempfile::tempdir().expect("tempdir");
        let missing_rg = temp.path().join("missing").join("rg");
        let settings = FileSearchSettings {
            global_search_roots: vec![temp.path().to_path_buf()],
            everything_enabled: false,
            ripgrep_executable_path: missing_rg.clone(),
            ..FileSearchSettings::default()
        };
        let mut coordinator = SearchCoordinator::with_settings(settings);

        coordinator.start_search(SearchRequest {
            kind: SearchKind::Filename,
            scope: SearchScope::Roots { roots: Vec::new() },
            text: "needle".to_owned(),
            case_sensitive: false,
            include_hidden_files: false,
            max_results: 10,
            max_file_size_bytes: 1024,
            included_extensions: Vec::new(),
            excluded_extensions: Vec::new(),
            excluded_directory_names: Vec::new(),
            filename_match_mode: crate::file_search::model::FilenameMatchMode::RankedSubstring,
            content_match_mode: crate::file_search::model::ContentMatchMode::ExactPhrase,
            whole_word: false,
            file_type_filter: crate::file_search::model::FileTypeFilter::FilesAndDirectories,
        });

        let events = drain_until_terminal(&mut coordinator);
        assert_eq!(coordinator.last_backend(), Some(SearchBackend::Ripgrep));
        if crate::file_search::ripgrep::resolve_ripgrep_executable(&missing_rg).is_err() {
            let error = events
                .iter()
                .find_map(|event| match event {
                    SearchEvent::Failed { error, .. } => Some(error.as_str()),
                    _ => None,
                })
                .expect("failed event");
            assert!(error.contains("ripgrep"), "{error}");
            assert!(!error.contains("Everything filename search"), "{error}");
        } else {
            assert!(
                events
                    .iter()
                    .any(|event| matches!(event, SearchEvent::Completed { .. })),
                "events: {events:?}"
            );
        }
        assert_no_unwired_placeholder(&events);
    }
}
