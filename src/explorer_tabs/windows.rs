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
const FOLDER_NOT_READY: &str = "selected tab folder does not match requested PIDL";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellObservationError {
    ComInitialization(String),
    ShellUnavailable(String),
}

#[derive(Debug, Clone, Default)]
struct ObservationTrace {
    phase: &'static str,
    stage: &'static str,
    index: Option<u16>,
    hwnd: Option<usize>,
    error_code: Option<i32>,
    related_hwnd: Option<usize>,
    related_root: Option<usize>,
    expected_root: Option<usize>,
}
impl ObservationTrace {
    fn set(
        &mut self,
        phase: &'static str,
        stage: &'static str,
        index: Option<u16>,
        hwnd: Option<usize>,
    ) {
        self.phase = phase;
        self.stage = stage;
        self.index = index;
        self.hwnd = hwnd;
        self.error_code = None;
        self.related_hwnd = None;
        self.related_root = None;
        self.expected_root = None;
    }
    fn error(&mut self, error: &windows::core::Error) {
        self.error_code = Some(error.code().0);
    }
    fn relation(&mut self, failure: WindowRelationFailure, expected: WindowIdentity) {
        self.stage = failure.stage;
        self.related_hwnd = failure.hwnd;
        self.related_root = failure.root;
        self.expected_root = Some(expected.root_hwnd());
        self.error_code = failure.error_code;
    }
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
    last_trace: ObservationTrace,
    apartment: Option<crate::mkmacro::uia::ComApartment>,
    _thread_bound: PhantomData<Rc<()>>,
}

impl WindowsShellObserver {
    pub fn connect() -> Result<Self, ShellObservationError> {
        // IOleWindow calls require an STA; callers must keep using the observer
        // on this thread and pump its messages while it is idle.
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .map_err(|error| ShellObservationError::ComInitialization(error.to_string()))?;
        Self::connect_with_apartment(crate::mkmacro::uia::ComApartment)
    }

    #[cfg(test)]
    fn connect_mta_for_diagnostic() -> Result<Self, ShellObservationError> {
        let apartment = crate::mkmacro::uia::initialize_com_apartment()
            .map_err(|error| ShellObservationError::ComInitialization(error.to_string()))?;
        Self::connect_with_apartment(apartment)
    }

    fn connect_with_apartment(
        apartment: crate::mkmacro::uia::ComApartment,
    ) -> Result<Self, ShellObservationError> {
        let shell: IShellWindows = unsafe { CoCreateInstance(&ShellWindows, None, CLSCTX_ALL) }
            .map_err(|error| ShellObservationError::ShellUnavailable(error.to_string()))?;

        Ok(Self {
            shell: Some(shell),
            previous_bindings: Vec::new(),
            previous_views: Vec::new(),
            next_binding_token: 1,
            next_view_token: 1,
            last_trace: ObservationTrace::default(),
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
        let mut trace = ObservationTrace::default();
        let snapshot = self.observe_inner(expected_folder, &mut trace);
        self.last_trace = trace;
        snapshot
    }

    fn observe_inner(
        &mut self,
        expected_folder: Option<&FolderPath>,
        trace: &mut ObservationTrace,
    ) -> ShellViewSnapshot {
        trace.set("initial", "expected-folder", None, None);
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

        trace.set("initial", "explorer-system-path", None, None);
        let explorer_path = match system_explorer_path() {
            Some(path) => path,
            None => {
                return ShellViewSnapshot::incomplete(SnapshotFailure::ExplorerIdentityUnavailable);
            }
        };

        trace.set("initial", "top-level-enumeration", None, None);
        let windows = match enumerate_unfiltered_windows(trace) {
            Ok(windows) => windows,
            Err(failure) => return ShellViewSnapshot::incomplete(failure),
        };
        let windows_by_handle: HashMap<_, _> = windows
            .iter()
            .map(|window| (window.hwnd(), *window))
            .collect();

        trace.set("initial", "shell-count", None, None);
        let Some(shell) = self.shell.as_ref() else {
            return ShellViewSnapshot::incomplete(SnapshotFailure::ShellUnavailable);
        };
        let count = match unsafe { shell.Count() } {
            Ok(count) if count >= 0 => count,
            Ok(_) => return ShellViewSnapshot::incomplete(SnapshotFailure::ShellCountInvalid),
            Err(error) => {
                trace.error(&error);
                return ShellViewSnapshot::incomplete(SnapshotFailure::ShellCountFailed);
            }
        };
        if count > MAX_SHELL_COUNT {
            return ShellViewSnapshot::incomplete(SnapshotFailure::ShellLimitExceeded);
        }

        let mut raw_views = Vec::with_capacity(count as usize);
        let mut row_facts = Vec::with_capacity(count as usize);
        for index in 0..count {
            let incomplete = |failure| ShellViewSnapshot::incomplete(failure);
            trace.set("initial", "shell-item", Some(index as u16), None);
            let dispatch = match unsafe { shell.Item(&VARIANT::from(index)) } {
                Ok(dispatch) => dispatch,
                Err(error) => {
                    trace.error(&error);
                    return incomplete(SnapshotFailure::ShellItemUnavailable {
                        index: index as u16,
                    });
                }
            };
            trace.set("initial", "shell-browser-cast", Some(index as u16), None);
            let browser: IWebBrowserApp = match dispatch.cast() {
                Ok(browser) => browser,
                Err(error) => {
                    trace.error(&error);
                    return incomplete(SnapshotFailure::ShellDispatchUnavailable {
                        index: index as u16,
                    });
                }
            };

            trace.set("initial", "shell-hwnd", Some(index as u16), None);
            let raw_hwnd = match unsafe { browser.HWND() } {
                Ok(hwnd) => hwnd,
                Err(error) => {
                    trace.error(&error);
                    return incomplete(SnapshotFailure::WindowHandleUnavailable {
                        index: index as u16,
                    });
                }
            };
            let hwnd_value = match usize::try_from(raw_hwnd.0).ok().filter(|hwnd| *hwnd != 0) {
                Some(hwnd) => hwnd,
                None => {
                    return incomplete(SnapshotFailure::WindowHandleUnavailable {
                        index: index as u16,
                    });
                }
            };
            trace.set(
                "initial",
                "shell-hwnd-membership",
                Some(index as u16),
                Some(hwnd_value),
            );
            let Some(window) = windows_by_handle.get(&hwnd_value).copied() else {
                return incomplete(SnapshotFailure::WindowIdentityUnavailable {
                    index: index as u16,
                });
            };
            let browser_unknown: IUnknown = match browser.cast() {
                Ok(unknown) => unknown,
                Err(error) => {
                    trace.error(&error);
                    return incomplete(SnapshotFailure::ShellDispatchUnavailable {
                        index: index as u16,
                    });
                }
            };

            // A ShellWindows row is irrelevant only after its live process
            // identity has been resolved and shown not to be Explorer.
            trace.set(
                "initial",
                "shell-process-path",
                Some(index as u16),
                Some(hwnd_value),
            );
            let Some(process_path) = crate::multi_manager::win::window_process_path(hwnd_value)
            else {
                return incomplete(SnapshotFailure::WindowIdentityUnavailable {
                    index: index as u16,
                });
            };
            trace.set(
                "initial",
                "shell-window-identity",
                Some(index as u16),
                Some(hwnd_value),
            );
            let identity_after_process_query =
                match window_identity_detailed(hwnd_from_usize(hwnd_value)) {
                    Ok(identity) => identity,
                    Err(stage) => {
                        trace.stage = stage;
                        return incomplete(SnapshotFailure::WindowIdentityUnavailable {
                            index: index as u16,
                        });
                    }
                };
            trace.set(
                "initial",
                "shell-window-changed",
                Some(index as u16),
                Some(hwnd_value),
            );
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

            trace.set(
                "initial",
                "shell-service-provider",
                Some(index as u16),
                Some(hwnd_value),
            );
            let service_provider: IServiceProvider = match browser.cast() {
                Ok(provider) => provider,
                Err(error) => {
                    trace.error(&error);
                    return incomplete(SnapshotFailure::ShellBrowserUnavailable {
                        index: index as u16,
                    });
                }
            };
            trace.set(
                "initial",
                "shell-browser-service",
                Some(index as u16),
                Some(hwnd_value),
            );
            let shell_browser: IShellBrowser =
                match unsafe { service_provider.QueryService(&SID_STopLevelBrowser) } {
                    Ok(browser) => browser,
                    Err(error) => {
                        trace.error(&error);
                        return incomplete(SnapshotFailure::ShellBrowserUnavailable {
                            index: index as u16,
                        });
                    }
                };
            trace.set(
                "initial",
                "shell-browser-window",
                Some(index as u16),
                Some(hwnd_value),
            );
            if let Err(failure) = shell_browser_window_match_detail(&shell_browser, window) {
                trace.relation(failure, window);
                return incomplete(SnapshotFailure::WindowIdentityUnavailable {
                    index: index as u16,
                });
            }
            let shell_browser_unknown: IUnknown = match shell_browser.cast() {
                Ok(unknown) => unknown,
                Err(error) => {
                    trace.error(&error);
                    return incomplete(SnapshotFailure::ShellBrowserUnavailable {
                        index: index as u16,
                    });
                }
            };
            trace.set(
                "initial",
                "active-view",
                Some(index as u16),
                Some(hwnd_value),
            );
            let shell_view = match unsafe { shell_browser.QueryActiveShellView() } {
                Ok(view) => view,
                Err(error) => {
                    trace.error(&error);
                    return incomplete(SnapshotFailure::ActiveViewUnavailable {
                        index: index as u16,
                    });
                }
            };
            // IShellView inherits IOleWindow; use its base vtable directly.
            // Some Shell objects reject a redundant IOleWindow QueryInterface.
            let view_window: &IOleWindow = &shell_view;
            trace.set(
                "initial",
                "active-view-window",
                Some(index as u16),
                Some(hwnd_value),
            );
            if let Err(failure) = ole_window_match_detail(view_window, window) {
                trace.relation(failure, window);
                return incomplete(SnapshotFailure::WindowIdentityUnavailable {
                    index: index as u16,
                });
            }
            trace.set(
                "initial",
                "view-folder",
                Some(index as u16),
                Some(hwnd_value),
            );
            let folder_view: IFolderView = match shell_view.cast() {
                Ok(view) => view,
                Err(error) => {
                    trace.error(&error);
                    return incomplete(SnapshotFailure::ActiveViewUnavailable {
                        index: index as u16,
                    });
                }
            };
            let persisted_folder: IPersistFolder2 = match unsafe { folder_view.GetFolder() } {
                Ok(folder) => folder,
                Err(error) => {
                    trace.error(&error);
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
                Err(error) => {
                    trace.error(&error);
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

        trace.set("recheck", "shell-count-before", None, None);
        if unsafe { shell.Count() }.ok() != Some(count) {
            return ShellViewSnapshot::incomplete(SnapshotFailure::ShellCountInvalid);
        }

        if let Err(failure) = recheck_shell_rows(shell, &row_facts, &explorer_path, trace) {
            return ShellViewSnapshot::incomplete(failure);
        }
        trace.set("recheck", "shell-count-after", None, None);
        if unsafe { shell.Count() }.ok() != Some(count) {
            return ShellViewSnapshot::incomplete(SnapshotFailure::ShellCountInvalid);
        }

        trace.set("final", "snapshot-validation", None, None);
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
    trace: &mut ObservationTrace,
) -> Result<(), SnapshotFailure> {
    for row in rows {
        trace.set("recheck", "shell-item", Some(row.index), Some(row.hwnd));
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
        trace.set(
            "recheck",
            "shell-window-identity",
            Some(row.index),
            Some(hwnd),
        );
        let live_window = window_identity_detailed(hwnd_from_usize(hwnd)).map_err(|stage| {
            trace.stage = stage;
            SnapshotFailure::WindowIdentityUnavailable { index: row.index }
        })?;
        trace.set("recheck", "shell-process-path", Some(row.index), Some(hwnd));
        let process_path = crate::multi_manager::win::window_process_path(hwnd)
            .ok_or(SnapshotFailure::WindowIdentityUnavailable { index: row.index })?;
        trace.set("recheck", "shell-row-changed", Some(row.index), Some(hwnd));
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

        trace.set(
            "recheck",
            "shell-browser-service",
            Some(row.index),
            Some(hwnd),
        );
        let service_provider: IServiceProvider = browser
            .cast()
            .map_err(|_| SnapshotFailure::ShellBrowserUnavailable { index: row.index })?;
        let shell_browser: IShellBrowser =
            unsafe { service_provider.QueryService(&SID_STopLevelBrowser) }
                .map_err(|_| SnapshotFailure::ShellBrowserUnavailable { index: row.index })?;
        trace.set(
            "recheck",
            "shell-browser-window",
            Some(row.index),
            Some(hwnd),
        );
        if let Err(failure) = shell_browser_window_match_detail(&shell_browser, row.window) {
            trace.relation(failure, row.window);
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

        trace.set("recheck", "active-view", Some(row.index), Some(hwnd));
        let shell_view = unsafe { shell_browser.QueryActiveShellView() }
            .map_err(|_| SnapshotFailure::ActiveViewUnavailable { index: row.index })?;
        let view_window: &IOleWindow = &shell_view;
        trace.set("recheck", "active-view-window", Some(row.index), Some(hwnd));
        if let Err(failure) = ole_window_match_detail(view_window, row.window) {
            trace.relation(failure, row.window);
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

        trace.set("recheck", "view-folder", Some(row.index), Some(hwnd));
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

#[derive(Debug)]
struct WindowRelationFailure {
    stage: &'static str,
    hwnd: Option<usize>,
    root: Option<usize>,
    error_code: Option<i32>,
}

fn shell_browser_window_match_detail(
    shell_browser: &IShellBrowser,
    expected: WindowIdentity,
) -> Result<(), WindowRelationFailure> {
    ole_window_match_detail(shell_browser, expected)
}

fn ole_window_match_detail(
    ole_window: &IOleWindow,
    expected: WindowIdentity,
) -> Result<(), WindowRelationFailure> {
    let hwnd = unsafe { ole_window.GetWindow() }.map_err(|error| WindowRelationFailure {
        stage: "ole-get-window",
        hwnd: None,
        root: None,
        error_code: Some(error.code().0),
    })?;
    let mut failure = WindowRelationFailure {
        stage: "ole-window-null",
        hwnd: Some(hwnd.0 as usize),
        root: None,
        error_code: None,
    };
    if hwnd.0.is_null() {
        return Err(failure);
    }
    failure.stage = "ole-window-invalid";
    if !unsafe { IsWindow(hwnd) }.as_bool() {
        return Err(failure);
    }
    let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    failure.root = Some(root.0 as usize);
    failure.stage = "ole-root-null";
    if root.0.is_null() {
        return Err(failure);
    }
    failure.stage = "ole-root-mismatch";
    if root.0 as usize != expected.root_hwnd() {
        return Err(failure);
    }
    let mut process_id = 0;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    let mut root_process_id = 0;
    let root_thread_id = unsafe { GetWindowThreadProcessId(root, Some(&mut root_process_id)) };
    failure.stage = "ole-window-process-mismatch";
    if process_id != expected.process_id() {
        return Err(failure);
    }
    failure.stage = "ole-root-process-mismatch";
    if root_process_id != expected.root_process_id() {
        return Err(failure);
    }
    failure.stage = "ole-root-thread-mismatch";
    if root_thread_id != expected.root_thread_id() {
        return Err(failure);
    }
    failure.stage = "ole-window-thread-zero";
    if thread_id == 0 {
        return Err(failure);
    }
    Ok(())
}

fn shell_browser_window_matches(shell_browser: &IShellBrowser, expected: WindowIdentity) -> bool {
    shell_browser_window_match_detail(shell_browser, expected).is_ok()
}

fn ole_window_matches(ole_window: &IOleWindow, expected: WindowIdentity) -> bool {
    ole_window_match_detail(ole_window, expected).is_ok()
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

fn enumerate_unfiltered_windows(
    trace: &mut ObservationTrace,
) -> Result<Vec<WindowIdentity>, SnapshotFailure> {
    struct Context {
        windows: Vec<WindowIdentity>,
        failure: Option<SnapshotFailure>,
        trace: ObservationTrace,
    }

    unsafe extern "system" fn callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let context = unsafe { &mut *(lparam.0 as *mut Context) };
        if context.windows.len() >= MAX_OBSERVED_WINDOWS {
            context.trace.set(
                "initial",
                "top-level-limit",
                Some(context.windows.len() as u16),
                Some(hwnd.0 as usize),
            );
            context.failure = Some(SnapshotFailure::WindowLimitExceeded);
            return BOOL(0);
        }
        let index = context.windows.len() as u16;
        match window_identity_detailed(hwnd) {
            Ok(identity) => context.windows.push(identity),
            Err(stage) => {
                context
                    .trace
                    .set("initial", stage, Some(index), Some(hwnd.0 as usize));
                context.failure = Some(SnapshotFailure::WindowIdentityUnavailable { index });
                return BOOL(0);
            }
        }
        BOOL(1)
    }

    let mut context = Context {
        windows: Vec::new(),
        failure: None,
        trace: ObservationTrace::default(),
    };
    let result = unsafe {
        EnumWindows(
            Some(callback),
            LPARAM((&mut context as *mut Context) as isize),
        )
    };
    if let Some(failure) = context.failure {
        *trace = context.trace;
        return Err(failure);
    }
    result.map_err(|error| {
        trace.error(&error);
        SnapshotFailure::WindowEnumerationFailed
    })?;
    Ok(context.windows)
}

fn hwnd_from_usize(hwnd: usize) -> HWND {
    HWND(hwnd as *mut std::ffi::c_void)
}

fn window_identity(hwnd: HWND) -> Option<WindowIdentity> {
    window_identity_detailed(hwnd).ok()
}

fn window_identity_detailed(hwnd: HWND) -> Result<WindowIdentity, &'static str> {
    if hwnd.0.is_null() {
        return Err("hwnd-null");
    }
    if !unsafe { IsWindow(hwnd) }.as_bool() {
        return Err("hwnd-not-window");
    }
    let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    if root.0.is_null() {
        return Err("ancestor-null");
    }
    if !unsafe { IsWindow(root) }.as_bool() {
        return Err("ancestor-not-window");
    }
    let mut process_id = 0;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    if thread_id == 0 || process_id == 0 {
        return Err("hwnd-thread-process-zero");
    }
    let mut root_process_id = 0;
    let root_thread_id = unsafe { GetWindowThreadProcessId(root, Some(&mut root_process_id)) };
    if root_thread_id == 0 || root_process_id == 0 {
        return Err("ancestor-thread-process-zero");
    }
    WindowIdentity::new(
        hwnd.0 as usize,
        root.0 as usize,
        process_id,
        thread_id,
        root_process_id,
        root_thread_id,
    )
    .ok_or("identity-invalid")
}

// The native port is worker-confined: UIA and Shell interfaces never leave the
// apartment, and only typed operation evidence crosses into the driver.
use crate::explorer_tabs::backend::{BackendDiagnostic, BootstrapState, ExplorerTabsPort};
use crate::explorer_tabs::identity::{CandidateCorrelation, correlate_shell_candidate};
use crate::explorer_tabs::native_uia::{ExplorerUia, UiaSnapshot};
use crate::explorer_tabs::operation::{
    BoundTargetEvidence, FolderIdentityEvidence, FolderIdentityResult, FolderRequestKind,
    FolderRequestStep, InitialTargetEvidence, OperationId, RequestResult, TargetAuthorityId,
    VerifiedFolderTab, VerifiedTabEvidence, VerifiedTabIdentity,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use windows::Win32::System::SystemInformation::OSVERSIONINFOW;
use windows::Win32::UI::Shell::{
    ASSOCF_NONE, ASSOCSTR_COMMAND, AssocQueryStringW, SBSP_ABSOLUTE, SBSP_NEWBROWSER,
    SBSP_SAMEBROWSER, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW,
};
use windows::Win32::UI::WindowsAndMessaging::{IsChild, SW_SHOWNORMAL};
use windows::core::w;

#[link(name = "ntdll")]
unsafe extern "system" {
    fn RtlGetVersion(info: *mut OSVERSIONINFOW) -> i32;
}

struct NativeTab {
    element: windows::Win32::UI::Accessibility::IUIAutomationElement,
    runtime: Vec<i32>,
    identity: VerifiedTabIdentity,
    folder: FolderPath,
    binding: ShellBindingIdentity,
}
#[derive(Clone)]
struct PendingNativeTab {
    element: windows::Win32::UI::Accessibility::IUIAutomationElement,
    runtime: Vec<i32>,
    identity: VerifiedTabIdentity,
}

struct BootstrapReceipt {
    operation_id: OperationId,
    private_path: FolderPath,
}

fn attributed_bootstrap(
    before: &ShellViewSnapshot,
    after: &ShellViewSnapshot,
    receipt: &BootstrapReceipt,
) -> Result<Option<BoundTargetEvidence>, String> {
    match correlate_shell_candidate(before, after, &receipt.private_path) {
        CandidateCorrelation::Unique(candidate) => {
            let window = candidate.window();
            if window.hwnd() != window.root_hwnd()
                || window.process_id() != window.root_process_id()
            {
                return Err("bootstrap is not a distinct Explorer root".into());
            }
            let authority =
                TargetAuthorityId::from_correlated_worker_token(1).ok_or("invalid authority")?;
            Ok(Some(BoundTargetEvidence::from_correlated_launch(
                receipt.operation_id,
                authority,
                window,
            )))
        }
        CandidateCorrelation::Ambiguous => Err("bootstrap target is ambiguous".into()),
        CandidateCorrelation::Incomplete(error) => {
            Err(format!("incomplete bootstrap observation: {error:?}"))
        }
        CandidateCorrelation::None => Ok(None),
    }
}

struct ShellState {
    observer: Option<WindowsShellObserver>,
    bindings: Vec<(IUnknown, ShellBindingIdentity)>,
    target: Option<BoundTargetEvidence>,
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
    test_apartment: Option<crate::mkmacro::uia::ComApartment>,
}
impl ShellState {
    fn check(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::Acquire) {
            Err("cancelled".into())
        } else if Instant::now() >= self.deadline {
            Err("deadline expired".into())
        } else {
            Ok(())
        }
    }

    fn observer(&mut self) -> Result<&mut WindowsShellObserver, String> {
        self.observer
            .as_mut()
            .ok_or_else(|| "Shell observer is unavailable".into())
    }

    fn target_window(&self) -> Result<WindowIdentity, String> {
        self.target
            .as_ref()
            .map(BoundTargetEvidence::window)
            .ok_or_else(|| "Explorer target is not attributed".into())
    }

    fn retained_browser(&self, binding: ShellBindingIdentity) -> Result<IShellBrowser, String> {
        let retained = self
            .observer
            .as_ref()
            .ok_or("missing observer")?
            .previous_bindings
            .iter()
            .find(|item| item.token == binding)
            .ok_or("Shell binding retired")?;
        let provider: IServiceProvider = self
            .call_native(|| retained.unknown.cast())?
            .map_err(|e| format!("Shell service provider: {e}"))?;
        self.call_native(|| unsafe { provider.QueryService(&SID_STopLevelBrowser) })?
            .map_err(|e| format!("Shell browser: {e}"))
    }

    fn call_native<T>(&self, call: impl FnOnce() -> T) -> Result<T, String> {
        self.check()?;
        let result = call();
        self.check()?;
        Ok(result)
    }

    fn current_folder_pidl(&self, folder: &IPersistFolder2) -> Result<PidlGuard, String> {
        self.check()?;
        let raw = unsafe { folder.GetCurFolder() }
            .map_err(|error| format!("Shell folder PIDL: {error}"))?;
        let owned = PidlGuard::new(raw).ok_or("Shell folder PIDL unavailable")?;
        self.check()?;
        Ok(owned)
    }

    /// Resolve one Shell view for the selected UIA pane on the Shell STA.
    fn selected_view(
        &mut self,
        pane: HWND,
        expected: Option<&FolderPath>,
    ) -> Result<(IShellBrowser, ShellBindingIdentity), String> {
        self.check()?;
        let target = self.target_window()?;
        let root = hwnd_from_usize(target.root_hwnd());
        if pane.0.is_null()
            || pane == root
            || self.call_native(|| unsafe { GetAncestor(pane, GA_ROOT) })? != root
            || !self
                .call_native(|| unsafe { IsChild(root, pane) })?
                .as_bool()
        {
            return Err("selected pane is not a distinct target content pane".into());
        }
        let shell = self
            .observer
            .as_ref()
            .and_then(|observer| observer.shell.as_ref())
            .cloned()
            .ok_or("Shell unavailable")?;
        let expected_pidl = match expected {
            Some(path) => Some(
                self.call_native(|| parse_display_name(path))?
                    .map_err(|_| "requested folder PIDL unavailable")?,
            ),
            None => None,
        };
        let count = self
            .call_native(|| unsafe { shell.Count() })?
            .map_err(|e| format!("Shell count: {e}"))?;
        if !(0..=MAX_SHELL_COUNT).contains(&count) {
            return Err("Shell enumeration limit".into());
        }
        let mut matches = Vec::new();
        for index in 0..count {
            self.check()?;
            let dispatch = self
                .call_native(|| unsafe { shell.Item(&VARIANT::from(index)) })?
                .map_err(|e| format!("Shell item: {e}"))?;
            let browser: IWebBrowserApp = self
                .call_native(|| dispatch.cast())?
                .map_err(|e| format!("Shell browser dispatch: {e}"))?;
            let hwnd = self
                .call_native(|| unsafe { browser.HWND() })?
                .map_err(|e| format!("Shell HWND: {e}"))?;
            if usize::try_from(hwnd.0).ok() != Some(target.hwnd()) {
                continue;
            }
            if self.call_native(|| window_identity(hwnd_from_usize(target.hwnd())))? != Some(target)
            {
                return Err("Explorer target identity changed".into());
            }
            let unknown: IUnknown = self
                .call_native(|| browser.cast())?
                .map_err(|e| format!("Shell identity: {e}"))?;
            let provider: IServiceProvider = self
                .call_native(|| browser.cast())?
                .map_err(|e| format!("Shell provider: {e}"))?;
            let shell_browser: IShellBrowser = self
                .call_native(|| unsafe { provider.QueryService(&SID_STopLevelBrowser) })?
                .map_err(|e| format!("Shell browser service: {e}"))?;
            let view = self
                .call_native(|| unsafe { shell_browser.QueryActiveShellView() })?
                .map_err(|e| format!("Shell active view: {e}"))?;
            let ole: &IOleWindow = &view;
            let view_hwnd = self
                .call_native(|| unsafe { ole.GetWindow() })?
                .map_err(|e| format!("Shell view HWND: {e}"))?;
            if !crate::explorer_tabs::native_uia::pane_view_relation(
                root.0 as usize,
                pane.0 as usize,
                view_hwnd.0 as usize,
                true,
                self.call_native(|| unsafe { IsChild(pane, view_hwnd) })?
                    .as_bool(),
            ) {
                continue;
            }
            if !self.call_native(|| shell_browser_window_matches(&shell_browser, target))?
                || !self.call_native(|| ole_window_matches(ole, target))?
            {
                return Err("Shell view ownership changed".into());
            }
            let folder_view: IFolderView = self
                .call_native(|| view.cast())?
                .map_err(|e| format!("Shell folder view: {e}"))?;
            let persisted: IPersistFolder2 = self
                .call_native(|| unsafe { folder_view.GetFolder() })?
                .map_err(|e| format!("Shell folder: {e}"))?;
            let current = self.current_folder_pidl(&persisted)?;
            if let Some(expected) = &expected_pidl
                && !self
                    .call_native(|| unsafe { ILIsEqual(current.as_ptr(), expected.as_ptr()) })?
                    .as_bool()
            {
                return Err(FOLDER_NOT_READY.into());
            }
            let view_unknown: IUnknown = self
                .call_native(|| view.cast())?
                .map_err(|e| format!("Shell view identity: {e}"))?;
            matches.push((
                index,
                unknown,
                shell_browser,
                view_unknown,
                view_hwnd,
                current,
            ));
        }
        if matches.len() != 1 {
            return Err("selected pane has no unique Shell view".into());
        }
        if self.call_native(|| unsafe { shell.Count() })?.ok() != Some(count) {
            return Err("Shell enumeration changed".into());
        }
        let (index, unknown, browser, view_identity, view_hwnd, folder) = matches.pop().unwrap();
        let again = self
            .call_native(|| unsafe { shell.Item(&VARIANT::from(index)) })?
            .map_err(|e| format!("Shell recheck: {e}"))?;
        let again: IWebBrowserApp = self
            .call_native(|| again.cast())?
            .map_err(|e| format!("Shell recheck dispatch: {e}"))?;
        let again_unknown: IUnknown = self
            .call_native(|| again.cast())?
            .map_err(|e| format!("Shell recheck identity: {e}"))?;
        if !same_com_identity(&unknown, &again_unknown)
            || self.call_native(|| window_identity(hwnd_from_usize(target.hwnd())))? != Some(target)
        {
            return Err("Shell binding changed".into());
        }
        let active = self
            .call_native(|| unsafe { browser.QueryActiveShellView() })?
            .map_err(|e| format!("Shell view recheck: {e}"))?;
        let active_unknown: IUnknown = self
            .call_native(|| active.cast())?
            .map_err(|e| format!("Shell view identity: {e}"))?;
        if !same_com_identity(&view_identity, &active_unknown) {
            return Err("Shell selected view changed".into());
        }
        let active_window: &IOleWindow = &active;
        let active_hwnd = self
            .call_native(|| unsafe { active_window.GetWindow() })?
            .map_err(|e| format!("Shell view HWND recheck: {e}"))?;
        if active_hwnd != view_hwnd
            || !crate::explorer_tabs::native_uia::pane_view_relation(
                root.0 as usize,
                pane.0 as usize,
                active_hwnd.0 as usize,
                true,
                self.call_native(|| unsafe { IsChild(pane, active_hwnd) })?
                    .as_bool(),
            )
        {
            return Err("Shell view pane relation changed".into());
        }
        let folder_view: IFolderView = self
            .call_native(|| active.cast())?
            .map_err(|e| format!("Shell folder recheck: {e}"))?;
        let persisted: IPersistFolder2 = self
            .call_native(|| unsafe { folder_view.GetFolder() })?
            .map_err(|e| format!("Shell folder recheck: {e}"))?;
        let current = self.current_folder_pidl(&persisted)?;
        if !self
            .call_native(|| unsafe { ILIsEqual(folder.as_ptr(), current.as_ptr()) })?
            .as_bool()
            || self.call_native(|| unsafe { shell.Count() })?.ok() != Some(count)
        {
            return Err("Shell folder or enumeration changed".into());
        }
        self.check()?;
        let binding = if let Some((_, identity)) = self
            .bindings
            .iter()
            .find(|(old, _)| same_com_identity(old, &unknown))
        {
            *identity
        } else {
            let observer = self.observer()?;
            let token = observer.next_binding_token;
            observer.next_binding_token = token
                .checked_add(1)
                .ok_or("Shell binding token exhausted")?;
            let identity = ShellBindingIdentity::from_worker_token(token)
                .ok_or("invalid Shell binding token")?;
            self.bindings.push((unknown, identity));
            identity
        };
        Ok((browser, binding))
    }
}

impl Drop for ShellState {
    fn drop(&mut self) {
        // Browser references must be released before the observer's apartment.
        self.bindings.clear();
        self.observer.take();
        self.test_apartment.take();
    }
}

// All Shell interfaces and PIDLs remain on this single operation-scoped STA.
type ShellJob = Box<dyn FnOnce(&mut ShellState) + Send>;
struct ShellStaOwner {
    sender: Option<std::sync::mpsc::SyncSender<ShellJob>>,
    thread_id: u32,
    thread: Option<std::thread::JoinHandle<()>>,
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
}
impl ShellStaOwner {
    fn start(
        operation_id: OperationId,
        cancelled: Arc<AtomicBool>,
        deadline: Instant,
    ) -> Result<Self, String> {
        Self::start_inner(operation_id, cancelled, deadline, true)
    }
    fn start_inner(
        _operation_id: OperationId,
        cancelled: Arc<AtomicBool>,
        deadline: Instant,
        connect_shell: bool,
    ) -> Result<Self, String> {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        use windows::Win32::System::Threading::GetCurrentThreadId;
        use windows::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, PM_REMOVE,
            PeekMessageW, QS_ALLINPUT, TranslateMessage,
        };
        let (sender, receiver) = std::sync::mpsc::sync_channel::<ShellJob>(1);
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let worker_id = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let worker_id_out = Arc::clone(&worker_id);
        let worker_cancelled = Arc::clone(&cancelled);
        let thread = std::thread::Builder::new()
            .name("explorer-shell-sta".into())
            .spawn(move || {
                let mut message = MSG::default();
                unsafe {
                    let _ = PeekMessageW(&mut message, None, 0, 0, PM_REMOVE);
                }
                let thread_id = unsafe { GetCurrentThreadId() };
                worker_id_out.store(thread_id, Ordering::Release);
                let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.ok();
                let mut local = ShellState {
                    observer: None,
                    bindings: Vec::new(),
                    target: None,
                    cancelled: worker_cancelled,
                    deadline,
                    test_apartment: None,
                };
                let initialized = initialized.map_err(|e| format!("Shell STA initialization: {e}"));
                let startup = initialized.and_then(|()| {
                    let apartment = crate::mkmacro::uia::ComApartment;
                    if connect_shell {
                        local.observer = Some(
                            WindowsShellObserver::connect_with_apartment(apartment)
                                .map_err(|e| format!("Shell observer: {e:?}"))?,
                        );
                    } else {
                        local.test_apartment = Some(apartment);
                    }
                    Ok(())
                });
                match startup {
                    Ok(()) => {
                        let _ = ready_tx.send(Ok(thread_id));
                    }
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
                }
                loop {
                    unsafe {
                        while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                            let _ = TranslateMessage(&message);
                            DispatchMessageW(&message);
                        }
                    }
                    loop {
                        match receiver.try_recv() {
                            Ok(job) => job(&mut local),
                            Err(std::sync::mpsc::TryRecvError::Empty) => break,
                            Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
                        }
                    }
                    unsafe {
                        MsgWaitForMultipleObjectsEx(None, 1000, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
                    }
                }
            })
            .map_err(|e| format!("Shell STA thread: {e}"))?;
        let thread_id = loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if cancelled.load(Ordering::Acquire) || remaining.is_zero() {
                drop(sender);
                let id = worker_id.load(Ordering::Acquire);
                if id != 0 {
                    use windows::Win32::Foundation::{LPARAM, WPARAM};
                    use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_APP};
                    let _ = unsafe { PostThreadMessageW(id, WM_APP + 41, WPARAM(0), LPARAM(0)) };
                }
                let _ = thread.join();
                return Err("Shell STA start cancelled or timed out".into());
            }
            match ready_rx.recv_timeout(remaining.min(Duration::from_millis(25))) {
                Ok(Ok(id)) => break id,
                Ok(Err(error)) => {
                    let _ = thread.join();
                    return Err(error);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => {
                    let _ = thread.join();
                    return Err("Shell STA failed to start".into());
                }
            }
        };
        Ok(Self {
            sender: Some(sender),
            thread_id,
            thread: Some(thread),
            cancelled,
            deadline,
        })
    }
    fn call<R: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut ShellState) -> Result<R, String> + Send + 'static,
    ) -> Result<R, String> {
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_APP};
        if self.cancelled.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        if Instant::now() >= self.deadline {
            return Err("deadline expired".into());
        }
        let (response_tx, response_rx) = std::sync::mpsc::sync_channel(1);
        let state = Arc::new(std::sync::atomic::AtomicU8::new(0)); // queued / running / retired
        let worker_state = Arc::clone(&state);
        let job = Box::new(move |local: &mut ShellState| {
            if worker_state
                .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                return;
            }
            let result = local.check().and_then(|()| operation(local));
            let _ = response_tx.send(result);
        });
        self.sender
            .as_ref()
            .ok_or("Shell STA stopped")?
            .try_send(job)
            .map_err(|_| "Shell STA is busy".to_owned())?;
        let wake_failed =
            unsafe { PostThreadMessageW(self.thread_id, WM_APP + 41, WPARAM(0), LPARAM(0)) }
                .is_err();
        if wake_failed {
            self.cancelled.store(true, Ordering::Release);
        }
        loop {
            let cancelled = self.cancelled.load(Ordering::Acquire);
            let remaining = self.deadline.saturating_duration_since(Instant::now());
            if (cancelled || remaining.is_zero())
                && state
                    .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            {
                return Err(if wake_failed {
                    "Shell STA wake failed"
                } else if cancelled {
                    "cancelled"
                } else {
                    "deadline expired"
                }
                .into());
            }
            // A running native call owns the attempt receipt. Wait for it so
            // the caller cannot report an attempt before its result is known.
            let wait = if remaining.is_zero() {
                Duration::from_millis(25)
            } else {
                remaining.min(Duration::from_millis(25))
            };
            match response_rx.recv_timeout(wait) {
                Ok(result) => {
                    if wake_failed {
                        return Err("Shell STA wake failed".into());
                    }
                    if self.cancelled.load(Ordering::Acquire) {
                        return Err("cancelled".into());
                    }
                    if Instant::now() >= self.deadline {
                        return Err("deadline expired".into());
                    }
                    return result;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => return Err("Shell STA stopped".into()),
            }
        }
    }
}
impl Drop for ShellStaOwner {
    fn drop(&mut self) {
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_APP};
        self.sender.take();
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_APP + 41, WPARAM(0), LPARAM(0)) };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(super) struct WindowsExplorerTabsPort {
    // UIA references drop before the operation thread's MTA guard.
    uia: Option<ExplorerUia>,
    tabs: Vec<NativeTab>,
    pending: Option<PendingNativeTab>,
    shell_sta: Option<ShellStaOwner>,
    mutation_attempted: Arc<AtomicBool>,
    bootstrap_attempted: Arc<AtomicBool>,
    mta_apartment: Option<crate::mkmacro::uia::ComApartment>,
    operation_id: OperationId,
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
    baseline: Option<ShellViewSnapshot>,
    target: Option<BoundTargetEvidence>,
    bootstrap: BootstrapState,
    bootstrap_path: Option<std::path::PathBuf>,
    requested_step_attempted: bool,
    last_error: Option<String>,
    opennew_available: bool,
}
impl WindowsExplorerTabsPort {
    pub(super) fn new(
        operation_id: OperationId,
        cancelled: Arc<AtomicBool>,
        deadline: Instant,
    ) -> Self {
        Self {
            uia: None,
            tabs: Vec::new(),
            pending: None,
            shell_sta: None,
            mutation_attempted: Arc::new(AtomicBool::new(false)),
            bootstrap_attempted: Arc::new(AtomicBool::new(false)),
            mta_apartment: None,
            operation_id,
            cancelled,
            deadline,
            baseline: None,
            target: None,
            bootstrap: BootstrapState::NotStarted,
            bootstrap_path: None,
            requested_step_attempted: false,
            last_error: None,
            opennew_available: false,
        }
    }
    fn check(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::Acquire) {
            Err("cancelled".into())
        } else if Instant::now() >= self.deadline {
            Err("deadline expired".into())
        } else {
            Ok(())
        }
    }
    fn diagnostic_for(&self, reason: impl Into<String>) -> BackendDiagnostic {
        BackendDiagnostic {
            reason: reason.into(),
            bootstrap: if self.bootstrap == BootstrapState::NotStarted
                && self.bootstrap_attempted.load(Ordering::Acquire)
            {
                BootstrapState::Attempted
            } else {
                self.bootstrap
            },
            owned_resource_remaining: self.bootstrap_path.is_some(),
            owned_resource_path: self.bootstrap_path.clone(),
            requested_step_attempted: self.requested_step_attempted
                || self.mutation_attempted.load(Ordering::Acquire),
        }
    }
    fn uia(&self) -> Result<&ExplorerUia, String> {
        self.uia
            .as_ref()
            .ok_or_else(|| "UI Automation is unavailable".into())
    }

    fn shell_call<R: Send + 'static>(
        &self,
        job: impl FnOnce(&mut ShellState) -> Result<R, String> + Send + 'static,
    ) -> Result<R, String> {
        self.shell_sta
            .as_ref()
            .ok_or("Shell STA unavailable")?
            .call(job)
    }
    fn shell_observe(
        &self,
        folder: Option<FolderPath>,
    ) -> Result<(ShellViewSnapshot, ObservationTrace), String> {
        self.shell_call(move |local| {
            let observer = local.observer()?;
            let snapshot = observer.observe(folder.as_ref());
            Ok((snapshot, observer.last_trace.clone()))
        })
    }
    fn shell_validate_folder(&self, folder: &FolderPath) -> Result<(), String> {
        let folder = folder.clone();
        self.shell_call(move |local| {
            let _pidl = local
                .call_native(|| parse_display_name(&folder))?
                .map_err(|_| "requested folder PIDL unavailable")?;
            Ok(())
        })
    }
    fn shell_selected_view(
        &self,
        pane: HWND,
        expected: Option<&FolderPath>,
    ) -> Result<ShellBindingIdentity, String> {
        let target = self
            .target
            .clone()
            .ok_or("Explorer target is not attributed")?;
        let pane = pane.0 as usize;
        let expected = expected.cloned();
        self.shell_call(move |local| {
            local.target = Some(target);
            local
                .selected_view(hwnd_from_usize(pane), expected.as_ref())
                .map(|(_, binding)| binding)
        })
    }
    fn shell_navigate(
        &self,
        pane: HWND,
        expected: Option<&FolderPath>,
        binding: ShellBindingIdentity,
        folder: &FolderPath,
    ) -> Result<(), String> {
        let target = self
            .target
            .clone()
            .ok_or("Explorer target is not attributed")?;
        let pane = pane.0 as usize;
        let expected = expected.cloned();
        let folder = folder.clone();
        let attempted = Arc::clone(&self.mutation_attempted);
        self.shell_call(move |local| {
            local.target = Some(target);
            let pidl = local
                .call_native(|| parse_display_name(&folder))?
                .map_err(|_| "requested folder PIDL unavailable")?;
            let (browser, current) =
                local.selected_view(hwnd_from_usize(pane), expected.as_ref())?;
            if current != binding {
                return Err("Shell binding changed before navigation".into());
            }
            local.check()?;
            attempted.store(true, Ordering::Release);
            let result =
                unsafe { browser.BrowseObject(pidl.as_ptr(), SBSP_ABSOLUTE | SBSP_SAMEBROWSER) };
            local.check()?;
            result.map_err(|e| format!("folder navigation: {e}"))
        })
    }
    fn target_window(&self) -> Result<WindowIdentity, String> {
        self.target
            .as_ref()
            .map(BoundTargetEvidence::window)
            .ok_or_else(|| "Explorer target is not attributed".into())
    }
    fn checked_snapshot(&self) -> Result<UiaSnapshot, String> {
        self.check()?;
        let target = self.target_window()?;
        if self.call_native(|| window_identity(hwnd_from_usize(target.hwnd())))? != Some(target) {
            return Err("Explorer target identity changed".into());
        }
        let snapshot = self.uia()?.snapshot(
            hwnd_from_usize(target.root_hwnd()),
            target.root_process_id(),
        )?;
        self.check()?;
        if self.call_native(|| window_identity(hwnd_from_usize(target.hwnd())))? != Some(target) {
            return Err("Explorer target identity changed".into());
        }
        Ok(snapshot)
    }
    fn check_existing_tabs(&self, snapshot: &UiaSnapshot, expected: usize) -> Result<(), String> {
        let expected_ids = self
            .tabs
            .iter()
            .map(|tab| tab.runtime.clone())
            .collect::<Vec<_>>();
        let observed_ids = snapshot
            .tabs
            .iter()
            .map(|tab| tab.runtime.clone())
            .collect::<Vec<_>>();
        let mut same = Vec::with_capacity(self.tabs.len());
        for (old, current) in self.tabs.iter().zip(&snapshot.tabs) {
            same.push(self.uia()?.same_element(&old.element, &current.element)?);
        }
        crate::explorer_tabs::evidence::validate_tab_prefix(
            &expected_ids,
            &observed_ids,
            &same,
            expected,
        )
        .map_err(str::to_owned)
    }
    fn wait_snapshot(&self, expected: usize) -> Result<UiaSnapshot, String> {
        loop {
            self.check()?;
            match self.checked_snapshot() {
                Ok(snapshot) if snapshot.tabs.len() == expected => return Ok(snapshot),
                Ok(snapshot) if snapshot.tabs.len() > expected => {
                    return Err("unexpected extra Explorer tabs".into());
                }
                Ok(_) => std::thread::sleep(Duration::from_millis(25)),
                Err(reason) => return Err(reason),
            }
        }
    }

    fn opennew_supported(&self) -> Result<bool, String> {
        let mut length = 0u32;
        let result = self.call_native(|| unsafe {
            AssocQueryStringW(
                ASSOCF_NONE,
                ASSOCSTR_COMMAND,
                w!("Directory"),
                w!("opennew"),
                PWSTR::null(),
                &mut length,
            )
        })?;
        if result.is_err() || !(2..=32768).contains(&length) {
            return Ok(false);
        }
        let mut buffer = vec![0u16; length as usize];
        let result = self.call_native(|| unsafe {
            AssocQueryStringW(
                ASSOCF_NONE,
                ASSOCSTR_COMMAND,
                w!("Directory"),
                w!("opennew"),
                PWSTR(buffer.as_mut_ptr()),
                &mut length,
            )
        })?;
        Ok(result.is_ok() && length > 1 && buffer.first().is_some_and(|value| *value != 0))
    }
    fn launch_bootstrap(&mut self, path: &FolderPath) -> Result<BootstrapReceipt, String> {
        self.check()?;
        let baseline = self.baseline.as_ref().ok_or("missing baseline")?;
        let browser = baseline.views().first().map(|view| view.shell_binding());
        if let Some(binding) = browser {
            let path = path.clone();
            let attempted = Arc::clone(&self.bootstrap_attempted);
            self.shell_call(move |local| {
                let shell = local.retained_browser(binding)?;
                let pidl = local
                    .call_native(|| parse_display_name(&path))?
                    .map_err(|_| "bootstrap PIDL unavailable")?;
                local.check()?;
                attempted.store(true, Ordering::Release);
                let result =
                    unsafe { shell.BrowseObject(pidl.as_ptr(), SBSP_ABSOLUTE | SBSP_NEWBROWSER) };
                local.check()?;
                result.map_err(|e| format!("new Explorer window request: {e}"))
            })?;
            self.bootstrap = BootstrapState::Attempted;
        } else {
            self.check()?;
            if !self.opennew_available || !self.opennew_supported()? {
                return Err("opennew Shell verb unavailable".into());
            }
            self.check()?;
            let wide: Vec<u16> = path.as_str().encode_utf16().chain(Some(0)).collect();
            let mut info = SHELLEXECUTEINFOW {
                cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
                fMask: SEE_MASK_FLAG_NO_UI | SEE_MASK_NOASYNC,
                lpVerb: w!("opennew"),
                lpFile: PCWSTR(wide.as_ptr()),
                nShow: SW_SHOWNORMAL.0,
                ..Default::default()
            };
            self.check()?;
            self.bootstrap = BootstrapState::Attempted;
            let result = unsafe { ShellExecuteExW(&mut info) };
            self.check()?;
            result.map_err(|e| format!("opennew Shell request: {e}"))?;
        }
        Ok(BootstrapReceipt {
            operation_id: self.operation_id,
            private_path: path.clone(),
        })
    }
    fn await_bootstrap(&mut self, receipt: &BootstrapReceipt) -> Result<(), String> {
        loop {
            self.check()?;
            let (after, _) = self.shell_observe(Some(receipt.private_path.clone()))?;
            self.check()?;
            let before = self.baseline.as_ref().ok_or("missing baseline")?;
            match attributed_bootstrap(before, &after, receipt)? {
                Some(target) => {
                    self.target = Some(target);
                    self.bootstrap = BootstrapState::Attributed;
                    return Ok(());
                }
                None => std::thread::sleep(Duration::from_millis(25)),
            }
        }
    }
}

impl WindowsExplorerTabsPort {
    fn call_native<T>(&self, call: impl FnOnce() -> T) -> Result<T, String> {
        self.check()?;
        let result = call();
        self.check()?;
        Ok(result)
    }
}

impl WindowsExplorerTabsPort {
    fn prepare_initial(&mut self, step: &FolderRequestStep) -> Result<(), String> {
        self.check()?;
        if !is_filesystem_directory(step.folder()) {
            return Err("requested folder is unavailable or not a directory".into());
        }
        self.check()?;
        self.shell_validate_folder(step.folder())?;
        let (baseline, _) = self.shell_observe(None)?;
        self.check()?;
        if !baseline.is_complete() {
            return Err(format!(
                "incomplete pre-launch Shell baseline: {:?}",
                baseline.failure()
            ));
        }
        if baseline.views().is_empty() && !self.opennew_available {
            return Err("opennew Shell verb unavailable".into());
        }
        self.baseline = Some(baseline);
        self.check()?;
        let temp = tempfile::Builder::new()
            .prefix("multi-launcher-explorer-")
            .tempdir()
            .map_err(|e| format!("bootstrap directory: {e}"))?;
        let path = temp.keep();
        self.bootstrap_path = Some(path.clone());
        self.check()?;
        let bootstrap = FolderPath::parse(path.to_str().ok_or("bootstrap path is not Unicode")?)
            .map_err(|e| format!("bootstrap path: {e:?}"))?;
        self.check()?;
        let (refreshed, _) = self.shell_observe(None)?;
        self.check()?;
        if self.baseline.as_ref() != Some(&refreshed) {
            return Err("Shell baseline changed before bootstrap request".into());
        }
        self.baseline = Some(refreshed);
        let receipt = self.launch_bootstrap(&bootstrap)?;
        self.await_bootstrap(&receipt)?;
        let snapshot = self.wait_snapshot(1)?;
        if snapshot.selected_index != 0 {
            return Err("bootstrap tab selection is ambiguous".into());
        }
        let binding = self.shell_selected_view(snapshot.selected_pane, Some(&bootstrap))?;
        let rechecked = self.checked_snapshot()?;
        if rechecked.tabs.len() != 1
            || rechecked.selected_index != 0
            || rechecked.selected_pane != snapshot.selected_pane
            || snapshot.tabs[0].runtime != rechecked.tabs[0].runtime
            || !self
                .uia()?
                .same_element(&snapshot.tabs[0].element, &rechecked.tabs[0].element)?
        {
            return Err("bootstrap tab changed before navigation".into());
        }
        let rechecked_binding =
            self.shell_selected_view(rechecked.selected_pane, Some(&bootstrap))?;
        if rechecked_binding != binding {
            return Err("bootstrap Shell binding changed before navigation".into());
        }
        self.shell_navigate(
            rechecked.selected_pane,
            Some(&bootstrap),
            binding,
            step.folder(),
        )?;
        self.pending = Some(PendingNativeTab {
            element: snapshot.tabs[0].element.clone(),
            runtime: snapshot.tabs[0].runtime.clone(),
            identity: VerifiedTabIdentity::from_worker_token(1).ok_or("invalid tab token")?,
        });
        Ok(())
    }
    fn prepare_append(&mut self, step: &FolderRequestStep) -> Result<(), String> {
        self.check()?;
        if !is_filesystem_directory(step.folder()) {
            return Err("requested folder is unavailable or not a directory".into());
        }
        self.check()?;
        self.shell_validate_folder(step.folder())?;
        let before = self.checked_snapshot()?;
        self.check_existing_tabs(&before, self.tabs.len())?;
        if self.tabs.is_empty() || before.selected_index + 1 != self.tabs.len() {
            return Err("Explorer selected tab changed".into());
        }
        let previous_folder = self.tabs.last().ok_or("missing prior tab")?.folder.clone();
        let binding = self.shell_selected_view(before.selected_pane, Some(&previous_folder))?;
        if self.tabs.last().is_none_or(|tab| tab.binding != binding) {
            return Err("selected Shell binding changed".into());
        }
        let rechecked = self.checked_snapshot()?;
        self.check_existing_tabs(&rechecked, self.tabs.len())?;
        if rechecked.selected_index != before.selected_index
            || rechecked.selected_pane != before.selected_pane
        {
            return Err("Explorer selection changed before append".into());
        }
        let rechecked_binding =
            self.shell_selected_view(rechecked.selected_pane, Some(&previous_folder))?;
        if rechecked_binding != binding {
            return Err("selected Shell binding changed before append".into());
        }
        let ready = self.checked_snapshot()?;
        self.check_existing_tabs(&ready, self.tabs.len())?;
        if ready.selected_index != rechecked.selected_index
            || ready.selected_pane != rechecked.selected_pane
            || !self
                .uia()?
                .same_element(&ready.new_tab, &rechecked.new_tab)?
        {
            return Err("New Tab control or selected pane changed before append".into());
        }
        let uia = self.uia.as_ref().ok_or("UI Automation is unavailable")?;
        let attempted = &mut self.requested_step_attempted;
        uia.invoke_new_tab(&ready.new_tab, || *attempted = true)?;
        let after = self.wait_snapshot(self.tabs.len() + 1)?;
        self.check_existing_tabs(&after, self.tabs.len() + 1)?;
        if after.selected_index != self.tabs.len() {
            return Err("new tab was not appended and selected".into());
        }
        let new = &after.tabs[after.selected_index];
        if self.tabs.iter().any(|tab| tab.runtime == new.runtime) {
            return Err("reused tab runtime ID".into());
        }
        for old in &self.tabs {
            if self.uia()?.same_element(&old.element, &new.element)? {
                return Err("reused tab element".into());
            }
        }
        let binding = self.shell_selected_view(after.selected_pane, None)?;
        let rechecked = self.checked_snapshot()?;
        self.check_existing_tabs(&rechecked, self.tabs.len() + 1)?;
        let current = &rechecked.tabs[self.tabs.len()];
        if rechecked.selected_index != self.tabs.len()
            || rechecked.selected_pane != after.selected_pane
            || current.runtime != new.runtime
            || !self.uia()?.same_element(&current.element, &new.element)?
        {
            return Err("new tab changed before navigation".into());
        }
        let rechecked_binding = self.shell_selected_view(rechecked.selected_pane, None)?;
        if rechecked_binding != binding {
            return Err("new tab Shell binding changed before navigation".into());
        }
        self.shell_navigate(rechecked.selected_pane, None, binding, step.folder())?;
        self.pending = Some(PendingNativeTab {
            element: new.element.clone(),
            runtime: new.runtime.clone(),
            identity: VerifiedTabIdentity::from_worker_token((self.tabs.len() + 1) as u64)
                .ok_or("tab token exhausted")?,
        });
        Ok(())
    }
    fn observe_pending(
        &mut self,
        step: &FolderRequestStep,
    ) -> Result<(PendingNativeTab, ShellBindingIdentity), PendingObservationError> {
        let snapshot = self.wait_snapshot(self.tabs.len() + 1)?;
        self.check_existing_tabs(&snapshot, self.tabs.len() + 1)?;
        let pending = self.pending.as_ref().ok_or("missing pending tab")?.clone();
        let current = &snapshot.tabs[self.tabs.len()];
        if snapshot.selected_index != self.tabs.len()
            || current.runtime != pending.runtime
            || !self
                .uia()?
                .same_element(&current.element, &pending.element)?
        {
            return Err("pending tab identity changed".into());
        }
        let binding = self
            .shell_selected_view(snapshot.selected_pane, Some(step.folder()))
            .map_err(|reason| {
                if reason == FOLDER_NOT_READY {
                    PendingObservationError::NotReady
                } else {
                    PendingObservationError::Failed(reason)
                }
            })?;
        let after = self.checked_snapshot()?;
        self.check_existing_tabs(&after, self.tabs.len() + 1)?;
        let current = &after.tabs[self.tabs.len()];
        if after.selected_index != self.tabs.len()
            || current.runtime != pending.runtime
            || !self
                .uia()?
                .same_element(&current.element, &pending.element)?
        {
            return Err("pending tab changed during folder observation".into());
        }
        let rechecked_binding = self
            .shell_selected_view(after.selected_pane, Some(step.folder()))
            .map_err(PendingObservationError::Failed)?;
        if !selected_relation_unchanged(
            snapshot.selected_pane,
            after.selected_pane,
            binding,
            rechecked_binding,
        ) {
            return Err("pending Shell binding changed during folder observation".into());
        }
        let final_snapshot = self.checked_snapshot()?;
        self.check_existing_tabs(&final_snapshot, self.tabs.len() + 1)?;
        let current = &final_snapshot.tabs[self.tabs.len()];
        if final_snapshot.selected_index != self.tabs.len()
            || final_snapshot.selected_pane != after.selected_pane
            || current.runtime != pending.runtime
            || !self
                .uia()?
                .same_element(&current.element, &pending.element)?
        {
            return Err("pending tab changed after exact folder observation".into());
        }
        Ok((self.pending.take().ok_or("pending tab retired")?, binding))
    }
    fn record_tab(
        &mut self,
        pending: PendingNativeTab,
        binding: ShellBindingIdentity,
        folder: FolderPath,
    ) -> VerifiedTabIdentity {
        let identity = pending.identity;
        self.tabs.push(NativeTab {
            element: pending.element,
            runtime: pending.runtime,
            identity,
            folder,
            binding,
        });
        identity
    }
    fn await_pending(
        &mut self,
        step: &FolderRequestStep,
    ) -> Result<(PendingNativeTab, ShellBindingIdentity), String> {
        poll_pending(|| {
            self.check().map_err(PendingObservationError::Failed)?;
            self.observe_pending(step)
        })
    }
    fn cleanup_bootstrap(&mut self) {
        if let Some(path) = &self.bootstrap_path {
            match std::fs::remove_dir(path) {
                Ok(()) => self.bootstrap_path = None,
                Err(error) => {
                    self.last_error = Some(format!("owned bootstrap directory remains: {error}"))
                }
            }
        }
    }
    fn observation_error<T>(&mut self, result: Result<T, String>) -> Result<T, SnapshotFailure> {
        result.map_err(|reason| {
            self.last_error = Some(reason);
            SnapshotFailure::ShellUnavailable
        })
    }
}

enum PendingObservationError {
    NotReady,
    Failed(String),
}

impl From<String> for PendingObservationError {
    fn from(reason: String) -> Self {
        Self::Failed(reason)
    }
}

impl From<&str> for PendingObservationError {
    fn from(reason: &str) -> Self {
        Self::Failed(reason.into())
    }
}

fn poll_pending<T>(
    mut observe: impl FnMut() -> Result<T, PendingObservationError>,
) -> Result<T, String> {
    loop {
        match observe() {
            Ok(value) => return Ok(value),
            Err(PendingObservationError::NotReady) => std::thread::sleep(Duration::from_millis(25)),
            Err(PendingObservationError::Failed(reason)) => return Err(reason),
        }
    }
}

fn selected_relation_unchanged(
    before_pane: HWND,
    after_pane: HWND,
    before_binding: ShellBindingIdentity,
    after_binding: ShellBindingIdentity,
) -> bool {
    before_pane == after_pane && before_binding == after_binding
}

#[cfg(test)]
mod native_relation_tests {
    use super::*;

    /// Opt-in, read-only diagnostic; never creates windows or starts an operation.
    #[test]
    #[ignore = "requires an interactive Windows desktop and explicit receipt path"]
    fn explorer_tabs_read_only_shell_observer_receipt() {
        let path = std::env::var_os("EXPLORER_TABS_OBSERVER_RECEIPT")
            .expect("set EXPLORER_TABS_OBSERVER_RECEIPT to an explicit receipt file");
        fn collect(
            label: &str,
            observer: Result<WindowsShellObserver, ShellObservationError>,
        ) -> String {
            let mut report = String::new();
            match observer {
                Ok(mut observer) => {
                    for sample in 0..2 {
                        let snapshot = observer.observe(None);
                        report.push_str(&format!("apartment={label} sample={sample} complete={} windows={} views={} failure={:?} trace={:?}\n",
                        snapshot.is_complete(), snapshot.windows().len(), snapshot.views().len(), snapshot.failure(), observer.last_trace));
                    }
                }
                Err(ShellObservationError::ComInitialization(_)) => report.push_str(&format!(
                    "apartment={label} connect=com-initialization-failed\n"
                )),
                Err(ShellObservationError::ShellUnavailable(_)) => {
                    report.push_str(&format!("apartment={label} connect=shell-unavailable\n"))
                }
            }
            report
        }
        let mut report = String::from("Explorer Tabs read-only Shell observation\n");
        report.push_str(&collect(
            "MTA",
            WindowsShellObserver::connect_mta_for_diagnostic(),
        ));
        let sta = ShellStaOwner::start(
            OperationId::from_token(999).unwrap(),
            Arc::new(AtomicBool::new(false)),
            Instant::now() + Duration::from_secs(10),
        );
        match sta {
            Ok(owner) => {
                for sample in 0..2 {
                    match owner.call(|state| {
                    let observer = state.observer()?;
                    let snapshot = observer.observe(None);
                    Ok((snapshot, observer.last_trace.clone()))
                }) {
                    Ok((snapshot, trace)) => report.push_str(&format!("apartment=STA-owner sample={sample} complete={} windows={} views={} failure={:?} trace={trace:?}\n",
                        snapshot.is_complete(), snapshot.windows().len(), snapshot.views().len(), snapshot.failure())),
                    Err(_) => report.push_str(&format!("apartment=STA-owner sample={sample} observation-failed\n")),
                }
                }
            }
            Err(_) => report.push_str("apartment=STA-owner connect-failed\n"),
        }
        std::fs::write(path, report).expect("write observer receipt");
    }

    #[test]
    fn explorer_tabs_shell_sta_owner_dispatch_cancel_and_shutdown() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let deadline = Instant::now() + Duration::from_secs(3);
        let owner = ShellStaOwner::start_inner(
            OperationId::from_token(91).unwrap(),
            Arc::clone(&cancelled),
            deadline,
            false,
        )
        .unwrap();
        let caller = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
        let worker = owner
            .call(|_| Ok(unsafe { windows::Win32::System::Threading::GetCurrentThreadId() }))
            .unwrap();
        assert_ne!(caller, worker);
        let started = Instant::now();
        assert_eq!(owner.call(|_| Ok(7)).unwrap(), 7);
        assert!(
            started.elapsed() < Duration::from_millis(800),
            "posted work wakes the pump"
        );
        let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
        let mutation = Arc::new(AtomicBool::new(false));
        std::thread::scope(|scope| {
            let attempt = Arc::clone(&mutation);
            let owner_ref = &owner;
            let call = scope.spawn(move || {
                owner_ref.call(move |state| {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    state.check()?;
                    attempt.store(true, Ordering::Release);
                    Ok(())
                })
            });
            entered_rx.recv().unwrap();
            cancelled.store(true, Ordering::Release);
            release_tx.send(()).unwrap();
            assert!(call.join().unwrap().is_err());
        });
        assert!(!mutation.load(Ordering::Acquire));
        drop(owner);

        // A queued command is retired before cancellation returns, even while
        // another command is still running on the owner.
        let queued_cancel = Arc::new(AtomicBool::new(false));
        let queued_owner = ShellStaOwner::start_inner(
            OperationId::from_token(93).unwrap(),
            Arc::clone(&queued_cancel),
            Instant::now() + Duration::from_secs(3),
            false,
        )
        .unwrap();
        let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
        let queued_mutation = Arc::new(AtomicBool::new(false));
        std::thread::scope(|scope| {
            let first_owner = &queued_owner;
            let first = scope.spawn(move || {
                first_owner.call(move |_| {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok(())
                })
            });
            entered_rx.recv().unwrap();
            let second_owner = &queued_owner;
            let marker = Arc::clone(&queued_mutation);
            let second = scope.spawn(move || {
                second_owner.call(move |_| {
                    marker.store(true, Ordering::Release);
                    Ok(())
                })
            });
            queued_cancel.store(true, Ordering::Release);
            assert!(second.join().unwrap().is_err());
            release_tx.send(()).unwrap();
            assert!(first.join().unwrap().is_err());
        });
        assert!(!queued_mutation.load(Ordering::Acquire));
        drop(queued_owner);

        // If mutation has already started, cancellation waits for the exact
        // attempt receipt before the caller returns.
        let running_cancel = Arc::new(AtomicBool::new(false));
        let running_owner = ShellStaOwner::start_inner(
            OperationId::from_token(94).unwrap(),
            Arc::clone(&running_cancel),
            Instant::now() + Duration::from_secs(3),
            false,
        )
        .unwrap();
        let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
        let did_attempt = Arc::new(AtomicBool::new(false));
        std::thread::scope(|scope| {
            let owner_ref = &running_owner;
            let marker = Arc::clone(&did_attempt);
            let running = scope.spawn(move || {
                owner_ref.call(move |_| {
                    marker.store(true, Ordering::Release);
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok(())
                })
            });
            entered_rx.recv().unwrap();
            running_cancel.store(true, Ordering::Release);
            release_tx.send(()).unwrap();
            assert!(running.join().unwrap().is_err());
        });
        assert!(did_attempt.load(Ordering::Acquire));
        drop(running_owner);

        let cancelled = Arc::new(AtomicBool::new(false));
        let owner = ShellStaOwner::start_inner(
            OperationId::from_token(95).unwrap(),
            Arc::clone(&cancelled),
            Instant::now() + Duration::from_secs(3),
            false,
        )
        .unwrap();
        let attempts = Arc::new(AtomicBool::new(false));
        cancelled.store(true, Ordering::Release);
        let marker = Arc::clone(&attempts);
        assert!(
            owner
                .call(move |_| {
                    marker.store(true, Ordering::Release);
                    Ok(())
                })
                .is_err()
        );
        assert!(!attempts.load(Ordering::Acquire));
        drop(owner);
        let mut expired = ShellStaOwner::start_inner(
            OperationId::from_token(92).unwrap(),
            Arc::new(AtomicBool::new(false)),
            Instant::now() + Duration::from_secs(3),
            false,
        )
        .unwrap();
        expired.deadline = Instant::now();
        assert!(expired.call(|_| Ok(())).is_err());
        drop(expired);
    }

    #[test]
    fn explorer_tabs_backend_replaced_selected_pane_cannot_verify_old_view() {
        let binding = ShellBindingIdentity::from_worker_token(1).unwrap();
        let other = ShellBindingIdentity::from_worker_token(2).unwrap();
        let first = hwnd_from_usize(10);
        let replacement = hwnd_from_usize(20);
        assert!(!selected_relation_unchanged(
            first,
            replacement,
            binding,
            binding
        ));
        assert!(!selected_relation_unchanged(first, first, binding, other));
        assert!(selected_relation_unchanged(first, first, binding, binding));
    }

    #[test]
    fn explorer_tabs_backend_only_initial_not_ready_may_retry() {
        let mut calls = 0;
        let ready = poll_pending(|| {
            calls += 1;
            if calls == 1 {
                Err(PendingObservationError::NotReady)
            } else {
                Ok(42)
            }
        });
        assert_eq!(ready.unwrap(), 42);
        assert_eq!(calls, 2);
        let mut calls = 0;
        let contradicted = poll_pending(|| {
            calls += 1;
            if calls == 1 {
                Err(PendingObservationError::Failed(FOLDER_NOT_READY.into()))
            } else {
                Ok(42)
            }
        });
        assert_eq!(contradicted.unwrap_err(), FOLDER_NOT_READY);
        assert_eq!(calls, 1, "a changed exact observation must not be retried");
    }

    #[test]
    fn explorer_tabs_backend_bootstrap_receipt_rejects_preexisting_and_ambiguous_windows() {
        use crate::explorer_tabs::identity::test_support as ids;
        let path = FolderPath::parse(r"C:\private-bootstrap").unwrap();
        let receipt = BootstrapReceipt {
            operation_id: OperationId::from_token(7).unwrap(),
            private_path: path.clone(),
        };
        let old = ids::window(10);
        let first = ids::window(20);
        let second = ids::window(30);
        let row = |window, binding, view, index| {
            ids::view(
                window,
                ids::shell_binding(binding),
                ids::view_identity(view),
                index,
                Some(ids::path_comparison(path.clone(), PidlPathMatch::Exact)),
            )
        };
        let before = ShellViewSnapshot::complete(vec![old], vec![row(old, 1, 1, 0)]).unwrap();
        assert!(
            attributed_bootstrap(&before, &before, &receipt)
                .unwrap()
                .is_none()
        );
        let unique = ShellViewSnapshot::complete(
            vec![old, first],
            vec![row(old, 1, 1, 0), row(first, 2, 2, 1)],
        )
        .unwrap();
        assert_eq!(
            attributed_bootstrap(&before, &unique, &receipt)
                .unwrap()
                .unwrap()
                .window(),
            first
        );
        let ambiguous = ShellViewSnapshot::complete(
            vec![old, first, second],
            vec![row(old, 1, 1, 0), row(first, 2, 2, 1), row(second, 3, 3, 2)],
        )
        .unwrap();
        assert!(attributed_bootstrap(&before, &ambiguous, &receipt).is_err());

        // Exercise the production driver with the same attribution helper and
        // record its fake port's navigation targets. No OS action is performed.
        struct AttributionPort {
            before: ShellViewSnapshot,
            after: ShellViewSnapshot,
            receipt: BootstrapReceipt,
            target: Option<BoundTargetEvidence>,
            navigated: Vec<usize>,
        }
        impl ExplorerTabsPort for AttributionPort {
            fn preflight(&mut self) -> Result<(), BackendDiagnostic> {
                Ok(())
            }
            fn request(
                &mut self,
                _step: &FolderRequestStep,
            ) -> Result<RequestResult, BackendDiagnostic> {
                let target = attributed_bootstrap(&self.before, &self.after, &self.receipt)
                    .ok()
                    .flatten()
                    .ok_or_else(|| BackendDiagnostic {
                        reason: "unattributed bootstrap".into(),
                        bootstrap: BootstrapState::Attempted,
                        owned_resource_remaining: true,
                        owned_resource_path: None,
                        requested_step_attempted: false,
                    })?;
                self.navigated.push(target.window().hwnd());
                self.target = Some(target);
                Ok(RequestResult::Accepted)
            }
            fn observe_initial(
                &mut self,
                step: &FolderRequestStep,
            ) -> Result<InitialTargetEvidence, SnapshotFailure> {
                Ok(InitialTargetEvidence::verified(
                    self.target
                        .clone()
                        .ok_or(SnapshotFailure::InvalidWindowIdentity)?,
                    ShellBindingIdentity::from_worker_token(1).unwrap(),
                    VerifiedTabIdentity::from_worker_token(1).unwrap(),
                    1,
                    FolderIdentityEvidence::from_pidl_check(
                        step.request_index(),
                        step.folder().clone(),
                        FolderIdentityResult::ExactPidl,
                    ),
                ))
            }
            fn observe_append(
                &mut self,
                _step: &FolderRequestStep,
                _prefix: &[VerifiedFolderTab],
            ) -> Result<VerifiedTabEvidence, SnapshotFailure> {
                Err(SnapshotFailure::ShellUnavailable)
            }
            fn verify_final(
                &mut self,
                _prefix: &[VerifiedFolderTab],
            ) -> Result<(), BackendDiagnostic> {
                Ok(())
            }
        }
        for (after, expected_targets) in [
            (&before, vec![]),
            (&ambiguous, vec![]),
            (&unique, vec![first.hwnd()]),
        ] {
            let mut port = AttributionPort {
                before: before.clone(),
                after: after.clone(),
                receipt: BootstrapReceipt {
                    operation_id: receipt.operation_id,
                    private_path: path.clone(),
                },
                target: None,
                navigated: Vec::new(),
            };
            let request = crate::explorer_tabs::operation::FolderTabRequest::new(vec![
                FolderPath::parse(r"C:\requested").unwrap(),
            ])
            .unwrap();
            let report = crate::explorer_tabs::backend::drive(
                request,
                receipt.operation_id,
                &mut port,
                &AtomicBool::new(false),
                Instant::now() + Duration::from_secs(1),
                &mut |_| {},
            );
            assert_eq!(port.navigated, expected_targets);
            assert!(!port.navigated.contains(&old.hwnd()));
            if port.navigated.is_empty() {
                assert!(report.operation.verified_prefix().is_empty());
                assert!(report.operation.attempted_unverified().is_none());
            } else {
                assert_eq!(
                    report.operation.outcome(),
                    crate::explorer_tabs::operation::OperationOutcome::Completed
                );
            }
        }
    }
}

impl ExplorerTabsPort for WindowsExplorerTabsPort {
    fn preflight(&mut self) -> Result<(), BackendDiagnostic> {
        let result = (|| -> Result<(), String> {
            self.check()?;
            let mut version = OSVERSIONINFOW {
                dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
                ..Default::default()
            };
            let status = unsafe { RtlGetVersion(&mut version) };
            self.check()?;
            if status < 0 || version.dwMajorVersion < 10 || version.dwBuildNumber < 22621 {
                return Err(
                    "Windows 11 Explorer tab capability requires build 22621 or later".into(),
                );
            }
            self.mta_apartment = Some(
                crate::mkmacro::uia::initialize_com_apartment()
                    .map_err(|e| format!("UIA apartment: {e}"))?,
            );
            self.shell_sta = Some(ShellStaOwner::start(
                self.operation_id,
                Arc::clone(&self.cancelled),
                self.deadline,
            )?);
            self.check()?;
            let uia = ExplorerUia::connect(Arc::clone(&self.cancelled), self.deadline)?;
            self.check()?;
            let (baseline, trace) = self.shell_observe(None)?;
            self.check()?;
            if !baseline.is_complete() {
                return Err(format!(
                    "incomplete Shell baseline: {:?} ({:?})",
                    baseline.failure(),
                    trace
                ));
            }
            let opennew = self.opennew_supported()?;
            self.check()?;
            if baseline.views().is_empty() && !opennew {
                return Err("no Shell browser and no supported opennew verb".into());
            }
            self.opennew_available = opennew;
            self.baseline = Some(baseline);
            self.uia = Some(uia);
            Ok(())
        })();
        result.map_err(|reason| self.diagnostic_for(reason))
    }
    fn request(&mut self, step: &FolderRequestStep) -> Result<RequestResult, BackendDiagnostic> {
        self.requested_step_attempted = false;
        self.mutation_attempted.store(false, Ordering::Release);
        let result = match step.kind() {
            FolderRequestKind::CreateInitialWindow => self.prepare_initial(step),
            FolderRequestKind::OpenAdditionalTab => self.prepare_append(step),
        };
        match result {
            Ok(()) => Ok(RequestResult::Accepted),
            Err(reason) => {
                if self.bootstrap == BootstrapState::NotStarted
                    && self.bootstrap_attempted.load(Ordering::Acquire)
                {
                    self.bootstrap = BootstrapState::Attempted;
                }
                if self.bootstrap == BootstrapState::NotStarted {
                    self.cleanup_bootstrap();
                }
                Err(self.diagnostic_for(reason))
            }
        }
    }
    fn observe_initial(
        &mut self,
        step: &FolderRequestStep,
    ) -> Result<InitialTargetEvidence, SnapshotFailure> {
        let result = self.await_pending(step);
        let (pending, binding) = self.observation_error(result)?;
        let Some(target) = self.target.clone() else {
            return Err(SnapshotFailure::InvalidWindowIdentity);
        };
        let identity = self.record_tab(pending, binding, step.folder().clone());
        self.cleanup_bootstrap();
        Ok(InitialTargetEvidence::verified(
            target,
            binding,
            identity,
            step.expected_ordinal(),
            FolderIdentityEvidence::from_pidl_check(
                step.request_index(),
                step.folder().clone(),
                FolderIdentityResult::ExactPidl,
            ),
        ))
    }
    fn observe_append(
        &mut self,
        step: &FolderRequestStep,
        prefix: &[VerifiedFolderTab],
    ) -> Result<VerifiedTabEvidence, SnapshotFailure> {
        if prefix.len() != self.tabs.len()
            || prefix.iter().zip(&self.tabs).any(|(verified, tab)| {
                verified.identity() != tab.identity
                    || verified.folder() != &tab.folder
                    || verified.shell_binding() != tab.binding
            })
        {
            self.last_error = Some("verified prefix changed".into());
            return Err(SnapshotFailure::InvalidWindowIdentity);
        }
        let result = self.await_pending(step);
        let (pending, binding) = self.observation_error(result)?;
        let Some(target) = self.target.clone() else {
            return Err(SnapshotFailure::InvalidWindowIdentity);
        };
        let identity = self.record_tab(pending, binding, step.folder().clone());
        Ok(VerifiedTabEvidence::verified(
            target,
            binding,
            identity,
            step.expected_ordinal(),
            FolderIdentityEvidence::from_pidl_check(
                step.request_index(),
                step.folder().clone(),
                FolderIdentityResult::ExactPidl,
            ),
        ))
    }
    fn verify_final(&mut self, prefix: &[VerifiedFolderTab]) -> Result<(), BackendDiagnostic> {
        let result = (|| -> Result<(), String> {
            if prefix.len() != self.tabs.len()
                || prefix.iter().zip(&self.tabs).any(|(verified, tab)| {
                    verified.identity() != tab.identity
                        || verified.folder() != &tab.folder
                        || verified.shell_binding() != tab.binding
                })
            {
                return Err("verified prefix changed".into());
            }
            for index in 0..self.tabs.len() {
                self.check()?;
                let before = self.checked_snapshot()?;
                self.check_existing_tabs(&before, self.tabs.len())?;
                let current_index = before.selected_index;
                let current_folder = self.tabs[current_index].folder.clone();
                let current_binding =
                    self.shell_selected_view(before.selected_pane, Some(&current_folder))?;
                if current_binding != self.tabs[current_index].binding {
                    return Err("current selected binding changed".into());
                }
                let pre_select = self.checked_snapshot()?;
                self.check_existing_tabs(&pre_select, self.tabs.len())?;
                if pre_select.selected_index != current_index
                    || pre_select.selected_pane != before.selected_pane
                {
                    return Err("selected pane changed before final selection".into());
                }
                let rechecked_current =
                    self.shell_selected_view(pre_select.selected_pane, Some(&current_folder))?;
                if rechecked_current != current_binding {
                    return Err("current Shell binding changed before final selection".into());
                }
                if current_index != index {
                    self.uia()?.select(&pre_select.tabs[index].element)?;
                }
                let after = self.checked_snapshot()?;
                self.check_existing_tabs(&after, self.tabs.len())?;
                if after.selected_index != index {
                    return Err("selected tab changed during final check".into());
                }
                let folder = self.tabs[index].folder.clone();
                let binding = self.shell_selected_view(after.selected_pane, Some(&folder))?;
                if binding != self.tabs[index].binding {
                    return Err("verified Shell binding changed".into());
                }
                let final_snapshot = self.checked_snapshot()?;
                self.check_existing_tabs(&final_snapshot, self.tabs.len())?;
                if final_snapshot.selected_index != index
                    || final_snapshot.selected_pane != after.selected_pane
                {
                    return Err("selection changed during final verification".into());
                }
                let rechecked_binding =
                    self.shell_selected_view(final_snapshot.selected_pane, Some(&folder))?;
                if rechecked_binding != binding {
                    return Err("Shell binding changed during final verification".into());
                }
            }
            Ok(())
        })();
        result.map_err(|reason| self.diagnostic_for(reason))
    }
    fn diagnostic(&self) -> Option<BackendDiagnostic> {
        self.last_error
            .as_ref()
            .map(|reason| self.diagnostic_for(reason.clone()))
    }
}
