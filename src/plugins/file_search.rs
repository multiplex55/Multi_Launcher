use crate::actions::Action;
use crate::file_search::actions::{
    encode_action_payload, mode_action_payload, start_action_payload, MODE_PREFIX, OPEN_ACTION,
    START_PREFIX,
};
use crate::file_search::discovery::{self, ExecutableResolutionSource, ExecutableSearchContext};
use crate::file_search::model::SearchKind;
use crate::file_search::query::{FileSearchCommand, SearchRequestDraft};
use crate::file_search::settings::{
    FileSearchDiagnosticsState, FileSearchSettings, DEFAULT_MAX_FULL_PREVIEW_FILE_SIZE_BYTES,
};
use crate::plugin::Plugin;
use eframe::egui;
use std::path::PathBuf;

#[derive(Debug, Clone)]
#[derive(Default)]
pub struct FileSearchPlugin {
    settings: FileSearchSettings,
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
        ripgrep_settings_ui(ui, &mut cfg.ripgrep_executable_path);
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
            ui.monospace(FileSearchDiagnosticsState::from_settings(&cfg).to_string());
        });
        self.settings = cfg.clone();
        if let Ok(v) = serde_json::to_value(&cfg) {
            *value = v;
        }
    }
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

fn ripgrep_settings_ui(ui: &mut egui::Ui, path: &mut PathBuf) {
    ui.separator();
    ui.label("ripgrep executable");
    let context = ExecutableSearchContext::from_process();
    let mut text = path.display().to_string();
    ui.horizontal(|ui| {
        ui.label("Absolute path");
        if ui.text_edit_singleline(&mut text).changed() {
            *path = PathBuf::from(text.trim());
        }
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

    let automatic = discovery::discover_ripgrep(std::path::Path::new(""), &context);
    let configured_resolution = discovery::discover_ripgrep(path, &context);
    let display_resolution = configured_resolution.as_ref().or(automatic.as_ref());

    ui.horizontal(|ui| {
        if ui.button("Auto-detect").clicked() {
            // Detection is intentionally non-mutating; the current automatic result is shown below.
        }
        if let Some(detected) = automatic
            .as_ref()
            .filter(|detected| detected.version.is_some())
        {
            ui.label(format!(
                "Best detected candidate: {}",
                detected.path.display()
            ));
            if detected.path != *path && ui.button("Use detected path").clicked() {
                *path = detected.path.clone();
            }
        } else {
            ui.label("Best detected candidate: not found");
        }
    });

    ui.horizontal(|ui| {
        if ui.button("Test").clicked() {
            // The labels below always reflect testing the entered path first, falling back to auto-discovery when empty.
        }
        let test_resolution = if path.as_os_str().is_empty() {
            automatic.as_ref()
        } else {
            configured_resolution.as_ref()
        };
        match test_resolution {
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
    });

    let open_folder_path = display_resolution
        .and_then(|resolution| resolution.version.as_ref().map(|_| resolution.path.clone()))
        .and_then(|path| path.parent().map(|parent| parent.to_path_buf()));
    ui.add_enabled_ui(open_folder_path.is_some(), |ui| {
        if ui.button("Open folder").clicked()
            && let Some(folder) = open_folder_path.as_ref() {
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
        ui.colored_label(
            egui::Color32::RED,
            "Validation error: no usable ripgrep executable resolved",
        );
    }
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
