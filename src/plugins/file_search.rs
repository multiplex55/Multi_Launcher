use crate::actions::Action;
use crate::file_search::actions::{
    MODE_PREFIX, OPEN_ACTION, START_PREFIX, encode_action_payload, mode_action_payload,
    start_action_payload,
};
use crate::file_search::discovery::{self, ExecutableResolutionSource, ExecutableSearchContext};
use crate::file_search::model::SearchKind;
use crate::file_search::query::{FileSearchCommand, SearchRequestDraft};
use crate::file_search::settings::{
    DEFAULT_MAX_FULL_PREVIEW_FILE_SIZE_BYTES, FileSearchDiagnosticsState,
    FileSearchExecutableProbe, FileSearchSettings,
};
use crate::plugin::Plugin;
use eframe::egui;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct FileSearchPlugin {
    settings: FileSearchSettings,
    ripgrep_ui: RipgrepSettingsUiState,
    diagnostics: Option<FileSearchDiagnosticsState>,
}

#[derive(Debug, Clone, Default)]
struct RipgrepSettingsUiState {
    automatic_result: Option<crate::file_search::discovery::RipgrepResolution>,
    configured_result: Option<crate::file_search::discovery::RipgrepResolution>,
    last_tested_path: Option<PathBuf>,
}

impl Plugin for FileSearchPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        match crate::file_search::query::parse_file_search_query(query) {
            None => Vec::new(),
            Some(FileSearchCommand::OpenWindow) => vec![open_action()],
            Some(FileSearchCommand::OpenWindowWithMode { kind }) => vec![mode_action(kind)],
            Some(FileSearchCommand::StartSearch(request)) => vec![start_action(request)],
            Some(FileSearchCommand::RequestDirectory { kind, search_text }) => {
                vec![request_directory_action(kind, search_text)]
            }
            Some(FileSearchCommand::Error(error)) => vec![error_action(error.to_string())],
        }
    }

    fn name(&self) -> &str {
        "file_search"
    }

    fn description(&self) -> &str {
        "Opens local filename/content search with prefix `fs`"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            command_action("fs", "Open local file search"),
            command_action("fs file", "Search local filenames"),
            command_action("fs content", "Search local file contents"),
            command_action("fs here", "Choose a folder for a local file search"),
            command_action("fs here file", "Search filenames in a selected folder"),
            command_action("fs here content", "Search contents in a selected folder"),
        ]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["fs"]
    }

    fn default_settings(&self) -> Option<serde_json::Value> {
        serde_json::to_value(FileSearchSettings::default()).ok()
    }

    fn apply_settings(&mut self, value: &serde_json::Value) {
        self.settings = serde_json::from_value(value.clone()).unwrap_or_else(|error| {
            tracing::warn!(%error, "invalid file_search settings; using defaults");
            FileSearchSettings::default()
        });
        for diagnostic in self.settings.validate() {
            tracing::warn!(%diagnostic, "file_search settings warning");
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui, value: &mut serde_json::Value) {
        let mut cfg: FileSearchSettings = serde_json::from_value(value.clone()).unwrap_or_default();
        ui.heading("File Search");
        path_list_editor(ui, "Global search roots", &mut cfg.global_search_roots);
        string_list_editor(
            ui,
            "Excluded directory names",
            &mut cfg.excluded_directory_names,
        );
        ui.add(
            egui::Slider::new(&mut cfg.max_search_results, 1..=10_000).text("Max search results"),
        );
        ui.add(
            egui::Slider::new(&mut cfg.max_matches_per_content_file, 1..=1_000)
                .text("Max matches per content file"),
        );
        ui.horizontal(|ui| {
            ui.label("Max content-search file size (bytes)");
            ui.add(
                egui::DragValue::new(&mut cfg.max_content_search_file_size_bytes)
                    .speed(1024.0)
                    .clamp_range(1..=u64::MAX),
            );
        });
        full_preview_limit_mib_editor(ui, &mut cfg.max_full_preview_file_size_bytes);
        ui.checkbox(&mut cfg.include_hidden_files, "Include hidden by default");
        ui.checkbox(&mut cfg.case_sensitive, "Case-sensitive by default");
        ui.checkbox(
            &mut cfg.everything_enabled,
            "Use Everything ES CLI for global filename search",
        );
        ui.label(
            "When disabled, global filename searches fall back to WalkDir/ripgrep as applicable.",
        );
        path_field(
            ui,
            "Everything ES CLI executable path (es.exe)",
            &mut cfg.everything_executable_path,
        );
        ripgrep_settings_ui(ui, &mut cfg.ripgrep_executable_path, &mut self.ripgrep_ui);
        ui.horizontal(|ui| {
            ui.label("Preferred editor command");
            ui.text_edit_singleline(&mut cfg.preferred_editor_command);
        });
        string_list_editor(ui, "Preferred editor args", &mut cfg.preferred_editor_args);
        ui.horizontal(|ui| {
            ui.label("Preferred terminal command");
            ui.text_edit_singleline(&mut cfg.preferred_terminal_command);
        });
        string_list_editor(
            ui,
            "Preferred terminal args",
            &mut cfg.preferred_terminal_args,
        );
        for diagnostic in cfg.validate() {
            ui.colored_label(egui::Color32::YELLOW, diagnostic.to_string());
        }
        ui.collapsing("Diagnostics", |ui| {
            if ui.button("Refresh diagnostics").clicked() {
                self.diagnostics = Some(refreshed_diagnostics(&cfg));
            }
            let diagnostics = self
                .diagnostics
                .clone()
                .unwrap_or_else(|| FileSearchDiagnosticsState::from_settings(&cfg));
            ui.monospace(diagnostics.to_string());
        });
        self.settings = cfg.clone();
        if let Ok(v) = serde_json::to_value(&cfg) {
            *value = v;
        }
    }
}

fn refreshed_diagnostics(settings: &FileSearchSettings) -> FileSearchDiagnosticsState {
    let mut diagnostics = FileSearchDiagnosticsState::from_settings(settings);
    diagnostics.detected_everything =
        crate::file_search::everything::detect_everything_executable(settings)
            .map(FileSearchExecutableProbe::Detected)
            .unwrap_or(FileSearchExecutableProbe::NotDetected);
    diagnostics.detected_ripgrep =
        crate::file_search::settings::detect_ripgrep_executable(settings)
            .map(FileSearchExecutableProbe::Detected)
            .unwrap_or(FileSearchExecutableProbe::NotDetected);
    diagnostics
}

const BYTES_PER_MIB: u64 = 1024 * 1024;

fn full_preview_limit_mib_editor(ui: &mut egui::Ui, bytes: &mut u64) {
    let max_mib = u64::MAX / BYTES_PER_MIB;
    let mut mib = (*bytes / BYTES_PER_MIB).max(1);
    ui.horizontal(|ui| {
        ui.label("Full-file preview limit (MiB)");
        if ui
            .add(
                egui::DragValue::new(&mut mib)
                    .speed(1.0)
                    .clamp_range(1..=max_mib),
            )
            .changed()
        {
            *bytes = mib
                .max(1)
                .checked_mul(BYTES_PER_MIB)
                .unwrap_or(DEFAULT_MAX_FULL_PREVIEW_FILE_SIZE_BYTES);
        }
    });
    if *bytes == 0 {
        *bytes = DEFAULT_MAX_FULL_PREVIEW_FILE_SIZE_BYTES;
    }
}

type RipgrepProbe<'a> =
    dyn Fn(&Path, &ExecutableSearchContext) -> Option<discovery::RipgrepResolution> + 'a;

fn ripgrep_settings_ui(ui: &mut egui::Ui, path: &mut PathBuf, state: &mut RipgrepSettingsUiState) {
    ripgrep_settings_ui_with_probe(ui, path, state, &discovery::discover_ripgrep);
}

fn ripgrep_settings_ui_with_probe(
    ui: &mut egui::Ui,
    path: &mut PathBuf,
    state: &mut RipgrepSettingsUiState,
    probe: &RipgrepProbe<'_>,
) {
    ui.separator();
    ui.label("ripgrep executable");
    let mut text = path.display().to_string();
    ui.horizontal(|ui| {
        ui.label("Absolute path");
        if ui.text_edit_singleline(&mut text).changed() {
            *path = PathBuf::from(text.trim());
            invalidate_configured_result(state);
        }
        if ui.button("Browse…").clicked() {
            #[cfg(windows)]
            let dialog = rfd::FileDialog::new().add_filter("Executable", &["exe"]);
            #[cfg(not(windows))]
            let dialog = rfd::FileDialog::new();
            if let Some(selected) = dialog.pick_file() {
                *path = selected.canonicalize().unwrap_or(selected);
                invalidate_configured_result(state);
            }
        }
    });

    ui.horizontal(|ui| {
        if ui.button("Auto-detect").clicked() {
            let context = ExecutableSearchContext::from_process();
            auto_detect_ripgrep(state, &context, probe);
        }
        if let Some(detected) = state
            .automatic_result
            .as_ref()
            .filter(|detected| detected.version.is_some())
        {
            ui.label(format!(
                "Best detected candidate: {}",
                detected.path.display()
            ));
            if detected.path != *path && ui.button("Use detected path").clicked() {
                *path = detected.path.clone();
                invalidate_configured_result(state);
            }
        } else if state.automatic_result.is_some() {
            ui.label("Best detected candidate: not found");
        } else {
            ui.label("Best detected candidate: not run yet");
        }
    });

    ui.horizontal(|ui| {
        if ui.button("Test").clicked() {
            let context = ExecutableSearchContext::from_process();
            test_configured_ripgrep(state, path, &context, probe);
        }
        if cached_configured_result_for_path(state, path).is_some() {
            match cached_configured_result_for_path(state, path) {
                Some(resolution) if resolution.version.is_some() => {
                    ui.colored_label(egui::Color32::GREEN, "Validation: ripgrep is available");
                }
                Some(resolution) if !resolution.warnings.is_empty() => {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!("Validation: {}", resolution.warnings.join("; ")),
                    );
                }
                _ => {
                    ui.colored_label(
                        egui::Color32::RED,
                        "Validation: ripgrep was not found or failed rg --version",
                    );
                }
            }
        } else {
            ui.label("Validation: not tested yet");
        }
    });

    let display_resolution = display_resolution_for_state(state, path);
    let open_folder_path = display_resolution
        .and_then(|resolution| resolution.version.as_ref().map(|_| resolution.path.clone()))
        .and_then(|path| path.parent().map(|parent| parent.to_path_buf()));
    ui.add_enabled_ui(open_folder_path.is_some(), |ui| {
        if ui.button("Open folder").clicked()
            && let Some(folder) = open_folder_path.as_ref()
        {
            let _ = open::that(folder);
        }
    });

    if let Some(resolution) = display_resolution {
        ui.label(format!(
            "Resolution source: {}",
            resolution_source_label(&resolution.source)
        ));
        ui.label(format!(
            "Detected version: {}",
            resolution.version.as_deref().unwrap_or("not available")
        ));
        for warning in &resolution.warnings {
            ui.colored_label(
                egui::Color32::YELLOW,
                format!("Validation warning: {warning}"),
            );
        }
        if resolution.version.is_none() {
            ui.colored_label(
                egui::Color32::RED,
                "Validation error: no usable ripgrep executable resolved",
            );
        }
    } else {
        ui.label("Resolution source: unavailable");
        ui.label("Detected version: not available");
        ui.label("Validation error: not tested yet");
    }
}

fn cached_configured_result_for_path<'a>(
    state: &'a RipgrepSettingsUiState,
    path: &Path,
) -> Option<&'a discovery::RipgrepResolution> {
    (state.last_tested_path.as_deref() == Some(path))
        .then_some(state.configured_result.as_ref())
        .flatten()
}

fn configured_ripgrep_result_is_current(state: &RipgrepSettingsUiState, path: &Path) -> bool {
    cached_configured_result_for_path(state, path).is_some()
}

fn invalidate_configured_result(state: &mut RipgrepSettingsUiState) {
    state.configured_result = None;
    state.last_tested_path = None;
}

fn display_resolution_for_state<'a>(
    state: &'a RipgrepSettingsUiState,
    path: &Path,
) -> Option<&'a discovery::RipgrepResolution> {
    cached_configured_result_for_path(state, path).or(state.automatic_result.as_ref())
}

fn auto_detect_ripgrep(
    state: &mut RipgrepSettingsUiState,
    context: &ExecutableSearchContext,
    probe: &RipgrepProbe<'_>,
) {
    state.automatic_result = probe(Path::new(""), context);
}

fn test_configured_ripgrep(
    state: &mut RipgrepSettingsUiState,
    path: &Path,
    context: &ExecutableSearchContext,
    probe: &RipgrepProbe<'_>,
) {
    let result = if path.as_os_str().is_empty() {
        state.automatic_result.clone().or_else(|| {
            let automatic = probe(Path::new(""), context);
            state.automatic_result = automatic.clone();
            automatic
        })
    } else {
        probe(path, context)
    };
    state.configured_result = result;
    state.last_tested_path = Some(path.to_path_buf());
}

fn resolution_source_label(source: &ExecutableResolutionSource) -> &'static str {
    match source {
        ExecutableResolutionSource::ConfiguredPath => "configured path",
        ExecutableResolutionSource::LauncherSidecar => "launcher sidecar",
        ExecutableResolutionSource::PortableToolsDirectory => "portable tools directory",
        ExecutableResolutionSource::ProcessPath => "process PATH",
    }
}

fn path_field(ui: &mut egui::Ui, label: &str, path: &mut PathBuf) {
    let mut text = path.display().to_string();
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.text_edit_singleline(&mut text).changed() {
            *path = PathBuf::from(text.trim());
        }
    });
}

fn path_list_editor(ui: &mut egui::Ui, label: &str, paths: &mut Vec<PathBuf>) {
    ui.collapsing(label, |ui| {
        let mut remove = None;
        for (idx, path) in paths.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                path_field(ui, "", path);
                if ui.button("Remove").clicked() {
                    remove = Some(idx);
                }
            });
        }
        if let Some(idx) = remove {
            paths.remove(idx);
        }
        if ui.button("Add root").clicked() {
            paths.push(PathBuf::new());
        }
    });
}

fn string_list_editor(ui: &mut egui::Ui, label: &str, items: &mut Vec<String>) {
    ui.collapsing(label, |ui| {
        let mut remove = None;
        for (idx, item) in items.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.text_edit_singleline(item);
                if ui.button("Remove").clicked() {
                    remove = Some(idx);
                }
            });
        }
        if let Some(idx) = remove {
            items.remove(idx);
        }
        if ui.button("Add").clicked() {
            items.push(String::new());
        }
    });
}

fn open_action() -> Action {
    Action {
        label: "Open file search".into(),
        desc: "Open local filename/content search".into(),
        action: OPEN_ACTION.into(),
        args: None,
    }
}

fn mode_action(kind: SearchKind) -> Action {
    Action {
        label: format!("Open file search ({})", kind_label(kind)),
        desc: format!("Open local search with {} mode selected", kind_label(kind)),
        action: format!(
            "{}{}",
            MODE_PREFIX,
            encode_action_payload(&mode_action_payload(kind))
                .expect("file search action payload should serialize")
        ),
        args: None,
    }
}

fn start_action(request: SearchRequestDraft) -> Action {
    let root = request
        .root
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned());
    let payload = start_action_payload(request.kind, root, request.search_text.clone());
    Action {
        label: format!("Start {} search", kind_label(request.kind)),
        desc: request.search_text,
        action: format!(
            "{}{}",
            START_PREFIX,
            encode_action_payload(&payload).expect("file search action payload should serialize")
        ),
        args: None,
    }
}

fn request_directory_action(kind: SearchKind, search_text: String) -> Action {
    let payload = start_action_payload(kind, None, search_text.clone());
    Action {
        label: format!("Choose folder for {} search", kind_label(kind)),
        desc: search_text,
        action: format!(
            "{}{}",
            MODE_PREFIX,
            encode_action_payload(&payload).expect("file search action payload should serialize")
        ),
        args: None,
    }
}

fn error_action(message: String) -> Action {
    Action {
        label: "Invalid file search query".into(),
        desc: message,
        action: OPEN_ACTION.into(),
        args: None,
    }
}

fn command_action(query: &str, desc: &str) -> Action {
    Action {
        label: query.into(),
        desc: desc.into(),
        action: format!("query:{query}"),
        args: None,
    }
}

fn kind_label(kind: SearchKind) -> &'static str {
    match kind {
        SearchKind::Filename => "filename",
        SearchKind::Content => "content",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use serde_json::Value;
    use std::cell::{Cell, RefCell};

    fn plugin() -> FileSearchPlugin {
        FileSearchPlugin::default()
    }

    fn decode_payload(action: &str, prefix: &str) -> Value {
        let encoded = action
            .strip_prefix(prefix)
            .expect("action should have expected prefix");
        let bytes = base64::Engine::decode(&URL_SAFE_NO_PAD, encoded)
            .expect("payload should be URL-safe base64");
        serde_json::from_slice(&bytes).expect("payload should be JSON")
    }

    fn sample_ripgrep_resolution(path: &str) -> crate::file_search::discovery::RipgrepResolution {
        crate::file_search::discovery::RipgrepResolution {
            path: PathBuf::from(path),
            source: ExecutableResolutionSource::ConfiguredPath,
            version: Some("ripgrep 14.1.0".to_owned()),
            warnings: Vec::new(),
        }
    }

    fn deterministic_context() -> ExecutableSearchContext {
        ExecutableSearchContext {
            launcher_directory: PathBuf::from("launcher"),
            path_directories: vec![PathBuf::from("path-entry")],
        }
    }

    #[test]
    fn display_helper_reads_cached_results_without_probing() {
        let call_count = Cell::new(0);
        let probe = |_path: &Path, _context: &ExecutableSearchContext| {
            call_count.set(call_count.get() + 1);
            Some(sample_ripgrep_resolution("unexpected"))
        };
        let state = RipgrepSettingsUiState {
            automatic_result: Some(sample_ripgrep_resolution("auto-rg")),
            configured_result: Some(sample_ripgrep_resolution("configured-rg")),
            last_tested_path: Some(PathBuf::from("configured-rg")),
        };

        let resolution = display_resolution_for_state(&state, Path::new("configured-rg"))
            .expect("cached configured resolution should be displayed");

        assert_eq!(resolution.path, PathBuf::from("configured-rg"));
        assert_eq!(call_count.get(), 0);
        let _ = probe;
    }

    #[test]
    fn auto_detect_action_probes_once_with_empty_path() {
        let call_count = Cell::new(0);
        let probed_paths = RefCell::new(Vec::new());
        let probe = |path: &Path, _context: &ExecutableSearchContext| {
            call_count.set(call_count.get() + 1);
            probed_paths.borrow_mut().push(path.to_path_buf());
            Some(sample_ripgrep_resolution("auto-rg"))
        };
        let mut state = RipgrepSettingsUiState::default();

        auto_detect_ripgrep(&mut state, &deterministic_context(), &probe);

        assert_eq!(call_count.get(), 1);
        assert_eq!(probed_paths.borrow().as_slice(), &[PathBuf::new()]);
        assert_eq!(
            state.automatic_result.as_ref().map(|result| &result.path),
            Some(&PathBuf::from("auto-rg"))
        );
    }

    #[test]
    fn test_action_probes_once_with_configured_path() {
        let call_count = Cell::new(0);
        let probed_paths = RefCell::new(Vec::new());
        let configured_path = PathBuf::from("custom-rg");
        let probe = |path: &Path, _context: &ExecutableSearchContext| {
            call_count.set(call_count.get() + 1);
            probed_paths.borrow_mut().push(path.to_path_buf());
            Some(sample_ripgrep_resolution(&path.to_string_lossy()))
        };
        let mut state = RipgrepSettingsUiState::default();

        test_configured_ripgrep(
            &mut state,
            &configured_path,
            &deterministic_context(),
            &probe,
        );

        assert_eq!(call_count.get(), 1);
        assert_eq!(probed_paths.borrow().as_slice(), &[configured_path.clone()]);
        assert_eq!(state.last_tested_path.as_ref(), Some(&configured_path));
        assert!(cached_configured_result_for_path(&state, &configured_path).is_some());
    }

    #[test]
    fn repeated_display_calls_after_test_action_do_not_probe_again() {
        let call_count = Cell::new(0);
        let configured_path = PathBuf::from("custom-rg");
        let probe = |path: &Path, _context: &ExecutableSearchContext| {
            call_count.set(call_count.get() + 1);
            Some(sample_ripgrep_resolution(&path.to_string_lossy()))
        };
        let mut state = RipgrepSettingsUiState::default();

        test_configured_ripgrep(
            &mut state,
            &configured_path,
            &deterministic_context(),
            &probe,
        );
        assert_eq!(call_count.get(), 1);

        for _ in 0..3 {
            let resolution = display_resolution_for_state(&state, &configured_path)
                .expect("tested configured result should be displayed");
            assert_eq!(resolution.path, configured_path);
        }
        assert_eq!(call_count.get(), 1);
    }

    #[test]
    fn changing_configured_path_invalidates_configured_cached_result() {
        let mut state = RipgrepSettingsUiState {
            configured_result: Some(sample_ripgrep_resolution("rg")),
            last_tested_path: Some(PathBuf::from("rg")),
            ..Default::default()
        };

        invalidate_configured_result(&mut state);

        assert!(cached_configured_result_for_path(&state, Path::new("rg")).is_none());
        assert!(state.configured_result.is_none());
        assert!(state.last_tested_path.is_none());
    }

    #[test]
    fn configured_ripgrep_result_is_valid_only_for_last_tested_path() {
        let mut state = RipgrepSettingsUiState {
            configured_result: Some(sample_ripgrep_resolution("rg")),
            last_tested_path: Some(PathBuf::from("rg")),
            ..Default::default()
        };

        assert!(configured_ripgrep_result_is_current(
            &state,
            Path::new("rg")
        ));
        assert!(!configured_ripgrep_result_is_current(
            &state,
            Path::new("/usr/bin/rg")
        ));

        state.configured_result = None;
        assert!(!configured_ripgrep_result_is_current(
            &state,
            Path::new("rg")
        ));

        state.last_tested_path = None;
        assert!(!configured_ripgrep_result_is_current(
            &state,
            Path::new("rg")
        ));
    }

    #[test]
    fn invalidating_configured_ripgrep_state_clears_test_result_and_path() {
        let mut state = RipgrepSettingsUiState {
            configured_result: Some(sample_ripgrep_resolution("rg")),
            last_tested_path: Some(PathBuf::from("rg")),
            ..Default::default()
        };

        invalidate_configured_result(&mut state);

        assert!(state.configured_result.is_none());
        assert!(state.last_tested_path.is_none());
    }

    #[test]
    fn editing_ripgrep_path_from_rg_to_another_path_clears_configured_test_state() {
        let mut state = RipgrepSettingsUiState {
            configured_result: Some(sample_ripgrep_resolution("rg")),
            last_tested_path: Some(PathBuf::from("rg")),
            ..Default::default()
        };
        let path = PathBuf::from("/custom/bin/rg");
        invalidate_configured_result(&mut state);

        assert_eq!(path, PathBuf::from("/custom/bin/rg"));
        assert!(state.configured_result.is_none());
        assert!(state.last_tested_path.is_none());
        assert!(!configured_ripgrep_result_is_current(&state, &path));
    }

    #[test]
    fn non_fs_queries_return_no_results() {
        assert!(plugin().search("note hello").is_empty());
        assert!(plugin().search("").is_empty());
    }

    #[test]
    fn fs_opens_search_window() {
        let actions = plugin().search("fs");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action, "file_search:open");
    }

    #[test]
    fn mode_commands_open_window_preselected() {
        let file = plugin().search("fs file");
        let file_payload = decode_payload(&file[0].action, "file_search:mode:");
        assert_eq!(file_payload["kind"], "file");

        let content = plugin().search("fs content");
        let content_payload = decode_payload(&content[0].action, "file_search:mode:");
        assert_eq!(content_payload["kind"], "content");
    }

    #[test]
    fn fully_specified_searches_produce_encoded_start_actions() {
        let temp = tempfile::tempdir().unwrap();
        let actions = plugin().search(&format!("fs content needle {}", temp.path().display()));
        assert_eq!(actions.len(), 1);
        assert!(actions[0].action.starts_with("file_search:start:"));

        let payload = decode_payload(&actions[0].action, "file_search:start:");
        assert_eq!(payload["kind"], "content");
        assert_eq!(payload["text"], "needle");
        assert_eq!(
            payload["root"].as_str(),
            Some(temp.path().to_string_lossy().as_ref())
        );
    }

    #[test]
    fn malformed_queries_do_not_emit_unsafe_start_actions() {
        let unterminated = plugin().search("fs file \"unterminated");
        assert_eq!(unterminated.len(), 1);
        assert_eq!(unterminated[0].action, "file_search:open");
        assert!(unterminated[0].label.contains("Invalid"));

        let missing_dir = plugin().search("fs file README ./definitely-not-a-directory");
        assert_eq!(missing_dir.len(), 1);
        assert!(!missing_dir[0].action.starts_with("file_search:start:"));
    }
}
