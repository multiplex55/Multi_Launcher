use crate::actions::Action;
use crate::common::json_watch::watch_json;
use crate::common::strip_prefix_ci;
use crate::mouse_gestures::db::{
    BindingMatchContext, GESTURES_FILE, GestureDb, SharedGestureDb, format_gesture_label,
    format_search_result_label, load_gestures,
};
use crate::mouse_gestures::service::{
    CancelBehavior, DEFAULT_RECOGNITION_INTERVAL_MS, DEFAULT_TRAIL_INTERVAL_MS, MouseGestureConfig,
    NoMatchBehavior, WheelCycleGate, with_service as with_gesture_service,
};
use crate::plugin::Plugin;
use eframe::egui;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

const PLUGIN_NAME: &str = "mouse_gestures";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MouseGestureSettings {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_debug_logging")]
    pub debug_logging: bool,
    #[serde(default = "default_show_trail")]
    pub show_trail: bool,
    #[serde(default = "default_trail_color")]
    pub trail_color: [u8; 4],
    #[serde(default = "default_trail_width")]
    pub trail_width: f32,
    #[serde(default = "default_trail_start_move_px")]
    pub trail_start_move_px: f32,
    #[serde(default = "default_trail_interval_ms")]
    pub trail_interval_ms: u64,
    #[serde(default = "default_recognition_interval_ms")]
    pub recognition_interval_ms: u64,
    #[serde(default = "default_show_hint")]
    pub show_hint: bool,
    #[serde(default = "default_hint_offset")]
    pub hint_offset: (f32, f32),
    #[serde(default = "default_cancel_behavior")]
    pub cancel_behavior: CancelBehavior,
    #[serde(default = "default_no_match_behavior")]
    pub no_match_behavior: NoMatchBehavior,
    #[serde(default = "default_wheel_cycle_gate")]
    pub wheel_cycle_gate: WheelCycleGate,
    #[serde(default = "default_practice_mode")]
    pub practice_mode: bool,
    #[serde(default)]
    pub ignore_window_titles: Vec<String>,
}

impl Default for MouseGestureSettings {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            debug_logging: default_debug_logging(),
            show_trail: default_show_trail(),
            trail_color: default_trail_color(),
            trail_width: default_trail_width(),
            trail_start_move_px: default_trail_start_move_px(),
            trail_interval_ms: default_trail_interval_ms(),
            recognition_interval_ms: default_recognition_interval_ms(),
            show_hint: default_show_hint(),
            hint_offset: default_hint_offset(),
            cancel_behavior: default_cancel_behavior(),
            no_match_behavior: default_no_match_behavior(),
            wheel_cycle_gate: default_wheel_cycle_gate(),
            practice_mode: default_practice_mode(),
            ignore_window_titles: Vec::new(),
        }
    }
}

fn default_enabled() -> bool {
    true
}

fn default_debug_logging() -> bool {
    false
}

fn default_show_trail() -> bool {
    true
}

fn default_trail_color() -> [u8; 4] {
    [0xff, 0x00, 0x00, 0xff]
}

fn default_trail_width() -> f32 {
    2.0
}

fn default_trail_start_move_px() -> f32 {
    8.0
}

fn default_trail_interval_ms() -> u64 {
    DEFAULT_TRAIL_INTERVAL_MS
}

fn default_recognition_interval_ms() -> u64 {
    DEFAULT_RECOGNITION_INTERVAL_MS
}

fn default_show_hint() -> bool {
    true
}

fn default_hint_offset() -> (f32, f32) {
    (16.0, 16.0)
}

fn default_cancel_behavior() -> CancelBehavior {
    CancelBehavior::DoNothing
}

fn default_no_match_behavior() -> NoMatchBehavior {
    NoMatchBehavior::PassThroughClick
}

fn default_wheel_cycle_gate() -> WheelCycleGate {
    WheelCycleGate::Deadzone
}

fn default_practice_mode() -> bool {
    false
}

pub fn normalize_ignore_window_titles(values: &mut Vec<String>) -> bool {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for value in values.iter() {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.to_lowercase();
        if seen.insert(key) {
            normalized.push(trimmed.to_string());
        }
    }
    let changed = normalized != *values;
    if changed {
        *values = normalized;
    }
    changed
}

pub fn add_ignore_window_title(values: &mut Vec<String>, title: &str) -> bool {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return false;
    }
    if values
        .iter()
        .any(|entry| entry.trim().eq_ignore_ascii_case(trimmed))
    {
        return false;
    }
    values.push(trimmed.to_string());
    normalize_ignore_window_titles(values);
    true
}

pub fn collect_visible_window_titles() -> anyhow::Result<Vec<String>> {
    #[cfg(windows)]
    {
        use crate::windows_layout::{LayoutWindowOptions, collect_layout_windows};
        let windows = collect_layout_windows(LayoutWindowOptions::default())?;
        let mut seen = HashSet::new();
        let mut titles = Vec::new();
        for window in windows {
            let Some(title) = window.matcher.title else {
                continue;
            };
            let trimmed = title.trim();
            if trimmed.is_empty() {
                continue;
            }
            let key = trimmed.to_lowercase();
            if seen.insert(key) {
                titles.push(trimmed.to_string());
            }
        }
        titles.sort_by_key(|a| a.to_lowercase());
        Ok(titles)
    }
    #[cfg(not(windows))]
    {
        Ok(Vec::new())
    }
}

#[derive(Debug)]
struct MouseGestureRuntime {
    settings: MouseGestureSettings,
    plugin_enabled: bool,
    db: SharedGestureDb,
    directory_revision: u64,
    #[allow(dead_code)]
    watcher: Option<crate::common::json_watch::JsonWatcher>,
}

impl Default for MouseGestureRuntime {
    fn default() -> Self {
        let initial_db = match load_gestures(GESTURES_FILE) {
            Ok(db) => db,
            Err(error) => {
                tracing::error!(
                    ?error,
                    "failed to load mouse gestures; using temporary defaults"
                );
                GestureDb::default()
            }
        };
        let db = Arc::new(Mutex::new(initial_db));
        let db_for_watcher = Arc::clone(&db);

        let watcher = watch_json(GESTURES_FILE, move || {
            let mut result: anyhow::Result<bool> = Ok(false);
            with_service(|service| {
                result = reload_gesture_runtime_database(service, GESTURES_FILE, &db_for_watcher);
            });
            match result {
                Ok(true) => {
                    crate::gui::send_event(crate::gui::WatchEvent::HotkeyDirectorySource(
                        hotkey_directory_source(),
                    ));
                }
                Ok(false) => {}
                Err(error) => tracing::error!(
                    ?error,
                    "failed to reload mouse gestures; retaining last-good data"
                ),
            }
        })
        .ok();

        Self {
            settings: MouseGestureSettings::default(),
            plugin_enabled: true,
            db,
            directory_revision: 0,
            watcher,
        }
    }
}

fn reload_shared_gesture_db(
    path: impl AsRef<std::path::Path>,
    shared: &SharedGestureDb,
) -> anyhow::Result<bool> {
    let _transaction = crate::mouse_gestures::db::gestures_transaction_guard();
    let loaded = match crate::mouse_gestures::db::load_gestures_typed(path.as_ref())? {
        crate::common::persistence::LoadState::Missing => {
            anyhow::bail!("mouse gesture definitions were removed; retaining last-good state")
        }
        crate::common::persistence::LoadState::Empty => GestureDb::default(),
        crate::common::persistence::LoadState::Loaded(db) => db,
    };
    let mut current = shared
        .lock()
        .map_err(|_| anyhow::anyhow!("mouse gesture database lock poisoned"))?;
    if *current == loaded {
        return Ok(false);
    }
    *current = loaded;
    Ok(true)
}

fn reload_gesture_runtime_database(
    runtime: &mut MouseGestureRuntime,
    path: impl AsRef<std::path::Path>,
    expected_db: &SharedGestureDb,
) -> anyhow::Result<bool> {
    if !Arc::ptr_eq(&runtime.db, expected_db) {
        return Ok(false);
    }
    let changed = reload_shared_gesture_db(path, &runtime.db)?;
    if changed {
        runtime.bump_directory_revision();
    }
    Ok(changed)
}

impl MouseGestureRuntime {
    fn update_settings(&mut self, settings: MouseGestureSettings) {
        if self.settings != settings {
            self.bump_directory_revision();
        }
        self.settings = settings;
        self.apply();
    }

    fn set_plugin_enabled(&mut self, enabled: bool) {
        if self.plugin_enabled != enabled {
            self.bump_directory_revision();
        }
        self.plugin_enabled = enabled;
        self.apply();
    }

    fn bump_directory_revision(&mut self) {
        self.directory_revision = self.directory_revision.checked_add(1).unwrap_or(1);
    }

    fn apply(&self) {
        let config = settings_to_config(&self.settings, self.plugin_enabled);
        with_gesture_service(|svc| {
            svc.update_config(config);
            svc.update_db(Some(self.db.clone()));
        });
    }
}

fn settings_to_config(settings: &MouseGestureSettings, plugin_enabled: bool) -> MouseGestureConfig {
    let mut config = MouseGestureConfig::default();
    config.enabled = settings.enabled && plugin_enabled;
    config.debug_logging = settings.debug_logging;
    config.trail_start_move_px = settings.trail_start_move_px;
    config.trail_interval_ms = settings.trail_interval_ms;
    config.recognition_interval_ms = settings.recognition_interval_ms;
    config.show_trail = settings.show_trail;
    config.trail_color = settings.trail_color;
    config.trail_width = settings.trail_width;
    config.show_hint = settings.show_hint;
    config.hint_offset = settings.hint_offset;
    config.cancel_behavior = settings.cancel_behavior;
    config.no_match_behavior = settings.no_match_behavior;
    config.wheel_cycle_gate = settings.wheel_cycle_gate;
    config.practice_mode = settings.practice_mode;
    config.ignore_window_titles = settings.ignore_window_titles.clone();
    config
}

static SERVICE: OnceCell<Mutex<MouseGestureRuntime>> = OnceCell::new();

fn with_service<F>(f: F)
where
    F: FnOnce(&mut MouseGestureRuntime),
{
    let service = SERVICE.get_or_init(|| Mutex::new(MouseGestureRuntime::default()));
    match service.lock() {
        Ok(mut guard) => f(&mut guard),
        Err(e) => tracing::error!(?e, "failed to lock mouse gestures runtime"),
    }
}

fn gesture_db_snapshot() -> GestureDb {
    let mut snapshot = GestureDb::default();
    with_service(|service| {
        if let Ok(db) = service.db.lock() {
            snapshot = db.clone();
        }
    });
    snapshot
}

pub fn publish_committed_gesture_db(committed: GestureDb) {
    with_service(|service| {
        let changed = if let Ok(mut db) = service.db.lock() {
            if *db != committed {
                *db = committed;
                true
            } else {
                false
            }
        } else {
            false
        };
        if changed {
            service.bump_directory_revision();
        }
    });
}

/// Returns a sanitized view of configured gesture bindings for the hotkey
/// directory. Gesture labels, tokens, action text, arguments, and stroke data
/// remain inside the gesture owner; stable identities are scoped to this
/// immutable database publication.
pub fn hotkey_directory_source() -> crate::hotkey::directory::SourceSnapshot {
    let mut snapshot = None;
    with_service(|service| {
        snapshot = Some(hotkey_directory_source_from_runtime(service));
    });
    snapshot.unwrap_or_else(|| {
        crate::hotkey::directory::SourceSnapshot::new(
            crate::hotkey::directory::Source::MouseGestures,
            0,
            Vec::new(),
            Vec::new(),
        )
    })
}

fn hotkey_directory_source_from_runtime(
    service: &MouseGestureRuntime,
) -> crate::hotkey::directory::SourceSnapshot {
    use crate::hotkey::directory::{
        Candidate, CandidateState, EditDestination, EntryId, Owner, Reason, RouteKind, Scope,
        Source, SourceSnapshot,
    };
    let revision = service.directory_revision;
    let plugin_enabled = service.plugin_enabled;
    let gestures_enabled = service.settings.enabled;
    let database = service.db.lock().map(|db| db.clone()).unwrap_or_default();

    let mut candidates = Vec::new();
    for (gesture_index, gesture) in database.gestures.iter().enumerate() {
        for (binding_index, binding) in gesture.bindings.iter().enumerate() {
            let enabled = plugin_enabled && gestures_enabled && gesture.enabled && binding.enabled;
            let id = format!("gesture:{revision}:{gesture_index}:{binding_index}");
            candidates.push(Candidate {
                id: EntryId::new(id),
                owner: Owner::MouseGestures,
                owner_label: "Mouse Gestures".into(),
                action_label: format!("{} — binding {}", gesture.label, binding_index + 1),
                chord: None,
                route: RouteKind::Gesture,
                scope: Scope::Gesture,
                source_revision: revision,
                expected_runtime_generation: None,
                state: if enabled {
                    CandidateState::Unknown
                } else {
                    CandidateState::Disabled
                },
                reason: if enabled {
                    Some(Reason::LocalRouteHasNoGlobalReceipt)
                } else {
                    Some(Reason::OwnerDisabled)
                },
                edit: Some(EditDestination::GestureBinding {
                    publication: revision,
                    gesture_index: gesture_index as u32,
                    binding_index: binding_index as u32,
                }),
            });
        }
    }
    SourceSnapshot::new(Source::MouseGestures, revision, candidates, Vec::new())
}

pub fn apply_runtime_settings(settings: MouseGestureSettings) {
    with_service(|svc| svc.update_settings(settings));
}

pub fn sync_enabled_plugins(enabled_plugins: Option<&HashSet<String>>) {
    let enabled = enabled_plugins
        .map(|set| set.contains(PLUGIN_NAME))
        .unwrap_or(true);
    with_service(|svc| svc.set_plugin_enabled(enabled));
}

#[derive(Default)]
pub struct MouseGesturesPlugin {
    settings: MouseGestureSettings,
    ignore_input: String,
    window_picker_open: bool,
    window_picker_titles: Vec<String>,
    window_picker_error: Option<String>,
}

impl MouseGesturesPlugin {
    fn command_actions() -> Vec<Action> {
        vec![
            Action {
                label: "mg".into(),
                desc: "Mouse gestures".into(),
                action: "query:mg ".into(),
                args: None,
            },
            Action {
                label: "mg settings".into(),
                desc: "Mouse gestures".into(),
                action: "mg:dialog:settings".into(),
                args: None,
            },
            Action {
                label: "mg edit".into(),
                desc: "Mouse gestures".into(),
                action: "mg:dialog".into(),
                args: None,
            },
            Action {
                label: "mg add".into(),
                desc: "Mouse gestures".into(),
                action: "mg:dialog:binding".into(),
                args: None,
            },
            Action {
                label: "mg list".into(),
                desc: "Mouse gestures".into(),
                action: "query:mg list".into(),
                args: None,
            },
            Action {
                label: "mg find".into(),
                desc: "Mouse gestures".into(),
                action: "query:mg find ".into(),
                args: None,
            },
            Action {
                label: "mg where".into(),
                desc: "Mouse gestures".into(),
                action: "query:mg where ".into(),
                args: None,
            },
            Action {
                label: "mg conflicts".into(),
                desc: "Mouse gestures".into(),
                action: "query:mg conflicts".into(),
                args: None,
            },
        ]
    }

    fn list_gestures(filter: &str) -> Vec<Action> {
        let db = gesture_db_snapshot();
        let matcher = SkimMatcherV2::default();
        let filter = filter.trim().to_lowercase();
        db.gestures
            .iter()
            .filter(|gesture| {
                if filter.is_empty() {
                    return true;
                }
                let label = format_gesture_label(gesture).to_lowercase();
                matcher.fuzzy_match(&label, &filter).is_some()
            })
            .map(|gesture| Action {
                label: format_gesture_label(gesture),
                desc: "Mouse gestures".into(),
                action: "mg:dialog".into(),
                args: None,
            })
            .collect()
    }

    fn format_match_desc(context: &BindingMatchContext) -> String {
        if context.fields.is_empty() {
            return "Mouse gestures".into();
        }
        let labels: Vec<&str> = context
            .fields
            .iter()
            .map(|field| match field {
                crate::mouse_gestures::db::BindingMatchField::GestureLabel => "gesture label",
                crate::mouse_gestures::db::BindingMatchField::Tokens => "tokens",
                crate::mouse_gestures::db::BindingMatchField::BindingLabel => "binding label",
                crate::mouse_gestures::db::BindingMatchField::Action => "action",
                crate::mouse_gestures::db::BindingMatchField::Args => "args",
            })
            .collect();
        format!("Mouse gestures (matches: {})", labels.join(", "))
    }
}

impl Plugin for MouseGesturesPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if trimmed.eq_ignore_ascii_case("mg") {
            return Self::command_actions();
        }
        if strip_prefix_ci(trimmed, "mg settings").is_some()
            || strip_prefix_ci(trimmed, "mg setting").is_some()
        {
            return vec![Action {
                label: "Open mouse gesture settings".into(),
                desc: "Mouse gestures".into(),
                action: "mg:dialog:settings".into(),
                args: None,
            }];
        }
        if strip_prefix_ci(trimmed, "mg edit").is_some() {
            return vec![Action {
                label: "Edit mouse gestures".into(),
                desc: "Mouse gestures".into(),
                action: "mg:dialog".into(),
                args: None,
            }];
        }
        if strip_prefix_ci(trimmed, "mg add").is_some() {
            return vec![Action {
                label: "Add mouse gesture binding".into(),
                desc: "Mouse gestures".into(),
                action: "mg:dialog:binding".into(),
                args: None,
            }];
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "mg find") {
            let query = rest.trim();
            let db = gesture_db_snapshot();
            return db
                .search_bindings(query)
                .into_iter()
                .map(|(gesture, binding, context)| Action {
                    label: format_search_result_label(&gesture, &binding),
                    desc: Self::format_match_desc(&context),
                    action: "mg:dialog".into(),
                    args: None,
                })
                .collect();
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "mg where") {
            let action_prefix = rest.trim();
            let db = gesture_db_snapshot();
            return db
                .find_by_action(action_prefix)
                .into_iter()
                .map(|(gesture, binding)| Action {
                    label: format_search_result_label(&gesture, &binding),
                    desc: "Mouse gestures".into(),
                    action: "mg:dialog".into(),
                    args: None,
                })
                .collect();
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "mg conflicts") {
            if !rest.trim().is_empty() {
                return Vec::new();
            }
            let db = gesture_db_snapshot();
            let mut actions = Vec::new();
            for conflict in db.find_conflicts() {
                let conflict_desc = match conflict.kind {
                    crate::mouse_gestures::db::GestureConflictKind::DuplicateTokens => {
                        "Mouse gestures (conflict: duplicate tokens)"
                    }
                    crate::mouse_gestures::db::GestureConflictKind::PrefixOverlap => {
                        "Mouse gestures (conflict: prefix overlap)"
                    }
                };
                for gesture in conflict.gestures {
                    for binding in gesture.bindings.iter().filter(|binding| binding.enabled) {
                        actions.push(Action {
                            label: format_search_result_label(&gesture, binding),
                            desc: conflict_desc.into(),
                            action: "mg:dialog".into(),
                            args: None,
                        });
                    }
                }
            }
            return actions;
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "mg list") {
            return Self::list_gestures(rest);
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "mg ") {
            return Self::list_gestures(rest);
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        PLUGIN_NAME
    }

    fn description(&self) -> &str {
        "Handle mouse gestures (prefix: `mg`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        Self::command_actions()
    }

    fn default_settings(&self) -> Option<serde_json::Value> {
        serde_json::to_value(MouseGestureSettings::default()).ok()
    }

    fn apply_settings(&mut self, value: &serde_json::Value) {
        if let Ok(settings) = serde_json::from_value::<MouseGestureSettings>(value.clone()) {
            self.settings = settings.clone();
            apply_runtime_settings(settings);
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui, value: &mut serde_json::Value) {
        use crate::gui::theme::widgets::{
            ButtonEmphasis, StatusTone, compact_section_heading, labeled_form_row, semantic_status,
            with_button_emphasis,
        };

        let mut cfg =
            serde_json::from_value::<MouseGestureSettings>(value.clone()).unwrap_or_default();

        let mut changed = false;

        compact_section_heading(ui, "Gesture behavior", None);
        changed |= ui
            .checkbox(&mut cfg.enabled, "Enable mouse gestures")
            .changed();
        changed |= ui
            .checkbox(&mut cfg.debug_logging, "Enable debug logging")
            .changed();

        changed |= ui
            .checkbox(&mut cfg.show_trail, "Show trail overlay")
            .changed();

        labeled_form_row(ui, "Trail color", |ui| {
            let mut color = egui::Color32::from_rgba_unmultiplied(
                cfg.trail_color[0],
                cfg.trail_color[1],
                cfg.trail_color[2],
                cfg.trail_color[3],
            );

            let resp = ui.color_edit_button_srgba(&mut color);
            if resp.changed() {
                cfg.trail_color = [color.r(), color.g(), color.b(), color.a()];
                changed = true;
            }
            resp
        });

        labeled_form_row(ui, "Trail width", |ui| {
            let response = ui.add(
                egui::DragValue::new(&mut cfg.trail_width)
                    .range(1.0..=20.0)
                    .speed(0.5),
            );
            changed |= response.changed();
            response
        });

        changed |= ui
            .checkbox(&mut cfg.show_hint, "Show hint overlay")
            .changed();

        labeled_form_row(ui, "Hint offset (x, y)", |ui| {
            let first = ui.add(
                egui::DragValue::new(&mut cfg.hint_offset.0)
                    .range(-200.0..=200.0)
                    .speed(1.0),
            );
            changed |= first.changed();
            let second = ui.add(
                egui::DragValue::new(&mut cfg.hint_offset.1)
                    .range(-200.0..=200.0)
                    .speed(1.0),
            );
            changed |= second.changed();
            second
        });

        labeled_form_row(ui, "Cancel behavior", |ui| {
            egui::ComboBox::from_id_salt("mg_cancel_behavior")
                .selected_text(cancel_behavior_label(cfg.cancel_behavior))
                .show_ui(ui, |ui| {
                    changed |= ui
                        .selectable_value(
                            &mut cfg.cancel_behavior,
                            CancelBehavior::DoNothing,
                            "Do nothing",
                        )
                        .changed();
                    changed |= ui
                        .selectable_value(
                            &mut cfg.cancel_behavior,
                            CancelBehavior::PassThroughClick,
                            "Pass through right-click",
                        )
                        .changed();
                })
                .response
        });

        labeled_form_row(ui, "No-match behavior", |ui| {
            egui::ComboBox::from_id_salt("mg_no_match_behavior")
                .selected_text(no_match_behavior_label(cfg.no_match_behavior))
                .show_ui(ui, |ui| {
                    changed |= ui
                        .selectable_value(
                            &mut cfg.no_match_behavior,
                            NoMatchBehavior::DoNothing,
                            "Do nothing",
                        )
                        .changed();
                    changed |= ui
                        .selectable_value(
                            &mut cfg.no_match_behavior,
                            NoMatchBehavior::PassThroughClick,
                            "Pass through right-click",
                        )
                        .changed();
                    changed |= ui
                        .selectable_value(
                            &mut cfg.no_match_behavior,
                            NoMatchBehavior::ShowNoMatchHint,
                            "Show no-match hint",
                        )
                        .changed();
                })
                .response
        });
        ui.small(
            "Fallback runs when a gesture does not match; default is pass-through right-click.",
        );

        ui.separator();
        compact_section_heading(
            ui,
            "Ignore windows (disable gestures)",
            Some(
                "Gestures will be ignored when the active window title contains one of these entries.",
            ),
        );

        let mut remove_index: Option<usize> = None;
        if cfg.ignore_window_titles.is_empty() {
            ui.label("No ignored windows.");
        } else {
            for (index, title) in cfg.ignore_window_titles.iter().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(title);
                    let remove = ui.button("Remove");
                    if with_button_emphasis(ui, remove, ButtonEmphasis::Destructive).clicked() {
                        remove_index = Some(index);
                    }
                });
            }
        }
        if let Some(index) = remove_index {
            cfg.ignore_window_titles.remove(index);
            changed = true;
        }

        ui.horizontal_wrapped(|ui| {
            let response = ui.text_edit_singleline(&mut self.ignore_input);
            let add_button = ui.button("Add");
            let mut add_now =
                with_button_emphasis(ui, add_button, ButtonEmphasis::Primary).clicked();
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                add_now = true;
            }
            if add_now {
                if add_ignore_window_title(&mut cfg.ignore_window_titles, &self.ignore_input) {
                    changed = true;
                }
                self.ignore_input.clear();
            }
        });

        let window_button = ui.add_enabled(cfg!(windows), egui::Button::new("Select window..."));
        if !cfg!(windows) {
            window_button.on_hover_text("Window picker is only available on Windows.");
        } else if window_button.clicked() {
            self.window_picker_error = None;
            match collect_visible_window_titles() {
                Ok(titles) => {
                    self.window_picker_titles = titles;
                    self.window_picker_open = true;
                }
                Err(err) => {
                    self.window_picker_error = Some(format!("Failed to enumerate windows: {err}"));
                    self.window_picker_titles.clear();
                    self.window_picker_open = true;
                }
            }
        }

        if self.window_picker_open {
            let mut open = self.window_picker_open;
            egui::Window::new("Select window to ignore")
                .open(&mut open)
                .resizable(true)
                .show(ui.ctx(), |ui| {
                    if let Some(err) = &self.window_picker_error {
                        semantic_status(ui, StatusTone::Error, "Window picker error", Some(err));
                        ui.separator();
                    }

                    if self.window_picker_titles.is_empty() {
                        ui.label("No windows found.");
                    } else {
                        egui::ScrollArea::vertical()
                            .max_height(220.0)
                            .show(ui, |ui| {
                                for title in self.window_picker_titles.clone() {
                                    ui.horizontal(|ui| {
                                        ui.label(&title);
                                        if ui.button("Add").clicked()
                                            && add_ignore_window_title(
                                                &mut cfg.ignore_window_titles,
                                                &title,
                                            )
                                        {
                                            changed = true;
                                        }
                                    });
                                }
                            });
                    }
                });
            self.window_picker_open = open;
        }

        if normalize_ignore_window_titles(&mut cfg.ignore_window_titles) {
            changed = true;
        }

        // Only write+apply when something changed.
        if changed {
            match serde_json::to_value(&cfg) {
                Ok(v) => *value = v,
                Err(e) => tracing::error!(?e, "failed to serialize mouse gesture settings"),
            }
            self.settings = cfg.clone();
            apply_runtime_settings(cfg);
        }
    }
}

fn cancel_behavior_label(value: CancelBehavior) -> &'static str {
    match value {
        CancelBehavior::DoNothing => "Do nothing",
        CancelBehavior::PassThroughClick => "Pass through right-click",
    }
}

fn no_match_behavior_label(value: NoMatchBehavior) -> &'static str {
    match value {
        NoMatchBehavior::DoNothing => "Do nothing",
        NoMatchBehavior::PassThroughClick => "Pass through right-click",
        NoMatchBehavior::ShowNoMatchHint => "Show no-match hint",
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    use crate::mouse_gestures::db::{
        BindingEntry, BindingKind, GestureDb, GestureEntry, SCHEMA_VERSION,
    };
    use crate::mouse_gestures::engine::DirMode;

    fn db(label: &str) -> GestureDb {
        GestureDb {
            schema_version: SCHEMA_VERSION,
            gestures: vec![GestureEntry {
                label: label.into(),
                tokens: "L".into(),
                dir_mode: DirMode::Four,
                stroke: Vec::new(),
                enabled: true,
                bindings: Vec::new(),
            }],
        }
    }

    #[test]
    fn mouse_gesture_timing_defaults_and_round_trip() {
        let old_settings: MouseGestureSettings = serde_json::from_value(serde_json::json!({
            "enabled": true,
            "show_trail": true,
        }))
        .unwrap();

        assert_eq!(old_settings.trail_interval_ms, 16);
        assert_eq!(old_settings.recognition_interval_ms, 40);

        let default_config = MouseGestureConfig::default();
        assert_eq!(default_config.trail_interval_ms, 16);
        assert_eq!(default_config.recognition_interval_ms, 40);

        let mut custom_settings = old_settings;
        custom_settings.trail_interval_ms = 8;
        custom_settings.recognition_interval_ms = 20;
        let round_trip: MouseGestureSettings =
            serde_json::from_value(serde_json::to_value(&custom_settings).unwrap()).unwrap();

        assert_eq!(round_trip, custom_settings);
        assert_eq!(round_trip.trail_interval_ms, 8);
        assert_eq!(round_trip.recognition_interval_ms, 20);
    }

    #[test]
    fn mouse_gesture_timing_runtime_mapping() {
        let mut settings = MouseGestureSettings::default();
        settings.trail_interval_ms = 8;
        settings.recognition_interval_ms = 20;

        let mapped = settings_to_config(&settings, true);
        assert!(mapped.enabled);
        assert_eq!(mapped.trail_interval_ms, 8);
        assert_eq!(mapped.recognition_interval_ms, 20);

        settings.trail_interval_ms = 40;
        settings.recognition_interval_ms = 8;
        let reversed = settings_to_config(&settings, true);
        assert_eq!(reversed.trail_interval_ms, 40);
        assert_eq!(reversed.recognition_interval_ms, 8);

        assert!(!settings_to_config(&settings, false).enabled);
        settings.enabled = false;
        assert!(!settings_to_config(&settings, true).enabled);
    }

    #[test]
    fn runtime_reload_retains_last_good_recovers_and_deduplicates() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mouse_gestures.json");
        let initial = db("initial");
        let shared = Arc::new(Mutex::new(initial.clone()));

        std::fs::write(&path, b"{broken").unwrap();
        assert!(reload_shared_gesture_db(&path, &shared).is_err());
        assert_eq!(*shared.lock().unwrap(), initial);

        std::fs::remove_file(&path).unwrap();
        assert!(reload_shared_gesture_db(&path, &shared).is_err());
        assert_eq!(*shared.lock().unwrap(), initial);

        crate::common::persistence::save_json_atomic(&path, &initial).unwrap();
        assert!(!reload_shared_gesture_db(&path, &shared).unwrap());

        let recovered = db("recovered");
        crate::common::persistence::save_json_atomic(&path, &recovered).unwrap();
        assert!(reload_shared_gesture_db(&path, &shared).unwrap());
        assert_eq!(*shared.lock().unwrap(), recovered);
    }

    #[test]
    fn hotkey_directory_publication_pairs_revision_with_database_reload() {
        fn db_with_binding(label: &str) -> GestureDb {
            let mut database = db(label);
            database.gestures[0].bindings.push(BindingEntry {
                label: "Harmless test binding".into(),
                kind: BindingKind::Execute,
                action: "test:inert-binding".into(),
                args: None,
                enabled: true,
            });
            database
        }

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mouse_gestures.json");
        let shared = Arc::new(Mutex::new(db_with_binding("before")));
        let mut runtime = MouseGestureRuntime {
            settings: MouseGestureSettings::default(),
            plugin_enabled: true,
            db: Arc::clone(&shared),
            directory_revision: 7,
            watcher: None,
        };
        let before = hotkey_directory_source_from_runtime(&runtime);
        assert_eq!(before.revision, 7);
        assert!(before.candidates[0].id.as_str().contains(":7:"));
        assert_eq!(before.candidates[0].action_label, "before — binding 1");

        crate::common::persistence::save_json_atomic(&path, &db_with_binding("after")).unwrap();
        assert!(reload_gesture_runtime_database(&mut runtime, &path, &shared).unwrap());
        let after = hotkey_directory_source_from_runtime(&runtime);
        assert_eq!(after.revision, 8);
        assert!(after.candidates[0].id.as_str().contains(":8:"));
        assert_eq!(after.candidates[0].action_label, "after — binding 1");

        // A duplicate file notification is a no-op and cannot diverge the
        // immutable source from its owner revision.
        assert!(!reload_gesture_runtime_database(&mut runtime, &path, &shared).unwrap());
        let duplicate = hotkey_directory_source_from_runtime(&runtime);
        assert_eq!(duplicate.revision, after.revision);
        assert_eq!(duplicate.candidates, after.candidates);
    }
}
