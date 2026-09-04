//! Platform-independent screen capture geometry used by image and pixel search.
//! Coordinates in a [`CapturedRegion`] are local; `origin` converts them back to
//! virtual-desktop coordinates (and may therefore be negative).
use crate::mkmacro::{
    DiagnosticKind, ExecResult, ExecutionDiagnostic, ImageSearchMatch, MkCoordinateTarget,
    MkImagePayload, MkImageRef, MkPixelSearchPayload, MkPoint, MkValue, MkWindowMatcher,
    RuntimeVariables, ScreenBackend,
};
use image::RgbaImage;
use std::sync::Arc;

/// Runtime-only variable holding the result of the most recent search for one
/// exact shared image reference. Length-prefixing plus byte escaping keeps the
/// internal key unambiguous even if a future filename policy permits separators.
pub(crate) fn image_result_variable(image: &MkImageRef) -> String {
    format!(
        "__image.{}:{}",
        image.filename().len(),
        encode_key(image.filename())
    )
}

/// Runtime-only variable recording whether the latest search for a reference found it.
pub(crate) fn image_found_variable(image: &MkImageRef) -> String {
    format!(
        "__image_found.{}:{}",
        image.filename().len(),
        encode_key(image.filename())
    )
}

fn encode_key(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub(crate) fn pixel_result_variable(search_id: u64) -> String {
    format!("__pixel.{search_id}")
}
pub(crate) fn pixel_found_variable(search_id: u64) -> String {
    format!("__pixel_found.{search_id}")
}

pub(crate) trait WindowsGeometry: Send + Sync {
    /// `(x, y, width, height)`, in desktop pixels. Width/height remain signed so
    /// malformed platform data can be diagnosed rather than silently cast.
    fn virtual_desktop(&self) -> ExecResult<(i32, i32, i32, i32)>;
    fn foreground_window(&self) -> ExecResult<Option<isize>>;
    fn client_origin(&self, hwnd: isize) -> ExecResult<MkPoint>;
    fn top_level_windows(&self) -> ExecResult<Vec<crate::mkmacro::windows::WindowCandidate>>;
}

pub(crate) trait VisualSearch: Send + Sync {
    fn find_image_match(
        &self,
        macro_id: u64,
        payload: &MkImagePayload,
    ) -> ExecResult<Option<ImageSearchMatch>>;
    fn find_image(&self, macro_id: u64, payload: &MkImagePayload) -> ExecResult<Option<MkPoint>> {
        self.find_image_match(macro_id, payload)
            .map(|matched| matched.map(|matched| matched.point))
    }
    fn read_pixel(&self, point: MkPoint) -> ExecResult<[u8; 4]>;
    fn find_pixel(&self, _: &MkPixelSearchPayload) -> ExecResult<Option<MkPoint>> {
        Err(ExecutionDiagnostic::new(
            DiagnosticKind::UnsupportedOperation,
            "pixel search is unavailable",
        ))
    }
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

    /// Constructs the live geometry/capture adapter while keeping asset access
    /// explicit.  This is the constructor used by the runtime; tests use the
    /// same adapter shape with fake capture and geometry boundaries.
    #[cfg(windows)]
    pub fn production(store: Arc<crate::mkmacro::MkMacroStore>) -> Self {
        let capture: Arc<dyn ScreenCaptureBackend> =
            Arc::new(WindowsScreenCaptureBackend::system());
        Self {
            geometry: Arc::new(SystemWindowsGeometry),
            visual: Arc::new(crate::mkmacro::image_search::ProductionVisualSearch::new(
                store, capture,
            )),
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
            MkCoordinateTarget::CurrentPosition => Err(ExecutionDiagnostic::new(
                DiagnosticKind::UnsupportedOperation,
                "Current Position is resolved by Mouse Click from the input backend",
            )),
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
            MkCoordinateTarget::WindowClient { matcher, point } => {
                let candidates = self.geometry.top_level_windows().map_err(|e| {
                    e.context("backend", "WindowsScreenBackend")
                        .context("action", "enumerate matched-window coordinate candidates")
                })?;
                let candidate = crate::mkmacro::windows::resolve_window(
                    matcher,
                    &candidates,
                    crate::mkmacro::windows::AmbiguityPolicy::Error,
                )
                .map_err(|e| e.context("window_matcher", format!("{matcher:?}")))?;
                let origin = self
                    .geometry
                    .client_origin(candidate.handle as isize)
                    .map_err(|e| {
                        e.context(
                            "window",
                            format!("{} ({})", candidate.title, candidate.executable),
                        )
                        .context("action", "read matched-window client origin")
                    })?;
                let desktop =
                    MkPoint {
                        x: origin.x.checked_add(point.x).ok_or_else(|| {
                            invalid("matched-window client X coordinate overflow")
                        })?,
                        y: origin.y.checked_add(point.y).ok_or_else(|| {
                            invalid("matched-window client Y coordinate overflow")
                        })?,
                    };
                self.require_on_desktop(desktop)
                    .map_err(|e| e.context("action", "resolve matched-window client point"))
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
            MkCoordinateTarget::Image { image, offset } => {
                let key = image_result_variable(image);
                match variables.get(&key) {
                    Some(MkValue::Point(point)) => Ok(MkPoint {
                        x: point.x.checked_add(offset.x).ok_or_else(|| {
                            invalid(format!("image '{}' X offset overflow", image.filename()))
                        })?,
                        y: point.y.checked_add(offset.y).ok_or_else(|| {
                            invalid(format!("image '{}' Y offset overflow", image.filename()))
                        })?,
                    }),
                    Some(value) => Err(type_mismatch(&key, value)),
                    None => Err(ExecutionDiagnostic::new(
                        DiagnosticKind::TargetNotFound,
                        format!(
                            "image '{}' has no result in the current run",
                            image.filename()
                        ),
                    )
                    .context("image", image.filename())
                    .context("variable", key)),
                }
            }
            MkCoordinateTarget::Pixel { search_id, offset } => {
                let key = pixel_result_variable(*search_id);
                match variables.get(&key) {
                    Some(MkValue::Point(point)) => Ok(MkPoint {
                        x: point
                            .x
                            .checked_add(offset.x)
                            .ok_or_else(|| invalid("pixel result X offset overflow"))?,
                        y: point
                            .y
                            .checked_add(offset.y)
                            .ok_or_else(|| invalid("pixel result Y offset overflow"))?,
                    }),
                    Some(value) => Err(type_mismatch(&key, value)),
                    None => Err(ExecutionDiagnostic::new(
                        DiagnosticKind::TargetNotFound,
                        format!("pixel search {search_id} has no result in the current run"),
                    )),
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
    fn find_pixel(&self, payload: &MkPixelSearchPayload) -> ExecResult<Option<MkPoint>> {
        self.visual.find_pixel(payload)
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
    fn top_level_windows(&self) -> ExecResult<Vec<crate::mkmacro::windows::WindowCandidate>> {
        crate::multi_manager::win::enumerate_top_level_windows()
            .map(|items| items.into_iter().map(Into::into).collect())
            .map_err(|e| {
                ExecutionDiagnostic::new(
                    DiagnosticKind::Backend,
                    format!("window enumeration failed: {e}"),
                )
            })
    }
}

#[cfg(windows)]
struct SystemVisualSearch;
#[cfg(windows)]
impl VisualSearch for SystemVisualSearch {
    fn find_image_match(&self, _: u64, _: &MkImagePayload) -> ExecResult<Option<ImageSearchMatch>> {
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

/// Fallback used by non-Windows hosts and tests that have not installed a
/// capture/search fixture. The dialog still exercises its asynchronous state
/// machine and presents this diagnostic without touching runtime state.
pub(crate) struct UnsupportedVisualSearch;

impl VisualSearch for UnsupportedVisualSearch {
    fn find_image_match(&self, _: u64, _: &MkImagePayload) -> ExecResult<Option<ImageSearchMatch>> {
        Err(ExecutionDiagnostic::new(
            DiagnosticKind::UnsupportedOperation,
            "production image lookup is not configured",
        )
        .context("backend", "VisualSearch"))
    }

    fn read_pixel(&self, _: MkPoint) -> ExecResult<[u8; 4]> {
        Err(ExecutionDiagnostic::new(
            DiagnosticKind::UnsupportedOperation,
            "pixel lookup is not configured",
        ))
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

    /// Checks every invariant required before allocating an RGBA capture.
    pub fn validate_capture(self) -> Result<(), CaptureGeometryError> {
        if self.width == 0 {
            return Err(CaptureGeometryError::ZeroWidth);
        }
        if self.height == 0 {
            return Err(CaptureGeometryError::ZeroHeight);
        }
        if self.right() > i64::from(i32::MAX) + 1 {
            return Err(CaptureGeometryError::RightOverflow);
        }
        if self.bottom() > i64::from(i32::MAX) + 1 {
            return Err(CaptureGeometryError::BottomOverflow);
        }
        let pixels = usize::try_from(u64::from(self.width) * u64::from(self.height))
            .map_err(|_| CaptureGeometryError::AllocationOverflow)?;
        pixels
            .checked_mul(4)
            .filter(|n| *n <= isize::MAX as usize)
            .ok_or(CaptureGeometryError::AllocationOverflow)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureGeometryError {
    ZeroWidth,
    ZeroHeight,
    RightOverflow,
    BottomOverflow,
    AllocationOverflow,
}

impl std::fmt::Display for CaptureGeometryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::ZeroWidth => "capture region width is zero",
            Self::ZeroHeight => "capture region height is zero",
            Self::RightOverflow => "capture region right endpoint overflow",
            Self::BottomOverflow => "capture region bottom endpoint overflow",
            Self::AllocationOverflow => "RGBA capture allocation size overflow",
        })
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

/// Live monitor metadata safe to expose to authoring UI. `index` is the exact
/// persisted index consumed by [`ScreenCaptureBackend::region_bounds`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorDescriptor {
    pub index: usize,
    pub bounds: ScreenRect,
    pub primary: bool,
}
impl MonitorDescriptor {
    pub fn label(&self) -> String {
        format!(
            "Monitor {} — {}×{} @ ({}, {}){}",
            self.index,
            self.bounds.width,
            self.bounds.height,
            self.bounds.x,
            self.bounds.y,
            if self.primary { " — Primary" } else { "" }
        )
    }
}
impl Default for SearchRegion {
    fn default() -> Self {
        Self::Desktop
    }
}

#[cfg(test)]
mod capture_geometry_tests {
    use super::*;
    #[test]
    fn checked_capture_geometry_covers_dimensions_endpoints_and_allocations() {
        assert_eq!(
            ScreenRect::new(0, 0, 0, 1).validate_capture(),
            Err(CaptureGeometryError::ZeroWidth)
        );
        assert_eq!(
            ScreenRect::new(0, 0, 1, 0).validate_capture(),
            Err(CaptureGeometryError::ZeroHeight)
        );
        assert_eq!(
            ScreenRect::new(i32::MAX, 0, 2, 1).validate_capture(),
            Err(CaptureGeometryError::RightOverflow)
        );
        assert_eq!(
            ScreenRect::new(0, i32::MAX, 1, 2).validate_capture(),
            Err(CaptureGeometryError::BottomOverflow)
        );
        assert_eq!(
            ScreenRect::new(i32::MIN, i32::MIN, u32::MAX, u32::MAX).validate_capture(),
            Err(CaptureGeometryError::AllocationOverflow)
        );
        assert!(
            ScreenRect::new(-850, 100, 600, 400)
                .validate_capture()
                .is_ok()
        );
        assert!(
            CaptureGeometryError::RightOverflow
                .to_string()
                .contains("overflow")
        );
        assert!(
            CaptureGeometryError::AllocationOverflow
                .to_string()
                .contains("allocation size overflow")
        );
    }

    fn frame(width: u32, height: u32, pixels: &[[u8; 4]]) -> CapturedRegion {
        CapturedRegion {
            image: RgbaImage::from_raw(width, height, pixels.iter().flatten().copied().collect())
                .unwrap(),
            origin: (0, 0),
        }
    }

    #[test]
    fn visual_difference_is_deterministic_at_boundary_and_ignores_noise() {
        let baseline = frame(2, 2, &[[0, 0, 0, 255]; 4]);
        let one_changed = frame(
            2,
            2,
            &[
                [9, 0, 0, 255],
                [0, 0, 0, 255],
                [0, 0, 0, 255],
                [0, 0, 0, 255],
            ],
        );
        assert_eq!(
            visual_frame_difference(&baseline, &one_changed, 8).unwrap(),
            VisualFrameDifference::ChangedPixelPercent(25.0)
        );
        assert_eq!(
            visual_frame_difference(&baseline, &one_changed, 9).unwrap(),
            VisualFrameDifference::ChangedPixelPercent(0.0)
        );
        assert_eq!(
            visual_frame_difference(&baseline, &frame(1, 1, &[[0, 0, 0, 255]]), 0).unwrap(),
            VisualFrameDifference::RegionSizeChanged
        );
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

/// Result of comparing a fresh capture with the immutable initial capture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VisualFrameDifference {
    /// A changed capture geometry is itself a meaningful visual change.
    RegionSizeChanged,
    /// Percentage of pixels for which at least one channel exceeds tolerance.
    ChangedPixelPercent(f64),
}

/// Compares logical pixels row-by-row. Iterating image rows deliberately avoids
/// treating storage outside a row's pixel width (backend row padding/stride) as
/// image content. Integer counts use `u64`; the dimension product is checked.
pub fn visual_frame_difference(
    baseline: &CapturedRegion,
    fresh: &CapturedRegion,
    tolerance: u8,
) -> ExecResult<VisualFrameDifference> {
    if baseline.image.dimensions() != fresh.image.dimensions() {
        return Ok(VisualFrameDifference::RegionSizeChanged);
    }
    let total = u64::from(baseline.image.width())
        .checked_mul(u64::from(baseline.image.height()))
        .ok_or_else(|| invalid("visual comparison pixel count overflow"))?;
    if total == 0 {
        return Err(invalid("visual comparison region is empty"));
    }
    let mut changed = 0u64;
    for (base_row, fresh_row) in baseline.image.rows().zip(fresh.image.rows()) {
        for (base, current) in base_row.zip(fresh_row) {
            if base
                .0
                .iter()
                .zip(current.0.iter())
                .any(|(a, b)| a.abs_diff(*b) > tolerance)
            {
                changed = changed
                    .checked_add(1)
                    .ok_or_else(|| invalid("visual comparison changed-pixel count overflow"))?;
            }
        }
    }
    Ok(VisualFrameDifference::ChangedPixelPercent(
        (changed as f64) * 100.0 / (total as f64),
    ))
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

    fn descriptors(&self) -> ExecResult<Vec<MonitorDescriptor>> {
        Ok(self
            .monitors()?
            .iter()
            .enumerate()
            .map(|(index, monitor)| MonitorDescriptor {
                index,
                bounds: monitor.bounds,
                // Windows defines the primary display origin as (0, 0).
                primary: monitor.bounds.x == 0 && monitor.bounds.y == 0,
            })
            .collect())
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

/// Enumerates monitors through the very same backend and sorting routine used
/// during playback, avoiding UI/runtime index drift.
pub fn monitor_descriptors() -> ExecResult<Vec<MonitorDescriptor>> {
    #[cfg(windows)]
    {
        WindowsScreenCaptureBackend::system().descriptors()
    }
    #[cfg(not(windows))]
    {
        Err(ExecutionDiagnostic::new(
            DiagnosticKind::UnsupportedOperation,
            "Monitor enumeration is available only on Windows",
        ))
    }
}

/// Resolves a matcher against a fresh top-level-window enumeration and returns
/// the same geometry used by image-search playback.  No native handle escapes
/// this call, so authoring previews cannot retain a stale HWND.
pub fn resolve_window_screen_rect(
    matcher: &MkWindowMatcher,
    client_area: bool,
) -> ExecResult<ScreenRect> {
    #[cfg(windows)]
    {
        SystemCapturePlatform.window_rect(matcher, client_area)
    }
    #[cfg(not(windows))]
    {
        let _ = (matcher, client_area);
        Err(ExecutionDiagnostic::new(
            DiagnosticKind::UnsupportedOperation,
            "Window target resolution is available only on Windows",
        ))
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
    target
        .validate_capture()
        .map_err(|error| invalid(format!("invalid capture rectangle: {error}")))?;
    // Pixels in gaps between physical monitors have a deterministic, opaque
    // background. Do not use `RgbaImage::new`: its transparent default would
    // make those pixels unsuitable for normal screen-color comparisons.
    let mut destination =
        RgbaImage::from_pixel(target.width, target.height, image::Rgba([0, 0, 0, 255]));
    for monitor in monitors {
        let Some(overlap) = intersection(target, monitor.bounds) else {
            continue;
        };
        if cancelled() {
            return Err(cancelled_error());
        }
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
            }
        }
    }
    if cancelled() {
        return Err(cancelled_error());
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
                        format!("Monitor {index} is not currently available"),
                    )
                    .context("monitor_index", index.to_string())
                }),
            SearchRegion::Rectangle { rect } => {
                rect.validate_capture().map(|()| *rect).map_err(|error| {
                    invalid(format!("Invalid capture rectangle: {error}"))
                        .context("rectangle", format!("{rect:?}"))
                })
            }
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
        candidates: Vec<crate::mkmacro::windows::WindowCandidate>,
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
        fn top_level_windows(&self) -> ExecResult<Vec<crate::mkmacro::windows::WindowCandidate>> {
            Ok(self.candidates.clone())
        }
    }
    #[derive(Default)]
    struct Visual {
        requested: Mutex<Vec<MkImageRef>>,
    }
    impl VisualSearch for Visual {
        fn find_image_match(
            &self,
            _: u64,
            payload: &MkImagePayload,
        ) -> ExecResult<Option<ImageSearchMatch>> {
            let image = payload.image.clone();
            self.requested.lock().unwrap().push(image);
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
                candidates: vec![],
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
        fn find_image_match(
            &self,
            _: u64,
            _: &MkImagePayload,
        ) -> ExecResult<Option<ImageSearchMatch>> {
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
                candidates: vec![],
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
    fn candidate(handle: usize, title: &str) -> crate::mkmacro::windows::WindowCandidate {
        crate::mkmacro::windows::WindowCandidate {
            handle,
            title: title.into(),
            executable: "app.exe".into(),
            process_path: "C:\\app.exe".into(),
            class_name: "App".into(),
        }
    }
    fn matched_backend(
        candidates: Vec<crate::mkmacro::windows::WindowCandidate>,
        origin: MkPoint,
    ) -> WindowsScreenBackend {
        WindowsScreenBackend::new(
            Arc::new(Geometry {
                desktop: (-1000, -1000, 2000, 2000),
                foreground: Some(999),
                origin,
                candidates,
            }),
            Arc::new(Visual::default()),
        )
    }
    #[test]
    fn matched_window_resolves_without_foreground_and_translates_negative_origin() {
        let backend = matched_backend(vec![candidate(7, "Editor")], MkPoint { x: -200, y: -100 });
        let target = MkCoordinateTarget::WindowClient {
            matcher: MkWindowMatcher {
                process: Some("app.exe".into()),
                title: Some("Edit".into()),
                ..Default::default()
            },
            point: MkPoint { x: 25, y: 30 },
        };
        assert_eq!(
            backend.resolve(&target, &RuntimeVariables::new()).unwrap(),
            MkPoint { x: -175, y: -70 }
        );
    }
    #[test]
    fn matched_window_reports_missing_ambiguity_and_overflow() {
        let target = |point| MkCoordinateTarget::WindowClient {
            matcher: MkWindowMatcher {
                process: Some("app.exe".into()),
                ..Default::default()
            },
            point,
        };
        assert_eq!(
            matched_backend(vec![], MkPoint { x: 0, y: 0 })
                .resolve(&target(MkPoint { x: 0, y: 0 }), &RuntimeVariables::new())
                .unwrap_err()
                .kind,
            DiagnosticKind::TargetNotFound
        );
        assert_eq!(
            matched_backend(
                vec![candidate(1, "One"), candidate(2, "Two")],
                MkPoint { x: 0, y: 0 }
            )
            .resolve(&target(MkPoint { x: 0, y: 0 }), &RuntimeVariables::new())
            .unwrap_err()
            .kind,
            DiagnosticKind::AmbiguousTarget
        );
        let error = matched_backend(vec![candidate(1, "One")], MkPoint { x: i32::MAX, y: 0 })
            .resolve(&target(MkPoint { x: 1, y: 0 }), &RuntimeVariables::new())
            .unwrap_err();
        assert!(error.message.contains("overflow"));
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
            image: MkImageRef::from_filename(format!("{id}.png")),
            offset: MkPoint { x, y },
        };
        let mut vars = RuntimeVariables::new();
        vars.insert(
            "last_image".into(),
            MkValue::Point(MkPoint { x: 99, y: 99 }),
        );
        vars.insert(
            image_result_variable(&MkImageRef::from_filename("2.png")),
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
        vars.insert(
            image_result_variable(&MkImageRef::from_filename("1.png")),
            MkValue::String("bad".into()),
        );
        assert_eq!(
            b.resolve(&target(1, 0, 0), &vars).unwrap_err().kind,
            DiagnosticKind::TypeMismatch
        );
        vars.insert(
            image_result_variable(&MkImageRef::from_filename("1.png")),
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
    struct FailingSource;
    impl MonitorCapture for FailingSource {
        fn capture(&self) -> ExecResult<RgbaImage> {
            Err(ExecutionDiagnostic::new(
                DiagnosticKind::Backend,
                "fixture monitor capture failed",
            ))
        }
    }
    struct CaptureFixture {
        desktop: (i32, i32, i32, i32),
        monitors: Vec<CaptureMonitor>,
    }
    enum WindowFixtureResult {
        Rect {
            outer: ScreenRect,
            client: ScreenRect,
        },
        Missing,
        Multiple,
        Backend,
    }
    struct WindowFixture(WindowFixtureResult);
    impl CapturePlatform for WindowFixture {
        fn virtual_desktop_metrics(&self) -> ExecResult<(i32, i32, i32, i32)> {
            Ok((-2000, -1000, 4000, 2000))
        }
        fn monitors(&self) -> ExecResult<Vec<CaptureMonitor>> {
            Ok(Vec::new())
        }
        fn window_rect(&self, _: &MkWindowMatcher, client: bool) -> ExecResult<ScreenRect> {
            match self.0 {
                WindowFixtureResult::Rect {
                    outer,
                    client: client_rect,
                } => Ok(if client { client_rect } else { outer }),
                WindowFixtureResult::Missing => Err(ExecutionDiagnostic::new(
                    DiagnosticKind::TargetNotFound,
                    "Window search target was not found",
                )),
                WindowFixtureResult::Multiple => Err(ExecutionDiagnostic::new(
                    DiagnosticKind::AmbiguousTarget,
                    "Window search target matched multiple windows",
                )),
                WindowFixtureResult::Backend => Err(ExecutionDiagnostic::new(
                    DiagnosticKind::Backend,
                    "fixture enumeration API failed",
                )),
            }
        }
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
    fn coordinate_monitor(id: u64, rect: ScreenRect, tag: u8) -> CaptureMonitor {
        let image = RgbaImage::from_fn(rect.width, rect.height, |x, y| {
            image::Rgba([tag, x as u8, y as u8, 255])
        });
        CaptureMonitor {
            bounds: rect,
            stable_id: id,
            source: Arc::new(ImageSource(image)),
        }
    }

    #[test]
    fn monitor_descriptors_share_sorted_playback_indices() {
        let platform = Arc::new(CaptureFixture {
            desktop: (-1920, 0, 3840, 1080),
            monitors: vec![
                monitor(8, ScreenRect::new(0, 0, 1920, 1080), [0; 4]),
                monitor(9, ScreenRect::new(-1920, 0, 1920, 1080), [0; 4]),
            ],
        });
        let backend = WindowsScreenCaptureBackend::new(platform);
        let descriptors = backend.descriptors().unwrap();
        assert_eq!(descriptors[0].bounds, ScreenRect::new(-1920, 0, 1920, 1080));
        assert_eq!(descriptors[0].label(), "Monitor 0 — 1920×1080 @ (-1920, 0)");
        assert_eq!(
            descriptors[1].label(),
            "Monitor 1 — 1920×1080 @ (0, 0) — Primary"
        );
        for descriptor in descriptors {
            assert_eq!(
                backend
                    .region_bounds(&SearchRegion::Monitor {
                        index: descriptor.index
                    })
                    .unwrap(),
                descriptor.bounds
            );
        }
    }

    #[test]
    fn window_and_client_regions_use_injected_desktop_rectangles() {
        let backend =
            WindowsScreenCaptureBackend::new(Arc::new(WindowFixture(WindowFixtureResult::Rect {
                outer: ScreenRect::new(-300, 40, 900, 700),
                client: ScreenRect::new(-292, 72, 884, 660),
            })));
        let matcher = MkWindowMatcher::default();
        assert_eq!(
            backend
                .region_bounds(&SearchRegion::Window {
                    matcher: matcher.clone()
                })
                .unwrap(),
            ScreenRect::new(-300, 40, 900, 700)
        );
        assert_eq!(
            backend
                .region_bounds(&SearchRegion::ClientArea { matcher })
                .unwrap(),
            ScreenRect::new(-292, 72, 884, 660)
        );
    }

    #[test]
    fn window_resolution_preserves_missing_ambiguous_and_backend_diagnostics() {
        let matcher = MkWindowMatcher::default();
        for (fixture, kind, message) in [
            (
                WindowFixtureResult::Missing,
                DiagnosticKind::TargetNotFound,
                "Window search target was not found",
            ),
            (
                WindowFixtureResult::Multiple,
                DiagnosticKind::AmbiguousTarget,
                "Window search target matched multiple windows",
            ),
            (
                WindowFixtureResult::Backend,
                DiagnosticKind::Backend,
                "fixture enumeration API failed",
            ),
        ] {
            let backend = WindowsScreenCaptureBackend::new(Arc::new(WindowFixture(fixture)));
            let error = backend
                .region_bounds(&SearchRegion::Window {
                    matcher: matcher.clone(),
                })
                .unwrap_err();
            assert_eq!(error.kind, kind);
            assert_eq!(error.message, message);
            assert_eq!(
                error.context.get("backend").map(String::as_str),
                Some("WindowsScreenCaptureBackend")
            );
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
    fn side_by_side_union_and_boundary_crossing_preserve_monitor_local_pixels() {
        let monitors = vec![
            coordinate_monitor(1, ScreenRect::new(0, 0, 3, 2), 10),
            coordinate_monitor(2, ScreenRect::new(3, 0, 2, 2), 20),
        ];
        let union = compose_monitors(ScreenRect::new(0, 0, 5, 2), &monitors, &|| false).unwrap();
        assert_eq!(union.dimensions(), (5, 2));
        assert_eq!(union.get_pixel(2, 1).0, [10, 2, 1, 255]);
        assert_eq!(union.get_pixel(3, 1).0, [20, 0, 1, 255]);

        let seam = compose_monitors(ScreenRect::new(2, 0, 2, 2), &monitors, &|| false).unwrap();
        assert_eq!(seam.get_pixel(0, 0).0, [10, 2, 0, 255]);
        assert_eq!(seam.get_pixel(1, 0).0, [20, 0, 0, 255]);
    }

    #[test]
    fn l_shaped_layout_maps_three_intersections_and_leaves_uncovered_quadrant() {
        let monitors = vec![
            coordinate_monitor(1, ScreenRect::new(0, 0, 2, 2), 1),
            coordinate_monitor(2, ScreenRect::new(2, 0, 2, 2), 2),
            coordinate_monitor(3, ScreenRect::new(0, 2, 2, 2), 3),
        ];
        let image = compose_monitors(ScreenRect::new(0, 0, 4, 4), &monitors, &|| false).unwrap();
        assert_eq!(image.get_pixel(1, 1).0, [1, 1, 1, 255]);
        assert_eq!(image.get_pixel(3, 1).0, [2, 1, 1, 255]);
        assert_eq!(image.get_pixel(1, 3).0, [3, 1, 1, 255]);
        assert_eq!(image.get_pixel(3, 3).0, [0, 0, 0, 255]);
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
    fn compositor_fills_diagonally_offset_desktop_gaps_with_opaque_black() {
        let monitors = vec![
            monitor(1, ScreenRect::new(0, 0, 2, 2), [200, 1, 2, 255]),
            monitor(2, ScreenRect::new(2, 2, 2, 2), [3, 200, 4, 255]),
        ];
        let image = compose_monitors(ScreenRect::new(0, 0, 4, 4), &monitors, &|| false).unwrap();

        assert_eq!(image.get_pixel(1, 1).0, [200, 1, 2, 255]);
        assert_eq!(image.get_pixel(2, 2).0, [3, 200, 4, 255]);
        assert_eq!(image.get_pixel(3, 0).0, [0, 0, 0, 255]);
        assert_eq!(image.get_pixel(0, 3).0, [0, 0, 0, 255]);
    }

    #[test]
    fn rectangle_spanning_two_monitors_preserves_offsets_and_black_gap() {
        let monitors = vec![
            monitor(1, ScreenRect::new(0, 0, 2, 2), [10, 0, 0, 255]),
            monitor(2, ScreenRect::new(4, 1, 2, 2), [20, 0, 0, 255]),
        ];
        let image = compose_monitors(ScreenRect::new(1, 0, 4, 3), &monitors, &|| false).unwrap();

        assert_eq!(image.dimensions(), (4, 3));
        assert_eq!(image.get_pixel(0, 0).0, [10, 0, 0, 255]);
        assert_eq!(image.get_pixel(3, 1).0, [20, 0, 0, 255]);
        assert_eq!(image.get_pixel(1, 1).0, [0, 0, 0, 255]);
        assert_eq!(image.get_pixel(3, 0).0, [0, 0, 0, 255]);
    }

    #[test]
    fn negative_desktop_capture_maps_pixels_and_signed_origin() {
        let backend = WindowsScreenCaptureBackend::new(Arc::new(CaptureFixture {
            desktop: (-3, -2, 4, 3),
            monitors: vec![monitor(1, ScreenRect::new(-3, -2, 2, 2), [9, 8, 7, 255])],
        }));
        let capture = backend.capture(&SearchRegion::Desktop, &|| false).unwrap();

        assert_eq!(capture.origin, (-3, -2));
        assert_eq!(capture.image.get_pixel(0, 0).0, [9, 8, 7, 255]);
        assert_eq!(capture.image.get_pixel(3, 2).0, [0, 0, 0, 255]);
        assert_eq!(capture.desktop_point((0, 0)), Some((-3, -2)));
        assert_eq!(capture.desktop_point((3, 2)), Some((0, 0)));
        assert_eq!(capture.local_point((-3, -2)), Some((0, 0)));
        assert_eq!(capture.local_point((0, 0)), Some((3, 2)));
    }

    #[test]
    fn rectangle_wholly_in_virtual_desktop_gap_is_opaque_black() {
        let backend = WindowsScreenCaptureBackend::new(Arc::new(CaptureFixture {
            desktop: (0, 0, 5, 2),
            monitors: vec![
                monitor(1, ScreenRect::new(0, 0, 1, 2), [1, 2, 3, 255]),
                monitor(2, ScreenRect::new(4, 0, 1, 2), [4, 5, 6, 255]),
            ],
        }));
        let capture = backend
            .capture(
                &SearchRegion::Rectangle {
                    rect: ScreenRect::new(1, 0, 3, 2),
                },
                &|| false,
            )
            .unwrap();

        assert_eq!(capture.image.dimensions(), (3, 2));
        assert!(
            capture
                .image
                .pixels()
                .all(|pixel| pixel.0 == [0, 0, 0, 255])
        );
    }

    #[test]
    fn validation_rejects_empty_overflow_and_outside_rectangles_but_fills_gaps() {
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
        let image = backend
            .capture_rect(ScreenRect::new(0, 0, 2, 2), &|| false)
            .unwrap();
        assert_eq!(image.get_pixel(1, 1).0, [0, 0, 0, 255]);
        for rect in [
            ScreenRect::new(i32::MAX, 0, 2, 1),
            ScreenRect::new(0, i32::MAX, 1, 2),
        ] {
            let error = compose_monitors(rect, &[], &|| false).unwrap_err();
            assert_eq!(error.kind, DiagnosticKind::InvalidTarget);
            assert!(error.message.contains("overflow"));
        }
        let allocation_error = compose_monitors(
            ScreenRect::new(i32::MIN, i32::MIN, u32::MAX, u32::MAX),
            &[],
            &|| false,
        )
        .unwrap_err();
        assert_eq!(allocation_error.kind, DiagnosticKind::InvalidTarget);
        assert!(
            allocation_error
                .message
                .contains("allocation size overflow")
        );
    }

    #[test]
    fn compositor_rejects_bad_dimensions_and_capture_errors() {
        let bad_dimensions = CaptureMonitor {
            bounds: ScreenRect::new(0, 0, 2, 1),
            stable_id: 1,
            source: Arc::new(ImageSource(RgbaImage::new(1, 1))),
        };
        let error = compose_monitors(ScreenRect::new(0, 0, 2, 1), &[bad_dimensions], &|| false)
            .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::Backend);
        assert!(error.message.contains("returned 1x1"));

        let failing = CaptureMonitor {
            bounds: ScreenRect::new(0, 0, 1, 1),
            stable_id: 2,
            source: Arc::new(FailingSource),
        };
        let error =
            compose_monitors(ScreenRect::new(0, 0, 1, 1), &[failing], &|| false).unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::Backend);
        assert!(error.message.contains("fixture monitor capture failed"));
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
        let error = backend
            .region_bounds(&SearchRegion::Monitor { index: 2 })
            .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::TargetNotFound);
        assert_eq!(error.message, "Monitor 2 is not currently available");
        assert_eq!(
            error.context.get("monitor_index").map(String::as_str),
            Some("2")
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
