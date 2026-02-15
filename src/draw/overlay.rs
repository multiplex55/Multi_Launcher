use crate::draw::input::{
    bridge_key_event_to_runtime, bridge_left_down_to_runtime, bridge_left_up_to_runtime,
    bridge_mouse_move_to_runtime, DrawInputState, InputCommand, PointerModifiers,
};
use crate::draw::keyboard_hook::{KeyEvent, KeyboardHook};
use crate::draw::messages::{ExitReason, MainToOverlay, OverlayToMain, SaveResult};
use crate::draw::model::{Color, ObjectStyle, StrokeStyle, Tool};
use crate::draw::render::{
    convert_rgba_to_dib_bgra, render_canvas_to_rgba, BackgroundClearMode, RenderSettings,
};
use crate::draw::service::MonitorRect;
use crate::draw::settings::{
    default_debug_hud_toggle_hotkey_value, default_toolbar_toggle_hotkey_value, DrawColor,
    DrawSettings, DrawTool, LiveBackgroundMode,
};
use crate::hotkey::{parse_hotkey, Hotkey, Key};
use anyhow::{anyhow, Result};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

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

#[derive(Debug, Clone, Default)]
pub struct OverlayDiagnostics {
    pub pointer_down_count: u64,
    pub pointer_move_count: u64,
    pub pointer_up_count: u64,
    pub key_event_count: u64,
    pub undo_count: u64,
    pub redo_count: u64,
    pub paint_count: u64,
    pub last_input_event_summary: String,
}

#[derive(Debug, Clone)]
struct OverlayThreadState {
    toolbar_visible: bool,
    toolbar_toggle_hotkey: Hotkey,
    debug_hud_visible: bool,
    debug_hud_toggle_hotkey: Hotkey,
    diagnostics: OverlayDiagnostics,
}

impl OverlayThreadState {
    fn from_settings(settings: &DrawSettings) -> Self {
        Self {
            toolbar_visible: !settings.toolbar_collapsed,
            toolbar_toggle_hotkey: parse_toolbar_hotkey_with_fallback(
                &settings.toolbar_toggle_hotkey,
            ),
            debug_hud_visible: settings.debug_hud_enabled,
            debug_hud_toggle_hotkey: parse_debug_hud_hotkey_with_fallback(
                &settings.debug_hud_toggle_hotkey,
            ),
            diagnostics: OverlayDiagnostics::default(),
        }
    }

    fn update_from_settings(&mut self, settings: &DrawSettings) {
        self.toolbar_toggle_hotkey =
            parse_toolbar_hotkey_with_fallback(&settings.toolbar_toggle_hotkey);
        self.debug_hud_toggle_hotkey =
            parse_debug_hud_hotkey_with_fallback(&settings.debug_hud_toggle_hotkey);
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
        && !hotkey.alt
        && !hotkey.win
}

fn handle_toolbar_toggle_hotkey_event(state: &mut OverlayThreadState, event: KeyEvent) -> bool {
    if !key_event_matches_hotkey(event, state.toolbar_toggle_hotkey) {
        return false;
    }
    state.toolbar_visible = !state.toolbar_visible;
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
    match key {
        KeyCode::U => Some(Key::KeyU),
        KeyCode::R => Some(Key::KeyR),
        KeyCode::Escape => Some(Key::Escape),
        KeyCode::D => Some(Key::KeyD),
        KeyCode::H => Some(Key::KeyH),
        KeyCode::Other => None,
    }
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

fn live_render_settings(settings: &DrawSettings) -> RenderSettings {
    let clear_mode = match settings.live_background_mode {
        LiveBackgroundMode::DesktopTransparent => BackgroundClearMode::Transparent,
        LiveBackgroundMode::SolidColor => {
            let background = settings.live_blank_color;
            BackgroundClearMode::Solid(Color::rgba(background.r, background.g, background.b, 255))
        }
    };

    RenderSettings { clear_mode }
}

fn rerender_and_repaint(
    window: &mut OverlayWindow,
    draw_input: &DrawInputState,
    settings: &DrawSettings,
    overlay_state: &mut OverlayThreadState,
) {
    let canvas = draw_input.canvas_with_active();
    let mut rgba = render_canvas_to_rgba(
        &canvas,
        live_render_settings(settings),
        window.bitmap_size(),
    );
    if overlay_state.toolbar_visible {
        draw_compact_toolbar_panel(&mut rgba, window.bitmap_size(), draw_input);
    }
    if overlay_state.debug_hud_visible {
        draw_debug_hud_panel(
            &mut rgba,
            window.bitmap_size(),
            draw_input,
            &overlay_state.diagnostics,
        );
    }
    window.with_bitmap_mut(|dib, width, height| {
        if width == 0 || height == 0 || dib.len() != rgba.len() {
            return;
        }
        convert_rgba_to_dib_bgra(&rgba, dib);
    });
    overlay_state.diagnostics.paint_count += 1;
    window.request_paint();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayPointerSample {
    pub global_point: (i32, i32),
    pub event: OverlayPointerEvent,
}

fn draw_compact_toolbar_panel(rgba: &mut [u8], size: (u32, u32), draw_input: &DrawInputState) {
    let (width, height) = size;
    if width < 160 || height < 60 {
        return;
    }

    let panel_x = 16;
    let panel_y = 16;
    fill_rect(
        rgba,
        width,
        height,
        panel_x,
        panel_y,
        220,
        46,
        [24, 24, 24, 200],
    );

    let style = draw_input.current_style();
    fill_rect(
        rgba,
        width,
        height,
        panel_x + 8,
        panel_y + 8,
        28,
        28,
        [
            style.stroke.color.r,
            style.stroke.color.g,
            style.stroke.color.b,
            255,
        ],
    );

    let width_px = draw_input.current_style().stroke.width.clamp(1, 64) as i32;
    fill_rect(
        rgba,
        width,
        height,
        panel_x + 46,
        panel_y + 18,
        width_px,
        8,
        [220, 220, 220, 255],
    );

    let tool_color = match draw_input.current_tool() {
        Tool::Pen => [80, 220, 120, 255],
        Tool::Line => [80, 170, 255, 255],
        Tool::Rect => [255, 220, 80, 255],
        Tool::Ellipse => [255, 120, 210, 255],
        Tool::Eraser => [255, 120, 120, 255],
    };
    fill_rect(
        rgba,
        width,
        height,
        panel_x + 120,
        panel_y + 8,
        18,
        18,
        tool_color,
    );

    // actionable control indicator: hotkey toggle glyph area
    fill_rect(
        rgba,
        width,
        height,
        panel_x + 190,
        panel_y + 8,
        22,
        22,
        [70, 70, 70, 255],
    );
}

fn draw_debug_hud_panel(
    rgba: &mut [u8],
    size: (u32, u32),
    draw_input: &DrawInputState,
    diagnostics: &OverlayDiagnostics,
) {
    let (width, height) = size;
    if width < 240 || height < 140 {
        return;
    }

    let panel_w = 300;
    let panel_h = 112;
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
            "KEY {} U {} R {} P {}",
            diagnostics.key_event_count,
            diagnostics.undo_count,
            diagnostics.redo_count,
            diagnostics.paint_count
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
            let mut exit_reason: Option<ExitReason> = None;
            let mut did_start = false;
            let mut window = match OverlayWindow::create_for_monitor(monitor_rect) {
                Some(window) => window,
                None => {
                    let _ = overlay_to_main_tx.send(OverlayToMain::SaveError {
                        error: "unable to initialize draw overlay window".to_string(),
                    });
                    let _ = overlay_to_main_tx.send(OverlayToMain::Exited {
                        reason: ExitReason::StartFailure,
                        save_result: SaveResult::Skipped,
                    });
                    return;
                }
            };

            let mut active_settings = crate::draw::runtime().settings_snapshot();
            let mut overlay_state = OverlayThreadState::from_settings(&active_settings);
            let mut keyboard_hook = KeyboardHook::default();
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
            loop {
                #[cfg(windows)]
                pump_overlay_messages();

                for key_event in keyboard_hook.drain_events() {
                    if handle_toolbar_toggle_hotkey_event(&mut overlay_state, key_event)
                        || handle_debug_hud_toggle_hotkey_event(&mut overlay_state, key_event)
                    {
                        rerender_and_repaint(
                            &mut window,
                            &draw_input,
                            &active_settings,
                            &mut overlay_state,
                        );
                        continue;
                    }

                    let dispatch = forward_key_event_to_draw_input(
                        &mut draw_input,
                        ExitDialogState::Hidden,
                        key_event,
                    );
                    if dispatch.handled {
                        overlay_state.apply_key_dispatch(dispatch.command);
                    }
                    if dispatch.should_repaint || overlay_state.debug_hud_visible {
                        rerender_and_repaint(
                            &mut window,
                            &draw_input,
                            &active_settings,
                            &mut overlay_state,
                        );
                    }
                }

                for pointer_event in window.drain_pointer_events() {
                    let handled = forward_pointer_event_to_draw_input(
                        &mut draw_input,
                        ExitDialogState::Hidden,
                        window.monitor_rect(),
                        pointer_event.global_point,
                        pointer_event.event,
                    );
                    if handled {
                        overlay_state.apply_pointer_event(pointer_event.event);
                        rerender_and_repaint(
                            &mut window,
                            &draw_input,
                            &active_settings,
                            &mut overlay_state,
                        );
                    }
                }

                match main_to_overlay_rx.recv_timeout(Duration::from_millis(16)) {
                    Ok(MainToOverlay::Start) => {
                        did_start = true;
                        if let Err(err) = keyboard_hook.activate() {
                            tracing::warn!(?err, "failed to activate draw keyboard hook");
                        }
                        window.show();
                        rerender_and_repaint(
                            &mut window,
                            &draw_input,
                            &active_settings,
                            &mut overlay_state,
                        );
                    }
                    Ok(MainToOverlay::UpdateSettings) => {
                        active_settings = crate::draw::runtime().settings_snapshot();
                        overlay_state.update_from_settings(&active_settings);
                        draw_input.set_tool(map_draw_tool(active_settings.last_tool));
                        draw_input.set_style(ObjectStyle {
                            stroke: StrokeStyle {
                                width: active_settings.last_width.max(1),
                                color: map_draw_color(active_settings.last_color),
                            },
                            fill: None,
                        });
                        rerender_and_repaint(
                            &mut window,
                            &draw_input,
                            &active_settings,
                            &mut overlay_state,
                        );
                        let _ = overlay_to_main_tx.send(OverlayToMain::SaveProgress {
                            canvas: draw_input.history().canvas(),
                        });
                    }
                    Ok(MainToOverlay::RequestExit { reason }) => {
                        exit_reason = Some(reason);
                        break;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        break;
                    }
                }
            }

            if !did_start {
                let _ = overlay_to_main_tx.send(OverlayToMain::SaveError {
                    error: "overlay exited before start command".to_string(),
                });
            }
            send_exit_after_cleanup(
                || {
                    keyboard_hook.deactivate();
                    window.shutdown();
                },
                &overlay_to_main_tx,
                exit_reason.unwrap_or(ExitReason::OverlayFailure),
                SaveResult::Skipped,
            );
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

pub fn monitor_contains_point(rect: MonitorRect, point: (i32, i32)) -> bool {
    point.0 >= rect.x
        && point.0 < rect.x + rect.width
        && point.1 >= rect.y
        && point.1 < rect.y + rect.height
}

pub fn resolve_monitor_from_cursor() -> Option<MonitorRect> {
    #[cfg(windows)]
    {
        let monitors = platform::enumerate_monitors();
        let cursor = platform::resolve_cursor_position()?;
        return select_monitor_for_point(&monitors, cursor).or_else(|| monitors.first().copied());
    }

    #[cfg(not(windows))]
    {
        None
    }
}

pub fn select_monitor_for_point(
    monitors: &[MonitorRect],
    point: (i32, i32),
) -> Option<MonitorRect> {
    monitors
        .iter()
        .copied()
        .find(|rect| monitor_contains_point(*rect, point))
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
    event: KeyEvent,
) -> KeyDispatch {
    if exit_dialog_state.blocks_drawing_input() {
        return KeyDispatch::default();
    }

    let command = bridge_key_event_to_runtime(draw_input, event);
    KeyDispatch {
        handled: true,
        should_repaint: command_requests_repaint(command.clone()),
        command,
    }
}

pub fn forward_key_event_and_request_paint(
    draw_input: &mut DrawInputState,
    exit_dialog_state: ExitDialogState,
    event: KeyEvent,
    window: &OverlayWindow,
) -> bool {
    let dispatch = forward_key_event_to_draw_input(draw_input, exit_dialog_state, event);
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
    use super::{global_to_local, OverlayPointerEvent, OverlayPointerSample};
    use crate::draw::model::FIRST_PASS_TRANSPARENCY_COLORKEY;
    use crate::draw::service::MonitorRect;
    use once_cell::sync::Lazy;
    use std::collections::HashMap;
    use std::mem;
    use std::ptr;
    use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
    use std::sync::Mutex;
    use std::sync::Once;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{BOOL, COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, EndPaint,
        EnumDisplayMonitors, GetMonitorInfoW, InvalidateRect, SelectObject, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ, MONITORINFOEXW,
        PAINTSTRUCT, SRCCOPY,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, GetCursorPos, GetWindowLongPtrW,
        RegisterClassW, SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, GWLP_USERDATA,
        HWND_TOPMOST, LWA_COLORKEY, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
        WINDOW_EX_STYLE, WINDOW_STYLE, WM_ACTIVATE, WM_ERASEBKGND, WM_LBUTTONDOWN, WM_LBUTTONUP,
        WM_MOUSEMOVE, WM_PAINT, WM_SHOWWINDOW, WM_WINDOWPOSCHANGED, WNDCLASSW, WS_EX_LAYERED,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    };

    static POINTER_SENDERS: Lazy<Mutex<HashMap<isize, Sender<OverlayPointerSample>>>> =
        Lazy::new(|| Mutex::new(HashMap::new()));
    static WINDOW_ORIGINS: Lazy<Mutex<HashMap<isize, (i32, i32)>>> =
        Lazy::new(|| Mutex::new(HashMap::new()));

    fn hwnd_key(hwnd: HWND) -> isize {
        hwnd.0 as isize
    }

    pub fn compose_overlay_window_ex_style() -> WINDOW_EX_STYLE {
        WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE
    }

    pub enum OverlayTransparencyMode {
        ColorKeyFirstPass,
    }

    pub fn first_pass_transparency_colorkey() -> COLORREF {
        COLORREF(
            (FIRST_PASS_TRANSPARENCY_COLORKEY.r as u32)
                | ((FIRST_PASS_TRANSPARENCY_COLORKEY.g as u32) << 8)
                | ((FIRST_PASS_TRANSPARENCY_COLORKEY.b as u32) << 16),
        )
    }

    pub fn configure_layered_window_transparency(
        hwnd: HWND,
        mode: OverlayTransparencyMode,
    ) -> windows::core::Result<()> {
        // Technical debt: this first pass uses colorkey transparency, which cannot
        // represent per-pixel alpha. Keep this seam so we can swap to
        // `UpdateLayeredWindow` without changing overlay window creation call sites.
        let (key, alpha, flags) = match mode {
            OverlayTransparencyMode::ColorKeyFirstPass => {
                (first_pass_transparency_colorkey(), 0, LWA_COLORKEY)
            }
        };
        unsafe { SetLayeredWindowAttributes(hwnd, key, alpha, flags) }
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
                if let (Ok(origins), Ok(senders)) = (WINDOW_ORIGINS.lock(), POINTER_SENDERS.lock())
                {
                    if let (Some(origin), Some(tx)) =
                        (origins.get(&hwnd_key(hwnd)), senders.get(&hwnd_key(hwnd)))
                    {
                        let event = match msg {
                            WM_LBUTTONDOWN => OverlayPointerEvent::LeftDown {
                                modifiers: Default::default(),
                            },
                            WM_MOUSEMOVE => OverlayPointerEvent::Move,
                            WM_LBUTTONUP => OverlayPointerEvent::LeftUp,
                            _ => unreachable!(),
                        };
                        let _ = tx.send(OverlayPointerSample {
                            global_point: (origin.0 + local_x, origin.1 + local_y),
                            event,
                        });
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
        pointer_rx: Receiver<OverlayPointerSample>,
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

            if configure_layered_window_transparency(
                hwnd,
                OverlayTransparencyMode::ColorKeyFirstPass,
            )
            .is_err()
            {
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

            let (pointer_tx, pointer_rx) = channel::<OverlayPointerSample>();
            if let Ok(mut senders) = POINTER_SENDERS.lock() {
                senders.insert(hwnd_key(hwnd), pointer_tx);
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
                pointer_rx,
            })
        }

        pub fn drain_pointer_events(&self) -> Vec<OverlayPointerSample> {
            let mut events = Vec::new();
            loop {
                match self.pointer_rx.try_recv() {
                    Ok(event) => events.push(event),
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
                }
            }
            events
        }

        pub fn monitor_rect(&self) -> MonitorRect {
            self.monitor_rect
        }

        pub fn global_to_local(&self, point: (i32, i32)) -> (i32, i32) {
            global_to_local(point, self.origin)
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
        }

        pub fn request_paint(&self) {
            unsafe {
                let _ = InvalidateRect(self.hwnd, None, false);
            }
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
                    if let Ok(mut senders) = POINTER_SENDERS.lock() {
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
        use super::{
            compose_overlay_window_ex_style, configure_layered_window_transparency,
            first_pass_transparency_colorkey, OverlayTransparencyMode,
        };
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            LWA_COLORKEY, WS_EX_LAYERED, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
        };

        #[test]
        fn style_flags_include_topmost_layered_but_no_clickthrough() {
            let style = compose_overlay_window_ex_style();
            assert_ne!(style.0 & WS_EX_LAYERED.0, 0);
            assert_ne!(style.0 & WS_EX_TOPMOST.0, 0);
            assert_eq!(style.0 & WS_EX_TRANSPARENT.0, 0);
        }

        #[test]
        fn colorkey_mode_is_wired_for_first_pass_transparency() {
            assert_eq!(first_pass_transparency_colorkey().0, 0x00ff00ff);

            let result = configure_layered_window_transparency(
                HWND::default(),
                OverlayTransparencyMode::ColorKeyFirstPass,
            );
            assert!(result.is_err());
            assert_eq!(LWA_COLORKEY.0, 0x1);
        }
    }
}

#[cfg(windows)]
pub use platform::OverlayWindow;

#[cfg(not(windows))]
#[derive(Debug, Default)]
pub struct OverlayWindow;

#[cfg(not(windows))]
impl OverlayWindow {
    pub fn create_for_cursor() -> Option<Self> {
        Some(Self)
    }

    pub fn create_for_monitor(_monitor_rect: MonitorRect) -> Option<Self> {
        Some(Self)
    }

    pub fn monitor_rect(&self) -> MonitorRect {
        MonitorRect::default()
    }

    pub fn global_to_local(&self, point: (i32, i32)) -> (i32, i32) {
        point
    }

    pub fn show(&self) {}

    pub fn request_paint(&self) {}

    pub fn bitmap_size(&self) -> (u32, u32) {
        (0, 0)
    }

    pub fn drain_pointer_events(&self) -> Vec<OverlayPointerSample> {
        Vec::new()
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
        command_requests_repaint, forward_key_event_to_draw_input,
        forward_pointer_event_to_draw_input, global_to_local, handle_debug_hud_toggle_hotkey_event,
        handle_toolbar_toggle_hotkey_event, live_render_settings, monitor_contains_point,
        monitor_local_point_for_global, parse_debug_hud_hotkey_with_fallback,
        parse_toolbar_hotkey_with_fallback, rerender_and_repaint, select_monitor_for_point,
        send_exit_after_cleanup, ExitDialogState, OverlayPointerEvent, OverlayThreadState,
    };
    use crate::draw::keyboard_hook::{KeyCode, KeyEvent, KeyModifiers};
    use crate::draw::messages::{ExitReason, OverlayToMain, SaveResult};
    use crate::draw::{
        input::DrawInputState,
        model::{CanvasModel, ObjectStyle, Tool},
        render::BackgroundClearMode,
        service::MonitorRect,
        settings::{DrawColor, DrawSettings, LiveBackgroundMode},
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
    fn toolbar_toggle_hotkey_flips_visibility_and_requests_repaint() {
        let mut state = OverlayThreadState::from_settings(&DrawSettings::default());
        state.toolbar_visible = true;
        state.toolbar_toggle_hotkey = parse_toolbar_hotkey_with_fallback("Ctrl+Shift+D");

        let toggled = handle_toolbar_toggle_hotkey_event(
            &mut state,
            KeyEvent {
                key: KeyCode::D,
                modifiers: KeyModifiers {
                    ctrl: true,
                    shift: true,
                },
            },
        );

        assert!(toggled);
        assert!(!state.toolbar_visible);
    }

    #[test]
    fn initial_toolbar_state_respects_toolbar_collapsed_setting() {
        let mut settings = DrawSettings::default();
        settings.toolbar_collapsed = true;
        assert!(!OverlayThreadState::from_settings(&settings).toolbar_visible);

        settings.toolbar_collapsed = false;
        assert!(OverlayThreadState::from_settings(&settings).toolbar_visible);
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

        rerender_and_repaint(&mut window, &mut input, &settings, &mut state);
        assert_eq!(state.diagnostics.paint_count, 1);
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
                key: KeyCode::H,
                modifiers: KeyModifiers {
                    ctrl: true,
                    shift: true,
                },
            },
        );

        assert!(toggled);
        assert!(state.debug_hud_visible);
        assert_eq!(
            state.debug_hud_toggle_hotkey,
            parse_debug_hud_hotkey_with_fallback("Ctrl+Shift+H")
        );
    }

    #[test]
    fn live_render_clear_transparent_vs_solid() {
        let mut settings = DrawSettings::default();
        settings.live_background_mode = LiveBackgroundMode::DesktopTransparent;
        settings.live_blank_color = DrawColor::rgba(12, 34, 56, 10);

        let transparent = crate::draw::render::render_canvas_to_rgba(
            &CanvasModel::default(),
            live_render_settings(&settings),
            (1, 1),
        );
        assert_eq!(transparent, vec![0, 0, 0, 0]);

        settings.live_background_mode = LiveBackgroundMode::SolidColor;
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
    }
}
