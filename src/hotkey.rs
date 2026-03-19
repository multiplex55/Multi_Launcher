pub mod parse;
pub mod runtime;

pub use parse::{parse_hotkey, EventType, Hotkey, Key};
pub use runtime::{HotkeyListener, HotkeyTrigger};
