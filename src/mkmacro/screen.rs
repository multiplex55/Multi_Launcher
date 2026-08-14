//! Platform-independent screen capture geometry used by image and pixel search.
//! Coordinates in a [`CapturedRegion`] are local; `origin` converts them back to
//! virtual-desktop coordinates (and may therefore be negative).
use crate::mkmacro::{DiagnosticKind, ExecResult, ExecutionDiagnostic, MkWindowMatcher};
use image::RgbaImage;

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
