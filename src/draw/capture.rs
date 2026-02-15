use crate::draw::composite::RgbaBuffer;
use crate::draw::service::MonitorRect;
use anyhow::{anyhow, Result};

#[cfg(windows)]
pub fn capture_monitor_rgba(rect: MonitorRect) -> Result<RgbaBuffer> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        HGDIOBJ, SRCCOPY,
    };

    if rect.width <= 0 || rect.height <= 0 {
        return Err(anyhow!("monitor capture bounds are empty"));
    }

    unsafe {
        let screen_dc = GetDC(HWND::default());
        if screen_dc.0.is_null() {
            return Err(anyhow!("GetDC failed for desktop capture"));
        }
        let mem_dc = CreateCompatibleDC(screen_dc);
        if mem_dc.0.is_null() {
            let _ = ReleaseDC(HWND::default(), screen_dc);
            return Err(anyhow!("CreateCompatibleDC failed for desktop capture"));
        }

        let bmp = CreateCompatibleBitmap(screen_dc, rect.width, rect.height);
        if bmp.0.is_null() {
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(HWND::default(), screen_dc);
            return Err(anyhow!("CreateCompatibleBitmap failed for desktop capture"));
        }

        let old_obj = SelectObject(mem_dc, HGDIOBJ(bmp.0));
        let ok = BitBlt(
            mem_dc,
            0,
            0,
            rect.width,
            rect.height,
            screen_dc,
            rect.x,
            rect.y,
            SRCCOPY,
        )
        .is_ok();

        if !ok {
            let _ = SelectObject(mem_dc, old_obj);
            let _ = DeleteObject(bmp);
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(HWND::default(), screen_dc);
            return Err(anyhow!("BitBlt failed for desktop capture"));
        }

        let mut bmi = BITMAPINFO::default();
        bmi.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: rect.width,
            biHeight: -rect.height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };

        let mut bgra = vec![0u8; (rect.width as usize) * (rect.height as usize) * 4];
        let rows = GetDIBits(
            mem_dc,
            bmp,
            0,
            rect.height as u32,
            Some(bgra.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        let _ = SelectObject(mem_dc, old_obj);
        let _ = DeleteObject(bmp);
        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(HWND::default(), screen_dc);

        if rows == 0 {
            return Err(anyhow!("GetDIBits failed for desktop capture"));
        }

        for px in bgra.chunks_exact_mut(4) {
            px.swap(0, 2);
            px[3] = 255;
        }

        Ok(RgbaBuffer::from_pixels(
            rect.width as u32,
            rect.height as u32,
            bgra,
        ))
    }
}

#[cfg(not(windows))]
pub fn capture_monitor_rgba(_rect: MonitorRect) -> Result<RgbaBuffer> {
    Err(anyhow!("desktop capture is only implemented for Windows"))
}
