use std::path::PathBuf;

pub fn preview_window_id_source() -> &'static str {
    "file_search_preview_window"
}

pub fn preview_scroll_id_source(path: impl Into<PathBuf>) -> (&'static str, PathBuf) {
    ("file_search_preview_scroll", path.into())
}

pub fn preview_line_id_source(
    path: impl Into<PathBuf>,
    line_number: usize,
) -> (&'static str, PathBuf, usize) {
    ("file_search_preview_line", path.into(), line_number)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_id_sources_are_stable_and_path_scoped() {
        let path = PathBuf::from("src/lib.rs");
        assert_eq!(preview_window_id_source(), "file_search_preview_window");
        assert_eq!(
            preview_scroll_id_source(&path),
            ("file_search_preview_scroll", path.clone())
        );
        assert_ne!(
            preview_scroll_id_source("src/lib.rs"),
            preview_scroll_id_source("src/main.rs")
        );
        assert_ne!(
            preview_line_id_source(&path, 1),
            preview_line_id_source(&path, 2)
        );
    }
}
