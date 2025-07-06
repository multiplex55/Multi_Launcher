use crate::actions::Action;
use crate::plugins::bookmarks::{append_bookmark, remove_bookmark};
use crate::history;
use arboard::Clipboard;
use std::path::Path;

pub fn launch_action(action: &Action) -> anyhow::Result<()> {
    if let Some(cmd) = action.action.strip_prefix("shell:") {
        #[cfg(target_os = "windows")]
        let mut command = {
            let mut c = std::process::Command::new("cmd");
            c.arg("/C").arg(cmd);
            c
        };
        #[cfg(not(target_os = "windows"))]
        let mut command = {
            let mut c = std::process::Command::new("sh");
            c.arg("-c").arg(cmd);
            c
        };
        return command.spawn().map(|_| ()).map_err(|e| e.into());
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
    if let Some(idx) = action.action.strip_prefix("history:") {
        if let Ok(i) = idx.parse::<usize>() {
            if let Some(entry) = history::get_history().get(i).cloned() {
                return launch_action(&entry.action);
            }
        }
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

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut should_spawn = false;

        #[cfg(target_os = "macos")]
        {
            if path.extension().map(|e| e == "app").unwrap_or(false) {
                should_spawn = true;
            }
        }

        if !should_spawn {
            if let Ok(meta) = path.metadata() {
                if meta.permissions().mode() & 0o111 != 0 {
                    should_spawn = true;
                }
            }
        }

        if should_spawn {
            return std::process::Command::new(path)
                .spawn()
                .map(|_| ())
                .map_err(|e| e.into());
        }
    }

    open::that(&action.action).map_err(|e| e.into())
}
