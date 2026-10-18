//! Opt-in, owned-window input-isolation evidence. Never accesses the clipboard.
use super::*;
use std::cell::Cell;
use windows::Win32::{
    Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, ClientToScreen, CreateCompatibleDC, CreateDIBSection,
        DIB_RGB_COLORS, DeleteDC,
    },
    UI::{
        Controls::EM_GETSEL,
        Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
            MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT, SendInput,
        },
        Shell::{DefSubclassProc, SetWindowSubclass},
        WindowsAndMessaging::{
            GetClientRect, GetForegroundWindow, GetWindowThreadProcessId, HMENU, PRF_CHILDREN,
            PRF_CLIENT, PRF_ERASEBKGND, PostMessageW, SW_SHOW, SendMessageW, SetCursorPos,
            SetForegroundWindow, WINDOW_EX_STYLE, WM_APP, WM_COMMAND, WM_PRINT, WS_BORDER,
            WS_CHILD, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
        },
    },
};
const FENCE: u32 = WM_APP + 0x62;
const TIMEOUT: Duration = Duration::from_secs(3);
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
    match session() {
        Ok(()) => Ok(()),
        Err(first) if first.environment => {
            eprintln!(
                "rectangle-input-isolation: environment diagnostic before injection: {}; GetLastError={}",
                first.message,
                unsafe { GetLastError().0 }
            );
            // Retry before receiver creation on a genuinely fresh UI thread;
            // never switch desktops or retry after sending native input.
            let retry = std::thread::Builder::new()
                .name("rectangle-isolation-ui-retry".into())
                .spawn(session)
                .map_err(|error| {
                    platform(format!("not_run_environment: fresh UI thread: {error}"))
                })?
                .join()
                .map_err(|_| platform("rectangle input fixture UI thread panicked"))?;
            retry.map_err(|error| {
                platform(format!(
                    "{}: initial={}; fresh_ui_thread={}",
                    if error.environment {
                        "not_run_environment"
                    } else {
                        "native_fixture_failed"
                    },
                    first.message,
                    error.message
                ))
            })
        }
        Err(error) => Err(platform(format!(
            "native_fixture_failed: {}",
            error.message
        ))),
    }
}
#[derive(Default)]
struct ReceiverState {
    down: Cell<usize>,
    up: Cell<usize>,
    commands: Cell<usize>,
    fence: Cell<usize>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Snapshot {
    down: usize,
    up: usize,
    commands: usize,
    selection: (u32, u32),
}
struct Receiver {
    hwnd: HWND,
    edit: HWND,
    button: HWND,
    state: Box<ReceiverState>,
    original_cursor: POINT,
    last_cursor: POINT,
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
            state.commands.set(state.commands.get() + 1);
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
        state.down.set(state.down.get() + 1);
    }
    if msg == WM_LBUTTONUP {
        state.up.set(state.up.get() + 1);
    }
}
fn idle_keys(allow_left: bool) -> bool {
    idle_keys_with_escape(allow_left, false)
}
fn idle_keys_with_escape(allow_left: bool, allow_escape: bool) -> bool {
    (1..=255).all(|key| {
        (allow_left && key == i32::from(VK_LBUTTON.0))
            || (allow_escape && key == i32::from(VK_ESCAPE.0))
            || unsafe { GetAsyncKeyState(key) } >= 0
    })
}
impl Receiver {
    fn create() -> Result<Self, Failure> {
        if !idle_keys(false) {
            return Err(Failure::environment(
                "native keys/buttons are not idle; no input sent",
            ));
        }
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
        let mut state = Box::<ReceiverState>::default();
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
        let mut receiver = Self {
            hwnd,
            edit: HWND::default(),
            button: HWND::default(),
            state,
            original_cursor: POINT::default(),
            last_cursor: POINT::default(),
        };
        unsafe { GetCursorPos(&mut receiver.original_cursor) }
            .map_err(|error| Failure::environment(format!("initial cursor query: {error}")))?;
        receiver.last_cursor = receiver.original_cursor;
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
        let data = receiver.state.as_ref() as *const ReceiverState as usize;
        for child in [receiver.edit, receiver.button] {
            if !unsafe { SetWindowSubclass(child, Some(child_proc), 1, data) }.as_bool() {
                return Err(Failure::environment(
                    "owned control subclass installation failed",
                ));
            }
        }
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
        }
        let deadline = Instant::now() + TIMEOUT;
        while !receiver.foreground() {
            pump();
            if Instant::now() >= deadline {
                return Err(Failure::environment(
                    "exact receiver HWND/PID could not become foreground; no input sent",
                ));
            }
            std::thread::yield_now();
        }
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
    fn guard(&self, allow_left: bool) -> Result<(), Failure> {
        if !self.foreground() {
            return Err(Failure::failed(
                "exact owned receiver HWND/PID lost foreground; refusing input",
            ));
        }
        if !idle_keys(allow_left) {
            return Err(Failure::failed(
                "unexpected held keys/buttons; refusing input",
            ));
        }
        Ok(())
    }
    fn position(
        &mut self,
        child: HWND,
        x: i32,
        y: i32,
        allow_left: bool,
    ) -> Result<MkPoint, Failure> {
        self.guard(allow_left)?;
        let mut point = POINT { x, y };
        if !unsafe { ClientToScreen(child, &mut point) }.as_bool() {
            return Err(Failure::failed("owned client-to-screen failed"));
        }
        unsafe { SetCursorPos(point.x, point.y) }
            .map_err(|error| Failure::failed(format!("owned cursor move: {error}")))?;
        self.last_cursor = point;
        self.guard(allow_left)?;
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
            down: self.state.down.get(),
            up: self.state.up.get(),
            commands: self.state.commands.get(),
            selection: (start, end),
        }
    }
    fn save_owned_client(&self) -> Result<(), Failure> {
        self.guard(false)?;
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
        let path = std::path::Path::new("target/refinement-evidence/m2-receiver.png");
        std::fs::create_dir_all(path.parent().unwrap())
            .map_err(|error| Failure::failed(error.to_string()))?;
        image
            .save(path)
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
        if self.foreground()
            && idle_keys(false)
            && unsafe { GetCursorPos(&mut current) }.is_ok()
            && current.x == self.last_cursor.x
            && current.y == self.last_cursor.y
        {
            unsafe {
                let _ = SetCursorPos(self.original_cursor.x, self.original_cursor.y);
            }
        }
        unsafe {
            if IsWindow(self.hwnd).as_bool() {
                let _ = DestroyWindow(self.hwnd);
            }
        }
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
    fn press(receiver: &'a Receiver) -> Result<Self, Failure> {
        Self::insert(receiver, mouse(true), mouse(false), false)
    }
    fn press_escape(receiver: &'a Receiver) -> Result<Self, Failure> {
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
        Self::insert(receiver, key(false), key(true), true)
    }
    fn insert(
        receiver: &'a Receiver,
        down: INPUT,
        up: INPUT,
        escape: bool,
    ) -> Result<Self, Failure> {
        receiver.guard(escape)?;
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
        if !receiver.foreground() || !idle_keys_with_escape(true, escape) {
            return Err(Failure::failed(
                "foreground/input guard changed after accepted Down; matching Up cleanup is owned",
            ));
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
        }
        Ok(())
    }
    fn release(&mut self) -> Result<(), Failure> {
        let guard_ok = self.receiver.foreground() && idle_keys_with_escape(true, self.escape);
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
) -> Result<(), Failure> {
    let mut escape = HeldLeft::press_escape(receiver)?;
    let deadline = Instant::now() + TIMEOUT;
    let result = loop {
        if !receiver.foreground() || !idle_keys_with_escape(true, true) {
            break Err(Failure::failed(
                "owned receiver foreground/idle guard changed during Escape",
            ));
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
fn wait_drag(controller: &mut VisualOverlayController, receiver: &Receiver) -> Result<(), Failure> {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        receiver.guard(true)?;
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
fn session() -> Result<(), Failure> {
    let mut receiver = Receiver::create()?;
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
    let baseline = receiver.snapshot();
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
    let start = receiver.position(receiver.edit, 12, 16, false)?;
    let mut held = HeldLeft::press(&receiver)?;
    wait_drag(&mut controller, &receiver)?;
    // Receiver is immutably borrowed by HeldLeft. Only move within its owned
    // client, repeating foreground/idle checks at every injection boundary.
    receiver.guard(true)?;
    let mut end = POINT { x: 260, y: 45 };
    if !unsafe { ClientToScreen(receiver.edit, &mut end) }.as_bool() {
        return Err(Failure::failed("owned drag conversion"));
    }
    unsafe { SetCursorPos(end.x, end.y) }.map_err(|error| Failure::failed(error.to_string()))?;
    wait_drag(&mut controller, &receiver)?;
    held.release()?;
    drop(held);
    receiver.last_cursor = end;
    let events = wait_event(&mut controller)?;
    let expected = normalized_rect(start, MkPoint { x: end.x, y: end.y })?;
    if !matches!(events.as_slice(), [VisualOverlayEvent::RectangleConfirmed { operation_id, purpose: RectanglePurpose::PinboardCapture, rect }] if *operation_id == id && *rect == expected)
    {
        return Err(Failure::failed(format!(
            "unexpected capture result: {events:?}; expected {expected:?}"
        )));
    }
    receiver.fence()?;
    if receiver.snapshot() != baseline {
        return Err(Failure::failed(format!(
            "native drag leaked to owned receiver: before={baseline:?}, after={:?}",
            receiver.snapshot()
        )));
    }
    eprintln!(
        "rectangle-input-isolation: PASS injected Down/drag/Up: exact rectangle {expected:?}, receiver Down/Up/commands/edit selection unchanged"
    );
    let id = controller.begin_rectangle_pick(RectanglePurpose::PinboardCapture, desktop);
    receiver.position(receiver.button, 40, 15, false)?;
    let mut held = HeldLeft::press(&receiver)?;
    wait_drag(&mut controller, &receiver)?;
    held_escape_cancel(&receiver, &mut controller)?;
    held.release()?;
    drop(held);
    let events = wait_event(&mut controller)?;
    if events != vec![VisualOverlayEvent::Cancelled { operation_id: id }] {
        return Err(Failure::failed(format!("held cancel result: {events:?}")));
    }
    receiver.fence()?;
    if receiver.snapshot() != baseline {
        return Err(Failure::failed("held Escape/Up leaked into receiver"));
    }
    let mut held = HeldLeft::press(&receiver)?;
    held.release()?;
    drop(held);
    let deadline = Instant::now() + TIMEOUT;
    while receiver.state.up.get() == baseline.up
        || receiver.state.commands.get() == baseline.commands
    {
        receiver.guard(false)?;
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
    if after.down != baseline.down + 1
        || after.up != baseline.up + 1
        || after.commands <= baseline.commands
    {
        return Err(Failure::failed(format!(
            "next ordinary click did not reach dummy button: {baseline:?} -> {after:?}"
        )));
    }
    eprintln!(
        "rectangle-input-isolation: PASS injected held Escape/release: no leak; next ordinary click reached owned button; counts={after:?}"
    );
    if let Err(error) = receiver.save_owned_client() {
        eprintln!(
            "rectangle-input-isolation: screenshot_not_run: {}",
            error.message
        );
    }
    controller.shutdown();
    Ok(())
}
