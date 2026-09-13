use crate::actions::Action;
use serde::{Deserialize, Serialize};

use super::ActionId;

/// Runtime identity for an object whose semantic actions can be discovered.
///
/// Ephemeral identifiers are intentionally valid here. Call [`Self::persistent_ref`]
/// before using a target in persisted configuration.
#[derive(Clone, Debug, PartialEq)]
pub enum ActionTarget {
    Generic {
        action: Action,
    },
    CustomAction {
        index: usize,
        action: Action,
    },
    Folder {
        path: String,
    },
    Bookmark {
        url: String,
    },
    Timer {
        id: u64,
    },
    Stopwatch {
        id: u64,
    },
    Snippet {
        alias: String,
    },
    Tempfile {
        path: String,
    },
    Note {
        slug: String,
    },
    ClipboardEntry {
        index: usize,
    },
    Todo {
        index: usize,
    },
    Window {
        hwnd: isize,
    },
    MkMacro {
        id: u64,
    },
    BrowserTab {
        runtime_id: Vec<i32>,
        url: Option<String>,
    },
}

impl ActionTarget {
    /// Return the stable identity suitable for future saved action bindings.
    ///
    /// Runtime handles, list indexes, and live timer instances are deliberately
    /// not promoted to persistent identities.
    pub fn persistent_ref(&self) -> Option<PersistableActionTargetRef> {
        match self {
            Self::Generic { action } => Some(PersistableActionTargetRef::LegacyAction {
                action: action.clone(),
            }),
            Self::CustomAction { action, .. } => Some(PersistableActionTargetRef::CustomAction {
                action: action.clone(),
            }),
            Self::Folder { path } => {
                Some(PersistableActionTargetRef::Folder { path: path.clone() })
            }
            Self::Bookmark { url } => {
                Some(PersistableActionTargetRef::Bookmark { url: url.clone() })
            }
            Self::Snippet { alias } => Some(PersistableActionTargetRef::Snippet {
                alias: alias.clone(),
            }),
            Self::Tempfile { path } => {
                Some(PersistableActionTargetRef::Tempfile { path: path.clone() })
            }
            Self::Note { slug } => Some(PersistableActionTargetRef::Note { slug: slug.clone() }),
            Self::MkMacro { id } => Some(PersistableActionTargetRef::MkMacro { id: *id }),
            Self::Timer { .. }
            | Self::Stopwatch { .. }
            | Self::ClipboardEntry { .. }
            | Self::Todo { .. }
            | Self::Window { .. }
            | Self::BrowserTab { .. } => None,
        }
    }
}

/// Stable subset of [`ActionTarget`] identities that may be stored in future
/// Universal Action configuration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PersistableActionTargetRef {
    LegacyAction { action: Action },
    CustomAction { action: Action },
    Folder { path: String },
    Bookmark { url: String },
    Snippet { alias: String },
    Tempfile { path: String },
    Note { slug: String },
    MkMacro { id: u64 },
}

/// Stable reference to a semantic action, optionally scoped to a persistable
/// target. A missing target supports future global/root-surface actions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PersistedUniversalActionRef {
    pub target: Option<PersistableActionTargetRef>,
    pub action_id: ActionId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::universal_actions::action_ids;

    fn action() -> Action {
        Action {
            label: "Daily note".into(),
            desc: "Note".into(),
            action: "note:open:daily-note".into(),
            args: None,
        }
    }

    #[test]
    fn stable_targets_produce_persistable_refs() {
        assert_eq!(
            ActionTarget::Note {
                slug: "daily-note".into()
            }
            .persistent_ref(),
            Some(PersistableActionTargetRef::Note {
                slug: "daily-note".into()
            })
        );

        let stable = [
            ActionTarget::Generic { action: action() },
            ActionTarget::Folder {
                path: "C:/work".into(),
            },
            ActionTarget::Bookmark {
                url: "https://example.test".into(),
            },
            ActionTarget::Snippet {
                alias: "sig".into(),
            },
            ActionTarget::Tempfile {
                path: "C:/tmp/a".into(),
            },
            ActionTarget::Note {
                slug: "daily-note".into(),
            },
            ActionTarget::MkMacro { id: 9 },
        ];
        assert!(
            stable
                .iter()
                .all(|target| target.persistent_ref().is_some())
        );
        assert_eq!(
            ActionTarget::CustomAction {
                index: 17,
                action: action()
            }
            .persistent_ref(),
            Some(PersistableActionTargetRef::CustomAction { action: action() })
        );
    }

    #[test]
    fn ephemeral_targets_do_not_claim_persistent_identity() {
        let ephemeral = [
            ActionTarget::Timer { id: 1 },
            ActionTarget::Stopwatch { id: 2 },
            ActionTarget::ClipboardEntry { index: 3 },
            ActionTarget::Todo { index: 4 },
            ActionTarget::Window { hwnd: 5 },
            ActionTarget::BrowserTab {
                runtime_id: vec![6, 7],
                url: Some("https://example.test".into()),
            },
        ];

        assert!(
            ephemeral
                .iter()
                .all(|target| target.persistent_ref().is_none())
        );
    }

    #[test]
    fn persisted_action_reference_round_trips_through_json() {
        let reference = PersistedUniversalActionRef {
            target: Some(PersistableActionTargetRef::Note {
                slug: "daily-note".into(),
            }),
            action_id: action_ids::NOTE_OPEN,
        };

        let json = serde_json::to_string(&reference).unwrap();
        assert_eq!(
            serde_json::from_str::<PersistedUniversalActionRef>(&json).unwrap(),
            reference
        );
    }

    #[test]
    fn legacy_action_json_remains_unchanged_by_universal_target_wrapping() {
        let legacy_json = r#"{"label":"Tool","desc":"Custom","action":"tool.exe","args":"--flag"}"#;
        let action: Action = serde_json::from_str(legacy_json).unwrap();
        let target = ActionTarget::Generic {
            action: action.clone(),
        };
        assert!(target.persistent_ref().is_some());
        assert_eq!(
            serde_json::to_value(&action).unwrap(),
            serde_json::json!({
                "label": "Tool",
                "desc": "Custom",
                "action": "tool.exe",
                "args": "--flag"
            })
        );
    }
}
