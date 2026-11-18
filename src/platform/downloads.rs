//! Resolve the current user's redirected Downloads known folder without creating it.
use std::{ffi::OsString, io, os::windows::ffi::OsStringExt, path::PathBuf};
use windows::Win32::{
    Foundation::HANDLE,
    System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize},
    UI::Shell::{FOLDERID_Downloads, KF_FLAG_DONT_VERIFY},
};
use windows::core::{GUID, HRESULT, PWSTR};

// The generated convenience wrapper discards the out pointer on failure.
// Keep ownership of the pointer so it can be freed for either HRESULT.
#[link(name = "shell32")]
unsafe extern "system" {
    fn SHGetKnownFolderPath(
        folder: *const GUID,
        flags: u32,
        token: HANDLE,
        path: *mut PWSTR,
    ) -> HRESULT;
}

struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() }
    }
}
struct TaskPath(PWSTR);
impl Drop for TaskPath {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            unsafe { CoTaskMemFree(Some(self.0.0.cast_const().cast())) }
        }
    }
}

pub(crate) fn downloads_known_folder() -> io::Result<PathBuf> {
    // This function runs on the caller's thread; an incompatible apartment is
    // reported instead of silently changing or borrowing its COM ownership.
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
        .ok()
        .map_err(io::Error::other)?;
    let _apartment = Apartment;
    let mut path = TaskPath(PWSTR(std::ptr::null_mut()));
    // SAFETY: COM is initialized and the RAII owner supplies a writable out pointer.
    unsafe {
        SHGetKnownFolderPath(
            &FOLDERID_Downloads,
            KF_FLAG_DONT_VERIFY.0 as u32,
            HANDLE::default(),
            &mut path.0,
        )
    }
    .ok()
    .map_err(io::Error::other)?;
    if path.0.0.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Downloads known folder returned an empty path",
        ));
    }
    // SAFETY: the shell supplies a NUL-terminated task-allocated UTF-16 string.
    let mut len = 0usize;
    while len < 32_767 && unsafe { *path.0.0.add(len) } != 0 {
        len += 1;
    }
    if len == 0 || len == 32_767 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Downloads known folder path is empty or too long",
        ));
    }
    let units = unsafe { std::slice::from_raw_parts(path.0.0, len) };
    Ok(PathBuf::from(OsString::from_wide(units)))
}
