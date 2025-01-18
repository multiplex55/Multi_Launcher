use std::ptr;
use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::*;

/// Determines whether the specified `hwnd` is currently located at the given **(x, y)** coordinates
/// with the specified **width** and **height**.
///
/// # Behavior
/// - Retrieves the window’s current position and size using
///   [`get_window_position`](#fn.get_window_position).
/// - Compares the returned `(x, y, width, height)` tuple to the provided parameters.
/// - Returns `true` if they match exactly, otherwise `false`.
///
/// # Side Effects
/// - Calls `get_window_position`, which uses the Win32 API [`GetWindowRect`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowrect)
///   to retrieve the actual window rectangle on screen.
///
/// # Example
/// ```rust
/// if is_window_at_position(hwnd, 100, 100, 800, 600) {
///     println!("The window is exactly at (100, 100) with size (800x600).");
/// } else {
///     println!("The window is not at the specified position/size.");
/// }
/// ```
///
/// # Notes
/// - If `get_window_position` fails or returns an error, this function returns `false`.
/// - Primarily used internally (e.g., in `are_all_windows_at_home`).
pub fn show_message_box(message: &str, title: &str) {
    unsafe {
        MessageBoxW(
            HWND(ptr::null_mut()), // Null pointer for no parent window
            PCWSTR(
                message
                    .encode_utf16()
                    .chain(Some(0))
                    .collect::<Vec<u16>>()
                    .as_ptr(),
            ),
            PCWSTR(
                title
                    .encode_utf16()
                    .chain(Some(0))
                    .collect::<Vec<u16>>()
                    .as_ptr(),
            ),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

/// Displays a **modal confirmation dialog** with “Yes” and “No” buttons, returning `true` if the user clicks “Yes,”
/// or `false` if they click “No” (or close the dialog).
///
/// # Behavior
/// - Uses the Win32 API [`MessageBoxW`](https://learn.microsoft.com/en-us/windows/winuser/nf-winuser-messageboxw)
///   with the flags `MB_YESNO | MB_ICONQUESTION`.
/// - Presents a question-mark icon and waits for user interaction.
/// - Returns a boolean:
///   - `true` if the user chooses “Yes”.
///   - `false` if the user chooses “No” or if the call fails for any reason.
///
/// # Side Effects
/// - Blocks until the user dismisses the dialog.
/// - Shows a native Windows message box on the screen, capturing the user’s response.
///
/// # Example
/// ```no_run
/// if show_confirmation_box("Are you sure you want to continue?", "Confirm Action") {
///     println!("User clicked Yes.");
/// } else {
///     println!("User clicked No or closed the dialog.");
/// }
/// ```
///
/// # Notes
/// - This function is **Windows-specific** due to its use of the native message box API.
/// - For an informational or one-button dialog, use
///   [`show_message_box`](#fn.show_message_box) instead.
pub fn show_confirmation_box(message: &str, title: &str) -> bool {
    unsafe {
        let result = MessageBoxW(
            HWND(ptr::null_mut()), // Null pointer for no parent window
            PCWSTR(
                message
                    .encode_utf16()
                    .chain(Some(0))
                    .collect::<Vec<u16>>()
                    .as_ptr(),
            ),
            PCWSTR(
                title
                    .encode_utf16()
                    .chain(Some(0))
                    .collect::<Vec<u16>>()
                    .as_ptr(),
            ),
            MB_YESNO | MB_ICONQUESTION,
        );

        result == windows::Win32::UI::WindowsAndMessaging::MESSAGEBOX_RESULT(6) // IDYES is defined as 6
    }
}
