use crate::actions::Action;
use std::path::Path;

pub fn launch_action(action: &Action) -> anyhow::Result<()> {
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
