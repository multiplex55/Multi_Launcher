use std::fmt;
use std::sync::Arc;

use crate::mkmacro::screen::ScreenRect;

use super::ScreenDrawGeneration;

/// Extra physical pixels kept between the virtual desktop and the parked
/// launcher. This protects capture exclusion from small platform adjustments.
pub(crate) const CAPTURE_PARKING_MARGIN: i64 = 96;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LauncherWindowRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl LauncherWindowRect {
    fn dimensions(self) -> Result<(u32, u32), String> {
        let width = i64::from(self.right) - i64::from(self.left);
        let height = i64::from(self.bottom) - i64::from(self.top);
        if width <= 0 || height <= 0 {
            return Err(format!(
                "launcher has invalid native geometry ({}, {})-({}, {})",
                self.left, self.top, self.right, self.bottom
            ));
        }
        Ok((
            u32::try_from(width).map_err(|_| "launcher width is not representable".to_string())?,
            u32::try_from(height)
                .map_err(|_| "launcher height is not representable".to_string())?,
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LauncherWindowSnapshot {
    hwnd: usize,
    rect: LauncherWindowRect,
}

impl LauncherWindowSnapshot {
    pub(crate) const fn hwnd(self) -> usize {
        self.hwnd
    }

    pub(crate) const fn rect(self) -> LauncherWindowRect {
        self.rect
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LauncherParkingState {
    Active,
    Committed,
    Restored,
}

trait LauncherWindowApi: Send + Sync {
    fn snapshot(&self, hwnd: usize) -> Result<LauncherWindowSnapshot, String>;
    fn park(&self, hwnd: usize, position: (i32, i32)) -> Result<(), String>;
    fn restore(&self, snapshot: LauncherWindowSnapshot) -> Result<(), String>;
    fn is_capture_safe(&self, hwnd: usize, desktop: ScreenRect) -> Result<bool, String>;
}

#[derive(Debug, Default)]
struct SystemLauncherWindowApi;

impl LauncherWindowApi for SystemLauncherWindowApi {
    fn snapshot(&self, hwnd: usize) -> Result<LauncherWindowSnapshot, String> {
        system_snapshot(hwnd)
    }

    fn park(&self, hwnd: usize, position: (i32, i32)) -> Result<(), String> {
        system_park(hwnd, position)
    }

    fn restore(&self, snapshot: LauncherWindowSnapshot) -> Result<(), String> {
        system_restore(snapshot)
    }

    fn is_capture_safe(&self, hwnd: usize, desktop: ScreenRect) -> Result<bool, String> {
        system_launcher_is_capture_safe(Some(hwnd), desktop)
    }
}

/// Owns one launcher parking attempt. Dropping an uncommitted transaction
/// restores its exact pre-parking native rectangle, including during early
/// returns and unwinding.
pub(crate) struct LauncherParkingTransaction {
    generation: ScreenDrawGeneration,
    original_snapshot: LauncherWindowSnapshot,
    parked_rect: LauncherWindowRect,
    virtual_desktop: ScreenRect,
    state: LauncherParkingState,
    window_api: Arc<dyn LauncherWindowApi>,
}

impl fmt::Debug for LauncherParkingTransaction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LauncherParkingTransaction")
            .field("generation", &self.generation)
            .field("original_snapshot", &self.original_snapshot)
            .field("parked_rect", &self.parked_rect)
            .field("virtual_desktop", &self.virtual_desktop)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl LauncherParkingTransaction {
    pub(crate) fn begin(
        generation: ScreenDrawGeneration,
        hwnd: usize,
        virtual_desktop: ScreenRect,
    ) -> Result<Self, String> {
        Self::begin_with_api(
            generation,
            hwnd,
            virtual_desktop,
            Arc::new(SystemLauncherWindowApi),
        )
    }

    fn begin_with_api(
        generation: ScreenDrawGeneration,
        hwnd: usize,
        virtual_desktop: ScreenRect,
        window_api: Arc<dyn LauncherWindowApi>,
    ) -> Result<Self, String> {
        let original_snapshot = window_api.snapshot(hwnd)?;
        let (width, height) = original_snapshot.rect.dimensions()?;
        let position = compute_capture_safe_parking_position(
            virtual_desktop,
            width,
            height,
            CAPTURE_PARKING_MARGIN,
        )?;
        window_api.park(hwnd, position)?;

        let parked_rect = rect_at(position, width, height)?;
        Ok(Self {
            generation,
            original_snapshot,
            parked_rect,
            virtual_desktop,
            state: LauncherParkingState::Active,
            window_api,
        })
    }

    pub(crate) const fn generation(&self) -> ScreenDrawGeneration {
        self.generation
    }

    pub(crate) const fn original_snapshot(&self) -> LauncherWindowSnapshot {
        self.original_snapshot
    }

    pub(crate) const fn parked_rect(&self) -> LauncherWindowRect {
        self.parked_rect
    }

    pub(crate) const fn state(&self) -> LauncherParkingState {
        self.state
    }

    /// A committed transaction deliberately leaves the launcher parked when
    /// ownership is released after Screen Draw becomes usable.
    pub(crate) fn commit_hidden(&mut self) {
        if self.state == LauncherParkingState::Active {
            self.state = LauncherParkingState::Committed;
        }
    }

    pub(crate) fn verify(&self) -> Result<bool, String> {
        if self.state == LauncherParkingState::Restored {
            return Ok(false);
        }
        self.window_api
            .is_capture_safe(self.original_snapshot.hwnd, self.virtual_desktop)
    }

    pub(crate) fn restore(&mut self) -> Result<(), String> {
        if self.state == LauncherParkingState::Restored {
            return Ok(());
        }
        self.window_api.restore(self.original_snapshot)?;
        self.state = LauncherParkingState::Restored;
        Ok(())
    }

    /// Snapshots the launcher's current native rectangle and begins a fresh
    /// parking attempt. This is used when resuming after the launcher was
    /// restored and may have been moved or resized by the user.
    pub(crate) fn update_snapshot_before_repark(
        &mut self,
        virtual_desktop: ScreenRect,
    ) -> Result<(), String> {
        let snapshot = self.window_api.snapshot(self.original_snapshot.hwnd)?;
        let (width, height) = snapshot.rect.dimensions()?;
        // Publish the fresh restore point before any fallible parking work so
        // recovery can never fall back to geometry captured before the user
        // moved or resized the restored launcher.
        self.original_snapshot = snapshot;
        self.virtual_desktop = virtual_desktop;
        self.state = LauncherParkingState::Restored;
        let position = compute_capture_safe_parking_position(
            virtual_desktop,
            width,
            height,
            CAPTURE_PARKING_MARGIN,
        )?;
        self.window_api.park(snapshot.hwnd, position)?;

        self.parked_rect = rect_at(position, width, height)?;
        self.state = LauncherParkingState::Active;
        Ok(())
    }
}

impl Drop for LauncherParkingTransaction {
    fn drop(&mut self) {
        if self.state == LauncherParkingState::Active {
            let _ = self.restore();
        }
    }
}

#[cfg(test)]
#[derive(Clone)]
pub(crate) struct LauncherParkingTestObserver {
    restores: Arc<std::sync::Mutex<Vec<LauncherWindowSnapshot>>>,
    current: Arc<std::sync::Mutex<LauncherWindowSnapshot>>,
    fail_next_park: Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(test)]
impl LauncherParkingTestObserver {
    pub(crate) fn restored_rects(&self) -> Vec<LauncherWindowRect> {
        self.restores
            .lock()
            .unwrap()
            .iter()
            .map(|snapshot| snapshot.rect)
            .collect()
    }

    pub(crate) fn set_current_rect(&self, rect: LauncherWindowRect) {
        self.current.lock().unwrap().rect = rect;
    }

    pub(crate) fn current_rect(&self) -> LauncherWindowRect {
        self.current.lock().unwrap().rect
    }

    pub(crate) fn fail_next_park(&self) {
        self.fail_next_park
            .store(true, std::sync::atomic::Ordering::Release);
    }
}

#[cfg(test)]
struct GuiTestLauncherWindowApi {
    current: Arc<std::sync::Mutex<LauncherWindowSnapshot>>,
    restores: Arc<std::sync::Mutex<Vec<LauncherWindowSnapshot>>>,
    fail_next_park: Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(test)]
impl LauncherWindowApi for GuiTestLauncherWindowApi {
    fn snapshot(&self, _hwnd: usize) -> Result<LauncherWindowSnapshot, String> {
        Ok(*self.current.lock().unwrap())
    }

    fn park(&self, _hwnd: usize, position: (i32, i32)) -> Result<(), String> {
        if self
            .fail_next_park
            .swap(false, std::sync::atomic::Ordering::AcqRel)
        {
            return Err("fixture launcher park failed".into());
        }
        let mut current = self.current.lock().unwrap();
        let (width, height) = current.rect.dimensions()?;
        current.rect = rect_at(position, width, height)?;
        Ok(())
    }

    fn restore(&self, snapshot: LauncherWindowSnapshot) -> Result<(), String> {
        self.restores.lock().unwrap().push(snapshot);
        *self.current.lock().unwrap() = snapshot;
        Ok(())
    }

    fn is_capture_safe(&self, _hwnd: usize, _desktop: ScreenRect) -> Result<bool, String> {
        Ok(true)
    }
}

#[cfg(test)]
pub(crate) fn launcher_parking_test_fixture(
    generation: ScreenDrawGeneration,
    rect: LauncherWindowRect,
    virtual_desktop: ScreenRect,
) -> (LauncherParkingTransaction, LauncherParkingTestObserver) {
    let restores = Arc::new(std::sync::Mutex::new(Vec::new()));
    let current = Arc::new(std::sync::Mutex::new(LauncherWindowSnapshot {
        hwnd: 42,
        rect,
    }));
    let fail_next_park = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let api = Arc::new(GuiTestLauncherWindowApi {
        current: Arc::clone(&current),
        restores: Arc::clone(&restores),
        fail_next_park: Arc::clone(&fail_next_park),
    });
    let transaction =
        LauncherParkingTransaction::begin_with_api(generation, 42, virtual_desktop, api)
            .expect("GUI parking fixture has valid geometry");
    (
        transaction,
        LauncherParkingTestObserver {
            restores,
            current,
            fail_next_park,
        },
    )
}

/// Chooses the first representable capture-safe location in the order right,
/// left, below, above. All arithmetic is widened so extreme signed desktop
/// coordinates cannot wrap.
pub(crate) fn compute_capture_safe_parking_position(
    desktop: ScreenRect,
    launcher_width: u32,
    launcher_height: u32,
    margin: i64,
) -> Result<(i32, i32), String> {
    if desktop.is_empty() {
        return Err("cannot park launcher outside an empty virtual desktop".into());
    }
    if launcher_width == 0 || launcher_height == 0 {
        return Err("cannot park a zero-sized launcher window".into());
    }
    if margin < 0 {
        return Err("launcher parking margin cannot be negative".into());
    }

    let left = i64::from(desktop.x);
    let top = i64::from(desktop.y);
    let right = desktop.right();
    let bottom = desktop.bottom();
    let width = i64::from(launcher_width);
    let height = i64::from(launcher_height);

    let candidates = [
        (right.checked_add(margin), Some(top)),
        (
            left.checked_sub(margin).and_then(|x| x.checked_sub(width)),
            Some(top),
        ),
        (Some(left), bottom.checked_add(margin)),
        (
            Some(left),
            top.checked_sub(margin).and_then(|y| y.checked_sub(height)),
        ),
    ];

    candidates
        .into_iter()
        .filter_map(|(x, y)| Some((x?, y?)))
        .find_map(|(x, y)| representable_rect_position(x, y, width, height))
        .ok_or_else(|| {
            "no capture-safe launcher position is representable in Win32 coordinates".into()
        })
}

fn representable_rect_position(x: i64, y: i64, width: i64, height: i64) -> Option<(i32, i32)> {
    let right = x.checked_add(width)?;
    let bottom = y.checked_add(height)?;
    if x < i64::from(i32::MIN)
        || y < i64::from(i32::MIN)
        || right > i64::from(i32::MAX)
        || bottom > i64::from(i32::MAX)
    {
        return None;
    }
    Some((i32::try_from(x).ok()?, i32::try_from(y).ok()?))
}

fn rect_at(position: (i32, i32), width: u32, height: u32) -> Result<LauncherWindowRect, String> {
    let right = i64::from(position.0) + i64::from(width);
    let bottom = i64::from(position.1) + i64::from(height);
    Ok(LauncherWindowRect {
        left: position.0,
        top: position.1,
        right: i32::try_from(right).map_err(|_| "parked launcher right edge overflowed")?,
        bottom: i32::try_from(bottom).map_err(|_| "parked launcher bottom edge overflowed")?,
    })
}

pub(crate) fn signed_rectangles_intersect(desktop: ScreenRect, window: LauncherWindowRect) -> bool {
    if window.left >= window.right || window.top >= window.bottom {
        return false;
    }
    i64::from(window.left) < desktop.right()
        && i64::from(window.right) > i64::from(desktop.x)
        && i64::from(window.top) < desktop.bottom()
        && i64::from(window.bottom) > i64::from(desktop.y)
}

fn launcher_state_is_capture_safe(
    is_valid: bool,
    is_visible: bool,
    is_iconic: bool,
    desktop: ScreenRect,
    window: LauncherWindowRect,
) -> bool {
    is_valid && is_visible && !is_iconic && !signed_rectangles_intersect(desktop, window)
}

#[cfg(windows)]
pub(crate) fn system_launcher_is_capture_safe(
    launcher_hwnd: Option<usize>,
    virtual_desktop: ScreenRect,
) -> Result<bool, String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{IsIconic, IsWindow, IsWindowVisible};

    let Some(raw_hwnd) = launcher_hwnd else {
        return Ok(false);
    };
    let hwnd = HWND(raw_hwnd as *mut core::ffi::c_void);
    let is_valid = unsafe { IsWindow(hwnd) }.as_bool();
    let is_visible = is_valid && unsafe { IsWindowVisible(hwnd) }.as_bool();
    let is_iconic = is_valid && unsafe { IsIconic(hwnd) }.as_bool();
    if !is_valid || !is_visible || is_iconic {
        return Ok(false);
    }

    let snapshot = system_snapshot(raw_hwnd)?;
    Ok(launcher_state_is_capture_safe(
        is_valid,
        is_visible,
        is_iconic,
        virtual_desktop,
        snapshot.rect,
    ))
}

#[cfg(not(windows))]
pub(crate) fn system_launcher_is_capture_safe(
    _launcher_hwnd: Option<usize>,
    _virtual_desktop: ScreenRect,
) -> Result<bool, String> {
    Err("launcher parking verification is available only on Windows".into())
}

#[cfg(windows)]
fn system_snapshot(hwnd: usize) -> Result<LauncherWindowSnapshot, String> {
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsWindow};

    let native = HWND(hwnd as *mut core::ffi::c_void);
    if !unsafe { IsWindow(native) }.as_bool() {
        return Err("cannot snapshot an invalid launcher HWND".into());
    }
    let mut rect = RECT::default();
    unsafe { GetWindowRect(native, &mut rect) }
        .map_err(|error| format!("GetWindowRect failed for launcher: {error}"))?;
    let snapshot = LauncherWindowSnapshot {
        hwnd,
        rect: LauncherWindowRect {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        },
    };
    snapshot.rect.dimensions()?;
    Ok(snapshot)
}

#[cfg(not(windows))]
fn system_snapshot(_hwnd: usize) -> Result<LauncherWindowSnapshot, String> {
    Err("launcher geometry snapshots are available only on Windows".into())
}

#[cfg(windows)]
fn system_park(hwnd: usize, position: (i32, i32)) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
    };

    unsafe {
        SetWindowPos(
            HWND(hwnd as *mut core::ffi::c_void),
            None,
            position.0,
            position.1,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOSIZE | SWP_NOZORDER,
        )
    }
    .map_err(|error| format!("SetWindowPos failed while parking launcher: {error}"))
}

#[cfg(not(windows))]
fn system_park(_hwnd: usize, _position: (i32, i32)) -> Result<(), String> {
    Err("launcher parking is available only on Windows".into())
}

#[cfg(windows)]
fn system_restore(snapshot: LauncherWindowSnapshot) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER, SetWindowPos,
    };

    let (width, height) = snapshot.rect.dimensions()?;
    unsafe {
        SetWindowPos(
            HWND(snapshot.hwnd as *mut core::ffi::c_void),
            None,
            snapshot.rect.left,
            snapshot.rect.top,
            i32::try_from(width).map_err(|_| "launcher restore width is too large")?,
            i32::try_from(height).map_err(|_| "launcher restore height is too large")?,
            SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER,
        )
    }
    .map_err(|error| format!("SetWindowPos failed while restoring launcher: {error}"))
}

#[cfg(not(windows))]
fn system_restore(_snapshot: LauncherWindowSnapshot) -> Result<(), String> {
    Err("launcher restoration is available only on Windows".into())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct FakeWindowApi {
        snapshot: Mutex<Option<LauncherWindowSnapshot>>,
        parked_at: Mutex<Vec<(i32, i32)>>,
        restores: Mutex<Vec<LauncherWindowSnapshot>>,
        capture_safe: Mutex<bool>,
    }

    impl FakeWindowApi {
        fn at(rect: LauncherWindowRect) -> Self {
            Self {
                snapshot: Mutex::new(Some(LauncherWindowSnapshot { hwnd: 42, rect })),
                ..Self::default()
            }
        }
    }

    impl LauncherWindowApi for FakeWindowApi {
        fn snapshot(&self, _hwnd: usize) -> Result<LauncherWindowSnapshot, String> {
            self.snapshot
                .lock()
                .unwrap()
                .ok_or("missing snapshot".into())
        }

        fn park(&self, _hwnd: usize, position: (i32, i32)) -> Result<(), String> {
            self.parked_at.lock().unwrap().push(position);
            Ok(())
        }

        fn restore(&self, snapshot: LauncherWindowSnapshot) -> Result<(), String> {
            self.restores.lock().unwrap().push(snapshot);
            Ok(())
        }

        fn is_capture_safe(&self, _hwnd: usize, _desktop: ScreenRect) -> Result<bool, String> {
            Ok(*self.capture_safe.lock().unwrap())
        }
    }

    fn rect(left: i32, top: i32, width: i32, height: i32) -> LauncherWindowRect {
        LauncherWindowRect {
            left,
            top,
            right: left + width,
            bottom: top + height,
        }
    }

    #[test]
    fn parking_prefers_right_of_signed_virtual_desktop() {
        let desktop = ScreenRect::new(-1920, -200, 5760, 2360);
        assert_eq!(
            compute_capture_safe_parking_position(desktop, 640, 480, 96).unwrap(),
            (3936, -200)
        );
    }

    #[test]
    fn parking_falls_back_left_when_right_is_not_representable() {
        let desktop = ScreenRect::new(i32::MAX - 100, 0, 100, 100);
        assert_eq!(
            compute_capture_safe_parking_position(desktop, 50, 50, 64).unwrap(),
            (i32::MAX - 214, 0)
        );
    }

    #[test]
    fn parking_falls_back_below_when_horizontal_candidates_are_not_representable() {
        let desktop = ScreenRect::new(i32::MIN, 0, u32::MAX, 100);
        assert_eq!(
            compute_capture_safe_parking_position(desktop, 100, 50, 64).unwrap(),
            (i32::MIN, 164)
        );
    }

    #[test]
    fn parking_rejects_unrepresentable_pathological_geometry() {
        let desktop = ScreenRect::new(i32::MIN, i32::MIN, u32::MAX, u32::MAX);
        assert!(compute_capture_safe_parking_position(desktop, u32::MAX, u32::MAX, 96).is_err());
    }

    #[test]
    fn intersection_uses_signed_half_open_coordinates() {
        let desktop = ScreenRect::new(-1920, -200, 3840, 1280);
        assert!(signed_rectangles_intersect(
            desktop,
            rect(-100, -100, 200, 200)
        ));
        assert!(!signed_rectangles_intersect(
            desktop,
            rect(1920, 0, 100, 100)
        ));
        assert!(!signed_rectangles_intersect(
            desktop,
            rect(-2020, 0, 100, 100)
        ));
    }

    #[test]
    fn capture_safety_requires_valid_visible_non_iconic_offscreen_window() {
        let desktop = ScreenRect::new(0, 0, 1920, 1080);
        let offscreen = rect(2016, 0, 300, 200);
        assert!(launcher_state_is_capture_safe(
            true, true, false, desktop, offscreen
        ));
        assert!(!launcher_state_is_capture_safe(
            false, true, false, desktop, offscreen
        ));
        assert!(!launcher_state_is_capture_safe(
            true, false, false, desktop, offscreen
        ));
        assert!(!launcher_state_is_capture_safe(
            true, true, true, desktop, offscreen
        ));
        assert!(!launcher_state_is_capture_safe(
            true,
            true,
            false,
            desktop,
            rect(1800, 0, 300, 200)
        ));
    }

    #[test]
    fn active_transaction_restores_exact_snapshot_on_drop() {
        let api = Arc::new(FakeWindowApi::at(rect(10, 20, 300, 200)));
        {
            let transaction = LauncherParkingTransaction::begin_with_api(
                ScreenDrawGeneration::from_raw(7),
                42,
                ScreenRect::new(0, 0, 1920, 1080),
                api.clone(),
            )
            .unwrap();
            assert_eq!(transaction.state(), LauncherParkingState::Active);
        }
        assert_eq!(
            api.restores.lock().unwrap().as_slice(),
            &[LauncherWindowSnapshot {
                hwnd: 42,
                rect: rect(10, 20, 300, 200),
            }]
        );
    }

    #[test]
    fn committed_transaction_stays_parked_on_drop() {
        let api = Arc::new(FakeWindowApi::at(rect(10, 20, 300, 200)));
        {
            let mut transaction = LauncherParkingTransaction::begin_with_api(
                ScreenDrawGeneration::from_raw(8),
                42,
                ScreenRect::new(0, 0, 1920, 1080),
                api.clone(),
            )
            .unwrap();
            transaction.commit_hidden();
        }
        assert!(api.restores.lock().unwrap().is_empty());
    }

    #[test]
    fn explicit_restore_is_idempotent() {
        let api = Arc::new(FakeWindowApi::at(rect(10, 20, 300, 200)));
        let mut transaction = LauncherParkingTransaction::begin_with_api(
            ScreenDrawGeneration::from_raw(9),
            42,
            ScreenRect::new(0, 0, 1920, 1080),
            api.clone(),
        )
        .unwrap();
        transaction.restore().unwrap();
        transaction.restore().unwrap();
        assert_eq!(api.restores.lock().unwrap().len(), 1);
    }

    #[test]
    fn repark_refreshes_the_restore_snapshot() {
        let api = Arc::new(FakeWindowApi::at(rect(10, 20, 300, 200)));
        let mut transaction = LauncherParkingTransaction::begin_with_api(
            ScreenDrawGeneration::from_raw(10),
            42,
            ScreenRect::new(0, 0, 1920, 1080),
            api.clone(),
        )
        .unwrap();
        transaction.restore().unwrap();
        *api.snapshot.lock().unwrap() = Some(LauncherWindowSnapshot {
            hwnd: 42,
            rect: rect(500, 600, 400, 250),
        });
        transaction
            .update_snapshot_before_repark(ScreenRect::new(-100, -100, 2100, 1200))
            .unwrap();
        transaction.restore().unwrap();

        let restores = api.restores.lock().unwrap();
        assert_eq!(restores.len(), 2);
        assert_eq!(restores[1].rect, rect(500, 600, 400, 250));
    }
}
