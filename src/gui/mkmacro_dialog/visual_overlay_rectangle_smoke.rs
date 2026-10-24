#![cfg(feature = "dev-smoke-binaries")]

//! Opt-in, owned-window input-isolation evidence. Never accesses the clipboard.
use super::*;
use std::{
    cell::Cell,
    sync::atomic::{AtomicUsize, Ordering},
};
use windows::Win32::{
    Foundation::{FILETIME, GetLastError, HANDLE, HWND, SetLastError, WIN32_ERROR},
    Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, ClientToScreen, CreateCompatibleDC, CreateDIBSection,
        DIB_RGB_COLORS, DeleteDC,
    },
    System::Threading::{GetCurrentProcess, GetProcessTimes},
    UI::{
        Controls::EM_GETSEL,
        Input::KeyboardAndMouse::{
            GetFocus, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
            MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT, SendInput,
        },
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            GWLP_USERDATA, GetClientRect, GetDlgCtrlID, GetForegroundWindow, GetParent, GetPropW,
            GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, HMENU, IsWindow,
            IsWindowVisible, PRF_CHILDREN, PRF_CLIENT, PRF_ERASEBKGND, PostMessageW, RemovePropW,
            SW_SHOW, SendMessageW, SetCursorPos, SetPropW, WINDOW_EX_STYLE, WM_APP, WM_CHAR,
            WM_COMMAND, WM_KEYDOWN, WM_KEYUP, WM_PRINT, WM_SYSCHAR, WM_SYSKEYDOWN, WM_SYSKEYUP,
            WS_BORDER, WS_CHILD, WS_OVERLAPPEDWINDOW, WS_VISIBLE, WindowFromPoint,
        },
    },
};
const FENCE: u32 = WM_APP + 0x62;
const TIMEOUT: Duration = Duration::from_secs(3);
const RECEIVER_IDENTITY_PROPERTY: PCWSTR =
    windows::core::w!("MultiLauncher.RectangleIsolation.Identity");
static NEXT_RECEIVER_IDENTITY: AtomicUsize = AtomicUsize::new(1);
#[derive(Debug)]
struct Failure {
    environment: bool,
    message: String,
}
impl From<VisualOverlayError> for Failure {
    fn from(error: VisualOverlayError) -> Self {
        Self::failed(error.to_string())
    }
}
impl Failure {
    fn failed(message: impl Into<String>) -> Self {
        Self {
            environment: false,
            message: message.into(),
        }
    }
    fn environment(message: impl Into<String>) -> Self {
        Self {
            environment: true,
            message: message.into(),
        }
    }
}
pub(super) fn run() -> Result<(), VisualOverlayError> {
    let evidence = SessionEvidence::default();
    let mut result = session(&evidence);
    let cleanup_failures = evidence.cleanup_failures();
    if !cleanup_failures.is_empty() {
        let cleanup = cleanup_failures.join("; ");
        result = Err(match result {
            Ok(()) => Failure::failed(format!(
                "session completed but receiver cleanup verification failed: {cleanup}"
            )),
            Err(original) => Failure {
                environment: original.environment,
                message: format!(
                    "{}; receiver cleanup verification also failed: {cleanup}",
                    original.message
                ),
            },
        });
    }
    let (status, message) = match &result {
        Ok(()) => ("complete", "none"),
        Err(error) if error.environment => ("not_run_environment", error.message.as_str()),
        Err(error) => ("native_fixture_failed", error.message.as_str()),
    };
    let completed = evidence.completed.get();
    let mut unperformed = Vec::new();
    for (bit, name) in [
        (STAGE_DRAG, "drag"),
        (STAGE_CANCEL, "cancel"),
        (STAGE_ORDINARY_CLICK, "ordinary_click"),
    ] {
        if completed & bit == 0 {
            unperformed.push(name);
        }
    }
    let counts = evidence.receiver.messages.snapshot();
    eprintln!(
        "rectangle-input-isolation: terminal status={status} phase={} failure={message:?} receiver_hwnds=root:{:#x},edit:{:#x},button:{:#x} created=root:{},edit:{},button:{} destroyed=root:{:?},edit:{:?},button:{:?} accepted_mouse_down/up={}/{} accepted_key_down/up={}/{} passive_wm_lbuttondown/up/command/key_down/key_up/char/syskeydown/syskeyup/syschar={}/{}/{}/{}/{}/{}/{}/{}/{} stages_completed={completed:#05b} unperformed={unperformed:?} screenshot={}",
        evidence.phase.get(),
        evidence.receiver.hwnd.load(Ordering::Relaxed),
        evidence.receiver.edit_hwnd.load(Ordering::Relaxed),
        evidence.receiver.button_hwnd.load(Ordering::Relaxed),
        evidence.receiver.created.load(Ordering::Relaxed),
        evidence.receiver.edit_created.load(Ordering::Relaxed),
        evidence.receiver.button_created.load(Ordering::Relaxed),
        match evidence.receiver.destroyed.load(Ordering::Relaxed) {
            -1 => "not_created",
            0 => "not_destroyed",
            _ => "destroyed",
        },
        match evidence.receiver.edit_destroyed.load(Ordering::Relaxed) {
            -1 => "not_created",
            0 => "not_destroyed",
            _ => "destroyed",
        },
        match evidence.receiver.button_destroyed.load(Ordering::Relaxed) {
            -1 => "not_created",
            0 => "not_destroyed",
            _ => "destroyed",
        },
        evidence
            .receiver
            .accepted_mouse_down
            .load(Ordering::Relaxed),
        evidence.receiver.accepted_mouse_up.load(Ordering::Relaxed),
        evidence.receiver.accepted_key_down.load(Ordering::Relaxed),
        evidence.receiver.accepted_key_up.load(Ordering::Relaxed),
        counts.down,
        counts.up,
        counts.commands,
        counts.key_down,
        counts.key_up,
        counts.character,
        counts.system_key_down,
        counts.system_key_up,
        counts.system_character,
        evidence.screenshot.get(),
    );
    result.map_err(|error| {
        platform(format!(
            "{}: phase={}: {}",
            if error.environment {
                "not_run_environment"
            } else {
                "native_fixture_failed"
            },
            evidence.phase.get(),
            error.message
        ))
    })
}

const STAGE_DRAG: u8 = 1;
const STAGE_CANCEL: u8 = 2;
const STAGE_ORDINARY_CLICK: u8 = 4;

#[derive(Default)]
struct MessageCounts {
    down: AtomicUsize,
    up: AtomicUsize,
    commands: AtomicUsize,
    key_down: AtomicUsize,
    key_up: AtomicUsize,
    character: AtomicUsize,
    system_key_down: AtomicUsize,
    system_key_up: AtomicUsize,
    system_character: AtomicUsize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct MessageSnapshot {
    down: usize,
    up: usize,
    commands: usize,
    key_down: usize,
    key_up: usize,
    character: usize,
    system_key_down: usize,
    system_key_up: usize,
    system_character: usize,
}

impl MessageCounts {
    fn snapshot(&self) -> MessageSnapshot {
        let load = |counter: &AtomicUsize| counter.load(Ordering::Relaxed);
        MessageSnapshot {
            down: load(&self.down),
            up: load(&self.up),
            commands: load(&self.commands),
            key_down: load(&self.key_down),
            key_up: load(&self.key_up),
            character: load(&self.character),
            system_key_down: load(&self.system_key_down),
            system_key_up: load(&self.system_key_up),
            system_character: load(&self.system_character),
        }
    }
}

struct SessionEvidence {
    phase: Cell<&'static str>,
    completed: Cell<u8>,
    screenshot: Cell<&'static str>,
    receiver: std::sync::Arc<ReceiverRunEvidence>,
}

#[derive(Default)]
struct ReceiverRunEvidence {
    hwnd: std::sync::atomic::AtomicIsize,
    edit_hwnd: std::sync::atomic::AtomicIsize,
    button_hwnd: std::sync::atomic::AtomicIsize,
    created: std::sync::atomic::AtomicBool,
    edit_created: std::sync::atomic::AtomicBool,
    button_created: std::sync::atomic::AtomicBool,
    // -1 means the receiver was never created, 0 means a native HWND
    // survived cleanup, and 1 means destruction was observed.
    destroyed: std::sync::atomic::AtomicIsize,
    edit_destroyed: std::sync::atomic::AtomicIsize,
    button_destroyed: std::sync::atomic::AtomicIsize,
    accepted_mouse_down: AtomicUsize,
    accepted_mouse_up: AtomicUsize,
    accepted_key_down: AtomicUsize,
    accepted_key_up: AtomicUsize,
    messages: std::sync::Arc<MessageCounts>,
}

impl Default for SessionEvidence {
    fn default() -> Self {
        Self {
            phase: Cell::new("session_start"),
            completed: Cell::new(0),
            screenshot: Cell::new("not_run"),
            receiver: std::sync::Arc::new(ReceiverRunEvidence {
                destroyed: std::sync::atomic::AtomicIsize::new(-1),
                edit_destroyed: std::sync::atomic::AtomicIsize::new(-1),
                button_destroyed: std::sync::atomic::AtomicIsize::new(-1),
                ..Default::default()
            }),
        }
    }
}

impl SessionEvidence {
    fn cleanup_failures(&self) -> Vec<String> {
        let receiver = &self.receiver;
        let mut failures = Vec::new();
        for (name, created, destroyed) in [
            (
                "root",
                receiver.created.load(Ordering::Relaxed),
                receiver.destroyed.load(Ordering::Relaxed),
            ),
            (
                "edit",
                receiver.edit_created.load(Ordering::Relaxed),
                receiver.edit_destroyed.load(Ordering::Relaxed),
            ),
            (
                "button",
                receiver.button_created.load(Ordering::Relaxed),
                receiver.button_destroyed.load(Ordering::Relaxed),
            ),
        ] {
            if created && destroyed != 1 {
                failures.push(format!("created {name} HWND survived cleanup"));
            } else if !created && destroyed != -1 {
                failures.push(format!(
                    "{name} HWND destruction evidence is inconsistent: created={created}, destroyed={destroyed}"
                ));
            }
        }
        let mouse_down = receiver.accepted_mouse_down.load(Ordering::Relaxed);
        let mouse_up = receiver.accepted_mouse_up.load(Ordering::Relaxed);
        if mouse_down != mouse_up {
            failures.push(format!(
                "accepted mouse input was not balanced: down={mouse_down}, up={mouse_up}"
            ));
        }
        let key_down = receiver.accepted_key_down.load(Ordering::Relaxed);
        let key_up = receiver.accepted_key_up.load(Ordering::Relaxed);
        if key_down != key_up {
            failures.push(format!(
                "accepted keyboard input was not balanced: down={key_down}, up={key_up}"
            ));
        }
        failures
    }
}
struct ReceiverState {
    messages: std::sync::Arc<MessageCounts>,
    fence: Cell<usize>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Snapshot {
    messages: MessageSnapshot,
    selection: (u32, u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeWindowIdentity {
    hwnd: HWND,
    parent: HWND,
    process_id: u32,
    thread_id: u32,
    control_id: i32,
    window_rect: NativeRect,
    client_rect: NativeRect,
    identity_token: usize,
}

#[derive(Debug, Clone, Copy)]
struct InputGuardReceipt {
    target: NativeWindowIdentity,
    point: MkPoint,
    foreground: HWND,
    foreground_pid: u32,
    expected_hit: HWND,
    actual_hit: HWND,
    owned_left_down: bool,
    owned_escape_down: bool,
}

struct Receiver {
    hwnd: HWND,
    edit: HWND,
    button: HWND,
    state: Box<ReceiverState>,
    run_evidence: std::sync::Arc<ReceiverRunEvidence>,
    process_birth: u64,
    root_identity: Option<NativeWindowIdentity>,
    edit_identity: Option<NativeWindowIdentity>,
    button_identity: Option<NativeWindowIdentity>,
    root_token: usize,
    edit_token: usize,
    button_token: usize,
    owned_left_down: Cell<bool>,
    owned_escape_down: Cell<bool>,
    original_cursor: POINT,
    last_cursor: Cell<(i32, i32)>,
}

fn current_process_birth() -> Result<u64, Failure> {
    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    }
    .map_err(|error| Failure::environment(format!("current process birth query: {error}")))?;
    Ok((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

fn next_identity_token() -> Result<usize, Failure> {
    NEXT_RECEIVER_IDENTITY
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .map_err(|_| Failure::environment("receiver identity token space exhausted"))
}

fn bind_window_identity(hwnd: HWND, token: usize) -> Result<(), Failure> {
    unsafe {
        SetPropW(
            hwnd,
            RECEIVER_IDENTITY_PROPERTY,
            HANDLE(token as *mut std::ffi::c_void),
        )
    }
    .map_err(|error| Failure::environment(format!("receiver HWND identity binding: {error}")))
}

fn rect_tuple(rect: RECT) -> NativeRect {
    NativeRect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}

fn current_parent(hwnd: HWND) -> Result<HWND, Failure> {
    if !unsafe { IsWindow(hwnd) }.as_bool() {
        return Err(Failure::failed(format!(
            "receiver HWND {hwnd:?} disappeared before parent query"
        )));
    }
    // GetParent's generated Result wrapper maps a NULL result to Err, even
    // though NULL is the documented parent for an unowned top-level HWND.
    // Clear and inspect last-error so only that exact valid-window result is
    // represented as HWND::default(); child-parent mismatches and real errors
    // remain failures.
    unsafe { SetLastError(WIN32_ERROR(0)) };
    let result = unsafe { GetParent(hwnd) };
    let last_error = unsafe { GetLastError() };
    let parent = match result {
        Ok(parent) => parent,
        Err(_) if last_error == WIN32_ERROR(0) && unsafe { IsWindow(hwnd) }.as_bool() => {
            HWND::default()
        }
        Err(error) => {
            return Err(Failure::failed(format!(
                "receiver GetParent({hwnd:?}) failed with last-error {}: {error}",
                last_error.0
            )));
        }
    };
    if !unsafe { IsWindow(hwnd) }.as_bool() {
        return Err(Failure::failed(format!(
            "receiver HWND {hwnd:?} disappeared during parent query"
        )));
    }
    Ok(parent)
}

fn capture_window_identity(
    hwnd: HWND,
    expected_parent: HWND,
    expected_control_id: i32,
    token: usize,
) -> Result<NativeWindowIdentity, Failure> {
    if !unsafe { IsWindow(hwnd) }.as_bool() || !unsafe { IsWindowVisible(hwnd) }.as_bool() {
        return Err(Failure::failed(format!(
            "receiver HWND {hwnd:?} is absent or not visible"
        )));
    }
    let parent = current_parent(hwnd)?;
    let mut process_id = 0;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    if parent != expected_parent
        || process_id != std::process::id()
        || thread_id == 0
        || unsafe { GetDlgCtrlID(hwnd) } != expected_control_id
        || unsafe { GetPropW(hwnd, RECEIVER_IDENTITY_PROPERTY).0 as usize } != token
    {
        return Err(Failure::failed(format!(
            "receiver HWND identity changed: hwnd={hwnd:?} parent={parent:?} pid={process_id} thread={thread_id} control_id={} token={:#x}",
            unsafe { GetDlgCtrlID(hwnd) },
            unsafe { GetPropW(hwnd, RECEIVER_IDENTITY_PROPERTY).0 as usize }
        )));
    }
    let mut window_rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut window_rect) }
        .map_err(|error| Failure::failed(format!("receiver GetWindowRect({hwnd:?}): {error}")))?;
    let mut client_rect = RECT::default();
    unsafe { GetClientRect(hwnd, &mut client_rect) }
        .map_err(|error| Failure::failed(format!("receiver GetClientRect({hwnd:?}): {error}")))?;
    Ok(NativeWindowIdentity {
        hwnd,
        parent,
        process_id,
        thread_id,
        control_id: expected_control_id,
        window_rect: rect_tuple(window_rect),
        client_rect: rect_tuple(client_rect),
        identity_token: token,
    })
}

unsafe extern "system" fn receiver_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        let create = unsafe { &*(l.0 as *const CREATESTRUCTW) };
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
    }
    let state = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const ReceiverState };
    if !state.is_null() {
        let state = unsafe { &*state };
        count_message(state, msg);
        if msg == WM_COMMAND {
            state.messages.commands.fetch_add(1, Ordering::Relaxed);
        }
        if msg == FENCE {
            state.fence.set(w.0);
            return LRESULT(0);
        }
    }
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
}
unsafe extern "system" fn child_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    _: usize,
    data: usize,
) -> LRESULT {
    // The receiver Box outlives both children; DestroyWindow(parent) destroys
    // subclasses before dropping that Box, on this same UI thread.
    let state = unsafe { &*(data as *const ReceiverState) };
    count_message(state, msg);
    unsafe { DefSubclassProc(hwnd, msg, w, l) }
}
fn count_message(state: &ReceiverState, msg: u32) {
    if msg == WM_LBUTTONDOWN {
        state.messages.down.fetch_add(1, Ordering::Relaxed);
    }
    if msg == WM_LBUTTONUP {
        state.messages.up.fetch_add(1, Ordering::Relaxed);
    }
    match msg {
        WM_KEYDOWN => {
            state.messages.key_down.fetch_add(1, Ordering::Relaxed);
        }
        WM_KEYUP => {
            state.messages.key_up.fetch_add(1, Ordering::Relaxed);
        }
        WM_CHAR => {
            state.messages.character.fetch_add(1, Ordering::Relaxed);
        }
        WM_SYSKEYDOWN => {
            state
                .messages
                .system_key_down
                .fetch_add(1, Ordering::Relaxed);
        }
        WM_SYSKEYUP => {
            state.messages.system_key_up.fetch_add(1, Ordering::Relaxed);
        }
        WM_SYSCHAR => {
            state
                .messages
                .system_character
                .fetch_add(1, Ordering::Relaxed);
        }
        _ => {}
    }
}
fn idle_keys_with_owned(allow_left: bool, allow_escape: bool) -> bool {
    (1..=255).all(|key| {
        (allow_left && key == i32::from(VK_LBUTTON.0))
            || (allow_escape && key == i32::from(VK_ESCAPE.0))
            || unsafe { GetAsyncKeyState(key) } >= 0
    })
}
impl Receiver {
    fn create(evidence: &SessionEvidence) -> Result<Self, Failure> {
        if !idle_keys_with_owned(false, false) {
            return Err(Failure::environment(
                "native keys/buttons are not idle; no input sent",
            ));
        }
        let process_birth = current_process_birth()?;
        let root_token = next_identity_token()?;
        let edit_token = next_identity_token()?;
        let button_token = next_identity_token()?;
        let module = unsafe { GetModuleHandleW(None) }
            .map_err(|error| Failure::environment(format!("module lookup: {error}")))?;
        let class = windows::core::w!("MultiLauncherRectangleIsolationReceiver");
        let wc = WNDCLASSW {
            hInstance: module.into(),
            lpszClassName: class,
            lpfnWndProc: Some(receiver_proc),
            ..Default::default()
        };
        if unsafe { RegisterClassW(&wc) } == 0
            && unsafe { GetLastError() } != ERROR_CLASS_ALREADY_EXISTS
        {
            return Err(Failure::environment(format!(
                "receiver class registration Win32 error {}",
                unsafe { GetLastError().0 }
            )));
        }
        let monitor = displays()?
            .into_iter()
            .find(|monitor| monitor.width >= 620 && monitor.height >= 380)
            .ok_or_else(|| Failure::environment("no active monitor fits the owned fixture"))?;
        let mut state = Box::new(ReceiverState {
            messages: std::sync::Arc::clone(&evidence.receiver.messages),
            fence: Cell::new(0),
        });
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class,
                windows::core::w!("Owned rectangle isolation fixture — public dummy data"),
                WS_OVERLAPPEDWINDOW,
                monitor.x + 40,
                monitor.y + 40,
                540,
                300,
                None,
                None,
                module,
                Some(state.as_mut() as *mut _ as *const _),
            )
        }
        .map_err(|error| Failure::environment(format!("receiver CreateWindowExW: {error}")))?;
        evidence
            .receiver
            .hwnd
            .store(hwnd.0 as isize, Ordering::Relaxed);
        evidence.receiver.created.store(true, Ordering::Relaxed);
        evidence.receiver.destroyed.store(0, Ordering::Relaxed);
        let mut receiver = Self {
            hwnd,
            edit: HWND::default(),
            button: HWND::default(),
            state,
            run_evidence: std::sync::Arc::clone(&evidence.receiver),
            process_birth,
            root_identity: None,
            edit_identity: None,
            button_identity: None,
            root_token,
            edit_token,
            button_token,
            owned_left_down: Cell::new(false),
            owned_escape_down: Cell::new(false),
            original_cursor: POINT::default(),
            last_cursor: Cell::new((0, 0)),
        };
        if let Err(error) = bind_window_identity(receiver.hwnd, receiver.root_token) {
            // This HWND was created by this call and has no children yet. If
            // destruction itself fails, clear our callback pointer before
            // releasing the state allocation.
            if unsafe { DestroyWindow(receiver.hwnd) }.is_err()
                && unsafe { GetWindowLongPtrW(receiver.hwnd, GWLP_USERDATA) }
                    == receiver.state.as_ref() as *const ReceiverState as isize
            {
                unsafe {
                    SetWindowLongPtrW(receiver.hwnd, GWLP_USERDATA, 0);
                }
            }
            return Err(error);
        }
        // Show normally as the explicit fixture target. It never calls
        // SetForegroundWindow; inability to own foreground is fail-closed.
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        receiver.root_identity = Some(capture_window_identity(
            receiver.hwnd,
            HWND::default(),
            0,
            receiver.root_token,
        )?);
        unsafe { GetCursorPos(&mut receiver.original_cursor) }
            .map_err(|error| Failure::environment(format!("initial cursor query: {error}")))?;
        receiver
            .last_cursor
            .set((receiver.original_cursor.x, receiver.original_cursor.y));
        receiver.edit = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                windows::core::w!("EDIT"),
                windows::core::w!("Public dummy text: dragging here must not select these words."),
                WS_CHILD | WS_VISIBLE | WS_BORDER,
                20,
                30,
                470,
                75,
                hwnd,
                HMENU(100usize as *mut _),
                module,
                None,
            )
        }
        .map_err(|error| Failure::environment(format!("owned EDIT creation: {error}")))?;
        evidence
            .receiver
            .edit_hwnd
            .store(receiver.edit.0 as isize, Ordering::Relaxed);
        evidence
            .receiver
            .edit_created
            .store(true, Ordering::Relaxed);
        evidence.receiver.edit_destroyed.store(0, Ordering::Relaxed);
        receiver.button = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                windows::core::w!("BUTTON"),
                windows::core::w!("Dummy button"),
                WS_CHILD | WS_VISIBLE,
                20,
                150,
                160,
                40,
                hwnd,
                HMENU(101usize as *mut _),
                module,
                None,
            )
        }
        .map_err(|error| Failure::environment(format!("owned BUTTON creation: {error}")))?;
        evidence
            .receiver
            .button_hwnd
            .store(receiver.button.0 as isize, Ordering::Relaxed);
        evidence
            .receiver
            .button_created
            .store(true, Ordering::Relaxed);
        evidence
            .receiver
            .button_destroyed
            .store(0, Ordering::Relaxed);
        bind_window_identity(receiver.edit, receiver.edit_token)?;
        bind_window_identity(receiver.button, receiver.button_token)?;
        let data = receiver.state.as_ref() as *const ReceiverState as usize;
        for child in [receiver.edit, receiver.button] {
            if !unsafe { SetWindowSubclass(child, Some(child_proc), 1, data) }.as_bool() {
                return Err(Failure::environment(
                    "owned control subclass installation failed",
                ));
            }
        }
        receiver.edit_identity = Some(capture_window_identity(
            receiver.edit,
            receiver.hwnd,
            100,
            receiver.edit_token,
        )?);
        receiver.button_identity = Some(capture_window_identity(
            receiver.button,
            receiver.hwnd,
            101,
            receiver.button_token,
        )?);
        let deadline = Instant::now() + TIMEOUT;
        while !receiver.foreground() {
            receiver.verify_native_identities()?;
            pump();
            if Instant::now() >= deadline {
                return Err(Failure::environment(
                    "exact receiver HWND/PID could not become foreground; no input sent",
                ));
            }
            std::thread::yield_now();
        }
        receiver.guard()?;
        let mut foreground_pid = 0;
        let foreground = unsafe { GetForegroundWindow() };
        unsafe { GetWindowThreadProcessId(foreground, Some(&mut foreground_pid)) };
        eprintln!(
            "rectangle-input-isolation: receiver identity pid={} birth={} root={:?} edit={:?} button={:?} foreground={foreground:?}/{foreground_pid} key_state=idle",
            std::process::id(),
            receiver.process_birth,
            receiver.root_identity,
            receiver.edit_identity,
            receiver.button_identity,
        );
        receiver.fence()?;
        Ok(receiver)
    }
    fn foreground(&self) -> bool {
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(self.hwnd, Some(&mut pid));
        }
        (unsafe { GetForegroundWindow() }) == self.hwnd && pid == std::process::id()
    }
    fn lifetime_matches(&self, identity: NativeWindowIdentity) -> bool {
        if !unsafe { IsWindow(identity.hwnd) }.as_bool() {
            return false;
        }
        let mut process_id = 0;
        let thread_id = unsafe { GetWindowThreadProcessId(identity.hwnd, Some(&mut process_id)) };
        process_id == identity.process_id
            && thread_id == identity.thread_id
            && current_parent(identity.hwnd).is_ok_and(|parent| parent == identity.parent)
            && unsafe { GetDlgCtrlID(identity.hwnd) } == identity.control_id
            && unsafe { GetPropW(identity.hwnd, RECEIVER_IDENTITY_PROPERTY).0 as usize }
                == identity.identity_token
    }
    fn root_lifetime_is_ours(&self) -> bool {
        let exact_binding = self.root_identity.map_or_else(
            || self.current_binding_matches(self.hwnd, HWND::default(), 0, self.root_token),
            |root| self.lifetime_matches(root),
        );
        current_process_birth().is_ok_and(|birth| birth == self.process_birth)
            && exact_binding
            && unsafe { GetWindowLongPtrW(self.hwnd, GWLP_USERDATA) }
                == self.state.as_ref() as *const ReceiverState as isize
    }
    fn current_binding_matches(
        &self,
        hwnd: HWND,
        parent: HWND,
        control_id: i32,
        token: usize,
    ) -> bool {
        if !unsafe { IsWindow(hwnd) }.as_bool() {
            return false;
        }
        let mut process_id = 0;
        let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        process_id == std::process::id()
            && thread_id != 0
            && self
                .root_identity
                .is_none_or(|root| root.thread_id == thread_id)
            && current_parent(hwnd).is_ok_and(|actual_parent| actual_parent == parent)
            && unsafe { GetDlgCtrlID(hwnd) } == control_id
            && unsafe { GetPropW(hwnd, RECEIVER_IDENTITY_PROPERTY).0 as usize } == token
    }
    fn detach_owned_callbacks(&self) -> bool {
        let mut detached = true;
        for (hwnd, parent, control_id, token) in [
            (self.edit, self.hwnd, 100, self.edit_token),
            (self.button, self.hwnd, 101, self.button_token),
        ] {
            if self.current_binding_matches(hwnd, parent, control_id, token) {
                let removed = unsafe { RemoveWindowSubclass(hwnd, Some(child_proc), 1) }.as_bool();
                if removed {
                    let _ = unsafe { RemovePropW(hwnd, RECEIVER_IDENTITY_PROPERTY) };
                } else {
                    detached = false;
                    eprintln!(
                        "rectangle-input-isolation: could not prove subclass detachment hwnd={hwnd:?}"
                    );
                }
            }
        }
        let mut root_process_id = 0;
        let root_thread_id =
            unsafe { GetWindowThreadProcessId(self.hwnd, Some(&mut root_process_id)) };
        let root_pointer_is_ours = unsafe { IsWindow(self.hwnd) }.as_bool()
            && root_process_id == std::process::id()
            && root_thread_id != 0
            && self
                .root_identity
                .is_none_or(|root| root.thread_id == root_thread_id)
            && current_process_birth().is_ok_and(|birth| birth == self.process_birth)
            && unsafe { GetWindowLongPtrW(self.hwnd, GWLP_USERDATA) }
                == self.state.as_ref() as *const ReceiverState as isize;
        if root_pointer_is_ours {
            unsafe {
                SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            }
            if unsafe { GetPropW(self.hwnd, RECEIVER_IDENTITY_PROPERTY).0 as usize }
                == self.root_token
            {
                let _ = unsafe { RemovePropW(self.hwnd, RECEIVER_IDENTITY_PROPERTY) };
            }
            detached &= unsafe { GetWindowLongPtrW(self.hwnd, GWLP_USERDATA) }
                != self.state.as_ref() as *const ReceiverState as isize;
        }
        detached
    }
    fn verify_native_identities(&self) -> Result<(), Failure> {
        if current_process_birth()? != self.process_birth {
            return Err(Failure::failed(
                "current process birth identity changed; refusing fixture action",
            ));
        }
        let root = self
            .root_identity
            .ok_or_else(|| Failure::failed("receiver root identity was not captured"))?;
        let edit = self
            .edit_identity
            .ok_or_else(|| Failure::failed("receiver EDIT identity was not captured"))?;
        let button = self
            .button_identity
            .ok_or_else(|| Failure::failed("receiver BUTTON identity was not captured"))?;
        if unsafe { GetWindowLongPtrW(self.hwnd, GWLP_USERDATA) }
            != self.state.as_ref() as *const ReceiverState as isize
        {
            return Err(Failure::failed(
                "receiver root userdata identity changed; refusing fixture action",
            ));
        }
        for (recorded, parent, control_id, token) in [
            (root, HWND::default(), 0, self.root_token),
            (edit, self.hwnd, 100, self.edit_token),
            (button, self.hwnd, 101, self.button_token),
        ] {
            let current = capture_window_identity(recorded.hwnd, parent, control_id, token)?;
            if current != recorded {
                return Err(Failure::failed(format!(
                    "receiver native geometry/identity changed: expected={recorded:?}, current={current:?}"
                )));
            }
        }
        Ok(())
    }
    fn owned_key_state_is_exact(&self) -> bool {
        idle_keys_with_owned(self.owned_left_down.get(), self.owned_escape_down.get())
    }
    fn identity_for(&self, hwnd: HWND) -> Result<NativeWindowIdentity, Failure> {
        if hwnd == self.hwnd {
            self.root_identity
                .ok_or_else(|| Failure::failed("receiver root identity unavailable"))
        } else if hwnd == self.edit {
            self.edit_identity
                .ok_or_else(|| Failure::failed("receiver EDIT identity unavailable"))
        } else if hwnd == self.button {
            self.button_identity
                .ok_or_else(|| Failure::failed("receiver BUTTON identity unavailable"))
        } else {
            Err(Failure::failed(format!(
                "HWND {hwnd:?} is not a retained receiver control"
            )))
        }
    }
    fn focus_is_owned(&self) -> bool {
        let focus = unsafe { GetFocus() };
        focus == self.hwnd || focus == self.edit || focus == self.button
    }
    fn cursor_hits_owned_window(&self, point: POINT) -> bool {
        if self.verify_native_identities().is_err() {
            return false;
        }
        let actual = unsafe { WindowFromPoint(point) };
        [self.hwnd, self.edit, self.button].contains(&actual)
            && self
                .identity_for(actual)
                .is_ok_and(|identity| self.lifetime_matches(identity))
    }
    fn guard(&self) -> Result<(), Failure> {
        self.verify_native_identities()?;
        if !self.foreground() {
            return Err(Failure::failed(
                "exact owned receiver HWND/PID lost foreground; refusing fixture input",
            ));
        }
        if !self.owned_key_state_is_exact() {
            return Err(Failure::failed(
                "unexpected held keys/buttons; refusing input",
            ));
        }
        Ok(())
    }
    fn guard_input(
        &self,
        controller: &VisualOverlayController,
        operation_id: Option<OperationId>,
        intended_child: HWND,
    ) -> Result<InputGuardReceipt, Failure> {
        let mut point = POINT::default();
        unsafe { GetCursorPos(&mut point) }
            .map_err(|error| Failure::failed(format!("input cursor query: {error}")))?;
        self.guard_point(
            controller,
            operation_id,
            intended_child,
            MkPoint {
                x: point.x,
                y: point.y,
            },
        )
    }
    fn guard_point(
        &self,
        controller: &VisualOverlayController,
        operation_id: Option<OperationId>,
        intended_child: HWND,
        point: MkPoint,
    ) -> Result<InputGuardReceipt, Failure> {
        self.guard()?;
        let intended = self.identity_for(intended_child)?;
        let in_child = point.x >= intended.window_rect.left
            && point.x < intended.window_rect.right
            && point.y >= intended.window_rect.top
            && point.y < intended.window_rect.bottom;
        if !in_child {
            return Err(Failure::failed(format!(
                "input point {:?} is outside exact receiver child {:?} geometry {:?}",
                (point.x, point.y),
                intended.hwnd,
                intended.window_rect
            )));
        }
        let current_shield = controller
            .fixture_rectangle_hit_target(operation_id, point)
            .map_err(Failure::from)?;
        let actual_hit = unsafe {
            WindowFromPoint(POINT {
                x: point.x,
                y: point.y,
            })
        };
        let expected_hit = match current_shield {
            Some(shield) => HWND(shield as *mut std::ffi::c_void),
            None => intended.hwnd,
        };
        if actual_hit != expected_hit || !unsafe { IsWindow(actual_hit) }.as_bool() {
            return Err(Failure::failed(format!(
                "pre-input hit-owner proof failed: operation={operation_id:?} expected={expected_hit:?} actual={actual_hit:?} point={point:?}"
            )));
        }
        let foreground = unsafe { GetForegroundWindow() };
        let mut foreground_pid = 0;
        unsafe { GetWindowThreadProcessId(foreground, Some(&mut foreground_pid)) };
        if foreground != self.hwnd || foreground_pid != std::process::id() {
            return Err(Failure::failed(format!(
                "foreground changed during point proof: expected={:?}/{} actual={foreground:?}/{foreground_pid}",
                self.hwnd,
                std::process::id()
            )));
        }
        Ok(InputGuardReceipt {
            target: intended,
            point,
            foreground,
            foreground_pid,
            expected_hit,
            actual_hit,
            owned_left_down: self.owned_left_down.get(),
            owned_escape_down: self.owned_escape_down.get(),
        })
    }
    fn position(
        &mut self,
        child: HWND,
        x: i32,
        y: i32,
        controller: &VisualOverlayController,
        operation_id: Option<OperationId>,
    ) -> Result<MkPoint, Failure> {
        let intended = self.identity_for(child)?;
        let mut point = POINT { x, y };
        if !unsafe { ClientToScreen(child, &mut point) }.as_bool() {
            return Err(Failure::failed("owned client-to-screen failed"));
        }
        if point.x < intended.window_rect.left
            || point.x >= intended.window_rect.right
            || point.y < intended.window_rect.top
            || point.y >= intended.window_rect.bottom
        {
            return Err(Failure::failed(
                "requested cursor position is outside exact retained child geometry",
            ));
        }
        self.guard_point(
            controller,
            operation_id,
            child,
            MkPoint {
                x: point.x,
                y: point.y,
            },
        )?;
        unsafe { SetCursorPos(point.x, point.y) }
            .map_err(|error| Failure::failed(format!("owned cursor move: {error}")))?;
        self.last_cursor.set((point.x, point.y));
        self.guard_input(controller, operation_id, child)?;
        Ok(MkPoint {
            x: point.x,
            y: point.y,
        })
    }
    fn fence(&self) -> Result<(), Failure> {
        let next = self.state.fence.get() + 1;
        unsafe { PostMessageW(self.hwnd, FENCE, WPARAM(next), LPARAM(0)) }
            .map_err(|error| Failure::failed(format!("receiver fence: {error}")))?;
        let deadline = Instant::now() + TIMEOUT;
        while self.state.fence.get() != next {
            pump();
            if Instant::now() >= deadline {
                return Err(Failure::failed("receiver fence timeout"));
            }
            std::thread::yield_now();
        }
        Ok(())
    }
    fn snapshot(&self) -> Snapshot {
        let (mut start, mut end) = (0u32, 0u32);
        unsafe {
            SendMessageW(
                self.edit,
                EM_GETSEL,
                WPARAM(&mut start as *mut _ as usize),
                LPARAM(&mut end as *mut _ as isize),
            );
        }
        Snapshot {
            messages: self.run_evidence.messages.snapshot(),
            selection: (start, end),
        }
    }
    fn save_owned_client(&self) -> Result<(), Failure> {
        self.guard()?;
        let mut rect = RECT::default();
        unsafe { GetClientRect(self.hwnd, &mut rect) }
            .map_err(|error| Failure::failed(format!("owned screenshot dimensions: {error}")))?;
        let (width, height) = (rect.right, rect.bottom);
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let dc = unsafe { CreateCompatibleDC(None) };
        if dc.0.is_null() {
            return Err(Failure::failed(
                "owned screenshot memory DC creation failed",
            ));
        }
        let mut bits = std::ptr::null_mut();
        let bitmap =
            match unsafe { CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, None, 0) } {
                Ok(bitmap) => bitmap,
                Err(error) => {
                    unsafe {
                        let _ = DeleteDC(dc);
                    }
                    return Err(Failure::failed(format!("owned screenshot bitmap: {error}")));
                }
            };
        if bits.is_null() {
            unsafe {
                let _ = DeleteObject(HGDIOBJ(bitmap.0));
                let _ = DeleteDC(dc);
            }
            return Err(Failure::failed(
                "owned screenshot bitmap has no pixel buffer",
            ));
        }
        let old = unsafe { SelectObject(dc, HGDIOBJ(bitmap.0)) };
        let white = unsafe { CreateSolidBrush(COLORREF(0x00ffffff)) };
        unsafe {
            FillRect(dc, &rect, white);
            let _ = DeleteObject(HGDIOBJ(white.0));
            // WM_PRINT renders only our receiver and owned children into our
            // memory bitmap. No desktop pixels or foreign windows are sampled.
            SendMessageW(
                self.hwnd,
                WM_PRINT,
                WPARAM(dc.0 as usize),
                LPARAM((PRF_CLIENT | PRF_CHILDREN | PRF_ERASEBKGND) as isize),
            );
        }
        let mut pixels = unsafe {
            std::slice::from_raw_parts(bits as *const u8, width as usize * height as usize * 4)
        }
        .to_vec();
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        unsafe {
            SelectObject(dc, old);
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            let _ = DeleteDC(dc);
        }
        let image = image::RgbaImage::from_raw(width as u32, height as u32, pixels)
            .ok_or_else(|| Failure::failed("owned screenshot buffer dimensions"))?;
        let path = std::path::PathBuf::from(format!(
            "target/post-queue-native-acceptance/m1/receiver-{}.png",
            std::process::id()
        ));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| Failure::failed(error.to_string()))?;
        }
        image
            .save(&path)
            .map_err(|error| Failure::failed(error.to_string()))?;
        eprintln!(
            "rectangle-input-isolation: owned WM_PRINT client image: {}",
            path.display()
        );
        Ok(())
    }
}
impl Drop for Receiver {
    fn drop(&mut self) {
        let mut current = POINT::default();
        let cursor_is_at_last_owned_position = unsafe { GetCursorPos(&mut current) }.is_ok()
            && (current.x, current.y) == self.last_cursor.get();
        let cursor_restore = if self.verify_native_identities().is_ok()
            && self.foreground()
            && self.owned_key_state_is_exact()
            && cursor_is_at_last_owned_position
            && self.cursor_hits_owned_window(self.original_cursor)
        {
            // Re-prove the exact target at the last practical point before the
            // cursor move; a previous WindowFromPoint result is not reusable.
            if !self.cursor_hits_owned_window(self.original_cursor) {
                "skipped_unowned_target"
            } else {
                match unsafe { SetCursorPos(self.original_cursor.x, self.original_cursor.y) } {
                    Ok(()) => "restored_to_owned_hit",
                    Err(_) => "restore_failed",
                }
            }
        } else if !self.cursor_hits_owned_window(self.original_cursor) {
            "skipped_unowned_target"
        } else {
            "skipped_current_state_changed"
        };
        if cursor_restore == "restored_to_owned_hit" {
            let mut restored = POINT::default();
            if unsafe { GetCursorPos(&mut restored) }.is_err()
                || restored.x != self.original_cursor.x
                || restored.y != self.original_cursor.y
                || !self.cursor_hits_owned_window(restored)
            {
                eprintln!(
                    "rectangle-input-isolation: cursor restore postcondition failed; no further cursor movement attempted"
                );
            }
        }
        let root_is_still_ours = self.root_lifetime_is_ours();
        let mut destroy_attempted = false;
        if root_is_still_ours {
            destroy_attempted = true;
            if let Err(error) = unsafe { DestroyWindow(self.hwnd) } {
                eprintln!(
                    "rectangle-input-isolation: receiver cleanup DestroyWindow failed hwnd={:?}: {error}",
                    self.hwnd
                );
            }
        }
        let destroyed = !unsafe { IsWindow(self.hwnd) }.as_bool();
        let edit_destroyed = !unsafe { IsWindow(self.edit) }.as_bool();
        let button_destroyed = !unsafe { IsWindow(self.button) }.as_bool();
        let callbacks_detached = if destroyed {
            true
        } else {
            self.detach_owned_callbacks()
        };
        if !destroyed && !callbacks_detached {
            // A failed native teardown must not leave callbacks pointing at a
            // freed Box. Retain this tiny state only in that exceptional case.
            let retained = std::mem::replace(
                &mut self.state,
                Box::new(ReceiverState {
                    messages: std::sync::Arc::clone(&self.run_evidence.messages),
                    fence: Cell::new(0),
                }),
            );
            let _ = Box::leak(retained);
            eprintln!(
                "rectangle-input-isolation: retained callback state because native callback detachment was not proven"
            );
        }
        self.run_evidence
            .destroyed
            .store(if destroyed { 1 } else { 0 }, Ordering::Relaxed);
        if self.run_evidence.edit_created.load(Ordering::Relaxed) {
            self.run_evidence
                .edit_destroyed
                .store(if edit_destroyed { 1 } else { 0 }, Ordering::Relaxed);
        }
        if self.run_evidence.button_created.load(Ordering::Relaxed) {
            self.run_evidence
                .button_destroyed
                .store(if button_destroyed { 1 } else { 0 }, Ordering::Relaxed);
        }
        eprintln!(
            "rectangle-input-isolation: receiver cleanup hwnd={:?} edit={:?} button={:?} destroy_attempted={destroy_attempted} destroyed(root/edit/button)={destroyed}/{edit_destroyed}/{button_destroyed} identity_matched={root_is_still_ours} callbacks_detached={callbacks_detached} cursor_restore={cursor_restore}",
            self.hwnd, self.edit, self.button,
        );
    }
}
fn pump() {
    let mut msg = MSG::default();
    while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
/// Own exactly one accepted Down. Normal input requires the receiver guard;
/// cleanup always balances only this fixture's matching Up, even after a
/// foreground/key-state change. It never adds a new Down or refocuses a window.
struct HeldLeft<'a> {
    receiver: &'a Receiver,
    held: bool,
    up: INPUT,
    escape: bool,
}
impl<'a> HeldLeft<'a> {
    fn press_mouse(
        receiver: &'a Receiver,
        controller: &VisualOverlayController,
        operation_id: Option<OperationId>,
        target: HWND,
    ) -> Result<Self, Failure> {
        Self::insert(
            receiver,
            controller,
            operation_id,
            target,
            mouse(true),
            mouse(false),
            false,
        )
    }
    fn press_escape(
        receiver: &'a Receiver,
        controller: &VisualOverlayController,
        operation_id: Option<OperationId>,
        target: HWND,
    ) -> Result<Self, Failure> {
        let key = |up| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_ESCAPE,
                    dwFlags: if up {
                        KEYEVENTF_KEYUP
                    } else {
                        Default::default()
                    },
                    ..Default::default()
                },
            },
        };
        Self::insert(
            receiver,
            controller,
            operation_id,
            target,
            key(false),
            key(true),
            true,
        )
    }
    fn insert(
        receiver: &'a Receiver,
        controller: &VisualOverlayController,
        operation_id: Option<OperationId>,
        target: HWND,
        down: INPUT,
        up: INPUT,
        escape: bool,
    ) -> Result<Self, Failure> {
        let receipt = receiver.guard_input(controller, operation_id, target)?;
        if escape && !receiver.focus_is_owned() {
            return Err(Failure::failed(
                "Escape target is not the exact owned receiver or child; no key sent",
            ));
        }
        if unsafe { SendInput(&[down], std::mem::size_of::<INPUT>() as i32) } != 1 {
            return Err(Failure::failed(
                "SendInput fixture Down rejected; no held input owned",
            ));
        }
        // Construct ownership immediately after acceptance, before any fallible
        // post-injection guard check, so every return path balances this Down.
        let held = Self {
            receiver,
            held: true,
            up,
            escape,
        };
        if escape {
            receiver.owned_escape_down.set(true);
            receiver
                .run_evidence
                .accepted_key_down
                .fetch_add(1, Ordering::Relaxed);
        } else {
            receiver.owned_left_down.set(true);
            receiver
                .run_evidence
                .accepted_mouse_down
                .fetch_add(1, Ordering::Relaxed);
        }
        eprintln!(
            "rectangle-input-isolation: guarded pre-Down receipt operation={operation_id:?} receiver_root={:?} process_id={} birth={} target={:?} point={:?} foreground={:?}/{} expected_hit={:?} actual_hit={:?} unexpected_keys_clear=true previously_owned_left_down={} previously_owned_escape_down={} accepted_input={}",
            receiver.hwnd,
            receipt.target.process_id,
            receiver.process_birth,
            receipt.target,
            receipt.point,
            receipt.foreground,
            receipt.foreground_pid,
            receipt.expected_hit,
            receipt.actual_hit,
            receipt.owned_left_down,
            receipt.owned_escape_down,
            if escape { "EscapeDown" } else { "LeftDown" },
        );
        if let Err(error) = receiver.guard_input(controller, operation_id, target) {
            return Err(Failure::failed(format!(
                "post-Down guard failed; matching Up cleanup is owned: {}",
                error.message
            )));
        }
        Ok(held)
    }
    fn balance(&mut self) -> Result<(), Failure> {
        if self.held {
            if unsafe { SendInput(&[self.up], std::mem::size_of::<INPUT>() as i32) } != 1 {
                return Err(Failure::failed(
                    "native_fixture_cleanup_failed: SendInput matching owned Up rejected",
                ));
            }
            self.held = false;
            if self.escape {
                self.receiver.owned_escape_down.set(false);
                self.receiver
                    .run_evidence
                    .accepted_key_up
                    .fetch_add(1, Ordering::Relaxed);
            } else {
                self.receiver.owned_left_down.set(false);
                self.receiver
                    .run_evidence
                    .accepted_mouse_up
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
        Ok(())
    }
    fn release(&mut self) -> Result<(), Failure> {
        // An accepted Down owns exactly one matching Up. Always balance it,
        // then report whether the receiver/foreground/input state still agrees.
        let guard_ok = self.receiver.verify_native_identities().is_ok()
            && self.receiver.foreground()
            && self.receiver.owned_key_state_is_exact();
        self.balance()?;
        if !guard_ok {
            return Err(Failure::failed(
                "foreground/input guard changed before release; only matching owned Up was sent",
            ));
        }
        Ok(())
    }
}
impl Drop for HeldLeft<'_> {
    fn drop(&mut self) {
        if let Err(error) = self.balance() {
            // An explicit release already failed, or an error path is unwinding.
            // Surface the cleanup failure even though Drop cannot return it.
            eprintln!(
                "rectangle-input-isolation: {} ({}); no further input attempted",
                error.message,
                if self.escape {
                    "fixture-owned Escape"
                } else {
                    "fixture-owned left button"
                }
            );
        }
    }
}
fn mouse(down: bool) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: if down {
                    MOUSEEVENTF_LEFTDOWN
                } else {
                    MOUSEEVENTF_LEFTUP
                },
                ..Default::default()
            },
        },
    }
}
fn held_escape_cancel(
    receiver: &Receiver,
    controller: &mut VisualOverlayController,
    operation_id: OperationId,
) -> Result<(), Failure> {
    let mut escape =
        HeldLeft::press_escape(receiver, controller, Some(operation_id), receiver.button)?;
    let deadline = Instant::now() + TIMEOUT;
    let result = loop {
        if let Err(error) = receiver.guard_input(controller, Some(operation_id), receiver.button) {
            break Err(error);
        }
        let events = tick(controller);
        if !events.is_empty() {
            break Err(Failure::failed(format!(
                "picker ended before held release: {events:?}"
            )));
        }
        if controller.renderer.close_pending() {
            break Ok(());
        }
        if Instant::now() >= deadline {
            break Err(Failure::failed(
                "held Escape did not enter deferred teardown",
            ));
        }
        std::thread::yield_now();
    };
    // Balance this accepted key even if the preceding guard/result failed.
    // On SendInput failure, propagate that exact cleanup error; Drop makes
    // one final owned-Up attempt and reports rejection to stderr.
    escape.release()?;
    result
}
fn tick(controller: &mut VisualOverlayController) -> Vec<VisualOverlayEvent> {
    controller.advance();
    controller.drain_events()
}
fn wait_event(
    controller: &mut VisualOverlayController,
) -> Result<Vec<VisualOverlayEvent>, Failure> {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let events = tick(controller);
        if !events.is_empty() {
            return Ok(events);
        }
        if Instant::now() >= deadline {
            return Err(Failure::failed("production picker terminal timeout"));
        }
        std::thread::yield_now();
    }
}
fn wait_drag(
    controller: &mut VisualOverlayController,
    receiver: &Receiver,
    operation_id: OperationId,
    target: HWND,
) -> Result<(), Failure> {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        receiver.guard_input(controller, Some(operation_id), target)?;
        let events = tick(controller);
        if !events.is_empty() {
            return Err(Failure::failed(format!(
                "picker ended before drag acknowledgement: {events:?}"
            )));
        }
        if matches!(
            controller.state(),
            VisualOverlayState::PickingRectangle {
                phase: RectangleInteractionPhase::Dragging { .. },
                ..
            }
        ) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(Failure::failed(
                "native shield did not acknowledge held Down",
            ));
        }
        std::thread::yield_now();
    }
}
fn session(evidence: &SessionEvidence) -> Result<(), Failure> {
    evidence.phase.set("receiver_create");
    let mut receiver = Receiver::create(evidence)?;
    let desktop = monitor_union(
        &displays()?
            .into_iter()
            .enumerate()
            .map(|(i, bounds)| MonitorDescriptor {
                index: i + 1,
                bounds,
                primary: i == 0,
            })
            .collect::<Vec<_>>(),
    )
    .ok_or_else(|| Failure::environment("empty desktop"))?;
    let mut controller = VisualOverlayController::default();
    receiver.fence()?;
    let baseline = receiver.snapshot();
    evidence.phase.set("primary_rectangle_start");
    let id = controller.begin_rectangle_pick(RectanglePurpose::PinboardCapture, desktop);
    if !matches!(
        controller.state(),
        VisualOverlayState::PickingRectangle {
            phase: RectangleInteractionPhase::Armed,
            ..
        }
    ) {
        return Err(Failure::failed(format!(
            "coverage/startup acknowledgement failed: {:?}",
            controller.drain_events()
        )));
    }
    evidence.phase.set("primary_drag_start_position");
    let start = receiver.position(receiver.edit, 12, 16, &controller, Some(id))?;
    evidence.phase.set("primary_drag_down");
    let mut held = HeldLeft::press_mouse(&receiver, &controller, Some(id), receiver.edit)?;
    wait_drag(&mut controller, &receiver, id, receiver.edit)?;
    // Receiver is immutably borrowed by HeldLeft. Only move within its owned
    // client, repeating foreground/idle checks at every injection boundary.
    evidence.phase.set("primary_drag_move");
    let mut end = POINT { x: 260, y: 45 };
    if !unsafe { ClientToScreen(receiver.edit, &mut end) }.as_bool() {
        return Err(Failure::failed("owned drag conversion"));
    }
    receiver.guard_point(
        &controller,
        Some(id),
        receiver.edit,
        MkPoint { x: end.x, y: end.y },
    )?;
    unsafe { SetCursorPos(end.x, end.y) }.map_err(|error| Failure::failed(error.to_string()))?;
    receiver.last_cursor.set((end.x, end.y));
    receiver.guard_input(&controller, Some(id), receiver.edit)?;
    wait_drag(&mut controller, &receiver, id, receiver.edit)?;
    evidence.phase.set("primary_drag_up");
    held.release()?;
    drop(held);
    let events = wait_event(&mut controller)?;
    let expected = normalized_rect(start, MkPoint { x: end.x, y: end.y })?;
    if !matches!(events.as_slice(), [VisualOverlayEvent::RectangleConfirmed { operation_id, purpose: RectanglePurpose::PinboardCapture, rect }] if *operation_id == id && *rect == expected)
    {
        return Err(Failure::failed(format!(
            "unexpected capture result: {events:?}; expected {expected:?}"
        )));
    }
    receiver.fence()?;
    let after_drag = receiver.snapshot();
    if after_drag != baseline {
        return Err(Failure::failed(format!(
            "native drag leaked to owned receiver: before={baseline:?}, after={:?}",
            after_drag
        )));
    }
    evidence
        .completed
        .set(evidence.completed.get() | STAGE_DRAG);
    eprintln!(
        "rectangle-input-isolation: PASS injected Down/drag/Up: exact rectangle {expected:?}, receiver Down/Up/commands/edit selection unchanged"
    );
    evidence.phase.set("cancel_rectangle_start");
    let id = controller.begin_rectangle_pick(RectanglePurpose::PinboardCapture, desktop);
    evidence.phase.set("cancel_target_position");
    receiver.position(receiver.button, 40, 15, &controller, Some(id))?;
    evidence.phase.set("cancel_mouse_down");
    let mut held = HeldLeft::press_mouse(&receiver, &controller, Some(id), receiver.button)?;
    wait_drag(&mut controller, &receiver, id, receiver.button)?;
    evidence.phase.set("cancel_escape_down");
    held_escape_cancel(&receiver, &mut controller, id)?;
    evidence.phase.set("cancel_mouse_up");
    held.release()?;
    drop(held);
    let events = wait_event(&mut controller)?;
    if events != vec![VisualOverlayEvent::Cancelled { operation_id: id }] {
        return Err(Failure::failed(format!("held cancel result: {events:?}")));
    }
    evidence
        .completed
        .set(evidence.completed.get() | STAGE_CANCEL);
    receiver.fence()?;
    let after_cancel = receiver.snapshot();
    if after_cancel.messages.down != baseline.messages.down
        || after_cancel.messages.up != baseline.messages.up
        || after_cancel.messages.commands != baseline.messages.commands
        || after_cancel.selection != baseline.selection
    {
        return Err(Failure::failed("held Escape/Up leaked into receiver"));
    }
    if after_cancel.messages.key_down != baseline.messages.key_down
        || after_cancel.messages.key_up != baseline.messages.key_up
        || after_cancel.messages.character != baseline.messages.character
        || after_cancel.messages.system_key_down != baseline.messages.system_key_down
        || after_cancel.messages.system_key_up != baseline.messages.system_key_up
        || after_cancel.messages.system_character != baseline.messages.system_character
    {
        return Err(Failure::failed(format!(
            "injected Escape reached the passive receiver; keyboard isolation is unproven (baseline={:?}, after_cancel={:?})",
            baseline.messages, after_cancel.messages
        )));
    }
    eprintln!(
        "rectangle-input-isolation: post-cancel passive receiver messages baseline={:?} after={:?}; injected Escape remains explicitly counted",
        baseline.messages, after_cancel.messages
    );
    evidence.phase.set("ordinary_button_target");
    receiver.position(receiver.button, 40, 15, &controller, None)?;
    let mut held = HeldLeft::press_mouse(&receiver, &controller, None, receiver.button)?;
    evidence.phase.set("ordinary_button_down");
    held.release()?;
    drop(held);
    evidence.phase.set("ordinary_button_acknowledgement");
    let deadline = Instant::now() + TIMEOUT;
    while receiver.run_evidence.messages.up.load(Ordering::Relaxed) == baseline.messages.up
        || receiver
            .run_evidence
            .messages
            .commands
            .load(Ordering::Relaxed)
            == baseline.messages.commands
    {
        receiver.guard()?;
        pump();
        if Instant::now() >= deadline {
            return Err(Failure::failed(
                "next ordinary owned button click acknowledgement timed out",
            ));
        }
        std::thread::yield_now();
    }
    receiver.fence()?;
    let after = receiver.snapshot();
    if after.messages.down != baseline.messages.down + 1
        || after.messages.up != baseline.messages.up + 1
        || after.messages.commands <= baseline.messages.commands
        || after.messages.key_down != after_cancel.messages.key_down
        || after.messages.key_up != after_cancel.messages.key_up
        || after.messages.character != after_cancel.messages.character
        || after.messages.system_key_down != after_cancel.messages.system_key_down
        || after.messages.system_key_up != after_cancel.messages.system_key_up
        || after.messages.system_character != after_cancel.messages.system_character
    {
        return Err(Failure::failed(format!(
            "next ordinary click did not reach dummy button: {baseline:?} -> {after:?}"
        )));
    }
    evidence
        .completed
        .set(evidence.completed.get() | STAGE_ORDINARY_CLICK);
    eprintln!(
        "rectangle-input-isolation: PASS injected held Escape/release: no leak; next ordinary click reached owned button; counts={after:?}"
    );
    evidence.phase.set("receiver_owned_print");
    if let Err(error) = receiver.save_owned_client() {
        evidence.screenshot.set("not_run_or_failed");
        eprintln!(
            "rectangle-input-isolation: screenshot_not_run: {}",
            error.message
        );
    } else {
        evidence.screenshot.set("receiver_only_wm_print_saved");
    }
    evidence.phase.set("controller_shutdown");
    controller.shutdown();
    Ok(())
}
