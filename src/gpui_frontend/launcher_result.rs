//! Display text policy for ordinary launcher results.
pub(super) fn list_description(description: &str, full_path: bool) -> String {
    if full_path {
        description.to_owned()
    } else {
        description
            .split_whitespace()
            .take(7)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::list_description;

    #[test]
    fn list_description_compacts_words_and_preserves_requested_full_path() {
        let text = "one  two\tthree four five six seven eight nine";
        assert_eq!(
            list_description(text, false),
            "one two three four five six seven"
        );
        assert_eq!(list_description(text, true), text);
        assert_eq!(list_description("", false), "");
    }
}
