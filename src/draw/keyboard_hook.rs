#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    U,
    R,
    Escape,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyModifiers {
    pub ctrl: bool,
    pub shift: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: KeyCode,
    pub modifiers: KeyModifiers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCommand {
    Undo,
    Redo,
    RequestExit,
}

pub fn should_consume_key_event(active: bool, _event: KeyEvent) -> bool {
    active
}

pub fn map_key_event_to_command(active: bool, event: KeyEvent) -> Option<KeyCommand> {
    if !should_consume_key_event(active, event) {
        return None;
    }

    match (event.key, event.modifiers) {
        (KeyCode::Escape, _) => Some(KeyCommand::RequestExit),
        (KeyCode::U, KeyModifiers { ctrl: false, .. }) => Some(KeyCommand::Undo),
        (KeyCode::R, KeyModifiers { ctrl: true, .. }) => Some(KeyCommand::Redo),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_u_to_undo() {
        assert_eq!(
            map_key_event_to_command(
                true,
                KeyEvent {
                    key: KeyCode::U,
                    modifiers: KeyModifiers::default(),
                },
            ),
            Some(KeyCommand::Undo)
        );
    }

    #[test]
    fn maps_ctrl_r_to_redo() {
        assert_eq!(
            map_key_event_to_command(
                true,
                KeyEvent {
                    key: KeyCode::R,
                    modifiers: KeyModifiers {
                        ctrl: true,
                        shift: false,
                    },
                },
            ),
            Some(KeyCommand::Redo)
        );
    }

    #[test]
    fn maps_escape_to_exit_request() {
        assert_eq!(
            map_key_event_to_command(
                true,
                KeyEvent {
                    key: KeyCode::Escape,
                    modifiers: KeyModifiers::default(),
                },
            ),
            Some(KeyCommand::RequestExit)
        );
    }

    #[test]
    fn does_not_map_non_matching_keys_or_inactive_state() {
        assert_eq!(
            map_key_event_to_command(
                true,
                KeyEvent {
                    key: KeyCode::Other,
                    modifiers: KeyModifiers::default(),
                },
            ),
            None
        );
        assert_eq!(
            map_key_event_to_command(
                false,
                KeyEvent {
                    key: KeyCode::Escape,
                    modifiers: KeyModifiers::default(),
                },
            ),
            None
        );
    }
}
