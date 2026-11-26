//! Authority captured at the Dock gesture and acknowledged by root presentation.
use super::*;

#[derive(Clone, Copy)]
pub(super) struct Gesture {
    pub source: NativeWindow,
    pub foreground_authorized: bool,
    pub visibility_revision: u64,
}

pub(super) struct Reveal {
    pub gesture: Gesture,
    pub mount_generation: u64,
    pub acknowledgement: Acknowledgement,
}

pub(super) struct UtilityReveal {
    pub gesture: Gesture,
    pub mount: super::utility_mount::MountReceipt,
    pub acknowledgement: UtilityAcknowledgement,
}

pub(super) struct UtilityAcknowledgement {
    pub signal: Arc<HashWakeSignal>,
    pub accepted_revision: Arc<AtomicU64>,
}
impl UtilityAcknowledgement {
    pub(super) fn accept(&self, revision: u64) { self.accepted_revision.store(revision, Ordering::Release); }
}
impl Drop for UtilityAcknowledgement {
    fn drop(&mut self) { self.signal.close(); }
}

pub(super) struct Acknowledgement(pub Arc<HashWakeSignal>);

impl Drop for Acknowledgement {
    fn drop(&mut self) {
        self.0.close();
    }
}

pub(super) fn may_activate(
    authorized: bool,
    source: ForegroundIdentity,
    foreground: Option<ForegroundIdentity>,
    current: bool,
) -> bool {
    authorized && current && foreground == Some(source)
}

#[cfg(test)]
mod tests {
    use super::super::ForegroundIdentity;
    use super::may_activate;

    #[test]
    fn dock_activation_requires_original_and_current_source_authority() {
        let source = ForegroundIdentity {
            hwnd: 1,
            process_id: 2,
            thread_id: 3,
        };
        let other = ForegroundIdentity { hwnd: 4, ..source };
        assert!(may_activate(true, source, Some(source), true));
        assert!(!may_activate(false, source, Some(source), true));
        assert!(!may_activate(true, source, Some(other), true));
        assert!(!may_activate(true, source, Some(source), false));
    }
}
