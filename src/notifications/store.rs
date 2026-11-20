//! Bounded profile-owned Inbox history and preferences.
use super::Inbox;
use crate::common::atomic_file::save_atomic;
use crate::persistence::StoreHealth;
use crate::platform::app_data::AppDataRoot;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};

pub(crate) const HISTORY_FILE: &str = "notification_inbox.json";
const VERSION: u8 = 1;
const MAX_HISTORY_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct History {
    version: u8,
    inbox: Inbox,
}

enum Loaded {
    Missing,
    Empty,
    Invalid,
    Unsupported(u8),
    History(Box<Inbox>),
}

fn read(path: &Path) -> std::io::Result<Loaded> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Loaded::Missing),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take(MAX_HISTORY_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_HISTORY_BYTES {
        return Ok(Loaded::Invalid);
    }
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(Loaded::Empty);
    }
    let Ok(history) = serde_json::from_slice::<History>(&bytes) else {
        return Ok(Loaded::Invalid);
    };
    if history.version != VERSION {
        return Ok(Loaded::Unsupported(history.version));
    }
    Ok(if history.inbox.valid_persistent_snapshot() {
        Loaded::History(Box::new(history.inbox))
    } else {
        Loaded::Invalid
    })
}

pub(crate) fn probe_history(path: &Path) -> StoreHealth {
    match read(path) {
        Ok(Loaded::Missing) => StoreHealth::Missing,
        Ok(Loaded::Empty) => StoreHealth::Empty,
        Ok(Loaded::History(_)) => StoreHealth::Healthy,
        Ok(Loaded::Invalid) => StoreHealth::Malformed {
            message: "Invalid or oversized notification history.".into(),
        },
        Ok(Loaded::Unsupported(version)) => StoreHealth::UnsupportedSchema {
            version: version.to_string(),
        },
        Err(_) => StoreHealth::Unreadable {
            message: "Could not read notification history.".into(),
        },
    }
}

pub(crate) struct InboxStore {
    path: Option<PathBuf>,
    inbox: Inbox,
    write_generation: u64,
}

impl InboxStore {
    pub(crate) fn open(profile: &AppDataRoot) -> Result<Self> {
        let path = profile.path().join(HISTORY_FILE);
        let inbox = match read(&path).context("Read notification history")? {
            Loaded::Missing => Inbox::default(),
            Loaded::Empty | Loaded::Invalid => {
                bail!("Notification history is invalid or exceeds its size limit.")
            }
            Loaded::Unsupported(version) => {
                bail!("Notification history uses unsupported version {version}.")
            }
            Loaded::History(inbox) => *inbox,
        };
        Ok(Self {
            path: Some(path),
            inbox,
            write_generation: 0,
        })
    }

    /// An isolated live Inbox for an unavailable profile. No history is written.
    pub(crate) fn memory_only() -> Self {
        Self {
            path: None,
            inbox: Inbox::default(),
            write_generation: 0,
        }
    }

    pub(crate) fn inbox(&self) -> &Inbox {
        &self.inbox
    }

    pub(super) fn write_generation(&self) -> u64 {
        self.write_generation
    }

    pub(crate) fn transact<T>(&mut self, change: impl FnOnce(&mut Inbox) -> T) -> Result<T> {
        self.transact_with_save(change, save_atomic)
    }

    fn transact_with_save<T>(
        &mut self,
        change: impl FnOnce(&mut Inbox) -> T,
        save: impl FnOnce(&Path, &[u8]) -> Result<()>,
    ) -> Result<T> {
        let mut candidate = self.inbox.clone();
        let result = change(&mut candidate);
        if let Some(path) = &self.path {
            let next = candidate.persistent_snapshot();
            if next != self.inbox.persistent_snapshot() {
                ensure!(
                    next.valid_persistent_snapshot(),
                    "Invalid notification history change."
                );
                let bytes = serde_json::to_vec(&History {
                    version: VERSION,
                    inbox: next,
                })
                .context("Encode notification history")?;
                ensure!(
                    bytes.len() as u64 <= MAX_HISTORY_BYTES,
                    "Notification history reached its size limit."
                );
                save(path, &bytes).context("Save notification history")?;
                self.write_generation = self.write_generation.saturating_add(1);
            }
        }
        self.inbox = candidate;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notifications::{Category, FactId, Ingest, InternalFact, Status};

    fn add_launcher(inbox: &mut Inbox, sequence: u64) -> Ingest {
        inbox.ingest_internal(
            FactId::Launcher {
                session: [7; 16],
                sequence,
            },
            12,
            Category::Information,
            Status::Current,
            InternalFact::LauncherInfo,
            None,
            12,
        )
    }

    #[test]
    fn notification_inbox_store_retains_typed_history_and_keeps_external_session_private() {
        let dir = tempfile::tempdir().unwrap();
        let profile = AppDataRoot::from_path(dir.path());
        let path = dir.path().join(HISTORY_FILE);
        let mut store = InboxStore::open(&profile).unwrap();
        assert!(!path.exists());
        assert!(matches!(
            store.transact(|inbox| add_launcher(inbox, 1)).unwrap(),
            Ingest::Added { .. }
        ));
        let mut preferences = store.inbox().settings().clone();
        preferences.external_enabled = true;
        assert!(!preferences.retain_external_text);
        preferences.retain_external_text = true;
        store
            .transact(|inbox| inbox.set_settings(preferences, 12))
            .unwrap()
            .unwrap();
        store
            .transact(|inbox| {
                inbox.ingest_windows(
                    FactId::Windows {
                        app: "Named.Synthetic.Application".into(),
                        notification: 7,
                        created: 123,
                    },
                    1,
                    12,
                    Category::Information,
                    Status::Current,
                    Some("SESSION_TITLE_SENTINEL"),
                    Some("SESSION_BODY_SENTINEL"),
                    12,
                )
            })
            .unwrap();
        assert_eq!(store.inbox().records().len(), 2);
        let json = std::fs::read_to_string(&path).unwrap();
        assert!(!json.contains("SESSION_TITLE_SENTINEL"));
        assert!(!json.contains("SESSION_BODY_SENTINEL"));
        assert!(!json.contains("Named.Synthetic.Application"));
        let reopened = InboxStore::open(&profile).unwrap();
        assert_eq!(reopened.inbox().records().len(), 1);
        assert!(matches!(probe_history(&path), StoreHealth::Healthy));
    }

    #[test]
    fn notification_inbox_store_failed_write_and_malformed_input_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let profile = AppDataRoot::from_path(dir.path());
        let path = dir.path().join(HISTORY_FILE);
        let mut store = InboxStore::open(&profile).unwrap();
        store.transact(|inbox| add_launcher(inbox, 1)).unwrap();
        let before = std::fs::read(&path).unwrap();
        let result = store.transact_with_save(
            |inbox| add_launcher(inbox, 2),
            |_, _| Err(anyhow::anyhow!("fixture write failure")),
        );
        assert!(result.is_err());
        assert_eq!(store.inbox().records().len(), 1);
        assert_eq!(std::fs::read(&path).unwrap(), before);

        std::fs::write(&path, b"malformed notification fixture").unwrap();
        assert!(InboxStore::open(&profile).is_err());
        let mut memory = InboxStore::memory_only();
        memory.transact(|inbox| add_launcher(inbox, 1)).unwrap();
        assert_eq!(memory.inbox().records().len(), 1);
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"malformed notification fixture"
        );
        assert!(matches!(
            probe_history(&path),
            StoreHealth::Malformed { .. }
        ));
    }
}
