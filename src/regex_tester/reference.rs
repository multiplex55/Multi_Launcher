//! Bundled Quick Reference for the Rust `regex` flavor.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReferenceCategory {
    CharacterClasses,
    Anchors,
    Quantifiers,
    Groups,
    Alternation,
    Escaping,
    Unicode,
    CommonPatterns,
}

impl ReferenceCategory {
    pub const ALL: [Self; 8] = [
        Self::CharacterClasses,
        Self::Anchors,
        Self::Quantifiers,
        Self::Groups,
        Self::Alternation,
        Self::Escaping,
        Self::Unicode,
        Self::CommonPatterns,
    ];

    pub const fn title(self) -> &'static str {
        match self {
            Self::CharacterClasses => "Character Classes",
            Self::Anchors => "Anchors",
            Self::Quantifiers => "Quantifiers",
            Self::Groups => "Groups",
            Self::Alternation => "Alternation",
            Self::Escaping => "Escaping",
            Self::Unicode => "Unicode",
            Self::CommonPatterns => "Common Patterns",
        }
    }
}

/// Indicates whether the syntax can be compiled by itself or needs context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceSyntaxKind {
    Pattern,
    Fragment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReferenceExample {
    pub pattern: &'static str,
    pub text: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReferenceEntry {
    /// Stable identifier, suitable for widget identity independently of search.
    pub id: &'static str,
    pub category: ReferenceCategory,
    pub title: &'static str,
    pub syntax: &'static str,
    pub syntax_kind: ReferenceSyntaxKind,
    pub description: &'static str,
    pub example: Option<ReferenceExample>,
}

/// Filters the static catalog in its display order. An empty query lists all
/// entries in the requested category; searches are case-insensitive substring
/// matches over titles, syntax, descriptions, and category names.
pub fn search_reference(
    query: &str,
    category: Option<ReferenceCategory>,
) -> impl Iterator<Item = &'static ReferenceEntry> {
    let query = query.trim().to_lowercase();
    QUICK_REFERENCE.iter().filter(move |entry| {
        category.is_none_or(|category| entry.category == category)
            && (query.is_empty()
                || [
                    entry.title,
                    entry.syntax,
                    entry.description,
                    entry.category.title(),
                ]
                .iter()
                .any(|field| field.to_lowercase().contains(&query)))
    })
}

use ReferenceCategory as Category;
use ReferenceSyntaxKind::{Fragment, Pattern};

pub static QUICK_REFERENCE: &[ReferenceEntry] = &[
    ReferenceEntry {
        id: "class-digit",
        category: Category::CharacterClasses,
        title: "Digit",
        syntax: r"\d",
        syntax_kind: Pattern,
        description: "A Unicode decimal digit by default. With u disabled, an ASCII digit (0–9). Use \\D for its complement.",
        example: Some(ReferenceExample {
            pattern: r"\d+",
            text: "item 42",
        }),
    },
    ReferenceEntry {
        id: "class-whitespace",
        category: Category::CharacterClasses,
        title: "Whitespace",
        syntax: r"\s",
        syntax_kind: Pattern,
        description: "A Unicode whitespace character by default; u disabled selects ASCII whitespace. Use \\S for its complement.",
        example: Some(ReferenceExample {
            pattern: r"\s+",
            text: "one  two",
        }),
    },
    ReferenceEntry {
        id: "class-word",
        category: Category::CharacterClasses,
        title: "Word character",
        syntax: r"\w",
        syntax_kind: Pattern,
        description: "Unicode letters, marks, decimal digits, connector punctuation and join controls by default. With u disabled: ASCII letters, digits and underscore. Use \\W for the complement.",
        example: Some(ReferenceExample {
            pattern: r"\w+",
            text: "café_42",
        }),
    },
    ReferenceEntry {
        id: "class-dot",
        category: Category::CharacterClasses,
        title: "Dot",
        syntax: ".",
        syntax_kind: Pattern,
        description: "Any character except newline by default. Enable s to include newline. In Rust string regexes, matches stay on UTF-8 boundaries.",
        example: Some(ReferenceExample {
            pattern: "a.b",
            text: "a-b",
        }),
    },
    ReferenceEntry {
        id: "class-range",
        category: Category::CharacterClasses,
        title: "Bracketed range",
        syntax: "[a-z]",
        syntax_kind: Pattern,
        description: "A character from the inclusive range a through z. The i flag also enables case-insensitive matching.",
        example: Some(ReferenceExample {
            pattern: "[a-z]+",
            text: "abc 123",
        }),
    },
    ReferenceEntry {
        id: "class-negated",
        category: Category::CharacterClasses,
        title: "Negated character set",
        syntax: "[^0-9]",
        syntax_kind: Pattern,
        description: "A character outside the bracketed set. A leading ^ inside brackets negates the set.",
        example: None,
    },
    ReferenceEntry {
        id: "anchor-start",
        category: Category::Anchors,
        title: "Start anchor",
        syntax: "^",
        syntax_kind: Pattern,
        description: "Start of input by default; with m enabled, also start of a line. R changes line handling to recognize CRLF as one line ending.",
        example: Some(ReferenceExample {
            pattern: "^hello",
            text: "hello world",
        }),
    },
    ReferenceEntry {
        id: "anchor-end",
        category: Category::Anchors,
        title: "End anchor",
        syntax: "$",
        syntax_kind: Pattern,
        description: "End of input by default; with m enabled, also end of a line. R changes line handling to recognize CRLF as one line ending.",
        example: Some(ReferenceExample {
            pattern: "world$",
            text: "hello world",
        }),
    },
    ReferenceEntry {
        id: "anchor-input",
        category: Category::Anchors,
        title: "Whole input anchors",
        syntax: r"\A[0-9]+\z",
        syntax_kind: Pattern,
        description: "\\A is the start and \\z the end of input regardless of m. This pattern requires the entire input to consist of ASCII digits.",
        example: Some(ReferenceExample {
            pattern: r"\A\d+\z",
            text: "42",
        }),
    },
    ReferenceEntry {
        id: "anchor-word-boundary",
        category: Category::Anchors,
        title: "Word boundary",
        syntax: r"\b",
        syntax_kind: Pattern,
        description: "A zero-width boundary between a word character and a non-word character, including input edges. Word characters depend on u. \\B is its negation.",
        example: Some(ReferenceExample {
            pattern: r"\bcat\b",
            text: "cat scatter",
        }),
    },
    ReferenceEntry {
        id: "quantifier-star",
        category: Category::Quantifiers,
        title: "Zero or more",
        syntax: "*",
        syntax_kind: Fragment,
        description: "Append to an expression to repeat it zero or more times. Greedy by default; U inverts greediness.",
        example: Some(ReferenceExample {
            pattern: "ab*",
            text: "a ab abbb",
        }),
    },
    ReferenceEntry {
        id: "quantifier-plus",
        category: Category::Quantifiers,
        title: "One or more",
        syntax: "+",
        syntax_kind: Fragment,
        description: "Append to an expression to repeat it one or more times. Greedy by default; U inverts greediness.",
        example: Some(ReferenceExample {
            pattern: r"\d+",
            text: "42",
        }),
    },
    ReferenceEntry {
        id: "quantifier-optional",
        category: Category::Quantifiers,
        title: "Optional",
        syntax: "?",
        syntax_kind: Fragment,
        description: "Append to an expression to allow zero or one occurrence. After another quantifier, ? instead flips its greediness.",
        example: Some(ReferenceExample {
            pattern: "colou?r",
            text: "color colour",
        }),
    },
    ReferenceEntry {
        id: "quantifier-bounded",
        category: Category::Quantifiers,
        title: "Repetition range",
        syntax: "{2,5}",
        syntax_kind: Fragment,
        description: "Append to an expression to repeat it from two to five times. {2} means exactly two; {2,} means at least two.",
        example: Some(ReferenceExample {
            pattern: r"\d{2,5}",
            text: "1234",
        }),
    },
    ReferenceEntry {
        id: "quantifier-lazy",
        category: Category::Quantifiers,
        title: "Lazy quantifier syntax",
        syntax: "+?",
        syntax_kind: Fragment,
        description: "Append to an expression for one or more repetitions, preferring fewer by default. U reverses that preference.",
        example: Some(ReferenceExample {
            pattern: "a+?",
            text: "aaa",
        }),
    },
    ReferenceEntry {
        id: "group-capture",
        category: Category::Groups,
        title: "Capturing group",
        syntax: r"(\w+)",
        syntax_kind: Pattern,
        description: "Groups an expression and records its matched text in a numbered capture. Capture numbering begins at one; capture zero is the full match.",
        example: None,
    },
    ReferenceEntry {
        id: "group-named",
        category: Category::Groups,
        title: "Named capturing group",
        syntax: r"(?P<word>\w+)",
        syntax_kind: Pattern,
        description: "Records a capture with a name as well as a number. (?<word>\\w+) is also supported. Replacement strings can refer to it as $word or ${word}.",
        example: None,
    },
    ReferenceEntry {
        id: "group-noncapturing",
        category: Category::Groups,
        title: "Non-capturing group",
        syntax: "(?:a|b)",
        syntax_kind: Pattern,
        description: "Groups an expression without recording a capture. Useful when applying a quantifier to an alternative expression.",
        example: Some(ReferenceExample {
            pattern: "(?:ab)+",
            text: "abab",
        }),
    },
    ReferenceEntry {
        id: "group-flags",
        category: Category::Groups,
        title: "Scoped flags",
        syntax: "(?i:abc)",
        syntax_kind: Pattern,
        description: "Enables i only inside the group. A minus disables flags, such as (?-i:abc). Bare (?i) applies from that point within the enclosing group. Supported flags include i, m, s, u, x, U and R.",
        example: Some(ReferenceExample {
            pattern: "(?i:abc)",
            text: "ABC",
        }),
    },
    ReferenceEntry {
        id: "alternation",
        category: Category::Alternation,
        title: "Alternative expressions",
        syntax: "cat|dog",
        syntax_kind: Pattern,
        description: "Matches either alternative. Alternatives are tried in their written order; group them when combining with other expressions.",
        example: Some(ReferenceExample {
            pattern: "cat|dog",
            text: "cat and dog",
        }),
    },
    ReferenceEntry {
        id: "escape-metacharacter",
        category: Category::Escaping,
        title: "Literal metacharacter",
        syntax: r"\.",
        syntax_kind: Pattern,
        description: "Escape regex punctuation to match it literally, such as \\. for a dot or \\+ for a plus. The tester accepts regex text directly, without Rust source-string escaping.",
        example: Some(ReferenceExample {
            pattern: r"a\.b",
            text: "a.b",
        }),
    },
    ReferenceEntry {
        id: "escape-newline",
        category: Category::Escaping,
        title: "Escaped control character",
        syntax: r"\n",
        syntax_kind: Pattern,
        description: "A line-feed character. \\r is carriage return and \\t is tab.",
        example: Some(ReferenceExample {
            pattern: r"\n",
            text: "first\nsecond",
        }),
    },
    ReferenceEntry {
        id: "unicode-property",
        category: Category::Unicode,
        title: "Unicode property",
        syntax: r"\p{Greek}",
        syntax_kind: Pattern,
        description: "A character with the Greek script property. Rust regex supports Unicode categories, scripts and selected binary properties; Unicode mode must be enabled.",
        example: Some(ReferenceExample {
            pattern: r"\p{Greek}+",
            text: "αβ text",
        }),
    },
    ReferenceEntry {
        id: "unicode-negated",
        category: Category::Unicode,
        title: "Negated Unicode property",
        syntax: r"\P{Letter}",
        syntax_kind: Pattern,
        description: "A character outside the Unicode Letter category. Uppercase P negates the property class.",
        example: None,
    },
    ReferenceEntry {
        id: "common-ascii-integer",
        category: Category::CommonPatterns,
        title: "ASCII integer",
        syntax: r"[+-]?[0-9]+",
        syntax_kind: Pattern,
        description: "An optional sign followed by ASCII digits. This finds integer-like substrings; wrap in \\A and \\z to test the entire input.",
        example: Some(ReferenceExample {
            pattern: r"[+-]?[0-9]+",
            text: "count=-42",
        }),
    },
    ReferenceEntry {
        id: "common-whitespace-run",
        category: Category::CommonPatterns,
        title: "Whitespace run",
        syntax: r"\s+",
        syntax_kind: Pattern,
        description: "One or more whitespace characters, including line endings by default. In substitution, replacing with one space also flattens line breaks.",
        example: Some(ReferenceExample {
            pattern: r"\s+",
            text: "one  two\nthree",
        }),
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regex_tester::{RegexFlags, engine::compile_regex};
    use std::collections::HashSet;

    #[test]
    fn categories_are_populated_and_identifiers_are_unique() {
        let mut ids = HashSet::new();
        for entry in QUICK_REFERENCE {
            assert!(ids.insert(entry.id), "duplicate id {}", entry.id);
            assert!(!entry.title.is_empty());
            assert!(!entry.syntax.is_empty());
            assert!(!entry.description.is_empty());
        }
        for category in ReferenceCategory::ALL {
            assert!(
                search_reference("", Some(category)).next().is_some(),
                "empty {category:?}"
            );
        }
    }

    #[test]
    fn required_core_syntax_and_context_labels_exist() {
        for syntax in [
            r"\d",
            r"\s",
            r"\w",
            "[a-z]",
            "^",
            "$",
            "*",
            "+",
            "?",
            "{2,5}",
            r"(\w+)",
            r"(?P<word>\w+)",
            "(?:a|b)",
            "cat|dog",
            r"\.",
            r"\p{Greek}",
        ] {
            assert!(
                QUICK_REFERENCE.iter().any(|entry| entry.syntax == syntax),
                "missing {syntax}"
            );
        }
        for entry in search_reference("", Some(Category::Quantifiers)) {
            assert_eq!(entry.syntax_kind, Fragment);
            assert!(entry.description.contains("Append") || entry.description.contains("append"));
        }
    }

    #[test]
    fn searches_titles_syntax_descriptions_and_categories_case_insensitively() {
        assert!(search_reference(" NAMED CAPTURING ", None).any(|entry| entry.id == "group-named"));
        assert!(search_reference(r"\p{greek}", None).any(|entry| entry.id == "unicode-property"));
        assert!(
            search_reference("FLATTENS", None).any(|entry| entry.id == "common-whitespace-run")
        );
        assert_eq!(search_reference("", None).count(), QUICK_REFERENCE.len());
        assert_eq!(search_reference("no-such-reference", None).count(), 0);
        assert_eq!(search_reference("digit", Some(Category::Groups)).count(), 0);
        assert!(
            search_reference("quantifiers", None)
                .all(|entry| entry.category == Category::Quantifiers)
        );
    }

    #[test]
    fn standalone_patterns_and_tiny_examples_use_supported_rust_regex() {
        let flags = RegexFlags::default();
        for entry in QUICK_REFERENCE {
            if entry.syntax_kind == Pattern {
                compile_regex(entry.syntax, &flags)
                    .unwrap_or_else(|error| panic!("{}: {}", entry.id, error.message));
            }
            if let Some(example) = entry.example {
                let regex = compile_regex(example.pattern, &flags).unwrap();
                assert!(
                    regex.is_match(example.text),
                    "example does not match: {}",
                    entry.id
                );
            }
        }
    }
}
