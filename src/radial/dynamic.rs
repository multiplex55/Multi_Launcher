use super::handoff::InteractionRequirement;
use super::model::{ActionBinding, DynamicSource};
use crate::actions::Action;
use crate::universal_actions::{ActionId, ActionTarget, PersistedUniversalActionRef};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FrozenEntryId(pub String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrozenAvailability {
    Available,
    Unavailable { reason: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct FrozenRadialEntry {
    pub id: FrozenEntryId,
    pub label: String,
    pub binding: Option<FrozenBinding>,
    pub availability: FrozenAvailability,
    /// Query captured when this entry was materialized. History attribution
    /// must never consult the launcher's later mutable root query.
    pub history_query: String,
    pub requirement: InteractionRequirement,
}

/// Invocation-scoped bindings may retain ephemeral identities, but this type
/// is deliberately not serializable and never appears in RadialDocument.
#[derive(Clone, Debug, PartialEq)]
pub enum FrozenBinding {
    Stable(ActionBinding),
    Runtime {
        target: ActionTarget,
        selected_action: Action,
        action_id: ActionId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFingerprint {
    pub generation: u64,
    pub source: String,
    pub query: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FrozenDynamicFrame {
    pub fingerprint: SourceFingerprint,
    pub entries: Vec<FrozenRadialEntry>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DynamicCandidate {
    pub stable_id: String,
    pub label: String,
    pub action: Option<PersistedUniversalActionRef>,
    pub runtime_action: Option<(ActionTarget, Action, ActionId)>,
    pub unavailable_reason: Option<String>,
    pub history_query: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DynamicSnapshots {
    pub generation: u64,
    pub favorites: Vec<DynamicCandidate>,
    pub recent: Vec<DynamicCandidate>,
    pub clipboard: Vec<DynamicCandidate>,
    pub snippets: Vec<DynamicCandidate>,
    pub notes: Vec<DynamicCandidate>,
    pub windows: Vec<DynamicCandidate>,
    pub macros: Vec<DynamicCandidate>,
    /// Results are produced with a private read-only query context. Keys are
    /// exact configured query strings, not the launcher's root query field.
    pub launcher_queries: BTreeMap<String, Vec<DynamicCandidate>>,
    /// The root launcher's invocation-time query/results are a distinct snapshot,
    /// even when their query text equals an explicitly configured query.
    pub launcher_results: Vec<DynamicCandidate>,
}

impl DynamicSnapshots {
    pub fn freeze(
        &self,
        source: &DynamicSource,
        invocation_query: Option<&str>,
    ) -> FrozenDynamicFrame {
        let (name, query, max_items, candidates): (
            &str,
            Option<String>,
            usize,
            &[DynamicCandidate],
        ) = match source {
            DynamicSource::LauncherResults { max_items } => {
                let query = invocation_query.unwrap_or_default().to_owned();
                (
                    "launcher_results_v1",
                    Some(query.clone()),
                    *max_items,
                    self.launcher_results.as_slice(),
                )
            }
            DynamicSource::LauncherQuery { query, max_items } => (
                "launcher_query",
                Some(query.clone()),
                *max_items,
                self.launcher_queries
                    .get(query)
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
            ),
            DynamicSource::Favorites => ("favorites", None, usize::MAX, &self.favorites),
            DynamicSource::RecentItems => ("recent", None, usize::MAX, &self.recent),
            DynamicSource::Clipboard => ("clipboard", None, usize::MAX, &self.clipboard),
            DynamicSource::Snippets => ("snippets", None, usize::MAX, &self.snippets),
            DynamicSource::Notes => ("notes", None, usize::MAX, &self.notes),
            DynamicSource::Windows => ("windows", None, usize::MAX, &self.windows),
            DynamicSource::Macros => ("macros", None, usize::MAX, &self.macros),
        };
        FrozenDynamicFrame {
            fingerprint: SourceFingerprint {
                generation: self.generation,
                source: name.into(),
                query: query.clone(),
            },
            entries: candidates
                .iter()
                .take(max_items)
                .map(|candidate| FrozenRadialEntry {
                    id: FrozenEntryId(candidate.stable_id.clone()),
                    label: candidate.label.clone(),
                    binding: candidate
                        .action
                        .clone()
                        .map(|action| FrozenBinding::Stable(ActionBinding::Persisted { action }))
                        .or_else(|| {
                            candidate.runtime_action.clone().map(
                                |(target, selected_action, action_id)| FrozenBinding::Runtime {
                                    target,
                                    selected_action,
                                    action_id,
                                },
                            )
                        }),
                    availability: candidate.unavailable_reason.as_ref().map_or(
                        FrozenAvailability::Available,
                        |reason| FrozenAvailability::Unavailable {
                            reason: reason.clone(),
                        },
                    ),
                    history_query: candidate
                        .history_query
                        .clone()
                        .or_else(|| query.clone())
                        .unwrap_or_default(),
                    requirement: InteractionRequirement::None,
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;
    use crate::universal_actions::{PersistableActionTargetRef, action_ids};

    fn candidate(id: &str) -> DynamicCandidate {
        DynamicCandidate {
            stable_id: id.into(),
            label: id.into(),
            action: Some(PersistedUniversalActionRef {
                target: Some(PersistableActionTargetRef::LegacyAction {
                    action: Action {
                        label: id.into(),
                        desc: "test".into(),
                        action: format!("run:{id}"),
                        args: None,
                    },
                }),
                action_id: action_ids::RESULT_EXECUTE,
            }),
            runtime_action: None,
            unavailable_reason: None,
            history_query: None,
        }
    }

    #[test]
    fn explicit_query_is_frozen_without_reading_or_mutating_root_state() {
        let mut snapshots = DynamicSnapshots {
            generation: 9,
            ..Default::default()
        };
        snapshots
            .launcher_queries
            .insert("docs".into(), vec![candidate("a"), candidate("b")]);
        let source = DynamicSource::LauncherQuery {
            query: "docs".into(),
            max_items: 1,
        };
        let first = snapshots.freeze(&source, Some("unrelated-root-query"));
        snapshots
            .launcher_queries
            .get_mut("docs")
            .unwrap()
            .reverse();
        assert_eq!(first.entries[0].id.0, "a");
        assert_eq!(first.fingerprint.query.as_deref(), Some("docs"));
    }

    #[test]
    fn configured_query_and_captured_results_do_not_collide_on_equal_text() {
        let mut snapshots = DynamicSnapshots {
            generation: 2,
            launcher_results: vec![candidate("captured")],
            ..Default::default()
        };
        snapshots
            .launcher_queries
            .insert("same".into(), vec![candidate("configured")]);
        let configured = snapshots.freeze(
            &DynamicSource::LauncherQuery {
                query: "same".into(),
                max_items: 5,
            },
            Some("same"),
        );
        let captured = snapshots.freeze(
            &DynamicSource::LauncherResults { max_items: 5 },
            Some("same"),
        );
        assert_eq!(configured.entries[0].id.0, "configured");
        assert_eq!(captured.entries[0].id.0, "captured");
        assert_ne!(configured.fingerprint.source, captured.fingerprint.source);
    }

    #[test]
    fn removed_entries_retain_a_disabled_stable_slot() {
        let mut missing = candidate("note:gone");
        missing.action = None;
        missing.runtime_action = None;
        missing.unavailable_reason = Some("note was deleted".into());
        let snapshots = DynamicSnapshots {
            notes: vec![missing],
            ..Default::default()
        };
        let frame = snapshots.freeze(&DynamicSource::Notes, None);
        assert_eq!(frame.entries[0].id.0, "note:gone");
        assert!(matches!(
            frame.entries[0].availability,
            FrozenAvailability::Unavailable { .. }
        ));
    }

    #[test]
    fn every_allowlisted_source_freezes_typed_selectable_entries() {
        let one = vec![candidate("entry")];
        let mut snapshots = DynamicSnapshots {
            favorites: one.clone(),
            recent: one.clone(),
            clipboard: one.clone(),
            snippets: one.clone(),
            notes: one.clone(),
            windows: one.clone(),
            macros: one.clone(),
            ..Default::default()
        };
        snapshots.launcher_queries.insert("q".into(), one);
        let sources = [
            DynamicSource::Favorites,
            DynamicSource::RecentItems,
            DynamicSource::Clipboard,
            DynamicSource::Snippets,
            DynamicSource::Notes,
            DynamicSource::Windows,
            DynamicSource::Macros,
            DynamicSource::LauncherQuery {
                query: "q".into(),
                max_items: 12,
            },
        ];
        for source in sources {
            let frame = snapshots.freeze(&source, None);
            assert_eq!(frame.entries.len(), 1, "{source:?}");
            assert!(frame.entries[0].binding.is_some());
        }
    }
}
