use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
    KeyG,
    KeyH,
    KeyI,
    KeyJ,
    KeyK,
    KeyL,
    KeyM,
    KeyN,
    KeyO,
    KeyP,
    KeyQ,
    KeyR,
    KeyS,
    KeyT,
    U,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ,
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    Space,
    Tab,
    Enter,
    Backspace,
    Delete,
    CapsLock,
    Home,
    End,
    PageUp,
    PageDown,
    Left,
    Right,
    Up,
    Down,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    Escape,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyModifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub win: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: KeyCode,
    pub modifiers: KeyModifiers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCommand {
    Undo,
    Redo,
    RequestExit,
}

pub fn should_consume_key_event(active: bool, _event: KeyEvent) -> bool {
    active
}

pub fn map_key_event_to_command(active: bool, event: KeyEvent) -> Option<KeyCommand> {
    if !should_consume_key_event(active, event) {
        return None;
    }

    match (event.key, event.modifiers) {
        (KeyCode::Escape, _) => Some(KeyCommand::RequestExit),
        (KeyCode::U, KeyModifiers { ctrl: false, .. }) => Some(KeyCommand::Undo),
        (KeyCode::KeyR, KeyModifiers { ctrl: true, .. }) => Some(KeyCommand::Redo),
        _ => None,
    }
}

#[derive(Debug, Default)]
pub struct KeyboardHook {
    active: bool,
    #[cfg(windows)]
    backend: platform::KeyboardHookBackend,
}

impl KeyboardHook {
    pub fn activate(&mut self) -> Result<()> {
        if self.active {
            return Ok(());
        }

        #[cfg(windows)]
        self.backend.install()?;

        self.active = true;
        Ok(())
    }

    pub fn deactivate(&mut self) {
        if !self.active {
            return;
        }

        #[cfg(windows)]
        if let Err(err) = self.backend.uninstall() {
            tracing::warn!(?err, "failed to uninstall draw keyboard hook");
        }

        self.active = false;
    }

    pub fn is_active(&self) -> bool {
        #[cfg(windows)]
        {
            self.active && self.backend.is_installed()
        }
        #[cfg(not(windows))]
        {
            self.active
        }
    }

    pub fn drain_events(&self) -> Vec<KeyEvent> {
        #[cfg(windows)]
        {
            return self.backend.drain_events();
        }
        #[cfg(not(windows))]
        {
            Vec::new()
        }
    }
}

impl Drop for KeyboardHook {
    fn drop(&mut self) {
        self.deactivate();
    }
}

#[cfg(windows)]
mod platform {
    use super::{map_key_event_to_command, KeyCode, KeyEvent, KeyModifiers};
    use anyhow::{anyhow, Result};
    use once_cell::sync::Lazy;
    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::sync::Mutex;
    use std::thread::JoinHandle;
    use std::time::Duration;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_BACK, VK_CAPITAL, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE,
        VK_F1, VK_F10, VK_F11, VK_F12, VK_F13, VK_F14, VK_F15, VK_F16, VK_F17, VK_F18, VK_F19,
        VK_F2, VK_F20, VK_F21, VK_F22, VK_F23, VK_F24, VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8,
        VK_F9, VK_HOME, VK_LEFT, VK_LWIN, VK_MENU, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_RWIN,
        VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
    };

    static KEY_EVENT_SENDER: Lazy<Mutex<Option<Sender<KeyEvent>>>> = Lazy::new(|| Mutex::new(None));

    #[derive(Debug)]
    struct HookThread {
        thread_id: u32,
        join: JoinHandle<()>,
    }

    #[derive(Debug, Default)]
    pub struct KeyboardHookBackend {
        hook_thread: Option<HookThread>,
        receiver: Option<Receiver<KeyEvent>>,
    }

    unsafe impl Send for KeyboardHookBackend {}

    impl KeyboardHookBackend {
        pub fn install(&mut self) -> Result<()> {
            if self.hook_thread.is_some() {
                return Ok(());
            }

            let (event_tx, event_rx) = channel::<KeyEvent>();
            if let Ok(mut guard) = KEY_EVENT_SENDER.lock() {
                *guard = Some(event_tx);
            }

            use windows::Win32::System::LibraryLoader::GetModuleHandleW;
            use windows::Win32::System::Threading::GetCurrentThreadId;
            use windows::Win32::UI::WindowsAndMessaging::{
                DispatchMessageW, GetMessageW, PeekMessageW, SetWindowsHookExW, TranslateMessage,
                UnhookWindowsHookEx, MSG, PM_NOREMOVE, WH_KEYBOARD_LL,
            };

            let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<Result<u32>>(1);

            let join = std::thread::spawn(move || {
                let mut msg = MSG::default();
                unsafe {
                    let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
                }

                let thread_id = unsafe { GetCurrentThreadId() };
                let hmodule = match unsafe { GetModuleHandleW(None) } {
                    Ok(h) => h,
                    Err(err) => {
                        let _ = ready_tx.send(Err(anyhow!(err)));
                        return;
                    }
                };

                let keyboard_hook = match unsafe {
                    SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), hmodule, 0)
                } {
                    Ok(h) if !h.0.is_null() => h,
                    Ok(_) => {
                        let _ = ready_tx.send(Err(anyhow!(windows::core::Error::from_win32())));
                        return;
                    }
                    Err(err) => {
                        let _ = ready_tx.send(Err(anyhow!(err)));
                        return;
                    }
                };

                let _ = ready_tx.send(Ok(thread_id));

                loop {
                    let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
                    if r.0 <= 0 {
                        break;
                    }
                    unsafe {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }

                unsafe {
                    let _ = UnhookWindowsHookEx(keyboard_hook);
                }
            });

            let thread_id = ready_rx
                .recv_timeout(Duration::from_secs(2))
                .map_err(|_| anyhow!("keyboard hook thread did not signal readiness"))??;

            self.receiver = Some(event_rx);
            self.hook_thread = Some(HookThread { thread_id, join });
            Ok(())
        }

        pub fn uninstall(&mut self) -> Result<()> {
            if let Ok(mut guard) = KEY_EVENT_SENDER.lock() {
                *guard = None;
            }

            if let Some(th) = self.hook_thread.take() {
                use windows::Win32::Foundation::{LPARAM, WPARAM};
                use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
                unsafe {
                    let _ = PostThreadMessageW(th.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
                }
                let _ = th.join.join();
            }

            self.receiver = None;
            Ok(())
        }

        pub fn is_installed(&self) -> bool {
            self.hook_thread.is_some()
        }

        pub fn drain_events(&self) -> Vec<KeyEvent> {
            let mut events = Vec::new();
            if let Some(rx) = &self.receiver {
                while let Ok(event) = rx.try_recv() {
                    events.push(event);
                }
            }
            events
        }
    }

    fn key_modifiers_snapshot() -> KeyModifiers {
        let ctrl = unsafe { GetAsyncKeyState(VK_CONTROL.0 as i32) } < 0;
        let shift = unsafe { GetAsyncKeyState(VK_SHIFT.0 as i32) } < 0;
        let alt = unsafe { GetAsyncKeyState(VK_MENU.0 as i32) } < 0;
        let win = unsafe { GetAsyncKeyState(VK_LWIN.0 as i32) } < 0
            || unsafe { GetAsyncKeyState(VK_RWIN.0 as i32) } < 0;
        KeyModifiers {
            ctrl,
            shift,
            alt,
            win,
        }
    }

    fn map_vk_to_keycode(vk_code: u32) -> KeyCode {
        if (0x41..=0x5A).contains(&vk_code) {
            return match vk_code {
                0x41 => KeyCode::KeyA,
                0x42 => KeyCode::KeyB,
                0x43 => KeyCode::KeyC,
                0x44 => KeyCode::KeyD,
                0x45 => KeyCode::KeyE,
                0x46 => KeyCode::KeyF,
                0x47 => KeyCode::KeyG,
                0x48 => KeyCode::KeyH,
                0x49 => KeyCode::KeyI,
                0x4A => KeyCode::KeyJ,
                0x4B => KeyCode::KeyK,
                0x4C => KeyCode::KeyL,
                0x4D => KeyCode::KeyM,
                0x4E => KeyCode::KeyN,
                0x4F => KeyCode::KeyO,
                0x50 => KeyCode::KeyP,
                0x51 => KeyCode::KeyQ,
                0x52 => KeyCode::KeyR,
                0x53 => KeyCode::KeyS,
                0x54 => KeyCode::KeyT,
                0x55 => KeyCode::U,
                0x56 => KeyCode::KeyV,
                0x57 => KeyCode::KeyW,
                0x58 => KeyCode::KeyX,
                0x59 => KeyCode::KeyY,
                0x5A => KeyCode::KeyZ,
                _ => KeyCode::Other,
            };
        }
        if (0x30..=0x39).contains(&vk_code) {
            return match vk_code {
                0x30 => KeyCode::Num0,
                0x31 => KeyCode::Num1,
                0x32 => KeyCode::Num2,
                0x33 => KeyCode::Num3,
                0x34 => KeyCode::Num4,
                0x35 => KeyCode::Num5,
                0x36 => KeyCode::Num6,
                0x37 => KeyCode::Num7,
                0x38 => KeyCode::Num8,
                0x39 => KeyCode::Num9,
                _ => KeyCode::Other,
            };
        }

        match vk_code {
            code if code == VK_ESCAPE.0 as u32 => KeyCode::Escape,
            code if code == VK_SPACE.0 as u32 => KeyCode::Space,
            code if code == VK_TAB.0 as u32 => KeyCode::Tab,
            code if code == VK_RETURN.0 as u32 => KeyCode::Enter,
            code if code == VK_BACK.0 as u32 => KeyCode::Backspace,
            code if code == VK_DELETE.0 as u32 => KeyCode::Delete,
            code if code == VK_CAPITAL.0 as u32 => KeyCode::CapsLock,
            code if code == VK_HOME.0 as u32 => KeyCode::Home,
            code if code == VK_END.0 as u32 => KeyCode::End,
            code if code == VK_PRIOR.0 as u32 => KeyCode::PageUp,
            code if code == VK_NEXT.0 as u32 => KeyCode::PageDown,
            code if code == VK_LEFT.0 as u32 => KeyCode::Left,
            code if code == VK_RIGHT.0 as u32 => KeyCode::Right,
            code if code == VK_UP.0 as u32 => KeyCode::Up,
            code if code == VK_DOWN.0 as u32 => KeyCode::Down,
            code if code == VK_F1.0 as u32 => KeyCode::F1,
            code if code == VK_F2.0 as u32 => KeyCode::F2,
            code if code == VK_F3.0 as u32 => KeyCode::F3,
            code if code == VK_F4.0 as u32 => KeyCode::F4,
            code if code == VK_F5.0 as u32 => KeyCode::F5,
            code if code == VK_F6.0 as u32 => KeyCode::F6,
            code if code == VK_F7.0 as u32 => KeyCode::F7,
            code if code == VK_F8.0 as u32 => KeyCode::F8,
            code if code == VK_F9.0 as u32 => KeyCode::F9,
            code if code == VK_F10.0 as u32 => KeyCode::F10,
            code if code == VK_F11.0 as u32 => KeyCode::F11,
            code if code == VK_F12.0 as u32 => KeyCode::F12,
            code if code == VK_F13.0 as u32 => KeyCode::F13,
            code if code == VK_F14.0 as u32 => KeyCode::F14,
            code if code == VK_F15.0 as u32 => KeyCode::F15,
            code if code == VK_F16.0 as u32 => KeyCode::F16,
            code if code == VK_F17.0 as u32 => KeyCode::F17,
            code if code == VK_F18.0 as u32 => KeyCode::F18,
            code if code == VK_F19.0 as u32 => KeyCode::F19,
            code if code == VK_F20.0 as u32 => KeyCode::F20,
            code if code == VK_F21.0 as u32 => KeyCode::F21,
            code if code == VK_F22.0 as u32 => KeyCode::F22,
            code if code == VK_F23.0 as u32 => KeyCode::F23,
            code if code == VK_F24.0 as u32 => KeyCode::F24,
            _ => KeyCode::Other,
        }
    }

    unsafe extern "system" fn keyboard_hook_proc(
        n_code: i32,
        w_param: windows::Win32::Foundation::WPARAM,
        l_param: windows::Win32::Foundation::LPARAM,
    ) -> windows::Win32::Foundation::LRESULT {
        use windows::Win32::UI::WindowsAndMessaging::{
            CallNextHookEx, HC_ACTION, KBDLLHOOKSTRUCT, KBDLLHOOKSTRUCT_FLAGS, WM_KEYDOWN,
            WM_SYSKEYDOWN,
        };

        if n_code == HC_ACTION as i32 {
            let msg = w_param.0 as u32;
            if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
                let info = unsafe { &*(l_param.0 as *const KBDLLHOOKSTRUCT) };
                let injected =
                    (info.flags & KBDLLHOOKSTRUCT_FLAGS(0x10)) != KBDLLHOOKSTRUCT_FLAGS(0);
                if !injected {
                    let event = KeyEvent {
                        key: map_vk_to_keycode(info.vkCode),
                        modifiers: key_modifiers_snapshot(),
                    };

                    if let Ok(guard) = KEY_EVENT_SENDER.lock() {
                        if let Some(sender) = guard.as_ref() {
                            let _ = sender.send(event);
                        }
                    }

                    if map_key_event_to_command(true, event).is_some() {
                        return windows::Win32::Foundation::LRESULT(1);
                    }
                }
            }
        }

        CallNextHookEx(
            windows::Win32::UI::WindowsAndMessaging::HHOOK(std::ptr::null_mut()),
            n_code,
            w_param,
            l_param,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_u_to_undo() {
        assert_eq!(
            map_key_event_to_command(
                true,
                KeyEvent {
                    key: KeyCode::U,
                    modifiers: KeyModifiers::default(),
                },
            ),
            Some(KeyCommand::Undo)
        );
    }

    #[test]
    fn maps_ctrl_r_to_redo() {
        assert_eq!(
            map_key_event_to_command(
                true,
                KeyEvent {
                    key: KeyCode::KeyR,
                    modifiers: KeyModifiers {
                        ctrl: true,
                        shift: false,
                        alt: false,
                        win: false,
                    },
                },
            ),
            Some(KeyCommand::Redo)
        );
    }

    #[test]
    fn maps_escape_to_exit_request() {
        assert_eq!(
            map_key_event_to_command(
                true,
                KeyEvent {
                    key: KeyCode::Escape,
                    modifiers: KeyModifiers::default(),
                },
            ),
            Some(KeyCommand::RequestExit)
        );
    }

    #[test]
    fn does_not_map_non_matching_keys_or_inactive_state() {
        assert_eq!(
            map_key_event_to_command(
                true,
                KeyEvent {
                    key: KeyCode::Other,
                    modifiers: KeyModifiers::default(),
                },
            ),
            None
        );
        assert_eq!(
            map_key_event_to_command(
                false,
                KeyEvent {
                    key: KeyCode::Escape,
                    modifiers: KeyModifiers::default(),
                },
            ),
            None
        );
    }

    #[test]
    fn enter_draw_mode_hook_active_exit_draw_mode_hook_inactive() {
        let mut hook = KeyboardHook::default();
        assert!(!hook.is_active());

        hook.activate()
            .expect("hook activate should not fail in tests");
        assert!(hook.is_active());

        hook.deactivate();
        assert!(!hook.is_active());
    }
}
