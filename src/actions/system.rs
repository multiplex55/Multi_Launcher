use sysinfo::System;

pub fn run_system(cmd: &str) -> anyhow::Result<()> {
    if let Some(mut command) = super::super::launcher::system_command(cmd) {
        command.spawn().map(|_| ()).map_err(|e| e.into())
    } else {
        Ok(())
    }
}

pub fn process_kill(pid: u32) {
    let system = System::new_all();
    if let Some(process) = system.process(sysinfo::Pid::from_u32(pid)) {
        let _ = process.kill();
    }
}

pub fn process_switch(_pid: u32) {
    #[cfg(target_os = "windows")]
    {
        super::super::window_manager::activate_process(pid);
    }
}

pub fn window_switch(_hwnd: isize) {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::HWND;
        super::super::window_manager::force_restore_and_foreground(HWND(hwnd as _));
    }
}

pub fn window_close(_hwnd: isize) {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
        unsafe {
            let _ = PostMessageW(HWND(hwnd as _), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn set_brightness(_v: u32) {
    #[cfg(target_os = "windows")]
    super::super::launcher::set_display_brightness(v);
}

pub fn set_volume(_v: u32) {
    #[cfg(target_os = "windows")]
    super::super::launcher::set_system_volume(v);
}

pub fn mute_active_window() {
    #[cfg(target_os = "windows")]
    super::super::launcher::mute_active_window();
}

pub fn recycle_clean() {
    #[cfg(target_os = "windows")]
    super::super::launcher::clean_recycle_bin();
}
