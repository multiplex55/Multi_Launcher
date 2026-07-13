use crate::file_search::model::FilenameResult;

pub const FILENAME_SORT_LABEL: &str = "Filename";
pub const CONTENT_SORT_LABEL: &str = "Content";

pub fn sort_filename_results(results: &mut [FilenameResult]) {
    results.sort_by(|a, b| {
        a.rank
            .cmp(&b.rank)
            .then_with(|| a.file_name.to_lowercase().cmp(&b.file_name.to_lowercase()))
            .then_with(|| a.path.cmp(&b.path))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_search::model::{FileKind, FilenameRank};

    fn result(path: &str, rank: FilenameRank) -> FilenameResult {
        let path = std::path::PathBuf::from(path);
        FilenameResult {
            file_name: path.file_name().unwrap().to_string_lossy().to_string(),
            parent_directory: path.parent().map(std::path::Path::to_path_buf),
            path,
            kind: FileKind::File,
            size: None,
            modified: None,
            rank,
            match_quality: rank,
            filename_match_ranges: Vec::new(),
            path_match_ranges: Vec::new(),
            arrival_index: 0,
        }
    }

    #[test]
    fn sort_is_stable_by_rank_name_then_path() {
        let mut results = vec![
            result("b/foo.txt", FilenameRank::FilenameContains),
            result("a/foo.txt", FilenameRank::FilenameContains),
            result("z/foo.txt", FilenameRank::ExactFilename),
        ];
        sort_filename_results(&mut results);
        assert_eq!(
            results
                .iter()
                .map(|r| r.path.to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            vec!["z/foo.txt", "a/foo.txt", "b/foo.txt"]
        );
    }
}
