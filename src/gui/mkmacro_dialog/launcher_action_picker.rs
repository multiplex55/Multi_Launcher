//! Searchable launcher-action picker and its transactional conversion helpers.
use crate::{
    actions::Action,
    mkmacro::{MkAction, MkLauncherCommandPayload, MkProcessPayload},
};
use eframe::egui;
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerPurpose {
    Process,
    LauncherCommand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LauncherActionRequest {
    pub purpose: PickerPurpose,
    pub macro_id: u64,
    pub step_id: Option<u64>,
    pub draft_generation: u64,
}

#[derive(Default)]
pub struct LauncherActionPickerState {
    pub open: bool,
    pub search: String,
    pub selected: Option<usize>,
    pub request: Option<LauncherActionRequest>,
    pub conversion_confirmation: Option<Action>,
}

impl LauncherActionPickerState {
    pub fn open(&mut self, request: LauncherActionRequest) {
        self.open = true;
        self.search.clear();
        self.selected = None;
        self.conversion_confirmation = None;
        self.request = Some(request);
    }
    pub fn cancel(&mut self) {
        self.open = false;
        self.search.clear();
        self.selected = None;
        self.request = None;
        self.conversion_confirmation = None;
    }
}

/// Results ranked with the launcher's fuzzy matcher; source position is the
/// stable tie breaker.
pub fn matching_action_indices(actions: &[Action], query: &str) -> Vec<usize> {
    let needle = query.trim();
    if needle.is_empty() {
        return (0..actions.len()).collect();
    }
    let matcher = SkimMatcherV2::default().ignore_case();
    let mut found: Vec<(usize, i64)> = actions
        .iter()
        .enumerate()
        .filter_map(|(i, a)| {
            [
                &a.label,
                &a.desc,
                &a.action,
                a.args.as_deref().unwrap_or(""),
            ]
            .into_iter()
            .filter_map(|v| matcher.fuzzy_match(v, needle))
            .max()
            .map(|score| (i, score))
        })
        .collect();
    found.sort_by(|(ai, ascore), (bi, bscore)| bscore.cmp(ascore).then(ai.cmp(bi)));
    found.into_iter().map(|(i, _)| i).collect()
}

/// Conservative recognition of launcher values that can be run directly.
pub fn is_direct_program(action: &str) -> bool {
    let value = action.trim();
    if value.is_empty()
        || value.starts_with("mm ")
        || value.starts_with("system:")
        || value.starts_with("notes:")
    {
        return false;
    }
    // Other canonical namespaces are launcher commands, except a Windows drive.
    if value.contains(':')
        && !(value.len() >= 3
            && value.as_bytes()[1] == b':'
            && matches!(value.as_bytes()[2], b'\\' | b'/'))
    {
        return false;
    }
    let lower = value.to_ascii_lowercase();
    let known = [".exe", ".bat", ".cmd", ".com"]
        .iter()
        .any(|e| lower.ends_with(e));
    known || value.contains('/') || value.contains('\\')
}

fn inferred_parent(program: &str) -> Option<String> {
    Path::new(program)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_string_lossy().into_owned())
}

pub fn apply_chosen_program_path(payload: &mut MkProcessPayload, path: PathBuf) {
    let program = path.to_string_lossy().into_owned();
    if payload
        .working_directory
        .as_deref()
        .is_none_or(|v| v.trim().is_empty())
    {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            payload.working_directory = Some(parent.to_string_lossy().into_owned());
        }
    }
    payload.program = program;
}

pub fn apply_chosen_folder(payload: &mut MkProcessPayload, path: PathBuf) {
    payload.working_directory = Some(path.to_string_lossy().into_owned());
}

pub fn apply_program_action(payload: &mut MkProcessPayload, action: &Action) {
    payload.program = action.action.clone();
    payload.arguments = action
        .args
        .as_deref()
        .map(|v| shlex::split(v).unwrap_or_else(|| vec![v.to_owned()]))
        .unwrap_or_default();
    if payload
        .working_directory
        .as_deref()
        .is_none_or(|v| v.trim().is_empty())
    {
        if let Some(parent) = inferred_parent(&action.action) {
            payload.working_directory = Some(parent);
        }
    }
}

/// A launcher action that can be represented without persisting a resolved
/// target. Everything else is deliberately absent from the command picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LauncherQuerySuggestion {
    Query(String),
    Application(String),
}

pub fn launcher_query_suggestion(action: &Action) -> Option<LauncherQuerySuggestion> {
    if let Some(query) = action.action.strip_prefix("query:") {
        return Some(LauncherQuerySuggestion::Query(
            crate::mkmacro::store::query_action_text(query, action.args.as_deref()),
        ));
    }

    // A resolved application is useful only as a normal Launcher search when
    // the immutable authoring snapshot supplied an actual display name.
    let label = action.label.trim();
    if is_direct_program(&action.action)
        && !action.action.starts_with("http://")
        && !action.action.starts_with("https://")
        && !label.is_empty()
        && label != action.action.trim()
    {
        return Some(LauncherQuerySuggestion::Application(label.to_owned()));
    }
    None
}

fn set_launcher_query(target: &mut MkAction, query: String) {
    *target = MkAction::LauncherCommand(MkLauncherCommandPayload {
        query,
        legacy_resolved_action: None,
    });
}

impl super::action_editor::ActionEditorState {
    pub fn launcher_picker_request(
        &self,
        purpose: PickerPurpose,
        macro_id: u64,
    ) -> LauncherActionRequest {
        LauncherActionRequest {
            purpose,
            macro_id,
            step_id: self.editing_id,
            draft_generation: self.draft_generation,
        }
    }
    pub fn apply_launcher_picker_action(
        &mut self,
        request: &LauncherActionRequest,
        action: &Action,
        current_macro_id: Option<u64>,
        convert: bool,
    ) -> bool {
        if Some(request.macro_id) != current_macro_id
            || request.step_id != self.editing_id
            || request.draft_generation != self.draft_generation
        {
            return false;
        }
        let Some(step) = self.draft.as_mut() else {
            return false;
        };
        match request.purpose {
            PickerPurpose::Process if is_direct_program(&action.action) => {
                let MkAction::Process(payload) = &mut step.action else {
                    return false;
                };
                apply_program_action(payload, action);
            }
            PickerPurpose::Process if convert => {
                let Some(suggestion) = launcher_query_suggestion(action) else {
                    return false;
                };
                let query = match suggestion {
                    LauncherQuerySuggestion::Query(query)
                    | LauncherQuerySuggestion::Application(query) => query,
                };
                set_launcher_query(&mut step.action, query);
                self.editor = Some(super::action_catalog::EditorKind::Launcher);
            }
            PickerPurpose::Process => return false,
            PickerPurpose::LauncherCommand => {
                if !matches!(step.action, MkAction::LauncherCommand(_)) {
                    return false;
                }
                let Some(suggestion) = launcher_query_suggestion(action) else {
                    return false;
                };
                let query = match suggestion {
                    LauncherQuerySuggestion::Query(query)
                    | LauncherQuerySuggestion::Application(query) => query,
                };
                set_launcher_query(&mut step.action, query);
            }
        }
        true
    }
}

pub fn show(ctx: &egui::Context, dialog: &mut super::MkMacroDialog) {
    if !dialog.launcher_action_picker.open {
        return;
    }
    let actions = dialog.authoring_context.launcher_actions.clone();
    let mut open = true;
    let mut choose = false;
    let mut cancel = false;
    let purpose = dialog
        .launcher_action_picker
        .request
        .as_ref()
        .map(|r| r.purpose);
    egui::Window::new(match purpose {
        Some(PickerPurpose::LauncherCommand) => "Choose Launcher Command Suggestion",
        _ => "Choose Program or Launcher Query",
    })
    .collapsible(false)
    .open(&mut open)
    .default_width(680.0)
    .show(ctx, |ui| {
        let state = &mut dialog.launcher_action_picker;
        let response = ui.add(
            egui::TextEdit::singleline(&mut state.search)
                .hint_text("Search command/query suggestions"),
        );
        response.request_focus();
        let eligible: Vec<usize> = actions
            .iter()
            .enumerate()
            .filter(|(_, action)| {
                purpose != Some(PickerPurpose::LauncherCommand)
                    || launcher_query_suggestion(action).is_some()
            })
            .map(|(index, _)| index)
            .collect();
        let ranked = matching_action_indices(&actions, &state.search);
        let visible: Vec<usize> = ranked
            .into_iter()
            .filter(|index| eligible.contains(index))
            .collect();
        if eligible.is_empty() {
            ui.label("No safe Launcher command/query suggestions are available.");
        } else if visible.is_empty() {
            ui.label("No Launcher command/query suggestions match this search.");
        }
        egui::ScrollArea::vertical()
            .max_height(320.0)
            .show(ui, |ui| {
                for &index in &visible {
                    let a = &actions[index];
                    let detail = match purpose {
                        Some(PickerPurpose::LauncherCommand) => launcher_query_suggestion(a)
                            .map(|suggestion| match suggestion {
                                LauncherQuerySuggestion::Query(query)
                                | LauncherQuerySuggestion::Application(query) => {
                                    format!("Query suggestion: {query}")
                                }
                            })
                            .unwrap_or_default(),
                        _ => format!(
                            "Program/action: {}{}",
                            a.action,
                            a.args
                                .as_ref()
                                .map(|v| format!("  {v}"))
                                .unwrap_or_default()
                        ),
                    };
                    let text = format!("{}\n{}\n{}", a.label, a.desc, detail);
                    let row = ui.selectable_label(state.selected == Some(index), text);
                    if row.clicked() {
                        state.selected = Some(index);
                    }
                    if row.double_clicked() {
                        state.selected = Some(index);
                        choose = true;
                    }
                }
            });
        let down = ui.input(|i| i.key_pressed(egui::Key::ArrowDown));
        let up = ui.input(|i| i.key_pressed(egui::Key::ArrowUp));
        if (down || up) && !visible.is_empty() {
            let pos = state
                .selected
                .and_then(|s| visible.iter().position(|i| *i == s));
            let next = if down {
                pos.map_or(0, |p| (p + 1).min(visible.len() - 1))
            } else {
                pos.map_or(visible.len() - 1, |p| p.saturating_sub(1))
            };
            state.selected = Some(visible[next]);
        }
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) && state.selected.is_some() {
            choose = true;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    state.selected.is_some(),
                    egui::Button::new(match purpose {
                        Some(PickerPurpose::LauncherCommand) => "Use Query Suggestion",
                        _ => "Use Selected Program",
                    }),
                )
                .clicked()
            {
                choose = true;
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
        if let Some(action) = state.conversion_confirmation.clone() {
            ui.separator();
            ui.label("This choice can be stored as a safe Launcher query instead of a program.");
            if ui.button("Use Launcher Query instead").clicked() {
                if let Some(request) = state.request.clone() {
                    dialog.action_editor.apply_launcher_picker_action(
                        &request,
                        &action,
                        dialog.selected_macro_id,
                        true,
                    );
                }
                cancel = true;
            }
            if ui.button("Keep Run Program").clicked() {
                state.conversion_confirmation = None;
            }
        }
    });
    if !open || cancel {
        dialog.launcher_action_picker.cancel();
        return;
    }
    if choose {
        let state = &mut dialog.launcher_action_picker;
        if let (Some(index), Some(request)) = (state.selected, state.request.clone()) {
            if let Some(action) = actions.get(index) {
                if request.purpose == PickerPurpose::Process && !is_direct_program(&action.action) {
                    if launcher_query_suggestion(action).is_some() {
                        state.conversion_confirmation = Some(action.clone());
                    }
                } else {
                    dialog.action_editor.apply_launcher_picker_action(
                        &request,
                        action,
                        dialog.selected_macro_id,
                        false,
                    );
                    state.cancel();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn action(label: &str, desc: &str, command: &str, args: Option<&str>) -> Action {
        Action {
            label: label.into(),
            desc: desc.into(),
            action: command.into(),
            args: args.map(str::to_owned),
        }
    }

    #[test]
    fn matching_covers_every_field_case_insensitively_and_is_stable() {
        let items = vec![
            action("Alpha", "Editor", "one.exe", Some("--FIRST")),
            action("Beta", "ALPHA helper", "two.exe", None),
        ];
        assert_eq!(matching_action_indices(&items, " alpha "), vec![0, 1]);
        assert_eq!(matching_action_indices(&items, "editor"), vec![0]);
        assert_eq!(matching_action_indices(&items, "ONE.EXE"), vec![0]);
        assert_eq!(matching_action_indices(&items, "first"), vec![0]);
        assert_eq!(matching_action_indices(&items, ""), vec![0, 1]);
    }

    #[test]
    fn program_classifier_is_conservative() {
        for value in [
            r"C:\Tools\app.exe",
            "wt.exe",
            "build.bat",
            "go.cmd",
            "old.com",
            "/usr/local/bin/tool",
            "scripts/run",
        ] {
            assert!(is_direct_program(value), "{value}");
        }
        for value in [
            "notes:dialog",
            "system:shutdown",
            "mm open settings",
            "ambiguous",
        ] {
            assert!(!is_direct_program(value), "{value}");
        }
    }

    #[test]
    fn process_transfer_parses_quotes_and_respects_working_directory() {
        let selected = action(
            "Display",
            "metadata",
            "/opt/tools/run",
            Some("--name \"two words\""),
        );
        let mut payload = MkProcessPayload {
            program: String::new(),
            arguments: vec![],
            working_directory: None,
            wait: false,
        };
        apply_program_action(&mut payload, &selected);
        assert_eq!(payload.program, "/opt/tools/run");
        assert_eq!(payload.arguments, ["--name", "two words"]);
        assert_eq!(payload.working_directory.as_deref(), Some("/opt/tools"));
        payload.working_directory = Some("explicit".into());
        apply_program_action(&mut payload, &action("", "", "/elsewhere/run", None));
        assert_eq!(payload.working_directory.as_deref(), Some("explicit"));
    }

    #[test]
    fn chosen_paths_infer_only_meaningful_parents() {
        let mut payload = MkProcessPayload {
            program: String::new(),
            arguments: vec![],
            working_directory: Some("  ".into()),
            wait: false,
        };
        apply_chosen_program_path(&mut payload, PathBuf::from("/tmp/bin/tool"));
        assert_eq!(payload.working_directory.as_deref(), Some("/tmp/bin"));
        payload.working_directory = None;
        apply_chosen_program_path(&mut payload, PathBuf::from("tool"));
        assert_eq!(payload.working_directory, None);
    }

    #[test]
    fn query_actions_use_the_launcher_argument_contract() {
        for (command, args, expected) in [
            ("query:bm list", None, "bm list"),
            ("query:f list", None, "f list"),
            (
                "query:note  open ${name}",
                Some(r#"{"query":"  --exact ${name}"}"#),
                "note  open ${name} --exact ${name}",
            ),
            ("query:f list", Some("   "), "f list"),
        ] {
            let selected = action("Suggestion", "", command, args);
            assert_eq!(
                launcher_query_suggestion(&selected),
                Some(LauncherQuerySuggestion::Query(expected.into()))
            );
        }
    }

    #[test]
    fn unsafe_resolved_targets_are_not_raw_queries() {
        for command in ["notes:dialog", "https://example.test", "/usr/bin/tool"] {
            let selected = action(command, "", command, None);
            assert_eq!(launcher_query_suggestion(&selected), None, "{command}");
        }
    }

    #[test]
    fn application_uses_searchable_display_name() {
        let selected = action("Firefox", "Web browser", "/usr/bin/firefox", None);
        assert_eq!(
            launcher_query_suggestion(&selected),
            Some(LauncherQuerySuggestion::Application("Firefox".into()))
        );
    }

    #[test]
    fn launcher_query_payload_clears_legacy_data_and_serializes_only_v8_data() {
        let mut target = MkAction::LauncherCommand(MkLauncherCommandPayload {
            query: "old".into(),
            legacy_resolved_action: Some(action(
                "Secret label",
                "Secret description",
                "notes:dialog",
                None,
            )),
        });
        set_launcher_query(&mut target, "bm list".into());
        assert_eq!(
            target,
            MkAction::LauncherCommand(MkLauncherCommandPayload {
                query: "bm list".into(),
                legacy_resolved_action: None,
            })
        );
        let json = serde_json::to_string(&target).unwrap();
        assert!(!json.contains("Secret"));
        assert!(!json.contains("legacy_resolved_action"));
        assert_eq!(serde_json::from_str::<MkAction>(&json).unwrap(), target);
    }

    #[test]
    fn stale_picker_request_does_not_mutate_draft() {
        let overlay = super::super::visual_capture_workflow::SharedVisualOverlayController::new(
            super::super::visual_overlay::VisualOverlayController::default(),
        );
        let mut editor = super::super::action_editor::ActionEditorState::new(overlay);
        editor.editing_id = Some(7);
        editor.draft = Some(crate::mkmacro::MkStep {
            id: 7,
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            action: MkAction::LauncherCommand(MkLauncherCommandPayload {
                query: "unchanged".into(),
                legacy_resolved_action: None,
            }),
        });
        let before = editor.draft.clone();
        let valid = LauncherActionRequest {
            purpose: PickerPurpose::LauncherCommand,
            macro_id: 12,
            step_id: Some(7),
            draft_generation: 0,
        };
        for request in [
            LauncherActionRequest {
                macro_id: 13,
                ..valid.clone()
            },
            LauncherActionRequest {
                step_id: Some(8),
                ..valid.clone()
            },
            LauncherActionRequest {
                draft_generation: 1,
                ..valid.clone()
            },
        ] {
            assert!(!editor.apply_launcher_picker_action(
                &request,
                &action("Bookmarks", "", "query:bm list", None),
                Some(12),
                false,
            ));
            assert_eq!(editor.draft, before);
        }
    }
}
