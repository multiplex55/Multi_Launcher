use super::Action;
use crate::common::query::{ActionFilterMetadata, action_matches_filters, split_action_filters};
use fuzzy_matcher::FuzzyMatcher;
use std::collections::HashMap;

pub const APP_PREFIX: &str = "app";

/// Lowercase text cached for a single complete action during launcher search.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CachedSearchEntry {
    pub label_lc: String,
    pub desc_lc: String,
    pub action_lc: String,
}

impl CachedSearchEntry {
    pub fn from_action(action: &Action) -> Self {
        Self {
            label_lc: action.label.to_lowercase(),
            desc_lc: action.desc.to_lowercase(),
            action_lc: action.action.to_lowercase(),
        }
    }

    pub fn matches_exact_display_text(&self, query_lc: &str) -> bool {
        let query_lc = query_lc.trim();
        query_lc.is_empty() || self.label_lc.contains(query_lc)
    }

    /// Score one already selected candidate while borrowing its full action.
    /// Callers retain their own provider admission and result installation.
    pub fn score_candidate<M: FuzzyMatcher>(
        &self,
        action: &Action,
        query: &str,
        query_lc: &str,
        exact_match_mode: bool,
        alias_match: bool,
        matcher: &M,
        fuzzy_weight: f32,
    ) -> Option<f32> {
        if exact_match_mode {
            let matches = self.matches_exact_display_text(query_lc)
                || self.desc_lc.contains(query_lc)
                || self.action_lc.contains(query_lc)
                || alias_match;
            return matches.then_some(if alias_match { 1.0 } else { 0.0 });
        }

        matcher
            .fuzzy_match(&action.label, query)
            .max(matcher.fuzzy_match(&action.desc, query))
            .map(|score| score as f32 * fuzzy_weight)
    }
}

/// Borrowed local-action search data. The egui and GPUI adapters can use the
/// same collector while retaining ownership of their respective catalogs.
pub struct LocalActionSearchInput<'a, M> {
    pub actions: &'a [Action],
    pub cached_entries: &'a [CachedSearchEntry],
    pub filter_metadata: &'a [ActionFilterMetadata],
    pub folder_aliases_lc: &'a HashMap<String, Option<String>>,
    pub bookmark_aliases_lc: &'a HashMap<String, Option<String>>,
    pub matcher: &'a M,
    pub match_exact: bool,
    pub fuzzy_weight: f32,
    pub collect_search_stats: bool,
    pub collect_candidate_count: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LocalActionSearchStats {
    pub catalog_visits: usize,
    pub filter_passes: usize,
    pub score_attempts: usize,
    pub hits: usize,
}

pub struct LocalActionSearchResults {
    pub scored_actions: Vec<(Action, f32)>,
    pub stats: LocalActionSearchStats,
}

/// Search the borrowed local catalog with the launcher's existing filters,
/// alias precedence, exact-mode override and cached candidate scorer.
pub fn collect_local_candidates<M, F>(
    input: LocalActionSearchInput<'_, M>,
    query: &str,
    mut on_clone: F,
) -> LocalActionSearchResults
where
    M: FuzzyMatcher,
    F: FnMut(&Action),
{
    debug_assert_eq!(input.actions.len(), input.cached_entries.len());
    debug_assert_eq!(input.actions.len(), input.filter_metadata.len());

    let (filtered_query, filters) = split_action_filters(query);
    let filtered_query = filtered_query.trim();
    let filtered_query_lc = filtered_query.to_lowercase();
    let exact_match_mode = input.match_exact || input.fuzzy_weight <= 0.0;
    let mut stats = LocalActionSearchStats::default();
    let mut scored_actions = Vec::new();

    for (index, action) in input.actions.iter().enumerate() {
        if input.collect_search_stats {
            stats.catalog_visits = stats.catalog_visits.saturating_add(1);
        }
        let Some(metadata) = input.filter_metadata.get(index) else {
            continue;
        };
        if !action_matches_filters(metadata, &filters) {
            continue;
        }
        if input.collect_search_stats {
            stats.filter_passes = stats.filter_passes.saturating_add(1);
        }

        let score = if filtered_query.is_empty() {
            Some(0.0)
        } else {
            if input.collect_search_stats || input.collect_candidate_count {
                stats.score_attempts = stats.score_attempts.saturating_add(1);
            }
            let Some(cached) = input.cached_entries.get(index) else {
                continue;
            };
            let alias_match = exact_match_mode
                && alias_matches_lc(
                    &action.action,
                    &filtered_query_lc,
                    input.folder_aliases_lc,
                    input.bookmark_aliases_lc,
                );
            cached.score_candidate(
                action,
                filtered_query,
                &filtered_query_lc,
                exact_match_mode,
                alias_match,
                input.matcher,
                input.fuzzy_weight,
            )
        };

        if let Some(score) = score {
            on_clone(action);
            scored_actions.push((action.clone(), score));
        }
    }

    stats.hits = scored_actions.len();
    LocalActionSearchResults {
        scored_actions,
        stats,
    }
}

/// Parse the launcher-local action prefix with the same trim, case folding,
/// and literal-space behavior as the existing egui outcome composer.
pub fn parse_app_prefix(query: &str) -> Option<&str> {
    let trimmed = query.trim();
    let trimmed_lc = trimmed.to_lowercase();
    if trimmed_lc == APP_PREFIX || trimmed_lc.starts_with("app ") {
        Some(trimmed.split_once(' ').map(|value| value.1).unwrap_or(""))
    } else {
        None
    }
}

/// Inputs to the ordered, read-only launcher composition boundary.
pub struct ReadOnlySearchInput<'a, M> {
    pub raw_query: &'a str,
    pub command_cache: &'a [Action],
    pub local_actions: &'a [Action],
    pub local_cached_entries: &'a [CachedSearchEntry],
    pub local_filter_metadata: &'a [ActionFilterMetadata],
    pub folder_aliases_lc: &'a HashMap<String, Option<String>>,
    pub bookmark_aliases_lc: &'a HashMap<String, Option<String>>,
    pub matcher: &'a M,
    pub match_exact: bool,
    pub fuzzy_weight: f32,
    pub collect_search_stats: bool,
    pub collect_candidate_count: bool,
    pub usage: &'a HashMap<String, u32>,
    pub usage_weight: f32,
    pub provider_results: Vec<(Action, f32)>,
}

/// Optional adapter-owned instrumentation at the scoring, sorting and final
/// materialization boundaries. Provider revisions and pending state stay out
/// of this neutral composition API.
pub trait ReadOnlySearchObserver {
    fn local_search_started(&mut self) {}
    fn local_search_completed(&mut self, _stats: LocalActionSearchStats) {}
    fn measure_sort(&self) -> bool {
        false
    }
    fn sort_completed(&mut self, _elapsed: std::time::Duration) {}
    fn materialization_started(&mut self) {}
    fn materialization_completed(&mut self, _entries_moved: usize) {}
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReadOnlySearchStats {
    pub local: LocalActionSearchStats,
    pub local_search_ran: bool,
    pub provider_actions_retained_moved: usize,
    pub sort_input_len: usize,
    pub final_scored_entries_moved: usize,
    pub empty_browse_command_cache_cloned: usize,
    pub empty_browse_catalog_materialized: usize,
}

pub struct ReadOnlySearchResults {
    pub actions: Vec<Action>,
    pub stats: ReadOnlySearchStats,
}

/// Compose local and already-admitted provider candidates in the established
/// order, then apply usage weighting and a stable sort while moving complete
/// Actions through to the result list.
pub fn compose_read_only_results<M, F>(
    input: ReadOnlySearchInput<'_, M>,
    mut on_local_clone: F,
    mut observer: Option<&mut dyn ReadOnlySearchObserver>,
) -> ReadOnlySearchResults
where
    M: FuzzyMatcher,
    F: FnMut(&Action),
{
    let trimmed = input.raw_query.trim();
    if trimmed.is_empty() {
        let mut actions = input.command_cache.to_vec();
        actions.extend(input.local_actions.iter().map(|action| Action {
            label: format!("{APP_PREFIX} {}", action.label),
            desc: action.desc.clone(),
            action: action.action.clone(),
            args: action.args.clone(),
        }));
        return ReadOnlySearchResults {
            actions,
            stats: ReadOnlySearchStats {
                empty_browse_command_cache_cloned: input.command_cache.len(),
                empty_browse_catalog_materialized: input.local_actions.len(),
                ..ReadOnlySearchStats::default()
            },
        };
    }

    let trimmed_lc = trimmed.to_lowercase();
    let app_query = parse_app_prefix(trimmed);
    let local_search_ran = !trimmed_lc.starts_with("g ") && app_query.is_some();
    let local_search = if local_search_ran {
        if let Some(observer) = observer.as_deref_mut() {
            observer.local_search_started();
        }
        let local_search = collect_local_candidates(
            LocalActionSearchInput {
                actions: input.local_actions,
                cached_entries: input.local_cached_entries,
                filter_metadata: input.local_filter_metadata,
                folder_aliases_lc: input.folder_aliases_lc,
                bookmark_aliases_lc: input.bookmark_aliases_lc,
                matcher: input.matcher,
                match_exact: input.match_exact,
                fuzzy_weight: input.fuzzy_weight,
                collect_search_stats: input.collect_search_stats,
                collect_candidate_count: input.collect_candidate_count,
            },
            app_query.unwrap_or_default(),
            &mut on_local_clone,
        );
        if let Some(observer) = observer.as_deref_mut() {
            observer.local_search_completed(local_search.stats);
        }
        local_search
    } else {
        LocalActionSearchResults {
            scored_actions: Vec::new(),
            stats: LocalActionSearchStats::default(),
        }
    };

    let provider_actions_retained_moved = input.provider_results.len();
    let mut scored = local_search.scored_actions;
    scored.extend(input.provider_results);
    let sort_input_len = scored.len();
    for (action, score) in &mut scored {
        *score += input.usage.get(&action.action).copied().unwrap_or(0) as f32 * input.usage_weight;
    }
    let sort_started = observer
        .as_deref()
        .is_some_and(|observer| observer.measure_sort())
        .then(std::time::Instant::now);
    scored.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if let (Some(observer), Some(sort_started)) = (observer.as_deref_mut(), sort_started) {
        observer.sort_completed(sort_started.elapsed());
    }
    let final_scored_entries_moved = scored.len();
    if let Some(observer) = observer.as_deref_mut() {
        observer.materialization_started();
    }
    let actions: Vec<Action> = scored.into_iter().map(|(action, _)| action).collect();
    if let Some(observer) = observer.as_deref_mut() {
        observer.materialization_completed(final_scored_entries_moved);
    }

    ReadOnlySearchResults {
        actions,
        stats: ReadOnlySearchStats {
            local: local_search.stats,
            local_search_ran,
            provider_actions_retained_moved,
            sort_input_len,
            final_scored_entries_moved,
            ..ReadOnlySearchStats::default()
        },
    }
}

/// Folder entries shadow bookmark aliases with the same action ID, including
/// an explicit folder entry whose alias is absent.
pub fn alias_matches_lc(
    action: &str,
    query_lc: &str,
    folder_aliases_lc: &HashMap<String, Option<String>>,
    bookmark_aliases_lc: &HashMap<String, Option<String>>,
) -> bool {
    folder_aliases_lc
        .get(action)
        .or_else(|| bookmark_aliases_lc.get(action))
        .and_then(Option::as_ref)
        .is_some_and(|alias| alias.contains(query_lc))
}

#[cfg(test)]
mod tests {
    use super::{
        CachedSearchEntry, ReadOnlySearchInput, alias_matches_lc, compose_read_only_results,
        parse_app_prefix,
    };
    use crate::{actions::Action, common::query::ActionFilterMetadata};
    use fuzzy_matcher::skim::SkimMatcherV2;
    use std::collections::HashMap;

    fn action(label: &str, args: &str) -> Action {
        Action {
            label: label.into(),
            desc: "Hash generator".into(),
            action: "hash:open".into(),
            args: Some(args.into()),
        }
    }

    #[test]
    fn app_prefix_keeps_literal_space_and_case_normalization() {
        assert_eq!(parse_app_prefix(" APP QMarker "), Some("QMarker"));
        assert_eq!(parse_app_prefix("app  QMarker"), Some(" QMarker"));
        assert_eq!(parse_app_prefix("app"), Some(""));
        assert_eq!(parse_app_prefix("app\tQMarker"), None);
        assert_eq!(parse_app_prefix("myapp QMarker"), None);
    }

    #[test]
    fn read_only_composition_preserves_duplicate_route_actions_and_stable_order() {
        let alpha = action("Alpha QMarker", "alpha argument");
        let beta = action("Beta QMarker", "beta argument");
        let provider = Action {
            label: "Provider route".into(),
            desc: "Hash generator".into(),
            action: "hash:open".into(),
            args: Some("provider argument".into()),
        };
        let actions = vec![alpha.clone(), beta.clone()];
        let cached = actions
            .iter()
            .map(CachedSearchEntry::from_action)
            .collect::<Vec<_>>();
        let metadata = actions
            .iter()
            .map(ActionFilterMetadata::from_action)
            .collect::<Vec<_>>();
        let matcher = SkimMatcherV2::default();
        let empty_aliases = HashMap::new();
        let output = compose_read_only_results(
            ReadOnlySearchInput {
                raw_query: "app QMarker kind:hash id:hash:open",
                command_cache: &[],
                local_actions: &actions,
                local_cached_entries: &cached,
                local_filter_metadata: &metadata,
                folder_aliases_lc: &empty_aliases,
                bookmark_aliases_lc: &empty_aliases,
                matcher: &matcher,
                match_exact: true,
                fuzzy_weight: 1.0,
                collect_search_stats: true,
                collect_candidate_count: false,
                usage: &HashMap::new(),
                usage_weight: 0.0,
                provider_results: vec![(provider.clone(), 0.0)],
            },
            |_| {},
            None,
        );

        assert_eq!(output.actions, vec![alpha, beta, provider]);
        assert_eq!(output.stats.local.hits, 2);
        assert_eq!(output.stats.provider_actions_retained_moved, 1);
    }

    #[test]
    fn read_only_composition_preserves_empty_browse_defaults() {
        let command = Action {
            label: "open settings".into(),
            desc: "Settings".into(),
            action: "query:settings".into(),
            args: None,
        };
        let local = action("Alpha QMarker", "alpha argument");
        let cached = vec![CachedSearchEntry::from_action(&local)];
        let metadata = vec![ActionFilterMetadata::from_action(&local)];
        let matcher = SkimMatcherV2::default();
        let aliases = HashMap::new();
        let empty = compose_read_only_results(
            ReadOnlySearchInput {
                raw_query: " \t",
                command_cache: std::slice::from_ref(&command),
                local_actions: std::slice::from_ref(&local),
                local_cached_entries: &cached,
                local_filter_metadata: &metadata,
                folder_aliases_lc: &aliases,
                bookmark_aliases_lc: &aliases,
                matcher: &matcher,
                match_exact: false,
                fuzzy_weight: 0.5,
                collect_search_stats: false,
                collect_candidate_count: false,
                usage: &HashMap::new(),
                usage_weight: 0.25,
                provider_results: vec![(local.clone(), 0.0)],
            },
            |_| {},
            None,
        );
        assert_eq!(empty.actions.len(), 2);
        assert_eq!(empty.actions[0], command);
        assert_eq!(empty.actions[1].label, "app Alpha QMarker");
        assert_eq!(empty.actions[1].args.as_deref(), Some("alpha argument"));

        let no_results = compose_read_only_results(
            ReadOnlySearchInput {
                raw_query: "app no match",
                command_cache: &[],
                local_actions: std::slice::from_ref(&local),
                local_cached_entries: &cached,
                local_filter_metadata: &metadata,
                folder_aliases_lc: &aliases,
                bookmark_aliases_lc: &aliases,
                matcher: &matcher,
                match_exact: true,
                fuzzy_weight: 1.0,
                collect_search_stats: false,
                collect_candidate_count: false,
                usage: &HashMap::new(),
                usage_weight: 0.0,
                provider_results: Vec::new(),
            },
            |_| {},
            None,
        );
        assert!(no_results.actions.is_empty());
    }

    #[test]
    fn folder_without_alias_shadows_bookmark_alias_for_the_same_action() {
        let folder_aliases = HashMap::from([("shared-action".to_owned(), None)]);
        let bookmark_aliases = HashMap::from([(
            "shared-action".to_owned(),
            Some("useful bookmark alias".to_owned()),
        )]);

        assert!(alias_matches_lc(
            "shared-action",
            "useful",
            &HashMap::new(),
            &bookmark_aliases,
        ));
        assert!(!alias_matches_lc(
            "shared-action",
            "useful",
            &folder_aliases,
            &bookmark_aliases,
        ));
    }
}
