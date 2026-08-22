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
        self.perform(MkVirtualDesktopAction::Create)
    }
    fn switch_left(&self) -> ExecResult {
        self.perform(MkVirtualDesktopAction::SwitchLeft)
    }
    fn switch_right(&self) -> ExecResult {
        self.perform(MkVirtualDesktopAction::SwitchRight)
    }
    fn close_current(&self) -> ExecResult {
        self.perform(MkVirtualDesktopAction::CloseCurrent)
    }
}
#[cfg(windows)]
impl WindowsVirtualDesktopBackend {
    fn perform(&self, action: MkVirtualDesktopAction) -> ExecResult {
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
pub fn shortcut(action: MkVirtualDesktopAction) -> [MkKey; 3] {
    use MkVirtualDesktopAction::*;
    let terminal = match action {
        Create => MkKey::Character("D".into()),
        SwitchLeft => MkKey::Left,
        SwitchRight => MkKey::Right,
        CloseCurrent => MkKey::Function(4),
    };
    [MkKey::Meta, MkKey::Control, terminal]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_all_shell_shortcuts_exactly() {
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
}
