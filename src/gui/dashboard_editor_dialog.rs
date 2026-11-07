use crate::dashboard::config::{DashboardConfig, OverflowMode, SlotConfig};
use crate::dashboard::widgets::{
    WidgetPropertyIdentityStore, WidgetRegistry, WidgetSettingsContext,
};
use crate::gui::confirmation_modal::{ConfirmationModal, ConfirmationResult, DestructiveAction};
use crate::gui::jump_mode::ui::DashboardEditorConfirmationKind;
use crate::gui::theme::widgets::{self, ButtonEmphasis, StatusTone};
use crate::gui::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use crate::gui::utility_host::{
    HostIdentity, UtilityHostState, UtilityPresentation, ViewportPassProvenance,
    presentation_button,
};
use crate::gui::utility_window_geometry::NativeViewportIdentity;
use eframe::egui;
use eframe::egui::collapsing_header::CollapsingState;
use serde_json::Value;
use std::cell::Cell;
use std::collections::HashSet;
use std::path::Path;
use std::rc::Rc;

const DASHBOARD_EDITOR_VIEWPORT_KEY: &str = "dashboard-editor";
const DASHBOARD_EDITOR_BODY_ID: &str = "dashboard-editor-body";
const DASHBOARD_EDITOR_NATIVE_TITLE: &str = "Dashboard Editor";
const DASHBOARD_EDITOR_DEFAULT_SIZE: egui::Vec2 = egui::vec2(980.0, 720.0);
const DASHBOARD_EDITOR_MINIMUM_SIZE: egui::Vec2 = egui::vec2(640.0, 420.0);

fn dashboard_editor_viewport_id(owner_instance: u64) -> egui::ViewportId {
    egui::ViewportId::from_hash_of((DASHBOARD_EDITOR_VIEWPORT_KEY, owner_instance))
}

fn dashboard_editor_native_title(owner_instance: u64) -> String {
    format!("{DASHBOARD_EDITOR_NATIVE_TITLE} — {owner_instance}")
}

fn dashboard_editor_viewport_builder(owner_instance: u64) -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title(dashboard_editor_native_title(owner_instance))
        .with_inner_size(DASHBOARD_EDITOR_DEFAULT_SIZE)
        .with_min_inner_size(DASHBOARD_EDITOR_MINIMUM_SIZE)
        .with_resizable(true)
        .with_visible(true)
}

fn dashboard_editor_native_identity(
    host: HostIdentity,
    owner_instance: u64,
) -> Option<NativeViewportIdentity> {
    (host.host == UtilityPresentation::Popout).then_some(NativeViewportIdentity {
        key: crate::settings::UtilityToolKey::parse(DASHBOARD_EDITOR_VIEWPORT_KEY).ok()?,
        viewport_id: dashboard_editor_viewport_id(owner_instance),
        owner_generation: owner_instance,
        host_generation: host.generation,
    })
}

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

#[derive(Clone)]
struct DashboardEditorDraftSnapshot {
    config: DashboardConfig,
    error: Option<String>,
    selected_slot: Option<usize>,
    show_preview: bool,
    blocked_warning: Option<String>,
    presentation_warning: Option<String>,
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
    slot_presentations: Vec<crate::dashboard::editor_presentation::DashboardSlotPresentation>,
    presentation_base: Vec<crate::dashboard::editor_presentation::DashboardSlotPresentationBase>,
    next_presentation_id: u64,
    slot_tokens: Vec<u64>,
    next_slot_token: u64,
    next_confirmation_instance: u64,
    target_generation: u64,
}

impl DashboardEditorDraftSnapshot {
    fn capture(dialog: &DashboardEditorDialog) -> Self {
        Self {
            config: dialog.config.clone(),
            error: dialog.error.clone(),
            selected_slot: dialog.selected_slot,
            show_preview: dialog.show_preview,
            blocked_warning: dialog.blocked_warning.clone(),
            presentation_warning: dialog.presentation_warning.clone(),
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
            slot_presentations: dialog.slot_presentations.clone(),
            presentation_base: dialog.presentation_base.clone(),
            next_presentation_id: dialog.next_presentation_id,
            slot_tokens: dialog.slot_tokens.clone(),
            next_slot_token: dialog.next_slot_token,
            next_confirmation_instance: dialog.next_confirmation_instance,
            target_generation: dialog.target_generation,
        }
    }

    fn restore(self, dialog: &mut DashboardEditorDialog) {
        dialog.config = self.config;
        dialog.error = self.error;
        dialog.selected_slot = self.selected_slot;
        dialog.show_preview = self.show_preview;
        dialog.blocked_warning = self.blocked_warning;
        dialog.presentation_warning = self.presentation_warning;
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
        dialog.slot_presentations = self.slot_presentations;
        dialog.presentation_base = self.presentation_base;
        dialog.next_presentation_id = self.next_presentation_id;
        dialog.slot_tokens = self.slot_tokens;
        dialog.next_slot_token = self.next_slot_token;
        dialog.next_confirmation_instance = self.next_confirmation_instance;
        dialog.target_generation = self.target_generation;
    }
}

#[derive(Clone, Copy)]
struct PendingDashboardEditorConfirmation {
    kind: DashboardEditorConfirmationKind,
    instance: u64,
    slot_token: u64,
}

struct PendingDashboardEditorEffects {
    root_frame_nr: u64,
    root_pass_nr: u64,
    target_generation: u64,
    dashboard_path: String,
    presentation_updates: Vec<(u64, bool)>,
    close_requested: bool,
    save_config: Option<DashboardConfig>,
}

pub(crate) struct DashboardEditorCompletion {
    pub(crate) dashboard_saved: bool,
    pub(crate) presentation_error: Option<String>,
    pub(crate) editor_open: bool,
    pub(crate) show_root: bool,
}

struct DashboardEditorHostFrame {
    root_pass: ViewportPassProvenance,
    host: HostIdentity,
    owner_instance: u64,
    target_generation: u64,
    dashboard_path: String,
    owner_viewport: egui::ViewportId,
    owner_pass: Option<ViewportPassProvenance>,
    owner_pass_accepted: bool,
    callback_observed: bool,
    callback_class: egui::ViewportClass,
    native_viewport: Option<egui::ViewportInfo>,
    embedded_root_layer: Option<egui::LayerId>,
    embedded_root_focused_id: Option<egui::Id>,
    host_request: Option<UtilityPresentation>,
    close_requested: bool,
    os_close_requested: bool,
    focus_host: bool,
    draft_checkpoint: Option<DashboardEditorDraftSnapshot>,
    native_jump_mode: crate::gui::jump_mode::ui::JumpModeUi,
    native_jump_frame: Option<u64>,
}

fn has_dashboard_editor_input_event(ctx: &egui::Context) -> bool {
    use egui::Event;
    ctx.input(|input| {
        input.raw.events.iter().any(|event| {
            matches!(
                event,
                Event::Key { pressed: true, .. }
                    | Event::Text(_)
                    | Event::Paste(_)
                    | Event::PointerButton { .. }
                    | Event::PointerMoved(_)
                    | Event::MouseWheel { .. }
            )
        })
    })
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
    host_state: UtilityHostState,
    owner_instance: u64,
    show_root_after_host_change: bool,
    pending_host_frame: Option<DashboardEditorHostFrame>,
    retained_host_checkpoint: Option<(ViewportPassProvenance, DashboardEditorDraftSnapshot)>,
    native_jump_mode: crate::gui::jump_mode::ui::JumpModeUi,
    native_jump_frame: Option<u64>,
    pending_launcher_close: bool,
    path: String,
    config: DashboardConfig,
    error: Option<String>,
    selected_slot: Option<usize>,
    show_preview: bool,
    blocked_warning: Option<String>,
    presentation_warning: Option<String>,
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
    presentation_store:
        Option<crate::dashboard::editor_presentation::DashboardEditorPresentationStore>,
    slot_presentations: Vec<crate::dashboard::editor_presentation::DashboardSlotPresentation>,
    presentation_base: Vec<crate::dashboard::editor_presentation::DashboardSlotPresentationBase>,
    next_presentation_id: u64,
    pending_root_effects: Option<PendingDashboardEditorEffects>,
    pub(crate) property_identity_store: Rc<WidgetPropertyIdentityStore>,
    pub(crate) native_combo_anchors: Vec<crate::dashboard::widgets::WidgetNativeComboAnchor>,
    #[cfg(test)]
    test_scroll_metrics: Option<(egui::Id, f32)>,
    #[cfg(test)]
    test_grid_rows_response: Option<(egui::Id, egui::Rect)>,
    #[cfg(test)]
    test_host_button: Option<(egui::Id, egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    test_save_button: Option<(egui::Id, egui::Rect, egui::Rect, bool)>,
}

impl Default for DashboardEditorDialog {
    fn default() -> Self {
        Self {
            open: false,
            host_state: UtilityHostState::docked(),
            owner_instance: 0,
            show_root_after_host_change: false,
            pending_host_frame: None,
            retained_host_checkpoint: None,
            native_jump_mode: Default::default(),
            native_jump_frame: None,
            pending_launcher_close: false,
            path: "dashboard.json".into(),
            config: DashboardConfig::default(),
            error: None,
            selected_slot: None,
            show_preview: false,
            blocked_warning: None,
            presentation_warning: None,
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
            presentation_store: None,
            slot_presentations: Vec::new(),
            presentation_base: Vec::new(),
            next_presentation_id: 1,
            pending_root_effects: None,
            property_identity_store: Rc::new(WidgetPropertyIdentityStore::default()),
            native_combo_anchors: Vec::new(),
            #[cfg(test)]
            test_scroll_metrics: None,
            #[cfg(test)]
            test_grid_rows_response: None,
            #[cfg(test)]
            test_host_button: None,
            #[cfg(test)]
            test_save_button: None,
        }
    }
}

impl DashboardEditorDialog {
    pub(crate) fn with_profile_root(root: &crate::platform::app_data::AppDataRoot) -> Self {
        let mut dialog = Self::default();
        dialog.presentation_store = Some(
            crate::dashboard::editor_presentation::DashboardEditorPresentationStore::for_profile(
                root.path(),
            ),
        );
        dialog
    }

    pub(crate) fn is_popped_out(&self) -> bool {
        self.host_state.presentation() == UtilityPresentation::Popout
    }

    pub(crate) fn needs_root_ui_when_hidden(&self) -> bool {
        (self.open && self.is_popped_out())
            || self.host_state.bootstrap_pending()
            || self.pending_host_frame.is_some()
    }

    pub(crate) fn host_layer_if_root(&self) -> Option<egui::LayerId> {
        if !self.open {
            return None;
        }
        match self.host_state.presentation() {
            UtilityPresentation::Docked => {
                crate::gui::jump_mode::ui::NavigationDialogKind::DashboardEditor.layer_id()
            }
            UtilityPresentation::Popout => self.host_state.embedded_root_layer(),
        }
    }

    pub(crate) fn owns_root_input(&self, ctx: &egui::Context) -> bool {
        let Some(layer) = self.host_layer_if_root() else {
            return false;
        };
        if ctx.viewport_id() != egui::ViewportId::ROOT
            || !ctx.input(|input| input.focused)
            || egui::Popup::is_any_open(ctx)
        {
            return false;
        }

        let confirmation_layer =
            self.jump_mode_pending_confirmation_identity()
                .map(|(instance, kind)| {
                    crate::gui::jump_mode::ui::dashboard_editor_confirmation_layer_id(
                        self.owner_instance,
                        instance,
                        kind,
                    )
                });
        let visible = crate::gui::jump_mode::ui::visible_top_middle_layer(ctx);
        let owns_visible_layer = visible == Some(layer)
            || confirmation_layer.is_some_and(|confirmation| visible == Some(confirmation));
        let owns_modal = confirmation_layer.is_some_and(|confirmation| {
            ctx.memory(|memory| memory.top_modal_layer()) == Some(confirmation)
        });
        owns_visible_layer
            && (ctx.memory(|memory| memory.top_modal_layer().is_none()) || owns_modal)
            && crate::gui::jump_mode::ui::focused_response_layer(ctx).is_none_or(|focused| {
                focused == layer
                    || Some(focused) == confirmation_layer
                    || focused.order != egui::Order::Middle
            })
    }

    pub(crate) fn request_host_focus(&mut self) {
        self.host_state.request_focus();
    }

    pub(crate) fn request_close(&mut self) {
        if self.open {
            self.pending_launcher_close = true;
        }
    }

    pub(crate) fn force_close(&mut self, ctx: &egui::Context) {
        let identity =
            dashboard_editor_native_identity(self.host_state.identity(), self.owner_instance);
        self.open = false;
        if let Some(identity) = identity {
            crate::gui::utility_dialog_size::close_native_utility_geometry_viewport(ctx, identity);
            ctx.send_viewport_cmd_to(identity.viewport_id, egui::ViewportCommand::Close);
        }
        self.host_state.retire_to_docked();
        self.native_jump_mode.cancel_all();
        self.native_jump_frame = None;
        self.pending_host_frame = None;
        self.pending_root_effects = None;
        self.retained_host_checkpoint = None;
        self.pending_launcher_close = false;
        self.show_root_after_host_change = false;
        ctx.request_repaint_of(egui::ViewportId::ROOT);
    }

    pub(crate) fn take_show_root_after_host_change(&mut self) -> bool {
        std::mem::take(&mut self.show_root_after_host_change)
    }

    #[cfg(test)]
    pub(crate) fn test_pop_out(&mut self) {
        self.open = true;
        self.host_state.transition(UtilityPresentation::Popout);
    }

    #[cfg(test)]
    pub(crate) fn test_viewport_id(&self) -> egui::ViewportId {
        dashboard_editor_viewport_id(self.owner_instance)
    }

    #[cfg(test)]
    pub(crate) fn test_host_button(&self) -> (egui::Id, egui::Rect, egui::Rect, bool) {
        self.test_host_button
            .expect("host presentation button was rendered")
    }

    #[cfg(test)]
    fn test_save_button(&self) -> (egui::Id, egui::Rect, egui::Rect, bool) {
        self.test_save_button.expect("Save button was rendered")
    }

    pub fn open(&mut self, path: &str, registry: &WidgetRegistry) {
        if self.open && self.path == path {
            if self.is_popped_out() {
                self.request_host_focus();
            }
            return;
        }
        if self.path != path {
            self.pending_host_frame = None;
            self.pending_root_effects = None;
            self.retained_host_checkpoint = None;
            self.pending_launcher_close = false;
        }
        self.path = path.to_string();
        self.reload(registry);
        self.open = true;
    }

    fn next_presentation_id(&mut self) -> u64 {
        let id = self.next_presentation_id;
        self.next_presentation_id = self.next_presentation_id.wrapping_add(1).max(1);
        id
    }

    fn initialize_presentations(&mut self, slots: &[SlotConfig], expanded: &[bool]) {
        self.slot_presentations.clear();
        self.presentation_base.clear();
        for (index, slot) in slots.iter().enumerate() {
            let id = self.next_presentation_id();
            self.slot_presentations.push(
                crate::dashboard::editor_presentation::DashboardSlotPresentation {
                    id,
                    expanded: expanded.get(index).copied().unwrap_or(true),
                },
            );
            self.presentation_base.push(
                crate::dashboard::editor_presentation::DashboardSlotPresentationBase {
                    id,
                    slot: slot.clone(),
                },
            );
        }
    }

    fn ensure_presentations(&mut self) {
        while self.slot_presentations.len() > self.config.slots.len() {
            self.slot_presentations.pop();
        }
        while self.slot_presentations.len() < self.config.slots.len() {
            let id = self.next_presentation_id();
            self.slot_presentations.push(
                crate::dashboard::editor_presentation::DashboardSlotPresentation {
                    id,
                    expanded: true,
                },
            );
        }
    }

    fn presentation_id_at(&self, index: usize) -> u64 {
        self.slot_presentations[index].id
    }

    fn set_presentation_expanded(&mut self, id: u64, expanded: bool) {
        let Some(presentation) = self
            .slot_presentations
            .iter_mut()
            .find(|presentation| presentation.id == id)
        else {
            return;
        };
        presentation.expanded = expanded;
    }

    fn base_presentation_records(&self) -> Vec<(SlotConfig, bool)> {
        self.presentation_base
            .iter()
            .filter_map(|base| {
                self.slot_presentations
                    .iter()
                    .find(|presentation| presentation.id == base.id)
                    .map(|presentation| (base.slot.clone(), presentation.expanded))
            })
            .collect()
    }

    fn current_presentation_records(&self) -> Vec<(SlotConfig, bool)> {
        self.config
            .slots
            .iter()
            .zip(&self.slot_presentations)
            .map(|(slot, presentation)| (slot.clone(), presentation.expanded))
            .collect()
    }

    fn adopt_saved_config(&mut self, config: DashboardConfig) {
        self.config = config;
        self.ensure_presentations();
        self.presentation_base = self
            .config
            .slots
            .iter()
            .zip(&self.slot_presentations)
            .map(|(slot, presentation)| {
                crate::dashboard::editor_presentation::DashboardSlotPresentationBase {
                    id: presentation.id,
                    slot: slot.clone(),
                }
            })
            .collect();
        self.renew_slot_tokens();
    }

    /// Publish staged dashboard writes only after this exact ROOT pass was
    /// retained and the original dialog/path owner is still current.
    pub(crate) fn on_ui_frame_complete(
        &mut self,
        ctx: &egui::Context,
        registry: &WidgetRegistry,
        active_dashboard_path: &str,
    ) -> Option<DashboardEditorCompletion> {
        let effects = self.pending_root_effects.take()?;
        let retained = ctx.cumulative_frame_nr_for(egui::ViewportId::ROOT)
            == effects.root_frame_nr.saturating_add(1)
            && ctx.cumulative_pass_nr_for(egui::ViewportId::ROOT)
                == effects.root_pass_nr.saturating_add(1);
        if !retained
            || self.target_generation != effects.target_generation
            || self.path != effects.dashboard_path
            || active_dashboard_path != effects.dashboard_path
        {
            return None;
        }

        for (id, expanded) in effects.presentation_updates {
            self.set_presentation_expanded(id, expanded);
        }

        let mut dashboard_saved = false;
        let mut presentation_records = self.base_presentation_records();
        let mut presentation_error = None;
        let save_requested = effects.save_config.is_some();
        if let Some(config) = effects.save_config {
            if config != self.config {
                self.error = Some("Dashboard draft changed before Save could finish.".into());
            } else {
                match self.persist_config(registry, &effects.dashboard_path, config) {
                    Ok(committed) => {
                        self.adopt_saved_config(committed);
                        presentation_records = self.current_presentation_records();
                        self.error = None;
                        dashboard_saved = true;
                    }
                    Err(error) => self.error = Some(format!("Failed to save: {error}")),
                }
            }
        }
        if let Some(store) = &self.presentation_store {
            match store.save(Path::new(&effects.dashboard_path), &presentation_records) {
                Ok(()) => self.presentation_warning = None,
                Err(error) => {
                    let error = error.to_string();
                    self.presentation_warning =
                        Some(format!("Could not save Dashboard Editor layout: {error}"));
                    presentation_error = Some(error);
                }
            }
        }
        if effects.close_requested && (!save_requested || dashboard_saved) {
            self.open = false;
        }
        Some(DashboardEditorCompletion {
            dashboard_saved,
            presentation_error,
            editor_open: self.open,
            show_root: false,
        })
    }

    pub(crate) fn target_generation(&self) -> u64 {
        self.target_generation
    }

    #[cfg(test)]
    pub(crate) fn test_scroll_area_max_offset(&self) -> Option<(egui::Id, f32)> {
        self.test_scroll_metrics
    }

    #[cfg(test)]
    pub(crate) fn test_grid_rows_response(&self) -> Option<(egui::Id, egui::Rect)> {
        self.test_grid_rows_response
    }

    #[cfg(test)]
    pub(crate) fn test_grid_rows(&self) -> u8 {
        self.config.grid.rows
    }

    #[cfg(test)]
    pub(crate) fn test_set_grid_rows(&mut self, rows: u8) {
        self.config.grid.rows = rows.clamp(1, 12);
    }

    #[cfg(test)]
    pub(crate) fn test_slot_token(&self, index: usize) -> Option<u64> {
        self.slot_tokens.get(index).copied()
    }

    #[cfg(test)]
    pub(crate) fn test_slot(&self, index: usize) -> Option<&SlotConfig> {
        self.config.slots.get(index)
    }

    #[cfg(test)]
    pub(crate) fn test_slot_expanded(&self, index: usize) -> Option<bool> {
        self.slot_presentations
            .get(index)
            .map(|presentation| presentation.expanded)
    }

    #[cfg(test)]
    pub(crate) fn test_selected_slot(&self) -> Option<usize> {
        self.selected_slot
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
        let presentation_id = self.next_presentation_id();
        self.config.slots.push(slot);
        self.slot_tokens.push(token);
        self.slot_presentations.push(
            crate::dashboard::editor_presentation::DashboardSlotPresentation {
                id: presentation_id,
                expanded: true,
            },
        );
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
                let expanded = if let Some(store) = &self.presentation_store {
                    match store.load(Path::new(&self.path), &self.config.slots) {
                        Ok(expanded) => {
                            self.presentation_warning = None;
                            expanded
                        }
                        Err(error) => {
                            self.presentation_warning = Some(format!(
                                "Saved Dashboard Editor layout could not be loaded; sections defaulted open: {error}"
                            ));
                            vec![true; self.config.slots.len()]
                        }
                    }
                } else {
                    vec![true; self.config.slots.len()]
                };
                self.initialize_presentations(&self.config.slots.clone(), &expanded);
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

    fn persist_config(
        &self,
        registry: &WidgetRegistry,
        path: &str,
        config: DashboardConfig,
    ) -> anyhow::Result<DashboardConfig> {
        DashboardConfig::replace(path, registry, config)
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
        let mut host_request = None;
        self.ui_with_jump_mode_inner(
            ctx,
            registry,
            settings_ctx,
            require_confirm_destructive,
            parent_instance,
            jump_scope,
            confirmation_scope,
            native_ui_owner,
            native_confirmation_owner,
            first_ui_pass,
            None,
            UtilityPresentation::Docked,
            &mut host_request,
            false,
            false,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn ui_with_jump_mode_inner(
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
        mut body_ui: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
        host_request: &mut Option<UtilityPresentation>,
        host_control_allowed: bool,
        show_host_control: bool,
        root_provenance: Option<ViewportPassProvenance>,
    ) -> bool {
        if !self.open {
            return false;
        }
        settings_ctx.begin_native_combo_anchor_pass();
        self.ensure_slot_tokens();
        self.ensure_presentations();
        #[cfg(test)]
        {
            self.test_grid_rows_response = None;
            self.test_host_button = None;
            self.test_save_button = None;
        }
        let mut save_requested = false;
        let mut presentation_updates = Vec::new();
        let mut close_requested = false;
        let mut open = self.open;
        let mut authorized_visible_pass = false;
        let mut draft_snapshot = None;
        let mut visual_order = 0_u64;
        let mut child_remove_confirmed = None;
        let mut child_cancelled = false;
        let mut reset_result = ConfirmationResult::None;
        let mut confirmation_opened_this_pass = false;
        let mut draw_body = |ui: &mut egui::Ui| {
            authorized_visible_pass =
                native_ui_owner && first_ui_pass && ui.is_visible() && !ui.is_sizing_pass();
            if !authorized_visible_pass {
                draft_snapshot = Some(DashboardEditorDraftSnapshot::capture(self));
            }
            ui.scope_builder(
                    egui::UiBuilder::new().id(egui::Id::new(DASHBOARD_EDITOR_BODY_ID)),
                    |ui| {
                if show_host_control {
                    ui.horizontal(|ui| {
                        let (response, requested) = presentation_button(
                            ui,
                            presentation,
                            host_control_allowed,
                        );
                        register_editor_target(
                            jump_scope,
                            ui,
                            &response,
                            "host/presentation",
                            "Move Dashboard Editor",
                            crate::gui::jump_mode::TargetKind::Button,
                            &mut visual_order,
                        );
                        #[cfg(test)]
                        if ui.is_visible() && !ui.is_sizing_pass() {
                            self.test_host_button = Some((
                                response.id,
                                response.rect,
                                ui.clip_rect(),
                                host_control_allowed && ui.is_enabled(),
                            ));
                        }
                        if response.clicked() && host_control_allowed {
                            *host_request = Some(requested);
                        }
                    });
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
                        #[cfg(test)]
                        {
                            self.test_grid_rows_response = Some((response.id, response.rect));
                        }
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
                    if let Some(err) = &self.presentation_warning {
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
                            #[cfg(test)]
                            if ui.is_visible() && !ui.is_sizing_pass() {
                                self.test_save_button = Some((
                                    response.id,
                                    response.rect,
                                    ui.clip_rect(),
                                    authorized_visible_pass && response.enabled(),
                                ));
                            }
                            register_editor_target(jump_scope, ui, &response, "save", "Save dashboard", crate::gui::jump_mode::TargetKind::Button, &mut visual_order);
                            if response.clicked() && authorized_visible_pass {
                                save_requested = true;
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
                        let presentation_id = self.presentation_id_at(idx);
                        let mut expanded_this_pass = self.slot_presentations[idx].expanded;
                        let mut slot = original_slot.clone();
                        let mut edited = false;
                        let mut swapped = false;
                        let mut section_response = None;
                        let mut section_order = visual_order;
                        ui.scope_builder(
                            egui::UiBuilder::new()
                                .id(ui.id().with(("dashboard-editor-slot", presentation_id))),
                            |ui| {
                            let collapsing_id = ui.id().with(("slot-collapse", presentation_id));
                            let mut state = CollapsingState::load_with_default_open(
                                ui.ctx(),
                                collapsing_id,
                                self.slot_presentations[idx].expanded,
                            );
                            if self.slot_expand_all {
                                state.set_open(true);
                                state.store(ui.ctx());
                            } else if self.slot_collapse_all {
                                state.set_open(false);
                                state.store(ui.ctx());
                            } else {
                                state.set_open(self.slot_presentations[idx].expanded);
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
                            expanded_this_pass = header_response.is_open();
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
                        if self.slot_presentations[idx].expanded != expanded_this_pass {
                            presentation_updates.push((presentation_id, expanded_this_pass));
                        }
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
                    },
                );
        };
        if let Some(ui) = body_ui.as_deref_mut() {
            draw_body(ui);
        } else {
            let _ = egui::Window::new("Dashboard Editor")
                .resizable(true)
                .show_utility(
                    ctx,
                    "dashboard-editor",
                    egui::Id::new("Dashboard Editor"),
                    Some(&mut open),
                    crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::new(
                        crate::gui::utility_dialog_size::UtilityWindowSizeDefault::intrinsic(),
                        egui::Vec2::ZERO,
                        [None, None],
                    ),
                    draw_body,
                );
        }
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
            close_requested = !open;
        }
        if authorized_visible_pass
            && first_ui_pass
            && !ctx.will_discard()
            && (save_requested || !presentation_updates.is_empty() || close_requested)
        {
            let root_pass = root_provenance
                .unwrap_or_else(|| ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT));
            self.pending_root_effects = Some(PendingDashboardEditorEffects {
                root_frame_nr: root_pass.frame_nr,
                root_pass_nr: root_pass.pass_nr,
                target_generation: self.target_generation,
                dashboard_path: self.path.clone(),
                presentation_updates,
                close_requested,
                save_config: save_requested.then(|| self.config.clone()),
            });
        }
        false
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn show_with_host(
        &mut self,
        ctx: &egui::Context,
        registry: &WidgetRegistry,
        settings_ctx: WidgetSettingsContext<'_>,
        require_confirm_destructive: bool,
        owner_instance: u64,
        first_ui_pass: bool,
        root_visible: bool,
        application_input_allowed: bool,
        root_input_allowed: bool,
        root_native_ui_owner: bool,
        root_confirmation_owner: bool,
        jump_mode_enabled: bool,
        root_confirmation_scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    ) {
        if self.owner_instance != owner_instance {
            if self.is_popped_out() {
                if let Some(identity) = dashboard_editor_native_identity(
                    self.host_state.identity(),
                    self.owner_instance,
                ) {
                    crate::gui::utility_dialog_size::close_native_utility_geometry_viewport(
                        ctx, identity,
                    );
                    ctx.send_viewport_cmd_to(identity.viewport_id, egui::ViewportCommand::Close);
                }
                self.host_state.request_focus();
            }
            self.pending_host_frame = None;
            self.pending_root_effects = None;
            self.retained_host_checkpoint = None;
            self.native_jump_mode.cancel_all();
            self.native_jump_frame = None;
        }
        self.owner_instance = owner_instance;
        let root_pass = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);

        let same_frame_checkpoint = self
            .retained_host_checkpoint
            .as_ref()
            .filter(|(pass, _)| pass.frame_nr == root_pass.frame_nr)
            .cloned();
        let mut draft_checkpoint = if let Some((checkpoint_pass, checkpoint)) =
            same_frame_checkpoint
        {
            if root_pass.pass_nr > checkpoint_pass.pass_nr {
                checkpoint.clone().restore(self);
            }
            Some(checkpoint)
        } else {
            if let Some((previous_pass, previous_checkpoint)) = self.retained_host_checkpoint.take()
                && !previous_pass.was_retained(ctx)
            {
                previous_checkpoint.restore(self);
                self.property_identity_store.clear();
                self.native_combo_anchors.clear();
            }
            None
        };

        if self.pending_root_effects.as_ref().is_some_and(|effects| {
            effects.root_frame_nr != root_pass.frame_nr || effects.root_pass_nr != root_pass.pass_nr
        }) {
            self.pending_root_effects = None;
        }

        let host = self.host_state.identity();
        let mut callback_observed = false;
        let mut callback_class = egui::ViewportClass::Root;
        let mut observed_owner_viewport = egui::ViewportId::ROOT;
        let mut owner_pass = None;
        let mut owner_pass_accepted = false;
        let mut native_viewport = None;
        let mut embedded_root_layer = None;
        let mut embedded_root_focused_id = None;
        let mut host_request = None;
        let mut close_requested = self.pending_launcher_close;
        let mut os_close_requested = false;
        let mut focus_host = false;
        let mut native_jump_mode = std::mem::take(&mut self.native_jump_mode);
        let mut native_jump_frame = self.native_jump_frame.take();

        match host.host {
            UtilityPresentation::Docked => {
                if self.open && (root_visible || self.host_state.bootstrap_pending()) {
                    callback_observed = true;
                    callback_class = egui::ViewportClass::Root;
                    observed_owner_viewport = egui::ViewportId::ROOT;
                    let pass = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
                    owner_pass = Some(pass);
                    owner_pass_accepted = first_ui_pass && !ctx.will_discard();
                    if draft_checkpoint.is_none()
                        && owner_pass_accepted
                        && has_dashboard_editor_input_event(ctx)
                    {
                        draft_checkpoint = Some(DashboardEditorDraftSnapshot::capture(self));
                    }
                    let confirmation_open =
                        self.jump_mode_pending_confirmation_identity().is_some();
                    let keyboard_close = owner_pass_accepted
                        && root_input_allowed
                        && root_native_ui_owner
                        && !confirmation_open
                        && native_jump_mode.active_session().is_none()
                        && ctx.input_mut(|input| {
                            input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                                || input.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                        });
                    close_requested |= keyboard_close;
                    let owner_current = owner_pass_accepted
                        && root_input_allowed
                        && root_native_ui_owner
                        && !close_requested;
                    focus_host = self.host_state.focus_pending() && owner_current;
                    let _ = self.ui_with_jump_mode_inner(
                        ctx,
                        registry,
                        settings_ctx,
                        require_confirm_destructive,
                        Some(owner_instance),
                        settings_ctx.jump_mode_scope,
                        root_confirmation_scope,
                        owner_current,
                        root_confirmation_owner && owner_pass_accepted,
                        owner_pass_accepted,
                        None,
                        UtilityPresentation::Docked,
                        &mut host_request,
                        owner_current && !egui::Popup::is_any_open(ctx),
                        true,
                        Some(root_pass),
                    );
                    if let Some(layer) =
                        crate::gui::jump_mode::ui::NavigationDialogKind::DashboardEditor.layer_id()
                    {
                        embedded_root_layer = Some(layer);
                        embedded_root_focused_id = ctx.memory(|memory| memory.focused());
                    }
                }
            }
            UtilityPresentation::Popout => {
                let viewport_id = dashboard_editor_viewport_id(owner_instance);
                let viewport_builder = dashboard_editor_viewport_builder(owner_instance);
                let focus_pending = self.host_state.focus_pending();
                let native_anchors = std::cell::RefCell::new(Vec::new());
                native_jump_mode.set_enabled(jump_mode_enabled);
                ctx.show_viewport_immediate(viewport_id, viewport_builder, |child, class| {
                    callback_observed = true;
                    callback_class = class;
                    let child_ctx = child.ctx().clone();
                    let independent = class == egui::ViewportClass::Immediate
                        && child_ctx.viewport_id() == viewport_id;
                    observed_owner_viewport = if independent {
                        viewport_id
                    } else {
                        egui::ViewportId::ROOT
                    };
                    let current_owner_pass =
                        ViewportPassProvenance::current(&child_ctx, child_ctx.viewport_id());
                    owner_pass = Some(current_owner_pass);
                    let pass_accepted = first_ui_pass
                        && child.current_pass_index() == 0
                        && !ctx.will_discard()
                        && !child_ctx.will_discard();
                    owner_pass_accepted = pass_accepted;
                    if independent {
                        native_viewport = Some(child.input(|input| input.viewport().clone()));
                    }

                    let visible_body =
                        independent || (child.is_visible() && !child.is_sizing_pass());
                    let focused = child_ctx.input(|input| input.focused);
                    let modal_open =
                        child_ctx.memory(|memory| memory.top_modal_layer().is_some());
                    let popup_open = egui::Popup::is_any_open(&child_ctx);
                    let expected_layer = if independent {
                        egui::LayerId::background()
                    } else {
                        child.layer_id()
                    };
                    let editor_binding = crate::gui::jump_mode::ui::NavigationViewportBinding {
                        viewport: viewport_id,
                        owner: crate::gui::jump_mode::ui::NavigationOwner::Dialog {
                            kind: crate::gui::jump_mode::ui::NavigationDialogKind::DashboardEditor,
                            instance: owner_instance,
                        },
                        base_layer: egui::LayerId::background(),
                    };

                    let mut native_scope = None;
                    let mut native_confirmation_scope = None;
                    let native_owner_current;
                    let mut native_confirmation_current = false;
                    let confirmation_identity =
                        self.jump_mode_pending_confirmation_identity();
                    let mut active_binding = editor_binding;
                    if independent {
                        let frame_number = child_ctx.cumulative_frame_nr();
                        if native_jump_frame != Some(frame_number) {
                            native_jump_mode.begin_frame();
                            native_jump_frame = Some(frame_number);
                        }
                        let confirmation_binding = confirmation_identity
                            .map(|(confirmation_instance, kind)| {
                                crate::gui::jump_mode::ui::NavigationViewportBinding {
                                    viewport: viewport_id,
                                    owner: crate::gui::jump_mode::ui::NavigationOwner::DashboardEditorConfirmation {
                                        parent_instance: owner_instance,
                                        confirmation_instance,
                                        kind,
                                    },
                                    base_layer: crate::gui::jump_mode::ui::dashboard_editor_confirmation_layer_id(
                                        owner_instance,
                                        confirmation_instance,
                                        kind,
                                    ),
                                }
                            })
                            .filter(|binding| {
                                crate::gui::jump_mode::ui::visible_top_middle_layer(&child_ctx)
                                    == Some(binding.base_layer)
                            });
                        active_binding = confirmation_binding.unwrap_or(editor_binding);
                        let navigation_owner = native_jump_mode.update_navigation_owner_for_viewport(
                            &child_ctx,
                            active_binding,
                            application_input_allowed,
                        );
                        let jump_owner_current = navigation_owner == active_binding.owner
                            && native_jump_mode.frame_navigation_owner() == active_binding.owner
                            && jump_mode_enabled;
                        native_jump_mode.handle_viewport_input(
                            &child_ctx,
                            jump_owner_current,
                            pass_accepted,
                            viewport_id,
                        );
                        let combo_current = popup_open
                            && self.native_combo_popup_is_current(
                                &child_ctx,
                                egui::LayerId::background(),
                            );
                        let focus_layer_current =
                            crate::gui::jump_mode::ui::focused_response_layer(&child_ctx)
                                .is_none_or(|layer| {
                                    layer == expected_layer
                                        || (combo_current && layer.order == egui::Order::Foreground)
                                });
                        native_owner_current = pass_accepted
                            && application_input_allowed
                            && visible_body
                            && focused
                            && !modal_open
                            && crate::gui::jump_mode::ui::visible_top_middle_layer(&child_ctx)
                                .is_none()
                            && (!popup_open || combo_current)
                            && active_binding == editor_binding
                            && focus_layer_current;
                        native_confirmation_current = pass_accepted
                            && application_input_allowed
                            && visible_body
                            && focused
                            && !modal_open
                            && confirmation_binding.is_some_and(|binding| {
                                binding == active_binding
                                    && crate::gui::jump_mode::ui::visible_top_middle_layer(
                                        &child_ctx,
                                    ) == Some(binding.base_layer)
                                    && crate::gui::jump_mode::ui::focused_response_layer(
                                        &child_ctx,
                                    )
                                    .is_none_or(|layer| {
                                        layer == binding.base_layer
                                            || layer == egui::LayerId::background()
                                            || layer.order != egui::Order::Middle
                                    })
                            });
                        native_jump_mode.begin_ui_pass();
                        if native_owner_current && jump_mode_enabled && jump_owner_current {
                            native_scope = native_jump_mode.scope_for_viewport_binding(
                                crate::gui::jump_mode::SurfaceId::dialog(
                                    "dashboard-editor",
                                    owner_instance,
                                ),
                                crate::gui::jump_mode::TargetGeneration(self.target_generation),
                                DASHBOARD_EDITOR_VIEWPORT_KEY,
                                editor_binding,
                            );
                        }
                        if native_confirmation_current
                            && jump_mode_enabled
                            && jump_owner_current
                            && let (Some(binding), Some((confirmation_instance, kind))) =
                                (confirmation_binding, confirmation_identity)
                        {
                            native_confirmation_scope = native_jump_mode.scope_for_viewport_binding(
                                crate::gui::jump_mode::SurfaceId::popup(
                                    kind.surface_kind(),
                                    confirmation_instance,
                                    crate::gui::jump_mode::SurfaceId::dialog(
                                        "dashboard-editor",
                                        owner_instance,
                                    ),
                                ),
                                crate::gui::jump_mode::TargetGeneration(confirmation_instance),
                                kind.surface_kind(),
                                binding,
                            );
                        }
                        let confirmation_open =
                            self.jump_mode_pending_confirmation_identity().is_some();
                        if native_owner_current
                            && !confirmation_open
                            && !popup_open
                            && native_jump_mode.active_session().is_none()
                        {
                            close_requested |= child_ctx.input_mut(|input| {
                                input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                                    || input.consume_key(
                                        egui::Modifiers::COMMAND,
                                        egui::Key::W,
                                    )
                            });
                        }
                        os_close_requested = pass_accepted
                            && child.input(|input| input.viewport().close_requested());
                        focus_host = focus_pending
                            && application_input_allowed
                            && independent
                            && visible_body
                            && !modal_open
                            && (crate::gui::jump_mode::ui::visible_top_middle_layer(&child_ctx)
                                .is_none()
                                || confirmation_binding.is_some());
                    } else {
                        let known_combo = popup_open
                            && self.native_combo_popup_is_current(&child_ctx, expected_layer);
                        let root_owner_current = pass_accepted
                            && root_input_allowed
                            && root_native_ui_owner
                            && visible_body
                            && focused
                            && crate::gui::jump_mode::ui::focused_response_layer(&child_ctx)
                                .is_none_or(|layer| {
                                    layer == expected_layer
                                        || (known_combo && layer.order == egui::Order::Foreground)
                                });
                        native_owner_current = root_owner_current;
                        let confirmation_open =
                            self.jump_mode_pending_confirmation_identity().is_some();
                        if root_owner_current
                            && !confirmation_open
                            && !popup_open
                            && native_jump_mode.active_session().is_none()
                        {
                            close_requested |= child_ctx.input_mut(|input| {
                                input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                                    || input.consume_key(
                                        egui::Modifiers::COMMAND,
                                        egui::Key::W,
                                    )
                            });
                        }
                        focus_host = focus_pending && root_owner_current && !modal_open;
                        if child_ctx.viewport_id() == egui::ViewportId::ROOT && pass_accepted {
                            embedded_root_layer = Some(child.layer_id());
                            embedded_root_focused_id =
                                child_ctx.memory(|memory| memory.focused());
                        }
                    }

                    let body_owner = if independent {
                        native_owner_current && !close_requested && !os_close_requested
                    } else {
                        native_owner_current && !close_requested
                    };
                    let confirmation_owner = if independent {
                        native_confirmation_current && !close_requested && !os_close_requested
                    } else {
                        root_confirmation_owner && pass_accepted && !close_requested
                    };
                    let host_controls_allowed = body_owner
                        && !modal_open
                        && (!popup_open
                            || (independent
                                && self.native_combo_popup_is_current(
                                    &child_ctx,
                                    egui::LayerId::background(),
                                )));

                    if independent {
                        if draft_checkpoint.is_none()
                            && pass_accepted
                            && has_dashboard_editor_input_event(&child_ctx)
                        {
                            draft_checkpoint =
                                Some(DashboardEditorDraftSnapshot::capture(self));
                        }
                        let mut child_settings = settings_ctx
                            .with_jump_mode_scope(native_scope.as_ref());
                        child_settings.native_combo_anchors = Some(&native_anchors);
                        egui::CentralPanel::default().show(child, |ui| {
                            let _ = self.ui_with_jump_mode_inner(
                                &child_ctx,
                                registry,
                                child_settings,
                                require_confirm_destructive,
                                Some(owner_instance),
                                native_scope.as_ref(),
                                native_confirmation_scope.as_ref(),
                                body_owner,
                                confirmation_owner,
                                pass_accepted,
                                Some(ui),
                                UtilityPresentation::Popout,
                                &mut host_request,
                                host_controls_allowed,
                                true,
                                Some(root_pass),
                            );
                        });
                        native_jump_mode.finish_ui_pass();
                        let after_owner = native_jump_mode.update_navigation_owner_for_viewport(
                            &child_ctx,
                            active_binding,
                            application_input_allowed,
                        );
                        if native_jump_mode.prepare_pass_for_viewport(
                            &child_ctx,
                            active_binding,
                            after_owner,
                            application_input_allowed,
                        ) {
                            crate::gui::jump_mode::paint::paint(
                                &child_ctx,
                                &native_jump_mode,
                            );
                        }
                        if !child_ctx.will_discard() {
                            native_jump_mode.complete_frame_for_viewport(
                                &child_ctx,
                                active_binding,
                                after_owner,
                                application_input_allowed,
                            );
                        }
                    } else {
                        if draft_checkpoint.is_none()
                            && pass_accepted
                            && has_dashboard_editor_input_event(&child_ctx)
                        {
                            draft_checkpoint = Some(DashboardEditorDraftSnapshot::capture(self));
                        }
                        let mut root_settings =
                            settings_ctx.with_jump_mode_scope(settings_ctx.jump_mode_scope);
                        root_settings.native_combo_anchors = Some(&native_anchors);
                        let _ = self.ui_with_jump_mode_inner(
                            &child_ctx,
                            registry,
                            root_settings,
                            require_confirm_destructive,
                            Some(owner_instance),
                            settings_ctx.jump_mode_scope,
                            root_confirmation_scope,
                            body_owner,
                            confirmation_owner,
                            pass_accepted,
                            Some(child),
                            UtilityPresentation::Popout,
                            &mut host_request,
                            host_controls_allowed,
                            true,
                            Some(root_pass),
                        );
                    }
                });
                self.native_combo_anchors = native_anchors.into_inner();
            }
        }

        if callback_observed {
            if self
                .retained_host_checkpoint
                .as_ref()
                .is_none_or(|(pass, _)| pass.frame_nr != root_pass.frame_nr)
                && let Some(checkpoint) = draft_checkpoint.as_ref()
            {
                self.retained_host_checkpoint = Some((root_pass, checkpoint.clone()));
            }
            self.pending_host_frame = Some(DashboardEditorHostFrame {
                root_pass,
                host,
                owner_instance,
                target_generation: self.target_generation,
                dashboard_path: self.path.clone(),
                owner_viewport: observed_owner_viewport,
                owner_pass,
                owner_pass_accepted,
                callback_observed,
                callback_class,
                native_viewport,
                embedded_root_layer,
                embedded_root_focused_id,
                host_request,
                close_requested,
                os_close_requested,
                focus_host,
                draft_checkpoint,
                native_jump_mode,
                native_jump_frame,
            });
        } else {
            self.native_jump_mode = native_jump_mode;
            self.native_jump_frame = native_jump_frame;
        }
    }

    pub(crate) fn on_host_frame_complete(
        &mut self,
        ctx: &egui::Context,
        registry: &WidgetRegistry,
        active_dashboard_path: &str,
    ) -> DashboardEditorCompletion {
        let Some(mut frame) = self.pending_host_frame.take() else {
            return DashboardEditorCompletion {
                dashboard_saved: false,
                presentation_error: None,
                editor_open: self.open,
                show_root: self.take_show_root_after_host_change(),
            };
        };
        let owner_pass_retained = frame.callback_observed
            && frame.owner_pass_accepted
            && frame.owner_pass.is_some_and(|pass| {
                pass.viewport_id == frame.owner_viewport && pass.was_retained(ctx)
            });
        let same_owner = frame.owner_instance == self.owner_instance
            && frame.dashboard_path == self.path
            && frame.target_generation == self.target_generation
            && active_dashboard_path == frame.dashboard_path
            && frame.host == self.host_state.identity();
        if !frame.root_pass.was_retained(ctx)
            || frame.host != self.host_state.identity()
            || frame.owner_instance != self.owner_instance
            || frame.target_generation != self.target_generation
            || frame.dashboard_path != self.path
            || active_dashboard_path != frame.dashboard_path
            || !owner_pass_retained
        {
            if same_owner && let Some(checkpoint) = frame.draft_checkpoint.take() {
                checkpoint.restore(self);
                self.property_identity_store.clear();
                self.native_combo_anchors.clear();
            }
            if frame.os_close_requested {
                ctx.send_viewport_cmd_to(
                    dashboard_editor_viewport_id(frame.owner_instance),
                    egui::ViewportCommand::CancelClose,
                );
            }
            if self.pending_root_effects.as_ref().is_some_and(|effects| {
                effects.root_frame_nr == frame.root_pass.frame_nr
                    && effects.root_pass_nr == frame.root_pass.pass_nr
            }) {
                self.pending_root_effects = None;
            }
            if self
                .retained_host_checkpoint
                .as_ref()
                .is_some_and(|(pass, _)| pass.frame_nr == frame.root_pass.frame_nr)
            {
                self.retained_host_checkpoint = None;
            }
            frame.native_jump_mode.cancel_all();
            self.native_jump_mode = frame.native_jump_mode;
            self.native_jump_frame = None;
            return DashboardEditorCompletion {
                dashboard_saved: false,
                presentation_error: None,
                editor_open: self.open,
                show_root: false,
            };
        }

        if self
            .retained_host_checkpoint
            .as_ref()
            .is_some_and(|(pass, _)| pass.frame_nr == frame.root_pass.frame_nr)
        {
            self.retained_host_checkpoint = None;
        }
        if frame.os_close_requested {
            ctx.send_viewport_cmd_to(
                dashboard_editor_viewport_id(frame.owner_instance),
                egui::ViewportCommand::CancelClose,
            );
        }
        self.host_state.complete_bootstrap(
            frame.host,
            frame.embedded_root_layer,
            frame.embedded_root_focused_id,
        );
        self.native_jump_mode = std::mem::take(&mut frame.native_jump_mode);
        self.native_jump_frame = frame.native_jump_frame;
        if frame.focus_host {
            if frame.callback_class == egui::ViewportClass::Immediate {
                ctx.send_viewport_cmd_to(
                    dashboard_editor_viewport_id(frame.owner_instance),
                    egui::ViewportCommand::Focus,
                );
            } else if let Some(layer) = frame.embedded_root_layer {
                ctx.move_to_top(layer);
            }
            self.host_state.complete_focus(frame.host);
        }

        if frame.close_requested || frame.os_close_requested {
            match self.pending_root_effects.as_mut() {
                Some(effects)
                    if effects.root_frame_nr == frame.root_pass.frame_nr
                        && effects.root_pass_nr == frame.root_pass.pass_nr
                        && effects.target_generation == frame.target_generation
                        && effects.dashboard_path == frame.dashboard_path =>
                {
                    effects.close_requested = true;
                }
                _ => {
                    self.pending_root_effects = Some(PendingDashboardEditorEffects {
                        root_frame_nr: frame.root_pass.frame_nr,
                        root_pass_nr: frame.root_pass.pass_nr,
                        target_generation: frame.target_generation,
                        dashboard_path: frame.dashboard_path.clone(),
                        presentation_updates: Vec::new(),
                        close_requested: true,
                        save_config: None,
                    });
                }
            }
        }

        let mut completion = self
            .on_ui_frame_complete(ctx, registry, active_dashboard_path)
            .unwrap_or(DashboardEditorCompletion {
                dashboard_saved: false,
                presentation_error: None,
                editor_open: self.open,
                show_root: false,
            });
        let native_identity = dashboard_editor_native_identity(frame.host, frame.owner_instance);
        let closing = !self.open;
        if let (Some(identity), Some(viewport_info)) =
            (native_identity, frame.native_viewport.as_ref())
            && frame.callback_class == egui::ViewportClass::Immediate
            && !closing
            && frame.host_request != Some(UtilityPresentation::Docked)
        {
            crate::gui::utility_dialog_size::apply_native_utility_geometry_restore(
                ctx,
                frame.callback_class,
                true,
                &dashboard_editor_native_title(frame.owner_instance),
                identity,
                viewport_info,
                DASHBOARD_EDITOR_DEFAULT_SIZE,
                Some(DASHBOARD_EDITOR_MINIMUM_SIZE),
                None,
            );
        }

        self.pending_launcher_close = false;
        if !self.open {
            if let Some(identity) = native_identity {
                crate::gui::utility_dialog_size::close_native_utility_geometry_viewport(
                    ctx, identity,
                );
                ctx.send_viewport_cmd_to(identity.viewport_id, egui::ViewportCommand::Close);
            }
            self.host_state.retire_to_docked();
            self.native_jump_mode.cancel_all();
            self.native_jump_frame = None;
            self.show_root_after_host_change = false;
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        } else if let Some(requested) = frame.host_request
            && requested != self.host_state.presentation()
        {
            if requested == UtilityPresentation::Docked {
                if let Some(identity) = native_identity {
                    crate::gui::utility_dialog_size::close_native_utility_geometry_viewport(
                        ctx, identity,
                    );
                    ctx.send_viewport_cmd_to(identity.viewport_id, egui::ViewportCommand::Close);
                }
            }
            if self.host_state.transition(requested) {
                self.native_jump_mode.cancel_all();
                self.native_jump_frame = None;
                if requested == UtilityPresentation::Docked {
                    self.show_root_after_host_change = true;
                    if let Some(layer) = self.host_layer_if_root() {
                        ctx.move_to_top(layer);
                    }
                }
                ctx.request_repaint_of(egui::ViewportId::ROOT);
                if requested == UtilityPresentation::Popout {
                    ctx.request_repaint_of(dashboard_editor_viewport_id(frame.owner_instance));
                }
            }
        }
        completion.editor_open = self.open;
        completion.show_root = self.take_show_root_after_host_change();
        completion
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
        self.slot_presentations.remove(idx);
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
                self.slot_presentations.swap(first, second);
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
                let presentation_id = self.next_presentation_id();
                self.slot_presentations.push(
                    crate::dashboard::editor_presentation::DashboardSlotPresentation {
                        id: presentation_id,
                        expanded: true,
                    },
                );
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

    fn dashboard_host_frame(
        dialog: &mut DashboardEditorDialog,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        root_visible: bool,
        root_input_allowed: bool,
    ) -> DashboardEditorCompletion {
        super::super::render::with_immediate_viewport_renderer(ctx, || {
            let registry = WidgetRegistry::with_defaults();
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1_000.0, 760.0),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    dialog.show_with_host(
                        ui.ctx(),
                        &registry,
                        WidgetSettingsContext::empty(),
                        false,
                        77,
                        ui.ctx().current_pass_index() == 0,
                        root_visible,
                        true,
                        root_input_allowed,
                        root_input_allowed,
                        false,
                        false,
                        None,
                    );
                },
            );
            output.drop_without_applying_deltas();
            let active_path = dialog.path.clone();
            dialog.on_host_frame_complete(ctx, &registry, &active_path)
        })
    }

    fn pointer_events(point: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }

    #[test]
    fn dashboard_editor_visible_host_buttons_pop_out_and_dock_the_same_draft() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("dashboard.json");
        let registry = WidgetRegistry::with_defaults();
        let invalid_document = b"{broken";
        std::fs::write(&path, invalid_document).unwrap();
        let path = path.to_string_lossy().into_owned();
        let mut dialog = DashboardEditorDialog::default();
        dialog.open(&path, &registry);
        dialog.config.grid.rows = 7;
        let original_config = dialog.config.clone();
        let original_selection = dialog.selected_slot;
        let ctx = egui::Context::default();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);

        for _ in 0..3 {
            dashboard_host_frame(&mut dialog, &ctx, Vec::new(), true, true);
        }
        // The first draw initializes slot identities for the fallback draft.
        let original_target_generation = dialog.target_generation;
        let (_, button, clip, enabled) = dialog.test_host_button();
        assert!(enabled);
        assert!(clip.contains_rect(button));
        for pressed in [true, false] {
            dashboard_host_frame(
                &mut dialog,
                &ctx,
                pointer_events(button.center(), pressed),
                true,
                true,
            );
        }
        assert_eq!(
            dialog.host_state.presentation(),
            UtilityPresentation::Popout
        );
        assert_eq!(dialog.config, original_config);
        assert_eq!(dialog.selected_slot, original_selection);

        for _ in 0..3 {
            dashboard_host_frame(&mut dialog, &ctx, Vec::new(), false, false);
        }
        assert_eq!(
            dialog.host_state.presentation(),
            UtilityPresentation::Popout
        );
        let (_, _, child_clip, child_enabled) = dialog.test_host_button();
        assert!(
            child_enabled,
            "the native editor remains interactive with ROOT hidden"
        );
        assert!(child_clip.is_positive());
        let (_, save_button, save_clip, save_enabled) = dialog.test_save_button();
        assert!(save_enabled, "Save is enabled for the pending draft");
        assert!(save_clip.contains_rect(save_button));
        let viewport_id = dialog.test_viewport_id();
        for pressed in [true, false] {
            super::super::render::with_immediate_viewport_test_events_for(
                viewport_id,
                pointer_events(save_button.center(), pressed),
                || dashboard_host_frame(&mut dialog, &ctx, Vec::new(), false, false),
            );
        }
        assert!(dialog.open, "a rejected Save must keep the editor open");
        assert_eq!(
            dialog.config.grid.rows, 7,
            "a failed Save retains the draft"
        );
        assert!(
            dialog
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Failed to save"))
        );
        assert_eq!(std::fs::read(&path).unwrap(), invalid_document);

        for _ in 0..3 {
            dashboard_host_frame(&mut dialog, &ctx, Vec::new(), false, false);
        }
        let (_, child_button, child_clip, child_enabled) = dialog.test_host_button();
        assert!(child_enabled);
        assert!(child_clip.contains_rect(child_button));
        let mut dock_show_root = false;
        for pressed in [true, false] {
            let completion = super::super::render::with_immediate_viewport_test_events_for(
                viewport_id,
                pointer_events(child_button.center(), pressed),
                || dashboard_host_frame(&mut dialog, &ctx, Vec::new(), false, false),
            );
            dock_show_root = completion.show_root;
        }
        assert_eq!(
            dialog.host_state.presentation(),
            UtilityPresentation::Docked
        );
        assert!(dock_show_root);
        assert!(dialog.open);
        assert_eq!(dialog.config, original_config);
        assert_eq!(dialog.selected_slot, original_selection);
        assert_eq!(dialog.target_generation, original_target_generation);
    }

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
        let save = dialog.persist_config(&registry, &dialog.path, dialog.config.clone());
        assert!(save.is_err());
        assert_eq!(
            dialog.config.grid.rows, 9,
            "a failed save retains the draft"
        );
        assert_eq!(std::fs::read(temporary.path()).unwrap(), original);
    }

    #[test]
    fn failed_coalesced_save_and_close_keeps_editor_open_and_draft_retryable() {
        let temporary = tempfile::NamedTempFile::new().unwrap();
        let original = b"{broken";
        std::fs::write(temporary.path(), original).unwrap();
        let registry = WidgetRegistry::with_defaults();
        let mut dialog = DashboardEditorDialog::default();
        let path = temporary.path().to_string_lossy().into_owned();
        dialog.open(&path, &registry);
        dialog.config.grid.rows = 9;
        let ctx = egui::Context::default();
        let root_frame_nr = ctx.cumulative_frame_nr_for(egui::ViewportId::ROOT);
        let root_pass_nr = ctx.cumulative_pass_nr_for(egui::ViewportId::ROOT);
        dialog.pending_root_effects = Some(PendingDashboardEditorEffects {
            root_frame_nr,
            root_pass_nr,
            target_generation: dialog.target_generation,
            dashboard_path: path.clone(),
            presentation_updates: Vec::new(),
            close_requested: true,
            save_config: Some(dialog.config.clone()),
        });

        let output = ctx.run_ui(egui::RawInput::default(), |_| {});
        output.drop_without_applying_deltas();
        let completion = dialog
            .on_ui_frame_complete(&ctx, &registry, &path)
            .expect("the coalesced receipt belongs to the retained ROOT pass");

        assert!(!completion.dashboard_saved);
        assert!(completion.editor_open);
        assert!(dialog.open, "failed Save must leave the editor open");
        assert_eq!(dialog.config.grid.rows, 9, "failed Save keeps the draft");
        assert!(dialog.error.is_some());
        assert_eq!(std::fs::read(temporary.path()).unwrap(), original);
    }

    #[test]
    fn known_local_slot_changes_preserve_separate_presentation_identity() {
        let profile = tempfile::tempdir().unwrap();
        let dashboard_path = profile.path().join("dashboard.json");
        let registry = WidgetRegistry::with_defaults();
        let mut config = DashboardConfig::default();
        config.grid.rows = 1;
        config.grid.cols = 2;
        let mut weather = SlotConfig::with_widget("weather_site", 0, 0);
        weather.id = Some("weather-user-id".into());
        let mut pinned = SlotConfig::with_widget("pinned_commands", 0, 1);
        pinned.id = Some("pinned-user-id".into());
        config.slots = vec![weather.clone(), pinned.clone()];
        std::fs::write(
            &dashboard_path,
            serde_json::to_vec(&config).expect("the fixture dashboard serializes"),
        )
        .unwrap();

        let store =
            crate::dashboard::editor_presentation::DashboardEditorPresentationStore::for_profile(
                profile.path(),
            );
        store
            .save(&dashboard_path, &[(weather, false), (pinned, true)])
            .unwrap();
        let data_root = crate::platform::app_data::AppDataRoot::from_path(profile.path());
        let mut dialog = DashboardEditorDialog::with_profile_root(&data_root);
        dialog.open(dashboard_path.to_str().unwrap(), &registry);
        assert_eq!(dialog.test_slot_expanded(0), Some(false));
        assert_eq!(dialog.test_slot_expanded(1), Some(true));

        dialog.swap_slots(0, 1, &registry).unwrap();
        assert_eq!(
            dialog.test_slot(0).unwrap().id.as_deref(),
            Some("pinned-user-id")
        );
        assert_eq!(
            dialog.test_slot(1).unwrap().id.as_deref(),
            Some("weather-user-id")
        );
        assert_eq!(dialog.test_slot_expanded(0), Some(true));
        assert_eq!(dialog.test_slot_expanded(1), Some(false));

        dialog.add_slot(SlotConfig::with_widget("todo", 0, 2));
        assert_eq!(dialog.test_slot_expanded(2), Some(true));
        let added_presentation_id = dialog.slot_presentations[2].id;
        dialog.remove_slot_at(2);
        assert!(
            dialog
                .slot_presentations
                .iter()
                .all(|presentation| presentation.id != added_presentation_id)
        );

        let local_presentation_id = dialog.slot_presentations[0].id;
        dialog.config.slots[0].id = Some("locally-renamed-pinned".into());
        assert_eq!(dialog.slot_presentations[0].id, local_presentation_id);
        assert_eq!(dialog.test_slot_expanded(0), Some(true));
        assert_eq!(
            dialog.test_slot(0).unwrap().id.as_deref(),
            Some("locally-renamed-pinned")
        );

        let committed = dialog
            .persist_config(&registry, &dialog.path, dialog.config.clone())
            .unwrap();
        dialog.adopt_saved_config(committed);
        store
            .save(&dashboard_path, &dialog.current_presentation_records())
            .unwrap();
        let mut reopened = DashboardEditorDialog::with_profile_root(&data_root);
        reopened.open(dashboard_path.to_str().unwrap(), &registry);
        assert_eq!(reopened.test_slot_expanded(0), Some(true));
        assert_eq!(reopened.test_slot_expanded(1), Some(false));
        assert_eq!(
            reopened.test_slot(0).unwrap().id.as_deref(),
            Some("locally-renamed-pinned")
        );
        assert_eq!(
            reopened.test_slot(1).unwrap().id.as_deref(),
            Some("weather-user-id")
        );
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
