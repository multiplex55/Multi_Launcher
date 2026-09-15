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
    Empty { reason: String },
    Loading { reason: String },
    Unavailable { reason: String },
}

impl FrozenAvailability {
    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Available => None,
            Self::Empty { reason } | Self::Loading { reason } | Self::Unavailable { reason } => {
                Some(reason)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrozenEntryKind {
    Action,
    Manage,
    Empty,
    Loading,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FrozenRadialEntry {
    pub id: FrozenEntryId,
    pub label: String,
    pub kind: FrozenEntryKind,
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
    Contextual {
        selector: super::model::TargetSelector,
        action_id: ActionId,
        identity: crate::window_catalog::WindowTargetIdentity,
    },
    Runtime {
        target: ActionTarget,
        selected_action: Action,
        action_id: ActionId,
        identity: Option<RuntimeTargetIdentity>,
    },
    /// A visible, non-dispatchable source status. This can only occur in an
    /// invocation-frozen frame and has no persisted representation.
    Informational,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeTargetIdentity {
    Window {
        target: crate::window_catalog::WindowTargetIdentity,
        catalog_generation: u64,
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
    pub runtime_identity: Option<RuntimeTargetIdentity>,
    pub unavailable_reason: Option<String>,
    pub history_query: Option<String>,
    pub kind: FrozenEntryKind,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum DynamicSourceState {
    #[default]
    Ready,
    Loading {
        label: String,
    },
    Unavailable {
        reason: String,
    },
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
    pub applications: Vec<DynamicCandidate>,
    pub dashboard: Vec<DynamicCandidate>,
    pub applications_state: DynamicSourceState,
    pub dashboard_state: DynamicSourceState,
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
        let ready = DynamicSourceState::Ready;
        let (name, query, max_items, candidates, state): (
            &str,
            Option<String>,
            usize,
            &[DynamicCandidate],
            &DynamicSourceState,
        ) = match source {
            DynamicSource::LauncherResults { max_items } => {
                let query = invocation_query.unwrap_or_default().to_owned();
                (
                    "launcher_results_v1",
                    Some(query.clone()),
                    *max_items,
                    self.launcher_results.as_slice(),
                    &ready,
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
                &ready,
            ),
            DynamicSource::Favorites => ("favorites", None, usize::MAX, &self.favorites, &ready),
            DynamicSource::RecentItems => ("recent", None, usize::MAX, &self.recent, &ready),
            DynamicSource::Clipboard => ("clipboard", None, usize::MAX, &self.clipboard, &ready),
            DynamicSource::Snippets => ("snippets", None, usize::MAX, &self.snippets, &ready),
            DynamicSource::Notes => ("notes", None, usize::MAX, &self.notes, &ready),
            DynamicSource::Windows => ("windows", None, usize::MAX, &self.windows, &ready),
            DynamicSource::Macros => ("macros", None, usize::MAX, &self.macros, &ready),
            DynamicSource::Applications => (
                "applications",
                None,
                usize::MAX,
                &self.applications,
                &self.applications_state,
            ),
            DynamicSource::Dashboard => (
                "dashboard",
                None,
                usize::MAX,
                &self.dashboard,
                &self.dashboard_state,
            ),
        };
        let empty_label = format!("No {} available", source_display_name(name));
        let mut entries = match state {
            DynamicSourceState::Ready if candidates.is_empty() => vec![status_entry(
                name,
                FrozenEntryKind::Empty,
                &empty_label,
                FrozenAvailability::Empty {
                    reason: empty_label.clone(),
                },
            )],
            DynamicSourceState::Ready => Vec::new(),
            DynamicSourceState::Loading { label } => vec![status_entry(
                name,
                FrozenEntryKind::Loading,
                label,
                FrozenAvailability::Loading {
                    reason: label.clone(),
                },
            )],
            DynamicSourceState::Unavailable { reason } => vec![status_entry(
                name,
                FrozenEntryKind::Unavailable,
                reason,
                FrozenAvailability::Unavailable {
                    reason: reason.clone(),
                },
            )],
        };
        entries.extend(candidates.iter().take(max_items).map(|candidate| {
            FrozenRadialEntry {
                id: FrozenEntryId(candidate.stable_id.clone()),
                label: candidate.label.clone(),
                kind: candidate.kind,
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
                                identity: candidate.runtime_identity.clone(),
                            },
                        )
                    }),
                availability: candidate_availability(candidate),
                history_query: candidate
                    .history_query
                    .clone()
                    .or_else(|| query.clone())
                    .unwrap_or_default(),
                requirement: InteractionRequirement::None,
            }
        }));
        FrozenDynamicFrame {
            fingerprint: SourceFingerprint {
                generation: self.generation,
                source: name.into(),
                query: query.clone(),
            },
            entries,
        }
    }
}

fn candidate_availability(candidate: &DynamicCandidate) -> FrozenAvailability {
    if let Some(reason) = candidate.unavailable_reason.as_ref() {
        return FrozenAvailability::Unavailable {
            reason: reason.clone(),
        };
    }
    if matches!(
        candidate.runtime_action.as_ref(),
        Some((ActionTarget::Window { .. }, _, _))
    ) && candidate.runtime_identity.is_none()
    {
        return FrozenAvailability::Unavailable {
            reason: "Window identity is unavailable; refresh the menu before using this item"
                .into(),
        };
    }
    FrozenAvailability::Available
}

fn source_display_name(source: &str) -> &str {
    match source {
        "launcher_results_v1" => "launcher results",
        "launcher_query" => "query results",
        "recent" => "recent items",
        _ => source,
    }
}

fn status_entry(
    source: &str,
    kind: FrozenEntryKind,
    label: &str,
    availability: FrozenAvailability,
) -> FrozenRadialEntry {
    FrozenRadialEntry {
        id: FrozenEntryId(format!("status:{source}:{kind:?}")),
        label: label.into(),
        kind,
        binding: None,
        availability,
        history_query: String::new(),
        requirement: InteractionRequirement::None,
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
            runtime_identity: None,
            unavailable_reason: None,
            history_query: None,
            kind: FrozenEntryKind::Action,
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
    fn runtime_window_without_identity_always_freezes_unavailable() {
        let mut window = candidate("window:44");
        window.action = None;
        window.runtime_action = Some((
            ActionTarget::Window { hwnd: 44 },
            Action {
                label: "Window".into(),
                desc: "Test".into(),
                action: "window:switch:44".into(),
                args: None,
            },
            action_ids::WINDOW_ACTIVATE,
        ));
        window.runtime_identity = None;
        window.unavailable_reason = None;
        let frame = DynamicSnapshots {
            windows: vec![window],
            ..Default::default()
        }
        .freeze(&DynamicSource::Windows, None);
        assert!(matches!(
            frame.entries[0].availability,
            FrozenAvailability::Unavailable { .. }
        ));
        assert!(matches!(
            frame.entries[0].binding,
            Some(FrozenBinding::Runtime { identity: None, .. })
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
            applications: one.clone(),
            dashboard: one.clone(),
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
            DynamicSource::Applications,
            DynamicSource::Dashboard,
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

    #[test]
    fn source_states_and_verified_manage_actions_are_typed_and_frozen() {
        let mut manage = candidate("dashboard:settings");
        manage.kind = FrozenEntryKind::Manage;
        let mut snapshots = DynamicSnapshots {
            generation: 7,
            dashboard: vec![manage],
            dashboard_state: DynamicSourceState::Unavailable {
                reason: "Dashboard is disabled".into(),
            },
            applications_state: DynamicSourceState::Loading {
                label: "Loading applications".into(),
            },
            ..Default::default()
        };
        let dashboard = snapshots.freeze(&DynamicSource::Dashboard, None);
        assert_eq!(dashboard.entries[0].kind, FrozenEntryKind::Unavailable);
        assert!(dashboard.entries[0].binding.is_none());
        assert_eq!(dashboard.entries[1].kind, FrozenEntryKind::Manage);
        assert!(matches!(
            dashboard.entries[1].binding,
            Some(FrozenBinding::Stable(_))
        ));

        let loading = snapshots.freeze(&DynamicSource::Applications, None);
        assert_eq!(loading.entries[0].kind, FrozenEntryKind::Loading);
        snapshots.applications_state = DynamicSourceState::Ready;
        snapshots.applications.push(candidate("calculator"));
        assert_eq!(
            loading.entries.len(),
            1,
            "a frozen invocation never refetches"
        );

        let empty = DynamicSnapshots::default().freeze(&DynamicSource::Notes, None);
        assert_eq!(empty.entries[0].kind, FrozenEntryKind::Empty);
        assert!(empty.entries[0].binding.is_none());
    }
}
