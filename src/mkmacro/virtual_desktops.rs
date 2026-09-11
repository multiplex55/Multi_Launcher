//! Semantic virtual-desktop automation backed by the shared desktop service.
use super::{DiagnosticKind, ExecResult, ExecutionDiagnostic, MkVirtualDesktopAction};
use crate::virtual_desktop::{
    AdjacentDirection, VirtualDesktopError, VirtualDesktopErrorKind, VirtualDesktopSelector,
    VirtualDesktopService,
};
use std::sync::Arc;

pub trait VirtualDesktopBackend: Send + Sync {
    fn create(&self) -> ExecResult;
    fn switch_left(&self) -> ExecResult;
    fn switch_right(&self) -> ExecResult;
    fn close_current(&self) -> ExecResult;
    fn go_to(&self, desktop: u32) -> ExecResult;
}

pub(crate) struct UnsupportedVirtualDesktopBackend;
impl VirtualDesktopBackend for UnsupportedVirtualDesktopBackend {
    fn create(&self) -> ExecResult {
        self.unsupported(MkVirtualDesktopAction::Create)
    }
    fn switch_left(&self) -> ExecResult {
        self.unsupported(MkVirtualDesktopAction::SwitchLeft)
    }
    fn switch_right(&self) -> ExecResult {
        self.unsupported(MkVirtualDesktopAction::SwitchRight)
    }
    fn close_current(&self) -> ExecResult {
        self.unsupported(MkVirtualDesktopAction::CloseCurrent)
    }
    fn go_to(&self, desktop: u32) -> ExecResult {
        self.unsupported(MkVirtualDesktopAction::GoTo { desktop })
            .map_err(|e| e.context("desktop", desktop.to_string()))
    }
}
impl UnsupportedVirtualDesktopBackend {
    fn unsupported(&self, action: MkVirtualDesktopAction) -> ExecResult {
        Err(ExecutionDiagnostic::new(
            DiagnosticKind::UnsupportedOperation,
            "Virtual desktop automation is available only on Windows",
        )
        .context("backend", "virtual desktop")
        .context("action", format!("{action:?}")))
    }
}

#[cfg(windows)]
trait SharedVirtualDesktopService: Send + Sync {
    fn create(&self) -> Result<(), VirtualDesktopError>;
    fn switch_adjacent(&self, direction: AdjacentDirection) -> Result<(), VirtualDesktopError>;
    fn close_current(&self) -> Result<(), VirtualDesktopError>;
    fn go_to(&self, desktop: u32) -> Result<(), VirtualDesktopError>;
}

#[cfg(windows)]
struct SystemVirtualDesktopService;
#[cfg(windows)]
impl SharedVirtualDesktopService for SystemVirtualDesktopService {
    fn create(&self) -> Result<(), VirtualDesktopError> {
        VirtualDesktopService.create().map(|_| ())
    }
    fn switch_adjacent(&self, direction: AdjacentDirection) -> Result<(), VirtualDesktopError> {
        VirtualDesktopService.switch_adjacent(direction).map(|_| ())
    }
    fn close_current(&self) -> Result<(), VirtualDesktopError> {
        VirtualDesktopService.close_current()
    }
    fn go_to(&self, desktop: u32) -> Result<(), VirtualDesktopError> {
        VirtualDesktopService.switch(&VirtualDesktopSelector::Number(desktop))
    }
}

#[cfg(windows)]
pub(crate) struct WindowsVirtualDesktopBackend {
    service: Arc<dyn SharedVirtualDesktopService>,
}
#[cfg(windows)]
impl WindowsVirtualDesktopBackend {
    pub(crate) fn new() -> Self {
        Self {
            service: Arc::new(SystemVirtualDesktopService),
        }
    }
    #[cfg(test)]
    fn with_service(service: Arc<dyn SharedVirtualDesktopService>) -> Self {
        Self { service }
    }
    fn perform(
        &self,
        action: MkVirtualDesktopAction,
        operation: impl FnOnce(&dyn SharedVirtualDesktopService) -> Result<(), VirtualDesktopError>,
    ) -> ExecResult {
        operation(self.service.as_ref()).map_err(|error| map_error(error, action))
    }
}
#[cfg(windows)]
impl VirtualDesktopBackend for WindowsVirtualDesktopBackend {
    fn create(&self) -> ExecResult {
        self.perform(MkVirtualDesktopAction::Create, |s| s.create())
    }
    fn switch_left(&self) -> ExecResult {
        self.perform(MkVirtualDesktopAction::SwitchLeft, |s| {
            s.switch_adjacent(AdjacentDirection::Previous)
        })
    }
    fn switch_right(&self) -> ExecResult {
        self.perform(MkVirtualDesktopAction::SwitchRight, |s| {
            s.switch_adjacent(AdjacentDirection::Next)
        })
    }
    fn close_current(&self) -> ExecResult {
        self.perform(MkVirtualDesktopAction::CloseCurrent, |s| s.close_current())
    }
    fn go_to(&self, desktop: u32) -> ExecResult {
        self.perform(MkVirtualDesktopAction::GoTo { desktop }, |s| {
            s.go_to(desktop)
        })
        .map_err(|e| {
            e.context("desktop", desktop.to_string())
                .context("requested_desktop", desktop.to_string())
        })
    }
}

#[cfg(windows)]
fn map_error(error: VirtualDesktopError, action: MkVirtualDesktopAction) -> ExecutionDiagnostic {
    let kind = match error.kind {
        VirtualDesktopErrorKind::InvalidSelector => DiagnosticKind::InvalidSelection,
        VirtualDesktopErrorKind::NotFound | VirtualDesktopErrorKind::StaleBinding => {
            DiagnosticKind::TargetNotFound
        }
        VirtualDesktopErrorKind::AmbiguousSelector => DiagnosticKind::InvalidSelection,
        VirtualDesktopErrorKind::UnsupportedCapability => DiagnosticKind::UnsupportedOperation,
        VirtualDesktopErrorKind::InvalidWindow => DiagnosticKind::TargetNotFound,
        VirtualDesktopErrorKind::Native => DiagnosticKind::ComFailure,
    };
    let mut diagnostic = ExecutionDiagnostic::new(kind, error.message)
        .context("backend", "virtual desktop")
        .context("backend_operation", error.operation)
        .context("action", format!("{action:?}"));
    for (key, value) in error.context {
        diagnostic = diagnostic.context(key, value);
    }
    diagnostic
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsupported_go_to_includes_requested_desktop() {
        let error = UnsupportedVirtualDesktopBackend.go_to(7).unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::UnsupportedOperation);
        assert_eq!(
            error.context.get("action").map(String::as_str),
            Some("GoTo { desktop: 7 }")
        );
        assert_eq!(error.context.get("desktop").map(String::as_str), Some("7"));
    }

    #[cfg(windows)]
    mod windows_tests {
        use super::*;
        use std::sync::Mutex;
        #[derive(Default)]
        struct FakeService {
            calls: Mutex<Vec<String>>,
        }
        impl SharedVirtualDesktopService for FakeService {
            fn create(&self) -> Result<(), VirtualDesktopError> {
                self.calls.lock().unwrap().push("create".into());
                Ok(())
            }
            fn switch_adjacent(
                &self,
                direction: AdjacentDirection,
            ) -> Result<(), VirtualDesktopError> {
                self.calls
                    .lock()
                    .unwrap()
                    .push(format!("adjacent:{direction:?}"));
                Ok(())
            }
            fn close_current(&self) -> Result<(), VirtualDesktopError> {
                self.calls.lock().unwrap().push("close".into());
                Ok(())
            }
            fn go_to(&self, desktop: u32) -> Result<(), VirtualDesktopError> {
                self.calls.lock().unwrap().push(format!("go_to:{desktop}"));
                Ok(())
            }
        }
        #[test]
        fn production_adapter_routes_every_action_through_shared_service() {
            let service = Arc::new(FakeService::default());
            let backend = WindowsVirtualDesktopBackend::with_service(service.clone());
            backend.create().unwrap();
            backend.switch_left().unwrap();
            backend.switch_right().unwrap();
            backend.close_current().unwrap();
            backend.go_to(3).unwrap();
            assert_eq!(
                *service.calls.lock().unwrap(),
                [
                    "create",
                    "adjacent:Previous",
                    "adjacent:Next",
                    "close",
                    "go_to:3"
                ]
            );
        }
    }
}
