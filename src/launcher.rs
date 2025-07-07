use crate::actions::Action;
use crate::plugins::bookmarks::{append_bookmark, remove_bookmark};
use crate::plugins::folders::{append_folder, remove_folder, FOLDERS_FILE};
use crate::history;
use arboard::Clipboard;
use std::path::Path;

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
    let path = Path::new(&action.action);

    // If it's an .exe, launch it directly
    if path
        .extension()
        .map(|e| e.eq_ignore_ascii_case("exe"))
        .unwrap_or(false)
    {
        return std::process::Command::new(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.into());
    }

    open::that(&action.action).map_err(|e| e.into())
}
