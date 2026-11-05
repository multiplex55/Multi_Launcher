//! Bounded persistence for user-authored Explorer folder lists.
//!
//! This module stores paths only. Loading a list never checks whether its
//! folders currently exist and never starts an Explorer operation.

use serde::de::{self, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

use crate::common::atomic_file::save_atomic;
use crate::platform::app_data::AppDataRoot;

use super::operation::{
    FolderPath, FolderRequestError, FolderTabRequest, MAX_REQUEST_FOLDERS, MAX_REQUEST_PATH_BYTES,
};

pub const FOLDER_LIST_FILE_NAME: &str = "explorer-folder-lists.json";
pub const CURRENT_FOLDER_LISTS_VERSION: u32 = 1;
pub const MAX_SAVED_FOLDER_LISTS: usize = 64;
pub const MAX_FOLDER_LIST_NAME_BYTES: usize = 128;
pub const MAX_FOLDER_LISTS_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FolderListId(NonZeroU64);

impl FolderListId {
    pub fn get(self) -> u64 {
        self.0.get()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedFolderList {
    id: FolderListId,
    name: String,
    folders: Vec<FolderPath>,
}

impl SavedFolderList {
    pub fn id(&self) -> FolderListId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn folders(&self) -> &[FolderPath] {
        &self.folders
    }
}

/// An owned request that cannot change when the saved list is later edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderListRunSnapshot {
    list_id: FolderListId,
    name: String,
    request: FolderTabRequest,
}

impl FolderListRunSnapshot {
    pub fn list_id(&self) -> FolderListId {
        self.list_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn request(&self) -> &FolderTabRequest {
        &self.request
    }
}

#[derive(Debug)]
pub enum FolderListError {
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    UnsafePath(PathBuf),
    Oversized {
        max_bytes: usize,
    },
    FutureVersion {
        found: u32,
    },
    UnsupportedVersion {
        found: u32,
    },
    Malformed(String),
    NameEmpty,
    NameTooLong {
        max_bytes: usize,
    },
    NameControlCharacter,
    DuplicateName,
    TooManyLists {
        max_lists: usize,
    },
    InvalidRequest(FolderRequestError),
    EmptyList,
    NotFound(FolderListId),
    InvalidOrder {
        index: usize,
        list_count: usize,
    },
    IdExhausted,
    Changed(PathBuf),
    Occupied(PathBuf),
    Encode(String),
}

impl fmt::Display for FolderListError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "could not {operation} Explorer folder lists at {}: {source}",
                path.display()
            ),
            Self::UnsafePath(path) => write!(
                formatter,
                "Explorer folder-list storage is not a regular file: {}",
                path.display()
            ),
            Self::Oversized { max_bytes } => write!(
                formatter,
                "Explorer folder lists exceed the {max_bytes}-byte limit"
            ),
            Self::FutureVersion { found } => write!(
                formatter,
                "Explorer folder lists use future format version {found}"
            ),
            Self::UnsupportedVersion { found } => write!(
                formatter,
                "Explorer folder-list format version {found} is unsupported"
            ),
            Self::Malformed(reason) => {
                write!(formatter, "Explorer folder lists are invalid: {reason}")
            }
            Self::NameEmpty => formatter.write_str("folder-list name cannot be empty"),
            Self::NameTooLong { max_bytes } => write!(
                formatter,
                "folder-list name exceeds the {max_bytes}-byte limit"
            ),
            Self::NameControlCharacter => {
                formatter.write_str("folder-list name cannot contain control characters")
            }
            Self::DuplicateName => {
                formatter.write_str("another folder list already uses that name")
            }
            Self::TooManyLists { max_lists } => {
                write!(formatter, "at most {max_lists} folder lists can be saved")
            }
            Self::InvalidRequest(error) => match error {
                FolderRequestError::Empty => formatter.write_str("folder list is empty"),
                FolderRequestError::TooManyFolders => write!(
                    formatter,
                    "folder list exceeds the {MAX_REQUEST_FOLDERS}-folder limit"
                ),
                FolderRequestError::PathBytesExceeded => write!(
                    formatter,
                    "folder list paths exceed the {MAX_REQUEST_PATH_BYTES}-byte limit"
                ),
            },
            Self::EmptyList => formatter.write_str("an empty folder list cannot be launched"),
            Self::NotFound(id) => write!(formatter, "folder list {} no longer exists", id.get()),
            Self::InvalidOrder { index, list_count } => write!(
                formatter,
                "folder-list position {index} is outside the {list_count}-list range"
            ),
            Self::IdExhausted => formatter.write_str("folder-list IDs are exhausted"),
            Self::Changed(path) => write!(
                formatter,
                "Explorer folder lists changed outside this session; refusing to overwrite {}",
                path.display()
            ),
            Self::Occupied(path) => write!(
                formatter,
                "Explorer folder-list storage appeared before initial publication: {}",
                path.display()
            ),
            Self::Encode(reason) => write!(
                formatter,
                "could not encode Explorer folder lists: {reason}"
            ),
        }
    }
}

impl Error for FolderListError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// An explicitly opened saved-list document. Opening is read-only and does
/// not create the app-data directory or rewrite a valid file.
pub struct FolderListStore {
    path: PathBuf,
    next_id: u64,
    lists: Vec<SavedFolderList>,
    disk_digest: Option<[u8; 32]>,
}

impl FolderListStore {
    pub fn open(root: &AppDataRoot) -> Result<Self, FolderListError> {
        let path = root.path().join(FOLDER_LIST_FILE_NAME);
        let Some(bytes) = read_bounded_file(&path)? else {
            return Ok(Self {
                path,
                next_id: 1,
                lists: Vec::new(),
                disk_digest: None,
            });
        };

        let version = read_format_version(&bytes)?;
        if version > CURRENT_FOLDER_LISTS_VERSION {
            return Err(FolderListError::FutureVersion { found: version });
        }
        if version != CURRENT_FOLDER_LISTS_VERSION {
            return Err(FolderListError::UnsupportedVersion { found: version });
        }
        let wire: FolderListsWire = serde_json::from_slice(&bytes)
            .map_err(|error| FolderListError::Malformed(error.to_string()))?;
        let lists = decode_lists(wire.next_id, wire.lists)?;
        Ok(Self {
            path,
            next_id: wire.next_id,
            lists,
            disk_digest: Some(digest(&bytes)),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn lists(&self) -> &[SavedFolderList] {
        &self.lists
    }

    pub fn create(
        &mut self,
        name: impl Into<String>,
        folders: Vec<FolderPath>,
    ) -> Result<FolderListId, FolderListError> {
        let name = name.into();
        validate_name(&name)?;
        validate_paths(&folders)?;
        if self.lists.len() >= MAX_SAVED_FOLDER_LISTS {
            return Err(FolderListError::TooManyLists {
                max_lists: MAX_SAVED_FOLDER_LISTS,
            });
        }
        ensure_unique_name(&self.lists, &name, None)?;

        let id = FolderListId(NonZeroU64::new(self.next_id).ok_or(FolderListError::IdExhausted)?);
        let next_id = self
            .next_id
            .checked_add(1)
            .ok_or(FolderListError::IdExhausted)?;
        let mut candidate = self.lists.clone();
        candidate.push(SavedFolderList { id, name, folders });
        self.commit(candidate, next_id)?;
        Ok(id)
    }

    pub fn update(
        &mut self,
        id: FolderListId,
        name: impl Into<String>,
        folders: Vec<FolderPath>,
    ) -> Result<(), FolderListError> {
        let Some(index) = self.lists.iter().position(|list| list.id == id) else {
            return Err(FolderListError::NotFound(id));
        };
        let name = name.into();
        validate_name(&name)?;
        validate_paths(&folders)?;
        ensure_unique_name(&self.lists, &name, Some(id))?;

        let mut candidate = self.lists.clone();
        candidate[index] = SavedFolderList { id, name, folders };
        self.commit(candidate, self.next_id)
    }

    pub fn remove(&mut self, id: FolderListId) -> Result<(), FolderListError> {
        let Some(index) = self.lists.iter().position(|list| list.id == id) else {
            return Err(FolderListError::NotFound(id));
        };
        let mut candidate = self.lists.clone();
        candidate.remove(index);
        self.commit(candidate, self.next_id)
    }

    /// Move a list to its zero-based position in the final ordering.
    pub fn reorder(&mut self, id: FolderListId, new_index: usize) -> Result<(), FolderListError> {
        let Some(old_index) = self.lists.iter().position(|list| list.id == id) else {
            return Err(FolderListError::NotFound(id));
        };
        if new_index >= self.lists.len() {
            return Err(FolderListError::InvalidOrder {
                index: new_index,
                list_count: self.lists.len(),
            });
        }
        if old_index == new_index {
            return Ok(());
        }
        let mut candidate = self.lists.clone();
        let list = candidate.remove(old_index);
        candidate.insert(new_index, list);
        self.commit(candidate, self.next_id)
    }

    pub fn launch_snapshot(
        &self,
        id: FolderListId,
    ) -> Result<FolderListRunSnapshot, FolderListError> {
        let list = self
            .lists
            .iter()
            .find(|list| list.id == id)
            .ok_or(FolderListError::NotFound(id))?;
        if list.folders.is_empty() {
            return Err(FolderListError::EmptyList);
        }
        let request =
            FolderTabRequest::new(list.folders.clone()).map_err(FolderListError::InvalidRequest)?;
        Ok(FolderListRunSnapshot {
            list_id: list.id,
            name: list.name.clone(),
            request,
        })
    }

    fn commit(
        &mut self,
        candidate: Vec<SavedFolderList>,
        next_id: u64,
    ) -> Result<(), FolderListError> {
        self.commit_with_initial_publisher(candidate, next_id, publish_initial_noclobber)
    }

    fn commit_with_initial_publisher(
        &mut self,
        candidate: Vec<SavedFolderList>,
        next_id: u64,
        publish_initial: impl FnOnce(&Path, &[u8]) -> Result<(), FolderListError>,
    ) -> Result<(), FolderListError> {
        validate_document(next_id, &candidate)?;
        let bytes = encode_document(next_id, &candidate)?;

        let current_bytes = read_bounded_file(&self.path)?;
        if current_bytes.as_deref().map(digest) != self.disk_digest {
            return Err(FolderListError::Changed(self.path.clone()));
        }

        if self.disk_digest.is_some() {
            save_atomic(&self.path, &bytes).map_err(|error| FolderListError::Io {
                operation: "save",
                path: self.path.clone(),
                source: io::Error::other(error),
            })?;
        } else {
            publish_initial(&self.path, &bytes)?;
        }

        self.lists = candidate;
        self.next_id = next_id;
        self.disk_digest = Some(digest(&bytes));
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FolderListsWire {
    version: u32,
    next_id: u64,
    #[serde(deserialize_with = "deserialize_bounded_lists")]
    lists: Vec<FolderListWire>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FolderListWire {
    id: u64,
    name: String,
    #[serde(deserialize_with = "deserialize_bounded_paths")]
    folders: Vec<String>,
}

fn decode_lists(
    next_id: u64,
    wire_lists: Vec<FolderListWire>,
) -> Result<Vec<SavedFolderList>, FolderListError> {
    if next_id == 0 {
        return Err(FolderListError::Malformed(
            "the next list ID must be nonzero".into(),
        ));
    }
    if wire_lists.len() > MAX_SAVED_FOLDER_LISTS {
        return Err(FolderListError::TooManyLists {
            max_lists: MAX_SAVED_FOLDER_LISTS,
        });
    }

    let mut ids = HashSet::with_capacity(wire_lists.len());
    let mut names = HashSet::with_capacity(wire_lists.len());
    let mut max_id = 0;
    let mut lists = Vec::with_capacity(wire_lists.len());
    for wire in wire_lists {
        let id = FolderListId(
            NonZeroU64::new(wire.id)
                .ok_or_else(|| FolderListError::Malformed("a list ID must be nonzero".into()))?,
        );
        if !ids.insert(id) {
            return Err(FolderListError::Malformed("duplicate list ID".into()));
        }
        validate_loaded_name(&wire.name)?;
        if !names.insert(name_key(&wire.name)) {
            return Err(FolderListError::Malformed("duplicate list name".into()));
        }
        let mut folders = Vec::with_capacity(wire.folders.len());
        for value in wire.folders {
            folders.push(FolderPath::parse(&value).map_err(|error| {
                FolderListError::Malformed(format!("a saved folder path is invalid: {error:?}"))
            })?);
        }
        validate_paths(&folders).map_err(|error| {
            FolderListError::Malformed(format!("a saved folder request is invalid: {error}"))
        })?;
        max_id = max_id.max(id.get());
        lists.push(SavedFolderList {
            id,
            name: wire.name,
            folders,
        });
    }
    if next_id <= max_id {
        return Err(FolderListError::Malformed(
            "the next list ID must exceed every saved ID".into(),
        ));
    }
    Ok(lists)
}

fn validate_document(next_id: u64, lists: &[SavedFolderList]) -> Result<(), FolderListError> {
    if next_id == 0 {
        return Err(FolderListError::IdExhausted);
    }
    if lists.len() > MAX_SAVED_FOLDER_LISTS {
        return Err(FolderListError::TooManyLists {
            max_lists: MAX_SAVED_FOLDER_LISTS,
        });
    }
    let mut ids = HashSet::with_capacity(lists.len());
    let mut names = HashSet::with_capacity(lists.len());
    let mut max_id = 0;
    for list in lists {
        if !ids.insert(list.id) {
            return Err(FolderListError::Malformed("duplicate list ID".into()));
        }
        validate_name(&list.name)?;
        if !names.insert(name_key(&list.name)) {
            return Err(FolderListError::DuplicateName);
        }
        validate_paths(&list.folders)?;
        max_id = max_id.max(list.id.get());
    }
    if next_id <= max_id {
        return Err(FolderListError::Malformed(
            "the next list ID must exceed every saved ID".into(),
        ));
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<(), FolderListError> {
    if name.trim().is_empty() {
        return Err(FolderListError::NameEmpty);
    }
    if name.len() > MAX_FOLDER_LIST_NAME_BYTES {
        return Err(FolderListError::NameTooLong {
            max_bytes: MAX_FOLDER_LIST_NAME_BYTES,
        });
    }
    if name.chars().any(char::is_control) {
        return Err(FolderListError::NameControlCharacter);
    }
    Ok(())
}

fn validate_loaded_name(name: &str) -> Result<(), FolderListError> {
    validate_name(name).map_err(|error| {
        FolderListError::Malformed(format!("a saved list name is invalid: {error}"))
    })
}

fn ensure_unique_name(
    lists: &[SavedFolderList],
    candidate: &str,
    except: Option<FolderListId>,
) -> Result<(), FolderListError> {
    let key = name_key(candidate);
    if lists
        .iter()
        .any(|list| Some(list.id) != except && name_key(&list.name) == key)
    {
        return Err(FolderListError::DuplicateName);
    }
    Ok(())
}

fn name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

fn validate_paths(folders: &[FolderPath]) -> Result<(), FolderListError> {
    if folders.is_empty() {
        return Ok(());
    }
    FolderTabRequest::new(folders.to_vec())
        .map(|_| ())
        .map_err(FolderListError::InvalidRequest)
}

#[derive(Serialize)]
struct FolderListsWireRef<'a> {
    version: u32,
    next_id: u64,
    lists: Vec<FolderListWireRef<'a>>,
}

#[derive(Serialize)]
struct FolderListWireRef<'a> {
    id: u64,
    name: &'a str,
    folders: Vec<&'a str>,
}

fn encode_document(next_id: u64, lists: &[SavedFolderList]) -> Result<Vec<u8>, FolderListError> {
    let wire = FolderListsWireRef {
        version: CURRENT_FOLDER_LISTS_VERSION,
        next_id,
        lists: lists
            .iter()
            .map(|list| FolderListWireRef {
                id: list.id.get(),
                name: &list.name,
                folders: list.folders.iter().map(FolderPath::as_str).collect(),
            })
            .collect(),
    };
    let mut output = CappedVecWriter::new(MAX_FOLDER_LISTS_BYTES);
    if let Err(error) = serde_json::to_writer(&mut output, &wire) {
        if output.exceeded_limit {
            return Err(FolderListError::Oversized {
                max_bytes: MAX_FOLDER_LISTS_BYTES,
            });
        }
        return Err(FolderListError::Encode(error.to_string()));
    }
    Ok(output.bytes)
}

struct CappedVecWriter {
    bytes: Vec<u8>,
    max_bytes: usize,
    exceeded_limit: bool,
}

impl CappedVecWriter {
    fn new(max_bytes: usize) -> Self {
        Self {
            bytes: Vec::new(),
            max_bytes,
            exceeded_limit: false,
        }
    }
}

impl Write for CappedVecWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let Some(new_len) = self.bytes.len().checked_add(buffer.len()) else {
            self.exceeded_limit = true;
            return Err(io::Error::other("encoded document size overflow"));
        };
        if new_len > self.max_bytes {
            self.exceeded_limit = true;
            return Err(io::Error::other("encoded document exceeds size limit"));
        }
        if self.bytes.capacity() < new_len {
            self.bytes
                .try_reserve_exact(new_len - self.bytes.len())
                .map_err(io::Error::other)?;
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn read_bounded_file(path: &Path) -> Result<Option<Vec<u8>>, FolderListError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(FolderListError::Io {
                operation: "inspect",
                path: path.to_path_buf(),
                source,
            });
        }
    };
    if !metadata.file_type().is_file() {
        return Err(FolderListError::UnsafePath(path.to_path_buf()));
    }
    if metadata.len() > MAX_FOLDER_LISTS_BYTES as u64 {
        return Err(FolderListError::Oversized {
            max_bytes: MAX_FOLDER_LISTS_BYTES,
        });
    }
    let file = File::open(path).map_err(|source| FolderListError::Io {
        operation: "open",
        path: path.to_path_buf(),
        source,
    })?;
    let opened_metadata = file.metadata().map_err(|source| FolderListError::Io {
        operation: "inspect opened file",
        path: path.to_path_buf(),
        source,
    })?;
    if !opened_metadata.file_type().is_file() {
        return Err(FolderListError::UnsafePath(path.to_path_buf()));
    }
    let mut bytes =
        Vec::with_capacity(opened_metadata.len().min(MAX_FOLDER_LISTS_BYTES as u64) as usize);
    file.take((MAX_FOLDER_LISTS_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|source| FolderListError::Io {
            operation: "read",
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() > MAX_FOLDER_LISTS_BYTES {
        return Err(FolderListError::Oversized {
            max_bytes: MAX_FOLDER_LISTS_BYTES,
        });
    }
    Ok(Some(bytes))
}

fn publish_initial_noclobber(path: &Path, bytes: &[u8]) -> Result<(), FolderListError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|source| FolderListError::Io {
        operation: "create app-data directory",
        path: parent.to_path_buf(),
        source,
    })?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".explorer-folder-lists-")
        .tempfile_in(parent)
        .map_err(|source| FolderListError::Io {
            operation: "create initial temporary file",
            path: path.to_path_buf(),
            source,
        })?;
    temporary
        .write_all(bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|source| FolderListError::Io {
            operation: "write initial temporary file",
            path: path.to_path_buf(),
            source,
        })?;
    temporary.persist_noclobber(path).map_err(|error| {
        if error.error.kind() == io::ErrorKind::AlreadyExists {
            FolderListError::Occupied(path.to_path_buf())
        } else {
            FolderListError::Io {
                operation: "publish initial file",
                path: path.to_path_buf(),
                source: error.error,
            }
        }
    })?;
    Ok(())
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn read_format_version(bytes: &[u8]) -> Result<u32, FolderListError> {
    struct VersionVisitor;

    impl<'de> Visitor<'de> for VersionVisitor {
        type Value = u32;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a folder-list document with one integer version")
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            let mut version = None;
            while let Some(key) = map.next_key::<String>()? {
                if key == "version" {
                    if version.is_some() {
                        return Err(de::Error::duplicate_field("version"));
                    }
                    version = Some(map.next_value::<u32>()?);
                } else {
                    map.next_value::<IgnoredAny>()?;
                }
            }
            version.ok_or_else(|| de::Error::missing_field("version"))
        }
    }

    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let version = deserializer
        .deserialize_map(VersionVisitor)
        .map_err(|error| FolderListError::Malformed(error.to_string()))?;
    deserializer
        .end()
        .map_err(|error| FolderListError::Malformed(error.to_string()))?;
    Ok(version)
}

fn deserialize_bounded_lists<'de, D>(deserializer: D) -> Result<Vec<FolderListWire>, D::Error>
where
    D: Deserializer<'de>,
{
    struct ListsVisitor;

    impl<'de> Visitor<'de> for ListsVisitor {
        type Value = Vec<FolderListWire>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(
                formatter,
                "at most {MAX_SAVED_FOLDER_LISTS} saved folder lists"
            )
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut lists = Vec::with_capacity(
                sequence
                    .size_hint()
                    .unwrap_or(0)
                    .min(MAX_SAVED_FOLDER_LISTS),
            );
            while let Some(list) = sequence.next_element::<FolderListWire>()? {
                if lists.len() >= MAX_SAVED_FOLDER_LISTS {
                    return Err(de::Error::custom(format!(
                        "more than {MAX_SAVED_FOLDER_LISTS} saved folder lists"
                    )));
                }
                lists.push(list);
            }
            Ok(lists)
        }
    }

    deserializer.deserialize_seq(ListsVisitor)
}

fn deserialize_bounded_paths<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct PathsVisitor;

    impl<'de> Visitor<'de> for PathsVisitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "at most {MAX_REQUEST_FOLDERS} folder paths")
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut paths =
                Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(MAX_REQUEST_FOLDERS));
            while let Some(path) = sequence.next_element::<String>()? {
                if paths.len() >= MAX_REQUEST_FOLDERS {
                    return Err(de::Error::custom(format!(
                        "more than {MAX_REQUEST_FOLDERS} folder paths"
                    )));
                }
                paths.push(path);
            }
            Ok(paths)
        }
    }

    deserializer.deserialize_seq(PathsVisitor)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_root(directory: &tempfile::TempDir) -> AppDataRoot {
        AppDataRoot::from_path(directory.path().join("app-data"))
    }

    fn folder(value: &str) -> FolderPath {
        FolderPath::parse(value).expect("valid fixture folder")
    }

    fn list_path(root: &AppDataRoot) -> PathBuf {
        root.path().join(FOLDER_LIST_FILE_NAME)
    }

    #[test]
    fn explorer_folder_list_crud_keeps_high_water_and_owned_launch_snapshot() {
        let directory = tempfile::tempdir().expect("isolated folder-list directory");
        let root = make_root(&directory);
        let path = list_path(&root);
        let mut store = FolderListStore::open(&root).expect("open missing list file");
        assert!(!path.exists(), "opening must not eagerly create the file");

        let first = store
            .create(
                "Research 東京",
                vec![
                    folder(r"C:\Résumé projects"),
                    folder(r"C:\東京"),
                    folder(r"C:\Résumé projects"),
                ],
            )
            .expect("create ordered list");
        let saved_snapshot = store
            .launch_snapshot(first)
            .expect("capture owned launch request");
        let empty = store
            .create("Empty", Vec::new())
            .expect("create empty list");
        assert!(matches!(
            store.launch_snapshot(empty),
            Err(FolderListError::EmptyList)
        ));

        store.reorder(empty, 0).expect("move empty list to front");
        store
            .update(
                first,
                "Research v2 東京",
                vec![
                    folder(r"D:\Unavailable path"),
                    folder(r"D:\Unavailable path"),
                ],
            )
            .expect("update without checking current folder availability");
        store.remove(empty).expect("remove empty list");
        let third = store
            .create("After removal", Vec::new())
            .expect("create after remove");
        assert_eq!(third.get(), empty.get() + 1, "removed IDs are never reused");

        assert_eq!(saved_snapshot.name(), "Research 東京");
        assert_eq!(
            saved_snapshot.request().folders(),
            &[
                folder(r"C:\Résumé projects"),
                folder(r"C:\東京"),
                folder(r"C:\Résumé projects")
            ]
        );

        let reopened = FolderListStore::open(&root).expect("reopen saved lists");
        assert_eq!(
            reopened
                .lists()
                .iter()
                .map(SavedFolderList::id)
                .collect::<Vec<_>>(),
            vec![first, third]
        );
        assert_eq!(reopened.lists()[0].name(), "Research v2 東京");
        assert_eq!(
            reopened.lists()[0].folders(),
            &[
                folder(r"D:\Unavailable path"),
                folder(r"D:\Unavailable path")
            ]
        );
        assert_eq!(
            reopened
                .launch_snapshot(first)
                .expect("updated launch request")
                .request()
                .folders(),
            &[
                folder(r"D:\Unavailable path"),
                folder(r"D:\Unavailable path")
            ]
        );
    }

    #[test]
    fn explorer_folder_list_rejects_invalid_edits_without_changing_saved_state() {
        let directory = tempfile::tempdir().expect("isolated folder-list directory");
        let root = make_root(&directory);
        let mut store = FolderListStore::open(&root).expect("open missing file");
        let id = store
            .create("Alpha", vec![folder(r"C:\one")])
            .expect("create initial list");
        let before = store.lists().to_vec();
        let bytes = fs::read(store.path()).expect("saved baseline bytes");

        assert!(matches!(
            store.create("  ", Vec::new()),
            Err(FolderListError::NameEmpty)
        ));
        assert!(matches!(
            store.create("x".repeat(MAX_FOLDER_LIST_NAME_BYTES + 1), Vec::new()),
            Err(FolderListError::NameTooLong { .. })
        ));
        assert!(matches!(
            store.create("bad\nname", Vec::new()),
            Err(FolderListError::NameControlCharacter)
        ));
        assert!(matches!(
            store.create("alpha", Vec::new()),
            Err(FolderListError::DuplicateName)
        ));
        assert!(matches!(
            store.update(
                id,
                "Duplicate",
                vec![folder(r"C:\one"); MAX_REQUEST_FOLDERS + 1]
            ),
            Err(FolderListError::InvalidRequest(
                FolderRequestError::TooManyFolders
            ))
        ));

        let long = folder(&format!("C:\\{}", "é".repeat(16_000)));
        assert!(matches!(
            store.update(id, "Duplicate", vec![long.clone(); 17]),
            Err(FolderListError::InvalidRequest(
                FolderRequestError::PathBytesExceeded
            ))
        ));
        assert_eq!(store.lists(), before);
        assert_eq!(
            fs::read(store.path()).expect("unchanged saved baseline"),
            bytes
        );
    }

    #[test]
    fn explorer_folder_list_caps_list_count_and_encoded_document_size() {
        let directory = tempfile::tempdir().expect("isolated folder-list directory");
        let root = make_root(&directory);
        let mut store = FolderListStore::open(&root).expect("open missing file");
        for index in 0..MAX_SAVED_FOLDER_LISTS {
            store
                .create(format!("list-{index}"), Vec::new())
                .expect("create within list cap");
        }
        let before = store.lists().to_vec();
        assert!(matches!(
            store.create("too many", Vec::new()),
            Err(FolderListError::TooManyLists { .. })
        ));
        assert_eq!(store.lists(), before);

        let large_directory = tempfile::tempdir().expect("large list fixture");
        let large_root = make_root(&large_directory);
        let mut large_store = FolderListStore::open(&large_root).expect("open large fixture");
        let repeated_folder = folder(&format!("C:\\{}", "a".repeat(16_000)));
        let folders = vec![repeated_folder; 16];
        for index in 0..4 {
            large_store
                .create(format!("bulk-{index}"), folders.clone())
                .expect("four bounded lists fit the encoded document cap");
        }
        let before = large_store.lists().to_vec();
        let bytes = fs::read(large_store.path()).expect("current bounded bytes");
        assert!(matches!(
            large_store.create("bulk-4", folders),
            Err(FolderListError::Oversized { .. })
        ));
        assert_eq!(large_store.lists(), before);
        assert_eq!(
            fs::read(large_store.path()).expect("preserved bytes"),
            bytes
        );
    }

    #[test]
    fn explorer_folder_list_preserves_missing_future_malformed_and_unsafe_files() {
        let directory = tempfile::tempdir().expect("isolated folder-list directory");
        let root = make_root(&directory);
        let path = list_path(&root);
        fs::create_dir_all(root.path()).expect("create fixture app-data directory");

        let future = br#"{"version":999,"future_field":{"opaque":true},"next_id":1,"lists":[]}"#;
        fs::write(&path, future).expect("write future fixture");
        assert!(matches!(
            FolderListStore::open(&root),
            Err(FolderListError::FutureVersion { found: 999 })
        ));
        assert_eq!(fs::read(&path).expect("future bytes preserved"), future);

        let malformed = b"{\"version\":1,\"lists\":[";
        fs::write(&path, malformed).expect("write malformed fixture");
        assert!(matches!(
            FolderListStore::open(&root),
            Err(FolderListError::Malformed(_))
        ));
        assert_eq!(
            fs::read(&path).expect("malformed bytes preserved"),
            malformed
        );

        let unknown = br#"{"version":1,"next_id":1,"lists":[],"future_field":"opaque"}"#;
        fs::write(&path, unknown).expect("write unknown-field fixture");
        assert!(matches!(
            FolderListStore::open(&root),
            Err(FolderListError::Malformed(_))
        ));
        assert_eq!(fs::read(&path).expect("unknown field preserved"), unknown);

        fs::remove_file(&path).expect("remove file before unsafe fixture");
        fs::create_dir(&path).expect("create directory at storage path");
        assert!(matches!(
            FolderListStore::open(&root),
            Err(FolderListError::UnsafePath(_))
        ));
        assert!(path.is_dir(), "unsafe target must not be replaced");
    }

    #[test]
    fn explorer_folder_list_rejects_oversized_input_and_initial_no_clobber() {
        let directory = tempfile::tempdir().expect("isolated folder-list directory");
        let root = make_root(&directory);
        fs::create_dir_all(root.path()).expect("create fixture app-data directory");
        let path = list_path(&root);
        fs::write(&path, vec![b' '; MAX_FOLDER_LISTS_BYTES + 1]).expect("write oversized fixture");
        assert!(matches!(
            FolderListStore::open(&root),
            Err(FolderListError::Oversized { .. })
        ));

        let other_directory = tempfile::tempdir().expect("no-clobber fixture");
        let occupied = other_directory.path().join(FOLDER_LIST_FILE_NAME);
        fs::write(&occupied, b"original").expect("occupy initial destination");
        assert!(matches!(
            publish_initial_noclobber(&occupied, b"replacement"),
            Err(FolderListError::Occupied(_))
        ));
        assert_eq!(
            fs::read(&occupied).expect("read existing destination"),
            b"original"
        );
        assert_eq!(
            fs::read_dir(other_directory.path())
                .expect("read temporary directory")
                .count(),
            1,
            "failed no-clobber publication cleans its temporary file"
        );
    }

    #[test]
    fn explorer_folder_list_initial_publication_race_preserves_store_baseline() {
        let directory = tempfile::tempdir().expect("isolated folder-list directory");
        let root = make_root(&directory);
        let mut store = FolderListStore::open(&root).expect("open missing file");
        let path = store.path().to_path_buf();
        let id = FolderListId(NonZeroU64::new(1).expect("nonzero fixture list ID"));
        let candidate = vec![SavedFolderList {
            id,
            name: "first writer".into(),
            folders: vec![folder(r"C:\first")],
        }];
        let race_bytes = b"external initial file";

        let result = store.commit_with_initial_publisher(candidate, 2, |path, bytes| {
            let parent = path.parent().expect("list file parent");
            fs::create_dir_all(parent).map_err(|source| FolderListError::Io {
                operation: "create race fixture directory",
                path: parent.to_path_buf(),
                source,
            })?;
            fs::write(path, race_bytes).map_err(|source| FolderListError::Io {
                operation: "publish race fixture file",
                path: path.to_path_buf(),
                source,
            })?;
            publish_initial_noclobber(path, bytes)
        });

        assert!(matches!(result, Err(FolderListError::Occupied(_))));
        assert!(
            store.lists().is_empty(),
            "failed publication keeps prior in-memory state"
        );
        assert_eq!(store.next_id, 1, "failed publication keeps ID high-water");
        assert_eq!(
            store.disk_digest, None,
            "failed publication keeps disk baseline"
        );
        assert_eq!(fs::read(path).expect("read racing file"), race_bytes);
    }

    #[test]
    fn explorer_folder_list_stale_writer_preserves_saved_and_in_memory_baselines() {
        let directory = tempfile::tempdir().expect("isolated folder-list directory");
        let root = make_root(&directory);
        let mut first = FolderListStore::open(&root).expect("open first store");
        let mut stale = FolderListStore::open(&root).expect("open stale store");
        first
            .create("Writer A", vec![folder(r"C:\first")])
            .expect("write first store");
        let stale_before = stale.lists().to_vec();
        let saved_bytes = fs::read(first.path()).expect("first saved document");

        assert!(matches!(
            stale.create("Writer B", vec![folder(r"C:\second")]),
            Err(FolderListError::Changed(_))
        ));
        assert_eq!(stale.lists(), stale_before);
        assert_eq!(
            fs::read(first.path()).expect("preserved saved file"),
            saved_bytes
        );

        let mut replaced = FolderListStore::open(&root).expect("open replacement baseline");
        let original_path = replaced.path().to_path_buf();
        let mut external = fs::read(&original_path).expect("external current bytes");
        external.extend_from_slice(b" ");
        fs::write(&original_path, &external).expect("simulate external rewrite");
        assert!(matches!(
            replaced.create("Writer C", Vec::new()),
            Err(FolderListError::Changed(_))
        ));
        assert_eq!(replaced.lists().len(), 1);
        assert_eq!(
            fs::read(&original_path).expect("external bytes retained"),
            external
        );
    }
}
