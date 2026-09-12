use std::fmt;
use std::sync::Arc;

use super::launcher_parking::system_launcher_is_capture_safe;
use crate::mkmacro::screen::{
    CapturedRegion, ScreenCaptureBackend, ScreenRect, SearchRegion, WindowsScreenCaptureBackend,
};

/// Narrow Screen Draw adapter over the shared MkMacro virtual-desktop
/// compositor. Screen Draw intentionally does not own another monitor capture
/// implementation.
pub struct ScreenDrawCaptureBackend {
    inner: Arc<dyn ScreenCaptureBackend>,
}

impl ScreenDrawCaptureBackend {
    #[cfg(windows)]
    pub fn system() -> Self {
        Self::new(Arc::new(WindowsScreenCaptureBackend::system()))
    }

    pub(crate) fn new(inner: Arc<dyn ScreenCaptureBackend>) -> Self {
        Self { inner }
    }
}

impl fmt::Debug for ScreenDrawCaptureBackend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ScreenDrawCaptureBackend")
            .finish_non_exhaustive()
    }
}

pub(crate) trait DesktopCaptureBackend: Send + Sync {
    fn virtual_desktop(&self) -> Result<ScreenRect, String>;
    fn capture_desktop(&self, cancelled: &dyn Fn() -> bool) -> Result<CapturedRegion, String>;
}

impl DesktopCaptureBackend for ScreenDrawCaptureBackend {
    fn virtual_desktop(&self) -> Result<ScreenRect, String> {
        self.inner
            .virtual_desktop()
            .map_err(|error| error.to_string())
    }

    fn capture_desktop(&self, cancelled: &dyn Fn() -> bool) -> Result<CapturedRegion, String> {
        self.inner
            .capture(&SearchRegion::Desktop, cancelled)
            .map_err(|error| error.to_string())
    }
}

/// Immutable pixels and signed virtual-desktop origin for one completed
/// Screen Draw generation. Later native workers can cheaply clone the `Arc`
/// without copying the full desktop image.
#[derive(Debug, Clone)]
pub struct ScreenDrawSessionSnapshot {
    generation: super::ScreenDrawGeneration,
    capture: Arc<CapturedRegion>,
}

impl ScreenDrawSessionSnapshot {
    pub(crate) fn new(generation: super::ScreenDrawGeneration, capture: CapturedRegion) -> Self {
        Self {
            generation,
            capture: Arc::new(capture),
        }
    }

    pub const fn generation(&self) -> super::ScreenDrawGeneration {
        self.generation
    }

    pub fn capture(&self) -> &Arc<CapturedRegion> {
        &self.capture
    }
}

pub(crate) trait LauncherVisibilityProbe: Send + Sync {
    /// Returns true only when the launcher is known not to contribute pixels to
    /// the requested virtual-desktop capture.
    fn launcher_is_capture_parked(
        &self,
        launcher_hwnd: Option<usize>,
        virtual_desktop: ScreenRect,
    ) -> Result<bool, String>;
}

#[derive(Debug, Default)]
pub(crate) struct SystemLauncherVisibilityProbe;

impl LauncherVisibilityProbe for SystemLauncherVisibilityProbe {
    fn launcher_is_capture_parked(
        &self,
        launcher_hwnd: Option<usize>,
        virtual_desktop: ScreenRect,
    ) -> Result<bool, String> {
        system_launcher_is_capture_safe(launcher_hwnd, virtual_desktop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::ExecResult;
    use image::{Rgba, RgbaImage};

    struct SharedCaptureFixture;

    impl ScreenCaptureBackend for SharedCaptureFixture {
        fn virtual_desktop(&self) -> ExecResult<ScreenRect> {
            Ok(ScreenRect::new(-3, -2, 5, 4))
        }

        fn region_bounds(&self, region: &SearchRegion) -> ExecResult<ScreenRect> {
            assert_eq!(region, &SearchRegion::Desktop);
            self.virtual_desktop()
        }

        fn capture_rect(
            &self,
            rect: ScreenRect,
            cancelled: &dyn Fn() -> bool,
        ) -> ExecResult<RgbaImage> {
            assert_eq!(rect, ScreenRect::new(-3, -2, 5, 4));
            assert!(!cancelled());
            Ok(RgbaImage::from_pixel(5, 4, Rgba([1, 2, 3, 255])))
        }
    }

    #[test]
    fn adapter_delegates_desktop_composition_and_preserves_signed_origin() {
        let adapter = ScreenDrawCaptureBackend::new(Arc::new(SharedCaptureFixture));
        let capture = adapter.capture_desktop(&|| false).unwrap();
        assert_eq!(capture.origin, (-3, -2));
        assert_eq!(capture.image.dimensions(), (5, 4));
        assert_eq!(capture.image.get_pixel(4, 3).0, [1, 2, 3, 255]);
    }
}
