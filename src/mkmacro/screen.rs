//! Platform-independent screen capture geometry used by image and pixel search.
//! Coordinates in a [`CapturedRegion`] are local; `origin` converts them back to
//! virtual-desktop coordinates (and may therefore be negative).
use crate::mkmacro::{
    DiagnosticKind, ExecResult, ExecutionDiagnostic, MkCoordinateTarget, MkImagePayload, MkPoint,
    MkValue, MkWindowMatcher, RuntimeVariables, ScreenBackend,
};
use image::RgbaImage;
use std::sync::Arc;

/// Runtime-only variable holding the result of the most recent search for one asset.
pub(crate) fn image_result_variable(asset_id: u64) -> String {
    format!("__image.{asset_id}")
}

trait WindowsGeometry: Send + Sync {
    /// `(x, y, width, height)`, in desktop pixels. Width/height remain signed so
    /// malformed platform data can be diagnosed rather than silently cast.
    fn virtual_desktop(&self) -> ExecResult<(i32, i32, i32, i32)>;
    fn foreground_window(&self) -> ExecResult<Option<isize>>;
    fn client_origin(&self, hwnd: isize) -> ExecResult<MkPoint>;
}

trait VisualSearch: Send + Sync {
    fn find_image(&self, macro_id: u64, payload: &MkImagePayload) -> ExecResult<Option<MkPoint>>;
    fn read_pixel(&self, point: MkPoint) -> ExecResult<[u8; 4]>;
}

/// Parse the persisted pixel-check color. The only accepted representation is
/// canonical CSS-style RGB (`#RRGGBB`); callers may use [`format_rgb`] to
/// normalize letter case after a successful parse.
pub fn parse_rgb(value: &str) -> ExecResult<[u8; 3]> {
    if value.len() != 7 || !value.starts_with('#') {
        return Err(ExecutionDiagnostic::new(
            DiagnosticKind::InvalidTarget,
            format!("invalid pixel color '{value}': expected #RRGGBB"),
        )
        .context("color", value));
    }
    let digits = &value[1..];
    if !digits.is_ascii() || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ExecutionDiagnostic::new(
            DiagnosticKind::InvalidTarget,
            format!("invalid pixel color '{value}': expected hexadecimal #RRGGBB"),
        )
        .context("color", value));
    }
    let channel = |at: usize| {
        u8::from_str_radix(&digits[at..at + 2], 16).map_err(|_| {
            ExecutionDiagnostic::new(
                DiagnosticKind::InvalidTarget,
                format!("invalid pixel color '{value}': expected hexadecimal #RRGGBB"),
            )
            .context("color", value)
        })
    };
    Ok([channel(0)?, channel(2)?, channel(4)?])
}

pub fn format_rgb(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

/// Windows coordinate resolver. OS geometry and visual lookup are injected so
/// coordinate behavior can be tested without a window station or live desktop.
pub struct WindowsScreenBackend {
    geometry: Arc<dyn WindowsGeometry>,
    visual: Arc<dyn VisualSearch>,
}

impl WindowsScreenBackend {
    #[cfg(windows)]
    pub fn system() -> Self {
        Self {
            geometry: Arc::new(SystemWindowsGeometry),
            visual: Arc::new(SystemVisualSearch),
        }
    }

    #[cfg(test)]
    fn new(geometry: Arc<dyn WindowsGeometry>, visual: Arc<dyn VisualSearch>) -> Self {
        Self { geometry, visual }
    }

    fn bounds(&self) -> ExecResult<(i32, i32, i32, i32)> {
        let (x, y, width, height) = self.geometry.virtual_desktop()?;
        if width <= 0 || height <= 0 {
            return Err(invalid(format!(
                "Windows virtual desktop has invalid dimensions {width}x{height}"
            ))
            .context("backend", "WindowsScreenBackend")
            .context("action", "read virtual desktop"));
        }
        let max_x = x.checked_add(width - 1).ok_or_else(|| {
            invalid("Windows virtual desktop X bounds overflow")
                .context("backend", "WindowsScreenBackend")
        })?;
        let max_y = y.checked_add(height - 1).ok_or_else(|| {
            invalid("Windows virtual desktop Y bounds overflow")
                .context("backend", "WindowsScreenBackend")
        })?;
        Ok((x, y, max_x, max_y))
    }

    fn clamp_to_desktop(&self, point: MkPoint) -> ExecResult<MkPoint> {
        let (min_x, min_y, max_x, max_y) = self.bounds()?;
        Ok(MkPoint {
            x: point.x.clamp(min_x, max_x),
            y: point.y.clamp(min_y, max_y),
        })
    }

    fn require_on_desktop(&self, point: MkPoint) -> ExecResult<MkPoint> {
        let (min_x, min_y, max_x, max_y) = self.bounds()?;
        if point.x < min_x || point.x > max_x || point.y < min_y || point.y > max_y {
            return Err(ExecutionDiagnostic::new(
                DiagnosticKind::InvalidTarget,
                "Requested coordinate lies outside the virtual desktop",
            )
            .context("coordinate", format!("{},{}", point.x, point.y))
            .context(
                "virtual_desktop",
                format!("{min_x},{min_y}..{max_x},{max_y}"),
            ));
        }
        Ok(point)
    }
}

impl ScreenBackend for WindowsScreenBackend {
    fn resolve(
        &self,
        target: &MkCoordinateTarget,
        variables: &RuntimeVariables,
    ) -> ExecResult<MkPoint> {
        match target {
            MkCoordinateTarget::Screen { point } => self.require_on_desktop(*point),
            MkCoordinateTarget::ActiveWindow { point } => {
                let hwnd = self.geometry.foreground_window()?.ok_or_else(|| {
                    ExecutionDiagnostic::new(
                        DiagnosticKind::TargetNotFound,
                        "Active window coordinate target requires an active window",
                    )
                    .context("backend", "WindowsScreenBackend")
                    .context("action", "resolve active-window client point")
                })?;
                let origin = self.geometry.client_origin(hwnd)?;
                let desktop = MkPoint {
                    x: origin
                        .x
                        .checked_add(point.x)
                        .ok_or_else(|| invalid("active-window client X coordinate overflow"))?,
                    y: origin
                        .y
                        .checked_add(point.y)
                        .ok_or_else(|| invalid("active-window client Y coordinate overflow"))?,
                };
                self.require_on_desktop(desktop)
            }
            MkCoordinateTarget::Variable { name } => match variables.get(name) {
                Some(MkValue::Point(point)) => Ok(*point),
                Some(value) => Err(type_mismatch(name, value)),
                None => Err(ExecutionDiagnostic::new(
                    DiagnosticKind::TargetNotFound,
                    format!("point variable '{name}' is missing"),
                )
                .context("variable", name)
                .context("expected", "Point")),
            },
            MkCoordinateTarget::Image { asset_id, offset } => {
                let key = image_result_variable(*asset_id);
                match variables.get(&key) {
                    Some(MkValue::Point(point)) => Ok(MkPoint {
                        x: point.x.checked_add(offset.x).ok_or_else(|| {
                            invalid(format!("image asset {asset_id} X offset overflow"))
                        })?,
                        y: point.y.checked_add(offset.y).ok_or_else(|| {
                            invalid(format!("image asset {asset_id} Y offset overflow"))
                        })?,
                    }),
                    Some(value) => Err(type_mismatch(&key, value)),
                    None => Err(ExecutionDiagnostic::new(
                        DiagnosticKind::TargetNotFound,
                        format!("image asset {asset_id} has no result in the current run"),
                    )
                    .context("asset_id", asset_id.to_string())
                    .context("variable", key)),
                }
            }
        }
    }

    fn finalize_point(&self, point: MkPoint) -> ExecResult<MkPoint> {
        self.clamp_to_desktop(point)
    }

    fn find_image(&self, macro_id: u64, payload: &MkImagePayload) -> ExecResult<Option<MkPoint>> {
        self.visual.find_image(macro_id, payload)
    }

    fn pixel_matches(
        &self,
        target: &MkCoordinateTarget,
        color: &str,
        tolerance: u8,
        variables: &RuntimeVariables,
    ) -> ExecResult<bool> {
        let point = self
            .resolve(target, variables)
            .map_err(|e| e.context("coordinate_target", format!("{target:?}")))?;
        let wanted = parse_rgb(color)?;
        let actual = self.visual.read_pixel(point).map_err(|e| {
            e.context("backend", "WindowsScreenBackend")
                .context("coordinate", format!("{},{}", point.x, point.y))
                .context("color", color)
        })?;
        Ok((0..3).all(|channel| actual[channel].abs_diff(wanted[channel]) <= tolerance))
    }
}

fn value_type(value: &MkValue) -> &'static str {
    match value {
        MkValue::String(_) => "String",
        MkValue::Number(_) => "Number",
        MkValue::Boolean(_) => "Boolean",
        MkValue::Point(_) => "Point",
        MkValue::Null => "Null",
    }
}
fn type_mismatch(name: &str, value: &MkValue) -> ExecutionDiagnostic {
    let actual = value_type(value);
    ExecutionDiagnostic::new(
        DiagnosticKind::TypeMismatch,
        format!("Variable '{name}' contains {actual}; coordinate target requires Point"),
    )
    .context("variable", name)
    .context("expected", "Point")
    .context("actual", actual)
}

#[cfg(windows)]
struct SystemWindowsGeometry;
#[cfg(windows)]
impl WindowsGeometry for SystemWindowsGeometry {
    fn virtual_desktop(&self) -> ExecResult<(i32, i32, i32, i32)> {
        use windows::Win32::UI::WindowsAndMessaging::*;
        unsafe {
            Ok((
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
            ))
        }
    }
    fn foreground_window(&self) -> ExecResult<Option<isize>> {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        let hwnd = unsafe { GetForegroundWindow() };
        Ok((!hwnd.0.is_null()).then_some(hwnd.0 as isize))
    }
    fn client_origin(&self, hwnd: isize) -> ExecResult<MkPoint> {
        use windows::Win32::{
            Foundation::{HWND, POINT},
            Graphics::Gdi::ClientToScreen,
        };
        let mut point = POINT::default();
        if !unsafe { ClientToScreen(HWND(hwnd as *mut _), &mut point) }.as_bool() {
            return Err(
                ExecutionDiagnostic::new(DiagnosticKind::Backend, "ClientToScreen failed")
                    .context("backend", "WindowsScreenBackend")
                    .context("action", "convert client origin"),
            );
        }
        Ok(MkPoint {
            x: point.x,
            y: point.y,
        })
    }
}

#[cfg(windows)]
struct SystemVisualSearch;
#[cfg(windows)]
impl VisualSearch for SystemVisualSearch {
    fn find_image(&self, _: u64, _: &MkImagePayload) -> ExecResult<Option<MkPoint>> {
        Err(ExecutionDiagnostic::new(
            DiagnosticKind::UnsupportedOperation,
            "production image lookup is not configured",
        )
        .context("backend", "WindowsScreenBackend"))
    }
    fn read_pixel(&self, point: MkPoint) -> ExecResult<[u8; 4]> {
        WindowsScreenCaptureBackend::system().read_pixel(point)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScreenRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl ScreenRect {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
    pub fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }
    pub fn right(self) -> i64 {
        i64::from(self.x) + i64::from(self.width)
    }
    pub fn bottom(self) -> i64 {
        i64::from(self.y) + i64::from(self.height)
    }
    pub fn contains(self, other: Self) -> bool {
        !other.is_empty()
            && other.x >= self.x
            && other.y >= self.y
            && other.right() <= self.right()
            && other.bottom() <= self.bottom()
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SearchRegion {
    Desktop,
    Monitor { index: usize },
    Window { matcher: MkWindowMatcher },
    ClientArea { matcher: MkWindowMatcher },
    Rectangle { rect: ScreenRect },
}
impl Default for SearchRegion {
    fn default() -> Self {
        Self::Desktop
    }
}

#[derive(Debug, Clone)]
pub struct CapturedRegion {
    pub image: RgbaImage,
    pub origin: (i32, i32),
}
impl CapturedRegion {
    pub fn rect(&self) -> ScreenRect {
        ScreenRect::new(
            self.origin.0,
            self.origin.1,
            self.image.width(),
            self.image.height(),
        )
    }
    pub fn desktop_point(&self, local: (u32, u32)) -> Option<(i32, i32)> {
        if local.0 >= self.image.width() || local.1 >= self.image.height() {
            return None;
        }
        Some((
            self.origin.0.checked_add(i32::try_from(local.0).ok()?)?,
            self.origin.1.checked_add(i32::try_from(local.1).ok()?)?,
        ))
    }
    pub fn local_point(&self, desktop: (i32, i32)) -> Option<(u32, u32)> {
        let x = desktop.0.checked_sub(self.origin.0)?;
        let y = desktop.1.checked_sub(self.origin.1)?;
        let p = (u32::try_from(x).ok()?, u32::try_from(y).ok()?);
        (p.0 < self.image.width() && p.1 < self.image.height()).then_some(p)
    }
}

/// Capture boundary. Implementations resolve windows/client areas without changing
/// their desktop origins. `cancelled` must be checked during any blocking capture.
pub trait ScreenCaptureBackend: Send + Sync {
    fn virtual_desktop(&self) -> ExecResult<ScreenRect>;
    fn region_bounds(&self, region: &SearchRegion) -> ExecResult<ScreenRect>;
    fn capture_rect(&self, rect: ScreenRect, cancelled: &dyn Fn() -> bool)
    -> ExecResult<RgbaImage>;
    fn capture(
        &self,
        region: &SearchRegion,
        cancelled: &dyn Fn() -> bool,
    ) -> ExecResult<CapturedRegion> {
        if cancelled() {
            return Err(cancelled_error());
        }
        let rect = self.region_bounds(region)?;
        if rect.is_empty() {
            return Err(invalid("capture region is empty"));
        }
        let desktop = self.virtual_desktop()?;
        if !desktop.contains(rect) {
            return Err(invalid(format!(
                "capture region ({},{},{}x{}) is outside virtual desktop ({},{},{}x{})",
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                desktop.x,
                desktop.y,
                desktop.width,
                desktop.height
            )));
        }
        let image = self.capture_rect(rect, cancelled)?;
        if cancelled() {
            return Err(cancelled_error());
        }
        if image.dimensions() != (rect.width, rect.height) {
            return Err(ExecutionDiagnostic::new(
                DiagnosticKind::Backend,
                format!(
                    "capture returned {}x{} pixels for requested {}x{} region",
                    image.width(),
                    image.height(),
                    rect.width,
                    rect.height
                ),
            ));
        }
        Ok(CapturedRegion {
            image,
            origin: (rect.x, rect.y),
        })
    }
}
fn invalid(message: impl Into<String>) -> ExecutionDiagnostic {
    ExecutionDiagnostic::new(DiagnosticKind::InvalidTarget, message)
}
pub fn cancelled_error() -> ExecutionDiagnostic {
    ExecutionDiagnostic::new(DiagnosticKind::Cancelled, "visual search cancelled")
}

/// A monitor capture together with its position in the signed virtual-desktop
/// coordinate space. The capture itself always starts at monitor-local `(0, 0)`.
#[derive(Clone)]
struct CaptureMonitor {
    bounds: ScreenRect,
    source: Arc<dyn MonitorCapture>,
    stable_id: u64,
}

trait MonitorCapture: Send + Sync {
    fn capture(&self) -> ExecResult<RgbaImage>;
}

trait CapturePlatform: Send + Sync {
    fn virtual_desktop_metrics(&self) -> ExecResult<(i32, i32, i32, i32)>;
    fn monitors(&self) -> ExecResult<Vec<CaptureMonitor>>;
    fn window_rect(&self, matcher: &MkWindowMatcher, client: bool) -> ExecResult<ScreenRect>;
}

/// Production Windows screen capture. Coordinate resolution remains in
/// [`WindowsScreenBackend`]; this type owns only capture geometry and pixels.
pub struct WindowsScreenCaptureBackend {
    platform: Arc<dyn CapturePlatform>,
}

impl WindowsScreenCaptureBackend {
    #[cfg(windows)]
    pub fn system() -> Self {
        Self {
            platform: Arc::new(SystemCapturePlatform),
        }
    }

    #[cfg(test)]
    fn new(platform: Arc<dyn CapturePlatform>) -> Self {
        Self { platform }
    }

    fn monitors(&self) -> ExecResult<Vec<CaptureMonitor>> {
        let mut monitors = self.platform.monitors().map_err(|e| {
            e.context("backend", "WindowsScreenCaptureBackend")
                .context("operation", "enumerate monitors")
        })?;
        // Geometry first makes indices stable even if the OS enumeration order changes.
        monitors.sort_by_key(|m| {
            (
                m.bounds.x,
                m.bounds.y,
                m.bounds.width,
                m.bounds.height,
                m.stable_id,
            )
        });
        Ok(monitors)
    }

    /// Capture exactly one live desktop pixel at a signed desktop coordinate.
    pub fn read_pixel(&self, point: MkPoint) -> ExecResult<[u8; 4]> {
        let rect = ScreenRect::new(point.x, point.y, 1, 1);
        let image = self
            .capture(&SearchRegion::Rectangle { rect }, &|| false)
            .map_err(|e| e.context("coordinate", format!("{},{}", point.x, point.y)))?
            .image;
        Ok(image.get_pixel(0, 0).0)
    }
}

fn rect_from_signed_metrics(x: i32, y: i32, width: i32, height: i32) -> ExecResult<ScreenRect> {
    if width <= 0 || height <= 0 {
        return Err(invalid(format!(
            "invalid screen dimensions {width}x{height}"
        )));
    }
    let width = u32::try_from(width).map_err(|_| invalid("screen width conversion overflow"))?;
    let height = u32::try_from(height).map_err(|_| invalid("screen height conversion overflow"))?;
    let rect = ScreenRect::new(x, y, width, height);
    if rect.right() > i64::from(i32::MAX) + 1 || rect.bottom() > i64::from(i32::MAX) + 1 {
        return Err(invalid("screen rectangle endpoint overflow"));
    }
    Ok(rect)
}

fn intersection(a: ScreenRect, b: ScreenRect) -> Option<ScreenRect> {
    let left = i64::from(a.x).max(i64::from(b.x));
    let top = i64::from(a.y).max(i64::from(b.y));
    let right = a.right().min(b.right());
    let bottom = a.bottom().min(b.bottom());
    (left < right && top < bottom).then(|| {
        ScreenRect::new(
            left as i32,
            top as i32,
            (right - left) as u32,
            (bottom - top) as u32,
        )
    })
}

fn compose_monitors(
    target: ScreenRect,
    monitors: &[CaptureMonitor],
    cancelled: &dyn Fn() -> bool,
) -> ExecResult<RgbaImage> {
    if target.is_empty() {
        return Err(invalid("capture region is empty"));
    }
    let pixels = usize::try_from(u64::from(target.width) * u64::from(target.height))
        .map_err(|_| invalid("capture allocation size overflow"))?;
    let _rgba_bytes = pixels
        .checked_mul(4)
        .filter(|bytes| *bytes <= isize::MAX as usize)
        .ok_or_else(|| invalid("RGBA capture allocation size overflow"))?;
    let mut covered = vec![false; pixels];
    let mut destination = RgbaImage::new(target.width, target.height);
    for monitor in monitors {
        if cancelled() {
            return Err(cancelled_error());
        }
        let Some(overlap) = intersection(target, monitor.bounds) else {
            continue;
        };
        let source = monitor.source.capture().map_err(|e| {
            e.context("operation", "capture monitor").context(
                "monitor",
                format!(
                    "{},{},{}x{}",
                    monitor.bounds.x, monitor.bounds.y, monitor.bounds.width, monitor.bounds.height
                ),
            )
        })?;
        if source.dimensions() != (monitor.bounds.width, monitor.bounds.height) {
            return Err(ExecutionDiagnostic::new(
                DiagnosticKind::Backend,
                format!(
                    "monitor capture returned {}x{} for {}x{} monitor",
                    source.width(),
                    source.height(),
                    monitor.bounds.width,
                    monitor.bounds.height
                ),
            )
            .context("operation", "validate monitor capture"));
        }
        let sx = u32::try_from(i64::from(overlap.x) - i64::from(monitor.bounds.x))
            .map_err(|_| invalid("monitor-local X offset overflow"))?;
        let sy = u32::try_from(i64::from(overlap.y) - i64::from(monitor.bounds.y))
            .map_err(|_| invalid("monitor-local Y offset overflow"))?;
        let dx = u32::try_from(i64::from(overlap.x) - i64::from(target.x))
            .map_err(|_| invalid("destination X offset overflow"))?;
        let dy = u32::try_from(i64::from(overlap.y) - i64::from(target.y))
            .map_err(|_| invalid("destination Y offset overflow"))?;
        if sx
            .checked_add(overlap.width)
            .is_none_or(|v| v > source.width())
            || sy
                .checked_add(overlap.height)
                .is_none_or(|v| v > source.height())
            || dx
                .checked_add(overlap.width)
                .is_none_or(|v| v > destination.width())
            || dy
                .checked_add(overlap.height)
                .is_none_or(|v| v > destination.height())
        {
            return Err(ExecutionDiagnostic::new(
                DiagnosticKind::Backend,
                "compositor offsets exceed image bounds",
            )
            .context("operation", "copy monitor intersection"));
        }
        for y in 0..overlap.height {
            for x in 0..overlap.width {
                destination.put_pixel(dx + x, dy + y, *source.get_pixel(sx + x, sy + y));
                covered
                    [(u64::from(dy + y) * u64::from(target.width) + u64::from(dx + x)) as usize] =
                    true;
            }
        }
    }
    if cancelled() {
        return Err(cancelled_error());
    }
    if covered.iter().any(|covered| !covered) {
        return Err(ExecutionDiagnostic::new(
            DiagnosticKind::Backend,
            "monitors do not cover every requested pixel",
        )
        .context("operation", "compose monitor captures"));
    }
    Ok(destination)
}

impl ScreenCaptureBackend for WindowsScreenCaptureBackend {
    fn virtual_desktop(&self) -> ExecResult<ScreenRect> {
        let (x, y, width, height) = self.platform.virtual_desktop_metrics().map_err(|e| {
            e.context("backend", "WindowsScreenCaptureBackend")
                .context("operation", "read virtual-screen metrics")
        })?;
        rect_from_signed_metrics(x, y, width, height)
            .map_err(|e| e.context("backend", "WindowsScreenCaptureBackend"))
    }

    fn region_bounds(&self, region: &SearchRegion) -> ExecResult<ScreenRect> {
        let result = match region {
            SearchRegion::Desktop => self.virtual_desktop(),
            SearchRegion::Monitor { index } => self
                .monitors()?
                .get(*index)
                .map(|m| m.bounds)
                .ok_or_else(|| {
                    ExecutionDiagnostic::new(
                        DiagnosticKind::TargetNotFound,
                        format!("monitor index {index} does not exist"),
                    )
                    .context("monitor_index", index.to_string())
                }),
            SearchRegion::Rectangle { rect } => Ok(*rect),
            SearchRegion::Window { matcher } => self.platform.window_rect(matcher, false),
            SearchRegion::ClientArea { matcher } => self.platform.window_rect(matcher, true),
        };
        result.map_err(|e| {
            e.context("backend", "WindowsScreenCaptureBackend")
                .context("requested_region", format!("{region:?}"))
        })
    }

    fn capture_rect(
        &self,
        rect: ScreenRect,
        cancelled: &dyn Fn() -> bool,
    ) -> ExecResult<RgbaImage> {
        if cancelled() {
            return Err(cancelled_error());
        }
        compose_monitors(rect, &self.monitors()?, cancelled)
    }
}

#[cfg(windows)]
struct ScreenshotMonitor(screenshots::Screen);
#[cfg(windows)]
impl MonitorCapture for ScreenshotMonitor {
    fn capture(&self) -> ExecResult<RgbaImage> {
        // screenshots performs the native Windows BGRA -> RGBA conversion; its
        // public image is therefore safe to copy without another channel swap.
        self.0
            .capture_area_ignore_area_check(
                0,
                0,
                self.0.display_info.width,
                self.0.display_info.height,
            )
            .map_err(|e| {
                ExecutionDiagnostic::new(
                    DiagnosticKind::Backend,
                    format!("screenshots monitor capture failed: {e}"),
                )
            })
    }
}

#[cfg(windows)]
struct SystemCapturePlatform;
#[cfg(windows)]
impl CapturePlatform for SystemCapturePlatform {
    fn virtual_desktop_metrics(&self) -> ExecResult<(i32, i32, i32, i32)> {
        use windows::Win32::UI::WindowsAndMessaging::*;
        Ok(unsafe {
            (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
            )
        })
    }

    fn monitors(&self) -> ExecResult<Vec<CaptureMonitor>> {
        screenshots::Screen::all()
            .map_err(|e| {
                ExecutionDiagnostic::new(
                    DiagnosticKind::Backend,
                    format!("screenshots monitor enumeration failed: {e}"),
                )
            })?
            .into_iter()
            .map(|screen| {
                let d = screen.display_info;
                if d.width == 0
                    || d.height == 0
                    || i64::from(d.x) + i64::from(d.width) > i64::from(i32::MAX) + 1
                    || i64::from(d.y) + i64::from(d.height) > i64::from(i32::MAX) + 1
                {
                    return Err(invalid(format!("monitor {} has invalid bounds", d.id)));
                }
                Ok(CaptureMonitor {
                    bounds: ScreenRect::new(d.x, d.y, d.width, d.height),
                    stable_id: u64::from(d.id),
                    source: Arc::new(ScreenshotMonitor(screen)),
                })
            })
            .collect()
    }

    fn window_rect(&self, matcher: &MkWindowMatcher, client: bool) -> ExecResult<ScreenRect> {
        use windows::Win32::{
            Foundation::{HWND, POINT, RECT},
            Graphics::Gdi::ClientToScreen,
            UI::WindowsAndMessaging::{GetClientRect, GetWindowRect},
        };
        let candidates = crate::multi_manager::win::enumerate_top_level_windows()
            .map_err(|e| {
                ExecutionDiagnostic::new(
                    DiagnosticKind::Backend,
                    format!("window enumeration failed: {e}"),
                )
            })?
            .into_iter()
            .map(|w| crate::mkmacro::windows::WindowCandidate {
                handle: w.hwnd,
                title: w.title,
                executable: w.executable,
                process_path: w.process_path,
                class_name: w.class_name,
            })
            .collect::<Vec<_>>();
        let candidate = crate::mkmacro::windows::resolve_window(
            matcher,
            &candidates,
            crate::mkmacro::windows::AmbiguityPolicy::Error,
        )
        .map_err(|e| e.context("window_matcher", format!("{matcher:?}")))?;
        let hwnd = HWND(candidate.handle as *mut _);
        let mut rect = RECT::default();
        if client {
            unsafe { GetClientRect(hwnd, &mut rect) }.map_err(|e| {
                ExecutionDiagnostic::new(
                    DiagnosticKind::Backend,
                    format!("GetClientRect failed: {e}"),
                )
            })?;
            let mut origin = POINT {
                x: rect.left,
                y: rect.top,
            };
            if !unsafe { ClientToScreen(hwnd, &mut origin) }.as_bool() {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::Backend,
                    "ClientToScreen failed",
                ));
            }
            rect.right = origin
                .x
                .checked_add(rect.right - rect.left)
                .ok_or_else(|| invalid("client rectangle X overflow"))?;
            rect.bottom = origin
                .y
                .checked_add(rect.bottom - rect.top)
                .ok_or_else(|| invalid("client rectangle Y overflow"))?;
            rect.left = origin.x;
            rect.top = origin.y;
        } else {
            unsafe { GetWindowRect(hwnd, &mut rect) }.map_err(|e| {
                ExecutionDiagnostic::new(
                    DiagnosticKind::Backend,
                    format!("GetWindowRect failed: {e}"),
                )
            })?;
        }
        rect_from_signed_metrics(
            rect.left,
            rect.top,
            rect.right
                .checked_sub(rect.left)
                .ok_or_else(|| invalid("window width overflow"))?,
            rect.bottom
                .checked_sub(rect.top)
                .ok_or_else(|| invalid("window height overflow"))?,
        )
    }
}

#[cfg(test)]
mod windows_backend_tests {
    use super::*;
    use std::sync::Mutex;

    #[cfg(windows)]
    #[test]
    fn system_constructor_has_the_production_screen_backend_type() {
        let _: WindowsScreenBackend = WindowsScreenBackend::system();
        let _: WindowsScreenCaptureBackend = WindowsScreenCaptureBackend::system();
    }

    struct Geometry {
        desktop: (i32, i32, i32, i32),
        foreground: Option<isize>,
        origin: MkPoint,
    }
    impl WindowsGeometry for Geometry {
        fn virtual_desktop(&self) -> ExecResult<(i32, i32, i32, i32)> {
            Ok(self.desktop)
        }
        fn foreground_window(&self) -> ExecResult<Option<isize>> {
            Ok(self.foreground)
        }
        fn client_origin(&self, _: isize) -> ExecResult<MkPoint> {
            Ok(self.origin)
        }
    }
    #[derive(Default)]
    struct Visual {
        requested: Mutex<Vec<u64>>,
    }
    impl VisualSearch for Visual {
        fn find_image(&self, _: u64, payload: &MkImagePayload) -> ExecResult<Option<MkPoint>> {
            let id = payload.asset_id;
            self.requested.lock().unwrap().push(id);
            Ok(None)
        }
        fn read_pixel(&self, _: MkPoint) -> ExecResult<[u8; 4]> {
            Ok([0, 0, 0, 255])
        }
    }
    fn backend(
        desktop: (i32, i32, i32, i32),
        foreground: Option<isize>,
        origin: MkPoint,
    ) -> WindowsScreenBackend {
        WindowsScreenBackend::new(
            Arc::new(Geometry {
                desktop,
                foreground,
                origin,
            }),
            Arc::new(Visual::default()),
        )
    }

    #[test]
    fn strict_rgb_parser_accepts_case_and_formats_canonically() {
        assert_eq!(parse_rgb("#00A1FF").unwrap(), [0, 161, 255]);
        assert_eq!(parse_rgb("#00a1fF").unwrap(), [0, 161, 255]);
        assert_eq!(format_rgb([0, 161, 255]), "#00A1FF");
        for malformed in [
            "000000", "#FFF", "#0000000", "#00GG00", "", "# 0000", "#aébcd",
        ] {
            let error = parse_rgb(malformed).unwrap_err();
            assert_eq!(error.kind, DiagnosticKind::InvalidTarget);
            assert!(error.to_string().contains("#RRGGBB"));
        }
    }

    struct PixelVisual([u8; 4]);
    impl VisualSearch for PixelVisual {
        fn find_image(&self, _: u64, _: &MkImagePayload) -> ExecResult<Option<MkPoint>> {
            Ok(None)
        }
        fn read_pixel(&self, _: MkPoint) -> ExecResult<[u8; 4]> {
            Ok(self.0)
        }
    }
    fn pixel_backend(pixel: [u8; 4]) -> WindowsScreenBackend {
        WindowsScreenBackend::new(
            Arc::new(Geometry {
                desktop: (-100, -100, 200, 200),
                foreground: Some(1),
                origin: MkPoint { x: -20, y: -30 },
            }),
            Arc::new(PixelVisual(pixel)),
        )
    }

    #[test]
    fn pixel_comparison_is_per_channel_overflow_safe_and_ignores_alpha() {
        let vars = RuntimeVariables::new();
        let target = screen(MkPoint { x: -50, y: -40 });
        assert!(
            pixel_backend([0, 255, 10, 0])
                .pixel_matches(&target, "#00FF0A", 0, &vars)
                .unwrap()
        );
        assert!(
            pixel_backend([0, 255, 10, 255])
                .pixel_matches(&target, "#05FA0F", 5, &vars)
                .unwrap()
        );
        assert!(
            !pixel_backend([0, 255, 10, 255])
                .pixel_matches(&target, "#06FA0F", 5, &vars)
                .unwrap()
        );
        assert!(
            pixel_backend([0, 255, 0, 1])
                .pixel_matches(&target, "#FF00FF", 255, &vars)
                .unwrap()
        );
    }

    #[test]
    fn pixel_target_is_resolved_before_reading() {
        let backend = pixel_backend([1, 2, 3, 4]);
        assert!(
            backend
                .pixel_matches(
                    &MkCoordinateTarget::ActiveWindow {
                        point: MkPoint { x: 2, y: 3 }
                    },
                    "#010203",
                    0,
                    &RuntimeVariables::new()
                )
                .unwrap()
        );
    }
    fn screen(point: MkPoint) -> MkCoordinateTarget {
        MkCoordinateTarget::Screen { point }
    }

    #[test]
    fn configured_screen_coordinates_must_be_on_desktop_and_randomized_points_clamp() {
        let b = backend((-100, -50, 300, 100), None, MkPoint { x: 0, y: 0 });
        assert_eq!(
            b.resolve(&screen(MkPoint { x: 0, y: 0 }), &RuntimeVariables::new())
                .unwrap(),
            MkPoint { x: 0, y: 0 }
        );
        for point in [
            MkPoint { x: -101, y: -51 },
            MkPoint { x: 200, y: 50 },
            MkPoint { x: -200, y: 20 },
            MkPoint { x: 20, y: 100 },
        ] {
            let error = b
                .resolve(&screen(point), &RuntimeVariables::new())
                .unwrap_err();
            assert_eq!(error.kind, DiagnosticKind::InvalidTarget);
            assert_eq!(
                error.message,
                "Requested coordinate lies outside the virtual desktop"
            );
        }
        // `resolve` validates configured input, while `finalize_point` deliberately
        // clamps a valid point after playback randomization shifts it over an edge.
        assert_eq!(
            b.finalize_point(MkPoint { x: -101, y: 50 }).unwrap(),
            MkPoint { x: -100, y: 49 }
        );
    }
    #[test]
    fn one_pixel_and_invalid_or_overflowing_desktops() {
        let one = backend((-7, 9, 1, 1), None, MkPoint { x: 0, y: 0 });
        assert_eq!(
            one.resolve(&screen(MkPoint { x: -7, y: 9 }), &RuntimeVariables::new())
                .unwrap(),
            MkPoint { x: -7, y: 9 }
        );
        let outside = one
            .resolve(
                &screen(MkPoint {
                    x: i32::MAX,
                    y: i32::MIN,
                }),
                &RuntimeVariables::new(),
            )
            .unwrap_err();
        assert_eq!(outside.kind, DiagnosticKind::InvalidTarget);
        assert_eq!(
            outside.message,
            "Requested coordinate lies outside the virtual desktop"
        );
        for desktop in [
            (0, 0, 0, 1),
            (0, 0, 1, -1),
            (i32::MAX, 0, 2, 1),
            (0, i32::MAX, 1, 2),
        ] {
            assert_eq!(
                backend(desktop, None, MkPoint { x: 0, y: 0 })
                    .resolve(&screen(MkPoint { x: 0, y: 0 }), &RuntimeVariables::new())
                    .unwrap_err()
                    .kind,
                DiagnosticKind::InvalidTarget
            );
        }
    }
    #[test]
    fn active_window_uses_client_origin_and_checked_add() {
        let b = backend(
            (-500, -500, 1000, 1000),
            Some(42),
            MkPoint { x: -100, y: -50 },
        );
        assert_eq!(
            b.resolve(
                &MkCoordinateTarget::ActiveWindow {
                    point: MkPoint { x: 10, y: 20 }
                },
                &RuntimeVariables::new()
            )
            .unwrap(),
            MkPoint { x: -90, y: -30 }
        );
        assert_eq!(
            backend((0, 0, 10, 10), None, MkPoint { x: 0, y: 0 })
                .resolve(
                    &MkCoordinateTarget::ActiveWindow {
                        point: MkPoint { x: 0, y: 0 }
                    },
                    &RuntimeVariables::new()
                )
                .unwrap_err()
                .kind,
            DiagnosticKind::TargetNotFound
        );
        let err = backend((0, 0, 10, 10), Some(1), MkPoint { x: i32::MAX, y: 0 })
            .resolve(
                &MkCoordinateTarget::ActiveWindow {
                    point: MkPoint { x: 1, y: 0 },
                },
                &RuntimeVariables::new(),
            )
            .unwrap_err();
        assert_eq!(err.kind, DiagnosticKind::InvalidTarget);
        assert!(err.message.contains("overflow"));
    }
    #[test]
    fn variables_are_strictly_typed() {
        let b = backend((0, 0, 10, 10), None, MkPoint { x: 0, y: 0 });
        let target = MkCoordinateTarget::Variable { name: "v".into() };
        let mut vars = RuntimeVariables::new();
        vars.insert("v".into(), MkValue::Point(MkPoint { x: 12, y: -3 }));
        assert_eq!(b.resolve(&target, &vars).unwrap(), MkPoint { x: 12, y: -3 });
        vars.remove("v");
        assert_eq!(
            b.resolve(&target, &vars).unwrap_err().kind,
            DiagnosticKind::TargetNotFound
        );
        for value in [
            MkValue::String("1,2".into()),
            MkValue::Number(1.0),
            MkValue::Boolean(true),
            MkValue::Null,
        ] {
            vars.insert("v".into(), value);
            let e = b.resolve(&target, &vars).unwrap_err();
            assert_eq!(e.kind, DiagnosticKind::TypeMismatch);
            assert!(e.message.contains("Point"));
            assert!(e.context.contains_key("actual"));
        }
    }
    #[test]
    fn images_are_asset_specific_typed_and_offset_checked() {
        let b = backend((0, 0, 10, 10), None, MkPoint { x: 0, y: 0 });
        let target = |id, x, y| MkCoordinateTarget::Image {
            asset_id: id,
            offset: MkPoint { x, y },
        };
        let mut vars = RuntimeVariables::new();
        vars.insert(
            "last_image".into(),
            MkValue::Point(MkPoint { x: 99, y: 99 }),
        );
        vars.insert(
            image_result_variable(2),
            MkValue::Point(MkPoint { x: 10, y: 20 }),
        );
        assert_eq!(
            b.resolve(&target(2, 3, -4), &vars).unwrap(),
            MkPoint { x: 13, y: 16 }
        );
        assert_eq!(
            b.resolve(&target(1, 0, 0), &vars).unwrap_err().kind,
            DiagnosticKind::TargetNotFound
        );
        vars.insert(image_result_variable(1), MkValue::String("bad".into()));
        assert_eq!(
            b.resolve(&target(1, 0, 0), &vars).unwrap_err().kind,
            DiagnosticKind::TypeMismatch
        );
        vars.insert(
            image_result_variable(1),
            MkValue::Point(MkPoint { x: i32::MAX, y: 0 }),
        );
        assert!(
            b.resolve(&target(1, 1, 0), &vars)
                .unwrap_err()
                .message
                .contains("overflow")
        );
    }

    struct ImageSource(RgbaImage);
    impl MonitorCapture for ImageSource {
        fn capture(&self) -> ExecResult<RgbaImage> {
            Ok(self.0.clone())
        }
    }
    struct CaptureFixture {
        desktop: (i32, i32, i32, i32),
        monitors: Vec<CaptureMonitor>,
    }
    impl CapturePlatform for CaptureFixture {
        fn virtual_desktop_metrics(&self) -> ExecResult<(i32, i32, i32, i32)> {
            Ok(self.desktop)
        }
        fn monitors(&self) -> ExecResult<Vec<CaptureMonitor>> {
            Ok(self.monitors.clone())
        }
        fn window_rect(&self, _: &MkWindowMatcher, _: bool) -> ExecResult<ScreenRect> {
            Err(ExecutionDiagnostic::new(
                DiagnosticKind::TargetNotFound,
                "fixture window missing",
            ))
        }
    }
    fn monitor(id: u64, rect: ScreenRect, color: [u8; 4]) -> CaptureMonitor {
        CaptureMonitor {
            bounds: rect,
            stable_id: id,
            source: Arc::new(ImageSource(RgbaImage::from_pixel(
                rect.width,
                rect.height,
                image::Rgba(color),
            ))),
        }
    }

    #[test]
    fn half_open_intersections_handle_primary_left_above_and_touching_edges() {
        let primary = ScreenRect::new(0, 0, 4, 3);
        assert_eq!(intersection(primary, ScreenRect::new(-3, 0, 3, 3)), None);
        assert_eq!(intersection(primary, ScreenRect::new(0, -2, 4, 2)), None);
        assert_eq!(
            intersection(primary, ScreenRect::new(-2, 1, 4, 1)),
            Some(ScreenRect::new(0, 1, 2, 1))
        );
    }

    #[test]
    fn compositor_translates_negative_origins_and_exact_rgba_offsets() {
        let monitors = vec![
            monitor(1, ScreenRect::new(-2, 0, 2, 2), [1, 2, 3, 4]),
            monitor(2, ScreenRect::new(0, 0, 3, 2), [10, 20, 30, 40]),
        ];
        let image = compose_monitors(ScreenRect::new(-1, 0, 3, 2), &monitors, &|| false).unwrap();
        assert_eq!(image.dimensions(), (3, 2));
        assert_eq!(image.get_pixel(0, 1).0, [1, 2, 3, 4]);
        assert_eq!(image.get_pixel(1, 1).0, [10, 20, 30, 40]);
        assert_eq!(image.get_pixel(2, 0).0, [10, 20, 30, 40]);
    }

    #[test]
    fn compositor_supports_above_ultrawide_and_single_monitor_targets() {
        let monitors = vec![
            monitor(1, ScreenRect::new(0, -2, 2, 2), [1, 0, 0, 255]),
            monitor(2, ScreenRect::new(0, 0, 2, 2), [2, 0, 0, 255]),
            monitor(3, ScreenRect::new(2, 0, 5, 2), [3, 0, 0, 255]),
        ];
        assert_eq!(
            compose_monitors(ScreenRect::new(3, 0, 3, 2), &monitors, &|| false)
                .unwrap()
                .get_pixel(2, 1)
                .0,
            [3, 0, 0, 255]
        );
        let span = compose_monitors(ScreenRect::new(0, -1, 2, 3), &monitors, &|| false).unwrap();
        assert_eq!(span.get_pixel(0, 0).0[0], 1);
        assert_eq!(span.get_pixel(0, 1).0[0], 2);
    }

    #[test]
    fn validation_rejects_empty_overflow_outside_and_uncovered_rectangles() {
        for metrics in [(0, 0, 0, 1), (0, 0, -1, 1), (i32::MAX, 0, 2, 1)] {
            assert_eq!(
                rect_from_signed_metrics(metrics.0, metrics.1, metrics.2, metrics.3)
                    .unwrap_err()
                    .kind,
                DiagnosticKind::InvalidTarget
            );
        }
        let backend = WindowsScreenCaptureBackend::new(Arc::new(CaptureFixture {
            desktop: (0, 0, 2, 2),
            monitors: vec![monitor(1, ScreenRect::new(0, 0, 1, 2), [0; 4])],
        }));
        assert_eq!(
            backend
                .capture(
                    &SearchRegion::Rectangle {
                        rect: ScreenRect::new(0, 0, 0, 1)
                    },
                    &|| false
                )
                .unwrap_err()
                .kind,
            DiagnosticKind::InvalidTarget
        );
        assert_eq!(
            backend
                .capture(
                    &SearchRegion::Rectangle {
                        rect: ScreenRect::new(-1, 0, 1, 1)
                    },
                    &|| false
                )
                .unwrap_err()
                .kind,
            DiagnosticKind::InvalidTarget
        );
        assert_eq!(
            backend
                .capture_rect(ScreenRect::new(0, 0, 2, 2), &|| false)
                .unwrap_err()
                .kind,
            DiagnosticKind::Backend
        );
    }

    #[test]
    fn monitor_indices_are_deterministic_and_invalid_indices_are_typed() {
        let backend = WindowsScreenCaptureBackend::new(Arc::new(CaptureFixture {
            desktop: (-2, 0, 4, 1),
            monitors: vec![
                monitor(8, ScreenRect::new(0, 0, 2, 1), [0; 4]),
                monitor(9, ScreenRect::new(-2, 0, 2, 1), [0; 4]),
            ],
        }));
        assert_eq!(
            backend
                .region_bounds(&SearchRegion::Monitor { index: 0 })
                .unwrap()
                .x,
            -2
        );
        assert_eq!(
            backend
                .region_bounds(&SearchRegion::Monitor { index: 2 })
                .unwrap_err()
                .kind,
            DiagnosticKind::TargetNotFound
        );
    }

    #[test]
    fn cancellation_is_observed_before_and_between_monitor_captures() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let monitors = vec![
            monitor(1, ScreenRect::new(0, 0, 1, 1), [0; 4]),
            monitor(2, ScreenRect::new(1, 0, 1, 1), [0; 4]),
        ];
        assert_eq!(
            compose_monitors(ScreenRect::new(0, 0, 2, 1), &monitors, &|| true)
                .unwrap_err()
                .kind,
            DiagnosticKind::Cancelled
        );
        let checks = AtomicUsize::new(0);
        let cancelled = || checks.fetch_add(1, Ordering::SeqCst) >= 1;
        assert_eq!(
            compose_monitors(ScreenRect::new(0, 0, 2, 1), &monitors, &cancelled)
                .unwrap_err()
                .kind,
            DiagnosticKind::Cancelled
        );
    }
}
