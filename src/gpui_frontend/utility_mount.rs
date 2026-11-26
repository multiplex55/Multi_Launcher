//! Checked ownership for one replaceable retained utility mount.
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum UtilityKind {
    Json,
    Xml,
    Regex,
    ClipboardHistory,
    ClipboardModify,
    FileSearch,
}

pub(super) struct UtilityPopup {
    pub window: super::AnyWindowHandle,
    pub native: Option<super::NativeWindow>,
    pub _subscription: super::Subscription,
    pub geometry: Option<super::utility_host::ShellGeometry>,
}

pub(super) fn transfer_source_is_owned<T: PartialEq>(
    transferring: bool,
    tracked: Option<T>,
    local: T,
) -> bool {
    transferring && tracked == Some(local)
}

#[derive(Clone, Default)]
pub(super) struct MountOwner(Arc<AtomicU64>);
#[derive(Clone)]
pub(super) struct MountReceipt {
    owner: MountOwner,
    generation: u64,
}
impl MountOwner {
    pub fn attach(&self) -> MountReceipt {
        let generation = self.0.fetch_add(1, Ordering::AcqRel).wrapping_add(1).max(1);
        MountReceipt {
            owner: self.clone(),
            generation,
        }
    }
    pub fn retire(&self) {
        self.0.fetch_add(1, Ordering::AcqRel);
    }
}
impl MountReceipt {
    pub fn matches(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner.0, &other.owner.0) && self.generation == other.generation
    }
    pub fn is_current(&self) -> bool {
        self.owner.0.load(Ordering::Acquire) == self.generation
    }
}
#[cfg(test)]
mod tests {
    use super::{MountOwner, transfer_source_is_owned};
    #[test]
    fn source_close_is_vetoed_only_for_exact_transfer_window() {
        assert!(transfer_source_is_owned(true, Some(1), 1));
        assert!(!transfer_source_is_owned(true, Some(1), 2));
        assert!(!transfer_source_is_owned(false, Some(1), 1));
        assert!(!transfer_source_is_owned(true, None, 1));
    }
    #[test]
    fn replacement_and_retirement_revoke_exact_mount() {
        let owner = MountOwner::default();
        let first = owner.attach();
        assert!(first.is_current());
        let second = owner.attach();
        assert!(!first.is_current());
        assert!(second.is_current());
        assert!(!first.matches(&second));
        assert!(second.matches(&second.clone()));
        let unrelated = MountOwner::default().attach();
        assert!(!second.matches(&unrelated));
        owner.retire();
        assert!(!second.is_current());
    }
}
