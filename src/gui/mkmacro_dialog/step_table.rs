use super::MkMacroDialog;
use crate::mkmacro::{
    MkDelayPayload, MkStep, MonitorValidation, RuntimePauseReason, RuntimeSnapshot, RuntimeState,
    StepState, ValidationContext, validate_document_with_context,
};
use std::collections::{BTreeSet, HashMap};

#[derive(Default, Debug, Clone)]
pub struct Selection {
    pub ids: BTreeSet<u64>,
    anchor: Option<usize>,
}
impl Selection {
    pub fn clear(&mut self) {
        self.ids.clear();
        self.anchor = None;
    }
    pub fn click(&mut self, rows: &[u64], index: usize, ctrl: bool, shift: bool) {
        if shift {
            let a = self.anchor.unwrap_or(index);
            if !ctrl {
                self.ids.clear();
            }
            for id in &rows[a.min(index)..=a.max(index)] {
                self.ids.insert(*id);
            }
        } else {
            if !ctrl {
                self.ids.clear();
            }
            if ctrl && self.ids.contains(&rows[index]) {
                self.ids.remove(&rows[index]);
            } else {
                self.ids.insert(rows[index]);
            }
            self.anchor = Some(index);
        }
    }
}
pub fn duplicate_steps_with_ids(steps: &mut Vec<MkStep>, ids: &BTreeSet<u64>) -> BTreeSet<u64> {
    let mut copies: Vec<_> = steps
        .iter()
        .filter(|s| ids.contains(&s.id))
        .cloned()
        .collect();
    for s in &mut copies {
        s.id = 0;
    }
    let insert_at = steps
        .iter()
        .rposition(|s| ids.contains(&s.id))
        .map_or(steps.len(), |i| i + 1);
    steps.splice(insert_at..insert_at, copies);
    let mut d = crate::mkmacro::MkMacroDocument {
        settings: Default::default(),
        schema_version: crate::mkmacro::SCHEMA_VERSION,
        macros: vec![crate::mkmacro::MkMacro {
            id: 1,
            name: "draft".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: Default::default(),
            steps: steps.clone(),
            image_assets: vec![],
        }],
        folders: vec![],
    };
    crate::mkmacro::repair_ids(&mut d);
    *steps = d.macros.remove(0).steps;
    steps[insert_at..]
        .iter()
        .take(ids.len())
        .map(|s| s.id)
        .collect()
}
pub fn duplicate_steps(steps: &mut Vec<MkStep>, ids: &BTreeSet<u64>) {
    let _ = duplicate_steps_with_ids(steps, ids);
}
pub fn move_steps(steps: &mut [MkStep], ids: &BTreeSet<u64>, down: bool) {
    if down {
        for i in (0..steps.len().saturating_sub(1)).rev() {
            if ids.contains(&steps[i].id) && !ids.contains(&steps[i + 1].id) {
                steps.swap(i, i + 1);
            }
        }
    } else {
        for i in 1..steps.len() {
            if ids.contains(&steps[i].id) && !ids.contains(&steps[i - 1].id) {
                steps.swap(i, i - 1);
            }
        }
    }
}

const BREAKPOINT_COLUMN_WIDTH: f32 = 26.0;
const BREAKPOINT_HOVER_TEXT: &str = "Breakpoint\nPauses before this step during Debug runs.\nNormal Run and macro hotkeys ignore breakpoints.";
const BREAKPOINT_LOCKED_HOVER_TEXT: &str = "Stop the current playback before changing breakpoints.";
const ACTIVE_BREAKPOINT_STATUS_TEXT: &str = "Paused at breakpoint\nThis step has not executed yet.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BreakpointVisual {
    glyph: &'static str,
    color: eframe::egui::Color32,
}

fn breakpoint_visual(breakpoint: bool) -> BreakpointVisual {
    if breakpoint {
        BreakpointVisual {
            glyph: "●",
            color: eframe::egui::Color32::from_rgb(239, 83, 80),
        }
    } else {
        BreakpointVisual {
            glyph: "○",
            color: eframe::egui::Color32::GRAY,
        }
    }
}

fn breakpoint_edit_locked(state: Option<RuntimeState>) -> bool {
    matches!(
        state,
        Some(RuntimeState::Running | RuntimeState::Paused | RuntimeState::Stopping)
    )
}

fn breakpoint_editable(runtime: Option<&RuntimeSnapshot>) -> bool {
    !breakpoint_edit_locked(runtime.map(|snapshot| snapshot.state))
}

fn toggle_breakpoint_by_id(steps: &mut [MkStep], step_id: u64) -> bool {
    let Some(step) = steps.iter_mut().find(|step| step.id == step_id) else {
        return false;
    };
    step.breakpoint = !step.breakpoint;
    true
}

fn active_breakpoint_status(
    runtime: Option<&RuntimeSnapshot>,
    displayed_macro_id: u64,
    step_id: u64,
    state: StepState,
) -> bool {
    state == StepState::Pending
        && runtime.is_some_and(|snapshot| {
            snapshot.macro_id == Some(displayed_macro_id)
                && snapshot.state == RuntimeState::Paused
                && matches!(
                    snapshot.pause_reason,
                    Some(RuntimePauseReason::Breakpoint { step_id: paused_step_id })
                        if paused_step_id == step_id
                )
        })
}

fn status_visual(
    state: StepState,
    active_breakpoint: bool,
) -> (&'static str, &'static str, eframe::egui::Color32) {
    if active_breakpoint && state == StepState::Pending {
        return (
            "⏸",
            ACTIVE_BREAKPOINT_STATUS_TEXT,
            eframe::egui::Color32::from_rgb(255, 152, 0),
        );
    }
    match state {
        StepState::Pending => ("○", "Pending", eframe::egui::Color32::GRAY),
        StepState::Running => ("▶", "Running", eframe::egui::Color32::YELLOW),
        StepState::Success => ("✓", "Success", eframe::egui::Color32::GREEN),
        StepState::Skipped => ("–", "Skipped", eframe::egui::Color32::GRAY),
        StepState::Failed => ("✕", "Failed", eframe::egui::Color32::RED),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command {
    Edit(u64),
    ToggleBreakpoint(u64),
    Duplicate,
    Toggle,
    Up,
    Down,
    Delete,
    DeleteRow(u64),
    DeleteBlock(u64),
    UnwrapBlock(u64),
    InsertAbove(u64),
    InsertBelow(u64),
    RunOne,
    RunFrom,
    DebugOne(u64),
    DebugFrom(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MenuEntry {
    label: &'static str,
    command: Option<Command>,
    enabled: bool,
    disabled_reason: Option<&'static str>,
}

impl MenuEntry {
    fn action(label: &'static str, command: Command, enabled: bool) -> Self {
        Self {
            label,
            command: Some(command),
            enabled,
            disabled_reason: None,
        }
    }
    fn separator() -> Self {
        Self {
            label: "",
            command: None,
            enabled: false,
            disabled_reason: None,
        }
    }
}

fn toggle_breakpoint_entry(id: u64, locked: bool) -> MenuEntry {
    let mut entry = MenuEntry::action("Toggle Breakpoint", Command::ToggleBreakpoint(id), !locked);
    if locked {
        entry.disabled_reason = Some(BREAKPOINT_LOCKED_HOVER_TEXT);
    }
    entry
}

/// Pure description of a row menu. Structural markers are deliberately resolved
/// through the analysis rather than inferred from their spelling or position.
fn menu_model(
    step: &MkStep,
    selection: &BTreeSet<u64>,
    steps: &[MkStep],
    analysis: &crate::mkmacro::StructureAnalysis,
    breakpoint_locked: bool,
) -> Vec<MenuEntry> {
    let id = step.id;
    if step.action.is_block_marker() {
        if let Some(block) = analysis.block_for_marker(id) {
            let edit = match block.kind {
                crate::mkmacro::BlockKind::If => "Edit Condition",
                crate::mkmacro::BlockKind::Repeat => "Edit Repeat",
                crate::mkmacro::BlockKind::While => "Edit While",
            };
            return vec![
                MenuEntry::action(edit, Command::Edit(block.opener_id), true),
                MenuEntry::action("Run From Here", Command::RunFrom, true),
                MenuEntry::action("Debug From Here", Command::DebugFrom(id), true),
                MenuEntry::separator(),
                toggle_breakpoint_entry(id, breakpoint_locked),
                MenuEntry::action("Insert Above", Command::InsertAbove(id), true),
                MenuEntry::action("Insert Below", Command::InsertBelow(id), true),
                MenuEntry::separator(),
                MenuEntry::action("Delete Block", Command::DeleteBlock(id), true),
                MenuEntry::action("Unwrap Block", Command::UnwrapBlock(id), true),
            ];
        }
        let mut delete = MenuEntry::action("Delete Block", Command::DeleteBlock(id), false);
        delete.disabled_reason = Some("This marker is not part of a complete block");
        let mut unwrap = MenuEntry::action("Unwrap Block", Command::UnwrapBlock(id), false);
        unwrap.disabled_reason = delete.disabled_reason;
        return vec![
            MenuEntry::action(
                "Edit",
                Command::Edit(id),
                step.action
                    .block_marker()
                    .is_some_and(|m| matches!(m, crate::mkmacro::MkBlockMarker::Open(_))),
            ),
            MenuEntry::action("Run From Here", Command::RunFrom, true),
            MenuEntry::action("Debug From Here", Command::DebugFrom(id), true),
            MenuEntry::separator(),
            toggle_breakpoint_entry(id, breakpoint_locked),
            MenuEntry::action("Insert Above", Command::InsertAbove(id), true),
            MenuEntry::action("Insert Below", Command::InsertBelow(id), true),
            MenuEntry::separator(),
            delete,
            unwrap,
        ];
    }
    let ids = if selection.contains(&id) {
        selection.clone()
    } else {
        BTreeSet::from([id])
    };
    let move_enabled = |down| {
        let mut candidate = steps.to_vec();
        move_selection_structurally(&mut candidate, &ids, down).is_ok_and(|_| candidate != steps)
    };
    let can_up = move_enabled(false);
    let can_down = move_enabled(true);
    vec![
        MenuEntry::action("Edit", Command::Edit(id), true),
        MenuEntry::action("Run This Step", Command::RunOne, true),
        MenuEntry::action("Debug This Step", Command::DebugOne(id), true),
        MenuEntry::action("Run From Here", Command::RunFrom, true),
        MenuEntry::action("Debug From Here", Command::DebugFrom(id), true),
        MenuEntry::separator(),
        toggle_breakpoint_entry(id, breakpoint_locked),
        MenuEntry::action("Insert Above", Command::InsertAbove(id), true),
        MenuEntry::action("Insert Below", Command::InsertBelow(id), true),
        MenuEntry::action("Duplicate", Command::Duplicate, true),
        MenuEntry::action(
            if step.enabled { "Disable" } else { "Enable" },
            Command::Toggle,
            step.action.can_be_disabled(),
        ),
        MenuEntry::action("Move Up", Command::Up, can_up),
        MenuEntry::action("Move Down", Command::Down, can_down),
        MenuEntry::action("Delete", Command::DeleteRow(id), true),
    ]
}
const MIN_TABLE_VIEWPORT_HEIGHT: f32 = 48.0;

/// Uses all height assigned to the step area; the number of rows is deliberately
/// irrelevant because rows belong to the table's scroll area.
fn table_viewport_height(available_height: f32) -> f32 {
    available_height.max(MIN_TABLE_VIEWPORT_HEIGHT)
}

pub(super) fn show(ui: &mut eframe::egui::Ui, d: &mut MkMacroDialog) {
    let Some(mid) = d.selected_macro_id else {
        ui.label("Select a macro");
        return;
    };
    let monitor_result = crate::mkmacro::monitor_descriptors();
    let monitor_validation = match &monitor_result {
        Ok(descriptors) => MonitorValidation::Available(descriptors),
        Err(_) => MonitorValidation::EnumerationFailed,
    };
    let asset_root = d.store.asset_root();
    let diagnostics = validate_document_with_context(
        &d.draft,
        ValidationContext {
            asset_root: Some(&asset_root),
            monitors: monitor_validation,
        },
    );
    let mut row_diagnostics = HashMap::<u64, Vec<_>>::new();
    for diagnostic in diagnostics.iter().filter(|x| x.macro_id == mid) {
        if let Some(step_id) = diagnostic.step_id {
            row_diagnostics.entry(step_id).or_default().push(diagnostic);
        }
    }
    for diagnostic in diagnostics
        .iter()
        .filter(|x| x.macro_id == mid && x.step_id.is_none())
    {
        let color = match diagnostic.severity {
            crate::mkmacro::DiagnosticSeverity::Fatal => eframe::egui::Color32::RED,
            crate::mkmacro::DiagnosticSeverity::Warning => eframe::egui::Color32::YELLOW,
        };
        ui.colored_label(color, format!("⚠ {}", diagnostic.message))
            .on_hover_text(format!("{}\nCode: {}", diagnostic.message, diagnostic.code));
    }
    let runtime = crate::mkmacro::runtime::snapshot();
    let breakpoint_locked = !breakpoint_editable(runtime.as_deref());
    let Some(m) = d.draft.macros.iter().find(|m| m.id == mid) else {
        return;
    };
    let rows: Vec<u64> = m.steps.iter().map(|s| s.id).collect();
    let depths = super::action_catalog::action_depths(m);
    let structure = crate::mkmacro::analyze_structure(&m.steps);
    let mut clicked = None;
    let mut changed = false;
    let mut updates = Vec::new();
    let mut breakpoint_toggles = Vec::new();
    let mut command = None;
    let table_height = table_viewport_height(ui.available_height());
    ui.allocate_ui_with_layout(
        eframe::egui::vec2(ui.available_width(), table_height),
        eframe::egui::Layout::top_down(eframe::egui::Align::Min),
        |ui| {
    let max_scroll_height = ui.available_height().max(MIN_TABLE_VIEWPORT_HEIGHT);
    egui_extras::TableBuilder::new(ui)
        .striped(true)
        .auto_shrink([false, false])
        .max_scroll_height(max_scroll_height)
        .column(egui_extras::Column::exact(BREAKPOINT_COLUMN_WIDTH))
        .column(egui_extras::Column::exact(28.0))
        .column(egui_extras::Column::exact(55.0))
        .column(egui_extras::Column::initial(100.0))
        .column(egui_extras::Column::remainder())
        .column(egui_extras::Column::exact(50.0))
        .column(egui_extras::Column::exact(55.0))
        .column(egui_extras::Column::initial(90.0))
        .header(20.0, |mut h| {
            for x in [
                "●", "#", "Enabled", "Action", "Details", "Repeat", "Delay", "Status",
            ] {
                h.col(|ui| {
                    ui.label(x);
                });
            }
        })
        .body(|mut body| {
            for (i, source) in m.steps.iter().enumerate() {
                let mut s = source.clone();
                body.row(22.0, |mut r| {
                    r.col(|ui| {
                        let visual = breakpoint_visual(s.breakpoint);
                        let response = ui.add_enabled(
                            !breakpoint_locked,
                            eframe::egui::Button::new(
                                eframe::egui::RichText::new(visual.glyph).color(visual.color),
                            )
                            .frame(false),
                        );
                        let response = if breakpoint_locked {
                            response.on_disabled_hover_text(BREAKPOINT_LOCKED_HOVER_TEXT)
                        } else {
                            response.on_hover_text(BREAKPOINT_HOVER_TEXT)
                        };
                        if response.clicked() {
                            breakpoint_toggles.push(s.id);
                        }
                    });
                    r.col(|ui| {
                        if ui
                            .selectable_label(d.selection.ids.contains(&s.id), (i + 1).to_string())
                            .clicked()
                        {
                            let mods = ui.input(|x| x.modifiers);
                            clicked = Some((i, mods.ctrl, mods.shift));
                        }
                    });
                    r.col(|ui| {
                        changed |= ui.add_enabled(
                            s.action.can_be_disabled(),
                            eframe::egui::Checkbox::without_text(&mut s.enabled),
                        ).changed();
                    });
                    r.col(|ui| {
                        let structural = matches!(s.action, crate::mkmacro::MkAction::Else|crate::mkmacro::MkAction::EndIf|crate::mkmacro::MkAction::RepeatEnd|crate::mkmacro::MkAction::WhileEnd);
                        let label = format!("{}{}", "  ".repeat(depths[i]), super::action_catalog::action_name(&s.action));
                        let response=if structural { ui.strong(label) } else { ui.label(label) };
                        if response.double_clicked(){command=Some(Command::Edit(s.id));}
                        if response.secondary_clicked() && !d.selection.ids.contains(&s.id) { clicked = Some((i, false, false)); }
                        let menu = menu_model(
                            &s,
                            &d.selection.ids,
                            &m.steps,
                            &structure,
                            breakpoint_locked,
                        );
                        response.context_menu(|ui| render_context_menu(ui, &menu, &mut command));
                        if let Some(items) = row_diagnostics.get(&s.id) {
                            let first = items[0];
                            let color = match first.severity {
                                crate::mkmacro::DiagnosticSeverity::Fatal => eframe::egui::Color32::RED,
                                crate::mkmacro::DiagnosticSeverity::Warning => eframe::egui::Color32::YELLOW,
                            };
                            let hover = items.iter().map(|x| format!("{}\nCode: {}", x.message, x.code)).collect::<Vec<_>>().join("\n\n");
                            ui.colored_label(color, format!("⚠ {}", first.message)).on_hover_text(hover);
                        }
                    });
                    r.col(|ui| {
                        let full=super::action_catalog::action_details_with_assets(&s.action, &m.image_assets); let short=if full.chars().count()>80 {format!("{}…",full.chars().take(80).collect::<String>())}else{full.clone()}; let response=ui.label(short).on_hover_text(full);if response.double_clicked(){command=Some(Command::Edit(s.id));}if response.secondary_clicked() && !d.selection.ids.contains(&s.id) { clicked = Some((i, false, false)); }let menu = menu_model(&s, &d.selection.ids, &m.steps, &structure, breakpoint_locked);response.context_menu(|ui|render_context_menu(ui, &menu, &mut command));
                    });
                    r.col(|ui| {
                        changed |= ui.add(eframe::egui::DragValue::new(&mut s.repeat).clamp_range(1..=1_000_000)).changed();
                    });
                    r.col(|ui| {
                        changed |= ui.add(eframe::egui::DragValue::new(&mut s.delay_after_ms).clamp_range(0..=86_400_000)).changed();
                    });
                    r.col(|ui| {
                        if let Some(state) = runtime.as_ref().and_then(|run| {
                            (run.macro_id == Some(mid)).then(|| run.steps.get(&s.id)).flatten()
                        }) {
                            let active_breakpoint = active_breakpoint_status(
                                runtime.as_deref(),
                                mid,
                                s.id,
                                *state,
                            );
                            let (label, full, color) = status_visual(*state, active_breakpoint);
                            let detail = if active_breakpoint {
                                full
                            } else {
                                runtime
                                    .as_ref()
                                    .and_then(|run| run.step_outcomes.get(&s.id))
                                    .and_then(crate::mkmacro::StepOutcome::detail)
                                    .unwrap_or(full)
                            };
                            let response = ui.colored_label(color, label).on_hover_text(detail);
                            if let Some(run) = runtime.as_ref()
                                && let Some(failure) = run.failures.get(&crate::mkmacro::DiagnosticKey { run_id: run.run_id, step_id: s.id })
                            {
                                response.on_hover_ui(|ui| {
                                    ui.strong(&failure.message);
                                    for (key, value) in &failure.context { ui.label(format!("{key}: {value}")); }
                                    if failure.kind == crate::mkmacro::DiagnosticKind::InputRejected {
                                        ui.label("Likely integrity/UIPI restriction: SendInput accepted zero events.");
                                    }
                                });
                            }
                        }
                    });
                });
                updates.push((s.id,s.enabled,s.repeat,s.delay_after_ms));
            }
        });
        },
    );
    if !matches!(command, Some(Command::ToggleBreakpoint(_))) {
        if let Some((i, ctrl, shift)) = clicked {
            d.selection.click(&rows, i, ctrl, shift);
        }
    }
    if !breakpoint_toggles.is_empty() {
        let mut toggled = false;
        if let Some(m) = d.selected_macro_mut() {
            for id in breakpoint_toggles {
                toggled |= toggle_breakpoint_by_id(&mut m.steps, id);
            }
        }
        if toggled {
            d.mark_dirty();
        }
    }
    if changed {
        if let Some(m) = d.selected_macro_mut() {
            for (id, en, repeat, delay) in updates {
                if let Some(s) = m.steps.iter_mut().find(|s| s.id == id) {
                    s.enabled = en;
                    s.repeat = repeat;
                    s.delay_after_ms = delay;
                }
            }
        }
        d.mark_dirty();
    }
    // Only route table shortcuts while no modal/editor or text edit owns input.
    if d.action_editor.draft.is_none() && !ui.ctx().wants_keyboard_input() {
        ui.input(|i| {
            if i.key_pressed(eframe::egui::Key::Enter) {
                if let Some(id) = d.selection.ids.iter().next() {
                    command = Some(Command::Edit(*id))
                }
            } else if i.key_pressed(eframe::egui::Key::Delete) {
                command = Some(Command::Delete)
            } else if i.modifiers.ctrl && i.key_pressed(eframe::egui::Key::D) {
                command = Some(Command::Duplicate)
            } else if i.modifiers.alt && i.key_pressed(eframe::egui::Key::ArrowUp) {
                command = Some(Command::Up)
            } else if i.modifiers.alt && i.key_pressed(eframe::egui::Key::ArrowDown) {
                command = Some(Command::Down)
            }
        });
    }
    if let Some(c) = command {
        apply_command(d, c, breakpoint_locked);
    }
}

fn render_context_menu(
    ui: &mut eframe::egui::Ui,
    entries: &[MenuEntry],
    out: &mut Option<Command>,
) {
    for entry in entries {
        let Some(command) = entry.command else {
            ui.separator();
            continue;
        };
        let response = ui.add_enabled(entry.enabled, eframe::egui::Button::new(entry.label));
        let response = if let Some(reason) = entry.disabled_reason {
            response.on_disabled_hover_text(reason)
        } else {
            response
        };
        if response.clicked() {
            *out = Some(command);
            ui.close_menu();
        }
    }
}
fn report_command(d: &mut MkMacroDialog, result: anyhow::Result<()>) {
    if let Err(e) = result {
        d.command_error = Some(e.to_string());
    }
}

fn debug_this_step(d: &mut MkMacroDialog, id: u64) -> anyhow::Result<()> {
    let selection_before_debug = d.selection.clone();
    d.selection = Selection {
        ids: BTreeSet::from([id]),
        anchor: None,
    };
    let result = d.debug_selected_steps();
    d.selection = selection_before_debug;
    result
}

fn apply_command(d: &mut MkMacroDialog, c: Command, breakpoint_locked: bool) {
    let selection_before_command = d.selection.clone();
    if let Command::ToggleBreakpoint(id) = c {
        if breakpoint_locked {
            return;
        }
        let toggled = d
            .selected_macro_mut()
            .is_some_and(|m| toggle_breakpoint_by_id(&mut m.steps, id));
        if toggled {
            d.mark_dirty();
        }
        return;
    }
    if let Command::DeleteBlock(id) | Command::DeleteRow(id) | Command::UnwrapBlock(id) = c {
        if !d.selection.ids.contains(&id) {
            d.selection.ids = BTreeSet::from([id]);
            d.selection.anchor = None;
        }
    }
    if let Command::Edit(id) = c {
        let edit_id = d.selected_macro().and_then(|m| {
            let step = m.steps.iter().find(|s| s.id == id)?;
            if !step.action.is_block_marker() {
                return Some(id);
            }
            crate::mkmacro::analyze_structure(&m.steps)
                .block_for_marker(id)
                .map(|block| block.opener_id)
                .or_else(|| {
                    matches!(
                        step.action.block_marker(),
                        Some(crate::mkmacro::MkBlockMarker::Open(_))
                    )
                    .then_some(id)
                })
        });
        let Some(edit_id) = edit_id else {
            d.command_error =
                Some("The selected closing marker is not part of a complete block".into());
            return;
        };
        if let Some(s) = d
            .selected_macro()
            .and_then(|m| m.steps.iter().find(|s| s.id == edit_id))
            .cloned()
        {
            if matches!(
                s.action,
                crate::mkmacro::MkAction::UiInvoke(_)
                    | crate::mkmacro::MkAction::UiSetValue { .. }
                    | crate::mkmacro::MkAction::UiReadValue { .. }
                    | crate::mkmacro::MkAction::UiToggle(_)
                    | crate::mkmacro::MkAction::UiSelect(_)
                    | crate::mkmacro::MkAction::UiFocus(_)
                    | crate::mkmacro::MkAction::UiWait(_)
            ) {
                d.command_error = Some(
                    "UI Automation actions are currently unavailable; the saved action was left unchanged."
                        .into(),
                );
            } else {
                d.action_editor.begin_edit(&s);
            }
        }
        return;
    }
    if matches!(
        c,
        Command::RunOne | Command::RunFrom | Command::DebugOne(_) | Command::DebugFrom(_)
    ) {
        let result = match (c, d.selection.ids.iter().next().copied()) {
            (Command::RunOne, _) => d.run_selected_steps(),
            (Command::RunFrom, Some(id)) => d.run_from_step(id),
            (Command::DebugOne(id), _) => debug_this_step(d, id),
            (Command::DebugFrom(id), _) => d.debug_from_step(id),
            _ => Err(anyhow::anyhow!("Select a step")),
        };
        report_command(d, result);
        return;
    }
    if let Command::UnwrapBlock(id) = c {
        let Some(block) = d.selected_macro().and_then(|m| {
            crate::mkmacro::analyze_structure(&m.steps)
                .block_for_marker(id)
                .cloned()
        }) else {
            d.command_error = Some("The selected marker is not part of a complete block".into());
            return;
        };
        if block.else_marker.is_some() {
            d.pending_unwrap_block = Some(id);
            d.pending_unwrap_selection = Some(selection_before_command);
            d.unwrap_confirmation.open_custom("Unwrap If block", "Unwrapping this If will preserve both branches and make them execute sequentially.");
        } else {
            apply_confirmed_unwrap(d, id);
        }
        return;
    }
    let ids = d.selection.ids.clone();
    let mut new_selection = None;
    let mut mutation_error = None;
    if let Some(m) = d.selected_macro_mut() {
        match c {
            Command::Duplicate => {
                new_selection = Some(duplicate_steps_with_ids(&mut m.steps, &ids))
            }
            Command::Toggle => {
                for s in &mut m.steps {
                    if ids.contains(&s.id) && s.action.can_be_disabled() {
                        s.enabled = !s.enabled
                    }
                }
            }
            Command::Up | Command::Down => {
                match move_selection_structurally(&mut m.steps, &ids, matches!(c, Command::Down)) {
                    Ok(s) => new_selection = Some(s),
                    Err(e) => mutation_error = Some(e),
                }
            }
            Command::Delete | Command::DeleteRow(_) | Command::DeleteBlock(_) => {
                match delete_selection(&mut m.steps, &ids) {
                    Ok(s) => new_selection = Some(s),
                    Err(e) => mutation_error = Some(e),
                }
            }
            Command::InsertAbove(id) | Command::InsertBelow(id) => {
                let mut i = m
                    .steps
                    .iter()
                    .position(|s| s.id == id)
                    .unwrap_or(m.steps.len());
                if matches!(c, Command::InsertBelow(_)) {
                    i += 1
                }
                m.steps.insert(
                    i,
                    MkStep {
                        id: 0,
                        enabled: true,
                        breakpoint: false,
                        repeat: 1,
                        delay_after_ms: 0,
                        on_error: Default::default(),
                        action: crate::mkmacro::MkAction::Delay(MkDelayPayload {
                            fixed_ms: 1000,
                            ..Default::default()
                        }),
                    },
                );
            }
            _ => {}
        }
    }
    if let Some(error) = mutation_error {
        d.command_error = Some(error);
        return;
    }
    crate::mkmacro::repair_ids(&mut d.draft);
    if let Some(s) = new_selection {
        d.selection.ids = s;
        d.selection.anchor = None;
    }
    d.mark_dirty();
}

fn delete_selection(steps: &mut Vec<MkStep>, ids: &BTreeSet<u64>) -> Result<BTreeSet<u64>, String> {
    if ids.is_empty() {
        return Ok(BTreeSet::new());
    }
    let analysis = crate::mkmacro::analyze_structure(steps);
    let mut remove = BTreeSet::new();
    for id in ids {
        let Some((i, s)) = steps.iter().enumerate().find(|(_, s)| s.id == *id) else {
            continue;
        };
        if s.action.is_block_marker() {
            let b = analysis
                .block_for_marker(*id)
                .ok_or_else(|| format!("Step {id} is not part of a complete block"))?;
            remove.extend(b.range.clone());
        } else {
            remove.insert(i);
        }
    }
    if remove.is_empty() {
        return Ok(BTreeSet::new());
    }
    let first = *remove.first().unwrap();
    let last = *remove.last().unwrap();
    let next = (last + 1..steps.len())
        .find(|i| !remove.contains(i))
        .map(|i| steps[i].id);
    let prev = (0..first)
        .rev()
        .find(|i| !remove.contains(i))
        .map(|i| steps[i].id);
    let mut i = 0;
    steps.retain(|_| {
        let keep = !remove.contains(&i);
        i += 1;
        keep
    });
    Ok(next
        .or(prev)
        .map(|id| BTreeSet::from([id]))
        .unwrap_or_default())
}

fn expanded_move_ids(steps: &[MkStep], ids: &BTreeSet<u64>) -> Result<BTreeSet<u64>, String> {
    let a = crate::mkmacro::analyze_structure(steps);
    let mut out = ids.clone();
    for id in ids {
        if let Some(s) = steps.iter().find(|s| s.id == *id)
            && s.action.is_block_marker()
        {
            let b = a
                .block_for_marker(*id)
                .ok_or_else(|| format!("Step {id} is not part of a complete block"))?;
            out.extend(steps[b.range.clone()].iter().map(|s| s.id));
        }
    }
    Ok(out)
}
fn move_selection_structurally(
    steps: &mut [MkStep],
    ids: &BTreeSet<u64>,
    down: bool,
) -> Result<BTreeSet<u64>, String> {
    let expanded = expanded_move_ids(steps, ids)?;
    let before = crate::mkmacro::analyze_structure(steps).diagnostics.len();
    let mut candidate = steps.to_vec();
    move_steps(&mut candidate, &expanded, down);
    if crate::mkmacro::analyze_structure(&candidate)
        .diagnostics
        .len()
        > before
    {
        return Err("The move would invalidate block nesting".into());
    }
    steps.clone_from_slice(&candidate);
    Ok(expanded)
}

pub(super) fn apply_confirmed_unwrap(d: &mut MkMacroDialog, id: u64) {
    let result = if let Some(m) = d.selected_macro_mut() {
        crate::mkmacro::unwrap_block(&mut m.steps, id)
    } else {
        return;
    };
    match result {
        Ok(r) => {
            let selected = r
                .first_preserved_body_id
                .or(r.following_id)
                .or(r.preceding_id);
            d.selection.ids = selected.map(|x| BTreeSet::from([x])).unwrap_or_default();
            d.selection.anchor = None;
            d.mark_dirty();
        }
        Err(e) => d.command_error = Some(e),
    }
}
#[cfg(test)]
mod layout_tests {
    use super::*;
    use crate::mkmacro::{MkAction, MkCondition, MkErrorPolicy};

    fn step(id: u64, action: MkAction) -> MkStep {
        MkStep {
            id,
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: MkErrorPolicy::Stop,
            action,
        }
    }
    fn delay(id: u64) -> MkStep {
        step(
            id,
            MkAction::Delay(MkDelayPayload {
                fixed_ms: 1,
                ..Default::default()
            }),
        )
    }

    #[test]
    fn table_uses_all_remaining_height() {
        assert_eq!(table_viewport_height(500.0), 500.0);
        assert_eq!(table_viewport_height(250.0), 250.0);
        assert_eq!(table_viewport_height(20.0), MIN_TABLE_VIEWPORT_HEIGHT);
    }

    #[test]
    fn row_count_cannot_affect_table_height() {
        let allocations = [0_usize, 10, 500].map(|_row_count| table_viewport_height(500.0));
        assert_eq!(allocations, [500.0; 3]);
    }

    #[test]
    fn deletion_resolves_every_if_marker_and_deduplicates_nested_selections() {
        for selected in [1, 3, 7] {
            let mut rows = vec![
                delay(9),
                step(1, MkAction::If(MkCondition::All { conditions: vec![] })),
                step(2, MkAction::RepeatStart { count: 2 }),
                step(4, MkAction::Break),
                step(5, MkAction::RepeatEnd),
                step(3, MkAction::Else),
                delay(6),
                step(7, MkAction::EndIf),
                delay(10),
            ];
            let ids = BTreeSet::from([selected, 2, 4]);
            let selection = delete_selection(&mut rows, &ids).unwrap();
            assert_eq!(rows.iter().map(|s| s.id).collect::<Vec<_>>(), vec![9, 10]);
            assert_eq!(selection, BTreeSet::from([10]));
        }
    }

    #[test]
    fn controls_delete_ordinary_and_selection_falls_back() {
        let mut rows = vec![
            delay(1),
            step(2, MkAction::Break),
            step(3, MkAction::Continue),
        ];
        assert_eq!(
            delete_selection(&mut rows, &BTreeSet::from([2])).unwrap(),
            BTreeSet::from([3])
        );
        assert_eq!(
            delete_selection(&mut rows, &BTreeSet::from([3])).unwrap(),
            BTreeSet::from([1])
        );
        assert_eq!(
            delete_selection(&mut rows, &BTreeSet::from([1])).unwrap(),
            BTreeSet::new()
        );
    }

    #[test]
    fn duplication_preserves_breakpoint_and_allocates_a_fresh_id() {
        let mut rows = vec![delay(1), delay(2)];
        rows[0].breakpoint = true;

        let duplicated_ids = duplicate_steps_with_ids(&mut rows, &BTreeSet::from([1]));

        assert_eq!(duplicated_ids.len(), 1);
        assert_ne!(rows[1].id, rows[0].id);
        assert!(duplicated_ids.contains(&rows[1].id));
        assert!(rows[1].breakpoint);
    }

    #[test]
    fn deletion_removes_the_breakpoint_with_its_step() {
        let mut rows = vec![delay(1), delay(2), delay(3)];
        rows[1].breakpoint = true;

        delete_selection(&mut rows, &BTreeSet::from([2])).unwrap();

        assert_eq!(rows.iter().map(|step| step.id).collect::<Vec<_>>(), [1, 3]);
        assert!(rows.iter().all(|step| !step.breakpoint));
    }

    #[test]
    fn moving_marker_moves_complete_block_as_one_unit() {
        let mut rows = vec![
            delay(9),
            step(
                1,
                MkAction::WhileStart {
                    condition: MkCondition::All { conditions: vec![] },
                },
            ),
            delay(2),
            step(3, MkAction::WhileEnd),
            delay(10),
        ];
        let selected = move_selection_structurally(&mut rows, &BTreeSet::from([3]), false).unwrap();
        assert_eq!(selected, BTreeSet::from([1, 2, 3]));
        assert_eq!(
            rows.iter().map(|s| s.id).collect::<Vec<_>>(),
            vec![1, 2, 3, 9, 10]
        );
        let mut malformed = vec![step(1, MkAction::RepeatStart { count: 1 }), delay(2)];
        assert!(move_selection_structurally(&mut malformed, &BTreeSet::from([1]), true).is_err());
    }

    fn labels(rows: &[MkStep], id: u64) -> Vec<(&'static str, bool)> {
        labels_with_lock(rows, id, false)
    }

    fn labels_with_lock(
        rows: &[MkStep],
        id: u64,
        breakpoint_locked: bool,
    ) -> Vec<(&'static str, bool)> {
        menu_entries(rows, id, breakpoint_locked)
            .into_iter()
            .map(|entry| (entry.label, entry.enabled))
            .collect()
    }

    fn menu_entries(rows: &[MkStep], id: u64, breakpoint_locked: bool) -> Vec<MenuEntry> {
        let analysis = crate::mkmacro::analyze_structure(rows);
        let row = rows.iter().find(|row| row.id == id).unwrap();
        menu_model(
            row,
            &BTreeSet::from([id]),
            rows,
            &analysis,
            breakpoint_locked,
        )
    }

    #[test]
    fn menu_models_are_action_and_structure_aware() {
        let ordinary = vec![delay(1), delay(2), delay(3)];
        assert_eq!(
            labels(&ordinary, 2),
            vec![
                ("Edit", true),
                ("Run This Step", true),
                ("Debug This Step", true),
                ("Run From Here", true),
                ("Debug From Here", true),
                ("", false),
                ("Toggle Breakpoint", true),
                ("Insert Above", true),
                ("Insert Below", true),
                ("Duplicate", true),
                ("Disable", true),
                ("Move Up", true),
                ("Move Down", true),
                ("Delete", true),
            ]
        );
        for action in [MkAction::Break, MkAction::Continue] {
            let rows = vec![delay(1), step(2, action), delay(3)];
            assert_eq!(labels(&rows, 2), labels(&ordinary, 2));
        }

        let if_rows = vec![
            step(1, MkAction::If(MkCondition::All { conditions: vec![] })),
            delay(2),
            step(3, MkAction::Else),
            delay(4),
            step(5, MkAction::EndIf),
        ];
        let if_expected = vec![
            ("Edit Condition", true),
            ("Run From Here", true),
            ("Debug From Here", true),
            ("", false),
            ("Toggle Breakpoint", true),
            ("Insert Above", true),
            ("Insert Below", true),
            ("", false),
            ("Delete Block", true),
            ("Unwrap Block", true),
        ];
        for id in [1, 3, 5] {
            assert_eq!(labels(&if_rows, id), if_expected);
        }

        let repeat = vec![
            step(1, MkAction::RepeatStart { count: 2 }),
            delay(2),
            step(3, MkAction::RepeatEnd),
        ];
        let while_rows = vec![
            step(
                1,
                MkAction::WhileStart {
                    condition: MkCondition::All { conditions: vec![] },
                },
            ),
            delay(2),
            step(3, MkAction::WhileEnd),
        ];
        for id in [1, 3] {
            assert_eq!(labels(&repeat, id)[0], ("Edit Repeat", true));
            assert_eq!(labels(&while_rows, id)[0], ("Edit While", true));
        }

        let malformed = vec![step(1, MkAction::EndIf)];
        let malformed_menu = labels(&malformed, 1);
        assert_eq!(malformed_menu[0], ("Edit", false));
        assert_eq!(
            &malformed_menu[8..],
            &[("Delete Block", false), ("Unwrap Block", false)]
        );
    }

    #[test]
    fn debug_commands_carry_the_context_row_id() {
        let ordinary = vec![delay(1), delay(2), delay(3)];
        let entries = menu_entries(&ordinary, 2, false);
        assert_eq!(
            entries
                .iter()
                .filter_map(|entry| entry.command)
                .collect::<Vec<_>>()[1..5],
            [
                Command::RunOne,
                Command::DebugOne(2),
                Command::RunFrom,
                Command::DebugFrom(2),
            ]
        );

        let structural = vec![
            step(1, MkAction::If(MkCondition::All { conditions: vec![] })),
            delay(2),
            step(3, MkAction::EndIf),
        ];
        for id in [1, 3] {
            let entries = menu_entries(&structural, id, false);
            assert!(
                entries
                    .iter()
                    .any(|entry| entry.command == Some(Command::DebugFrom(id)))
            );
            assert!(
                !entries
                    .iter()
                    .any(|entry| entry.command == Some(Command::DebugOne(id)))
            );
        }
    }

    #[test]
    fn breakpoint_visual_classifies_set_and_unset_symbols() {
        let unset = breakpoint_visual(false);
        assert_eq!(unset.glyph, "○");
        assert_eq!(unset.color, eframe::egui::Color32::GRAY);

        let set = breakpoint_visual(true);
        assert_eq!(set.glyph, "●");
        assert_eq!(set.color, eframe::egui::Color32::from_rgb(239, 83, 80));
        assert_ne!(set.color, unset.color);
    }

    #[test]
    fn breakpoint_editability_only_locks_active_playback() {
        let cases = [
            (RuntimeState::Idle, true),
            (RuntimeState::Running, false),
            (RuntimeState::Paused, false),
            (RuntimeState::Stopping, false),
            (RuntimeState::Completed, true),
            (RuntimeState::Stopped, true),
            (RuntimeState::Failed, true),
        ];
        for (state, editable) in cases {
            let runtime = RuntimeSnapshot {
                state,
                ..RuntimeSnapshot::default()
            };
            assert_eq!(breakpoint_editable(Some(&runtime)), editable);
            assert_eq!(breakpoint_edit_locked(Some(state)), !editable);
        }
        assert!(breakpoint_editable(None));
    }

    #[test]
    fn breakpoint_mutation_uses_stable_id_without_touching_selection() {
        let mut rows = vec![delay(11), delay(22), delay(33)];
        rows[0].enabled = false;
        rows[1].repeat = 7;
        rows[2].delay_after_ms = 99;
        let before = rows.clone();
        let selection = Selection {
            ids: BTreeSet::from([11, 33]),
            anchor: Some(2),
        };
        let selection_before = selection.clone();

        assert!(toggle_breakpoint_by_id(&mut rows, 22));
        assert_eq!(rows[0], before[0]);
        assert_eq!(rows[2], before[2]);
        assert_eq!(rows[1].id, before[1].id);
        assert_eq!(rows[1].repeat, before[1].repeat);
        assert_eq!(rows[1].breakpoint, !before[1].breakpoint);
        assert_eq!(selection.ids, selection_before.ids);
        assert_eq!(selection.anchor, selection_before.anchor);

        assert!(!toggle_breakpoint_by_id(&mut rows, 999));
        assert_eq!(selection.ids, BTreeSet::from([11, 33]));
    }

    #[test]
    fn breakpoint_menu_entry_is_disabled_during_active_playback() {
        let rows = vec![delay(1)];
        let entries = menu_entries(&rows, 1, true);
        let entry = entries
            .iter()
            .find(|entry| entry.label == "Toggle Breakpoint")
            .unwrap();
        assert_eq!(entry.command, Some(Command::ToggleBreakpoint(1)));
        assert!(!entry.enabled);
        assert_eq!(entry.disabled_reason, Some(BREAKPOINT_LOCKED_HOVER_TEXT));
    }

    #[test]
    fn active_breakpoint_status_requires_matching_paused_runtime_boundary() {
        let mut runtime = RuntimeSnapshot {
            state: RuntimeState::Paused,
            macro_id: Some(7),
            pause_reason: Some(RuntimePauseReason::Breakpoint { step_id: 22 }),
            ..RuntimeSnapshot::default()
        };
        assert!(active_breakpoint_status(
            Some(&runtime),
            7,
            22,
            StepState::Pending
        ));
        let (glyph, tooltip, _) = status_visual(StepState::Pending, true);
        assert_eq!(glyph, "⏸");
        assert_eq!(tooltip, ACTIVE_BREAKPOINT_STATUS_TEXT);

        assert!(!active_breakpoint_status(
            Some(&runtime),
            8,
            22,
            StepState::Pending
        ));
        assert!(!active_breakpoint_status(
            Some(&runtime),
            7,
            23,
            StepState::Pending
        ));
        runtime.pause_reason = Some(RuntimePauseReason::User);
        assert!(!active_breakpoint_status(
            Some(&runtime),
            7,
            22,
            StepState::Pending
        ));
        runtime.pause_reason = Some(RuntimePauseReason::Breakpoint { step_id: 22 });
        runtime.state = RuntimeState::Running;
        assert!(!active_breakpoint_status(
            Some(&runtime),
            7,
            22,
            StepState::Pending
        ));
        runtime.state = RuntimeState::Paused;
        assert!(!active_breakpoint_status(
            Some(&runtime),
            7,
            22,
            StepState::Success
        ));
        assert_eq!(status_visual(StepState::Pending, false).0, "○");
    }
}
