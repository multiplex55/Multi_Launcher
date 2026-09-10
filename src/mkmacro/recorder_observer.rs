//! Transient, bounded recorder observations. Expensive providers run away from
//! hook callbacks and the recorder processor; failures are deliberately best effort.

use super::{
    EventContext, HookEvent, KeyTransition, MkPoint, MouseMessage, UiElementInfo, WindowContext,
};
use std::{
    collections::{HashMap, HashSet},
    fmt,
    sync::{Arc, mpsc},
    thread::{self, JoinHandle},
};

#[derive(Debug, Clone, Copy)]
pub struct RawWindowObservation {
    pub timestamp_us: u64,
    pub root: usize,
    pub kind: WindowObservationKind,
}

#[cfg(windows)]
thread_local! {
    // WINEVENT_OUTOFCONTEXT dispatches on the installing message-pump thread.
    // Thread-local publication keeps the callback lock-free and session-owned.
    static WINDOW_EVENT_SENDER: std::cell::RefCell<Option<mpsc::SyncSender<RawWindowObservation>>> =
        const { std::cell::RefCell::new(None) };
}

pub struct NativeWindowObserver {
    receiver: mpsc::Receiver<RawWindowObservation>,
    #[cfg(windows)]
    cancel: Arc<std::sync::atomic::AtomicBool>,
    join: Option<JoinHandle<()>>,
}

/// Injectable session-scoped source for native window notifications.
pub trait WindowEventSource: Send {
    fn drain(&mut self) -> Vec<RawWindowObservation>;
    /// Stops the source before returning its final events.
    fn shutdown_and_drain(&mut self) -> Vec<RawWindowObservation>;
}

impl NativeWindowObserver {
    pub fn start() -> Option<Self> {
        #[cfg(windows)]
        {
            use windows::Win32::{
                Foundation::HWND,
                UI::{
                    Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent},
                    WindowsAndMessaging::{
                        DispatchMessageW, EVENT_OBJECT_SHOW, EVENT_SYSTEM_FOREGROUND, MSG,
                        OBJID_WINDOW, PM_NOREMOVE, PM_REMOVE, PeekMessageW, TranslateMessage,
                        WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
                    },
                },
            };
            unsafe extern "system" fn callback(
                _: HWINEVENTHOOK,
                event: u32,
                hwnd: HWND,
                object: i32,
                child: i32,
                _: u32,
                _: u32,
            ) {
                if hwnd.0.is_null()
                    || (event == EVENT_OBJECT_SHOW && (object != OBJID_WINDOW.0 || child != 0))
                {
                    return;
                }
                let kind = if event == EVENT_SYSTEM_FOREGROUND {
                    WindowObservationKind::Foreground
                } else {
                    WindowObservationKind::Shown
                };
                WINDOW_EVENT_SENDER.with(|slot| {
                    let Ok(sender) = slot.try_borrow() else {
                        return;
                    };
                    let Some(sender) = sender.as_ref() else {
                        return;
                    };
                    let _ = sender.try_send(RawWindowObservation {
                        timestamp_us: super::recorder_now_us(),
                        root: hwnd.0 as usize,
                        kind,
                    });
                });
            }
            let (sender, receiver) = mpsc::sync_channel(64);
            use std::sync::atomic::Ordering;
            let (ready_tx, ready_rx) = mpsc::sync_channel(0);
            let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let worker_cancel = cancel.clone();
            let join = match thread::Builder::new()
                .name("mkmacro-window-events".into())
                .spawn(move || {
                    WINDOW_EVENT_SENDER.with(|slot| *slot.borrow_mut() = Some(sender));
                    let mut queued = MSG::default();
                    let _ = unsafe { PeekMessageW(&mut queued, None, 0, 0, PM_NOREMOVE) };
                    let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
                    let foreground = unsafe {
                        SetWinEventHook(
                            EVENT_SYSTEM_FOREGROUND,
                            EVENT_SYSTEM_FOREGROUND,
                            None,
                            Some(callback),
                            0,
                            0,
                            flags,
                        )
                    };
                    let shown = unsafe {
                        SetWinEventHook(
                            EVENT_OBJECT_SHOW,
                            EVENT_OBJECT_SHOW,
                            None,
                            Some(callback),
                            0,
                            0,
                            flags,
                        )
                    };
                    let usable = !foreground.0.is_null() || !shown.0.is_null();
                    let _ = ready_tx.send(usable);
                    if usable {
                        let mut message = MSG::default();
                        while !worker_cancel.load(Ordering::Acquire) {
                            while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }
                                .as_bool()
                            {
                                let _ = unsafe { TranslateMessage(&message) };
                                unsafe { DispatchMessageW(&message) };
                            }
                            thread::sleep(std::time::Duration::from_millis(5));
                        }
                    }
                    if !foreground.0.is_null() {
                        let _ = unsafe { UnhookWinEvent(foreground) };
                    }
                    if !shown.0.is_null() {
                        let _ = unsafe { UnhookWinEvent(shown) };
                    }
                    WINDOW_EVENT_SENDER.with(|slot| *slot.borrow_mut() = None);
                }) {
                Ok(join) => join,
                Err(_) => return None,
            };
            let Some(usable) = ready_rx
                .recv_timeout(std::time::Duration::from_secs(1))
                .ok()
            else {
                cancel.store(true, Ordering::Release);
                // SetWinEventHook has no cancellation primitive. Do not make
                // recorder start unbounded if setup itself wedges; if it later
                // returns, the cancellation flag makes the worker unhook and exit.
                drop(join);
                return None;
            };
            if !usable {
                let _ = join.join();
                return None;
            }
            Some(Self {
                receiver,
                cancel,
                join: Some(join),
            })
        }
        #[cfg(not(windows))]
        {
            None
        }
    }
    pub fn drain(&mut self) -> Vec<RawWindowObservation> {
        self.receiver.try_iter().collect()
    }
    #[cfg(windows)]
    pub fn shutdown_and_drain(&mut self) -> Vec<RawWindowObservation> {
        use std::sync::atomic::Ordering;
        self.cancel.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        self.drain()
    }
}
#[cfg(windows)]
impl WindowEventSource for NativeWindowObserver {
    fn drain(&mut self) -> Vec<RawWindowObservation> {
        NativeWindowObserver::drain(self)
    }
    fn shutdown_and_drain(&mut self) -> Vec<RawWindowObservation> {
        NativeWindowObserver::shutdown_and_drain(self)
    }
}
#[cfg(not(windows))]
impl WindowEventSource for NativeWindowObserver {
    fn drain(&mut self) -> Vec<RawWindowObservation> {
        NativeWindowObserver::drain(self)
    }
    fn shutdown_and_drain(&mut self) -> Vec<RawWindowObservation> {
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        self.drain()
    }
}
impl Drop for NativeWindowObserver {
    fn drop(&mut self) {
        #[cfg(windows)]
        if self.join.is_some() {
            let _ = self.shutdown_and_drain();
        }
        #[cfg(not(windows))]
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub started_at: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObservationBaseline {
    pub processes: HashSet<ProcessIdentity>,
    pub top_level_windows: HashSet<(usize, ProcessIdentity)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowObservationKind {
    Shown,
    Foreground,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowObservation {
    pub timestamp_us: u64,
    pub source_hint: Option<usize>,
    pub kind: WindowObservationKind,
    pub window: WindowContext,
    pub visible_top_level: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub struct SensitiveClipboardText(String);
impl SensitiveClipboardText {
    pub fn new(text: String) -> Self {
        Self(text)
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
    pub fn len(&self) -> usize {
        self.0.chars().count()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
impl fmt::Debug for SensitiveClipboardText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<clipboard text: {} chars>", self.len())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardObservation {
    pub timestamp_us: u64,
    pub text: SensitiveClipboardText,
}

/// UIA names can themselves contain document contents, so the recorder result's
/// debug representation intentionally exposes only structural metadata.
#[derive(Clone, PartialEq, Eq)]
pub struct ClickInspection {
    pub timestamp_us: u64,
    pub info: UiElementInfo,
}
impl fmt::Debug for ClickInspection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClickInspection")
            .field("timestamp_us", &self.timestamp_us)
            .field("control_type", &self.info.selector.control_type)
            .field("supported_patterns", &self.info.supported_patterns)
            .field("bounds", &self.info.bounds)
            .finish_non_exhaustive()
    }
}

pub trait ClipboardReader: Send + Sync + 'static {
    fn read_text(&self) -> anyhow::Result<Option<String>>;
}
pub trait ClickInspector: Send + Sync + 'static {
    fn inspect_at(&self, point: MkPoint) -> anyhow::Result<UiElementInfo>;
}
impl ClickInspector for super::UiaWorker {
    fn inspect_at(&self, point: MkPoint) -> anyhow::Result<UiElementInfo> {
        super::UiaWorker::inspect_at(self, point)
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }
}

struct SystemClipboard;
impl ClipboardReader for SystemClipboard {
    fn read_text(&self) -> anyhow::Result<Option<String>> {
        match arboard::Clipboard::new()?.get_text() {
            Ok(text) => Ok(Some(text)),
            Err(_) => Ok(None),
        }
    }
}
enum Job {
    Paste { timestamp_us: u64 },
    Click { timestamp_us: u64, point: MkPoint },
    Flush(mpsc::SyncSender<()>),
    Stop,
}
#[derive(Debug)]
pub enum ObservationResult {
    Clipboard(ClipboardObservation),
    UiElement(ClickInspection),
}

/// Capacity-one jobs intentionally shed enrichment under provider stalls rather
/// than allowing recording memory to grow or delaying input processing.
pub struct AuxiliaryObservationWorker {
    jobs: mpsc::SyncSender<Job>,
    results: mpsc::Receiver<ObservationResult>,
    done: mpsc::Receiver<()>,
    join: Option<JoinHandle<()>>,
}
enum InspectorProvider {
    Injected(Option<Arc<dyn ClickInspector>>),
    System,
}

#[cfg(windows)]
fn inspect_with_system_session(
    session: &mut Option<super::SystemUiaInspector>,
    point: MkPoint,
) -> anyhow::Result<UiElementInfo> {
    if session.is_none() {
        *session = Some(
            super::SystemUiaInspector::new().map_err(|error| anyhow::anyhow!(error.to_string()))?,
        );
    }
    session
        .as_mut()
        .expect("UI Automation session initialized above")
        .inspect_at(point)
        .map_err(|error| anyhow::anyhow!(error.to_string()))
}

impl AuxiliaryObservationWorker {
    pub fn production() -> Self {
        Self::spawn_inner(Some(Arc::new(SystemClipboard)), InspectorProvider::System)
    }
    pub fn spawn(
        clipboard: Option<Arc<dyn ClipboardReader>>,
        inspector: Option<Arc<dyn ClickInspector>>,
    ) -> Self {
        Self::spawn_inner(clipboard, InspectorProvider::Injected(inspector))
    }
    fn spawn_inner(
        clipboard: Option<Arc<dyn ClipboardReader>>,
        inspector: InspectorProvider,
    ) -> Self {
        let (jobs, rx) = mpsc::sync_channel(1);
        let (tx, results) = mpsc::sync_channel(16);
        let (done_tx, done) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("mkmacro-recorder-observations".into())
            .spawn(move || {
                let mut inspector_enabled = true;
                #[cfg(windows)]
                let mut system_inspector = None;
                while let Ok(job) = rx.recv() {
                    match job {
                        Job::Paste { timestamp_us } => {
                            if let Some(reader) = &clipboard
                                && let Ok(Some(text)) = reader.read_text()
                                && !text.is_empty()
                            {
                                let _ = tx.try_send(ObservationResult::Clipboard(
                                    ClipboardObservation {
                                        timestamp_us,
                                        text: SensitiveClipboardText::new(text),
                                    },
                                ));
                            }
                        }
                        Job::Click {
                            timestamp_us,
                            point,
                        } => {
                            if inspector_enabled {
                                let started = std::time::Instant::now();
                                let inspected = match &inspector {
                                    InspectorProvider::Injected(Some(inspector)) => {
                                        inspector.inspect_at(point)
                                    }
                                    InspectorProvider::Injected(None) => {
                                        Err(anyhow::anyhow!("UIA inspection is disabled"))
                                    }
                                    InspectorProvider::System => {
                                        #[cfg(windows)]
                                        {
                                            inspect_with_system_session(
                                                &mut system_inspector,
                                                point,
                                            )
                                        }
                                        #[cfg(not(windows))]
                                        {
                                            super::inspect_at_system(point)
                                                .map_err(|error| anyhow::anyhow!(error.to_string()))
                                        }
                                    }
                                };
                                if started.elapsed() >= std::time::Duration::from_millis(750) {
                                    // A provider that exceeds the native UIA timeout is not
                                    // invoked again during this recording session.
                                    inspector_enabled = false;
                                }
                                if let Ok(info) = inspected {
                                    let _ = tx.try_send(ObservationResult::UiElement(
                                        ClickInspection { timestamp_us, info },
                                    ));
                                }
                            }
                        }
                        Job::Stop => break,
                        Job::Flush(reply) => {
                            let _ = reply.send(());
                        }
                    }
                }
                let _ = done_tx.send(());
            })
            .expect("spawn recorder observation worker");
        Self {
            jobs,
            results,
            done,
            join: Some(join),
        }
    }
    pub fn observe_paste(&self, timestamp_us: u64) {
        let _ = self.jobs.try_send(Job::Paste { timestamp_us });
    }
    pub fn inspect_click(&self, timestamp_us: u64, point: MkPoint) {
        let _ = self.jobs.try_send(Job::Click {
            timestamp_us,
            point,
        });
    }
    pub fn drain(&self) -> Vec<ObservationResult> {
        self.results.try_iter().collect()
    }
    pub fn flush(&self, timeout: std::time::Duration) -> bool {
        let (tx, rx) = mpsc::sync_channel(0);
        let deadline = std::time::Instant::now() + timeout;
        let mut job = Job::Flush(tx);
        loop {
            match self.jobs.try_send(job) {
                Ok(()) => {
                    return rx
                        .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                        .is_ok();
                }
                Err(mpsc::TrySendError::Full(returned)) if std::time::Instant::now() < deadline => {
                    job = returned;
                    thread::sleep(std::time::Duration::from_millis(1));
                }
                Err(_) => return false,
            }
        }
    }
    pub fn finish(&mut self, timeout: std::time::Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        if !self.flush(timeout) {
            self.join.take();
            return false;
        }
        if self.jobs.send(Job::Stop).is_err() {
            self.join.take();
            return false;
        }
        let completed = self
            .done
            .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            .is_ok();
        if completed {
            if let Some(join) = self.join.take() {
                let _ = join.join();
            }
        } else {
            self.join.take();
        }
        completed
    }
}
impl Drop for AuxiliaryObservationWorker {
    fn drop(&mut self) {
        if self.join.is_some() {
            let _ = self.jobs.try_send(Job::Stop);
            // A stalled provider is not joined: dropping the handle detaches the bounded worker.
            self.join.take();
        }
    }
}

pub fn capture_process_baseline() -> ObservationBaseline {
    use sysinfo::System;
    let system = System::new_all();
    let processes: HashSet<_> = system
        .processes()
        .iter()
        .map(|(pid, process)| ProcessIdentity {
            pid: pid.as_u32(),
            started_at: process.start_time(),
        })
        .collect();
    let top_level_windows = baseline_top_level_windows()
        .into_iter()
        .filter_map(|(root, pid)| {
            processes
                .iter()
                .find(|process| process.pid == pid)
                .copied()
                .map(|process| (root, process))
        })
        .collect();
    ObservationBaseline {
        processes,
        top_level_windows,
    }
}

#[cfg(windows)]
fn baseline_top_level_windows() -> HashSet<(usize, u32)> {
    use windows::Win32::{
        Foundation::{BOOL, HWND, LPARAM},
        UI::WindowsAndMessaging::{
            EnumWindows, GA_ROOT, GWL_EXSTYLE, GetAncestor, GetWindowLongW,
            GetWindowThreadProcessId, IsWindowVisible, WS_EX_TOOLWINDOW,
        },
    };
    unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> BOOL {
        let top_level = unsafe { GetAncestor(hwnd, GA_ROOT) } == hwnd;
        let not_tool =
            unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32 & WS_EX_TOOLWINDOW.0 == 0;
        if unsafe { IsWindowVisible(hwnd) }.as_bool() && top_level && not_tool {
            let windows = unsafe { &mut *(data.0 as *mut HashSet<(usize, u32)>) };
            let mut pid = 0;
            unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
            if pid != 0 {
                windows.insert((hwnd.0 as usize, pid));
            }
        }
        true.into()
    }
    let mut windows = HashSet::new();
    let _ = unsafe { EnumWindows(Some(collect), LPARAM(&mut windows as *mut _ as isize)) };
    windows
}
#[cfg(not(windows))]
fn baseline_top_level_windows() -> HashSet<(usize, u32)> {
    HashSet::new()
}

#[cfg(windows)]
fn process_start_time(pid: u32) -> Option<u64> {
    use windows::Win32::{
        Foundation::{CloseHandle, FILETIME},
        System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let result =
        unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) };
    let _ = unsafe { CloseHandle(handle) };
    result.ok()?;
    let ticks = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
    // FILETIME is 100ns intervals since 1601; sysinfo baseline identities are
    // Unix seconds, so retain the same domain for stable comparisons.
    ticks
        .checked_div(10_000_000)
        .and_then(|seconds| seconds.checked_sub(11_644_473_600))
}

#[cfg(not(windows))]
fn process_start_time(_pid: u32) -> Option<u64> {
    None
}

/// Session-scoped correlation state. Its production window feed currently uses
/// input-correlated foreground/show discovery; the pure observation model also
/// accepts native WinEvent observations when that adapter is available.
pub struct RecorderObserverSession {
    pub baseline: ObservationBaseline,
    auxiliary: AuxiliaryObservationWorker,
    click_auxiliary: Option<AuxiliaryObservationWorker>,
    native: Option<Box<dyn WindowEventSource>>,
    process_starts: HashMap<u32, u64>,
    recent_shows: HashMap<(usize, u32, u64), u64>,
    foreground: Option<(usize, u32, u64)>,
    mouse_down: Option<(super::MouseButton, i32, i32, u64)>,
    pub windows: Vec<WindowObservation>,
    pub clipboards: Vec<ClipboardObservation>,
    pub inspections: Vec<ClickInspection>,
}
impl RecorderObserverSession {
    pub fn production() -> Self {
        let mut session = Self::with_parts(
            capture_process_baseline(),
            AuxiliaryObservationWorker::spawn(Some(Arc::new(SystemClipboard)), None),
        );
        // UIA has its own bounded lane so a provider stall cannot block clipboard
        // capture or its finish barrier.
        session.click_auxiliary = Some(AuxiliaryObservationWorker::spawn_inner(
            None,
            InspectorProvider::System,
        ));
        session.native = NativeWindowObserver::start()
            .map(|source| Box::new(source) as Box<dyn WindowEventSource>);
        session
    }
    pub fn with_parts(
        baseline: ObservationBaseline,
        auxiliary: AuxiliaryObservationWorker,
    ) -> Self {
        let process_starts = baseline
            .processes
            .iter()
            .map(|process| (process.pid, process.started_at))
            .collect();
        Self {
            baseline,
            auxiliary,
            click_auxiliary: None,
            native: None,
            process_starts,
            recent_shows: HashMap::new(),
            foreground: None,
            mouse_down: None,
            windows: Vec::new(),
            clipboards: Vec::new(),
            inspections: Vec::new(),
        }
    }
    pub fn with_sources(
        baseline: ObservationBaseline,
        auxiliary: AuxiliaryObservationWorker,
        native: Option<Box<dyn WindowEventSource>>,
    ) -> Self {
        let mut session = Self::with_parts(baseline, auxiliary);
        session.native = native;
        session
    }
    pub fn observe(
        &mut self,
        event: &HookEvent,
        context: Option<&EventContext>,
        inspect_clicks: bool,
        record_mouse_buttons: bool,
        capture_paste: bool,
        control_down: bool,
        alt_down: bool,
        click_max_ms: u64,
        click_distance_px: i32,
    ) {
        let timestamp_us = event.timestamp_us();
        if let Some(context) = context {
            if let Some(root) = context.foreground.native_root_id {
                let mut window = context.foreground.clone();
                self.fill_known_start(&mut window);
                let identity = (
                    root,
                    window.process_id.unwrap_or(0),
                    window.process_started_at.unwrap_or(0),
                );
                if self.foreground.replace(identity) != Some(identity) {
                    self.windows.push(WindowObservation {
                        timestamp_us,
                        source_hint: None,
                        kind: WindowObservationKind::Foreground,
                        window,
                        visible_top_level: true,
                    });
                }
            }
        }
        match *event {
            HookEvent::Key {
                transition: KeyTransition::Down,
                vk,
                ..
            } if capture_paste && control_down && !alt_down && vk == 0x56 => {
                self.auxiliary.observe_paste(timestamp_us)
            }
            HookEvent::Mouse {
                message: MouseMessage::Down(button),
                x,
                y,
                ..
            } => self.mouse_down = Some((button, x, y, timestamp_us)),
            HookEvent::Mouse {
                message: MouseMessage::Up(button),
                x,
                y,
                ..
            } if inspect_clicks && record_mouse_buttons && button == super::MouseButton::Left => {
                if let Some((down_button, down_x, down_y, down_at)) = self.mouse_down.take()
                    && down_button == button
                    && timestamp_us.saturating_sub(down_at) <= click_max_ms.saturating_mul(1000)
                    && i32::abs_diff(x, down_x).max(i32::abs_diff(y, down_y))
                        <= click_distance_px.max(0) as u32
                {
                    self.click_auxiliary
                        .as_ref()
                        .unwrap_or(&self.auxiliary)
                        .inspect_click(timestamp_us, MkPoint { x, y });
                }
            }
            _ => {}
        }
        self.drain();
    }
    fn fill_known_start(&mut self, window: &mut WindowContext) {
        if window.process_started_at.is_none()
            && let Some(pid) = window.process_id
        {
            if let Some(start) = self.process_starts.get(&pid).copied() {
                window.process_started_at = Some(start);
            } else if let Some(start) = process_start_time(pid) {
                self.process_starts.insert(pid, start);
                window.process_started_at = Some(start);
            }
        }
    }
    pub fn drain_native(&mut self, enricher: &mut dyn super::EventEnricher) {
        let events = self
            .native
            .as_mut()
            .map(|source| source.drain())
            .unwrap_or_default();
        self.record_native(events, enricher);
    }
    fn record_native(
        &mut self,
        events: Vec<RawWindowObservation>,
        enricher: &mut dyn super::EventEnricher,
    ) {
        for event in events {
            if event.kind == WindowObservationKind::Shown
                && !enricher.is_recordable_top_level(event.root)
            {
                continue;
            }
            if event.kind == WindowObservationKind::Shown {
                enricher.invalidate_root(event.root);
            }
            let Some(mut window) = enricher.context_for_root(event.root) else {
                continue;
            };
            self.fill_known_start(&mut window);
            let identity = (
                event.root,
                window.process_id.unwrap_or(0),
                window.process_started_at.unwrap_or(0),
            );
            if event.kind == WindowObservationKind::Shown {
                const SHOW_DEDUPE_US: u64 = 100_000;
                if self.recent_shows.get(&identity).is_some_and(|previous| {
                    event.timestamp_us.saturating_sub(*previous) <= SHOW_DEDUPE_US
                }) {
                    continue;
                }
                self.recent_shows.insert(identity, event.timestamp_us);
                self.recent_shows.retain(|_, timestamp| {
                    event.timestamp_us.saturating_sub(*timestamp) <= SHOW_DEDUPE_US
                });
            }
            if event.kind == WindowObservationKind::Foreground {
                self.foreground = Some(identity);
            }
            self.windows.push(WindowObservation {
                timestamp_us: event.timestamp_us,
                // Native notifications are correlated against normalized sources by
                // capture timestamp once the literal plan exists.
                source_hint: None,
                kind: event.kind,
                window,
                visible_top_level: true,
            });
        }
    }
    pub fn drain(&mut self) {
        for result in self.auxiliary.drain() {
            match result {
                ObservationResult::Clipboard(x) => self.clipboards.push(x),
                ObservationResult::UiElement(x) => self.inspections.push(x),
            }
        }
        if let Some(click_auxiliary) = &self.click_auxiliary {
            for result in click_auxiliary.drain() {
                match result {
                    ObservationResult::Clipboard(x) => self.clipboards.push(x),
                    ObservationResult::UiElement(x) => self.inspections.push(x),
                }
            }
        }
    }
    pub fn retime(&mut self, mut normalize: impl FnMut(u64) -> u64) {
        for observation in &mut self.windows {
            observation.timestamp_us = normalize(observation.timestamp_us);
        }
        for observation in &mut self.clipboards {
            observation.timestamp_us = normalize(observation.timestamp_us);
        }
        for observation in &mut self.inspections {
            observation.timestamp_us = normalize(observation.timestamp_us);
        }
    }
    pub fn retain_active(&mut self, mut active: impl FnMut(u64) -> bool) {
        self.windows
            .retain(|observation| active(observation.timestamp_us));
        self.clipboards
            .retain(|observation| active(observation.timestamp_us));
        self.inspections
            .retain(|inspection| active(inspection.timestamp_us));
    }
    pub fn finish(&mut self, enricher: &mut dyn super::EventEnricher) {
        if let Some(mut native) = self.native.take() {
            let events = native.shutdown_and_drain();
            self.record_native(events, enricher);
        }
        let _ = self
            .auxiliary
            .finish(std::time::Duration::from_millis(1_250));
        if let Some(click_auxiliary) = &mut self.click_auxiliary {
            let _ = click_auxiliary.finish(std::time::Duration::from_millis(1_250));
        }
        self.drain();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FixedClipboard {
        calls: Arc<AtomicUsize>,
        value: Option<&'static str>,
        fail: bool,
    }
    impl ClipboardReader for FixedClipboard {
        fn read_text(&self) -> anyhow::Result<Option<String>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                anyhow::bail!("unavailable")
            }
            Ok(self.value.map(str::to_owned))
        }
    }

    fn ui_info() -> UiElementInfo {
        UiElementInfo {
            selector: super::super::MkUiSelector {
                automation_id: Some("save".into()),
                name: Some("Save".into()),
                class_name: None,
                control_type: Some(super::super::MkUiControlType::Button),
                framework_id: None,
                ancestor_path: Vec::new(),
            },
            user_facing_name: "Save".into(),
            target_executable: "app.exe".into(),
            supported_patterns: HashSet::new(),
            bounds: None,
        }
    }

    struct CountingInspector(Arc<AtomicUsize>);
    impl ClickInspector for CountingInspector {
        fn inspect_at(&self, _: MkPoint) -> anyhow::Result<UiElementInfo> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(ui_info())
        }
    }
    struct FailingInspector;
    impl ClickInspector for FailingInspector {
        fn inspect_at(&self, _: MkPoint) -> anyhow::Result<UiElementInfo> {
            anyhow::bail!("provider unavailable")
        }
    }
    struct SlowInspector(Arc<AtomicUsize>);
    impl ClickInspector for SlowInspector {
        fn inspect_at(&self, _: MkPoint) -> anyhow::Result<UiElementInfo> {
            self.0.fetch_add(1, Ordering::SeqCst);
            thread::sleep(std::time::Duration::from_millis(760));
            anyhow::bail!("provider timed out")
        }
    }
    struct FakeWindowSource(Vec<RawWindowObservation>);
    impl WindowEventSource for FakeWindowSource {
        fn drain(&mut self) -> Vec<RawWindowObservation> {
            Vec::new()
        }
        fn shutdown_and_drain(&mut self) -> Vec<RawWindowObservation> {
            std::mem::take(&mut self.0)
        }
    }
    struct FakeEnricher(bool);
    impl super::super::EventEnricher for FakeEnricher {
        fn enrich(&mut self, _: &HookEvent) -> Option<EventContext> {
            None
        }
        fn context_for_root(&mut self, root: usize) -> Option<WindowContext> {
            Some(WindowContext {
                executable: "app.exe".into(),
                title: "App".into(),
                native_root_id: Some(root),
                process_id: Some(7),
                process_started_at: Some(11),
                ..Default::default()
            })
        }
        fn is_recordable_top_level(&self, _: usize) -> bool {
            self.0
        }
    }

    #[test]
    fn clipboard_debug_is_redacted() {
        let value = SensitiveClipboardText::new("do not leak this".into());
        let debug = format!("{value:?}");
        assert!(!debug.contains("do not leak"));
        assert_eq!(debug, "<clipboard text: 16 chars>");
    }

    #[test]
    fn clipboard_none_and_failure_are_best_effort_and_success_stays_transient() {
        for (value, fail, expected) in
            [(None, false, 0), (Some("paste"), false, 1), (None, true, 0)]
        {
            let calls = Arc::new(AtomicUsize::new(0));
            let mut worker = AuxiliaryObservationWorker::spawn(
                Some(Arc::new(FixedClipboard {
                    calls: calls.clone(),
                    value,
                    fail,
                })),
                None,
            );
            worker.observe_paste(10);
            assert!(worker.finish(std::time::Duration::from_secs(1)));
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            assert_eq!(worker.drain().len(), expected);
        }
    }

    #[test]
    fn only_completed_recorded_left_clicks_schedule_uia() {
        let calls = Arc::new(AtomicUsize::new(0));
        let worker = AuxiliaryObservationWorker::spawn(
            None,
            Some(Arc::new(CountingInspector(calls.clone()))),
        );
        let mut session =
            RecorderObserverSession::with_parts(ObservationBaseline::default(), worker);
        let mouse = |timestamp_us, message, x, y| HookEvent::Mouse {
            timestamp_us,
            message,
            x,
            y,
            flags: 0,
            extra_info: 0,
        };
        session.observe(
            &mouse(1, MouseMessage::Move, 4, 5),
            None,
            true,
            true,
            false,
            false,
            false,
            250,
            4,
        );
        session.observe(
            &mouse(2, MouseMessage::Down(super::super::MouseButton::Left), 4, 5),
            None,
            true,
            true,
            false,
            false,
            false,
            250,
            4,
        );
        session.observe(
            &mouse(3, MouseMessage::Up(super::super::MouseButton::Left), 4, 5),
            None,
            true,
            true,
            false,
            false,
            false,
            250,
            4,
        );
        assert!(session.auxiliary.finish(std::time::Duration::from_secs(1)));
        session.drain();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(session.inspections.len(), 1);
    }

    #[test]
    fn uia_failure_is_best_effort_and_slow_provider_circuit_breaks() {
        let mut failing = AuxiliaryObservationWorker::spawn(None, Some(Arc::new(FailingInspector)));
        failing.inspect_click(1, MkPoint { x: 1, y: 1 });
        assert!(failing.finish(std::time::Duration::from_secs(1)));
        assert!(failing.drain().is_empty());

        let calls = Arc::new(AtomicUsize::new(0));
        let mut slow =
            AuxiliaryObservationWorker::spawn(None, Some(Arc::new(SlowInspector(calls.clone()))));
        slow.inspect_click(1, MkPoint { x: 1, y: 1 });
        assert!(slow.flush(std::time::Duration::from_secs(2)));
        slow.inspect_click(2, MkPoint { x: 2, y: 2 });
        assert!(slow.finish(std::time::Duration::from_secs(1)));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(slow.drain().is_empty());
    }

    #[test]
    fn native_source_is_final_drained_and_show_requires_top_level_eligibility() {
        for (eligible, expected) in [(false, 0), (true, 1)] {
            let worker = AuxiliaryObservationWorker::spawn(None, None);
            let source = FakeWindowSource(vec![RawWindowObservation {
                timestamp_us: 50,
                root: 9,
                kind: WindowObservationKind::Shown,
            }]);
            let mut session = RecorderObserverSession::with_sources(
                ObservationBaseline::default(),
                worker,
                Some(Box::new(source)),
            );
            session.finish(&mut FakeEnricher(eligible));
            assert_eq!(session.windows.len(), expected);
        }
    }

    #[test]
    fn show_dedupe_is_short_lived_so_reused_handles_are_observed() {
        let worker = AuxiliaryObservationWorker::spawn(None, None);
        let mut session =
            RecorderObserverSession::with_parts(ObservationBaseline::default(), worker);
        session.record_native(
            vec![
                RawWindowObservation {
                    timestamp_us: 50,
                    root: 9,
                    kind: WindowObservationKind::Shown,
                },
                RawWindowObservation {
                    timestamp_us: 60,
                    root: 9,
                    kind: WindowObservationKind::Shown,
                },
                RawWindowObservation {
                    timestamp_us: 200_000,
                    root: 9,
                    kind: WindowObservationKind::Shown,
                },
            ],
            &mut FakeEnricher(true),
        );
        assert_eq!(session.windows.len(), 2);
    }
}
