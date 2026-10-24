//! Narrow imgdiff clipboard input. `arboard::get_image` eagerly copies and
//! decodes provider data, so this reader accepts only bounded CF_DIB formats
//! whose storage and dimensions can be checked before allocating RGBA.

#[cfg(windows)]
use super::input::{LoadedInput, decode_dib_bytes};
use super::{ImageCompareBudget, ImageCompareError};
use std::sync::atomic::AtomicBool;

#[cfg(windows)]
const MAX_CLIPBOARD_DIB_BYTES: usize = 65 * 1024 * 1024;

#[cfg(windows)]
pub(super) fn load_windows_clipboard(
    budget: &ImageCompareBudget,
    cancelled: &AtomicBool,
) -> Result<LoadedInput, ImageCompareError> {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

    struct ClipboardOpen;
    impl Drop for ClipboardOpen {
        fn drop(&mut self) {
            // SAFETY: construction follows a successful OpenClipboard call.
            unsafe {
                let _ = CloseClipboard();
            }
        }
    }

    struct LockedGlobal(HGLOBAL);
    impl Drop for LockedGlobal {
        fn drop(&mut self) {
            // SAFETY: this handle was locked successfully and remains owned by
            // the open clipboard until the guard closes it.
            unsafe {
                let _ = GlobalUnlock(self.0);
            }
        }
    }

    if cancelled.load(std::sync::atomic::Ordering::Acquire) {
        return Err(ImageCompareError::Cancelled);
    }
    // SAFETY: a null owner is permitted for this worker-owned clipboard read.
    unsafe { OpenClipboard(None) }.map_err(|error| {
        ImageCompareError::Read(format!("clipboard could not be opened: {error}"))
    })?;
    let _clipboard = ClipboardOpen;

    let format = [17_u32, 8_u32]
        .into_iter()
        .find(|format| unsafe { IsClipboardFormatAvailable(*format).is_ok() })
        .ok_or_else(|| {
            ImageCompareError::Unsupported(
                "clipboard has no bounded CF_DIB or CF_DIBV5 image".into(),
            )
        })?;
    // SAFETY: the clipboard is open and the format was advertised as present.
    let handle = unsafe { GetClipboardData(format) }.map_err(|error| {
        ImageCompareError::Read(format!("clipboard image could not be read: {error}"))
    })?;
    if handle.0.is_null() {
        return Err(ImageCompareError::Read(
            "clipboard returned empty DIB storage".into(),
        ));
    }
    let global = HGLOBAL(handle.0);
    // GlobalSize examines the provider-owned allocation without copying it.
    let size = unsafe { GlobalSize(global) };
    if size < 40 || size > MAX_CLIPBOARD_DIB_BYTES {
        return Err(ImageCompareError::Unsupported(format!(
            "clipboard DIB storage size {size} is outside the supported bound"
        )));
    }
    // SAFETY: GlobalLock returns a readable allocation for this open clipboard
    // handle; GlobalSize supplied the exact slice length and the RAII guard
    // unlocks it before the clipboard closes.
    let pointer = unsafe { GlobalLock(global) }.cast::<u8>();
    if pointer.is_null() {
        return Err(ImageCompareError::Read(
            "clipboard DIB storage could not be locked".into(),
        ));
    }
    let _locked = LockedGlobal(global);
    // SAFETY: validated non-null pointer and bounded GlobalSize above.
    let bytes = unsafe { std::slice::from_raw_parts(pointer, size) };
    let (width, height, pixels, reservation) = decode_dib_bytes(bytes, format, budget, cancelled)?;
    let format_name = if format == 17 { "CF_DIBV5" } else { "CF_DIB" };
    super::input::LoadedInput::from_rgba(
        width,
        height,
        pixels,
        super::input::ImageInputSource::Clipboard {
            format: format_name.to_owned(),
        },
        reservation,
    )
}
