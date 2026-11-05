//! Small explicit-path index for recently used native Diagram documents.

use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

pub const MAX_DIAGRAM_RECENTS: usize = 32;
pub const MAX_DIAGRAM_RECENTS_BYTES: usize = 256 * 1024;
pub const MAX_DIAGRAM_RECENT_PATH_BYTES: usize = 16 * 1024;
const RECENT_INDEX_VERSION: u8 = 1;

#[derive(Debug)]
pub enum RecentDiagramIndexError {
    Io { path: PathBuf, source: io::Error },
    UnsafePath(PathBuf),
    Oversized { max_bytes: usize },
    InvalidIndex(String),
    InvalidRecentPath(PathBuf),
    Changed(PathBuf),
}

impl fmt::Display for RecentDiagramIndexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(
                formatter,
                "could not read or save Diagram recents at {}: {source}",
                path.display()
            ),
            Self::UnsafePath(path) => write!(
                formatter,
                "Diagram recents path is not a regular file: {}",
                path.display()
            ),
            Self::Oversized { max_bytes } => write!(
                formatter,
                "Diagram recents exceed the {max_bytes}-byte limit"
            ),
            Self::InvalidIndex(reason) => {
                write!(formatter, "Diagram recents are invalid: {reason}")
            }
            Self::InvalidRecentPath(path) => write!(
                formatter,
                "recent Diagram path is not a bounded absolute path: {}",
                path.display()
            ),
            Self::Changed(path) => write!(
                formatter,
                "Diagram recents changed outside this session and were not overwritten: {}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for RecentDiagramIndexError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecentDiagramIndex {
    paths: Vec<PathBuf>,
    disk_digest: Option<[u8; 32]>,
}

impl RecentDiagramIndex {
    pub fn load(path: &Path) -> Result<Self, RecentDiagramIndexError> {
        let Some(bytes) = read_index_file(path)? else {
            return Ok(Self::default());
        };

        let wire: RecentDiagramIndexWire = serde_json::from_slice(&bytes)
            .map_err(|error| RecentDiagramIndexError::InvalidIndex(error.to_string()))?;
        if wire.version != RECENT_INDEX_VERSION {
            return Err(RecentDiagramIndexError::InvalidIndex(format!(
                "unsupported version {}",
                wire.version
            )));
        }
        let mut index = Self {
            paths: Vec::with_capacity(wire.paths.len().min(MAX_DIAGRAM_RECENTS)),
            disk_digest: Some(index_digest(&bytes)),
        };
        let mut seen = std::collections::HashSet::with_capacity(wire.paths.len());
        for value in wire.paths {
            if value.len() > MAX_DIAGRAM_RECENT_PATH_BYTES
                || value.is_empty()
                || value.chars().any(char::is_control)
            {
                return Err(RecentDiagramIndexError::InvalidIndex(
                    "an entry is not a bounded path".into(),
                ));
            }
            let recent_path = PathBuf::from(value);
            if !recent_path.is_absolute() {
                return Err(RecentDiagramIndexError::InvalidRecentPath(recent_path));
            }
            if seen.insert(path_key(&recent_path)) {
                // The file is already newest-first. Keep its first occurrence
                // so a duplicate cannot move an older path ahead of the MRU.
                index.paths.push(recent_path);
            }
        }
        Ok(index)
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    pub fn matching<'a>(&'a self, query: &str) -> impl Iterator<Item = &'a PathBuf> {
        let needle = query.trim().to_lowercase();
        self.paths.iter().filter(move |path| {
            needle.is_empty() || path.to_string_lossy().to_lowercase().contains(&needle)
        })
    }

    pub fn record(&mut self, path: impl AsRef<Path>) -> Result<(), RecentDiagramIndexError> {
        let path = path.as_ref();
        let Some(text) = path.to_str() else {
            return Err(RecentDiagramIndexError::InvalidRecentPath(
                path.to_path_buf(),
            ));
        };
        if !path.is_absolute()
            || text.is_empty()
            || text.len() > MAX_DIAGRAM_RECENT_PATH_BYTES
            || text.chars().any(char::is_control)
        {
            return Err(RecentDiagramIndexError::InvalidRecentPath(
                path.to_path_buf(),
            ));
        }
        self.push_deduplicated(path.to_path_buf());
        Ok(())
    }

    pub fn save(&mut self, path: &Path) -> Result<(), RecentDiagramIndexError> {
        let wire = RecentDiagramIndexWire {
            version: RECENT_INDEX_VERSION,
            paths: self
                .paths
                .iter()
                .map(|path| {
                    path.to_str()
                        .expect("recorded paths are valid Unicode")
                        .to_owned()
                })
                .collect(),
        };
        let bytes = serde_json::to_vec(&wire)
            .map_err(|error| RecentDiagramIndexError::InvalidIndex(error.to_string()))?;
        if bytes.len() > MAX_DIAGRAM_RECENTS_BYTES {
            return Err(RecentDiagramIndexError::Oversized {
                max_bytes: MAX_DIAGRAM_RECENTS_BYTES,
            });
        }
        let current = read_index_file(path)?;
        let current_digest = current.as_deref().map(index_digest);
        if current_digest != self.disk_digest {
            return Err(RecentDiagramIndexError::Changed(path.to_path_buf()));
        }
        crate::common::atomic_file::save_atomic(path, &bytes).map_err(|source| {
            RecentDiagramIndexError::Io {
                path: path.to_path_buf(),
                source: io::Error::other(source),
            }
        })?;
        self.disk_digest = Some(index_digest(&bytes));
        Ok(())
    }

    fn push_deduplicated(&mut self, path: PathBuf) {
        let key = path_key(&path);
        self.paths.retain(|existing| path_key(existing) != key);
        self.paths.insert(0, path);
        self.paths.truncate(MAX_DIAGRAM_RECENTS);
    }
}

fn read_index_file(path: &Path) -> Result<Option<Vec<u8>>, RecentDiagramIndexError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(RecentDiagramIndexError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    if !metadata.file_type().is_file() {
        return Err(RecentDiagramIndexError::UnsafePath(path.to_path_buf()));
    }
    if metadata.len() > MAX_DIAGRAM_RECENTS_BYTES as u64 {
        return Err(RecentDiagramIndexError::Oversized {
            max_bytes: MAX_DIAGRAM_RECENTS_BYTES,
        });
    }
    let file = File::open(path).map_err(|source| RecentDiagramIndexError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if !file
        .metadata()
        .map_err(|source| RecentDiagramIndexError::Io {
            path: path.to_path_buf(),
            source,
        })?
        .file_type()
        .is_file()
    {
        return Err(RecentDiagramIndexError::UnsafePath(path.to_path_buf()));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take((MAX_DIAGRAM_RECENTS_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|source| RecentDiagramIndexError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() > MAX_DIAGRAM_RECENTS_BYTES {
        return Err(RecentDiagramIndexError::Oversized {
            max_bytes: MAX_DIAGRAM_RECENTS_BYTES,
        });
    }
    Ok(Some(bytes))
}

fn index_digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecentDiagramIndexWire {
    version: u8,
    #[serde(deserialize_with = "deserialize_bounded_paths")]
    paths: Vec<String>,
}

fn deserialize_bounded_paths<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct PathsVisitor;

    impl<'de> Visitor<'de> for PathsVisitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "at most {MAX_DIAGRAM_RECENTS} recent paths")
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut paths =
                Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(MAX_DIAGRAM_RECENTS));
            while let Some(path) = sequence.next_element::<String>()? {
                if paths.len() >= MAX_DIAGRAM_RECENTS {
                    return Err(de::Error::custom(format!(
                        "more than {MAX_DIAGRAM_RECENTS} recent paths"
                    )));
                }
                if path.len() > MAX_DIAGRAM_RECENT_PATH_BYTES {
                    return Err(de::Error::custom(format!(
                        "recent path exceeds the {MAX_DIAGRAM_RECENT_PATH_BYTES}-byte limit"
                    )));
                }
                paths.push(path);
            }
            Ok(paths)
        }
    }

    deserializer.deserialize_seq(PathsVisitor)
}

fn path_key(path: &Path) -> String {
    let path = path.to_str().unwrap_or_default();
    #[cfg(windows)]
    {
        path.replace('/', "\\").to_lowercase()
    }
    #[cfg(not(windows))]
    {
        path.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_DIAGRAM_RECENTS, RecentDiagramIndex, RecentDiagramIndexError};
    use std::fs;

    #[test]
    fn diagram_recent_index_deduplicates_searches_and_reopens_without_scanning() {
        let directory = tempfile::tempdir().unwrap();
        let index_path = directory.path().join("diagram-recents.json");
        let first = directory.path().join("東京.mld");
        let second = directory.path().join("architecture.mmd");
        let mut index = RecentDiagramIndex::load(&index_path).unwrap();
        index.record(&first).unwrap();
        index.record(&second).unwrap();
        index.record(&first).unwrap();
        index.save(&index_path).unwrap();

        let loaded = RecentDiagramIndex::load(&index_path).unwrap();
        assert_eq!(loaded.paths(), [first.clone(), second]);
        assert_eq!(loaded.matching("ARCHITECTURE").count(), 1);
        assert_eq!(loaded.matching("東京").count(), 1);
        assert!(
            !first.exists(),
            "recents do not create or scan document files"
        );
    }

    #[test]
    fn diagram_recent_index_preserves_newest_first_order_and_first_duplicate() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("diagram-recents.json");
        let newest = directory.path().join("newest.mld");
        let older = directory.path().join("older.mmd");
        let contents = serde_json::json!({
            "version": 1,
            "paths": [
                newest.to_str().unwrap(),
                older.to_str().unwrap(),
                newest.to_str().unwrap()
            ]
        });
        fs::write(&path, serde_json::to_vec(&contents).unwrap()).unwrap();

        let loaded = RecentDiagramIndex::load(&path).unwrap();
        assert_eq!(loaded.paths(), [newest, older]);
    }

    #[test]
    fn diagram_recent_index_does_not_replace_a_changed_or_opaque_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("diagram-recents.json");
        let first = directory.path().join("first.mld");
        let second = directory.path().join("second.mld");
        let mut index = RecentDiagramIndex::load(&path).unwrap();
        index.record(&first).unwrap();
        index.save(&path).unwrap();

        fs::write(&path, br#"{"version":9,"paths":[]}"#).unwrap();
        index.record(&second).unwrap();
        assert!(matches!(
            index.save(&path),
            Err(RecentDiagramIndexError::Changed(_))
        ));
        assert_eq!(fs::read(&path).unwrap(), br#"{"version":9,"paths":[]}"#);
    }

    #[test]
    fn diagram_recent_index_rejects_oversized_and_corrupt_data_separately() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("diagram-recents.json");
        fs::write(&path, vec![b' '; super::MAX_DIAGRAM_RECENTS_BYTES + 1]).unwrap();
        assert!(matches!(
            RecentDiagramIndex::load(&path),
            Err(RecentDiagramIndexError::Oversized { .. })
        ));

        fs::write(&path, br#"{"version":1,"paths":["relative.mld"]}"#).unwrap();
        assert!(matches!(
            RecentDiagramIndex::load(&path),
            Err(RecentDiagramIndexError::InvalidRecentPath(_))
        ));
    }

    #[test]
    fn diagram_recent_index_rejects_future_versions_and_caps_entries() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("diagram-recents.json");
        fs::write(&path, br#"{"version":2,"paths":[]}"#).unwrap();
        assert!(matches!(
            RecentDiagramIndex::load(&path),
            Err(RecentDiagramIndexError::InvalidIndex(_))
        ));

        let paths = (0..=MAX_DIAGRAM_RECENTS)
            .map(|index| format!(r#""C:\\diagrams\\{index}.mld""#))
            .collect::<Vec<_>>()
            .join(",");
        fs::write(&path, format!(r#"{{"version":1,"paths":[{paths}]}}"#)).unwrap();
        assert!(matches!(
            RecentDiagramIndex::load(&path),
            Err(RecentDiagramIndexError::InvalidIndex(_))
        ));
    }
}
