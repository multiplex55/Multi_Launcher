use chrono::NaiveDate;
use regex::Regex;
use std::collections::HashSet;

pub const PRIORITY_MIN: u8 = 1;
pub const PRIORITY_MAX: u8 = 5;

pub fn tag_re() -> Regex {
    Regex::new(r"(?P<tag>#[A-Za-z][A-Za-z0-9_-]*)").expect("valid tag regex")
}

pub fn priority_re() -> Regex {
    Regex::new(r"\bp(?P<n>[0-9]+)\b").expect("valid priority regex")
}

pub fn due_re() -> Regex {
    Regex::new(r"@due\s+(?P<date>\d{4}-\d{2}-\d{2})").expect("valid due regex")
}

pub fn normalize_text(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn parse_metadata(body: &str) -> (String, Vec<String>, Option<u8>, Option<NaiveDate>) {
    let tags = tag_re()
        .captures_iter(body)
        .filter_map(|c| c.name("tag"))
        .map(|m| m.as_str().trim_start_matches('#').to_lowercase())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let priority = priority_re()
        .captures_iter(body)
        .filter_map(|c| c.name("n"))
        .filter_map(|m| m.as_str().parse::<u8>().ok())
        .find(|p| (*p >= PRIORITY_MIN) && (*p <= PRIORITY_MAX));

    let due = due_re()
        .captures_iter(body)
        .filter_map(|c| c.name("date"))
        .find_map(|m| NaiveDate::parse_from_str(m.as_str(), "%Y-%m-%d").ok());

    let stripped = due_re().replace_all(body, "");
    let stripped = priority_re().replace_all(&stripped, "");
    let stripped = tag_re().replace_all(&stripped, "");
    (stripped.trim().to_string(), tags, priority, due)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_parses_edge_cases() {
        let (text, mut tags, priority, due) =
            parse_metadata("Ship parser #work #rust #work p2 p6 @due 2026-03-01 @due nope");
        tags.sort();
        assert_eq!(text, "Ship parser");
        assert_eq!(tags, vec!["rust", "work"]);
        assert_eq!(priority, Some(2));
        assert_eq!(due, NaiveDate::from_ymd_opt(2026, 3, 1));
    }
}
