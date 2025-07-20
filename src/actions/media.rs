#[cfg(target_os = "windows")]
fn send_key(vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    };
    unsafe {
        let mut input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: KEYBD_EVENT_FLAGS(0),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let _ = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
        input.Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;
        let _ = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
    }
}

#[cfg(target_os = "linux")]
fn playerctl(cmd: &str) {
    let _ = std::process::Command::new("playerctl").arg(cmd).spawn();
}

#[cfg(target_os = "windows")]
pub fn play() -> anyhow::Result<()> {
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_MEDIA_PLAY_PAUSE;
    send_key(VK_MEDIA_PLAY_PAUSE);
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn play() -> anyhow::Result<()> {
    playerctl("play");
    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub fn play() -> anyhow::Result<()> {
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn pause() -> anyhow::Result<()> {
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_MEDIA_PLAY_PAUSE;
    send_key(VK_MEDIA_PLAY_PAUSE);
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn pause() -> anyhow::Result<()> {
    playerctl("pause");
    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub fn pause() -> anyhow::Result<()> {
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn next() -> anyhow::Result<()> {
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_MEDIA_NEXT_TRACK;
    send_key(VK_MEDIA_NEXT_TRACK);
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn next() -> anyhow::Result<()> {
    playerctl("next");
    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub fn next() -> anyhow::Result<()> {
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn prev() -> anyhow::Result<()> {
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_MEDIA_PREV_TRACK;
    send_key(VK_MEDIA_PREV_TRACK);
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn prev() -> anyhow::Result<()> {
    playerctl("previous");
    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub fn prev() -> anyhow::Result<()> {
    Ok(())
}
