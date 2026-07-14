use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

pub const DEFAULT_MAX_FULL_PREVIEW_FILE_SIZE_BYTES: u64 = 5 * 1024 * 1024;

/// Settings used to construct and validate file-search requests/backends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileSearchSettings {
    #[serde(alias = "global_content_search_roots")]
    pub global_search_roots: Vec<PathBuf>,
    pub excluded_directory_names: Vec<String>,
    pub max_search_results: usize,
    pub max_matches_per_content_file: usize,
    pub max_content_search_file_size_bytes: u64,
    pub max_full_preview_file_size_bytes: u64,
    pub include_hidden_files: bool,
    pub case_sensitive: bool,
    pub everything_executable_path: PathBuf,
    pub ripgrep_executable_path: PathBuf,
    pub everything_enabled: bool,
    pub preferred_editor_command: String,
    pub preferred_editor_args: Vec<String>,
    pub preferred_terminal_command: String,
    pub preferred_terminal_args: Vec<String>,
    #[serde(default)]
    pub ui_preferences: FileSearchUiPreferences,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileSearchFilenameSort {
    Relevance,
    #[serde(alias = "name")]
    FilenameAscending,
    FilenameDescending,
    #[serde(alias = "path")]
    FullPathAscending,
    ModifiedNewest,
    ModifiedOldest,
    SizeLargest,
    SizeSmallest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileSearchContentSort {
    DiscoveryOrder,
    PathThenLine,
    MatchCountDescending,
    ModifiedNewest,
    FilenameRelevance,
    #[serde(alias = "line_then_path")]
    LineNumber,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileSearchColumn {
    Name,
    Directory,
    Kind,
    MatchQuality,
    Path,
    Line,
    MatchText,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileSearchUiPreferences {
    pub filename_sort: FileSearchFilenameSort,
    pub content_sort: FileSearchContentSort,
    pub filename_match_mode: crate::file_search::model::FilenameMatchMode,
    pub content_match_mode: crate::file_search::model::ContentMatchMode,
    pub whole_word: bool,
    pub file_type_filter: crate::file_search::model::FileTypeFilter,
    pub included_extensions: Vec<String>,
    pub excluded_extensions: Vec<String>,
    pub excluded_directory_names: Vec<String>,
    pub visible_columns: Vec<FileSearchColumn>,
    pub column_widths: BTreeMap<FileSearchColumn, u32>,
}

impl Default for FileSearchUiPreferences {
    fn default() -> Self {
        Self {
            filename_sort: FileSearchFilenameSort::Relevance,
            content_sort: FileSearchContentSort::PathThenLine,
            filename_match_mode: crate::file_search::model::FilenameMatchMode::RankedSubstring,
            content_match_mode: crate::file_search::model::ContentMatchMode::ExactPhrase,
            whole_word: false,
            file_type_filter: crate::file_search::model::FileTypeFilter::FilesAndDirectories,
            included_extensions: Vec::new(),
            excluded_extensions: Vec::new(),
            excluded_directory_names: Vec::new(),
            visible_columns: vec![
                FileSearchColumn::Name,
                FileSearchColumn::Directory,
                FileSearchColumn::MatchQuality,
            ],
            column_widths: BTreeMap::new(),
        }
    }
}

/// Non-panicking validation diagnostics for user-editable file-search settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileSearchSettingsDiagnostic {
    InvalidRootPath {
        path: PathBuf,
        message: String,
    },
    MissingExecutable {
        name: &'static str,
        path: PathBuf,
    },
    UnusableMaxValue {
        field: &'static str,
        value: u64,
        message: String,
    },
}

impl fmt::Display for FileSearchSettingsDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRootPath { path, message } => {
                write!(f, "Invalid root '{}': {message}", path.display())
            }
            Self::MissingExecutable { name, path } => {
                write!(f, "Missing {name} executable: '{}'", path.display())
            }
            Self::UnusableMaxValue {
                field,
                value,
                message,
            } => {
                write!(f, "Invalid {field} ({value}): {message}")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSearchDiagnosticsState {
    pub everything_enabled: bool,
    pub detected_everything: Option<PathBuf>,
    pub detected_ripgrep: Option<PathBuf>,
    pub valid_roots: Vec<PathBuf>,
    pub invalid_roots: Vec<PathBuf>,
    pub current_backend: Option<String>,
    pub active_search_state: String,
    pub last_search_duration_ms: Option<u128>,
    pub last_result_count: usize,
    pub last_backend_error: Option<String>,
    pub inaccessible_entry_count: usize,
    pub preview_cache_usage: String,
    pub max_full_preview_file_size_bytes: u64,
}

impl FileSearchDiagnosticsState {
    pub fn from_settings(settings: &FileSearchSettings) -> Self {
        let mut valid_roots = Vec::new();
        let mut invalid_roots = Vec::new();
        for root in &settings.global_search_roots {
            if root.is_dir() {
                valid_roots.push(root.clone());
            } else {
                invalid_roots.push(root.clone());
            }
        }
        Self {
            everything_enabled: settings.everything_enabled,
            detected_everything: crate::file_search::everything::detect_everything_executable(
                settings,
            ),
            detected_ripgrep: detect_ripgrep_executable(settings),
            valid_roots,
            invalid_roots,
            current_backend: None,
            active_search_state: "idle".to_owned(),
            last_search_duration_ms: None,
            last_result_count: 0,
            last_backend_error: None,
            inaccessible_entry_count: 0,
            preview_cache_usage: "0 entries".to_owned(),
            max_full_preview_file_size_bytes: settings.max_full_preview_file_size_bytes,
        }
    }
}

impl fmt::Display for FileSearchDiagnosticsState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Use Everything for global filename search: {}; detected es.exe: {}; detected rg: {}; valid roots: {}; invalid roots: {}; current backend: {}; active search state: {}; last search duration: {}; last result count: {}; last backend error: {}; inaccessible entries: {}; preview cache: {}; full-file preview limit: {} bytes",
            self.everything_enabled,
            self.detected_everything
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "not detected".into()),
            self.detected_ripgrep
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "not detected".into()),
            self.valid_roots.len(),
            self.invalid_roots.len(),
            self.current_backend.as_deref().unwrap_or("none"),
            self.active_search_state,
            self.last_search_duration_ms
                .map(|ms| format!("{ms} ms"))
                .unwrap_or_else(|| "none".into()),
            self.last_result_count,
            self.last_backend_error.as_deref().unwrap_or("none"),
            self.inaccessible_entry_count,
            self.preview_cache_usage,
            self.max_full_preview_file_size_bytes
        )
    }
}

pub fn detect_ripgrep_executable(settings: &FileSearchSettings) -> Option<PathBuf> {
    crate::file_search::ripgrep::resolve_ripgrep_executable(&settings.ripgrep_executable_path).ok()
}

impl Default for FileSearchSettings {
    fn default() -> Self {
        Self {
            global_search_roots: default_global_search_roots(),
            excluded_directory_names: [
                ".git",
                "target",
                "node_modules",
                ".vs",
                ".idea",
                "bin",
                "obj",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            max_search_results: 500,
            max_matches_per_content_file: 25,
            max_content_search_file_size_bytes: 2 * 1024 * 1024,
            max_full_preview_file_size_bytes: DEFAULT_MAX_FULL_PREVIEW_FILE_SIZE_BYTES,
            include_hidden_files: false,
            case_sensitive: false,
            everything_executable_path: PathBuf::from("Everything.exe"),
            ripgrep_executable_path: PathBuf::from("rg"),
            everything_enabled: cfg!(target_os = "windows"),
            preferred_editor_command: String::new(),
            preferred_editor_args: Vec::new(),
            preferred_terminal_command: String::new(),
            preferred_terminal_args: Vec::new(),
            ui_preferences: FileSearchUiPreferences::default(),
        }
    }
}

impl FileSearchSettings {
    /// Returns all validation diagnostics without panicking.
    pub fn validate(&self) -> Vec<FileSearchSettingsDiagnostic> {
        let mut diagnostics = Vec::new();
        diagnostics.extend(self.validate_root_paths());
        diagnostics.extend(self.validate_configured_executables());
        diagnostics.extend(self.validate_max_values());
        diagnostics
    }

    pub fn validate_root_paths(&self) -> Vec<FileSearchSettingsDiagnostic> {
        self.global_search_roots
            .iter()
            .filter_map(|path| match path.metadata() {
                Ok(metadata) if metadata.is_dir() => None,
                Ok(_) => Some(FileSearchSettingsDiagnostic::InvalidRootPath {
                    path: path.clone(),
                    message: "path is not a directory".to_owned(),
                }),
                Err(error) => Some(FileSearchSettingsDiagnostic::InvalidRootPath {
                    path: path.clone(),
                    message: error.to_string(),
                }),
            })
            .collect()
    }

    pub fn validate_configured_executables(&self) -> Vec<FileSearchSettingsDiagnostic> {
        let mut diagnostics = Vec::new();

        if self.everything_enabled
            && executable_path_is_configured_file(&self.everything_executable_path) == Some(false)
        {
            diagnostics.push(FileSearchSettingsDiagnostic::MissingExecutable {
                name: "Everything",
                path: self.everything_executable_path.clone(),
            });
        }

        let ripgrep_path = &self.ripgrep_executable_path;
        let ripgrep_explicit_path_missing = !ripgrep_path.as_os_str().is_empty()
            && ((ripgrep_path.is_absolute() && !ripgrep_path.is_file())
                || (ripgrep_path.components().count() > 1 && !ripgrep_path.is_absolute()));
        if ripgrep_explicit_path_missing
            || crate::file_search::ripgrep::resolve_ripgrep_executable(ripgrep_path).is_err()
        {
            diagnostics.push(FileSearchSettingsDiagnostic::MissingExecutable {
                name: "ripgrep",
                path: self.ripgrep_executable_path.clone(),
            });
        }

        if !self.preferred_editor_command.trim().is_empty()
            && !crate::file_search::actions::configured_executable_available(
                &self.preferred_editor_command,
            )
        {
            diagnostics.push(FileSearchSettingsDiagnostic::MissingExecutable {
                name: "preferred editor",
                path: PathBuf::from(self.preferred_editor_command.trim()),
            });
        }

        if !self.preferred_terminal_command.trim().is_empty()
            && !crate::file_search::actions::configured_executable_available(
                &self.preferred_terminal_command,
            )
        {
            diagnostics.push(FileSearchSettingsDiagnostic::MissingExecutable {
                name: "preferred terminal",
                path: PathBuf::from(self.preferred_terminal_command.trim()),
            });
        }

        diagnostics
    }

    pub fn validate_max_values(&self) -> Vec<FileSearchSettingsDiagnostic> {
        let mut diagnostics = Vec::new();

        if self.max_search_results == 0 {
            diagnostics.push(FileSearchSettingsDiagnostic::UnusableMaxValue {
                field: "max_search_results",
                value: self.max_search_results as u64,
                message: "must be greater than zero".to_owned(),
            });
        }

        if self.max_matches_per_content_file == 0 {
            diagnostics.push(FileSearchSettingsDiagnostic::UnusableMaxValue {
                field: "max_matches_per_content_file",
                value: self.max_matches_per_content_file as u64,
                message: "must be greater than zero".to_owned(),
            });
        }

        if self.max_content_search_file_size_bytes == 0 {
            diagnostics.push(FileSearchSettingsDiagnostic::UnusableMaxValue {
                field: "max_content_search_file_size_bytes",
                value: self.max_content_search_file_size_bytes,
                message: "must be greater than zero".to_owned(),
            });
        }

        if self.max_full_preview_file_size_bytes == 0 {
            diagnostics.push(FileSearchSettingsDiagnostic::UnusableMaxValue {
                field: "max_full_preview_file_size_bytes",
                value: self.max_full_preview_file_size_bytes,
                message: "must be greater than zero".to_owned(),
            });
        }

        diagnostics
    }
}

fn default_global_search_roots() -> Vec<PathBuf> {
    dirs_next::home_dir().into_iter().collect()
}

fn executable_path_is_configured_file(path: &PathBuf) -> Option<bool> {
    if path.as_os_str().is_empty() || path.components().count() == 1 {
        return None;
    }

    Some(path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_include_expected_values() {
        let settings = FileSearchSettings::default();

        assert_eq!(
            settings.excluded_directory_names,
            vec![
                ".git",
                "target",
                "node_modules",
                ".vs",
                ".idea",
                "bin",
                "obj"
            ]
        );
        assert_eq!(settings.max_search_results, 500);
        assert_eq!(settings.max_matches_per_content_file, 25);
        assert_eq!(settings.max_content_search_file_size_bytes, 2 * 1024 * 1024);
        assert_eq!(
            settings.max_full_preview_file_size_bytes,
            DEFAULT_MAX_FULL_PREVIEW_FILE_SIZE_BYTES
        );
        assert!(!settings.include_hidden_files);
        assert!(!settings.case_sensitive);
        assert_eq!(
            settings.everything_executable_path,
            PathBuf::from("Everything.exe")
        );
        assert_eq!(settings.ripgrep_executable_path, PathBuf::from("rg"));
        assert!(settings.preferred_editor_args.is_empty());
        assert!(settings.preferred_terminal_args.is_empty());
    }

    #[test]
    fn validation_reports_invalid_roots_and_unusable_max_values() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let file_path = temp_dir.path().join("file.txt");
        std::fs::write(&file_path, "content").expect("write temp file");

        let settings = FileSearchSettings {
            global_search_roots: vec![file_path.clone(), temp_dir.path().join("missing")],
            max_search_results: 0,
            max_matches_per_content_file: 0,
            max_content_search_file_size_bytes: 0,
            max_full_preview_file_size_bytes: 0,
            ..FileSearchSettings::default()
        };

        let diagnostics = settings.validate();

        assert!(diagnostics.iter().any(|diagnostic| matches!(
            diagnostic,
            FileSearchSettingsDiagnostic::InvalidRootPath { path, .. } if path == &file_path
        )));
        assert!(diagnostics.iter().any(|diagnostic| matches!(
            diagnostic,
            FileSearchSettingsDiagnostic::UnusableMaxValue {
                field: "max_search_results",
                ..
            }
        )));
        assert!(diagnostics.iter().any(|diagnostic| matches!(
            diagnostic,
            FileSearchSettingsDiagnostic::UnusableMaxValue {
                field: "max_matches_per_content_file",
                ..
            }
        )));
        assert!(diagnostics.iter().any(|diagnostic| matches!(
            diagnostic,
            FileSearchSettingsDiagnostic::UnusableMaxValue {
                field: "max_content_search_file_size_bytes",
                ..
            }
        )));
        assert!(diagnostics.iter().any(|diagnostic| matches!(
            diagnostic,
            FileSearchSettingsDiagnostic::UnusableMaxValue {
                field: "max_full_preview_file_size_bytes",
                ..
            }
        )));
    }

    #[test]
    fn zero_preview_limit_creates_validation_diagnostic() {
        let settings = FileSearchSettings {
            max_full_preview_file_size_bytes: 0,
            ..FileSearchSettings::default()
        };

        let diagnostics = settings.validate_max_values();

        assert!(diagnostics.iter().any(|diagnostic| matches!(
            diagnostic,
            FileSearchSettingsDiagnostic::UnusableMaxValue {
                field: "max_full_preview_file_size_bytes",
                value: 0,
                ..
            }
        )));
    }

    #[test]
    fn validation_reports_missing_configured_executables() {
        let settings = FileSearchSettings {
            everything_enabled: true,
            everything_executable_path: PathBuf::from("/definitely/missing/Everything.exe"),
            ripgrep_executable_path: PathBuf::from("/definitely/missing/rg"),
            ..FileSearchSettings::default()
        };

        let diagnostics = settings.validate_configured_executables();

        assert!(diagnostics.iter().any(|diagnostic| matches!(
            diagnostic,
            FileSearchSettingsDiagnostic::MissingExecutable {
                name: "Everything",
                ..
            }
        )));
        assert!(diagnostics.iter().any(|diagnostic| matches!(
            diagnostic,
            FileSearchSettingsDiagnostic::MissingExecutable {
                name: "ripgrep",
                ..
            }
        )));
    }

    #[test]
    fn validation_reports_missing_preferred_invocation_executables() {
        let settings = FileSearchSettings {
            preferred_editor_command: "/definitely/missing/editor".to_string(),
            preferred_terminal_command: "/definitely/missing/terminal".to_string(),
            ..FileSearchSettings::default()
        };

        let diagnostics = settings.validate_configured_executables();

        assert!(diagnostics.iter().any(|diagnostic| matches!(
            diagnostic,
            FileSearchSettingsDiagnostic::MissingExecutable {
                name: "preferred editor",
                ..
            }
        )));
        assert!(diagnostics.iter().any(|diagnostic| matches!(
            diagnostic,
            FileSearchSettingsDiagnostic::MissingExecutable {
                name: "preferred terminal",
                ..
            }
        )));
    }
}

#[cfg(test)]
mod diagnostics_state_tests {
    use super::*;

    #[test]
    fn settings_deserialization_with_missing_fields_uses_defaults() {
        let parsed: FileSearchSettings =
            serde_json::from_str(r#"{"max_search_results":42}"#).unwrap();
        assert_eq!(parsed.max_search_results, 42);
        assert_eq!(
            parsed.max_matches_per_content_file,
            FileSearchSettings::default().max_matches_per_content_file
        );
        assert_eq!(parsed.ripgrep_executable_path, PathBuf::from("rg"));
        assert_eq!(
            parsed.max_full_preview_file_size_bytes,
            DEFAULT_MAX_FULL_PREVIEW_FILE_SIZE_BYTES
        );
    }

    #[test]
    fn old_global_content_search_roots_alias_deserializes() {
        let parsed: FileSearchSettings =
            serde_json::from_str(r#"{"global_content_search_roots":["/tmp/legacy-root"]}"#)
                .expect("legacy root key should deserialize");

        assert_eq!(
            parsed.global_search_roots,
            vec![PathBuf::from("/tmp/legacy-root")]
        );
    }

    #[test]
    fn default_ui_preferences_match_expected_file_search_defaults() {
        let prefs = FileSearchUiPreferences::default();

        assert_eq!(prefs.filename_sort, FileSearchFilenameSort::Relevance);
        assert_eq!(prefs.content_sort, FileSearchContentSort::PathThenLine);
        assert_eq!(
            prefs.filename_match_mode,
            crate::file_search::model::FilenameMatchMode::RankedSubstring
        );
        assert_eq!(
            prefs.content_match_mode,
            crate::file_search::model::ContentMatchMode::ExactPhrase
        );
        assert!(!prefs.whole_word);
        assert_eq!(
            prefs.file_type_filter,
            crate::file_search::model::FileTypeFilter::FilesAndDirectories
        );
        assert_eq!(
            prefs.visible_columns,
            vec![
                FileSearchColumn::Name,
                FileSearchColumn::Directory,
                FileSearchColumn::MatchQuality
            ]
        );
    }

    #[test]
    fn ui_preferences_serialization_excludes_search_session_state() {
        let serialized =
            serde_json::to_string(&FileSearchUiPreferences::default()).expect("serialize prefs");

        assert!(!serialized.contains("query"));
        assert!(!serialized.contains("selected"));
        assert!(!serialized.contains("timestamp"));
        assert!(!serialized.contains("history"));
    }

    #[test]
    fn existing_settings_json_without_preview_limit_remains_deserializable() {
        let parsed: FileSearchSettings = serde_json::from_str(
            r#"{
                "global_search_roots": [],
                "excluded_directory_names": [".git", "target"],
                "max_search_results": 123,
                "max_matches_per_content_file": 10,
                "max_content_search_file_size_bytes": 4096,
                "include_hidden_files": true,
                "case_sensitive": true,
                "everything_executable_path": "Everything.exe",
                "ripgrep_executable_path": "rg",
                "everything_enabled": false,
                "preferred_editor_command": "",
                "preferred_editor_args": [],
                "preferred_terminal_command": "",
                "preferred_terminal_args": []
            }"#,
        )
        .expect("old settings JSON should deserialize");

        assert_eq!(parsed.max_search_results, 123);
        assert_eq!(
            parsed.max_full_preview_file_size_bytes,
            DEFAULT_MAX_FULL_PREVIEW_FILE_SIZE_BYTES
        );
    }

    #[test]
    fn invalid_settings_do_not_panic() {
        let settings = FileSearchSettings {
            global_search_roots: vec![PathBuf::from("/definitely/missing/root")],
            everything_enabled: true,
            everything_executable_path: PathBuf::from("/definitely/missing/es.exe"),
            ripgrep_executable_path: PathBuf::from("/definitely/missing/rg"),
            max_search_results: 0,
            max_matches_per_content_file: 0,
            max_content_search_file_size_bytes: 0,
            max_full_preview_file_size_bytes: 0,
            ..FileSearchSettings::default()
        };
        let diagnostics = settings.validate();
        assert!(diagnostics.len() >= 5);
    }

    #[test]
    fn diagnostics_formatting_includes_expected_fields() {
        let state = FileSearchDiagnosticsState {
            everything_enabled: true,
            detected_everything: Some(PathBuf::from("es.exe")),
            detected_ripgrep: Some(PathBuf::from("rg")),
            valid_roots: vec![PathBuf::from("/")],
            invalid_roots: vec![PathBuf::from("/missing")],
            current_backend: Some("ripgrep".into()),
            active_search_state: "running".into(),
            last_search_duration_ms: Some(12),
            last_result_count: 7,
            last_backend_error: Some("boom".into()),
            inaccessible_entry_count: 3,
            preview_cache_usage: "2 entries".into(),
            max_full_preview_file_size_bytes: DEFAULT_MAX_FULL_PREVIEW_FILE_SIZE_BYTES,
        };
        let formatted = state.to_string();
        assert!(formatted.contains("Use Everything for global filename search: true"));
        assert!(formatted.contains("current backend: ripgrep"));
        assert!(formatted.contains("preview cache: 2 entries"));
        assert!(formatted.contains("full-file preview limit: 5242880 bytes"));
    }

    #[test]
    fn sensitive_query_text_is_not_in_normal_diagnostics_string() {
        let secret = "super-secret-content-query";
        let mut state = FileSearchDiagnosticsState::from_settings(&FileSearchSettings::default());
        state.active_search_state = "running".into();
        let formatted = state.to_string();
        assert!(!formatted.contains(secret));
    }
}
