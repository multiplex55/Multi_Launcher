use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkTarget {
    Note,
    Todo,
    Bookmark,
    Layout,
    File,
}

impl LinkTarget {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            LinkTarget::Note => "note",
            LinkTarget::Todo => "todo",
            LinkTarget::Bookmark => "bookmark",
            LinkTarget::Layout => "layout",
            LinkTarget::File => "file",
        }
    }

    pub(crate) fn parse(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "note" => Some(LinkTarget::Note),
            "todo" => Some(LinkTarget::Todo),
            "bookmark" => Some(LinkTarget::Bookmark),
            "layout" => Some(LinkTarget::Layout),
            "file" => Some(LinkTarget::File),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LinkRef {
    pub target_type: LinkTarget,
    pub target_id: String,
    #[serde(default)]
    pub anchor: Option<String>,
    #[serde(default)]
    pub display_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkTrigger {
    pub at_char_index: usize,
    pub query: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkParseError {
    InvalidScheme,
    InvalidTargetType,
    MissingTargetId,
}

pub fn detect_link_trigger(text: &str, cursor_char_index: usize) -> Option<LinkTrigger> {
    let cursor_byte_index = char_to_byte_index(text, cursor_char_index);
    let prefix = &text[..cursor_byte_index];
    let at_byte_index = prefix.rfind('@')?;

    if is_code_context_at(prefix, at_byte_index) {
        return None;
    }
    if at_byte_index > 0 && prefix.as_bytes()[at_byte_index - 1] == b'\\' {
        return None;
    }
    let before = prefix[..at_byte_index].chars().next_back();
    if before.is_some_and(|ch| ch.is_alphanumeric()) {
        return None;
    }
    let query = &prefix[at_byte_index + 1..];
    if query
        .chars()
        .any(|ch| ch.is_whitespace() || matches!(ch, '`' | ']'))
    {
        return None;
    }

    Some(LinkTrigger {
        at_char_index: prefix[..at_byte_index].chars().count(),
        query: query.to_string(),
    })
}

pub fn format_inserted_link(link: &LinkRef) -> String {
    format_link_id(link)
}

pub fn format_link_id(link: &LinkRef) -> String {
    let mut out = format!(
        "link://{}/{}",
        link.target_type.as_str(),
        urlencoding::encode(&link.target_id)
    );
    if let Some(anchor) = &link.anchor
        && !anchor.is_empty() {
            out.push('#');
            out.push_str(&urlencoding::encode(anchor));
        }
    if let Some(text) = &link.display_text
        && !text.is_empty() {
            out.push_str("?text=");
            out.push_str(&urlencoding::encode(text));
        }
    out
}

pub fn parse_link_id(link_id: &str) -> Result<LinkRef, LinkParseError> {
    let rest = link_id
        .strip_prefix("link://")
        .ok_or(LinkParseError::InvalidScheme)?;
    let (path_part, query_part) = rest.split_once('?').unwrap_or((rest, ""));
    let (path_core, anchor) = path_part
        .split_once('#')
        .map(|(a, b)| (a, Some(b)))
        .unwrap_or((path_part, None));
    let (target_type_raw, target_id_raw) = path_core
        .split_once('/')
        .ok_or(LinkParseError::MissingTargetId)?;
    let target_type =
        LinkTarget::parse(target_type_raw).ok_or(LinkParseError::InvalidTargetType)?;
    let target_id = urlencoding::decode(target_id_raw)
        .map_err(|_| LinkParseError::MissingTargetId)?
        .to_string();
    if target_id.trim().is_empty() {
        return Err(LinkParseError::MissingTargetId);
    }
    let anchor = anchor
        .filter(|s| !s.is_empty())
        .map(|s| urlencoding::decode(s).unwrap_or_default().to_string())
        .filter(|s| !s.trim().is_empty());
    let mut display_text = None;
    if !query_part.is_empty() {
        for pair in query_part.split('&') {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            if k == "text" {
                let decoded = urlencoding::decode(v).unwrap_or_default().to_string();
                if !decoded.trim().is_empty() {
                    display_text = Some(decoded);
                }
            }
        }
    }
    Ok(LinkRef {
        target_type,
        target_id,
        anchor,
        display_text,
    })
}

fn is_code_context_at(text: &str, at_byte_index: usize) -> bool {
    let bytes = text.as_bytes();
    let mut idx = 0;
    let mut in_fenced = false;
    let mut in_inline = false;

    while idx < at_byte_index {
        if idx + 2 < at_byte_index && &bytes[idx..idx + 3] == b"```" {
            in_fenced = !in_fenced;
            idx += 3;
            continue;
        }
        if !in_fenced && bytes[idx] == b'`' {
            in_inline = !in_inline;
        }
        idx += 1;
    }

    in_fenced || in_inline
}

fn char_to_byte_index(s: &str, char_index: usize) -> usize {
    s.char_indices()
        .nth(char_index)
        .map(|(idx, _)| idx)
        .unwrap_or_else(|| s.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_parse_format_round_trip() {
        let link = LinkRef {
            target_type: LinkTarget::Note,
            target_id: "release plan".into(),
            anchor: Some("section 2".into()),
            display_text: Some("Release Plan".into()),
        };
        let id = format_link_id(&link);
        assert_eq!(parse_link_id(&id).unwrap(), link);
    }

    #[test]
    fn trigger_detection_rejects_escaped_or_code_context() {
        assert_eq!(
            detect_link_trigger("hello @pla", "hello @pla".chars().count()),
            Some(LinkTrigger {
                at_char_index: 6,
                query: "pla".to_string()
            })
        );
        assert!(detect_link_trigger("hello \\@pla", "hello \\@pla".chars().count()).is_none());
        assert!(detect_link_trigger("`hello @pla`", "`hello @pla`".chars().count()).is_none());
        assert!(detect_link_trigger("```\n@pla\n```", "```\n@pla".chars().count()).is_none());
    }

    #[test]
    fn insertion_formatter_preserves_anchor_and_text() {
        let base = LinkRef {
            target_type: LinkTarget::Note,
            target_id: "alpha".to_string(),
            anchor: None,
            display_text: None,
        };
        assert_eq!(format_inserted_link(&base), "link://note/alpha");

        let with_anchor = LinkRef {
            target_type: LinkTarget::Note,
            target_id: "alpha".to_string(),
            anchor: Some("section-1".to_string()),
            display_text: Some("Section 1".to_string()),
        };
        assert_eq!(
            format_inserted_link(&with_anchor),
            "link://note/alpha#section-1?text=Section%201"
        );
    }
}
