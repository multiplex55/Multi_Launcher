use rdev::{listen, EventType, Key};
use std::sync::{Arc, Mutex};
use std::thread;

// Shared signal to open launcher
pub struct HotkeyTrigger {
    pub open: Arc<Mutex<bool>>,
    pub key: Key,
}

impl HotkeyTrigger {
    pub fn new(key: Key) -> Self {
        Self {
            open: Arc::new(Mutex::new(false)),
            key,
        }
    }

    pub fn start_listener(&self) {
        let open = self.open.clone();
        let watch = self.key;
        thread::spawn(move || {
            listen(move |event| {
                if let EventType::KeyPress(k) = event.event_type {
                    if k == watch {
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
