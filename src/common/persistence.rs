//! Small, typed persistence boundaries shared by domain-owned stores.
//!
//! This module owns byte reading, JSON decoding, and atomic JSON encoding. It
//! deliberately does not own validation, migration, mutation locking, backup,
//! or publication of in-memory state.

use crate::common::atomic_file::{save_atomic, save_atomic_replaceable};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::error::Error;
use std::fmt;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// The three file states that generic persistence code can identify safely.
#[derive(Clone, Debug, PartialEq, Eq)]
#[must_use]
pub enum LoadState<T> {
    Missing,
    Empty,
    Loaded(T),
}

/// Contextual failures at the shared persistence boundary.
#[derive(Debug)]
pub enum PersistenceError {
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    MalformedJson {
        path: PathBuf,
        source: serde_json::Error,
    },
    SerializeJson {
        path: PathBuf,
        source: serde_json::Error,
    },
    AtomicWrite {
        path: PathBuf,
        source: anyhow::Error,
    },
}

impl PersistenceError {
    pub fn path(&self) -> &Path {
        match self {
            Self::Read { path, .. }
            | Self::MalformedJson { path, .. }
            | Self::SerializeJson { path, .. }
            | Self::AtomicWrite { path, .. } => path,
        }
    }

    pub fn operation(&self) -> &'static str {
        match self {
            Self::Read { .. } => "read",
            Self::MalformedJson { .. } => "parse JSON",
            Self::SerializeJson { .. } => "serialize JSON",
            Self::AtomicWrite { .. } => "atomically save JSON",
        }
    }
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(formatter, "failed to read {}: {source}", path.display())
            }
            Self::MalformedJson { path, source } => write!(
                formatter,
                "failed to parse JSON from {}: {source}",
                path.display()
            ),
            Self::SerializeJson { path, source } => write!(
                formatter,
                "failed to serialize JSON for {}: {source}",
                path.display()
            ),
            Self::AtomicWrite { path, source } => write!(
                formatter,
                "failed to atomically save JSON to {}: {source}",
                path.display()
            ),
        }
    }
}

impl Error for PersistenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::MalformedJson { source, .. } | Self::SerializeJson { source, .. } => Some(source),
            Self::AtomicWrite { source, .. } => Some(source.as_ref()),
        }
    }
}

/// Read raw bytes while preserving missing and whitespace-only states.
///
/// Domain stores can use this boundary before invoking legacy decoders or
/// performing their own schema/version handling.
pub fn read_bytes(path: impl AsRef<Path>) -> Result<LoadState<Vec<u8>>, PersistenceError> {
    let path = path.as_ref();
    match std::fs::read(path) {
        Ok(bytes) if bytes.iter().all(u8::is_ascii_whitespace) => Ok(LoadState::Empty),
        Ok(bytes) => Ok(LoadState::Loaded(bytes)),
        Err(source) if source.kind() == ErrorKind::NotFound => Ok(LoadState::Missing),
        Err(source) => Err(PersistenceError::Read {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Load JSON without assigning domain meaning to missing or empty files.
pub fn load_json<T: DeserializeOwned>(
    path: impl AsRef<Path>,
) -> Result<LoadState<T>, PersistenceError> {
    let path = path.as_ref();
    match read_bytes(path)? {
        LoadState::Missing => Ok(LoadState::Missing),
        LoadState::Empty => Ok(LoadState::Empty),
        LoadState::Loaded(bytes) => serde_json::from_slice(&bytes)
            .map(LoadState::Loaded)
            .map_err(|source| PersistenceError::MalformedJson {
                path: path.to_path_buf(),
                source,
            }),
    }
}

/// Pretty-serialize JSON and commit it with the existing durable atomic writer.
pub fn save_json_atomic<T: Serialize + ?Sized>(
    path: impl AsRef<Path>,
    value: &T,
) -> Result<(), PersistenceError> {
    let path = path.as_ref();
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|source| PersistenceError::SerializeJson {
            path: path.to_path_buf(),
            source,
        })?;
    save_atomic(path, &bytes).map_err(|source| PersistenceError::AtomicWrite {
        path: path.to_path_buf(),
        source,
    })
}

/// Pretty-serialize high-frequency, replaceable JSON and commit it with an
/// atomic replacement that intentionally omits full durability syncs.
pub fn save_json_atomic_replaceable<T: Serialize + ?Sized>(
    path: impl AsRef<Path>,
    value: &T,
) -> Result<(), PersistenceError> {
    let path = path.as_ref();
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|source| PersistenceError::SerializeJson {
            path: path.to_path_buf(),
            source,
        })?;
    save_atomic_replaceable(path, &bytes).map_err(|source| PersistenceError::AtomicWrite {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::{LoadState, PersistenceError, load_json, read_bytes, save_json_atomic};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
    struct Fixture {
        name: String,
        count: u32,
    }

    #[test]
    fn missing_is_distinct_from_empty() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("missing.json");

        assert_eq!(read_bytes(&path).unwrap(), LoadState::Missing);
        assert_eq!(load_json::<Fixture>(&path).unwrap(), LoadState::Missing);
    }

    #[test]
    fn whitespace_only_is_empty_for_bytes_and_json() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("empty.json");
        std::fs::write(&path, b" \t\r\n").unwrap();

        assert_eq!(read_bytes(&path).unwrap(), LoadState::Empty);
        assert_eq!(load_json::<Fixture>(&path).unwrap(), LoadState::Empty);
    }

    #[test]
    fn raw_bytes_are_available_for_domain_decoders() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("legacy.data");
        std::fs::write(&path, b"legacy-v1").unwrap();

        assert_eq!(
            read_bytes(&path).unwrap(),
            LoadState::Loaded(b"legacy-v1".to_vec())
        );
    }

    #[test]
    fn valid_json_loads_typed_value() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("valid.json");
        std::fs::write(&path, br#"{"name":"alpha","count":2}"#).unwrap();

        assert_eq!(
            load_json(&path).unwrap(),
            LoadState::Loaded(Fixture {
                name: "alpha".into(),
                count: 2,
            })
        );
    }

    #[test]
    fn malformed_json_retains_parse_operation_and_path() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("malformed.json");
        std::fs::write(&path, b"{not-json").unwrap();

        let error = load_json::<Fixture>(&path).unwrap_err();
        assert!(matches!(error, PersistenceError::MalformedJson { .. }));
        assert_eq!(error.operation(), "parse JSON");
        assert_eq!(error.path(), path);
        assert!(error.to_string().contains("malformed.json"));
    }

    #[test]
    fn non_not_found_read_error_is_not_missing() {
        let directory = tempfile::tempdir().unwrap();

        let error = read_bytes(directory.path()).unwrap_err();
        match error {
            PersistenceError::Read { path, source } => {
                assert_eq!(path, directory.path());
                assert_ne!(source.kind(), std::io::ErrorKind::NotFound);
            }
            other => panic!("expected read error, got {other:?}"),
        }
    }

    #[test]
    fn atomic_save_uses_existing_pretty_json_shape() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("pretty.json");
        let value = Fixture {
            name: "alpha".into(),
            count: 2,
        };

        save_json_atomic(&path, &value).unwrap();

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\n  \"name\": \"alpha\",\n  \"count\": 2\n}"
        );
        assert_eq!(load_json(&path).unwrap(), LoadState::Loaded(value));
    }

    #[test]
    fn atomic_save_creates_parent_directories_and_parseable_output() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested").join("config.json");
        let value = Fixture {
            name: "nested".into(),
            count: 3,
        };

        save_json_atomic(&path, &value).unwrap();

        assert!(path.parent().unwrap().is_dir());
        let parsed: Fixture = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(parsed, value);
    }
}
