//! Scoped transient Settings search and navigation to mounted controls.
use super::*;
use multi_launcher::settings_editor::catalog::{SettingsCatalog, ids};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Target {
    Autocomplete,
    Exact,
    Fuzzy,
    Usage,
    Page,
    Grid,
    Columns,
    Respect,
    ClearQuery,
    HideWindow,
    Toasts,
    Duration,
    Inline,
    ErrorToasts,
    FollowMouse,
    StaticPosition,
    StaticX,
    StaticY,
    StaticWidth,
    StaticHeight,
}
impl Target {
    fn from_id(id: &str) -> Option<Self> {
        Some(match id {
            ids::SEARCH_AUTOCOMPLETE => Self::Autocomplete,
            ids::SEARCH_MATCH_EXACT => Self::Exact,
            ids::SEARCH_FUZZY_WEIGHT | ids::SEARCH_FUZZY_WEIGHT_VALUE => Self::Fuzzy,
            ids::SEARCH_USAGE_WEIGHT | ids::SEARCH_USAGE_WEIGHT_VALUE => Self::Usage,
            "search.page_jump" => Self::Page,
            ids::SEARCH_GRID_LAYOUT => Self::Grid,
            "search.grid_columns" => Self::Columns,
            ids::SEARCH_RESPECT_PLUGIN_CAPABILITY => Self::Respect,
            ids::ACTIONS_CLEAR_QUERY => Self::ClearQuery,
            ids::ACTIONS_HIDE_AFTER_RUN => Self::HideWindow,
            ids::FEEDBACK_TOASTS => Self::Toasts,
            ids::FEEDBACK_TOAST_DURATION | ids::FEEDBACK_TOAST_DURATION_VALUE => Self::Duration,
            ids::FEEDBACK_INLINE_ERRORS => Self::Inline,
            ids::FEEDBACK_ERROR_TOASTS => Self::ErrorToasts,
            ids::LAUNCHER_FOLLOW_MOUSE => Self::FollowMouse,
            ids::LAUNCHER_STATIC_POSITION => Self::StaticPosition,
            "launcher.static_x" => Self::StaticX,
            "launcher.static_y" => Self::StaticY,
            "launcher.static_width" => Self::StaticWidth,
            "launcher.static_height" => Self::StaticHeight,
            _ => return None,
        })
    }
    fn search_section(self) -> bool {
        matches!(
            self,
            Self::Autocomplete
                | Self::Exact
                | Self::Fuzzy
                | Self::Usage
                | Self::Page
                | Self::Grid
                | Self::Columns
                | Self::Respect
                | Self::FollowMouse | Self::StaticPosition | Self::StaticX | Self::StaticY | Self::StaticWidth | Self::StaticHeight
        )
    }
}

pub(super) fn current_provider(
    query: &str,
    action: &Action,
    enabled: Option<&HashSet<String>>,
    caps: Option<&HashMap<String, Vec<String>>>,
) -> Option<String> {
    let Ok(Command::Dialog(DialogCommand::SettingsSearch { query: term })) = parse_action(action)
    else {
        return None;
    };
    let plugin = multi_launcher::plugins::settings::SettingsPlugin;
    if !search_providers::search_routed(
        [&plugin as &dyn multi_launcher::plugin::Plugin],
        query,
        enabled,
        caps,
    )
    .into_iter()
    .any(|(name, candidate)| name == "settings" && candidate == *action)
    {
        return None;
    }
    Some(term)
}

fn navigation_receipt_current(
    issued: &utility_mount::MountReceipt,
    current: &utility_mount::MountReceipt,
    issued_epoch: u64,
    current_epoch: u64,
    issued_query: &str,
    current_query: &str,
) -> bool {
    issued.is_current()
        && issued.matches(current)
        && issued_epoch == current_epoch
        && issued_query == current_query
}

pub(super) struct SettingsNavigation {
    root: WeakEntity<RootWindow>,
    owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    input: Entity<InputState>,
    _sub: Subscription,
    _watchers: Vec<Subscription>,
    query_epoch: u64,
    enter_latched: bool,
    focus: FocusHandle,
    results_focus: HashMap<Target, FocusHandle>,
    catalog: SettingsCatalog,
}
impl SettingsNavigation {
    pub(super) fn new(
        root: WeakEntity<RootWindow>,
        search: Option<Entity<settings_search::SearchSettings>>,
        actions: Option<Entity<settings_actions::ActionsSettings>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let owner = utility_mount::MountOwner::default();
        let mount = owner.attach();
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search settings"));
        let sub = cx.subscribe(&input, |view: &mut Self, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                view.query_epoch = view.query_epoch.wrapping_add(1);
                cx.notify();
            }
        });
        let mut watchers = Vec::new();
        if let Some(search) = search {
            watchers.push(cx.observe(&search, |_: &mut Self, _, cx| cx.notify()));
        }
        if let Some(actions) = actions {
            watchers.push(cx.observe(&actions, |_: &mut Self, _, cx| cx.notify()));
        }
        Self {
            root,
            owner,
            mount,
            input,
            _sub: sub,
            _watchers: watchers,
            query_epoch: 0,
            enter_latched: false,
            focus: cx.focus_handle(),
            results_focus: HashMap::new(),
            catalog: SettingsCatalog::default(),
        }
    }
    pub(super) fn retire(&mut self) {
        self.owner.retire();
    }
    pub(super) fn set_query(&mut self, query: String, window: &mut Window, cx: &mut Context<Self>) {
        if !self.mount.is_current() {
            return;
        }
        self.query_epoch = self.query_epoch.wrapping_add(1);
        self.input
            .update(cx, |input, cx| input.set_value(query, window, cx));
        self.input.focus_handle(cx).focus(window, cx);
        cx.notify();
    }
    pub(super) fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.focus.contains_focused(window, cx) || self.input.read(cx).value().trim().is_empty()
        {
            return false;
        }
        let filter_focus = self.input.focus_handle(cx);
        let result_owned_focus = !filter_focus.is_focused(window);
        self.query_epoch = self.query_epoch.wrapping_add(1);
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
        if result_owned_focus {
            filter_focus.focus(window, cx);
        }
        cx.notify();
        true
    }
    pub(super) fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.query_epoch = self.query_epoch.wrapping_add(1);
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }
    pub(super) fn release_enter(&mut self) {
        self.enter_latched = false;
    }
    fn submit_first(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.input.focus_handle(cx).is_focused(window) {
            return false;
        }
        if self.enter_latched {
            return true;
        }
        self.enter_latched = true;
        let _ = self
            .root
            .update(cx, |root, _| root.pane_enter_latched = true);
        let Some((target, _)) = self.matches(cx).into_iter().next() else {
            return true;
        };
        let root = self.root.clone();
        let weak = cx.entity().downgrade();
        let issued = self.mount.clone();
        let epoch = self.query_epoch;
        let query = self.input.read(cx).value().to_string();
        let focus = self.input.focus_handle(cx);
        window.defer(cx, move |window, app| {
            navigate(root, weak, issued, epoch, query, target, focus, window, app)
        });
        true
    }

    fn matches(&self, cx: &App) -> Vec<(Target, String)> {
        let Some(root) = self.root.upgrade() else {
            return Vec::new();
        };
        let root = root.read(cx);
        let query = self.input.read(cx).value();
        let mut seen = HashSet::new();
        self.catalog
            .search(query.as_str())
            .into_iter()
            .filter_map(|entry| {
                let target = Target::from_id(entry.id.as_str())?;
                let available = if target.search_section() {
                    root.settings_search
                        .as_ref()
                        .is_some_and(|view| view.read(cx).can_focus(target))
                } else {
                    root.settings_actions
                        .as_ref()
                        .is_some_and(|view| view.read(cx).can_focus(target))
                };
                (available && seen.insert(target)).then(|| (target, entry.label.clone()))
            })
            .collect()
    }
}

fn navigate(
    root: WeakEntity<RootWindow>,
    weak: WeakEntity<SettingsNavigation>,
    issued: utility_mount::MountReceipt,
    epoch: u64,
    expected_query: String,
    target: Target,
    focus: FocusHandle,
    window: &mut Window,
    app: &mut App,
) {
    let Some(nav) = weak.upgrade() else {
        return;
    };
    if window.has_active_dialog(app) || !window.is_window_active() {
        return;
    }
    let Some(root_entity) = root.upgrade() else {
        return;
    };
    let nav_state = nav.read(app);
    if !navigation_receipt_current(
        &issued,
        &nav_state.mount,
        epoch,
        nav_state.query_epoch,
        &expected_query,
        nav_state.input.read(app).value().as_str(),
    ) || !nav_state
        .catalog
        .search(&expected_query)
        .iter()
        .any(|entry| Target::from_id(entry.id.as_str()) == Some(target))
    {
        return;
    }
    let state = root_entity.read(app);
    let target_live = if target.search_section() {
        state
            .settings_search
            .as_ref()
            .is_some_and(|view| view.read(app).can_focus(target))
    } else {
        state
            .settings_actions
            .as_ref()
            .is_some_and(|view| view.read(app).can_focus(target))
    };
    if !target_live
        || !issued.is_current()
        || !state.settings_editor.open
        || state.restart_admission.is_fenced()
        || state
            .root_window
            .is_none_or(|host| host.window_id() != window.window_handle().window_id())
        || !state.root_visibility.visible.load(Ordering::Acquire)
        || !state.root_visibility.active.load(Ordering::Acquire)
        || state
            .settings_navigation
            .as_ref()
            .is_none_or(|current| current.entity_id() != nav.entity_id())
        || !state.external_owner_is_current(window.focused(app).as_ref(), window, app)
        || state
            .native_window
            .is_none_or(|native| current_foreground_identity() != Some(native.identity))
    {
        return;
    }
    focus.focus(window, app);
    window.defer(app, move |window, app| {
        let Some(nav) = weak.upgrade() else {
            return;
        };
        let _ = root.update(app, |root, cx| {
            let valid = root.settings_editor.open
                && root
                    .settings_navigation
                    .as_ref()
                    .is_some_and(|current| current.entity_id() == nav.entity_id())
                && root
                    .root_window
                    .is_some_and(|host| host.window_id() == window.window_handle().window_id())
                && !root.restart_admission.is_fenced()
                && !window.has_active_dialog(cx)
                && window.is_window_active()
                && root
                    .native_window
                    .is_some_and(|native| current_foreground_identity() == Some(native.identity))
                && root.external_owner_is_current(Some(&focus), window, cx);
            if !valid {
                return;
            }
            let state = nav.read(cx);
            if !navigation_receipt_current(
                &issued,
                &state.mount,
                epoch,
                state.query_epoch,
                &expected_query,
                state.input.read(cx).value().as_str(),
            ) || !state
                .catalog
                .search(&expected_query)
                .iter()
                .any(|entry| Target::from_id(entry.id.as_str()) == Some(target))
            {
                return;
            }
            let focused = if target.search_section() {
                root.settings_search.as_ref().is_some_and(|view| {
                    view.update(cx, |view, cx| view.focus_control(target, window, cx))
                })
            } else {
                root.settings_actions.as_ref().is_some_and(|view| {
                    view.update(cx, |view, cx| view.focus_control(target, window, cx))
                })
            };
            if focused {
                root.settings_panel_scroll
                    .scroll_to_top_of_item(if target.search_section() { 2 } else { 3 });
                cx.notify();
            }
        });
    });
}

impl Render for SettingsNavigation {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let matches = self.matches(cx);
        for (target, _) in &matches {
            self.results_focus
                .entry(*target)
                .or_insert_with(|| cx.focus_handle());
        }
        let epoch = self.query_epoch;
        let query = self.input.read(cx).value().to_string();
        let mut panel = div()
            .id("settings-navigation")
            .track_focus(&self.focus)
            .flex()
            .flex_col()
            .gap_2()
            .capture_action(cx.listener(
                |view, action: &gpui_kit::base::input::Enter, window, cx| {
                    if !action.secondary && !action.shift && view.submit_first(window, cx) {
                        cx.stop_propagation();
                    }
                },
            ))
            .capture_key_down(cx.listener(|view, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key != "enter"
                    || event.keystroke.modifiers != Modifiers::default()
                    || !view.input.focus_handle(cx).is_focused(window)
                {
                    return;
                }
                window.prevent_default();
                cx.stop_propagation();
                if !event.is_held {
                    view.submit_first(window, cx);
                }
            }))
            .child(Input::new(&self.input).aria_label("Search settings"));
        if !query.trim().is_empty() && matches.is_empty() {
            panel = panel.child(Label::new("No available settings match."));
        }
        for (target, label) in matches.into_iter().take(16) {
            let Some(focus) = self.results_focus.get(&target).cloned() else {
                continue;
            };
            let issued = self.mount.clone();
            let expected_query = query.clone();
            let weak = cx.entity().downgrade();
            let root = self.root.clone();
            panel = panel.child(
                Button::new(SharedString::from(format!("settings-result-{target:?}")))
                    .label(label)
                    .tab_stop(true)
                    .track_focus(&focus)
                    .on_click(move |_, window, app| {
                        navigate(
                            root.clone(),
                            weak.clone(),
                            issued.clone(),
                            epoch,
                            expected_query.clone(),
                            target,
                            focus.clone(),
                            window,
                            app,
                        )
                    }),
            );
        }
        panel
    }
}

#[cfg(test)]
mod tests {
    use super::{Target, current_provider, navigation_receipt_current};
    use multi_launcher::plugin::Plugin;
    use std::collections::{HashMap, HashSet};
    #[test]
    fn typed_settings_admission_requires_exact_installed_provider_row() {
        let query = "settings   toast duration";
        let plugin = multi_launcher::plugins::settings::SettingsPlugin;
        let action = super::super::search_providers::search_routed(
            [&plugin as &dyn Plugin],
            query,
            None,
            None,
        )
        .remove(0)
        .1;
        assert_eq!(
            current_provider(query, &action, None, None).as_deref(),
            action.args.as_deref()
        );
        assert!(current_provider("settings toast", &action, None, None).is_none());
        let mut forged = action.clone();
        forged.label.push('!');
        assert!(current_provider(query, &forged, None, None).is_none());
        assert!(current_provider(query, &action, Some(&HashSet::new()), None).is_none());
        let no_search = HashMap::from([("settings".to_owned(), Vec::new())]);
        assert!(current_provider(query, &action, None, Some(&no_search)).is_none());
    }
    #[test]
    fn navigation_rejects_stale_query_and_retired_mount() {
        let owner = super::super::utility_mount::MountOwner::default();
        let first = owner.attach();
        assert!(navigation_receipt_current(
            &first, &first, 1, 1, "toast", "toast"
        ));
        assert!(!navigation_receipt_current(
            &first, &first, 1, 2, "toast", "toast"
        ));
        assert!(!navigation_receipt_current(
            &first, &first, 1, 1, "toast", "other"
        ));
        let next = owner.attach();
        assert!(!navigation_receipt_current(
            &first, &next, 1, 1, "toast", "toast"
        ));
    }
    #[test]
    fn catalog_mapping_only_exposes_implemented_controls() {
        assert_eq!(
            Target::from_id("search.autocomplete"),
            Some(Target::Autocomplete)
        );
        assert_eq!(
            Target::from_id("feedback.toast_duration/value"),
            Some(Target::Duration)
        );
        assert_eq!(Target::from_id("search.grid_rows"), None);
        assert_eq!(Target::from_id("plugins.toggle_sections"), None);
    }

    #[test]
    fn real_catalog_routes_launcher_placement_controls() {
        let catalog = multi_launcher::settings_editor::catalog::SettingsCatalog::default();
        for (query, expected) in [("follow mouse", Target::FollowMouse), ("use static position", Target::StaticPosition), ("static position x", Target::StaticX), ("static window height", Target::StaticHeight)] {
            assert!(catalog.search(query).iter().any(|entry| Target::from_id(entry.id.as_str()) == Some(expected)));
        }
    }
}
