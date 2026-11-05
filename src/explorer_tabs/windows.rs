use std::collections::HashMap;
use std::marker::PhantomData;
use std::path::Path;
use std::rc::Rc;

use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree, IServiceProvider};
use windows::Win32::System::Ole::IOleWindow;
use windows::Win32::System::SystemInformation::GetSystemWindowsDirectoryW;
use windows::Win32::UI::Shell::{
    IFolderView, ILFree, ILIsEqual, IPersistFolder2, IShellBrowser, IShellWindows, IWebBrowserApp,
    SHGetNameFromIDList, SHParseDisplayName, SID_STopLevelBrowser, SIGDN_FILESYSPATH, ShellWindows,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GA_ROOT, GetAncestor, GetWindowThreadProcessId, IsWindow,
};
use windows::core::{IUnknown, Interface, PCWSTR, PWSTR, VARIANT};

use crate::explorer_tabs::identity::{
    MAX_OBSERVED_WINDOWS, MAX_SHELL_VIEWS, PidlPathMatch, ShellBindingIdentity,
    ShellPathComparison, ShellViewCandidate, ShellViewIdentity, ShellViewSnapshot, SnapshotFailure,
    WindowIdentity,
};
use crate::explorer_tabs::operation::{FolderPath, MAX_FOLDER_PATH_UTF16_UNITS};

const MAX_SHELL_COUNT: i32 = MAX_SHELL_VIEWS as i32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellObservationError {
    ComInitialization(String),
    ShellUnavailable(String),
}

struct RetainedIdentity<T> {
    token: T,
    unknown: IUnknown,
    window: WindowIdentity,
}

struct RawShellView {
    window: WindowIdentity,
    binding: IUnknown,
    view: IUnknown,
    registration_index: u16,
    current_folder: Option<FolderPath>,
    path_comparison: Option<ShellPathComparison>,
}

struct ShellRowFact {
    index: u16,
    browser: IUnknown,
    hwnd: usize,
    window: WindowIdentity,
    process_path: String,
    shell_browser: Option<IUnknown>,
    view: Option<IUnknown>,
    folder_pidl: Option<PidlGuard>,
}

/// Read-only Shell observer. Construct, use, and drop this object on the same
/// worker thread. Canonical COM references are retained only there so new
/// observations can be compared safely; no COM pointer is returned as an ID.
pub struct WindowsShellObserver {
    shell: Option<IShellWindows>,
    previous_bindings: Vec<RetainedIdentity<ShellBindingIdentity>>,
    previous_views: Vec<RetainedIdentity<ShellViewIdentity>>,
    next_binding_token: u64,
    next_view_token: u64,
    apartment: Option<crate::mkmacro::uia::ComApartment>,
    _thread_bound: PhantomData<Rc<()>>,
}

impl WindowsShellObserver {
    pub fn connect() -> Result<Self, ShellObservationError> {
        // Keep the apartment guard in this worker invocation. Local COM
        // interfaces are declared after it and therefore drop first on an
        // early return.
        let apartment = crate::mkmacro::uia::initialize_com_apartment()
            .map_err(|error| ShellObservationError::ComInitialization(error.to_string()))?;
        let shell: IShellWindows = unsafe { CoCreateInstance(&ShellWindows, None, CLSCTX_ALL) }
            .map_err(|error| ShellObservationError::ShellUnavailable(error.to_string()))?;

        Ok(Self {
            shell: Some(shell),
            previous_bindings: Vec::new(),
            previous_views: Vec::new(),
            next_binding_token: 1,
            next_view_token: 1,
            apartment: Some(apartment),
            _thread_bound: PhantomData,
        })
    }

    /// Captures an unfiltered top-level HWND baseline and read-only active
    /// Shell views. `expected_folder`, when supplied, is resolved and compared
    /// to each active view by exact PIDL equality on this worker thread.
    ///
    /// A returned `ShellViewSnapshot` contains candidates only. It is not
    /// UIA tab-order evidence and cannot advance a folder-tab operation.
    pub fn observe(&mut self, expected_folder: Option<&FolderPath>) -> ShellViewSnapshot {
        if expected_folder.is_some_and(|path| !is_filesystem_directory(path)) {
            return ShellViewSnapshot::incomplete(SnapshotFailure::ExpectedFolderUnavailable);
        }
        let expected_pidl = match expected_folder {
            Some(path) => match parse_display_name(path) {
                Ok(pidl) => Some(pidl),
                Err(()) => {
                    return ShellViewSnapshot::incomplete(
                        SnapshotFailure::ExpectedFolderUnavailable,
                    );
                }
            },
            None => None,
        };

        let explorer_path = match system_explorer_path() {
            Some(path) => path,
            None => {
                return ShellViewSnapshot::incomplete(SnapshotFailure::ExplorerIdentityUnavailable);
            }
        };

        let windows = match enumerate_unfiltered_windows() {
            Ok(windows) => windows,
            Err(failure) => return ShellViewSnapshot::incomplete(failure),
        };
        let windows_by_handle: HashMap<_, _> = windows
            .iter()
            .map(|window| (window.hwnd(), *window))
            .collect();

        let Some(shell) = self.shell.as_ref() else {
            return ShellViewSnapshot::incomplete(SnapshotFailure::ShellUnavailable);
        };
        let count = match unsafe { shell.Count() } {
            Ok(count) if count >= 0 => count,
            Ok(_) => return ShellViewSnapshot::incomplete(SnapshotFailure::ShellCountInvalid),
            Err(_) => return ShellViewSnapshot::incomplete(SnapshotFailure::ShellCountFailed),
        };
        if count > MAX_SHELL_COUNT {
            return ShellViewSnapshot::incomplete(SnapshotFailure::ShellLimitExceeded);
        }

        let mut raw_views = Vec::with_capacity(count as usize);
        let mut row_facts = Vec::with_capacity(count as usize);
        for index in 0..count {
            let incomplete = |failure| ShellViewSnapshot::incomplete(failure);
            let dispatch = match unsafe { shell.Item(&VARIANT::from(index)) } {
                Ok(dispatch) => dispatch,
                Err(_) => {
                    return incomplete(SnapshotFailure::ShellItemUnavailable {
                        index: index as u16,
                    });
                }
            };
            let browser: IWebBrowserApp = match dispatch.cast() {
                Ok(browser) => browser,
                Err(_) => {
                    return incomplete(SnapshotFailure::ShellDispatchUnavailable {
                        index: index as u16,
                    });
                }
            };

            let hwnd_value = match unsafe { browser.HWND() }
                .ok()
                .and_then(|hwnd| usize::try_from(hwnd.0).ok())
                .filter(|hwnd| *hwnd != 0)
            {
                Some(hwnd) => hwnd,
                None => {
                    return incomplete(SnapshotFailure::WindowHandleUnavailable {
                        index: index as u16,
                    });
                }
            };
            let Some(window) = windows_by_handle.get(&hwnd_value).copied() else {
                return incomplete(SnapshotFailure::WindowIdentityUnavailable {
                    index: index as u16,
                });
            };
            let browser_unknown: IUnknown = match browser.cast() {
                Ok(unknown) => unknown,
                Err(_) => {
                    return incomplete(SnapshotFailure::ShellDispatchUnavailable {
                        index: index as u16,
                    });
                }
            };

            // A ShellWindows row is irrelevant only after its live process
            // identity has been resolved and shown not to be Explorer.
            let Some(process_path) = crate::multi_manager::win::window_process_path(hwnd_value)
            else {
                return incomplete(SnapshotFailure::WindowIdentityUnavailable {
                    index: index as u16,
                });
            };
            let Some(identity_after_process_query) = window_identity(hwnd_from_usize(hwnd_value))
            else {
                return incomplete(SnapshotFailure::WindowIdentityUnavailable {
                    index: index as u16,
                });
            };
            if identity_after_process_query != window {
                return incomplete(SnapshotFailure::WindowIdentityUnavailable {
                    index: index as u16,
                });
            }
            if !same_windows_path(&process_path, &explorer_path) {
                row_facts.push(ShellRowFact {
                    index: index as u16,
                    browser: browser_unknown,
                    hwnd: hwnd_value,
                    window,
                    process_path,
                    shell_browser: None,
                    view: None,
                    folder_pidl: None,
                });
                continue;
            }

            let service_provider: IServiceProvider = match browser.cast() {
                Ok(provider) => provider,
                Err(_) => {
                    return incomplete(SnapshotFailure::ShellBrowserUnavailable {
                        index: index as u16,
                    });
                }
            };
            let shell_browser: IShellBrowser =
                match unsafe { service_provider.QueryService(&SID_STopLevelBrowser) } {
                    Ok(browser) => browser,
                    Err(_) => {
                        return incomplete(SnapshotFailure::ShellBrowserUnavailable {
                            index: index as u16,
                        });
                    }
                };
            if !shell_browser_window_matches(&shell_browser, window) {
                return incomplete(SnapshotFailure::WindowIdentityUnavailable {
                    index: index as u16,
                });
            }
            let shell_browser_unknown: IUnknown = match shell_browser.cast() {
                Ok(unknown) => unknown,
                Err(_) => {
                    return incomplete(SnapshotFailure::ShellBrowserUnavailable {
                        index: index as u16,
                    });
                }
            };
            let shell_view = match unsafe { shell_browser.QueryActiveShellView() } {
                Ok(view) => view,
                Err(_) => {
                    return incomplete(SnapshotFailure::ActiveViewUnavailable {
                        index: index as u16,
                    });
                }
            };
            let view_window: IOleWindow = match shell_view.cast() {
                Ok(window) => window,
                Err(_) => {
                    return incomplete(SnapshotFailure::ActiveViewUnavailable {
                        index: index as u16,
                    });
                }
            };
            if !ole_window_matches(&view_window, window) {
                return incomplete(SnapshotFailure::WindowIdentityUnavailable {
                    index: index as u16,
                });
            }
            let folder_view: IFolderView = match shell_view.cast() {
                Ok(view) => view,
                Err(_) => {
                    return incomplete(SnapshotFailure::ActiveViewUnavailable {
                        index: index as u16,
                    });
                }
            };
            let persisted_folder: IPersistFolder2 = match unsafe { folder_view.GetFolder() } {
                Ok(folder) => folder,
                Err(_) => {
                    return incomplete(SnapshotFailure::FolderUnavailable {
                        index: index as u16,
                    });
                }
            };
            let folder_raw = match unsafe { persisted_folder.GetCurFolder() } {
                Ok(pidl) if !pidl.is_null() => pidl,
                _ => {
                    return incomplete(SnapshotFailure::FolderUnavailable {
                        index: index as u16,
                    });
                }
            };
            let folder_pidl = PidlGuard(folder_raw);
            let view_unknown: IUnknown = match shell_view.cast() {
                Ok(unknown) => unknown,
                Err(_) => {
                    return incomplete(SnapshotFailure::ActiveViewUnavailable {
                        index: index as u16,
                    });
                }
            };

            let current_folder = filesystem_path_from_pidl(folder_pidl.as_ptr())
                .and_then(|path| FolderPath::parse(&path).ok());
            let path_comparison = match (expected_folder, expected_pidl.as_ref()) {
                (Some(path), Some(expected)) => {
                    let exact =
                        unsafe { ILIsEqual(folder_pidl.as_ptr(), expected.as_ptr()) }.as_bool();
                    Some(ShellPathComparison::new(
                        path.clone(),
                        if exact {
                            PidlPathMatch::Exact
                        } else {
                            PidlPathMatch::Different
                        },
                    ))
                }
                (None, None) => None,
                (Some(_), None) => {
                    return incomplete(SnapshotFailure::ExpectedFolderUnavailable);
                }
                (None, Some(_)) => {
                    return incomplete(SnapshotFailure::ExpectedFolderUnavailable);
                }
            };

            row_facts.push(ShellRowFact {
                index: index as u16,
                browser: browser_unknown.clone(),
                hwnd: hwnd_value,
                window,
                process_path,
                shell_browser: Some(shell_browser_unknown),
                view: Some(view_unknown.clone()),
                folder_pidl: Some(folder_pidl),
            });
            raw_views.push(RawShellView {
                window,
                binding: browser_unknown,
                view: view_unknown,
                registration_index: index as u16,
                current_folder,
                path_comparison,
            });
        }

        if unsafe { shell.Count() }.ok() != Some(count) {
            return ShellViewSnapshot::incomplete(SnapshotFailure::ShellCountInvalid);
        }

        if let Err(failure) = recheck_shell_rows(shell, &row_facts, &explorer_path) {
            return ShellViewSnapshot::incomplete(failure);
        }
        if unsafe { shell.Count() }.ok() != Some(count) {
            return ShellViewSnapshot::incomplete(SnapshotFailure::ShellCountInvalid);
        }

        self.finish_snapshot(windows, raw_views)
    }

    fn finish_snapshot(
        &mut self,
        windows: Vec<WindowIdentity>,
        raw_views: Vec<RawShellView>,
    ) -> ShellViewSnapshot {
        let mut current_bindings = Vec::with_capacity(raw_views.len());
        let mut current_views = Vec::with_capacity(raw_views.len());
        let mut views = Vec::with_capacity(raw_views.len());

        for raw in raw_views {
            let shell_binding =
                match self.assign_binding(&raw.binding, raw.window, &mut current_bindings) {
                    Ok(identity) => identity,
                    Err(failure) => return ShellViewSnapshot::incomplete(failure),
                };
            let view_identity = match self.assign_view(&raw.view, raw.window, &mut current_views) {
                Ok(identity) => identity,
                Err(failure) => return ShellViewSnapshot::incomplete(failure),
            };
            views.push(ShellViewCandidate::new(
                raw.window,
                shell_binding,
                view_identity,
                raw.registration_index,
                raw.current_folder,
                raw.path_comparison,
            ));
        }

        match ShellViewSnapshot::complete(windows, views) {
            Ok(snapshot) => {
                self.previous_bindings = current_bindings;
                self.previous_views = current_views;
                snapshot
            }
            Err(failure) => ShellViewSnapshot::incomplete(failure),
        }
    }

    fn assign_binding(
        &mut self,
        unknown: &IUnknown,
        window: WindowIdentity,
        current: &mut Vec<RetainedIdentity<ShellBindingIdentity>>,
    ) -> Result<ShellBindingIdentity, SnapshotFailure> {
        if let Some(retained) = self
            .previous_bindings
            .iter()
            .find(|retained| same_com_identity(&retained.unknown, unknown))
        {
            if retained.window != window {
                return Err(SnapshotFailure::WindowIdentityUnavailable { index: 0 });
            }
            current.push(RetainedIdentity {
                token: retained.token,
                unknown: unknown.clone(),
                window,
            });
            return Ok(retained.token);
        }
        if let Some(retained) = current
            .iter()
            .find(|retained| same_com_identity(&retained.unknown, unknown))
        {
            return Ok(retained.token);
        }
        let token = self.next_binding_token;
        self.next_binding_token = token
            .checked_add(1)
            .ok_or(SnapshotFailure::ShellLimitExceeded)?;
        let identity = ShellBindingIdentity::from_worker_token(token)
            .ok_or(SnapshotFailure::ShellLimitExceeded)?;
        current.push(RetainedIdentity {
            token: identity,
            unknown: unknown.clone(),
            window,
        });
        Ok(identity)
    }

    fn assign_view(
        &mut self,
        unknown: &IUnknown,
        window: WindowIdentity,
        current: &mut Vec<RetainedIdentity<ShellViewIdentity>>,
    ) -> Result<ShellViewIdentity, SnapshotFailure> {
        if let Some(retained) = self
            .previous_views
            .iter()
            .find(|retained| same_com_identity(&retained.unknown, unknown))
        {
            if retained.window != window {
                return Err(SnapshotFailure::WindowIdentityUnavailable { index: 0 });
            }
            current.push(RetainedIdentity {
                token: retained.token,
                unknown: unknown.clone(),
                window,
            });
            return Ok(retained.token);
        }
        if let Some(retained) = current
            .iter()
            .find(|retained| same_com_identity(&retained.unknown, unknown))
        {
            return Ok(retained.token);
        }
        let token = self.next_view_token;
        self.next_view_token = token
            .checked_add(1)
            .ok_or(SnapshotFailure::ShellLimitExceeded)?;
        let identity = ShellViewIdentity::from_worker_token(token)
            .ok_or(SnapshotFailure::ShellLimitExceeded)?;
        current.push(RetainedIdentity {
            token: identity,
            unknown: unknown.clone(),
            window,
        });
        Ok(identity)
    }
}

impl Drop for WindowsShellObserver {
    fn drop(&mut self) {
        // Release all COM references before CoUninitialize, on the same
        // worker thread that initialized this apartment.
        self.previous_views.clear();
        self.previous_bindings.clear();
        self.shell.take();
        self.apartment.take();
    }
}

fn same_com_identity(left: &IUnknown, right: &IUnknown) -> bool {
    Interface::as_raw(left) == Interface::as_raw(right)
}

fn same_windows_path(left: &str, right: &str) -> bool {
    let normalize = |path: &str| path.replace('/', "\\").trim_end_matches('\\').to_owned();
    normalize(left).eq_ignore_ascii_case(&normalize(right))
}

fn recheck_shell_rows(
    shell: &IShellWindows,
    rows: &[ShellRowFact],
    explorer_path: &str,
) -> Result<(), SnapshotFailure> {
    for row in rows {
        let index = i32::from(row.index);
        let dispatch = unsafe { shell.Item(&VARIANT::from(index)) }
            .map_err(|_| SnapshotFailure::ShellItemUnavailable { index: row.index })?;
        let browser: IWebBrowserApp = dispatch
            .cast()
            .map_err(|_| SnapshotFailure::ShellDispatchUnavailable { index: row.index })?;
        let browser_unknown: IUnknown = browser
            .cast()
            .map_err(|_| SnapshotFailure::ShellDispatchUnavailable { index: row.index })?;
        let hwnd = unsafe { browser.HWND() }
            .ok()
            .and_then(|hwnd| usize::try_from(hwnd.0).ok())
            .filter(|hwnd| *hwnd != 0)
            .ok_or(SnapshotFailure::WindowHandleUnavailable { index: row.index })?;
        let live_window = window_identity(hwnd_from_usize(hwnd))
            .ok_or(SnapshotFailure::WindowIdentityUnavailable { index: row.index })?;
        let process_path = crate::multi_manager::win::window_process_path(hwnd)
            .ok_or(SnapshotFailure::WindowIdentityUnavailable { index: row.index })?;
        if hwnd != row.hwnd
            || live_window != row.window
            || !same_com_identity(&browser_unknown, &row.browser)
            || !same_windows_path(&process_path, &row.process_path)
        {
            return Err(SnapshotFailure::WindowIdentityUnavailable { index: row.index });
        }

        let is_explorer = same_windows_path(&process_path, explorer_path);
        if is_explorer != row.shell_browser.is_some() {
            return Err(SnapshotFailure::WindowIdentityUnavailable { index: row.index });
        }
        if !is_explorer {
            continue;
        }

        let service_provider: IServiceProvider = browser
            .cast()
            .map_err(|_| SnapshotFailure::ShellBrowserUnavailable { index: row.index })?;
        let shell_browser: IShellBrowser =
            unsafe { service_provider.QueryService(&SID_STopLevelBrowser) }
                .map_err(|_| SnapshotFailure::ShellBrowserUnavailable { index: row.index })?;
        if !shell_browser_window_matches(&shell_browser, row.window) {
            return Err(SnapshotFailure::WindowIdentityUnavailable { index: row.index });
        }
        let shell_browser_unknown: IUnknown = shell_browser
            .cast()
            .map_err(|_| SnapshotFailure::ShellBrowserUnavailable { index: row.index })?;
        if !row
            .shell_browser
            .as_ref()
            .is_some_and(|old| same_com_identity(old, &shell_browser_unknown))
        {
            return Err(SnapshotFailure::ShellBrowserUnavailable { index: row.index });
        }

        let shell_view = unsafe { shell_browser.QueryActiveShellView() }
            .map_err(|_| SnapshotFailure::ActiveViewUnavailable { index: row.index })?;
        let view_window: IOleWindow = shell_view
            .cast()
            .map_err(|_| SnapshotFailure::ActiveViewUnavailable { index: row.index })?;
        if !ole_window_matches(&view_window, row.window) {
            return Err(SnapshotFailure::WindowIdentityUnavailable { index: row.index });
        }
        let view_unknown: IUnknown = shell_view
            .cast()
            .map_err(|_| SnapshotFailure::ActiveViewUnavailable { index: row.index })?;
        if !row
            .view
            .as_ref()
            .is_some_and(|old| same_com_identity(old, &view_unknown))
        {
            return Err(SnapshotFailure::ActiveViewUnavailable { index: row.index });
        }

        let folder_view: IFolderView = shell_view
            .cast()
            .map_err(|_| SnapshotFailure::FolderUnavailable { index: row.index })?;
        let persisted_folder: IPersistFolder2 = unsafe { folder_view.GetFolder() }
            .map_err(|_| SnapshotFailure::FolderUnavailable { index: row.index })?;
        let raw_pidl = unsafe { persisted_folder.GetCurFolder() }
            .ok()
            .filter(|pidl| !pidl.is_null())
            .ok_or(SnapshotFailure::FolderUnavailable { index: row.index })?;
        let pidl = PidlGuard(raw_pidl);
        if !row
            .folder_pidl
            .as_ref()
            .is_some_and(|old| unsafe { ILIsEqual(old.as_ptr(), pidl.as_ptr()) }.as_bool())
        {
            return Err(SnapshotFailure::FolderUnavailable { index: row.index });
        }
    }
    Ok(())
}

fn system_explorer_path() -> Option<String> {
    let mut buffer = vec![0u16; 512];
    for _ in 0..2 {
        let length = unsafe { GetSystemWindowsDirectoryW(Some(&mut buffer)) } as usize;
        if length == 0 || length > MAX_FOLDER_PATH_UTF16_UNITS {
            return None;
        }
        if length < buffer.len() {
            let directory = String::from_utf16(&buffer[..length]).ok()?;
            return Some(format!(
                "{}\\explorer.exe",
                directory.trim_end_matches(['\\', '/'])
            ));
        }
        buffer.resize(length.saturating_add(1), 0);
    }
    None
}

fn is_filesystem_directory(path: &FolderPath) -> bool {
    std::fs::metadata(Path::new(path.as_str())).is_ok_and(|metadata| metadata.is_dir())
}

fn shell_browser_window_matches(shell_browser: &IShellBrowser, expected: WindowIdentity) -> bool {
    let Ok(ole_window) = shell_browser.cast::<IOleWindow>() else {
        return false;
    };
    ole_window_matches(&ole_window, expected)
}

fn ole_window_matches(ole_window: &IOleWindow, expected: WindowIdentity) -> bool {
    let Ok(hwnd) = (unsafe { ole_window.GetWindow() }) else {
        return false;
    };
    if hwnd.0.is_null() || !unsafe { IsWindow(hwnd) }.as_bool() {
        return false;
    }
    let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    if root.0.is_null() || root.0 as usize != expected.root_hwnd() {
        return false;
    }
    let mut process_id = 0;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    let mut root_process_id = 0;
    let root_thread_id = unsafe { GetWindowThreadProcessId(root, Some(&mut root_process_id)) };
    process_id == expected.process_id()
        && root_process_id == expected.root_process_id()
        && root_thread_id == expected.root_thread_id()
        && thread_id != 0
}

fn parse_display_name(path: &FolderPath) -> Result<PidlGuard, ()> {
    let wide: Vec<u16> = path.as_str().encode_utf16().chain(Some(0)).collect();
    let mut raw = std::ptr::null_mut();
    let result = unsafe { SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut raw, 0, None) };
    let pidl = PidlGuard::new(raw);
    match (result, pidl) {
        (Ok(()), Some(pidl)) => Ok(pidl),
        _ => Err(()),
    }
}

struct PidlGuard(*mut windows::Win32::UI::Shell::Common::ITEMIDLIST);

impl PidlGuard {
    fn new(raw: *mut windows::Win32::UI::Shell::Common::ITEMIDLIST) -> Option<Self> {
        (!raw.is_null()).then_some(Self(raw))
    }

    fn as_ptr(&self) -> *const windows::Win32::UI::Shell::Common::ITEMIDLIST {
        self.0.cast_const()
    }
}

impl Drop for PidlGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { ILFree(Some(self.as_ptr())) };
        }
    }
}

fn filesystem_path_from_pidl(
    pidl: *const windows::Win32::UI::Shell::Common::ITEMIDLIST,
) -> Option<String> {
    let wide = unsafe { SHGetNameFromIDList(pidl, SIGDN_FILESYSPATH) }.ok()?;
    let guard = TaskWideString(wide);
    let ptr = guard.0.0;
    if ptr.is_null() {
        return None;
    }
    let len = (0..=MAX_FOLDER_PATH_UTF16_UNITS).find(|index| unsafe { *ptr.add(*index) == 0 })?;
    String::from_utf16(unsafe { std::slice::from_raw_parts(ptr, len) }).ok()
}

struct TaskWideString(PWSTR);

impl Drop for TaskWideString {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            unsafe { CoTaskMemFree(Some(self.0.0.cast_const().cast())) };
        }
    }
}

fn enumerate_unfiltered_windows() -> Result<Vec<WindowIdentity>, SnapshotFailure> {
    struct Context {
        windows: Vec<WindowIdentity>,
        failure: Option<SnapshotFailure>,
    }

    unsafe extern "system" fn callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let context = unsafe { &mut *(lparam.0 as *mut Context) };
        if context.windows.len() >= MAX_OBSERVED_WINDOWS {
            context.failure = Some(SnapshotFailure::WindowLimitExceeded);
            return BOOL(0);
        }
        let index = context.windows.len() as u16;
        match window_identity(hwnd) {
            Some(identity) => context.windows.push(identity),
            None => {
                context.failure = Some(SnapshotFailure::WindowIdentityUnavailable { index });
                return BOOL(0);
            }
        }
        BOOL(1)
    }

    let mut context = Context {
        windows: Vec::new(),
        failure: None,
    };
    let result = unsafe {
        EnumWindows(
            Some(callback),
            LPARAM((&mut context as *mut Context) as isize),
        )
    };
    if let Some(failure) = context.failure {
        return Err(failure);
    }
    result.map_err(|_| SnapshotFailure::WindowEnumerationFailed)?;
    Ok(context.windows)
}

fn hwnd_from_usize(hwnd: usize) -> HWND {
    HWND(hwnd as *mut std::ffi::c_void)
}

fn window_identity(hwnd: HWND) -> Option<WindowIdentity> {
    if hwnd.0.is_null() || !unsafe { IsWindow(hwnd) }.as_bool() {
        return None;
    }
    let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    if root.0.is_null() || !unsafe { IsWindow(root) }.as_bool() {
        return None;
    }
    let mut process_id = 0;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    let mut root_process_id = 0;
    let root_thread_id = unsafe { GetWindowThreadProcessId(root, Some(&mut root_process_id)) };
    WindowIdentity::new(
        hwnd.0 as usize,
        root.0 as usize,
        process_id,
        thread_id,
        root_process_id,
        root_thread_id,
    )
}
