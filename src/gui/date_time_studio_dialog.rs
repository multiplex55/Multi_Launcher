use super::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use crate::date_arithmetic::{
    DateBetweenEndpoint, DateBetweenRequest, DateOffsetRequest, DateOffsetUnit, DateValue,
    FoldChoice, LocalResolution, NamedTimeZone, ScheduleCadence, SchedulePreviewRequest,
    apply_offset_request, convert_to_time_zones, evaluate_between, parse_anchor, preview_schedule,
    resolve_local_datetime, search_time_zones,
};
use chrono::{
    DateTime, Datelike, Local, Months, NaiveDate, NaiveDateTime, NaiveTime, Offset, TimeZone,
    Timelike, Utc, Weekday,
};
use eframe::egui;

const ZONE_SUGGESTION_LIMIT: usize = 8;
const ZONE_TARGET_LIMIT: usize = crate::date_arithmetic::MAX_ZONE_TARGETS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StudioMode {
    Between,
    AddSubtract,
    TimeZones,
    SchedulePreview,
}

impl StudioMode {
    const ALL: [Self; 4] = [
        Self::Between,
        Self::AddSubtract,
        Self::TimeZones,
        Self::SchedulePreview,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::Between => "Between",
            Self::AddSubtract => "Add / Subtract",
            Self::TimeZones => "Time zones",
            Self::SchedulePreview => "Schedule preview",
        }
    }
}

#[derive(Clone, PartialEq)]
struct StudioDraft {
    mode: StudioMode,
    reference_now: Option<NaiveDateTime>,
    between_start_date: String,
    between_end_date: String,
    between_start_month: NaiveDate,
    between_end_month: NaiveDate,
    between_include_time: bool,
    between_start_time: String,
    between_end_time: String,
    offset_anchor: String,
    offset_picker_date: String,
    offset_picker_month: NaiveDate,
    offset_include_time: bool,
    offset_time: String,
    offset_amount: String,
    offset_subtract: bool,
    offset_unit: DateOffsetUnit,
    zones_date: String,
    zones_month: NaiveDate,
    zones_time: String,
    zones_source: String,
    zones_targets: Vec<String>,
    zones_target_search: String,
    zones_fold_choice: Option<FoldChoice>,
    schedule_date: String,
    schedule_month: NaiveDate,
    schedule_time: String,
    schedule_zone: String,
    schedule_zone_search: String,
    schedule_cadence: ScheduleCadence,
    schedule_count: String,
    selected_schedule_index: Option<usize>,
    schedule_fold_choice: Option<FoldChoice>,
}

impl Default for StudioDraft {
    fn default() -> Self {
        let placeholder = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap_or(NaiveDate::MIN);
        Self {
            mode: StudioMode::Between,
            reference_now: None,
            between_start_date: String::new(),
            between_end_date: String::new(),
            between_start_month: placeholder,
            between_end_month: placeholder,
            between_include_time: false,
            between_start_time: "09:00".into(),
            between_end_time: "10:00".into(),
            offset_anchor: "now".into(),
            offset_picker_date: String::new(),
            offset_picker_month: placeholder,
            offset_include_time: true,
            offset_time: "09:00".into(),
            offset_amount: "1".into(),
            offset_subtract: false,
            offset_unit: DateOffsetUnit::Day,
            zones_date: String::new(),
            zones_month: placeholder,
            zones_time: "09:00".into(),
            zones_source: "UTC".into(),
            zones_targets: vec!["Europe/London".into()],
            zones_target_search: String::new(),
            zones_fold_choice: None,
            schedule_date: String::new(),
            schedule_month: placeholder,
            schedule_time: "09:00".into(),
            schedule_zone: "UTC".into(),
            schedule_zone_search: String::new(),
            schedule_cadence: ScheduleCadence::Daily,
            schedule_count: "7".into(),
            selected_schedule_index: None,
            schedule_fold_choice: None,
        }
    }
}

impl StudioDraft {
    fn initialize_reference(&mut self, reference: NaiveDateTime) {
        let date = reference.date();
        let month = first_of_month(date);
        let date_text = date.format("%Y-%m-%d").to_string();
        let time_text = format_time(reference.time());
        self.reference_now = Some(reference);
        self.between_start_date.clone_from(&date_text);
        self.between_end_date.clone_from(&date_text);
        self.between_start_month = month;
        self.between_end_month = month;
        self.between_start_time.clone_from(&time_text);
        self.between_end_time.clone_from(&time_text);
        self.offset_anchor = "now".into();
        self.offset_picker_date.clone_from(&date_text);
        self.offset_picker_month = month;
        self.offset_include_time = true;
        self.offset_time.clone_from(&time_text);
        self.zones_date.clone_from(&date_text);
        self.zones_month = month;
        self.zones_time.clone_from(&time_text);
        self.schedule_date = date_text;
        self.schedule_month = month;
        self.schedule_time = time_text;
        self.selected_schedule_index = None;
        self.zones_fold_choice = None;
        self.schedule_fold_choice = None;
    }

    fn refresh_reference(&mut self, reference: NaiveDateTime) {
        self.reference_now = Some(reference);
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DateTimeStudioEffect {
    CopyText(String),
    PrepareCalendar { instant_utc: DateTime<Utc> },
}

#[derive(Clone, Copy)]
enum StudioIntent {
    RefreshReference,
}

#[derive(Clone)]
struct StudioPopupAnchor {
    popup: egui::Id,
    layer: egui::LayerId,
    viewport: egui::ViewportId,
    epoch: u64,
}
impl StudioPopupAnchor {
    fn new(ui: &egui::Ui, response: &egui::Response, epoch: u64) -> Self {
        Self {
            popup: response.id.with("popup"),
            layer: response.layer_id,
            viewport: ui.ctx().viewport_id(),
            epoch,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct DeferredStudioFocus {
    host: super::utility_host::HostIdentity,
    epoch: u64,
    viewport: egui::ViewportId,
    id: egui::Id,
}

struct HostedFrame {
    root: super::utility_host::ViewportPassProvenance,
    owner: super::utility_host::ViewportPassProvenance,
    host: super::utility_host::HostIdentity,
    epoch: u64,
    draft: Option<StudioDraft>,
    changed: bool,
    focus_id: Option<egui::Id>,
    consume_focus: bool,
    intent: Option<StudioIntent>,
    effect: Option<DateTimeStudioEffect>,
    close: bool,
    request: Option<super::utility_host::UtilityPresentation>,
    info: Option<egui::ViewportInfo>,
    copies: Vec<String>,
    installed_focus: Option<DeferredStudioFocus>,
}
#[derive(Default)]
pub(super) struct DateTimeStudioHostOutcome {
    pub(super) effect: Option<(
        DateTimeStudioEffect,
        super::utility_host::AcceptedParentOpen<u64>,
    )>,
    pub(super) copies: Vec<String>,
    pub(super) show_root: bool,
    pub(super) closed: bool,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
struct TestPrepareUiObservation {
    rect: egui::Rect,
    clip_rect: egui::Rect,
    enabled: bool,
    clicked: bool,
    interact_pointer_pos: Option<egui::Pos2>,
}

#[derive(Default)]
pub(crate) struct DateTimeStudioDialog {
    pub(crate) open: bool,
    focus_pending: bool,
    draft: StudioDraft,
    notice: Option<String>,
    host_close_requested: bool,
    host: super::utility_host::UtilityHostState,
    presentation_prepared: bool,
    host_epoch: u64,
    host_pending: Option<HostedFrame>,
    host_os_close_pending: bool,
    host_focus: bool,
    deferred_host_focus: Option<DeferredStudioFocus>,
    host_popup: Option<(egui::ViewportId, egui::Id)>,
    retired_host_popups: Vec<(egui::ViewportId, egui::Id)>,
    host_popups: Vec<StudioPopupAnchor>,
    #[cfg(test)]
    host_button: Option<(egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    test_owner_focus: Option<egui::Id>,
    #[cfg(test)]
    test_window_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_controls: Vec<(String, egui::Rect)>,
    #[cfg(test)]
    test_focusable_controls: Vec<(String, egui::Id, egui::Rect, egui::Rect)>,
    #[cfg(test)]
    test_prepare_observation: Option<TestPrepareUiObservation>,
    #[cfg(test)]
    test_prepare_effect_staged: bool,
    #[cfg(test)]
    test_prepare_effect_retained: bool,
}

impl DateTimeStudioDialog {
    pub(crate) fn open(&mut self) {
        self.initialize_reference_if_needed();
        if !self.open {
            self.presentation_prepared = false;
            self.host_epoch = self.host_epoch.wrapping_add(1).max(1);
        }
        self.open = true;
        self.host_close_requested = false;
        self.request_host_focus();
    }

    pub(crate) fn ensure_open(&mut self) {
        self.initialize_reference_if_needed();
        if !self.open {
            self.presentation_prepared = false;
            self.host_epoch = self.host_epoch.wrapping_add(1).max(1);
        }
        self.open = true;
    }

    pub(super) fn prepare_open_presentation(&mut self, ctx: &egui::Context) {
        if self.open && !self.presentation_prepared {
            let key = crate::settings::UtilityToolKey::parse("date-time-studio")
                .expect("stable Date & Time Studio key");
            super::utility_host::restore_remembered_presentation(ctx, key, &mut self.host);
            self.presentation_prepared = true;
        }
    }

    pub(crate) fn request_close(&mut self) {
        if self.open {
            self.host_close_requested = true;
        }
    }

    pub(crate) fn copy_completed(&mut self, result: Result<(), String>) {
        self.notice = Some(match result {
            Ok(()) => "Copied to the clipboard.".into(),
            Err(error) => format!("Clipboard copy failed: {error}"),
        });
    }

    pub(crate) fn calendar_prepare_completed(&mut self, result: Result<String, String>) {
        self.notice = Some(match result {
            Ok(message) => message,
            Err(error) => error,
        });
    }

    #[cfg(test)]
    pub(crate) fn test_set_anchor_and_reference(
        &mut self,
        anchor: String,
        reference: NaiveDateTime,
    ) {
        self.draft.reference_now = Some(reference);
        self.draft.offset_anchor = anchor;
    }

    #[cfg(test)]
    pub(crate) fn test_anchor_and_reference(&self) -> (&str, Option<NaiveDateTime>) {
        (&self.draft.offset_anchor, self.draft.reference_now)
    }

    #[cfg(test)]
    pub(crate) fn test_focus_pending(&self) -> bool {
        self.focus_pending
    }

    #[cfg(test)]
    pub(crate) fn test_set_schedule_preview(&mut self, date: &str, time: &str, zone: &str) {
        self.draft.mode = StudioMode::SchedulePreview;
        self.draft.schedule_date = date.to_owned();
        self.draft.schedule_time = time.to_owned();
        self.draft.schedule_zone = zone.to_owned();
        self.draft.schedule_count = "1".into();
        self.draft.selected_schedule_index = Some(0);
        self.open = true;
    }

    #[cfg(test)]
    pub(crate) fn test_control_rect(&self, id: &str) -> Option<egui::Rect> {
        self.test_controls
            .iter()
            .find(|(control, _)| control == id)
            .map(|(_, rect)| *rect)
    }

    #[cfg(test)]
    pub(crate) fn test_focusable_control(
        &self,
        id: &str,
    ) -> Option<(egui::Id, egui::Rect, egui::Rect)> {
        self.test_focusable_controls
            .iter()
            .find(|(control, _, _, _)| control == id)
            .map(|(_, response_id, rect, clip)| (*response_id, *rect, *clip))
    }

    #[cfg(test)]
    pub(crate) fn test_notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn test_prepare_observation(
        &self,
    ) -> Option<(egui::Rect, egui::Rect, bool, bool, Option<egui::Pos2>)> {
        self.test_prepare_observation.map(|observation| {
            (
                observation.rect,
                observation.clip_rect,
                observation.enabled,
                observation.clicked,
                observation.interact_pointer_pos,
            )
        })
    }

    #[cfg(test)]
    pub(crate) fn test_prepare_effect_staged(&self) -> bool {
        self.test_prepare_effect_staged
    }

    #[cfg(test)]
    pub(crate) fn test_prepare_effect_retained(&self) -> bool {
        self.test_prepare_effect_retained
    }

    #[cfg(test)]
    pub(crate) fn test_was_rendered(&self) -> bool {
        self.test_window_rect.is_some()
    }

    #[cfg(test)]
    pub(crate) fn test_primary_input_has_focus(&self, ctx: &egui::Context) -> bool {
        self.test_focusable_controls
            .iter()
            .find(|(id, _, _, _)| id == "between_start_input")
            .is_some_and(|(_, id, _, _)| ctx.memory(|memory| memory.focused()) == Some(*id))
    }

    #[cfg(test)]
    pub(crate) fn show(
        &mut self,
        ctx: &egui::Context,
        first_ui_pass: bool,
        calendar_editor_open: bool,
    ) -> Option<DateTimeStudioEffect> {
        self.show_hosted(
            ctx,
            first_ui_pass,
            calendar_editor_open,
            true,
            true,
            true,
            false,
            true,
        );
        None
    }

    #[cfg(test)]
    pub(crate) fn on_ui_frame_complete(
        &mut self,
        ctx: &egui::Context,
    ) -> Option<DateTimeStudioEffect> {
        self.on_hosted_frame_complete(ctx)
            .effect
            .map(|(effect, _)| effect)
    }

    pub(super) fn accepts_calendar_origin(
        &self,
        receipt: &super::utility_host::AcceptedParentOpen<u64>,
    ) -> bool {
        self.open && receipt.is_current(self.host.identity(), &self.host_epoch)
    }

    pub(super) fn root_layer(&self) -> Option<egui::LayerId> {
        (self.open
            && (self.host.presentation() == super::utility_host::UtilityPresentation::Docked
                || self.host.embedded_root_layer().is_some()))
        .then_some(egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new("date-time-studio-window"),
        ))
    }
    pub(super) fn needs_root_ui_when_hidden(&self) -> bool {
        self.open && self.host.presentation() == super::utility_host::UtilityPresentation::Popout
    }
    pub(super) fn request_host_focus(&mut self) {
        self.host_focus = true;
        self.focus_pending = true;
        self.host.request_focus();
    }
    pub(super) fn presentation(&self) -> super::utility_host::UtilityPresentation {
        self.host.presentation()
    }
    pub(super) fn viewport() -> egui::ViewportId {
        egui::ViewportId::from_hash_of("date-time-studio-viewport")
    }
    fn native_identity(
        &self,
        host: super::utility_host::HostIdentity,
    ) -> super::utility_window_geometry::NativeViewportIdentity {
        super::utility_window_geometry::NativeViewportIdentity {
            key: crate::settings::UtilityToolKey::parse("date-time-studio")
                .expect("stable studio key"),
            viewport_id: Self::viewport(),
            owner_generation: self.host_epoch,
            host_generation: host.generation,
        }
    }
    fn close_retired_popups(&mut self, ctx: &egui::Context) {
        self.retired_host_popups.retain(|&(viewport, id)| {
            if ctx.viewport_id() == viewport {
                egui::Popup::close_id(ctx, id);
                false
            } else {
                true
            }
        });
    }

    fn retire_owned_popup(&mut self, ctx: &egui::Context) {
        if let Some(popup) = self.host_popup.take()
            && !self.retired_host_popups.contains(&popup)
        {
            self.retired_host_popups.push(popup);
        }
        self.close_retired_popups(ctx);
    }

    pub(super) fn end_session_with_host(&mut self, ctx: &egui::Context) {
        if self.open {
            let key = crate::settings::UtilityToolKey::parse("date-time-studio")
                .expect("stable Date & Time Studio key");
            super::utility_host::remember_presentation(ctx, key, self.host.presentation());
        }
        self.presentation_prepared = false;
        self.retire_owned_popup(ctx);
        self.deferred_host_focus = None;
        if self.host.presentation() == super::utility_host::UtilityPresentation::Popout {
            let identity = self.native_identity(self.host.identity());
            super::utility_dialog_size::close_native_utility_geometry_viewport(ctx, identity);
            ctx.send_viewport_cmd_to(identity.viewport_id, egui::ViewportCommand::Close);
        }
        self.host_pending = None;
        self.host_os_close_pending = false;
        self.host_close_requested = false;
        self.host_focus = false;
        self.focus_pending = false;
        self.host_popups.clear();
        self.host.retire_to_docked();
        self.open = false;
    }
    pub(super) fn owned_popup_layer(
        &self,
        ctx: &egui::Context,
        parent: egui::LayerId,
    ) -> Option<egui::LayerId> {
        let anchor = self.host_popups.iter().find(|a| {
            a.epoch == self.host_epoch
                && a.viewport == ctx.viewport_id()
                && a.layer == parent
                && egui::Popup::is_id_open(ctx, a.popup)
        })?;
        let rect = ctx.memory(|m| m.area_rect(anchor.popup))?;
        let layer = egui::LayerId::new(egui::Order::Foreground, anchor.popup);
        let top = ctx.memory(|m| {
            m.layer_ids()
                .filter(|l| l.order == egui::Order::Foreground && m.areas().visible_last_frame(l))
                .last()
        });
        let sizing =
            egui::containers::AreaState::load(ctx, anchor.popup).is_some_and(|a| !a.interactable);
        (rect.is_positive()
            && top == Some(layer)
            && (ctx.layer_id_at(rect.center()) == Some(layer) || sizing))
            .then_some(layer)
    }
    #[cfg(test)]
    pub(super) fn test_selected_schedule_index(&self) -> Option<usize> {
        self.draft.selected_schedule_index
    }

    #[cfg(test)]
    pub(super) fn test_host_button(&self) -> Option<(egui::Rect, egui::Rect, bool)> {
        self.host_button
    }

    #[cfg(test)]
    pub(super) fn test_owner_focus(&self) -> Option<egui::Id> {
        self.test_owner_focus
    }

    #[cfg(test)]
    pub(super) fn test_owner_popup_open(&self) -> bool {
        self.host_popup.is_some()
    }

    pub(super) fn show_hosted(
        &mut self,
        ctx: &egui::Context,
        first: bool,
        calendar_editor_open: bool,
        root_visible: bool,
        root_owner: bool,
        input_allowed: bool,
        pinned: bool,
        shortcut_allowed: bool,
    ) {
        use super::utility_host::{UtilityPresentation, ViewportPassProvenance};
        self.prepare_open_presentation(ctx);
        self.host_pending = None;
        self.close_retired_popups(ctx);
        #[cfg(test)]
        {
            self.test_window_rect = None;
            self.test_controls.clear();
            self.test_focusable_controls.clear();
            self.host_button = None;
            if first {
                self.test_prepare_observation = None;
                self.test_prepare_effect_staged = false;
                self.test_prepare_effect_retained = false;
            }
        }
        if !self.open {
            return;
        }
        if pinned {
            self.host_os_close_pending = false;
        }
        let host = self.host.identity();
        let root = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
        let mut owner = None;
        let mut info = None;
        let mut staged = None;
        if host.host == UtilityPresentation::Docked {
            if root_visible {
                let allowed = root_owner && input_allowed;
                let shortcut = first
                    && allowed
                    && shortcut_allowed
                    && !pinned
                    && !egui::Popup::is_any_open(ctx)
                    && ctx.input_mut(|i| {
                        i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                            || i.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                    });
                staged = Some(self.hosted_draw(
                    ctx,
                    first,
                    allowed,
                    calendar_editor_open,
                    None,
                    host.host,
                ));
                if let Some(result) = &mut staged {
                    result.close |= shortcut;
                }
                owner = Some(root);
            }
        } else {
            let viewport = Self::viewport();
            let builder = egui::ViewportBuilder::default()
                .with_title("Date & Time Studio")
                .with_inner_size([900.0, 720.0])
                .with_min_inner_size([460.0, 360.0])
                .with_resizable(true);
            ctx.show_viewport_immediate(viewport, builder, |child, class| {
                let c = child.ctx().clone();
                self.close_retired_popups(&c);
                let native = class == egui::ViewportClass::Immediate && c.viewport_id() == viewport;
                let child_first = first && child.current_pass_index() == 0;
                if child_first {
                    owner = Some(ViewportPassProvenance::current(&c, c.viewport_id()));
                    if native {
                        info = Some(child.input(|i| i.viewport().clone()));
                    }
                } else {
                    owner = None;
                    info = None;
                    staged = None;
                }
                let parent = if native {
                    egui::LayerId::background()
                } else {
                    egui::LayerId::new(
                        egui::Order::Middle,
                        egui::Id::new("date-time-studio-window"),
                    )
                };
                let popup = self.owned_popup_layer(&c, parent);
                let allowed = if native {
                    input_allowed
                        && c.input(|i| i.focused)
                        && c.memory(|m| m.top_modal_layer().is_none())
                        && super::jump_mode::ui::visible_top_middle_layer(&c).is_none()
                        && (!egui::Popup::is_any_open(&c) || popup.is_some())
                        && super::jump_mode::ui::focused_response_layer(&c)
                            .is_none_or(|l| l == parent || Some(l) == popup)
                } else {
                    root_owner && input_allowed
                };
                if native && child_first && child.input(|i| i.viewport().close_requested()) {
                    child.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    if !pinned {
                        self.host_os_close_pending = true;
                    }
                }
                let shortcut = child_first
                    && allowed
                    && shortcut_allowed
                    && !pinned
                    && !egui::Popup::is_any_open(&c)
                    && c.input_mut(|i| {
                        i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                            || i.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                    });
                let mut result = if native {
                    let mut result = None;
                    egui::CentralPanel::default().show(child, |ui| {
                        result = Some(self.hosted_draw(
                            &c,
                            child_first,
                            allowed,
                            calendar_editor_open,
                            Some(ui),
                            host.host,
                        ));
                    });
                    result
                } else {
                    Some(self.hosted_draw(
                        &c,
                        child_first,
                        allowed,
                        calendar_editor_open,
                        None,
                        host.host,
                    ))
                };
                if let Some(result) = &mut result {
                    if !pinned {
                        result.close |= shortcut || self.host_os_close_pending;
                    }
                }
                staged = result;
            });
        }
        if let (Some(owner), Some(mut frame)) = (owner, staged) {
            if pinned {
                frame.close = false;
            }
            frame.root = root;
            frame.owner = owner;
            frame.host = host;
            frame.epoch = self.host_epoch;
            frame.info = info;
            if first && !ctx.will_discard() {
                self.host_pending = Some(frame);
            }
        }
    }

    fn hosted_draw(
        &mut self,
        ctx: &egui::Context,
        first: bool,
        allowed: bool,
        calendar_editor_open: bool,
        child_ui: Option<&mut egui::Ui>,
        presentation: super::utility_host::UtilityPresentation,
    ) -> HostedFrame {
        use super::utility_host::ViewportPassProvenance;
        let installed_focus = self.deferred_host_focus.filter(|focus| {
            first
                && allowed
                && !ctx.will_discard()
                && focus.host == self.host.identity()
                && focus.epoch == self.host_epoch
                && focus.viewport == ctx.viewport_id()
        });
        if let Some(focus) = installed_focus {
            ctx.memory_mut(|memory| memory.request_focus(focus.id));
        }
        let mut draft = self.draft.clone();
        let mut changed = false;
        let mut focus_id = None;
        let mut intent = None;
        let mut effect = None;
        let mut request = None;
        let mut window_open = self.open;
        let mut visible = false;
        let start = ctx.output(|o| o.commands.len());
        self.host_popups.clear();
        let mut body = |ui: &mut egui::Ui| {
            let authorized =
                first && allowed && ui.is_visible() && !ui.is_sizing_pass() && !ctx.will_discard();
            visible |= authorized;
            let (button, target) = super::utility_host::presentation_button(
                ui,
                presentation,
                allowed && ui.is_visible() && !ui.is_sizing_pass(),
            );
            #[cfg(test)]
            {
                self.host_button = Some((button.rect, ui.clip_rect(), button.enabled()));
                self.record_focusable_control(ui, "host-presentation", &button);
            }
            if authorized && button.clicked() {
                request = Some(target);
            }
            ui.add_enabled_ui(authorized, |ui| {
                self.contents_ui(
                    ui,
                    &mut draft,
                    &mut changed,
                    &mut focus_id,
                    &mut intent,
                    &mut effect,
                    calendar_editor_open,
                )
            });
        };
        let mut lifecycle = child_ui.is_some();
        if let Some(ui) = child_ui {
            body(ui);
        } else {
            let size = ctx.content_rect().size();
            let output = egui::Window::new("Date & Time Studio")
                .id(egui::Id::new("date-time-studio-window"))
                .resizable(true)
                .default_size(egui::vec2(900.0, 720.0))
                .min_size(egui::vec2(460.0, 360.0).min(size))
                .max_size(size)
                .show_utility(
                    ctx,
                    "date-time-studio",
                    egui::Id::new("date-time-studio-window"),
                    Some(&mut window_open),
                    super::utility_dialog_size::UtilityWindowSizeRecipe::new(
                        super::utility_dialog_size::UtilityWindowSizeDefault::outer(egui::vec2(
                            900.0, 720.0,
                        )),
                        egui::vec2(460.0, 360.0),
                        [None, None],
                    )
                    .with_maximum(size),
                    |ui| body(ui),
                );
            lifecycle = output
                .as_ref()
                .is_some_and(|o| o.response.rect.intersects(ctx.content_rect()));
            #[cfg(test)]
            {
                self.test_window_rect = output.as_ref().map(|o| o.response.rect);
            }
        }
        self.host_popup = self.host_popups.iter().find_map(|anchor| {
            (anchor.epoch == self.host_epoch
                && anchor.viewport == ctx.viewport_id()
                && egui::Popup::is_id_open(ctx, anchor.popup))
            .then_some((anchor.viewport, anchor.popup))
        });
        #[cfg(test)]
        {
            self.test_owner_focus = ctx.memory(|memory| memory.focused());
        }
        let copies = ctx.output_mut(|output| {
            let mut copies = Vec::new();
            for cmd in output.commands.split_off(start) {
                match cmd {
                    egui::OutputCommand::CopyText(text) => copies.push(text),
                    other => output.commands.push(other),
                }
            }
            copies
        });
        #[cfg(test)]
        if matches!(
            effect.as_ref(),
            Some(DateTimeStudioEffect::PrepareCalendar { .. })
        ) {
            self.test_prepare_effect_staged = true;
        }
        let close = first
            && !ctx.will_discard()
            && lifecycle
            && (self.host_close_requested || (allowed && !window_open));
        HostedFrame {
            root: ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT),
            owner: ViewportPassProvenance::current(ctx, ctx.viewport_id()),
            host: self.host.identity(),
            epoch: self.host_epoch,
            draft: visible.then_some(draft),
            changed: visible && changed,
            focus_id: if visible { focus_id } else { None },
            consume_focus: visible && self.focus_pending,
            intent: if visible { intent } else { None },
            effect: if visible { effect } else { None },
            close,
            request: if visible { request } else { None },
            info: None,
            installed_focus,
            copies: if visible { copies } else { Vec::new() },
        }
    }
    pub(super) fn on_hosted_frame_complete(
        &mut self,
        ctx: &egui::Context,
    ) -> DateTimeStudioHostOutcome {
        use super::utility_host::UtilityPresentation;
        let Some(frame) = self.host_pending.take() else {
            return DateTimeStudioHostOutcome::default();
        };
        if !self.open
            || frame.epoch != self.host_epoch
            || frame.host != self.host.identity()
            || !frame.root.was_retained(ctx)
            || !frame.owner.was_retained(ctx)
        {
            return DateTimeStudioHostOutcome::default();
        }
        let origin = frame.host.host;
        let identity = self.native_identity(frame.host);
        if let Some(info) = frame.info.as_ref() {
            super::utility_dialog_size::apply_native_utility_geometry_restore(
                ctx,
                egui::ViewportClass::Immediate,
                true,
                "Date & Time Studio",
                identity,
                info,
                egui::vec2(900.0, 720.0),
                Some(egui::vec2(460.0, 360.0)),
                None,
            );
            if self.host_focus {
                ctx.send_viewport_cmd_to(identity.viewport_id, egui::ViewportCommand::Focus);
            }
        } else if self.host_focus {
            if let Some(layer) = self.root_layer() {
                ctx.move_to_top(layer);
            }
        }
        self.host_focus = false;
        if let Some(draft) = frame.draft {
            self.draft = draft;
            if frame.changed {
                self.notice = None;
            }
        }
        if let Some(StudioIntent::RefreshReference) = frame.intent {
            self.draft.refresh_reference(Local::now().naive_local());
            self.notice = Some("Reference refreshed. Existing inputs are unchanged.".into());
        }
        if frame.installed_focus.is_some() && frame.installed_focus == self.deferred_host_focus {
            self.deferred_host_focus = None;
        }
        if frame.consume_focus && frame.request.is_none() && !frame.close {
            if let Some(id) = frame.focus_id {
                if frame.owner.viewport_id == ctx.viewport_id() {
                    ctx.memory_mut(|memory| memory.request_focus(id));
                } else {
                    self.deferred_host_focus = Some(DeferredStudioFocus {
                        host: frame.host,
                        epoch: frame.epoch,
                        viewport: frame.owner.viewport_id,
                        id,
                    });
                    ctx.request_repaint_of(frame.owner.viewport_id);
                }
            }
            self.focus_pending = false;
        }
        if frame.close {
            self.end_session_with_host(ctx);
            return DateTimeStudioHostOutcome {
                closed: true,
                ..Default::default()
            };
        }
        let show_root = frame.request == Some(UtilityPresentation::Docked);
        if let Some(next) = frame.request {
            if self.host.transition(next) {
                let key = crate::settings::UtilityToolKey::parse("date-time-studio").expect("stable Date & Time Studio key");
                super::utility_host::remember_presentation(ctx, key, next);
                self.retire_owned_popup(ctx);
                self.deferred_host_focus = None;
                self.focus_pending = true;
                if next == UtilityPresentation::Docked {
                    super::utility_dialog_size::close_native_utility_geometry_viewport(
                        ctx, identity,
                    );
                    ctx.send_viewport_cmd_to(identity.viewport_id, egui::ViewportCommand::Close);
                }
                self.host_focus = true;
                ctx.request_repaint();
            }
        }
        if origin == UtilityPresentation::Popout {
            self.host.complete_bootstrap(
                frame.host,
                frame.info.is_none().then_some(egui::LayerId::new(
                    egui::Order::Middle,
                    egui::Id::new("date-time-studio-window"),
                )),
                None,
            );
        }
        #[cfg(test)]
        if matches!(
            frame.effect.as_ref(),
            Some(DateTimeStudioEffect::PrepareCalendar { .. })
        ) {
            self.test_prepare_effect_retained = true;
        }
        if frame.changed
            || frame.effect.is_some()
            || frame.intent.is_some()
            || frame.consume_focus
            || !frame.copies.is_empty()
        {
            ctx.request_repaint();
        }
        let receipt = super::utility_host::AcceptedParentOpen::from_retained(
            ctx, frame.host, frame.epoch, frame.root, frame.owner,
        );
        DateTimeStudioHostOutcome {
            effect: if frame.request.is_none() {
                frame.effect.zip(receipt)
            } else {
                None
            },
            copies: if frame.request.is_none() {
                frame.copies
            } else {
                Vec::new()
            },
            show_root,
            closed: false,
        }
    }

    fn initialize_reference_if_needed(&mut self) {
        if self.draft.reference_now.is_none() {
            self.draft.initialize_reference(Local::now().naive_local());
        }
    }

    fn contents_ui(
        &mut self,
        ui: &mut egui::Ui,
        draft: &mut StudioDraft,
        changed: &mut bool,
        focus_id: &mut Option<egui::Id>,
        intent: &mut Option<StudioIntent>,
        effect: &mut Option<DateTimeStudioEffect>,
        calendar_editor_open: bool,
    ) {
        let mut refresh = false;
        let scroll = egui::ScrollArea::vertical()
            .id_salt("date-time-studio-content")
            .auto_shrink([false, false])
            .max_height(ui.available_height())
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width());
                ui.scope_builder(
                    stable_wrapped_scope(studio_root_id().with("mode-selectors")),
                    |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for mode in StudioMode::ALL {
                                let response = ui.selectable_label(draft.mode == mode, mode.label());
                                reveal_when_focused(&response);
                                if response.clicked() && draft.mode != mode {
                                    draft.mode = mode;
                                    *changed = true;
                                }
                                #[cfg(test)]
                                self.record_focusable_control(
                                    ui,
                                    format!("mode_{mode:?}"),
                                    &response,
                                );
                            }
                        });
                    },
                );
                ui.scope_builder(
                    stable_wrapped_scope(studio_root_id().with("reference-actions")),
                    |ui| {
                        ui.horizontal_wrapped(|ui| {
                            if let Some(reference) = draft.reference_now {
                                ui.label(format!(
                                    "Frozen reference: {}",
                                    reference.format("%Y-%m-%d %H:%M:%S%.f")
                                ));
                            }
                            let refresh_button = ui.button("Refresh now");
                            reveal_when_focused(&refresh_button);
                            if refresh_button.clicked() {
                                refresh = true;
                            }
                            #[cfg(test)]
                            self.record_focusable_control(
                                ui,
                                "refresh_reference",
                                &refresh_button,
                            );
                        });
                    },
                );
                if let Some(notice) = self.notice.as_deref() {
                    ui.scope_builder(
                        egui::UiBuilder::new()
                            .id(studio_root_id().with("notice"))
                            .layout(egui::Layout::top_down(egui::Align::Min)),
                        |ui| {
                            ui.label(notice);
                        },
                    );
                }
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .id(studio_root_id().with("help"))
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                    |ui| {
                        egui::CollapsingHeader::new("Date & Time Studio help")
                            .id_salt("date-time-studio-help")
                            .show(ui, |ui| {
                                ui.label("Date and local time fields accept keyboard input; calendars and hour/minute controls are also available.");
                                ui.label("Named-zone comparisons use the bundled offline IANA rules. Repeated times need an explicit fold choice; skipped times remain errors.");
                                ui.label("The frozen reference stays fixed until Refresh now. Schedule preview never creates events or alarms by itself.");
                            });
                    },
                );
                ui.separator();

                ui.scope_builder(
                    egui::UiBuilder::new()
                        .id(studio_root_id().with(("mode-body", draft.mode.label())))
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                    |ui| match draft.mode {
                        StudioMode::Between => {
                            self.between_ui(ui, draft, changed, focus_id, effect);
                        }
                        StudioMode::AddSubtract => {
                            self.offset_ui(ui, draft, changed, focus_id, effect);
                        }
                        StudioMode::TimeZones => {
                            self.time_zones_ui(ui, draft, changed, focus_id, effect);
                        }
                        StudioMode::SchedulePreview => {
                            self.schedule_ui(
                                ui,
                                draft,
                                changed,
                                focus_id,
                                effect,
                                calendar_editor_open,
                            );
                        }
                    },
                );
            });
        let _ = scroll;
        if refresh {
            *intent = Some(StudioIntent::RefreshReference);
        }
    }

    fn between_ui(
        &mut self,
        ui: &mut egui::Ui,
        draft: &mut StudioDraft,
        changed: &mut bool,
        focus_id: &mut Option<egui::Id>,
        effect: &mut Option<DateTimeStudioEffect>,
    ) {
        ui.label("Compare two calendar dates or local date-times. The calendar span uses clipped complete months plus an exact remainder.");
        self.date_picker_ui(
            ui,
            "between_start",
            "Start date",
            &mut draft.between_start_date,
            &mut draft.between_start_month,
            changed,
            focus_id,
        );
        self.date_picker_ui(
            ui,
            "between_end",
            "End date",
            &mut draft.between_end_date,
            &mut draft.between_end_month,
            changed,
            focus_id,
        );
        let include = ui.checkbox(&mut draft.between_include_time, "Include local times");
        if include.changed() {
            *changed = true;
        }
        #[cfg(test)]
        self.record_focusable_control(ui, "between_include_time", &include);
        if draft.between_include_time {
            self.time_input_ui(
                ui,
                "between_start_time",
                "Start time",
                &mut draft.between_start_time,
                changed,
                focus_id,
            );
            self.time_input_ui(
                ui,
                "between_end_time",
                "End time",
                &mut draft.between_end_time,
                changed,
                focus_id,
            );
        }

        let result = calculate_between_from_draft(draft);
        match result {
            Ok(result) => {
                ui.heading(format_signed_duration(result.exact_duration));
                ui.label(format!(
                    "{} days · {} weeks",
                    format_f64(result.days),
                    format_f64(result.weeks)
                ));
                ui.label(format_calendar_span(result.calendar_span));
                let copy_text = format!(
                    "Between {} and {}: {}; {} days; {}.",
                    format_date_value(result.first),
                    format_date_value(result.second),
                    format_signed_duration(result.exact_duration),
                    format_f64(result.days),
                    format_calendar_span(result.calendar_span)
                );
                let copy = stable_action_button(ui, "copy-between", "Copy difference");
                reveal_when_focused(&copy);
                if copy.clicked() {
                    *effect = Some(DateTimeStudioEffect::CopyText(copy_text));
                }
                #[cfg(test)]
                self.record_focusable_control(ui, "between_copy", &copy);
            }
            Err(error) => show_error(ui, &error.to_string()),
        }
    }

    fn offset_ui(
        &mut self,
        ui: &mut egui::Ui,
        draft: &mut StudioDraft,
        changed: &mut bool,
        focus_id: &mut Option<egui::Id>,
        effect: &mut Option<DateTimeStudioEffect>,
    ) {
        ui.label("Apply one checked calendar or elapsed-time offset to an explicit anchor.");
        ui.horizontal_wrapped(|ui| {
            ui.label("Anchor");
            let id = ui.make_persistent_id("date-time-studio-offset-anchor");
            if focus_id.is_none() {
                *focus_id = Some(id);
            }
            let response = ui.add(
                egui::TextEdit::singleline(&mut draft.offset_anchor)
                    .id(id)
                    .desired_width(220.0)
                    .char_limit(128),
            );
            if response.changed() {
                *changed = true;
                if let Some(reference) = draft.reference_now
                    && let Ok(value) = parse_anchor(&draft.offset_anchor, reference)
                {
                    let date = value_date(value);
                    draft.offset_picker_date = date.format("%Y-%m-%d").to_string();
                    draft.offset_picker_month = first_of_month(date);
                    if let DateValue::DateTime(date_time) = value {
                        draft.offset_time = format_time(date_time.time());
                        draft.offset_include_time = true;
                    } else {
                        draft.offset_include_time = false;
                    }
                }
            }
            #[cfg(test)]
            self.record_focusable_control(ui, "offset_anchor", &response);
            let use_now = ui.button("Use frozen now");
            reveal_when_focused(&use_now);
            if use_now.clicked()
                && let Some(reference) = draft.reference_now
            {
                draft.offset_anchor = "now".into();
                draft.offset_picker_date = reference.date().format("%Y-%m-%d").to_string();
                draft.offset_picker_month = first_of_month(reference.date());
                draft.offset_time = format_time(reference.time());
                draft.offset_include_time = true;
                *changed = true;
            }
            #[cfg(test)]
            self.record_focusable_control(ui, "offset_use_now", &use_now);
        });
        let picker_date_before = draft.offset_picker_date.clone();
        self.date_picker_ui(
            ui,
            "offset_anchor_date",
            "Anchor date",
            &mut draft.offset_picker_date,
            &mut draft.offset_picker_month,
            changed,
            focus_id,
        );
        let picker_date_changed = picker_date_before != draft.offset_picker_date;
        let include_time = ui.checkbox(&mut draft.offset_include_time, "Use a local time");
        if include_time.changed() {
            *changed = true;
            if let Some(reference) = draft.reference_now
                && draft.offset_time.trim().is_empty()
            {
                draft.offset_time = format_time(reference.time());
            }
        }
        #[cfg(test)]
        self.record_control("offset_include_time", include_time.rect);
        let mut anchor_time_changed = false;
        if draft.offset_include_time {
            let time_before = draft.offset_time.clone();
            self.time_input_ui(
                ui,
                "offset_time",
                "Anchor time",
                &mut draft.offset_time,
                changed,
                focus_id,
            );
            anchor_time_changed = time_before != draft.offset_time;
        }
        if (picker_date_changed || include_time.changed() || anchor_time_changed)
            && let Ok(date) = parse_date(&draft.offset_picker_date)
        {
            draft.offset_anchor = if draft.offset_include_time {
                format!("{} {}", date, draft.offset_time)
            } else {
                date.to_string()
            };
        }

        ui.horizontal_wrapped(|ui| {
            ui.label("Amount");
            let id = ui.make_persistent_id("date-time-studio-offset-amount");
            let amount = ui.add(
                egui::TextEdit::singleline(&mut draft.offset_amount)
                    .id(id)
                    .desired_width(100.0)
                    .char_limit(20),
            );
            if amount.changed() {
                *changed = true;
            }
            #[cfg(test)]
            self.record_focusable_control(ui, "offset_amount", &amount);
            let add = ui.selectable_value(&mut draft.offset_subtract, false, "Add");
            let subtract = ui.selectable_value(&mut draft.offset_subtract, true, "Subtract");
            if add.changed() || subtract.changed() {
                *changed = true;
            }
            #[cfg(test)]
            {
                self.record_focusable_control(ui, "offset_add", &add);
                self.record_focusable_control(ui, "offset_subtract", &subtract);
            }
            let previous = draft.offset_unit;
            let options_enabled = ui.is_enabled();
            let combo = egui::ComboBox::from_id_salt("date-time-studio-offset-unit")
                .selected_text(offset_unit_label(draft.offset_unit))
                .show_ui(ui, |ui| {
                    ui.add_enabled_ui(options_enabled, |ui| {
                        for unit in DATE_OFFSET_UNITS {
                            ui.selectable_value(
                                &mut draft.offset_unit,
                                unit,
                                offset_unit_label(unit),
                            );
                        }
                    });
                });
            self.host_popups
                .push(StudioPopupAnchor::new(ui, &combo.response, self.host_epoch));
            #[cfg(test)]
            self.record_focusable_control(ui, "offset_unit", &combo.response);
            if previous != draft.offset_unit {
                *changed = true;
            }
        });

        match calculate_offset_from_draft(draft) {
            Ok((result, copy_text)) => {
                ui.heading(format_offset_result(result));
                let copy = stable_action_button(ui, "copy-offset", "Copy result");
                reveal_when_focused(&copy);
                if copy.clicked() {
                    *effect = Some(DateTimeStudioEffect::CopyText(copy_text));
                }
                #[cfg(test)]
                self.record_focusable_control(ui, "offset_copy", &copy);
            }
            Err(error) => show_error(ui, &error),
        }
    }

    fn time_zones_ui(
        &mut self,
        ui: &mut egui::Ui,
        draft: &mut StudioDraft,
        changed: &mut bool,
        focus_id: &mut Option<egui::Id>,
        effect: &mut Option<DateTimeStudioEffect>,
    ) {
        ui.label(format!(
            "Offline IANA rules {} · compare the same UTC instant in each target zone.",
            crate::date_arithmetic::ZONE_DATABASE_VERSION
        ));
        let source_before = (
            draft.zones_date.clone(),
            draft.zones_time.clone(),
            draft.zones_source.clone(),
        );
        self.date_picker_ui(
            ui,
            "zones_local_date",
            "Local date",
            &mut draft.zones_date,
            &mut draft.zones_month,
            changed,
            focus_id,
        );
        self.time_input_ui(
            ui,
            "zones_local_time",
            "Local time",
            &mut draft.zones_time,
            changed,
            focus_id,
        );
        let source_zone_response = self.zone_text_input(
            ui,
            "zones_source",
            "Source zone",
            &mut draft.zones_source,
            focus_id,
        );
        if source_zone_response {
            *changed = true;
            draft.zones_fold_choice = None;
        }
        let source_suggestion_query = draft.zones_source.clone();
        self.zone_suggestions_ui(
            ui,
            "zones_source_suggestions",
            &source_suggestion_query,
            changed,
            |zone| {
                draft.zones_source = zone.name().to_owned();
                draft.zones_fold_choice = None;
            },
        );
        self.target_zones_ui(ui, draft, changed);
        if source_before
            != (
                draft.zones_date.clone(),
                draft.zones_time.clone(),
                draft.zones_source.clone(),
            )
        {
            draft.zones_fold_choice = None;
        }

        let local = match (parse_date(&draft.zones_date), parse_time(&draft.zones_time)) {
            (Ok(date), Ok(time)) => date.and_time(time),
            (Err(error), _) => {
                show_error(ui, &format!("Local date: {error}"));
                return;
            }
            (_, Err(error)) => {
                show_error(ui, &format!("Local time: {error}"));
                return;
            }
        };
        let zone = match NamedTimeZone::parse(&draft.zones_source) {
            Ok(zone) => zone,
            Err(error) => {
                show_error(ui, &error.to_string());
                return;
            }
        };
        let resolution = match resolve_local_datetime(local, zone) {
            Ok(resolution) => resolution,
            Err(error) => {
                show_error(ui, &error.to_string());
                return;
            }
        };
        let mut copy_text = None;
        let selected = match &resolution {
            LocalResolution::Single(value) => {
                ui.label(value.display_label());
                copy_text = Some(value.display_label());
                Some(value.clone())
            }
            LocalResolution::Ambiguous { earlier, later, .. } => {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    "This local wall time occurs twice. Choose a specific instant.",
                );
                ui.label(format!("Earlier — {}", earlier.display_label()));
                ui.label(format!("Later — {}", later.display_label()));
                let earlier_button = ui.selectable_value(
                    &mut draft.zones_fold_choice,
                    Some(FoldChoice::Earlier),
                    "Use earlier occurrence",
                );
                let later_button = ui.selectable_value(
                    &mut draft.zones_fold_choice,
                    Some(FoldChoice::Later),
                    "Use later occurrence",
                );
                reveal_when_focused(&earlier_button);
                reveal_when_focused(&later_button);
                if earlier_button.changed() || later_button.changed() {
                    *changed = true;
                }
                #[cfg(test)]
                {
                    self.record_focusable_control(ui, "zones_fold_earlier", &earlier_button);
                    self.record_focusable_control(ui, "zones_fold_later", &later_button);
                }
                copy_text = Some(format!(
                    "Earlier — {}\nLater — {}",
                    earlier.display_label(),
                    later.display_label()
                ));
                resolution.select(draft.zones_fold_choice).ok()
            }
            LocalResolution::Nonexistent { explanation, .. } => {
                show_error(ui, explanation);
                None
            }
        };
        if let Some(source) = selected {
            match parse_target_zones(&draft.zones_targets) {
                Ok(targets) => match convert_to_time_zones(&source, &targets) {
                    Ok(converted) => {
                        if !converted.is_empty() {
                            ui.separator();
                            ui.strong("Same instant in target zones");
                            for target in &converted {
                                ui.label(target.display_label());
                            }
                            let mut lines = vec![format!("Source — {}", source.display_label())];
                            lines.extend(converted.iter().map(|target| target.display_label()));
                            copy_text = Some(lines.join("\n"));
                        }
                    }
                    Err(error) => show_error(ui, &error.to_string()),
                },
                Err(error) => show_error(ui, &error),
            }
        }
        if let Some(copy_text) = copy_text {
            let copy = stable_action_button(ui, "copy-zones", "Copy current resolution");
            reveal_when_focused(&copy);
            if copy.clicked() {
                *effect = Some(DateTimeStudioEffect::CopyText(copy_text));
            }
            #[cfg(test)]
            self.record_focusable_control(ui, "zones_copy", &copy);
        }
    }

    fn schedule_ui(
        &mut self,
        ui: &mut egui::Ui,
        draft: &mut StudioDraft,
        changed: &mut bool,
        focus_id: &mut Option<egui::Id>,
        effect: &mut Option<DateTimeStudioEffect>,
        calendar_editor_open: bool,
    ) {
        ui.label("Preview calendar-date occurrences only. This does not create alarms or recurring events.");
        let schedule_inputs_before = (
            draft.schedule_date.clone(),
            draft.schedule_time.clone(),
            draft.schedule_zone.clone(),
            draft.schedule_cadence,
            draft.schedule_count.clone(),
        );
        self.date_picker_ui(
            ui,
            "schedule_first_date",
            "First date",
            &mut draft.schedule_date,
            &mut draft.schedule_month,
            changed,
            focus_id,
        );
        self.time_input_ui(
            ui,
            "schedule_first_time",
            "First local time",
            &mut draft.schedule_time,
            changed,
            focus_id,
        );
        let zone_changed = self.zone_text_input(
            ui,
            "schedule_zone",
            "Named zone",
            &mut draft.schedule_zone,
            focus_id,
        );
        if zone_changed {
            *changed = true;
            draft.selected_schedule_index = None;
            draft.schedule_fold_choice = None;
        }
        let schedule_zone_query = draft.schedule_zone.clone();
        self.zone_suggestions_ui(
            ui,
            "schedule_zone_suggestions",
            &schedule_zone_query,
            changed,
            |zone| {
                draft.schedule_zone = zone.name().to_owned();
                draft.selected_schedule_index = None;
                draft.schedule_fold_choice = None;
            },
        );
        ui.horizontal_wrapped(|ui| {
            let prior_cadence = draft.schedule_cadence;
            let options_enabled = ui.is_enabled();
            let combo = egui::ComboBox::from_id_salt("date-time-studio-schedule-cadence")
                .selected_text(match draft.schedule_cadence {
                    ScheduleCadence::Daily => "Daily",
                    ScheduleCadence::Weekly => "Weekly",
                })
                .show_ui(ui, |ui| {
                    ui.add_enabled_ui(options_enabled, |ui| {
                        ui.selectable_value(
                            &mut draft.schedule_cadence,
                            ScheduleCadence::Daily,
                            "Daily",
                        );
                        ui.selectable_value(
                            &mut draft.schedule_cadence,
                            ScheduleCadence::Weekly,
                            "Weekly",
                        );
                    });
                });
            self.host_popups
                .push(StudioPopupAnchor::new(ui, &combo.response, self.host_epoch));
            #[cfg(test)]
            self.record_focusable_control(ui, "schedule_cadence", &combo.response);
            if draft.schedule_cadence != prior_cadence {
                *changed = true;
            }
            let id = ui.make_persistent_id("date-time-studio-schedule-count");
            let count = ui.add(
                egui::TextEdit::singleline(&mut draft.schedule_count)
                    .id(id)
                    .desired_width(50.0)
                    .char_limit(2),
            );
            if count.changed() {
                *changed = true;
            }
            #[cfg(test)]
            self.record_focusable_control(ui, "schedule_count", &count);
            ui.label("occurrences (1–32)");
        });
        if schedule_inputs_before
            != (
                draft.schedule_date.clone(),
                draft.schedule_time.clone(),
                draft.schedule_zone.clone(),
                draft.schedule_cadence,
                draft.schedule_count.clone(),
            )
        {
            draft.selected_schedule_index = None;
            draft.schedule_fold_choice = None;
        }
        let request = match schedule_request(draft) {
            Ok(request) => request,
            Err(error) => {
                show_error(ui, &error);
                return;
            }
        };
        let occurrences = match preview_schedule(request) {
            Ok(occurrences) => occurrences,
            Err(error) => {
                show_error(ui, &error.to_string());
                return;
            }
        };
        let mut focused_row_rect = None;
        egui::ScrollArea::vertical()
            .id_salt("date-time-studio-schedule-results")
            .auto_shrink([false, false])
            .max_height(ui.available_height().min(240.0))
            .show(ui, |ui| {
                for occurrence in &occurrences {
                    let selected = draft.selected_schedule_index == Some(occurrence.index);
                    let response = ui.selectable_label(
                        selected,
                        format!(
                            "{} — {}",
                            occurrence.requested_local.format("%a %Y-%m-%d %H:%M:%S%.f"),
                            resolution_summary(&occurrence.resolution)
                        ),
                    );
                    if response.gained_focus() {
                        response.scroll_to_me(Some(egui::Align::Center));
                        focused_row_rect = Some(response.rect);
                    }
                    if response.clicked() && !selected {
                        draft.selected_schedule_index = Some(occurrence.index);
                        draft.schedule_fold_choice = None;
                        *changed = true;
                    }
                    #[cfg(test)]
                    self.record_focusable_control(
                        ui,
                        format!("schedule_row_{}", occurrence.index),
                        &response,
                    );
                }
            });
        if let Some(rect) = focused_row_rect {
            ui.scroll_to_rect(rect, Some(egui::Align::Center));
        }
        let Some(index) = draft.selected_schedule_index else {
            return;
        };
        let Some(occurrence) = occurrences.get(index) else {
            return;
        };

        let mut selected_zoned = None;
        let mut copy_text = None;
        match &occurrence.resolution {
            LocalResolution::Single(value) => {
                selected_zoned = Some(value.clone());
                copy_text = Some(value.display_label());
                ui.label(value.display_label());
            }
            LocalResolution::Ambiguous { earlier, later, .. } => {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    "Choose which instant this repeated local time means.",
                );
                let earlier_button = ui.selectable_value(
                    &mut draft.schedule_fold_choice,
                    Some(FoldChoice::Earlier),
                    format!("Earlier — {}", earlier.display_label()),
                );
                let later_button = ui.selectable_value(
                    &mut draft.schedule_fold_choice,
                    Some(FoldChoice::Later),
                    format!("Later — {}", later.display_label()),
                );
                reveal_when_focused(&earlier_button);
                reveal_when_focused(&later_button);
                if earlier_button.changed() || later_button.changed() {
                    *changed = true;
                }
                #[cfg(test)]
                {
                    self.record_focusable_control(ui, "schedule_fold_earlier", &earlier_button);
                    self.record_focusable_control(ui, "schedule_fold_later", &later_button);
                }
                selected_zoned = occurrence
                    .resolution
                    .select(draft.schedule_fold_choice)
                    .ok();
                if let Some(selected) = &selected_zoned {
                    copy_text = Some(format!(
                        "Requested local time {}\nSelected instant — {}",
                        occurrence.requested_local.format("%Y-%m-%d %H:%M:%S%.f"),
                        selected.display_label()
                    ));
                }
            }
            LocalResolution::Nonexistent { explanation, .. } => {
                show_error(ui, explanation);
            }
        }
        if let Some(copy_text) = copy_text.take() {
            let copy = stable_action_button(ui, "copy-schedule", "Copy selected occurrence");
            reveal_when_focused(&copy);
            if copy.clicked() {
                *effect = Some(DateTimeStudioEffect::CopyText(copy_text));
            }
            #[cfg(test)]
            self.record_focusable_control(ui, "schedule_copy", &copy);
        }
        let Some(selected) = selected_zoned else {
            if calendar_editor_open {
                ui.label("Close the open Calendar Event editor before preparing another entry.");
            }
            return;
        };
        match machine_local_datetime(selected.instant_utc) {
            Ok(local) => {
                ui.label(format!(
                    "Calendar preview: {} in this computer's local time. Calendar entries do not store a named zone.",
                    local.format("%Y-%m-%d %H:%M:%S%.f")
                ));
                if local.time().second() != 0 || local.time().nanosecond() != 0 {
                    show_error(
                        ui,
                        "This occurrence resolves to sub-minute local time; the calendar editor currently accepts minute-aligned times.",
                    );
                } else if calendar_editor_open {
                    show_error(
                        ui,
                        "The Calendar Event editor is already open. Close it before preparing this entry.",
                    );
                } else {
                    let prepare = stable_action_button(
                        ui,
                        "prepare-calendar",
                        "Prepare unsaved calendar entry…",
                    );
                    reveal_when_focused(&prepare);
                    if prepare.clicked() {
                        *effect = Some(DateTimeStudioEffect::PrepareCalendar {
                            instant_utc: selected.instant_utc,
                        });
                    }
                    #[cfg(test)]
                    self.record_focusable_control(ui, "schedule_prepare_calendar", &prepare);
                }
            }
            Err(error) => show_error(ui, &error),
        }
    }

    fn date_picker_ui(
        &mut self,
        ui: &mut egui::Ui,
        control_id: &'static str,
        label: &'static str,
        text: &mut String,
        month: &mut NaiveDate,
        changed: &mut bool,
        focus_id: &mut Option<egui::Id>,
    ) {
        let source_before = text.clone();
        ui.scope_builder(
            egui::UiBuilder::new()
                .id(egui::Id::new(("date-time-studio-date", control_id)))
                .layout(egui::Layout::top_down(egui::Align::Min)),
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(label);
                    let id = ui.make_persistent_id("date-field");
                    if focus_id.is_none() {
                        *focus_id = Some(id);
                    }
                    let response = ui.add(
                        egui::TextEdit::singleline(text)
                            .id(id)
                            .desired_width(120.0)
                            .char_limit(10),
                    );
                    reveal_when_focused(&response);
                    if response.changed() {
                        *changed = true;
                    }
                    #[cfg(test)]
                    self.record_focusable_control(ui, format!("{control_id}_input"), &response);
                });
                if source_before != *text
                    && let Ok(date) = parse_date(text)
                {
                    *month = first_of_month(date);
                }
                let before_date = text.clone();
                let _header = egui::CollapsingHeader::new("Choose date…")
                    .id_salt("calendar-picker")
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            let previous = ui.button("‹");
                            reveal_when_focused(&previous);
                            if previous.clicked()
                                && let Some(next) = month.checked_sub_months(Months::new(1))
                            {
                                *month = first_of_month(next);
                            }
                            #[cfg(test)]
                            self.record_control(
                                format!("{control_id}_month_previous"),
                                previous.rect,
                            );
                            ui.label(month.format("%B %Y").to_string());
                            let next = ui.button("›");
                            reveal_when_focused(&next);
                            if next.clicked()
                                && let Some(next_month) = month.checked_add_months(Months::new(1))
                            {
                                *month = first_of_month(next_month);
                            }
                            #[cfg(test)]
                            self.record_control(format!("{control_id}_month_next"), next.rect);
                        });
                        let first_weekday = month.weekday().num_days_from_monday() as i32;
                        let days = days_in_month(*month);
                        egui::Grid::new(("date-time-studio-calendar-grid", control_id))
                            .num_columns(7)
                            .spacing([3.0, 3.0])
                            .show(ui, |ui| {
                                for weekday in [
                                    Weekday::Mon,
                                    Weekday::Tue,
                                    Weekday::Wed,
                                    Weekday::Thu,
                                    Weekday::Fri,
                                    Weekday::Sat,
                                    Weekday::Sun,
                                ] {
                                    ui.label(weekday.to_string());
                                }
                                ui.end_row();
                                let selected = parse_date(text).ok();
                                for cell in 0..42 {
                                    if cell > 0 && cell % 7 == 0 {
                                        ui.end_row();
                                    }
                                    let day = cell - first_weekday + 1;
                                    if day < 1 || day > days as i32 {
                                        ui.label(" ");
                                        continue;
                                    }
                                    let Some(date) = NaiveDate::from_ymd_opt(
                                        month.year(),
                                        month.month(),
                                        day as u32,
                                    ) else {
                                        ui.label(" ");
                                        continue;
                                    };
                                    let response = ui
                                        .selectable_label(selected == Some(date), day.to_string());
                                    reveal_when_focused(&response);
                                    if response.clicked() {
                                        *text = date.format("%Y-%m-%d").to_string();
                                        *changed = true;
                                    }
                                    #[cfg(test)]
                                    self.record_control(
                                        format!("{control_id}_day_{day}"),
                                        response.rect,
                                    );
                                }
                            });
                    });
                if before_date != *text
                    && let Ok(date) = parse_date(text)
                {
                    *month = first_of_month(date);
                }
                #[cfg(test)]
                self.record_focusable_control(
                    ui,
                    format!("{control_id}_calendar_toggle"),
                    &_header.header_response,
                );
            },
        );
    }

    fn time_input_ui(
        &mut self,
        ui: &mut egui::Ui,
        control_id: &'static str,
        label: &'static str,
        text: &mut String,
        changed: &mut bool,
        focus_id: &mut Option<egui::Id>,
    ) {
        ui.horizontal_wrapped(|ui| {
            ui.label(label);
            let id = ui.make_persistent_id(("date-time-studio-time", control_id));
            if focus_id.is_none() {
                *focus_id = Some(id);
            }
            let response = ui.add(
                egui::TextEdit::singleline(text)
                    .id(id)
                    .desired_width(120.0)
                    .char_limit(32),
            );
            reveal_when_focused(&response);
            if response.changed() {
                *changed = true;
            }
            #[cfg(test)]
            self.record_focusable_control(ui, format!("{control_id}_input"), &response);
            if let Ok(time) = parse_time(text) {
                let mut hour = time.hour();
                let mut minute = time.minute();
                let hours = ui.add(egui::DragValue::new(&mut hour).range(0..=23).prefix("H "));
                let minutes = ui.add(egui::DragValue::new(&mut minute).range(0..=59).prefix("M "));
                reveal_when_focused(&hours);
                reveal_when_focused(&minutes);
                if hours.changed() || minutes.changed() {
                    *text = format_time_parts(hour, minute, time.second(), time.nanosecond());
                    *changed = true;
                }
                #[cfg(test)]
                {
                    self.record_control(format!("{control_id}_hour"), hours.rect);
                    self.record_control(format!("{control_id}_minute"), minutes.rect);
                }
            }
        });
    }

    fn zone_text_input(
        &mut self,
        ui: &mut egui::Ui,
        control_id: &'static str,
        label: &'static str,
        text: &mut String,
        focus_id: &mut Option<egui::Id>,
    ) -> bool {
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.label(label);
            let id = ui.make_persistent_id(("date-time-studio-zone", control_id));
            if focus_id.is_none() {
                *focus_id = Some(id);
            }
            let response = ui.add(
                egui::TextEdit::singleline(text)
                    .id(id)
                    .desired_width(220.0)
                    .char_limit(crate::date_arithmetic::MAX_ZONE_TEXT_BYTES),
            );
            reveal_when_focused(&response);
            changed = response.changed();
            #[cfg(test)]
            self.record_focusable_control(ui, format!("{control_id}_input"), &response);
        });
        changed
    }

    fn zone_suggestions_ui(
        &mut self,
        ui: &mut egui::Ui,
        _control_id: &'static str,
        query: &str,
        changed: &mut bool,
        mut select: impl FnMut(NamedTimeZone),
    ) {
        match search_time_zones(query, ZONE_SUGGESTION_LIMIT) {
            Ok(zones) if !zones.is_empty() => {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Matches");
                    for zone in zones {
                        let response = ui.button(zone.name());
                        reveal_when_focused(&response);
                        if response.clicked() {
                            select(zone);
                            *changed = true;
                        }
                        #[cfg(test)]
                        self.record_control(
                            format!("{_control_id}_{}", zone.name()),
                            response.rect,
                        );
                    }
                });
            }
            Ok(_) => {}
            Err(error) => show_error(ui, &error.to_string()),
        }
    }

    fn target_zones_ui(&mut self, ui: &mut egui::Ui, draft: &mut StudioDraft, changed: &mut bool) {
        ui.horizontal_wrapped(|ui| {
            ui.label("Search target zones");
            let id = ui.make_persistent_id("date-time-studio-target-search");
            let response = ui.add(
                egui::TextEdit::singleline(&mut draft.zones_target_search)
                    .id(id)
                    .desired_width(180.0)
                    .char_limit(crate::date_arithmetic::MAX_ZONE_TEXT_BYTES),
            );
            reveal_when_focused(&response);
            if response.changed() {
                *changed = true;
            }
            #[cfg(test)]
            self.record_focusable_control(ui, "zones_target_search", &response);
        });
        if draft.zones_targets.len() >= ZONE_TARGET_LIMIT {
            ui.label(format!("Target limit: {ZONE_TARGET_LIMIT} zones."));
        } else if let Ok(matches) =
            search_time_zones(&draft.zones_target_search, ZONE_SUGGESTION_LIMIT)
        {
            ui.horizontal_wrapped(|ui| {
                for zone in matches {
                    if draft.zones_targets.iter().any(|name| name == zone.name()) {
                        continue;
                    }
                    let add = ui.button(format!("+ {}", zone.name()));
                    reveal_when_focused(&add);
                    if add.clicked() && draft.zones_targets.len() < ZONE_TARGET_LIMIT {
                        draft.zones_targets.push(zone.name().to_owned());
                        *changed = true;
                    }
                    #[cfg(test)]
                    self.record_focusable_control(ui, format!("zones_add_{}", zone.name()), &add);
                }
            });
        }
        ui.horizontal_wrapped(|ui| {
            for (index, zone) in draft.zones_targets.iter().enumerate() {
                ui.label(zone);
                let remove = ui.small_button("Remove");
                reveal_when_focused(&remove);
                if remove.clicked() {
                    draft.zones_targets.remove(index);
                    *changed = true;
                    break;
                }
                #[cfg(test)]
                self.record_focusable_control(ui, format!("zones_remove_{index}"), &remove);
            }
        });
    }

    #[cfg(test)]
    fn record_control(&mut self, id: impl Into<String>, rect: egui::Rect) {
        self.test_controls.push((id.into(), rect));
    }

    #[cfg(test)]
    fn record_focusable_control(
        &mut self,
        ui: &egui::Ui,
        id: impl Into<String>,
        response: &egui::Response,
    ) {
        let id = id.into();
        if id == "schedule_prepare_calendar" && response.enabled() {
            self.test_prepare_observation = Some(TestPrepareUiObservation {
                rect: response.rect,
                clip_rect: ui.clip_rect(),
                enabled: response.enabled(),
                clicked: response.clicked(),
                interact_pointer_pos: response.interact_pointer_pos(),
            });
        }
        self.test_controls.push((id.clone(), response.rect));
        self.test_focusable_controls
            .push((id, response.id, response.rect, ui.clip_rect()));
    }
}

fn calculate_between_from_draft(
    draft: &StudioDraft,
) -> Result<crate::date_arithmetic::DateBetweenResult, String> {
    let start_date =
        parse_date(&draft.between_start_date).map_err(|e| format!("Start date: {e}"))?;
    let end_date = parse_date(&draft.between_end_date).map_err(|e| format!("End date: {e}"))?;
    let (start_time, end_time) = if draft.between_include_time {
        (
            Some(parse_time(&draft.between_start_time).map_err(|e| format!("Start time: {e}"))?),
            Some(parse_time(&draft.between_end_time).map_err(|e| format!("End time: {e}"))?),
        )
    } else {
        (None, None)
    };
    evaluate_between(DateBetweenRequest {
        first: DateBetweenEndpoint {
            value: DateValue::Date(start_date),
            explicit_time: start_time,
        },
        second: DateBetweenEndpoint {
            value: DateValue::Date(end_date),
            explicit_time: end_time,
        },
    })
    .map_err(|error| error.to_string())
}

fn calculate_offset_from_draft(draft: &StudioDraft) -> Result<(DateValue, String), String> {
    let reference = draft
        .reference_now
        .ok_or_else(|| "The reference snapshot is not initialized.".to_owned())?;
    let anchor =
        parse_anchor(&draft.offset_anchor, reference).map_err(|error| error.to_string())?;
    let anchor = match (anchor, draft.offset_include_time) {
        (DateValue::Date(date), true) => DateValue::DateTime(
            date.and_time(parse_time(&draft.offset_time).map_err(|e| format!("Anchor time: {e}"))?),
        ),
        (DateValue::DateTime(date_time), true) => DateValue::DateTime(date_time),
        (DateValue::Date(date), false) => DateValue::Date(date),
        (DateValue::DateTime(date_time), false) => DateValue::Date(date_time.date()),
    };
    let magnitude = draft
        .offset_amount
        .trim()
        .parse::<i64>()
        .ok()
        .filter(|amount| *amount >= 0)
        .ok_or_else(|| "Enter a nonnegative whole-number amount.".to_owned())?;
    let amount = if draft.offset_subtract {
        magnitude
            .checked_neg()
            .ok_or_else(|| "Offset amount is outside the supported range.".to_owned())?
    } else {
        magnitude
    };
    let result = apply_offset_request(
        anchor,
        DateOffsetRequest {
            amount,
            unit: draft.offset_unit,
        },
    )
    .map_err(|error| error.to_string())?;
    let copy_text = if draft.offset_subtract {
        format!(
            "Subtract {magnitude} {} from {} = {}",
            offset_unit_label(draft.offset_unit),
            format_date_value(anchor),
            format_offset_result(result)
        )
    } else {
        format!(
            "Add {magnitude} {} to {} = {}",
            offset_unit_label(draft.offset_unit),
            format_date_value(anchor),
            format_offset_result(result)
        )
    };
    Ok((result, copy_text))
}

fn schedule_request(draft: &StudioDraft) -> Result<SchedulePreviewRequest, String> {
    let date = parse_date(&draft.schedule_date).map_err(|error| format!("First date: {error}"))?;
    let time = parse_time(&draft.schedule_time).map_err(|error| format!("First time: {error}"))?;
    let zone = NamedTimeZone::parse(&draft.schedule_zone).map_err(|error| error.to_string())?;
    let count = draft
        .schedule_count
        .trim()
        .parse::<usize>()
        .map_err(|_| "Occurrence count must be between 1 and 32.".to_owned())?;
    Ok(SchedulePreviewRequest {
        first_local: date.and_time(time),
        zone,
        cadence: draft.schedule_cadence,
        count,
    })
}

fn parse_target_zones(names: &[String]) -> Result<Vec<NamedTimeZone>, String> {
    if names.len() > ZONE_TARGET_LIMIT {
        return Err(format!(
            "At most {ZONE_TARGET_LIMIT} target zones are supported."
        ));
    }
    names
        .iter()
        .map(|name| NamedTimeZone::parse(name).map_err(|error| error.to_string()))
        .collect()
}

fn parse_date(input: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(input.trim(), "%Y-%m-%d")
        .map_err(|_| "Use a valid YYYY-MM-DD date.".into())
}

fn parse_time(input: &str) -> Result<NaiveTime, String> {
    ["%H:%M", "%H:%M:%S%.f", "%H:%M:%S"]
        .into_iter()
        .find_map(|format| NaiveTime::parse_from_str(input.trim(), format).ok())
        .ok_or_else(|| "Use HH:MM or HH:MM:SS[.fraction].".into())
}

fn format_time(time: NaiveTime) -> String {
    if time.second() == 0 && time.nanosecond() == 0 {
        time.format("%H:%M").to_string()
    } else {
        let value = time.format("%H:%M:%S%.f").to_string();
        let Some((whole, fraction)) = value.split_once('.') else {
            return value;
        };
        let fraction = fraction.trim_end_matches('0');
        if fraction.is_empty() {
            whole.to_owned()
        } else {
            format!("{whole}.{fraction}")
        }
    }
}

fn format_time_parts(hour: u32, minute: u32, second: u32, nanosecond: u32) -> String {
    NaiveTime::from_hms_nano_opt(hour, minute, second, nanosecond)
        .map(format_time)
        .unwrap_or_else(|| format!("{hour:02}:{minute:02}"))
}

fn format_f64(value: f64) -> String {
    crate::common::number_format::format_number(value).unwrap_or_else(|| "unavailable".into())
}

fn format_date_value(value: DateValue) -> String {
    match value {
        DateValue::Date(date) => date.format("%A, %B %-d, %Y").to_string(),
        DateValue::DateTime(date_time) => format!(
            "{} {}",
            date_time.date().format("%A, %B %-d, %Y"),
            format_time(date_time.time())
        ),
    }
}

fn format_offset_result(value: DateValue) -> String {
    match value {
        DateValue::Date(date) => format!("{} ({})", date.format("%Y-%m-%d"), date.weekday()),
        DateValue::DateTime(date_time) => format!(
            "{} {} ({})",
            date_time.date().format("%Y-%m-%d"),
            format_time(date_time.time()),
            date_time.weekday()
        ),
    }
}

fn format_signed_duration(duration: chrono::Duration) -> String {
    let negative = duration < chrono::Duration::zero();
    let absolute = if negative { -duration } else { duration };
    let days = absolute.num_days();
    let remainder = absolute - chrono::Duration::days(days);
    let hours = remainder.num_hours();
    let remainder = remainder - chrono::Duration::hours(hours);
    let minutes = remainder.num_minutes();
    let remainder = remainder - chrono::Duration::minutes(minutes);
    let whole_seconds = remainder.num_seconds();
    let nanos = remainder.subsec_nanos().unsigned_abs();
    let sign = if negative { "−" } else { "" };
    let seconds = if nanos == 0 {
        format!("{whole_seconds}s")
    } else {
        let fraction = format!("{nanos:09}").trim_end_matches('0').to_owned();
        format!("{whole_seconds}.{fraction}s")
    };
    if days > 0 {
        format!("{sign}{days}d {hours:02}:{minutes:02}:{seconds}")
    } else {
        format!("{sign}{hours:02}:{minutes:02}:{seconds}")
    }
}

fn format_calendar_span(span: crate::date_arithmetic::CalendarSpan) -> String {
    let sign = if span.sign < 0 { "−" } else { "" };
    let months = if span.complete_months == 1 {
        "month"
    } else {
        "months"
    };
    format!(
        "Calendar span: {sign}{} complete clipped {months}, then {}",
        span.complete_months,
        format_signed_duration(span.remaining)
    )
}

fn offset_unit_label(unit: DateOffsetUnit) -> &'static str {
    match unit {
        DateOffsetUnit::Day => "days",
        DateOffsetUnit::Week => "weeks",
        DateOffsetUnit::Month => "months",
        DateOffsetUnit::Year => "years",
        DateOffsetUnit::Hour => "hours",
        DateOffsetUnit::Minute => "minutes",
        DateOffsetUnit::Second => "seconds",
        DateOffsetUnit::Millisecond => "milliseconds",
        DateOffsetUnit::Microsecond => "microseconds",
        DateOffsetUnit::Nanosecond => "nanoseconds",
    }
}

const DATE_OFFSET_UNITS: [DateOffsetUnit; 10] = [
    DateOffsetUnit::Day,
    DateOffsetUnit::Week,
    DateOffsetUnit::Month,
    DateOffsetUnit::Year,
    DateOffsetUnit::Hour,
    DateOffsetUnit::Minute,
    DateOffsetUnit::Second,
    DateOffsetUnit::Millisecond,
    DateOffsetUnit::Microsecond,
    DateOffsetUnit::Nanosecond,
];

fn resolution_summary(resolution: &LocalResolution) -> &'static str {
    match resolution {
        LocalResolution::Single(_) => "resolved",
        LocalResolution::Ambiguous { .. } => "repeated local time; choose earlier or later",
        LocalResolution::Nonexistent { .. } => "nonexistent local time",
    }
}

fn value_date(value: DateValue) -> NaiveDate {
    match value {
        DateValue::Date(date) => date,
        DateValue::DateTime(date_time) => date_time.date(),
    }
}

fn first_of_month(date: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(date.year(), date.month(), 1).unwrap_or(date)
}

fn days_in_month(month: NaiveDate) -> u32 {
    (28..=31)
        .rev()
        .find(|day| NaiveDate::from_ymd_opt(month.year(), month.month(), *day).is_some())
        .unwrap_or(28)
}

pub(super) fn machine_local_datetime(instant_utc: DateTime<Utc>) -> Result<NaiveDateTime, String> {
    let instant = instant_utc.naive_utc();
    let offset = Local.offset_from_utc_datetime(&instant).fix();
    instant.checked_add_offset(offset).ok_or_else(|| {
        "The selected instant is outside the local calendar's supported date range.".into()
    })
}

fn studio_root_id() -> egui::Id {
    egui::Id::new("date-time-studio")
}

fn stable_action_scope(id: egui::Id) -> egui::UiBuilder {
    egui::UiBuilder::new()
        .id(id)
        .layout(egui::Layout::left_to_right(egui::Align::Min))
}

fn stable_wrapped_scope(id: egui::Id) -> egui::UiBuilder {
    egui::UiBuilder::new()
        .id(id)
        .layout(egui::Layout::top_down(egui::Align::Min))
}

fn stable_action_button(ui: &mut egui::Ui, id: &'static str, label: &str) -> egui::Response {
    ui.scope_builder(
        stable_action_scope(studio_root_id().with(("action", id))),
        |ui| ui.button(label),
    )
    .inner
}

fn show_error(ui: &mut egui::Ui, error: &str) {
    ui.colored_label(ui.visuals().error_fg_color, error);
}

fn reveal_when_focused(response: &egui::Response) {
    if response.gained_focus() {
        response.scroll_to_me(Some(egui::Align::Center));
    } else if response.has_focus() && !response.interact_rect.contains_rect(response.rect) {
        response.scroll_to_me(None);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DateTimeStudioDialog, DateTimeStudioEffect, StudioMode, calculate_between_from_draft,
        calculate_offset_from_draft, machine_local_datetime, parse_date, parse_time,
        schedule_request,
    };
    use crate::date_arithmetic::{DateOffsetUnit, DateValue, FoldChoice, LocalResolution};
    use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
    use eframe::egui;

    fn reference() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 25)
            .unwrap()
            .and_hms_nano_opt(9, 15, 30, 125_000_000)
            .unwrap()
    }

    fn input(size: egui::Vec2, events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            focused: true,
            events,
            ..Default::default()
        }
    }

    fn frame(
        dialog: &mut DateTimeStudioDialog,
        ctx: &egui::Context,
        size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> Option<DateTimeStudioEffect> {
        let output = ctx.run_ui(input(size, events), |ui| {
            let _ = dialog.show(ui.ctx(), ui.ctx().current_pass_index() == 0, false);
        });
        let effect = dialog.on_ui_frame_complete(ctx);
        output.drop_without_applying_deltas();
        effect
    }

    fn settle(dialog: &mut DateTimeStudioDialog, ctx: &egui::Context, size: egui::Vec2) {
        for _ in 0..3 {
            frame(dialog, ctx, size, Vec::new());
        }
    }

    fn click_events(point: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
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

    fn click(dialog: &mut DateTimeStudioDialog, ctx: &egui::Context, size: egui::Vec2, id: &str) {
        let rect = dialog
            .test_controls
            .iter()
            .find(|(control, _)| control == id)
            .map(|(_, rect)| *rect)
            .unwrap_or_else(|| panic!("missing test control {id}"));
        frame(dialog, ctx, size, click_events(rect.center(), true));
        frame(dialog, ctx, size, click_events(rect.center(), false));
    }

    fn is_fully_visible(rect: egui::Rect, clip: egui::Rect) -> bool {
        clip.min.x <= rect.min.x
            && clip.min.y <= rect.min.y
            && clip.max.x >= rect.max.x
            && clip.max.y >= rect.max.y
    }

    fn assert_focusable_controls_visible(dialog: &DateTimeStudioDialog) {
        for (id, _, rect, clip) in &dialog.test_focusable_controls {
            assert!(
                is_fully_visible(*rect, *clip),
                "{id} is clipped: rect={rect:?}, clip={clip:?}"
            );
        }
    }

    fn initialized_dialog() -> DateTimeStudioDialog {
        let mut dialog = DateTimeStudioDialog::default();
        dialog.draft.initialize_reference(reference());
        dialog.open = true;
        dialog
    }

    #[test]
    fn date_time_studio_date_picker_and_keyboard_time_edit_update_between_inputs() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        settle(&mut dialog, &ctx, size);
        click(&mut dialog, &ctx, size, "between_start_calendar_toggle");
        frame(&mut dialog, &ctx, size, Vec::new());
        click(&mut dialog, &ctx, size, "between_start_day_12");
        assert_eq!(dialog.draft.between_start_date, "2026-10-12");

        click(&mut dialog, &ctx, size, "between_include_time");
        frame(&mut dialog, &ctx, size, Vec::new());
        let time_rect = dialog
            .test_controls
            .iter()
            .find(|(id, _)| id == "between_start_time_input")
            .map(|(_, rect)| *rect)
            .unwrap();
        frame(
            &mut dialog,
            &ctx,
            size,
            click_events(time_rect.center(), true),
        );
        frame(
            &mut dialog,
            &ctx,
            size,
            click_events(time_rect.center(), false),
        );
        let mut modifiers = egui::Modifiers::NONE;
        modifiers.command = true;
        frame(
            &mut dialog,
            &ctx,
            size,
            vec![
                egui::Event::Key {
                    key: egui::Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                },
                egui::Event::Text("08:45".into()),
            ],
        );
        assert_eq!(dialog.draft.between_start_time, "08:45");
        let result = calculate_between_from_draft(&dialog.draft).unwrap();
        assert!(matches!(result.first, DateValue::DateTime(_)));
    }

    #[test]
    fn date_time_studio_date_picker_month_navigation_survives_idle_frames() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        settle(&mut dialog, &ctx, size);
        click(&mut dialog, &ctx, size, "between_start_calendar_toggle");
        frame(&mut dialog, &ctx, size, Vec::new());
        click(&mut dialog, &ctx, size, "between_start_month_previous");
        frame(&mut dialog, &ctx, size, Vec::new());
        assert_eq!(dialog.draft.between_start_month.month(), 9);
        click(&mut dialog, &ctx, size, "between_start_day_7");
        assert_eq!(dialog.draft.between_start_date, "2026-09-07");
    }

    #[test]
    fn date_time_studio_between_and_offset_use_the_accepted_domain_owners() {
        let mut dialog = initialized_dialog();
        dialog.draft.between_start_date = "2024-01-31".into();
        dialog.draft.between_end_date = "2024-03-30".into();
        let between = calculate_between_from_draft(&dialog.draft).unwrap();
        assert_eq!(between.calendar_span.complete_months, 1);
        assert_eq!(between.calendar_span.remaining, chrono::Duration::days(30));

        dialog.draft.offset_anchor = "2024-01-31".into();
        dialog.draft.offset_picker_date = "2024-01-31".into();
        dialog.draft.offset_include_time = false;
        dialog.draft.offset_amount = "1".into();
        dialog.draft.offset_unit = DateOffsetUnit::Month;
        let (offset, copy) = calculate_offset_from_draft(&dialog.draft).unwrap();
        assert_eq!(
            offset,
            DateValue::Date(NaiveDate::from_ymd_opt(2024, 2, 29).unwrap())
        );
        assert!(copy.contains("2024-02-29"));
    }

    #[test]
    fn date_time_studio_refresh_and_mode_changes_preserve_authoritative_anchor_drafts() {
        let mut dialog = initialized_dialog();
        dialog.draft.offset_anchor = "not a valid anchor".into();
        dialog.draft.offset_picker_date = "2024-01-31".into();
        dialog.draft.offset_picker_month = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let original_reference = dialog.draft.reference_now.unwrap();
        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        frame(&mut dialog, &ctx, size, Vec::new());
        click(&mut dialog, &ctx, size, "mode_AddSubtract");
        frame(&mut dialog, &ctx, size, Vec::new());
        assert_eq!(dialog.draft.offset_anchor, "not a valid anchor");

        click(&mut dialog, &ctx, size, "refresh_reference");
        assert_eq!(dialog.draft.offset_anchor, "not a valid anchor");
        assert_eq!(dialog.draft.offset_picker_date, "2024-01-31");
        assert_ne!(dialog.draft.reference_now, Some(original_reference));

        dialog.draft.reference_now = Some(original_reference);
        dialog.draft.offset_anchor = "now".into();
        dialog.draft.offset_picker_date = original_reference.date().to_string();
        // The raw anchor owns the time for `now`; a stale picker value must
        // not replace the newly refreshed frozen reference time.
        dialog.draft.offset_time = "12:01".into();
        dialog.draft.offset_include_time = true;
        dialog.draft.offset_subtract = false;
        dialog.draft.offset_amount = "1".into();
        dialog.draft.offset_unit = DateOffsetUnit::Day;
        let (result, _) = calculate_offset_from_draft(&dialog.draft).unwrap();
        assert_eq!(
            result,
            DateValue::DateTime(original_reference + chrono::Duration::days(1))
        );
    }

    #[test]
    fn date_time_studio_invalid_inputs_keep_the_source_and_do_not_produce_stale_copy() {
        let mut dialog = initialized_dialog();
        dialog.draft.between_start_date = "2026-02-31".into();
        assert_eq!(dialog.draft.between_start_date, "2026-02-31");
        assert!(calculate_between_from_draft(&dialog.draft).is_err());
        assert!(parse_date("2026-02-31").is_err());
        assert!(parse_time("25:90").is_err());
    }

    #[test]
    fn date_time_studio_fold_choice_is_explicit_and_clears_when_source_changes() {
        let mut dialog = initialized_dialog();
        dialog.draft.mode = StudioMode::TimeZones;
        dialog.draft.zones_date = "2026-11-01".into();
        dialog.draft.zones_time = "01:30".into();
        dialog.draft.zones_source = "America/New_York".into();
        let zone = crate::date_arithmetic::NamedTimeZone::parse("America/New_York").unwrap();
        let local = parse_date(&dialog.draft.zones_date)
            .unwrap()
            .and_time(parse_time(&dialog.draft.zones_time).unwrap());
        let resolution = crate::date_arithmetic::resolve_local_datetime(local, zone).unwrap();
        assert!(matches!(resolution, LocalResolution::Ambiguous { .. }));
        assert!(resolution.select(None).is_err());
        assert!(resolution.select(Some(FoldChoice::Earlier)).is_ok());

        dialog.draft.zones_fold_choice = None;
        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        settle(&mut dialog, &ctx, size);
        click(&mut dialog, &ctx, size, "zones_fold_later");
        assert_eq!(dialog.draft.zones_fold_choice, Some(FoldChoice::Later));
        let time_rect = dialog
            .test_controls
            .iter()
            .find(|(id, _)| id == "zones_local_time_input")
            .map(|(_, rect)| *rect)
            .unwrap();
        frame(
            &mut dialog,
            &ctx,
            size,
            click_events(time_rect.center(), true),
        );
        frame(
            &mut dialog,
            &ctx,
            size,
            click_events(time_rect.center(), false),
        );
        let mut modifiers = egui::Modifiers::NONE;
        modifiers.command = true;
        frame(
            &mut dialog,
            &ctx,
            size,
            vec![
                egui::Event::Key {
                    key: egui::Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                },
                egui::Event::Text("01:31".into()),
            ],
        );
        assert_eq!(dialog.draft.zones_time, "01:31");
        assert_eq!(dialog.draft.zones_fold_choice, None);
    }

    #[test]
    fn date_time_studio_schedule_preview_bounds_count_and_keeps_gap_occurrences() {
        let mut dialog = initialized_dialog();
        dialog.draft.schedule_date = "2026-03-07".into();
        dialog.draft.schedule_time = "02:30".into();
        dialog.draft.schedule_zone = "America/New_York".into();
        dialog.draft.schedule_count = "3".into();
        let request = schedule_request(&dialog.draft).unwrap();
        let occurrences = crate::date_arithmetic::preview_schedule(request).unwrap();
        assert_eq!(occurrences.len(), 3);
        assert!(matches!(
            occurrences[1].resolution,
            LocalResolution::Nonexistent { .. }
        ));
        dialog.draft.schedule_count = "33".into();
        assert!(
            crate::date_arithmetic::preview_schedule(schedule_request(&dialog.draft).unwrap())
                .is_err()
        );
    }

    #[test]
    fn date_time_studio_schedule_selection_survives_and_copy_matches_selected_row() {
        let mut dialog = initialized_dialog();
        dialog.draft.mode = StudioMode::SchedulePreview;
        dialog.draft.schedule_date = "2026-11-02".into();
        dialog.draft.schedule_time = "09:15".into();
        dialog.draft.schedule_count = "3".into();
        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        settle(&mut dialog, &ctx, size);
        click(&mut dialog, &ctx, size, "schedule_row_1");
        assert_eq!(dialog.draft.selected_schedule_index, Some(1));
        frame(&mut dialog, &ctx, size, Vec::new());
        assert_eq!(dialog.draft.selected_schedule_index, Some(1));
        let copy = dialog
            .test_controls
            .iter()
            .find(|(id, _)| id == "schedule_copy")
            .map(|(_, rect)| *rect)
            .unwrap();
        frame(&mut dialog, &ctx, size, click_events(copy.center(), true));
        let effect = frame(&mut dialog, &ctx, size, click_events(copy.center(), false));
        assert!(matches!(
            effect,
            Some(DateTimeStudioEffect::CopyText(text)) if text.contains("2026-11-03")
        ));
    }

    #[test]
    fn date_time_studio_ambiguous_schedule_copy_uses_the_explicit_fold_choice() {
        let mut dialog = initialized_dialog();
        dialog.draft.mode = StudioMode::SchedulePreview;
        dialog.draft.schedule_date = "2026-11-01".into();
        dialog.draft.schedule_time = "01:30".into();
        dialog.draft.schedule_zone = "America/New_York".into();
        dialog.draft.schedule_count = "1".into();
        let occurrence =
            crate::date_arithmetic::preview_schedule(schedule_request(&dialog.draft).unwrap())
                .unwrap()
                .remove(0);
        let LocalResolution::Ambiguous { earlier, later, .. } = occurrence.resolution else {
            panic!("expected the autumn repeated local time");
        };
        let earlier_label = earlier.display_label();
        let later_label = later.display_label();

        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        settle(&mut dialog, &ctx, size);
        click(&mut dialog, &ctx, size, "schedule_row_0");
        assert!(
            !dialog
                .test_controls
                .iter()
                .any(|(id, _)| id == "schedule_copy")
        );

        click(&mut dialog, &ctx, size, "schedule_fold_later");
        let copy = dialog
            .test_controls
            .iter()
            .find(|(id, _)| id == "schedule_copy")
            .map(|(_, rect)| *rect)
            .expect("Copy should become available after choosing the later fold");
        frame(&mut dialog, &ctx, size, click_events(copy.center(), true));
        let effect = frame(&mut dialog, &ctx, size, click_events(copy.center(), false));
        let Some(DateTimeStudioEffect::CopyText(text)) = effect else {
            panic!("expected a Copy effect for the selected fold");
        };
        assert!(text.contains(&later_label));
        assert!(!text.contains(&earlier_label));
    }

    #[test]
    fn date_time_studio_calendar_prepare_effect_uses_the_selected_resolved_instant() {
        let mut dialog = initialized_dialog();
        dialog.draft.mode = StudioMode::SchedulePreview;
        dialog.draft.schedule_date = "2026-11-02".into();
        dialog.draft.schedule_time = "09:00".into();
        dialog.draft.schedule_zone = "UTC".into();
        dialog.draft.schedule_count = "1".into();
        let expected =
            crate::date_arithmetic::preview_schedule(schedule_request(&dialog.draft).unwrap())
                .unwrap()[0]
                .resolution
                .select(None)
                .unwrap()
                .instant_utc;
        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        settle(&mut dialog, &ctx, size);
        click(&mut dialog, &ctx, size, "schedule_row_0");
        let prepare = dialog
            .test_controls
            .iter()
            .find(|(id, _)| id == "schedule_prepare_calendar")
            .map(|(_, rect)| *rect)
            .unwrap();
        frame(
            &mut dialog,
            &ctx,
            size,
            click_events(prepare.center(), true),
        );
        let effect = frame(
            &mut dialog,
            &ctx,
            size,
            click_events(prepare.center(), false),
        );
        assert_eq!(
            effect,
            Some(DateTimeStudioEffect::PrepareCalendar {
                instant_utc: expected
            })
        );
    }

    #[test]
    fn date_time_studio_tiny_window_bounds_all_painted_controls() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        let size = egui::vec2(320.0, 360.0);
        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        let window = dialog.test_window_rect.unwrap();
        assert!(window.width() <= size.x + 1.0, "window: {window:?}");
        assert!(window.height() <= size.y + 1.0, "window: {window:?}");
        assert!(!dialog.test_controls.is_empty());
        for (id, rect) in &dialog.test_controls {
            assert!(rect.width() > 0.0, "{id} has no width");
            assert!(rect.height() > 0.0, "{id} has no height");
            assert!(
                rect.left() >= 0.0 && rect.right() <= size.x + 1.0,
                "{id}: {rect:?}"
            );
        }
    }

    #[test]
    fn date_time_studio_copy_effect_is_rejected_on_a_discarded_pass() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        settle(&mut dialog, &ctx, size);
        let copy = dialog
            .test_controls
            .iter()
            .find(|(id, _)| id == "between_copy")
            .map(|(_, rect)| *rect)
            .unwrap();
        frame(&mut dialog, &ctx, size, click_events(copy.center(), true));
        let output = ctx.run_ui(input(size, click_events(copy.center(), false)), |ui| {
            let first = ui.ctx().current_pass_index() == 0;
            let _ = dialog.show(ui.ctx(), first, false);
            if first {
                ui.ctx().request_discard("Date Studio effect regression");
            }
        });
        output.drop_without_applying_deltas();
        assert_eq!(dialog.on_ui_frame_complete(&ctx), None);

        // A normal release after republishing enabled hit targets still
        // produces the effect, proving the discarded release was rejected.
        settle(&mut dialog, &ctx, size);
        let copy = dialog
            .test_controls
            .iter()
            .find(|(id, _)| id == "between_copy")
            .map(|(_, rect)| *rect)
            .unwrap();
        frame(&mut dialog, &ctx, size, click_events(copy.center(), true));
        let effect = frame(&mut dialog, &ctx, size, click_events(copy.center(), false));
        assert!(matches!(effect, Some(DateTimeStudioEffect::CopyText(_))));
    }

    #[test]
    fn date_time_studio_collapsed_titlebar_close_publishes_window_lifecycle() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        ctx.global_style_mut(|style| style.animation_time = 0.0);
        settle(&mut dialog, &ctx, size);

        let mut collapsing = egui::collapsing_header::CollapsingState::load_with_default_open(
            &ctx,
            egui::Id::new("date-time-studio-window").with("collapsing"),
            true,
        );
        collapsing.set_open(false);
        collapsing.store(&ctx);
        settle(&mut dialog, &ctx, size);
        assert!(
            dialog.test_controls.is_empty(),
            "collapsed window skips contents"
        );
        let window = dialog.test_window_rect.expect("collapsed window response");
        let close = egui::pos2(window.right() - 18.0, window.top() + 19.0);
        frame(&mut dialog, &ctx, size, click_events(close, true));
        frame(&mut dialog, &ctx, size, click_events(close, false));

        assert!(!dialog.open, "titlebar X should close a collapsed window");
        assert!(frame(&mut dialog, &ctx, size, Vec::new()).is_none());
        assert!(!dialog.test_was_rendered());
    }

    #[test]
    fn date_time_studio_collapsed_host_close_is_applied_without_contents_ui() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        ctx.global_style_mut(|style| style.animation_time = 0.0);
        settle(&mut dialog, &ctx, size);

        let mut collapsing = egui::collapsing_header::CollapsingState::load_with_default_open(
            &ctx,
            egui::Id::new("date-time-studio-window").with("collapsing"),
            true,
        );
        collapsing.set_open(false);
        collapsing.store(&ctx);
        settle(&mut dialog, &ctx, size);
        assert!(dialog.test_controls.is_empty());

        dialog.request_close();
        frame(&mut dialog, &ctx, size, Vec::new());
        assert!(!dialog.open);
        assert!(dialog.test_was_rendered());
    }

    #[test]
    fn date_time_studio_keyboard_reveals_current_copy_action_in_a_tiny_window() {
        let mut dialog = DateTimeStudioDialog::default();
        dialog.open();
        dialog.draft.initialize_reference(reference());
        let ctx = egui::Context::default();
        ctx.global_style_mut(|style| {
            style.scroll_animation = egui::style::ScrollAnimation::none();
        });
        let size = egui::vec2(320.0, 360.0);
        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        let copy_id = dialog
            .test_focusable_controls
            .iter()
            .find(|(id, _, _, _)| id == "between_copy")
            .map(|(_, id, _, _)| *id)
            .unwrap();
        let mut revealed = false;
        for _ in 0..40 {
            frame(
                &mut dialog,
                &ctx,
                size,
                vec![egui::Event::Key {
                    key: egui::Key::Tab,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            frame(
                &mut dialog,
                &ctx,
                size,
                vec![egui::Event::Key {
                    key: egui::Key::Tab,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            if ctx.memory(|memory| memory.focused()) == Some(copy_id) {
                for _ in 0..4 {
                    frame(&mut dialog, &ctx, size, Vec::new());
                }
                let target = dialog
                    .test_focusable_controls
                    .iter()
                    .find(|(id, _, _, _)| id == "between_copy")
                    .unwrap();
                let fully_visible = is_fully_visible(target.2, target.3);
                if fully_visible {
                    revealed = true;
                    break;
                }
            }
        }
        assert!(
            revealed,
            "Tab should reveal the focused current-result action"
        );
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(copy_id));
    }

    #[test]
    fn date_time_studio_normal_window_controls_fit_and_copy_notice_preserves_focus() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        let size = egui::vec2(800.0, 650.0);
        settle(&mut dialog, &ctx, size);
        assert_focusable_controls_visible(&dialog);
        let copy_id = dialog
            .test_focusable_controls
            .iter()
            .find(|(id, _, _, _)| id == "between_copy")
            .map(|(_, id, _, _)| *id)
            .expect("Copy has a stable focusable response");

        let mut focused = false;
        for _ in 0..40 {
            frame(
                &mut dialog,
                &ctx,
                size,
                vec![egui::Event::Key {
                    key: egui::Key::Tab,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            frame(
                &mut dialog,
                &ctx,
                size,
                vec![egui::Event::Key {
                    key: egui::Key::Tab,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            if ctx.memory(|memory| memory.focused()) == Some(copy_id) {
                focused = true;
                break;
            }
        }
        assert!(focused, "Tab should focus the current-result Copy action");

        let enter_event = |pressed| {
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]
        };
        let pressed_effect = frame(&mut dialog, &ctx, size, enter_event(true));
        let released_effect = frame(&mut dialog, &ctx, size, enter_event(false));
        let effect = pressed_effect.or(released_effect);
        assert!(matches!(effect, Some(DateTimeStudioEffect::CopyText(_))));

        dialog.copy_completed(Ok(()));
        frame(&mut dialog, &ctx, size, Vec::new());
        assert_focusable_controls_visible(&dialog);
        let after_notice_id = dialog
            .test_focusable_controls
            .iter()
            .find(|(id, _, _, _)| id == "between_copy")
            .map(|(_, id, _, _)| *id)
            .expect("Copy remains present after the success notice appears");
        assert_eq!(after_notice_id, copy_id);
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(copy_id));
    }

    #[test]
    fn date_time_studio_calendar_preview_uses_checked_machine_local_conversion() {
        let instant = NaiveDate::from_ymd_opt(2026, 10, 25)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_utc();
        let local = machine_local_datetime(instant).unwrap();
        assert_eq!(local.time().second(), 0);
        assert_eq!(local.time().nanosecond(), 0);
    }
}
