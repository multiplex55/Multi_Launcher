pub mod atomic_file;
pub fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let candidate = s.get(..prefix.len())?;
    if candidate.eq_ignore_ascii_case(prefix) {
        s.get(prefix.len()..)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::strip_prefix_ci;

    #[test]
    fn strip_prefix_ci_preserves_suffix_and_rejects_non_boundaries() {
        assert_eq!(strip_prefix_ci("", ""), Some(""));
        assert_eq!(strip_prefix_ci("content", ""), Some("content"));
        assert_eq!(strip_prefix_ci("", "x"), None);
        assert_eq!(strip_prefix_ci("set", "settings"), None);
        assert_eq!(
            strip_prefix_ci("SeTtInGs\u{2003}字体", "settings"),
            Some("\u{2003}字体")
        );
        assert_eq!(strip_prefix_ci("settings\u{2003}font", "settingsx"), None);
    }
}

pub mod command;
pub mod config_files;
pub mod entity_ref;
pub mod json_watch;
pub mod lru;
pub mod number_format;
pub mod persistence;
pub mod query;
pub(crate) mod repair_watch;
pub mod slug;
