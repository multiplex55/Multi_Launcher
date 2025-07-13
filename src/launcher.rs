use crate::actions::Action;
use crate::plugins::bookmarks::{append_bookmark, remove_bookmark};
use crate::plugins::folders::{append_folder, remove_folder, FOLDERS_FILE};
use crate::plugins::notes::{append_note, remove_note, load_notes, QUICK_NOTES_FILE};
use crate::plugins::snippets::{remove_snippet, SNIPPETS_FILE};
use crate::plugins::timer;
use crate::history;
use sysinfo::System;
use arboard::Clipboard;
use std::path::Path;
use shlex;

#[cfg(target_os = "windows")]
fn set_system_volume(percent: u32) {
    use windows::Win32::Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator, eRender, eMultimedia};
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED};

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if let Ok(enm) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) {
            if let Ok(device) = enm.GetDefaultAudioEndpoint(eRender, eMultimedia) {
                if let Ok(vol) = device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) {
                    let _ = vol.SetMasterVolumeLevelScalar(percent as f32 / 100.0, std::ptr::null());
                }
            }
        }
        CoUninitialize();
    }
}

#[cfg(not(target_os = "windows"))]
fn set_system_volume(_percent: u32) {}

#[cfg(target_os = "windows")]
fn mute_active_window() {
    use windows::core::Interface;
    use windows::Win32::Media::Audio::{IAudioSessionManager2, IAudioSessionControl2, ISimpleAudioVolume, IMMDeviceEnumerator, MMDeviceEnumerator, eRender, eMultimedia};
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if let Ok(enm) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) {
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

#[cfg(not(target_os = "windows"))]
fn mute_active_window() {}

#[cfg(target_os = "windows")]
fn set_display_brightness(percent: u32) {
    use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};
    use windows::Win32::Devices::Display::{
        DestroyPhysicalMonitors, GetNumberOfPhysicalMonitorsFromHMONITOR,
        GetPhysicalMonitorsFromHMONITOR, PHYSICAL_MONITOR, SetMonitorBrightness,
    };

    unsafe extern "system" fn enum_monitors(hmonitor: HMONITOR, _hdc: HDC, _rect: *mut RECT, lparam: LPARAM) -> BOOL {
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

#[cfg(not(target_os = "windows"))]
fn set_display_brightness(_percent: u32) {}

#[cfg(target_os = "windows")]
fn clean_recycle_bin() {
    use windows::Win32::UI::Shell::{SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND};
    unsafe {
        let _ = SHEmptyRecycleBinW(None, None, SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND);
    }
}

#[cfg(not(target_os = "windows"))]
fn clean_recycle_bin() {}

fn system_command(action: &str) -> Option<std::process::Command> {
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

/// Launch an [`Action`], interpreting a variety of custom prefixes.
///
/// Depending on the prefix, this may spawn external processes, modify
/// bookmarks or folders, copy text to the clipboard or evaluate calculator
/// expressions. Shell commands are only executed on Windows.
///
/// Returns an error if spawning an external process or interacting with the
/// clipboard fails.
pub fn launch_action(action: &Action) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    if let Some(cmd) = action.action.strip_prefix("shell:") {
        let mut command = {
            let mut c = std::process::Command::new("cmd");
            c.arg("/C").arg(cmd);
            c
        };
        return command.spawn().map(|_| ()).map_err(|e| e.into());
    }
    #[cfg(not(target_os = "windows"))]
    if let Some(_cmd) = action.action.strip_prefix("shell:") {
        // Shell commands are only supported on Windows
        return Ok(());
    }
    if let Some(rest) = action.action.strip_prefix("clipboard:") {
        if rest == "clear" {
            crate::plugins::clipboard::clear_history_file(crate::plugins::clipboard::CLIPBOARD_FILE)?;
            return Ok(());
        }
        if let Some(idx_str) = rest.strip_prefix("copy:") {
            if let Ok(i) = idx_str.parse::<usize>() {
                if let Some(entry) = crate::plugins::clipboard::load_history(crate::plugins::clipboard::CLIPBOARD_FILE)
                    .unwrap_or_default()
                    .get(i)
                    .cloned()
                {
                    let mut cb = Clipboard::new()?;
                    cb.set_text(entry)?;
                }
            }
            return Ok(());
        }
        let mut cb = Clipboard::new()?;
        cb.set_text(rest.to_string())?;
        return Ok(());
    }
    if let Some(value) = action.action.strip_prefix("calc:") {
        let mut cb = Clipboard::new()?;
        cb.set_text(value.to_string())?;
        return Ok(());
    }
    if let Some(url) = action.action.strip_prefix("bookmark:add:") {
        append_bookmark("bookmarks.json", url)?;
        return Ok(());
    }
    if let Some(url) = action.action.strip_prefix("bookmark:remove:") {
        remove_bookmark("bookmarks.json", url)?;
        return Ok(());
    }
    if let Some(path) = action.action.strip_prefix("folder:add:") {
        append_folder(FOLDERS_FILE, path)?;
        return Ok(());
    }
    if let Some(path) = action.action.strip_prefix("folder:remove:") {
        remove_folder(FOLDERS_FILE, path)?;
        return Ok(());
    }
    if action.action == "history:clear" {
        history::clear_history()?;
        return Ok(());
    }
    if let Some(idx) = action.action.strip_prefix("history:") {
        if let Ok(i) = idx.parse::<usize>() {
            if let Some(entry) = history::get_history().get(i).cloned() {
                return launch_action(&entry.action);
            }
        }
    }
    if let Some(cmd) = action.action.strip_prefix("system:") {
        if let Some(mut command) = system_command(cmd) {
            return command.spawn().map(|_| ()).map_err(|e| e.into());
        }
        return Ok(());
    }
    if let Some(pid) = action.action.strip_prefix("process:kill:") {
        if let Ok(pid) = pid.parse::<u32>() {
            let system = System::new_all();
            if let Some(process) = system.process(sysinfo::Pid::from_u32(pid)) {
                let _ = process.kill();
            }
        }
        return Ok(());
    }
    if let Some(pid) = action.action.strip_prefix("process:switch:") {
        if let Ok(pid) = pid.parse::<u32>() {
            #[cfg(target_os = "windows")]
            {
                crate::window_manager::activate_process(pid);
            }
        }
        return Ok(());
    }
    if let Some(id) = action.action.strip_prefix("timer:cancel:") {
        if let Ok(id) = id.parse::<u64>() {
            timer::cancel_timer(id);
        }
        return Ok(());
    }
    if let Some(arg) = action.action.strip_prefix("timer:start:") {
        let (dur_str, name) = arg.split_once('|').unwrap_or((arg, ""));
        if let Some(dur) = timer::parse_duration(dur_str) {
            if name.is_empty() {
                timer::start_timer(dur);
            } else {
                timer::start_timer_named(dur, Some(name.to_string()));
            }
        }
        return Ok(());
    }
    if let Some(arg) = action.action.strip_prefix("alarm:set:") {
        let (time_str, name) = arg.split_once('|').unwrap_or((arg, ""));
        if let Some((h, m)) = timer::parse_hhmm(time_str) {
            if name.is_empty() {
                timer::start_alarm(h, m);
            } else {
                timer::start_alarm_named(h, m, Some(name.to_string()));
            }
        }
        return Ok(());
    }
    if let Some(text) = action.action.strip_prefix("note:add:") {
        append_note(QUICK_NOTES_FILE, text)?;
        return Ok(());
    }
    if let Some(idx) = action.action.strip_prefix("note:remove:") {
        if let Ok(i) = idx.parse::<usize>() {
            remove_note(QUICK_NOTES_FILE, i)?;
        }
        return Ok(());
    }
    if let Some(idx) = action.action.strip_prefix("note:copy:") {
        if let Ok(i) = idx.parse::<usize>() {
            if let Some(entry) = load_notes(QUICK_NOTES_FILE)?.get(i).cloned() {
                let mut cb = Clipboard::new()?;
                cb.set_text(entry.text)?;
            }
        }
        return Ok(());
    }
    if let Some(text) = action.action.strip_prefix("todo:add:") {
        crate::plugins::todo::append_todo(crate::plugins::todo::TODO_FILE, text)?;
        return Ok(());
    }
    if let Some(idx) = action.action.strip_prefix("todo:remove:") {
        if let Ok(i) = idx.parse::<usize>() {
            crate::plugins::todo::remove_todo(crate::plugins::todo::TODO_FILE, i)?;
        }
        return Ok(());
    }
    if let Some(idx) = action.action.strip_prefix("todo:done:") {
        if let Ok(i) = idx.parse::<usize>() {
            crate::plugins::todo::mark_done(crate::plugins::todo::TODO_FILE, i)?;
        }
        return Ok(());
    }
    if action.action == "todo:clear" {
        crate::plugins::todo::clear_done(crate::plugins::todo::TODO_FILE)?;
        return Ok(());
    }
    if let Some(alias) = action.action.strip_prefix("snippet:remove:") {
        remove_snippet(SNIPPETS_FILE, alias)?;
        return Ok(());
    }
    if let Some(val) = action.action.strip_prefix("brightness:set:") {
        if let Ok(v) = val.parse::<u32>() {
            #[cfg(target_os = "windows")]
            set_display_brightness(v);
        }
        return Ok(());
    }
    if let Some(val) = action.action.strip_prefix("volume:set:") {
        if let Ok(v) = val.parse::<u32>() {
            #[cfg(target_os = "windows")]
            set_system_volume(v);
        }
        return Ok(());
    }
    if action.action == "volume:mute_active" {
        #[cfg(target_os = "windows")]
        mute_active_window();
        return Ok(());
    }
    if action.action == "recycle:clean" {
        #[cfg(target_os = "windows")]
        clean_recycle_bin();
        return Ok(());
    }
    if action.action == "tempfile:new" {
        let path = crate::plugins::tempfile::create_file()?;
        open::that(&path)?;
        return Ok(());
    }
    if action.action == "tempfile:open" {
        let dir = crate::plugins::tempfile::storage_dir();
        std::fs::create_dir_all(&dir)?;
        open::that(dir)?;
        return Ok(());
    }
    if action.action == "tempfile:clear" {
        crate::plugins::tempfile::clear_files()?;
        return Ok(());
    }
    let path = Path::new(&action.action);

    // If it's an .exe or we have additional args, launch it directly
    let is_exe = path
        .extension()
        .map(|e| e.eq_ignore_ascii_case("exe"))
        .unwrap_or(false);

    if is_exe || action.args.is_some() {
        let mut command = std::process::Command::new(path);
        if let Some(arg_str) = &action.args {
            if let Some(list) = shlex::split(arg_str) {
                command.args(list);
            } else {
                command.args(arg_str.split_whitespace());
            }
        }
        return command.spawn().map(|_| ()).map_err(|e| e.into());
    }

    open::that(&action.action).map_err(|e| e.into())
}
