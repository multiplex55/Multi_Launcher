//! One-shot ownership for an explicit XML overwrite review.
#[derive(Default)]
pub(super) struct ReviewPresentation {
    next: u64,
    current: Option<(u64, u64)>,
}
impl ReviewPresentation {
    pub fn issue(&mut self, review: u64) -> Option<u64> {
        let next = self.next.checked_add(1)?;
        self.next = next;
        self.current = Some((review, next));
        Some(next)
    }
    pub fn owns_review(&self, review: u64) -> bool {
        self.current.is_some_and(|(id, _)| id == review)
    }
    pub fn revoke(&mut self) {
        self.current = None;
    }
    pub fn is_current(&self, review: u64, presentation: u64) -> bool {
        self.current == Some((review, presentation))
    }
}
pub(super) fn take_review<T>(review: &mut Option<(u64, T)>, id: u64) -> Option<T> {
    if review.as_ref().is_some_and(|(current, _)| *current == id) {
        review.take().map(|(_, value)| value)
    } else {
        None
    }
}
pub(super) fn review_description(path: &std::path::Path) -> Option<String> {
    path.to_str()
        .map(|path| format!("Replace the existing file {path}?"))
}
#[cfg(test)]
mod tests {
    use super::{ReviewPresentation, review_description, take_review};
    #[test]
    fn review_presentation_revokes_old_modal_even_for_same_review() {
        let mut authority = ReviewPresentation::default();
        let old = authority.issue(17).unwrap();
        authority.revoke();
        assert!(!authority.is_current(17, old));
        let current = authority.issue(17).unwrap();
        assert!(!authority.is_current(17, old));
        assert!(authority.is_current(17, current));
        assert!(!authority.is_current(18, current));
        authority.revoke();
        assert!(!authority.is_current(17, current));
    }
    #[test]
    fn review_contains_complete_unclipped_target() {
        let target = format!("C:/{}end.xml", "long-segment/".repeat(60));
        let text = review_description(std::path::Path::new(&target)).unwrap();
        assert!(target.len() > 512);
        assert_eq!(text, format!("Replace the existing file {target}?"));
    }
    #[test]
    fn overwrite_review_is_exact_one_shot_and_keeps_frozen_payload() {
        let mut editor = String::from("original");
        let mut review = Some((17, editor.clone()));
        editor.push_str(" changed");
        assert!(take_review(&mut review, 18).is_none());
        assert_eq!(take_review(&mut review, 17).as_deref(), Some("original"));
        assert!(take_review(&mut review, 17).is_none());
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReviewAction {
    Cancel,
    Replace,
    Transfer,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReviewKeyDecision {
    Pass,
    Consume(Option<ReviewAction>),
}
pub(super) fn review_key_decision(
    key: u16,
    unmodified: bool,
    held: bool,
    cancel: bool,
    replace: bool,
    transfer: bool,
) -> ReviewKeyDecision {
    if !unmodified {
        return ReviewKeyDecision::Pass;
    }
    match key {
        0x20 if held => ReviewKeyDecision::Consume(None),
        0x0d => ReviewKeyDecision::Consume(if held {
            None
        } else if cancel {
            Some(ReviewAction::Cancel)
        } else if replace {
            Some(ReviewAction::Replace)
        } else if transfer {
            Some(ReviewAction::Transfer)
        } else {
            None
        }),
        _ => ReviewKeyDecision::Pass,
    }
}
#[cfg(test)]
mod jump_tests {
    use super::{
        ReviewAction,
        ReviewKeyDecision::{Consume, Pass},
        review_key_decision as decide,
    };
    #[test]
    fn review_keys_require_fresh_enter_and_only_fresh_space_passes() {
        assert_eq!(
            decide(0x0d, true, false, false, false, false),
            Consume(None)
        );
        assert_eq!(
            decide(0x0d, true, false, true, false, false),
            Consume(Some(ReviewAction::Cancel))
        );
        assert_eq!(
            decide(0x0d, true, false, false, true, false),
            Consume(Some(ReviewAction::Replace))
        );
        assert_eq!(
            decide(0x0d, true, false, false, false, true),
            Consume(Some(ReviewAction::Transfer))
        );
        assert_eq!(decide(0x0d, true, true, false, true, false), Consume(None));
        assert_eq!(decide(0x20, true, true, false, true, false), Consume(None));
        assert_eq!(decide(0x20, true, true, false, false, false), Consume(None));
        assert_eq!(decide(0x20, true, false, true, false, false), Pass);
        assert_eq!(decide(0x20, true, false, false, false, true), Pass);
        assert_eq!(decide(0x20, false, true, false, true, false), Pass);
    }
}
