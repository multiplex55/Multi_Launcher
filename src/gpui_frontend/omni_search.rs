//! Retained Omni projection over the launcher's existing watched owners.
use super::*;
use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::omni_search::{
    OmniAppCatalog, OmniRow, OmniSearchSettings, OmniSource, OmniSources, project_omni,
};
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, RwLock};

pub(super) fn is_omni_query(raw: &str) -> bool {
    let effective = multi_launcher::common::query::split_action_filters(raw).0;
    effective
        .split_whitespace()
        .next()
        .is_some_and(|head| head.eq_ignore_ascii_case("o"))
}

pub(super) fn notes_enabled(
    enabled: Option<&HashSet<String>>,
    capabilities: Option<&HashMap<String, Vec<String>>>,
    include_notes: bool,
) -> bool {
    include_notes
        && enabled.is_none_or(|enabled| enabled.contains("omni_search"))
        && multi_launcher::settings::provider_search_capability_enabled(capabilities, "omni_search")
}

fn action_hash(action: &Action) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    action.label.hash(&mut hasher);
    action.desc.hash(&mut hasher);
    action.action.hash(&mut hasher);
    action.args.hash(&mut hasher);
    hasher.finish()
}
pub(super) struct Projection {
    query: String,
    rows: HashMap<u64, Vec<OmniRow>>,
    notes: Option<Arc<super::search_providers::NoteSnapshot>>,
    todos: Option<(Arc<multi_launcher::plugins::todo::CheckedTodoOwner>, Arc<multi_launcher::plugins::todo::CheckedTodoSnapshot>)>,
    calendar: Option<(Arc<multi_launcher::plugins::calendar::CheckedCalendarOwner>, Arc<multi_launcher::plugins::calendar::CheckedCalendarSnapshot>)>,
    settings_revision: (Arc<AtomicU64>, u64),
}
impl Projection {
    pub fn calendar_stamp(&self) -> Option<(Arc<multi_launcher::plugins::calendar::CheckedCalendarOwner>, Arc<multi_launcher::plugins::calendar::CheckedCalendarSnapshot>)> {
        self.calendar.clone()
    }
    pub fn source_for(&self, action: &Action) -> Option<OmniSource> {
        let source = self
            .rows
            .get(&action_hash(action))?
            .iter()
            .find(|row| &row.action == action)
            .map(|row| row.source.clone())?;
        if source == OmniSource::Note && self.note_for(action).is_none() {
            return None;
        }
        Some(source)
    }
    pub fn note_for(
        &self,
        action: &Action,
    ) -> Option<(u64, Arc<multi_launcher::plugins::note::Note>)> {
        let snapshot = self.notes.as_ref()?;
        if snapshot.version != multi_launcher::plugins::note::note_version() {
            return None;
        }
        let Command::Note(multi_launcher::commands::NoteCommand::Open { slug }) =
            parse_action(action).ok()?
        else {
            return None;
        };
        Some((snapshot.version, snapshot.by_slug.get(&slug)?.clone()))
    }
    pub fn todo_for(&self, action: &Action) -> Option<(Arc<multi_launcher::plugins::todo::CheckedTodoOwner>, multi_launcher::plugins::todo::CheckedTodoReceipt, Arc<AtomicU64>, u64)> {
        if self.settings_revision.0.load(Ordering::Acquire) != self.settings_revision.1 { return None; }
        let (owner, snapshot) = self.todos.as_ref()?;
        Some((owner.clone(), owner.receipt(snapshot, action)?, self.settings_revision.0.clone(), self.settings_revision.1))
    }
    pub fn calendar_for(&self, action: &Action) -> Option<(Arc<multi_launcher::plugins::calendar::CheckedCalendarOwner>, Arc<multi_launcher::plugins::calendar::CheckedCalendarSnapshot>, Arc<AtomicU64>, u64)> {
        if self.settings_revision.0.load(Ordering::Acquire) != self.settings_revision.1 || self.source_for(action) != Some(OmniSource::Calendar) { return None; }
        let (owner, snapshot) = self.calendar.as_ref()?;
        if !snapshot.actionable || !Arc::ptr_eq(&owner.snapshot(), snapshot) { return None; }
        Some((owner.clone(), snapshot.clone(), self.settings_revision.0.clone(), self.settings_revision.1))
    }
}

pub(super) fn supported_child(source: &OmniSource, action: &Action) -> bool {
    match source {
        OmniSource::Folder => {
            super::external_launch::provider_association_by_name("folders", action).is_some()
        }
        OmniSource::Bookmark => {
            super::external_launch::provider_association_by_name("bookmarks", action).is_some()
        }
        OmniSource::App { .. } => parse_action(action).is_ok_and(|command| {
            super::external_launch::ordinary_external(&command).is_some()
                || super::external_launch::document_external(action, &command).is_some()
                || super::external_launch::url_external(action, &command).is_some()
                || super::saved_folder::candidate(action).is_some()
                || super::indexed_applications::supported_shape(action)
                || {
                    #[cfg(windows)]
                    {
                        super::external_launch::saved_shortcut(action).is_some()
                    }
                    #[cfg(not(windows))]
                    {
                        false
                    }
                }
        }),
        OmniSource::Note => matches!(
            parse_action(action),
            Ok(Command::Note(
                multi_launcher::commands::NoteCommand::Open { .. }
            ))
        ),
        OmniSource::Todo => matches!(parse_action(action), Ok(Command::Todo(multi_launcher::commands::TodoCommand::Done { .. }))),
        OmniSource::Calendar => matches!(parse_action(action), Ok(Command::Calendar(multi_launcher::commands::CalendarCommand::Open { .. } | multi_launcher::commands::CalendarCommand::Search { .. } | multi_launcher::commands::CalendarCommand::Upcoming))),
    }
}

pub(super) struct Provider {
    folders: Option<Arc<multi_launcher::plugins::folders::FoldersPlugin>>,
    bookmarks: Option<Arc<multi_launcher::plugins::bookmarks::BookmarksPlugin>>,
    notes: Arc<Mutex<Option<Arc<multi_launcher::plugins::note::NotePlugin>>>>,
    note_snapshots: Arc<super::search_providers::NoteSnapshots>,
    settings: RwLock<OmniSearchSettings>,
    catalog: RwLock<Option<Arc<OmniAppCatalog>>>,
    projected: Mutex<Option<Arc<Projection>>>,
    todos: RwLock<Option<Arc<Mutex<Option<Result<Arc<multi_launcher::plugins::todo::CheckedTodoOwner>, String>>>>>>,
    calendar: RwLock<Option<Arc<Mutex<Option<Result<Arc<multi_launcher::plugins::calendar::CheckedCalendarOwner>, String>>>>>>,
    settings_revision: Arc<AtomicU64>,
}

impl Provider {
    pub fn new(
        folders: Option<Arc<multi_launcher::plugins::folders::FoldersPlugin>>,
        bookmarks: Option<Arc<multi_launcher::plugins::bookmarks::BookmarksPlugin>>,
        notes: Arc<Mutex<Option<Arc<multi_launcher::plugins::note::NotePlugin>>>>,
        note_snapshots: Arc<super::search_providers::NoteSnapshots>,
    ) -> Self {
        Self {
            folders,
            bookmarks,
            notes,
            note_snapshots,
            settings: RwLock::new(OmniSearchSettings::default()),
            catalog: RwLock::new(None),
            projected: Mutex::new(None),
            todos: RwLock::new(None),
            calendar: RwLock::new(None),
            settings_revision: Arc::new(AtomicU64::new(1)),
        }
    }
    pub fn attach_todos(&self, owner: Arc<Mutex<Option<Result<Arc<multi_launcher::plugins::todo::CheckedTodoOwner>, String>>>>) {
        if let Ok(mut slot) = self.todos.write() { *slot = Some(owner); }
    }
    pub fn attach_calendar(&self, owner: Arc<Mutex<Option<Result<Arc<multi_launcher::plugins::calendar::CheckedCalendarOwner>, String>>>>) {
        if let Ok(mut slot) = self.calendar.write() { *slot = Some(owner); }
    }
    pub fn configure(&self, value: Option<&serde_json::Value>) {
        if let Some(settings) = value.and_then(|value| serde_json::from_value(value.clone()).ok())
            && let Ok(mut current) = self.settings.write()
        {
            *current = settings;
            self.settings_revision.fetch_add(1, Ordering::AcqRel);
        }
    }
    pub fn install(&self, catalog: Option<Arc<OmniAppCatalog>>) {
        if let Ok(mut current) = self.catalog.write() {
            *current = catalog;
        }
    }
    pub fn settings(&self) -> OmniSearchSettings {
        self.settings
            .read()
            .map(|value| value.clone())
            .unwrap_or_default()
    }
    pub fn projection(&self, raw_query: &str) -> Option<Arc<Projection>> {
        let effective = multi_launcher::common::query::split_action_filters(raw_query).0;
        self.projected
            .lock()
            .ok()?
            .as_ref()
            .filter(|value| value.query == effective)
            .cloned()
    }
    pub fn source_for(&self, raw_query: &str, action: &Action) -> Option<OmniSource> {
        self.projection(raw_query)?.source_for(action)
    }
}

#[derive(Default)]
pub(super) struct Owner {
    generation: u64,
    current: Option<Arc<AtomicBool>>,
}
impl Owner {
    fn begin(&mut self) -> (u64, Arc<AtomicBool>) {
        if let Some(old) = self.current.take() {
            old.store(false, Ordering::Release);
        }
        self.generation = self.generation.wrapping_add(1).max(1);
        let current = Arc::new(AtomicBool::new(true));
        self.current = Some(current.clone());
        (self.generation, current)
    }
    pub fn authority(&self, generation: u64) -> Option<Arc<AtomicBool>> {
        (self.generation == generation)
            .then(|| self.current.clone())
            .flatten()
            .filter(|current| current.load(Ordering::Acquire))
    }
    pub fn current_authority(&self) -> Option<Arc<AtomicBool>> {
        self.authority(self.generation)
    }
    pub fn retire(&mut self) {
        if let Some(current) = self.current.take() {
            current.store(false, Ordering::Release);
        }
    }
}

impl RootWindow {
    pub(super) fn start_omni_watch(&mut self, cx: &mut Context<Self>) {
        let signal = self.search_catalog.providers.omni_wake.clone();
        let root = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            while signal.wait().await {
                if root
                    .update(cx, |root, cx| {
                        if root.restart_admission.is_fenced()
                            || root
                                .enabled_plugins
                                .as_ref()
                                .is_some_and(|enabled| !enabled.contains("omni_search"))
                            || !multi_launcher::settings::provider_search_capability_enabled(
                                root.enabled_capabilities.as_ref(),
                                "omni_search",
                            )
                        {
                            return;
                        }
                        let query = root.query.read(cx).value().to_string();
                        if is_omni_query(&query) {
                            root.refresh_launcher_results_with_viewport(query, false, cx);
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }
    pub(super) fn activate_omni(
        &mut self,
        result: &LauncherSearchResult,
        query: String,
        source: ActivationSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let LauncherResultOrigin::Omni(child) = &result.origin else {
            return;
        };
        if self
            .enabled_plugins
            .as_ref()
            .is_some_and(|enabled| !enabled.contains("omni_search"))
            || !multi_launcher::settings::provider_search_capability_enabled(
                self.enabled_capabilities.as_ref(),
                "omni_search",
            )
            || self
                .search_catalog
                .providers
                .omni
                .source_for(&query, &result.action)
                .as_ref()
                != Some(child)
            || !supported_child(child, &result.action)
            || window.has_active_dialog(cx)
            || !self.external_owner_is_current(window.focused(cx).as_ref(), window, cx)
        {
            return;
        }
        let settings = self.search_catalog.providers.omni.settings();
        let Some(native) = self
            .native_window
            .filter(|native| current_foreground_identity() == Some(native.identity))
        else {
            return;
        };
        if *child == OmniSource::Calendar {
            if !settings.include_calendar { return; }
            let Some((owner, snapshot, scope, revision)) = self.search_catalog.providers.omni.projection(&query).and_then(|projection| projection.calendar_for(&result.action)) else { return; };
            let Some(catalog) = self.omni_search.current_authority() else { return; };
            match parse_action(&result.action) {
                Ok(Command::Calendar(multi_launcher::commands::CalendarCommand::Open { .. })) => {
                    self.open_calendar_view(query, result.action.clone(), result.origin.clone(), Some((snapshot, scope, revision, catalog)), window, cx);
                }
                Ok(Command::Calendar(command @ (multi_launcher::commands::CalendarCommand::Search { .. } | multi_launcher::commands::CalendarCommand::Upcoming))) => {
                    self.start_calendar_query(result.action.clone(), command, owner, snapshot, scope, revision, catalog, query, native, window, cx);
                }
                _ => {}
            }
            return;
        }
        if *child == OmniSource::Todo {
            if !settings.include_todos { return; }
            let Some((owner, receipt, scope, revision)) = self.search_catalog.providers.omni.projection(&query).and_then(|projection| projection.todo_for(&result.action)) else { return; };
            let Some(catalog) = self.omni_search.current_authority() else { return; };
            let guard = external_launch::OmniGuard { catalog, lease: Arc::new(AtomicBool::new(true)), effect_gate: Arc::new(external_launch::EffectGate::default()), visibility: self.root_visibility.clone(), revision: self.root_visibility.request_revision.load(Ordering::Acquire), native, admission: self.restart_admission.clone() };
            self.start_omni_todo(result.action.clone(), owner, receipt, scope, revision, guard, query, source, window, cx);
            return;
        }
        let (catalog, target, command) = match child {
            OmniSource::Folder if settings.include_folders => {
                let Some(catalog) = self.omni_search.current_authority() else {
                    return;
                };
                let Some((_, command)) =
                    external_launch::provider_association_by_name("folders", &result.action)
                else {
                    return;
                };
                (catalog, external_launch::OmniTarget::Folder, command)
            }
            OmniSource::Bookmark if settings.include_bookmarks => {
                let Some(catalog) = self.omni_search.current_authority() else {
                    return;
                };
                let Some((_, command)) =
                    external_launch::provider_association_by_name("bookmarks", &result.action)
                else {
                    return;
                };
                (catalog, external_launch::OmniTarget::Bookmark, command)
            }
            OmniSource::App {
                generation,
                ordinal,
            } if settings.include_apps => {
                let Some(catalog) = self.omni_search.authority(*generation) else {
                    return;
                };
                if self.search_catalog.actions.get(*ordinal) != Some(&result.action) {
                    return;
                }
                let Ok(Command::External(command)) = parse_action(&result.action) else {
                    return;
                };
                if *ordinal < self.search_catalog.saved_count {
                    let association = if external_launch::saved_url(&result.action).is_some() {
                        Some(external_launch::AssociationKind::SavedUrl)
                    } else if external_launch::saved_document(&result.action).is_some() {
                        Some(external_launch::AssociationKind::SavedDocument)
                    } else if self
                        .search_catalog
                        .verified_folders
                        .contains(&result.action)
                    {
                        Some(external_launch::AssociationKind::SavedFolder)
                    } else {
                        #[cfg(windows)]
                        if external_launch::saved_shortcut(&result.action).is_some() {
                            Some(external_launch::AssociationKind::SavedShortcut)
                        } else {
                            None
                        }
                        #[cfg(not(windows))]
                        {
                            None
                        }
                    };
                    if association.is_none()
                        && external_launch::saved_external(&result.action).as_ref()
                            != Some(&command)
                    {
                        return;
                    }
                    (
                        catalog,
                        external_launch::OmniTarget::Saved(association),
                        command,
                    )
                } else {
                    let index = *ordinal - self.search_catalog.saved_count;
                    let Some(index_generation) = self.search_catalog.index_generation else {
                        return;
                    };
                    let indexed = LauncherSearchResult {
                        action: result.action.clone(),
                        origin: LauncherResultOrigin::Indexed {
                            generation: index_generation,
                            index,
                        },
                        display_label: None,
                        hide_description: false,
                    };
                    let Some(authority) = indexed_applications::admitted_result(
                        &indexed,
                        self.indexed_applications.snapshot.as_ref(),
                        &self.search_catalog.actions[self.search_catalog.saved_count..],
                    ) else {
                        return;
                    };
                    (
                        catalog,
                        external_launch::OmniTarget::Indexed(authority),
                        command,
                    )
                }
            }
            _ => return,
        };
        let guard = external_launch::OmniGuard {
            catalog,
            lease: Arc::new(AtomicBool::new(true)),
            effect_gate: Arc::new(external_launch::EffectGate::default()),
            visibility: self.root_visibility.clone(),
            revision: self
                .root_visibility
                .request_revision
                .load(Ordering::Acquire),
            native,
            admission: self.restart_admission.clone(),
        };
        self.start_omni_launch(
            result.action.clone(),
            command,
            target,
            guard,
            query,
            source,
            window,
            cx,
        );
    }

    pub(super) fn rebuild_omni_catalog(&mut self, cx: &mut Context<Self>) {
        if self
            .enabled_plugins
            .as_ref()
            .is_some_and(|enabled| !enabled.contains("omni_search"))
        {
            self.omni_search.retire();
            self.search_catalog.providers.omni.install(None);
            return;
        }
        let (generation, authority) = self.omni_search.begin();
        self.search_catalog.providers.omni.install(None);
        let actions = self.search_catalog.actions.clone();
        let worker_authority = authority.clone();
        let admission = self.restart_admission.clone();
        let task = cx.background_executor().spawn(async move {
            OmniAppCatalog::build(actions, generation, || {
                worker_authority.load(Ordering::Acquire) && !admission.is_fenced()
            })
        });
        let root = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let Some(catalog) = task.await else {
                return;
            };
            let _ = root.update(cx, |root, cx| {
                if root.restart_admission.is_fenced()
                    || root.omni_search.authority(generation).is_none()
                {
                    return;
                }
                root.search_catalog
                    .providers
                    .omni
                    .install(Some(Arc::new(catalog)));
                let query = root.query.read(cx).value().to_string();
                if is_omni_query(&query) {
                    root.refresh_launcher_results_with_viewport(query, false, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Plugin for Provider {
    fn name(&self) -> &str {
        "omni_search"
    }
    fn description(&self) -> &str {
        "Search applications, folders and bookmarks (prefix: `o`)"
    }
    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
    fn query_prefixes(&self) -> &[&str] {
        &["o"]
    }
    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "o".into(),
                desc: "Omni".into(),
                action: "query:o ".into(),
                args: None,
            },
            Action {
                label: "o list".into(),
                desc: "Omni".into(),
                action: "query:o list".into(),
                args: None,
            },
        ]
    }
    fn search(&self, query: &str) -> Vec<Action> {
        let (settings, settings_revision) = self.settings.read().map(|settings| (settings.clone(), self.settings_revision.load(Ordering::Acquire))).unwrap_or_else(|_| (OmniSearchSettings::default(), self.settings_revision.load(Ordering::Acquire)));
        let catalog = self.catalog.read().ok().and_then(|value| value.clone());
        let notes = if settings.include_notes {
            self.notes.lock().ok().and_then(|owner| owner.clone())
        } else {
            None
        };
        let note_snapshot = notes.as_ref().and_then(|_| self.note_snapshots.current());
        let todo_owner = settings.include_todos.then(|| self.todos.read().ok().and_then(|slot| slot.clone()).and_then(|slot| slot.lock().ok().and_then(|owner| owner.as_ref().and_then(|result| result.as_ref().ok()).cloned()))).flatten();
        let todo_snapshot = todo_owner.as_ref().map(|owner| owner.snapshot());
        let todo_provider = todo_snapshot.as_ref().map(|snapshot| TodoProjection(snapshot.clone()));
        let calendar_owner = settings.include_calendar.then(|| self.calendar.read().ok().and_then(|slot| slot.clone()).and_then(|slot| slot.lock().ok().and_then(|owner| owner.as_ref().and_then(|result| result.as_ref().ok()).cloned()))).flatten();
        let calendar_snapshot = calendar_owner.as_ref().map(|owner| owner.snapshot());
        let mut rows = project_omni(
            query,
            &settings,
            &OmniSources {
                folders: self.folders.as_deref().map(|owner| owner as &dyn Plugin),
                bookmarks: self.bookmarks.as_deref().map(|owner| owner as &dyn Plugin),
                apps: catalog.as_deref(),
                notes: notes.as_deref().map(|owner| owner as &dyn Plugin),
                todos: todo_provider.as_ref().map(|provider| provider as &dyn Plugin),
                calendar: calendar_snapshot.as_ref().filter(|snapshot| snapshot.actionable).map(|_| &multi_launcher::plugins::calendar::CalendarPlugin as &dyn Plugin),
            },
        );
        if note_snapshot.as_ref().is_none_or(|snapshot| {
            snapshot.version != multi_launcher::plugins::note::note_version()
        }) {
            rows.retain(|row| row.source != OmniSource::Note);
        }
        rows.retain(|row| row.source != OmniSource::Todo || todo_owner.as_ref().zip(todo_snapshot.as_ref())
            .is_some_and(|(owner, snapshot)| owner.receipt(snapshot, &row.action).is_some()));
        let actions = rows.iter().map(|row| row.action.clone()).collect();
        let mut indexed = HashMap::<u64, Vec<OmniRow>>::new();
        for row in rows {
            indexed
                .entry(action_hash(&row.action))
                .or_default()
                .push(row);
        }
        if let Ok(mut projected) = self.projected.lock() {
            *projected = Some(Arc::new(Projection {
                query: query.to_owned(),
                rows: indexed,
                notes: note_snapshot,
                todos: todo_owner.zip(todo_snapshot),
                calendar: calendar_owner.zip(calendar_snapshot),
                settings_revision: (self.settings_revision.clone(), settings_revision),
            }));
        }
        actions
    }
}

struct TodoProjection(Arc<multi_launcher::plugins::todo::CheckedTodoSnapshot>);
impl Plugin for TodoProjection {
    fn name(&self) -> &str { "todo" }
    fn description(&self) -> &str { "Todo snapshot" }
    fn capabilities(&self) -> &[&str] { &["search"] }
    fn search(&self, query: &str) -> Vec<Action> {
        query.strip_prefix("todo list").map(|filter| multi_launcher::plugins::todo::project_todo_list(&self.0.entries, filter)).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::{Owner, Provider};
    use multi_launcher::actions::Action;
    use multi_launcher::plugin::Plugin;
    use multi_launcher::plugins::omni_search::{OmniAppCatalog, OmniSource};
    use std::{
        collections::{HashMap, HashSet},
        sync::{Arc, Mutex},
    };

    #[test]
    fn calendar_omni_projects_stock_routes_without_direct_calendar_gate() {
        let temp = tempfile::tempdir().unwrap();
        let root = multi_launcher::platform::app_data::AppDataRoot::from_settings_path(temp.path().join("settings.json")).unwrap();
        let owner = multi_launcher::plugins::calendar::CheckedCalendarOwner::start(&root, Arc::new(|| {})).unwrap();
        let provider = Provider::new(None, None, Arc::new(Mutex::new(None)), Arc::new(super::super::search_providers::NoteSnapshots::default()));
        provider.attach_calendar(Arc::new(Mutex::new(Some(Ok(owner.clone())))));
        let enabled = HashSet::from(["omni_search".to_owned()]);
        for query in ["o", "o list"] {
            let rows = super::super::search_providers::search_routed([&provider as &dyn Plugin], query, Some(&enabled), None);
            assert!(rows.iter().any(|(_, action)| action.action == "calendar:open"));
            assert!(rows.iter().any(|(_, action)| action.action == "calendar:upcoming"));
        }
        let rows = provider.search("o café");
        let row = rows.iter().find(|action| action.action == "calendar:search:café").unwrap();
        let projection = provider.projection("o café").unwrap();
        assert!(projection.calendar_for(row).is_some());
        provider.configure(Some(&serde_json::json!({"include_calendar": false})));
        assert!(projection.calendar_for(row).is_none());
        assert!(provider.search("o café").iter().all(|action| !action.action.starts_with("calendar:")));
        owner.shutdown();
    }

    #[test]
    fn todo_projection_uses_snapshot_and_own_omni_gate() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("todo.json"), br#"[{"id":"one","text":"Caf\u00e9 task","done":false,"priority":2,"tags":[],"entity_refs":[]},{"id":"","text":"blank","done":false},{"id":"dupe","text":"duplicate one","done":false},{"id":"dupe","text":"duplicate two","done":true},{"id":"other","text":"ambiguous","done":false,"extra":1,"extra":2}]"#).unwrap();
        let root = multi_launcher::platform::app_data::AppDataRoot::from_settings_path(temp.path().join("settings.json")).unwrap();
        let owner = multi_launcher::plugins::todo::CheckedTodoOwner::start(&root, Arc::new(|| {})).unwrap();
        let provider = Provider::new(None, None, Arc::new(Mutex::new(None)), Arc::new(super::super::search_providers::NoteSnapshots::default()));
        provider.attach_todos(Arc::new(Mutex::new(Some(Ok(owner.clone())))));
        assert_eq!(owner.snapshot().entries.len(), 5);
        assert_eq!(provider.search("o").len(), 1);
        let rows = provider.search("o Café");
        assert_eq!(rows.len(), 1);
        let projection = provider.projection("o Café").unwrap();
        assert_eq!(projection.source_for(&rows[0]), Some(OmniSource::Todo));
        assert!(projection.todo_for(&rows[0]).is_some());
        let enabled = HashSet::from(["omni_search".to_owned()]);
        assert_eq!(super::super::search_providers::search_routed([&provider as &dyn Plugin], "o Café", Some(&enabled), None).len(), 1);
        provider.configure(Some(&serde_json::json!({"include_todos": false})));
        assert!(projection.todo_for(&rows[0]).is_none());
        assert!(provider.search("o Café").is_empty());
        owner.shutdown();
    }

    #[test]
    fn omni_note_gate_does_not_depend_on_direct_notes_search() {
        let enabled = HashSet::from(["omni_search".to_owned()]);
        let caps = HashMap::from([("notes".to_owned(), Vec::new())]);
        assert!(super::notes_enabled(Some(&enabled), Some(&caps), true));
        assert!(!super::notes_enabled(Some(&enabled), Some(&caps), false));
        assert!(!super::notes_enabled(Some(&HashSet::new()), None, true));
        let omni_disabled = HashMap::from([("omni_search".to_owned(), Vec::new())]);
        assert!(!super::notes_enabled(
            Some(&enabled),
            Some(&omni_disabled),
            true
        ));
    }

    #[cfg(windows)]
    #[test]
    fn app_child_accepts_indexed_and_saved_windows_shapes() {
        let source = OmniSource::App {
            generation: 1,
            ordinal: 0,
        };
        let action = |path: &str| Action {
            label: "Target".into(),
            desc: path.into(),
            action: path.into(),
            args: None,
        };
        assert!(super::supported_child(
            &source,
            &action(r"\\?\C:\Apps\Editor.exe")
        ));
        assert!(super::supported_child(&source, &action(r"C:\Data")));
        assert!(super::supported_child(
            &source,
            &action(r"C:\Apps\Editor.lnk")
        ));
        assert!(!super::supported_child(
            &source,
            &action(r"\\.\C:\Apps\Editor.lnk")
        ));
    }

    #[test]
    fn projection_retains_exact_action_and_current_catalog_generation() {
        let provider = Provider::new(
            None,
            None,
            Arc::new(Mutex::new(None)),
            Arc::new(super::super::search_providers::NoteSnapshots::default()),
        );
        let action = Action {
            label: "Editor".into(),
            desc: "App".into(),
            action: "editor.exe".into(),
            args: Some("--one".into()),
        };
        let catalog = OmniAppCatalog::build(Arc::new(vec![action.clone()]), 9, || true).unwrap();
        provider.install(Some(Arc::new(catalog)));
        assert_eq!(provider.search("o"), vec![action.clone()]);
        assert_eq!(
            provider.source_for("o", &action),
            Some(OmniSource::App {
                generation: 9,
                ordinal: 0
            })
        );
        let mut forged = action.clone();
        forged.args = Some("--two".into());
        assert!(provider.source_for("o", &forged).is_none());
        assert!(provider.source_for("o other", &action).is_none());
        let mut owner = Owner::default();
        let (first, old) = owner.begin();
        let (second, _) = owner.begin();
        assert!(!old.load(std::sync::atomic::Ordering::Acquire));
        assert!(owner.authority(first).is_none());
        assert!(owner.authority(second).is_some());
    }

    #[test]
    fn omni_routing_obeys_its_own_search_capability() {
        let provider = Provider::new(
            None,
            None,
            Arc::new(Mutex::new(None)),
            Arc::new(super::super::search_providers::NoteSnapshots::default()),
        );
        provider.install(Some(Arc::new(
            OmniAppCatalog::build(
                Arc::new(vec![Action {
                    label: "Editor".into(),
                    desc: "App".into(),
                    action: "editor.exe".into(),
                    args: None,
                }]),
                1,
                || true,
            )
            .unwrap(),
        )));
        let enabled = HashSet::from(["omni_search".to_owned()]);
        let routed = |enabled: Option<&HashSet<String>>,
                      caps: Option<&HashMap<String, Vec<String>>>| {
            super::super::search_providers::search_routed(
                [&provider as &dyn Plugin],
                "o",
                enabled,
                caps,
            )
        };
        assert_eq!(routed(Some(&enabled), None).len(), 1);
        assert!(routed(Some(&HashSet::new()), None).is_empty());
        assert!(
            routed(
                Some(&enabled),
                Some(&HashMap::from([("omni_search".into(), vec![])]))
            )
            .is_empty()
        );
    }

    #[test]
    fn exact_postfilter_rejects_subsequence_only_omni_result() {
        use fuzzy_matcher::skim::SkimMatcherV2;
        use multi_launcher::actions::search::{CachedSearchEntry, score_provider_candidate};
        let action = Action {
            label: "Primary Alpha".into(),
            desc: "Application".into(),
            action: "alpha.exe".into(),
            args: None,
        };
        let provider = Provider::new(
            None,
            None,
            Arc::new(Mutex::new(None)),
            Arc::new(super::super::search_providers::NoteSnapshots::default()),
        );
        provider.install(Some(Arc::new(
            OmniAppCatalog::build(Arc::new(vec![action.clone()]), 1, || true).unwrap(),
        )));
        assert_eq!(provider.search("o prmry"), vec![action.clone()]);
        assert!(
            score_provider_candidate(
                &action,
                &CachedSearchEntry::from_action(&action),
                "o prmry",
                "prmry",
                true,
                false,
                &SkimMatcherV2::default(),
                1.0,
                &HashMap::new(),
                &HashMap::new()
            )
            .is_none()
        );
    }
}
