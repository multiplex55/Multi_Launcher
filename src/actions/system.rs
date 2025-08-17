use sysinfo::System;

pub fn run_system(cmd: &str) -> anyhow::Result<()> {
    if let Some(mut command) = super::super::launcher::system_command(cmd) {
        command.spawn().map(|_| ()).map_err(|e| e.into())
    } else {
        Ok(())
    }
}

pub fn process_kill(pid: u32) {
    let system = System::new_all();
    if let Some(process) = system.process(sysinfo::Pid::from_u32(pid)) {
        let _ = process.kill();
    }
}

#[cfg_attr(not(target_os = "windows"), allow(unused_variables))]
pub fn process_switch(pid: u32) {
    #[cfg(target_os = "windows")]
    {
        super::super::window_manager::activate_process(pid);
    }
}

#[cfg_attr(not(target_os = "windows"), allow(unused_variables))]
pub fn window_switch(hwnd: isize) {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::HWND;
        super::super::window_manager::force_restore_and_foreground(HWND(hwnd as _));
    }
}

#[cfg_attr(not(target_os = "windows"), allow(unused_variables))]
pub fn window_close(hwnd: isize) {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
        unsafe {
            let _ = PostMessageW(HWND(hwnd as _), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

#[cfg_attr(not(target_os = "windows"), allow(unused_variables))]
pub fn set_brightness(v: u32) {
    #[cfg(target_os = "windows")]
    super::super::launcher::set_display_brightness(v);
}

#[cfg_attr(not(target_os = "windows"), allow(unused_variables))]
pub fn set_volume(v: u32) {
    #[cfg(target_os = "windows")]
    super::super::launcher::set_system_volume(v);
}

pub fn mute_active_window() {
    #[cfg(target_os = "windows")]
    super::super::launcher::mute_active_window();
}

#[cfg_attr(not(target_os = "windows"), allow(unused_variables))]
pub fn set_process_volume(pid: u32, level: u32) {
    #[cfg(target_os = "windows")]
    super::super::launcher::set_process_volume(pid, level);
}

#[cfg_attr(not(target_os = "windows"), allow(unused_variables))]
pub fn toggle_process_mute(pid: u32) {
    #[cfg(target_os = "windows")]
    super::super::launcher::toggle_process_mute(pid);
}

pub fn recycle_clean() {
    #[cfg(target_os = "windows")]
    {
        std::thread::spawn(|| {
            let res = super::super::launcher::clean_recycle_bin().map_err(|e| format!("{e:?}"));
            crate::gui::send_event(crate::gui::WatchEvent::Recycle(res));
        });
    }
}

#[cfg_attr(not(target_os = "windows"), allow(unused_variables))]
pub fn browser_tab_switch(runtime_id: &[i32]) {
    #[cfg(target_os = "windows")]
    {
        use windows::core::VARIANT;
        use windows::Win32::Foundation::{HWND, POINT};
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
            COINIT_APARTMENTTHREADED,
        };
        use windows::Win32::System::Ole::{
            SafeArrayCreateVector, SafeArrayDestroy, SafeArrayPutElement,
        };
        use windows::Win32::System::Variant::VT_I4;
        use windows::Win32::UI::Accessibility::*;
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
            MOUSEINPUT,
        };
        use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, SetCursorPos};

        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            if let Ok(automation) =
                CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
            {
                // Build SAFEARRAY from runtime ID pieces
                let psa = SafeArrayCreateVector(VT_I4, 0, runtime_id.len() as u32);
                if !psa.is_null() {
                    for (i, v) in runtime_id.iter().enumerate() {
                        let mut idx = i as i32;
                        let val = *v;
                        let _ = SafeArrayPutElement(
                            psa,
                            &mut idx,
                            &val as *const _ as *const core::ffi::c_void,
                        );
                    }

                    // Enumerate tab elements and find matching runtime ID
                    if let Ok(cond) = automation.CreatePropertyCondition(
                        UIA_ControlTypePropertyId,
                        &VARIANT::from(UIA_TabItemControlTypeId.0),
                    ) {
                        if let Ok(root) = automation.GetRootElement() {
                            if let Ok(tabs) = root.FindAll(TreeScope_Subtree, &cond) {
                                if let Ok(count) = tabs.Length() {
                                    'outer: for i in 0..count {
                                        if let Ok(elem) = tabs.GetElement(i) {
                                            if let Ok(elem_id) = elem.GetRuntimeId() {
                                                if !elem_id.is_null() {
                                                    if let Ok(same) = automation.CompareRuntimeIds(
                                                        elem_id as *const _,
                                                        psa as *const _,
                                                    ) {
                                                        if same.as_bool() {
                                                            let mut activated = false;
                                                            if let Ok(sel) = elem
                                                                .GetCurrentPatternAs::<
                                                                    IUIAutomationSelectionItemPattern,
                                                                >(UIA_SelectionItemPatternId)
                                                            {
                                                                activated = sel.Select().is_ok();
                                                            } else if let Ok(inv) = elem
                                                                .GetCurrentPatternAs::<
                                                                    IUIAutomationInvokePattern,
                                                                >(UIA_InvokePatternId)
                                                            {
                                                                activated = inv.Invoke().is_ok();
                                                            } else if let Ok(acc) = elem
                                                                .GetCurrentPatternAs::<
                                                                    IUIAutomationLegacyIAccessiblePattern,
                                                                >(UIA_LegacyIAccessiblePatternId)
                                                            {
                                                                activated = acc.DoDefaultAction().is_ok();
                                                            }

                                                            if activated {
                                                                if let Ok(focused) =
                                                                    automation.GetFocusedElement()
                                                                {
                                                                    if let Ok(fid) =
                                                                        focused.GetRuntimeId()
                                                                    {
                                                                        activated = automation
                                                                            .CompareRuntimeIds(
                                                                                fid as *const _,
                                                                                psa as *const _,
                                                                            )
                                                                            .map(|b| b.as_bool())
                                                                            .unwrap_or(false);
                                                                        let _ = SafeArrayDestroy(
                                                                            fid as *const _,
                                                                        );
                                                                    } else {
                                                                        activated = false;
                                                                    }
                                                                } else {
                                                                    activated = false;
                                                                }
                                                            }

                                                            if !activated {
                                                                if let Ok(rect) =
                                                                    elem.CurrentBoundingRectangle()
                                                                {
                                                                    let x = (rect.left
                                                                        + rect.right)
                                                                        / 2;
                                                                    let y = (rect.top
                                                                        + rect.bottom)
                                                                        / 2;

                                                                    let mut hwnd = elem
                                                                        .CurrentNativeWindowHandle()
                                                                        .unwrap_or(HWND(
                                                                            std::ptr::null_mut(),
                                                                        ));
                                                                    if hwnd.0.is_null() {
                                                                        if let Ok(walker) =
                                                                            automation
                                                                                .RawViewWalker()
                                                                        {
                                                                            let mut cur =
                                                                                elem.clone();
                                                                            loop {
                                                                                if let Ok(h) = cur
                                                                                    .CurrentNativeWindowHandle()
                                                                                {
                                                                                    if !h.0.is_null() {
                                                                                        hwnd = h;
                                                                                        break;
                                                                                    }
                                                                                }
                                                                                if let Ok(p) = walker
                                                                                    .GetParentElement(&cur)
                                                                                {
                                                                                    cur = p;
                                                                                } else {
                                                                                    break;
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                    if !hwnd.0.is_null() {
                                                                        super::super::window_manager::force_restore_and_foreground(hwnd);
                                                                    }

                                                                    let mut old = POINT::default();
                                                                    let _ = GetCursorPos(&mut old);
                                                                    let _ = SetCursorPos(x, y);
                                                                    let inputs = [
                                                                        INPUT {
                                                                            r#type: INPUT_MOUSE,
                                                                            Anonymous: INPUT_0 {
                                                                                mi: MOUSEINPUT {
                                                                                    dx: 0,
                                                                                    dy: 0,
                                                                                    mouseData: 0,
                                                                                    dwFlags:
                                                                                        MOUSEEVENTF_LEFTDOWN,
                                                                                    time: 0,
                                                                                    dwExtraInfo: 0,
                                                                                },
                                                                            },
                                                                        },
                                                                        INPUT {
                                                                            r#type: INPUT_MOUSE,
                                                                            Anonymous: INPUT_0 {
                                                                                mi: MOUSEINPUT {
                                                                                    dx: 0,
                                                                                    dy: 0,
                                                                                    mouseData: 0,
                                                                                    dwFlags:
                                                                                        MOUSEEVENTF_LEFTUP,
                                                                                    time: 0,
                                                                                    dwExtraInfo: 0,
                                                                                },
                                                                            },
                                                                        },
                                                                    ];
                                                                    let _ = SendInput(
                                                                        &inputs,
                                                                        core::mem::size_of::<INPUT>(
                                                                        )
                                                                            as i32,
                                                                    );
                                                                    let _ =
                                                                        SetCursorPos(old.x, old.y);
                                                                    tracing::debug!(
                                                                        "simulated click for browser tab"
                                                                    );
                                                                }
                                                            }

                                                            let _ = elem.SetFocus();
                                                            let _ = SafeArrayDestroy(
                                                                elem_id as *const _,
                                                            );
                                                            break 'outer;
                                                        }
                                                    }
                                                    let _ = SafeArrayDestroy(elem_id as *const _);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    let _ = SafeArrayDestroy(psa as *const _);
                }
            }
            CoUninitialize();
        }
    }
}
