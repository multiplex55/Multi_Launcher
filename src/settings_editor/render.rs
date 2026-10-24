use super::catalog::{
    MainSettingsSection as TopLevelSettingsSection, SettingsAvailability, SettingsCatalogEntry,
    SettingsOwner, SettingsSection, ids as setting_ids,
};
use super::state::{
    PendingSettingsNavigation, SettingsEditor, SettingsNavigationDelivery,
    SettingsNavigationHighlight, SettingsNavigationStage,
};
use crate::gui::LauncherApp;
use crate::gui::jump_mode::{
    TargetKind,
    ui::{FocusDirective, JumpModeScope, RegistrationResult},
};
use crate::plugins::note::{NoteExternalOpen, NotePluginSettings};
use crate::settings::NoteViewMode;
use eframe::egui;
use std::sync::{Arc, Mutex};

const RADIAL_TAP_HOLD_EXPLANATION: &str = "Release before the threshold to toggle the launcher. Crossing the threshold opens the radial menu; releasing afterward never also toggles the launcher.";
const RADIAL_LEGACY_TIMING_EXPLANATION: &str = "Legacy timing: the launcher hotkey toggles immediately; named radial commands and configured direct triggers remain available.";

const SETTINGS_NAVIGATION_OBSERVER_ID: &str = "settings_navigation_frame_observer";
const SETTINGS_NAVIGATION_HIGHLIGHT_DURATION: std::time::Duration =
    std::time::Duration::from_millis(1800);

#[derive(Clone, Copy, Debug, Default)]
struct SettingsNavigationFrameOutcome {
    focus_requested: bool,
    scroll_requested: bool,
    superseded: bool,
    focused_response: Option<egui::Id>,
    target_seen: bool,
    highlighted_response: Option<egui::Id>,
    #[cfg(feature = "qa-framebuffer-capture")]
    qa_paint: Option<SettingsNavigationPaintedResponse>,
}

#[cfg(feature = "qa-framebuffer-capture")]
#[derive(Clone, Copy, Debug)]
struct SettingsNavigationPaintedResponse {
    response_id: egui::Id,
    rect: egui::Rect,
    egui_frame_nr: u64,
    egui_pass_index: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SettingsNavigationObservationMode {
    Focus,
    Highlight,
}

#[cfg(test)]
#[derive(Clone, Debug)]
struct TestPendingResponseObservation {
    control_id: String,
    sizing_pass: bool,
    ui_visible: bool,
    ui_enabled: bool,
    response_enabled: bool,
    response_rect: egui::Rect,
    ui_clip_rect: egui::Rect,
    clipped_rect_positive: bool,
    interaction_allowed: bool,
    response_has_focus: bool,
    focused_id: Option<egui::Id>,
}

struct SettingsNavigationFrameState {
    request: PendingSettingsNavigation,
    mode: SettingsNavigationObservationMode,
    first_ui_pass: bool,
    owner_current: bool,
    active: bool,
    outcome: SettingsNavigationFrameOutcome,
    #[cfg(test)]
    superseding_response: Option<String>,
    #[cfg(test)]
    response_observations: Vec<TestPendingResponseObservation>,
}

/// A cloneable view of one navigation request for the current egui frame.
/// It stores request provenance and owned outcomes only, never widget responses.
#[derive(Clone)]
pub(crate) struct SettingsNavigationFrameObserver {
    state: Arc<Mutex<SettingsNavigationFrameState>>,
}

impl SettingsNavigationFrameObserver {
    fn new(
        request: PendingSettingsNavigation,
        first_ui_pass: bool,
        owner_current: bool,
        mode: SettingsNavigationObservationMode,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(SettingsNavigationFrameState {
                request,
                mode,
                first_ui_pass,
                owner_current,
                active: true,
                outcome: SettingsNavigationFrameOutcome::default(),
                #[cfg(test)]
                superseding_response: None,
                #[cfg(test)]
                response_observations: Vec::new(),
            })),
        }
    }

    fn request(&self) -> Option<PendingSettingsNavigation> {
        self.state.lock().ok().map(|state| state.request.clone())
    }

    fn mode(&self) -> SettingsNavigationObservationMode {
        self.state
            .lock()
            .map(|state| state.mode)
            .unwrap_or(SettingsNavigationObservationMode::Highlight)
    }

    pub(crate) fn is_highlight_only(&self) -> bool {
        self.mode() == SettingsNavigationObservationMode::Highlight
    }

    fn outcome(&self) -> SettingsNavigationFrameOutcome {
        self.state
            .lock()
            .map(|state| state.outcome)
            .unwrap_or_default()
    }

    fn invalidate(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.active = false;
        }
    }

    #[cfg(test)]
    fn test_details(&self) -> (Option<String>, Vec<TestPendingResponseObservation>) {
        self.state
            .lock()
            .map(|state| {
                (
                    state.superseding_response.clone(),
                    state.response_observations.clone(),
                )
            })
            .unwrap_or_default()
    }

    #[cfg(test)]
    fn test_owner_was_current(&self) -> bool {
        self.state
            .lock()
            .map(|state| state.owner_current)
            .unwrap_or(false)
    }

    fn mark_highlighted_response(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        painted_rect: egui::Rect,
    ) {
        #[cfg(not(feature = "qa-framebuffer-capture"))]
        let _ = (ui, painted_rect);
        if let Ok(mut state) = self.state.lock() {
            state.outcome.highlighted_response = Some(response.id);
            #[cfg(feature = "qa-framebuffer-capture")]
            {
                state.outcome.qa_paint = Some(SettingsNavigationPaintedResponse {
                    response_id: response.id,
                    rect: painted_rect,
                    egui_frame_nr: ui.ctx().cumulative_frame_nr(),
                    egui_pass_index: ui.ctx().current_pass_index(),
                });
            }
        }
    }

    fn is_deferred_plugin(&self, name: &str) -> bool {
        self.request().is_some_and(|request| {
            request.stage == SettingsNavigationStage::DeferredControl
                && matches!(
                    (&request.owner, &request.section),
                    (
                        SettingsOwner::Plugin { name: owner_name, .. },
                        SettingsSection::Plugin {
                            name: section_name,
                            ..
                        }
                    ) if owner_name == name && section_name == name
                )
        })
    }

    fn is_deferred_notes(&self) -> bool {
        self.request().is_some_and(|request| {
            request.stage == SettingsNavigationStage::DeferredControl
                && request.section == SettingsSection::Notes
        })
    }

    fn observe_response(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        control_id: &str,
        kind: TargetKind,
    ) {
        #[cfg(test)]
        let diagnostic_sample = if self
            .request()
            .is_some_and(|request| request.control_id == control_id)
        {
            let visible_rect = response.rect.intersect(ui.clip_rect());
            Some(TestPendingResponseObservation {
                control_id: control_id.to_owned(),
                sizing_pass: ui.is_sizing_pass(),
                ui_visible: ui.is_visible(),
                ui_enabled: ui.is_enabled(),
                response_enabled: response.enabled(),
                response_rect: response.rect,
                ui_clip_rect: ui.clip_rect(),
                clipped_rect_positive: visible_rect.is_positive(),
                interaction_allowed: ui
                    .ctx()
                    .memory(|memory| memory.allows_interaction(response.layer_id)),
                response_has_focus: response.has_focus(),
                focused_id: ui.ctx().memory(|memory| memory.focused()),
            })
        } else {
            None
        };
        let (
            request,
            mode,
            first_ui_pass,
            owner_current,
            active,
            already_superseded,
            already_focused,
        ) = {
            let Ok(state) = self.state.lock() else {
                return;
            };
            #[cfg(test)]
            let mut state = state;
            #[cfg(test)]
            if let Some(sample) = diagnostic_sample {
                state.response_observations.push(sample);
            }
            (
                state.request.clone(),
                state.mode,
                state.first_ui_pass,
                state.owner_current,
                state.active,
                state.outcome.superseded,
                state.outcome.focus_requested,
            )
        };
        if !active || !owner_current || already_superseded {
            return;
        }

        if request.control_id == control_id {
            if mode == SettingsNavigationObservationMode::Highlight {
                if let Ok(mut state) = self.state.lock() {
                    state.outcome.target_seen = true;
                }
                if ui.is_sizing_pass() || !ui.is_visible() {
                    return;
                }
                if !ui.is_enabled() || !response.enabled() {
                    self.mark_superseded(format!("disabled-highlight:{control_id}"));
                    return;
                }
                let visible_rect = response.rect.intersect(ui.clip_rect());
                if !visible_rect.is_positive()
                    || !ui
                        .ctx()
                        .memory(|memory| memory.allows_interaction(response.layer_id))
                {
                    return;
                }
                paint_settings_navigation_highlight(ui, visible_rect);
                self.mark_highlighted_response(ui, response, visible_rect);
                return;
            }
            if ui.is_sizing_pass() || !ui.is_visible() {
                return;
            }
            if !ui.is_enabled() || !response.enabled() {
                self.mark_superseded(format!("disabled:{control_id}:{:?}", response.id));
                return;
            }
            let visible_rect = response.rect.intersect(ui.clip_rect());
            let interaction_allowed = ui
                .ctx()
                .memory(|memory| memory.allows_interaction(response.layer_id));
            if !visible_rect.is_positive() || !interaction_allowed {
                if first_ui_pass && interaction_allowed {
                    ui.scroll_to_rect_animation(
                        response.rect,
                        Some(egui::Align::Center),
                        egui::style::ScrollAnimation::none(),
                    );
                    if let Ok(mut state) = self.state.lock() {
                        state.outcome.scroll_requested = true;
                    }
                }
                return;
            }
            if already_focused {
                paint_settings_navigation_highlight(ui, visible_rect);
                self.mark_highlighted_response(ui, response, visible_rect);
                if let Ok(mut state) = self.state.lock() {
                    state.outcome.focus_requested = true;
                    state.outcome.focused_response = Some(response.id);
                    state.outcome.target_seen = true;
                }
                return;
            }
            FocusDirective::Focus(kind).apply_to_response(response);
            paint_settings_navigation_highlight(ui, visible_rect);
            self.mark_highlighted_response(ui, response, visible_rect);
            if let Ok(mut state) = self.state.lock() {
                state.outcome.focus_requested = true;
                state.outcome.focused_response = Some(response.id);
                state.outcome.target_seen = true;
            }
        } else if first_ui_pass
            && (response.clicked()
                || response.gained_focus()
                || response.is_pointer_button_down_on())
        {
            self.mark_superseded(format!("{control_id}:{:?}", response.id));
        }
    }

    fn mark_superseded(&self, response: String) {
        if let Ok(mut state) = self.state.lock() {
            state.outcome.superseded = true;
            #[cfg(test)]
            {
                state.superseding_response = Some(response);
            }
            #[cfg(not(test))]
            let _ = response;
        }
    }

    pub(crate) fn install(&self, ctx: &egui::Context) -> ScopedSettingsNavigationObserver {
        ScopedSettingsNavigationObserver::install(ctx, self.clone())
    }
}

fn paint_settings_navigation_highlight(ui: &egui::Ui, visible_rect: egui::Rect) {
    if visible_rect.is_positive() {
        ui.painter().with_clip_rect(ui.clip_rect()).rect_stroke(
            visible_rect.expand(2.0),
            4.0,
            egui::Stroke::new(2.0, egui::Color32::from_rgb(92, 173, 255)),
            egui::StrokeKind::Inside,
        );
    }
}

/// Temporarily exposes the current frame observer to existing renderers without
/// changing the Plugin trait or retaining response/geometry data.
pub(crate) struct ScopedSettingsNavigationObserver {
    ctx: egui::Context,
    previous: Option<SettingsNavigationFrameObserver>,
}

impl ScopedSettingsNavigationObserver {
    fn install(ctx: &egui::Context, observer: SettingsNavigationFrameObserver) -> Self {
        let id = egui::Id::new(SETTINGS_NAVIGATION_OBSERVER_ID);
        let previous = ctx.data(|data| data.get_temp::<SettingsNavigationFrameObserver>(id));
        ctx.data_mut(|data| data.insert_temp(id, observer));
        Self {
            ctx: ctx.clone(),
            previous,
        }
    }
}

impl Drop for ScopedSettingsNavigationObserver {
    fn drop(&mut self) {
        let id = egui::Id::new(SETTINGS_NAVIGATION_OBSERVER_ID);
        if let Some(previous) = self.previous.take() {
            self.ctx.data_mut(|data| data.insert_temp(id, previous));
        } else {
            self.ctx.data_mut(|data| {
                data.remove::<SettingsNavigationFrameObserver>(id);
            });
        }
    }
}

pub(crate) fn observe_scoped_settings_navigation_response(
    ui: &egui::Ui,
    response: &egui::Response,
    control_id: &str,
    kind: TargetKind,
) {
    let observer = ui.ctx().data(|data| {
        data.get_temp::<SettingsNavigationFrameObserver>(egui::Id::new(
            SETTINGS_NAVIGATION_OBSERVER_ID,
        ))
    });
    if let Some(observer) = observer {
        observer.observe_response(ui, response, control_id, kind);
    }
}

pub(crate) fn scoped_settings_navigation_request(
    ctx: &egui::Context,
) -> Option<PendingSettingsNavigation> {
    let observer = ctx.data(|data| {
        data.get_temp::<SettingsNavigationFrameObserver>(egui::Id::new(
            SETTINGS_NAVIGATION_OBSERVER_ID,
        ))
    });
    observer.and_then(|observer| observer.request())
}

pub(crate) fn scoped_settings_navigation_is_highlight_only(ctx: &egui::Context) -> bool {
    let observer = ctx.data(|data| {
        data.get_temp::<SettingsNavigationFrameObserver>(egui::Id::new(
            SETTINGS_NAVIGATION_OBSERVER_ID,
        ))
    });
    observer.is_some_and(|observer| observer.is_highlight_only())
}

impl TopLevelSettingsSection {
    fn egui_id(self, ui: &egui::Ui) -> egui::Id {
        ui.make_persistent_id(self.persistent_id())
    }
}

/// Optional, frame-local adapter for the native controls owned by this editor.
/// It registers only the real `Response`s returned by the existing widgets.
struct SettingsJumpTargets {
    scope: Option<JumpModeScope>,
    next_visual_order: u64,
    first_ui_pass: bool,
    activation_guarded: bool,
    navigation_observer: Option<SettingsNavigationFrameObserver>,
    #[cfg(test)]
    last_navigation_focus: Option<(String, egui::Id)>,
    #[cfg(test)]
    fuzzy_weight_response_ids: Option<(egui::Id, egui::Id)>,
}

impl SettingsJumpTargets {
    fn new(
        scope: Option<JumpModeScope>,
        first_ui_pass: bool,
        activation_guarded: bool,
        navigation_observer: Option<SettingsNavigationFrameObserver>,
    ) -> Self {
        Self {
            scope,
            next_visual_order: 0,
            first_ui_pass,
            activation_guarded,
            navigation_observer,
            #[cfg(test)]
            last_navigation_focus: None,
            #[cfg(test)]
            fuzzy_weight_response_ids: None,
        }
    }

    fn register(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        identity: impl AsRef<str>,
        label: impl Into<String>,
        kind: TargetKind,
    ) {
        let visual_order = self.next_visual_order;
        self.next_visual_order = self.next_visual_order.saturating_add(1);
        self.register_at(ui, response, identity, label, kind, visual_order);
    }

    fn reserve_order(&mut self) -> u64 {
        let visual_order = self.next_visual_order;
        self.next_visual_order = self.next_visual_order.saturating_add(1);
        visual_order
    }

    fn observe_unregistered_response(&mut self, ui: &egui::Ui, response: &egui::Response) {
        if let Some(observer) = &self.navigation_observer {
            observer.observe_response(ui, response, "<unregistered>", TargetKind::Button);
        }
    }

    fn register_at(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        identity: impl AsRef<str>,
        label: impl Into<String>,
        kind: TargetKind,
        visual_order: u64,
    ) {
        self.register_at_with_paint_label_rect(
            ui,
            response,
            identity,
            label,
            kind,
            visual_order,
            None,
        );
    }

    fn register_at_with_paint_label_rect(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        identity: impl AsRef<str>,
        label: impl Into<String>,
        kind: TargetKind,
        visual_order: u64,
        paint_label_rect: Option<egui::Rect>,
    ) {
        let identity = identity.as_ref().to_owned();
        if let Some(observer) = &self.navigation_observer {
            observer.observe_response(ui, response, &identity, kind);
            #[cfg(test)]
            if observer
                .request()
                .is_some_and(|request| request.control_id == identity)
                && observer.outcome().focus_requested
            {
                self.last_navigation_focus = Some((identity.clone(), response.id));
            }
        }
        let Some(scope) = self.scope.as_ref() else {
            return;
        };
        let label = label.into();
        let registration = match paint_label_rect {
            Some(rect) => scope.register_with_paint_label_rect(
                ui,
                response,
                &identity,
                label,
                kind,
                visual_order,
                rect,
            ),
            None => scope.register(ui, response, identity, label, kind, visual_order),
        };
        if let RegistrationResult::Registered {
            directive: Some(directive),
            ..
        } = registration
        {
            directive.apply_to_response(response);
        }
    }

    fn checkbox(
        &mut self,
        ui: &mut egui::Ui,
        value: &mut bool,
        identity: &'static str,
        label: &str,
    ) -> egui::Response {
        let response = ui.checkbox(value, label);
        self.register(ui, &response, identity, label, TargetKind::Checkbox);
        response
    }

    fn text_edit_singleline(
        &mut self,
        ui: &mut egui::Ui,
        value: &mut String,
        identity: &'static str,
        label: &str,
    ) -> egui::Response {
        let response = ui.text_edit_singleline(value);
        self.register(ui, &response, identity, label, TargetKind::TextEntry);
        response
    }

    fn response(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        identity: impl AsRef<str>,
        label: &str,
        kind: TargetKind,
    ) {
        self.register(ui, response, identity, label, kind);
    }

    fn slider_with_numeric_editor(
        &mut self,
        ui: &mut egui::Ui,
        value: &mut f32,
        range: std::ops::RangeInclusive<f32>,
        identity: &'static str,
        label: &'static str,
        numeric_identity: &'static str,
        numeric_label: &'static str,
    ) -> egui::Response {
        let slider = ui.add(
            egui::Slider::new(value, range.clone())
                .text("")
                .show_value(false),
        );
        self.response(ui, &slider, identity, label, TargetKind::Slider);

        let numeric = ui.add(egui::DragValue::new(value).range(range).speed(0.1));
        self.response(
            ui,
            &numeric,
            numeric_identity,
            numeric_label,
            TargetKind::TextEntry,
        );
        #[cfg(test)]
        if identity == setting_ids::SEARCH_FUZZY_WEIGHT {
            self.fuzzy_weight_response_ids = Some((slider.id, numeric.id));
        }
        slider | numeric
    }

    fn combo_popup_scope(&self, identity: &'static str) -> Option<JumpModeScope> {
        self.scope
            .as_ref()
            .map(|scope| scope.combo_popup_scope(identity))
    }

    fn popup_option(
        &mut self,
        popup_scope: Option<&JumpModeScope>,
        ui: &egui::Ui,
        response: &egui::Response,
        identity: &'static str,
        label: &'static str,
    ) {
        let order = self.reserve_order();
        let Some(scope) = popup_scope else {
            return;
        };
        if let RegistrationResult::Registered {
            directive: Some(directive),
            ..
        } = scope.register(ui, response, identity, label, TargetKind::Selectable, order)
        {
            directive.apply_to_response(response);
        }
    }

    fn button(
        &mut self,
        ui: &mut egui::Ui,
        identity: &'static str,
        label: &'static str,
    ) -> egui::Response {
        let response = ui.button(label);
        self.register(ui, &response, identity, label, TargetKind::Button);
        response
    }

    fn clicked(&self, response: &egui::Response) -> bool {
        response.clicked() && (!self.activation_guarded || self.first_ui_pass)
    }
}

fn show_settings_section(
    ui: &mut egui::Ui,
    section: TopLevelSettingsSection,
    forced_open: Option<bool>,
    targets: &mut SettingsJumpTargets,
    add_contents: impl FnOnce(&mut egui::Ui, &mut SettingsJumpTargets),
) -> egui::Rect {
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        section.egui_id(ui),
        section.default_open(),
    );
    if let Some(open) = forced_open {
        state.set_open(open);
    }
    let header_visual_order = targets.reserve_order();
    let mut title_rect = egui::Rect::NOTHING;
    let header = state.show_header(ui, |ui| {
        title_rect = ui.heading(section.title()).rect;
    });
    let (toggle_response, header, _) = header.body(|ui| {
        crate::gui::theme::widgets::secondary_help(ui, section.description());
        add_contents(ui, targets);
    });
    targets.register_at_with_paint_label_rect(
        ui,
        &toggle_response,
        section.persistent_id(),
        section.title(),
        TargetKind::Button,
        header_visual_order,
        Some(title_rect),
    );
    header.response.rect
}

fn set_all_settings_sections_open(ui: &egui::Ui, open: bool) {
    for section in TopLevelSettingsSection::ALL {
        let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            section.egui_id(ui),
            section.default_open(),
        );
        state.set_open(open);
        state.store(ui.ctx());
    }
}

fn settings_footer_reserved_height(ui: &egui::Ui) -> f32 {
    let spacing = ui.spacing();
    let style = ui.style();
    let font_id = style
        .override_font_id
        .clone()
        .or_else(|| {
            style
                .override_text_style
                .as_ref()
                .map(|text_style| text_style.resolve(style))
        })
        .unwrap_or_else(|| egui::TextStyle::Body.resolve(style));
    let text_height =
        ui.fonts_mut(|fonts| fonts.row_height(&font_id)) + spacing.extra_text_line_spacing;
    let button_frame = style
        .button_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Inactive,
        )
        .frame;
    let button_height =
        (text_height + button_frame.total_margin().sum().y).max(spacing.interact_size.y);
    let separator_height = style
        .separator_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Noninteractive,
        )
        .spacing;
    button_height + 2.0 * spacing.item_spacing.y + separator_height
}

const MAX_VISIBLE_SETTINGS_SEARCH_RESULTS: usize = 8;
const PENDING_SETTINGS_NAVIGATION_FRAME_BUDGET: u8 = 6;

fn reveal_request(
    section: TopLevelSettingsSection,
    default_request: Option<bool>,
    destination: Option<&SettingsSection>,
) -> Option<bool> {
    let requested = match destination {
        Some(SettingsSection::Main(target)) => *target == section,
        Some(SettingsSection::Notes | SettingsSection::Plugin { .. }) => {
            section == TopLevelSettingsSection::Plugins
        }
        _ => false,
    };
    requested.then_some(true).or(default_request)
}

fn settings_owner_label(owner: &SettingsOwner) -> String {
    match owner {
        SettingsOwner::Application => "Application".into(),
        SettingsOwner::Plugin { name, .. } => name.clone(),
        SettingsOwner::Theme => "Theme".into(),
        SettingsOwner::Mouse => "Mouse".into(),
        SettingsOwner::BrightnessDialog => "Brightness".into(),
    }
}

pub(crate) fn settings_destination_owner_open(
    app: &LauncherApp,
    section: &SettingsSection,
) -> bool {
    match section {
        SettingsSection::Theme => app.is_theme_settings_dialog_open(),
        SettingsSection::Mouse => app.is_mouse_settings_dialog_open(),
        SettingsSection::MouseGestures => app.is_mouse_gesture_settings_dialog_open(),
        SettingsSection::PluginEditor => app.show_plugins,
        SettingsSection::Brightness => app.is_brightness_dialog_open(),
        SettingsSection::Main(_) | SettingsSection::Notes | SettingsSection::Plugin { .. } => false,
    }
}

impl SettingsEditor {
    pub(crate) fn request_control_navigation(
        &mut self,
        control_id: &str,
        settings_instance: Option<u64>,
        plugins: &crate::plugin::PluginManager,
    ) {
        self.refresh_search_catalog(plugins);
        let Some(entry) = self.search_catalog.entry(control_id).cloned() else {
            return;
        };
        if !matches!(&entry.availability, SettingsAvailability::Available) {
            return;
        }
        self.queue_settings_navigation(&entry, settings_instance);
    }

    fn queue_settings_navigation(
        &mut self,
        entry: &SettingsCatalogEntry,
        settings_instance: Option<u64>,
    ) {
        self.queue_settings_navigation_with_delivery(
            entry,
            settings_instance,
            SettingsNavigationDelivery::ExactControl,
        );
    }

    fn queue_settings_navigation_with_delivery(
        &mut self,
        entry: &SettingsCatalogEntry,
        settings_instance: Option<u64>,
        delivery: SettingsNavigationDelivery,
    ) {
        self.settings_navigation_highlight = None;
        #[cfg(feature = "qa-framebuffer-capture")]
        {
            self.settings_navigation_qa_paint = None;
        }
        let stage = match (&entry.section, &entry.owner) {
            (SettingsSection::Main(_), SettingsOwner::Application) => {
                SettingsNavigationStage::MainControl
            }
            (
                SettingsSection::Main(_) | SettingsSection::Notes | SettingsSection::Plugin { .. },
                _,
            ) => SettingsNavigationStage::DeferredControl,
            _ => SettingsNavigationStage::DedicatedOwner,
        };
        let request_token = self.next_settings_navigation_token;
        self.next_settings_navigation_token = request_token.wrapping_add(1).max(1);
        self.pending_settings_navigation = Some(PendingSettingsNavigation {
            request_token,
            control_id: entry.id.as_str().to_owned(),
            owner: entry.owner.clone(),
            section: entry.section.clone(),
            catalog_revision: self.search_catalog.plugin_revision(),
            settings_instance,
            stage,
            frames_remaining: PENDING_SETTINGS_NAVIGATION_FRAME_BUDGET,
            scroll_requested: false,
            destination_observed: false,
            destination_owner_eligible: false,
            delivery,
        });
        #[cfg(test)]
        {
            self.test_last_dedicated_navigation_focus = None;
        }
        #[cfg(test)]
        self.record_search_navigation_event(format!(
            "queued control={} stage={stage:?} owner_instance={settings_instance:?}",
            entry.id.as_str()
        ));
    }

    pub(crate) fn apply_navigation_frame_outcome(
        &mut self,
        observer: &SettingsNavigationFrameObserver,
        owner_current: bool,
        now: f64,
    ) {
        let Some(request) = observer.request() else {
            return;
        };
        let outcome = observer.outcome();
        if observer.is_highlight_only() {
            let matches_highlight =
                self.settings_navigation_highlight
                    .as_ref()
                    .is_some_and(|highlight| {
                        highlight.request_token == request.request_token
                            && highlight.control_id == request.control_id
                            && highlight.owner == request.owner
                            && highlight.section == request.section
                            && highlight.catalog_revision == request.catalog_revision
                            && highlight.settings_instance == request.settings_instance
                    });
            if !matches_highlight
                || !owner_current
                || outcome.superseded
                || !outcome.target_seen
                || now
                    >= self
                        .settings_navigation_highlight
                        .as_ref()
                        .map(|highlight| highlight.expires_at)
                        .unwrap_or(0.0)
            {
                self.settings_navigation_highlight = None;
                #[cfg(feature = "qa-framebuffer-capture")]
                {
                    self.settings_navigation_qa_paint = None;
                }
            } else {
                #[cfg(feature = "qa-framebuffer-capture")]
                self.record_settings_navigation_qa_paint(&request, outcome.qa_paint);
            }
            #[cfg(test)]
            if owner_current && let Some(response_id) = outcome.highlighted_response {
                self.test_last_navigation_highlight =
                    Some((request.control_id.clone(), response_id));
            }
            return;
        }
        let Some(pending) = self.pending_settings_navigation.as_ref() else {
            return;
        };
        if !owner_current
            || pending.request_token != request.request_token
            || pending.control_id != request.control_id
            || pending.owner != request.owner
            || pending.section != request.section
            || pending.catalog_revision != request.catalog_revision
            || pending.settings_instance != request.settings_instance
            || pending.stage != request.stage
            || self.search_catalog.plugin_revision() != request.catalog_revision
        {
            return;
        }
        if outcome.focus_requested {
            self.pending_settings_navigation = None;
            self.settings_navigation_highlight =
                Some(SettingsNavigationHighlight::from_request(&request, now));
            #[cfg(feature = "qa-framebuffer-capture")]
            self.record_settings_navigation_qa_paint(&request, outcome.qa_paint);
        } else if outcome.superseded {
            self.pending_settings_navigation = None;
            self.settings_navigation_highlight = None;
            #[cfg(feature = "qa-framebuffer-capture")]
            {
                self.settings_navigation_qa_paint = None;
            }
        } else if outcome.scroll_requested
            && let Some(pending) = self.pending_settings_navigation.as_mut()
        {
            pending.scroll_requested = true;
        }
    }

    #[cfg(feature = "qa-framebuffer-capture")]
    pub(crate) fn settings_navigation_qa_paint_for_current_pass(
        &self,
        ctx: &egui::Context,
    ) -> Option<&super::state::SettingsNavigationQaPaint> {
        let paint = self.settings_navigation_qa_paint.as_ref()?;
        let highlight = self.settings_navigation_highlight.as_ref()?;
        let now = ctx.input(|input| input.time);
        (paint.egui_frame_nr == ctx.cumulative_frame_nr()
            && paint.egui_pass_index == ctx.current_pass_index()
            && paint.request_token == highlight.request_token
            && paint.control_id == highlight.control_id
            && paint.owner == highlight.owner
            && paint.owner_instance == highlight.settings_instance
            && paint.catalog_revision == highlight.catalog_revision
            && now < highlight.expires_at)
            .then_some(paint)
    }

    #[cfg(feature = "qa-framebuffer-capture")]
    fn record_settings_navigation_qa_paint(
        &mut self,
        request: &PendingSettingsNavigation,
        painted: Option<SettingsNavigationPaintedResponse>,
    ) {
        let Some(painted) = painted else {
            return;
        };
        self.settings_navigation_qa_paint = Some(super::state::SettingsNavigationQaPaint {
            request_token: request.request_token,
            control_id: request.control_id.clone(),
            owner: request.owner.clone(),
            owner_instance: request.settings_instance,
            catalog_revision: request.catalog_revision,
            response_id: painted.response_id,
            rect: painted.rect,
            egui_frame_nr: painted.egui_frame_nr,
            egui_pass_index: painted.egui_pass_index,
        });
    }

    pub(crate) fn dedicated_navigation_observer(
        &mut self,
        section: &SettingsSection,
        owner_open: bool,
        owner_current: bool,
        protected_takeover: bool,
        first_ui_pass: bool,
    ) -> Option<SettingsNavigationFrameObserver> {
        if self.pending_settings_navigation.is_none() {
            let highlight = self.settings_navigation_highlight.as_ref()?.clone();
            if &highlight.section != section {
                return None;
            }
            let current_entry = self.search_catalog.entry(&highlight.control_id);
            if highlight.catalog_revision != self.search_catalog.plugin_revision()
                || !current_entry.is_some_and(|entry| {
                    entry.owner == highlight.owner
                        && entry.section == highlight.section
                        && !matches!(entry.availability, SettingsAvailability::Unavailable { .. })
                })
            {
                self.settings_navigation_highlight = None;
                return None;
            }
            if protected_takeover || !owner_open || !owner_current {
                self.settings_navigation_highlight = None;
                return None;
            }
            return Some(SettingsNavigationFrameObserver::new(
                highlight.as_request(),
                first_ui_pass,
                true,
                SettingsNavigationObservationMode::Highlight,
            ));
        }
        let request = self.pending_settings_navigation.as_ref()?;
        if request.stage != SettingsNavigationStage::DedicatedOwner
            || &request.section != section
            || request.catalog_revision != self.search_catalog.plugin_revision()
            || !self
                .search_catalog
                .entry(&request.control_id)
                .is_some_and(|entry| {
                    entry.owner == request.owner
                        && entry.section == request.section
                        && !matches!(entry.availability, SettingsAvailability::Unavailable { .. })
                })
        {
            return None;
        }

        if protected_takeover {
            #[cfg(test)]
            let control_id = self
                .pending_settings_navigation
                .as_ref()
                .map(|request| request.control_id.clone())
                .unwrap_or_default();
            self.pending_settings_navigation = None;
            #[cfg(test)]
            self.record_search_navigation_event(format!(
                "dedicated owner retired control={control_id} reason=protected-takeover"
            ));
            return None;
        }

        if owner_open && let Some(request) = self.pending_settings_navigation.as_mut() {
            request.destination_observed = true;
        }

        if owner_current {
            if let Some(request) = self.pending_settings_navigation.as_mut() {
                request.destination_owner_eligible = true;
            }
        } else {
            let Some((destination_owner_eligible, destination_observed, frames_remaining)) =
                self.pending_settings_navigation.as_ref().map(|request| {
                    (
                        request.destination_owner_eligible,
                        request.destination_observed,
                        request.frames_remaining,
                    )
                })
            else {
                return None;
            };
            #[cfg(test)]
            let control_id = self
                .pending_settings_navigation
                .as_ref()
                .map(|request| request.control_id.clone())
                .unwrap_or_default();
            if destination_owner_eligible {
                // This exact destination was already eligible and has since
                // lost native ownership. A sizing pass before first
                // eligibility is handled by the bounded readiness budget
                // below instead.
                self.pending_settings_navigation = None;
                #[cfg(test)]
                self.record_search_navigation_event(format!(
                    "dedicated owner retired control={control_id} reason=owner-lost-after-eligibility"
                ));
                return None;
            }
            #[cfg(test)]
            if destination_observed {
                self.record_search_navigation_event(format!(
                    "dedicated owner waiting control={} eligible=false first={first_ui_pass} remaining={}",
                    control_id, frames_remaining
                ));
            }
            if !owner_open && destination_observed {
                self.pending_settings_navigation = None;
                #[cfg(test)]
                self.record_search_navigation_event(format!(
                    "dedicated owner retired control={control_id} reason=destination-closed-before-eligibility"
                ));
                return None;
            }
            if owner_open && first_ui_pass {
                let remaining = frames_remaining.saturating_sub(1);
                if let Some(request) = self.pending_settings_navigation.as_mut() {
                    request.frames_remaining = remaining;
                    if remaining == 0 {
                        self.pending_settings_navigation = None;
                        #[cfg(test)]
                        self.record_search_navigation_event(format!(
                            "dedicated owner retired control={control_id} reason=readiness-timeout"
                        ));
                        return None;
                    }
                }
            }
            return None;
        }

        let request = self.pending_settings_navigation.as_ref()?;
        Some(SettingsNavigationFrameObserver::new(
            request.clone(),
            first_ui_pass,
            true,
            SettingsNavigationObservationMode::Focus,
        ))
    }

    pub(crate) fn finish_dedicated_navigation_frame(
        &mut self,
        observer: &SettingsNavigationFrameObserver,
        owner_current: bool,
        first_ui_pass: bool,
        now: f64,
    ) {
        let Some(request) = observer.request() else {
            return;
        };
        #[cfg(test)]
        let outcome = observer.outcome();
        #[cfg(test)]
        {
            let owner_was_current = observer.test_owner_was_current();
            let (superseding_response, response_observations) = observer.test_details();
            self.record_search_navigation_event(format!(
                "dedicated frame token={} control={} stage={:?} remaining={} first={} \
                 owner_before={} owner_after={} focus_requested={} scroll_requested={} \
                 superseded={} superseding_response={superseding_response:?} \
                 focused_response={:?} observations={response_observations:?}",
                request.request_token,
                request.control_id,
                request.stage,
                request.frames_remaining,
                first_ui_pass,
                owner_was_current,
                owner_current,
                outcome.focus_requested,
                outcome.scroll_requested,
                outcome.superseded,
                outcome.focused_response,
            ));
        }
        #[cfg(test)]
        if owner_current
            && outcome.focus_requested
            && let Some(response_id) = outcome.focused_response
        {
            self.test_last_dedicated_navigation_focus =
                Some((request.control_id.clone(), response_id));
        }
        self.apply_navigation_frame_outcome(observer, owner_current, now);
        let Some(pending) = self.pending_settings_navigation.as_mut() else {
            return;
        };
        if pending.request_token != request.request_token
            || pending.stage != SettingsNavigationStage::DedicatedOwner
        {
            return;
        }
        if !owner_current {
            self.pending_settings_navigation = None;
            return;
        }
        if first_ui_pass {
            pending.frames_remaining = pending.frames_remaining.saturating_sub(1);
            if pending.frames_remaining == 0 {
                self.pending_settings_navigation = None;
            }
        }
    }

    #[cfg(test)]
    fn record_search_navigation_event(&mut self, event: String) {
        // A discarded Enter pass and the bounded eight-frame focus handoff can
        // emit up to six events per pass. Keep the full first-event trace rather
        // than evicting the queue or retirement cause at the front.
        const MAX_HISTORY: usize = 64;
        if self.test_search_navigation_history.len() < MAX_HISTORY {
            self.test_search_navigation_history.push(event);
        }
    }

    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        app: &mut LauncherApp,
        jump_mode_scope: Option<JumpModeScope>,
        settings_owner_instance: Option<u64>,
        jump_mode_active: bool,
        first_ui_pass: bool,
    ) {
        let now = ctx.input(|input| input.time);
        self.expire_settings_navigation_highlight(now);
        if let Some(highlight) = self.settings_navigation_highlight.as_ref() {
            ctx.request_repaint_after(SETTINGS_NAVIGATION_HIGHLIGHT_DURATION.min(
                std::time::Duration::from_secs_f64((highlight.expires_at - now).max(0.0)),
            ));
        }
        #[cfg(test)]
        let test_pending_before = self.pending_settings_navigation.as_ref().map(|pending| {
            format!(
                "{}:{:?}:remaining={}",
                pending.control_id, pending.stage, pending.frames_remaining
            )
        });
        #[cfg(test)]
        let test_raw_enter_before_frame = ctx.input(|input| input.key_pressed(egui::Key::Enter));
        #[cfg(test)]
        if first_ui_pass
            && test_raw_enter_before_frame
            && settings_owner_instance.is_some()
            && !self.test_search_navigation_trace_started
        {
            self.test_search_navigation_history.clear();
            self.test_search_navigation_trace_started = true;
        }
        #[cfg(test)]
        {
            self.test_last_navigation_diagnostic = None;
            self.test_last_navigation_highlight = None;
            self.test_notes_external_open_response = None;
        }
        if first_ui_pass {
            self.search_enter_consumption_pending = false;
        } else if self.search_enter_consumption_pending {
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        }
        #[cfg(test)]
        if first_ui_pass {
            self.test_search_query_response = None;
            self.test_search_result_responses.clear();
            self.test_last_navigation_focus = None;
        }

        let old_revision = self.search_catalog.plugin_revision();
        self.refresh_search_catalog(&app.plugins);
        let catalog_revision = self.search_catalog.plugin_revision();
        if old_revision != catalog_revision {
            self.settings_search_match_revision = None;
            self.settings_search_match_ids.clear();
            self.settings_search_selected_id = None;
            self.settings_search_page = 0;
            if self
                .pending_settings_navigation
                .as_ref()
                .is_some_and(|pending| pending.catalog_revision != catalog_revision)
            {
                self.pending_settings_navigation = None;
            }
            self.settings_navigation_highlight = None;
        }
        self.refresh_settings_search_matches();
        self.validate_pending_settings_navigation(app, settings_owner_instance, first_ui_pass);
        let main_or_notes_highlight_lost_owner = self
            .settings_navigation_highlight
            .as_ref()
            .is_some_and(|highlight| {
                matches!(
                    &highlight.section,
                    SettingsSection::Main(_) | SettingsSection::Notes
                ) && (settings_owner_instance != highlight.settings_instance
                    || !app.settings_native_ui_owner_current(ctx, true))
            });
        if main_or_notes_highlight_lost_owner {
            self.settings_navigation_highlight = None;
            #[cfg(feature = "qa-framebuffer-capture")]
            {
                self.settings_navigation_qa_paint = None;
            }
        }
        let navigation_observer = if let Some(pending) =
            self.pending_settings_navigation.as_ref().filter(|pending| {
                pending.stage != SettingsNavigationStage::DedicatedOwner
                    && settings_owner_instance.is_some()
                    && settings_owner_instance == pending.settings_instance
                    && pending.catalog_revision == catalog_revision
            }) {
            Some(SettingsNavigationFrameObserver::new(
                pending.clone(),
                first_ui_pass,
                true,
                SettingsNavigationObservationMode::Focus,
            ))
        } else {
            self.settings_navigation_highlight
                .as_ref()
                .filter(|highlight| {
                    highlight.settings_instance.is_some()
                        && settings_owner_instance == highlight.settings_instance
                        && highlight.catalog_revision == catalog_revision
                        && highlight.section != SettingsSection::PluginEditor
                        && !matches!(
                            highlight.section,
                            SettingsSection::Theme
                                | SettingsSection::Mouse
                                | SettingsSection::MouseGestures
                                | SettingsSection::Brightness
                        )
                })
                .map(|highlight| {
                    SettingsNavigationFrameObserver::new(
                        highlight.as_request(),
                        first_ui_pass,
                        true,
                        SettingsNavigationObservationMode::Highlight,
                    )
                })
        };
        let mut jump_targets = SettingsJumpTargets::new(
            jump_mode_scope,
            first_ui_pass,
            app.jump_mode_enabled(),
            navigation_observer,
        );
        let mut open = app.show_settings;
        let mut cancel = false;
        #[cfg(test)]
        let mut test_window_state = None;
        let mut reveal_section = self
            .pending_settings_navigation
            .as_ref()
            .filter(|pending| {
                settings_owner_instance == pending.settings_instance
                    && pending.stage != SettingsNavigationStage::DedicatedOwner
            })
            .map(|pending| pending.section.clone());
        egui::Window::new("Settings")
            .resizable(true)
            .default_size(Self::settings_window_default_size(ctx))
            .min_height(Self::SETTINGS_WINDOW_MIN_HEIGHT)
            .open(&mut open)
            .show(ctx, |ui| {
                #[cfg(test)]
                {
                    test_window_state =
                        Some((ui.is_sizing_pass(), ui.is_visible(), ui.is_enabled()));
                }
                let settings_content_height = Self::settings_content_height(
                    ui.available_height(),
                    settings_footer_reserved_height(ui),
                );
                egui::ScrollArea::vertical()
                    .min_scrolled_height(0.0)
                    .max_height(settings_content_height)
                    .show(ui, |ui| {
                        let activated_result = self.render_settings_search(
                            ctx,
                            ui,
                            app,
                            settings_owner_instance,
                            jump_mode_active,
                            first_ui_pass,
                            &mut jump_targets,
                        );
                        if let Some(control_id) = activated_result
                            && let Some(entry) = self.search_catalog.entry(&control_id).cloned()
                        {
                            reveal_section = Some(entry.section.clone());
                            self.activate_settings_search_result(
                                app,
                                settings_owner_instance,
                                entry,
                            );
                        }
                        if let Some(observer) = jump_targets.navigation_observer.as_ref()
                            && !self.settings_navigation_observer_is_authoritative(observer)
                        {
                            observer.invalidate();
                        }

                        let top_level_expand_request = ui
                            .horizontal(|ui| {
                                let mut request = None;
                                let expand = jump_targets.button(
                                    ui,
                                    setting_ids::SECTIONS_EXPAND_ALL,
                                    "Expand all",
                                );
                                if jump_targets.clicked(&expand) {
                                    request = Some(true);
                                }
                                let collapse = jump_targets.button(
                                    ui,
                                    setting_ids::SECTIONS_COLLAPSE_ALL,
                                    "Collapse all",
                                );
                                if jump_targets.clicked(&collapse) {
                                    request = Some(false);
                                }
                                request
                            })
                            .inner;
                        if let Some(open) = top_level_expand_request {
                            set_all_settings_sections_open(ui, open);
                        }

                        show_settings_section(
                            ui,
                            TopLevelSettingsSection::Hotkeys,
                            reveal_request(
                                TopLevelSettingsSection::Hotkeys,
                                top_level_expand_request,
                                reveal_section.as_ref(),
                            ),
                            &mut jump_targets,
                            |ui, targets| self.render_hotkey_section(ui, app, targets),
                        );
                        show_settings_section(
                            ui,
                            TopLevelSettingsSection::PasteSequences,
                            reveal_request(
                                TopLevelSettingsSection::PasteSequences,
                                top_level_expand_request,
                                reveal_section.as_ref(),
                            ),
                            &mut jump_targets,
                            |ui, targets| {
                                ui.label(app.paste_sequence_shortcut_summary());
                                ui.separator();
                                self.render_paste_sequence_section(ui, targets);
                            },
                        );
                        show_settings_section(
                            ui,
                            TopLevelSettingsSection::LauncherWindow,
                            reveal_request(
                                TopLevelSettingsSection::LauncherWindow,
                                top_level_expand_request,
                                reveal_section.as_ref(),
                            ),
                            &mut jump_targets,
                            |ui, targets| self.render_launcher_window_section(ui, app, targets),
                        );
                        show_settings_section(
                            ui,
                            TopLevelSettingsSection::SearchResults,
                            reveal_request(
                                TopLevelSettingsSection::SearchResults,
                                top_level_expand_request,
                                reveal_section.as_ref(),
                            ),
                            &mut jump_targets,
                            |ui, targets| self.render_search_results_section(ui, targets),
                        );
                        show_settings_section(
                            ui,
                            TopLevelSettingsSection::ActionsFeedback,
                            reveal_request(
                                TopLevelSettingsSection::ActionsFeedback,
                                top_level_expand_request,
                                reveal_section.as_ref(),
                            ),
                            &mut jump_targets,
                            |ui, targets| self.render_actions_safety_feedback_section(ui, targets),
                        );
                        show_settings_section(
                            ui,
                            TopLevelSettingsSection::Dashboard,
                            reveal_request(
                                TopLevelSettingsSection::Dashboard,
                                top_level_expand_request,
                                reveal_section.as_ref(),
                            ),
                            &mut jump_targets,
                            |ui, targets| self.render_dashboard_section(ui, app, targets),
                        );
                        show_settings_section(
                            ui,
                            TopLevelSettingsSection::Radial,
                            reveal_request(
                                TopLevelSettingsSection::Radial,
                                top_level_expand_request,
                                reveal_section.as_ref(),
                            ),
                            &mut jump_targets,
                            |ui, targets| self.render_radial_section(ui, app, targets),
                        );
                        show_settings_section(
                            ui,
                            TopLevelSettingsSection::Plugins,
                            reveal_request(
                                TopLevelSettingsSection::Plugins,
                                top_level_expand_request,
                                reveal_section.as_ref(),
                            ),
                            &mut jump_targets,
                            |ui, targets| {
                                self.render_plugin_sections(
                                    ui,
                                    app,
                                    targets,
                                    reveal_section.as_ref(),
                                )
                            },
                        );
                        self.expand_request = None;
                    });

                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    let pending = app.settings_save_pending();
                    let save = ui.add_enabled(!pending, egui::Button::new("Save"));
                    let save = crate::gui::theme::widgets::with_button_emphasis(
                        ui,
                        save,
                        crate::gui::theme::widgets::ButtonEmphasis::Primary,
                    );
                    jump_targets.response(
                        ui,
                        &save,
                        setting_ids::FOOTER_SAVE,
                        "Save",
                        TargetKind::Button,
                    );
                    if jump_targets.clicked(&save) {
                        self.save_settings(ctx, app);
                    }
                    let cancel_button = ui.add_enabled(!pending, egui::Button::new("Cancel"));
                    jump_targets.register(
                        ui,
                        &cancel_button,
                        setting_ids::FOOTER_CANCEL,
                        "Cancel",
                        TargetKind::Button,
                    );
                    if jump_targets.clicked(&cancel_button) {
                        cancel = true;
                    }
                    if app.settings_save_reconciliation_required() {
                        ui.label("Hotkey state needs reconciliation; saving is disabled.");
                    } else if pending {
                        ui.label("Saving settings…");
                    }
                });
            });
        #[cfg(test)]
        let navigation_outcome = jump_targets
            .navigation_observer
            .as_ref()
            .map(SettingsNavigationFrameObserver::outcome)
            .unwrap_or_default();
        #[cfg(test)]
        let navigation_request = jump_targets
            .navigation_observer
            .as_ref()
            .and_then(SettingsNavigationFrameObserver::request);
        #[cfg(test)]
        let test_main_control_before_cleanup = self
            .pending_settings_navigation
            .as_ref()
            .filter(|pending| pending.stage != SettingsNavigationStage::DedicatedOwner)
            .map(|pending| pending.control_id.clone());
        if let Some(observer) = jump_targets.navigation_observer.as_ref() {
            let owner_current = settings_owner_instance.is_some()
                && app.settings_native_ui_owner_current(ctx, true);
            let observer_request = observer.request();
            let observer_is_authoritative =
                self.settings_navigation_observer_is_authoritative(observer);
            #[cfg(test)]
            if owner_current
                && observer_is_authoritative
                && navigation_outcome.focus_requested
                && let Some(request) = navigation_request.as_ref()
                && request.stage != SettingsNavigationStage::DedicatedOwner
                && self
                    .pending_settings_navigation
                    .as_ref()
                    .is_some_and(|pending| pending == request)
                && let Some(response_id) = navigation_outcome.focused_response
            {
                self.test_last_navigation_focus = Some((request.control_id.clone(), response_id));
            }
            if observer_is_authoritative {
                self.apply_navigation_frame_outcome(observer, owner_current, now);
                if !observer.is_highlight_only()
                    && let Some(pending) = self.pending_settings_navigation.as_mut()
                    && observer_request
                        .as_ref()
                        .is_some_and(|request| pending.request_token == request.request_token)
                    && pending.stage != SettingsNavigationStage::DedicatedOwner
                {
                    if first_ui_pass {
                        pending.frames_remaining = pending.frames_remaining.saturating_sub(1);
                    }
                    if pending.frames_remaining == 0 {
                        self.pending_settings_navigation = None;
                    }
                }
            }
        }
        #[cfg(test)]
        if let Some(control_id) = test_main_control_before_cleanup
            && self.pending_settings_navigation.is_none()
        {
            let reason = if navigation_outcome.focus_requested {
                "destination-focus-requested"
            } else if navigation_outcome.superseded {
                "superseded-by-response"
            } else {
                "timeout"
            };
            self.record_search_navigation_event(format!(
                "main-control retired control={control_id} reason={reason} \
                 superseding_response={:?} target_responses={:?}",
                jump_targets
                    .navigation_observer
                    .as_ref()
                    .and_then(|observer| observer.test_details().0),
                jump_targets
                    .navigation_observer
                    .as_ref()
                    .map(|observer| observer.test_details().1)
                    .unwrap_or_default(),
            ));
        }
        #[cfg(test)]
        if let Some(focus) = jump_targets.last_navigation_focus.take() {
            self.test_last_navigation_focus = Some(focus);
        }
        #[cfg(test)]
        {
            self.test_settings_window_state = test_window_state;
            self.test_fuzzy_weight_response_ids = jump_targets.fuzzy_weight_response_ids;
            let pending_after = self.pending_settings_navigation.as_ref().map(|pending| {
                format!(
                    "{}:{:?}:remaining={}",
                    pending.control_id, pending.stage, pending.frames_remaining
                )
            });
            let rendered_result_ids = self
                .test_search_result_responses
                .iter()
                .map(|(id, _, _)| id.as_str())
                .collect::<Vec<_>>();
            self.test_last_navigation_diagnostic = Some(format!(
                "owner={settings_owner_instance:?} first={first_ui_pass} window={test_window_state:?} \
                 response_owner={:?} query_nonempty={} matches={} page={} selected={:?} \
                 rendered_results={rendered_result_ids:?} pending_before={test_pending_before:?} \
                 pending_after={pending_after:?} focused={:?} focus_requested={} scroll_requested={} superseded={} target_responses={:?}",
                self.settings_search_response_owner_instance,
                !self.settings_search_query.trim().is_empty(),
                self.settings_search_match_ids.len(),
                self.settings_search_page,
                self.settings_search_selected_id,
                ctx.memory(|memory| memory.focused()),
                navigation_outcome.focus_requested,
                navigation_outcome.scroll_requested,
                navigation_outcome.superseded,
                jump_targets
                    .navigation_observer
                    .as_ref()
                    .map(|observer| observer.test_details().1)
                    .unwrap_or_default(),
            ));
            if test_raw_enter_before_frame
                || test_pending_before.is_some()
                || pending_after.is_some()
                || navigation_outcome.superseded
            {
                self.record_search_navigation_event(format!(
                    "frame owner={settings_owner_instance:?} first={first_ui_pass} \
                     raw_enter_before={test_raw_enter_before_frame} \
                     raw_enter_after={:?} window={test_window_state:?} \
                     pending_before={test_pending_before:?} pending_after={pending_after:?} \
                     focus_requested={} scroll_requested={} superseded={} by={:?} \
                     target_responses={:?} focused={:?}",
                    ctx.input(|input| input.key_pressed(egui::Key::Enter)),
                    navigation_outcome.focus_requested,
                    navigation_outcome.scroll_requested,
                    navigation_outcome.superseded,
                    jump_targets
                        .navigation_observer
                        .as_ref()
                        .and_then(|observer| observer.test_details().0),
                    jump_targets
                        .navigation_observer
                        .as_ref()
                        .map(|observer| observer.test_details().1)
                        .unwrap_or_default(),
                    ctx.memory(|memory| memory.focused()),
                ));
            }
        }
        if cancel {
            *self = Self::new_with_plugins(&self.baseline_settings);
            open = false;
        }
        if !open {
            self.pending_settings_navigation = None;
            self.settings_navigation_highlight = None;
            self.settings_search_query.clear();
            self.settings_search_match_query.clear();
            self.settings_search_match_revision = None;
            self.settings_search_match_ids.clear();
            self.settings_search_selected_id = None;
            self.settings_search_page = 0;
            self.settings_search_response_owner_instance = None;
            self.settings_search_query_response_id = None;
            self.settings_search_result_response_ids.clear();
        }
        app.show_settings = open;
    }

    pub(crate) fn handle_search_escape_before_container_close(
        &mut self,
        ctx: &egui::Context,
        settings_owner_instance: u64,
    ) -> bool {
        let current_focus = ctx.memory(|memory| memory.focused());
        let search_owns_focus = self.settings_search_response_owner_instance
            == Some(settings_owner_instance)
            && match current_focus {
                Some(focused_id) => {
                    self.settings_search_query_response_id == Some(focused_id)
                        || self
                            .settings_search_result_response_ids
                            .contains(&focused_id)
                }
                None => ctx.memory(|memory| {
                    self.settings_search_query_response_id
                        .is_some_and(|id| memory.had_focus_last_frame(id))
                        || self
                            .settings_search_result_response_ids
                            .iter()
                            .any(|id| memory.had_focus_last_frame(*id))
                }),
            };
        if self.settings_search_query.trim().is_empty() || !search_owns_focus {
            return false;
        }

        self.settings_search_query.clear();
        self.settings_search_match_query.clear();
        self.settings_search_match_revision = None;
        self.settings_search_match_ids.clear();
        self.settings_search_selected_id = None;
        self.settings_search_page = 0;
        self.pending_settings_navigation = None;
        self.settings_navigation_highlight = None;
        ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        true
    }

    fn refresh_settings_search_matches(&mut self) {
        let revision = self.search_catalog.plugin_revision();
        if self.settings_search_match_query == self.settings_search_query
            && self.settings_search_match_revision == Some(revision)
        {
            return;
        }
        self.settings_search_match_ids = self
            .search_catalog
            .search(&self.settings_search_query)
            .into_iter()
            .map(|entry| entry.id.as_str().to_owned())
            .collect();
        self.settings_search_match_query = self.settings_search_query.clone();
        self.settings_search_match_revision = Some(revision);
        self.settings_search_page = 0;
        if !self
            .settings_search_selected_id
            .as_ref()
            .is_some_and(|id| self.settings_search_match_ids.contains(id))
        {
            self.settings_search_selected_id = self
                .settings_search_match_ids
                .iter()
                .find(|id| {
                    self.search_catalog.entry(id).is_some_and(|entry| {
                        !matches!(entry.availability, SettingsAvailability::Unavailable { .. })
                    })
                })
                .cloned();
        }
        if self.settings_search_match_ids.is_empty() {
            self.settings_search_selected_id = None;
        }
    }

    fn settings_navigation_observer_is_authoritative(
        &self,
        observer: &SettingsNavigationFrameObserver,
    ) -> bool {
        let Some(request) = observer.request() else {
            return false;
        };
        if self.search_catalog.plugin_revision() != request.catalog_revision {
            return false;
        }
        if observer.is_highlight_only() {
            self.settings_navigation_highlight
                .as_ref()
                .is_some_and(|highlight| {
                    highlight.request_token == request.request_token
                        && highlight.control_id == request.control_id
                        && highlight.owner == request.owner
                        && highlight.section == request.section
                        && highlight.catalog_revision == request.catalog_revision
                        && highlight.settings_instance == request.settings_instance
                        && highlight.delivery == request.delivery
                })
        } else {
            self.pending_settings_navigation
                .as_ref()
                .is_some_and(|pending| pending == &request)
        }
    }

    fn validate_pending_settings_navigation(
        &mut self,
        app: &LauncherApp,
        settings_owner_instance: Option<u64>,
        first_ui_pass: bool,
    ) {
        let Some(pending) = self.pending_settings_navigation.as_mut() else {
            return;
        };
        #[cfg(test)]
        let test_pending_identity = pending.control_id.clone();
        #[cfg(test)]
        let test_pending_stage = pending.stage;
        #[cfg(test)]
        let test_pending_owner_instance = pending.settings_instance;
        if pending.stage != SettingsNavigationStage::DedicatedOwner {
            match (pending.settings_instance, settings_owner_instance) {
                (None, Some(instance)) => pending.settings_instance = Some(instance),
                (Some(expected), Some(actual)) if expected == actual => {}
                (None, None) => {
                    if first_ui_pass {
                        pending.frames_remaining = pending.frames_remaining.saturating_sub(1);
                    }
                    if pending.frames_remaining == 0 {
                        self.pending_settings_navigation = None;
                        #[cfg(test)]
                        self.record_search_navigation_event(format!(
                            "validation retired control={test_pending_identity} stage={test_pending_stage:?} reason=owner-never-observed owner_instance={settings_owner_instance:?}"
                        ));
                    }
                    return;
                }
                _ => {
                    self.pending_settings_navigation = None;
                    #[cfg(test)]
                    self.record_search_navigation_event(format!(
                        "validation retired control={test_pending_identity} stage={test_pending_stage:?} reason=owner-instance-mismatch expected={test_pending_owner_instance:?} actual={settings_owner_instance:?}"
                    ));
                    return;
                }
            }
        }
        let current_entry = self.search_catalog.entry(&pending.control_id);
        let entry_is_current = current_entry.is_some_and(|entry| {
            entry.owner == pending.owner
                && entry.section == pending.section
                && !matches!(
                    &entry.availability,
                    SettingsAvailability::Unavailable { .. }
                )
        });
        if pending.catalog_revision != self.search_catalog.plugin_revision() || !entry_is_current {
            #[cfg(test)]
            let reason = if pending.catalog_revision != self.search_catalog.plugin_revision() {
                "catalog-revision-mismatch"
            } else {
                "entry-owner-section-or-availability-invalid"
            };
            self.pending_settings_navigation = None;
            #[cfg(test)]
            self.record_search_navigation_event(format!(
                "validation retired control={test_pending_identity} stage={test_pending_stage:?} reason={reason}"
            ));
            return;
        }
        match pending.stage {
            SettingsNavigationStage::MainControl => {
                debug_assert_eq!(settings_owner_instance, pending.settings_instance);
            }
            SettingsNavigationStage::DeferredControl => {
                debug_assert_eq!(settings_owner_instance, pending.settings_instance);
                if first_ui_pass {
                    pending.frames_remaining = pending.frames_remaining.saturating_sub(1);
                }
                if pending.frames_remaining == 0 {
                    self.pending_settings_navigation = None;
                    #[cfg(test)]
                    self.record_search_navigation_event(format!(
                        "validation retired control={test_pending_identity} stage=DeferredControl reason=timeout"
                    ));
                }
            }
            SettingsNavigationStage::DedicatedOwner => {
                let owner_open = settings_destination_owner_open(app, &pending.section);
                if owner_open {
                    pending.destination_observed = true;
                } else if pending.destination_observed {
                    self.pending_settings_navigation = None;
                    #[cfg(test)]
                    self.record_search_navigation_event(format!(
                        "validation retired control={test_pending_identity} stage=DedicatedOwner reason=destination-closed"
                    ));
                } else if first_ui_pass {
                    pending.frames_remaining = pending.frames_remaining.saturating_sub(1);
                    if pending.frames_remaining == 0 {
                        self.pending_settings_navigation = None;
                        #[cfg(test)]
                        self.record_search_navigation_event(format!(
                            "validation retired control={test_pending_identity} stage=DedicatedOwner reason=timeout"
                        ));
                    }
                }
            }
        }
    }

    fn render_settings_search(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        app: &LauncherApp,
        settings_owner_instance: Option<u64>,
        jump_mode_active: bool,
        first_ui_pass: bool,
        targets: &mut SettingsJumpTargets,
    ) -> Option<String> {
        #[cfg(test)]
        let raw_enter_before_query = ctx.input(|input| input.key_pressed(egui::Key::Enter));
        #[cfg(test)]
        let focus_before_query = ctx.memory(|memory| memory.focused());
        #[cfg(test)]
        let pending_before_query = self.pending_settings_navigation.as_ref().map(|pending| {
            format!(
                "{}:{:?}:remaining={}",
                pending.control_id, pending.stage, pending.frames_remaining
            )
        });
        self.settings_search_response_owner_instance = settings_owner_instance;
        self.settings_search_query_response_id = None;
        self.settings_search_result_response_ids.clear();
        #[cfg(test)]
        {
            self.test_search_page_responses.clear();
        }
        ui.label(egui::RichText::new("Search settings").strong());
        let query = ui.add(
            egui::TextEdit::singleline(&mut self.settings_search_query)
                .return_key(None)
                .hint_text("Search settings…")
                .desired_width(f32::INFINITY),
        );
        ui.small(
            "Enter focuses or opens the selected setting without changing it. With search focused, Escape clears it. Unavailable results stay disabled and are never enabled automatically.",
        );
        targets.response(
            ui,
            &query,
            "settings.search.query",
            "Search settings",
            TargetKind::TextEntry,
        );
        self.settings_search_query_response_id = Some(query.id);
        #[cfg(test)]
        {
            self.test_search_query_response = Some((query.id, query.rect));
            self.test_search_query_clip_rect = Some(ui.clip_rect());
        }
        let query_changed = query.changed();
        #[cfg(test)]
        let raw_enter_after_query = ctx.input(|input| input.key_pressed(egui::Key::Enter));
        if query_changed {
            self.pending_settings_navigation = None;
            self.settings_navigation_highlight = None;
            self.refresh_settings_search_matches();
            #[cfg(test)]
            self.record_search_navigation_event(format!(
                "query_changed while pending={pending_before_query:?}"
            ));
        }

        let query_has_text = !self.settings_search_query.trim().is_empty();
        let mut activated_result = None;
        let mut result_has_focus = false;
        if query_has_text {
            ui.add_space(3.0);
            let total = self.settings_search_match_ids.len();
            let page_count = total.div_ceil(MAX_VISIBLE_SETTINGS_SEARCH_RESULTS);
            self.settings_search_page = self.settings_search_page.min(page_count.saturating_sub(1));
            let mut page_changed = false;
            if page_count > 1 {
                ui.horizontal(|ui| {
                    let previous = ui.add_enabled(
                        self.settings_search_page > 0,
                        egui::Button::new("Previous results"),
                    );
                    targets.observe_unregistered_response(ui, &previous);
                    self.settings_search_result_response_ids.push(previous.id);
                    #[cfg(test)]
                    self.test_search_page_responses.push((
                        "previous".into(),
                        previous.id,
                        previous.rect,
                    ));
                    if previous.clicked() && first_ui_pass {
                        self.settings_search_page = self.settings_search_page.saturating_sub(1);
                        page_changed = true;
                    }
                    ui.label(format!(
                        "Page {} of {page_count}",
                        self.settings_search_page + 1
                    ));
                    let next = ui.add_enabled(
                        self.settings_search_page + 1 < page_count,
                        egui::Button::new("Next results"),
                    );
                    targets.observe_unregistered_response(ui, &next);
                    self.settings_search_result_response_ids.push(next.id);
                    #[cfg(test)]
                    self.test_search_page_responses
                        .push(("next".into(), next.id, next.rect));
                    if next.clicked() && first_ui_pass {
                        self.settings_search_page =
                            (self.settings_search_page + 1).min(page_count - 1);
                        page_changed = true;
                    }
                });
            }
            let page_start = self
                .settings_search_page
                .saturating_mul(MAX_VISIBLE_SETTINGS_SEARCH_RESULTS);
            let visible_ids = self
                .settings_search_match_ids
                .iter()
                .skip(page_start)
                .take(MAX_VISIBLE_SETTINGS_SEARCH_RESULTS)
                .cloned()
                .collect::<Vec<_>>();
            if page_changed
                || !self
                    .settings_search_selected_id
                    .as_ref()
                    .is_some_and(|selected| visible_ids.contains(selected))
            {
                self.settings_search_selected_id = visible_ids
                    .iter()
                    .find(|id| {
                        self.search_catalog.entry(id).is_some_and(|entry| {
                            !matches!(
                                &entry.availability,
                                SettingsAvailability::Unavailable { .. }
                            )
                        })
                    })
                    .or_else(|| visible_ids.first())
                    .cloned();
            }
            if visible_ids.is_empty() {
                ui.small("No matching settings.");
            } else {
                for control_id in visible_ids {
                    let Some(entry) = self.search_catalog.entry(&control_id).cloned() else {
                        continue;
                    };
                    let (availability_text, unavailable) =
                        self.search_result_availability(&entry, app);
                    let selected =
                        self.settings_search_selected_id.as_deref() == Some(control_id.as_str());
                    let identity = format!("settings.search.result/{control_id}");
                    ui.horizontal_top(|ui| {
                        let response = ui.add_enabled(
                            !unavailable,
                            egui::Button::new(egui::RichText::new(&entry.label).strong())
                                .selected(selected),
                        );
                        targets.register(
                            ui,
                            &response,
                            &identity,
                            entry.label.clone(),
                            TargetKind::Button,
                        );
                        if response.has_focus() {
                            result_has_focus = true;
                            self.settings_search_selected_id = Some(control_id.clone());
                        }
                        #[cfg(test)]
                        self.test_search_result_responses.push((
                            control_id.clone(),
                            response.id,
                            response.rect,
                        ));
                        self.settings_search_result_response_ids.push(response.id);
                        let enter_while_jump_owns_input = jump_mode_active
                            && ctx.input(|input| input.key_pressed(egui::Key::Enter));
                        if response.clicked() && first_ui_pass && !enter_while_jump_owns_input {
                            self.settings_search_selected_id = Some(control_id.clone());
                            if settings_owner_instance.is_some() {
                                activated_result = Some(control_id.clone());
                            }
                        }
                        ui.vertical(|ui| {
                            ui.small(format!(
                                "{} · {}",
                                settings_owner_label(&entry.owner),
                                entry.breadcrumb.join(" › ")
                            ));
                            ui.small(&entry.help);
                            if !entry.synonyms.is_empty() {
                                ui.small(format!("Also: {}", entry.synonyms.join(", ")));
                            }
                            if let Some(availability_text) = availability_text {
                                ui.small(egui::RichText::new(availability_text).weak());
                            }
                        });
                    });
                }
            }

            let local_enter_pressed = settings_owner_instance.is_some()
                && !jump_mode_active
                && (query.has_focus() || result_has_focus)
                && ctx.input(|input| {
                    input.key_pressed(egui::Key::Enter) && input.modifiers == egui::Modifiers::NONE
                });
            #[cfg(test)]
            let test_local_enter_pressed = local_enter_pressed;
            if local_enter_pressed {
                // Search activation takes ownership before the newly revealed
                // setting response is rendered, and the latch reapplies this
                // capture if egui discards and repeats the current pass.
                ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                self.search_enter_consumption_pending = true;
                if first_ui_pass && activated_result.is_none() {
                    activated_result = self.settings_search_selected_id.clone();
                }
            }
            if first_ui_pass
                && settings_owner_instance.is_some()
                && !jump_mode_active
                && (query.has_focus() || result_has_focus)
                && ctx.input(|input| input.key_pressed(egui::Key::Escape))
            {
                ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                self.settings_search_query.clear();
                self.settings_search_match_query.clear();
                self.settings_search_match_revision = None;
                self.settings_search_match_ids.clear();
                self.settings_search_selected_id = None;
                self.settings_search_page = 0;
                self.pending_settings_navigation = None;
                activated_result = None;
            }
            #[cfg(test)]
            self.record_search_navigation_event(format!(
                "search_input owner={settings_owner_instance:?} first={first_ui_pass} \
                 raw_enter_before={raw_enter_before_query} raw_enter_after_textedit={raw_enter_after_query} \
                 query_changed={query_changed} query_focus_before={focus_before_query:?} \
                 query_response={:?} query_has_focus={} result_has_focus={result_has_focus} \
                 local_enter={test_local_enter_pressed} activated={activated_result:?} \
                 page={} visible_results={:?} pending_before={pending_before_query:?} \
                 pending_after={:?} focused_after={:?}",
                query.id,
                query.has_focus(),
                self.settings_search_page,
                self.test_search_result_responses
                    .iter()
                    .map(|(id, _, _)| id.as_str())
                    .collect::<Vec<_>>(),
                self.pending_settings_navigation
                    .as_ref()
                    .map(|pending| format!(
                        "{}:{:?}:remaining={}",
                        pending.control_id, pending.stage, pending.frames_remaining
                    )),
                ctx.memory(|memory| memory.focused()),
            ));
        }
        activated_result
    }

    fn search_result_availability(
        &self,
        entry: &SettingsCatalogEntry,
        app: &LauncherApp,
    ) -> (Option<String>, bool) {
        if entry.id.as_str() == "plugin.notes.external_open"
            && self.plugin_settings.get("note").is_some_and(|settings| {
                serde_json::from_value::<NotePluginSettings>(settings.clone()).is_err()
            })
        {
            return (
                Some(
                    "Stored Notes options are malformed; edit the stored value before changing this option. The original value is preserved."
                        .into(),
                ),
                true,
            );
        }
        match &entry.availability {
            SettingsAvailability::Available => (None, false),
            SettingsAvailability::Unavailable { prerequisite } => {
                if self.builtin_plugin_prerequisite(entry, app).is_some() {
                    let plugin_name = match &entry.owner {
                        SettingsOwner::Plugin { name, .. } => name.as_str(),
                        _ => "plugin",
                    };
                    (
                        Some(format!(
                            "Unavailable: {prerequisite}. Select to reveal Enable {plugin_name} in Plugin Settings; it remains disabled."
                        )),
                        false,
                    )
                } else {
                    (Some(format!("Unavailable: {prerequisite}")), true)
                }
            }
            SettingsAvailability::Conditional { prerequisite } => {
                let satisfied = self.conditional_search_prerequisite_satisfied(entry, app);
                if satisfied == Some(true) {
                    let message = if matches!(entry.section, SettingsSection::Brightness) {
                        "A controllable monitor is available; Set remains explicit."
                    } else if matches!(entry.section, SettingsSection::Main(_)) {
                        "Available in the current draft."
                    } else {
                        "Available in the current owner."
                    };
                    (Some(message.into()), false)
                } else if matches!(entry.section, SettingsSection::Brightness) {
                    match satisfied {
                        Some(false) => (
                            Some("No controllable monitor is currently available.".into()),
                            true,
                        ),
                        None => (
                            Some("Open Brightness to check connected monitors; discovery runs only while the dialog is open.".into()),
                            false,
                        ),
                        Some(true) => (None, false),
                    }
                } else {
                    if let Some(target) = self.conditional_search_prerequisite_entry(entry, app) {
                        (
                            Some(format!(
                                "Requires: {prerequisite}. Select to reveal {} in {}; the prerequisite remains unchanged until you change it.",
                                target.label,
                                target.section.title(),
                            )),
                            false,
                        )
                    } else {
                        (Some(format!("Unavailable: {prerequisite}")), true)
                    }
                }
            }
        }
    }

    fn builtin_plugin_prerequisite(
        &self,
        entry: &SettingsCatalogEntry,
        app: &LauncherApp,
    ) -> Option<SettingsCatalogEntry> {
        let SettingsOwner::Plugin {
            name,
            origin: crate::plugin::PluginCatalogOrigin::Builtin,
            epoch: Some(epoch),
        } = &entry.owner
        else {
            return None;
        };
        if !matches!(entry.availability, SettingsAvailability::Unavailable { .. })
            || !self
                .search_catalog
                .entry(entry.id.as_str())
                .is_some_and(|current| {
                    current.owner == entry.owner && current.section == entry.section
                })
        {
            return None;
        }
        let snapshot = app.plugins.settings_catalog_snapshot_if_changed(None)?;
        if snapshot.revision != self.search_catalog.plugin_revision() {
            return None;
        }
        let mut registrations = snapshot
            .plugins
            .iter()
            .filter(|plugin| plugin.name == *name);
        let plugin = registrations.next()?;
        if registrations.next().is_some()
            || plugin.origin != crate::plugin::PluginCatalogOrigin::Builtin
            || plugin.epoch != *epoch
            || plugin.enabled
        {
            return None;
        }
        let prerequisite_id = format!("plugin_editor.builtin.{name}.{epoch}.enabled");
        let prerequisite = self.search_catalog.entry(&prerequisite_id)?;
        if prerequisite.section != SettingsSection::PluginEditor
            || !matches!(prerequisite.availability, SettingsAvailability::Available)
            || !matches!(
                &prerequisite.owner,
                SettingsOwner::Plugin {
                    name: owner_name,
                    origin: crate::plugin::PluginCatalogOrigin::Builtin,
                    epoch: Some(owner_epoch),
                } if owner_name == name && owner_epoch == epoch
            )
        {
            return None;
        }
        Some(prerequisite.clone())
    }

    fn conditional_search_prerequisite_satisfied(
        &self,
        entry: &SettingsCatalogEntry,
        app: &LauncherApp,
    ) -> Option<bool> {
        self.main_prerequisite_satisfied(entry, app).or_else(|| {
            match (&entry.section, entry.id.as_str()) {
                (SettingsSection::Theme, "theme.accent.color") => {
                    Some(app.theme_accent_override_is_set())
                }
                (SettingsSection::Brightness, "brightness.level") => {
                    app.brightness_settings_control_available()
                }
                _ => None,
            }
        })
    }

    fn conditional_search_prerequisite_entry(
        &self,
        entry: &SettingsCatalogEntry,
        app: &LauncherApp,
    ) -> Option<SettingsCatalogEntry> {
        if !matches!(entry.availability, SettingsAvailability::Conditional { .. }) {
            return None;
        }
        let target_id = match entry.id.as_str() {
            "hotkeys.quit.value" => "hotkeys.quit.enabled",
            "hotkeys.help.value" => "hotkeys.help.enabled",
            "paste.next.hotkey" => "paste.next.enabled",
            "paste.cancel.hotkey" => "paste.cancel.enabled",
            "launcher.static_position" => "launcher.follow_mouse",
            "launcher.static_x"
            | "launcher.static_y"
            | "launcher.static_width"
            | "launcher.static_height"
                if self.follow_mouse =>
            {
                "launcher.follow_mouse"
            }
            "launcher.static_x"
            | "launcher.static_y"
            | "launcher.static_width"
            | "launcher.static_height" => "launcher.static_position",
            "search.grid_rows"
            | "search.grid_columns"
            | "search.respect_plugin_capability"
            | "search.plugin_opt_out" => "search.grid_layout",
            "feedback.toast_duration" | "feedback.toast_duration/value" => "feedback.toasts",
            "feedback.timer_refresh" => "feedback.disable_timer_updates",
            "dashboard.diagnostics" if !cfg!(debug_assertions) => "feedback.debug_logging",
            "radial.shared_tap_hold"
            | "radial.hold_threshold"
            | "radial.tooltip_scope"
            | "radial.tooltip_delay"
            | "radial.expected_layout_diagnostics"
            | "radial.default_menu"
            | "radial.default_interaction"
            | "radial.default_submenu_presentation"
            | "radial.safety_policy"
            | "radial.global_item_inputs" => "radial.enabled",
            "radial.default_item_input_scope" if !self.radial_enabled => "radial.enabled",
            "radial.default_item_input_scope" => "radial.global_item_inputs",
            "notes.outline_default_open" => "notes.outline_sidebar",
            "theme.accent.color" => "theme.accent",
            // A completed migration has no current user-editable prerequisite.
            // Keep its recovery action disabled rather than inventing a route.
            "radial.restore_submenu_presentation" | "brightness.level" => return None,
            _ => return None,
        };
        let target = self.search_catalog.entry(target_id)?;
        let same_owner = matches!(
            (&entry.owner, &target.owner),
            (SettingsOwner::Application, SettingsOwner::Application)
                | (SettingsOwner::Theme, SettingsOwner::Theme)
        );
        if !same_owner
            || self.search_catalog.plugin_revision() != app.plugins.settings_catalog_revision()
            || !self
                .search_catalog
                .entry(entry.id.as_str())
                .is_some_and(|current| {
                    current.owner == entry.owner && current.section == entry.section
                })
            || !self.search_catalog.entry(target_id).is_some_and(|current| {
                current.owner == target.owner
                    && current.section == target.section
                    && match &current.availability {
                        SettingsAvailability::Available => true,
                        SettingsAvailability::Conditional { .. } => {
                            self.conditional_search_prerequisite_satisfied(current, app)
                                == Some(true)
                        }
                        SettingsAvailability::Unavailable { .. } => false,
                    }
            })
        {
            return None;
        }
        Some(target.clone())
    }

    fn main_prerequisite_satisfied(
        &self,
        entry: &SettingsCatalogEntry,
        app: &LauncherApp,
    ) -> Option<bool> {
        if !matches!(&entry.section, SettingsSection::Main(_)) {
            return None;
        }
        Some(match entry.id.as_str() {
            "hotkeys.quit.value" => self.quit_hotkey_enabled,
            "hotkeys.help.value" => self.help_hotkey_enabled,
            "paste.next.hotkey" => self.paste_next_enabled,
            "paste.cancel.hotkey" => self.paste_cancel_enabled,
            "launcher.static_position" => !self.follow_mouse,
            "launcher.static_x"
            | "launcher.static_y"
            | "launcher.static_width"
            | "launcher.static_height" => !self.follow_mouse && self.static_enabled,
            "search.grid_rows"
            | "search.grid_columns"
            | "search.respect_plugin_capability"
            | "search.plugin_opt_out" => self.query_results_layout_enabled,
            "feedback.toast_duration" | "feedback.toast_duration/value" => self.show_toasts,
            "feedback.timer_refresh" => !self.disable_timer_updates,
            "dashboard.diagnostics" => self.debug_logging || cfg!(debug_assertions),
            "radial.shared_tap_hold"
            | "radial.hold_threshold"
            | "radial.tooltip_scope"
            | "radial.tooltip_delay"
            | "radial.expected_layout_diagnostics"
            | "radial.default_menu"
            | "radial.default_interaction"
            | "radial.default_submenu_presentation"
            | "radial.safety_policy"
            | "radial.global_item_inputs" => self.radial_enabled,
            "radial.default_item_input_scope" => {
                self.radial_enabled && self.radial_global_item_inputs
            }
            "radial.restore_submenu_presentation" => app
                .radial_migration_receipt
                .as_ref()
                .is_some_and(|receipt| {
                    receipt.state == crate::settings::SubmenuMigrationState::Applied
                }),
            _ => false,
        })
    }

    fn activate_settings_search_result(
        &mut self,
        app: &mut LauncherApp,
        settings_owner_instance: Option<u64>,
        entry: SettingsCatalogEntry,
    ) {
        let Some(settings_instance) = settings_owner_instance else {
            return;
        };
        let (navigation_entry, delivery) = match &entry.availability {
            SettingsAvailability::Unavailable { .. } => {
                let Some(prerequisite) = self.builtin_plugin_prerequisite(&entry, app) else {
                    return;
                };
                (
                    prerequisite,
                    SettingsNavigationDelivery::BuiltinPrerequisite {
                        requested_control_id: entry.id.as_str().to_owned(),
                    },
                )
            }
            SettingsAvailability::Conditional { .. }
                if self.conditional_search_prerequisite_satisfied(&entry, app) == Some(false) =>
            {
                let Some(prerequisite) = self.conditional_search_prerequisite_entry(&entry, app)
                else {
                    self.pending_settings_navigation = None;
                    return;
                };
                (
                    prerequisite,
                    SettingsNavigationDelivery::ConditionalPrerequisite {
                        requested_control_id: entry.id.as_str().to_owned(),
                    },
                )
            }
            _ => (entry.clone(), SettingsNavigationDelivery::ExactControl),
        };
        let condition_met = match &entry.availability {
            SettingsAvailability::Available => true,
            SettingsAvailability::Unavailable { .. } => false,
            SettingsAvailability::Conditional { .. } => {
                let satisfied = self.conditional_search_prerequisite_satisfied(&entry, app);
                satisfied == Some(true)
                    || (matches!(entry.section, SettingsSection::Brightness) && satisfied.is_none())
            }
        };
        if matches!(
            &entry.availability,
            SettingsAvailability::Conditional { .. }
        ) && !condition_met
            && !matches!(
                &delivery,
                SettingsNavigationDelivery::ConditionalPrerequisite { .. }
            )
        {
            self.pending_settings_navigation = None;
            return;
        }

        self.queue_settings_navigation_with_delivery(
            &navigation_entry,
            Some(settings_instance),
            delivery,
        );

        match &navigation_entry.section {
            SettingsSection::Main(_) | SettingsSection::Notes | SettingsSection::Plugin { .. } => {}
            SettingsSection::Theme => {
                app.open_theme_settings_dialog();
                app.focus_panel(crate::gui::Panel::ThemeSettingsDialog);
            }
            SettingsSection::Mouse => {
                app.open_mouse_settings_dialog();
                app.focus_panel(crate::gui::Panel::MouseSettingsDialog);
            }
            SettingsSection::MouseGestures => {
                if !app.is_mouse_gesture_settings_dialog_open() {
                    app.open_mouse_gesture_settings_dialog();
                }
                app.focus_panel(crate::gui::Panel::MouseGestureSettingsDialog);
            }
            SettingsSection::PluginEditor => {
                app.show_plugins = true;
                app.focus_panel(crate::gui::Panel::Plugins);
            }
            SettingsSection::Brightness => {
                app.open_brightness_settings_dialog();
            }
        }
    }

    fn render_paste_sequence_section(
        &mut self,
        ui: &mut egui::Ui,
        targets: &mut SettingsJumpTargets,
    ) {
        render_paste_shortcut(
            ui,
            "Next",
            setting_ids::PASTE_NEXT_ENABLED,
            setting_ids::PASTE_NEXT_HOTKEY,
            &mut self.paste_next_enabled,
            &mut self.paste_sequence.next_hotkey,
            targets,
        );
        render_paste_shortcut(
            ui,
            "Cancel",
            setting_ids::PASTE_CANCEL_ENABLED,
            setting_ids::PASTE_CANCEL_HOTKEY,
            &mut self.paste_cancel_enabled,
            &mut self.paste_sequence.cancel_hotkey,
            targets,
        );
        targets.checkbox(
            ui,
            &mut self.paste_sequence.escape_cancel_enabled,
            setting_ids::PASTE_ESCAPE_CANCEL,
            "Escape cancels active playback",
        );
        targets.checkbox(
            ui,
            &mut self.paste_sequence.keep_dialog_visible_during_playback,
            setting_ids::PASTE_KEEP_VISIBLE,
            "Keep Paste Sequences dialog visible during playback",
        );
        crate::gui::theme::widgets::secondary_help(
            ui,
            "When started from this dialog, leave it open while you paste into another application.",
        );
        crate::gui::theme::widgets::secondary_help(
            ui,
            "Empty shortcuts are disabled. Shortcut conflicts are checked on Save. Changes take effect after Save succeeds.",
        );
    }

    fn render_hotkey_section(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        targets: &mut SettingsJumpTargets,
    ) {
        crate::gui::theme::widgets::labeled_form_row(ui, "Launcher hotkey", |ui| {
            let resp = targets.text_edit_singleline(
                ui,
                &mut self.hotkey,
                setting_ids::HOTKEY_LAUNCHER,
                "Launcher hotkey",
            );
            if resp.changed() {
                self.hotkey_valid = crate::hotkey::parse_hotkey(&self.hotkey).is_some();
                if self.hotkey_valid {
                    self.last_valid_hotkey = self.hotkey.clone();
                }
            }
            hotkey_indicator(ui, self.hotkey_valid);
            resp
        });
        self.render_optional_hotkey(
            ui,
            targets,
            "Enable quit hotkey",
            "Quit hotkey",
            HotkeyKind::Quit,
        );
        self.render_optional_hotkey(
            ui,
            targets,
            "Enable help hotkey",
            "Help hotkey",
            HotkeyKind::Help,
        );
        let directory = targets.button(ui, "hotkeys.open_directory", "Open Hotkey Directory");
        crate::gui::theme::widgets::secondary_help(
            ui,
            "Search configured shortcuts, see reported status and conflicts, and open the current owning editor.",
        );
        if targets.clicked(&directory) {
            app.request_hotkey_directory_open();
        }
    }

    fn render_radial_section(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        targets: &mut SettingsJumpTargets,
    ) {
        use crate::commands::RadialCommandHost;
        use crate::radial::model::{
            InteractionMode, RadialSafetyPolicy, SubmenuPresentation, TooltipScope, TriggerScope,
        };

        targets.checkbox(
            ui,
            &mut self.radial_enabled,
            setting_ids::RADIAL_ENABLED,
            "Enable radial menus",
        );
        ui.add_enabled_ui(self.radial_enabled, |ui| {
            targets.checkbox(
                ui,
                &mut self.radial_shared_tap_hold,
                setting_ids::RADIAL_SHARED_TAP_HOLD,
                "Share the launcher hotkey between tap and hold",
            );
            if self.radial_shared_tap_hold {
            crate::gui::theme::widgets::secondary_help(ui, RADIAL_TAP_HOLD_EXPLANATION);
            } else {
            crate::gui::theme::widgets::secondary_help(ui, RADIAL_LEGACY_TIMING_EXPLANATION);
            }
        crate::gui::theme::widgets::labeled_form_row(
            ui,
            "Hold threshold (ms)",
            |ui| {
                let response = ui.add(
                    egui::DragValue::new(&mut self.radial_hold_threshold_ms)
                        .range(
                            crate::radial::settings::MIN_HOLD_THRESHOLD_MS
                                ..=crate::radial::settings::MAX_HOLD_THRESHOLD_MS,
                        )
                        .speed(10),
                );
                targets.response(
                    ui,
                    &response,
                    "radial.hold_threshold",
                    "Radial hold threshold",
                    TargetKind::TextEntry,
                );
                response
            },
        );
            let tooltip_scope_order = targets.reserve_order();
            let tooltip_scope = egui::ComboBox::from_label("Full-label tooltip scope")
                .selected_text(match self.radial_tooltip_scope {
                    TooltipScope::Off => "Off",
                    TooltipScope::TruncatedOnly => "Truncated labels only",
                    TooltipScope::AllCells => "All cells",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.radial_tooltip_scope,
                        TooltipScope::Off,
                        "Off",
                    );
                    ui.selectable_value(
                        &mut self.radial_tooltip_scope,
                        TooltipScope::TruncatedOnly,
                        "Truncated labels only",
                    );
                    ui.selectable_value(
                        &mut self.radial_tooltip_scope,
                        TooltipScope::AllCells,
                        "All cells",
                    );
                });
            targets.register_at(
                ui,
                &tooltip_scope.response,
                "radial.tooltip_scope",
                "Radial tooltip scope",
                TargetKind::ComboBox,
                tooltip_scope_order,
            );
        crate::gui::theme::widgets::labeled_form_row(ui, "Tooltip delay (ms)", |ui| {
            let response = ui.add(
                    egui::DragValue::new(&mut self.radial_tooltip_delay_ms)
                        .range(
                            crate::radial::settings::MIN_TOOLTIP_DELAY_MS
                                ..=crate::radial::settings::MAX_TOOLTIP_DELAY_MS,
                        )
                        .speed(25),
            );
            targets.response(
                ui,
                &response,
                "radial.tooltip_delay",
                "Radial tooltip delay",
                TargetKind::TextEntry,
            );
            response
        });
        crate::gui::theme::widgets::secondary_help(
            ui,
            "Tooltip delay is independent of the launcher hold threshold and item dwell delay.",
        );
            targets.checkbox(
                ui,
                &mut self.radial_show_expected_layout_diagnostics,
                setting_ids::RADIAL_EXPECTED_LAYOUT_DIAGNOSTICS,
                "Show expected label-layout diagnostics",
            );

            let document = crate::gui::radial_published_document();
            let effective_default = if self.radial_default_menu_id.trim().is_empty() {
                document.default_menu_id.as_str().to_owned()
            } else {
                self.radial_default_menu_id.trim().to_owned()
            };
            let default_menu_order = targets.reserve_order();
            let default_menu = egui::ComboBox::from_label("Default menu")
                .selected_text(effective_default)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.radial_default_menu_id,
                        String::new(),
                        format!("Document default ({})", document.default_menu_id),
                    );
                    for menu in &document.menus {
                        ui.selectable_value(
                            &mut self.radial_default_menu_id,
                            menu.id.as_str().to_owned(),
                            format!("{} ({})", menu.name, menu.id),
                        );
                    }
                });
            targets.register_at(
                ui,
                &default_menu.response,
                "radial.default_menu",
                "Default radial menu",
                TargetKind::ComboBox,
                default_menu_order,
            );
            let interaction_order = targets.reserve_order();
            let interaction = egui::ComboBox::from_label("New menu interaction")
                .selected_text(match self.radial_default_interaction {
                    InteractionMode::StickyClick => "Sticky click",
                    InteractionMode::ReleaseToSelect => "Release to select",
                    InteractionMode::HoldAndClick => "Hold and click",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.radial_default_interaction,
                        InteractionMode::StickyClick,
                        "Sticky click",
                    );
                    ui.selectable_value(
                        &mut self.radial_default_interaction,
                        InteractionMode::ReleaseToSelect,
                        "Release to select",
                    );
                    ui.selectable_value(
                        &mut self.radial_default_interaction,
                        InteractionMode::HoldAndClick,
                        "Hold and click",
                    );
                });
            targets.register_at(
                ui,
                &interaction.response,
                "radial.default_interaction",
                "New menu interaction mode",
                TargetKind::ComboBox,
                interaction_order,
            );
        crate::gui::theme::widgets::secondary_help(
            ui,
                "Sticky click keeps the tree available for repeated choices; release-to-select activates on release, and hold-and-click requires an explicit click.",
            );
            let submenu_order = targets.reserve_order();
            let submenu = egui::ComboBox::from_label("New-menu default submenu behavior")
                .selected_text(match self.radial_default_submenu_presentation {
                    SubmenuPresentation::Cascade => "Cascade",
                    SubmenuPresentation::SameCenter => "Same center",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.radial_default_submenu_presentation,
                        SubmenuPresentation::Cascade,
                        "Cascade",
                    );
                    ui.selectable_value(
                        &mut self.radial_default_submenu_presentation,
                        SubmenuPresentation::SameCenter,
                        "Same center",
                    );
                });
            targets.register_at(
                ui,
                &submenu.response,
                "radial.default_submenu_presentation",
                "New submenu presentation",
                TargetKind::ComboBox,
                submenu_order,
            );
            let safety_order = targets.reserve_order();
            let safety = egui::ComboBox::from_label("Destructive action safety")
                .selected_text(match self.radial_safety_policy {
                    RadialSafetyPolicy::InheritLauncher => "Use launcher setting",
                    RadialSafetyPolicy::AlwaysConfirmDestructive => "Always confirm",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.radial_safety_policy,
                        RadialSafetyPolicy::InheritLauncher,
                        "Use launcher setting",
                    );
                    ui.selectable_value(
                        &mut self.radial_safety_policy,
                        RadialSafetyPolicy::AlwaysConfirmDestructive,
                        "Always confirm destructive radial actions",
                    );
                });
            targets.register_at(
                ui,
                &safety.response,
                "radial.safety_policy",
                "Radial destructive action safety",
                TargetKind::ComboBox,
                safety_order,
            );
            let global_inputs = targets.checkbox(
                ui,
                &mut self.radial_global_item_inputs,
                setting_ids::RADIAL_GLOBAL_ITEM_INPUTS,
                "Allow process-wide radial item shortcuts and hotstrings",
            );
            if global_inputs.changed() && !self.radial_global_item_inputs {
                self.radial_default_item_input_scope = TriggerScope::MenuLocal;
            }
            let item_scope_order = targets.reserve_order();
            let item_scope = egui::ComboBox::from_label("New item input scope")
                .selected_text(match self.radial_default_item_input_scope {
                    TriggerScope::MenuLocal => "Current menu only",
                    TriggerScope::Global => "Process-wide",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.radial_default_item_input_scope,
                        TriggerScope::MenuLocal,
                        "Current menu only",
                    );
                    ui.add_enabled_ui(self.radial_global_item_inputs, |ui| {
                        ui.selectable_value(
                            &mut self.radial_default_item_input_scope,
                            TriggerScope::Global,
                            "Process-wide",
                        );
                    });
                });
            targets.register_at(
                ui,
                &item_scope.response,
                "radial.default_item_input_scope",
                "New radial item input scope",
                TargetKind::ComboBox,
                item_scope_order,
            );

        crate::gui::theme::widgets::secondary_help(ui, &format!(
                "Current shared trigger: {} ({})",
                self.hotkey,
                if self.radial_shared_tap_hold {
                    "tap launcher / hold radial"
                } else {
                    "legacy launcher timing"
                }
        ));
            let issues = self.radial_validation_issues(&document);
            if issues.is_empty() {
            crate::gui::theme::widgets::secondary_help(
                ui,
                "No conflicts with launcher, quit, help, Screen Draw, or global radial inputs.",
            );
            } else {
                for issue in issues {
                crate::gui::theme::widgets::semantic_status(
                    ui,
                    crate::gui::theme::widgets::StatusTone::Warning,
                    "Warning",
                    Some(&issue.message),
                );
                }
            }
        crate::gui::theme::widgets::secondary_help(
            ui,
            "Back returns from a submenu; Esc or `radial close` closes the radial tree.",
        );
        });
        ui.horizontal(|ui| {
            let edit_menus = targets.button(ui, setting_ids::RADIAL_EDIT_MENUS, "Edit menus...");
            if targets.clicked(&edit_menus) {
                RadialCommandHost::open_radial_editor(app, false);
            }
            let edit_skins = targets.button(ui, setting_ids::RADIAL_EDIT_SKINS, "Edit skins...");
            if targets.clicked(&edit_skins) {
                RadialCommandHost::open_radial_editor(app, true);
            }
        });
        if let Some(receipt) = app.radial_migration_receipt.as_ref() {
            match receipt.state {
                crate::settings::SubmenuMigrationState::Applied => {
                    crate::gui::theme::widgets::secondary_help(
                        ui,
                        "Existing submenu presentations were converted to Same center.",
                    );
                    let restore = targets.button(
                        ui,
                        setting_ids::RADIAL_RESTORE_SUBMENU,
                        "Restore previous submenu presentation",
                    );
                    if targets.clicked(&restore) {
                        app.request_radial_submenu_migration_restore();
                    }
                }
                crate::settings::SubmenuMigrationState::Prepared
                | crate::settings::SubmenuMigrationState::UndoPrepared => {
                    crate::gui::theme::widgets::secondary_help(
                        ui,
                        "Radial submenu migration recovery is pending.",
                    );
                }
                crate::settings::SubmenuMigrationState::Undone => {
                    crate::gui::theme::widgets::secondary_help(
                        ui,
                        "The one-time radial submenu migration has been restored.",
                    );
                }
                crate::settings::SubmenuMigrationState::Failed => {
                    crate::gui::theme::widgets::semantic_status(
                        ui,
                        crate::gui::theme::widgets::StatusTone::Error,
                        "Error",
                        Some(
                            receipt
                                .failure
                                .as_deref()
                                .unwrap_or("Radial migration failed."),
                        ),
                    );
                }
            }
        }
    }

    fn render_optional_hotkey(
        &mut self,
        ui: &mut egui::Ui,
        targets: &mut SettingsJumpTargets,
        toggle_label: &str,
        field_label: &str,
        kind: HotkeyKind,
    ) {
        let (enabled, value, valid, last_valid) = match kind {
            HotkeyKind::Quit => (
                &mut self.quit_hotkey_enabled,
                &mut self.quit_hotkey,
                &mut self.quit_hotkey_valid,
                &mut self.last_valid_quit_hotkey,
            ),
            HotkeyKind::Help => (
                &mut self.help_hotkey_enabled,
                &mut self.help_hotkey,
                &mut self.help_hotkey_valid,
                &mut self.last_valid_help_hotkey,
            ),
        };
        let (toggle_identity, field_identity) = match kind {
            HotkeyKind::Quit => (
                setting_ids::HOTKEY_QUIT_ENABLED,
                setting_ids::HOTKEY_QUIT_VALUE,
            ),
            HotkeyKind::Help => (
                setting_ids::HOTKEY_HELP_ENABLED,
                setting_ids::HOTKEY_HELP_VALUE,
            ),
        };
        targets.checkbox(ui, enabled, toggle_identity, toggle_label);
        if *enabled {
            crate::gui::theme::widgets::labeled_form_row(ui, field_label, |ui| {
                let resp = targets.text_edit_singleline(ui, value, field_identity, field_label);
                if resp.changed() {
                    *valid = crate::hotkey::parse_hotkey(value).is_some();
                    if *valid {
                        *last_valid = value.clone();
                    }
                }
                hotkey_indicator(ui, *valid);
                resp
            });
        }
    }

    fn render_actions_safety_feedback_section(
        &mut self,
        ui: &mut egui::Ui,
        targets: &mut SettingsJumpTargets,
    ) {
        ui.label(egui::RichText::new("After running an action").strong());
        ui.horizontal_wrapped(|ui| {
            targets.checkbox(
                ui,
                &mut self.hide_after_run,
                setting_ids::ACTIONS_HIDE_AFTER_RUN,
                "Hide window after running action",
            );
            targets.checkbox(
                ui,
                &mut self.preserve_command,
                setting_ids::ACTIONS_PRESERVE_COMMAND,
                "Preserve command after run",
            );
            targets.checkbox(
                ui,
                &mut self.clear_query_after_run,
                setting_ids::ACTIONS_CLEAR_QUERY,
                "Clear query after run",
            );
        });

        ui.label(egui::RichText::new("Safety").strong());
        targets.checkbox(
            ui,
            &mut self.require_confirm_destructive,
            setting_ids::ACTIONS_CONFIRM_DESTRUCTIVE,
            "Require confirm for destructive actions",
        );

        ui.label(egui::RichText::new("Notifications and errors").strong());
        targets.checkbox(
            ui,
            &mut self.show_toasts,
            setting_ids::FEEDBACK_TOASTS,
            "Enable toast notifications",
        );
        if self.show_toasts {
            crate::gui::theme::widgets::labeled_form_row(ui, "Toast duration (s)", |ui| {
                targets.slider_with_numeric_editor(
                    ui,
                    &mut self.toast_duration,
                    0.1..=5.0,
                    setting_ids::FEEDBACK_TOAST_DURATION,
                    "Toast duration",
                    setting_ids::FEEDBACK_TOAST_DURATION_VALUE,
                    "Toast duration value",
                )
            });
        }
        ui.horizontal_wrapped(|ui| {
            targets.checkbox(
                ui,
                &mut self.show_inline_errors,
                setting_ids::FEEDBACK_INLINE_ERRORS,
                "Show inline errors",
            );
            targets.checkbox(
                ui,
                &mut self.show_error_toasts,
                setting_ids::FEEDBACK_ERROR_TOASTS,
                "Show error toasts",
            );
        });

        ui.label(egui::RichText::new("Diagnostics and refresh").strong());
        ui.horizontal(|ui| {
            let combo_identity = setting_ids::FEEDBACK_DEBUG_LOGGING;
            let combo_order = targets.reserve_order();
            let popup_scope = targets.combo_popup_scope(combo_identity);
            let combo = egui::ComboBox::from_label("Debug logging")
                .selected_text(if self.debug_logging {
                    "Enabled"
                } else {
                    "Disabled"
                })
                .show_ui(ui, |ui| {
                    let disabled = ui.selectable_value(&mut self.debug_logging, false, "Disabled");
                    targets.popup_option(
                        popup_scope.as_ref(),
                        ui,
                        &disabled,
                        "option/disabled",
                        "Debug logging: Disabled",
                    );
                    let enabled = ui.selectable_value(&mut self.debug_logging, true, "Enabled");
                    targets.popup_option(
                        popup_scope.as_ref(),
                        ui,
                        &enabled,
                        "option/enabled",
                        "Debug logging: Enabled",
                    );
                });
            targets.register_at(
                ui,
                &combo.response,
                combo_identity,
                "Debug logging",
                TargetKind::ComboBox,
                combo_order,
            );
        });
        targets.checkbox(
            ui,
            &mut self.disable_timer_updates,
            setting_ids::FEEDBACK_DISABLE_TIMER_UPDATES,
            "Disable timer auto refresh",
        );
        crate::gui::theme::widgets::labeled_form_row(ui, "Timer refresh rate (s)", |ui| {
            let response = ui.add_enabled_ui(!self.disable_timer_updates, |ui| {
                let response = ui.add(
                    egui::DragValue::new(&mut self.timer_refresh)
                        .range(0.1..=60.0)
                        .speed(0.1),
                );
                targets.response(
                    ui,
                    &response,
                    "feedback.timer_refresh",
                    "Timer refresh rate",
                    TargetKind::TextEntry,
                );
                response
            });
            response.inner
        });
    }

    fn render_launcher_window_section(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        targets: &mut SettingsJumpTargets,
    ) {
        targets.checkbox(
            ui,
            &mut self.always_on_top,
            setting_ids::LAUNCHER_ALWAYS_ON_TOP,
            "Always on top",
        );
        ui.horizontal_wrapped(|ui| {
            targets.checkbox(
                ui,
                &mut self.show_launcher_icon,
                setting_ids::LAUNCHER_SHOW_ICON,
                "Show Launcher Icon",
            );
            targets.checkbox(
                ui,
                &mut self.show_launcher_title,
                setting_ids::LAUNCHER_SHOW_TITLE,
                "Show Launcher Title",
            );
        });
        targets.checkbox(
            ui,
            &mut self.experimental_jump_mode_enabled,
            setting_ids::LAUNCHER_JUMP_MODE,
            "Experimental Jump Mode",
        )
        .on_hover_text(
            "Shift+Space displays hints; typing a hint selects, Enter activates. Available in supported screens only.",
        );
        crate::gui::theme::widgets::labeled_form_row(ui, "Off-screen position", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("X");
                let x = ui.add(egui::DragValue::new(&mut self.offscreen_x));
                targets.response(
                    ui,
                    &x,
                    "launcher.offscreen_x",
                    "Off-screen position X",
                    TargetKind::TextEntry,
                );
                ui.label("Y");
                let y = ui.add(egui::DragValue::new(&mut self.offscreen_y));
                targets.response(
                    ui,
                    &y,
                    "launcher.offscreen_y",
                    "Off-screen position Y",
                    TargetKind::TextEntry,
                );
                y
            })
            .inner
        });
        let follow_mouse_resp = targets.checkbox(
            ui,
            &mut self.follow_mouse,
            setting_ids::LAUNCHER_FOLLOW_MOUSE,
            "Follow mouse",
        );
        if follow_mouse_resp.changed() && self.follow_mouse {
            self.static_enabled = false;
        }
        ui.add_enabled_ui(!self.follow_mouse, |ui| {
            targets.checkbox(
                ui,
                &mut self.static_enabled,
                setting_ids::LAUNCHER_STATIC_POSITION,
                "Use static position",
            );
        });
        if !self.follow_mouse && self.static_enabled {
            crate::gui::theme::widgets::labeled_form_row(ui, "Static X / Y / W / H", |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("X");
                    let x = ui.add(egui::DragValue::new(&mut self.static_x));
                    targets.response(
                        ui,
                        &x,
                        "launcher.static_x",
                        "Static position X",
                        TargetKind::TextEntry,
                    );
                    ui.label("Y");
                    let y = ui.add(egui::DragValue::new(&mut self.static_y));
                    targets.response(
                        ui,
                        &y,
                        "launcher.static_y",
                        "Static position Y",
                        TargetKind::TextEntry,
                    );
                    ui.label("W");
                    let width = ui.add(egui::DragValue::new(&mut self.static_w));
                    targets.response(
                        ui,
                        &width,
                        "launcher.static_width",
                        "Static window width",
                        TargetKind::TextEntry,
                    );
                    ui.label("H");
                    let height = ui.add(egui::DragValue::new(&mut self.static_h));
                    targets.response(
                        ui,
                        &height,
                        "launcher.static_height",
                        "Static window height",
                        TargetKind::TextEntry,
                    );
                    let snapshot = targets.button(ui, "launcher.snapshot", "Snapshot");
                    if targets.clicked(&snapshot) {
                        self.static_x = app.window_pos.0;
                        self.static_y = app.window_pos.1;
                        self.static_w = app.window_size.0;
                        self.static_h = app.window_size.1;
                    }
                    snapshot
                })
                .inner
            });
        }
        crate::gui::theme::widgets::labeled_form_row(ui, "Query scale", |ui| {
            targets.slider_with_numeric_editor(
                ui,
                &mut self.query_scale,
                0.5..=5.0,
                setting_ids::LAUNCHER_QUERY_SCALE,
                "Query scale",
                setting_ids::LAUNCHER_QUERY_SCALE_VALUE,
                "Query scale value",
            )
        });
        crate::gui::theme::widgets::labeled_form_row(ui, "List scale", |ui| {
            targets.slider_with_numeric_editor(
                ui,
                &mut self.list_scale,
                0.5..=5.0,
                setting_ids::LAUNCHER_LIST_SCALE,
                "List scale",
                setting_ids::LAUNCHER_LIST_SCALE_VALUE,
                "List scale value",
            )
        });
        let theme_settings = targets.button(
            ui,
            setting_ids::LAUNCHER_THEME_SETTINGS,
            "Open Theme Settings...",
        );
        if targets.clicked(&theme_settings) {
            app.open_theme_settings_dialog();
        }
    }

    fn render_search_results_section(
        &mut self,
        ui: &mut egui::Ui,
        targets: &mut SettingsJumpTargets,
    ) {
        targets.checkbox(
            ui,
            &mut self.query_autocomplete,
            setting_ids::SEARCH_AUTOCOMPLETE,
            "Enable query autocomplete",
        );
        crate::gui::theme::widgets::labeled_form_row(ui, "Fuzzy weight", |ui| {
            targets.slider_with_numeric_editor(
                ui,
                &mut self.fuzzy_weight,
                0.0..=5.0,
                setting_ids::SEARCH_FUZZY_WEIGHT,
                "Fuzzy weight",
                setting_ids::SEARCH_FUZZY_WEIGHT_VALUE,
                "Fuzzy weight value",
            )
        });
        crate::gui::theme::widgets::labeled_form_row(ui, "Usage weight", |ui| {
            targets.slider_with_numeric_editor(
                ui,
                &mut self.usage_weight,
                0.0..=5.0,
                setting_ids::SEARCH_USAGE_WEIGHT,
                "Usage weight",
                setting_ids::SEARCH_USAGE_WEIGHT_VALUE,
                "Usage weight value",
            )
        });
        targets.checkbox(
            ui,
            &mut self.match_exact,
            setting_ids::SEARCH_MATCH_EXACT,
            "Match exact",
        );
        crate::gui::theme::widgets::labeled_form_row(ui, "Page jump", |ui| {
            let response = ui.add(
                egui::DragValue::new(&mut self.page_jump)
                    .range(1..=100)
                    .speed(1),
            );
            targets.response(
                ui,
                &response,
                "search.page_jump",
                "Page jump",
                TargetKind::TextEntry,
            );
            response
        });
        targets.checkbox(
            ui,
            &mut self.query_results_layout_enabled,
            setting_ids::SEARCH_GRID_LAYOUT,
            "Display results in grid layout",
        );
        ui.add_enabled_ui(self.query_results_layout_enabled, |ui| {
            crate::gui::theme::widgets::labeled_form_row(ui, "Grid size", |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Rows");
                    let rows = ui.add(
                        egui::DragValue::new(&mut self.query_results_layout_rows)
                            .range(1..=100)
                            .speed(1),
                    );
                    targets.response(
                        ui,
                        &rows,
                        "search.grid_rows",
                        "Grid rows",
                        TargetKind::TextEntry,
                    );
                    ui.label("Columns");
                    let columns = ui.add(
                        egui::DragValue::new(&mut self.query_results_layout_cols)
                            .range(1..=100)
                            .speed(1),
                    );
                    targets.response(
                        ui,
                        &columns,
                        "search.grid_columns",
                        "Grid columns",
                        TargetKind::TextEntry,
                    );
                    columns
                })
                .inner
            });
            self.query_results_layout_rows = self.query_results_layout_rows.max(1);
            self.query_results_layout_cols = self.query_results_layout_cols.max(1);
            targets.checkbox(
                ui,
                &mut self.query_results_layout_respect_plugin_capability,
                setting_ids::SEARCH_RESPECT_PLUGIN_CAPABILITY,
                "Respect plugin list/grid capability",
            );
            ui.horizontal(|ui| {
                ui.label("Force list for plugins (comma separated)");
                targets.text_edit_singleline(
                    ui,
                    &mut self.query_results_layout_plugin_opt_out,
                    setting_ids::SEARCH_PLUGIN_OPT_OUT,
                    "Force list for plugins",
                );
            });
        });
    }

    fn render_dashboard_section(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        targets: &mut SettingsJumpTargets,
    ) {
        targets.checkbox(
            ui,
            &mut self.dashboard_enabled,
            setting_ids::DASHBOARD_ENABLED,
            "Enable dashboard when query is empty",
        );
        ui.horizontal(|ui| {
            ui.label("Dashboard config path");
            targets.text_edit_singleline(
                ui,
                &mut self.dashboard_path,
                setting_ids::DASHBOARD_PATH,
                "Dashboard config path",
            );
        });
        ui.horizontal(|ui| {
            ui.label("Default location");
            targets.text_edit_singleline(
                ui,
                &mut self.dashboard_default_location,
                setting_ids::DASHBOARD_DEFAULT_LOCATION,
                "Default location",
            );
        });
        targets.checkbox(
            ui,
            &mut self.dashboard_show_when_empty,
            setting_ids::DASHBOARD_SHOW_WHEN_EMPTY,
            "Show dashboard when the search box is blank",
        );
        targets.checkbox(
            ui,
            &mut self.reduce_dashboard_work_when_unfocused,
            setting_ids::DASHBOARD_REDUCE_WORK,
            "Reduce dashboard work when not focused",
        );
        if self.debug_logging || cfg!(debug_assertions) {
            targets.checkbox(
                ui,
                &mut self.show_dashboard_diagnostics,
                setting_ids::DASHBOARD_DIAGNOSTICS,
                "Show dashboard diagnostics (dev)",
            );
        }
        let customize = targets.button(
            ui,
            setting_ids::DASHBOARD_CUSTOMIZE,
            "Customize Dashboard...",
        );
        if targets.clicked(&customize) {
            app.show_dashboard_editor = true;
        }
    }

    fn render_plugin_sections(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        targets: &mut SettingsJumpTargets,
        reveal_section: Option<&SettingsSection>,
    ) {
        ui.separator();
        let toggle_sections = targets.button(
            ui,
            setting_ids::PLUGINS_TOGGLE_SECTIONS,
            if self.plugins_expanded {
                "Collapse plugin sections"
            } else {
                "Expand plugin sections"
            },
        );
        if targets.clicked(&toggle_sections) {
            self.plugins_expanded = !self.plugins_expanded;
            self.expand_request = Some(self.plugins_expanded);
        }
        let enabled_list = app.enabled_plugins_list();
        let mut open_mg_settings_dialog = false;
        let mut clipboard_modify_command = None;
        let clipboard_modify_status = app
            .clipboard_modify_config_diagnostic
            .clone()
            .unwrap_or_else(|| "valid".to_owned());

        for path in app.plugins.deferred_plugin_reload_paths() {
            ui.colored_label(
                egui::Color32::YELLOW,
                format!(
                    "Plugin reload pending for '{}'; close active plugin views and reload again.",
                    path.display()
                ),
            );
        }

        for name in app.plugins.plugin_names() {
            if name == "notes" {
                continue;
            }
            let enabled = match &enabled_list {
                Some(list) => list.contains(&name),
                None => true,
            };
            if !enabled {
                continue;
            }
            let Ok(mut plugin) = app.plugins.try_write_plugin(&name) else {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    format!("{name} settings are temporarily busy; retry next frame."),
                );
                continue;
            };
            let has_settings =
                plugin.default_settings().is_some() || self.plugin_settings.contains_key(&name);
            if !has_settings {
                continue;
            }

            let entry = self
                .plugin_settings
                .entry(name.clone())
                .or_insert_with(|| plugin.default_settings().unwrap_or(serde_json::Value::Null));
            let reveal_plugin = matches!(
                reveal_section,
                Some(SettingsSection::Plugin { name: target_name, .. })
                    if target_name == &name
            );
            Self::render_plugin_section(
                ui,
                reveal_plugin.then_some(true).or(self.expand_request),
                &name,
                targets
                    .navigation_observer
                    .as_ref()
                    .filter(|observer| observer.is_deferred_plugin(&name))
                    .cloned(),
                entry,
                plugin.as_mut(),
                &mut open_mg_settings_dialog,
                &mut clipboard_modify_command,
                &clipboard_modify_status,
                &mut self.confirm_clipboard_modify_factory_reset,
            );
        }

        if open_mg_settings_dialog {
            app.open_mouse_gesture_settings_dialog();
        }
        if let Some(command) = clipboard_modify_command {
            match command {
                ClipboardModifySettingsCommand::OpenDialog => app.open_clipboard_modify_dialog(),
                ClipboardModifySettingsCommand::OpenConfigFile => {
                    app.open_clipboard_modify_config_file()
                }
                ClipboardModifySettingsCommand::ReloadConfig => {
                    app.reload_clipboard_modify_config()
                }
                ClipboardModifySettingsCommand::ResetFactoryDefaults => {
                    app.reset_clipboard_modify_config_to_factory_defaults()
                }
            }
        }
        self.render_note_section(
            ui,
            matches!(reveal_section, Some(SettingsSection::Notes)),
            targets
                .navigation_observer
                .as_ref()
                .filter(|observer| observer.is_deferred_notes())
                .cloned(),
        );
    }

    fn render_plugin_section(
        ui: &mut egui::Ui,
        expand_request: Option<bool>,
        name: &str,
        navigation_observer: Option<SettingsNavigationFrameObserver>,
        entry: &mut serde_json::Value,
        plugin: &mut dyn crate::plugin::Plugin,
        open_mg_settings_dialog: &mut bool,
        clipboard_modify_command: &mut Option<ClipboardModifySettingsCommand>,
        clipboard_modify_status: &str,
        confirm_factory_reset: &mut bool,
    ) {
        let id = ui.make_persistent_id(format!("plugin_{name}"));
        let mut state =
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
        if let Some(open) = expand_request {
            state.set_open(open);
        }
        state
            .show_header(ui, |ui| {
                ui.label(format!("{name} settings"));
            })
            .body(|ui| {
                if name == "mouse_gestures" {
                    ui.label("Mouse gesture settings are managed in a dedicated dialog.");
                    ui.add_space(6.0);
                    if ui.button("Open Mouse Gesture Settings...").clicked() {
                        *open_mg_settings_dialog = true;
                    }
                    ui.add_space(4.0);
                    ui.small("Tip: you can also open this window via `mg settings`.");
                } else if name == "clipboard_modify" {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Open Clipboard Modify").clicked() {
                            *clipboard_modify_command =
                                Some(ClipboardModifySettingsCommand::OpenDialog);
                        }
                        if ui.button("Open clipboard_modifiers.json").clicked() {
                            *clipboard_modify_command =
                                Some(ClipboardModifySettingsCommand::OpenConfigFile);
                        }
                        if ui.button("Reload configuration").clicked() {
                            *clipboard_modify_command =
                                Some(ClipboardModifySettingsCommand::ReloadConfig);
                        }
                        if ui.button("Reset to factory defaults").clicked() {
                            *confirm_factory_reset = true;
                        }
                    });
                    if *confirm_factory_reset {
                        ui.group(|ui| {
                            ui.label("Replace the configuration with factory defaults? A timestamped backup will be created first.");
                            ui.horizontal(|ui| {
                                if ui.button("Confirm reset").clicked() {
                                    *clipboard_modify_command = Some(
                                        ClipboardModifySettingsCommand::ResetFactoryDefaults,
                                    );
                                    *confirm_factory_reset = false;
                                }
                                if ui.button("Cancel").clicked() {
                                    *confirm_factory_reset = false;
                                }
                            });
                        });
                    }
                    ui.label(format!("Configuration status: {clipboard_modify_status}"));
                    ui.separator();
                    let _observer_scope = navigation_observer
                        .as_ref()
                        .map(|observer| observer.install(ui.ctx()));
                    plugin.settings_ui(ui, entry);
                } else {
                    let _observer_scope = navigation_observer
                        .as_ref()
                        .map(|observer| observer.install(ui.ctx()));
                    plugin.settings_ui(ui, entry);
                }
            });
    }

    fn render_note_section(
        &mut self,
        ui: &mut egui::Ui,
        force_open: bool,
        navigation_observer: Option<SettingsNavigationFrameObserver>,
    ) {
        let id = ui.make_persistent_id(setting_ids::NOTES);
        let mut state =
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
        if force_open {
            state.set_open(true);
        } else if let Some(open) = self.expand_request {
            state.set_open(open);
        }
        state
            .show_header(ui, |ui| {
                ui.label("Note settings");
            })
            .body(|ui| {
                let _observer_scope = navigation_observer
                    .as_ref()
                    .map(|observer| observer.install(ui.ctx()));
                macro_rules! observed_checkbox {
                    ($ui:ident, $id:literal, $value:expr, $label:literal) => {{
                        let response = $ui.checkbox($value, $label);
                        observe_scoped_settings_navigation_response(
                            $ui,
                            &response,
                            $id,
                            TargetKind::Checkbox,
                        );
                        response
                    }};
                }
                ui.heading("Notes / Markdown");
                observed_checkbox!(
                    ui,
                    "notes.markdown",
                    &mut self.note_rich_markdown_enabled,
                    "Enable rich Markdown"
                );
                observed_checkbox!(
                    ui,
                    "notes.task_lists",
                    &mut self.note_task_lists_enabled,
                    "Enable task lists"
                );
                observed_checkbox!(
                    ui,
                    "notes.interactive_checkboxes",
                    &mut self.note_interactive_checkboxes_enabled,
                    "Enable interactive checkboxes"
                );
                observed_checkbox!(
                    ui,
                    "notes.collapsible_sections",
                    &mut self.note_collapsible_sections_enabled,
                    "Enable collapsible sections"
                );
                observed_checkbox!(
                    ui,
                    "notes.outline_sidebar",
                    &mut self.note_outline_sidebar_enabled,
                    "Enable outline sidebar"
                );
                ui.add_enabled_ui(self.note_outline_sidebar_enabled, |ui| {
                    observed_checkbox!(
                        ui,
                        "notes.outline_default_open",
                        &mut self.note_outline_sidebar_default_open,
                        "Open outline sidebar by default"
                    );
                });
                observed_checkbox!(
                    ui,
                    "notes.split_view",
                    &mut self.note_split_view_enabled,
                    "Enable split view"
                );
                let default_view_mode = egui::ComboBox::from_label("Default view mode")
                    .selected_text(match self.note_default_view_mode {
                        NoteViewMode::Edit => "Edit",
                        NoteViewMode::Preview => "Preview",
                        NoteViewMode::Split => "Split",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.note_default_view_mode,
                            NoteViewMode::Edit,
                            "Edit",
                        );
                        ui.selectable_value(
                            &mut self.note_default_view_mode,
                            NoteViewMode::Preview,
                            "Preview",
                        );
                        ui.add_enabled_ui(self.note_split_view_enabled, |ui| {
                            ui.selectable_value(
                                &mut self.note_default_view_mode,
                                NoteViewMode::Split,
                                "Split",
                            );
                        });
                    });
                observe_scoped_settings_navigation_response(
                    ui,
                    &default_view_mode.response,
                    "notes.default_view_mode",
                    TargetKind::ComboBox,
                );
                observed_checkbox!(
                    ui,
                    "notes.callouts",
                    &mut self.note_callouts_enabled,
                    "Enable callouts"
                );
                observed_checkbox!(
                    ui,
                    "notes.backlinks",
                    &mut self.note_backlinks_enabled,
                    "Enable backlinks"
                );
                observed_checkbox!(
                    ui,
                    "notes.aliases",
                    &mut self.note_aliases_enabled,
                    "Enable aliases"
                );
                observed_checkbox!(
                    ui,
                    "notes.templates",
                    &mut self.note_templates_enabled,
                    "Enable templates"
                );
                observed_checkbox!(
                    ui,
                    "notes.collapsed_sections_persist",
                    &mut self.note_collapsed_sections_persist,
                    "Remember collapsed sections"
                );
                ui.horizontal(|ui| {
                    ui.label("Max outline depth");
                    let response = ui.add(
                        egui::DragValue::new(&mut self.note_max_outline_depth)
                            .range(1..=6)
                            .clamp_existing_to_range(false)
                            .speed(1),
                    );
                    observe_scoped_settings_navigation_response(
                        ui,
                        &response,
                        "notes.max_outline_depth",
                        TargetKind::TextEntry,
                    );
                });
                ui.separator();
                ui.heading("Legacy note options");
                observed_checkbox!(
                    ui,
                    "notes.save_on_close",
                    &mut self.note_save_on_close,
                    "Save note on close (Esc)"
                );
                observed_checkbox!(
                    ui,
                    "notes.confirm_discard",
                    &mut self.note_confirm_discard_unsaved_changes,
                    "Confirm before discarding unsaved note changes"
                );
                observed_checkbox!(
                    ui,
                    "notes.always_overwrite",
                    &mut self.note_always_overwrite,
                    "Always overwrite existing notes"
                );
                observed_checkbox!(
                    ui,
                    "notes.images_as_links",
                    &mut self.note_images_as_links,
                    "Display images as links"
                );
                observed_checkbox!(
                    ui,
                    "notes.show_details",
                    &mut self.note_show_details,
                    "Show note details by default"
                );
                ui.horizontal(|ui| {
                    ui.label("Tag/link preview limit");
                    let response = ui
                        .add(egui::DragValue::new(&mut self.note_more_limit).range(1..=usize::MAX));
                    observe_scoped_settings_navigation_response(
                        ui,
                        &response,
                        "notes.preview_limit",
                        TargetKind::TextEntry,
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Note panel W");
                    let width =
                        ui.add(egui::DragValue::new(&mut self.note_panel_w).range(200.0..=2000.0));
                    observe_scoped_settings_navigation_response(
                        ui,
                        &width,
                        "notes.panel_width",
                        TargetKind::TextEntry,
                    );
                    ui.label("H");
                    let height =
                        ui.add(egui::DragValue::new(&mut self.note_panel_h).range(150.0..=1600.0));
                    observe_scoped_settings_navigation_response(
                        ui,
                        &height,
                        "notes.panel_height",
                        TargetKind::TextEntry,
                    );
                });
                let malformed_note_settings =
                    self.plugin_settings.get("note").is_some_and(|value| {
                        serde_json::from_value::<NotePluginSettings>(value.clone()).is_err()
                    });
                let mut cfg = self
                    .plugin_settings
                    .get("note")
                    .and_then(|value| {
                        serde_json::from_value::<NotePluginSettings>(value.clone()).ok()
                    })
                    .unwrap_or_default();
                if malformed_note_settings {
                    ui.colored_label(
                        egui::Color32::from_rgb(205, 145, 70),
                        "Stored Notes options are malformed; the original value is preserved.",
                    );
                }
                let original_external_open = cfg.external_open;
                let external_open = ui
                    .add_enabled_ui(!malformed_note_settings, |ui| {
                        egui::ComboBox::from_label("Open externally")
                            .selected_text(match cfg.external_open {
                                NoteExternalOpen::Neither => "Neither",
                                NoteExternalOpen::Powershell => "Powershell",
                                NoteExternalOpen::Notepad => "Notepad",
                                NoteExternalOpen::Wezterm => "WezTerm",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut cfg.external_open,
                                    NoteExternalOpen::Neither,
                                    "Neither",
                                );
                                ui.selectable_value(
                                    &mut cfg.external_open,
                                    NoteExternalOpen::Powershell,
                                    "Powershell",
                                );
                                ui.selectable_value(
                                    &mut cfg.external_open,
                                    NoteExternalOpen::Notepad,
                                    "Notepad",
                                );
                                ui.selectable_value(
                                    &mut cfg.external_open,
                                    NoteExternalOpen::Wezterm,
                                    "WezTerm",
                                );
                            })
                    })
                    .inner;
                observe_scoped_settings_navigation_response(
                    ui,
                    &external_open.response,
                    "plugin.notes.external_open",
                    TargetKind::ComboBox,
                );
                #[cfg(test)]
                {
                    self.test_notes_external_open_response =
                        Some(super::state::TestNotesControlResponse {
                            id: external_open.response.id,
                            rect: external_open.response.rect,
                            clip_rect: ui.clip_rect(),
                            visible: ui.is_visible(),
                            ui_enabled: ui.is_enabled(),
                            response_enabled: external_open.response.enabled(),
                            interaction_allowed: ui.ctx().memory(|memory| {
                                memory.allows_interaction(external_open.response.layer_id)
                            }),
                            has_focus: external_open.response.has_focus(),
                        });
                }
                if cfg.external_open != original_external_open {
                    let value = self
                        .plugin_settings
                        .entry("note".into())
                        .or_insert_with(|| serde_json::json!({}));
                    if let Some(object) = value.as_object_mut() {
                        let stored_value = match cfg.external_open {
                            NoteExternalOpen::Neither => "Neither",
                            NoteExternalOpen::Powershell => "Powershell",
                            NoteExternalOpen::Notepad => "Notepad",
                            NoteExternalOpen::Wezterm => "Wezterm",
                        };
                        object.insert(
                            "external_open".into(),
                            serde_json::Value::String(stored_value.into()),
                        );
                    }
                }
            });
    }
}

#[cfg(test)]
mod radial_language_tests {
    use super::*;

    #[test]
    fn tap_hold_language_states_release_crossing_and_legacy_semantics() {
        assert!(RADIAL_TAP_HOLD_EXPLANATION.contains("Release before the threshold"));
        assert!(RADIAL_TAP_HOLD_EXPLANATION.contains("Crossing the threshold"));
        assert!(RADIAL_TAP_HOLD_EXPLANATION.contains("never also toggles"));
        assert!(RADIAL_LEGACY_TIMING_EXPLANATION.contains("toggles immediately"));
    }
}

#[derive(Clone, Copy)]
enum HotkeyKind {
    Quit,
    Help,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClipboardModifySettingsCommand {
    OpenDialog,
    OpenConfigFile,
    ReloadConfig,
    ResetFactoryDefaults,
}

fn render_paste_shortcut(
    ui: &mut egui::Ui,
    name: &str,
    enabled_identity: &'static str,
    value_identity: &'static str,
    enabled: &mut bool,
    value: &mut Option<String>,
    targets: &mut SettingsJumpTargets,
) {
    ui.horizontal_wrapped(|ui| {
        let enable_label = format!("Enable {name} shortcut");
        let enabled_response = ui.checkbox(enabled, &enable_label);
        targets.response(
            ui,
            &enabled_response,
            enabled_identity,
            &enable_label,
            TargetKind::Checkbox,
        );
        ui.add_enabled_ui(*enabled, |ui| {
            let text = value.get_or_insert_with(String::new);
            let response =
                ui.add(egui::TextEdit::singleline(text).hint_text(format!("{name} shortcut")));
            targets.response(
                ui,
                &response,
                value_identity,
                match name {
                    "Next" => "Next shortcut",
                    "Cancel" => "Cancel shortcut",
                    _ => "Paste Sequence shortcut",
                },
                TargetKind::TextEntry,
            );
        });
        if !*enabled {
            ui.label("Disabled");
        } else {
            match crate::settings::PasteSequencePreferences::parse_optional_hotkey(value.as_deref())
            {
                Ok(None) => {
                    ui.label("Disabled (empty shortcut)");
                }
                Ok(Some(_)) => hotkey_indicator(ui, true),
                Err(_) => hotkey_indicator(ui, false),
            }
        }
    });
}

fn hotkey_indicator(ui: &mut egui::Ui, valid: bool) {
    let (tone, label) = if valid {
        (crate::gui::theme::widgets::StatusTone::Success, "Valid")
    } else {
        (crate::gui::theme::widgets::StatusTone::Error, "Invalid")
    };
    crate::gui::theme::widgets::semantic_status(ui, tone, label, None);
}

#[cfg(test)]
mod settings_shell_tests {
    use super::*;

    #[test]
    fn settings_search_delivery_highlight_expires_at_its_deadline() {
        let request = PendingSettingsNavigation {
            request_token: 7,
            control_id: "paste.next.enabled".into(),
            owner: SettingsOwner::Application,
            section: SettingsSection::Main(TopLevelSettingsSection::PasteSequences),
            catalog_revision: 3,
            settings_instance: Some(11),
            stage: SettingsNavigationStage::MainControl,
            frames_remaining: 0,
            scroll_requested: false,
            destination_observed: true,
            destination_owner_eligible: true,
            delivery: SettingsNavigationDelivery::ConditionalPrerequisite {
                requested_control_id: "paste.next.hotkey".into(),
            },
        };
        let highlight = SettingsNavigationHighlight::from_request(&request, 4.0);
        let deadline = highlight.expires_at;
        let mut editor = SettingsEditor::default();
        editor.settings_navigation_highlight = Some(highlight);

        editor.expire_settings_navigation_highlight(deadline - 0.001);
        assert!(editor.settings_navigation_highlight.is_some());
        editor.expire_settings_navigation_highlight(deadline);
        assert!(editor.settings_navigation_highlight.is_none());
    }

    fn paste_settings_frame(
        ctx: &egui::Context,
        app: &mut LauncherApp,
        events: Vec<egui::Event>,
    ) -> Vec<(String, egui::Rect)> {
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 900.0),
            )),
            events,
            ..Default::default()
        });
        let mut editor = std::mem::take(&mut app.settings_editor);
        let settings_owner_instance = app
            .show_settings
            .then(|| app.settings_navigation_owner_instance());
        let jump_mode_active = app.jump_mode_session_active();
        editor.ui(
            ctx,
            app,
            None,
            settings_owner_instance,
            jump_mode_active,
            true,
        );
        app.settings_editor = editor;
        let output = ctx.end_pass();
        fn collect(shape: &egui::epaint::Shape, labels: &mut Vec<(String, egui::Rect)>) {
            match shape {
                egui::epaint::Shape::Text(text) => labels.push((
                    text.galley.text().to_owned(),
                    text.galley.rect.translate(text.pos.to_vec2()),
                )),
                egui::epaint::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, labels);
                    }
                }
                _ => {}
            }
        }
        let mut labels = Vec::new();
        for clipped in &output.shapes {
            collect(&clipped.shape, &mut labels);
        }
        output.drop_without_applying_deltas();
        labels
    }

    fn paste_settings_click(ctx: &egui::Context, app: &mut LauncherApp, label: &str) {
        let labels = paste_settings_frame(ctx, app, vec![]);
        let label_rect = labels
            .iter()
            .find(|(text, _)| text == label)
            .unwrap_or_else(|| panic!("missing {label}"))
            .1;
        let mut pos = label_rect.center();
        if label == "Search & Results" {
            // Custom Settings headers paint their title as a label; egui's
            // collapsing interaction is the narrow disclosure control just
            // to its left.
            pos.x = label_rect.left() - 8.0;
        }
        for pressed in [true, false] {
            paste_settings_frame(
                ctx,
                app,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }

    fn settings_test_app(
        ctx: &egui::Context,
        directory: &tempfile::TempDir,
        mut settings: crate::settings::Settings,
    ) -> LauncherApp {
        use std::sync::{Arc, atomic::AtomicBool};

        settings.index_paths = Some(Vec::new());
        settings.plugin_dirs = Some(Vec::new());
        settings.enabled_plugins = Some(std::collections::HashSet::new());
        settings.paste_sequence.next_hotkey = None;
        settings.paste_sequence.cancel_hotkey = None;
        settings
            .save(directory.path().join("settings.json").to_str().unwrap())
            .unwrap();

        LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            crate::plugin::PluginManager::new_inert_for_test(),
            directory
                .path()
                .join("actions.json")
                .to_string_lossy()
                .into_owned(),
            directory
                .path()
                .join("settings.json")
                .to_string_lossy()
                .into_owned(),
            settings,
            None,
            None,
            Some(std::collections::HashSet::new()),
            Some(std::collections::HashMap::new()),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn settings_search_frame(
        ctx: &egui::Context,
        app: &mut LauncherApp,
        settings_owner_instance: Option<u64>,
        jump_mode_active: bool,
        events: Vec<egui::Event>,
        discard_first_pass: bool,
    ) -> usize {
        settings_search_frame_observing(
            ctx,
            app,
            settings_owner_instance,
            jump_mode_active,
            events,
            discard_first_pass,
            |_, _| (),
        )
        .0
    }

    fn settings_search_frame_observing<T>(
        ctx: &egui::Context,
        app: &mut LauncherApp,
        settings_owner_instance: Option<u64>,
        jump_mode_active: bool,
        events: Vec<egui::Event>,
        discard_first_pass: bool,
        mut observe_after_render: impl FnMut(&egui::Context, &LauncherApp) -> T,
    ) -> (usize, T) {
        if settings_owner_instance.is_some() {
            app.mark_visible_root_for_test();
        }
        let mut observation = None;
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 900.0),
                )),
                focused: true,
                events,
                ..Default::default()
            },
            |ui| {
                let first_ui_pass = ui.output(|output| output.num_completed_passes == 0);
                let mut editor = std::mem::take(&mut app.settings_editor);
                editor.ui(
                    ctx,
                    app,
                    None,
                    settings_owner_instance,
                    jump_mode_active,
                    first_ui_pass,
                );
                app.settings_editor = editor;
                observation = Some(observe_after_render(ctx, app));
                if discard_first_pass && first_ui_pass {
                    ui.ctx().request_discard("Settings search navigation test");
                }
            },
        );
        let passes = output.platform_output.num_completed_passes;
        output.drop_without_applying_deltas();
        (
            passes,
            observation.expect("the Settings frame renders at least one UI pass"),
        )
    }

    fn settings_search_frame_with_foreign_takeover(
        ctx: &egui::Context,
        app: &mut LauncherApp,
        settings_owner_instance: u64,
    ) {
        app.mark_visible_root_for_test();
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 900.0),
                )),
                focused: true,
                ..Default::default()
            },
            |ui| {
                let first_ui_pass = ui.output(|output| output.num_completed_passes == 0);
                let mut editor = std::mem::take(&mut app.settings_editor);
                editor.ui(
                    ctx,
                    app,
                    None,
                    Some(settings_owner_instance),
                    false,
                    first_ui_pass,
                );
                app.settings_editor = editor;
                egui::Window::new("Unrelated settings owner")
                    .show(ctx, |ui| ui.label("Protected owner"));
            },
        );
        output.drop_without_applying_deltas();
    }

    fn settle_settings_search_window(
        ctx: &egui::Context,
        app: &mut LauncherApp,
        settings_owner_instance: Option<u64>,
    ) {
        settings_search_frame(ctx, app, settings_owner_instance, false, Vec::new(), false);
        assert_eq!(
            app.settings_editor.test_settings_window_state,
            Some((false, true, true)),
            "the next Settings frame uses its visible, interactive window responses"
        );
    }

    fn click_settings_search_response(
        ctx: &egui::Context,
        app: &mut LauncherApp,
        settings_owner_instance: Option<u64>,
        rect: egui::Rect,
    ) {
        for pressed in [true, false] {
            settings_search_frame(
                ctx,
                app,
                settings_owner_instance,
                false,
                vec![
                    egui::Event::PointerMoved(rect.center()),
                    egui::Event::PointerButton {
                        pos: rect.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                false,
            );
        }
    }

    fn scroll_settings_search_to_top(
        ctx: &egui::Context,
        app: &mut LauncherApp,
        settings_owner_instance: Option<u64>,
    ) {
        for _ in 0..4 {
            let (_, query_rect) = app
                .settings_editor
                .test_search_query_response
                .expect("the Settings search has a current response");
            let clip_rect = app
                .settings_editor
                .test_search_query_clip_rect
                .expect("the Settings search has a current scroll viewport");
            if clip_rect.contains(query_rect.center()) {
                return;
            }

            let scroll_position = clip_rect.center();
            settings_search_frame(
                ctx,
                app,
                settings_owner_instance,
                false,
                vec![
                    egui::Event::PointerMoved(scroll_position),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, 10_000.0),
                        phase: egui::TouchPhase::Move,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                false,
            );
        }

        let (_, query_rect) = app
            .settings_editor
            .test_search_query_response
            .expect("the Settings search has a current response");
        let clip_rect = app
            .settings_editor
            .test_search_query_clip_rect
            .expect("the Settings search has a current scroll viewport");
        assert!(
            clip_rect.contains(query_rect.center()),
            "a real Settings scroll brings the current query response into the viewport: query={query_rect:?} viewport={clip_rect:?}"
        );
    }

    fn prepare_search_result(
        ctx: &egui::Context,
        app: &mut LauncherApp,
        settings_owner_instance: Option<u64>,
        control_id: &str,
        query: &str,
    ) -> egui::Id {
        app.settings_editor.settings_search_query = query.to_owned();
        app.settings_editor.settings_search_selected_id = Some(control_id.to_owned());
        settings_search_frame(ctx, app, settings_owner_instance, false, Vec::new(), false);
        assert!(
            app.settings_editor
                .test_search_result_responses
                .iter()
                .any(|(id, _, _)| id == control_id),
            "the requested setting is present in the actual rendered search results; {}; history={:?}",
            app.settings_editor
                .test_last_navigation_diagnostic
                .as_deref()
                .unwrap_or("no Settings navigation frame was recorded"),
            app.settings_editor.test_search_navigation_history
        );
        let (query_id, _) = app
            .settings_editor
            .test_search_query_response
            .expect("the actual Settings query response was rendered");
        ctx.memory_mut(|memory| memory.request_focus(query_id));
        query_id
    }

    fn settings_jump_frame(
        ctx: &egui::Context,
        app: &mut LauncherApp,
        jump: &mut crate::gui::jump_mode::ui::JumpModeUi,
        events: Vec<egui::Event>,
        focus_target: Option<crate::gui::jump_mode::TargetId>,
        first_ui_pass: bool,
    ) -> Vec<crate::gui::jump_mode::ui::RegisteredTarget> {
        use crate::gui::jump_mode::ui::NavigationOwner;
        use crate::gui::jump_mode::{SurfaceId, TargetGeneration};

        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 900.0),
            )),
            focused: true,
            events,
            ..Default::default()
        });
        jump.begin_frame();
        let owner = NavigationOwner::Settings {
            instance: jump.settings_instance(),
        };
        assert_eq!(jump.update_navigation_owner(ctx, owner, false), owner);
        if let Some(target) = focus_target {
            assert!(jump.queue_focus_intent(target, ctx.viewport_id(), jump.navigation_epoch()));
        }
        jump.begin_ui_pass();
        let scope = jump
            .scope_for_navigation_owner(
                SurfaceId::Settings {
                    instance: jump.settings_instance(),
                },
                TargetGeneration(jump.settings_instance()),
                "settings",
                ctx.viewport_id(),
                owner,
                false,
            )
            .expect("enabled Settings owner creates a collection scope");
        let mut editor = std::mem::take(&mut app.settings_editor);
        editor.ui(
            ctx,
            app,
            Some(scope),
            Some(jump.settings_instance()),
            jump.active_session().is_some(),
            first_ui_pass,
        );
        app.settings_editor = editor;
        jump.finish_ui_pass();
        let targets = jump
            .pass_registry()
            .expect("Settings pass is collected")
            .targets()
            .to_vec();
        let output = ctx.end_pass();
        assert!(jump.prepare_pass_for_navigation_owner(ctx, owner, false, true));
        jump.complete_frame_for_navigation_owner(ctx, owner, false, true);
        output.drop_without_applying_deltas();
        targets
    }

    fn settings_jump_discard_frame(
        ctx: &egui::Context,
        app: &mut LauncherApp,
        jump: &mut crate::gui::jump_mode::ui::JumpModeUi,
        events: Vec<egui::Event>,
    ) -> (
        Vec<crate::gui::jump_mode::ui::RegisteredTarget>,
        Vec<SettingsDiscardPassState>,
        usize,
    ) {
        use crate::gui::jump_mode::ui::NavigationOwner;
        use crate::gui::jump_mode::{SurfaceId, TargetGeneration};

        let owner = NavigationOwner::Settings {
            instance: jump.settings_instance(),
        };
        jump.begin_frame();
        assert_eq!(jump.update_navigation_owner(ctx, owner, false), owner);
        let mut state_after_pass = Vec::new();
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 900.0),
                )),
                focused: true,
                events,
                ..Default::default()
            },
            |ui| {
                let first_ui_pass = ui.output(|output| output.num_completed_passes == 0);
                jump.begin_ui_pass();
                let scope = jump
                    .scope_for_navigation_owner(
                        SurfaceId::Settings {
                            instance: jump.settings_instance(),
                        },
                        TargetGeneration(jump.settings_instance()),
                        "settings",
                        ctx.viewport_id(),
                        owner,
                        false,
                    )
                    .expect("enabled Settings owner creates a collection scope");
                let mut editor = std::mem::take(&mut app.settings_editor);
                editor.ui(
                    ctx,
                    app,
                    Some(scope),
                    Some(jump.settings_instance()),
                    jump.active_session().is_some(),
                    first_ui_pass,
                );
                app.settings_editor = editor;
                jump.finish_ui_pass();
                let hotkey_cursor = jump
                    .pass_registry()
                    .and_then(|registry| {
                        registry
                            .targets()
                            .iter()
                            .find(|target| target.metadata().label() == "Launcher hotkey")
                    })
                    .and_then(|target| egui::TextEdit::load_state(ctx, target.response_id()))
                    .and_then(|state| state.cursor.char_range().map(|range| range.primary.index.0));
                let paste_body_visible = jump.pass_registry().is_some_and(|registry| {
                    registry
                        .targets()
                        .iter()
                        .any(|target| target.metadata().label() == "Enable Next shortcut")
                });
                state_after_pass.push(SettingsDiscardPassState {
                    quit_hotkey_enabled: app.settings_editor.quit_hotkey_enabled,
                    hotkey: app.settings_editor.hotkey.clone(),
                    hotkey_cursor,
                    paste_body_visible,
                });
                if first_ui_pass {
                    ui.ctx()
                        .request_discard("Settings native-control state test");
                }
            },
        );
        let targets = jump
            .pass_registry()
            .expect("retained Settings pass is collected")
            .targets()
            .to_vec();
        assert!(jump.prepare_pass_for_navigation_owner(ctx, owner, false, true));
        jump.complete_frame_for_navigation_owner(ctx, owner, false, true);
        let passes = output.platform_output.num_completed_passes;
        output.drop_without_applying_deltas();
        (targets, state_after_pass, passes)
    }

    fn target_by_label<'a>(
        targets: &'a [crate::gui::jump_mode::ui::RegisteredTarget],
        label: &str,
    ) -> &'a crate::gui::jump_mode::ui::RegisteredTarget {
        targets
            .iter()
            .find(|target| target.metadata().label() == label)
            .unwrap_or_else(|| {
                let available = targets
                    .iter()
                    .map(|target| target.metadata().label())
                    .collect::<Vec<_>>()
                    .join(", ");
                panic!("missing Settings Jump target {label}; available: {available}")
            })
    }

    #[derive(Clone, Copy)]
    enum SettingsScrollDirection {
        Earlier,
        Later,
    }

    fn settings_scroll_until_target(
        ctx: &egui::Context,
        app: &mut LauncherApp,
        jump: &mut crate::gui::jump_mode::ui::JumpModeUi,
        mut targets: Vec<crate::gui::jump_mode::ui::RegisteredTarget>,
        wanted_label: &str,
        direction: SettingsScrollDirection,
    ) -> Vec<crate::gui::jump_mode::ui::RegisteredTarget> {
        for _ in 0..16 {
            if targets
                .iter()
                .any(|target| target.metadata().label() == wanted_label && target.eligible())
            {
                return targets;
            }
            let scroll_clip = app
                .settings_editor
                .test_search_query_clip_rect
                .expect("the current Settings search viewport is recorded");
            let delta_y = match direction {
                SettingsScrollDirection::Earlier => 180.0,
                SettingsScrollDirection::Later => -180.0,
            };
            targets = settings_jump_frame(
                ctx,
                app,
                jump,
                vec![
                    egui::Event::PointerMoved(scroll_clip.center()),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, delta_y),
                        phase: egui::TouchPhase::Move,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                None,
                true,
            );
        }
        if let Some(target) = targets
            .iter()
            .find(|target| target.metadata().label() == wanted_label)
        {
            assert!(
                target.eligible(),
                "Settings Jump target {wanted_label} did not become eligible after bounded scrolling"
            );
        } else {
            target_by_label(&targets, wanted_label);
        }
        targets
    }

    struct SettingsDiscardPassState {
        quit_hotkey_enabled: bool,
        hotkey: String,
        hotkey_cursor: Option<usize>,
        paste_body_visible: bool,
    }

    #[test]
    fn paste_settings_navigation_controls_and_cancel_preserve_committed_state() {
        use std::sync::{Arc, atomic::AtomicBool};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let mut settings = crate::settings::Settings::default();
        settings.enable_toasts = false;
        settings.dashboard.enabled = false;
        settings.paste_sequence.next_hotkey = None;
        settings.paste_sequence.cancel_hotkey = None;
        settings.save(path.to_str().unwrap()).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let ctx = egui::Context::default();
        ctx.global_style_mut(|style| style.animation_time = 0.0);
        let mut app = LauncherApp::new(
            &ctx,
            Arc::new(Vec::new()),
            0,
            crate::plugin::PluginManager::new_inert_for_test(),
            directory
                .path()
                .join("actions.json")
                .to_string_lossy()
                .into_owned(),
            path.to_string_lossy().into_owned(),
            settings.clone(),
            None,
            None,
            Some(std::collections::HashSet::new()),
            Some(std::collections::HashMap::new()),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        app.mark_visible_root_for_test();
        app.open_paste_sequence_settings_dialog();
        assert!(app.show_settings);
        assert_eq!(
            app.settings_editor
                .pending_settings_navigation
                .as_ref()
                .map(|pending| pending.control_id.as_str()),
            Some("paste.next.enabled")
        );
        app.settings_editor.paste_sequence.next_hotkey = Some("Ctrl+F9".into());
        app.settings_editor.paste_cancel_enabled = true;
        app.settings_editor.paste_sequence.cancel_hotkey = Some("Ctrl+F10".into());
        app.open_paste_sequence_settings_dialog();
        assert_eq!(
            app.settings_editor.paste_sequence.next_hotkey.as_deref(),
            Some("Ctrl+F9")
        );
        let mut next_response_id = None;
        for _ in 0..PENDING_SETTINGS_NAVIGATION_FRAME_BUDGET {
            paste_settings_frame(&ctx, &mut app, vec![]);
            if let Some((control_id, response_id)) =
                app.settings_editor.test_last_navigation_focus.as_ref()
                && control_id == "paste.next.enabled"
                && ctx.memory(|memory| memory.focused()) == Some(*response_id)
            {
                next_response_id = Some(*response_id);
                break;
            }
        }
        assert!(
            next_response_id.is_some(),
            "Paste Next navigation focuses its actual response within the bounded request lifetime; {}; history={:?}",
            app.settings_editor
                .test_last_navigation_diagnostic
                .as_deref()
                .unwrap_or("no Settings navigation frame was recorded"),
            app.settings_editor.test_search_navigation_history
        );
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            next_response_id,
            "the focused egui ID belongs to the actual Paste Next response"
        );
        assert!(
            app.settings_editor.pending_settings_navigation.is_none(),
            "the one-shot Paste Next request is consumed after actual focus handoff; {}; history={:?}",
            app.settings_editor
                .test_last_navigation_diagnostic
                .as_deref()
                .unwrap_or("no Settings navigation frame was recorded"),
            app.settings_editor.test_search_navigation_history
        );
        paste_settings_click(&ctx, &mut app, "Enable Cancel shortcut");
        assert!(!app.settings_editor.paste_cancel_enabled);
        assert!(
            app.settings_editor
                .to_settings(&settings)
                .paste_sequence
                .cancel_hotkey
                .is_none()
        );
        paste_settings_click(&ctx, &mut app, "Enable Cancel shortcut");
        assert_eq!(
            app.settings_editor
                .to_settings(&settings)
                .paste_sequence
                .cancel_hotkey
                .as_deref(),
            Some("Ctrl+F10")
        );
        paste_settings_click(&ctx, &mut app, "Escape cancels active playback");
        assert!(!app.settings_editor.paste_sequence.escape_cancel_enabled);
        paste_settings_click(
            &ctx,
            &mut app,
            "Keep Paste Sequences dialog visible during playback",
        );
        assert!(
            app.settings_editor
                .paste_sequence
                .keep_dialog_visible_during_playback
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(
            app.settings_editor.baseline_settings.paste_sequence,
            settings.paste_sequence
        );
        paste_settings_click(&ctx, &mut app, "Cancel");
        assert!(!app.show_settings);
        assert!(!app.settings_save_pending());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        app.open_paste_sequence_settings_dialog();
        assert_eq!(app.settings_editor.paste_sequence, settings.paste_sequence);
    }

    #[test]
    fn settings_search_navigation_consumes_enter_before_focus_in_both_jump_settings() {
        for jump_enabled in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let ctx = egui::Context::default();
            ctx.global_style_mut(|style| style.animation_time = 0.0);
            let mut settings = crate::settings::Settings::default();
            settings.experimental_jump_mode_enabled = jump_enabled;
            let mut app = settings_test_app(&ctx, &directory, settings);
            app.show_settings = true;
            app.settings_editor.paste_next_enabled = false;
            app.settings_editor.fuzzy_weight = 1.75;
            let committed_bytes = std::fs::read(directory.path().join("settings.json")).unwrap();
            let owner = app.settings_navigation_owner_instance();

            settings_search_frame(&ctx, &mut app, Some(owner), false, Vec::new(), false);
            let (query_id, _) = app
                .settings_editor
                .test_search_query_response
                .expect("the Settings search is a real TextEdit response");
            ctx.memory_mut(|memory| memory.request_focus(query_id));
            settings_search_frame(
                &ctx,
                &mut app,
                Some(owner),
                false,
                vec![egui::Event::Text("Next shortcut".into())],
                false,
            );
            assert_eq!(app.settings_editor.settings_search_query, "Next shortcut");
            assert!(
                app.settings_editor
                    .settings_search_match_ids
                    .iter()
                    .any(|id| id == "paste.next.enabled")
            );
            app.settings_editor.settings_search_selected_id = Some("paste.next.hotkey".into());
            ctx.memory_mut(|memory| memory.request_focus(query_id));

            let enter = egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            };
            assert_eq!(
                settings_search_frame(&ctx, &mut app, Some(owner), false, vec![enter], true),
                2,
                "Jump setting {jump_enabled}: Enter navigation survives an egui discarded pass"
            );
            let mut reached_target = app
                .settings_editor
                .test_last_navigation_focus
                .as_ref()
                .is_some_and(|(id, _)| id == "paste.next.enabled");
            for _ in 0..8 {
                if reached_target {
                    break;
                }
                settings_search_frame(&ctx, &mut app, Some(owner), false, Vec::new(), false);
                reached_target = app
                    .settings_editor
                    .test_last_navigation_focus
                    .as_ref()
                    .is_some_and(|(id, _)| id == "paste.next.enabled");
            }
            assert!(
                reached_target,
                "Jump setting {jump_enabled}: the typed result focuses its actual checkbox response; {}; history={:?}",
                app.settings_editor
                    .test_last_navigation_diagnostic
                    .as_deref()
                    .unwrap_or("no Settings navigation frame was recorded"),
                app.settings_editor.test_search_navigation_history
            );
            assert!(
                app.settings_editor
                    .test_last_navigation_focus
                    .as_ref()
                    .is_some_and(|(_, response_id)| {
                        ctx.memory(|memory| memory.focused()) == Some(*response_id)
                    }),
                "Jump setting {jump_enabled}: memory focus matches the actual destination response"
            );
            assert!(
                app.settings_editor
                    .settings_navigation_highlight
                    .as_ref()
                    .is_some_and(|highlight| {
                        highlight.control_id == "paste.next.enabled"
                            && matches!(
                                &highlight.delivery,
                                SettingsNavigationDelivery::ConditionalPrerequisite {
                                    requested_control_id,
                                } if requested_control_id == "paste.next.hotkey"
                            )
                    })
            );
            let focused_response_id = app
                .settings_editor
                .test_last_navigation_focus
                .as_ref()
                .expect("the current checkbox focus receipt is recorded")
                .1;
            settings_search_frame(&ctx, &mut app, Some(owner), false, Vec::new(), false);
            assert_eq!(
                app.settings_editor
                    .test_last_navigation_highlight
                    .as_ref()
                    .map(|(id, response_id)| (id.as_str(), *response_id)),
                Some(("paste.next.enabled", focused_response_id)),
                "Jump setting {jump_enabled}: a later pass outlines the same actual response"
            );
            assert_eq!(
                ctx.memory(|memory| memory.focused()),
                Some(focused_response_id),
                "Jump setting {jump_enabled}: highlight observation does not redirect focus"
            );
            assert!(
                app.settings_editor
                    .test_last_navigation_diagnostic
                    .as_ref()
                    .is_some_and(|diagnostic| diagnostic.contains("focus_requested=false"))
            );
            assert!(
                app.settings_editor
                    .settings_navigation_highlight
                    .as_ref()
                    .is_some_and(|highlight| highlight.control_id == "paste.next.enabled")
            );
            assert!(
                !app.settings_editor.paste_next_enabled,
                "Jump setting {jump_enabled}: search Enter cannot also toggle the newly focused checkbox"
            );
            assert_eq!(
                app.settings_editor.fuzzy_weight, 1.75,
                "Jump setting {jump_enabled}: unrelated dirty editor state remains intact"
            );
            assert_eq!(
                std::fs::read(directory.path().join("settings.json")).unwrap(),
                committed_bytes,
                "Jump setting {jump_enabled}: search navigation does not save the editor"
            );

            scroll_settings_search_to_top(&ctx, &mut app, Some(owner));
            let (query_id, query_rect) = app
                .settings_editor
                .test_search_query_response
                .expect("the active search remains an actual response");
            let query_clip_rect = app
                .settings_editor
                .test_search_query_clip_rect
                .expect("the current Settings search viewport is recorded");
            assert!(query_clip_rect.contains(query_rect.center()));
            click_settings_search_response(&ctx, &mut app, Some(owner), query_rect);
            assert_eq!(
                ctx.memory(|memory| memory.focused()),
                Some(query_id),
                "Jump setting {jump_enabled}: a later search click takes focus from the destination; {}",
                app.settings_editor
                    .test_last_navigation_diagnostic
                    .as_deref()
                    .unwrap_or("no Settings navigation frame was recorded")
            );
            assert!(app.settings_editor.pending_settings_navigation.is_none());
            settings_search_frame(&ctx, &mut app, Some(owner), false, Vec::new(), false);
            assert_eq!(
                ctx.memory(|memory| memory.focused()),
                Some(query_id),
                "Jump setting {jump_enabled}: a steady frame does not rearm the old focus request"
            );
        }
    }

    #[test]
    fn settings_search_navigation_user_focus_supersedes_pending_destination() {
        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        ctx.global_style_mut(|style| style.animation_time = 0.0);
        let mut app = settings_test_app(&ctx, &directory, crate::settings::Settings::default());
        app.show_settings = true;
        let owner = app.settings_navigation_owner_instance();
        settings_search_frame(&ctx, &mut app, Some(owner), false, Vec::new(), false);
        settle_settings_search_window(&ctx, &mut app, Some(owner));
        let (query_id, query_rect) = app
            .settings_editor
            .test_search_query_response
            .expect("the user can take focus through the real search response");
        let query_clip_rect = app
            .settings_editor
            .test_search_query_clip_rect
            .expect("the current Settings search viewport is recorded");
        assert!(
            query_clip_rect.contains(query_rect.center()),
            "the takeover click starts at the visible current query response"
        );
        let plugins = &app.plugins;
        app.settings_editor
            .request_control_navigation("paste.next.enabled", Some(owner), plugins);
        assert!(app.settings_editor.pending_settings_navigation.is_some());

        let (current_query_id, current_query_rect) = app
            .settings_editor
            .test_search_query_response
            .expect("the Settings search response remains current before click");
        assert_eq!(current_query_id, query_id);
        assert_eq!(current_query_rect, query_rect);
        click_settings_search_response(&ctx, &mut app, Some(owner), query_rect);
        assert!(
            app.settings_editor.pending_settings_navigation.is_none(),
            "the search click retires the pending request before its later response is visited"
        );
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(query_id),
            "the deliberate query click establishes its real response focus; {}",
            app.settings_editor
                .test_last_navigation_diagnostic
                .as_deref()
                .unwrap_or("no Settings navigation frame was recorded")
        );
        settings_search_frame(&ctx, &mut app, Some(owner), false, Vec::new(), false);
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(query_id),
            "a later steady pass does not reclaim user focus"
        );
    }

    #[test]
    fn settings_search_navigation_uses_distinct_slider_and_numeric_responses() {
        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        ctx.global_style_mut(|style| style.animation_time = 0.0);
        let mut app = settings_test_app(&ctx, &directory, crate::settings::Settings::default());
        app.show_settings = true;
        let owner = app.settings_navigation_owner_instance();
        let initial_value = app.settings_editor.fuzzy_weight;
        let mut actual_response_ids = Vec::new();

        for (control_id, query, edit_key) in [
            ("search.fuzzy_weight", "Fuzzy weight", egui::Key::ArrowRight),
            (
                "search.fuzzy_weight/value",
                "Fuzzy weight value",
                egui::Key::ArrowUp,
            ),
        ] {
            let value_before = app.settings_editor.fuzzy_weight;
            prepare_search_result(&ctx, &mut app, Some(owner), control_id, query);
            settings_search_frame(
                &ctx,
                &mut app,
                Some(owner),
                false,
                vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                false,
            );
            let mut focused_exact_control = false;
            for _ in 0..8 {
                if let Some((_, response_id)) = app
                    .settings_editor
                    .test_last_navigation_focus
                    .as_ref()
                    .filter(|(id, _)| id == control_id)
                {
                    let response_id = *response_id;
                    assert_eq!(
                        ctx.memory(|memory| memory.focused()),
                        Some(response_id),
                        "{control_id} memory focus matches its own response"
                    );
                    actual_response_ids.push(response_id);
                    focused_exact_control = true;
                    assert_eq!(app.settings_editor.fuzzy_weight, value_before);
                    settings_search_frame(
                        &ctx,
                        &mut app,
                        Some(owner),
                        false,
                        vec![egui::Event::Key {
                            key: edit_key,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        }],
                        false,
                    );
                    assert!(app.settings_editor.fuzzy_weight > value_before);
                    assert!(app.settings_editor.fuzzy_weight <= 5.0);
                    let focused_after_edit = ctx
                        .memory(|memory| memory.focused())
                        .expect("the deliberately edited Settings control retains native focus");
                    let (slider_id, numeric_id) = app
                        .settings_editor
                        .test_fuzzy_weight_response_ids
                        .expect("the live Fuzzy weight slider and numeric responses are recorded");
                    assert_ne!(
                        slider_id, numeric_id,
                        "the live slider and numeric editor retain distinct response identities"
                    );
                    let expected_focus = if control_id == "search.fuzzy_weight" {
                        slider_id
                    } else {
                        numeric_id
                    };
                    assert_eq!(
                        focused_after_edit, expected_focus,
                        "the first native arrow stays on the exact {control_id} response"
                    );
                    settings_search_frame(
                        &ctx,
                        &mut app,
                        Some(owner),
                        false,
                        vec![egui::Event::Key {
                            key: edit_key,
                            physical_key: None,
                            pressed: false,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        }],
                        false,
                    );
                    break;
                }
                settings_search_frame(&ctx, &mut app, Some(owner), false, Vec::new(), false);
            }
            assert!(
                focused_exact_control,
                "search result {control_id} requests its own actual response focus; {}; history={:?}",
                app.settings_editor
                    .test_last_navigation_diagnostic
                    .as_deref()
                    .unwrap_or("no Settings navigation frame was recorded"),
                app.settings_editor.test_search_navigation_history
            );
        }
        assert_eq!(actual_response_ids.len(), 2);
        assert_ne!(
            actual_response_ids[0], actual_response_ids[1],
            "slider and numeric value are distinct focused responses"
        );
        assert!(app.settings_editor.fuzzy_weight > initial_value);
    }

    #[test]
    fn settings_search_navigation_pages_all_matches_and_activates_only_visible_selection() {
        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        ctx.global_style_mut(|style| style.animation_time = 0.0);
        let mut app = settings_test_app(&ctx, &directory, crate::settings::Settings::default());
        app.show_settings = true;
        let owner = app.settings_navigation_owner_instance();
        settings_search_frame(&ctx, &mut app, Some(owner), false, Vec::new(), false);
        settle_settings_search_window(&ctx, &mut app, Some(owner));
        app.settings_editor.settings_search_query = "search".into();
        settings_search_frame(&ctx, &mut app, Some(owner), false, Vec::new(), false);
        let all_matches = app.settings_editor.settings_search_match_ids.clone();
        assert!(all_matches.len() > MAX_VISIBLE_SETTINGS_SEARCH_RESULTS);
        assert_eq!(app.settings_editor.settings_search_page, 0);
        assert_eq!(
            app.settings_editor.test_search_result_responses.len(),
            MAX_VISIBLE_SETTINGS_SEARCH_RESULTS
        );

        let next_rect = app
            .settings_editor
            .test_search_page_responses
            .iter()
            .find(|(id, _, _)| id == "next")
            .map(|(_, _, rect)| *rect)
            .expect("the bounded search results expose a real next-page button");
        click_settings_search_response(&ctx, &mut app, Some(owner), next_rect);
        assert_eq!(app.settings_editor.settings_search_page, 1);
        let page_count = all_matches
            .len()
            .div_ceil(MAX_VISIBLE_SETTINGS_SEARCH_RESULTS);
        let mut available_later_match = None;
        for expected_page in 1..page_count {
            assert_eq!(app.settings_editor.settings_search_page, expected_page);
            let page_start = expected_page * MAX_VISIBLE_SETTINGS_SEARCH_RESULTS;
            let expected_page_ids = all_matches
                .iter()
                .skip(page_start)
                .take(MAX_VISIBLE_SETTINGS_SEARCH_RESULTS)
                .cloned()
                .collect::<Vec<_>>();
            let rendered_page_ids = app
                .settings_editor
                .test_search_result_responses
                .iter()
                .map(|(id, _, _)| id.clone())
                .collect::<Vec<_>>();
            assert_eq!(
                rendered_page_ids, expected_page_ids,
                "page {expected_page} responses match exactly their reachable result slice"
            );

            available_later_match = rendered_page_ids.iter().find_map(|id| {
                app.settings_editor
                    .search_catalog
                    .entry(id)
                    .is_some_and(|entry| {
                        matches!(&entry.availability, SettingsAvailability::Available)
                            && matches!(&entry.section, SettingsSection::Main(_))
                            && entry.owner == SettingsOwner::Application
                    })
                    .then(|| id.clone())
            });
            if available_later_match.is_some() {
                break;
            }

            assert!(
                expected_page + 1 < page_count,
                "all later pages were reached but none contained an available result"
            );
            let next_rect = app
                .settings_editor
                .test_search_page_responses
                .iter()
                .find(|(id, _, _)| id == "next")
                .map(|(_, _, rect)| *rect)
                .expect("an intermediate page exposes its actual Next response");
            click_settings_search_response(&ctx, &mut app, Some(owner), next_rect);
            assert_eq!(
                app.settings_editor.settings_search_page,
                expected_page + 1,
                "the actual Next response advances to the following reachable page"
            );
        }
        let available_later_match = available_later_match
            .expect("a later rendered page contains an available real settings control");
        let selected_page = all_matches
            .iter()
            .position(|id| id == &available_later_match)
            .expect("the selected setting belongs to the full match set")
            / MAX_VISIBLE_SETTINGS_SEARCH_RESULTS;
        assert_eq!(app.settings_editor.settings_search_page, selected_page);
        let response_id = app
            .settings_editor
            .test_search_result_responses
            .iter()
            .find(|(id, _, _)| id == &available_later_match)
            .map(|(_, response_id, _)| *response_id)
            .expect("the later-page control has an actual response");
        app.settings_editor.settings_search_selected_id = Some(available_later_match.clone());
        ctx.memory_mut(|memory| memory.request_focus(response_id));
        settings_search_frame(&ctx, &mut app, Some(owner), false, Vec::new(), false);
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(response_id));
        assert!(
            app.settings_editor
                .test_search_result_responses
                .iter()
                .any(|(id, _, _)| id == &available_later_match)
        );
        settings_search_frame(
            &ctx,
            &mut app,
            Some(owner),
            false,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            false,
        );
        assert_eq!(
            app.settings_editor
                .pending_settings_navigation
                .as_ref()
                .map(|pending| pending.control_id.as_str()),
            Some(available_later_match.as_str()),
            "Enter activates the current visible response rather than an off-page selection"
        );
        let pending = app
            .settings_editor
            .pending_settings_navigation
            .as_ref()
            .expect("the exact Main/Application destination is queued after the result pass");
        assert_eq!(pending.stage, SettingsNavigationStage::MainControl);
        assert_eq!(pending.owner, SettingsOwner::Application);
        let mut reached_destination = app
            .settings_editor
            .test_last_navigation_focus
            .as_ref()
            .is_some_and(|(id, _)| id == &available_later_match);
        for _ in 0..PENDING_SETTINGS_NAVIGATION_FRAME_BUDGET {
            if reached_destination {
                break;
            }
            settings_search_frame(&ctx, &mut app, Some(owner), false, Vec::new(), false);
            reached_destination = app
                .settings_editor
                .test_last_navigation_focus
                .as_ref()
                .is_some_and(|(id, _)| id == &available_later_match);
        }
        assert!(
            reached_destination,
            "Enter eventually focuses the actual reachable Main/Application response; {}; history={:?}",
            app.settings_editor
                .test_last_navigation_diagnostic
                .as_deref()
                .unwrap_or("no Settings navigation frame was recorded"),
            app.settings_editor.test_search_navigation_history
        );
        assert!(
            app.settings_editor
                .test_last_navigation_focus
                .as_ref()
                .is_some_and(|(_, response_id)| {
                    ctx.memory(|memory| memory.focused()) == Some(*response_id)
                }),
            "the current egui focused ID matches the destination's actual response"
        );
        assert!(
            app.settings_editor.pending_settings_navigation.is_none(),
            "the one-shot destination request is consumed after actual focus handoff"
        );
        assert_eq!(app.settings_editor.settings_search_query, "search");
    }

    #[test]
    fn settings_search_navigation_requires_current_owner_and_hands_off_dedicated_settings() {
        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        ctx.global_style_mut(|style| style.animation_time = 0.0);
        let mut app = settings_test_app(&ctx, &directory, crate::settings::Settings::default());
        app.show_settings = true;
        let owner = app.settings_navigation_owner_instance();
        let original_toast_duration = app.settings_editor.toast_duration;

        prepare_search_result(
            &ctx,
            &mut app,
            None,
            "feedback.toasts",
            "notification popups",
        );
        let (query_id, _) = app
            .settings_editor
            .test_search_query_response
            .expect("the query remains a real focused response");
        ctx.memory_mut(|memory| memory.request_focus(query_id));
        settings_search_frame(
            &ctx,
            &mut app,
            None,
            false,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            false,
        );
        assert!(
            app.settings_editor.pending_settings_navigation.is_none(),
            "a stale frame without the exact Settings owner cannot navigate"
        );
        assert_eq!(app.settings_editor.toast_duration, original_toast_duration);

        settings_search_frame(
            &ctx,
            &mut app,
            Some(owner),
            true,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            false,
        );
        assert_eq!(
            app.settings_editor.settings_search_query, "notification popups",
            "an active Jump session keeps Escape priority over search dismissal"
        );
        settings_search_frame(
            &ctx,
            &mut app,
            Some(owner),
            true,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            false,
        );
        assert!(
            app.settings_editor.pending_settings_navigation.is_none(),
            "an active Jump session keeps Enter priority over search activation"
        );

        // The dedicated Theme owner is opened through its established path;
        // search retains a typed request for its later exact response binding.
        prepare_search_result(
            &ctx,
            &mut app,
            Some(owner),
            "theme.typography_scale",
            "font size",
        );
        ctx.memory_mut(|memory| memory.request_focus(query_id));
        settings_search_frame(
            &ctx,
            &mut app,
            Some(owner),
            false,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            false,
        );
        assert!(app.is_theme_settings_dialog_open());
        let pending = app
            .settings_editor
            .pending_settings_navigation
            .as_ref()
            .expect("Theme navigation waits for its later actual owner response");
        assert_eq!(pending.control_id, "theme.typography_scale");
        assert_eq!(pending.section, SettingsSection::Theme);
        assert_eq!(pending.settings_instance, Some(owner));
        assert_eq!(pending.stage, SettingsNavigationStage::DedicatedOwner);

        let plugin_revision = app.plugins.settings_catalog_revision();
        let snapshot = crate::plugin::PluginCatalogSnapshot {
            revision: plugin_revision,
            plugins: vec![crate::plugin::PluginCatalogEntry {
                name: "mouse_gestures".into(),
                origin: crate::plugin::PluginCatalogOrigin::Builtin,
                epoch: 7,
                has_settings: true,
                enabled: true,
                capabilities: Vec::new(),
            }],
        };
        app.settings_editor.search_catalog =
            super::super::catalog::SettingsCatalog::from_plugin_snapshot(&snapshot);
        app.settings_editor.catalog_plugin_revision = Some(plugin_revision);
        let query_id = prepare_search_result(
            &ctx,
            &mut app,
            Some(owner),
            "mouse_gestures.trail_interval",
            "gesture trail refresh interval",
        );
        ctx.memory_mut(|memory| memory.request_focus(query_id));
        settings_search_frame(
            &ctx,
            &mut app,
            Some(owner),
            false,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            false,
        );
        assert!(app.is_mouse_gesture_settings_dialog_open());
        let pending = app
            .settings_editor
            .pending_settings_navigation
            .as_ref()
            .expect("gesture interval navigation waits for its later exact owner response");
        assert_eq!(pending.control_id, "mouse_gestures.trail_interval");
        assert_eq!(
            pending.owner,
            SettingsOwner::Plugin {
                name: "mouse_gestures".into(),
                origin: crate::plugin::PluginCatalogOrigin::Builtin,
                epoch: Some(7),
            }
        );
        assert_eq!(pending.section, SettingsSection::MouseGestures);
        assert_eq!(pending.settings_instance, Some(owner));
        assert_eq!(pending.stage, SettingsNavigationStage::DedicatedOwner);
    }

    #[test]
    fn settings_search_cross_owner_inline_notes_focus_and_query_cancellation_preserve_editor_state()
    {
        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        ctx.global_style_mut(|style| style.animation_time = 0.0);
        let mut app = settings_test_app(&ctx, &directory, crate::settings::Settings::default());
        app.show_settings = true;
        app.settings_editor.toast_duration = 9.25;
        app.settings_editor.note_max_outline_depth = 9;
        let main_draft = app.settings_editor.toast_duration;
        let committed = std::fs::read(directory.path().join("settings.json")).unwrap();
        let owner = app.settings_navigation_owner_instance();

        prepare_search_result(
            &ctx,
            &mut app,
            Some(owner),
            "plugin.notes.external_open",
            "open externally",
        );
        let key_event = |key, pressed, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        };
        settings_search_frame(
            &ctx,
            &mut app,
            Some(owner),
            false,
            vec![key_event(egui::Key::Enter, true, egui::Modifiers::NONE)],
            false,
        );
        assert!(app.settings_editor.pending_settings_navigation.is_some());
        settings_search_frame(
            &ctx,
            &mut app,
            Some(owner),
            false,
            vec![key_event(egui::Key::Enter, false, egui::Modifiers::NONE)],
            false,
        );
        let mut remaining_test_frames = 8;
        let focused_control = loop {
            if let Some(focused) = app
                .settings_editor
                .test_last_navigation_focus
                .as_ref()
                .filter(|(id, response_id)| {
                    id == "plugin.notes.external_open"
                        && ctx.memory(|memory| memory.focused()) == Some(*response_id)
                })
                .cloned()
            {
                break Some(focused);
            }

            let Some(remaining_frames) = app
                .settings_editor
                .pending_settings_navigation
                .as_ref()
                .filter(|pending| pending.control_id == "plugin.notes.external_open")
                .map(|pending| pending.frames_remaining)
            else {
                break None;
            };
            if remaining_frames == 0 || remaining_test_frames == 0 {
                break None;
            }

            remaining_test_frames -= 1;
            settings_search_frame(&ctx, &mut app, Some(owner), false, Vec::new(), false);
        };
        let focused_control = focused_control.unwrap_or_else(|| {
            panic!(
                "the bounded inline Notes handoff either focuses its current response or retires; diagnostic={:?} history={:?}",
                app.settings_editor.test_last_navigation_diagnostic,
                app.settings_editor.test_search_navigation_history,
            )
        });
        assert_eq!(focused_control.0, "plugin.notes.external_open");
        assert!(app.settings_editor.pending_settings_navigation.is_none());
        assert!(
            !app.settings_editor.plugin_settings.contains_key("note"),
            "passive Notes rendering and navigation leave an absent raw plugin value absent"
        );
        assert_eq!(
            app.settings_editor.note_max_outline_depth, 9,
            "merely rendering the numeric response does not clamp an existing draft"
        );
        assert_eq!(app.settings_editor.toast_duration, main_draft);
        assert_eq!(
            std::fs::read(directory.path().join("settings.json")).unwrap(),
            committed,
            "response focus does not save or change Main Settings"
        );
        #[cfg(feature = "qa-framebuffer-capture")]
        let (_, painted) = settings_search_frame_observing(
            &ctx,
            &mut app,
            Some(owner),
            false,
            Vec::new(),
            false,
            |ctx, app| {
                app.settings_editor
                    .settings_navigation_qa_paint_for_current_pass(ctx)
                    .cloned()
            },
        );
        #[cfg(not(feature = "qa-framebuffer-capture"))]
        settings_search_frame(&ctx, &mut app, Some(owner), false, Vec::new(), false);
        assert_eq!(
            app.settings_editor
                .test_last_navigation_highlight
                .as_ref()
                .map(|(control_id, response_id)| (control_id.as_str(), *response_id)),
            Some(("plugin.notes.external_open", focused_control.1)),
            "the later Notes pass highlights its current real response"
        );
        #[cfg(feature = "qa-framebuffer-capture")]
        {
            let painted =
                painted.expect("QA evidence is retained only for the current real paint pass");
            assert_eq!(painted.control_id, "plugin.notes.external_open");
            assert_eq!(painted.response_id, focused_control.1);
            assert_eq!(painted.owner_instance, Some(owner));
            assert!(
                app.settings_editor
                    .settings_navigation_qa_paint_for_current_pass(&ctx)
                    .is_none(),
                "the same receipt is rejected once its egui pass has ended"
            );
        }
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(focused_control.1)
        );
        assert!(
            app.settings_editor
                .test_last_navigation_diagnostic
                .as_ref()
                .is_some_and(|diagnostic| diagnostic.contains("focus_requested=false"))
        );

        // A real foreground owner takes over for two frames. The second frame
        // observes that layer as the previous-frame top owner and must retire
        // the persistent Main/Notes highlight, even though its one-shot focus
        // request has already completed. Returning to Settings must not revive
        // the old highlight or deliver another focus request.
        settings_search_frame_with_foreign_takeover(&ctx, &mut app, owner);
        settings_search_frame_with_foreign_takeover(&ctx, &mut app, owner);
        assert!(app.settings_editor.settings_navigation_highlight.is_none());
        #[cfg(feature = "qa-framebuffer-capture")]
        assert!(
            app.settings_editor
                .settings_navigation_qa_paint_for_current_pass(&ctx)
                .is_none(),
            "a protected owner takeover invalidates stale geometry evidence"
        );
        settings_search_frame(&ctx, &mut app, Some(owner), false, Vec::new(), false);
        settings_search_frame(&ctx, &mut app, Some(owner), false, Vec::new(), false);
        assert!(app.settings_editor.settings_navigation_highlight.is_none());
        assert!(
            !app.settings_editor
                .test_last_navigation_focus
                .as_ref()
                .is_some_and(|(control_id, _)| control_id == "plugin.notes.external_open")
        );

        // A second request is pending at the start of this frame. Typing into
        // the already-focused query invalidates that copied observer before
        // the later inline Notes response is rendered.
        let query_id = prepare_search_result(
            &ctx,
            &mut app,
            Some(owner),
            "plugin.notes.external_open",
            "open externally",
        );
        let windows_select_all = egui::Modifiers::CTRL | egui::Modifiers::COMMAND;
        settings_search_frame(
            &ctx,
            &mut app,
            Some(owner),
            false,
            vec![key_event(egui::Key::Enter, true, egui::Modifiers::NONE)],
            false,
        );
        assert!(app.settings_editor.pending_settings_navigation.is_some());
        ctx.memory_mut(|memory| memory.request_focus(query_id));
        settings_search_frame(
            &ctx,
            &mut app,
            Some(owner),
            false,
            vec![
                key_event(egui::Key::Enter, false, egui::Modifiers::NONE),
                key_event(egui::Key::A, true, windows_select_all),
                egui::Event::Text("notes markdown".into()),
                key_event(egui::Key::A, false, windows_select_all),
            ],
            false,
        );
        assert_eq!(app.settings_editor.settings_search_query, "notes markdown");
        assert!(app.settings_editor.pending_settings_navigation.is_none());
        assert!(app.settings_editor.settings_navigation_highlight.is_none());
        assert!(
            !app.settings_editor
                .test_last_navigation_focus
                .as_ref()
                .is_some_and(|(id, _)| id == "plugin.notes.external_open")
        );

        let mut notes_response = app
            .settings_editor
            .test_notes_external_open_response
            .expect("the inline Notes ComboBox response is observed independently of navigation");
        assert!(
            !notes_response
                .rect
                .intersect(notes_response.clip_rect)
                .is_positive(),
            "the focused query leaves the Notes response outside the current scroll viewport"
        );
        let mut notes_received_focus = notes_response.has_focus
            || ctx.memory(|memory| memory.focused()) == Some(notes_response.id);
        let mut scrolled_notes_into_view = false;
        for _ in 0..8 {
            if notes_response
                .rect
                .intersect(notes_response.clip_rect)
                .is_positive()
            {
                break;
            }
            scrolled_notes_into_view = true;
            let scroll_pointer = notes_response.clip_rect.center();
            settings_search_frame(
                &ctx,
                &mut app,
                Some(owner),
                false,
                vec![
                    egui::Event::PointerMoved(scroll_pointer),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, -10_000.0),
                        phase: egui::TouchPhase::Move,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                false,
            );
            assert!(app.settings_editor.pending_settings_navigation.is_none());
            notes_response = app
                .settings_editor
                .test_notes_external_open_response
                .expect("Notes keeps rendering its real ComboBox while scrolling");
            notes_received_focus |= notes_response.has_focus
                || ctx.memory(|memory| memory.focused()) == Some(notes_response.id);
        }
        assert!(
            scrolled_notes_into_view,
            "the test used a real wheel event to reveal the Notes control after cancellation"
        );
        assert!(
            notes_response
                .rect
                .intersect(notes_response.clip_rect)
                .is_positive(),
            "a real wheel scroll brings the independently observed Notes response into its current viewport: {notes_response:?}"
        );
        assert!(notes_response.visible);
        assert!(notes_response.ui_enabled && notes_response.response_enabled);
        assert!(notes_response.interaction_allowed);
        for _ in 0..3 {
            settings_search_frame(&ctx, &mut app, Some(owner), false, Vec::new(), false);
            assert!(app.settings_editor.pending_settings_navigation.is_none());
            let current_notes_response = app
                .settings_editor
                .test_notes_external_open_response
                .expect("Notes remains observable on later frames");
            notes_received_focus |= current_notes_response.has_focus
                || ctx.memory(|memory| memory.focused()) == Some(current_notes_response.id);
            notes_response = current_notes_response;
        }
        assert!(
            !notes_received_focus,
            "the canceled request never gives focus to the Notes response; latest={notes_response:?}"
        );

        app.settings_editor.plugin_settings.insert(
            "note".into(),
            serde_json::json!({"external_open": "Neither", "preserved_note_key": "sentinel"}),
        );
        settings_search_frame(&ctx, &mut app, Some(owner), false, Vec::new(), false);
        notes_response = app
            .settings_editor
            .test_notes_external_open_response
            .expect("the actual Notes ComboBox remains rendered for a deliberate edit");
        assert!(notes_response.response_enabled && notes_response.visible);
        click_settings_search_response(&ctx, &mut app, Some(owner), notes_response.rect);
        let popup_labels = paste_settings_frame(&ctx, &mut app, Vec::new());
        assert!(
            popup_labels.iter().any(|(label, _)| label == "Notepad"),
            "the explicit Notes popup offers the real Notepad option"
        );
        paste_settings_click(&ctx, &mut app, "Notepad");
        let note_settings = app
            .settings_editor
            .plugin_settings
            .get("note")
            .expect("the explicit combo choice creates or patches its owned raw object");
        assert_eq!(note_settings["external_open"], "Notepad");
        assert_eq!(
            note_settings["preserved_note_key"], "sentinel",
            "an intentional field edit retains unrelated raw object keys"
        );
        assert_eq!(app.settings_editor.toast_duration, main_draft);
        assert_eq!(
            std::fs::read(directory.path().join("settings.json")).unwrap(),
            committed
        );
    }

    #[test]
    fn settings_search_delivery_preserves_malformed_notes_value() {
        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        ctx.global_style_mut(|style| style.animation_time = 0.0);
        let mut app = settings_test_app(&ctx, &directory, crate::settings::Settings::default());
        app.show_settings = true;
        app.settings_editor.plugin_settings.insert(
            "note".into(),
            serde_json::Value::String("malformed-note-sentinel".into()),
        );
        let owner = app.settings_navigation_owner_instance();
        prepare_search_result(
            &ctx,
            &mut app,
            Some(owner),
            "plugin.notes.external_open",
            "open externally",
        );
        let key_event = |pressed| egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        settings_search_frame(
            &ctx,
            &mut app,
            Some(owner),
            false,
            vec![key_event(true)],
            false,
        );
        settings_search_frame(
            &ctx,
            &mut app,
            Some(owner),
            false,
            vec![key_event(false)],
            false,
        );
        for _ in 0..8 {
            if app
                .settings_editor
                .test_notes_external_open_response
                .is_some()
            {
                break;
            }
            settings_search_frame(&ctx, &mut app, Some(owner), false, Vec::new(), false);
        }
        let raw_value = app
            .settings_editor
            .plugin_settings
            .get("note")
            .expect("the malformed stored value remains present");
        assert_eq!(raw_value, "malformed-note-sentinel");
        let response = app
            .settings_editor
            .test_notes_external_open_response
            .expect("the Notes owner reports the malformed option through its real response");
        assert!(response.visible);
        assert!(!response.response_enabled);
    }

    #[test]
    fn jump_mode_focuses_real_settings_controls_and_later_egui_keys_activate_them() {
        use crate::gui::jump_mode::ui::{JumpModeUi, NavigationOwner};

        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        ctx.global_style_mut(|style| style.animation_time = 0.0);
        let mut settings = crate::settings::Settings::default();
        settings.experimental_jump_mode_enabled = true;
        let mut app = settings_test_app(&ctx, &directory, settings);
        app.show_settings = true;
        for _ in 0..3 {
            paste_settings_frame(&ctx, &mut app, Vec::new());
        }
        paste_settings_click(&ctx, &mut app, "Expand all");

        let mut jump = JumpModeUi::new(true);
        let settings_owner = NavigationOwner::Settings { instance: 1 };
        let setup_output = ctx.run_ui(
            egui::RawInput {
                focused: true,
                ..Default::default()
            },
            |ui| {
                jump.begin_frame();
                assert!(jump.observe_settings_visibility(true));
                assert_eq!(
                    jump.update_navigation_owner(&ctx, settings_owner, true),
                    NavigationOwner::Blocked,
                    "the newly opened Settings owner waits for retained window ordering"
                );
                ui.label("Settings owner setup");
            },
        );
        setup_output.drop_without_applying_deltas();

        let mut targets = settings_jump_frame(&ctx, &mut app, &mut jump, Vec::new(), None, true);
        let hotkeys_header = target_by_label(&targets, "Hotkeys");
        let hotkeys_title_rect = hotkeys_header
            .paint_label_rect()
            .expect("the collapsible section registers its actual painted heading rect");
        assert!(
            hotkeys_title_rect.left() >= hotkeys_header.rect().right(),
            "the Settings heading is painted to the right of its native collapse response"
        );
        app.settings_save_reconciliation_required = true;
        let disabled_targets =
            settings_jump_frame(&ctx, &mut app, &mut jump, Vec::new(), None, true);
        assert!(
            disabled_targets
                .iter()
                .all(|target| !matches!(target.metadata().label(), "Save" | "Cancel")),
            "disabled footer controls are excluded from the registry"
        );
        app.settings_save_reconciliation_required = false;
        assert!(app.jump_mode_enabled());
        let collapse = target_by_label(&targets, "Collapse all").clone();
        let focused = settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            Vec::new(),
            Some(collapse.id().clone()),
            true,
        );
        assert!(
            target_by_label(&focused, "Launcher hotkey").eligible(),
            "focus handoff never activates Expand/Collapse"
        );
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(collapse.response_id()),
            "the one-shot intent focuses the real Settings button response"
        );

        let enter = egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let collapsed =
            settings_jump_frame(&ctx, &mut app, &mut jump, vec![enter.clone()], None, true);
        assert!(
            collapsed
                .iter()
                .all(|target| target.metadata().label() != "Launcher hotkey"),
            "a later native Enter activates Collapse all"
        );
        let expand = target_by_label(&collapsed, "Expand all").clone();
        settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            Vec::new(),
            Some(expand.id().clone()),
            true,
        );
        targets = settings_jump_frame(&ctx, &mut app, &mut jump, vec![enter.clone()], None, true);
        assert!(target_by_label(&targets, "Launcher hotkey").eligible());

        let checkbox = target_by_label(&targets, "Enable quit hotkey").clone();
        let quit_hotkey_before = app.settings_editor.quit_hotkey_enabled;
        settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            Vec::new(),
            Some(checkbox.id().clone()),
            true,
        );
        assert_eq!(app.settings_editor.quit_hotkey_enabled, quit_hotkey_before);
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(checkbox.response_id()),
            "checkbox intent uses its actual egui response"
        );
        let space = egui::Event::Key {
            key: egui::Key::Space,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let (checkbox_targets, checkbox_values, checkbox_passes) =
            settings_jump_discard_frame(&ctx, &mut app, &mut jump, vec![space]);
        targets = checkbox_targets;
        assert_eq!(checkbox_passes, 2, "egui retains its second pass");
        assert_eq!(checkbox_values.len(), checkbox_passes);
        assert_ne!(
            checkbox_values[0].quit_hotkey_enabled, quit_hotkey_before,
            "the first native pass applies the Space activation"
        );
        assert_eq!(
            checkbox_values[1].quit_hotkey_enabled, checkbox_values[0].quit_hotkey_enabled,
            "the actual discarded pass retains the first pass's checkbox state"
        );

        let pointer_checkbox = target_by_label(&targets, "Enable quit hotkey").clone();
        let pointer_click =
            std::iter::once(egui::Event::PointerMoved(pointer_checkbox.rect().center()))
                .chain(
                    [true, false]
                        .into_iter()
                        .map(|pressed| egui::Event::PointerButton {
                            pos: pointer_checkbox.rect().center(),
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        }),
                )
                .collect();
        let before_pointer = app.settings_editor.quit_hotkey_enabled;
        let (pointer_targets, pointer_values, pointer_passes) =
            settings_jump_discard_frame(&ctx, &mut app, &mut jump, pointer_click);
        targets = pointer_targets;
        assert_eq!(
            pointer_passes, 2,
            "pointer activation also requests a retry"
        );
        assert_eq!(pointer_values[0].quit_hotkey_enabled, !before_pointer);
        assert_eq!(
            pointer_values[1].quit_hotkey_enabled,
            pointer_values[0].quit_hotkey_enabled
        );

        targets = settings_scroll_until_target(
            &ctx,
            &mut app,
            &mut jump,
            targets,
            "Enable Next shortcut",
            SettingsScrollDirection::Later,
        );
        assert!(target_by_label(&targets, "Enable Next shortcut").eligible());
        targets = settings_scroll_until_target(
            &ctx,
            &mut app,
            &mut jump,
            targets,
            "Paste Sequences",
            SettingsScrollDirection::Later,
        );
        assert!(target_by_label(&targets, "Paste Sequences").eligible());
        let paste_header = target_by_label(&targets, "Paste Sequences").clone();
        settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            Vec::new(),
            Some(paste_header.id().clone()),
            true,
        );
        let (collapsed_targets, header_values, header_passes) =
            settings_jump_discard_frame(&ctx, &mut app, &mut jump, vec![enter.clone()]);
        assert_eq!(header_passes, 2, "section toggles survive the retry");
        assert!(
            collapsed_targets
                .iter()
                .all(|target| target.metadata().label() != "Enable Next shortcut")
        );
        assert!(header_values.iter().all(|pass| !pass.paste_body_visible));

        targets = settings_scroll_until_target(
            &ctx,
            &mut app,
            &mut jump,
            collapsed_targets,
            "Launcher hotkey",
            SettingsScrollDirection::Earlier,
        );
        assert!(target_by_label(&targets, "Launcher hotkey").eligible());
        let launcher_hotkey = target_by_label(&targets, "Launcher hotkey").clone();
        settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            Vec::new(),
            Some(launcher_hotkey.id().clone()),
            true,
        );
        let original_hotkey = app.settings_editor.hotkey.clone();
        let (hotkey_targets, hotkey_values, hotkey_passes) = settings_jump_discard_frame(
            &ctx,
            &mut app,
            &mut jump,
            vec![egui::Event::Text("q".into())],
        );
        targets = hotkey_targets;
        assert_eq!(hotkey_passes, 2, "text entry is checked in a retained pass");
        assert_eq!(hotkey_values[0].hotkey, format!("{original_hotkey}q"));
        assert_eq!(hotkey_values[1].hotkey, hotkey_values[0].hotkey);
        assert_ne!(app.settings_editor.hotkey, original_hotkey);
        assert!(app.settings_editor.hotkey.contains('q'));
        assert_eq!(
            hotkey_values[1].hotkey_cursor,
            Some(original_hotkey.chars().count() + 1),
            "the retained TextEdit pass keeps its caret after the inserted character"
        );

        targets = settings_scroll_until_target(
            &ctx,
            &mut app,
            &mut jump,
            targets,
            "Experimental Jump Mode",
            SettingsScrollDirection::Later,
        );
        assert!(target_by_label(&targets, "Experimental Jump Mode").eligible());
        let jump_mode_toggle = target_by_label(&targets, "Experimental Jump Mode").clone();
        settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            Vec::new(),
            Some(jump_mode_toggle.id().clone()),
            true,
        );
        settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            vec![egui::Event::Key {
                key: egui::Key::Space,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            None,
            true,
        );
        assert!(!app.settings_editor.experimental_jump_mode_enabled);
        assert!(app.jump_mode_enabled(), "draft edits do not change runtime");

        let cancel = target_by_label(&targets, "Cancel").clone();
        settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            Vec::new(),
            Some(cancel.id().clone()),
            true,
        );
        settings_jump_frame(&ctx, &mut app, &mut jump, vec![enter], None, true);
        assert!(!app.show_settings);
        assert!(
            app.jump_mode_enabled(),
            "Cancel keeps the committed runtime flag"
        );
        assert!(app.settings_editor.experimental_jump_mode_enabled);
    }

    #[test]
    fn jump_mode_settings_slider_and_value_editor_are_distinct_native_targets() {
        use crate::gui::jump_mode::ui::{JumpModeUi, NavigationOwner};

        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        ctx.global_style_mut(|style| style.animation_time = 0.0);
        let mut settings = crate::settings::Settings::default();
        settings.experimental_jump_mode_enabled = true;
        let mut app = settings_test_app(&ctx, &directory, settings);
        app.show_settings = true;
        for _ in 0..3 {
            paste_settings_frame(&ctx, &mut app, Vec::new());
        }
        paste_settings_click(&ctx, &mut app, "Expand all");
        paste_settings_click(&ctx, &mut app, "Collapse all");
        paste_settings_click(&ctx, &mut app, "Search & Results");

        let mut jump = JumpModeUi::new(true);
        let settings_owner = NavigationOwner::Settings { instance: 1 };
        let setup_output = ctx.run_ui(
            egui::RawInput {
                focused: true,
                ..Default::default()
            },
            |ui| {
                jump.begin_frame();
                assert!(jump.observe_settings_visibility(true));
                assert_eq!(
                    jump.update_navigation_owner(&ctx, settings_owner, true),
                    NavigationOwner::Blocked,
                    "the newly opened Settings owner waits for retained window ordering"
                );
                ui.label("Settings slider owner setup");
            },
        );
        setup_output.drop_without_applying_deltas();

        let targets = settings_jump_frame(&ctx, &mut app, &mut jump, Vec::new(), None, true);
        let slider = target_by_label(&targets, "Fuzzy weight").clone();
        assert!(
            slider.clipped_rect().contains_rect(slider.rect()),
            "the actual Settings slider is fully visible before focus handoff"
        );
        let numeric = target_by_label(&targets, "Fuzzy weight value").clone();
        assert!(slider.eligible());
        assert!(numeric.eligible());
        assert_eq!(
            slider.metadata().kind(),
            crate::gui::jump_mode::TargetKind::Slider
        );
        assert_eq!(
            numeric.metadata().kind(),
            crate::gui::jump_mode::TargetKind::TextEntry
        );
        assert_ne!(
            slider.id(),
            numeric.id(),
            "semantic IDs distinguish the slider thumb from its numeric editor"
        );
        assert_ne!(
            slider.response_id(),
            numeric.response_id(),
            "the actual egui slider and DragValue responses have distinct native IDs"
        );

        let original = app.settings_editor.fuzzy_weight;
        let focused = settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            Vec::new(),
            Some(slider.id().clone()),
            true,
        );
        assert_eq!(
            target_by_label(&focused, "Fuzzy weight").response_id(),
            slider.response_id()
        );
        assert_eq!(
            target_by_label(&focused, "Fuzzy weight").rect(),
            slider.rect(),
            "the Settings slider geometry remains stationary through focus handoff"
        );
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(slider.response_id())
        );
        assert_eq!(
            app.settings_editor.fuzzy_weight, original,
            "focusing either control does not change the setting"
        );
        settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            vec![egui::Event::Key {
                key: egui::Key::ArrowRight,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            None,
            true,
        );
        assert!(app.settings_editor.fuzzy_weight > original);
        assert!(app.settings_editor.fuzzy_weight <= 5.0);
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(slider.response_id()),
            "the first native slider arrow keeps focus on the actual thumb response"
        );
        let after_first_arrow = app.settings_editor.fuzzy_weight;
        settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            vec![egui::Event::Key {
                key: egui::Key::ArrowRight,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            None,
            true,
        );
        assert!(app.settings_editor.fuzzy_weight > after_first_arrow);
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(slider.response_id()),
            "subsequent native slider arrows remain focused without another hint"
        );

        let before_numeric = app.settings_editor.fuzzy_weight;
        settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            Vec::new(),
            Some(numeric.id().clone()),
            true,
        );
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(numeric.response_id())
        );
        settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            vec![egui::Event::Key {
                key: egui::Key::ArrowUp,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            None,
            true,
        );
        assert!(app.settings_editor.fuzzy_weight > before_numeric);
        assert!(app.settings_editor.fuzzy_weight <= 5.0);

        app.settings_editor.fuzzy_weight = 5.0;
        settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            vec![egui::Event::Key {
                key: egui::Key::ArrowUp,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            None,
            true,
        );
        assert_eq!(
            app.settings_editor.fuzzy_weight, 5.0,
            "the actual Settings DragValue does not step above its configured maximum"
        );
    }

    fn draw_sections(ctx: &egui::Context, expand_all: Option<bool>) -> (Vec<bool>, Vec<egui::Id>) {
        let mut open_states = Vec::new();
        let mut ids = Vec::new();
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640.0, 520.0),
                )),
                ..Default::default()
            },
            |ui| {
                if let Some(open) = expand_all {
                    set_all_settings_sections_open(ui, open);
                }
                let mut targets = SettingsJumpTargets::new(None, true, false, None);
                for section in TopLevelSettingsSection::ALL {
                    let id = section.egui_id(ui);
                    show_settings_section(ui, section, None, &mut targets, |ui, _| {
                        ui.label("section content");
                    });
                    open_states.push(
                        egui::collapsing_header::CollapsingState::load_with_default_open(
                            ctx,
                            id,
                            section.default_open(),
                        )
                        .is_open(),
                    );
                    ids.push(id);
                }
            },
        );
        output.drop_without_applying_deltas();
        (open_states, ids)
    }

    #[test]
    fn settings_sections_keep_ids_and_collapse_state_while_footer_reserves_space() {
        let ctx = egui::Context::default();
        let (defaults, ids) = draw_sections(&ctx, None);
        assert_eq!(
            defaults,
            vec![true, false, true, true, false, false, false, false],
            "the established defaults remain intact beside the new collapsed Paste section"
        );

        let (expanded, expanded_ids) = draw_sections(&ctx, Some(true));
        assert!(expanded.iter().all(|open| *open));
        assert_eq!(expanded_ids, ids, "expand-all keeps persistent section IDs");

        let (collapsed, collapsed_ids) = draw_sections(&ctx, Some(false));
        assert!(collapsed.iter().all(|open| !open));
        assert_eq!(
            collapsed_ids, ids,
            "collapse-all keeps persistent section IDs"
        );

        let mut save_rect = egui::Rect::NOTHING;
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640.0, 240.0),
                )),
                ..Default::default()
            },
            |ui| {
                let content_height = SettingsEditor::settings_content_height(
                    ui.available_height(),
                    settings_footer_reserved_height(ui),
                );
                egui::ScrollArea::vertical()
                    .min_scrolled_height(0.0)
                    .max_height(content_height)
                    .show(ui, |ui| ui.add_space(600.0));
                ui.separator();
                save_rect = ui.button("Save").rect;
            },
        );
        output.drop_without_applying_deltas();
        assert!(
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 240.0))
                .contains_rect(save_rect),
            "the Save control remains inside a short viewport"
        );
    }
}
