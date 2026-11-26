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
