//! Narrow Windows boundary for virtual-desktop operations.
//!
//! The internal shell interfaces are undocumented and have changed shape several times. Each
//! layout below is tied to an established IID/build family; no method is invoked through a
//! layout whose identity cannot be established. Unknown Insider layouts fail as unsupported.

use super::{
    VirtualDesktopCapabilities, VirtualDesktopCapability, VirtualDesktopError,
    VirtualDesktopErrorKind, VirtualDesktopId, VirtualDesktopInfo, VirtualDesktopSnapshot,
};
use core::ffi::c_void;
use windows::Win32::Foundation::{ERROR_SUCCESS, HWND};
use windows::Win32::System::Com::{
    CLSCTX_ALL, CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize, IServiceProvider,
};
use windows::Win32::UI::Shell::Common::IObjectArray;
use windows::Win32::UI::Shell::{IVirtualDesktopManager, VirtualDesktopManager};
use windows::Win32::UI::WindowsAndMessaging::IsWindow;
use windows::core::{GUID, HRESULT, HSTRING, IUnknown, IUnknown_Vtbl, Interface, Type};

const IMMERSIVE_SHELL_CLSID: GUID = GUID::from_u128(0xc2f03a33_21f5_47fa_b4bb_156362a2f239);
const MANAGER_SERVICE_ID: GUID = GUID::from_u128(0xc5e0cdca_7b6e_41b2_9fc4_d93975cc467b);
type GetCount = unsafe extern "system" fn(*mut c_void, *mut i32) -> HRESULT;
type GetCountMonitor = unsafe extern "system" fn(*mut c_void, isize, *mut i32) -> HRESULT;
type GetDesktop = unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT;
type GetDesktopMonitor = unsafe extern "system" fn(*mut c_void, isize, *mut *mut c_void) -> HRESULT;
type ViewMethod = unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void) -> HRESULT;
type CanMove = unsafe extern "system" fn(*mut c_void, *mut c_void, *mut i32) -> HRESULT;
type Adjacent =
    unsafe extern "system" fn(*mut c_void, *mut c_void, i32, *mut *mut c_void) -> HRESULT;
type Switch = unsafe extern "system" fn(*mut c_void, *mut c_void) -> HRESULT;
type SwitchMonitor = unsafe extern "system" fn(*mut c_void, isize, *mut c_void) -> HRESULT;
type MoveDesktop = unsafe extern "system" fn(*mut c_void, *mut c_void, isize) -> HRESULT;
type Remove = unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void) -> HRESULT;
type Find = unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT;
// Padding for methods we never invoke; deliberately carries no guessed signature.
type UnknownSlot = usize;
type SetName = unsafe extern "system" fn(*mut c_void, *mut c_void, HSTRING) -> HRESULT;

macro_rules! interface {
    ($name:ident, $vtable:ident, $iid:expr) => {
        #[repr(transparent)]
        #[derive(Clone, PartialEq, Eq)]
        struct $name(IUnknown);
        unsafe impl Interface for $name {
            type Vtable = $vtable;
            const IID: GUID = GUID::from_u128($iid);
        }
    };
}
interface!(
    ManagerWin10,
    ManagerWin10Vtable,
    0xf31574d6_b682_4cdc_bd56_1827860abec6
);
interface!(
    ManagerWin10Names,
    ManagerWin10NamesVtable,
    0x0f3a72b0_4566_487e_9a33_4ed302f6d6ce
);
interface!(
    ManagerServer,
    ManagerServerVtable,
    0x094afe11_44f2_4ba0_976f_29a97e263ee0
);
interface!(
    ManagerWin11Monitor,
    ManagerWin11MonitorVtable,
    0xb2f925b9_5a0f_4d2e_9f4d_2b1507593c10
);
interface!(
    ManagerWin11Shifted,
    ManagerWin11ShiftedVtable,
    0xb2f925b9_5a0f_4d2e_9f4d_2b1507593c10
);
interface!(
    ManagerWin11Flat,
    ManagerWin11FlatVtable,
    0xa3175f2d_239c_4bd2_8aa0_eeba8b0b138e
);
interface!(
    ManagerWin11Current,
    ManagerWin11CurrentVtable,
    0x53f5ca0b_158f_4124_900c_057158060b27
);
interface!(
    ManagerWin11CurrentLegacy,
    ManagerWin11FlatVtable,
    0x53f5ca0b_158f_4124_900c_057158060b27
);

#[repr(C)]
#[allow(non_snake_case)]
struct ManagerWin10Vtable {
    base__: IUnknown_Vtbl,
    GetCount: GetCount,
    MoveViewToDesktop: ViewMethod,
    CanViewMoveDesktops: CanMove,
    GetCurrentDesktop: GetDesktop,
    GetDesktops: GetDesktop,
    GetAdjacentDesktop: Adjacent,
    SwitchDesktop: Switch,
    CreateDesktop: GetDesktop,
    RemoveDesktop: Remove,
    FindDesktop: Find,
    Unknown: UnknownSlot,
}
#[repr(C)]
#[allow(non_snake_case)]
struct ManagerWin10NamesVtable {
    base__: IUnknown_Vtbl,
    GetCount: GetCount,
    MoveViewToDesktop: ViewMethod,
    CanViewMoveDesktops: CanMove,
    GetCurrentDesktop: GetDesktop,
    GetDesktops: GetDesktop,
    GetAdjacentDesktop: Adjacent,
    SwitchDesktop: Switch,
    CreateDesktop: GetDesktop,
    RemoveDesktop: Remove,
    FindDesktop: Find,
    Unknown: UnknownSlot,
    SetDesktopName: SetName,
}
#[repr(C)]
#[allow(non_snake_case)]
struct ManagerServerVtable {
    base__: IUnknown_Vtbl,
    GetCount: GetCountMonitor,
    MoveViewToDesktop: ViewMethod,
    CanViewMoveDesktops: CanMove,
    GetCurrentDesktop: GetDesktopMonitor,
    GetDesktops: GetDesktopMonitor,
    GetAdjacentDesktop: Adjacent,
    SwitchDesktop: SwitchMonitor,
    CreateDesktop: GetDesktopMonitor,
    RemoveDesktop: Remove,
    FindDesktop: Find,
    Unknown: UnknownSlot,
    SetDesktopName: SetName,
}
#[repr(C)]
#[allow(non_snake_case)]
struct ManagerWin11MonitorVtable {
    base__: IUnknown_Vtbl,
    GetCount: GetCountMonitor,
    MoveViewToDesktop: ViewMethod,
    CanViewMoveDesktops: CanMove,
    GetCurrentDesktop: GetDesktopMonitor,
    GetDesktops: GetDesktopMonitor,
    GetAdjacentDesktop: Adjacent,
    SwitchDesktop: SwitchMonitor,
    CreateDesktop: GetDesktopMonitor,
    MoveDesktop: MoveDesktop,
    RemoveDesktop: Remove,
    FindDesktop: Find,
    Unknown: UnknownSlot,
    SetDesktopName: SetName,
}
#[repr(C)]
#[allow(non_snake_case)]
struct ManagerWin11ShiftedVtable {
    base__: IUnknown_Vtbl,
    GetCount: GetCountMonitor,
    MoveViewToDesktop: ViewMethod,
    CanViewMoveDesktops: CanMove,
    GetCurrentDesktop: GetDesktopMonitor,
    GetAllCurrentDesktops: UnknownSlot,
    GetDesktops: GetDesktopMonitor,
    GetAdjacentDesktop: Adjacent,
    SwitchDesktop: SwitchMonitor,
    CreateDesktop: GetDesktopMonitor,
    MoveDesktop: MoveDesktop,
    RemoveDesktop: Remove,
    FindDesktop: Find,
    Unknown: UnknownSlot,
    SetDesktopName: SetName,
}
#[repr(C)]
#[allow(non_snake_case)]
struct ManagerWin11FlatVtable {
    base__: IUnknown_Vtbl,
    GetCount: GetCount,
    MoveViewToDesktop: ViewMethod,
    CanViewMoveDesktops: CanMove,
    GetCurrentDesktop: GetDesktop,
    GetDesktops: GetDesktop,
    GetAdjacentDesktop: Adjacent,
    SwitchDesktop: Switch,
    CreateDesktop: GetDesktop,
    MoveDesktop: MoveDesktop,
    RemoveDesktop: Remove,
    FindDesktop: Find,
    Unknown: UnknownSlot,
    SetDesktopName: SetName,
}
#[repr(C)]
#[allow(non_snake_case)]
struct ManagerWin11CurrentVtable {
    base__: IUnknown_Vtbl,
    GetCount: GetCount,
    MoveViewToDesktop: ViewMethod,
    CanViewMoveDesktops: CanMove,
    GetCurrentDesktop: GetDesktop,
    GetDesktops: GetDesktop,
    GetAdjacentDesktop: Adjacent,
    SwitchDesktop: Switch,
    SwitchDesktopAndMoveForegroundView: ViewMethod,
    CreateDesktop: GetDesktop,
    MoveDesktop: MoveDesktop,
    RemoveDesktop: Remove,
    FindDesktop: Find,
    Unknown: UnknownSlot,
    SetDesktopName: SetName,
}
#[repr(C)]
#[allow(non_snake_case)]
struct DesktopIdentityVtable {
    base__: IUnknown_Vtbl,
    IsViewVisible: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut i32) -> HRESULT,
    GetID: unsafe extern "system" fn(*mut c_void, *mut GUID) -> HRESULT,
}

fn desktop_id(desktop: &IUnknown) -> windows::core::Result<GUID> {
    let mut id = GUID::zeroed();
    // GetID is the stable fourth method on desktop identities returned by these managers.
    let vtable =
        unsafe { &*(Interface::vtable(desktop) as *const _ as *const DesktopIdentityVtable) };
    unsafe { (vtable.GetID)(Interface::as_raw(desktop), &mut id) }.map(|| id)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShellLayout {
    Win10,
    Server,
    Win11Monitor,
    Win11Transitional,
    Win11Flat,
    Win11CurrentLegacy,
    Win11Current,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WindowsVersion {
    build: u32,
    revision: u32,
}
fn shell_layout(version: WindowsVersion) -> Option<ShellLayout> {
    match version.build {
        10_240
        | 10_586
        | 14_393
        | 15_063
        | 16_299
        | 17_134
        | 17_763
        | 18_362
        | 18_363
        | 19_041..=19_045 => Some(ShellLayout::Win10),
        20_348 => Some(ShellLayout::Server),
        22_000 => Some(ShellLayout::Win11Monitor),
        22_621 => Some(ShellLayout::Win11Transitional),
        22_631 => Some(ShellLayout::Win11Flat),
        26_100 if version.revision < 863 => Some(ShellLayout::Win11CurrentLegacy),
        26_100 | 26_200 => Some(ShellLayout::Win11Current),
        _ => None,
    }
}

enum Manager {
    Win10(ManagerWin10),
    Server(ManagerServer),
    Win11Monitor(ManagerWin11Monitor),
    Win11Shifted(ManagerWin11Shifted),
    Win11Flat(ManagerWin11Flat),
    Win11CurrentLegacy(ManagerWin11CurrentLegacy),
    Win11Current(ManagerWin11Current),
}

#[repr(transparent)]
struct ShellServiceProvider(IServiceProvider);
impl ShellServiceProvider {
    fn connect() -> windows::core::Result<Self> {
        unsafe {
            CoCreateInstance::<_, IServiceProvider>(
                &IMMERSIVE_SHELL_CLSID,
                None,
                CLSCTX_LOCAL_SERVER,
            )
        }
        .map(Self)
    }

    unsafe fn query<T: Interface>(&self) -> windows::core::Result<T> {
        unsafe { self.0.QueryService(&MANAGER_SERVICE_ID) }
    }
}

impl Manager {
    fn open(provider: &ShellServiceProvider, layout: ShellLayout) -> windows::core::Result<Self> {
        unsafe {
            Ok(match layout {
                ShellLayout::Win10 => Self::Win10(provider.query()?),
                ShellLayout::Server => Self::Server(provider.query()?),
                ShellLayout::Win11Monitor => Self::Win11Monitor(provider.query()?),
                ShellLayout::Win11Transitional => {
                    let flat: windows::core::Result<ManagerWin11Flat> = provider.query();
                    match flat {
                        Ok(manager) => Self::Win11Flat(manager),
                        Err(_) => Self::Win11Shifted(provider.query()?),
                    }
                }
                ShellLayout::Win11Flat => Self::Win11Flat(provider.query()?),
                ShellLayout::Win11CurrentLegacy => Self::Win11CurrentLegacy(provider.query()?),
                ShellLayout::Win11Current => Self::Win11Current(provider.query()?),
            })
        }
    }
    fn supports_rename(&self, version: WindowsVersion) -> bool {
        match self {
            Self::Win10(manager) => {
                version.build >= 19_041 && manager.cast::<ManagerWin10Names>().is_ok()
            }
            _ => true,
        }
    }
    fn current(&self) -> windows::core::Result<IUnknown> {
        let mut out = core::ptr::null_mut();
        let hr = unsafe {
            match self {
                Self::Win10(x) => {
                    (Interface::vtable(x).GetCurrentDesktop)(Interface::as_raw(x), &mut out)
                }
                Self::Server(x) => {
                    (Interface::vtable(x).GetCurrentDesktop)(Interface::as_raw(x), 0, &mut out)
                }
                Self::Win11Monitor(x) => {
                    (Interface::vtable(x).GetCurrentDesktop)(Interface::as_raw(x), 0, &mut out)
                }
                Self::Win11Shifted(x) => {
                    (Interface::vtable(x).GetCurrentDesktop)(Interface::as_raw(x), 0, &mut out)
                }
                Self::Win11Flat(x) => {
                    (Interface::vtable(x).GetCurrentDesktop)(Interface::as_raw(x), &mut out)
                }
                Self::Win11CurrentLegacy(x) => {
                    (Interface::vtable(x).GetCurrentDesktop)(Interface::as_raw(x), &mut out)
                }
                Self::Win11Current(x) => {
                    (Interface::vtable(x).GetCurrentDesktop)(Interface::as_raw(x), &mut out)
                }
            }
        };
        hr.and_then(|| unsafe { Type::from_abi(out) })
    }
    fn desktops(&self) -> windows::core::Result<IObjectArray> {
        let mut out = core::ptr::null_mut();
        let hr = unsafe {
            match self {
                Self::Win10(x) => {
                    (Interface::vtable(x).GetDesktops)(Interface::as_raw(x), &mut out)
                }
                Self::Server(x) => {
                    (Interface::vtable(x).GetDesktops)(Interface::as_raw(x), 0, &mut out)
                }
                Self::Win11Monitor(x) => {
                    (Interface::vtable(x).GetDesktops)(Interface::as_raw(x), 0, &mut out)
                }
                Self::Win11Shifted(x) => {
                    (Interface::vtable(x).GetDesktops)(Interface::as_raw(x), 0, &mut out)
                }
                Self::Win11Flat(x) => {
                    (Interface::vtable(x).GetDesktops)(Interface::as_raw(x), &mut out)
                }
                Self::Win11CurrentLegacy(x) => {
                    (Interface::vtable(x).GetDesktops)(Interface::as_raw(x), &mut out)
                }
                Self::Win11Current(x) => {
                    (Interface::vtable(x).GetDesktops)(Interface::as_raw(x), &mut out)
                }
            }
        };
        hr.and_then(|| unsafe { Type::from_abi(out) })
    }
    fn switch(&self, desktop: &IUnknown) -> windows::core::Result<()> {
        unsafe {
            match self {
                Self::Win10(x) => (Interface::vtable(x).SwitchDesktop)(
                    Interface::as_raw(x),
                    Interface::as_raw(desktop),
                ),
                Self::Server(x) => (Interface::vtable(x).SwitchDesktop)(
                    Interface::as_raw(x),
                    0,
                    Interface::as_raw(desktop),
                ),
                Self::Win11Monitor(x) => (Interface::vtable(x).SwitchDesktop)(
                    Interface::as_raw(x),
                    0,
                    Interface::as_raw(desktop),
                ),
                Self::Win11Shifted(x) => (Interface::vtable(x).SwitchDesktop)(
                    Interface::as_raw(x),
                    0,
                    Interface::as_raw(desktop),
                ),
                Self::Win11Flat(x) => (Interface::vtable(x).SwitchDesktop)(
                    Interface::as_raw(x),
                    Interface::as_raw(desktop),
                ),
                Self::Win11CurrentLegacy(x) => (Interface::vtable(x).SwitchDesktop)(
                    Interface::as_raw(x),
                    Interface::as_raw(desktop),
                ),
                Self::Win11Current(x) => (Interface::vtable(x).SwitchDesktop)(
                    Interface::as_raw(x),
                    Interface::as_raw(desktop),
                ),
            }
        }
        .ok()
    }
    fn create(&self) -> windows::core::Result<IUnknown> {
        let mut out = core::ptr::null_mut();
        let hr = unsafe {
            match self {
                Self::Win10(x) => {
                    (Interface::vtable(x).CreateDesktop)(Interface::as_raw(x), &mut out)
                }
                Self::Server(x) => {
                    (Interface::vtable(x).CreateDesktop)(Interface::as_raw(x), 0, &mut out)
                }
                Self::Win11Monitor(x) => {
                    (Interface::vtable(x).CreateDesktop)(Interface::as_raw(x), 0, &mut out)
                }
                Self::Win11Shifted(x) => {
                    (Interface::vtable(x).CreateDesktop)(Interface::as_raw(x), 0, &mut out)
                }
                Self::Win11Flat(x) => {
                    (Interface::vtable(x).CreateDesktop)(Interface::as_raw(x), &mut out)
                }
                Self::Win11CurrentLegacy(x) => {
                    (Interface::vtable(x).CreateDesktop)(Interface::as_raw(x), &mut out)
                }
                Self::Win11Current(x) => {
                    (Interface::vtable(x).CreateDesktop)(Interface::as_raw(x), &mut out)
                }
            }
        };
        hr.and_then(|| unsafe { Type::from_abi(out) })
    }
    fn remove(&self, desktop: &IUnknown, fallback: &IUnknown) -> windows::core::Result<()> {
        unsafe {
            match self {
                Self::Win10(x) => (Interface::vtable(x).RemoveDesktop)(
                    Interface::as_raw(x),
                    Interface::as_raw(desktop),
                    Interface::as_raw(fallback),
                ),
                Self::Server(x) => (Interface::vtable(x).RemoveDesktop)(
                    Interface::as_raw(x),
                    Interface::as_raw(desktop),
                    Interface::as_raw(fallback),
                ),
                Self::Win11Monitor(x) => (Interface::vtable(x).RemoveDesktop)(
                    Interface::as_raw(x),
                    Interface::as_raw(desktop),
                    Interface::as_raw(fallback),
                ),
                Self::Win11Shifted(x) => (Interface::vtable(x).RemoveDesktop)(
                    Interface::as_raw(x),
                    Interface::as_raw(desktop),
                    Interface::as_raw(fallback),
                ),
                Self::Win11Flat(x) => (Interface::vtable(x).RemoveDesktop)(
                    Interface::as_raw(x),
                    Interface::as_raw(desktop),
                    Interface::as_raw(fallback),
                ),
                Self::Win11CurrentLegacy(x) => (Interface::vtable(x).RemoveDesktop)(
                    Interface::as_raw(x),
                    Interface::as_raw(desktop),
                    Interface::as_raw(fallback),
                ),
                Self::Win11Current(x) => (Interface::vtable(x).RemoveDesktop)(
                    Interface::as_raw(x),
                    Interface::as_raw(desktop),
                    Interface::as_raw(fallback),
                ),
            }
        }
        .ok()
    }
    fn rename(&self, desktop: &IUnknown, name: &str) -> windows::core::Result<()> {
        let name = HSTRING::from(name);
        unsafe {
            match self {
                Self::Win10(x) => {
                    let named: ManagerWin10Names = x.cast()?;
                    (Interface::vtable(&named).SetDesktopName)(
                        Interface::as_raw(&named),
                        Interface::as_raw(desktop),
                        name,
                    )
                }
                Self::Server(x) => (Interface::vtable(x).SetDesktopName)(
                    Interface::as_raw(x),
                    Interface::as_raw(desktop),
                    name,
                ),
                Self::Win11Monitor(x) => (Interface::vtable(x).SetDesktopName)(
                    Interface::as_raw(x),
                    Interface::as_raw(desktop),
                    name,
                ),
                Self::Win11Shifted(x) => (Interface::vtable(x).SetDesktopName)(
                    Interface::as_raw(x),
                    Interface::as_raw(desktop),
                    name,
                ),
                Self::Win11Flat(x) => (Interface::vtable(x).SetDesktopName)(
                    Interface::as_raw(x),
                    Interface::as_raw(desktop),
                    name,
                ),
                Self::Win11CurrentLegacy(x) => (Interface::vtable(x).SetDesktopName)(
                    Interface::as_raw(x),
                    Interface::as_raw(desktop),
                    name,
                ),
                Self::Win11Current(x) => (Interface::vtable(x).SetDesktopName)(
                    Interface::as_raw(x),
                    Interface::as_raw(desktop),
                    name,
                ),
            }
        }
        .ok()
    }
}

struct ComApartment;
impl ComApartment {
    fn initialize(operation: &'static str) -> Result<Self, VirtualDesktopError> {
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() }
            .map_err(|e| native_error(operation, e))?;
        Ok(Self)
    }
}
impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}
struct InternalSession {
    manager: Manager,
    desktops: IObjectArray,
    version: WindowsVersion,
    _service_provider: ShellServiceProvider,
    _apartment: ComApartment,
}
impl InternalSession {
    fn open() -> Result<Self, VirtualDesktopError> {
        let version = windows_version().ok_or_else(|| unsupported_layout(None))?;
        let layout = shell_layout(version).ok_or_else(|| unsupported_layout(Some(version)))?;
        let apartment = ComApartment::initialize("initialize COM for virtual desktops")?;
        let service_provider = ShellServiceProvider::connect()
            .map_err(|e| native_error("open Immersive Shell service provider", e))?;
        let manager = Manager::open(&service_provider, layout).map_err(|e| {
            native_error("open virtual desktop manager", e)
                .context("windows_build", version.build.to_string())
                .context("windows_revision", version.revision.to_string())
        })?;
        let desktops = manager
            .desktops()
            .map_err(|e| native_error("enumerate virtual desktops", e))?;
        Ok(Self {
            manager,
            desktops,
            version,
            _service_provider: service_provider,
            _apartment: apartment,
        })
    }
    fn desktop(&self, index: u32) -> Result<IUnknown, VirtualDesktopError> {
        unsafe { self.desktops.GetAt(index) }.map_err(|e| {
            native_error("index virtual desktop", e).context("native_index", index.to_string())
        })
    }
    fn find(&self, id: &VirtualDesktopId) -> Result<IUnknown, VirtualDesktopError> {
        let count = unsafe { self.desktops.GetCount() }
            .map_err(|e| native_error("read virtual desktop count", e))?;
        for index in 0..count {
            let desktop = self.desktop(index)?;
            if id_from_guid(
                desktop_id(&desktop)
                    .map_err(|e| native_error("read virtual desktop identity", e))?,
            ) == *id
            {
                return Ok(desktop);
            }
        }
        Err(VirtualDesktopError::new(
            VirtualDesktopErrorKind::NotFound,
            "find virtual desktop",
            format!("Virtual desktop {id} no longer exists"),
        )
        .context("desktop_id", id.to_string()))
    }
}
fn unsupported_layout(version: Option<WindowsVersion>) -> VirtualDesktopError {
    let mut error = VirtualDesktopError::unsupported(
        "select virtual desktop shell layout",
        VirtualDesktopCapability::Enumeration,
    )
    .context(
        "reason",
        "No verified internal shell ABI is available for this Windows build",
    );
    if let Some(v) = version {
        error = error
            .context("windows_build", v.build.to_string())
            .context("windows_revision", v.revision.to_string());
    }
    error
}

pub(super) fn snapshot() -> Result<VirtualDesktopSnapshot, VirtualDesktopError> {
    let session = InternalSession::open()?;
    let current_id = id_from_guid(
        desktop_id(
            &session
                .manager
                .current()
                .map_err(|e| native_error("query current virtual desktop", e))?,
        )
        .map_err(|e| native_error("read current virtual desktop identity", e))?,
    );
    let count = unsafe { session.desktops.GetCount() }
        .map_err(|e| native_error("read virtual desktop count", e))?;
    let mut desktops = Vec::with_capacity(count as usize);
    for index in 0..count {
        let id = id_from_guid(desktop_id(&session.desktop(index)?).map_err(|e| {
            native_error("read virtual desktop identity", e)
                .context("native_index", index.to_string())
        })?);
        desktops.push(VirtualDesktopInfo {
            name: desktop_name(&id),
            is_current: id == current_id,
            id,
            index: index + 1,
        });
    }
    Ok(VirtualDesktopSnapshot {
        desktops,
        capabilities: VirtualDesktopCapabilities {
            enumeration: true,
            direct_switching: true,
            creation: true,
            closing: true,
            renaming: session.manager.supports_rename(session.version),
            window_membership: true,
            window_movement: true,
        },
    })
}
pub(super) fn switch_to(target: &VirtualDesktopId) -> Result<(), VirtualDesktopError> {
    let session = InternalSession::open()?;
    let desktop = session.find(target)?;
    session.manager.switch(&desktop).map_err(|e| {
        native_error("switch virtual desktop", e).context("desktop_id", target.to_string())
    })
}
pub(super) fn create() -> Result<VirtualDesktopInfo, VirtualDesktopError> {
    let session = InternalSession::open()?;
    let old_count = unsafe { session.desktops.GetCount() }
        .map_err(|e| native_error("read virtual desktop count", e))?;
    let desktop = session
        .manager
        .create()
        .map_err(|e| native_error("create virtual desktop", e))?;
    let id = id_from_guid(
        desktop_id(&desktop)
            .map_err(|e| native_error("read created virtual desktop identity", e))?,
    );
    Ok(VirtualDesktopInfo {
        id,
        index: old_count + 1,
        name: None,
        is_current: false,
    })
}
pub(super) fn close(
    current: &VirtualDesktopId,
    fallback: &VirtualDesktopId,
) -> Result<(), VirtualDesktopError> {
    let session = InternalSession::open()?;
    let current = session.find(current)?;
    let fallback = session.find(fallback)?;
    session
        .manager
        .remove(&current, &fallback)
        .map_err(|e| native_error("close virtual desktop", e))
}
pub(super) fn rename(target: &VirtualDesktopId, name: &str) -> Result<(), VirtualDesktopError> {
    let session = InternalSession::open()?;
    if !session.manager.supports_rename(session.version) {
        return Err(VirtualDesktopError::unsupported(
            "rename virtual desktop",
            VirtualDesktopCapability::Renaming,
        )
        .context("windows_build", session.version.build.to_string()));
    }
    let desktop = session.find(target)?;
    session.manager.rename(&desktop, name).map_err(|e| {
        native_error("rename virtual desktop", e).context("desktop_id", target.to_string())
    })
}

struct PublicSession {
    manager: IVirtualDesktopManager,
    _apartment: ComApartment,
}
impl PublicSession {
    fn open(operation: &'static str) -> Result<Self, VirtualDesktopError> {
        let apartment = ComApartment::initialize(operation)?;
        let manager = unsafe {
            CoCreateInstance::<_, IVirtualDesktopManager>(&VirtualDesktopManager, None, CLSCTX_ALL)
        }
        .map_err(|e| native_error(operation, e))?;
        Ok(Self {
            manager,
            _apartment: apartment,
        })
    }
}
pub(super) fn desktop_for_window(hwnd: HWND) -> Result<VirtualDesktopId, VirtualDesktopError> {
    validate_window(hwnd, "get desktop for window")?;
    let session = PublicSession::open("get desktop for window")?;
    unsafe { session.manager.GetWindowDesktopId(hwnd) }
        .map(id_from_guid)
        .map_err(|e| native_error("get desktop for window", e))
}
pub(super) fn is_window_on_current_desktop(hwnd: HWND) -> Result<bool, VirtualDesktopError> {
    validate_window(hwnd, "check window desktop")?;
    let session = PublicSession::open("check window desktop")?;
    unsafe { session.manager.IsWindowOnCurrentVirtualDesktop(hwnd) }
        .map(|x| x.as_bool())
        .map_err(|e| native_error("check window desktop", e))
}
pub(super) fn move_window_to_desktop(
    hwnd: HWND,
    desktop: &VirtualDesktopId,
) -> Result<(), VirtualDesktopError> {
    validate_window(hwnd, "move window to desktop")?;
    let session = PublicSession::open("move window to desktop")?;
    let target = guid_from_id(desktop);
    if unsafe { session.manager.GetWindowDesktopId(hwnd) }
        .map_err(|e| native_error("get desktop for window", e))?
        == target
    {
        return Ok(());
    }
    unsafe { session.manager.MoveWindowToDesktop(hwnd, &target) }.map_err(|e| {
        native_error("move window to desktop", e).context("desktop_id", desktop.to_string())
    })
}
fn validate_window(hwnd: HWND, operation: &'static str) -> Result<(), VirtualDesktopError> {
    if hwnd.0.is_null() || !unsafe { IsWindow(hwnd) }.as_bool() {
        return Err(VirtualDesktopError::new(
            VirtualDesktopErrorKind::InvalidWindow,
            operation,
            "Target window no longer exists",
        ));
    }
    Ok(())
}
pub(super) fn id_from_guid(guid: GUID) -> VirtualDesktopId {
    VirtualDesktopId::parse(&format_guid(&guid)).expect("formatted GUID is valid")
}
fn guid_from_id(id: &VirtualDesktopId) -> GUID {
    GUID::from(id.as_str())
}
fn format_guid(guid: &GUID) -> String {
    format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        guid.data1,
        guid.data2,
        guid.data3,
        guid.data4[0],
        guid.data4[1],
        guid.data4[2],
        guid.data4[3],
        guid.data4[4],
        guid.data4[5],
        guid.data4[6],
        guid.data4[7]
    )
}
fn desktop_name(id: &VirtualDesktopId) -> Option<String> {
    use windows::Win32::System::Registry::HKEY_CURRENT_USER;
    registry_string(
        HKEY_CURRENT_USER,
        &format!(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VirtualDesktops\\Desktops\\{{{}}}",
            id.as_str().to_ascii_uppercase()
        ),
        "Name",
    )
}
fn windows_version() -> Option<WindowsVersion> {
    use windows::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    let path = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion";
    Some(WindowsVersion {
        build: registry_string(HKEY_LOCAL_MACHINE, path, "CurrentBuildNumber")?
            .parse()
            .ok()?,
        revision: registry_dword(HKEY_LOCAL_MACHINE, path, "UBR").unwrap_or(0),
    })
}
fn registry_string(
    root: windows::Win32::System::Registry::HKEY,
    subkey: &str,
    value: &str,
) -> Option<String> {
    use windows::Win32::System::Registry::{REG_VALUE_TYPE, RRF_RT_REG_SZ, RegGetValueW};
    use windows::core::PCWSTR;
    let subkey = wide(subkey);
    let value = wide(value);
    let mut bytes = 0;
    let mut kind = REG_VALUE_TYPE::default();
    if unsafe {
        RegGetValueW(
            root,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            Some(&mut kind),
            None,
            Some(&mut bytes),
        )
    } != ERROR_SUCCESS
        || bytes < 2
    {
        return None;
    }
    let mut buffer = vec![0u16; (bytes as usize).div_ceil(2)];
    if unsafe {
        RegGetValueW(
            root,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            Some(&mut kind),
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    } != ERROR_SUCCESS
    {
        return None;
    }
    let end = buffer.iter().position(|x| *x == 0).unwrap_or(buffer.len());
    let text = String::from_utf16_lossy(&buffer[..end]);
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}
fn registry_dword(
    root: windows::Win32::System::Registry::HKEY,
    subkey: &str,
    value: &str,
) -> Option<u32> {
    use windows::Win32::System::Registry::{REG_VALUE_TYPE, RRF_RT_REG_DWORD, RegGetValueW};
    use windows::core::PCWSTR;
    let subkey = wide(subkey);
    let value = wide(value);
    let mut result = 0u32;
    let mut bytes = 4;
    let mut kind = REG_VALUE_TYPE::default();
    (unsafe {
        RegGetValueW(
            root,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_DWORD,
            Some(&mut kind),
            Some((&mut result as *mut u32).cast()),
            Some(&mut bytes),
        )
    } == ERROR_SUCCESS)
        .then_some(result)
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
fn native_error(operation: &'static str, error: windows::core::Error) -> VirtualDesktopError {
    VirtualDesktopError::new(
        VirtualDesktopErrorKind::Native,
        operation,
        format!("Failed to {operation}: {error}"),
    )
    .context("hresult", format!("0x{:08x}", error.code().0 as u32))
    .context("windows_error", error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guid_conversion_is_normalized_and_round_trips() {
        let guid = GUID::from_u128(0x550e8400_e29b_41d4_a716_446655440000);
        let id = id_from_guid(guid);
        assert_eq!(id.as_str(), "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(guid_from_id(&id), guid);
    }
    #[test]
    fn selects_only_verified_layout_families() {
        let v = |build, revision| WindowsVersion { build, revision };
        assert_eq!(shell_layout(v(19045, 0)), Some(ShellLayout::Win10));
        assert_eq!(shell_layout(v(20348, 0)), Some(ShellLayout::Server));
        assert_eq!(shell_layout(v(22000, 0)), Some(ShellLayout::Win11Monitor));
        assert_eq!(shell_layout(v(22500, 0)), None);
        assert_eq!(
            shell_layout(v(22621, 0)),
            Some(ShellLayout::Win11Transitional)
        );
        assert_eq!(shell_layout(v(22631, 0)), Some(ShellLayout::Win11Flat));
        assert_eq!(shell_layout(v(22650, 0)), None);
        assert_eq!(
            shell_layout(v(26100, 862)),
            Some(ShellLayout::Win11CurrentLegacy)
        );
        assert_eq!(shell_layout(v(26100, 863)), Some(ShellLayout::Win11Current));
        assert_eq!(shell_layout(v(26200, 0)), Some(ShellLayout::Win11Current));
        assert_eq!(shell_layout(v(26300, 0)), None);
    }

    #[test]
    fn internal_manager_construction_is_routed_through_immersive_shell_service() {
        assert_eq!(
            IMMERSIVE_SHELL_CLSID,
            GUID::from_u128(0xc2f03a33_21f5_47fa_b4bb_156362a2f239)
        );
        assert_eq!(
            MANAGER_SERVICE_ID,
            GUID::from_u128(0xc5e0cdca_7b6e_41b2_9fc4_d93975cc467b)
        );
        let source = include_str!("windows.rs");
        let manager_open = source
            .split("impl Manager {")
            .nth(1)
            .unwrap()
            .split("fn supports_rename")
            .next()
            .unwrap();
        assert!(manager_open.contains("provider.query()"));
        assert!(!manager_open.contains("CoCreateInstance"));
    }
}
