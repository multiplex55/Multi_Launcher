use std::time::{Duration, Instant};

use eframe::egui;

use crate::screen_draw::{
    CanvasBackground, DesktopPoint, DesktopRect, DesktopSize, ExportBackground, ExportDestination,
    ExportRequest, ExportScope, RgbaColor, ScreenDrawController, ScreenDrawSettings,
    ScreenDrawState, ScreenDrawTool, ToolbarOrientation, clamp_toolbar_position,
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
    settings_dirty_since: Option<Instant>,
    export_background: ExportBackground,
    #[cfg(test)]
    pub(super) focus_request_count: usize,
}

impl ScreenDrawToolbarUi {
    fn begin_open(&mut self, settings: &ScreenDrawSettings, pixels_per_point: f32) {
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
        let mut observed_scale = None;
        let mut escape_pressed = false;

        let toolbar_size = toolbar_size_points(settings.toolbar_orientation);
        let mut builder = egui::ViewportBuilder::default()
            .with_title("Screen Draw")
            .with_inner_size(toolbar_size)
            .with_min_inner_size(toolbar_size)
            .with_max_inner_size(toolbar_size)
            .with_resizable(false)
            .with_always_on_top()
            .with_taskbar(false)
            .with_minimize_button(false)
            .with_maximize_button(false);
        if let Some(position) = initial_position {
            builder = builder.with_position(position);
        }

        ctx.show_viewport_immediate(viewport_id(), builder, |child, _| {
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
                observed_scale = viewport.native_pixels_per_point;
                close_requested = viewport.close_requested();
            });
            escape_pressed = consume_drawing_escape(child, &state);
        });
        self.screen_draw_toolbar.export_background = export_background;

        if let (Some(position), Some(scale)) = (observed_position, observed_scale) {
            let physical = logical_to_physical(position, scale);
            let settings = self.screen_draw_controller.settings().clone();
            let mut updated = settings;
            self.screen_draw_toolbar
                .note_position(physical, &mut updated);
            self.screen_draw_controller.update_settings(updated);
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
        self.screen_draw_controller.close();
        if self.screen_draw_toolbar.is_dirty() {
            self.persist_screen_draw_settings();
        }
    }
}

fn consume_drawing_escape(ctx: &egui::Context, state: &ScreenDrawState) -> bool {
    matches!(state, ScreenDrawState::Drawing { .. })
        && ctx.input_mut(|input| {
            let modifiers = input.modifiers;
            input.consume_key(modifiers, egui::Key::Escape)
        })
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
