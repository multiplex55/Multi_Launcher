use crate::file_search::model::{
    SearchBackend, SearchEvent, SearchId, SearchKind, SearchRequest, SearchResult, SearchScope,
    SearchStatus,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
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

#[derive(Clone)]
struct UnimplementedExecutor;

impl SearchExecutor for UnimplementedExecutor {
    fn execute(
        &self,
        id: SearchId,
        _request: SearchRequest,
        token: CancellationToken,
        events: mpsc::Sender<SearchEvent>,
    ) {
        if token.is_cancelled() {
            let _ = events.send(SearchEvent::Cancelled { id });
            return;
        }

        let _ = events.send(SearchEvent::Failed {
            id,
            error: "Search backend execution is not wired yet".to_string(),
        });
    }
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
}

impl Default for SearchCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchCoordinator {
    pub fn new() -> Self {
        Self::with_executor(Arc::new(UnimplementedExecutor))
    }

    pub fn with_executor(executor: Arc<dyn SearchExecutor>) -> Self {
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
        }
    }

    pub fn start_search(&mut self, request: SearchRequest) -> SearchId {
        self.cancel_active_token();

        let id = SearchId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        let backend = Self::select_backend(&request);
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
        match (&request.kind, &request.scope) {
            (SearchKind::Filename, SearchScope::Directory { .. }) => SearchBackend::WalkDir,
            (SearchKind::Filename, SearchScope::Global) => SearchBackend::Everything,
            (SearchKind::Filename, SearchScope::File { .. }) => SearchBackend::WalkDir,
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
                SearchScope::Directory { root: ".".into() },
                10,
            )),
            SearchBackend::WalkDir
        );
        assert_eq!(
            SearchCoordinator::select_backend(&request(
                SearchKind::Filename,
                SearchScope::Global,
                10
            )),
            SearchBackend::Everything
        );
        assert_eq!(
            SearchCoordinator::select_backend(&request(
                SearchKind::Content,
                SearchScope::Global,
                10
            )),
            SearchBackend::Ripgrep
        );
    }

    #[test]
    fn assigns_monotonic_search_ids() {
        let exec = FakeExecutor::new(vec![FakeMode::Empty, FakeMode::Empty]);
        let mut coordinator = SearchCoordinator::with_executor(exec);
        assert_eq!(
            coordinator.start_search(request(SearchKind::Filename, SearchScope::Global, 10)),
            SearchId(1)
        );
        assert_eq!(
            coordinator.start_search(request(SearchKind::Filename, SearchScope::Global, 10)),
            SearchId(2)
        );
    }

    #[test]
    fn starting_new_search_cancels_previous_token() {
        let exec = FakeExecutor::new(vec![FakeMode::Wait, FakeMode::Empty]);
        let mut coordinator = SearchCoordinator::with_executor(exec.clone());
        coordinator.start_search(request(SearchKind::Filename, SearchScope::Global, 10));
        thread::sleep(Duration::from_millis(10));
        coordinator.start_search(request(SearchKind::Filename, SearchScope::Global, 10));
        assert!(exec.token(0).is_cancelled());
    }

    #[test]
    fn stale_events_are_ignored_by_current_drain() {
        let exec = FakeExecutor::new(vec![FakeMode::Wait, FakeMode::Empty]);
        let mut coordinator = SearchCoordinator::with_executor(exec);
        let old = coordinator.start_search(request(SearchKind::Filename, SearchScope::Global, 10));
        let new = coordinator.start_search(request(SearchKind::Filename, SearchScope::Global, 10));
        let events = drain_after(&mut coordinator);
        assert!(events.iter().all(|event| event_id(event) == new));
        assert!(events.iter().all(|event| event_id(event) != old));
        assert!(coordinator.diagnostics().stale_events_ignored > 0);
    }

    #[test]
    fn successful_completion_updates_status() {
        let exec = FakeExecutor::new(vec![FakeMode::Success(1)]);
        let mut coordinator = SearchCoordinator::with_executor(exec);
        let id = coordinator.start_search(request(SearchKind::Filename, SearchScope::Global, 10));
        let events = drain_after(&mut coordinator);
        assert!(events.contains(&SearchEvent::Completed { id }));
        assert_eq!(coordinator.active_status(), SearchStatus::Completed);
    }

    #[test]
    fn empty_completion_updates_status() {
        let exec = FakeExecutor::new(vec![FakeMode::Empty]);
        let mut coordinator = SearchCoordinator::with_executor(exec);
        let id = coordinator.start_search(request(SearchKind::Filename, SearchScope::Global, 10));
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
        coordinator.start_search(request(SearchKind::Filename, SearchScope::Global, 10));
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
        let id = coordinator.start_search(request(SearchKind::Filename, SearchScope::Global, 10));
        coordinator.cancel_active();
        let events = drain_after(&mut coordinator);
        assert!(events.contains(&SearchEvent::Cancelled { id }));
        assert_eq!(coordinator.active_status(), SearchStatus::Cancelled);
    }

    #[test]
    fn result_limit_is_enforced() {
        let exec = FakeExecutor::new(vec![FakeMode::Success(5)]);
        let mut coordinator = SearchCoordinator::with_executor(exec);
        coordinator.start_search(request(SearchKind::Filename, SearchScope::Global, 2));
        let events = drain_after(&mut coordinator);
        let results = events
            .iter()
            .filter(|event| matches!(event, SearchEvent::Result { .. }))
            .count();
        assert_eq!(results, 2);
    }
}
