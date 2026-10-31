//! Deterministic persisted datasets for bounded C07 startup measurements.
//!
//! The generator writes the application's current JSON stores and Markdown
//! notes into a `TrackCWorkspace`-owned temporary profile. Receipts contain
//! item counts, actual file-content byte totals, and hashes of synthetic
//! fixture identities only; they never include profile paths or content.

use super::{track_c_workloads, workloads};
use crate::actions::save_actions;
use crate::performance::track_c_workloads::TrackCWorkspace;
use crate::plugins::snippets::{SnippetEntry, save_snippets};
use crate::settings::Settings;
use serde::Serialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const HISTORY_SEED: u64 = 0x4d37_0003;
const SNIPPET_SEED: u64 = 0x4d37_0005;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProfileScale {
    Small,
    Large,
}

impl ProfileScale {
    pub(crate) fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "small" => Ok(Self::Small),
            "large" => Ok(Self::Large),
            _ => Err("profile scale must be `small` or `large`"),
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Large => "large",
        }
    }

    pub(crate) const fn counts(self) -> ProfileCounts {
        match self {
            Self::Small => ProfileCounts {
                actions: 100,
                notes: 100,
                snippets: 100,
                history: 100,
                indexed_files: 32,
            },
            Self::Large => ProfileCounts {
                actions: 10_000,
                notes: 2_000,
                snippets: 5_000,
                history: 10_000,
                indexed_files: 2_000,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ProfileCounts {
    pub actions: usize,
    pub notes: usize,
    pub snippets: usize,
    pub history: usize,
    pub indexed_files: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub(crate) struct ProfileBytes {
    pub actions_json: u64,
    pub notes_markdown: u64,
    pub snippets_json: u64,
    pub history_json: u64,
    pub settings_json: u64,
    pub indexed_file_contents: u64,
    pub total_file_contents: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ProfileFileCounts {
    pub actions_json: u64,
    pub note_markdown: u64,
    pub snippets_json: u64,
    pub history_json: u64,
    pub settings_json: u64,
    pub indexed_files: u64,
    pub total_files: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ProfileSignatures {
    pub actions: u64,
    pub notes: u64,
    pub snippets: u64,
    pub history: u64,
    pub indexed_tree: u64,
    pub action_last_sentinel: u64,
    pub note_last_sentinel: u64,
    pub snippet_last_sentinel: u64,
    pub history_last_sentinel: u64,
    pub index_last_sentinel: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ProfileReceipt {
    pub schema: &'static str,
    pub profile: &'static str,
    pub counts: ProfileCounts,
    pub files: ProfileFileCounts,
    pub bytes: ProfileBytes,
    pub signatures_fnv64: ProfileSignatures,
    pub configured_index_roots: usize,
    pub paste_next_hotkey_disabled: bool,
    pub paste_cancel_hotkey_disabled: bool,
}

impl ProfileReceipt {
    pub(crate) fn json_line(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("profile receipt is serializable")
    }
}

pub(crate) struct StartupProfile {
    root: PathBuf,
    actions_path: PathBuf,
    settings_path: PathBuf,
    snippets_path: PathBuf,
    history_path: PathBuf,
    index_root: PathBuf,
    receipt: ProfileReceipt,
}

impl StartupProfile {
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn actions_path(&self) -> &Path {
        &self.actions_path
    }

    pub(crate) fn settings_path(&self) -> &Path {
        &self.settings_path
    }

    pub(crate) fn snippets_path(&self) -> &Path {
        &self.snippets_path
    }

    pub(crate) fn history_path(&self) -> &Path {
        &self.history_path
    }

    pub(crate) fn index_root(&self) -> &Path {
        &self.index_root
    }

    pub(crate) fn receipt(&self) -> &ProfileReceipt {
        &self.receipt
    }
}

/// Write one complete profile into an already-owned Track C workspace.
/// Existing fixture content is rejected so a partial prior profile cannot be
/// silently mixed into a new workload.
pub(crate) fn materialize(
    workspace: &mut TrackCWorkspace,
    scale: ProfileScale,
) -> anyhow::Result<StartupProfile> {
    let root = workspace.root().join("c07-startup-profile");
    workspace
        .scope_to_profile(&root)
        .map_err(|error| anyhow::anyhow!("scope C07 profile workspace: {error}"))?;
    let notes_dir = root.join("notes");
    ensure_empty_directory(&notes_dir)?;

    let actions_path = root.join("actions.json");
    let settings_path = root.join("settings.json");
    let snippets_path = root.join(crate::plugins::snippets::SNIPPETS_FILE);
    let history_path = root.join("history.json");
    let index_root = root.join("indexed-root");
    for path in [
        &actions_path,
        &settings_path,
        &snippets_path,
        &history_path,
        &index_root,
    ] {
        anyhow::ensure!(
            !path.exists(),
            "C07 profile output already exists; refusing to mix fixtures"
        );
    }

    let counts = scale.counts();
    let actions = track_c_workloads::action_fixture(counts.actions);
    let notes = track_c_workloads::note_fixture(counts.notes);
    let history = workloads::history_fixture(HISTORY_SEED, counts.history);
    let snippets = snippet_fixture(counts.snippets);

    save_actions(&path_text(&actions_path), &actions.values)?;
    for note in &notes.values {
        let path = notes_dir.join(format!("{}.md", note.slug));
        fs::write(path, &note.content)?;
    }
    save_snippets(&path_text(&snippets_path), &snippets.values)?;
    let history_entries = history.entries.iter().cloned().collect::<Vec<_>>();
    fs::write(&history_path, serde_json::to_vec_pretty(&history_entries)?)?;

    let index_summary = track_c_workloads::index_tree(&index_root, counts.indexed_files);
    let canonical_index_root = fs::canonicalize(&index_root)?;
    let mut settings = Settings::default();
    settings.hotkey = None;
    settings.quit_hotkey = None;
    settings.help_hotkey = None;
    settings.paste_sequence.next_hotkey = None;
    settings.paste_sequence.cancel_hotkey = None;
    settings.index_paths = Some(vec![path_text(&canonical_index_root)]);
    settings.max_indexed_items = Some(counts.indexed_files);
    crate::plugins::clipboard_modify::migrate_enablement(&mut settings);
    settings.save(&path_text(&settings_path))?;

    let (notes_markdown, note_files) = tree_bytes(&notes_dir)?;
    let (indexed_file_contents, indexed_files) = tree_bytes(&index_root)?;
    let bytes = ProfileBytes {
        actions_json: file_len(&actions_path)?,
        notes_markdown,
        snippets_json: file_len(&snippets_path)?,
        history_json: file_len(&history_path)?,
        settings_json: file_len(&settings_path)?,
        indexed_file_contents,
        total_file_contents: 0,
    };
    let bytes = ProfileBytes {
        total_file_contents: bytes
            .actions_json
            .saturating_add(bytes.notes_markdown)
            .saturating_add(bytes.snippets_json)
            .saturating_add(bytes.history_json)
            .saturating_add(bytes.settings_json)
            .saturating_add(bytes.indexed_file_contents),
        ..bytes
    };

    let receipt = ProfileReceipt {
        schema: "c07-startup-profile-v1",
        profile: scale.name(),
        counts,
        files: ProfileFileCounts {
            actions_json: 1,
            note_markdown: note_files,
            snippets_json: 1,
            history_json: 1,
            settings_json: 1,
            indexed_files,
            total_files: 4_u64
                .saturating_add(note_files)
                .saturating_add(indexed_files),
        },
        bytes,
        signatures_fnv64: ProfileSignatures {
            actions: actions.summary.signature,
            notes: notes.summary.signature,
            snippets: snippets.signature,
            history: history.summary.signature,
            indexed_tree: index_summary.signature,
            action_last_sentinel: safe_hash(
                actions
                    .values
                    .last()
                    .expect("profile action fixture is nonempty")
                    .action
                    .as_bytes(),
            ),
            note_last_sentinel: safe_hash(
                notes
                    .values
                    .last()
                    .expect("profile note fixture is nonempty")
                    .slug
                    .as_bytes(),
            ),
            snippet_last_sentinel: safe_hash(
                snippets
                    .values
                    .last()
                    .expect("profile snippet fixture is nonempty")
                    .alias
                    .as_bytes(),
            ),
            history_last_sentinel: safe_hash(
                history
                    .entries
                    .back()
                    .expect("profile history fixture is nonempty")
                    .query
                    .as_bytes(),
            ),
            index_last_sentinel: safe_hash(
                index_relative_path(counts.indexed_files - 1)
                    .to_string_lossy()
                    .as_bytes(),
            ),
        },
        configured_index_roots: settings.index_paths.as_ref().map_or(0, Vec::len),
        paste_next_hotkey_disabled: settings.paste_sequence.next_hotkey.is_none(),
        paste_cancel_hotkey_disabled: settings.paste_sequence.cancel_hotkey.is_none(),
    };

    Ok(StartupProfile {
        root,
        actions_path,
        settings_path,
        snippets_path,
        history_path,
        index_root: canonical_index_root,
        receipt,
    })
}

fn snippet_fixture(count: usize) -> SnippetFixture {
    let mut values = Vec::with_capacity(count);
    let mut signature = workloads::StableSignature::new(SNIPPET_SEED, "c07-snippets", count);
    for index in 0..count {
        let alias = format!("synthetic-snippet-{index:05}");
        let text =
            format!("Synthetic snippet {index:05}; profile sentinel SNIP-{index:05}; sample body.");
        signature.bytes(alias.as_bytes());
        signature.bytes(text.as_bytes());
        values.push(SnippetEntry {
            alias,
            text,
            hide_contents: false,
            prompt_for_fields: false,
            fields: Vec::new(),
            tags: vec![format!("synthetic-group-{:02}", index % 31)],
        });
    }
    SnippetFixture {
        values,
        signature: signature.finish(),
    }
}

struct SnippetFixture {
    values: Vec<SnippetEntry>,
    signature: u64,
}

fn ensure_empty_directory(path: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(path)?;
    anyhow::ensure!(
        fs::read_dir(path)?.next().transpose()?.is_none(),
        "C07 notes directory is not empty; refusing to mix fixtures"
    );
    Ok(())
}

fn file_len(path: &Path) -> io::Result<u64> {
    Ok(fs::metadata(path)?.len())
}

fn tree_bytes(root: &Path) -> io::Result<(u64, u64)> {
    let mut bytes = 0_u64;
    let mut files = 0_u64;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            let (child_bytes, child_files) = tree_bytes(&entry.path())?;
            bytes = bytes.saturating_add(child_bytes);
            files = files.saturating_add(child_files);
        } else if kind.is_file() {
            bytes = bytes.saturating_add(entry.metadata()?.len());
            files = files.saturating_add(1);
        }
    }
    Ok((bytes, files))
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub(crate) fn safe_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

pub(crate) fn index_relative_path(index: usize) -> PathBuf {
    let leaf = index / 64;
    PathBuf::from(format!(
        "shard-{:03}/group-{leaf:04}/file-{index:05}.txt",
        leaf / 16
    ))
}

#[cfg(test)]
mod tests {
    use super::{ProfileScale, materialize};
    use crate::common::persistence::LoadState;
    use crate::performance::track_c_workloads::TrackCWorkspace;

    #[test]
    fn c07_profile_scales_match_the_documented_counts() {
        assert_eq!(
            ProfileScale::Small.counts(),
            super::ProfileCounts {
                actions: 100,
                notes: 100,
                snippets: 100,
                history: 100,
                indexed_files: 32,
            }
        );
        assert_eq!(
            ProfileScale::Large.counts(),
            super::ProfileCounts {
                actions: 10_000,
                notes: 2_000,
                snippets: 5_000,
                history: 10_000,
                indexed_files: 2_000,
            }
        );
        assert_eq!(ProfileScale::parse("small"), Ok(ProfileScale::Small));
        assert_eq!(ProfileScale::parse("large"), Ok(ProfileScale::Large));
        assert!(ProfileScale::parse("full").is_err());
    }

    #[test]
    fn c07_small_profile_uses_current_persisted_schemas_and_safe_receipts() {
        let mut workspace = TrackCWorkspace::new();
        let profile = materialize(&mut workspace, ProfileScale::Small)
            .expect("small C07 profile materializes");
        let receipt = profile.receipt();

        assert_eq!(receipt.counts, ProfileScale::Small.counts());
        assert!(receipt.bytes.total_file_contents > 0);
        assert!(receipt.bytes.notes_markdown > 0);
        assert!(receipt.bytes.indexed_file_contents > 0);
        assert_eq!(receipt.files.note_markdown, receipt.counts.notes as u64);
        assert_eq!(
            receipt.files.indexed_files,
            receipt.counts.indexed_files as u64
        );
        assert_eq!(receipt.configured_index_roots, 1);
        assert!(receipt.paste_next_hotkey_disabled);
        assert!(receipt.paste_cancel_hotkey_disabled);

        let actions = crate::actions::load_startup_actions(profile.actions_path());
        assert!(actions.diagnostic.is_none());
        assert_eq!(actions.actions.len(), receipt.counts.actions);
        assert_eq!(
            super::safe_hash(
                actions
                    .actions
                    .last()
                    .expect("loaded action list is nonempty")
                    .action
                    .as_bytes()
            ),
            receipt.signatures_fnv64.action_last_sentinel
        );
        let notes = crate::plugins::note::load_notes().expect("persisted notes load");
        assert_eq!(notes.len(), receipt.counts.notes);
        assert!(notes.iter().any(|note| {
            super::safe_hash(note.slug.as_bytes()) == receipt.signatures_fnv64.note_last_sentinel
        }));
        let snippets = crate::plugins::snippets::load_snippets_typed(profile.snippets_path())
            .expect("persisted snippets load");
        let LoadState::Loaded(snippets) = snippets else {
            panic!("persisted snippets are present");
        };
        assert_eq!(snippets.len(), receipt.counts.snippets);
        assert!(snippets.iter().any(|snippet| {
            super::safe_hash(snippet.alias.as_bytes())
                == receipt.signatures_fnv64.snippet_last_sentinel
        }));
        let history = crate::history::load_startup_profile_history(profile.history_path())
            .expect("persisted history loads");
        assert_eq!(history.len(), receipt.counts.history);
        assert_eq!(
            super::safe_hash(
                history
                    .back()
                    .expect("loaded history is nonempty")
                    .query
                    .as_bytes()
            ),
            receipt.signatures_fnv64.history_last_sentinel
        );
        let last_index_relative = super::index_relative_path(receipt.counts.indexed_files - 1);
        assert!(profile.index_root().join(&last_index_relative).is_file());
        assert_eq!(
            super::safe_hash(last_index_relative.to_string_lossy().as_bytes()),
            receipt.signatures_fnv64.index_last_sentinel
        );
        let settings = crate::startup::load_startup_settings(profile.settings_path());
        assert!(settings.diagnostic.is_none());
        assert_eq!(
            settings.settings.index_paths.as_ref().map_or(0, Vec::len),
            1
        );
        assert!(settings.settings.paste_sequence.next_hotkey.is_none());
        assert!(settings.settings.paste_sequence.cancel_hotkey.is_none());

        let receipt_text = receipt.json_line().to_string();
        assert!(receipt_text.contains("c07-startup-profile-v1"));
        assert!(!receipt_text.contains("Synthetic"));
        assert!(!receipt_text.contains(&profile.root().to_string_lossy().to_string()));
        assert!(receipt.signatures_fnv64.action_last_sentinel != 0);
        assert!(receipt.signatures_fnv64.note_last_sentinel != 0);
        assert!(receipt.signatures_fnv64.snippet_last_sentinel != 0);
        assert!(receipt.signatures_fnv64.history_last_sentinel != 0);
        assert!(receipt.signatures_fnv64.index_last_sentinel != 0);

        workspace
            .finish()
            .expect("owned C07 fixture restores process state and removes root");
    }
}
