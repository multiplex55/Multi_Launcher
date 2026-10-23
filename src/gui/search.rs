use super::*;

pub(crate) const NOTE_SEARCH_DEBOUNCE: Duration = Duration::from_secs(1);
pub(crate) const COMPLETION_REBUILD_DEBOUNCE: Duration = Duration::from_millis(120);

#[cfg(test)]
pub(super) mod c2c_install_observer {
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub(in crate::gui) struct Receipt {
        pub(in crate::gui) comparison_calls: u64,
        pub(in crate::gui) comparison_ns: u64,
        pub(in crate::gui) comparison_input_results: u64,
        pub(in crate::gui) equal_reuses: u64,
        pub(in crate::gui) discarded_incoming_actions: u64,
        pub(in crate::gui) replacements: u64,
        pub(in crate::gui) installed_actions: u64,
        pub(in crate::gui) installation_ns: u64,
    }

    impl Receipt {
        pub(in crate::gui) fn arrays(self) -> [u64; 8] {
            [
                self.comparison_calls,
                self.comparison_ns,
                self.comparison_input_results,
                self.equal_reuses,
                self.discarded_incoming_actions,
                self.replacements,
                self.installed_actions,
                self.installation_ns,
            ]
        }
    }

    #[derive(Default)]
    struct Counters {
        values: [Cell<u64>; 8],
    }

    #[derive(Clone, Copy)]
    #[repr(usize)]
    pub(super) enum Counter {
        ComparisonCalls,
        ComparisonNanoseconds,
        ComparisonInputResults,
        EqualReuses,
        DiscardedIncomingActions,
        Replacements,
        InstalledActions,
        InstallationNanoseconds,
    }

    impl Counters {
        fn add(&self, counter: Counter, amount: u64) {
            let value = &self.values[counter as usize];
            value.set(value.get().saturating_add(amount));
        }

        fn receipt(&self) -> Receipt {
            let values = &self.values;
            Receipt {
                comparison_calls: values[Counter::ComparisonCalls as usize].get(),
                comparison_ns: values[Counter::ComparisonNanoseconds as usize].get(),
                comparison_input_results: values[Counter::ComparisonInputResults as usize].get(),
                equal_reuses: values[Counter::EqualReuses as usize].get(),
                discarded_incoming_actions: values[Counter::DiscardedIncomingActions as usize]
                    .get(),
                replacements: values[Counter::Replacements as usize].get(),
                installed_actions: values[Counter::InstalledActions as usize].get(),
                installation_ns: values[Counter::InstallationNanoseconds as usize].get(),
            }
        }
    }

    thread_local! {
        static ACTIVE: RefCell<Option<Rc<Counters>>> = const { RefCell::new(None) };
    }

    pub(in crate::gui) struct Scope {
        counters: Rc<Counters>,
        previous: Option<Rc<Counters>>,
    }

    impl Scope {
        pub(in crate::gui) fn install() -> Self {
            let counters = Rc::new(Counters::default());
            let previous = ACTIVE.with(|active| active.replace(Some(Rc::clone(&counters))));
            Self { counters, previous }
        }

        pub(in crate::gui) fn receipt(&self) -> Receipt {
            self.counters.receipt()
        }
    }

    impl Drop for Scope {
        fn drop(&mut self) {
            ACTIVE.with(|active| {
                let current = active.replace(self.previous.take());
                debug_assert!(
                    current.is_some_and(|current| Rc::ptr_eq(&current, &self.counters)),
                    "C2-C installation observers must be restored in stack order"
                );
            });
        }
    }

    #[derive(Clone)]
    pub(super) struct Handle(Rc<Counters>);

    impl Handle {
        pub(super) fn add(&self, counter: Counter, amount: u64) {
            self.0.add(counter, amount);
        }
    }

    pub(super) fn current() -> Option<Handle> {
        ACTIVE.with(|active| {
            active
                .borrow()
                .as_ref()
                .map(|counters| Handle(Rc::clone(counters)))
        })
    }
}

#[cfg(test)]
mod c2a_search_observer {
    use std::{cell::Cell, cell::RefCell, rc::Rc};

    use crate::actions::Action;

    #[derive(Clone, Copy, Debug, Default)]
    pub(super) struct Receipt {
        pub local_catalog_visits: u64,
        pub local_filter_passes: u64,
        pub local_score_attempts: u64,
        pub local_hits: u64,
        pub local_action_clones: u64,
        pub local_clone_logical_bytes: u64,
        pub local_clone_strings: u64,
        pub provider_input_actions: u64,
        pub provider_filter_rejections: u64,
        pub provider_actions_retained_moved: u64,
        pub provider_projection_strings: u64,
        pub provider_projection_bytes: u64,
        pub command_fallback_actions_visited: u64,
        pub command_fallback_hits_cloned: u64,
        pub command_fallback_clone_logical_bytes: u64,
        pub command_fallback_clone_strings: u64,
        pub sort_input_len: u64,
        pub sort_ns: u64,
        pub final_scored_entries_moved: u64,
        pub final_local_action_clones: u64,
        pub final_local_clone_logical_bytes: u64,
        pub final_local_clone_strings: u64,
        pub empty_browse_command_cache_cloned: u64,
        pub empty_browse_catalog_materialized: u64,
        pub empty_browse_logical_bytes: u64,
        pub empty_browse_strings: u64,
        pub final_output_count: u64,
        pub outcome_state: u64,
    }

    impl Receipt {
        pub(super) fn arrays(self) -> [u64; 26] {
            [
                self.local_catalog_visits,
                self.local_filter_passes,
                self.local_score_attempts,
                self.local_hits,
                self.local_action_clones,
                self.local_clone_logical_bytes,
                self.local_clone_strings,
                self.provider_input_actions,
                self.provider_filter_rejections,
                self.provider_actions_retained_moved,
                self.provider_projection_strings,
                self.provider_projection_bytes,
                self.command_fallback_actions_visited,
                self.command_fallback_hits_cloned,
                self.command_fallback_clone_logical_bytes,
                self.command_fallback_clone_strings,
                self.sort_input_len,
                self.sort_ns,
                self.final_scored_entries_moved,
                0, // Final materialization maps and moves the Action values.
                self.empty_browse_command_cache_cloned,
                self.empty_browse_catalog_materialized,
                self.empty_browse_logical_bytes,
                self.empty_browse_strings,
                self.final_output_count,
                self.outcome_state,
            ]
        }

        pub(super) fn materialization_arrays(self) -> [u64; 31] {
            [
                self.local_catalog_visits,
                self.local_filter_passes,
                self.local_score_attempts,
                self.local_hits,
                self.local_action_clones,
                self.local_clone_logical_bytes,
                self.local_clone_strings,
                self.provider_input_actions,
                self.provider_filter_rejections,
                self.provider_actions_retained_moved,
                self.provider_projection_strings,
                self.provider_projection_bytes,
                self.command_fallback_actions_visited,
                self.command_fallback_hits_cloned,
                self.command_fallback_clone_logical_bytes,
                self.command_fallback_clone_strings,
                self.sort_input_len,
                self.sort_ns,
                self.final_scored_entries_moved,
                self.final_local_action_clones,
                self.empty_browse_command_cache_cloned,
                self.empty_browse_catalog_materialized,
                self.empty_browse_logical_bytes,
                self.empty_browse_strings,
                self.final_output_count,
                self.outcome_state,
                self.final_local_clone_logical_bytes,
                self.final_local_clone_strings,
                self.local_action_clones
                    .saturating_add(self.final_local_action_clones),
                self.local_clone_logical_bytes
                    .saturating_add(self.final_local_clone_logical_bytes),
                self.local_clone_strings
                    .saturating_add(self.final_local_clone_strings),
            ]
        }
    }

    #[derive(Default)]
    struct Counters {
        values: [Cell<u64>; 28],
        empty_cache_payload_bytes: u64,
        empty_cache_payload_strings: u64,
    }

    #[derive(Clone, Copy)]
    #[repr(usize)]
    pub(super) enum Counter {
        LocalCatalogVisits,
        LocalFilterPasses,
        LocalScoreAttempts,
        LocalHits,
        LocalActionClones,
        LocalCloneLogicalBytes,
        LocalCloneStrings,
        ProviderInputActions,
        ProviderFilterRejections,
        ProviderActionsRetainedMoved,
        ProviderProjectionStrings,
        ProviderProjectionBytes,
        CommandFallbackActionsVisited,
        CommandFallbackHitsCloned,
        CommandFallbackCloneLogicalBytes,
        CommandFallbackCloneStrings,
        SortInputLen,
        SortNanoseconds,
        FinalScoredEntriesMoved,
        FinalLocalActionClones,
        FinalLocalCloneLogicalBytes,
        FinalLocalCloneStrings,
        EmptyBrowseCommandCacheCloned,
        EmptyBrowseCatalogMaterialized,
        EmptyBrowseLogicalBytes,
        EmptyBrowseStrings,
        FinalOutputCount,
        OutcomeState,
    }

    impl Counters {
        fn add(&self, counter: Counter, amount: u64) {
            let value = &self.values[counter as usize];
            value.set(value.get().saturating_add(amount));
        }

        fn set(&self, counter: Counter, value: u64) {
            self.values[counter as usize].set(value);
        }

        fn receipt(&self) -> Receipt {
            let v = &self.values;
            Receipt {
                local_catalog_visits: v[Counter::LocalCatalogVisits as usize].get(),
                local_filter_passes: v[Counter::LocalFilterPasses as usize].get(),
                local_score_attempts: v[Counter::LocalScoreAttempts as usize].get(),
                local_hits: v[Counter::LocalHits as usize].get(),
                local_action_clones: v[Counter::LocalActionClones as usize].get(),
                local_clone_logical_bytes: v[Counter::LocalCloneLogicalBytes as usize].get(),
                local_clone_strings: v[Counter::LocalCloneStrings as usize].get(),
                provider_input_actions: v[Counter::ProviderInputActions as usize].get(),
                provider_filter_rejections: v[Counter::ProviderFilterRejections as usize].get(),
                provider_actions_retained_moved: v[Counter::ProviderActionsRetainedMoved as usize]
                    .get(),
                provider_projection_strings: v[Counter::ProviderProjectionStrings as usize].get(),
                provider_projection_bytes: v[Counter::ProviderProjectionBytes as usize].get(),
                command_fallback_actions_visited: v
                    [Counter::CommandFallbackActionsVisited as usize]
                    .get(),
                command_fallback_hits_cloned: v[Counter::CommandFallbackHitsCloned as usize].get(),
                command_fallback_clone_logical_bytes: v
                    [Counter::CommandFallbackCloneLogicalBytes as usize]
                    .get(),
                command_fallback_clone_strings: v[Counter::CommandFallbackCloneStrings as usize]
                    .get(),
                sort_input_len: v[Counter::SortInputLen as usize].get(),
                sort_ns: v[Counter::SortNanoseconds as usize].get(),
                final_scored_entries_moved: v[Counter::FinalScoredEntriesMoved as usize].get(),
                final_local_action_clones: v[Counter::FinalLocalActionClones as usize].get(),
                final_local_clone_logical_bytes: v[Counter::FinalLocalCloneLogicalBytes as usize]
                    .get(),
                final_local_clone_strings: v[Counter::FinalLocalCloneStrings as usize].get(),
                empty_browse_command_cache_cloned: v
                    [Counter::EmptyBrowseCommandCacheCloned as usize]
                    .get(),
                empty_browse_catalog_materialized: v
                    [Counter::EmptyBrowseCatalogMaterialized as usize]
                    .get(),
                empty_browse_logical_bytes: v[Counter::EmptyBrowseLogicalBytes as usize].get(),
                empty_browse_strings: v[Counter::EmptyBrowseStrings as usize].get(),
                final_output_count: v[Counter::FinalOutputCount as usize].get(),
                outcome_state: v[Counter::OutcomeState as usize].get(),
            }
        }
    }

    thread_local! {
        static ACTIVE: RefCell<Option<Rc<Counters>>> = const { RefCell::new(None) };
    }

    pub(super) struct Scope {
        counters: Rc<Counters>,
        previous: Option<Rc<Counters>>,
        active: bool,
    }

    impl Scope {
        pub(super) fn new(command_cache: &[Action]) -> Self {
            let empty_cache_payload_bytes = command_cache.iter().fold(0_u64, |sum, action| {
                sum.saturating_add(action_payload_bytes(action))
            });
            let empty_cache_payload_strings = command_cache.iter().fold(0_u64, |sum, action| {
                sum.saturating_add(action_payload_strings(action))
            });
            Self {
                counters: Rc::new(Counters {
                    empty_cache_payload_bytes,
                    empty_cache_payload_strings,
                    ..Counters::default()
                }),
                previous: None,
                active: false,
            }
        }

        pub(super) fn activate(&mut self) -> Activation<'_> {
            assert!(
                !self.active,
                "C2-A search observer cannot be nested on itself"
            );
            self.previous =
                ACTIVE.with(|active| active.borrow_mut().replace(Rc::clone(&self.counters)));
            self.active = true;
            Activation { scope: self }
        }

        pub(super) fn receipt(&self) -> Receipt {
            self.counters.receipt()
        }
    }

    #[derive(Clone)]
    pub(super) struct Handle(Rc<Counters>);

    impl Handle {
        pub(super) fn add(&self, counter: Metric, amount: u64) {
            self.0.add(counter, amount);
        }

        pub(super) fn set(&self, counter: Metric, value: u64) {
            self.0.set(counter, value);
        }

        pub(super) fn empty_cache_bytes(&self) -> u64 {
            self.0.empty_cache_payload_bytes
        }

        pub(super) fn empty_cache_strings(&self) -> u64 {
            self.0.empty_cache_payload_strings
        }
    }

    pub(super) struct Activation<'a> {
        scope: &'a mut Scope,
    }

    impl Drop for Activation<'_> {
        fn drop(&mut self) {
            if !self.scope.active {
                return;
            }
            ACTIVE.with(|active| {
                let mut active = active.borrow_mut();
                let current = active.take();
                debug_assert!(
                    current.is_some_and(|value| Rc::ptr_eq(&value, &self.scope.counters))
                );
                *active = self.scope.previous.take();
            });
            self.scope.active = false;
        }
    }

    pub(super) fn current() -> Option<Handle> {
        ACTIVE.with(|active| {
            active
                .borrow()
                .as_ref()
                .map(|counters| Handle(Rc::clone(counters)))
        })
    }

    pub(super) fn action_payload_bytes(action: &Action) -> u64 {
        (action.label.len() as u64)
            .saturating_add(action.desc.len() as u64)
            .saturating_add(action.action.len() as u64)
            .saturating_add(action.args.as_ref().map_or(0, |args| args.len() as u64))
    }

    pub(super) fn action_payload_strings(action: &Action) -> u64 {
        3 + u64::from(action.args.is_some())
    }

    pub(super) use Counter as Metric;
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LauncherSearchState {
    Results,
    NoResults,
    Pending,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ProviderSearchDeferral {
    #[default]
    None,
    Capacity,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LauncherSearchOutcome {
    pub state: LauncherSearchState,
    pub actions: Vec<Action>,
    pub provider_revision: u64,
    pub result_catalog_versions: Option<crate::radial::dynamic::MutableResultCatalogVersions>,
    pub result_catalog_versions_stable: bool,
    pub(super) provider_deferral: ProviderSearchDeferral,
}

impl LauncherApp {
    fn normalize_alias(alias: Option<String>) -> (Option<String>, Option<String>) {
        let alias_lc = alias.as_ref().map(|text| text.to_lowercase());
        (alias, alias_lc)
    }

    pub(crate) fn try_folder_alias_maps() -> anyhow::Result<(
        HashMap<String, Option<String>>,
        HashMap<String, Option<String>>,
    )> {
        Self::folder_alias_maps(crate::plugins::folders::load_folders(
            crate::plugins::folders::FOLDERS_FILE,
        )?)
    }

    /// Reload aliases from a WatchEvent whose dispatcher already holds the
    /// exact folders.json read permit. Re-entering the public loader here
    /// would acquire a nested permit and could reject an event admitted just
    /// before a repair fence was installed.
    pub(crate) fn try_folder_alias_maps_for_admitted_read() -> anyhow::Result<(
        HashMap<String, Option<String>>,
        HashMap<String, Option<String>>,
    )> {
        let folders = match crate::plugins::folders::load_folders_typed_unfenced(
            crate::plugins::folders::FOLDERS_FILE,
        )? {
            crate::common::persistence::LoadState::Missing
            | crate::common::persistence::LoadState::Empty => {
                crate::plugins::folders::default_folders()
            }
            crate::common::persistence::LoadState::Loaded(folders) => folders,
        };
        Self::folder_alias_maps(folders)
    }

    fn folder_alias_maps(
        folders: Vec<crate::plugins::folders::FolderEntry>,
    ) -> anyhow::Result<(
        HashMap<String, Option<String>>,
        HashMap<String, Option<String>>,
    )> {
        let mut aliases = HashMap::new();
        let mut aliases_lc = HashMap::new();
        for folder in folders {
            let (alias, alias_lc) = Self::normalize_alias(folder.alias);
            aliases.insert(folder.path.clone(), alias);
            aliases_lc.insert(folder.path, alias_lc);
        }
        Ok((aliases, aliases_lc))
    }

    pub(crate) fn try_bookmark_alias_maps() -> anyhow::Result<(
        HashMap<String, Option<String>>,
        HashMap<String, Option<String>>,
    )> {
        let mut aliases = HashMap::new();
        let mut aliases_lc = HashMap::new();
        for bookmark in
            crate::plugins::bookmarks::load_bookmarks(crate::plugins::bookmarks::BOOKMARKS_FILE)?
        {
            let (alias, alias_lc) = Self::normalize_alias(bookmark.alias);
            aliases.insert(bookmark.url.clone(), alias);
            aliases_lc.insert(bookmark.url, alias_lc);
        }
        Ok((aliases, aliases_lc))
    }

    fn alias_matches_lc(&self, action: &str, query_lc: &str) -> bool {
        self.folder_aliases_lc
            .get(action)
            .or_else(|| self.bookmark_aliases_lc.get(action))
            .and_then(|v| v.as_ref())
            .map(|s| s.contains(query_lc))
            .unwrap_or(false)
    }

    fn is_exact_match_mode(&self) -> bool {
        // `match_exact` is a strict override: if enabled, we always bypass fuzzy scoring.
        self.match_exact || self.fuzzy_weight <= 0.0
    }

    pub(super) fn matches_exact_display_text(cached: &CachedSearchEntry, query_lc: &str) -> bool {
        let query_lc = query_lc.trim();
        if query_lc.is_empty() {
            return true;
        }
        cached.label_lc.contains(query_lc)
    }

    fn should_bypass_exact_post_filter(query: &str, action: &str) -> bool {
        // `query:*` actions are command suggestions that should still participate in
        // exact display-text filtering when users are browsing command names/options.
        if action.starts_with("query:") {
            return false;
        }

        let mut parts = query.split_whitespace();
        let Some(head) = parts.next().map(str::to_ascii_lowercase) else {
            return false;
        };
        if head == "qr" && action == "qr:open" {
            return true;
        }
        let Some(subcommand) = parts.next().map(str::to_ascii_lowercase) else {
            return false;
        };

        // Only bypass launcher-side exact display filtering when the query is an
        // explicit plugin command whose plugin already returned resolved outputs.
        // Example: `note today` / `note search <term>` yielding `note:new:*` or
        // `note:open:*` actions; re-filtering those by label text can hide valid results.
        let note_resolved = matches!(head.as_str(), "note" | "notes")
            && matches!(
                subcommand.as_str(),
                "today"
                    | "search"
                    | "links"
                    | "link"
                    | "list"
                    | "open"
                    | "new"
                    | "add"
                    | "create"
                    | "graph"
                    | "templates"
                    | "tag"
                    | "rm"
            )
            && action.starts_with("note:");

        let clipboard_modify_resolved = head == "cm"
            && !action.starts_with("query:")
            && (action == "clipboard_modify:execute"
                || action == "clipboard_modify:undo"
                || action.starts_with("clipboard_modify:execute:")
                || action.starts_with("clipboard_modify:open:")
                || action.starts_with("clipboard_modify:undo:"));

        // Date expressions resolve to computed values or explanatory errors,
        // neither of which must contain the original expression's words.
        let date_resolved =
            head == "date" && (action.starts_with("clipboard:") || action.starts_with("noop:"));

        note_resolved || clipboard_modify_resolved || date_resolved
    }

    pub(crate) fn has_diagnostics_widget(&self) -> bool {
        self.dashboard
            .slots
            .iter()
            .any(|slot| slot.widget == "diagnostics")
    }

    pub fn update_action_cache(&mut self) {
        let action_cache = {
            #[cfg(test)]
            let _track_c_search_entries = super::track_c_action_observer::scope(
                super::track_c_action_observer::Phase::SearchEntries,
            );
            self.actions
                .iter()
                .map(CachedSearchEntry::from_action)
                .collect::<Vec<_>>()
        };
        #[cfg(test)]
        super::track_c_action_observer::add(
            super::track_c_action_observer::Counter::SearchEntriesProcessed,
            action_cache.len(),
        );
        let action_filter_metadata = {
            #[cfg(test)]
            let _track_c_filter_metadata = super::track_c_action_observer::scope(
                super::track_c_action_observer::Phase::FilterMetadata,
            );
            self.actions
                .iter()
                .map(ActionFilterMetadata::from_action)
                .collect::<Vec<_>>()
        };
        #[cfg(test)]
        super::track_c_action_observer::add(
            super::track_c_action_observer::Counter::FilterEntriesProcessed,
            action_filter_metadata.len(),
        );
        #[cfg(test)]
        let (actions_by_id, id_lookup_build_receipt) = {
            #[cfg(test)]
            let _track_c_id_map =
                super::track_c_action_observer::scope(super::track_c_action_observer::Phase::IdMap);
            crate::actions::ActionIdLookup::new_with_test_receipt(Arc::clone(&self.actions))
        };
        #[cfg(not(test))]
        let actions_by_id = crate::actions::ActionIdLookup::new(Arc::clone(&self.actions));
        #[cfg(test)]
        let id_entries = actions_by_id.len();
        self.action_cache = action_cache;
        self.action_filter_metadata = action_filter_metadata;
        self.actions_by_id = actions_by_id;
        #[cfg(test)]
        {
            super::track_c_action_observer::add(
                super::track_c_action_observer::Counter::IdActionsProcessed,
                id_lookup_build_receipt.key_clone_attempts,
            );
            super::track_c_action_observer::add(
                super::track_c_action_observer::Counter::IdKeysCloned,
                id_lookup_build_receipt.key_clone_attempts,
            );
            super::track_c_action_observer::set(
                super::track_c_action_observer::Counter::CacheEntries,
                self.action_cache.len(),
            );
            super::track_c_action_observer::set(
                super::track_c_action_observer::Counter::FilterEntries,
                self.action_filter_metadata.len(),
            );
            super::track_c_action_observer::set(
                super::track_c_action_observer::Counter::IdEntries,
                id_entries,
            );
        }
        // Search results are derived from the action catalog as well as the
        // query. Keep the cached-query fast path from treating old results as
        // current after a catalog publication.
        {
            #[cfg(test)]
            let _track_c_invalidation = super::track_c_action_observer::scope(
                super::track_c_action_observer::Phase::CompletionInvalidation,
            );
            self.last_results_valid = false;
            self.action_completion_dirty = true;
            self.schedule_completion_rebuild();
        }
    }

    pub fn update_command_cache(&mut self) {
        let mut cmds = self
            .plugins
            .commands_filtered(self.enabled_plugins.as_ref());
        cmds.sort_by_cached_key(|a| a.label.to_lowercase());
        self.command_search_cache = cmds.iter().map(CachedSearchEntry::from_action).collect();
        self.command_cache = cmds;
        self.command_completion_dirty = true;
        self.schedule_completion_rebuild();
    }

    fn schedule_completion_rebuild(&mut self) {
        self.completion_rebuild_after = Some(Instant::now() + COMPLETION_REBUILD_DEBOUNCE);
        self.completion_index = None;
        self.autocomplete_index = 0;
        self.suggestions.clear();
    }

    pub(crate) fn maybe_rebuild_completion_index(&mut self, now: Instant) {
        let should_rebuild = self
            .completion_rebuild_after
            .is_some_and(|scheduled| now >= scheduled)
            && (self.action_completion_dirty || self.command_completion_dirty);
        if should_rebuild {
            self.update_completion_index();
            self.action_completion_dirty = false;
            self.command_completion_dirty = false;
            self.completion_rebuild_after = None;
        }
    }

    pub(crate) fn rebuild_completion_index_now(&mut self) {
        if self.action_completion_dirty || self.command_completion_dirty {
            self.update_completion_index();
            self.action_completion_dirty = false;
            self.command_completion_dirty = false;
        }
        self.completion_rebuild_after = None;
    }

    fn update_completion_index(&mut self) {
        self.completion_index = Some(crate::completion::build_index(
            &self.command_cache,
            self.actions.as_ref(),
        ));
        self.update_suggestions();
    }

    pub(crate) fn update_suggestions(&mut self) {
        self.autocomplete_index = 0;
        self.suggestions.clear();
        if !self.query_autocomplete
            || self.query.is_empty()
            || self.should_show_dashboard(self.query.as_str())
        {
            return;
        }
        if let Some(ref index) = self.completion_index {
            self.suggestions = crate::completion::suggestions(index, &self.query, 5);
        }
    }

    pub(crate) fn is_note_search_query(query: &str) -> bool {
        query.trim_start().to_lowercase().starts_with("note search")
    }

    pub(crate) fn note_search_debounce_ready(
        last_change: Option<Instant>,
        now: Instant,
        debounce: Duration,
    ) -> bool {
        last_change
            .map(|changed_at| now.duration_since(changed_at) >= debounce)
            .unwrap_or(false)
    }

    pub(crate) fn maybe_run_note_search_debounce(&mut self) -> bool {
        if !Self::is_note_search_query(&self.query) {
            self.last_note_search_change = None;
            return false;
        }

        if Self::note_search_debounce_ready(
            self.last_note_search_change,
            Instant::now(),
            NOTE_SEARCH_DEBOUNCE,
        ) {
            self.search();
            self.last_note_search_change = None;
            return true;
        }

        false
    }

    pub(crate) fn clear_selected_after_results_replaced(&mut self) {
        self.selected = None;
    }

    pub fn search(&mut self) {
        self.track_c_query_observer.search_started();
        #[cfg(test)]
        let _track_c_search_total = super::track_c_action_observer::scope(
            super::track_c_action_observer::Phase::SearchTotal,
        );
        #[cfg(test)]
        super::track_c_action_observer::add(
            super::track_c_action_observer::Counter::SameQueryRefreshCalls,
            1,
        );
        // An explicit search satisfies any pending background refresh using
        // the current query, including a newer user intent during OCR.
        self.background_query_refresh_pending = false;
        let perf_enabled = crate::performance::enabled();
        let total_started = crate::performance::started_if(perf_enabled);
        let suppress_deferred_fallback_provider = self
            .radial_suppressed_provider_query
            .take()
            .is_some_and(|query| query == self.query);
        if self.last_results_valid && self.query == self.last_search_query {
            self.clear_selected_after_results_replaced();
            self.record_track_c_query_search_acceptance();
            crate::performance::log_elapsed("search.cached", total_started);
            crate::performance::log_elapsed("search.total", total_started);
            return;
        }

        let normalization_started = crate::performance::started_if(perf_enabled);
        let trimmed = self.query.trim();
        self.last_timer_query =
            trimmed.starts_with("timer list") || trimmed.starts_with("alarm list");
        self.last_stopwatch_query = trimmed.starts_with("sw list");
        if trimmed.is_empty() {
            self.last_search_provider_deferral = ProviderSearchDeferral::None;
            self.autocomplete_index = 0;
            self.suggestions.clear();
            let outcome = self.search_read_only_outcome(&self.query);
            self.last_search_pending = outcome.state == LauncherSearchState::Pending;
            self.last_search_provider_revision = outcome.provider_revision;
            self.last_search_result_catalog_versions = outcome.result_catalog_versions;
            self.last_search_result_catalog_versions_stable =
                outcome.result_catalog_versions_stable;
            self.results = outcome.actions;
            self.invalidate_root_list_results();
            self.clear_selected_after_results_replaced();
            self.recompute_query_results_layout();
            self.record_track_c_query_search_acceptance();
            crate::performance::log_elapsed("search.normalize", normalization_started);
            crate::performance::log_elapsed("search.total", total_started);
            return;
        }
        crate::performance::log_elapsed("search.normalize", normalization_started);
        let outcome = if suppress_deferred_fallback_provider {
            self.search_read_only_outcome_without_providers(&self.query)
        } else {
            self.search_read_only_outcome(&self.query)
        };
        self.last_search_pending = outcome.state == LauncherSearchState::Pending;
        self.last_search_provider_revision = outcome.provider_revision;
        self.last_search_result_catalog_versions = outcome.result_catalog_versions;
        self.last_search_result_catalog_versions_stable = outcome.result_catalog_versions_stable;
        self.last_search_provider_deferral = outcome.provider_deferral;

        #[cfg(test)]
        let c2c_install_observer = c2c_install_observer::current();
        #[cfg(test)]
        let installation_started = c2c_install_observer.as_ref().map(|_| Instant::now());
        let same_raw_query = self.query == self.last_search_query;
        let incoming_results = outcome.actions;
        let can_reuse_results =
            same_raw_query && !self.results.is_empty() && !incoming_results.is_empty();
        #[cfg(test)]
        let incoming_actions = incoming_results.len();
        let results_are_equal = if can_reuse_results {
            #[cfg(test)]
            let comparison_started = c2c_install_observer.as_ref().map(|_| Instant::now());
            let equal = self.results == incoming_results;
            #[cfg(test)]
            if let (Some(observer), Some(started)) =
                (c2c_install_observer.as_ref(), comparison_started)
            {
                observer.add(c2c_install_observer::Counter::ComparisonCalls, 1);
                observer.add(
                    c2c_install_observer::Counter::ComparisonInputResults,
                    incoming_actions as u64,
                );
                observer.add(
                    c2c_install_observer::Counter::ComparisonNanoseconds,
                    started.elapsed().as_nanos().min(u64::MAX as u128) as u64,
                );
            }
            equal
        } else {
            false
        };

        if results_are_equal {
            drop(incoming_results);
            self.queue_popup_cleanup_after_preserved_results();
            #[cfg(test)]
            if let Some(observer) = c2c_install_observer.as_ref() {
                observer.add(c2c_install_observer::Counter::EqualReuses, 1);
                observer.add(
                    c2c_install_observer::Counter::DiscardedIncomingActions,
                    incoming_actions as u64,
                );
            }
        } else {
            self.results = incoming_results;
            self.invalidate_root_list_results();
            #[cfg(test)]
            if let Some(observer) = c2c_install_observer.as_ref() {
                observer.add(c2c_install_observer::Counter::Replacements, 1);
                observer.add(
                    c2c_install_observer::Counter::InstalledActions,
                    incoming_actions as u64,
                );
            }
        }
        #[cfg(test)]
        if let (Some(observer), Some(started)) =
            (c2c_install_observer.as_ref(), installation_started)
        {
            observer.add(
                c2c_install_observer::Counter::InstallationNanoseconds,
                started.elapsed().as_nanos().min(u64::MAX as u128) as u64,
            );
        }
        self.clear_selected_after_results_replaced();
        self.last_search_query = self.query.clone();
        self.last_results_valid = outcome.provider_deferral == ProviderSearchDeferral::None;
        let completion_started = crate::performance::started_if(perf_enabled);
        self.update_suggestions();
        crate::performance::log_elapsed("search.completion", completion_started);
        let layout_started = crate::performance::started_if(perf_enabled);
        self.recompute_query_results_layout();
        crate::performance::log_elapsed("search.layout", layout_started);
        self.record_track_c_query_search_acceptance();
        crate::performance::log_elapsed("search.total", total_started);
    }

    fn record_track_c_query_search_acceptance(&mut self) {
        self.track_c_query_observer.accept_search(
            &self.query,
            self.root_list_geometry.result_generation(),
            self.last_search_provider_revision,
            self.last_search_result_catalog_versions,
            self.last_search_result_catalog_versions_stable,
            self.last_search_pending,
            self.last_search_provider_deferral == ProviderSearchDeferral::Capacity,
        );
    }

    pub(super) fn request_background_query_refresh(&mut self) {
        if self.ocr_defers_launcher_query_refresh() {
            self.background_query_refresh_pending = true;
        } else {
            self.search();
        }
    }

    pub(super) fn flush_background_query_refresh(&mut self) {
        if self.background_query_refresh_pending {
            self.request_background_query_refresh();
        }
    }

    pub(super) fn resume_capacity_deferred_search(&mut self) {
        if std::mem::take(&mut self.last_search_provider_deferral)
            == ProviderSearchDeferral::Capacity
            && !self.last_results_valid
            && self.query == self.last_search_query
        {
            // Refresh the current cache owner; never restore a query captured
            // by the worker that released capacity. Deliberate failed-query
            // fallback has no capacity deferral and stays suppressed.
            self.last_results_valid = false;
            self.last_search_pending = false;
            self.request_background_query_refresh();
        }
    }

    fn search_actions(&self, query: &str, _query_lc: &str) -> Vec<(Action, f32)> {
        let telemetry_enabled = crate::performance::enabled();
        #[cfg(test)]
        let c2a_observer = c2a_search_observer::current();
        #[cfg(test)]
        let c2a_observer_active = c2a_observer.is_some();
        let mut timer = crate::performance::track_c::Timer::start_if(
            crate::performance::track_c::Phase::SearchScoreAndCloneHits,
            telemetry_enabled,
        );
        let (filtered_query, filters) = split_action_filters(query);
        let filtered_query = filtered_query.trim();
        let filtered_query_lc = filtered_query.to_lowercase();
        let query = filtered_query;
        let query_lc = filtered_query_lc.as_str();

        let mut res = Vec::new();
        let mut candidates_scored = 0_usize;
        #[cfg(test)]
        let (mut c2a_catalog_visits, mut c2a_filter_passes, mut c2a_score_attempts) =
            (0_u64, 0_u64, 0_u64);
        #[cfg(test)]
        let (mut c2a_clone_count, mut c2a_clone_bytes, mut c2a_clone_strings) =
            (0_u64, 0_u64, 0_u64);
        if query.is_empty() {
            for (i, a) in self.actions.iter().enumerate() {
                #[cfg(test)]
                if c2a_observer_active {
                    c2a_catalog_visits = c2a_catalog_visits.saturating_add(1);
                }
                if action_matches_filters(&self.action_filter_metadata[i], &filters) {
                    #[cfg(test)]
                    if c2a_observer_active {
                        c2a_filter_passes = c2a_filter_passes.saturating_add(1);
                        c2a_clone_count = c2a_clone_count.saturating_add(1);
                        c2a_clone_bytes = c2a_clone_bytes
                            .saturating_add(c2a_search_observer::action_payload_bytes(a));
                        c2a_clone_strings = c2a_clone_strings
                            .saturating_add(c2a_search_observer::action_payload_strings(a));
                    }
                    let cloned = a.clone();
                    res.push((cloned, 0.0));
                }
            }
        } else {
            for (i, a) in self.actions.iter().enumerate() {
                #[cfg(test)]
                if c2a_observer_active {
                    c2a_catalog_visits = c2a_catalog_visits.saturating_add(1);
                }
                if !action_matches_filters(&self.action_filter_metadata[i], &filters) {
                    continue;
                }

                #[cfg(test)]
                if c2a_observer_active {
                    c2a_filter_passes = c2a_filter_passes.saturating_add(1);
                    c2a_score_attempts = c2a_score_attempts.saturating_add(1);
                }

                if telemetry_enabled {
                    candidates_scored = candidates_scored.saturating_add(1);
                }
                let cached = &self.action_cache[i];
                if self.is_exact_match_mode() {
                    let alias_match = self.alias_matches_lc(&a.action, query_lc);
                    let label_match = Self::matches_exact_display_text(cached, query_lc);
                    // Prefer displayed label text, but keep `desc`/aliases as supplemental
                    // filters for compatibility with existing query behavior.
                    let desc_match = cached.desc_lc.contains(query_lc);
                    let action_match = cached.action_lc.contains(query_lc);
                    if label_match || desc_match || action_match || alias_match {
                        let score = if alias_match { 1.0 } else { 0.0 };
                        #[cfg(test)]
                        if c2a_observer_active {
                            c2a_clone_count = c2a_clone_count.saturating_add(1);
                            c2a_clone_bytes = c2a_clone_bytes
                                .saturating_add(c2a_search_observer::action_payload_bytes(a));
                            c2a_clone_strings = c2a_clone_strings
                                .saturating_add(c2a_search_observer::action_payload_strings(a));
                        }
                        let cloned = a.clone();
                        res.push((cloned, score));
                    }
                } else {
                    let s1 = self.matcher.fuzzy_match(&a.label, query);
                    let s2 = self.matcher.fuzzy_match(&a.desc, query);
                    if let Some(score) = s1.max(s2) {
                        #[cfg(test)]
                        if c2a_observer_active {
                            c2a_clone_count = c2a_clone_count.saturating_add(1);
                            c2a_clone_bytes = c2a_clone_bytes
                                .saturating_add(c2a_search_observer::action_payload_bytes(a));
                            c2a_clone_strings = c2a_clone_strings
                                .saturating_add(c2a_search_observer::action_payload_strings(a));
                        }
                        let cloned = a.clone();
                        res.push((cloned, score as f32 * self.fuzzy_weight));
                    }
                }
            }
        }
        #[cfg(test)]
        if let Some(observer) = c2a_observer {
            observer.add(
                c2a_search_observer::Metric::LocalCatalogVisits,
                c2a_catalog_visits,
            );
            observer.add(
                c2a_search_observer::Metric::LocalFilterPasses,
                c2a_filter_passes,
            );
            observer.add(
                c2a_search_observer::Metric::LocalScoreAttempts,
                c2a_score_attempts,
            );
            observer.add(c2a_search_observer::Metric::LocalHits, res.len() as u64);
            observer.add(
                c2a_search_observer::Metric::LocalActionClones,
                c2a_clone_count,
            );
            observer.add(
                c2a_search_observer::Metric::LocalCloneLogicalBytes,
                c2a_clone_bytes,
            );
            observer.add(
                c2a_search_observer::Metric::LocalCloneStrings,
                c2a_clone_strings,
            );
        }
        timer.set_work_units(self.actions.len());
        timer.set_search_counts(candidates_scored, res.len());
        timer.finish(crate::performance::track_c::Outcome::Completed);
        res
    }

    fn search_plugins_for(
        &self,
        raw_query: &str,
        trimmed: &str,
        trimmed_lc: &str,
    ) -> (Vec<(Action, f32)>, bool) {
        let (plugin_results, tickets) = if trimmed_lc.starts_with("g ") {
            let filter = std::collections::HashSet::from(["web_search".to_string()]);
            self.plugins.search_filtered_with_tickets(
                raw_query,
                Some(&filter),
                self.enabled_capabilities.as_ref(),
            )
        } else {
            self.plugins.search_filtered_with_tickets(
                raw_query,
                self.enabled_plugins.as_ref(),
                self.enabled_capabilities.as_ref(),
            )
        };
        let pending = tickets
            .iter()
            .any(|(source, ticket)| !self.plugins.search_ticket_resolved(source, *ticket));
        self.search_plugins_from_results(raw_query, trimmed, trimmed_lc, plugin_results, pending)
    }

    fn search_plugins_from_results(
        &self,
        raw_query: &str,
        trimmed: &str,
        trimmed_lc: &str,
        plugin_results: Vec<Action>,
        pending: bool,
    ) -> (Vec<(Action, f32)>, bool) {
        #[cfg(test)]
        let c2a_observer = c2a_search_observer::current();
        #[cfg(test)]
        let c2a_provider_input_actions = plugin_results.len() as u64;
        #[cfg(test)]
        let mut c2a_provider_projection_strings = 0_u64;
        #[cfg(test)]
        let mut c2a_provider_projection_bytes = 0_u64;
        #[cfg(test)]
        let mut c2a_command_fallback_actions_visited = 0_u64;
        #[cfg(test)]
        let mut c2a_command_fallback_hits_cloned = 0_u64;
        #[cfg(test)]
        let mut c2a_command_fallback_clone_logical_bytes = 0_u64;
        #[cfg(test)]
        let mut c2a_command_fallback_clone_strings = 0_u64;
        let mut res = Vec::new();
        if trimmed_lc.starts_with("g ") {
            let query_term = trimmed_lc.split_once(' ').map(|x| x.1).unwrap_or("");
            for a in plugin_results {
                let cached = CachedSearchEntry::from_action(&a);
                #[cfg(test)]
                if c2a_observer.is_some() {
                    c2a_provider_projection_strings =
                        c2a_provider_projection_strings.saturating_add(3);
                    c2a_provider_projection_bytes = c2a_provider_projection_bytes
                        .saturating_add(cached.label_lc.len() as u64)
                        .saturating_add(cached.desc_lc.len() as u64)
                        .saturating_add(cached.action_lc.len() as u64);
                }
                if self.is_exact_match_mode() {
                    if Self::should_bypass_exact_post_filter(trimmed, &a.action) {
                        // Plugin commands like `note today`/`note search <term>` already
                        // returned concrete results (e.g. `note:new:*`, `note:open:*`).
                        // Re-filtering by label/desc text can hide valid plugin-resolved
                        // outputs, so keep them as-is in exact mode.
                        res.push((a, 0.0));
                        continue;
                    }
                    if query_term.is_empty() {
                        res.push((a, 0.0));
                    } else {
                        let alias_match = self.alias_matches_lc(&a.action, query_term);
                        let label_match = Self::matches_exact_display_text(&cached, query_term);
                        let desc_match = cached.desc_lc.contains(query_term);
                        let action_match = cached.action_lc.contains(query_term);
                        if label_match || desc_match || action_match || alias_match {
                            let score = if alias_match { 1.0 } else { 0.0 };
                            res.push((a, score));
                        }
                    }
                } else {
                    let score = if raw_query.is_empty() {
                        0.0
                    } else {
                        self.matcher
                            .fuzzy_match(&a.label, raw_query)
                            .max(self.matcher.fuzzy_match(&a.desc, raw_query))
                            .unwrap_or(0) as f32
                            * self.fuzzy_weight
                    };
                    res.push((a, score));
                }
            }
            #[cfg(test)]
            if let Some(observer) = c2a_observer {
                observer.add(
                    c2a_search_observer::Metric::ProviderInputActions,
                    c2a_provider_input_actions,
                );
                observer.add(
                    c2a_search_observer::Metric::ProviderFilterRejections,
                    c2a_provider_input_actions.saturating_sub(res.len() as u64),
                );
                observer.add(
                    c2a_search_observer::Metric::ProviderProjectionStrings,
                    c2a_provider_projection_strings,
                );
                observer.add(
                    c2a_search_observer::Metric::ProviderProjectionBytes,
                    c2a_provider_projection_bytes,
                );
            }
            return (res, pending);
        }

        if plugin_results.is_empty() && !trimmed.is_empty() {
            for (a, cached) in self
                .command_cache
                .iter()
                .zip(self.command_search_cache.iter())
            {
                #[cfg(test)]
                if c2a_observer.is_some() {
                    c2a_command_fallback_actions_visited =
                        c2a_command_fallback_actions_visited.saturating_add(1);
                }
                if self.is_exact_match_mode() {
                    let alias_match = self.alias_matches_lc(&a.action, trimmed_lc);
                    let label_match = Self::matches_exact_display_text(cached, trimmed_lc);
                    let desc_match = cached.desc_lc.contains(trimmed_lc);
                    let action_match = cached.action_lc.contains(trimmed_lc);
                    if label_match || desc_match || action_match || alias_match {
                        let score = if alias_match { 1.0 } else { 0.0 };
                        #[cfg(test)]
                        if c2a_observer.is_some() {
                            c2a_command_fallback_hits_cloned =
                                c2a_command_fallback_hits_cloned.saturating_add(1);
                            c2a_command_fallback_clone_logical_bytes =
                                c2a_command_fallback_clone_logical_bytes
                                    .saturating_add(c2a_search_observer::action_payload_bytes(a));
                            c2a_command_fallback_clone_strings = c2a_command_fallback_clone_strings
                                .saturating_add(c2a_search_observer::action_payload_strings(a));
                        }
                        res.push((a.clone(), score));
                    }
                } else {
                    let s1 = self.matcher.fuzzy_match(&a.label, trimmed);
                    let s2 = self.matcher.fuzzy_match(&a.desc, trimmed);
                    if let Some(score) = s1.max(s2) {
                        #[cfg(test)]
                        if c2a_observer.is_some() {
                            c2a_command_fallback_hits_cloned =
                                c2a_command_fallback_hits_cloned.saturating_add(1);
                            c2a_command_fallback_clone_logical_bytes =
                                c2a_command_fallback_clone_logical_bytes
                                    .saturating_add(c2a_search_observer::action_payload_bytes(a));
                            c2a_command_fallback_clone_strings = c2a_command_fallback_clone_strings
                                .saturating_add(c2a_search_observer::action_payload_strings(a));
                        }
                        res.push((a.clone(), score as f32 * self.fuzzy_weight));
                    }
                }
            }
        } else {
            let tail = trimmed_lc.split_once(" ").map(|x| x.1).unwrap_or("");
            let mut query_term = tail.split(" ").nth(1).unwrap_or("").to_string();
            if query_term.is_empty() {
                let parts: Vec<&str> = tail.split_whitespace().collect();
                if parts.len() == 1 && !SUBCOMMANDS.contains(&parts[0]) {
                    query_term = parts[0].to_string();
                } else if parts.len() > 1 {
                    query_term = parts[1..].join(" ");
                }
            }
            let query_term_lc = query_term.to_lowercase();
            for a in plugin_results {
                let cached = CachedSearchEntry::from_action(&a);
                #[cfg(test)]
                if c2a_observer.is_some() {
                    c2a_provider_projection_strings =
                        c2a_provider_projection_strings.saturating_add(3);
                    c2a_provider_projection_bytes = c2a_provider_projection_bytes
                        .saturating_add(cached.label_lc.len() as u64)
                        .saturating_add(cached.desc_lc.len() as u64)
                        .saturating_add(cached.action_lc.len() as u64);
                }
                if self.is_exact_match_mode() {
                    if Self::should_bypass_exact_post_filter(trimmed, &a.action) {
                        // Explicit plugin commands can resolve into result lists/artifacts.
                        // Preserve those resolved actions in exact mode instead of applying
                        // a second label/description exact filter in the launcher layer.
                        res.push((a, 0.0));
                        continue;
                    }
                    if query_term_lc.is_empty() {
                        res.push((a, 0.0));
                    } else {
                        let alias_match = self.alias_matches_lc(&a.action, &query_term_lc);
                        let label_match = Self::matches_exact_display_text(&cached, &query_term_lc);
                        let desc_match = cached.desc_lc.contains(&query_term_lc);
                        let action_match = cached.action_lc.contains(&query_term_lc);
                        if label_match || desc_match || action_match || alias_match {
                            let score = if alias_match { 1.0 } else { 0.0 };
                            res.push((a, score));
                        }
                    }
                } else {
                    let score = if raw_query.is_empty() {
                        0.0
                    } else {
                        self.matcher
                            .fuzzy_match(&a.label, raw_query)
                            .max(self.matcher.fuzzy_match(&a.desc, raw_query))
                            .unwrap_or(0) as f32
                            * self.fuzzy_weight
                    };
                    res.push((a, score));
                }
            }
        }

        #[cfg(test)]
        if let Some(observer) = c2a_observer {
            observer.add(
                c2a_search_observer::Metric::ProviderInputActions,
                c2a_provider_input_actions,
            );
            let accepted_provider_actions = if c2a_provider_input_actions == 0 {
                0
            } else {
                res.len() as u64
            };
            observer.add(
                c2a_search_observer::Metric::ProviderFilterRejections,
                c2a_provider_input_actions.saturating_sub(accepted_provider_actions),
            );
            observer.add(
                c2a_search_observer::Metric::ProviderProjectionStrings,
                c2a_provider_projection_strings,
            );
            observer.add(
                c2a_search_observer::Metric::ProviderProjectionBytes,
                c2a_provider_projection_bytes,
            );
            observer.add(
                c2a_search_observer::Metric::CommandFallbackActionsVisited,
                c2a_command_fallback_actions_visited,
            );
            observer.add(
                c2a_search_observer::Metric::CommandFallbackHitsCloned,
                c2a_command_fallback_hits_cloned,
            );
            observer.add(
                c2a_search_observer::Metric::CommandFallbackCloneLogicalBytes,
                c2a_command_fallback_clone_logical_bytes,
            );
            observer.add(
                c2a_search_observer::Metric::CommandFallbackCloneStrings,
                c2a_command_fallback_clone_strings,
            );
        }

        (res, pending)
    }

    /// Runs the established ordered launcher/provider search boundary without
    /// mutating GUI state. Search and radial query resolution intentionally use
    /// this same result ordering, cache, alias, filter, plugin, and usage path.
    pub(super) fn search_read_only_outcome(&self, raw_query: &str) -> LauncherSearchOutcome {
        let trimmed = raw_query.trim();
        let trimmed_lc = trimmed.to_lowercase();
        if trimmed.is_empty() {
            let mut results = self.command_cache.clone();
            results.extend(self.actions.iter().map(|action| Action {
                label: format!("app {}", action.label),
                desc: action.desc.clone(),
                action: action.action.clone(),
                args: action.args.clone(),
            }));
            return LauncherSearchOutcome {
                state: LauncherSearchState::Results,
                actions: results,
                provider_revision: self.plugins.search_generation(),
                result_catalog_versions: Some(
                    crate::radial::dynamic::MutableResultCatalogVersions::current(),
                ),
                result_catalog_versions_stable: true,
                provider_deferral: ProviderSearchDeferral::None,
            };
        }
        if self.radial_provider_search_capacity.is_occupied() {
            let revision = self.plugins.search_generation();
            let mut outcome = self.search_read_only_outcome_from_scored_plugins(
                raw_query,
                Vec::new(),
                true,
                revision,
                revision,
                Some(crate::radial::dynamic::MutableResultCatalogVersions::current()),
                false,
            );
            outcome.provider_deferral = ProviderSearchDeferral::Capacity;
            return outcome;
        }
        let catalog_versions_at_start =
            crate::radial::dynamic::MutableResultCatalogVersions::current();
        let start_revision = self.plugins.search_generation();
        let (plugins, pending) = self.search_plugins_for(raw_query, trimmed, &trimmed_lc);
        let provider_revision = self.plugins.search_generation();
        let catalog_versions = crate::radial::dynamic::MutableResultCatalogVersions::current();
        self.search_read_only_outcome_from_scored_plugins(
            raw_query,
            plugins,
            pending,
            start_revision,
            provider_revision,
            Some(catalog_versions),
            catalog_versions_at_start == catalog_versions,
        )
    }

    /// Search only local launcher state when a deferred query has failed and
    /// is being opened for manual editing. This deliberately does not consult
    /// provider capacity: the failed provider may have just returned and
    /// released its slot, but retrying it synchronously would re-enter the
    /// same slow/erroring lookup during fallback.
    fn search_read_only_outcome_without_providers(&self, raw_query: &str) -> LauncherSearchOutcome {
        let trimmed = raw_query.trim();
        let trimmed_lc = trimmed.to_lowercase();
        let revision = self.plugins.search_generation();
        let (plugins, _) =
            self.search_plugins_from_results(raw_query, trimmed, &trimmed_lc, Vec::new(), true);
        self.search_read_only_outcome_from_scored_plugins(
            raw_query,
            plugins,
            true,
            revision,
            revision,
            Some(crate::radial::dynamic::MutableResultCatalogVersions::current()),
            true,
        )
    }

    pub(super) fn search_read_only_outcome_with_plugin_snapshot(
        &self,
        raw_query: &str,
        result: crate::plugin::PluginSearchSnapshotResult,
    ) -> LauncherSearchOutcome {
        let trimmed = raw_query.trim();
        let trimmed_lc = trimmed.to_lowercase();
        let catalog_versions_stable = result.catalog_versions_at_start == result.catalog_versions;
        let catalog_versions = result.catalog_versions;
        let (plugins, pending) = self.search_plugins_from_results(
            raw_query,
            trimmed,
            &trimmed_lc,
            result.actions,
            result.pending,
        );
        self.search_read_only_outcome_from_scored_plugins(
            raw_query,
            plugins,
            pending,
            result.start_revision,
            result.provider_revision,
            Some(catalog_versions),
            catalog_versions_stable,
        )
    }

    fn search_read_only_outcome_from_scored_plugins(
        &self,
        raw_query: &str,
        plugin_results: Vec<(Action, f32)>,
        provider_pending: bool,
        start_revision: u64,
        provider_revision: u64,
        result_catalog_versions: Option<crate::radial::dynamic::MutableResultCatalogVersions>,
        catalog_versions_stable: bool,
    ) -> LauncherSearchOutcome {
        #[cfg(test)]
        let c2a_observer = c2a_search_observer::current();
        let trimmed = raw_query.trim();
        let trimmed_lc = trimmed.to_lowercase();
        if trimmed.is_empty() {
            let mut results = self.command_cache.clone();
            #[cfg(test)]
            let mut c2a_catalog_bytes = 0_u64;
            #[cfg(test)]
            let mut c2a_catalog_strings = 0_u64;
            results.extend(self.actions.iter().map(|action| {
                #[cfg(test)]
                if c2a_observer.is_some() {
                    c2a_catalog_bytes = c2a_catalog_bytes
                        .saturating_add(c2a_search_observer::action_payload_bytes(action))
                        .saturating_add(4); // `app ` prefix in the materialized label.
                    c2a_catalog_strings = c2a_catalog_strings
                        .saturating_add(c2a_search_observer::action_payload_strings(action));
                }
                Action {
                    label: format!("app {}", action.label),
                    desc: action.desc.clone(),
                    action: action.action.clone(),
                    args: action.args.clone(),
                }
            }));
            #[cfg(test)]
            if let Some(observer) = c2a_observer {
                observer.set(c2a_search_observer::Metric::ProviderActionsRetainedMoved, 0);
                observer.set(c2a_search_observer::Metric::SortInputLen, 0);
                observer.set(c2a_search_observer::Metric::FinalScoredEntriesMoved, 0);
                observer.set(c2a_search_observer::Metric::FinalLocalActionClones, 0);
                observer.set(c2a_search_observer::Metric::FinalLocalCloneLogicalBytes, 0);
                observer.set(c2a_search_observer::Metric::FinalLocalCloneStrings, 0);
                observer.set(
                    c2a_search_observer::Metric::EmptyBrowseCommandCacheCloned,
                    self.command_cache.len() as u64,
                );
                observer.set(
                    c2a_search_observer::Metric::EmptyBrowseCatalogMaterialized,
                    self.actions.len() as u64,
                );
                observer.set(
                    c2a_search_observer::Metric::EmptyBrowseLogicalBytes,
                    observer
                        .empty_cache_bytes()
                        .saturating_add(c2a_catalog_bytes),
                );
                observer.set(
                    c2a_search_observer::Metric::EmptyBrowseStrings,
                    observer
                        .empty_cache_strings()
                        .saturating_add(c2a_catalog_strings),
                );
                observer.set(
                    c2a_search_observer::Metric::FinalOutputCount,
                    results.len() as u64,
                );
                observer.set(c2a_search_observer::Metric::OutcomeState, 1);
            }
            return LauncherSearchOutcome {
                state: LauncherSearchState::Results,
                actions: results,
                provider_revision,
                result_catalog_versions,
                result_catalog_versions_stable: catalog_versions_stable,
                provider_deferral: ProviderSearchDeferral::None,
            };
        }
        let search_actions =
            trimmed_lc == APP_PREFIX || trimmed_lc.starts_with(&format!("{APP_PREFIX} "));
        let action_query = if search_actions {
            trimmed.split_once(' ').map(|value| value.1).unwrap_or("")
        } else {
            ""
        };
        let mut scored = Vec::new();
        if !trimmed_lc.starts_with("g ") && search_actions {
            scored.extend(self.search_actions(action_query, &action_query.to_lowercase()));
        }
        #[cfg(test)]
        let c2a_provider_actions_retained_moved = plugin_results.len() as u64;
        scored.extend(plugin_results);
        #[cfg(test)]
        let c2a_sort_input_len = scored.len() as u64;
        self.apply_usage_weight(&mut scored);
        #[cfg(test)]
        let c2a_sort_started = c2a_observer.as_ref().map(|_| std::time::Instant::now());
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        #[cfg(test)]
        if let (Some(observer), Some(started)) = (c2a_observer.as_ref(), c2a_sort_started) {
            observer.add(
                c2a_search_observer::Metric::SortNanoseconds,
                started.elapsed().as_nanos().min(u64::MAX as u128) as u64,
            );
        }
        let mut materialize_timer = crate::performance::track_c::Timer::start(
            crate::performance::track_c::Phase::SearchMoveResults,
        );
        let materialized = scored.len();
        let actions: Vec<Action> = scored.into_iter().map(|(action, _)| action).collect();
        materialize_timer.set_work_units(materialized);
        materialize_timer.finish(crate::performance::track_c::Outcome::Completed);
        let state = if provider_pending
            || start_revision != provider_revision
            || !catalog_versions_stable
        {
            LauncherSearchState::Pending
        } else if actions.is_empty() {
            LauncherSearchState::NoResults
        } else {
            LauncherSearchState::Results
        };
        #[cfg(test)]
        if let Some(observer) = c2a_observer {
            observer.set(
                c2a_search_observer::Metric::ProviderActionsRetainedMoved,
                c2a_provider_actions_retained_moved,
            );
            observer.set(
                c2a_search_observer::Metric::SortInputLen,
                c2a_sort_input_len,
            );
            observer.set(
                c2a_search_observer::Metric::FinalScoredEntriesMoved,
                materialized as u64,
            );
            // Eager scoring already owns every hit Action. The final pass
            // transfers those values into the result vector; it does not clone
            // a catalog Action a second time.
            observer.set(c2a_search_observer::Metric::FinalLocalActionClones, 0);
            observer.set(c2a_search_observer::Metric::FinalLocalCloneLogicalBytes, 0);
            observer.set(c2a_search_observer::Metric::FinalLocalCloneStrings, 0);
            observer.set(
                c2a_search_observer::Metric::FinalOutputCount,
                actions.len() as u64,
            );
            observer.set(
                c2a_search_observer::Metric::OutcomeState,
                match &state {
                    LauncherSearchState::NoResults => 0,
                    LauncherSearchState::Results => 1,
                    LauncherSearchState::Pending => 2,
                },
            );
        }
        LauncherSearchOutcome {
            state,
            actions,
            provider_revision,
            result_catalog_versions,
            result_catalog_versions_stable: catalog_versions_stable,
            provider_deferral: ProviderSearchDeferral::None,
        }
    }

    /// Compatibility helper for existing read-only catalog consumers.
    pub(super) fn search_read_only(&self, raw_query: &str) -> Vec<Action> {
        self.search_read_only_outcome(raw_query).actions
    }

    fn apply_usage_weight(&self, res: &mut Vec<(Action, f32)>) {
        for (result, score) in res.iter_mut() {
            *score +=
                self.usage.get(&result.action).cloned().unwrap_or(0) as f32 * self.usage_weight;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        clipboard_modify::actions::{
            ClipboardModifyActionPayload, ClipboardModifySectionPayload, decode_action_payload,
        },
        plugin::PluginManager,
        plugins::clipboard_modify::ClipboardModifyPlugin,
        settings::Settings,
    };
    use eframe::egui;
    use std::sync::{Arc, atomic::AtomicBool};

    fn new_app(ctx: &egui::Context) -> LauncherApp {
        LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new(),
            "actions.json".into(),
            "settings.json".into(),
            Settings::default(),
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn new_inert_app(ctx: &egui::Context, root: &std::path::Path) -> LauncherApp {
        let mut settings = Settings::default();
        settings.hotkey = None;
        settings.quit_hotkey = None;
        settings.help_hotkey = None;
        settings.dashboard.enabled = false;
        settings.enable_toasts = false;
        LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new_inert_for_test(),
            root.join("actions.json").to_string_lossy().into_owned(),
            root.join("settings.json").to_string_lossy().into_owned(),
            settings,
            None,
            None,
            // The inert manager has no default plugins; allow only the
            // explicitly registered fixture plugins in each test.
            None,
            Some(HashMap::new()),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn c2a_snapshot(
        actions: Vec<Action>,
        pending: bool,
        start_revision: u64,
        provider_revision: u64,
        catalog_versions_at_start: crate::radial::dynamic::MutableResultCatalogVersions,
        catalog_versions: crate::radial::dynamic::MutableResultCatalogVersions,
    ) -> crate::plugin::PluginSearchSnapshotResult {
        crate::plugin::PluginSearchSnapshotResult {
            actions,
            pending,
            start_revision,
            provider_revision,
            catalog_versions_at_start,
            catalog_versions,
        }
    }

    fn c2a_assert_reference_parity(
        app: &LauncherApp,
        query: &str,
        snapshot: crate::plugin::PluginSearchSnapshotResult,
    ) -> LauncherSearchOutcome {
        let expected = track_c2a_eager_search_outcome(app, query, &snapshot);
        let actual = app.search_read_only_outcome_with_plugin_snapshot(query, snapshot);
        assert_eq!(actual, expected, "C2-A semantic fixture: {query:?}");
        actual
    }

    fn new_app_with_clipboard_modify(ctx: &egui::Context, root: &std::path::Path) -> LauncherApp {
        let mut app = new_inert_app(ctx, root);
        let catalog = app.plugins.clipboard_modifier_catalog();
        app.plugins
            .register(Box::new(ClipboardModifyPlugin::new(catalog)));
        app
    }

    fn unified_search_fixture_actions() -> Vec<Action> {
        [
            ("Scope Marker App", "legacy:scope-marker-app"),
            ("Scope Marker File", r"folder:add:C:\Scope Marker"),
            ("Scope Marker Note", "note:open:scope-marker-note"),
            ("Scope Marker Command", "query:regex"),
        ]
        .into_iter()
        .map(|(label, action)| Action {
            label: label.into(),
            desc: String::new(),
            action: action.into(),
            args: None,
        })
        .collect()
    }

    fn new_unified_search_app(ctx: &egui::Context, root: &std::path::Path) -> LauncherApp {
        let mut app = new_inert_app(ctx, root);
        let actions = unified_search_fixture_actions();
        app.custom_len = actions.len();
        app.actions = Arc::new(actions);
        app.update_action_cache();
        app.command_cache.clear();
        app
    }

    #[test]
    fn unified_launcher_and_read_only_snapshot_keep_order() {
        let context = egui::Context::default();
        let workspace = crate::performance::track_c_workloads::TrackCWorkspace::new();
        let mut app = new_unified_search_app(&context, workspace.root());
        app.query = "app scope marker".into();
        app.search();

        let all_actions = app.results.clone();
        assert_eq!(
            all_actions
                .iter()
                .map(|action| action.action.as_str())
                .collect::<Vec<_>>(),
            [
                "legacy:scope-marker-app",
                r"folder:add:C:\Scope Marker",
                "note:open:scope-marker-note",
                "query:regex",
            ]
        );
        assert_eq!(app.search_read_only("app scope marker"), all_actions);

        let provider_action = Action {
            label: "Scope Marker Provider result".into(),
            desc: String::new(),
            action: "provider:captured-result".into(),
            args: None,
        };
        let versions = crate::radial::dynamic::MutableResultCatalogVersions::default();
        let snapshot = c2a_snapshot(
            vec![provider_action.clone()],
            false,
            1,
            1,
            versions,
            versions,
        );
        let snapshot_outcome =
            app.search_read_only_outcome_with_plugin_snapshot("app scope marker", snapshot);
        assert_eq!(
            snapshot_outcome
                .actions
                .iter()
                .take(all_actions.len())
                .cloned()
                .collect::<Vec<_>>(),
            all_actions,
            "captured radial/provider snapshots retain the launcher boundary's local order"
        );
        assert_eq!(
            snapshot_outcome.actions.last(),
            Some(&provider_action),
            "the snapshot provider result remains present in unified launcher search"
        );
        drop(app);
        drop(workspace);
    }

    struct StaticSearchPlugin {
        name: &'static str,
        result: Action,
        always: bool,
        searched: Option<Arc<AtomicBool>>,
    }

    impl crate::plugin::Plugin for StaticSearchPlugin {
        fn search(&self, _query: &str) -> Vec<Action> {
            if let Some(searched) = &self.searched {
                searched.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            vec![self.result.clone()]
        }

        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            self.name
        }

        fn capabilities(&self) -> &[&str] {
            &["search"]
        }

        fn always_search(&self) -> bool {
            self.always
        }
    }

    struct StaticSearchResultsPlugin {
        name: &'static str,
        results: Vec<Action>,
        searched: Option<Arc<AtomicBool>>,
    }

    impl crate::plugin::Plugin for StaticSearchResultsPlugin {
        fn search(&self, _query: &str) -> Vec<Action> {
            if let Some(searched) = &self.searched {
                searched.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            self.results.clone()
        }

        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            self.name
        }

        fn capabilities(&self) -> &[&str] {
            &["search"]
        }

        fn always_search(&self) -> bool {
            true
        }
    }

    fn g_search_plugin(name: &'static str, label: &str) -> StaticSearchPlugin {
        StaticSearchPlugin {
            name,
            result: Action {
                label: label.into(),
                desc: "needle result".into(),
                action: format!("{name}:needle"),
                args: None,
            },
            always: true,
            searched: None,
        }
    }

    #[derive(Clone, Copy, Debug)]
    enum TrackCSearchCase {
        SparseExact,
        DenseExact,
        DenseFuzzy,
        EmptyBrowse,
        PendingExact,
    }

    impl TrackCSearchCase {
        fn query(self) -> &'static str {
            match self {
                Self::SparseExact | Self::PendingExact => "app marker",
                Self::DenseExact => "app",
                Self::DenseFuzzy => "app synthetic",
                Self::EmptyBrowse => "",
            }
        }

        fn name(self) -> &'static str {
            match self {
                Self::SparseExact => "sparse-exact-alias-tie",
                Self::DenseExact => "dense-exact-empty-term",
                Self::DenseFuzzy => "dense-fuzzy-usage-weighted",
                Self::EmptyBrowse => "empty-query-browse",
                Self::PendingExact => "sparse-exact-pending-provider",
            }
        }

        fn id(self) -> u64 {
            match self {
                Self::SparseExact => 0,
                Self::DenseExact => 1,
                Self::DenseFuzzy => 2,
                Self::EmptyBrowse => 3,
                Self::PendingExact => 4,
            }
        }
    }

    struct TrackCSearchState {
        app: LauncherApp,
        scenario: TrackCSearchCase,
        query: String,
        plugin_snapshot: Option<crate::plugin::PluginSearchSnapshotResult>,
        provider_search_was_called: Option<Arc<AtomicBool>>,
        expected: LauncherSearchOutcome,
        expected_local_hits: u64,
        fixture_signature: u64,
    }

    struct TrackC2ASearchState {
        frozen: TrackCSearchState,
        observer: c2a_search_observer::Scope,
        provider_action_ids: HashSet<String>,
        provider_input_actions: u64,
        provider_projection_strings: u64,
        provider_projection_bytes: u64,
    }

    // Companion eager reference for C2-A. It deliberately performs fresh
    // string matching and score assembly from source Actions; only the stable
    // query-filter parser/predicate and fuzzy scorer are shared pure helpers.
    // The original C0 frozen oracle above remains unchanged.
    fn track_c2a_eager_search_outcome(
        app: &LauncherApp,
        raw_query: &str,
        provider: &crate::plugin::PluginSearchSnapshotResult,
    ) -> LauncherSearchOutcome {
        let trimmed = raw_query.trim();
        let trimmed_lc = trimmed.to_lowercase();
        let exact_mode = app.match_exact || app.fuzzy_weight <= 0.0;
        let catalog_versions_stable =
            provider.catalog_versions_at_start == provider.catalog_versions;
        if trimmed.is_empty() {
            let mut actions = app.command_cache.clone();
            actions.extend(app.actions.iter().map(|action| Action {
                label: format!("app {}", action.label),
                desc: action.desc.clone(),
                action: action.action.clone(),
                args: action.args.clone(),
            }));
            return LauncherSearchOutcome {
                state: LauncherSearchState::Results,
                actions,
                provider_revision: provider.provider_revision,
                result_catalog_versions: Some(provider.catalog_versions),
                result_catalog_versions_stable: catalog_versions_stable,
                provider_deferral: ProviderSearchDeferral::None,
            };
        }

        let searches_app =
            trimmed_lc == APP_PREFIX || trimmed_lc.starts_with(&format!("{APP_PREFIX} "));
        let action_query = if searches_app {
            trimmed.split_once(' ').map(|value| value.1).unwrap_or("")
        } else {
            ""
        };
        let (local_text, filters) = split_action_filters(action_query);
        let local_text = local_text.trim();
        let local_text_lc = local_text.to_lowercase();
        let alias_for = |action_id: &str, term: &str| {
            app.folder_aliases_lc
                .get(action_id)
                .or_else(|| app.bookmark_aliases_lc.get(action_id))
                .and_then(Option::as_ref)
                .is_some_and(|alias| alias.contains(term))
        };

        let mut scored = Vec::<(Action, f32)>::new();
        if !trimmed_lc.starts_with("g ") && searches_app {
            for (action, metadata) in app.actions.iter().zip(app.action_filter_metadata.iter()) {
                if !action_matches_filters(metadata, &filters) {
                    continue;
                }
                if local_text_lc.is_empty() {
                    scored.push((action.clone(), 0.0));
                } else if exact_mode {
                    let alias = alias_for(&action.action, &local_text_lc);
                    let label = action.label.to_lowercase().contains(&local_text_lc);
                    let desc = action.desc.to_lowercase().contains(&local_text_lc);
                    let id = action.action.to_lowercase().contains(&local_text_lc);
                    if label || desc || id || alias {
                        scored.push((action.clone(), if alias { 1.0 } else { 0.0 }));
                    }
                } else if let Some(score) = app
                    .matcher
                    .fuzzy_match(&action.label, local_text)
                    .max(app.matcher.fuzzy_match(&action.desc, local_text))
                {
                    scored.push((action.clone(), score as f32 * app.fuzzy_weight));
                }
            }
        }

        if trimmed_lc.starts_with("g ") {
            let query_term = trimmed_lc
                .split_once(' ')
                .map(|value| value.1)
                .unwrap_or("");
            for action in &provider.actions {
                if exact_mode {
                    if LauncherApp::should_bypass_exact_post_filter(trimmed, &action.action) {
                        scored.push((action.clone(), 0.0));
                    } else if query_term.is_empty() {
                        // Empty terms match every provider row at neutral
                        // score; an empty alias must not promote them.
                        scored.push((action.clone(), 0.0));
                    } else if action.label.to_lowercase().contains(query_term.trim())
                        || action.desc.to_lowercase().contains(query_term)
                        || action.action.to_lowercase().contains(query_term)
                        || alias_for(&action.action, query_term)
                    {
                        let alias = alias_for(&action.action, query_term);
                        scored.push((action.clone(), if alias { 1.0 } else { 0.0 }));
                    }
                } else {
                    let score = if raw_query.is_empty() {
                        0.0
                    } else {
                        app.matcher
                            .fuzzy_match(&action.label, raw_query)
                            .max(app.matcher.fuzzy_match(&action.desc, raw_query))
                            .unwrap_or(0) as f32
                            * app.fuzzy_weight
                    };
                    scored.push((action.clone(), score));
                }
            }
        } else if provider.actions.is_empty() {
            for action in &app.command_cache {
                if exact_mode {
                    let alias = alias_for(&action.action, &trimmed_lc);
                    let label = action.label.to_lowercase().contains(&trimmed_lc);
                    let desc = action.desc.to_lowercase().contains(&trimmed_lc);
                    let id = action.action.to_lowercase().contains(&trimmed_lc);
                    if label || desc || id || alias {
                        scored.push((action.clone(), if alias { 1.0 } else { 0.0 }));
                    }
                } else if let Some(score) = app
                    .matcher
                    .fuzzy_match(&action.label, trimmed)
                    .max(app.matcher.fuzzy_match(&action.desc, trimmed))
                {
                    scored.push((action.clone(), score as f32 * app.fuzzy_weight));
                }
            }
        } else {
            let tail = trimmed_lc
                .split_once(' ')
                .map(|value| value.1)
                .unwrap_or("");
            let mut provider_term = tail.split(' ').nth(1).unwrap_or("").to_string();
            if provider_term.is_empty() {
                let parts = tail.split_whitespace().collect::<Vec<_>>();
                if parts.len() == 1 && !SUBCOMMANDS.contains(&parts[0]) {
                    provider_term = parts[0].to_string();
                } else if parts.len() > 1 {
                    provider_term = parts[1..].join(" ");
                }
            }
            let provider_term = provider_term.to_lowercase();
            for action in &provider.actions {
                if exact_mode {
                    if LauncherApp::should_bypass_exact_post_filter(trimmed, &action.action) {
                        scored.push((action.clone(), 0.0));
                    } else if provider_term.is_empty() {
                        scored.push((action.clone(), 0.0));
                    } else if action.label.to_lowercase().contains(provider_term.trim())
                        || action.desc.to_lowercase().contains(&provider_term)
                        || action.action.to_lowercase().contains(&provider_term)
                        || alias_for(&action.action, &provider_term)
                    {
                        let alias = alias_for(&action.action, &provider_term);
                        scored.push((action.clone(), if alias { 1.0 } else { 0.0 }));
                    }
                } else {
                    let score = app
                        .matcher
                        .fuzzy_match(&action.label, raw_query)
                        .max(app.matcher.fuzzy_match(&action.desc, raw_query))
                        .unwrap_or(0) as f32
                        * app.fuzzy_weight;
                    scored.push((action.clone(), score));
                }
            }
        }

        for (action, score) in &mut scored {
            *score += app.usage.get(&action.action).copied().unwrap_or(0) as f32 * app.usage_weight;
        }
        scored.sort_by(|left, right| {
            right
                .1
                .partial_cmp(&left.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let actions: Vec<Action> = scored.into_iter().map(|(action, _)| action).collect();
        let state = if provider.pending
            || provider.start_revision != provider.provider_revision
            || !catalog_versions_stable
        {
            LauncherSearchState::Pending
        } else if actions.is_empty() {
            LauncherSearchState::NoResults
        } else {
            LauncherSearchState::Results
        };
        LauncherSearchOutcome {
            state,
            actions,
            provider_revision: provider.provider_revision,
            result_catalog_versions: Some(provider.catalog_versions),
            result_catalog_versions_stable: catalog_versions_stable,
            provider_deferral: ProviderSearchDeferral::None,
        }
    }

    fn track_c2a_prepare_search_state(
        root: &std::path::Path,
        actions: &[Action],
        scenario: TrackCSearchCase,
    ) -> TrackC2ASearchState {
        let mut frozen = track_c_prepare_search_state(root, actions, scenario);
        let captured_provider = frozen
            .plugin_snapshot
            .as_ref()
            .expect("fixed provider snapshot is captured during setup");
        let companion_expected =
            track_c2a_eager_search_outcome(&frozen.app, &frozen.query, captured_provider);
        assert_eq!(
            companion_expected, frozen.expected,
            "C2-A companion eager oracle agrees with the frozen C0 reference"
        );
        frozen.expected = companion_expected;
        let provider_actions = &captured_provider.actions;
        let provider_action_ids = provider_actions
            .iter()
            .map(|action| action.action.clone())
            .collect();
        let provider_input_actions = provider_actions.len() as u64;
        let (provider_projection_strings, provider_projection_bytes) = provider_actions
            .iter()
            .fold((0_u64, 0_u64), |(strings, bytes), action| {
                let projected = CachedSearchEntry::from_action(action);
                (
                    strings.saturating_add(3),
                    bytes
                        .saturating_add(projected.label_lc.len() as u64)
                        .saturating_add(projected.desc_lc.len() as u64)
                        .saturating_add(projected.action_lc.len() as u64),
                )
            });
        let observer = c2a_search_observer::Scope::new(&frozen.app.command_cache);
        TrackC2ASearchState {
            frozen,
            observer,
            provider_action_ids,
            provider_input_actions,
            provider_projection_strings,
            provider_projection_bytes,
        }
    }

    fn track_c2b_plugin_heavy_provider_actions() -> Vec<Action> {
        (0..128)
            .map(|index| {
                let args = match index % 3 {
                    0 => None,
                    1 => Some(String::new()),
                    _ => Some(format!("--provider-index {index:03}")),
                };
                Action {
                    label: format!("Synthetic provider heavy result {index:03}"),
                    desc: format!(
                        "Captured provider payload row {index:03} for launcher query app"
                    ),
                    action: format!("track_c_heavy_provider:{index:03}"),
                    args,
                }
            })
            .collect()
    }

    fn track_c2b_prepare_plugin_heavy_state(
        root: &std::path::Path,
        actions: &[Action],
    ) -> TrackC2ASearchState {
        let provider_actions = track_c2b_plugin_heavy_provider_actions();
        let provider_searched = Arc::new(AtomicBool::new(false));
        let ctx = egui::Context::default();
        let mut app = track_c_search_app_with_provider_results(
            &ctx,
            root,
            actions.to_vec(),
            provider_actions.clone(),
            Some(Arc::clone(&provider_searched)),
        );

        // This case is intentionally independent of whatever aliases or
        // command state a future app-constructor fixture may add.
        app.folder_aliases.clear();
        app.folder_aliases_lc.clear();
        app.bookmark_aliases.clear();
        app.bookmark_aliases_lc.clear();
        app.command_cache.clear();
        app.command_search_cache.clear();
        app.usage.clear();
        for index in [1, 7, 13] {
            if let Some(action) = actions.get(index) {
                app.usage.insert(action.action.clone(), 8);
            }
            if let Some(action) = provider_actions.get(index) {
                app.usage.insert(action.action.clone(), 8);
            }
        }
        app.fuzzy_weight = 0.25;
        app.match_exact = true;
        app.usage_weight = 0.25;

        // LauncherApp construction may have performed an initial search. The
        // measured owner must consume only the immutable snapshot captured
        // below, never call the provider itself.
        provider_searched.store(false, std::sync::atomic::Ordering::SeqCst);
        let query = "app";
        let captured = app
            .plugins
            .search_snapshot(
                app.enabled_plugins.as_ref(),
                app.enabled_capabilities.as_ref(),
            )
            .search(query);
        assert_eq!(captured.actions, provider_actions);
        assert!(provider_searched.load(std::sync::atomic::Ordering::SeqCst));
        provider_searched.store(false, std::sync::atomic::Ordering::SeqCst);

        let frozen_expected = track_c_eager_search_outcome(&app, query, &captured);
        let companion_expected = track_c2a_eager_search_outcome(&app, query, &captured);
        assert_eq!(
            frozen_expected, companion_expected,
            "plugin-heavy C2-B setup agrees with both unchanged eager references"
        );
        assert_eq!(
            frozen_expected.actions.len(),
            actions.len() + provider_actions.len()
        );
        let provider_action_ids = captured
            .actions
            .iter()
            .map(|action| action.action.clone())
            .collect::<HashSet<_>>();
        let expected_local_hits = frozen_expected
            .actions
            .iter()
            .filter(|action| !provider_action_ids.contains(&action.action))
            .count() as u64;
        let fixture_signature = track_c_search_fixture_signature(
            &app,
            query,
            TrackCSearchCase::DenseExact,
            &captured.actions,
        );
        let provider_input_actions = captured.actions.len() as u64;
        let (provider_projection_strings, provider_projection_bytes) = captured
            .actions
            .iter()
            .fold((0_u64, 0_u64), |(strings, bytes), action| {
                let projected = CachedSearchEntry::from_action(action);
                (
                    strings.saturating_add(3),
                    bytes
                        .saturating_add(projected.label_lc.len() as u64)
                        .saturating_add(projected.desc_lc.len() as u64)
                        .saturating_add(projected.action_lc.len() as u64),
                )
            });
        let observer = c2a_search_observer::Scope::new(&app.command_cache);
        crate::performance::track_c::reset();
        let frozen = TrackCSearchState {
            app,
            scenario: TrackCSearchCase::DenseExact,
            query: query.to_string(),
            plugin_snapshot: Some(captured),
            provider_search_was_called: Some(provider_searched),
            expected: frozen_expected,
            expected_local_hits,
            fixture_signature,
        };
        TrackC2ASearchState {
            frozen,
            observer,
            provider_action_ids,
            provider_input_actions,
            provider_projection_strings,
            provider_projection_bytes,
        }
    }

    fn track_c2b_validate_search_state(
        state: &mut TrackC2ASearchState,
        actual: &LauncherSearchOutcome,
        require_metrics: bool,
    ) -> crate::performance::track_c_workloads::OwnerObservation {
        let mut observation = track_c2a_validate_search_state(state, actual, require_metrics);
        if let Some(provider_searched) = &state.frozen.provider_search_was_called {
            assert!(
                !provider_searched.load(std::sync::atomic::Ordering::SeqCst),
                "the captured provider was not invoked from the shared search owner"
            );
        }
        let receipt = state.observer.receipt();
        observation.work_counters = receipt.materialization_arrays().to_vec();
        assert_eq!(observation.work_counters.len(), 31);
        observation
    }

    fn track_c2a_run_search_owner(state: &mut TrackC2ASearchState) -> LauncherSearchOutcome {
        let activation = state.observer.activate();
        let outcome = state
            .frozen
            .app
            .search_read_only_outcome_with_plugin_snapshot(
                &state.frozen.query,
                state
                    .frozen
                    .plugin_snapshot
                    .take()
                    .expect("captured provider snapshot is consumed once"),
            );
        drop(activation);
        outcome
    }

    fn track_c2a_validate_receipt(
        state: &TrackC2ASearchState,
        actual: &LauncherSearchOutcome,
        receipt: c2a_search_observer::Receipt,
    ) {
        let frozen = &state.frozen;
        let is_empty_browse = matches!(frozen.scenario, TrackCSearchCase::EmptyBrowse);
        let expected_local_actions = if is_empty_browse {
            Vec::new()
        } else {
            frozen
                .expected
                .actions
                .iter()
                .filter(|action| !state.provider_action_ids.contains(&action.action))
                .collect::<Vec<_>>()
        };
        let expected_local_bytes = expected_local_actions.iter().fold(0_u64, |sum, action| {
            sum.saturating_add(c2a_search_observer::action_payload_bytes(action))
        });
        let expected_local_strings = expected_local_actions.iter().fold(0_u64, |sum, action| {
            sum.saturating_add(c2a_search_observer::action_payload_strings(action))
        });
        let expected_provider_retained = frozen
            .expected
            .actions
            .iter()
            .filter(|action| state.provider_action_ids.contains(&action.action))
            .count() as u64;
        let expected_empty_bytes = frozen.expected.actions.iter().fold(0_u64, |sum, action| {
            sum.saturating_add(c2a_search_observer::action_payload_bytes(action))
        });
        let expected_empty_strings = frozen.expected.actions.iter().fold(0_u64, |sum, action| {
            sum.saturating_add(c2a_search_observer::action_payload_strings(action))
        });
        let expected_outcome_state = match frozen.expected.state {
            LauncherSearchState::NoResults => 0,
            LauncherSearchState::Results => 1,
            LauncherSearchState::Pending => 2,
        };

        assert_eq!(
            receipt.local_catalog_visits,
            if is_empty_browse {
                0
            } else {
                frozen.app.actions.len() as u64
            }
        );
        assert_eq!(
            receipt.local_filter_passes,
            if is_empty_browse {
                0
            } else {
                frozen.app.actions.len() as u64
            }
        );
        assert_eq!(
            receipt.local_score_attempts,
            if matches!(
                frozen.scenario,
                TrackCSearchCase::DenseExact | TrackCSearchCase::EmptyBrowse
            ) {
                0
            } else {
                frozen.app.actions.len() as u64
            }
        );
        assert_eq!(receipt.local_hits, expected_local_actions.len() as u64);
        assert_eq!(
            receipt.local_action_clones,
            expected_local_actions.len() as u64
        );
        assert_eq!(receipt.local_clone_logical_bytes, expected_local_bytes);
        assert_eq!(receipt.local_clone_strings, expected_local_strings);
        assert_eq!(receipt.final_local_action_clones, 0);
        assert_eq!(receipt.final_local_clone_logical_bytes, 0);
        assert_eq!(receipt.final_local_clone_strings, 0);
        assert_eq!(receipt.provider_input_actions, state.provider_input_actions);
        assert_eq!(receipt.provider_filter_rejections, 0);
        assert_eq!(
            receipt.provider_actions_retained_moved,
            expected_provider_retained
        );
        assert_eq!(
            receipt.provider_projection_strings,
            state.provider_projection_strings
        );
        assert_eq!(
            receipt.provider_projection_bytes,
            state.provider_projection_bytes
        );
        assert_eq!(receipt.command_fallback_actions_visited, 0);
        assert_eq!(receipt.command_fallback_hits_cloned, 0);
        assert_eq!(receipt.command_fallback_clone_logical_bytes, 0);
        assert_eq!(receipt.command_fallback_clone_strings, 0);
        assert_eq!(
            receipt.sort_input_len,
            if is_empty_browse {
                0
            } else {
                frozen.expected.actions.len() as u64
            }
        );
        assert_eq!(
            receipt.final_scored_entries_moved,
            if is_empty_browse {
                0
            } else {
                frozen.expected.actions.len() as u64
            }
        );
        assert_eq!(
            receipt.empty_browse_command_cache_cloned,
            if is_empty_browse {
                frozen.app.command_cache.len() as u64
            } else {
                0
            }
        );
        assert_eq!(
            receipt.empty_browse_catalog_materialized,
            if is_empty_browse {
                frozen.app.actions.len() as u64
            } else {
                0
            }
        );
        assert_eq!(
            receipt.empty_browse_logical_bytes,
            if is_empty_browse {
                expected_empty_bytes
            } else {
                0
            }
        );
        assert_eq!(
            receipt.empty_browse_strings,
            if is_empty_browse {
                expected_empty_strings
            } else {
                0
            }
        );
        assert_eq!(receipt.final_output_count, actual.actions.len() as u64);
        assert_eq!(receipt.outcome_state, expected_outcome_state);
    }

    fn track_c2a_validate_search_state(
        state: &mut TrackC2ASearchState,
        actual: &LauncherSearchOutcome,
        require_metrics: bool,
    ) -> crate::performance::track_c_workloads::OwnerObservation {
        let observation = track_c_validate_search_state(&state.frozen, actual, require_metrics);
        let receipt = state.observer.receipt();
        track_c2a_validate_receipt(state, actual, receipt);
        crate::performance::track_c_workloads::OwnerObservation {
            work_counters: receipt.arrays().to_vec(),
            ..observation
        }
    }

    fn emit_track_c2a_samples(
        scenario: TrackCSearchCase,
        count: usize,
        fixture_signature: u64,
        mode: crate::performance::track_c_workloads::BenchmarkMode,
        samples: &[crate::performance::track_c_workloads::SampleRecord],
    ) {
        use crate::performance::track_c_workloads::{MEASURED_SAMPLES, WARMUPS};

        assert_eq!(samples.len(), MEASURED_SAMPLES);
        assert!(
            samples
                .iter()
                .all(|sample| sample.work_counters.len() == 26)
        );
        let source = std::env::var("ML_TRACK_C_SOURCE_SHA")
            .unwrap_or_else(|_| "UNCOMMITTED_SOURCE".to_string());
        let profile = std::env::var("ML_TRACK_C_PROFILE")
            .unwrap_or_else(|_| "unspecified-profile".to_string());
        for value in [&source, &profile] {
            assert!(!value.bytes().any(|byte| byte.is_ascii_whitespace()));
        }
        let mode = match mode {
            crate::performance::track_c_workloads::BenchmarkMode::Small => "small",
            crate::performance::track_c_workloads::BenchmarkMode::Full => "full",
        };
        let array = |index: usize| {
            samples
                .iter()
                .map(|sample| sample.work_counters[index].to_string())
                .collect::<Vec<_>>()
                .join(",")
        };
        let elapsed = samples
            .iter()
            .map(|sample| sample.elapsed_nanos.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let complete_output_ids = samples
            .iter()
            .map(|sample| format!("{:016x}", sample.output_identity))
            .collect::<Vec<_>>()
            .join(",");
        let structural_output_ids = samples
            .iter()
            .map(|sample| format!("{:016x}", sample.structural_signature))
            .collect::<Vec<_>>()
            .join(",");

        eprintln!(
            "C2A_SEARCH_RESULT schema=1 stage=eager_rank_attribution owner=search_read_only_outcome_with_plugin_snapshot fixture=synthetic_action_catalog_captured_provider source={} profile={} mode={} telemetry_enabled=true observer_enabled=true warmups={} samples={} exclusions=0 provider_scope=captured_snapshot_no_provider_invocation materialization_scope=local_hit_cloned_once_then_scored_entries_moved empty_browse_scope=command_cache_and_catalog_actions_materialized_provider_input_discarded payload_scope=logical_utf8_and_string_values_not_allocator_or_rss outcome_scope=noresults_0_results_1_pending_2 case={} count={} fixture_signature={:016x} elapsed_ns=[{}] local_catalog_visits=[{}] local_filter_passes=[{}] local_score_attempts=[{}] local_hits=[{}] local_action_clones=[{}] local_clone_logical_bytes=[{}] local_clone_strings=[{}] provider_input_actions=[{}] provider_filter_rejections=[{}] provider_actions_retained_moved=[{}] provider_projection_strings=[{}] provider_projection_bytes=[{}] command_fallback_actions_visited=[{}] command_fallback_hits_cloned=[{}] command_fallback_clone_logical_bytes=[{}] command_fallback_clone_strings=[{}] sort_input_len=[{}] sort_ns=[{}] final_scored_entries_moved=[{}] final_local_action_clones=[{}] empty_browse_command_cache_cloned=[{}] empty_browse_catalog_materialized=[{}] empty_browse_logical_bytes=[{}] empty_browse_strings=[{}] final_output_count=[{}] outcome_state=[{}] complete_output_ids=[{}] structural_output_ids=[{}]",
            source,
            profile,
            mode,
            WARMUPS,
            samples.len(),
            scenario.name(),
            count,
            fixture_signature,
            elapsed,
            array(0),
            array(1),
            array(2),
            array(3),
            array(4),
            array(5),
            array(6),
            array(7),
            array(8),
            array(9),
            array(10),
            array(11),
            array(12),
            array(13),
            array(14),
            array(15),
            array(16),
            array(17),
            array(18),
            array(19),
            array(20),
            array(21),
            array(22),
            array(23),
            array(24),
            array(25),
            complete_output_ids,
            structural_output_ids,
        );
    }

    fn emit_track_c2b_samples(
        case: &str,
        count: usize,
        provider_count: usize,
        fixture_signature: u64,
        mode: crate::performance::track_c_workloads::BenchmarkMode,
        samples: &[crate::performance::track_c_workloads::SampleRecord],
    ) {
        use crate::performance::track_c_workloads::{MEASURED_SAMPLES, WARMUPS};

        assert_eq!(samples.len(), MEASURED_SAMPLES);
        assert!(
            samples
                .iter()
                .all(|sample| sample.work_counters.len() == 31)
        );
        let source = std::env::var("ML_TRACK_C_SOURCE_SHA")
            .unwrap_or_else(|_| "UNCOMMITTED_SOURCE".to_string());
        let profile = std::env::var("ML_TRACK_C_PROFILE")
            .unwrap_or_else(|_| "unspecified-profile".to_string());
        for value in [&source, &profile, case] {
            assert!(!value.bytes().any(|byte| byte.is_ascii_whitespace()));
        }
        let mode = match mode {
            crate::performance::track_c_workloads::BenchmarkMode::Small => "small",
            crate::performance::track_c_workloads::BenchmarkMode::Full => "full",
        };
        let array = |index: usize| {
            samples
                .iter()
                .map(|sample| sample.work_counters[index].to_string())
                .collect::<Vec<_>>()
                .join(",")
        };
        let elapsed = samples
            .iter()
            .map(|sample| sample.elapsed_nanos.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let complete_output_ids = samples
            .iter()
            .map(|sample| format!("{:016x}", sample.output_identity))
            .collect::<Vec<_>>()
            .join(",");
        let structural_output_ids = samples
            .iter()
            .map(|sample| format!("{:016x}", sample.structural_signature))
            .collect::<Vec<_>>()
            .join(",");

        eprintln!(
            "C2B_SEARCH_RESULT schema=1 stage=search_materialization materialization=eager owner=search_read_only_outcome_with_plugin_snapshot fixture=synthetic_action_catalog_captured_provider source={} profile={} mode={} telemetry_enabled=true observer_enabled=true warmups={} samples={} exclusions=0 provider_scope=captured_snapshot_no_provider_invocation materialization_scope=catalog_hit_copies_once_staged_provider_owned_moves_browse_separate empty_browse_scope=command_cache_and_catalog_actions_materialized_provider_input_discarded payload_scope=logical_utf8_and_string_values_not_allocator_or_rss outcome_scope=noresults_0_results_1_pending_2 case={} count={} provider_count={} rank_record_bytes={} fixture_signature={:016x} elapsed_ns=[{}] local_catalog_visits=[{}] local_filter_passes=[{}] local_score_attempts=[{}] local_hits=[{}] local_action_clones=[{}] local_clone_logical_bytes=[{}] local_clone_strings=[{}] provider_input_actions=[{}] provider_filter_rejections=[{}] provider_actions_retained_moved=[{}] provider_projection_strings=[{}] provider_projection_bytes=[{}] command_fallback_actions_visited=[{}] command_fallback_hits_cloned=[{}] command_fallback_clone_logical_bytes=[{}] command_fallback_clone_strings=[{}] sort_input_len=[{}] sort_ns=[{}] final_scored_entries_moved=[{}] final_local_action_clones=[{}] empty_browse_command_cache_cloned=[{}] empty_browse_catalog_materialized=[{}] empty_browse_logical_bytes=[{}] empty_browse_strings=[{}] final_output_count=[{}] outcome_state=[{}] final_local_clone_logical_bytes=[{}] final_local_clone_strings=[{}] hit_action_clone_total=[{}] hit_clone_logical_bytes_total=[{}] hit_clone_strings_total=[{}] complete_output_ids=[{}] structural_output_ids=[{}]",
            source,
            profile,
            mode,
            WARMUPS,
            samples.len(),
            case,
            count,
            provider_count,
            std::mem::size_of::<(Action, f32)>(),
            fixture_signature,
            elapsed,
            array(0),
            array(1),
            array(2),
            array(3),
            array(4),
            array(5),
            array(6),
            array(7),
            array(8),
            array(9),
            array(10),
            array(11),
            array(12),
            array(13),
            array(14),
            array(15),
            array(16),
            array(17),
            array(18),
            array(19),
            array(20),
            array(21),
            array(22),
            array(23),
            array(24),
            array(25),
            array(26),
            array(27),
            array(28),
            array(29),
            array(30),
            complete_output_ids,
            structural_output_ids,
        );
    }

    fn track_c_search_app(
        ctx: &egui::Context,
        root: &std::path::Path,
        actions: Vec<Action>,
        provider_result: Action,
        searched: Option<Arc<AtomicBool>>,
    ) -> LauncherApp {
        let mut settings = Settings::default();
        settings.hotkey = None;
        settings.quit_hotkey = None;
        settings.help_hotkey = None;
        settings.dashboard.enabled = false;
        settings.enable_toasts = false;
        let mut plugins = PluginManager::new_inert_for_test();
        plugins.register(Box::new(StaticSearchPlugin {
            name: "track_c_fixture_provider",
            result: provider_result,
            always: true,
            searched,
        }));
        let mut app = LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            plugins,
            root.join("actions.json").to_string_lossy().into_owned(),
            root.join("settings.json").to_string_lossy().into_owned(),
            settings,
            None,
            None,
            Some(HashSet::from(["track_c_fixture_provider".into()])),
            Some(HashMap::new()),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        app.actions = Arc::new(actions);
        app.update_action_cache();
        app.match_exact = true;
        app.usage_weight = 0.25;
        app.usage.clear();
        app
    }

    fn track_c_search_app_with_provider_results(
        ctx: &egui::Context,
        root: &std::path::Path,
        actions: Vec<Action>,
        provider_results: Vec<Action>,
        searched: Option<Arc<AtomicBool>>,
    ) -> LauncherApp {
        let mut settings = Settings::default();
        settings.hotkey = None;
        settings.quit_hotkey = None;
        settings.help_hotkey = None;
        settings.dashboard.enabled = false;
        settings.enable_toasts = false;
        let mut plugins = PluginManager::new_inert_for_test();
        plugins.register(Box::new(StaticSearchResultsPlugin {
            name: "track_c_fixture_provider",
            results: provider_results,
            searched,
        }));
        let mut app = LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            plugins,
            root.join("actions.json").to_string_lossy().into_owned(),
            root.join("settings.json").to_string_lossy().into_owned(),
            settings,
            None,
            None,
            Some(HashSet::from(["track_c_fixture_provider".into()])),
            Some(HashMap::new()),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        app.actions = Arc::new(actions);
        app.update_action_cache();
        app.match_exact = true;
        app.usage_weight = 0.25;
        app.usage.clear();
        app
    }

    // Frozen eager reference for the small Track C search scenario set. It
    // deliberately spells out the original exact/fuzzy selection, provider
    // filtering, usage weighting, stable score sort, and result assembly.
    fn track_c_eager_search_outcome(
        app: &LauncherApp,
        raw_query: &str,
        provider: &crate::plugin::PluginSearchSnapshotResult,
    ) -> LauncherSearchOutcome {
        let trimmed = raw_query.trim();
        let trimmed_lc = trimmed.to_lowercase();
        // Keep the frozen test reference independent of LauncherApp's helper
        // while retaining its current exact-mode predicate.
        let exact_mode = app.match_exact || app.fuzzy_weight <= 0.0;
        if trimmed.is_empty() {
            let mut actions = app.command_cache.clone();
            actions.extend(app.actions.iter().map(|action| Action {
                label: format!("app {}", action.label),
                desc: action.desc.clone(),
                action: action.action.clone(),
                args: action.args.clone(),
            }));
            return LauncherSearchOutcome {
                state: LauncherSearchState::Results,
                actions,
                provider_revision: provider.provider_revision,
                result_catalog_versions: Some(provider.catalog_versions),
                result_catalog_versions_stable: provider.catalog_versions_at_start
                    == provider.catalog_versions,
                provider_deferral: ProviderSearchDeferral::None,
            };
        }

        let searches_app = trimmed_lc == "app" || trimmed_lc.starts_with("app ");
        let action_query = if searches_app {
            trimmed.split_once(' ').map(|(_, tail)| tail).unwrap_or("")
        } else {
            ""
        };
        let query_term = action_query.trim().to_lowercase();
        let alias_matches = |action_id: &str| {
            app.folder_aliases_lc
                .get(action_id)
                .or_else(|| app.bookmark_aliases_lc.get(action_id))
                .and_then(Option::as_ref)
                .is_some_and(|alias| alias.contains(&query_term))
        };
        let mut scored = Vec::<(Action, f32)>::new();
        if searches_app {
            for action in app.actions.iter() {
                // Production's app-action path treats an empty app term as a
                // score-zero catalog browse before alias matching.
                if query_term.is_empty() {
                    scored.push((action.clone(), 0.0));
                } else if exact_mode {
                    let alias = alias_matches(&action.action);
                    let label = action.label.to_lowercase().contains(&query_term);
                    let desc = action.desc.to_lowercase().contains(&query_term);
                    let id = action.action.to_lowercase().contains(&query_term);
                    if alias || label || desc || id {
                        scored.push((action.clone(), if alias { 1.0 } else { 0.0 }));
                    }
                } else if let Some(score) = app
                    .matcher
                    .fuzzy_match(&action.label, action_query)
                    .max(app.matcher.fuzzy_match(&action.desc, action_query))
                {
                    scored.push((action.clone(), score as f32 * app.fuzzy_weight));
                }
            }
        }
        for action in &provider.actions {
            if exact_mode {
                let alias = alias_matches(&action.action);
                let label = action.label.to_lowercase().contains(&query_term);
                let desc = action.desc.to_lowercase().contains(&query_term);
                let id = action.action.to_lowercase().contains(&query_term);
                if query_term.is_empty() || alias || label || desc || id {
                    scored.push((action.clone(), if alias { 1.0 } else { 0.0 }));
                }
            } else {
                let score = app
                    .matcher
                    .fuzzy_match(&action.label, raw_query)
                    .max(app.matcher.fuzzy_match(&action.desc, raw_query))
                    .unwrap_or(0) as f32
                    * app.fuzzy_weight;
                scored.push((action.clone(), score));
            }
        }
        for (action, score) in &mut scored {
            *score += app.usage.get(&action.action).copied().unwrap_or(0) as f32 * app.usage_weight;
        }
        scored.sort_by(|left, right| {
            right
                .1
                .partial_cmp(&left.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let actions: Vec<Action> = scored.into_iter().map(|(action, _)| action).collect();
        let stable = provider.catalog_versions_at_start == provider.catalog_versions;
        let state =
            if provider.pending || provider.start_revision != provider.provider_revision || !stable
            {
                LauncherSearchState::Pending
            } else if actions.is_empty() {
                LauncherSearchState::NoResults
            } else {
                LauncherSearchState::Results
            };
        LauncherSearchOutcome {
            state,
            actions,
            provider_revision: provider.provider_revision,
            result_catalog_versions: Some(provider.catalog_versions),
            result_catalog_versions_stable: stable,
            provider_deferral: ProviderSearchDeferral::None,
        }
    }

    fn track_c_search_state_signature(outcome: &LauncherSearchOutcome) -> u64 {
        let mut signature = crate::performance::track_c_workloads::StableSignature::new(
            0x435F_5345_4152_4348,
            "track-c-search-complete-output",
            outcome.actions.len(),
        );
        signature.number(
            crate::performance::track_c_workloads::action_fixture_identity(&outcome.actions),
        );
        signature.number(match outcome.state {
            LauncherSearchState::Results => 0,
            LauncherSearchState::NoResults => 1,
            LauncherSearchState::Pending => 2,
        });
        signature.number(outcome.provider_revision);
        if let Some(versions) = outcome.result_catalog_versions {
            signature.number(1);
            signature.number(versions.clipboard);
            signature.number(versions.todo);
            signature.number(versions.notes);
        } else {
            signature.number(0);
        }
        signature.number(outcome.result_catalog_versions_stable as u64);
        signature.number(match outcome.provider_deferral {
            ProviderSearchDeferral::None => 0,
            ProviderSearchDeferral::Capacity => 1,
        });
        signature.finish()
    }

    fn track_c_search_fixture_signature(
        app: &LauncherApp,
        query: &str,
        scenario: TrackCSearchCase,
        provider_actions: &[Action],
    ) -> u64 {
        use crate::performance::track_c_workloads::{StableSignature, action_fixture_identity};

        let mut signature = StableSignature::new(
            0x435F_5345_4152_4348,
            "track-c-search-input-state",
            app.actions.len().saturating_add(provider_actions.len()),
        );
        let append_string = |signature: &mut StableSignature, value: &str| {
            signature.number(value.len() as u64);
            signature.bytes(value.as_bytes());
        };
        signature.number(scenario.id());
        append_string(&mut signature, query);
        signature.number(action_fixture_identity(&app.actions));
        signature.number(action_fixture_identity(provider_actions));
        signature.number(action_fixture_identity(&app.command_cache));
        signature.number(app.match_exact as u64);
        signature.number(u64::from(app.fuzzy_weight.to_bits()));
        signature.number(u64::from(app.usage_weight.to_bits()));
        for aliases in [&app.folder_aliases_lc, &app.bookmark_aliases_lc] {
            let mut entries = aliases.iter().collect::<Vec<_>>();
            entries.sort_unstable_by(|left, right| left.0.cmp(right.0));
            signature.number(entries.len() as u64);
            for (id, alias) in entries {
                append_string(&mut signature, id);
                match alias {
                    Some(alias) => {
                        signature.number(1);
                        append_string(&mut signature, alias);
                    }
                    None => signature.number(0),
                }
            }
        }
        let mut usage = app.usage.iter().collect::<Vec<_>>();
        usage.sort_unstable_by(|left, right| left.0.cmp(right.0));
        signature.number(usage.len() as u64);
        for (id, value) in usage {
            append_string(&mut signature, id);
            signature.number(*value as u64);
        }
        let mut enabled = app.enabled_plugins.as_ref().map(|values| {
            let mut values = values.iter().collect::<Vec<_>>();
            values.sort_unstable();
            values
        });
        match enabled.as_mut() {
            Some(values) => {
                signature.number(1);
                signature.number(values.len() as u64);
                for value in values.iter() {
                    append_string(&mut signature, value);
                }
            }
            None => signature.number(0),
        }
        signature.finish()
    }

    fn track_c_prepare_search_state(
        root: &std::path::Path,
        actions: &[Action],
        scenario: TrackCSearchCase,
    ) -> TrackCSearchState {
        let provider_result = Action {
            label: "Track C marker provider result".into(),
            desc: "controlled provider marker".into(),
            action: "track_c_provider:marker".into(),
            args: Some(String::new()),
        };
        let query = scenario.query();
        let ctx = egui::Context::default();
        let mut app = track_c_search_app(&ctx, root, actions.to_vec(), provider_result, None);
        if matches!(
            scenario,
            TrackCSearchCase::SparseExact | TrackCSearchCase::PendingExact
        ) {
            for index in [
                actions.len().saturating_sub(1),
                actions.len().saturating_sub(2),
                actions.len().saturating_sub(3),
            ] {
                if let Some(action) = actions.get(index) {
                    app.folder_aliases_lc.insert(
                        action.action.clone(),
                        Some("track-c-marker shared-alias".into()),
                    );
                }
            }
        }
        if matches!(scenario, TrackCSearchCase::DenseExact) {
            for index in [
                actions.len().saturating_sub(1),
                actions.len().saturating_sub(2),
                actions.len().saturating_sub(3),
            ] {
                if let Some(action) = actions.get(index) {
                    app.folder_aliases_lc.insert(
                        action.action.clone(),
                        Some("track-c-empty-query alias".into()),
                    );
                }
            }
        }
        for index in [1, 7, 13] {
            if let Some(action) = actions.get(index) {
                app.usage.insert(action.action.clone(), 8);
            }
        }
        if matches!(
            scenario,
            TrackCSearchCase::SparseExact | TrackCSearchCase::PendingExact
        ) {
            for index in [
                actions.len().saturating_sub(1),
                actions.len().saturating_sub(2),
                actions.len().saturating_sub(3),
            ] {
                if let Some(action) = actions.get(index) {
                    app.usage.insert(action.action.clone(), 8);
                }
            }
        }
        app.match_exact = !matches!(scenario, TrackCSearchCase::DenseFuzzy);
        let captured = app
            .plugins
            .search_snapshot(
                app.enabled_plugins.as_ref(),
                app.enabled_capabilities.as_ref(),
            )
            .search(query);
        let mut captured = captured;
        if matches!(scenario, TrackCSearchCase::PendingExact) {
            captured.pending = true;
        }
        let expected = track_c_eager_search_outcome(&app, query, &captured);
        let provider_action_ids = captured
            .actions
            .iter()
            .map(|action| action.action.as_str())
            .collect::<HashSet<_>>();
        let expected_local_hits = expected
            .actions
            .iter()
            .filter(|action| !provider_action_ids.contains(action.action.as_str()))
            .count() as u64;
        let fixture_signature =
            track_c_search_fixture_signature(&app, query, scenario, &captured.actions);
        crate::performance::track_c::reset();
        TrackCSearchState {
            app,
            scenario,
            query: query.to_string(),
            plugin_snapshot: Some(captured),
            provider_search_was_called: None,
            expected,
            expected_local_hits,
            fixture_signature,
        }
    }

    fn track_c_validate_search_state(
        state: &TrackCSearchState,
        actual: &LauncherSearchOutcome,
        require_metrics: bool,
    ) -> crate::performance::track_c_workloads::OwnerObservation {
        use crate::performance::track_c::{self, Phase};
        use crate::performance::track_c_workloads::OwnerObservation;

        assert_eq!(actual.state, state.expected.state);
        assert_eq!(
            actual.actions, state.expected.actions,
            "complete ordered Actions/args parity"
        );
        assert_eq!(actual.provider_revision, state.expected.provider_revision);
        assert_eq!(
            actual.result_catalog_versions,
            state.expected.result_catalog_versions
        );
        assert_eq!(
            actual.result_catalog_versions_stable,
            state.expected.result_catalog_versions_stable
        );
        assert_eq!(actual.provider_deferral, state.expected.provider_deferral);
        let score = track_c::snapshot()
            .into_iter()
            .find(|(phase, _)| *phase == Phase::SearchScoreAndCloneHits)
            .expect("search score phase is fixed-cardinality")
            .1;
        let moved = track_c::snapshot()
            .into_iter()
            .find(|(phase, _)| *phase == Phase::SearchMoveResults)
            .expect("search move phase is fixed-cardinality")
            .1;
        if require_metrics && !state.query.trim().is_empty() {
            assert_eq!(
                score.calls, 1,
                "actual shared search owner scored this query"
            );
            assert_eq!(
                moved.calls, 1,
                "actual shared search owner moved final results"
            );
            assert_eq!(score.work_units, state.app.actions.len() as u64);
            assert_eq!(
                score.candidates_scored,
                if matches!(state.scenario, TrackCSearchCase::DenseExact) {
                    0
                } else {
                    state.app.actions.len() as u64
                }
            );
            assert_eq!(score.action_clones, state.expected_local_hits);
            assert_eq!(moved.work_units, actual.actions.len() as u64);
        }
        let structure =
            crate::performance::track_c_workloads::action_fixture_identity(&actual.actions);
        let complete = track_c_search_state_signature(actual);
        let catalog_versions = actual.result_catalog_versions;
        let mut revisions = vec![actual.provider_revision];
        if let Some(versions) = catalog_versions {
            revisions.extend([versions.clipboard, versions.todo, versions.notes]);
        }
        revisions.push(actual.result_catalog_versions_stable as u64);
        revisions.push(match actual.provider_deferral {
            ProviderSearchDeferral::None => 0,
            ProviderSearchDeferral::Capacity => 1,
        });
        OwnerObservation {
            output_identity: complete,
            structural_signature: structure,
            revision_receipts: revisions,
            viewport_receipt: None,
            work_units: score.candidates_scored + moved.work_units,
            work_counters: vec![
                score.work_units,
                score.candidates_scored,
                score.action_clones,
                moved.work_units,
            ],
        }
    }

    #[test]
    fn track_c_oracle_search_shared_owner_matches_frozen_complete_results() {
        use crate::performance::track_c_workloads::{TrackCWorkspace, action_fixture};

        let workspace = TrackCWorkspace::new();
        let fixture = action_fixture(100);
        let mut state = track_c_prepare_search_state(
            workspace.root(),
            &fixture.values,
            TrackCSearchCase::SparseExact,
        );
        let actual = state.app.search_read_only_outcome_with_plugin_snapshot(
            &state.query,
            state
                .plugin_snapshot
                .take()
                .expect("captured provider snapshot is consumed once"),
        );
        let observation = track_c_validate_search_state(&state, &actual, false);
        assert_eq!(
            actual.actions.len(),
            4,
            "three colliding aliases and one controlled provider action participate"
        );
        assert_eq!(actual.actions[0].action, fixture.values[97].action);
        assert_eq!(actual.actions[1].action, fixture.values[98].action);
        assert_eq!(actual.actions[2].action, fixture.values[99].action);
        assert_eq!(actual.actions[0].args, fixture.values[97].args);
        assert_eq!(actual.actions[1].args, fixture.values[98].args);
        assert_eq!(actual.actions[2].args, fixture.values[99].args);
        assert_eq!(
            actual.actions[3].action, "track_c_provider:marker",
            "provider contribution follows higher weighted alias hits"
        );
        assert_ne!(observation.output_identity, 0);
        drop(state);

        for scenario in [
            TrackCSearchCase::DenseExact,
            TrackCSearchCase::DenseFuzzy,
            TrackCSearchCase::EmptyBrowse,
            TrackCSearchCase::PendingExact,
        ] {
            let mut state =
                track_c_prepare_search_state(workspace.root(), &fixture.values, scenario);
            let actual = state.app.search_read_only_outcome_with_plugin_snapshot(
                &state.query,
                state
                    .plugin_snapshot
                    .take()
                    .expect("captured provider snapshot is consumed once"),
            );
            let _ = track_c_validate_search_state(&state, &actual, false);
            match scenario {
                TrackCSearchCase::DenseExact | TrackCSearchCase::DenseFuzzy => {
                    assert_eq!(actual.actions.len(), fixture.values.len() + 1);
                }
                TrackCSearchCase::EmptyBrowse => {
                    assert_eq!(actual.actions.len(), fixture.values.len());
                    assert!(actual.actions[0].label.starts_with("app "));
                }
                TrackCSearchCase::PendingExact => {
                    assert_eq!(actual.state, LauncherSearchState::Pending);
                }
                TrackCSearchCase::SparseExact => unreachable!(),
            }
            drop(state);
        }
        drop(workspace);
    }

    #[test]
    fn track_c2a_oracle_search_shared_owner_matches_work_receipts() {
        use crate::performance::track_c_workloads::{TrackCWorkspace, action_fixture};

        let workspace = TrackCWorkspace::new();
        let fixture = action_fixture(100);
        for scenario in [
            TrackCSearchCase::SparseExact,
            TrackCSearchCase::DenseExact,
            TrackCSearchCase::DenseFuzzy,
            TrackCSearchCase::EmptyBrowse,
            TrackCSearchCase::PendingExact,
        ] {
            let mut state =
                track_c2a_prepare_search_state(workspace.root(), &fixture.values, scenario);
            let signature = state.frozen.fixture_signature;
            let actual = track_c2a_run_search_owner(&mut state);
            let observation = track_c2a_validate_search_state(&mut state, &actual, false);
            assert_eq!(state.frozen.fixture_signature, signature);
            assert_eq!(observation.work_counters.len(), 26);
            assert_eq!(
                observation.structural_signature,
                crate::performance::track_c_workloads::action_fixture_identity(&actual.actions)
            );
            drop(state);
        }
        drop(workspace);
    }

    #[test]
    fn track_c2a_oracle_covers_local_filter_identity_usage_and_mode_edges() {
        use crate::{
            performance::track_c_workloads::TrackCWorkspace,
            radial::dynamic::MutableResultCatalogVersions,
        };

        let workspace = TrackCWorkspace::new();
        let ctx = egui::Context::default();
        let mut app = new_inert_app(&ctx, workspace.root());
        app.folder_aliases.clear();
        app.folder_aliases_lc.clear();
        app.bookmark_aliases.clear();
        app.bookmark_aliases_lc.clear();

        let folder_keep = Action {
            label: "Needle keep".into(),
            desc: "Folder".into(),
            action: "folder:keep".into(),
            args: Some("kept".into()),
        };
        let duplicate_none = Action {
            label: "Needle".into(),
            desc: "Folder".into(),
            action: "duplicate:item".into(),
            args: None,
        };
        let duplicate_empty = Action {
            label: "NEEDLE".into(),
            desc: "Folder".into(),
            action: "duplicate:item".into(),
            args: Some(String::new()),
        };
        let duplicate_value = Action {
            label: "needle".into(),
            desc: "Folder".into(),
            action: "duplicate:item".into(),
            args: Some("--mode full".into()),
        };
        let folder_skip = Action {
            label: "Needle skip".into(),
            desc: "Folder".into(),
            action: "folder:skip".into(),
            args: None,
        };
        let file_keep = Action {
            label: "Needle file".into(),
            desc: "File".into(),
            action: "file:keep".into(),
            args: None,
        };
        app.actions = Arc::new(vec![
            folder_keep.clone(),
            duplicate_none.clone(),
            duplicate_empty.clone(),
            duplicate_value.clone(),
            folder_skip.clone(),
            file_keep.clone(),
        ]);
        app.update_action_cache();
        app.match_exact = true;
        app.usage_weight = 0.5;
        app.usage.insert("duplicate:item".into(), 8);
        let versions = MutableResultCatalogVersions::default();
        let empty_provider = || c2a_snapshot(Vec::new(), false, 7, 7, versions, versions);

        let tied_and_weighted = c2a_assert_reference_parity(&app, "app needle", empty_provider());
        assert_eq!(
            tied_and_weighted.actions,
            vec![
                duplicate_none.clone(),
                duplicate_empty.clone(),
                duplicate_value.clone(),
                folder_keep.clone(),
                folder_skip.clone(),
                file_keep.clone(),
            ],
            "usage moves the duplicate-ID group ahead while equal case-folded matches retain source order and all args variants"
        );

        let positive_filters = c2a_assert_reference_parity(
            &app,
            "app needle kind:folder id:folder:keep",
            empty_provider(),
        );
        assert_eq!(positive_filters.actions, vec![folder_keep.clone()]);

        let negative_filters = c2a_assert_reference_parity(
            &app,
            "app needle kind:folder !id:duplicate:item !id:folder:skip",
            empty_provider(),
        );
        assert_eq!(negative_filters.actions, vec![folder_keep]);

        let no_hits = c2a_assert_reference_parity(&app, "app absent", empty_provider());
        assert!(no_hits.actions.is_empty());

        let fuzzy_candidate = Action {
            label: "Needle".into(),
            desc: "A fuzzy-only subsequence candidate".into(),
            action: "candidate:one".into(),
            args: None,
        };
        app.actions = Arc::new(vec![fuzzy_candidate.clone()]);
        app.update_action_cache();
        app.usage.clear();
        app.match_exact = false;
        app.fuzzy_weight = 0.0;
        assert!(
            app.matcher
                .fuzzy_match(&fuzzy_candidate.label, "ndl")
                .is_some()
        );
        let exact_override = c2a_assert_reference_parity(&app, "app ndl", empty_provider());
        assert!(
            exact_override.actions.is_empty(),
            "nonpositive fuzzy weight forces literal exact matching even when the fuzzy matcher accepts"
        );
        app.fuzzy_weight = 0.25;
        let fuzzy_enabled = c2a_assert_reference_parity(&app, "app ndl", empty_provider());
        assert_eq!(fuzzy_enabled.actions, vec![fuzzy_candidate]);

        drop(app);
        drop(workspace);
    }

    #[test]
    fn track_c2a_oracle_covers_provider_precedence_and_snapshot_outcomes() {
        use crate::{
            performance::track_c_workloads::TrackCWorkspace,
            radial::dynamic::MutableResultCatalogVersions,
        };

        let workspace = TrackCWorkspace::new();
        let ctx = egui::Context::default();
        let mut app = new_inert_app(&ctx, workspace.root());
        app.folder_aliases.clear();
        app.folder_aliases_lc.clear();
        app.bookmark_aliases.clear();
        app.bookmark_aliases_lc.clear();
        let local = Action {
            label: "Local marker".into(),
            desc: "Owned app result".into(),
            action: "local:marker".into(),
            args: Some("local".into()),
        };
        let command = Action {
            label: "Needle command".into(),
            desc: "Owned command fallback".into(),
            action: "command:needle".into(),
            args: Some("command".into()),
        };
        app.actions = Arc::new(vec![local.clone()]);
        app.update_action_cache();
        app.command_cache = vec![command.clone()];
        app.command_search_cache = vec![CachedSearchEntry::from_action(&command)];
        app.match_exact = false;
        app.fuzzy_weight = 0.25;
        let versions = MutableResultCatalogVersions::default();
        let unrelated_provider = Action {
            label: "Unrelated provider result".into(),
            desc: "No matching terms".into(),
            action: "provider:zero-score".into(),
            args: Some("provider".into()),
        };

        let provider_wins = c2a_assert_reference_parity(
            &app,
            "needle",
            c2a_snapshot(
                vec![unrelated_provider.clone()],
                false,
                10,
                10,
                versions,
                versions,
            ),
        );
        assert_eq!(provider_wins.actions, vec![unrelated_provider.clone()]);
        assert_ne!(provider_wins.actions, vec![command.clone()]);

        let command_fallback = c2a_assert_reference_parity(
            &app,
            "needle",
            c2a_snapshot(Vec::new(), false, 10, 10, versions, versions),
        );
        assert_eq!(command_fallback.actions, vec![command.clone()]);

        let empty_browse = c2a_assert_reference_parity(
            &app,
            "",
            c2a_snapshot(
                vec![unrelated_provider.clone()],
                false,
                10,
                10,
                versions,
                versions,
            ),
        );
        assert_eq!(empty_browse.actions[0], command);
        assert_eq!(
            empty_browse.actions[1],
            Action {
                label: "app Local marker".into(),
                ..local.clone()
            }
        );
        assert_eq!(empty_browse.actions.len(), 2);

        // The production provider filter gives empty exact terms score zero;
        // a present alias must not turn those empty terms into score-one hits.
        app.match_exact = true;
        app.folder_aliases_lc
            .insert("provider:empty-alias".into(), Some("fixture alias".into()));
        let empty_term_provider = Action {
            label: "Provider empty-term row".into(),
            desc: "No matching local query".into(),
            action: "provider:empty-alias".into(),
            args: None,
        };
        let empty_term = c2a_assert_reference_parity(
            &app,
            "app",
            c2a_snapshot(
                vec![empty_term_provider.clone()],
                false,
                10,
                10,
                versions,
                versions,
            ),
        );
        assert_eq!(empty_term.actions, vec![local.clone(), empty_term_provider]);

        // Exact display matching trims only its label query; desc, ID, and
        // alias predicates retain the provider term they were given.
        app.folder_aliases_lc.clear();
        let spaced_provider = Action {
            label: "Needle".into(),
            desc: "No matching description".into(),
            action: "provider:spaced".into(),
            args: None,
        };
        let spaced_g_prefix = c2a_assert_reference_parity(
            &app,
            "g  needle",
            c2a_snapshot(
                vec![spaced_provider.clone()],
                false,
                10,
                10,
                versions,
                versions,
            ),
        );
        assert_eq!(spaced_g_prefix.actions, vec![spaced_provider]);

        app.actions = Arc::new(vec![local.clone()]);
        app.update_action_cache();
        app.command_cache.clear();
        app.command_search_cache.clear();
        app.update_command_cache();
        for (snapshot, expected_state) in [
            (
                c2a_snapshot(Vec::new(), false, 20, 20, versions, versions),
                LauncherSearchState::Results,
            ),
            (
                c2a_snapshot(Vec::new(), true, 20, 20, versions, versions),
                LauncherSearchState::Pending,
            ),
            (
                c2a_snapshot(Vec::new(), false, 20, 21, versions, versions),
                LauncherSearchState::Pending,
            ),
            (
                c2a_snapshot(
                    Vec::new(),
                    false,
                    20,
                    20,
                    versions,
                    MutableResultCatalogVersions {
                        notes: versions.notes + 1,
                        ..versions
                    },
                ),
                LauncherSearchState::Pending,
            ),
        ] {
            let outcome = c2a_assert_reference_parity(&app, "app marker", snapshot);
            assert_eq!(outcome.state, expected_state);
            assert_eq!(outcome.actions, vec![local.clone()]);
        }

        drop(app);
        drop(workspace);
    }

    #[test]
    #[ignore = "opt-in C2-A search work attribution; uses only isolated synthetic catalog/provider state"]
    fn track_c2a_benchmark_search_shared_owner_attribution() {
        use crate::performance::track_c_workloads::{
            BenchmarkMode, TrackCWorkspace, action_fixture, measure_owner,
        };

        assert!(crate::performance::enabled());
        let mode = BenchmarkMode::from_process_env().expect("valid Track C mode");
        let workspace = TrackCWorkspace::new();
        for count in mode.action_sizes() {
            let fixture = action_fixture(*count);
            for scenario in [
                TrackCSearchCase::SparseExact,
                TrackCSearchCase::DenseExact,
                TrackCSearchCase::DenseFuzzy,
                TrackCSearchCase::EmptyBrowse,
                TrackCSearchCase::PendingExact,
            ] {
                let fixture_state =
                    track_c2a_prepare_search_state(workspace.root(), &fixture.values, scenario);
                let fixture_signature = fixture_state.frozen.fixture_signature;
                drop(fixture_state);
                let samples = measure_owner(
                    || track_c2a_prepare_search_state(workspace.root(), &fixture.values, scenario),
                    track_c2a_run_search_owner,
                    |state, actual| {
                        assert_eq!(state.frozen.fixture_signature, fixture_signature);
                        track_c2a_validate_search_state(state, actual, true)
                    },
                );
                emit_track_c2a_samples(scenario, *count, fixture_signature, mode, &samples);
            }
        }
        drop(workspace);
    }

    #[test]
    fn track_c2b_oracle_plugin_heavy_search_matches_c2a_reference() {
        use crate::performance::track_c_workloads::{TrackCWorkspace, action_fixture};

        let workspace = TrackCWorkspace::new();
        let fixture = action_fixture(1_000);
        let mut state = track_c2b_prepare_plugin_heavy_state(workspace.root(), &fixture.values);
        assert_eq!(state.provider_input_actions, 128);
        assert_eq!(state.provider_projection_strings, 384);
        assert_eq!(state.provider_projection_bytes, 14_976);
        let provider_payload = state
            .frozen
            .plugin_snapshot
            .as_ref()
            .expect("provider snapshot is captured during setup")
            .actions
            .iter()
            .fold((0_u64, 0_u64), |(bytes, strings), action| {
                (
                    bytes.saturating_add(c2a_search_observer::action_payload_bytes(action)),
                    strings.saturating_add(c2a_search_observer::action_payload_strings(action)),
                )
            });
        assert_eq!(provider_payload, (15_816, 469));
        assert_eq!(state.frozen.expected.actions.len(), 1_128);
        let signature = state.frozen.fixture_signature;
        assert_eq!(signature, 0xd5a1_cd25_4a20_5e24);
        let actual = track_c2a_run_search_owner(&mut state);
        let observation = track_c2b_validate_search_state(&mut state, &actual, false);
        assert_eq!(state.frozen.fixture_signature, signature);
        assert_eq!(actual.actions.len(), 1_128);
        assert_eq!(observation.work_counters.len(), 31);
        assert_eq!(observation.work_counters[4], 1_000);
        assert_eq!(observation.work_counters[7], 128);
        assert_eq!(observation.work_counters[8], 0);
        assert_eq!(observation.work_counters[9], 128);
        assert_eq!(observation.work_counters[10], 384);
        assert_eq!(observation.work_counters[11], 14_976);
        assert_eq!(observation.work_counters[16], 1_128);
        assert_eq!(observation.work_counters[19], 0);
        assert_eq!(observation.work_counters[28], 1_000);
        drop(state);
        drop(workspace);
    }

    #[test]
    #[ignore = "opt-in C2-B eager materialization baseline; isolated synthetic catalog and captured provider actions only"]
    fn track_c2b_benchmark_search_materialization() {
        use crate::performance::track_c_workloads::{
            BenchmarkMode, TrackCWorkspace, action_fixture, measure_owner,
        };

        assert!(crate::performance::enabled());
        let mode = BenchmarkMode::from_process_env().expect("valid Track C mode");
        let workspace = TrackCWorkspace::new();
        for count in mode.action_sizes() {
            let fixture = action_fixture(*count);
            for scenario in [
                TrackCSearchCase::SparseExact,
                TrackCSearchCase::DenseExact,
                TrackCSearchCase::DenseFuzzy,
                TrackCSearchCase::EmptyBrowse,
                TrackCSearchCase::PendingExact,
            ] {
                let fixture_state =
                    track_c2a_prepare_search_state(workspace.root(), &fixture.values, scenario);
                let fixture_signature = fixture_state.frozen.fixture_signature;
                let provider_count = fixture_state.provider_input_actions as usize;
                drop(fixture_state);
                let samples = measure_owner(
                    || track_c2a_prepare_search_state(workspace.root(), &fixture.values, scenario),
                    track_c2a_run_search_owner,
                    |state, actual| {
                        assert_eq!(state.frozen.fixture_signature, fixture_signature);
                        track_c2b_validate_search_state(state, actual, true)
                    },
                );
                emit_track_c2b_samples(
                    scenario.name(),
                    *count,
                    provider_count,
                    fixture_signature,
                    mode,
                    &samples,
                );
            }
        }

        let fixture = action_fixture(1_000);
        let fixture_state = track_c2b_prepare_plugin_heavy_state(workspace.root(), &fixture.values);
        let fixture_signature = fixture_state.frozen.fixture_signature;
        let provider_count = fixture_state.provider_input_actions as usize;
        drop(fixture_state);
        let samples = measure_owner(
            || track_c2b_prepare_plugin_heavy_state(workspace.root(), &fixture.values),
            track_c2a_run_search_owner,
            |state, actual| {
                assert_eq!(state.frozen.fixture_signature, fixture_signature);
                track_c2b_validate_search_state(state, actual, true)
            },
        );
        emit_track_c2b_samples(
            "plugin-heavy-dense",
            1_000,
            provider_count,
            fixture_signature,
            mode,
            &samples,
        );
        drop(workspace);
    }

    #[test]
    fn track_c_fixture_search_ordinary_path_calls_controlled_provider() {
        use crate::performance::track_c_workloads::TrackCWorkspace;

        let workspace = TrackCWorkspace::new();
        let searched = Arc::new(AtomicBool::new(false));
        let ctx = egui::Context::default();
        let provider_result = Action {
            label: "Track C ordinary provider result".into(),
            desc: "ordinary provider marker".into(),
            action: "track_c_provider:marker".into(),
            args: Some("ordinary".into()),
        };
        let app = track_c_search_app(
            &ctx,
            workspace.root(),
            Vec::new(),
            provider_result,
            Some(Arc::clone(&searched)),
        );
        let outcome = app.search_read_only_outcome("app marker");
        assert!(searched.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(outcome.state, LauncherSearchState::Results);
        assert_eq!(outcome.actions.len(), 1);
        assert_eq!(outcome.actions[0].action, "track_c_provider:marker");
        drop(app);
        drop(workspace);
    }

    #[test]
    #[ignore = "opt-in Track C owner benchmark; isolated action catalog/provider only"]
    fn track_c_benchmark_search_shared_owner() {
        use crate::performance::track_c_workloads::{
            BenchmarkMode, ReportMetadata, TrackCWorkspace, action_fixture, emit_samples,
            measure_owner,
        };

        assert!(crate::performance::enabled());
        let mode = BenchmarkMode::from_process_env().expect("valid Track C mode");
        let workspace = TrackCWorkspace::new();
        for count in mode.action_sizes() {
            let fixture = action_fixture(*count);
            for scenario in [
                TrackCSearchCase::SparseExact,
                TrackCSearchCase::DenseExact,
                TrackCSearchCase::DenseFuzzy,
                TrackCSearchCase::EmptyBrowse,
                TrackCSearchCase::PendingExact,
            ] {
                let fixture_state =
                    track_c_prepare_search_state(workspace.root(), &fixture.values, scenario);
                let fixture_signature = fixture_state.fixture_signature;
                drop(fixture_state);
                let samples = measure_owner(
                    || track_c_prepare_search_state(workspace.root(), &fixture.values, scenario),
                    |state| {
                        state.app.search_read_only_outcome_with_plugin_snapshot(
                            &state.query,
                            state
                                .plugin_snapshot
                                .take()
                                .expect("captured provider snapshot is consumed once"),
                        )
                    },
                    |state, actual| {
                        assert_eq!(state.fixture_signature, fixture_signature);
                        track_c_validate_search_state(state, actual, true)
                    },
                );
                let (settings, cold_type) = match scenario {
                    TrackCSearchCase::SparseExact => (
                        "exact-alias-collision-usage-tied",
                        "fresh-query-sparse-provider-result",
                    ),
                    TrackCSearchCase::DenseExact => (
                        "exact-empty-term-usage-weighted",
                        "fresh-query-dense-results",
                    ),
                    TrackCSearchCase::DenseFuzzy => {
                        ("fuzzy-mode-usage-weighted", "fresh-query-dense-results")
                    }
                    TrackCSearchCase::EmptyBrowse => {
                        ("empty-query-browse", "fresh-query-complete-catalog")
                    }
                    TrackCSearchCase::PendingExact => (
                        "exact-alias-collision-provider-pending",
                        "fresh-query-pending-provider-snapshot",
                    ),
                };
                emit_samples(
                    ReportMetadata {
                        owner: "LauncherApp::search_read_only_outcome_with_plugin_snapshot",
                        fixture_name: scenario.name(),
                        fixture_signature,
                        item_count: *count,
                        viewport: "not-rendered",
                        scale_milli: 1_000,
                        font_state: "not-applicable",
                        settings,
                        cold_type,
                        mode,
                    },
                    &samples,
                );
            }
        }
        drop(workspace);
    }

    #[test]
    fn deferred_snapshot_matches_ordinary_g_prefix_provider_selection() {
        let workspace = crate::performance::track_c_workloads::TrackCWorkspace::new();
        for enabled in [
            Some(HashSet::from([
                "web_search".to_string(),
                "always_provider".to_string(),
            ])),
            Some(HashSet::new()),
        ] {
            let ctx = egui::Context::default();
            let mut app = new_inert_app(&ctx, workspace.root());
            app.plugins
                .register(Box::new(g_search_plugin("web_search", "Web needle")));
            app.plugins.register(Box::new(g_search_plugin(
                "always_provider",
                "Always needle",
            )));
            app.enabled_plugins = enabled;

            let ordinary = app.search_read_only_outcome("g needle");
            let provider_snapshot = app
                .plugins
                .search_snapshot(
                    app.enabled_plugins.as_ref(),
                    app.enabled_capabilities.as_ref(),
                )
                .search("g needle");
            let deferred =
                app.search_read_only_outcome_with_plugin_snapshot("g needle", provider_snapshot);

            assert_eq!(deferred.state, ordinary.state);
            assert_eq!(deferred.actions, ordinary.actions);
            assert_eq!(
                deferred
                    .actions
                    .iter()
                    .map(|action| action.action.as_str())
                    .collect::<Vec<_>>(),
                ["web_search:needle"]
            );
            drop(app);
        }
        drop(workspace);
    }

    #[test]
    fn manual_query_fallback_does_not_reenter_a_blocked_provider() {
        let ctx = egui::Context::default();
        let workspace = crate::performance::track_c_workloads::TrackCWorkspace::new();
        let mut app = new_inert_app(&ctx, workspace.root());
        let searched = Arc::new(AtomicBool::new(false));
        app.plugins.register(Box::new(StaticSearchPlugin {
            name: "blocked_provider",
            result: Action {
                label: "Provider result".into(),
                desc: "needle".into(),
                action: "blocked:result".into(),
                args: None,
            },
            always: true,
            searched: Some(Arc::clone(&searched)),
        }));
        let _provider_permit = app
            .radial_provider_search_capacity
            .try_acquire()
            .expect("test owns the active provider slot");

        let outcome = app.search_read_only_outcome("needle");

        assert_eq!(outcome.state, LauncherSearchState::Pending);
        assert!(outcome.actions.is_empty());
        assert!(!searched.load(std::sync::atomic::Ordering::SeqCst));
        drop(app);
        drop(workspace);
    }

    #[test]
    fn deferred_fallback_suppression_survives_provider_slot_becoming_free() {
        let ctx = egui::Context::default();
        let workspace = crate::performance::track_c_workloads::TrackCWorkspace::new();
        let mut app = new_inert_app(&ctx, workspace.root());
        let searched = Arc::new(AtomicBool::new(false));
        app.plugins.register(Box::new(StaticSearchPlugin {
            name: "fallback_provider",
            result: Action {
                label: "Provider result".into(),
                desc: "needle".into(),
                action: "fallback:result".into(),
                args: None,
            },
            always: true,
            searched: Some(Arc::clone(&searched)),
        }));

        // No provider currently owns the bounded slot. The suppression is
        // attached to the failed deferred fallback itself, not inferred from
        // this transient capacity state.
        assert!(!app.radial_provider_search_capacity.is_occupied());
        app.query = "needle".into();
        app.radial_suppressed_provider_query = Some("needle".into());
        app.search();
        assert!(!searched.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(app.radial_suppressed_provider_query, None);

        // A later ordinary edit returns to the regular provider path.
        app.query = "needle again".into();
        app.search();
        assert!(searched.load(std::sync::atomic::Ordering::SeqCst));
        drop(app);
        drop(workspace);
    }

    fn assert_first_result_is_clipboard_modify_open_modify(app: &LauncherApp) {
        let first = app
            .results
            .first()
            .expect("clipboard modify result from launcher search path");
        assert_eq!(first.label, "cm: Open Clipboard Modify");
        assert_eq!(first.desc, "Opens the Clipboard Modify dialog section");
        assert_eq!(first.action, "clipboard_modify:open:modify");

        let encoded = first
            .args
            .as_deref()
            .expect("direct Clipboard Modify open action carries encoded section payload");
        let payload: ClipboardModifyActionPayload = decode_action_payload(encoded).unwrap();
        assert_eq!(
            payload,
            ClipboardModifyActionPayload::OpenDialogSection {
                section: ClipboardModifySectionPayload::Modify,
            }
        );
    }

    #[test]
    fn search_replacement_clears_selected_index() {
        let ctx = egui::Context::default();
        let workspace = crate::performance::track_c_workloads::TrackCWorkspace::new();
        let mut app = new_inert_app(&ctx, workspace.root());
        app.actions = Arc::new(vec![Action {
            label: "Calculator".into(),
            desc: "App".into(),
            action: "calc".into(),
            args: None,
        }]);
        app.update_action_cache();
        app.selected = Some(0);
        app.query = "app calc".into();

        app.search();

        assert_eq!(app.results.len(), 1);
        assert_eq!(app.selected, None);
        drop(app);
        drop(workspace);
    }

    #[test]
    fn read_only_search_uses_launcher_boundary_without_mutating_root_state() {
        let ctx = egui::Context::default();
        let workspace = crate::performance::track_c_workloads::TrackCWorkspace::new();
        let mut app = new_inert_app(&ctx, workspace.root());
        app.actions = Arc::new(vec![Action {
            label: "Calculator".into(),
            desc: "App".into(),
            action: "calc".into(),
            args: Some("--safe".into()),
        }]);
        app.update_action_cache();
        app.query = "root query".into();
        app.results = vec![Action {
            label: "Root result".into(),
            desc: String::new(),
            action: "root".into(),
            args: None,
        }];
        app.selected = Some(0);
        let before = (
            app.query.clone(),
            app.results.clone(),
            app.selected,
            app.root_list_geometry.result_generation_for_test(),
            app.last_search_query.clone(),
            app.last_results_valid,
        );
        let results = app.search_read_only("app calc");
        assert!(
            results
                .iter()
                .any(|action| action.action == "calc" && action.args.as_deref() == Some("--safe"))
        );
        assert_eq!(
            (
                app.query.clone(),
                app.results.clone(),
                app.selected,
                app.root_list_geometry.result_generation_for_test(),
                app.last_search_query.clone(),
                app.last_results_valid,
            ),
            before
        );
        drop(app);
        drop(workspace);
    }

    #[test]
    fn screen_draw_priority_fixture_requires_normal_app_query_without_read_only_effects() {
        let workspace = crate::performance::track_c_workloads::TrackCWorkspace::new();
        let ctx = egui::Context::default();
        let mut app = new_inert_app(&ctx, workspace.root());
        app.folder_aliases.clear();
        app.folder_aliases_lc.clear();
        app.bookmark_aliases.clear();
        app.bookmark_aliases_lc.clear();
        app.dashboard_initial_refresh_queued = true;
        let label = "Radial Acceptance Screen Draw Priority Smoke";
        let fixture_action = Action {
            label: label.into(),
            desc: "Safe owned Screen Draw acceptance entry".into(),
            action: "screen_draw:start".into(),
            args: None,
        };
        app.actions = Arc::new(vec![fixture_action.clone()]);
        app.update_action_cache();
        app.query = "unchanged root query".into();
        assert!(app.launcher_hwnd.is_none());
        app.selected = Some(0);
        app.results = vec![Action {
            label: "Existing row".into(),
            desc: String::new(),
            action: "existing".into(),
            args: None,
        }];
        let before = (
            app.query.clone(),
            app.results.clone(),
            app.selected,
            app.usage.clone(),
            app.test_activation_trace.clone(),
            app.test_recorded_history_queries.clone(),
            app.visible_flag.load(std::sync::atomic::Ordering::SeqCst),
            app.restore_flag.load(std::sync::atomic::Ordering::SeqCst),
        );

        // The normal read-only boundary calls the real custom-action search.
        // A bare label deliberately does not opt into that search namespace.
        let prefixed = app.search_read_only_outcome(&format!("app {label}"));
        assert_eq!(prefixed.state, LauncherSearchState::Results);
        assert_eq!(prefixed.actions, vec![fixture_action.clone()]);
        let bare = app.search_read_only_outcome(label);
        assert!(!bare.actions.iter().any(|action| action == &fixture_action));
        assert_eq!(bare.state, LauncherSearchState::NoResults);
        assert!(app.launcher_hwnd.is_none());
        assert_eq!(
            (
                app.query.clone(),
                app.results.clone(),
                app.selected,
                app.usage.clone(),
                app.test_activation_trace.clone(),
                app.test_recorded_history_queries.clone(),
                app.visible_flag.load(std::sync::atomic::Ordering::SeqCst),
                app.restore_flag.load(std::sync::atomic::Ordering::SeqCst)
            ),
            before,
        );
        drop(app);
        drop(workspace);
    }

    #[test]
    fn app_prefixed_qmarker_fixture_returns_alpha_before_beta() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.actions = Arc::new(vec![
            Action {
                label: "QMarker Alpha".into(),
                desc: "First harmless marker".into(),
                action: "marker.exe".into(),
                args: Some("--marker q-first".into()),
            },
            Action {
                label: "QMarker Beta".into(),
                desc: "Second harmless marker".into(),
                action: "marker.exe".into(),
                args: Some("--marker q-second".into()),
            },
        ]);
        app.update_action_cache();

        let outcome = app.search_read_only_outcome("app QMarker");

        assert_eq!(outcome.state, LauncherSearchState::Results);
        assert_eq!(
            outcome
                .actions
                .iter()
                .map(|action| (action.label.as_str(), action.args.as_deref()))
                .collect::<Vec<_>>(),
            [
                ("QMarker Alpha", Some("--marker q-first")),
                ("QMarker Beta", Some("--marker q-second")),
            ]
        );
    }

    #[test]
    fn repeated_search_clears_stale_out_of_bounds_selected_index() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.results = vec![Action {
            label: "Only".into(),
            desc: "Result".into(),
            action: "only".into(),
            args: None,
        }];
        app.query = "same".into();
        app.last_search_query = "same".into();
        app.last_results_valid = true;
        app.selected = Some(5);

        app.search();

        assert_eq!(app.selected, None);
    }

    #[test]
    fn clipboard_modify_root_query_returns_direct_open_modify_first_in_fuzzy_mode() {
        let ctx = egui::Context::default();
        let workspace = crate::performance::track_c_workloads::TrackCWorkspace::new();
        let mut app = new_app_with_clipboard_modify(&ctx, workspace.root());
        app.query = "cm".into();
        app.match_exact = false;
        app.fuzzy_weight = 1.0;

        app.search();

        assert_first_result_is_clipboard_modify_open_modify(&app);
        drop(app);
        drop(workspace);
    }

    #[test]
    fn clipboard_modify_root_query_returns_direct_open_modify_first_in_exact_mode() {
        let ctx = egui::Context::default();
        let workspace = crate::performance::track_c_workloads::TrackCWorkspace::new();
        let mut app = new_app_with_clipboard_modify(&ctx, workspace.root());
        app.query = "cm".into();
        app.match_exact = true;
        app.fuzzy_weight = 1.0;

        app.search();

        assert_first_result_is_clipboard_modify_open_modify(&app);
        drop(app);
        drop(workspace);
    }

    #[test]
    fn cache_normalization_and_match_exact_filters_by_normalized_label() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.actions = Arc::new(vec![Action {
            label: "MiXeD Label".into(),
            desc: "MiXeD Desc".into(),
            action: "Action:ID".into(),
            args: None,
        }]);
        app.update_action_cache();

        assert_eq!(app.action_cache[0].label_lc, "mixed label");
        assert_eq!(app.action_cache[0].desc_lc, "mixed desc");
        assert_eq!(app.action_cache[0].action_lc, "action:id");
        assert!(LauncherApp::matches_exact_display_text(
            &app.action_cache[0],
            " mixed "
        ));
        assert!(!LauncherApp::matches_exact_display_text(
            &app.action_cache[0],
            "nomatch"
        ));

        app.query = "app mxd lbl".into();
        app.match_exact = false;
        app.search();
        assert!(
            app.results
                .iter()
                .any(|action| action.action == "Action:ID")
        );

        app.query = "app mxd lbl".into();
        app.match_exact = true;
        app.last_results_valid = false;
        app.search();
        assert!(app.results.is_empty());
    }

    #[test]
    fn completion_rebuild_debounce_waits_for_latest_schedule() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.query_autocomplete = true;
        app.actions = Arc::new(vec![Action {
            label: "Old App".into(),
            desc: "demo".into(),
            action: "old:app".into(),
            args: None,
        }]);
        app.update_action_cache();
        let first_due = app
            .completion_rebuild_after
            .expect("initial rebuild schedule");

        app.actions = Arc::new(vec![Action {
            label: "New App".into(),
            desc: "demo".into(),
            action: "new:app".into(),
            args: None,
        }]);
        app.update_action_cache();
        let second_due = app.completion_rebuild_after.expect("rescheduled rebuild");
        assert!(second_due >= first_due);
        assert!(app.completion_index.is_none());
        assert!(app.suggestions.is_empty());

        app.query = "app ".into();
        app.maybe_rebuild_completion_index(first_due);
        assert!(app.completion_index.is_none());
        assert!(app.suggestions.is_empty());

        app.maybe_rebuild_completion_index(second_due + Duration::from_millis(1));
        assert!(app.completion_index.is_some());
        assert!(app.suggestions.iter().any(|s| s == "app new app"));
        assert!(app.suggestions.iter().all(|s| s != "app old app"));
    }

    #[test]
    fn note_search_debounce_gate_only_fires_after_delay() {
        let start = Instant::now();
        assert!(!LauncherApp::note_search_debounce_ready(
            None,
            start,
            NOTE_SEARCH_DEBOUNCE
        ));
        assert!(!LauncherApp::note_search_debounce_ready(
            Some(start),
            start + NOTE_SEARCH_DEBOUNCE - Duration::from_millis(1),
            NOTE_SEARCH_DEBOUNCE,
        ));
        assert!(LauncherApp::note_search_debounce_ready(
            Some(start),
            start + NOTE_SEARCH_DEBOUNCE,
            NOTE_SEARCH_DEBOUNCE,
        ));
    }

    #[test]
    fn literal_qr_payload_survives_exact_sync_and_snapshot_search() {
        let ctx = egui::Context::default();
        let workspace = crate::performance::track_c_workloads::TrackCWorkspace::new();
        let mut app = new_inert_app(&ctx, workspace.root());
        app.plugins.register(Box::new(crate::plugins::qr::QrPlugin));
        app.match_exact = true;

        let payload =
            "two  spaces \"quoted\" C:\\qr\n日本語 kind:private id:private !kind:other !id:other";
        let query = format!(" \tQR {payload}");
        app.query = query.clone();
        app.search();

        let synchronous = app
            .results
            .iter()
            .find(|action| action.action == "qr:open")
            .expect("literal QR result should survive exact display filtering");
        assert_eq!(synchronous.args.as_deref(), Some(payload));

        let provider_snapshot = app
            .plugins
            .search_snapshot(
                app.enabled_plugins.as_ref(),
                app.enabled_capabilities.as_ref(),
            )
            .search(&query);
        let deferred = app.search_read_only_outcome_with_plugin_snapshot(&query, provider_snapshot);
        let snapshot_action = deferred
            .actions
            .iter()
            .find(|action| action.action == "qr:open")
            .expect("snapshot QR result should survive exact display filtering");
        assert_eq!(snapshot_action.args.as_deref(), Some(payload));
        drop(app);
        drop(workspace);
    }
}

#[cfg(test)]
mod clipboard_modify_exact_filter_tests {
    use super::*;

    #[test]
    fn date_exact_filter_preserves_resolved_values_and_errors_only() {
        assert!(LauncherApp::should_bypass_exact_post_filter(
            "date 30 days from today",
            "clipboard:2026-11-04"
        ));
        assert!(LauncherApp::should_bypass_exact_post_filter(
            "date 2026-02-30",
            "noop:invalid date"
        ));
        assert!(!LauncherApp::should_bypass_exact_post_filter(
            "date 30 days from today",
            "query:date today + 7 days"
        ));
        assert!(!LauncherApp::should_bypass_exact_post_filter(
            "date 30 days from today",
            "shell:unrelated"
        ));
        assert!(!LauncherApp::should_bypass_exact_post_filter(
            "other 30 days from today",
            "clipboard:2026-11-04"
        ));
    }

    #[test]
    fn exact_match_filter_keeps_complete_clipboard_modify_actions() {
        assert!(LauncherApp::should_bypass_exact_post_filter(
            "cm upper",
            "clipboard_modify:execute:abc"
        ));
        assert!(LauncherApp::should_bypass_exact_post_filter(
            "cm undo",
            "clipboard_modify:undo:abc"
        ));
        assert!(!LauncherApp::should_bypass_exact_post_filter(
            "cm upper",
            "query:cm uppercase"
        ));
    }

    #[test]
    fn qr_resolved_action_bypasses_only_for_qr_query_head() {
        assert!(LauncherApp::should_bypass_exact_post_filter(
            "qr hello", "qr:open"
        ));
        assert!(LauncherApp::should_bypass_exact_post_filter(
            "  QR hello",
            "qr:open"
        ));
        assert!(!LauncherApp::should_bypass_exact_post_filter(
            "qrfoo hello",
            "qr:open"
        ));
        assert!(!LauncherApp::should_bypass_exact_post_filter(
            "qr hello",
            "other:open"
        ));
    }
}
