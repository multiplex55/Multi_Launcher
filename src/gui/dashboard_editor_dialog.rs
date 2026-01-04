use crate::dashboard::config::{DashboardConfig, OverflowMode, SlotConfig};
use crate::dashboard::widgets::{WidgetRegistry, WidgetSettingsContext};
use eframe::egui;
use eframe::egui::collapsing_header::CollapsingState;
use serde_json::Value;

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
    slot_expand_all: bool,
    slot_collapse_all: bool,
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
            slot_expand_all: false,
            slot_collapse_all: false,
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
                        if ui.button("Auto-place").clicked() {
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
                    }
                    if self.show_preview {
                        self.preview(ui, registry);
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
                        if ui.button("Save").clicked() {
                            self.save();
                        }
                        if ui.button("Expand all").clicked() {
                            self.slot_expand_all = true;
                            self.slot_collapse_all = false;
                        }
                        if ui.button("Collapse all").clicked() {
                            self.slot_collapse_all = true;
                            self.slot_expand_all = false;
                        }
                    });

                    ui.separator();
                    let mut idx = 0;
                    while idx < self.config.slots.len() {
                        let original_slot = self.config.slots[idx].clone();
                        let mut slot = original_slot.clone();
                        let mut removed = false;
                        let mut edited = false;
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
                                            if ui.button("Remove").clicked() {
                                                removed = true;
                                            }
                                            if ui.button("Select").clicked() {
                                                self.selected_slot = Some(idx);
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
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut slot.row)
                                                .clamp_range(0..=rows.saturating_sub(1)),
                                        )
                                        .changed()
                                    {
                                        edited = true;
                                    }
                                    ui.label("Col");
                                    let cols = self.config.grid.cols.max(1) as i32;
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut slot.col)
                                                .clamp_range(0..=cols.saturating_sub(1)),
                                        )
                                        .changed()
                                    {
                                        edited = true;
                                    }
                                });
                                ui.horizontal(|ui| {
                                    ui.label("Row span");
                                    let rows = self.config.grid.rows.max(1) as usize;
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut slot.row_span).clamp_range(
                                                1..=rows.saturating_sub(0).max(1) as u8,
                                            ),
                                        )
                                        .changed()
                                    {
                                        edited = true;
                                    }
                                    ui.label("Col span");
                                    let cols = self.config.grid.cols.max(1) as usize;
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut slot.col_span).clamp_range(
                                                1..=cols.saturating_sub(0).max(1) as u8,
                                            ),
                                        )
                                        .changed()
                                    {
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
                                egui::CollapsingHeader::new("Settings")
                                    .default_open(true)
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            if ui.button("Reset to defaults").clicked() {
                                                slot.settings = registry
                                                    .default_settings(&slot.widget)
                                                    .unwrap_or_else(|| {
                                                        Value::Object(Default::default())
                                                    });
                                                edited = true;
                                            }
                                            if slot.settings.is_null() {
                                                ui.colored_label(
                                                    egui::Color32::YELLOW,
                                                    "Settings were empty; defaults will be used.",
                                                );
                                                slot.settings = registry
                                                    .default_settings(&slot.widget)
                                                    .unwrap_or_else(|| {
                                                        Value::Object(Default::default())
                                                    });
                                                edited = true;
                                            }
                                        });

                                        if let Some(result) = registry.render_settings_ui(
                                            &slot.widget,
                                            ui,
                                            &mut slot.settings,
                                            &settings_ctx,
                                        ) {
                                            if result.changed {
                                                edited = true;
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
                                    });
                            });
                            if body.is_none() {
                                // Ensure state is stored even when collapsed
                            }
                        });
                        if removed {
                            self.config.slots.remove(idx);
                            if let Some(sel) = self.selected_slot {
                                if sel == idx {
                                    self.selected_slot = None;
                                } else if sel > idx {
                                    self.selected_slot = Some(sel - 1);
                                }
                            }
                        } else if edited && slot != original_slot {
                            if let Err(err) = self.commit_slot(idx, slot, registry) {
                                self.blocked_warning = Some(err);
                            }
                            idx += 1;
                        } else {
                            idx += 1;
                        }
                    }
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
    }

    fn clamp_slot(slot: &mut SlotConfig, rows: usize, cols: usize) {
        let last_row = rows.saturating_sub(1) as i32;
        let last_col = cols.saturating_sub(1) as i32;
        slot.row = slot.row.clamp(0, last_row);
        slot.col = slot.col.clamp(0, last_col);
        let remaining_rows = rows.saturating_sub(slot.row as usize);
        let remaining_cols = cols.saturating_sub(slot.col as usize);
        slot.row_span = slot.row_span.max(1).min(remaining_rows.max(1) as u8);
        slot.col_span = slot.col_span.max(1).min(remaining_cols.max(1) as u8);
    }

    fn commit_slot(
        &mut self,
        idx: usize,
        mut slot: SlotConfig,
        registry: &WidgetRegistry,
    ) -> Result<(), String> {
        if idx >= self.config.slots.len() {
            return Err("Invalid slot selection".into());
        }
        let rows = self.config.grid.rows.max(1) as usize;
        let cols = self.config.grid.cols.max(1) as usize;
        Self::clamp_slot(&mut slot, rows, cols);

        let mut cfg = self.config.clone();
        cfg.slots[idx] = slot;
        let (normalized, warnings) = crate::dashboard::layout::normalize_slots(&cfg, registry);
        let target_label = cfg.slots[idx]
            .id
            .clone()
            .unwrap_or_else(|| cfg.slots[idx].widget.clone());
        let placed = normalized.iter().any(|s| {
            s.id.as_deref() == cfg.slots[idx].id.as_deref()
                && s.widget == cfg.slots[idx].widget
                && s.row == cfg.slots[idx].row as usize
                && s.col == cfg.slots[idx].col as usize
        });
        if !placed {
            let err = warnings
                .iter()
                .find(|w| w.contains(&target_label))
                .cloned()
                .unwrap_or_else(|| "slot placement collides with another slot".to_string());
            return Err(err);
        }
        if let Some(conflict) = warnings
            .iter()
            .find(|w| w.contains("overlaps") && w.contains(&target_label))
        {
            return Err(conflict.clone());
        }

        self.blocked_warning = None;
        self.config = cfg;
        Ok(())
    }

    fn preview(&mut self, ui: &mut egui::Ui, registry: &WidgetRegistry) {
        let rows = self.config.grid.rows.max(1) as usize;
        let cols = self.config.grid.cols.max(1) as usize;
        if rows == 0 || cols == 0 {
            return;
        }

        if self.selected_slot.is_none() && !self.config.slots.is_empty() {
            self.selected_slot = Some(0);
        }
        let (slots, _) = crate::dashboard::layout::normalize_slots(&self.config, registry);

        let grid_size = egui::vec2((cols as f32).max(1.0) * 40.0, (rows as f32).max(1.0) * 40.0);
        let (response, painter) = ui.allocate_painter(grid_size, egui::Sense::click_and_drag());
        let rect = response.rect;
        let row_h = rect.height() / rows as f32;
        let col_w = rect.width() / cols as f32;

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

        for slot in &slots {
            let slot_rect = egui::Rect::from_min_size(
                rect.min + egui::vec2(col_w * slot.col as f32, row_h * slot.row as f32),
                egui::vec2(col_w * slot.col_span as f32, row_h * slot.row_span as f32),
            );
            let is_selected = selected_slot_cfg.as_ref().map_or(false, |selected| {
                let selected_row = selected.row.max(0) as usize;
                let selected_col = selected.col.max(0) as usize;
                (slot.id.is_some() && slot.id == selected.id)
                    || (slot.id.is_none()
                        && selected.id.is_none()
                        && slot.widget == selected.widget
                        && slot.row == selected_row
                        && slot.col == selected_col)
            });
            let fill = if is_selected {
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
                    if is_selected {
                        ui.visuals().selection.stroke.color
                    } else {
                        ui.visuals().window_stroke().color
                    },
                ),
            );
            painter.text(
                slot_rect.center(),
                egui::Align2::CENTER_CENTER,
                slot.id.as_deref().unwrap_or(&slot.widget),
                egui::FontId::monospace(12.0),
                ui.visuals().strong_text_color(),
            );
        }

        if response.drag_started() {
            if let Some(pos) = response.interact_pointer_pos() {
                let cell = self.point_to_cell(pos, rect, rows, cols);
                self.drag_anchor = Some(cell);
            }
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
                        self.blocked_warning = Some(err);
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
                        self.blocked_warning = Some(err);
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

    fn auto_place(&mut self, idx: usize, registry: &WidgetRegistry) -> Result<(), String> {
        if idx >= self.config.slots.len() {
            return Err("Select a slot to auto-place".into());
        }
        let rows = self.config.grid.rows.max(1) as usize;
        let cols = self.config.grid.cols.max(1) as usize;
        let mut base_cfg = self.config.clone();
        let mut slot = base_cfg.slots.remove(idx);
        Self::clamp_slot(&mut slot, rows, cols);
        let (normalized, warnings) = crate::dashboard::layout::normalize_slots(&base_cfg, registry);
        if normalized.len() < base_cfg.slots.len() {
            return Err(warnings
                .get(0)
                .cloned()
                .unwrap_or_else(|| "layout has invalid slots".into()));
        }
        let mut occupied = vec![vec![false; cols]; rows];
        for s in normalized {
            for r in s.row..s.row + s.row_span {
                for c in s.col..s.col + s.col_span {
                    if r < rows && c < cols {
                        occupied[r][c] = true;
                    }
                }
            }
        }

        let span_r = slot.row_span.max(1) as usize;
        let span_c = slot.col_span.max(1) as usize;
        for r in 0..rows {
            for c in 0..cols {
                if r + span_r > rows || c + span_c > cols {
                    continue;
                }
                if Self::can_fit(&occupied, r, c, span_r, span_c) {
                    return self.commit_slot(
                        idx,
                        SlotConfig {
                            row: r as i32,
                            col: c as i32,
                            row_span: span_r as u8,
                            col_span: span_c as u8,
                            ..slot.clone()
                        },
                        registry,
                    );
                }
            }
        }
        Err("No free space for this span".into())
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
}
