use crate::multi_manager::win::{self, CaptureKeyAction, CapturedWindow};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_millis(12);
const VK_ENTER: u32 = 0x0D;
const VK_ESCAPE: u32 = 0x1B;
const VK_S: u32 = 0x53;

#[derive(Debug, Clone)]
pub struct CaptureEvent {
    pub action: CaptureKeyAction,
    pub captured: Option<CapturedWindow>,
}

pub struct CaptureSession {
    pub rx: mpsc::Receiver<CaptureEvent>,
    cancel: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CaptureKeySnapshot {
    pub enter: bool,
    pub escape: bool,
    pub s: bool,
}

impl CaptureKeySnapshot {
    fn any_down(self) -> bool {
        self.enter || self.escape || self.s
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureKeyEdgeDetector {
    armed: bool,
    previous: CaptureKeySnapshot,
}

impl CaptureKeyEdgeDetector {
    pub fn new(initial: CaptureKeySnapshot) -> Self {
        Self {
            armed: !initial.any_down(),
            previous: initial,
        }
    }

    pub fn update(&mut self, current: CaptureKeySnapshot) -> Option<CaptureKeyAction> {
        if !self.armed {
            self.previous = current;
            if !current.any_down() {
                self.armed = true;
            }
            return None;
        }

        let action = if current.enter && !self.previous.enter {
            Some(CaptureKeyAction::Confirm)
        } else if current.escape && !self.previous.escape {
            Some(CaptureKeyAction::Cancel)
        } else if current.s && !self.previous.s {
            Some(CaptureKeyAction::Skip)
        } else {
            None
        };
        self.previous = current;
        action
    }
}

pub fn start_capture_session(ctx: egui::Context) -> CaptureSession {
    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let thread_cancel = Arc::clone(&cancel);
    let join = thread::spawn(move || {
        let mut detector = CaptureKeyEdgeDetector::new(current_snapshot());
        while !thread_cancel.load(Ordering::Relaxed) {
            thread::sleep(POLL_INTERVAL);
            if let Some(action) = detector.update(current_snapshot()) {
                let captured = (action == CaptureKeyAction::Confirm)
                    .then(win::active_window)
                    .flatten();
                let _ = tx.send(CaptureEvent { action, captured });
                ctx.request_repaint();
                break;
            }
        }
    });

    CaptureSession {
        rx,
        cancel,
        join: Some(join),
    }
}

fn current_snapshot() -> CaptureKeySnapshot {
    CaptureKeySnapshot {
        enter: win::capture_key_is_down(VK_ENTER),
        escape: win::capture_key_is_down(VK_ESCAPE),
        s: win::capture_key_is_down(VK_S),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(enter: bool, escape: bool, s: bool) -> CaptureKeySnapshot {
        CaptureKeySnapshot { enter, escape, s }
    }

    #[test]
    fn enter_down_once_produces_exactly_one_confirm_event() {
        let mut detector = CaptureKeyEdgeDetector::new(snap(false, false, false));
        assert_eq!(
            detector.update(snap(true, false, false)),
            Some(CaptureKeyAction::Confirm)
        );
        assert_eq!(detector.update(snap(true, false, false)), None);
        assert_eq!(detector.update(snap(false, false, false)), None);
    }

    #[test]
    fn holding_enter_does_not_repeat_confirm_events() {
        let mut detector = CaptureKeyEdgeDetector::new(snap(false, false, false));
        assert_eq!(
            detector.update(snap(true, false, false)),
            Some(CaptureKeyAction::Confirm)
        );
        for _ in 0..5 {
            assert_eq!(detector.update(snap(true, false, false)), None);
        }
    }

    #[test]
    fn escape_produces_cancel() {
        let mut detector = CaptureKeyEdgeDetector::new(snap(false, false, false));
        assert_eq!(
            detector.update(snap(false, true, false)),
            Some(CaptureKeyAction::Cancel)
        );
    }

    #[test]
    fn s_produces_skip() {
        let mut detector = CaptureKeyEdgeDetector::new(snap(false, false, false));
        assert_eq!(
            detector.update(snap(false, false, true)),
            Some(CaptureKeyAction::Skip)
        );
    }

    #[test]
    fn keys_held_at_session_start_are_ignored_until_released() {
        let mut detector = CaptureKeyEdgeDetector::new(snap(true, false, false));
        assert_eq!(detector.update(snap(true, false, false)), None);
        assert_eq!(detector.update(snap(false, false, false)), None);
        assert_eq!(
            detector.update(snap(true, false, false)),
            Some(CaptureKeyAction::Confirm)
        );
    }
}
