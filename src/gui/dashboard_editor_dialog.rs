use crate::dashboard::config::{DashboardConfig, OverflowMode, SlotConfig};
use crate::dashboard::widgets::{WidgetRegistry, WidgetSettingsContext};
use eframe::egui;
use eframe::egui::collapsing_header::CollapsingState;
use serde_json::Value;
use std::collections::HashSet;

#[derive(Default)]
struct SlotCoverage {
    cells: Vec<(usize, usize)>,
    out_of_bounds: bool,
}

#[derive(Default)]
struct OccupancyMap {
    owners: Vec<Vec<Vec<usize>>>,
    collisions: Vec<Vec<bool>>,
    slot_conflicts: Vec<bool>,
}

enum SplitDirection {
    Horizontal,
    Vertical,
}

pub struct DashboardEditorDialog {
    pub open: bool,
    path: String,
    config: DashboardConfig,
    error: Option<String>,
    pending_save: bool,
    selected_slot: Option<usize>,
    show_preview: bool,
    blocked_warning: Option<String>,
    drag_anchor: Option<(usize, usize)>,
    swap_anchor: Option<usize>,
    slot_expand_all: bool,
    slot_collapse_all: bool,
    snap_on_edit: bool,
    show_swap_buttons: bool,
    show_remove_buttons: bool,
    confirm_remove_slot: Option<usize>,
}

impl Default for DashboardEditorDialog {
    fn default() -> Self {
        Self {
            open: false,
            path: "dashboard.json".into(),
            config: DashboardConfig::default(),
            error: None,
            pending_save: false,
            selected_slot: None,
            show_preview: false,
            blocked_warning: None,
            drag_anchor: None,
            swap_anchor: None,
            slot_expand_all: false,
            slot_collapse_all: false,
            snap_on_edit: false,
            show_swap_buttons: true,
            show_remove_buttons: true,
            confirm_remove_slot: None,
        }
    }
}

impl DashboardEditorDialog {
    pub fn open(&mut self, path: &str, registry: &WidgetRegistry) {
        self.path = path.to_string();
        self.reload(registry);
        self.open = true;
    }

    fn reload(&mut self, registry: &WidgetRegistry) {
        match DashboardConfig::load(&self.path, registry) {
            Ok(cfg) => {
                self.config = cfg;
                self.error = None;
                self.ensure_selected_slot();
            }
            Err(e) => {
                self.error = Some(format!("Failed to load dashboard: {e}"));
            }
        }
    }

    fn save(&mut self) {
        let tmp = format!("{}.tmp", self.path);
        if let Err(e) = self.config.save(&tmp) {
            self.error = Some(format!("Failed to save: {e}"));
            return;
        }
        if let Err(e) = std::fs::rename(&tmp, &self.path) {
            self.error = Some(format!("Failed to finalize save: {e}"));
            return;
        }
        self.pending_save = true;
    }

    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        registry: &WidgetRegistry,
        settings_ctx: WidgetSettingsContext<'_>,
    ) -> bool {
        if !self.open {
            return false;
        }
        let mut should_reload = false;
        let mut open = self.open;
        egui::Window::new("Dashboard Editor")
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if let Some(err) = &self.error {
                        ui.colored_label(egui::Color32::RED, err);
                    }

                    ui.horizontal(|ui| {
                        ui.label("Rows");
                        if ui
                            .add(
                                egui::DragValue::new(&mut self.config.grid.rows)
                                    .clamp_range(1..=12),
                            )
                            .changed()
                        {
                            self.clamp_all_slots();
                        }
                        ui.label("Cols");
                        if ui
                            .add(
                                egui::DragValue::new(&mut self.config.grid.cols)
                                    .clamp_range(1..=12),
                            )
                            .changed()
                        {
                            self.clamp_all_slots();
                        }
                    });

                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Preview");
                        ui.checkbox(&mut self.show_preview, "Show preview");
                        ui.checkbox(&mut self.snap_on_edit, "Snap to free space on edit");
                        ui.checkbox(&mut self.show_swap_buttons, "Show swap");
                        ui.checkbox(&mut self.show_remove_buttons, "Show remove");
                        if ui.button("Auto-place nearest").clicked() {
                            if let Some(idx) = self.selected_slot {
                                if let Err(err) = self.auto_place(idx, registry) {
                                    self.blocked_warning = Some(err);
                                }
                            }
                        }
                        if ui.button("Compact layout").clicked() {
                            if let Err(err) = self.compact_layout(registry) {
                                self.blocked_warning = Some(err);
                            }
                        }
                    });
                    let (_, mut warnings) =
                        crate::dashboard::layout::normalize_slots(&self.config, registry);
                    if let Some(err) = &self.blocked_warning {
                        warnings.push(err.clone());
                    }
                    if !warnings.is_empty() {
                        warnings.sort();
                        warnings.dedup();
                        for warn in warnings {
                            ui.colored_label(egui::Color32::YELLOW, warn);
                        }
                        if let Some(idx) = self.selected_slot {
                            if ui.button("Auto-place nearest free slot").clicked() {
                                if let Err(err) = self.auto_place(idx, registry) {
                                    self.blocked_warning = Some(err);
                                }
                            }
                        }
                    }
                    let rows = self.config.grid.rows.max(1) as usize;
                    let cols = self.config.grid.cols.max(1) as usize;
                    let occupancy = self.occupancy_map(rows, cols, None);
                    let conflict_messages = self.conflict_messages(&occupancy, rows, cols);
                    let has_conflicts = occupancy.slot_conflicts.iter().any(|c| *c);
                    if self.show_preview {
                        self.preview(ui, registry, &occupancy);
                    }

                    if !conflict_messages.is_empty() {
                        ui.separator();
                        for msg in &conflict_messages {
                            ui.colored_label(egui::Color32::RED, msg);
                        }
                    }

                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Add slot").clicked() {
                            self.config
                                .slots
                                .push(SlotConfig::with_widget("weather_site", 0, 0));
                        }
                        if ui.button("Reload from disk").clicked() {
                            self.reload(registry);
                        }
                        ui.add_enabled_ui(!has_conflicts, |ui| {
                            if ui.button("Save").clicked() {
                                self.save();
                            }
                        });
                        if has_conflicts {
                            ui.colored_label(
                                egui::Color32::RED,
                                "Resolve slot conflicts before saving.",
                            );
                        }
                        if ui.button("Expand all").clicked() {
                            self.slot_expand_all = true;
                            self.slot_collapse_all = false;
                        }
                        if ui.button("Collapse all").clicked() {
                            self.slot_collapse_all = true;
                            self.slot_expand_all = false;
                        }
                        if ui.button("Split H").clicked() {
                            if let Err(err) =
                                self.split_selected_slot(registry, SplitDirection::Horizontal)
                            {
                                self.blocked_warning = Some(err);
                            }
                        }
                        if ui.button("Split V").clicked() {
                            if let Err(err) =
                                self.split_selected_slot(registry, SplitDirection::Vertical)
                            {
                                self.blocked_warning = Some(err);
                            }
                        }
                    });

                    ui.separator();
                    let mut confirm_remove = None;
                    if let Some(idx) = self.confirm_remove_slot {
                        if let Some(slot) = self.config.slots.get(idx) {
                            let label = Self::slot_label(slot);
                            let mut open = true;
                            let mut should_close = false;
                            egui::Window::new("Confirm remove")
                                .collapsible(false)
                                .resizable(false)
                                .open(&mut open)
                                .show(ctx, |ui| {
                                    ui.label(format!("Remove slot '{label}'?"));
                                    ui.horizontal(|ui| {
                                        if ui.button("Remove").clicked() {
                                            confirm_remove = Some(idx);
                                            should_close = true;
                                        }
                                        if ui.button("Cancel").clicked() {
                                            should_close = true;
                                        }
                                    });
                                });
                            if should_close {
                                open = false;
                            }
                            if !open {
                                self.confirm_remove_slot = None;
                            }
                        } else {
                            self.confirm_remove_slot = None;
                        }
                    }
                    let mut idx = 0;
                    while idx < self.config.slots.len() {
                        let original_slot = self.config.slots[idx].clone();
                        let mut slot = original_slot.clone();
                        let removed = confirm_remove == Some(idx);
                        let mut edited = false;
                        let mut swapped = false;
                        ui.push_id(idx, |ui| {
                            let collapsing_id = ui.id().with(("slot-collapse", idx));
                            let mut state = CollapsingState::load_with_default_open(
                                ui.ctx(),
                                collapsing_id,
                                true,
                            );
                            if self.slot_expand_all {
                                state.set_open(true);
                                state.store(ui.ctx());
                            } else if self.slot_collapse_all {
                                state.set_open(false);
                                state.store(ui.ctx());
                            }
                            let header_response = state.show_header(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(format!("Slot {idx}"));
                                    ui.label("Label");
                                    let id = slot.id.get_or_insert_with(|| format!("slot-{idx}"));
                                    edited |= ui.text_edit_singleline(id).changed();
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if self.show_remove_buttons
                                                && ui.button("Remove").clicked()
                                            {
                                                self.confirm_remove_slot = Some(idx);
                                            }
                                            if ui.button("Select").clicked() {
                                                self.selected_slot = Some(idx);
                                            }
                                            if self.show_swap_buttons {
                                                if let Some(selected_idx) = self.selected_slot {
                                                    if selected_idx != idx
                                                        && ui.button("Swap with selected").clicked()
                                                    {
                                                        if let Err(err) = self.swap_slots(
                                                            selected_idx,
                                                            idx,
                                                            registry,
                                                        ) {
                                                            self.blocked_warning = Some(err);
                                                        }
                                                        self.swap_anchor = None;
                                                        swapped = true;
                                                    }
                                                }
                                                let is_swap_source = self.swap_anchor == Some(idx);
                                                let swap_label = if is_swap_source {
                                                    "Swap source"
                                                } else {
                                                    "Swap"
                                                };
                                                let swap_button = if is_swap_source {
                                                    egui::Button::new(swap_label)
                                                        .fill(egui::Color32::from_rgb(
                                                            60, 120, 200,
                                                        ))
                                                } else {
                                                    egui::Button::new(swap_label)
                                                };
                                                if ui.add(swap_button).clicked() {
                                                    if is_swap_source {
                                                        self.swap_anchor = None;
                                                    } else if let Some(anchor) = self.swap_anchor {
                                                        if let Err(err) =
                                                            self.swap_slots(anchor, idx, registry)
                                                        {
                                                            self.blocked_warning = Some(err);
                                                        }
                                                        self.swap_anchor = None;
                                                        swapped = true;
                                                    } else {
                                                        self.swap_anchor = Some(idx);
                                                    }
                                                }
                                            }
                                        },
                                    );
                                });
                            });
                            let (_, _, body) = header_response.body_unindented(|ui| {
                                egui::ComboBox::from_label("Widget type")
                                    .selected_text(slot.widget.clone())
                                    .show_ui(ui, |ui| {
                                        for name in registry.names() {
                                            if ui
                                                .selectable_value(
                                                    &mut slot.widget,
                                                    name.clone(),
                                                    name,
                                                )
                                                .changed()
                                            {
                                                edited = true;
                                            }
                                        }
                                    });
                                ui.horizontal(|ui| {
                                    ui.label("Row");
                                    let rows = self.config.grid.rows.max(1) as i32;
                                    let cols = self.config.grid.cols.max(1) as i32;
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut slot.row)
                                                .clamp_range(0..=rows.saturating_sub(1)),
                                        )
                                        .changed()
                                    {
                                        Self::clamp_position_only(&mut slot, rows, cols);
                                        slot.row_span =
                                            slot.row_span.min(self.max_row_span_for(&slot));
                                        slot.col_span =
                                            slot.col_span.min(self.max_col_span_for(&slot));
                                        edited = true;
                                    }
                                    ui.label("Col");
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut slot.col)
                                                .clamp_range(0..=cols.saturating_sub(1)),
                                        )
                                        .changed()
                                    {
                                        Self::clamp_position_only(&mut slot, rows, cols);
                                        slot.row_span =
                                            slot.row_span.min(self.max_row_span_for(&slot));
                                        slot.col_span =
                                            slot.col_span.min(self.max_col_span_for(&slot));
                                        edited = true;
                                    }
                                });
                                ui.horizontal(|ui| {
                                    ui.label("Row span");
                                    let max_row_span = self.max_row_span_for(&slot);
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut slot.row_span)
                                                .clamp_range(1..=max_row_span),
                                        )
                                        .changed()
                                    {
                                        slot.row_span = slot.row_span.min(max_row_span);
                                        edited = true;
                                    }
                                    ui.label("Col span");
                                    let max_col_span = self.max_col_span_for(&slot);
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut slot.col_span)
                                                .clamp_range(1..=max_col_span),
                                        )
                                        .changed()
                                    {
                                        slot.col_span = slot.col_span.min(max_col_span);
                                        edited = true;
                                    }
                                });
                                egui::ComboBox::from_label("Overflow")
                                    .selected_text(slot.overflow.as_str())
                                    .show_ui(ui, |ui| {
                                        for mode in [
                                            OverflowMode::Scroll,
                                            OverflowMode::Clip,
                                            OverflowMode::Auto,
                                        ] {
                                            ui.selectable_value(
                                                &mut slot.overflow,
                                                mode,
                                                mode.as_str(),
                                            );
                                        }
                                    });
                                ui.separator();
                            });
                            if body.is_none() {
                                // Ensure state is stored even when collapsed
                            }
                        });
                        if removed {
                            self.config.slots.remove(idx);
                            if let Some(sel) = self.selected_slot {
                                if sel >= idx && !self.config.slots.is_empty() {
                                    let next =
                                        sel.saturating_sub(1).min(self.config.slots.len() - 1);
                                    self.selected_slot = Some(next);
                                } else if self.config.slots.is_empty() {
                                    self.selected_slot = None;
                                }
                            }
                            if let Some(anchor) = self.swap_anchor {
                                if anchor == idx {
                                    self.swap_anchor = None;
                                } else if anchor > idx {
                                    self.swap_anchor = Some(anchor - 1);
                                }
                            }
                            self.ensure_selected_slot();
                            self.ensure_swap_anchor();
                        } else if swapped {
                            idx += 1;
                        } else if edited && slot != original_slot {
                            if let Err(err) = self.commit_slot(idx, slot, registry) {
                                if self.blocked_warning.is_none() {
                                    self.blocked_warning = Some(err);
                                }
                            }
                            idx += 1;
                        } else {
                            idx += 1;
                        }
                    }
                    ui.separator();
                    self.render_selected_settings(ui, registry, &settings_ctx);
                    // Reset batch flags after applying once
                    self.slot_expand_all = false;
                    self.slot_collapse_all = false;
                });
            });
        self.open = open;
        if self.pending_save {
            self.pending_save = false;
            should_reload = true;
        }
        should_reload
    }

    fn clamp_all_slots(&mut self) {
        let rows = self.config.grid.rows.max(1) as usize;
        let cols = self.config.grid.cols.max(1) as usize;
        for slot in &mut self.config.slots {
            Self::clamp_slot(slot, rows, cols);
        }
        self.ensure_selected_slot();
    }

    fn clamp_slot(slot: &mut SlotConfig, rows: usize, cols: usize) -> bool {
        let original = slot.clone();
        let last_row = rows.saturating_sub(1) as i32;
        let last_col = cols.saturating_sub(1) as i32;
        slot.row = slot.row.clamp(0, last_row);
        slot.col = slot.col.clamp(0, last_col);
        let remaining_rows = rows.saturating_sub(slot.row as usize);
        let remaining_cols = cols.saturating_sub(slot.col as usize);
        slot.row_span = slot.row_span.max(1).min(remaining_rows.max(1) as u8);
        slot.col_span = slot.col_span.max(1).min(remaining_cols.max(1) as u8);
        *slot != original
    }

    fn clamp_position_only(slot: &mut SlotConfig, rows: i32, cols: i32) {
        let last_row = rows.saturating_sub(1);
        let last_col = cols.saturating_sub(1);
        slot.row = slot.row.clamp(0, last_row);
        slot.col = slot.col.clamp(0, last_col);
    }

    fn ensure_selected_slot(&mut self) {
        if self.config.slots.is_empty() {
            self.selected_slot = None;
            return;
        }
        if let Some(idx) = self.selected_slot {
            if idx >= self.config.slots.len() {
                self.selected_slot = Some(self.config.slots.len().saturating_sub(1));
            }
        }
        self.ensure_swap_anchor();
    }

    fn ensure_swap_anchor(&mut self) {
        if self.config.slots.is_empty() {
            self.swap_anchor = None;
            return;
        }
        if let Some(idx) = self.swap_anchor {
            if idx >= self.config.slots.len() {
                self.swap_anchor = None;
            }
        }
    }

    fn render_selected_settings(
        &mut self,
        ui: &mut egui::Ui,
        registry: &WidgetRegistry,
        settings_ctx: &WidgetSettingsContext<'_>,
    ) {
        ui.heading("Settings");
        if self.config.slots.is_empty() {
            ui.label("Add a slot to edit its settings.");
            return;
        }
        let Some(selected_idx) = self.selected_slot else {
            ui.label("Select a slot to edit settings.");
            return;
        };
        if selected_idx >= self.config.slots.len() {
            ui.colored_label(
                egui::Color32::YELLOW,
                "Selected slot no longer exists; please reselect.",
            );
            return;
        }

        let slot = &mut self.config.slots[selected_idx];
        let widget_name = slot.widget.clone();
        let default_settings = registry
            .default_settings(&widget_name)
            .unwrap_or_else(|| Value::Object(Default::default()));
        let mut settings_changed = Self::ensure_slot_settings_defaults(slot, &default_settings, ui);

        ui.horizontal(|ui| {
            ui.label(format!("Widget: {widget_name}"));
            if let Some(meta) = registry.metadata_for(&widget_name) {
                if meta.has_settings_ui {
                    ui.colored_label(egui::Color32::GREEN, "Settings available");
                } else {
                    ui.colored_label(egui::Color32::GRAY, "No custom settings UI");
                }
            }
            if ui.button("Reset to defaults").clicked() {
                slot.settings = default_settings.clone();
                settings_changed = true;
            }
        });

        if let Some(result) =
            registry.render_settings_ui(&widget_name, ui, &mut slot.settings, settings_ctx)
        {
            if result.changed {
                settings_changed = true;
            }
            if let Some(err) = result.error {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    format!("{err}. Settings saved after edits."),
                );
            }
        } else {
            ui.label("No settings available for this widget.");
        }

        if settings_changed {
            self.blocked_warning = None;
        }
    }

    fn swap_slots(
        &mut self,
        first: usize,
        second: usize,
        registry: &WidgetRegistry,
    ) -> Result<(), String> {
        if first >= self.config.slots.len() || second >= self.config.slots.len() {
            return Err("Invalid slot selection".into());
        }
        if first == second {
            return Ok(());
        }
        let rows = self.config.grid.rows.max(1) as usize;
        let cols = self.config.grid.cols.max(1) as usize;
        let original_first = self.config.slots[first].clone();
        let original_second = self.config.slots[second].clone();

        {
            let (left, right) = if first < second {
                let (left, right) = self.config.slots.split_at_mut(second);
                (&mut left[first], &mut right[0])
            } else {
                let (left, right) = self.config.slots.split_at_mut(first);
                (&mut right[0], &mut left[second])
            };
            std::mem::swap(&mut left.widget, &mut right.widget);
            std::mem::swap(&mut left.settings, &mut right.settings);
            std::mem::swap(&mut left.id, &mut right.id);
        }

        let first_slot = self.config.slots[first].clone();
        let second_slot = self.config.slots[second].clone();
        let occupancy_first = self.occupancy_map(rows, cols, Some(first));
        let occupancy_second = self.occupancy_map(rows, cols, Some(second));
        let validated_first =
            self.validate_slot(first, first_slot, rows, cols, registry, &occupancy_first);
        let validated_second =
            self.validate_slot(second, second_slot, rows, cols, registry, &occupancy_second);

        match (validated_first, validated_second) {
            (Ok(first_slot), Ok(second_slot)) => {
                self.config.slots[first] = first_slot;
                self.config.slots[second] = second_slot;
                self.blocked_warning = None;
                Ok(())
            }
            (Err(err), _) | (_, Err(err)) => {
                self.config.slots[first] = original_first;
                self.config.slots[second] = original_second;
                Err(err)
            }
        }
    }

    fn max_row_span_for(&self, slot: &SlotConfig) -> u8 {
        let rows = self.config.grid.rows.max(1) as usize;
        let row = slot.row.max(0) as usize;
        rows.saturating_sub(row).max(1) as u8
    }

    fn max_col_span_for(&self, slot: &SlotConfig) -> u8 {
        let cols = self.config.grid.cols.max(1) as usize;
        let col = slot.col.max(0) as usize;
        cols.saturating_sub(col).max(1) as u8
    }

    fn ensure_slot_settings_defaults(
        slot: &mut SlotConfig,
        default_settings: &Value,
        ui: &mut egui::Ui,
    ) -> bool {
        if slot.settings.is_null() {
            ui.colored_label(
                egui::Color32::YELLOW,
                "Settings were empty; defaults were applied.",
            );
            slot.settings = default_settings.clone();
            return true;
        }
        false
    }

    fn commit_slot(
        &mut self,
        idx: usize,
        slot: SlotConfig,
        registry: &WidgetRegistry,
    ) -> Result<(), String> {
        if idx >= self.config.slots.len() {
            return Err("Invalid slot selection".into());
        }
        let rows = self.config.grid.rows.max(1) as usize;
        let cols = self.config.grid.cols.max(1) as usize;
        let occupancy = self.occupancy_map(rows, cols, Some(idx));
        match self.validate_slot(idx, slot.clone(), rows, cols, registry, &occupancy) {
            Ok(clamped) => {
                self.blocked_warning = None;
                self.config.slots[idx] = clamped;
                Ok(())
            }
            Err(err) => {
                if self.snap_on_edit {
                    match self.auto_place_slot(idx, slot, registry) {
                        Ok(placed) => {
                            self.blocked_warning = Some(
                                "Slot snapped to the nearest free space after a conflict.".into(),
                            );
                            self.config.slots[idx] = placed;
                            return Ok(());
                        }
                        Err(auto_err) => {
                            self.blocked_warning =
                                Some(format!("{err}; auto-place failed: {auto_err}"));
                        }
                    }
                } else {
                    self.blocked_warning = Some(err.clone());
                }
                Err(err)
            }
        }
    }

    fn validate_slot(
        &self,
        idx: usize,
        slot: SlotConfig,
        rows: usize,
        cols: usize,
        registry: &WidgetRegistry,
        occupancy: &OccupancyMap,
    ) -> Result<SlotConfig, String> {
        let target_label = Self::slot_label(&slot);
        let mut clamped = slot.clone();
        let changed = Self::clamp_slot(&mut clamped, rows, cols);
        if changed
            && (clamped.row != slot.row
                || clamped.col != slot.col
                || clamped.row_span != slot.row_span
                || clamped.col_span != slot.col_span)
        {
            return Err(format!(
                "slot '{}' exceeds the configured grid bounds",
                target_label
            ));
        }
        let coverage = Self::coverage_for_slot(&clamped, rows, cols);
        if coverage.out_of_bounds {
            return Err(format!(
                "slot '{}' exceeds the configured grid bounds",
                target_label
            ));
        }
        for (r, c) in &coverage.cells {
            if let Some(conflict_idx) = occupancy.owners[*r][*c].first() {
                let other_label = self
                    .config
                    .slots
                    .get(*conflict_idx)
                    .map(Self::slot_label)
                    .unwrap_or_else(|| "another slot".to_string());
                return Err(format!(
                    "slot '{}' overlaps '{}' at row {}, col {}",
                    target_label, other_label, r, c
                ));
            }
        }

        let mut cfg = self.config.clone();
        cfg.slots[idx] = clamped.clone();
        let (_, warnings) = crate::dashboard::layout::normalize_slots(&cfg, registry);
        if let Some(conflict) = warnings.iter().find(|w| {
            w.contains("overlaps") || w.contains("outside") || w.contains("negative position")
        }) {
            return Err(conflict.clone());
        }

        Ok(clamped)
    }

    fn preview(&mut self, ui: &mut egui::Ui, registry: &WidgetRegistry, occupancy: &OccupancyMap) {
        let rows = self.config.grid.rows.max(1) as usize;
        let cols = self.config.grid.cols.max(1) as usize;
        if rows == 0 || cols == 0 {
            return;
        }

        self.ensure_selected_slot();
        if self.selected_slot.is_none() && !self.config.slots.is_empty() {
            self.selected_slot = Some(0);
        }
        let occupancy_without_selected = self.occupancy_map(rows, cols, self.selected_slot);

        let cell_size = 60.0;
        let grid_size = egui::vec2(
            (cols as f32).max(1.0) * cell_size,
            (rows as f32).max(1.0) * cell_size,
        );
        let (response, painter) = ui.allocate_painter(grid_size, egui::Sense::click_and_drag());
        let rect = response.rect;
        let row_h = rect.height() / rows as f32;
        let col_w = rect.width() / cols as f32;

        for r in 0..rows {
            for c in 0..cols {
                let cell_rect = egui::Rect::from_min_size(
                    rect.min + egui::vec2(col_w * c as f32, row_h * r as f32),
                    egui::vec2(col_w, row_h),
                );
                if !occupancy.owners[r][c].is_empty() {
                    let fill = if occupancy.collisions[r][c] {
                        egui::Color32::from_rgba_unmultiplied(200, 64, 64, 60)
                    } else {
                        ui.visuals().faint_bg_color
                    };
                    painter.rect_filled(cell_rect, 0.0, fill);
                }
            }
        }

        for r in 0..=rows {
            let y = rect.top() + r as f32 * row_h;
            painter.line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                (1.0, ui.visuals().weak_text_color()),
            );
        }
        for c in 0..=cols {
            let x = rect.left() + c as f32 * col_w;
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                (1.0, ui.visuals().weak_text_color()),
            );
        }

        let selected_slot_cfg = self
            .selected_slot
            .and_then(|idx| self.config.slots.get(idx))
            .cloned();

        for (idx, slot) in self.config.slots.iter().enumerate() {
            let mut display_slot = slot.clone();
            Self::clamp_slot(&mut display_slot, rows, cols);
            let coverage = Self::coverage_for_slot(&display_slot, rows, cols);
            if coverage.cells.is_empty() {
                continue;
            }
            let slot_rect = egui::Rect::from_min_size(
                rect.min
                    + egui::vec2(
                        col_w * display_slot.col.max(0) as f32,
                        row_h * display_slot.row.max(0) as f32,
                    ),
                egui::vec2(
                    col_w * display_slot.col_span as f32,
                    row_h * display_slot.row_span as f32,
                ),
            );
            let is_selected = selected_slot_cfg.as_ref().map_or(false, |selected| {
                let selected_row = selected.row.max(0);
                let selected_col = selected.col.max(0);
                (display_slot.id.is_some() && display_slot.id == selected.id)
                    || (display_slot.id.is_none()
                        && selected.id.is_none()
                        && display_slot.widget == selected.widget
                        && display_slot.row == selected_row
                        && display_slot.col == selected_col)
            });
            let has_conflict = occupancy.slot_conflicts.get(idx).copied().unwrap_or(false);
            let fill = if has_conflict {
                egui::Color32::from_rgba_unmultiplied(200, 64, 64, 70)
            } else if is_selected {
                ui.visuals().selection.bg_fill.gamma_multiply(0.35)
            } else {
                ui.visuals().faint_bg_color
            };
            painter.rect_filled(slot_rect, 2.0, fill);
            painter.rect_stroke(
                slot_rect,
                2.0,
                (
                    2.0,
                    if has_conflict {
                        egui::Color32::from_rgb(200, 64, 64)
                    } else if is_selected {
                        ui.visuals().selection.stroke.color
                    } else {
                        ui.visuals().window_stroke().color
                    },
                ),
            );
            let slot_label = Self::slot_label(&display_slot);
            let slot_response = ui.interact(
                slot_rect,
                ui.id().with(("preview-slot", idx)),
                egui::Sense::hover(),
            );
            slot_response.on_hover_text(format!(
                "{slot_label} (row {}, col {}, span {}x{})",
                display_slot.row, display_slot.col, display_slot.row_span, display_slot.col_span
            ));
        }

        let mut drag_preview: Option<(usize, usize, usize, usize, bool)> = None;
        if response.drag_started() {
            if let Some(pos) = response.interact_pointer_pos() {
                let cell = self.point_to_cell(pos, rect, rows, cols);
                self.drag_anchor = Some(cell);
            }
        }
        if let (Some(start), Some(pos)) = (self.drag_anchor, response.interact_pointer_pos()) {
            let end = self.point_to_cell(pos, rect, rows, cols);
            let top_row = start.0.min(end.0);
            let left_col = start.1.min(end.1);
            let row_span = start.0.max(end.0) - top_row + 1;
            let col_span = start.1.max(end.1) - left_col + 1;
            let mut conflict = top_row + row_span > rows || left_col + col_span > cols;
            if !conflict {
                for r in top_row..top_row + row_span {
                    for c in left_col..left_col + col_span {
                        if !occupancy_without_selected.owners[r][c].is_empty() {
                            conflict = true;
                            break;
                        }
                    }
                    if conflict {
                        break;
                    }
                }
            }
            drag_preview = Some((top_row, left_col, row_span, col_span, conflict));
        }
        if response.drag_stopped() {
            if let (Some(start), Some(pos)) =
                (self.drag_anchor.take(), response.interact_pointer_pos())
            {
                let end = self.point_to_cell(pos, rect, rows, cols);
                let top_row = start.0.min(end.0);
                let left_col = start.1.min(end.1);
                let row_span = start.0.max(end.0) - top_row + 1;
                let col_span = start.1.max(end.1) - left_col + 1;
                if let Some(idx) = self.selected_slot {
                    if idx >= self.config.slots.len() {
                        self.blocked_warning =
                            Some("Selected slot no longer exists; please reselect".into());
                        return;
                    }
                    let res = self.commit_slot(
                        idx,
                        self.updated_slot(
                            idx,
                            top_row as i32,
                            left_col as i32,
                            row_span as u8,
                            col_span as u8,
                        ),
                        registry,
                    );
                    if let Err(err) = res {
                        if self.blocked_warning.is_none() {
                            self.blocked_warning = Some(err);
                        }
                    }
                }
            }
        } else if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let (row, col) = self.point_to_cell(pos, rect, rows, cols);
                self.drag_anchor = None;
                if let Some(idx) = self.selected_slot {
                    if idx >= self.config.slots.len() {
                        self.blocked_warning =
                            Some("Selected slot no longer exists; please reselect".into());
                        return;
                    }
                    let res = self.commit_slot(
                        idx,
                        self.updated_slot(
                            idx,
                            row as i32,
                            col as i32,
                            self.config.slots[idx].row_span,
                            self.config.slots[idx].col_span,
                        ),
                        registry,
                    );
                    if let Err(err) = res {
                        if self.blocked_warning.is_none() {
                            self.blocked_warning = Some(err);
                        }
                    }
                }
            }
        }

        if let Some((row, col, row_span, col_span, conflict)) = drag_preview {
            let preview_rect = egui::Rect::from_min_size(
                rect.min + egui::vec2(col_w * col as f32, row_h * row as f32),
                egui::vec2(col_w * col_span as f32, row_h * row_span as f32),
            );
            let fill = if conflict {
                egui::Color32::from_rgba_unmultiplied(200, 64, 64, 60)
            } else {
                egui::Color32::from_rgba_unmultiplied(64, 200, 120, 60)
            };
            painter.rect_filled(preview_rect, 2.0, fill);
            painter.rect_stroke(
                preview_rect,
                2.0,
                (
                    2.0,
                    if conflict {
                        egui::Color32::from_rgb(200, 64, 64)
                    } else {
                        ui.visuals().selection.stroke.color
                    },
                ),
            );
            if conflict {
                for r in row..row + row_span {
                    for c in col..col + col_span {
                        if !occupancy_without_selected.owners[r][c].is_empty() {
                            let cell_rect = egui::Rect::from_min_size(
                                rect.min + egui::vec2(col_w * c as f32, row_h * r as f32),
                                egui::vec2(col_w, row_h),
                            );
                            painter.rect_stroke(
                                cell_rect.shrink(1.0),
                                1.0,
                                (1.0, egui::Color32::from_rgb(200, 64, 64)),
                            );
                        }
                    }
                }
            }
        }
    }

    fn updated_slot(
        &self,
        idx: usize,
        row: i32,
        col: i32,
        row_span: u8,
        col_span: u8,
    ) -> SlotConfig {
        let mut slot = self.config.slots[idx].clone();
        slot.row = row;
        slot.col = col;
        slot.row_span = row_span;
        slot.col_span = col_span;
        slot
    }

    fn coverage_for_slot(slot: &SlotConfig, rows: usize, cols: usize) -> SlotCoverage {
        if slot.row < 0 || slot.col < 0 {
            return SlotCoverage {
                cells: Vec::new(),
                out_of_bounds: true,
            };
        }
        let row = slot.row as usize;
        let col = slot.col as usize;
        if row >= rows || col >= cols {
            return SlotCoverage {
                cells: Vec::new(),
                out_of_bounds: true,
            };
        }
        let max_row_span = rows.saturating_sub(row).max(1);
        let max_col_span = cols.saturating_sub(col).max(1);
        let desired_row_span = slot.row_span.max(1) as usize;
        let desired_col_span = slot.col_span.max(1) as usize;
        let row_span = desired_row_span.min(max_row_span);
        let col_span = desired_col_span.min(max_col_span);
        let mut cells = Vec::with_capacity(row_span * col_span);
        for r in row..row + row_span {
            for c in col..col + col_span {
                cells.push((r, c));
            }
        }
        SlotCoverage {
            cells,
            out_of_bounds: desired_row_span > max_row_span || desired_col_span > max_col_span,
        }
    }

    fn build_occupancy_map_for(
        slots: &[SlotConfig],
        rows: usize,
        cols: usize,
        exclude: Option<usize>,
    ) -> OccupancyMap {
        let mut map = OccupancyMap {
            owners: vec![vec![Vec::new(); cols]; rows],
            collisions: vec![vec![false; cols]; rows],
            slot_conflicts: vec![false; slots.len()],
        };

        for (idx, slot) in slots.iter().enumerate() {
            if Some(idx) == exclude {
                continue;
            }
            let coverage = Self::coverage_for_slot(slot, rows, cols);
            if coverage.out_of_bounds {
                if let Some(flag) = map.slot_conflicts.get_mut(idx) {
                    *flag = true;
                }
            }
            for (r, c) in coverage.cells {
                for owner in &map.owners[r][c] {
                    if let Some(flag) = map.slot_conflicts.get_mut(*owner) {
                        *flag = true;
                    }
                    if let Some(flag) = map.slot_conflicts.get_mut(idx) {
                        *flag = true;
                    }
                }
                map.owners[r][c].push(idx);
                if map.owners[r][c].len() > 1 {
                    map.collisions[r][c] = true;
                }
            }
        }

        map
    }

    fn occupancy_map(&self, rows: usize, cols: usize, exclude: Option<usize>) -> OccupancyMap {
        Self::build_occupancy_map_for(&self.config.slots, rows, cols, exclude)
    }

    fn conflict_messages(&self, occupancy: &OccupancyMap, rows: usize, cols: usize) -> Vec<String> {
        let mut messages = Vec::new();
        let mut seen = HashSet::new();
        for (idx, slot) in self.config.slots.iter().enumerate() {
            if !occupancy.slot_conflicts.get(idx).copied().unwrap_or(false) {
                continue;
            }
            let label = Self::slot_label(slot);
            let coverage = Self::coverage_for_slot(slot, rows, cols);
            if coverage.out_of_bounds {
                messages.push(format!(
                    "Slot '{}' at row {}, col {} exceeds the {}x{} grid.",
                    label, slot.row, slot.col, rows, cols
                ));
            }
            for (r, c) in coverage.cells {
                let owners = occupancy.owners.get(r).and_then(|row| row.get(c));
                if let Some(owners) = owners {
                    if owners.len() > 1 {
                        for other_idx in owners {
                            if *other_idx == idx {
                                continue;
                            }
                            let key = (idx.min(*other_idx), idx.max(*other_idx), r, c);
                            if seen.insert(key) {
                                let other_label = self
                                    .config
                                    .slots
                                    .get(*other_idx)
                                    .map(Self::slot_label)
                                    .unwrap_or_else(|| "another slot".to_string());
                                messages.push(format!(
                                    "Slot '{}' overlaps '{}' at row {}, col {}",
                                    label, other_label, r, c
                                ));
                            }
                        }
                    }
                }
            }
        }
        messages
    }

    fn slot_label(slot: &SlotConfig) -> String {
        slot.id.clone().unwrap_or_else(|| slot.widget.clone())
    }

    fn point_to_cell(
        &self,
        pos: egui::Pos2,
        rect: egui::Rect,
        rows: usize,
        cols: usize,
    ) -> (usize, usize) {
        let row = ((pos.y - rect.top()) / rect.height() * rows as f32)
            .clamp(0.0, rows.saturating_sub(1) as f32)
            .floor() as usize;
        let col = ((pos.x - rect.left()) / rect.width() * cols as f32)
            .clamp(0.0, cols.saturating_sub(1) as f32)
            .floor() as usize;
        (row, col)
    }

    fn auto_place_slot(
        &self,
        idx: usize,
        slot: SlotConfig,
        _registry: &WidgetRegistry,
    ) -> Result<SlotConfig, String> {
        if idx >= self.config.slots.len() {
            return Err("Select a slot to auto-place".into());
        }
        let rows = self.config.grid.rows.max(1) as usize;
        let cols = self.config.grid.cols.max(1) as usize;
        let mut base_cfg = self.config.clone();
        base_cfg.slots[idx] = slot.clone();
        let mut slot = base_cfg.slots.remove(idx);
        Self::clamp_slot(&mut slot, rows, cols);
        let base_occupancy = Self::build_occupancy_map_for(&base_cfg.slots, rows, cols, None);
        if base_occupancy.slot_conflicts.iter().any(|c| *c) {
            return Err("layout has invalid slots".into());
        }
        let preferred_row = slot.row.max(0).min(rows.saturating_sub(1) as i32) as usize;
        let preferred_col = slot.col.max(0).min(cols.saturating_sub(1) as i32) as usize;
        let mut occupied = vec![vec![false; cols]; rows];
        for r in 0..rows {
            for c in 0..cols {
                occupied[r][c] = !base_occupancy.owners[r][c].is_empty();
            }
        }

        let span_r = slot.row_span.max(1) as usize;
        let span_c = slot.col_span.max(1) as usize;
        let mut best: Option<(usize, usize, usize)> = None;
        for r in 0..rows {
            for c in 0..cols {
                if r + span_r > rows || c + span_c > cols {
                    continue;
                }
                if Self::can_fit(&occupied, r, c, span_r, span_c) {
                    let dist = (preferred_row.max(r) - preferred_row.min(r))
                        + (preferred_col.max(c) - preferred_col.min(c));
                    let candidate = (dist, r, c);
                    if best.map_or(true, |current| candidate < current) {
                        best = Some(candidate);
                    }
                }
            }
        }
        if let Some((_, r, c)) = best {
            return Ok(SlotConfig {
                row: r as i32,
                col: c as i32,
                row_span: span_r as u8,
                col_span: span_c as u8,
                ..slot.clone()
            });
        }
        Err("No free space for this span".into())
    }

    fn auto_place(&mut self, idx: usize, registry: &WidgetRegistry) -> Result<(), String> {
        if idx >= self.config.slots.len() {
            return Err("Select a slot to auto-place".into());
        }
        let slot = self.config.slots[idx].clone();
        let placed = self.auto_place_slot(idx, slot, registry)?;
        self.config.slots[idx] = placed;
        self.blocked_warning = None;
        Ok(())
    }

    fn can_fit(
        occupied: &[Vec<bool>],
        row: usize,
        col: usize,
        row_span: usize,
        col_span: usize,
    ) -> bool {
        for r in row..row + row_span {
            for c in col..col + col_span {
                if occupied
                    .get(r)
                    .and_then(|row| row.get(c))
                    .copied()
                    .unwrap_or(true)
                {
                    return false;
                }
            }
        }
        true
    }

    fn compact_layout(&mut self, registry: &WidgetRegistry) -> Result<(), String> {
        let rows = self.config.grid.rows.max(1) as usize;
        let cols = self.config.grid.cols.max(1) as usize;
        let mut placed: Vec<SlotConfig> = Vec::new();

        for slot in &self.config.slots {
            let mut candidate = slot.clone();
            Self::clamp_slot(&mut candidate, rows, cols);
            let mut found = None;
            for r in 0..rows {
                for c in 0..cols {
                    candidate.row = r as i32;
                    candidate.col = c as i32;
                    let mut temp_cfg = DashboardConfig {
                        version: self.config.version,
                        grid: self.config.grid.clone(),
                        slots: placed.clone(),
                    };
                    temp_cfg.slots.push(candidate.clone());
                    let (normalized, warnings) =
                        crate::dashboard::layout::normalize_slots(&temp_cfg, registry);
                    if normalized.len() == temp_cfg.slots.len()
                        && warnings
                            .iter()
                            .all(|w| !w.contains("overlaps") && !w.contains("outside"))
                    {
                        found = Some(candidate.clone());
                        let last = temp_cfg.slots.pop().unwrap();
                        placed.push(last);
                        break;
                    }
                }
                if found.is_some() {
                    break;
                }
            }
            if found.is_none() {
                return Err("Failed to compact layout without collisions".into());
            }
        }

        self.config.slots = placed;
        Ok(())
    }

    fn split_selected_slot(
        &mut self,
        registry: &WidgetRegistry,
        direction: SplitDirection,
    ) -> Result<(), String> {
        let Some(selected_idx) = self.selected_slot else {
            return Err("Select a slot to split.".into());
        };
        if selected_idx >= self.config.slots.len() {
            return Err("Selected slot no longer exists; please reselect.".into());
        }

        let rows = self.config.grid.rows.max(1) as usize;
        let cols = self.config.grid.cols.max(1) as usize;
        let original_slot = self.config.slots[selected_idx].clone();
        let (primary_span, secondary_span) = match direction {
            SplitDirection::Horizontal => {
                if original_slot.row_span <= 1 {
                    return Err("Selected slot cannot be split horizontally.".into());
                }
                let primary = original_slot.row_span / 2;
                (primary.max(1), original_slot.row_span - primary.max(1))
            }
            SplitDirection::Vertical => {
                if original_slot.col_span <= 1 {
                    return Err("Selected slot cannot be split vertically.".into());
                }
                let primary = original_slot.col_span / 2;
                (primary.max(1), original_slot.col_span - primary.max(1))
            }
        };

        let mut updated_slot = original_slot.clone();
        let mut new_slot =
            SlotConfig::with_widget("weather_site", original_slot.row, original_slot.col);
        match direction {
            SplitDirection::Horizontal => {
                updated_slot.row_span = primary_span;
                new_slot.row = original_slot.row + primary_span as i32;
                new_slot.col = original_slot.col;
                new_slot.row_span = secondary_span;
                new_slot.col_span = original_slot.col_span;
            }
            SplitDirection::Vertical => {
                updated_slot.col_span = primary_span;
                new_slot.col = original_slot.col + primary_span as i32;
                new_slot.row = original_slot.row;
                new_slot.col_span = secondary_span;
                new_slot.row_span = original_slot.row_span;
            }
        }

        let original_slots = self.config.slots.clone();
        self.config.slots[selected_idx] = updated_slot.clone();
        self.config.slots.push(new_slot.clone());
        let new_idx = self.config.slots.len() - 1;

        let updated_result = {
            let occupancy = self.occupancy_map(rows, cols, Some(selected_idx));
            self.validate_slot(selected_idx, updated_slot, rows, cols, registry, &occupancy)
        };
        let new_result = {
            let occupancy = self.occupancy_map(rows, cols, Some(new_idx));
            self.validate_slot(new_idx, new_slot, rows, cols, registry, &occupancy)
        };

        match (updated_result, new_result) {
            (Ok(updated_slot), Ok(new_slot)) => {
                self.config.slots[selected_idx] = updated_slot;
                self.config.slots[new_idx] = new_slot;
                self.blocked_warning = None;
                Ok(())
            }
            (Err(err), _) | (_, Err(err)) => {
                self.config.slots = original_slots;
                Err(err)
            }
        }
    }
}
