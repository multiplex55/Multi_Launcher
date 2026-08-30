//! Windows virtual-desktop COM bindings and operations.
//!
//! The interfaces in this module are undocumented Windows shell interfaces. Keep their
//! vtable declarations and the unsafe calls that use them together so callers only need a
//! small, structured safe boundary.

use super::virtual_desktop_selection::{DesktopSelectionError, select_virtual_desktop_index};
use crate::mkmacro::{DiagnosticKind, ExecResult, ExecutionDiagnostic};
use windows::Win32::UI::Shell::Common::IObjectArray;
use windows::core::{GUID, HRESULT, HSTRING, IUnknown, IUnknown_Vtbl, Interface};

const VIRTUAL_DESKTOP_MANAGER_INTERNAL_CLSID: GUID =
    GUID::from_u128(0xc5e0cdca_7b6e_41b2_9fc4_d93975cc467b);

#[repr(transparent)]
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct IVirtualDesktop(pub IUnknown);

unsafe impl Interface for IVirtualDesktop {
    type Vtable = IVirtualDesktop_Vtbl;
    const IID: GUID = GUID::from_u128(0xff72ffdd_be7e_43fc_9c03_ad81681e88e4);
}

#[repr(C)]
#[allow(non_snake_case)]
pub(crate) struct IVirtualDesktop_Vtbl {
    pub base__: IUnknown_Vtbl,
    pub IsViewVisible: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut i32,
    ) -> HRESULT,
    pub GetID: unsafe extern "system" fn(*mut core::ffi::c_void, *mut GUID) -> HRESULT,
    pub Proc5: unsafe extern "system" fn(*mut core::ffi::c_void) -> HRESULT,
    pub GetName: unsafe extern "system" fn(*mut core::ffi::c_void, *mut HSTRING) -> HRESULT,
    pub GetWallpaperPath:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut HSTRING) -> HRESULT,
}

impl IVirtualDesktop {
    pub(crate) unsafe fn get_id(&self) -> windows::core::Result<GUID> {
        let mut result = GUID::zeroed();
        unsafe { (Interface::vtable(self).GetID)(Interface::as_raw(self), &mut result) }
            .map(|| result)
    }

    pub(crate) unsafe fn get_name(&self) -> windows::core::Result<HSTRING> {
        let mut result = HSTRING::new();
        unsafe { (Interface::vtable(self).GetName)(Interface::as_raw(self), &mut result) }
            .map(|| result)
    }
}

#[repr(transparent)]
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct IVirtualDesktopManagerInternal(pub IUnknown);

unsafe impl Interface for IVirtualDesktopManagerInternal {
    type Vtable = IVirtualDesktopManagerInternal_Vtbl;
    const IID: GUID = GUID::from_u128(0xf31574d6_b682_4cdc_bd56_1827860abec6);
}

#[repr(C)]
#[allow(non_snake_case)]
pub(crate) struct IVirtualDesktopManagerInternal_Vtbl {
    pub base__: IUnknown_Vtbl,
    pub GetCount: unsafe extern "system" fn(*mut core::ffi::c_void, isize, *mut i32) -> HRESULT,
    pub MoveViewToDesktop: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> HRESULT,
    pub CanViewMoveDesktops: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut i32,
    ) -> HRESULT,
    pub GetCurrentDesktop: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        isize,
        *mut *mut core::ffi::c_void,
    ) -> HRESULT,
    pub GetDesktops: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        isize,
        *mut *mut core::ffi::c_void,
    ) -> HRESULT,
    pub GetAdjacentDesktop: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        i32,
        *mut *mut core::ffi::c_void,
    ) -> HRESULT,
    pub SwitchDesktop:
        unsafe extern "system" fn(*mut core::ffi::c_void, isize, *mut core::ffi::c_void) -> HRESULT,
    pub CreateDesktop: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        isize,
        *mut *mut core::ffi::c_void,
    ) -> HRESULT,
    pub MoveDesktop: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        isize,
        i32,
    ) -> HRESULT,
    pub RemoveDesktop: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> HRESULT,
    pub FindDesktop: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const GUID,
        *mut *mut core::ffi::c_void,
    ) -> HRESULT,
    pub GetDesktopSwitchIncludeExcludeViews: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> HRESULT,
    pub SetDesktopName: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        HSTRING,
    ) -> HRESULT,
    pub SetDesktopWallpaper: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        HSTRING,
    ) -> HRESULT,
    pub UpdateWallpaperPathForAllDesktops:
        unsafe extern "system" fn(*mut core::ffi::c_void, HSTRING) -> HRESULT,
    pub CopyDesktopState: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> HRESULT,
    pub GetDesktopIsPerMonitor:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut i32) -> HRESULT,
    pub SetDesktopIsPerMonitor: unsafe extern "system" fn(*mut core::ffi::c_void, i32) -> HRESULT,
}

impl IVirtualDesktopManagerInternal {
    pub(crate) unsafe fn get_current_desktop(
        &self,
        hwnd_or_mon: isize,
    ) -> windows::core::Result<IVirtualDesktop> {
        let mut result = core::ptr::null_mut();
        unsafe {
            (Interface::vtable(self).GetCurrentDesktop)(
                Interface::as_raw(self),
                hwnd_or_mon,
                &mut result,
            )
        }
        .and_then(|| unsafe { windows::core::Type::from_abi(result) })
    }

    pub(crate) unsafe fn get_desktops(
        &self,
        hwnd_or_mon: isize,
    ) -> windows::core::Result<IObjectArray> {
        let mut result = core::ptr::null_mut();
        unsafe {
            (Interface::vtable(self).GetDesktops)(Interface::as_raw(self), hwnd_or_mon, &mut result)
        }
        .and_then(|| unsafe { windows::core::Type::from_abi(result) })
    }

    pub(crate) unsafe fn switch_desktop(
        &self,
        hwnd_or_mon: isize,
        desktop: &IVirtualDesktop,
    ) -> windows::core::Result<()> {
        unsafe {
            (Interface::vtable(self).SwitchDesktop)(
                Interface::as_raw(self),
                hwnd_or_mon,
                Interface::as_raw(desktop),
            )
        }
        .ok()
    }
}

/// Switch to a virtual desktop by its one-based position in the Windows desktop list.
pub fn switch_virtual_desktop_by_number(number: u32) -> ExecResult {
    if number == 0 {
        return Err(selection_failure(DesktopSelectionError::Zero));
    }

    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
    };

    let initialization = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() };
    if let Err(error) = initialization {
        return Err(com_failure("initialize COM apartment", error));
    }

    let result = unsafe { switch_virtual_desktop_after_initialization(number, CLSCTX_ALL) };
    unsafe { CoUninitialize() };
    result
}

unsafe fn switch_virtual_desktop_after_initialization(
    number: u32,
    class_context: windows::Win32::System::Com::CLSCTX,
) -> ExecResult {
    use windows::Win32::System::Com::CoCreateInstance;

    let manager = unsafe {
        CoCreateInstance::<_, IVirtualDesktopManagerInternal>(
            &VIRTUAL_DESKTOP_MANAGER_INTERNAL_CLSID,
            None,
            class_context,
        )
    }
    .map_err(|error| com_failure("activate virtual desktop manager", error))?;

    let desktops = unsafe { manager.get_desktops(0) }
        .map_err(|error| com_failure("enumerate virtual desktops", error))?;
    let desktop_count = unsafe { desktops.GetCount() }
        .map_err(|error| com_failure("read virtual desktop count", error))?;
    let index = select_virtual_desktop_index(number, desktop_count).map_err(selection_failure)?;

    let desktop: IVirtualDesktop = unsafe { desktops.GetAt(index) }
        .map_err(|error| com_failure("index virtual desktop", error))?;

    // GetCurrentDesktop and IVirtualDesktop::GetID are available on the same internal
    // interfaces. If either identity lookup is unavailable, SwitchDesktop remains the
    // authoritative operation and Windows handles an already-active target as a no-op.
    if let Ok(current) = unsafe { manager.get_current_desktop(0) }
        && let Ok(current_id) = unsafe { current.get_id() }
        && let Ok(target_id) = unsafe { desktop.get_id() }
        && current_id == target_id
    {
        return Ok(());
    }

    unsafe { manager.switch_desktop(0, &desktop) }
        .map_err(|error| com_failure("switch virtual desktop", error))
}

fn selection_failure(error: DesktopSelectionError) -> ExecutionDiagnostic {
    match error {
        DesktopSelectionError::Zero => ExecutionDiagnostic::new(
            DiagnosticKind::InvalidSelection,
            "Virtual desktop number must be at least 1",
        )
        .context("requested_desktop", "0")
        .context("backend_operation", "virtual desktop"),
        DesktopSelectionError::BeyondCount { requested, count } => ExecutionDiagnostic::new(
            DiagnosticKind::InvalidSelection,
            format!("Virtual desktop {requested} does not exist"),
        )
        .context("requested_desktop", requested.to_string())
        .context("desktop_count", count.to_string())
        .context("backend_operation", "virtual desktop"),
    }
}

fn com_failure(operation: &'static str, error: windows::core::Error) -> ExecutionDiagnostic {
    ExecutionDiagnostic::new(
        DiagnosticKind::ComFailure,
        format!("Failed to {operation}: {error}"),
    )
    .context("backend", "virtual desktop")
    .context("backend_operation", "virtual desktop")
    .context("operation", operation)
    .context("hresult", format!("0x{:08x}", error.code().0 as u32))
    .context("com_error", error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_rejects_zero_without_initializing_com() {
        let error = switch_virtual_desktop_by_number(0).unwrap_err();

        assert_eq!(error.kind, DiagnosticKind::InvalidSelection);
        assert_eq!(error.message, "Virtual desktop number must be at least 1");
        assert_eq!(
            error.context.get("requested_desktop").map(String::as_str),
            Some("0")
        );
        assert_eq!(
            error.context.get("backend_operation").map(String::as_str),
            Some("virtual desktop")
        );
    }

    #[test]
    fn missing_desktop_diagnostic_is_structured_without_creating_one() {
        let error = selection_failure(DesktopSelectionError::BeyondCount {
            requested: 3,
            count: 2,
        });

        assert_eq!(error.kind, DiagnosticKind::InvalidSelection);
        assert_eq!(error.message, "Virtual desktop 3 does not exist");
        assert_eq!(
            error.context.get("requested_desktop").map(String::as_str),
            Some("3")
        );
        assert_eq!(
            error.context.get("desktop_count").map(String::as_str),
            Some("2")
        );
        assert_eq!(
            error.context.get("backend_operation").map(String::as_str),
            Some("virtual desktop")
        );
    }
}
