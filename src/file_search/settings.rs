use std::path::PathBuf;

/// Settings used to construct and validate file-search requests/backends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSearchSettings {
    pub global_content_search_roots: Vec<PathBuf>,
    pub excluded_directory_names: Vec<String>,
    pub max_search_results: usize,
    pub max_matches_per_content_file: usize,
    pub max_content_search_file_size_bytes: u64,
    pub include_hidden_files: bool,
    pub case_sensitive: bool,
    pub everything_executable_path: PathBuf,
    pub ripgrep_executable_path: PathBuf,
    pub everything_enabled: bool,
    pub preferred_editor_command: String,
    pub preferred_editor_args: Vec<String>,
    pub preferred_terminal_command: String,
    pub preferred_terminal_args: Vec<String>,
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

impl Default for FileSearchSettings {
    fn default() -> Self {
        Self {
            global_content_search_roots: default_global_content_search_roots(),
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
            include_hidden_files: false,
            case_sensitive: false,
            everything_executable_path: PathBuf::from("Everything.exe"),
            ripgrep_executable_path: PathBuf::from("rg"),
            everything_enabled: cfg!(target_os = "windows"),
            preferred_editor_command: String::new(),
            preferred_editor_args: Vec::new(),
            preferred_terminal_command: String::new(),
            preferred_terminal_args: Vec::new(),
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
        self.global_content_search_roots
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

        if executable_path_is_configured_file(&self.ripgrep_executable_path) == Some(false) {
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

        diagnostics
    }
}

fn default_global_content_search_roots() -> Vec<PathBuf> {
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
            global_content_search_roots: vec![file_path.clone(), temp_dir.path().join("missing")],
            max_search_results: 0,
            max_matches_per_content_file: 0,
            max_content_search_file_size_bytes: 0,
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
