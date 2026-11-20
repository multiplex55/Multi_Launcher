//! Windows Recycle Bin adapter for an explicitly confirmed exact selection.
use super::{AdmissionError, EntrySnapshot, HeldParent, ScanCancellation, TargetOccupancy};
use anyhow::{Context, Result, ensure};
use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use windows::Win32::{
    Foundation::{E_ABORT, HWND},
    System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree},
    UI::{
        Shell::{
            FOF_NO_CONNECTED_ELEMENTS, FOF_NOERRORUI, FOFX_RECYCLEONDELETE, FileOperation,
            IFileOperation, IFileOperationProgressSink, IFileOperationProgressSink_Impl,
            IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
            TSF_DELETE_RECYCLE_IF_POSSIBLE,
        },
        WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow},
    },
};
use windows::core::{Error, HRESULT, PCWSTR, PWSTR, implement};

pub(crate) const MAX_RECYCLE_SELECTION: usize = 128;

#[derive(Debug)]
pub(crate) struct RecycleReport {
    pub recycled: usize,
    pub attempted: usize,
    pub stopped: bool,
    pub issues: Vec<(PathBuf, String)>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PostDelete {
    #[default]
    Missing,
    Recycled,
    Failed,
    Unconfirmed,
}

#[derive(Debug, Default)]
struct Evidence {
    pre_admitted: bool,
    refused: Option<&'static str>,
    post: PostDelete,
}

#[implement(IFileOperationProgressSink)]
struct RecycleSink {
    expected: EntrySnapshot,
    cancellation: ScanCancellation,
    evidence: Arc<Mutex<Evidence>>,
}

fn abort() -> Error {
    Error::from_hresult(E_ABORT)
}

struct ShellPath(PWSTR);
impl Drop for ShellPath {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            // SAFETY: GetDisplayName returns a COM task allocation.
            unsafe { CoTaskMemFree(Some(self.0.0.cast_const().cast())) };
        }
    }
}

fn shell_path(item: &IShellItem) -> windows::core::Result<PathBuf> {
    let path = ShellPath(unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }?);
    if path.0.0.is_null() {
        return Err(abort());
    }
    // SAFETY: The Shell returned a NUL-terminated task-allocated string.
    let units = unsafe { path.0.as_wide() };
    Ok(PathBuf::from(OsString::from_wide(units)))
}

impl IFileOperationProgressSink_Impl for RecycleSink_Impl {
    fn StartOperations(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn FinishOperations(&self, _: HRESULT) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreRenameItem(
        &self,
        _: u32,
        _: Option<&IShellItem>,
        _: &PCWSTR,
    ) -> windows::core::Result<()> {
        Err(abort())
    }
    fn PostRenameItem(
        &self,
        _: u32,
        _: Option<&IShellItem>,
        _: &PCWSTR,
        _: HRESULT,
        _: Option<&IShellItem>,
    ) -> windows::core::Result<()> {
        Err(abort())
    }
    fn PreMoveItem(
        &self,
        _: u32,
        _: Option<&IShellItem>,
        _: Option<&IShellItem>,
        _: &PCWSTR,
    ) -> windows::core::Result<()> {
        Err(abort())
    }
    fn PostMoveItem(
        &self,
        _: u32,
        _: Option<&IShellItem>,
        _: Option<&IShellItem>,
        _: &PCWSTR,
        _: HRESULT,
        _: Option<&IShellItem>,
    ) -> windows::core::Result<()> {
        Err(abort())
    }
    fn PreCopyItem(
        &self,
        _: u32,
        _: Option<&IShellItem>,
        _: Option<&IShellItem>,
        _: &PCWSTR,
    ) -> windows::core::Result<()> {
        Err(abort())
    }
    fn PostCopyItem(
        &self,
        _: u32,
        _: Option<&IShellItem>,
        _: Option<&IShellItem>,
        _: &PCWSTR,
        _: HRESULT,
        _: Option<&IShellItem>,
    ) -> windows::core::Result<()> {
        Err(abort())
    }
    fn PreNewItem(&self, _: u32, _: Option<&IShellItem>, _: &PCWSTR) -> windows::core::Result<()> {
        Err(abort())
    }
    fn PostNewItem(
        &self,
        _: u32,
        _: Option<&IShellItem>,
        _: &PCWSTR,
        _: &PCWSTR,
        _: u32,
        _: HRESULT,
        _: Option<&IShellItem>,
    ) -> windows::core::Result<()> {
        Err(abort())
    }
    fn UpdateProgress(&self, _: u32, _: u32) -> windows::core::Result<()> {
        Ok(())
    }
    fn ResetTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn PauseTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn ResumeTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }

    fn PreDeleteItem(&self, flags: u32, item: Option<&IShellItem>) -> windows::core::Result<()> {
        let mut evidence = self.evidence.lock().map_err(|_| abort())?;
        let reason = if evidence.pre_admitted {
            Some("Windows attempted to process the selection more than once.")
        } else if self.cancellation.is_cancelled() {
            Some("The Recycle Bin request was cancelled.")
        } else if flags & TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32 == 0 {
            Some("Windows did not offer a Recycle Bin operation for this item.")
        } else if item.and_then(|item| shell_path(item).ok()).as_deref()
            != Some(self.expected.path().as_path())
        {
            Some("Windows did not identify the exact selected item.")
        } else if super::super::recheck_source_metadata(&self.expected, &self.cancellation).is_err()
        {
            Some("The selected item changed before Windows could recycle it.")
        } else {
            None
        };
        if let Some(reason) = reason {
            evidence.refused = Some(reason);
            return Err(abort());
        }
        evidence.pre_admitted = true;
        Ok(())
    }

    fn PostDeleteItem(
        &self,
        _: u32,
        _: Option<&IShellItem>,
        status: HRESULT,
        recycled: Option<&IShellItem>,
    ) -> windows::core::Result<()> {
        let mut evidence = self.evidence.lock().map_err(|_| abort())?;
        evidence.post = if evidence.post != PostDelete::Missing || !evidence.pre_admitted {
            PostDelete::Unconfirmed
        } else if status.is_err() {
            PostDelete::Failed
        } else if recycled.is_some() {
            PostDelete::Recycled
        } else {
            PostDelete::Unconfirmed
        };
        Ok(())
    }
}

fn validate_owner(hwnd: HWND) -> Result<()> {
    let mut process = 0;
    // SAFETY: These queries do not operate on the window beyond identifying its owner.
    ensure!(
        unsafe { IsWindow(hwnd) }.as_bool()
            && unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process)) } != 0
            && process == std::process::id(),
        "The Downloads window is no longer available to own the Recycle Bin request."
    );
    Ok(())
}

fn recycle_one(
    expected: &EntrySnapshot,
    owner: HWND,
    cancellation: &ScanCancellation,
) -> Result<(bool, bool, Option<String>)> {
    let held = HeldParent::open(expected.parent().path(), cancellation)?;
    ensure!(
        &held.snapshot == expected.parent(),
        AdmissionError::StaleParent
    );
    ensure!(
        matches!(held.inspect_leaf(expected.leaf(), cancellation)?, TargetOccupancy::Occupied(observed) if observed == *expected),
        AdmissionError::StaleEntry
    );
    validate_owner(owner)?;
    let path: Vec<u16> = expected
        .path()
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let proof = Arc::new(Mutex::new(Evidence::default()));
    let sink: IFileOperationProgressSink = RecycleSink {
        expected: expected.clone(),
        cancellation: cancellation.clone(),
        evidence: proof.clone(),
    }
    .into();

    // The Shell accepts a path/PIDL. Keep the parent chain held and recheck the
    // exact child in PreDeleteItem. Holding the child without delete sharing
    // here would also veto the Shell's authorized Recycle Bin operation.
    let (performed, aborted) = unsafe {
        let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(path.as_ptr()), None)
            .context("Bind the selected item in the Windows Shell.")?;
        let operation: IFileOperation =
            CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER)
                .context("Open the Windows Recycle Bin operation.")?;
        operation.SetOwnerWindow(owner)?;
        operation
            .SetOperationFlags(FOFX_RECYCLEONDELETE | FOF_NOERRORUI | FOF_NO_CONNECTED_ELEMENTS)?;
        operation.DeleteItem(&item, &sink)?;
        let performed = operation.PerformOperations();
        let aborted = operation.GetAnyOperationsAborted();
        (performed, aborted)
    };
    drop(held);
    let evidence = proof
        .lock()
        .map_err(|_| anyhow::anyhow!("Windows Recycle Bin evidence is unavailable."))?;
    let stopped =
        aborted.as_ref().map_or(true, |value| value.as_bool()) || cancellation.is_cancelled();
    let recycled = evidence.post == PostDelete::Recycled;
    let note = if let Some(reason) = evidence.refused {
        Some(reason.into())
    } else if recycled {
        None
    } else if performed.is_err() || evidence.post == PostDelete::Failed {
        Some("Windows reported a Recycle Bin failure.".into())
    } else if stopped {
        Some("Windows cancelled the Recycle Bin request.".into())
    } else {
        Some("Windows did not return evidence that the item was recycled.".into())
    };
    Ok((recycled, stopped, note))
}

pub(crate) fn recycle_downloads(
    selection: &[EntrySnapshot],
    hwnd: usize,
    cancellation: &ScanCancellation,
) -> Result<RecycleReport> {
    ensure!(
        !selection.is_empty() && selection.len() <= MAX_RECYCLE_SELECTION,
        "Review up to {MAX_RECYCLE_SELECTION} Downloads items for the Recycle Bin at one time."
    );
    let owner = HWND(hwnd as *mut std::ffi::c_void);
    validate_owner(owner)?;
    let _apartment = crate::platform::downloads::StaApartment::initialize()?;
    let mut report = RecycleReport {
        recycled: 0,
        attempted: 0,
        stopped: false,
        issues: Vec::new(),
    };
    for entry in selection {
        if cancellation.is_cancelled() {
            report.stopped = true;
            break;
        }
        report.attempted += 1;
        match recycle_one(entry, owner, cancellation) {
            Ok((recycled, stopped, note)) => {
                report.recycled += usize::from(recycled);
                if let Some(note) = note {
                    report.issues.push((entry.path(), note));
                }
                if stopped {
                    report.stopped = true;
                    break;
                }
            }
            Err(error) => report.issues.push((entry.path(), format!("{error:#}"))),
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_transfer::inspect_source;
    use windows::Win32::{
        Foundation::HINSTANCE,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, HMENU, WINDOW_EX_STYLE, WS_OVERLAPPED,
        },
    };
    use windows::core::w;

    struct FixtureOwner(HWND);

    impl Drop for FixtureOwner {
        fn drop(&mut self) {
            // SAFETY: This fixture owns the hidden window on its own thread.
            let _ = unsafe { DestroyWindow(self.0) };
        }
    }

    #[test]
    #[ignore = "Moves one uniquely named disposable file to the real Windows Recycle Bin"]
    fn downloads_triage_native_recycle_keeps_unselected_connected_items() {
        let directory = tempfile::Builder::new()
            .prefix("multi-launcher-downloads-recycle-proof-")
            .tempdir()
            .unwrap();
        let stem = directory.path().file_name().unwrap().to_string_lossy();
        let input = directory.path().join(format!("{stem}.html"));
        let connected = directory.path().join(format!("{stem}_files"));
        let unselected = connected.join("unselected.txt");
        std::fs::create_dir(&connected).unwrap();
        std::fs::write(&input, b"disposable HTML test").unwrap();
        std::fs::write(&unselected, b"unselected fixture").unwrap();
        let cancellation = ScanCancellation::new();
        let expected = inspect_source(&input, &cancellation).unwrap();

        // SAFETY: Built-in STATIC creates an invisible window on the test thread.
        let owner = FixtureOwner(
            unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("STATIC"),
                    w!("Multi Launcher Recycle fixture"),
                    WS_OVERLAPPED,
                    0,
                    0,
                    0,
                    0,
                    HWND::default(),
                    HMENU::default(),
                    HINSTANCE::default(),
                    None,
                )
            }
            .unwrap(),
        );

        let cancelled = ScanCancellation::new();
        cancelled.cancel();
        let cancelled_report = recycle_downloads(
            std::slice::from_ref(&expected),
            owner.0.0 as usize,
            &cancelled,
        )
        .unwrap();
        assert!(cancelled_report.stopped);
        assert_eq!(cancelled_report.attempted, 0);
        assert!(input.exists());

        let report = recycle_downloads(&[expected], owner.0.0 as usize, &cancellation).unwrap();
        assert_eq!(report.recycled, 1, "{report:?}");
        assert!(report.issues.is_empty(), "{report:?}");
        assert!(!input.exists());
        assert_eq!(std::fs::read(unselected).unwrap(), b"unselected fixture");
    }
}
