//! Filter controls for the file-search dialog.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::file_search::model::{
    ContentMatchMode, FileTypeFilter, FilenameMatchMode, SearchRequest, SearchScope,
};
use crate::file_search::settings::{FileSearchContentSort, FileSearchFilenameSort};
use eframe::egui;

use super::{
    resolve_valid_roots, FileSearchDialogState, FileSearchMode, FileSearchRequestError,
    FileSearchScopeMode,
};

impl FileSearchRequestError {
    pub fn user_message(&self) -> &'static str {
        match self {
            Self::EmptySearchText => "Enter search text before searching.",
            Self::EmptyGlobalRoots => {
                "Configure at least one valid global search root in File Search settings."
            }
            Self::EmptyDirectoryRoots => "Choose at least one valid root directory first.",
        }
    }
}

pub fn normalize_extension_input(input: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for ext in input.split(',') {
        let ext = ext.trim().trim_start_matches('.').to_ascii_lowercase();
        if !ext.is_empty() && seen.insert(ext.clone()) {
            out.push(ext);
        }
    }
    out
}

fn extension_list_text(values: &[String]) -> String {
    values.join(", ")
}

impl FileSearchDialogState {
    pub fn effective_excluded_directory_names(&self) -> Vec<String> {
        if self.excluded_directory_names_overridden {
            self.ui_preferences.excluded_directory_names.clone()
        } else {
            self.settings.excluded_directory_names.clone()
        }
    }

    pub fn build_search_request(&self) -> Result<SearchRequest, FileSearchRequestError> {
        let text = self.search_text.trim();
        if text.is_empty() {
            return Err(FileSearchRequestError::EmptySearchText);
        }
        let scope = match self.selected_scope {
            FileSearchScopeMode::Global => {
                let roots = resolve_valid_roots(self.settings.global_search_roots.clone());
                if roots.is_empty() {
                    return Err(FileSearchRequestError::EmptyGlobalRoots);
                }
                SearchScope::Roots { roots }
            }
            FileSearchScopeMode::Directory => {
                let roots = resolve_valid_roots(self.custom_roots.iter().map(PathBuf::from));
                if roots.is_empty() {
                    return Err(FileSearchRequestError::EmptyDirectoryRoots);
                }
                SearchScope::Roots { roots }
            }
        };

        Ok(SearchRequest {
            kind: self.selected_mode.into(),
            scope,
            text: text.to_string(),
            case_sensitive: self.case_sensitive,
            include_hidden_files: self.include_hidden,
            max_results: self.settings.max_search_results.max(1),
            max_file_size_bytes: self.settings.max_content_search_file_size_bytes.max(1),
            included_extensions: self.ui_preferences.included_extensions.clone(),
            excluded_extensions: self.ui_preferences.excluded_extensions.clone(),
            excluded_directory_names: self.effective_excluded_directory_names(),
            filename_match_mode: self.ui_preferences.filename_match_mode,
            content_match_mode: self.ui_preferences.content_match_mode,
            whole_word: self.ui_preferences.whole_word,
            file_type_filter: match self.selected_mode {
                FileSearchMode::Filename => self.ui_preferences.file_type_filter,
                FileSearchMode::Content => FileTypeFilter::FilesOnly,
            },
        })
    }

    pub fn filters_changed_since_last_search(&self) -> bool {
        matches!(
            self.current_status,
            crate::file_search::model::SearchStatus::Completed
        ) && self
            .last_submitted_request
            .as_ref()
            .is_some_and(|last| self.build_search_request().ok().as_ref() != Some(last))
    }

    pub fn filters_ui(&mut self, ui: &mut egui::Ui) {
        let prefs_before = self.ui_preferences.clone();
        egui::CollapsingHeader::new("Filters")
            .default_open(false)
            .show(ui, |ui| {
                match self.selected_mode {
                    FileSearchMode::Filename => {
                        ui.label("Filename matching");
                        ui.horizontal(|ui| {
                            ui.radio_value(
                                &mut self.ui_preferences.filename_match_mode,
                                FilenameMatchMode::RankedSubstring,
                                "Ranked substring",
                            );
                            ui.radio_value(
                                &mut self.ui_preferences.filename_match_mode,
                                FilenameMatchMode::Fuzzy,
                                "Fuzzy",
                            );
                        });
                    }
                    FileSearchMode::Content => {
                        ui.label("Content matching");
                        ui.horizontal(|ui| {
                            ui.radio_value(
                                &mut self.ui_preferences.content_match_mode,
                                ContentMatchMode::ExactPhrase,
                                "Exact phrase",
                            );
                            ui.radio_value(
                                &mut self.ui_preferences.content_match_mode,
                                ContentMatchMode::AnyTerm,
                                "Match any term",
                            );
                            ui.checkbox(&mut self.ui_preferences.whole_word, "Whole word");
                        });
                    }
                }

                ui.separator();
                match self.selected_mode {
                    FileSearchMode::Filename => {
                        ui.label("Sort");
                        ui.horizontal_wrapped(|ui| {
                            ui.radio_value(
                                &mut self.ui_preferences.filename_sort,
                                FileSearchFilenameSort::Relevance,
                                "Relevance",
                            );
                            ui.radio_value(
                                &mut self.ui_preferences.filename_sort,
                                FileSearchFilenameSort::FilenameAscending,
                                "Filename ↑",
                            );
                            ui.radio_value(
                                &mut self.ui_preferences.filename_sort,
                                FileSearchFilenameSort::FilenameDescending,
                                "Filename ↓",
                            );
                            ui.radio_value(
                                &mut self.ui_preferences.filename_sort,
                                FileSearchFilenameSort::FullPathAscending,
                                "Path ↑",
                            );
                            ui.radio_value(
                                &mut self.ui_preferences.filename_sort,
                                FileSearchFilenameSort::ModifiedNewest,
                                "Modified newest",
                            );
                            ui.radio_value(
                                &mut self.ui_preferences.filename_sort,
                                FileSearchFilenameSort::ModifiedOldest,
                                "Modified oldest",
                            );
                            ui.radio_value(
                                &mut self.ui_preferences.filename_sort,
                                FileSearchFilenameSort::SizeLargest,
                                "Size largest",
                            );
                            ui.radio_value(
                                &mut self.ui_preferences.filename_sort,
                                FileSearchFilenameSort::SizeSmallest,
                                "Size smallest",
                            );
                        });
                    }
                    FileSearchMode::Content => {
                        ui.label("Sort");
                        ui.horizontal_wrapped(|ui| {
                            ui.radio_value(
                                &mut self.ui_preferences.content_sort,
                                FileSearchContentSort::DiscoveryOrder,
                                "Discovery",
                            );
                            ui.radio_value(
                                &mut self.ui_preferences.content_sort,
                                FileSearchContentSort::PathThenLine,
                                "Path then line",
                            );
                            ui.radio_value(
                                &mut self.ui_preferences.content_sort,
                                FileSearchContentSort::MatchCountDescending,
                                "Match count",
                            );
                            ui.radio_value(
                                &mut self.ui_preferences.content_sort,
                                FileSearchContentSort::ModifiedNewest,
                                "Modified newest",
                            );
                            ui.radio_value(
                                &mut self.ui_preferences.content_sort,
                                FileSearchContentSort::FilenameRelevance,
                                "Filename relevance",
                            );
                            ui.radio_value(
                                &mut self.ui_preferences.content_sort,
                                FileSearchContentSort::LineNumber,
                                "Line number",
                            );
                        });
                    }
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Type");
                    let enabled = self.selected_mode == FileSearchMode::Filename;
                    ui.add_enabled_ui(enabled, |ui| {
                        ui.radio_value(
                            &mut self.ui_preferences.file_type_filter,
                            FileTypeFilter::FilesOnly,
                            "Files",
                        );
                        ui.radio_value(
                            &mut self.ui_preferences.file_type_filter,
                            FileTypeFilter::DirectoriesOnly,
                            "Directories",
                        );
                        ui.radio_value(
                            &mut self.ui_preferences.file_type_filter,
                            FileTypeFilter::FilesAndDirectories,
                            "Files and directories",
                        );
                    })
                    .response
                    .on_hover_text(
                        "Content search reads file contents, so it always searches files only.",
                    );
                });

                ui.horizontal(|ui| {
                    ui.label("Include extensions");
                    let mut text = extension_list_text(&self.ui_preferences.included_extensions);
                    if ui.text_edit_singleline(&mut text).changed() {
                        self.ui_preferences.included_extensions = normalize_extension_input(&text);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Exclude extensions");
                    let mut text = extension_list_text(&self.ui_preferences.excluded_extensions);
                    if ui.text_edit_singleline(&mut text).changed() {
                        self.ui_preferences.excluded_extensions = normalize_extension_input(&text);
                    }
                });

                ui.separator();
                ui.label("Excluded directories");
                if !self.excluded_directory_names_overridden {
                    self.ui_preferences.excluded_directory_names =
                        self.settings.excluded_directory_names.clone();
                    self.excluded_directory_names_overridden = true;
                }
                let mut remove = None;
                for (idx, value) in self
                    .ui_preferences
                    .excluded_directory_names
                    .iter_mut()
                    .enumerate()
                {
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(value);
                        if ui.button("Remove").clicked() {
                            remove = Some(idx);
                        }
                    });
                }
                if let Some(idx) = remove {
                    self.ui_preferences.excluded_directory_names.remove(idx);
                }
                ui.horizontal(|ui| {
                    if ui.button("Add exclusion").clicked() {
                        self.ui_preferences
                            .excluded_directory_names
                            .push(String::new());
                    }
                    if ui.button("Restore defaults").clicked() {
                        self.ui_preferences.excluded_directory_names =
                            self.settings.excluded_directory_names.clone();
                        self.excluded_directory_names_overridden = true;
                    }
                    if ui.button("Clear").clicked() {
                        self.ui_preferences.excluded_directory_names.clear();
                        self.excluded_directory_names_overridden = true;
                    }
                });
            });
        if self.ui_preferences != prefs_before {
            self.handle_sort_changed();
        }
        if self.filters_changed_since_last_search() {
            ui.colored_label(
                egui::Color32::YELLOW,
                "Search again to apply changed filters",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_search::settings::FileSearchSettings;
    use tempfile::tempdir;

    #[test]
    fn extension_parsing_splits_trims_and_drops_empty_values() {
        assert_eq!(
            normalize_extension_input(" rs, toml, , md "),
            vec!["rs", "toml", "md"]
        );
    }

    #[test]
    fn leading_dot_normalization_lowercases_extensions() {
        assert_eq!(normalize_extension_input(".RS, .Toml"), vec!["rs", "toml"]);
    }

    #[test]
    fn duplicate_extension_removal_preserves_first_order() {
        assert_eq!(
            normalize_extension_input("rs, .RS, toml, rs"),
            vec!["rs", "toml"]
        );
    }

    #[test]
    fn invalid_empty_root_validation() {
        let state = FileSearchDialogState {
            search_text: "needle".into(),
            selected_scope: FileSearchScopeMode::Directory,
            custom_roots: vec!["/definitely/missing/root".into()],
            ..Default::default()
        };
        assert_eq!(
            state.build_search_request().unwrap_err(),
            FileSearchRequestError::EmptyDirectoryRoots
        );
    }

    #[test]
    fn empty_search_text_validation() {
        let state = FileSearchDialogState::default();
        assert_eq!(
            state.build_search_request().unwrap_err(),
            FileSearchRequestError::EmptySearchText
        );
    }

    #[test]
    fn temporary_removal_of_target_reflected_in_request() {
        let temp = tempdir().unwrap();
        let mut state = FileSearchDialogState {
            search_text: "needle".into(),
            selected_scope: FileSearchScopeMode::Directory,
            custom_roots: vec![temp.path().display().to_string()],
            settings: FileSearchSettings {
                excluded_directory_names: vec!["target".into(), ".git".into()],
                ..Default::default()
            },
            excluded_directory_names_overridden: true,
            ..Default::default()
        };
        state.ui_preferences.excluded_directory_names = vec![".git".into()];
        let request = state.build_search_request().unwrap();
        assert_eq!(request.excluded_directory_names, vec![".git"]);
    }

    #[test]
    fn cleared_settings_exclusions_are_not_readded_to_request() {
        let temp = tempdir().unwrap();
        let state = FileSearchDialogState {
            search_text: "needle".into(),
            selected_scope: FileSearchScopeMode::Directory,
            custom_roots: vec![temp.path().display().to_string()],
            settings: FileSearchSettings {
                excluded_directory_names: vec!["target".into()],
                ..Default::default()
            },
            excluded_directory_names_overridden: true,
            ..Default::default()
        };

        let request = state.build_search_request().unwrap();

        assert!(request.excluded_directory_names.is_empty());
    }

    #[test]
    fn content_mode_always_produces_files_only() {
        let temp = tempdir().unwrap();
        let mut state = FileSearchDialogState {
            search_text: "needle".into(),
            selected_mode: FileSearchMode::Content,
            selected_scope: FileSearchScopeMode::Directory,
            custom_roots: vec![temp.path().display().to_string()],
            ..Default::default()
        };
        state.ui_preferences.file_type_filter = FileTypeFilter::DirectoriesOnly;
        let request = state.build_search_request().unwrap();
        assert_eq!(request.file_type_filter, FileTypeFilter::FilesOnly);
    }

    #[test]
    fn changing_filters_does_not_start_search_automatically() {
        let mut state = FileSearchDialogState::default();
        assert!(state.active_search_id.is_none());
        state.ui_preferences.included_extensions = normalize_extension_input("rs");
        assert!(state.active_search_id.is_none());
        assert_eq!(
            state.current_status,
            crate::file_search::model::SearchStatus::Pending
        );
    }
}
