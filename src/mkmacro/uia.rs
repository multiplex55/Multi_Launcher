//! UI Automation boundaries. COM objects are owned exclusively by `UiaWorker`'s thread.
use super::{
    DiagnosticKind, ExecResult, ExecutionDiagnostic, MkPoint, MkUiControlType, MkUiPattern,
    MkUiPayload, MkUiSelector, MkUiSelectorPart,
};
use std::{collections::HashSet, sync::mpsc, thread, time::Duration};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiElementInfo {
    pub selector: MkUiSelector,
    pub user_facing_name: String,
    pub target_executable: String,
    pub supported_patterns: HashSet<MkUiPattern>,
    /// Screen-space bounds used only to paint the noninteractive picker highlight.
    pub bounds: Option<(i32, i32, i32, i32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiCommand {
    Exists,
    Invoke,
    SetValue(String),
    ReadValue,
    Toggle,
    Select,
    Focus,
}

/// Implementations may contain COM pointers; consequently they are constructed, used and
/// dropped on the worker thread and are deliberately not required to be `Send`.
pub trait UiaDriver: 'static {
    fn execute(&mut self, target: &MkUiPayload, command: UiCommand) -> ExecResult<Option<String>>;
    fn inspect_at(&mut self, point: MkPoint) -> ExecResult<UiElementInfo>;
}

enum Request {
    Execute(
        Box<MkUiPayload>,
        UiCommand,
        mpsc::Sender<ExecResult<Option<String>>>,
    ),
    Inspect(MkPoint, mpsc::Sender<ExecResult<UiElementInfo>>),
    Stop,
}

/// Synchronous facade over a dedicated COM-initialized UIA worker.
pub struct UiaWorker {
    tx: mpsc::Sender<Request>,
    join: Option<thread::JoinHandle<()>>,
    timeout: Duration,
}
impl UiaWorker {
    pub fn spawn<F, D>(initialize_com_and_driver: F, timeout: Duration) -> ExecResult<Self>
    where
        F: FnOnce() -> ExecResult<D> + Send + 'static,
        D: UiaDriver,
    {
        let (tx, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("mkmacro-uia-com".into())
            .spawn(move || {
                let mut driver = match initialize_com_and_driver() {
                    Ok(x) => {
                        let _ = ready_tx.send(Ok(()));
                        x
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                while let Ok(request) = rx.recv() {
                    match request {
                        Request::Execute(p, c, out) => {
                            let _ = out.send(driver.execute(&p, c));
                        }
                        Request::Inspect(p, out) => {
                            let _ = out.send(driver.inspect_at(p));
                        }
                        Request::Stop => break,
                    }
                }
            })
            .map_err(|e| {
                diag(
                    DiagnosticKind::ComFailure,
                    format!("could not start UIA COM worker: {e}"),
                )
            })?;
        ready_rx
            .recv_timeout(timeout)
            .map_err(|_| diag(DiagnosticKind::Timeout, "UIA COM initialization timed out"))??;
        Ok(Self {
            tx,
            join: Some(join),
            timeout,
        })
    }
    pub fn execute(&self, p: &MkUiPayload, c: UiCommand) -> ExecResult<Option<String>> {
        let (tx, rx) = mpsc::channel();
        self.tx
            .send(Request::Execute(Box::new(p.clone()), c, tx))
            .map_err(disconnected)?;
        rx.recv_timeout(self.timeout)
            .map_err(|_| diag(DiagnosticKind::Timeout, "UI Automation action timed out"))?
    }
    pub fn inspect_at(&self, p: MkPoint) -> ExecResult<UiElementInfo> {
        let (tx, rx) = mpsc::channel();
        self.tx
            .send(Request::Inspect(p, tx))
            .map_err(disconnected)?;
        rx.recv_timeout(self.timeout).map_err(|_| {
            diag(
                DiagnosticKind::Timeout,
                "UI Automation inspection timed out",
            )
        })?
    }
}
impl Drop for UiaWorker {
    fn drop(&mut self) {
        let _ = self.tx.send(Request::Stop);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}
impl super::UiAutomationBackend for UiaWorker {
    fn exists(&self, p: &MkUiPayload) -> ExecResult<bool> {
        self.execute(p, UiCommand::Exists).map(|_| true)
    }
    fn invoke(&self, p: &MkUiPayload) -> ExecResult {
        self.execute(p, UiCommand::Invoke).map(drop)
    }
    fn set_value(&self, p: &MkUiPayload, v: &str) -> ExecResult {
        self.execute(p, UiCommand::SetValue(v.into())).map(drop)
    }
    fn read_value(&self, p: &MkUiPayload) -> ExecResult<String> {
        self.execute(p, UiCommand::ReadValue)?.ok_or_else(|| {
            diag(
                DiagnosticKind::UnsupportedPattern,
                "Value pattern returned no value",
            )
        })
    }
    fn toggle(&self, p: &MkUiPayload) -> ExecResult {
        self.execute(p, UiCommand::Toggle).map(drop)
    }
    fn select(&self, p: &MkUiPayload) -> ExecResult {
        self.execute(p, UiCommand::Select).map(drop)
    }
    fn focus(&self, p: &MkUiPayload) -> ExecResult {
        self.execute(p, UiCommand::Focus).map(drop)
    }
}
impl super::UiAutomationInspector for UiaWorker {
    fn inspect_at(&self, p: MkPoint) -> ExecResult<UiElementInfo> {
        UiaWorker::inspect_at(self, p)
    }
}

fn disconnected<T: std::fmt::Display>(e: T) -> ExecutionDiagnostic {
    diag(
        DiagnosticKind::ComFailure,
        format!("UIA COM worker disconnected: {e}"),
    )
}
fn diag(k: DiagnosticKind, m: impl Into<String>) -> ExecutionDiagnostic {
    ExecutionDiagnostic::new(k, m)
}

/// Required selector rules: a non-empty window matcher (validated with the payload), plus at
/// least one of AutomationId, Name, ClassName or ControlType. FrameworkId only narrows a match.
pub fn validate_selector(s: &MkUiSelector) -> ExecResult<()> {
    if s.automation_id.as_deref().is_none_or(str::is_empty)
        && s.name.as_deref().is_none_or(str::is_empty)
        && s.class_name.as_deref().is_none_or(str::is_empty)
        && s.control_type.is_none()
    {
        return Err(diag(
            DiagnosticKind::InvalidTarget,
            "UIA selector requires AutomationId, Name, ClassName, or ControlType",
        ));
    }
    if s.ancestor_path.iter().any(part_empty) {
        return Err(diag(
            DiagnosticKind::InvalidTarget,
            "UIA ancestor path contains an empty selector",
        ));
    }
    Ok(())
}
fn part_empty(p: &MkUiSelectorPart) -> bool {
    p.automation_id.as_deref().is_none_or(str::is_empty)
        && p.name.as_deref().is_none_or(str::is_empty)
        && p.class_name.as_deref().is_none_or(str::is_empty)
        && p.control_type.is_none()
}

/// Enforces unique resolution. Zero and multiple results never select an arbitrary element.
pub fn require_unique<T>(mut matches: Vec<T>) -> ExecResult<T> {
    match matches.len() {
        0 => Err(diag(
            DiagnosticKind::TargetNotFound,
            "UI Automation element was not found",
        )),
        1 => Ok(matches.pop().unwrap()),
        n => Err(diag(
            DiagnosticKind::AmbiguousTarget,
            format!("UI Automation selector matched {n} elements"),
        )),
    }
}
pub fn require_pattern(info: &UiElementInfo, pattern: MkUiPattern) -> ExecResult<()> {
    if pattern == MkUiPattern::Focus || info.supported_patterns.contains(&pattern) {
        Ok(())
    } else {
        Err(diag(
            DiagnosticKind::UnsupportedPattern,
            format!("element does not support {pattern:?}"),
        ))
    }
}

#[cfg(windows)]
pub struct ComApartment;
#[cfg(windows)]
impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { windows::Win32::System::Com::CoUninitialize() }
    }
}
#[cfg(windows)]
pub fn initialize_com_apartment() -> ExecResult<ComApartment> {
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
        .ok()
        .map_err(|e| {
            diag(
                DiagnosticKind::ComFailure,
                format!("CoInitializeEx failed: {e}"),
            )
        })?;
    Ok(ComApartment)
}

const MAX_INSPECTED_STRING_BYTES: usize = 4096;

fn bounded_inspection_string(mut text: String) -> Option<String> {
    if text.len() > MAX_INSPECTED_STRING_BYTES {
        let mut end = MAX_INSPECTED_STRING_BYTES;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn executable_matcher(path: &str) -> String {
    path.rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
        .trim()
        .to_owned()
}

/// Persistent recorder click-inspection state. It must be created, used, and
/// dropped on the recorder's dedicated UIA provider lane.
#[cfg(windows)]
pub struct SystemUiaInspector {
    automation: windows::Win32::UI::Accessibility::IUIAutomation,
    cache: windows::Win32::UI::Accessibility::IUIAutomationCacheRequest,
    walker: windows::Win32::UI::Accessibility::IUIAutomationTreeWalker,
    // Fields drop in declaration order; COM interfaces must release first.
    _apartment: ComApartment,
}

#[cfg(windows)]
impl SystemUiaInspector {
    pub fn new() -> ExecResult<Self> {
        use windows::{
            Win32::{
                System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
                UI::Accessibility::*,
            },
            core::Interface,
        };
        let apartment = initialize_com_apartment()?;
        let automation: IUIAutomation =
            unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.map_err(
                |e| {
                    diag(
                        DiagnosticKind::ComFailure,
                        format!("UI Automation initialization failed: {e}"),
                    )
                },
            )?;
        if let Ok(v2) = automation.cast::<IUIAutomation2>() {
            // These native provider timeouts bound cross-process calls. The outer
            // recorder lane independently circuit-breaks if the budget is exceeded.
            let _ = unsafe { v2.SetConnectionTimeout(500) };
            let _ = unsafe { v2.SetTransactionTimeout(500) };
        }
        let cache = unsafe { automation.CreateCacheRequest() }.map_err(|e| {
            diag(
                DiagnosticKind::ComFailure,
                format!("could not create UI Automation cache request: {e}"),
            )
        })?;
        unsafe { cache.SetTreeScope(TreeScope_Element) }.map_err(|e| {
            diag(
                DiagnosticKind::ComFailure,
                format!("could not configure UI Automation cache scope: {e}"),
            )
        })?;
        for property in [
            UIA_ProcessIdPropertyId,
            UIA_ControlTypePropertyId,
            UIA_NamePropertyId,
            UIA_AutomationIdPropertyId,
            UIA_ClassNamePropertyId,
            UIA_FrameworkIdPropertyId,
            UIA_BoundingRectanglePropertyId,
        ] {
            unsafe { cache.AddProperty(property) }.map_err(|e| {
                diag(
                    DiagnosticKind::ComFailure,
                    format!("could not configure UI Automation property cache: {e}"),
                )
            })?;
        }
        for pattern in [
            UIA_InvokePatternId,
            UIA_ValuePatternId,
            UIA_TogglePatternId,
            UIA_SelectionItemPatternId,
        ] {
            unsafe { cache.AddPattern(pattern) }.map_err(|e| {
                diag(
                    DiagnosticKind::ComFailure,
                    format!("could not configure UI Automation pattern cache: {e}"),
                )
            })?;
        }
        let walker = unsafe { automation.ControlViewWalker() }.map_err(|e| {
            diag(
                DiagnosticKind::ComFailure,
                format!("could not create UI Automation tree walker: {e}"),
            )
        })?;
        Ok(Self {
            automation,
            cache,
            walker,
            _apartment: apartment,
        })
    }

    pub fn inspect_at(&mut self, point: MkPoint) -> ExecResult<UiElementInfo> {
        use windows::Win32::{Foundation::POINT, UI::Accessibility::*};
        let element = unsafe {
            self.automation.ElementFromPointBuildCache(
                POINT {
                    x: point.x,
                    y: point.y,
                },
                &self.cache,
            )
        }
        .map_err(|e| {
            diag(
                DiagnosticKind::TargetNotFound,
                format!("UI Automation inspection failed: {e}"),
            )
        })?;
        let bounded = |value: windows::core::BSTR| bounded_inspection_string(value.to_string());
        let automation_id = unsafe { element.CachedAutomationId() }
            .ok()
            .and_then(bounded);
        let name = unsafe { element.CachedName() }.ok().and_then(bounded);
        let class_name = unsafe { element.CachedClassName() }.ok().and_then(bounded);
        let framework_id = unsafe { element.CachedFrameworkId() }
            .ok()
            .and_then(bounded);
        let control_id = unsafe { element.CachedControlType() }.ok();
        let control_type = control_id.map(|id| {
            if id == UIA_ButtonControlTypeId {
                MkUiControlType::Button
            } else if id == UIA_EditControlTypeId {
                MkUiControlType::Edit
            } else if id == UIA_CheckBoxControlTypeId {
                MkUiControlType::CheckBox
            } else if id == UIA_RadioButtonControlTypeId {
                MkUiControlType::RadioButton
            } else if id == UIA_ComboBoxControlTypeId {
                MkUiControlType::ComboBox
            } else if id == UIA_ListItemControlTypeId {
                MkUiControlType::ListItem
            } else if id == UIA_TabItemControlTypeId {
                MkUiControlType::TabItem
            } else if id == UIA_MenuItemControlTypeId {
                MkUiControlType::MenuItem
            } else if id == UIA_TreeItemControlTypeId {
                MkUiControlType::TreeItem
            } else if id == UIA_TextControlTypeId {
                MkUiControlType::Text
            } else if id == UIA_CustomControlTypeId {
                MkUiControlType::Custom
            } else {
                MkUiControlType::Other(format!("UIA {}", id.0))
            }
        });
        let bounds = unsafe { element.CachedBoundingRectangle() }
            .ok()
            .and_then(|rect| {
                (rect.right > rect.left && rect.bottom > rect.top).then_some((
                    rect.left,
                    rect.top,
                    rect.right,
                    rect.bottom,
                ))
            });
        let mut supported_patterns = HashSet::new();
        for (id, pattern) in [
            (UIA_InvokePatternId, MkUiPattern::Invoke),
            (UIA_ValuePatternId, MkUiPattern::Value),
            (UIA_TogglePatternId, MkUiPattern::Toggle),
            (UIA_SelectionItemPatternId, MkUiPattern::SelectionItem),
        ] {
            if unsafe { element.GetCachedPattern(id) }.is_ok() {
                supported_patterns.insert(pattern);
            }
        }
        supported_patterns.insert(MkUiPattern::Focus);
        let pid = unsafe { element.CachedProcessId() }
            .ok()
            .filter(|pid| *pid > 0)
            .map(|pid| pid as u32);
        let target_executable = pid
            .and_then(process_path_for_pid)
            .map(|path| executable_matcher(&path))
            .unwrap_or_default();
        let mut ancestor_path = Vec::new();
        // Ancestors are not a legal CacheRequest tree scope. Fetch each parent
        // explicitly with the same element-only cache, with a strict depth cap.
        let mut parent = unsafe {
            self.walker
                .GetParentElementBuildCache(&element, &self.cache)
        }
        .ok();
        while let Some(ancestor) = parent.take() {
            if ancestor_path.len() >= 4 {
                break;
            }
            let part = MkUiSelectorPart {
                automation_id: unsafe { ancestor.CachedAutomationId() }
                    .ok()
                    .and_then(bounded),
                name: unsafe { ancestor.CachedName() }.ok().and_then(bounded),
                class_name: unsafe { ancestor.CachedClassName() }.ok().and_then(bounded),
                control_type: unsafe { ancestor.CachedControlType() }
                    .ok()
                    .map(control_type_from_id),
                framework_id: unsafe { ancestor.CachedFrameworkId() }
                    .ok()
                    .and_then(bounded),
            };
            if !part_empty(&part) {
                ancestor_path.push(part);
            }
            parent = unsafe {
                self.walker
                    .GetParentElementBuildCache(&ancestor, &self.cache)
            }
            .ok();
        }
        ancestor_path.reverse();
        Ok(UiElementInfo {
            selector: MkUiSelector {
                automation_id,
                name: name.clone(),
                class_name,
                control_type,
                framework_id,
                ancestor_path,
            },
            user_facing_name: name.unwrap_or_default(),
            target_executable,
            supported_patterns,
            bounds,
        })
    }
}

#[cfg(windows)]
fn control_type_from_id(
    id: windows::Win32::UI::Accessibility::UIA_CONTROLTYPE_ID,
) -> MkUiControlType {
    use windows::Win32::UI::Accessibility::*;
    if id == UIA_ButtonControlTypeId {
        MkUiControlType::Button
    } else if id == UIA_EditControlTypeId {
        MkUiControlType::Edit
    } else if id == UIA_CheckBoxControlTypeId {
        MkUiControlType::CheckBox
    } else if id == UIA_RadioButtonControlTypeId {
        MkUiControlType::RadioButton
    } else if id == UIA_ComboBoxControlTypeId {
        MkUiControlType::ComboBox
    } else if id == UIA_ListItemControlTypeId {
        MkUiControlType::ListItem
    } else if id == UIA_TabItemControlTypeId {
        MkUiControlType::TabItem
    } else if id == UIA_MenuItemControlTypeId {
        MkUiControlType::MenuItem
    } else if id == UIA_TreeItemControlTypeId {
        MkUiControlType::TreeItem
    } else if id == UIA_TextControlTypeId {
        MkUiControlType::Text
    } else if id == UIA_CustomControlTypeId {
        MkUiControlType::Custom
    } else {
        MkUiControlType::Other(format!("UIA {}", id.0))
    }
}

#[cfg(windows)]
fn process_path_for_pid(pid: u32) -> Option<String> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};
    use windows::{
        Win32::{
            Foundation::CloseHandle,
            System::Threading::{
                OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        },
        core::PWSTR,
    };
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buffer = vec![0u16; 32_768];
    let mut size = buffer.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        )
    };
    let _ = unsafe { CloseHandle(handle) };
    result.ok()?;
    (size > 0).then(|| {
        OsString::from_wide(&buffer[..size as usize])
            .to_string_lossy()
            .into_owned()
    })
}

/// Compatibility entry point; recorder production keeps `SystemUiaInspector`
/// alive on its bounded provider lane instead of recreating it per click.
#[cfg(windows)]
pub fn inspect_at_system(point: MkPoint) -> ExecResult<UiElementInfo> {
    SystemUiaInspector::new()?.inspect_at(point)
}

#[cfg(not(windows))]
pub fn inspect_at_system(_: MkPoint) -> ExecResult<UiElementInfo> {
    Err(diag(
        DiagnosticKind::UnsupportedPlatform,
        "UI Automation inspection is only available on Windows",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::MkWindowMatcher;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    fn selector() -> MkUiSelector {
        MkUiSelector {
            automation_id: Some("save".into()),
            name: Some("Save".into()),
            control_type: Some(MkUiControlType::Button),
            class_name: Some("Button".into()),
            framework_id: Some("Win32".into()),
            ancestor_path: vec![],
        }
    }
    #[test]
    fn inspected_strings_truncate_only_on_utf8_boundaries() {
        let value = format!("{}érest", "a".repeat(MAX_INSPECTED_STRING_BYTES - 1));
        let bounded = bounded_inspection_string(value).unwrap();
        assert_eq!(bounded.len(), MAX_INSPECTED_STRING_BYTES - 1);
        assert!(bounded.is_char_boundary(bounded.len()));
        assert_eq!(executable_matcher(r"C:\Tools\Runner.EXE"), "Runner.EXE");
    }
    fn info(patterns: &[MkUiPattern]) -> UiElementInfo {
        UiElementInfo {
            selector: selector(),
            user_facing_name: "Save".into(),
            target_executable: "app.exe".into(),
            supported_patterns: patterns.iter().copied().collect(),
            bounds: Some((10, 10, 100, 30)),
        }
    }
    struct Fake {
        stopped: Arc<AtomicBool>,
    }
    impl Drop for Fake {
        fn drop(&mut self) {
            self.stopped.store(true, Ordering::SeqCst)
        }
    }
    impl UiaDriver for Fake {
        fn execute(&mut self, _: &MkUiPayload, c: UiCommand) -> ExecResult<Option<String>> {
            match c {
                UiCommand::ReadValue => Ok(Some("hello".into())),
                _ => Ok(None),
            }
        }
        fn inspect_at(&mut self, _: MkPoint) -> ExecResult<UiElementInfo> {
            Ok(info(&[MkUiPattern::Invoke]))
        }
    }
    #[test]
    fn selector_serialization_round_trip() {
        let s = selector();
        assert_eq!(
            serde_json::from_str::<MkUiSelector>(&serde_json::to_string(&s).unwrap()).unwrap(),
            s
        );
        validate_selector(&s).unwrap();
    }
    #[test]
    fn unique_missing_and_ambiguous() {
        assert_eq!(require_unique(vec![3]).unwrap(), 3);
        assert_eq!(
            require_unique::<i32>(vec![]).unwrap_err().kind,
            DiagnosticKind::TargetNotFound
        );
        assert_eq!(
            require_unique(vec![1, 2]).unwrap_err().kind,
            DiagnosticKind::AmbiguousTarget
        );
    }
    #[test]
    fn unsupported_pattern_is_structured() {
        assert_eq!(
            require_pattern(&info(&[]), MkUiPattern::Invoke)
                .unwrap_err()
                .kind,
            DiagnosticKind::UnsupportedPattern
        );
    }
    #[test]
    fn worker_reads_and_shuts_down() {
        let stopped = Arc::new(AtomicBool::new(false));
        let flag = stopped.clone();
        let w =
            UiaWorker::spawn(move || Ok(Fake { stopped: flag }), Duration::from_secs(1)).unwrap();
        let p = MkUiPayload {
            window: MkWindowMatcher {
                title: None,
                title_regex: None,
                process: Some("app.exe".into()),
                class: None,
            },
            selector: selector(),
            wait: None,
        };
        assert_eq!(
            super::super::UiAutomationBackend::read_value(&w, &p).unwrap(),
            "hello"
        );
        drop(w);
        assert!(stopped.load(Ordering::SeqCst));
    }

    #[test]
    fn inspected_strings_truncate_on_utf8_boundaries() {
        let value = format!("{}é-tail", "a".repeat(MAX_INSPECTED_STRING_BYTES - 1));
        let bounded = bounded_inspection_string(value).unwrap();
        assert_eq!(bounded.len(), MAX_INSPECTED_STRING_BYTES - 1);
        assert!(bounded.is_char_boundary(bounded.len()));
        assert_eq!(executable_matcher(r"C:\Tools\Editor.EXE"), "Editor.EXE");
    }
}
