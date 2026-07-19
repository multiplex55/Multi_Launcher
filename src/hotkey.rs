pub mod parse;
pub mod runtime;

pub use parse::{EventType, Hotkey, Key, parse_hotkey};
pub use runtime::{HotkeyListener, HotkeyTrigger, process_test_events};
