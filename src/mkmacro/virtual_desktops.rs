//! Semantic Windows virtual-desktop shortcuts.
use super::{
    DiagnosticKind, ExecResult, ExecutionDiagnostic, InputBackend, MkKey, MkVirtualDesktopAction,
};
use std::sync::Arc;

use crate::window_manager::virtual_desktop_selection::{
    DesktopSelectionError, select_virtual_desktop_index,
};

/// A native desktop-list session. Indices are zero-based and refer to the same
/// enumeration for the lifetime of this session. Creation is deliberately absent.
/// Sessions need not be Send/Sync: COM objects stay on the calling thread.
pub trait NativeVirtualDesktopBackend {
    fn desktop_count(&self) -> ExecResult<u32>;
    fn is_current(&self, index: u32) -> ExecResult<bool>;
    fn switch_to(&self, index: u32) -> ExecResult;
}

/// Go to an existing desktop using the one-based public numbering model.
/// Open the native session only after validation, so zero never touches COM.
pub fn go_to_with_native<B: NativeVirtualDesktopBackend>(
    desktop: u32,
    open: impl FnOnce() -> ExecResult<B>,
) -> ExecResult {
    let mut desktop_count = None;
    let result = (|| {
        if desktop == 0 {
            return Err(selection_failure(DesktopSelectionError::Zero));
        }
        let native = open()?;
        let count = native.desktop_count()?;
        desktop_count = Some(count);
        let index = select_virtual_desktop_index(desktop, count).map_err(selection_failure)?;
        if native.is_current(index)? {
            return Ok(());
        }
        native.switch_to(index)
    })();
    result.map_err(|error| {
        let mut error = error
            .context("backend", "virtual desktop")
            .context("backend_operation", "virtual desktop")
            .context(
                "action",
                format!("{:?}", MkVirtualDesktopAction::GoTo { desktop }),
            )
            .context("desktop", desktop.to_string())
            .context("requested_desktop", desktop.to_string());
        if let Some(count) = desktop_count {
            error = error.context("desktop_count", count.to_string());
        }
        error
    })
}

fn selection_failure(error: DesktopSelectionError) -> ExecutionDiagnostic {
    match error {
        DesktopSelectionError::Zero => ExecutionDiagnostic::new(
            DiagnosticKind::InvalidSelection,
            "Virtual desktop number must be at least 1",
        ),
        DesktopSelectionError::BeyondCount { requested, .. } => ExecutionDiagnostic::new(
            DiagnosticKind::TargetNotFound,
            format!("Virtual desktop {requested} does not exist"),
        ),
    }
}

/// Injectable boundary for semantic virtual-desktop operations.
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
            .map_err(|error| error.context("desktop", desktop.to_string()))
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
pub(crate) struct WindowsVirtualDesktopBackend(pub Arc<dyn InputBackend>);
#[cfg(windows)]
impl VirtualDesktopBackend for WindowsVirtualDesktopBackend {
    fn create(&self) -> ExecResult {
        self.perform(ShortcutAction::Create)
    }
    fn switch_left(&self) -> ExecResult {
        self.perform(ShortcutAction::SwitchLeft)
    }
    fn switch_right(&self) -> ExecResult {
        self.perform(ShortcutAction::SwitchRight)
    }
    fn close_current(&self) -> ExecResult {
        self.perform(ShortcutAction::CloseCurrent)
    }
    fn go_to(&self, desktop: u32) -> ExecResult {
        crate::window_manager::switch_virtual_desktop_by_number(desktop)
    }
}
#[cfg(windows)]
impl WindowsVirtualDesktopBackend {
    fn perform(&self, action: ShortcutAction) -> ExecResult {
        let keys = shortcut(action);
        let mut pressed = Vec::new();
        for key in &keys {
            if let Err(error) = self.0.key_down(key) {
                for key in pressed.iter().rev() {
                    let _ = self.0.key_up(key);
                }
                return Err(error);
            }
            pressed.push(key.clone());
        }
        let mut result = Ok(());
        for key in pressed.into_iter().rev() {
            if let Err(error) = self.0.key_up(&key) {
                if result.is_ok() {
                    result = Err(error);
                }
            }
        }
        result
    }
}

/// Returns the documented Windows shell chord for an operation, modifiers first.
#[derive(Debug, Clone, Copy)]
enum ShortcutAction {
    Create,
    SwitchLeft,
    SwitchRight,
    CloseCurrent,
}

fn shortcut(action: ShortcutAction) -> [MkKey; 3] {
    let terminal = match action {
        ShortcutAction::Create => MkKey::Character("D".into()),
        ShortcutAction::SwitchLeft => MkKey::Left,
        ShortcutAction::SwitchRight => MkKey::Right,
        ShortcutAction::CloseCurrent => MkKey::Function(4),
    };
    [MkKey::Meta, MkKey::Control, terminal]
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Mutex;

    struct FakeNativeBackend {
        count: u32,
        current_index: u32,
        fail_at: Option<&'static str>,
        calls: Mutex<Vec<String>>,
    }

    impl FakeNativeBackend {
        fn new(count: u32, current_index: u32) -> Self {
            Self {
                count,
                current_index,
                fail_at: None,
                calls: Mutex::new(Vec::new()),
            }
        }

        fn record(&self, operation: &str) -> ExecResult {
            self.calls.lock().unwrap().push(operation.to_string());
            if self.fail_at == Some(operation) {
                return Err(
                    ExecutionDiagnostic::new(DiagnosticKind::ComFailure, "native failure")
                        .context("operation", operation)
                        .context("hresult", "0x80004005")
                        .context("com_error", "fake COM error"),
                );
            }
            Ok(())
        }

        fn go_to(&self, desktop: u32) -> ExecResult {
            go_to_with_native(desktop, || {
                self.record("open")?;
                Ok(self)
            })
        }

        fn assert_calls(&self, expected: &[&str]) {
            let calls = self.calls.lock().unwrap();
            assert_eq!(*calls, expected);
            // Go To's boundary has no Create capability. Also check the complete
            // event trace on successes and failures to catch any added fallback.
            assert!(!calls.iter().any(|call| call.contains("Create")));
        }
    }

    impl NativeVirtualDesktopBackend for &FakeNativeBackend {
        fn desktop_count(&self) -> ExecResult<u32> {
            self.record("count")?;
            Ok(self.count)
        }

        fn is_current(&self, index: u32) -> ExecResult<bool> {
            self.record(&format!("is_current:{index}"))?;
            Ok(index == self.current_index)
        }

        fn switch_to(&self, index: u32) -> ExecResult {
            self.record(&format!("switch:{index}"))
        }
    }

    fn assert_go_to_context(error: &ExecutionDiagnostic, requested: u32, count: Option<u32>) {
        assert_eq!(
            error.context.get("requested_desktop"),
            Some(&requested.to_string())
        );
        assert_eq!(error.context.get("desktop"), Some(&requested.to_string()));
        assert_eq!(
            error.context.get("desktop_count"),
            count.map(|c| c.to_string()).as_ref()
        );
        assert_eq!(
            error.context.get("backend").map(String::as_str),
            Some("virtual desktop")
        );
        assert_eq!(
            error.context.get("backend_operation").map(String::as_str),
            Some("virtual desktop")
        );
        assert_eq!(
            error.context.get("action"),
            Some(&format!("GoTo {{ desktop: {requested} }}"))
        );
    }

    #[test]
    fn go_to_three_switches_directly_to_native_index_two() {
        let native = FakeNativeBackend::new(3, 0);
        native.go_to(3).unwrap();
        native.assert_calls(&["open", "count", "is_current:2", "switch:2"]);
    }

    #[test]
    fn go_to_current_desktop_succeeds_without_switching() {
        let native = FakeNativeBackend::new(3, 2);
        native.go_to(3).unwrap();
        native.assert_calls(&["open", "count", "is_current:2"]);
    }

    #[test]
    fn go_to_missing_desktop_returns_target_not_found_without_creating() {
        let native = FakeNativeBackend::new(2, 0);
        let error = native.go_to(3).unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::TargetNotFound);
        assert_eq!(error.message, "Virtual desktop 3 does not exist");
        assert_go_to_context(&error, 3, Some(2));
        native.assert_calls(&["open", "count"]);
    }

    #[test]
    fn go_to_zero_is_rejected_at_runtime_before_opening_native_backend() {
        let native = FakeNativeBackend::new(3, 0);
        let error = native.go_to(0).unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::InvalidSelection);
        assert_eq!(error.message, "Virtual desktop number must be at least 1");
        assert_go_to_context(&error, 0, None);
        native.assert_calls(&[]);
    }

    #[test]
    fn go_to_empty_desktop_list_returns_target_not_found() {
        let native = FakeNativeBackend::new(0, 0);
        let error = native.go_to(1).unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::TargetNotFound);
        assert_go_to_context(&error, 1, Some(0));
        native.assert_calls(&["open", "count"]);
    }

    #[test]
    fn native_open_count_current_and_switch_failures_retain_context() {
        let cases: &[(&str, Option<u32>, &[&str])] = &[
            ("open", None, &["open"]),
            ("count", None, &["open", "count"]),
            ("is_current:2", Some(3), &["open", "count", "is_current:2"]),
            (
                "switch:2",
                Some(3),
                &["open", "count", "is_current:2", "switch:2"],
            ),
        ];
        for &(operation, count, calls) in cases {
            let mut native = FakeNativeBackend::new(3, 0);
            native.fail_at = Some(operation);
            let error = native.go_to(3).unwrap_err();
            assert_eq!(error.kind, DiagnosticKind::ComFailure);
            assert_eq!(error.message, "native failure");
            assert_go_to_context(&error, 3, count);
            assert_eq!(
                error.context.get("operation").map(String::as_str),
                Some(operation)
            );
            assert_eq!(
                error.context.get("hresult").map(String::as_str),
                Some("0x80004005")
            );
            assert_eq!(
                error.context.get("com_error").map(String::as_str),
                Some("fake COM error")
            );
            native.assert_calls(calls);
        }
    }

    #[test]
    fn maps_all_shell_shortcuts_exactly() {
        assert_eq!(
            shortcut(ShortcutAction::Create),
            [MkKey::Meta, MkKey::Control, MkKey::Character("D".into())]
        );
        assert_eq!(
            shortcut(ShortcutAction::SwitchLeft),
            [MkKey::Meta, MkKey::Control, MkKey::Left]
        );
        assert_eq!(
            shortcut(ShortcutAction::SwitchRight),
            [MkKey::Meta, MkKey::Control, MkKey::Right]
        );
        assert_eq!(
            shortcut(ShortcutAction::CloseCurrent),
            [MkKey::Meta, MkKey::Control, MkKey::Function(4)]
        );
    }

    #[cfg(windows)]
    #[test]
    fn legacy_operations_emit_exact_chords() {
        let input = Arc::new(crate::mkmacro::executor::fake::FakeBackend::default());
        let backend = WindowsVirtualDesktopBackend(input.clone());

        backend.create().unwrap();
        backend.switch_left().unwrap();
        backend.switch_right().unwrap();
        backend.close_current().unwrap();

        assert_eq!(
            input.events(),
            vec![
                "key_down:Meta",
                "key_down:Control",
                "key_down:Character(\"D\")",
                "key_up:Character(\"D\")",
                "key_up:Control",
                "key_up:Meta",
                "key_down:Meta",
                "key_down:Control",
                "key_down:Left",
                "key_up:Left",
                "key_up:Control",
                "key_up:Meta",
                "key_down:Meta",
                "key_down:Control",
                "key_down:Right",
                "key_up:Right",
                "key_up:Control",
                "key_up:Meta",
                "key_down:Meta",
                "key_down:Control",
                "key_down:Function(4)",
                "key_up:Function(4)",
                "key_up:Control",
                "key_up:Meta",
            ]
        );
    }

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
}
