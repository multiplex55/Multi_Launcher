use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use eframe::egui;

use super::screenshot_capture::{ScreenshotCaptureSnapshot, ScreenshotRequestId};

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

const WINDOW_CREATE_TIMEOUT: Duration = Duration::from_secs(4);
const WINDOW_VISIBILITY_TIMEOUT: Duration = Duration::from_secs(3);
const WINDOW_CLOSE_TIMEOUT: Duration = Duration::from_secs(3);
pub(crate) const NATIVE_POLL_INTERVAL: Duration = Duration::from_millis(40);

const WINDOW_WIDTH_DIP: i32 = 360;
const WINDOW_HEIGHT_DIP: i32 = 142;
const CANCEL_Y_DIP: i32 = 82;
const CANCEL_HEIGHT_DIP: i32 = 30;

#[derive(Clone, Copy)]
struct ClientLayout {
    width: i32,
    height: i32,
    label_left: i32,
    label_top: i32,
    label_width: i32,
    label_height: i32,
    cancel_left: i32,
    cancel_top: i32,
    cancel_width: i32,
    cancel_height: i32,
}

fn scaled_metric(value_dip: i32, dpi: u32) -> Option<i32> {
    if dpi == 0 || value_dip < 0 {
        return None;
    }
    let scaled = (i64::from(value_dip) * i64::from(dpi) + 48) / 96;
    i32::try_from(scaled).ok()
}

fn client_layout_for_dpi(dpi: u32) -> Option<ClientLayout> {
    let width = scaled_metric(WINDOW_WIDTH_DIP, dpi)?;
    let height = scaled_metric(WINDOW_HEIGHT_DIP, dpi)?;
    let cancel_width = scaled_metric(164, dpi)?;
    let cancel_height = scaled_metric(CANCEL_HEIGHT_DIP, dpi)?;
    let cancel_top = scaled_metric(CANCEL_Y_DIP, dpi)?;
    let layout = ClientLayout {
        width,
        height,
        label_left: scaled_metric(16, dpi)?,
        label_top: scaled_metric(18, dpi)?,
        label_width: width.saturating_sub(scaled_metric(32, dpi)?),
        label_height: scaled_metric(52, dpi)?,
        cancel_left: width.saturating_sub(cancel_width) / 2,
        cancel_top,
        cancel_width,
        cancel_height,
    };
    let cancel_fits = layout.cancel_left >= 0
        && layout.cancel_top >= 0
        && layout.cancel_width > 0
        && layout.cancel_height > 0
        && layout.cancel_left.saturating_add(layout.cancel_width) <= layout.width
        && layout.cancel_top.saturating_add(layout.cancel_height) <= layout.height;
    cancel_fits.then_some(layout)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    AwaitingNativeWindow,
    Showing,
    Visible,
    Hiding,
    Hidden,
    Closing,
}

#[cfg(feature = "qa-framebuffer-capture")]
impl Stage {
    fn label(self) -> &'static str {
        match self {
            Self::AwaitingNativeWindow => "awaiting_native_window",
            Self::Showing => "showing",
            Self::Visible => "visible",
            Self::Hiding => "hiding",
            Self::Hidden => "hidden",
            Self::Closing => "closing",
        }
    }
}

#[derive(Default)]
struct ChildSignals {
    cancel_requested: AtomicBool,
    close_requested: AtomicBool,
    cancel_button: AtomicUsize,
}

#[derive(Clone, Copy)]
struct NativeCountdownWindow {
    hwnd: usize,
    label_hwnd: usize,
    cancel_hwnd: usize,
    owner_hwnd: usize,
    owner_thread_id: u32,
    signals_identity: usize,
}

struct NativeCreationFailure {
    message: String,
    window: Option<NativeCountdownWindow>,
    retired: bool,
}

#[cfg(test)]
#[derive(Clone)]
pub(crate) struct ScreenshotCountdownTestNative {
    state: Arc<std::sync::Mutex<TestNativeState>>,
}

#[cfg(test)]
#[derive(Default)]
struct TestNativeState {
    alive: bool,
    visible: bool,
    root_hidden: bool,
    acknowledge_visibility: bool,
    allow_destroy: bool,
    valid_identity: bool,
    root_hwnd: Option<usize>,
    title: Option<String>,
    signals: Option<Arc<ChildSignals>>,
    operations: Vec<&'static str>,
}

#[cfg(test)]
impl Default for ScreenshotCountdownTestNative {
    fn default() -> Self {
        Self {
            state: Arc::new(std::sync::Mutex::new(TestNativeState {
                acknowledge_visibility: true,
                allow_destroy: true,
                valid_identity: true,
                ..Default::default()
            })),
        }
    }
}

#[cfg(test)]
impl ScreenshotCountdownTestNative {
    pub(crate) fn set_root_hidden(&self, hidden: bool) {
        self.state.lock().unwrap().root_hidden = hidden;
    }

    pub(crate) fn root_is_hidden(&self) -> bool {
        self.state.lock().unwrap().root_hidden
    }

    pub(crate) fn set_visibility_acknowledgement(&self, enabled: bool) {
        self.state.lock().unwrap().acknowledge_visibility = enabled;
    }

    pub(crate) fn set_identity_valid(&self, valid: bool) {
        self.state.lock().unwrap().valid_identity = valid;
    }

    pub(crate) fn request_cancel(&self) {
        if let Some(signals) = &self.state.lock().unwrap().signals {
            signals.cancel_requested.store(true, Ordering::Release);
        }
    }

    pub(crate) fn operations(&self) -> Vec<&'static str> {
        self.state.lock().unwrap().operations.clone()
    }

    pub(crate) fn is_alive(&self) -> bool {
        self.state.lock().unwrap().alive
    }

    fn create(
        &self,
        title: &str,
        root_hwnd: usize,
        signals: Arc<ChildSignals>,
    ) -> Result<NativeCountdownWindow, NativeCreationFailure> {
        let mut state = self.state.lock().unwrap();
        state.operations.push("create");
        state.alive = true;
        state.visible = false;
        state.root_hwnd = Some(root_hwnd);
        state.title = Some(title.to_owned());
        state.signals = Some(signals);
        state.valid_identity = true;
        Ok(NativeCountdownWindow {
            hwnd: 0x5343_0001,
            label_hwnd: 0x5343_0002,
            cancel_hwnd: 0x5343_0003,
            owner_hwnd: root_hwnd,
            owner_thread_id: 1,
            signals_identity: Arc::as_ptr(state.signals.as_ref().unwrap()) as usize,
        })
    }

    fn show(
        &self,
        window: NativeCountdownWindow,
        title: &str,
        signals: &Arc<ChildSignals>,
    ) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        state.operations.push("show");
        if !test_identity_is_current(&state, window, title, signals) || state.visible {
            return Err("fake countdown show refused a stale native binding".into());
        }
        if state.acknowledge_visibility {
            state.visible = true;
        }
        Ok(())
    }

    fn set_visible(
        &self,
        window: NativeCountdownWindow,
        title: &str,
        signals: &Arc<ChildSignals>,
        visible: bool,
    ) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        state.operations.push(if visible { "show" } else { "hide" });
        if !test_identity_is_current(&state, window, title, signals) {
            return Err("fake countdown visibility change refused a stale native binding".into());
        }
        if state.acknowledge_visibility {
            state.visible = visible;
        }
        Ok(())
    }

    fn set_label_text(
        &self,
        window: NativeCountdownWindow,
        title: &str,
        signals: &Arc<ChildSignals>,
        _text: &str,
    ) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        if !test_identity_is_current(&state, window, title, signals) {
            return Err("fake countdown label update refused a stale native binding".into());
        }
        state.operations.push("update_label");
        Ok(())
    }

    fn inspect(
        &self,
        window: NativeCountdownWindow,
        title: &str,
        root_hwnd: Option<usize>,
        signals: &Arc<ChildSignals>,
    ) -> Result<Option<bool>, String> {
        let mut state = self.state.lock().unwrap();
        state.operations.push("inspect");
        if !state.alive {
            return Ok(None);
        }
        if !state.valid_identity
            || state.title.as_deref() != Some(title)
            || state.root_hwnd != root_hwnd
            || !test_identity_is_current(&state, window, title, signals)
        {
            return Err("fake countdown owner identity changed".into());
        }
        Ok(Some(state.visible))
    }

    fn destroy(
        &self,
        window: NativeCountdownWindow,
        title: &str,
        signals: &Arc<ChildSignals>,
    ) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        state.operations.push("destroy");
        if !test_identity_is_current(&state, window, title, signals) {
            return Err("fake countdown destruction refused a stale native binding".into());
        }
        if !state.allow_destroy {
            return Err("fake countdown destruction is pending".into());
        }
        state.alive = false;
        state.visible = false;
        Ok(())
    }

    fn launcher_native_visible_and_owned(&self, root_hwnd: usize) -> Result<bool, String> {
        let mut state = self.state.lock().unwrap();
        state.operations.push("verify_root_visibility");
        if state.root_hwnd != Some(root_hwnd) || !state.valid_identity {
            return Err("fake launcher root identity changed".into());
        }
        Ok(!state.root_hidden)
    }

    #[cfg(feature = "qa-framebuffer-capture")]
    fn qa_geometry(
        &self,
        window: NativeCountdownWindow,
        title: &str,
        signals: &Arc<ChildSignals>,
    ) -> Option<NativeCountdownGeometry> {
        let state = self.state.lock().unwrap();
        test_identity_is_current(&state, window, title, signals).then_some(
            NativeCountdownGeometry {
                visible: state.visible,
                no_activate_verified: true,
                countdown_screen_rect: [100, 100, 460, 242],
                cancel_screen_rect: [198, 182, 362, 212],
            },
        )
    }
}

#[cfg(test)]
fn test_identity_is_current(
    state: &TestNativeState,
    window: NativeCountdownWindow,
    title: &str,
    signals: &Arc<ChildSignals>,
) -> bool {
    state.alive
        && state.valid_identity
        && state.title.as_deref() == Some(title)
        && state.root_hwnd == Some(window.owner_hwnd)
        && window.signals_identity == Arc::as_ptr(signals) as usize
        && state
            .signals
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, signals))
}

struct CountdownWindow {
    capture: ScreenshotCaptureSnapshot,
    title: String,
    signals: Arc<ChildSignals>,
    stage: Stage,
    started_at: Instant,
    started_at_unix_ms: u64,
    stage_started_at: Instant,
    created_at_unix_ms: Option<u64>,
    visible_ack_at_unix_ms: Option<u64>,
    hide_requested_at_unix_ms: Option<u64>,
    hidden_ack_at_unix_ms: Option<u64>,
    close_requested_at_unix_ms: Option<u64>,
    cancel_requested_at_unix_ms: Option<u64>,
    retired_at_unix_ms: Option<u64>,
    window: Option<NativeCountdownWindow>,
    creation_attempted: bool,
    creation_retired: bool,
    native_created: bool,
    native_destroyed: bool,
    cancel_requested: bool,
    native_closed: bool,
    close_timeout_reported: bool,
    failure_reported: bool,
    displayed_seconds: Option<u64>,
}

#[cfg(feature = "qa-framebuffer-capture")]
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub(crate) struct ScreenshotCountdownQaSnapshot {
    pub request_id: u64,
    pub stage: &'static str,
    pub countdown_hwnd: Option<usize>,
    pub cancel_hwnd: Option<usize>,
    pub owner_hwnd: Option<usize>,
    pub visible: Option<bool>,
    pub no_activate_verified: Option<bool>,
    pub countdown_screen_rect: Option<[i32; 4]>,
    pub cancel_screen_rect: Option<[i32; 4]>,
    pub started_at_unix_ms: u64,
    pub created_at_unix_ms: Option<u64>,
    pub visible_ack_at_unix_ms: Option<u64>,
    pub hide_requested_at_unix_ms: Option<u64>,
    pub hidden_ack_at_unix_ms: Option<u64>,
    pub close_requested_at_unix_ms: Option<u64>,
    pub cancel_requested_at_unix_ms: Option<u64>,
    pub retired_at_unix_ms: Option<u64>,
}

#[cfg(feature = "qa-framebuffer-capture")]
#[derive(Clone, Copy)]
struct NativeCountdownGeometry {
    visible: bool,
    no_activate_verified: bool,
    countdown_screen_rect: [i32; 4],
    cancel_screen_rect: [i32; 4],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CountdownEvent {
    Visible(ScreenshotRequestId),
    Cancel(ScreenshotRequestId),
    NativeClosed(ScreenshotRequestId),
    Hidden(ScreenshotRequestId),
    Closed(ScreenshotRequestId),
    Failed(ScreenshotRequestId, String),
}

/// Owns the one nonactivating native countdown window for a delayed capture.
/// Creation, visibility changes, and destruction are synchronous Win32 calls
/// made by ROOT, so there is no deferred child creation that can outlive cancel.
#[derive(Default)]
pub(crate) struct ScreenshotCountdownPresentation {
    active: Option<CountdownWindow>,
    #[cfg(feature = "qa-framebuffer-capture")]
    last_qa_snapshot: Option<ScreenshotCountdownQaSnapshot>,
    #[cfg(test)]
    test_native: Option<ScreenshotCountdownTestNative>,
}

impl ScreenshotCountdownPresentation {
    pub(crate) fn begin(
        &mut self,
        capture: ScreenshotCaptureSnapshot,
        now: Instant,
    ) -> Result<(), String> {
        if self.active.is_some() {
            return Err("A screenshot countdown window is still retiring".into());
        }
        let request = capture.request_id.get();
        let process = std::process::id();
        let title = format!("Multi Launcher Screenshot Countdown [{process}:{request}]");
        self.active = Some(CountdownWindow {
            capture,
            title,
            signals: Arc::new(ChildSignals::default()),
            stage: Stage::AwaitingNativeWindow,
            started_at: now,
            started_at_unix_ms: unix_millis(),
            stage_started_at: now,
            created_at_unix_ms: None,
            visible_ack_at_unix_ms: None,
            hide_requested_at_unix_ms: None,
            hidden_ack_at_unix_ms: None,
            close_requested_at_unix_ms: None,
            cancel_requested_at_unix_ms: None,
            retired_at_unix_ms: None,
            window: None,
            creation_attempted: false,
            creation_retired: false,
            native_created: false,
            native_destroyed: false,
            cancel_requested: false,
            native_closed: false,
            close_timeout_reported: false,
            failure_reported: false,
            displayed_seconds: None,
        });
        #[cfg(feature = "qa-framebuffer-capture")]
        {
            self.last_qa_snapshot = None;
        }
        Ok(())
    }

    pub(crate) fn is_busy(&self) -> bool {
        self.active.is_some()
    }

    #[cfg(test)]
    pub(crate) fn set_test_native(&mut self, native: ScreenshotCountdownTestNative) {
        self.test_native = Some(native);
    }

    pub(crate) fn request_id(&self) -> Option<ScreenshotRequestId> {
        self.active.as_ref().map(|active| active.capture.request_id)
    }

    pub(crate) fn hwnd(&self, request_id: ScreenshotRequestId) -> Option<usize> {
        self.active
            .as_ref()
            .filter(|active| active.capture.request_id == request_id)
            .and_then(|active| active.window.map(|window| window.hwnd))
    }

    #[cfg(feature = "qa-framebuffer-capture")]
    pub(crate) fn qa_snapshot(&self) -> Option<ScreenshotCountdownQaSnapshot> {
        let Some(active) = self.active.as_ref() else {
            return self.last_qa_snapshot.clone();
        };
        let mut snapshot = self
            .last_qa_snapshot
            .as_ref()
            .filter(|last| last.request_id == active.capture.request_id.get())
            .cloned()
            .unwrap_or_default();
        snapshot.request_id = active.capture.request_id.get();
        snapshot.stage = active.stage.label();
        snapshot.countdown_hwnd = active.window.map(|window| window.hwnd);
        snapshot.cancel_hwnd = active.window.map(|window| window.cancel_hwnd);
        snapshot.owner_hwnd = active.window.map(|window| window.owner_hwnd);
        snapshot.started_at_unix_ms = active.started_at_unix_ms;
        snapshot.created_at_unix_ms = active.created_at_unix_ms;
        snapshot.visible_ack_at_unix_ms = active.visible_ack_at_unix_ms;
        snapshot.hide_requested_at_unix_ms = active.hide_requested_at_unix_ms;
        snapshot.hidden_ack_at_unix_ms = active.hidden_ack_at_unix_ms;
        snapshot.close_requested_at_unix_ms = active.close_requested_at_unix_ms;
        snapshot.cancel_requested_at_unix_ms = active.cancel_requested_at_unix_ms;
        snapshot.retired_at_unix_ms = active.retired_at_unix_ms;
        self.update_qa_geometry(&mut snapshot, active);
        Some(snapshot)
    }

    pub(crate) fn visible_acknowledged(&self, request_id: ScreenshotRequestId) -> bool {
        self.active.as_ref().is_some_and(|active| {
            active.capture.request_id == request_id && active.stage == Stage::Visible
        })
    }

    pub(crate) fn hidden_acknowledged(&self, request_id: ScreenshotRequestId) -> bool {
        self.active.as_ref().is_some_and(|active| {
            active.capture.request_id == request_id && active.stage == Stage::Hidden
        })
    }

    /// Creates the native window only on the ROOT runtime tick. The HWND is
    /// owned by ROOT at creation and remains hidden until its activation guard
    /// is installed.
    pub(crate) fn poll_native(
        &mut self,
        ctx: &egui::Context,
        root_hwnd: Option<usize>,
        now: Instant,
    ) -> Option<CountdownEvent> {
        let mut active = self.active.take()?;
        if active
            .signals
            .cancel_requested
            .swap(false, Ordering::AcqRel)
        {
            active.cancel_requested = true;
            active
                .cancel_requested_at_unix_ms
                .get_or_insert_with(unix_millis);
        }
        if active.signals.close_requested.swap(false, Ordering::AcqRel) {
            active.native_closed = true;
        }
        let id = active.capture.request_id;

        if cancellation_is_actionable(active.stage, active.cancel_requested) {
            active.cancel_requested = false;
            self.active = Some(active);
            return Some(CountdownEvent::Cancel(id));
        }
        if active.native_closed && active.stage != Stage::Closing {
            self.active = Some(active);
            return Some(CountdownEvent::NativeClosed(id));
        }

        let event = match active.stage {
            Stage::AwaitingNativeWindow => {
                if now.saturating_duration_since(active.started_at) > WINDOW_CREATE_TIMEOUT {
                    Self::fail(&mut active, "countdown native window could not be created")
                } else if let Some(root_hwnd) = root_hwnd {
                    active.creation_attempted = true;
                    match self.create_native(
                        &active.title,
                        root_hwnd,
                        &active.capture,
                        Arc::clone(&active.signals),
                    ) {
                        Ok(window) => {
                            active.native_created = true;
                            active.created_at_unix_ms = Some(unix_millis());
                            active.window = Some(window);
                            active.stage = Stage::Showing;
                            active.stage_started_at = now;
                            match self.show_native(window, &active.title, &active.signals) {
                                Ok(()) => {
                                    ctx.request_repaint_after(NATIVE_POLL_INTERVAL);
                                    None
                                }
                                Err(error) => Self::fail(&mut active, &error),
                            }
                        }
                        Err(failure) => {
                            active.window = failure.window;
                            active.creation_retired = failure.retired;
                            Self::fail(&mut active, &failure.message)
                        }
                    }
                } else {
                    ctx.request_repaint_after(NATIVE_POLL_INTERVAL);
                    None
                }
            }
            Stage::Showing | Stage::Visible | Stage::Hiding | Stage::Hidden => {
                let Some(window) = active.window else {
                    let event = Self::fail(&mut active, "countdown HWND identity was not retained");
                    self.active = Some(active);
                    return event;
                };
                match self.inspect_native(window, &active.title, root_hwnd, &active.signals) {
                    Ok(None) => {
                        active.window = None;
                        active.native_destroyed = active.native_created;
                        match active.stage {
                            Stage::Hiding | Stage::Hidden => {
                                active.stage = Stage::Hidden;
                                Some(CountdownEvent::Hidden(id))
                            }
                            _ => Some(CountdownEvent::NativeClosed(id)),
                        }
                    }
                    Err(error) => Self::fail(&mut active, &error),
                    Ok(Some(visible)) => match active.stage {
                        Stage::Showing if visible => {
                            active.stage = Stage::Visible;
                            active.stage_started_at = now;
                            active.visible_ack_at_unix_ms = Some(unix_millis());
                            Some(CountdownEvent::Visible(id))
                        }
                        Stage::Showing
                            if visibility_ack_timed_out(
                                Stage::Showing,
                                now.saturating_duration_since(active.stage_started_at),
                            ) =>
                        {
                            Self::fail(
                                &mut active,
                                "countdown window did not acknowledge visibility",
                            )
                        }
                        Stage::Hiding
                            if visibility_ack_timed_out(
                                Stage::Hiding,
                                now.saturating_duration_since(active.stage_started_at),
                            ) =>
                        {
                            Self::fail(&mut active, "countdown window did not acknowledge hide")
                        }
                        Stage::Hiding if !visible => {
                            active.stage = Stage::Hidden;
                            active.hidden_ack_at_unix_ms = Some(unix_millis());
                            Some(CountdownEvent::Hidden(id))
                        }
                        Stage::Hiding | Stage::Showing => {
                            ctx.request_repaint_after(NATIVE_POLL_INTERVAL);
                            None
                        }
                        Stage::Visible if !visible => Some(CountdownEvent::NativeClosed(id)),
                        Stage::Visible => {
                            let seconds = remaining_seconds(active.capture.deadline, now);
                            if active.displayed_seconds != Some(seconds) {
                                let label = countdown_label(active.capture.mode, seconds);
                                if let Err(error) = self.set_label_text_native(
                                    window,
                                    &active.title,
                                    &active.signals,
                                    &label,
                                ) {
                                    Self::fail(&mut active, &error)
                                } else {
                                    active.displayed_seconds = Some(seconds);
                                    ctx.request_repaint_after(countdown_repaint_delay(
                                        active.capture.deadline,
                                        now,
                                    ));
                                    None
                                }
                            } else {
                                ctx.request_repaint_after(countdown_repaint_delay(
                                    active.capture.deadline,
                                    now,
                                ));
                                None
                            }
                        }
                        Stage::Hidden if visible => Some(CountdownEvent::NativeClosed(id)),
                        Stage::Hidden => None,
                        _ => None,
                    },
                }
            }
            Stage::Closing => {
                if let Some(window) = active.window {
                    match self.destroy_native(window, &active.title, &active.signals) {
                        Ok(()) => {
                            active.window = None;
                            active.native_destroyed = true;
                            active.retired_at_unix_ms = Some(unix_millis());
                            Some(CountdownEvent::Closed(id))
                        }
                        Err(error) => {
                            if !active.close_timeout_reported
                                && now.saturating_duration_since(active.stage_started_at)
                                    > WINDOW_CLOSE_TIMEOUT
                            {
                                active.close_timeout_reported = true;
                                Some(CountdownEvent::Failed(id, error))
                            } else {
                                ctx.request_repaint_after(NATIVE_POLL_INTERVAL);
                                None
                            }
                        }
                    }
                } else if can_finish_native_retirement(
                    active.creation_attempted,
                    active.creation_retired,
                    active.native_created,
                    active.native_destroyed,
                ) {
                    active.retired_at_unix_ms.get_or_insert_with(unix_millis);
                    Some(CountdownEvent::Closed(id))
                } else if !active.close_timeout_reported
                    && now.saturating_duration_since(active.stage_started_at) > WINDOW_CLOSE_TIMEOUT
                {
                    active.close_timeout_reported = true;
                    Some(CountdownEvent::Failed(
                        id,
                        "countdown native window retirement is still unverified".into(),
                    ))
                } else {
                    ctx.request_repaint_after(NATIVE_POLL_INTERVAL);
                    None
                }
            }
        };

        if !matches!(event, Some(CountdownEvent::Closed(_))) {
            #[cfg(feature = "qa-framebuffer-capture")]
            if let Some(snapshot) = self.qa_snapshot_for(&active) {
                self.last_qa_snapshot = Some(snapshot);
            }
            self.active = Some(active);
        } else {
            #[cfg(feature = "qa-framebuffer-capture")]
            if let Some(snapshot) = self.qa_snapshot_for(&active) {
                self.last_qa_snapshot = Some(snapshot);
            }
        }
        event
    }

    pub(crate) fn request_hide(
        &mut self,
        request_id: ScreenshotRequestId,
        ctx: &egui::Context,
        now: Instant,
    ) -> Result<(), String> {
        let (window, title, signals) = {
            let active = self.current_mut(request_id)?;
            if active.stage != Stage::Visible {
                return Err("countdown was not visibly acknowledged before hide".into());
            }
            let window = active
                .window
                .ok_or_else(|| "countdown native window is unavailable".to_owned())?;
            (window, active.title.clone(), Arc::clone(&active.signals))
        };
        self.set_visible_native(window, &title, &signals, false)?;
        let active = self.current_mut(request_id)?;
        active.stage = Stage::Hiding;
        active.stage_started_at = now;
        active.hide_requested_at_unix_ms = Some(unix_millis());
        ctx.request_repaint_after(NATIVE_POLL_INTERVAL);
        Ok(())
    }

    pub(crate) fn request_close(
        &mut self,
        request_id: ScreenshotRequestId,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        let active = self.current_mut(request_id)?;
        if active.stage != Stage::Closing {
            active.stage = Stage::Closing;
            active.stage_started_at = Instant::now();
            active.close_timeout_reported = false;
            active.close_requested_at_unix_ms = Some(unix_millis());
        }
        ctx.request_repaint_after(NATIVE_POLL_INTERVAL);
        Ok(())
    }

    pub(crate) fn shutdown(&mut self) {
        if let Some(active) = self.active.take()
            && let Some(window) = active.window
            && let Err(error) = self.destroy_native(window, &active.title, &active.signals)
        {
            tracing::error!(%error, request_id = active.capture.request_id.get(), "failed to destroy screenshot countdown during shutdown; native callback lease remains owned by its HWND");
        }
    }

    /// Verifies the exact launcher-root HWND and reports its native visible
    /// bit. The bit is informational; capture safety belongs to the parking
    /// transaction that owns and verifies the off-desktop position.
    pub(crate) fn launcher_native_visible_and_owned(&self, hwnd: usize) -> Result<bool, String> {
        self.launcher_native_visible(hwnd)
    }

    fn create_native(
        &self,
        title: &str,
        root_hwnd: usize,
        capture: &ScreenshotCaptureSnapshot,
        signals: Arc<ChildSignals>,
    ) -> Result<NativeCountdownWindow, NativeCreationFailure> {
        #[cfg(test)]
        if let Some(native) = &self.test_native {
            return native.create(title, root_hwnd, signals);
        }
        native::create(title, root_hwnd, capture, signals)
    }

    fn show_native(
        &self,
        window: NativeCountdownWindow,
        title: &str,
        signals: &Arc<ChildSignals>,
    ) -> Result<(), String> {
        #[cfg(test)]
        if let Some(native) = &self.test_native {
            return native.show(window, title, signals);
        }
        native::show(window, title, signals)
    }

    fn set_visible_native(
        &self,
        window: NativeCountdownWindow,
        title: &str,
        signals: &Arc<ChildSignals>,
        visible: bool,
    ) -> Result<(), String> {
        #[cfg(test)]
        if let Some(native) = &self.test_native {
            return native.set_visible(window, title, signals, visible);
        }
        native::set_visible(window, title, signals, visible)
    }

    fn set_label_text_native(
        &self,
        window: NativeCountdownWindow,
        title: &str,
        signals: &Arc<ChildSignals>,
        text: &str,
    ) -> Result<(), String> {
        #[cfg(test)]
        if let Some(native) = &self.test_native {
            return native.set_label_text(window, title, signals, text);
        }
        native::set_label_text(window, title, signals, text)
    }

    fn inspect_native(
        &self,
        window: NativeCountdownWindow,
        title: &str,
        root_hwnd: Option<usize>,
        signals: &Arc<ChildSignals>,
    ) -> Result<Option<bool>, String> {
        #[cfg(test)]
        if let Some(native) = &self.test_native {
            return native.inspect(window, title, root_hwnd, signals);
        }
        native::inspect(window, title, root_hwnd, signals)
    }

    fn destroy_native(
        &self,
        window: NativeCountdownWindow,
        title: &str,
        signals: &Arc<ChildSignals>,
    ) -> Result<(), String> {
        #[cfg(test)]
        if let Some(native) = &self.test_native {
            return native.destroy(window, title, signals);
        }
        native::destroy(window, title, signals)
    }

    fn launcher_native_visible(&self, hwnd: usize) -> Result<bool, String> {
        #[cfg(test)]
        if let Some(native) = &self.test_native {
            return native.launcher_native_visible_and_owned(hwnd);
        }
        native::launcher_native_visible_and_owned(hwnd)
    }

    fn current_mut(
        &mut self,
        request_id: ScreenshotRequestId,
    ) -> Result<&mut CountdownWindow, String> {
        self.active
            .as_mut()
            .filter(|active| active.capture.request_id == request_id)
            .ok_or_else(|| "screenshot countdown request is stale".to_owned())
    }

    #[cfg(feature = "qa-framebuffer-capture")]
    fn qa_snapshot_for(&self, active: &CountdownWindow) -> Option<ScreenshotCountdownQaSnapshot> {
        let mut snapshot = self
            .last_qa_snapshot
            .as_ref()
            .filter(|last| last.request_id == active.capture.request_id.get())
            .cloned()
            .unwrap_or_default();
        snapshot.request_id = active.capture.request_id.get();
        snapshot.stage = active.stage.label();
        snapshot.countdown_hwnd = active.window.map(|window| window.hwnd);
        snapshot.cancel_hwnd = active.window.map(|window| window.cancel_hwnd);
        snapshot.owner_hwnd = active.window.map(|window| window.owner_hwnd);
        snapshot.started_at_unix_ms = active.started_at_unix_ms;
        snapshot.created_at_unix_ms = active.created_at_unix_ms;
        snapshot.visible_ack_at_unix_ms = active.visible_ack_at_unix_ms;
        snapshot.hide_requested_at_unix_ms = active.hide_requested_at_unix_ms;
        snapshot.hidden_ack_at_unix_ms = active.hidden_ack_at_unix_ms;
        snapshot.close_requested_at_unix_ms = active.close_requested_at_unix_ms;
        snapshot.cancel_requested_at_unix_ms = active.cancel_requested_at_unix_ms;
        snapshot.retired_at_unix_ms = active.retired_at_unix_ms;
        self.update_qa_geometry(&mut snapshot, active);
        Some(snapshot)
    }

    #[cfg(feature = "qa-framebuffer-capture")]
    fn update_qa_geometry(
        &self,
        snapshot: &mut ScreenshotCountdownQaSnapshot,
        active: &CountdownWindow,
    ) {
        match active.window.and_then(|window| {
            self.qa_geometry_native(window, &active.title, &active.signals)
                .ok()
                .flatten()
        }) {
            Some(geometry) => {
                snapshot.visible = Some(geometry.visible);
                snapshot.no_activate_verified = Some(geometry.no_activate_verified);
                snapshot.countdown_screen_rect = Some(geometry.countdown_screen_rect);
                snapshot.cancel_screen_rect = Some(geometry.cancel_screen_rect);
            }
            None if active.retired_at_unix_ms.is_some() => {
                snapshot.visible = Some(false);
            }
            None => {
                snapshot.visible = None;
                snapshot.no_activate_verified = None;
                snapshot.countdown_screen_rect = None;
                snapshot.cancel_screen_rect = None;
            }
        }
    }

    #[cfg(feature = "qa-framebuffer-capture")]
    fn qa_geometry_native(
        &self,
        window: NativeCountdownWindow,
        title: &str,
        signals: &Arc<ChildSignals>,
    ) -> Result<Option<NativeCountdownGeometry>, String> {
        #[cfg(test)]
        if let Some(native) = &self.test_native {
            return Ok(native.qa_geometry(window, title, signals));
        }
        native::qa_geometry(window, title, signals)
    }

    fn fail(active: &mut CountdownWindow, error: &str) -> Option<CountdownEvent> {
        let id = active.capture.request_id;
        let report = !active.failure_reported;
        active.failure_reported = true;
        if active.stage != Stage::Closing {
            active.stage = Stage::Closing;
            active.stage_started_at = Instant::now();
            active.close_timeout_reported = false;
        }
        report.then(|| CountdownEvent::Failed(id, error.to_owned()))
    }
}

fn cancellation_is_actionable(stage: Stage, cancel_requested: bool) -> bool {
    cancel_requested && stage != Stage::Closing
}

fn can_finish_native_retirement(
    creation_attempted: bool,
    creation_retired: bool,
    native_created: bool,
    native_destroyed: bool,
) -> bool {
    !creation_attempted || creation_retired || (native_created && native_destroyed)
}

fn visibility_ack_timed_out(stage: Stage, elapsed: Duration) -> bool {
    matches!(stage, Stage::Showing | Stage::Hiding) && elapsed > WINDOW_VISIBILITY_TIMEOUT
}

fn remaining_seconds(deadline: Instant, now: Instant) -> u64 {
    let remaining = deadline.saturating_duration_since(now);
    remaining
        .as_secs()
        .saturating_add(u64::from(remaining.subsec_nanos() > 0))
}

fn countdown_label(mode: crate::commands::ScreenshotMode, seconds: u64) -> String {
    let mode = match mode {
        crate::commands::ScreenshotMode::Window => "Window",
        crate::commands::ScreenshotMode::Region => "Region",
        crate::commands::ScreenshotMode::Desktop => "Desktop",
    };
    format!("{mode} capture starts in {seconds} seconds. Prepare the target window.")
}

fn countdown_repaint_delay(deadline: Instant, now: Instant) -> Duration {
    let remaining = deadline.saturating_duration_since(now);
    let within_second = remaining.subsec_nanos() as u64;
    let until_next_second = if within_second == 0 {
        Duration::from_secs(1)
    } else {
        Duration::from_nanos(within_second)
    };
    until_next_second.min(remaining.max(Duration::from_millis(16)))
}

#[cfg(windows)]
mod native {
    use std::ffi::c_void;
    use std::sync::Arc;
    use std::sync::atomic::Ordering;

    use windows::{
        Win32::{
            Foundation::{HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
            System::LibraryLoader::GetModuleHandleW,
            UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow},
            UI::{
                Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
                WindowsAndMessaging::{
                    BN_CLICKED, BS_PUSHBUTTON, CreateWindowExW, DestroyWindow, GA_ROOT,
                    GA_ROOTOWNER, GWL_EXSTYLE, GetAncestor, GetParent, GetPropW, GetWindowLongPtrW,
                    GetWindowRect, GetWindowThreadProcessId, HMENU, IsWindow, IsWindowVisible,
                    MA_NOACTIVATE, RemovePropW, STYLESTRUCT, SW_HIDE, SW_SHOWNOACTIVATE,
                    SWP_NOACTIVATE, SWP_NOZORDER, SetPropW, SetWindowPos, SetWindowTextW,
                    ShowWindow, WM_CLOSE, WM_COMMAND, WM_MOUSEACTIVATE, WM_NCDESTROY,
                    WM_STYLECHANGING, WS_CAPTION, WS_CHILD, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
                    WS_EX_TOPMOST, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
                },
            },
        },
        core::w,
    };

    use std::time::Instant;

    #[cfg(feature = "qa-framebuffer-capture")]
    use super::NativeCountdownGeometry;
    use super::{
        CANCEL_HEIGHT_DIP, CANCEL_Y_DIP, ChildSignals, NativeCountdownWindow,
        NativeCreationFailure, WINDOW_HEIGHT_DIP, WINDOW_WIDTH_DIP, client_layout_for_dpi,
        countdown_label, remaining_seconds,
    };

    const CANCEL_CONTROL_ID: usize = 1;
    const SUBCLASS_ID: usize = 0x5343_524e;
    // A per-HWND token replaces GetWindowSubclass, which is only exported by
    // the v6 common-controls side-by-side assembly on supported Windows hosts.
    const BINDING_PROPERTY_NAME: windows::core::PCWSTR =
        w!("MultiLauncher.ScreenshotCountdown.Binding");

    fn verify_root(hwnd: HWND) -> Result<u32, String> {
        if !unsafe { IsWindow(hwnd) }.as_bool() {
            return Err("launcher root HWND was destroyed before countdown setup".into());
        }
        let mut process_id = 0;
        let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        if process_id != std::process::id() || unsafe { GetAncestor(hwnd, GA_ROOT) } != hwnd {
            return Err("launcher root HWND identity changed before countdown setup".into());
        }
        Ok(thread_id)
    }

    unsafe extern "system" fn countdown_subclass(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _subclass_id: usize,
        reference_data: usize,
    ) -> LRESULT {
        match message {
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_STYLECHANGING if wparam.0 as i32 == GWL_EXSTYLE.0 => {
                let styles = lparam.0 as *mut STYLESTRUCT;
                if !styles.is_null() {
                    unsafe { (*styles).styleNew |= WS_EX_NOACTIVATE.0 };
                }
                unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
            }
            WM_COMMAND
                if (wparam.0 & 0xFFFF) == CANCEL_CONTROL_ID
                    && (wparam.0 >> 16) as u32 == BN_CLICKED
                    && lparam.0 as usize
                        == unsafe { &*(reference_data as *const ChildSignals) }
                            .cancel_button
                            .load(Ordering::Acquire) =>
            {
                unsafe { &*(reference_data as *const ChildSignals) }
                    .cancel_requested
                    .store(true, Ordering::Release);
                LRESULT(0)
            }
            WM_CLOSE => {
                unsafe { &*(reference_data as *const ChildSignals) }
                    .close_requested
                    .store(true, Ordering::Release);
                LRESULT(0)
            }
            WM_NCDESTROY => {
                let property = unsafe { GetPropW(hwnd, BINDING_PROPERTY_NAME) };
                if property.0 as usize == reference_data {
                    if let Err(error) = unsafe { RemovePropW(hwnd, BINDING_PROPERTY_NAME) } {
                        tracing::warn!(%error, "failed to remove screenshot countdown HWND binding during destruction");
                    }
                } else {
                    tracing::warn!(
                        hwnd = hwnd.0 as usize,
                        "screenshot countdown HWND binding changed before destruction"
                    );
                }
                if !unsafe { RemoveWindowSubclass(hwnd, Some(countdown_subclass), SUBCLASS_ID) }
                    .as_bool()
                {
                    tracing::warn!(
                        hwnd = hwnd.0 as usize,
                        "failed to remove screenshot countdown subclass during destruction"
                    );
                }
                let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
                drop(unsafe { Arc::from_raw(reference_data as *const ChildSignals) });
                result
            }
            _ => unsafe { DefSubclassProc(hwnd, message, wparam, lparam) },
        }
    }

    pub(super) fn create(
        title: &str,
        root_raw_hwnd: usize,
        capture: &crate::gui::screenshot_capture::ScreenshotCaptureSnapshot,
        signals: std::sync::Arc<ChildSignals>,
    ) -> Result<NativeCountdownWindow, NativeCreationFailure> {
        let root = HWND(root_raw_hwnd as *mut c_void);
        let root_thread_id = match verify_root(root) {
            Ok(thread_id) => thread_id,
            Err(message) => {
                return Err(NativeCreationFailure {
                    message,
                    window: None,
                    retired: true,
                });
            }
        };
        let mut root_rect = RECT::default();
        if let Err(error) = unsafe { GetWindowRect(root, &mut root_rect) } {
            return Err(NativeCreationFailure {
                message: format!("could not position screenshot countdown: {error}"),
                window: None,
                retired: true,
            });
        }
        let dpi = unsafe { GetDpiForWindow(root) };
        let Some(layout) = client_layout_for_dpi(dpi) else {
            return Err(NativeCreationFailure {
                message: "the countdown Cancel button does not fit at the launcher DPI".into(),
                window: None,
                retired: true,
            });
        };
        let style = WS_POPUP | WS_CAPTION | WS_SYSMENU;
        let ex_style = WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST;
        let mut outer = RECT {
            left: 0,
            top: 0,
            right: layout.width,
            bottom: layout.height,
        };
        if let Err(error) =
            unsafe { AdjustWindowRectExForDpi(&mut outer, style, false, ex_style, dpi) }
        {
            return Err(NativeCreationFailure {
                message: format!("could not size countdown controls for launcher DPI: {error}"),
                window: None,
                retired: true,
            });
        }
        let outer_width = outer.right.saturating_sub(outer.left);
        let outer_height = outer.bottom.saturating_sub(outer.top);
        if outer_width <= 0 || outer_height <= 0 {
            return Err(NativeCreationFailure {
                message: "Windows returned invalid countdown window dimensions".into(),
                window: None,
                retired: true,
            });
        }
        let x = i64::from(root_rect.left)
            .saturating_add(
                (i64::from(root_rect.right) - i64::from(root_rect.left) - i64::from(outer_width))
                    / 2,
            )
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        let y = i64::from(root_rect.top)
            .saturating_add(
                (i64::from(root_rect.bottom) - i64::from(root_rect.top) - i64::from(outer_height))
                    / 2,
            )
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        let instance = match unsafe { GetModuleHandleW(None) } {
            Ok(instance) => HINSTANCE::from(instance),
            Err(error) => {
                return Err(NativeCreationFailure {
                    message: format!(
                        "could not identify the launcher module for countdown: {error}"
                    ),
                    window: None,
                    retired: true,
                });
            }
        };
        let title_text = windows::core::HSTRING::from(title);
        let label_text = windows::core::HSTRING::from(countdown_label(
            capture.mode,
            remaining_seconds(capture.deadline, Instant::now()),
        ));
        let top = match unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                w!("STATIC"),
                &title_text,
                style,
                x,
                y,
                outer_width,
                outer_height,
                root,
                HMENU::default(),
                instance,
                None,
            )
        } {
            Ok(hwnd) if !hwnd.is_invalid() => hwnd,
            Ok(_) => {
                return Err(NativeCreationFailure {
                    message: "Windows returned an invalid countdown HWND".into(),
                    window: None,
                    retired: true,
                });
            }
            Err(error) => {
                return Err(NativeCreationFailure {
                    message: format!("could not create screenshot countdown window: {error}"),
                    window: None,
                    retired: true,
                });
            }
        };
        let mut window = NativeCountdownWindow {
            hwnd: top.0 as usize,
            label_hwnd: 0,
            cancel_hwnd: 0,
            owner_hwnd: root_raw_hwnd,
            owner_thread_id: root_thread_id,
            signals_identity: Arc::as_ptr(&signals) as usize,
        };
        let subclass_signals = Arc::into_raw(Arc::clone(&signals)) as usize;
        if let Err(error) = unsafe {
            SetPropW(
                top,
                BINDING_PROPERTY_NAME,
                HANDLE(subclass_signals as *mut c_void),
            )
        } {
            drop(unsafe { Arc::from_raw(subclass_signals as *const ChildSignals) });
            return Err(cleanup_failed_creation(
                window,
                title,
                &signals,
                false,
                &format!("could not bind screenshot countdown identity: {error}"),
            ));
        }
        if !unsafe {
            SetWindowSubclass(top, Some(countdown_subclass), SUBCLASS_ID, subclass_signals)
        }
        .as_bool()
        {
            let _ = unsafe { RemovePropW(top, BINDING_PROPERTY_NAME) };
            drop(unsafe { Arc::from_raw(subclass_signals as *const ChildSignals) });
            return Err(cleanup_failed_creation(
                window,
                title,
                &signals,
                false,
                "could not install screenshot countdown input guard",
            ));
        }
        let label = match unsafe {
            CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                &label_text,
                WS_CHILD | WS_VISIBLE,
                layout.label_left,
                layout.label_top,
                layout.label_width,
                layout.label_height,
                top,
                HMENU::default(),
                instance,
                None,
            )
        } {
            Ok(hwnd) if !hwnd.is_invalid() => hwnd,
            _ => {
                return Err(cleanup_failed_creation(
                    window,
                    title,
                    &signals,
                    true,
                    "could not create countdown text",
                ));
            }
        };
        window.label_hwnd = label.0 as usize;
        let button_text = windows::core::HSTRING::from("Cancel screenshot");
        let button = match unsafe {
            CreateWindowExW(
                Default::default(),
                w!("BUTTON"),
                &button_text,
                WS_CHILD
                    | WS_VISIBLE
                    | WS_TABSTOP
                    | windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(BS_PUSHBUTTON as u32),
                layout.cancel_left,
                layout.cancel_top,
                layout.cancel_width,
                layout.cancel_height,
                top,
                HMENU(CANCEL_CONTROL_ID as *mut c_void),
                instance,
                None,
            )
        } {
            Ok(hwnd) if !hwnd.is_invalid() => hwnd,
            _ => {
                return Err(cleanup_failed_creation(
                    window,
                    title,
                    &signals,
                    true,
                    "could not create countdown Cancel button",
                ));
            }
        };
        window.cancel_hwnd = button.0 as usize;
        signals
            .cancel_button
            .store(window.cancel_hwnd, Ordering::Release);
        if let Err(error) = unsafe {
            SetWindowPos(
                top,
                windows::Win32::UI::WindowsAndMessaging::HWND_TOPMOST,
                x,
                y,
                outer_width,
                outer_height,
                SWP_NOACTIVATE | SWP_NOZORDER,
            )
        } {
            return Err(cleanup_failed_creation(
                window,
                title,
                &signals,
                true,
                &format!("could not position screenshot countdown window: {error}"),
            ));
        }
        if let Err(error) = verify_owned_window(
            window,
            title,
            root_raw_hwnd,
            &signals,
            Some(false),
            true,
            true,
        ) {
            return Err(cleanup_failed_creation(
                window, title, &signals, true, &error,
            ));
        }
        Ok(window)
    }

    fn cleanup_failed_creation(
        window: NativeCountdownWindow,
        title: &str,
        signals: &Arc<ChildSignals>,
        require_binding: bool,
        error: &str,
    ) -> NativeCreationFailure {
        let hwnd = HWND(window.hwnd as *mut c_void);
        if let Err(identity_error) = verify_owned_window(
            window,
            title,
            window.owner_hwnd,
            signals,
            None,
            false,
            require_binding,
        ) {
            return NativeCreationFailure {
                message: format!(
                    "{error}; refusing to destroy an unverified countdown window: {identity_error}"
                ),
                window: Some(window),
                retired: false,
            };
        }
        match unsafe { DestroyWindow(hwnd) } {
            Ok(()) if !unsafe { IsWindow(hwnd) }.as_bool() => NativeCreationFailure {
                message: error.to_owned(),
                window: None,
                retired: true,
            },
            _ => {
                // The owner retains this exact HWND and retries destruction.
                // Child handles are kept only for the native close operation.
                NativeCreationFailure {
                    message: format!("{error}; countdown cleanup is still pending"),
                    window: Some(window),
                    retired: false,
                }
            }
        }
    }

    fn verify_owned_window(
        window: NativeCountdownWindow,
        title: &str,
        root_raw_hwnd: usize,
        signals: &Arc<ChildSignals>,
        expected_visible: Option<bool>,
        require_controls: bool,
        require_binding: bool,
    ) -> Result<(), String> {
        let hwnd = HWND(window.hwnd as *mut c_void);
        let root = HWND(root_raw_hwnd as *mut c_void);
        if !unsafe { IsWindow(hwnd) }.as_bool() || !unsafe { IsWindow(root) }.as_bool() {
            return Err("countdown or launcher HWND was destroyed during setup".into());
        }
        let mut process_id = 0;
        let window_thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        let mut root_process_id = 0;
        let root_thread_id = unsafe { GetWindowThreadProcessId(root, Some(&mut root_process_id)) };
        let binding = unsafe { GetPropW(hwnd, BINDING_PROPERTY_NAME) };
        let binding_matches = binding.0 as usize == window.signals_identity;
        let label = HWND(window.label_hwnd as *mut c_void);
        let cancel = HWND(window.cancel_hwnd as *mut c_void);
        let controls_are_owned = !require_controls
            || (unsafe { IsWindow(label) }.as_bool()
                && unsafe { IsWindow(cancel) }.as_bool()
                && unsafe { GetParent(label) }.is_ok_and(|parent| parent == hwnd)
                && unsafe { GetParent(cancel) }.is_ok_and(|parent| parent == hwnd));
        if process_id != std::process::id()
            || root_process_id != std::process::id()
            || window_thread_id != window.owner_thread_id
            || root_thread_id != window.owner_thread_id
            || window.owner_hwnd != root_raw_hwnd
            || window.signals_identity != Arc::as_ptr(signals) as usize
            || hwnd == root
            || unsafe { GetAncestor(hwnd, GA_ROOT) } != hwnd
            || unsafe { GetAncestor(hwnd, GA_ROOTOWNER) } != root
            || window_title(hwnd).as_deref() != Some(title)
            || (require_binding && !binding_matches)
            || !controls_are_owned
            || expected_visible
                .is_some_and(|expected| unsafe { IsWindowVisible(hwnd) }.as_bool() != expected)
            || unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } & WS_EX_NOACTIVATE.0 as isize == 0
        {
            return Err(
                "countdown native window identity, owner, visibility, or no-activate style changed"
                    .into(),
            );
        }
        Ok(())
    }

    fn window_title(hwnd: HWND) -> Option<String> {
        let mut buffer = vec![0u16; 512];
        let copied =
            unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(hwnd, &mut buffer) };
        if copied <= 0 || copied as usize >= buffer.len() {
            return None;
        }
        String::from_utf16(&buffer[..copied as usize]).ok()
    }

    pub(super) fn show(
        window: NativeCountdownWindow,
        title: &str,
        signals: &Arc<ChildSignals>,
    ) -> Result<(), String> {
        let hwnd = HWND(window.hwnd as *mut c_void);
        verify_owned_window(
            window,
            title,
            window.owner_hwnd,
            signals,
            Some(false),
            true,
            true,
        )?;
        let _ = unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
        verify_owned_window(
            window,
            title,
            window.owner_hwnd,
            signals,
            Some(true),
            true,
            true,
        )
    }

    pub(super) fn set_visible(
        window: NativeCountdownWindow,
        title: &str,
        signals: &Arc<ChildSignals>,
        visible: bool,
    ) -> Result<(), String> {
        let hwnd = HWND(window.hwnd as *mut c_void);
        verify_owned_window(window, title, window.owner_hwnd, signals, None, true, true)?;
        if visible {
            let _ = unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
        } else {
            let _ = unsafe { ShowWindow(hwnd, SW_HIDE) };
        }
        verify_owned_window(
            window,
            title,
            window.owner_hwnd,
            signals,
            Some(visible),
            true,
            true,
        )
    }

    pub(super) fn set_label_text(
        window: NativeCountdownWindow,
        title: &str,
        signals: &Arc<ChildSignals>,
        text: &str,
    ) -> Result<(), String> {
        verify_owned_window(window, title, window.owner_hwnd, signals, None, true, true)?;
        unsafe {
            SetWindowTextW(
                HWND(window.label_hwnd as *mut c_void),
                &windows::core::HSTRING::from(text),
            )
        }
        .map_err(|error| format!("could not update countdown text: {error}"))
    }

    pub(super) fn inspect(
        window: NativeCountdownWindow,
        title: &str,
        root_hwnd: Option<usize>,
        signals: &Arc<ChildSignals>,
    ) -> Result<Option<bool>, String> {
        let hwnd = HWND(window.hwnd as *mut c_void);
        if !unsafe { IsWindow(hwnd) }.as_bool() {
            return Ok(None);
        }
        let Some(root_hwnd) = root_hwnd else {
            return Err("launcher root HWND is unavailable for countdown verification".into());
        };
        verify_owned_window(
            window,
            title,
            root_hwnd,
            signals,
            Some(unsafe { IsWindowVisible(hwnd) }.as_bool()),
            true,
            true,
        )?;
        Ok(Some(unsafe { IsWindowVisible(hwnd) }.as_bool()))
    }

    #[cfg(feature = "qa-framebuffer-capture")]
    pub(super) fn qa_geometry(
        window: NativeCountdownWindow,
        title: &str,
        signals: &Arc<ChildSignals>,
    ) -> Result<Option<NativeCountdownGeometry>, String> {
        let hwnd = HWND(window.hwnd as *mut c_void);
        if !unsafe { IsWindow(hwnd) }.as_bool() {
            return Ok(None);
        }
        let visible = unsafe { IsWindowVisible(hwnd) }.as_bool();
        verify_owned_window(
            window,
            title,
            window.owner_hwnd,
            signals,
            Some(visible),
            true,
            true,
        )?;
        let mut countdown = RECT::default();
        let mut cancel = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut countdown) }
            .map_err(|error| format!("could not read countdown screen bounds: {error}"))?;
        unsafe { GetWindowRect(HWND(window.cancel_hwnd as *mut c_void), &mut cancel) }
            .map_err(|error| format!("could not read countdown Cancel bounds: {error}"))?;
        let valid_rect = |rect: RECT| rect.right > rect.left && rect.bottom > rect.top;
        if !valid_rect(countdown)
            || !valid_rect(cancel)
            || cancel.left < countdown.left
            || cancel.top < countdown.top
            || cancel.right > countdown.right
            || cancel.bottom > countdown.bottom
        {
            return Err(
                "countdown native screen bounds are invalid or outside the owned window".into(),
            );
        }
        Ok(Some(NativeCountdownGeometry {
            visible,
            no_activate_verified: true,
            countdown_screen_rect: [
                countdown.left,
                countdown.top,
                countdown.right,
                countdown.bottom,
            ],
            cancel_screen_rect: [cancel.left, cancel.top, cancel.right, cancel.bottom],
        }))
    }

    pub(super) fn destroy(
        window: NativeCountdownWindow,
        title: &str,
        signals: &Arc<ChildSignals>,
    ) -> Result<(), String> {
        let hwnd = HWND(window.hwnd as *mut c_void);
        if !unsafe { IsWindow(hwnd) }.as_bool() {
            return Ok(());
        }
        verify_owned_window(
            window,
            title,
            window.owner_hwnd,
            signals,
            Some(unsafe { IsWindowVisible(hwnd) }.as_bool()),
            true,
            true,
        )?;
        unsafe { DestroyWindow(hwnd) }
            .map_err(|error| format!("could not close countdown window: {error}"))?;
        if unsafe { IsWindow(hwnd) }.as_bool() {
            return Err("countdown HWND remains after DestroyWindow returned".into());
        }
        Ok(())
    }

    pub(super) fn launcher_native_visible_and_owned(raw_hwnd: usize) -> Result<bool, String> {
        let hwnd = HWND(raw_hwnd as *mut c_void);
        if !unsafe { IsWindow(hwnd) }.as_bool() {
            return Err("launcher root HWND was destroyed before screenshot grant".into());
        }
        let mut process_id = 0;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        if process_id != std::process::id() || unsafe { GetAncestor(hwnd, GA_ROOT) } != hwnd {
            return Err("launcher root HWND identity changed before screenshot grant".into());
        }
        Ok(unsafe { IsWindowVisible(hwnd) }.as_bool())
    }
}

#[cfg(not(windows))]
mod native {
    use std::sync::Arc;

    use super::{ChildSignals, NativeCountdownWindow, NativeCreationFailure};

    pub(super) fn create(
        _: &str,
        _: usize,
        _: &crate::gui::screenshot_capture::ScreenshotCaptureSnapshot,
        _: Arc<ChildSignals>,
    ) -> Result<NativeCountdownWindow, NativeCreationFailure> {
        Err(NativeCreationFailure {
            message: "native screenshot countdown presentation is only available on Windows".into(),
            window: None,
            retired: true,
        })
    }
    pub(super) fn show(
        _: NativeCountdownWindow,
        _: &str,
        _: &Arc<ChildSignals>,
    ) -> Result<(), String> {
        Err("native screenshot countdown presentation is only available on Windows".into())
    }
    pub(super) fn set_visible(
        _: NativeCountdownWindow,
        _: &str,
        _: &Arc<ChildSignals>,
        _: bool,
    ) -> Result<(), String> {
        Err("native screenshot countdown presentation is only available on Windows".into())
    }
    pub(super) fn set_label_text(
        _: NativeCountdownWindow,
        _: &str,
        _: &Arc<ChildSignals>,
        _: &str,
    ) -> Result<(), String> {
        Err("native screenshot countdown presentation is only available on Windows".into())
    }
    pub(super) fn inspect(
        _: NativeCountdownWindow,
        _: &str,
        _: Option<usize>,
        _: &Arc<ChildSignals>,
    ) -> Result<Option<bool>, String> {
        Err("native screenshot countdown presentation is only available on Windows".into())
    }
    #[cfg(feature = "qa-framebuffer-capture")]
    pub(super) fn qa_geometry(
        _: NativeCountdownWindow,
        _: &str,
        _: &Arc<ChildSignals>,
    ) -> Result<Option<super::NativeCountdownGeometry>, String> {
        Err("native screenshot countdown presentation is only available on Windows".into())
    }
    pub(super) fn destroy(
        _: NativeCountdownWindow,
        _: &str,
        _: &Arc<ChildSignals>,
    ) -> Result<(), String> {
        Err("native screenshot countdown presentation is only available on Windows".into())
    }
    pub(super) fn launcher_native_visible_and_owned(_: usize) -> Result<bool, String> {
        Err("native screenshot countdown presentation is only available on Windows".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(id: u64) -> ScreenshotCaptureSnapshot {
        ScreenshotCaptureSnapshot {
            request_id: ScreenshotRequestId::new_for_test(id),
            deadline: Instant::now(),
            scheduled_at_unix_ms: 0,
            deadline_unix_ms: 0,
            capture_started_at_unix_ms: None,
            acquisition_count: 0,
            expected_window_target: None,
            phase: super::super::screenshot_capture::ScreenshotCapturePhase::AwaitingPresentation,
            mode: crate::commands::ScreenshotMode::Window,
            destination: crate::commands::ScreenshotDestination::Clipboard,
            markup: crate::commands::ScreenshotMarkup::Pen,
        }
    }

    #[test]
    fn screenshot_countdown_retirement_requires_sync_creation_or_destroy_receipt() {
        assert!(can_finish_native_retirement(false, false, false, false));
        assert!(can_finish_native_retirement(true, true, false, false));
        assert!(!can_finish_native_retirement(true, false, false, false));
        assert!(!can_finish_native_retirement(true, false, true, false));
        assert!(can_finish_native_retirement(true, false, true, true));
    }

    #[test]
    fn screenshot_countdown_cancel_does_not_starve_closing_retirement() {
        assert!(cancellation_is_actionable(Stage::Visible, true));
        assert!(!cancellation_is_actionable(Stage::Closing, true));
        assert!(!cancellation_is_actionable(Stage::Visible, false));
    }

    #[test]
    fn screenshot_countdown_hide_acknowledgement_has_a_bounded_failure() {
        assert!(!visibility_ack_timed_out(
            Stage::Hiding,
            WINDOW_VISIBILITY_TIMEOUT
        ));
        assert!(visibility_ack_timed_out(
            Stage::Hiding,
            WINDOW_VISIBILITY_TIMEOUT + Duration::from_millis(1)
        ));
        assert!(!visibility_ack_timed_out(
            Stage::Visible,
            WINDOW_VISIBILITY_TIMEOUT * 2
        ));
    }

    #[test]
    fn screenshot_countdown_repaints_at_displayed_second_boundaries() {
        let now = Instant::now();
        let deadline = now + Duration::from_millis(2_250);
        let delay = countdown_repaint_delay(deadline, now);
        assert!(delay > Duration::from_millis(200));
        assert!(delay <= Duration::from_secs(1));
        assert_eq!(remaining_seconds(deadline, now), 3);
        assert_eq!(remaining_seconds(deadline, deadline), 0);
    }

    #[test]
    fn screenshot_countdown_cancel_button_fits_client_area_at_scaled_dpi() {
        for dpi in [96, 120, 144, 192, 240] {
            let layout = client_layout_for_dpi(dpi)
                .unwrap_or_else(|| panic!("Cancel should fit at {dpi} DPI"));
            assert!(layout.width > 0 && layout.height > 0);
            assert!(layout.cancel_left >= 0 && layout.cancel_top >= 0);
            assert!(layout.cancel_width > 0 && layout.cancel_height > 0);
            assert!(layout.cancel_left + layout.cancel_width <= layout.width);
            assert!(layout.cancel_top + layout.cancel_height <= layout.height);
        }
        assert!(client_layout_for_dpi(0).is_none());
    }

    #[test]
    fn screenshot_countdown_refuses_destroying_a_reused_native_binding() {
        let ctx = egui::Context::default();
        let native = ScreenshotCountdownTestNative::default();
        let mut presentation = ScreenshotCountdownPresentation::default();
        presentation.set_test_native(native.clone());
        let started = Instant::now();
        presentation.begin(snapshot(17), started).unwrap();
        assert_eq!(presentation.poll_native(&ctx, Some(42), started), None);
        assert_eq!(
            presentation.poll_native(&ctx, Some(42), started + Duration::from_millis(1)),
            Some(CountdownEvent::Visible(ScreenshotRequestId::new_for_test(
                17
            )))
        );

        presentation
            .request_close(ScreenshotRequestId::new_for_test(17), &ctx)
            .unwrap();
        native.set_identity_valid(false);
        assert_eq!(
            presentation.poll_native(&ctx, Some(42), started + Duration::from_millis(2)),
            None,
            "a recycled or foreign HWND is retained for review, never destroyed"
        );
        assert!(presentation.is_busy());
        assert!(native.is_alive());

        native.set_identity_valid(true);
        assert_eq!(
            presentation.poll_native(&ctx, Some(42), started + Duration::from_millis(3)),
            Some(CountdownEvent::Closed(ScreenshotRequestId::new_for_test(
                17
            )))
        );
        assert!(!presentation.is_busy());
        assert!(!native.is_alive());
    }
}
