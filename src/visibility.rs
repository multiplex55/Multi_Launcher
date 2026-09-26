use eframe::egui;
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, AtomicIsize, AtomicU8, AtomicU64, Ordering},
};

use crate::hotkey::HotkeyTrigger;
use crate::mkmacro::screen::ScreenRect;
use crate::radial::acceptance_trace::{
    self, Correlation, Event, RootCommandKind, VisibilitySource,
};
use crate::screen_draw::launcher_parking::{
    CAPTURE_PARKING_MARGIN, compute_capture_safe_parking_position,
};

/// A small, explicit wake boundary for work owned by one egui viewport.
/// Keeping the viewport id with the callback prevents background producers
/// from accidentally waking whichever viewport happened to be current on the
/// GUI thread.
#[derive(Clone)]
pub struct ViewportWake {
    viewport: egui::ViewportId,
    request: Arc<dyn Fn(egui::ViewportId) + Send + Sync>,
}

impl ViewportWake {
    pub fn for_context(ctx: &egui::Context, viewport: egui::ViewportId) -> Self {
        let ctx = ctx.clone();
        Self {
            viewport,
            request: Arc::new(move |viewport| ctx.request_repaint_of(viewport)),
        }
    }

    pub fn root(ctx: &egui::Context) -> Self {
        Self::for_context(ctx, egui::ViewportId::ROOT)
    }

    #[cfg(test)]
    pub(crate) fn from_callback(
        viewport: egui::ViewportId,
        request: impl Fn(egui::ViewportId) + Send + Sync + 'static,
    ) -> Self {
        Self {
            viewport,
            request: Arc::new(request),
        }
    }

    pub(crate) fn wake(&self) {
        (self.request)(self.viewport);
    }
}

/// Trait abstracting over an `egui::Context` for viewport commands.
pub trait ViewportCtx {
    /// Make the native viewport eligible to process the commands that follow.
    ///
    /// Most contexts need no special handling. The root Windows viewport uses
    /// this hook to break the hidden-window redraw cycle before egui queues its
    /// normal placement, visibility, and focus commands.
    fn wake_for_show(&self) {}

    /// Present a viewport without asking the platform backend to activate it.
    /// Contexts that cannot separate visibility from activation keep the
    /// regular viewport command behavior.
    fn show_without_activation(&self) {
        self.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    }

    fn pixels_per_point(&self) -> f32 {
        1.0
    }

    fn send_viewport_cmd(&self, cmd: egui::ViewportCommand);
    fn request_repaint(&self);
}

impl ViewportCtx for egui::Context {
    fn pixels_per_point(&self) -> f32 {
        egui::Context::pixels_per_point(self)
    }

    fn send_viewport_cmd(&self, cmd: egui::ViewportCommand) {
        egui::Context::send_viewport_cmd(self, cmd);
    }

    fn request_repaint(&self) {
        egui::Context::request_repaint(self);
    }
}

/// The native ROOT HWND captured by the GUI owner.
///
/// The handle is stored as an integer so the bridge can cross from eframe's
/// GUI thread to the main hotkey thread without extending a borrowed raw
/// window handle. Every use revalidates both the HWND and its owning process.
#[derive(Clone, Default)]
pub struct RootWindowBridge {
    hwnd: Arc<AtomicIsize>,
    generation: Arc<AtomicU64>,
    designer_hwnd: Arc<AtomicIsize>,
    designer_generation: Arc<AtomicU64>,
    identity_gate: Arc<Mutex<()>>,
    presentation_reconcile_requested: Arc<AtomicBool>,
    repaint_context: Arc<OnceLock<egui::Context>>,
}

impl RootWindowBridge {
    fn publish_hwnd(&self, hwnd: isize) {
        let _gate = self
            .identity_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.hwnd.load(Ordering::Acquire) != hwnd {
            self.hwnd.store(hwnd, Ordering::Release);
            self.generation.fetch_add(1, Ordering::AcqRel);
        }
    }

    pub fn identity(&self) -> (usize, u64) {
        let _gate = self
            .identity_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (
            self.hwnd.load(Ordering::Acquire).max(0) as usize,
            self.generation.load(Ordering::Acquire),
        )
    }

    pub fn is_current(&self, hwnd: usize, generation: u64) -> bool {
        let _gate = self
            .identity_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.hwnd.load(Ordering::Acquire) == hwnd as isize
            && self.generation.load(Ordering::Acquire) == generation
    }

    /// Request a ROOT frame to reapply its current visibility-owned native
    /// presentation after a fenced activation was superseded while a Windows
    /// call was in flight. Geometry remains owned by the GUI and, when active,
    /// by the Screen Draw parking transaction.
    pub(crate) fn request_presentation_reconcile(&self) {
        self.presentation_reconcile_requested
            .store(true, Ordering::Release);
        if let Some(context) = self.repaint_context.get() {
            context.request_repaint_of(egui::ViewportId::ROOT);
        }
    }

    pub(crate) fn take_presentation_reconcile_request(&self) -> bool {
        self.presentation_reconcile_requested
            .swap(false, Ordering::AcqRel)
    }

    fn attach_repaint_context(&self, context: &egui::Context) {
        let _ = self.repaint_context.set(context.clone());
    }

    fn publish_designer_hwnd(&self, hwnd: isize) {
        let _gate = self
            .identity_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.designer_hwnd.load(Ordering::Acquire) != hwnd {
            self.designer_hwnd.store(hwnd, Ordering::Release);
            self.designer_generation.fetch_add(1, Ordering::AcqRel);
        }
    }

    fn designer_identity(&self) -> (usize, u64) {
        let _gate = self
            .identity_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (
            self.designer_hwnd.load(Ordering::Acquire).max(0) as usize,
            self.designer_generation.load(Ordering::Acquire),
        )
    }

    fn is_current_designer(&self, hwnd: usize, generation: u64) -> bool {
        let _gate = self
            .identity_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.designer_hwnd.load(Ordering::Acquire) == hwnd as isize
            && self.designer_generation.load(Ordering::Acquire) == generation
    }

    pub(crate) fn clear_designer_identity(&self) {
        self.publish_designer_hwnd(0);
    }

    /// Called from the focused Designer viewport callback. The callback's
    /// viewport identity supplies the semantic association; the cached native
    /// HWND avoids probing another thread's window text on the hotkey path.
    pub(crate) fn capture_focused_designer(&self) {
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::System::Threading::GetCurrentProcessId;
            use windows::Win32::UI::WindowsAndMessaging::{
                GetForegroundWindow, GetWindowThreadProcessId, IsWindow,
            };

            let foreground = unsafe { GetForegroundWindow() };
            let foreground_value = foreground.0 as usize;
            let (root_hwnd, _) = self.identity();
            if foreground.0.is_null()
                || foreground_value == root_hwnd
                || !unsafe { IsWindow(foreground) }.as_bool()
            {
                return;
            }
            let mut owner_process_id = 0;
            if unsafe { GetWindowThreadProcessId(foreground, Some(&mut owner_process_id)) } == 0
                || owner_process_id != unsafe { GetCurrentProcessId() }
            {
                return;
            }
            self.publish_designer_hwnd(foreground.0 as isize);
        }
    }

    /// Preserve foreground ownership only when the actual foreground HWND is
    /// the cached independent Designer viewport owned by this process. Native
    /// title queries are deliberately kept out of the hotkey path because a
    /// same-process WM_GETTEXT can wait on the GUI thread.
    pub fn focus_intent_for_launcher_toggle(&self) -> RootFocusIntent {
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::System::Threading::GetCurrentProcessId;
            use windows::Win32::UI::WindowsAndMessaging::{
                GetForegroundWindow, GetWindowThreadProcessId, IsWindow,
            };

            let (root_hwnd, root_generation) = self.identity();
            let (designer_hwnd, designer_generation) = self.designer_identity();
            if root_hwnd == 0
                || designer_hwnd == 0
                || designer_hwnd == root_hwnd
                || !self.is_current(root_hwnd, root_generation)
                || !self.is_current_designer(designer_hwnd, designer_generation)
            {
                return RootFocusIntent::ActivateRoot;
            }
            let foreground = unsafe { GetForegroundWindow() };
            if foreground.0 as usize != designer_hwnd || !unsafe { IsWindow(foreground) }.as_bool()
            {
                return RootFocusIntent::ActivateRoot;
            }
            let mut owner_process_id = 0;
            if unsafe { GetWindowThreadProcessId(foreground, Some(&mut owner_process_id)) } == 0
                || owner_process_id != unsafe { GetCurrentProcessId() }
            {
                return RootFocusIntent::ActivateRoot;
            }
            if should_preserve_registered_designer_foreground(
                root_hwnd,
                designer_hwnd,
                foreground.0 as usize,
                owner_process_id,
                unsafe { GetCurrentProcessId() },
            ) && self.is_current(root_hwnd, root_generation)
                && self.is_current_designer(designer_hwnd, designer_generation)
            {
                RootFocusIntent::PreserveForeground
            } else {
                RootFocusIntent::ActivateRoot
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            RootFocusIntent::ActivateRoot
        }
    }

    #[cfg(test)]
    pub(crate) fn set_identity_for_test(&self, hwnd: usize) {
        self.publish_hwnd(hwnd as isize);
    }

    #[cfg(test)]
    pub(crate) fn set_designer_identity_for_test(&self, hwnd: usize) {
        self.publish_designer_hwnd(hwnd as isize);
    }

    pub fn capture_frame(&self, frame: &eframe::Frame) {
        #[cfg(target_os = "windows")]
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};

            let Ok(handle) = frame.window_handle() else {
                return;
            };
            if let RawWindowHandle::Win32(handle) = handle.as_raw() {
                self.publish_hwnd(handle.hwnd.get());
            }
        }

        #[cfg(not(target_os = "windows"))]
        let _ = frame;
    }

    pub fn clear(&self) {
        self.clear_designer_identity();
        self.publish_hwnd(0);
    }

    fn show_without_activation(&self) {
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::System::Threading::GetCurrentProcessId;
            use windows::Win32::UI::WindowsAndMessaging::{
                GetWindowThreadProcessId, IsWindow, SW_SHOWNOACTIVATE, ShowWindowAsync,
            };

            let (raw, generation) = self.identity();
            if raw == 0 {
                return;
            }
            let hwnd = HWND(raw as *mut _);
            let mut owner_process_id = 0;
            if !self.is_current(raw, generation)
                || !unsafe { IsWindow(hwnd) }.as_bool()
                || unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner_process_id)) } == 0
                || owner_process_id != unsafe { GetCurrentProcessId() }
            {
                return;
            }
            // SW_SHOWNOACTIVATE is also the non-activating unminimize path.
            // Do not enqueue winit Visible(true), which can map to SW_SHOW after
            // its initial-show marker has been consumed.
            let _ = unsafe { ShowWindowAsync(hwnd, SW_SHOWNOACTIVATE) };
        }
    }

    fn wake_for_show(&self) {
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::System::Threading::GetCurrentProcessId;
            use windows::Win32::UI::WindowsAndMessaging::{
                GetWindowThreadProcessId, IsWindow, IsWindowVisible, SW_SHOWNOACTIVATE,
                ShowWindowAsync,
            };

            let raw = self.hwnd.load(Ordering::Acquire);
            if raw == 0 {
                return;
            }
            let hwnd = HWND(raw as *mut _);
            let mut owner_process_id = 0;
            let valid = unsafe { IsWindow(hwnd) }.as_bool()
                && unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner_process_id)) } != 0
                && owner_process_id == unsafe { GetCurrentProcessId() };
            if !valid {
                let _ = self
                    .hwnd
                    .compare_exchange(raw, 0, Ordering::AcqRel, Ordering::Acquire);
                return;
            }
            if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
                if !unsafe { ShowWindowAsync(hwnd, SW_SHOWNOACTIVATE) }.as_bool() {
                    let error = windows::core::Error::from_win32();
                    tracing::warn!(
                        %error,
                        hwnd = raw,
                        "failed to wake hidden root window before queued viewport commands"
                    );
                }
            }
        }
    }
}

fn should_preserve_registered_designer_foreground(
    root_hwnd: usize,
    registered_designer_hwnd: usize,
    foreground_hwnd: usize,
    foreground_process_id: u32,
    current_process_id: u32,
) -> bool {
    root_hwnd != 0
        && registered_designer_hwnd != 0
        && registered_designer_hwnd != root_hwnd
        && foreground_hwnd != 0
        && foreground_hwnd != root_hwnd
        && foreground_hwnd == registered_designer_hwnd
        && foreground_process_id == current_process_id
}

/// Controls whether making the launcher visible also reapplies its configured
/// placement. Restoring an already-visible launcher must preserve any geometry
/// changes made during the current visible session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VisiblePlacementPolicy {
    ApplyConfiguredPlacement,
    PreserveCurrentGeometry,
}

/// Controls whether restoring ROOT may take foreground ownership from an
/// independently focused application window such as the Radial Designer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum RootFocusIntent {
    #[default]
    ActivateRoot = 0,
    PreserveForeground = 1,
}

impl RootFocusIntent {
    fn from_atomic(value: u8) -> Self {
        match value {
            1 => Self::PreserveForeground,
            _ => Self::ActivateRoot,
        }
    }
}

/// Orders ROOT presentation requests against asynchronous native activation.
/// The desired visible bit remains owned by the existing visibility atomics;
/// this revision only identifies which request is current and serializes its
/// short native side effects with a newer request.
#[derive(Clone, Default)]
pub struct VisibilityRevision {
    revision: Arc<AtomicU64>,
    focus_intent: Arc<AtomicU8>,
    invocation_id: Arc<AtomicU64>,
    side_effect_gate: Arc<Mutex<()>>,
}

impl VisibilityRevision {
    pub fn current(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }

    /// Read visibility-owned values and their revision under the same gate so
    /// snapshots cannot tag stale flags with a newer request.
    pub fn inspect<T>(&self, read: impl FnOnce() -> T) -> (u64, T) {
        let _gate = self
            .side_effect_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (self.current(), read())
    }

    /// Apply a desired-state update before publishing its new revision.
    pub fn request<T>(&self, update: impl FnOnce() -> T) -> (u64, T) {
        self.request_with_focus_intent(RootFocusIntent::ActivateRoot, update)
    }

    /// Apply a desired-state update and its focus behavior before publishing
    /// their shared revision. A newer visibility request replaces both.
    pub fn request_with_focus_intent<T>(
        &self,
        focus_intent: RootFocusIntent,
        update: impl FnOnce() -> T,
    ) -> (u64, T) {
        self.request_with_focus_intent_and_invocation(focus_intent, None, update)
    }

    pub fn request_with_focus_intent_and_invocation<T>(
        &self,
        focus_intent: RootFocusIntent,
        invocation_id: Option<u64>,
        update: impl FnOnce() -> T,
    ) -> (u64, T) {
        let _gate = self
            .side_effect_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let result = update();
        self.focus_intent
            .store(focus_intent as u8, Ordering::Release);
        self.invocation_id
            .store(invocation_id.unwrap_or(0), Ordering::Release);
        let revision = self.revision.fetch_add(1, Ordering::AcqRel) + 1;
        (revision, result)
    }

    pub fn focus_intent(&self) -> RootFocusIntent {
        RootFocusIntent::from_atomic(self.focus_intent.load(Ordering::Acquire))
    }

    pub fn invocation_id(&self) -> Option<u64> {
        match self.invocation_id.load(Ordering::Acquire) {
            0 => None,
            invocation_id => Some(invocation_id),
        }
    }

    /// Commit a desired-state update only if no newer request arrived since
    /// the caller began an external operation. This keeps long native window
    /// calls outside the ordering gate without allowing their completion to
    /// overwrite a newer visibility choice.
    pub fn request_if_current<T>(
        &self,
        expected_revision: u64,
        update: impl FnOnce() -> T,
    ) -> Option<(u64, T)> {
        self.request_if_current_with_focus_intent(
            expected_revision,
            RootFocusIntent::ActivateRoot,
            update,
        )
    }

    pub fn request_if_current_with_focus_intent<T>(
        &self,
        expected_revision: u64,
        focus_intent: RootFocusIntent,
        update: impl FnOnce() -> T,
    ) -> Option<(u64, T)> {
        self.request_if_current_with_focus_intent_and_invocation(
            expected_revision,
            focus_intent,
            None,
            update,
        )
    }

    pub fn request_if_current_with_focus_intent_and_invocation<T>(
        &self,
        expected_revision: u64,
        focus_intent: RootFocusIntent,
        invocation_id: Option<u64>,
        update: impl FnOnce() -> T,
    ) -> Option<(u64, T)> {
        let _gate = self
            .side_effect_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.current() != expected_revision {
            return None;
        }
        let result = update();
        self.focus_intent
            .store(focus_intent as u8, Ordering::Release);
        self.invocation_id
            .store(invocation_id.unwrap_or(0), Ordering::Release);
        let revision = self.revision.fetch_add(1, Ordering::AcqRel) + 1;
        Some((revision, result))
    }

    /// Run a short side effect only while its presentation request remains
    /// authoritative. The gate is held for the call; potentially blocking
    /// platform work should instead validate, run outside this gate, and
    /// revalidate afterward.
    pub fn with_current<T>(
        &self,
        revision: u64,
        still_desired: impl FnOnce() -> bool,
        side_effect: impl FnOnce() -> T,
    ) -> Option<T> {
        let _gate = self
            .side_effect_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.current() != revision || !still_desired() {
            return None;
        }
        Some(side_effect())
    }
}

/// Ordered visibility toggles accumulated while one main-loop batch is being
/// routed.  `record_toggle` returns the state immediately before that toggle,
/// allowing other owners (such as radial keyboard scope) to follow each edge
/// even though viewport commands are applied later in the loop.
#[derive(Clone, Debug, Default)]
pub struct VisibilityToggleBatch {
    targets: Vec<(bool, Option<u64>, RootFocusIntent, Option<u64>)>,
}

impl VisibilityToggleBatch {
    pub fn record_toggle(&mut self, visibility: &AtomicBool) -> bool {
        self.record_toggle_ordered(&VisibilityRevision::default(), visibility)
            .0
    }

    pub fn record_toggle_ordered(
        &mut self,
        revision: &VisibilityRevision,
        visibility: &AtomicBool,
    ) -> (bool, u64) {
        self.record_toggle_ordered_with_invocation(revision, visibility, None)
    }

    pub fn record_toggle_ordered_with_invocation(
        &mut self,
        revision: &VisibilityRevision,
        visibility: &AtomicBool,
        invocation_id: Option<u64>,
    ) -> (bool, u64) {
        self.record_toggle_ordered_with_intent(
            revision,
            visibility,
            invocation_id,
            RootFocusIntent::ActivateRoot,
        )
    }

    pub fn record_toggle_ordered_with_intent(
        &mut self,
        revision: &VisibilityRevision,
        visibility: &AtomicBool,
        invocation_id: Option<u64>,
        focus_intent: RootFocusIntent,
    ) -> (bool, u64) {
        let (revision, was_visible) =
            revision.request_with_focus_intent_and_invocation(focus_intent, invocation_id, || {
                let was_visible = visibility.load(Ordering::SeqCst);
                visibility.store(!was_visible, Ordering::SeqCst);
                was_visible
            });
        let next_visible = !was_visible;
        self.targets
            .push((next_visible, Some(revision), focus_intent, invocation_id));
        acceptance_trace::emit(Event::DesiredVisibility {
            visible: next_visible,
            revision,
            source: VisibilitySource::ToggleBatch,
            invocation_id,
        });
        (was_visible, revision)
    }

    pub fn final_visible(&self) -> Option<bool> {
        self.targets.last().map(|(visible, _, _, _)| *visible)
    }
}

/// A root-bound command boundary for visibility work issued outside the root
/// viewport's own frame callback.  An `egui::Context` can be shared by the
/// root and deferred child viewports; its unqualified command methods target
/// whichever viewport is current at the call site.  Visibility ownership is
/// always the root launcher, so make that target explicit here.
#[derive(Clone)]
pub struct RootViewportCtx {
    ctx: egui::Context,
    window: RootWindowBridge,
}

impl RootViewportCtx {
    pub fn new(ctx: &egui::Context) -> Self {
        Self {
            ctx: ctx.clone(),
            window: RootWindowBridge::default(),
        }
    }

    pub fn with_window_bridge(ctx: &egui::Context, window: RootWindowBridge) -> Self {
        window.attach_repaint_context(ctx);
        Self {
            ctx: ctx.clone(),
            window,
        }
    }

    #[cfg(test)]
    fn viewport_id(&self) -> egui::ViewportId {
        egui::ViewportId::ROOT
    }
}

fn trace_root_command(cmd: &egui::ViewportCommand) -> Option<RootCommandKind> {
    Some(match cmd {
        egui::ViewportCommand::OuterPosition(position) => RootCommandKind::Position {
            x: position.x.round() as i32,
            y: position.y.round() as i32,
        },
        egui::ViewportCommand::InnerSize(size) => RootCommandKind::Size {
            width: size.x.round() as i32,
            height: size.y.round() as i32,
        },
        egui::ViewportCommand::Visible(true) => RootCommandKind::Show,
        egui::ViewportCommand::Visible(false) => RootCommandKind::ParkingBoundary,
        egui::ViewportCommand::Minimized(_) => RootCommandKind::Minimize,
        egui::ViewportCommand::Focus => RootCommandKind::Focus,
        _ => return None,
    })
}

/// A configured parking point can fall on another monitor. Keep the stored
/// preference unchanged, but move the live root outside the virtual desktop
/// whenever its configured rectangle would still be visible there.
fn parking_position_for_desktop(
    configured: (f32, f32),
    window_size: (f32, f32),
    pixels_per_point: f32,
    desktop: ScreenRect,
) -> (f32, f32) {
    if desktop.is_empty() || !pixels_per_point.is_finite() || pixels_per_point <= 0.0 {
        return configured;
    }
    let x = (configured.0 * pixels_per_point).round() as i64;
    let y = (configured.1 * pixels_per_point).round() as i64;
    let width = (window_size.0 * pixels_per_point).ceil().max(1.0) as u32;
    let height = (window_size.1 * pixels_per_point).ceil().max(1.0) as u32;
    let intersects = x < desktop.right()
        && x + i64::from(width) > i64::from(desktop.x)
        && y < desktop.bottom()
        && y + i64::from(height) > i64::from(desktop.y);
    if !intersects {
        return configured;
    }
    match compute_capture_safe_parking_position(desktop, width, height, CAPTURE_PARKING_MARGIN) {
        Ok((x, y)) => (x as f32 / pixels_per_point, y as f32 / pixels_per_point),
        Err(error) => {
            tracing::warn!(%error, "failed to choose offscreen launcher parking position");
            configured
        }
    }
}

fn safe_parking_position<C: ViewportCtx>(
    ctx: &C,
    configured: (f32, f32),
    window_size: (f32, f32),
) -> (f32, f32) {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
            SM_YVIRTUALSCREEN,
        };
        let (x, y, width, height) = unsafe {
            (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
            )
        };
        if width > 0 && height > 0 {
            return parking_position_for_desktop(
                configured,
                window_size,
                ctx.pixels_per_point(),
                ScreenRect::new(x, y, width as u32, height as u32),
            );
        }
    }
    let _ = (ctx, window_size);
    configured
}

impl ViewportCtx for RootViewportCtx {
    fn wake_for_show(&self) {
        self.window.wake_for_show();
    }

    fn show_without_activation(&self) {
        let correlation = if acceptance_trace::enabled() {
            acceptance_trace::root_command_correlation()
        } else {
            Correlation::default()
        };
        acceptance_trace::emit(Event::RootCommand {
            command: RootCommandKind::Show,
            correlation,
        });
        acceptance_trace::request_window_sample(correlation);
        self.window.show_without_activation();
    }

    fn pixels_per_point(&self) -> f32 {
        self.ctx.pixels_per_point()
    }

    fn send_viewport_cmd(&self, cmd: egui::ViewportCommand) {
        let Some(command) = trace_root_command(&cmd) else {
            return self.ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, cmd);
        };
        let correlation = if acceptance_trace::enabled() {
            acceptance_trace::root_command_correlation()
        } else {
            Correlation::default()
        };
        acceptance_trace::emit(Event::RootCommand {
            command,
            correlation,
        });
        acceptance_trace::request_window_sample(correlation);
        self.ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, cmd);
    }

    fn request_repaint(&self) {
        self.ctx.request_repaint_of(egui::ViewportId::ROOT);
    }
}

/// Apply every queued toggle in order. This deliberately applies each edge,
/// rather than reducing a batch to its final parity, so viewport side effects
/// and owners that follow those edges remain synchronized.
pub fn handle_visibility_toggle_batch<C: ViewportCtx>(
    batch: &VisibilityToggleBatch,
    restore_flag: &Arc<AtomicBool>,
    ctx_handle: &Arc<Mutex<Option<C>>>,
    queued_visibility: &mut Option<bool>,
    offscreen: (f32, f32),
    follow_mouse: bool,
    static_enabled: bool,
    static_pos: Option<(f32, f32)>,
    static_size: Option<(f32, f32)>,
    window_size: (f32, f32),
) -> bool {
    for &(next, _, focus_intent, _) in &batch.targets {
        apply_visibility_owner(
            next,
            focus_intent,
            restore_flag,
            ctx_handle,
            queued_visibility,
            offscreen,
            follow_mouse,
            static_enabled,
            static_pos,
            static_size,
            window_size,
        );
    }
    !batch.targets.is_empty()
}

pub fn handle_visibility_toggle_batch_ordered<C: ViewportCtx>(
    batch: &VisibilityToggleBatch,
    order: &VisibilityRevision,
    restore_flag: &Arc<AtomicBool>,
    ctx_handle: &Arc<Mutex<Option<C>>>,
    queued_visibility: &mut Option<bool>,
    offscreen: (f32, f32),
    follow_mouse: bool,
    static_enabled: bool,
    static_pos: Option<(f32, f32)>,
    static_size: Option<(f32, f32)>,
    window_size: (f32, f32),
) -> bool {
    for &(next, revision, focus_intent, invocation_id) in &batch.targets {
        let Some(revision) = revision else {
            continue;
        };
        let _ = order.with_current(
            revision,
            || true,
            || {
                acceptance_trace::with_visibility_trace_link(revision, invocation_id, || {
                    apply_visibility_owner(
                        next,
                        focus_intent,
                        restore_flag,
                        ctx_handle,
                        queued_visibility,
                        offscreen,
                        follow_mouse,
                        static_enabled,
                        static_pos,
                        static_size,
                        window_size,
                    )
                })
            },
        );
    }
    !batch.targets.is_empty()
}

/// Process a hotkey trigger and update the minimized state, issuing viewport
/// commands when possible. This mirrors the logic from `main.rs`.
pub fn handle_visibility_trigger<C: ViewportCtx>(
    trigger: &HotkeyTrigger,
    visibility: &Arc<AtomicBool>,
    restore_flag: &Arc<AtomicBool>,
    ctx_handle: &Arc<Mutex<Option<C>>>,
    queued_visibility: &mut Option<bool>,
    offscreen: (f32, f32),
    follow_mouse: bool,
    static_enabled: bool,
    static_pos: Option<(f32, f32)>,
    static_size: Option<(f32, f32)>,
    window_size: (f32, f32),
) -> bool {
    handle_visibility_trigger_with_owner(
        trigger,
        visibility,
        restore_flag,
        ctx_handle,
        queued_visibility,
        offscreen,
        follow_mouse,
        static_enabled,
        static_pos,
        static_size,
        window_size,
        |_| {},
    )
}

pub fn handle_visibility_trigger_with_owner<C: ViewportCtx>(
    trigger: &HotkeyTrigger,
    visibility: &Arc<AtomicBool>,
    restore_flag: &Arc<AtomicBool>,
    ctx_handle: &Arc<Mutex<Option<C>>>,
    queued_visibility: &mut Option<bool>,
    offscreen: (f32, f32),
    follow_mouse: bool,
    static_enabled: bool,
    static_pos: Option<(f32, f32)>,
    static_size: Option<(f32, f32)>,
    window_size: (f32, f32),
    on_grid_toggle: impl FnMut(bool),
) -> bool {
    handle_visibility_trigger_with_owner_ordered(
        trigger,
        visibility,
        restore_flag,
        ctx_handle,
        queued_visibility,
        offscreen,
        follow_mouse,
        static_enabled,
        static_pos,
        static_size,
        window_size,
        &VisibilityRevision::default(),
        on_grid_toggle,
    )
}

pub fn handle_visibility_trigger_with_owner_ordered<C: ViewportCtx>(
    trigger: &HotkeyTrigger,
    visibility: &Arc<AtomicBool>,
    restore_flag: &Arc<AtomicBool>,
    ctx_handle: &Arc<Mutex<Option<C>>>,
    queued_visibility: &mut Option<bool>,
    offscreen: (f32, f32),
    follow_mouse: bool,
    static_enabled: bool,
    static_pos: Option<(f32, f32)>,
    static_size: Option<(f32, f32)>,
    window_size: (f32, f32),
    order: &VisibilityRevision,
    mut on_grid_toggle: impl FnMut(bool),
) -> bool {
    let mut changed = false;
    if trigger.take() {
        let (revision, old) = order.request_with_focus_intent_and_invocation(
            RootFocusIntent::ActivateRoot,
            None,
            || {
                let old = visibility.load(Ordering::SeqCst);
                visibility.store(!old, Ordering::SeqCst);
                old
            },
        );
        let next = !old;
        acceptance_trace::emit(Event::DesiredVisibility {
            visible: next,
            revision,
            source: VisibilitySource::LegacyTrigger,
            invocation_id: None,
        });
        on_grid_toggle(old);
        changed = old != next;
        let _ = order.with_current(
            revision,
            || visibility.load(Ordering::SeqCst) == next,
            || {
                acceptance_trace::with_visibility_trace_link(revision, None, || {
                    apply_visibility_owner(
                        next,
                        RootFocusIntent::ActivateRoot,
                        restore_flag,
                        ctx_handle,
                        queued_visibility,
                        offscreen,
                        follow_mouse,
                        static_enabled,
                        static_pos,
                        static_size,
                        window_size,
                    )
                })
            },
        );
    } else if let Some(next) = *queued_visibility {
        tracing::debug!("Processing previously queued visibility: {}", next);
        let applied = with_current_queued_visibility(
            order,
            visibility,
            next,
            |revision, focus_intent, invocation_id| {
                acceptance_trace::with_visibility_trace_link(revision, invocation_id, || {
                    if let Ok(guard) = ctx_handle.lock()
                        && let Some(c) = &*guard
                    {
                        acceptance_trace::emit(Event::DesiredVisibility {
                            visible: next,
                            revision,
                            source: VisibilitySource::Queued,
                            invocation_id,
                        });
                        let old = visibility.load(Ordering::SeqCst);
                        visibility.store(next, Ordering::SeqCst);
                        apply_visibility_with_focus_intent(
                            next,
                            focus_intent,
                            VisiblePlacementPolicy::ApplyConfiguredPlacement,
                            c,
                            offscreen,
                            follow_mouse,
                            static_enabled,
                            static_pos,
                            static_size,
                            window_size,
                        );
                        // The root remains drawable while parked. Radial preparation and
                        // action handoff are processed by its frame even when the grid is
                        // logically hidden; hiding the HWND stalls that work until a tap
                        // shows the grid again.
                        restore_flag.store(next, Ordering::SeqCst);
                        *queued_visibility = None;
                        tracing::debug!("Applied queued visibility: {}", next);
                        Some(old != next)
                    } else {
                        None
                    }
                })
            },
        );
        match applied {
            Some(Some(changed_now)) => changed = changed_now,
            Some(None) => {}
            None => *queued_visibility = None,
        }
    }
    changed
}

fn with_current_queued_visibility<T>(
    order: &VisibilityRevision,
    visibility: &AtomicBool,
    next: bool,
    apply: impl FnOnce(u64, RootFocusIntent, Option<u64>) -> T,
) -> Option<T> {
    let (revision, (focus_intent, invocation_id)) =
        order.inspect(|| (order.focus_intent(), order.invocation_id()));
    order.with_current(
        revision,
        || visibility.load(Ordering::SeqCst) == next,
        || apply(revision, focus_intent, invocation_id),
    )
}

fn apply_visibility_owner<C: ViewportCtx>(
    next: bool,
    focus_intent: RootFocusIntent,
    restore_flag: &Arc<AtomicBool>,
    ctx_handle: &Arc<Mutex<Option<C>>>,
    queued_visibility: &mut Option<bool>,
    offscreen: (f32, f32),
    follow_mouse: bool,
    static_enabled: bool,
    static_pos: Option<(f32, f32)>,
    static_size: Option<(f32, f32)>,
    window_size: (f32, f32),
) {
    if let Ok(guard) = ctx_handle.lock() {
        if let Some(ctx) = &*guard {
            apply_visibility_with_focus_intent(
                next,
                focus_intent,
                VisiblePlacementPolicy::ApplyConfiguredPlacement,
                ctx,
                offscreen,
                follow_mouse,
                static_enabled,
                static_pos,
                static_size,
                window_size,
            );
            restore_flag.store(next, Ordering::SeqCst);
            *queued_visibility = None;
            tracing::debug!("Applied queued visibility: {}", next);
        } else {
            *queued_visibility = Some(next);
            restore_flag.store(next, Ordering::SeqCst);
        }
    } else {
        *queued_visibility = Some(next);
        restore_flag.store(next, Ordering::SeqCst);
    }
}

/// Apply the current visibility state to the viewport.
pub fn apply_visibility<C: ViewportCtx>(
    visible: bool,
    placement_policy: VisiblePlacementPolicy,
    ctx: &C,
    offscreen: (f32, f32),
    follow_mouse: bool,
    static_enabled: bool,
    static_pos: Option<(f32, f32)>,
    static_size: Option<(f32, f32)>,
    window_size: (f32, f32),
) {
    apply_visibility_with_focus_intent(
        visible,
        RootFocusIntent::ActivateRoot,
        placement_policy,
        ctx,
        offscreen,
        follow_mouse,
        static_enabled,
        static_pos,
        static_size,
        window_size,
    );
}

pub fn apply_visibility_with_focus_intent<C: ViewportCtx>(
    visible: bool,
    focus_intent: RootFocusIntent,
    placement_policy: VisiblePlacementPolicy,
    ctx: &C,
    offscreen: (f32, f32),
    follow_mouse: bool,
    static_enabled: bool,
    static_pos: Option<(f32, f32)>,
    static_size: Option<(f32, f32)>,
    window_size: (f32, f32),
) {
    if visible {
        ctx.wake_for_show();
        if placement_policy == VisiblePlacementPolicy::ApplyConfiguredPlacement {
            if static_enabled {
                if let Some((x, y)) = static_pos {
                    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
                }
                if let Some((w, h)) = static_size {
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
                }
            } else if follow_mouse
                && let Some((x, y)) = crate::window_manager::current_mouse_position()
            {
                let pos_x = x - window_size.0 / 2.0;
                let pos_y = y - window_size.1 / 2.0;
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                    pos_x, pos_y,
                )));
            }
        }
        if focus_intent == RootFocusIntent::ActivateRoot {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        } else {
            ctx.show_without_activation();
        }
    } else {
        acceptance_trace::emit(Event::RootCommand {
            command: RootCommandKind::ParkingBoundary,
            correlation: if acceptance_trace::enabled() {
                acceptance_trace::root_command_correlation()
            } else {
                Correlation::default()
            },
        });
        let parked = safe_parking_position(ctx, offscreen, window_size);
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
            parked.0, parked.1,
        )));
    }
    ctx.request_repaint();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkey::parse_hotkey;
    use std::sync::atomic::AtomicUsize;

    #[derive(Clone, Default)]
    struct RecordingViewport {
        commands: Arc<Mutex<Vec<egui::ViewportCommand>>>,
        trace_correlations: Arc<Mutex<Vec<Correlation>>>,
        repaint_count: Arc<AtomicUsize>,
        wake_count: Arc<AtomicUsize>,
        order: Arc<Mutex<Vec<&'static str>>>,
    }

    impl ViewportCtx for RecordingViewport {
        fn wake_for_show(&self) {
            self.wake_count.fetch_add(1, Ordering::SeqCst);
            self.order.lock().unwrap().push("wake");
        }

        fn send_viewport_cmd(&self, cmd: egui::ViewportCommand) {
            self.order.lock().unwrap().push("command");
            self.trace_correlations
                .lock()
                .unwrap()
                .push(acceptance_trace::root_command_correlation());
            self.commands.lock().unwrap().push(cmd);
        }

        fn request_repaint(&self) {
            self.order.lock().unwrap().push("repaint");
            self.repaint_count.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn stale_activation_reconciliation_reapplies_latest_hidden_geometry() {
        let revision = VisibilityRevision::default();
        let visible = AtomicBool::new(false);
        let bridge = RootWindowBridge::default();
        revision.request_with_focus_intent(RootFocusIntent::ActivateRoot, || ());
        bridge.request_presentation_reconcile();
        let viewport = RecordingViewport::default();
        let (request_revision, (should_be_visible, focus_intent, reconcile)) =
            revision.inspect(|| {
                (
                    visible.load(Ordering::Acquire),
                    revision.focus_intent(),
                    bridge.take_presentation_reconcile_request(),
                )
            });

        assert!(reconcile);
        assert!(!should_be_visible);
        assert_eq!(request_revision, revision.current());
        apply_visibility_with_focus_intent(
            should_be_visible,
            focus_intent,
            VisiblePlacementPolicy::PreserveCurrentGeometry,
            &viewport,
            (-100_000.0, -100_000.0),
            false,
            false,
            None,
            None,
            (900.0, 600.0),
        );

        assert!(matches!(
            viewport.commands.lock().unwrap().as_slice(),
            [egui::ViewportCommand::OuterPosition(position)]
                if position.x == -100_000.0 && position.y == -100_000.0
        ));
        assert_eq!(viewport.wake_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn stale_activation_reconciliation_preserves_the_newest_foreground_intent() {
        let revision = VisibilityRevision::default();
        let visible = AtomicBool::new(true);
        let bridge = RootWindowBridge::default();
        revision.request_with_focus_intent(RootFocusIntent::PreserveForeground, || ());
        bridge.request_presentation_reconcile();
        let viewport = RecordingViewport::default();
        let (request_revision, (should_be_visible, focus_intent, reconcile)) =
            revision.inspect(|| {
                (
                    visible.load(Ordering::Acquire),
                    revision.focus_intent(),
                    bridge.take_presentation_reconcile_request(),
                )
            });

        assert!(reconcile);
        assert!(should_be_visible);
        assert_eq!(request_revision, revision.current());
        apply_visibility_with_focus_intent(
            should_be_visible,
            focus_intent,
            VisiblePlacementPolicy::PreserveCurrentGeometry,
            &viewport,
            (0.0, 0.0),
            false,
            false,
            None,
            None,
            (900.0, 600.0),
        );

        assert!(matches!(
            viewport.commands.lock().unwrap().as_slice(),
            [egui::ViewportCommand::Visible(true)]
        ));
        assert!(
            !viewport
                .commands
                .lock()
                .unwrap()
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::Focus))
        );
    }

    fn trigger() -> HotkeyTrigger {
        HotkeyTrigger::new(parse_hotkey("End").expect("test hotkey parses"))
    }

    fn toggle(trigger: &HotkeyTrigger) {
        *trigger.open.lock().unwrap() = true;
    }

    #[test]
    fn newer_visibility_request_rejects_stale_native_restore() {
        let revision = VisibilityRevision::default();
        let visible = Arc::new(AtomicBool::new(true));
        let old_revision = revision.request(|| ()).0;
        let (new_revision, ()) = revision.request(|| {
            visible.store(false, Ordering::SeqCst);
        });
        let side_effects = AtomicUsize::new(0);

        assert!(
            revision
                .with_current(
                    old_revision,
                    || visible.load(Ordering::SeqCst),
                    || side_effects.fetch_add(1, Ordering::SeqCst),
                )
                .is_none()
        );
        assert_eq!(side_effects.load(Ordering::SeqCst), 0);
        assert_eq!(revision.current(), new_revision);
    }

    #[test]
    fn visibility_request_and_native_side_effect_share_one_ordering_gate() {
        let revision = VisibilityRevision::default();
        let side_effect_revision = revision.request(|| ()).0;
        let effect_started = Arc::new(std::sync::Barrier::new(2));
        let update_done = Arc::new(AtomicBool::new(false));
        let effect_started_on_thread = Arc::clone(&effect_started);
        let update_done_on_thread = Arc::clone(&update_done);
        let revision_for_thread = revision.clone();
        let update = std::thread::spawn(move || {
            effect_started_on_thread.wait();
            revision_for_thread.request(|| {
                update_done_on_thread.store(true, Ordering::SeqCst);
            });
        });
        let observed = revision.with_current(
            side_effect_revision,
            || true,
            || {
                effect_started.wait();
                assert!(!update_done.load(Ordering::SeqCst));
            },
        );
        update.join().unwrap();
        assert_eq!(observed, Some(()));
        assert!(update_done.load(Ordering::SeqCst));
    }

    #[test]
    fn visibility_snapshot_reads_flags_restore_and_focus_intent_under_one_gate() {
        let revision = VisibilityRevision::default();
        let initial_revision = revision
            .request_with_focus_intent(RootFocusIntent::PreserveForeground, || ())
            .0;
        let visible = Arc::new(AtomicBool::new(true));
        let restore = Arc::new(AtomicBool::new(true));
        let (inside_tx, inside_rx) = std::sync::mpsc::channel();
        let (resume_tx, resume_rx) = std::sync::mpsc::channel();
        let snapshot_revision = revision.clone();
        let snapshot_visible = Arc::clone(&visible);
        let snapshot_restore = Arc::clone(&restore);
        let snapshot = std::thread::spawn(move || {
            snapshot_revision.inspect(|| {
                inside_tx.send(()).unwrap();
                resume_rx.recv().unwrap();
                (
                    snapshot_visible.load(Ordering::SeqCst),
                    snapshot_restore.load(Ordering::SeqCst),
                    snapshot_revision.focus_intent(),
                )
            })
        });
        inside_rx.recv().unwrap();

        let request_revision = revision.clone();
        let request_visible = Arc::clone(&visible);
        let request_restore = Arc::clone(&restore);
        let (request_started_tx, request_started_rx) = std::sync::mpsc::channel();
        let request = std::thread::spawn(move || {
            request_started_tx.send(()).unwrap();
            request_revision.request(|| {
                request_visible.store(false, Ordering::SeqCst);
                request_restore.store(false, Ordering::SeqCst);
            })
        });
        request_started_rx.recv().unwrap();
        resume_tx.send(()).unwrap();

        let (captured_revision, captured_flags) = snapshot.join().unwrap();
        let (new_revision, ()) = request.join().unwrap();
        assert_eq!(captured_revision, initial_revision);
        assert_eq!(
            captured_flags,
            (true, true, RootFocusIntent::PreserveForeground)
        );
        assert_eq!(new_revision, initial_revision + 1);
        assert_eq!(revision.current(), new_revision);
        assert_eq!(
            (
                visible.load(Ordering::SeqCst),
                restore.load(Ordering::SeqCst)
            ),
            (false, false)
        );
    }

    #[test]
    fn conditional_visibility_commit_cannot_overwrite_a_newer_request() {
        let revision = VisibilityRevision::default();
        let visible = AtomicBool::new(false);
        let starting_revision = revision.current();
        revision.request(|| visible.store(false, Ordering::SeqCst));

        let committed = revision.request_if_current(starting_revision, || {
            visible.store(true, Ordering::SeqCst);
        });
        assert!(committed.is_none());
        assert!(!visible.load(Ordering::SeqCst));
    }

    #[test]
    fn ordered_toggle_batch_queues_only_the_newest_viewport_effect() {
        let revision = VisibilityRevision::default();
        let visibility = AtomicBool::new(false);
        let mut batch = VisibilityToggleBatch::default();
        assert_eq!(batch.record_toggle_ordered(&revision, &visibility).0, false);
        assert_eq!(batch.record_toggle_ordered(&revision, &visibility).0, true);
        let viewport = RecordingViewport::default();
        let ctx = Arc::new(Mutex::new(Some(viewport.clone())));
        let restore = Arc::new(AtomicBool::new(false));
        let mut queued = None;

        handle_visibility_toggle_batch_ordered(
            &batch,
            &revision,
            &restore,
            &ctx,
            &mut queued,
            (-10_000.0, -10_000.0),
            false,
            false,
            None,
            None,
            (400.0, 220.0),
        );

        assert_eq!(revision.current(), 2);
        assert!(!visibility.load(Ordering::SeqCst));
        assert!(!restore.load(Ordering::SeqCst));
        assert_eq!(viewport.repaint_count.load(Ordering::SeqCst), 1);
        assert_eq!(viewport.wake_count.load(Ordering::SeqCst), 0);
        assert!(matches!(
            viewport.commands.lock().unwrap().as_slice(),
            [egui::ViewportCommand::OuterPosition(position)]
                if *position == egui::pos2(-10_000.0, -10_000.0)
        ));
    }

    #[test]
    fn ordered_toggle_batch_links_invocation_to_root_command_trace() {
        let order = VisibilityRevision::default();
        let visibility = AtomicBool::new(false);
        let mut batch = VisibilityToggleBatch::default();
        let (_, revision) =
            batch.record_toggle_ordered_with_invocation(&order, &visibility, Some(41));
        let viewport = RecordingViewport::default();
        let ctx = Arc::new(Mutex::new(Some(viewport.clone())));
        let restore = Arc::new(AtomicBool::new(false));
        let mut queued = None;

        handle_visibility_toggle_batch_ordered(
            &batch,
            &order,
            &restore,
            &ctx,
            &mut queued,
            (-10_000.0, -10_000.0),
            false,
            false,
            None,
            None,
            (400.0, 220.0),
        );

        let correlations = viewport.trace_correlations.lock().unwrap();
        assert!(!correlations.is_empty());
        assert!(correlations.iter().all(|correlation| {
            correlation.visibility_revision == revision && correlation.invocation_id == 41
        }));
    }

    #[test]
    fn preserve_foreground_show_queues_visible_without_focus_or_unminimize() {
        let revision = VisibilityRevision::default();
        let visibility = AtomicBool::new(false);
        let mut batch = VisibilityToggleBatch::default();
        batch.record_toggle_ordered_with_intent(
            &revision,
            &visibility,
            Some(9),
            RootFocusIntent::PreserveForeground,
        );
        let viewport = RecordingViewport::default();
        let ctx = Arc::new(Mutex::new(Some(viewport.clone())));
        let restore = Arc::new(AtomicBool::new(false));
        let mut queued = None;
        handle_visibility_toggle_batch_ordered(
            &batch,
            &revision,
            &restore,
            &ctx,
            &mut queued,
            (-10_000.0, -10_000.0),
            false,
            false,
            None,
            None,
            (400.0, 220.0),
        );
        let commands = viewport.commands.lock().unwrap();
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::Visible(true)))
        );
        assert!(!commands.iter().any(|command| matches!(
            command,
            egui::ViewportCommand::Focus | egui::ViewportCommand::Minimized(false)
        )));
        assert_eq!(revision.focus_intent(), RootFocusIntent::PreserveForeground);
    }

    #[test]
    fn queued_visibility_retains_focus_intent_until_context_is_available() {
        let revision = VisibilityRevision::default();
        let visibility = Arc::new(AtomicBool::new(false));
        let mut batch = VisibilityToggleBatch::default();
        batch.record_toggle_ordered_with_intent(
            &revision,
            &visibility,
            Some(7),
            RootFocusIntent::PreserveForeground,
        );
        let restore = Arc::new(AtomicBool::new(false));
        let ctx = Arc::new(Mutex::new(None::<RecordingViewport>));
        let mut queued = None;
        handle_visibility_toggle_batch_ordered(
            &batch,
            &revision,
            &restore,
            &ctx,
            &mut queued,
            (-10_000.0, -10_000.0),
            false,
            false,
            None,
            None,
            (400.0, 220.0),
        );
        assert_eq!(queued, Some(true));
        let viewport = RecordingViewport::default();
        *ctx.lock().unwrap() = Some(viewport.clone());
        let dormant_trigger = trigger();
        handle_visibility_trigger_with_owner_ordered(
            &dormant_trigger,
            &visibility,
            &restore,
            &ctx,
            &mut queued,
            (-10_000.0, -10_000.0),
            false,
            false,
            None,
            None,
            (400.0, 220.0),
            &revision,
            |_| {},
        );
        assert!(queued.is_none());
        assert_eq!(revision.focus_intent(), RootFocusIntent::PreserveForeground);
        assert!(
            !viewport
                .commands
                .lock()
                .unwrap()
                .iter()
                .any(|command| matches!(
                    command,
                    egui::ViewportCommand::Focus | egui::ViewportCommand::Minimized(false)
                ))
        );
    }

    #[test]
    fn superseded_queued_visibility_does_not_emit_a_desired_visibility_edge() {
        let revision = VisibilityRevision::default();
        let visibility = AtomicBool::new(false);
        revision.request_with_focus_intent_and_invocation(
            RootFocusIntent::PreserveForeground,
            Some(41),
            || visibility.store(true, Ordering::SeqCst),
        );
        let (hide_revision, ()) = revision.request_with_focus_intent_and_invocation(
            RootFocusIntent::ActivateRoot,
            Some(42),
            || visibility.store(false, Ordering::SeqCst),
        );
        let emitted = Mutex::new(Vec::new());

        assert!(
            with_current_queued_visibility(
                &revision,
                &visibility,
                true,
                |rev, intent, invocation| {
                    emitted.lock().unwrap().push((rev, intent, invocation));
                }
            )
            .is_none()
        );
        assert!(emitted.lock().unwrap().is_empty());

        let current = with_current_queued_visibility(
            &revision,
            &visibility,
            false,
            |rev, intent, invocation| (rev, intent, invocation),
        );
        assert_eq!(
            current,
            Some((hide_revision, RootFocusIntent::ActivateRoot, Some(42)))
        );
    }

    #[test]
    fn newer_activate_request_supersedes_preserve_foreground_intent() {
        let revision = VisibilityRevision::default();
        let old = revision
            .request_with_focus_intent(RootFocusIntent::PreserveForeground, || ())
            .0;
        let current = revision.request(|| ()).0;
        let (snapshot_revision, intent) = revision.inspect(|| revision.focus_intent());
        assert!(current > old);
        assert_eq!(snapshot_revision, current);
        assert_eq!(intent, RootFocusIntent::ActivateRoot);
    }

    #[test]
    fn only_the_registered_radial_designer_foreground_preserves_focus() {
        assert!(should_preserve_registered_designer_foreground(
            10, 20, 20, 42, 42
        ));
        assert!(!should_preserve_registered_designer_foreground(
            10, 10, 20, 42, 42
        ));
        assert!(!should_preserve_registered_designer_foreground(
            10, 20, 20, 43, 42
        ));
        assert!(!should_preserve_registered_designer_foreground(
            10, 20, 20, 42, 43
        ));
    }

    #[test]
    fn replacing_registered_designer_invalidates_the_previous_lifetime() {
        let bridge = RootWindowBridge::default();
        bridge.set_identity_for_test(10);
        bridge.set_designer_identity_for_test(20);
        let (old_hwnd, old_generation) = bridge.designer_identity();
        bridge.clear_designer_identity();
        bridge.set_designer_identity_for_test(20);

        assert_eq!(old_hwnd, 20);
        assert!(!bridge.is_current_designer(old_hwnd, old_generation));
    }

    fn handle(
        trigger: &HotkeyTrigger,
        visibility: &Arc<AtomicBool>,
        restore_flag: &Arc<AtomicBool>,
        ctx: &Arc<Mutex<Option<RecordingViewport>>>,
        queued_visibility: &mut Option<bool>,
    ) {
        handle_visibility_trigger(
            trigger,
            visibility,
            restore_flag,
            ctx,
            queued_visibility,
            (-10_000.0, -10_000.0),
            false,
            false,
            None,
            None,
            (400.0, 220.0),
        );
    }

    #[test]
    fn root_visibility_boundary_targets_root_from_child_context() {
        let ctx = egui::Context::default();
        let child_id = egui::ViewportId::from_hash_of("designer-test");
        ctx.set_embed_viewports(false);
        ctx.show_viewport_deferred(
            child_id,
            egui::ViewportBuilder::default(),
            |_child, _class| {},
        );
        let mut input = egui::RawInput::default();
        input.viewport_id = child_id;
        input.viewports.insert(
            child_id,
            egui::ViewportInfo {
                parent: Some(egui::ViewportId::ROOT),
                ..Default::default()
            },
        );
        let _ = ctx.run(input, |child| {
            assert_eq!(child.viewport_id(), child_id);
            let root = RootViewportCtx::new(child);
            assert_eq!(root.viewport_id(), egui::ViewportId::ROOT);
            root.request_repaint();
            assert!(child.has_requested_repaint_for(&egui::ViewportId::ROOT));
            root.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        });
    }

    #[test]
    fn root_command_trace_retains_requested_coordinates() {
        assert_eq!(
            trace_root_command(&egui::ViewportCommand::OuterPosition(
                egui::pos2(1.6, -2.4,)
            )),
            Some(RootCommandKind::Position { x: 2, y: -2 })
        );
        assert_eq!(
            trace_root_command(&egui::ViewportCommand::InnerSize(egui::vec2(640.4, 479.6,))),
            Some(RootCommandKind::Size {
                width: 640,
                height: 480,
            })
        );
    }

    #[test]
    fn show_wakes_native_root_before_queuing_viewport_commands() {
        let viewport = RecordingViewport::default();

        apply_visibility(
            true,
            VisiblePlacementPolicy::ApplyConfiguredPlacement,
            &viewport,
            (-10_000.0, -10_000.0),
            false,
            false,
            None,
            None,
            (400.0, 220.0),
        );

        assert_eq!(viewport.wake_count.load(Ordering::SeqCst), 1);
        assert_eq!(viewport.order.lock().unwrap().first(), Some(&"wake"));
        assert!(matches!(
            viewport.commands.lock().unwrap().as_slice(),
            [
                egui::ViewportCommand::Visible(true),
                egui::ViewportCommand::Minimized(false),
                egui::ViewportCommand::Focus
            ]
        ));
    }

    #[test]
    fn hide_does_not_wake_native_root() {
        let viewport = RecordingViewport::default();

        apply_visibility(
            false,
            VisiblePlacementPolicy::ApplyConfiguredPlacement,
            &viewport,
            (-10_000.0, -10_000.0),
            false,
            false,
            None,
            None,
            (400.0, 220.0),
        );

        assert_eq!(viewport.wake_count.load(Ordering::SeqCst), 0);
        assert_eq!(viewport.order.lock().unwrap().first(), Some(&"command"));
    }

    #[test]
    fn parking_moves_a_configured_point_outside_the_virtual_desktop() {
        let desktop = ScreenRect::new(-1920, 0, 7680, 3240);
        assert_eq!(
            parking_position_for_desktop((3000.0, 3000.0), (1779.0, 1070.0), 1.0, desktop),
            (5856.0, 0.0)
        );
        assert_eq!(
            parking_position_for_desktop((1500.0, 1500.0), (1779.0, 1070.0), 2.0, desktop),
            (2928.0, 0.0)
        );
        assert_eq!(
            parking_position_for_desktop(
                (3000.0, 3000.0),
                (1779.0, 1070.0),
                1.0,
                ScreenRect::new(0, 0, 1920, 1080)
            ),
            (3000.0, 3000.0)
        );
    }

    #[test]
    fn hidden_grid_keeps_the_root_drawable_for_radial_preparation() {
        let trigger = trigger();
        let visibility = Arc::new(AtomicBool::new(true));
        let restore_flag = Arc::new(AtomicBool::new(true));
        let viewport = RecordingViewport::default();
        let ctx = Arc::new(Mutex::new(Some(viewport.clone())));
        let mut queued_visibility = None;

        toggle(&trigger);
        handle(
            &trigger,
            &visibility,
            &restore_flag,
            &ctx,
            &mut queued_visibility,
        );
        assert!(!visibility.load(Ordering::SeqCst));
        let commands = viewport.commands.lock().unwrap();
        assert_eq!(commands.len(), 1);
        assert!(matches!(
            commands.first(),
            Some(egui::ViewportCommand::OuterPosition(position))
                if *position == egui::pos2(-10_000.0, -10_000.0)
        ));
    }

    #[test]
    fn hide_before_the_next_frame_invalidates_show_restore() {
        let trigger = trigger();
        let visibility = Arc::new(AtomicBool::new(false));
        let restore_flag = Arc::new(AtomicBool::new(false));
        let viewport = RecordingViewport::default();
        let ctx = Arc::new(Mutex::new(Some(viewport)));
        let mut queued_visibility = None;

        toggle(&trigger);
        handle(
            &trigger,
            &visibility,
            &restore_flag,
            &ctx,
            &mut queued_visibility,
        );
        assert!(visibility.load(Ordering::SeqCst));
        assert!(restore_flag.load(Ordering::SeqCst));

        toggle(&trigger);
        handle(
            &trigger,
            &visibility,
            &restore_flag,
            &ctx,
            &mut queued_visibility,
        );
        assert!(!visibility.load(Ordering::SeqCst));
        assert!(!restore_flag.load(Ordering::SeqCst));
        assert!(queued_visibility.is_none());
    }

    #[test]
    fn queued_hide_replaces_show_and_does_not_restore_when_context_attaches() {
        let trigger = trigger();
        let visibility = Arc::new(AtomicBool::new(false));
        let restore_flag = Arc::new(AtomicBool::new(false));
        let ctx = Arc::new(Mutex::new(None));
        let mut queued_visibility = None;

        toggle(&trigger);
        handle(
            &trigger,
            &visibility,
            &restore_flag,
            &ctx,
            &mut queued_visibility,
        );
        assert_eq!(queued_visibility, Some(true));
        assert!(restore_flag.load(Ordering::SeqCst));

        toggle(&trigger);
        handle(
            &trigger,
            &visibility,
            &restore_flag,
            &ctx,
            &mut queued_visibility,
        );
        assert_eq!(queued_visibility, Some(false));
        assert!(!restore_flag.load(Ordering::SeqCst));

        *ctx.lock().unwrap() = Some(RecordingViewport::default());
        handle(
            &trigger,
            &visibility,
            &restore_flag,
            &ctx,
            &mut queued_visibility,
        );
        assert!(!visibility.load(Ordering::SeqCst));
        assert!(!restore_flag.load(Ordering::SeqCst));
        assert!(queued_visibility.is_none());
    }

    #[test]
    fn two_queued_grid_toggles_preserve_order_and_restore_keyboard_ownership() {
        let trigger = trigger();
        let visibility = Arc::new(AtomicBool::new(false));
        let restore_flag = Arc::new(AtomicBool::new(false));
        let viewport = RecordingViewport::default();
        let ctx = Arc::new(Mutex::new(Some(viewport.clone())));
        let mut queued_visibility = None;
        let mut batch = VisibilityToggleBatch::default();
        let mut keyboard_suspended = false;
        let mut transitions = Vec::new();

        for _ in 0..2 {
            let was_visible = batch.record_toggle(&visibility);
            transitions.push(was_visible);
            keyboard_suspended = !was_visible;
        }

        assert_eq!(transitions, [false, true]);
        assert_eq!(batch.final_visible(), Some(false));
        assert!(!visibility.load(Ordering::SeqCst));
        assert!(handle_visibility_toggle_batch(
            &batch,
            &restore_flag,
            &ctx,
            &mut queued_visibility,
            (-10_000.0, -10_000.0),
            false,
            false,
            None,
            None,
            (400.0, 220.0),
        ));

        assert!(!visibility.load(Ordering::SeqCst));
        assert!(!restore_flag.load(Ordering::SeqCst));
        assert!(!keyboard_suspended);
        assert!(queued_visibility.is_none());
        assert_eq!(viewport.repaint_count.load(Ordering::SeqCst), 2);
        assert_eq!(viewport.wake_count.load(Ordering::SeqCst), 1);
        assert_eq!(viewport.order.lock().unwrap().first(), Some(&"wake"));
        let commands = viewport.commands.lock().unwrap();
        assert_eq!(commands.len(), 4);
        assert!(matches!(
            commands.last(),
            Some(egui::ViewportCommand::OuterPosition(position))
                if *position == egui::pos2(-10_000.0, -10_000.0)
        ));
        assert!(!trigger.take());
    }

    #[test]
    fn legacy_hotkey_visibility_edges_report_grid_keyboard_transfer() {
        let trigger = trigger();
        let visibility = Arc::new(AtomicBool::new(false));
        let restore_flag = Arc::new(AtomicBool::new(false));
        let ctx = Arc::new(Mutex::new(Some(RecordingViewport::default())));
        let mut queued_visibility = None;
        let mut keyboard_suspended = false;

        for (index, expected_was_visible) in [false, true].into_iter().enumerate() {
            toggle(&trigger);
            let mut reported_was_visible = None;
            assert!(handle_visibility_trigger_with_owner(
                &trigger,
                &visibility,
                &restore_flag,
                &ctx,
                &mut queued_visibility,
                (-10_000.0, -10_000.0),
                false,
                false,
                None,
                None,
                (400.0, 220.0),
                |was_visible| reported_was_visible = Some(was_visible),
            ));
            assert_eq!(reported_was_visible, Some(expected_was_visible));
            keyboard_suspended = !reported_was_visible.unwrap();
            assert_eq!(keyboard_suspended, index == 0);
        }

        assert!(!visibility.load(Ordering::SeqCst));
        assert!(!keyboard_suspended);
    }
}
