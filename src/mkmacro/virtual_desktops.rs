//! Semantic Windows virtual-desktop shortcuts.
use super::{
    MkKey, MkVirtualDesktopAction,
    executor::{
        DiagnosticKind, ExecResult, ExecutionDiagnostic, InputBackend, VirtualDesktopBackend,
    },
};
use std::sync::Arc;

/// Returns the exact native shell chord for an operation, modifiers first.
pub fn shortcut(action: MkVirtualDesktopAction) -> [MkKey; 3] {
    use MkVirtualDesktopAction::*;
    let key = match action {
        Create => MkKey::Character("D".into()),
        SwitchLeft => MkKey::Left,
        SwitchRight => MkKey::Right,
        CloseCurrent => MkKey::Function(4),
    };
    [MkKey::Meta, MkKey::Control, key]
}

/// Executes a chord atomically and always releases every key whose press succeeded.
pub fn send_shortcut(input: &dyn InputBackend, action: MkVirtualDesktopAction) -> ExecResult {
    let keys = shortcut(action);
    let mut pressed = Vec::new();
    let mut result = Ok(());
    for key in &keys {
        if let Err(error) = input.key_down(key) {
            result = Err(error);
            break;
        }
        pressed.push(key);
    }
    for key in pressed.into_iter().rev() {
        if let Err(error) = input.key_up(key) {
            if result.is_ok() {
                result = Err(error);
            }
        }
    }
    result.map_err(|error| {
        ExecutionDiagnostic::new(
            error.kind,
            format!("Virtual desktop {:?} failed: {error}", action),
        )
        .context("backend", "virtual desktop")
        .context("action", format!("{action:?}"))
    })
}

pub struct ShortcutVirtualDesktopBackend {
    input: Arc<dyn InputBackend>,
}
impl ShortcutVirtualDesktopBackend {
    pub fn new(input: Arc<dyn InputBackend>) -> Self {
        Self { input }
    }
}
impl VirtualDesktopBackend for ShortcutVirtualDesktopBackend {
    fn execute(&self, action: MkVirtualDesktopAction) -> ExecResult {
        #[cfg(windows)]
        {
            send_shortcut(self.input.as_ref(), action)
        }
        #[cfg(not(windows))]
        {
            let _ = (&self.input, action);
            Err(ExecutionDiagnostic::new(
                DiagnosticKind::UnsupportedOperation,
                "Virtual Desktop automation is available only on Windows",
            )
            .context("backend", "virtual desktop"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Sink {
        events: Mutex<Vec<String>>,
        fail_on: Mutex<Option<String>>,
    }
    impl InputBackend for Sink {
        fn key_down(&self, key: &MkKey) -> ExecResult {
            self.record(format!("down:{key:?}"))
        }
        fn key_up(&self, key: &MkKey) -> ExecResult {
            self.record(format!("up:{key:?}"))
        }
        fn button_down(&self, _: super::super::MkMouseButton) -> ExecResult {
            unreachable!()
        }
        fn button_up(&self, _: super::super::MkMouseButton) -> ExecResult {
            unreachable!()
        }
        fn move_mouse(&self, _: super::super::MkPoint) -> ExecResult {
            unreachable!()
        }
        fn cursor_position(&self) -> ExecResult<super::super::MkPoint> {
            unreachable!()
        }
        fn scroll(&self, _: i32) -> ExecResult {
            unreachable!()
        }
        fn text(&self, _: &super::super::MkTextPayload) -> ExecResult {
            unreachable!()
        }
    }
    impl Sink {
        fn record(&self, event: String) -> ExecResult {
            self.events.lock().unwrap().push(event.clone());
            if self.fail_on.lock().unwrap().as_deref() == Some(&event) {
                Err(ExecutionDiagnostic::new(
                    DiagnosticKind::InputRejected,
                    "injected",
                ))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn maps_all_shell_shortcuts() {
        assert_eq!(
            shortcut(MkVirtualDesktopAction::Create),
            [MkKey::Meta, MkKey::Control, MkKey::Character("D".into())]
        );
        assert_eq!(
            shortcut(MkVirtualDesktopAction::SwitchLeft),
            [MkKey::Meta, MkKey::Control, MkKey::Left]
        );
        assert_eq!(
            shortcut(MkVirtualDesktopAction::SwitchRight),
            [MkKey::Meta, MkKey::Control, MkKey::Right]
        );
        assert_eq!(
            shortcut(MkVirtualDesktopAction::CloseCurrent),
            [MkKey::Meta, MkKey::Control, MkKey::Function(4)]
        );
    }

    #[test]
    fn releases_modifiers_after_success_and_failure() {
        let success = Sink::default();
        send_shortcut(&success, MkVirtualDesktopAction::Create).unwrap();
        assert!(
            success
                .events
                .lock()
                .unwrap()
                .ends_with(&["up:Control".into(), "up:Meta".into()])
        );

        let failure = Sink::default();
        *failure.fail_on.lock().unwrap() = Some("down:Character(\"D\")".into());
        assert!(send_shortcut(&failure, MkVirtualDesktopAction::Create).is_err());
        assert!(
            failure
                .events
                .lock()
                .unwrap()
                .ends_with(&["up:Control".into(), "up:Meta".into()])
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn platform_backend_reports_windows_only() {
        let backend = ShortcutVirtualDesktopBackend::new(Arc::new(Sink::default()));
        let error = backend.execute(MkVirtualDesktopAction::Create).unwrap_err();
        assert_eq!(
            error.message,
            "Virtual Desktop automation is available only on Windows"
        );
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "manual: create; test left/right boundaries; close with many and one desktop; verify modifiers are released"]
    fn manual_windows_virtual_desktop_smoke_checklist() {}
}
