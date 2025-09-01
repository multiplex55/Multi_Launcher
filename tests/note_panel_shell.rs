#![cfg(windows)]
use multi_launcher::gui::{build_nvim_command, build_wezterm_command};
use once_cell::sync::Lazy;
use std::env;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[test]
fn prefers_powershell7_then_powershell_then_cmd() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let note = Path::new("note.txt");
    let orig_ps7 = env::var_os("ML_PWSH7_PATH");
    let orig_path = env::var_os("PATH");

    // PowerShell 7 available
    let dir = tempdir().unwrap();
    let ps7 = dir.path().join("pwsh.exe");
    fs::write(&ps7, "").unwrap();
    unsafe {
        env::set_var("ML_PWSH7_PATH", &ps7);
        env::set_var("PATH", "");
    }
    let (cmd, _) = build_nvim_command(note);
    assert!(cmd.get_program().to_string_lossy().ends_with("pwsh.exe"));
    let args: Vec<_> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(args, ["-NoLogo", "-NoExit", "-Command", "nvim note.txt"]);

    // PowerShell in PATH
    let dir = tempdir().unwrap();
    let ps = dir.path().join("powershell.exe");
    fs::write(&ps, "").unwrap();
    let missing = dir.path().join("missing_pwsh.exe");
    unsafe {
        env::set_var("ML_PWSH7_PATH", &missing);
        env::set_var("PATH", dir.path());
    }
    let (cmd, _) = build_nvim_command(note);
    assert!(cmd
        .get_program()
        .to_string_lossy()
        .ends_with("powershell.exe"));

    // Fallback to cmd.exe
    unsafe {
        env::set_var("PATH", "");
    }
    let (cmd, _) = build_nvim_command(note);
    assert!(cmd.get_program().to_string_lossy().ends_with("cmd.exe"));
    let args: Vec<_> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(args, ["/C", "nvim", "note.txt"]);

    unsafe {
        match orig_ps7 {
            Some(v) => env::set_var("ML_PWSH7_PATH", v),
            None => env::remove_var("ML_PWSH7_PATH"),
        }
        match orig_path {
            Some(v) => env::set_var("PATH", v),
            None => env::remove_var("PATH"),
        }
    }
}

#[test]
fn wezterm_fallbacks_to_powershell_when_missing() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let note = Path::new("note.txt");
    let orig_ps7 = env::var_os("ML_PWSH7_PATH");
    let orig_path = env::var_os("PATH");

    let dir = tempdir().unwrap();
    let pwsh = dir.path().join("pwsh.exe");
    fs::write(&pwsh, "").unwrap();
    unsafe {
        env::set_var("ML_PWSH7_PATH", &pwsh);
        env::set_var("PATH", "");
    }

    let (mut cmd, _) = build_wezterm_command(note);
    let res = cmd.spawn();
    assert!(res.is_err());

    let (cmd, _) = build_nvim_command(note);
    assert!(cmd.get_program().to_string_lossy().ends_with("pwsh.exe"));

    unsafe {
        match orig_ps7 {
            Some(v) => env::set_var("ML_PWSH7_PATH", v),
            None => env::remove_var("ML_PWSH7_PATH"),
        }
        match orig_path {
            Some(v) => env::set_var("PATH", v),
            None => env::remove_var("PATH"),
        }
    }
}
