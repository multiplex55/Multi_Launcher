use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const OPEN_PREFIX: &str = "diff:open:";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffOpenPayload {
    pub left: Option<String>,
    pub right: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffCommand {
    Open,
    OpenWithLeft {
        visible: String,
        normalized: PathBuf,
    },
    Compare {
        left_visible: String,
        right_visible: String,
        left_normalized: PathBuf,
        right_normalized: PathBuf,
    },
    Error(String),
}

pub fn parse_diff_query(query: &str) -> Option<DiffCommand> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }
    let tokens = match shlex::split(trimmed) {
        Some(tokens) => tokens,
        None if trimmed
            .split_whitespace()
            .next()
            .is_some_and(|h| h.eq_ignore_ascii_case("diff")) =>
        {
            return Some(DiffCommand::Error(
                "Malformed quoting in diff command".into(),
            ));
        }
        None => return None,
    };
    if !tokens
        .first()
        .is_some_and(|head| head.eq_ignore_ascii_case("diff"))
    {
        return None;
    }
    match tokens.as_slice() {
        [_] => Some(DiffCommand::Open),
        [_, left] => Some(DiffCommand::OpenWithLeft {
            visible: left.clone(),
            normalized: normalize_path(left),
        }),
        [_, left, right] => Some(DiffCommand::Compare {
            left_visible: left.clone(),
            right_visible: right.clone(),
            left_normalized: normalize_path(left),
            right_normalized: normalize_path(right),
        }),
        _ => Some(DiffCommand::Error(
            "Usage: diff [<left-path> [<right-path>]]".into(),
        )),
    }
}

pub fn normalize_path(value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if let Ok(canonical) = std::fs::canonicalize(&path) {
        return canonical;
    }
    // Missing paths still need a stable identity. Making them absolute also
    // collapses harmless `./` components without changing the visible value.
    let absolute = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    absolute.components().collect()
}

pub fn encode_payload(payload: &DiffOpenPayload) -> Result<String, String> {
    let bytes = serde_json::to_vec(payload).map_err(|e| e.to_string())?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

pub fn decode_payload(value: &str) -> Result<DiffOpenPayload, String> {
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|e| format!("invalid diff payload: {e}"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("invalid diff payload JSON: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_visible_paths() {
        let command = parse_diff_query(r#"diff "./left file" "right file""#).unwrap();
        let DiffCommand::Compare {
            left_visible,
            right_visible,
            ..
        } = command
        else {
            panic!()
        };
        assert_eq!(left_visible, "./left file");
        assert_eq!(right_visible, "right file");
    }
    #[test]
    fn escaped_spaces_are_tokenized() {
        assert!(
            matches!(parse_diff_query(r#"diff left\ file right"#), Some(DiffCommand::Compare { left_visible, .. }) if left_visible == "left file")
        );
    }
    #[test]
    fn malformed_quote_is_error() {
        assert!(matches!(
            parse_diff_query("diff \"oops"),
            Some(DiffCommand::Error(_))
        ));
    }
}
