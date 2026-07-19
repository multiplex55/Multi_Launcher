use crate::text_transform::TextTransformError;
use base64::{Engine as _, engine::general_purpose};
pub fn json_pretty(s: &str) -> Result<String, TextTransformError> {
    let v: serde_json::Value =
        serde_json::from_str(s).map_err(|e| TextTransformError::InvalidJson(e.to_string()))?;
    serde_json::to_string_pretty(&v).map_err(|e| TextTransformError::InvalidJson(e.to_string()))
}
pub fn json_compact(s: &str) -> Result<String, TextTransformError> {
    let v: serde_json::Value =
        serde_json::from_str(s).map_err(|e| TextTransformError::InvalidJson(e.to_string()))?;
    serde_json::to_string(&v).map_err(|e| TextTransformError::InvalidJson(e.to_string()))
}
pub fn json_escape(s: &str) -> String {
    let q = serde_json::to_string(s).unwrap();
    q[1..q.len() - 1].to_string()
}
pub fn json_unescape(s: &str) -> Result<String, TextTransformError> {
    let t = s.trim();
    let q = if t.starts_with('"') && t.ends_with('"') {
        t.to_string()
    } else {
        format!("\"{t}\"")
    };
    serde_json::from_str(&q).map_err(|e| TextTransformError::InvalidJsonEscape(e.to_string()))
}
pub fn percent_encode_component(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
}
pub fn percent_decode_strict(s: &str) -> Result<String, TextTransformError> {
    let b = s.as_bytes();
    let mut bytes = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            if i + 2 >= b.len() || !b[i + 1].is_ascii_hexdigit() || !b[i + 2].is_ascii_hexdigit() {
                return Err(TextTransformError::InvalidPercentEncoding(format!(
                    "bad sequence at byte {i}"
                )));
            }
            let h = &s[i + 1..i + 3];
            bytes.push(u8::from_str_radix(h, 16).unwrap());
            i += 3
        } else {
            bytes.push(b[i]);
            i += 1
        }
    }
    String::from_utf8(bytes).map_err(|e| TextTransformError::InvalidUtf8(e.to_string()))
}
pub fn base64_encode(s: &str) -> String {
    general_purpose::STANDARD.encode(s.as_bytes())
}
pub fn base64_decode(s: &str) -> Result<String, TextTransformError> {
    let t = s.trim();
    if !t
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=')
    {
        return Err(TextTransformError::InvalidBase64(
            "non-standard alphabet".into(),
        ));
    }
    let padded = match t.len() % 4 {
        0 => t.to_string(),
        2 => format!("{t}=="),
        3 => format!("{t}="),
        _ => return Err(TextTransformError::InvalidBase64("invalid length".into())),
    };
    let bytes = general_purpose::STANDARD
        .decode(padded)
        .map_err(|e| TextTransformError::InvalidBase64(e.to_string()))?;
    String::from_utf8(bytes).map_err(|e| TextTransformError::InvalidUtf8(e.to_string()))
}
pub fn windows_to_unix_path(s: &str) -> String {
    s.replace('\\', "/")
}
pub fn unix_to_windows_path(s: &str) -> String {
    s.replace('/', "\\")
}
pub fn rust_regex_escape(s: &str) -> String {
    regex::escape(s)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dev_valid_invalid() {
        assert_eq!(json_compact("{ \"a\" : 1 }").unwrap(), "{\"a\":1}");
        assert!(percent_decode_strict("%xz").is_err());
        assert_eq!(percent_decode_strict("a%20b").unwrap(), "a b");
        assert_eq!(base64_decode("Y2Fmw6k").unwrap(), "café");
        assert!(base64_decode("_-_").is_err())
    }
}
