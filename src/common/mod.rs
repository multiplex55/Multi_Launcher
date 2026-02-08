pub fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

pub mod command;
pub mod config_files;
pub mod entity_ref;
pub mod json_watch;
pub mod lru;
pub mod query;
pub mod slug;
