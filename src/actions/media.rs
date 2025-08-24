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

pub fn play() -> anyhow::Result<()> {
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_MEDIA_PLAY_PAUSE;
    send_key(VK_MEDIA_PLAY_PAUSE);
    Ok(())
}

pub fn pause() -> anyhow::Result<()> {
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_MEDIA_PLAY_PAUSE;
    send_key(VK_MEDIA_PLAY_PAUSE);
    Ok(())
}

pub fn next() -> anyhow::Result<()> {
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_MEDIA_NEXT_TRACK;
    send_key(VK_MEDIA_NEXT_TRACK);
    Ok(())
}

pub fn prev() -> anyhow::Result<()> {
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_MEDIA_PREV_TRACK;
    send_key(VK_MEDIA_PREV_TRACK);
    Ok(())
}
