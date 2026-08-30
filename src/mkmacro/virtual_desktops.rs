//! Semantic Windows virtual-desktop shortcuts.
use super::{
    DiagnosticKind, ExecResult, ExecutionDiagnostic, InputBackend, MkKey, MkVirtualDesktopAction,
};
use std::sync::Arc;

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
        crate::window_manager::switch_to_virtual_desktop(desktop).map_err(|error| {
            ExecutionDiagnostic::new(
                DiagnosticKind::ComFailure,
                format!("Failed to switch to virtual desktop {desktop}: {error}"),
            )
            .context("backend", "virtual desktop")
            .context(
                "action",
                format!("{:?}", MkVirtualDesktopAction::GoTo { desktop }),
            )
            .context("desktop", desktop.to_string())
        })
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
