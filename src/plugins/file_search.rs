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
use crate::gui::theme::widgets::{StatusTone, semantic_status};
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
    automatic_checked: bool,
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
        ui.label("Max content-search file size (bytes)");
        ui.add(
            egui::DragValue::new(&mut cfg.max_content_search_file_size_bytes)
                .speed(1024.0)
                .range(1..=u64::MAX),
        );
        full_preview_limit_mib_editor(ui, &mut cfg.max_full_preview_file_size_bytes);
        ui.checkbox(&mut cfg.include_hidden_files, "Include hidden by default");
        ui.checkbox(&mut cfg.case_sensitive, "Case-sensitive by default");
        ui.checkbox(
            &mut cfg.everything_enabled,
            "Use Everything ES CLI for global filename search",
        );
        ui.add(
            egui::Label::new(
                "When disabled, global filename searches fall back to WalkDir/ripgrep as applicable.",
            )
            .wrap_mode(egui::TextWrapMode::Wrap),
        );
        path_field(
            ui,
            "Everything ES CLI executable path (es.exe)",
            &mut cfg.everything_executable_path,
        );
        ripgrep_settings_ui(ui, &mut cfg.ripgrep_executable_path, &mut self.ripgrep_ui);
        ui.label("Preferred editor command");
        ui.add(
            egui::TextEdit::singleline(&mut cfg.preferred_editor_command)
                .desired_width(ui.available_width()),
        );
        string_list_editor(ui, "Preferred editor args", &mut cfg.preferred_editor_args);
        ui.label("Preferred terminal command");
        ui.add(
            egui::TextEdit::singleline(&mut cfg.preferred_terminal_command)
                .desired_width(ui.available_width()),
        );
        string_list_editor(
            ui,
            "Preferred terminal args",
            &mut cfg.preferred_terminal_args,
        );
        for diagnostic in cfg.validate() {
            let diagnostic = diagnostic.to_string();
            semantic_status(ui, StatusTone::Warning, &diagnostic, None);
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
    ui.label("Full-file preview limit (MiB)");
    if ui
        .add(egui::DragValue::new(&mut mib).speed(1.0).range(1..=max_mib))
        .changed()
    {
        *bytes = mib
            .max(1)
            .checked_mul(BYTES_PER_MIB)
            .unwrap_or(DEFAULT_MAX_FULL_PREVIEW_FILE_SIZE_BYTES);
    }
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
    ui.label("Absolute path");
    if ui
        .add(egui::TextEdit::singleline(&mut text).desired_width(ui.available_width()))
        .changed()
    {
        *path = PathBuf::from(text.trim());
    }
    ui.horizontal_wrapped(|ui| {
        if ui.button("Browse…").clicked() {
            #[cfg(windows)]
            let dialog = rfd::FileDialog::new().add_filter("Executable", &["exe"]);
            #[cfg(not(windows))]
            let dialog = rfd::FileDialog::new();
            if let Some(selected) = dialog.pick_file() {
                *path = selected.canonicalize().unwrap_or(selected);
            }
        }
    });

    ui.horizontal_wrapped(|ui| {
        if ui.button("Auto-detect").clicked() {
            let context = ExecutableSearchContext::from_process();
            auto_detect_ripgrep(state, &context, probe);
        }
        ui.add(egui::Label::new(auto_detect_label(state)).wrap_mode(egui::TextWrapMode::Wrap));
        if let Some(detected) = state
            .automatic_result
            .as_ref()
            .filter(|detected| ripgrep_resolution_has_valid_path_and_version(detected))
            && detected.path != *path
            && ui.button("Use detected path").clicked()
        {
            *path = detected.path.clone();
            invalidate_configured_result(state);
        }
    });

    if ui.button("Test").clicked() {
        let context = ExecutableSearchContext::from_process();
        test_configured_ripgrep(state, path, &context, probe);
    }
    let label = configured_validation_label(state, path);
    match configured_validation_label_kind(state, path) {
        ValidationLabelKind::Available => {
            semantic_status(ui, StatusTone::Success, &label, None);
        }
        ValidationLabelKind::Warning => {
            semantic_status(ui, StatusTone::Warning, &label, None);
        }
        ValidationLabelKind::Failure => {
            semantic_status(ui, StatusTone::Error, &label, None);
        }
        ValidationLabelKind::Untested => {
            ui.add(egui::Label::new(label).wrap_mode(egui::TextWrapMode::Wrap));
        }
    }

    let display_resolution = display_resolution_for_state(state, path);
    let open_folder_path = display_resolution
        .filter(|resolution| ripgrep_resolution_has_valid_path_and_version(resolution))
        .map(|resolution| resolution.path.clone())
        .and_then(|path| path.parent().map(|parent| parent.to_path_buf()));
    ui.add_enabled_ui(open_folder_path.is_some(), |ui| {
        if ui.button("Open folder").clicked()
            && let Some(folder) = open_folder_path.as_ref()
        {
            let _ = open::that(folder);
        }
    });

    if let Some(resolution) = display_resolution {
        ui.add(
            egui::Label::new(format!(
                "Resolution source: {}",
                resolution_source_label(&resolution.source)
            ))
            .wrap_mode(egui::TextWrapMode::Wrap),
        );
        ui.add(
            egui::Label::new(format!(
                "Detected version: {}",
                resolution.version.as_deref().unwrap_or("not available")
            ))
            .wrap_mode(egui::TextWrapMode::Wrap),
        );
        for warning in &resolution.warnings {
            semantic_status(ui, StatusTone::Warning, "Validation warning", Some(warning));
        }
        if resolution.version.is_none() {
            semantic_status(
                ui,
                StatusTone::Error,
                "Validation error: no usable ripgrep executable resolved",
                None,
            );
        }
    } else {
        ui.label("Resolution source: unavailable");
        ui.label("Detected version: not available");
        ui.add(
            egui::Label::new("Validation error: not tested yet")
                .wrap_mode(egui::TextWrapMode::Wrap),
        );
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

fn ripgrep_resolution_has_valid_path_and_version(
    resolution: &discovery::RipgrepResolution,
) -> bool {
    !resolution.path.as_os_str().is_empty() && resolution.version.is_some()
}

fn auto_detect_label(state: &RipgrepSettingsUiState) -> String {
    match (state.automatic_checked, state.automatic_result.as_ref()) {
        (false, _) => "Best detected candidate: not checked".to_owned(),
        (true, Some(resolution)) if ripgrep_resolution_has_valid_path_and_version(resolution) => {
            format!(
                "Best detected candidate: {} ({})",
                resolution.path.display(),
                resolution
                    .version
                    .as_deref()
                    .unwrap_or("version not available")
            )
        }
        (true, Some(resolution)) if !resolution.path.as_os_str().is_empty() => {
            format!("Best detected candidate: {}", resolution.path.display())
        }
        (true, _) => "Best detected candidate: not found".to_owned(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValidationLabelKind {
    Available,
    Warning,
    Failure,
    Untested,
}

fn configured_validation_label_kind(
    state: &RipgrepSettingsUiState,
    path: &Path,
) -> ValidationLabelKind {
    match cached_configured_result_for_path(state, path) {
        Some(resolution) if !resolution.warnings.is_empty() => ValidationLabelKind::Warning,
        Some(resolution) if ripgrep_resolution_has_valid_path_and_version(resolution) => {
            ValidationLabelKind::Available
        }
        Some(_) => ValidationLabelKind::Failure,
        None => ValidationLabelKind::Untested,
    }
}

fn configured_validation_label(state: &RipgrepSettingsUiState, path: &Path) -> String {
    match cached_configured_result_for_path(state, path) {
        Some(resolution) if !resolution.warnings.is_empty() => {
            format!("Validation: {}", resolution.warnings.join("; "))
        }
        Some(resolution) if ripgrep_resolution_has_valid_path_and_version(resolution) => {
            "Validation: ripgrep is available".to_owned()
        }
        Some(_) => "Validation: ripgrep was not found or failed rg --version".to_owned(),
        None if state.configured_result.is_some() || state.last_tested_path.is_some() => {
            "Validation: not tested for this path".to_owned()
        }
        None => "Validation: not tested".to_owned(),
    }
}

fn auto_detect_ripgrep(
    state: &mut RipgrepSettingsUiState,
    context: &ExecutableSearchContext,
    probe: &RipgrepProbe<'_>,
) {
    state.automatic_checked = true;
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
    if !label.is_empty() {
        ui.add(egui::Label::new(label).wrap_mode(egui::TextWrapMode::Wrap));
    }
    if ui
        .add(egui::TextEdit::singleline(&mut text).desired_width(ui.available_width()))
        .changed()
    {
        *path = PathBuf::from(text.trim());
    }
}

fn path_list_editor(ui: &mut egui::Ui, label: &str, paths: &mut Vec<PathBuf>) {
    ui.collapsing(label, |ui| {
        let mut remove = None;
        for (idx, path) in paths.iter_mut().enumerate() {
            path_field(ui, "", path);
            ui.horizontal_wrapped(|ui| {
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
            ui.add(egui::TextEdit::singleline(item).desired_width(ui.available_width()));
            ui.horizontal_wrapped(|ui| {
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
    fn auto_detect_label_tracks_not_checked_success_and_failure_states() {
        let mut state = RipgrepSettingsUiState::default();
        assert_eq!(
            auto_detect_label(&state),
            "Best detected candidate: not checked"
        );

        state.automatic_checked = true;
        state.automatic_result = Some(sample_ripgrep_resolution("auto-rg"));
        assert_eq!(
            auto_detect_label(&state),
            "Best detected candidate: auto-rg (ripgrep 14.1.0)"
        );

        state.automatic_result = None;
        assert_eq!(
            auto_detect_label(&state),
            "Best detected candidate: not found"
        );
    }

    #[test]
    fn configured_validation_label_tracks_current_stale_warning_and_failure_states() {
        let mut state = RipgrepSettingsUiState::default();
        assert_eq!(
            configured_validation_label(&state, Path::new("rg")),
            "Validation: not tested"
        );

        state.configured_result = Some(sample_ripgrep_resolution("rg"));
        state.last_tested_path = Some(PathBuf::from("rg"));
        assert_eq!(
            configured_validation_label(&state, Path::new("rg")),
            "Validation: ripgrep is available"
        );
        assert_eq!(
            configured_validation_label(&state, Path::new("other-rg")),
            "Validation: not tested for this path"
        );

        let mut warning = sample_ripgrep_resolution("rg");
        warning.version = None;
        warning.warnings = vec!["rg --version exited with status 1".to_owned()];
        state.configured_result = Some(warning);
        assert_eq!(
            configured_validation_label(&state, Path::new("rg")),
            "Validation: rg --version exited with status 1"
        );

        let mut failure = sample_ripgrep_resolution("rg");
        failure.version = None;
        state.configured_result = Some(failure);
        assert_eq!(
            configured_validation_label(&state, Path::new("rg")),
            "Validation: ripgrep was not found or failed rg --version"
        );
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
            automatic_checked: true,
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
        assert_eq!(
            probed_paths.borrow().as_slice(),
            std::slice::from_ref(&configured_path)
        );
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

    fn accesskit_bounds(
        output: &egui::FullOutput,
        role: egui::accesskit::Role,
        expected_label: Option<&str>,
        expected_value: Option<&str>,
    ) -> Option<egui::Rect> {
        output
            .platform_output
            .accesskit_update
            .as_ref()?
            .nodes
            .iter()
            .find_map(|(_, node)| {
                let label_matches = expected_label
                    .map(|expected| node.label() == Some(expected))
                    .unwrap_or(true);
                let value_matches = expected_value
                    .map(|expected| node.value() == Some(expected))
                    .unwrap_or(true);
                if node.role() != role || !label_matches || !value_matches {
                    return None;
                }
                let bounds = node.bounds()?;
                Some(egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                ))
            })
    }

    #[test]
    fn settings_reflow_chrome_ripgrep_form_stays_bounded_without_probing_or_editing_state() {
        let long_path = format!(
            "C:\\{}\\configured-rg.exe",
            (0..7)
                .map(|index| format!("search-tool-directory-{index:02}-{}", "x".repeat(13)))
                .collect::<Vec<_>>()
                .join("\\")
        );
        let detected_path = format!(
            "C:\\{}\\detected-rg.exe",
            (0..7)
                .map(|index| format!("search-tool-directory-{index:02}-{}", "y".repeat(13)))
                .collect::<Vec<_>>()
                .join("\\")
        );
        let configured_path = PathBuf::from(&long_path);
        let detected = sample_ripgrep_resolution(&detected_path);
        let mut path = configured_path.clone();
        let mut state = RipgrepSettingsUiState {
            automatic_result: Some(detected),
            automatic_checked: true,
            configured_result: Some(sample_ripgrep_resolution(&long_path)),
            last_tested_path: Some(configured_path.clone()),
        };
        let path_before = path.clone();
        let state_before = state.clone();
        let probe_calls = Cell::new(0);
        let probe = |_path: &Path, _context: &ExecutableSearchContext| {
            probe_calls.set(probe_calls.get() + 1);
            None
        };

        let context = egui::Context::default();
        context.enable_accesskit();
        context.style_mut_of(context.theme(), |style| style.animation_time = 0.0);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 720.0));
        let expected_candidate = auto_detect_label(&state);
        let mut observation = None;
        for _ in 0..3 {
            let mut viewport = egui::Rect::NOTHING;
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    focused: true,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        viewport = ui.clip_rect();
                        ripgrep_settings_ui_with_probe(ui, &mut path, &mut state, &probe);
                    });
                },
            );
            let path_bounds = accesskit_bounds(
                &output,
                egui::accesskit::Role::TextInput,
                None,
                Some(&long_path),
            );
            let button_bounds = [
                "Browse…",
                "Auto-detect",
                "Use detected path",
                "Test",
                "Open folder",
            ]
            .map(|label| {
                accesskit_bounds(&output, egui::accesskit::Role::Button, Some(label), None)
            });
            let candidate_bounds = accesskit_bounds(
                &output,
                egui::accesskit::Role::Label,
                None,
                Some(&expected_candidate),
            );
            observation = Some((
                viewport,
                path_bounds,
                button_bounds,
                candidate_bounds,
                path.clone(),
                state.clone(),
            ));
            output.drop_without_applying_deltas();
        }

        let (viewport, path_bounds, button_bounds, candidate_bounds, path_after, state_after) =
            observation.expect("the settled ripgrep form reports its rendered observations");
        assert!(path_bounds.is_some_and(|bounds| viewport.contains_rect(bounds)));
        assert!(
            button_bounds
                .iter()
                .all(|bounds| bounds.is_some_and(|bounds| viewport.contains_rect(bounds)))
        );
        assert!(candidate_bounds.is_some_and(|bounds| viewport.contains_rect(bounds)));
        assert_eq!(
            probe_calls.get(),
            0,
            "rendering does not invoke the injected probe"
        );
        assert_eq!(
            path_after, path_before,
            "rendering does not edit the configured path"
        );
        assert_eq!(
            state_after.automatic_result, state_before.automatic_result,
            "rendering does not edit the automatic probe result"
        );
        assert_eq!(
            state_after.automatic_checked, state_before.automatic_checked,
            "rendering does not alter the automatic probe state"
        );
        assert_eq!(
            state_after.configured_result, state_before.configured_result,
            "rendering does not edit the configured probe result"
        );
        assert_eq!(
            state_after.last_tested_path, state_before.last_tested_path,
            "rendering does not change the tested path"
        );
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
