use crate::actions::Action;
use crate::plugin::Plugin;
use base64::{engine::general_purpose, Engine as _};
use hex;
use std::collections::HashMap;

pub struct TextCasePlugin;

impl Plugin for TextCasePlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "case ";
        if let Some(rest) = crate::common::strip_prefix_ci(query.trim_start(), PREFIX) {
            let mut text = rest.trim().to_string();
            if !text.is_empty() {
                let mut words: Vec<&str> = text.split_whitespace().collect();

                // Allow specifying the desired case as the first word, e.g. "case hex foo".
                // If the first token matches a known case name and there is additional text,
                // treat the remainder as the text to convert and only output that case.
                let mut specific_case: Option<String> = None;
                if words.len() > 1 {
                    let first = words[0].to_lowercase();
                    let known = [
                        "upper",
                        "lower",
                        "capitalized",
                        "camel",
                        "pascal",
                        "snake",
                        "screaming",
                        "kebab",
                        "train",
                        "dot",
                        "alternating",
                        "mocking",
                        "inverse",
                        "backwards",
                        "acronym",
                        "initials",
                        "title",
                        "sentence",
                        "base64",
                        "hex",
                        "binary",
                        "rot13",
                        "clap",
                        "emoji",
                        "custom",
                        "morse",
                    ];
                    if known.contains(&first.as_str()) {
                        specific_case = Some(first.clone());
                        text = text[first.len()..].trim_start().to_string();
                        words = text.split_whitespace().collect();
                    }
                }

                fn cap(w: &str) -> String {
                    let mut c = w.chars();
                    match c.next() {
                        Some(f) => {
                            let mut s = f.to_uppercase().to_string();
                            s.push_str(&c.as_str().to_lowercase());
                            s
                        }
                        None => String::new(),
                    }
                }

                let upper = text.to_uppercase();
                let lower = text.to_lowercase();
                let capitalized = words.iter().map(|w| cap(w)).collect::<Vec<_>>().join(" ");

                let camel = if let Some((first, rest)) = words.split_first() {
                    let mut s = first.to_lowercase();
                    for w in rest {
                        s.push_str(&cap(w));
                    }
                    s
                } else {
                    String::new()
                };

                let pascal = words.iter().map(|w| cap(w)).collect::<String>();
                let snake = words
                    .iter()
                    .map(|w| w.to_lowercase())
                    .collect::<Vec<_>>()
                    .join("_");
                let screaming = words
                    .iter()
                    .map(|w| w.to_uppercase())
                    .collect::<Vec<_>>()
                    .join("_");
                let kebab = words
                    .iter()
                    .map(|w| w.to_lowercase())
                    .collect::<Vec<_>>()
                    .join("-");
                let train = words.iter().map(|w| cap(w)).collect::<Vec<_>>().join("-");
                let dot = words
                    .iter()
                    .map(|w| w.to_lowercase())
                    .collect::<Vec<_>>()
                    .join(".");

                let alt_case = {
                    let mut upper_flag = true;
                    text.chars()
                        .map(|c| {
                            if c.is_ascii_alphabetic() {
                                let out = if upper_flag {
                                    c.to_ascii_uppercase()
                                } else {
                                    c.to_ascii_lowercase()
                                };
                                upper_flag = !upper_flag;
                                out
                            } else {
                                c
                            }
                        })
                        .collect::<String>()
                };

                let mocking = {
                    let mut upper_flag = false;
                    text.chars()
                        .map(|c| {
                            if c.is_ascii_alphabetic() {
                                let out = if upper_flag {
                                    c.to_ascii_uppercase()
                                } else {
                                    c.to_ascii_lowercase()
                                };
                                upper_flag = !upper_flag;
                                out
                            } else {
                                c
                            }
                        })
                        .collect::<String>()
                };

                let inverse = text
                    .chars()
                    .map(|c| {
                        if c.is_ascii_lowercase() {
                            c.to_ascii_uppercase()
                        } else if c.is_ascii_uppercase() {
                            c.to_ascii_lowercase()
                        } else {
                            c
                        }
                    })
                    .collect::<String>();

                let backwards = text.chars().rev().collect::<String>();

                let acronym = words
                    .iter()
                    .filter_map(|w| w.chars().next())
                    .map(|c| c.to_ascii_uppercase())
                    .collect::<String>();

                let initial_caps = words
                    .iter()
                    .filter_map(|w| w.chars().next())
                    .map(|c| format!("{}.", c.to_ascii_uppercase()))
                    .collect::<Vec<_>>()
                    .join(" ");

                let small_words = [
                    "a", "an", "and", "or", "the", "in", "on", "of", "for", "to", "at", "by",
                    "with", "without",
                ];
                let small_set: std::collections::HashSet<&str> =
                    small_words.iter().cloned().collect();
                let title_case = words
                    .iter()
                    .enumerate()
                    .map(|(i, w)| {
                        if i > 0 && small_set.contains(&w.to_lowercase().as_str()) {
                            w.to_lowercase()
                        } else {
                            cap(w)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");

                let sentence = {
                    let mut chars = lower.chars();
                    match chars.next() {
                        Some(f) => {
                            let mut s = f.to_ascii_uppercase().to_string();
                            s.push_str(chars.as_str());
                            s
                        }
                        None => String::new(),
                    }
                };

                let b64 = general_purpose::STANDARD.encode(text.as_bytes());
                let hex_enc = hex::encode(text.as_bytes());
                let binary = text
                    .as_bytes()
                    .iter()
                    .map(|b| format!("{:08b}", b))
                    .collect::<Vec<_>>()
                    .join(" ");

                let rot13 = text
                    .chars()
                    .map(|c| match c {
                        'a'..='z' => (((c as u8 - b'a' + 13) % 26) + b'a') as char,
                        'A'..='Z' => (((c as u8 - b'A' + 13) % 26) + b'A') as char,
                        _ => c,
                    })
                    .collect::<String>();

                let clap = words
                    .iter()
                    .map(|w| w.to_lowercase())
                    .collect::<Vec<_>>()
                    .join(" \u{1F44F} ");

                let emoji_map: HashMap<&str, &str> = [
                    ("world", "\u{1F30D}"),
                    ("love", "\u{2764}\u{FE0F}"),
                    ("fire", "\u{1F525}"),
                    ("smile", "\u{1F604}"),
                ]
                .iter()
                .cloned()
                .collect();
                let emoji_case = words
                    .iter()
                    .map(|w| {
                        emoji_map
                            .get(&w.to_lowercase().as_str())
                            .copied()
                            .unwrap_or(*w)
                    })
                    .collect::<Vec<_>>()
                    .join(" ");

                let custom = words
                    .iter()
                    .map(|w| {
                        w.chars()
                            .map(|c| c.to_string())
                            .collect::<Vec<_>>()
                            .join("-")
                    })
                    .collect::<Vec<_>>()
                    .join(" ");

                let morse_map: HashMap<char, &str> = [
                    ('a', ".-"),
                    ('b', "-..."),
                    ('c', "-.-."),
                    ('d', "-.."),
                    ('e', "."),
                    ('f', "..-."),
                    ('g', "--."),
                    ('h', "...."),
                    ('i', ".."),
                    ('j', ".---"),
                    ('k', "-.-"),
                    ('l', ".-.."),
                    ('m', "--"),
                    ('n', "-."),
                    ('o', "---"),
                    ('p', ".--."),
                    ('q', "--.-"),
                    ('r', ".-."),
                    ('s', "..."),
                    ('t', "-"),
                    ('u', "..-"),
                    ('v', "...-"),
                    ('w', ".--"),
                    ('x', "-..-"),
                    ('y', "-.--"),
                    ('z', "--.."),
                    ('0', "-----"),
                    ('1', ".----"),
                    ('2', "..---"),
                    ('3', "...--"),
                    ('4', "....-"),
                    ('5', "....."),
                    ('6', "-...."),
                    ('7', "--..."),
                    ('8', "---.."),
                    ('9', "----."),
                ]
                .iter()
                .cloned()
                .collect();

                let morse = text
                    .to_lowercase()
                    .chars()
                    .map(|c| {
                        if c == ' ' {
                            "/".to_string()
                        } else {
                            morse_map.get(&c).unwrap_or(&"?").to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");

                let actions = vec![
                    (
                        "upper",
                        Action {
                            label: upper.clone(),
                            desc: "Text Case-Uppercase".into(),
                            action: format!("clipboard:{}", upper),
                            args: None,
                        },
                    ),
                    (
                        "lower",
                        Action {
                            label: lower.clone(),
                            desc: "Text Case-Lowercase".into(),
                            action: format!("clipboard:{}", lower),
                            args: None,
                        },
                    ),
                    (
                        "capitalized",
                        Action {
                            label: capitalized.clone(),
                            desc: "Text Case-Capitalized".into(),
                            action: format!("clipboard:{}", capitalized),
                            args: None,
                        },
                    ),
                    (
                        "camel",
                        Action {
                            label: camel.clone(),
                            desc: "Text Case-Camel".into(),
                            action: format!("clipboard:{}", camel),
                            args: None,
                        },
                    ),
                    (
                        "pascal",
                        Action {
                            label: pascal.clone(),
                            desc: "Text Case-Pascal".into(),
                            action: format!("clipboard:{}", pascal),
                            args: None,
                        },
                    ),
                    (
                        "snake",
                        Action {
                            label: snake.clone(),
                            desc: "Text Case-Snake".into(),
                            action: format!("clipboard:{}", snake),
                            args: None,
                        },
                    ),
                    (
                        "screaming",
                        Action {
                            label: screaming.clone(),
                            desc: "Text Case-Screaming".into(),
                            action: format!("clipboard:{}", screaming),
                            args: None,
                        },
                    ),
                    (
                        "kebab",
                        Action {
                            label: kebab.clone(),
                            desc: "Text Case-Kebab".into(),
                            action: format!("clipboard:{}", kebab),
                            args: None,
                        },
                    ),
                    (
                        "train",
                        Action {
                            label: train.clone(),
                            desc: "Text Case-Train".into(),
                            action: format!("clipboard:{}", train),
                            args: None,
                        },
                    ),
                    (
                        "dot",
                        Action {
                            label: dot.clone(),
                            desc: "Text Case-Dot".into(),
                            action: format!("clipboard:{}", dot),
                            args: None,
                        },
                    ),
                    (
                        "alternating",
                        Action {
                            label: alt_case.clone(),
                            desc: "Text Case-Alternating".into(),
                            action: format!("clipboard:{}", alt_case),
                            args: None,
                        },
                    ),
                    (
                        "mocking",
                        Action {
                            label: mocking.clone(),
                            desc: "Text Case-Mocking".into(),
                            action: format!("clipboard:{}", mocking),
                            args: None,
                        },
                    ),
                    (
                        "inverse",
                        Action {
                            label: inverse.clone(),
                            desc: "Text Case-Inverse".into(),
                            action: format!("clipboard:{}", inverse),
                            args: None,
                        },
                    ),
                    (
                        "backwards",
                        Action {
                            label: backwards.clone(),
                            desc: "Text Case-Backwards".into(),
                            action: format!("clipboard:{}", backwards),
                            args: None,
                        },
                    ),
                    (
                        "acronym",
                        Action {
                            label: acronym.clone(),
                            desc: "Text Case-Acronym".into(),
                            action: format!("clipboard:{}", acronym),
                            args: None,
                        },
                    ),
                    (
                        "initials",
                        Action {
                            label: initial_caps.clone(),
                            desc: "Text Case-Initials".into(),
                            action: format!("clipboard:{}", initial_caps),
                            args: None,
                        },
                    ),
                    (
                        "title",
                        Action {
                            label: title_case.clone(),
                            desc: "Text Case-Title".into(),
                            action: format!("clipboard:{}", title_case),
                            args: None,
                        },
                    ),
                    (
                        "sentence",
                        Action {
                            label: sentence.clone(),
                            desc: "Text Case-Sentence".into(),
                            action: format!("clipboard:{}", sentence),
                            args: None,
                        },
                    ),
                    (
                        "base64",
                        Action {
                            label: b64.clone(),
                            desc: "Text Case-Base64".into(),
                            action: format!("clipboard:{}", b64),
                            args: None,
                        },
                    ),
                    (
                        "hex",
                        Action {
                            label: hex_enc.clone(),
                            desc: "Text Case-Hex".into(),
                            action: format!("clipboard:{}", hex_enc),
                            args: None,
                        },
                    ),
                    (
                        "binary",
                        Action {
                            label: binary.clone(),
                            desc: "Text Case-Binary".into(),
                            action: format!("clipboard:{}", binary),
                            args: None,
                        },
                    ),
                    (
                        "rot13",
                        Action {
                            label: rot13.clone(),
                            desc: "Text Case-ROT13".into(),
                            action: format!("clipboard:{}", rot13),
                            args: None,
                        },
                    ),
                    (
                        "clap",
                        Action {
                            label: clap.clone(),
                            desc: "Text Case-Clap".into(),
                            action: format!("clipboard:{}", clap),
                            args: None,
                        },
                    ),
                    (
                        "emoji",
                        Action {
                            label: emoji_case.clone(),
                            desc: "Text Case-Emoji".into(),
                            action: format!("clipboard:{}", emoji_case),
                            args: None,
                        },
                    ),
                    (
                        "custom",
                        Action {
                            label: custom.clone(),
                            desc: "Text Case-Custom".into(),
                            action: format!("clipboard:{}", custom),
                            args: None,
                        },
                    ),
                    (
                        "morse",
                        Action {
                            label: morse.clone(),
                            desc: "Text Case-Morse".into(),
                            action: format!("clipboard:{}", morse),
                            args: None,
                        },
                    ),
                ];

                if let Some(case) = specific_case {
                    if let Some((_, act)) = actions.iter().find(|(name, _)| *name == case) {
                        return vec![act.clone()];
                    }
                }

                return actions.into_iter().map(|(_, a)| a).collect();
            }
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "text_case"
    }

    fn description(&self) -> &str {
        "Convert text cases (prefix: `case`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "case <text>".into(),
            desc: "Text Case".into(),
            action: "query:case ".into(),
            args: None,
        }]
    }
}
