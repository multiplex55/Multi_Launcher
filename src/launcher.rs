use crate::actions::Action;

#[cfg(target_os = "windows")]
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

#[cfg(target_os = "windows")]
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

#[cfg(target_os = "windows")]
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

#[cfg(target_os = "windows")]
pub(crate) fn clean_recycle_bin() {
    use windows::Win32::UI::Shell::{
        SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND,
    };
    unsafe {
        let _ = SHEmptyRecycleBinW(
            None,
            None,
            SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
        );
    }
}

pub(crate) fn system_command(action: &str) -> Option<std::process::Command> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        return match action {
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
        };
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = action;
        None
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ActionKind<'a> {
    Shell(&'a str),
    ShellAdd { name: &'a str, args: &'a str },
    ShellRemove(&'a str),
    ClipboardClear,
    ClipboardCopy(usize),
    ClipboardText(&'a str),
    Calc(&'a str),
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
    TimerStart { dur: &'a str, name: &'a str },
    AlarmSet { time: &'a str, name: &'a str },
    NoteAdd(&'a str),
    NoteRemove(usize),
    NoteCopy(usize),
    TodoAdd { text: &'a str, priority: u8, tags: Vec<String> },
    TodoSetPriority { idx: usize, priority: u8 },
    TodoSetTags { idx: usize, tags: Vec<String> },
    TodoRemove(usize),
    TodoDone(usize),
    TodoClear,
    SnippetRemove(&'a str),
    SnippetAdd { alias: &'a str, text: &'a str },
    BrightnessSet(u32),
    VolumeSet(u32),
    VolumeMuteActive,
    Screenshot { mode: crate::actions::screenshot::Mode, clip: bool },
    MediaPlay,
    MediaPause,
    MediaNext,
    MediaPrev,
    RecycleClean,
    WindowSwitch(isize),
    WindowClose(isize),
    TempfileNew(Option<&'a str>),
    TempfileOpen,
    TempfileClear,
    TempfileRemove(&'a str),
    TempfileAlias { path: &'a str, alias: &'a str },
    ExecPath { path: &'a str, args: Option<&'a str> },
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
    if let Some(cmd) = s.strip_prefix("shell:") {
        return ActionKind::Shell(cmd);
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
    if let Some(val) = s.strip_prefix("calc:") {
        return ActionKind::Calc(val);
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
    if let Some(text) = s.strip_prefix("note:add:") {
        return ActionKind::NoteAdd(text);
    }
    if let Some(idx) = s.strip_prefix("note:remove:") {
        if let Ok(i) = idx.parse::<usize>() {
            return ActionKind::NoteRemove(i);
        }
    }
    if let Some(idx) = s.strip_prefix("note:copy:") {
        if let Ok(i) = idx.parse::<usize>() {
            return ActionKind::NoteCopy(i);
        }
    }
    if let Some(rest) = s.strip_prefix("todo:add:") {
        let mut parts = rest.splitn(3, '|');
        let text = parts.next().unwrap_or("");
        let priority = parts
            .next()
            .and_then(|p| p.parse::<u8>().ok())
            .unwrap_or(0);
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
        return ActionKind::TodoAdd { text, priority, tags };
    }
    if let Some(rest) = s.strip_prefix("todo:pset:") {
        if let Some((idx, p)) = rest.split_once('|') {
            if let (Ok(i), Ok(pr)) = (idx.parse::<usize>(), p.parse::<u8>()) {
                return ActionKind::TodoSetPriority { idx: i, priority: pr };
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
    if let Some(alias) = s.strip_prefix("snippet:remove:") {
        return ActionKind::SnippetRemove(alias);
    }
    if let Some(rest) = s.strip_prefix("snippet:add:") {
        if let Some((alias, text)) = rest.split_once('|') {
            return ActionKind::SnippetAdd { alias, text };
        }
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
    if s == "volume:mute_active" {
        return ActionKind::VolumeMuteActive;
    }
    if let Some(mode) = s.strip_prefix("screenshot:") {
        use crate::actions::screenshot::Mode as ScreenshotMode;
        return match mode {
            "window" => ActionKind::Screenshot { mode: ScreenshotMode::Window, clip: false },
            "region" => ActionKind::Screenshot { mode: ScreenshotMode::Region, clip: false },
            "desktop" => ActionKind::Screenshot { mode: ScreenshotMode::Desktop, clip: false },
            "window_clip" => ActionKind::Screenshot { mode: ScreenshotMode::Window, clip: true },
            "region_clip" => ActionKind::Screenshot { mode: ScreenshotMode::Region, clip: true },
            "desktop_clip" => ActionKind::Screenshot { mode: ScreenshotMode::Desktop, clip: true },
            _ => ActionKind::ExecPath { path: s, args: action.args.as_deref() },
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
    if let Some(alias) = s.strip_prefix("tempfile:new:") {
        return ActionKind::TempfileNew(Some(alias));
    }
    if s == "tempfile:new" {
        return ActionKind::TempfileNew(None);
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
        ActionKind::Shell(cmd) => shell::run(cmd),
        ActionKind::ShellAdd { name, args } => shell::add(name, args),
        ActionKind::ShellRemove(name) => shell::remove(name),
        ActionKind::ClipboardClear => clipboard::clear_history(),
        ActionKind::ClipboardCopy(i) => clipboard::copy_entry(i),
        ActionKind::ClipboardText(text) => clipboard::set_text(text),
        ActionKind::Calc(val) => clipboard::calc_to_clipboard(val),
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
        ActionKind::NoteAdd(text) => notes::add(text),
        ActionKind::NoteRemove(i) => notes::remove(i),
        ActionKind::NoteCopy(i) => notes::copy(i),
        ActionKind::TodoAdd { text, priority, tags } => todo::add(text, priority, &tags),
        ActionKind::TodoSetPriority { idx, priority } => todo::set_priority(idx, priority),
        ActionKind::TodoSetTags { idx, tags } => todo::set_tags(idx, &tags),
        ActionKind::TodoRemove(i) => todo::remove(i),
        ActionKind::TodoDone(i) => todo::mark_done(i),
        ActionKind::TodoClear => todo::clear_done(),
        ActionKind::SnippetRemove(alias) => snippets::remove(alias),
        ActionKind::SnippetAdd { alias, text } => snippets::add(alias, text),
        ActionKind::BrightnessSet(v) => {
            system::set_brightness(v);
            Ok(())
        }
        ActionKind::VolumeSet(v) => {
            system::set_volume(v);
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
        ActionKind::TempfileNew(alias) => tempfiles::new(alias),
        ActionKind::TempfileOpen => tempfiles::open_dir(),
        ActionKind::TempfileClear => tempfiles::clear(),
        ActionKind::TempfileRemove(path) => tempfiles::remove(path),
        ActionKind::TempfileAlias { path, alias } => tempfiles::set_alias(path, alias),
        ActionKind::ExecPath { path, args } => exec::launch(path, args),
    }
}
