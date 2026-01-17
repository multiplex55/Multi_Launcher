use crate::actions::Action;
use crate::plugins::calc_history::{self, CalcHistoryEntry, CALC_HISTORY_FILE, MAX_ENTRIES};

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

#[cfg(test)]
mod tests {
    use super::*;

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

#[derive(Debug, Clone, PartialEq)]
enum ActionKind<'a> {
    Shell {
        cmd: &'a str,
        keep_open: bool,
    },
    ShellAdd {
        name: &'a str,
        args: &'a str,
    },
    ShellRemove(&'a str),
    ClipboardClear,
    ClipboardCopy(usize),
    ClipboardText(&'a str),
    Calc {
        result: &'a str,
        expr: Option<&'a str>,
    },
    CalcHistory(usize),
    BookmarkAdd(&'a str),
    BookmarkRemove(&'a str),
    FolderAdd(&'a str),
    FolderRemove(&'a str),
    HistoryClear,
    HistoryIndex(usize),
    System(&'a str),
    ProcessKill(u32),
    ProcessSwitch(u32),
    TimerCancel(u64),
    TimerPause(u64),
    TimerResume(u64),
    TimerStart {
        dur: &'a str,
        name: &'a str,
    },
    AlarmSet {
        time: &'a str,
        name: &'a str,
    },
    StopwatchPause(u64),
    StopwatchResume(u64),
    StopwatchStop(u64),
    StopwatchStart {
        name: &'a str,
    },
    StopwatchShow(u64),
    TodoAdd {
        text: &'a str,
        priority: u8,
        tags: Vec<String>,
    },
    TodoSetPriority {
        idx: usize,
        priority: u8,
    },
    TodoSetTags {
        idx: usize,
        tags: Vec<String>,
    },
    TodoRemove(usize),
    TodoDone(usize),
    TodoClear,
    TodoExport,
    SnippetRemove(&'a str),
    SnippetEdit(&'a str),
    SnippetAdd {
        alias: &'a str,
        text: &'a str,
    },
    FavAdd {
        label: &'a str,
        command: &'a str,
        args: Option<&'a str>,
    },
    FavRemove(&'a str),
    BrightnessSet(u32),
    VolumeSet(u32),
    VolumeSetProcess {
        pid: u32,
        level: u32,
    },
    VolumeToggleMuteProcess {
        pid: u32,
    },
    VolumeMuteActive,
    Screenshot {
        mode: crate::actions::screenshot::Mode,
        clip: bool,
    },
    MediaPlay,
    MediaPause,
    MediaNext,
    MediaPrev,
    RecycleClean,
    WindowSwitch(isize),
    WindowClose(isize),
    BrowserTabSwitch(Vec<i32>),
    BrowserTabCache,
    BrowserTabClear,
    TempfileNew(Option<&'a str>),
    TempfileOpen,
    TempfileOpenFile(&'a str),
    TempfileClear,
    TempfileRemove(&'a str),
    TempfileAlias {
        path: &'a str,
        alias: &'a str,
    },
    LayoutSave {
        name: &'a str,
        flags: Option<&'a str>,
    },
    LayoutLoad {
        name: &'a str,
        flags: Option<&'a str>,
    },
    LayoutShow {
        name: &'a str,
        flags: Option<&'a str>,
    },
    LayoutRemove {
        name: &'a str,
        flags: Option<&'a str>,
    },
    LayoutList {
        flags: Option<&'a str>,
    },
    LayoutEdit,
    NoteReload,
    WatchlistRefresh,
    WatchlistInit {
        force: bool,
    },
    WatchlistAdd(&'a str),
    WatchlistRemove(&'a str),
    WatchlistSetEnabled {
        id: &'a str,
        enabled: bool,
    },
    WatchlistSetRefresh {
        refresh_ms: u64,
    },
    WatchlistMove {
        id: &'a str,
        direction: &'a str,
    },
    ExecPath {
        path: &'a str,
        args: Option<&'a str>,
    },
    Macro(&'a str),
}

fn parse_action_kind(action: &Action) -> ActionKind<'_> {
    let s = action.action.as_str();
    if let Some(rest) = s.strip_prefix("shell:add:") {
        if let Some((name, args)) = rest.split_once('|') {
            return ActionKind::ShellAdd { name, args };
        }
    }
    if let Some(name) = s.strip_prefix("shell:remove:") {
        return ActionKind::ShellRemove(name);
    }
    if let Some(cmd) = s.strip_prefix("shell_keep:") {
        return ActionKind::Shell {
            cmd,
            keep_open: true,
        };
    }
    if let Some(cmd) = s.strip_prefix("shell:") {
        return ActionKind::Shell {
            cmd,
            keep_open: false,
        };
    }
    if let Some(rest) = s.strip_prefix("clipboard:") {
        if rest == "clear" {
            return ActionKind::ClipboardClear;
        }
        if let Some(idx) = rest.strip_prefix("copy:") {
            if let Ok(i) = idx.parse::<usize>() {
                return ActionKind::ClipboardCopy(i);
            }
        }
        return ActionKind::ClipboardText(rest);
    }
    if let Some(idx) = s.strip_prefix("calc:history:") {
        if let Ok(i) = idx.parse::<usize>() {
            return ActionKind::CalcHistory(i);
        }
    }
    if let Some(val) = s.strip_prefix("calc:") {
        return ActionKind::Calc {
            result: val,
            expr: action.args.as_deref(),
        };
    }
    if let Some(url) = s.strip_prefix("bookmark:add:") {
        return ActionKind::BookmarkAdd(url);
    }
    if let Some(url) = s.strip_prefix("bookmark:remove:") {
        return ActionKind::BookmarkRemove(url);
    }
    if let Some(path) = s.strip_prefix("folder:add:") {
        return ActionKind::FolderAdd(path);
    }
    if let Some(path) = s.strip_prefix("folder:remove:") {
        return ActionKind::FolderRemove(path);
    }
    if s == "history:clear" {
        return ActionKind::HistoryClear;
    }
    if let Some(idx) = s.strip_prefix("history:") {
        if let Ok(i) = idx.parse::<usize>() {
            return ActionKind::HistoryIndex(i);
        }
    }
    if let Some(cmd) = s.strip_prefix("system:") {
        return ActionKind::System(cmd);
    }
    if let Some(pid) = s.strip_prefix("process:kill:") {
        if let Ok(p) = pid.parse::<u32>() {
            return ActionKind::ProcessKill(p);
        }
    }
    if let Some(pid) = s.strip_prefix("process:switch:") {
        if let Ok(p) = pid.parse::<u32>() {
            return ActionKind::ProcessSwitch(p);
        }
    }
    if let Some(hwnd) = s.strip_prefix("window:switch:") {
        if let Ok(h) = hwnd.parse::<isize>() {
            return ActionKind::WindowSwitch(h);
        }
    }
    if let Some(hwnd) = s.strip_prefix("window:close:") {
        if let Ok(h) = hwnd.parse::<isize>() {
            return ActionKind::WindowClose(h);
        }
    }
    if let Some(ids) = s.strip_prefix("tab:switch:") {
        let parts: Vec<i32> = ids
            .split('_')
            .filter_map(|p| p.parse::<i32>().ok())
            .collect();
        if !parts.is_empty() {
            return ActionKind::BrowserTabSwitch(parts);
        }
    }
    if s == "tab:cache" {
        return ActionKind::BrowserTabCache;
    }
    if s == "tab:clear" {
        return ActionKind::BrowserTabClear;
    }
    if let Some(id) = s.strip_prefix("timer:cancel:") {
        if let Ok(i) = id.parse::<u64>() {
            return ActionKind::TimerCancel(i);
        }
    }
    if let Some(id) = s.strip_prefix("timer:pause:") {
        if let Ok(i) = id.parse::<u64>() {
            return ActionKind::TimerPause(i);
        }
    }
    if let Some(id) = s.strip_prefix("timer:resume:") {
        if let Ok(i) = id.parse::<u64>() {
            return ActionKind::TimerResume(i);
        }
    }
    if let Some(arg) = s.strip_prefix("timer:start:") {
        let (dur, name) = arg.split_once('|').unwrap_or((arg, ""));
        return ActionKind::TimerStart { dur, name };
    }
    if let Some(arg) = s.strip_prefix("alarm:set:") {
        let (time, name) = arg.split_once('|').unwrap_or((arg, ""));
        return ActionKind::AlarmSet { time, name };
    }
    if let Some(id) = s.strip_prefix("stopwatch:pause:") {
        if let Ok(i) = id.parse::<u64>() {
            return ActionKind::StopwatchPause(i);
        }
    }
    if let Some(id) = s.strip_prefix("stopwatch:resume:") {
        if let Ok(i) = id.parse::<u64>() {
            return ActionKind::StopwatchResume(i);
        }
    }
    if let Some(id) = s.strip_prefix("stopwatch:stop:") {
        if let Ok(i) = id.parse::<u64>() {
            return ActionKind::StopwatchStop(i);
        }
    }
    if let Some(name) = s.strip_prefix("stopwatch:start:") {
        return ActionKind::StopwatchStart { name };
    }
    if let Some(id) = s.strip_prefix("stopwatch:show:") {
        if let Ok(i) = id.parse::<u64>() {
            return ActionKind::StopwatchShow(i);
        }
    }
    if let Some(rest) = s.strip_prefix("todo:add:") {
        let mut parts = rest.splitn(3, '|');
        let text = parts.next().unwrap_or("");
        let priority = parts.next().and_then(|p| p.parse::<u8>().ok()).unwrap_or(0);
        let tags: Vec<String> = parts
            .next()
            .map(|t| {
                if t.is_empty() {
                    Vec::new()
                } else {
                    t.split(',').map(|s| s.to_string()).collect()
                }
            })
            .unwrap_or_default();
        return ActionKind::TodoAdd {
            text,
            priority,
            tags,
        };
    }
    if let Some(rest) = s.strip_prefix("todo:pset:") {
        if let Some((idx, p)) = rest.split_once('|') {
            if let (Ok(i), Ok(pr)) = (idx.parse::<usize>(), p.parse::<u8>()) {
                return ActionKind::TodoSetPriority {
                    idx: i,
                    priority: pr,
                };
            }
        }
    }
    if let Some(rest) = s.strip_prefix("todo:tag:") {
        if let Some((idx, tags_str)) = rest.split_once('|') {
            if let Ok(i) = idx.parse::<usize>() {
                let tags: Vec<String> = if tags_str.is_empty() {
                    Vec::new()
                } else {
                    tags_str.split(',').map(|s| s.to_string()).collect()
                };
                return ActionKind::TodoSetTags { idx: i, tags };
            }
        }
    }
    if let Some(idx) = s.strip_prefix("todo:remove:") {
        if let Ok(i) = idx.parse::<usize>() {
            return ActionKind::TodoRemove(i);
        }
    }
    if let Some(idx) = s.strip_prefix("todo:done:") {
        if let Ok(i) = idx.parse::<usize>() {
            return ActionKind::TodoDone(i);
        }
    }
    if s == "todo:clear" {
        return ActionKind::TodoClear;
    }
    if s == "todo:export" {
        return ActionKind::TodoExport;
    }
    if let Some(alias) = s.strip_prefix("snippet:remove:") {
        return ActionKind::SnippetRemove(alias);
    }
    if let Some(alias) = s.strip_prefix("snippet:edit:") {
        return ActionKind::SnippetEdit(alias);
    }
    if let Some(rest) = s.strip_prefix("snippet:add:") {
        if let Some((alias, text)) = rest.split_once('|') {
            return ActionKind::SnippetAdd { alias, text };
        }
    }
    if let Some(rest) = s.strip_prefix("fav:add:") {
        let mut parts = rest.splitn(3, '|');
        let label = parts.next().unwrap_or("");
        let cmd = parts.next().unwrap_or("");
        let args = parts.next();
        return ActionKind::FavAdd {
            label,
            command: cmd,
            args,
        };
    }
    if let Some(label) = s.strip_prefix("fav:remove:") {
        return ActionKind::FavRemove(label);
    }
    if let Some(val) = s.strip_prefix("brightness:set:") {
        if let Ok(v) = val.parse::<u32>() {
            return ActionKind::BrightnessSet(v);
        }
    }
    if let Some(val) = s.strip_prefix("volume:set:") {
        if let Ok(v) = val.parse::<u32>() {
            return ActionKind::VolumeSet(v);
        }
    }
    if let Some(rest) = s.strip_prefix("volume:pid:") {
        if let Some((pid_str, level_str)) = rest.split_once(':') {
            if let (Ok(pid), Ok(level)) = (pid_str.parse::<u32>(), level_str.parse::<u32>()) {
                return ActionKind::VolumeSetProcess { pid, level };
            }
        }
    }
    if let Some(pid) = s.strip_prefix("volume:pid_toggle_mute:") {
        if let Ok(pid) = pid.parse::<u32>() {
            return ActionKind::VolumeToggleMuteProcess { pid };
        }
    }
    if s == "volume:mute_active" {
        return ActionKind::VolumeMuteActive;
    }
    if let Some(mode) = s.strip_prefix("screenshot:") {
        use crate::actions::screenshot::Mode as ScreenshotMode;
        return match mode {
            "window" => ActionKind::Screenshot {
                mode: ScreenshotMode::Window,
                clip: false,
            },
            "region" => ActionKind::Screenshot {
                mode: ScreenshotMode::Region,
                clip: false,
            },
            "desktop" => ActionKind::Screenshot {
                mode: ScreenshotMode::Desktop,
                clip: false,
            },
            "window_clip" => ActionKind::Screenshot {
                mode: ScreenshotMode::Window,
                clip: true,
            },
            "region_clip" => ActionKind::Screenshot {
                mode: ScreenshotMode::Region,
                clip: true,
            },
            "desktop_clip" => ActionKind::Screenshot {
                mode: ScreenshotMode::Desktop,
                clip: true,
            },
            _ => ActionKind::ExecPath {
                path: s,
                args: action.args.as_deref(),
            },
        };
    }
    if s == "media:play" {
        return ActionKind::MediaPlay;
    }
    if s == "media:pause" {
        return ActionKind::MediaPause;
    }
    if s == "media:next" {
        return ActionKind::MediaNext;
    }
    if s == "media:prev" {
        return ActionKind::MediaPrev;
    }
    if s == "recycle:clean" {
        return ActionKind::RecycleClean;
    }
    if s == "note:reload" {
        return ActionKind::NoteReload;
    }
    if s == "watch:refresh" {
        return ActionKind::WatchlistRefresh;
    }
    if s == "watch:init" {
        return ActionKind::WatchlistInit { force: false };
    }
    if s == "watch:init:force" {
        return ActionKind::WatchlistInit { force: true };
    }
    if let Some(payload) = s.strip_prefix("watch:add:") {
        return ActionKind::WatchlistAdd(payload);
    }
    if let Some(id) = s.strip_prefix("watch:remove:") {
        return ActionKind::WatchlistRemove(id);
    }
    if let Some(id) = s.strip_prefix("watch:enable:") {
        return ActionKind::WatchlistSetEnabled { id, enabled: true };
    }
    if let Some(id) = s.strip_prefix("watch:disable:") {
        return ActionKind::WatchlistSetEnabled { id, enabled: false };
    }
    if let Some(value) = s.strip_prefix("watch:set_refresh:") {
        if let Ok(refresh_ms) = value.parse::<u64>() {
            return ActionKind::WatchlistSetRefresh { refresh_ms };
        }
    }
    if let Some(rest) = s.strip_prefix("watch:move:") {
        if let Some((id, direction)) = rest.split_once('|') {
            return ActionKind::WatchlistMove { id, direction };
        }
    }
    if let Some(alias) = s.strip_prefix("tempfile:new:") {
        return ActionKind::TempfileNew(Some(alias));
    }
    if s == "tempfile:new" {
        return ActionKind::TempfileNew(None);
    }
    if let Some(path) = s.strip_prefix("tempfile:open:") {
        return ActionKind::TempfileOpenFile(path);
    }
    if s == "tempfile:open" {
        return ActionKind::TempfileOpen;
    }
    if s == "tempfile:clear" {
        return ActionKind::TempfileClear;
    }
    if let Some(p) = s.strip_prefix("tempfile:remove:") {
        return ActionKind::TempfileRemove(p);
    }
    if let Some(rest) = s.strip_prefix("tempfile:alias:") {
        if let Some((path, alias)) = rest.split_once('|') {
            return ActionKind::TempfileAlias { path, alias };
        }
    }
    if let Some(rest) = s.strip_prefix("layout:save:") {
        let (name, flags) = rest.split_once('|').unwrap_or((rest, ""));
        return ActionKind::LayoutSave {
            name,
            flags: if flags.is_empty() { None } else { Some(flags) },
        };
    }
    if let Some(rest) = s.strip_prefix("layout:load:") {
        let (name, flags) = rest.split_once('|').unwrap_or((rest, ""));
        return ActionKind::LayoutLoad {
            name,
            flags: if flags.is_empty() { None } else { Some(flags) },
        };
    }
    if let Some(rest) = s.strip_prefix("layout:show:") {
        let (name, flags) = rest.split_once('|').unwrap_or((rest, ""));
        return ActionKind::LayoutShow {
            name,
            flags: if flags.is_empty() { None } else { Some(flags) },
        };
    }
    if let Some(rest) = s.strip_prefix("layout:rm:") {
        let (name, flags) = rest.split_once('|').unwrap_or((rest, ""));
        return ActionKind::LayoutRemove {
            name,
            flags: if flags.is_empty() { None } else { Some(flags) },
        };
    }
    if let Some(rest) = s.strip_prefix("layout:list") {
        if rest.is_empty() {
            return ActionKind::LayoutList { flags: None };
        }
        if let Some(flags) = rest.strip_prefix('|') {
            return ActionKind::LayoutList {
                flags: if flags.is_empty() { None } else { Some(flags) },
            };
        }
    }
    if s == "layout:edit" {
        return ActionKind::LayoutEdit;
    }
    if let Some(name) = s.strip_prefix("macro:") {
        return ActionKind::Macro(name);
    }
    ActionKind::ExecPath {
        path: s,
        args: action.args.as_deref(),
    }
}

/// Launch an [`Action`], interpreting a variety of custom prefixes.
///
/// Depending on the prefix, this may spawn external processes, modify
/// bookmarks or folders, copy text to the clipboard or evaluate calculator
/// expressions. Shell commands are only executed on Windows.
///
/// Returns an error if spawning an external process or interacting with the
/// clipboard fails.
pub fn launch_action(action: &Action) -> anyhow::Result<()> {
    use crate::actions::*;
    match parse_action_kind(action) {
        ActionKind::Shell { cmd, keep_open } => shell::run(cmd, keep_open),
        ActionKind::ShellAdd { name, args } => shell::add(name, args),
        ActionKind::ShellRemove(name) => shell::remove(name),
        ActionKind::ClipboardClear => clipboard::clear_history(),
        ActionKind::ClipboardCopy(i) => clipboard::copy_entry(i),
        ActionKind::ClipboardText(text) => clipboard::set_text(text),
        ActionKind::Calc { result, expr } => {
            if let Some(e) = expr {
                let entry = CalcHistoryEntry {
                    expr: e.to_string(),
                    result: result.to_string(),
                };
                let _ = calc_history::append_entry(CALC_HISTORY_FILE, entry, MAX_ENTRIES);
            }
            clipboard::calc_to_clipboard(result)
        }
        ActionKind::CalcHistory(i) => crate::actions::calc::copy_history_result(i),
        ActionKind::BookmarkAdd(url) => bookmarks::add(url),
        ActionKind::BookmarkRemove(url) => bookmarks::remove(url),
        ActionKind::FolderAdd(path) => folders::add(path),
        ActionKind::FolderRemove(path) => folders::remove(path),
        ActionKind::HistoryClear => history::clear(),
        ActionKind::HistoryIndex(i) => history::launch_index(i),
        ActionKind::System(cmd) => system::run_system(cmd),
        ActionKind::ProcessKill(pid) => {
            system::process_kill(pid);
            Ok(())
        }
        ActionKind::ProcessSwitch(pid) => {
            system::process_switch(pid);
            Ok(())
        }
        ActionKind::WindowSwitch(hwnd) => {
            system::window_switch(hwnd);
            Ok(())
        }
        ActionKind::WindowClose(hwnd) => {
            system::window_close(hwnd);
            Ok(())
        }
        ActionKind::BrowserTabSwitch(ids) => {
            system::browser_tab_switch(&ids);
            Ok(())
        }
        ActionKind::BrowserTabCache => {
            crate::plugins::browser_tabs::rebuild_cache();
            Ok(())
        }
        ActionKind::BrowserTabClear => {
            crate::plugins::browser_tabs::clear_cache();
            Ok(())
        }
        ActionKind::TimerCancel(id) => {
            timer::cancel(id);
            Ok(())
        }
        ActionKind::TimerPause(id) => {
            timer::pause(id);
            Ok(())
        }
        ActionKind::TimerResume(id) => {
            timer::resume(id);
            Ok(())
        }
        ActionKind::TimerStart { dur, name } => {
            timer::start(dur, name);
            Ok(())
        }
        ActionKind::AlarmSet { time, name } => {
            timer::set_alarm(time, name);
            Ok(())
        }
        ActionKind::StopwatchPause(id) => {
            stopwatch::pause(id);
            Ok(())
        }
        ActionKind::StopwatchResume(id) => {
            stopwatch::resume(id);
            Ok(())
        }
        ActionKind::StopwatchStop(id) => {
            stopwatch::stop(id);
            Ok(())
        }
        ActionKind::StopwatchStart { name } => {
            stopwatch::start(name);
            Ok(())
        }
        ActionKind::StopwatchShow(_id) => Ok(()),
        ActionKind::TodoAdd {
            text,
            priority,
            tags,
        } => todo::add(text, priority, &tags),
        ActionKind::TodoSetPriority { idx, priority } => todo::set_priority(idx, priority),
        ActionKind::TodoSetTags { idx, tags } => todo::set_tags(idx, &tags),
        ActionKind::TodoRemove(i) => todo::remove(i),
        ActionKind::TodoDone(i) => todo::mark_done(i),
        ActionKind::TodoClear => todo::clear_done(),
        ActionKind::TodoExport => {
            todo::export()?;
            Ok(())
        }
        ActionKind::SnippetRemove(alias) => snippets::remove(alias),
        ActionKind::SnippetEdit(_alias) => Ok(()),
        ActionKind::SnippetAdd { alias, text } => snippets::add(alias, text),
        ActionKind::FavAdd {
            label,
            command,
            args,
        } => crate::actions::fav::add(label, command, args),
        ActionKind::FavRemove(label) => crate::actions::fav::remove(label),
        ActionKind::BrightnessSet(v) => {
            system::set_brightness(v);
            Ok(())
        }
        ActionKind::VolumeSet(v) => {
            system::set_volume(v);
            Ok(())
        }
        ActionKind::VolumeSetProcess { pid, level } => {
            system::set_process_volume(pid, level);
            Ok(())
        }
        ActionKind::VolumeToggleMuteProcess { pid } => {
            system::toggle_process_mute(pid);
            Ok(())
        }
        ActionKind::VolumeMuteActive => {
            system::mute_active_window();
            Ok(())
        }
        ActionKind::Screenshot { mode, clip } => {
            crate::actions::screenshot::capture(mode, clip)?;
            Ok(())
        }
        ActionKind::MediaPlay => {
            crate::actions::media::play()?;
            Ok(())
        }
        ActionKind::MediaPause => {
            crate::actions::media::pause()?;
            Ok(())
        }
        ActionKind::MediaNext => {
            crate::actions::media::next()?;
            Ok(())
        }
        ActionKind::MediaPrev => {
            crate::actions::media::prev()?;
            Ok(())
        }
        ActionKind::RecycleClean => {
            system::recycle_clean();
            Ok(())
        }
        ActionKind::NoteReload => {
            crate::plugins::note::load_notes()?;
            crate::plugins::note::refresh_cache()?;
            Ok(())
        }
        ActionKind::WatchlistRefresh => {
            let _ = crate::watchlist::refresh_watchlist_cache(
                &crate::watchlist::watchlist_path_string(),
            );
            Ok(())
        }
        ActionKind::WatchlistInit { force } => {
            crate::watchlist::init_watchlist(&crate::watchlist::watchlist_path_string(), force)?;
            Ok(())
        }
        ActionKind::WatchlistAdd(payload) => {
            crate::watchlist::apply_watch_add_payload(
                &crate::watchlist::watchlist_path_string(),
                payload,
            )?;
            Ok(())
        }
        ActionKind::WatchlistRemove(id) => {
            crate::watchlist::remove_watch_item(&crate::watchlist::watchlist_path_string(), id)?;
            Ok(())
        }
        ActionKind::WatchlistSetEnabled { id, enabled } => {
            crate::watchlist::set_watch_item_enabled(
                &crate::watchlist::watchlist_path_string(),
                id,
                enabled,
            )?;
            Ok(())
        }
        ActionKind::WatchlistSetRefresh { refresh_ms } => {
            crate::watchlist::set_watchlist_refresh_ms(
                &crate::watchlist::watchlist_path_string(),
                refresh_ms,
            )?;
            Ok(())
        }
        ActionKind::WatchlistMove { id, direction } => {
            let Some(direction) = crate::watchlist::parse_move_direction(direction) else {
                return Ok(());
            };
            crate::watchlist::move_watch_item(
                &crate::watchlist::watchlist_path_string(),
                id,
                direction,
            )?;
            Ok(())
        }
        ActionKind::TempfileNew(alias) => tempfiles::new(alias),
        ActionKind::TempfileOpen => tempfiles::open_dir(),
        ActionKind::TempfileOpenFile(path) => tempfiles::open_file(path),
        ActionKind::TempfileClear => tempfiles::clear(),
        ActionKind::TempfileRemove(path) => tempfiles::remove(path),
        ActionKind::TempfileAlias { path, alias } => tempfiles::set_alias(path, alias),
        ActionKind::LayoutSave { name, flags } => layout::save_layout(name, flags),
        ActionKind::LayoutLoad { name, flags } => layout::load_layout(name, flags),
        ActionKind::LayoutShow { name, flags } => layout::show_layout(name, flags),
        ActionKind::LayoutRemove { name, flags } => layout::remove_layout(name, flags),
        ActionKind::LayoutList { flags } => layout::list_layouts(flags),
        ActionKind::LayoutEdit => layout::edit_layouts(),
        ActionKind::Macro(name) => {
            crate::plugins::macros::run_macro(name)?;
            Ok(())
        }
        ActionKind::ExecPath { path, args } => exec::launch(path, args),
    }
}
