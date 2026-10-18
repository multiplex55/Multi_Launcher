use super::{
    ClipboardWriteFailure, SequencePlaybackError, SequencePlaybackPlatform, SequenceTarget,
    TargetObservation,
};
use crate::mkmacro::input::{LiveInputOptIn, Win32InputBackend};
use std::ptr;
use std::time::Instant;
use windows::Win32::Foundation::{
    ERROR_SUCCESS, GetLastError, GlobalFree, HGLOBAL, HWND, SetLastError,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardSequenceNumber, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::UI::WindowsAndMessaging::{
    ES_PASSWORD, GUITHREADINFO, GWL_STYLE, GetForegroundWindow, GetGUIThreadInfo,
    GetWindowLongPtrW, GetWindowThreadProcessId, IsWindow,
};

const MAX_TEXT_BYTES: usize = 16 * 1024 * 1024;

pub(crate) struct NativeSequencePlaybackPlatform {
    clipboard_owner: Option<HWND>,
    input: Win32InputBackend,
}

impl NativeSequencePlaybackPlatform {
    pub(crate) fn new(clipboard_owner_hwnd: Option<usize>) -> Self {
        Self {
            clipboard_owner: clipboard_owner_hwnd.map(|hwnd| HWND(hwnd as *mut std::ffi::c_void)),
            input: Win32InputBackend::system(LiveInputOptIn::production()),
        }
    }

    fn open_clipboard(&self) -> Result<ClipboardGuard, SequencePlaybackError> {
        let owner = self
            .clipboard_owner
            .ok_or(SequencePlaybackError::ClipboardUnavailable)?;
        ClipboardGuard::open(owner)
    }
}

impl SequencePlaybackPlatform for NativeSequencePlaybackPlatform {
    fn input_is_eligible(&mut self) -> bool {
        let modifiers = paste_modifiers();
        crate::hotkey::launcher_invocation::exclusive_owners() == 0
            && !modifiers.shift
            && !modifiers.alt
            && !modifiers.windows
    }
    fn now(&mut self) -> Instant {
        Instant::now()
    }

    fn clipboard_revision(&mut self) -> Result<u32, SequencePlaybackError> {
        let _clipboard = self.open_clipboard()?;
        let revision = unsafe { GetClipboardSequenceNumber() };
        if revision == 0 {
            return Err(SequencePlaybackError::ClipboardUnavailable);
        }
        Ok(revision)
    }

    fn observe_target(&mut self, target: &SequenceTarget) -> TargetObservation {
        let target_hwnd = target_hwnd(target);
        if target_hwnd.0.is_null() || !unsafe { IsWindow(target_hwnd) }.as_bool() {
            return TargetObservation::Invalid;
        }

        let mut target_process = 0;
        let target_thread =
            unsafe { GetWindowThreadProcessId(target_hwnd, Some(&mut target_process)) };
        if target_process != target.process_id() || target_thread != target.thread_id() {
            return TargetObservation::Invalid;
        }

        let foreground = unsafe { GetForegroundWindow() };
        if foreground.0.is_null() {
            return TargetObservation::Unavailable;
        }
        let mut foreground_process = 0;
        let foreground_thread =
            unsafe { GetWindowThreadProcessId(foreground, Some(&mut foreground_process)) };
        if foreground_process == 0 || foreground_thread == 0 {
            return TargetObservation::Unavailable;
        }
        if foreground_process == std::process::id() {
            return TargetObservation::Launcher;
        }
        if foreground == target_hwnd
            && foreground_process == target.process_id()
            && foreground_thread == target.thread_id()
        {
            return match focused_control_is_password(foreground_thread) {
                Some(password_control) => TargetObservation::Target { password_control },
                None => TargetObservation::Unavailable,
            };
        }
        TargetObservation::Other
    }

    fn current_target(&mut self) -> Result<SequenceTarget, SequencePlaybackError> {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.0.is_null() || !unsafe { IsWindow(hwnd) }.as_bool() {
            return Err(SequencePlaybackError::FocusChanged);
        }
        let mut process_id = 0;
        let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        if process_id == 0 || thread_id == 0 || process_id == std::process::id() {
            return Err(SequencePlaybackError::FocusChanged);
        }
        // Titles are invocation-time template inputs. Target switching must not
        // perform another title read or change any already-rendered step.
        SequenceTarget::from_external_window(
            crate::active_window::ExternalForegroundWindow {
                hwnd: hwnd.0 as isize,
                process_id,
                thread_id,
                title: None,
            },
            std::process::id(),
        )
        .map_err(|_| SequencePlaybackError::InvalidTarget)
    }

    fn publish_text(
        &mut self,
        expected_sequence: u32,
        rollback_text: Option<&str>,
        text: &str,
    ) -> Result<u32, ClipboardWriteFailure> {
        let fail = |error, owned_sequence| ClipboardWriteFailure {
            error,
            owned_sequence,
        };
        let _clipboard = self.open_clipboard().map_err(|error| fail(error, None))?;
        if unsafe { GetClipboardSequenceNumber() } != expected_sequence {
            return Err(fail(SequencePlaybackError::ClipboardChanged, None));
        }
        if unsafe { EmptyClipboard() }.is_err() {
            return Err(fail(SequencePlaybackError::ClipboardWriteFailed, None));
        }

        match write_unicode_text(text) {
            Ok(()) => {
                let sequence = unsafe { GetClipboardSequenceNumber() };
                if sequence == 0 {
                    Err(fail(SequencePlaybackError::ClipboardUnavailable, None))
                } else {
                    Ok(sequence)
                }
            }
            Err(()) => {
                // We still own the clipboard lock. Roll back only sequence-authored
                // text. The pre-run payload is intentionally never captured.
                if let Some(previous) = rollback_text {
                    let _ = write_unicode_text(previous);
                }
                let sequence = unsafe { GetClipboardSequenceNumber() };
                let owned_sequence = (sequence != 0).then_some(sequence);
                Err(fail(
                    SequencePlaybackError::ClipboardWriteFailed,
                    owned_sequence,
                ))
            }
        }
    }

    fn clipboard_sequence_matches(&mut self, expected_sequence: u32) -> bool {
        expected_sequence != 0
            && self.open_clipboard().is_ok_and(|_clipboard| {
                (unsafe { GetClipboardSequenceNumber() }) == expected_sequence
            })
    }

    fn paste_chord(&mut self) -> Result<(), SequencePlaybackError> {
        super::paste_input::paste_chord(&self.input, paste_modifiers())
    }
}

fn paste_modifiers() -> super::paste_input::PasteModifiers {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    let pressed = |key| unsafe { GetAsyncKeyState(key) < 0 };
    super::paste_input::PasteModifiers {
        control: pressed(0xA2) || pressed(0xA3),
        shift: pressed(0xA0) || pressed(0xA1),
        alt: pressed(0xA4) || pressed(0xA5),
        windows: pressed(0x5B) || pressed(0x5C),
    }
}

fn target_hwnd(target: &SequenceTarget) -> HWND {
    HWND(target.hwnd() as *mut std::ffi::c_void)
}

struct ClipboardGuard;

impl ClipboardGuard {
    fn open(owner: HWND) -> Result<Self, SequencePlaybackError> {
        unsafe { OpenClipboard(owner) }.map_err(|_| SequencePlaybackError::ClipboardUnavailable)?;
        Ok(Self)
    }
}

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

fn write_unicode_text(text: &str) -> Result<(), ()> {
    let mut units = text.encode_utf16().collect::<Vec<_>>();
    units.push(0);
    let byte_count = units.len().saturating_mul(std::mem::size_of::<u16>());
    if byte_count > MAX_TEXT_BYTES {
        return Err(());
    }
    let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, byte_count) }.map_err(|_| ())?;
    let pointer = unsafe { GlobalLock(memory) }.cast::<u16>();
    if pointer.is_null() {
        let _ = unsafe { GlobalFree(memory) };
        return Err(());
    }
    // SAFETY: the allocation has byte_count bytes and units contains exactly
    // byte_count / 2 initialized UTF-16 code units.
    unsafe { ptr::copy_nonoverlapping(units.as_ptr(), pointer, units.len()) };
    let _ = unsafe { GlobalUnlock(memory) };
    let handle = windows::Win32::Foundation::HANDLE(memory.0);
    if unsafe { SetClipboardData(u32::from(CF_UNICODETEXT.0), handle) }.is_err() {
        let _ = unsafe { GlobalFree(memory) };
        return Err(());
    }
    Ok(())
}

fn focused_control_is_password(thread_id: u32) -> Option<bool> {
    let mut information = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    if unsafe { GetGUIThreadInfo(thread_id, &mut information) }.is_err()
        || information.hwndFocus.0.is_null()
    {
        return None;
    }
    // A failed style query is unknown, never evidence of an unprotected control.
    unsafe { SetLastError(ERROR_SUCCESS) };
    let style = unsafe { GetWindowLongPtrW(information.hwndFocus, GWL_STYLE) };
    if style == 0 && unsafe { GetLastError() } != ERROR_SUCCESS {
        return None;
    }
    Some(style as u32 & ES_PASSWORD as u32 != 0)
}
