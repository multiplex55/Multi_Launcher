use crate::text_transform::words::tokenize;
use base64::{Engine as _, engine::general_purpose};
use std::collections::{HashMap, HashSet};

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

pub mod identifier {
    use super::{cap, tokenize};
    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
    pub enum IdentifierCase {
        Upper,
        Lower,
        Title,
        Snake,
        Kebab,
        Camel,
        Pascal,
        ScreamingSnake,
    }
    pub fn transform(input: &str, case: IdentifierCase) -> String {
        let toks: Vec<String> = tokenize(input).into_iter().map(|t| t.text).collect();
        match case {
            IdentifierCase::Upper => input.to_uppercase(),
            IdentifierCase::Lower => input.to_lowercase(),
            IdentifierCase::Title => toks.iter().map(|t| cap(t)).collect::<Vec<_>>().join(" "),
            IdentifierCase::Snake => toks
                .iter()
                .map(|t| t.to_lowercase())
                .collect::<Vec<_>>()
                .join("_"),
            IdentifierCase::Kebab => toks
                .iter()
                .map(|t| t.to_lowercase())
                .collect::<Vec<_>>()
                .join("-"),
            IdentifierCase::Camel => {
                let mut it = toks.iter();
                let mut s = it.next().map(|t| t.to_lowercase()).unwrap_or_default();
                for t in it {
                    s.push_str(&cap(t));
                }
                s
            }
            IdentifierCase::Pascal => toks.iter().map(|t| cap(t)).collect(),
            IdentifierCase::ScreamingSnake => toks
                .iter()
                .map(|t| t.to_uppercase())
                .collect::<Vec<_>>()
                .join("_"),
        }
    }
}

pub mod legacy {
    use super::*;
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct LegacyResult {
        pub name: &'static str,
        pub label: String,
        pub desc: &'static str,
    }
    const KNOWN: &[&str] = &[
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
    pub fn parse_query(rest: &str) -> Option<(Option<String>, String)> {
        let mut text = rest.trim().to_string();
        if text.is_empty() {
            return None;
        }
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.len() > 1 {
            let first = words[0].to_lowercase();
            if KNOWN.contains(&first.as_str()) {
                text = text[first.len()..].trim_start().to_string();
                return Some((Some(first), text));
            }
        }
        Some((None, text))
    }
    pub fn transform_all(text: &str) -> Vec<LegacyResult> {
        let words: Vec<&str> = text.split_whitespace().collect();
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
            let mut up = true;
            text.chars()
                .map(|c| {
                    if c.is_ascii_alphabetic() {
                        let o = if up {
                            c.to_ascii_uppercase()
                        } else {
                            c.to_ascii_lowercase()
                        };
                        up = !up;
                        o
                    } else {
                        c
                    }
                })
                .collect::<String>()
        };
        let mocking = {
            let mut up = false;
            text.chars()
                .map(|c| {
                    if c.is_ascii_alphabetic() {
                        let o = if up {
                            c.to_ascii_uppercase()
                        } else {
                            c.to_ascii_lowercase()
                        };
                        up = !up;
                        o
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
        let initials = words
            .iter()
            .filter_map(|w| w.chars().next())
            .map(|c| format!("{}.", c.to_ascii_uppercase()))
            .collect::<Vec<_>>()
            .join(" ");
        let small_set: HashSet<&str> = [
            "a", "an", "and", "or", "the", "in", "on", "of", "for", "to", "at", "by", "with",
            "without",
        ]
        .into_iter()
        .collect();
        let title = words
            .iter()
            .enumerate()
            .map(|(i, w)| {
                if i > 0 && small_set.contains(w.to_lowercase().as_str()) {
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
            .join(" 👏 ");
        let emoji_map: HashMap<&str, &str> = [
            ("world", "🌍"),
            ("love", "❤️"),
            ("fire", "🔥"),
            ("smile", "😄"),
        ]
        .into_iter()
        .collect();
        let emoji = words
            .iter()
            .map(|w| {
                emoji_map
                    .get(w.to_lowercase().as_str())
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
        .into_iter()
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
        let vals = [
            ("upper", upper, "Text Case-Uppercase"),
            ("lower", lower, "Text Case-Lowercase"),
            ("capitalized", capitalized, "Text Case-Capitalized"),
            ("camel", camel, "Text Case-Camel"),
            ("pascal", pascal, "Text Case-Pascal"),
            ("snake", snake, "Text Case-Snake"),
            ("screaming", screaming, "Text Case-Screaming"),
            ("kebab", kebab, "Text Case-Kebab"),
            ("train", train, "Text Case-Train"),
            ("dot", dot, "Text Case-Dot"),
            ("alternating", alt_case, "Text Case-Alternating"),
            ("mocking", mocking, "Text Case-Mocking"),
            ("inverse", inverse, "Text Case-Inverse"),
            ("backwards", backwards, "Text Case-Backwards"),
            ("acronym", acronym, "Text Case-Acronym"),
            ("initials", initials, "Text Case-Initials"),
            ("title", title, "Text Case-Title"),
            ("sentence", sentence, "Text Case-Sentence"),
            ("base64", b64, "Text Case-Base64"),
            ("hex", hex_enc, "Text Case-Hex"),
            ("binary", binary, "Text Case-Binary"),
            ("rot13", rot13, "Text Case-ROT13"),
            ("clap", clap, "Text Case-Clap"),
            ("emoji", emoji, "Text Case-Emoji"),
            ("custom", custom, "Text Case-Custom"),
            ("morse", morse, "Text Case-Morse"),
        ];
        vals.into_iter()
            .map(|(name, label, desc)| LegacyResult { name, label, desc })
            .collect()
    }
    pub fn transform_query(rest: &str) -> Vec<LegacyResult> {
        let Some((specific, text)) = parse_query(rest) else {
            return vec![];
        };
        let all = transform_all(&text);
        if let Some(case) = specific {
            all.into_iter().filter(|r| r.name == case).collect()
        } else {
            all
        }
    }
}

#[cfg(test)]
mod tests {
    use super::identifier::*;
    #[test]
    fn cm_digits() {
        assert_eq!(
            transform("version 2 config", IdentifierCase::Snake),
            "version_2_config"
        );
        assert_eq!(
            transform("version 2 config", IdentifierCase::Camel),
            "version2Config"
        );
        assert_eq!(
            transform("HTTP 2 server", IdentifierCase::Pascal),
            "Http2Server"
        );
        assert_eq!(
            transform("the lord of the rings", IdentifierCase::Title),
            "The Lord Of The Rings"
        );
    }
}
