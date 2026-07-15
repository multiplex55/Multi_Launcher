use super::*;

pub(crate) const NOTE_SEARCH_DEBOUNCE: Duration = Duration::from_secs(1);
pub(crate) const COMPLETION_REBUILD_DEBOUNCE: Duration = Duration::from_millis(120);

impl LauncherApp {
    fn normalize_alias(alias: Option<String>) -> (Option<String>, Option<String>) {
        let alias_lc = alias.as_ref().map(|text| text.to_lowercase());
        (alias, alias_lc)
    }

    pub(crate) fn folder_alias_maps() -> (
        HashMap<String, Option<String>>,
        HashMap<String, Option<String>>,
    ) {
        let mut aliases = HashMap::new();
        let mut aliases_lc = HashMap::new();
        for folder in crate::plugins::folders::load_folders(crate::plugins::folders::FOLDERS_FILE)
            .unwrap_or_else(|_| crate::plugins::folders::default_folders())
        {
            let (alias, alias_lc) = Self::normalize_alias(folder.alias);
            aliases.insert(folder.path.clone(), alias);
            aliases_lc.insert(folder.path, alias_lc);
        }
        (aliases, aliases_lc)
    }

    pub(crate) fn bookmark_alias_maps() -> (
        HashMap<String, Option<String>>,
        HashMap<String, Option<String>>,
    ) {
        let mut aliases = HashMap::new();
        let mut aliases_lc = HashMap::new();
        for bookmark in
            crate::plugins::bookmarks::load_bookmarks(crate::plugins::bookmarks::BOOKMARKS_FILE)
                .unwrap_or_default()
        {
            let (alias, alias_lc) = Self::normalize_alias(bookmark.alias);
            aliases.insert(bookmark.url.clone(), alias);
            aliases_lc.insert(bookmark.url, alias_lc);
        }
        (aliases, aliases_lc)
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
        let Some(subcommand) = parts.next().map(str::to_ascii_lowercase) else {
            return false;
        };

        // Only bypass launcher-side exact display filtering when the query is an
        // explicit plugin command whose plugin already returned resolved outputs.
        // Example: `note today` / `note search <term>` yielding `note:new:*` or
        // `note:open:*` actions; re-filtering those by label text can hide valid results.
        matches!(head.as_str(), "note" | "notes")
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
            && action.starts_with("note:")
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
        let mut entries: Vec<String> = Vec::new();
        entries.extend(self.command_cache.iter().map(|a| a.label.to_lowercase()));
        for a in self.actions.iter() {
            entries.push(format!("app {}", a.label.to_lowercase()));
        }
        entries.sort();
        entries.dedup();
        let mut builder = MapBuilder::memory();
        for (i, k) in entries.iter().enumerate() {
            if let Err(e) = builder.insert(k, i as u64) {
                tracing::warn!(key = %k, ?e, "failed to insert key into completion index");
            }
        }
        let map = Map::new(builder.into_inner().unwrap()).unwrap();
        self.completion_index = Some(map);
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
            let q = self.query.to_lowercase();
            let mut stream = index.range().ge(q.as_str()).into_stream();
            while let Some((k, _)) = stream.next() {
                let key = std::str::from_utf8(k).unwrap();
                if !key.starts_with(&q) {
                    break;
                }
                if key != q {
                    self.suggestions.push(key.to_string());
                }
                if self.suggestions.len() >= 5 {
                    break;
                }
            }
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

    pub fn search(&mut self) {
        if self.last_results_valid && self.query == self.last_search_query {
            self.selected = None;
            return;
        }

        let trimmed = self.query.trim();
        let trimmed_lc = trimmed.to_lowercase();
        self.last_timer_query =
            trimmed.starts_with("timer list") || trimmed.starts_with("alarm list");
        self.last_stopwatch_query = trimmed.starts_with("sw list");
        if trimmed.is_empty() {
            self.autocomplete_index = 0;
            self.suggestions.clear();
            let mut res = self.command_cache.clone();
            for a in self.actions.iter() {
                res.push(Action {
                    label: format!("app {}", a.label),
                    desc: a.desc.clone(),
                    action: a.action.clone(),
                    args: a.args.clone(),
                });
            }
            self.results = res;
            self.selected = None;
            self.recompute_query_results_layout();
            return;
        }

        let mut res: Vec<(Action, f32)> = Vec::new();

        let search_actions =
            trimmed_lc == APP_PREFIX || trimmed_lc.starts_with(&format!("{} ", APP_PREFIX));
        let action_query = if search_actions {
            if trimmed_lc == APP_PREFIX {
                "".to_string()
            } else {
                trimmed.split_once(' ').map(|x| x.1).unwrap_or("").to_string()
            }
        } else {
            String::new()
        };
        let action_query_lc = action_query.to_lowercase();

        if trimmed_lc.starts_with("g ") {
            res.extend(self.search_plugins(trimmed, &trimmed_lc));
        } else {
            if search_actions {
                res.extend(self.search_actions(&action_query, &action_query_lc));
            }
            res.extend(self.search_plugins(trimmed, &trimmed_lc));
        }

        self.apply_usage_weight(&mut res);

        res.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        self.results = res.into_iter().map(|(a, _)| a).collect();
        self.selected = None;
        self.last_search_query = self.query.clone();
        self.last_results_valid = true;
        self.update_suggestions();
        self.recompute_query_results_layout();
    }

    fn search_actions(&self, query: &str, _query_lc: &str) -> Vec<(Action, f32)> {
        let (filtered_query, filters) = split_action_filters(query);
        let filtered_query = filtered_query.trim();
        let filtered_query_lc = filtered_query.to_lowercase();
        let query = filtered_query;
        let query_lc = filtered_query_lc.as_str();

        let mut res = Vec::new();
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
        res
    }

    fn search_plugins(&self, trimmed: &str, trimmed_lc: &str) -> Vec<(Action, f32)> {
        let mut res = Vec::new();
        if trimmed_lc.starts_with("g ") {
            let filter = std::collections::HashSet::from(["web_search".to_string()]);
            let plugin_results = self.plugins.search_filtered(
                &self.query,
                Some(&filter),
                self.enabled_capabilities.as_ref(),
            );
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
                    let score = if self.query.is_empty() {
                        0.0
                    } else {
                        self.matcher
                            .fuzzy_match(&a.label, &self.query)
                            .max(self.matcher.fuzzy_match(&a.desc, &self.query))
                            .unwrap_or(0) as f32
                            * self.fuzzy_weight
                    };
                    res.push((a, score));
                }
            }
            return res;
        }

        let plugin_results = self.plugins.search_filtered(
            &self.query,
            self.enabled_plugins.as_ref(),
            self.enabled_capabilities.as_ref(),
        );

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
                    let score = if self.query.is_empty() {
                        0.0
                    } else {
                        self.matcher
                            .fuzzy_match(&a.label, &self.query)
                            .max(self.matcher.fuzzy_match(&a.desc, &self.query))
                            .unwrap_or(0) as f32
                            * self.fuzzy_weight
                    };
                    res.push((a, score));
                }
            }
        }

        res
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
    use crate::{plugin::PluginManager, settings::Settings};
    use eframe::egui;
    use std::sync::{atomic::AtomicBool, Arc};

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
        assert!(app
            .results
            .iter()
            .any(|action| action.action == "Action:ID"));

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
}
