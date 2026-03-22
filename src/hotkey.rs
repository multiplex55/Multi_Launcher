pub mod parse;
pub mod runtime;

pub use parse::{parse_hotkey, EventType, Hotkey, Key};
pub use runtime::{process_test_events, HotkeyListener, HotkeyTrigger};
