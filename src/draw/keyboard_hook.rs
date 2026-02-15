use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    U,
    R,
    Escape,
    D,
    H,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyModifiers {
    pub ctrl: bool,
    pub shift: bool,
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
        (KeyCode::R, KeyModifiers { ctrl: true, .. }) => Some(KeyCommand::Redo),
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
        GetAsyncKeyState, VK_CONTROL, VK_D, VK_ESCAPE, VK_H, VK_R, VK_SHIFT, VK_U,
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
        KeyModifiers { ctrl, shift }
    }

    fn map_vk_to_keycode(vk_code: u32) -> KeyCode {
        match vk_code {
            code if code == VK_U.0 as u32 => KeyCode::U,
            code if code == VK_R.0 as u32 => KeyCode::R,
            code if code == VK_ESCAPE.0 as u32 => KeyCode::Escape,
            code if code == VK_D.0 as u32 => KeyCode::D,
            code if code == VK_H.0 as u32 => KeyCode::H,
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
                    key: KeyCode::R,
                    modifiers: KeyModifiers {
                        ctrl: true,
                        shift: false,
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
