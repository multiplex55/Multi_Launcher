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

#[cfg(test)]
#[derive(Clone)]
struct LauncherPickerChromeObservation {
    body_id: egui::Id,
    body_rect: egui::Rect,
    content_size: egui::Vec2,
    scroll_offset: egui::Vec2,
    last_result: Option<(String, egui::Id, egui::Rect)>,
    footer_clip: egui::Rect,
    footer_buttons: Vec<(String, egui::Id, egui::Rect)>,
    confirmation_label: Option<(egui::Id, egui::Rect)>,
}

#[derive(Default)]
pub struct LauncherActionPickerState {
    pub open: bool,
    pub search: String,
    pub selected: Option<usize>,
    pub request: Option<LauncherActionRequest>,
    pub conversion_confirmation: Option<Action>,
    #[cfg(test)]
    test_chrome: Option<LauncherPickerChromeObservation>,
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
        && let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty())
    {
        payload.working_directory = Some(parent.to_string_lossy().into_owned());
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
        && let Some(parent) = inferred_parent(&action.action)
    {
        payload.working_directory = Some(parent);
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
        let use_label = match purpose {
            Some(PickerPurpose::LauncherCommand) => "Use Query Suggestion",
            _ => "Use Selected Program",
        };
        let has_conversion_confirmation = state.conversion_confirmation.is_some();
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
        let footer_height = launcher_action_picker_footer_reserved_height(
            ui,
            use_label,
            has_conversion_confirmation,
        );
        let body_max_height = (ui.available_height() - footer_height).max(0.0);
        #[cfg(test)]
        let mut last_result = None;
        let body_output = egui::ScrollArea::vertical()
            .id_salt("mkmacro_launcher_action_picker_results")
            .auto_shrink([false, false])
            .min_scrolled_height(0.0)
            .max_height(body_max_height)
            .show(ui, |ui| {
                for (_visible_index, &index) in visible.iter().enumerate() {
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
                    #[cfg(test)]
                    let last_text = (_visible_index + 1 == visible.len()).then(|| text.clone());
                    let row = ui.selectable_label(state.selected == Some(index), text);
                    if row.clicked() {
                        state.selected = Some(index);
                    }
                    if row.double_clicked() {
                        state.selected = Some(index);
                        choose = true;
                    }
                    #[cfg(test)]
                    if let Some(text) = last_text {
                        last_result = Some((text, row.id, row.rect));
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
        #[cfg(test)]
        let footer_clip = ui.clip_rect();
        #[cfg(test)]
        let mut footer_buttons = Vec::new();
        #[cfg(test)]
        let mut confirmation_label = None;
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            let response = ui.add_enabled(state.selected.is_some(), egui::Button::new(use_label));
            #[cfg(test)]
            footer_buttons.push((use_label.to_owned(), response.id, response.rect));
            if response.clicked() {
                choose = true;
            }
            let response = ui.button("Cancel");
            #[cfg(test)]
            footer_buttons.push(("Cancel".to_owned(), response.id, response.rect));
            if response.clicked() {
                cancel = true;
            }
        });
        if let Some(action) = state.conversion_confirmation.clone() {
            ui.separator();
            let _response = ui
                .label("This choice can be stored as a safe Launcher query instead of a program.");
            #[cfg(test)]
            {
                confirmation_label = Some((_response.id, _response.rect));
            }
            ui.horizontal_wrapped(|ui| {
                let response = ui.button("Use Launcher Query instead");
                #[cfg(test)]
                footer_buttons.push((
                    "Use Launcher Query instead".to_owned(),
                    response.id,
                    response.rect,
                ));
                if response.clicked() {
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
                let response = ui.button("Keep Run Program");
                #[cfg(test)]
                footer_buttons.push(("Keep Run Program".to_owned(), response.id, response.rect));
                if response.clicked() {
                    state.conversion_confirmation = None;
                }
            });
        }
        #[cfg(test)]
        {
            state.test_chrome = Some(LauncherPickerChromeObservation {
                body_id: body_output.id,
                body_rect: body_output.inner_rect,
                content_size: body_output.content_size,
                scroll_offset: body_output.state.offset,
                last_result,
                footer_clip,
                footer_buttons,
                confirmation_label,
            });
        }
        #[cfg(not(test))]
        let _ = (body_output, footer_height);
    });
    if !open || cancel {
        dialog.launcher_action_picker.cancel();
        return;
    }
    if choose {
        let state = &mut dialog.launcher_action_picker;
        if let (Some(index), Some(request)) = (state.selected, state.request.clone())
            && let Some(action) = actions.get(index)
        {
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

fn launcher_action_picker_button_group_height(
    ui: &mut egui::Ui,
    labels: &[&str],
    button_font: egui::FontId,
    frame_margin: egui::Vec2,
    available_width: f32,
) -> f32 {
    let spacing = ui.spacing().clone();
    let text_width = (available_width - frame_margin.x).max(1.0);
    let wrap_mode = ui.wrap_mode();
    let mut row_width = 0.0;
    let mut row_height = 0.0_f32;
    let mut total_height = 0.0;
    for label in labels {
        let text_size = egui::WidgetText::from(*label)
            .into_galley(ui, Some(wrap_mode), text_width, button_font.clone())
            .size();
        let button_width = (text_size.x + frame_margin.x)
            .max(spacing.interact_size.x)
            .min(available_width);
        let button_height = (text_size.y + spacing.extra_text_line_spacing + frame_margin.y)
            .max(spacing.interact_size.y);
        if row_width > 0.0 && row_width + spacing.item_spacing.x + button_width > available_width {
            total_height += row_height + spacing.item_spacing.y;
            row_width = button_width;
            row_height = button_height;
        } else {
            if row_width > 0.0 {
                row_width += spacing.item_spacing.x;
            }
            row_width += button_width;
            row_height = row_height.max(button_height);
        }
    }
    total_height + row_height.max(spacing.interact_size.y)
}

fn launcher_action_picker_footer_reserved_height(
    ui: &mut egui::Ui,
    use_label: &str,
    has_conversion_confirmation: bool,
) -> f32 {
    let spacing = ui.spacing().clone();
    let style = ui.style().clone();
    let native_button_style = style.button_style(
        &egui::widget_style::Classes::default(),
        egui::widget_style::WidgetState::Inactive,
    );
    let button_font = style
        .override_font_id
        .clone()
        .or_else(|| {
            style
                .override_text_style
                .as_ref()
                .map(|text_style| text_style.resolve(&style))
        })
        .unwrap_or_else(|| native_button_style.text_style.font_id.clone());
    let frame_margin = if ui.visuals().button_frame {
        native_button_style.frame.total_margin().sum()
    } else {
        egui::Vec2::ZERO
    };
    let available_width = ui.available_width().max(1.0);
    let wrap_mode = ui.wrap_mode();
    let pixels_per_point = ui.ctx().pixels_per_point().max(f32::EPSILON);
    let separator_spacing = style
        .separator_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Noninteractive,
        )
        .spacing;
    let mut footer_height = launcher_action_picker_button_group_height(
        ui,
        &[use_label, "Cancel"],
        button_font.clone(),
        frame_margin,
        available_width,
    ) + separator_spacing
        + 2.0 * spacing.item_spacing.y;

    if has_conversion_confirmation {
        let label_font = style
            .override_font_id
            .clone()
            .or_else(|| {
                style
                    .override_text_style
                    .as_ref()
                    .map(|text_style| text_style.resolve(&style))
            })
            .unwrap_or_else(|| style.text_styles[&egui::TextStyle::Body].clone());
        let confirmation_height = egui::WidgetText::from(
            "This choice can be stored as a safe Launcher query instead of a program.",
        )
        .into_galley(ui, Some(wrap_mode), available_width, label_font)
        .size()
        .y;
        footer_height += separator_spacing
            + 3.0 * spacing.item_spacing.y
            + confirmation_height
            + launcher_action_picker_button_group_height(
                ui,
                &["Use Launcher Query instead", "Keep Run Program"],
                button_font,
                frame_margin,
                available_width,
            );
    }
    (footer_height * pixels_per_point).ceil() / pixels_per_point
}

#[cfg(test)]
mod tests {
    use super::*;

    const PICKER_FOOTER_LABELS: [&str; 4] = [
        "Use Selected Program",
        "Cancel",
        "Use Launcher Query instead",
        "Keep Run Program",
    ];
    const CONVERSION_CONFIRMATION: &str =
        "This choice can be stored as a safe Launcher query instead of a program.";

    fn action(label: &str, desc: &str, command: &str, args: Option<&str>) -> Action {
        Action {
            label: label.into(),
            desc: desc.into(),
            action: command.into(),
            args: args.map(str::to_owned),
        }
    }

    #[test]
    fn mkmacro_picker_chrome_keeps_confirmation_and_actions_above_long_results_without_mutation() {
        use egui::accesskit::{NodeId, Role};

        #[derive(Clone)]
        struct FooterButton {
            label: String,
            id: egui::Id,
            response_rect: egui::Rect,
            accesskit_rect: egui::Rect,
        }

        #[derive(Clone)]
        struct Layout {
            body_id: egui::Id,
            body_rect: egui::Rect,
            content_size: egui::Vec2,
            scroll_offset: egui::Vec2,
            last_result: (String, egui::Id, egui::Rect),
            footer_clip: egui::Rect,
            footer_buttons: Vec<FooterButton>,
            confirmation_label: (egui::Id, egui::Rect, egui::Rect),
        }

        fn same_rect(left: egui::Rect, right: egui::Rect) -> bool {
            (left.min.x - right.min.x).abs() < 0.5
                && (left.min.y - right.min.y).abs() < 0.5
                && (left.max.x - right.max.x).abs() < 0.5
                && (left.max.y - right.max.y).abs() < 0.5
        }

        fn accesskit_button_bounds(
            output: &egui::FullOutput,
            response_id: egui::Id,
            label: &str,
        ) -> Option<egui::Rect> {
            let node_id = NodeId(response_id.value());
            let (_, node) = output
                .platform_output
                .accesskit_update
                .as_ref()?
                .nodes
                .iter()
                .find(|(id, _)| *id == node_id)?;
            if node.role() != Role::Button || node.label() != Some(label) {
                return None;
            }
            let bounds = node.bounds()?;
            Some(egui::Rect::from_min_max(
                egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
            ))
        }

        fn accesskit_value_bounds(
            output: &egui::FullOutput,
            response_id: egui::Id,
            value: &str,
        ) -> Option<egui::Rect> {
            let node_id = NodeId(response_id.value());
            let (_, node) = output
                .platform_output
                .accesskit_update
                .as_ref()?
                .nodes
                .iter()
                .find(|(id, _)| *id == node_id)?;
            if node.value() != Some(value) {
                return None;
            }
            let bounds = node.bounds()?;
            Some(egui::Rect::from_min_max(
                egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
            ))
        }

        fn same_layout(left: &Layout, right: &Layout) -> bool {
            left.body_id == right.body_id
                && same_rect(left.body_rect, right.body_rect)
                && (left.content_size - right.content_size).length() < 0.5
                && (left.scroll_offset - right.scroll_offset).length() < 0.5
                && left.last_result.0 == right.last_result.0
                && left.last_result.1 == right.last_result.1
                && same_rect(left.last_result.2, right.last_result.2)
                && same_rect(left.footer_clip, right.footer_clip)
                && left.footer_buttons.len() == right.footer_buttons.len()
                && left
                    .footer_buttons
                    .iter()
                    .zip(&right.footer_buttons)
                    .all(|(left, right)| {
                        left.label == right.label
                            && left.id == right.id
                            && same_rect(left.response_rect, right.response_rect)
                            && same_rect(left.accesskit_rect, right.accesskit_rect)
                    })
                && left.confirmation_label.0 == right.confirmation_label.0
                && same_rect(left.confirmation_label.1, right.confirmation_label.1)
                && same_rect(left.confirmation_label.2, right.confirmation_label.2)
        }

        fn layout_from_observation(
            observation: &LauncherPickerChromeObservation,
            latest_footer_nodes: &[Option<(egui::Id, egui::Rect)>],
            latest_confirmation_node: Option<(egui::Id, egui::Rect)>,
        ) -> Option<Layout> {
            if observation.footer_buttons.len() != PICKER_FOOTER_LABELS.len()
                || !observation
                    .footer_buttons
                    .iter()
                    .zip(PICKER_FOOTER_LABELS)
                    .all(|((label, _, _), expected)| label == expected)
            {
                return None;
            }
            let mut footer_buttons = Vec::with_capacity(PICKER_FOOTER_LABELS.len());
            for (index, (label, id, response_rect)) in observation.footer_buttons.iter().enumerate()
            {
                let (node_id, accesskit_rect) = latest_footer_nodes.get(index)?.as_ref()?;
                if node_id != id {
                    return None;
                }
                footer_buttons.push(FooterButton {
                    label: label.clone(),
                    id: *id,
                    response_rect: *response_rect,
                    accesskit_rect: *accesskit_rect,
                });
            }
            let (label_id, label_rect) = observation.confirmation_label?;
            let (node_id, accesskit_label_rect) = latest_confirmation_node?;
            if node_id != label_id {
                return None;
            }
            Some(Layout {
                body_id: observation.body_id,
                body_rect: observation.body_rect,
                content_size: observation.content_size,
                scroll_offset: observation.scroll_offset,
                last_result: observation.last_result.clone()?,
                footer_clip: observation.footer_clip,
                footer_buttons,
                confirmation_label: (label_id, label_rect, accesskit_label_rect),
            })
        }

        fn frame_layout(
            output: &egui::FullOutput,
            observation: Option<&LauncherPickerChromeObservation>,
            latest_footer_nodes: &mut [Option<(egui::Id, egui::Rect)>],
            latest_confirmation_node: &mut Option<(egui::Id, egui::Rect)>,
        ) -> Option<Layout> {
            let observation = observation?;
            for (index, (label, id, _)) in observation.footer_buttons.iter().enumerate() {
                let Some(slot) = latest_footer_nodes.get_mut(index) else {
                    continue;
                };
                if let Some((old_id, _)) = *slot
                    && old_id != *id
                {
                    *slot = None;
                }
                if let Some(bounds) = accesskit_button_bounds(output, *id, label) {
                    *slot = Some((*id, bounds));
                }
            }
            if let Some((label_id, _)) = observation.confirmation_label {
                if latest_confirmation_node.is_some_and(|(old_id, _)| old_id != label_id) {
                    *latest_confirmation_node = None;
                }
                if let Some(bounds) =
                    accesskit_value_bounds(output, label_id, CONVERSION_CONFIRMATION)
                {
                    *latest_confirmation_node = Some((label_id, bounds));
                }
            }
            layout_from_observation(observation, latest_footer_nodes, *latest_confirmation_node)
        }

        let directory = tempfile::tempdir().unwrap();
        let (store, _) = crate::mkmacro::MkMacroStore::open(directory.path()).unwrap();
        let actions: Vec<_> = (0..48)
            .map(|index| {
                action(
                    &format!("Candidate {index:02}"),
                    &format!("Long launcher description for candidate {index:02}"),
                    &format!("query:launcher item {index:02}"),
                    None,
                )
            })
            .collect();
        let context = std::sync::Arc::new(crate::gui::mkmacro_dialog::MkMacroAuthoringContext {
            launcher_actions: std::sync::Arc::new(actions.clone()),
        });
        let mut dialog = super::super::MkMacroDialog::new_with_authoring_context(
            std::sync::Arc::new(store),
            (*context).clone(),
        );
        dialog.launcher_action_picker.open = true;
        dialog.launcher_action_picker.search.clear();
        dialog.launcher_action_picker.selected = Some(0);
        dialog.launcher_action_picker.request = Some(LauncherActionRequest {
            purpose: PickerPurpose::Process,
            macro_id: 7,
            step_id: Some(8),
            draft_generation: 3,
        });
        dialog.launcher_action_picker.conversion_confirmation = actions.first().cloned();

        let draft_before = dialog.draft.clone();
        let baseline_before = dialog.baseline.clone();
        let persisted_before = dialog.store.snapshot();
        let revision_before = dialog.draft_revision();
        let dirty_before = dialog.dirty;
        let conflict_before = dialog.conflict;
        let selected_before = dialog.selected_macro_id;
        let editor_draft_before = dialog.action_editor.draft.clone();
        let picker_before = (
            dialog.launcher_action_picker.open,
            dialog.launcher_action_picker.search.clone(),
            dialog.launcher_action_picker.selected,
            dialog.launcher_action_picker.request.clone(),
            dialog
                .launcher_action_picker
                .conversion_confirmation
                .clone(),
        );

        let context = egui::Context::default();
        context.enable_accesskit();
        context.style_mut_of(context.theme(), |style| {
            style.override_font_id = Some(egui::FontId::new(22.0, egui::FontFamily::Proportional));
        });
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(560.0, 420.0));
        let input = || egui::RawInput {
            screen_rect: Some(screen_rect),
            ..Default::default()
        };

        let mut latest_footer_nodes = vec![None; PICKER_FOOTER_LABELS.len()];
        let mut latest_confirmation_node = None;
        let mut previous_layout: Option<Layout> = None;
        let mut settled_initial_layout: Option<Layout> = None;
        for _ in 0..8 {
            let output = context.run_ui(input(), |ctx| show(ctx, &mut dialog));
            let observation = dialog.launcher_action_picker.test_chrome.clone();
            let current_layout = frame_layout(
                &output,
                observation.as_ref(),
                &mut latest_footer_nodes,
                &mut latest_confirmation_node,
            );
            if let (Some(previous), Some(current)) = (&previous_layout, &current_layout)
                && same_layout(previous, current)
            {
                settled_initial_layout = current_layout.clone();
            }
            previous_layout = current_layout;
            output.drop_without_applying_deltas();
        }

        let mut requested_scroll = false;
        if let Some(layout) = &settled_initial_layout
            && let Some(mut state) = egui::scroll_area::State::load(&context, layout.body_id)
        {
            state.offset.y = (layout.content_size.y - layout.body_rect.height()).max(0.0);
            state.store(&context, layout.body_id);
            requested_scroll = true;
        }

        let mut previous_scrolled_layout: Option<Layout> = None;
        let mut settled_scrolled_layout: Option<Layout> = None;
        for _ in 0..8 {
            let output = context.run_ui(input(), |ctx| show(ctx, &mut dialog));
            let observation = dialog.launcher_action_picker.test_chrome.clone();
            let current_layout = frame_layout(
                &output,
                observation.as_ref(),
                &mut latest_footer_nodes,
                &mut latest_confirmation_node,
            );
            if let (Some(previous), Some(current)) = (&previous_scrolled_layout, &current_layout)
                && same_layout(previous, current)
            {
                settled_scrolled_layout = current_layout.clone();
            }
            previous_scrolled_layout = current_layout;
            output.drop_without_applying_deltas();
        }

        let scroll_state_matches = settled_scrolled_layout.as_ref().is_some_and(|layout| {
            egui::scroll_area::State::load(&context, layout.body_id).is_some_and(|state| {
                let max_scroll = (layout.content_size.y - layout.body_rect.height()).max(0.0);
                state.offset == layout.scroll_offset
                    && state.offset.y > 0.0
                    && state.offset.y <= max_scroll + 0.5
            })
        });
        let unchanged = dialog.draft == draft_before
            && dialog.baseline == baseline_before
            && dialog.store.snapshot() == persisted_before
            && dialog.draft_revision() == revision_before
            && dialog.dirty == dirty_before
            && dialog.conflict == conflict_before
            && dialog.selected_macro_id == selected_before
            && dialog.action_editor.draft == editor_draft_before
            && (
                dialog.launcher_action_picker.open,
                dialog.launcher_action_picker.search.clone(),
                dialog.launcher_action_picker.selected,
                dialog.launcher_action_picker.request.clone(),
                dialog
                    .launcher_action_picker
                    .conversion_confirmation
                    .clone(),
            ) == picker_before;

        assert!(
            unchanged,
            "picker layout and scrolling do not transfer an action"
        );
        assert!(
            requested_scroll,
            "the native results ScrollArea accepted its saved state"
        );
        let initial =
            settled_initial_layout.expect("the constrained picker settles with its footer");
        let scrolled =
            settled_scrolled_layout.expect("long results reach a settled scroll position");
        assert!(initial.body_rect.is_positive());
        assert!(screen_rect.contains_rect(initial.body_rect));
        assert!(screen_rect.contains_rect(initial.footer_clip));
        assert!(initial.content_size.y > initial.body_rect.height());
        assert!(initial.scroll_offset.y.abs() <= 0.5);
        for button in &initial.footer_buttons {
            assert!(initial.footer_clip.contains_rect(button.response_rect));
            assert!(screen_rect.contains_rect(button.response_rect));
            assert!(screen_rect.contains_rect(button.accesskit_rect));
            assert!(same_rect(button.response_rect, button.accesskit_rect));
            assert!(initial.body_rect.bottom() <= button.response_rect.top());
        }
        assert!(
            initial
                .footer_clip
                .contains_rect(initial.confirmation_label.1)
        );
        assert!(screen_rect.contains_rect(initial.confirmation_label.1));
        assert!(same_rect(
            initial.confirmation_label.1,
            initial.confirmation_label.2,
        ));
        assert!(
            scroll_state_matches,
            "native results state uses a legal trailing offset"
        );
        let max_scroll = (scrolled.content_size.y - scrolled.body_rect.height()).max(0.0);
        assert!(scrolled.scroll_offset.y > 0.0);
        assert!(scrolled.scroll_offset.y <= max_scroll + 0.5);
        assert!(scrolled.body_rect.contains_rect(scrolled.last_result.2));
        assert!(scrolled.last_result.0.starts_with("Candidate 47\n"));
        assert!(scrolled.body_rect.bottom() <= scrolled.footer_buttons[0].response_rect.top());
        assert!(same_rect(initial.body_rect, scrolled.body_rect));
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
            metadata: Default::default(),
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
