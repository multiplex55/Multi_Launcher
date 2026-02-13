#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawLifecycle {
    Idle,
    Starting,
    Active,
    Exiting,
    Restoring,
}

impl DrawLifecycle {
    pub fn is_active(self) -> bool {
        !matches!(self, Self::Idle)
    }
}

pub fn can_transition(from: DrawLifecycle, to: DrawLifecycle) -> bool {
    matches!(
        (from, to),
        (DrawLifecycle::Idle, DrawLifecycle::Starting)
            | (DrawLifecycle::Starting, DrawLifecycle::Active)
            | (DrawLifecycle::Starting, DrawLifecycle::Restoring)
            | (DrawLifecycle::Active, DrawLifecycle::Exiting)
            | (DrawLifecycle::Exiting, DrawLifecycle::Restoring)
            | (DrawLifecycle::Restoring, DrawLifecycle::Idle)
            | (DrawLifecycle::Starting, DrawLifecycle::Idle)
    ) || from == to
}
