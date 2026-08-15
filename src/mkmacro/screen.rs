//! Platform-independent screen capture geometry used by image and pixel search.
//! Coordinates in a [`CapturedRegion`] are local; `origin` converts them back to
//! virtual-desktop coordinates (and may therefore be negative).
use crate::mkmacro::{
    DiagnosticKind, ExecResult, ExecutionDiagnostic, MkCoordinateTarget, MkPoint, MkValue,
    MkWindowMatcher, RuntimeVariables, ScreenBackend,
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
    fn image_found(&self, asset_id: u64, confidence: f32) -> ExecResult<Option<MkPoint>>;
    fn pixel_matches(&self, point: MkPoint, color: &str, tolerance: u8) -> ExecResult<bool>;
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
}

impl ScreenBackend for WindowsScreenBackend {
    fn resolve(
        &self,
        target: &MkCoordinateTarget,
        variables: &RuntimeVariables,
    ) -> ExecResult<MkPoint> {
        match target {
            MkCoordinateTarget::Screen { point } => self.clamp_to_desktop(*point),
            MkCoordinateTarget::ActiveWindow { point } => {
                let hwnd = self.geometry.foreground_window()?.ok_or_else(|| {
                    ExecutionDiagnostic::new(
                        DiagnosticKind::TargetNotFound,
                        "no foreground window is available",
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
                self.clamp_to_desktop(desktop)
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

    fn image_found(&self, asset_id: u64, confidence: f32) -> ExecResult<Option<MkPoint>> {
        self.visual.image_found(asset_id, confidence)
    }

    fn pixel_matches(
        &self,
        target: &MkCoordinateTarget,
        color: &str,
        tolerance: u8,
        variables: &RuntimeVariables,
    ) -> ExecResult<bool> {
        self.visual
            .pixel_matches(self.resolve(target, variables)?, color, tolerance)
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
        format!("variable '{name}' must be Point, but is {actual}"),
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
    fn image_found(&self, _: u64, _: f32) -> ExecResult<Option<MkPoint>> {
        Err(ExecutionDiagnostic::new(
            DiagnosticKind::UnsupportedOperation,
            "production image lookup is not configured",
        )
        .context("backend", "WindowsScreenBackend"))
    }
    fn pixel_matches(&self, _: MkPoint, _: &str, _: u8) -> ExecResult<bool> {
        Err(ExecutionDiagnostic::new(
            DiagnosticKind::UnsupportedOperation,
            "production pixel lookup is not configured",
        )
        .context("backend", "WindowsScreenBackend"))
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

#[cfg(test)]
mod windows_backend_tests {
    use super::*;
    use std::sync::Mutex;

    #[cfg(windows)]
    #[test]
    fn system_constructor_has_the_production_screen_backend_type() {
        let _: WindowsScreenBackend = WindowsScreenBackend::system();
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
        fn image_found(&self, id: u64, _: f32) -> ExecResult<Option<MkPoint>> {
            self.requested.lock().unwrap().push(id);
            Ok(None)
        }
        fn pixel_matches(&self, _: MkPoint, _: &str, _: u8) -> ExecResult<bool> {
            Ok(true)
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
    fn screen(point: MkPoint) -> MkCoordinateTarget {
        MkCoordinateTarget::Screen { point }
    }

    #[test]
    fn screen_coordinates_are_desktop_pixels_and_clamp_every_edge() {
        let b = backend((-100, -50, 300, 100), None, MkPoint { x: 0, y: 0 });
        for (input, expected) in [
            ((0, 0), (0, 0)),
            ((-101, -51), (-100, -50)),
            ((200, 50), (199, 49)),
            ((-200, 20), (-100, 20)),
            ((20, 100), (20, 49)),
        ] {
            assert_eq!(
                b.resolve(
                    &screen(MkPoint {
                        x: input.0,
                        y: input.1
                    }),
                    &RuntimeVariables::new()
                )
                .unwrap(),
                MkPoint {
                    x: expected.0,
                    y: expected.1
                }
            );
        }
    }
    #[test]
    fn one_pixel_and_invalid_or_overflowing_desktops() {
        let one = backend((-7, 9, 1, 1), None, MkPoint { x: 0, y: 0 });
        assert_eq!(
            one.resolve(
                &screen(MkPoint {
                    x: i32::MAX,
                    y: i32::MIN
                }),
                &RuntimeVariables::new()
            )
            .unwrap(),
            MkPoint { x: -7, y: 9 }
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
}
