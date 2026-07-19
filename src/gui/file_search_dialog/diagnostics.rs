//! Diagnostics rendering helpers for the file-search dialog.

use crate::file_search::model::{
    BackendExecutionDetails, InaccessiblePathDetail, PathIssue, SearchBackend, SearchDiagnostic,
    SearchSummary,
};
use std::path::PathBuf;
use std::time::Duration;

const INACCESSIBLE_SAMPLE_LIMIT: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileSearchDiagnostics {
    pub backend: Option<SearchBackend>,
    pub fallback_warnings: Vec<String>,
    pub inaccessible_paths: Vec<InaccessiblePathDetail>,
    pub per_file_truncations: Vec<(PathBuf, usize, usize)>,
    pub global_matched_file_truncation: Option<usize>,
    pub filename_result_limit_truncation: Option<usize>,
    pub backend_stderr_snippets: Vec<String>,
    pub backend_execution: Option<BackendExecutionDetails>,
    pub summary: Option<SearchSummary>,
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
            SearchDiagnostic::BackendExecution(details) => {
                self.backend = Some(details.backend);
                self.backend_execution = Some(*details);
            }
        }
    }

    pub fn warning_count(&self) -> usize {
        self.fallback_warnings.len() + self.inaccessible_paths.len()
    }

    pub fn summary_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
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

    pub fn record_summary(&mut self, summary: SearchSummary) {
        let mut summary = summary;
        if summary.inaccessible_entries.len() > INACCESSIBLE_SAMPLE_LIMIT {
            summary
                .inaccessible_entries
                .truncate(INACCESSIBLE_SAMPLE_LIMIT);
        }
        self.summary = Some(summary);
    }

    pub fn copy_diagnostics_text(&self) -> String {
        let mut lines = Vec::new();
        if let Some(details) = &self.backend_execution {
            lines.push(format!("Backend: {:?}", details.backend));
            lines.push(format!(
                "Executable path: {}",
                details
                    .executable_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "n/a".to_string())
            ));
            lines.push(format!(
                "Version: {}",
                details.version.as_deref().unwrap_or("n/a")
            ));
            lines.push(format!(
                "Command: {}",
                details.command_without_query.as_deref().unwrap_or("n/a")
            ));
            lines.push(format!(
                "Resolution source: {}",
                details.resolution_source.as_deref().unwrap_or("n/a")
            ));
            lines.push(format!(
                "Search roots: {}",
                format_paths(&details.search_roots)
            ));
            lines.push(format!("Started: {:?}", details.started_at));
            lines.push(format!("Ended: {:?}", details.ended_at));
            lines.push(format!("Cancelled: {}", details.cancelled));
            if let Some(reason) = &details.fallback_reason {
                lines.push(format!("Fallback reason: {reason}"));
            }
        }
        if let Some(summary) = &self.summary {
            lines.push(format!("Duration: {}", format_duration(summary.elapsed)));
            lines.push(format!("Files scanned: {}", summary.files_scanned));
            lines.push(format!(
                "Directories scanned: {}",
                summary.directories_scanned
            ));
            lines.push(format!("Results: {}", summary.result_files));
            lines.push(format!("Displayed rows: {}", summary.displayed_rows));
            lines.push(format!(
                "Truncation: global={}, per-file={}",
                summary.result_limit_reached, summary.per_file_limit_reached
            ));
            lines.push(format!(
                "Inaccessible count: {}",
                summary.inaccessible_count
            ));
            for issue in &summary.inaccessible_entries {
                lines.push(format!(
                    "Inaccessible: {} — {}",
                    issue.path.display(),
                    issue.message
                ));
            }
            if let Some(stderr) = &summary.stderr {
                lines.push(format!("Stderr: {}", stderr.trim()));
            }
            lines.push(format!("Cancellation: {}", summary.cancelled));
        }
        lines.extend(self.summary_lines());
        lines.join("\n")
    }

    pub fn full_command_text(&self) -> Option<String> {
        self.backend_execution
            .as_ref()
            .and_then(|details| details.command_for_display.clone())
    }
}

pub fn format_status_line(
    status: crate::file_search::model::SearchStatus,
    results: usize,
    limit_reached: bool,
) -> String {
    match status {
        crate::file_search::model::SearchStatus::Running => format!("Searching… {results} results"),
        crate::file_search::model::SearchStatus::Completed if limit_reached => {
            format!("Completed — showing first {results} results")
        }
        crate::file_search::model::SearchStatus::Completed => {
            format!("Completed — {results} results")
        }
        crate::file_search::model::SearchStatus::Cancelled => {
            format!("Cancelled — {results} partial results")
        }
        other => format!("{other:?} — {results} results"),
    }
}

pub fn shell_quote_arg(arg: &str) -> String {
    if arg.is_empty() {
        return "''".to_string();
    }
    if arg.chars().all(|ch| {
        ch.is_ascii_alphanumeric() || matches!(ch, '/' | '\\' | '.' | '_' | '-' | ':' | '=')
    }) {
        return arg.to_string();
    }
    format!("'{}'", arg.replace('\'', "'\\''"))
}

pub fn format_command_args<I, S>(program: &str, args: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    std::iter::once(shell_quote_arg(program))
        .chain(args.into_iter().map(|arg| shell_quote_arg(arg.as_ref())))
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_duration(duration: Duration) -> String {
    format!("{} ms", duration.as_millis())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_search::model::{
        BackendExecutionDetails, InaccessiblePathDetail, PathIssue, SearchDiagnostic, SearchStatus,
    };
    use std::time::{Duration, SystemTime};

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

    #[test]
    fn inaccessible_count_and_sample_are_bounded() {
        let mut diagnostics = FileSearchDiagnostics::default();
        diagnostics.record_summary(SearchSummary {
            inaccessible_count: 123,
            inaccessible_entries: (0..150)
                .map(|i| PathIssue {
                    path: format!("/p/{i}").into(),
                    message: "denied".to_string(),
                })
                .collect(),
            ..SearchSummary::default()
        });
        let summary = diagnostics.summary.as_ref().unwrap();
        assert_eq!(summary.inaccessible_count, 123);
        assert_eq!(summary.inaccessible_entries.len(), 100);
    }

    #[test]
    fn global_and_per_file_truncation_status_display() {
        assert_eq!(
            format_status_line(SearchStatus::Completed, 500, true),
            "Completed — showing first 500 results"
        );
        let mut diagnostics = FileSearchDiagnostics::default();
        diagnostics.record_summary(SearchSummary {
            result_limit_reached: true,
            per_file_limit_reached: true,
            ..SearchSummary::default()
        });
        let text = diagnostics.copy_diagnostics_text();
        assert!(text.contains("global=true, per-file=true"));
    }

    #[test]
    fn backend_fallback_recording() {
        let mut diagnostics = FileSearchDiagnostics::default();
        diagnostics.record(SearchDiagnostic::BackendExecution(Box::new(
            BackendExecutionDetails {
                backend: SearchBackend::Native,
                executable_path: None,
                version: None,
                resolution_source: Some("fallback".to_string()),
                command_for_display: None,
                command_without_query: None,
                search_roots: vec!["/tmp".into()],
                started_at: SystemTime::UNIX_EPOCH,
                ended_at: Some(SystemTime::UNIX_EPOCH),
                stderr: None,
                fallback_reason: Some("ripgrep missing".to_string()),
                cancelled: false,
            },
        )));
        let text = diagnostics.copy_diagnostics_text();
        assert!(text.contains("Backend: Native"));
        assert!(text.contains("Fallback reason: ripgrep missing"));
    }

    #[test]
    fn normal_diagnostics_formatting_does_not_leak_query_text() {
        let mut diagnostics = FileSearchDiagnostics::default();
        diagnostics.record(SearchDiagnostic::BackendExecution(Box::new(
            BackendExecutionDetails {
                backend: SearchBackend::Ripgrep,
                executable_path: Some("/usr/bin/rg".into()),
                version: Some("ripgrep 14".to_string()),
                resolution_source: Some("ProcessPath".to_string()),
                command_for_display: Some("rg -e secret_query /tmp".to_string()),
                command_without_query: Some("rg -e <query> /tmp".to_string()),
                search_roots: vec!["/tmp".into()],
                started_at: SystemTime::UNIX_EPOCH,
                ended_at: None,
                stderr: None,
                fallback_reason: None,
                cancelled: false,
            },
        )));
        assert!(!diagnostics.copy_diagnostics_text().contains("secret_query"));
        assert!(
            diagnostics
                .full_command_text()
                .unwrap()
                .contains("secret_query")
        );
    }

    #[test]
    fn detailed_command_formatting_quotes_paths_with_spaces() {
        let command = format_command_args("rg", ["--", "/tmp/path with spaces"]);
        assert_eq!(command, "rg -- '/tmp/path with spaces'");
    }
}
