//! Retained Search Results preferences backed by canonical scoped Settings transactions.
use super::*;
use multi_launcher::settings::{
    ScopedSetting as Field, ScopedSettingChange as Change, ScopedSettingValue as Value,
    ScopedSettingsAcknowledgement, ScopedSettingsError, Settings, SettingsBooleanSaveToken,
};

pub(super) const WAIT: &str = "Wait for Search Results settings to finish.";
#[derive(Clone, Debug, PartialEq)]
struct Values {
    autocomplete: bool,
    exact: bool,
    fuzzy: f32,
    usage: f32,
    page: usize,
    grid: bool,
    columns: usize,
    respect: bool,
    follow_mouse: bool,
    static_enabled: bool,
    static_position: Option<(i32, i32)>,
    static_size: Option<(i32, i32)>,
}
impl Values {
    fn from_settings(settings: &Settings) -> Self {
        Self {
            autocomplete: settings.query_autocomplete,
            exact: settings.match_exact,
            fuzzy: settings.fuzzy_weight,
            usage: settings.usage_weight,
            page: settings.page_jump,
            grid: settings.query_results_layout.enabled,
            columns: settings.query_results_layout.cols,
            respect: settings.query_results_layout.respect_plugin_capability,
            follow_mouse: settings.follow_mouse,
            static_enabled: settings.static_location_enabled,
            static_position: settings.static_pos,
            static_size: settings.static_size,
        }
    }
    fn changes(&self, next: &Self) -> Vec<Change> {
        let mut changes: Vec<Change> = [
            (
                Field::QueryAutocomplete,
                Value::Bool(self.autocomplete),
                Value::Bool(next.autocomplete),
            ),
            (
                Field::MatchExact,
                Value::Bool(self.exact),
                Value::Bool(next.exact),
            ),
            (
                Field::FuzzyWeight,
                Value::Float(self.fuzzy),
                Value::Float(next.fuzzy),
            ),
            (
                Field::UsageWeight,
                Value::Float(self.usage),
                Value::Float(next.usage),
            ),
            (
                Field::PageJump,
                Value::Count(self.page),
                Value::Count(next.page),
            ),
            (
                Field::GridEnabled,
                Value::Bool(self.grid),
                Value::Bool(next.grid),
            ),
            (
                Field::GridColumns,
                Value::Count(self.columns),
                Value::Count(next.columns),
            ),
            (
                Field::GridRespectCapability,
                Value::Bool(self.respect),
                Value::Bool(next.respect),
            ),
        ]
        .into_iter()
        .filter(|(_, old, new)| old != new)
        .map(|(field, baseline, desired)| Change {
            field,
            baseline,
            desired,
        })
        .collect();
        if (self.follow_mouse, self.static_enabled, self.static_position, self.static_size) != (next.follow_mouse, next.static_enabled, next.static_position, next.static_size) {
            changes.extend([
                (Field::FollowMouse, Value::Bool(self.follow_mouse), Value::Bool(next.follow_mouse)),
                (Field::StaticLocationEnabled, Value::Bool(self.static_enabled), Value::Bool(next.static_enabled)),
                (Field::StaticPosition, Value::OptionalPair(self.static_position), Value::OptionalPair(next.static_position)),
                (Field::StaticSize, Value::OptionalPair(self.static_size), Value::OptionalPair(next.static_size)),
            ].into_iter().map(|(field, baseline, desired)| Change { field, baseline, desired }));
        }
        changes
    }
    fn texts(&self) -> [String; 8] {
        let (x, y) = self.static_position.unwrap_or((0, 0));
        let (width, height) = self.static_size.unwrap_or((400, 220));
        [
            self.fuzzy.to_string(),
            self.usage.to_string(),
            self.page.to_string(),
            self.columns.to_string(),
            x.to_string(), y.to_string(), width.to_string(), height.to_string(),
        ]
    }
}
fn resolve_float(text: &str, baseline: f32) -> Result<f32, &'static str> {
    if text == baseline.to_string() {
        return Ok(baseline);
    }
    match text.trim().parse::<f32>() {
        Ok(value) if value.is_finite() && (0.0..=5.0).contains(&value) => Ok(value),
        _ => Err("Weights must be finite numbers from 0 to 5."),
    }
}
fn resolve_count(text: &str, baseline: usize, enabled: bool) -> Result<usize, &'static str> {
    if text == baseline.to_string() {
        return Ok(baseline);
    }
    match text.trim().parse::<usize>() {
        Ok(value) if (1..=100).contains(&value) => Ok(value),
        _ if !enabled => Ok(baseline),
        _ => Err("Page jump and grid columns must be whole numbers from 1 to 100."),
    }
}
fn resolve_coordinate(text: &str, fallback: i32, enabled: bool) -> Result<i32, &'static str> {
    match text.trim().parse::<i32>() {
        Ok(value) => Ok(value),
        Err(_) if !enabled => Ok(fallback),
        Err(_) => Err("Static placement values must be signed 32-bit integers."),
    }
}
fn displayed_static(follow_mouse: bool, static_enabled: bool) -> bool {
    !follow_mouse && static_enabled
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct RestartDraft {
    baseline: Option<Values>,
    draft: Option<Values>,
    texts: [String; 8],
    placement_text_baseline: [String; 4],
    busy: bool,
}
impl RestartDraft {
    pub(super) fn dirty(&self) -> bool {
        self.baseline != self.draft
            || self
                .baseline
                .as_ref()
                .is_some_and(|base| self.texts[..4] != base.texts()[..4] || self.texts[4..] != self.placement_text_baseline)
    }
}

pub(super) struct SearchSettings {
    root: WeakEntity<RootWindow>,
    owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    focus: FocusHandle,
    checkbox_focus: HashMap<Field, FocusHandle>,
    scroll: ScrollHandle,
    baseline: Option<Values>,
    draft: Option<Values>,
    inputs: [Entity<InputState>; 8],
    placement_text_baseline: [String; 4],
    _subscriptions: Vec<Subscription>,
    reading: bool,
    pending: Option<(SettingsBooleanSaveToken, Vec<Change>)>,
    status: Option<String>,
}
impl SearchSettings {
    pub(super) fn new(
        root: WeakEntity<RootWindow>,
        focus: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let owner = utility_mount::MountOwner::default();
        let mount = owner.attach();
        let inputs = std::array::from_fn(|_| cx.new(|cx| InputState::new(window, cx)));
        let subscriptions = inputs
            .iter()
            .map(|input| {
                cx.subscribe(input, |_: &mut Self, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                })
            })
            .collect();
        let checkbox_focus = [
            Field::QueryAutocomplete,
            Field::MatchExact,
            Field::GridEnabled,
            Field::GridRespectCapability,
            Field::FollowMouse,
            Field::StaticLocationEnabled,
        ]
        .into_iter()
        .map(|field| {
            (
                field,
                if field == Field::QueryAutocomplete {
                    focus.clone()
                } else {
                    cx.focus_handle()
                },
            )
        })
        .collect();
        Self {
            root,
            owner,
            mount,
            focus,
            checkbox_focus,
            scroll: ScrollHandle::new(),
            baseline: None,
            draft: None,
            inputs,
            placement_text_baseline: std::array::from_fn(|_| String::new()),
            _subscriptions: subscriptions,
            reading: false,
            pending: None,
            status: None,
        }
    }
    pub(super) fn can_focus(&self, target: settings_navigation::Target) -> bool {
        use settings_navigation::Target::*;
        if self.busy() || !self.mount.is_current() {
            return false;
        }
        let Some(draft) = &self.draft else {
            return false;
        };
        match target {
            Autocomplete | Exact | Fuzzy | Usage | Page | Grid => true,
            Columns | Respect => draft.grid,
            FollowMouse => true,
            StaticPosition => !draft.follow_mouse,
            StaticX | StaticY | StaticWidth | StaticHeight => !draft.follow_mouse && draft.static_enabled,
            _ => false,
        }
    }
    pub(super) fn focus_control(
        &mut self,
        target: settings_navigation::Target,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        use settings_navigation::Target::*;
        if !self.can_focus(target) {
            return false;
        }
        let grid = self.draft.as_ref().is_some_and(|draft| draft.grid);
        let placement_focus = match target {
            FollowMouse => self.checkbox_focus.get(&Field::FollowMouse).cloned(),
            StaticPosition => self.checkbox_focus.get(&Field::StaticLocationEnabled).cloned(),
            StaticX => Some(self.inputs[4].focus_handle(cx)),
            StaticY => Some(self.inputs[5].focus_handle(cx)),
            StaticWidth => Some(self.inputs[6].focus_handle(cx)),
            StaticHeight => Some(self.inputs[7].focus_handle(cx)),
            _ => None,
        };
        if let Some(focus) = placement_focus {
            let header = 10 + 3 * usize::from(grid);
            let index = match target {
                FollowMouse => header + 1,
                StaticPosition => header + 2,
                StaticX => header + 4,
                StaticY => header + 6,
                StaticWidth => header + 8,
                StaticHeight => header + 10,
                _ => return false,
            };
            self.scroll.scroll_to_top_of_item(index);
            focus.focus(window, cx);
            cx.notify();
            return true;
        }
        let base = 4 + usize::from(grid);
        let (index, focus) = match target {
            Autocomplete => (
                1,
                self.checkbox_focus.get(&Field::QueryAutocomplete).cloned(),
            ),
            Exact => (2, self.checkbox_focus.get(&Field::MatchExact).cloned()),
            Grid => (3, self.checkbox_focus.get(&Field::GridEnabled).cloned()),
            Respect => (
                4,
                self.checkbox_focus
                    .get(&Field::GridRespectCapability)
                    .cloned(),
            ),
            Fuzzy => (base + 1, Some(self.inputs[0].focus_handle(cx))),
            Usage => (base + 3, Some(self.inputs[1].focus_handle(cx))),
            Page => (base + 5, Some(self.inputs[2].focus_handle(cx))),
            Columns => (base + 7, Some(self.inputs[3].focus_handle(cx))),
            _ => return false,
        };
        let Some(focus) = focus else {
            return false;
        };
        self.scroll.scroll_to_top_of_item(index);
        focus.focus(window, cx);
        cx.notify();
        true
    }

    pub(super) fn busy(&self) -> bool {
        self.reading || self.pending.is_some()
    }
    pub(super) fn pending_token(&self) -> Option<SettingsBooleanSaveToken> {
        self.pending.as_ref().map(|(token, _)| token.clone())
    }

    pub(super) fn retire(&mut self) {
        self.owner.retire();
    }
    fn texts(&self, cx: &App) -> [String; 8] {
        std::array::from_fn(|i| self.inputs[i].read(cx).value().to_string())
    }
    pub(super) fn restart_draft(&self, cx: &App) -> RestartDraft {
        RestartDraft {
            baseline: self.baseline.clone(),
            draft: self.draft.clone(),
            texts: self.texts(cx),
            placement_text_baseline: self.placement_text_baseline.clone(),
            busy: self.busy(),
        }
    }
    fn set_texts(&self, values: &Values, window: &mut Window, cx: &mut Context<Self>) {
        for (input, text) in self.inputs.iter().zip(values.texts()) {
            input.update(cx, |input, cx| input.set_value(text, window, cx));
        }
    }
    fn set_search_texts(&self, values: &Values, window: &mut Window, cx: &mut Context<Self>) {
        for (input, text) in self.inputs[..4].iter().zip(values.texts().into_iter().take(4)) {
            input.update(cx, |input, cx| input.set_value(text, window, cx));
        }
    }
    fn settle_texts(&mut self, values: &Values, window: &mut Window, cx: &mut Context<Self>) {
        self.set_search_texts(values, window, cx);
        if !values.follow_mouse && values.static_enabled {
            for (input, text) in self.inputs[4..].iter().zip(values.texts().into_iter().skip(4)) {
                input.update(cx, |input, cx| input.set_value(text, window, cx));
            }
        }
        let texts = self.texts(cx);
        self.placement_text_baseline = std::array::from_fn(|i| texts[i + 4].clone());
    }
    fn readonly(&self, readonly: bool, cx: &mut Context<Self>) {
        for input in &self.inputs {
            input.update(cx, |input, cx| input.set_readonly(readonly, cx));
        }
    }
    fn clear_wait(&self, cx: &mut Context<Self>) {
        let _ = self.root.update(cx, |root, cx| {
            if root.restart_status.as_deref() == Some(WAIT) {
                root.restart_status = None;
            }
            if root.settings_editor.status.as_deref() == Some(WAIT) {
                root.settings_editor.status = None;
            }
            cx.notify();
        });
    }
    fn installed(&self, host: WindowId, id: EntityId, cx: &App) -> bool {
        self.root.upgrade().is_some_and(|root| {
            let root = root.read(cx);
            root.settings_editor.open
                && root
                    .settings_search
                    .as_ref()
                    .is_some_and(|current| current.entity_id() == id)
                && root
                    .root_window
                    .is_some_and(|current| current.window_id() == host)
                && !root.restart_admission.is_fenced()
        })
    }
    pub(super) fn read(
        &mut self,
        path: String,
        workers: SettingsSaveWorkerRegistry,
        permit: EffectPermit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reading = true;
        let receiver = match workers.begin_scoped_preferences_read(path) {
            Ok(receiver) => receiver,
            Err(error) => {
                self.reading = false;
                self.status = Some(error);
                cx.notify();
                return;
            }
        };
        let host = window.window_handle();
        let mount = self.mount.clone();
        cx.spawn(async move |weak, cx| {
            let _permit = permit;
            let result = loop {
                match receiver.try_recv() {
                    Ok(result) => break result,
                    Err(TryRecvError::Disconnected) => {
                        break Err("Settings reader ended without a receipt.".into());
                    }
                    Err(TryRecvError::Empty) => {
                        cx.background_executor()
                            .timer(Duration::from_millis(20))
                            .await
                    }
                }
            };
            let Some(entity) = weak.upgrade() else {
                return;
            };
            let updated = update_kit_window_content(host, &entity, cx, |view, window, cx| {
                if !read_is_current(
                    &mount,
                    &view.mount,
                    view.reading,
                    view.installed(host.window_id(), cx.entity_id(), cx),
                ) {
                    return;
                }
                view.reading = false;
                match result {
                    Ok(settings) => {
                        let values = Values::from_settings(&settings);
                        view.set_texts(&values, window, cx);
                        let texts = values.texts();
                        view.placement_text_baseline = std::array::from_fn(|i| texts[i + 4].clone());
                        view.baseline = Some(values.clone());
                        view.draft = Some(values);
                        view.status = None;
                    }
                    Err(error) => {
                        view.status =
                            Some(format!("Could not read Search Results settings: {error}"))
                    }
                }
                view.clear_wait(cx);
                cx.notify();
            });
            if updated.is_err() {
                let _ = entity.update(cx, |view, cx| {
                    if mount.is_current() {
                        view.reading = false;
                        view.status = Some("Settings window is unavailable.".into());
                        view.clear_wait(cx);
                        cx.notify();
                    }
                });
            }
        })
        .detach();
        cx.notify();
    }
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy() || !self.mount.is_current() {
            return;
        }
        let (Some(baseline), Some(mut desired)) = (self.baseline.clone(), self.draft.clone())
        else {
            return;
        };
        let texts = self.texts(cx);
        let parsed = (|| {
            desired.fuzzy = resolve_float(&texts[0], baseline.fuzzy)?;
            desired.usage = resolve_float(&texts[1], baseline.usage)?;
            desired.page = resolve_count(&texts[2], baseline.page, true)?;
            desired.columns = resolve_count(&texts[3], baseline.columns, desired.grid)?;
            let placement_edited = desired.follow_mouse != baseline.follow_mouse || desired.static_enabled != baseline.static_enabled || texts[4..] != self.placement_text_baseline;
            if placement_edited {
                if desired.follow_mouse {
                    desired.static_enabled = false;
                    desired.static_position = None;
                    desired.static_size = None;
                } else {
                    let (x, y) = desired.static_position.unwrap_or((0, 0));
                    let (width, height) = desired.static_size.unwrap_or((400, 220));
                    let parse = |index: usize, fallback: i32| resolve_coordinate(&texts[index], fallback, desired.static_enabled);
                    desired.static_position = Some((parse(4, x)?, parse(5, y)?));
                    desired.static_size = Some((parse(6, width)?, parse(7, height)?));
                }
            }
            Ok::<(), &'static str>(())
        })();
        if let Err(error) = parsed {
            self.status = Some(error.into());
            cx.notify();
            return;
        }
        let changes = baseline.changes(&desired);
        if changes.is_empty() {
            self.settle_texts(&baseline, window, cx);
            self.status = Some("No changes to save.".into());
            cx.notify();
            return;
        }
        let Some(root) = self.root.upgrade() else {
            return;
        };
        let modal = window.has_active_dialog(cx);
        let root_state = root.read(cx);
        if !self.installed(window.window_handle().window_id(), cx.entity_id(), cx)
            || modal
            || !window.is_window_active()
            || !root_state.external_owner_is_current(window.focused(cx).as_ref(), window, cx)
            || root_state
                .native_window
                .is_none_or(|native| current_foreground_identity() != Some(native.identity))
        {
            return;
        }
        let Some(permit) = root_state.restart_admission.admit() else {
            return;
        };
        let workers = root_state.settings_save_workers.clone();
        let path = root_state.settings_path.clone();
        let (receiver, token) = match workers.begin_scoped_preferences_save(path, changes.clone()) {
            Ok(value) => value,
            Err(error) => {
                self.status = Some(error);
                cx.notify();
                return;
            }
        };
        self.pending = Some((token.clone(), changes.clone()));
        self.status = None;
        self.readonly(true, cx);
        let host = window.window_handle();
        let mount = self.mount.clone();
        cx.spawn(async move |weak, cx| {
            let _permit = permit;
            let result = loop {
                match receiver.try_recv() {
                    Ok(result) => break result,
                    Err(TryRecvError::Disconnected) => {
                        break Err(ScopedSettingsError::Store(
                            "Settings writer ended without an acknowledgement.".into(),
                        ));
                    }
                    Err(TryRecvError::Empty) => {
                        cx.background_executor()
                            .timer(Duration::from_millis(20))
                            .await
                    }
                }
            };
            let Some(entity) = weak.upgrade() else {
                return;
            };
            let fallback = mount.clone();
            let mut terminal = None;
            let updated = update_kit_window_content(host, &entity, cx, |view, window, cx| {
                if !acknowledgement_is_current(
                    &mount,
                    &view.mount,
                    view.pending.as_ref().map(|(pending, _)| pending.id()),
                    token.id(),
                ) || !view.installed(host.window_id(), cx.entity_id(), cx)
                {
                    return;
                }
                workers.acknowledge_boolean_preference_save(token.id());
                view.pending = None;
                view.readonly(false, cx);
                let failed = match &result {
                    Ok(ack) => !settings_actions::ack_matches(&changes, ack),
                    Err(ScopedSettingsError::Cancelled) => false,
                    Err(_) => true,
                };
                let accepted = accept_draft_ack(
                    &mut view.baseline,
                    &mut view.draft,
                    &desired,
                    &changes,
                    &result,
                );
                match result {
                    Ok(ack) if accepted => {
                        view.settle_texts(&desired, window, cx);
                        let _ = view.root.update(cx, |root, cx| {
                            if changes.iter().any(|change| change.field == Field::FollowMouse) {
                                let mut placement = root.placement_preferences.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                                *placement = placement.with_preferences(desired.follow_mouse, desired.static_enabled, desired.static_position, desired.static_size);
                            }
                            if apply_acknowledged(&mut root.search_catalog, &changes, &ack) {
                                let query = root.query.read(cx).value().to_string();
                                let policy = refresh_policy(&changes, &query);
                                if policy.completion {
                                    root.search_completion
                                        .reset_for_query(&root.search_catalog, &query);
                                }
                                if policy.rescore {
                                    root.refresh_launcher_results_with_viewport(query, false, cx);
                                } else if root.launcher_results.query == query {
                                    if policy.relayout {
                                        root.retire_search_jump_mode();
                                        root.fresh_search_enter = None;
                                        root.launcher_results.selected = None;
                                        let layout = root.search_catalog.resolve_result_layout(
                                            &query,
                                            root.enabled_plugins.as_ref(),
                                        );
                                        let show_full_path = root
                                            .enabled_capabilities
                                            .as_ref()
                                            .and_then(|caps| caps.get("folders"))
                                            .is_some_and(|caps| {
                                                caps.iter().any(|cap| cap == "show_full_path")
                                            });
                                        let aliases = if !layout.grid && !show_full_path {
                                            root.search_catalog.providers.aliased_folder_rows()
                                        } else {
                                            HashMap::new()
                                        };
                                        decorate_folder_descriptions(
                                            &mut root.launcher_results.actions,
                                            layout.grid,
                                            show_full_path,
                                            &aliases,
                                        );
                                        root.launcher_results.layout = layout;
                                        root.launcher_results_scroll = ScrollHandle::new();
                                    }
                                    if policy.page_jump {
                                        root.launcher_results.page_jump =
                                            root.search_catalog.page_jump;
                                    }
                                }
                            }
                            if root
                                .settings_actions
                                .as_ref()
                                .is_none_or(|actions| !actions.read(cx).restart_draft(cx).dirty())
                            {
                                clear_stale_settings_restart_status(&mut root.restart_status, true);
                            }
                            cx.notify();
                        });
                        view.status = Some("Search Results settings saved.".into());
                    }
                    Ok(_) => {
                        view.status = Some(
                            "Settings acknowledgement did not match the requested values.".into(),
                        )
                    }
                    Err(error) => {
                        view.status =
                            Some(format!("Could not save Search Results settings: {error}"))
                    }
                }
                view.clear_wait(cx);
                terminal = Some((view.root.clone(), failed));
                cx.notify();
            });
            if updated.is_err() {
                let _ = entity.update(cx, |view, cx| {
                    if fallback.is_current()
                        && view
                            .pending
                            .as_ref()
                            .is_some_and(|(pending, _)| pending.id() == token.id())
                    {
                        workers.acknowledge_boolean_preference_save(token.id());
                        view.pending = None;
                        view.readonly(false, cx);
                        view.status = Some(
                            "Settings window is unavailable; reopen Settings to reload.".into(),
                        );
                        view.clear_wait(cx);
                        terminal = Some((view.root.clone(), true));
                        cx.notify();
                    }
                });
            }
            if let Some((root, failed)) = terminal {
                let _ = root.update(cx, |root, cx| {
                    root.finish_deferred_preference_close(failed, cx)
                });
            }
        })
        .detach();
        cx.notify();
    }
}

fn accept_draft_ack(
    baseline: &mut Option<Values>,
    draft: &mut Option<Values>,
    desired: &Values,
    changes: &[Change],
    result: &Result<ScopedSettingsAcknowledgement, ScopedSettingsError>,
) -> bool {
    if !result
        .as_ref()
        .is_ok_and(|ack| settings_actions::ack_matches(changes, ack))
    {
        return false;
    }
    *baseline = Some(desired.clone());
    *draft = Some(desired.clone());
    true
}

fn read_is_current(
    issued: &utility_mount::MountReceipt,
    current: &utility_mount::MountReceipt,
    reading: bool,
    installed: bool,
) -> bool {
    reading && installed && issued.is_current() && issued.matches(current)
}

fn acknowledgement_is_current(
    issued: &utility_mount::MountReceipt,
    current: &utility_mount::MountReceipt,
    pending: Option<u64>,
    received: u64,
) -> bool {
    issued.is_current() && issued.matches(current) && pending == Some(received)
}

#[derive(Debug, PartialEq, Eq)]
struct RefreshPolicy {
    completion: bool,
    rescore: bool,
    relayout: bool,
    page_jump: bool,
}
fn refresh_policy(changes: &[Change], query: &str) -> RefreshPolicy {
    let completion = changes
        .iter()
        .any(|change| change.field == Field::QueryAutocomplete);
    let scoring = changes.iter().any(|change| {
        matches!(
            change.field,
            Field::MatchExact | Field::FuzzyWeight | Field::UsageWeight
        )
    });
    let relayout = changes.iter().any(|change| {
        matches!(
            change.field,
            Field::GridEnabled | Field::GridColumns | Field::GridRespectCapability
        )
    });
    let page_jump = changes.iter().any(|change| change.field == Field::PageJump);
    // A scalar evaluation can append history; preserve its current or pending
    // owner until the user changes the query. Calculator history is read-only.
    let scalar = calculator_search::scalar_expression(query).is_some();
    RefreshPolicy {
        completion,
        rescore: scoring && !scalar,
        relayout,
        page_jump,
    }
}

fn apply_acknowledged(
    catalog: &mut LauncherSearchCatalog,
    changes: &[Change],
    ack: &ScopedSettingsAcknowledgement,
) -> bool {
    if !settings_actions::ack_matches(changes, ack) {
        return false;
    }
    for change in changes {
        match (&change.field, &change.desired) {
            (Field::QueryAutocomplete, Value::Bool(v)) => catalog.query_autocomplete = *v,
            (Field::MatchExact, Value::Bool(v)) => catalog.match_exact = *v,
            (Field::FuzzyWeight, Value::Float(v)) => catalog.fuzzy_weight = *v,
            (Field::UsageWeight, Value::Float(v)) => catalog.usage_weight = *v,
            (Field::PageJump, Value::Count(v)) => catalog.page_jump = *v,
            (Field::GridEnabled, Value::Bool(v)) => catalog.query_results_layout.enabled = *v,
            (Field::GridColumns, Value::Count(v)) => catalog.query_results_layout.cols = *v,
            (Field::GridRespectCapability, Value::Bool(v)) => {
                catalog.query_results_layout.respect_plugin_capability = *v
            }
            _ => {}
        }
    }
    true
}

impl Render for SearchSettings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let busy = self.busy();
        let scroll = self.scroll.clone();
        let mut panel = div()
            .id("settings-search-results")
            .when(self.draft.is_none(), |panel| panel.track_focus(&self.focus))
            .flex()
            .flex_col()
            .gap_2()
            .max_h(px(350.0))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .on_scroll(move |event, _, cx| {
                let pixels = event.delta.pixel_delta(px(20.0));
                let delta = if pixels.y.is_zero() {
                    pixels.x
                } else {
                    pixels.y
                }
                .as_f32();
                let offset = scroll.offset();
                let maximum = scroll.max_offset().y.as_f32();
                let previous = (offset.y.as_f32() - delta).clamp(-maximum, 0.0);
                scroll.set_offset(point(offset.x, px(offset.y.as_f32().clamp(-maximum, 0.0))));
                if inner_results_consumed_scroll(previous, maximum, delta) {
                    cx.stop_propagation();
                }
            })
            .child(Label::new("Search & Results"));
        if let Some(draft) = &self.draft {
            for (field, label, checked) in [
                (
                    Field::QueryAutocomplete,
                    "Enable query autocomplete",
                    draft.autocomplete,
                ),
                (Field::MatchExact, "Match exact", draft.exact),
                (
                    Field::GridEnabled,
                    "Display results in grid layout",
                    draft.grid,
                ),
                (
                    Field::GridRespectCapability,
                    "Respect plugin list/grid capability",
                    draft.respect,
                ),
            ] {
                if field == Field::GridRespectCapability && !draft.grid {
                    continue;
                }
                let weak = cx.entity().downgrade();
                let mount = self.mount.clone();
                let Some(focus) = self.checkbox_focus.get(&field).cloned() else {
                    continue;
                };
                panel = panel.child(
                    Checkbox::new(SharedString::from(format!("search-setting-{field:?}")))
                        .checked(checked)
                        .track_focus(&focus)
                        .disabled(busy)
                        .accessibility_label(label)
                        .on_change(move |state, _, _, app| {
                            let _ = weak.update(app, |view, cx| {
                                if view.busy() || !mount.is_current() || !mount.matches(&view.mount)
                                {
                                    return;
                                }
                                if let Some(draft) = &mut view.draft {
                                    let value = state == CheckboxState::Checked;
                                    match field {
                                        Field::QueryAutocomplete => draft.autocomplete = value,
                                        Field::MatchExact => draft.exact = value,
                                        Field::GridEnabled => draft.grid = value,
                                        Field::GridRespectCapability => draft.respect = value,
                                        _ => {}
                                    }
                                }
                                view.status = None;
                                cx.notify();
                            });
                        })
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(CheckboxIndicator::new().checked(checked))
                                .child(Label::new(label)),
                        ),
                );
            }
            for (index, label) in [
                (0, "Fuzzy weight"),
                (1, "Usage weight"),
                (2, "Page jump"),
                (3, "Grid columns"),
            ] {
                if index == 3 && !draft.grid {
                    continue;
                }
                panel = panel
                    .child(div().id(SharedString::from(format!("search-setting-label-{index}"))).child(Label::new(label)))
                    .child(Input::new(&self.inputs[index]).readonly(busy).aria_label(label));
            }
            panel = panel.child(Label::new("Launcher Window"));
            for (field, label, checked, disabled) in [
                (Field::FollowMouse, "Follow mouse", draft.follow_mouse, false),
                (Field::StaticLocationEnabled, "Use static position", displayed_static(draft.follow_mouse, draft.static_enabled), draft.follow_mouse),
            ] {
                let weak = cx.entity().downgrade();
                let mount = self.mount.clone();
                let Some(focus) = self.checkbox_focus.get(&field).cloned() else { continue; };
                panel = panel.child(Checkbox::new(SharedString::from(format!("launcher-placement-{field:?}")))
                    .checked(checked).track_focus(&focus).disabled(busy || disabled).accessibility_label(label)
                    .on_change(move |state, _, _, app| {
                        let _ = weak.update(app, |view, cx| {
                            if view.busy() || !mount.is_current() || !mount.matches(&view.mount) { return; }
                            if let Some(draft) = &mut view.draft {
                                let checked = state == CheckboxState::Checked;
                                match field {
                                    Field::FollowMouse => { if draft.follow_mouse != checked { draft.static_enabled = false; } draft.follow_mouse = checked; },
                                    Field::StaticLocationEnabled if !draft.follow_mouse => draft.static_enabled = checked,
                                    _ => {}
                                }
                            }
                            view.status = None;
                            cx.notify();
                        });
                    }).child(div().flex().gap_2().child(CheckboxIndicator::new().checked(checked)).child(Label::new(label))));
            }
            if !draft.follow_mouse && draft.static_enabled {
                for (index, label) in [(4, "Static position X"), (5, "Static position Y"), (6, "Static window width"), (7, "Static window height")] {
                    panel = panel.child(div().id(SharedString::from(format!("launcher-placement-label-{index}"))).child(Label::new(label))).child(Input::new(&self.inputs[index]).readonly(busy).aria_label(label));
                }
            }
            panel = panel.child(
                Button::new("settings-search-save")
                    .label(if self.pending.is_some() {
                        "Saving…"
                    } else {
                        "Save Search Settings"
                    })
                    .disabled(busy)
                    .on_click(cx.listener(|view, _, window, cx| view.save(window, cx))),
            );
        } else if self.reading {
            panel = panel.child(Label::new("Reading Search Results…"));
        }
        if let Some(status) = &self.status {
            panel = panel.child(Label::new(status.clone()).id("settings-search-status"));
        }
        panel
    }
}
impl RootWindow {
    pub(super) fn open_search_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_search.is_some() {
            return;
        }
        let Some(permit) = self.restart_admission.admit() else {
            return;
        };
        let root = cx.entity().downgrade();
        let focus = self.settings_focus_handle.clone();
        let view = cx.new(|cx| SearchSettings::new(root, focus, window, cx));
        self.settings_search = Some(view.clone());
        let path = self.settings_path.clone();
        let workers = self.settings_save_workers.clone();
        view.update(cx, |view, cx| view.read(path, workers, permit, window, cx));
    }
}

#[cfg(test)]
mod tests {
    use super::{Values, apply_acknowledged, refresh_policy, resolve_count, resolve_float};
    use multi_launcher::settings::{ScopedSetting as Field, ScopedSettingValue as Value, Settings};
    #[test]
    fn exact_ack_changes_live_catalog_and_chooses_one_refresh() {
        use multi_launcher::settings::{
            ScopedSettingChange as Change, ScopedSettingsAcknowledgement,
        };
        let mut catalog = super::super::tests::completion_catalog(Vec::new(), true, None);
        let changes = vec![
            Change {
                field: Field::QueryAutocomplete,
                baseline: Value::Bool(true),
                desired: Value::Bool(false),
            },
            Change {
                field: Field::GridColumns,
                baseline: Value::Count(catalog.query_results_layout.cols),
                desired: Value::Count(4),
            },
        ];
        let wrong = ScopedSettingsAcknowledgement {
            values: vec![(Field::QueryAutocomplete, Value::Bool(false))],
            persisted: true,
        };
        assert!(!apply_acknowledged(&mut catalog, &changes, &wrong));
        assert!(catalog.query_autocomplete);
        let correct = ScopedSettingsAcknowledgement {
            values: changes
                .iter()
                .map(|change| (change.field, change.desired.clone()))
                .collect(),
            persisted: true,
        };
        assert!(apply_acknowledged(&mut catalog, &changes, &correct));
        assert!(!catalog.query_autocomplete);
        assert_eq!(catalog.query_results_layout.cols, 4);
        let both = refresh_policy(&changes, "needle");
        assert!(both.completion && both.relayout && !both.rescore);
        let autocomplete = refresh_policy(&changes[..1], "needle");
        assert!(autocomplete.completion && !autocomplete.relayout && !autocomplete.rescore);
        let layout = refresh_policy(&changes[1..], "=2+2");
        assert!(!layout.completion && layout.relayout && !layout.rescore);
        let score = [Change {
            field: Field::FuzzyWeight,
            baseline: Value::Float(1.0),
            desired: Value::Float(2.0),
        }];
        let scalar = refresh_policy(&score, "=2+2");
        assert!(!scalar.rescore && !scalar.relayout);
        assert!(refresh_policy(&score, "ordinary").rescore);
        assert!(refresh_policy(&score, "= history").rescore);
        let page = [Change {
            field: Field::PageJump,
            baseline: Value::Count(2),
            desired: Value::Count(3),
        }];
        let page_policy = refresh_policy(&page, "=2+2");
        assert!(page_policy.page_jump && !page_policy.relayout && !page_policy.rescore);
    }

    #[test]
    fn failed_or_wrong_ack_keeps_retryable_draft_until_exact_ack() {
        use multi_launcher::settings::{ScopedSettingsAcknowledgement, ScopedSettingsError};
        let mut baseline = Some(Values::from_settings(&Settings::default()));
        let mut desired = baseline.clone().unwrap();
        desired.exact = !desired.exact;
        let mut draft = Some(desired.clone());
        let changes = baseline.as_ref().unwrap().changes(&desired);
        let failure = Err(ScopedSettingsError::Store("failed".into()));
        assert!(!super::accept_draft_ack(
            &mut baseline,
            &mut draft,
            &desired,
            &changes,
            &failure
        ));
        assert_ne!(baseline, draft);
        let wrong = Ok(ScopedSettingsAcknowledgement {
            values: vec![],
            persisted: true,
        });
        assert!(!super::accept_draft_ack(
            &mut baseline,
            &mut draft,
            &desired,
            &changes,
            &wrong
        ));
        assert_ne!(baseline, draft);
        let exact = Ok(ScopedSettingsAcknowledgement {
            values: changes
                .iter()
                .map(|change| (change.field, change.desired.clone()))
                .collect(),
            persisted: true,
        });
        assert!(super::accept_draft_ack(
            &mut baseline,
            &mut draft,
            &desired,
            &changes,
            &exact
        ));
        assert_eq!(baseline, draft);
    }

    #[test]
    fn stale_read_does_not_publish_into_a_new_settings_mount() {
        let owner = super::super::utility_mount::MountOwner::default();
        let first = owner.attach();
        assert!(super::read_is_current(&first, &first, true, true));
        assert!(!super::read_is_current(&first, &first, false, true));
        assert!(!super::read_is_current(&first, &first, true, false));
        let next = owner.attach();
        assert!(!super::read_is_current(&first, &next, true, true));
    }

    #[test]
    fn stale_or_retired_ack_cannot_release_current_save() {
        let owner = super::super::utility_mount::MountOwner::default();
        let first = owner.attach();
        assert!(super::acknowledgement_is_current(
            &first,
            &first,
            Some(7),
            7
        ));
        assert!(!super::acknowledgement_is_current(
            &first,
            &first,
            Some(8),
            7
        ));
        let next = owner.attach();
        assert!(!super::acknowledgement_is_current(
            &first,
            &next,
            Some(7),
            7
        ));
        owner.retire();
        assert!(!super::acknowledgement_is_current(&next, &next, Some(7), 7));
    }

    #[test]
    fn numeric_controls_require_stock_ranges_and_preserve_hidden_or_untouched_values() {
        assert_eq!(resolve_float("0.10", 0.1), Ok(0.1));
        assert!(resolve_float("NaN", 0.1).is_err());
        assert!(resolve_float("5.1", 0.1).is_err());
        assert_eq!(resolve_float("7", 7.0), Ok(7.0));
        assert_eq!(resolve_count("invalid", 3, false), Ok(3));
        assert!(resolve_count("0", 3, true).is_err());
        assert!(resolve_count("101", 3, true).is_err());
    }
    #[test]
    fn changes_include_only_edited_search_values_and_preserve_unowned_grid_fields() {
        let settings = Settings::default();
        let before = Values::from_settings(&settings);
        let mut after = before.clone();
        after.fuzzy = 3.0;
        after.columns = 4;
        let changes = before.changes(&after);
        assert_eq!(changes.len(), 2);
        assert!(changes.iter().any(
            |change| change.field == Field::FuzzyWeight && change.desired == Value::Float(3.0)
        ));
        assert!(
            changes
                .iter()
                .any(|change| change.field == Field::GridColumns
                    && change.desired == Value::Count(4))
        );
        assert!(
            !changes
                .iter()
                .any(|change| matches!(change.field, Field::GridRows | Field::GridPluginOptOut))
        );
    }

    #[test]
    fn placement_changes_include_the_whole_group_and_optional_signed_values() {
        let mut settings = Settings::default();
        settings.follow_mouse = false;
        settings.static_location_enabled = true;
        settings.static_pos = Some((-50, 20));
        settings.static_size = None;
        let before = Values::from_settings(&settings);
        let mut after = before.clone();
        after.follow_mouse = true;
        after.static_enabled = false;
        after.static_position = None;
        let changes = before.changes(&after);
        assert_eq!(changes.len(), 4);
        assert!(changes.iter().any(|change| change.field == Field::StaticPosition && change.baseline == Value::OptionalPair(Some((-50, 20))) && change.desired == Value::OptionalPair(None)));
        assert!(changes.iter().any(|change| change.field == Field::StaticSize && change.baseline == change.desired));
        assert_eq!(super::resolve_coordinate("-25", 4, true), Ok(-25));
        assert_eq!(super::resolve_coordinate("invalid", 4, false), Ok(4));
        assert!(super::resolve_coordinate("invalid", 4, true).is_err());
        assert!(!super::displayed_static(true, true));
        assert!(super::displayed_static(false, true));
    }

    #[test]
    fn hidden_placement_scratch_has_its_own_acknowledged_baseline() {
        let mut settings = Settings::default();
        settings.static_pos = None;
        settings.static_size = None;
        let values = Values::from_settings(&settings);
        let mut texts = values.texts();
        texts[4] = "-321".into();
        let mut draft = super::RestartDraft {
            baseline: Some(values.clone()), draft: Some(values), texts,
            placement_text_baseline: ["0".into(), "0".into(), "400".into(), "220".into()], busy: false,
        };
        assert!(draft.dirty());
        draft.placement_text_baseline = std::array::from_fn(|i| draft.texts[i + 4].clone());
        assert!(!draft.dirty());
        assert_eq!(draft.texts[4], "-321");
    }
}
