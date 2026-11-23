use std::collections::HashSet;

use crate::{
    common::query::split_action_filters,
    plugin::{CAP_FORCE_LIST_RESULTS, CAP_GRID_RESULTS_COMPATIBLE},
    settings::QueryResultsLayoutSettings,
};

/// The resolved layout for one live query. `columns` stays normalized even in
/// list mode so renderers can use one value when building their result view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedQueryResultsLayout {
    pub grid: bool,
    pub columns: usize,
}

impl Default for ResolvedQueryResultsLayout {
    fn default() -> Self {
        Self::new(false, 1)
    }
}

impl ResolvedQueryResultsLayout {
    pub fn new(grid: bool, columns: usize) -> Self {
        Self {
            grid,
            columns: columns.max(1),
        }
    }

    /// Return the direct child in the results viewport that contains an item.
    /// Grid renderers group each visual row as one child; list renderers use
    /// one child per result.
    pub fn scroll_child_for(self, item_index: usize) -> usize {
        if self.grid {
            item_index / self.columns.max(1)
        } else {
            item_index
        }
    }

    pub fn row_count(self, result_count: usize) -> usize {
        if self.grid {
            result_count.div_ceil(self.columns.max(1))
        } else {
            result_count
        }
    }
}

/// The grid-relevant metadata exposed by an installed plugin.
#[derive(Clone, Copy, Debug)]
pub struct QueryLayoutPlugin<'a> {
    pub name: &'a str,
    pub query_prefixes: &'a [&'a str],
    pub capabilities: &'a [&'a str],
}

/// Resolve the query's list/grid policy using the launcher's existing plugin
/// prefix and capability rules. `metadata` only borrows from the current
/// iterator item; the resolver retains owned policy flags, never plugin guards.
pub fn resolve_query_results_layout<P>(
    query: &str,
    settings: &QueryResultsLayoutSettings,
    enabled_plugins: Option<&HashSet<String>>,
    plugins: impl IntoIterator<Item = P>,
    plugin_name: impl for<'a> Fn(&'a P) -> &'a str,
    metadata: impl for<'a> Fn(&'a P) -> QueryLayoutPlugin<'a>,
) -> ResolvedQueryResultsLayout {
    let columns = settings.cols.max(1);
    if !settings.enabled {
        return ResolvedQueryResultsLayout::new(false, columns);
    }

    // Global grid mode when capability gating is disabled.
    if !settings.respect_plugin_capability {
        return ResolvedQueryResultsLayout::new(true, columns);
    }

    let trimmed = query.trim();
    if trimmed.is_empty() {
        return ResolvedQueryResultsLayout::new(false, columns);
    }

    let (filtered_query, _) = split_action_filters(trimmed);
    let query_head = filtered_query
        .split_whitespace()
        .next()
        .map(str::to_ascii_lowercase);

    let Some(head) = query_head.as_deref() else {
        // A nonblank query made only from action filters has no plugin head,
        // so it uses the configured non-prefixed layout just like egui.
        return ResolvedQueryResultsLayout::new(true, columns);
    };

    let mut matching_plugins = 0;
    let mut matching_plugin_opted_out = false;
    let mut matching_plugin_forces_list = false;
    let mut matching_plugin_supports_grid = false;
    for plugin in plugins {
        if enabled_plugins.is_some_and(|enabled| !enabled.contains(plugin_name(&plugin))) {
            continue;
        }
        let plugin = metadata(&plugin);

        if plugin.query_prefixes.is_empty()
            || !plugin
                .query_prefixes
                .iter()
                .any(|prefix| prefix.eq_ignore_ascii_case(head))
        {
            continue;
        }

        matching_plugins += 1;
        if matching_plugins > 1 {
            // Ambiguous/mixed plugin context: safely fall back to list mode.
            return ResolvedQueryResultsLayout::new(false, columns);
        }

        matching_plugin_opted_out = settings
            .plugin_opt_out
            .iter()
            .any(|name| name.eq_ignore_ascii_case(plugin.name));
        matching_plugin_forces_list = plugin.capabilities.contains(&CAP_FORCE_LIST_RESULTS);
        matching_plugin_supports_grid = plugin.capabilities.contains(&CAP_GRID_RESULTS_COMPATIBLE);
    }

    if matching_plugins == 0 {
        // No plugin-prefixed context: use the configured grid layout.
        return ResolvedQueryResultsLayout::new(true, columns);
    }

    if matching_plugin_opted_out || matching_plugin_forces_list {
        return ResolvedQueryResultsLayout::new(false, columns);
    }

    ResolvedQueryResultsLayout::new(matching_plugin_supports_grid, columns)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuerySelectionMove {
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
}

/// Apply one launcher result-selection move. Event ownership remains with the
/// frontend adapter so it can preserve editor behavior for list-mode lateral
/// keys and empty result sets.
pub fn move_query_selection(
    selected: Option<usize>,
    result_count: usize,
    layout: ResolvedQueryResultsLayout,
    page_jump: usize,
    movement: QuerySelectionMove,
) -> Option<usize> {
    if result_count == 0 {
        return selected;
    }

    let max = result_count - 1;
    let columns = layout.columns.max(1);
    match movement {
        QuerySelectionMove::Down => match selected {
            Some(index) if layout.grid => Some(index.saturating_add(columns).min(max)),
            Some(index) if index < max => Some(index + 1),
            Some(index) => Some(index),
            None => Some(0),
        },
        QuerySelectionMove::Up => match selected {
            Some(0) | None => None,
            Some(index) if layout.grid => Some(index.saturating_sub(columns).min(max)),
            Some(index) => Some(index - 1),
        },
        QuerySelectionMove::Left if layout.grid => match selected {
            Some(index) if index > 0 => Some(index - 1),
            Some(index) => Some(index),
            None => Some(0),
        },
        QuerySelectionMove::Right if layout.grid => match selected {
            Some(index) if index < max => Some(index + 1),
            Some(index) => Some(index),
            None => Some(0),
        },
        QuerySelectionMove::Left | QuerySelectionMove::Right => selected,
        QuerySelectionMove::PageDown => match selected {
            Some(index) => Some(index.saturating_add(page_jump).min(max)),
            None => Some(0),
        },
        QuerySelectionMove::PageUp => match selected {
            Some(index) => Some(index.saturating_sub(page_jump).min(max)),
            None => Some(0),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRID_CAPABILITIES: &[&str] = &[CAP_GRID_RESULTS_COMPATIBLE];
    const SEARCH_CAPABILITIES: &[&str] = &["search"];
    const FORCE_LIST_CAPABILITIES: &[&str] = &[CAP_FORCE_LIST_RESULTS];
    const NOTE_PREFIXES: &[&str] = &["note"];
    const HASH_PREFIXES: &[&str] = &["hash"];

    fn plugin<'a>(
        name: &'a str,
        query_prefixes: &'a [&'a str],
        capabilities: &'a [&'a str],
    ) -> QueryLayoutPlugin<'a> {
        QueryLayoutPlugin {
            name,
            query_prefixes,
            capabilities,
        }
    }

    fn resolve_layout<'a>(
        query: &str,
        settings: &QueryResultsLayoutSettings,
        enabled_plugins: Option<&HashSet<String>>,
        plugins: impl IntoIterator<Item = QueryLayoutPlugin<'a>>,
    ) -> ResolvedQueryResultsLayout {
        resolve_query_results_layout(
            query,
            settings,
            enabled_plugins,
            plugins,
            |plugin| plugin.name,
            |plugin| *plugin,
        )
    }

    #[test]
    fn resolved_layout_preserves_global_plugin_and_ambiguity_rules() {
        let mut settings = QueryResultsLayoutSettings {
            enabled: true,
            cols: 3,
            ..QueryResultsLayoutSettings::default()
        };
        let plugins = [plugin("note", NOTE_PREFIXES, GRID_CAPABILITIES)];

        assert!(resolve_layout("note text", &settings, None, plugins).grid);

        settings.plugin_opt_out = vec!["NOTE".into()];
        assert!(!resolve_layout("note text", &settings, None, plugins).grid);
        settings.plugin_opt_out.clear();

        let forced_list = [plugin("note", NOTE_PREFIXES, FORCE_LIST_CAPABILITIES)];
        assert!(!resolve_layout("note text", &settings, None, forced_list).grid);

        let ambiguous = [
            plugin("note", NOTE_PREFIXES, GRID_CAPABILITIES),
            plugin("note_alias", NOTE_PREFIXES, GRID_CAPABILITIES),
        ];
        assert!(!resolve_layout("note", &settings, None, ambiguous).grid);

        settings.respect_plugin_capability = false;
        let list_only_hash = [plugin("hash_generator", HASH_PREFIXES, SEARCH_CAPABILITIES)];
        assert!(resolve_layout("hash", &settings, None, list_only_hash).grid);
    }

    #[test]
    fn resolved_layout_uses_enabled_plugin_prefixes_and_action_filter_stripping() {
        let settings = QueryResultsLayoutSettings {
            enabled: true,
            cols: 0,
            ..QueryResultsLayoutSettings::default()
        };
        let plugin = plugin("note", NOTE_PREFIXES, GRID_CAPABILITIES);
        let enabled = HashSet::from(["note".to_owned()]);
        let layout = resolve_layout("kind:plugin note text", &settings, Some(&enabled), [plugin]);
        assert!(layout.grid);
        assert_eq!(layout.columns, 1);

        assert!(resolve_layout("kind:plugin", &settings, None, [plugin]).grid);

        let disabled = HashSet::new();
        assert!(resolve_layout("note text", &settings, Some(&disabled), [plugin]).grid);
    }

    #[test]
    fn selection_moves_preserve_grid_stride_horizontal_order_and_page_endpoints() {
        let grid = ResolvedQueryResultsLayout::new(true, 3);
        let move_to = |selected, movement| move_query_selection(selected, 7, grid, 2, movement);

        assert_eq!(move_to(None, QuerySelectionMove::Down), Some(0));
        assert_eq!(move_to(Some(0), QuerySelectionMove::Down), Some(3));
        assert_eq!(move_to(Some(3), QuerySelectionMove::Right), Some(4));
        assert_eq!(move_to(Some(4), QuerySelectionMove::Up), Some(1));
        assert_eq!(move_to(Some(4), QuerySelectionMove::PageDown), Some(6));
        assert_eq!(move_to(Some(6), QuerySelectionMove::PageDown), Some(6));
        assert_eq!(move_to(Some(6), QuerySelectionMove::PageUp), Some(4));
        assert_eq!(move_to(None, QuerySelectionMove::PageUp), Some(0));
        assert_eq!(move_to(Some(6), QuerySelectionMove::Down), Some(6));
        assert_eq!(
            move_query_selection(Some(2), 0, grid, 2, QuerySelectionMove::Down),
            Some(2)
        );
    }

    #[test]
    fn list_lateral_selection_is_unchanged_for_the_editor_adapter() {
        let list = ResolvedQueryResultsLayout::new(false, 3);
        assert_eq!(
            move_query_selection(Some(2), 7, list, 2, QuerySelectionMove::Left),
            Some(2)
        );
        assert_eq!(
            move_query_selection(Some(2), 7, list, 2, QuerySelectionMove::Right),
            Some(2)
        );
        assert_eq!(
            move_query_selection(None, 7, list, 2, QuerySelectionMove::Right),
            None
        );
        assert_eq!(list.scroll_child_for(6), 6);
    }

    #[test]
    fn grid_scroll_rows_include_a_partial_final_row() {
        let grid = ResolvedQueryResultsLayout::new(true, 3);
        assert_eq!(grid.row_count(7), 3);
        assert_eq!(grid.scroll_child_for(6), 2);
        assert_eq!(grid.scroll_child_for(5), 1);
    }

    #[test]
    fn selection_moves_preserve_existing_stale_index_transitions() {
        let grid = ResolvedQueryResultsLayout::new(true, 3);
        let list = ResolvedQueryResultsLayout::new(false, 3);

        assert_eq!(
            move_query_selection(Some(99), 7, list, 2, QuerySelectionMove::Down),
            Some(99)
        );
        assert_eq!(
            move_query_selection(Some(99), 7, list, 2, QuerySelectionMove::Up),
            Some(98)
        );
        assert_eq!(
            move_query_selection(Some(99), 7, grid, 2, QuerySelectionMove::Left),
            Some(98)
        );
        assert_eq!(
            move_query_selection(Some(99), 7, grid, 2, QuerySelectionMove::Right),
            Some(99)
        );
        assert_eq!(
            move_query_selection(Some(99), 7, grid, 2, QuerySelectionMove::Down),
            Some(6)
        );
        assert_eq!(
            move_query_selection(Some(99), 7, list, 2, QuerySelectionMove::PageDown),
            Some(6)
        );
        assert_eq!(
            move_query_selection(Some(99), 0, grid, 2, QuerySelectionMove::Down),
            Some(99)
        );
    }
}
