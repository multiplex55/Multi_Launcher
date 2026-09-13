use std::time::{Duration, Instant};

use eframe::egui;

use crate::hotkey::Key as HotkeyKey;
use crate::screen_draw::hotkeys::{
    LocalShortcutAction, LocalShortcutInput, LocalShortcutKey, LocalShortcutModifiers,
    resolve_local_shortcut,
};
use crate::screen_draw::window_layers::{
    ScreenDrawToolbarNativeBridge, SystemToolbarWindowBackend, TOOLBAR_WINDOW_TITLE,
    desktop_rect_from_logical_edges,
};
use crate::screen_draw::{
    CanvasBackground, DesktopPoint, DesktopRect, DesktopSize, ExportBackground, ExportDestination,
    ExportRequest, ExportScope, RgbaColor, ScreenDrawController, ScreenDrawMode,
    ScreenDrawSettings, ScreenDrawState, ScreenDrawTool, ToolbarOrientation,
    clamp_toolbar_position,
};

const SETTINGS_KEY: &str = "screen_draw";
const VERTICAL_TOOLBAR_SIZE_POINTS: egui::Vec2 = egui::vec2(264.0, 700.0);
const HORIZONTAL_TOOLBAR_SIZE_POINTS: egui::Vec2 = egui::vec2(700.0, 264.0);
const PERSIST_DEBOUNCE: Duration = Duration::from_millis(750);

pub(crate) fn viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("screen_draw_toolbar")
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ToolbarAction {
    Lifecycle(LifecycleAction),
    SetTool(ScreenDrawTool),
    SetColor(RgbaColor),
    SetPaletteColor(usize, RgbaColor),
    SetThickness(f32),
    Undo,
    Redo,
    SetAnnotationsVisible(bool),
    SetBackground(CanvasBackground),
    ToggleOrientation,
    Clear,
    Ghost,
    Finish,
    Export(ExportDestination, ExportBackground),
    RegionExport(ExportDestination, ExportBackground),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LifecycleAction {
    Start,
    NewCapture,
    Resume,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolbarControl {
    Start,
    DrawingTools,
    UndoRedo,
    Eye,
    Ghost,
    Background,
    Clear,
    Finish,
    Export,
    Resume,
    SessionControls,
}

fn controls_for_state(state: &ScreenDrawState) -> &'static [ToolbarControl] {
    use ToolbarControl::*;
    match state {
        ScreenDrawState::NoSession | ScreenDrawState::Failed { .. } => &[Start],
        ScreenDrawState::Drawing { .. } => &[
            DrawingTools,
            UndoRedo,
            Eye,
            Ghost,
            Background,
            Clear,
            Finish,
            SessionControls,
        ],
        ScreenDrawState::Ghost { .. } => &[
            Resume,
            UndoRedo,
            Eye,
            Background,
            Clear,
            Finish,
            SessionControls,
        ],
        ScreenDrawState::Finish { .. } => &[Resume, Eye, Export, Clear, SessionControls],
        ScreenDrawState::DisplayChanged { .. } => &[Export, SessionControls],
        ScreenDrawState::AwaitingLauncherParking { .. }
        | ScreenDrawState::AwaitingNativeTeardown { .. }
        | ScreenDrawState::Capturing { .. }
        | ScreenDrawState::SelectingRegion { .. } => &[SessionControls],
    }
}

#[derive(Default)]
pub(crate) struct ScreenDrawToolbarUi {
    pub(super) was_open: bool,
    initial_position_points: Option<egui::Pos2>,
    known_monitors: Vec<DesktopRect>,
    last_physical_position: Option<DesktopPoint>,
    last_physical_bounds: Option<DesktopRect>,
    native_bridge: ScreenDrawToolbarNativeBridge,
    last_observed_mode: Option<ScreenDrawMode>,
    settings_dirty_since: Option<Instant>,
    export_background: ExportBackground,
    #[cfg(test)]
    pub(super) focus_request_count: usize,
}

impl ScreenDrawToolbarUi {
    fn begin_open(&mut self, settings: &ScreenDrawSettings, pixels_per_point: f32) {
        self.native_bridge.begin_viewport();
        self.last_observed_mode = None;
        self.last_physical_bounds = None;
        self.known_monitors = current_monitor_rects();
        let requested = settings
            .toolbar_position
            .unwrap_or(DesktopPoint::new(24, 24));
        let toolbar_size = logical_size_to_physical(
            toolbar_size_points(settings.toolbar_orientation),
            pixels_per_point,
        );
        let recovered = recover_toolbar_position(requested, toolbar_size, &self.known_monitors);
        self.initial_position_points = Some(physical_to_logical(recovered, pixels_per_point));
        self.last_physical_position = Some(recovered);
    }

    fn note_position(&mut self, position: DesktopPoint, settings: &mut ScreenDrawSettings) {
        if self.last_physical_position == Some(position) {
            return;
        }
        self.last_physical_position = Some(position);
        settings.toolbar_position = Some(position);
        self.mark_dirty();
    }

    fn mark_dirty(&mut self) {
        self.settings_dirty_since.get_or_insert_with(Instant::now);
    }

    fn persistence_due(&self, now: Instant) -> bool {
        self.settings_dirty_since
            .is_some_and(|changed| now.saturating_duration_since(changed) >= PERSIST_DEBOUNCE)
    }

    fn clear_dirty(&mut self) {
        self.settings_dirty_since = None;
    }

    fn defer_persistence_retry(&mut self) {
        self.settings_dirty_since = Some(Instant::now());
    }

    fn is_dirty(&self) -> bool {
        self.settings_dirty_since.is_some()
    }
}

impl super::LauncherApp {
    pub(super) fn start_or_focus_screen_draw(&mut self) -> Result<bool, String> {
        match self.screen_draw_controller.state() {
            ScreenDrawState::NoSession | ScreenDrawState::Failed { .. } => {
                self.screen_draw_controller
                    .request_start()
                    .map_err(|error| error.to_string())?;
                self.egui_ctx.request_repaint();
                Ok(true)
            }
            ScreenDrawState::Drawing { .. }
            | ScreenDrawState::Ghost { .. }
            | ScreenDrawState::Finish { .. }
            | ScreenDrawState::DisplayChanged { .. } => {
                self.focus_screen_draw_toolbar();
                Ok(false)
            }
            ScreenDrawState::AwaitingLauncherParking { .. }
            | ScreenDrawState::Capturing { .. }
            | ScreenDrawState::SelectingRegion { .. }
            | ScreenDrawState::AwaitingNativeTeardown { .. } => Ok(false),
        }
    }

    pub(super) fn focus_screen_draw_toolbar(&mut self) {
        self.screen_draw_controller.open_toolbar();
        #[cfg(test)]
        {
            self.screen_draw_toolbar.focus_request_count += 1;
        }
        self.egui_ctx
            .send_viewport_cmd_to(viewport_id(), egui::ViewportCommand::Focus);
        self.egui_ctx.request_repaint();
    }

    pub(super) fn request_new_screen_draw_capture(&mut self) -> Result<(), String> {
        self.cancel_screen_draw_region_picker();
        self.clear_screen_draw_toolbar_native_bridge();
        self.screen_draw_controller
            .request_new_capture()
            .map_err(|error| error.to_string())?;
        // Native teardown is requested before the launcher is restored. The
        // replacement generation cannot capture until SessionClosed arrives.
        self.restore_screen_draw_launcher_exact()?;
        self.egui_ctx
            .send_viewport_cmd_to(viewport_id(), egui::ViewportCommand::Close);
        self.screen_draw_toolbar.was_open = false;
        Ok(())
    }

    pub(super) fn resume_screen_draw(&mut self) -> Result<(), String> {
        let generation = match self.screen_draw_controller.state() {
            ScreenDrawState::Ghost { generation } | ScreenDrawState::Finish { generation } => {
                *generation
            }
            state => return Err(format!("cannot resume Screen Draw while in {state:?}")),
        };
        let virtual_desktop = self
            .screen_draw_controller
            .session_snapshot()
            .map(|snapshot| snapshot.virtual_desktop())
            .ok_or_else(|| "Screen Draw resume has no retained desktop snapshot".to_string())?;

        if self
            .screen_draw_launcher_parking
            .as_ref()
            .is_some_and(|transaction| transaction.generation() != generation)
        {
            if let Some(transaction) = self.screen_draw_launcher_parking.as_mut() {
                transaction.restore()?;
            }
            self.screen_draw_launcher_parking = None;
        }

        let parking_result = if let Some(transaction) = self.screen_draw_launcher_parking.as_mut() {
            match transaction.verify() {
                Ok(true) => Ok(()),
                Ok(false) => transaction.update_snapshot_before_repark(virtual_desktop),
                Err(error) => Err(error),
            }
        } else {
            let hwnd = self
                .launcher_hwnd
                .ok_or_else(|| "launcher HWND is unavailable for Screen Draw resume".to_string())?;
            crate::screen_draw::launcher_parking::LauncherParkingTransaction::begin(
                generation,
                hwnd,
                virtual_desktop,
            )
            .map(|transaction| self.screen_draw_launcher_parking = Some(transaction))
        };
        if let Err(error) = parking_result {
            let _ = self.restore_screen_draw_launcher_exact();
            return Err(error);
        }
        match self
            .screen_draw_launcher_parking
            .as_ref()
            .expect("resume parking creates or updates one transaction")
            .verify()
        {
            Ok(true) => {}
            Ok(false) => {
                let _ = self.restore_screen_draw_launcher_exact();
                return Err("launcher did not reach a capture-safe position for resume".into());
            }
            Err(error) => {
                let _ = self.restore_screen_draw_launcher_exact();
                return Err(error);
            }
        }

        self.visible_flag
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.last_visible = false;
        self.restore_flag
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let toolbar = self
            .screen_draw_toolbar
            .native_bridge
            .synchronize_for_resume(
                generation.get(),
                self.screen_draw_toolbar.last_physical_bounds,
                &SystemToolbarWindowBackend,
            );
        if let Err(error) = self.screen_draw_controller.set_toolbar_window(toolbar) {
            let _ = self.restore_screen_draw_launcher_exact();
            return Err(error.to_string());
        }
        if let Err(error) = self.screen_draw_controller.resume_drawing() {
            let _ = self.restore_screen_draw_launcher_exact();
            return Err(error.to_string());
        }
        self.screen_draw_launcher_parking
            .as_mut()
            .expect("verified resume parking remains owned")
            .commit_hidden();
        Ok(())
    }

    pub(super) fn close_screen_draw_session(&mut self) -> Result<(), String> {
        self.cancel_screen_draw_region_picker();
        self.clear_screen_draw_toolbar_native_bridge();
        // Controller close disarms native input before launcher restoration.
        self.screen_draw_controller.close();
        self.restore_screen_draw_launcher_exact()?;
        self.egui_ctx
            .send_viewport_cmd_to(viewport_id(), egui::ViewportCommand::Close);
        self.screen_draw_toolbar.was_open = false;
        Ok(())
    }

    fn apply_screen_draw_lifecycle(
        &mut self,
        action: LifecycleAction,
    ) -> Result<ActionEffect, String> {
        let mut effect = ActionEffect::default();
        match action {
            LifecycleAction::Start => {
                effect.close_viewport = self.start_or_focus_screen_draw()?;
            }
            LifecycleAction::NewCapture => {
                self.request_new_screen_draw_capture()?;
                effect.close_viewport = true;
            }
            LifecycleAction::Resume => self.resume_screen_draw()?,
            LifecycleAction::Close => {
                self.close_screen_draw_session()?;
                effect.close_viewport = true;
            }
        }
        Ok(effect)
    }

    pub(super) fn show_screen_draw_toolbar(&mut self, ctx: &egui::Context) {
        let open = self.screen_draw_controller.toolbar_open();
        if !open {
            if self.screen_draw_toolbar.was_open {
                self.clear_screen_draw_toolbar_native_bridge();
            }
            self.screen_draw_toolbar.was_open = false;
            return;
        }

        if !self.screen_draw_toolbar.was_open {
            let pixels_per_point = ctx.native_pixels_per_point().unwrap_or(1.0).max(0.25);
            self.screen_draw_toolbar
                .begin_open(self.screen_draw_controller.settings(), pixels_per_point);
            self.screen_draw_toolbar.was_open = true;
        }

        let state = self.screen_draw_controller.state().clone();
        let runtime = self.screen_draw_controller.runtime_state();
        let observed_mode = runtime.map(|runtime| runtime.mode);
        if self.screen_draw_toolbar.last_observed_mode.is_some()
            && self.screen_draw_toolbar.last_observed_mode != observed_mode
        {
            self.screen_draw_toolbar
                .native_bridge
                .observe_layer_transition();
        }
        self.screen_draw_toolbar.last_observed_mode = observed_mode;
        let settings = self.screen_draw_controller.settings().clone();
        let export_error = self
            .screen_draw_controller
            .latest_runtime_error()
            .map(str::to_owned);
        let export_in_flight = self.screen_draw_controller.export_in_flight();
        let mut export_background = self.screen_draw_toolbar.export_background;
        let initial_position = self.screen_draw_toolbar.initial_position_points;
        let mut actions = Vec::new();
        let mut close_requested = false;
        let mut observed_position = None;
        let mut observed_outer_rect = None;
        let mut observed_pixels_per_point = None;
        let mut escape_pressed = false;

        let toolbar_size = toolbar_size_points(settings.toolbar_orientation);
        let builder = toolbar_viewport_builder(toolbar_size, initial_position);

        ctx.show_viewport_immediate(viewport_id(), builder, |child, _| {
            // Consume configured shortcuts before widgets see this frame's key
            // events. Otherwise Space/Enter may both switch tools and activate
            // whichever toolbar button retained focus from the prior frame.
            let shortcut_actions = consume_toolbar_local_shortcuts(child, &state, &settings);
            let thickness = runtime.map_or(settings.default_thickness, |state| state.thickness);
            actions.extend(toolbar_actions_for_local_shortcuts(
                shortcut_actions,
                thickness,
            ));
            render_toolbar(
                child,
                &state,
                runtime,
                &settings,
                export_error.as_deref(),
                export_in_flight,
                &mut export_background,
                &mut actions,
            );
            child.input(|input| {
                let viewport = input.viewport();
                observed_position = viewport.outer_rect.map(|rect| rect.min);
                observed_outer_rect = viewport.outer_rect;
                close_requested = viewport.close_requested();
            });
            observed_pixels_per_point = Some(child.pixels_per_point());
            escape_pressed = consume_drawing_escape(child, &state);
        });
        self.screen_draw_toolbar.export_background = export_background;

        if let (Some(position), Some(scale)) = (observed_position, observed_pixels_per_point) {
            let physical = logical_to_physical(position, scale);
            let settings = self.screen_draw_controller.settings().clone();
            let mut updated = settings;
            self.screen_draw_toolbar
                .note_position(physical, &mut updated);
            self.screen_draw_controller.update_settings(updated);
        }
        let physical_bounds =
            observed_outer_rect
                .zip(observed_pixels_per_point)
                .and_then(|(rect, scale)| {
                    desktop_rect_from_logical_edges(
                        rect.min.x, rect.min.y, rect.max.x, rect.max.y, scale,
                    )
                });
        if let Some(bounds) = physical_bounds {
            self.screen_draw_toolbar.last_physical_bounds = Some(bounds);
        }
        let bridge_fallback = physical_bounds.or(self.screen_draw_toolbar.last_physical_bounds);
        if let Some(toolbar) = self.screen_draw_toolbar.native_bridge.synchronize(
            state.generation().map(|generation| generation.get()),
            bridge_fallback,
            &SystemToolbarWindowBackend,
        ) {
            if let Err(error) = self.screen_draw_controller.set_toolbar_window(toolbar) {
                self.report_error_message("screen_draw.toolbar", error.to_string());
            }
        }
        if self.screen_draw_toolbar.native_bridge.resolution_pending() {
            // Bounded child-viewport creation retries; this stops as soon as
            // the handle resolves or the fixed attempt budget is exhausted.
            ctx.request_repaint();
        }
        if close_requested {
            actions.push(ToolbarAction::Lifecycle(LifecycleAction::Close));
        }
        if escape_pressed {
            actions.insert(0, ToolbarAction::Ghost);
        }

        let mut preferences_changed = false;
        let mut should_close_viewport = false;
        for action in actions {
            if matches!(action, ToolbarAction::Ghost | ToolbarAction::Finish)
                && let Err(error) = self.revalidate_screen_draw_toolbar_for_layer_transition()
            {
                self.report_error_message("screen_draw.toolbar", error);
            }
            let result = match action {
                ToolbarAction::Lifecycle(action) => self.apply_screen_draw_lifecycle(action),
                action => apply_controller_toolbar_action(&mut self.screen_draw_controller, action),
            };
            match result {
                Ok(effect) => {
                    preferences_changed |= effect.preferences_changed;
                    should_close_viewport |= effect.close_viewport;
                }
                Err(error) => self.report_error_message("screen_draw.toolbar", error),
            }
        }
        if preferences_changed {
            self.screen_draw_toolbar.mark_dirty();
        }
        if should_close_viewport || !self.screen_draw_controller.toolbar_open() {
            // Lifecycle actions perform their launcher work before requesting
            // this viewport-only close.
            self.clear_screen_draw_toolbar_native_bridge();
            ctx.send_viewport_cmd_to(viewport_id(), egui::ViewportCommand::Close);
            self.screen_draw_toolbar.was_open = false;
        }

        if should_close_viewport && self.screen_draw_toolbar.is_dirty() {
            self.persist_screen_draw_settings();
        } else if self.screen_draw_toolbar.persistence_due(Instant::now()) {
            self.persist_screen_draw_settings();
        } else if self.screen_draw_toolbar.is_dirty() {
            ctx.request_repaint_after(PERSIST_DEBOUNCE);
        }
    }

    fn persist_screen_draw_settings(&mut self) {
        let screen_draw = self.screen_draw_controller.settings().clone();
        match update_screen_draw_settings(&self.settings_path, &screen_draw) {
            Ok(_) => self.screen_draw_toolbar.clear_dirty(),
            Err(error) => {
                self.screen_draw_toolbar.defer_persistence_retry();
                self.report_error_message(
                    "screen_draw.settings",
                    format!("Failed to save Screen Draw preferences: {error}"),
                )
            }
        }
    }

    pub(super) fn close_screen_draw_for_exit(&mut self) {
        // Native teardown is synchronous to initiate: the worker receives its
        // shutdown request before UI state is discarded or settings are saved.
        self.cancel_screen_draw_region_picker();
        self.clear_screen_draw_toolbar_native_bridge();
        self.screen_draw_controller.close();
        if self.screen_draw_toolbar.is_dirty() {
            self.persist_screen_draw_settings();
        }
    }

    fn clear_screen_draw_toolbar_native_bridge(&mut self) {
        if self.screen_draw_toolbar.native_bridge.close_viewport() {
            if let Err(error) = self.screen_draw_controller.set_toolbar_window(None) {
                tracing::warn!(%error, "failed to clear Screen Draw toolbar native bridge");
            }
        }
        self.screen_draw_toolbar.last_physical_bounds = None;
        self.screen_draw_toolbar.last_observed_mode = None;
    }

    fn revalidate_screen_draw_toolbar_for_layer_transition(&mut self) -> Result<(), String> {
        self.screen_draw_toolbar
            .native_bridge
            .observe_layer_transition();
        let generation = self
            .screen_draw_controller
            .state()
            .generation()
            .map(|generation| generation.get());
        if let Some(toolbar) = self.screen_draw_toolbar.native_bridge.synchronize(
            generation,
            self.screen_draw_toolbar.last_physical_bounds,
            &SystemToolbarWindowBackend,
        ) {
            self.screen_draw_controller
                .set_toolbar_window(toolbar)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

fn toolbar_viewport_builder(
    toolbar_size: egui::Vec2,
    initial_position: Option<egui::Pos2>,
) -> egui::ViewportBuilder {
    let builder = egui::ViewportBuilder::default()
        .with_title(TOOLBAR_WINDOW_TITLE)
        .with_inner_size(toolbar_size)
        .with_min_inner_size(toolbar_size)
        .with_max_inner_size(toolbar_size)
        .with_resizable(false)
        .with_always_on_top()
        .with_taskbar(false)
        .with_minimize_button(false)
        .with_maximize_button(false);
    match initial_position {
        Some(position) => builder.with_position(position),
        None => builder,
    }
}

fn consume_drawing_escape(ctx: &egui::Context, state: &ScreenDrawState) -> bool {
    matches!(state, ScreenDrawState::Drawing { .. })
        && ctx.input_mut(|input| {
            let modifiers = input.modifiers;
            input.consume_key(modifiers, egui::Key::Escape)
        })
}

fn consume_toolbar_local_shortcuts(
    ctx: &egui::Context,
    state: &ScreenDrawState,
    settings: &ScreenDrawSettings,
) -> Vec<LocalShortcutAction> {
    consume_toolbar_local_shortcuts_with_win_modifier(
        ctx,
        state,
        settings,
        current_egui_win_modifier,
    )
}

fn consume_toolbar_local_shortcuts_with_win_modifier(
    ctx: &egui::Context,
    state: &ScreenDrawState,
    settings: &ScreenDrawSettings,
    mut read_win_modifier: impl FnMut() -> bool,
) -> Vec<LocalShortcutAction> {
    if !matches!(state, ScreenDrawState::Drawing { .. })
        || !ctx.input(|input| input.focused)
        || toolbar_text_editing_owns_keyboard(ctx)
    {
        return Vec::new();
    }

    ctx.input_mut(|input| {
        let mut actions = Vec::new();
        let mut win = None;
        input.events.retain(|event| {
            let egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                modifiers,
                ..
            } = event
            else {
                return true;
            };
            let Some(key) = egui_key_to_local_shortcut(*key) else {
                return true;
            };
            let win = *win.get_or_insert_with(&mut read_win_modifier);
            let shortcut_input = egui_local_shortcut_input(key, *modifiers, win, false);
            let Some(action) = resolve_local_shortcut(settings, shortcut_input) else {
                return true;
            };
            actions.push(action);
            false
        });
        actions
    })
}

fn egui_local_shortcut_input(
    key: LocalShortcutKey,
    modifiers: egui::Modifiers,
    win: bool,
    text_editing: bool,
) -> LocalShortcutInput {
    LocalShortcutInput {
        key,
        modifiers: LocalShortcutModifiers {
            alt: modifiers.alt,
            ctrl: modifiers.ctrl,
            shift: modifiers.shift,
            win,
        },
        text_editing,
    }
}

fn toolbar_text_editing_owns_keyboard(ctx: &egui::Context) -> bool {
    if !ctx.wants_keyboard_input() {
        return false;
    }
    let focused = ctx.memory(|memory| memory.focused());
    focused.is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some())
}

fn toolbar_action_for_local_shortcut(action: LocalShortcutAction, thickness: f32) -> ToolbarAction {
    match action {
        LocalShortcutAction::Tool(tool) => ToolbarAction::SetTool(tool),
        LocalShortcutAction::Undo => ToolbarAction::Undo,
        LocalShortcutAction::Redo => ToolbarAction::Redo,
        LocalShortcutAction::IncreaseThickness => {
            ToolbarAction::SetThickness((thickness + 1.0).clamp(0.5, 64.0))
        }
        LocalShortcutAction::DecreaseThickness => {
            ToolbarAction::SetThickness((thickness - 1.0).clamp(0.5, 64.0))
        }
        LocalShortcutAction::Color(color) => ToolbarAction::SetColor(color),
    }
}

fn toolbar_actions_for_local_shortcuts(
    actions: Vec<LocalShortcutAction>,
    initial_thickness: f32,
) -> Vec<ToolbarAction> {
    let mut thickness = initial_thickness;
    actions
        .into_iter()
        .map(|action| {
            let action = toolbar_action_for_local_shortcut(action, thickness);
            if let ToolbarAction::SetThickness(next) = action {
                thickness = next;
            }
            action
        })
        .collect()
}

fn egui_key_to_local_shortcut(key: egui::Key) -> Option<LocalShortcutKey> {
    let key = match key {
        egui::Key::OpenBracket => return Some(LocalShortcutKey::OpenBracket),
        egui::Key::CloseBracket => return Some(LocalShortcutKey::CloseBracket),
        egui::Key::Space => HotkeyKey::Space,
        egui::Key::Tab => HotkeyKey::Tab,
        egui::Key::Enter => HotkeyKey::Return,
        egui::Key::Delete => HotkeyKey::Delete,
        egui::Key::Backspace => HotkeyKey::Backspace,
        egui::Key::Home => HotkeyKey::Home,
        egui::Key::End => HotkeyKey::End,
        egui::Key::PageUp => HotkeyKey::PageUp,
        egui::Key::PageDown => HotkeyKey::PageDown,
        egui::Key::ArrowLeft => HotkeyKey::LeftArrow,
        egui::Key::ArrowUp => HotkeyKey::UpArrow,
        egui::Key::ArrowRight => HotkeyKey::RightArrow,
        egui::Key::ArrowDown => HotkeyKey::DownArrow,
        egui::Key::Num0 => HotkeyKey::Num0,
        egui::Key::Num1 => HotkeyKey::Num1,
        egui::Key::Num2 => HotkeyKey::Num2,
        egui::Key::Num3 => HotkeyKey::Num3,
        egui::Key::Num4 => HotkeyKey::Num4,
        egui::Key::Num5 => HotkeyKey::Num5,
        egui::Key::Num6 => HotkeyKey::Num6,
        egui::Key::Num7 => HotkeyKey::Num7,
        egui::Key::Num8 => HotkeyKey::Num8,
        egui::Key::Num9 => HotkeyKey::Num9,
        egui::Key::A => HotkeyKey::KeyA,
        egui::Key::B => HotkeyKey::KeyB,
        egui::Key::C => HotkeyKey::KeyC,
        egui::Key::D => HotkeyKey::KeyD,
        egui::Key::E => HotkeyKey::KeyE,
        egui::Key::F => HotkeyKey::KeyF,
        egui::Key::G => HotkeyKey::KeyG,
        egui::Key::H => HotkeyKey::KeyH,
        egui::Key::I => HotkeyKey::KeyI,
        egui::Key::J => HotkeyKey::KeyJ,
        egui::Key::K => HotkeyKey::KeyK,
        egui::Key::L => HotkeyKey::KeyL,
        egui::Key::M => HotkeyKey::KeyM,
        egui::Key::N => HotkeyKey::KeyN,
        egui::Key::O => HotkeyKey::KeyO,
        egui::Key::P => HotkeyKey::KeyP,
        egui::Key::Q => HotkeyKey::KeyQ,
        egui::Key::R => HotkeyKey::KeyR,
        egui::Key::S => HotkeyKey::KeyS,
        egui::Key::T => HotkeyKey::KeyT,
        egui::Key::U => HotkeyKey::KeyU,
        egui::Key::V => HotkeyKey::KeyV,
        egui::Key::W => HotkeyKey::KeyW,
        egui::Key::X => HotkeyKey::KeyX,
        egui::Key::Y => HotkeyKey::KeyY,
        egui::Key::Z => HotkeyKey::KeyZ,
        egui::Key::F1 => HotkeyKey::F1,
        egui::Key::F2 => HotkeyKey::F2,
        egui::Key::F3 => HotkeyKey::F3,
        egui::Key::F4 => HotkeyKey::F4,
        egui::Key::F5 => HotkeyKey::F5,
        egui::Key::F6 => HotkeyKey::F6,
        egui::Key::F7 => HotkeyKey::F7,
        egui::Key::F8 => HotkeyKey::F8,
        egui::Key::F9 => HotkeyKey::F9,
        egui::Key::F10 => HotkeyKey::F10,
        egui::Key::F11 => HotkeyKey::F11,
        egui::Key::F12 => HotkeyKey::F12,
        egui::Key::F13 => HotkeyKey::F13,
        egui::Key::F14 => HotkeyKey::F14,
        egui::Key::F15 => HotkeyKey::F15,
        egui::Key::F16 => HotkeyKey::F16,
        egui::Key::F17 => HotkeyKey::F17,
        egui::Key::F18 => HotkeyKey::F18,
        egui::Key::F19 => HotkeyKey::F19,
        egui::Key::F20 => HotkeyKey::F20,
        egui::Key::F21 => HotkeyKey::F21,
        egui::Key::F22 => HotkeyKey::F22,
        egui::Key::F23 => HotkeyKey::F23,
        egui::Key::F24 => HotkeyKey::F24,
        _ => return None,
    };
    Some(LocalShortcutKey::Standard(key))
}

#[cfg(target_os = "windows")]
fn current_egui_win_modifier() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState;

    unsafe { GetKeyState(0x5B) < 0 || GetKeyState(0x5C) < 0 }
}

#[cfg(not(target_os = "windows"))]
fn current_egui_win_modifier() -> bool {
    false
}

#[derive(Default)]
struct ActionEffect {
    preferences_changed: bool,
    close_viewport: bool,
}

fn apply_controller_toolbar_action(
    controller: &mut ScreenDrawController,
    action: ToolbarAction,
) -> Result<ActionEffect, String> {
    let mut effect = ActionEffect::default();
    match action {
        ToolbarAction::Lifecycle(_) => unreachable!("lifecycle actions are owned by LauncherApp"),
        ToolbarAction::SetTool(tool) => {
            if active_session(controller.state()) {
                controller
                    .set_tool(tool)
                    .map_err(|error| error.to_string())?;
            }
            let mut settings = controller.settings().clone();
            settings.default_tool = tool;
            controller.update_settings(settings);
            effect.preferences_changed = true;
        }
        ToolbarAction::SetColor(color) => {
            if active_session(controller.state()) {
                controller
                    .set_color(color)
                    .map_err(|error| error.to_string())?;
            }
            let mut settings = controller.settings().clone();
            settings.default_color = color;
            controller.update_settings(settings);
            effect.preferences_changed = true;
        }
        ToolbarAction::SetPaletteColor(index, color) => {
            let mut settings = controller.settings().clone();
            let Some(slot) = settings.palette.get_mut(index) else {
                return Err(format!("invalid palette slot {}", index + 1));
            };
            *slot = color;
            settings.default_color = color;
            if active_session(controller.state()) {
                controller
                    .set_color(color)
                    .map_err(|error| error.to_string())?;
            }
            controller.update_settings(settings);
            effect.preferences_changed = true;
        }
        ToolbarAction::SetThickness(thickness) => {
            if active_session(controller.state()) {
                controller
                    .set_thickness(thickness)
                    .map_err(|error| error.to_string())?;
            }
            let mut settings = controller.settings().clone();
            settings.default_thickness = thickness;
            controller.update_settings(settings);
            effect.preferences_changed = true;
        }
        ToolbarAction::Undo => controller.undo().map_err(|error| error.to_string())?,
        ToolbarAction::Redo => controller.redo().map_err(|error| error.to_string())?,
        ToolbarAction::SetAnnotationsVisible(visible) => controller
            .set_annotations_visible(visible)
            .map_err(|error| error.to_string())?,
        ToolbarAction::SetBackground(background) => {
            if active_session(controller.state()) {
                controller
                    .set_background(background)
                    .map_err(|error| error.to_string())?;
            }
            let mut settings = controller.settings().clone();
            settings.default_background = background;
            if let CanvasBackground::Solid(color) = background {
                settings.custom_background = color;
            }
            controller.update_settings(settings);
            effect.preferences_changed = true;
        }
        ToolbarAction::ToggleOrientation => {
            let mut settings = controller.settings().clone();
            settings.toolbar_orientation = match settings.toolbar_orientation {
                ToolbarOrientation::Vertical => ToolbarOrientation::Horizontal,
                ToolbarOrientation::Horizontal => ToolbarOrientation::Vertical,
            };
            controller.update_settings(settings);
            effect.preferences_changed = true;
        }
        ToolbarAction::Clear => controller
            .request_clear()
            .map_err(|error| error.to_string())?,
        ToolbarAction::Ghost => controller
            .enter_ghost()
            .map_err(|error| error.to_string())?,
        ToolbarAction::Finish => controller.finish().map_err(|error| error.to_string())?,
        ToolbarAction::Export(destination, background) => controller
            .request_export(ExportRequest {
                scope: ExportScope::FullDesktop,
                background,
                destination,
            })
            .map_err(|error| error.to_string())?,
        ToolbarAction::RegionExport(destination, background) => {
            controller
                .begin_region_selection(background, destination)
                .map_err(|error| error.to_string())?;
            effect.close_viewport = true;
        }
    }
    Ok(effect)
}

fn active_session(state: &ScreenDrawState) -> bool {
    matches!(
        state,
        ScreenDrawState::Drawing { .. }
            | ScreenDrawState::Ghost { .. }
            | ScreenDrawState::Finish { .. }
    )
}

fn render_toolbar(
    ctx: &egui::Context,
    state: &ScreenDrawState,
    runtime: Option<crate::screen_draw::NativeRuntimeState>,
    settings: &ScreenDrawSettings,
    export_error: Option<&str>,
    export_in_flight: bool,
    export_background: &mut ExportBackground,
    actions: &mut Vec<ToolbarAction>,
) {
    egui::CentralPanel::default().show(ctx, |ui| {
        let mut contents = |ui: &mut egui::Ui| {
            render_toolbar_contents(
                ui,
                state,
                runtime,
                settings,
                export_error,
                export_in_flight,
                export_background,
                actions,
            )
        };
        match settings.toolbar_orientation {
            ToolbarOrientation::Vertical => {
                egui::ScrollArea::vertical().show(ui, &mut contents);
            }
            ToolbarOrientation::Horizontal => {
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    ui.horizontal(&mut contents);
                });
            }
        }
    });
}

fn render_toolbar_contents(
    ui: &mut egui::Ui,
    state: &ScreenDrawState,
    runtime: Option<crate::screen_draw::NativeRuntimeState>,
    settings: &ScreenDrawSettings,
    export_error: Option<&str>,
    export_in_flight: bool,
    export_background: &mut ExportBackground,
    actions: &mut Vec<ToolbarAction>,
) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.heading("Screen Draw");
            let label = match settings.toolbar_orientation {
                ToolbarOrientation::Vertical => "Use horizontal layout",
                ToolbarOrientation::Horizontal => "Use vertical layout",
            };
            if ui.small_button("↔").on_hover_text(label).clicked() {
                actions.push(ToolbarAction::ToggleOrientation);
            }
        });
        ui.small(state_label(state));
    });
    ui.separator();

    match state {
        ScreenDrawState::NoSession | ScreenDrawState::Failed { .. } => {
            if ui.button("Start / New Capture").clicked() {
                actions.push(ToolbarAction::Lifecycle(LifecycleAction::Start));
            }
            if let ScreenDrawState::Failed { message, .. } = state {
                ui.colored_label(egui::Color32::LIGHT_RED, message);
            }
            idle_preferences(ui, settings, actions);
            ui.separator();
            if ui.button("Close Toolbar").clicked() {
                actions.push(ToolbarAction::Lifecycle(LifecycleAction::Close));
            }
        }
        ScreenDrawState::Drawing { .. } => {
            drawing_controls(ui, runtime, settings, actions);
            if ui.button("Ghost").clicked() {
                actions.push(ToolbarAction::Ghost);
            }
            if ui.button("Done").clicked() {
                actions.push(ToolbarAction::Finish);
            }
            session_controls(ui, actions);
        }
        ScreenDrawState::Ghost { .. } => {
            if ui.button("Resume Drawing").clicked() {
                actions.push(ToolbarAction::Lifecycle(LifecycleAction::Resume));
            }
            history_controls(ui, actions);
            visibility_controls(ui, runtime, actions);
            background_controls(ui, runtime, settings, actions);
            if ui.button("Clear").clicked() {
                actions.push(ToolbarAction::Clear);
            }
            if ui.button("Done").clicked() {
                actions.push(ToolbarAction::Finish);
            }
            session_controls(ui, actions);
        }
        ScreenDrawState::Finish { .. } => {
            if ui.button("Resume Drawing").clicked() {
                actions.push(ToolbarAction::Lifecycle(LifecycleAction::Resume));
            }
            visibility_controls(ui, runtime, actions);
            export_controls(
                ui,
                "Export",
                settings,
                export_error,
                export_in_flight,
                export_background,
                actions,
            );
            session_controls(ui, actions);
        }
        ScreenDrawState::DisplayChanged { .. } => {
            ui.colored_label(
                egui::Color32::YELLOW,
                "Display layout changed. The session is safely paused.",
            );
            ui.small("Drawing cannot resume, but the original capture and annotations remain exportable.");
            export_controls(
                ui,
                "Export Original Capture",
                settings,
                export_error,
                export_in_flight,
                export_background,
                actions,
            );
            session_controls(ui, actions);
        }
        ScreenDrawState::AwaitingLauncherParking { .. } => {
            ui.spinner();
            ui.label("Hiding launcher…");
            session_controls(ui, actions);
        }
        ScreenDrawState::AwaitingNativeTeardown { .. } => {
            ui.spinner();
            ui.label("Closing previous drawing surface…");
            session_controls(ui, actions);
        }
        ScreenDrawState::Capturing { .. } => {
            ui.spinner();
            ui.label("Capturing desktop…");
            session_controls(ui, actions);
        }
        ScreenDrawState::SelectingRegion { .. } => {
            ui.label("Select an export region on the desktop.");
            session_controls(ui, actions);
        }
    }
}

fn export_controls(
    ui: &mut egui::Ui,
    heading: &str,
    settings: &ScreenDrawSettings,
    export_error: Option<&str>,
    export_in_flight: bool,
    export_background: &mut ExportBackground,
    actions: &mut Vec<ToolbarAction>,
) {
    ui.separator();
    ui.label(heading);
    ui.add_enabled_ui(!export_in_flight, |ui| {
        egui::ComboBox::from_id_source("screen_draw_export_background")
            .selected_text(export_background_label(*export_background))
            .show_ui(ui, |ui| {
                for background in [
                    ExportBackground::FrozenDesktop,
                    ExportBackground::Transparent,
                    ExportBackground::White,
                    ExportBackground::Black,
                    ExportBackground::Solid(settings.custom_background),
                ] {
                    ui.selectable_value(
                        export_background,
                        background,
                        export_background_label(background),
                    );
                }
            });
        if ui.button("Copy full image").clicked() {
            actions.push(ToolbarAction::Export(
                ExportDestination::Clipboard,
                *export_background,
            ));
        }
        if ui.button("Save full image").clicked() {
            actions.push(ToolbarAction::Export(
                ExportDestination::File,
                *export_background,
            ));
        }
        if ui.button("Open full image in Screenshot Editor").clicked() {
            actions.push(ToolbarAction::Export(
                ExportDestination::ScreenshotEditor,
                *export_background,
            ));
        }
        ui.separator();
        ui.label("Region");
        if ui.button("Copy region").clicked() {
            actions.push(ToolbarAction::RegionExport(
                ExportDestination::Clipboard,
                *export_background,
            ));
        }
        if ui.button("Save region").clicked() {
            actions.push(ToolbarAction::RegionExport(
                ExportDestination::File,
                *export_background,
            ));
        }
        if ui.button("Open region in Screenshot Editor").clicked() {
            actions.push(ToolbarAction::RegionExport(
                ExportDestination::ScreenshotEditor,
                *export_background,
            ));
        }
    });
    if export_in_flight {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Exporting…");
        });
    }
    if let Some(error) = export_error {
        ui.colored_label(egui::Color32::LIGHT_RED, error);
        ui.small("The session is preserved; choose an export action to retry.");
    }
}

fn export_background_label(background: ExportBackground) -> &'static str {
    match background {
        ExportBackground::FrozenDesktop => "Frozen desktop",
        ExportBackground::Transparent => "Transparent",
        ExportBackground::White => "White",
        ExportBackground::Black => "Black",
        ExportBackground::Solid(_) => "Custom color",
    }
}

fn idle_preferences(
    ui: &mut egui::Ui,
    settings: &ScreenDrawSettings,
    actions: &mut Vec<ToolbarAction>,
) {
    ui.collapsing("Defaults", |ui| {
        egui::ComboBox::from_label("Tool")
            .selected_text(tool_label(settings.default_tool))
            .show_ui(ui, |ui| {
                for tool in ALL_TOOLS {
                    if ui
                        .selectable_label(settings.default_tool == tool, tool_label(tool))
                        .clicked()
                    {
                        actions.push(ToolbarAction::SetTool(tool));
                    }
                }
            });
        let mut thickness = settings.default_thickness;
        if ui
            .add(egui::Slider::new(&mut thickness, 0.5..=64.0).text("Thickness"))
            .changed()
        {
            actions.push(ToolbarAction::SetThickness(thickness));
        }
        palette_grid(ui, &settings.palette[..9], None, settings, actions);
        let mut color = rgba_to_egui(settings.default_color);
        if ui.color_edit_button_srgba(&mut color).changed() {
            actions.push(ToolbarAction::SetColor(egui_to_rgba(color)));
        }
        egui::ComboBox::from_label("Background")
            .selected_text(background_label(settings.default_background))
            .show_ui(ui, |ui| {
                for (label, background) in [
                    ("Frozen", CanvasBackground::FrozenDesktop),
                    ("Whiteboard", CanvasBackground::White),
                    ("Blackboard", CanvasBackground::Black),
                    (
                        "Custom",
                        CanvasBackground::Solid(settings.custom_background),
                    ),
                ] {
                    if ui
                        .selectable_label(settings.default_background == background, label)
                        .clicked()
                    {
                        actions.push(ToolbarAction::SetBackground(background));
                    }
                }
            });
    });
}

fn drawing_controls(
    ui: &mut egui::Ui,
    runtime: Option<crate::screen_draw::NativeRuntimeState>,
    settings: &ScreenDrawSettings,
    actions: &mut Vec<ToolbarAction>,
) {
    let selected_tool = runtime.map_or(settings.default_tool, |state| state.tool);
    egui::Grid::new("screen_draw_tools")
        .num_columns(2)
        .show(ui, |ui| {
            for (index, tool) in ALL_TOOLS.iter().copied().enumerate() {
                if ui
                    .selectable_label(selected_tool == tool, tool_label(tool))
                    .clicked()
                {
                    actions.push(ToolbarAction::SetTool(tool));
                }
                if index % 2 == 1 {
                    ui.end_row();
                }
            }
        });

    let mut thickness = runtime.map_or(settings.default_thickness, |state| state.thickness);
    if ui
        .add(egui::Slider::new(&mut thickness, 0.5..=64.0).text("Thickness"))
        .changed()
    {
        actions.push(ToolbarAction::SetThickness(thickness));
    }

    ui.label("Colors");
    palette_grid(ui, &settings.palette[..9], runtime, settings, actions);
    ui.collapsing("Expanded palette", |ui| {
        ui.small("Click a swatch to edit that persisted slot.");
        editable_palette_grid(ui, &settings.palette, actions);
    });
    let selected = runtime.map_or(settings.default_color, |state| state.color);
    let mut custom = rgba_to_egui(selected);
    if ui.color_edit_button_srgba(&mut custom).changed() {
        actions.push(ToolbarAction::SetColor(egui_to_rgba(custom)));
    }

    history_controls(ui, actions);
    visibility_controls(ui, runtime, actions);
    background_controls(ui, runtime, settings, actions);
    if ui.button("Clear").clicked() {
        actions.push(ToolbarAction::Clear);
    }
}

fn history_controls(ui: &mut egui::Ui, actions: &mut Vec<ToolbarAction>) {
    ui.horizontal(|ui| {
        if ui.button("Undo").clicked() {
            actions.push(ToolbarAction::Undo);
        }
        if ui.button("Redo").clicked() {
            actions.push(ToolbarAction::Redo);
        }
    });
}

fn background_controls(
    ui: &mut egui::Ui,
    runtime: Option<crate::screen_draw::NativeRuntimeState>,
    settings: &ScreenDrawSettings,
    actions: &mut Vec<ToolbarAction>,
) {
    ui.label("Background");
    let selected_background = runtime.map_or(settings.default_background, |state| state.background);
    for (label, background) in [
        ("Frozen", CanvasBackground::FrozenDesktop),
        ("Whiteboard", CanvasBackground::White),
        ("Blackboard", CanvasBackground::Black),
    ] {
        if ui
            .selectable_label(selected_background == background, label)
            .clicked()
        {
            actions.push(ToolbarAction::SetBackground(background));
        }
    }
    let mut custom_background = rgba_to_egui(settings.custom_background);
    ui.horizontal(|ui| {
        ui.label("Custom");
        if ui.color_edit_button_srgba(&mut custom_background).changed() {
            actions.push(ToolbarAction::SetBackground(CanvasBackground::Solid(
                egui_to_rgba(custom_background),
            )));
        }
    });
}

fn editable_palette_grid(
    ui: &mut egui::Ui,
    palette: &[RgbaColor],
    actions: &mut Vec<ToolbarAction>,
) {
    egui::Grid::new("screen_draw_editable_palette")
        .num_columns(6)
        .show(ui, |ui| {
            for (index, color) in palette.iter().copied().enumerate() {
                let mut edited = rgba_to_egui(color);
                if ui.color_edit_button_srgba(&mut edited).changed() {
                    actions.push(ToolbarAction::SetPaletteColor(index, egui_to_rgba(edited)));
                }
                if index % 6 == 5 {
                    ui.end_row();
                }
            }
        });
}

fn palette_grid(
    ui: &mut egui::Ui,
    palette: &[RgbaColor],
    runtime: Option<crate::screen_draw::NativeRuntimeState>,
    settings: &ScreenDrawSettings,
    actions: &mut Vec<ToolbarAction>,
) {
    let selected = runtime.map_or(settings.default_color, |state| state.color);
    egui::Grid::new(("screen_draw_palette", palette.len()))
        .num_columns(6)
        .show(ui, |ui| {
            for (index, color) in palette.iter().copied().enumerate() {
                let button = egui::Button::new("  ").fill(rgba_to_egui(color)).stroke(
                    if selected == color {
                        egui::Stroke::new(2.0_f32, egui::Color32::WHITE)
                    } else {
                        egui::Stroke::NONE
                    },
                );
                if ui
                    .add(button)
                    .on_hover_text(format!("Color {}", index + 1))
                    .clicked()
                {
                    actions.push(ToolbarAction::SetColor(color));
                }
                if index % 6 == 5 {
                    ui.end_row();
                }
            }
        });
}

fn visibility_controls(
    ui: &mut egui::Ui,
    runtime: Option<crate::screen_draw::NativeRuntimeState>,
    actions: &mut Vec<ToolbarAction>,
) {
    let mut visible = runtime.is_none_or(|state| state.annotations_visible);
    if ui
        .checkbox(&mut visible, "Eye (show annotations)")
        .changed()
    {
        actions.push(ToolbarAction::SetAnnotationsVisible(visible));
    }
}

fn session_controls(ui: &mut egui::Ui, actions: &mut Vec<ToolbarAction>) {
    ui.separator();
    if ui.button("Discard / New Capture").clicked() {
        actions.push(ToolbarAction::Lifecycle(LifecycleAction::NewCapture));
    }
    if ui.button("Close Screen Draw").clicked() {
        actions.push(ToolbarAction::Lifecycle(LifecycleAction::Close));
    }
}

fn state_label(state: &ScreenDrawState) -> &'static str {
    match state {
        ScreenDrawState::NoSession => "Idle",
        ScreenDrawState::AwaitingLauncherParking { .. } => "Preparing capture",
        ScreenDrawState::AwaitingNativeTeardown { .. } => "Closing previous capture",
        ScreenDrawState::Capturing { .. } => "Capturing",
        ScreenDrawState::Drawing { .. } => "Drawing",
        ScreenDrawState::Ghost { .. } => "Ghost",
        ScreenDrawState::Finish { .. } => "Finish",
        ScreenDrawState::SelectingRegion { .. } => "Selecting region",
        ScreenDrawState::DisplayChanged { .. } => "Display changed",
        ScreenDrawState::Failed { .. } => "Capture failed",
    }
}

fn tool_label(tool: ScreenDrawTool) -> &'static str {
    match tool {
        ScreenDrawTool::Pen => "Pen",
        ScreenDrawTool::Highlighter => "Highlighter",
        ScreenDrawTool::StraightLine => "Line",
        ScreenDrawTool::Arrow => "Arrow",
        ScreenDrawTool::Rectangle => "Rectangle",
        ScreenDrawTool::Ellipse => "Ellipse",
        ScreenDrawTool::Text => "Text",
        ScreenDrawTool::Eraser => "Eraser",
        ScreenDrawTool::FadingInk => "Fading Ink",
        ScreenDrawTool::Eyedropper => "Eyedropper",
    }
}

fn background_label(background: CanvasBackground) -> &'static str {
    match background {
        CanvasBackground::FrozenDesktop => "Frozen",
        CanvasBackground::White => "Whiteboard",
        CanvasBackground::Black => "Blackboard",
        CanvasBackground::Solid(_) => "Custom",
    }
}

const ALL_TOOLS: [ScreenDrawTool; 10] = [
    ScreenDrawTool::Pen,
    ScreenDrawTool::Highlighter,
    ScreenDrawTool::StraightLine,
    ScreenDrawTool::Arrow,
    ScreenDrawTool::Rectangle,
    ScreenDrawTool::Ellipse,
    ScreenDrawTool::Text,
    ScreenDrawTool::Eraser,
    ScreenDrawTool::FadingInk,
    ScreenDrawTool::Eyedropper,
];

fn toolbar_size_points(orientation: ToolbarOrientation) -> egui::Vec2 {
    match orientation {
        ToolbarOrientation::Vertical => VERTICAL_TOOLBAR_SIZE_POINTS,
        ToolbarOrientation::Horizontal => HORIZONTAL_TOOLBAR_SIZE_POINTS,
    }
}

fn rgba_to_egui(color: RgbaColor) -> egui::Color32 {
    let [r, g, b, a] = color.channels();
    egui::Color32::from_rgba_unmultiplied(r, g, b, a)
}

fn egui_to_rgba(color: egui::Color32) -> RgbaColor {
    RgbaColor::rgba(color.r(), color.g(), color.b(), color.a())
}

fn logical_to_physical(position: egui::Pos2, native_pixels_per_point: f32) -> DesktopPoint {
    let scale = native_pixels_per_point.max(0.25);
    DesktopPoint::new(
        (position.x * scale).round() as i32,
        (position.y * scale).round() as i32,
    )
}

fn physical_to_logical(position: DesktopPoint, native_pixels_per_point: f32) -> egui::Pos2 {
    let scale = native_pixels_per_point.max(0.25);
    egui::pos2(position.x as f32 / scale, position.y as f32 / scale)
}

fn logical_size_to_physical(size: egui::Vec2, native_pixels_per_point: f32) -> DesktopSize {
    let scale = native_pixels_per_point.max(0.25);
    DesktopSize::new(
        (size.x * scale).ceil() as u32,
        (size.y * scale).ceil() as u32,
    )
}

fn recover_toolbar_position(
    requested: DesktopPoint,
    toolbar_size: DesktopSize,
    monitors: &[DesktopRect],
) -> DesktopPoint {
    let requested_rect = DesktopRect::new(
        requested.x,
        requested.y,
        toolbar_size.width,
        toolbar_size.height,
    );
    if monitors
        .iter()
        .any(|monitor| requested_rect.intersection(*monitor).is_some())
    {
        requested
    } else {
        clamp_toolbar_position(requested, toolbar_size, monitors).unwrap_or(requested)
    }
}

fn update_screen_draw_settings(path: &str, screen_draw: &ScreenDrawSettings) -> anyhow::Result<()> {
    crate::settings::Settings::update(path, |settings| {
        settings
            .plugin_settings
            .insert(SETTINGS_KEY.into(), serde_json::to_value(screen_draw)?);
        Ok(())
    })?;
    Ok(())
}

fn current_monitor_rects() -> Vec<DesktopRect> {
    crate::mkmacro::screen::monitor_descriptors()
        .map(|monitors| {
            monitors
                .into_iter()
                .map(|monitor| monitor.bounds.into())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen_draw::ScreenDrawGeneration;

    fn key_press(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn key_release(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers,
        }
    }

    fn raw_input(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            focused: true,
            events,
            ..Default::default()
        }
    }

    fn ctrl() -> egui::Modifiers {
        egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        }
    }

    fn ctrl_shift() -> egui::Modifiers {
        egui::Modifiers {
            ctrl: true,
            shift: true,
            command: true,
            ..Default::default()
        }
    }

    #[test]
    fn toolbar_viewport_policy_is_fixed_topmost_and_absent_from_taskbar() {
        let size = egui::vec2(700.0, 264.0);
        let position = egui::pos2(-500.0, 25.0);
        let builder = toolbar_viewport_builder(size, Some(position));
        assert_eq!(builder.title.as_deref(), Some(TOOLBAR_WINDOW_TITLE));
        assert_eq!(builder.position, Some(position));
        assert_eq!(builder.inner_size, Some(size));
        assert_eq!(builder.min_inner_size, Some(size));
        assert_eq!(builder.max_inner_size, Some(size));
        assert_eq!(builder.resizable, Some(false));
        assert_eq!(builder.taskbar, Some(false));
        assert_eq!(builder.minimize_button, Some(false));
        assert_eq!(builder.maximize_button, Some(false));
        assert_eq!(builder.window_level, Some(egui::WindowLevel::AlwaysOnTop));
    }

    #[test]
    fn focused_toolbar_escape_is_consumed_and_requests_safe_pause_only_while_drawing() {
        let generation = ScreenDrawGeneration::from_raw(1);
        for (state, expected) in [
            (ScreenDrawState::Drawing { generation }, true),
            (ScreenDrawState::Ghost { generation }, false),
            (ScreenDrawState::Finish { generation }, false),
        ] {
            let ctx = egui::Context::default();
            ctx.begin_frame(egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            });
            assert_eq!(consume_drawing_escape(&ctx, &state), expected);
            if expected {
                assert!(!ctx.input_mut(|input| {
                    input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                }));
            }
            let _ = ctx.end_frame();
        }
    }

    #[test]
    fn toolbar_drawing_shortcuts_route_every_local_action_once() {
        let state = ScreenDrawState::Drawing {
            generation: ScreenDrawGeneration::from_raw(1),
        };
        let settings = ScreenDrawSettings::default();
        let ctx = egui::Context::default();
        ctx.begin_frame(raw_input(vec![
            key_press(egui::Key::P, egui::Modifiers::NONE),
            key_press(egui::Key::Z, ctrl()),
            key_release(egui::Key::Z, ctrl()),
            key_press(egui::Key::Y, ctrl()),
            key_press(egui::Key::Z, ctrl_shift()),
            key_press(egui::Key::OpenBracket, egui::Modifiers::NONE),
            key_press(egui::Key::CloseBracket, egui::Modifiers::NONE),
            key_press(egui::Key::Num1, egui::Modifiers::NONE),
        ]));

        let resolved = consume_toolbar_local_shortcuts(&ctx, &state, &settings);
        assert_eq!(
            resolved,
            vec![
                LocalShortcutAction::Tool(ScreenDrawTool::Pen),
                LocalShortcutAction::Undo,
                LocalShortcutAction::Redo,
                LocalShortcutAction::Redo,
                LocalShortcutAction::DecreaseThickness,
                LocalShortcutAction::IncreaseThickness,
                LocalShortcutAction::Color(settings.palette[0]),
            ]
        );
        assert_eq!(
            toolbar_actions_for_local_shortcuts(resolved, 3.0),
            vec![
                ToolbarAction::SetTool(ScreenDrawTool::Pen),
                ToolbarAction::Undo,
                ToolbarAction::Redo,
                ToolbarAction::Redo,
                ToolbarAction::SetThickness(2.0),
                ToolbarAction::SetThickness(3.0),
                ToolbarAction::SetColor(settings.palette[0]),
            ]
        );
        assert_eq!(ctx.input(|input| input.events.len()), 1);
        assert!(
            ctx.input(|input| matches!(input.events[0], egui::Event::Key { pressed: false, .. }))
        );
        let _ = ctx.end_frame();
    }

    #[test]
    fn configured_toolbar_shortcut_and_native_adapter_have_resolver_parity() {
        let state = ScreenDrawState::Drawing {
            generation: ScreenDrawGeneration::from_raw(1),
        };
        let mut settings = ScreenDrawSettings::default();
        settings.tool_hotkeys.insert(
            ScreenDrawTool::Rectangle,
            crate::screen_draw::HotkeyChord::from_unchecked("Ctrl+F12"),
        );
        let egui_input = egui_local_shortcut_input(
            egui_key_to_local_shortcut(egui::Key::F12).unwrap(),
            ctrl(),
            false,
            false,
        );
        let native_input = LocalShortcutInput::from_native(0x7B, 0x0002, false).unwrap();
        assert_eq!(
            resolve_local_shortcut(&settings, egui_input),
            resolve_local_shortcut(&settings, native_input)
        );

        let ctx = egui::Context::default();
        ctx.begin_frame(raw_input(vec![key_press(egui::Key::F12, ctrl())]));
        assert_eq!(
            consume_toolbar_local_shortcuts(&ctx, &state, &settings),
            vec![LocalShortcutAction::Tool(ScreenDrawTool::Rectangle)]
        );
        let _ = ctx.end_frame();
    }

    #[test]
    fn toolbar_shortcuts_leave_unrelated_and_repeated_events_unconsumed() {
        let state = ScreenDrawState::Drawing {
            generation: ScreenDrawGeneration::from_raw(1),
        };
        let settings = ScreenDrawSettings::default();
        let ctx = egui::Context::default();
        ctx.begin_frame(raw_input(vec![key_press(
            egui::Key::P,
            egui::Modifiers::NONE,
        )]));
        assert_eq!(
            consume_toolbar_local_shortcuts(&ctx, &state, &settings),
            vec![LocalShortcutAction::Tool(ScreenDrawTool::Pen)]
        );
        let _ = ctx.end_frame();

        let mut repeated = key_press(egui::Key::P, egui::Modifiers::NONE);
        if let egui::Event::Key { repeat, .. } = &mut repeated {
            *repeat = true;
        }
        ctx.begin_frame(raw_input(vec![
            repeated,
            key_press(egui::Key::F24, egui::Modifiers::NONE),
        ]));
        assert!(consume_toolbar_local_shortcuts(&ctx, &state, &settings).is_empty());
        assert_eq!(ctx.input(|input| input.events.len()), 2);
        let _ = ctx.end_frame();
    }

    #[test]
    fn idle_unsupported_and_repeat_only_input_do_not_query_win_modifier_state() {
        use std::cell::Cell;

        let state = ScreenDrawState::Drawing {
            generation: ScreenDrawGeneration::from_raw(1),
        };
        let settings = ScreenDrawSettings::default();
        let reads = Cell::new(0);
        let mut read_win = || {
            reads.set(reads.get() + 1);
            false
        };

        let empty = egui::Context::default();
        empty.begin_frame(raw_input(Vec::new()));
        assert!(
            consume_toolbar_local_shortcuts_with_win_modifier(
                &empty,
                &state,
                &settings,
                &mut read_win,
            )
            .is_empty()
        );
        assert_eq!(reads.get(), 0);
        let _ = empty.end_frame();

        let unsupported = egui::Context::default();
        unsupported.begin_frame(raw_input(vec![key_press(
            egui::Key::Comma,
            egui::Modifiers::NONE,
        )]));
        assert!(
            consume_toolbar_local_shortcuts_with_win_modifier(
                &unsupported,
                &state,
                &settings,
                &mut read_win,
            )
            .is_empty()
        );
        assert_eq!(reads.get(), 0);
        let _ = unsupported.end_frame();

        let repeated = egui::Context::default();
        repeated.begin_frame(raw_input(vec![key_press(
            egui::Key::P,
            egui::Modifiers::NONE,
        )]));
        assert_eq!(
            consume_toolbar_local_shortcuts(&repeated, &state, &settings),
            vec![LocalShortcutAction::Tool(ScreenDrawTool::Pen)]
        );
        let _ = repeated.end_frame();
        let mut repeated_press = key_press(egui::Key::P, egui::Modifiers::NONE);
        if let egui::Event::Key { repeat, .. } = &mut repeated_press {
            *repeat = true;
        }
        repeated.begin_frame(raw_input(vec![repeated_press]));
        assert!(
            consume_toolbar_local_shortcuts_with_win_modifier(
                &repeated,
                &state,
                &settings,
                &mut read_win,
            )
            .is_empty()
        );
        assert_eq!(reads.get(), 0);
        let _ = repeated.end_frame();
    }

    #[test]
    fn batched_thickness_shortcuts_evolve_from_each_previous_action() {
        assert_eq!(
            toolbar_actions_for_local_shortcuts(
                vec![
                    LocalShortcutAction::DecreaseThickness,
                    LocalShortcutAction::IncreaseThickness,
                ],
                3.0,
            ),
            vec![
                ToolbarAction::SetThickness(2.0),
                ToolbarAction::SetThickness(3.0),
            ]
        );
        assert_eq!(
            toolbar_actions_for_local_shortcuts(
                vec![
                    LocalShortcutAction::IncreaseThickness,
                    LocalShortcutAction::IncreaseThickness,
                ],
                3.0,
            ),
            vec![
                ToolbarAction::SetThickness(4.0),
                ToolbarAction::SetThickness(5.0),
            ]
        );
    }

    #[test]
    fn toolbar_shortcuts_are_ignored_outside_drawing_or_without_viewport_focus() {
        let generation = ScreenDrawGeneration::from_raw(1);
        let settings = ScreenDrawSettings::default();
        for (state, focused) in [
            (ScreenDrawState::Ghost { generation }, true),
            (ScreenDrawState::Finish { generation }, true),
            (ScreenDrawState::Drawing { generation }, false),
        ] {
            let ctx = egui::Context::default();
            ctx.begin_frame(egui::RawInput {
                focused,
                events: vec![key_press(egui::Key::P, egui::Modifiers::NONE)],
                ..Default::default()
            });
            assert!(consume_toolbar_local_shortcuts(&ctx, &state, &settings).is_empty());
            assert_eq!(ctx.input(|input| input.events.len()), 1);
            let _ = ctx.end_frame();
        }
    }

    #[test]
    fn real_text_edit_focus_suppresses_toolbar_shortcuts_but_button_focus_does_not() {
        let state = ScreenDrawState::Drawing {
            generation: ScreenDrawGeneration::from_raw(1),
        };
        let settings = ScreenDrawSettings::default();

        let text_ctx = egui::Context::default();
        text_ctx.begin_frame(raw_input(Vec::new()));
        let mut text = String::new();
        egui::CentralPanel::default().show(&text_ctx, |ui| {
            ui.text_edit_singleline(&mut text).request_focus();
        });
        let _ = text_ctx.end_frame();
        text_ctx.begin_frame(raw_input(vec![key_press(
            egui::Key::P,
            egui::Modifiers::NONE,
        )]));
        assert!(toolbar_text_editing_owns_keyboard(&text_ctx));
        assert!(consume_toolbar_local_shortcuts(&text_ctx, &state, &settings).is_empty());
        assert_eq!(text_ctx.input(|input| input.events.len()), 1);
        let _ = text_ctx.end_frame();

        let button_ctx = egui::Context::default();
        button_ctx.begin_frame(raw_input(Vec::new()));
        egui::CentralPanel::default().show(&button_ctx, |ui| {
            ui.button("Focused button").request_focus();
        });
        let _ = button_ctx.end_frame();
        button_ctx.begin_frame(raw_input(vec![key_press(
            egui::Key::P,
            egui::Modifiers::NONE,
        )]));
        assert!(button_ctx.wants_keyboard_input());
        assert!(!toolbar_text_editing_owns_keyboard(&button_ctx));
        assert_eq!(
            consume_toolbar_local_shortcuts(&button_ctx, &state, &settings),
            vec![LocalShortcutAction::Tool(ScreenDrawTool::Pen)]
        );
        let _ = button_ctx.end_frame();
    }

    #[test]
    fn space_and_enter_shortcuts_are_consumed_before_a_focused_button_can_activate() {
        let state = ScreenDrawState::Drawing {
            generation: ScreenDrawGeneration::from_raw(1),
        };
        for (key, chord) in [(egui::Key::Space, "Space"), (egui::Key::Enter, "Enter")] {
            let mut settings = ScreenDrawSettings::default();
            settings.tool_hotkeys.insert(
                ScreenDrawTool::Pen,
                crate::screen_draw::HotkeyChord::from_unchecked(chord),
            );
            let ctx = egui::Context::default();
            ctx.begin_frame(raw_input(Vec::new()));
            egui::CentralPanel::default().show(&ctx, |ui| {
                ui.button("Focused button").request_focus();
            });
            let _ = ctx.end_frame();

            ctx.begin_frame(raw_input(vec![key_press(key, egui::Modifiers::NONE)]));
            assert_eq!(
                consume_toolbar_local_shortcuts(&ctx, &state, &settings),
                vec![LocalShortcutAction::Tool(ScreenDrawTool::Pen)]
            );
            let mut clicked = false;
            egui::CentralPanel::default().show(&ctx, |ui| {
                clicked = ui.button("Focused button").clicked();
            });
            assert!(!clicked, "{chord} activated the focused button");
            let _ = ctx.end_frame();
        }
    }

    #[test]
    fn egui_history_adapter_rejects_extra_modifiers() {
        let settings = ScreenDrawSettings::default();
        let key = egui_key_to_local_shortcut(egui::Key::Z).unwrap();
        for modifiers in [
            egui::Modifiers {
                ctrl: true,
                alt: true,
                ..Default::default()
            },
            egui::Modifiers {
                ctrl: true,
                shift: true,
                alt: true,
                ..Default::default()
            },
        ] {
            assert_eq!(
                resolve_local_shortcut(
                    &settings,
                    egui_local_shortcut_input(key, modifiers, false, false)
                ),
                None
            );
        }
        assert_eq!(
            resolve_local_shortcut(
                &settings,
                egui_local_shortcut_input(key, ctrl_shift(), false, false)
            ),
            Some(LocalShortcutAction::Redo)
        );
    }

    #[test]
    fn escape_remains_owned_by_the_drawing_lifecycle_path() {
        let state = ScreenDrawState::Drawing {
            generation: ScreenDrawGeneration::from_raw(1),
        };
        let settings = ScreenDrawSettings::default();
        let ctx = egui::Context::default();
        ctx.begin_frame(raw_input(vec![key_press(
            egui::Key::Escape,
            egui::Modifiers::NONE,
        )]));
        assert!(consume_toolbar_local_shortcuts(&ctx, &state, &settings).is_empty());
        assert!(consume_drawing_escape(&ctx, &state));
        assert!(ctx.input(|input| input.events.is_empty()));
        let _ = ctx.end_frame();
    }

    #[test]
    fn state_controls_map_to_the_expected_session_capabilities() {
        let generation = ScreenDrawGeneration::from_raw(1);
        assert_eq!(
            controls_for_state(&ScreenDrawState::NoSession),
            &[ToolbarControl::Start]
        );
        let drawing = controls_for_state(&ScreenDrawState::Drawing { generation });
        assert!(drawing.contains(&ToolbarControl::DrawingTools));
        assert!(drawing.contains(&ToolbarControl::Ghost));
        assert!(drawing.contains(&ToolbarControl::Finish));
        let ghost = controls_for_state(&ScreenDrawState::Ghost { generation });
        assert!(ghost.contains(&ToolbarControl::Resume));
        assert!(ghost.contains(&ToolbarControl::UndoRedo));
        assert!(ghost.contains(&ToolbarControl::Background));
        assert!(ghost.contains(&ToolbarControl::Clear));
        assert!(!ghost.contains(&ToolbarControl::DrawingTools));
        let finish = controls_for_state(&ScreenDrawState::Finish { generation });
        assert!(finish.contains(&ToolbarControl::Resume));
        assert!(finish.contains(&ToolbarControl::Export));
        assert!(!finish.contains(&ToolbarControl::Ghost));
        let display_changed = controls_for_state(&ScreenDrawState::DisplayChanged { generation });
        assert!(display_changed.contains(&ToolbarControl::Export));
        assert!(!display_changed.contains(&ToolbarControl::Resume));
    }

    #[test]
    fn physical_logical_toolbar_coordinates_are_explicit_and_signed() {
        let physical = DesktopPoint::new(-1800, 150);
        let logical = physical_to_logical(physical, 1.5);
        assert_eq!(logical, egui::pos2(-1200.0, 100.0));
        assert_eq!(logical_to_physical(logical, 1.5), physical);
        assert_eq!(
            logical_size_to_physical(egui::vec2(200.0, 300.0), 1.5),
            DesktopSize::new(300, 450)
        );
    }

    #[test]
    fn orientation_switch_changes_geometry_and_is_a_persisted_preference() {
        assert_eq!(
            toolbar_size_points(ToolbarOrientation::Vertical),
            egui::vec2(264.0, 700.0)
        );
        assert_eq!(
            toolbar_size_points(ToolbarOrientation::Horizontal),
            egui::vec2(700.0, 264.0)
        );
        let mut controller = ScreenDrawController::default();
        let effect =
            apply_controller_toolbar_action(&mut controller, ToolbarAction::ToggleOrientation)
                .unwrap();
        assert!(effect.preferences_changed);
        assert_eq!(
            controller.settings().toolbar_orientation,
            ToolbarOrientation::Horizontal
        );
    }

    #[test]
    fn position_recovery_covers_visible_disconnected_negative_oversized_and_empty() {
        let monitors = [
            DesktopRect::new(-1920, 0, 1920, 1080),
            DesktopRect::new(0, 0, 2560, 1440),
        ];
        assert_eq!(
            recover_toolbar_position(
                DesktopPoint::new(-1800, 100),
                DesktopSize::new(250, 600),
                &monitors
            ),
            DesktopPoint::new(-1800, 100)
        );
        assert_eq!(
            recover_toolbar_position(
                DesktopPoint::new(-2000, 100),
                DesktopSize::new(250, 600),
                &monitors
            ),
            DesktopPoint::new(-2000, 100),
        );
        assert_eq!(
            recover_toolbar_position(
                DesktopPoint::new(5000, 5000),
                DesktopSize::new(250, 600),
                &monitors
            ),
            DesktopPoint::new(2310, 840)
        );
        assert_eq!(
            recover_toolbar_position(
                DesktopPoint::new(-100, 500),
                DesktopSize::new(3000, 2000),
                &monitors
            ),
            DesktopPoint::new(-100, 500)
        );
        assert_eq!(
            recover_toolbar_position(DesktopPoint::new(3, 4), DesktopSize::new(10, 10), &[]),
            DesktopPoint::new(3, 4)
        );
        assert_eq!(
            clamp_toolbar_position(
                DesktopPoint::new(5000, 5000),
                DesktopSize::new(3000, 2000),
                &monitors
            ),
            Some(DesktopPoint::new(0, 0))
        );
    }

    #[test]
    fn position_and_preferences_share_one_debounced_dirty_boundary() {
        let mut ui = ScreenDrawToolbarUi::default();
        let mut settings = ScreenDrawSettings::default();
        ui.note_position(DesktopPoint::new(-20, 40), &mut settings);
        let changed = ui.settings_dirty_since.unwrap();
        assert!(!ui.persistence_due(changed + PERSIST_DEBOUNCE - Duration::from_millis(1)));
        assert!(ui.persistence_due(changed + PERSIST_DEBOUNCE));
        assert_eq!(settings.toolbar_position, Some(DesktopPoint::new(-20, 40)));
        ui.clear_dirty();
        assert!(!ui.is_dirty());
    }

    #[test]
    fn toolbar_opens_only_when_controller_marks_capture_complete() {
        let mut controller = ScreenDrawController::default();
        let generation = controller.request_start().unwrap();
        assert!(!controller.toolbar_open());
        controller.launcher_parked(generation).unwrap();
        assert!(!controller.toolbar_open());
        controller.capture_succeeded(generation).unwrap();
        assert!(controller.toolbar_open());
    }

    #[test]
    fn idle_preference_actions_update_defaults_without_starting_a_session() {
        let mut controller = ScreenDrawController::default();
        for action in [
            ToolbarAction::SetTool(ScreenDrawTool::Ellipse),
            ToolbarAction::SetColor(RgbaColor::rgba(3, 4, 5, 255)),
            ToolbarAction::SetThickness(9.0),
            ToolbarAction::SetPaletteColor(23, RgbaColor::rgba(6, 7, 8, 255)),
            ToolbarAction::SetBackground(CanvasBackground::Black),
        ] {
            assert!(
                apply_controller_toolbar_action(&mut controller, action)
                    .unwrap()
                    .preferences_changed
            );
        }
        assert_eq!(controller.state(), &ScreenDrawState::NoSession);
        let settings = controller.settings();
        assert_eq!(settings.default_tool, ScreenDrawTool::Ellipse);
        assert_eq!(settings.default_color, RgbaColor::rgba(6, 7, 8, 255));
        assert_eq!(settings.default_thickness, 9.0);
        assert_eq!(settings.palette[23], RgbaColor::rgba(6, 7, 8, 255));
        assert_eq!(settings.default_background, CanvasBackground::Black);
    }

    #[test]
    fn screen_draw_settings_round_trip_as_one_plugin_owned_value() {
        let mut expected = ScreenDrawSettings {
            default_tool: ScreenDrawTool::Arrow,
            default_color: RgbaColor::rgba(1, 2, 3, 255),
            default_thickness: 7.5,
            default_background: CanvasBackground::Solid(RgbaColor::rgba(8, 9, 10, 255)),
            custom_background: RgbaColor::rgba(8, 9, 10, 255),
            toolbar_position: Some(DesktopPoint::new(-900, 30)),
            ..Default::default()
        };
        expected.normalize();
        let mut settings = crate::settings::Settings::default();
        settings.plugin_settings.insert(
            SETTINGS_KEY.into(),
            serde_json::to_value(&expected).unwrap(),
        );
        let restored: ScreenDrawSettings =
            serde_json::from_value(settings.plugin_settings.get(SETTINGS_KEY).unwrap().clone())
                .unwrap();
        assert_eq!(restored, expected);
    }

    #[test]
    fn settings_update_owns_only_the_screen_draw_plugin_entry() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let mut initial = crate::settings::Settings::default();
        initial
            .plugin_settings
            .insert("unrelated".into(), serde_json::json!({ "keep": true }));
        initial.save(path.to_str().unwrap()).unwrap();

        let screen_draw = ScreenDrawSettings {
            default_tool: ScreenDrawTool::Text,
            toolbar_position: Some(DesktopPoint::new(-1000, 25)),
            ..Default::default()
        };
        update_screen_draw_settings(path.to_str().unwrap(), &screen_draw).unwrap();

        let restored = crate::settings::Settings::load(path.to_str().unwrap()).unwrap();
        assert_eq!(
            restored.plugin_settings.get("unrelated"),
            Some(&serde_json::json!({ "keep": true }))
        );
        assert_eq!(
            serde_json::from_value::<ScreenDrawSettings>(
                restored.plugin_settings[SETTINGS_KEY].clone()
            )
            .unwrap(),
            screen_draw
        );
    }
}
