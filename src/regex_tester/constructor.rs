//! Shared, source-faithful regex constructor edits.
use std::ops::Range;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Constructor {
    EscapeSelection,
    DigitClass,
    WordClass,
    WhitespaceClass,
    Dot,
    StartAnchor,
    EndAnchor,
    Group,
    Alternation,
    OneOrMore,
    ZeroOrMore,
}
pub const ALL: [(Constructor, &str); 11] = [
    (Constructor::EscapeSelection, "Escape selection"),
    (Constructor::DigitClass, "\\d"),
    (Constructor::WordClass, "\\w"),
    (Constructor::WhitespaceClass, "\\s"),
    (Constructor::Dot, "."),
    (Constructor::StartAnchor, "^"),
    (Constructor::EndAnchor, "$"),
    (Constructor::Group, "Group"),
    (Constructor::Alternation, "|"),
    (Constructor::OneOrMore, "+"),
    (Constructor::ZeroOrMore, "*"),
];
/// Returns the complete new pattern and resulting UTF-8 caret range.
pub fn replacement(
    source: &str,
    range: Range<usize>,
    constructor: Constructor,
) -> Option<(String, Range<usize>)> {
    let selected = source.get(range.clone())?;
    let (inserted, caret) = match constructor {
        Constructor::EscapeSelection => (regex::escape(selected), None),
        Constructor::DigitClass => (r"\d".to_owned(), None),
        Constructor::WordClass => (r"\w".to_owned(), None),
        Constructor::WhitespaceClass => (r"\s".to_owned(), None),
        Constructor::Dot => (".".to_owned(), None),
        Constructor::StartAnchor => ("^".to_owned(), None),
        Constructor::EndAnchor => ("$".to_owned(), None),
        Constructor::Alternation => ("|".to_owned(), None),
        Constructor::Group => {
            if selected.is_empty() {
                ("(?:)".to_owned(), Some(3))
            } else {
                (format!("(?:{selected})"), None)
            }
        }
        Constructor::OneOrMore | Constructor::ZeroOrMore => {
            let suffix = if constructor == Constructor::OneOrMore {
                "+"
            } else {
                "*"
            };
            if selected.is_empty() {
                (format!("(?:){suffix}"), Some(3))
            } else {
                (format!("(?:{selected}){suffix}"), None)
            }
        }
    };
    let mut result = String::with_capacity(source.len() + inserted.len());
    result.push_str(&source[..range.start]);
    result.push_str(&inserted);
    result.push_str(&source[range.end..]);
    let caret = range.start + caret.unwrap_or(inserted.len());
    Some((result, caret..caret))
}
#[cfg(test)]
mod tests {
    use super::{Constructor, replacement};
    #[test]
    fn unicode_and_invalid_ranges_are_source_faithful() {
        assert_eq!(
            replacement("é+", 0..3, Constructor::Group),
            Some(("(?:é+)".into(), 7..7))
        );
        assert_eq!(replacement("é", 1..2, Constructor::Group), None);
        assert_eq!(
            replacement("", 0..0, Constructor::ZeroOrMore),
            Some(("(?:)*".into(), 3..3))
        );
    }
}
