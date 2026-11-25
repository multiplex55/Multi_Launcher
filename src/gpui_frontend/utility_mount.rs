//! Checked ownership for one replaceable retained utility mount.
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

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
    pub fn is_current(&self) -> bool {
        self.owner.0.load(Ordering::Acquire) == self.generation
    }
}
#[cfg(test)]
mod tests {
    use super::MountOwner;
    #[test]
    fn replacement_and_retirement_revoke_exact_mount() {
        let owner = MountOwner::default();
        let first = owner.attach();
        assert!(first.is_current());
        let second = owner.attach();
        assert!(!first.is_current());
        assert!(second.is_current());
        owner.retire();
        assert!(!second.is_current());
    }
}
