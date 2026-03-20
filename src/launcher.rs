pub(crate) fn set_system_volume(percent: u32) {
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        eMultimedia, eRender, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if let Ok(enm) =
            CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
        {
            if let Ok(device) = enm.GetDefaultAudioEndpoint(eRender, eMultimedia) {
                if let Ok(vol) = device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) {
                    let _ =
                        vol.SetMasterVolumeLevelScalar(percent as f32 / 100.0, std::ptr::null());
                }
            }
        }
        CoUninitialize();
    }
}

pub(crate) fn toggle_system_mute() {
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        eMultimedia, eRender, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if let Ok(enm) =
            CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
        {
            if let Ok(device) = enm.GetDefaultAudioEndpoint(eRender, eMultimedia) {
                if let Ok(vol) = device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) {
                    if let Ok(val) = vol.GetMute() {
                        let _ = vol.SetMute(!val.as_bool(), std::ptr::null());
                    }
                }
            }
        }
        CoUninitialize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launcher::parse::{parse_action_kind, ActionKind};

    #[test]
    fn parse_volume_pid_toggle_mute() {
        let action = Action {
            label: String::new(),
            desc: String::new(),
            action: "volume:pid_toggle_mute:42".into(),
            args: None,
        };
        assert_eq!(
            parse_action_kind(&action),
            ActionKind::VolumeToggleMuteProcess { pid: 42 }
        );
    }

    #[test]
    fn parse_volume_toggle_mute() {
        let action = Action {
            label: String::new(),
            desc: String::new(),
            action: "volume:toggle_mute".into(),
            args: None,
        };
        assert_eq!(parse_action_kind(&action), ActionKind::VolumeToggleMute);
    }

    #[test]
    fn parse_power_plan_set() {
        let action = Action {
            label: String::new(),
            desc: String::new(),
            action: "power:plan:set:balanced".into(),
            args: None,
        };
        assert_eq!(
            parse_action_kind(&action),
            ActionKind::PowerPlanSet { guid: "balanced" }
        );
    }

    #[test]
    fn parse_todo_add_payload_with_delimiters_and_whitespace() {
        let payload = crate::plugins::todo::TodoAddActionPayload {
            text: "ship | release, notes now".into(),
            priority: 9,
            tags: vec!["team|alpha,beta".into(), "has space".into()],
            refs: Vec::new(),
        };
        let encoded = crate::plugins::todo::encode_todo_add_action_payload(&payload)
            .expect("encode todo add payload");
        let action = Action {
            label: String::new(),
            desc: String::new(),
            action: format!("todo:add:{encoded}"),
            args: None,
        };

        assert_eq!(
            parse_action_kind(&action),
            ActionKind::TodoAdd {
                text: "ship | release, notes now".into(),
                priority: 9,
                tags: vec!["team|alpha,beta".into(), "has space".into()],
                refs: Vec::new(),
            }
        );
    }

    #[test]
    fn parse_todo_tag_payload_with_delimiters_and_whitespace() {
        let payload = crate::plugins::todo::TodoTagActionPayload {
            idx: 12,
            tags: vec!["owner|dev,ops".into(), "needs review".into()],
        };
        let encoded = crate::plugins::todo::encode_todo_tag_action_payload(&payload)
            .expect("encode todo tag payload");
        let action = Action {
            label: String::new(),
            desc: String::new(),
            action: format!("todo:tag:{encoded}"),
            args: None,
        };

        assert_eq!(
            parse_action_kind(&action),
            ActionKind::TodoSetTags {
                idx: 12,
                tags: vec!["owner|dev,ops".into(), "needs review".into()],
            }
        );
    }
}

pub(crate) fn mute_active_window() {
    use windows::core::Interface;
    use windows::Win32::Media::Audio::{
        eMultimedia, eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
        ISimpleAudioVolume, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if let Ok(enm) =
            CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
        {
            if let Ok(device) = enm.GetDefaultAudioEndpoint(eRender, eMultimedia) {
                if let Ok(manager) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) {
                    if let Ok(list) = manager.GetSessionEnumerator() {
                        let count = list.GetCount().unwrap_or(0);
                        for i in 0..count {
                            if let Ok(ctrl) = list.GetSession(i) {
                                if let Ok(c2) = ctrl.cast::<IAudioSessionControl2>() {
                                    if let Ok(session_pid) = c2.GetProcessId() {
                                        if session_pid == pid {
                                            if let Ok(vol) = ctrl.cast::<ISimpleAudioVolume>() {
                                                let _ = vol.SetMute(true, std::ptr::null());
                                            }
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        CoUninitialize();
    }
}

pub(crate) fn set_process_volume(pid: u32, level: u32) {
    use windows::core::Interface;
    use windows::Win32::Media::Audio::{
        eMultimedia, eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
        ISimpleAudioVolume, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };

    let level = level.min(100);
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if let Ok(enm) =
            CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
        {
            if let Ok(device) = enm.GetDefaultAudioEndpoint(eRender, eMultimedia) {
                if let Ok(manager) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) {
                    if let Ok(list) = manager.GetSessionEnumerator() {
                        let count = list.GetCount().unwrap_or(0);
                        for i in 0..count {
                            if let Ok(ctrl) = list.GetSession(i) {
                                if let Ok(c2) = ctrl.cast::<IAudioSessionControl2>() {
                                    if let Ok(session_pid) = c2.GetProcessId() {
                                        if session_pid == pid {
                                            if let Ok(vol) = ctrl.cast::<ISimpleAudioVolume>() {
                                                let _ = vol.SetMasterVolume(
                                                    level as f32 / 100.0,
                                                    std::ptr::null(),
                                                );
                                            }
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        CoUninitialize();
    }
}

pub(crate) fn toggle_process_mute(pid: u32) {
    use windows::core::Interface;
    use windows::Win32::Media::Audio::{
        eMultimedia, eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
        ISimpleAudioVolume, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if let Ok(enm) =
            CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
        {
            if let Ok(device) = enm.GetDefaultAudioEndpoint(eRender, eMultimedia) {
                if let Ok(manager) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) {
                    if let Ok(list) = manager.GetSessionEnumerator() {
                        let count = list.GetCount().unwrap_or(0);
                        for i in 0..count {
                            if let Ok(ctrl) = list.GetSession(i) {
                                if let Ok(c2) = ctrl.cast::<IAudioSessionControl2>() {
                                    if let Ok(session_pid) = c2.GetProcessId() {
                                        if session_pid == pid {
                                            if let Ok(vol) = ctrl.cast::<ISimpleAudioVolume>() {
                                                if let Ok(m) = vol.GetMute() {
                                                    let _ = vol.SetMute(!m, std::ptr::null());
                                                }
                                            }
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        CoUninitialize();
    }
}

pub(crate) fn set_display_brightness(percent: u32) {
    use windows::Win32::Devices::Display::{
        DestroyPhysicalMonitors, GetNumberOfPhysicalMonitorsFromHMONITOR,
        GetPhysicalMonitorsFromHMONITOR, SetMonitorBrightness, PHYSICAL_MONITOR,
    };
    use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};

    unsafe extern "system" fn enum_monitors(
        hmonitor: HMONITOR,
        _hdc: HDC,
        _rect: *mut RECT,
        lparam: LPARAM,
    ) -> BOOL {
        let percent = lparam.0 as u32;
        let mut count: u32 = 0;
        if GetNumberOfPhysicalMonitorsFromHMONITOR(hmonitor, &mut count).is_ok() {
            let mut monitors = vec![PHYSICAL_MONITOR::default(); count as usize];
            if GetPhysicalMonitorsFromHMONITOR(hmonitor, &mut monitors).is_ok() {
                for m in &monitors {
                    let _ = SetMonitorBrightness(m.hPhysicalMonitor, percent);
                }
                let _ = DestroyPhysicalMonitors(&monitors);
            }
        }
        true.into()
    }

    unsafe {
        let _ = EnumDisplayMonitors(
            HDC(std::ptr::null_mut()),
            None,
            Some(enum_monitors),
            LPARAM(percent as isize),
        );
    }
}

pub(crate) fn clean_recycle_bin() -> windows::core::Result<()> {
    use windows::Win32::UI::Shell::{
        SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND,
    };
    unsafe {
        SHEmptyRecycleBinW(
            None,
            None,
            SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RecycleBinInfo {
    pub size_bytes: u64,
    pub items: u64,
}

#[cfg(target_os = "windows")]
pub(crate) fn query_recycle_bin() -> Option<RecycleBinInfo> {
    use windows::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};
    let mut info = SHQUERYRBINFO {
        cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
        ..Default::default()
    };
    let result = unsafe { SHQueryRecycleBinW(None, &mut info) };
    if result.is_ok() {
        Some(RecycleBinInfo {
            size_bytes: info.i64Size.max(0) as u64,
            items: info.i64NumItems.max(0) as u64,
        })
    } else {
        None
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn query_recycle_bin() -> Option<RecycleBinInfo> {
    None
}

pub(crate) fn system_command(action: &str) -> Option<std::process::Command> {
    use std::process::Command;
    match action {
        "shutdown" => {
            let mut c = Command::new("shutdown");
            c.args(["/s", "/t", "0"]);
            Some(c)
        }
        "reboot" => {
            let mut c = Command::new("shutdown");
            c.args(["/r", "/t", "0"]);
            Some(c)
        }
        "lock" => {
            let mut c = Command::new("rundll32.exe");
            c.args(["user32.dll,LockWorkStation"]);
            Some(c)
        }
        "logoff" => {
            let mut c = Command::new("shutdown");
            c.arg("/l");
            Some(c)
        }
        _ => None,
    }
}

pub(crate) mod exec;
pub(crate) mod parse;
pub(crate) mod plan;

pub use exec::launch_action;
