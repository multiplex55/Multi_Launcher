//! Owned printer-device review and bounded GDI spooling. No chooser or UI.
use super::{PaginatedDocument, Rect, raster::raster_page};
use std::{
    cell::RefCell,
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use windows::{
    Win32::{
        Foundation::BOOL,
        Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteDC, GetDeviceCaps,
            GetMapMode, HDC, HORZRES, LOGPIXELSX, LOGPIXELSY, MM_TEXT, PHYSICALHEIGHT,
            PHYSICALOFFSETX, PHYSICALOFFSETY, PHYSICALWIDTH, RASTERCAPS, RC_STRETCHDIB,
            SP_APPABORT, SP_OUTOFDISK, SP_USERABORT, SRCCOPY, StretchDIBits, VERTRES,
        },
        Storage::Xps::{AbortDoc, DOCINFOW, EndDoc, EndPage, SetAbortProc, StartDocW, StartPage},
    },
    core::PCWSTR,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrinterError {
    InvalidDevice,
    UnsupportedRaster,
    SpoolFull,
    InvalidSelection,
    InvalidPage,
    InvalidName,
    Cancelled,
    Stale,
    Device { operation: &'static str, code: i32 },
    Raster(String),
    AbortFailed { cause: String, code: i32 },
}
impl fmt::Display for PrinterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDevice => f.write_str("Printer device metrics are invalid."),
            Self::UnsupportedRaster => {
                f.write_str("This printer cannot accept the prepared page raster.")
            }
            Self::SpoolFull => f.write_str("The Windows print spool ran out of disk space."),
            Self::InvalidSelection => f.write_str("Choose valid, increasing page numbers."),
            Self::InvalidPage => f.write_str("Page geometry is invalid."),
            Self::InvalidName => f.write_str("Print job name is invalid."),
            Self::Cancelled => f.write_str("Print cancelled."),
            Self::Stale => f.write_str("Print preview is no longer current."),
            Self::Device { operation, code } => write!(f, "Printer {operation} failed ({code})."),
            Self::Raster(message) => write!(f, "Could not rasterize print page: {message}"),
            Self::AbortFailed { cause, code } => {
                write!(f, "{cause} Aborting the printer job also failed ({code}).")
            }
        }
    }
}
impl std::error::Error for PrinterError {}

/// Exclusively owned printer DC. The chooser must transfer the DC once and
/// stop using it; `DeleteDC` runs exactly once when this value is dropped.
pub struct OwnedPrinterDc(HDC);
// SAFETY: construction requires unique ownership; all GDI use and destruction
// occur on the receiving worker, with no simultaneous use on the chooser thread.
unsafe impl Send for OwnedPrinterDc {}
impl OwnedPrinterDc {
    /// # Safety
    /// `hdc` must be a valid printer DC uniquely owned by the caller. After a
    /// successful call, the caller must never use or delete it again.
    pub unsafe fn from_raw(hdc: HDC) -> Result<Self, PrinterError> {
        if hdc.0.is_null() {
            Err(PrinterError::InvalidDevice)
        } else {
            Ok(Self(hdc))
        }
    }
}
impl Drop for OwnedPrinterDc {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteDC(self.0);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintScale {
    ActualSize,
    Fit,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeviceMetrics {
    pub dpi_x: u32,
    pub dpi_y: u32,
    pub physical_width_px: u32,
    pub physical_height_px: u32,
    pub printable_width_px: u32,
    pub printable_height_px: u32,
    pub offset_x_px: u32,
    pub offset_y_px: u32,
}
#[derive(Debug, Clone, PartialEq)]
pub struct PagePlacement {
    pub page: usize,
    pub scale: f64,
    pub fit_percent: f64,
    /// Device coordinates are relative to the printable origin.
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub paper_mismatch: bool,
    pub painted_clipped: bool,
}
#[derive(Debug, Clone, PartialEq)]
pub struct PrintReview {
    pub device: DeviceMetrics,
    pub pages: Vec<PagePlacement>,
}
pub struct PreparedPrint {
    dc: OwnedPrinterDc,
    scene: Arc<PaginatedDocument>,
    review: PrintReview,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrintSubmission {
    pub job_id: u32,
}

fn metrics(dc: HDC) -> Result<DeviceMetrics, PrinterError> {
    let get = |index| unsafe { GetDeviceCaps(dc, index) };
    if get(RASTERCAPS) as u32 & RC_STRETCHDIB == 0 {
        return Err(PrinterError::UnsupportedRaster);
    }
    let (dx, dy, pw, ph, ox, oy, rw, rh) = (
        get(LOGPIXELSX),
        get(LOGPIXELSY),
        get(PHYSICALWIDTH),
        get(PHYSICALHEIGHT),
        get(PHYSICALOFFSETX),
        get(PHYSICALOFFSETY),
        get(HORZRES),
        get(VERTRES),
    );
    if unsafe { GetMapMode(dc) } != MM_TEXT
        || dx <= 0
        || dy <= 0
        || pw <= 0
        || ph <= 0
        || ox < 0
        || oy < 0
        || rw <= 0
        || rh <= 0
        || ox.checked_add(rw).is_none_or(|v| v > pw)
        || oy.checked_add(rh).is_none_or(|v| v > ph)
    {
        return Err(PrinterError::InvalidDevice);
    }
    Ok(DeviceMetrics {
        dpi_x: dx as u32,
        dpi_y: dy as u32,
        physical_width_px: pw as u32,
        physical_height_px: ph as u32,
        printable_width_px: rw as u32,
        printable_height_px: rh as u32,
        offset_x_px: ox as u32,
        offset_y_px: oy as u32,
    })
}
fn rounded(value: f64) -> Result<i32, PrinterError> {
    if !value.is_finite() || value.round() < i32::MIN as f64 || value.round() > i32::MAX as f64 {
        return Err(PrinterError::InvalidPage);
    }
    Ok(value.round() as i32)
}
fn placement(
    device: DeviceMetrics,
    page: usize,
    width: f32,
    height: f32,
    ink: Option<Rect>,
    mode: PrintScale,
) -> Result<PagePlacement, PrinterError> {
    let (w, h) = (f64::from(width), f64::from(height));
    if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 || w > 2880.0 || h > 2880.0 {
        return Err(PrinterError::InvalidPage);
    }
    let physical_w = f64::from(device.physical_width_px) * 72.0 / f64::from(device.dpi_x);
    let physical_h = f64::from(device.physical_height_px) * 72.0 / f64::from(device.dpi_y);
    let dpi_x = f64::from(device.dpi_x);
    let dpi_y = f64::from(device.dpi_y);
    let printable_w = f64::from(device.printable_width_px);
    let printable_h = f64::from(device.printable_height_px);
    let (scale, x, y) = match mode {
        PrintScale::Fit => {
            let scale = (printable_w * 72.0 / (w * dpi_x)).min(printable_h * 72.0 / (h * dpi_y));
            (
                scale,
                (printable_w - w * scale * dpi_x / 72.0) / 2.0,
                (printable_h - h * scale * dpi_y / 72.0) / 2.0,
            )
        }
        PrintScale::ActualSize => (
            1.0,
            -f64::from(device.offset_x_px),
            -f64::from(device.offset_y_px),
        ),
    };
    let x = rounded(x)?;
    let y = rounded(y)?;
    let width = rounded(w * scale * dpi_x / 72.0)?;
    let height = rounded(h * scale * dpi_y / 72.0)?;
    if width <= 0 || height <= 0 {
        return Err(PrinterError::InvalidPage);
    }
    let sx = f64::from(width) / w;
    let sy = f64::from(height) / h;
    let painted_clipped = if let Some(r) = ink {
        if ![r.x, r.y, r.width, r.height].iter().all(|v| v.is_finite())
            || r.width < 0.0
            || r.height < 0.0
        {
            return Err(PrinterError::InvalidPage);
        }
        let x0 = f64::from(x) + f64::from(r.x) * sx;
        let y0 = f64::from(y) + f64::from(r.y) * sy;
        let x1 = x0 + f64::from(r.width) * sx;
        let y1 = y0 + f64::from(r.height) * sy;
        x0 < 0.0
            || y0 < 0.0
            || x1 > f64::from(device.printable_width_px)
            || y1 > f64::from(device.printable_height_px)
    } else {
        false
    };
    Ok(PagePlacement {
        page,
        scale,
        fit_percent: scale * 100.0,
        x,
        y,
        width,
        height,
        paper_mismatch: (w - physical_w).abs() > (72.0 / dpi_x).max(0.1)
            || (h - physical_h).abs() > (72.0 / dpi_y).max(0.1),
        painted_clipped,
    })
}
fn validate_selection(pages: &[usize], total: usize) -> Result<(), PrinterError> {
    if pages.is_empty()
        || pages.len() > super::MAX_PAGES
        || pages.windows(2).any(|p| p[0] >= p[1])
        || pages.iter().any(|p| *p == 0 || *p > total)
    {
        Err(PrinterError::InvalidSelection)
    } else {
        Ok(())
    }
}
/// Bind the selected device to the exact immutable page snapshot. The page
/// numbers are increasing, unique and 1-based.
pub fn prepare_print(
    dc: OwnedPrinterDc,
    scene: Arc<PaginatedDocument>,
    pages: Vec<usize>,
    mode: PrintScale,
) -> Result<PreparedPrint, PrinterError> {
    validate_selection(&pages, scene.pages.len())?;
    let device = metrics(dc.0)?;
    let review_pages = pages
        .into_iter()
        .map(|number| {
            let page = &scene.pages[number - 1];
            placement(
                device,
                number,
                page.geometry.width,
                page.geometry.height,
                page.painted_bounds,
                mode,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PreparedPrint {
        dc,
        scene,
        review: PrintReview {
            device,
            pages: review_pages,
        },
    })
}
impl PreparedPrint {
    pub fn review(&self) -> &PrintReview {
        &self.review
    }
    /// Consumes the reviewed device and scene. Success means Windows accepted
    /// the completed job, not that paper physically emerged from a printer.
    pub fn submit(
        self,
        name: &str,
        cancel: Arc<AtomicBool>,
        source_current: impl FnMut() -> bool,
        admit: impl FnOnce() -> bool,
    ) -> Result<PrintSubmission, PrinterError> {
        self.submit_inner(name, None, cancel, source_current, admit)
    }
    fn submit_inner(
        self,
        name: &str,
        output: Option<&std::path::Path>,
        cancel: Arc<AtomicBool>,
        mut source_current: impl FnMut() -> bool,
        admit: impl FnOnce() -> bool,
    ) -> Result<PrintSubmission, PrinterError> {
        use std::os::windows::ffi::OsStrExt;
        if name.is_empty() || name.len() > 512 || name.contains('\0') {
            return Err(PrinterError::InvalidName);
        }
        if cancel.load(Ordering::Acquire) {
            return Err(PrinterError::Cancelled);
        }
        if !source_current() {
            return Err(PrinterError::Stale);
        }
        let name: Vec<u16> = std::ffi::OsStr::new(name)
            .encode_wide()
            .chain(Some(0))
            .collect();
        let output: Option<Vec<u16>> =
            output.map(|p| p.as_os_str().encode_wide().chain(Some(0)).collect());
        let callback = AbortScope::enter(&cancel)?;
        let dc = self.dc.0;
        let status = unsafe { SetAbortProc(dc, Some(abort_callback)) };
        if status <= 0 {
            return Err(callback.failure("SetAbortProc", status));
        }
        if cancel.load(Ordering::Acquire) {
            return Err(PrinterError::Cancelled);
        }
        if !source_current() {
            return Err(PrinterError::Stale);
        }
        let info = DOCINFOW {
            cbSize: std::mem::size_of::<DOCINFOW>() as i32,
            lpszDocName: PCWSTR(name.as_ptr()),
            lpszOutput: output
                .as_ref()
                .map_or(PCWSTR::null(), |v| PCWSTR(v.as_ptr())),
            ..Default::default()
        };
        if !admit() || cancel.load(Ordering::Acquire) {
            return Err(PrinterError::Cancelled);
        }
        let job = unsafe { StartDocW(dc, &info) };
        if job <= 0 {
            return Err(callback.failure("StartDoc", job));
        }
        let mut active = ActiveJob { dc, ended: false };
        let result = (|| {
            for entry in &self.review.pages {
                if cancel.load(Ordering::Acquire) {
                    return Err(PrinterError::Cancelled);
                }
                if !source_current() {
                    return Err(PrinterError::Stale);
                }
                // Match effective physical sample density while keeping raster and
                // driver transfer costs bounded on high-resolution devices.
                let dpi = self
                    .review
                    .device
                    .dpi_x
                    .max(self.review.device.dpi_y)
                    .min(300);
                let sampling = (f64::from(dpi) * entry.scale / 72.0) as f32;
                let mut raster = raster_page(&self.scene, entry.page - 1, sampling, || {
                    cancel.load(Ordering::Acquire)
                })
                .map_err(|e| {
                    if cancel.load(Ordering::Acquire) {
                        PrinterError::Cancelled
                    } else {
                        PrinterError::Raster(e.to_string())
                    }
                })?;
                for chunk in raster.rgba.chunks_mut(16384) {
                    if cancel.load(Ordering::Acquire) {
                        return Err(PrinterError::Cancelled);
                    }
                    for pixel in chunk.as_chunks_mut::<4>().0 {
                        pixel.swap(0, 2);
                    }
                }
                if cancel.load(Ordering::Acquire) {
                    return Err(PrinterError::Cancelled);
                }
                if !source_current() {
                    return Err(PrinterError::Stale);
                }
                let status = unsafe { StartPage(dc) };
                if status <= 0 {
                    return Err(callback.failure("StartPage", status));
                }
                if cancel.load(Ordering::Acquire) {
                    return Err(PrinterError::Cancelled);
                }
                if !source_current() {
                    return Err(PrinterError::Stale);
                }
                let bitmap = BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER {
                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: raster.width as i32,
                        biHeight: -(raster.height as i32),
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: BI_RGB.0,
                        biSizeImage: raster.rgba.len() as u32,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let status = unsafe {
                    StretchDIBits(
                        dc,
                        entry.x,
                        entry.y,
                        entry.width,
                        entry.height,
                        0,
                        0,
                        raster.width as i32,
                        raster.height as i32,
                        Some(raster.rgba.as_ptr().cast()),
                        &bitmap,
                        DIB_RGB_COLORS,
                        SRCCOPY,
                    )
                };
                if status <= 0 {
                    return Err(callback.failure("StretchDIBits", status));
                }
                if cancel.load(Ordering::Acquire) {
                    return Err(PrinterError::Cancelled);
                }
                if !source_current() {
                    return Err(PrinterError::Stale);
                }
                let status = unsafe { EndPage(dc) };
                if status <= 0 {
                    return Err(callback.failure("EndPage", status));
                }
            }
            if cancel.load(Ordering::Acquire) {
                return Err(PrinterError::Cancelled);
            }
            if !source_current() {
                return Err(PrinterError::Stale);
            }
            let status = unsafe { EndDoc(dc) };
            if status <= 0 {
                return Err(callback.failure("EndDoc", status));
            }
            active.ended = true;
            Ok(PrintSubmission { job_id: job as u32 })
        })();
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                let status = active.abort();
                if status <= 0 {
                    Err(PrinterError::AbortFailed {
                        cause: error.to_string(),
                        code: status,
                    })
                } else {
                    Err(error)
                }
            }
        }
    }
}
struct ActiveJob {
    dc: HDC,
    ended: bool,
}
impl ActiveJob {
    fn abort(&mut self) -> i32 {
        if self.ended {
            return 1;
        }
        self.ended = true;
        unsafe { AbortDoc(self.dc) }
    }
}
impl Drop for ActiveJob {
    fn drop(&mut self) {
        if !self.ended {
            let _ = self.abort();
        }
    }
}
struct AbortState {
    cancelled: Arc<AtomicBool>,
    spool_full: bool,
}
thread_local! {
    static ABORT_STATE: RefCell<Option<AbortState>> = const { RefCell::new(None) };
}
struct AbortScope;
impl AbortScope {
    fn enter(cancelled: &Arc<AtomicBool>) -> Result<Self, PrinterError> {
        ABORT_STATE.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot.is_some() {
                return Err(PrinterError::InvalidDevice);
            }
            *slot = Some(AbortState {
                cancelled: Arc::clone(cancelled),
                spool_full: false,
            });
            Ok(Self)
        })
    }

    fn failure(&self, operation: &'static str, code: i32) -> PrinterError {
        ABORT_STATE.with(|slot| {
            let slot = slot.borrow();
            if code == SP_OUTOFDISK || slot.as_ref().is_some_and(|state| state.spool_full) {
                PrinterError::SpoolFull
            } else if matches!(code, SP_APPABORT | SP_USERABORT)
                || slot
                    .as_ref()
                    .is_some_and(|state| state.cancelled.load(Ordering::Acquire))
            {
                PrinterError::Cancelled
            } else {
                PrinterError::Device { operation, code }
            }
        })
    }
}
impl Drop for AbortScope {
    fn drop(&mut self) {
        ABORT_STATE.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}
unsafe extern "system" fn abort_callback(_: HDC, error: i32) -> BOOL {
    ABORT_STATE
        .try_with(|slot| {
            let Ok(mut slot) = slot.try_borrow_mut() else {
                return BOOL(0);
            };
            let Some(state) = slot.as_mut() else {
                return BOOL(0);
            };
            // A worker does not pump window messages. Stop rather than continuing
            // after the spooler reports insufficient disk space.
            state.spool_full |= error == SP_OUTOFDISK;
            BOOL(i32::from(
                !state.spool_full && !state.cancelled.load(Ordering::Acquire),
            ))
        })
        .unwrap_or(BOOL(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn device() -> DeviceMetrics {
        DeviceMetrics {
            dpi_x: 300,
            dpi_y: 600,
            physical_width_px: 2550,
            physical_height_px: 6600,
            printable_width_px: 2400,
            printable_height_px: 6300,
            offset_x_px: 75,
            offset_y_px: 150,
        }
    }
    #[test]
    fn printer_placement_reports_hard_margins_and_fit() {
        let actual = placement(
            device(),
            1,
            612.0,
            792.0,
            Some(Rect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
            }),
            PrintScale::ActualSize,
        )
        .unwrap();
        assert_eq!((actual.x, actual.y), (-75, -150));
        assert!(!actual.paper_mismatch && actual.painted_clipped);
        let fit = placement(
            device(),
            1,
            720.0,
            720.0,
            Some(Rect {
                x: 0.0,
                y: 0.0,
                width: 720.0,
                height: 720.0,
            }),
            PrintScale::Fit,
        )
        .unwrap();
        assert_eq!((fit.x, fit.y, fit.width, fit.height), (0, 750, 2400, 4800));
        assert!(
            (fit.fit_percent - 80.0).abs() < 0.01 && fit.paper_mismatch && !fit.painted_clipped
        );
        assert!(placement(device(), 1, f32::NAN, 100.0, None, PrintScale::Fit).is_err());
    }
    #[test]
    fn printer_selection_is_ordered_unique_and_one_based() {
        assert!(validate_selection(&[1, 3], 3).is_ok());
        for pages in [&[][..], &[0][..], &[4][..], &[2, 1][..], &[1, 1][..]] {
            assert_eq!(
                validate_selection(pages, 3),
                Err(PrinterError::InvalidSelection)
            );
        }
    }
    #[test]
    fn printer_callback_distinguishes_cancel_and_spool_exhaustion() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let callback = AbortScope::enter(&cancelled).unwrap();
        assert!(unsafe { abort_callback(HDC::default(), 0) }.as_bool());
        assert_eq!(
            callback.failure("StartDoc", SP_USERABORT),
            PrinterError::Cancelled
        );
        cancelled.store(true, Ordering::Release);
        assert!(!unsafe { abort_callback(HDC::default(), 0) }.as_bool());
        assert_eq!(callback.failure("EndDoc", 0), PrinterError::Cancelled);
        assert!(!unsafe { abort_callback(HDC::default(), SP_OUTOFDISK) }.as_bool());
        assert_eq!(callback.failure("EndDoc", 0), PrinterError::SpoolFull);
    }

    #[test]
    #[ignore = "submits a real job to Microsoft Print to PDF and needs a fresh output path"]
    fn printer_opt_in_microsoft_pdf() {
        use std::{ffi::OsStr, os::windows::ffi::OsStrExt};
        use windows::Win32::Graphics::Gdi::CreateDCW;
        let output = std::path::PathBuf::from(
            std::env::var_os("MULTI_LAUNCHER_TEST_PRINTER_PDF")
                .expect("set an absolute, fresh disposable PDF output path"),
        );
        assert!(output.is_absolute() && !output.exists());
        let wide = |s: &str| {
            OsStr::new(s)
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>()
        };
        let driver = wide("WINSPOOL");
        let device = wide("Microsoft Print to PDF");
        let raw = unsafe {
            CreateDCW(
                PCWSTR(driver.as_ptr()),
                PCWSTR(device.as_ptr()),
                PCWSTR::null(),
                None,
            )
        };
        let dc = unsafe { OwnedPrinterDc::from_raw(raw) }.unwrap();
        let scene = Arc::new(
            super::super::paginate(
                "# Printer test\n\nCafé\n\n<!-- pagebreak -->\n\n# Second printer page\n\nPrinted from the same prepared scene.",
                &super::super::PrintSettings::default(),
                "",
                || false,
            )
            .unwrap(),
        );
        let worker_output = output.clone();
        let result = std::thread::spawn(move || {
            prepare_print(dc, scene, vec![1, 2], PrintScale::Fit)?.submit_inner(
                "Multi Launcher test",
                Some(&worker_output),
                Arc::new(AtomicBool::new(false)),
                || true,
                || true,
            )
        })
        .join()
        .unwrap()
        .unwrap();
        eprintln!("Printer job {} submitted", result.job_id);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            if std::fs::read(&output).is_ok_and(|bytes| {
                bytes.starts_with(b"%PDF-") && bytes.trim_ascii_end().ends_with(b"%%EOF")
            }) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "printer did not produce PDF bytes"
            );
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}
