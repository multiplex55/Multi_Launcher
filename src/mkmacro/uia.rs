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
        MkUiPayload,
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
            .send(Request::Execute(p.clone(), c, tx))
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
    if s.ancestor_path.iter().any(|p| part_empty(p)) {
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
}
