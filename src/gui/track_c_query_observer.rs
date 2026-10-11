//! Bounded, opt-in receipts for query-to-usable-root-frame observation.
//!
//! The observer is attached to one `LauncherApp` and enabled only when the
//! process performance switch was enabled at app construction. It retains at
//! most one pending intent and a fixed-size window of terminal receipts. Query
//! text is kept only with the current intent for exact equality checks and is
//! never emitted.

use std::time::{Duration, Instant};

use crate::radial::dynamic::MutableResultCatalogVersions;

const RECEIPT_CAPACITY: usize = 8;
const GEOMETRY_CAUSE_COUNT: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    TextEdit,
    History,
    PendingQuery,
    CommandSet,
    CommandReplacement,
    Suggestion,
}

impl Origin {
    pub(crate) const fn token(self) -> &'static str {
        match self {
            Self::TextEdit => "text_edit",
            Self::History => "history",
            Self::PendingQuery => "pending_query",
            Self::CommandSet => "command_set",
            Self::CommandReplacement => "command_replacement",
            Self::Suggestion => "suggestion",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Terminal {
    Completed,
    Superseded,
    Cancelled,
}

impl Terminal {
    pub(crate) const fn token(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Superseded => "superseded",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cause {
    FirstUsableRootFrame,
    SupersededByQuery,
    ExternalQueryOwner,
    ExternalSourceChange,
    RootHidden,
    RootNotOwned,
    QueryChangedWithoutIntent,
}

impl Cause {
    pub(crate) const fn token(self) -> &'static str {
        match self {
            Self::FirstUsableRootFrame => "first_usable_root_frame",
            Self::SupersededByQuery => "superseded_by_query",
            Self::ExternalQueryOwner => "external_query_owner",
            Self::ExternalSourceChange => "external_source_change",
            Self::RootHidden => "root_hidden",
            Self::RootNotOwned => "root_not_owned",
            Self::QueryChangedWithoutIntent => "query_changed_without_intent",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AcceptanceKind {
    Search,
    CommandReplacement,
}

impl AcceptanceKind {
    pub(crate) const fn token(self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::CommandReplacement => "command_replacement",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub(crate) enum GeometryCause {
    Query = 0,
    ProviderOrCatalog = 1,
    DisplayOrResize = 2,
    Navigation = 3,
    Other = 4,
}

impl GeometryCause {
    pub(crate) const fn token(self) -> &'static str {
        match self {
            Self::Query => "query",
            Self::ProviderOrCatalog => "provider_or_catalog",
            Self::DisplayOrResize => "display_or_resize",
            Self::Navigation => "navigation",
            Self::Other => "other",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AcceptanceReceipt {
    pub sequence: u64,
    pub kind: AcceptanceKind,
    pub frame: u64,
    pub result_generation: u64,
    pub provider_revision: u64,
    pub catalog_versions: Option<MutableResultCatalogVersions>,
    pub catalog_versions_stable: bool,
    pub search_pending: bool,
    pub capacity_deferred: bool,
    /// Direct command replacements use their own installation-time source
    /// baseline instead of `LauncherSearchOutcome`'s search-only fields.
    pub direct_provider_before: Option<u64>,
    pub direct_provider_after: Option<u64>,
    pub direct_catalog_before: Option<MutableResultCatalogVersions>,
    pub direct_catalog_after: Option<MutableResultCatalogVersions>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Receipt {
    pub sequence: u64,
    pub origin: Origin,
    pub terminal: Terminal,
    pub cause: Cause,
    pub started_frame: u64,
    pub terminal_frame: u64,
    pub frames: u64,
    /// Time to any terminal disposition. Abandoned durations are diagnostics,
    /// never successful completion latency.
    pub terminal_elapsed_ns: u64,
    /// Only completed usable-frame latency is exposed as latency. Abandoned
    /// intents retain their disposition and frame count, never completed time.
    pub completion_latency_ns: Option<u64>,
    pub acceptance: Option<AcceptanceReceipt>,
    pub painted_sequence: Option<u64>,
    pub painted_acceptance_frame: Option<u64>,
    pub painted_frame: Option<u64>,
    pub painted_generation: Option<u64>,
    pub geometry_rebuilds: u64,
    pub geometry_rows: u64,
    pub geometry_by_cause: [GeometryWork; GEOMETRY_CAUSE_COUNT],
    #[cfg(test)]
    pub started_at: Instant,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GeometryWork {
    pub rebuilds: u64,
    pub rows: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub frame_count: u64,
    pub intent_count: u64,
    pub superseded_count: u64,
    pub cancelled_count: u64,
    pub completed_count: u64,
    pub geometry: [GeometryWork; GEOMETRY_CAUSE_COUNT],
    pub receipts: [Option<Receipt>; RECEIPT_CAPACITY],
    pub receipt_count: usize,
}

#[derive(Debug)]
struct Intent {
    sequence: u64,
    origin: Origin,
    started: Instant,
    started_frame: u64,
    query: String,
    acceptance: Option<AcceptanceReceipt>,
    painted_sequence: Option<u64>,
    painted_acceptance_frame: Option<u64>,
    painted_frame: Option<u64>,
    painted_generation: Option<u64>,
    geometry_rebuilds: u64,
    geometry_rows: u64,
    geometry_by_cause: [GeometryWork; GEOMETRY_CAUSE_COUNT],
}

/// One app-local observer. `enabled == false` is the normal state and all
/// entrypoints return before reading a clock or hashing query metadata.
#[derive(Debug, Default)]
pub(crate) struct Observer {
    enabled: bool,
    phase_enabled: bool,
    in_frame: bool,
    frame: u64,
    next_sequence: u64,
    pending: Option<Intent>,
    /// One normalized command outcome may publish the same query to both the
    /// command query and the next-frame pending-query slot. In that case the
    /// pending slot consumes the existing command intent exactly once.
    command_pending_handoff: Option<u64>,
    snapshot: Snapshot,
}

impl Observer {
    pub(crate) fn new(enabled: bool) -> Self {
        let mut observer = Self::default();
        observer.enabled = enabled;
        observer.phase_enabled = enabled;
        observer
    }

    #[cfg(test)]
    pub(crate) fn enable_for_test(&mut self) {
        let phase_enabled = self.phase_enabled;
        *self = Self::new(true);
        self.phase_enabled = phase_enabled;
    }

    #[cfg(test)]
    pub(crate) fn reset_for_test(&mut self) {
        let enabled = self.enabled;
        let phase_enabled = self.phase_enabled;
        let mut observer = Self::default();
        observer.enabled = enabled;
        observer.phase_enabled = phase_enabled;
        *self = observer;
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn has_pending(&self) -> bool {
        self.enabled && self.pending.is_some()
    }

    pub(crate) fn geometry_cause_hint(&self) -> Option<GeometryCause> {
        if !self.enabled {
            return None;
        }
        self.pending
            .as_ref()
            .filter(|intent| intent.started_frame == self.frame)
            .map(|intent| match intent.origin {
                Origin::TextEdit
                | Origin::History
                | Origin::PendingQuery
                | Origin::CommandSet
                | Origin::Suggestion => GeometryCause::Query,
                Origin::CommandReplacement => GeometryCause::Other,
            })
    }

    pub(crate) fn tentative_start(&self) -> Option<Instant> {
        self.enabled.then(Instant::now)
    }

    pub(crate) fn begin_frame(&mut self) {
        if !self.enabled {
            return;
        }
        self.frame = self.frame.saturating_add(1);
        self.in_frame = true;
        self.snapshot.frame_count = self.snapshot.frame_count.saturating_add(1);
    }

    pub(crate) fn end_frame_boundary(&mut self) {
        if self.enabled {
            self.in_frame = false;
        }
    }

    pub(crate) fn begin_from(
        &mut self,
        started: Option<Instant>,
        origin: Origin,
        query: &str,
    ) -> Option<u64> {
        if !self.enabled {
            return None;
        }
        self.command_pending_handoff = None;
        let started = started.unwrap_or_else(Instant::now);
        if self.pending.is_some() {
            self.finish_pending(Terminal::Superseded, Cause::SupersededByQuery, None);
        }
        self.next_sequence = self.next_sequence.wrapping_add(1).max(1);
        let sequence = self.next_sequence;
        let started_frame = if self.in_frame {
            self.frame
        } else {
            self.frame.saturating_add(1)
        };
        self.pending = Some(Intent {
            sequence,
            origin,
            started,
            started_frame,
            query: query.to_owned(),
            acceptance: None,
            painted_sequence: None,
            painted_acceptance_frame: None,
            painted_frame: None,
            painted_generation: None,
            geometry_rebuilds: 0,
            geometry_rows: 0,
            geometry_by_cause: [GeometryWork::default(); GEOMETRY_CAUSE_COUNT],
        });
        self.snapshot.intent_count = self.snapshot.intent_count.saturating_add(1);
        Some(sequence)
    }

    /// Clear any command/pending association before an independent owner
    /// publishes `pending_query`. Equal text does not make it the same intent.
    pub(crate) fn pending_query_published(&mut self) {
        if self.enabled {
            self.command_pending_handoff = None;
        }
    }

    /// Arm the one-shot handoff only while the original command intent is
    /// still current and the normalized query values match exactly.
    pub(crate) fn arm_command_pending_handoff(&mut self, sequence: Option<u64>, query: &str) {
        if !self.enabled {
            return;
        }
        let Some(sequence) = sequence else {
            return;
        };
        if self.pending.as_ref().is_some_and(|intent| {
            intent.sequence == sequence
                && intent.origin == Origin::CommandSet
                && intent.query == query
        }) {
            self.command_pending_handoff = Some(sequence);
        }
    }

    /// Consume the pending-query slot. A matching paired command preserves
    /// its sequence, start time, and origin; every other publication starts a
    /// distinct pending-query intent, even when its text happens to be equal.
    pub(crate) fn begin_pending_query(&mut self, query: &str) -> Option<u64> {
        if !self.enabled {
            return None;
        }
        let handoff = self.command_pending_handoff.take();
        if let Some(sequence) = handoff
            && self.pending.as_ref().is_some_and(|intent| {
                intent.sequence == sequence
                    && intent.origin == Origin::CommandSet
                    && intent.query == query
            })
        {
            return Some(sequence);
        }
        self.begin_from(Some(Instant::now()), Origin::PendingQuery, query)
    }

    #[cfg(test)]
    pub(crate) fn pending_started_at_for_test(&self) -> Option<Instant> {
        self.pending.as_ref().map(|intent| intent.started)
    }

    pub(crate) fn supersede_external(&mut self) {
        if self.enabled && self.pending.is_some() {
            self.finish_pending(Terminal::Superseded, Cause::ExternalQueryOwner, None);
        }
    }

    pub(crate) fn accept_search(
        &mut self,
        query: &str,
        result_generation: u64,
        provider_revision: u64,
        catalog_versions: Option<MutableResultCatalogVersions>,
        catalog_versions_stable: bool,
        search_pending: bool,
        capacity_deferred: bool,
    ) {
        if !self.enabled {
            return;
        }
        let Some(intent) = self.pending.as_mut() else {
            return;
        };
        if intent.query != query {
            return;
        }
        // A new accepted installation must be painted in a frame that follows
        // this receipt, even when C2-C preserves the result generation.
        intent.painted_sequence = None;
        intent.painted_acceptance_frame = None;
        intent.painted_frame = None;
        intent.painted_generation = None;
        intent.acceptance = Some(AcceptanceReceipt {
            sequence: intent.sequence,
            kind: AcceptanceKind::Search,
            frame: self.frame,
            result_generation,
            provider_revision,
            catalog_versions,
            catalog_versions_stable,
            search_pending,
            capacity_deferred,
            direct_provider_before: None,
            direct_provider_after: None,
            direct_catalog_before: None,
            direct_catalog_after: None,
        });
    }

    /// A new search attempt invalidates a previous direct-install receipt until
    /// this attempt produces its own accepted search result.
    pub(crate) fn search_started(&mut self) {
        if self.enabled
            && let Some(intent) = self.pending.as_mut()
        {
            intent.acceptance = None;
            intent.painted_frame = None;
            intent.painted_generation = None;
        }
    }

    /// A new result generation invalidates acceptance against the old result
    /// set. The same current query intent may be accepted again after search.
    pub(crate) fn results_invalidated(&mut self) {
        if self.enabled
            && let Some(intent) = self.pending.as_mut()
        {
            intent.acceptance = None;
            intent.painted_frame = None;
            intent.painted_generation = None;
        }
    }

    pub(crate) fn accept_command_replacement(
        &mut self,
        query: &str,
        result_generation: u64,
        provider_before: u64,
        provider_after: u64,
        catalog_before: MutableResultCatalogVersions,
        catalog_after: MutableResultCatalogVersions,
    ) {
        if !self.enabled {
            return;
        }
        let Some(intent) = self.pending.as_mut() else {
            return;
        };
        if intent.query != query {
            return;
        }
        intent.painted_sequence = None;
        intent.painted_acceptance_frame = None;
        intent.painted_frame = None;
        intent.painted_generation = None;
        intent.acceptance = Some(AcceptanceReceipt {
            sequence: intent.sequence,
            kind: AcceptanceKind::CommandReplacement,
            frame: self.frame,
            result_generation,
            provider_revision: provider_after,
            catalog_versions: Some(catalog_after),
            catalog_versions_stable: provider_before == provider_after
                && catalog_before == catalog_after,
            search_pending: false,
            capacity_deferred: false,
            direct_provider_before: Some(provider_before),
            direct_provider_after: Some(provider_after),
            direct_catalog_before: Some(catalog_before),
            direct_catalog_after: Some(catalog_after),
        });
    }

    pub(crate) fn note_painted_root(&mut self, generation: u64) -> bool {
        if !self.enabled {
            return false;
        }
        if let Some(intent) = self.pending.as_mut()
            && let Some(accepted) = intent
                .acceptance
                .filter(|accepted| accepted.result_generation == generation)
        {
            intent.painted_sequence = Some(accepted.sequence);
            intent.painted_acceptance_frame = Some(accepted.frame);
            intent.painted_frame = Some(self.frame);
            intent.painted_generation = Some(generation);
            true
        } else {
            false
        }
    }

    pub(crate) fn note_geometry_rebuild(&mut self, cause: GeometryCause, rows: usize) {
        if !self.enabled {
            return;
        }
        let work = &mut self.snapshot.geometry[cause as usize];
        work.rebuilds = work.rebuilds.saturating_add(1);
        work.rows = work.rows.saturating_add(rows.min(u64::MAX as usize) as u64);
        if let Some(intent) = self.pending.as_mut() {
            intent.geometry_rebuilds = intent.geometry_rebuilds.saturating_add(1);
            intent.geometry_rows = intent
                .geometry_rows
                .saturating_add(rows.min(u64::MAX as usize) as u64);
            let attributed = &mut intent.geometry_by_cause[cause as usize];
            attributed.rebuilds = attributed.rebuilds.saturating_add(1);
            attributed.rows = attributed
                .rows
                .saturating_add(rows.min(u64::MAX as usize) as u64);
        }
    }

    /// Completes only an accepted installation that this frame actually
    /// painted, after the caller has rechecked all current root/source gates.
    pub(crate) fn finish_frame(
        &mut self,
        query: &str,
        root_visible: bool,
        root_owned: bool,
        root_gates_clear: bool,
        current_provider_revision: u64,
        current_catalog_versions: MutableResultCatalogVersions,
        painted_root_generation: Option<u64>,
    ) {
        if !self.enabled {
            return;
        }
        let Some(intent) = self.pending.as_ref() else {
            return;
        };
        if intent.query != query {
            self.finish_pending(Terminal::Superseded, Cause::QueryChangedWithoutIntent, None);
            return;
        }
        if !root_visible {
            self.finish_pending(Terminal::Cancelled, Cause::RootHidden, None);
            return;
        }
        if !root_owned {
            self.finish_pending(Terminal::Cancelled, Cause::RootNotOwned, None);
            return;
        }
        let Some(accepted) = intent.acceptance else {
            return;
        };
        let Some(painted_generation) = painted_root_generation else {
            return;
        };
        let paint_matches = intent.painted_sequence == Some(intent.sequence)
            && intent.painted_acceptance_frame == Some(accepted.frame)
            && intent.painted_frame == Some(self.frame)
            && accepted.sequence == intent.sequence
            && accepted.frame <= self.frame
            && intent.painted_generation == Some(painted_generation)
            && accepted.result_generation == painted_generation;
        let search_sources_current = match accepted.kind {
            AcceptanceKind::Search => {
                accepted.catalog_versions_stable
                    && !accepted.search_pending
                    && !accepted.capacity_deferred
                    && accepted.provider_revision == current_provider_revision
                    && accepted.catalog_versions == Some(current_catalog_versions)
            }
            AcceptanceKind::CommandReplacement => {
                accepted.catalog_versions_stable
                    && accepted.direct_provider_before == accepted.direct_provider_after
                    && accepted.direct_provider_after == Some(current_provider_revision)
                    && accepted.direct_catalog_before == accepted.direct_catalog_after
                    && accepted.direct_catalog_after == Some(current_catalog_versions)
            }
        };
        if !search_sources_current
            && accepted.kind == AcceptanceKind::CommandReplacement
            && (accepted.direct_provider_before != accepted.direct_provider_after
                || accepted.direct_provider_after != Some(current_provider_revision)
                || accepted.direct_catalog_before != accepted.direct_catalog_after
                || accepted.direct_catalog_after != Some(current_catalog_versions))
        {
            self.finish_pending(Terminal::Cancelled, Cause::ExternalSourceChange, None);
        } else if root_gates_clear && search_sources_current && paint_matches {
            self.finish_pending(
                Terminal::Completed,
                Cause::FirstUsableRootFrame,
                Some(painted_generation),
            );
        }
    }

    pub(crate) fn cancel(&mut self, cause: Cause) {
        if self.enabled && self.pending.is_some() {
            self.finish_pending(Terminal::Cancelled, cause, None);
        }
    }

    fn finish_pending(
        &mut self,
        terminal: Terminal,
        cause: Cause,
        painted_generation: Option<u64>,
    ) {
        let Some(intent) = self.pending.take() else {
            return;
        };
        if self.command_pending_handoff == Some(intent.sequence) {
            self.command_pending_handoff = None;
        }
        let terminal_frame = self.frame;
        let frames = terminal_frame
            .saturating_sub(intent.started_frame)
            .saturating_add(u64::from(self.in_frame));
        let completion_latency_ns = (terminal == Terminal::Completed)
            .then(|| intent.started.elapsed().as_nanos().min(u64::MAX as u128) as u64);
        let terminal_elapsed_ns = intent.started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        match terminal {
            Terminal::Completed => {
                self.snapshot.completed_count = self.snapshot.completed_count.saturating_add(1);
            }
            Terminal::Superseded => {
                self.snapshot.superseded_count = self.snapshot.superseded_count.saturating_add(1);
            }
            Terminal::Cancelled => {
                self.snapshot.cancelled_count = self.snapshot.cancelled_count.saturating_add(1);
            }
        }
        self.push_receipt(Receipt {
            sequence: intent.sequence,
            origin: intent.origin,
            terminal,
            cause,
            started_frame: intent.started_frame,
            terminal_frame,
            frames,
            terminal_elapsed_ns,
            completion_latency_ns,
            acceptance: intent.acceptance,
            painted_sequence: intent.painted_sequence,
            painted_acceptance_frame: intent.painted_acceptance_frame,
            painted_frame: intent.painted_frame,
            painted_generation: painted_generation.or(intent.painted_generation),
            geometry_rebuilds: intent.geometry_rebuilds,
            geometry_rows: intent.geometry_rows,
            geometry_by_cause: intent.geometry_by_cause,
            #[cfg(test)]
            started_at: intent.started,
        });
        let metric_outcome = if terminal == Terminal::Completed {
            crate::performance::track_c::Outcome::Completed
        } else {
            crate::performance::track_c::Outcome::Abandoned
        };
        // Only completed receipts contribute elapsed latency to the new phase.
        // Abandoned timing remains available in the bounded observer receipt,
        // but is not mixed into a time-to-usable-frame distribution.
        let phase_elapsed = if terminal == Terminal::Completed {
            Duration::from_nanos(terminal_elapsed_ns)
        } else {
            Duration::ZERO
        };
        crate::performance::track_c::record_sample_if(
            self.phase_enabled,
            crate::performance::track_c::Phase::GuiQueryToUsableCpuFrame,
            usize::try_from(frames).unwrap_or(usize::MAX),
            phase_elapsed,
            metric_outcome,
        );
    }

    fn push_receipt(&mut self, receipt: Receipt) {
        if self.snapshot.receipt_count < RECEIPT_CAPACITY {
            self.snapshot.receipts[self.snapshot.receipt_count] = Some(receipt);
            self.snapshot.receipt_count += 1;
            return;
        }
        self.snapshot.receipts.copy_within(1..RECEIPT_CAPACITY, 0);
        self.snapshot.receipts[RECEIPT_CAPACITY - 1] = Some(receipt);
    }

    #[cfg(test)]
    pub(crate) fn snapshot(&self) -> Snapshot {
        self.snapshot
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        if self.enabled && self.pending.is_some() {
            self.finish_pending(Terminal::Cancelled, Cause::RootNotOwned, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog_versions() -> MutableResultCatalogVersions {
        MutableResultCatalogVersions {
            clipboard: 0,
            todo: 0,
            notes: 0,
        }
    }

    #[test]
    fn lifecycle_rejects_stale_paint_and_direct_source_drift_and_keeps_disabled_path_inert() {
        let mut disabled = Observer::new(false);
        assert!(disabled.tentative_start().is_none());
        disabled.begin_frame();
        assert!(
            disabled
                .begin_from(None, Origin::TextEdit, "disabled query")
                .is_none()
        );
        disabled.accept_search("disabled query", 1, 0, None, true, false, false);
        assert!(!disabled.note_painted_root(1));
        assert_eq!(disabled.snapshot(), Snapshot::default());

        let versions = catalog_versions();
        let mut observer = Observer::new(true);
        observer.begin_from(observer.tentative_start(), Origin::TextEdit, "A");
        observer.begin_frame();
        observer.accept_search("A", 7, 2, Some(versions), true, false, false);
        assert!(observer.note_painted_root(7));
        observer.finish_frame("A", true, true, true, 2, versions, Some(7));
        assert_eq!(observer.snapshot().completed_count, 1);
        observer.finish_frame("A", true, true, true, 2, versions, Some(7));
        assert_eq!(
            observer.snapshot().receipt_count,
            1,
            "a terminal receipt is emitted once"
        );
        observer.end_frame_boundary();

        // A second acceptance with the same generation must earn a new paint.
        // Simulate a paint, then both sources of invalidation used by the
        // real search owner, before accepting the replacement receipt.
        observer.begin_from(observer.tentative_start(), Origin::TextEdit, "A");
        observer.begin_frame();
        observer.accept_search("A", 7, 2, Some(versions), true, false, false);
        assert!(observer.note_painted_root(7));

        // Reaccepting the same generation after that paint invalidates the
        // previous painted receipt by itself.
        observer.accept_search("A", 7, 2, Some(versions), true, false, false);
        observer.finish_frame("A", true, true, true, 2, versions, Some(7));
        assert_eq!(observer.snapshot().completed_count, 1);
        assert!(observer.has_pending());
        assert!(observer.note_painted_root(7));

        observer.search_started();
        assert!(!observer.note_painted_root(7));

        observer.accept_search("A", 7, 2, Some(versions), true, false, false);
        assert!(observer.note_painted_root(7));
        observer.results_invalidated();
        assert!(!observer.note_painted_root(7));

        observer.accept_search("A", 7, 2, Some(versions), true, false, false);
        observer.finish_frame("A", true, true, true, 2, versions, Some(7));
        assert_eq!(observer.snapshot().completed_count, 1);
        assert!(observer.has_pending());
        assert!(observer.note_painted_root(7));
        observer.finish_frame("A", true, true, true, 2, versions, Some(7));
        assert_eq!(observer.snapshot().completed_count, 2);
        observer.end_frame_boundary();

        observer.begin_from(observer.tentative_start(), Origin::TextEdit, "A");
        observer.begin_frame();
        observer.begin_from(observer.tentative_start(), Origin::TextEdit, "B");
        observer.begin_from(observer.tentative_start(), Origin::TextEdit, "A");
        assert!(
            !observer.note_painted_root(7),
            "an old A acceptance cannot paint a new A intent"
        );
        observer.accept_search("A", 8, 2, Some(versions), true, false, false);
        assert!(observer.note_painted_root(8));
        observer.finish_frame("A", true, true, true, 3, versions, Some(8));
        assert_eq!(
            observer.snapshot().completed_count,
            2,
            "a painted result from an older provider revision cannot complete"
        );
        observer.end_frame_boundary();

        observer.begin_frame();
        observer.accept_search("A", 8, 3, Some(versions), true, false, false);
        assert!(observer.note_painted_root(8));
        observer.finish_frame("A", true, true, true, 3, versions, Some(8));
        assert_eq!(observer.snapshot().completed_count, 3);
        observer.end_frame_boundary();

        observer.begin_from(
            observer.tentative_start(),
            Origin::CommandReplacement,
            "command query",
        );
        observer.begin_frame();
        observer.accept_command_replacement("command query", 9, 3, 4, versions, versions);
        assert!(observer.note_painted_root(9));
        observer.search_started();
        assert!(!observer.note_painted_root(9));
        observer.results_invalidated();
        observer.accept_search("command query", 9, 3, Some(versions), true, false, false);
        assert!(observer.note_painted_root(9));
        observer.finish_frame("command query", true, true, true, 3, versions, Some(9));
        assert_eq!(observer.snapshot().completed_count, 4);
        let search_precedence =
            observer.snapshot().receipts[observer.snapshot().receipt_count - 1].unwrap();
        assert_eq!(
            search_precedence.acceptance.unwrap().kind,
            AcceptanceKind::Search,
            "a later search acceptance supersedes a direct replacement receipt"
        );
        observer.end_frame_boundary();

        observer.begin_from(
            observer.tentative_start(),
            Origin::CommandReplacement,
            "source drift",
        );
        observer.begin_frame();
        observer.accept_command_replacement("source drift", 10, 3, 4, versions, versions);
        assert!(observer.note_painted_root(10));
        observer.finish_frame("source drift", true, true, true, 4, versions, Some(10));
        observer.end_frame_boundary();

        observer.begin_from(observer.tentative_start(), Origin::TextEdit, "hidden query");
        observer.begin_frame();
        observer.accept_search("hidden query", 11, 4, Some(versions), true, false, false);
        assert!(observer.note_painted_root(11));
        observer.finish_frame("hidden query", false, true, true, 4, versions, Some(11));
        observer.end_frame_boundary();

        let snapshot = observer.snapshot();
        assert_eq!(snapshot.completed_count, 4);
        assert_eq!(snapshot.superseded_count, 2);
        assert_eq!(snapshot.cancelled_count, 2);
        let hidden = snapshot.receipts[snapshot.receipt_count - 1].unwrap();
        assert_eq!(hidden.terminal, Terminal::Cancelled);
        assert_eq!(hidden.cause, Cause::RootHidden);
        let source_drift = snapshot.receipts[snapshot.receipt_count - 2].unwrap();
        assert_eq!(source_drift.terminal, Terminal::Cancelled);
        assert_eq!(source_drift.cause, Cause::ExternalSourceChange);
        let direct = source_drift.acceptance.unwrap();
        assert_eq!(direct.kind, AcceptanceKind::CommandReplacement);
        assert_eq!(direct.direct_provider_before, Some(3));
        assert_eq!(direct.direct_provider_after, Some(4));
    }
}
