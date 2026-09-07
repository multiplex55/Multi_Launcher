use super::MkMacroDialog;
use super::editor_operations::{self, ClipboardCommand};
use crate::mkmacro::editor_mutation::InsertionAnchor;
use crate::mkmacro::{
    MkDelayPayload, MkStep, RuntimePauseReason, RuntimeSnapshot, RuntimeState, StepState,
};
use std::collections::{BTreeSet, HashMap};

#[derive(Default, Debug, Clone)]
pub struct Selection {
    pub ids: BTreeSet<u64>,
    pub anchor: Option<u64>,
    pub primary: Option<u64>,
}
impl Selection {
    pub fn clear(&mut self) {
        self.ids.clear();
        self.anchor = None;
        self.primary = None;
    }
    pub fn click(&mut self, rows: &[u64], index: usize, ctrl: bool, shift: bool) {
        let Some(&clicked) = rows.get(index) else {
            return;
        };
        if shift {
            let a = self
                .anchor
                .and_then(|id| rows.iter().position(|row| *row == id))
                .unwrap_or(index);
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
            self.anchor = Some(clicked);
        }
        self.primary = self.ids.contains(&clicked).then_some(clicked);
        self.reconcile(rows);
    }
    /// Retain stable anchors across reordering, with deterministic source-order fallback.
    pub fn reconcile(&mut self, rows: &[u64]) {
        self.ids.retain(|id| rows.contains(id));
        // Folding can give a hidden selection a visible opener as its primary
        // without adding that entire block to the selected fragment.
        if self.ids.is_empty() || self.primary.is_none_or(|id| !rows.contains(&id)) {
            self.primary = rows.iter().find(|id| self.ids.contains(id)).copied();
        }
        if self.anchor.is_none_or(|id| !rows.contains(&id)) {
            self.anchor = self.primary;
        }
    }
    pub fn replace(&mut self, ids: impl IntoIterator<Item = u64>) {
        let ids: Vec<_> = ids.into_iter().collect();
        self.primary = ids.first().copied();
        self.anchor = self.primary;
        self.ids = ids.into_iter().collect();
    }
}
// Compatibility forwards for existing public callers. UI commands use the
// fallible domain API directly so invalid drafts receive a useful error.
pub fn duplicate_steps_with_ids(steps: &mut Vec<MkStep>, ids: &BTreeSet<u64>) -> BTreeSet<u64> {
    crate::mkmacro::editor_mutation::duplicate_selection(steps, ids).unwrap_or_default()
}
pub fn duplicate_steps(steps: &mut Vec<MkStep>, ids: &BTreeSet<u64>) {
    let _ = duplicate_steps_with_ids(steps, ids);
}
pub fn move_steps(steps: &mut [MkStep], ids: &BTreeSet<u64>, down: bool) {
    let _ = crate::mkmacro::editor_mutation::move_selection(steps, ids, down);
}
use crate::mkmacro::editor_mutation::{
    delete_selection, move_selection as move_selection_structurally,
};

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
    EditAnnotations(u64),
    Fold(u64),
    ToggleBreakpoint(u64),
    Copy,
    Cut,
    Paste,
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

/// Routes only movement keys. The surrounding UI supplies the modal and input
/// ownership state so this helper can be tested without synthesizing egui
/// frames.
fn table_move_command(
    pressed_keys: &[eframe::egui::Key],
    modifiers: eframe::egui::Modifiers,
    editor_open: bool,
    wants_keyboard_input: bool,
    pointer_in_use: bool,
    popup_open: bool,
    modal_open: bool,
) -> Option<Command> {
    if editor_open || wants_keyboard_input || pointer_in_use || popup_open || modal_open {
        return None;
    }

    if modifiers.alt && pressed_keys.contains(&eframe::egui::Key::ArrowUp) {
        Some(Command::Up)
    } else if modifiers.alt && pressed_keys.contains(&eframe::egui::Key::ArrowDown) {
        Some(Command::Down)
    } else if modifiers == eframe::egui::Modifiers::NONE
        && pressed_keys.contains(&eframe::egui::Key::ArrowUp)
    {
        Some(Command::Up)
    } else if modifiers == eframe::egui::Modifiers::NONE
        && pressed_keys.contains(&eframe::egui::Key::ArrowDown)
    {
        Some(Command::Down)
    } else {
        None
    }
}

pub(super) fn table_modal_open(d: &MkMacroDialog) -> bool {
    d.pending_folder_rename.is_some()
        || d.pending_delete_folder.is_some()
        || d.delete_confirmation.is_open()
        || d.folder_delete_confirmation.is_open()
        || d.unwrap_confirmation.is_open()
        || d.hotkey_capture
        || d.record_hotkey_capture
        || d.action_catalog_visible
        || d.structural_insertion.is_some()
        || d.uia_editor.editor_hidden()
        || d.image_crop_editor.is_open()
        || d.window_picker.open
        || d.launcher_action_picker.open
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
fn base_menu_model(
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

fn menu_model(
    step: &MkStep,
    selection: &BTreeSet<u64>,
    steps: &[MkStep],
    analysis: &crate::mkmacro::StructureAnalysis,
    breakpoint_locked: bool,
) -> Vec<MenuEntry> {
    let mut entries = base_menu_model(step, selection, steps, analysis, breakpoint_locked);
    let ids = if selection.contains(&step.id) {
        selection.clone()
    } else {
        BTreeSet::from([step.id])
    };
    let safe = crate::mkmacro::editor_mutation::normalize_selection(steps, &ids).is_ok();
    entries.extend([
        MenuEntry::separator(),
        MenuEntry::action("Copy", Command::Copy, safe),
        MenuEntry::action("Cut", Command::Cut, safe),
        MenuEntry::action("Paste", Command::Paste, true),
        MenuEntry::action("Edit annotations", Command::EditAnnotations(step.id), true),
    ]);
    if step.action.is_block_marker() {
        entries.push(MenuEntry::action("Duplicate", Command::Duplicate, safe));
    }
    if let Some(block) = analysis.block_for_marker(step.id)
        && super::folding::foldable_block(analysis, block.opener_id).is_some()
    {
        entries.push(MenuEntry::action(
            "Expand / collapse block",
            Command::Fold(block.opener_id),
            true,
        ));
    }
    entries
}

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
    let diagnostics = d.cached_diagnostics();
    if d.analysis_cache.borrow().environment_pending {
        ui.weak(
            "External image and monitor checks need refresh. Save and Run refresh automatically.",
        );
    }
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
    let Some(structure) = d.cached_structure(mid) else {
        return;
    };
    if let Some(primary) = d.selection.primary {
        d.selection.primary = Some(
            d.editor_state
                .folds
                .visible_primary(mid, primary, &structure),
        );
    }
    if d.editor_state
        .drag
        .as_ref()
        .is_some_and(|drag| drag.macro_id != mid)
    {
        d.editor_state.drag = None;
    }
    let visible = d.editor_state.folds.visible_rows(mid, &structure);
    let scroll_row = d
        .editor_state
        .scroll_to
        .take()
        .filter(|(macro_id, _)| *macro_id == mid)
        .and_then(|(_, id)| visible.iter().position(|i| rows[*i] == id));
    let depths: Vec<_> = structure.steps.iter().map(|s| s.depth).collect();
    let mut clicked = None;
    let mut changed = false;
    let mut updates = Vec::new();
    let mut breakpoint_toggles = Vec::new();
    let mut command = None;
    let mut drag_start = None;
    let mut drop_rows = Vec::new();
    let interaction_blocked = d.action_editor.draft.is_some() || table_modal_open(d);
    let table_height = table_viewport_height(ui.available_height());
    let table_response = ui.allocate_ui_with_layout(
        eframe::egui::vec2(ui.available_width(), table_height),
        eframe::egui::Layout::top_down(eframe::egui::Align::Min),
        |ui| {
    ui.push_id(("mkmacro_steps", mid), |ui| {
    let max_scroll_height = ui.available_height().max(MIN_TABLE_VIEWPORT_HEIGHT);
    let mut table = egui_extras::TableBuilder::new(ui)
        .striped(true)
        .auto_shrink([false, false])
        .max_scroll_height(max_scroll_height)
        .column(egui_extras::Column::exact(BREAKPOINT_COLUMN_WIDTH))
        .column(egui_extras::Column::exact(38.0))
        .column(egui_extras::Column::exact(55.0))
        .column(egui_extras::Column::initial(100.0))
        .column(egui_extras::Column::remainder())
        .column(egui_extras::Column::exact(50.0))
        .column(egui_extras::Column::exact(55.0))
        .column(egui_extras::Column::initial(90.0));
    if let Some(row) = scroll_row { table = table.scroll_to_row(row, Some(eframe::egui::Align::Center)); }
    table.header(20.0, |mut h| {
            for x in [
                "●", "#", "Enabled", "Action", "Details", "Repeat", "Delay", "Status",
            ] {
                h.col(|ui| {
                    ui.label(x);
                });
            }
        })
        .body(|mut body| {
            for &i in &visible {
                let source = &m.steps[i];
                let mut s = source.clone();
                body.row(22.0, |mut r| {
                    r.set_selected(d.selection.ids.contains(&s.id) || d.selection.primary == Some(s.id));
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
                        let response = ui.push_id((mid, s.id, "step_drag"), |ui| ui.add(
                            eframe::egui::Label::new(format!("⠿ {}", i + 1))
                                .sense(eframe::egui::Sense::click_and_drag())
                        )).inner.on_hover_text("Select or drag this step; selected steps move together");
                        if response.clicked() {
                            let mods = ui.input(|x| x.modifiers);
                            clicked = Some((i, mods.ctrl, mods.shift));
                        }
                        if !interaction_blocked && response.drag_started() { drag_start = Some(s.id); }
                    });
                    r.col(|ui| {
                        changed |= ui.add_enabled(
                            s.action.can_be_disabled(),
                            eframe::egui::Checkbox::without_text(&mut s.enabled),
                        ).changed();
                    });
                    r.col(|ui| {
                        ui.horizontal(|ui| {
                        if let Some(block) = super::folding::foldable_block(&structure, s.id) {
                            let collapsed = d.editor_state.folds.is_collapsed(mid, s.id);
                            let response = ui.small_button(if collapsed { "▶" } else { "▼" })
                                .on_hover_text(if collapsed { "Expand block" } else { "Collapse block" });
                            if response.clicked() { command = Some(Command::Fold(s.id)); }
                            response.on_hover_text(format!("{} steps in this block", block.closer_index - block.opener_index));
                        }
                        let structural = matches!(s.action, crate::mkmacro::MkAction::Else|crate::mkmacro::MkAction::EndIf|crate::mkmacro::MkAction::RepeatEnd|crate::mkmacro::MkAction::WhileEnd);
                        let label = format!("{}{}", "  ".repeat(depths[i]), super::action_catalog::action_name(&s.action));
                        let response=if structural { ui.strong(label) } else { ui.label(label) };
                        if response.double_clicked(){command=Some(Command::Edit(s.id));}
                        if response.secondary_clicked() && !d.selection.ids.contains(&s.id) { clicked = Some((i, false, false)); }
                        response.context_menu(|ui| {
                            let menu = menu_model(
                            &s,
                            &d.selection.ids,
                            &m.steps,
                            &structure,
                            breakpoint_locked,
                            );
                            render_context_menu(ui, &menu, &mut command);
                        });
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
                    });
                    r.col(|ui| {
                        ui.horizontal(|ui| {
                            if let Some(color) = super::action_editor::accent_color(s.metadata.accent) { ui.colored_label(color, "●").on_hover_text("Step accent"); }
                            if s.metadata.bookmarked { ui.label("★").on_hover_text("Bookmarked step"); }
                            if !s.metadata.comment.is_empty() { ui.label("≡").on_hover_text(&s.metadata.comment); }
                            let details = super::action_catalog::action_details(&s.action);
                            let details = if d.editor_state.folds.is_collapsed(mid, s.id) {
                                super::folding::foldable_block(&structure, s.id)
                                    .map_or(details.clone(), |b| format!("{details} — {} hidden steps", b.closer_index - b.opener_index))
                            } else { details };
                            let full = if s.metadata.label.is_empty() { details } else { format!("{} — {details}", s.metadata.label) };
                            let short = if full.chars().count() > 80 { format!("{}…", full.chars().take(80).collect::<String>()) } else { full.clone() };
                            let response = ui.label(short).on_hover_text(full);
                            if response.double_clicked() { command = Some(Command::Edit(s.id)); }
                            if response.secondary_clicked() && !d.selection.ids.contains(&s.id) { clicked = Some((i, false, false)); }
                            response.context_menu(|ui| {
                                let menu = menu_model(&s, &d.selection.ids, &m.steps, &structure, breakpoint_locked);
                                render_context_menu(ui, &menu, &mut command);
                            });
                        });
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
                    let after = if d.editor_state.folds.is_collapsed(mid, s.id) {
                        super::folding::foldable_block(&structure, s.id).map_or(s.id, |b| b.closer_id)
                    } else { s.id };
                    drop_rows.push((r.response().rect, InsertionAnchor::Before(s.id), InsertionAnchor::After(after)));
                });
                updates.push((s.id,s.enabled,s.repeat,s.delay_after_ms));
            }
        });
    });
        },
    );
    if let Some(id) = drag_start {
        let result = editor_operations::begin_drag(d, id);
        report_command(d, result);
    }
    if d.editor_state.drag.is_some() {
        let cancelled =
            interaction_blocked || ui.input(|i| i.key_pressed(eframe::egui::Key::Escape));
        if cancelled {
            d.editor_state.drag = None;
        } else {
            let target = ui.input(|i| i.pointer.hover_pos()).and_then(|pointer| {
                if !table_response
                    .response
                    .rect
                    .intersect(ui.clip_rect())
                    .contains(pointer)
                {
                    return None;
                }
                drop_rows
                    .iter()
                    .find(|(rect, _, _)| pointer.y <= rect.bottom())
                    .map(|(rect, before, after)| {
                        if pointer.y < rect.center().y {
                            (*before, rect.top(), *rect)
                        } else {
                            (*after, rect.bottom(), *rect)
                        }
                    })
                    .or_else(|| {
                        drop_rows
                            .last()
                            .map(|(rect, _, after)| (*after, rect.bottom(), *rect))
                    })
            });
            if let Some((anchor, y, rect)) = target {
                let result = editor_operations::preview_drop(d, anchor);
                let color = if result.is_ok() {
                    ui.visuals().selection.stroke.color
                } else {
                    eframe::egui::Color32::RED
                };
                ui.painter().line_segment(
                    [
                        eframe::egui::pos2(rect.left(), y),
                        eframe::egui::pos2(rect.right(), y),
                    ],
                    eframe::egui::Stroke::new(2.0_f32, color),
                );
                if ui.input(|i| i.pointer.any_released()) {
                    let result = editor_operations::finish_drag(d, anchor);
                    report_command(d, result);
                } else if let Err(reason) = result {
                    eframe::egui::show_tooltip_at_pointer(
                        ui.ctx(),
                        eframe::egui::Id::new("mkmacro_drop_error"),
                        |ui| {
                            ui.label(reason);
                        },
                    );
                }
            } else if ui.input(|i| i.pointer.any_released()) {
                d.editor_state.drag = None;
                d.command_error = Some("Drop cancelled: choose a step insertion boundary".into());
            }
        }
    }
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
    // Only route table shortcuts while no modal/editor, focused control, active
    // pointer drag, popup, or context menu owns input.
    let editor_open = d.action_editor.draft.is_some();
    let wants_keyboard_input = ui.ctx().wants_keyboard_input();
    let pointer_in_use = ui.ctx().is_using_pointer();
    let popup_open =
        ui.ctx().memory(|memory| memory.any_popup_open()) || ui.ctx().is_context_menu_open();
    let modal_open = table_modal_open(d);
    let table_owns_input = d.action_editor.draft.is_none()
        && !wants_keyboard_input
        && !pointer_in_use
        && !popup_open
        && !modal_open;
    let clipboard_command = ui.input_mut(|input| {
        editor_operations::clipboard_shortcut(
            input,
            table_owns_input,
            &mut d.editor_state.shortcut_state,
        )
    });
    if let Some(clipboard_command) = clipboard_command {
        command = Some(match clipboard_command {
            ClipboardCommand::Copy => Command::Copy,
            ClipboardCommand::Cut => Command::Cut,
            ClipboardCommand::Paste => Command::Paste,
            ClipboardCommand::Duplicate => Command::Duplicate,
        });
    } else if table_owns_input && command.is_none() {
        ui.input(|i| {
            if i.key_pressed(eframe::egui::Key::Enter) {
                if let Some(id) = d.selection.primary {
                    command = Some(Command::Edit(id))
                }
            } else if i.key_pressed(eframe::egui::Key::Delete) {
                command = Some(Command::Delete)
            } else {
                let pressed_keys = [eframe::egui::Key::ArrowUp, eframe::egui::Key::ArrowDown]
                    .into_iter()
                    .filter(|key| i.key_pressed(*key))
                    .collect::<Vec<_>>();
                command = table_move_command(
                    &pressed_keys,
                    i.modifiers,
                    editor_open,
                    wants_keyboard_input,
                    pointer_in_use,
                    popup_open,
                    modal_open,
                );
            }
        });
    }
    if let Some(c) = command {
        apply_command(d, c, breakpoint_locked);
    }
    if d.editor_state.scroll_to.is_some() || matches!(command, Some(Command::Fold(_))) {
        ui.ctx().request_repaint();
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
        primary: Some(id),
    };
    let result = d.debug_selected_steps();
    d.selection = selection_before_debug;
    result
}

fn apply_command(d: &mut MkMacroDialog, c: Command, breakpoint_locked: bool) {
    let clipboard = match c {
        Command::Copy => Some(ClipboardCommand::Copy),
        Command::Cut => Some(ClipboardCommand::Cut),
        Command::Paste => Some(ClipboardCommand::Paste),
        Command::Duplicate => Some(ClipboardCommand::Duplicate),
        _ => None,
    };
    if let Some(command) = clipboard {
        let result = editor_operations::clipboard(d, command);
        report_command(d, result);
        return;
    }
    if let Command::Fold(id) = c {
        if let Some(mid) = d.selected_macro_id
            && let Some(analysis) = d.cached_structure(mid)
        {
            d.editor_state.folds.toggle(mid, id, &analysis);
            if let Some(primary) = d.selection.primary {
                d.selection.primary = Some(
                    d.editor_state
                        .folds
                        .visible_primary(mid, primary, &analysis),
                );
            }
        }
        return;
    }
    if let Command::EditAnnotations(id) = c {
        if let Some(step) = d
            .selected_macro()
            .and_then(|m| m.steps.iter().find(|s| s.id == id))
            .cloned()
        {
            d.action_editor.begin_edit(&step);
        }
        return;
    }
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
            d.selection.replace([id]);
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
    let movement_before = matches!(c, Command::Up | Command::Down).then(|| d.draft.clone());
    if let Some(m) = d.selected_macro_mut() {
        match c {
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
                        metadata: Default::default(),
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
        d.selection = selection_before_command;
        d.command_error = Some(error.to_string());
        return;
    }
    crate::mkmacro::repair_ids(&mut d.draft);
    if let Some(s) = new_selection {
        if let Some(m) = d.selected_macro() {
            let rows = m.steps.iter().map(|s| s.id).collect::<Vec<_>>();
            if matches!(c, Command::Up | Command::Down) {
                d.selection.ids = s;
                d.selection.reconcile(&rows);
            } else {
                d.selection
                    .replace(rows.into_iter().filter(|id| s.contains(id)));
            }
        }
    }
    if movement_before
        .as_ref()
        .is_none_or(|before| before != &d.draft)
    {
        d.mark_dirty();
    }
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
            d.selection.replace(selected);
            d.mark_dirty();
        }
        Err(e) => d.command_error = Some(e),
    }
}
#[cfg(test)]
mod layout_tests {
    use super::*;
    use crate::mkmacro::{MkAction, MkCondition, MkErrorPolicy};

    #[test]
    fn selection_anchors_follow_stable_ids_after_reorder() {
        let mut selection = Selection::default();
        selection.click(&[40, 10, 30, 20], 1, false, false);
        selection.reconcile(&[10, 40, 30, 20]);
        selection.click(&[10, 40, 30, 20], 2, false, true);
        assert_eq!(selection.ids, BTreeSet::from([10, 40, 30]));
        assert_eq!(selection.anchor, Some(10));
        assert_eq!(selection.primary, Some(30));
        selection.reconcile(&[40, 20]);
        assert_eq!(selection.ids, BTreeSet::from([40]));
        assert_eq!(selection.anchor, Some(40));
        assert_eq!(selection.primary, Some(40));
    }

    #[test]
    fn fold_keeps_hidden_selection_and_visible_primary_without_dirtying() {
        let (_directory, mut d) = dialog_with_steps(vec![
            step(1, MkAction::If(MkCondition::All { conditions: vec![] })),
            delay(2),
            step(3, MkAction::Else),
            delay(4),
            step(5, MkAction::EndIf),
        ]);
        d.mark_dirty();
        d.dirty = false;
        let before = d.draft.clone();
        let revision = d.draft_revision();
        d.selection.replace([4]);
        apply_command(&mut d, Command::Fold(1), false);
        assert_eq!(d.selection.ids, BTreeSet::from([4]));
        assert_eq!(d.selection.primary, Some(1));
        d.selection.reconcile(&[1, 2, 3, 4, 5]);
        assert_eq!(d.selection.primary, Some(1));
        assert_eq!(d.selection.ids, BTreeSet::from([4]));
        assert_eq!(d.draft_revision(), revision);
        assert_eq!(d.draft, before);
        assert!(!d.dirty);
        apply_command(&mut d, Command::Copy, false);
        assert_eq!(
            d.editor_state
                .clipboard
                .iter()
                .map(|s| s.id)
                .collect::<Vec<_>>(),
            [4]
        );
        apply_command(&mut d, Command::Fold(1), false);
        assert_eq!(d.selection.ids, BTreeSet::from([4]));
        assert!(!d.dirty);
    }

    #[test]
    fn annotations_edit_the_exact_marker_transactionally() {
        let (_directory, mut d) = dialog_with_steps(vec![
            step(1, MkAction::RepeatStart { count: 2 }),
            delay(2),
            step(3, MkAction::RepeatEnd),
        ]);
        d.mark_dirty();
        d.dirty = false;
        let original = d.selected_macro().unwrap().steps.clone();
        apply_command(&mut d, Command::EditAnnotations(3), false);
        d.action_editor.draft.as_mut().unwrap().metadata.label = "End of loop".into();
        assert_eq!(d.selected_macro().unwrap().steps, original);
        assert!(!d.dirty);
        d.action_editor.cancel();
        assert_eq!(d.selected_macro().unwrap().steps, original);
        apply_command(&mut d, Command::EditAnnotations(3), false);
        let metadata = crate::mkmacro::MkStepMetadata {
            label: "End of loop".into(),
            comment: "Repeat marker\nwith notes".into(),
            bookmarked: true,
            accent: crate::mkmacro::MkStepAccent::Blue,
        };
        d.action_editor.draft.as_mut().unwrap().metadata = metadata.clone();
        let replacement =
            super::super::action_editor::ActionEditorState::new(d.visual_overlay.clone());
        let mut editor = std::mem::replace(&mut d.action_editor, replacement);
        assert_eq!(editor.apply(&mut d), Some(3));
        d.action_editor = editor;
        assert_eq!(d.selected_macro().unwrap().steps[2].metadata, metadata);
        let mut expected = original;
        expected[2].metadata = metadata;
        assert_eq!(d.selected_macro().unwrap().steps, expected);
        assert!(d.dirty);
    }

    fn step(id: u64, action: MkAction) -> MkStep {
        MkStep {
            metadata: Default::default(),
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

    fn row_ids(rows: &[MkStep]) -> Vec<u64> {
        rows.iter().map(|row| row.id).collect()
    }

    fn selected_row_ids(rows: &[MkStep], ids: &BTreeSet<u64>) -> Vec<u64> {
        rows.iter()
            .filter(|row| ids.contains(&row.id))
            .map(|row| row.id)
            .collect()
    }

    fn dialog_with_steps(steps: Vec<MkStep>) -> (tempfile::TempDir, MkMacroDialog) {
        let directory = tempfile::tempdir().unwrap();
        let (store, _) = crate::mkmacro::MkMacroStore::open(directory.path()).unwrap();
        let mut dialog = MkMacroDialog::new(std::sync::Arc::new(store));
        dialog.create_macro();
        dialog.selected_macro_mut().unwrap().steps = steps;
        dialog.dirty = false;
        (directory, dialog)
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

    #[test]
    fn movement_routes_single_ordinary_selection_both_directions() {
        let mut rows = vec![delay(1), delay(2), delay(3)];
        let selected = BTreeSet::from([2]);

        assert_eq!(
            move_selection_structurally(&mut rows, &selected, false).unwrap(),
            selected
        );
        assert_eq!(row_ids(&rows), vec![2, 1, 3]);

        assert_eq!(
            move_selection_structurally(&mut rows, &selected, true).unwrap(),
            selected
        );
        assert_eq!(row_ids(&rows), vec![1, 2, 3]);
    }

    #[test]
    fn movement_preserves_contiguous_and_non_contiguous_selection_order() {
        let contiguous = BTreeSet::from([2, 3]);
        let mut rows = vec![delay(1), delay(2), delay(3), delay(4)];
        assert_eq!(
            move_selection_structurally(&mut rows, &contiguous, false).unwrap(),
            contiguous
        );
        assert_eq!(row_ids(&rows), vec![2, 3, 1, 4]);
        assert_eq!(selected_row_ids(&rows, &contiguous), vec![2, 3]);

        let mut rows = vec![delay(1), delay(2), delay(3), delay(4), delay(5)];
        let non_contiguous = BTreeSet::from([2, 4]);
        assert_eq!(
            move_selection_structurally(&mut rows, &non_contiguous, false).unwrap(),
            non_contiguous
        );
        assert_eq!(row_ids(&rows), vec![2, 1, 4, 3, 5]);
        assert_eq!(selected_row_ids(&rows, &non_contiguous), vec![2, 4]);

        let mut rows = vec![delay(1), delay(2), delay(3), delay(4), delay(5)];
        assert_eq!(
            move_selection_structurally(&mut rows, &non_contiguous, true).unwrap(),
            non_contiguous
        );
        assert_eq!(row_ids(&rows), vec![1, 3, 2, 5, 4]);
        assert_eq!(selected_row_ids(&rows, &non_contiguous), vec![2, 4]);
    }

    #[test]
    fn movement_expands_if_else_and_loop_ranges_from_any_marker() {
        let if_rows = || {
            vec![
                delay(9),
                step(1, MkAction::If(MkCondition::All { conditions: vec![] })),
                delay(2),
                step(3, MkAction::Else),
                delay(4),
                step(5, MkAction::EndIf),
                delay(10),
            ]
        };
        for selected_marker in [1, 3, 5] {
            let mut rows = if_rows();
            let selected =
                move_selection_structurally(&mut rows, &BTreeSet::from([selected_marker]), false)
                    .unwrap();
            assert_eq!(selected, BTreeSet::from([1, 2, 3, 4, 5]));
            assert_eq!(row_ids(&rows), vec![1, 2, 3, 4, 5, 9, 10]);
        }

        for (opener, closer) in [
            (MkAction::RepeatStart { count: 2 }, MkAction::RepeatEnd),
            (
                MkAction::WhileStart {
                    condition: MkCondition::All { conditions: vec![] },
                },
                MkAction::WhileEnd,
            ),
        ] {
            for selected_marker in [1, 3] {
                let mut rows = vec![
                    delay(9),
                    step(1, opener.clone()),
                    delay(2),
                    step(3, closer.clone()),
                    delay(10),
                ];
                let selected = move_selection_structurally(
                    &mut rows,
                    &BTreeSet::from([selected_marker]),
                    false,
                )
                .unwrap();
                assert_eq!(selected, BTreeSet::from([1, 2, 3]));
                assert_eq!(row_ids(&rows), vec![1, 2, 3, 9, 10]);
            }
        }
    }

    #[test]
    fn movement_handles_nested_blocks_and_neighboring_structures() {
        let mut nested = vec![
            delay(0),
            step(1, MkAction::If(MkCondition::All { conditions: vec![] })),
            step(2, MkAction::RepeatStart { count: 2 }),
            delay(3),
            step(4, MkAction::RepeatEnd),
            delay(5),
            step(6, MkAction::EndIf),
            delay(7),
        ];
        let inner = move_selection_structurally(&mut nested, &BTreeSet::from([4]), true).unwrap();
        assert_eq!(inner, BTreeSet::from([2, 3, 4]));
        assert_eq!(row_ids(&nested), vec![0, 1, 5, 2, 3, 4, 6, 7]);

        let mut nested = vec![
            delay(0),
            step(1, MkAction::If(MkCondition::All { conditions: vec![] })),
            step(2, MkAction::RepeatStart { count: 2 }),
            delay(3),
            step(4, MkAction::RepeatEnd),
            delay(5),
            step(6, MkAction::EndIf),
            delay(7),
        ];
        let outer = move_selection_structurally(&mut nested, &BTreeSet::from([1]), false).unwrap();
        assert_eq!(outer, BTreeSet::from([1, 2, 3, 4, 5, 6]));
        assert_eq!(row_ids(&nested), vec![1, 2, 3, 4, 5, 6, 0, 7]);

        let mut neighboring = vec![
            delay(9),
            step(1, MkAction::If(MkCondition::All { conditions: vec![] })),
            delay(2),
            step(3, MkAction::EndIf),
            step(4, MkAction::RepeatStart { count: 2 }),
            delay(5),
            step(6, MkAction::RepeatEnd),
            delay(10),
        ];
        move_selection_structurally(&mut neighboring, &BTreeSet::from([3]), true).unwrap();
        assert_eq!(row_ids(&neighboring), vec![9, 4, 5, 6, 1, 2, 3, 10]);
    }

    #[test]
    fn movement_at_legal_boundaries_is_unchanged() {
        let mut rows = vec![delay(1), delay(2), delay(3)];
        let before = row_ids(&rows);
        assert_eq!(
            move_selection_structurally(&mut rows, &BTreeSet::from([1]), false).unwrap(),
            BTreeSet::from([1])
        );
        assert_eq!(row_ids(&rows), before);
        assert_eq!(
            move_selection_structurally(&mut rows, &BTreeSet::from([3]), true).unwrap(),
            BTreeSet::from([3])
        );
        assert_eq!(row_ids(&rows), before);
    }

    #[test]
    fn malformed_movement_is_transactional_and_does_not_dirty_the_macro() {
        let malformed = vec![step(1, MkAction::RepeatStart { count: 1 }), delay(2)];
        let before = malformed.clone();
        let mut rows = malformed.clone();
        assert!(move_selection_structurally(&mut rows, &BTreeSet::from([1]), true).is_err());
        assert_eq!(rows, before);

        let (_directory, mut dialog) = dialog_with_steps(malformed);
        dialog.selection.ids = BTreeSet::from([1]);
        let selection_before = dialog.selection.clone();
        apply_command(&mut dialog, Command::Down, false);
        assert_eq!(dialog.selected_macro().unwrap().steps, before);
        assert_eq!(dialog.selection.ids, selection_before.ids);
        assert!(!dialog.dirty);
        assert!(dialog.command_error.is_some());
    }

    #[test]
    fn table_move_shortcuts_route_plain_and_legacy_alt_keys_only_when_unowned() {
        use eframe::egui::{Key, Modifiers};

        for (key, modifiers, expected) in [
            (Key::ArrowUp, Modifiers::NONE, Command::Up),
            (Key::ArrowDown, Modifiers::NONE, Command::Down),
            (Key::ArrowUp, Modifiers::ALT, Command::Up),
            (Key::ArrowDown, Modifiers::ALT, Command::Down),
        ] {
            assert_eq!(
                table_move_command(&[key], modifiers, false, false, false, false, false,),
                Some(expected)
            );
        }
        assert_eq!(
            table_move_command(
                &[Key::F1],
                Modifiers::NONE,
                false,
                false,
                false,
                false,
                false,
            ),
            None
        );
        for (editor_open, wants_keyboard_input, pointer_in_use, popup_open, modal_open) in [
            (true, false, false, false, false),
            (false, true, false, false, false),
            (false, false, true, false, false),
            (false, false, false, true, false),
            (false, false, false, false, true),
        ] {
            assert_eq!(
                table_move_command(
                    &[Key::ArrowUp],
                    Modifiers::NONE,
                    editor_open,
                    wants_keyboard_input,
                    pointer_in_use,
                    popup_open,
                    modal_open,
                ),
                None
            );
        }
    }

    #[test]
    fn plain_and_alt_shortcuts_apply_the_same_up_and_down_commands() {
        use eframe::egui::{Key, Modifiers};

        let apply_shortcut = |key, modifiers| {
            let (_directory, mut dialog) = dialog_with_steps(vec![delay(1), delay(2), delay(3)]);
            dialog.selection.ids = BTreeSet::from([2]);
            let command = table_move_command(&[key], modifiers, false, false, false, false, false)
                .expect("shortcut should route to a movement command");
            apply_command(&mut dialog, command, false);
            row_ids(&dialog.selected_macro().unwrap().steps)
        };

        assert_eq!(
            apply_shortcut(Key::ArrowUp, Modifiers::NONE),
            apply_shortcut(Key::ArrowUp, Modifiers::ALT)
        );
        assert_eq!(
            apply_shortcut(Key::ArrowDown, Modifiers::NONE),
            apply_shortcut(Key::ArrowDown, Modifiers::ALT)
        );
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
                ("", false),
                ("Copy", true),
                ("Cut", true),
                ("Paste", true),
                ("Edit annotations", true),
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
            ("", false),
            ("Copy", true),
            ("Cut", true),
            ("Paste", true),
            ("Edit annotations", true),
            ("Duplicate", true),
            ("Expand / collapse block", true),
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
            &[
                ("Delete Block", false),
                ("Unwrap Block", false),
                ("", false),
                ("Copy", false),
                ("Cut", false),
                ("Paste", true),
                ("Edit annotations", true),
                ("Duplicate", false)
            ]
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
            primary: Some(2),
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
