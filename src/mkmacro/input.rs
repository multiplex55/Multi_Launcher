//! Production input synthesis. This is intentionally independent of legacy `actions::keys`.
use super::{
    DiagnosticKind, ExecResult, ExecutionDiagnostic, InputBackend, MkKey, MkMouseButton, MkPoint,
    MkTextMode, MkTextPayload,
};
use std::time::{Duration, Instant};

/// Stable marker used to identify (and ignore while recording) mkmacro input.
pub const MKMACRO_EXTRA_INFO: usize = 0x4D4B_4D41_4352_4F01;
pub const KEYEVENTF_EXTENDEDKEY_: u32 = 0x0001;
pub const KEYEVENTF_KEYUP_: u32 = 0x0002;
pub const KEYEVENTF_UNICODE_: u32 = 0x0004;
pub const KEYEVENTF_SCANCODE_: u32 = 0x0008;
pub const MOUSEEVENTF_MOVE_: u32 = 0x0001;
pub const MOUSEEVENTF_LEFTDOWN_: u32 = 0x0002;
pub const MOUSEEVENTF_LEFTUP_: u32 = 0x0004;
pub const MOUSEEVENTF_RIGHTDOWN_: u32 = 0x0008;
pub const MOUSEEVENTF_RIGHTUP_: u32 = 0x0010;
pub const MOUSEEVENTF_MIDDLEDOWN_: u32 = 0x0020;
pub const MOUSEEVENTF_MIDDLEUP_: u32 = 0x0040;
pub const MOUSEEVENTF_XDOWN_: u32 = 0x0080;
pub const MOUSEEVENTF_XUP_: u32 = 0x0100;
pub const MOUSEEVENTF_WHEEL_: u32 = 0x0800;
pub const MOUSEEVENTF_HWHEEL_: u32 = 0x1000;
pub const MOUSEEVENTF_VIRTUALDESK_: u32 = 0x4000;
pub const MOUSEEVENTF_ABSOLUTE_: u32 = 0x8000;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawInputEvent {
    Keyboard {
        vk: u16,
        scan: u16,
        flags: u32,
        extra: usize,
    },
    Mouse {
        dx: i32,
        dy: i32,
        data: u32,
        flags: u32,
        extra: usize,
    },
}
pub trait InputSink: Send + Sync {
    fn send(&self, events: &[RawInputEvent]) -> Result<usize, String>;
}
fn rejected(wanted: usize, sent: usize, detail: impl Into<String>) -> ExecutionDiagnostic {
    ExecutionDiagnostic::new(
        DiagnosticKind::InputRejected,
        format!(
            "SendInput accepted {sent} of {wanted} event(s): {}{}",
            if sent == 0 {
                " likely integrity/UIPI restrictions; "
            } else {
                ""
            },
            detail.into()
        ),
    )
    .context("requested", wanted.to_string())
    .context("sent", sent.to_string())
}
#[derive(Clone)]
pub struct Win32InputBackend<S = SystemInputSink> {
    sink: S,
}
/// Deliberate capability required to construct the backend that can affect the
/// user's real desktop. Tests should instead use `FakeBackend` or `with_sink`.
/// Keeping this token out of `Default` prevents an acceptance harness from
/// accidentally turning a harmless fixture into live `SendInput` calls.
#[derive(Debug, Clone, Copy)]
pub struct LiveInputOptIn(());
impl LiveInputOptIn {
    /// Explicitly opts into destructive, production input synthesis.
    pub fn production() -> Self {
        Self(())
    }
}
impl Win32InputBackend<SystemInputSink> {
    pub fn system(_: LiveInputOptIn) -> Self {
        Self {
            sink: SystemInputSink(()),
        }
    }
}
impl<S> Win32InputBackend<S> {
    pub fn with_sink(sink: S) -> Self {
        Self { sink }
    }
}
impl<S: InputSink> Win32InputBackend<S> {
    fn emit(&self, e: &[RawInputEvent]) -> ExecResult {
        let n = self.sink.send(e).map_err(|x| rejected(e.len(), 0, x))?;
        if n != e.len() {
            Err(rejected(e.len(), n, "the operating system rejected input"))
        } else {
            Ok(())
        }
    }
    fn key_event(&self, key: &MkKey, up: bool) -> ExecResult {
        let (vk, scan, extended) = key_metadata(key)?;
        let mut flags = if up { KEYEVENTF_KEYUP_ } else { 0 };
        if scan != 0 {
            flags |= KEYEVENTF_SCANCODE_
        }
        if extended {
            flags |= KEYEVENTF_EXTENDEDKEY_
        }
        self.emit(&[RawInputEvent::Keyboard {
            vk,
            scan,
            flags,
            extra: MKMACRO_EXTRA_INFO,
        }])
    }
    pub fn unicode_text(&self, text: &str) -> ExecResult {
        let mut e = Vec::new();
        for unit in text.encode_utf16() {
            e.push(RawInputEvent::Keyboard {
                vk: 0,
                scan: unit,
                flags: KEYEVENTF_UNICODE_,
                extra: MKMACRO_EXTRA_INFO,
            });
            e.push(RawInputEvent::Keyboard {
                vk: 0,
                scan: unit,
                flags: KEYEVENTF_UNICODE_ | KEYEVENTF_KEYUP_,
                extra: MKMACRO_EXTRA_INFO,
            });
        }
        self.emit(&e)
    }
    pub fn key_press(&self, key: &MkKey) -> ExecResult {
        self.key_event(key, false)?;
        self.key_event(key, true)
    }
    pub fn chord(&self, keys: &[MkKey]) -> ExecResult {
        let mut down: Vec<&MkKey> = Vec::new();
        for key in keys {
            if let Err(primary) = self.key_event(key, false) {
                for owned in down.iter().rev() {
                    let _ = self.key_event(owned, true);
                }
                return Err(primary);
            }
            down.push(key);
        }
        let mut primary = None;
        for key in down.into_iter().rev() {
            if let Err(e) = self.key_event(key, true)
                && primary.is_none()
            {
                primary = Some(e);
            }
        }
        primary.map_or(Ok(()), Err)
    }
    pub fn horizontal_scroll(&self, delta: i32) -> ExecResult {
        self.mouse(0, 0, delta as u32, MOUSEEVENTF_HWHEEL_)
    }
    fn mouse(&self, dx: i32, dy: i32, data: u32, flags: u32) -> ExecResult {
        self.emit(&[RawInputEvent::Mouse {
            dx,
            dy,
            data,
            flags,
            extra: MKMACRO_EXTRA_INFO,
        }])
    }
}
fn key_metadata(k: &MkKey) -> ExecResult<(u16, u16, bool)> {
    let v = match k {
        MkKey::Character(s) if s.len() == 1 => s.as_bytes()[0].to_ascii_uppercase() as u16,
        MkKey::Enter => 0x0D,
        MkKey::Tab => 9,
        MkKey::Escape => 0x1B,
        MkKey::Space => 0x20,
        MkKey::Backspace => 8,
        MkKey::Delete => 0x2E,
        MkKey::Up => 0x26,
        MkKey::Down => 0x28,
        MkKey::Left => 0x25,
        MkKey::Right => 0x27,
        MkKey::Home => 0x24,
        MkKey::End => 0x23,
        MkKey::PageUp => 0x21,
        MkKey::PageDown => 0x22,
        MkKey::Control => 0x11,
        MkKey::LeftControl => 0xA2,
        MkKey::RightControl => 0xA3,
        MkKey::Alt => 0x12,
        MkKey::LeftAlt => 0xA4,
        MkKey::RightAlt => 0xA5,
        MkKey::Shift => 0x10,
        MkKey::LeftShift => 0xA0,
        MkKey::RightShift => 0xA1,
        MkKey::Meta | MkKey::LeftMeta => 0x5B,
        MkKey::RightMeta => 0x5C,
        MkKey::Function(n @ 1..=24) => 0x6F + *n as u16,
        _ => {
            return Err(ExecutionDiagnostic::new(
                DiagnosticKind::InvalidTarget,
                "key has no virtual-key representation",
            ));
        }
    };
    let ext = matches!(
        k,
        MkKey::Delete
            | MkKey::Up
            | MkKey::Down
            | MkKey::Left
            | MkKey::Right
            | MkKey::Home
            | MkKey::End
            | MkKey::PageUp
            | MkKey::PageDown
            | MkKey::Meta
            | MkKey::LeftMeta
            | MkKey::RightMeta
            | MkKey::RightControl
            | MkKey::RightAlt
    );
    Ok((v, 0, ext))
}
impl<S: InputSink> InputBackend for Win32InputBackend<S> {
    fn key_down(&self, k: &MkKey) -> ExecResult {
        self.key_event(k, false)
    }
    fn key_up(&self, k: &MkKey) -> ExecResult {
        self.key_event(k, true)
    }
    fn button_down(&self, b: MkMouseButton) -> ExecResult {
        let (f, d) = button(b, false);
        self.mouse(0, 0, d, f)
    }
    fn button_up(&self, b: MkMouseButton) -> ExecResult {
        let (f, d) = button(b, true);
        self.mouse(0, 0, d, f)
    }
    fn move_mouse(&self, p: MkPoint) -> ExecResult {
        let (x, y) = super::normalize_absolute(p, system_virtual_desktop()?)?;
        self.mouse(
            x,
            y,
            0,
            MOUSEEVENTF_MOVE_ | MOUSEEVENTF_ABSOLUTE_ | MOUSEEVENTF_VIRTUALDESK_,
        )
    }
    fn scroll(&self, d: i32) -> ExecResult {
        self.mouse(0, 0, d as u32, MOUSEEVENTF_WHEEL_)
    }
    fn text(&self, p: &MkTextPayload) -> ExecResult {
        match p.mode {
            MkTextMode::Type => self.unicode_text(&p.text),
            MkTextMode::Paste => Err(ExecutionDiagnostic::new(
                DiagnosticKind::UnsupportedOperation,
                "clipboard paste mode is not enabled; use Unicode text",
            )),
        }
    }
}

#[cfg(windows)]
fn system_virtual_desktop() -> ExecResult<super::VirtualDesktop> {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };
    Ok(super::VirtualDesktop {
        x: unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) },
        y: unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) },
        width: unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) },
        height: unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) },
    })
}
#[cfg(not(windows))]
fn system_virtual_desktop() -> ExecResult<super::VirtualDesktop> {
    Ok(super::VirtualDesktop {
        x: 0,
        y: 0,
        width: 65_536,
        height: 65_536,
    })
}
fn button(b: MkMouseButton, up: bool) -> (u32, u32) {
    match (b, up) {
        (MkMouseButton::Left, false) => (MOUSEEVENTF_LEFTDOWN_, 0),
        (MkMouseButton::Left, true) => (MOUSEEVENTF_LEFTUP_, 0),
        (MkMouseButton::Right, false) => (MOUSEEVENTF_RIGHTDOWN_, 0),
        (MkMouseButton::Right, true) => (MOUSEEVENTF_RIGHTUP_, 0),
        (MkMouseButton::Middle, false) => (MOUSEEVENTF_MIDDLEDOWN_, 0),
        (MkMouseButton::Middle, true) => (MOUSEEVENTF_MIDDLEUP_, 0),
        (MkMouseButton::X1, false) => (MOUSEEVENTF_XDOWN_, 1),
        (MkMouseButton::X1, true) => (MOUSEEVENTF_XUP_, 1),
        (MkMouseButton::X2, false) => (MOUSEEVENTF_XDOWN_, 2),
        (MkMouseButton::X2, true) => (MOUSEEVENTF_XUP_, 2),
    }
}

/// Time-based movement capped at 120 updates/second. Cancellation is checked
/// before every update; the number of events depends on time, not pixel count.
pub fn smooth_move(
    backend: &dyn InputBackend,
    control: &super::RunControl,
    from: MkPoint,
    to: MkPoint,
    duration: Duration,
) -> ExecResult {
    if duration.is_zero() {
        return backend.move_mouse(to);
    }
    let started = Instant::now();
    let tick = Duration::from_micros(8_333);
    loop {
        control.checkpoint()?;
        let elapsed = started.elapsed();
        let t = (elapsed.as_secs_f64() / duration.as_secs_f64()).min(1.0);
        backend.move_mouse(MkPoint {
            x: (from.x as f64 + (to.x - from.x) as f64 * t).round() as i32,
            y: (from.y as f64 + (to.y - from.y) as f64 * t).round() as i32,
        })?;
        if t >= 1.0 {
            return Ok(());
        }
        control.wait(tick.min(duration.saturating_sub(elapsed)))?;
    }
}

/// Holds a button during smooth movement and always attempts release. A release
/// failure is returned only when it is the primary error, so it cannot hide a
/// movement/cancellation error.
pub fn drag(
    backend: &dyn InputBackend,
    control: &super::RunControl,
    button: MkMouseButton,
    from: MkPoint,
    to: MkPoint,
    duration: Duration,
) -> ExecResult {
    backend.move_mouse(from)?;
    backend.button_down(button.clone())?;
    let primary = smooth_move(backend, control, from, to, duration);
    let release = backend.button_up(button);
    match (primary, release) {
        (Err(e), _) => Err(e),
        (Ok(()), result) => result,
    }
}
#[derive(Clone, Copy)]
pub struct SystemInputSink(());
#[cfg(not(windows))]
impl InputSink for SystemInputSink {
    fn send(&self, _: &[RawInputEvent]) -> Result<usize, String> {
        Err("SendInput is available only on Windows".into())
    }
}
#[cfg(windows)]
impl InputSink for SystemInputSink {
    fn send(&self, events: &[RawInputEvent]) -> Result<usize, String> {
        use windows::Win32::UI::Input::KeyboardAndMouse::*;
        let native: Vec<INPUT> = events
            .iter()
            .map(|e| match *e {
                RawInputEvent::Keyboard {
                    vk,
                    scan,
                    flags,
                    extra,
                } => INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(vk),
                            wScan: scan,
                            dwFlags: KEYBD_EVENT_FLAGS(flags),
                            time: 0,
                            dwExtraInfo: extra,
                        },
                    },
                },
                RawInputEvent::Mouse {
                    dx,
                    dy,
                    data,
                    flags,
                    extra,
                } => INPUT {
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 {
                        mi: MOUSEINPUT {
                            dx,
                            dy,
                            mouseData: data,
                            dwFlags: MOUSE_EVENT_FLAGS(flags),
                            time: 0,
                            dwExtraInfo: extra,
                        },
                    },
                },
            })
            .collect();
        let n = unsafe { SendInput(&native, std::mem::size_of::<INPUT>() as i32) } as usize;
        if n == 0 {
            Err(windows::core::Error::from_win32().to_string())
        } else {
            Ok(n)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    #[derive(Default)]
    struct Sink(Mutex<Vec<RawInputEvent>>);
    impl InputSink for &Sink {
        fn send(&self, e: &[RawInputEvent]) -> Result<usize, String> {
            self.0.lock().unwrap().extend_from_slice(e);
            Ok(e.len())
        }
    }
    #[test]
    fn unicode_surrogates_have_down_up() {
        let s = Sink::default();
        Win32InputBackend::with_sink(&s).unicode_text("😀").unwrap();
        let e = s.0.lock().unwrap();
        assert_eq!(e.len(), 4);
        assert!(e.iter().all(|x| match x {
            RawInputEvent::Keyboard { extra, .. } => *extra == MKMACRO_EXTRA_INFO,
            _ => false,
        }));
    }
    #[test]
    fn x2_data_and_flags() {
        let s = Sink::default();
        let b = Win32InputBackend::with_sink(&s);
        b.button_down(MkMouseButton::X2).unwrap();
        assert!(matches!(
            s.0.lock().unwrap()[0],
            RawInputEvent::Mouse {
                data: 2,
                flags: MOUSEEVENTF_XDOWN_,
                ..
            }
        ));
    }
}
