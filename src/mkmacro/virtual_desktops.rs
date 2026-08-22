//! Semantic Windows virtual-desktop shortcuts.
use super::{MkKey, MkVirtualDesktopAction};

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
