//! One-shot ownership for an explicit XML overwrite review.
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
    use super::{review_description, take_review};
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
