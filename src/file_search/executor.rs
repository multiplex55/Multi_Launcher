use crate::file_search::coordinator::{CancellationToken, SearchExecutor};
use crate::file_search::everything::{everything_diagnostic, EverythingSearchExecutor};
use crate::file_search::model::{
    FilenameMatchMode, SearchBackend, SearchEvent, SearchId, SearchKind, SearchRequest, SearchScope,
};
use crate::file_search::native::NativeSearchExecutor;
use crate::file_search::ripgrep::{resolve_ripgrep_executable, RipgrepSearchExecutor};
use crate::file_search::settings::FileSearchSettings;
use crate::file_search::walkdir::WalkDirSearchExecutor;
use std::sync::{mpsc, Arc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendPlan {
    pub candidates: Vec<SearchBackend>,
}

/// Production file-search dispatcher that selects an available backend for a
/// request and delegates execution to the corresponding backend executor.
#[derive(Clone)]
pub struct FileSearchExecutor {
    settings: FileSearchSettings,
    ripgrep: Arc<dyn SearchExecutor>,
    native: Arc<dyn SearchExecutor>,
    walkdir: Arc<dyn SearchExecutor>,
    everything: Arc<dyn SearchExecutor>,
}

impl FileSearchExecutor {
    pub fn new(settings: FileSearchSettings) -> Self {
        Self::with_backend_executors(
            settings.clone(),
            Arc::new(RipgrepSearchExecutor::new(settings.clone())),
            Arc::new(NativeSearchExecutor::new(settings.clone())),
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
        native: Arc<dyn SearchExecutor>,
        walkdir: Arc<dyn SearchExecutor>,
        everything: Arc<dyn SearchExecutor>,
    ) -> Self {
        Self {
            settings,
            ripgrep,
            native,
            walkdir,
            everything,
        }
    }

    pub fn plan_for_request(request: &SearchRequest, settings: &FileSearchSettings) -> BackendPlan {
        let candidates = match (&request.kind, &request.scope) {
            (SearchKind::Content, _) => vec![SearchBackend::Ripgrep, SearchBackend::Native],
            (SearchKind::Filename, SearchScope::Roots { roots })
                if is_custom_root(roots, settings) =>
            {
                vec![SearchBackend::WalkDir]
            }
            (SearchKind::Filename, SearchScope::Roots { .. })
                if settings.everything_enabled
                    && request.filename_match_mode == FilenameMatchMode::RankedSubstring =>
            {
                vec![SearchBackend::Everything, SearchBackend::WalkDir]
            }
            (SearchKind::Filename, SearchScope::Roots { .. }) => vec![SearchBackend::WalkDir],
            (SearchKind::Filename, SearchScope::Files { .. }) => vec![SearchBackend::WalkDir],
        };
        BackendPlan { candidates }
    }

    fn availability(&self, backend: SearchBackend) -> Result<(), String> {
        match backend {
            SearchBackend::Ripgrep => {
                let configured = &self.settings.ripgrep_executable_path;
                if configured.is_absolute() && !configured.is_file() {
                    return Err(format!(
                        "ripgrep executable was not found at configured path '{}'",
                        configured.display()
                    ));
                }
                if !configured.as_os_str().is_empty()
                    && configured.components().count() > 1
                    && !configured.is_absolute()
                {
                    return Err(format!(
                        "ripgrep executable path '{}' must be absolute when it includes directories",
                        configured.display()
                    ));
                }
                resolve_ripgrep_executable(configured)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
            SearchBackend::Everything => {
                let diagnostic = everything_diagnostic(&self.settings);
                diagnostic.detected_path.map(|_| ()).ok_or_else(|| {
                    diagnostic
                        .unavailable_reason
                        .unwrap_or_else(|| "Everything executable is unavailable".to_owned())
                })
            }
            SearchBackend::WalkDir | SearchBackend::Native => Ok(()),
        }
    }

    fn executor_for(&self, backend: SearchBackend) -> Arc<dyn SearchExecutor> {
        match backend {
            SearchBackend::Ripgrep => self.ripgrep.clone(),
            SearchBackend::Native => self.native.clone(),
            SearchBackend::WalkDir => self.walkdir.clone(),
            SearchBackend::Everything => self.everything.clone(),
        }
    }

    fn prepare_request_for_backend(
        &self,
        mut request: SearchRequest,
        backend: SearchBackend,
    ) -> SearchRequest {
        if backend == SearchBackend::WalkDir {
            if let SearchScope::Roots { roots } = &mut request.scope {
                if roots.is_empty() {
                    *roots = self.settings.global_search_roots.clone();
                }
            }
        }
        request
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
        let plan = Self::plan_for_request(&request, &self.settings);
        let mut skipped: Option<(SearchBackend, String)> = None;
        for backend in plan.candidates {
            match self.availability(backend) {
                Ok(()) => {
                    if let Some((from, reason)) = skipped.take() {
                        let _ = events.send(SearchEvent::BackendFallback {
                            id,
                            from,
                            to: backend,
                            reason,
                        });
                    }
                    if events.send(SearchEvent::Started { id, backend }).is_err() {
                        return;
                    }
                    let request = self.prepare_request_for_backend(request, backend);
                    self.executor_for(backend)
                        .execute(id, request, token, events);
                    return;
                }
                Err(reason) => {
                    if skipped.is_none() {
                        skipped = Some((backend, reason));
                    }
                }
            }
        }
        let error = skipped
            .map(|(_, r)| r)
            .unwrap_or_else(|| "No search backend is available".to_owned());
        let _ = events.send(SearchEvent::Failed { id, error });
    }
}

fn is_custom_root(roots: &[std::path::PathBuf], settings: &FileSearchSettings) -> bool {
    !roots_match_global_search_roots(roots, settings)
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
        .map(|p| crate::file_search::model::normalize_path_for_identity(p))
        .collect();
    let mut global_roots: Vec<_> = settings
        .global_search_roots
        .iter()
        .map(|p| crate::file_search::model::normalize_path_for_identity(p))
        .collect();
    request_roots.sort();
    global_roots.sort();
    request_roots == global_roots
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_search::model::{ContentMatchMode, FileTypeFilter};
    use std::path::PathBuf;
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
            filename_match_mode: FilenameMatchMode::RankedSubstring,
            content_match_mode: ContentMatchMode::ExactPhrase,
            whole_word: false,
            file_type_filter: FileTypeFilter::FilesAndDirectories,
        }
    }

    fn dispatcher(
        settings: FileSearchSettings,
        ripgrep: Arc<RecordingExecutor>,
        native: Arc<RecordingExecutor>,
        walkdir: Arc<RecordingExecutor>,
        everything: Arc<RecordingExecutor>,
    ) -> FileSearchExecutor {
        FileSearchExecutor::with_backend_executors(settings, ripgrep, native, walkdir, everything)
    }

    #[test]
    fn available_ripgrep_selects_ripgrep() {
        let Ok(rg) = resolve_ripgrep_executable(std::path::Path::new("rg")) else {
            return;
        };
        let ripgrep = Arc::new(RecordingExecutor::default());
        let native = Arc::new(RecordingExecutor::default());
        let walkdir = Arc::new(RecordingExecutor::default());
        let everything = Arc::new(RecordingExecutor::default());
        let executor = dispatcher(
            FileSearchSettings {
                ripgrep_executable_path: rg,
                ..FileSearchSettings::default()
            },
            ripgrep.clone(),
            native.clone(),
            walkdir.clone(),
            everything.clone(),
        );
        let (tx, rx) = mpsc::channel();
        executor.execute(
            SearchId(1),
            request(
                SearchKind::Content,
                SearchScope::Roots {
                    roots: vec![".".into()],
                },
            ),
            CancellationToken::new(),
            tx,
        );
        let events: Vec<_> = rx.try_iter().collect();
        assert!(events.contains(&SearchEvent::Started {
            id: SearchId(1),
            backend: SearchBackend::Ripgrep
        }));
        assert_eq!(ripgrep.calls(), vec![SearchId(1)]);
        assert!(native.calls().is_empty());
    }

    #[test]
    fn missing_ripgrep_selects_native_and_started_reports_native_after_fallback() {
        let ripgrep = Arc::new(RecordingExecutor::default());
        let native = Arc::new(RecordingExecutor::default());
        let walkdir = Arc::new(RecordingExecutor::default());
        let everything = Arc::new(RecordingExecutor::default());
        let temp = tempfile::tempdir().expect("tempdir");
        let executor = dispatcher(
            FileSearchSettings {
                ripgrep_executable_path: temp.path().join("missing-rg"),
                ..FileSearchSettings::default()
            },
            ripgrep.clone(),
            native.clone(),
            walkdir.clone(),
            everything.clone(),
        );
        let (tx, rx) = mpsc::channel();
        executor.execute(
            SearchId(2),
            request(
                SearchKind::Content,
                SearchScope::Roots {
                    roots: vec![temp.path().into()],
                },
            ),
            CancellationToken::new(),
            tx,
        );
        let events: Vec<_> = rx.try_iter().collect();
        assert!(events.iter().any(|event| matches!(event, SearchEvent::BackendFallback { id: SearchId(2), from: SearchBackend::Ripgrep, to: SearchBackend::Native, reason } if reason.contains("ripgrep"))));
        assert!(events.contains(&SearchEvent::Started {
            id: SearchId(2),
            backend: SearchBackend::Native
        }));
        assert!(events.contains(&SearchEvent::Completed { id: SearchId(2) }));
        assert_eq!(native.calls(), vec![SearchId(2)]);
        assert!(ripgrep.calls().is_empty());
        assert!(!events
            .iter()
            .any(|event| matches!(event, SearchEvent::Failed { .. })));
    }

    #[test]
    fn global_fuzzy_filename_does_not_plan_everything() {
        let mut req = request(
            SearchKind::Filename,
            SearchScope::Roots { roots: Vec::new() },
        );
        req.filename_match_mode = FilenameMatchMode::Fuzzy;
        let plan = FileSearchExecutor::plan_for_request(
            &req,
            &FileSearchSettings {
                everything_enabled: true,
                ..FileSearchSettings::default()
            },
        );
        assert_eq!(plan.candidates, vec![SearchBackend::WalkDir]);
    }

    #[test]
    fn newly_configured_path_is_used_by_next_search() {
        let Ok(rg) = resolve_ripgrep_executable(std::path::Path::new("rg")) else {
            return;
        };
        let temp = tempfile::tempdir().expect("tempdir");
        let settings = FileSearchSettings {
            ripgrep_executable_path: temp.path().join("missing-rg"),
            ..FileSearchSettings::default()
        };
        let executor = FileSearchExecutor::new(FileSearchSettings {
            ripgrep_executable_path: rg,
            ..settings
        });
        let plan = FileSearchExecutor::plan_for_request(
            &request(
                SearchKind::Content,
                SearchScope::Roots {
                    roots: vec![PathBuf::from(".")],
                },
            ),
            executor.settings(),
        );
        assert_eq!(plan.candidates[0], SearchBackend::Ripgrep);
        assert!(executor.availability(SearchBackend::Ripgrep).is_ok());
    }
}
