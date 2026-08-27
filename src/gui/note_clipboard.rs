//! Clipboard image acquisition and Windows-only format diagnostics.
//!
//! `arboard` remains the production decoder. Native inspection is diagnostic-only: no
//! fallback format has yet been justified by Windows smoke-test evidence.

use super::note_panel::ClipboardRgbaData;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClipboardFormatRecord {
    id: u32,
    registered_name: Option<Result<String, String>>,
}

fn predefined_format_name(id: u32) -> Option<&'static str> {
    match id {
        1 => Some("CF_TEXT"),
        2 => Some("CF_BITMAP"),
        8 => Some("CF_DIB"),
        13 => Some("CF_UNICODETEXT"),
        17 => Some("CF_DIBV5"),
        _ => None,
    }
}

/// Pure presentation boundary, independent of Win32 handles and UTF-16 buffers.
fn format_clipboard_format(record: &ClipboardFormatRecord) -> String {
    let name = predefined_format_name(record.id)
        .map(str::to_owned)
        .or_else(|| match &record.registered_name {
            Some(Ok(name)) => Some(name.clone()),
            Some(Err(error)) => Some(format!("<name unavailable: {error}>")),
            None => None,
        })
        .unwrap_or_else(|| "<unknown>".to_owned());
    format!("{} ({name})", record.id)
}

pub(super) fn read_clipboard_image() -> Result<Option<ClipboardRgbaData>, String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|error| {
        tracing::warn!(stage = "clipboard_construct", error = %error, "note image paste failed");
        format!("clipboard could not be opened: {error}")
    })?;
    match clipboard.get_image() {
        Ok(image) => Ok(Some(ClipboardRgbaData {
            width: image.width,
            height: image.height,
            bytes: image.bytes.into_owned(),
        })),
        Err(arboard::Error::ContentNotAvailable) => {
            diagnose_formats_after_no_image();
            Ok(None)
        }
        Err(error) => {
            tracing::warn!(stage = "clipboard_image_decode", error = %error, "note image paste failed");
            Err(format!("clipboard image could not be decoded: {error}"))
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn diagnose_formats_after_no_image() {}

#[cfg(target_os = "windows")]
fn diagnose_formats_after_no_image() {
    match windows_clipboard_formats() {
        Err(error) => {
            tracing::debug!(error = %error, "clipboard image format diagnostics unavailable")
        }
        Ok(formats) if formats.is_empty() => {
            tracing::debug!("clipboard formats were enumerated, but none were present")
        }
        Ok(formats) => {
            for format in &formats {
                tracing::debug!(format = %format_clipboard_format(format), "clipboard format available");
            }
            tracing::debug!(
                format_count = formats.len(),
                "clipboard formats were present but arboard decoded no image"
            );
        }
    }
}

#[cfg(target_os = "windows")]
fn windows_clipboard_formats() -> Result<Vec<ClipboardFormatRecord>, String> {
    use windows::Win32::Foundation::{ERROR_SUCCESS, GetLastError, SetLastError};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EnumClipboardFormats, GetClipboardFormatNameW, OpenClipboard,
    };

    struct ClipboardGuard;
    impl ClipboardGuard {
        fn open() -> Result<Self, String> {
            // SAFETY: A null owner is supported and Drop pairs every successful open with close.
            unsafe {
                OpenClipboard(None).map_err(|e| format!("clipboard could not be opened: {e}"))?
            };
            Ok(Self)
        }
    }
    impl Drop for ClipboardGuard {
        fn drop(&mut self) {
            // SAFETY: the guard is constructed only after OpenClipboard succeeds.
            unsafe {
                let _ = CloseClipboard();
            }
        }
    }

    let _guard = ClipboardGuard::open()?;
    let mut records = Vec::new();
    let mut current = 0;
    loop {
        // Zero means either exhaustion or failure, so clear last-error before enumeration.
        unsafe { SetLastError(ERROR_SUCCESS) };
        // SAFETY: clipboard is open and current is zero or an ID returned by this API.
        let next = unsafe { EnumClipboardFormats(current) };
        if next == 0 {
            let error = unsafe { GetLastError() };
            if error != ERROR_SUCCESS {
                return Err(format!(
                    "clipboard formats could not be enumerated: {error:?}"
                ));
            }
            break;
        }
        current = next;
        let registered_name = if next >= 0xC000 {
            let mut buffer = [0_u16; 256];
            // SAFETY: buffer is valid and writable for its declared length.
            let length = unsafe { GetClipboardFormatNameW(next, &mut buffer) };
            if length > 0 {
                Some(String::from_utf16(&buffer[..length as usize]).map_err(|e| e.to_string()))
            } else {
                Some(Err(format!(
                    "GetClipboardFormatNameW failed: {:?}",
                    unsafe { GetLastError() }
                )))
            }
        } else {
            None
        };
        records.push(ClipboardFormatRecord {
            id: next,
            registered_name,
        });
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_predefined_registered_unknown_and_failed_names() {
        let formatted = |id, registered_name| {
            format_clipboard_format(&ClipboardFormatRecord {
                id,
                registered_name,
            })
        };
        assert_eq!(formatted(8, None), "8 (CF_DIB)");
        assert_eq!(formatted(17, None), "17 (CF_DIBV5)");
        assert_eq!(formatted(49_152, Some(Ok("PNG".into()))), "49152 (PNG)");
        assert_eq!(formatted(42, None), "42 (<unknown>)");
        assert_eq!(
            formatted(49_153, Some(Err("invalid UTF-16".into()))),
            "49153 (<name unavailable: invalid UTF-16>)"
        );
    }
}
