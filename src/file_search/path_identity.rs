use std::path::{Component, Path, PathBuf};

pub fn normalize_path_for_identity(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub fn dedup_overlapping_roots(mut roots: Vec<PathBuf>) -> Vec<PathBuf> {
    roots.sort();
    roots.dedup();
    let mut deduped: Vec<PathBuf> = Vec::new();
    'outer: for root in roots {
        let normalized = normalize_path_for_identity(&root);
        for existing in &deduped {
            if normalized.starts_with(existing) {
                continue 'outer;
            }
        }
        deduped.push(normalized);
    }
    deduped
}

#[cfg(windows)]
pub fn case_insensitive_path_key(path: &Path) -> String {
    normalize_path_for_identity(path)
        .to_string_lossy()
        .to_lowercase()
}
#[cfg(not(windows))]
pub fn case_insensitive_path_key(path: &Path) -> String {
    normalize_path_for_identity(path)
        .to_string_lossy()
        .to_string()
}
