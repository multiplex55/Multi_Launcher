//! Standalone native feasibility proof for the Windows Magnification API.
//! This deliberately does not link the production coordinate-tool renderer.

#[cfg(not(windows))]
fn main() {
    println!("cursor-effects-smoke: UNVERIFIED; this harness requires Windows");
}

#[cfg(windows)]
fn main() {
    if let Err(error) = win32::run() {
        eprintln!("cursor-effects-smoke: startup/runtime failure: {error}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
mod win32 {
    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::fs::{File, OpenOptions, create_dir_all};
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use windows::Win32::Foundation::{
        BOOL, COLORREF, GetLastError, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
    };
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, BitBlt, CAPTUREBLT, CreateCompatibleDC,
        CreateDIBSection, CreateEllipticRgn, CreateSolidBrush, DIB_RGB_COLORS, DeleteDC,
        DeleteObject, EndPaint, FillRect, GdiFlush, GetDC, GetPixel, HBITMAP, HDC, HGDIOBJ,
        InvalidateRect, PAINTSTRUCT, ROP_CODE, ReleaseDC, SRCCOPY, ScreenToClient, SelectObject,
        SetBkMode, SetTextColor, TRANSPARENT, TextOutW, UpdateWindow,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
    };
    use windows::Win32::UI::Magnification::{
        MAGCOLOREFFECT, MAGTRANSFORM, MS_SHOWMAGNIFIEDCURSOR, MW_FILTERMODE_EXCLUDE, MagInitialize,
        MagSetColorEffect, MagSetWindowFilterList, MagSetWindowSource, MagSetWindowTransform,
        MagUninitialize, WC_MAGNIFIER,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        BS_PUSHBUTTON, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
        GWLP_USERDATA, GetClientRect, GetCursorPos, GetForegroundWindow, GetMessageW,
        GetWindowLongPtrW, GetWindowRect, GetWindowTextW, HMENU, HWND_TOPMOST, IsWindow,
        IsWindowVisible, MA_NOACTIVATE, MSG, PostMessageW, PostQuitMessage, RegisterClassW,
        SW_HIDE, SW_SHOW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
        SWP_SHOWWINDOW, SetLayeredWindowAttributes, SetTimer, SetWindowLongPtrW, SetWindowPos,
        SetWindowTextW, ShowWindow, TranslateMessage, WINDOW_STYLE, WM_CLOSE, WM_COMMAND,
        WM_DESTROY, WM_KEYDOWN, WM_MOUSEACTIVATE, WM_NCCREATE, WM_NCHITTEST, WM_PAINT, WM_TIMER,
        WNDCLASSW, WS_CHILD, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
        WS_OVERLAPPEDWINDOW, WS_POPUP, WS_VISIBLE,
    };
    use windows::core::{PCWSTR, w};

    const SCENE_CLASS: PCWSTR = w!("MultiLauncherCursorEffectsSmokeScene");
    const OVERLAY_CLASS: PCWSTR = w!("MultiLauncherCursorEffectsSmokeOverlay");
    const HOST_CLASS: PCWSTR = w!("MultiLauncherCursorEffectsSmokeHost");
    const TIMER_ID: usize = 1;
    const TIMER_INTERVAL_MS: u32 = 16;
    const SCENE_WIDTH: i32 = 1230;
    const SCENE_HEIGHT: i32 = 790;
    const HALO_RADIUS: i32 = 60;
    const LENS_DIAMETER: i32 = 160;
    const LENS_SOURCE_HALF: i32 = LENS_DIAMETER / 4;
    const LENS_OFFSET_X: i32 = 120;
    const LENS_OFFSET_Y: i32 = 80;
    const READBACK_MAX_WIDTH: i32 = 320;
    const READBACK_MAX_HEIGHT: i32 = 280;
    const ID_HALO: i32 = 1001;
    const ID_LENS: i32 = 1002;
    const ID_STRENGTH_0: i32 = 1003;
    const ID_STRENGTH_40: i32 = 1004;
    const ID_STRENGTH_100: i32 = 1005;
    const ID_LENS_MODE: i32 = 1006;
    const ID_RECREATE: i32 = 1007;
    const ID_OVERLAYS: i32 = 1008;
    const ID_MARKER: i32 = 1009;
    const ID_INSPECT: i32 = 1010;
    const ID_EXIT: i32 = 1011;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum EffectKind {
        Halo,
        Lens,
    }

    impl EffectKind {
        fn name(self) -> &'static str {
            match self {
                Self::Halo => "halo-1x",
                Self::Lens => "lens-2x",
            }
        }

        fn title(self) -> PCWSTR {
            match self {
                Self::Halo => w!("Cursor Effects Halo 1x"),
                Self::Lens => w!("Cursor Effects Zoom Lens 2x"),
            }
        }

        fn diameter(self) -> i32 {
            match self {
                Self::Halo => HALO_RADIUS * 2,
                Self::Lens => LENS_DIAMETER,
            }
        }

        fn scale(self) -> f32 {
            match self {
                Self::Halo => 1.0,
                Self::Lens => 2.0,
            }
        }
    }

    #[derive(Clone, Copy, Debug)]
    enum OverlayKind {
        Hud,
        Crosshair,
        HorizontalGuide,
        VerticalGuide,
    }

    impl OverlayKind {
        fn id(self) -> isize {
            match self {
                Self::Hud => 1,
                Self::Crosshair => 2,
                Self::HorizontalGuide => 3,
                Self::VerticalGuide => 4,
            }
        }

        fn title(self) -> PCWSTR {
            match self {
                Self::Hud => w!("Cursor Effects Smoke Fake HUD"),
                Self::Crosshair => w!("Cursor Effects Smoke Fake Crosshair"),
                Self::HorizontalGuide => w!("Cursor Effects Smoke Fake Horizontal Guide"),
                Self::VerticalGuide => w!("Cursor Effects Smoke Fake Vertical Guide"),
            }
        }

        fn from_id(id: isize) -> Option<Self> {
            match id {
                1 => Some(Self::Hud),
                2 => Some(Self::Crosshair),
                3 => Some(Self::HorizontalGuide),
                4 => Some(Self::VerticalGuide),
                _ => None,
            }
        }
    }

    #[derive(Clone, Copy)]
    struct OverlayWindow {
        hwnd: HWND,
        kind: OverlayKind,
    }

    struct EffectSurface {
        kind: EffectKind,
        host: HWND,
        magnifier: HWND,
        source: RECT,
        destination: RECT,
        visible: bool,
    }

    struct Reporter {
        path: PathBuf,
        file: Option<File>,
    }

    impl Reporter {
        fn new() -> Self {
            let path = std::env::temp_dir().join("MultiLauncherCursorEffectsSmoke.log");
            let file = match OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&path)
            {
                Ok(file) => Some(file),
                Err(error) => {
                    eprintln!(
                        "cursor-effects-smoke: could not create {}: {error}",
                        path.display()
                    );
                    None
                }
            };
            Self { path, file }
        }

        fn line(&mut self, message: impl AsRef<str>) {
            let message = message.as_ref();
            println!("cursor-effects-smoke: {message}");
            if let Some(file) = &mut self.file {
                let _ = writeln!(file, "{message}");
                let _ = file.flush();
            }
        }
    }

    struct App {
        instance: HINSTANCE,
        scene: HWND,
        buttons: Vec<(i32, HWND)>,
        overlays: Vec<OverlayWindow>,
        overlays_visible: bool,
        marker_moving: bool,
        marker_frame: usize,
        last_marker_change: Instant,
        halo: Option<EffectSurface>,
        lens: Option<EffectSurface>,
        strength_index: usize,
        lens_offset: bool,
        create_count: usize,
        destroy_count: usize,
        cursor_read_failed: bool,
        cleaned: bool,
        reporter: Reporter,
    }

    impl App {
        fn new(instance: HINSTANCE, reporter: Reporter) -> Self {
            Self {
                instance,
                scene: HWND::default(),
                buttons: Vec::new(),
                overlays: Vec::new(),
                overlays_visible: true,
                marker_moving: true,
                marker_frame: 0,
                last_marker_change: Instant::now(),
                halo: None,
                lens: None,
                strength_index: 1,
                lens_offset: true,
                create_count: 0,
                destroy_count: 0,
                cursor_read_failed: false,
                cleaned: false,
                reporter,
            }
        }

        fn log(&mut self, message: impl AsRef<str>) {
            self.reporter.line(message);
        }

        fn create_buttons(&mut self) -> Result<(), String> {
            let specs = [
                (ID_HALO, "Halo: ON", 108),
                (ID_LENS, "Lens: ON", 108),
                (ID_STRENGTH_0, "Halo 0%", 95),
                (ID_STRENGTH_40, "Halo 40%", 95),
                (ID_STRENGTH_100, "Halo 100%", 100),
                (ID_LENS_MODE, "Lens: OFFSET", 110),
                (ID_RECREATE, "Recreate", 100),
                (ID_OVERLAYS, "Fake UI: ON", 105),
                (ID_MARKER, "Marker: moving", 115),
                (ID_INSPECT, "Inspect / log", 110),
                (ID_EXIT, "Exit", 65),
            ];
            let mut x = 10;
            for (id, label, width) in specs {
                let label_wide = wide(label);
                let hwnd = unsafe {
                    CreateWindowExW(
                        Default::default(),
                        w!("BUTTON"),
                        PCWSTR(label_wide.as_ptr()),
                        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_PUSHBUTTON as u32),
                        x,
                        12,
                        width,
                        34,
                        self.scene,
                        HMENU(id as *mut c_void),
                        self.instance,
                        None,
                    )
                }
                .map_err(|error| format!("create scene control {label:?}: {error}"))?;
                self.buttons.push((id, hwnd));
                x += width + 4;
            }
            Ok(())
        }

        fn create_reference_overlays(&mut self) -> Result<(), String> {
            let specs = [
                (OverlayKind::Hud, 55, 635, 285, 42),
                (OverlayKind::Crosshair, 465, 360, 90, 90),
                (OverlayKind::HorizontalGuide, 50, 520, 970, 5),
                (OverlayKind::VerticalGuide, 560, 155, 5, 450),
            ];
            for (kind, client_x, client_y, width, height) in specs {
                let (screen_x, screen_y) = self.client_to_screen(client_x, client_y);
                let hwnd = unsafe {
                    CreateWindowExW(
                        WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                        OVERLAY_CLASS,
                        kind.title(),
                        WS_POPUP,
                        screen_x,
                        screen_y,
                        width,
                        height,
                        HWND::default(),
                        HMENU::default(),
                        self.instance,
                        Some(kind.id() as *const c_void),
                    )
                }
                .map_err(|error| format!("create named {:?} reference overlay: {error}", kind))?;
                if let Err(error) = unsafe {
                    SetLayeredWindowAttributes(
                        hwnd,
                        COLORREF(0),
                        255,
                        windows::Win32::UI::WindowsAndMessaging::LWA_COLORKEY,
                    )
                } {
                    let _ = unsafe { DestroyWindow(hwnd) };
                    return Err(format!("configure {:?} transparency: {error}", kind));
                }
                self.overlays.push(OverlayWindow { hwnd, kind });
                if let Err(error) = unsafe {
                    SetWindowPos(
                        hwnd,
                        HWND_TOPMOST,
                        screen_x,
                        screen_y,
                        width,
                        height,
                        SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    )
                } {
                    return Err(format!("position {:?} reference overlay: {error}", kind));
                }
            }
            Ok(())
        }

        fn create_effect(&mut self, kind: EffectKind) -> Result<EffectSurface, String> {
            let diameter = kind.diameter();
            let host = unsafe {
                CreateWindowExW(
                    WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                    HOST_CLASS,
                    kind.title(),
                    WS_POPUP,
                    -diameter - 10,
                    -diameter - 10,
                    diameter,
                    diameter,
                    HWND::default(),
                    HMENU::default(),
                    self.instance,
                    None,
                )
            }
            .map_err(|error| format!("create {} host window: {error}", kind.name()))?;
            self.create_count += 1;

            let region = unsafe { CreateEllipticRgn(0, 0, diameter, diameter) };
            if region.0.is_null() {
                let error = format!(
                    "CreateEllipticRgn failed for {} (GetLastError={})",
                    kind.name(),
                    unsafe { GetLastError().0 }
                );
                self.discard_partial_host(kind, host, &error);
                return Err(error);
            }
            if unsafe { windows::Win32::Graphics::Gdi::SetWindowRgn(host, region, BOOL(1)) } == 0 {
                let error = format!(
                    "SetWindowRgn failed for {} (GetLastError={})",
                    kind.name(),
                    unsafe { GetLastError().0 }
                );
                let region_deleted = unsafe { DeleteObject(region) }.as_bool();
                self.discard_partial_host(
                    kind,
                    host,
                    &format!("{error}; DeleteObject(region)={region_deleted}"),
                );
                return Err(error);
            }

            if let Err(error) = unsafe {
                SetLayeredWindowAttributes(
                    host,
                    COLORREF(0),
                    255,
                    windows::Win32::UI::WindowsAndMessaging::LWA_ALPHA,
                )
            } {
                let error = format!("set {} host alpha=255: {error}", kind.name());
                self.discard_partial_host(kind, host, &error);
                return Err(error);
            }

            let magnifier = match unsafe {
                CreateWindowExW(
                    Default::default(),
                    WC_MAGNIFIER,
                    PCWSTR::null(),
                    WS_CHILD | WS_VISIBLE,
                    0,
                    0,
                    diameter,
                    diameter,
                    host,
                    HMENU::default(),
                    self.instance,
                    None,
                )
            } {
                Ok(hwnd) => hwnd,
                Err(error) => {
                    let error = format!("create {} WC_MAGNIFIER child: {error}", kind.name());
                    self.discard_partial_host(kind, host, &error);
                    return Err(error);
                }
            };

            let mut transform = MAGTRANSFORM {
                v: [
                    kind.scale(),
                    0.0,
                    0.0,
                    0.0,
                    kind.scale(),
                    0.0,
                    0.0,
                    0.0,
                    1.0,
                ],
            };
            if !unsafe { MagSetWindowTransform(magnifier, &mut transform) }.as_bool() {
                let reason = format!(
                    "MagSetWindowTransform({}) failed (GetLastError={})",
                    kind.name(),
                    unsafe { GetLastError().0 }
                );
                self.discard_partial_host(kind, host, &reason);
                return Err(reason);
            }
            let strength = if kind == EffectKind::Halo {
                self.strength()
            } else {
                0.0
            };
            let mut effect = inversion_matrix(strength);
            if !unsafe { MagSetColorEffect(magnifier, &mut effect) }.as_bool() {
                let reason = format!(
                    "MagSetColorEffect({}) failed (GetLastError={})",
                    kind.name(),
                    unsafe { GetLastError().0 }
                );
                self.discard_partial_host(kind, host, &reason);
                return Err(reason);
            }

            self.log(format!(
                "created {} host={} magnifier={} scale={} MS_SHOWMAGNIFIEDCURSOR_not_set=true (constant={}) host_styles=layered|transparent|noactivate|toolwindow circle_region=true layered_alpha=255",
                kind.name(),
                hwnd_value(host),
                hwnd_value(magnifier),
                kind.scale(),
                MS_SHOWMAGNIFIEDCURSOR
            ));
            if kind == EffectKind::Halo {
                self.log(format!(
                    "halo matrix strength={:.0}% transform={:?}",
                    self.strength() * 100.0,
                    inversion_matrix(self.strength()).transform
                ));
            }
            Ok(EffectSurface {
                kind,
                host,
                magnifier,
                source: RECT::default(),
                destination: RECT::default(),
                visible: false,
            })
        }

        fn toggle_effect(&mut self, kind: EffectKind) {
            let was_enabled = match kind {
                EffectKind::Halo => self.halo.is_some(),
                EffectKind::Lens => self.lens.is_some(),
            };
            if was_enabled {
                self.destroy_effect(kind);
                if let Err(error) = self.update_filters() {
                    self.log(format!(
                        "remaining effect filter refresh failed; disabling all effects: {error}"
                    ));
                    self.destroy_all_effects();
                }
                self.reorder_topmost();
                return;
            }

            match self.create_effect(kind) {
                Ok(surface) => match kind {
                    EffectKind::Halo => self.halo = Some(surface),
                    EffectKind::Lens => self.lens = Some(surface),
                },
                Err(error) => {
                    self.log(format!("{} remains OFF: {error}", kind.name()));
                    self.update_button_labels();
                    return;
                }
            }

            if let Err(error) = self.update_filters() {
                self.log(format!(
                    "filtering incomplete; disabling all magnifier surfaces: {error}"
                ));
                self.destroy_all_effects();
            }
            self.reorder_topmost();
            self.update_button_labels();
        }

        fn destroy_effect(&mut self, kind: EffectKind) {
            let surface = match kind {
                EffectKind::Halo => self.halo.take(),
                EffectKind::Lens => self.lens.take(),
            };
            if let Some(surface) = surface {
                self.destroy_surface(surface);
            }
        }

        fn destroy_surface(&mut self, surface: EffectSurface) {
            let result = unsafe { DestroyWindow(surface.host) };
            self.destroy_count += 1;
            self.log(format!(
                "destroyed {} host={} magnifier={} result={:?} resources_created={} resources_destroyed={}",
                surface.kind.name(),
                hwnd_value(surface.host),
                hwnd_value(surface.magnifier),
                result,
                self.create_count,
                self.destroy_count
            ));
        }

        fn discard_partial_host(&mut self, kind: EffectKind, host: HWND, reason: &str) {
            let result = unsafe { DestroyWindow(host) };
            self.destroy_count += 1;
            self.log(format!(
                "partial {} initialization failed: {reason}; host={} destroyed={result:?} resources_created={} resources_destroyed={}",
                kind.name(),
                hwnd_value(host),
                self.create_count,
                self.destroy_count
            ));
        }

        fn destroy_all_effects(&mut self) {
            self.destroy_effect(EffectKind::Lens);
            self.destroy_effect(EffectKind::Halo);
            self.update_button_labels();
        }

        fn recreate_effects(&mut self) {
            self.destroy_all_effects();
            self.toggle_effect(EffectKind::Halo);
            self.toggle_effect(EffectKind::Lens);
            self.log(format!(
                "explicit recreate complete; resources_created={} resources_destroyed={}",
                self.create_count, self.destroy_count
            ));
        }

        fn update_filters(&mut self) -> Result<(), String> {
            let mut excluded: Vec<HWND> =
                self.overlays.iter().map(|overlay| overlay.hwnd).collect();
            if let Some(surface) = &self.halo {
                excluded.push(surface.host);
            }
            if let Some(surface) = &self.lens {
                excluded.push(surface.host);
            }
            let names = self
                .overlays
                .iter()
                .map(|overlay| {
                    format!(
                        "{}={}",
                        overlay_name(overlay.kind),
                        hwnd_value(overlay.hwnd)
                    )
                })
                .chain(
                    self.halo
                        .iter()
                        .map(|surface| format!("halo-host={}", hwnd_value(surface.host))),
                )
                .chain(
                    self.lens
                        .iter()
                        .map(|surface| format!("lens-host={}", hwnd_value(surface.host))),
                )
                .collect::<Vec<_>>()
                .join(",");

            let targets: Vec<(EffectKind, HWND)> = [&self.halo, &self.lens]
                .into_iter()
                .flatten()
                .map(|surface| (surface.kind, surface.magnifier))
                .collect();
            let mut configured = Vec::with_capacity(targets.len());
            for (kind, magnifier) in targets {
                if excluded.len() > i32::MAX as usize {
                    return Err("too many HWNDs for MagSetWindowFilterList".into());
                }
                if !unsafe {
                    MagSetWindowFilterList(
                        magnifier,
                        MW_FILTERMODE_EXCLUDE,
                        excluded.len() as i32,
                        excluded.as_mut_ptr(),
                    )
                }
                .as_bool()
                {
                    let error = unsafe { GetLastError().0 };
                    return Err(format!(
                        "MagSetWindowFilterList({}) failed (GetLastError={error}); excluded=[{names}]",
                        kind.name()
                    ));
                }
                configured.push(format!(
                    "filter configured {} count={} mode=exclude hwnds=[{names}]",
                    kind.name(),
                    excluded.len()
                ));
            }
            for line in configured {
                self.log(line);
            }
            Ok(())
        }

        fn strength(&self) -> f32 {
            [0.0, 0.4, 1.0][self.strength_index]
        }

        fn set_strength_index(&mut self, index: usize) {
            self.strength_index = index.min(2);
            let strength = self.strength();
            let result = if let Some(halo) = &mut self.halo {
                let mut effect = inversion_matrix(strength);
                if unsafe { MagSetColorEffect(halo.magnifier, &mut effect) }.as_bool() {
                    Ok(())
                } else {
                    Err(format!(
                        "MagSetColorEffect update failed (GetLastError={})",
                        unsafe { GetLastError().0 }
                    ))
                }
            } else {
                Ok(())
            };
            match result {
                Ok(()) => self.log(format!(
                    "halo strength set to {:.0}% matrix={:?}",
                    strength * 100.0,
                    inversion_matrix(strength).transform
                )),
                Err(error) => {
                    self.log(format!(
                        "halo disabled after color-effect update error: {error}"
                    ));
                    self.destroy_effect(EffectKind::Halo);
                    if let Err(filter_error) = self.update_filters() {
                        self.log(format!(
                            "remaining effect filter refresh failed; disabling all effects: {filter_error}"
                        ));
                        self.destroy_all_effects();
                    }
                }
            }
            self.update_button_labels();
        }

        fn update_button_labels(&mut self) {
            let labels = [
                (
                    ID_HALO,
                    if self.halo.is_some() {
                        "Halo: ON"
                    } else {
                        "Halo: OFF"
                    },
                ),
                (
                    ID_LENS,
                    if self.lens.is_some() {
                        "Lens: ON"
                    } else {
                        "Lens: OFF"
                    },
                ),
                (
                    ID_LENS_MODE,
                    if self.lens_offset {
                        "Lens: OFFSET"
                    } else {
                        "Lens: CENTER"
                    },
                ),
                (
                    ID_OVERLAYS,
                    if self.overlays_visible {
                        "Fake UI: ON"
                    } else {
                        "Fake UI: OFF"
                    },
                ),
                (
                    ID_MARKER,
                    if self.marker_moving {
                        "Marker: moving"
                    } else {
                        "Marker: still"
                    },
                ),
            ];
            for (id, label) in labels {
                if let Some((_, hwnd)) = self.buttons.iter().find(|(button_id, _)| *button_id == id)
                {
                    let label = wide(label);
                    let _ = unsafe { SetWindowTextW(*hwnd, PCWSTR(label.as_ptr())) };
                }
            }
        }

        fn handle_command(&mut self, command: i32) {
            match command {
                ID_HALO => self.toggle_effect(EffectKind::Halo),
                ID_LENS => self.toggle_effect(EffectKind::Lens),
                ID_STRENGTH_0 => self.set_strength_index(0),
                ID_STRENGTH_40 => self.set_strength_index(1),
                ID_STRENGTH_100 => self.set_strength_index(2),
                ID_LENS_MODE => self.toggle_lens_mode(),
                ID_RECREATE => self.recreate_effects(),
                ID_OVERLAYS => self.toggle_overlays(),
                ID_MARKER => self.toggle_marker(),
                ID_INSPECT => self.inspect(),
                ID_EXIT => self.request_close(),
                _ => {}
            }
        }

        fn handle_key(&mut self, key: usize) -> bool {
            match key {
                0x74 => self.inspect(),                                         // F5
                0x75 => self.toggle_effect(EffectKind::Halo),                   // F6
                0x76 => self.toggle_effect(EffectKind::Lens),                   // F7
                0x77 => self.set_strength_index((self.strength_index + 1) % 3), // F8
                0x78 => self.toggle_lens_mode(),                                // F9
                0x79 => self.toggle_overlays(),                                 // F10
                0x7b => self.save_composed_readback(),                          // F12
                0x1b => self.request_close(),
                _ => return false,
            }
            true
        }

        fn request_close(&mut self) {
            if let Err(error) = unsafe { PostMessageW(self.scene, WM_CLOSE, WPARAM(0), LPARAM(0)) }
            {
                self.log(format!("could not post deferred scene close: {error}"));
            }
        }

        fn toggle_lens_mode(&mut self) {
            self.lens_offset = !self.lens_offset;
            self.log(format!(
                "lens destination mode={} (source remains centered on cursor); offset=({LENS_OFFSET_X},{LENS_OFFSET_Y}) physical pixels",
                if self.lens_offset { "offset" } else { "centered" }
            ));
            self.update_button_labels();
        }

        fn toggle_overlays(&mut self) {
            self.overlays_visible = !self.overlays_visible;
            for overlay in &self.overlays {
                let command = if self.overlays_visible {
                    SW_SHOWNOACTIVATE
                } else {
                    SW_HIDE
                };
                let _ = unsafe { ShowWindow(overlay.hwnd, command) };
            }
            self.log(format!(
                "fake reference overlays visible={}",
                self.overlays_visible
            ));
            self.reorder_topmost();
            self.update_button_labels();
        }

        fn toggle_marker(&mut self) {
            self.marker_moving = !self.marker_moving;
            self.last_marker_change = Instant::now();
            self.log(format!("scene marker moving={}", self.marker_moving));
            self.update_button_labels();
            let _ = unsafe { InvalidateRect(self.scene, None, BOOL(1)) };
        }

        fn reorder_topmost(&mut self) {
            // Reorder only after user actions or surface creation; the 16 ms timer never churns z-order.
            let mut ordered: Vec<HWND> = self.overlays.iter().map(|overlay| overlay.hwnd).collect();
            if let Some(surface) = &self.halo {
                ordered.push(surface.host);
            }
            if let Some(surface) = &self.lens {
                ordered.push(surface.host);
            }
            for hwnd in ordered {
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
            }
        }

        fn client_to_screen(&self, x: i32, y: i32) -> (i32, i32) {
            let mut point = POINT { x, y };
            if !self.scene.0.is_null() {
                let _ = unsafe {
                    windows::Win32::Graphics::Gdi::ClientToScreen(self.scene, &mut point)
                };
            }
            (point.x, point.y)
        }

        fn tick(&mut self) {
            if self.marker_moving && self.last_marker_change.elapsed() >= Duration::from_millis(500)
            {
                self.marker_frame = (self.marker_frame + 1) % 18;
                self.last_marker_change = Instant::now();
                let _ = unsafe { InvalidateRect(self.scene, None, BOOL(1)) };
            }

            let mut cursor = POINT::default();
            if let Err(error) = unsafe { GetCursorPos(&mut cursor) } {
                if !self.cursor_read_failed {
                    self.log(format!("GetCursorPos unavailable; effects paused: {error}"));
                }
                self.cursor_read_failed = true;
                return;
            }
            if self.cursor_read_failed {
                self.log("GetCursorPos recovered; effects resumed");
            }
            self.cursor_read_failed = false;

            let mut failures = Vec::new();
            let lens_offset = self.lens_offset;
            for surface in [&mut self.halo, &mut self.lens].into_iter().flatten() {
                if let Err(error) = update_surface(surface, cursor, lens_offset) {
                    failures.push((surface.kind, error));
                }
            }
            let had_failures = !failures.is_empty();
            for (kind, error) in failures {
                self.log(format!(
                    "{} paused after native update failure: {error}",
                    kind.name()
                ));
                self.destroy_effect(kind);
            }
            if had_failures {
                if let Err(error) = self.update_filters() {
                    self.log(format!("filter refresh after native update failure failed; disabling remaining effects: {error}"));
                    self.destroy_all_effects();
                }
            }
        }

        fn inspect(&mut self) {
            let foreground_before = unsafe { GetForegroundWindow() };
            let mut cursor = POINT::default();
            let cursor_result = unsafe { GetCursorPos(&mut cursor) };
            if let Err(error) = cursor_result {
                self.log(format!("inspect: GetCursorPos failed: {error}"));
                return;
            }

            let mut lines = vec![format!(
                "inspect cursor_physical=({}, {}) foreground_before={}",
                cursor.x,
                cursor.y,
                hwnd_value(foreground_before)
            )];
            if let Some(surface) = &self.halo {
                lines.push(format!(
                    "  halo host={} mag={} source={} destination={} strength={:.0}% matrix={:?}",
                    hwnd_value(surface.host),
                    hwnd_value(surface.magnifier),
                    rect_text(surface.source),
                    rect_text(surface.destination),
                    self.strength() * 100.0,
                    inversion_matrix(self.strength()).transform
                ));
            } else {
                lines.push("  halo OFF".into());
            }
            if let Some(surface) = &self.lens {
                lines.push(format!(
                    "  lens host={} mag={} scale=2.0 source={} destination={} placement={}",
                    hwnd_value(surface.host),
                    hwnd_value(surface.magnifier),
                    rect_text(surface.source),
                    rect_text(surface.destination),
                    if self.lens_offset {
                        "offset(+120,+80)"
                    } else {
                        "centered"
                    }
                ));
            } else {
                lines.push("  lens OFF".into());
            }
            lines.push(format!(
                "  reference_overlays_visible={} filters_exclude_overlays_and_all_active_hosts=true resources_created={} resources_destroyed={}",
                self.overlays_visible, self.create_count, self.destroy_count
            ));
            lines.push(self.pixel_observation(cursor));

            for line in lines {
                self.log(line);
            }
            let foreground_after = unsafe { GetForegroundWindow() };
            self.log(format!(
                "inspect foreground_after={} preserved={}",
                hwnd_value(foreground_after),
                foreground_before == foreground_after
            ));
        }

        fn save_composed_readback(&mut self) {
            let foreground_before = unsafe { GetForegroundWindow() };
            let mut cursor = POINT::default();
            if let Err(error) = unsafe { GetCursorPos(&mut cursor) } {
                self.log(format!("F12 readback: GetCursorPos failed: {error}"));
                return;
            }

            let observation_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("target")
                .join("cursor-effects-smoke")
                .join("observations");
            if let Err(error) = create_dir_all(&observation_dir) {
                self.log(format!(
                    "F12 readback: could not create {}: {error}",
                    observation_dir.display()
                ));
                return;
            }
            let observation_dir = match std::fs::canonicalize(&observation_dir) {
                Ok(path) => path,
                Err(error) => {
                    self.log(format!(
                        "F12 readback: could not resolve output directory: {error}"
                    ));
                    return;
                }
            };
            let capture_id = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();

            let halo_rect = centered_rect(cursor, HALO_RADIUS, HALO_RADIUS);
            let source_rect = centered_rect(cursor, LENS_SOURCE_HALF, LENS_SOURCE_HALF);
            let lens_host_destination =
                self.lens.as_ref().map(|lens| (lens.host, lens.destination));
            let lens_rect = if let Some((lens_host, last_destination)) = lens_host_destination {
                let mut rect = RECT::default();
                match unsafe {
                    windows::Win32::UI::WindowsAndMessaging::GetWindowRect(lens_host, &mut rect)
                } {
                    Ok(()) => Some(rect),
                    Err(error) => {
                        self.log(format!(
                            "F12 readback: GetWindowRect(lens host) failed ({error}); using last destination {}",
                            rect_text(last_destination)
                        ));
                        Some(last_destination)
                    }
                }
            } else {
                None
            };
            // The default offset lens occupies cursor+[40..200] x cursor+[0..160], while the
            // halo occupies cursor+[-60..60] in both axes. This fixed bounded rectangle contains
            // both circles with surrounding scene pixels and remains useful in centered mode.
            let context_rect = RECT {
                left: cursor.x.saturating_sub(80),
                top: cursor.y.saturating_sub(80),
                right: cursor.x.saturating_add(240),
                bottom: cursor.y.saturating_add(200),
            };

            self.log(format!(
                "F12 readback: BitBlt(SRCCOPY|CAPTUREBLT) is one-shot; the Windows compositor may omit layered WC_MAGNIFIER output. API success alone is not proof, but saved pixels can support inspection if composed output is included. cursor_physical=({}, {}) halo_active={} halo_rect={} lens_active={} lens_source={} lens_destination={} context_rect={}",
                cursor.x,
                cursor.y,
                self.halo.is_some(),
                rect_text(halo_rect),
                self.lens.is_some(),
                rect_text(source_rect),
                lens_rect.map(rect_text).unwrap_or_else(|| "OFF".into()),
                rect_text(context_rect)
            ));

            let mut captures = vec![
                (
                    "halo-120x120",
                    halo_rect,
                    vec![
                        ("hotspot", HALO_RADIUS, HALO_RADIUS),
                        ("inside_top", HALO_RADIUS, 10),
                        ("outside_corner", 0, 0),
                    ],
                ),
                (
                    "lens-source-80x80",
                    source_rect,
                    vec![("cursor_source_center", LENS_SOURCE_HALF, LENS_SOURCE_HALF)],
                ),
                (
                    "context-320x280",
                    context_rect,
                    vec![
                        ("halo_hotspot", 80, 80),
                        ("halo_inside_top", 80, 30),
                        ("halo_outside_top", 80, 10),
                        (
                            if self.lens_offset {
                                "offset_lens_center"
                            } else {
                                "centered_lens_center"
                            },
                            if self.lens_offset { 200 } else { 80 },
                            if self.lens_offset { 160 } else { 80 },
                        ),
                        ("context_background", 310, 20),
                    ],
                ),
            ];
            if let Some(rect) = lens_rect {
                captures.insert(
                    1,
                    (
                        "lens-destination-160x160",
                        rect,
                        vec![
                            ("lens_center", 80, 80),
                            ("lens_inside_left", 20, 80),
                            ("lens_top", 80, 10),
                            ("lens_corner", 0, 0),
                        ],
                    ),
                );
            }

            for (name, rect, samples) in captures {
                let path = observation_dir.join(format!("readback-{capture_id}-{name}.bmp"));
                match save_desktop_rect_bmp(&path, rect, &samples) {
                    Ok(sample_text) => self.log(format!(
                        "F12 readback saved; inspect whether WC_MAGNIFIER output is present: name={name} rect={} path={} rgb_samples=[{}]",
                        rect_text(rect),
                        path.display(),
                        sample_text
                    )),
                    Err(error) => self.log(format!(
                        "F12 readback failed: name={name} rect={} path={} error={error}",
                        rect_text(rect),
                        path.display()
                    )),
                }
            }
            let foreground_after = unsafe { GetForegroundWindow() };
            self.log(format!(
                "F12 readback complete foreground_before={} foreground_after={} preserved={}",
                hwnd_value(foreground_before),
                hwnd_value(foreground_after),
                foreground_before == foreground_after
            ));
        }

        fn pixel_observation(&self, cursor: POINT) -> String {
            let pixel_dc = unsafe { GetDC(HWND::default()) };
            if pixel_dc.0.is_null() {
                return "  desktop GetDC failed; pixel readback unavailable".into();
            }
            let color = unsafe { GetPixel(pixel_dc, cursor.x, cursor.y) };
            let _ = unsafe { ReleaseDC(HWND::default(), pixel_dc) };
            if color.0 == u32::MAX {
                return "  desktop GetPixel returned CLR_INVALID; composed pixel readback inconclusive".into();
            }
            let rgb = color_to_rgb(color);
            let patch = self.palette_patch_at_cursor(cursor);
            let expected = patch.map(|(name, source)| {
                let transformed = transform_rgb(source, self.strength());
                let mode = if self.lens.is_some() {
                    "lens active; composed value depends on destination overlap".to_string()
                } else if self.halo.is_some() {
                    format!(
                        "halo_expected_at_{}%={transformed:?}",
                        self.strength() * 100.0
                    )
                } else {
                    format!("native_expected={source:?}")
                };
                format!(" patch={name} {mode}")
            });
            format!(
                "  desktop GetPixel at cursor rgb={rgb:?};{} readback_is_advisory_composition_may_omit_overlay",
                expected.unwrap_or_else(|| " cursor_not_over_static_palette".into())
            )
        }

        fn palette_patch_at_cursor(&self, cursor: POINT) -> Option<(&'static str, [u8; 3])> {
            let mut client = cursor;
            if !unsafe { ScreenToClient(self.scene, &mut client) }.as_bool() {
                return None;
            }
            let patches = [
                ("black", 60, [0, 0, 0]),
                ("white", 220, [255, 255, 255]),
                ("red", 380, [255, 0, 0]),
                ("green", 540, [0, 255, 0]),
                ("blue", 700, [0, 0, 255]),
            ];
            patches
                .into_iter()
                .find(|(_, left, _)| {
                    client.x >= *left && client.x < *left + 140 && (165..295).contains(&client.y)
                })
                .map(|(name, _, color)| (name, color))
        }

        fn paint_scene(&self, hwnd: HWND) {
            let mut paint = PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
            if hdc.0.is_null() {
                return;
            }
            let background = RECT {
                left: 0,
                top: 0,
                right: SCENE_WIDTH,
                bottom: SCENE_HEIGHT,
            };
            let _ = fill_rect(hdc, background, [31, 34, 42]);
            let _ = draw_text(
                hdc,
                20,
                65,
                "Cursor Effects Smoke Scene - Windows Magnification API candidate",
                [245, 245, 245],
            );
            let _ = draw_text(
                hdc,
                20,
                91,
                "Move over a swatch; F5 logs a desktop pixel without moving the pointer. Disable the lens to inspect halo-only output.",
                [195, 205, 220],
            );
            let _ = draw_text(
                hdc,
                20,
                116,
                "Strength: 0%, 40%, 100%. Expected at 40%: black=(102,102,102), white=(153,153,153), red=(153,102,102).",
                [195, 205, 220],
            );

            for (name, x, color) in [
                ("BLACK", 60, [0, 0, 0]),
                ("WHITE", 220, [255, 255, 255]),
                ("RED", 380, [255, 0, 0]),
                ("GREEN", 540, [0, 255, 0]),
                ("BLUE", 700, [0, 0, 255]),
            ] {
                let _ = fill_rect(
                    hdc,
                    RECT {
                        left: x,
                        top: 165,
                        right: x + 140,
                        bottom: 295,
                    },
                    color,
                );
                let label_color = if name == "BLACK" || name == "RED" || name == "BLUE" {
                    [255, 255, 255]
                } else {
                    [15, 15, 15]
                };
                let _ = draw_text(hdc, x + 10, 273, name, label_color);
            }

            let _ = draw_text(hdc, 880, 160, "CHECKER + TEXT", [245, 245, 245]);
            for row in 0..8 {
                for column in 0..8 {
                    let color = if (row + column) % 2 == 0 {
                        [245, 245, 245]
                    } else {
                        [25, 25, 25]
                    };
                    let x = 880 + column * 16;
                    let y = 184 + row * 16;
                    let _ = fill_rect(
                        hdc,
                        RECT {
                            left: x,
                            top: y,
                            right: x + 16,
                            bottom: y + 16,
                        },
                        color,
                    );
                }
            }
            let _ = draw_text(
                hdc,
                60,
                325,
                "Readable text: The quick brown fox jumps over 1234567890.",
                [245, 245, 245],
            );
            let _ = draw_text(
                hdc,
                60,
                350,
                "Reference overlays are separate named passive windows. Toggle them to inspect filter exclusions.",
                [190, 205, 220],
            );

            for column in 0..18 {
                let x = 60 + column * 48;
                let color = if column % 2 == 0 {
                    [48, 88, 150]
                } else {
                    [165, 75, 45]
                };
                let _ = fill_rect(
                    hdc,
                    RECT {
                        left: x,
                        top: 450,
                        right: x + 34,
                        bottom: 500,
                    },
                    color,
                );
            }
            let marker_x = 60 + self.marker_frame as i32 * 48;
            let _ = fill_rect(
                hdc,
                RECT {
                    left: marker_x,
                    top: 455,
                    right: marker_x + 22,
                    bottom: 495,
                },
                [255, 220, 0],
            );
            let _ = draw_text(
                hdc,
                60,
                510,
                "Changing marker strip (updates every 500 ms even with a stationary pointer)",
                [245, 245, 245],
            );
            let _ = draw_text(
                hdc,
                60,
                555,
                "Log: %TEMP%\\MultiLauncherCursorEffectsSmoke.log | F12 saves bounded desktop BMP readbacks.",
                [190, 205, 220],
            );
            let _ = draw_text(
                hdc,
                60,
                585,
                "Observe: cursor unchanged; circle mask; live content; exclusion; focus/click-through. F12 saves bounded readbacks.",
                [190, 205, 220],
            );
            let _ = unsafe { EndPaint(hwnd, &paint) };
        }

        fn cleanup(&mut self) {
            if self.cleaned {
                return;
            }
            self.cleaned = true;
            if !self.scene.0.is_null() {
                let _ = unsafe {
                    windows::Win32::UI::WindowsAndMessaging::KillTimer(self.scene, TIMER_ID)
                };
            }
            self.destroy_all_effects();
            let overlays: Vec<_> = self.overlays.drain(..).collect();
            for overlay in overlays {
                let result = unsafe { DestroyWindow(overlay.hwnd) };
                self.log(format!(
                    "destroyed reference {} hwnd={} result={result:?}",
                    overlay_name(overlay.kind),
                    hwnd_value(overlay.hwnd)
                ));
            }
            self.log(format!(
                "cleanup complete resources_created={} resources_destroyed={} (host HWND count; child magnifiers are destroyed with hosts)",
                self.create_count, self.destroy_count
            ));
        }
    }

    impl Drop for App {
        fn drop(&mut self) {
            self.cleanup();
        }
    }

    fn centered_rect(center: POINT, half_width: i32, half_height: i32) -> RECT {
        RECT {
            left: center.x.saturating_sub(half_width),
            top: center.y.saturating_sub(half_height),
            right: center.x.saturating_add(half_width),
            bottom: center.y.saturating_add(half_height),
        }
    }

    struct ScreenReadback {
        desktop_dc: HDC,
        memory_dc: HDC,
        bitmap: HBITMAP,
        previous_bitmap: HGDIOBJ,
        bitmap_selected: bool,
        pixel_bits: *mut c_void,
        byte_count: usize,
    }

    impl ScreenReadback {
        fn new(width: i32, height: i32) -> Result<Self, String> {
            let byte_count = (width as usize)
                .checked_mul(height as usize)
                .and_then(|pixel_count| pixel_count.checked_mul(4))
                .ok_or_else(|| "readback byte size overflow".to_string())?;
            let mut capture = Self {
                desktop_dc: HDC::default(),
                memory_dc: HDC::default(),
                bitmap: HBITMAP::default(),
                previous_bitmap: HGDIOBJ::default(),
                bitmap_selected: false,
                pixel_bits: std::ptr::null_mut(),
                byte_count,
            };
            capture.desktop_dc = unsafe { GetDC(HWND::default()) };
            if capture.desktop_dc.is_invalid() {
                return Err(format!(
                    "GetDC(desktop) returned an invalid handle (GetLastError={})",
                    unsafe { GetLastError().0 }
                ));
            }
            capture.memory_dc = unsafe { CreateCompatibleDC(capture.desktop_dc) };
            if capture.memory_dc.is_invalid() {
                return Err(format!(
                    "CreateCompatibleDC returned an invalid handle (GetLastError={})",
                    unsafe { GetLastError().0 }
                ));
            }
            let bitmap_info = readback_bitmap_info(width, height, byte_count);
            capture.bitmap = unsafe {
                CreateDIBSection(
                    capture.desktop_dc,
                    &bitmap_info,
                    DIB_RGB_COLORS,
                    &mut capture.pixel_bits,
                    HANDLE::default(),
                    0,
                )
            }
            .map_err(|error| format!("CreateDIBSection({width}x{height}) failed: {error}"))?;
            if capture.pixel_bits.is_null() {
                return Err("CreateDIBSection returned a null pixel buffer".into());
            }
            capture.previous_bitmap = unsafe { SelectObject(capture.memory_dc, capture.bitmap) };
            if capture.previous_bitmap.is_invalid() {
                return Err(format!(
                    "SelectObject(memory_dc, bitmap) failed (GetLastError={})",
                    unsafe { GetLastError().0 }
                ));
            }
            capture.bitmap_selected = true;
            Ok(capture)
        }

        fn copy_rect(&mut self, rect: RECT, width: i32, height: i32) -> Result<&[u8], String> {
            let rop = ROP_CODE(SRCCOPY.0 | CAPTUREBLT.0);
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
                    rop,
                )
            }
            .map_err(|error| format!("BitBlt(SRCCOPY|CAPTUREBLT) failed: {error}"))?;
            if !unsafe { GdiFlush() }.as_bool() {
                return Err(format!(
                    "GdiFlush failed before reading DIB pixels (GetLastError={})",
                    unsafe { GetLastError().0 }
                ));
            }
            // SAFETY: CreateDIBSection allocated this buffer for byte_count bytes; it remains
            // valid until bitmap deletion, and GdiFlush completed queued GDI writes before reading.
            Ok(unsafe { std::slice::from_raw_parts(self.pixel_bits.cast(), self.byte_count) })
        }
    }

    impl Drop for ScreenReadback {
        fn drop(&mut self) {
            if self.bitmap_selected {
                let restored = unsafe { SelectObject(self.memory_dc, self.previous_bitmap) };
                if restored.is_invalid() {
                    eprintln!(
                        "cursor-effects-smoke: readback SelectObject restore failed (GetLastError={})",
                        unsafe { GetLastError().0 }
                    );
                    if !self.memory_dc.is_invalid() && unsafe { DeleteDC(self.memory_dc) }.as_bool()
                    {
                        self.memory_dc = HDC::default();
                        self.bitmap_selected = false;
                    } else {
                        eprintln!("cursor-effects-smoke: readback DeleteDC fallback failed");
                    }
                } else {
                    self.bitmap_selected = false;
                }
            }
            if !self.bitmap.is_invalid() && !unsafe { DeleteObject(self.bitmap) }.as_bool() {
                eprintln!(
                    "cursor-effects-smoke: readback DeleteObject(bitmap) failed (GetLastError={})",
                    unsafe { GetLastError().0 }
                );
            }
            if !self.memory_dc.is_invalid() && !unsafe { DeleteDC(self.memory_dc) }.as_bool() {
                eprintln!(
                    "cursor-effects-smoke: readback DeleteDC failed (GetLastError={})",
                    unsafe { GetLastError().0 }
                );
            }
            if !self.desktop_dc.is_invalid()
                && unsafe { ReleaseDC(HWND::default(), self.desktop_dc) } == 0
            {
                eprintln!(
                    "cursor-effects-smoke: readback ReleaseDC(desktop) failed (GetLastError={})",
                    unsafe { GetLastError().0 }
                );
            }
        }
    }

    fn save_desktop_rect_bmp(
        path: &Path,
        rect: RECT,
        samples: &[(&str, i32, i32)],
    ) -> Result<String, String> {
        let width = rect
            .right
            .checked_sub(rect.left)
            .ok_or_else(|| "rectangle width overflow".to_string())?;
        let height = rect
            .bottom
            .checked_sub(rect.top)
            .ok_or_else(|| "rectangle height overflow".to_string())?;
        if width <= 0 || height <= 0 || width > READBACK_MAX_WIDTH || height > READBACK_MAX_HEIGHT {
            return Err(format!(
                "bounded readback rectangle rejected ({}x{}, maximum {}x{})",
                width, height, READBACK_MAX_WIDTH, READBACK_MAX_HEIGHT
            ));
        }

        let mut capture = ScreenReadback::new(width, height)?;
        let pixels = capture.copy_rect(rect, width, height)?;
        write_bmp(path, width, height, pixels)?;
        Ok(format_samples(pixels, width, height, samples))
    }

    fn write_bmp(path: &Path, width: i32, height: i32, pixels: &[u8]) -> Result<(), String> {
        let image_size = (width as u32)
            .checked_mul(height as u32)
            .and_then(|pixel_count| pixel_count.checked_mul(4))
            .ok_or_else(|| "BMP image size overflow".to_string())?;
        if pixels.len() != image_size as usize {
            return Err(format!(
                "BMP pixel buffer length {} did not match expected {image_size}",
                pixels.len()
            ));
        }
        let file_size = 14_u32
            .checked_add(40)
            .and_then(|header_size| header_size.checked_add(image_size))
            .ok_or_else(|| "BMP file size overflow".to_string())?;
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

    fn readback_bitmap_info(width: i32, height: i32, byte_count: usize) -> BITMAPINFO {
        BITMAPINFO {
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
        }
    }

    fn format_samples(
        pixels: &[u8],
        width: i32,
        height: i32,
        samples: &[(&str, i32, i32)],
    ) -> String {
        samples
            .iter()
            .map(|(name, x, y)| {
                let x = (*x).clamp(0, width - 1) as usize;
                let y = (*y).clamp(0, height - 1) as usize;
                let index = ((y * width as usize) + x) * 4;
                let rgb = [pixels[index + 2], pixels[index + 1], pixels[index]];
                format!("{name}={rgb:?}")
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn update_surface(
        surface: &mut EffectSurface,
        cursor: POINT,
        lens_offset: bool,
    ) -> Result<(), String> {
        let (source, destination) = match surface.kind {
            EffectKind::Halo => {
                let radius = HALO_RADIUS;
                (
                    RECT {
                        left: cursor.x.saturating_sub(radius),
                        top: cursor.y.saturating_sub(radius),
                        right: cursor.x.saturating_add(radius),
                        bottom: cursor.y.saturating_add(radius),
                    },
                    RECT {
                        left: cursor.x.saturating_sub(radius),
                        top: cursor.y.saturating_sub(radius),
                        right: cursor.x.saturating_add(radius),
                        bottom: cursor.y.saturating_add(radius),
                    },
                )
            }
            EffectKind::Lens => {
                let half = LENS_SOURCE_HALF;
                let center_x = if lens_offset {
                    cursor.x.saturating_add(LENS_OFFSET_X)
                } else {
                    cursor.x
                };
                let center_y = if lens_offset {
                    cursor.y.saturating_add(LENS_OFFSET_Y)
                } else {
                    cursor.y
                };
                (
                    RECT {
                        left: cursor.x.saturating_sub(half),
                        top: cursor.y.saturating_sub(half),
                        right: cursor.x.saturating_add(half),
                        bottom: cursor.y.saturating_add(half),
                    },
                    RECT {
                        left: center_x.saturating_sub(LENS_DIAMETER / 2),
                        top: center_y.saturating_sub(LENS_DIAMETER / 2),
                        right: center_x.saturating_add(LENS_DIAMETER / 2),
                        bottom: center_y.saturating_add(LENS_DIAMETER / 2),
                    },
                )
            }
        };

        if !unsafe { MagSetWindowSource(surface.magnifier, source) }.as_bool() {
            return Err(format!(
                "MagSetWindowSource failed (GetLastError={}) source={}",
                unsafe { GetLastError().0 },
                rect_text(source)
            ));
        }
        if !unsafe { InvalidateRect(surface.magnifier, None, BOOL(1)) }.as_bool() {
            return Err(format!(
                "InvalidateRect(WC_MAGNIFIER) failed after source update (GetLastError={})",
                unsafe { GetLastError().0 }
            ));
        }
        if !surface.visible
            || surface.destination.left != destination.left
            || surface.destination.top != destination.top
        {
            let result = if surface.visible {
                unsafe {
                    SetWindowPos(
                        surface.host,
                        HWND::default(),
                        destination.left,
                        destination.top,
                        surface.kind.diameter(),
                        surface.kind.diameter(),
                        SWP_NOACTIVATE | SWP_NOZORDER,
                    )
                }
            } else {
                unsafe {
                    SetWindowPos(
                        surface.host,
                        HWND_TOPMOST,
                        destination.left,
                        destination.top,
                        surface.kind.diameter(),
                        surface.kind.diameter(),
                        SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    )
                }
            };
            result.map_err(|error| {
                format!(
                    "position {} host at {}: {error}",
                    surface.kind.name(),
                    rect_text(destination)
                )
            })?;
            surface.visible = true;
        }
        surface.source = source;
        surface.destination = destination;
        Ok(())
    }

    fn inversion_matrix(strength: f32) -> MAGCOLOREFFECT {
        let slope = 1.0 - (2.0 * strength);
        MAGCOLOREFFECT {
            transform: [
                slope, 0.0, 0.0, 0.0, 0.0, // input red contributes to output red
                0.0, slope, 0.0, 0.0, 0.0, // input green contributes to output green
                0.0, 0.0, slope, 0.0, 0.0, // input blue contributes to output blue
                0.0, 0.0, 0.0, 1.0, 0.0, // preserve alpha
                strength, strength, strength, 0.0,
                1.0, // add the constant term to each RGB output
            ],
        }
    }

    fn transform_rgb(source: [u8; 3], strength: f32) -> [u8; 3] {
        let slope = 1.0 - 2.0 * strength;
        source.map(|channel| {
            (slope * f32::from(channel) + strength * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8
        })
    }

    #[cfg(test)]
    mod tests {
        use super::{inversion_matrix, transform_rgb};

        fn apply_mag_color_matrix(rgb: [u8; 3], matrix: &[f32; 25]) -> [u8; 3] {
            // MagSetColorEffect's documented grayscale example stores input-channel weights
            // across output columns, so this evaluates its row-vector affine convention.
            let input = [
                f32::from(rgb[0]) / 255.0,
                f32::from(rgb[1]) / 255.0,
                f32::from(rgb[2]) / 255.0,
                1.0,
                1.0,
            ];
            std::array::from_fn(|output| {
                let value = (0..5)
                    .map(|row| input[row] * matrix[row * 5 + output])
                    .sum::<f32>();
                (value * 255.0).round().clamp(0.0, 255.0) as u8
            })
        }

        #[test]
        fn magnification_matrix_uses_the_documented_row_vector_affine_layout() {
            for (strength, black, white, red) in [
                (0.0, [0, 0, 0], [255, 255, 255], [255, 0, 0]),
                (0.4, [102, 102, 102], [153, 153, 153], [153, 102, 102]),
                (1.0, [255, 255, 255], [0, 0, 0], [0, 255, 255]),
            ] {
                let matrix = inversion_matrix(strength);
                assert_eq!(apply_mag_color_matrix([0, 0, 0], &matrix.transform), black);
                assert_eq!(
                    apply_mag_color_matrix([255, 255, 255], &matrix.transform),
                    white
                );
                assert_eq!(apply_mag_color_matrix([255, 0, 0], &matrix.transform), red);
                assert_eq!(
                    apply_mag_color_matrix([255, 0, 0], &matrix.transform),
                    transform_rgb([255, 0, 0], strength)
                );
            }
            let transform = inversion_matrix(0.4).transform;
            assert_eq!(&transform[20..23], &[0.4, 0.4, 0.4]);
            assert_eq!(&transform[4..5], &[0.0]);
            assert_eq!(&transform[9..10], &[0.0]);
            assert_eq!(&transform[14..15], &[0.0]);
        }
    }

    fn fill_rect(hdc: windows::Win32::Graphics::Gdi::HDC, rect: RECT, rgb: [u8; 3]) -> bool {
        let brush = unsafe { CreateSolidBrush(color_ref(rgb)) };
        if brush.0.is_null() {
            return false;
        }
        let result = unsafe { FillRect(hdc, &rect, brush) } != 0;
        let _ = unsafe { DeleteObject(brush) };
        result
    }

    fn draw_text(
        hdc: windows::Win32::Graphics::Gdi::HDC,
        x: i32,
        y: i32,
        text: &str,
        rgb: [u8; 3],
    ) -> bool {
        let text = wide(text);
        unsafe {
            let _ = SetBkMode(hdc, TRANSPARENT);
            let _ = SetTextColor(hdc, color_ref(rgb));
            TextOutW(hdc, x, y, &text).as_bool()
        }
    }

    fn color_ref([red, green, blue]: [u8; 3]) -> COLORREF {
        COLORREF(u32::from(red) | (u32::from(green) << 8) | (u32::from(blue) << 16))
    }

    fn color_to_rgb(color: COLORREF) -> [u8; 3] {
        [
            (color.0 & 0xff) as u8,
            ((color.0 >> 8) & 0xff) as u8,
            ((color.0 >> 16) & 0xff) as u8,
        ]
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn rect_text(rect: RECT) -> String {
        format!(
            "[{},{}..{},{}]",
            rect.left, rect.top, rect.right, rect.bottom
        )
    }

    fn hwnd_value(hwnd: HWND) -> isize {
        hwnd.0 as isize
    }

    fn overlay_name(kind: OverlayKind) -> &'static str {
        match kind {
            OverlayKind::Hud => "fake-hud",
            OverlayKind::Crosshair => "fake-crosshair",
            OverlayKind::HorizontalGuide => "fake-horizontal-guide",
            OverlayKind::VerticalGuide => "fake-vertical-guide",
        }
    }

    unsafe extern "system" fn scene_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == WM_NCCREATE {
            // SAFETY: Win32 supplies a CREATESTRUCTW pointer in lParam for WM_NCCREATE.
            let create = unsafe {
                &*(lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW)
            };
            let _ =
                unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize) };
            return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
        }
        if message == WM_CLOSE {
            let _ = unsafe { DestroyWindow(hwnd) };
            return LRESULT(0);
        }
        if message == WM_DESTROY {
            let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const RefCell<App>;
            // SAFETY: the App state is boxed before HWND creation and remains alive until after
            // the message loop destroys the scene HWND.
            if let Some(state) = unsafe { pointer.as_ref() } {
                if let Ok(mut app) = state.try_borrow_mut() {
                    app.cleanup();
                } else {
                    eprintln!(
                        "cursor-effects-smoke: WM_DESTROY cleanup deferred until the message loop exits"
                    );
                }
            }
            unsafe { PostQuitMessage(0) };
            return LRESULT(0);
        }

        match message {
            WM_COMMAND | WM_KEYDOWN | WM_TIMER | WM_PAINT => {}
            _ => return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        }

        let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const RefCell<App>;
        // SAFETY: the App state is boxed before HWND creation and remains alive until after
        // the message loop destroys the scene HWND.
        let Some(state) = (unsafe { pointer.as_ref() }) else {
            return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
        };

        match message {
            WM_COMMAND => {
                let Ok(mut app) = state.try_borrow_mut() else {
                    return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
                };
                let before = unsafe { GetForegroundWindow() };
                app.handle_command((wparam.0 & 0xffff) as i32);
                let after = unsafe { GetForegroundWindow() };
                app.log(format!(
                    "operation=button:{} foreground_before={} foreground_after={} preserved={}",
                    wparam.0 & 0xffff,
                    hwnd_value(before),
                    hwnd_value(after),
                    before == after
                ));
                LRESULT(0)
            }
            WM_KEYDOWN => {
                let Ok(mut app) = state.try_borrow_mut() else {
                    return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
                };
                let before = unsafe { GetForegroundWindow() };
                if app.handle_key(wparam.0) {
                    let after = unsafe { GetForegroundWindow() };
                    app.log(format!(
                        "operation=key:{} foreground_before={} foreground_after={} preserved={}",
                        wparam.0,
                        hwnd_value(before),
                        hwnd_value(after),
                        before == after
                    ));
                    LRESULT(0)
                } else {
                    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
                }
            }
            WM_TIMER if wparam.0 == TIMER_ID => {
                if let Ok(mut app) = state.try_borrow_mut() {
                    app.tick();
                }
                LRESULT(0)
            }
            WM_PAINT => {
                if let Ok(app) = state.try_borrow() {
                    app.paint_scene(hwnd);
                    LRESULT(0)
                } else {
                    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
                }
            }
            _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        }
    }

    unsafe extern "system" fn overlay_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == WM_NCCREATE {
            let create = unsafe {
                &*(lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW)
            };
            let _ =
                unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize) };
        }
        match message {
            WM_NCHITTEST => {
                LRESULT(windows::Win32::UI::WindowsAndMessaging::HTTRANSPARENT as isize)
            }
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_PAINT => {
                let kind = OverlayKind::from_id(unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) });
                if let Some(kind) = kind {
                    // The overlay's content is constant and the enum tag is stored in GWLP_USERDATA.
                    let mut paint = PAINTSTRUCT::default();
                    let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
                    if !hdc.0.is_null() {
                        paint_overlay_contents(hdc, kind);
                        let _ = unsafe { EndPaint(hwnd, &paint) };
                    }
                }
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        }
    }

    fn paint_overlay_contents(hdc: windows::Win32::Graphics::Gdi::HDC, kind: OverlayKind) {
        match kind {
            OverlayKind::Hud => {
                let _ = fill_rect(
                    hdc,
                    RECT {
                        left: 0,
                        top: 0,
                        right: 285,
                        bottom: 42,
                    },
                    [8, 42, 92],
                );
                let _ = draw_text(
                    hdc,
                    8,
                    13,
                    "FAKE HUD - excluded by each magnifier",
                    [255, 255, 255],
                );
            }
            OverlayKind::Crosshair => {
                let _ = fill_rect(
                    hdc,
                    RECT {
                        left: 0,
                        top: 0,
                        right: 90,
                        bottom: 90,
                    },
                    [0, 0, 0],
                );
                let _ = fill_rect(
                    hdc,
                    RECT {
                        left: 43,
                        top: 6,
                        right: 47,
                        bottom: 84,
                    },
                    [255, 230, 20],
                );
                let _ = fill_rect(
                    hdc,
                    RECT {
                        left: 6,
                        top: 43,
                        right: 84,
                        bottom: 47,
                    },
                    [255, 230, 20],
                );
            }
            OverlayKind::HorizontalGuide => {
                let _ = fill_rect(
                    hdc,
                    RECT {
                        left: 0,
                        top: 0,
                        right: 970,
                        bottom: 5,
                    },
                    [0, 0, 0],
                );
                let _ = fill_rect(
                    hdc,
                    RECT {
                        left: 0,
                        top: 1,
                        right: 970,
                        bottom: 4,
                    },
                    [255, 255, 255],
                );
            }
            OverlayKind::VerticalGuide => {
                let _ = fill_rect(
                    hdc,
                    RECT {
                        left: 0,
                        top: 0,
                        right: 5,
                        bottom: 450,
                    },
                    [0, 0, 0],
                );
                let _ = fill_rect(
                    hdc,
                    RECT {
                        left: 1,
                        top: 0,
                        right: 4,
                        bottom: 450,
                    },
                    [255, 255, 255],
                );
            }
        }
    }

    struct MagnificationSession;

    impl Drop for MagnificationSession {
        fn drop(&mut self) {
            if !unsafe { MagUninitialize() }.as_bool() {
                eprintln!(
                    "cursor-effects-smoke: MagUninitialize returned FALSE (GetLastError={})",
                    unsafe { GetLastError().0 }
                );
            }
        }
    }

    fn register_classes(instance: HINSTANCE) -> Result<(), String> {
        for (name, procedure) in [
            (
                SCENE_CLASS,
                scene_proc as unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
            ),
            (
                OVERLAY_CLASS,
                overlay_proc as unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
            ),
            (
                HOST_CLASS,
                host_proc as unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
            ),
        ] {
            let class = WNDCLASSW {
                lpfnWndProc: Some(procedure),
                hInstance: instance,
                lpszClassName: name,
                ..Default::default()
            };
            if unsafe { RegisterClassW(&class) } == 0 {
                return Err(format!(
                    "RegisterClassW failed for class (GetLastError={})",
                    unsafe { GetLastError().0 }
                ));
            }
        }
        Ok(())
    }

    unsafe extern "system" fn host_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_NCHITTEST => {
                LRESULT(windows::Win32::UI::WindowsAndMessaging::HTTRANSPARENT as isize)
            }
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        }
    }

    pub fn run() -> Result<(), String> {
        match unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) } {
            Ok(()) => println!(
                "cursor-effects-smoke: per-monitor-v2 DPI awareness set before window creation"
            ),
            Err(error) => eprintln!(
                "cursor-effects-smoke: DPI-awareness request was not accepted: {error}; position logs remain signed physical POINT/RECT values"
            ),
        }
        let instance = HINSTANCE(
            unsafe { GetModuleHandleW(None) }
                .map_err(|error| format!("GetModuleHandleW: {error}"))?
                .0,
        );
        register_classes(instance)?;

        if !unsafe { MagInitialize() }.as_bool() {
            return Err(format!(
                "MagInitialize returned FALSE (GetLastError={})",
                unsafe { GetLastError().0 }
            ));
        }
        let _magnification_session = MagnificationSession;
        let reporter = Reporter::new();
        let app = Box::new(RefCell::new(App::new(instance, reporter)));
        let mut scene = HWND::default();
        let mut result = (|| -> Result<(), String> {
            let app_pointer = (&*app) as *const RefCell<App>;
            scene = unsafe {
                CreateWindowExW(
                    Default::default(),
                    SCENE_CLASS,
                    w!("Cursor Effects Smoke Scene"),
                    WS_OVERLAPPEDWINDOW,
                    60,
                    50,
                    SCENE_WIDTH,
                    SCENE_HEIGHT,
                    HWND::default(),
                    HMENU::default(),
                    instance,
                    Some(app_pointer.cast()),
                )
            }
            .map_err(|error| format!("create ordinary scene window: {error}"))?;

            {
                let mut state = app
                    .try_borrow_mut()
                    .map_err(|_| "scene state was unexpectedly borrowed during startup")?;
                state.scene = scene;
                state.create_buttons()?;
                state.create_reference_overlays()?;
                let log_path = state.reporter.path.display().to_string();
                state.log(format!(
                    "log file={} scene_hwnd={} title=Cursor Effects Smoke Scene; reference overlay HWNDs are named and distinct",
                    log_path, hwnd_value(scene)
                ));
                state.log("API candidate: actual scene pixels through WC_MAGNIFIER; API success is not visual acceptance. No hooks, injected input, or worker threads are used.");
                state.log("controls: buttons or F5 inspect / F6 halo / F7 lens / F8 strength / F9 centered-offset / F10 reference overlays / F12 save bounded desktop readbacks / Escape exit");
                state.log("filters exclude named fake HUD, crosshair, both guides, and both active magnifier hosts; exact production class discovery is not enabled in this standalone harness");
                state
                    .log("native host titles: Cursor Effects Halo 1x; Cursor Effects Zoom Lens 2x");
                state.log("initial color matrix and visual observations will be printed when halo is created");

                state.toggle_effect(EffectKind::Halo);
                state.toggle_effect(EffectKind::Lens);
                state.reorder_topmost();
            }

            if unsafe { SetTimer(scene, TIMER_ID, TIMER_INTERVAL_MS, None) } == 0 {
                return Err(format!(
                    "SetTimer(16 ms) failed (GetLastError={})",
                    unsafe { GetLastError().0 }
                ));
            }

            let was_visible = unsafe { ShowWindow(scene, SW_SHOW) }.as_bool();
            let update_window_result = unsafe { UpdateWindow(scene) }.as_bool();
            let is_window = unsafe { IsWindow(scene) }.as_bool();
            let is_visible = unsafe { IsWindowVisible(scene) }.as_bool();
            let mut window_rect = RECT::default();
            let window_rect_result = unsafe { GetWindowRect(scene, &mut window_rect) };
            let mut client_rect = RECT::default();
            let client_rect_result = unsafe { GetClientRect(scene, &mut client_rect) };
            let mut title_buffer = [0_u16; 128];
            let title_length = unsafe { GetWindowTextW(scene, &mut title_buffer) };
            let title = if title_length > 0 {
                String::from_utf16_lossy(&title_buffer[..title_length as usize])
            } else {
                String::new()
            };
            if let Ok(mut state) = app.try_borrow_mut() {
                let window_geometry = match window_rect_result {
                    Ok(()) => format!(
                        "({},{}..{},{}; {}x{})",
                        window_rect.left,
                        window_rect.top,
                        window_rect.right,
                        window_rect.bottom,
                        window_rect.right - window_rect.left,
                        window_rect.bottom - window_rect.top
                    ),
                    Err(error) => format!("unavailable ({error})"),
                };
                let client_geometry = match client_rect_result {
                    Ok(()) => format!(
                        "(0,0..{},{}; {}x{})",
                        client_rect.right,
                        client_rect.bottom,
                        client_rect.right - client_rect.left,
                        client_rect.bottom - client_rect.top
                    ),
                    Err(error) => format!("unavailable ({error})"),
                };
                state.log(format!(
                    "scene startup visibility: hwnd={} is_window={} was_visible_before_show={} visible_after_show={} UpdateWindow_returned_true={}",
                    hwnd_value(scene), is_window, was_visible, is_visible, update_window_result
                ));
                state.log(format!(
                    "scene startup geometry: title_chars={} title={:?} window_rect={} client_rect={}",
                    title_length, title, window_geometry, client_geometry
                ));
            }
            let mut msg = MSG::default();
            loop {
                let message_result = unsafe { GetMessageW(&mut msg, HWND::default(), 0, 0) }.0;
                if message_result == 0 {
                    break;
                }
                if message_result < 0 {
                    return Err(format!("GetMessageW failed (GetLastError={})", unsafe {
                        GetLastError().0
                    }));
                }
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            Ok(())
        })();

        // Always finish teardown on the owning UI thread, including WM_DESTROY when a nested
        // RefCell borrow forced cleanup to be deferred until after dispatch.
        if let Ok(mut state) = app.try_borrow_mut() {
            state.cleanup();
        } else {
            eprintln!(
                "cursor-effects-smoke: state remained borrowed after the message loop; cleanup deferred to App::drop"
            );
        }
        if !scene.0.is_null() && unsafe { IsWindow(scene) }.as_bool() {
            if let Err(error) = unsafe { DestroyWindow(scene) } {
                let detail = format!("DestroyWindow(scene) during final cleanup failed: {error}");
                if let Ok(mut state) = app.try_borrow_mut() {
                    state.log(&detail);
                }
                if result.is_ok() {
                    result = Err(detail);
                }
            }
        }
        result
    }
}
