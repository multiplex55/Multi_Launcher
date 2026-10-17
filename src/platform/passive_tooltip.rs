//! Thread-bound, input-transparent native tracking tooltip. No worker/service.

#[cfg(windows)]
mod native {
    use std::{marker::PhantomData, rc::Rc};
    use windows::Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Gdi::{CreateFontIndirectW, DeleteObject, HFONT, LOGFONTW},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::{
                ICC_WIN95_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx, TOOLTIPS_CLASSW,
                TTDT_AUTOPOP, TTDT_INITIAL, TTDT_RESHOW, TTF_ABSOLUTE, TTF_TRACK, TTM_ADDTOOLW,
                TTM_GETBUBBLESIZE, TTM_SETDELAYTIME, TTM_SETMAXTIPWIDTH, TTM_TRACKACTIVATE,
                TTM_UPDATETIPTEXTW, TTS_ALWAYSTIP, TTS_NOPREFIX, TTTOOLINFOW,
            },
            Shell::{DefSubclassProc, SetWindowSubclass},
            WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, GetWindowRect, HTTRANSPARENT, LWA_ALPHA,
                MA_NOACTIVATE, SWP_NOACTIVATE, SWP_NOZORDER, SendMessageW,
                SetLayeredWindowAttributes, SetWindowPos, WINDOW_EX_STYLE, WINDOW_STYLE,
                WM_MOUSEACTIVATE, WM_NCHITTEST, WM_SETFONT, WS_EX_LAYERED, WS_EX_NOACTIVATE,
                WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
            },
        },
    };
    use windows::core::{PCWSTR, PWSTR};

    #[derive(Debug)]
    struct OwnedWindow(HWND);
    impl Drop for OwnedWindow {
        fn drop(&mut self) {
            // All owners are !Send and are created, used and dropped on one thread.
            let _ = unsafe { DestroyWindow(self.0) };
        }
    }
    #[derive(Debug)]
    struct OwnedFont(HFONT);
    impl Drop for OwnedFont {
        fn drop(&mut self) {
            let _ = unsafe { DeleteObject(self.0) };
        }
    }

    unsafe extern "system" fn input_passthrough(
        hwnd: HWND,
        message: u32,
        w: WPARAM,
        l: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        match message {
            WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            _ => unsafe { DefSubclassProc(hwnd, message, w, l) },
        }
    }

    fn tooltip_styles() -> WINDOW_EX_STYLE {
        // Layered + transparent is required for cross-thread/process pass-through;
        // HTTRANSPARENT alone only forwards to windows on the same thread.
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED
    }

    #[derive(Debug)]
    pub(crate) struct PassiveTooltip {
        // Destruction order is deliberate: tooltip first, then owner, then font.
        window: OwnedWindow,
        _owner: OwnedWindow,
        font: Option<OwnedFont>,
        tool: TTTOOLINFOW,
        text: Vec<u16>,
        visible: bool,
        position: Option<(i32, i32)>,
        dpi: Option<u32>,
        _thread_bound: PhantomData<Rc<()>>,
    }

    impl PassiveTooltip {
        pub(crate) fn new() -> Option<Self> {
            let controls = INITCOMMONCONTROLSEX {
                dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_WIN95_CLASSES,
            };
            if !unsafe { InitCommonControlsEx(&controls) }.as_bool() {
                return None;
            }
            let instance = unsafe { GetModuleHandleW(PCWSTR::null()) }.ok()?;
            let owner_class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let owner = OwnedWindow(
                unsafe {
                    CreateWindowExW(
                        WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
                        PCWSTR(owner_class.as_ptr()),
                        PCWSTR::null(),
                        WS_POPUP,
                        0,
                        0,
                        0,
                        0,
                        None,
                        None,
                        instance,
                        None,
                    )
                }
                .ok()?,
            );
            let window = OwnedWindow(
                unsafe {
                    CreateWindowExW(
                        tooltip_styles(),
                        TOOLTIPS_CLASSW,
                        PCWSTR::null(),
                        WS_POPUP | WINDOW_STYLE(TTS_ALWAYSTIP) | WINDOW_STYLE(TTS_NOPREFIX),
                        0,
                        0,
                        0,
                        0,
                        owner.0,
                        None,
                        instance,
                        None,
                    )
                }
                .ok()?,
            );
            unsafe {
                SetLayeredWindowAttributes(
                    window.0,
                    windows::Win32::Foundation::COLORREF(0),
                    255,
                    LWA_ALPHA,
                )
            }
            .ok()?;
            if !unsafe { SetWindowSubclass(window.0, Some(input_passthrough), 1, 0) }.as_bool() {
                return None;
            }
            let mut text = vec![0];
            let tool = TTTOOLINFOW {
                cbSize: std::mem::size_of::<TTTOOLINFOW>() as u32,
                hwnd: owner.0,
                uId: 1,
                uFlags: TTF_TRACK | TTF_ABSOLUTE,
                lpszText: PWSTR(text.as_mut_ptr()),
                ..Default::default()
            };
            // SendMessage completes pointer use before the stack value can move.
            if unsafe {
                SendMessageW(
                    window.0,
                    TTM_ADDTOOLW,
                    WPARAM(0),
                    LPARAM(&tool as *const _ as isize),
                )
            }
            .0 == 0
            {
                return None;
            }
            for (kind, delay) in [(TTDT_INITIAL, 0), (TTDT_RESHOW, 0), (TTDT_AUTOPOP, 10_000)] {
                unsafe {
                    SendMessageW(
                        window.0,
                        TTM_SETDELAYTIME,
                        WPARAM(kind as usize),
                        LPARAM(delay),
                    )
                };
            }
            unsafe { SendMessageW(window.0, TTM_SETMAXTIPWIDTH, WPARAM(0), LPARAM(600)) };
            Some(Self {
                window,
                _owner: owner,
                font: None,
                tool,
                text,
                visible: false,
                position: None,
                dpi: None,
                _thread_bound: PhantomData,
            })
        }

        pub(crate) fn set_text(&mut self, value: &str) {
            if self.text[..self.text.len().saturating_sub(1)]
                .iter()
                .copied()
                .eq(value.encode_utf16())
            {
                return;
            }
            let replacement: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
            // Keep the old buffer alive until the control has accepted its replacement.
            let old = std::mem::replace(&mut self.text, replacement);
            self.tool.lpszText = PWSTR(self.text.as_mut_ptr());
            unsafe {
                SendMessageW(
                    self.window.0,
                    TTM_UPDATETIPTEXTW,
                    WPARAM(0),
                    LPARAM(&self.tool as *const _ as isize),
                )
            };
            drop(old);
        }

        pub(crate) fn set_dpi(&mut self, dpi: u32) -> Option<()> {
            if self.dpi == Some(dpi) {
                return Some(());
            }
            if !(48..=768).contains(&dpi) {
                return None;
            }
            let mut description = LOGFONTW {
                lfHeight: -((13.0 * dpi as f32 / 96.0).round() as i32),
                ..Default::default()
            };
            for (slot, unit) in description
                .lfFaceName
                .iter_mut()
                .zip("Segoe UI\0".encode_utf16())
            {
                *slot = unit;
            }
            let font = unsafe { CreateFontIndirectW(&description) };
            if font.0.is_null() {
                return None;
            }
            let font = OwnedFont(font);
            unsafe {
                SendMessageW(
                    self.window.0,
                    WM_SETFONT,
                    WPARAM(font.0.0 as usize),
                    LPARAM(1),
                )
            };
            self.font = Some(font);
            unsafe {
                SendMessageW(
                    self.window.0,
                    TTM_SETMAXTIPWIDTH,
                    WPARAM(0),
                    LPARAM((320 * dpi / 96) as isize),
                )
            };
            self.dpi = Some(dpi);
            self.position = None;
            Some(())
        }

        pub(crate) fn size(&self) -> Option<(i32, i32)> {
            let packed = unsafe {
                SendMessageW(
                    self.window.0,
                    TTM_GETBUBBLESIZE,
                    WPARAM(0),
                    LPARAM(&self.tool as *const _ as isize),
                )
            }
            .0 as u32;
            let size = ((packed & 0xffff) as i32, (packed >> 16) as i32);
            (size.0 > 0 && size.1 > 0).then_some(size)
        }

        pub(crate) fn move_to(&mut self, position: (f32, f32)) {
            self.position = Some((position.0.round() as i32, position.1.round() as i32));
        }

        pub(crate) fn show(&mut self) -> Option<()> {
            let position = self.position?;
            self.show_at(position)
        }

        pub(crate) fn show_at(&mut self, position: (i32, i32)) -> Option<()> {
            if !self.visible {
                unsafe {
                    SendMessageW(
                        self.window.0,
                        TTM_TRACKACTIVATE,
                        WPARAM(1),
                        LPARAM(&self.tool as *const _ as isize),
                    )
                };
                self.visible = true;
            }
            let mut rect = windows::Win32::Foundation::RECT::default();
            unsafe { GetWindowRect(self.window.0, &mut rect) }.ok()?;
            let size = self.size()?;
            if (
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
            ) != (position.0, position.1, size.0, size.1)
            {
                // Full signed coordinates; never use TTM_TRACKPOSITION's 16-bit halves.
                // Preserve existing z order instead of repeatedly promoting this surface.
                unsafe {
                    SetWindowPos(
                        self.window.0,
                        None,
                        position.0,
                        position.1,
                        size.0,
                        size.1,
                        SWP_NOACTIVATE | SWP_NOZORDER,
                    )
                }
                .ok()?;
            }
            self.position = Some(position);
            Some(())
        }

        pub(crate) fn hide(&mut self) {
            if self.visible {
                unsafe {
                    SendMessageW(
                        self.window.0,
                        TTM_TRACKACTIVATE,
                        WPARAM(0),
                        LPARAM(&self.tool as *const _ as isize),
                    )
                };
                self.visible = false;
            }
        }
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn passive_tooltip_native_policy_is_layered_transparent_and_nonactivating() {
            let required = WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW;
            assert_eq!(tooltip_styles().0 & required.0, required.0);
        }
        #[test]
        fn passive_tooltip_native_callback_declines_mouse_hits_and_activation() {
            // The handled messages do not touch a HWND or call a native API.
            assert_eq!(
                unsafe {
                    input_passthrough(HWND::default(), WM_NCHITTEST, WPARAM(0), LPARAM(0), 1, 0)
                },
                LRESULT(HTTRANSPARENT as isize)
            );
            assert_eq!(
                unsafe {
                    input_passthrough(
                        HWND::default(),
                        WM_MOUSEACTIVATE,
                        WPARAM(0),
                        LPARAM(0),
                        1,
                        0,
                    )
                },
                LRESULT(MA_NOACTIVATE as isize)
            );
        }
    }
}

#[cfg(windows)]
pub(crate) use native::PassiveTooltip;
