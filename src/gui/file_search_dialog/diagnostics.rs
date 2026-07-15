//! Diagnostics rendering helpers for the file-search dialog.

use crate::file_search::model::{InaccessiblePathDetail, SearchBackend, SearchDiagnostic};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileSearchDiagnostics {
    pub backend: Option<SearchBackend>,
    pub fallback_warnings: Vec<String>,
    pub inaccessible_paths: Vec<InaccessiblePathDetail>,
    pub per_file_truncations: Vec<(PathBuf, usize, usize)>,
    pub global_matched_file_truncation: Option<usize>,
    pub filename_result_limit_truncation: Option<usize>,
    pub backend_stderr_snippets: Vec<String>,
}

impl FileSearchDiagnostics {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn record(&mut self, diagnostic: SearchDiagnostic) {
        match diagnostic {
            SearchDiagnostic::Warning(message) => self.fallback_warnings.push(message),
            SearchDiagnostic::BackendStderr(snippet) => {
                let bounded: String = snippet.chars().take(4096).collect();
                if !bounded.trim().is_empty() {
                    self.backend_stderr_snippets.push(bounded);
                }
            }
            SearchDiagnostic::InaccessiblePath(detail) => self.inaccessible_paths.push(detail),
            SearchDiagnostic::PerFileContentTruncated {
                path,
                total_matches,
                displayed_matches,
            } => {
                self.per_file_truncations
                    .push((path, total_matches, displayed_matches));
            }
            SearchDiagnostic::GlobalMatchedFilesTruncated { limit } => {
                self.global_matched_file_truncation = Some(limit);
            }
            SearchDiagnostic::FilenameResultsTruncated { limit } => {
                self.filename_result_limit_truncation = Some(limit);
            }
        }
    }

    pub fn warning_count(&self) -> usize {
        self.fallback_warnings.len() + self.inaccessible_paths.len()
    }

    pub fn summary_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(backend) = self.backend {
            lines.push(format!("Backend: {backend:?}"));
        }
        lines.extend(self.fallback_warnings.iter().cloned());
        for detail in &self.inaccessible_paths {
            lines.push(format!(
                "Inaccessible path: {} ({}: {})",
                detail.path.display(),
                detail.operation,
                detail.error
            ));
        }
        for (path, total, displayed) in &self.per_file_truncations {
            lines.push(format!("Per-file content matches truncated for {}: showing {displayed} of {total} matches.", path.display()));
        }
        if let Some(limit) = self.global_matched_file_truncation {
            lines.push(format!("Matched-file results truncated at {limit}."));
        }
        if let Some(limit) = self.filename_result_limit_truncation {
            lines.push(format!("Filename results truncated at {limit}."));
        }
        for snippet in &self.backend_stderr_snippets {
            lines.push(format!("Backend stderr: {}", snippet.trim()));
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_search::model::{InaccessiblePathDetail, SearchDiagnostic};

    #[test]
    fn fallback_warning_is_diagnostic_not_failure() {
        let mut diagnostics = FileSearchDiagnostics::default();
        diagnostics.record(SearchDiagnostic::Warning(
            "ripgrep was not found. Native content search is being used.".to_string(),
        ));
        assert_eq!(diagnostics.warning_count(), 1);
        assert!(diagnostics.summary_lines()[0].contains("Native content search"));
    }

    #[test]
    fn bounded_stderr_display_is_limited() {
        let mut diagnostics = FileSearchDiagnostics::default();
        diagnostics.record(SearchDiagnostic::BackendStderr("x".repeat(10_000)));
        assert_eq!(diagnostics.backend_stderr_snippets[0].len(), 4096);
    }

    #[test]
    fn truncation_lines_are_consistent() {
        let mut diagnostics = FileSearchDiagnostics::default();
        diagnostics.record(SearchDiagnostic::PerFileContentTruncated {
            path: "a.txt".into(),
            total_matches: 4,
            displayed_matches: 2,
        });
        diagnostics.record(SearchDiagnostic::GlobalMatchedFilesTruncated { limit: 10 });
        diagnostics.record(SearchDiagnostic::FilenameResultsTruncated { limit: 20 });
        let lines = diagnostics.summary_lines().join("\n");
        assert!(lines.contains("showing 2 of 4"));
        assert!(lines.contains("Matched-file results truncated at 10"));
        assert!(lines.contains("Filename results truncated at 20"));
    }

    #[test]
    fn inaccessible_details_include_path_operation_and_error() {
        let mut diagnostics = FileSearchDiagnostics::default();
        diagnostics.record(SearchDiagnostic::InaccessiblePath(InaccessiblePathDetail {
            path: "/missing".into(),
            operation: "metadata".to_string(),
            error: "denied".to_string(),
        }));
        let line = diagnostics.summary_lines().join("\n");
        assert!(line.contains("/missing"));
        assert!(line.contains("metadata"));
        assert!(line.contains("denied"));
    }
}
