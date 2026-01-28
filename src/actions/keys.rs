use anyhow::Context;

#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, VIRTUAL_KEY,
};

/// Send a key specification string via Win32 `SendInput`.
///
/// Supported forms (comma-separated steps):
/// - Chords: `Ctrl+W`, `Ctrl+Shift+T`
/// - Single keys: `Enter`, `Esc`, `Tab`
/// - Text: `text:hello world` (or `type:hello world`)
///
/// This is intentionally conservative (no key holds / timing yet).
pub fn send(spec: &str) -> anyhow::Result<()> {
    let spec = spec.trim();
    if spec.is_empty() {
        anyhow::bail!("empty key spec");
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = spec;
        anyhow::bail!("keys action is only supported on Windows");
    }

    #[cfg(target_os = "windows")]
    {
        send_windows(spec).context("send keys")?;
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn send_windows(spec: &str) -> anyhow::Result<()> {
    for step in spec.split(',') {
        let step = step.trim();
        if step.is_empty() {
            continue;
        }

        if let Some(text) = strip_prefix_ci(step, "text:").or_else(|| strip_prefix_ci(step, "type:"))
        {
            send_text(text.trim())?;
            continue;
        }

        send_chord(step)?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn send_chord(step: &str) -> anyhow::Result<()> {
    let parts: Vec<&str> = step
        .split('+')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        return Ok(());
    }

    let vks: Vec<VIRTUAL_KEY> = parts
        .iter()
        .map(|p| parse_vk(p))
        .collect::<anyhow::Result<Vec<_>>>()?;

    // key down in order
    for &vk in &vks {
        send_vk(vk, KEYBD_EVENT_FLAGS(0))?;
    }

    // key up in reverse order
    for &vk in vks.iter().rev() {
        send_vk(vk, KEYEVENTF_KEYUP)?;
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn send_vk(vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> anyhow::Result<()> {
    unsafe {
        let input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let sent = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
        if sent == 0 {
            anyhow::bail!("SendInput returned 0");
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn send_text(text: &str) -> anyhow::Result<()> {
    if text.is_empty() {
        return Ok(());
    }

    // Send as unicode scan codes (KEYEVENTF_UNICODE).
    for ch in text.chars() {
        let code = ch as u16;

        unsafe {
            let mut down = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: code,
                        dwFlags: KEYEVENTF_UNICODE,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };

            let sent = SendInput(&[down], std::mem::size_of::<INPUT>() as i32);
            if sent == 0 {
                anyhow::bail!("SendInput returned 0");
            }

            down.Anonymous.ki.dwFlags = KEYBD_EVENT_FLAGS(KEYEVENTF_UNICODE.0 | KEYEVENTF_KEYUP.0);
            let sent = SendInput(&[down], std::mem::size_of::<INPUT>() as i32);
            if sent == 0 {
                anyhow::bail!("SendInput returned 0");
            }
        }
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn parse_vk(token: &str) -> anyhow::Result<VIRTUAL_KEY> {
    let t = token.trim();
    if t.is_empty() {
        anyhow::bail!("empty key token");
    }

    // modifiers
    if eq_ci(t, "ctrl") || eq_ci(t, "control") {
        return Ok(VIRTUAL_KEY(0x11)); // VK_CONTROL
    }
    if eq_ci(t, "shift") {
        return Ok(VIRTUAL_KEY(0x10)); // VK_SHIFT
    }
    if eq_ci(t, "alt") {
        return Ok(VIRTUAL_KEY(0x12)); // VK_MENU
    }
    if eq_ci(t, "win") || eq_ci(t, "windows") || eq_ci(t, "lwin") {
        return Ok(VIRTUAL_KEY(0x5B)); // VK_LWIN
    }

    // common keys
    if eq_ci(t, "enter") || eq_ci(t, "return") {
        return Ok(VIRTUAL_KEY(0x0D)); // VK_RETURN
    }
    if eq_ci(t, "tab") {
        return Ok(VIRTUAL_KEY(0x09)); // VK_TAB
    }
    if eq_ci(t, "esc") || eq_ci(t, "escape") {
        return Ok(VIRTUAL_KEY(0x1B)); // VK_ESCAPE
    }
    if eq_ci(t, "space") {
        return Ok(VIRTUAL_KEY(0x20)); // VK_SPACE
    }
    if eq_ci(t, "backspace") || eq_ci(t, "bksp") {
        return Ok(VIRTUAL_KEY(0x08)); // VK_BACK
    }
    if eq_ci(t, "delete") || eq_ci(t, "del") {
        return Ok(VIRTUAL_KEY(0x2E)); // VK_DELETE
    }
    if eq_ci(t, "insert") || eq_ci(t, "ins") {
        return Ok(VIRTUAL_KEY(0x2D)); // VK_INSERT
    }
    if eq_ci(t, "home") {
        return Ok(VIRTUAL_KEY(0x24)); // VK_HOME
    }
    if eq_ci(t, "end") {
        return Ok(VIRTUAL_KEY(0x23)); // VK_END
    }
    if eq_ci(t, "pageup") || eq_ci(t, "pgup") {
        return Ok(VIRTUAL_KEY(0x21)); // VK_PRIOR
    }
    if eq_ci(t, "pagedown") || eq_ci(t, "pgdn") {
        return Ok(VIRTUAL_KEY(0x22)); // VK_NEXT
    }
    if eq_ci(t, "up") {
        return Ok(VIRTUAL_KEY(0x26)); // VK_UP
    }
    if eq_ci(t, "down") {
        return Ok(VIRTUAL_KEY(0x28)); // VK_DOWN
    }
    if eq_ci(t, "left") {
        return Ok(VIRTUAL_KEY(0x25)); // VK_LEFT
    }
    if eq_ci(t, "right") {
        return Ok(VIRTUAL_KEY(0x27)); // VK_RIGHT
    }

    // function keys: F1..F24
    if (t.len() == 2 || t.len() == 3) && (t.starts_with('F') || t.starts_with('f')) {
        if let Ok(n) = t[1..].parse::<u8>() {
            if (1..=24).contains(&n) {
                return Ok(VIRTUAL_KEY(0x6F + n as u16)); // VK_F1=0x70
            }
        }
    }

    // single ASCII letter/digit
    if t.len() == 1 {
        let ch = t.chars().next().unwrap();
        if ch.is_ascii_alphabetic() {
            return Ok(VIRTUAL_KEY(ch.to_ascii_uppercase() as u16));
        }
        if ch.is_ascii_digit() {
            return Ok(VIRTUAL_KEY(ch as u16));
        }
    }

    anyhow::bail!("unknown key token '{t}'");
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() < prefix.len() {
        return None;
    }
    if s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

fn eq_ci(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}
