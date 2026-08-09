//! Background comparison job primitives. Actual jobs are introduced separately.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiffProgress {
    pub completed: u64,
    pub total: Option<u64>,
}
