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

pub fn should_consume_key_event(active: bool, _event: KeyEvent) -> bool {
    active
}
