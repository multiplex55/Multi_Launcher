//! Built-in learning examples evaluated entirely by the local Rust engine.

use super::model::{RegexDraft, RegexFlags};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegexExample {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub pattern: &'static str,
    pub flags: RegexFlags,
    pub sample_text: &'static str,
    pub replacement: Option<&'static str>,
}

impl RegexExample {
    /// Creates an independent editable draft for an explicit Load Example action.
    pub fn to_draft(self) -> RegexDraft {
        RegexDraft {
            pattern: self.pattern.to_owned(),
            flags: self.flags,
            test_text: self.sample_text.to_owned(),
            replacement: self.replacement.unwrap_or_default().to_owned(),
        }
    }
}

const UNICODE_FLAGS: RegexFlags = RegexFlags {
    case_insensitive: false,
    multi_line: false,
    dot_matches_new_line: false,
    unicode: true,
    ignore_whitespace: false,
};

pub static BUILT_IN_EXAMPLES: &[RegexExample] = &[
    RegexExample {
        id: "email-like",
        name: "Email-like text",
        description: "A simplified extractor for common ASCII email-like text. It is not a standards-complete email validator.",
        pattern: r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b",
        flags: UNICODE_FLAGS,
        sample_text: "Contact user@example.com or admin+alerts@example.org; not-an-email.",
        replacement: None,
    },
    RegexExample {
        id: "ipv4",
        name: "IPv4 address",
        description: "Finds four decimal octets from 0 to 255 without leading zeroes. Extracts address-like substrings; it does not validate an entire network configuration.",
        pattern: r"\b(?:(?:25[0-5]|2[0-4][0-9]|1[0-9]{2}|[1-9]?[0-9])\.){3}(?:25[0-5]|2[0-4][0-9]|1[0-9]{2}|[1-9]?[0-9])\b",
        flags: UNICODE_FLAGS,
        sample_text: "Loopback 127.0.0.1, gateway 192.168.1.1; invalid 999.1.2.3.",
        replacement: None,
    },
    RegexExample {
        id: "url",
        name: "HTTP(S)-like URL",
        description: "Finds HTTP or HTTPS followed by non-whitespace URL-like text. A simplified candidate extractor, not a URL validator; trailing punctuation may be included.",
        pattern: r"https?://[^\s<>]+",
        flags: RegexFlags {
            case_insensitive: true,
            ..UNICODE_FLAGS
        },
        sample_text: "Visit https://example.com/docs?q=rust and HTTP://localhost:8080/test",
        replacement: None,
    },
    RegexExample {
        id: "uuid",
        name: "UUID format",
        description: "Finds hexadecimal text in the 8-4-4-4-12 UUID format. Does not enforce a particular UUID version or variant.",
        pattern: r"\b[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}\b",
        flags: UNICODE_FLAGS,
        sample_text: "ID 550e8400-e29b-41d4-a716-446655440000; incomplete 550e8400-e29b.",
        replacement: None,
    },
    RegexExample {
        id: "date",
        name: "Date format (YYYY-MM-DD)",
        description: "Finds YYYY-MM-DD with months 01–12 and days 01–31. This checks format and simple ranges, not calendar validity or leap years.",
        pattern: r"\b[0-9]{4}-(?:0[1-9]|1[0-2])-(?:0[1-9]|[12][0-9]|3[01])\b",
        flags: UNICODE_FLAGS,
        sample_text: "Dates 2026-10-06 and 2024-02-29; bad month 2026-13-01.",
        replacement: None,
    },
    RegexExample {
        id: "time",
        name: "24-hour time format",
        description: "Finds HH:MM or HH:MM:SS with hours 00–23 and minutes/seconds 00–59. Does not parse time zones or leap seconds.",
        pattern: r"\b(?:[01][0-9]|2[0-3]):[0-5][0-9](?::[0-5][0-9])?\b",
        flags: UNICODE_FLAGS,
        sample_text: "Start 09:30, finish 23:59:58; invalid 24:00.",
        replacement: None,
    },
    RegexExample {
        id: "hex-color",
        name: "Hex color",
        description: "Finds #RGB or #RRGGBB hexadecimal color notation. Alpha-channel variants are outside this example.",
        pattern: r"#(?:[0-9A-Fa-f]{6}|[0-9A-Fa-f]{3})\b",
        flags: UNICODE_FLAGS,
        sample_text: "Colors #F80 and #12abEF; invalid #GGG.",
        replacement: None,
    },
    RegexExample {
        id: "integer",
        name: "ASCII integer",
        description: "Finds optionally signed ASCII integer-like substrings. To validate the whole input instead, wrap the expression in \\A and \\z.",
        pattern: r"[+-]?\b[0-9]+\b",
        flags: UNICODE_FLAGS,
        sample_text: "Counts -42, +7 and 0.",
        replacement: None,
    },
    RegexExample {
        id: "decimal",
        name: "Decimal number format",
        description: "Finds optionally signed decimals with ASCII digits on both sides of a dot. Does not cover exponent notation, decimal commas or leading-dot forms.",
        pattern: r"[+-]?\b[0-9]+\.[0-9]+\b",
        flags: UNICODE_FLAGS,
        sample_text: "Values -3.14, +0.50 and 12.0; integer 7.",
        replacement: None,
    },
    RegexExample {
        id: "whitespace-cleanup",
        name: "Horizontal whitespace cleanup",
        description: "Replaces each run of ASCII spaces or tabs with one space while preserving line endings.",
        pattern: r"[ \t]+",
        flags: UNICODE_FLAGS,
        sample_text: "one   two\tthree\nfour  five",
        replacement: Some(" "),
    },
    RegexExample {
        id: "file-extension",
        name: "File extension tokens",
        description: "Finds a dot followed by an alphanumeric extension, recording the extension in a named capture. Multiple dotted suffixes produce multiple tokens; this is not a full path parser.",
        pattern: r"\.(?P<extension>[A-Za-z0-9]+)\b",
        flags: UNICODE_FLAGS,
        sample_text: "report.pdf photo.PNG archive.tar.gz README",
        replacement: None,
    },
    RegexExample {
        id: "key-value",
        name: "Simple key=value",
        description: "Finds an ASCII identifier and a non-whitespace value separated by =. Named captures can be rearranged in the replacement; quoted values containing spaces are not parsed.",
        pattern: r"\b(?P<key>[A-Za-z_][A-Za-z0-9_]*)=(?P<value>[^\s=]+)",
        flags: UNICODE_FLAGS,
        sample_text: "user=alice count=42 debug=true",
        replacement: Some("${key}: ${value}"),
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regex_tester::{
        EvaluationResult, SubstitutionEvaluationResult, evaluate, evaluate_substitution,
    };
    use std::collections::HashSet;

    #[test]
    fn required_examples_have_unique_identifiers_and_editable_drafts() {
        let required = [
            "email-like",
            "ipv4",
            "url",
            "uuid",
            "date",
            "time",
            "hex-color",
            "integer",
            "decimal",
            "whitespace-cleanup",
            "file-extension",
            "key-value",
        ];
        let mut ids = HashSet::new();
        for example in BUILT_IN_EXAMPLES {
            assert!(ids.insert(example.id), "duplicate {}", example.id);
            assert!(!example.name.is_empty());
            assert!(!example.description.is_empty());
            let draft = example.to_draft();
            assert_eq!(draft.pattern, example.pattern);
            assert_eq!(draft.flags, example.flags);
            assert_eq!(draft.test_text, example.sample_text);
            assert_eq!(draft.replacement, example.replacement.unwrap_or_default());
        }
        for id in required {
            assert!(ids.contains(id), "missing {id}");
        }
    }

    #[test]
    fn every_example_compiles_and_produces_its_expected_sample_matches() {
        let expectations: &[(&str, &[&str])] = &[
            (
                "email-like",
                &["user@example.com", "admin+alerts@example.org"],
            ),
            ("ipv4", &["127.0.0.1", "192.168.1.1"]),
            (
                "url",
                &[
                    "https://example.com/docs?q=rust",
                    "HTTP://localhost:8080/test",
                ],
            ),
            ("uuid", &["550e8400-e29b-41d4-a716-446655440000"]),
            ("date", &["2026-10-06", "2024-02-29"]),
            ("time", &["09:30", "23:59:58"]),
            ("hex-color", &["#F80", "#12abEF"]),
            ("integer", &["-42", "+7", "0"]),
            ("decimal", &["-3.14", "+0.50", "12.0"]),
            ("whitespace-cleanup", &["   ", "\t", "  "]),
            ("file-extension", &[".pdf", ".PNG", ".tar", ".gz"]),
            ("key-value", &["user=alice", "count=42", "debug=true"]),
        ];
        assert_eq!(expectations.len(), BUILT_IN_EXAMPLES.len());
        for example in BUILT_IN_EXAMPLES {
            let expected = expectations
                .iter()
                .find(|(id, _)| *id == example.id)
                .unwrap()
                .1;
            match evaluate(example.pattern, &example.flags, example.sample_text) {
                EvaluationResult::Success {
                    matches,
                    completeness,
                } => {
                    assert_eq!(
                        completeness,
                        crate::regex_tester::MatchCompleteness::Complete
                    );
                    assert_eq!(
                        matches
                            .iter()
                            .map(|matched| matched.text.as_str())
                            .collect::<Vec<_>>(),
                        expected,
                        "{}",
                        example.id
                    );
                }
                EvaluationResult::InvalidPattern(error) => {
                    panic!("{}: {}", example.id, error.message)
                }
                EvaluationResult::Suspended(reason) => panic!("unexpected suspension: {reason:?}"),
            }
        }
    }

    #[test]
    fn all_substitution_examples_produce_expected_output_and_counts() {
        let expectations = [
            ("whitespace-cleanup", "one two three\nfour five", 3),
            ("key-value", "user: alice count: 42 debug: true", 3),
        ];
        assert_eq!(
            BUILT_IN_EXAMPLES
                .iter()
                .filter(|example| example.replacement.is_some())
                .count(),
            expectations.len()
        );
        for (id, output, replacements_made) in expectations {
            let example = BUILT_IN_EXAMPLES
                .iter()
                .find(|example| example.id == id)
                .unwrap();
            match evaluate_substitution(
                example.pattern,
                &example.flags,
                example.sample_text,
                example.replacement.unwrap(),
            ) {
                SubstitutionEvaluationResult::Success(result) => {
                    assert_eq!(result.output, output);
                    assert_eq!(result.replacements_made, replacements_made);
                }
                SubstitutionEvaluationResult::InvalidPattern(error) => {
                    panic!("{id}: {}", error.message)
                }
                SubstitutionEvaluationResult::Suspended(reason) => {
                    panic!("unexpected suspension: {reason:?}")
                }
            }
        }
    }
}
