//! Interactive integration fixture for the production cursor-effect runtime.
//! Run with `cargo run --bin coordinate_tool_smoke -- --cursor-effects`.

use std::cell::RefCell;
use std::ffi::c_void;
use std::fs::{File, create_dir_all};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use multi_launcher::coordinate_tool::{
    CoordinateToolController, CoordinateToolPreferences, ZoomMode,
};
use windows::Win32::Foundation::{
    BOOL, COLORREF, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, BitBlt, CAPTUREBLT, CreateCompatibleDC,
    CreateDIBSection, CreateSolidBrush, DeleteDC, DeleteObject, EndPaint, FillRect, GdiFlush,
    GetDC, HBITMAP, HDC, HGDIOBJ, PAINTSTRUCT, ROP_CODE, ReleaseDC, SRCCOPY, SelectObject,
    SetBkMode, SetTextColor, TRANSPARENT, TextOutW,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Magnification::{
    MAGCOLOREFFECT, MAGTRANSFORM, MW_FILTERMODE, MagGetColorEffect, MagGetWindowFilterList,
    MagGetWindowSource, MagGetWindowTransform,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    EnumChildWindows, EnumWindows, GWL_EXSTYLE, GWL_STYLE, GWLP_USERDATA, GetClassNameW,
    GetCursorPos, GetMessageW, GetSystemMetrics, GetWindowLongPtrW, GetWindowRect, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible, MSG, RegisterClassW, SM_CXVIRTUALSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_SHOW, SetTimer, SetWindowLongPtrW,
    ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WM_CLOSE, WM_DESTROY, WM_KEYDOWN, WM_NCCREATE,
    WM_NCDESTROY, WM_PAINT, WM_QUIT, WM_TIMER, WNDCLASSW, WS_OVERLAPPEDWINDOW,
};
use windows::core::w;

use super::{CountingFactory, Counts, DpiScope};

const SCENE_CLASS: &str = "MultiLauncherCursorEffectsSmokeScene";
const EFFECT_HOST_CLASS: &str = "MultiLauncherCoordinateEffectPassiveHost";
const COORDINATE_SURFACE_CLASS: &str = "MultiLauncherCoordinatePassiveSurface";
const TIMER_ID: usize = 1;
const TIMER_MS: u32 = 350;
const READBACK_WIDTH: i32 = 480;
const READBACK_HEIGHT: i32 = 520;
const AUTO_STAGE_HOLD: Duration = Duration::from_millis(1_400);
const AUTO_STAGES: [&str; 15] = [
    "hud-only",
    "halo-0-percent",
    "halo-40-percent",
    "halo-100-percent",
    "zoom-2x-offset",
    "zoom-2x-centered",
    "all-four-default-guides",
    "all-four-stationary-repeat",
    "all-four-1_7x-163px-offset",
    "effects-off-hud-retained",
    "repeat-halo-zoom-enable",
    "repeat-halo-zoom-disable",
    "guides-visible-effects-off",
    "recreated-effects-after-guides",
    "effects-off-after-guides",
];

pub(super) fn run(auto: bool) -> Result<(), String> {
    let _dpi = DpiScope::enter()?;
    let output_dir = std::env::current_dir()
        .map_err(|error| format!("get current directory: {error}"))?
        .join("target")
        .join("coordinate-tool-smoke");
    create_dir_all(&output_dir).map_err(|error| {
        format!(
            "create smoke output directory {}: {error}",
            output_dir.display()
        )
    })?;
    let log_path = output_dir.join("cursor-effects.log");
    let log = File::create(&log_path)
        .map_err(|error| format!("create smoke log {}: {error}", log_path.display()))?;

    let counts = Arc::new(Counts::default());
    let app = Box::new(RefCell::new(App {
        controller: CoordinateToolController::new(Arc::new(CountingFactory(Arc::clone(&counts)))),
        preferences: CoordinateToolPreferences::default().normalized(),
        counts: Arc::clone(&counts),
        output_dir,
        log,
        marker: 0,
        scene: HWND::default(),
        auto,
        auto_stage: None,
        auto_transition_at: None,
    }));
    register_scene_class()?;
    let scene = create_scene_window(&app)?;
    app.borrow_mut().scene = scene;
    unsafe {
        if SetTimer(scene, TIMER_ID, TIMER_MS, None) == 0 {
            let _ = DestroyWindow(scene);
            return Err("SetTimer for the animated marker failed".into());
        }
        let _ = ShowWindow(scene, SW_SHOW);
    }
    if let Err(error) = app.borrow_mut().controller.set_hud_enabled(true) {
        unsafe {
            let _ = DestroyWindow(scene);
        }
        return Err(error);
    }
    let output_path = app.borrow().output_dir.display().to_string();
    app.borrow_mut().log(format!(
        "READY scene=0x{:x} pid={} log={} output={}",
        scene.0 as usize,
        std::process::id(),
        log_path.display(),
        output_path
    ));
    app.borrow_mut().log(
        "This fixture uses the production passive controller; BitBlt readback may omit magnifier composition, so API success alone is not proof.",
    );
    app.borrow_mut().log(
        "Controls: F1 HUD, F2 crosshair, F3 halo, F4 zoom, F5 effects off (HUD remains), F6 inversion 0/40/100%, F7 centered/offset zoom, F8 factor cycle (1.25/1.7/2/4), F9 diameter 160/163, F10 guides, F11 status/HWND diagnostics, F12 bounded BMP readback, Escape exit.",
    );
    app.borrow_mut()
        .log("Initial state: HUD on; crosshair, halo, zoom off.");
    if auto {
        app.borrow_mut().start_auto_sequence()?;
    }

    let mut message = MSG::default();
    loop {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 <= 0 || message.message == WM_QUIT {
            break;
        }
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    let shutdown_result = app.borrow_mut().controller.shutdown();
    let leftovers = enumerate_owned_windows()?;
    let summary = {
        let app = app.borrow();
        format!(
            "SHUTDOWN result={shutdown_result:?} remaining_owned_windows={} sampler_creations={} backend_creations={} samples={} renders={} backend_shutdowns={}",
            leftovers.len(),
            app.counts.sampler_creations.load(Ordering::Acquire),
            app.counts.backend_creations.load(Ordering::Acquire),
            app.counts.samples.load(Ordering::Acquire),
            app.counts.renders.load(Ordering::Acquire),
            app.counts.shutdowns.load(Ordering::Acquire)
        )
    };
    let mut app = app.borrow_mut();
    app.log(summary);
    if !leftovers.is_empty() {
        app.log(format!("CLEANUP WARNING remaining windows: {leftovers:?}"));
    }
    shutdown_result?;
    if !leftovers.is_empty() {
        return Err(format!(
            "{} fixture-owned passive windows remained after shutdown",
            leftovers.len()
        ));
    }
    Ok(())
}

struct App {
    controller: CoordinateToolController,
    preferences: CoordinateToolPreferences,
    counts: Arc<Counts>,
    output_dir: PathBuf,
    log: File,
    marker: usize,
    scene: HWND,
    auto: bool,
    auto_stage: Option<usize>,
    auto_transition_at: Option<Instant>,
}

impl App {
    fn log(&mut self, message: impl AsRef<str>) {
        let line = format!("cursor-effects-smoke: {}", message.as_ref());
        println!("{line}");
        let _ = std::io::stdout().flush();
        let _ = writeln!(self.log, "{line}");
        let _ = self.log.flush();
    }

    fn handle_key(&mut self, hwnd: HWND, key: usize) -> bool {
        if self.auto && key != 0x1b {
            return true;
        }
        let result = match key {
            0x70 => {
                let enabled = !self.controller.runtime_state().hud_enabled();
                self.controller
                    .set_hud_enabled(enabled)
                    .map(|_| format!("HUD {}", on_off(enabled)))
            }
            0x71 => {
                let enabled = !self.controller.runtime_state().crosshair_enabled();
                self.controller
                    .set_crosshair_enabled(enabled)
                    .map(|_| format!("crosshair {}", on_off(enabled)))
            }
            0x72 => {
                let enabled = !self.controller.runtime_state().halo_enabled();
                self.controller
                    .set_halo_enabled(enabled)
                    .map(|_| format!("halo {}", on_off(enabled)))
            }
            0x73 => {
                let enabled = !self.controller.runtime_state().zoom_enabled();
                self.controller
                    .set_zoom_enabled(enabled)
                    .map(|_| format!("zoom {}", on_off(enabled)))
            }
            0x74 => self.controller.disable_effects().map(|_| {
                format!(
                    "effects off; HUD remains {}",
                    on_off(self.controller.runtime_state().hud_enabled())
                )
            }),
            0x75 => self.cycle_halo_strength(),
            0x76 => self.toggle_zoom_mode(),
            0x77 => self.cycle_zoom_factor(),
            0x78 => self.toggle_zoom_diameter(),
            0x79 => self.toggle_guides(),
            0x7a => self.dump_diagnostics(),
            0x7b => self.save_readback("manual"),
            0x1b => {
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                        hwnd,
                        WM_CLOSE,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }
                return true;
            }
            _ => return false,
        };
        match result {
            Ok(description) => {
                self.log(description);
                if !unsafe { windows::Win32::Graphics::Gdi::InvalidateRect(hwnd, None, BOOL(1)) }
                    .as_bool()
                {
                    let error = "InvalidateRect failed after control update";
                    self.log(format!("scene redraw request failed: {error}"));
                }
            }
            Err(error) => self.log(format!("control failed: {error}")),
        }
        true
    }

    fn cycle_halo_strength(&mut self) -> Result<String, String> {
        let strength = self.preferences.halo.inversion_strength;
        self.preferences.halo.inversion_strength = if strength < 0.2 {
            0.4
        } else if strength < 0.7 {
            1.0
        } else {
            0.0
        };
        self.publish_preferences()?;
        Ok(format!(
            "halo inversion strength {:.0}% (saved preference in this fixture only)",
            self.preferences.halo.inversion_strength * 100.0
        ))
    }

    fn toggle_zoom_mode(&mut self) -> Result<String, String> {
        self.preferences.zoom.mode = match self.preferences.zoom.mode {
            ZoomMode::Offset => ZoomMode::Centered,
            ZoomMode::Centered => ZoomMode::Offset,
        };
        self.publish_preferences()?;
        Ok(format!(
            "zoom destination mode {:?}; source remains anchored at live cursor",
            self.preferences.zoom.mode
        ))
    }

    fn cycle_zoom_factor(&mut self) -> Result<String, String> {
        let next = match self.preferences.zoom.zoom_factor {
            value if value < 1.5 => 1.7,
            value if value < 1.9 => 2.0,
            value if value < 3.0 => 4.0,
            _ => 1.25,
        };
        self.preferences.zoom.zoom_factor = next;
        self.publish_preferences()?;
        Ok(format!("zoom factor set to {next:.2}×"))
    }

    fn toggle_zoom_diameter(&mut self) -> Result<String, String> {
        self.preferences.zoom.diameter = if self.preferences.zoom.diameter == 160 {
            163
        } else {
            160
        };
        self.publish_preferences()?;
        Ok(format!(
            "zoom lens diameter set to {} physical pixels",
            self.preferences.zoom.diameter
        ))
    }

    fn toggle_guides(&mut self) -> Result<String, String> {
        self.preferences.crosshair.virtual_desktop_guides =
            !self.preferences.crosshair.virtual_desktop_guides;
        self.publish_preferences()?;
        Ok(format!(
            "virtual desktop guides {} (visible when crosshair is enabled)",
            on_off(self.preferences.crosshair.virtual_desktop_guides)
        ))
    }

    fn publish_preferences(&mut self) -> Result<(), String> {
        self.controller
            .set_preferences(self.preferences.clone().normalized())
    }

    fn dump_diagnostics(&mut self) -> Result<String, String> {
        self.log(format!(
            "DIAGNOSTIC_STAGE name={}",
            self.auto_stage
                .map(|index| AUTO_STAGES[index])
                .unwrap_or("interactive")
        ));
        let state = self.controller.runtime_state();
        self.log(format!(
            "STATE hud={} crosshair={} halo={} zoom={} effects_status={:?} preferences={:?}",
            state.hud_enabled(),
            state.crosshair_enabled(),
            state.halo_enabled(),
            state.zoom_enabled(),
            self.controller.effects_status(),
            self.preferences
        ));
        match enumerate_owned_windows() {
            Ok(windows) => {
                self.log(format!("OWNED_WINDOWS count={}", windows.len()));
                for window in windows {
                    self.log(format!(
                        "HWND hwnd=0x{:x} class={} title={:?} visible={} rect={:?} style=0x{:x} exstyle=0x{:x}",
                        window.hwnd.0 as usize,
                        window.class,
                        window.title,
                        window.visible,
                        window.rect,
                        window.style,
                        window.ex_style
                    ));
                    for child in window.children {
                        self.log(format!(
                            "MAG_CHILD hwnd=0x{:x} class={} visible={} rect={:?}",
                            child.hwnd.0 as usize, child.class, child.visible, child.rect
                        ));
                        if child.class.eq_ignore_ascii_case("Magnifier") {
                            self.log(magnifier_details(child.hwnd));
                        }
                    }
                }
            }
            Err(error) => self.log(format!("HWND_DIAGNOSTICS_FAILED {error}")),
        }
        let mut point = POINT::default();
        if let Err(error) = unsafe { GetCursorPos(&mut point) } {
            self.log(format!("CURSOR_QUERY_FAILED {error}"));
        } else {
            self.log(format!(
                "CURSOR physical=({}, {}) foreground=0x{:x}",
                point.x,
                point.y,
                unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() }.0
                    as usize
            ));
        }
        self.log(format!(
            "COUNTS sampler_creations={} backend_creations={} samples={} renders={} backend_shutdowns={}",
            self.counts.sampler_creations.load(Ordering::Acquire),
            self.counts.backend_creations.load(Ordering::Acquire),
            self.counts.samples.load(Ordering::Acquire),
            self.counts.renders.load(Ordering::Acquire),
            self.counts.shutdowns.load(Ordering::Acquire)
        ));
        Ok("native diagnostics written to the fixture log".into())
    }

    fn save_readback(&mut self, stage: &str) -> Result<String, String> {
        let mut cursor = POINT::default();
        unsafe { GetCursorPos(&mut cursor) }.map_err(|error| format!("GetCursorPos: {error}"))?;
        let rect = RECT {
            left: cursor.x - READBACK_WIDTH / 2,
            top: cursor.y - READBACK_HEIGHT / 2,
            right: cursor.x - READBACK_WIDTH / 2 + READBACK_WIDTH,
            bottom: cursor.y - READBACK_HEIGHT / 2 + READBACK_HEIGHT,
        };
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();
        let path = self
            .output_dir
            .join(format!("desktop-readback-{stage}-{stamp}.bmp"));
        match save_desktop_bmp(&path, rect) {
            Ok(samples) => {
                self.log(format!(
                    "READBACK path={} rect=({},{}..{}, {}) cursor_rgb={:?}; BitBlt(SRCCOPY|CAPTUREBLT) may omit WC_MAGNIFIER composition; an included real rendered image can support review, while API success alone cannot.",
                    path.display(),
                    rect.left,
                    rect.top,
                    rect.right,
                    rect.bottom,
                    samples
                ));
                Ok(format!("readback saved to {}", path.display()))
            }
            Err(error) => {
                self.log(format!("READBACK failed: {error}"));
                Err(error)
            }
        }
    }

    fn start_auto_sequence(&mut self) -> Result<(), String> {
        self.auto_stage = Some(0);
        self.auto_transition_at = Some(Instant::now());
        self.apply_auto_stage(0)
    }

    fn advance_auto_sequence(&mut self, hwnd: HWND) {
        if !self.auto {
            return;
        }
        let Some(stage) = self.auto_stage else {
            return;
        };
        if self
            .auto_transition_at
            .is_none_or(|last| last.elapsed() < AUTO_STAGE_HOLD)
        {
            return;
        }
        let _ = self.dump_diagnostics();
        if matches!(stage, 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 13) {
            let _ = self.save_readback(AUTO_STAGES[stage]);
        }
        let next = stage + 1;
        if next >= AUTO_STAGES.len() {
            self.log(format!(
                "AUTO complete foreground_before_close=0x{:x}; posting scene close",
                foreground_hwnd()
            ));
            let _ = unsafe {
                windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    hwnd,
                    WM_CLOSE,
                    WPARAM(0),
                    LPARAM(0),
                )
            };
            self.auto = false;
            return;
        }
        self.auto_stage = Some(next);
        self.auto_transition_at = Some(Instant::now());
        if let Err(error) = self.apply_auto_stage(next) {
            self.log(format!(
                "AUTO stage={} failed: {error}; posting scene close",
                AUTO_STAGES[next]
            ));
            let _ = unsafe {
                windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    hwnd,
                    WM_CLOSE,
                    WPARAM(0),
                    LPARAM(0),
                )
            };
            self.auto = false;
        }
    }

    fn apply_auto_stage(&mut self, stage: usize) -> Result<(), String> {
        let name = AUTO_STAGES[stage];
        let foreground_before = foreground_hwnd();
        self.log(format!(
            "AUTO stage={name} begin foreground_before=0x{foreground_before:x}"
        ));
        let mut modes = (false, false, false, false);
        match stage {
            0 => modes.0 = true,
            1..=3 => {
                self.preferences.halo.inversion_strength = match stage {
                    1 => 0.0,
                    2 => 0.4,
                    _ => 1.0,
                };
                modes.2 = true;
            }
            4 | 5 => {
                self.preferences.zoom.mode = if stage == 4 {
                    ZoomMode::Offset
                } else {
                    ZoomMode::Centered
                };
                self.preferences.zoom.zoom_factor = 2.0;
                self.preferences.zoom.diameter = 160;
                modes.3 = true;
            }
            6 | 7 => {
                self.preferences.halo.inversion_strength = 0.4;
                self.preferences.zoom.mode = ZoomMode::Offset;
                self.preferences.zoom.zoom_factor = 2.0;
                self.preferences.zoom.diameter = 160;
                self.preferences.crosshair.virtual_desktop_guides = true;
                modes = (true, true, true, true);
            }
            8 => {
                self.preferences.halo.inversion_strength = 0.4;
                self.preferences.zoom.mode = ZoomMode::Offset;
                self.preferences.zoom.zoom_factor = 1.7;
                self.preferences.zoom.diameter = 163;
                self.preferences.crosshair.virtual_desktop_guides = true;
                modes = (true, true, true, true);
            }
            9 => {
                modes.0 = true;
                self.controller.set_hud_enabled(true)?;
                self.controller.disable_effects()?;
            }
            10 => {
                self.preferences.halo.inversion_strength = 0.4;
                self.preferences.zoom.mode = ZoomMode::Offset;
                self.preferences.zoom.zoom_factor = 2.0;
                self.preferences.zoom.diameter = 160;
                modes = (true, false, true, true);
            }
            11 => {
                modes.0 = true;
                self.controller.set_hud_enabled(true)?;
                self.controller.disable_effects()?;
            }
            12 => {
                self.preferences.crosshair.virtual_desktop_guides = true;
                modes = (true, true, false, false);
            }
            13 => {
                self.preferences.halo.inversion_strength = 0.4;
                self.preferences.zoom.mode = ZoomMode::Offset;
                self.preferences.zoom.zoom_factor = 2.0;
                self.preferences.zoom.diameter = 160;
                self.preferences.crosshair.virtual_desktop_guides = true;
                modes = (true, true, true, true);
            }
            14 => {
                modes.0 = true;
                self.controller.set_hud_enabled(true)?;
                self.controller.disable_effects()?;
            }
            _ => return Err(format!("invalid auto stage index {stage}")),
        }
        self.publish_preferences()?;
        if stage != 9 && stage != 11 && stage != 14 {
            self.apply_modes(modes)?;
        }
        let foreground_after = foreground_hwnd();
        self.log(format!(
            "AUTO stage={name} applied foreground_after=0x{foreground_after:x} requested_modes={modes:?} preferences={:?}",
            self.preferences
        ));
        Ok(())
    }

    fn apply_modes(&mut self, desired: (bool, bool, bool, bool)) -> Result<(), String> {
        let current = self.controller.runtime_state();
        if desired.0 && !current.hud_enabled() {
            self.controller.set_hud_enabled(true)?;
        }
        if desired.1 && !current.crosshair_enabled() {
            self.controller.set_crosshair_enabled(true)?;
        }
        if desired.2 && !current.halo_enabled() {
            self.controller.set_halo_enabled(true)?;
        }
        if desired.3 && !current.zoom_enabled() {
            self.controller.set_zoom_enabled(true)?;
        }
        let current = self.controller.runtime_state();
        if !desired.0 && current.hud_enabled() {
            self.controller.set_hud_enabled(false)?;
        }
        if !desired.1 && current.crosshair_enabled() {
            self.controller.set_crosshair_enabled(false)?;
        }
        if !desired.2 && current.halo_enabled() {
            self.controller.set_halo_enabled(false)?;
        }
        if !desired.3 && current.zoom_enabled() {
            self.controller.set_zoom_enabled(false)?;
        }
        Ok(())
    }
}

fn on_off(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

fn foreground_hwnd() -> usize {
    unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() }.0 as usize
}

fn register_scene_class() -> Result<(), String> {
    let instance = HINSTANCE(
        unsafe { GetModuleHandleW(None) }
            .map_err(|e| e.to_string())?
            .0,
    );
    let class = WNDCLASSW {
        lpfnWndProc: Some(scene_proc),
        hInstance: instance,
        lpszClassName: w!("MultiLauncherCursorEffectsSmokeScene"),
        ..Default::default()
    };
    if unsafe { RegisterClassW(&class) } == 0 {
        return Err("RegisterClassW for cursor-effects scene failed".into());
    }
    Ok(())
}

fn create_scene_window(app: &Box<RefCell<App>>) -> Result<HWND, String> {
    let instance = HINSTANCE(
        unsafe { GetModuleHandleW(None) }
            .map_err(|e| e.to_string())?
            .0,
    );
    let desktop_left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let desktop_top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let desktop_width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let desktop_height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
    if desktop_width < 640 || desktop_height < 480 {
        return Err(format!(
            "virtual desktop is too small: {desktop_width}x{desktop_height}"
        ));
    }
    let width = 1100.min(desktop_width - 24);
    let height = 720.min(desktop_height - 48);
    let mut cursor = POINT::default();
    unsafe { GetCursorPos(&mut cursor) }
        .map_err(|error| format!("GetCursorPos while positioning scene: {error}"))?;
    let left = (cursor.x - width / 2).clamp(desktop_left, desktop_left + desktop_width - width);
    let top = (cursor.y - height / 2).clamp(desktop_top, desktop_top + desktop_height - height);
    let title = w!("Cursor effects production smoke — F1–F12 controls");
    let pointer = (&**app as *const RefCell<App>).cast::<c_void>();
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("MultiLauncherCursorEffectsSmokeScene"),
            title,
            WS_OVERLAPPEDWINDOW,
            left,
            top,
            width,
            height,
            None,
            None,
            instance,
            Some(pointer),
        )
    }
    .map_err(|error| format!("CreateWindowExW for cursor-effects scene: {error}"))
}

unsafe extern "system" fn scene_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize) };
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    if message == WM_DESTROY {
        unsafe { windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0) };
        return LRESULT(0);
    }
    if message == WM_NCDESTROY {
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    if message == WM_PAINT {
        let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<App> };
        let marker = if pointer.is_null() {
            0
        } else {
            unsafe { &*pointer }
                .try_borrow()
                .map(|app| app.marker)
                .unwrap_or_default()
        };
        paint_scene(hwnd, marker);
        return LRESULT(0);
    }
    if message == WM_TIMER && wparam.0 == TIMER_ID {
        let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<App> };
        if !pointer.is_null()
            && let Ok(mut app) = (unsafe { &*pointer }).try_borrow_mut()
        {
            app.marker = app.marker.wrapping_add(1);
            app.advance_auto_sequence(hwnd);
        }
        let _ = unsafe { windows::Win32::Graphics::Gdi::InvalidateRect(hwnd, None, BOOL(0)) };
        return LRESULT(0);
    }
    if message == WM_KEYDOWN {
        // Ignore auto-repeat so one held key cannot cycle several settings.
        if (lparam.0 as u64 & (1 << 30)) != 0 {
            return LRESULT(0);
        }
        let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<App> };
        if !pointer.is_null()
            && let Ok(mut app) = (unsafe { &*pointer }).try_borrow_mut()
            && app.handle_key(hwnd, wparam.0)
        {
            return LRESULT(0);
        }
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

fn paint_scene(hwnd: HWND, marker: usize) {
    let mut paint = PAINTSTRUCT::default();
    let dc = unsafe { BeginPaint(hwnd, &mut paint) };
    if dc.0.is_null() {
        return;
    }
    let mut client = RECT::default();
    let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut client) };
    let background = unsafe { CreateSolidBrush(COLORREF(0x00f8f8f8)) };
    if !background.0.is_null() {
        let _ = unsafe { FillRect(dc, &client, background) };
        let _ = unsafe { DeleteObject(background) };
    }
    let _ = unsafe { SetBkMode(dc, TRANSPARENT) };
    let _ = unsafe { SetTextColor(dc, COLORREF(0x00101010)) };
    draw_text(dc, 24, 18, "Production cursor effects smoke scene");
    draw_text(
        dc,
        24,
        42,
        "Move the real pointer over the changing center marker; leave it still to inspect live updates.",
    );
    draw_text(
        dc,
        24,
        66,
        "This window paints ordinary desktop pixels: checkerboard, RGB palette, text and animated marker.",
    );
    draw_text(
        dc,
        24,
        90,
        "F1 HUD  F2 crosshair  F3 halo  F4 zoom  F5 effects off (HUD retained)  F10 guides",
    );
    draw_text(
        dc,
        24,
        114,
        "F6 inversion 0/40/100%  F7 centered/offset  F8 factor incl. 1.7  F9 diameter 160/163",
    );
    draw_text(
        dc,
        24,
        138,
        "F11 native HWND/source/transform/filter diagnostics  F12 bounded desktop BMP  Escape exits",
    );

    let swatch_y = 178;
    let swatches = [
        ("black", COLORREF(0x00000000)),
        ("white", COLORREF(0x00ffffff)),
        ("gray", COLORREF(0x00808080)),
        ("red", COLORREF(0x000000ff)),
        ("green", COLORREF(0x0000ff00)),
        ("blue", COLORREF(0x00ff0000)),
        ("yellow", COLORREF(0x0000ffff)),
    ];
    for (index, (name, color)) in swatches.into_iter().enumerate() {
        let left = 24 + index as i32 * 112;
        let rect = RECT {
            left,
            top: swatch_y,
            right: left + 88,
            bottom: swatch_y + 56,
        };
        fill(dc, rect, color);
        draw_text(dc, left, swatch_y + 62, name);
    }

    let checker_left = 24;
    let checker_top = 280;
    for row in 0..8 {
        for column in 0..12 {
            let row_offset = row as i32 * 24;
            let column_offset = column as i32 * 24;
            let color = if (row + column + marker / 5) % 2 == 0 {
                COLORREF(0x00ffffff)
            } else {
                COLORREF(0x00202020)
            };
            fill(
                dc,
                RECT {
                    left: checker_left + column_offset,
                    top: checker_top + row_offset,
                    right: checker_left + column_offset + 24,
                    bottom: checker_top + row_offset + 24,
                },
                color,
            );
        }
    }

    let marker_x = client.right / 2 - 18;
    let marker_y = client.bottom / 2 - 18;
    let marker_colors = [
        COLORREF(0x000000ff),
        COLORREF(0x00ff0000),
        COLORREF(0x0000ff00),
        COLORREF(0x0000ffff),
        COLORREF(0x00808080),
        COLORREF(0x00ffffff),
        COLORREF(0x00000000),
    ];
    let color = marker_colors[marker % marker_colors.len()];
    fill(
        dc,
        RECT {
            left: marker_x,
            top: marker_y,
            right: marker_x + 36,
            bottom: marker_y + 36,
        },
        color,
    );
    draw_text(
        dc,
        marker_x + 44,
        marker_y + 8,
        "animated color changes in place every 350 ms",
    );
    let _ = unsafe { EndPaint(hwnd, &paint) };
}

fn fill(dc: HDC, rect: RECT, color: COLORREF) {
    let brush = unsafe { CreateSolidBrush(color) };
    if brush.0.is_null() {
        return;
    }
    let _ = unsafe { FillRect(dc, &rect, brush) };
    let _ = unsafe { DeleteObject(brush) };
}

fn draw_text(dc: HDC, x: i32, y: i32, text: &str) {
    let wide = text.encode_utf16().collect::<Vec<_>>();
    let _ = unsafe { TextOutW(dc, x, y, &wide) };
}

#[derive(Debug)]
struct WindowInfo {
    hwnd: HWND,
    class: String,
    title: String,
    visible: bool,
    rect: RECT,
    style: isize,
    ex_style: isize,
    children: Vec<WindowInfo>,
}

fn enumerate_owned_windows() -> Result<Vec<WindowInfo>, String> {
    let mut windows = Vec::new();
    unsafe extern "system" fn visit(hwnd: HWND, parameter: LPARAM) -> BOOL {
        let windows = unsafe { &mut *(parameter.0 as *mut Vec<WindowInfo>) };
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid != std::process::id() {
            return BOOL(1);
        }
        let class = window_class(hwnd);
        if class != EFFECT_HOST_CLASS && class != COORDINATE_SURFACE_CLASS && class != SCENE_CLASS {
            return BOOL(1);
        }
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(hwnd, &mut rect) }.is_err() {
            return BOOL(1);
        }
        let mut children = Vec::new();
        unsafe extern "system" fn visit_child(hwnd: HWND, parameter: LPARAM) -> BOOL {
            let children = unsafe { &mut *(parameter.0 as *mut Vec<WindowInfo>) };
            let mut rect = RECT::default();
            if unsafe { GetWindowRect(hwnd, &mut rect) }.is_ok() {
                children.push(WindowInfo {
                    hwnd,
                    class: window_class(hwnd),
                    title: window_title(hwnd),
                    visible: unsafe { IsWindowVisible(hwnd) }.as_bool(),
                    rect,
                    style: unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) },
                    ex_style: unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) },
                    children: Vec::new(),
                });
            }
            BOOL(1)
        }
        let _ = unsafe {
            EnumChildWindows(
                hwnd,
                Some(visit_child),
                LPARAM(&mut children as *mut _ as isize),
            )
        };
        windows.push(WindowInfo {
            hwnd,
            class,
            title: window_title(hwnd),
            visible: unsafe { IsWindowVisible(hwnd) }.as_bool(),
            rect,
            style: unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) },
            ex_style: unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) },
            children,
        });
        BOOL(1)
    }
    unsafe { EnumWindows(Some(visit), LPARAM(&mut windows as *mut _ as isize)) }
        .map_err(|error| format!("EnumWindows failed: {error}"))?;
    Ok(windows)
}

fn window_class(hwnd: HWND) -> String {
    let mut name = [0_u16; 256];
    let length = unsafe { GetClassNameW(hwnd, &mut name) };
    if length <= 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&name[..length as usize])
    }
}

fn window_title(hwnd: HWND) -> String {
    let mut title = [0_u16; 256];
    let length = unsafe { GetWindowTextW(hwnd, &mut title) };
    if length <= 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&title[..length as usize])
    }
}

fn magnifier_details(hwnd: HWND) -> String {
    let mut source = RECT::default();
    let source_result = unsafe { MagGetWindowSource(hwnd, &mut source) }.as_bool();
    let mut transform = MAGTRANSFORM::default();
    let transform_result = unsafe { MagGetWindowTransform(hwnd, &mut transform) }.as_bool();
    let mut color_effect = MAGCOLOREFFECT::default();
    let color_result = unsafe { MagGetColorEffect(hwnd, &mut color_effect) }.as_bool();
    let mut filter_mode = MW_FILTERMODE(0);
    let filter_count =
        unsafe { MagGetWindowFilterList(hwnd, &mut filter_mode, 0, std::ptr::null_mut()) };
    let mut filters = Vec::new();
    let filter_result = if (0..=64).contains(&filter_count) {
        filters.resize(filter_count as usize, HWND::default());
        let returned = unsafe {
            MagGetWindowFilterList(
                hwnd,
                &mut filter_mode,
                filters.len() as i32,
                filters.as_mut_ptr(),
            )
        };
        format!(
            "returned={returned} mode={filter_mode:?} handles={:?}",
            filters
                .iter()
                .map(|filter| filter.0 as usize)
                .collect::<Vec<_>>()
        )
    } else {
        format!("count-query={filter_count}")
    };
    format!(
        "MAGNIFIER hwnd=0x{:x} source_ok={source_result} source={source:?} transform_ok={transform_result} transform={:?} color_ok={color_result} color_matrix={:?} filter={filter_result}",
        hwnd.0 as usize, transform.v, color_effect.transform
    )
}

struct ScreenReadback {
    desktop_dc: HDC,
    memory_dc: HDC,
    bitmap: HBITMAP,
    previous_bitmap: HGDIOBJ,
    bitmap_selected: bool,
    bits: *mut c_void,
    byte_count: usize,
}

impl ScreenReadback {
    fn new(width: i32, height: i32) -> Result<Self, String> {
        let byte_count = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or("readback buffer size overflow")?;
        let mut capture = Self {
            desktop_dc: HDC::default(),
            memory_dc: HDC::default(),
            bitmap: HBITMAP::default(),
            previous_bitmap: HGDIOBJ::default(),
            bitmap_selected: false,
            bits: std::ptr::null_mut(),
            byte_count,
        };
        capture.desktop_dc = unsafe { GetDC(HWND::default()) };
        if capture.desktop_dc.0.is_null() {
            return Err("GetDC(desktop) returned null".into());
        }
        capture.memory_dc = unsafe { CreateCompatibleDC(capture.desktop_dc) };
        if capture.memory_dc.0.is_null() {
            return Err("CreateCompatibleDC returned null".into());
        }
        let bitmap_info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                biSizeImage: byte_count as u32,
                ..Default::default()
            },
            ..Default::default()
        };
        capture.bitmap = unsafe {
            CreateDIBSection(
                capture.desktop_dc,
                &bitmap_info,
                windows::Win32::Graphics::Gdi::DIB_RGB_COLORS,
                &mut capture.bits,
                HANDLE::default(),
                0,
            )
        }
        .map_err(|error| format!("CreateDIBSection failed: {error}"))?;
        if capture.bits.is_null() {
            return Err("CreateDIBSection returned null pixel storage".into());
        }
        capture.previous_bitmap = unsafe { SelectObject(capture.memory_dc, capture.bitmap) };
        if capture.previous_bitmap.0.is_null() {
            return Err("SelectObject(memory_dc, bitmap) failed".into());
        }
        capture.bitmap_selected = true;
        Ok(capture)
    }

    fn copy_rect(&mut self, rect: RECT, width: i32, height: i32) -> Result<&[u8], String> {
        unsafe {
            BitBlt(
                self.memory_dc,
                0,
                0,
                width,
                height,
                self.desktop_dc,
                rect.left,
                rect.top,
                ROP_CODE(SRCCOPY.0 | CAPTUREBLT.0),
            )
        }
        .map_err(|error| format!("BitBlt(SRCCOPY|CAPTUREBLT): {error}"))?;
        if !unsafe { GdiFlush() }.as_bool() {
            return Err("GdiFlush failed before reading DIB pixels".into());
        }
        // SAFETY: CreateDIBSection allocated byte_count bytes; the bitmap stays
        // selected and alive, and GdiFlush completed queued GDI writes.
        Ok(unsafe { std::slice::from_raw_parts(self.bits.cast(), self.byte_count) })
    }
}

impl Drop for ScreenReadback {
    fn drop(&mut self) {
        if self.bitmap_selected {
            let restored = unsafe { SelectObject(self.memory_dc, self.previous_bitmap) };
            if restored.0.is_null() {
                // Deleting the memory DC releases its selected bitmap before
                // DeleteObject is attempted below.
                let _ = unsafe { DeleteDC(self.memory_dc) };
                self.memory_dc = HDC::default();
            }
            self.bitmap_selected = false;
        }
        if !self.bitmap.0.is_null() {
            let _ = unsafe { DeleteObject(self.bitmap) };
        }
        if !self.memory_dc.0.is_null() {
            let _ = unsafe { DeleteDC(self.memory_dc) };
        }
        if !self.desktop_dc.0.is_null() {
            let _ = unsafe { ReleaseDC(HWND::default(), self.desktop_dc) };
        }
    }
}

fn save_desktop_bmp(path: &PathBuf, rect: RECT) -> Result<[u8; 3], String> {
    let width = rect
        .right
        .checked_sub(rect.left)
        .ok_or("readback width overflow")?;
    let height = rect
        .bottom
        .checked_sub(rect.top)
        .ok_or("readback height overflow")?;
    if width != READBACK_WIDTH || height != READBACK_HEIGHT {
        return Err("readback exceeded the fixed-size bound".into());
    }
    let mut readback = ScreenReadback::new(width, height)?;
    let pixels = readback.copy_rect(rect, width, height)?;
    let center = ((height as usize / 2) * width as usize + width as usize / 2) * 4;
    let cursor_rgb = [pixels[center + 2], pixels[center + 1], pixels[center]];
    write_bmp(path, width, height, pixels)?;
    Ok(cursor_rgb)
}

fn write_bmp(path: &PathBuf, width: i32, height: i32, pixels: &[u8]) -> Result<(), String> {
    let image_size = (width as u32)
        .checked_mul(height as u32)
        .and_then(|count| count.checked_mul(4))
        .ok_or("BMP image size overflow")?;
    if pixels.len() != image_size as usize {
        return Err("BMP pixel buffer length mismatch".into());
    }
    let file_size = 54_u32
        .checked_add(image_size)
        .ok_or("BMP file size overflow")?;
    let mut bmp = Vec::with_capacity(file_size as usize);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&file_size.to_le_bytes());
    bmp.extend_from_slice(&0_u32.to_le_bytes());
    bmp.extend_from_slice(&54_u32.to_le_bytes());
    bmp.extend_from_slice(&40_u32.to_le_bytes());
    bmp.extend_from_slice(&width.to_le_bytes());
    bmp.extend_from_slice(&(-height).to_le_bytes());
    bmp.extend_from_slice(&1_u16.to_le_bytes());
    bmp.extend_from_slice(&32_u16.to_le_bytes());
    bmp.extend_from_slice(&BI_RGB.0.to_le_bytes());
    bmp.extend_from_slice(&image_size.to_le_bytes());
    bmp.extend_from_slice(&0_i32.to_le_bytes());
    bmp.extend_from_slice(&0_i32.to_le_bytes());
    bmp.extend_from_slice(&0_u32.to_le_bytes());
    bmp.extend_from_slice(&0_u32.to_le_bytes());
    bmp.extend_from_slice(pixels);
    std::fs::write(path, bmp).map_err(|error| format!("write {}: {error}", path.display()))
}
