use crate::text_transform::TextTransformError;
pub fn inline(s: &str, left: &str, right: &str) -> String {
    format!("{left}{s}{right}")
}
pub fn block(s: &str, prefix: &str, suffix: &str) -> String {
    format!("{prefix}\n{s}\n{suffix}")
}
pub fn validate_language_identifier(
    lang: Option<&str>,
) -> Result<Option<String>, TextTransformError> {
    match lang {
        None => Ok(None),
        Some(raw) => {
            let t = raw.trim();
            if t.is_empty() {
                return Err(TextTransformError::InvalidLanguageIdentifier(
                    "empty language identifier".into(),
                ));
            }
            if t.chars().any(char::is_whitespace) {
                return Err(TextTransformError::InvalidLanguageIdentifier(
                    "language identifier contains whitespace".into(),
                ));
            }
            Ok(Some(t.to_string()))
        }
    }
}
fn longest_backtick_run(s: &str) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for c in s.chars() {
        if c == '`' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}

pub fn markdown_fence(s: &str, lang: Option<&str>) -> Result<String, TextTransformError> {
    let lang = validate_language_identifier(lang)?.unwrap_or_default();
    let fence = "`".repeat(std::cmp::max(3, longest_backtick_run(s) + 1));
    Ok(format!("{fence}{lang}\n{s}\n{fence}"))
}
pub fn json_string(s: &str) -> String {
    serde_json::to_string(s).unwrap()
}
pub fn powershell_single_quoted(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}
pub fn rust_escaped_string(s: &str) -> String {
    format!("{:?}", s)
}
pub fn rust_raw_string(s: &str) -> String {
    let mut hashes = 0;
    loop {
        let end = format!("\"{}", "#".repeat(hashes));
        if !s.contains(&end) {
            return format!("r{}\"{}\"{}", "#".repeat(hashes), s, "#".repeat(hashes));
        }
        hashes += 1
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fence_longer() {
        let s = markdown_fence("a ``` b ```` c", Some("Rust")).unwrap();
        assert!(s.starts_with("`````Rust"));
        assert!(validate_language_identifier(Some(" ")).is_err());
        assert!(validate_language_identifier(Some("rs lang")).is_err())
    }
}

#[cfg(test)]
mod comprehensive_wrapper_regressions {
    use super::*;
    #[test]
    fn empty_and_embedded_delimiters() {
        assert_eq!(inline("", "[", "]"), "[]");
        assert_eq!(block("a\r\nb\n", "<", ">"), "<\na\r\nb\n\n>");
        assert_eq!(powershell_single_quoted("Bob's 'hat'"), "'Bob''s ''hat'''");
        assert_eq!(json_string("a\n\"b\\c"), r#""a\n\"b\\c""#);
    }
    #[test]
    fn backtick_and_rust_raw_string_collisions() {
        let fenced = markdown_fence("```\n````", Some("rs")).unwrap();
        assert!(fenced.starts_with("`````rs"));
        assert_eq!(
            rust_raw_string("contains \"# and \"##"),
            "r###\"contains \"# and \"##\"###"
        );
    }
}
