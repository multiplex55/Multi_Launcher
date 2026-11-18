//! Owner-bound Windows printer selection. No document or worker state is mutated.
use crate::notes_markdown::print_layout::{MAX_PAGES, printer::OwnedPrinterDc};
use std::{fmt, mem::size_of};
use windows::Win32::{
    Foundation::{GlobalFree, HGLOBAL, HWND},
    Graphics::Gdi::{DM_COLLATE, DM_COPIES, DeleteDC, HDC},
    System::{
        Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize},
        Memory::{GlobalLock, GlobalSize, GlobalUnlock},
        Threading::{GetCurrentProcessId, GetCurrentThreadId},
    },
    UI::{
        Controls::Dialogs::{
            DEVNAMES, PD_CURRENTPAGE, PD_EXCLUSIONFLAGS, PD_HIDEPRINTTOFILE, PD_NOSELECTION,
            PD_PAGENUMS, PD_PRINTTOFILE, PD_RESULT_APPLY, PD_RESULT_CANCEL, PD_RESULT_PRINT,
            PD_RETURNDC, PD_SELECTION, PD_USEDEVMODECOPIESANDCOLLATE, PRINTDLGEXW, PRINTPAGERANGE,
            PrintDlgExW, START_PAGE_GENERAL,
        },
        WindowsAndMessaging::GetWindowThreadProcessId,
    },
};

pub(in crate::gui) struct PrinterChoice {
    pub dc: OwnedPrinterDc,
    pub pages: Vec<usize>,
    pub device_label: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::gui) enum PrinterDialogError {
    InvalidOwner,
    InvalidPages,
    InvalidResponse,
    UnsupportedCopies,
    Com(String),
    Dialog(String),
}
impl fmt::Display for PrinterDialogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
        Self::InvalidOwner => f.write_str("The print preview window is no longer available."),
        Self::InvalidPages => f.write_str("The printer returned invalid page ranges."),
        Self::InvalidResponse => f.write_str("The printer returned an invalid selection."),
        Self::UnsupportedCopies => f.write_str("This printer requires application-managed copies. Choose one copy in printer settings."),
        Self::Com(error) => write!(f,"Could not initialize the printer dialog: {error}"),
        Self::Dialog(error) => write!(f,"Could not open the printer dialog: {error}"),
    }
    }
}
impl std::error::Error for PrinterDialogError {}
struct ComScope;
impl ComScope {
    fn enter() -> Result<Self, PrinterDialogError> {
        let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if result.is_err() {
            return Err(PrinterDialogError::Com(format!("{result:?}")));
        }
        Ok(Self)
    }
}
impl Drop for ComScope {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}
struct Resources(PRINTDLGEXW);
impl Drop for Resources {
    fn drop(&mut self) {
        unsafe {
            if !self.0.hDC.0.is_null() {
                let _ = DeleteDC(self.0.hDC);
            }
            if !self.0.hDevMode.0.is_null() {
                let _ = GlobalFree(self.0.hDevMode);
            }
            if !self.0.hDevNames.0.is_null() && self.0.hDevNames != self.0.hDevMode {
                let _ = GlobalFree(self.0.hDevNames);
            }
        }
    }
}
struct Locked(HGLOBAL);
impl Drop for Locked {
    fn drop(&mut self) {
        unsafe {
            let _ = GlobalUnlock(self.0);
        }
    }
}
fn device_label(handle: HGLOBAL) -> Option<String> {
    if handle.0.is_null() {
        return None;
    }
    let bytes = unsafe { GlobalSize(handle) };
    if !(size_of::<DEVNAMES>()..=65536).contains(&bytes) || bytes % 2 != 0 {
        return None;
    }
    let ptr = unsafe { GlobalLock(handle) };
    if ptr.is_null() {
        return None;
    }
    let _locked = Locked(handle);
    let names = unsafe { std::ptr::read_unaligned(ptr.cast::<DEVNAMES>()) };
    let offset = usize::from(names.wDeviceOffset);
    let len = bytes / 2;
    if offset < size_of::<DEVNAMES>() / 2 || offset >= len {
        return None;
    }
    let words = unsafe { std::slice::from_raw_parts(ptr.cast::<u16>(), len) };
    let rest = &words[offset..];
    let end = rest.iter().position(|v| *v == 0)?;
    if end == 0 || end > 512 {
        return None;
    }
    let result = String::from_utf16(&rest[..end]).ok()?;
    (!result.chars().any(char::is_control)).then_some(result)
}
fn printing_action(action: u32) -> Result<bool, PrinterDialogError> {
    match action {
        PD_RESULT_PRINT => Ok(true),
        PD_RESULT_CANCEL | PD_RESULT_APPLY => Ok(false),
        _ => Err(PrinterDialogError::InvalidResponse),
    }
}
fn normalize_pages(
    total: usize,
    current: usize,
    flags: u32,
    ranges: &[(u32, u32)],
) -> Result<Vec<usize>, PrinterDialogError> {
    if total == 0 || total > MAX_PAGES || current == 0 || current > total {
        return Err(PrinterDialogError::InvalidPages);
    }
    let range_mode = flags & PD_PAGENUMS.0 != 0;
    let current_mode = flags & PD_CURRENTPAGE.0 != 0;
    if flags & PD_SELECTION.0 != 0 || (range_mode && current_mode) {
        return Err(PrinterDialogError::InvalidResponse);
    }
    if current_mode {
        return Ok(vec![current]);
    }
    if !range_mode {
        return Ok((1..=total).collect());
    }
    if ranges.is_empty() || ranges.len() > MAX_PAGES {
        return Err(PrinterDialogError::InvalidPages);
    }
    let mut selected = vec![false; total];
    for &(start, end) in ranges {
        if start == 0 || end < start || end as usize > total {
            return Err(PrinterDialogError::InvalidPages);
        }
        for page in start..=end {
            selected[page as usize - 1] = true;
        }
    }
    Ok(selected
        .into_iter()
        .enumerate()
        .filter_map(|(i, yes)| yes.then_some(i + 1))
        .collect())
}
fn current_owner(hwnd: HWND) -> bool {
    let mut pid = 0;
    let thread = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    thread != 0
        && pid == unsafe { GetCurrentProcessId() }
        && thread == unsafe { GetCurrentThreadId() }
}

/// Show the native modal chooser on the thread owning the accepted preview HWND.
/// The returned DC is uniquely owned and may be transferred to the print worker.
pub(in crate::gui) fn choose_printer(
    owner: Option<usize>,
    total_pages: usize,
    current_page: usize,
) -> Result<Option<PrinterChoice>, PrinterDialogError> {
    if total_pages == 0
        || total_pages > MAX_PAGES
        || current_page == 0
        || current_page > total_pages
    {
        return Err(PrinterDialogError::InvalidPages);
    }
    let raw = owner
        .filter(|v| *v != 0)
        .ok_or(PrinterDialogError::InvalidOwner)?;
    let hwnd = HWND(raw as *mut std::ffi::c_void);
    if !current_owner(hwnd) {
        return Err(PrinterDialogError::InvalidOwner);
    }
    let _com = ComScope::enter()?;
    let mut ranges = vec![PRINTPAGERANGE::default(); MAX_PAGES];
    let mut resources = Resources(PRINTDLGEXW {
        lStructSize: size_of::<PRINTDLGEXW>() as u32,
        hwndOwner: hwnd,
        Flags: PD_RETURNDC
            | PD_USEDEVMODECOPIESANDCOLLATE
            | PD_NOSELECTION
            | PD_HIDEPRINTTOFILE
            | PD_EXCLUSIONFLAGS,
        ExclusionFlags: DM_COPIES.0 | DM_COLLATE.0,
        nMaxPageRanges: MAX_PAGES as u32,
        lpPageRanges: ranges.as_mut_ptr(),
        nMinPage: 1,
        nMaxPage: total_pages as u32,
        nCopies: 1,
        nStartPage: START_PAGE_GENERAL,
        ..Default::default()
    });
    unsafe { PrintDlgExW(&mut resources.0) }
        .map_err(|e| PrinterDialogError::Dialog(e.to_string()))?;
    if !printing_action(resources.0.dwResultAction)? {
        return Ok(None);
    }
    if !current_owner(hwnd) {
        return Err(PrinterDialogError::InvalidOwner);
    }
    if resources.0.Flags.0 & PD_PRINTTOFILE.0 != 0 {
        return Err(PrinterDialogError::InvalidResponse);
    }
    if resources.0.nCopies != 1 {
        return Err(PrinterDialogError::UnsupportedCopies);
    }
    let count = if resources.0.Flags.0 & PD_PAGENUMS.0 != 0 {
        resources.0.nPageRanges as usize
    } else {
        0
    };
    if count > ranges.len() {
        return Err(PrinterDialogError::InvalidPages);
    }
    let specified = ranges[..count]
        .iter()
        .map(|range| (range.nFromPage, range.nToPage))
        .collect::<Vec<_>>();
    let pages = normalize_pages(total_pages, current_page, resources.0.Flags.0, &specified)?;
    let label = device_label(resources.0.hDevNames);
    let raw = resources.0.hDC;
    if raw.0.is_null() {
        return Err(PrinterDialogError::InvalidResponse);
    }
    resources.0.hDC = HDC::default();
    let dc = unsafe { OwnedPrinterDc::from_raw(raw) }
        .map_err(|_| PrinterDialogError::InvalidResponse)?;
    Ok(Some(PrinterChoice {
        dc,
        pages,
        device_label: label,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn printer_chooser_ranges_are_bounded_sorted_and_unique() {
        assert_eq!(normalize_pages(5, 3, 0, &[]).unwrap(), vec![1, 2, 3, 4, 5]);
        assert_eq!(
            normalize_pages(5, 3, PD_CURRENTPAGE.0, &[]).unwrap(),
            vec![3]
        );
        assert_eq!(
            normalize_pages(5, 3, PD_PAGENUMS.0, &[(4, 5), (1, 2), (2, 4)]).unwrap(),
            vec![1, 2, 3, 4, 5]
        );
        for ranges in [&[][..], &[(0, 1)][..], &[(4, 3)][..], &[(1, 6)][..]] {
            assert!(normalize_pages(5, 3, PD_PAGENUMS.0, ranges).is_err());
        }
        assert!(normalize_pages(5, 3, PD_PAGENUMS.0 | PD_CURRENTPAGE.0, &[(1, 1)]).is_err());
        assert_eq!(
            normalize_pages(5, 3, 0, &[(1, 2)]).unwrap(),
            vec![1, 2, 3, 4, 5]
        );
    }
    #[test]
    fn printer_chooser_result_gate_and_owner_validation() {
        assert_eq!(printing_action(PD_RESULT_PRINT), Ok(true));
        assert_eq!(printing_action(PD_RESULT_APPLY), Ok(false));
        assert_eq!(printing_action(PD_RESULT_CANCEL), Ok(false));
        assert_eq!(
            printing_action(99),
            Err(PrinterDialogError::InvalidResponse)
        );
        assert!(matches!(
            choose_printer(None, 1, 1),
            Err(PrinterDialogError::InvalidOwner)
        ));
    }
}
