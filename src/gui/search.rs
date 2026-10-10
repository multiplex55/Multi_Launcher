use super::*;

pub(crate) const NOTE_SEARCH_DEBOUNCE: Duration = Duration::from_secs(1);
pub(crate) const COMPLETION_REBUILD_DEBOUNCE: Duration = Duration::from_millis(120);

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
        let mut aliases = HashMap::new();
        let mut aliases_lc = HashMap::new();
        for folder in crate::plugins::folders::load_folders(crate::plugins::folders::FOLDERS_FILE)?
        {
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
        self.action_cache = self
            .actions
            .iter()
            .map(CachedSearchEntry::from_action)
            .collect();
        self.action_filter_metadata = self
            .actions
            .iter()
            .map(ActionFilterMetadata::from_action)
            .collect();
        self.actions_by_id = self
            .actions
            .iter()
            .map(|a| (a.action.clone(), a.clone()))
            .collect();
        // Search results are derived from the action catalog as well as the
        // query. Keep the cached-query fast path from treating old results as
        // current after a catalog publication.
        self.last_results_valid = false;
        self.action_completion_dirty = true;
        self.schedule_completion_rebuild();
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

    pub(crate) fn maybe_run_note_search_debounce(&mut self) {
        if !Self::is_note_search_query(&self.query) {
            self.last_note_search_change = None;
            return;
        }

        if Self::note_search_debounce_ready(
            self.last_note_search_change,
            Instant::now(),
            NOTE_SEARCH_DEBOUNCE,
        ) {
            self.search();
            self.last_note_search_change = None;
        }
    }

    pub(crate) fn clear_selected_after_results_replaced(&mut self) {
        self.selected = None;
    }

    pub fn search(&mut self) {
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
        self.results = outcome.actions;
        self.invalidate_root_list_results();
        self.clear_selected_after_results_replaced();
        self.last_search_query = self.query.clone();
        self.last_results_valid = outcome.provider_deferral == ProviderSearchDeferral::None;
        let completion_started = crate::performance::started_if(perf_enabled);
        self.update_suggestions();
        crate::performance::log_elapsed("search.completion", completion_started);
        let layout_started = crate::performance::started_if(perf_enabled);
        self.recompute_query_results_layout();
        crate::performance::log_elapsed("search.layout", layout_started);
        crate::performance::log_elapsed("search.total", total_started);
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
        if query.is_empty() {
            for (i, a) in self.actions.iter().enumerate() {
                if action_matches_filters(&self.action_filter_metadata[i], &filters) {
                    res.push((a.clone(), 0.0));
                }
            }
        } else {
            for (i, a) in self.actions.iter().enumerate() {
                if !action_matches_filters(&self.action_filter_metadata[i], &filters) {
                    continue;
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
                        res.push((a.clone(), score));
                    }
                } else {
                    let s1 = self.matcher.fuzzy_match(&a.label, query);
                    let s2 = self.matcher.fuzzy_match(&a.desc, query);
                    if let Some(score) = s1.max(s2) {
                        res.push((a.clone(), score as f32 * self.fuzzy_weight));
                    }
                }
            }
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
        let mut res = Vec::new();
        if trimmed_lc.starts_with("g ") {
            let query_term = trimmed_lc.split_once(' ').map(|x| x.1).unwrap_or("");
            for a in plugin_results {
                let cached = CachedSearchEntry::from_action(&a);
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
            return (res, pending);
        }

        if plugin_results.is_empty() && !trimmed.is_empty() {
            for (a, cached) in self
                .command_cache
                .iter()
                .zip(self.command_search_cache.iter())
            {
                if self.is_exact_match_mode() {
                    let alias_match = self.alias_matches_lc(&a.action, trimmed_lc);
                    let label_match = Self::matches_exact_display_text(cached, trimmed_lc);
                    let desc_match = cached.desc_lc.contains(trimmed_lc);
                    let action_match = cached.action_lc.contains(trimmed_lc);
                    if label_match || desc_match || action_match || alias_match {
                        let score = if alias_match { 1.0 } else { 0.0 };
                        res.push((a.clone(), score));
                    }
                } else {
                    let s1 = self.matcher.fuzzy_match(&a.label, trimmed);
                    let s2 = self.matcher.fuzzy_match(&a.desc, trimmed);
                    if let Some(score) = s1.max(s2) {
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
                provider_revision,
                result_catalog_versions,
                result_catalog_versions_stable: catalog_versions_stable,
                provider_deferral: ProviderSearchDeferral::None,
            };
        }
        let search_actions =
            trimmed_lc == APP_PREFIX || trimmed_lc.starts_with(&format!("{APP_PREFIX} "));
        let action_query = search_actions
            .then(|| trimmed.split_once(' ').map(|value| value.1).unwrap_or(""))
            .unwrap_or("");
        let mut scored = Vec::new();
        if !trimmed_lc.starts_with("g ") && search_actions {
            scored.extend(self.search_actions(action_query, &action_query.to_lowercase()));
        }
        scored.extend(plugin_results);
        self.apply_usage_weight(&mut scored);
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let mut materialize_timer = crate::performance::track_c::Timer::start(
            crate::performance::track_c::Phase::SearchMoveResults,
        );
        let materialized = scored.len();
        let actions = scored
            .into_iter()
            .map(|(action, _)| action)
            .collect::<Vec<_>>();
        materialize_timer.set_work_units(materialized);
        materialize_timer.finish(crate::performance::track_c::Outcome::Completed);
        LauncherSearchOutcome {
            state: if provider_pending
                || start_revision != provider_revision
                || !catalog_versions_stable
            {
                LauncherSearchState::Pending
            } else if actions.is_empty() {
                LauncherSearchState::NoResults
            } else {
                LauncherSearchState::Results
            },
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
        for (a, score) in res.iter_mut() {
            *score += self.usage.get(&a.action).cloned().unwrap_or(0) as f32 * self.usage_weight;
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

    fn new_app_with_clipboard_modify(ctx: &egui::Context) -> LauncherApp {
        let mut plugin_manager = PluginManager::new();
        let catalog = plugin_manager.clipboard_modifier_catalog();
        plugin_manager.register(Box::new(ClipboardModifyPlugin::new(catalog)));

        LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            plugin_manager,
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

    #[test]
    fn deferred_snapshot_matches_ordinary_g_prefix_provider_selection() {
        for enabled in [
            Some(HashSet::from([
                "web_search".to_string(),
                "always_provider".to_string(),
            ])),
            Some(HashSet::new()),
        ] {
            let ctx = egui::Context::default();
            let mut app = new_app(&ctx);
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
        }
    }

    #[test]
    fn manual_query_fallback_does_not_reenter_a_blocked_provider() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
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
    }

    #[test]
    fn deferred_fallback_suppression_survives_provider_slot_becoming_free() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
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
        let mut app = new_app(&ctx);
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
    }

    #[test]
    fn read_only_search_uses_launcher_boundary_without_mutating_root_state() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
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
        let before = (app.query.clone(), app.results.clone(), app.selected);
        let results = app.search_read_only("app calc");
        assert!(
            results
                .iter()
                .any(|action| action.action == "calc" && action.args.as_deref() == Some("--safe"))
        );
        assert_eq!(
            (app.query.clone(), app.results.clone(), app.selected),
            before
        );
    }

    #[test]
    fn screen_draw_priority_fixture_requires_normal_app_query_without_read_only_effects() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
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
        let mut app = new_app_with_clipboard_modify(&ctx);
        app.query = "cm".into();
        app.match_exact = false;
        app.fuzzy_weight = 1.0;

        app.search();

        assert_first_result_is_clipboard_modify_open_modify(&app);
    }

    #[test]
    fn clipboard_modify_root_query_returns_direct_open_modify_first_in_exact_mode() {
        let ctx = egui::Context::default();
        let mut app = new_app_with_clipboard_modify(&ctx);
        app.query = "cm".into();
        app.match_exact = true;
        app.fuzzy_weight = 1.0;

        app.search();

        assert_first_result_is_clipboard_modify_open_modify(&app);
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
        let mut app = new_app(&ctx);
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
