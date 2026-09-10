//! Keyboard-layout translation for recording. This boundary is invoked by the
//! recorder processor, never by a hook callback or an egui frame.

use super::MkKey;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyTranslation {
    Text(String),
    DeadKey,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardTranslationRequest {
    pub key: MkKey,
    pub vk: u32,
    pub scan_code: u32,
    pub extended: bool,
    /// Win32-compatible state captured immediately after applying the key-down.
    pub key_state: [u8; 256],
    pub keyboard_layout: Option<isize>,
}

pub trait KeyboardTranslator: Send {
    fn initial_key_state(&mut self) -> [u8; 256] {
        [0; 256]
    }
    fn translate(&mut self, request: &KeyboardTranslationRequest) -> KeyTranslation;
}

#[derive(Default)]
pub struct SystemKeyboardTranslator;

#[cfg(windows)]
impl KeyboardTranslator for SystemKeyboardTranslator {
    fn initial_key_state(&mut self) -> [u8; 256] {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, GetKeyState};
        let mut state = [0; 256];
        // GetKeyboardState is queue/thread-local and the processor owns no UI
        // queue. Async state supplies physical high bits; GetKeyState supplies
        // the toggle bits that matter to layout translation.
        for (vk, slot) in state.iter_mut().enumerate() {
            if unsafe { GetAsyncKeyState(vk as i32) } < 0 {
                *slot |= 0x80;
            }
            if matches!(vk, 0x14 | 0x90 | 0x91) && unsafe { GetKeyState(vk as i32) } & 1 != 0 {
                *slot |= 1;
            }
        }
        state
    }

    fn translate(&mut self, request: &KeyboardTranslationRequest) -> KeyTranslation {
        use windows::Win32::UI::Input::KeyboardAndMouse::HKL;
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayout, ToUnicodeEx};

        let mut buffer = [0u16; 8];
        let count = unsafe {
            ToUnicodeEx(
                request.vk,
                request.scan_code,
                &request.key_state,
                &mut buffer,
                // Do not mutate the calling thread's dead-key buffer. A dead
                // key therefore stays a conservative physical action.
                0x4,
                request
                    .keyboard_layout
                    .map_or_else(|| GetKeyboardLayout(0), |layout| HKL(layout as *mut _)),
            )
        };
        if count < 0 {
            KeyTranslation::DeadKey
        } else if count == 0 {
            KeyTranslation::None
        } else {
            String::from_utf16(&buffer[..count as usize])
                .ok()
                .filter(|text| !text.is_empty())
                .map(KeyTranslation::Text)
                .unwrap_or(KeyTranslation::None)
        }
    }
}

#[cfg(not(windows))]
impl KeyboardTranslator for SystemKeyboardTranslator {
    fn translate(&mut self, _: &KeyboardTranslationRequest) -> KeyTranslation {
        KeyTranslation::None
    }
}
