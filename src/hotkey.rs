use rdev::{listen, EventType, Key};
use std::sync::{Arc, Mutex};
use std::thread;

// Shared signal to open launcher
pub struct HotkeyTrigger {
    pub open: Arc<Mutex<bool>>,
}

impl HotkeyTrigger {
    pub fn new() -> Self {
        Self {
            open: Arc::new(Mutex::new(false)),
        }
    }

    pub fn start_listener(&self) {
        let open = self.open.clone();
        thread::spawn(move || {
            listen(move |event| {
                if let EventType::KeyPress(key) = event.event_type {
                    if key == Key::CapsLock {
                        if let Ok(mut flag) = open.lock() {
                            *flag = true;
                        }
                    }
                }
            })
            .unwrap();
        });
    }

    pub fn take(&self) -> bool {
        let mut open = self.open.lock().unwrap();
        if *open {
            *open = false;
            true
        } else {
            false
        }
    }
}
