use crate::file_search::coordinator::{CancellationToken, SearchCoordinator, SearchExecutor};
use crate::file_search::everything::EverythingSearchExecutor;
use crate::file_search::model::{SearchBackend, SearchEvent, SearchId, SearchRequest};
use crate::file_search::ripgrep::RipgrepSearchExecutor;
use crate::file_search::settings::FileSearchSettings;
use crate::file_search::walkdir::WalkDirSearchExecutor;
use std::sync::{Arc, mpsc};

/// Production file-search dispatcher that selects the configured backend for a
/// request and delegates execution to the corresponding backend executor.
#[derive(Clone)]
pub struct FileSearchExecutor {
    settings: FileSearchSettings,
    ripgrep: Arc<dyn SearchExecutor>,
    walkdir: Arc<dyn SearchExecutor>,
    everything: Arc<dyn SearchExecutor>,
}

impl FileSearchExecutor {
    pub fn new(settings: FileSearchSettings) -> Self {
        Self::with_backend_executors(
            settings.clone(),
            Arc::new(RipgrepSearchExecutor::new(settings.clone())),
            Arc::new(WalkDirSearchExecutor::new(settings.clone())),
            Arc::new(EverythingSearchExecutor::new(settings.clone())),
        )
    }

    pub fn settings(&self) -> &FileSearchSettings {
        &self.settings
    }

    pub fn with_backend_executors(
        settings: FileSearchSettings,
        ripgrep: Arc<dyn SearchExecutor>,
        walkdir: Arc<dyn SearchExecutor>,
        everything: Arc<dyn SearchExecutor>,
    ) -> Self {
        Self {
            settings,
            ripgrep,
            walkdir,
            everything,
        }
    }
}

impl SearchExecutor for FileSearchExecutor {
    fn execute(
        &self,
        id: SearchId,
        request: SearchRequest,
        token: CancellationToken,
        events: mpsc::Sender<SearchEvent>,
    ) {
        match SearchCoordinator::select_backend_with_settings(&request, Some(&self.settings)) {
            SearchBackend::Ripgrep => self.ripgrep.execute(id, request, token, events),
            SearchBackend::WalkDir => self.walkdir.execute(id, request, token, events),
            SearchBackend::Everything => self.everything.execute(id, request, token, events),
            SearchBackend::Native => {
                let _ = events.send(SearchEvent::Failed {
                    id,
                    error: "Native file search backend is not implemented".to_owned(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_search::model::{SearchKind, SearchScope};
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingExecutor {
        calls: Mutex<Vec<SearchId>>,
    }

    impl RecordingExecutor {
        fn calls(&self) -> Vec<SearchId> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl SearchExecutor for RecordingExecutor {
        fn execute(
            &self,
            id: SearchId,
            _request: SearchRequest,
            _token: CancellationToken,
            events: mpsc::Sender<SearchEvent>,
        ) {
            self.calls.lock().unwrap().push(id);
            let _ = events.send(SearchEvent::Completed { id });
        }
    }

    fn request(kind: SearchKind, scope: SearchScope) -> SearchRequest {
        SearchRequest {
            kind,
            scope,
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
        }
    }

    fn dispatcher(
        ripgrep: Arc<RecordingExecutor>,
        walkdir: Arc<RecordingExecutor>,
        everything: Arc<RecordingExecutor>,
    ) -> FileSearchExecutor {
        dispatcher_with_settings(FileSearchSettings::default(), ripgrep, walkdir, everything)
    }

    fn dispatcher_with_settings(
        settings: FileSearchSettings,
        ripgrep: Arc<RecordingExecutor>,
        walkdir: Arc<RecordingExecutor>,
        everything: Arc<RecordingExecutor>,
    ) -> FileSearchExecutor {
        FileSearchExecutor::with_backend_executors(settings, ripgrep, walkdir, everything)
    }

    #[test]
    fn content_request_routes_to_ripgrep() {
        let ripgrep = Arc::new(RecordingExecutor::default());
        let walkdir = Arc::new(RecordingExecutor::default());
        let everything = Arc::new(RecordingExecutor::default());
        let executor = dispatcher(ripgrep.clone(), walkdir.clone(), everything.clone());

        let (tx, _rx) = mpsc::channel();
        executor.execute(
            SearchId(7),
            request(
                SearchKind::Content,
                SearchScope::Roots {
                    roots: vec![".".into()],
                },
            ),
            CancellationToken::new(),
            tx,
        );

        assert_eq!(ripgrep.calls(), vec![SearchId(7)]);
        assert!(walkdir.calls().is_empty());
        assert!(everything.calls().is_empty());
    }

    #[test]
    fn directory_filename_request_routes_to_walkdir() {
        let ripgrep = Arc::new(RecordingExecutor::default());
        let walkdir = Arc::new(RecordingExecutor::default());
        let everything = Arc::new(RecordingExecutor::default());
        let executor = dispatcher(ripgrep.clone(), walkdir.clone(), everything.clone());

        let (tx, _rx) = mpsc::channel();
        executor.execute(
            SearchId(8),
            request(
                SearchKind::Filename,
                SearchScope::Roots {
                    roots: vec![".".into()],
                },
            ),
            CancellationToken::new(),
            tx,
        );

        assert!(ripgrep.calls().is_empty());
        assert_eq!(walkdir.calls(), vec![SearchId(8)]);
        assert!(everything.calls().is_empty());
    }

    #[test]
    fn global_filename_request_routes_to_everything() {
        let ripgrep = Arc::new(RecordingExecutor::default());
        let walkdir = Arc::new(RecordingExecutor::default());
        let everything = Arc::new(RecordingExecutor::default());
        let executor = dispatcher_with_settings(
            FileSearchSettings {
                everything_enabled: true,
                ..FileSearchSettings::default()
            },
            ripgrep.clone(),
            walkdir.clone(),
            everything.clone(),
        );

        let (tx, _rx) = mpsc::channel();
        executor.execute(
            SearchId(9),
            request(
                SearchKind::Filename,
                SearchScope::Roots { roots: Vec::new() },
            ),
            CancellationToken::new(),
            tx,
        );

        assert!(ripgrep.calls().is_empty());
        assert!(walkdir.calls().is_empty());
        assert_eq!(everything.calls(), vec![SearchId(9)]);
    }

    #[test]
    fn global_filename_request_routes_to_ripgrep_when_everything_disabled() {
        let ripgrep = Arc::new(RecordingExecutor::default());
        let walkdir = Arc::new(RecordingExecutor::default());
        let everything = Arc::new(RecordingExecutor::default());
        let executor = dispatcher_with_settings(
            FileSearchSettings {
                everything_enabled: false,
                ..FileSearchSettings::default()
            },
            ripgrep.clone(),
            walkdir.clone(),
            everything.clone(),
        );

        let (tx, _rx) = mpsc::channel();
        executor.execute(
            SearchId(10),
            request(
                SearchKind::Filename,
                SearchScope::Roots { roots: Vec::new() },
            ),
            CancellationToken::new(),
            tx,
        );

        assert_eq!(ripgrep.calls(), vec![SearchId(10)]);
        assert!(walkdir.calls().is_empty());
        assert!(everything.calls().is_empty());
    }
}
