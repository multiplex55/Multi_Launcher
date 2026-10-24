use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) const DEFAULT_IMAGE_COMPARE_BUDGET_BYTES: usize = 512 * 1024 * 1024;

#[derive(Clone)]
pub(crate) struct ImageCompareBudget(Arc<BudgetState>);

struct BudgetState {
    limit: usize,
    used: AtomicUsize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ImageBudgetSnapshot {
    pub(crate) used_bytes: usize,
    pub(crate) limit_bytes: usize,
}

/// A charge follows the allocation it protects. Cloning an Arc-backed input
/// does not clone this lease; independent retained uploads reserve their own.
pub(crate) struct ImageBudgetLease {
    budget: Arc<BudgetState>,
    bytes: usize,
}

impl ImageCompareBudget {
    pub(crate) fn new() -> Self {
        Self::with_limit(DEFAULT_IMAGE_COMPARE_BUDGET_BYTES)
    }

    pub(crate) fn with_limit(limit_bytes: usize) -> Self {
        Self(Arc::new(BudgetState {
            limit: limit_bytes,
            used: AtomicUsize::new(0),
        }))
    }

    pub(crate) fn reserve(&self, bytes: usize) -> Result<ImageBudgetLease, BudgetExceeded> {
        self.0
            .used
            .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes).filter(|next| *next <= self.0.limit)
            })
            .map(|_| ImageBudgetLease {
                budget: Arc::clone(&self.0),
                bytes,
            })
            .map_err(|used| BudgetExceeded {
                requested_bytes: bytes,
                used_bytes: used,
                limit_bytes: self.0.limit,
            })
    }

    pub(crate) fn snapshot(&self) -> ImageBudgetSnapshot {
        ImageBudgetSnapshot {
            used_bytes: self.0.used.load(Ordering::Acquire),
            limit_bytes: self.0.limit,
        }
    }

    /// Reserve bytes for a future CPU/GPU representation. The caller must
    /// retain the returned value for as long as that copy remains live.
    pub(crate) fn reserve_retained_representation(
        &self,
        bytes: usize,
    ) -> Result<ImageBudgetLease, BudgetExceeded> {
        self.reserve(bytes)
    }
}

impl ImageBudgetLease {
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }

    /// Resize before allocating or releasing the corresponding buffer.
    pub(crate) fn resize(&mut self, bytes: usize) -> Result<(), BudgetExceeded> {
        if bytes > self.bytes {
            let additional = bytes - self.bytes;
            self.budget
                .used
                .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                    used.checked_add(additional)
                        .filter(|next| *next <= self.budget.limit)
                })
                .map_err(|used| BudgetExceeded {
                    requested_bytes: additional,
                    used_bytes: used,
                    limit_bytes: self.budget.limit,
                })?;
        } else if bytes < self.bytes {
            self.release(self.bytes - bytes);
        }
        self.bytes = bytes;
        Ok(())
    }

    fn release(&self, bytes: usize) {
        let result = self
            .budget
            .used
            .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_sub(bytes)
            });
        debug_assert!(result.is_ok(), "image comparison lease released once");
    }
}

impl Drop for ImageBudgetLease {
    fn drop(&mut self) {
        self.release(self.bytes);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BudgetExceeded {
    pub(crate) requested_bytes: usize,
    pub(crate) used_bytes: usize,
    pub(crate) limit_bytes: usize,
}

impl std::fmt::Display for BudgetExceeded {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "image comparison memory budget is full ({} of {} bytes already reserved; {} requested)",
            self.used_bytes, self.limit_bytes, self.requested_bytes
        )
    }
}

impl std::error::Error for BudgetExceeded {}

impl Default for ImageCompareBudget {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_compare_domain_budget_leases_track_clones_retained_copies_and_replacement_peak() {
        let budget = ImageCompareBudget::with_limit(32);
        let first = budget.reserve(20).unwrap();
        let input_clone = Arc::new(first);
        let clone = Arc::clone(&input_clone);
        assert_eq!(budget.snapshot().used_bytes, 20);
        drop(clone);
        let replacement = budget.reserve(12).unwrap();
        assert_eq!(budget.snapshot().used_bytes, 32);
        assert!(
            budget.reserve(1).is_err(),
            "old plus pending peak is charged"
        );
        drop(replacement);
        drop(input_clone);
        assert_eq!(budget.snapshot().used_bytes, 0);

        let retained = budget.reserve_retained_representation(24).unwrap();
        let mut pending = budget.reserve(4).unwrap();
        assert_eq!(budget.snapshot().used_bytes, 28);
        pending.resize(8).unwrap();
        assert_eq!(budget.snapshot().used_bytes, 32);
        drop(pending);
        assert_eq!(budget.snapshot().used_bytes, 24);
        drop(retained);
        assert_eq!(budget.snapshot().used_bytes, 0);
    }
}
