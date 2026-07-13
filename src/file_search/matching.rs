use crate::file_search::model::FilenameRank;
use std::path::Path;

pub fn rank_filename_match(
    file_name: &str,
    path: &Path,
    search_text: &str,
    case_sensitive: bool,
) -> Option<FilenameRank> {
    let (name, path_text) = if case_sensitive {
        (file_name.to_owned(), path.to_string_lossy().to_string())
    } else {
        (
            file_name.to_lowercase(),
            path.to_string_lossy().to_lowercase(),
        )
    };
    if name == search_text {
        Some(FilenameRank::ExactFilename)
    } else if name.starts_with(search_text) {
        Some(FilenameRank::FilenameStartsWith)
    } else if name.contains(search_text) {
        Some(FilenameRank::FilenameContains)
    } else if path_text.contains(search_text) {
        Some(FilenameRank::FullPathContains)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_search::model::FilenameRank;

    #[test]
    fn ranks_filename_before_path_matches() {
        assert_eq!(
            rank_filename_match("needle.txt", "src/needle.txt".as_ref(), "needle.txt", false),
            Some(FilenameRank::ExactFilename)
        );
        assert_eq!(
            rank_filename_match("needle.txt", "src/needle.txt".as_ref(), "need", false),
            Some(FilenameRank::FilenameStartsWith)
        );
        assert_eq!(
            rank_filename_match(
                "my-needle.txt",
                "src/my-needle.txt".as_ref(),
                "needle",
                false
            ),
            Some(FilenameRank::FilenameContains)
        );
        assert_eq!(
            rank_filename_match("main.rs", "needle/main.rs".as_ref(), "needle", false),
            Some(FilenameRank::FullPathContains)
        );
    }
}
