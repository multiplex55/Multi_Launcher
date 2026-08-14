#![allow(clippy::field_reassign_with_default)]

use multi_launcher::diff::text_compare::*;
use multi_launcher::diff::text_file::*;

#[test]
fn encoding_round_trips_and_binary_is_not_an_io_error() {
    let dir = tempfile::tempdir().unwrap();
    for (name, bytes, encoding) in [
        ("utf8", b"a\r\nb\r\n".to_vec(), TextEncoding::Utf8),
        (
            "bom",
            [vec![0xef, 0xbb, 0xbf], b"a\n".to_vec()].concat(),
            TextEncoding::Utf8,
        ),
        (
            "le",
            [
                vec![0xff, 0xfe],
                "a\n".encode_utf16().flat_map(u16::to_le_bytes).collect(),
            ]
            .concat(),
            TextEncoding::Utf16Le,
        ),
        (
            "be",
            [
                vec![0xfe, 0xff],
                "a\n".encode_utf16().flat_map(u16::to_be_bytes).collect(),
            ]
            .concat(),
            TextEncoding::Utf16Be,
        ),
    ] {
        let path = dir.path().join(name);
        std::fs::write(&path, &bytes).unwrap();
        let loaded = load_text_file(&path).unwrap();
        assert_eq!(loaded.encoding, Some(encoding));
        let mut doc = TextDocument::from_loaded(&loaded).unwrap();
        doc.save(&path).unwrap();
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    let path = dir.path().join("binary");
    std::fs::write(&path, [0, 1, 2, 3]).unwrap();
    assert!(load_text_file(path).unwrap().is_binary());
}

#[test]
fn ignored_changes_are_projected_but_source_and_navigation_survive() {
    let left = " A 👩🏽‍💻 \n\nSame".to_string();
    let right = "a 👩🏽‍💻\nSame".to_string();
    let mut rules = TextComparisonRules::default();
    rules.ignore_leading_whitespace = true;
    rules.ignore_trailing_whitespace = true;
    rules.ignore_blank_lines = true;
    rules.case_sensitive = false;
    let compiled = CompiledRules::compile(&rules).unwrap();
    let result = compare(&left, &right, 4, 7, &compiled, 1024);
    assert_eq!(left, " A 👩🏽‍💻 \n\nSame");
    assert!(result.equal_under_rules);
    assert!(!result.raw_equal);
    assert_eq!(result.navigation.all_difference_rows.len(), 1);
    assert!(result.navigation.important_difference_rows.is_empty());
    assert!(result.is_stale(5, 7, rules.revision));
}

#[test]
fn replacement_order_and_invalid_rules_are_deterministic() {
    let mut r = TextComparisonRules::default();
    r.replacements = vec![
        RegexReplacement {
            pattern: "ab".into(),
            replacement: "x".into(),
        },
        RegexReplacement {
            pattern: "x".into(),
            replacement: "y".into(),
        },
    ];
    let c = CompiledRules::compile(&r).unwrap();
    assert_eq!(project("ab", &c).lines[0].key, "y");
    r.unimportant_sections.push("[".into());
    assert!(CompiledRules::compile(&r).unwrap_err()[0].contains("unimportant expression"));
}
