//! Immutable, payload-free view of configured hotkey routes.
//!
//! A configured candidate is kept separate from an applied runtime receipt.
//! In particular, preparing a route can only produce `Pending`; it cannot
//! produce `Active` without an exact receipt for the same entry, chord,
//! source revision, and runtime generation.

use crate::hotkey::{parse_hotkey, validation::hotkeys_can_cofire};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Source {
    MainSettings,
    PasteSequences,
    Radial,
    Macros,
    ScreenDraw,
    MouseGestures,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntryId(String);

impl EntryId {
    pub fn new(stable_id: impl Into<String>) -> Self {
        Self(stable_id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConflictTarget {
    pub source: Source,
    pub id: EntryId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    Launcher,
    PasteSequences,
    Radial,
    MkMacro,
    ScreenDraw,
    MouseGestures,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteKind {
    LegacyPolling,
    SharedHook,
    MacroPolling,
    Radial,
    Local,
    Gesture,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    ProcessWide,
    Global,
    MenuLocal(String),
    ActiveWindow,
    LocalSession,
    Gesture,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditDestination {
    MainSettings(MainSetting),
    PasteSequence {
        sequence_id: Option<u64>,
        setting: PasteSetting,
    },
    RadialTrigger {
        trigger_id: String,
        menu_id: String,
    },
    RadialShortcut {
        menu_id: String,
        cell_id: String,
        shortcut_id: String,
    },
    MacroPlayback {
        macro_id: u64,
    },
    MacroRecorder(MacroRecorderSetting),
    ScreenDraw(ScreenDrawSetting),
    GestureBinding {
        publication: u64,
        gesture_index: u32,
        binding_index: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainSetting {
    Launcher,
    Quit,
    Help,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteSetting {
    Next,
    Cancel,
    Start,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroRecorderSetting {
    RecordToggle,
    PauseResume,
    Marker,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScreenDrawSetting {
    Launch,
    Emergency,
    Tool(String),
    IncreaseThickness,
    DecreaseThickness,
    QuickColor(u8),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CandidateState {
    Configured,
    Pending { generation: u64 },
    Disabled,
    Invalid,
    Conflict,
    Unsupported,
    OmittedByPrecedence,
    Unavailable,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    InvalidChord,
    InvalidScope,
    RouteNotAcknowledged,
    RuntimeRestorationUncertain,
    OwnerDisabled,
    OwnerUnavailable,
    UnsupportedByRoute,
    OmittedByPrecedence,
    ConflictsWithConfiguredRoute,
    LocalRouteHasNoGlobalReceipt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub id: EntryId,
    pub owner: Owner,
    pub owner_label: String,
    pub action_label: String,
    pub chord: Option<String>,
    pub route: RouteKind,
    pub scope: Scope,
    pub source_revision: u64,
    pub expected_runtime_generation: Option<u64>,
    pub state: CandidateState,
    pub reason: Option<Reason>,
    pub edit: Option<EditDestination>,
}

/// Proof that an exact configured route has already reached its runtime owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppliedRouteReceipt {
    pub id: EntryId,
    pub chord: String,
    pub source_revision: u64,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSnapshot {
    pub source: Source,
    pub revision: u64,
    /// Runtime transaction currently changing this source's routes, if any.
    /// This lets the merged snapshot retain applied receipts even when a
    /// candidate is being removed and therefore has no Pending row of its own.
    pub pending_generation: Option<u64>,
    pub candidates: Arc<[Candidate]>,
    pub applied_routes: Arc<[AppliedRouteReceipt]>,
}

impl SourceSnapshot {
    pub fn new(
        source: Source,
        revision: u64,
        candidates: Vec<Candidate>,
        applied_routes: Vec<AppliedRouteReceipt>,
    ) -> Self {
        Self {
            source,
            revision,
            pending_generation: None,
            candidates: candidates.into(),
            applied_routes: applied_routes.into(),
        }
    }

    pub fn with_pending_generation(mut self, generation: u64) -> Self {
        self.pending_generation = Some(generation);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Active { generation: u64 },
    Pending { generation: u64 },
    Disabled,
    Invalid,
    Conflict,
    Unsupported,
    OmittedByPrecedence,
    Unavailable,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub source: Source,
    pub id: EntryId,
    pub owner: Owner,
    pub owner_label: String,
    pub action_label: String,
    pub chord: Option<String>,
    pub route: RouteKind,
    pub scope: Scope,
    pub source_revision: u64,
    pub status: Status,
    pub reason: Option<Reason>,
    pub conflicts: Arc<[ConflictTarget]>,
    /// Retained separately from a candidate so a pending replacement never
    /// erases the last acknowledged route from the read model.
    pub last_applied: Option<AppliedRouteReceipt>,
    pub edit: Option<EditDestination>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    publication: u64,
    sources: Arc<[SourceSnapshot]>,
    entries: Arc<[Entry]>,
}

impl Snapshot {
    pub fn empty(publication: u64) -> Self {
        Self::from_sources(publication, Vec::new())
    }

    pub fn from_sources(publication: u64, sources: Vec<SourceSnapshot>) -> Self {
        let entries = build_entries(&sources);
        Self {
            publication,
            sources: sources.into(),
            entries: entries.into(),
        }
    }

    pub fn publication(&self) -> u64 {
        self.publication
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn source_revision(&self, source: Source) -> Option<u64> {
        self.sources
            .iter()
            .find(|current| current.source == source)
            .map(|current| current.revision)
    }

    /// Replaces one source only when its revision is strictly newer. Equal
    /// revisions are duplicates or conflicting stale deliveries and cannot
    /// replace the retained source.
    pub fn replacing_source(
        &self,
        publication: u64,
        mut replacement: SourceSnapshot,
    ) -> Option<Self> {
        if self
            .source_revision(replacement.source)
            .is_some_and(|revision| replacement.revision <= revision)
        {
            return None;
        }
        // A pending candidate has not replaced the last acknowledged route
        // yet. Carry that receipt forward by entry identity so the read model
        // can show both facts. A terminal unavailable/unknown publication
        // intentionally does not inherit receipts after uncertain restoration.
        let pending_ids = replacement
            .candidates
            .iter()
            .filter(|candidate| matches!(candidate.state, CandidateState::Pending { .. }))
            .map(|candidate| candidate.id.clone())
            .collect::<std::collections::BTreeSet<_>>();
        if replacement.pending_generation.is_some() || !pending_ids.is_empty() {
            let mut receipts = replacement.applied_routes.to_vec();
            let old_sources = self
                .sources
                .iter()
                .filter(|source| source.source == replacement.source)
                .collect::<Vec<_>>();
            for old_receipt in old_sources
                .iter()
                .flat_map(|source| source.applied_routes.iter())
            {
                if !receipts.iter().any(|receipt| {
                    receipt.id == old_receipt.id && receipt.generation >= old_receipt.generation
                }) {
                    receipts.push(old_receipt.clone());
                }
            }
            replacement.applied_routes = receipts.into();

            // A disabled candidate may represent a pending removal. Preserve
            // that distinction while its old exact receipt is still live.
            if let Some(generation) = replacement.pending_generation {
                let old_applied_ids = old_sources
                    .iter()
                    .flat_map(|source| source.applied_routes.iter())
                    .map(|receipt| receipt.id.clone())
                    .collect::<std::collections::BTreeSet<_>>();
                let mut candidates = replacement.candidates.to_vec();
                for candidate in &mut candidates {
                    if old_applied_ids.contains(&candidate.id)
                        && matches!(candidate.state, CandidateState::Disabled)
                    {
                        candidate.state = CandidateState::Pending { generation };
                        candidate.expected_runtime_generation = Some(generation);
                    }
                }
                replacement.candidates = candidates.into();
            }

            // A route whose stable identity changed is still applied until the
            // pending transaction completes. Keep its old candidate alongside
            // the new pending candidate so the old receipt remains visible.
            let replacement_ids = replacement
                .candidates
                .iter()
                .map(|candidate| candidate.id.clone())
                .collect::<std::collections::BTreeSet<_>>();
            let old_applied_candidates = old_sources
                .iter()
                .flat_map(|source| source.candidates.iter())
                .filter(|candidate| {
                    !replacement_ids.contains(&candidate.id)
                        && !pending_ids.contains(&candidate.id)
                        && old_sources.iter().any(|source| {
                            source.applied_routes.iter().any(|receipt| {
                                receipt.id == candidate.id
                                    && candidate.chord.as_deref() == Some(receipt.chord.as_str())
                                    && candidate.source_revision == receipt.source_revision
                                    && candidate.expected_runtime_generation
                                        == Some(receipt.generation)
                            })
                        })
                })
                .cloned()
                .collect::<Vec<_>>();
            if !old_applied_candidates.is_empty() {
                let mut candidates = replacement.candidates.to_vec();
                candidates.extend(old_applied_candidates);
                replacement.candidates = candidates.into();
            }
        }
        let mut sources = self
            .sources
            .iter()
            .filter(|source| source.source != replacement.source)
            .cloned()
            .collect::<Vec<_>>();
        sources.push(replacement);
        Some(Self::from_sources(publication, sources))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CollisionDomain {
    ProcessWide,
    RadialGlobal,
    RadialMenuLocal(String),
    MacroGlobal,
}

fn build_entries(sources: &[SourceSnapshot]) -> Vec<Entry> {
    let mut candidates = sources
        .iter()
        .flat_map(|source| {
            source
                .candidates
                .iter()
                .cloned()
                .map(move |candidate| (source.source, candidate))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|(left_source, left), (right_source, right)| {
        (*left_source, &left.id).cmp(&(*right_source, &right.id))
    });

    let conflicts = find_conflicts(&candidates);
    let mut entries = candidates
        .into_iter()
        .map(|(source, candidate)| {
            let applied = sources
                .iter()
                .filter(|snapshot| snapshot.source == source)
                .flat_map(|snapshot| snapshot.applied_routes.iter())
                .filter(|receipt| receipt.id == candidate.id)
                .max_by_key(|receipt| receipt.generation)
                .cloned();
            let conflicts = conflicts
                .get(&(source, candidate.id.clone()))
                .cloned()
                .unwrap_or_default();
            let status = status_for(&candidate, applied.as_ref(), !conflicts.is_empty());
            Entry {
                source,
                id: candidate.id,
                owner: candidate.owner,
                owner_label: candidate.owner_label,
                action_label: candidate.action_label,
                chord: candidate.chord,
                route: candidate.route,
                scope: candidate.scope,
                source_revision: candidate.source_revision,
                status,
                reason: candidate.reason.or_else(|| {
                    (!conflicts.is_empty()).then_some(Reason::ConflictsWithConfiguredRoute)
                }),
                conflicts: conflicts.into(),
                last_applied: applied,
                edit: candidate.edit,
            }
        })
        .collect::<Vec<_>>();
    entries.dedup_by(|left, right| left.source == right.source && left.id == right.id);
    entries
}

fn status_for(
    candidate: &Candidate,
    applied: Option<&AppliedRouteReceipt>,
    conflicts: bool,
) -> Status {
    match candidate.state {
        CandidateState::Disabled => Status::Disabled,
        CandidateState::Invalid => Status::Invalid,
        CandidateState::Conflict => Status::Conflict,
        CandidateState::Unsupported => Status::Unsupported,
        CandidateState::OmittedByPrecedence => Status::OmittedByPrecedence,
        CandidateState::Unavailable => Status::Unavailable,
        CandidateState::Unknown => Status::Unknown,
        CandidateState::Pending { generation } => Status::Pending { generation },
        CandidateState::Configured => {
            if candidate
                .chord
                .as_deref()
                .is_none_or(|chord| parse_hotkey(chord).is_none())
            {
                return Status::Invalid;
            }
            if conflicts {
                return Status::Conflict;
            }
            let exact_applied = applied.is_some_and(|receipt| {
                Some(receipt.chord.as_str()) == candidate.chord.as_deref()
                    && receipt.source_revision == candidate.source_revision
                    && Some(receipt.generation) == candidate.expected_runtime_generation
            });
            if exact_applied {
                Status::Active {
                    generation: applied.expect("checked above").generation,
                }
            } else {
                Status::Unknown
            }
        }
    }
}

fn find_conflicts(
    candidates: &[(Source, Candidate)],
) -> std::collections::BTreeMap<(Source, EntryId), Vec<ConflictTarget>> {
    let mut conflicts = std::collections::BTreeMap::new();
    for left_index in 0..candidates.len() {
        let (left_source, left) = &candidates[left_index];
        let Some(left_chord) = left.chord.as_deref().and_then(parse_hotkey) else {
            continue;
        };
        let Some(left_domain) = collision_domain(left) else {
            continue;
        };
        if !matches!(
            left.state,
            CandidateState::Configured | CandidateState::Pending { .. }
        ) {
            continue;
        }
        for (right_source, right) in candidates.iter().skip(left_index + 1) {
            if left_source == right_source && left.id == right.id {
                continue;
            }
            let Some(right_chord) = right.chord.as_deref().and_then(parse_hotkey) else {
                continue;
            };
            let Some(right_domain) = collision_domain(right) else {
                continue;
            };
            if !matches!(
                right.state,
                CandidateState::Configured | CandidateState::Pending { .. }
            ) || !domains_can_cofire(&left_domain, &right_domain)
                || !hotkeys_can_cofire(left_chord, right_chord)
            {
                continue;
            }
            conflicts
                .entry((*left_source, left.id.clone()))
                .or_insert_with(Vec::new)
                .push(ConflictTarget {
                    source: *right_source,
                    id: right.id.clone(),
                });
            conflicts
                .entry((*right_source, right.id.clone()))
                .or_insert_with(Vec::new)
                .push(ConflictTarget {
                    source: *left_source,
                    id: left.id.clone(),
                });
        }
    }
    conflicts
}

fn collision_domain(candidate: &Candidate) -> Option<CollisionDomain> {
    match (&candidate.scope, candidate.owner) {
        (Scope::ProcessWide, _) => Some(CollisionDomain::ProcessWide),
        (Scope::Global, Owner::Radial) => Some(CollisionDomain::RadialGlobal),
        (Scope::Global, Owner::MkMacro) => Some(CollisionDomain::MacroGlobal),
        (Scope::MenuLocal(menu), Owner::Radial) => {
            Some(CollisionDomain::RadialMenuLocal(menu.clone()))
        }
        _ => None,
    }
}

fn domains_can_cofire(left: &CollisionDomain, right: &CollisionDomain) -> bool {
    match (left, right) {
        (CollisionDomain::ProcessWide, CollisionDomain::ProcessWide)
        | (CollisionDomain::ProcessWide, CollisionDomain::RadialGlobal)
        | (CollisionDomain::RadialGlobal, CollisionDomain::ProcessWide)
        | (CollisionDomain::RadialGlobal, CollisionDomain::RadialGlobal)
        | (CollisionDomain::ProcessWide, CollisionDomain::MacroGlobal)
        | (CollisionDomain::MacroGlobal, CollisionDomain::ProcessWide)
        | (CollisionDomain::MacroGlobal, CollisionDomain::MacroGlobal)
        | (CollisionDomain::RadialGlobal, CollisionDomain::MacroGlobal)
        | (CollisionDomain::MacroGlobal, CollisionDomain::RadialGlobal) => true,
        (CollisionDomain::RadialMenuLocal(left), CollisionDomain::RadialMenuLocal(right)) => {
            left == right
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, chord: &str, revision: u64, generation: Option<u64>) -> Candidate {
        Candidate {
            id: EntryId::new(id),
            owner: Owner::Launcher,
            owner_label: "Launcher".into(),
            action_label: "Toggle launcher".into(),
            chord: Some(chord.into()),
            route: RouteKind::LegacyPolling,
            scope: Scope::ProcessWide,
            source_revision: revision,
            expected_runtime_generation: generation,
            state: CandidateState::Configured,
            reason: None,
            edit: Some(EditDestination::MainSettings(MainSetting::Launcher)),
        }
    }

    fn source(candidates: Vec<Candidate>, receipts: Vec<AppliedRouteReceipt>) -> SourceSnapshot {
        SourceSnapshot::new(Source::MainSettings, 1, candidates, receipts)
    }

    #[test]
    fn hotkey_directory_reports_cofiring_end_routes_but_preserves_caps_lock_exception() {
        let end = candidate("launcher:end", "End", 1, Some(3));
        let quit = candidate("quit:end", "Ctrl+End", 1, Some(3));
        let caps = candidate("caps:bare", "CapsLock", 1, Some(3));
        let modified_caps = candidate("caps:ctrl", "Ctrl+CapsLock", 1, Some(3));
        let snapshot = Snapshot::from_sources(
            4,
            vec![source(vec![end, quit, caps, modified_caps], Vec::new())],
        );
        let end_entry = snapshot
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "launcher:end")
            .unwrap();
        assert_eq!(end_entry.status, Status::Conflict);
        assert!(end_entry.conflicts.iter().any(|conflict| {
            conflict.source == Source::MainSettings && conflict.id.as_str() == "quit:end"
        }));
        assert!(!snapshot.entries().iter().any(|entry| {
            (entry.id.as_str() == "caps:bare" || entry.id.as_str() == "caps:ctrl")
                && entry.status == Status::Conflict
        }));
    }

    #[test]
    fn hotkey_directory_requires_exact_applied_route_and_keeps_prior_receipt_pending() {
        let pending = Candidate {
            state: CandidateState::Pending { generation: 8 },
            expected_runtime_generation: Some(8),
            ..candidate("launcher", "Ctrl+F2", 5, Some(8))
        };
        let old_receipt = AppliedRouteReceipt {
            id: EntryId::new("launcher"),
            chord: "F2".into(),
            source_revision: 4,
            generation: 7,
        };
        let pending_snapshot = Snapshot::from_sources(
            1,
            vec![source(vec![pending.clone()], vec![old_receipt.clone()])],
        );
        assert_eq!(
            pending_snapshot.entries()[0].status,
            Status::Pending { generation: 8 }
        );
        assert_eq!(
            pending_snapshot.entries()[0].last_applied,
            Some(old_receipt)
        );

        let applied = AppliedRouteReceipt {
            id: EntryId::new("launcher"),
            chord: "Ctrl+F2".into(),
            source_revision: 5,
            generation: 8,
        };
        let configured = Candidate {
            state: CandidateState::Configured,
            ..pending
        };
        let active =
            Snapshot::from_sources(2, vec![source(vec![configured.clone()], vec![applied])]);
        assert_eq!(active.entries()[0].status, Status::Active { generation: 8 });

        let stale = Snapshot::from_sources(
            3,
            vec![source(
                vec![configured.clone()],
                vec![AppliedRouteReceipt {
                    id: EntryId::new("launcher"),
                    chord: "Ctrl+F2".into(),
                    source_revision: 5,
                    generation: 7,
                }],
            )],
        );
        assert_eq!(stale.entries()[0].status, Status::Unknown);

        let changed = Candidate {
            chord: Some("Ctrl+F3".into()),
            ..configured
        };
        let mismatched = Snapshot::from_sources(
            4,
            vec![source(
                vec![changed],
                vec![AppliedRouteReceipt {
                    id: EntryId::new("launcher"),
                    chord: "Ctrl+F2".into(),
                    source_revision: 5,
                    generation: 8,
                }],
            )],
        );
        assert_eq!(mismatched.entries()[0].status, Status::Unknown);
    }

    #[test]
    fn hotkey_directory_pending_source_replacement_retains_only_prior_receipt() {
        let active = AppliedRouteReceipt {
            id: EntryId::new("launcher"),
            chord: "F2".into(),
            source_revision: 4,
            generation: 9,
        };
        let initial = Snapshot::from_sources(
            1,
            vec![SourceSnapshot::new(
                Source::MainSettings,
                4,
                vec![candidate("launcher", "F2", 4, Some(9))],
                vec![active.clone()],
            )],
        );
        let pending_candidate = Candidate {
            state: CandidateState::Pending { generation: 10 },
            ..candidate("launcher", "Ctrl+F2", 5, Some(10))
        };
        let pending = initial
            .replacing_source(
                2,
                SourceSnapshot::new(Source::MainSettings, 5, vec![pending_candidate], Vec::new()),
            )
            .unwrap();
        assert_eq!(
            pending.entries()[0].status,
            Status::Pending { generation: 10 }
        );
        assert_eq!(pending.entries()[0].last_applied, Some(active));

        let uncertain = initial
            .replacing_source(
                3,
                SourceSnapshot::new(
                    Source::MainSettings,
                    5,
                    vec![Candidate {
                        state: CandidateState::Unavailable,
                        reason: Some(Reason::RuntimeRestorationUncertain),
                        ..candidate("launcher", "Ctrl+F2", 5, Some(10))
                    }],
                    Vec::new(),
                ),
            )
            .unwrap();
        assert_eq!(uncertain.entries()[0].status, Status::Unavailable);
        assert_eq!(uncertain.entries()[0].last_applied, None);
    }

    #[test]
    fn hotkey_directory_source_replacement_is_revisioned_and_payload_free() {
        let initial = Snapshot::from_sources(
            4,
            vec![source(
                vec![candidate("same-id", "F2", 1, None)],
                Vec::new(),
            )],
        );
        let duplicate_label = Candidate {
            id: EntryId::new("other-id"),
            action_label: "Toggle launcher".into(),
            ..candidate("other-id", "F3", 2, None)
        };
        let next_source =
            SourceSnapshot::new(Source::MainSettings, 2, vec![duplicate_label], Vec::new());
        let next = initial.replacing_source(5, next_source).unwrap();
        assert_eq!(next.publication(), 5);
        assert_eq!(next.entries().len(), 1);
        assert_eq!(next.entries()[0].id.as_str(), "other-id");
        assert_eq!(next.entries()[0].status, Status::Unknown);
        assert!(
            next.replacing_source(
                6,
                SourceSnapshot::new(Source::MainSettings, 1, Vec::new(), Vec::new()),
            )
            .is_none()
        );
        assert!(
            next.replacing_source(
                7,
                SourceSnapshot::new(
                    Source::MainSettings,
                    2,
                    next.sources[0].candidates.to_vec(),
                    next.sources[0].applied_routes.to_vec(),
                ),
            )
            .is_none(),
            "identical equal-revision deliveries are ignored as duplicates"
        );
        assert!(
            next.replacing_source(
                8,
                SourceSnapshot::new(
                    Source::MainSettings,
                    2,
                    vec![candidate("divergent", "F4", 2, None)],
                    Vec::new(),
                ),
            )
            .is_none(),
            "a divergent equal-revision delivery cannot replace the retained source"
        );
    }

    #[test]
    fn hotkey_directory_pending_identity_change_retains_the_old_applied_candidate_separately() {
        let old_candidate = candidate("old-route", "F2", 1, Some(9));
        let old_receipt = AppliedRouteReceipt {
            id: old_candidate.id.clone(),
            chord: "F2".into(),
            source_revision: 1,
            generation: 9,
        };
        let initial =
            Snapshot::from_sources(1, vec![source(vec![old_candidate], vec![old_receipt])]);
        let replacement = SourceSnapshot::new(
            Source::MainSettings,
            2,
            vec![Candidate {
                state: CandidateState::Pending { generation: 10 },
                ..candidate("new-route", "F3", 2, Some(10))
            }],
            Vec::new(),
        )
        .with_pending_generation(10);

        let pending = initial.replacing_source(2, replacement).unwrap();
        let old = pending
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "old-route")
            .expect("the still-applied old identity remains visible");
        assert_eq!(old.status, Status::Active { generation: 9 });
        let new = pending
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "new-route")
            .expect("the replacement candidate is separately visible");
        assert_eq!(new.status, Status::Pending { generation: 10 });
    }

    #[test]
    fn hotkey_directory_pending_removal_retains_the_old_receipt_until_the_transaction_finishes() {
        let old_candidate = candidate("removed-route", "F2", 1, Some(9));
        let old_receipt = AppliedRouteReceipt {
            id: old_candidate.id.clone(),
            chord: "F2".into(),
            source_revision: 1,
            generation: 9,
        };
        let initial =
            Snapshot::from_sources(1, vec![source(vec![old_candidate], vec![old_receipt])]);
        let removed = Candidate {
            chord: None,
            state: CandidateState::Disabled,
            expected_runtime_generation: None,
            ..candidate("removed-route", "F2", 2, None)
        };
        let replacement =
            SourceSnapshot::new(Source::MainSettings, 2, vec![removed.clone()], Vec::new())
                .with_pending_generation(10);

        let pending = initial.replacing_source(2, replacement).unwrap();
        let entry = pending
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "removed-route")
            .expect("pending removal remains visible");
        assert_eq!(entry.status, Status::Pending { generation: 10 });
        assert_eq!(entry.chord, None);
        assert_eq!(
            entry
                .last_applied
                .as_ref()
                .map(|receipt| receipt.chord.as_str()),
            Some("F2")
        );

        let completed = pending
            .replacing_source(
                3,
                SourceSnapshot::new(
                    Source::MainSettings,
                    3,
                    vec![Candidate {
                        state: CandidateState::Disabled,
                        ..removed
                    }],
                    Vec::new(),
                ),
            )
            .unwrap();
        let entry = completed
            .entries()
            .iter()
            .find(|entry| entry.id.as_str() == "removed-route")
            .unwrap();
        assert_eq!(entry.status, Status::Disabled);
        assert!(entry.last_applied.is_none());
    }
}
