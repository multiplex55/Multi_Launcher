//! Low-level hook transport. Hook callbacks are deliberately limited to copying POD data,
//! reading the monotonic clock, a `try_send`, and chaining the hook.
use crate::mkmacro::input::MKMACRO_EXTRA_INFO;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::Instant,
};

static RECORDER_EPOCH: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// Shared monotonic timestamp domain for hook events and recorder boundaries.
pub fn recorder_now_us() -> u64 {
    RECORDER_EPOCH
        .get_or_init(Instant::now)
        .elapsed()
        .as_micros() as u64
}

pub const LLKHF_EXTENDED: u32 = 0x01;
pub const LLKHF_INJECTED: u32 = 0x10;
pub const LLMHF_INJECTED: u32 = 0x01;

#[cfg(any(windows, test))]
fn clear_hook_pair<T>(
    keyboard_hook: &mut Option<T>,
    mouse_hook: &mut Option<T>,
    uninstall: &mut impl FnMut(T),
) {
    if let Some(handle) = keyboard_hook.take() {
        uninstall(handle);
    }
    if let Some(handle) = mouse_hook.take() {
        uninstall(handle);
    }
}

/// Retain hook handles only when the install transaction produced a complete pair.
/// A partial pair cannot capture a session correctly and must not survive into a retry.
#[cfg(any(windows, test))]
fn retain_complete_hook_pair<T>(
    keyboard_hook: &mut Option<T>,
    mouse_hook: &mut Option<T>,
    uninstall: &mut impl FnMut(T),
) -> bool {
    if keyboard_hook.is_some() && mouse_hook.is_some() {
        true
    } else {
        clear_hook_pair(keyboard_hook, mouse_hook, uninstall);
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyTransition {
    Down,
    Up,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseMessage {
    Move,
    Down(MouseButton),
    Up(MouseButton),
    Wheel(i32),
    HorizontalWheel(i32),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    X1,
    X2,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    Key {
        timestamp_us: u64,
        transition: KeyTransition,
        vk: u32,
        scan_code: u32,
        flags: u32,
        extra_info: usize,
    },
    Mouse {
        timestamp_us: u64,
        message: MouseMessage,
        x: i32,
        y: i32,
        flags: u32,
        extra_info: usize,
    },
}
impl HookEvent {
    pub fn timestamp_us(&self) -> u64 {
        match self {
            Self::Key { timestamp_us, .. } | Self::Mouse { timestamp_us, .. } => *timestamp_us,
        }
    }
    pub fn is_injected(&self) -> bool {
        match self {
            Self::Key { flags, .. } => flags & LLKHF_INJECTED != 0,
            Self::Mouse { flags, .. } => flags & LLMHF_INJECTED != 0,
        }
    }
    pub fn extra_info(&self) -> usize {
        match self {
            Self::Key { extra_info, .. } | Self::Mouse { extra_info, .. } => *extra_info,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookCommand {
    Start,
    Pause,
    Resume,
    Fence,
    Stop,
    Shutdown,
}
fn requires_pre_boundary_drain(command: HookCommand) -> bool {
    matches!(
        command,
        HookCommand::Pause | HookCommand::Fence | HookCommand::Stop | HookCommand::Shutdown
    )
}
pub struct HookCommandRequest {
    pub command: HookCommand,
    acknowledgement: mpsc::SyncSender<bool>,
}
impl HookCommandRequest {
    pub fn acknowledge(self, succeeded: bool) {
        let _ = self.acknowledgement.send(succeeded);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SequencedHookEvent {
    pub sequence: u64,
    pub event: HookEvent,
}
pub trait HookLoopAdapter: Send + 'static {
    /// Runs on the hook thread. Implementations install/uninstall hooks and own their message loop.
    fn run(self, commands: mpsc::Receiver<HookCommandRequest>, callback: CallbackSender);
}

#[derive(Clone)]
pub struct CallbackSender {
    tx: SyncSender<SequencedHookEvent>,
    next_sequence: Arc<AtomicU64>,
    dropped: Arc<AtomicU64>,
    record_injected: Arc<AtomicBool>,
}
impl CallbackSender {
    /// Bounded and nonblocking; safe for direct use by a hook callback.
    pub fn submit(&self, event: HookEvent) {
        // Do the cheap, immutable filtering in the callback, before an event can consume
        // queue capacity.  Everything else belongs to the recorder worker.
        if !should_record(&event, self.record_injected.load(Ordering::Relaxed)) {
            return;
        }
        let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        if let Err(TrySendError::Full(_)) = self.tx.try_send(SequencedHookEvent { sequence, event })
        {
            self.dropped
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

pub struct HookService {
    commands: mpsc::Sender<HookCommandRequest>,
    events: Mutex<Option<mpsc::Receiver<SequencedHookEvent>>>,
    next_sequence: Arc<AtomicU64>,
    dropped: Arc<std::sync::atomic::AtomicU64>,
    record_injected: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
}
impl HookService {
    pub fn with_adapter<A: HookLoopAdapter>(adapter: A, capacity: usize) -> Self {
        let (commands, rx) = mpsc::channel();
        let (tx, events) = mpsc::sync_channel(capacity.max(1));
        let dropped = Arc::new(AtomicU64::new(0));
        let next_sequence = Arc::new(AtomicU64::new(0));
        let record_injected = Arc::new(AtomicBool::new(false));
        let callback = CallbackSender {
            tx,
            next_sequence: next_sequence.clone(),
            dropped: dropped.clone(),
            record_injected: record_injected.clone(),
        };
        let thread = thread::Builder::new()
            .name("mkmacro-hooks".into())
            .spawn(move || adapter.run(rx, callback))
            .expect("spawn hook loop");
        Self {
            commands,
            events: Mutex::new(Some(events)),
            next_sequence,
            dropped,
            record_injected,
            thread: Mutex::new(Some(thread)),
        }
    }
    pub fn command(&self, command: HookCommand) -> bool {
        let (acknowledgement, acknowledged) = mpsc::sync_channel(0);
        self.commands
            .send(HookCommandRequest {
                command,
                acknowledgement,
            })
            .is_ok()
            && acknowledged.recv().unwrap_or(false)
    }
    pub fn start(&self) -> bool {
        self.command(HookCommand::Start)
    }
    /// Sets callback filtering for the next session. Call before `start`.
    pub fn set_record_injected_input(&self, enabled: bool) {
        self.record_injected.store(enabled, Ordering::Relaxed);
    }
    pub fn pause(&self) -> bool {
        self.command(HookCommand::Pause)
    }
    pub fn resume(&self) -> bool {
        self.command(HookCommand::Resume)
    }
    pub fn stop(&self) -> bool {
        self.command(HookCommand::Stop)
    }
    pub fn synchronize(&self) -> Option<u64> {
        self.command(HookCommand::Fence).then(|| self.fence())
    }
    pub fn take_events(&self) -> Option<mpsc::Receiver<SequencedHookEvent>> {
        self.events.lock().unwrap().take()
    }
    /// The first sequence that cannot have been submitted before the acknowledged transition.
    pub fn fence(&self) -> u64 {
        self.next_sequence.load(Ordering::Acquire)
    }
    pub fn dropped_events(&self) -> u64 {
        self.dropped.load(std::sync::atomic::Ordering::Relaxed)
    }
    pub fn shutdown(&self) {
        let _ = self.command(HookCommand::Shutdown);
        if let Some(t) = self.thread.lock().unwrap().take() {
            let _ = t.join();
        }
    }
}
impl Drop for HookService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub fn should_record(event: &HookEvent, record_injected_input: bool) -> bool {
    event.extra_info() != MKMACRO_EXTRA_INFO && (record_injected_input || !event.is_injected())
}

/// Portable no-op adapter; the Windows implementation is selected by the application on Windows.
pub struct NoopHookLoop;
impl HookLoopAdapter for NoopHookLoop {
    fn run(self, commands: mpsc::Receiver<HookCommandRequest>, _: CallbackSender) {
        while let Ok(request) = commands.recv() {
            let shutdown = request.command == HookCommand::Shutdown;
            request.acknowledge(true);
            if shutdown {
                break;
            }
        }
    }
}

/// Application hook transport. Tests construct `HookService` with their own adapter instead.
pub fn production_hook_service(capacity: usize) -> HookService {
    #[cfg(windows)]
    {
        HookService::with_adapter(WindowsHookLoop, capacity)
    }
    #[cfg(not(windows))]
    {
        HookService::with_adapter(NoopHookLoop, capacity)
    }
}

#[cfg(windows)]
mod win32 {
    use super::*;
    use std::{sync::OnceLock, time::Duration};
    use windows::Win32::{
        Foundation::{LPARAM, LRESULT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::*,
    };
    static CALLBACK: OnceLock<Mutex<Option<CallbackSender>>> = OnceLock::new();
    fn now() -> u64 {
        super::recorder_now_us()
    }
    unsafe extern "system" fn keyboard(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
        if code >= 0 {
            // SAFETY: Windows supplies a valid KBDLLHOOKSTRUCT pointer for non-negative
            // low-level keyboard hook codes, and the value is copied before returning.
            let d = unsafe { &*(l.0 as *const KBDLLHOOKSTRUCT) };
            let transition = if w.0 as u32 == WM_KEYUP || w.0 as u32 == WM_SYSKEYUP {
                KeyTransition::Up
            } else {
                KeyTransition::Down
            };
            if let Some(tx) = CALLBACK
                .get()
                .and_then(|x| x.lock().ok())
                .and_then(|x| x.clone())
            {
                tx.submit(HookEvent::Key {
                    timestamp_us: now(),
                    transition,
                    vk: d.vkCode,
                    scan_code: d.scanCode,
                    flags: d.flags.0,
                    extra_info: d.dwExtraInfo,
                });
            }
        }
        // SAFETY: Forward the hook parameters unchanged as required by the hook contract.
        unsafe { CallNextHookEx(None, code, w, l) }
    }
    unsafe extern "system" fn mouse(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
        if code >= 0 {
            // SAFETY: Windows supplies a valid MSLLHOOKSTRUCT pointer for non-negative
            // low-level mouse hook codes, and the value is copied before returning.
            let d = unsafe { &*(l.0 as *const MSLLHOOKSTRUCT) };
            let hi = ((d.mouseData >> 16) & 0xffff) as u16;
            let delta = hi as i16 as i32;
            let message = match w.0 as u32 {
                WM_MOUSEMOVE => Some(MouseMessage::Move),
                WM_LBUTTONDOWN => Some(MouseMessage::Down(MouseButton::Left)),
                WM_LBUTTONUP => Some(MouseMessage::Up(MouseButton::Left)),
                WM_RBUTTONDOWN => Some(MouseMessage::Down(MouseButton::Right)),
                WM_RBUTTONUP => Some(MouseMessage::Up(MouseButton::Right)),
                WM_MBUTTONDOWN => Some(MouseMessage::Down(MouseButton::Middle)),
                WM_MBUTTONUP => Some(MouseMessage::Up(MouseButton::Middle)),
                WM_XBUTTONDOWN => Some(MouseMessage::Down(if hi == 1 {
                    MouseButton::X1
                } else {
                    MouseButton::X2
                })),
                WM_XBUTTONUP => Some(MouseMessage::Up(if hi == 1 {
                    MouseButton::X1
                } else {
                    MouseButton::X2
                })),
                WM_MOUSEWHEEL => Some(MouseMessage::Wheel(delta)),
                WM_MOUSEHWHEEL => Some(MouseMessage::HorizontalWheel(delta)),
                _ => None,
            };
            if let (Some(message), Some(tx)) = (
                message,
                CALLBACK
                    .get()
                    .and_then(|x| x.lock().ok())
                    .and_then(|x| x.clone()),
            ) {
                tx.submit(HookEvent::Mouse {
                    timestamp_us: now(),
                    message,
                    x: d.pt.x,
                    y: d.pt.y,
                    flags: d.flags,
                    extra_info: d.dwExtraInfo,
                });
            }
        }
        // SAFETY: Forward the hook parameters unchanged as required by the hook contract.
        unsafe { CallNextHookEx(None, code, w, l) }
    }
    /// Dedicated low-level-hook thread. Commands are polled beside its Windows message queue;
    /// shutdown always unhooks both handles before returning and allowing the owner to join.
    pub struct WindowsHookLoop;
    unsafe fn drain_pending_messages() {
        let mut msg = MSG::default();
        while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
            let _ = unsafe { TranslateMessage(&msg) };
            unsafe { DispatchMessageW(&msg) };
        }
    }
    impl HookLoopAdapter for WindowsHookLoop {
        fn run(self, commands: mpsc::Receiver<HookCommandRequest>, callback: CallbackSender) {
            unsafe {
                *CALLBACK.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(callback.clone());
                // HMODULE implements the windows crate's HINSTANCE parameter conversion.
                // Passing it directly also avoids wrapping the parameter in Option, which
                // SetWindowsHookExW does not accept in windows 0.58.
                let module = GetModuleHandleW(None).unwrap_or_default();
                let mut keyboard_hook = None;
                let mut mouse_hook = None;
                let mut enabled = false;
                'outer: loop {
                    while let Ok(request) = commands.try_recv() {
                        // Low-level hook notifications already in this thread's
                        // queue precede the acknowledged lifecycle boundary.
                        // Dispatch them while CALLBACK is still installed so
                        // the returned fence includes the complete pre-boundary tail.
                        if enabled && requires_pre_boundary_drain(request.command) {
                            drain_pending_messages();
                        }
                        let mut succeeded = true;
                        match request.command {
                            HookCommand::Start => {
                                if keyboard_hook.is_none() || mouse_hook.is_none() {
                                    let mut unhook = |handle| {
                                        let _ = UnhookWindowsHookEx(handle);
                                    };
                                    clear_hook_pair(
                                        &mut keyboard_hook,
                                        &mut mouse_hook,
                                        &mut unhook,
                                    );
                                    keyboard_hook = SetWindowsHookExW(
                                        WH_KEYBOARD_LL,
                                        Some(keyboard),
                                        module,
                                        0,
                                    )
                                    .ok();
                                    mouse_hook =
                                        SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), module, 0).ok();
                                }
                                succeeded = retain_complete_hook_pair(
                                    &mut keyboard_hook,
                                    &mut mouse_hook,
                                    &mut |handle| {
                                        let _ = UnhookWindowsHookEx(handle);
                                    },
                                );
                                enabled = succeeded;
                                *CALLBACK.get().unwrap().lock().unwrap() =
                                    succeeded.then(|| callback.clone());
                            }
                            HookCommand::Pause | HookCommand::Stop => {
                                enabled = false;
                                *CALLBACK.get().unwrap().lock().unwrap() = None;
                            }
                            HookCommand::Resume => {
                                succeeded = keyboard_hook.is_some() && mouse_hook.is_some();
                                enabled = succeeded;
                                *CALLBACK.get().unwrap().lock().unwrap() =
                                    succeeded.then(|| callback.clone());
                            }
                            HookCommand::Fence => {}
                            HookCommand::Shutdown => {
                                enabled = false;
                                *CALLBACK.get().unwrap().lock().unwrap() = None;
                            }
                        }
                        let shutdown = request.command == HookCommand::Shutdown;
                        request.acknowledge(succeeded);
                        if shutdown {
                            break 'outer;
                        }
                    }
                    if !enabled {
                        *CALLBACK.get().unwrap().lock().unwrap() = None;
                    } // paused callbacks still chain, but do not enqueue
                    drain_pending_messages();
                    std::thread::sleep(Duration::from_millis(2));
                    if enabled && CALLBACK.get().unwrap().lock().unwrap().is_none() {
                        *CALLBACK.get().unwrap().lock().unwrap() = Some(callback.clone());
                    }
                }
                *CALLBACK.get().unwrap().lock().unwrap() = None;
                clear_hook_pair(&mut keyboard_hook, &mut mouse_hook, &mut |handle| {
                    let _ = UnhookWindowsHookEx(handle);
                });
            }
        }
    }
}
#[cfg(windows)]
pub use win32::WindowsHookLoop;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn partial_hook_install_is_rolled_back_before_clean_retry_and_teardown() {
        for (keyboard, mouse, expected_uninstalled) in
            [(Some(11), None, vec![11]), (None, Some(12), vec![12])]
        {
            let mut keyboard = keyboard;
            let mut mouse = mouse;
            let mut uninstalled = Vec::new();
            assert!(!retain_complete_hook_pair(
                &mut keyboard,
                &mut mouse,
                &mut |handle| uninstalled.push(handle),
            ));
            assert_eq!(uninstalled, expected_uninstalled);
            assert!(keyboard.is_none() && mouse.is_none());

            keyboard = Some(21);
            mouse = Some(22);
            assert!(retain_complete_hook_pair(
                &mut keyboard,
                &mut mouse,
                &mut |handle| uninstalled.push(handle),
            ));
            clear_hook_pair(&mut keyboard, &mut mouse, &mut |handle| {
                uninstalled.push(handle)
            });
            assert_eq!(&uninstalled[uninstalled.len() - 2..], &[21, 22]);
            assert!(keyboard.is_none() && mouse.is_none());
        }
    }

    struct Fake {
        unhooked: Arc<AtomicUsize>,
        event: HookEvent,
    }
    impl HookLoopAdapter for Fake {
        fn run(self, rx: mpsc::Receiver<HookCommandRequest>, cb: CallbackSender) {
            let mut active = false;
            while let Ok(request) = rx.recv() {
                let shutdown = request.command == HookCommand::Shutdown;
                match request.command {
                    HookCommand::Start => {
                        active = true;
                        cb.submit(self.event);
                        cb.submit(self.event);
                    }
                    HookCommand::Stop => active = false,
                    HookCommand::Shutdown => {
                        if active {
                            active = false;
                        }
                        self.unhooked.fetch_add(1, Ordering::SeqCst);
                    }
                    HookCommand::Fence => {}
                    _ => {}
                }
                request.acknowledge(true);
                if shutdown {
                    break;
                }
            }
            let _ = active;
        }
    }
    fn key(flags: u32, extra_info: usize) -> HookEvent {
        HookEvent::Key {
            timestamp_us: 1,
            transition: KeyTransition::Down,
            vk: 65,
            scan_code: 30,
            flags,
            extra_info,
        }
    }
    #[test]
    fn filtering_and_overflow_are_callback_local() {
        assert!(!should_record(&key(LLKHF_INJECTED, 0), false));
        assert!(!should_record(&key(0, MKMACRO_EXTRA_INFO), true));
        let u = Arc::new(AtomicUsize::new(0));
        let s = HookService::with_adapter(
            Fake {
                unhooked: u.clone(),
                event: key(0, 0),
            },
            1,
        );
        s.start();
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert_eq!(s.dropped_events(), 1);
        s.shutdown();
        assert_eq!(u.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn start_stop_are_idempotent_and_shutdown_joins() {
        let u = Arc::new(AtomicUsize::new(0));
        let s = HookService::with_adapter(
            Fake {
                unhooked: u.clone(),
                event: key(0, 0),
            },
            8,
        );
        assert!(s.start());
        assert!(s.start());
        assert!(s.stop());
        assert!(s.stop());
        s.shutdown();
        s.shutdown();
        assert_eq!(u.load(Ordering::SeqCst), 1);
    }

    struct QueuedTail {
        queued: Arc<Mutex<VecDeque<HookEvent>>>,
    }
    impl HookLoopAdapter for QueuedTail {
        fn run(self, commands: mpsc::Receiver<HookCommandRequest>, callback: CallbackSender) {
            let mut active = false;
            while let Ok(request) = commands.recv() {
                if active && requires_pre_boundary_drain(request.command) {
                    for event in self.queued.lock().unwrap().drain(..) {
                        callback.submit(event);
                    }
                }
                let shutdown = request.command == HookCommand::Shutdown;
                match request.command {
                    HookCommand::Start | HookCommand::Resume => active = true,
                    HookCommand::Pause | HookCommand::Stop | HookCommand::Shutdown => {
                        active = false
                    }
                    HookCommand::Fence => {}
                }
                request.acknowledge(true);
                if shutdown {
                    break;
                }
            }
        }
    }

    #[test]
    fn acknowledged_stop_drains_pre_boundary_tail_and_excludes_later_input() {
        let queued = Arc::new(Mutex::new(VecDeque::new()));
        let service = HookService::with_adapter(
            QueuedTail {
                queued: queued.clone(),
            },
            8,
        );
        let events = service.take_events().unwrap();
        assert!(service.start());
        queued.lock().unwrap().push_back(key(0, 0));
        assert!(service.stop());
        let fence = service.fence();
        queued.lock().unwrap().push_back(HookEvent::Key {
            timestamp_us: 2,
            transition: KeyTransition::Up,
            vk: 65,
            scan_code: 30,
            flags: 0,
            extra_info: 0,
        });
        assert!(service.synchronize().is_some());
        let retained = events.try_iter().collect::<Vec<_>>();
        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].sequence, 0);
        assert!(retained[0].sequence < fence);
        service.shutdown();
    }
}
