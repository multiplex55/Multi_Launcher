//! Shared native GDI segment rendering used by desktop overlay surfaces.

/// A segment translated into the target device context's local coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalSegment {
    pub from: (i32, i32),
    pub to: (i32, i32),
}

/// Converts an RGB triplet to Win32's `0x00bbggrr` `COLORREF` value.
pub const fn rgb_colorref_value(color: [u8; 3]) -> u32 {
    (color[0] as u32) | ((color[1] as u32) << 8) | ((color[2] as u32) << 16)
}

/// Applies the width conversion used by the native Mouse Gesture trail.
pub fn gdi_pen_width(width: f32) -> i32 {
    width.max(1.0) as i32
}

/// Translates desktop-space floating-point endpoints into local GDI coordinates.
///
/// Conversion intentionally occurs before origin subtraction to preserve the
/// Mouse Gesture trail's existing truncation behavior. Offscreen callers can
/// pass `(0, 0)` when their input is already relative to the target bitmap.
pub fn translate_segment(from: (f32, f32), to: (f32, f32), origin: (i32, i32)) -> LocalSegment {
    LocalSegment {
        from: (from.0 as i32 - origin.0, from.1 as i32 - origin.1),
        to: (to.0 as i32 - origin.0, to.1 as i32 - origin.1),
    }
}

/// Draws one solid native GDI segment with the same pen lifecycle as the Mouse
/// Gesture trail.
///
/// # Safety
///
/// `hdc` must be a valid device context for the duration of this call and must
/// remain owned by the caller. The helper restores the previously selected pen
/// before deleting the temporary pen it creates.
#[cfg(windows)]
pub unsafe fn draw_solid_segment(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    from: (f32, f32),
    to: (f32, f32),
    origin: (i32, i32),
    color: [u8; 3],
    width: f32,
) {
    use windows::Win32::Foundation::COLORREF;
    use windows::Win32::Graphics::Gdi::{
        CreatePen, DeleteObject, LineTo, MoveToEx, PS_SOLID, SelectObject,
    };

    let segment = translate_segment(from, to, origin);
    let pen = unsafe {
        CreatePen(
            PS_SOLID,
            gdi_pen_width(width),
            COLORREF(rgb_colorref_value(color)),
        )
    };
    let old_pen = unsafe { SelectObject(hdc, pen) };

    let _ = unsafe { MoveToEx(hdc, segment.from.0, segment.from.1, None) };
    let _ = unsafe { LineTo(hdc, segment.to.0, segment.to.1) };

    unsafe {
        SelectObject(hdc, old_pen);
        let _ = DeleteObject(pen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colorref_preserves_mouse_gesture_rgb_mapping() {
        assert_eq!(rgb_colorref_value([0x12, 0x34, 0x56]), 0x0056_3412);
        assert_eq!(rgb_colorref_value([255, 0, 0]), 0x0000_00ff);
        assert_eq!(rgb_colorref_value([0, 0, 255]), 0x00ff_0000);
    }

    #[test]
    fn pen_width_preserves_clamp_and_truncation() {
        assert_eq!(gdi_pen_width(0.0), 1);
        assert_eq!(gdi_pen_width(0.75), 1);
        assert_eq!(gdi_pen_width(1.99), 1);
        assert_eq!(gdi_pen_width(5.9), 5);
    }

    #[test]
    fn translation_preserves_signed_origin_and_truncation() {
        assert_eq!(
            translate_segment((-1919.8, -1079.2), (-1800.9, -900.7), (-1920, -1080)),
            LocalSegment {
                from: (1, 1),
                to: (120, 180),
            }
        );
        assert_eq!(
            translate_segment((10.9, 20.1), (30.7, 40.9), (0, 0)),
            LocalSegment {
                from: (10, 20),
                to: (30, 40),
            }
        );
    }
}
