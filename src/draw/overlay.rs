use crate::draw::controller::OverlayController;
use crate::draw::input::{
    bridge_key_event_to_runtime, bridge_left_down_to_runtime, bridge_left_up_to_runtime,
    bridge_mouse_move_to_runtime, DrawInputState, InputCommand, PointerModifiers,
};
use crate::draw::keyboard_hook::{map_key_event_to_command, KeyCommand, KeyEvent, KeyboardHook};
use crate::draw::messages::{
    ExitDialogMode, ExitReason, MainToOverlay, OverlayCommand, OverlayToMain, SaveResult,
};
use crate::draw::model::{Color, ObjectStyle, StrokeStyle, Tool};
use crate::draw::monitor;
use crate::draw::perf::{draw_perf_runtime_enabled, DrawPerfSnapshot, DrawPerfStats};
use crate::draw::render::{
    BackgroundClearMode, DirtyRect, LayeredRenderer, RenderFrameBuffer, RenderSettings,
};
use crate::draw::service::MonitorRect;
use crate::draw::settings::{
    default_debug_hud_toggle_hotkey_value, default_toolbar_toggle_hotkey_value,
    CanvasBackgroundMode, DrawColor, DrawSettings, DrawTool,
};
use crate::draw::toolbar::{self, ToolbarCommand, ToolbarPointerEvent, ToolbarState};
use crate::hotkey::{parse_hotkey, Hotkey, Key};
use anyhow::{anyhow, Result};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitDialogState {
    Hidden,
    PromptVisible,
    Saving,
    ErrorVisible,
}

impl ExitDialogState {
    pub fn blocks_drawing_input(self) -> bool {
        !matches!(self, Self::Hidden)
    }
}

impl From<ExitDialogMode> for ExitDialogState {
    fn from(value: ExitDialogMode) -> Self {
        match value {
            ExitDialogMode::Hidden => Self::Hidden,
            ExitDialogMode::PromptVisible => Self::PromptVisible,
            ExitDialogMode::Saving => Self::Saving,
            ExitDialogMode::ErrorVisible => Self::ErrorVisible,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayPointerEvent {
    LeftDown { modifiers: PointerModifiers },
    Move,
    LeftUp,
}

pub struct OverlayHandles {
    pub overlay_thread_handle: JoinHandle<()>,
    pub main_to_overlay_tx: Sender<MainToOverlay>,
    pub overlay_to_main_rx: Receiver<OverlayToMain>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentBackend {
    Cpu,
    Gpu,
}

fn select_present_backend() -> PresentBackend {
    match std::env::var("ML_DRAW_PRESENT_BACKEND") {
        Ok(raw) if raw.trim().eq_ignore_ascii_case("cpu") => PresentBackend::Cpu,
        Ok(raw) if raw.trim().eq_ignore_ascii_case("gpu") => PresentBackend::Gpu,
        _ => {
            #[cfg(windows)]
            {
                PresentBackend::Gpu
            }
            #[cfg(not(windows))]
            {
                PresentBackend::Cpu
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct OverlayDiagnostics {
    pub pointer_down_count: u64,
    pub pointer_move_count: u64,
    pub pointer_up_count: u64,
    pub key_event_count: u64,
    pub undo_count: u64,
    pub redo_count: u64,
    pub paint_count: u64,
    pub queue_depth: usize,
    pub dropped_events: u64,
    pub coalesced_events: u64,
    pub frame_misses: u64,
    pub tick_budget_overruns: u64,
    pub input_to_present_ms: f64,
    pub effective_present_hz: f64,
    pub last_input_event_summary: String,
}

#[derive(Debug, Clone)]
struct OverlayThreadState {
    toolbar_state: ToolbarState,
    toolbar_toggle_hotkey: Hotkey,
    debug_hud_visible: bool,
    debug_hud_toggle_hotkey: Hotkey,
    quick_colors: Vec<DrawColor>,
    tick_interval: Duration,
    diagnostics: OverlayDiagnostics,
    perf_stats: DrawPerfStats,
}

impl OverlayThreadState {
    fn from_settings(settings: &DrawSettings) -> Self {
        Self {
            toolbar_state: ToolbarState::new(
                !settings.toolbar_collapsed,
                settings.toolbar_collapsed,
                (settings.toolbar_origin_x, settings.toolbar_origin_y),
            ),
            toolbar_toggle_hotkey: parse_toolbar_hotkey_with_fallback(
                &settings.toolbar_toggle_hotkey,
            ),
            debug_hud_visible: settings.debug_hud_enabled,
            debug_hud_toggle_hotkey: parse_debug_hud_hotkey_with_fallback(
                &settings.debug_hud_toggle_hotkey,
            ),
            quick_colors: settings.quick_colors.clone(),
            tick_interval: settings.tick_interval(),
            diagnostics: OverlayDiagnostics::default(),
            perf_stats: DrawPerfStats::new(
                draw_perf_runtime_enabled(settings.draw_perf_debug),
                120,
            ),
        }
    }

    fn update_from_settings(&mut self, settings: &DrawSettings) {
        self.toolbar_toggle_hotkey =
            parse_toolbar_hotkey_with_fallback(&settings.toolbar_toggle_hotkey);
        self.toolbar_state.collapsed = settings.toolbar_collapsed;
        self.toolbar_state.position = (settings.toolbar_origin_x, settings.toolbar_origin_y);
        self.debug_hud_toggle_hotkey =
            parse_debug_hud_hotkey_with_fallback(&settings.debug_hud_toggle_hotkey);
        self.quick_colors = settings.quick_colors.clone();
        self.tick_interval = settings.tick_interval();
        self.perf_stats
            .set_enabled(draw_perf_runtime_enabled(settings.draw_perf_debug));
    }

    fn apply_pointer_event(&mut self, event: OverlayPointerEvent) {
        match event {
            OverlayPointerEvent::LeftDown { .. } => {
                self.diagnostics.pointer_down_count += 1;
                self.diagnostics.last_input_event_summary = "pointer_down".to_string();
            }
            OverlayPointerEvent::Move => {
                self.diagnostics.pointer_move_count += 1;
                self.diagnostics.last_input_event_summary = "pointer_move".to_string();
            }
            OverlayPointerEvent::LeftUp => {
                self.diagnostics.pointer_up_count += 1;
                self.diagnostics.last_input_event_summary = "pointer_up".to_string();
            }
        }
    }

    fn apply_queue_metrics(&mut self, depth: usize, dropped: u64, coalesced: u64) {
        self.diagnostics.queue_depth = depth;
        self.diagnostics.dropped_events = self.diagnostics.dropped_events.saturating_add(dropped);
        self.diagnostics.coalesced_events =
            self.diagnostics.coalesced_events.saturating_add(coalesced);
    }

    fn apply_key_dispatch(&mut self, command: Option<InputCommand>) {
        self.diagnostics.key_event_count += 1;
        self.diagnostics.last_input_event_summary = format!("key:{command:?}");
        match command {
            Some(InputCommand::Undo) => self.diagnostics.undo_count += 1,
            Some(InputCommand::Redo) => self.diagnostics.redo_count += 1,
            _ => {}
        }
    }
}

fn parse_toolbar_hotkey_with_fallback(raw: &str) -> Hotkey {
    parse_hotkey(raw)
        .or_else(|| parse_hotkey(&default_toolbar_toggle_hotkey_value()))
        .unwrap_or_default()
}

fn parse_debug_hud_hotkey_with_fallback(raw: &str) -> Hotkey {
    parse_hotkey(raw)
        .or_else(|| parse_hotkey(&default_debug_hud_toggle_hotkey_value()))
        .unwrap_or_default()
}

fn key_event_matches_hotkey(event: KeyEvent, hotkey: Hotkey) -> bool {
    let Some(key) = map_overlay_key_to_hotkey_key(event.key) else {
        return false;
    };
    key == hotkey.key
        && event.modifiers.ctrl == hotkey.ctrl
        && event.modifiers.shift == hotkey.shift
        && event.modifiers.alt == hotkey.alt
        && event.modifiers.win == hotkey.win
}

fn handle_toolbar_toggle_hotkey_event(state: &mut OverlayThreadState, event: KeyEvent) -> bool {
    if !key_event_matches_hotkey(event, state.toolbar_toggle_hotkey) {
        return false;
    }
    state
        .toolbar_state
        .apply_command(ToolbarCommand::ToggleVisibility);
    true
}

fn handle_debug_hud_toggle_hotkey_event(state: &mut OverlayThreadState, event: KeyEvent) -> bool {
    if !key_event_matches_hotkey(event, state.debug_hud_toggle_hotkey) {
        return false;
    }
    state.debug_hud_visible = !state.debug_hud_visible;
    state.diagnostics.last_input_event_summary = "toggle_debug_hud".to_string();
    true
}

fn map_overlay_key_to_hotkey_key(key: crate::draw::keyboard_hook::KeyCode) -> Option<Key> {
    use crate::draw::keyboard_hook::KeyCode;
    Some(match key {
        KeyCode::KeyA => Key::KeyA,
        KeyCode::KeyB => Key::KeyB,
        KeyCode::KeyC => Key::KeyC,
        KeyCode::KeyD => Key::KeyD,
        KeyCode::KeyE => Key::KeyE,
        KeyCode::KeyF => Key::KeyF,
        KeyCode::KeyG => Key::KeyG,
        KeyCode::KeyH => Key::KeyH,
        KeyCode::KeyI => Key::KeyI,
        KeyCode::KeyJ => Key::KeyJ,
        KeyCode::KeyK => Key::KeyK,
        KeyCode::KeyL => Key::KeyL,
        KeyCode::KeyM => Key::KeyM,
        KeyCode::KeyN => Key::KeyN,
        KeyCode::KeyO => Key::KeyO,
        KeyCode::KeyP => Key::KeyP,
        KeyCode::KeyQ => Key::KeyQ,
        KeyCode::KeyR => Key::KeyR,
        KeyCode::KeyS => Key::KeyS,
        KeyCode::KeyT => Key::KeyT,
        KeyCode::U => Key::KeyU,
        KeyCode::KeyV => Key::KeyV,
        KeyCode::KeyW => Key::KeyW,
        KeyCode::KeyX => Key::KeyX,
        KeyCode::KeyY => Key::KeyY,
        KeyCode::KeyZ => Key::KeyZ,
        KeyCode::Num0 => Key::Num0,
        KeyCode::Num1 => Key::Num1,
        KeyCode::Num2 => Key::Num2,
        KeyCode::Num3 => Key::Num3,
        KeyCode::Num4 => Key::Num4,
        KeyCode::Num5 => Key::Num5,
        KeyCode::Num6 => Key::Num6,
        KeyCode::Num7 => Key::Num7,
        KeyCode::Num8 => Key::Num8,
        KeyCode::Num9 => Key::Num9,
        KeyCode::Space => Key::Space,
        KeyCode::Tab => Key::Tab,
        KeyCode::Enter => Key::Return,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::CapsLock => Key::CapsLock,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Left => Key::LeftArrow,
        KeyCode::Right => Key::RightArrow,
        KeyCode::Up => Key::UpArrow,
        KeyCode::Down => Key::DownArrow,
        KeyCode::F1 => Key::F1,
        KeyCode::F2 => Key::F2,
        KeyCode::F3 => Key::F3,
        KeyCode::F4 => Key::F4,
        KeyCode::F5 => Key::F5,
        KeyCode::F6 => Key::F6,
        KeyCode::F7 => Key::F7,
        KeyCode::F8 => Key::F8,
        KeyCode::F9 => Key::F9,
        KeyCode::F10 => Key::F10,
        KeyCode::F11 => Key::F11,
        KeyCode::F12 => Key::F12,
        KeyCode::F13 => Key::F13,
        KeyCode::F14 => Key::F14,
        KeyCode::F15 => Key::F15,
        KeyCode::F16 => Key::F16,
        KeyCode::F17 => Key::F17,
        KeyCode::F18 => Key::F18,
        KeyCode::F19 => Key::F19,
        KeyCode::F20 => Key::F20,
        KeyCode::F21 => Key::F21,
        KeyCode::F22 => Key::F22,
        KeyCode::F23 => Key::F23,
        KeyCode::F24 => Key::F24,
        KeyCode::Escape => Key::Escape,
        KeyCode::Other => return None,
    })
}

fn map_draw_tool(tool: DrawTool) -> Tool {
    match tool {
        DrawTool::Pen => Tool::Pen,
        DrawTool::Line => Tool::Line,
        DrawTool::Rect => Tool::Rect,
        DrawTool::Ellipse => Tool::Ellipse,
        DrawTool::Eraser => Tool::Eraser,
    }
}

fn map_draw_color(color: DrawColor) -> Color {
    Color::rgba(color.r, color.g, color.b, color.a)
}

fn map_overlay_command_to_toolbar(command: OverlayCommand) -> Option<ToolbarCommand> {
    Some(match command {
        OverlayCommand::SelectTool(tool) => ToolbarCommand::SelectTool(tool),
        OverlayCommand::SetStrokeWidth(width) => ToolbarCommand::SetStrokeWidth(width),
        OverlayCommand::SetColor(color) => ToolbarCommand::SetColor(color),
        OverlayCommand::SetFillEnabled(enabled) => ToolbarCommand::SetFillEnabled(enabled),
        OverlayCommand::SetFillColor(color) => ToolbarCommand::SetFillColor(color),
        OverlayCommand::Undo => ToolbarCommand::Undo,
        OverlayCommand::Redo => ToolbarCommand::Redo,
        OverlayCommand::Save => ToolbarCommand::Save,
        OverlayCommand::ToggleToolbarVisibility => ToolbarCommand::ToggleVisibility,
        OverlayCommand::ToggleToolbarCollapsed => ToolbarCommand::ToggleCollapsed,
        OverlayCommand::SetToolbarPosition { x, y } => ToolbarCommand::SetPosition { x, y },
        OverlayCommand::Exit => ToolbarCommand::Exit,
    })
}

fn live_render_settings(settings: &DrawSettings) -> RenderSettings {
    let clear_mode = match settings.canvas_background_mode {
        CanvasBackgroundMode::Transparent => BackgroundClearMode::Transparent,
        CanvasBackgroundMode::Solid => {
            let color = settings
                .canvas_solid_background_color
                .resolve_first_pass_colorkey_collision();
            BackgroundClearMode::Solid(Color::rgba(color.r, color.g, color.b, 255))
        }
    };

    RenderSettings {
        clear_mode,
        wide_stroke_threshold: 10,
    }
}

fn rerender_and_repaint(
    window: &mut OverlayWindow,
    draw_input: &DrawInputState,
    settings: &DrawSettings,
    overlay_state: &mut OverlayThreadState,
    framebuffer: &mut RenderFrameBuffer,
    layered_renderer: &mut LayeredRenderer,
    dirty: Option<DirtyRect>,
    force_full_redraw: bool,
) {
    let frame_start = Instant::now();
    let window_size = window.bitmap_size();
    let dirty_pixels = dirty
        .and_then(|rect| rect.clamp(window_size.0, window_size.1))
        .map(|rect| (rect.width.max(0) as u64).saturating_mul(rect.height.max(0) as u64))
        .unwrap_or_else(|| window_size.0 as u64 * window_size.1 as u64);
    let requires_full_overlay =
        overlay_state.toolbar_state.visible || overlay_state.debug_hud_visible;

    let raster_span = overlay_state.perf_stats.begin_raster();
    if !requires_full_overlay {
        let active_object = draw_input.active_object();
        layered_renderer.render_to_window(
            window,
            &draw_input.committed_canvas(),
            active_object.as_ref(),
            live_render_settings(settings),
            window_size,
            dirty,
            force_full_redraw,
            draw_input.committed_revision(),
        );
    } else {
        let canvas = draw_input.committed_canvas();
        let mut canvas = canvas;
        if let Some(active) = draw_input.active_object() {
            canvas.objects.push(active);
        }

        framebuffer.render(
            &canvas,
            live_render_settings(settings),
            window_size,
            None,
            force_full_redraw || requires_full_overlay,
        );

        let mut rgba = framebuffer_clone_rgba(framebuffer);
        if overlay_state.toolbar_state.visible {
            draw_compact_toolbar_panel(
                &mut rgba,
                window.bitmap_size(),
                draw_input,
                &overlay_state.quick_colors,
                &overlay_state.toolbar_state,
            );
        }
        if overlay_state.debug_hud_visible {
            let perf_snapshot = overlay_state.perf_stats.snapshot();
            draw_debug_hud_panel(
                &mut rgba,
                window.bitmap_size(),
                draw_input,
                &overlay_state.diagnostics,
                perf_snapshot,
            );
        }
        window.with_bitmap_mut(|dib, width, height| {
            if width == 0 || height == 0 || dib.len() != rgba.len() {
                return;
            }
            crate::draw::render::convert_rgba_to_dib_bgra(&rgba, dib);
        });
    }
    overlay_state
        .perf_stats
        .end_raster(raster_span, dirty_pixels.max(1));

    overlay_state.diagnostics.paint_count += 1;
    overlay_state.perf_stats.mark_invalidate_requested();
    if requires_full_overlay || force_full_redraw {
        window.request_paint();
    } else if let Some(rect) = dirty.and_then(|d| d.clamp(window_size.0, window_size.1)) {
        window.request_paint_rect(rect);
    } else {
        window.request_paint();
    }
    overlay_state.perf_stats.mark_paint_completed();
    overlay_state.perf_stats.finish_frame(
        frame_start.elapsed().as_secs_f64() * 1000.0,
        dirty_pixels.max(1),
    );
    let perf = overlay_state.perf_stats.snapshot();
    overlay_state.diagnostics.input_to_present_ms = perf.input_to_present_ms;
    overlay_state.diagnostics.effective_present_hz = perf.effective_present_hz;
}

fn framebuffer_clone_rgba(framebuffer: &RenderFrameBuffer) -> Vec<u8> {
    framebuffer.rgba_pixels().to_vec()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OverlayInputDrain {
    pub queue_depth: usize,
    pub dropped_events: u64,
    pub coalesced_events: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayPointerSample {
    pub global_point: (i32, i32),
    pub event: OverlayPointerEvent,
}

#[derive(Debug, Clone, Default)]
pub struct PointerDrainBatch {
    pub events: Vec<OverlayPointerSample>,
    pub diagnostics: OverlayInputDrain,
}

#[derive(Debug, Clone)]
struct CoalesceOutcome {
    events: Vec<OverlayPointerSample>,
    coalesced_count: u64,
}

fn flush_move_run(
    run: &mut Vec<OverlayPointerSample>,
    out: &mut Vec<OverlayPointerSample>,
    keep_samples: usize,
    coalesced_count: &mut u64,
) {
    if run.is_empty() {
        return;
    }
    let keep = keep_samples.max(1).min(run.len());
    let dropped = run.len().saturating_sub(keep) as u64;
    *coalesced_count = coalesced_count.saturating_add(dropped);
    let start = run.len().saturating_sub(keep);
    out.extend(run.drain(start..));
    run.clear();
}

fn coalesce_pointer_events(
    events: Vec<OverlayPointerSample>,
    keep_move_samples: usize,
) -> CoalesceOutcome {
    let mut coalesced = Vec::with_capacity(events.len());
    let mut move_run: Vec<OverlayPointerSample> = Vec::new();
    let mut coalesced_count = 0_u64;
    for event in events {
        if matches!(event.event, OverlayPointerEvent::Move) {
            move_run.push(event);
            continue;
        }
        flush_move_run(
            &mut move_run,
            &mut coalesced,
            keep_move_samples,
            &mut coalesced_count,
        );
        coalesced.push(event);
    }
    flush_move_run(
        &mut move_run,
        &mut coalesced,
        keep_move_samples,
        &mut coalesced_count,
    );
    CoalesceOutcome {
        events: coalesced,
        coalesced_count,
    }
}

fn apply_toolbar_command(
    draw_input: &mut DrawInputState,
    overlay_state: &mut OverlayThreadState,
    command: ToolbarCommand,
) {
    #[cfg(debug_assertions)]
    let full_redraw_count_before = draw_input.full_redraw_request_count();

    match command {
        ToolbarCommand::SelectTool(tool) => draw_input.set_tool(tool),
        ToolbarCommand::SetStrokeWidth(width) => {
            let mut style = draw_input.current_style();
            style.stroke.width = width.max(1);
            draw_input.set_style(style);
        }
        ToolbarCommand::SetColor(color) => {
            let mut style = draw_input.current_style();
            style.stroke.color = color;
            draw_input.set_style(style);
        }
        ToolbarCommand::SetFillEnabled(enabled) => {
            let mut style = draw_input.current_style();
            style.fill = if enabled {
                Some(crate::draw::model::FillStyle {
                    color: style.stroke.color,
                })
            } else {
                None
            };
            draw_input.set_style(style);
        }
        ToolbarCommand::SetFillColor(color) => {
            let mut style = draw_input.current_style();
            style.fill = Some(crate::draw::model::FillStyle { color });
            draw_input.set_style(style);
        }
        ToolbarCommand::Undo => {
            let _ = bridge_key_event_to_runtime(
                draw_input,
                KeyEvent {
                    key: crate::draw::keyboard_hook::KeyCode::U,
                    modifiers: Default::default(),
                },
            );
        }
        ToolbarCommand::Redo => {
            let _ = bridge_key_event_to_runtime(
                draw_input,
                KeyEvent {
                    key: crate::draw::keyboard_hook::KeyCode::KeyR,
                    modifiers: crate::draw::keyboard_hook::KeyModifiers {
                        ctrl: true,
                        shift: false,
                        alt: false,
                        win: false,
                    },
                },
            );
        }
        ToolbarCommand::Exit => {
            let _ = bridge_key_event_to_runtime(
                draw_input,
                KeyEvent {
                    key: crate::draw::keyboard_hook::KeyCode::Escape,
                    modifiers: Default::default(),
                },
            );
        }
        ToolbarCommand::ToggleCollapsed => {
            overlay_state
                .toolbar_state
                .apply_command(ToolbarCommand::ToggleCollapsed);
        }
        ToolbarCommand::SetPosition { x, y } => {
            overlay_state
                .toolbar_state
                .apply_command(ToolbarCommand::SetPosition { x, y });
            let mut settings = crate::draw::runtime().settings_snapshot();
            settings.toolbar_origin_x = x;
            settings.toolbar_origin_y = y;
            settings.toolbar_collapsed = overlay_state.toolbar_state.collapsed;
            crate::draw::runtime().apply_settings(settings);
        }
        ToolbarCommand::Save => {
            let _ = crate::draw::runtime().request_exit(ExitReason::UserRequest);
        }
        ToolbarCommand::ToggleVisibility => {
            overlay_state
                .toolbar_state
                .apply_command(ToolbarCommand::ToggleVisibility);
        }
    }

    #[cfg(debug_assertions)]
    {
        let full_redraw_count_after = draw_input.full_redraw_request_count();
        if matches!(
            command,
            ToolbarCommand::SelectTool(_)
                | ToolbarCommand::SetStrokeWidth(_)
                | ToolbarCommand::SetColor(_)
                | ToolbarCommand::SetFillEnabled(_)
                | ToolbarCommand::SetFillColor(_)
        ) {
            debug_assert_eq!(
                full_redraw_count_before, full_redraw_count_after,
                "tool/color hotkey transition unexpectedly requested full redraw"
            );
        }
    }
}

fn handle_toolbar_pointer_event(
    draw_input: &mut DrawInputState,
    overlay_state: &mut OverlayThreadState,
    local_point: (i32, i32),
    event: OverlayPointerEvent,
    window_size: (u32, u32),
) -> bool {
    let pointer_event = match event {
        OverlayPointerEvent::LeftDown { .. } => ToolbarPointerEvent::LeftDown,
        OverlayPointerEvent::Move => ToolbarPointerEvent::Move,
        OverlayPointerEvent::LeftUp => ToolbarPointerEvent::LeftUp,
    };
    let Some(layout) = toolbar::ToolbarLayout::for_state(
        window_size,
        &overlay_state.toolbar_state,
        overlay_state.quick_colors.len(),
    ) else {
        return false;
    };

    if !layout.panel.contains(local_point) && !overlay_state.toolbar_state.dragging {
        overlay_state.toolbar_state.focused = false;
        overlay_state.toolbar_state.hovered_target = None;
        return false;
    }

    overlay_state.toolbar_state.hovered_target =
        layout.hit_test(local_point, overlay_state.toolbar_state.collapsed);

    if matches!(pointer_event, ToolbarPointerEvent::LeftDown) {
        overlay_state.toolbar_state.focused = true;
        if let Some(target) = layout.hit_test(local_point, overlay_state.toolbar_state.collapsed) {
            if matches!(target, toolbar::ToolbarHitTarget::Header) {
                overlay_state.toolbar_state.dragging = true;
                overlay_state.toolbar_state.drag_anchor = (
                    local_point.0 - overlay_state.toolbar_state.position.0,
                    local_point.1 - overlay_state.toolbar_state.position.1,
                );
                return true;
            }
            if let Some(command) = toolbar::map_hit_to_command(
                target,
                draw_input.current_style(),
                &overlay_state.quick_colors,
            ) {
                apply_toolbar_command(draw_input, overlay_state, command);
            }
            return true;
        }
    }

    if matches!(pointer_event, ToolbarPointerEvent::Move) && overlay_state.toolbar_state.dragging {
        apply_toolbar_command(
            draw_input,
            overlay_state,
            ToolbarCommand::SetPosition {
                x: local_point.0 - overlay_state.toolbar_state.drag_anchor.0,
                y: local_point.1 - overlay_state.toolbar_state.drag_anchor.1,
            },
        );
        return true;
    }

    if matches!(pointer_event, ToolbarPointerEvent::LeftUp) {
        overlay_state.toolbar_state.dragging = false;
        return layout.panel.contains(local_point);
    }

    true
}

#[derive(Clone, Copy)]
struct ButtonVisualState {
    active: bool,
    hovered: bool,
    disabled: bool,
}

fn draw_icon_button(
    rgba: &mut [u8],
    width: u32,
    height: u32,
    rect: crate::draw::toolbar::ToolbarRect,
    target: toolbar::ToolbarHitTarget,
    toolbar_state: &ToolbarState,
    visual: ButtonVisualState,
) {
    let bg = if visual.disabled {
        [52, 52, 52, 255]
    } else if visual.active {
        [110, 132, 160, 255]
    } else if visual.hovered {
        [96, 96, 96, 255]
    } else {
        [76, 76, 76, 255]
    };
    fill_rect(rgba, width, height, rect.x, rect.y, rect.w, rect.h, bg);

    if let Some(icon) = toolbar::hit_target_icon(target, toolbar_state.collapsed) {
        let color = if visual.disabled {
            [120, 120, 120, 255]
        } else if visual.active {
            [240, 245, 255, 255]
        } else {
            [220, 220, 220, 255]
        };
        draw_icon_glyph(rgba, width, height, rect, icon, color);
    }
}

fn draw_icon_glyph(
    rgba: &mut [u8],
    width: u32,
    height: u32,
    rect: crate::draw::toolbar::ToolbarRect,
    icon: crate::draw::toolbar_icons::ToolbarIcon,
    color: [u8; 4],
) {
    let glyph = crate::draw::toolbar_icons::icon_bitmap(icon);
    let glyph_h = glyph.len() as i32;
    let glyph_w = glyph.iter().map(|row| row.len()).max().unwrap_or(0) as i32;
    let x0 = rect.x + (rect.w - glyph_w) / 2;
    let y0 = rect.y + (rect.h - glyph_h) / 2;
    for (row_idx, row) in glyph.iter().enumerate() {
        for (col_idx, pixel) in row.as_bytes().iter().enumerate() {
            if *pixel == b'1' {
                fill_rect(
                    rgba,
                    width,
                    height,
                    x0 + col_idx as i32,
                    y0 + row_idx as i32,
                    1,
                    1,
                    color,
                );
            }
        }
    }
}

fn draw_compact_toolbar_panel(
    rgba: &mut [u8],
    size: (u32, u32),
    draw_input: &DrawInputState,
    quick_colors: &[DrawColor],
    toolbar_state: &ToolbarState,
) {
    let (width, height) = size;
    let Some(layout) = toolbar::ToolbarLayout::for_state(size, toolbar_state, quick_colors.len())
    else {
        return;
    };

    fill_rect(
        rgba,
        width,
        height,
        layout.panel.x,
        layout.panel.y,
        layout.panel.w,
        layout.panel.h,
        [24, 24, 24, 200],
    );
    fill_rect(
        rgba,
        width,
        height,
        layout.header.x,
        layout.header.y,
        layout.header.w,
        layout.header.h,
        [36, 36, 36, 220],
    );

    draw_icon_button(
        rgba,
        width,
        height,
        layout.collapse_toggle,
        toolbar::ToolbarHitTarget::ToggleCollapse,
        toolbar_state,
        ButtonVisualState {
            active: toolbar_state.collapsed,
            hovered: toolbar_state.hovered_target
                == Some(toolbar::ToolbarHitTarget::ToggleCollapse),
            disabled: false,
        },
    );

    if toolbar_state.collapsed {
        return;
    }

    for (tool, rect) in &layout.tool_rects {
        draw_icon_button(
            rgba,
            width,
            height,
            *rect,
            toolbar::ToolbarHitTarget::Tool(*tool),
            toolbar_state,
            ButtonVisualState {
                active: draw_input.current_tool() == *tool,
                hovered: toolbar_state.hovered_target
                    == Some(toolbar::ToolbarHitTarget::Tool(*tool)),
                disabled: false,
            },
        );
    }

    let style = draw_input.current_style();
    let active_color = style.stroke.color;
    for (idx, rect) in &layout.quick_color_rects {
        if let Some(color) = quick_colors.get(*idx) {
            fill_rect(
                rgba,
                width,
                height,
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                [color.r, color.g, color.b, 255],
            );
            if active_color.r == color.r && active_color.g == color.g && active_color.b == color.b {
                fill_rect(
                    rgba,
                    width,
                    height,
                    rect.x - 1,
                    rect.y - 1,
                    rect.w + 2,
                    1,
                    [255, 255, 255, 255],
                );
            }
        }
    }

    if let Some(fill_color) = style.fill.map(|f| f.color) {
        for (idx, rect) in &layout.fill_color_rects {
            if let Some(color) = quick_colors.get(*idx) {
                fill_rect(
                    rgba,
                    width,
                    height,
                    rect.x,
                    rect.y,
                    rect.w,
                    rect.h,
                    [color.r, color.g, color.b, 255],
                );
                if fill_color.r == color.r && fill_color.g == color.g && fill_color.b == color.b {
                    fill_rect(
                        rgba,
                        width,
                        height,
                        rect.x - 1,
                        rect.y - 1,
                        rect.w + 2,
                        1,
                        [255, 255, 255, 255],
                    );
                }
            }
        }
    } else {
        for (idx, rect) in &layout.fill_color_rects {
            if let Some(color) = quick_colors.get(*idx) {
                fill_rect(
                    rgba,
                    width,
                    height,
                    rect.x,
                    rect.y,
                    rect.w,
                    rect.h,
                    [color.r, color.g, color.b, 255],
                );
            }
        }
    }

    let undo_disabled = draw_input.history().undo_len() == 0;
    let redo_disabled = draw_input.history().redo_len() == 0;
    let stroke_down_disabled = style.stroke.width <= 1;

    let controls = [
        (
            layout.width_down_rect,
            toolbar::ToolbarHitTarget::StrokeWidthDown,
            ButtonVisualState {
                active: false,
                hovered: toolbar_state.hovered_target
                    == Some(toolbar::ToolbarHitTarget::StrokeWidthDown),
                disabled: stroke_down_disabled,
            },
        ),
        (
            layout.width_up_rect,
            toolbar::ToolbarHitTarget::StrokeWidthUp,
            ButtonVisualState {
                active: false,
                hovered: toolbar_state.hovered_target
                    == Some(toolbar::ToolbarHitTarget::StrokeWidthUp),
                disabled: false,
            },
        ),
        (
            layout.fill_toggle_rect,
            toolbar::ToolbarHitTarget::FillToggle,
            ButtonVisualState {
                active: style.fill.is_some(),
                hovered: toolbar_state.hovered_target
                    == Some(toolbar::ToolbarHitTarget::FillToggle),
                disabled: false,
            },
        ),
        (
            layout.undo_rect,
            toolbar::ToolbarHitTarget::Undo,
            ButtonVisualState {
                active: false,
                hovered: toolbar_state.hovered_target == Some(toolbar::ToolbarHitTarget::Undo),
                disabled: undo_disabled,
            },
        ),
        (
            layout.redo_rect,
            toolbar::ToolbarHitTarget::Redo,
            ButtonVisualState {
                active: false,
                hovered: toolbar_state.hovered_target == Some(toolbar::ToolbarHitTarget::Redo),
                disabled: redo_disabled,
            },
        ),
        (
            layout.save_rect,
            toolbar::ToolbarHitTarget::Save,
            ButtonVisualState {
                active: false,
                hovered: toolbar_state.hovered_target == Some(toolbar::ToolbarHitTarget::Save),
                disabled: false,
            },
        ),
        (
            layout.exit_rect,
            toolbar::ToolbarHitTarget::Exit,
            ButtonVisualState {
                active: false,
                hovered: toolbar_state.hovered_target == Some(toolbar::ToolbarHitTarget::Exit),
                disabled: false,
            },
        ),
    ];

    for (rect, target, visual) in controls {
        draw_icon_button(rgba, width, height, rect, target, toolbar_state, visual);
    }
}

fn draw_debug_hud_panel(
    rgba: &mut [u8],
    size: (u32, u32),
    draw_input: &DrawInputState,
    diagnostics: &OverlayDiagnostics,
    perf: DrawPerfSnapshot,
) {
    let (width, height) = size;
    if width < 280 || height < 180 {
        return;
    }

    let panel_w = 360;
    let panel_h = 168;
    let panel_x = width as i32 - panel_w - 16;
    let panel_y = 16;
    fill_rect(
        rgba,
        width,
        height,
        panel_x,
        panel_y,
        panel_w,
        panel_h,
        [12, 12, 12, 220],
    );

    let canvas = draw_input.history().canvas();
    let lines = [
        format!("TOOL {:?}", draw_input.current_tool()),
        format!(
            "STROKES {} OBJECTS {}",
            draw_input.history().undo_len(),
            canvas.objects.len()
        ),
        format!("LAST {}", diagnostics.last_input_event_summary),
        format!(
            "PD {} PM {} PU {}",
            diagnostics.pointer_down_count,
            diagnostics.pointer_move_count,
            diagnostics.pointer_up_count
        ),
        format!(
            "avg_ms {:.2} worst_ms {:.2} p95_ms {:.2}",
            perf.avg_ms, perf.worst_ms, perf.p95_ms
        ),
        format!(
            "pps {:.0} present_hz {:.0} in->present {:.2}ms",
            perf.points_per_second, perf.effective_present_hz, perf.input_to_present_ms
        ),
        format!(
            "dirty_pixels {} inv_to_paint {:.2}ms",
            perf.dirty_pixels, perf.invalidate_to_paint_ms
        ),
        format!(
            "raster {:.2}ms touched {}",
            perf.raster_ms, perf.estimated_pixels_touched
        ),
        format!(
            "q {} drop {} coal {} miss {}",
            diagnostics.queue_depth,
            diagnostics.dropped_events,
            diagnostics.coalesced_events,
            diagnostics.frame_misses
        ),
    ];

    for (idx, line) in lines.iter().enumerate() {
        draw_tiny_text(
            rgba,
            width,
            height,
            panel_x + 8,
            panel_y + 8 + (idx as i32 * 20),
            line,
            [230, 230, 230, 255],
        );
    }
}

fn draw_tiny_text(
    rgba: &mut [u8],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    text: &str,
    color: [u8; 4],
) {
    let mut cursor_x = x;
    for ch in text.chars() {
        let glyph = tiny_glyph(ch.to_ascii_uppercase());
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..5 {
                if bits & (1 << (4 - col)) != 0 {
                    fill_rect(
                        rgba,
                        width,
                        height,
                        cursor_x + col,
                        y + row as i32,
                        1,
                        1,
                        color,
                    );
                }
            }
        }
        cursor_x += 6;
    }
}

fn tiny_glyph(ch: char) -> [u8; 7] {
    match ch {
        'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        'D' => [0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E],
        'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'I' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x1F],
        'J' => [0x01, 0x01, 0x01, 0x01, 0x11, 0x11, 0x0E],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        'M' => [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x11, 0x19, 0x15, 0x13, 0x11, 0x11],
        'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        'S' => [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
        'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04],
        'Y' => [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04],
        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        '3' => [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        ':' => [0x00, 0x04, 0x04, 0x00, 0x04, 0x04, 0x00],
        '_' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1F],
        ' ' => [0x00; 7],
        _ => [0x1F, 0x11, 0x11, 0x1F, 0x10, 0x10, 0x10],
    }
}

fn fill_rect(
    rgba: &mut [u8],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: [u8; 4],
) {
    for py in y.max(0)..(y + h).min(height as i32) {
        for px in x.max(0)..(x + w).min(width as i32) {
            let idx = ((py as u32 * width + px as u32) * 4) as usize;
            if idx + 3 < rgba.len() {
                rgba[idx..idx + 4].copy_from_slice(&color);
            }
        }
    }
}

fn send_exit_after_cleanup<F>(
    cleanup: F,
    overlay_to_main_tx: &Sender<OverlayToMain>,
    reason: ExitReason,
    save_result: SaveResult,
) where
    F: FnOnce(),
{
    cleanup();
    let _ = overlay_to_main_tx.send(OverlayToMain::Exited {
        reason,
        save_result,
    });
}

pub fn spawn_overlay_for_monitor(monitor_rect: MonitorRect) -> Result<OverlayHandles> {
    let (main_to_overlay_tx, main_to_overlay_rx) = channel::<MainToOverlay>();
    let (overlay_to_main_tx, overlay_to_main_rx) = channel::<OverlayToMain>();

    let overlay_thread_handle = thread::Builder::new()
        .name("draw-overlay".to_string())
        .spawn(move || {
            let controller_tx = overlay_to_main_tx.clone();
            let mut controller = OverlayController::new(main_to_overlay_rx, overlay_to_main_tx);
            let mut did_start = false;
            let mut window = match OverlayWindow::create_for_monitor(monitor_rect) {
                Some(window) => window,
                None => {
                    let _ = controller_tx.send(OverlayToMain::SaveError {
                        error: "unable to initialize draw overlay window".to_string(),
                    });
                    let _ = controller_tx.send(OverlayToMain::Exited {
                        reason: ExitReason::StartFailure,
                        save_result: SaveResult::Skipped,
                    });
                    return;
                }
            };

            let mut active_settings = crate::draw::runtime().settings_snapshot();
            let mut overlay_state = OverlayThreadState::from_settings(&active_settings);
            let mut keyboard_hook = KeyboardHook::default();
            keyboard_hook.set_toolbar_toggle_hotkey(Some(overlay_state.toolbar_toggle_hotkey));
            let mut draw_input = DrawInputState::new(
                map_draw_tool(active_settings.last_tool),
                ObjectStyle {
                    stroke: StrokeStyle {
                        width: active_settings.last_width.max(1),
                        color: map_draw_color(active_settings.last_color),
                    },
                    fill: None,
                },
            );
            let mut framebuffer = RenderFrameBuffer::default();
            let mut layered_renderer = LayeredRenderer::default();
            let mut last_reported_revision = draw_input.committed_revision();
            let mut next_frame = Instant::now();
            let mut consecutive_tick_misses = 0_u32;
            let mut needs_redraw = true;
            let mut forced_full_redraw = true;

            loop {
                #[cfg(windows)]
                pump_overlay_messages();

                let mut had_work = false;

                for key_event in keyboard_hook.drain_events() {
                    had_work = true;
                    if handle_toolbar_toggle_hotkey_event(&mut overlay_state, key_event)
                        || handle_debug_hud_toggle_hotkey_event(&mut overlay_state, key_event)
                    {
                        needs_redraw = true;
                        forced_full_redraw = true;
                        continue;
                    }

                    let dispatch = forward_key_event_to_draw_input(
                        &mut draw_input,
                        controller.exit_dialog_mode().into(),
                        &overlay_state.quick_colors,
                        key_event,
                    );
                    if dispatch.handled {
                        overlay_state.apply_key_dispatch(dispatch.command);
                        needs_redraw = true;
                    }
                    if dispatch.should_repaint {
                        forced_full_redraw = true;
                    }
                }

                let drained_pointer_events = window.drain_pointer_events();
                let lag_drop_mode = active_settings.drop_intermediate_move_points_on_lag
                    && consecutive_tick_misses >= 3;
                let keep_move_samples = if lag_drop_mode {
                    1
                } else if drained_pointer_events.diagnostics.queue_depth >= 512
                    || drained_pointer_events.diagnostics.dropped_events > 0
                {
                    2
                } else {
                    4
                };
                let coalesced =
                    coalesce_pointer_events(drained_pointer_events.events, keep_move_samples);
                let combined_coalesced = drained_pointer_events
                    .diagnostics
                    .coalesced_events
                    .saturating_add(coalesced.coalesced_count);
                overlay_state.apply_queue_metrics(
                    drained_pointer_events.diagnostics.queue_depth,
                    drained_pointer_events.diagnostics.dropped_events,
                    combined_coalesced,
                );
                overlay_state
                    .perf_stats
                    .mark_coalesced_moves(combined_coalesced);

                for pointer_event in coalesced.events {
                    had_work = true;
                    let input_span = overlay_state.perf_stats.begin_input_ingestion();
                    let local_point = global_to_local(
                        pointer_event.global_point,
                        (window.monitor_rect().x, window.monitor_rect().y),
                    );
                    let handled = if handle_toolbar_pointer_event(
                        &mut draw_input,
                        &mut overlay_state,
                        local_point,
                        pointer_event.event,
                        window.bitmap_size(),
                    ) {
                        true
                    } else {
                        forward_pointer_event_to_draw_input(
                            &mut draw_input,
                            controller.exit_dialog_mode().into(),
                            window.monitor_rect(),
                            pointer_event.global_point,
                            pointer_event.event,
                        )
                    };
                    if handled {
                        overlay_state.apply_pointer_event(pointer_event.event);
                        overlay_state.perf_stats.end_input_ingestion(input_span, 1);
                        needs_redraw = true;
                    } else {
                        overlay_state.perf_stats.end_input_ingestion(input_span, 0);
                    }
                }

                let mut queued_commands = Vec::new();
                controller.pump_runtime_messages(
                    || {
                        let previous = active_settings.clone();
                        active_settings = crate::draw::runtime().settings_snapshot();
                        overlay_state.update_from_settings(&active_settings);
                        keyboard_hook
                            .set_toolbar_toggle_hotkey(Some(overlay_state.toolbar_toggle_hotkey));
                        draw_input.set_tool(map_draw_tool(active_settings.last_tool));
                        draw_input.set_style(ObjectStyle {
                            stroke: StrokeStyle {
                                width: active_settings.last_width.max(1),
                                color: map_draw_color(active_settings.last_color),
                            },
                            fill: None,
                        });
                        needs_redraw = true;
                        if live_render_settings(&previous).clear_mode
                            != live_render_settings(&active_settings).clear_mode
                        {
                            forced_full_redraw = true;
                        }
                        let _ = controller_tx.send(OverlayToMain::SaveProgress {
                            canvas: draw_input.history().canvas(),
                        });
                    },
                    |command| {
                        queued_commands.push(command);
                    },
                );
                for command in queued_commands {
                    if let Some(mapped) = map_overlay_command_to_toolbar(command) {
                        let should_force_full_redraw =
                            matches!(mapped, ToolbarCommand::Undo | ToolbarCommand::Redo);
                        apply_toolbar_command(&mut draw_input, &mut overlay_state, mapped);
                        forced_full_redraw |= should_force_full_redraw;
                    }
                    needs_redraw = true;
                }

                if draw_input.committed_revision() != last_reported_revision {
                    last_reported_revision = draw_input.committed_revision();
                    let _ = controller_tx.send(OverlayToMain::SaveProgress {
                        canvas: draw_input.history().canvas(),
                    });
                }

                if controller.lifecycle() == crate::draw::controller::ControllerLifecycle::Active
                    && !did_start
                {
                    had_work = true;
                    did_start = true;
                    if let Err(err) = keyboard_hook.activate() {
                        tracing::warn!(?err, "failed to activate draw keyboard hook");
                    }
                    window.show();
                    overlay_state.perf_stats.reset();
                    needs_redraw = true;
                    forced_full_redraw = true;
                }

                let exit_reason = controller.exit_reason();

                if exit_reason.is_some() {
                    break;
                }

                let now = Instant::now();
                let input_interval = overlay_state.tick_interval.min(Duration::from_millis(8));
                let input_due = now + input_interval >= next_frame;
                if needs_redraw && (now >= next_frame || input_due) {
                    if now > next_frame + overlay_state.tick_interval {
                        consecutive_tick_misses = consecutive_tick_misses.saturating_add(1);
                        overlay_state.diagnostics.frame_misses =
                            overlay_state.diagnostics.frame_misses.saturating_add(1);
                    } else {
                        consecutive_tick_misses = 0;
                    }
                    let tick_start = Instant::now();
                    let dirty = draw_input.take_dirty_rect();
                    forced_full_redraw |= draw_input.take_full_redraw_request();
                    rerender_and_repaint(
                        &mut window,
                        &draw_input,
                        &active_settings,
                        &mut overlay_state,
                        &mut framebuffer,
                        &mut layered_renderer,
                        dirty,
                        forced_full_redraw,
                    );
                    needs_redraw = false;
                    forced_full_redraw = false;
                    while next_frame <= now {
                        next_frame += overlay_state.tick_interval;
                    }
                    if active_settings.drop_intermediate_move_points_on_lag
                        && consecutive_tick_misses >= 3
                    {
                        overlay_state.diagnostics.last_input_event_summary =
                            "lag_drop_intermediate_moves".to_string();
                    }
                    if cfg!(debug_assertions) && tick_start.elapsed() > overlay_state.tick_interval
                    {
                        overlay_state.diagnostics.tick_budget_overruns = overlay_state
                            .diagnostics
                            .tick_budget_overruns
                            .saturating_add(1);
                        tracing::warn!(
                            elapsed_ms = tick_start.elapsed().as_secs_f64() * 1000.0,
                            budget_ms = overlay_state.tick_interval.as_secs_f64() * 1000.0,
                            "draw overlay tick budget exceeded"
                        );
                    }
                }

                if !had_work {
                    let now = Instant::now();
                    if now < next_frame {
                        let sleep_for = (next_frame - now).min(Duration::from_millis(1));
                        thread::sleep(sleep_for);
                    } else {
                        thread::yield_now();
                    }
                }
            }

            if !did_start {
                let _ = controller_tx.send(OverlayToMain::SaveError {
                    error: "overlay exited before start command".to_string(),
                });
            }
            send_exit_after_cleanup(
                || {
                    keyboard_hook.deactivate();
                    window.shutdown();
                },
                &controller_tx,
                controller
                    .exit_reason()
                    .unwrap_or(ExitReason::OverlayFailure),
                SaveResult::Skipped,
            );
            controller.mark_exited();
        })
        .map_err(|err| anyhow!("failed to spawn draw overlay thread: {err}"))?;

    Ok(OverlayHandles {
        overlay_thread_handle,
        main_to_overlay_tx,
        overlay_to_main_rx,
    })
}

#[cfg(windows)]
fn pump_overlay_messages() {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
    };

    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, HWND::default(), 0, 0, PM_REMOVE).into() {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }
    }
}

pub fn resolve_monitor_from_cursor() -> Option<MonitorRect> {
    monitor::resolve_monitor_from_cursor()
}

pub fn select_monitor_for_point(
    monitors: &[MonitorRect],
    point: (i32, i32),
) -> Option<MonitorRect> {
    monitor::select_monitor_for_point(monitors, point)
}

pub fn monitor_contains_point(rect: MonitorRect, point: (i32, i32)) -> bool {
    monitor::monitor_contains_point(rect, point)
}

pub fn global_to_local(point: (i32, i32), origin: (i32, i32)) -> (i32, i32) {
    (point.0 - origin.0, point.1 - origin.1)
}

pub fn monitor_local_point_for_global(
    monitors: &[MonitorRect],
    point: (i32, i32),
) -> Option<(MonitorRect, (i32, i32))> {
    let monitor =
        select_monitor_for_point(monitors, point).or_else(|| monitors.first().copied())?;
    Some((monitor, global_to_local(point, (monitor.x, monitor.y))))
}

pub fn forward_pointer_event_to_draw_input(
    draw_input: &mut DrawInputState,
    exit_dialog_state: ExitDialogState,
    tool_monitor_rect: MonitorRect,
    global_point: (i32, i32),
    event: OverlayPointerEvent,
) -> bool {
    if exit_dialog_state.blocks_drawing_input() {
        return false;
    }

    let (_, local_point) = monitor_local_point_for_global(&[tool_monitor_rect], global_point)
        .unwrap_or((
            tool_monitor_rect,
            global_to_local(global_point, (tool_monitor_rect.x, tool_monitor_rect.y)),
        ));
    match event {
        OverlayPointerEvent::LeftDown { modifiers } => {
            bridge_left_down_to_runtime(draw_input, local_point, modifiers);
        }
        OverlayPointerEvent::Move => bridge_mouse_move_to_runtime(draw_input, local_point),
        OverlayPointerEvent::LeftUp => bridge_left_up_to_runtime(draw_input, local_point),
    }
    true
}

pub fn forward_pointer_event_and_request_paint(
    draw_input: &mut DrawInputState,
    exit_dialog_state: ExitDialogState,
    tool_monitor_rect: MonitorRect,
    global_point: (i32, i32),
    event: OverlayPointerEvent,
    window: &OverlayWindow,
) -> bool {
    let handled = forward_pointer_event_to_draw_input(
        draw_input,
        exit_dialog_state,
        tool_monitor_rect,
        global_point,
        event,
    );
    if handled {
        window.request_paint();
    }
    handled
}
pub fn forward_key_event_to_draw_input(
    draw_input: &mut DrawInputState,
    exit_dialog_state: ExitDialogState,
    quick_colors: &[DrawColor],
    event: KeyEvent,
) -> KeyDispatch {
    if exit_dialog_state.blocks_drawing_input() {
        return KeyDispatch::default();
    }

    let mapped = map_key_event_to_command(true, event, None);
    if let Some(KeyCommand::SelectQuickColor(index)) = mapped {
        if let Some(color) = quick_colors.get(index).copied() {
            draw_input.apply_quick_color(map_draw_color(color));
            return KeyDispatch {
                handled: true,
                should_repaint: true,
                command: None,
            };
        }
        return KeyDispatch {
            handled: true,
            should_repaint: false,
            command: None,
        };
    }

    let command = draw_input.handle_key_command(mapped);
    crate::draw::input::route_command_to_runtime(command.clone(), ExitReason::UserRequest);
    KeyDispatch {
        handled: mapped.is_some(),
        should_repaint: command_requests_repaint(command.clone()),
        command,
    }
}

pub fn forward_key_event_and_request_paint(
    draw_input: &mut DrawInputState,
    exit_dialog_state: ExitDialogState,
    quick_colors: &[DrawColor],
    event: KeyEvent,
    window: &OverlayWindow,
) -> bool {
    let dispatch =
        forward_key_event_to_draw_input(draw_input, exit_dialog_state, quick_colors, event);
    if dispatch.should_repaint {
        window.request_paint();
    }
    dispatch.handled
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KeyDispatch {
    pub handled: bool,
    pub should_repaint: bool,
    pub command: Option<InputCommand>,
}

pub fn command_requests_repaint(command: Option<InputCommand>) -> bool {
    matches!(command, Some(InputCommand::Undo | InputCommand::Redo))
}
#[cfg(windows)]
mod platform {
    use super::{global_to_local, OverlayInputDrain, OverlayPointerEvent, OverlayPointerSample};
    use crate::draw::service::MonitorRect;
    use once_cell::sync::Lazy;
    use std::collections::{HashMap, VecDeque};
    use std::mem;
    use std::ptr;
    use std::sync::Once;
    use std::sync::{Arc, Mutex};
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{
        BOOL, COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
    };
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, EndPaint,
        EnumDisplayMonitors, GetDC, GetMonitorInfoW, ReleaseDC, SelectObject, AC_SRC_ALPHA,
        AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP,
        HDC, HGDIOBJ, MONITORINFOEXW, PAINTSTRUCT, SRCCOPY,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, GetCursorPos, GetWindowLongPtrW,
        RegisterClassW, SetWindowLongPtrW, SetWindowPos, UpdateLayeredWindow, GWLP_USERDATA,
        HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, ULW_ALPHA,
        WINDOW_EX_STYLE, WINDOW_STYLE, WM_ACTIVATE, WM_ERASEBKGND, WM_LBUTTONDOWN, WM_LBUTTONUP,
        WM_MOUSEMOVE, WM_PAINT, WM_SHOWWINDOW, WM_WINDOWPOSCHANGED, WNDCLASSW, WS_EX_LAYERED,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    };

    static POINTER_QUEUES: Lazy<Mutex<HashMap<isize, Arc<Mutex<PointerEventQueue>>>>> =
        Lazy::new(|| Mutex::new(HashMap::new()));
    static WINDOW_ORIGINS: Lazy<Mutex<HashMap<isize, (i32, i32)>>> =
        Lazy::new(|| Mutex::new(HashMap::new()));

    const POINTER_QUEUE_CAPACITY: usize = 2048;
    const POINTER_MOVE_SOFT_COALESCE_THRESHOLD: usize = 512;

    #[derive(Debug, Default)]
    struct PointerEventQueue {
        events: VecDeque<OverlayPointerSample>,
        dropped_events: u64,
        coalesced_events: u64,
    }

    impl PointerEventQueue {
        fn enqueue(&mut self, event: OverlayPointerSample) {
            if matches!(event.event, OverlayPointerEvent::Move)
                && self.events.len() >= POINTER_MOVE_SOFT_COALESCE_THRESHOLD
                && matches!(
                    self.events.back().map(|sample| sample.event),
                    Some(OverlayPointerEvent::Move)
                )
            {
                let _ = self.events.pop_back();
                self.events.push_back(event);
                self.coalesced_events = self.coalesced_events.saturating_add(1);
                return;
            }

            if self.events.len() >= POINTER_QUEUE_CAPACITY {
                self.dropped_events = self.dropped_events.saturating_add(1);
                if matches!(event.event, OverlayPointerEvent::Move) {
                    if let Some(last) = self.events.back_mut() {
                        if matches!(last.event, OverlayPointerEvent::Move) {
                            *last = event;
                            self.coalesced_events = self.coalesced_events.saturating_add(1);
                            return;
                        }
                    }
                }
                return;
            }

            self.events.push_back(event);
        }

        fn drain(&mut self) -> (Vec<OverlayPointerSample>, OverlayInputDrain) {
            let queued = self.events.len();
            let mut drained = Vec::with_capacity(queued);
            while let Some(event) = self.events.pop_front() {
                drained.push(event);
            }
            (
                drained,
                OverlayInputDrain {
                    queue_depth: queued,
                    dropped_events: std::mem::take(&mut self.dropped_events),
                    coalesced_events: std::mem::take(&mut self.coalesced_events),
                },
            )
        }
    }

    fn hwnd_key(hwnd: HWND) -> isize {
        hwnd.0 as isize
    }

    pub fn compose_overlay_window_ex_style() -> WINDOW_EX_STYLE {
        WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE
    }

    pub fn configure_layered_window_transparency(_hwnd: HWND) -> windows::core::Result<()> {
        Ok(())
    }

    fn widestring(value: &str) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        std::ffi::OsStr::new(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    pub(super) fn resolve_cursor_position() -> Option<(i32, i32)> {
        let mut point = POINT::default();
        unsafe {
            if GetCursorPos(&mut point).is_ok() {
                Some((point.x, point.y))
            } else {
                None
            }
        }
    }

    pub(super) fn enumerate_monitors() -> Vec<MonitorRect> {
        unsafe extern "system" fn enum_proc(
            monitor: windows::Win32::Graphics::Gdi::HMONITOR,
            _hdc: HDC,
            _rect: *mut RECT,
            data: LPARAM,
        ) -> BOOL {
            let monitors = unsafe { &mut *(data.0 as *mut Vec<MonitorRect>) };
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = mem::size_of::<MONITORINFOEXW>() as u32;
            if unsafe { GetMonitorInfoW(monitor, &mut info.monitorInfo as *mut _ as *mut _) }
                .as_bool()
            {
                let rc = info.monitorInfo.rcMonitor;
                monitors.push(MonitorRect {
                    x: rc.left,
                    y: rc.top,
                    width: rc.right - rc.left,
                    height: rc.bottom - rc.top,
                });
            }
            BOOL(1)
        }

        let mut monitors = Vec::new();
        unsafe {
            let _ = EnumDisplayMonitors(
                HDC::default(),
                None,
                Some(enum_proc),
                LPARAM(&mut monitors as *mut Vec<MonitorRect> as isize),
            );
        }
        monitors
    }

    unsafe extern "system" fn overlay_wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_ERASEBKGND => LRESULT(1),
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
                if !hdc.0.is_null() {
                    let mem_dc = HDC(unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut _);
                    if !mem_dc.0.is_null() {
                        let width = ps.rcPaint.right - ps.rcPaint.left;
                        let height = ps.rcPaint.bottom - ps.rcPaint.top;
                        let _ = unsafe {
                            BitBlt(
                                hdc,
                                ps.rcPaint.left,
                                ps.rcPaint.top,
                                width,
                                height,
                                mem_dc,
                                ps.rcPaint.left,
                                ps.rcPaint.top,
                                SRCCOPY,
                            )
                        };
                    }
                }
                unsafe {
                    let _ = EndPaint(hwnd, &ps);
                }
                LRESULT(0)
            }
            WM_SHOWWINDOW | WM_ACTIVATE | WM_WINDOWPOSCHANGED => {
                let _ = unsafe {
                    SetWindowPos(
                        hwnd,
                        HWND_TOPMOST,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    )
                };
                unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
            }
            WM_LBUTTONDOWN | WM_MOUSEMOVE | WM_LBUTTONUP => {
                if msg == WM_LBUTTONDOWN {
                    let _ = unsafe { SetCapture(hwnd) };
                } else if msg == WM_LBUTTONUP {
                    let _ = unsafe { ReleaseCapture() };
                }

                let local_x = (lparam.0 & 0xffff) as i16 as i32;
                let local_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
                if let (Ok(origins), Ok(queues)) = (WINDOW_ORIGINS.lock(), POINTER_QUEUES.lock()) {
                    if let (Some(origin), Some(tx)) =
                        (origins.get(&hwnd_key(hwnd)), queues.get(&hwnd_key(hwnd)))
                    {
                        let event = match msg {
                            WM_LBUTTONDOWN => OverlayPointerEvent::LeftDown {
                                modifiers: Default::default(),
                            },
                            WM_MOUSEMOVE => OverlayPointerEvent::Move,
                            WM_LBUTTONUP => OverlayPointerEvent::LeftUp,
                            _ => unreachable!(),
                        };
                        if let Ok(mut queue) = tx.lock() {
                            queue.enqueue(OverlayPointerSample {
                                global_point: (origin.0 + local_x, origin.1 + local_y),
                                event,
                            });
                        }
                    }
                }
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    #[derive(Debug)]
    pub struct OverlayWindow {
        hwnd: HWND,
        mem_dc: HDC,
        dib: HBITMAP,
        old_bitmap: HGDIOBJ,
        pub bits: *mut u8,
        size_bytes: usize,
        monitor_rect: MonitorRect,
        origin: (i32, i32),
        pointer_queue: Arc<Mutex<PointerEventQueue>>,
        present_backend: super::PresentBackend,
    }

    unsafe impl Send for OverlayWindow {}

    impl OverlayWindow {
        pub fn create_for_cursor() -> Option<Self> {
            let cursor = resolve_cursor_position()?;
            let monitors = enumerate_monitors();
            let monitor_rect = super::select_monitor_for_point(&monitors, cursor)
                .or_else(|| monitors.first().copied())?;
            Self::create_for_monitor(monitor_rect)
        }

        pub fn create_for_monitor(monitor_rect: MonitorRect) -> Option<Self> {
            static REGISTER_CLASS: Once = Once::new();
            let class_name = widestring("MultiLauncherDrawOverlay");
            let hinstance = unsafe { GetModuleHandleW(PCWSTR::null()) }.ok()?;

            REGISTER_CLASS.call_once(|| unsafe {
                let wc = WNDCLASSW {
                    hInstance: hinstance.into(),
                    lpszClassName: PCWSTR(class_name.as_ptr()),
                    lpfnWndProc: Some(overlay_wndproc),
                    ..Default::default()
                };
                let _ = RegisterClassW(&wc);
            });

            let hwnd = unsafe {
                CreateWindowExW(
                    compose_overlay_window_ex_style(),
                    PCWSTR(class_name.as_ptr()),
                    PCWSTR::null(),
                    WINDOW_STYLE(WS_POPUP.0),
                    monitor_rect.x,
                    monitor_rect.y,
                    monitor_rect.width,
                    monitor_rect.height,
                    None,
                    None,
                    hinstance,
                    None,
                )
                .ok()?
            };

            if configure_layered_window_transparency(hwnd).is_err() {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                return None;
            }

            let mem_dc = unsafe { CreateCompatibleDC(HDC::default()) };
            if mem_dc.0.is_null() {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                return None;
            }

            let mut bmi = BITMAPINFO::default();
            bmi.bmiHeader = BITMAPINFOHEADER {
                biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: monitor_rect.width,
                biHeight: -monitor_rect.height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            };

            let mut bits: *mut core::ffi::c_void = ptr::null_mut();
            let dib = unsafe {
                CreateDIBSection(
                    mem_dc,
                    &bmi,
                    DIB_RGB_COLORS,
                    &mut bits,
                    windows::Win32::Foundation::HANDLE::default(),
                    0,
                )
                .ok()?
            };
            if bits.is_null() {
                unsafe {
                    let _ = DeleteDC(mem_dc);
                    let _ = DestroyWindow(hwnd);
                }
                return None;
            }

            let old_bitmap = unsafe { SelectObject(mem_dc, dib) };
            unsafe {
                let _ = SetWindowLongPtrW(hwnd, GWLP_USERDATA, mem_dc.0 as isize);
            }

            let pointer_queue = Arc::new(Mutex::new(PointerEventQueue::default()));
            if let Ok(mut senders) = POINTER_QUEUES.lock() {
                senders.insert(hwnd_key(hwnd), Arc::clone(&pointer_queue));
            }
            if let Ok(mut origins) = WINDOW_ORIGINS.lock() {
                origins.insert(hwnd_key(hwnd), (monitor_rect.x, monitor_rect.y));
            }

            let size_bytes = (monitor_rect.width as usize)
                .saturating_mul(monitor_rect.height as usize)
                .saturating_mul(4);

            Some(Self {
                hwnd,
                mem_dc,
                dib,
                old_bitmap,
                bits: bits as *mut u8,
                size_bytes,
                monitor_rect,
                origin: (monitor_rect.x, monitor_rect.y),
                pointer_queue,
                present_backend: super::select_present_backend(),
            })
        }

        pub fn drain_pointer_events(&self) -> super::PointerDrainBatch {
            if let Ok(mut queue) = self.pointer_queue.lock() {
                let (events, diagnostics) = queue.drain();
                return super::PointerDrainBatch {
                    events,
                    diagnostics,
                };
            }
            super::PointerDrainBatch::default()
        }

        pub fn monitor_rect(&self) -> MonitorRect {
            self.monitor_rect
        }

        pub fn global_to_local(&self, point: (i32, i32)) -> (i32, i32) {
            global_to_local(point, self.origin)
        }

        fn present_cpu(&self) {
            unsafe {
                let screen_dc = GetDC(HWND::default());
                if screen_dc.0.is_null() {
                    return;
                }
                let dst = POINT {
                    x: self.monitor_rect.x,
                    y: self.monitor_rect.y,
                };
                let src = POINT { x: 0, y: 0 };
                let size = SIZE {
                    cx: self.monitor_rect.width,
                    cy: self.monitor_rect.height,
                };
                let blend = BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                };
                let _ = UpdateLayeredWindow(
                    self.hwnd,
                    screen_dc,
                    Some(&dst),
                    Some(&size),
                    self.mem_dc,
                    Some(&src),
                    COLORREF(0),
                    Some(&blend),
                    ULW_ALPHA,
                );
                let _ = ReleaseDC(HWND::default(), screen_dc);
            }
        }

        fn present_gpu(&self) {
            // Windows compositor path (UpdateLayeredWindow) performs hardware-accelerated blending
            // once pixels are uploaded to the layered surface.
            self.present_cpu();
        }

        fn present(&self) {
            match self.present_backend {
                super::PresentBackend::Cpu => self.present_cpu(),
                super::PresentBackend::Gpu => self.present_gpu(),
            }
        }

        pub fn show(&self) {
            unsafe {
                let _ = SetWindowPos(
                    self.hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
            self.present();
        }

        pub fn request_paint(&self) {
            self.present();
        }

        pub fn request_paint_rect(&self, _rect: crate::draw::render::DirtyRect) {
            self.present();
        }

        pub fn bitmap_size(&self) -> (u32, u32) {
            (
                self.monitor_rect.width as u32,
                self.monitor_rect.height as u32,
            )
        }

        pub fn with_bitmap_mut<F>(&mut self, mut f: F)
        where
            F: FnMut(&mut [u8], u32, u32),
        {
            if self.bits.is_null() || self.size_bytes == 0 {
                return;
            }

            let pixels = unsafe { std::slice::from_raw_parts_mut(self.bits, self.size_bytes) };
            f(
                pixels,
                self.monitor_rect.width as u32,
                self.monitor_rect.height as u32,
            );
        }

        pub fn shutdown(&mut self) {
            unsafe {
                if !self.mem_dc.0.is_null() {
                    let _ = SelectObject(self.mem_dc, self.old_bitmap);
                }
                if !self.dib.0.is_null() {
                    let _ = DeleteObject(self.dib);
                    self.dib = HBITMAP::default();
                }
                if !self.mem_dc.0.is_null() {
                    let _ = DeleteDC(self.mem_dc);
                    self.mem_dc = HDC::default();
                }
                if !self.hwnd.0.is_null() {
                    if let Ok(mut senders) = POINTER_QUEUES.lock() {
                        senders.remove(&hwnd_key(self.hwnd));
                    }
                    if let Ok(mut origins) = WINDOW_ORIGINS.lock() {
                        origins.remove(&hwnd_key(self.hwnd));
                    }
                    let _ = DestroyWindow(self.hwnd);
                    self.hwnd = HWND::default();
                }
                self.bits = ptr::null_mut();
                self.size_bytes = 0;
            }
        }
    }

    impl Drop for OverlayWindow {
        fn drop(&mut self) {
            self.shutdown();
        }
    }

    #[cfg(test)]
    mod windows_tests {
        use super::{compose_overlay_window_ex_style, configure_layered_window_transparency};
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            WS_EX_LAYERED, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
        };

        #[test]
        fn style_flags_include_topmost_layered_but_no_clickthrough() {
            let style = compose_overlay_window_ex_style();
            assert_ne!(style.0 & WS_EX_LAYERED.0, 0);
            assert_ne!(style.0 & WS_EX_TOPMOST.0, 0);
            assert_eq!(style.0 & WS_EX_TRANSPARENT.0, 0);
        }

        #[test]
        fn layered_window_configuration_accepts_per_pixel_alpha_pipeline() {
            let result = configure_layered_window_transparency(HWND::default());
            assert!(result.is_ok());
        }
    }
}

#[cfg(windows)]
pub use platform::OverlayWindow;

#[cfg(not(windows))]
#[derive(Debug, Default)]
pub struct OverlayWindow {
    present_backend: PresentBackend,
}

#[cfg(not(windows))]
impl OverlayWindow {
    pub fn create_for_cursor() -> Option<Self> {
        Some(Self)
    }

    pub fn create_for_monitor(_monitor_rect: MonitorRect) -> Option<Self> {
        Some(Self {
            present_backend: select_present_backend(),
        })
    }

    pub fn monitor_rect(&self) -> MonitorRect {
        MonitorRect::default()
    }

    pub fn global_to_local(&self, point: (i32, i32)) -> (i32, i32) {
        point
    }

    pub fn show(&self) {}

    pub fn request_paint(&self) {}

    pub fn request_paint_rect(&self, _rect: crate::draw::render::DirtyRect) {}

    pub fn bitmap_size(&self) -> (u32, u32) {
        (0, 0)
    }

    pub fn drain_pointer_events(&self) -> PointerDrainBatch {
        PointerDrainBatch::default()
    }

    pub fn with_bitmap_mut<F>(&mut self, mut f: F)
    where
        F: FnMut(&mut [u8], u32, u32),
    {
        let mut pixels = [];
        f(&mut pixels, 0, 0);
    }

    pub fn shutdown(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::{
        apply_toolbar_command, coalesce_pointer_events, command_requests_repaint,
        forward_key_event_to_draw_input, forward_pointer_event_to_draw_input, global_to_local,
        handle_debug_hud_toggle_hotkey_event, handle_toolbar_pointer_event,
        handle_toolbar_toggle_hotkey_event, live_render_settings, map_overlay_command_to_toolbar,
        monitor_contains_point, monitor_local_point_for_global,
        parse_debug_hud_hotkey_with_fallback, parse_toolbar_hotkey_with_fallback,
        rerender_and_repaint, select_monitor_for_point, send_exit_after_cleanup, ExitDialogState,
        OverlayPointerEvent, OverlayThreadState, OverlayWindow,
    };
    use crate::draw::keyboard_hook::{KeyCode, KeyEvent, KeyModifiers};
    use crate::draw::messages::{ExitReason, OverlayCommand, OverlayToMain, SaveResult};
    use crate::draw::{
        input::DrawInputState,
        model::{CanvasModel, Color, ObjectStyle, StrokeStyle, Tool},
        render::{BackgroundClearMode, LayeredRenderer, RenderFrameBuffer},
        service::MonitorRect,
        settings::{CanvasBackgroundMode, DrawColor, DrawSettings},
        toolbar::{ToolbarCommand, ToolbarHitTarget},
    };

    fn draw_state(tool: Tool) -> DrawInputState {
        DrawInputState::new(tool, ObjectStyle::default())
    }

    #[test]
    fn undo_redo_dispatch_requests_repaint() {
        assert!(command_requests_repaint(Some(
            crate::draw::input::InputCommand::Undo
        )));
        assert!(command_requests_repaint(Some(
            crate::draw::input::InputCommand::Redo
        )));
        assert!(!command_requests_repaint(Some(
            crate::draw::input::InputCommand::RequestExit
        )));
        assert!(!command_requests_repaint(None));
    }

    #[test]
    fn monitor_local_resolution_uses_selected_monitor_origin() {
        let monitors = [
            MonitorRect {
                x: -1920,
                y: 0,
                width: 1920,
                height: 1080,
            },
            MonitorRect {
                x: 0,
                y: 0,
                width: 2560,
                height: 1440,
            },
        ];

        let (monitor, local) =
            monitor_local_point_for_global(&monitors, (100, 100)).expect("monitor and local point");
        assert_eq!(monitor, monitors[1]);
        assert_eq!(local, (100, 100));

        let (monitor2, local2) =
            monitor_local_point_for_global(&monitors, (-100, 30)).expect("monitor and local point");
        assert_eq!(monitor2, monitors[0]);
        assert_eq!(local2, (1820, 30));
    }

    #[test]
    fn pointer_events_route_to_draw_input_state_and_commit_stroke() {
        let mut input = draw_state(Tool::Line);
        let monitor = MonitorRect {
            x: 1920,
            y: 0,
            width: 2560,
            height: 1440,
        };

        assert!(forward_pointer_event_to_draw_input(
            &mut input,
            ExitDialogState::Hidden,
            monitor,
            (2000, 200),
            OverlayPointerEvent::LeftDown {
                modifiers: Default::default()
            }
        ));
        assert!(forward_pointer_event_to_draw_input(
            &mut input,
            ExitDialogState::Hidden,
            monitor,
            (2100, 260),
            OverlayPointerEvent::Move,
        ));
        assert!(forward_pointer_event_to_draw_input(
            &mut input,
            ExitDialogState::Hidden,
            monitor,
            (2200, 300),
            OverlayPointerEvent::LeftUp,
        ));

        assert_eq!(input.history().undo_len(), 1);
        assert_eq!(input.history().canvas().objects.len(), 1);
    }

    #[test]
    fn non_hidden_dialog_blocks_key_dispatch() {
        let mut input = draw_state(Tool::Pen);
        let dispatch = forward_key_event_to_draw_input(
            &mut input,
            ExitDialogState::PromptVisible,
            &[],
            KeyEvent {
                key: KeyCode::U,
                modifiers: KeyModifiers::default(),
            },
        );
        assert!(!dispatch.handled);
        assert_eq!(dispatch.command, None);
    }

    #[test]
    fn quick_color_hotkey_updates_stroke_from_overlay_palette() {
        let mut input = draw_state(Tool::Pen);
        let quick_colors = vec![DrawColor::rgba(9, 8, 7, 255), DrawColor::rgba(1, 2, 3, 255)];

        let dispatch = forward_key_event_to_draw_input(
            &mut input,
            ExitDialogState::Hidden,
            &quick_colors,
            KeyEvent {
                key: KeyCode::Num2,
                modifiers: KeyModifiers::default(),
            },
        );

        assert!(dispatch.handled);
        assert!(dispatch.should_repaint);
        assert_eq!(dispatch.command, None);
        assert_eq!(
            input.current_style().stroke.color,
            Color::rgba(1, 2, 3, 255)
        );
    }

    #[test]
    fn quick_color_hotkey_out_of_range_is_ignored_safely() {
        let mut input = draw_state(Tool::Pen);
        let baseline = input.current_style().stroke.color;

        let dispatch = forward_key_event_to_draw_input(
            &mut input,
            ExitDialogState::Hidden,
            &[DrawColor::rgba(4, 5, 6, 255)],
            KeyEvent {
                key: KeyCode::Num8,
                modifiers: KeyModifiers::default(),
            },
        );

        assert!(dispatch.handled);
        assert!(!dispatch.should_repaint);
        assert_eq!(dispatch.command, None);
        assert_eq!(input.current_style().stroke.color, baseline);
    }

    #[test]
    fn non_hidden_dialog_blocks_pointer_events_and_prevents_commits() {
        for state in [
            ExitDialogState::PromptVisible,
            ExitDialogState::Saving,
            ExitDialogState::ErrorVisible,
        ] {
            let mut input = draw_state(Tool::Rect);
            let monitor = MonitorRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            };

            assert!(!forward_pointer_event_to_draw_input(
                &mut input,
                state,
                monitor,
                (100, 100),
                OverlayPointerEvent::LeftDown {
                    modifiers: Default::default()
                }
            ));
            assert!(!forward_pointer_event_to_draw_input(
                &mut input,
                state,
                monitor,
                (120, 120),
                OverlayPointerEvent::Move,
            ));
            assert!(!forward_pointer_event_to_draw_input(
                &mut input,
                state,
                monitor,
                (140, 140),
                OverlayPointerEvent::LeftUp,
            ));

            assert_eq!(
                input.history().undo_len(),
                0,
                "unexpected commit with state {state:?}"
            );
        }
    }

    #[test]
    fn select_monitor_by_containment() {
        let monitors = [
            MonitorRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            MonitorRect {
                x: 1920,
                y: 0,
                width: 2560,
                height: 1440,
            },
        ];

        let selected = select_monitor_for_point(&monitors, (2000, 100)).expect("monitor exists");
        assert_eq!(selected, monitors[1]);
        assert!(monitor_contains_point(monitors[0], (1919, 1079)));
        assert!(!monitor_contains_point(monitors[0], (1920, 10)));
    }

    #[test]
    fn global_to_local_uses_overlay_monitor_rect_not_launcher_rect() {
        let launcher_rect = MonitorRect {
            x: 100,
            y: 100,
            width: 800,
            height: 600,
        };
        let overlay_monitor_rect = MonitorRect {
            x: 1920,
            y: 200,
            width: 2560,
            height: 1440,
        };
        let point = (2050, 310);

        let overlay_local =
            global_to_local(point, (overlay_monitor_rect.x, overlay_monitor_rect.y));
        let launcher_local = global_to_local(point, (launcher_rect.x, launcher_rect.y));
        assert_eq!(overlay_local, (130, 110));
        assert_ne!(overlay_local, launcher_local);
    }

    #[test]
    fn exit_notification_is_emitted_only_after_cleanup_runs() {
        let (tx, rx) = std::sync::mpsc::channel();
        let cleaned = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cleaned_flag = cleaned.clone();

        send_exit_after_cleanup(
            move || {
                cleaned_flag.store(true, std::sync::atomic::Ordering::SeqCst);
            },
            &tx,
            ExitReason::UserRequest,
            SaveResult::Skipped,
        );

        let msg = rx.recv().expect("exit message should be sent");
        assert!(cleaned.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(
            msg,
            OverlayToMain::Exited {
                reason: ExitReason::UserRequest,
                save_result: SaveResult::Skipped,
            }
        );
    }

    #[test]
    fn overlay_command_maps_to_toolbar_command() {
        assert_eq!(
            map_overlay_command_to_toolbar(OverlayCommand::SetStrokeWidth(5)),
            Some(ToolbarCommand::SetStrokeWidth(5))
        );
        assert_eq!(
            map_overlay_command_to_toolbar(OverlayCommand::SelectTool(Tool::Rect)),
            Some(ToolbarCommand::SelectTool(Tool::Rect))
        );
    }

    #[test]
    fn toolbar_toggle_hotkey_flips_visibility_and_requests_repaint() {
        let mut state = OverlayThreadState::from_settings(&DrawSettings::default());
        state.toolbar_state.visible = true;
        state.toolbar_toggle_hotkey = parse_toolbar_hotkey_with_fallback("Ctrl+Shift+D");

        let toggled = handle_toolbar_toggle_hotkey_event(
            &mut state,
            KeyEvent {
                key: KeyCode::KeyD,
                modifiers: KeyModifiers {
                    ctrl: true,
                    shift: true,
                    alt: false,
                    win: false,
                },
            },
        );

        assert!(toggled);
        assert!(!state.toolbar_state.visible);
    }

    #[test]
    fn toolbar_toggle_supports_non_default_alt_hotkey() {
        let mut state = OverlayThreadState::from_settings(&DrawSettings::default());
        state.toolbar_state.visible = true;
        state.toolbar_toggle_hotkey = parse_toolbar_hotkey_with_fallback("Ctrl+Alt+1");

        let toggled = handle_toolbar_toggle_hotkey_event(
            &mut state,
            KeyEvent {
                key: KeyCode::Num1,
                modifiers: KeyModifiers {
                    ctrl: true,
                    shift: false,
                    alt: true,
                    win: false,
                },
            },
        );

        assert!(toggled);
        assert!(!state.toolbar_state.visible);
    }

    #[test]
    fn toolbar_hit_test_tool_region_updates_active_tool() {
        let mut state = OverlayThreadState::from_settings(&DrawSettings::default());
        let mut input = draw_state(Tool::Pen);
        let layout = crate::draw::toolbar::ToolbarLayout::for_state(
            (800, 600),
            &state.toolbar_state,
            state.quick_colors.len(),
        )
        .expect("toolbar layout available");
        let (_, rect) = layout.tool_rects[2]; // rect tool
        let click_point = (rect.x + 2, rect.y + 2);

        let handled = handle_toolbar_pointer_event(
            &mut input,
            &mut state,
            click_point,
            OverlayPointerEvent::LeftDown {
                modifiers: Default::default(),
            },
            (800, 600),
        );

        assert!(handled);
        assert_eq!(input.current_tool(), Tool::Rect);
    }

    #[test]
    fn toolbar_hit_test_quick_color_region_updates_style_color() {
        let mut settings = DrawSettings::default();
        settings.quick_colors = vec![
            DrawColor::rgba(17, 34, 51, 255),
            DrawColor::rgba(1, 2, 3, 255),
        ];
        let mut state = OverlayThreadState::from_settings(&settings);
        let mut input = draw_state(Tool::Pen);
        let layout = crate::draw::toolbar::ToolbarLayout::for_state(
            (800, 600),
            &state.toolbar_state,
            state.quick_colors.len(),
        )
        .expect("toolbar layout available");
        let (_, rect) = layout.quick_color_rects[0];

        let handled = handle_toolbar_pointer_event(
            &mut input,
            &mut state,
            (rect.x + 1, rect.y + 1),
            OverlayPointerEvent::LeftDown {
                modifiers: Default::default(),
            },
            (800, 600),
        );

        assert!(handled);
        let stroke = input.current_style().stroke.color;
        assert_eq!((stroke.r, stroke.g, stroke.b, stroke.a), (17, 34, 51, 255));
    }

    #[test]
    fn toolbar_command_reducer_updates_tool_color_and_width() {
        let mut state = OverlayThreadState::from_settings(&DrawSettings::default());
        let mut input = draw_state(Tool::Pen);

        apply_toolbar_command(
            &mut input,
            &mut state,
            ToolbarCommand::SelectTool(Tool::Ellipse),
        );
        apply_toolbar_command(&mut input, &mut state, ToolbarCommand::SetStrokeWidth(9));
        apply_toolbar_command(
            &mut input,
            &mut state,
            ToolbarCommand::SetColor(Color::rgba(10, 20, 30, 255)),
        );

        assert_eq!(input.current_tool(), Tool::Ellipse);
        assert_eq!(input.current_style().stroke.width, 9);
        let color = input.current_style().stroke.color;
        assert_eq!((color.r, color.g, color.b, color.a), (10, 20, 30, 255));
    }

    #[test]
    fn toolbar_tool_and_color_commands_do_not_request_full_redraw() {
        let mut state = OverlayThreadState::from_settings(&DrawSettings::default());
        let mut input = draw_state(Tool::Pen);
        assert!(input.take_full_redraw_request());

        let baseline = input.full_redraw_request_count();
        apply_toolbar_command(
            &mut input,
            &mut state,
            ToolbarCommand::SelectTool(Tool::Ellipse),
        );
        apply_toolbar_command(
            &mut input,
            &mut state,
            ToolbarCommand::SetColor(Color::rgba(10, 20, 30, 255)),
        );

        assert_eq!(input.full_redraw_request_count(), baseline);
        assert!(!input.take_full_redraw_request());
    }

    #[test]
    fn toolbar_undo_redo_commands_request_full_redraw() {
        let mut state = OverlayThreadState::from_settings(&DrawSettings::default());
        let mut input = draw_state(Tool::Pen);
        assert!(input.take_full_redraw_request());

        let _ = input.handle_left_down((0, 0), Default::default());
        input.handle_left_up((10, 10));

        let baseline = input.full_redraw_request_count();
        apply_toolbar_command(&mut input, &mut state, ToolbarCommand::Undo);
        assert!(input.take_full_redraw_request());
        assert!(input.full_redraw_request_count() > baseline);

        let after_undo = input.full_redraw_request_count();
        apply_toolbar_command(&mut input, &mut state, ToolbarCommand::Redo);
        assert!(input.take_full_redraw_request());
        assert!(input.full_redraw_request_count() > after_undo);
    }

    #[test]
    fn toolbar_renders_quick_and_fill_swatches_with_selection_outlines() {
        let mut settings = DrawSettings::default();
        settings.quick_colors = vec![
            DrawColor::rgba(200, 10, 20, 255),
            DrawColor::rgba(2, 220, 40, 255),
            DrawColor::rgba(10, 20, 200, 255),
        ];
        let state = OverlayThreadState::from_settings(&settings);
        let mut input = draw_state(Tool::Pen);
        input.set_style(ObjectStyle {
            stroke: StrokeStyle {
                width: input.current_style().stroke.width,
                color: Color::rgba(2, 220, 40, 255),
            },
            fill: Some(crate::draw::model::FillStyle {
                color: Color::rgba(200, 10, 20, 255),
            }),
        });

        let mut rgba = vec![0; 800 * 600 * 4];
        super::draw_compact_toolbar_panel(
            &mut rgba,
            (800, 600),
            &input,
            &state.quick_colors,
            &state.toolbar_state,
        );

        let layout = crate::draw::toolbar::ToolbarLayout::for_state(
            (800, 600),
            &state.toolbar_state,
            state.quick_colors.len(),
        )
        .expect("toolbar layout available");
        let quick_rect = layout.quick_color_rects[1].1;
        let fill_rect = layout.fill_color_rects[0].1;

        let quick_idx =
            (((quick_rect.y + 2) as usize * 800 + (quick_rect.x + 2) as usize) * 4) as usize;
        assert_eq!(&rgba[quick_idx..quick_idx + 3], &[2, 220, 40]);
        let quick_outline_idx =
            ((((quick_rect.y - 1) as usize) * 800 + quick_rect.x as usize) * 4) as usize;
        assert_eq!(
            &rgba[quick_outline_idx..quick_outline_idx + 3],
            &[255, 255, 255]
        );

        let fill_idx =
            (((fill_rect.y + 2) as usize * 800 + (fill_rect.x + 2) as usize) * 4) as usize;
        assert_eq!(&rgba[fill_idx..fill_idx + 3], &[200, 10, 20]);
        let fill_outline_idx =
            ((((fill_rect.y - 1) as usize) * 800 + fill_rect.x as usize) * 4) as usize;
        assert_eq!(
            &rgba[fill_outline_idx..fill_outline_idx + 3],
            &[255, 255, 255]
        );
    }

    #[test]
    fn toolbar_action_buttons_use_icons_instead_of_unique_fill_colors() {
        let settings = DrawSettings::default();
        let mut state = OverlayThreadState::from_settings(&settings);
        let input = draw_state(Tool::Pen);
        let layout = crate::draw::toolbar::ToolbarLayout::for_state(
            (800, 600),
            &state.toolbar_state,
            state.quick_colors.len(),
        )
        .expect("toolbar layout available");

        let mut rgba = vec![0; 800 * 600 * 4];
        super::draw_compact_toolbar_panel(
            &mut rgba,
            (800, 600),
            &input,
            &state.quick_colors,
            &state.toolbar_state,
        );

        let sample = |rect: crate::draw::toolbar::ToolbarRect, dx: i32, dy: i32| -> [u8; 3] {
            let idx = (((rect.y + dy) as usize * 800 + (rect.x + dx) as usize) * 4) as usize;
            [rgba[idx], rgba[idx + 1], rgba[idx + 2]]
        };

        assert_eq!(
            sample(layout.save_rect, 1, 1),
            sample(layout.exit_rect, 1, 1)
        );

        let bg = sample(layout.save_rect, 1, 1);
        let mut found_icon_pixel = false;
        for y in 0..layout.save_rect.h {
            for x in 0..layout.save_rect.w {
                if sample(layout.save_rect, x, y) != bg {
                    found_icon_pixel = true;
                    break;
                }
            }
            if found_icon_pixel {
                break;
            }
        }
        assert!(
            found_icon_pixel,
            "expected save icon pixels to alter button interior"
        );

        state.toolbar_state.hovered_target = Some(ToolbarHitTarget::Save);
        let mut hovered = vec![0; 800 * 600 * 4];
        super::draw_compact_toolbar_panel(
            &mut hovered,
            (800, 600),
            &input,
            &state.quick_colors,
            &state.toolbar_state,
        );
        let hovered_idx = (((layout.save_rect.y + 1) as usize * 800
            + (layout.save_rect.x + 1) as usize)
            * 4) as usize;
        assert_ne!(
            &hovered[hovered_idx..hovered_idx + 3],
            &rgba[hovered_idx..hovered_idx + 3],
            "hover state should alter icon button visual"
        );
    }

    #[test]
    fn update_settings_refreshes_quick_colors_and_selection_uses_new_palette() {
        let mut state = OverlayThreadState::from_settings(&DrawSettings::default());
        let mut input = draw_state(Tool::Pen);
        let mut settings = DrawSettings::default();
        settings.quick_colors = vec![
            DrawColor::rgba(200, 10, 20, 255),
            DrawColor::rgba(2, 220, 40, 255),
        ];
        state.update_from_settings(&settings);

        assert_eq!(state.quick_colors, settings.quick_colors);

        let layout = crate::draw::toolbar::ToolbarLayout::for_state(
            (800, 600),
            &state.toolbar_state,
            state.quick_colors.len(),
        )
        .expect("toolbar layout available");
        let (_, rect) = layout.quick_color_rects[1];
        assert!(handle_toolbar_pointer_event(
            &mut input,
            &mut state,
            (rect.x + 1, rect.y + 1),
            OverlayPointerEvent::LeftDown {
                modifiers: Default::default(),
            },
            (800, 600),
        ));

        let stroke = input.current_style().stroke.color;
        assert_eq!((stroke.r, stroke.g, stroke.b, stroke.a), (2, 220, 40, 255));

        let mut rgba = vec![0; 800 * 600 * 4];
        super::draw_compact_toolbar_panel(
            &mut rgba,
            (800, 600),
            &input,
            &state.quick_colors,
            &state.toolbar_state,
        );
        let sample = layout.quick_color_rects[1].1;
        let idx = (((sample.y + 2) as usize * 800 + (sample.x + 2) as usize) * 4) as usize;
        assert_eq!(&rgba[idx..idx + 3], &[2, 220, 40]);
    }

    #[test]
    fn initial_toolbar_state_respects_toolbar_collapsed_setting() {
        let mut settings = DrawSettings::default();
        settings.toolbar_collapsed = true;
        assert!(
            !OverlayThreadState::from_settings(&settings)
                .toolbar_state
                .visible
        );

        settings.toolbar_collapsed = false;
        assert!(
            OverlayThreadState::from_settings(&settings)
                .toolbar_state
                .visible
        );
    }

    #[test]
    fn diagnostics_counters_increment_on_pointer_and_key_events() {
        let mut input = draw_state(Tool::Pen);
        let monitor = MonitorRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let mut state = OverlayThreadState::from_settings(&DrawSettings::default());

        for event in [
            OverlayPointerEvent::LeftDown {
                modifiers: Default::default(),
            },
            OverlayPointerEvent::Move,
            OverlayPointerEvent::LeftUp,
        ] {
            assert!(forward_pointer_event_to_draw_input(
                &mut input,
                ExitDialogState::Hidden,
                monitor,
                (100, 100),
                event,
            ));
            state.apply_pointer_event(event);
        }

        let dispatch = forward_key_event_to_draw_input(
            &mut input,
            ExitDialogState::Hidden,
            &state.quick_colors,
            KeyEvent {
                key: KeyCode::U,
                modifiers: KeyModifiers::default(),
            },
        );
        state.apply_key_dispatch(dispatch.command);

        assert_eq!(state.diagnostics.pointer_down_count, 1);
        assert_eq!(state.diagnostics.pointer_move_count, 1);
        assert_eq!(state.diagnostics.pointer_up_count, 1);
        assert_eq!(state.diagnostics.key_event_count, 1);
        assert_eq!(state.diagnostics.undo_count, 1);
        assert_eq!(state.diagnostics.redo_count, 0);
    }

    #[test]
    fn paint_counter_increments_on_render_pass() {
        let settings = DrawSettings::default();
        let mut state = OverlayThreadState::from_settings(&settings);
        let mut input = draw_state(Tool::Pen);
        let mut window = OverlayWindow::create_for_monitor(MonitorRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        })
        .expect("overlay window test stub");
        let mut framebuffer = RenderFrameBuffer::default();
        let mut layered_renderer = LayeredRenderer::default();

        rerender_and_repaint(
            &mut window,
            &mut input,
            &settings,
            &mut state,
            &mut framebuffer,
            &mut layered_renderer,
            None,
            true,
        );
        assert_eq!(state.diagnostics.paint_count, 1);
    }

    #[test]
    fn coalesces_pointer_moves_while_preserving_last_sample() {
        let events = vec![
            super::OverlayPointerSample {
                global_point: (10, 10),
                event: OverlayPointerEvent::LeftDown {
                    modifiers: Default::default(),
                },
            },
            super::OverlayPointerSample {
                global_point: (11, 10),
                event: OverlayPointerEvent::Move,
            },
            super::OverlayPointerSample {
                global_point: (14, 10),
                event: OverlayPointerEvent::Move,
            },
            super::OverlayPointerSample {
                global_point: (18, 10),
                event: OverlayPointerEvent::Move,
            },
            super::OverlayPointerSample {
                global_point: (22, 10),
                event: OverlayPointerEvent::LeftUp,
            },
        ];

        let coalesced = coalesce_pointer_events(events, 1);
        assert_eq!(coalesced.events.len(), 3);
        assert_eq!(coalesced.events[1].event, OverlayPointerEvent::Move);
        assert_eq!(coalesced.events[1].global_point, (18, 10));
        assert_eq!(coalesced.events[2].event, OverlayPointerEvent::LeftUp);
        assert_eq!(coalesced.coalesced_count, 2);
    }

    #[test]
    fn coalescing_preserves_more_move_samples_under_normal_load() {
        let events = vec![
            super::OverlayPointerSample {
                global_point: (0, 0),
                event: OverlayPointerEvent::Move,
            },
            super::OverlayPointerSample {
                global_point: (1, 0),
                event: OverlayPointerEvent::Move,
            },
            super::OverlayPointerSample {
                global_point: (2, 0),
                event: OverlayPointerEvent::Move,
            },
            super::OverlayPointerSample {
                global_point: (3, 0),
                event: OverlayPointerEvent::Move,
            },
        ];

        let normal = coalesce_pointer_events(events.clone(), 4);
        assert_eq!(normal.events.len(), 4);

        let lag = coalesce_pointer_events(events, 1);
        assert_eq!(lag.events.len(), 1);
        assert_eq!(lag.events[0].global_point, (3, 0));
    }

    #[test]
    fn hud_toggle_controls_visibility_state() {
        let mut settings = DrawSettings::default();
        settings.debug_hud_enabled = false;
        settings.debug_hud_toggle_hotkey = "Ctrl+Shift+H".to_string();
        let mut state = OverlayThreadState::from_settings(&settings);

        assert!(!state.debug_hud_visible);
        let toggled = handle_debug_hud_toggle_hotkey_event(
            &mut state,
            KeyEvent {
                key: KeyCode::KeyH,
                modifiers: KeyModifiers {
                    ctrl: true,
                    shift: true,
                    alt: false,
                    win: false,
                },
            },
        );

        assert!(toggled);
        assert!(state.debug_hud_visible);
        let expected = parse_debug_hud_hotkey_with_fallback("Ctrl+Shift+H");
        assert_eq!(state.debug_hud_toggle_hotkey.key, expected.key);
        assert_eq!(state.debug_hud_toggle_hotkey.ctrl, expected.ctrl);
        assert_eq!(state.debug_hud_toggle_hotkey.shift, expected.shift);
        assert_eq!(state.debug_hud_toggle_hotkey.alt, expected.alt);
        assert_eq!(state.debug_hud_toggle_hotkey.win, expected.win);
    }

    #[test]
    fn live_render_clear_transparent_vs_blank() {
        let mut settings = DrawSettings::default();
        settings.canvas_background_mode = CanvasBackgroundMode::Transparent;

        let transparent = crate::draw::render::render_canvas_to_rgba(
            &CanvasModel::default(),
            live_render_settings(&settings),
            (1, 1),
        );
        assert_eq!(transparent, vec![255, 0, 255, 255]);
        assert_eq!(
            live_render_settings(&settings).clear_mode,
            BackgroundClearMode::Transparent
        );

        settings.canvas_background_mode = CanvasBackgroundMode::Solid;
        settings.canvas_solid_background_color = DrawColor::rgba(12, 34, 56, 10);
        let solid = crate::draw::render::render_canvas_to_rgba(
            &CanvasModel::default(),
            live_render_settings(&settings),
            (1, 1),
        );
        assert_eq!(solid, vec![12, 34, 56, 255]);

        assert_eq!(
            live_render_settings(&settings).clear_mode,
            BackgroundClearMode::Solid(crate::draw::model::Color::rgba(12, 34, 56, 255))
        );

        settings.canvas_solid_background_color = DrawColor::rgba(255, 0, 255, 3);
        assert_eq!(
            live_render_settings(&settings).clear_mode,
            BackgroundClearMode::Solid(crate::draw::model::Color::rgba(254, 0, 255, 255))
        );
    }
}
