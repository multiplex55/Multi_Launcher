use crate::gui::LauncherApp;
use crate::gui::theme::widgets::{ButtonEmphasis, secondary_help, with_button_emphasis};
use crate::plugins::timer::{
    AlarmDateSpec, AlarmSpec, parse_alarm_spec, parse_duration, resolve_alarm_spec_at,
    try_start_alarm_named, try_start_timer_named,
};
use chrono::{Local, TimeZone};
use eframe::egui;
use std::time::Duration;
#[derive(Default)]
pub struct TimerDialog {
    pub open: bool,
    host_state: Option<super::utility_host::UtilityHostState>,
    host_session_epoch: u64,
    host_pending: Option<TimerHostFrame>,
    retired_native_identity: Option<super::utility_window_geometry::NativeViewportIdentity>,
    host_jump: crate::gui::jump_mode::ui::JumpModeUi,
    host_jump_frame: Option<u64>,
    host_focus: bool,
    host_os_close_pending: bool,
    sound_popup: Option<TimerPopupAnchor>,
    #[cfg(test)]
    host_button: Option<(egui::Rect, egui::Rect, bool)>,
    mode: Mode,
    duration: String,
    duration_text_observed: String,
    time: String,
    time_text_observed: String,
    label: String,
    sound_idx: usize,
    focus_duration_next_frame: bool,
    duration_hours: u64,
    duration_minutes: u32,
    duration_seconds: u32,
    alarm_hour: u32,
    alarm_minute: u32,
    start_error: Option<String>,
    #[cfg(test)]
    alarm_preview: Option<String>,
}

#[derive(Debug)]
enum TimerSubmission {
    Timer {
        duration: Duration,
        name: Option<String>,
        sound: String,
    },
    Alarm {
        spec: AlarmSpec,
        name: Option<String>,
        sound: String,
    },
}

#[derive(Default)]
struct TimerDialogEffects {
    submission: Option<TimerSubmission>,
    close: bool,
    host_request: Option<super::utility_host::UtilityPresentation>,
    focus_input: bool,
    play_sound: Option<&'static str>,
    #[cfg(test)]
    start_rect: Option<egui::Rect>,
    #[cfg(test)]
    start_clicked: Option<bool>,
    #[cfg(test)]
    cancel_rect: Option<egui::Rect>,
    #[cfg(test)]
    alarm_hour_rect: Option<egui::Rect>,
    #[cfg(test)]
    alarm_hour_enabled: Option<bool>,
    #[cfg(test)]
    duration_hours_rect: Option<egui::Rect>,
    #[cfg(test)]
    duration_minutes_rect: Option<egui::Rect>,
    #[cfg(test)]
    duration_seconds_rect: Option<egui::Rect>,
    #[cfg(test)]
    control_clips: Vec<egui::Rect>,
    #[cfg(test)]
    control_enabled: Vec<bool>,
    #[cfg(test)]
    control_changed: Vec<bool>,
    #[cfg(test)]
    control_clicked: Vec<bool>,
    #[cfg(test)]
    body_rect: Option<egui::Rect>,
    #[cfg(test)]
    body_clip: Option<egui::Rect>,
    #[cfg(test)]
    window_rect: Option<egui::Rect>,
}

#[derive(Clone, Copy)]
struct TimerPopupAnchor {
    response: egui::Id,
    popup: egui::Id,
    layer: egui::LayerId,
    viewport: egui::ViewportId,
    epoch: u64,
}
#[derive(Clone)]
struct TimerHostDraft {
    duration: String,
    duration_text_observed: String,
    time: String,
    time_text_observed: String,
    label: String,
    sound_idx: usize,
    focus_duration_next_frame: bool,
    duration_hours: u64,
    duration_minutes: u32,
    duration_seconds: u32,
    alarm_hour: u32,
    alarm_minute: u32,
    start_error: Option<String>,
    #[cfg(test)]
    alarm_preview: Option<String>,
}
struct TimerHostFrame {
    root: super::utility_host::ViewportPassProvenance,
    owner: super::utility_host::ViewportPassProvenance,
    host: super::utility_host::HostIdentity,
    epoch: u64,
    pinned: bool,
    draft: TimerHostDraft,
    effects: TimerDialogEffects,
    native_info: Option<egui::ViewportInfo>,
    copies: Vec<String>,
}
#[derive(Default)]
pub(crate) struct TimerHostOutcome {
    effects: TimerDialogEffects,
    epoch: u64,
    restore_root_focus: bool,
    pinned: bool,
    pub(crate) show_root: bool,
}

#[cfg(feature = "qa-framebuffer-capture")]
pub(crate) struct TimerDialogQaObservation {
    pub(crate) open: bool,
    pub(crate) mode: &'static str,
    pub(crate) duration_text_chars: usize,
    pub(crate) label_text_chars: usize,
    pub(crate) sound_index: usize,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
enum Mode {
    #[default]
    Timer,
    Alarm,
}

impl TimerDialog {
    #[cfg(feature = "qa-framebuffer-capture")]
    pub(crate) fn qa_observation(&self) -> TimerDialogQaObservation {
        let (mode, value) = match self.mode {
            Mode::Timer => ("timer", self.duration.as_str()),
            Mode::Alarm => ("alarm", self.time.as_str()),
        };
        TimerDialogQaObservation {
            open: self.open,
            mode,
            duration_text_chars: value.chars().count(),
            label_text_chars: self.label.chars().count(),
            sound_index: self.sound_idx,
        }
    }

    fn begin_host_session(&mut self) {
        if self.open && self.retired_native_identity.is_none() {
            self.retired_native_identity = self.native_identity(self.host_state());
        }
        if let Some(state) = &mut self.host_state {
            state.complete_bootstrap(state.identity(), None, None);
        }
        self.host_session_epoch = self.host_session_epoch.wrapping_add(1).max(1);
        self.host_pending = None;
        self.host_os_close_pending = false;
        self.host_jump.cancel_all();
        self.host_jump_frame = None;
        self.sound_popup = None;
        if !self.open {
            self.host_state = Some(super::utility_host::UtilityHostState::docked());
        }
        self.host_focus = true;
    }
    pub(crate) fn host_session_epoch(&self) -> u64 {
        self.host_session_epoch
    }
    fn host_state(&self) -> super::utility_host::HostIdentity {
        self.host_state
            .as_ref()
            .map(|state| state.identity())
            .unwrap_or_else(|| super::utility_host::UtilityHostState::docked().identity())
    }
    pub(crate) fn root_layer(&self) -> Option<egui::LayerId> {
        if !self.open {
            return None;
        }
        if self.host_state().host == super::utility_host::UtilityPresentation::Docked {
            Some(match self.mode {
                Mode::Timer => super::jump_mode::ui::timer_dialog_layer_id(),
                Mode::Alarm => super::jump_mode::ui::alarm_dialog_layer_id(),
            })
        } else {
            self.host_state
                .as_ref()
                .and_then(|state| state.embedded_root_layer())
        }
    }
    pub(crate) fn needs_root_ui_when_hidden(&self) -> bool {
        (self.open && self.host_state().host == super::utility_host::UtilityPresentation::Popout)
            || self.host_pending.is_some()
    }
    #[cfg(test)]
    pub(crate) fn test_host_button(&self) -> Option<(egui::Rect, egui::Rect, bool)> {
        self.host_button
    }

    pub(crate) fn ensure_open(&mut self) {
        if !self.open {
            self.open_timer();
        }
    }
    pub(crate) fn request_host_focus(&mut self) {
        self.host_focus = true;
    }

    pub fn open_timer(&mut self) {
        self.begin_host_session();
        self.mode = Mode::Timer;
        self.open = true;
        self.duration.clear();
        self.duration_text_observed.clear();
        self.duration_hours = 0;
        self.duration_minutes = 0;
        self.duration_seconds = 0;
        self.label.clear();
        self.sound_idx = 0;
        self.start_error = None;
        #[cfg(test)]
        {
            self.alarm_preview = None;
        }
        self.focus_duration_next_frame = true;
    }

    pub fn open_alarm(&mut self) {
        self.begin_host_session();
        self.focus_duration_next_frame = false;
        self.mode = Mode::Alarm;
        self.open = true;
        self.time.clear();
        self.time_text_observed.clear();
        self.alarm_hour = 0;
        self.alarm_minute = 0;
        self.label.clear();
        self.sound_idx = 0;
        self.start_error = None;
        #[cfg(test)]
        {
            self.alarm_preview = None;
        }
    }

    fn title(&self) -> &'static str {
        match self.mode {
            Mode::Timer => "Create Timer",
            Mode::Alarm => "Set Alarm",
        }
    }

    pub(crate) fn jump_mode_window_title(&self) -> Option<&'static str> {
        self.open.then(|| self.title())
    }

    #[cfg(test)]
    pub(crate) fn test_duration(&self) -> &str {
        &self.duration
    }

    #[cfg(test)]
    pub(crate) fn test_sound_index(&self) -> usize {
        self.sound_idx
    }

    fn capture_host_draft(&self) -> TimerHostDraft {
        TimerHostDraft {
            duration: self.duration.clone(),
            duration_text_observed: self.duration_text_observed.clone(),
            time: self.time.clone(),
            time_text_observed: self.time_text_observed.clone(),
            label: self.label.clone(),
            sound_idx: self.sound_idx,
            focus_duration_next_frame: self.focus_duration_next_frame,
            duration_hours: self.duration_hours,
            duration_minutes: self.duration_minutes,
            duration_seconds: self.duration_seconds,
            alarm_hour: self.alarm_hour,
            alarm_minute: self.alarm_minute,
            start_error: self.start_error.clone(),
            #[cfg(test)]
            alarm_preview: self.alarm_preview.clone(),
        }
    }
    fn restore_host_draft(&mut self, draft: TimerHostDraft) {
        self.duration = draft.duration;
        self.duration_text_observed = draft.duration_text_observed;
        self.time = draft.time;
        self.time_text_observed = draft.time_text_observed;
        self.label = draft.label;
        self.sound_idx = draft.sound_idx;
        self.focus_duration_next_frame = draft.focus_duration_next_frame;
        self.duration_hours = draft.duration_hours;
        self.duration_minutes = draft.duration_minutes;
        self.duration_seconds = draft.duration_seconds;
        self.alarm_hour = draft.alarm_hour;
        self.alarm_minute = draft.alarm_minute;
        self.start_error = draft.start_error;
        #[cfg(test)]
        {
            self.alarm_preview = draft.alarm_preview;
        }
    }
    pub(crate) fn owned_sound_popup_layer(
        &self,
        ctx: &egui::Context,
        parent: egui::LayerId,
    ) -> Option<egui::LayerId> {
        let anchor = self.sound_popup?;
        if !self.open
            || anchor.epoch != self.host_session_epoch
            || anchor.viewport != ctx.viewport_id()
            || anchor.layer != parent
        {
            return None;
        }
        let response = ctx.read_response(anchor.response)?;
        if !response.enabled()
            || response.layer_id != parent
            || !egui::Popup::is_id_open(ctx, anchor.popup)
        {
            return None;
        }
        use super::jump_mode::ui::{focused_response_layer, visible_top_middle_layer};

        let rect = ctx.memory(|memory| memory.area_rect(anchor.popup))?;
        let layer = egui::LayerId::new(egui::Order::Foreground, anchor.popup);
        let parent_is_current = if parent == egui::LayerId::background() {
            visible_top_middle_layer(ctx).is_none()
        } else {
            visible_top_middle_layer(ctx) == Some(parent)
        };
        let top_popup = ctx.memory(|memory| {
            memory
                .layer_ids()
                .filter(|layer| {
                    layer.order == egui::Order::Foreground
                        && memory.areas().visible_last_frame(layer)
                })
                .last()
        });
        let sizing = egui::containers::AreaState::load(ctx, anchor.popup)
            .is_some_and(|state| !state.interactable);
        (parent_is_current
            && rect.is_positive()
            && top_popup == Some(layer)
            && (ctx.layer_id_at(rect.center()) == Some(layer) || sizing)
            && focused_response_layer(ctx).is_none_or(|focus| {
                focus == parent || focus == layer || focus.order == egui::Order::Background
            }))
        .then_some(layer)
    }
    fn native_viewport() -> egui::ViewportId {
        egui::ViewportId::from_hash_of("timer_authoring_viewport")
    }
    fn native_identity(
        &self,
        host: super::utility_host::HostIdentity,
    ) -> Option<super::utility_window_geometry::NativeViewportIdentity> {
        (host.host == super::utility_host::UtilityPresentation::Popout).then_some(
            super::utility_window_geometry::NativeViewportIdentity {
                key: crate::settings::UtilityToolKey::parse("timer").expect("stable timer key"),
                viewport_id: Self::native_viewport(),
                owner_generation: self.host_session_epoch,
                host_generation: host.generation,
            },
        )
    }
    fn retire_previous_native_identity(&mut self, ctx: &egui::Context) {
        if let Some(identity) = self.retired_native_identity.take() {
            super::utility_dialog_size::close_native_utility_geometry_viewport(ctx, identity);
        }
    }

    pub(crate) fn end_session_with_host(&mut self, ctx: &egui::Context) {
        self.retire_previous_native_identity(ctx);
        if let Some(identity) = self.native_identity(self.host_state()) {
            super::utility_dialog_size::close_native_utility_geometry_viewport(ctx, identity);
            ctx.send_viewport_cmd_to(identity.viewport_id, egui::ViewportCommand::Close);
        }
        self.host_pending = None;
        self.host_os_close_pending = false;
        self.host_jump.cancel_all();
        self.host_jump_frame = None;
        if let Some(state) = &mut self.host_state {
            state.retire_to_docked();
        }
        self.open = false;
    }
    pub(crate) fn show_hosted(
        &mut self,
        ctx: &egui::Context,
        root_scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        instance: Option<u64>,
        root_owner: crate::gui::jump_mode::ui::NavigationOwner,
        application_input_allowed: bool,
        root_native_owner_current: bool,
        launcher_jump: &crate::gui::jump_mode::ui::JumpModeUi,
        jump_enabled: bool,
        root_visible: bool,
        first: bool,
        pinned: bool,
    ) {
        use super::utility_host::{UtilityPresentation, ViewportPassProvenance};
        use crate::gui::jump_mode::ui::{NavigationOwner, NavigationViewportBinding};
        if let Some(previous) = self.host_pending.take() {
            if previous.epoch == self.host_session_epoch {
                self.restore_host_draft(previous.draft);
            }
        }
        if !self.open {
            return;
        }
        self.retire_previous_native_identity(ctx);
        if pinned {
            self.host_os_close_pending = false;
        }
        self.host_state
            .get_or_insert_with(super::utility_host::UtilityHostState::docked);
        let replay_scope = (!first)
            .then(|| root_scope.map(crate::gui::jump_mode::ui::JumpModeScope::registration_only))
            .flatten();
        let root_scope = replay_scope.as_ref().or(root_scope);
        let root = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
        let host = self.host_state();
        let draft = self.capture_host_draft();
        let mut owner_pass = None;
        let mut native_info = None;
        let mut effects = None;
        let mut copies = Vec::new();
        #[cfg(test)]
        {
            self.host_button = None;
        }
        if host.host == UtilityPresentation::Docked {
            if root_visible {
                let allowed = root_native_owner_current && application_input_allowed;
                if first
                    && allowed
                    && !egui::Popup::is_any_open(ctx)
                    && launcher_jump.active_session().is_none()
                    && ctx.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                            || input.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                    })
                    && !pinned
                {
                    let mut result = TimerDialogEffects::default();
                    result.close = true;
                    effects = Some(result);
                }
                let start = ctx.output(|output| output.commands.len());
                let rendered = self.render_inner(
                    ctx,
                    root_scope.cloned(),
                    allowed,
                    first,
                    None,
                    Some(UtilityPresentation::Docked),
                    Local::now,
                    |_| Ok(()),
                );
                let captured = take_timer_copies(ctx, start);
                if allowed && first {
                    copies = captured;
                }
                if effects.is_none() {
                    effects = Some(rendered);
                }
                owner_pass = Some(root);
            }
        } else {
            let viewport = Self::native_viewport();
            let builder = egui::ViewportBuilder::default()
                .with_title(self.title())
                .with_inner_size([480.0, 320.0])
                .with_min_inner_size([300.0, 220.0])
                .with_resizable(true);
            ctx.show_viewport_immediate(viewport, builder, |child, class| {
                let child_ctx = child.ctx().clone();
                let native =
                    class == egui::ViewportClass::Immediate && child_ctx.viewport_id() == viewport;
                let child_first = first && child.current_pass_index() == 0;
                if child_first {
                    owner_pass = Some(ViewportPassProvenance::current(
                        &child_ctx,
                        child_ctx.viewport_id(),
                    ));
                    if native {
                        native_info = Some(child.input(|input| input.viewport().clone()));
                    }
                } else {
                    owner_pass = None;
                    native_info = None;
                    effects = None;
                    self.restore_host_draft(draft.clone());
                }
                let parent_layer = if native {
                    egui::LayerId::background()
                } else {
                    self.root_layer().unwrap_or_else(egui::LayerId::background)
                };
                let popup = self
                    .owned_sound_popup_layer(&child_ctx, parent_layer)
                    .is_some();
                let allowed = if native {
                    application_input_allowed
                        && child_ctx.input(|input| input.focused)
                        && child_ctx.memory(|memory| memory.top_modal_layer().is_none())
                        && super::jump_mode::ui::visible_top_middle_layer(&child_ctx).is_none()
                        && (!egui::Popup::is_any_open(&child_ctx) || popup)
                        && super::jump_mode::ui::focused_response_layer(&child_ctx).is_none_or(
                            |layer| {
                                layer == egui::LayerId::background()
                                    || self.owned_sound_popup_layer(&child_ctx, parent_layer)
                                        == Some(layer)
                            },
                        )
                } else {
                    root_native_owner_current && application_input_allowed
                };
                if child_first && native && child.input(|input| input.viewport().close_requested())
                {
                    child.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    if !pinned {
                        self.host_os_close_pending = true;
                    }
                }
                let start = child_ctx.output(|output| output.commands.len());
                if native {
                    let (chord, alphabet) = launcher_jump.configuration();
                    self.host_jump.set_configuration(chord, alphabet);
                    self.host_jump.set_enabled(jump_enabled);
                    let frame = child_ctx.cumulative_frame_nr();
                    if self.host_jump_frame != Some(frame) {
                        self.host_jump.begin_frame();
                        self.host_jump_frame = Some(frame);
                    }
                    let binding = instance.map(|instance| NavigationViewportBinding {
                        viewport,
                        owner: NavigationOwner::Timer { instance },
                        base_layer: egui::LayerId::background(),
                    });
                    let nav = binding.map_or(NavigationOwner::Blocked, |binding| {
                        self.host_jump.update_navigation_owner_for_viewport(
                            &child_ctx,
                            binding,
                            application_input_allowed && jump_enabled,
                        )
                    });
                    self.host_jump.handle_viewport_input(
                        &child_ctx,
                        binding.is_some_and(|binding| binding.owner == nav),
                        child_first,
                        viewport,
                    );
                    let shortcut_close = child_first
                        && allowed
                        && !popup
                        && self.host_jump.active_session().is_none()
                        && child_ctx.input_mut(|input| {
                            input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                                || input.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                        });
                    self.host_jump.begin_ui_pass();
                    let scope =
                        binding
                            .filter(|binding| nav == binding.owner)
                            .and_then(|binding| {
                                self.host_jump.scope_for_viewport_binding(
                                    crate::gui::jump_mode::SurfaceId::dialog(
                                        "timer",
                                        instance.unwrap_or_default(),
                                    ),
                                    crate::gui::jump_mode::TargetGeneration(
                                        self.host_session_epoch,
                                    ),
                                    "timer/form",
                                    binding,
                                )
                            });
                    let scope = scope.map(|scope| {
                        if child_first {
                            scope
                        } else {
                            scope.registration_only()
                        }
                    });
                    let mut rendered = None;
                    egui::CentralPanel::default().show(child, |ui| {
                        rendered = Some(self.render_inner(
                            &child_ctx,
                            scope,
                            allowed,
                            child_first,
                            Some(ui),
                            Some(UtilityPresentation::Popout),
                            Local::now,
                            |_| Ok(()),
                        ));
                    });
                    if let Some(result) = &mut rendered
                        && !pinned
                        && (shortcut_close || self.host_os_close_pending)
                    {
                        result.close = true;
                    }
                    effects = rendered;
                    self.host_jump.finish_ui_pass();
                    if let Some(binding) = binding {
                        let current = self.host_jump.update_navigation_owner_for_viewport(
                            &child_ctx,
                            binding,
                            application_input_allowed && jump_enabled,
                        );
                        if self.host_jump.prepare_pass_for_viewport(
                            &child_ctx,
                            binding,
                            current,
                            application_input_allowed && jump_enabled,
                        ) {
                            super::jump_mode::paint::paint(&child_ctx, &self.host_jump);
                        }
                        if !child_ctx.will_discard() {
                            self.host_jump.complete_frame_for_viewport(
                                &child_ctx,
                                binding,
                                current,
                                application_input_allowed && jump_enabled,
                            );
                        }
                    }
                } else {
                    let shortcut_close = child_first
                        && allowed
                        && !pinned
                        && !egui::Popup::is_any_open(&child_ctx)
                        && launcher_jump.active_session().is_none()
                        && child_ctx.input_mut(|input| {
                            input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                                || input.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                        });
                    let mut rendered = self.render_inner(
                        &child_ctx,
                        root_scope.cloned(),
                        allowed,
                        child_first,
                        None,
                        Some(UtilityPresentation::Popout),
                        Local::now,
                        |_| Ok(()),
                    );
                    rendered.close |= shortcut_close;
                    effects = Some(rendered);
                }
                let captured = take_timer_copies(&child_ctx, start);
                if allowed && child_first {
                    copies = captured;
                }
            });
        }
        let _ = root_owner;
        if let (Some(owner), Some(mut effects)) = (owner_pass, effects) {
            if pinned {
                effects.close = false;
            }
            if first && !ctx.will_discard() {
                self.host_pending = Some(TimerHostFrame {
                    root,
                    owner,
                    host,
                    epoch: self.host_session_epoch,
                    pinned,
                    draft,
                    effects,
                    native_info,
                    copies,
                });
                return;
            }
        }
        self.restore_host_draft(draft);
    }
    pub(crate) fn on_ui_frame_complete(&mut self, ctx: &egui::Context) -> TimerHostOutcome {
        use super::utility_host::UtilityPresentation;
        let Some(frame) = self.host_pending.take() else {
            return TimerHostOutcome::default();
        };
        if !self.open
            || frame.epoch != self.host_session_epoch
            || frame.host != self.host_state()
            || !frame.root.was_retained(ctx)
            || !frame.owner.was_retained(ctx)
        {
            if self.open && frame.epoch == self.host_session_epoch {
                self.restore_host_draft(frame.draft);
            }
            return TimerHostOutcome::default();
        }
        let identity = self.native_identity(frame.host);
        if let (Some(identity), Some(info)) = (identity, frame.native_info.as_ref()) {
            super::utility_dialog_size::apply_native_utility_geometry_restore(
                ctx,
                egui::ViewportClass::Immediate,
                true,
                self.title(),
                identity,
                info,
                egui::vec2(480.0, 320.0),
                Some(egui::vec2(300.0, 220.0)),
                None,
            );
            if self.host_focus {
                ctx.send_viewport_cmd_to(identity.viewport_id, egui::ViewportCommand::Focus);
            }
        } else if self.host_focus && frame.owner.viewport_id == egui::ViewportId::ROOT {
            let layer = match self.mode {
                Mode::Timer => super::jump_mode::ui::timer_dialog_layer_id(),
                Mode::Alarm => super::jump_mode::ui::alarm_dialog_layer_id(),
            };
            ctx.move_to_top(layer);
        }
        self.host_focus = false;
        for copy in frame.copies {
            let _ = crate::actions::clipboard::set_text(&copy);
        }
        let mut effects = frame.effects;
        if effects.close || effects.host_request.is_some() {
            effects.submission = None;
            effects.play_sound = None;
        }
        let request = if effects.close {
            None
        } else {
            effects.host_request
        };
        let show_root = request == Some(UtilityPresentation::Docked);
        if let Some(requested) = request
            && let Some(state) = &mut self.host_state
            && state.transition(requested)
        {
            self.host_jump.cancel_all();
            self.host_jump_frame = None;
            if requested == UtilityPresentation::Docked {
                self.host_focus = true;
                if let Some(identity) = identity {
                    super::utility_dialog_size::close_native_utility_geometry_viewport(
                        ctx, identity,
                    );
                    ctx.send_viewport_cmd_to(identity.viewport_id, egui::ViewportCommand::Close);
                }
            } else {
                self.host_focus = true;
            }
            ctx.request_repaint();
        }
        if frame.host.host == UtilityPresentation::Popout
            && let Some(state) = &mut self.host_state
        {
            let embedded = frame.native_info.is_none().then(|| match self.mode {
                Mode::Timer => super::jump_mode::ui::timer_dialog_layer_id(),
                Mode::Alarm => super::jump_mode::ui::alarm_dialog_layer_id(),
            });
            state.complete_bootstrap(frame.host, embedded, None);
        }
        TimerHostOutcome {
            effects,
            epoch: frame.epoch,
            restore_root_focus: frame.owner.viewport_id == egui::ViewportId::ROOT,
            pinned: frame.pinned,
            show_root,
        }
    }
    pub(crate) fn apply_host_effects(
        &mut self,
        ctx: &egui::Context,
        outcome: TimerHostOutcome,
        app: &mut LauncherApp,
    ) -> bool {
        let restore_root_focus = outcome.restore_root_focus;
        let (closed, focus) = self.apply_host_effects_with(
            ctx,
            outcome,
            |submission| match submission {
                TimerSubmission::Timer {
                    duration,
                    name,
                    sound,
                } => try_start_timer_named(duration, name, sound)
                    .map(|_| ())
                    .map_err(|error| error.to_string()),
                TimerSubmission::Alarm { spec, name, sound } => {
                    try_start_alarm_named(spec, name, sound)
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                }
            },
            crate::sound::play_sound,
        );
        if focus && restore_root_focus {
            app.focus_input();
        }
        closed
    }
    fn apply_host_effects_with(
        &mut self,
        ctx: &egui::Context,
        outcome: TimerHostOutcome,
        mut submit: impl FnMut(TimerSubmission) -> Result<(), String>,
        mut play_sound: impl FnMut(&'static str),
    ) -> (bool, bool) {
        if !self.open || outcome.epoch != self.host_session_epoch {
            return (false, false);
        }
        if let Some(sound) = outcome.effects.play_sound {
            play_sound(sound);
        }
        let mut close = outcome.effects.close;
        let mut focus = false;
        if let Some(submission) = outcome.effects.submission {
            match submit(submission) {
                Ok(()) => {
                    close = !outcome.pinned;
                    focus = close && self.mode == Mode::Timer;
                }
                Err(error) => self.start_error = Some(error),
            }
        }
        if close {
            self.end_session_with_host(ctx);
        }
        (close, focus)
    }

    #[cfg(test)]
    pub(crate) fn test_native_jump_targets(
        &self,
    ) -> Vec<crate::gui::jump_mode::ui::RegisteredTarget> {
        self.host_jump.retained_registry().targets().to_vec()
    }
    #[cfg(test)]
    pub(crate) fn test_label(&self) -> &str {
        &self.label
    }

    fn duration_ui(&mut self, ui: &mut egui::Ui, allow_focus: bool) -> egui::Response {
        let response = ui.add(
            egui::TextEdit::singleline(&mut self.duration)
                .desired_width(ui.available_width().min(220.0).max(1.0)),
        );
        if self.focus_duration_next_frame
            && allow_focus
            && response.enabled()
            && !ui.ctx().will_discard()
        {
            response.request_focus();
            self.focus_duration_next_frame = false;
        }
        response
    }

    fn observe_duration_text(&mut self) {
        if self.duration == self.duration_text_observed {
            return;
        }
        self.duration_text_observed.clone_from(&self.duration);
        if let Some(duration) = parse_duration(&self.duration) {
            let total = duration.as_secs();
            self.duration_hours = total / 3_600;
            self.duration_minutes = ((total % 3_600) / 60) as u32;
            self.duration_seconds = (total % 60) as u32;
        }
    }

    fn observe_alarm_text(&mut self) {
        if self.time == self.time_text_observed {
            return;
        }
        self.time_text_observed.clone_from(&self.time);
        if let Ok(spec) = parse_alarm_spec(&self.time) {
            self.alarm_hour = spec.hour;
            self.alarm_minute = spec.minute;
        }
    }

    fn alarm_picker_date(&self) -> Option<AlarmDateSpec> {
        if self.time.trim().is_empty() {
            Some(AlarmDateSpec::NextOccurrence)
        } else {
            parse_alarm_spec(&self.time).ok().map(|spec| spec.date)
        }
    }

    fn canonical_alarm_time(&self, date: AlarmDateSpec) -> String {
        let time = format!("{:02}:{:02}", self.alarm_hour, self.alarm_minute);
        match date {
            AlarmDateSpec::NextOccurrence => time,
            AlarmDateSpec::DayOffset(days) => format!("{days}d {time}"),
            AlarmDateSpec::ExplicitDate(date) => {
                format!("{} {time}", date.format("%Y-%m-%d"))
            }
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        self.ui_with_jump_mode(ctx, app, None, true);
    }

    pub(crate) fn ui_with_jump_mode(
        &mut self,
        ctx: &egui::Context,
        app: &mut LauncherApp,
        jump_scope: Option<crate::gui::jump_mode::ui::JumpModeScope>,
        first_ui_pass: bool,
    ) {
        let effects =
            self.ui_with_runtime(ctx, jump_scope, first_ui_pass, Local::now, |submission| {
                match submission {
                    TimerSubmission::Timer {
                        duration,
                        name,
                        sound,
                    } => try_start_timer_named(duration, name, sound)
                        .map(|_| ())
                        .map_err(|error| error.to_string()),
                    TimerSubmission::Alarm { spec, name, sound } => {
                        try_start_alarm_named(spec, name, sound)
                            .map(|_| ())
                            .map_err(|error| error.to_string())
                    }
                }
            });
        if effects.focus_input {
            app.focus_input();
        }
        if let Some(sound) = effects.play_sound {
            crate::sound::play_sound(sound);
        }
    }

    fn ui_with_runtime(
        &mut self,
        ctx: &egui::Context,
        jump_scope: Option<crate::gui::jump_mode::ui::JumpModeScope>,
        first_ui_pass: bool,
        mut now: impl FnMut() -> chrono::DateTime<Local>,
        mut submit: impl FnMut(TimerSubmission) -> Result<(), String>,
    ) -> TimerDialogEffects {
        self.render_inner(
            ctx,
            jump_scope,
            true,
            first_ui_pass,
            None,
            None,
            &mut now,
            &mut submit,
        )
    }

    fn render_inner(
        &mut self,
        ctx: &egui::Context,
        jump_scope: Option<crate::gui::jump_mode::ui::JumpModeScope>,
        owner_current: bool,
        first_ui_pass: bool,
        child_ui: Option<&mut egui::Ui>,
        presentation: Option<super::utility_host::UtilityPresentation>,
        mut now: impl FnMut() -> chrono::DateTime<Local>,
        mut submit: impl FnMut(TimerSubmission) -> Result<(), String>,
    ) -> TimerDialogEffects {
        let mut effects = TimerDialogEffects::default();
        if !self.open {
            return effects;
        }
        match self.mode {
            Mode::Timer => self.observe_duration_text(),
            Mode::Alarm => self.observe_alarm_text(),
        }

        let mut open_val = self.open;
        let mut close = false;
        let mut jump_visual_order = 0;
        self.sound_popup = None;
        let title = self.title();
        let mut draw_body = |ui: &mut egui::Ui| {
            let contents_visible = ui.is_visible() && !ui.is_sizing_pass();
            let owner_pass =
                owner_current && first_ui_pass && contents_visible && !ui.ctx().will_discard();
            if let Some(current) = presentation {
                let (response, requested) = super::utility_host::presentation_button(
                    ui,
                    current,
                    owner_current && contents_visible,
                );
                #[cfg(test)]
                {
                    self.host_button = Some((response.rect, ui.clip_rect(), response.enabled()));
                }
                register_timer_jump_target(
                    jump_scope.as_ref(),
                    ui,
                    &response,
                    "host",
                    if current == super::utility_host::UtilityPresentation::Docked {
                        "Pop Out"
                    } else {
                        "Dock"
                    },
                    crate::gui::jump_mode::TargetKind::Button,
                    owner_pass,
                    &mut jump_visual_order,
                );
                if response.clicked() && owner_pass {
                    effects.host_request = Some(requested);
                }
            }
            match self.mode {
                Mode::Timer => {
                    ui.horizontal_wrapped(|ui| {
                        let label = ui.label("Duration");
                        let old = self.duration.clone();
                        let response = self.duration_ui(ui, owner_pass).labelled_by(label.id);
                        if response.changed() {
                            if response_is_authorized(ui, &response, owner_pass) {
                                self.observe_duration_text();
                                self.start_error = None;
                            } else {
                                self.duration = old;
                            }
                        }
                        register_timer_jump_target(
                            jump_scope.as_ref(),
                            ui,
                            &response,
                            "duration",
                            "Duration",
                            crate::gui::jump_mode::TargetKind::TextEntry,
                            owner_pass,
                            &mut jump_visual_order,
                        );
                    });
                    ui.horizontal_wrapped(|ui| {
                        let hours_label = ui.label("Hours");
                        let old_hours = self.duration_hours;
                        let hours = ui
                            .add(egui::DragValue::new(&mut self.duration_hours).range(0..=u64::MAX))
                            .labelled_by(hours_label.id);
                        #[cfg(test)]
                        {
                            effects.duration_hours_rect = Some(hours.rect);
                            effects.control_clips.push(ui.clip_rect());
                            effects.control_enabled.push(hours.enabled());
                            effects.control_changed.push(hours.changed());
                            effects.control_clicked.push(hours.clicked());
                        }
                        register_timer_jump_target(
                            jump_scope.as_ref(),
                            ui,
                            &hours,
                            "duration-hours",
                            "Duration hours",
                            crate::gui::jump_mode::TargetKind::Slider,
                            owner_pass,
                            &mut jump_visual_order,
                        );
                        let minutes_label = ui.label("Minutes");
                        let old_minutes = self.duration_minutes;
                        let minutes = ui
                            .add(egui::DragValue::new(&mut self.duration_minutes).range(0..=59))
                            .labelled_by(minutes_label.id);
                        #[cfg(test)]
                        {
                            effects.duration_minutes_rect = Some(minutes.rect);
                            effects.control_clips.push(ui.clip_rect());
                            effects.control_enabled.push(minutes.enabled());
                            effects.control_changed.push(minutes.changed());
                            effects.control_clicked.push(minutes.clicked());
                        }
                        register_timer_jump_target(
                            jump_scope.as_ref(),
                            ui,
                            &minutes,
                            "duration-minutes",
                            "Duration minutes",
                            crate::gui::jump_mode::TargetKind::Slider,
                            owner_pass,
                            &mut jump_visual_order,
                        );
                        let seconds_label = ui.label("Seconds");
                        let old_seconds = self.duration_seconds;
                        let seconds = ui
                            .add(egui::DragValue::new(&mut self.duration_seconds).range(0..=59))
                            .labelled_by(seconds_label.id);
                        #[cfg(test)]
                        {
                            effects.duration_seconds_rect = Some(seconds.rect);
                            effects.control_clips.push(ui.clip_rect());
                            effects.control_enabled.push(seconds.enabled());
                            effects.control_changed.push(seconds.changed());
                            effects.control_clicked.push(seconds.clicked());
                        }
                        register_timer_jump_target(
                            jump_scope.as_ref(),
                            ui,
                            &seconds,
                            "duration-seconds",
                            "Duration seconds",
                            crate::gui::jump_mode::TargetKind::Slider,
                            owner_pass,
                            &mut jump_visual_order,
                        );
                        let changed = hours.changed() || minutes.changed() || seconds.changed();
                        let authorized = (!hours.changed()
                            || response_is_authorized(ui, &hours, owner_pass))
                            && (!minutes.changed()
                                || response_is_authorized(ui, &minutes, owner_pass))
                            && (!seconds.changed()
                                || response_is_authorized(ui, &seconds, owner_pass));
                        if changed {
                            if authorized {
                                self.duration = format!(
                                    "{}:{:02}:{:02}",
                                    self.duration_hours,
                                    self.duration_minutes,
                                    self.duration_seconds
                                );
                                self.duration_text_observed.clone_from(&self.duration);
                                self.start_error = None;
                            } else {
                                self.duration_hours = old_hours;
                                self.duration_minutes = old_minutes;
                                self.duration_seconds = old_seconds;
                            }
                        }
                    });
                    secondary_help(ui, "Use Ns, Nm, Nh, mm:ss, or hh:mm:ss.");
                    if !self.duration.trim().is_empty()
                        && parse_duration(&self.duration).is_none()
                        && self.start_error.is_none()
                    {
                        ui.label("Enter a positive, valid duration.");
                    }
                }
                Mode::Alarm => {
                    ui.horizontal_wrapped(|ui| {
                        let label = ui.label("Time");
                        let old = self.time.clone();
                        let response = ui
                            .add(
                                egui::TextEdit::singleline(&mut self.time)
                                    .desired_width(ui.available_width().min(220.0).max(1.0)),
                            )
                            .labelled_by(label.id);
                        if response.changed() {
                            if response_is_authorized(ui, &response, owner_pass) {
                                self.observe_alarm_text();
                                self.start_error = None;
                            } else {
                                self.time = old;
                            }
                        }
                        register_timer_jump_target(
                            jump_scope.as_ref(),
                            ui,
                            &response,
                            "time",
                            "Time",
                            crate::gui::jump_mode::TargetKind::TextEntry,
                            owner_pass,
                            &mut jump_visual_order,
                        );
                    });
                    let picker_date = self.alarm_picker_date();
                    ui.add_enabled_ui(picker_date.is_some(), |ui| {
                        ui.horizontal_wrapped(|ui| {
                            let hour_label = ui.label("Hour");
                            let old_hour = self.alarm_hour;
                            let hour = ui
                                .add(egui::DragValue::new(&mut self.alarm_hour).range(0..=23))
                                .labelled_by(hour_label.id);
                            #[cfg(test)]
                            {
                                effects.alarm_hour_rect = Some(hour.rect);
                                effects.alarm_hour_enabled = Some(hour.enabled());
                            }
                            register_timer_jump_target(
                                jump_scope.as_ref(),
                                ui,
                                &hour,
                                "alarm-hour",
                                "Alarm hour",
                                crate::gui::jump_mode::TargetKind::Slider,
                                owner_pass,
                                &mut jump_visual_order,
                            );
                            let minute_label = ui.label("Minute");
                            let old_minute = self.alarm_minute;
                            let minute = ui
                                .add(egui::DragValue::new(&mut self.alarm_minute).range(0..=59))
                                .labelled_by(minute_label.id);
                            register_timer_jump_target(
                                jump_scope.as_ref(),
                                ui,
                                &minute,
                                "alarm-minute",
                                "Alarm minute",
                                crate::gui::jump_mode::TargetKind::Slider,
                                owner_pass,
                                &mut jump_visual_order,
                            );
                            let changed = hour.changed() || minute.changed();
                            let authorized = (!hour.changed()
                                || response_is_authorized(ui, &hour, owner_pass))
                                && (!minute.changed()
                                    || response_is_authorized(ui, &minute, owner_pass));
                            if changed {
                                if authorized && let Some(date) = picker_date {
                                    self.time = self.canonical_alarm_time(date);
                                    self.time_text_observed.clone_from(&self.time);
                                    self.start_error = None;
                                } else {
                                    self.alarm_hour = old_hour;
                                    self.alarm_minute = old_minute;
                                }
                            }
                        });
                    });
                    secondary_help(ui, "Use HH:MM, Nd HH:MM, or YYYY-MM-DD HH:MM.");
                    #[cfg(test)]
                    {
                        self.alarm_preview = None;
                    }
                    if self.time.trim().is_empty() {
                        ui.label("Choose or enter an alarm time to preview it.");
                    } else {
                        match parse_alarm_spec(&self.time) {
                            Ok(spec) => match resolve_alarm_spec_at(spec, now(), |local| {
                                Local.from_local_datetime(&local)
                            }) {
                                Ok(target) => {
                                    let preview =
                                        target.format("%Y-%m-%d %H:%M:%S %:z").to_string();
                                    ui.label(format!("Next alarm: {preview}"));
                                    #[cfg(test)]
                                    {
                                        self.alarm_preview = Some(preview);
                                    }
                                }
                                Err(error) => {
                                    let message = error.to_string();
                                    ui.label(format!("Alarm preview unavailable: {message}"));
                                }
                            },
                            Err(_) => {
                                let message = "Enter a valid alarm date and time.".to_string();
                                ui.label(&message);
                            }
                        }
                    }
                }
            }

            ui.horizontal_wrapped(|ui| {
                let label = ui.label("Name");
                let old = self.label.clone();
                let response = ui
                    .add(
                        egui::TextEdit::singleline(&mut self.label)
                            .desired_width(ui.available_width().min(220.0).max(1.0)),
                    )
                    .labelled_by(label.id);
                if response.changed() {
                    if response_is_authorized(ui, &response, owner_pass) {
                        self.start_error = None;
                    } else {
                        self.label = old;
                    }
                }
                register_timer_jump_target(
                    jump_scope.as_ref(),
                    ui,
                    &response,
                    "name",
                    "Name",
                    crate::gui::jump_mode::TargetKind::TextEntry,
                    owner_pass,
                    &mut jump_visual_order,
                );
            });

            ui.horizontal_wrapped(|ui| {
                ui.label("Sound");
                let old_sound = self.sound_idx;
                let mut changed_option_authorized = false;
                let mut changed_option = false;
                let sound_combo = egui::ComboBox::from_id_salt("sound_select")
                    .width(ui.available_width().min(160.0).max(1.0))
                    .selected_text(crate::sound::SOUND_NAMES[self.sound_idx])
                    .show_ui(ui, |ui| {
                        let popup_scope = jump_scope
                            .as_ref()
                            .map(|scope| scope.combo_popup_scope("sound"));
                        for (idx, name) in crate::sound::SOUND_NAMES.iter().enumerate() {
                            let option = ui.selectable_value(&mut self.sound_idx, idx, *name);
                            if option.changed() {
                                changed_option = true;
                                changed_option_authorized =
                                    response_is_authorized(ui, &option, owner_pass);
                            }
                            if let Some(scope) = popup_scope.as_ref() {
                                register_timer_jump_target(
                                    Some(scope),
                                    ui,
                                    &option,
                                    format!("sound/option/{idx}"),
                                    format!("Sound: {name}"),
                                    crate::gui::jump_mode::TargetKind::Selectable,
                                    owner_pass,
                                    &mut jump_visual_order,
                                );
                            }
                        }
                    });
                if changed_option && !changed_option_authorized {
                    self.sound_idx = old_sound;
                } else if changed_option {
                    self.start_error = None;
                }
                if sound_combo
                    .response
                    .rect
                    .intersect(ui.clip_rect())
                    .is_positive()
                {
                    self.sound_popup = Some(TimerPopupAnchor {
                        response: sound_combo.response.id,
                        popup: sound_combo.response.id.with("popup"),
                        layer: sound_combo.response.layer_id,
                        viewport: ui.ctx().viewport_id(),
                        epoch: self.host_session_epoch,
                    });
                }
                register_timer_jump_target(
                    jump_scope.as_ref(),
                    ui,
                    &sound_combo.response,
                    "sound",
                    "Sound",
                    crate::gui::jump_mode::TargetKind::ComboBox,
                    owner_pass,
                    &mut jump_visual_order,
                );
                let play = ui.button("Play");
                register_timer_jump_target(
                    jump_scope.as_ref(),
                    ui,
                    &play,
                    "play",
                    "Play",
                    crate::gui::jump_mode::TargetKind::Button,
                    owner_pass,
                    &mut jump_visual_order,
                );
                if play.clicked() && response_is_authorized(ui, &play, owner_pass) {
                    let name = crate::sound::SOUND_NAMES[self.sound_idx];
                    if name == "None" {
                        self.start_error = Some("'None' is not a valid sound.".into());
                    } else {
                        effects.play_sound = Some(name);
                    }
                }
            });

            ui.horizontal_wrapped(|ui| {
                let start = ui.button("Start");
                let start = with_button_emphasis(ui, start, ButtonEmphasis::Primary);
                register_timer_jump_target(
                    jump_scope.as_ref(),
                    ui,
                    &start,
                    "start",
                    "Start",
                    crate::gui::jump_mode::TargetKind::Button,
                    owner_pass,
                    &mut jump_visual_order,
                );
                #[cfg(test)]
                {
                    effects.start_rect = Some(start.rect);
                    effects.start_clicked = Some(start.clicked());
                    effects.control_clips.push(ui.clip_rect());
                    effects.control_enabled.push(start.enabled());
                    effects.control_changed.push(start.changed());
                    effects.control_clicked.push(start.clicked());
                }
                if start.clicked() && response_is_authorized(ui, &start, owner_pass) {
                    let name = (!self.label.is_empty()).then(|| self.label.clone());
                    let sound = crate::sound::SOUND_NAMES[self.sound_idx].to_string();
                    let request = match self.mode {
                        Mode::Timer => parse_duration(&self.duration)
                            .ok_or_else(|| "Enter a positive, valid duration.".to_string())
                            .map(|duration| TimerSubmission::Timer {
                                duration,
                                name,
                                sound,
                            }),
                        Mode::Alarm => parse_alarm_spec(&self.time)
                            .map(|spec| TimerSubmission::Alarm { spec, name, sound })
                            .map_err(|_| "Enter a valid alarm date and time.".to_string()),
                    };
                    if presentation.is_some() {
                        match request {
                            Ok(request) => {
                                effects.submission = Some(request);
                                self.start_error = None;
                            }
                            Err(error) => self.start_error = Some(error),
                        }
                    } else {
                        match request.and_then(&mut submit) {
                            Ok(()) => {
                                close = true;
                                effects.focus_input = self.mode == Mode::Timer;
                            }
                            Err(error) => self.start_error = Some(error),
                        }
                    }
                }
                let cancel = ui.button("Cancel");
                #[cfg(test)]
                {
                    effects.cancel_rect = Some(cancel.rect);
                    effects.control_clips.push(ui.clip_rect());
                    effects.control_enabled.push(cancel.enabled());
                    effects.control_changed.push(cancel.changed());
                    effects.control_clicked.push(cancel.clicked());
                }
                register_timer_jump_target(
                    jump_scope.as_ref(),
                    ui,
                    &cancel,
                    "cancel",
                    "Cancel",
                    crate::gui::jump_mode::TargetKind::Button,
                    owner_pass,
                    &mut jump_visual_order,
                );
                if cancel.clicked() && response_is_authorized(ui, &cancel, owner_pass) {
                    close = true;
                }
            });
            if let Some(error) = self.start_error.as_deref() {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
            }
            #[cfg(test)]
            {
                effects.body_rect = Some(ui.min_rect());
                effects.body_clip = Some(ui.clip_rect());
            }
        };
        let shown = if let Some(ui) = child_ui {
            draw_body(ui);
            None
        } else {
            let window = egui::Window::new(title)
                .resizable(false)
                .max_width(ctx.content_rect().width().max(1.0));
            if presentation.is_some() {
                use super::utility_dialog_size::{
                    UtilityDialogSizeWindowExt as _, UtilityWindowSizeDefault,
                    UtilityWindowSizeRecipe,
                };

                window.show_utility(
                    ctx,
                    "timer",
                    egui::Id::new(Some(title)),
                    Some(&mut open_val),
                    UtilityWindowSizeRecipe::new(
                        UtilityWindowSizeDefault::outer(egui::vec2(480.0, 320.0)),
                        egui::Vec2::ZERO,
                        [None, None],
                    ),
                    draw_body,
                )
            } else {
                window.open(&mut open_val).show(ctx, draw_body)
            }
        };

        if close {
            open_val = false;
        }
        #[cfg(test)]
        {
            effects.window_rect = shown.as_ref().map(|response| response.response.rect);
        }
        let window_visible = shown.as_ref().is_some_and(|response| {
            response.response.rect.is_positive()
                && response.response.rect.intersects(ctx.content_rect())
        });
        if presentation.is_some() {
            effects.close = owner_current && first_ui_pass && !open_val;
        } else if first_ui_pass && window_visible && !ctx.will_discard() {
            self.open = open_val;
        }
        effects
    }
}

fn take_timer_copies(ctx: &egui::Context, start: usize) -> Vec<String> {
    ctx.output_mut(|output| {
        let mut copies = Vec::new();
        for command in output.commands.split_off(start) {
            match command {
                egui::OutputCommand::CopyText(text) => copies.push(text),
                other => output.commands.push(other),
            }
        }
        copies
    })
}

fn response_is_authorized(ui: &egui::Ui, response: &egui::Response, owner_pass: bool) -> bool {
    owner_pass
        && response.enabled()
        && ui.is_enabled()
        && ui.is_visible()
        && !ui.is_sizing_pass()
        && !ui.ctx().will_discard()
        && response.rect.is_positive()
        && response.rect.intersects(ui.clip_rect())
}

fn register_timer_jump_target(
    scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    ui: &egui::Ui,
    response: &egui::Response,
    local_identity: impl AsRef<str>,
    label: impl Into<String>,
    kind: crate::gui::jump_mode::TargetKind,
    allow_directive: bool,
    visual_order: &mut u64,
) {
    let order = *visual_order;
    *visual_order = visual_order.saturating_add(1);
    let Some(scope) = scope else {
        return;
    };
    match scope.register(ui, response, local_identity, label, kind, order) {
        crate::gui::jump_mode::ui::RegistrationResult::Registered {
            directive: Some(directive),
            ..
        } if allow_directive && response_is_authorized(ui, response, true) => {
            directive.apply_to_response(response);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn detached_sound_and_start_act_once_after_root_and_child_discard() {
        use crate::gui::jump_mode::ui::{JumpModeUi, NavigationOwner};
        use crate::gui::render::{
            IMMEDIATE_VIEWPORT_TEST_DISCARD, with_immediate_viewport_renderer,
            with_immediate_viewport_test_events_for,
        };
        use crate::gui::utility_host::UtilityPresentation;

        let ctx = egui::Context::default();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let launcher_jump = JumpModeUi::default();
        let mut dialog = TimerDialog::default();
        dialog.open_timer();
        dialog.duration = "17s".into();
        dialog.label = "Detached timer".into();
        assert!(
            dialog
                .host_state
                .as_mut()
                .unwrap()
                .transition(UtilityPresentation::Popout)
        );
        let viewport = TimerDialog::native_viewport();
        let submissions = RefCell::new(Vec::new());
        let sounds = RefCell::new(Vec::new());
        let render = |dialog: &mut TimerDialog, events, discard_root| {
            with_immediate_viewport_renderer(&ctx, || {
                with_immediate_viewport_test_events_for(viewport, events, || {
                    ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(900.0, 650.0),
                            )),
                            focused: false,
                            ..Default::default()
                        },
                        |ui| {
                            let first = ui.ctx().current_pass_index() == 0;
                            dialog.show_hosted(
                                ui.ctx(),
                                None,
                                Some(1),
                                NavigationOwner::Blocked,
                                true,
                                false,
                                &launcher_jump,
                                true,
                                false,
                                first,
                                false,
                            );
                            if first && discard_root {
                                ui.ctx().request_discard("Timer effect ROOT discard");
                            }
                        },
                    )
                    .drop_without_applying_deltas();
                });
            });
            let outcome = dialog.on_ui_frame_complete(&ctx);
            assert!(!outcome.restore_root_focus);
            dialog.apply_host_effects_with(
                &ctx,
                outcome,
                |submission| {
                    submissions.borrow_mut().push(submission);
                    Ok(())
                },
                |sound| sounds.borrow_mut().push(sound),
            );
        };
        let target = |dialog: &TimerDialog, label| {
            dialog
                .test_native_jump_targets()
                .iter()
                .find(|target| target.metadata().label() == label)
                .unwrap_or_else(|| panic!("native Timer target {label} is missing"))
                .clipped_rect()
                .center()
        };
        let pointer = |pos, pressed| {
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        for _ in 0..4 {
            render(&mut dialog, Vec::new(), false);
        }
        let open_sound = |dialog: &mut TimerDialog| {
            let sound = target(dialog, "Sound");
            for pressed in [true, false] {
                render(dialog, pointer(sound, pressed), false);
            }
            for _ in 0..4 {
                render(dialog, Vec::new(), false);
            }
        };
        open_sound(&mut dialog);
        let _ = target(&dialog, "Sound: Alarm.wav");
        for pressed in [true, false] {
            render(
                &mut dialog,
                vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: Some(egui::Key::Escape),
                    pressed,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                false,
            );
        }
        assert!(
            dialog.open,
            "Escape dismisses the Sound popup before the timer"
        );
        open_sound(&mut dialog);
        let alarm_sound = target(&dialog, "Sound: Alarm.wav");
        for pressed in [true, false] {
            render(&mut dialog, pointer(alarm_sound, pressed), false);
        }
        assert_eq!(dialog.sound_idx, 1);

        for label in ["Play", "Start"] {
            for discard_child in [false, true] {
                for _ in 0..3 {
                    render(&mut dialog, Vec::new(), false);
                }
                let action = target(&dialog, label);
                render(&mut dialog, pointer(action, true), false);
                if discard_child {
                    IMMEDIATE_VIEWPORT_TEST_DISCARD.with(|request| request.set(Some(viewport)));
                }
                render(&mut dialog, pointer(action, false), !discard_child);
                assert!(dialog.open);
                assert!(submissions.borrow().is_empty());
                assert!(sounds.borrow().is_empty());
            }
        }
        render(&mut dialog, Vec::new(), false);
        let play = target(&dialog, "Play");
        for pressed in [true, false] {
            render(&mut dialog, pointer(play, pressed), false);
        }
        assert_eq!(*sounds.borrow(), ["Alarm.wav"]);
        let start = target(&dialog, "Start");
        for pressed in [true, false] {
            render(&mut dialog, pointer(start, pressed), false);
        }
        assert!(!dialog.open);
        assert!(matches!(
            submissions.borrow().as_slice(),
            [TimerSubmission::Timer { duration, name: Some(name), sound }]
                if *duration == Duration::from_secs(17) && name == "Detached timer" && sound == "Alarm.wav"
        ));
    }

    #[test]
    fn retained_host_submission_keeps_draft_on_failure_then_closes_on_success() {
        let ctx = egui::Context::default();
        let mut dialog = super::TimerDialog::default();
        dialog.open_timer();
        dialog.duration = "5m".into();
        let make_outcome = |epoch| super::TimerHostOutcome {
            epoch,
            show_root: false,
            restore_root_focus: true,
            pinned: false,
            effects: super::TimerDialogEffects {
                submission: Some(super::TimerSubmission::Timer {
                    duration: std::time::Duration::from_secs(300),
                    name: None,
                    sound: "None".into(),
                }),
                ..Default::default()
            },
        };
        let epoch = dialog.host_session_epoch();
        let (closed, _) = dialog.apply_host_effects_with(
            &ctx,
            make_outcome(epoch),
            |_| Err("failed".into()),
            |_| {},
        );
        assert!(!closed && dialog.open);
        assert_eq!(dialog.duration, "5m");
        assert_eq!(dialog.start_error.as_deref(), Some("failed"));
        let (closed, focus) =
            dialog.apply_host_effects_with(&ctx, make_outcome(epoch), |_| Ok(()), |_| {});
        assert!(closed && focus && !dialog.open);
    }

    use super::{Mode, TimerDialog, TimerDialogEffects, TimerSubmission};
    use chrono::{Days, Local, TimeZone, Timelike};
    use eframe::egui;
    use std::cell::{Cell, RefCell};
    use std::time::Duration;

    fn run_runtime_frame(
        ctx: &egui::Context,
        dialog: &mut TimerDialog,
        frame: &mut usize,
        screen: egui::Vec2,
        first_ui_pass: bool,
        events: Vec<egui::Event>,
        now: chrono::DateTime<Local>,
        submit: &mut impl FnMut(TimerSubmission) -> Result<(), String>,
    ) -> TimerDialogEffects {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen)),
            time: Some(1.0 + *frame as f64 / 60.0),
            focused: true,
            events,
            ..Default::default()
        };
        *frame += 1;
        let mut effects = TimerDialogEffects::default();
        let output = ctx.run_ui(input, |ctx| {
            effects = dialog.ui_with_runtime(
                ctx,
                None,
                first_ui_pass,
                || now.clone(),
                |request| submit(request),
            );
        });
        output.drop_without_applying_deltas();
        effects
    }

    fn run_requested_discard_start_frame(
        ctx: &egui::Context,
        dialog: &mut TimerDialog,
        frame: &mut usize,
        screen: egui::Vec2,
        pos: egui::Pos2,
        now: chrono::DateTime<Local>,
        submit: &mut impl FnMut(TimerSubmission) -> Result<(), String>,
    ) -> (TimerDialogEffects, usize, Vec<bool>, Vec<bool>) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen)),
            time: Some(1.0 + *frame as f64 / 60.0),
            focused: true,
            events: vec![pointer_event(pos, false)],
            ..Default::default()
        };
        *frame += 1;
        let mut effects = TimerDialogEffects::default();
        let mut pass_count = 0;
        let mut discard_states = Vec::new();
        let mut clicked_states = Vec::new();
        let output = ctx.run_ui(input, |ctx| {
            pass_count += 1;
            let first_ui_pass = pass_count == 1;
            if first_ui_pass {
                ctx.request_discard("timer dialog discard-gate regression fixture");
            }
            discard_states.push(ctx.will_discard());
            effects = dialog.ui_with_runtime(
                ctx,
                None,
                first_ui_pass,
                || now.clone(),
                |request| submit(request),
            );
            clicked_states.push(effects.start_clicked.unwrap_or(false));
        });
        output.drop_without_applying_deltas();
        (effects, pass_count, discard_states, clicked_states)
    }

    fn pointer_event(position: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn click_at(
        ctx: &egui::Context,
        dialog: &mut TimerDialog,
        frame: &mut usize,
        screen: egui::Vec2,
        rect: egui::Rect,
        release_first_pass: bool,
        now: chrono::DateTime<Local>,
        submit: &mut impl FnMut(TimerSubmission) -> Result<(), String>,
    ) -> TimerDialogEffects {
        let pos = rect.center();
        run_runtime_frame(
            ctx,
            dialog,
            frame,
            screen,
            true,
            vec![egui::Event::PointerMoved(pos)],
            now.clone(),
            submit,
        );
        run_runtime_frame(
            ctx,
            dialog,
            frame,
            screen,
            true,
            vec![pointer_event(pos, true)],
            now.clone(),
            submit,
        );
        run_runtime_frame(
            ctx,
            dialog,
            frame,
            screen,
            release_first_pass,
            vec![pointer_event(pos, false)],
            now,
            submit,
        )
    }

    #[test]
    fn open_timer_resets_timer_fields_and_arms_duration_focus() {
        let mut dialog = TimerDialog {
            duration: "5m".into(),
            time: "12:30".into(),
            label: "Tea".into(),
            sound_idx: 2,
            ..Default::default()
        };

        dialog.open_timer();

        assert_eq!(dialog.mode, Mode::Timer);
        assert!(dialog.open);
        assert!(dialog.duration.is_empty());
        assert!(dialog.label.is_empty());
        assert_eq!(dialog.sound_idx, 0);
        assert!(dialog.focus_duration_next_frame);
        assert_eq!(dialog.time, "12:30");
        assert_eq!(dialog.title(), "Create Timer");
    }

    #[test]
    fn duration_focus_request_is_consumed_once() {
        let ctx = egui::Context::default();
        let screen = egui::vec2(900.0, 700.0);
        let now = Local.timestamp_opt(1_800_000_000, 0).single().unwrap();
        let mut dialog = TimerDialog::default();
        dialog.open_timer();
        let mut submit = |_| Err("not used".to_string());
        let mut frame = 0;

        for _ in 0..3 {
            run_runtime_frame(
                &ctx,
                &mut dialog,
                &mut frame,
                screen,
                false,
                Vec::new(),
                now.clone(),
                &mut submit,
            );
        }
        assert!(dialog.focus_duration_next_frame);

        run_runtime_frame(
            &ctx,
            &mut dialog,
            &mut frame,
            screen,
            true,
            Vec::new(),
            now.clone(),
            &mut submit,
        );
        assert!(!dialog.focus_duration_next_frame);

        run_runtime_frame(
            &ctx,
            &mut dialog,
            &mut frame,
            screen,
            true,
            Vec::new(),
            now,
            &mut submit,
        );
        assert!(!dialog.focus_duration_next_frame);
    }

    #[test]
    fn open_alarm_clears_pending_timer_focus_and_keeps_alarm_title() {
        let mut dialog = TimerDialog::default();
        dialog.open_timer();
        assert!(dialog.focus_duration_next_frame);

        dialog.open_alarm();

        assert!(!dialog.focus_duration_next_frame);
        assert_eq!(dialog.mode, Mode::Alarm);
        assert!(dialog.open);
        assert_eq!(dialog.title(), "Set Alarm");

        dialog.open_timer();
        assert_eq!(dialog.title(), "Create Timer");
    }

    #[test]
    fn typed_values_populate_numeric_controls_without_rewriting_drafts() {
        let mut dialog = TimerDialog::default();
        dialog.open_timer();
        dialog.duration = "01:02:03".into();
        dialog.observe_duration_text();

        assert_eq!(dialog.duration, "01:02:03");
        assert_eq!(
            (
                dialog.duration_hours,
                dialog.duration_minutes,
                dialog.duration_seconds
            ),
            (1, 2, 3)
        );

        dialog.duration = "incomplete".into();
        dialog.observe_duration_text();
        assert_eq!(dialog.duration, "incomplete");
        assert_eq!(
            (
                dialog.duration_hours,
                dialog.duration_minutes,
                dialog.duration_seconds
            ),
            (1, 2, 3),
            "invalid text remains authoritative until an explicit picker edit"
        );
    }

    #[test]
    fn alarm_picker_allows_empty_input_and_keeps_valid_date_prefix() {
        let mut dialog = TimerDialog::default();
        dialog.open_alarm();
        assert_eq!(
            dialog.alarm_picker_date(),
            Some(crate::plugins::timer::AlarmDateSpec::NextOccurrence)
        );
        dialog.alarm_hour = 7;
        dialog.alarm_minute = 35;
        dialog.time = dialog.canonical_alarm_time(dialog.alarm_picker_date().unwrap());
        dialog.time_text_observed.clone_from(&dialog.time);
        assert_eq!(dialog.time, "07:35");

        dialog.time = "2d 08:10".into();
        dialog.observe_alarm_text();
        dialog.alarm_hour = 9;
        dialog.time = dialog.canonical_alarm_time(dialog.alarm_picker_date().unwrap());
        assert_eq!(dialog.time, "2d 09:10");

        dialog.time = "invalid prefix 10:00".into();
        dialog.observe_alarm_text();
        assert_eq!(dialog.alarm_picker_date(), None);
        assert_eq!(dialog.time, "invalid prefix 10:00");
    }

    #[test]
    fn actual_alarm_picker_edits_empty_and_dated_drafts() {
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(1.0);
        let screen = egui::vec2(900.0, 700.0);
        let now = Local.timestamp_opt(1_800_000_000, 0).single().unwrap();
        let mut frame = 0;
        let mut submit = |_| Err("not used".to_string());
        let mut dialog = TimerDialog::default();
        dialog.open_alarm();

        let mut effects = TimerDialogEffects::default();
        for _ in 0..4 {
            effects = run_runtime_frame(
                &ctx,
                &mut dialog,
                &mut frame,
                screen,
                false,
                Vec::new(),
                now.clone(),
                &mut submit,
            );
        }
        let hour = effects
            .alarm_hour_rect
            .expect("actual alarm hour control is rendered");
        click_at(
            &ctx,
            &mut dialog,
            &mut frame,
            screen,
            hour,
            true,
            now.clone(),
            &mut submit,
        );
        run_runtime_frame(
            &ctx,
            &mut dialog,
            &mut frame,
            screen,
            true,
            vec![egui::Event::Key {
                key: egui::Key::ArrowUp,
                physical_key: Some(egui::Key::ArrowUp),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            now.clone(),
            &mut submit,
        );
        assert_eq!(
            dialog.time, "01:00",
            "blank input permits direct numeric selection"
        );

        dialog.time = "2d 06:30".into();
        dialog.time_text_observed.clear();
        for _ in 0..3 {
            effects = run_runtime_frame(
                &ctx,
                &mut dialog,
                &mut frame,
                screen,
                false,
                Vec::new(),
                now.clone(),
                &mut submit,
            );
        }
        let hour = effects
            .alarm_hour_rect
            .expect("dated hour control is rendered");
        click_at(
            &ctx,
            &mut dialog,
            &mut frame,
            screen,
            hour,
            true,
            now.clone(),
            &mut submit,
        );
        run_runtime_frame(
            &ctx,
            &mut dialog,
            &mut frame,
            screen,
            true,
            vec![egui::Event::Key {
                key: egui::Key::ArrowUp,
                physical_key: Some(egui::Key::ArrowUp),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            now,
            &mut submit,
        );
        assert_eq!(dialog.time, "2d 07:30");

        dialog.time = "invalid prefix 10:00".into();
        dialog.time_text_observed.clear();
        let effects = run_runtime_frame(
            &ctx,
            &mut dialog,
            &mut frame,
            screen,
            false,
            Vec::new(),
            now,
            &mut submit,
        );
        assert_eq!(effects.alarm_hour_enabled, Some(false));
        assert_eq!(dialog.time, "invalid prefix 10:00");
    }

    #[test]
    fn alarm_preview_and_start_use_the_current_local_clock() {
        let ctx = egui::Context::default();
        let screen = egui::vec2(900.0, 700.0);
        let seed = Local.timestamp_opt(1_800_000_000, 0).single().unwrap();
        let date = seed.date_naive();
        let before = Local
            .from_local_datetime(&date.and_hms_opt(8, 0, 0).unwrap())
            .single()
            .unwrap();
        let after = Local
            .from_local_datetime(&date.and_hms_opt(10, 0, 0).unwrap())
            .single()
            .unwrap();
        let clock = RefCell::new(before.clone());
        let mut dialog = TimerDialog::default();
        dialog.open_alarm();
        dialog.time = "09:00".into();
        let mut frame = 0;
        let submitted = std::rc::Rc::new(RefCell::new(None));
        let submitted_by_callback = submitted.clone();
        let submission_count = std::rc::Rc::new(Cell::new(0));
        let count_by_callback = submission_count.clone();
        let mut submit = |request| match request {
            TimerSubmission::Alarm { spec, .. } => {
                assert_eq!(
                    spec,
                    crate::plugins::timer::parse_alarm_spec("09:00").unwrap()
                );
                count_by_callback.set(count_by_callback.get() + 1);
                let resolved = crate::plugins::timer::resolve_alarm_spec_at(
                    spec,
                    clock.borrow().clone(),
                    |local| Local.from_local_datetime(&local),
                )
                .unwrap();
                *submitted_by_callback.borrow_mut() = Some(resolved);
                Ok(())
            }
            TimerSubmission::Timer { .. } => Err("unexpected timer submission".into()),
        };

        run_runtime_frame(
            &ctx,
            &mut dialog,
            &mut frame,
            screen,
            false,
            Vec::new(),
            before.clone(),
            &mut submit,
        );
        let first_preview = dialog
            .alarm_preview
            .clone()
            .expect("a valid alarm shows its target");
        let expected_first = crate::plugins::timer::resolve_alarm_spec_at(
            crate::plugins::timer::parse_alarm_spec("09:00").unwrap(),
            before,
            |local| Local.from_local_datetime(&local),
        )
        .unwrap();
        assert_eq!(
            first_preview,
            expected_first.format("%Y-%m-%d %H:%M:%S %:z").to_string()
        );
        assert!(first_preview.ends_with(&expected_first.format("%:z").to_string()));

        *clock.borrow_mut() = after.clone();
        let effects = run_runtime_frame(
            &ctx,
            &mut dialog,
            &mut frame,
            screen,
            false,
            Vec::new(),
            after.clone(),
            &mut submit,
        );
        let start = effects
            .start_rect
            .expect("Start is present on the visible dialog");
        let refreshed = dialog.alarm_preview.clone().unwrap();
        assert_ne!(
            refreshed, first_preview,
            "preview is recalculated from the new clock"
        );
        click_at(
            &ctx,
            &mut dialog,
            &mut frame,
            screen,
            start,
            false,
            after.clone(),
            &mut submit,
        );
        assert!(
            submitted.borrow().is_none(),
            "a discarded UI pass cannot start an alarm"
        );
        assert_eq!(submission_count.get(), 0);
        let effects = run_runtime_frame(
            &ctx,
            &mut dialog,
            &mut frame,
            screen,
            false,
            Vec::new(),
            after.clone(),
            &mut submit,
        );
        click_at(
            &ctx,
            &mut dialog,
            &mut frame,
            screen,
            effects
                .start_rect
                .expect("Start remains visible after discarded pass"),
            true,
            after.clone(),
            &mut submit,
        );
        drop(submit);
        assert_eq!(
            submission_count.get(),
            1,
            "one visible Start produces one submission"
        );
        let submitted = submitted
            .borrow()
            .clone()
            .expect("Start submits the typed alarm specification");
        assert_eq!(
            submitted.date_naive(),
            date.checked_add_days(Days::new(1)).unwrap()
        );
        assert_eq!((submitted.hour(), submitted.minute()), (9, 0));
        assert!(!dialog.open);
        run_runtime_frame(
            &ctx,
            &mut dialog,
            &mut frame,
            screen,
            false,
            Vec::new(),
            after,
            &mut |_| Err("closed dialog cannot submit".into()),
        );
        assert_eq!(
            submission_count.get(),
            1,
            "the next pass cannot duplicate Start"
        );
    }

    #[test]
    fn start_failure_stays_open_and_cancel_does_not_submit() {
        let ctx = egui::Context::default();
        let screen = egui::vec2(900.0, 700.0);
        let now = Local.timestamp_opt(1_800_000_000, 0).single().unwrap();
        let mut dialog = TimerDialog::default();
        dialog.open_timer();
        dialog.duration = "5s".into();
        let calls = std::rc::Rc::new(Cell::new(0));
        let start_calls = calls.clone();
        let mut submit = move |_| {
            start_calls.set(start_calls.get() + 1);
            Err("alarm store unavailable".to_string())
        };
        let mut frame = 0;
        let mut effects = TimerDialogEffects::default();
        for _ in 0..4 {
            effects = run_runtime_frame(
                &ctx,
                &mut dialog,
                &mut frame,
                screen,
                false,
                Vec::new(),
                now.clone(),
                &mut submit,
            );
        }
        click_at(
            &ctx,
            &mut dialog,
            &mut frame,
            screen,
            effects.start_rect.unwrap(),
            true,
            now.clone(),
            &mut submit,
        );
        drop(submit);
        assert_eq!(calls.get(), 1);
        assert!(dialog.open);
        assert_eq!(
            dialog.start_error.as_deref(),
            Some("alarm store unavailable")
        );

        let cancel_calls = calls.clone();
        let mut cancel_submit = move |_| {
            cancel_calls.set(cancel_calls.get() + 1);
            Err("Cancel must not submit a timer".to_string())
        };
        effects = run_runtime_frame(
            &ctx,
            &mut dialog,
            &mut frame,
            screen,
            false,
            Vec::new(),
            now.clone(),
            &mut cancel_submit,
        );
        click_at(
            &ctx,
            &mut dialog,
            &mut frame,
            screen,
            effects.cancel_rect.unwrap(),
            true,
            now,
            &mut cancel_submit,
        );
        assert_eq!(calls.get(), 1, "Cancel cannot call the scheduling owner");
        assert!(!dialog.open);
    }

    #[test]
    fn requested_discard_before_start_keeps_failure_and_draft_without_submission() {
        let ctx = egui::Context::default();
        let screen = egui::vec2(900.0, 700.0);
        let now = Local.timestamp_opt(1_800_000_000, 0).single().unwrap();
        let mut dialog = TimerDialog::default();
        dialog.open_timer();
        dialog.duration = "13m".into();
        dialog.start_error = Some("previous persistence error".into());
        let submissions = std::rc::Rc::new(Cell::new(0));
        let submitted = submissions.clone();
        let mut submit = move |_| {
            submitted.set(submitted.get() + 1);
            Ok(())
        };
        let mut frame = 0;
        let mut effects = TimerDialogEffects::default();
        for _ in 0..4 {
            effects = run_runtime_frame(
                &ctx,
                &mut dialog,
                &mut frame,
                screen,
                false,
                Vec::new(),
                now.clone(),
                &mut submit,
            );
        }
        let start = effects
            .start_rect
            .expect("visible Start response was rendered");
        let pos = start.center();
        run_runtime_frame(
            &ctx,
            &mut dialog,
            &mut frame,
            screen,
            true,
            vec![egui::Event::PointerMoved(pos)],
            now.clone(),
            &mut submit,
        );
        run_runtime_frame(
            &ctx,
            &mut dialog,
            &mut frame,
            screen,
            true,
            vec![pointer_event(pos, true)],
            now.clone(),
            &mut submit,
        );
        let (_, passes, discard_states, clicked_states) = run_requested_discard_start_frame(
            &ctx,
            &mut dialog,
            &mut frame,
            screen,
            pos,
            now,
            &mut submit,
        );

        assert!(
            passes >= 2,
            "egui reran the requested-discard frame: {passes}"
        );
        assert_eq!(discard_states.first(), Some(&true));
        assert_eq!(
            clicked_states.first(),
            Some(&true),
            "Start received the real click"
        );
        assert_eq!(
            submissions.get(),
            0,
            "discarded Start cannot reach scheduling"
        );
        assert!(dialog.open, "a discarded Start cannot close the dialog");
        assert_eq!(dialog.duration, "13m");
        assert_eq!(
            dialog.start_error.as_deref(),
            Some("previous persistence error"),
            "the precise prior failure remains visible"
        );
    }

    #[test]
    fn narrow_high_density_layout_keeps_picker_and_actions_visible() {
        let ctx = egui::Context::default();
        ctx.set_theme(egui::Theme::Dark);
        let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
        style.override_font_id = Some(egui::FontId::proportional(24.0));
        ctx.set_style_of(egui::Theme::Dark, style);
        ctx.set_pixels_per_point(2.0);
        let screen = egui::vec2(360.0, 480.0);
        let now = Local.timestamp_opt(1_800_000_000, 0).single().unwrap();
        let mut dialog = TimerDialog::default();
        dialog.open_timer();
        let mut submit = |_| Err("not used".into());
        let mut frame = 0;
        let mut effects = TimerDialogEffects::default();
        for _ in 0..5 {
            effects = run_runtime_frame(
                &ctx,
                &mut dialog,
                &mut frame,
                screen,
                false,
                Vec::new(),
                now.clone(),
                &mut submit,
            );
        }
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, screen);
        let rects = [
            effects.duration_hours_rect,
            effects.duration_minutes_rect,
            effects.duration_seconds_rect,
            effects.start_rect,
            effects.cancel_rect,
        ]
        .map(|rect| rect.expect("timer control was rendered"));
        let body_rect = effects.body_rect.expect("actual Window body was rendered");
        let body_clip = effects
            .body_clip
            .expect("actual Window body clip was recorded");
        let window_rect = effects
            .window_rect
            .expect("actual Window response was recorded");
        assert!(body_rect.is_positive());
        assert!(body_clip.is_positive());
        assert!(window_rect.contains_rect(body_rect));
        assert!(viewport.intersects(body_clip));
        assert_eq!(effects.control_clips.len(), rects.len());
        assert_eq!(effects.control_enabled.len(), rects.len());
        for (index, ((rect, clip), enabled)) in rects
            .into_iter()
            .zip(effects.control_clips.iter().copied())
            .zip(effects.control_enabled.iter().copied())
            .enumerate()
        {
            assert!(rect.is_positive());
            assert!(
                viewport.contains_rect(rect),
                "control {rect:?} must fit viewport {viewport:?}"
            );
            assert!(
                enabled,
                "control {index} must remain enabled at 200% theme metrics"
            );
            assert!(
                body_rect.contains_rect(rect),
                "control {rect:?} must fit the actual Window body {body_rect:?}"
            );
            assert!(
                clip.contains_rect(rect) && body_clip.contains_rect(rect),
                "control {index} must be fully inside its actual effective clip; \
                 rect={rect:?}, clip={clip:?}, body_clip={body_clip:?}"
            );
        }
    }

    #[test]
    fn actual_duration_picker_edits_submit_exact_timer_once() {
        let ctx = egui::Context::default();
        ctx.set_theme(egui::Theme::Light);
        let screen = egui::vec2(900.0, 700.0);
        let now = Local.timestamp_opt(1_800_000_000, 0).single().unwrap();
        let mut dialog = TimerDialog::default();
        dialog.open_timer();
        dialog.label = "Morning tea".into();
        dialog.sound_idx = 2;

        let submissions = std::rc::Rc::new(RefCell::new(Vec::new()));
        let submitted = submissions.clone();
        let mut submit = move |request| {
            submitted.borrow_mut().push(request);
            Ok(())
        };
        let mut frame = 0;
        let mut effects = TimerDialogEffects::default();
        for _ in 0..4 {
            effects = run_runtime_frame(
                &ctx,
                &mut dialog,
                &mut frame,
                screen,
                false,
                Vec::new(),
                now.clone(),
                &mut submit,
            );
        }

        let picker_rects = [
            effects.duration_hours_rect.unwrap(),
            effects.duration_minutes_rect.unwrap(),
            effects.duration_seconds_rect.unwrap(),
        ];
        let body_rect = effects.body_rect.unwrap();
        let body_clip = effects.body_clip.unwrap();
        let initial_controls = [
            picker_rects[0],
            picker_rects[1],
            picker_rects[2],
            effects.start_rect.unwrap(),
            effects.cancel_rect.unwrap(),
        ];
        for (index, rect) in initial_controls.into_iter().enumerate() {
            assert!(effects.control_enabled[index]);
            assert!(effects.control_clips[index].contains_rect(rect));
            assert!(body_rect.contains_rect(rect));
            assert!(body_clip.contains_rect(rect));
        }
        for (index, (rect, (value, expected))) in picker_rects
            .into_iter()
            .zip([("1", 1_u64), ("2", 2_u64), ("3", 3_u64)])
            .enumerate()
        {
            assert!(effects.control_enabled[index]);
            assert!(effects.control_clips[index].contains_rect(rect));
            let clicked = click_at(
                &ctx,
                &mut dialog,
                &mut frame,
                screen,
                rect,
                true,
                now.clone(),
                &mut submit,
            );
            assert_eq!(clicked.control_clicked[index], true);
            effects = run_runtime_frame(
                &ctx,
                &mut dialog,
                &mut frame,
                screen,
                true,
                vec![egui::Event::Text(value.into())],
                now.clone(),
                &mut submit,
            );
            assert_eq!(effects.control_changed[index], true);
            match value {
                "1" => assert_eq!(dialog.duration_hours, expected),
                "2" => assert_eq!(dialog.duration_minutes, expected as u32),
                "3" => assert_eq!(dialog.duration_seconds, expected as u32),
                _ => unreachable!(),
            }
        }

        assert_eq!(dialog.duration, "1:02:03");
        assert_eq!(
            super::parse_duration(&dialog.duration),
            Some(Duration::from_secs(3_723))
        );
        let start = effects
            .start_rect
            .expect("the actual Start response is rendered");
        assert!(effects.control_enabled[3]);
        assert!(effects.control_clips[3].contains_rect(start));
        let clicked = click_at(
            &ctx,
            &mut dialog,
            &mut frame,
            screen,
            start,
            true,
            now,
            &mut submit,
        );
        assert_eq!(clicked.start_clicked, Some(true));
        drop(submit);

        let submissions = submissions.borrow();
        assert_eq!(submissions.len(), 1, "one visible click submits one timer");
        assert!(matches!(
            submissions.first(),
            Some(TimerSubmission::Timer { duration, name, sound })
                if *duration == Duration::from_secs(3_723)
                    && name.as_deref() == Some("Morning tea")
                    && sound.as_str() == crate::sound::SOUND_NAMES[2]
        ));
        assert!(!dialog.open, "a successful actual Start closes the dialog");
    }

    #[cfg(feature = "qa-framebuffer-capture")]
    #[test]
    fn qa_observation_reports_the_actual_sound_selection() {
        let mut dialog = TimerDialog {
            sound_idx: 3,
            ..Default::default()
        };

        assert_eq!(dialog.qa_observation().sound_index, 3);

        dialog.open_timer();
        assert_eq!(dialog.qa_observation().sound_index, 0);
    }
}

#[derive(Default)]
pub struct TimerCompletionDialog {
    pub open: bool,
    msg: String,
}

impl TimerCompletionDialog {
    pub fn open_message(&mut self, msg: String) {
        self.msg = msg;
        self.open = true;
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let mut open_val = self.open;
        let mut close = false;
        egui::Window::new("Timer Finished")
            .collapsible(false)
            .resizable(false)
            .open(&mut open_val)
            .show(ctx, |ui| {
                ui.label(&self.msg);
                if ui.button("OK").clicked() {
                    close = true;
                }
            });
        if close {
            open_val = false;
        }
        self.open = open_val;
    }
}
