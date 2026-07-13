use crate::file_search::coordinator::{CancellationToken, SearchExecutor};
use crate::file_search::model::{
    ContentFileResultBuilder, ContentMatch, SearchEvent, SearchId, SearchKind, SearchProgress,
    SearchRequest, SearchResult, SearchScope, SearchStatus,
};
use crate::file_search::settings::FileSearchSettings;
use std::fs;
use std::io::{BufRead, BufReader};
use std::sync::mpsc;
use walkdir::WalkDir;

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

pub fn search_content_native(
    id: SearchId,
    request: SearchRequest,
    settings: &FileSearchSettings,
    cancellation: &CancellationToken,
    events: &mpsc::Sender<SearchEvent>,
) -> Result<(), String> {
    if request.kind != SearchKind::Content {
        return Err("native search only supports content requests".to_owned());
    }
    let roots = match &request.scope {
        SearchScope::Roots { roots } if roots.is_empty() => settings.global_search_roots.clone(),
        SearchScope::Roots { roots } => roots.clone(),
        SearchScope::Files { files } => files.clone(),
    };
    let needle = if request.case_sensitive {
        request.text.clone()
    } else {
        request.text.to_lowercase()
    };
    let mut files_scanned = 0;
    let mut results_found = 0;
    let mut cancelled = false;
    for root in roots {
        if cancellation.is_cancelled() {
            cancelled = true;
            break;
        }
        let paths: Box<dyn Iterator<Item = std::path::PathBuf>> = if root.is_file() {
            Box::new(std::iter::once(root))
        } else if root.is_dir() {
            Box::new(
                WalkDir::new(root)
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().is_file())
                    .map(|e| e.into_path()),
            )
        } else {
            continue;
        };
        for path in paths {
            if cancellation.is_cancelled() {
                cancelled = true;
                break;
            }
            let Ok(meta) = fs::metadata(&path) else {
                continue;
            };
            if meta.len() > request.max_file_size_bytes {
                continue;
            }
            files_scanned += 1;
            let Ok(file) = fs::File::open(&path) else {
                continue;
            };
            let mut builder =
                ContentFileResultBuilder::new(path.clone(), settings.max_matches_per_content_file);
            for (idx, line) in BufReader::new(file).lines().enumerate() {
                let Ok(line) = line else {
                    continue;
                };
                let hay = if request.case_sensitive {
                    line.clone()
                } else {
                    line.to_lowercase()
                };
                if let Some(start) = hay.find(&needle) {
                    builder.push_match(ContentMatch::new(
                        idx + 1,
                        line,
                        start,
                        start + needle.len(),
                    ));
                }
            }
            let result = builder.finish();
            if result.total_matches > 0 {
                results_found += 1;
                if events
                    .send(SearchEvent::Result {
                        id,
                        result: SearchResult::ContentFile(result),
                    })
                    .is_err()
                {
                    return Ok(());
                }
                if results_found >= request.max_results {
                    break;
                }
            }
        }
    }
    let status = if cancelled {
        SearchStatus::Cancelled
    } else {
        SearchStatus::Completed
    };
    let _ = events.send(SearchEvent::Progress {
        id,
        progress: SearchProgress {
            files_scanned,
            directories_scanned: 0,
            results_found,
            status,
        },
    });
    let _ = if cancelled {
        events.send(SearchEvent::Cancelled { id })
    } else {
        events.send(SearchEvent::Completed { id })
    };
    Ok(())
}
