#[cfg(windows)]
mod shortcut;
#[cfg(windows)]
pub use shortcut::{launch_shortcut, shortcut_path_is_admissible};

use std::{ffi::OsStr, path::Path};

pub fn launch(path: &str, args: Option<&str>) -> anyhow::Result<()> {
    launch_with_identity(path, args).map(|_| ())
}

/// Open the exact target through the operating system's registered association.
#[cfg(windows)]
pub fn launch_association(target: impl AsRef<OsStr>) -> anyhow::Result<()> {
    use anyhow::Context as _;
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        Win32::{
            Foundation::HWND,
            System::Com::{
                COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
            },
            UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
        },
        core::{PCWSTR, w},
    };

    let mut wide: Vec<u16> = target.as_ref().encode_wide().collect();
    anyhow::ensure!(
        !wide.contains(&0),
        "Association target contains a NUL character"
    );
    wide.push(0);
    // Some association handlers use apartment-threaded Shell extensions. The
    // synchronous call keeps the literal UTF-16 target alive and COM balanced.
    let result = unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE)
            .ok()
            .context("Could not initialize COM for the Windows association")?;
        let result = ShellExecuteW(
            HWND::default(),
            w!("open"),
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
        CoUninitialize();
        result
    };
    let code = result.0 as isize;
    anyhow::ensure!(code > 32, "Shell association failed with code {code}");
    Ok(())
}

#[cfg(not(windows))]
pub fn launch_association(target: impl AsRef<OsStr>) -> anyhow::Result<()> {
    open::that(target).map_err(Into::into)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchIdentity {
    pub pid: Option<u32>,
    pub target: String,
}

/// Launch an external target while retaining process identity when the platform launcher exposes it.
pub fn launch_with_identity(path: &str, args: Option<&str>) -> anyhow::Result<LaunchIdentity> {
    let path = Path::new(path);
    let is_exe = path
        .extension()
        .map(|e| e.eq_ignore_ascii_case("exe"))
        .unwrap_or(false);

    let has_args = args.map(|a| !a.trim().is_empty()).unwrap_or(false);

    if is_exe || has_args {
        let mut command = std::process::Command::new(path);
        if let Some(arg_str) = args {
            let arg_str = arg_str.trim();
            if !arg_str.is_empty() {
                if let Some(list) = shlex::split(arg_str) {
                    command.args(list);
                } else {
                    command.args(arg_str.split_whitespace());
                }
            }
        }
        command
            .spawn()
            .map(|child| LaunchIdentity {
                pid: Some(child.id()),
                target: path.to_string_lossy().into_owned(),
            })
            .map_err(|e| e.into())
    } else {
        open::that(path)
            .map(|_| LaunchIdentity {
                pid: None,
                target: path.to_string_lossy().into_owned(),
            })
            .map_err(Into::into)
    }
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn association_rejects_embedded_nul_before_shell_handoff() {
        assert!(super::launch_association(std::ffi::OsStr::new("target\0other")).is_err());
    }
}
