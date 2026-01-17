use sysinfo::System;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PowerPlan {
    pub guid: String,
    pub name: String,
    pub active: bool,
}

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

pub fn process_switch(pid: u32) {
    super::super::window_manager::activate_process(pid);
}

pub fn window_switch(hwnd: isize) {
    use windows::Win32::Foundation::HWND;
    super::super::window_manager::force_restore_and_foreground(HWND(hwnd as _));
}

pub fn window_close(hwnd: isize) {
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
    unsafe {
        let _ = PostMessageW(HWND(hwnd as _), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}

pub fn set_brightness(v: u32) {
    super::super::launcher::set_display_brightness(v);
}

pub fn set_volume(v: u32) {
    super::super::launcher::set_system_volume(v);
}

pub fn toggle_system_mute() {
    super::super::launcher::toggle_system_mute();
}

pub fn mute_active_window() {
    super::super::launcher::mute_active_window();
}

pub fn set_process_volume(pid: u32, level: u32) {
    super::super::launcher::set_process_volume(pid, level);
}

pub fn toggle_process_mute(pid: u32) {
    super::super::launcher::toggle_process_mute(pid);
}

pub fn get_system_volume() -> Option<u8> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
        use windows::Win32::Media::Audio::{
            eMultimedia, eRender, IMMDeviceEnumerator, MMDeviceEnumerator,
        };
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
        };

        unsafe {
            let mut percent = None;
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            if let Ok(enm) =
                CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
            {
                if let Ok(device) = enm.GetDefaultAudioEndpoint(eRender, eMultimedia) {
                    if let Ok(vol) = device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) {
                        if let Ok(val) = vol.GetMasterVolumeLevelScalar() {
                            percent = Some((val * 100.0).round() as u8);
                        }
                    }
                }
            }
            CoUninitialize();
            percent
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

pub fn get_system_mute() -> Option<bool> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
        use windows::Win32::Media::Audio::{
            eMultimedia, eRender, IMMDeviceEnumerator, MMDeviceEnumerator,
        };
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
        };

        unsafe {
            let mut muted = None;
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            if let Ok(enm) =
                CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
            {
                if let Ok(device) = enm.GetDefaultAudioEndpoint(eRender, eMultimedia) {
                    if let Ok(vol) = device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) {
                        if let Ok(val) = vol.GetMute() {
                            muted = Some(val.as_bool());
                        }
                    }
                }
            }
            CoUninitialize();
            muted
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

pub fn get_main_display_brightness() -> Option<u8> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Devices::Display::{
            DestroyPhysicalMonitors, GetMonitorBrightness, GetNumberOfPhysicalMonitorsFromHMONITOR,
            GetPhysicalMonitorsFromHMONITOR, PHYSICAL_MONITOR,
        };
        use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
        use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};

        unsafe extern "system" fn enum_monitors(
            hmonitor: HMONITOR,
            _hdc: HDC,
            _rect: *mut RECT,
            lparam: LPARAM,
        ) -> BOOL {
            let percent_ptr = lparam.0 as *mut u32;
            let mut count: u32 = 0;
            if GetNumberOfPhysicalMonitorsFromHMONITOR(hmonitor, &mut count).is_ok() {
                let mut monitors = vec![PHYSICAL_MONITOR::default(); count as usize];
                if GetPhysicalMonitorsFromHMONITOR(hmonitor, &mut monitors).is_ok() {
                    if let Some(m) = monitors.first() {
                        let mut min = 0u32;
                        let mut cur = 0u32;
                        let mut max = 0u32;
                        if GetMonitorBrightness(m.hPhysicalMonitor, &mut min, &mut cur, &mut max)
                            != 0
                        {
                            if max > min {
                                *percent_ptr = ((cur - min) * 100 / (max - min)) as u32;
                            } else {
                                *percent_ptr = 0;
                            }
                        }
                    }
                    let _ = DestroyPhysicalMonitors(&monitors);
                }
            }
            false.into()
        }

        let mut percent: u32 = 50;
        unsafe {
            let _ = EnumDisplayMonitors(
                HDC(std::ptr::null_mut()),
                None,
                Some(enum_monitors),
                LPARAM(&mut percent as *mut u32 as isize),
            );
        }
        Some(percent as u8)
    }

    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

pub fn get_power_plans() -> Result<Vec<PowerPlan>, String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;

        let output = Command::new("powercfg")
            .arg("/L")
            .output()
            .map_err(|err| format!("Failed to query power plans: {err}"))?;
        if !output.status.success() {
            return Err("Failed to query power plans.".into());
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let plans = parse_powercfg_list(&stdout);
        if plans.is_empty() {
            Err("No power plans detected.".into())
        } else {
            Ok(plans)
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        Err("Power plans are not supported on this OS.".into())
    }
}

pub fn set_power_plan(guid: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;

        Command::new("powercfg").arg("/S").arg(guid).status()?;
        return Ok(());
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = guid;
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn parse_powercfg_list(output: &str) -> Vec<PowerPlan> {
    let mut plans = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("Power Scheme GUID:") else {
            continue;
        };
        let rest = rest.trim();
        let mut parts = rest.splitn(2, ' ');
        let guid = parts.next().unwrap_or("").trim();
        let details = parts.next().unwrap_or("").trim();
        if guid.is_empty() {
            continue;
        }
        let name = if let Some(start) = details.find('(') {
            if let Some(end) = details[start + 1..].find(')') {
                details[start + 1..start + 1 + end].trim()
            } else {
                details
            }
        } else {
            details
        };
        let active = details.contains('*');
        plans.push(PowerPlan {
            guid: guid.to_string(),
            name: name.to_string(),
            active,
        });
    }
    plans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "windows")]
    fn parse_powercfg_output() {
        let sample = r#"
Power Scheme GUID: 381b4222-f694-41f0-9685-ff5bb260df2e  (Balanced) *
Power Scheme GUID: 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c  (High performance)
Power Scheme GUID: a1841308-3541-4fab-bc81-f71556f20b4a  (Power saver)
"#;
        let plans = parse_powercfg_list(sample);
        assert_eq!(plans.len(), 3);
        assert_eq!(plans[0].name, "Balanced");
        assert!(plans[0].active);
        assert_eq!(plans[1].guid, "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c");
        assert!(!plans[1].active);
    }
}

pub fn recycle_clean() {
    // Emptying the recycle bin can take a noticeable amount of time on
    // Windows. Running it on the current thread would block the UI and
    // cause `launch_action` to return slowly, which in turn makes the
    // `recycle_plugin` test fail. Spawn a background thread instead so the
    // command returns immediately while the cleanup happens asynchronously.
    //
    // To keep callers responsive, dispatch a success event right away and
    // perform the actual cleanup in the background. Any errors from the
    // cleanup are ignored since we have already notified listeners.
    std::thread::spawn(|| {
        let _ = super::super::launcher::clean_recycle_bin();
    });
    crate::gui::send_event(crate::gui::WatchEvent::Recycle(Ok(())));
}

pub fn browser_tab_switch(runtime_id: &[i32]) {
    use windows::core::VARIANT;
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
            COINIT_APARTMENTTHREADED,
        };
        use windows::Win32::System::Ole::{
            SafeArrayCreateVector, SafeArrayDestroy, SafeArrayPutElement,
        };
        use windows::Win32::System::Variant::VT_I4;
        use windows::Win32::UI::Accessibility::*;
        use windows::Win32::Foundation::{HWND, POINT};
        use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, SetCursorPos};
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEINPUT, MOUSEEVENTF_LEFTDOWN,
            MOUSEEVENTF_LEFTUP,
        };

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
                                                                    let x = (rect.left + rect.right) / 2;
                                                                    let y = (rect.top + rect.bottom) / 2;

                                                                    let mut hwnd = elem
                                                                        .CurrentNativeWindowHandle()
                                                                        .unwrap_or(HWND(std::ptr::null_mut()));
                                                                    if hwnd.0.is_null() {
                                                                        if let Ok(walker) =
                                                                            automation.RawViewWalker()
                                                                        {
                                                                            let mut cur = elem.clone();
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
                                                                        core::mem::size_of::<INPUT>()
                                                                            as i32,
                                                                    );
                                                                    let _ = SetCursorPos(old.x, old.y);
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
