use crate::dashboard::config::{DashboardConfig, OverflowMode, SlotConfig};
use crate::dashboard::widgets::{
    WidgetPropertyIdentityStore, WidgetRegistry, WidgetSettingsContext,
};
use crate::gui::confirmation_modal::{ConfirmationModal, ConfirmationResult, DestructiveAction};
use crate::gui::jump_mode::ui::DashboardEditorConfirmationKind;
use crate::gui::theme::widgets::{self, ButtonEmphasis, StatusTone};
use eframe::egui;
use eframe::egui::collapsing_header::CollapsingState;
use serde_json::Value;
use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;

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

struct DashboardEditorDraftSnapshot {
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
    confirm_modal: ConfirmationModal,
    pending_reset: Option<(usize, Value)>,
    pending_confirmation: Option<PendingDashboardEditorConfirmation>,
}

impl DashboardEditorDraftSnapshot {
    fn capture(dialog: &DashboardEditorDialog) -> Self {
        Self {
            config: dialog.config.clone(),
            error: dialog.error.clone(),
            pending_save: dialog.pending_save,
            selected_slot: dialog.selected_slot,
            show_preview: dialog.show_preview,
            blocked_warning: dialog.blocked_warning.clone(),
            drag_anchor: dialog.drag_anchor,
            swap_anchor: dialog.swap_anchor,
            slot_expand_all: dialog.slot_expand_all,
            slot_collapse_all: dialog.slot_collapse_all,
            snap_on_edit: dialog.snap_on_edit,
            show_swap_buttons: dialog.show_swap_buttons,
            show_remove_buttons: dialog.show_remove_buttons,
            confirm_remove_slot: dialog.confirm_remove_slot,
            confirm_modal: dialog.confirm_modal.clone(),
            pending_reset: dialog.pending_reset.clone(),
            pending_confirmation: dialog.pending_confirmation,
        }
    }

    fn restore(self, dialog: &mut DashboardEditorDialog) {
        dialog.config = self.config;
        dialog.error = self.error;
        dialog.pending_save = self.pending_save;
        dialog.selected_slot = self.selected_slot;
        dialog.show_preview = self.show_preview;
        dialog.blocked_warning = self.blocked_warning;
        dialog.drag_anchor = self.drag_anchor;
        dialog.swap_anchor = self.swap_anchor;
        dialog.slot_expand_all = self.slot_expand_all;
        dialog.slot_collapse_all = self.slot_collapse_all;
        dialog.snap_on_edit = self.snap_on_edit;
        dialog.show_swap_buttons = self.show_swap_buttons;
        dialog.show_remove_buttons = self.show_remove_buttons;
        dialog.confirm_remove_slot = self.confirm_remove_slot;
        dialog.confirm_modal = self.confirm_modal;
        dialog.pending_reset = self.pending_reset;
        dialog.pending_confirmation = self.pending_confirmation;
    }
}

#[derive(Clone, Copy)]
struct PendingDashboardEditorConfirmation {
    kind: DashboardEditorConfirmationKind,
    instance: u64,
    slot_token: u64,
}

fn register_editor_target(
    scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    ui: &egui::Ui,
    response: &egui::Response,
    identity: impl AsRef<str>,
    label: &'static str,
    kind: crate::gui::jump_mode::TargetKind,
    visual_order: &mut u64,
) -> Option<crate::gui::jump_mode::ui::FocusDirective> {
    crate::gui::jump_mode::ui::register_and_apply_target(
        scope,
        ui,
        response,
        identity,
        label,
        kind,
        visual_order,
    )
}

fn register_editor_target_at_order(
    scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    ui: &egui::Ui,
    response: &egui::Response,
    identity: impl AsRef<str>,
    label: &'static str,
    kind: crate::gui::jump_mode::TargetKind,
    visual_order: u64,
) -> Option<crate::gui::jump_mode::ui::FocusDirective> {
    let mut next_order = visual_order;
    register_editor_target(scope, ui, response, identity, label, kind, &mut next_order)
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
    confirm_modal: ConfirmationModal,
    pending_reset: Option<(usize, Value)>,
    pending_confirmation: Option<PendingDashboardEditorConfirmation>,
    next_confirmation_instance: u64,
    slot_tokens: Vec<u64>,
    next_slot_token: u64,
    target_generation: u64,
    pub(crate) property_identity_store: Rc<WidgetPropertyIdentityStore>,
    pub(crate) native_combo_anchors: Vec<crate::dashboard::widgets::WidgetNativeComboAnchor>,
    #[cfg(test)]
    test_scroll_metrics: Option<(egui::Id, f32)>,
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
            confirm_modal: ConfirmationModal::default(),
            pending_reset: None,
            pending_confirmation: None,
            next_confirmation_instance: 1,
            slot_tokens: Vec::new(),
            next_slot_token: 1,
            target_generation: 0,
            property_identity_store: Rc::new(WidgetPropertyIdentityStore::default()),
            native_combo_anchors: Vec::new(),
            #[cfg(test)]
            test_scroll_metrics: None,
        }
    }
}

impl DashboardEditorDialog {
    pub fn open(&mut self, path: &str, registry: &WidgetRegistry) {
        self.path = path.to_string();
        self.reload(registry);
        self.open = true;
    }

    pub(crate) fn target_generation(&self) -> u64 {
        self.target_generation
    }

    #[cfg(test)]
    pub(crate) fn test_scroll_area_max_offset(&self) -> Option<(egui::Id, f32)> {
        self.test_scroll_metrics
    }

    #[cfg(test)]
    pub(crate) fn test_slot_token(&self, index: usize) -> Option<u64> {
        self.slot_tokens.get(index).copied()
    }

    pub(crate) fn native_combo_popup_is_current(
        &self,
        ctx: &egui::Context,
        parent_layer: egui::LayerId,
    ) -> bool {
        if !egui::Popup::is_any_open(ctx) {
            return false;
        }

        self.native_combo_anchors.iter().any(|anchor| {
            if anchor.layer_id != parent_layer {
                return false;
            }
            let Some(response) = ctx.read_response(anchor.response_id) else {
                return false;
            };
            if !response.enabled() || response.layer_id != anchor.layer_id {
                return false;
            }
            let popup_id = egui::Popup::default_response_id(&response);
            if !egui::ComboBox::is_open(ctx, response.id) || !egui::Popup::is_id_open(ctx, popup_id)
            {
                return false;
            }
            let Some(popup_rect) = ctx.memory(|memory| memory.area_rect(popup_id)) else {
                return false;
            };
            if !popup_rect.is_positive() {
                return false;
            }
            let popup_layer = egui::LayerId::new(egui::Order::Foreground, popup_id);
            let popup_is_current = ctx.layer_id_at(popup_rect.center()) == Some(popup_layer);
            let popup_is_sizing = egui::containers::AreaState::load(ctx, popup_id)
                .is_some_and(|state| !state.interactable);
            let focus_is_owned =
                crate::gui::jump_mode::ui::focused_response_layer(ctx).is_none_or(|layer| {
                    layer == anchor.layer_id
                        || layer == popup_layer
                        || layer.order != egui::Order::Middle
                });
            (popup_is_current || popup_is_sizing) && focus_is_owned
        })
    }

    pub(crate) fn jump_mode_pending_confirmation_identity(
        &self,
    ) -> Option<(u64, DashboardEditorConfirmationKind)> {
        let pending = self.pending_confirmation?;
        let current_slot = self
            .slot_tokens
            .iter()
            .position(|token| *token == pending.slot_token)?;
        let is_current = match pending.kind {
            DashboardEditorConfirmationKind::RemoveSlot => {
                self.confirm_remove_slot == Some(current_slot)
            }
            DashboardEditorConfirmationKind::ResetWidgetSettings => {
                self.pending_reset
                    .as_ref()
                    .is_some_and(|(index, _)| *index == current_slot)
                    && self.confirm_modal.is_open()
            }
        };
        is_current.then_some((pending.instance, pending.kind))
    }

    fn begin_confirmation(&mut self, kind: DashboardEditorConfirmationKind, slot_index: usize) {
        let Some(slot_token) = self.slot_tokens.get(slot_index).copied() else {
            return;
        };
        let instance = self.next_confirmation_instance;
        self.next_confirmation_instance = self.next_confirmation_instance.wrapping_add(1).max(1);
        self.pending_confirmation = Some(PendingDashboardEditorConfirmation {
            kind,
            instance,
            slot_token,
        });
    }

    fn finish_confirmation(&mut self) {
        self.pending_confirmation = None;
        self.confirm_remove_slot = None;
        self.pending_reset = None;
        self.confirm_modal.close();
    }

    fn next_slot_token(&mut self) -> u64 {
        let token = self.next_slot_token;
        self.next_slot_token = self.next_slot_token.wrapping_add(1).max(1);
        token
    }

    fn renew_slot_tokens(&mut self) {
        self.target_generation = self.target_generation.wrapping_add(1).max(1);
        self.property_identity_store.clear();
        self.native_combo_anchors.clear();
        self.finish_confirmation();
        self.slot_tokens.clear();
        for _ in 0..self.config.slots.len() {
            let token = self.next_slot_token();
            self.slot_tokens.push(token);
        }
    }

    fn ensure_slot_tokens(&mut self) {
        if self.slot_tokens.len() != self.config.slots.len() {
            self.renew_slot_tokens();
        }
    }

    fn add_slot(&mut self, slot: SlotConfig) {
        let token = self.next_slot_token();
        self.config.slots.push(slot);
        self.slot_tokens.push(token);
    }

    fn reload(&mut self, registry: &WidgetRegistry) {
        self.target_generation = self.target_generation.wrapping_add(1).max(1);
        self.property_identity_store.clear();
        self.native_combo_anchors.clear();
        self.finish_confirmation();
        match DashboardConfig::load(&self.path, registry) {
            Ok(cfg) => {
                self.config = cfg;
                self.error = None;
                self.slot_tokens.clear();
                for _ in 0..self.config.slots.len() {
                    let token = self.next_slot_token();
                    self.slot_tokens.push(token);
                }
                self.ensure_selected_slot();
            }
            Err(e) => {
                self.error = Some(format!("Failed to load dashboard: {e}"));
            }
        }
    }

    fn save(&mut self, registry: &WidgetRegistry) {
        match DashboardConfig::replace(&self.path, registry, self.config.clone()) {
            Ok(committed) => {
                self.config = committed;
                self.renew_slot_tokens();
                self.error = None;
                self.pending_save = true;
            }
            Err(e) => {
                self.error = Some(format!("Failed to save: {e}"));
            }
        }
    }

    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        registry: &WidgetRegistry,
        settings_ctx: WidgetSettingsContext<'_>,
        require_confirm_destructive: bool,
    ) -> bool {
        self.ui_with_jump_mode(
            ctx,
            registry,
            settings_ctx,
            require_confirm_destructive,
            None,
            None,
            None,
            true,
            true,
            true,
        )
    }

    pub(crate) fn ui_with_jump_mode(
        &mut self,
        ctx: &egui::Context,
        registry: &WidgetRegistry,
        settings_ctx: WidgetSettingsContext<'_>,
        require_confirm_destructive: bool,
        parent_instance: Option<u64>,
        jump_scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        confirmation_scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        native_ui_owner: bool,
        native_confirmation_owner: bool,
        first_ui_pass: bool,
    ) -> bool {
        if !self.open {
            return false;
        }
        settings_ctx.begin_native_combo_anchor_pass();
        self.ensure_slot_tokens();
        let mut should_reload = false;
        let mut open = self.open;
        let mut authorized_visible_pass = false;
        let mut draft_snapshot = None;
        let mut visual_order = 0_u64;
        let mut child_remove_confirmed = None;
        let mut child_cancelled = false;
        let mut reset_result = ConfirmationResult::None;
        let mut confirmation_opened_this_pass = false;
        egui::Window::new("Dashboard Editor")
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                authorized_visible_pass = native_ui_owner
                    && first_ui_pass
                    && ui.is_visible()
                    && !ui.is_sizing_pass();
                if !authorized_visible_pass {
                    draft_snapshot = Some(DashboardEditorDraftSnapshot::capture(self));
                }
                let _scroll_output = egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.set_max_width(ui.available_width().min(900.0));
                    if let Some(err) = &self.error {
                        widgets::semantic_status(ui, StatusTone::Error, err, None);
                    }

                    ui.horizontal(|ui| {
                        ui.label("Rows");
                        let response = ui.add(
                            egui::DragValue::new(&mut self.config.grid.rows).range(1..=12),
                        );
                        register_editor_target(jump_scope, ui, &response, "grid/rows", "Grid rows", crate::gui::jump_mode::TargetKind::Slider, &mut visual_order);
                        if response.changed() {
                            self.clamp_all_slots();
                        }
                        ui.label("Cols");
                        let response = ui.add(
                            egui::DragValue::new(&mut self.config.grid.cols).range(1..=12),
                        );
                        register_editor_target(jump_scope, ui, &response, "grid/columns", "Grid columns", crate::gui::jump_mode::TargetKind::Slider, &mut visual_order);
                        if response.changed() {
                            self.clamp_all_slots();
                        }
                    });

                    ui.separator();
                    widgets::compact_section_heading(ui, "Preview", None);
                    ui.horizontal_wrapped(|ui| {
                        for (identity, label, value) in [
                            ("show-preview", "Show preview", &mut self.show_preview),
                            ("snap-on-edit", "Snap to free space on edit", &mut self.snap_on_edit),
                            ("show-swap", "Show swap", &mut self.show_swap_buttons),
                            ("show-remove", "Show remove", &mut self.show_remove_buttons),
                        ] {
                            let response = ui.checkbox(value, label);
                            register_editor_target(jump_scope, ui, &response, identity, label, crate::gui::jump_mode::TargetKind::Checkbox, &mut visual_order);
                        }
                        let response = ui.button("Auto-place nearest");
                        register_editor_target(jump_scope, ui, &response, "auto-place-nearest", "Auto-place nearest", crate::gui::jump_mode::TargetKind::Button, &mut visual_order);
                        if response.clicked() && authorized_visible_pass
                            && let Some(idx) = self.selected_slot
                            && let Err(err) = self.auto_place(idx, registry)
                        {
                            self.blocked_warning = Some(err);
                        }
                        let response = ui.button("Compact layout");
                        register_editor_target(jump_scope, ui, &response, "compact-layout", "Compact layout", crate::gui::jump_mode::TargetKind::Button, &mut visual_order);
                        if response.clicked() && authorized_visible_pass
                            && let Err(err) = self.compact_layout(registry)
                        {
                            self.blocked_warning = Some(err);
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
                            widgets::semantic_status(ui, StatusTone::Warning, &warn, None);
                        }
                        if let Some(idx) = self.selected_slot {
                            let response = ui.button("Auto-place nearest free slot");
                            register_editor_target(jump_scope, ui, &response, "auto-place-free-slot", "Auto-place free slot", crate::gui::jump_mode::TargetKind::Button, &mut visual_order);
                            if response.clicked() && authorized_visible_pass
                            && let Err(err) = self.auto_place(idx, registry)
                            {
                                self.blocked_warning = Some(err);
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
                            widgets::semantic_status(ui, StatusTone::Error, msg, None);
                        }
                    }

                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        let response = ui.button("Add slot");
                        register_editor_target(jump_scope, ui, &response, "slot/add", "Add slot", crate::gui::jump_mode::TargetKind::Button, &mut visual_order);
                        if response.clicked() && authorized_visible_pass {
                            self.add_slot(SlotConfig::with_widget("weather_site", 0, 0));
                        }
                        let response = ui.button("Reload from disk");
                        register_editor_target(jump_scope, ui, &response, "reload", "Reload dashboard", crate::gui::jump_mode::TargetKind::Button, &mut visual_order);
                        if response.clicked() && authorized_visible_pass {
                            self.reload(registry);
                        }
                        ui.add_enabled_ui(!has_conflicts, |ui| {
                            let response = ui.button("Save");
                            let response = widgets::with_button_emphasis(
                                ui,
                                response,
                                ButtonEmphasis::Primary,
                            );
                            register_editor_target(jump_scope, ui, &response, "save", "Save dashboard", crate::gui::jump_mode::TargetKind::Button, &mut visual_order);
                            if response.clicked() && authorized_visible_pass {
                                self.save(registry);
                            }
                        });
                        if has_conflicts {
                            widgets::semantic_status(
                                ui,
                                StatusTone::Error,
                                "Resolve slot conflicts before saving.",
                                None,
                            );
                        }
                        let response = ui.button("Expand all");
                        register_editor_target(jump_scope, ui, &response, "slots/expand-all", "Expand all slots", crate::gui::jump_mode::TargetKind::Button, &mut visual_order);
                        if response.clicked() && authorized_visible_pass {
                            self.slot_expand_all = true;
                            self.slot_collapse_all = false;
                        }
                        let response = ui.button("Collapse all");
                        register_editor_target(jump_scope, ui, &response, "slots/collapse-all", "Collapse all slots", crate::gui::jump_mode::TargetKind::Button, &mut visual_order);
                        if response.clicked() && authorized_visible_pass {
                            self.slot_collapse_all = true;
                            self.slot_expand_all = false;
                        }
                        let response = ui.button("Split H");
                        register_editor_target(jump_scope, ui, &response, "slot/split-horizontal", "Split slot horizontally", crate::gui::jump_mode::TargetKind::Button, &mut visual_order);
                        if response.clicked() && authorized_visible_pass
                            && let Err(err) =
                                self.split_selected_slot(registry, SplitDirection::Horizontal)
                        {
                            self.blocked_warning = Some(err);
                        }
                        let response = ui.button("Split V");
                        register_editor_target(jump_scope, ui, &response, "slot/split-vertical", "Split slot vertically", crate::gui::jump_mode::TargetKind::Button, &mut visual_order);
                        if response.clicked() && authorized_visible_pass
                            && let Err(err) =
                                self.split_selected_slot(registry, SplitDirection::Vertical)
                        {
                            self.blocked_warning = Some(err);
                        }
                    });

                    ui.separator();
                    if let Some((confirmation_instance, DashboardEditorConfirmationKind::RemoveSlot)) =
                        self.jump_mode_pending_confirmation_identity()
                        && let Some(idx) = self.confirm_remove_slot
                        && let (Some(slot), Some(slot_token)) =
                            (self.config.slots.get(idx), self.slot_tokens.get(idx).copied())
                    {
                        let label = Self::slot_label(slot);
                        let mut open = true;
                        let child_layer = crate::gui::jump_mode::ui::dashboard_editor_confirmation_layer_id(
                            parent_instance.unwrap_or_default(),
                            confirmation_instance,
                            DashboardEditorConfirmationKind::RemoveSlot,
                        );
                        let mut child_visible_pass = false;
                        egui::Window::new(format!("Remove slot '{label}'?"))
                            .id(child_layer.id)
                            .collapsible(false)
                            .resizable(false)
                            .open(&mut open)
                            .show(ctx, |ui| {
                                child_visible_pass = native_confirmation_owner
                                    && first_ui_pass
                                    && !confirmation_opened_this_pass
                                    && ui.is_visible()
                                    && !ui.is_sizing_pass();
                                ui.label("Remove this slot from the dashboard?");
                                ui.horizontal(|ui| {
                                    let confirm = ui.button("Remove");
                                    register_editor_target(
                                        confirmation_scope,
                                        ui,
                                        &confirm,
                                        "confirmation/confirm",
                                        "Confirm remove slot",
                                        crate::gui::jump_mode::TargetKind::Button,
                                        &mut visual_order,
                                    );
                                    if confirm.clicked() && child_visible_pass {
                                        child_remove_confirmed = Some(slot_token);
                                    }
                                    let cancel = ui.button("Cancel");
                                    register_editor_target(
                                        confirmation_scope,
                                        ui,
                                        &cancel,
                                        "confirmation/cancel",
                                        "Cancel remove slot",
                                        crate::gui::jump_mode::TargetKind::Button,
                                        &mut visual_order,
                                    );
                                    if cancel.clicked() && child_visible_pass {
                                        child_cancelled = true;
                                    }
                                });
                            });
                        if !open && child_visible_pass {
                            child_cancelled = true;
                        }
                    }
                    let mut idx = 0;
                    while idx < self.config.slots.len() {
                        let original_slot = self.config.slots[idx].clone();
                        let slot_token = self.slot_tokens[idx];
                        let mut slot = original_slot.clone();
                        let mut edited = false;
                        let mut swapped = false;
                        let mut section_response = None;
                        let mut section_order = visual_order;
                        ui.scope_builder(
                            egui::UiBuilder::new()
                                .id(ui.id().with(("dashboard-editor-slot", slot_token))),
                            |ui| {
                            let collapsing_id = ui.id().with(("slot-collapse", slot_token));
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
                                    let id_response = ui.text_edit_singleline(id);
                                    register_editor_target(jump_scope, ui, &id_response, format!("slot/{slot_token}/id"), "Slot identifier", crate::gui::jump_mode::TargetKind::TextEntry, &mut visual_order);
                                    edited |= id_response.changed();
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if self.show_remove_buttons {
                                                let response = ui.button("Remove");
                                                register_editor_target(jump_scope, ui, &response, format!("slot/{slot_token}/remove"), "Remove slot", crate::gui::jump_mode::TargetKind::Button, &mut visual_order);
                                                if response.clicked() && authorized_visible_pass {
                                                    self.confirm_remove_slot = Some(idx);
                                                    self.begin_confirmation(
                                                        DashboardEditorConfirmationKind::RemoveSlot,
                                                        idx,
                                                    );
                                                    confirmation_opened_this_pass = true;
                                                }
                                            }
                                            let response = ui.button("Select");
                                            register_editor_target(jump_scope, ui, &response, format!("slot/{slot_token}/select"), "Select slot", crate::gui::jump_mode::TargetKind::Button, &mut visual_order);
                                            if response.clicked() && authorized_visible_pass {
                                                self.selected_slot = Some(idx);
                                            }
                                            if self.show_swap_buttons {
                                                if let Some(selected_idx) = self.selected_slot
                                                    && selected_idx != idx
                                                {
                                                    let response = ui.button("Swap with selected");
                                                    register_editor_target(jump_scope, ui, &response, format!("slot/{slot_token}/swap-with-selected"), "Swap with selected", crate::gui::jump_mode::TargetKind::Button, &mut visual_order);
                                                    if response.clicked() && authorized_visible_pass {
                                                    if let Err(err) =
                                                        self.swap_slots(selected_idx, idx, registry)
                                                    {
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
                                                let response = ui.selectable_label(is_swap_source, swap_label);
                                                register_editor_target(jump_scope, ui, &response, format!("slot/{slot_token}/swap"), "Swap slot", crate::gui::jump_mode::TargetKind::Selectable, &mut visual_order);
                                                if response.clicked() && authorized_visible_pass {
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
                            section_order = visual_order;
                            visual_order = visual_order.saturating_add(1);
                            let (response, _, body) = header_response.body_unindented(|ui| {
                                let widget_anchor = format!("slot/{slot_token}/widget-type");
                                let widget_popup = jump_scope.map(|scope| scope.combo_popup_scope(&widget_anchor));
                                let widget_combo = egui::ComboBox::from_id_salt(
                                    ui.id().with(("dashboard-editor-widget-type", slot_token)),
                                )
                                    .selected_text(slot.widget.clone())
                                    .show_ui(ui, |ui| {
                                        for name in registry.names() {
                                            let response = ui.selectable_value(
                                                &mut slot.widget,
                                                name.to_string(),
                                                name.as_str(),
                                            );
                                            let choice_id = crate::dashboard::widgets::opaque_settings_key(name);
                                            if let Some(popup_scope) = widget_popup.as_ref() {
                                                register_editor_target(Some(popup_scope), ui, &response, format!("widget/{choice_id}"), "Widget type option", crate::gui::jump_mode::TargetKind::Selectable, &mut visual_order);
                                            }
                                            if response.changed() {
                                                edited = true;
                                            }
                                        }
                                    });
                                settings_ctx.record_native_combo_anchor(ui, &widget_combo.response);
                                register_editor_target(jump_scope, ui, &widget_combo.response, widget_anchor, "Widget type", crate::gui::jump_mode::TargetKind::ComboBox, &mut visual_order);
                                ui.horizontal(|ui| {
                                    ui.label("Row");
                                    let rows = self.config.grid.rows.max(1) as i32;
                                    let cols = self.config.grid.cols.max(1) as i32;
                                    let response = ui.add(egui::DragValue::new(&mut slot.row).range(0..=rows.saturating_sub(1)));
                                    register_editor_target(jump_scope, ui, &response, format!("slot/{slot_token}/row"), "Slot row", crate::gui::jump_mode::TargetKind::Slider, &mut visual_order);
                                    if response.changed() {
                                        Self::clamp_position_only(&mut slot, rows, cols);
                                        slot.row_span =
                                            slot.row_span.min(self.max_row_span_for(&slot));
                                        slot.col_span =
                                            slot.col_span.min(self.max_col_span_for(&slot));
                                        edited = true;
                                    }
                                    ui.label("Col");
                                    let response = ui.add(egui::DragValue::new(&mut slot.col).range(0..=cols.saturating_sub(1)));
                                    register_editor_target(jump_scope, ui, &response, format!("slot/{slot_token}/column"), "Slot column", crate::gui::jump_mode::TargetKind::Slider, &mut visual_order);
                                    if response.changed() {
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
                                    let response = ui.add(egui::DragValue::new(&mut slot.row_span).range(1..=max_row_span));
                                    register_editor_target(jump_scope, ui, &response, format!("slot/{slot_token}/row-span"), "Slot row span", crate::gui::jump_mode::TargetKind::Slider, &mut visual_order);
                                    if response.changed() {
                                        slot.row_span = slot.row_span.min(max_row_span);
                                        edited = true;
                                    }
                                    ui.label("Col span");
                                    let max_col_span = self.max_col_span_for(&slot);
                                    let response = ui.add(egui::DragValue::new(&mut slot.col_span).range(1..=max_col_span));
                                    register_editor_target(jump_scope, ui, &response, format!("slot/{slot_token}/column-span"), "Slot column span", crate::gui::jump_mode::TargetKind::Slider, &mut visual_order);
                                    if response.changed() {
                                        slot.col_span = slot.col_span.min(max_col_span);
                                        edited = true;
                                    }
                                });
                                let overflow_anchor = format!("slot/{slot_token}/overflow");
                                let overflow_popup = jump_scope.map(|scope| scope.combo_popup_scope(&overflow_anchor));
                                let overflow_combo = egui::ComboBox::from_id_salt(
                                    ui.id().with(("dashboard-editor-overflow", slot_token)),
                                )
                                    .selected_text(slot.overflow.as_str())
                                    .show_ui(ui, |ui| {
                                        for (option_id, mode) in [
                                            ("scroll", OverflowMode::Scroll),
                                            ("clip", OverflowMode::Clip),
                                            ("auto", OverflowMode::Auto),
                                        ] {
                                            let response = ui.selectable_value(
                                                &mut slot.overflow,
                                                mode,
                                                mode.as_str(),
                                            );
                                            if let Some(popup_scope) = overflow_popup.as_ref() {
                                                register_editor_target(Some(popup_scope), ui, &response, format!("option/{option_id}"), "Overflow option", crate::gui::jump_mode::TargetKind::Selectable, &mut visual_order);
                                            }
                                        }
                                    });
                                settings_ctx.record_native_combo_anchor(ui, &overflow_combo.response);
                                register_editor_target(jump_scope, ui, &overflow_combo.response, overflow_anchor, "Overflow policy", crate::gui::jump_mode::TargetKind::ComboBox, &mut visual_order);
                                ui.separator();
                            });
                            section_response = Some(response);
                            if body.is_none() {
                                // Ensure state is stored even when collapsed
                            }
                            },
                        );
                        let section_response = section_response
                            .expect("the slot section has a real header response");
                        register_editor_target_at_order(
                            jump_scope,
                            ui,
                            &section_response,
                            format!("slot/{slot_token}/section"),
                            "Slot section",
                            crate::gui::jump_mode::TargetKind::Selectable,
                            section_order,
                        );
                        if swapped {
                            idx += 1;
                        } else if authorized_visible_pass && edited && slot != original_slot {
                            if let Err(err) = self.commit_slot(idx, slot, registry)
                                && self.blocked_warning.is_none()
                            {
                                self.blocked_warning = Some(err);
                            }
                            idx += 1;
                        } else {
                            idx += 1;
                        }
                    }
                    ui.separator();
                    self.render_selected_settings(
                        ui,
                        registry,
                        &settings_ctx,
                        require_confirm_destructive,
                        jump_scope,
                        confirmation_scope,
                        parent_instance,
                        authorized_visible_pass,
                        native_confirmation_owner,
                        first_ui_pass,
                        &mut confirmation_opened_this_pass,
                        &mut visual_order,
                        &mut reset_result,
                    );
                    // Reset batch flags after applying once
                    self.slot_expand_all = false;
                    self.slot_collapse_all = false;
                });
                #[cfg(test)]
                {
                    self.test_scroll_metrics = Some((
                        _scroll_output.id,
                        (_scroll_output.content_size.y - _scroll_output.inner_rect.height())
                            .max(0.0),
                    ));
                }
            });
        if let Some(snapshot) = draft_snapshot {
            snapshot.restore(self);
        }
        if let Some(slot_token) = child_remove_confirmed
            && self
                .jump_mode_pending_confirmation_identity()
                .is_some_and(|(_, kind)| kind == DashboardEditorConfirmationKind::RemoveSlot)
            && let Some(idx) = self
                .slot_tokens
                .iter()
                .position(|token| *token == slot_token)
        {
            self.remove_slot_at(idx);
            self.finish_confirmation();
        } else if child_cancelled || reset_result != ConfirmationResult::None {
            match reset_result {
                ConfirmationResult::Confirmed => {
                    if let Some((idx, settings)) = self.pending_reset.take()
                        && self.slot_tokens.get(idx).copied().is_some_and(|token| {
                            self.pending_confirmation
                                .is_some_and(|pending| pending.slot_token == token)
                        })
                    {
                        if let Some(slot_token) = self.slot_tokens.get(idx).copied() {
                            self.property_identity_store.clear_slot(slot_token);
                        }
                        if let Some(slot) = self.config.slots.get_mut(idx) {
                            slot.settings = settings;
                            self.blocked_warning = None;
                        }
                    }
                    self.finish_confirmation();
                }
                ConfirmationResult::Cancelled => self.finish_confirmation(),
                ConfirmationResult::None if child_cancelled => self.finish_confirmation(),
                ConfirmationResult::None => {}
            }
        }
        if native_ui_owner && first_ui_pass {
            self.open = open;
        }
        if authorized_visible_pass && self.pending_save {
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
        if let Some(idx) = self.selected_slot
            && idx >= self.config.slots.len()
        {
            self.selected_slot = Some(self.config.slots.len().saturating_sub(1));
        }
        self.ensure_swap_anchor();
    }

    fn ensure_swap_anchor(&mut self) {
        if self.config.slots.is_empty() {
            self.swap_anchor = None;
            return;
        }
        if let Some(idx) = self.swap_anchor
            && idx >= self.config.slots.len()
        {
            self.swap_anchor = None;
        }
    }

    fn remove_slot_at(&mut self, idx: usize) {
        if idx >= self.config.slots.len() {
            return;
        }
        let token = self.slot_tokens.remove(idx);
        self.config.slots.remove(idx);
        self.property_identity_store.clear_slot(token);
        if let Some(selected) = self.selected_slot {
            if selected >= idx && !self.config.slots.is_empty() {
                self.selected_slot =
                    Some(selected.saturating_sub(1).min(self.config.slots.len() - 1));
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
    }

    fn render_selected_settings(
        &mut self,
        ui: &mut egui::Ui,
        registry: &WidgetRegistry,
        settings_ctx: &WidgetSettingsContext<'_>,
        require_confirm_destructive: bool,
        jump_scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        confirmation_scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        parent_instance: Option<u64>,
        authorized_visible_pass: bool,
        native_confirmation_owner: bool,
        first_ui_pass: bool,
        confirmation_opened_this_pass: &mut bool,
        visual_order: &mut u64,
        reset_result: &mut ConfirmationResult,
    ) {
        widgets::compact_section_heading(ui, "Settings", None);
        if self.config.slots.is_empty() {
            ui.label("Add a slot to edit its settings.");
            return;
        }
        let Some(selected_idx) = self.selected_slot else {
            ui.label("Select a slot to edit settings.");
            return;
        };
        if selected_idx >= self.config.slots.len() {
            widgets::semantic_status(
                ui,
                StatusTone::Warning,
                "Selected slot no longer exists; please reselect.",
                None,
            );
            return;
        }

        let slot_token = self.slot_tokens[selected_idx];
        let slot = &mut self.config.slots[selected_idx];
        let widget_name = slot.widget.clone();
        let property_scope = jump_scope
            .map(|scope| scope.child_namespace(format!("slot/{slot_token}/widget/{widget_name}")));
        let scoped_settings_ctx = settings_ctx.with_slot_scope(
            property_scope.as_ref(),
            slot_token,
            authorized_visible_pass,
        );
        let default_settings = registry
            .default_settings(&widget_name)
            .unwrap_or_else(|| Value::Object(Default::default()));
        let mut settings_changed = Self::ensure_slot_settings_defaults(slot, &default_settings, ui);
        let mut request_reset_confirmation = false;
        let mut reset_immediately = false;

        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Widget: {widget_name}"));
            if let Some(meta) = registry.metadata_for(&widget_name) {
                if meta.has_settings_ui {
                    widgets::semantic_status(ui, StatusTone::Success, "Settings available", None);
                } else {
                    widgets::secondary_help(ui, "No custom settings UI");
                }
            }
            let reset_response = ui.button("Reset to defaults");
            register_editor_target(
                jump_scope,
                ui,
                &reset_response,
                format!("slot/{slot_token}/reset-settings"),
                "Reset widget settings",
                crate::gui::jump_mode::TargetKind::Button,
                visual_order,
            );
            if reset_response.clicked() && authorized_visible_pass {
                if require_confirm_destructive {
                    request_reset_confirmation = true;
                    *confirmation_opened_this_pass = true;
                } else {
                    slot.settings = default_settings.clone();
                    settings_changed = true;
                    reset_immediately = true;
                }
            }
        });

        let settings_result = ui
            .scope_builder(
                egui::UiBuilder::new().id(ui.id().with((
                    "dashboard-editor-widget-properties",
                    slot_token,
                    widget_name.as_str(),
                ))),
                |ui| {
                    registry.render_settings_ui(
                        &widget_name,
                        ui,
                        &mut slot.settings,
                        &scoped_settings_ctx,
                    )
                },
            )
            .inner;
        if let Some(result) = settings_result {
            if result.changed {
                settings_changed = true;
            }
            if let Some(err) = result.error {
                widgets::semantic_status(
                    ui,
                    StatusTone::Warning,
                    &format!("{err}. Settings saved after edits."),
                    None,
                );
            }
        } else {
            widgets::secondary_help(ui, "No settings available for this widget.");
        }
        if reset_immediately {
            self.property_identity_store.clear_slot(slot_token);
        }
        if request_reset_confirmation {
            self.pending_reset = Some((selected_idx, default_settings.clone()));
            self.confirm_modal
                .open_for(DestructiveAction::ResetWidgetSettings);
            self.begin_confirmation(
                DashboardEditorConfirmationKind::ResetWidgetSettings,
                selected_idx,
            );
        }

        let confirmation_result = if let (Some(parent_instance), Some((instance, kind))) = (
            parent_instance,
            self.jump_mode_pending_confirmation_identity(),
        ) {
            let window_id = crate::gui::jump_mode::ui::dashboard_editor_confirmation_layer_id(
                parent_instance,
                instance,
                kind,
            )
            .id;
            self.confirm_modal.ui_with_jump_mode(
                ui.ctx(),
                window_id,
                confirmation_scope,
                native_confirmation_owner,
                first_ui_pass && !*confirmation_opened_this_pass,
            )
        } else {
            self.confirm_modal.ui(ui.ctx())
        };
        match confirmation_result {
            ConfirmationResult::Confirmed => {
                *reset_result = ConfirmationResult::Confirmed;
            }
            ConfirmationResult::Cancelled => {
                *reset_result = ConfirmationResult::Cancelled;
            }
            ConfirmationResult::None => {}
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
                self.slot_tokens.swap(first, second);
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
            widgets::semantic_status(
                ui,
                StatusTone::Warning,
                "Settings were empty; defaults were applied.",
                None,
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
                if self.config.slots[idx].widget != clamped.widget {
                    self.property_identity_store
                        .clear_slot(self.slot_tokens[idx]);
                }
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
            let is_selected = selected_slot_cfg.as_ref().is_some_and(|selected| {
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
                egui::StrokeKind::Middle,
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
        if response.drag_started()
            && let Some(pos) = response.interact_pointer_pos()
        {
            let cell = self.point_to_cell(pos, rect, rows, cols);
            self.drag_anchor = Some(cell);
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
                    if let Err(err) = res
                        && self.blocked_warning.is_none()
                    {
                        self.blocked_warning = Some(err);
                    }
                }
            }
        } else if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
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
                if let Err(err) = res
                    && self.blocked_warning.is_none()
                {
                    self.blocked_warning = Some(err);
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
                egui::StrokeKind::Middle,
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
                                egui::StrokeKind::Middle,
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
            if coverage.out_of_bounds
                && let Some(flag) = map.slot_conflicts.get_mut(idx)
            {
                *flag = true;
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
                if let Some(owners) = owners
                    && owners.len() > 1
                {
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
                    if best.is_none_or(|current| candidate < current) {
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
                let token = self.next_slot_token();
                self.slot_tokens.push(token);
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

#[cfg(test)]
mod persistence_tests {
    use super::*;

    #[test]
    fn corrupt_document_stays_read_only_when_editor_save_is_attempted() {
        let temporary = tempfile::NamedTempFile::new().unwrap();
        let original = b"{broken";
        std::fs::write(temporary.path(), original).unwrap();
        let registry = WidgetRegistry::with_defaults();
        let mut dialog = DashboardEditorDialog::default();
        dialog.open(temporary.path().to_str().unwrap(), &registry);
        assert!(dialog.error.is_some());

        dialog.config.grid.rows = 9;
        dialog.save(&registry);

        assert!(!dialog.pending_save);
        assert!(dialog.error.as_deref().unwrap().contains("Failed to save"));
        assert_eq!(std::fs::read(temporary.path()).unwrap(), original);
    }

    #[test]
    fn jump_mode_dashboard_editor_native_popup_requires_its_current_combo_anchor() {
        let ctx = egui::Context::default();
        let mut dialog = DashboardEditorDialog {
            open: true,
            ..DashboardEditorDialog::default()
        };
        let combo_id = egui::Id::new("dashboard-editor-native-popup-test-combo");
        let expected_layer = crate::gui::jump_mode::ui::NavigationDialogKind::DashboardEditor
            .layer_id()
            .expect("the Dashboard Editor has an exact native window layer");
        let mut selected = 0_u8;
        ctx.all_styles_mut(|style| style.animation_time = 0.0);

        let mut draw = |events: Vec<egui::Event>| -> (egui::Rect, egui::Id) {
            let anchors = std::cell::RefCell::new(Vec::new());
            let mut settings_context = WidgetSettingsContext::empty();
            settings_context.native_combo_anchors = Some(&anchors);
            let mut combo_geometry = None;
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1_000.0, 700.0),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |root_ui| {
                    egui::Window::new("Dashboard Editor").show(root_ui.ctx(), |ui| {
                        let combo = egui::ComboBox::from_id_salt(combo_id)
                            .selected_text(format!("Option {selected}"))
                            .show_ui(ui, |ui| {
                                for option in 0..2 {
                                    ui.selectable_value(
                                        &mut selected,
                                        option,
                                        format!("Option {option}"),
                                    );
                                }
                            });
                        assert_eq!(combo.response.layer_id, expected_layer);
                        settings_context.record_native_combo_anchor(ui, &combo.response);
                        combo_geometry = Some((combo.response.rect, combo.response.id));
                    });
                },
            );
            dialog.native_combo_anchors = anchors.into_inner();
            output.drop_without_applying_deltas();
            combo_geometry.expect("the actual ComboBox response was rendered")
        };

        let _ = draw(Vec::new()); // establish actual Window geometry before interaction
        let (combo_rect, _) = draw(Vec::new());
        let center = combo_rect.center();
        let (_, combo_response_id) = draw(vec![
            egui::Event::PointerMoved(center),
            egui::Event::PointerButton {
                pos: center,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos: center,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        drop(draw);
        let combo_response = ctx
            .read_response(combo_response_id)
            .expect("the actual ComboBox response remains current");
        let popup_id = egui::Popup::default_response_id(&combo_response);
        assert!(egui::Popup::is_any_open(&ctx));
        assert!(egui::ComboBox::is_open(&ctx, combo_response_id));
        assert!(dialog.native_combo_popup_is_current(&ctx, expected_layer));

        let unrelated_popup = egui::Id::new("dashboard-editor-unowned-popup");
        egui::Popup::open_id(&ctx, unrelated_popup);
        assert!(!egui::Popup::is_id_open(&ctx, popup_id));
        assert!(egui::Popup::is_any_open(&ctx));
        assert!(
            !dialog.native_combo_popup_is_current(&ctx, expected_layer),
            "an unrelated foreground popup cannot borrow the Dashboard Editor's combo anchor"
        );
    }
}
