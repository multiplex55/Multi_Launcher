use crate::common::persistence::save_json_atomic_replaceable;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const NOTE_UI_STATE_RELATIVE_PATH: &str = "note_ui_state.json";
static NOTE_UI_STATE_TRANSACTION: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteUiState {
    #[serde(default)]
    pub notes: BTreeMap<String, NoteCollapsedState>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteCollapsedState {
    #[serde(default)]
    pub collapsed_sections: BTreeSet<String>,
}

impl NoteUiState {
    pub fn collapsed_sections_for(&self, note_slug: &str) -> HashSet<String> {
        self.notes
            .get(note_slug)
            .map(|note| note.collapsed_sections.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn set_collapsed_sections<I>(&mut self, note_slug: impl Into<String>, collapsed_sections: I)
    where
        I: IntoIterator<Item = String>,
    {
        let collapsed_sections = collapsed_sections.into_iter().collect::<BTreeSet<_>>();
        let note_slug = note_slug.into();
        if collapsed_sections.is_empty() {
            self.notes.remove(&note_slug);
        } else {
            self.notes
                .insert(note_slug, NoteCollapsedState { collapsed_sections });
        }
    }

    pub fn from_json(contents: &str) -> serde_json::Result<Self> {
        serde_json::from_str(contents)
    }

    pub fn to_json_pretty(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self).map(|json| format!("{json}\n"))
    }
}

pub fn path_for_settings(settings_path: &Path) -> PathBuf {
    settings_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(NOTE_UI_STATE_RELATIVE_PATH)
}

pub fn load(path: &Path) -> anyhow::Result<NoteUiState> {
    if !path.exists() {
        return Ok(NoteUiState::default());
    }
    let contents = std::fs::read_to_string(path)?;
    Ok(NoteUiState::from_json(&contents)?)
}

pub fn save(path: &Path, state: &NoteUiState) -> anyhow::Result<()> {
    save_json_atomic_replaceable(path, state).map_err(Into::into)
}

pub fn update(path: &Path, mutate: impl FnOnce(&mut NoteUiState)) -> anyhow::Result<NoteUiState> {
    let _transaction = NOTE_UI_STATE_TRANSACTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut state = load(path)?;
    mutate(&mut state);
    save(path, &state)?;
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::{NoteUiState, update};

    #[test]
    fn serializes_collapsed_sections_per_note_slug() {
        let mut state = NoteUiState::default();
        state.set_collapsed_sections(
            "daily-note",
            vec![
                "daily-note::later::Later::8".to_string(),
                "daily-note::top::Top::0".to_string(),
            ],
        );
        state.set_collapsed_sections(
            "project-note",
            vec!["project-note::ideas::Ideas::3".to_string()],
        );

        let json = state.to_json_pretty().expect("serialize note ui state");

        assert!(json.contains("daily-note"));
        assert!(json.contains("project-note"));
        assert!(json.contains("collapsed_sections"));
        assert_eq!(json, state.to_json_pretty().expect("stable serialization"));
    }

    #[test]
    fn deserializes_collapsed_sections_and_defaults_missing_notes() {
        let state = NoteUiState::from_json(
            r#"{
              "notes": {
                "daily-note": {
                  "collapsed_sections": [
                    "daily-note::later::Later::8",
                    "daily-note::top::Top::0"
                  ]
                }
              }
            }"#,
        )
        .expect("deserialize note ui state");

        let collapsed = state.collapsed_sections_for("daily-note");
        assert!(collapsed.contains("daily-note::later::Later::8"));
        assert!(collapsed.contains("daily-note::top::Top::0"));
        assert!(state.collapsed_sections_for("missing-note").is_empty());
    }

    #[test]
    fn empty_collapsed_sections_remove_note_entry() {
        let mut state = NoteUiState::default();
        state.set_collapsed_sections("daily-note", vec!["daily-note::top::Top::0".to_string()]);
        state.set_collapsed_sections("daily-note", Vec::<String>::new());

        assert!(state.notes.is_empty());
    }

    #[test]
    fn missing_initializes_but_malformed_update_is_rejected_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note-ui.json");
        update(&path, |state| {
            state.set_collapsed_sections("note", ["section".to_string()]);
        })
        .unwrap();

        let invalid = b"not note ui state";
        std::fs::write(&path, invalid).unwrap();
        assert!(update(&path, |state| state.notes.clear()).is_err());
        assert_eq!(std::fs::read(path).unwrap(), invalid);
    }
}
