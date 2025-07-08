use crate::actions::Action;
use crate::plugins::bookmarks::{append_bookmark, remove_bookmark};
use crate::plugins::folders::{append_folder, remove_folder, FOLDERS_FILE};
use crate::plugins::notes::{append_note, remove_note, load_notes, QUICK_NOTES_FILE};
use crate::plugins::timer;
use crate::history;
use sysinfo::System;
use arboard::Clipboard;
use std::path::Path;
use shlex;

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
    if let Some(text) = action.action.strip_prefix("clipboard:") {
        let mut cb = Clipboard::new()?;
        cb.set_text(text.to_string())?;
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
            let mut system = System::new_all();
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
