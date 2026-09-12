//! Passive per-pixel-alpha annotation surface used by Ghost and Finish modes.
//!
//! The window never owns input. It exactly covers the signed virtual-desktop
//! capture bounds and is updated only when its caller supplies changed pixels.

use image::RgbaImage;

/// Converts straight-alpha RGBA into the premultiplied BGRA required by
/// `UpdateLayeredWindow(ULW_ALPHA)`.
pub(crate) fn premultiplied_bgra(image: &RgbaImage, output: &mut [u8]) -> Result<(), &'static str> {
    if output.len() != image.as_raw().len() {
        return Err("overlay pixel buffer length does not match image");
    }
    for (source, destination) in image
        .as_raw()
        .chunks_exact(4)
        .zip(output.chunks_exact_mut(4))
    {
        let alpha = u16::from(source[3]);
        let premultiply = |channel: u8| ((u16::from(channel) * alpha + 127) / 255) as u8;
        destination.copy_from_slice(&[
            premultiply(source[2]),
            premultiply(source[1]),
            premultiply(source[0]),
            source[3],
        ]);
    }
    Ok(())
}

#[cfg(windows)]
mod windows_overlay {
    use std::{mem, ptr};

    use image::RgbaImage;
    use windows::Win32::Foundation::{
        COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM,
    };
    use windows::Win32::Graphics::Gdi::{
        AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
        CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, HBITMAP, HDC,
        SelectObject,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, HTTRANSPARENT, RegisterClassW, SW_HIDE,
        SW_SHOWNOACTIVATE, ShowWindow, ULW_ALPHA, UpdateLayeredWindow, WM_NCHITTEST, WNDCLASSW,
        WS_POPUP,
    };
    use windows::core::{PCWSTR, w};

    use super::premultiplied_bgra;
    use crate::screen_draw::DesktopRect;
    use crate::screen_draw::window_layers::{
        NativeWindowHandle, ToolbarZOrderCeiling, passive_overlay_extended_style,
    };

    unsafe extern "system" fn overlay_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == WM_NCHITTEST {
            return LRESULT(HTTRANSPARENT as isize);
        }
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }

    struct LayeredDib {
        dc: HDC,
        bitmap: HBITMAP,
        old_bitmap: windows::Win32::Graphics::Gdi::HGDIOBJ,
        bits: *mut u8,
        byte_len: usize,
    }

    impl LayeredDib {
        unsafe fn new(width: u32, height: u32) -> Result<Self, String> {
            let width = i32::try_from(width).map_err(|_| "overlay width is too large")?;
            let height = i32::try_from(height).map_err(|_| "overlay height is too large")?;
            let byte_len = (width as usize)
                .checked_mul(height as usize)
                .and_then(|pixels| pixels.checked_mul(4))
                .ok_or("overlay allocation is too large")?;
            let dc = unsafe { CreateCompatibleDC(None) };
            if dc.0.is_null() {
                return Err("CreateCompatibleDC failed for passive overlay".into());
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                bmiColors: [Default::default()],
            };
            let mut bits = ptr::null_mut();
            let bitmap =
                match unsafe { CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, None, 0) } {
                    Ok(bitmap) if !bits.is_null() => bitmap,
                    _ => {
                        let _ = unsafe { DeleteDC(dc) };
                        return Err("CreateDIBSection failed for passive overlay".into());
                    }
                };
            let old_bitmap = unsafe { SelectObject(dc, bitmap) };
            Ok(Self {
                dc,
                bitmap,
                old_bitmap,
                bits: bits.cast(),
                byte_len,
            })
        }
    }

    impl Drop for LayeredDib {
        fn drop(&mut self) {
            unsafe {
                let _ = SelectObject(self.dc, self.old_bitmap);
                let _ = DeleteObject(self.bitmap);
                let _ = DeleteDC(self.dc);
            }
        }
    }

    pub(crate) struct NativeOverlaySurface {
        hwnd: HWND,
        bounds: DesktopRect,
        backing: LayeredDib,
        visible: bool,
        toolbar_ceiling: ToolbarZOrderCeiling,
    }

    // Created, updated, and destroyed on the owning native worker thread.
    unsafe impl Send for NativeOverlaySurface {}

    impl NativeOverlaySurface {
        pub(crate) fn create(
            bounds: DesktopRect,
            toolbar_ceiling: ToolbarZOrderCeiling,
        ) -> Result<Self, String> {
            let instance = HINSTANCE(
                unsafe { GetModuleHandleW(None) }
                    .map_err(|error| error.to_string())?
                    .0,
            );
            let class_name = w!("MultiLauncherScreenDrawPassiveOverlay");
            let class = WNDCLASSW {
                lpfnWndProc: Some(overlay_proc),
                hInstance: instance,
                lpszClassName: class_name,
                ..Default::default()
            };
            unsafe {
                RegisterClassW(&class);
            }
            let width = i32::try_from(bounds.width).map_err(|_| "overlay width is too large")?;
            let height = i32::try_from(bounds.height).map_err(|_| "overlay height is too large")?;
            let hwnd = unsafe {
                CreateWindowExW(
                    passive_overlay_extended_style(),
                    class_name,
                    PCWSTR::null(),
                    WS_POPUP,
                    bounds.x,
                    bounds.y,
                    width,
                    height,
                    None,
                    None,
                    instance,
                    None,
                )
            }
            .map_err(|error| format!("CreateWindowExW failed for passive overlay: {error}"))?;
            let backing = match unsafe { LayeredDib::new(bounds.width, bounds.height) } {
                Ok(backing) => backing,
                Err(error) => {
                    unsafe {
                        let _ = DestroyWindow(hwnd);
                    }
                    return Err(error);
                }
            };
            let mut surface = Self {
                hwnd,
                bounds,
                backing,
                visible: false,
                toolbar_ceiling: ToolbarZOrderCeiling::default(),
            };
            surface.set_toolbar_z_order_ceiling(toolbar_ceiling);
            Ok(surface)
        }

        fn native_handle(&self) -> NativeWindowHandle {
            NativeWindowHandle::from_raw(self.hwnd.0 as isize)
                .expect("created passive overlay has a non-null HWND")
        }

        fn enforce_toolbar_ceiling(&mut self) {
            let handle = self.native_handle();
            self.toolbar_ceiling.place_passive_below(handle);
        }

        pub(crate) fn set_toolbar_z_order_ceiling(
            &mut self,
            toolbar_ceiling: ToolbarZOrderCeiling,
        ) {
            self.toolbar_ceiling = toolbar_ceiling;
            self.enforce_toolbar_ceiling();
        }

        pub(crate) fn update(&mut self, image: &RgbaImage) -> Result<(), String> {
            if image.dimensions() != (self.bounds.width, self.bounds.height) {
                return Err("passive overlay image dimensions do not match capture".into());
            }
            let destination =
                unsafe { std::slice::from_raw_parts_mut(self.backing.bits, self.backing.byte_len) };
            premultiplied_bgra(image, destination).map_err(str::to_string)?;
            let destination_point = POINT {
                x: self.bounds.x,
                y: self.bounds.y,
            };
            let size = SIZE {
                cx: i32::try_from(self.bounds.width).map_err(|_| "overlay width is too large")?,
                cy: i32::try_from(self.bounds.height).map_err(|_| "overlay height is too large")?,
            };
            let source_point = POINT::default();
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            unsafe {
                UpdateLayeredWindow(
                    self.hwnd,
                    None,
                    Some(&destination_point),
                    Some(&size),
                    self.backing.dc,
                    Some(&source_point),
                    COLORREF(0),
                    Some(&blend),
                    ULW_ALPHA,
                )
            }
            .map_err(|error| format!("UpdateLayeredWindow failed: {error}"))?;
            self.enforce_toolbar_ceiling();
            Ok(())
        }

        pub(crate) fn show(&mut self) {
            if !self.visible {
                unsafe {
                    let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                }
                self.visible = true;
            }
            self.enforce_toolbar_ceiling();
        }

        pub(crate) fn hide(&mut self) {
            if !self.visible {
                return;
            }
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            self.visible = false;
        }
    }

    impl Drop for NativeOverlaySurface {
        fn drop(&mut self) {
            self.hide();
            unsafe {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

#[cfg(windows)]
pub(crate) use windows_overlay::NativeOverlaySurface;

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn converts_straight_rgba_to_premultiplied_bgra() {
        let image = RgbaImage::from_pixel(1, 1, Rgba([200, 100, 50, 128]));
        let mut output = [0; 4];
        premultiplied_bgra(&image, &mut output).unwrap();
        assert_eq!(output, [25, 50, 100, 128]);
    }

    #[test]
    fn transparent_pixels_have_zero_rgb_and_length_is_checked() {
        let image = RgbaImage::from_pixel(1, 1, Rgba([255, 200, 100, 0]));
        let mut output = [9; 4];
        premultiplied_bgra(&image, &mut output).unwrap();
        assert_eq!(output, [0, 0, 0, 0]);
        assert!(premultiplied_bgra(&image, &mut []).is_err());
    }
}
