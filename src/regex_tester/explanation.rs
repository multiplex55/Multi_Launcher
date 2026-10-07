//! Deterministic syntax explanations with spans in the original pattern.
//!
//! Descriptions identify syntax rather than assuming effective inline flags.
//! The Rust regex compiler remains authoritative for accepted patterns.

use regex_syntax::ast::{
    self, Ast, ClassSet, ClassSetItem, GroupKind, RepetitionKind, RepetitionRange,
};

use super::engine::compile_regex;
use super::model::{ByteSpan, RegexFlags, RegexValidationError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplanationKind {
    Literal,
    Dot,
    Assertion,
    PerlClass,
    UnicodeClass,
    AsciiClass,
    CharacterClass,
    CharacterRange,
    ClassSetOperation,
    CapturingGroup,
    NamedGroup,
    NonCapturingGroup,
    Flags,
    Alternation,
    Repetition,
}

/// One syntax construct. Parent and child spans can overlap intentionally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Explanation {
    pub span: ByteSpan,
    pub token: String,
    pub kind: ExplanationKind,
    pub label: &'static str,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExplanationResult {
    Success { explanations: Vec<Explanation> },
    InvalidPattern(RegexValidationError),
}

/// Explains supported syntax locally, or returns compiler/parser validation.
/// Invalid patterns never return partial explanations.
pub fn explain(pattern: &str, flags: &RegexFlags) -> ExplanationResult {
    if let Err(error) = compile_regex(pattern, flags) {
        return ExplanationResult::InvalidPattern(error);
    }
    let parsed = ast::parse::ParserBuilder::new()
        .ignore_whitespace(flags.ignore_whitespace)
        .build()
        .parse(pattern);
    let parsed = match parsed {
        Ok(parsed) => parsed,
        Err(error) => {
            return ExplanationResult::InvalidPattern(RegexValidationError {
                message: error.to_string(),
            });
        }
    };
    let mut collector = Collector {
        pattern,
        explanations: Vec::new(),
    };
    collector.visit(&parsed);
    ExplanationResult::Success {
        explanations: collector.explanations,
    }
}

struct Collector<'a> {
    pattern: &'a str,
    explanations: Vec<Explanation>,
}

impl Collector<'_> {
    fn push(
        &mut self,
        span: &ast::Span,
        kind: ExplanationKind,
        label: &'static str,
        description: impl Into<String>,
    ) {
        // Parser spans refer to the original UTF-8 pattern. Safe slicing also
        // ensures that an unexpected span cannot fabricate a token or panic.
        let Some(token) = self.pattern.get(span.start.offset..span.end.offset) else {
            return;
        };
        let Some(span) = ByteSpan::new(span.start.offset, span.end.offset) else {
            return;
        };
        self.explanations.push(Explanation {
            span,
            token: token.to_owned(),
            kind,
            label,
            description: description.into(),
        });
    }

    fn visit(&mut self, node: &Ast) {
        use ExplanationKind as Kind;
        match node {
            Ast::Empty(_) => {}
            Ast::Flags(flags) => self.push(
                &flags.span,
                Kind::Flags,
                "Inline flags",
                "Changes flags from this point within the enclosing group; a minus sign disables the following flags.",
            ),
            Ast::Literal(literal) => self.literal(literal),
            Ast::Dot(span) => self.push(
                span,
                Kind::Dot,
                "Dot",
                "Dot character-class syntax; its newline behavior depends on the active s flag.",
            ),
            Ast::Assertion(assertion) => {
                let description = match assertion.kind {
                    ast::AssertionKind::StartLine => {
                        "Start anchor; behavior depends on active m and R flags."
                    }
                    ast::AssertionKind::EndLine => {
                        "End anchor; behavior depends on active m and R flags."
                    }
                    ast::AssertionKind::StartText => "Start of the input text.",
                    ast::AssertionKind::EndText => "End of the input text.",
                    _ => "Zero-width word-boundary assertion; its form is shown in the token and word characters depend on the active u flag.",
                };
                self.push(&assertion.span, Kind::Assertion, "Assertion", description);
            }
            Ast::ClassPerl(class) => self.perl_class(class),
            Ast::ClassUnicode(class) => self.unicode_class(class),
            Ast::ClassBracketed(class) => self.bracketed(class),
            Ast::Repetition(repetition) => {
                self.visit(&repetition.ast);
                let count = match &repetition.op.kind {
                    RepetitionKind::ZeroOrOne => "zero or one time".to_owned(),
                    RepetitionKind::ZeroOrMore => "zero or more times".to_owned(),
                    RepetitionKind::OneOrMore => "one or more times".to_owned(),
                    RepetitionKind::Range(RepetitionRange::Exactly(n)) => {
                        format!("exactly {n} times")
                    }
                    RepetitionKind::Range(RepetitionRange::AtLeast(n)) => {
                        format!("at least {n} times")
                    }
                    RepetitionKind::Range(RepetitionRange::Bounded(min, max)) => {
                        format!("from {min} to {max} times")
                    }
                };
                self.push(
                    &repetition.op.span,
                    Kind::Repetition,
                    "Repetition",
                    format!("Repeats the preceding expression {count}. A trailing ? changes preference; the active U flag can invert greediness."),
                );
            }
            Ast::Group(group) => {
                match &group.kind {
                    GroupKind::CaptureIndex(index) => self.push(
                        &group.span,
                        Kind::CapturingGroup,
                        "Capturing group",
                        format!("Groups an expression and records capture {index}."),
                    ),
                    GroupKind::CaptureName { name, .. } => self.push(
                        &group.span,
                        Kind::NamedGroup,
                        "Named capturing group",
                        format!("Groups an expression and records capture {} named {:?}.", name.index, name.name),
                    ),
                    GroupKind::NonCapturing(flags) => {
                        self.push(
                            &group.span,
                            Kind::NonCapturingGroup,
                            "Non-capturing group",
                            "Groups an expression without recording a capture.",
                        );
                        if !flags.items.is_empty() {
                            self.push(
                                &flags.span,
                                Kind::Flags,
                                "Scoped flags",
                                "Changes flags only within this group; a minus sign disables the following flags.",
                            );
                        }
                    }
                }
                self.visit(&group.ast);
            }
            Ast::Alternation(alternation) => {
                self.push(
                    &alternation.span,
                    Kind::Alternation,
                    "Alternation",
                    format!("Chooses between {} alternative expressions separated by |.", alternation.asts.len()),
                );
                for alternative in &alternation.asts {
                    self.visit(alternative);
                }
            }
            Ast::Concat(concat) => {
                for child in &concat.asts {
                    self.visit(child);
                }
            }
        }
    }

    fn literal(&mut self, literal: &ast::Literal) {
        self.push(
            &literal.span,
            ExplanationKind::Literal,
            "Literal",
            format!(
                "Literal syntax for character {:?}; case matching depends on the active i flag.",
                literal.c
            ),
        );
    }

    fn perl_class(&mut self, class: &ast::ClassPerl) {
        let name = match class.kind {
            ast::ClassPerlKind::Digit => "digit",
            ast::ClassPerlKind::Space => "whitespace",
            ast::ClassPerlKind::Word => "word-character",
        };
        let negation = if class.negated { "Negated" } else { "Positive" };
        self.push(
            &class.span,
            ExplanationKind::PerlClass,
            "Character class",
            format!(
                "{negation} {name} class syntax; its character set depends on the active u flag."
            ),
        );
    }

    fn unicode_class(&mut self, class: &ast::ClassUnicode) {
        self.push(
            &class.span,
            ExplanationKind::UnicodeClass,
            "Unicode property",
            "Unicode property class syntax, with the property and any negation shown in the token.",
        );
    }

    fn bracketed(&mut self, class: &ast::ClassBracketed) {
        let description = if class.negated {
            "Negated bracketed character-set expression."
        } else {
            "Bracketed character-set expression."
        };
        self.push(
            &class.span,
            ExplanationKind::CharacterClass,
            "Character set",
            description,
        );
        self.class_set(&class.kind);
    }

    fn class_set(&mut self, set: &ClassSet) {
        match set {
            ClassSet::Item(item) => self.class_item(item),
            ClassSet::BinaryOp(operation) => {
                let description = match operation.kind {
                    ast::ClassSetBinaryOpKind::Intersection => {
                        "Intersection of the two character sets (&&)."
                    }
                    ast::ClassSetBinaryOpKind::Difference => {
                        "Difference of the two character sets (--)."
                    }
                    ast::ClassSetBinaryOpKind::SymmetricDifference => {
                        "Characters belonging to one of the two sets, but not both (~~)."
                    }
                };
                self.push(
                    &operation.span,
                    ExplanationKind::ClassSetOperation,
                    "Set operation",
                    description,
                );
                self.class_set(&operation.lhs);
                self.class_set(&operation.rhs);
            }
        }
    }

    fn class_item(&mut self, item: &ClassSetItem) {
        match item {
            ClassSetItem::Empty(_) => {},
            ClassSetItem::Literal(literal) => self.literal(literal),
            ClassSetItem::Range(range) => self.push(&range.span, ExplanationKind::CharacterRange, "Character range", format!("Character-range syntax from {:?} through {:?}, inclusive; case matching depends on the active i flag.", range.start.c, range.end.c)),
            ClassSetItem::Ascii(class) => self.push(&class.span, ExplanationKind::AsciiClass, "ASCII class", "ASCII character-class syntax, with its category and any negation shown in the token."),
            ClassSetItem::Unicode(class) => self.unicode_class(class),
            ClassSetItem::Perl(class) => self.perl_class(class),
            ClassSetItem::Bracketed(class) => self.bracketed(class),
            ClassSetItem::Union(union) => { for item in &union.items { self.class_item(item); } },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(pattern: &str, flags: &RegexFlags) -> Vec<Explanation> {
        match explain(pattern, flags) {
            ExplanationResult::Success { explanations } => {
                for entry in &explanations {
                    assert_eq!(
                        pattern.get(entry.span.as_range()),
                        Some(entry.token.as_str())
                    );
                }
                explanations
            }
            ExplanationResult::InvalidPattern(error) => {
                panic!("unexpected invalid pattern: {}", error.message)
            }
        }
    }

    #[test]
    fn explains_common_constructs_with_original_spans() {
        let pattern = r"^é(?P<word>\w+)(?:\d{2,5}|[a-z\s])\p{Greek}$";
        let entries = entries(pattern, &RegexFlags::default());
        for kind in [
            ExplanationKind::Assertion,
            ExplanationKind::Literal,
            ExplanationKind::NamedGroup,
            ExplanationKind::PerlClass,
            ExplanationKind::NonCapturingGroup,
            ExplanationKind::Alternation,
            ExplanationKind::Repetition,
            ExplanationKind::CharacterClass,
            ExplanationKind::CharacterRange,
            ExplanationKind::UnicodeClass,
        ] {
            assert!(
                entries.iter().any(|entry| entry.kind == kind),
                "missing {kind:?}"
            );
        }
        let literal = entries.iter().find(|entry| entry.token == "é").unwrap();
        assert_eq!(literal.span, ByteSpan::new(1, 3).unwrap());
        let repeat = entries.iter().find(|entry| entry.token == "{2,5}").unwrap();
        assert!(repeat.description.contains("from 2 to 5 times"));
    }

    #[test]
    fn honors_external_extended_mode_and_retains_original_offsets() {
        let pattern = "é # comment\n [ a-z ] +";
        let flags = RegexFlags {
            ignore_whitespace: true,
            ..RegexFlags::default()
        };
        let entries = entries(pattern, &flags);
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.kind == ExplanationKind::Literal)
                .count(),
            1
        );
        assert!(entries.iter().any(|entry| entry.token == "a-z"));
        assert!(entries.iter().any(|entry| entry.token == "+"));
        assert!(!entries.iter().any(|entry| entry.token.contains("comment")));
    }

    #[test]
    fn describes_scoped_unicode_flags_without_claiming_ascii_or_unicode_semantics() {
        let entries = entries(r"(?-u:\w)(?u:\w)", &RegexFlags::default());
        let classes = entries
            .iter()
            .filter(|entry| entry.kind == ExplanationKind::PerlClass)
            .collect::<Vec<_>>();
        assert_eq!(classes.len(), 2);
        assert_eq!(classes[0].description, classes[1].description);
        assert!(classes[0].description.contains("active u flag"));
        assert!(
            entries
                .iter()
                .any(|entry| entry.kind == ExplanationKind::Flags && entry.token == "-u")
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.kind == ExplanationKind::Flags && entry.token == "u")
        );
    }

    #[test]
    fn describes_u_r_and_nested_groups_conservatively() {
        let entries = entries(r"(?U)(?mR:(a+?)(?:b*))^.$", &RegexFlags::default());
        assert!(
            entries
                .iter()
                .any(|entry| entry.kind == ExplanationKind::CapturingGroup
                    && entry.description.contains("capture 1"))
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.token == "(?U)" && entry.kind == ExplanationKind::Flags)
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.token == "mR" && entry.kind == ExplanationKind::Flags)
        );
        let repeat = entries.iter().find(|entry| entry.token == "+?").unwrap();
        assert!(repeat.description.contains("U flag can invert"));
        for entry in entries
            .iter()
            .filter(|entry| entry.token == "^" || entry.token == "$")
        {
            assert!(entry.description.contains("active m and R"));
        }
        assert!(
            entries
                .iter()
                .find(|entry| entry.token == ".")
                .unwrap()
                .description
                .contains("active s")
        );
    }

    #[test]
    fn explains_nested_character_sets_and_repetition_ranges() {
        let entries = entries(
            r"[a-z&&[^aeiou]][[:digit:]]{3}(x){2,}y?z*",
            &RegexFlags::default(),
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.kind == ExplanationKind::ClassSetOperation)
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.kind == ExplanationKind::AsciiClass)
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.description.contains("exactly 3 times"))
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.description.contains("at least 2 times"))
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.description.contains("zero or one time"))
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.description.contains("zero or more times"))
        );
    }

    #[test]
    fn invalid_or_unsupported_patterns_return_only_validation() {
        for pattern in ["[", "(?=a)", r"(a)\1", r"\p{NotAProperty}"] {
            match explain(pattern, &RegexFlags::default()) {
                ExplanationResult::InvalidPattern(error) => assert!(!error.message.is_empty()),
                ExplanationResult::Success { .. } => {
                    panic!("expected compiler rejection: {pattern}")
                }
            }
        }
        assert_eq!(
            explain("", &RegexFlags::default()),
            ExplanationResult::Success {
                explanations: vec![]
            }
        );
    }
}
