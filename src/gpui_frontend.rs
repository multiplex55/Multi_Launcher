use std::pin::Pin;
#[cfg(windows)]
use std::sync::mpsc::{Receiver, Sender, channel};
mod action_sheet;
mod activity;
mod add_action;
mod appearance;
mod external_launch;
mod file_search;
mod folder_size;
mod file_lock;
mod file_search_handoff;
mod file_search_preview;
mod settings_actions;
mod settings_search;
mod settings_navigation;
mod date_search;
mod calculator_search;
mod base_convert_search;
mod unit_convert_search;
mod plugin_editor;
#[cfg(windows)]
mod downloads_triage;
mod favorites;
mod favorite_search;
mod hash_jump;
mod hash_progress;
mod hash_layout;
mod hash_pin;
mod structured_pin;
mod hash_dock;
mod wake;
mod json_utility;
mod xml_utility;
mod regex_tester;
mod regex_substitution;
mod regex_cases;
mod regex_inspectors;
mod regex_syntax;
mod regex_jump;
mod clipboard_history;
mod snippets;
mod snippet_fill;
mod clipboard_modify_view;
mod tempfile_view;
mod clipboard_modify_startup;
mod regex_history;
mod regex_presets;
mod regex_clipboard;
mod xml_host;
mod xml_tree;
mod xml_xpath;
mod xml_schema;
mod xml_save;
mod json_tree;
mod json_jump;
mod scoped_jump;
mod xml_jump;
mod utility_host;
mod utility_mount;
use wake::HashWakeSignal;
mod indexed_applications;
mod window_search;
mod hash_jump_select;
mod launcher_controls;
pub(crate) mod launcher_header;
mod launcher_result;
mod jump_mode;
mod search_providers;
pub(crate) mod root_placement;
pub(crate) mod saved_folder;

pub(crate) use external_launch::Settings as ExternalLaunchPreferences;

use favorites::{FAVORITES_WRITE_PENDING, ResultFavorites};
use jump_mode::{
    JumpGeometryDraft, JumpGeometryElement, JumpGeometryFrame, JumpModeController,
    jump_geometry_publisher, jump_hint_badge,
};
#[cfg(test)]
use jump_mode::{
    JumpTargetGeometry, bounds_are_contained_by, clipped_jump_geometry, jump_badge_placement,
};
use multi_launcher::gui::{Panel, panel_stack};

use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    ffi::{OsStr, OsString},
    future::Future,
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc, Mutex, mpsc::TryRecvError,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    },
    task::{Context as FutureContext, Poll, Waker},
    time::Duration,
};

use gpui_kit::StatefulInteractiveElement;
use gpui_kit::base::Disableable;
use gpui_kit::base::{Checkbox, CheckboxIndicator, CheckboxState};
use gpui_kit::component::IndexPath;
use gpui_kit::component::input::{
    IndentInline, InputEvent, MoveDown, MoveLeft, MovePageDown, MovePageUp, MoveRight, MoveUp,
};
use gpui_kit::component::{
    WindowExt as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    input::{Input, InputState, Textarea, TextareaState},
    label::Label,
    menu::{DropdownMenu, PopupMenu, PopupMenuItem},
    scroll::ScrollableElement as _,
    searchable_list::SearchableListItem,
    select::{Select, SelectEvent, SelectState},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use fuzzy_matcher::skim::SkimMatcherV2;

#[cfg(windows)]
use gpui_windows::NativeKeydownReceipt;
#[cfg(windows)]
use std::ffi::c_void;

pub(crate) use crate::gpui_restart::Frontend;
use crate::gpui_restart::{
    EffectPermit, HashWork, RestartAdmission, RestartBlocker, RestartContext, RestartPlan,
};
use multi_launcher::SettingsSaveWorkerRegistry;
use multi_launcher::actions::Action;
use multi_launcher::universal_actions::{
    ActionResolutionContext, ActionSurface, ActionTargetResolver, ActionTargetResolverContext,
    RootLauncherPolicy, UniversalActionInvocationContext, UniversalActionOperation,
    UniversalActionRegistry, UniversalUiIntent, action_ids,
};
use multi_launcher::actions::search::{
    CachedSearchEntry, ReadOnlySearchInput,
};
use multi_launcher::common::query::ActionFilterMetadata;
use multi_launcher::clipboard_modify::clipboard::{
    ArboardClipboardBackend, ClipboardBackend, ClipboardError,
};
use multi_launcher::commands::handlers::{handle_hash_generator, handle_query};
use multi_launcher::commands::{
    ActivationSource, Command, CommandInvocation, CommandOutcome, DialogCommand,
    FavoriteLogPolicy, HashGeneratorCommand, HistoryPolicy, PendingQueryPolicy, QueryCommand,
    QueryPolicy, ResultsPolicy, StorageCommand, VisibilityPolicy, parse_action, parse_command,
};
use multi_launcher::coordinate_tool::{
    CoordinateToolController, CoordinateToolPreferences, NativeCoordinateRuntimeFactory,
};
use multi_launcher::hash_generator::{
    HashAlgorithm, HashClipboardCopy, HashComparison, HashInputMode, HashSession,
};
use multi_launcher::hotkey::HotkeyListener;
#[cfg(windows)]
use multi_launcher::hotkey::runtime::HotkeyTrigger;
use multi_launcher::hotkey::{Hotkey, parse_hotkey};
use multi_launcher::hotkey::runtime::legacy_virtual_key;
use multi_launcher::hotkey::validation::{hotkeys_can_cofire, jump_chord_conflict};
use multi_launcher::launcher::query_history::{QueryHistoryDirection, QueryHistoryNavigator};
use multi_launcher::launcher::command_help::{
    CommandHelpProvider, command_prefixes, example_line, matches_filter,
};
use multi_launcher::launcher::quick_help::{
    GuideBlock, HelpFilterPeer, QUICK_HELP_GUIDE, QuickHelpProvider, SharedHelpFilter,
    matches_provider_filter,
};
use multi_launcher::launcher::jump_mode::{
    Hint, SemanticTargetId, SurfaceId,
    TargetGeneration, TargetId, TargetKind, TargetMetadata,
};
use multi_launcher::launcher::query_results_layout::{
    QueryLayoutPlugin, QuerySelectionMove, ResolvedQueryResultsLayout, move_query_selection,
    resolve_query_results_layout,
};
use multi_launcher::settings::{
    AppliedWindowSize, HintAlphabet, JumpModeChord, NativeWindowGeometry, PreferredWindowSize,
    QueryResultsLayoutSettings, RememberedPresentation, UtilityDialogSizes,
    UtilityPresentations, UtilityToolKey,
};
use multi_launcher::toast_log;
use multi_launcher::utility_geometry_binding::{
    CheckedNativeWindow, NativeGeometryBinding, NativeGeometrySlot, NativeGeometryStamp,
    NativeWindowMetrics, NativeWindowRect, native_window_metrics, new_native_geometry_slot,
    restore_native_window_geometry, take_native_geometry,
};
use multi_launcher::utility_presentation::{UtilityPresentationStore, UtilityPresentationUpdate};
use multi_launcher::utility_size::{
    PhysicalWindowRect, UtilityNativeGeometrySample, UtilitySizeStore,
};
use fst::Map;

pub(crate) struct LauncherSearchStartup {
    pub actions: Vec<Action>,
    pub verified_folders: Vec<Action>,
    pub index_config: multi_launcher::indexer::coordinator::IndexConfig,
    pub branding: launcher_header::Branding,
    pub calculator_settings: Option<serde_json::Value>,
    pub match_exact: bool,
    pub fuzzy_weight: f32,
    pub usage_weight: f32,
    pub query_autocomplete: bool,
    pub show_examples: bool,
    pub page_jump: usize,
    pub query_results_layout: QueryResultsLayoutSettings,
    pub jump_mode_enabled: bool,
    pub jump_mode_activation_chord: JumpModeChord,
    pub jump_mode_hint_alphabet: HintAlphabet,
    pub usage: HashMap<String, u32>,
    pub folder_aliases_lc: HashMap<String, Option<String>>,
    pub bookmark_aliases_lc: HashMap<String, Option<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LauncherResultOrigin {
    Saved,
    Indexed { generation: u64, index: usize },
    CachedCommand,
    Provider(String),
}

#[derive(Clone, Debug, PartialEq)]
struct LauncherSearchResult {
    action: Action,
    origin: LauncherResultOrigin,
    display_label: Option<String>,
    hide_description: bool,
}

impl LauncherSearchResult {
    fn label(&self) -> &str {
        self.display_label.as_deref().unwrap_or(&self.action.label)
    }

    #[cfg(windows)]
    fn saved_shortcut(&self) -> Option<multi_launcher::commands::ExternalCommand> {
        (self.origin == LauncherResultOrigin::Saved)
            .then(|| external_launch::saved_shortcut(&self.action))
            .flatten()
    }

    fn saved_url(&self) -> Option<multi_launcher::commands::ExternalCommand> {
        (self.origin == LauncherResultOrigin::Saved)
            .then(|| external_launch::saved_url(&self.action))
            .flatten()
    }

    fn saved_document(&self) -> Option<multi_launcher::commands::ExternalCommand> {
        (self.origin == LauncherResultOrigin::Saved)
            .then(|| external_launch::saved_document(&self.action))
            .flatten()
    }

    fn saved_external(&self) -> Option<multi_launcher::commands::ExternalCommand> {
        if self.origin == LauncherResultOrigin::Saved {
            external_launch::saved_external(&self.action)
        } else {
            None
        }
    }
}

struct LauncherSearchCatalog {
    providers: InstalledLauncherProviders,
    branding: launcher_header::Branding,
    calculator_settings: Option<serde_json::Value>,
    command_help_providers: Arc<[CommandHelpProvider]>,
    quick_help_providers: Arc<[QuickHelpProvider]>,
    actions: Vec<Action>,
    saved_count: usize,
    index_config: multi_launcher::indexer::coordinator::IndexConfig,
    index_generation: Option<u64>,
    verified_folders: Vec<Action>,
    cached_entries: Vec<CachedSearchEntry>,
    filter_metadata: Vec<ActionFilterMetadata>,
    commands: Vec<Action>,
    command_cached_entries: Vec<CachedSearchEntry>,
    matcher: SkimMatcherV2,
    match_exact: bool,
    fuzzy_weight: f32,
    usage_weight: f32,
    query_autocomplete: bool,
    startup_show_examples: bool,
    page_jump: usize,
    query_results_layout: QueryResultsLayoutSettings,
    completion_index: Map<Vec<u8>>,
    indexed_completion_index: Option<Map<Vec<u8>>>,
    usage: HashMap<String, u32>,
    folder_aliases_lc: HashMap<String, Option<String>>,
    bookmark_aliases_lc: HashMap<String, Option<String>>,
}

use search_providers::InstalledLauncherProviders;

impl LauncherSearchCatalog {
    fn new(
        startup: LauncherSearchStartup,
        enabled_plugins: Option<&HashSet<String>>,
    ) -> Self {
        // Keep only persisted actions that the installed typed activation
        // boundary can execute without changing their Search payload. Saved
        // typed actions retain the shared local-action admission semantics.
        let actions = supported_saved_search_actions(startup.actions, &startup.verified_folders);
        let saved_count = actions.len();
        #[cfg(not(test))]
        let providers = InstalledLauncherProviders::new(enabled_plugins);
        #[cfg(test)]
        let providers = InstalledLauncherProviders::test_without_persisted_providers();
        // Installed command labels and completion fallbacks follow plugin
        // enablement. Search capability gates provider rows at query time.
        let commands = providers.cached_commands(enabled_plugins);
        let command_help_providers = providers.command_help_providers();
        let quick_help_providers = command_help_providers
            .iter()
            .map(|provider| QuickHelpProvider {
                name: provider.name.clone(),
                description: provider.description.clone(),
            })
            .collect::<Vec<_>>()
            .into();
        let cached_entries = actions.iter().map(CachedSearchEntry::from_action).collect();
        let filter_metadata = actions
            .iter()
            .map(ActionFilterMetadata::from_action)
            .collect();
        let command_cached_entries = commands
            .iter()
            .map(CachedSearchEntry::from_action)
            .collect();
        let completion_index = multi_launcher::completion::build_index(&commands, &actions);
        Self {
            providers,
            branding: startup.branding,
            calculator_settings: startup.calculator_settings,
            command_help_providers,
            quick_help_providers,
            actions,
            saved_count,
            index_config: startup.index_config,
            index_generation: None,
            verified_folders: startup.verified_folders,
            cached_entries,
            filter_metadata,
            commands,
            command_cached_entries,
            matcher: SkimMatcherV2::default(),
            match_exact: startup.match_exact,
            fuzzy_weight: startup.fuzzy_weight,
            usage_weight: startup.usage_weight,
            query_autocomplete: startup.query_autocomplete,
            startup_show_examples: startup.show_examples,
            page_jump: startup.page_jump,
            query_results_layout: startup.query_results_layout,
            completion_index,
            indexed_completion_index: None,
            usage: startup.usage,
            folder_aliases_lc: startup.folder_aliases_lc,
            bookmark_aliases_lc: startup.bookmark_aliases_lc,
        }
    }

    fn replace_saved_actions(&mut self, rows: Vec<Action>, verified_folders: Vec<Action>) -> bool {
        let saved = supported_saved_search_actions(rows, &verified_folders);
        if self.actions[..self.saved_count] == saved && self.verified_folders == verified_folders {
            return false;
        }
        let indexed = self.actions.split_off(self.saved_count);
        let indexed_cached = self.cached_entries.split_off(self.saved_count);
        let indexed_metadata = self.filter_metadata.split_off(self.saved_count);
        self.cached_entries = saved.iter().map(CachedSearchEntry::from_action).collect();
        self.filter_metadata = saved.iter().map(ActionFilterMetadata::from_action).collect();
        self.saved_count = saved.len();
        self.actions = saved;
        self.actions.extend(indexed);
        self.cached_entries.extend(indexed_cached);
        self.filter_metadata.extend(indexed_metadata);
        self.completion_index = multi_launcher::completion::build_index(
            &self.commands, &self.actions[..self.saved_count],
        );
        self.verified_folders = verified_folders;
        true
    }

    fn clear_index(&mut self) {
        self.actions.truncate(self.saved_count);
        self.cached_entries.truncate(self.saved_count);
        self.filter_metadata.truncate(self.saved_count);
        self.index_generation = None;
        self.indexed_completion_index = None;
    }

    fn install_index(&mut self, snapshot: indexed_applications::Snapshot) -> indexed_applications::Identity {
        self.actions.truncate(self.saved_count);
        self.cached_entries.truncate(self.saved_count);
        self.filter_metadata.truncate(self.saved_count);
        self.actions.extend(snapshot.actions);
        self.cached_entries.extend(snapshot.cached);
        self.filter_metadata.extend(snapshot.metadata);
        self.index_generation = Some(snapshot.generation);
        self.indexed_completion_index = Some(snapshot.completion_index);
        indexed_applications::Identity { generation: snapshot.generation, roots: snapshot.roots, raw_actions: snapshot.raw_actions, authority: Arc::new(AtomicBool::new(true)) }
    }

    fn saved_actions(&self) -> &[Action] {
        &self.actions[..self.saved_count]
    }

    fn local_origin(&self, index: usize) -> LauncherResultOrigin {
        if index < self.saved_count {
            LauncherResultOrigin::Saved
        } else {
            LauncherResultOrigin::Indexed {
                generation: self.index_generation.unwrap_or_default(),
                index: index - self.saved_count,
            }
        }
    }

    fn resolve_result_layout(
        &self,
        query: &str,
        enabled_plugins: Option<&HashSet<String>>,
    ) -> ResolvedQueryResultsLayout {
        resolve_query_results_layout(
            query,
            &self.query_results_layout,
            enabled_plugins,
            self.providers.plugins(),
            |plugin| plugin.name(),
            |plugin| QueryLayoutPlugin {
                name: plugin.name(),
                query_prefixes: plugin.query_prefixes(),
                capabilities: plugin.capabilities(),
            },
        )
    }

    fn initial_results(&self, enabled_plugins: Option<&HashSet<String>>) -> LauncherSearchResults {
        let mut results = LauncherSearchResults::default();
        results.replace(
            String::new(),
            self.compose("", Vec::new(), true),
            self.resolve_result_layout("", enabled_plugins),
            self.page_jump,
        );
        results
    }

    fn compose(
        &self,
        raw_query: &str,
        provider_actions: Vec<(String, Action)>,
        raw_provider_results_empty: bool,
    ) -> Vec<LauncherSearchResult> {
        if raw_query.trim().is_empty() {
            use multi_launcher::actions::search::{EmptyBrowseKind, empty_browse_rows};
            let mut local_index = 0;
            return empty_browse_rows(&self.commands, &self.actions)
                .map(|row| LauncherSearchResult {
                    action: row.action.clone(),
                    origin: match row.kind {
                        EmptyBrowseKind::Command => LauncherResultOrigin::CachedCommand,
                        EmptyBrowseKind::Saved => {
                            let origin = self.local_origin(local_index);
                            local_index += 1;
                            origin
                        },
                    },
                    display_label: row.display_label(),
                    hide_description: false,
                })
                .collect();
        }
        let trimmed = raw_query.trim();
        let trimmed_lc = trimmed.to_lowercase();
        let command_fallback = multi_launcher::actions::search::collect_command_fallback_candidates(
            multi_launcher::actions::search::CommandFallbackInput {
                actions: &self.commands,
                cached_entries: &self.command_cached_entries,
                matcher: &self.matcher,
                match_exact: self.match_exact,
                fuzzy_weight: self.fuzzy_weight,
                folder_aliases_lc: &self.folder_aliases_lc,
                bookmark_aliases_lc: &self.bookmark_aliases_lc,
            },
            trimmed,
            &trimmed_lc,
            raw_provider_results_empty,
        );
        let provider_term = multi_launcher::actions::search::provider_query_term(&trimmed_lc);
        let targets: HashSet<&str> =
            if (self.match_exact || self.fuzzy_weight <= 0.0) && !provider_term.is_empty() {
                provider_actions
                    .iter()
                    .filter(|(provider, _)| {
                        external_launch::AssociationKind::from_provider_name(provider).is_some()
                    })
                    .map(|(_, action)| action.action.as_str())
                    .collect()
            } else {
                HashSet::new()
            };
        let (folder_aliases, bookmark_aliases) = if targets.is_empty() {
            (None, None)
        } else {
            self.providers.current_search_aliases(&targets)
        };
        let folder_aliases = folder_aliases.as_ref().unwrap_or(&self.folder_aliases_lc);
        let bookmark_aliases = bookmark_aliases
            .as_ref()
            .unwrap_or(&self.bookmark_aliases_lc);
        let mut scored_provider_actions = provider_actions
            .into_iter()
            .filter_map(|(provider, action)| {
                let date_result = date_search::classify(&provider, &action).is_some();
                let base_result = base_convert_search::classify(&provider, &action).is_some();
                let unit_result = unit_convert_search::classify(&provider, &action).is_some();
                let window_result = window_search::action_is_operation(&provider, &action);
                let calculator_result = calculator_search::classify(&provider, raw_query, &action).is_some()
                    || calculator_search::history_index(&provider, raw_query, &action).is_some();
                let folder_size_result = provider == "folder_size" && folder_size::current_provider(raw_query, &action, None, None);
                let file_lock_result = cfg!(windows) && provider == "file_lock" && file_lock::current_provider(raw_query, &action, None, None);
                let file_result = provider == "file_search" && multi_launcher::plugins::file_search::scoped_search_start(raw_query, &action).is_some();
                let settings_result = provider == "settings" && settings_navigation::current_provider(raw_query, &action, None, None).is_some();
                let clipboard_modify_result = provider == "clipboard_modify";
                let score = if external_launch::AssociationKind::from_provider_name(&provider)
                    .is_some() || date_result || base_result || unit_result || calculator_result || window_result || file_result || folder_size_result || file_lock_result || settings_result || clipboard_modify_result || (provider == "favorites" && favorite_search::is_executable(&action))
                {
                    let cached = CachedSearchEntry::from_action(&action);
                    multi_launcher::actions::search::score_provider_candidate(
                        &action,
                        &cached,
                        raw_query,
                        &provider_term,
                        self.match_exact || self.fuzzy_weight <= 0.0,
                        (date_result && multi_launcher::actions::search::date_resolved_postfilter_bypass(raw_query, &action.action)) || file_result || folder_size_result || file_lock_result || settings_result
                            || (clipboard_modify_result && multi_launcher::actions::search::clipboard_modify_resolved_postfilter_bypass(raw_query, &action.action)),
                        &self.matcher,
                        self.fuzzy_weight,
                        folder_aliases,
                        bookmark_aliases,
                    )?
                } else {
                    0.0
                };
                Some((action, score, LauncherResultOrigin::Provider(provider)))
            })
            .collect::<Vec<_>>();
        scored_provider_actions.extend(
            command_fallback
                .scored_actions
                .into_iter()
                .map(|(action, score)| (action, score, LauncherResultOrigin::CachedCommand)),
        );
        multi_launcher::actions::search::compose_read_only_results_with_local_origin(
            ReadOnlySearchInput {
                raw_query,
                command_cache: &[],
                local_actions: &self.actions,
                local_cached_entries: &self.cached_entries,
                local_filter_metadata: &self.filter_metadata,
                folder_aliases_lc: &self.folder_aliases_lc,
                bookmark_aliases_lc: &self.bookmark_aliases_lc,
                matcher: &self.matcher,
                match_exact: self.match_exact,
                fuzzy_weight: self.fuzzy_weight,
                collect_search_stats: false,
                collect_candidate_count: false,
                usage: &self.usage,
                usage_weight: self.usage_weight,
                provider_results: scored_provider_actions,
            },
            |index| self.local_origin(index),
            LauncherResultOrigin::CachedCommand,
            |_| {},
            None,
        )
        .actions
        .into_iter()
        .map(|(action, origin)| LauncherSearchResult {
            action,
            origin,
            display_label: None,
            hide_description: false,
        })
        .collect()
    }

    #[cfg(test)]
    fn compose_actions(&self, query: &str, providers: Vec<Action>) -> Vec<Action> {
        let raw_provider_results_empty = providers.is_empty();
        self.compose(
            query,
            providers
                .into_iter()
                .map(|action| ("test".to_owned(), action))
                .collect(),
            raw_provider_results_empty,
        )
        .into_iter()
        .map(|row| row.action)
        .collect()
    }
}

#[derive(Default)]
struct SearchCompletion {
    suggestions: Vec<String>,
    next_index: usize,
    pending_change: Option<PendingCompletionChange>,
}

struct PendingCompletionChange {
    expected_query: String,
    next_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettingsOpenOrigin {
    Search,
    Menu,
}

#[derive(Default)]
struct SettingsEditorState { open: bool, status: Option<String> }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CommandHelpSaveAttempt {
    id: u64,
    expected_baseline: bool,
    desired: bool,
}

struct CommandHelpState {
    open: bool,
    show_examples: bool,
    status: Option<String>,
    pending: Option<CommandHelpSaveAttempt>,
    next_request_id: u64,
}

#[derive(Default)]
struct QuickHelpState {
    open: bool,
}

impl CommandHelpState {
    fn new(show_examples: bool) -> Self {
        Self {
            open: false,
            show_examples,
            status: None,
            pending: None,
            next_request_id: 0,
        }
    }

    fn begin_save(&mut self, desired: bool) -> Option<CommandHelpSaveAttempt> {
        if !self.open || self.pending.is_some() || desired == self.show_examples {
            return None;
        }
        self.next_request_id = self.next_request_id.wrapping_add(1).max(1);
        let attempt = CommandHelpSaveAttempt {
            id: self.next_request_id,
            expected_baseline: self.show_examples,
            desired,
        };
        self.pending = Some(attempt);
        self.status = None;
        Some(attempt)
    }

    fn fail_to_start(&mut self, request_id: u64, message: String) {
        if self.pending.is_some_and(|pending| pending.id == request_id) {
            self.pending = None;
            self.status = Some(message);
        }
    }

    fn finish_save(
        &mut self,
        request_id: u64,
        result: Result<
            multi_launcher::settings::SettingsBooleanSaveAcknowledgement,
            multi_launcher::settings::SettingsBooleanSaveError,
        >,
    ) -> Option<bool> {
        let pending = self.pending.filter(|pending| pending.id == request_id)?;
        self.pending = None;
        match result {
            Ok(acknowledgement)
                if acknowledgement.preference()
                    == multi_launcher::settings::SettingsBooleanPreference::ShowExamples
                    && acknowledgement.value() == pending.desired =>
            {
                self.show_examples = acknowledgement.value();
                self.status = Some("Show examples saved.".to_owned());
                Some(acknowledgement.value())
            }
            Ok(acknowledgement) => {
                self.status = Some(format!(
                    "Could not accept Show examples acknowledgement for {:?}={} (requested {}).",
                    acknowledgement.preference(), acknowledgement.value(), pending.desired
                ));
                None
            }
            Err(error) => {
                self.status = Some(format!("Could not save Show examples: {error}"));
                None
            }
        }
    }

    fn cancel_idle(&mut self) -> bool {
        if self.pending.is_some() {
            return false;
        }
        self.open = false;
        self.status = None;
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandHelpOpenOrigin {
    Search,
    Menu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QuickHelpOpenOrigin {
    Menu,
    Shortcut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ToastLogReadReceipt {
    session_id: u64,
    request_id: u64,
}

#[derive(Default)]
struct ToastLogViewerState {
    open: bool,
    rows: Vec<String>,
    status: Option<String>,
    session_id: u64,
    next_request_id: u64,
    pending: Option<ToastLogReadReceipt>,
}

impl ToastLogViewerState {
    fn open(&mut self) -> Option<ToastLogReadReceipt> {
        if self.open {
            return None;
        }
        self.open = true;
        self.rows.clear();
        self.session_id = self.session_id.wrapping_add(1).max(1);
        self.status = None;
        self.begin_read()
    }

    fn begin_read(&mut self) -> Option<ToastLogReadReceipt> {
        if !self.open {
            return None;
        }
        self.next_request_id = self.next_request_id.wrapping_add(1).max(1);
        let receipt = ToastLogReadReceipt {
            session_id: self.session_id,
            request_id: self.next_request_id,
        };
        self.pending = Some(receipt);
        self.status = None;
        Some(receipt)
    }

    fn finish_read(&mut self, receipt: ToastLogReadReceipt, rows: Vec<String>) -> bool {
        if !self.open || self.pending != Some(receipt) || receipt.session_id != self.session_id {
            return false;
        }
        self.rows = rows;
        self.pending = None;
        self.status = None;
        true
    }

    fn close(&mut self) -> bool {
        if !self.open {
            return false;
        }
        self.open = false;
        self.session_id = self.session_id.wrapping_add(1).max(1);
        self.pending = None;
        self.status = None;
        true
    }
}

#[derive(IntoElement)]
struct LauncherMenuBar {
    root: WeakEntity<RootWindow>,
    row_height: f32,
    action_context: FocusHandle,
    settings_menu_focus_handle: FocusHandle,
    toast_log_open_pending: bool,
    favorites_pin_focus_handle: FocusHandle,
    favorites_pinned: bool,
    favorites_toolbar_disabled: bool,
    favorites_unpin_status: Option<String>,
    saved_pin_order: Vec<Panel>,
    hash_pin_focus_handle: FocusHandle,
    hash_pinned: bool,
    hash_toolbar_disabled: bool,
    json_pinned: bool,
    xml_pinned: bool,
    structured_pin_disabled: bool,
    json_pin_focus_handle: FocusHandle,
    xml_pin_focus_handle: FocusHandle,
}

impl RenderOnce for LauncherMenuBar {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let file_action_context = self.action_context.clone();
        let file_root = self.root.clone();
        let file_menu = Button::new("launcher-file-menu")
            .max_w_full()
            .min_w(px(0.0))
            .label("File")
            .ghost()
            .small()
            .dropdown_menu(move |menu, window, cx| {
                let action_context = file_action_context.clone();
                let editor_context = file_action_context.clone();
                let hash_context = file_action_context.clone();
                let activity_context = file_action_context.clone();
                let restart_context = file_action_context.clone();
                menu.submenu("Apps", window, cx, move |menu, _, _| {
                    let editor_context = editor_context.clone();
                    let plugin_context = editor_context.clone();
                    let hash_context = hash_context.clone();
                    menu.item(
                        PopupMenuItem::new("Edit Apps")
                            .action(Box::new(OpenAppEditor))
                            .on_click(move |_, window, app| {
                                editor_context.dispatch_action(&OpenAppEditor, window, app);
                            }),
                    ).item(
                        PopupMenuItem::new("Edit Plugins")
                            .action(Box::new(OpenPluginEditor))
                            .on_click(move |_, window, app| {
                                plugin_context.dispatch_action(&OpenPluginEditor, window, app);
                            }),
                    ).item(
                        PopupMenuItem::new("Hash Generator")
                            .action(Box::new(OpenHashGenerator))
                            .on_click(move |_, window, app| {
                                hash_context.dispatch_action(&OpenHashGenerator, window, app);
                            }),
                    )
                })
                .item(
                    PopupMenuItem::new("Activity")
                        .action(Box::new(OpenActivity))
                        .on_click(move |_, window, app| {
                            activity_context.dispatch_action(&OpenActivity, window, app);
                        }),
                )
                .item(
                    PopupMenuItem::new("Restart in egui")
                        .action(Box::new(RestartWithEgui))
                        .on_click(move |_, window, app| {
                            restart_context.dispatch_action(&RestartWithEgui, window, app);
                        }),
                )
                .item(
                    PopupMenuItem::new("Close Application")
                        .action(Box::new(CloseApplication))
                        .on_click(move |_, window, app| {
                            action_context.dispatch_action(&CloseApplication, window, app);
                        }),
                )
            })
            .on_open_change(move |open, window, app| {
                root_menu_open_change(&file_root, RootMenuKind::File, *open, window, app);
            });

        let settings_action_context = self.action_context.clone();
        let settings_root = self.root.clone();
        let settings_menu = Button::new("launcher-settings-menu")
            .max_w_full()
            .min_w(px(0.0))
            .label("Settings")
            .ghost()
            .small()
            .track_focus(&self.settings_menu_focus_handle)
            .dropdown_menu(move |menu, _, _| {
                let action_context = settings_action_context.clone();
                menu.item(
                    PopupMenuItem::new("Edit Settings")
                        .action(Box::new(ToggleSettings))
                        .on_click(move |_, window, app| {
                            action_context.dispatch_action(&ToggleSettings, window, app);
                        }),
                )
            })
            .on_open_change(move |open, window, app| {
                root_menu_open_change(&settings_root, RootMenuKind::Settings, *open, window, app);
            });

        let favorites_root = self.root.clone();
        let help_action_context = self.action_context;
        let help_root = self.root.clone();
        let toast_log_open_pending = self.toast_log_open_pending;
        let help_menu = Button::new("launcher-help-menu")
            .max_w_full()
            .min_w(px(0.0))
            .label("Help")
            .ghost()
            .small()
            .dropdown_menu(move |menu, _, _| {
                let command_list_context = help_action_context.clone();
                let quick_help_context = help_action_context.clone();
                let linking_guide_context = help_action_context.clone();
                let open_action_context = help_action_context.clone();
                let view_action_context = help_action_context.clone();
                let retained_context = help_action_context.clone();
                let coordinate_context = help_action_context.clone();
                menu.item(
                    PopupMenuItem::new("Command List")
                        .action(Box::new(CommandList))
                        .on_click(move |_, window, app| {
                            command_list_context.dispatch_action(&CommandList, window, app);
                        }),
                )
                .item(
                    PopupMenuItem::new("Linking Guide (todo/note/cal)")
                        .action(Box::new(LinkingGuide))
                        .on_click(move |_, window, app| {
                            linking_guide_context.dispatch_action(&LinkingGuide, window, app);
                        }),
                )
                .item(
                    PopupMenuItem::new("Quick Help Overlay")
                        .action(Box::new(QuickHelp))
                        .on_click(move |_, window, app| {
                            quick_help_context.dispatch_action(&QuickHelp, window, app);
                        }),
                )
                .item(
                    PopupMenuItem::new("Open or dismiss retained window")
                        .action(Box::new(ToggleRetainedWindow))
                        .on_click(move |_, window, app| {
                            retained_context.dispatch_action(&ToggleRetainedWindow, window, app);
                        }),
                )
                .item(
                    PopupMenuItem::new("Toggle Coordinate HUD (F6)")
                        .action(Box::new(ToggleCoordinateHudProbe))
                        .on_click(move |_, window, app| {
                            coordinate_context.dispatch_action(&ToggleCoordinateHudProbe, window, app);
                        }),
                )
                .item(
                    PopupMenuItem::new("Open Toast Log")
                        .action(Box::new(OpenToastLog))
                        .disabled(toast_log_open_pending)
                        .on_click(move |_, window, app| {
                            open_action_context.dispatch_action(&OpenToastLog, window, app);
                        }),
                )
                .item(
                    PopupMenuItem::new("View Toast Log")
                        .action(Box::new(ViewToastLog))
                        .on_click(move |_, window, app| {
                            view_action_context.dispatch_action(&ViewToastLog, window, app);
                        }),
                )
            })
            .on_open_change(move |open, window, app| {
                root_menu_open_change(&help_root, RootMenuKind::Help, *open, window, app);
            });

        let saved_buttons = self.saved_pin_order.iter().filter_map(|panel| match panel {
            Panel::FavDialog if self.favorites_pinned => {
                let root = favorites_root.clone();
                Some(Button::new("pinned-favorites-toolbar")
                    .max_w_full().min_w(px(0.0)).label(Panel::FavDialog.display_label())
                    .track_focus(&self.favorites_pin_focus_handle).tab_stop(true)
                    .disabled(self.favorites_toolbar_disabled)
                    .on_click(move |_, window, app| {
                        let _ = root.update(app, |root, cx| root.pinned_favorites_click(window, cx));
                    }).into_any_element())
            }
            Panel::HashGeneratorDialog if self.hash_pinned => {
                let root = self.root.clone();
                Some(Button::new("pinned-hash-toolbar")
                    .max_w_full().min_w(px(0.0)).label(Panel::HashGeneratorDialog.display_label())
                    .track_focus(&self.hash_pin_focus_handle).tab_stop(true)
                    .disabled(self.hash_toolbar_disabled)
                    .on_click(move |_, window, app| {
                        let _ = root.update(app, |root, cx| root.pinned_hash_click(window, cx));
                    }).into_any_element())
            }
            Panel::JsonUtilityDialog if self.json_pinned => {
                let root = self.root.clone();
                Some(Button::new("pinned-json-toolbar").label(Panel::JsonUtilityDialog.display_label())
                    .track_focus(&self.json_pin_focus_handle).tab_stop(true).disabled(self.structured_pin_disabled)
                    .on_click(move |_, window, app| { let _ = root.update(app, |root, cx| root.pinned_structured_click(Panel::JsonUtilityDialog, window, cx)); }).into_any_element())
            }
            Panel::XmlToolkitDialog if self.xml_pinned => {
                let root = self.root.clone();
                Some(Button::new("pinned-xml-toolbar").label(Panel::XmlToolkitDialog.display_label())
                    .track_focus(&self.xml_pin_focus_handle).tab_stop(true).disabled(self.structured_pin_disabled)
                    .on_click(move |_, window, app| { let _ = root.update(app, |root, cx| root.pinned_structured_click(Panel::XmlToolkitDialog, window, cx)); }).into_any_element())
            }
            _ => None,
        }).collect::<Vec<_>>();
        let menus = div()
            .min_h(px(self.row_height))
            .flex()
            .flex_row()
            .flex_wrap()
            .w_full()
            .min_w(px(0.0))
            .items_center()
            .gap_x(px(6.0))
            .gap_y(px(6.0))
            .child(file_menu)
            .child(settings_menu)
            .child(help_menu)
            .children(saved_buttons);
        div()
            .flex()
            .flex_col()
            .w_full()
            .min_w(px(0.0))
            .child(menus)
            .children(self.favorites_unpin_status.map(|status| {
                Label::new(status)
                    .id("pinned-favorites-toolbar-status")
                    .w_full()
                    .min_w(px(0.0))
                    .overflow_hidden()
            }))
    }
}

#[derive(IntoElement)]
struct ToastLogViewerPanel {
    root: WeakEntity<RootWindow>,
    rows: Vec<String>,
    reading: bool,
    status: Option<String>,
    width: f32,
    metrics: multi_launcher::gui::theme::ThemeMetrics,
    palette: multi_launcher::gui::theme::SemanticPalette,
    owner_focus_handle: FocusHandle,
    close_focus_handle: FocusHandle,
    scroll: ScrollHandle,
}

#[derive(Clone, Copy)]
enum RootMenuKind {
    File,
    Settings,
    Help,
}

fn root_menu_open_change(
    root: &WeakEntity<RootWindow>,
    menu: RootMenuKind,
    open: bool,
    window: &mut Window,
    app: &mut App,
) {
    let _ = root.update(app, |root, cx| {
        root.set_root_menu_open(menu, open, window, cx);
    });
}

impl RenderOnce for ToastLogViewerPanel {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let status = if self.reading {
            Some("Refreshing Toast Log…".to_owned())
        } else {
            self.status
        };
        let metrics = self.metrics;
        let footer_height = metrics.row_min_height + 2.0 * metrics.button_padding_y;
        let reserved_height = 2.0 * metrics.window_padding
            + 2.0 * metrics.panel_padding
            + metrics.heading_text_size.max(metrics.line_height)
            + metrics.line_height
            + footer_height
            + 3.0 * metrics.item_spacing_y;
        let body_height =
            toast_log_viewport_body_height(window.viewport_size().height.as_f32(), reserved_height);
        let refresh_root = self.root.clone();
        let close_root = self.root;
        div()
            .id("toast-log-viewer")
            .track_focus(&self.owner_focus_handle)
            .key_context("ToastLogViewer")
            .flex()
            .flex_col()
            .gap_y(px(self.metrics.item_spacing_y))
            .w(px(self.width))
            .max_w_full()
            .p(px(self.metrics.panel_padding))
            .bg(appearance::to_hsla(self.palette.panel_surface))
            .rounded(px(self.metrics.corner_radius))
            .text_color(appearance::to_hsla(self.palette.primary_text))
            .text_size(px(self.metrics.body_text_size))
            .line_height(px(self.metrics.line_height))
            .child(
                Label::new("Toast Log")
                    .text_size(px(metrics.heading_text_size))
                    .line_height(px(metrics.line_height)),
            )
            .child(
                div()
                    .id("toast-log-lines")
                    .flex()
                    .flex_col()
                    .gap_y(px(metrics.item_spacing_y))
                    .w_full()
                    .h(px(body_height))
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .children(self.rows.into_iter().enumerate().map(|(index, line)| {
                        Label::new(line)
                            .id(format!("toast-log-line-{index}"))
                            .line_height(px(metrics.line_height))
                    })),
            )
            .child(
                div()
                    .h(px(metrics.line_height))
                    .children(status.map(|message| {
                        Label::new(message)
                            .id("toast-log-status")
                            .text_color(appearance::to_hsla(self.palette.warning))
                            .text_size(px(metrics.small_text_size))
                            .line_height(px(metrics.line_height))
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_x(px(metrics.item_spacing_x))
                    .child(
                        Button::new("toast-log-refresh")
                            .label("Refresh")
                            .tab_stop(true)
                            .on_click(move |_, _, app| {
                                let _ = refresh_root.update(app, |root, cx| {
                                    root.refresh_toast_log(cx);
                                });
                            }),
                    )
                    .child(
                        Button::new("toast-log-close")
                            .label("Close")
                            .tab_stop(true)
                            .track_focus(&self.close_focus_handle)
                            .on_click(move |_, window, app| {
                                let _ = close_root.update(app, |root, cx| {
                                    root.close_toast_log_viewer(window, cx, true);
                                });
                            }),
                    ),
            )
    }
}

#[derive(IntoElement)]
struct SettingsEditorPanel {
    root: WeakEntity<RootWindow>,
    navigation: Option<Entity<settings_navigation::SettingsNavigation>>,
    scroll: ScrollHandle,
    search: Option<Entity<settings_search::SearchSettings>>,
    actions: Option<Entity<settings_actions::ActionsSettings>>,
    busy: bool,
    status: Option<String>, index_busy: bool, index_status: Option<String>, width: f32,
    metrics: multi_launcher::gui::theme::ThemeMetrics,
    palette: multi_launcher::gui::theme::SemanticPalette,
    settings_owner_focus_handle: FocusHandle,
}

#[derive(IntoElement)]
struct CommandHelpPanel {
    root: WeakEntity<RootWindow>,
    providers: Arc<[CommandHelpProvider]>,
    filter_state: Entity<InputState>,
    filter: String,
    show_examples: bool,
    saving: bool,
    status: Option<String>,
    width: f32,
    metrics: multi_launcher::gui::theme::ThemeMetrics,
    palette: multi_launcher::gui::theme::SemanticPalette,
    owner_focus_handle: FocusHandle,
    close_focus_handle: FocusHandle,
    scroll: ScrollHandle,
}

impl RenderOnce for CommandHelpPanel {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let metrics = self.metrics;
        let status_color = if self.saving {
            self.palette.warning
        } else if self
            .status
            .as_deref()
            .is_some_and(|status| status.starts_with("Could not") || status.contains("not the requested"))
        {
            self.palette.error
        } else {
            self.palette.success
        };
        // The default Kit Input and Button are each two rems tall. Reserve
        // the message slot before any receipt so it cannot shift Close.
        let control_height = 2.0 * window.rem_size().as_f32();
        let status_height = 2.0 * metrics.line_height;
        let reserved_height = 2.0 * metrics.window_padding
            + 2.0 * metrics.panel_padding
            + metrics.heading_text_size.max(metrics.line_height)
            + metrics.line_height.max(18.0)
            + 2.0 * control_height
            + status_height
            + 5.0 * metrics.item_spacing_y;
        let body_height = command_help_viewport_body_height(
            window.viewport_size().height.as_f32(),
            reserved_height,
        );
        let filtered = self
            .providers
            .iter()
            .filter(|provider| {
                matches_filter(
                    &self.filter,
                    &provider.name,
                    &provider.description,
                    &provider.capabilities,
                    &provider.commands,
                )
            })
            .collect::<Vec<_>>();
        let filter_is_empty = self.filter.is_empty();

        let checkbox_root = self.root.clone();
        let show_examples = Checkbox::new("command-help-show-examples")
            .checked(self.show_examples)
            .disabled(self.saving)
            .accessibility_label("Show examples")
            .on_change(move |state, _, _, app| {
                let _ = checkbox_root.update(app, |root, cx| {
                    root.begin_command_help_save(state == CheckboxState::Checked, cx);
                });
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_x(px(metrics.item_spacing_x))
                    .child(
                        CheckboxIndicator::new()
                            .checked(self.show_examples)
                            .size(px(18.0))
                            .rounded(px(3.0))
                            .border_1()
                            .border_color(appearance::to_hsla(self.palette.control_border))
                            .when(self.show_examples, |indicator| {
                                indicator
                                    .bg(appearance::to_hsla(self.palette.accent))
                                    .text_color(appearance::to_hsla(self.palette.primary_text))
                            })
                            .text_size(px(metrics.small_text_size))
                            .items_center()
                            .justify_center()
                            .child(if self.show_examples { "✓" } else { "" }),
                    )
                    .child(Label::new("Show examples")),
            );

        let body = div()
            .id("command-help-content")
            .flex()
            .flex_col()
            .gap_y(px(metrics.item_spacing_y))
            .w_full()
            .h(px(body_height))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .children(filtered.iter().map(|provider| {
                let prefixes = command_prefixes(&provider.commands);
                let mut section = div()
                    .id(format!("command-help-provider-{}", provider.name))
                    .flex()
                    .flex_col()
                    .gap_y(px(metrics.item_spacing_y))
                    .w_full()
                    .child(Label::new(provider.name.clone()).text_size(px(metrics.heading_text_size)))
                    .child(Label::new(provider.description.clone()).w_full())
                    .child(Label::new(if provider.capabilities.is_empty() {
                        "Capabilities: none".to_owned()
                    } else {
                        format!("Capabilities: {}", provider.capabilities.join(", "))
                    }).w_full())
                    .child(Label::new(if prefixes.is_empty() {
                        "Prefixes: none".to_owned()
                    } else {
                        format!("Prefixes: {}", prefixes.join(", "))
                    }).w_full());
                if provider.commands.is_empty() {
                    section = section.child(Label::new("Usage: none"));
                } else {
                    section = section.child(Label::new("Usage:"));
                    for (index, command) in provider.commands.iter().enumerate() {
                        section = section.child(
                            Label::new(format!("  {}", command.label))
                                .id(format!("command-help-usage-{index}"))
                                .w_full()
                                .text_size(px(metrics.small_text_size)),
                        );
                    }
                    if self.show_examples {
                        section = section.child(Label::new("Examples:"));
                        for (index, command) in provider.commands.iter().enumerate() {
                            section = section.child(
                                Label::new(format!("  {}", example_line(command)))
                                    .id(format!("command-help-example-{index}"))
                                    .w_full()
                                    .text_size(px(metrics.small_text_size)),
                            );
                        }
                    }
                }
                section.child(Label::new("────────────────"))
            }))
            .children((filtered.is_empty() && !filter_is_empty)
                .then(|| Label::new("No commands match the filter.")));

        let close_root = self.root;
        div()
            .id("command-help")
            .track_focus(&self.owner_focus_handle)
            .key_context("CommandHelp")
            .flex()
            .flex_col()
            .gap_y(px(metrics.item_spacing_y))
            .w(px(self.width))
            .max_w_full()
            .p(px(metrics.panel_padding))
            .bg(appearance::to_hsla(self.palette.panel_surface))
            .rounded(px(metrics.corner_radius))
            .text_color(appearance::to_hsla(self.palette.primary_text))
            .text_size(px(metrics.body_text_size))
            .line_height(px(metrics.line_height))
            .child(
                Label::new("Available commands")
                    .text_size(px(metrics.heading_text_size))
                    .line_height(px(metrics.line_height)),
            )
            .child(show_examples)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_x(px(metrics.item_spacing_x))
                    .child(Label::new("Filter:"))
                    .child(Input::new(&self.filter_state).aria_label("Filter")),
            )
            .child(body)
            .child(
                div().h(px(status_height)).overflow_hidden().children(
                    self.saving
                        .then(|| "Saving Show examples…".to_owned())
                        .or(self.status)
                        .map(|status| {
                            Label::new(status)
                                .id("command-help-status")
                                .text_color(appearance::to_hsla(status_color))
                                .text_size(px(metrics.small_text_size))
                                .line_height(px(metrics.line_height))
                        }),
                ),
            )
            .child(
                Button::new("command-help-close")
                    .label("Close")
                    .disabled(self.saving)
                    .tab_stop(true)
                    .track_focus(&self.close_focus_handle)
                    .on_click(move |_, window, app| {
                        let _ = close_root.update(app, |root, cx| {
                            root.close_command_help(window, cx, true);
                        });
                    }),
            )
    }
}

#[derive(IntoElement)]
struct QuickHelpPanel {
    root: WeakEntity<RootWindow>,
    providers: Arc<[QuickHelpProvider]>,
    filter_state: Entity<InputState>,
    filter: String,
    hotkeys: Arc<[QuickHelpHotkeyLine]>,
    paste_sequence_summary: String,
    sequence_starts: Arc<[String]>,
    status: Option<String>,
    width: f32,
    metrics: multi_launcher::gui::theme::ThemeMetrics,
    palette: multi_launcher::gui::theme::SemanticPalette,
    owner_focus_handle: FocusHandle,
    close_focus_handle: FocusHandle,
    scroll: ScrollHandle,
}

impl RenderOnce for QuickHelpPanel {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let metrics = self.metrics;
        let control_height = 2.0 * window.rem_size().as_f32();
        let status_height = self
            .status
            .as_ref()
            .map_or(0.0, |_| 3.0 * metrics.line_height.max(18.0));
        let reserved_height = 2.0 * metrics.window_padding
            + 2.0 * metrics.panel_padding
            + metrics.heading_text_size.max(metrics.line_height)
            + control_height
            + control_height
            + status_height
            + 5.0 * metrics.item_spacing_y;
        let body_height = quick_help_viewport_body_height(
            window.viewport_size().height.as_f32(),
            reserved_height,
        );
        let filter_is_empty = self.filter.is_empty();
        let providers = self
            .providers
            .iter()
            .enumerate()
            .filter(|(_, provider)| {
                matches_provider_filter(&self.filter, &provider.name, &provider.description)
            })
            .collect::<Vec<_>>();

        let mut body = div()
            .id("quick-help-content")
            .flex()
            .flex_col()
            .gap_y(px(metrics.item_spacing_y))
            .w_full()
            .h(px(body_height))
            .overflow_y_scroll()
            .track_scroll(&self.scroll);
        for (block_index, block) in QUICK_HELP_GUIDE.iter().enumerate() {
            let block_id = format!("quick-help-guide-block-{block_index:02}");
            body = match *block {
                GuideBlock::Heading(text) => body.child(
                    Label::new(text)
                        .id(block_id)
                        .text_size(px(metrics.heading_text_size))
                        .font_weight(FontWeight::BOLD)
                        .line_height(px(metrics.line_height)),
                ),
                GuideBlock::Paragraph(text) | GuideBlock::WrappedParagraph(text) => {
                    body.child(Label::new(text).id(block_id).w_full())
                }
                GuideBlock::Monospace(text) => body.child(
                    Label::new(text)
                        .id(block_id)
                        .font_family("monospace")
                        .text_size(px(metrics.small_text_size))
                        .w_full(),
                ),
                GuideBlock::Separator => body.child(
                    div()
                        .id(block_id)
                        .w_full()
                        .h(px(1.0))
                        .bg(appearance::to_hsla(self.palette.control_border)),
                ),
                GuideBlock::Hotkeys => body.child(
                    div()
                        .id(block_id)
                        .flex()
                        .flex_col()
                        .w_full()
                        .gap_y(px(metrics.item_spacing_y))
                        .children(self.hotkeys.iter().enumerate().map(|(index, line)| {
                            Label::new(format!("{}: {}", line.label, line.chord))
                                .id(format!("quick-help-hotkey-{index}"))
                                .w_full()
                        })),
                ),
                GuideBlock::PasteSequenceSummary => body.child(
                    div()
                        .id(block_id)
                        .flex()
                        .flex_col()
                        .w_full()
                        .gap_y(px(metrics.item_spacing_y))
                        .children(
                            self.paste_sequence_summary
                                .lines()
                                .enumerate()
                                .map(|(index, line)| {
                                    Label::new(line.to_owned())
                                        .id(format!("quick-help-paste-status-{index}"))
                                        .w_full()
                                }),
                        ),
                ),
                GuideBlock::SequenceStarts => body.child(
                    div()
                        .id(block_id)
                        .flex()
                        .flex_col()
                        .w_full()
                        .gap_y(px(metrics.item_spacing_y))
                        .children(self.sequence_starts.iter().enumerate().map(
                            |(index, line)| {
                                Label::new(line.clone())
                                    .id(format!("quick-help-sequence-start-{index}"))
                                    .w_full()
                            },
                        )),
                ),
                GuideBlock::Providers => body.child(
                    div()
                        .id(block_id)
                        .flex()
                        .flex_col()
                        .w_full()
                        .gap_y(px(metrics.item_spacing_y))
                        .children(providers.iter().map(|(index, provider)| {
                            Label::new(format!("{}: {}", provider.name, provider.description))
                                .id(format!("quick-help-provider-{index}-{}", provider.name))
                                .w_full()
                        }))
                        .children((providers.is_empty() && !filter_is_empty).then(|| {
                            Label::new("No providers match the filter.")
                                .id("quick-help-provider-empty")
                                .w_full()
                        })),
                ),
            };
        }

        let close_root = self.root;
        div()
            .id("quick-help")
            .track_focus(&self.owner_focus_handle)
            .key_context("QuickHelp")
            .flex()
            .flex_col()
            .gap_y(px(metrics.item_spacing_y))
            .w(px(self.width))
            .max_w_full()
            .p(px(metrics.panel_padding))
            .bg(appearance::to_hsla(self.palette.panel_surface))
            .rounded(px(metrics.corner_radius))
            .text_color(appearance::to_hsla(self.palette.primary_text))
            .text_size(px(metrics.body_text_size))
            .line_height(px(metrics.line_height))
            .child(
                Label::new("Quick Help Overlay")
                    .text_size(px(metrics.heading_text_size))
                    .line_height(px(metrics.line_height)),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_x(px(metrics.item_spacing_x))
                    .child(Label::new("Filter:"))
                    .child(Input::new(&self.filter_state).aria_label("Filter Quick Help providers")),
            )
            .child(body)
            .child(
                div().h(px(status_height)).overflow_hidden().children(
                    self.status.map(|status| {
                        Label::new(status)
                            .id("quick-help-shortcut-status")
                            .text_color(appearance::to_hsla(self.palette.error))
                            .text_size(px(metrics.small_text_size))
                            .line_height(px(metrics.line_height))
                            .w_full()
                    }),
                ),
            )
            .child(
                Button::new("quick-help-close")
                    .label("Close")
                    .tab_stop(true)
                    .track_focus(&self.close_focus_handle)
                    .on_click(move |_, window, app| {
                        let _ = close_root.update(app, |root, cx| {
                            root.close_quick_help(window, cx, true);
                        });
                    }),
            )
    }
}

impl RenderOnce for SettingsEditorPanel {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let reload_root = self.root.clone();
        let cancel_root = self.root;
        let scroll = self.scroll.clone();
        let line_height = px(self.metrics.line_height);
        div().id("settings-editor").track_focus(&self.settings_owner_focus_handle).key_context("SettingsEditor")
            .flex().flex_col().gap_y(px(self.metrics.item_spacing_y)).w(px(self.width)).max_w_full()
            .max_h(px(650.0)).overflow_y_scroll().track_scroll(&self.scroll)
            .on_scroll(move |event, _, cx| {
                let pixels = event.delta.pixel_delta(line_height);
                let delta = if pixels.y.is_zero() { pixels.x } else { pixels.y }.as_f32();
                let offset = scroll.offset(); let maximum = scroll.max_offset().y.as_f32();
                let previous = (offset.y.as_f32() - delta).clamp(-maximum, 0.0);
                scroll.set_offset(point(offset.x, px(offset.y.as_f32().clamp(-maximum, 0.0))));
                if inner_results_consumed_scroll(previous, maximum, delta) { cx.stop_propagation(); }
            })
            .p(px(self.metrics.panel_padding)).bg(appearance::to_hsla(self.palette.panel_surface))
            .rounded(px(self.metrics.corner_radius)).text_color(appearance::to_hsla(self.palette.primary_text))
            .text_size(px(self.metrics.body_text_size)).line_height(px(self.metrics.line_height))
            .child(Label::new("Settings").text_size(px(self.metrics.heading_text_size)))
            .when_some(self.navigation, |panel, navigation| panel.child(navigation))
            .when_some(self.search, |panel, search| panel.child(search))
            .when_some(self.actions, |panel, actions| panel.child(actions))
            .child(Button::new("settings-reload-application-index").label("Reload application index")
                .disabled(self.index_busy || self.busy).on_click(move |_, _, app| {
                    let _ = reload_root.update(app, |root, cx| root.reload_application_index(cx));
                }))
            .children(self.index_status.map(|status| Label::new(status).id("settings-index-status")))
            .children(self.status.map(|status| Label::new(status).id("settings-save-status")))
            .child(Button::new("settings-cancel").label("Cancel").disabled(self.busy)
                .on_click(move |_, window, app| { let _ = cancel_root.update(app, |root, cx| root.cancel_settings_editor(window, cx, true)); }))
    }
}

impl SearchCompletion {
    fn clear_for_query_replacement(&mut self) {
        self.suggestions.clear();
        self.next_index = 0;
        self.pending_change = None;
    }

    fn reset_for_query(&mut self, catalog: &LauncherSearchCatalog, query: &str) {
        self.pending_change = None;
        self.next_index = 0;
        self.suggestions = if catalog.query_autocomplete {
            let mut suggestions = multi_launcher::completion::single_line_suggestions(
                &catalog.completion_index, query, 5,
            );
            if let Some(index) = &catalog.indexed_completion_index {
                suggestions.extend(multi_launcher::completion::single_line_suggestions(index, query, 5));
                suggestions.sort();
                suggestions.dedup();
                suggestions.truncate(5);
            }
            suggestions
        } else {
            Vec::new()
        };
    }

    fn apply_change(&mut self, catalog: &LauncherSearchCatalog, query: &str) {
        match self.pending_change.take() {
            Some(pending) if pending.expected_query == query => {
                self.next_index = pending.next_index;
            }
            _ => self.reset_for_query(catalog, query),
        }
    }

    fn begin_replacement(&mut self, query: &str) -> Option<String> {
        if self.pending_change.is_some() {
            return None;
        }
        let candidate = multi_launcher::completion::checked_tab_candidate(
            &self.suggestions,
            self.next_index,
            query,
        )?
        .to_owned();
        let next_index =
            multi_launcher::completion::next_tab_index(self.next_index, self.suggestions.len())?;
        self.pending_change = Some(PendingCompletionChange {
            expected_query: candidate.clone(),
            next_index,
        });
        Some(candidate)
    }
}

fn supported_launcher_search_actions(actions: Vec<Action>) -> Vec<Action> {
    actions
        .into_iter()
        .filter(is_supported_launcher_search_action)
        .collect()
}

fn is_downloads_triage_opener(action: &Action) -> bool {
    action.args.is_none() && matches!(parse_action(action), Ok(Command::Dialog(DialogCommand::DownloadsTriage)))
}

fn is_file_search_opener(query: &str, action: &Action) -> bool {
    query.trim().eq_ignore_ascii_case("fs")
        && action.args.is_none()
        && matches!(
            parse_action(action),
            Ok(Command::FileSearch(multi_launcher::commands::FileSearchCommand::Open))
        )
}

fn is_supported_launcher_search_action(action: &Action) -> bool {
    match parse_action(action) {
        Ok(command) => {
            classify_installed_search_command(&command, ActivationSource::Click).is_ok()
        }
        Err(_) => false,
    }
}

fn supported_saved_search_actions(actions: Vec<Action>, verified_folders: &[Action]) -> Vec<Action> {
    actions
        .into_iter()
        .filter(|action| {
            verified_folders.contains(action) || parse_action(action).is_ok_and(|command| {
                #[cfg(windows)]
                if external_launch::shortcut_external(action, &command).is_some() {
                    return true;
                }
                external_launch::ordinary_external(&command).is_some()
                    || external_launch::document_external(action, &command).is_some()
                    || external_launch::url_external(action, &command).is_some()
                    || classify_installed_search_command(&command, ActivationSource::Click).is_ok()
            })
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
enum InstalledSearchRoute {
    HashOpen,
    QuerySet(CommandOutcome),
    Settings,
    Help,
    Activity,
    JsonUtility(multi_launcher::commands::JsonUtilityIntent),
    XmlUtility,
    RegexTester,
    ClipboardHistory,
    ClipboardModify(multi_launcher::clipboard_modify::actions::ClipboardModifySectionPayload),
    FavoritesBrowse,
    FavoritesNamed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum InstalledSearchActionError {
    Unsupported(String),
    QuerySet(String),
}

fn query_set_outcome_text(outcome: &CommandOutcome) -> Option<&str> {
    let QueryPolicy::Set(query) = &outcome.query else {
        return None;
    };
    (!query.contains(['\r', '\n'])
        && matches!(&outcome.pending_query, PendingQueryPolicy::Keep)
        && outcome.search
        && !outcome.invalidate_results
        && matches!(&outcome.results, ResultsPolicy::Keep)
        && outcome.visibility == VisibilityPolicy::Show
        && outcome.restore
        && outcome.focus
        && outcome.move_cursor_end
        && outcome.activate_first_result.is_none()
        && outcome.history == HistoryPolicy::Skip
        && outcome.toasts.is_empty()
        && matches!(&outcome.favorite_log, FavoriteLogPolicy::None))
    .then_some(query)
}

fn classify_installed_search_command(
    command: &Command,
    source: ActivationSource,
) -> Result<InstalledSearchRoute, InstalledSearchActionError> {
    match command {
        Command::HashGenerator(HashGeneratorCommand::Open) => Ok(InstalledSearchRoute::HashOpen),
        Command::Query(command @ QueryCommand::Set { .. }) => {
            let outcome = handle_query(command, source);
            if query_set_outcome_text(&outcome).is_none() {
                return Err(InstalledSearchActionError::QuerySet(
                    "The saved Query Set outcome cannot be represented by single-line Search."
                        .to_owned(),
                ));
            }
            Ok(InstalledSearchRoute::QuerySet(outcome))
        }
        Command::Dialog(DialogCommand::Settings) => Ok(InstalledSearchRoute::Settings),
        Command::Dialog(DialogCommand::Help) => Ok(InstalledSearchRoute::Help),
        Command::Dialog(DialogCommand::Activity) => Ok(InstalledSearchRoute::Activity),
        Command::Dialog(DialogCommand::RegexTester) => Ok(InstalledSearchRoute::RegexTester),
        Command::JsonUtility(multi_launcher::commands::JsonUtilityCommand::Open { intent }) => Ok(InstalledSearchRoute::JsonUtility(*intent)),
        Command::XmlToolkit(multi_launcher::commands::XmlToolkitCommand::Open) => Ok(InstalledSearchRoute::XmlUtility),
        Command::Storage(StorageCommand::FavoriteDialog(label)) => {
            if label.is_empty() {
                Ok(InstalledSearchRoute::FavoritesBrowse)
            } else {
                Ok(InstalledSearchRoute::FavoritesNamed(label.clone()))
            }
        }
        _ => Err(InstalledSearchActionError::Unsupported(
            "The selected launcher action is not an installed Search route.".to_owned(),
        )),
    }
}

#[derive(Clone, Debug, PartialEq)]
struct InstalledSearchInvocation {
    invocation: CommandInvocation,
    route: InstalledSearchRoute,
}

fn parse_installed_search_invocation(
    action: Action,
    source: ActivationSource,
) -> Result<InstalledSearchInvocation, InstalledSearchActionError> {
    let invocation = parse_command(action, None, source)
        .map_err(|error| InstalledSearchActionError::Unsupported(error.to_string()))?;
    let route = classify_installed_search_command(&invocation.command, source)?;
    Ok(InstalledSearchInvocation { invocation, route })
}

fn dispatch_hash_open(
    invocation: CommandInvocation,
    open_hash: impl FnOnce() -> Result<(), String>,
) -> Result<CommandOutcome, String> {
    match invocation.command {
        Command::HashGenerator(command) => handle_hash_generator(&command, open_hash),
        _ => Err("The selected launcher action is not an installed Hash Generator route.".into()),
    }
}

fn single_line_history_queries<'a>(queries: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    queries
        .into_iter()
        .filter(|query| !query.contains(['\n', '\r']))
        .map(str::to_owned)
        .collect()
}

#[derive(Clone, Copy)]
enum GpuiBoundAction {
    OpenActionSheet,
    ToggleCoordinateHudProbe,
    OpenHashGenerator,
    FocusDockedHashResize,
    RestartWithEgui,
    SearchHistoryOlder,
    SearchHistoryNewer,
    CalculateHash,
    DockHashGenerator,
    PopoutHashGenerator,
    CloseHashGenerator,
}

#[derive(Clone, Copy)]
struct GpuiBoundKey {
    chord: &'static str,
    context: &'static str,
    label: &'static str,
    action: GpuiBoundAction,
}

// Keep the shortcut shown in Quick Help and optional process-level shortcut
// collision checks tied to the exact strings registered by GPUI below.
const GPUI_BOUND_KEYS: &[GpuiBoundKey] = &[
    GpuiBoundKey {
        chord: "ctrl-enter",
        context: "LauncherSearch",
        label: "Open action sheet",
        action: GpuiBoundAction::OpenActionSheet,
    },
    GpuiBoundKey {
        chord: "f6",
        context: "RootWindow",
        label: "Toggle passive Coordinate HUD",
        action: GpuiBoundAction::ToggleCoordinateHudProbe,
    },
    GpuiBoundKey {
        chord: "ctrl-g",
        context: "RootWindow",
        label: "Open Hash Generator",
        action: GpuiBoundAction::OpenHashGenerator,
    },
    GpuiBoundKey {
        chord: "ctrl-shift-r",
        context: "RootWindow",
        label: "Focus docked Hash resize",
        action: GpuiBoundAction::FocusDockedHashResize,
    },
    GpuiBoundKey {
        chord: "ctrl-shift-e",
        context: "RootWindow",
        label: "Restart in egui",
        action: GpuiBoundAction::RestartWithEgui,
    },
    GpuiBoundKey {
        chord: "ctrl-up",
        context: "LauncherSearch",
        label: "Older query",
        action: GpuiBoundAction::SearchHistoryOlder,
    },
    GpuiBoundKey {
        chord: "ctrl-down",
        context: "LauncherSearch",
        label: "Newer query",
        action: GpuiBoundAction::SearchHistoryNewer,
    },
    GpuiBoundKey {
        chord: "ctrl-enter",
        context: "HashGeneratorWindow",
        label: "Calculate Hash Generator",
        action: GpuiBoundAction::CalculateHash,
    },
    GpuiBoundKey {
        chord: "ctrl-shift-d",
        context: "HashGeneratorWindow",
        label: "Dock Hash Generator",
        action: GpuiBoundAction::DockHashGenerator,
    },
    GpuiBoundKey {
        chord: "ctrl-shift-p",
        context: "HashGeneratorWindow",
        label: "Pop Out Hash Generator",
        action: GpuiBoundAction::PopoutHashGenerator,
    },
    GpuiBoundKey {
        chord: "ctrl-w",
        context: "HashGeneratorWindow",
        label: "Close Hash Generator",
        action: GpuiBoundAction::CloseHashGenerator,
    },
];

fn gpui_key_bindings() -> Vec<KeyBinding> {
    GPUI_BOUND_KEYS
        .iter()
        .map(|binding| match binding.action {
            GpuiBoundAction::OpenActionSheet => KeyBinding::new(
                binding.chord, OpenActionSheet, Some(binding.context),
            ),
            GpuiBoundAction::ToggleCoordinateHudProbe => KeyBinding::new(
                binding.chord,
                ToggleCoordinateHudProbe,
                Some(binding.context),
            ),
            GpuiBoundAction::OpenHashGenerator => {
                KeyBinding::new(binding.chord, OpenHashGenerator, Some(binding.context))
            }
            GpuiBoundAction::FocusDockedHashResize => KeyBinding::new(
                binding.chord,
                FocusDockedHashResize,
                Some(binding.context),
            ),
            GpuiBoundAction::RestartWithEgui => {
                KeyBinding::new(binding.chord, RestartWithEgui, Some(binding.context))
            }
            GpuiBoundAction::SearchHistoryOlder => {
                KeyBinding::new(binding.chord, SearchHistoryOlder, Some(binding.context))
            }
            GpuiBoundAction::SearchHistoryNewer => {
                KeyBinding::new(binding.chord, SearchHistoryNewer, Some(binding.context))
            }
            GpuiBoundAction::CalculateHash => {
                KeyBinding::new(binding.chord, CalculateHash, Some(binding.context))
            }
            GpuiBoundAction::DockHashGenerator => {
                KeyBinding::new(binding.chord, DockHashGenerator, Some(binding.context))
            }
            GpuiBoundAction::PopoutHashGenerator => {
                KeyBinding::new(binding.chord, PopoutHashGenerator, Some(binding.context))
            }
            GpuiBoundAction::CloseHashGenerator => {
                KeyBinding::new(binding.chord, CloseHashGenerator, Some(binding.context))
            }
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct QuickHelpShortcut {
    hotkey: Hotkey,
    display: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct QuickHelpShortcutAdmission {
    shortcut: Option<QuickHelpShortcut>,
    diagnostic: Option<String>,
}

fn admit_quick_help_shortcut(
    configured: Option<&str>,
    launcher: Hotkey,
    jump_mode_enabled: bool,
    jump_chord: JumpModeChord,
) -> QuickHelpShortcutAdmission {
    let Some(configured) = configured else {
        return QuickHelpShortcutAdmission {
            shortcut: None,
            diagnostic: None,
        };
    };
    let unavailable = |reason: &str| QuickHelpShortcutAdmission {
        shortcut: None,
        diagnostic: Some(format!(
            "Configured Help shortcut {:?} {reason}; use Help → Quick Help Overlay instead.",
            configured.trim()
        )),
    };
    let Some(hotkey) = parse_hotkey(configured) else {
        return unavailable("is invalid");
    };
    if hotkeys_can_cofire(hotkey, launcher) {
        return unavailable("can co-fire with the launcher shortcut");
    }
    if legacy_virtual_key(hotkey.key).is_none() {
        return unavailable("is unsupported by the Windows GPUI listener");
    }
    if jump_mode_enabled
        && jump_chord_conflict(jump_chord, [("Help shortcut", hotkey)]).is_some()
    {
        return unavailable(&format!(
            "conflicts with the enabled Search Jump shortcut {:?}",
            jump_chord.canonical_text()
        ));
    }
    if let Some(binding) = GPUI_BOUND_KEYS.iter().find(|binding| {
        parse_gpui_bound_chord(binding.chord)
            .is_some_and(|local| polling_hotkey_can_cofire_with_exact_binding(hotkey, local))
    }) {
        return unavailable(&format!(
            "can co-fire with GPUI {} ({})",
            binding.label, binding.chord
        ));
    }
    QuickHelpShortcutAdmission {
        shortcut: Some(QuickHelpShortcut {
            hotkey,
            display: configured.trim().to_owned(),
        }),
        diagnostic: None,
    }
}

fn parse_gpui_bound_chord(chord: &str) -> Option<Hotkey> {
    parse_hotkey(&chord.replace('-', "+"))
}

fn polling_hotkey_can_cofire_with_exact_binding(polling: Hotkey, local: Hotkey) -> bool {
    polling.key == local.key
        && (!polling.ctrl || local.ctrl)
        && (!polling.shift || local.shift)
        && (!polling.alt || local.alt)
        && (!polling.alt_gr || local.alt_gr)
        && (!polling.win || local.win)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct QuickHelpHotkeyLine {
    label: String,
    chord: String,
}

fn quick_help_hotkey_lines(
    launcher_display: &str,
    help_shortcut: Option<&QuickHelpShortcut>,
) -> Arc<[QuickHelpHotkeyLine]> {
    let mut lines = vec![QuickHelpHotkeyLine {
        label: "Toggle launcher".to_owned(),
        chord: launcher_display.to_owned(),
    }];
    if let Some(shortcut) = help_shortcut {
        lines.push(QuickHelpHotkeyLine {
            label: "Help overlay".to_owned(),
            chord: shortcut.display.clone(),
        });
    }
    lines.extend(GPUI_BOUND_KEYS.iter().map(|binding| QuickHelpHotkeyLine {
        label: binding.label.to_owned(),
        chord: display_gpui_chord(binding.chord),
    }));
    lines.into()
}

fn display_gpui_chord(chord: &str) -> String {
    chord
        .split('-')
        .map(|part| match part {
            "ctrl" => "Ctrl".to_owned(),
            "shift" => "Shift".to_owned(),
            "enter" => "Enter".to_owned(),
            "up" => "Up".to_owned(),
            "down" => "Down".to_owned(),
            value => value.to_ascii_uppercase(),
        })
        .collect::<Vec<_>>()
        .join("+")
}

gpui_kit::actions!([
    OpenActionSheet,
    ToggleCoordinateHudProbe,
    ToggleSettings,
    OpenAppEditor,
    OpenActivity,
    OpenPluginEditor,
    ToggleRetainedWindow,
    CloseApplication,
    CommandList,
    QuickHelp,
    LinkingGuide,
    OpenToastLog,
    ViewToastLog,
    OpenHashGenerator,
    SearchHistoryOlder,
    SearchHistoryNewer,
    FocusDockedHashResize,
    CalculateHash,
    DockHashGenerator,
    PopoutHashGenerator,
    CloseHashGenerator,
    RestartWithEgui
]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrontendSelection {
    DefaultEgui,
    Explicit(Frontend),
}

impl FrontendSelection {
    pub(crate) const fn frontend(self) -> Frontend {
        match self {
            Self::DefaultEgui | Self::Explicit(Frontend::Egui) => Frontend::Egui,
            Self::Explicit(Frontend::Gpui) => Frontend::Gpui,
        }
    }

    const fn requested_frontend(self) -> Option<Frontend> {
        match self {
            Self::DefaultEgui => None,
            Self::Explicit(frontend) => Some(frontend),
        }
    }
}

fn selector_choice_message() -> String {
    format!(
        "Choose {} or {}.",
        Frontend::Egui.selector_argument(),
        Frontend::Gpui.selector_argument()
    )
}

pub(crate) fn selection_from_args(
    args: impl IntoIterator<Item = OsString>,
) -> Result<FrontendSelection, String> {
    let mut selection = FrontendSelection::DefaultEgui;

    for arg in args {
        match arg.as_os_str() {
            value if value == OsStr::new(Frontend::Egui.selector_argument()) => {
                if matches!(selection, FrontendSelection::Explicit(_)) {
                    return Err("Specify only one --ui selector.".to_owned());
                }
                selection = FrontendSelection::Explicit(Frontend::Egui);
            }
            value if value == OsStr::new(Frontend::Gpui.selector_argument()) => {
                if matches!(selection, FrontendSelection::Explicit(_)) {
                    return Err("Specify only one --ui selector.".to_owned());
                }
                selection = FrontendSelection::Explicit(Frontend::Gpui);
            }
            value if value == OsStr::new("--ui") => {
                return Err(selector_choice_message());
            }
            value if value.to_string_lossy().starts_with("--ui=") => {
                return Err(selector_choice_message());
            }
            _ => {}
        }
    }

    Ok(selection)
}

pub(crate) fn duplicate_instance_message(selection: FrontendSelection) -> Option<String> {
    let selector = selection.requested_frontend()?.selector_argument();
    Some(format!(
        "The data directory is already owned by another Multi Launcher instance. Close that instance, then rerun with {selector} from the same working directory."
    ))
}

pub(crate) fn show_startup_error(message: &str) {
    #[cfg(windows)]
    {
        use windows::{
            Win32::{
                Foundation::HWND,
                UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW},
            },
            core::PCWSTR,
        };

        let message: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
        let title: Vec<u16> = "Multi Launcher".encode_utf16().chain(Some(0)).collect();

        // SAFETY: both strings remain alive and NUL-terminated throughout the call.
        unsafe {
            MessageBoxW(
                HWND::default(),
                PCWSTR(message.as_ptr()),
                PCWSTR(title.as_ptr()),
                MB_OK | MB_ICONERROR,
            );
        }
    }

    #[cfg(not(windows))]
    eprintln!("Multi Launcher: {message}");
}

fn toggle_coordinate_hud_probe(
    controller: &mut CoordinateToolController,
) -> Result<String, String> {
    let enabled = !controller.runtime_state().hud_enabled();
    let disabled_sample = if enabled {
        None
    } else {
        controller.sample_for_copy().ok()
    };
    controller.set_hud_enabled(enabled)?;
    if !enabled {
        let dpi = disabled_sample
            .as_ref()
            .and_then(|sample| sample.monitor.as_ref())
            .and_then(|monitor| monitor.effective_dpi);
        return Ok(match dpi {
            Some((horizontal, vertical)) => format!(
                "Coordinate HUD disabled. Last sample monitor DPI: {horizontal}×{vertical}."
            ),
            None => "Coordinate HUD disabled.".to_owned(),
        });
    }

    match controller.sample_for_copy() {
        Ok(sample) => {
            let point = sample.desktop_point;
            let mut status = format!("Coordinate HUD enabled at ({}, {}).", point.x, point.y);
            if let Some((horizontal, vertical)) = sample
                .monitor
                .as_ref()
                .and_then(|monitor| monitor.effective_dpi)
            {
                status.push_str(&format!(" Monitor DPI: {horizontal}×{vertical}."));
            }
            Ok(status)
        }
        Err(error) => Ok(format!(
            "Coordinate HUD enabled; current sample unavailable: {error}"
        )),
    }
}

struct RootWindow {
    root_window: Option<AnyWindowHandle>,
    listener_slot: Arc<Mutex<Option<HotkeyListener>>>,
    root_visibility: RootVisibility,
    query: Entity<InputState>,
    enabled_plugins: Option<HashSet<String>>,
    enabled_capabilities: Option<HashMap<String, Vec<String>>>,
    search_catalog: LauncherSearchCatalog,
    launcher_results: LauncherSearchResults,
    calculator_search: calculator_search::SearchOwner,
    indexed_applications: indexed_applications::Owner,
    window_search: window_search::Owner,
    file_search_probe: file_search::ProbeOwner,
    file_search_popup_intent: Arc<AtomicBool>,
    external_launch: external_launch::ExternalLaunch,
    external_launch_preferences: ExternalLaunchPreferences,
    action_sheet: Option<action_sheet::Sheet>,
    action_sheet_serial: u64,
    context_favorite_popup: Option<ContextFavoritePopup>,
    context_favorite_serial: u64,
    result_favorites: ResultFavorites,
    panel_stack: Vec<Panel>,
    hash_pin: hash_pin::HashPinState,
    structured_pins: structured_pin::StructuredPins,
    launcher_results_scroll: ScrollHandle,
    query_history: QueryHistoryNavigator,
    fresh_search_enter: Option<FreshSearchEnter>,
    search_jump_mode: JumpModeController,
    search_jump_geometry_frame: Rc<RefCell<Option<JumpGeometryFrame>>>,
    search_completion: SearchCompletion,
    search_completion_focus_revision: Option<u64>,
    settings_editor: SettingsEditorState,
    settings_search: Option<Entity<settings_search::SearchSettings>>,
    settings_navigation: Option<Entity<settings_navigation::SettingsNavigation>>,
    settings_panel_scroll: ScrollHandle,
    settings_actions: Option<Entity<settings_actions::ActionsSettings>>,
    app_editor: add_action::AppEditorState,
    plugin_editor: Option<Entity<plugin_editor::PluginEditorView>>,
    command_help: CommandHelpState,
    command_help_filter: Entity<InputState>,
    quick_help: QuickHelpState,
    quick_help_filter: Entity<InputState>,
    shared_help_filter: SharedHelpFilter,
    pending_help_filter_sync: Option<(HelpFilterPeer, String)>,
    quick_help_shortcut_diagnostic: Option<String>,
    quick_help_return_focus: Option<FocusHandle>,
    quick_help_return_scroll_offset: Option<Point<Pixels>>,
    quick_help_owner_focus_handle: FocusHandle,
    quick_help_close_focus_handle: FocusHandle,
    quick_help_scroll: ScrollHandle,
    quick_help_hotkeys: Arc<[QuickHelpHotkeyLine]>,
    quick_help_paste_sequence_summary: String,
    quick_help_sequence_starts: Arc<[String]>,
    settings_path: String,
    settings_save_workers: SettingsSaveWorkerRegistry,
    command_help_save_permit: Option<EffectPermit>,
    command_help_save_token: Option<multi_launcher::settings::SettingsBooleanSaveToken>,
    preference_close_after_terminal: bool,
    preference_close_failed: bool,
    file_menu_open: bool,
    settings_menu_open: bool,
    help_menu_open: bool,
    root_menu_previous_focus: Option<FocusHandle>,
    root_menu_action_routed_focus: bool,
    toast_log_external_open_pending: bool,
    toast_log_external_open_error: Option<String>,
    toast_log_viewer: ToastLogViewerState,
    toast_log_return_focus: Option<FocusHandle>,
    toast_log_return_scroll_offset: Option<Point<Pixels>>,
    toast_log_focus_handle: FocusHandle,
    toast_log_owner_focus_handle: FocusHandle,
    toast_log_scroll: ScrollHandle,
    settings_focus_handle: FocusHandle,
    settings_owner_focus_handle: FocusHandle,
    settings_menu_focus_handle: FocusHandle,
    favorites_pin_focus_handle: FocusHandle,
    hash_pin_focus_handle: FocusHandle,
    json_pin_focus_handle: FocusHandle,
    xml_pin_focus_handle: FocusHandle,
    command_help_owner_focus_handle: FocusHandle,
    command_help_close_focus_handle: FocusHandle,
    command_help_scroll: ScrollHandle,
    command_help_return_focus: Option<FocusHandle>,
    command_help_return_scroll_offset: Option<Point<Pixels>>,
    settings_return_focus: Option<FocusHandle>,
    settings_return_scroll_offset: Option<Point<Pixels>>,
    native_window: Option<NativeWindow>,
    focus_handle: FocusHandle,
    resize_focus_handle: FocusHandle,
    root_scroll: ScrollHandle,
    utility_presentations: UtilityPresentationStore,
    utility_size_store: Arc<Mutex<UtilitySizeStore>>,
    utility_size_polling: Arc<AtomicBool>,
    hash_docked_preferred_size: PreferredWindowSize,
    root_body_viewport: Option<AppliedWindowSize>,
    hash_docked_resize: Arc<Mutex<Option<DockedHashResize>>>,
    hash_geometry_sequence: Arc<AtomicU64>,
    hash_tool_key: UtilityToolKey,
    hash_session: Arc<Mutex<HashSession>>,
    // The root owns the shared center independently of Hash presentation mounts.
    activity_center: multi_launcher::activity::ActivityCenter,
    activity_view: Option<Entity<activity::ActivityView>>,
    json_view: Option<Entity<json_utility::JsonWorkspace>>,
    xml_view: Option<Entity<xml_utility::XmlWorkspace>>,
    regex_view: Option<Entity<regex_tester::RegexWorkspace>>,
    clipboard_history: Option<Entity<clipboard_history::ClipboardHistory>>,
    snippets_view: Option<Entity<snippets::SnippetsView>>,
    tempfile_view: Option<Entity<tempfile_view::TempfileView>>,
    snippet_fill: Option<Entity<snippet_fill::SnippetFill>>,
    pane_escape_latched: bool,
    pane_enter_latched: bool,
    snippets_search: snippets::SearchState,
    clipboard_modify_view: Option<Entity<clipboard_modify_view::ClipboardModifyView>>,
    clipboard_modify_startup: clipboard_modify_startup::Owner,
    clipboard_popup: Option<utility_mount::UtilityPopup>,
    clipboard_retiring: Vec<RetiringHashWindow>,
    clipboard_modify_popup: Option<utility_mount::UtilityPopup>,
    clipboard_modify_retiring: Vec<RetiringHashWindow>,
    xml_popup: Option<utility_mount::UtilityPopup>,
    regex_popup: Option<utility_mount::UtilityPopup>,
    regex_retiring: Vec<RetiringHashWindow>,
    xml_retiring: Vec<RetiringHashWindow>,
    xml_terminal_quit: bool,
    json_popup: Option<utility_mount::UtilityPopup>,
    json_retiring: Vec<RetiringHashWindow>,
    hash_wake_signal: Arc<HashWakeSignal>,
    hash_file_picker_pending: Arc<AtomicBool>,
    hash_clipboard_pending: Arc<HashClipboardPendingState>,
    hash_clipboard_operation: Option<HashClipboardOperation>,
    hash_clipboard_operation_counter: u64,
    file_search: Option<Entity<file_search::FileSearchView>>,
    folder_size_view: Option<Entity<folder_size::FolderSizeView>>,
    file_lock_view: Option<Entity<file_lock::FileLockView>>,
    file_lock_popup: Option<utility_mount::UtilityPopup>,
    file_lock_retiring: Vec<RetiringHashWindow>,
    folder_size_popup: Option<utility_mount::UtilityPopup>,
    folder_size_retiring: Vec<RetiringHashWindow>,
    file_search_popup: Option<utility_mount::UtilityPopup>,
    file_search_retiring: Vec<RetiringHashWindow>,
    #[cfg(windows)]
    downloads_triage: Option<Entity<downloads_triage::DownloadsTriageView>>,
    #[cfg(windows)]
    app_data_root: multi_launcher::platform::app_data::AppDataRoot,
    file_search_settings: multi_launcher::file_search::settings::FileSearchSettings,
    hash_window: HashWindowState,
    hash_return: Option<HashReturnReceipt>,
    hash_window_revision: Arc<AtomicU64>,
    hash_mount_generation: Arc<AtomicU64>,
    hash_mount_counter: u64,
    hash_rehost_pending: Arc<AtomicBool>,
    hash_rehost_revision: Arc<AtomicU64>,
    hash_window_close_subscriptions: Vec<(AnyWindowHandle, Subscription)>,
    hash_retiring_windows: Vec<RetiringHashWindow>,
    hash_presentation_feedback: Arc<Mutex<Option<String>>>,
    launcher_search_error: Option<String>,
    hash_open_error: Option<String>,
    hash_presentation_error: Option<String>,
    hash_native_geometry_error: Option<String>,
    hash_size_write_error: Option<String>,
    secondary: SecondaryWindowState,
    secondary_revision: Arc<AtomicU64>,
    secondary_close_subscription: Option<Subscription>,
    secondary_error: Option<String>,
    coordinate_hud_probe_controller: Arc<Mutex<CoordinateToolController>>,
    coordinate_hud_probe_closing: Arc<AtomicBool>,
    coordinate_hud_probe_pending: bool,
    coordinate_hud_probe_status: String,
    coordinate_shutdown_complete: Arc<AtomicBool>,
    restart_context: Option<RestartContext>,
    restart_admission: Arc<RestartAdmission>,
    restart_retiring: Arc<AtomicBool>,
    restart_status: Option<String>,
    restart_exit: Rc<RefCell<Option<(RestartContext, Vec<NativeWindow>)>>>,
    restart_failure: Rc<RefCell<Option<String>>>,
}

fn new_activity_hash_session() -> (multi_launcher::activity::ActivityCenter, Arc<Mutex<HashSession>>) {
    let center = multi_launcher::activity::ActivityCenter::new();
    let mut session = HashSession::new();
    session.attach_activity_center(center.clone());
    (center, Arc::new(Mutex::new(session)))
}

#[derive(Clone)]
struct ContextFavoriteReceipt {
    serial: u64,
    generation: u64,
    row: usize,
    invocation: UniversalActionInvocationContext,
    action: Action,
    visibility_revision: u64,
    previous_focus: Option<FocusHandle>,
}
struct ContextFavoritePopup {
    receipt: ContextFavoriteReceipt,
    position: Point<Pixels>,
    menu: Option<Entity<PopupMenu>>,
    subscription: Option<Subscription>,
}
impl ContextFavoriteReceipt {
    fn matches(&self, results: &LauncherSearchResults, query: &str, revision: u64) -> bool {
        self.visibility_revision == revision
            && self.invocation.surface == ActionSurface::ContextMenu
            && self.invocation.source == ActivationSource::Click
            && self.invocation.history_query == query
            && results.resolve_action(self.generation, self.row, query).as_ref() == Some(&self.action)
    }
}

#[derive(Default)]
struct LauncherSearchResults {
    generation: u64,
    query: String,
    actions: Vec<LauncherSearchResult>,
    selected: Option<usize>,
    layout: ResolvedQueryResultsLayout,
    page_jump: usize,
}

impl LauncherSearchResults {
    fn replace(
        &mut self,
        query: String,
        actions: Vec<LauncherSearchResult>,
        layout: ResolvedQueryResultsLayout,
        page_jump: usize,
    ) {
        self.generation = self.generation.wrapping_add(1).max(1);
        self.query = query;
        self.actions = actions;
        self.selected = None;
        self.layout = layout;
        self.page_jump = page_jump;
    }

    #[cfg(test)]
    fn replace_test_actions(
        &mut self,
        query: String,
        actions: Vec<Action>,
        layout: ResolvedQueryResultsLayout,
        page_jump: usize,
    ) {
        self.replace(
            query,
            actions
                .into_iter()
                .map(|action| LauncherSearchResult {
                    action,
                    origin: LauncherResultOrigin::Saved,
                    display_label: None,
                    hide_description: false,
                })
                .collect(),
            layout,
            page_jump,
        );
    }

    fn move_selection(&mut self, movement: QuerySelectionMove) -> bool {
        let next = move_query_selection(
            self.selected,
            self.actions.len(),
            self.layout,
            self.page_jump,
            movement,
        );
        let changed = self.selected != next;
        self.selected = next;
        changed
    }

    fn move_keypad_selection(&mut self, movement: QuerySelectionMove) -> bool {
        let next = match movement {
            QuerySelectionMove::Left if !self.layout.grid => None,
            QuerySelectionMove::Right if !self.layout.grid && !self.actions.is_empty() => {
                Some(self.selected.unwrap_or(0))
            }
            QuerySelectionMove::Right if !self.layout.grid => self.selected,
            _ => {
                return self.move_selection(movement);
            }
        };
        let changed = self.selected != next;
        self.selected = next;
        changed
    }

    fn can_capture_lateral_move(&self) -> bool {
        // The current Search layout owns plain lateral moves in Grid mode even
        // when there are no results to select. In List mode the input owns them.
        self.layout.grid
    }

    fn selected_scroll_child(&self) -> Option<usize> {
        self.selected
            .filter(|index| *index < self.actions.len())
            .map(|index| self.layout.scroll_child_for(index))
    }

    fn enter_target(&self, current_query: &str) -> Option<(u64, usize)> {
        if self.query != current_query {
            return None;
        }
        let index = self
            .selected
            .filter(|index| *index < self.actions.len())
            .or_else(|| (self.actions.len() == 1).then_some(0))?;
        Some((self.generation, index))
    }

    fn resolve_action(
        &self,
        generation: u64,
        index: usize,
        current_query: &str,
    ) -> Option<multi_launcher::actions::Action> {
        self.resolve_result(generation, index, current_query)
            .map(|result| result.action)
    }

    fn resolve_result(
        &self,
        generation: u64,
        index: usize,
        current_query: &str,
    ) -> Option<LauncherSearchResult> {
        if generation != self.generation || self.query != current_query {
            return None;
        }
        self.actions.get(index).cloned()
    }
}

fn decorate_folder_descriptions(rows: &mut [LauncherSearchResult], grid: bool, show_full_path: bool, aliases: &HashMap<String, HashSet<String>>) {
    for row in rows {
        if matches!(&row.origin, LauncherResultOrigin::Provider(name) if name == "folders") {
            row.hide_description = !grid && !show_full_path && row.action.args.is_none()
                && row.action.desc == row.action.action
                && aliases.get(&row.action.action).is_some_and(|labels| labels.contains(&row.action.label));
        }
    }
}

fn select_search_result_state(
    results: &mut LauncherSearchResults,
    current_query: &str,
    generation: u64,
    index: usize,
) -> Option<(bool, Option<usize>)> {
    if generation != results.generation
        || results.query != current_query
        || index >= results.actions.len()
    {
        return None;
    }
    let changed = results.selected != Some(index);
    results.selected = Some(index);
    let reveal_row = changed.then(|| results.layout.scroll_child_for(index));
    Some((changed, reveal_row))
}

fn search_jump_search_target_id(generation: u64) -> TargetId {
    TargetId::new(
        SurfaceId::LauncherResults,
        SemanticTargetId::named("launcher-search"),
        TargetGeneration(generation),
    )
}

fn search_jump_result_target_id(generation: u64, index: usize) -> TargetId {
    TargetId::root_result(TargetGeneration(generation), index)
}

fn inner_results_consumed_scroll(previous_offset: f32, max_offset: f32, delta: f32) -> bool {
    (delta < 0.0 && previous_offset > -max_offset) || (delta > 0.0 && previous_offset < 0.0)
}

fn launcher_result_row_id(generation: u64, absolute_index: usize) -> String {
    format!("launcher-search-result-row-{generation}-{absolute_index}")
}

fn launcher_result_grid_row_id(generation: u64, first_absolute_index: usize) -> String {
    format!("launcher-search-grid-row-{generation}-{first_absolute_index}")
}

// GPUI snaps style lengths to device pixels with half ties toward zero.
fn launcher_snapped_length(value: f32, scale: f32) -> f32 {
    ((value.max(0.0) * scale - 0.5).ceil().max(0.0)) / scale
}

fn launcher_result_grid_geometry(
    width: f32,
    columns: usize,
    spacing: f32,
    scale: f32,
) -> (f32, f32) {
    let width_pixels = (width.max(0.0) * scale).floor();
    let columns = columns.max(1);
    let gaps = columns.saturating_sub(1) as f32;
    let gap_pixels = if gaps > 0.0 {
        (launcher_snapped_length(spacing, scale) * scale).min((width_pixels / gaps).floor())
    } else {
        0.0
    };
    let cell_pixels = ((width_pixels - gaps * gap_pixels) / columns as f32).floor();
    (gap_pixels / scale, cell_pixels / scale)
}

fn render_launcher_result_cell(
    action: &Action,
    display_label: &str,
    hide_description: bool,
    compact_description: bool,
    list: bool,
    full_folder_path: bool,
    horizontal_padding: f32,
    generation: u64,
    absolute_index: usize,
    selected: bool,
    hint: Option<&Hint>,
    jump_geometry_draft: Option<Rc<RefCell<JumpGeometryDraft>>>,
    jump_geometry_frame: Rc<RefCell<Option<JumpGeometryFrame>>>,
    palette: multi_launcher::gui::theme::SemanticPalette,
    metrics: multi_launcher::gui::theme::ThemeMetrics,
    cx: &mut Context<RootWindow>,
) -> AnyElement {
    let description = if list {
        launcher_result::list_description(&action.desc, full_folder_path)
    } else if compact_description {
        launcher_result::list_description(&action.desc, false)
    } else {
        action.desc.clone()
    };
    let target_id = search_jump_result_target_id(generation, absolute_index);
    let hint = jump_hint_badge(
        hint,
        &target_id,
        generation,
        jump_geometry_frame,
        palette,
        metrics,
    );
    let context_action = action.clone();
    let cell = div()
        .id(launcher_result_row_id(generation, absolute_index))
        .relative()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .w_full()
        .min_w(px(0.0))
        .overflow_hidden()
        .when(!list, |cell| {
            cell.gap_y(px(metrics.item_spacing_y))
                .py(px(metrics.panel_padding))
        })
        .when(list, |cell| cell.h(px(metrics.row_min_height)))
        .px(px(horizontal_padding))
        .rounded(px(metrics.corner_radius))
        .bg(appearance::to_hsla(if selected {
            palette.selection_fill
        } else {
            palette.panel_surface
        }))
        .text_color(appearance::to_hsla(palette.primary_text))
        .role(Role::Button)
        .aria_label(if hide_description || (list && description.is_empty()) {
            display_label.to_owned()
        } else {
            format!("{}: {}", display_label, description)
        })
        .tab_stop(false)
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |root, event: &MouseDownEvent, window, cx| {
                root.open_context_favorite(
                    generation,
                    absolute_index,
                    &context_action,
                    event.position,
                    window,
                    cx,
                );
                cx.stop_propagation();
            }),
        )
        .on_click(cx.listener(move |root, event: &ClickEvent, window, cx| {
            root.activate_launcher_result(
                generation,
                absolute_index,
                result_click_source(event.is_keyboard()),
                LauncherResultActivationOrigin::ResultWidget,
                window,
                cx,
            );
        }))
        .when(list, |cell| {
            cell.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .w_full()
                    .h_full()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .child(
                        Label::new(display_label.to_owned())
                            .min_w(px(0.0))
                            .flex_shrink(0.25)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .id("launcher-search-result-label")
                            .text_size(px(metrics.body_text_size))
                            .line_height(px(metrics.line_height)),
                    )
                    .children((!hide_description && !description.is_empty()).then(|| {
                        Label::new(format!(" : {description}"))
                            .min_w(px(0.0))
                            .flex_shrink_1()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .id("launcher-search-result-description")
                            .text_size(px(metrics.small_text_size))
                            .line_height(px(metrics.line_height))
                    })),
            )
        })
        .when(!list, |cell| {
            cell.child(
                Label::new(display_label.to_owned())
                    .w_full()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .id("launcher-search-result-label")
                    .text_size(px(metrics.body_text_size))
                    .line_height(px(metrics.line_height)),
            )
            .children((!hide_description).then(|| {
                Label::new(description.clone())
                    .w_full()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .id("launcher-search-result-description")
                    .text_size(px(metrics.small_text_size))
                    .line_height(px(metrics.line_height))
            }))
        })
        .children(hint);
    if let Some(draft) = jump_geometry_draft {
        let metadata = TargetMetadata::new(
            target_id,
            display_label.to_owned(),
            TargetKind::Action,
            absolute_index.saturating_add(1) as u64,
        );
        JumpGeometryElement::new(cell, metadata, draft).into_any_element()
    } else {
        cell.into_any_element()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FreshSearchEnter {
    generation: u64,
    query: String,
    jump_mode_revision: u64,
}

fn is_fresh_plain_search_enter(
    focused: bool,
    key: &str,
    modifiers: Modifiers,
    is_held: bool,
) -> bool {
    focused && key == "enter" && modifiers == Modifiers::default() && !is_held
}

fn fresh_search_enter_marker(
    focused: bool,
    key: &str,
    modifiers: Modifiers,
    is_held: bool,
    generation: u64,
    query: String,
    jump_mode_revision: u64,
) -> Option<FreshSearchEnter> {
    is_fresh_plain_search_enter(focused, key, modifiers, is_held)
        .then_some(FreshSearchEnter {
            generation,
            query,
            jump_mode_revision,
        })
}

fn result_click_source(is_keyboard: bool) -> ActivationSource {
    if is_keyboard {
        ActivationSource::Enter
    } else {
        ActivationSource::Click
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LauncherResultActivationOrigin {
    DeferredSearchEnter,
    ResultWidget,
}

enum HashWindowState {
    Closed,
    Docked {
        mount_generation: u64,
        view: Entity<HashGeneratorWindow>,
    },
    Popout {
        mount_generation: u64,
        window: AnyWindowHandle,
        view: Entity<HashGeneratorWindow>,
    },
}

enum BuiltHashMount {
    Docked {
        view: Entity<HashGeneratorWindow>,
    },
    Popout {
        window: AnyWindowHandle,
        view: Entity<HashGeneratorWindow>,
        native_window: Option<NativeWindow>,
        close_subscription: Subscription,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HashPresentationMode {
    Docked,
    Popout,
}

#[derive(Clone, Copy)]
enum DockedHashAction {
    Calculate,
    Dock,
    Popout,
    Close,
}

const HASH_DOCK_FACTORY_SIZE: PreferredWindowSize = PreferredWindowSize {
    width: 620.0,
    height: 440.0,
};
const HASH_DOCK_MIN_WIDTH: f32 = 420.0;
const HASH_DOCK_MIN_HEIGHT: f32 = 320.0;
const HASH_DOCK_MAX_DIMENSION: f32 = 8192.0;
const HASH_DOCK_RESIZE_HANDLE_SIZE: f32 = 26.0;
const HASH_DOCK_KEYBOARD_RESIZE_STEP: f32 = 16.0;

#[derive(Clone, Copy, Debug)]
enum DockedHashResizeOrigin {
    Pointer {
        position: Point<Pixels>,
        visible_size: AppliedWindowSize,
    },
    Keyboard,
}

#[derive(Clone, Copy, Debug)]
struct DockedHashResize {
    mount_generation: u64,
    session_epoch: u64,
    start_size: PreferredWindowSize,
    available_at_start: AppliedWindowSize,
    preview_size: PreferredWindowSize,
    changed_width: bool,
    changed_height: bool,
    drag_started: bool,
    origin: DockedHashResizeOrigin,
}

#[derive(Clone)]
struct DockedHashResizePayload {
    mount_generation: u64,
    session_epoch: u64,
    start_size: PreferredWindowSize,
    preview_extent: Arc<Mutex<Option<DockedHashResize>>>,
}

impl DockedHashResizePayload {
    fn mark_drag_started(&self) {
        let mut preview = self
            .preview_extent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(resize) = preview.as_mut().filter(|resize| {
            resize.mount_generation == self.mount_generation
                && resize.session_epoch == self.session_epoch
                && resize.start_size == self.start_size
                && matches!(resize.origin, DockedHashResizeOrigin::Pointer { .. })
        }) {
            resize.drag_started = true;
        }
    }
}

fn launcher_column_width(viewport_width: f32, window_padding: f32, scale: f32) -> f32 {
    launcher_search_width(viewport_width, window_padding, scale).min(520.0)
}

fn launcher_search_width(viewport_width: f32, window_padding: f32, scale: f32) -> f32 {
    let available = ((viewport_width * scale).floor() / scale
        - 2.0 * launcher_snapped_length(window_padding, scale))
        .max(1.0);
    launcher_snapped_length(available, scale)
}

fn root_scroll_viewport_size(bounds: Bounds<Pixels>) -> AppliedWindowSize {
    AppliedWindowSize {
        width: bounds.size.width.as_f32().max(1.0),
        height: bounds.size.height.as_f32().max(1.0),
    }
}

fn docked_hash_fit(
    preferred: PreferredWindowSize,
    available: AppliedWindowSize,
) -> AppliedWindowSize {
    preferred.apply_within(available.width, available.height)
}

fn docked_hash_resize_accessibility_label(
    preferred: PreferredWindowSize,
    current_visible: AppliedWindowSize,
    preview: PreferredWindowSize,
    preview_visible: AppliedWindowSize,
    preview_active: bool,
) -> String {
    let current_preferred = format!("{:.0} by {:.0}", preferred.width, preferred.height);
    let current_visible = format!(
        "{:.0} by {:.0}",
        current_visible.width, current_visible.height
    );
    if preview_active {
        format!(
            "Resize docked Hash Generator. Current preferred size: {current_preferred} logical points, displayed {current_visible}. Preview size: {:.0} by {:.0} logical points, displayed {:.0} by {:.0}. Use arrow keys to preview, Enter to save, Escape to discard.",
            preview.width, preview.height, preview_visible.width, preview_visible.height,
        )
    } else {
        format!(
            "Resize docked Hash Generator. Current preferred size: {current_preferred} logical points, displayed {current_visible}. Use arrow keys to preview, Enter to save, Escape to discard."
        )
    }
}

fn docked_hash_resize_identity_matches(
    active_mount_generation: u64,
    mount_generation: u64,
    active_session_epoch: u64,
    session_epoch: u64,
    docked_mount_is_current: bool,
) -> bool {
    mount_generation != 0
        && active_mount_generation == mount_generation
        && active_session_epoch == session_epoch
        && docked_mount_is_current
}

fn pointer_docked_resize_preview(
    start: PreferredWindowSize,
    visible_at_start: AppliedWindowSize,
    delta_x: f32,
    delta_y: f32,
) -> (PreferredWindowSize, bool, bool) {
    let changed_width = delta_x.abs() > 1.0;
    let changed_height = delta_y.abs() > 1.0;
    let width = if changed_width {
        (visible_at_start.width + delta_x).clamp(HASH_DOCK_MIN_WIDTH, HASH_DOCK_MAX_DIMENSION)
    } else {
        start.width
    };
    let height = if changed_height {
        (visible_at_start.height + delta_y).clamp(HASH_DOCK_MIN_HEIGHT, HASH_DOCK_MAX_DIMENSION)
    } else {
        start.height
    };
    (
        PreferredWindowSize::new(width, height).unwrap_or(start),
        changed_width,
        changed_height,
    )
}

fn keyboard_docked_resize_preview(
    current: PreferredWindowSize,
    delta_x: f32,
    delta_y: f32,
) -> PreferredWindowSize {
    PreferredWindowSize::new(
        if delta_x == 0.0 {
            current.width
        } else {
            (current.width + delta_x).clamp(HASH_DOCK_MIN_WIDTH, HASH_DOCK_MAX_DIMENSION)
        },
        if delta_y == 0.0 {
            current.height
        } else {
            (current.height + delta_y).clamp(HASH_DOCK_MIN_HEIGHT, HASH_DOCK_MAX_DIMENSION)
        },
    )
    .unwrap_or(current)
}

fn docked_resize_commit_size(resize: DockedHashResize) -> Option<PreferredWindowSize> {
    if matches!(resize.origin, DockedHashResizeOrigin::Pointer { .. }) && !resize.drag_started {
        return None;
    }
    let applied_preview = docked_hash_fit(resize.preview_size, resize.available_at_start);
    let visible_at_start = match resize.origin {
        DockedHashResizeOrigin::Pointer { visible_size, .. } => visible_size,
        DockedHashResizeOrigin::Keyboard => {
            docked_hash_fit(resize.start_size, resize.available_at_start)
        }
    };
    let width = if resize.changed_width
        && (applied_preview.width - visible_at_start.width).abs() > 1.0
        && (resize.preview_size.width - resize.start_size.width).abs() > 1.0
    {
        resize.preview_size.width
    } else {
        resize.start_size.width
    };
    let height = if resize.changed_height
        && (applied_preview.height - visible_at_start.height).abs() > 1.0
        && (resize.preview_size.height - resize.start_size.height).abs() > 1.0
    {
        resize.preview_size.height
    } else {
        resize.start_size.height
    };
    if (width - resize.start_size.width).abs() <= 1.0
        && (height - resize.start_size.height).abs() <= 1.0
    {
        return None;
    }
    PreferredWindowSize::new(width, height).ok()
}

impl HashPresentationMode {
    fn from_saved(preference: Option<RememberedPresentation>) -> Self {
        match preference {
            Some(RememberedPresentation::Popout) => Self::Popout,
            Some(RememberedPresentation::Docked) | None => Self::Docked,
        }
    }

    fn remembered(self) -> RememberedPresentation {
        match self {
            Self::Docked => RememberedPresentation::Docked,
            Self::Popout => RememberedPresentation::Popout,
        }
    }
}

fn hash_mount_identity_matches(
    active_mount_generation: u64,
    mount_generation: u64,
    active_session_epoch: u64,
    session_epoch: u64,
) -> bool {
    mount_generation != 0
        && active_mount_generation == mount_generation
        && active_session_epoch == session_epoch
}

fn docked_hash_focus_is_current(
    requested_mount_generation: Option<u64>,
    active_mount_generation: u64,
    docked_mount_generation: Option<u64>,
) -> bool {
    requested_mount_generation.is_some_and(|generation| {
        generation != 0
            && generation == active_mount_generation
            && docked_mount_generation == Some(generation)
    })
}

fn root_search_focus_is_current(
    current_mode: Option<HashPresentationMode>,
    current_mount_generation: Option<u64>,
    active_mount_generation: u64,
    requested_mode: Option<HashPresentationMode>,
    requested_mount_generation: u64,
    restart_fenced: bool,
) -> bool {
    !restart_fenced
        && current_mode == requested_mode
        && current_mount_generation
            == (requested_mount_generation != 0).then_some(requested_mount_generation)
        && active_mount_generation == requested_mount_generation
}

fn search_input_is_current(
    root_visible: bool,
    lifecycle_active: bool,
    root_window_matches: bool,
    search_focused: bool,
    window_active: bool,
) -> bool {
    launcher_root_is_current(
        root_visible,
        lifecycle_active,
        root_window_matches,
        window_active,
    ) && search_focused
}

fn launcher_root_is_current(
    root_visible: bool,
    lifecycle_active: bool,
    root_window_matches: bool,
    window_active: bool,
) -> bool {
    root_visible && lifecycle_active && root_window_matches && window_active
}

fn launcher_result_activation_is_current(
    root_visible: bool,
    lifecycle_active: bool,
    root_window_matches: bool,
    search_focused: bool,
    window_active: bool,
    origin: LauncherResultActivationOrigin,
) -> bool {
    launcher_root_is_current(
        root_visible,
        lifecycle_active,
        root_window_matches,
        window_active,
    ) && (origin != LauncherResultActivationOrigin::DeferredSearchEnter || search_focused)
}

fn search_completion_session_is_current(session_revision: Option<u64>, revision: u64) -> bool {
    session_revision == Some(revision)
}

fn context_stack_has_primary(context_stack: &[KeyContext], name: &str) -> bool {
    context_stack.iter().any(|context| {
        context
            .primary()
            .is_some_and(|entry| entry.key.as_ref() == name && entry.value.is_none())
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PhysicalKeydown {
    hwnd: usize,
    virtual_key: u16,
    scan_code: u8,
    extended: bool,
    modifiers: Modifiers,
    held: bool,
}

#[cfg(windows)]
impl From<NativeKeydownReceipt> for PhysicalKeydown {
    fn from(receipt: NativeKeydownReceipt) -> Self {
        Self {
            hwnd: receipt.hwnd,
            virtual_key: receipt.virtual_key,
            scan_code: receipt.scan_code,
            extended: receipt.extended,
            modifiers: receipt.modifiers,
            held: receipt.held,
        }
    }
}

#[cfg(any(windows, test))]
fn physical_keypad_selection_move(
    receipt: Option<PhysicalKeydown>,
    event_modifiers: Modifiers,
    context_stack: &[KeyContext],
    root_identity: Option<ForegroundIdentity>,
    foreground_identity: Option<ForegroundIdentity>,
    event_window_is_root: bool,
) -> Option<QuerySelectionMove> {
    let receipt = receipt?;
    let movement = physical_keypad_receipt_direction(receipt)?;
    let root_identity = root_identity?;
    if !event_window_is_root
        || foreground_identity != Some(root_identity)
        || receipt.hwnd != root_identity.hwnd
        || event_modifiers != Modifiers::default()
        || !context_stack_has_primary(context_stack, "LauncherSearch")
        || context_stack_has_primary(context_stack, "HashGeneratorWindow")
    {
        return None;
    }

    Some(movement)
}

#[cfg(any(windows, test))]
fn physical_keypad_receipt_direction(receipt: PhysicalKeydown) -> Option<QuerySelectionMove> {
    if receipt.extended || receipt.modifiers.modified() {
        return None;
    }

    match (receipt.scan_code, receipt.virtual_key) {
        // Some Win32 input drivers omit scan metadata; VK_NUMPAD* still
        // identifies the dedicated keypad key without matching arrows/digits.
        (0x50, 0x62 | 0x28) | (0, 0x62) => Some(QuerySelectionMove::Down),
        (0x4b, 0x64 | 0x25) | (0, 0x64) => Some(QuerySelectionMove::Left),
        (0x4d, 0x66 | 0x27) | (0, 0x66) => Some(QuerySelectionMove::Right),
        (0x48, 0x68 | 0x26) | (0, 0x68) => Some(QuerySelectionMove::Up),
        _ => None,
    }
}

#[cfg(windows)]
fn intercept_root_pane_escape(
    root: &WeakEntity<RootWindow>,
    receipt: PhysicalKeydown,
    event: &gpui_kit::KeystrokeEvent,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let plugin_editor = context_stack_has_primary(&event.context_stack, "PluginEditorPane");
    let folder_size = context_stack_has_primary(&event.context_stack, "FolderSizePane");
    let file_lock = context_stack_has_primary(&event.context_stack, "FileLockPane");
    if event.keystroke.key != "escape"
        || receipt.virtual_key != 0x1b
        || receipt.modifiers.modified()
        || (!plugin_editor && !folder_size && !file_lock && !context_stack_has_primary(&event.context_stack, "FileSearchPane"))
    {
        return false;
    }
    let Some(root) = root.upgrade() else { return false; };
    let state = root.read(cx);
    if file_lock && state.file_lock_popup.as_ref().is_some_and(|popup| {
        popup.window.window_id() == window.window_handle().window_id()
            && popup.native.is_some_and(|native| native.identity.hwnd == receipt.hwnd
                && native_window_is_valid(native) && current_foreground_identity() == Some(native.identity))
    }) {
        let view = state.file_lock_view.clone();
        return view.is_some_and(|view| view.update(cx, |view, cx| {
            if !view.is_mounted_in(window.window_handle().window_id(), true) { return false; }
            view.handle_escape(window, cx, receipt.held)
        }));
    }
    if folder_size && state.folder_size_popup.as_ref().is_some_and(|popup| {
        popup.window.window_id() == window.window_handle().window_id()
            && popup.native.is_some_and(|native| native.identity.hwnd == receipt.hwnd
                && native_window_is_valid(native) && current_foreground_identity() == Some(native.identity))
    }) {
        let view = state.folder_size_view.clone();
        return view.is_some_and(|view| view.update(cx, |view, cx| {
            if !view.is_mounted_in(window.window_handle().window_id(), true) { return false; }
            view.handle_escape(window, cx, receipt.held)
        }));
    }
    if !state.root_visibility.active.load(Ordering::Acquire)
        || !state.root_visibility.visible.load(Ordering::Acquire)
        || !state.root_window.is_some_and(|handle| handle.window_id() == window.window_handle().window_id())
        || !state.native_window.is_some_and(|native| receipt.hwnd == native.identity.hwnd
            && native_window_is_valid(native) && current_foreground_identity() == Some(native.identity)) { return false; }
    if plugin_editor {
        let view = state.plugin_editor.clone();
        return view.is_some_and(|view| view.update(cx, |view, cx| view.handle_escape(window, cx, Some(receipt.held))));
    }
    if file_lock {
        let view = state.file_lock_view.clone();
        let handled = view.is_some_and(|view| view.update(cx, |view, cx| view.handle_escape(window, cx, receipt.held)));
        if handled && !receipt.held { root.update(cx, |root, _| root.pane_escape_latched = true); }
        return handled;
    }
    if folder_size {
        let view = state.folder_size_view.clone();
        let handled = view.is_some_and(|view| view.update(cx, |view, cx| view.handle_escape(window, cx, receipt.held)));
        if handled && !receipt.held { root.update(cx, |root, _| root.pane_escape_latched = true); }
        return handled;
    }
    let view = state.file_search.clone();
    let detached = view.as_ref().is_some_and(|view| view.read(cx)
        .detached_inline_preview(window.window_handle().window_id(), cx).is_some_and(|child| child.read(cx).owns_focus(window, cx)));
    let handled = view.is_some_and(|view| view.update(cx, |view, cx| view.handle_escape(window, cx, Some(receipt.held))));
    if detached && handled && !receipt.held { root.update(cx, |root, _| root.pane_escape_latched = true); }
    handled
}

fn hash_menu_focus_is_owned(menu_open: bool, docked: bool, owns_focus: bool) -> bool {
    menu_open && docked && owns_focus
}

fn root_tab_traversed_to_search(
    key: &str,
    modifiers: Modifiers,
    action_present: bool,
    context_stack: &[KeyContext],
    search_focused_after_dispatch: bool,
    popup_menu_open: bool,
) -> bool {
    key == "tab"
        && !popup_menu_open
        && modifiers == Modifiers::default()
        && action_present
        && search_focused_after_dispatch
        && context_stack_has_primary(context_stack, "RootWindow")
        && !context_stack_has_primary(context_stack, "LauncherSearch")
        && !context_stack_has_primary(context_stack, "HashGeneratorWindow")
        && !context_stack_has_primary(context_stack, "SettingsEditor")
        && !context_stack_has_primary(context_stack, "CommandHelp")
        && !context_stack_has_primary(context_stack, "ToastLogViewer")
        && !context_stack_has_primary(context_stack, "FileSearchPane")
        && !context_stack_has_primary(context_stack, "FolderSizePane")
        && !context_stack_has_primary(context_stack, "FileLockPane")
        && !context_stack_has_primary(context_stack, "DownloadsTriagePane")
        && !context_stack_has_primary(context_stack, "PluginEditorPane")
        && !context_stack_has_primary(context_stack, "PopupMenu")
}

fn command_help_escape_capture_should_close(
    context_stack: &[KeyContext],
    help_panel_owns_focus: bool,
) -> bool {
    help_panel_owns_focus
        && !context_stack_has_primary(context_stack, "Input")
        && !context_stack_has_primary(context_stack, "PopupMenu")
}

fn hash_escape_capture_should_close(
    context_stack: &[KeyContext],
    hash_owns_focus: bool,
    file_picker_pending: bool,
) -> bool {
    hash_owns_focus
        && !file_picker_pending
        && !context_stack_has_primary(context_stack, "List")
        && !context_stack_has_primary(context_stack, "PopupMenu")
        && !context_stack_has_primary(context_stack, "AlertDialog")
        && !context_stack_has_primary(context_stack, "Dialog")
}

fn quick_help_escape_capture_should_close(
    context_stack: &[KeyContext],
    quick_help_owns_focus: bool,
) -> bool {
    quick_help_owns_focus
        && !context_stack_has_primary(context_stack, "PopupMenu")
        && !context_stack_has_primary(context_stack, "AlertDialog")
        && !context_stack_has_primary(context_stack, "Dialog")
}

fn quick_help_event_targets_visible_revision(
    active: bool,
    visible: bool,
    native_window_visible: bool,
    observed_revision: u64,
    current_revision: u64,
) -> bool {
    active && visible && native_window_visible && observed_revision == current_revision
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct QuickHelpFocusBlockers {
    menu_open: bool,
    popup_or_alert_owns_focus: bool,
    settings_owner_has_focus: bool,
    toast_log_owner_has_focus: bool,
    command_help_owner_has_focus: bool,
    docked_hash_owner_has_focus: bool,
    hash_resize_owner_has_focus: bool,
}

impl QuickHelpFocusBlockers {
    fn blocks_handoff(self) -> bool {
        self.menu_open
            || self.popup_or_alert_owns_focus
            || self.settings_owner_has_focus
            || self.toast_log_owner_has_focus
            || self.command_help_owner_has_focus
            || self.docked_hash_owner_has_focus
            || self.hash_resize_owner_has_focus
    }
}

fn quick_help_focus_change_is_allowed(
    root_is_foreground: bool,
    window_is_active: bool,
    focus_blockers: QuickHelpFocusBlockers,
    restart_is_fenced: bool,
) -> bool {
    root_is_foreground
        && window_is_active
        && !focus_blockers.blocks_handoff()
        && !restart_is_fenced
}

fn root_menu_close_should_restore_focus(
    another_menu_open: bool,
    current_focus_is_live_in_root: bool,
    accepted_panel_route: bool,
    window_is_active: bool,
) -> bool {
    !another_menu_open
        && !current_focus_is_live_in_root
        && !accepted_panel_route
        && window_is_active
}

fn toast_log_viewport_body_height(viewport_height: f32, reserved_height: f32) -> f32 {
    (viewport_height - reserved_height).clamp(0.0, 240.0)
}

fn command_help_viewport_body_height(viewport_height: f32, reserved_height: f32) -> f32 {
    (viewport_height - reserved_height).clamp(0.0, 280.0)
}

fn quick_help_viewport_body_height(viewport_height: f32, reserved_height: f32) -> f32 {
    (viewport_height - reserved_height).clamp(0.0, 280.0)
}

fn settings_return_focus_is_still_owned(
    settings_owner_has_focus: bool,
    restart_is_fenced: bool,
) -> bool {
    settings_owner_has_focus && !restart_is_fenced
}

const ROOT_SCROLL_SEARCH_ITEM: usize = 0;
const ROOT_SCROLL_SETTINGS_ITEM: usize = 1;

fn docked_hash_root_scroll_item_index(
    settings_open: bool,
    toast_log_open: bool,
    command_help_open: bool,
) -> usize {
    ROOT_SCROLL_SETTINGS_ITEM
        + usize::from(settings_open)
        + usize::from(toast_log_open)
        + usize::from(command_help_open)
}

fn quick_help_root_scroll_item_index(
    settings_open: bool,
    toast_log_open: bool,
    command_help_open: bool,
    docked_hash_open: bool,
) -> usize {
    docked_hash_root_scroll_item_index(settings_open, toast_log_open, command_help_open)
        + usize::from(docked_hash_open)
}

const LINKING_GUIDE_FILTER: &str = "todo note cal @note: @todo:";
const SETTINGS_DIRTY_RESTART_STATUS: &str =
    "Save or Cancel the dirty Settings preference before restarting in egui.";
const PREFERENCE_COMMIT_CLOSE_STATUS: &str =
    "A Settings preference is committing; Multi Launcher will close after it finishes.";
const TOAST_LOG_OPEN_PENDING_STATUS: &str =
    "Wait for the Toast Log open request to finish before closing Multi Launcher.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RootCloseBlocker {
    HashClipboard(&'static str),
    HashFilePicker,
    XmlFilePicker,
    ToastLogOpen,
    FavoritesWrite,
    HashPinWrite,
    HashRehost,
    StructuredPinWrite,
    RegexRehost,
    RegexHistory,
    RegexPresets,
    RegexClipboard,
    ClipboardHistory,
    ClipboardModifyBlocked,
    SnippetsSave,
    TempfileSave,
    FileSearchEffect,
    FolderSize,
    FileLock,
    AppWrite,
    ExternalLaunch,
    BooleanPreferenceSave,
    SettingsActionsSave,
    PluginPreferenceSave,
    CalculatorSearch,
}

fn root_close_blocker(
    clipboard_message: Option<&'static str>,
    file_picker_pending: bool,
    toast_log_open_pending: bool,
    settings_save_pending: bool,
    command_help_save_pending: bool,
) -> Option<RootCloseBlocker> {
    if let Some(message) = clipboard_message {
        Some(RootCloseBlocker::HashClipboard(message))
    } else if file_picker_pending {
        Some(RootCloseBlocker::HashFilePicker)
    } else if toast_log_open_pending {
        Some(RootCloseBlocker::ToastLogOpen)
    } else if settings_save_pending || command_help_save_pending {
        Some(RootCloseBlocker::BooleanPreferenceSave)
    } else {
        None
    }
}

fn clear_toast_log_open_pending_status(status: &mut Option<String>) -> bool {
    if status.as_deref() == Some(TOAST_LOG_OPEN_PENDING_STATUS) {
        *status = None;
        true
    } else {
        false
    }
}

fn clear_stale_settings_restart_status(
    status: &mut Option<String>,
    settings_clean_or_closed: bool,
) -> bool {
    if settings_clean_or_closed
        && status.as_deref() == Some(SETTINGS_DIRTY_RESTART_STATUS)
    {
        *status = None;
        true
    } else {
        false
    }
}

fn settings_menu_close_needs_trigger_focus(
    settings_scope_has_focus: bool,
    menu_was_open: bool,
    menu_origin_was_settings: bool,
) -> bool {
    settings_scope_has_focus || (menu_was_open && menu_origin_was_settings)
}

impl HashWindowState {
    fn mode(&self) -> Option<HashPresentationMode> {
        match self {
            Self::Closed => None,
            Self::Docked { .. } => Some(HashPresentationMode::Docked),
            Self::Popout { .. } => Some(HashPresentationMode::Popout),
        }
    }

    fn mount_generation(&self) -> Option<u64> {
        match self {
            Self::Closed => None,
            Self::Docked {
                mount_generation, ..
            }
            | Self::Popout {
                mount_generation, ..
            } => Some(*mount_generation),
        }
    }

    fn view(&self) -> Option<&Entity<HashGeneratorWindow>> {
        match self {
            Self::Closed => None,
            Self::Docked { view, .. } | Self::Popout { view, .. } => Some(view),
        }
    }
}

#[derive(Clone, Copy)]
struct RetiringHashWindow {
    window: AnyWindowHandle,
    native_window: Option<NativeWindow>,
}

/// Typed Hash open target shared by explicit launcher and file-producing tools.
#[derive(Clone, Debug, PartialEq, Eq)]
enum HashOpenIntent {
    Default,
    Reactivate,
    File(PathBuf),
}

#[derive(Clone)]
enum HashReturnReceipt {
    FileSearch(file_search::SelectionReceipt),
    FileSearchPopup { receipt: file_search::SelectionReceipt, mount: utility_mount::MountReceipt, host: AnyWindowHandle, native: NativeWindow },
    #[cfg(windows)]
    Downloads(downloads_triage::InspectReceipt),
}

fn hash_popup_visibility_eligible(visible: bool, minimized: bool) -> bool { visible && !minimized }

fn checked_hash_popup_state(native: NativeWindow) -> Option<(bool, bool)> {
    #[cfg(windows)]
    {
        use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::IsIconic};
        if !native_window_is_valid(native) { return None; }
        let visible = native_window_is_visible(native).ok()?;
        let minimized = unsafe { IsIconic(HWND(native.identity.hwnd as *mut _)) }.as_bool();
        native_window_is_valid(native).then_some((visible, minimized))
    }
    #[cfg(not(windows))]
    { let _ = native; None }
}

fn checked_hash_popup_visible(native: NativeWindow) -> bool {
    checked_hash_popup_state(native).is_some_and(|(visible, minimized)| hash_popup_visibility_eligible(visible, minimized))
}

fn prepare_direct_hash_popup(fresh: bool, state: Option<(bool, bool)>, mut current: impl FnMut() -> bool,
    show: impl FnOnce() -> bool, displayed: impl FnOnce() -> bool) -> bool {
    if !fresh && state.is_none_or(|(visible, _)| !visible) { return false; }
    if !current() { return false; }
    if (fresh || state.is_some_and(|(_, minimized)| minimized)) && !show() { return false; }
    displayed() && current()
}
fn restore_direct_hash_popup(fresh: bool, state: Option<(bool, bool)>, show: impl FnOnce() -> bool) -> bool {
    if fresh { return show(); }
    match state {
        Some((true, false)) => true,
        Some((true, true)) => show(),
        _ => false,
    }
}

fn direct_inherited_hash_commit_current(accepted: bool, slot: bool, revision: bool, target: bool, native: bool, epoch: bool, origin: bool) -> bool {
    accepted && slot && revision && target && native && epoch && origin
}

fn inherited_hash_parent_return_allowed(remount: bool, unpin: bool) -> bool { !remount && !unpin }

fn commit_inherited_hash_file(session: &mut HashSession, path: &std::path::Path, current: bool) -> bool {
    if !current { return false; }
    session.set_file_path(Some(path.to_path_buf()));
    true
}

#[derive(Clone)]
struct HashInheritedOrigin {
    receipt: file_search::SelectionReceipt,
    mount: utility_mount::MountReceipt,
    host: AnyWindowHandle,
    native: NativeWindow,
    focus: FocusHandle,
}

struct HashMountConfig {
    jump_controller: JumpModeController,
    session: Arc<Mutex<HashSession>>,
    wake_signal: Arc<HashWakeSignal>,
    file_picker_pending: Arc<AtomicBool>,
    clipboard_pending: Arc<HashClipboardPendingState>,
    pinned: Arc<AtomicBool>,
    restart_admission: Arc<RestartAdmission>,
    mount_generation: u64,
    session_epoch: u64,
    active_mount_generation: Arc<AtomicU64>,
    rehost_pending: Arc<AtomicBool>,
    rehost_revision: Arc<AtomicU64>,
    presentation_feedback: Arc<Mutex<Option<String>>>,
    native_geometry_sequence: Arc<AtomicU64>,
    hash_tool_key: UtilityToolKey,
    size_write_error: Option<String>,
    root: WeakEntity<RootWindow>,
    mode: HashPresentationMode,
}

#[derive(Clone, Copy, Debug)]
struct HashPickerRequest {
    mount_generation: u64,
    session_epoch: u64,
    source_generation: u64,
}

struct HashPickerTarget {
    mode: HashPresentationMode,
    window: AnyWindowHandle,
    view: Entity<HashGeneratorWindow>,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HashClipboardPendingKind {
    Idle = 0,
    Load = 1,
    Copy = 2,
}

#[derive(Default)]
struct HashClipboardPendingState(AtomicU8);

impl HashClipboardPendingState {
    fn begin(&self, kind: HashClipboardPendingKind) -> bool {
        debug_assert_ne!(kind, HashClipboardPendingKind::Idle);
        self.0
            .compare_exchange(
                HashClipboardPendingKind::Idle as u8,
                kind as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    fn finish(&self) {
        self.0
            .store(HashClipboardPendingKind::Idle as u8, Ordering::Release);
    }

    fn kind(&self) -> HashClipboardPendingKind {
        match self.0.load(Ordering::Acquire) {
            1 => HashClipboardPendingKind::Load,
            2 => HashClipboardPendingKind::Copy,
            _ => HashClipboardPendingKind::Idle,
        }
    }

    fn is_pending(&self) -> bool {
        self.kind() != HashClipboardPendingKind::Idle
    }

    fn is_copying(&self) -> bool {
        self.kind() == HashClipboardPendingKind::Copy
    }
}

fn hash_clipboard_pending_feedback(kind: HashClipboardPendingKind) -> Option<&'static str> {
    match kind {
        HashClipboardPendingKind::Idle => None,
        HashClipboardPendingKind::Load => Some(
            "Loading Clipboard… Wait for it to finish before closing. A source change discards this load.",
        ),
        HashClipboardPendingKind::Copy => Some(
            "Copying digest… Editing, Calculate, rehosting, and closing are paused until the clipboard write finishes.",
        ),
    }
}

fn hash_clipboard_close_message(kind: HashClipboardPendingKind) -> Option<&'static str> {
    match kind {
        HashClipboardPendingKind::Idle => None,
        HashClipboardPendingKind::Load => {
            Some("Wait for the Clipboard load to finish before closing Multi Launcher.")
        }
        HashClipboardPendingKind::Copy => {
            Some("Wait for the digest clipboard write to finish before closing Multi Launcher.")
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HashClipboardRequest {
    mount_generation: u64,
    session_epoch: u64,
    source_generation: u64,
}

#[derive(Clone)]
enum HashClipboardOperationKind {
    Load,
    Copy(HashClipboardCopy),
}

struct HashClipboardOperation {
    id: u64,
    request: HashClipboardRequest,
    kind: HashClipboardOperationKind,
}

enum HashClipboardOutcome {
    Load(Result<String, ClipboardError>),
    Copy(Result<(), ClipboardError>),
}

struct HashClipboardTarget {
    window: AnyWindowHandle,
    mount_generation: u64,
    session_epoch: u64,
    view: Entity<HashGeneratorWindow>,
}

fn hash_clipboard_copy_admissible(
    pending: HashClipboardPendingKind,
    file_picker_pending: bool,
) -> bool {
    pending == HashClipboardPendingKind::Idle && !file_picker_pending
}

fn hash_file_picker_admissible(
    pending: HashClipboardPendingKind,
    file_picker_pending: bool,
) -> bool {
    !file_picker_pending && pending != HashClipboardPendingKind::Copy
}

enum HashPickerOutcome {
    Selected(PathBuf),
    Cancelled,
    Failed(String),
}

fn classify_hash_picker_result<E>(
    result: Result<anyhow::Result<Option<Vec<PathBuf>>>, E>,
) -> HashPickerOutcome {
    match result {
        Ok(Ok(Some(mut paths))) => paths.drain(..).next().map_or_else(
            || HashPickerOutcome::Failed("The file picker returned no file".to_owned()),
            HashPickerOutcome::Selected,
        ),
        Ok(Ok(None)) => HashPickerOutcome::Cancelled,
        Ok(Err(error)) => HashPickerOutcome::Failed(error.to_string()),
        Err(_) => HashPickerOutcome::Failed(
            "The file picker did not return a selection result".to_owned(),
        ),
    }
}

struct HashPickerPendingGuard {
    pending: Arc<AtomicBool>,
    cleared: bool,
}

impl HashPickerPendingGuard {
    fn new(pending: Arc<AtomicBool>) -> Self {
        Self {
            pending,
            cleared: false,
        }
    }

    fn clear_and_notify(&mut self, root: &WeakEntity<RootWindow>, cx: &mut AsyncApp) {
        self.pending.store(false, Ordering::Release);
        self.cleared = true;
        let _ = root.update(cx, |root, cx| {
            if let Some(view) = root.hash_window.view().cloned() {
                view.update(cx, |_, cx| cx.notify());
            }
            cx.notify();
        });
    }
}

impl Drop for HashPickerPendingGuard {
    fn drop(&mut self) {
        if !self.cleared {
            self.pending.store(false, Ordering::Release);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HashDraft {
    text: String,
    expected: String,
    mode: HashInputMode,
    algorithm: HashAlgorithm,
    file_path: Option<PathBuf>,
    clipboard_snapshot_revision: Option<u64>,
}

impl HashDraft {
    fn from_session(session: &HashSession) -> Self {
        Self {
            text: session.text().to_owned(),
            expected: session.expected().to_owned(),
            mode: session.mode(),
            algorithm: session.algorithm(),
            file_path: session.file_path().map(PathBuf::from),
            clipboard_snapshot_revision: session.clipboard_snapshot_revision(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HashSourceChoice {
    Text,
    Clipboard,
    File,
}

impl HashSourceChoice {
    const fn mode(self) -> HashInputMode {
        match self {
            Self::Text => HashInputMode::Text,
            Self::Clipboard => HashInputMode::Clipboard,
            Self::File => HashInputMode::File,
        }
    }

    const fn from_mode(mode: HashInputMode) -> Option<Self> {
        match mode {
            HashInputMode::Text => Some(Self::Text),
            HashInputMode::Clipboard => Some(Self::Clipboard),
            HashInputMode::File => Some(Self::File),
        }
    }
}

impl SearchableListItem for HashSourceChoice {
    type Value = Self;

    fn title(&self) -> SharedString {
        match self {
            Self::Text => "Text",
            Self::Clipboard => "Clipboard",
            Self::File => "File",
        }
        .into()
    }

    fn render(&self, _: &mut Window, _: &mut App) -> impl IntoElement {
        Label::new(self.title())
    }

    fn value(&self) -> &Self::Value {
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HashAlgorithmChoice(HashAlgorithm);

impl SearchableListItem for HashAlgorithmChoice {
    type Value = Self;

    fn title(&self) -> SharedString {
        self.0.name().into()
    }

    fn render(&self, _: &mut Window, _: &mut App) -> impl IntoElement {
        Label::new(self.title())
    }

    fn value(&self) -> &Self::Value {
        self
    }
}

fn hash_picker_result_is_current(
    active_mount_generation: u64,
    mount_generation: u64,
    active_session_epoch: u64,
    session_epoch: u64,
    active_source_generation: u64,
    source_generation: u64,
) -> bool {
    hash_mount_identity_matches(
        active_mount_generation,
        mount_generation,
        active_session_epoch,
        session_epoch,
    ) && active_source_generation == source_generation
}

fn hash_clipboard_request_is_current(
    active_mount_generation: u64,
    active_session_epoch: u64,
    active_source_generation: u64,
    request: HashClipboardRequest,
    rehost_pending: bool,
) -> bool {
    !rehost_pending
        && hash_picker_result_is_current(
            active_mount_generation,
            request.mount_generation,
            active_session_epoch,
            request.session_epoch,
            active_source_generation,
            request.source_generation,
        )
}

fn create_hash_generator_view(
    window: &mut Window,
    cx: &mut App,
    initial_draft: HashDraft,
    native_window: Option<NativeWindow>,
    config: HashMountConfig,
) -> Entity<HashGeneratorWindow> {
    let HashMountConfig {
        jump_controller,
        session,
        wake_signal,
        file_picker_pending,
        clipboard_pending,
        pinned,
        restart_admission,
        mount_generation,
        session_epoch,
        active_mount_generation,
        rehost_pending,
        rehost_revision,
        presentation_feedback,
        native_geometry_sequence,
        hash_tool_key,
        size_write_error,
        root,
        mode,
    } = config;
    if mode == HashPresentationMode::Popout {
        let pending = Arc::clone(&clipboard_pending);
        let pinned_for_close = Arc::clone(&pinned);
        let root_for_close = root.clone();
        window.on_window_should_close(cx, move |window, app| {
            if !hash_pin::allows_close(pinned_for_close.load(Ordering::Acquire), hash_pin::CloseOrigin::Native) { return false; }
            if let Some(message) = hash_clipboard_close_message(pending.kind()) {
                let _ = root_for_close.update(app, |root, cx| { root.restart_status = Some(message.to_owned()); cx.notify(); });
                return false;
            }
            let view = root_for_close.update(app, |root, _| match (&root.hash_window, &root.hash_return) {
                (HashWindowState::Popout { window: host, view, mount_generation: active }, Some(HashReturnReceipt::FileSearchPopup { .. }))
                    if host.window_id() == window.window_handle().window_id() && *active == mount_generation => Some(view.clone()),
                _ => None,
            }).ok().flatten();
            if let Some(view) = view {
                window.defer(app, move |_, app| { view.update(app, |tool, cx| tool.request_close(hash_pin::CloseOrigin::Native, cx)); });
                false
            } else { true }
        });
    }
    let text = cx.new(|cx| {
        TextareaState::new(window, cx)
            .default_value(initial_draft.text.clone())
            .auto_grow(4, 10)
            .placeholder("Enter text to hash")
    });
    let expected = cx.new(|cx| {
        TextareaState::new(window, cx)
            .default_value(initial_draft.expected.clone())
            .auto_grow(1, 2)
            .placeholder("Optional expected digest")
    });

    let source_popup = hash_jump_select::PopupCollector::new(
        hash_jump_select::HashPopupKind::Source,
        mount_generation,
        jump_controller.enabled,
    );
    let algorithm_popup = hash_jump_select::PopupCollector::new(
        hash_jump_select::HashPopupKind::Algorithm,
        mount_generation,
        jump_controller.enabled,
    );
    let source_index = HashSourceChoice::from_mode(initial_draft.mode).map(|source| match source {
        HashSourceChoice::Text => 0,
        HashSourceChoice::Clipboard => 1,
        HashSourceChoice::File => 2,
    });
    let source = cx.new(|cx| {
        SelectState::new(
            vec![
                HashSourceChoice::Text,
                HashSourceChoice::Clipboard,
                HashSourceChoice::File,
            ].into_iter().enumerate().map(|(index, value)| hash_jump_select::HashSelectItem {
                value, index, collector: Rc::clone(&source_popup),
            }).collect::<Vec<_>>(),
            source_index.map(|index| IndexPath::new(index)),
            window,
            cx,
        )
    });
    let algorithm_index = match initial_draft.algorithm {
        HashAlgorithm::Sha256 => 0,
        HashAlgorithm::Sha512 => 1,
        HashAlgorithm::Sha1 => 2,
    };
    let algorithm = cx.new(|cx| {
        SelectState::new(
            vec![
                HashAlgorithmChoice(HashAlgorithm::Sha256),
                HashAlgorithmChoice(HashAlgorithm::Sha512),
                HashAlgorithmChoice(HashAlgorithm::Sha1),
            ].into_iter().enumerate().map(|(index, value)| hash_jump_select::HashSelectItem {
                value, index, collector: Rc::clone(&algorithm_popup),
            }).collect::<Vec<_>>(),
            Some(IndexPath::new(algorithm_index)),
            window,
            cx,
        )
    });

    if initial_draft.mode == HashInputMode::Text {
        text.focus_handle(cx).focus(window, cx);
    } else {
        source.update(cx, |state, cx| state.focus(window, cx));
    }

    let source_trigger = source.read(cx).focus_handle(cx);
    let algorithm_trigger = algorithm.read(cx).focus_handle(cx);
    let native_geometry_slot = new_native_geometry_slot();
    let native_geometry_stamp = NativeGeometryStamp {
        session_epoch,
        mount_generation,
    };
    let mut native_geometry_error = None;
    let factory_outer_size = native_window
        .and_then(|window| checked_geometry_target(window))
        .and_then(|target| match native_window_metrics(target, 1.0) {
            Ok(Some(metrics)) => outer_size_in_points(metrics),
            Ok(None) => None,
            Err(error) => {
                native_geometry_error = Some(format!(
                    "Could not inspect the Hash Generator window geometry: {error}"
                ));
                None
            }
        });
    let text_for_subscription = text.clone();
    let expected_for_subscription = expected.clone();
    let source_for_subscription = source.clone();
    let algorithm_for_subscription = algorithm.clone();
    cx.new(move |cx| {
        let text_subscription = cx.subscribe(
            &text_for_subscription,
            |this: &mut HashGeneratorWindow,
             text: Entity<TextareaState>,
             event: &InputEvent,
             cx: &mut Context<HashGeneratorWindow>| {
                if !matches!(event, InputEvent::Change)
                    || this.rehost_pending.load(Ordering::Acquire)
                    || this.clipboard_pending.is_copying()
                    || !this.is_current_mount()
                {
                    return;
                }
                let Some(_operation) = this.restart_admission.admit() else {
                    return;
                };
                let text = text.read(cx).value().to_string();
                let mut session = this
                    .session
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if session.mode() != HashInputMode::Text {
                    return;
                }
                if !hash_mount_identity_matches(
                    this.active_mount_generation.load(Ordering::Acquire),
                    this.mount_generation,
                    session.epoch(),
                    this.session_epoch,
                ) {
                    return;
                }
                session.set_text(text);
                cx.notify();
            },
        );

        let expected_subscription = cx.subscribe(
            &expected_for_subscription,
            |this: &mut HashGeneratorWindow,
             expected: Entity<TextareaState>,
             event: &InputEvent,
             cx: &mut Context<HashGeneratorWindow>| {
                if !matches!(event, InputEvent::Change)
                    || this.rehost_pending.load(Ordering::Acquire)
                    || this.clipboard_pending.is_copying()
                    || !this.is_current_mount()
                {
                    return;
                }
                let Some(_operation) = this.restart_admission.admit() else {
                    return;
                };
                let mut session = this
                    .session
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if session.epoch() != this.session_epoch {
                    return;
                }
                session.set_expected(expected.read(cx).value().to_string());
                cx.notify();
            },
        );

        let source_subscription = cx.subscribe(
            &source_for_subscription,
            |this: &mut HashGeneratorWindow,
             source: Entity<SelectState<Vec<hash_jump_select::HashSelectItem<HashSourceChoice>>>>,
             event: &SelectEvent<Vec<hash_jump_select::HashSelectItem<HashSourceChoice>>>,
             cx: &mut Context<HashGeneratorWindow>| {
                if this.clipboard_pending.is_copying() || !this.is_current_mount() {
                    return;
                }
                let SelectEvent::Confirm(Some(choice)) = event else {
                    return;
                };
                let Some(_operation) = this.restart_admission.admit() else {
                    return;
                };
                let selected = source.read(cx).selected_value().copied();
                if selected != Some(*choice) {
                    return;
                }
                let mut session = this
                    .session
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if session.epoch() != this.session_epoch {
                    return;
                }
                session.set_mode(choice.mode());
                this.file_picker_feedback = None;
                cx.notify();
            },
        );

        let algorithm_subscription = cx.subscribe(
            &algorithm_for_subscription,
            |this: &mut HashGeneratorWindow,
             algorithm: Entity<SelectState<Vec<hash_jump_select::HashSelectItem<HashAlgorithmChoice>>>>,
             event: &SelectEvent<Vec<hash_jump_select::HashSelectItem<HashAlgorithmChoice>>>,
             cx: &mut Context<HashGeneratorWindow>| {
                if this.clipboard_pending.is_copying() || !this.is_current_mount() {
                    return;
                }
                let SelectEvent::Confirm(Some(choice)) = event else {
                    return;
                };
                let Some(_operation) = this.restart_admission.admit() else {
                    return;
                };
                let selected = algorithm.read(cx).selected_value().copied();
                if selected != Some(*choice) {
                    return;
                }
                let mut session = this
                    .session
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if session.epoch() != this.session_epoch {
                    return;
                }
                session.set_algorithm(choice.0);
                cx.notify();
            },
        );

        source_popup.borrow_mut().bind(cx.entity().downgrade(), source_trigger);
        algorithm_popup.borrow_mut().bind(cx.entity().downgrade(), algorithm_trigger);
        let source_popup_for_dismiss = Rc::clone(&source_popup);
        let source_dismiss = cx.subscribe(
            &source_for_subscription,
            move |this: &mut HashGeneratorWindow, _, _: &DismissEvent, cx| {
                let epoch = source_popup_for_dismiss.borrow().epoch;
                source_popup_for_dismiss.borrow_mut().dismiss();
                if this.jump.scope
                    == Some(hash_jump::HashJumpScope::Popup(
                        hash_jump_select::HashPopupKind::Source,
                        epoch,
                    ))
                {
                    this.jump.retire();
                    cx.notify();
                }
            },
        );
        let algorithm_popup_for_dismiss = Rc::clone(&algorithm_popup);
        let algorithm_dismiss = cx.subscribe(
            &algorithm_for_subscription,
            move |this: &mut HashGeneratorWindow, _, _: &DismissEvent, cx| {
                let epoch = algorithm_popup_for_dismiss.borrow().epoch;
                algorithm_popup_for_dismiss.borrow_mut().dismiss();
                if this.jump.scope
                    == Some(hash_jump::HashJumpScope::Popup(
                        hash_jump_select::HashPopupKind::Algorithm,
                        epoch,
                    ))
                {
                    this.jump.retire();
                    cx.notify();
                }
            },
        );
        let jump = hash_jump::HashJump::new(jump_controller, cx);
        let focus_handle = cx.focus_handle();
        let focus_out = cx.on_focus_out(
            &focus_handle,
            window,
            |this: &mut HashGeneratorWindow, _, window, cx| {
                if !this.focus_handle.contains_focused(window, cx) && this.hash_popup_scope(window, cx).is_none() && this.jump.retire()
                {
                    cx.notify();
                }
            },
        );
        let activation =
            cx.observe_window_activation(window, |this: &mut HashGeneratorWindow, window, cx| {
                if window.is_window_active() && this.mode == HashPresentationMode::Popout {
                    let root = this.root.clone();
                    let mount = this.mount_generation;
                    let epoch = this.session_epoch;
                    let native = this.native_window;
                    let window_id = window.window_handle().window_id();
                    let view_id = cx.entity_id();
                    let _ = root.update(cx, |root, cx| {
                        root.observe_hash_pin_activation(mount, epoch, window_id, view_id, native, cx);
                    });
                } else if !window.is_window_active() {
                    let root = this.root.clone();
                    let mount = this.mount_generation;
                    let view_id = cx.entity_id();
                    let _ = root.update(cx, |root, _| root.retire_hash_pin_activation_on_blur(mount, view_id));
                    if this.jump.retire() { cx.notify(); }
                }
            });
        HashGeneratorWindow {
            jump,
            source_popup,
            algorithm_popup,
            text,
            expected,
            source,
            algorithm,
            focus_handle,
            session,
            wake_signal,
            file_picker_pending,
            clipboard_pending,
            pinned,
            native_window,
            native_geometry_slot,
            native_geometry_stamp,
            hash_tool_key,
            native_geometry_sequence,
            native_geometry_binding: None,
            native_geometry_error,
            size_write_error,
            factory_outer_size,
            restart_admission,
            mount_generation,
            session_epoch,
            active_mount_generation,
            rehost_pending,
            rehost_revision,
            presentation_feedback,
            root,
            mode,
            file_picker_feedback: None,
            _subscriptions: vec![
                text_subscription,
                expected_subscription,
                source_subscription,
                algorithm_subscription,
                source_dismiss,
                algorithm_dismiss,
                focus_out,
                activation,
            ],
        }
    })
}

fn hash_popout_window_options(cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::centered(size(px(620.), px(440.)), cx)),
        window_min_size: Some(size(px(420.), px(320.))),
        titlebar: Some(TitlebarOptions {
            title: Some("Multi Launcher GPUI Hash Generator".into()),
            ..Default::default()
        }),
        ..Default::default()
    }
}


enum SecondaryWindowState {
    Closed,
    OpenPending,
    Live {
        window: AnyWindowHandle,
        view: Entity<SecondaryWindow>,
    },
}

fn update_kit_window_content<T: 'static, C: AppContext, R>(
    window: AnyWindowHandle,
    content: &Entity<T>,
    cx: &mut C,
    update: impl FnOnce(&mut T, &mut Window, &mut Context<T>) -> R,
) -> anyhow::Result<R> {
    window.update(cx, |_, window, cx| {
        content.update(cx, |content, content_cx| {
            update(content, window, content_cx)
        })
    })
}

#[derive(Clone, Debug, PartialEq)]
struct RestartDrafts {
    search: String,
    utility: Option<String>,
    json: Option<String>,
    xml: Option<String>,
    regex: Option<multi_launcher::regex_tester::RegexDraft>,
    clipboard_history: Option<clipboard_history::RestartDraft>,
    clipboard_modify: Option<clipboard_modify_view::RestartDraft>,
    snippets: Option<snippets::RestartDraft>,
    snippet_fill: Option<Vec<String>>,
    tempfile: Option<(String, String)>,
    hash: HashDraft,
    hash_window_open: bool,
    settings_search: Option<settings_search::RestartDraft>,
    show_examples: bool,
    file_search_open: bool,
    downloads_triage_open: bool,
    plugin_editor_open: bool,
}

struct RestartSnapshot {
    drafts: RestartDrafts,
    hash_work: HashWork,
    child_windows: Vec<AnyWindowHandle>,
    native_windows: Vec<NativeWindow>,
}

struct AcceptedRestart {
    context: RestartContext,
    root_window: AnyWindowHandle,
    child_windows: Vec<AnyWindowHandle>,
    native_windows: Vec<NativeWindow>,
    listener_slot: Arc<Mutex<Option<HotkeyListener>>>,
    coordinate_controller: Arc<Mutex<CoordinateToolController>>,
    coordinate_shutdown_complete: Arc<AtomicBool>,
    restart_retiring: Arc<AtomicBool>,
    restart_exit: Rc<RefCell<Option<(RestartContext, Vec<NativeWindow>)>>>,
    restart_failure: Rc<RefCell<Option<String>>>,
}

async fn wait_for_native_retirement(cx: &mut AsyncApp, windows: &[NativeWindow]) -> bool {
    for _ in 0..100 {
        if windows
            .iter()
            .all(|window| !native_window_is_valid(*window))
        {
            return true;
        }
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    windows
        .iter()
        .all(|window| !native_window_is_valid(*window))
}

async fn wait_for_hash_window_retirement(
    cx: &mut AsyncApp,
    native_window: Option<NativeWindow>,
) -> bool {
    if let Some(native_window) = native_window {
        return wait_for_native_retirement(cx, &[native_window]).await;
    }
    #[cfg(windows)]
    {
        // A Windows handle must be checked before it can be retired safely.
        return false;
    }
    #[cfg(not(windows))]
    {
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
        true
    }
}

async fn remove_and_wait_for_hash_window(cx: &mut AsyncApp, retiring: RetiringHashWindow) -> bool {
    if let Err(error) = cx.update(|app| {
        retiring
            .window
            .update(app, |_, window, _| window.remove_window())
    }) {
        tracing::debug!(%error, "GPUI Hash window was already unavailable during retirement");
    }
    wait_for_hash_window_retirement(cx, retiring.native_window).await
}

async fn retire_unchecked_hash_candidate(root: &WeakEntity<RootWindow>, window: AnyWindowHandle, cx: &mut AsyncApp) {
    let signal = Arc::new(HashWakeSignal::default());
    let wait = signal.clone();
    let id = window.window_id();
    let installed = root.update(cx, |root, cx| {
        root.hash_retiring_windows.push(RetiringHashWindow { window, native_window: None });
        let owner = cx.entity().downgrade();
        let closed = signal.clone();
        let subscription = cx.on_window_closed(move |app, closed_id| {
            if closed_id == id {
                let _ = owner.update(app, |root, cx| {
                    root.hash_retiring_windows.retain(|item| item.window.window_id() != id);
                    cx.notify();
                });
                closed.notify();
            }
        });
        cx.spawn(async move |_, _| { let _subscription = subscription; wait.wait().await; }).detach();
    }).is_ok();
    if cx.update(|app| window.update(app, |_, window, _| window.remove_window())).is_err() && installed {
        let _ = root.update(cx, |root, cx| {
            root.hash_retiring_windows.retain(|item| item.window.window_id() != id);
            cx.notify();
        });
        signal.notify();
    }
}

async fn restore_root_search_focus_after_hash_close(
    root: WeakEntity<RootWindow>,
    close_revision: u64,
    return_receipt: Option<HashReturnReceipt>,
    retired_docked_focus: Option<FocusHandle>,
    cx: &mut AsyncApp,
) {
    if let Some(HashReturnReceipt::FileSearchPopup { receipt, mount, host, native }) = return_receipt.as_ref() {
        let view = root.update(cx, |root, cx| {
            if root.hash_window_revision.load(Ordering::Acquire) != close_revision || root.restart_admission.is_fenced()
                || !mount.is_current() || current_foreground_identity() != Some(native.identity) { return None; }
            root.file_search.clone().filter(|view| view.entity_id() == receipt.owner && view.read(cx).is_mounted_in(host.window_id(), true))
        }).ok().flatten();
        if let Some(view) = view {
            let _ = update_kit_window_content(*host, &view, cx, |view, window, cx| {
                if mount.is_current() && window.is_window_active() && current_foreground_identity() == Some(native.identity)
                    && window.focused(cx).is_none_or(|focus| focus == view.hash_action_focus(cx)) {
                    view.restore_receipt_focus(receipt, cx.entity_id(), window, cx);
                }
            });
        }
        return;
    }
    let target = root.update(cx, |root, cx| {
        let may_restore_search = root.action_sheet.is_none()
            && root.context_favorite_popup.is_none()
            && root.hash_window_revision.load(Ordering::Acquire) == close_revision
            && root.root_visibility.visible.load(Ordering::Acquire)
            && root_search_focus_is_current(
                root.hash_window.mode(),
                root.hash_window.mount_generation(),
                root.hash_mount_generation.load(Ordering::Acquire),
                None,
                0,
                root.restart_admission.is_fenced(),
            )
            && (!cfg!(windows) || root.native_window.is_some_and(native_window_is_valid));
        let target = may_restore_search.then(|| {
            root.root_window
                .map(|root_window| (root_window, root.native_window))
        });
        if target.is_some() {
            cx.notify();
        }
        target.flatten()
    });
    let Ok(Some((root_window, native_root))) = target else {
        return;
    };
    let Some(root_view) = root.upgrade() else {
        return;
    };

    let _ = update_kit_window_content(root_window, &root_view, cx, |root, window, cx| {
        let root_handle_matches = root
            .root_window
            .is_some_and(|handle| handle.window_id() == root_window.window_id());
        let still_current = root_handle_matches
            && root.action_sheet.is_none()
            && root.context_favorite_popup.is_none()
            && root.hash_window_revision.load(Ordering::Acquire) == close_revision
            && root.root_visibility.visible.load(Ordering::Acquire)
            && root_search_focus_is_current(
                root.hash_window.mode(),
                root.hash_window.mount_generation(),
                root.hash_mount_generation.load(Ordering::Acquire),
                None,
                0,
                root.restart_admission.is_fenced(),
            )
            && root.native_window == native_root
            && (!cfg!(windows) || native_root.is_some_and(native_window_is_valid));
        if !still_current {
            return;
        }
        if let Some(receipt) = return_receipt {
            let may_leave_focus = window.focused(cx).is_none_or(|focus| {
                !root.focus_handle.contains(&focus, window)
                    || retired_docked_focus.as_ref().is_some_and(|owner| owner.contains(&focus, window))
            });
            let restored = match receipt {
                HashReturnReceipt::FileSearchPopup { .. } => false,
                HashReturnReceipt::FileSearch(receipt) => {
                    if let Some(view) = root.file_search.clone().filter(|view| view.entity_id() == receipt.owner) {
                        if may_leave_focus || view.read(cx).hash_action_is_focused(window) {
                            let restored = view.update(cx, |view, cx| view.restore_receipt_focus(&receipt, cx.entity_id(), window, cx));
                            if restored { root.root_scroll.scroll_to_top_of_item(root.file_search_scroll_item()); }
                            restored
                        } else { false }
                    } else { false }
                }
                #[cfg(windows)]
                HashReturnReceipt::Downloads(receipt) => {
                    if let Some(view) = root.downloads_triage.clone().filter(|view| view.entity_id() == receipt.owner) {
                        if may_leave_focus || view.read(cx).inspect_is_focused(window) {
                            let restored = view.update(cx, |view, cx| view.restore_receipt_focus(&receipt, cx.entity_id(), window, cx));
                            if restored {
                                let index = root.file_search_scroll_item() + usize::from(root.file_search.as_ref().is_some_and(|view| view.read(cx).root_slot_occupied()));
                                root.root_scroll.scroll_to_top_of_item(index);
                            }
                            restored
                        } else { false }
                    } else { false }
                }
            };
            if restored || !may_leave_focus { return; }
        }
        // Restore logical focus while Windows independently chooses the next foreground window.
        root.query.focus_handle(cx).focus(window, cx);
    });
}

fn describe_restart_drafts(drafts: &RestartDrafts) -> String {
    let mut names = Vec::new();
    if !drafts.search.is_empty() {
        names.push("Search query");
    }
    if drafts.utility.as_ref().is_some_and(|text| !text.is_empty()) {
        names.push("retained utility editor");
    }
    if drafts.json.as_ref().is_some_and(|text| !text.is_empty()) {
        names.push("JSON text");
    }
    if drafts.xml.as_ref().is_some_and(|text| !text.is_empty()) { names.push("XML source"); }
    if drafts.regex.is_some() { names.push("Regex Tester draft"); }
    if drafts.clipboard_history.is_some() { names.push("Clipboard History edit or review"); }
    if drafts.clipboard_modify.is_some() { names.push("Clipboard Modify draft"); }
    if drafts.snippets.is_some() { names.push("Snippets draft"); }
    if drafts.snippet_fill.is_some() { names.push("Snippet prompt values"); }
    if drafts.tempfile.is_some() { names.push("Create Temp File draft"); }
    if !drafts.hash.text.is_empty() {
        names.push("Hash Generator text");
    }
    if !drafts.hash.expected.is_empty() {
        names.push("Expected digest");
    }
    if drafts.hash.file_path.is_some() {
        names.push("selected Hash Generator file");
    }
    if drafts.hash.clipboard_snapshot_revision.is_some() {
        names.push("loaded Clipboard snapshot");
    }
    if drafts.hash.mode != HashInputMode::Text {
        names.push("Hash Generator source selection");
    }
    if drafts.hash.algorithm != HashAlgorithm::Sha256 {
        names.push("non-default Hash Generator algorithm");
    }
    if drafts.settings_search.as_ref().is_some_and(|draft| draft.dirty()) { names.push("unsaved Settings preference"); }
    if drafts.file_search_open {
        names.push("File Search form and results");
    }
    if drafts.downloads_triage_open {
        names.push("Downloads Triage view and results");
    }
    if drafts.plugin_editor_open {
        names.push("Plugin Editor filter");
    }
    if names.is_empty() {
        return "Restart Multi Launcher in the egui fallback? The GPUI windows will close."
            .to_owned();
    }
    format!(
        "Restart Multi Launcher in the egui fallback? This will discard the current {} and close the GPUI windows.",
        names.join(", ")
    )
}

fn restart_blocker_message(blocker: RestartBlocker) -> String {
    match blocker {
        RestartBlocker::HashWork(HashWork::Active) => {
            "Hash Generator is still calculating. Cancel it or wait for it to finish, then retry the egui restart.".to_owned()
        }
        RestartBlocker::HashWork(HashWork::Retiring) => {
            "Hash Generator is still stopping its previous calculation. Wait for it to retire, then retry the egui restart.".to_owned()
        }
        RestartBlocker::HashWork(HashWork::Idle) => {
            "The restart confirmation is no longer active. Start it again to switch to egui.".to_owned()
        }
        RestartBlocker::PendingEffects(count) => format!(
            "Wait for the {count} pending GPUI operation(s) to finish, then retry the egui restart."
        ),
        RestartBlocker::ConfirmationInactive => {
            "The egui restart confirmation is no longer active. Start it again to switch frontends."
                .to_owned()
        }
    }
}

impl RootWindow {
    fn apply_pending_help_filter_sync(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((source, source_value)) = self.pending_help_filter_sync.take() else {
            return;
        };
        let peer_value = match source {
            HelpFilterPeer::QuickHelp => {
                self.command_help_filter.read(cx).value().to_string()
            }
            HelpFilterPeer::CommandHelp => self.quick_help_filter.read(cx).value().to_string(),
        };
        let Some((peer, value)) = self
            .shared_help_filter
            .synchronize(source, &source_value, &peer_value)
        else {
            return;
        };
        let state = match peer {
            HelpFilterPeer::QuickHelp => &self.quick_help_filter,
            HelpFilterPeer::CommandHelp => &self.command_help_filter,
        };
        if state.read(cx).value().to_string() != value {
            state.update(cx, |state, cx| state.replace_all(value, window, cx));
        }
    }

    fn seed_shared_help_filter(&mut self, seed: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.pending_help_filter_sync = None;
        let value = self.shared_help_filter.seed(seed);
        for state in [&self.command_help_filter, &self.quick_help_filter] {
            if state.read(cx).value().to_string() != value {
                let value = value.clone();
                state.update(cx, |state, cx| state.replace_all(value, window, cx));
            }
        }
    }

    fn set_root_menu_open(
        &mut self,
        menu: RootMenuKind,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if open {
            self.external_launch.retire_focus_bound_effects();
            self.retire_context_favorite();
        }
        let was_menu_open = self.file_menu_open || self.settings_menu_open || self.help_menu_open;
        match menu {
            RootMenuKind::File => self.file_menu_open = open,
            RootMenuKind::Settings => self.settings_menu_open = open,
            RootMenuKind::Help => self.help_menu_open = open,
        }
        if open && !was_menu_open {
            self.root_menu_action_routed_focus = false;
        }

        let another_menu_open =
            self.file_menu_open || self.settings_menu_open || self.help_menu_open;
        if !open {
            let current_focus_is_live_in_root = window
                .focused(cx)
                .is_some_and(|focus| self.focus_handle.contains(&focus, window));
            if root_menu_close_should_restore_focus(
                another_menu_open,
                current_focus_is_live_in_root,
                self.root_menu_action_routed_focus,
                window.is_window_active(),
            ) {
                let last_live_focus = self
                    .root_menu_previous_focus
                    .clone()
                    .filter(|focus| self.focus_handle.contains(focus, window));
                if let Some(focus) = last_live_focus {
                    focus.focus(window, cx);
                }
            }
            if !another_menu_open {
                self.root_menu_action_routed_focus = false;
            }
        }
        cx.notify();
    }

    fn remember_live_root_menu_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(focus) = window
            .focused(cx)
            .filter(|focus| self.focus_handle.contains(focus, window))
        {
            self.root_menu_previous_focus = Some(focus);
        }
    }

    fn focus_docked_hash_resize_handle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.hash_window.mode() != Some(HashPresentationMode::Docked)
            || self.hash_rehost_pending.load(Ordering::Acquire)
            || self.restart_admission.is_fenced()
        {
            return;
        }
        if self.quick_help.open || self.result_favorites.is_open() || self.file_search.as_ref().is_some_and(|view| view.read(cx).root_slot_occupied()) || self.downloads_triage_open(cx) || self.plugin_editor.as_ref().is_some_and(|view| view.read(cx).open) {
            self.root_scroll
                .scroll_to_top_of_item(self.docked_hash_scroll_item());
        } else {
            self.root_scroll.scroll_to_bottom();
        }
        self.resize_focus_handle.focus(window, cx);
        cx.notify();
    }

    fn forward_docked_hash_action(
        &mut self,
        action: DockedHashAction,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.resize_focus_handle.is_focused(window)
            || self.hash_window.mode() != Some(HashPresentationMode::Docked)
            || self.hash_rehost_pending.load(Ordering::Acquire)
            || self.restart_admission.is_fenced()
        {
            return false;
        }
        let HashWindowState::Docked {
            mount_generation,
            view,
        } = &self.hash_window
        else {
            return false;
        };
        let mount_generation = *mount_generation;
        let session_epoch = self
            .hash_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .epoch();
        if self.hash_mount_generation.load(Ordering::Acquire) != mount_generation {
            return false;
        }

        view.update(cx, |tool, cx| {
            if tool.mount_generation != mount_generation
                || tool.session_epoch != session_epoch
                || !tool.is_current_mount()
            {
                return false;
            }
            match action {
                DockedHashAction::Calculate => tool.calculate_hash(cx),
                DockedHashAction::Dock => tool.request_rehost(HashPresentationMode::Docked, cx),
                DockedHashAction::Popout => tool.request_rehost(HashPresentationMode::Popout, cx),
                DockedHashAction::Close => tool.request_close(hash_pin::CloseOrigin::Keyboard, cx),
            }
            true
        })
    }

    fn docked_hash_resize_is_current(&self, payload: &DockedHashResizePayload) -> bool {
        let docked_mount_is_current = self.hash_window.mode() == Some(HashPresentationMode::Docked)
            && self.hash_window.mount_generation() == Some(payload.mount_generation);
        let active_session_epoch = self
            .hash_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .epoch();
        Arc::ptr_eq(&self.hash_docked_resize, &payload.preview_extent)
            && docked_hash_resize_identity_matches(
                self.hash_mount_generation.load(Ordering::Acquire),
                payload.mount_generation,
                active_session_epoch,
                payload.session_epoch,
                docked_mount_is_current,
            )
            && self.hash_docked_preferred_size == payload.start_size
            && !self.hash_rehost_pending.load(Ordering::Acquire)
            && !self.restart_admission.is_fenced()
    }

    fn begin_docked_hash_pointer_resize(
        &mut self,
        payload: &DockedHashResizePayload,
        position: Point<Pixels>,
        _window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !self.docked_hash_resize_is_current(payload) {
            return;
        }
        let available = root_scroll_viewport_size(self.root_scroll.bounds());
        let visible_size = docked_hash_fit(payload.start_size, available);
        let mut preview = payload
            .preview_extent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *preview = Some(DockedHashResize {
            mount_generation: payload.mount_generation,
            session_epoch: payload.session_epoch,
            start_size: payload.start_size,
            available_at_start: available,
            preview_size: payload.start_size,
            changed_width: false,
            changed_height: false,
            drag_started: false,
            origin: DockedHashResizeOrigin::Pointer {
                position,
                visible_size,
            },
        });
        drop(preview);
        cx.notify();
    }

    fn update_docked_hash_pointer_resize(
        &mut self,
        payload: &DockedHashResizePayload,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if !self.docked_hash_resize_is_current(payload) {
            return;
        }
        let mut preview = payload
            .preview_extent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(resize) = preview.as_mut().filter(|resize| {
            resize.mount_generation == payload.mount_generation
                && resize.session_epoch == payload.session_epoch
                && resize.start_size == payload.start_size
                && resize.drag_started
        }) else {
            return;
        };
        let DockedHashResizeOrigin::Pointer {
            position: origin,
            visible_size,
        } = resize.origin
        else {
            return;
        };
        let delta_x = position.x.as_f32() - origin.x.as_f32();
        let delta_y = position.y.as_f32() - origin.y.as_f32();
        let (preview_size, changed_width, changed_height) =
            pointer_docked_resize_preview(resize.start_size, visible_size, delta_x, delta_y);
        if resize.preview_size != preview_size
            || resize.changed_width != changed_width
            || resize.changed_height != changed_height
        {
            resize.preview_size = preview_size;
            resize.changed_width = changed_width;
            resize.changed_height = changed_height;
            drop(preview);
            cx.notify();
        }
    }

    fn preview_docked_hash_keyboard_resize(
        &mut self,
        payload: &DockedHashResizePayload,
        delta_x: f32,
        delta_y: f32,
        available: AppliedWindowSize,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.docked_hash_resize_is_current(payload) {
            return false;
        }
        let mut preview = payload
            .preview_extent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match preview.as_mut() {
            Some(resize)
                if resize.mount_generation == payload.mount_generation
                    && resize.session_epoch == payload.session_epoch
                    && resize.start_size == payload.start_size
                    && matches!(resize.origin, DockedHashResizeOrigin::Keyboard) =>
            {
                resize.preview_size =
                    keyboard_docked_resize_preview(resize.preview_size, delta_x, delta_y);
                resize.changed_width |= delta_x != 0.0;
                resize.changed_height |= delta_y != 0.0;
            }
            Some(_) => return false,
            None => {
                let preview_size =
                    keyboard_docked_resize_preview(payload.start_size, delta_x, delta_y);
                *preview = Some(DockedHashResize {
                    mount_generation: payload.mount_generation,
                    session_epoch: payload.session_epoch,
                    start_size: payload.start_size,
                    available_at_start: available,
                    preview_size,
                    changed_width: delta_x != 0.0,
                    changed_height: delta_y != 0.0,
                    drag_started: false,
                    origin: DockedHashResizeOrigin::Keyboard,
                });
            }
        }
        drop(preview);
        cx.notify();
        true
    }

    fn cancel_docked_hash_resize(&mut self, payload: &DockedHashResizePayload) -> bool {
        if !Arc::ptr_eq(&self.hash_docked_resize, &payload.preview_extent) {
            return false;
        }
        let mut preview = payload
            .preview_extent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let matches = preview.as_ref().is_some_and(|resize| {
            resize.mount_generation == payload.mount_generation
                && resize.session_epoch == payload.session_epoch
                && resize.start_size == payload.start_size
        });
        if matches {
            preview.take();
        }
        matches
    }

    fn cancel_docked_hash_preview(&mut self) -> bool {
        let mut preview = self
            .hash_docked_resize
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        preview.take().is_some()
    }

    fn cancel_docked_hash_resize_for_mount(&mut self, mount_generation: u64, session_epoch: u64) {
        let mut preview = self
            .hash_docked_resize
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if preview.as_ref().is_some_and(|resize| {
            resize.mount_generation == mount_generation && resize.session_epoch == session_epoch
        }) {
            preview.take();
        }
    }

    fn commit_docked_hash_resize(
        &mut self,
        payload: &DockedHashResizePayload,
        keyboard: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.docked_hash_resize_is_current(payload) {
            return false;
        }
        let resize = {
            let mut preview = payload
                .preview_extent
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let matches = preview.as_ref().is_some_and(|resize| {
                resize.mount_generation == payload.mount_generation
                    && resize.session_epoch == payload.session_epoch
                    && resize.start_size == payload.start_size
                    && matches!(
                        (keyboard, resize.origin),
                        (true, DockedHashResizeOrigin::Keyboard)
                            | (false, DockedHashResizeOrigin::Pointer { .. })
                    )
            });
            matches.then(|| preview.take()).flatten()
        };
        let Some(resize) = resize else {
            return false;
        };
        let Some(size) = docked_resize_commit_size(resize) else {
            cx.notify();
            return true;
        };
        let accepted = self
            .utility_size_store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_preferred_size(self.hash_tool_key, size);
        match accepted {
            Ok(_revision) => {
                self.hash_docked_preferred_size = size;
                self.start_utility_size_ack_poll(cx);
            }
            Err(error) => {
                self.set_hash_size_write_error(
                    Some(format!(
                        "Could not save the Hash Generator docked size. Resize it again to retry. {error}"
                    )),
                    cx,
                );
            }
        }
        cx.notify();
        true
    }

    fn handle_docked_hash_resize_key(
        &mut self,
        payload: &DockedHashResizePayload,
        key: &str,
        available: AppliedWindowSize,
        cx: &mut Context<Self>,
    ) -> bool {
        match key {
            "left" => self.preview_docked_hash_keyboard_resize(
                payload,
                -HASH_DOCK_KEYBOARD_RESIZE_STEP,
                0.0,
                available,
                cx,
            ),
            "right" => self.preview_docked_hash_keyboard_resize(
                payload,
                HASH_DOCK_KEYBOARD_RESIZE_STEP,
                0.0,
                available,
                cx,
            ),
            "up" => self.preview_docked_hash_keyboard_resize(
                payload,
                0.0,
                -HASH_DOCK_KEYBOARD_RESIZE_STEP,
                available,
                cx,
            ),
            "down" => self.preview_docked_hash_keyboard_resize(
                payload,
                0.0,
                HASH_DOCK_KEYBOARD_RESIZE_STEP,
                available,
                cx,
            ),
            "enter" | "return" => self.commit_docked_hash_resize(payload, true, cx),
            "escape" => {
                let cancelled = self.cancel_docked_hash_resize(payload);
                if cancelled {
                    cx.notify();
                }
                cancelled
            }
            _ => false,
        }
    }

    fn docked_hash_resize_preview(
        &self,
        mount_generation: u64,
        session_epoch: u64,
    ) -> Option<PreferredWindowSize> {
        self.hash_docked_resize
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .filter(|resize| {
                resize.mount_generation == mount_generation
                    && resize.session_epoch == session_epoch
                    && self.hash_docked_preferred_size == resize.start_size
            })
            .map(|resize| resize.preview_size)
    }

    fn drain_hash_native_geometry(&mut self, cx: &mut Context<Self>, retire_binding: bool) {
        let Some(view) = self.hash_window.view().cloned() else {
            return;
        };
        let sample = view.update(cx, |tool, _| {
            let sample = tool.take_native_geometry();
            if retire_binding {
                tool.detach_native_geometry();
            }
            sample
        });
        let Some((sample, factory_size)) = sample else {
            return;
        };
        let key = self.hash_tool_key;
        let mut store = self
            .utility_size_store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let factory_size = store
            .effective_native_geometry(key)
            .is_none()
            .then_some(factory_size)
            .flatten();
        match store.observe_native_geometry(key, sample, factory_size) {
            Ok(Some(_revision)) => {
                drop(store);
                self.start_utility_size_ack_poll(cx);
            }
            Ok(None) => {}
            Err(error) => {
                drop(store);
                self.set_hash_size_write_error(
                    Some(format!(
                        "Could not save Hash Generator window geometry. Move or resize it again to retry. {error}"
                    )),
                    cx,
                );
            }
        }
    }

    fn poll_utility_size_writer(&mut self, cx: &mut Context<Self>) -> bool {
        let (pending, poll_error, write_failure, acknowledged, json_failure, json_acknowledged, xml_failure, xml_acknowledged, regex_failure, regex_acknowledged, clipboard_failure, clipboard_acknowledged, modify_failure, modify_acknowledged, search_failure, search_acknowledged, preview_failure, preview_acknowledged, folder_failure, folder_acknowledged, lock_failure, lock_acknowledged) = {
            let mut store = self
                .utility_size_store
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let poll = store.poll();
            let poll_error = poll.as_ref().err().cloned();
            let acknowledged = poll.as_ref().is_ok_and(|poll| poll.accepted.iter().any(|ack| ack.acknowledgement.key == self.hash_tool_key));
            let json_key = UtilityToolKey::parse("json-utility").ok();
            let json_acknowledged = poll.as_ref().is_ok_and(|poll| poll.accepted.iter().any(|ack| Some(ack.acknowledgement.key) == json_key));
            let xml_key = UtilityToolKey::parse("xml-toolkit").ok();
            let xml_acknowledged = poll.as_ref().is_ok_and(|poll| poll.accepted.iter().any(|ack| Some(ack.acknowledgement.key) == xml_key));
            let regex_key = UtilityToolKey::parse("regex-tester").ok();
            let regex_acknowledged = poll.as_ref().is_ok_and(|poll| poll.accepted.iter().any(|ack| Some(ack.acknowledgement.key) == regex_key));
            let clipboard_key = UtilityToolKey::parse("clipboard-history").ok();
            let clipboard_acknowledged = poll.as_ref().is_ok_and(|poll| poll.accepted.iter().any(|ack| Some(ack.acknowledgement.key) == clipboard_key));
            let modify_key = UtilityToolKey::parse("clipboard-modify").ok();
            let modify_acknowledged = poll.as_ref().is_ok_and(|poll| poll.accepted.iter().any(|ack| Some(ack.acknowledgement.key) == modify_key));
            let preview_key = UtilityToolKey::parse("file-preview").ok();
            let preview_acknowledged = poll.as_ref().is_ok_and(|poll| poll.accepted.iter().any(|ack| Some(ack.acknowledgement.key) == preview_key));
            let lock_key = UtilityToolKey::parse("file-lock").ok();
            let lock_acknowledged = poll.as_ref().is_ok_and(|poll| poll.accepted.iter().any(|ack| Some(ack.acknowledgement.key) == lock_key));
            let folder_key = UtilityToolKey::parse("folder-size").ok();
            let folder_acknowledged = poll.as_ref().is_ok_and(|poll| poll.accepted.iter().any(|ack| Some(ack.acknowledgement.key) == folder_key));
            let search_key = UtilityToolKey::parse("file-search").ok();
            let search_acknowledged = poll.as_ref().is_ok_and(|poll| poll.accepted.iter().any(|ack| Some(ack.acknowledgement.key) == search_key));
            let failures = store.take_new_write_failures();
            let preview_failure = failures.iter().find(|failure| Some(failure.key) == preview_key).map(|failure| failure.message.clone());
            let lock_failure = failures.iter().find(|failure| Some(failure.key) == lock_key).map(|failure| failure.message.clone());
            let folder_failure = failures.iter().find(|failure| Some(failure.key) == folder_key).map(|failure| failure.message.clone());
            let search_failure = failures.iter().find(|failure| Some(failure.key) == search_key).map(|failure| failure.message.clone());
            let modify_failure = failures.iter().find(|failure| Some(failure.key) == modify_key).map(|failure| failure.message.clone());
            let clipboard_failure = failures.iter().find(|failure| Some(failure.key) == clipboard_key).map(|failure| failure.message.clone());
            let regex_failure = failures.iter().find(|failure| Some(failure.key) == regex_key).map(|failure| failure.message.clone());
            let xml_failure = failures.iter().find(|failure| Some(failure.key) == xml_key).map(|failure| failure.message.clone());
            let write_failure = failures.iter().find(|failure| failure.key == self.hash_tool_key).map(|failure| failure.message.clone());
            let json_failure = failures.into_iter().find(|failure| Some(failure.key) == json_key).map(|failure| failure.message);
            (
                store.has_pending_acknowledgement(),
                poll_error,
                write_failure,
                acknowledged,
                json_failure,
                json_acknowledged,
                xml_failure,
                xml_acknowledged,
                regex_failure,
                regex_acknowledged,
                clipboard_failure,
                clipboard_acknowledged,
                modify_failure,
                modify_acknowledged,
                search_failure, search_acknowledged, preview_failure, preview_acknowledged, folder_failure, folder_acknowledged, lock_failure, lock_acknowledged,
            )
        };
        if let Some(error) = poll_error {
            self.set_hash_size_write_error(
                Some(format!(
                    "Could not publish the Hash Generator geometry save: {error}"
                )),
                cx,
            );
        } else if let Some(error) = write_failure {
            self.set_hash_size_write_error(
                Some(format!(
                    "Could not save Hash Generator window geometry. Move or resize it again to retry. {error}"
                )),
                cx,
            );
        } else if acknowledged {
            self.set_hash_size_write_error(None, cx);
        }
        if let Some(error) = json_failure { self.set_json_size_error(Some(format!("Could not save JSON Utility window geometry: {error}")), cx); }
        else if json_acknowledged { self.set_json_size_error(None, cx); }
        if let Some(error) = xml_failure { self.set_xml_size_error(Some(format!("Could not save XML Toolkit window geometry: {error}")), cx); }
        else if xml_acknowledged { self.set_xml_size_error(None, cx); }
        if let Some(error) = regex_failure { self.set_regex_size_error(Some(format!("Could not save Regex Tester window geometry: {error}")), cx); }
        else if regex_acknowledged { self.set_regex_size_error(None, cx); }
        if let Some(error) = clipboard_failure { self.set_clipboard_size_error(Some(format!("Could not save Clipboard History window geometry: {error}")), cx); }
        else if clipboard_acknowledged { self.set_clipboard_size_error(None, cx); }
        if let Some(error) = modify_failure { self.set_clipboard_modify_size_error(Some(format!("Could not save Clipboard Modify window geometry: {error}")), cx); }
        else if modify_acknowledged { self.set_clipboard_modify_size_error(None, cx); }
        if let Some(error) = search_failure { self.set_file_search_size_error(Some(format!("Could not save File Search window geometry: {error}")), cx); }
        else if search_acknowledged { self.set_file_search_size_error(None, cx); }
        if lock_failure.is_some() || lock_acknowledged {
            if let Some(view) = self.file_lock_view.clone() { view.update(cx, |view, cx| { view.geometry_error = lock_failure; cx.notify(); }); }
        }
        if folder_failure.is_some() || folder_acknowledged {
            if let Some(view) = self.folder_size_view.clone() { view.update(cx, |view, cx| { view.geometry_error = folder_failure; cx.notify(); }); }
        }
        if preview_failure.is_some() || preview_acknowledged {
            if let Some(view) = self.file_search.clone() { view.update(cx, |view, cx| view.set_preview_geometry_error(preview_failure, cx)); }
        }
        if !pending {
            self.utility_size_polling.store(false, Ordering::Release);
        }
        pending
    }

    fn set_hash_size_write_error(&mut self, message: Option<String>, cx: &mut Context<Self>) {
        self.hash_size_write_error = message.clone();
        if let Some(view) = self.hash_window.view().cloned() {
            view.update(cx, |tool, cx| {
                tool.size_write_error = message;
                cx.notify();
            });
        }
        cx.notify();
    }

    fn start_utility_size_ack_poll(&self, cx: &mut Context<Self>) {
        if !self
            .utility_size_polling
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            return;
        }
        let root = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(35))
                    .await;
                let pending = root.update(cx, |root, cx| root.poll_utility_size_writer(cx));
                match pending {
                    Ok(true) => {}
                    _ => break,
                }
            }
        })
        .detach();
    }

    fn restored_hash_geometry(&self) -> Option<NativeWindowGeometry> {
        self.utility_size_store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .effective_native_geometry(self.hash_tool_key)
    }

    fn schedule_hash_popout_setup(
        &self,
        native_window: NativeWindow,
        view: Entity<HashGeneratorWindow>,
        mount_generation: u64,
        session_epoch: u64,
        operation: EffectPermit,
        cx: &mut Context<Self>,
    ) {
        let target = checked_geometry_target(native_window);
        let geometry = self.restored_hash_geometry();
        let active_mount_generation = Arc::clone(&self.hash_mount_generation);
        let session = Arc::clone(&self.hash_session);
        let restart_admission = Arc::clone(&self.restart_admission);
        let root = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let _operation = operation;
            let current = || {
                active_mount_generation.load(Ordering::Acquire) == mount_generation
                    && session
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .epoch()
                        == session_epoch
                    && !restart_admission.is_fenced()
            };
            if !current() {
                return;
            }

            // This foreground task runs on GPUI's owner thread. Do not wrap
            // SetWindowPos in AsyncApp::update: WM_DPICHANGED may synchronously
            // call back into GPUI while the App RefCell is borrowed.
            let placement_error = target
                .ok_or_else(|| "the Hash Generator HWND could not be checked".to_owned())
                .and_then(|target| {
                    restore_native_window_geometry(target, geometry, (420, 320), 1.0).map(|_| ())
                })
                .err()
                .map(|error| {
                    format!("Could not restore the Hash Generator popout position: {error}")
                });
            if !current() {
                return;
            }

            let view_error = view.update(cx, |tool, cx| {
                if !tool.is_current_mount() || !current() {
                    return None;
                }
                let attach_error = tool
                    .attach_native_geometry()
                    .err()
                    .map(|error| format!("Native move/resize persistence is unavailable: {error}"));
                let mut errors = Vec::new();
                if let Some(error) = tool.native_geometry_error.take() {
                    errors.push(error);
                }
                if let Some(error) = placement_error.clone() {
                    errors.push(error);
                }
                if let Some(error) = attach_error {
                    errors.push(error);
                }
                let message = (!errors.is_empty()).then(|| errors.join(" "));
                tool.native_geometry_error = message.clone();
                cx.notify();
                message
            });
            if let Some(error) = view_error {
                let _ = root.update(cx, |root, cx| {
                    if root.hash_window.mount_generation() == Some(mount_generation)
                        && root
                            .hash_session
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .epoch()
                            == session_epoch
                    {
                        root.hash_native_geometry_error = Some(error);
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }

    fn capture_restart_snapshot(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<RestartSnapshot, String> {
        if self.folder_size_view.as_ref().is_some_and(|view| view.read(cx).busy()) { return Err(folder_size::WAIT.into()); }
        if self.file_lock_view.as_ref().is_some_and(|view| view.read(cx).busy()) { return Err(file_lock::WAIT.into()); }
        if self.calculator_search.active {
            return Err(calculator_search::SEARCH_PENDING_STATUS.to_owned());
        }
        if self.plugin_editor.as_ref().is_some_and(|view| view.read(cx).pending) {
            return Err("A plugin Search capability save is still pending.".to_owned());
        }
        if let Some(blocker) = self.app_editor.restart_blocker(cx) {
            return Err(blocker.to_owned());
        }
        if let Some(blocker) = self.result_favorites.restart_blocker() {
            return Err(blocker.to_owned());
        }
        if self.settings_search.as_ref().is_some_and(|view| view.read(cx).busy()) { return Err(settings_search::WAIT.to_owned()); }
        if self.command_help.pending.is_some() {
            return Err(
                "The Show examples preference save is still pending. Wait for its acknowledgement before restarting in egui."
                    .to_owned(),
            );
        }
        if self.settings_actions.as_ref().is_some_and(|view| view.read(cx).busy()) { return Err(settings_actions::WAIT.to_owned()); }
        if self.settings_actions.as_ref().is_some_and(|view| view.read(cx).restart_draft(cx).dirty()) { return Err(SETTINGS_DIRTY_RESTART_STATUS.to_owned()); }
        if self.settings_search.as_ref().is_some_and(|view| view.read(cx).restart_draft(cx).dirty()) {
            return Err(SETTINGS_DIRTY_RESTART_STATUS.to_owned());
        }
        if self
            .hash_docked_resize
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .is_some_and(|resize| {
                self.hash_window.mode() == Some(HashPresentationMode::Docked)
                    && self.hash_window.mount_generation() == Some(resize.mount_generation)
                    && self.hash_mount_generation.load(Ordering::Acquire) == resize.mount_generation
                    && self
                        .hash_session
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .epoch()
                        == resize.session_epoch
            })
        {
            return Err(
                "A docked Hash Generator resize is being previewed. Press Enter to save the size or Escape to discard it, then retry the egui restart."
                    .to_owned(),
            );
        }
        if self.clipboard_modify_startup.pending || self.clipboard_modify_view.as_ref().is_some_and(|view| view.read(cx).blocks_close() || view.read(cx).rehosting) { return Err(clipboard_modify_view::BLOCK_WAIT.to_owned()); }
        if self.external_launch.auxiliary_effect_pending() { return Err(file_search_preview::WAIT.to_owned()); }
        if self.tempfile_view.as_ref().is_some_and(|view| view.read(cx).pending) { return Err(tempfile_view::WAIT.to_owned()); }
        if self.snippets_view.as_ref().is_some_and(|view| view.read(cx).busy()) { return Err(snippets::SAVE_WAIT.to_owned()); }
        self.clipboard_modify_retiring.retain(|window| window.native_window.is_none_or(native_window_is_valid));
        if !self.clipboard_modify_retiring.is_empty() { return Err("A previous Clipboard Modify window has not retired.".to_owned()); }
        if self.clipboard_history.as_ref().is_some_and(|view| view.read(cx).busy() || view.read(cx).rehosting) { return Err(clipboard_history::WAIT.to_owned()); }
        self.clipboard_retiring.retain(|window| window.native_window.is_none_or(native_window_is_valid));
        if !self.clipboard_retiring.is_empty() { return Err("A previous Clipboard History window has not retired.".to_owned()); }
        if self.structured_pins.busy() { return Err("Wait for the utility pin update to finish before restarting.".to_owned()); }
        if self.regex_view.as_ref().is_some_and(|view| view.read(cx).clipboard_pending) { return Err("Wait for the Regex Tester clipboard operation before restarting.".to_owned()); }
        if self.regex_view.as_ref().is_some_and(|view| view.read(cx).history.busy()) { return Err(regex_history::WAIT_RESTART.to_owned()); }
        if self.regex_view.as_ref().is_some_and(|view| view.read(cx).presets.busy()) { return Err(regex_presets::WAIT_RESTART.to_owned()); }
        if self.xml_view.as_ref().is_some_and(|view| view.read(cx).file_effect_pending()) { return Err("Wait for the XML file operation to finish before restarting.".to_owned()); }
        if let Some(error) = self.xml_view.as_ref().and_then(|view| view.read(cx).shutdown_failure()) {
            return Err(format!("Cannot restart because {error}. Close and rerun Multi Launcher."));
        }
        self.drain_hash_native_geometry(cx, false);
        self.drain_json_native_geometry(cx, false);
        self.drain_xml_native_geometry(cx, false);
        self.drain_regex_native_geometry(cx, false);
        self.drain_file_search_geometry(cx, false);
        self.drain_folder_size_geometry(cx, false);
        self.drain_file_lock_geometry(cx, false);
        self.drain_file_preview_geometry(cx, false);
        self.drain_clipboard_native_geometry(cx, false);
        self.drain_clipboard_modify_geometry(cx, false);
        let _ = self.poll_utility_size_writer(cx);
        let size_status = self
            .utility_size_store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .status(self.hash_tool_key);
        if size_status.pending_revision.is_some() {
            self.start_utility_size_ack_poll(cx);
            return Err(
                "The Hash Generator window geometry is still being saved. Wait for the settings write to finish, then retry the egui restart."
                    .to_owned(),
            );
        }
        if let Some((_, error)) = size_status.failure {
            return Err(format!(
                "The Hash Generator window geometry could not be saved: {error}. Move or resize the popout to retry the save before restarting."
            ));
        }
        if let Ok(key) = UtilityToolKey::parse("json-utility") {
            let status = self.utility_size_store.lock().unwrap_or_else(std::sync::PoisonError::into_inner).status(key);
            if status.pending_revision.is_some() {
                self.start_utility_size_ack_poll(cx);
                return Err("The JSON Utility window geometry is still being saved. Wait for the settings write to finish, then retry the egui restart.".to_owned());
            }
            if let Some((_, error)) = status.failure {
                return Err(format!("The JSON Utility window geometry could not be saved: {error}. Move or resize the popout to retry before restarting."));
            }
        }
        if let Ok(key) = UtilityToolKey::parse("xml-toolkit") {
            let status = self.utility_size_store.lock().unwrap_or_else(std::sync::PoisonError::into_inner).status(key);
            if status.pending_revision.is_some() { self.start_utility_size_ack_poll(cx); return Err("The XML Toolkit window geometry is still being saved.".to_owned()); }
            if let Some((_, error)) = status.failure { return Err(format!("The XML Toolkit window geometry could not be saved: {error}")); }
        }
        if let Ok(key) = UtilityToolKey::parse("regex-tester") {
            let status = self.utility_size_store.lock().unwrap_or_else(std::sync::PoisonError::into_inner).status(key);
            if status.pending_revision.is_some() { self.start_utility_size_ack_poll(cx); return Err("The Regex Tester window geometry is still being saved.".to_owned()); }
            if let Some((_, error)) = status.failure { return Err(format!("The Regex Tester window geometry could not be saved: {error}")); }
        }
        if let Ok(key) = UtilityToolKey::parse("file-lock") {
            let status = self.utility_size_store.lock().unwrap_or_else(std::sync::PoisonError::into_inner).status(key);
            if status.pending_revision.is_some() { self.start_utility_size_ack_poll(cx); return Err("The File Lock window geometry is still being saved.".to_owned()); }
            if let Some((_, error)) = status.failure { return Err(format!("The File Lock window geometry could not be saved: {error}")); }
        }
        if let Ok(key) = UtilityToolKey::parse("folder-size") {
            let status = self.utility_size_store.lock().unwrap_or_else(std::sync::PoisonError::into_inner).status(key);
            if status.pending_revision.is_some() { self.start_utility_size_ack_poll(cx); return Err("The Folder Size window geometry is still being saved.".to_owned()); }
            if let Some((_, error)) = status.failure { return Err(format!("The Folder Size window geometry could not be saved: {error}")); }
        }
        if let Ok(key) = UtilityToolKey::parse("file-search") {
            let status = self.utility_size_store.lock().unwrap_or_else(std::sync::PoisonError::into_inner).status(key);
            if status.pending_revision.is_some() { self.start_utility_size_ack_poll(cx); return Err("The File Search window geometry is still being saved.".to_owned()); }
            if let Some((_, error)) = status.failure { return Err(format!("The File Search window geometry could not be saved: {error}")); }
        }
        if let Ok(key) = UtilityToolKey::parse("file-preview") {
            let status = self.utility_size_store.lock().unwrap_or_else(std::sync::PoisonError::into_inner).status(key);
            if status.pending_revision.is_some() { self.start_utility_size_ack_poll(cx); return Err("The File Search Preview window geometry is still being saved.".to_owned()); }
            if let Some((_, error)) = status.failure { return Err(format!("The File Search Preview window geometry could not be saved: {error}")); }
        }
        if let Some(message) = hash_clipboard_close_message(self.hash_clipboard_pending.kind()) {
            return Err(format!("{message} Then retry the egui restart."));
        }
        self.hash_retiring_windows
            .retain(|retiring| retiring.native_window.is_none_or(native_window_is_valid));
        if self.hash_rehost_pending.load(Ordering::Acquire) {
            return Err(
                "Hash Generator is changing presentation. Wait for docking or popout to finish, then retry the egui restart."
                    .to_owned(),
            );
        }
        if self.utility_presentations.has_pending() {
            return Err(
                "The Hash Generator presentation preference is still being saved. Wait for the save to finish, then retry the egui restart."
                    .to_owned(),
            );
        }
        if !self.hash_retiring_windows.is_empty() {
            return Err(
                "A previous Hash Generator popout has not retired yet. Wait for its window to close, then retry the egui restart."
                    .to_owned(),
            );
        }
        if self.coordinate_hud_probe_pending {
            return Err(
                "Coordinate HUD is still changing state. Wait for it to finish, then retry the egui restart."
                    .to_owned(),
            );
        }
        if self.hash_file_picker_pending.load(Ordering::Acquire) {
            return Err(
                "The Hash Generator file picker is still open. Finish or cancel it, then retry the egui restart."
                    .to_owned(),
            );
        }

        let mut child_windows = Vec::new();
        let mut native_windows = self.native_window.into_iter().collect::<Vec<_>>();
        if let Some(view) = self.file_search.clone() {
            for (host, native) in view.update(cx, |view, cx| view.preview_windows_for_restart(cx))? {
                child_windows.push(host); native_windows.push(native);
            }
        }
        if let Some(popup) = &self.clipboard_popup { child_windows.push(popup.window); if let Some(native) = popup.native { native_windows.push(native); } else if cfg!(windows) { return Err("The Clipboard History window has no checked native handle.".to_owned()); } }
        if let Some(popup) = &self.clipboard_modify_popup { child_windows.push(popup.window); if let Some(native) = popup.native { native_windows.push(native); } else if cfg!(windows) { return Err("The Clipboard Modify window has no checked native handle.".to_owned()); } }

        if self.file_lock_view.as_ref().is_some_and(|view| view.read(cx).rehosting) { return Err("File Lock is changing presentation.".to_owned()); }
        self.file_lock_retiring.retain(|window| window.native_window.is_none_or(native_window_is_valid));
        if !self.file_lock_retiring.is_empty() { return Err("A previous File Lock window has not retired.".to_owned()); }
        if let Some(popup) = &self.file_lock_popup {
            child_windows.push(popup.window);
            if let Some(native) = popup.native { native_windows.push(native); }
            else if cfg!(windows) { return Err("The File Lock window has no checked native handle.".to_owned()); }
        }
        if self.folder_size_view.as_ref().is_some_and(|view| view.read(cx).rehosting) { return Err("Folder Size is changing presentation.".to_owned()); }
        self.folder_size_retiring.retain(|window| window.native_window.is_none_or(native_window_is_valid));
        if !self.folder_size_retiring.is_empty() { return Err("A previous Folder Size window has not retired.".to_owned()); }
        if let Some(popup) = &self.folder_size_popup {
            child_windows.push(popup.window);
            if let Some(native) = popup.native { native_windows.push(native); }
            else if cfg!(windows) { return Err("The Folder Size window has no checked native handle.".to_owned()); }
        }
        if self.file_search.as_ref().is_some_and(|view| view.read(cx).rehosting) { return Err("File Search is changing presentation. Wait for it to finish before restarting.".to_owned()); }
        self.file_search_retiring.retain(|window| window.native_window.is_none_or(native_window_is_valid));
        if !self.file_search_retiring.is_empty() { return Err("A previous File Search window has not retired.".to_owned()); }
        if let Some(popup) = &self.file_search_popup {
            child_windows.push(popup.window);
            if let Some(native) = popup.native { native_windows.push(native); }
            else if cfg!(windows) { return Err("The File Search window has no checked native handle.".to_owned()); }
        }
        if self.json_view.as_ref().is_some_and(|view| view.read(cx).rehosting) {
            return Err("JSON Utility is changing presentation. Wait for it to finish before restarting.".to_owned());
        }
        self.json_retiring.retain(|window| window.native_window.is_none_or(native_window_is_valid));
        if !self.json_retiring.is_empty() { return Err("A previous JSON window has not retired.".to_owned()); }
        if let Some(popup) = &self.json_popup {
            child_windows.push(popup.window);
            if let Some(native) = popup.native { native_windows.push(native); }
            else if cfg!(windows) { return Err("The JSON window has no checked native handle.".to_owned()); }
        }
        if self.xml_view.as_ref().is_some_and(|view| view.read(cx).rehosting) { return Err("XML Toolkit is changing presentation. Wait for it to finish before restarting.".to_owned()); }
        self.xml_retiring.retain(|window| window.native_window.is_none_or(native_window_is_valid));
        if !self.xml_retiring.is_empty() { return Err("A previous XML window has not retired.".to_owned()); }
        if let Some(popup) = &self.xml_popup {
            child_windows.push(popup.window);
            if let Some(native) = popup.native { native_windows.push(native); }
            else if cfg!(windows) { return Err("The XML window has no checked native handle.".to_owned()); }
        }
        if self.regex_view.as_ref().is_some_and(|view| view.read(cx).rehosting) { return Err("Regex Tester is changing presentation. Wait for it to finish before restarting.".to_owned()); }
        self.regex_retiring.retain(|window| window.native_window.is_none_or(native_window_is_valid));
        if !self.regex_retiring.is_empty() { return Err("A previous Regex Tester window has not retired.".to_owned()); }
        if let Some(popup) = &self.regex_popup {
            child_windows.push(popup.window);
            if let Some(native) = popup.native { native_windows.push(native); }
            else if cfg!(windows) { return Err("The Regex Tester window has no checked native handle.".to_owned()); }
        }
        let utility = match &self.secondary {
            SecondaryWindowState::Closed => None,
            SecondaryWindowState::OpenPending => {
                return Err(
                    "The retained utility window is still opening. Wait for it to finish, then retry the egui restart."
                        .to_owned(),
                );
            }
            SecondaryWindowState::Live { window, view } => {
                let handle = *window;
                let (text, native) =
                    update_kit_window_content(handle, view, cx, |utility, _, cx| {
                        (
                            utility.query.read(cx).value().to_string(),
                            utility.native_window,
                        )
                    })
                    .map_err(|error| {
                        format!("Could not inspect the retained utility draft: {error}")
                    })?;
                child_windows.push(handle);
                if let Some(native) = native {
                    native_windows.push(native);
                } else if cfg!(windows) {
                    return Err(
                        "The retained utility window has no checked Windows handle; close it and retry the egui restart."
                            .to_owned(),
                    );
                }
                Some(text)
            }
        };

        let (mut hash_draft, hash_work) = {
            let session = self
                .hash_session
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let work = if session.is_running() {
                HashWork::Active
            } else if session.has_job() {
                HashWork::Retiring
            } else {
                HashWork::Idle
            };
            (HashDraft::from_session(&session), work)
        };
        let hash_window_open = self.hash_window.mode().is_some();
        match &self.hash_window {
            HashWindowState::Closed => {}
            HashWindowState::Docked { view, .. } => {
                hash_draft = view.update(cx, |tool, cx| tool.draft(cx));
            }
            HashWindowState::Popout { window, view, .. } => {
                let handle = *window;
                let (draft, native) = update_kit_window_content(handle, view, cx, |tool, _, cx| {
                    (tool.draft(cx), tool.native_window)
                })
                .map_err(|error| format!("Could not inspect the Hash Generator window: {error}"))?;
                hash_draft = draft;
                child_windows.push(handle);
                if let Some(native) = native {
                    native_windows.push(native);
                } else if cfg!(windows) {
                    return Err(
                        "The Hash Generator window has no checked Windows handle; close it and retry the egui restart."
                            .to_owned(),
                    );
                }
            }
        }

        if cfg!(windows) {
            if self.native_window.is_none() {
                return Err(
                    "The GPUI root has no checked Windows handle; restart is unavailable until the window is restored."
                        .to_owned(),
                );
            }
            if native_windows
                .iter()
                .any(|window| !native_window_is_valid(*window))
            {
                return Err(
                    "A GPUI window handle became stale. Close any utility windows and retry the egui restart."
                        .to_owned(),
                );
            }
        }

        Ok(RestartSnapshot {
            drafts: RestartDrafts {
                search: self.query.read(cx).value().to_string(),
                utility,
                json: self.json_view.as_ref().and_then(|view| view.read(cx).draft()),
                xml: self.xml_view.as_ref().and_then(|view| view.read(cx).draft()),
                regex: self.regex_view.as_ref().and_then(|view| view.read(cx).draft()),
                clipboard_history: self.clipboard_history.as_ref().and_then(|view| view.read(cx).restart_draft(cx)),
                clipboard_modify: self.clipboard_modify_view.as_ref().and_then(|view| view.read(cx).restart_draft(cx)),
                snippets: self.snippets_view.as_ref().and_then(|view| view.read(cx).restart_draft(cx)),
                snippet_fill: self.snippet_fill.as_ref().and_then(|view| view.read(cx).restart_draft(cx)),
                tempfile: self.tempfile_view.as_ref().and_then(|view| view.read(cx).restart_draft(cx)),
                hash: hash_draft,
                hash_window_open,
                settings_search: self.settings_search.as_ref().map(|view| view.read(cx).restart_draft(cx)),
                show_examples: self.command_help.show_examples,
                file_search_open: self.file_search.as_ref().is_some_and(|view| view.read(cx).open),
                downloads_triage_open: self.downloads_triage_open(cx),
                plugin_editor_open: self.plugin_editor.as_ref().is_some_and(|view| view.read(cx).open),
            },
            hash_work,
            child_windows,
            native_windows,
        })
    }

    fn open_restart_confirmation(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.external_launch.retire_focus_bound_effects();
        self.retire_action_sheet_now(window, cx, true);
        if self.action_sheet.is_some() { return false; }
        self.retire_context_favorite();
        let Some(context) = self.restart_context.clone() else {
            self.restart_status = Some(format!(
                "Could not capture the executable path and working directory before restart. Close and relaunch manually with {}.",
                Frontend::Egui.selector_argument()
            ));
            cx.notify();
            return false;
        };
        if !self.restart_admission.begin_confirmation() {
            self.restart_status = Some(
                "An egui restart has already been accepted or is awaiting cancellation.".to_owned(),
            );
            cx.notify();
            return false;
        }

        let snapshot = match self.capture_restart_snapshot(cx) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.restart_admission.cancel_confirmation();
                self.restart_status = Some(error);
                cx.notify();
                return false;
            }
        };
        if snapshot.hash_work != HashWork::Idle {
            self.restart_admission.cancel_confirmation();
            self.restart_status = Some(restart_blocker_message(RestartBlocker::HashWork(
                snapshot.hash_work,
            )));
            cx.notify();
            return false;
        }
        let pending = self.restart_admission.pending_effects();
        if pending != 0 {
            self.restart_admission.cancel_confirmation();
            self.restart_status = Some(restart_blocker_message(RestartBlocker::PendingEffects(
                pending,
            )));
            cx.notify();
            return false;
        }

        let description = describe_restart_drafts(&snapshot.drafts);
        let expected_drafts = snapshot.drafts;
        let root = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let root_for_cancel = root.clone();
            let root_for_close = root.clone();
            let root_for_accept = root.clone();
            let context_for_accept = context.clone();
            let expected_drafts_for_accept = expected_drafts.clone();
            alert
                .title(Label::new("Restart in egui?"))
                .description(Label::new(description.clone()))
                .confirm()
                .ok_text("Restart in egui")
                .cancel_text("Cancel")
                .on_cancel(move |_, _, app| {
                    let _ = root_for_cancel.update(app, |root, cx| {
                        root.restart_admission.cancel_confirmation();
                        root.restart_status = None;
                        cx.notify();
                    });
                    true
                })
                .on_close(move |_, _, app| {
                    let _ = root_for_close.update(app, |root, cx| {
                        root.restart_admission.cancel_confirmation();
                        cx.notify();
                    });
                })
                .on_ok(move |_, _, app| {
                    let root_for_task = root_for_accept.clone();
                    let accepted = root_for_accept.update(app, |root, cx| {
                        root.accept_restart(
                            context_for_accept.clone(),
                            &expected_drafts_for_accept,
                            cx,
                        )
                    });
                    match accepted {
                        Ok(Ok(accepted)) => {
                            app.spawn(async move |cx| {
                                Self::complete_restart(root_for_task, accepted, cx).await;
                            })
                            .detach();
                        }
                        Ok(Err(error)) => {
                            let _ = root_for_accept.update(app, |root, cx| {
                                root.restart_admission.cancel_confirmation();
                                root.restart_status = Some(error);
                                cx.notify();
                            });
                        }
                        Err(error) => {
                            tracing::error!(%error, "could not validate the GPUI restart request");
                        }
                    }
                    true
                })
        });
        true
    }

    fn accept_restart(
        &mut self,
        context: RestartContext,
        expected_drafts: &RestartDrafts,
        cx: &mut Context<Self>,
    ) -> Result<AcceptedRestart, String> {
        let snapshot = self.capture_restart_snapshot(cx)?;
        if &snapshot.drafts != expected_drafts {
            return Err(
                "A GPUI editor changed after the confirmation opened. Review its current draft and confirm the egui restart again."
                    .to_owned(),
            );
        }
        let root_window = self.root_window.ok_or_else(|| {
            "The GPUI root window handle is unavailable; no egui process was started.".to_owned()
        })?;
        self.restart_admission
            .try_accept(snapshot.hash_work)
            .map_err(restart_blocker_message)?;
        self.drain_hash_native_geometry(cx, true);

        self.query
            .update(cx, |input, cx| input.set_readonly(true, cx));
        if let SecondaryWindowState::Live { view, .. } = &self.secondary {
            let view = view.clone();
            view.update(cx, |utility, cx| {
                utility
                    .query
                    .update(cx, |input, cx| input.set_readonly(true, cx));
            });
        }
        if let Some(view) = self.hash_window.view() {
            let view = view.clone();
            view.update(cx, |tool, cx| tool.set_editors_readonly(true, cx));
        }

        {
            let mut session = self
                .hash_session
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if session.file_path() != snapshot.drafts.hash.file_path.as_deref() {
                session.set_file_path(snapshot.drafts.hash.file_path.clone());
            }
            session.set_text(snapshot.drafts.hash.text.clone());
            session.set_expected(snapshot.drafts.hash.expected.clone());
            session.set_mode(snapshot.drafts.hash.mode);
            session.set_algorithm(snapshot.drafts.hash.algorithm);
        }
        self.root_visibility.deactivate();
        self.snippets_search.wake.close();
        self.window_search.retire();
        self.file_search_popup_intent.store(false, Ordering::Release); self.file_search_probe.retire();
        self.indexed_applications.retire();
        self.retire_search_jump_mode();
        self.drain_file_search_geometry(cx, true);
        self.drain_folder_size_geometry(cx, true);
        self.drain_file_lock_geometry(cx, true);
        self.drain_file_preview_geometry(cx, true);
        if let Some(view) = self.file_search.clone() { view.update(cx, |view, cx| view.shutdown(cx)); }
        if let Some(view) = self.folder_size_view.clone() { view.update(cx, |view, _| view.shutdown()); }
        if let Some(view) = self.file_lock_view.clone() { view.update(cx, |view, _| view.shutdown()); }
        self.retire_json_workspace(cx);
        self.drain_regex_native_geometry(cx, true);
        self.drain_clipboard_native_geometry(cx, true);
        self.drain_clipboard_modify_geometry(cx, true);
        if let Some(view) = &self.regex_view { view.update(cx, |view, _| view.shutdown()); }
        if let Some(view) = &self.clipboard_history { view.update(cx, |view, _| view.shutdown()); }
        if let Some(view) = &self.clipboard_modify_view { view.update(cx, |view, _| view.shutdown_host()); }
        if let Some(view) = &self.xml_view { view.update(cx, |view, cx| view.shutdown(cx)); }
        #[cfg(windows)]
        if let Some(view) = self.downloads_triage.clone() { view.update(cx, |view, _| view.shutdown()); }
        self.coordinate_hud_probe_closing
            .store(true, Ordering::Release);
        self.coordinate_hud_probe_status = "Closing GPUI services for the egui restart…".to_owned();
        self.hash_wake_signal.close();
        self.hash_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .shutdown();
        self.hash_window_revision.fetch_add(1, Ordering::AcqRel);
        self.secondary_revision.fetch_add(1, Ordering::AcqRel);
        self.restart_status = None;
        cx.notify();

        Ok(AcceptedRestart {
            context,
            root_window,
            child_windows: snapshot.child_windows,
            native_windows: snapshot.native_windows,
            listener_slot: Arc::clone(&self.listener_slot),
            coordinate_controller: Arc::clone(&self.coordinate_hud_probe_controller),
            coordinate_shutdown_complete: Arc::clone(&self.coordinate_shutdown_complete),
            restart_retiring: Arc::clone(&self.restart_retiring),
            restart_exit: Rc::clone(&self.restart_exit),
            restart_failure: Rc::clone(&self.restart_failure),
        })
    }

    async fn complete_restart(
        root: WeakEntity<Self>,
        accepted: AcceptedRestart,
        cx: &mut AsyncApp,
    ) {
        loop {
            let state = root.update(cx, |root, cx| root.xml_view.as_ref().map(|view| (view.read(cx).reaped(), view.read(cx).shutdown_failure())));
            match state {
                Ok(None) | Ok(Some((true, None))) => break,
                Ok(Some((_, Some(error)))) => {
                    let _ = root.update(cx, |root, cx| { root.restart_status = Some(format!("Egui restart stopped because GPUI teardown failed: {error}. No new process was started. Close and rerun with {}.", Frontend::Egui.selector_argument())); cx.notify(); });
                    return;
                },
                Ok(Some((false, None))) => {},
                Err(_) => return,
            }
            cx.background_executor().timer(Duration::from_millis(40)).await;
        }
        let listener_slot = Arc::clone(&accepted.listener_slot);
        let coordinate_controller = Arc::clone(&accepted.coordinate_controller);
        let coordinate_shutdown_complete = Arc::clone(&accepted.coordinate_shutdown_complete);
        let teardown = cx
            .background_executor()
            .spawn(async move {
                let listener_stop = match listener_slot.lock() {
                    Ok(mut slot) => slot
                        .take()
                        .map_or(Ok(()), |mut listener| listener.stop_checked()),
                    Err(_) => Err("the hotkey listener owner lock was poisoned".to_owned()),
                };
                listener_stop?;
                coordinate_controller
                    .lock()
                    .map_err(|_| "the Coordinate HUD controller lock was poisoned".to_owned())?
                    .shutdown()?;
                coordinate_shutdown_complete.store(true, Ordering::Release);
                Ok::<(), String>(())
            })
            .await;

        if let Err(error) = teardown {
            let _ = root.update(cx, |root, cx| {
                root.restart_status = Some(format!(
                    "Egui restart stopped because GPUI teardown failed: {error}. No new process was started. GPUI remains open; copy any unsaved text, then close and rerun with {}.",
                    Frontend::Egui.selector_argument()
                ));
                cx.notify();
            });
            return;
        }

        let child_windows = accepted.child_windows;
        let child_native_windows = accepted
            .native_windows
            .iter()
            .skip(1)
            .copied()
            .collect::<Vec<_>>();
        let close_children_error = RefCell::new(None::<String>);
        cx.update(|app| {
            for window in child_windows {
                if let Err(error) = window.update(app, |_, window, _| window.remove_window()) {
                    close_children_error
                        .borrow_mut()
                        .replace(format!("could not retire a GPUI utility window: {error}"));
                    break;
                }
            }
        });
        let close_children = close_children_error.into_inner().map_or(Ok(()), Err);
        if let Err(error) = close_children {
            let _ = root.update(cx, |root, cx| {
                root.restart_status = Some(format!(
                    "Egui restart stopped because a GPUI utility window could not be closed: {error}. No new process was started. GPUI remains open; copy any unsaved text, then close and rerun with {}.",
                    Frontend::Egui.selector_argument()
                ));
                cx.notify();
            });
            return;
        }
        if !wait_for_native_retirement(cx, &child_native_windows).await {
            let _ = root.update(cx, |root, cx| {
                root.restart_status = Some(format!(
                    "Egui restart stopped because a utility window did not retire. No new process was started. GPUI remains open; copy any unsaved text, then close and rerun with {}.",
                    Frontend::Egui.selector_argument()
                ));
                cx.notify();
            });
            return;
        }

        let root_window = accepted.root_window;
        let close_root_error = RefCell::new(None::<String>);
        cx.update(|app| {
            app.set_quit_mode(QuitMode::Explicit);
            accepted.restart_retiring.store(true, Ordering::Release);
            if let Err(error) = root_window.update(app, |_, window, _| window.remove_window()) {
                close_root_error
                    .borrow_mut()
                    .replace(format!("could not retire the GPUI root window: {error}"));
            }
        });
        let close_root = close_root_error.into_inner().map_or(Ok(()), Err);
        if let Err(error) = close_root {
            accepted.restart_failure.borrow_mut().replace(format!(
                "The GPUI root window could not be closed during restart: {error}. No egui process was started; manually close Multi Launcher and rerun with {}.",
                Frontend::Egui.selector_argument()
            ));
            let _ = cx.update(|app| app.quit());
            return;
        }

        let root_native_window = accepted.native_windows.first().copied();
        let root_retired = match root_native_window {
            Some(window) => wait_for_native_retirement(cx, &[window]).await,
            None => true,
        };
        if !root_retired {
            accepted.restart_failure.borrow_mut().replace(format!(
                "The GPUI root window did not retire before restart. No egui process was started; manually close Multi Launcher and rerun with {}.",
                Frontend::Egui.selector_argument()
            ));
            let _ = cx.update(|app| app.quit());
            return;
        }

        accepted
            .restart_exit
            .borrow_mut()
            .replace((accepted.context, accepted.native_windows));
        let _ = cx.update(|app| app.quit());
    }

    fn run_hash_generator_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(operation) = self.restart_admission.admit() else {
            self.restart_status =
                Some("GPUI is closing; new utility operations are disabled.".to_owned());
            cx.notify();
            return;
        };
        let outcome = handle_hash_generator(&HashGeneratorCommand::Open, || {
            self.open_hash_generator_window(HashOpenIntent::Default, operation, window, cx)
        });
        let docked_focus_owned = matches!(&self.hash_window, HashWindowState::Docked { view, .. }
            if {
                let tool = view.read(cx);
                tool.focus_handle.contains_focused(window, cx)
                    || tool.editable_focus_is_exact(window, cx)
            });
        if outcome.is_ok() && hash_menu_focus_is_owned(
            self.file_menu_open || self.help_menu_open,
            self.hash_window.mode() == Some(HashPresentationMode::Docked),
            docked_focus_owned,
        ) {
            self.root_menu_action_routed_focus = true;
        }
        self.hash_open_error = outcome
            .err()
            .map(|error| format!("Could not open Hash Generator: {error}"));
        cx.notify();
    }

    fn open_toast_log_external(&mut self, cx: &mut Context<Self>) {
        if self.toast_log_external_open_pending {
            return;
        }
        let Some(operation) = self.restart_admission.admit() else {
            self.toast_log_external_open_error =
                Some("GPUI is closing; Toast Log cannot be opened.".to_owned());
            cx.notify();
            return;
        };
        self.toast_log_external_open_pending = true;
        self.toast_log_external_open_error = None;
        cx.notify();

        let task = cx
            .background_executor()
            .spawn(async move { toast_log::open_toast_log() });
        let root = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let _ = root.update(cx, |root, cx| {
                root.toast_log_external_open_pending = false;
                root.toast_log_external_open_error = result.err().map(|error| error.to_string());
                clear_toast_log_open_pending_status(&mut root.restart_status);
                cx.notify();
            });
            drop(operation);
        })
        .detach();
    }

    fn toast_log_viewer_scroll_item(&self) -> usize {
        if self.settings_editor.open {
            ROOT_SCROLL_SETTINGS_ITEM + 1
        } else {
            ROOT_SCROLL_SETTINGS_ITEM
        }
    }

    fn open_toast_log_viewer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.toast_log_viewer.open {
            if self.help_menu_open {
                self.root_menu_action_routed_focus = true;
            }
            self.root_scroll
                .scroll_to_top_of_item(self.toast_log_viewer_scroll_item());
            self.toast_log_focus_handle.focus(window, cx);
            cx.notify();
            return;
        }
        if self.restart_admission.is_fenced() {
            return;
        }

        let return_focus = self
            .root_menu_previous_focus
            .clone()
            .unwrap_or_else(|| self.query.focus_handle(cx));
        let return_scroll_offset = self.root_scroll.offset();
        let Some(receipt) = self.toast_log_viewer.open() else {
            return;
        };
        panel_stack::promote(&mut self.panel_stack, Panel::ToastLogDialog);
        if self.help_menu_open {
            self.root_menu_action_routed_focus = true;
        }
        self.toast_log_scroll.set_offset(point(px(0.0), px(0.0)));
        self.toast_log_return_focus = Some(return_focus);
        self.toast_log_return_scroll_offset = Some(return_scroll_offset);
        self.root_scroll
            .scroll_to_top_of_item(self.toast_log_viewer_scroll_item());
        self.toast_log_focus_handle.focus(window, cx);
        self.start_toast_log_read(receipt, cx);
        cx.notify();
    }

    fn refresh_toast_log(&mut self, cx: &mut Context<Self>) {
        if !self.toast_log_viewer.open {
            return;
        }
        if self.restart_admission.is_fenced() {
            self.toast_log_viewer.status =
                Some("GPUI is closing; Toast Log refresh was not started.".to_owned());
            cx.notify();
            return;
        }
        let Some(receipt) = self.toast_log_viewer.begin_read() else {
            return;
        };
        self.start_toast_log_read(receipt, cx);
        cx.notify();
    }

    fn start_toast_log_read(&self, receipt: ToastLogReadReceipt, cx: &mut Context<Self>) {
        let task = cx.background_executor().spawn(async move {
            toast_log::read_last_lines(toast_log::TOAST_LOG_FILE, toast_log::TOAST_LOG_TAIL_LINES)
        });
        let root = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let rows = task.await;
            let _ = root.update(cx, |root, cx| {
                if root.toast_log_viewer.finish_read(receipt, rows) {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn close_toast_log_viewer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        restore_origin: bool,
    ) {
        if !self.toast_log_viewer.close() {
            return;
        }
        panel_stack::retire(&mut self.panel_stack, Panel::ToastLogDialog);
        let restore_origin = restore_origin
            && self
                .toast_log_owner_focus_handle
                .contains_focused(window, cx)
            && !self.restart_admission.is_fenced();
        let return_focus = self.toast_log_return_focus.take();
        let return_scroll_offset = self.toast_log_return_scroll_offset.take();
        if restore_origin {
            let return_focus_is_live = return_focus
                .as_ref()
                .is_some_and(|focus| self.focus_handle.contains(focus, window));
            if return_focus_is_live {
                if let Some(offset) = return_scroll_offset {
                    self.root_scroll.set_offset(offset);
                }
                if let Some(focus) = return_focus {
                    focus.focus(window, cx);
                }
            } else {
                self.root_scroll
                    .scroll_to_top_of_item(ROOT_SCROLL_SEARCH_ITEM);
                self.query.focus_handle(cx).focus(window, cx);
            }
        }
        self.toast_log_return_focus = None;
        self.toast_log_return_scroll_offset = None;
        cx.notify();
    }

    fn quick_help_scroll_item(&self) -> usize {
        quick_help_root_scroll_item_index(
            self.settings_editor.open,
            self.toast_log_viewer.open,
            self.command_help.open,
            self.hash_window.mode() == Some(HashPresentationMode::Docked),
        )
    }

    fn docked_hash_scroll_item(&self) -> usize {
        docked_hash_root_scroll_item_index(
            self.settings_editor.open,
            self.toast_log_viewer.open,
            self.command_help.open,
        )
    }

    fn quick_help_focus_change_allowed(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.action_sheet.is_some()
            || self.context_favorite_popup.is_some()
            || self.result_favorites.owns_focus(window, cx)
            || self.file_search.as_ref().is_some_and(|view| view.read(cx).owns_focus(window, cx))
            || self.folder_size_view.as_ref().is_some_and(|view| view.read(cx).owns_focus(window, cx))
            || self.file_lock_view.as_ref().is_some_and(|view| view.read(cx).owns_focus(window, cx))
            || self.downloads_triage_owns_focus(window, cx)
            || self.plugin_editor.as_ref().is_some_and(|view| view.read(cx).owns_focus(window, cx))
        {
            return false;
        }
        let contexts = window.context_stack();
        let docked_hash_owner_has_focus = matches!(
            &self.hash_window,
            HashWindowState::Docked { view, .. }
                if view.read(cx).focus_handle.contains_focused(window, cx)
        );
        let focus_blockers = QuickHelpFocusBlockers {
            menu_open: self.file_menu_open || self.settings_menu_open || self.help_menu_open,
            popup_or_alert_owns_focus: context_stack_has_primary(&contexts, "PopupMenu")
                || context_stack_has_primary(&contexts, "AlertDialog")
                || context_stack_has_primary(&contexts, "Dialog"),
            settings_owner_has_focus: self
                .settings_owner_focus_handle
                .contains_focused(window, cx),
            toast_log_owner_has_focus: self
                .toast_log_owner_focus_handle
                .contains_focused(window, cx),
            command_help_owner_has_focus: self
                .command_help_owner_focus_handle
                .contains_focused(window, cx),
            docked_hash_owner_has_focus,
            hash_resize_owner_has_focus: self.resize_focus_handle.is_focused(window),
        };
        #[cfg(windows)]
        let root_is_foreground = self
            .native_window
            .is_some_and(|native| current_foreground_identity() == Some(native.identity));
        #[cfg(not(windows))]
        let root_is_foreground = true;
        quick_help_focus_change_is_allowed(
            root_is_foreground,
            window.is_window_active(),
            focus_blockers,
            self.restart_admission.is_fenced(),
        )
    }

    fn open_quick_help(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        origin: QuickHelpOpenOrigin,
        take_focus: bool,
    ) {
        if self.restart_admission.is_fenced() {
            return;
        }
        self.apply_pending_help_filter_sync(window, cx);
        if self.quick_help.open {
            if take_focus && origin == QuickHelpOpenOrigin::Menu && self.help_menu_open {
                self.root_menu_action_routed_focus = true;
            }
            if take_focus {
                self.root_scroll
                    .scroll_to_top_of_item(self.quick_help_scroll_item());
                self.quick_help_filter
                    .focus_handle(cx)
                    .focus(window, cx);
            }
            cx.notify();
            return;
        }

        let return_focus = match origin {
            QuickHelpOpenOrigin::Menu => self
                .root_menu_previous_focus
                .clone()
                .filter(|focus| self.focus_handle.contains(focus, window))
                .or_else(|| Some(self.query.focus_handle(cx))),
            QuickHelpOpenOrigin::Shortcut => window
                .focused(cx)
                .filter(|focus| self.focus_handle.contains(focus, window)),
        };
        self.quick_help.open = true;
        panel_stack::promote(&mut self.panel_stack, Panel::HelpOverlay);
        self.fresh_search_enter = None;
        self.quick_help_return_focus = return_focus;
        self.quick_help_return_scroll_offset = Some(self.root_scroll.offset());
        if take_focus && origin == QuickHelpOpenOrigin::Menu && self.help_menu_open {
            self.root_menu_action_routed_focus = true;
        }
        if take_focus {
            self.root_scroll
                .scroll_to_top_of_item(self.quick_help_scroll_item());
            self.quick_help_filter
                .focus_handle(cx)
                .focus(window, cx);
        }
        cx.notify();
    }

    fn toggle_quick_help_from_shortcut(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        take_focus: bool,
    ) {
        if self.restart_admission.is_fenced() {
            return;
        }
        if self.quick_help.open {
            self.close_quick_help(window, cx, take_focus);
        } else {
            self.open_quick_help(window, cx, QuickHelpOpenOrigin::Shortcut, take_focus);
        }
    }

    fn close_quick_help(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        restore_origin: bool,
    ) {
        if !self.quick_help.open {
            return;
        }
        let restore_origin = restore_origin
            && self
                .quick_help_owner_focus_handle
                .contains_focused(window, cx)
            && self.quick_help_focus_change_allowed(window, cx);
        self.quick_help.open = false;
        panel_stack::retire(&mut self.panel_stack, Panel::HelpOverlay);
        let return_focus = self.quick_help_return_focus.take();
        let return_scroll_offset = self.quick_help_return_scroll_offset.take();
        if restore_origin {
            let return_focus_is_live = return_focus
                .as_ref()
                .is_some_and(|focus| self.focus_handle.contains(focus, window));
            if return_focus_is_live {
                if let Some(offset) = return_scroll_offset {
                    self.root_scroll.set_offset(offset);
                }
                if let Some(focus) = return_focus {
                    focus.focus(window, cx);
                }
            } else {
                self.root_scroll
                    .scroll_to_top_of_item(ROOT_SCROLL_SEARCH_ITEM);
                self.query.focus_handle(cx).focus(window, cx);
            }
        }
        cx.notify();
    }

    fn command_help_scroll_item(&self) -> usize {
        ROOT_SCROLL_SETTINGS_ITEM
            + usize::from(self.settings_editor.open)
            + usize::from(self.toast_log_viewer.open)
    }

    fn open_command_help(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        origin: CommandHelpOpenOrigin,
        filter_seed: Option<&str>,
    ) {
        if self.restart_admission.is_fenced() {
            self.restart_status =
                Some("GPUI is closing; Command Help can no longer be opened.".to_owned());
            cx.notify();
            return;
        }

        self.apply_pending_help_filter_sync(window, cx);
        if let Some(filter) = filter_seed {
            self.seed_shared_help_filter(filter, window, cx);
        }
        if self.command_help.open {
            if origin == CommandHelpOpenOrigin::Menu && self.help_menu_open {
                self.root_menu_action_routed_focus = true;
            }
            self.root_scroll
                .scroll_to_top_of_item(self.command_help_scroll_item());
            self.command_help_filter
                .focus_handle(cx)
                .focus(window, cx);
            cx.notify();
            return;
        }

        let return_focus = match origin {
            CommandHelpOpenOrigin::Search => window
                .focused(cx)
                .filter(|focus| self.focus_handle.contains(focus, window))
                .unwrap_or_else(|| self.query.focus_handle(cx)),
            CommandHelpOpenOrigin::Menu => self
                .root_menu_previous_focus
                .clone()
                .filter(|focus| self.focus_handle.contains(focus, window))
                .unwrap_or_else(|| self.query.focus_handle(cx)),
        };
        self.command_help.open = true;
        panel_stack::promote(&mut self.panel_stack, Panel::HelpWindow);
        self.command_help.status = None;
        self.command_help_return_focus = Some(return_focus);
        self.command_help_return_scroll_offset = Some(self.root_scroll.offset());
        if origin == CommandHelpOpenOrigin::Menu && self.help_menu_open {
            self.root_menu_action_routed_focus = true;
        }
        self.fresh_search_enter = None;
        self.root_scroll
            .scroll_to_top_of_item(self.command_help_scroll_item());
        self.command_help_filter
            .focus_handle(cx)
            .focus(window, cx);
        cx.notify();
    }

    fn close_command_help(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        restore_origin: bool,
    ) {
        if !self.command_help.open {
            return;
        }
        if self.command_help.pending.is_some() {
            self.command_help.status = Some(
                "Wait for the Show examples save to finish before closing Command Help."
                    .to_owned(),
            );
            self.command_help_filter
                .focus_handle(cx)
                .focus(window, cx);
            cx.notify();
            return;
        }
        let restore_origin = restore_origin
            && self
                .command_help_owner_focus_handle
                .contains_focused(window, cx)
            && !self.restart_admission.is_fenced();
        if !self.command_help.cancel_idle() {
            return;
        }
        panel_stack::retire(&mut self.panel_stack, Panel::HelpWindow);
        let return_focus = self.command_help_return_focus.take();
        let return_scroll_offset = self.command_help_return_scroll_offset.take();
        if restore_origin {
            let return_focus_is_live = return_focus
                .as_ref()
                .is_some_and(|focus| self.focus_handle.contains(focus, window));
            if return_focus_is_live {
                if let Some(offset) = return_scroll_offset {
                    self.root_scroll.set_offset(offset);
                }
                if let Some(focus) = return_focus {
                    focus.focus(window, cx);
                }
            } else {
                self.root_scroll
                    .scroll_to_top_of_item(ROOT_SCROLL_SEARCH_ITEM);
                self.query.focus_handle(cx).focus(window, cx);
            }
        } else {
            self.command_help_return_focus = None;
            self.command_help_return_scroll_offset = None;
        }
        cx.notify();
    }

    fn begin_command_help_save(&mut self, desired: bool, cx: &mut Context<Self>) {
        if !self.command_help.open {
            return;
        }
        let Some(operation) = self.restart_admission.admit() else {
            self.command_help.status =
                Some("GPUI is closing; Show examples cannot be saved.".to_owned());
            cx.notify();
            return;
        };
        let Some(attempt) = self.command_help.begin_save(desired) else {
            drop(operation);
            return;
        };
        let request = multi_launcher::settings::SettingsBooleanSaveRequest::new(
            multi_launcher::settings::SettingsBooleanPreference::ShowExamples,
            attempt.expected_baseline,
            attempt.desired,
        );
        let (receiver, token) = match self
            .settings_save_workers
            .begin_boolean_preference_save(self.settings_path.clone(), request)
        {
            Ok(pending) => pending,
            Err(error) => {
                self.command_help.fail_to_start(
                    attempt.id,
                    format!("Could not start Show examples save: {error}"),
                );
                drop(operation);
                cx.notify();
                return;
            }
        };
        self.command_help_save_permit = Some(operation);
        self.command_help_save_token = Some(token);
        cx.notify();

        let root = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let result = loop {
                match receiver.try_recv() {
                    Ok(result) => break result,
                    Err(TryRecvError::Disconnected) => {
                        break Err(multi_launcher::settings::SettingsBooleanSaveError::Store(
                            "Settings writer ended without an acknowledgement.".into(),
                        ));
                    }
                    Err(TryRecvError::Empty) => {
                        cx.background_executor()
                            .timer(Duration::from_millis(20))
                            .await;
                    }
                }
            };
            let _ = root.update(cx, |root, cx| {
                root.finish_command_help_save(attempt.id, result, cx);
            });
        })
        .detach();
    }

    fn finish_command_help_save(
        &mut self,
        request_id: u64,
        result: Result<
            multi_launcher::settings::SettingsBooleanSaveAcknowledgement,
            multi_launcher::settings::SettingsBooleanSaveError,
        >,
        cx: &mut Context<Self>,
    ) {
        if !self
            .command_help
            .pending
            .is_some_and(|pending| pending.id == request_id)
        {
            return;
        }
        let canceled = matches!(&result, Err(multi_launcher::settings::SettingsBooleanSaveError::Cancelled));
        let failed = result.is_err() && !canceled;
        self.command_help.finish_save(request_id, result);
        if let Some(token) = self.command_help_save_token.take() {
            self.settings_save_workers
                .acknowledge_boolean_preference_save(token.id());
        }
        self.command_help_save_permit.take();
        clear_stale_settings_restart_status(
            &mut self.restart_status,
            self.settings_search.as_ref().is_none_or(|view| !view.read(cx).restart_draft(cx).dirty())
                && self.settings_actions.as_ref().is_none_or(|view| !view.read(cx).restart_draft(cx).dirty())
                && self.command_help.pending.is_none(),
        );
        self.finish_deferred_preference_close(failed, cx);
        cx.notify();
    }

    fn finish_deferred_preference_close(&mut self, failed: bool, cx: &mut Context<Self>) {
        if !self.preference_close_after_terminal {
            return;
        }
        self.preference_close_failed |= failed;
        let another_pending = self.settings_search.as_ref().is_some_and(|view| view.read(cx).pending_token().is_some()) || self.command_help.pending.is_some();
        if another_pending {
            return;
        }
        self.preference_close_after_terminal = false;
        let close_failed = std::mem::take(&mut self.preference_close_failed);
        if close_failed {
            if self.restart_status.as_deref() == Some(PREFERENCE_COMMIT_CLOSE_STATUS) {
                self.restart_status = None;
            }
        } else {
            self.request_quit(cx);
        }
    }

    fn prepare_boolean_preference_close(&mut self, cx: &mut Context<Self>) -> bool {
        let mut pending = false;
        let mut cancellation_accepted = true;
        if let Some(token) = self.settings_search.as_ref().and_then(|view| view.read(cx).pending_token()) {
            pending = true;
            cancellation_accepted &= token.request_cancel();
        }
        if self.command_help.pending.is_some() {
            pending = true;
            cancellation_accepted &= self
                .command_help_save_token
                .as_ref()
                .is_some_and(|token| token.request_cancel());
        }
        if !pending || cancellation_accepted {
            return true;
        }
        self.preference_close_after_terminal = true;
        self.preference_close_failed = false;
        self.restart_status = Some(PREFERENCE_COMMIT_CLOSE_STATUS.to_owned());
        cx.notify();
        false
    }

    fn open_settings_editor(&mut self, window: &mut Window, cx: &mut Context<Self>, origin: SettingsOpenOrigin) {
        if self.settings_editor.open {
            self.root_scroll.scroll_to_top_of_item(ROOT_SCROLL_SETTINGS_ITEM);
            self.settings_focus_handle.focus(window, cx); cx.notify(); return;
        }
        if self.restart_admission.is_fenced() { self.restart_status = Some("GPUI is closing; Settings can no longer be opened.".into()); cx.notify(); return; }
        let return_focus = match origin { SettingsOpenOrigin::Search => self.query.focus_handle(cx),
            SettingsOpenOrigin::Menu => self.root_menu_previous_focus.clone().unwrap_or_else(|| self.settings_menu_focus_handle.clone()) };
        self.settings_editor.open = true; self.settings_editor.status = None;
        panel_stack::promote(&mut self.panel_stack, Panel::Settings);
        if origin == SettingsOpenOrigin::Menu && self.settings_menu_open { self.root_menu_action_routed_focus = true; }
        self.settings_return_focus = Some(return_focus); self.settings_return_scroll_offset = Some(self.root_scroll.offset());
        self.fresh_search_enter = None; self.root_scroll.scroll_to_top_of_item(ROOT_SCROLL_SETTINGS_ITEM);
        self.open_search_settings(window, cx);
        self.open_actions_settings(window, cx);
        let root = cx.entity().downgrade();
        let search = self.settings_search.clone();
        let actions = self.settings_actions.clone();
        self.settings_navigation = Some(cx.new(|cx| settings_navigation::SettingsNavigation::new(root, search, actions, window, cx)));
        self.settings_panel_scroll.set_offset(point(px(0.0), px(0.0)));
        self.settings_focus_handle.focus(window, cx); cx.notify();
    }

    fn settings_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.settings_editor.open || !self.settings_owner_focus_handle.contains_focused(window, cx) { return false; }
        if self.pane_escape_latched { return true; }
        let cleared = self.settings_navigation.clone().is_some_and(|view| view.update(cx, |view, cx| view.clear(window, cx)));
        if cleared { self.pane_escape_latched = true; }
        else { self.cancel_settings_editor(window, cx, true); }
        true
    }

    fn toggle_settings_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_editor.open {
            if self.settings_actions.as_ref().is_some_and(|view| view.read(cx).busy()) {
                self.settings_editor.status = Some(settings_actions::WAIT.into()); cx.notify(); return;
            }
            if self.settings_search.as_ref().is_some_and(|view| view.read(cx).busy()) {
                self.settings_editor.status = Some(settings_search::WAIT.into()); cx.notify(); return;
            }
            let settings_owned_focus = settings_return_focus_is_still_owned(
                self.settings_owner_focus_handle.contains_focused(window, cx),
                self.restart_admission.is_fenced(),
            );
            let menu_origin_was_settings = self
                .root_menu_previous_focus
                .as_ref()
                .is_some_and(|focus| self.settings_owner_focus_handle.contains(focus, window));
            let focus_settings_menu_after_close = settings_menu_close_needs_trigger_focus(
                settings_owned_focus,
                self.settings_menu_open,
                menu_origin_was_settings,
            );
            self.cancel_settings_editor(window, cx, false);
            if !self.settings_editor.open && focus_settings_menu_after_close {
                self.root_scroll
                    .scroll_to_top_of_item(ROOT_SCROLL_SEARCH_ITEM);
                self.settings_menu_focus_handle.focus(window, cx);
            }
        } else {
            self.open_settings_editor(window, cx, SettingsOpenOrigin::Menu);
        }
    }

    fn cancel_settings_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        restore_origin: bool,
    ) {
        if !self.settings_editor.open {
            return;
        }
        let restore_origin = restore_origin
            && settings_return_focus_is_still_owned(
                self.settings_owner_focus_handle.contains_focused(window, cx),
                self.restart_admission.is_fenced(),
            );
        if self.settings_actions.as_ref().is_some_and(|view| view.read(cx).busy()) {
            self.settings_editor.status = Some(settings_actions::WAIT.into()); cx.notify(); return;
        }
        if self.settings_search.as_ref().is_some_and(|view| view.read(cx).busy()) {
            self.settings_editor.status = Some(settings_search::WAIT.into()); cx.notify(); return;
        }
        self.settings_editor.open = false;
        self.settings_editor.status = None;
        if let Some(view) = self.settings_navigation.take() { view.update(cx, |view, _| view.retire()); }
        if let Some(view) = self.settings_search.take() { view.update(cx, |view, _| view.retire()); }
        if let Some(view) = self.settings_actions.take() { view.update(cx, |view, _| view.retire()); }
        panel_stack::retire(&mut self.panel_stack, Panel::Settings);
        clear_stale_settings_restart_status(&mut self.restart_status, true);
        let return_focus = self.settings_return_focus.take();
        let return_scroll_offset = self.settings_return_scroll_offset.take();
        if restore_origin {
            let return_focus_is_live = return_focus
                .as_ref()
                .is_some_and(|focus| self.focus_handle.contains(focus, window));
            if return_focus_is_live {
                if let Some(offset) = return_scroll_offset {
                    self.root_scroll.set_offset(offset);
                }
            } else {
                self.root_scroll
                    .scroll_to_top_of_item(ROOT_SCROLL_SEARCH_ITEM);
            }
            if return_focus_is_live {
                if let Some(focus) = return_focus {
                    focus.focus(window, cx);
                }
            } else {
                self.query.focus_handle(cx).focus(window, cx);
            }
        } else {
            self.settings_return_focus = None;
            self.settings_return_scroll_offset = None;
        }
        cx.notify();
    }

    fn refresh_launcher_results(&mut self, query: String, cx: &mut Context<Self>) {
        self.refresh_launcher_results_with_viewport(query, true, cx);
    }

    fn refresh_launcher_results_with_viewport(&mut self, query: String, scroll_to_search: bool, cx: &mut Context<Self>) {
        self.external_launch.retire_clipboard_effect();
        self.snippets_search.request_epoch.fetch_add(1, Ordering::AcqRel);
        self.snippets_search.projection = None;
        self.snippets_search.published = None;
        self.search_catalog.providers.take_snippets_projection();
        self.clipboard_modify_startup.projection = None;
        self.clipboard_modify_startup.published = None;
        self.search_catalog.providers.take_clipboard_modify_projection();
        self.retire_context_favorite();
        self.retire_search_jump_mode();
        self.calculator_search.pending = None;
        self.calculator_search.retire_history();
        self.window_search.retire();
        self.file_search_popup_intent.store(false, Ordering::Release); self.file_search_probe.retire();
        if query.trim().is_empty() {
            self.publish_launcher_provider_results(query, Vec::new(), scroll_to_search);
            return;
        }
        let raw_provider_actions = search_providers::search_routed(
            self.search_catalog.providers.plugins(),
            &query,
            self.enabled_plugins.as_ref(),
            self.enabled_capabilities.as_ref(),
        );
        self.clipboard_modify_startup.projection = self.search_catalog.providers
            .take_clipboard_modify_projection().map(|(effective, catalog)| (query.clone(), effective, catalog));
        self.snippets_search.projection = self.search_catalog.providers.take_snippets_projection()
            .map(|(effective, snapshot)| (query.clone(), effective, snapshot));
        if file_search::typed_query(&query) && file_search::enabled(self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref()) {
            self.start_file_search_probe(query, raw_provider_actions, scroll_to_search, cx);
            return;
        }
        if window_search::query_is_windows(&query) && window_search::enabled(
            self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref(),
        ) {
            self.start_window_search(query, raw_provider_actions, scroll_to_search, cx);
            return;
        }
        let calculator_enabled = calculator_search::eligible_query(&query)
            && self.enabled_plugins.as_ref().is_none_or(|enabled| enabled.contains("calculator"))
            && multi_launcher::settings::provider_search_capability_enabled(self.enabled_capabilities.as_ref(), "calculator");
        if calculator_enabled {
            let layout = self.search_catalog.resolve_result_layout(&query, self.enabled_plugins.as_ref());
            self.launcher_results.replace(query.clone(), Vec::new(), layout, self.search_catalog.page_jump);
            self.fresh_search_enter = None;
            self.launcher_results_scroll = ScrollHandle::new();
            if scroll_to_search { self.root_scroll.scroll_to_top_of_item(ROOT_SCROLL_SEARCH_ITEM); }
            let names: Vec<_> = self.search_catalog.providers.plugins().map(|plugin| plugin.name().to_owned()).collect();
            let calculator_index = names.iter().position(|name| name == "calculator").unwrap_or(names.len());
            let insert_at = raw_provider_actions.iter().position(|(name, _)| {
                names.iter().position(|candidate| candidate == name).is_some_and(|index| index > calculator_index)
            }).unwrap_or(raw_provider_actions.len());
            self.calculator_search.pending = Some(calculator_search::PendingSearch {
                query, generation: self.launcher_results.generation,
                other_rows: raw_provider_actions, insert_at,
            });
            self.start_pending_calculator_search(cx);
            return;
        }
        self.publish_launcher_provider_results(query, raw_provider_actions, scroll_to_search);
    }

    fn publish_launcher_provider_results(&mut self, query: String, raw_provider_actions: Vec<(String, Action)>, scroll_to_search: bool) {
        let raw_provider_results_empty = raw_provider_actions.is_empty();
        let provider_actions = raw_provider_actions
            .into_iter()
            .filter(|(provider, action)| {
                if provider == "windows" {
                    window_search::action_is_operation(provider, action)
                } else if provider == "calculator" {
                    calculator_search::classify(provider, &query, action).is_some()
                        || calculator_search::history_index(provider, &query, action).is_some()
                } else if provider == "date_arithmetic" {
                    date_search::classify(provider, action).is_some()
                } else if provider == "base_convert" {
                    base_convert_search::classify(provider, action).is_some()
                } else if provider == "unit_convert" {
                    unit_convert_search::classify(provider, action).is_some()
                } else if provider == "clipboard_modify" {
                    clipboard_modify_view::current_provider(&query, action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref())
                        || clipboard_modify_view::current_undo_provider(&query, action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref())
                        || (self.enabled_plugins.as_ref().is_none_or(|enabled| enabled.contains("clipboard_modify"))
                            && multi_launcher::settings::provider_search_capability_enabled(self.enabled_capabilities.as_ref(), "clipboard_modify")
                            && self.clipboard_modify_startup.projection.as_ref().is_some_and(|(projected, _, catalog)| {
                                projected == &query && (action.action == "clipboard_modify:execute" || action.action == "clipboard_modify:undo" || action.action.starts_with("clipboard_modify:open:") || action.action.starts_with("query:"))
                                    && search_providers::clipboard_catalog_is_current(&self.search_catalog.providers.clipboard_modify_catalog(), catalog)
                            }))
                } else if provider == "clipboard" {
                    clipboard_history::current_provider(&query, action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref())
                } else if provider == "tempfile" {
                    tempfile_view::current_provider(&query, action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref())
                } else if provider == "snippets" {
                    snippets::current_provider(&query, action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref())
                        || self.snippets_search.projection.as_ref().is_some_and(|(raw, _, snapshot)| {
                            raw == &query && snippets::named_row_is_current(&query, action, snapshot,
                                self.search_catalog.providers.snippets_owner().as_deref(), self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref())
                        })
                } else if provider == "favorites" {
                    favorite_search::is_executable(action) || is_supported_launcher_search_action(action)
                } else if external_launch::AssociationKind::from_provider_name(provider).is_some() {
                    external_launch::provider_association_by_name(provider, action).is_some()
                } else if cfg!(windows) && provider == "downloads_triage" {
                    is_downloads_triage_opener(action)
                } else if provider == "settings" && action.action == "settings:search" {
                    settings_navigation::current_provider(&query, action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref()).is_some()
                } else if provider == "file_lock" {
                    cfg!(windows) && file_lock::current_provider(&query, action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref())
                } else if provider == "folder_size" {
                    folder_size::current_provider(&query, action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref())
                } else if provider == "file_search" {
                    is_file_search_opener(&query, action) || multi_launcher::plugins::file_search::scoped_search_start(&query, action).is_some()
                } else {
                    is_supported_launcher_search_action(action)
                }
            })
            .collect();
        let mut actions =
            self.search_catalog
                .compose(&query, provider_actions, raw_provider_results_empty);
        let layout = self
            .search_catalog
            .resolve_result_layout(&query, self.enabled_plugins.as_ref());
        let show_full_path = self
            .enabled_capabilities
            .as_ref()
            .and_then(|caps| caps.get("folders"))
            .is_some_and(|caps| caps.iter().any(|cap| cap == "show_full_path"));
        if !layout.grid && !show_full_path && actions.iter().any(|row| matches!(&row.origin, LauncherResultOrigin::Provider(name) if name == "folders")) {
            let aliases = self.search_catalog.providers.aliased_folder_rows();
            decorate_folder_descriptions(&mut actions, layout.grid, show_full_path, &aliases);
        }
        self.launcher_results
            .replace(query.clone(), actions, layout, self.search_catalog.page_jump);
        self.clipboard_modify_startup.published = self.clipboard_modify_startup.projection.take()
            .filter(|(projected, _, catalog)| projected == &query
                && self.search_catalog.providers.clipboard_modify_catalog().read().ok()
                    .is_some_and(|current| Arc::ptr_eq(&current, catalog)))
            .map(|(query, effective, catalog)| (query, effective, self.launcher_results.generation, catalog));
        self.snippets_search.published = self.snippets_search.projection.take()
            .filter(|(raw, _, snapshot)| raw == &query && self.search_catalog.providers.snippets_owner().is_some_and(|owner| snippets::search_snapshot_current(&owner, snapshot)))
            .map(|(raw, effective, snapshot)| (raw, effective, self.launcher_results.generation, snapshot));
        self.fresh_search_enter = None;
        self.launcher_results_scroll = ScrollHandle::new();
        // Query replacement can change the list height while the root is
        // scrolled near the end. Keep the Search field at the leading edge
        // whenever a tall result set overflows the shared root scroller.
        if scroll_to_search {
            self.root_scroll
                .scroll_to_top_of_item(ROOT_SCROLL_SEARCH_ITEM);
        }
    }

    fn publish_saved_search_actions(&mut self, rows: Vec<Action>, verified_folders: Vec<Action>, cx: &mut Context<Self>) -> bool {
        if self.search_catalog.replace_saved_actions(rows, verified_folders) {
            let query = self.query.read(cx).value().to_string();
            self.search_completion.reset_for_query(&self.search_catalog, &query);
            self.refresh_launcher_results_with_viewport(query, false, cx);
            true
        } else {
            false
        }
    }

    fn record_search_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.fresh_search_enter = fresh_search_enter_marker(
            self.search_input_is_live(window, cx),
            event.keystroke.key.as_str(),
            event.keystroke.modifiers,
            event.is_held,
            self.launcher_results.generation,
            self.query.read(cx).value().to_string(),
            self.search_jump_mode.revision(),
        );
    }

    fn record_search_key_up(&mut self, event: &KeyUpEvent) {
        if event.keystroke.key == "enter" {
            self.fresh_search_enter = None;
        }
    }

    fn retire_search_jump_mode(&mut self) -> bool {
        let retired = self.search_jump_mode.retire();
        if retired {
            self.fresh_search_enter = None;
        }
        retired
    }

    fn reconcile_search_jump_geometry_frame(
        &mut self,
        frame: &JumpGeometryFrame,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let retired = if self.search_input_is_live(window, cx) {
            self.search_jump_mode.reconcile_frame(Some(frame))
        } else {
            self.search_jump_mode.retire()
        };
        if retired {
            self.fresh_search_enter = None;
        }
        retired
    }

    fn handle_search_jump_keydown(
        &mut self,
        receipt: PhysicalKeydown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.search_jump_mode.should_process_key_down(receipt) {
            return false;
        }
        let Some(operation) = self.search_input_operation(window, cx) else {
            let retired = self.search_jump_mode.retire();
            if retired {
                self.fresh_search_enter = None;
                cx.notify();
            }
            return false;
        };

        let query_is_current = self.launcher_results.query
            == self.query.read(cx).value().to_string();
        let geometry_slot = self.search_jump_geometry_frame.borrow();
        let frame = query_is_current
            .then_some(geometry_slot.as_ref())
            .flatten();
        let retired = self.search_jump_mode.reconcile_frame(frame);
        if retired {
            self.fresh_search_enter = None;
        }
        let result = self.search_jump_mode.process_key_down(
            receipt,
            frame,
            self.launcher_results.generation,
        );
        if result.session_transition {
            self.fresh_search_enter = None;
        }
        drop(operation);

        let selected_result_index = result.selected_target.as_ref().and_then(|target| {
            let target_is_current = frame.is_some_and(|frame| {
                frame.generation == self.launcher_results.generation
                    && target.generation() == TargetGeneration(frame.generation)
                    && frame.has_target(target)
            });
            if target_is_current {
                if let SemanticTargetId::RootResult(index) = target.semantic() {
                    return Some(*index);
                }
            }
            None
        });
        drop(geometry_slot);

        if let Some(index) = selected_result_index {
            self.select_search_result_exact(
                self.launcher_results.generation,
                index,
                window,
                cx,
            );
        }

        if retired || result.repaint || result.session_transition || result.selected_target.is_some()
        {
            cx.notify();
        }
        result.consumed
    }

    fn search_input_operation(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<EffectPermit> {
        if !self.search_input_is_live(window, cx) {
            return None;
        }
        self.restart_admission.admit()
    }

    fn search_input_is_live(&self, window: &Window, cx: &mut Context<Self>) -> bool {
        let root_window_matches = self
            .root_window
            .is_some_and(|root| root.window_id() == window.window_handle().window_id());
        search_input_is_current(
            self.root_visibility.visible.load(Ordering::Acquire),
            self.root_visibility.active.load(Ordering::Acquire),
            root_window_matches,
            self.query.focus_handle(cx).is_focused(window),
            window.is_window_active(),
        )
    }

    fn launcher_result_activation_is_live(
        &self,
        origin: LauncherResultActivationOrigin,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let root_window_matches = self
            .root_window
            .is_some_and(|root| root.window_id() == window.window_handle().window_id());
        launcher_result_activation_is_current(
            self.root_visibility.visible.load(Ordering::Acquire),
            self.root_visibility.active.load(Ordering::Acquire),
            root_window_matches,
            self.query.focus_handle(cx).is_focused(window),
            window.is_window_active(),
            origin,
        )
    }

    fn accept_search_completion(
        &mut self,
        require_focus_session: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.search_catalog.query_autocomplete {
            return false;
        }
        let Some(_operation) = self.search_input_operation(window, cx) else {
            return false;
        };
        let visibility_revision = self
            .root_visibility
            .request_revision
            .load(Ordering::Acquire);
        if require_focus_session
            && !search_completion_session_is_current(
                self.search_completion_focus_revision,
                visibility_revision,
            )
        {
            return false;
        }

        let current_query = self.query.read(cx).value().to_string();
        if self.launcher_results.query != current_query {
            return false;
        }
        let Some(completed_query) = self.search_completion.begin_replacement(&current_query) else {
            return false;
        };
        self.query.update(cx, |state, cx| {
            state.replace_all(completed_query, window, cx);
        });
        true
    }

    fn record_search_completion_pointer_press(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.search_input_operation(window, cx).is_some() {
            self.search_completion_focus_revision = Some(
                self.root_visibility
                    .request_revision
                    .load(Ordering::Acquire),
            );
        }
    }

    fn revoke_search_completion_focus(&mut self) {
        self.search_completion_focus_revision = None;
    }

    fn grant_search_completion_focus_after_show(
        &mut self,
        revision: u64,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.root_visibility.request_revision.load(Ordering::Acquire) != revision
            || !self.root_visibility.visible.load(Ordering::Acquire)
            || self.restart_admission.admit().is_none()
            || !self.search_input_is_live(window, cx)
        {
            return false;
        }
        self.search_completion_focus_revision = Some(revision);
        true
    }

    fn navigate_search_history(
        &mut self,
        direction: QueryHistoryDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(_operation) = self.search_input_operation(window, cx) else {
            return false;
        };
        let current_query = self.query.read(cx).value().to_string();
        self.query_history.synchronize(&current_query);
        let next_query = match direction {
            QueryHistoryDirection::Older => self.query_history.older(&current_query, || {
                multi_launcher::history::with_history(|entries| {
                    single_line_history_queries(entries.iter().map(|entry| entry.query.as_str()))
                })
                .unwrap_or_default()
            }),
            QueryHistoryDirection::Newer => self.query_history.newer(&current_query),
        };
        if let Some(next_query) = next_query {
            self.query.update(cx, |state, cx| {
                state.replace_all(next_query, window, cx);
            });
        }
        true
    }

    fn move_search_selection(
        &mut self,
        movement: QuerySelectionMove,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.move_search_selection_inner(movement, false, window, cx)
    }

    fn move_search_keypad_selection(
        &mut self,
        movement: QuerySelectionMove,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.move_search_selection_inner(movement, true, window, cx)
    }

    fn move_search_selection_inner(
        &mut self,
        movement: QuerySelectionMove,
        physical_keypad: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(_operation) = self.search_input_operation(window, cx) else {
            return false;
        };
        if self.launcher_results.query != self.query.read(cx).value().to_string() {
            return false;
        }
        if matches!(
            movement,
            QuerySelectionMove::Left | QuerySelectionMove::Right
        ) && !self.launcher_results.can_capture_lateral_move()
            && !physical_keypad
        {
            return false;
        }

        let changed = if physical_keypad {
            self.launcher_results.move_keypad_selection(movement)
        } else {
            self.launcher_results.move_selection(movement)
        };
        if changed && let Some(row) = self.launcher_results.selected_scroll_child() {
            self.root_scroll.scroll_to_top_of_item(ROOT_SCROLL_SEARCH_ITEM);
            self.launcher_results_scroll.scroll_to_item(row);
        }
        cx.notify();
        true
    }

    fn retire_context_favorite(&mut self) {
        self.context_favorite_popup = None;
        self.context_favorite_serial = self.context_favorite_serial.wrapping_add(1);
    }

    fn context_favorite_is_live(
        &self,
        receipt: &ContextFavoriteReceipt,
        window: &Window,
        cx: &Context<Self>,
    ) -> bool {
        #[cfg(windows)]
        if !self.native_window.is_some_and(|native| {
            self.root_visibility.is_current_visible_revision(receipt.visibility_revision, native)
        }) {
            return false;
        }
        let contexts = window.context_stack();
        let competing_popup = self.context_favorite_popup.as_ref()
            .and_then(|popup| popup.menu.as_ref())
            .is_some_and(|menu| {
                context_stack_has_primary(&contexts, "PopupMenu")
                    && !menu.focus_handle(cx).contains_focused(window, cx)
            });
        self.root_visibility.visible.load(Ordering::Acquire)
            && self.root_visibility.active.load(Ordering::Acquire)
            && window.is_window_active()
            && !self.restart_admission.is_fenced()
            && !self.file_menu_open
            && !self.settings_menu_open
            && !self.help_menu_open
            && !competing_popup
            && !context_stack_has_primary(&contexts, "AlertDialog")
            && !context_stack_has_primary(&contexts, "Dialog")
            && receipt.matches(
                &self.launcher_results,
                self.query.read(cx).value().as_str(),
                self.root_visibility.request_revision.load(Ordering::Acquire),
            )
    }

    fn open_context_favorite(
        &mut self,
        generation: u64,
        row: usize,
        rendered_action: &Action,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let query = self.query.read(cx).value().to_string();
        let Some(result) = self
            .launcher_results
            .resolve_result(generation, row, &query)
        else {
            return;
        };
        if matches!(&result.origin, LauncherResultOrigin::Provider(name) if external_launch::AssociationKind::from_provider_name(name).is_some())
        {
            return;
        }
        let indexed = matches!(result.origin, LauncherResultOrigin::Indexed { .. });
        let action = result.action;
        if &action != rendered_action {
            return;
        }
        let previous_focus = self
            .context_favorite_popup
            .as_ref()
            .and_then(|popup| popup.receipt.previous_focus.clone())
            .or_else(|| window.focused(cx).filter(|focus| self.focus_handle.contains(focus, window)));
        self.retire_context_favorite();
        cx.notify();
        let receipt = ContextFavoriteReceipt {
            serial: self.context_favorite_serial,
            generation,
            row,
            invocation: UniversalActionInvocationContext {
                history_query: query,
                root_policy: RootLauncherPolicy::PreserveOrdinaryState,
                ..UniversalActionInvocationContext::legacy(ActionSurface::ContextMenu, ActivationSource::Click)
            },
            action,
            visibility_revision: self.root_visibility.request_revision.load(Ordering::Acquire),
            previous_focus,
        };
        if !self.context_favorite_is_live(&receipt, window, cx) {
            return;
        }
        let resolver = ActionTargetResolverContext::new(
            &self.search_catalog.folder_aliases_lc,
            &self.search_catalog.bookmark_aliases_lc,
            if indexed { &[] } else { self.search_catalog.saved_actions() },
        );
        let target = ActionTargetResolver.resolve(&receipt.action, &resolver);
        let surface = receipt.invocation.surface;
        let mut context = ActionResolutionContext::new(surface, &receipt.invocation.history_query);
        context.can_add_favorite = target.target.persistent_ref().is_some();
        let Some(favorite) = UniversalActionRegistry.resolve(&target, &context).into_iter()
            .find(|item| item.id == action_ids::RESULT_FAVORITE && item.is_available())
        else {
            return;
        };
        let presentation = favorite.effective_presentation(surface);
        let UniversalActionOperation::UiIntent(UniversalUiIntent::AddFavorite { action }) = favorite.operation else {
            return;
        };
        if !presentation.visible || action != receipt.action {
            return;
        }
        self.external_launch.retire_focus_bound_effects();
        self.context_favorite_popup = Some(ContextFavoritePopup {
            receipt: receipt.clone(),
            position,
            menu: None,
            subscription: None,
        });
        self.fresh_search_enter = None;
        self.retire_search_jump_mode();
        let root = cx.entity().downgrade();
        window.defer(cx, move |window, cx| {
            let _ = root.update(cx, |this, cx| {
                if this.context_favorite_serial != receipt.serial {
                    return;
                }
                if !this.context_favorite_is_live(&receipt, window, cx) {
                    this.retire_context_favorite();
                    cx.notify();
                    return;
                }
                let selected = Rc::new(Cell::new(false));
                let clicked = selected.clone();
                let menu = PopupMenu::build(window, cx, move |menu, _, _| {
                    menu.item(PopupMenuItem::new(presentation.label).on_click(move |_, _, _| {
                        clicked.set(true);
                    }))
                });
                let menu_focus = menu.focus_handle(cx);
                let root = cx.entity().downgrade();
                let serial = receipt.serial;
                let subscription = window.subscribe(&menu, cx, move |_, _: &DismissEvent, window, cx| {
                    let _ = root.update(cx, |this, cx| {
                        if !this.context_favorite_popup.as_ref().is_some_and(|popup| popup.receipt.serial == serial) {
                            return;
                        }
                        let Some(popup) = this.context_favorite_popup.take() else {
                            return;
                        };
                        let choice = selected.replace(false);
                        let restore_focus = !choice && menu_focus.contains_focused(window, cx);
                        let menu_focus = menu_focus.clone();
                        let root = cx.entity().downgrade();
                        let action = action.clone();
                        window.defer(cx, move |window, cx| {
                            let _ = root.update(cx, |this, cx| {
                                let receipt = popup.receipt;
                                if this.context_favorite_serial != receipt.serial
                                    || !this.context_favorite_is_live(&receipt, window, cx)
                                {
                                    return;
                                }
                                if choice {
                                    this.open_result_favorites(action, receipt.previous_focus, window, cx);
                                } else if restore_focus && window.focused(cx).is_none_or(|focus| {
                                    menu_focus.contains(&focus, window)
                                }) {
                                    if let Some(focus) = receipt.previous_focus.filter(|focus| this.focus_handle.contains(focus, window)) {
                                        focus.focus(window, cx);
                                    }
                                }
                            });
                        });
                        cx.notify();
                    });
                });
                menu.focus_handle(cx).focus(window, cx);
                if let Some(popup) = this.context_favorite_popup.as_mut() {
                    popup.menu = Some(menu);
                    popup.subscription = Some(subscription);
                }
                cx.notify();
            });
        });
    }

    fn select_search_result_exact(
        &mut self,
        generation: u64,
        index: usize,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(_operation) = self.search_input_operation(window, cx) else {
            return false;
        };
        let current_query = self.query.read(cx).value().to_string();
        let Some((_, reveal_row)) = select_search_result_state(
            &mut self.launcher_results,
            &current_query,
            generation,
            index,
        ) else {
            return false;
        };
        if let Some(scroll_child) = reveal_row {
            self.root_scroll.scroll_to_top_of_item(ROOT_SCROLL_SEARCH_ITEM);
            self.launcher_results_scroll.scroll_to_item(scroll_child);
        }
        cx.notify();
        true
    }

    fn open_hash_from_file_search(
        &mut self,
        source: WeakEntity<file_search::FileSearchView>,
        receipt: file_search::SelectionReceipt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = source.upgrade() else {
            return;
        };
        let source = view.read(cx);
        let source_popup = source.is_mounted_in(window.window_handle().window_id(), true);
        let current = self.file_search.as_ref().is_some_and(|mounted| mounted.entity_id() == receipt.owner)
            && view.entity_id() == receipt.owner
            && source.is_mounted_in(window.window_handle().window_id(), source_popup)
            && !source.rehosting && (source_popup || (self.root_visibility.active.load(Ordering::Acquire)
                && self.root_visibility.visible.load(Ordering::Acquire)))
            && window.is_window_active() && self.action_sheet.is_none() && self.context_favorite_popup.is_none()
            && source.hash_action_is_focused(window) && source.receipt_is_current(&receipt, view.entity_id());
        if !current { return; }
        let Some(source_native) = source.host_native() else { return; };
        if !native_window_is_valid(source_native) || current_foreground_identity() != Some(source_native.identity) { return; }
        let source_mount = source.mount_receipt();
        let source_host = window.window_handle();
        if source_popup {
            let Some(operation) = self.restart_admission.admit() else { return; };
            if let HashWindowState::Docked { view: hash_view, mount_generation } = &self.hash_window {
                if self.hash_file_picker_pending.load(Ordering::Acquire) || self.hash_clipboard_pending.is_pending()
                    || self.hash_rehost_pending.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() { return; }
                let hash_view = hash_view.clone(); let generation = *mount_generation;
                let epoch = self.hash_session.lock().unwrap_or_else(std::sync::PoisonError::into_inner).epoch();
                let revision = self.hash_rehost_revision.fetch_add(1, Ordering::AcqRel).wrapping_add(1).max(1);
                let origin = HashInheritedOrigin { receipt, mount: source_mount, host: source_host,
                    native: source_native, focus: source.hash_action_focus(cx) };
                hash_view.update(cx, |tool, cx| { tool.set_editors_readonly(true, cx); cx.notify(); });
                self.start_hash_rehost(HashPresentationMode::Popout, generation, epoch, revision, None, Some(origin), operation, cx);
                return;
            }
            let origin = HashInheritedOrigin { receipt, mount: source_mount, host: source_host,
                native: source_native, focus: source.hash_action_focus(cx) };
            self.start_direct_inherited_hash(origin, operation, cx);
            return;
        }
        let Some(operation) = self.restart_admission.admit() else {
            return;
        };
        let outcome = self.open_hash_generator_window(
            HashOpenIntent::File(receipt.path.clone()), operation, window, cx,
        );
        match outcome {
            Ok(()) => {
                self.hash_return = Some(HashReturnReceipt::FileSearch(receipt));
                if self.hash_window.mode() == Some(HashPresentationMode::Docked) {
                    self.root_scroll.scroll_to_top_of_item(self.docked_hash_scroll_item());
                }
            }
            Err(error) => view.update(cx, |view, cx| view.report_hash_error(error, cx)),
        }
        cx.notify();
    }

    fn downloads_triage_open(&self, cx: &App) -> bool {
        #[cfg(windows)]
        { self.downloads_triage.as_ref().is_some_and(|view| view.read(cx).open) }
        #[cfg(not(windows))]
        { let _ = cx; false }
    }

    fn downloads_triage_owns_focus(&self, window: &Window, cx: &App) -> bool {
        #[cfg(windows)]
        { self.downloads_triage.as_ref().is_some_and(|view| view.read(cx).owns_focus(window, cx)) }
        #[cfg(not(windows))]
        { let _ = (window, cx); false }
    }

    #[cfg(windows)]
    fn open_downloads_triage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(_permit) = self.restart_admission.admit() else { return; };
        let view = if let Some(view) = &self.downloads_triage {
            view.clone()
        } else {
            let root = cx.entity().downgrade();
            let admission = Arc::clone(&self.restart_admission);
            let profile = self.app_data_root.clone();
            let view = cx.new(|cx| downloads_triage::DownloadsTriageView::new(cx, root, admission, profile));
            self.downloads_triage = Some(view.clone());
            view
        };
        view.update(cx, |view, cx| view.show(window, cx));
        let index = self.file_search_scroll_item() + usize::from(self.file_search.as_ref().is_some_and(|view| view.read(cx).root_slot_occupied()));
        self.root_scroll.scroll_to_top_of_item(index);
        self.query_history.reset();
        cx.notify();
    }

    fn open_plugin_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.restart_admission.is_fenced() { return; }
        let view = if let Some(view) = &self.plugin_editor { view.clone() } else {
            let root = cx.entity().downgrade();
            let view = cx.new(|cx| plugin_editor::PluginEditorView::new(window, cx, root));
            self.plugin_editor = Some(view.clone());
            view
        };
        let origin = self.root_menu_previous_focus.clone().filter(|focus| self.focus_handle.contains(focus, window));
        let scroll = self.root_scroll.offset();
        view.update(cx, |view, cx| {
            view.show(origin, scroll, window, cx);
            let session = view.session();
            view.focus_filter(session, window, cx);
        });
        let index = self.file_search_scroll_item()
            + usize::from(self.file_search.as_ref().is_some_and(|view| view.read(cx).root_slot_occupied()))
            + usize::from(self.downloads_triage_open(cx));
        self.root_scroll.scroll_to_top_of_item(index);
        cx.notify();
    }

    fn open_file_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(_permit) = self.restart_admission.admit() else { return; };
        if self.file_search_popup.is_some() { self.reveal_file_search_popup(None, cx); return; }
        if self.file_search.as_ref().is_some_and(|view| view.read(cx).rehosting) { return; }
        let was_open = self.file_search.as_ref().is_some_and(|view| view.read(cx).open);
        let view = if let Some(view) = &self.file_search {
            view.clone()
        } else {
            let root = cx.entity().downgrade();
            let admission = Arc::clone(&self.restart_admission);
            let settings = self.file_search_settings.clone();
            let native = self.native_window;
            let view = cx.new(|cx| file_search::FileSearchView::new(window, cx, root, admission, settings, native));
            self.file_search = Some(view.clone());
            view
        };
        view.update(cx, |view, cx| {
            if !view.open { view.popout = false; view.replace_mount(window.window_handle(), self.native_window, window, cx); }
            view.show(window, cx);
        });
        if !was_open && self.file_search_prefers_popout() {
            let weak = view.downgrade(); cx.spawn(async move |_, cx| { let _ = weak.update(cx, |view, cx| view.request_rehost(true, cx)); }).detach();
        }
        self.root_scroll.scroll_to_top_of_item(self.file_search_scroll_item());
        self.query_history.reset();
        cx.notify();
    }

    fn file_search_scroll_item(&self) -> usize {
        self.quick_help_scroll_item()
            + usize::from(self.quick_help.open)
            + usize::from(self.result_favorites.is_open())
            + usize::from(self.app_editor.open)
    }

    fn activate_launcher_result(
        &mut self,
        generation: u64,
        index: usize,
        source: ActivationSource,
        origin: LauncherResultActivationOrigin,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let query = self.query.read(cx).value().to_string();
        let Some(result) = self
            .launcher_results
            .resolve_result(generation, index, &query)
        else {
            return;
        };
        if !self.launcher_result_activation_is_live(origin, window, cx) {
            return;
        }
        if result.action.action == "settings:search" {
            if self.restart_admission.is_fenced() || window.has_active_dialog(cx)
                || !self.external_owner_is_current(window.focused(cx).as_ref(), window, cx)
                || self.native_window.is_none_or(|native| current_foreground_identity() != Some(native.identity)) { return; }
            if matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "settings") {
                if let Some(term) = settings_navigation::current_provider(&query, &result.action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref()) {
                    self.open_settings_editor(window, cx, SettingsOpenOrigin::Search);
                    self.settings_panel_scroll.set_offset(point(px(0.0), px(0.0)));
                    if let Some(navigation) = self.settings_navigation.clone() { navigation.update(cx, |navigation, cx| navigation.set_query(term, window, cx)); }
                }
            }
            return;
        }
        if matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "favorites")
            && favorite_search::is_executable(&result.action)
        {
            if self.enabled_plugins.as_ref().is_none_or(|enabled| enabled.contains("favorites"))
                && multi_launcher::settings::provider_search_capability_enabled(self.enabled_capabilities.as_ref(), "favorites")
            {
                self.start_favorite_launch(result.action, query, source, window, cx);
            }
            return;
        }
        if result.action.action == "regex:open" {
            if !matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "regex_tester")
                || !regex_tester::provider_is_current(&query, &result.action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref()) { return; }
        }
        if matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "activity")
            && !activity::provider_is_current(&query, &result.action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref()) {
            return;
        }
        if matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "xml_toolkit")
            && !xml_utility::provider_is_current(&query, &result.action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref()) { return; }
        if matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "json_utility")
            && !json_utility::provider_is_current(&query, &result.action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref()) { return; }
        #[cfg(windows)]
        if matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "downloads_triage") {
            if is_downloads_triage_opener(&result.action)
                && self.enabled_plugins.as_ref().is_none_or(|enabled| enabled.contains("downloads_triage"))
                && self.enabled_capabilities.as_ref().is_none_or(|caps| caps.get("downloads_triage").is_none_or(|caps| caps.iter().any(|cap| cap == "search")))
            {
                self.open_downloads_triage(window, cx);
            }
            return;
        }
        if matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "file_lock") {
            if cfg!(windows) && file_lock::current_provider(&query, &result.action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref()) {
                self.open_file_lock(window, cx);
            }
            return;
        }
        if matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "folder_size") {
            if folder_size::current_provider(&query, &result.action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref()) {
                self.open_folder_size(window, cx);
            }
            return;
        }
        if matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "file_search") {
            if let Some(receipt) = self.file_search_probe.receipt(generation, &query, &result.action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref(), self.restart_admission.is_fenced()) {
                self.open_typed_file_search(receipt, window, cx);
                return;
            }
            if is_file_search_opener(&query, &result.action)
                && self.enabled_plugins.as_ref().is_none_or(|enabled| enabled.contains("file_search"))
                && self.enabled_capabilities.as_ref().is_none_or(|caps| caps.get("file_search").is_none_or(|caps| caps.iter().any(|cap| cap == "search")))
            {
                self.open_file_search(window, cx);
            }
            return;
        }
        if let LauncherResultOrigin::Provider(name) = &result.origin {
            if name == "calculator" {
                if let Some(history) = self.calculator_search.history_result(
                    self.launcher_results.generation, &query, &result.action,
                ) {
                    if self.enabled_plugins.as_ref().is_some_and(|enabled| !enabled.contains(name))
                        || !multi_launcher::settings::provider_search_capability_enabled(self.enabled_capabilities.as_ref(), name)
                    { return; }
                    self.start_calculator_history_launch(result.action, history, query, source, window, cx);
                    return;
                }
            }
            if let Some(scalar) = calculator_search::classify(name, &query, &result.action) {
                if self.enabled_plugins.as_ref().is_some_and(|enabled| !enabled.contains(name))
                    || !multi_launcher::settings::provider_search_capability_enabled(self.enabled_capabilities.as_ref(), name)
                { return; }
                self.start_calculator_launch(result.action, scalar, query, source, window, cx);
                return;
            }
        }
        if let LauncherResultOrigin::Provider(name) = &result.origin {
            if let Some(route) = unit_convert_search::classify(name, &result.action) {
                if !unit_convert_search::is_current(
                    &query, &result.action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref(),
                ) { return; }
                self.start_unit_launch(result.action, route, query, source, window, cx);
                return;
            }
        }
        if let LauncherResultOrigin::Provider(name) = &result.origin {
            if let Some(text) = base_convert_search::classify(name, &result.action) {
                if !base_convert_search::is_current(
                    &query,
                    &result.action,
                    self.enabled_plugins.as_ref(),
                    self.enabled_capabilities.as_ref(),
                ) {
                    return;
                }
                self.start_copy_launch(result.action, text, query, source, window, cx);
                return;
            }
        }
        if let LauncherResultOrigin::Provider(name) = &result.origin {
            if let Some(route) = date_search::classify(name, &result.action) {
                if !multi_launcher::settings::provider_search_capability_enabled(self.enabled_capabilities.as_ref(), name)
                    || self.enabled_plugins.as_ref().is_some_and(|enabled| !enabled.contains(name))
                { return; }
                self.start_date_launch(result.action, route, query, source, window, cx);
                return;
            }
        }
        if matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "windows") {
            self.activate_window_search_result(&result, index, query, source, window, cx);
            return;
        }
        if matches!(&result.origin, LauncherResultOrigin::Indexed { .. }) {
            if let Some(authority) = indexed_applications::admitted_result(
                &result,
                self.indexed_applications.snapshot.as_ref(),
                &self.search_catalog.actions[self.search_catalog.saved_count..],
            ) {
                let command = multi_launcher::commands::ExternalCommand {
                    target: result.action.action.clone(), args: None, namespace: None,
                };
                self.start_indexed_launch(
                    result.action, command, authority, query, source,
                    result.display_label, window, cx,
                );
            }
            return;
        }
        let clipboard_provider_query = if matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "clipboard_modify") {
            if self.clipboard_modify_startup.runtime.is_some() {
                let Some((published, effective, generation, catalog)) = &self.clipboard_modify_startup.published else { return; };
                if published != &query || *generation != self.launcher_results.generation
                    || !search_providers::clipboard_catalog_is_current(&self.search_catalog.providers.clipboard_modify_catalog(), catalog)
                    || !multi_launcher::plugins::clipboard_modify::search_with_catalog(effective, catalog).contains(&result.action)
                    || self.enabled_plugins.as_ref().is_some_and(|enabled| !enabled.contains("clipboard_modify"))
                    || !multi_launcher::settings::provider_search_capability_enabled(self.enabled_capabilities.as_ref(), "clipboard_modify") { return; }
                effective.clone()
            } else { query.clone() }
        } else { query.clone() };
        if result.action.action == "clipboard_modify:undo" {
            if matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "clipboard_modify")
                && clipboard_modify_view::current_undo_provider(&clipboard_provider_query, &result.action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref()) {
                self.start_clipboard_undo(result.action, query, source, window, cx);
            }
            return;
        }
        if result.action.action == "tempfile:dialog" {
            if matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "tempfile")
                && tempfile_view::current_provider(&query, &result.action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref()) {
                self.open_tempfile(window, cx);
            }
            return;
        }
        if result.action.action == "snippet:dialog" {
            if matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "snippets")
                && snippets::current_provider(&query, &result.action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref())
            {
                self.open_snippets(window, cx);
            }
            return;
        }
        if result.action.action.starts_with("snippet:run:") {
            if matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "snippets") {
                if let Some((raw, effective, generation, snapshot)) = &self.snippets_search.published {
                    if raw == &query && *generation == self.launcher_results.generation
                        && multi_launcher::common::query::split_action_filters(&query).0 == *effective
                        && snippets::named_row_is_current(&query, &result.action, snapshot, self.search_catalog.providers.snippets_owner().as_deref(), self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref())
                        && search_providers::snippets_named_rows(&query, &snapshot.entries, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref()).contains(&result.action)
                    {
                        if let Ok(Command::Storage(multi_launcher::commands::StorageCommand::SnippetRun(alias))) = parse_action(&result.action) {
                            if let Some(index) = snippets::exact_named_index(&snapshot.entries, &snapshot.entries, &alias) {
                                let entry = snapshot.entries[index].clone();
                                let snapshot = snapshot.clone();
                                self.start_plain_snippet_run(result.action, entry, snapshot, query, source, window, cx);
                            }
                        }
                    }
                }
            }
            return;
        }
        if result.action.action.starts_with("snippet:edit:") {
            if matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "snippets") {
                if let Some((raw, effective, generation, snapshot)) = &self.snippets_search.published {
                    if raw == &query && *generation == self.launcher_results.generation
                        && multi_launcher::common::query::split_action_filters(&query).0 == *effective
                        && snippets::named_row_is_current(&query, &result.action, snapshot, self.search_catalog.providers.snippets_owner().as_deref(), self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref())
                        && search_providers::snippets_named_rows(&query, &snapshot.entries, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref()).contains(&result.action)
                    {
                        if let Ok(Command::Storage(multi_launcher::commands::StorageCommand::SnippetEdit(alias))) = parse_action(&result.action) {
                            let generation = *generation;
                            let snapshot = snapshot.clone();
                            self.start_named_snippet(alias, query, generation, snapshot, window, cx);
                        }
                    }
                }
            }
            return;
        }
        if result.action.action == "clipboard_modify:execute" {
            if matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "clipboard_modify")
                && self.enabled_plugins.as_ref().is_none_or(|enabled| enabled.contains("clipboard_modify"))
                && multi_launcher::settings::provider_search_capability_enabled(self.enabled_capabilities.as_ref(), "clipboard_modify") {
                if let Some((published_query, effective, generation, catalog)) = &self.clipboard_modify_startup.published {
                    if published_query == &query && *generation == self.launcher_results.generation
                        && search_providers::clipboard_catalog_is_current(&self.search_catalog.providers.clipboard_modify_catalog(), catalog) {
                        let intent = multi_launcher::plugins::clipboard_modify::execution_from_catalog(effective, &result.action, catalog);
                        let payload = result.action.args.as_deref().and_then(|args| multi_launcher::clipboard_modify::actions::decode_action_payload::<multi_launcher::clipboard_modify::actions::ClipboardModifyActionPayload>(args).ok());
                        let canonical = match payload {
                            Some(multi_launcher::clipboard_modify::actions::ClipboardModifyActionPayload::ExecuteAdHocStages { canonical_command, .. })
                            | Some(multi_launcher::clipboard_modify::actions::ClipboardModifyActionPayload::ExecuteTemplate { canonical_command, .. })
                            | Some(multi_launcher::clipboard_modify::actions::ClipboardModifyActionPayload::ExecuteSavedPipeline { canonical_command, .. }) => Some(canonical_command),
                            _ => None,
                        };
                        if let (Some(intent), Some(canonical)) = (intent, canonical) {
                            self.start_clipboard_execute(result.action, intent, catalog.clone(), canonical, query, source, window, cx);
                        }
                    }
                }
            }
            return;
        }
        let history_label = result.display_label.clone();
        #[cfg(windows)]
        if let Some(command) = result.saved_shortcut() {
            self.start_external_launch(
                result.action,
                command,
                query,
                source,
                Some(external_launch::AssociationKind::SavedShortcut),
                history_label,
                window,
                cx,
            );
            return;
        }
        if let Some(command) = result.saved_url() {
            self.start_external_launch(result.action, command, query, source,
                Some(external_launch::AssociationKind::SavedUrl), history_label, window, cx);
            return;
        }
        if let Some(command) = result.saved_document() {
            self.start_external_launch(
                result.action,
                command,
                query,
                source,
                Some(external_launch::AssociationKind::SavedDocument),
                history_label,
                window,
                cx,
            );
            return;
        }
        if result.origin == LauncherResultOrigin::Saved
            && self.search_catalog.verified_folders.contains(&result.action)
        {
            if let Some(command) = saved_folder::candidate(&result.action) {
                self.start_external_launch(
                    result.action, command, query, source,
                    Some(external_launch::AssociationKind::SavedFolder), history_label, window, cx,
                );
                return;
            }
        }
        if let Some(command) = result.saved_external() {
            self.start_external_launch(result.action, command, query, source, None, history_label, window, cx);
            return;
        }
        if let Some((kind, command)) =
            external_launch::provider_association(&result.origin, &result.action)
        {
            if let external_launch::AssociationKind::Web(provider) = kind {
                if !provider.is_current(&query, &result.action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref()) {
                    return;
                }
            }
            self.start_external_launch(
                result.action,
                command,
                query,
                source,
                Some(kind),
                None,
                window,
                cx,
            );
            return;
        }
        let modify_open = matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "clipboard_modify")
            && clipboard_modify_view::current_provider(&clipboard_provider_query, &result.action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref());
        let clipboard_open = matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "clipboard")
            && clipboard_history::current_provider(&query, &result.action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref());
        let installed_invocation = match if modify_open {
            parse_command(result.action, None, source).and_then(|invocation| {
                let Command::ClipboardModify(multi_launcher::commands::ClipboardModifyCommand::Open { section }) = &invocation.command else { unreachable!() };
                let section = *section;
                Ok(InstalledSearchInvocation { invocation, route: InstalledSearchRoute::ClipboardModify(section) })
            }).map_err(|error| InstalledSearchActionError::Unsupported(error.to_string()))
        } else if clipboard_open {
            parse_command(result.action, None, source).map(|invocation| InstalledSearchInvocation { invocation, route: InstalledSearchRoute::ClipboardHistory }).map_err(|error| InstalledSearchActionError::Unsupported(error.to_string()))
        } else { parse_installed_search_invocation(result.action, source) } {
            Ok(installed_invocation) => installed_invocation,
            Err(InstalledSearchActionError::QuerySet(error)) => {
                self.launcher_search_error = Some(format!("Could not apply saved query: {error}"));
                cx.notify();
                return;
            }
            Err(InstalledSearchActionError::Unsupported(error)) => {
                self.hash_open_error = Some(format!("Could not open Hash Generator: {error}"));
                cx.notify();
                return;
            }
        };
        let query_set = matches!(
            &installed_invocation.route,
            InstalledSearchRoute::QuerySet(_)
        );
        let settings_open = matches!(
            &installed_invocation.route,
            InstalledSearchRoute::Settings
        );
        let help_open = matches!(&installed_invocation.route, InstalledSearchRoute::Help);
        let activity_open = matches!(&installed_invocation.route, InstalledSearchRoute::Activity);
        let favorites_open = matches!(
            &installed_invocation.route,
            InstalledSearchRoute::FavoritesBrowse | InstalledSearchRoute::FavoritesNamed(_)
        );
        let Some(operation) = self.restart_admission.admit() else {
            if query_set {
                self.launcher_search_error =
                    Some("GPUI is closing; saved queries can no longer be applied.".to_owned());
            } else if settings_open {
                self.launcher_search_error =
                    Some("GPUI is closing; Settings can no longer be opened.".to_owned());
            } else if help_open {
                self.launcher_search_error =
                    Some("GPUI is closing; Command Help can no longer be opened.".to_owned());
            } else if activity_open {
                self.launcher_search_error =
                    Some("GPUI is closing; Activity can no longer be opened.".to_owned());
            } else if favorites_open {
                self.launcher_search_error =
                    Some("GPUI is closing; Favorites can no longer be opened.".to_owned());
            } else {
                self.hash_open_error =
                    Some("GPUI is closing; new utility operations are disabled.".to_owned());
            }
            cx.notify();
            return;
        };
        let InstalledSearchInvocation { invocation, route } = installed_invocation;
        match route {
            InstalledSearchRoute::HashOpen => {
                let outcome = dispatch_hash_open(invocation, || {
                    self.open_hash_generator_window(HashOpenIntent::Default, operation, window, cx)
                });
                if outcome.is_ok() {
                    self.query_history.reset();
                }
                self.hash_open_error = outcome
                    .err()
                    .map(|error| format!("Could not open Hash Generator: {error}"));
            }
            InstalledSearchRoute::QuerySet(outcome) => {
                if let Err(error) = self.apply_query_set_outcome(outcome, operation, window, cx) {
                    self.launcher_search_error =
                        Some(format!("Could not apply saved query: {error}"));
                }
            }
            InstalledSearchRoute::Settings => {
                drop(operation);
                self.open_settings_editor(window, cx, SettingsOpenOrigin::Search);
                self.settings_panel_scroll.set_offset(point(px(0.0), px(0.0)));
                if let Some(navigation) = self.settings_navigation.clone() { navigation.update(cx, |navigation, cx| navigation.reset(window, cx)); }
                self.settings_focus_handle.focus(window, cx);
            }
            InstalledSearchRoute::Help => {
                drop(operation);
                self.open_command_help(window, cx, CommandHelpOpenOrigin::Search, None);
            }
            InstalledSearchRoute::Activity => {
                drop(operation);
                let focus = Some(self.query.focus_handle(cx));
                self.open_activity(focus, window, cx);
            }
            InstalledSearchRoute::JsonUtility(intent) => {
                drop(operation);
                self.open_json_utility(intent, Some(self.query.focus_handle(cx)), window, cx);
            }
            InstalledSearchRoute::XmlUtility => { drop(operation); self.open_xml_toolkit(window, cx); }
            InstalledSearchRoute::RegexTester => { drop(operation); self.open_regex_tester(window, cx); }
            InstalledSearchRoute::ClipboardHistory => { drop(operation); self.open_clipboard_history(window, cx); }
            InstalledSearchRoute::ClipboardModify(section) => { drop(operation); self.open_clipboard_modify(section, window, cx); }
            InstalledSearchRoute::FavoritesBrowse => {
                drop(operation);
                let focus = Some(self.query.focus_handle(cx));
                self.open_favorites_browse(focus, window, cx);
            }
            InstalledSearchRoute::FavoritesNamed(label) => {
                drop(operation);
                let focus = Some(self.query.focus_handle(cx));
                self.open_favorites_named(label, focus, window, cx);
            }
        }
        // Hash Open and Settings preserve the launcher query and results.
        // Query Set uses the real editor Change path to refresh transient rows.
        cx.notify();
    }

    fn apply_query_set_outcome(
        &mut self,
        outcome: CommandOutcome,
        _operation: EffectPermit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if query_set_outcome_text(&outcome).is_none() {
            return Err("the Query Set outcome is not supported by Search".to_owned());
        }
        let CommandOutcome {
            query: QueryPolicy::Set(query),
            focus,
            ..
        } = outcome
        else {
            return Err("the Query Set outcome does not contain a new query".to_owned());
        };
        self.query_history.reset();
        self.search_completion.clear_for_query_replacement();
        self.launcher_search_error = None;
        self.query.update(cx, |state, cx| {
            state.replace_all(query, window, cx);
        });
        if focus {
            self.query.focus_handle(cx).focus(window, cx);
        }
        Ok(())
    }

    fn next_hash_mount_generation(&mut self) -> u64 {
        self.hash_mount_counter = self.hash_mount_counter.wrapping_add(1).max(1);
        self.hash_mount_counter
    }

    fn notify_hash_mount(&self, cx: &mut Context<Self>) {
        if let Some(view) = self.hash_window.view().cloned() {
            view.update(cx, |_, cx| cx.notify());
        }
        cx.notify();
    }

    fn observe_hash_popout_close(
        &mut self,
        window: AnyWindowHandle,
        native_window: Option<NativeWindow>,
        mount_generation: u64,
        session_epoch: u64,
        cx: &mut Context<Self>,
    ) -> Subscription {
        let window_id = window.window_id();
        let root = cx.entity().downgrade();
        let active_mount_generation = Arc::clone(&self.hash_mount_generation);
        let session = Arc::clone(&self.hash_session);
        let restart_admission = Arc::clone(&self.restart_admission);
        let hash_window_revision = Arc::clone(&self.hash_window_revision);
        cx.on_window_closed(move |cx, closed| {
            if closed != window_id {
                return;
            }
            let Some(operation) = restart_admission.admit() else {
                return;
            };
            let active_mount_generation = Arc::clone(&active_mount_generation);
            let session = Arc::clone(&session);
            let hash_window_revision = Arc::clone(&hash_window_revision);
            let root_for_update = root.clone();
            let root_for_retirement = root.clone();
            let _ = root_for_update.update(cx, move |root, cx| {
                root.hash_window_close_subscriptions
                    .retain(|(handle, _)| handle.window_id() != closed);
                let is_active_mount = matches!(
                    &root.hash_window,
                    HashWindowState::Popout {
                        mount_generation: active_generation,
                        window,
                        ..
                    } if *active_generation == mount_generation
                        && window.window_id() == closed
                        && active_mount_generation.load(Ordering::Acquire) == mount_generation
                );
                if is_active_mount {
                    root.drain_hash_native_geometry(cx, true);
                }
                let mut session = session
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if is_active_mount && session.epoch() == session_epoch {
                    root.hash_retiring_windows.push(RetiringHashWindow {
                        window,
                        native_window,
                    });
                    session.end();
                    active_mount_generation.store(0, Ordering::Release);
                    root.hash_window = HashWindowState::Closed;
                    panel_stack::retire(&mut root.panel_stack, Panel::HashGeneratorDialog);
                    root.hash_rehost_pending.store(false, Ordering::Release);
                    root.hash_rehost_revision.fetch_add(1, Ordering::AcqRel);
                    let close_revision = hash_window_revision
                        .fetch_add(1, Ordering::AcqRel)
                        .wrapping_add(1);
                    let return_receipt = root.hash_return.take();
                    let root_for_retirement = root_for_retirement.clone();
                    cx.spawn(async move |_, cx| {
                        let _operation = operation;
                        let retired =
                            wait_for_hash_window_retirement(cx, native_window).await;
                        let _ = root_for_retirement.update(cx, |root, cx| {
                            if retired {
                                root.hash_retiring_windows
                                    .retain(|retiring| retiring.window.window_id() != window_id);
                            } else {
                                let message = "The Hash Generator window closed, but Windows has not retired its native handle yet.".to_owned();
                                root.hash_presentation_error = Some(message.clone());
                                *root.hash_presentation_feedback
                                    .lock()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(message);
                            }
                            cx.notify();
                        });
                        if retired {
                            restore_root_search_focus_after_hash_close(
                                root_for_retirement.clone(),
                                close_revision,
                                return_receipt,
                                None,
                                cx,
                            )
                            .await;
                        }
                    })
                    .detach();
                }
                cx.notify();
            });
        })
    }

    fn open_hash_generator_window(
        &mut self,
        intent: HashOpenIntent,
        operation: EffectPermit,
        root_window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.open_hash_generator_with_mount_intent(intent, operation, root_window, hash_pin::HashMountIntent::User, cx)
    }

    fn open_hash_generator_with_mount_intent(
        &mut self,
        intent: HashOpenIntent,
        operation: EffectPermit,
        root_window: &mut Window,
        mount_intent: hash_pin::HashMountIntent,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.hash_pin.pending.is_some() {
            return Err(hash_pin::UNPIN_PENDING.to_owned());
        }
        if mount_intent == hash_pin::HashMountIntent::User {
            self.hash_pin.remount_revision = None;
        }
        let reactivation = matches!(&intent, HashOpenIntent::Reactivate);
        let file = match intent {
            HashOpenIntent::Default => {
                self.hash_return = None;
                None
            }
            HashOpenIntent::Reactivate => None,
            HashOpenIntent::File(path) => {
                if self.hash_rehost_pending.load(Ordering::Acquire)
                    || self.hash_file_picker_pending.load(Ordering::Acquire)
                    || self.hash_clipboard_pending.is_pending()
                {
                    return Err("Hash Generator is busy with another operation.".to_owned());
                }
                Some(path)
            }
        };
        match &self.hash_window {
            HashWindowState::Docked { view, .. } => {
                if !mount_intent.is_user_activation() { return Ok(()); }
                let view = view.clone();
                view.update(cx, |tool, cx| {
                    if let Some(path) = file {
                        tool.select_external_file(path, root_window, cx)?;
                    }
                    tool.focus_editable(root_window, cx);
                    Ok::<(), String>(())
                })?;
                panel_stack::promote(&mut self.panel_stack, Panel::HashGeneratorDialog);
                self.root_scroll.scroll_to_top_of_item(self.docked_hash_scroll_item());
                return Ok(());
            }
            HashWindowState::Popout { window, view, mount_generation } => {
                if !mount_intent.is_user_activation() { return Ok(()); }
                let window = *window;
                let view = view.clone();
                let mount_generation = *mount_generation;
                let session_epoch = self.hash_session.lock().unwrap_or_else(std::sync::PoisonError::into_inner).epoch();
                let expected_foreground = if reactivation {
                    self.native_window.map(|native| native.identity)
                } else {
                    current_foreground_identity()
                };
                let revision = self
                    .hash_window_revision
                    .fetch_add(1, Ordering::AcqRel)
                    .wrapping_add(1)
                    .max(1);
                let native_window = update_kit_window_content(window, &view, cx, |tool, window, cx| {
                    if let Some(path) = file {
                        if cfg!(windows) && !tool.native_window.is_some_and(native_window_is_valid) {
                            return Err("The Hash Generator window has no current Windows handle.".to_owned());
                        }
                        tool.select_external_file(path, window, cx)?;
                    }
                    tool.focus_editable(window, cx);
                    Ok(tool.native_window)
                }).map_err(|error| format!("Could not focus the existing Hash window: {error}"))??;
                match native_window {
                    Some(native_window) => {
                        if !reactivation {
                            panel_stack::promote(&mut self.panel_stack, Panel::HashGeneratorDialog);
                        }
                        let revision_guard = Arc::clone(&self.hash_window_revision);
                        let root = cx.entity().downgrade();
                        let view_id = view.entity_id();
                        if reactivation {
                            self.hash_pin.activation = Some(hash_pin::ActivationReceipt {
                                mount: mount_generation, epoch: session_epoch, revision,
                                window: window.window_id(), view: view_id, native: native_window,
                                stack: self.panel_stack.clone(), accepted: false, observed: false,
                            });
                        }
                        cx.spawn(async move |_, cx| {
                            let _operation = operation;
                            let accepted = cx.background_executor()
                                .spawn(async move {
                                    if reactivation {
                                        activate_for_hash_pin(&revision_guard, revision, native_window, expected_foreground)
                                    } else {
                                        activate_for_secondary_window(&revision_guard, revision, native_window, expected_foreground);
                                        false
                                    }
                                })
                                .await;
                            if reactivation {
                                let _ = root.update(cx, |root, cx| {
                                    if root.hash_pin.activation.as_ref().is_some_and(|receipt| receipt.revision == revision) {
                                        if accepted {
                                            if let Some(receipt) = root.hash_pin.activation.as_mut() { receipt.accept(revision); }
                                            root.confirm_hash_pin_activation(cx);
                                        } else {
                                            root.hash_pin.activation = None;
                                        }
                                    }
                                });
                            }
                        })
                        .detach();
                        return Ok(());
                    }
                    None if !cfg!(windows) => {
                        panel_stack::promote(&mut self.panel_stack, Panel::HashGeneratorDialog);
                        return Ok(());
                    }
                    None => {
                        return Err(
                            "The Hash Generator window has no checked Windows handle to activate."
                                .to_owned(),
                        );
                    }
                }
            }
            HashWindowState::Closed => {}
        }

        self.hash_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .begin(file);
        self.hash_native_geometry_error = None;
        let session_epoch = self
            .hash_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .epoch();
        let initial_draft = {
            let session = self
                .hash_session
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            HashDraft::from_session(&session)
        };
        let mount_generation = self.next_hash_mount_generation();
        let mode = HashPresentationMode::from_saved(
            self.utility_presentations.preference(self.hash_tool_key),
        );
        let root = cx.entity().downgrade();
        let mount_config = HashMountConfig {
            jump_controller: self.search_jump_mode.fresh(),
            session: Arc::clone(&self.hash_session),
            wake_signal: Arc::clone(&self.hash_wake_signal),
            file_picker_pending: Arc::clone(&self.hash_file_picker_pending),
            clipboard_pending: Arc::clone(&self.hash_clipboard_pending),
            pinned: Arc::clone(&self.hash_pin.saved),
            restart_admission: Arc::clone(&self.restart_admission),
            mount_generation,
            session_epoch,
            active_mount_generation: Arc::clone(&self.hash_mount_generation),
            rehost_pending: Arc::clone(&self.hash_rehost_pending),
            rehost_revision: Arc::clone(&self.hash_rehost_revision),
            presentation_feedback: Arc::clone(&self.hash_presentation_feedback),
            native_geometry_sequence: Arc::clone(&self.hash_geometry_sequence),
            hash_tool_key: self.hash_tool_key,
            size_write_error: self.hash_size_write_error.clone(),
            root,
            mode,
        };
        let mut popout_setup = None;
        match mode {
            HashPresentationMode::Docked => {
                self.hash_mount_generation
                    .store(mount_generation, Ordering::Release);
                let view =
                    create_hash_generator_view(root_window, cx, initial_draft, None, mount_config);
                if mount_intent == hash_pin::HashMountIntent::Passive {
                    view.update(cx, |tool, cx| tool.focus_editable(root_window, cx));
                }
                self.hash_window = HashWindowState::Docked {
                    mount_generation,
                    view,
                };
            }
            HashPresentationMode::Popout => {
                let mut options = hash_popout_window_options(cx);
                options.focus = mount_intent.is_user_activation();
                let (window, view) = match gpui_kit::open_window(options, cx, move |window, cx| {
                    let native_window = checked_native_window(window).ok().flatten();
                    let view = create_hash_generator_view(
                        window,
                        cx,
                        initial_draft,
                        native_window,
                        mount_config,
                    );
                    if mount_intent == hash_pin::HashMountIntent::Passive {
                        view.update(cx, |tool, cx| tool.focus_editable(window, cx));
                    }
                    view
                }) {
                    Ok(opened) => opened,
                    Err(error) => {
                        self.hash_session
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .end();
                        return Err(format!(
                            "Kit could not create the Hash Generator window: {error}"
                        ));
                    }
                };
                let native_window = view.read(cx).native_window;
                if cfg!(windows) && native_window.is_none() {
                    self.hash_retiring_windows.push(RetiringHashWindow {
                        window,
                        native_window,
                    });
                    let _ = window.update(cx, |_, window, _| window.remove_window());
                    self.hash_session
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .end();
                    return Err(
                        "The Hash Generator popout has no checked Windows handle. It was asked to close, but restart is blocked because Windows retirement cannot be confirmed."
                            .to_owned(),
                    );
                }
                let geometry_error = view.read(cx).native_geometry_error.clone();
                self.hash_native_geometry_error = geometry_error;
                self.hash_mount_generation
                    .store(mount_generation, Ordering::Release);
                let close_subscription = self.observe_hash_popout_close(
                    window,
                    native_window,
                    mount_generation,
                    session_epoch,
                    cx,
                );
                let view_for_setup = view.clone();
                self.hash_window = HashWindowState::Popout {
                    mount_generation,
                    window,
                    view,
                };
                self.hash_window_close_subscriptions
                    .push((window, close_subscription));
                popout_setup = native_window.map(|native_window| (native_window, view_for_setup));
            }
        }
        mount_intent.record_stack(&mut self.panel_stack);
        if mode == HashPresentationMode::Docked && mount_intent.is_user_activation() {
            self.root_scroll.scroll_to_top_of_item(self.docked_hash_scroll_item());
        }
        self.hash_wake_signal.notify();
        self.hash_open_error = None;
        if let Some((native_window, view)) = popout_setup {
            self.schedule_hash_popout_setup(
                native_window,
                view,
                mount_generation,
                session_epoch,
                operation,
                cx,
            );
        } else {
            drop(operation);
        }
        Ok(())
    }

    fn restore_hash_transition(
        &mut self,
        mount_generation: u64,
        session_epoch: u64,
        revision: u64,
        message: String,
        cx: &mut Context<Self>,
    ) {
        if self.hash_rehost_revision.load(Ordering::Acquire) != revision {
            return;
        }
        self.hash_rehost_pending.store(false, Ordering::Release);
        let still_current = self.hash_mount_generation.load(Ordering::Acquire) == mount_generation
            && self
                .hash_session
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .epoch()
                == session_epoch;
        if still_current && let Some(view) = self.hash_window.view().cloned() {
            view.update(cx, |tool, cx| {
                tool.set_editors_readonly(false, cx);
                cx.notify();
            });
        }
        self.hash_open_error = Some(message.clone());
        *self
            .hash_presentation_feedback
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(message);
        cx.notify();
    }

    fn release_direct_inherited_hash_transition(&mut self, target_mount: u64, epoch: u64, revision: u64, cx: &mut Context<Self>) {
        if self.hash_rehost_revision.load(Ordering::Acquire) != revision { return; }
        self.hash_rehost_pending.store(false, Ordering::Release);
        let current = self.hash_window.mount_generation() == Some(target_mount)
            && self.hash_mount_generation.load(Ordering::Acquire) == target_mount
            && self.hash_session.lock().unwrap_or_else(std::sync::PoisonError::into_inner).epoch() == epoch;
        if current { if let Some(view) = self.hash_window.view().cloned() {
            view.update(cx, |tool, cx| { tool.set_editors_readonly(false, cx); cx.notify(); });
        }}
        cx.notify();
    }

    fn report_inherited_hash_origin_error(&self, origin: &HashInheritedOrigin, message: &str, cx: &mut Context<Self>) {
        if !self.hash_inherited_origin_current(origin, origin.native.identity, cx) { return; }
        if let Some(view) = self.file_search.clone().filter(|view| view.entity_id() == origin.receipt.owner) {
            view.update(cx, |view, cx| view.report_hash_error(message.to_owned(), cx));
        }
    }

    fn start_direct_inherited_hash(&mut self, origin: HashInheritedOrigin, operation: EffectPermit, cx: &mut Context<Self>) {
        if self.hash_pin.pending.is_some() || self.hash_file_picker_pending.load(Ordering::Acquire)
            || self.hash_clipboard_pending.is_pending()
            || self.hash_rehost_pending.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
            self.report_inherited_hash_origin_error(&origin, "Hash Generator is busy with another operation.", cx); return;
        }
        let revision = self.hash_rehost_revision.fetch_add(1, Ordering::AcqRel).wrapping_add(1).max(1);
        let epoch = self.hash_session.lock().unwrap_or_else(std::sync::PoisonError::into_inner).epoch();
        let existing = match &self.hash_window {
            HashWindowState::Popout { window, view, mount_generation } => Some((*window, view.clone(), *mount_generation)),
            HashWindowState::Closed => None,
            _ => { self.hash_rehost_pending.store(false, Ordering::Release);
                self.report_inherited_hash_origin_error(&origin, "Hash Generator changed before activation.", cx); return; }
        };
        if let Some((_, view, _)) = &existing { view.update(cx, |tool, cx| { tool.set_editors_readonly(true, cx); cx.notify(); }); }
        let target_mount = existing.as_ref().map(|(_, _, mount)| *mount).unwrap_or_else(|| self.next_hash_mount_generation());
        let candidate_epoch = if existing.is_some() { epoch } else { epoch.wrapping_add(1) };
        let root = cx.entity().downgrade();
        let active_mount = self.hash_mount_generation.clone();
        let initial_mount = active_mount.load(Ordering::Acquire);
        let transition_pending = self.hash_rehost_pending.clone();
        let transition_revision = self.hash_rehost_revision.clone();
        let transition_session = self.hash_session.clone();
        let admission = self.restart_admission.clone();
        let config = existing.is_none().then(|| HashMountConfig {
            jump_controller: self.search_jump_mode.fresh(), session: Arc::clone(&self.hash_session), wake_signal: Arc::clone(&self.hash_wake_signal),
            file_picker_pending: Arc::clone(&self.hash_file_picker_pending), clipboard_pending: Arc::clone(&self.hash_clipboard_pending),
            pinned: Arc::clone(&self.hash_pin.saved), restart_admission: Arc::clone(&self.restart_admission), mount_generation: target_mount,
            session_epoch: candidate_epoch, active_mount_generation: Arc::clone(&self.hash_mount_generation), rehost_pending: Arc::clone(&self.hash_rehost_pending),
            rehost_revision: Arc::clone(&self.hash_rehost_revision), presentation_feedback: Arc::clone(&self.hash_presentation_feedback),
            native_geometry_sequence: Arc::clone(&self.hash_geometry_sequence), hash_tool_key: self.hash_tool_key,
            size_write_error: self.hash_size_write_error.clone(), root: root.clone(), mode: HashPresentationMode::Popout,
        });
        cx.spawn(async move |_, cx| {
            let _operation = operation;
            let fresh = existing.is_none();
            let target = if let Some((window, view, _)) = existing {
                Some((window, view))
            } else {
                let mut local = HashSession::new(); local.begin(Some(origin.receipt.path.clone()));
                let draft = HashDraft::from_session(&local);
                let Some(config) = config else { return; };
                let mut options = cx.update(|app| hash_popout_window_options(app)); options.show = false; options.focus = false;
                cx.update(|app| gpui_kit::open_window(options, app, move |window, app| {
                    let native = checked_native_window(window).ok().flatten();
                    create_hash_generator_view(window, app, draft, native, config)
                })).ok()
            };
            let Some((window, view)) = target else {
                let _ = root.update(cx, |root, cx| {
                    if root.hash_rehost_revision.load(Ordering::Acquire) != revision { return; }
                    root.release_direct_inherited_hash_transition(target_mount, epoch, revision, cx);
                    root.report_inherited_hash_origin_error(&origin, "Could not open Hash Generator.", cx);
                });
                return;
            };
            let native = cx.update(|app| view.read(app).native_window);
            let Some(native) = native else {
                if fresh { retire_unchecked_hash_candidate(&root, window, cx).await; }
                let _ = root.update(cx, |root, cx| {
                    if root.hash_rehost_revision.load(Ordering::Acquire) != revision { return; }
                    root.release_direct_inherited_hash_transition(target_mount, epoch, revision, cx);
                    root.report_inherited_hash_origin_error(&origin, "Hash Generator has no checked native window.", cx);
                });
                return;
            };
            let state = checked_hash_popup_state(native);
            let target_unavailable = !fresh && state.is_none_or(|(visible, _)| !visible);
            let prepared = prepare_direct_hash_popup(fresh, state, || {
                root.update(cx, |root, cx| {
                    let target_current = if fresh { matches!(root.hash_window, HashWindowState::Closed) }
                        else { matches!(&root.hash_window, HashWindowState::Popout { window: current, view: mounted, mount_generation }
                            if current.window_id() == window.window_id() && mounted.entity_id() == view.entity_id() && *mount_generation == target_mount)
                            && root.hash_mount_generation.load(Ordering::Acquire) == target_mount };
                    target_current && root.hash_rehost_pending.load(Ordering::Acquire)
                        && root.hash_rehost_revision.load(Ordering::Acquire) == revision
                        && root.hash_session.lock().unwrap_or_else(std::sync::PoisonError::into_inner).epoch() == epoch
                        && view.read(cx).native_window == Some(native) && native_window_is_valid(native)
                        && root.hash_inherited_origin_current(&origin, origin.native.identity, cx)
                }).unwrap_or(false)
            }, || restore_direct_hash_popup(fresh, checked_hash_popup_state(native), || set_native_window_visible(native, true).is_ok()),
                || checked_hash_popup_visible(native));
            let accepted = prepared && activate_for_secondary_window_if(&transition_revision, revision, native, Some(origin.native.identity), &|| {
                transition_pending.load(Ordering::Acquire) && !admission.is_fenced()
                    && origin.mount.is_current() && native_window_is_valid(origin.native)
                    && active_mount.load(Ordering::Acquire) == (if fresh { initial_mount } else { target_mount })
                    && transition_session.lock().unwrap_or_else(std::sync::PoisonError::into_inner).epoch() == epoch
                    && checked_hash_popup_visible(native)
                    && current_foreground_identity() == Some(origin.native.identity)
            })
                && current_foreground_identity() == Some(native.identity) && checked_hash_popup_visible(native);
            let committed = if accepted { root.update(cx, |root, cx| {
                let target_current = if fresh { matches!(root.hash_window, HashWindowState::Closed) }
                    else { matches!(&root.hash_window, HashWindowState::Popout { window: current, view: mounted, mount_generation }
                        if current.window_id() == window.window_id() && mounted.entity_id() == view.entity_id() && *mount_generation == target_mount) };
                let slot = root.hash_rehost_pending.load(Ordering::Acquire);
                let same_revision = root.hash_rehost_revision.load(Ordering::Acquire) == revision;
                let native_current = native_window_is_valid(native) && checked_hash_popup_visible(native);
                let epoch_current = root.hash_session.lock().unwrap_or_else(std::sync::PoisonError::into_inner).epoch() == epoch;
                let origin_current = target_current && slot && same_revision && native_current && epoch_current
                    && root.hash_inherited_origin_current(&origin, native.identity, cx);
                if !direct_inherited_hash_commit_current(accepted, slot, same_revision, target_current, native_current, epoch_current, origin_current) { return false; }
                if fresh {
                    root.hash_session.lock().unwrap_or_else(std::sync::PoisonError::into_inner).begin(Some(origin.receipt.path.clone()));
                    root.hash_mount_generation.store(target_mount, Ordering::Release);
                    let subscription = root.observe_hash_popout_close(window, Some(native), target_mount, candidate_epoch, cx);
                    root.hash_window = HashWindowState::Popout { mount_generation: target_mount, window, view: view.clone() };
                    root.hash_window_close_subscriptions.push((window, subscription));
                } else if update_kit_window_content(window, &view, cx, |tool, window, cx| {
                    tool.apply_file_selection(origin.receipt.path.clone(), window, cx); tool.set_editors_readonly(false, cx);
                }).is_err() { return false; }
                root.hash_rehost_pending.store(false, Ordering::Release);
                if let Some(source) = root.file_search.clone().filter(|view| view.entity_id() == origin.receipt.owner) {
                    source.update(cx, |view, cx| view.clear_hash_error(cx));
                }
                root.hash_return = Some(HashReturnReceipt::FileSearchPopup { receipt: origin.receipt.clone(), mount: origin.mount.clone(), host: origin.host, native: origin.native });
                root.remember_host_presentation("hash-generator", multi_launcher::settings::RememberedPresentation::Popout, cx);
                panel_stack::promote(&mut root.panel_stack, Panel::HashGeneratorDialog);
                root.hash_wake_signal.notify(); cx.notify(); true
            }).unwrap_or(false) } else { false };
            if committed {
                let _ = update_kit_window_content(window, &view, cx, |tool, window, cx| tool.focus_editable(window, cx));
                if fresh { let _ = root.update(cx, |root, cx| root.schedule_hash_popout_setup(native, view, target_mount, candidate_epoch, _operation, cx)); }
                return;
            }
            if fresh {
                let candidate = RetiringHashWindow { window, native_window: Some(native) };
                if !remove_and_wait_for_hash_window(cx, candidate).await { let _ = root.update(cx, |root, _| root.hash_retiring_windows.push(candidate)); }
            }
            let _ = root.update(cx, |root, cx| {
                let error = if target_unavailable { "The Hash Generator window is not available." }
                    else { "File Search or Hash Generator changed before activation completed." };
                if root.hash_rehost_revision.load(Ordering::Acquire) != revision { return; }
                root.release_direct_inherited_hash_transition(target_mount, epoch, revision, cx);
                root.report_inherited_hash_origin_error(&origin, if target_unavailable { error } else { "Hash Generator could not be activated." }, cx);
            });
        }).detach();
    }

    fn hash_inherited_origin_current(&self, origin: &HashInheritedOrigin, foreground: ForegroundIdentity, cx: &mut Context<Self>) -> bool {
        if self.restart_admission.is_fenced() || !origin.mount.is_current() || !native_window_is_valid(origin.native)
            || current_foreground_identity() != Some(foreground) || self.action_sheet.is_some() || self.context_favorite_popup.is_some() { return false; }
        let current = self.file_search.as_ref().is_some_and(|view| {
            view.entity_id() == origin.receipt.owner && view.read(cx).is_mounted_in(origin.host.window_id(), true)
                && view.read(cx).receipt_is_current(&origin.receipt, view.entity_id())
        });
        current && origin.host.update(cx, |_, window, app| {
            !window.has_active_dialog(app) && origin.focus.is_focused(window)
        }).unwrap_or(false)
    }

    fn start_hash_rehost(
        &mut self,
        target: HashPresentationMode,
        source_mount_generation: u64,
        session_epoch: u64,
        revision: u64,
        dock_gesture: Option<hash_dock::Gesture>,
        inherited: Option<HashInheritedOrigin>,
        operation: EffectPermit,
        cx: &mut Context<Self>,
    ) {
        self.drain_hash_native_geometry(cx, false);
        if self.hash_rehost_revision.load(Ordering::Acquire) != revision {
            return;
        }
        if !self.hash_rehost_pending.load(Ordering::Acquire)
            || self.hash_mount_generation.load(Ordering::Acquire) != source_mount_generation
            || self.hash_window.mount_generation() != Some(source_mount_generation)
        {
            self.restore_hash_transition(
                source_mount_generation,
                session_epoch,
                revision,
                "Hash Generator changed before the requested presentation could start.".to_owned(),
                cx,
            );
            return;
        }
        self.cancel_docked_hash_resize_for_mount(source_mount_generation, session_epoch);
        let source_mode = self.hash_window.mode();
        if source_mode == Some(target) {
            self.hash_rehost_pending.store(false, Ordering::Release);
            if let Some(view) = self.hash_window.view().cloned() {
                view.update(cx, |tool, cx| {
                    tool.set_editors_readonly(false, cx);
                    cx.notify();
                });
            }
            return;
        }
        let session_is_current = self
            .hash_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .epoch()
            == session_epoch;
        if !session_is_current {
            self.restore_hash_transition(
                source_mount_generation,
                session_epoch,
                revision,
                "Hash Generator session changed before rehosting. Reopen it and try again."
                    .to_owned(),
                cx,
            );
            return;
        }

        let Some(root_window) = self.root_window else {
            self.restore_hash_transition(
                source_mount_generation,
                session_epoch,
                revision,
                "The GPUI root window is unavailable; Hash Generator could not be docked or popped out."
                    .to_owned(),
                cx,
            );
            return;
        };
        let mut initial_draft = {
            let session = self
                .hash_session
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            HashDraft::from_session(&session)
        };
        if let Some(origin) = &inherited { initial_draft.mode = HashInputMode::File; initial_draft.file_path = Some(origin.receipt.path.clone()); }
        let destination_mount_generation = self.next_hash_mount_generation();
        let destination_session_epoch = session_epoch;
        let root = cx.entity().downgrade();
        let session = Arc::clone(&self.hash_session);
        let wake_signal = Arc::clone(&self.hash_wake_signal);
        let file_picker_pending = Arc::clone(&self.hash_file_picker_pending);
        let clipboard_pending = Arc::clone(&self.hash_clipboard_pending);
        let pinned = Arc::clone(&self.hash_pin.saved);
        let restart_admission = Arc::clone(&self.restart_admission);
        let active_mount_generation = Arc::clone(&self.hash_mount_generation);
        let rehost_pending = Arc::clone(&self.hash_rehost_pending);
        let rehost_revision = Arc::clone(&self.hash_rehost_revision);
        let presentation_feedback = Arc::clone(&self.hash_presentation_feedback);
        let native_geometry_sequence = Arc::clone(&self.hash_geometry_sequence);
        let hash_tool_key = self.hash_tool_key;
        let restore_geometry = self.restored_hash_geometry();
        let size_write_error = self.hash_size_write_error.clone();
        let utility_presentations = self.utility_presentations.clone();
        let root_visibility = self.root_visibility.clone();
        let jump_controller = self.search_jump_mode.fresh();
        let setup_session = Arc::clone(&session);
        let setup_active_mount_generation = Arc::clone(&active_mount_generation);
        let setup_restart_admission = Arc::clone(&restart_admission);
        cx.notify();

        cx.spawn(async move |_, cx| {
            let _operation = operation;
            let mount_config = HashMountConfig {
                jump_controller,
                session,
                wake_signal,
                file_picker_pending,
                clipboard_pending,
                pinned,
                restart_admission,
                mount_generation: destination_mount_generation,
                session_epoch: destination_session_epoch,
                active_mount_generation,
                rehost_pending: Arc::clone(&rehost_pending),
                rehost_revision,
                presentation_feedback: Arc::clone(&presentation_feedback),
                native_geometry_sequence,
                hash_tool_key,
                size_write_error,
                root: root.clone(),
                mode: target,
            };
            let built = match target {
                HashPresentationMode::Docked => {
                    let config = mount_config;
                    match cx.update(|app| {
                        root_window.update(app, |_, window, app| {
                            create_hash_generator_view(
                                window,
                                app,
                                initial_draft,
                                None,
                                config,
                            )
                        })
                    }) {
                        Ok(view) => Ok(BuiltHashMount::Docked { view }),
                        Err(error) => Err(format!(
                            "Could not build the docked Hash Generator view: {error}"
                        )),
                    }
                }
                HashPresentationMode::Popout => {
                    let mut options = cx.update(|app| hash_popout_window_options(app));
                    if inherited.is_some() { options.show = false; options.focus = false; }
                    let config = mount_config;
                    match cx.update(|app| {
                        gpui_kit::open_window(options, app, move |window, app| {
                            let native_window = checked_native_window(window).ok().flatten();
                            create_hash_generator_view(
                                window,
                                app,
                                initial_draft,
                                native_window,
                                config,
                            )
                        })
                    }) {
                        Ok((window, view)) => {
                            let native_window = cx.update(|app| view.read(app).native_window);
                            if cfg!(windows) && native_window.is_none() {
                                let _ = view.update(cx, |tool, _| {
                                    let _ = tool.take_native_geometry();
                                    tool.detach_native_geometry();
                                });
                                let candidate = RetiringHashWindow {
                                    window,
                                    native_window,
                                };
                                if inherited.is_some() {
                                    retire_unchecked_hash_candidate(&root, window, cx).await;
                                } else if !remove_and_wait_for_hash_window(cx, candidate).await {
                                    let _ = root.update(cx, |root, cx| {
                                        root.hash_retiring_windows.push(candidate);
                                        cx.notify();
                                    });
                                }
                                Err("The Hash Generator popout did not provide a checked Windows handle.".to_owned())
                            } else {
                                match root.update(cx, |root, cx| {
                                    root.observe_hash_popout_close(
                                        window,
                                        native_window,
                                        destination_mount_generation,
                                        destination_session_epoch,
                                        cx,
                                    )
                                }) {
                                    Ok(close_subscription) => Ok(BuiltHashMount::Popout {
                                        window,
                                        view,
                                        native_window,
                                        close_subscription,
                                    }),
                                    Err(error) => {
                                        let _ = view.update(cx, |tool, _| {
                                            let _ = tool.take_native_geometry();
                                            tool.detach_native_geometry();
                                        });
                                        let candidate = RetiringHashWindow {
                                            window,
                                            native_window,
                                        };
                                        if !remove_and_wait_for_hash_window(cx, candidate).await {
                                            let _ = root.update(cx, |root, cx| {
                                                root.hash_retiring_windows.push(candidate);
                                                cx.notify();
                                            });
                                        }
                                        Err(format!(
                                            "Could not install the Hash Generator close owner: {error}"
                                        ))
                                    }
                                }
                            }
                        }
                        Err(error) => Err(format!(
                            "Could not open the Hash Generator popout: {error}"
                        )),
                    }
                }
            };

            let built = match built {
                Ok(built) => built,
                Err(error) => {
                    let _ = root.update(cx, |root, cx| {
                        root.restore_hash_transition(
                            source_mount_generation,
                            session_epoch,
                            revision,
                            error,
                            cx,
                        );
                    });
                    return;
                }
            };

            let new_view = match &built {
                BuiltHashMount::Docked { view } | BuiltHashMount::Popout { view, .. } => {
                    view.clone()
                }
            };
            let candidate_popout = match &built {
                BuiltHashMount::Docked { .. } => None,
                BuiltHashMount::Popout {
                    window,
                    native_window,
                    ..
                } => Some(RetiringHashWindow {
                    window: *window,
                    native_window: *native_window,
                }),
            };
            let candidate_native_window = candidate_popout.and_then(|candidate| candidate.native_window);
            let (new_state, new_popout_subscription) = match built {
                BuiltHashMount::Docked { view } => (
                    HashWindowState::Docked {
                        mount_generation: destination_mount_generation,
                        view,
                    },
                    None,
                ),
                BuiltHashMount::Popout {
                    window,
                    view,
                    close_subscription,
                    ..
                } => (
                    HashWindowState::Popout {
                        mount_generation: destination_mount_generation,
                        window,
                        view,
                    },
                    Some((window, close_subscription)),
                ),
            };
            let inherited_ready = if let Some(origin) = inherited.as_ref() {
                let current = root.update(cx, |root, cx| {
                    root.hash_rehost_pending.load(Ordering::Acquire)
                        && root.hash_rehost_revision.load(Ordering::Acquire) == revision
                        && root.hash_mount_generation.load(Ordering::Acquire) == source_mount_generation
                        && root.hash_window.mount_generation() == Some(source_mount_generation)
                        && root.hash_inherited_origin_current(origin, origin.native.identity, cx)
                }).unwrap_or(false);
                if let Some(native) = candidate_native_window {
                    current && native_window_is_valid(native)
                        && current_foreground_identity() == Some(origin.native.identity)
                        && set_native_window_visible(native, true).is_ok()
                        && current_foreground_identity() == Some(origin.native.identity)
                        && activate_native_window(native).is_ok()
                        && current_foreground_identity() == Some(native.identity)
                } else { false }
            } else { true };
            let commit = root.update(cx, |root, cx| {
                let still_current = root.hash_rehost_pending.load(Ordering::Acquire)
                    && root.hash_rehost_revision.load(Ordering::Acquire) == revision
                    && root.hash_mount_generation.load(Ordering::Acquire)
                        == source_mount_generation
                    && root.hash_window.mount_generation() == Some(source_mount_generation)
                    && root
                        .hash_session
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .epoch()
                        == session_epoch;
                if !still_current { return Err("The active Hash Generator mount changed before rehosting completed.".to_owned()); }
                if !inherited_ready { return Err("The initiating File Search changed before rehosting completed.".to_owned()); }
                if let Some(origin) = inherited.as_ref() {
                    let Some(candidate) = candidate_native_window else { return Err("Hash Generator has no checked native window.".into()); };
                    if !native_window_is_valid(candidate) || !root.hash_inherited_origin_current(origin, candidate.identity, cx) {
                        return Err("File Search or Hash Generator changed before the handoff completed.".into());
                    }
                    commit_inherited_hash_file(&mut root.hash_session.lock().unwrap_or_else(std::sync::PoisonError::into_inner),
                        &origin.receipt.path, true);
                    root.hash_return = Some(HashReturnReceipt::FileSearchPopup { receipt: origin.receipt.clone(),
                        mount: origin.mount.clone(), host: origin.host, native: origin.native });
                }

                root.drain_hash_native_geometry(cx, true);
                let old_state = std::mem::replace(&mut root.hash_window, new_state);
                let new_geometry_error = new_view.read(cx).native_geometry_error.clone();
                root.hash_native_geometry_error = new_geometry_error;
                let retiring = match old_state {
                    HashWindowState::Popout { window, view, .. } => {
                        let retiring = RetiringHashWindow {
                            window,
                            native_window: view.read(cx).native_window,
                        };
                        root.hash_retiring_windows.push(retiring);
                        Some(retiring)
                    }
                    _ => None,
                };
                root.hash_mount_generation
                    .store(destination_mount_generation, Ordering::Release);
                root.hash_rehost_pending.store(false, Ordering::Release);
                if let Some((window, subscription)) = new_popout_subscription {
                    root.hash_window_close_subscriptions
                        .push((window, subscription));
                }
                new_view.update(cx, |tool, cx| {
                    tool.set_editors_readonly(false, cx);
                    cx.notify();
                });
                root.hash_open_error = None;
                root.hash_presentation_error = None;
                *root.hash_presentation_feedback
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
                let preference_update = root
                    .utility_presentations
                    .remember(hash_tool_key, target.remembered());
                match preference_update {
                    UtilityPresentationUpdate::Unchanged => {}
                    UtilityPresentationUpdate::Pending => {
                        *root.hash_presentation_feedback
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner) =
                            Some("Saving Hash Generator presentation preference…".to_owned());
                    }
                    UtilityPresentationUpdate::ImmediateFailure => {
                        let failures = root.utility_presentations.poll();
                        root.report_presentation_failures(failures, cx);
                    }
                }
                let show_root = target == HashPresentationMode::Docked
                    && source_mode == Some(HashPresentationMode::Popout)
                    && !root.root_visibility.visible.load(Ordering::Acquire);
                root.notify_hash_mount(cx);
                root.hash_wake_signal.notify();
                Ok((retiring, preference_update, show_root))
            });

            let (retiring, preference_update, show_root) = match commit {
                Ok(Ok(committed)) => committed,
                Ok(Err(error)) => {
                    let _ = new_view.update(cx, |tool, _| {
                        let _ = tool.take_native_geometry();
                        tool.detach_native_geometry();
                    });
                    if let Some(candidate) = candidate_popout {
                        let retired = remove_and_wait_for_hash_window(cx, candidate).await;
                        if !retired {
                            let _ = root.update(cx, |root, cx| {
                                root.hash_retiring_windows.push(candidate);
                                cx.notify();
                            });
                        }
                    }
                    let _ = root.update(cx, |root, cx| {
                        root.restore_hash_transition(
                            source_mount_generation,
                            session_epoch,
                            revision,
                            error,
                            cx,
                        );
                    });
                    return;
                }
                Err(error) => {
                    let _ = root.update(cx, |root, cx| {
                        root.restore_hash_transition(
                            source_mount_generation,
                            session_epoch,
                            revision,
                            format!("Could not commit the Hash Generator presentation: {error}"),
                            cx,
                        );
                    });
                    return;
                }
            };

            if let Some(native_window) = candidate_native_window {
                let still_current = || {
                    setup_active_mount_generation.load(Ordering::Acquire)
                        == destination_mount_generation
                        && setup_session
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .epoch()
                            == destination_session_epoch
                        && !setup_restart_admission.is_fenced()
                };
                if still_current() {
                    // Native placement can synchronously deliver WM_DPICHANGED,
                    // so it must run after the root commit and outside every
                    // App, Window, and entity update closure.
                    let placement_error = checked_geometry_target(native_window)
                        .ok_or_else(|| {
                            "the Hash Generator HWND could not be checked".to_owned()
                        })
                        .and_then(|target| {
                            restore_native_window_geometry(
                                target,
                                restore_geometry,
                                (420, 320),
                                1.0,
                            )
                            .map(|_| ())
                        })
                        .err()
                        .map(|error| {
                            format!("Could not restore the Hash Generator popout position: {error}")
                        });
                    if still_current() {
                        let setup_error = new_view.update(cx, |tool, cx| {
                            if !tool.is_current_mount() || !still_current() {
                                return None;
                            }
                            let mut errors = Vec::new();
                            if let Some(error) = tool.native_geometry_error.take() {
                                errors.push(error);
                            }
                            if let Some(error) = placement_error.clone() {
                                errors.push(error);
                            }
                            if let Err(error) = tool.attach_native_geometry() {
                                errors.push(format!(
                                    "Native move/resize persistence is unavailable: {error}"
                                ));
                            }
                            let message = (!errors.is_empty()).then(|| errors.join(" "));
                            tool.native_geometry_error = message.clone();
                            cx.notify();
                            message
                        });
                        if let Some(error) = setup_error {
                            let _ = root.update(cx, |root, cx| {
                                if root.hash_window.mount_generation()
                                    == Some(destination_mount_generation)
                                {
                                    root.hash_native_geometry_error = Some(error);
                                    cx.notify();
                                }
                            });
                        }
                    }
                }
            }

            let reveal_ack = if show_root {
                dock_gesture.and_then(|gesture| root_visibility.request_hash_visible(gesture, destination_mount_generation))
            } else { None };
            if show_root && reveal_ack.is_none() {
                let _ = root.update(cx, |root, cx| {
                    let message = "Hash Generator is docked, but the root window could not be shown by its visibility owner.".to_owned();
                    root.hash_open_error = Some(message.clone());
                    *root.hash_presentation_feedback
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(message);
                    cx.notify();
                });
            }

            if let Some(receiver) = reveal_ack {
                receiver.wait().await;
            }

            if inherited.is_none() && source_mode == Some(HashPresentationMode::Docked)
                && target == HashPresentationMode::Popout
                && let Some(root_view) = root.upgrade()
            {
                let focus_result = update_kit_window_content(
                    root_window,
                    &root_view,
                    cx,
                    |root, window, cx| {
                        let root_handle_matches = root.root_window.is_some_and(|handle| {
                            handle.window_id() == root_window.window_id()
                        });
                        let mount_matches = root_search_focus_is_current(
                            root.hash_window.mode(),
                            root.hash_window.mount_generation(),
                            root.hash_mount_generation.load(Ordering::Acquire),
                            Some(HashPresentationMode::Popout),
                            destination_mount_generation,
                            root.restart_admission.is_fenced(),
                        );
                        let native_root_is_current = !cfg!(windows)
                            || root.native_window.is_some_and(native_window_is_valid);
                        if root_handle_matches
                            && root.action_sheet.is_none()
                            && mount_matches
                            && native_root_is_current
                        {
                            root.query.focus_handle(cx).focus(window, cx);
                        }
                    },
                );
                if let Err(error) = focus_result {
                    tracing::debug!(%error, "could not restore GPUI root focus after popping out Hash Generator");
                }
            }

            if let Some(retiring) = retiring {
                let retired = remove_and_wait_for_hash_window(cx, retiring).await;
                if retired {
                    let _ = root.update(cx, |root, cx| {
                        root.hash_retiring_windows.retain(|window| {
                            window.window.window_id() != retiring.window.window_id()
                        });
                        cx.notify();
                    });
                } else {
                    let _ = root.update(cx, |root, cx| {
                        let message = "The previous Hash Generator popout has not retired. Close it before restarting in egui.".to_owned();
                        root.hash_open_error = Some(message.clone());
                        *root.hash_presentation_feedback
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(message);
                        cx.notify();
                    });
                }
            }

            if preference_update == UtilityPresentationUpdate::Pending {
                while utility_presentations.has_pending() {
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;
                    let failures = utility_presentations.poll();
                    if !failures.is_empty() {
                        let _ = root.update(cx, |root, cx| {
                            root.report_presentation_failures(failures, cx);
                        root.notify_hash_mount(cx);
                        });
                    }
                }
                let _ = root.update(cx, |root, cx| {
                    if root.hash_presentation_error.is_none() {
                        *root.hash_presentation_feedback
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
                    }
                    root.notify_hash_mount(cx);
                });
            }
        })
        .detach();
    }

    fn hash_return_activation_origin(&self, child: NativeWindow, cx: &App) -> Option<HashInheritedOrigin> {
        let HashReturnReceipt::FileSearchPopup { receipt, mount, host, native } = self.hash_return.as_ref()? else { return None; };
        if !mount.is_current() || self.restart_admission.is_fenced() || !native_window_is_valid(*native)
            || !native_window_is_valid(child) || current_foreground_identity() != Some(child.identity) { return None; }
        let view = self.file_search.as_ref().filter(|view| view.entity_id() == receipt.owner)?;
        if !view.read(cx).is_mounted_in(host.window_id(), true) || !view.read(cx).receipt_is_current(receipt, view.entity_id()) { return None; }
        Some(HashInheritedOrigin { receipt: receipt.clone(), mount: mount.clone(), host: *host, native: *native,
            focus: view.read(cx).hash_action_focus(cx) })
    }

    fn start_hash_close(
        &mut self,
        mount_generation: u64,
        session_epoch: u64,
        revision: u64,
        origin: hash_pin::CloseOrigin,
        operation: EffectPermit,
        cx: &mut Context<Self>,
    ) {
        if !hash_pin::allows_close(self.hash_pin.is_saved(), origin) {
            self.restore_hash_transition(mount_generation, session_epoch, revision, "Hash Generator is pinned.".to_owned(), cx);
            return;
        }
        if self.hash_rehost_revision.load(Ordering::Acquire) != revision
            || !self.hash_rehost_pending.load(Ordering::Acquire)
            || self.hash_mount_generation.load(Ordering::Acquire) != mount_generation
            || self.hash_window.mount_generation() != Some(mount_generation)
        {
            self.restore_hash_transition(
                mount_generation,
                session_epoch,
                revision,
                "Hash Generator changed before it could close.".to_owned(),
                cx,
            );
            return;
        }
        let epoch_matches = self
            .hash_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .epoch()
            == session_epoch;
        if !epoch_matches {
            self.restore_hash_transition(
                mount_generation,
                session_epoch,
                revision,
                "Hash Generator session changed before it could close.".to_owned(),
                cx,
            );
            return;
        }
        let remount = self.hash_pin.is_saved() && origin == hash_pin::CloseOrigin::Button;
        let return_activation = match &self.hash_window {
            HashWindowState::Popout { view, .. } if inherited_hash_parent_return_allowed(remount, origin == hash_pin::CloseOrigin::Unpin) => view.read(cx).native_window.and_then(|child|
                self.hash_return_activation_origin(child, cx).map(|origin| (origin, child))),
            _ => None,
        };
        self.cancel_docked_hash_resize_for_mount(mount_generation, session_epoch);
        self.drain_hash_native_geometry(cx, true);
        let retired_docked_focus = match &self.hash_window {
            HashWindowState::Docked { view, .. } => Some(view.read(cx).focus_handle.clone()),
            _ => None,
        };
        let retiring = match &self.hash_window {
            HashWindowState::Popout { window, view, .. } => Some(RetiringHashWindow {
                window: *window,
                native_window: view.read(cx).native_window,
            }),
            _ => None,
        };
        if let Some(retiring) = retiring {
            self.hash_retiring_windows.push(retiring);
        }
        self.hash_mount_generation.store(0, Ordering::Release);
        self.hash_rehost_pending.store(false, Ordering::Release);
        self.hash_window = HashWindowState::Closed;
        panel_stack::retire(&mut self.panel_stack, Panel::HashGeneratorDialog);
        self.hash_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .end();
        let close_revision = self.hash_window_revision
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        self.hash_pin.remount_revision = remount.then_some(close_revision);
        let return_receipt = self.hash_return.take();
        self.hash_open_error = None;
        cx.notify();

        let root = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let _operation = operation;
            if let Some((origin, child)) = return_activation {
                let allowed = root.update(cx, |root, cx| root.hash_window_revision.load(Ordering::Acquire) == close_revision
                    && root.hash_window.mode().is_none() && root.hash_inherited_origin_current(&origin, child.identity, cx)).unwrap_or(false);
                if allowed && origin.mount.is_current() && native_window_is_valid(child)
                    && current_foreground_identity() == Some(child.identity) { let _ = activate_native_window(origin.native); }
            }
            let retired = if let Some(retiring) = retiring {
                let retired = remove_and_wait_for_hash_window(cx, retiring).await;
                let _ = root.update(cx, |root, cx| {
                    if retired {
                        root.hash_retiring_windows.retain(|window| {
                            window.window.window_id() != retiring.window.window_id()
                        });
                    } else {
                        let message =
                            "Hash Generator closed, but its popout window did not retire."
                                .to_owned();
                        root.hash_open_error = Some(message.clone());
                        *root
                            .hash_presentation_feedback
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(message);
                    }
                    cx.notify();
                });
                retired
            } else {
                true
            };
            if retired {
                if remount {
                    let target = root.update(cx, |root, _| root.root_window).ok().flatten();
                    if let (Some(target), Some(view)) = (target, root.upgrade()) {
                        let _ = update_kit_window_content(target, &view, cx, |root, window, cx| {
                            root.remount_pinned_hash_if_ready(window, cx);
                        });
                    }
                } else if origin != hash_pin::CloseOrigin::Unpin {
                    restore_root_search_focus_after_hash_close(
                        root.clone(), close_revision, return_receipt, retired_docked_focus, cx,
                    ).await;
                }
            }
        })
        .detach();
    }

    fn remount_pinned_hash_if_ready(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let revision = self.hash_window_revision.load(Ordering::Acquire);
        if !hash_pin::remount_is_current(
            self.hash_pin.is_saved(), self.hash_pin.remount_revision, revision,
            matches!(self.hash_window, HashWindowState::Closed), self.hash_retiring_windows.is_empty(),
            self.restart_admission.is_fenced(),
        ) || !self.root_visibility.active.load(Ordering::Acquire)
            || !hash_pin::remount_visibility_allows(self.root_visibility.visible.load(Ordering::Acquire))
            || !self.root_window.is_some_and(|root| root.window_id() == window.window_handle().window_id())
            || (cfg!(windows) && !self.native_window.is_some_and(native_window_is_valid))
            || self.hash_file_picker_pending.load(Ordering::Acquire)
            || self.hash_rehost_pending.load(Ordering::Acquire) { return; }
        let Some(operation) = self.restart_admission.admit() else { return; };
        self.hash_pin.remount_revision = None;
        if let Err(error) = self.open_hash_generator_with_mount_intent(
            HashOpenIntent::Default, operation, window, hash_pin::HashMountIntent::Passive, cx,
        ) {
            self.hash_open_error = Some(error);
        }
        cx.notify();
    }

    fn toggle_coordinate_hud_probe(&mut self, cx: &mut Context<Self>) {
        if self.coordinate_hud_probe_pending
            || self.coordinate_hud_probe_closing.load(Ordering::Acquire)
        {
            return;
        }
        let Some(operation) = self.restart_admission.admit() else {
            self.coordinate_hud_probe_status =
                "GPUI is closing; Coordinate HUD changes are disabled.".to_owned();
            cx.notify();
            return;
        };

        self.coordinate_hud_probe_pending = true;
        self.coordinate_hud_probe_status = "Updating Coordinate HUD…".to_owned();
        cx.notify();

        let controller = Arc::clone(&self.coordinate_hud_probe_controller);
        let closing = Arc::clone(&self.coordinate_hud_probe_closing);
        cx.spawn(async move |this, cx| {
            let _operation = operation;
            let result = cx
                .background_executor()
                .spawn(async move {
                    if closing.load(Ordering::Acquire) {
                        return Err("launcher is closing".to_owned());
                    }
                    let mut controller = controller
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if closing.load(Ordering::Acquire) {
                        return Err("launcher is closing".to_owned());
                    }
                    toggle_coordinate_hud_probe(&mut controller)
                })
                .await;
            let status = match result {
                Ok(status) => status,
                Err(error) => format!("Coordinate HUD error: {error}"),
            };
            let _ = this.update(cx, |this, cx| {
                this.coordinate_hud_probe_pending = false;
                this.coordinate_hud_probe_status = status;
                cx.notify();
            });
        })
        .detach();
    }

    fn open_or_toggle_secondary(&mut self, cx: &mut Context<Self>) {
        let Some(operation) = self.restart_admission.admit() else {
            self.restart_status =
                Some("GPUI is closing; utility window changes are disabled.".to_owned());
            cx.notify();
            return;
        };
        let foreground_before_request = current_foreground_identity();
        let (handle, view) = match &self.secondary {
            SecondaryWindowState::Closed => {
                self.open_secondary(operation, cx);
                return;
            }
            SecondaryWindowState::OpenPending => return,
            SecondaryWindowState::Live { window, view } => (*window, view.clone()),
        };

        {
            let state = update_kit_window_content(handle, &view, cx, |secondary, window, cx| {
                let is_visible = secondary
                    .native_window
                    .and_then(|native| native_window_is_visible(native).ok())
                    .unwrap_or(true);
                if !is_visible {
                    secondary.query.focus_handle(cx).focus(window, cx);
                }
                (secondary.native_window, is_visible)
            });

            if let Ok((Some(native_window), was_visible)) = state {
                let show = !was_visible;
                let secondary_revision = Arc::clone(&self.secondary_revision);
                let revision = secondary_revision.fetch_add(1, Ordering::AcqRel) + 1;
                cx.spawn(async move |_, cx| {
                    let _operation = operation;
                    if secondary_revision.load(Ordering::Acquire) != revision {
                        return;
                    }
                    if let Err(error) = set_native_window_visible(native_window, show) {
                        tracing::warn!(%error, "could not change the retained GPUI window visibility");
                        return;
                    }
                    if show {
                        let expected_after = current_foreground_identity()
                            .filter(|foreground| Some(*foreground) == foreground_before_request);
                        let secondary_revision = Arc::clone(&secondary_revision);
                        cx.background_executor()
                            .spawn(async move {
                                activate_for_secondary_window(
                                    &secondary_revision,
                                    revision,
                                    native_window,
                                    expected_after,
                                );
                            })
                            .await;
                    }
                })
                .detach();
                return;
            }

            match state {
                Ok(_) => {
                    self.secondary_error = Some(
                        "The retained window has no usable native handle on this platform."
                            .to_owned(),
                    );
                    cx.notify();
                    return;
                }
                Err(_) => {
                    self.secondary = SecondaryWindowState::Closed;
                    self.secondary_revision.fetch_add(1, Ordering::AcqRel);
                    self.secondary_close_subscription = None;
                }
            }
        }

        self.open_secondary(operation, cx);
    }

    fn open_secondary(&mut self, operation: EffectPermit, cx: &mut Context<Self>) {
        if !matches!(self.secondary, SecondaryWindowState::Closed) {
            return;
        }
        self.secondary = SecondaryWindowState::OpenPending;
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Multi Launcher GPUI Retained Window".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let secondary_revision = Arc::clone(&self.secondary_revision);
        let restart_admission = Arc::clone(&self.restart_admission);
        cx.spawn(async move |this, cx| {
            let _operation = operation;
            let opened = cx.update(|cx| {
                gpui_kit::open_window(options, cx, |window, cx| {
                    let query = cx.new(|cx| {
                        InputState::new(window, cx).placeholder("Keep text while this window is open")
                    });
                    query.focus_handle(cx).focus(window, cx);
                    let native_window = match checked_native_window(window) {
                        Ok(native_window) => native_window,
                        Err(error) => {
                            tracing::error!(%error, "could not capture retained GPUI window handle");
                            None
                        }
                    };
                    cx.new(|_| SecondaryWindow {
                        query,
                        native_window,
                        secondary_revision: Arc::clone(&secondary_revision),
                        restart_admission: Arc::clone(&restart_admission),
                    })
                })
            });
            match opened {
                Ok((window, view)) => {
                    let window_id = window.window_id();
                    let root = this.clone();
                    let close_subscription = cx.update(|cx| {
                        cx.on_window_closed(move |cx, closed| {
                            if closed != window_id {
                                return;
                            }
                            let _ = root.update(cx, |root, cx| {
                                if matches!(
                                    &root.secondary,
                                    SecondaryWindowState::Live { window, .. }
                                        if window.window_id() == window_id
                                ) {
                                    root.secondary = SecondaryWindowState::Closed;
                                    root.secondary_revision.fetch_add(1, Ordering::AcqRel);
                                    root.secondary_close_subscription = None;
                                    cx.notify();
                                }
                            });
                        })
                    });
                    let _ = this.update(cx, |this, cx| {
                        if matches!(this.secondary, SecondaryWindowState::OpenPending) {
                            this.secondary = SecondaryWindowState::Live { window, view };
                            this.secondary_close_subscription = Some(close_subscription);
                            this.secondary_error = None;
                            cx.notify();
                        }
                    });
                }
                Err(error) => {
                    let _ = this.update(cx, |this, cx| {
                        if matches!(this.secondary, SecondaryWindowState::OpenPending) {
                            this.secondary = SecondaryWindowState::Closed;
                            this.secondary_error = Some(format!(
                                "Could not open the retained GPUI window: {error}"
                            ));
                            cx.notify();
                        }
                    });
                }
            }
        })
        .detach();
    }

    fn hash_clipboard_request_is_current(&self, request: HashClipboardRequest) -> bool {
        if self.restart_admission.is_fenced()
            || self.hash_window.mount_generation() != Some(request.mount_generation)
        {
            return false;
        }
        let session = self
            .hash_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        hash_clipboard_request_is_current(
            self.hash_mount_generation.load(Ordering::Acquire),
            session.epoch(),
            session.generation(),
            request,
            self.hash_rehost_pending.load(Ordering::Acquire),
        )
    }

    fn current_hash_clipboard_target(&self) -> Option<HashClipboardTarget> {
        let session_epoch = self
            .hash_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .epoch();
        match &self.hash_window {
            HashWindowState::Closed => None,
            HashWindowState::Docked {
                mount_generation,
                view,
            } => Some(HashClipboardTarget {
                window: self.root_window?,
                mount_generation: *mount_generation,
                session_epoch,
                view: view.clone(),
            }),
            HashWindowState::Popout {
                mount_generation,
                window,
                view,
            } => Some(HashClipboardTarget {
                window: *window,
                mount_generation: *mount_generation,
                session_epoch,
                view: view.clone(),
            }),
        }
    }

    fn start_hash_clipboard_operation(
        &mut self,
        request: HashClipboardRequest,
        kind: HashClipboardOperationKind,
        operation: EffectPermit,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.hash_clipboard_operation.is_some() || self.hash_clipboard_pending.is_pending() {
            return Err(
                "Another Hash Generator clipboard operation is already in progress.".into(),
            );
        }
        if !self.hash_clipboard_request_is_current(request) {
            return Err(
                "Hash Generator changed before the clipboard operation could start. Try again."
                    .into(),
            );
        }
        if let HashClipboardOperationKind::Copy(copy) = &kind {
            if !hash_clipboard_copy_admissible(
                self.hash_clipboard_pending.kind(),
                self.hash_file_picker_pending.load(Ordering::Acquire),
            ) {
                return Err(
                    "Finish the file picker or other Clipboard operation before copying the digest."
                        .into(),
                );
            }
            let session = self
                .hash_session
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !session.is_clipboard_copy_current(copy) {
                return Err(
                    "The current digest changed before it could be copied. Calculate again and retry."
                        .into(),
                );
            }
        }

        let pending_kind = match &kind {
            HashClipboardOperationKind::Load => HashClipboardPendingKind::Load,
            HashClipboardOperationKind::Copy(_) => HashClipboardPendingKind::Copy,
        };
        if !self.hash_clipboard_pending.begin(pending_kind) {
            return Err(
                "Another Hash Generator clipboard operation is already in progress.".into(),
            );
        }
        self.hash_clipboard_operation_counter =
            self.hash_clipboard_operation_counter.wrapping_add(1).max(1);
        let operation_id = self.hash_clipboard_operation_counter;
        let worker_kind = kind.clone();
        self.hash_clipboard_operation = Some(HashClipboardOperation {
            id: operation_id,
            request,
            kind,
        });
        let pending = Arc::clone(&self.hash_clipboard_pending);
        cx.spawn(async move |this, cx| {
            let _operation = operation;
            let outcome = match worker_kind {
                HashClipboardOperationKind::Load => {
                    let result = cx
                        .background_executor()
                        .spawn(async { ArboardClipboardBackend.read_text() })
                        .await;
                    HashClipboardOutcome::Load(result)
                }
                HashClipboardOperationKind::Copy(copy) => {
                    let digest = copy.digest().to_owned();
                    let result = cx
                        .background_executor()
                        .spawn(async move { ArboardClipboardBackend.write_text(&digest) })
                        .await;
                    HashClipboardOutcome::Copy(result)
                }
            };
            if this
                .update(cx, |root, cx| {
                    root.finish_hash_clipboard_operation(operation_id, outcome, cx);
                })
                .is_err()
            {
                pending.finish();
            }
        })
        .detach();
        Ok(())
    }

    fn finish_hash_clipboard_operation(
        &mut self,
        operation_id: u64,
        outcome: HashClipboardOutcome,
        cx: &mut Context<Self>,
    ) {
        if !self
            .hash_clipboard_operation
            .as_ref()
            .is_some_and(|operation| operation.id == operation_id)
        {
            return;
        }
        let Some(operation) = self.hash_clipboard_operation.take() else {
            return;
        };
        let request_is_current = self.hash_clipboard_request_is_current(operation.request);
        let is_copy = matches!(&operation.kind, HashClipboardOperationKind::Copy(_));
        let restore_copy_editors = is_copy
            && self.hash_window.mount_generation() == Some(operation.request.mount_generation)
            && self
                .hash_session
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .epoch()
                == operation.request.session_epoch;
        let load_was_applied = request_is_current
            && matches!(&operation.kind, HashClipboardOperationKind::Load)
            && matches!(&outcome, HashClipboardOutcome::Load(Ok(_)));

        if request_is_current {
            let mut session = self
                .hash_session
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match (operation.kind, outcome) {
                (HashClipboardOperationKind::Load, HashClipboardOutcome::Load(result)) => {
                    session.apply_clipboard_load(result);
                }
                (HashClipboardOperationKind::Copy(request), HashClipboardOutcome::Copy(result))
                    if session.is_clipboard_copy_current(&request) =>
                {
                    session.apply_clipboard_copy(&request, result);
                }
                _ => {}
            }
        }

        self.hash_clipboard_pending.finish();
        if let Some(target) = self.current_hash_clipboard_target() {
            let same_mount = target.mount_generation == operation.request.mount_generation
                && target.session_epoch == operation.request.session_epoch;
            if let Err(error) =
                update_kit_window_content(target.window, &target.view, cx, |tool, window, cx| {
                    if restore_copy_editors && same_mount {
                        tool.set_editors_readonly(false, cx);
                    }
                    if load_was_applied && same_mount {
                        tool.source.update(cx, |source, cx| {
                            source.set_selected_value(&HashSourceChoice::Clipboard, window, cx)
                        });
                    }
                    cx.notify();
                })
            {
                tracing::debug!(%error, "could not notify the current Hash view after clipboard work");
            }
        }
        cx.notify();
    }

    fn hash_picker_target(&self, request: HashPickerRequest) -> Option<HashPickerTarget> {
        if self.hash_rehost_pending.load(Ordering::Acquire)
            || self.restart_admission.is_fenced()
            || self.hash_mount_generation.load(Ordering::Acquire) != request.mount_generation
        {
            return None;
        }
        let session = self
            .hash_session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !hash_picker_result_is_current(
            self.hash_mount_generation.load(Ordering::Acquire),
            request.mount_generation,
            session.epoch(),
            request.session_epoch,
            session.generation(),
            request.source_generation,
        ) {
            return None;
        }
        drop(session);

        match &self.hash_window {
            HashWindowState::Docked {
                mount_generation,
                view,
            } if *mount_generation == request.mount_generation => Some(HashPickerTarget {
                mode: HashPresentationMode::Docked,
                window: self.root_window?,
                view: view.clone(),
            }),
            HashWindowState::Popout {
                mount_generation,
                window,
                view,
            } if *mount_generation == request.mount_generation => Some(HashPickerTarget {
                mode: HashPresentationMode::Popout,
                window: *window,
                view: view.clone(),
            }),
            _ => None,
        }
    }

    fn start_hash_file_picker<F, E>(
        &mut self,
        request: HashPickerRequest,
        picker: F,
        operation: EffectPermit,
        cx: &mut Context<Self>,
    ) where
        F: Future<Output = Result<anyhow::Result<Option<Vec<PathBuf>>>, E>> + 'static,
        E: 'static,
    {
        let picker_pending = Arc::clone(&self.hash_file_picker_pending);
        cx.spawn(async move |this, cx| {
            let _operation = operation;
            let mut picker_pending = HashPickerPendingGuard::new(picker_pending);
            let outcome = classify_hash_picker_result(picker.await);
            let target = this
                .update(cx, |root, _| root.hash_picker_target(request))
                .ok()
                .flatten();
            let Some(target) = target else {
                picker_pending.clear_and_notify(&this, cx);
                return;
            };

            let update_result = match target.mode {
                HashPresentationMode::Docked => {
                    let Some(root_view) = this.upgrade() else {
                        picker_pending.clear_and_notify(&this, cx);
                        return;
                    };
                    cx.update(|app| {
                        update_kit_window_content(
                            target.window,
                            &root_view,
                            app,
                            |root, window, cx| {
                                if root.hash_picker_target(request).is_none() {
                                    return;
                                }
                                if let Some(view) = root.hash_window.view().cloned() {
                                    view.update(cx, |tool, cx| {
                                        tool.resolve_file_picker(request, outcome, window, cx);
                                    });
                                }
                            },
                        )
                    })
                }
                HashPresentationMode::Popout => cx.update(|app| {
                    update_kit_window_content(
                        target.window,
                        &target.view,
                        app,
                        |tool, window, cx| {
                            tool.resolve_file_picker(request, outcome, window, cx);
                        },
                    )
                }),
            };
            picker_pending.clear_and_notify(&this, cx);
            if let Err(error) = update_result {
                tracing::debug!(%error, "could not apply Hash Generator file picker result");
            }
        })
        .detach();
    }

    fn request_quit(&mut self, cx: &mut Context<Self>) {
        self.retire_context_favorite();
        let blocker = self.current_root_close_blocker(
            hash_clipboard_close_message(self.hash_clipboard_pending.kind()),
            self.hash_file_picker_pending.load(Ordering::Acquire),
            cx,
        );
        match blocker {
            Some(RootCloseBlocker::HashClipboard(message)) => {
                self.restart_status = Some(message.to_owned());
            }
            Some(RootCloseBlocker::HashFilePicker) => {
                self.restart_status = Some(
                    "Finish or cancel the Hash Generator file picker before quitting Multi Launcher."
                        .to_owned(),
                );
            }
            Some(RootCloseBlocker::XmlFilePicker) => {
                self.restart_status = Some("Finish the XML file operation or overwrite review, or wait for its window to finish docking, before quitting Multi Launcher.".to_owned());
            }
            Some(RootCloseBlocker::ToastLogOpen) => {
                self.restart_status = Some(TOAST_LOG_OPEN_PENDING_STATUS.to_owned());
            }
            Some(RootCloseBlocker::FavoritesWrite) => {
                self.restart_status = Some(FAVORITES_WRITE_PENDING.to_owned());
            }
            Some(RootCloseBlocker::RegexHistory) => { self.restart_status = Some(regex_history::WAIT_QUIT.to_owned()); }
            Some(RootCloseBlocker::ClipboardModifyBlocked) => { self.restart_status = Some(clipboard_modify_view::BLOCK_WAIT.to_owned()); }
            Some(RootCloseBlocker::SettingsActionsSave) => { self.restart_status = Some(settings_actions::WAIT.to_owned()); }
            Some(RootCloseBlocker::FileSearchEffect) => { self.restart_status = Some(file_search_preview::WAIT.to_owned()); }
            Some(RootCloseBlocker::FolderSize) => { self.restart_status = Some(folder_size::WAIT.to_owned()); }
            Some(RootCloseBlocker::FileLock) => { self.restart_status = Some(file_lock::WAIT.to_owned()); }
            Some(RootCloseBlocker::TempfileSave) => { self.restart_status = Some(tempfile_view::WAIT.to_owned()); }
            Some(RootCloseBlocker::SnippetsSave) => { self.restart_status = Some(snippets::SAVE_WAIT.to_owned()); }
            Some(RootCloseBlocker::ClipboardHistory) => { self.restart_status = Some(clipboard_history::WAIT.to_owned()); }
            Some(RootCloseBlocker::RegexPresets) => { self.restart_status = Some(regex_presets::WAIT_QUIT.to_owned()); }
            Some(RootCloseBlocker::RegexClipboard) => { self.restart_status = Some("Wait for the Regex Tester clipboard operation before quitting.".to_owned()); }
            Some(RootCloseBlocker::RegexRehost) => { self.restart_status = Some("Wait for Regex Tester to finish changing windows before quitting.".to_owned()); }
            Some(RootCloseBlocker::StructuredPinWrite) => {
                self.restart_status = Some("Wait for the utility pin update to finish before quitting Multi Launcher.".to_owned());
            }
            Some(RootCloseBlocker::HashRehost) => { self.restart_status = Some("Wait for the Hash Generator candidate window to close.".into()); }
            Some(RootCloseBlocker::HashPinWrite) => {
                self.restart_status = Some(hash_pin::UNPIN_PENDING.to_owned());
            }
            Some(RootCloseBlocker::AppWrite) => {
                self.restart_status = Some(add_action::APP_WRITE_CLOSE_STATUS.to_owned());
            }
            Some(RootCloseBlocker::ExternalLaunch) => {
                self.restart_status = Some(external_launch::CLOSE_STATUS.to_owned());
            }
            Some(RootCloseBlocker::CalculatorSearch) => {
                self.restart_status = Some(calculator_search::SEARCH_PENDING_STATUS.to_owned());
            }
            Some(RootCloseBlocker::PluginPreferenceSave) => {
                self.restart_status = Some("Wait for the plugin Search capability save.".to_owned());
            }
            Some(RootCloseBlocker::BooleanPreferenceSave) => {
                if !self.prepare_boolean_preference_close(cx) {
                    return;
                }
            }
            None => {}
        }
        if blocker.is_some_and(|blocker| blocker != RootCloseBlocker::BooleanPreferenceSave) {
            cx.notify();
            return;
        }
        self.quit_after_xml_drain(cx);
    }

    fn quit_after_xml_drain(&mut self, cx: &mut Context<Self>) {
        if self.xml_terminal_quit { return; }
        self.xml_terminal_quit = true;
        self.snippets_search.wake.close();
        self.restart_admission.fence();
        self.root_visibility.deactivate();
        if let Some(view) = &self.file_search { view.read(cx).retire_explorer(); }
        self.retire_json_workspace(cx);
        self.drain_regex_native_geometry(cx, true);
        self.drain_file_search_geometry(cx, true);
        self.drain_folder_size_geometry(cx, true);
        self.drain_file_lock_geometry(cx, true);
        self.drain_file_preview_geometry(cx, true);
        if let Some(view) = &self.file_search { view.update(cx, |view, cx| view.shutdown(cx)); }
        if let Some(view) = &self.regex_view { view.update(cx, |view, _| view.shutdown()); }
        self.window_search.retire();
        self.file_search_popup_intent.store(false, Ordering::Release); self.file_search_probe.retire();
        self.indexed_applications.retire();
        self.hash_docked_resize.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
        self.drain_hash_native_geometry(cx, true);
        self.drain_xml_native_geometry(cx, true);
        self.drain_clipboard_native_geometry(cx, true);
        self.drain_clipboard_modify_geometry(cx, true);
        let mut retiring_clipboard = std::mem::take(&mut self.clipboard_retiring);
        if let Some(popup) = self.clipboard_popup.take() { retiring_clipboard.push(RetiringHashWindow { window: popup.window, native_window: popup.native }); }
        retiring_clipboard.extend(std::mem::take(&mut self.clipboard_modify_retiring));
        if let Some(popup) = self.clipboard_modify_popup.take() { retiring_clipboard.push(RetiringHashWindow { window: popup.window, native_window: popup.native }); }
        if let Some(view) = &self.clipboard_modify_view { view.update(cx, |view, _| view.shutdown_host()); }
        if let Some(view) = &self.clipboard_history { view.update(cx, |view, _| view.shutdown()); }
        let mut retiring_search = self.file_search.clone().map(|view| view.update(cx, |view, _| view.take_preview_retiring())).unwrap_or_default();
        retiring_search.extend(std::mem::take(&mut self.file_search_retiring));
        if let Some(popup) = self.file_search_popup.take() { retiring_search.push(RetiringHashWindow { window: popup.window, native_window: popup.native }); }
        retiring_search.extend(std::mem::take(&mut self.folder_size_retiring));
        if let Some(popup) = self.folder_size_popup.take() { retiring_search.push(RetiringHashWindow { window: popup.window, native_window: popup.native }); }
        if let Some(view) = &self.folder_size_view { view.update(cx, |view, _| view.shutdown()); }
        retiring_search.extend(std::mem::take(&mut self.file_lock_retiring));
        if let Some(popup) = self.file_lock_popup.take() { retiring_search.push(RetiringHashWindow { window: popup.window, native_window: popup.native }); }
        if let Some(view) = &self.file_lock_view { view.update(cx, |view, _| view.shutdown()); }
        let mut retiring_regex = std::mem::take(&mut self.regex_retiring);
        if let Some(popup) = self.regex_popup.take() { retiring_regex.push(RetiringHashWindow { window: popup.window, native_window: popup.native }); }
        let retiring = self.xml_popup.take().map(|popup| RetiringHashWindow { window: popup.window, native_window: popup.native });
        let view = self.xml_view.clone();
        if let Some(view) = &view { view.update(cx, |view, cx| view.shutdown(cx)); }
        cx.spawn(async move |_, cx| {
            for retiring in retiring_clipboard { let _ = remove_and_wait_for_hash_window(cx, retiring).await; }
            for retiring in retiring_search { let _ = remove_and_wait_for_hash_window(cx, retiring).await; }
            for retiring in retiring_regex { let _ = remove_and_wait_for_hash_window(cx, retiring).await; }
            if let Some(retiring) = retiring { let _ = remove_and_wait_for_hash_window(cx, retiring).await; }
            if let Some(view) = view {
                loop {
                    if cx.update(|app| view.read(app).reaped()) { break; }
                    cx.background_executor().timer(Duration::from_millis(40)).await;
                }
            }
            cx.update(|app| app.quit());
        }).detach();
    }

    fn current_root_close_blocker(
        &self,
        clipboard_message: Option<&'static str>,
        file_picker_pending: bool,
        cx: &App,
    ) -> Option<RootCloseBlocker> {
        if self.clipboard_modify_startup.pending || self.clipboard_modify_view.as_ref().is_some_and(|view| view.read(cx).blocks_close() || view.read(cx).rehosting) { return Some(RootCloseBlocker::ClipboardModifyBlocked); }
        if self.settings_actions.as_ref().is_some_and(|view| view.read(cx).busy()) { return Some(RootCloseBlocker::SettingsActionsSave); }
        if self.file_lock_retiring.iter().any(|window| window.native_window.is_none()) || self.file_lock_view.as_ref().is_some_and(|view| view.read(cx).busy() || view.read(cx).rehosting) { return Some(RootCloseBlocker::FileLock); }
        if self.folder_size_retiring.iter().any(|window| window.native_window.is_none())
            || self.folder_size_view.as_ref().is_some_and(|view| view.read(cx).busy() || view.read(cx).rehosting) { return Some(RootCloseBlocker::FolderSize); }
        if self.external_launch.auxiliary_effect_pending() || self.file_search.as_ref().is_some_and(|view| view.read(cx).rehosting || view.read(cx).host_effect_pending(cx)) { return Some(RootCloseBlocker::FileSearchEffect); }
        if self.tempfile_view.as_ref().is_some_and(|view| view.read(cx).pending) { return Some(RootCloseBlocker::TempfileSave); }
        if self.snippets_view.as_ref().is_some_and(|view| view.read(cx).busy()) { return Some(RootCloseBlocker::SnippetsSave); }
        if self.clipboard_history.as_ref().is_some_and(|view| view.read(cx).busy() || view.read(cx).rehosting) { return Some(RootCloseBlocker::ClipboardHistory); }
        if self.structured_pins.busy() { return Some(RootCloseBlocker::StructuredPinWrite); }
        if self.regex_view.as_ref().is_some_and(|view| view.read(cx).clipboard_pending) { return Some(RootCloseBlocker::RegexClipboard); }
        if self.regex_view.as_ref().is_some_and(|view| view.read(cx).history.busy()) { return Some(RootCloseBlocker::RegexHistory); }
        if self.regex_view.as_ref().is_some_and(|view| view.read(cx).presets.busy()) { return Some(RootCloseBlocker::RegexPresets); }
        if self.regex_view.as_ref().is_some_and(|view| view.read(cx).rehosting) { return Some(RootCloseBlocker::RegexRehost); }
        if self.xml_view.as_ref().is_some_and(|view| view.read(cx).file_effect_pending() || view.read(cx).rehosting || view.read(cx).save_review_pending()) { return Some(RootCloseBlocker::XmlFilePicker); }
        if self.calculator_search.active {
            return Some(RootCloseBlocker::CalculatorSearch);
        }
        if self.hash_pin.pending.is_some() { return Some(RootCloseBlocker::HashPinWrite); }
        if self.hash_retiring_windows.iter().any(|window| window.native_window.is_none()) { return Some(RootCloseBlocker::HashRehost); }
        if self.plugin_editor.as_ref().is_some_and(|view| view.read(cx).pending) {
            return Some(RootCloseBlocker::PluginPreferenceSave);
        }
        self.external_launch
            .is_pending()
            .then_some(RootCloseBlocker::ExternalLaunch)
            .or_else(|| {
                self.app_editor
                    .write_pending()
                    .then_some(RootCloseBlocker::AppWrite)
            })
            .or_else(|| {
                self.result_favorites
                    .write_pending()
                    .then_some(RootCloseBlocker::FavoritesWrite)
            })
            .or_else(|| {
                root_close_blocker(
                    clipboard_message,
                    file_picker_pending,
                    self.toast_log_external_open_pending,
                    self.settings_search.as_ref().is_some_and(|view| view.read(cx).pending_token().is_some()),
                    self.command_help.pending.is_some(),
                )
            })
    }
}

impl Render for RootWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.apply_pending_help_filter_sync(window, cx);
        self.retire_stale_action_sheet(window, cx);
        if self.context_favorite_popup.as_ref().is_some_and(|popup| {
            !self.context_favorite_is_live(&popup.receipt, window, cx)
        }) {
            self.retire_context_favorite();
        }
        if !self.file_menu_open && !self.settings_menu_open && !self.help_menu_open {
            // Popovers may transiently report no focus. Keep the last focus
            // that was actually inside the live root focus tree.
            self.remember_live_root_menu_focus(window, cx);
        }
        let resolved = appearance::current(cx);
        let metrics = resolved.metrics;
        let palette = resolved.palette;
        let docked_hash = match &self.hash_window {
            HashWindowState::Docked {
                mount_generation,
                view,
            } => Some((view.clone(), *mount_generation)),
            _ => None,
        };
        let scale = window.scale_factor();
        let root_controls_width = launcher_column_width(
            window.viewport_size().width.as_f32(),
            metrics.window_padding,
            scale,
        );
        let search_width = launcher_search_width(
            window.viewport_size().width.as_f32(), metrics.window_padding, scale,
        );
        let completion_preview = self
            .search_completion
            .suggestions
            .iter()
            .enumerate()
            .map(|(index, suggestion)| {
                if index == self.search_completion.next_index {
                    format!("› {suggestion}")
                } else {
                    suggestion.clone()
                }
            })
            .collect::<Vec<_>>();
        let result_layout = self.launcher_results.layout;
        let result_generation = self.launcher_results.generation;
        let result_count = self.launcher_results.actions.len();
        let jump_mode_enabled = self.search_jump_mode.enabled;
        let jump_geometry_draft = jump_mode_enabled.then(|| {
            Rc::new(RefCell::new(JumpGeometryDraft::new(result_generation)))
        });
        let jump_geometry_frame = Rc::clone(&self.search_jump_geometry_frame);
        let jump_target_ids = jump_mode_enabled
            .then(|| Rc::new(self.search_jump_mode.target_ids().to_vec()));
        let jump_session_generation = self.search_jump_mode.session_generation();
        let result_row_count = result_layout.row_count(result_count);
        let result_columns = result_layout.columns.max(1);
        let result_width = (search_width
            - 2.0 * launcher_snapped_length(metrics.panel_padding, scale))
            .max(0.0);
        let (result_gap, grid_cell_width) = launcher_result_grid_geometry(
            result_width, result_columns, metrics.item_spacing_x, scale,
        );
        let grid_padding = metrics
            .panel_padding
            .min((grid_cell_width * scale / 2.0).floor() / scale);
        let list_padding = metrics
            .panel_padding
            .min((result_width * scale / 2.0).floor() / scale);
        let show_full_folder_path = self.enabled_capabilities.as_ref()
            .and_then(|caps| caps.get("folders"))
            .is_some_and(|caps| caps.iter().any(|cap| cap == "show_full_path"));
        let result_rows = (0..result_row_count)
            .map(|row_index| {
                let first_index = if result_layout.grid {
                    row_index * result_columns
                } else {
                    row_index
                };
                let end_index = if result_layout.grid {
                    first_index.saturating_add(result_columns).min(result_count)
                } else {
                    first_index.saturating_add(1).min(result_count)
                };

                if result_layout.grid {
                    let mut row = div()
                        .id(launcher_result_grid_row_id(result_generation, first_index))
                        .flex()
                        .flex_row()
                        .flex_shrink_0()
                        .gap_x(px(result_gap))
                        .w_full()
                        .min_w(px(0.0));
                    for absolute_index in first_index..end_index {
                        let action = &self.launcher_results.actions[absolute_index].action;
                        let target_id =
                            search_jump_result_target_id(result_generation, absolute_index);
                        let hint = self.search_jump_mode.hint_for(&target_id);
                        row = row.child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .child(render_launcher_result_cell(
                                    action,
                                    self.launcher_results.actions[absolute_index].label(),
                                    self.launcher_results.actions[absolute_index].hide_description,
                                    matches!(&self.launcher_results.actions[absolute_index].origin, LauncherResultOrigin::Provider(name) if name == "folders"),
                                    false,
                                    false,
                                    grid_padding,
                                    result_generation,
                                    absolute_index,
                                    self.launcher_results.selected == Some(absolute_index),
                                    hint,
                                    jump_geometry_draft.as_ref().map(Rc::clone),
                                    Rc::clone(&jump_geometry_frame),
                                    palette,
                                    metrics,
                                    cx,
                                )),
                        );
                    }
                    let missing_columns = result_columns.saturating_sub(end_index - first_index);
                    if missing_columns > 0 {
                        let omitted_gaps = missing_columns.saturating_sub(1);
                        row = row.child(
                            div()
                                .flex_1()
                                .flex_grow(missing_columns as f32)
                                .flex_basis(px(result_gap * omitted_gaps as f32))
                                .min_w(px(0.0)),
                        );
                    }
                    row.into_any_element()
                } else {
                    let action = &self.launcher_results.actions[first_index].action;
                    let target_id = search_jump_result_target_id(result_generation, first_index);
                    let hint = self.search_jump_mode.hint_for(&target_id);
                    render_launcher_result_cell(
                        action,
                        self.launcher_results.actions[first_index].label(),
                        self.launcher_results.actions[first_index].hide_description,
                        false,
                        true,
                        show_full_folder_path && matches!(&self.launcher_results.actions[first_index].origin, LauncherResultOrigin::Provider(name) if name == "folders"),
                        list_padding,
                        result_generation,
                        first_index,
                        self.launcher_results.selected == Some(first_index),
                        hint,
                        jump_geometry_draft.as_ref().map(Rc::clone),
                        Rc::clone(&jump_geometry_frame),
                        palette,
                        metrics,
                        cx,
                    )
                    .into_any_element()
                }
            })
            .collect::<Vec<_>>();
        let docked_hash_frame = docked_hash.as_ref().map(|(view, mount_generation)| {
            let session_epoch = self
                .hash_session
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .epoch();
            let payload = DockedHashResizePayload {
                mount_generation: *mount_generation,
                session_epoch,
                start_size: self.hash_docked_preferred_size,
                preview_extent: Arc::clone(&self.hash_docked_resize),
            };
            let resize_preview = self.docked_hash_resize_preview(*mount_generation, session_epoch);
            let requested_size = resize_preview.unwrap_or(self.hash_docked_preferred_size);
            let available_size = root_scroll_viewport_size(self.root_scroll.bounds());
            let current_visible_size =
                docked_hash_fit(self.hash_docked_preferred_size, available_size);
            let displayed_size = docked_hash_fit(requested_size, available_size);
            let resize_accessibility_label = docked_hash_resize_accessibility_label(
                self.hash_docked_preferred_size,
                current_visible_size,
                requested_size,
                displayed_size,
                resize_preview.is_some(),
            );
            let handle_size = HASH_DOCK_RESIZE_HANDLE_SIZE
                .min(displayed_size.width)
                .min(displayed_size.height)
                .max(1.0);

            let pointer_payload = payload.clone();
            let key_payload = payload.clone();
            let drag_started_payload = payload.clone();
            let key_available_size = available_size;
            let resize_handle = div()
                .id("hash-generator-docked-resize-handle")
                .absolute()
                .right_0()
                .bottom_0()
                .size(px(handle_size))
                .cursor(CursorStyle::ResizeUpLeftDownRight)
                .role(Role::Button)
                .aria_label(resize_accessibility_label)
                .key_context("HashGeneratorWindow")
                .track_focus(&self.resize_focus_handle)
                .tab_index(0)
                .focus_visible(|style| {
                    style
                        .border_1()
                        .border_color(appearance::to_hsla(palette.focus))
                })
                .bg(appearance::to_hsla(palette.panel_surface))
                .text_color(appearance::to_hsla(palette.accent))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |root, event: &MouseDownEvent, window, cx| {
                        root.resize_focus_handle.focus(window, cx);
                        cx.stop_propagation();
                        root.begin_docked_hash_pointer_resize(
                            &pointer_payload,
                            event.position,
                            window,
                            cx,
                        );
                    }),
                )
                .on_drag(payload.clone(), move |_, _, _, cx| {
                    drag_started_payload.mark_drag_started();
                    cx.new(|_| Empty)
                })
                .on_key_down(cx.listener(move |root, event: &KeyDownEvent, _, cx| {
                    if event.keystroke.modifiers != Modifiers::default() {
                        return;
                    }
                    if root.handle_docked_hash_resize_key(
                        &key_payload,
                        event.keystroke.key.as_str(),
                        key_available_size,
                        cx,
                    ) {
                        cx.stop_propagation();
                    }
                }))
                .child(Label::new("↘").text_size(px(metrics.heading_text_size)));

            let release_payload = payload.clone();
            let outside_release_payload = payload.clone();
            div()
                .id("hash-generator-docked-frame")
                .relative()
                .flex_shrink_0()
                .w(px(displayed_size.width))
                .h(px(displayed_size.height))
                .on_drag_move(cx.listener(
                    move |root, event: &DragMoveEvent<DockedHashResizePayload>, _, cx| {
                        let drag_payload = event.drag(cx).clone();
                        root.update_docked_hash_pointer_resize(
                            &drag_payload,
                            event.event.position,
                            cx,
                        );
                    },
                ))
                .capture_any_mouse_up(cx.listener(move |root, event: &MouseUpEvent, _, cx| {
                    if event.button == MouseButton::Left {
                        root.update_docked_hash_pointer_resize(
                            &release_payload,
                            event.position,
                            cx,
                        );
                        root.commit_docked_hash_resize(&release_payload, false, cx);
                    }
                }))
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(move |root, event: &MouseUpEvent, _, cx| {
                        root.update_docked_hash_pointer_resize(
                            &outside_release_payload,
                            event.position,
                            cx,
                        );
                        root.commit_docked_hash_resize(&outside_release_payload, false, cx);
                    }),
                )
                .child(
                    div()
                        .id("hash-generator-docked-scroll-viewport")
                        .size_full()
                        .flex()
                        .flex_col()
                        .pr(px(handle_size))
                        .pb(px(handle_size))
                        .child(
                            Label::new("Ctrl+Shift+R: focus resize grip · arrows preview · Enter saves · Escape discards")
                                .id("docked-hash-resize-instructions")
                                .w_full()
                                .text_size(px(metrics.small_text_size))
                                .line_height(px(metrics.line_height)),
                        )
                        .child(div().flex_1().min_h(px(0.0)).child(view.clone())),
                )
                .child(resize_handle)
        });
        let search_target_id = search_jump_search_target_id(result_generation);
        let search_hint = self.search_jump_mode.hint_for(&search_target_id);
        let search_hint_badge = jump_hint_badge(
            search_hint,
            &search_target_id,
            result_generation,
            Rc::clone(&jump_geometry_frame),
            palette,
            metrics,
        );
        let search_field = div()
                .id("launcher-search-input")
                .relative()
                .w_full()
                .min_w(px(0.0))
                .key_context("LauncherSearch")
                .capture_action(cx.listener(|root, _: &OpenActionSheet, window, cx| {
                    root.open_action_sheet(window, cx);
                    cx.stop_propagation();
                }))
                .capture_action(cx.listener(|root, _: &IndentInline, window, cx| {
                    if root.accept_search_completion(true, window, cx) {
                        cx.stop_propagation();
                    }
                }))
                .capture_action(cx.listener(|root, _: &MoveDown, window, cx| {
                    if root.move_search_selection(QuerySelectionMove::Down, window, cx) {
                        cx.stop_propagation();
                    }
                }))
                .capture_action(cx.listener(|root, _: &MoveUp, window, cx| {
                    if root.move_search_selection(QuerySelectionMove::Up, window, cx) {
                        cx.stop_propagation();
                    }
                }))
                .capture_action(cx.listener(|root, _: &MoveLeft, window, cx| {
                    if root.move_search_selection(QuerySelectionMove::Left, window, cx) {
                        cx.stop_propagation();
                    }
                }))
                .capture_action(cx.listener(|root, _: &MoveRight, window, cx| {
                    if root.move_search_selection(QuerySelectionMove::Right, window, cx) {
                        cx.stop_propagation();
                    }
                }))
                .capture_action(cx.listener(|root, _: &MovePageUp, window, cx| {
                    if root.move_search_selection(QuerySelectionMove::PageUp, window, cx) {
                        cx.stop_propagation();
                    }
                }))
                .capture_action(cx.listener(|root, _: &MovePageDown, window, cx| {
                    if root.move_search_selection(QuerySelectionMove::PageDown, window, cx) {
                        cx.stop_propagation();
                    }
                }))
                .capture_action(cx.listener(|root, _: &SearchHistoryOlder, window, cx| {
                    if root.navigate_search_history(QueryHistoryDirection::Older, window, cx) {
                        cx.stop_propagation();
                    }
                }))
                .capture_action(cx.listener(|root, _: &SearchHistoryNewer, window, cx| {
                    if root.navigate_search_history(QueryHistoryDirection::Newer, window, cx) {
                        cx.stop_propagation();
                    }
                }))
                .capture_key_down(cx.listener(|root, event, window, cx| {
                    root.record_search_key_down(event, window, cx);
                }))
                .on_key_up(cx.listener(|root, event, _, _| {
                    root.record_search_key_up(event);
                }))
                .on_mouse_down(MouseButton::Left, cx.listener(|root, _, window, cx| {
                    root.record_search_completion_pointer_press(window, cx);
                }))
                .child(
                    Input::new(&self.query)
                        .w_full()
                        .min_w(px(0.0))
                        .aria_label("Search")
                        .readonly(self.restart_admission.is_fenced()),
                )
                .children(search_hint_badge);
        let search_field = if let Some(draft) = jump_geometry_draft.as_ref() {
            let search_target_metadata = TargetMetadata::new(
                search_target_id,
                "Search",
                TargetKind::TextEntry,
                0,
            );
            JumpGeometryElement::new(
                search_field,
                search_target_metadata,
                Rc::clone(draft),
            )
            .into_any_element()
        } else {
            search_field.into_any_element()
        };
        let root_menu_bar = LauncherMenuBar {
            row_height: metrics.row_min_height,
            root: cx.entity().downgrade(),
            action_context: self.focus_handle.clone(),
            settings_menu_focus_handle: self.settings_menu_focus_handle.clone(),
            toast_log_open_pending: self.toast_log_external_open_pending,
            favorites_pin_focus_handle: self.favorites_pin_focus_handle.clone(),
            favorites_pinned: self.result_favorites.is_pinned(),
            favorites_toolbar_disabled: self.result_favorites.toolbar_disabled(&self.panel_stack) || self.structured_pins.busy(),
            favorites_unpin_status: self.result_favorites.unpin_status().map(str::to_owned),
            saved_pin_order: self.hash_pin.order.clone(),
            hash_pin_focus_handle: self.hash_pin_focus_handle.clone(),
            hash_pinned: self.hash_pin.is_saved(),
            hash_toolbar_disabled: self.hash_pin.pending.is_some() || self.structured_pins.busy(),
            json_pinned: self.structured_pins.saved(Panel::JsonUtilityDialog),
            xml_pinned: self.structured_pins.saved(Panel::XmlToolkitDialog),
            structured_pin_disabled: self.structured_pins.busy(),
            json_pin_focus_handle: self.json_pin_focus_handle.clone(),
            xml_pin_focus_handle: self.xml_pin_focus_handle.clone(),
        };
        let settings_editor_panel = self.settings_editor.open.then(|| SettingsEditorPanel {
            root: cx.entity().downgrade(), navigation: self.settings_navigation.clone(), scroll: self.settings_panel_scroll.clone(),
            search: self.settings_search.clone(), actions: self.settings_actions.clone(),
            busy: self.settings_search.as_ref().is_some_and(|view| view.read(cx).busy())
                || self.settings_actions.as_ref().is_some_and(|view| view.read(cx).busy()),
            status: self.settings_editor.status.clone(), index_busy: self.indexed_applications.busy(),
            index_status: self.indexed_applications.status.clone(), width: root_controls_width, metrics, palette,
            settings_owner_focus_handle: self.settings_owner_focus_handle.clone(),
        });
        let toast_log_viewer_panel = self.toast_log_viewer.open.then(|| ToastLogViewerPanel {
            root: cx.entity().downgrade(),
            rows: self.toast_log_viewer.rows.clone(),
            reading: self.toast_log_viewer.pending.is_some(),
            status: self.toast_log_viewer.status.clone(),
            width: root_controls_width,
            metrics,
            palette,
            owner_focus_handle: self.toast_log_owner_focus_handle.clone(),
            close_focus_handle: self.toast_log_focus_handle.clone(),
            scroll: self.toast_log_scroll.clone(),
        });
        let command_help_panel = self.command_help.open.then(|| CommandHelpPanel {
            root: cx.entity().downgrade(),
            providers: Arc::clone(&self.search_catalog.command_help_providers),
            filter_state: self.command_help_filter.clone(),
            filter: self.command_help_filter.read(cx).value().to_string(),
            show_examples: self.command_help.show_examples,
            saving: self.command_help.pending.is_some(),
            status: self.command_help.status.clone(),
            width: root_controls_width,
            metrics,
            palette,
            owner_focus_handle: self.command_help_owner_focus_handle.clone(),
            close_focus_handle: self.command_help_close_focus_handle.clone(),
            scroll: self.command_help_scroll.clone(),
        });
        let quick_help_panel = self.quick_help.open.then(|| QuickHelpPanel {
            root: cx.entity().downgrade(),
            providers: Arc::clone(&self.search_catalog.quick_help_providers),
            filter_state: self.quick_help_filter.clone(),
            filter: self.quick_help_filter.read(cx).value().to_string(),
            hotkeys: Arc::clone(&self.quick_help_hotkeys),
            paste_sequence_summary: self.quick_help_paste_sequence_summary.clone(),
            sequence_starts: Arc::clone(&self.quick_help_sequence_starts),
            status: self.quick_help_shortcut_diagnostic.clone(),
            width: root_controls_width,
            metrics,
            palette,
            owner_focus_handle: self.quick_help_owner_focus_handle.clone(),
            close_focus_handle: self.quick_help_close_focus_handle.clone(),
            scroll: self.quick_help_scroll.clone(),
        });
        let app_editor_panel = self.app_editor.open.then(|| {
            add_action::AppEditorPanel::new(
                cx.entity().downgrade(),
                &self.app_editor,
                root_controls_width,
                self.root_scroll.bounds().size.height.as_f32(),
                metrics,
                palette,
            )
        });
        let header = div()
            .flex().flex_col().flex_shrink_0().w_full()
            .text_color(appearance::to_hsla(palette.primary_text))
            .text_size(px(metrics.body_text_size))
            .line_height(px(metrics.line_height))
            .child(root_menu_bar)
            .child(div().flex().flex_col().items_start().self_center()
                .gap_y(px(metrics.item_spacing_y))
                .w(px(search_width)).max_w_full().px(px(metrics.panel_padding))
                .pt(px(metrics.window_padding))
                .children(launcher_header::identity(self.search_catalog.branding, metrics))
                .child(div().flex().items_center().gap_2().w_full()
                    .child(gpui_kit::component::Icon::new(gpui_kit::component::IconName::Search))
                    .child(div().flex_1().min_w(px(0.0)).child(search_field)))
                .children(self.structured_pins.error.clone().map(|status| Label::new(status)))
                .children((if self.hash_pin.pending.is_some() { Some("Unpinning Hash Generator…".to_owned()) } else { self.hash_pin.error.clone() }).map(|status| {
                    Label::new(status).id("pinned-hash-toolbar-status").w_full().min_w(px(0.0))
                })));
        let search_body = div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .max_h_full()
            .gap_y(px(metrics.item_spacing_y))
            .w(px(search_width))
            .max_w_full()
            .px(px(metrics.panel_padding))
            .text_color(appearance::to_hsla(palette.primary_text))
            .text_size(px(metrics.body_text_size))
            .line_height(px(metrics.line_height))
            .children(self.launcher_search_error.as_ref().map(|error| {
                Label::new(error.clone())
                    .w_full()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .id("launcher-search-error")
                    .text_color(appearance::to_hsla(palette.error))
                    .text_size(px(metrics.small_text_size))
                    .line_height(px(metrics.line_height))
            }))
            .children((!completion_preview.is_empty()).then(|| {
                div()
                    .id("launcher-search-completion")
                    .role(Role::Group)
                    .flex()
                    .flex_col()
                    .gap_y(px(metrics.item_spacing_y))
                    .w_full()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .opacity(0.7)
                    .children(completion_preview.into_iter().enumerate().map(|(index, preview)| {
                        Label::new(preview)
                            .id(format!("launcher-search-completion-{index}"))
                            .w_full()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .text_size(px(metrics.small_text_size))
                            .line_height(px(metrics.small_text_size * 1.2))
                    }))
            }))
            .children((result_count > 0).then(|| {
                div()
                    .id("launcher-search-results")
                    .flex()
                    .flex_col()
                    .gap_y(px(metrics.item_spacing_y))
                    .w_full()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .track_scroll(&self.launcher_results_scroll)
                    .on_scroll({
                        let scroll = self.launcher_results_scroll.clone();
                        let line_height = px(metrics.line_height);
                        move |event, _, cx| {
                            // The built-in scroll listener uses the result element's
                            // resolved text line height and falls back to x for y=0.
                            let pixels = event.delta.pixel_delta(line_height);
                            let delta = if pixels.y.is_zero() { pixels.x } else { pixels.y }.as_f32();
                            let offset = scroll.offset();
                            let maximum = scroll.max_offset().y.as_f32();
                            let previous = (offset.y.as_f32() - delta).clamp(-maximum, 0.0);
                            scroll.set_offset(point(offset.x, px(offset.y.as_f32().clamp(-maximum, 0.0))));
                            if inner_results_consumed_scroll(previous, maximum, delta) {
                                cx.stop_propagation();
                            }
                        }
                    })
                    .children(result_rows)
            }))
            .children(launcher_controls::LauncherUtilityControls::visible(self).then(|| {
                launcher_controls::LauncherUtilityControls::new(self, metrics, palette)
            }));
        let mut root_shell = div()
            .id("multi-launcher-root")
            .capture_key_up(cx.listener(|root, event: &KeyUpEvent, _, cx| {
                if event.keystroke.key == "escape" { root.pane_escape_latched = false; }
                if event.keystroke.key == "enter" {
                    root.pane_enter_latched = false;
                    if let Some(view) = root.settings_navigation.clone() { view.update(cx, |view, _| view.release_enter()); }
                }
            }))
            .capture_any_mouse_down(cx.listener(|root, _, _, _| {
                root.external_launch.retire_focus_bound_effects();
            }))
            .role(Role::Group)
            .flex()
            .flex_col()
            .size_full()
            .px(px(metrics.window_padding))
            .pt(px(2.0))
            .pb(px(metrics.window_padding))
            .items_center()
            .justify_start()
            .track_focus(&self.focus_handle)
            .key_context("RootWindow")
            .capture_key_down(cx.listener(|root, event: &KeyDownEvent, window, cx| {
                if window.default_prevented() { return; }
                if event.keystroke.key == "enter" && event.is_held && root.pane_enter_latched {
                    window.prevent_default(); cx.stop_propagation(); return;
                }
                if event.keystroke.key != "escape"
                    || event.keystroke.modifiers != Modifiers::default()
                {
                    return;
                }
                if event.is_held && root.pane_escape_latched { window.prevent_default(); cx.stop_propagation(); return; }
                if root.app_editor_escape(window, cx) {
                    cx.stop_propagation();
                } else if quick_help_escape_capture_should_close(
                    &window.context_stack(),
                    root.result_favorites.owns_focus(window, cx),
                ) && !context_stack_has_primary(&window.context_stack(), "List") {
                    root.dismiss_result_favorites(window, cx);
                    cx.stop_propagation();
                } else if root.toast_log_viewer.open
                    && root
                        .toast_log_owner_focus_handle
                        .contains_focused(window, cx)
                {
                    root.close_toast_log_viewer(window, cx, true);
                    cx.stop_propagation();
                } else if root.settings_escape(window, cx) {
                    cx.stop_propagation();
                } else if root.quick_help.open
                    && quick_help_escape_capture_should_close(
                        &window.context_stack(),
                        root.quick_help_owner_focus_handle.contains_focused(window, cx),
                    )
                {
                    root.close_quick_help(window, cx, true);
                    cx.stop_propagation();
                } else if root.command_help.open
                    && command_help_escape_capture_should_close(
                        &window.context_stack(),
                        root.command_help_owner_focus_handle.contains_focused(window, cx),
                    )
                {
                    root.close_command_help(window, cx, true);
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|root, _: &gpui_kit::base::input::Escape, window, cx| {
                  if root.app_editor_escape(window, cx) {
                      cx.stop_propagation();
                  } else if quick_help_escape_capture_should_close(
                      &window.context_stack(),
                      root.result_favorites.owns_focus(window, cx),
                  ) && !context_stack_has_primary(&window.context_stack(), "List") {
                      root.dismiss_result_favorites(window, cx);
                      cx.stop_propagation();
                  } else if root.settings_escape(window, cx) {
                      cx.stop_propagation();
                  } else if root.quick_help.open
                    && quick_help_escape_capture_should_close(
                        &window.context_stack(),
                        root.quick_help_owner_focus_handle.contains_focused(window, cx),
                    )
                {
                    root.close_quick_help(window, cx, true);
                    cx.stop_propagation();
                } else if root.command_help.open
                    && root
                        .command_help_owner_focus_handle
                        .contains_focused(window, cx)
                {
                    root.close_command_help(window, cx, true);
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &FocusDockedHashResize, window, cx| {
                this.focus_docked_hash_resize_handle(window, cx);
            }))
            .on_action(cx.listener(|this, _: &CalculateHash, window, cx| {
                if this.forward_docked_hash_action(DockedHashAction::Calculate, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &DockHashGenerator, window, cx| {
                if this.forward_docked_hash_action(DockedHashAction::Dock, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &PopoutHashGenerator, window, cx| {
                if this.forward_docked_hash_action(DockedHashAction::Popout, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &CloseHashGenerator, window, cx| {
                if this.forward_docked_hash_action(DockedHashAction::Close, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleRetainedWindow, _, cx| {
                this.open_or_toggle_secondary(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleCoordinateHudProbe, _, cx| {
                this.toggle_coordinate_hud_probe(cx);
            }))
            .on_action(cx.listener(|this, _: &OpenHashGenerator, window, cx| {
                this.run_hash_generator_open(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleSettings, window, cx| {
                this.toggle_settings_editor(window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenPluginEditor, window, cx| {
                if !this.restart_admission.is_fenced() {
                    if this.file_menu_open { this.root_menu_action_routed_focus = true; }
                    this.open_plugin_editor(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &OpenAppEditor, window, cx| {
                if !this.restart_admission.is_fenced() {
                    let origin = this.root_menu_previous_focus.clone()
                        .filter(|focus| this.focus_handle.contains(focus, window))
                        .unwrap_or_else(|| this.query.focus_handle(cx));
                    if this.file_menu_open {
                        this.root_menu_action_routed_focus = true;
                    }
                    this.open_app_editor(Some(origin), window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &OpenActivity, window, cx| {
                let origin = this.root_menu_previous_focus.clone()
                    .filter(|focus| this.focus_handle.contains(focus, window))
                    .or_else(|| Some(this.query.focus_handle(cx)));
                if this.file_menu_open { this.root_menu_action_routed_focus = true; }
                this.open_activity(origin, window, cx);
            }))
            .on_action(cx.listener(|this, _: &CommandList, window, cx| {
                this.open_command_help(window, cx, CommandHelpOpenOrigin::Menu, None);
            }))
            .on_action(cx.listener(|this, _: &QuickHelp, window, cx| {
                this.open_quick_help(window, cx, QuickHelpOpenOrigin::Menu, true);
            }))
            .on_action(cx.listener(|this, _: &LinkingGuide, window, cx| {
                this.open_command_help(
                    window,
                    cx,
                    CommandHelpOpenOrigin::Menu,
                    Some(LINKING_GUIDE_FILTER),
                );
            }))
            .on_action(cx.listener(|this, _: &CloseApplication, _, cx| {
                this.request_quit(cx);
            }))
            .on_action(cx.listener(|this, _: &OpenToastLog, _, cx| {
                this.open_toast_log_external(cx);
            }))
            .on_action(cx.listener(|this, _: &ViewToastLog, window, cx| {
                this.open_toast_log_viewer(window, cx);
            }))
            .on_action(cx.listener(|this, _: &RestartWithEgui, window, cx| {
                let menu_origin = this.file_menu_open;
                if menu_origin {
                    let focus = this.root_menu_previous_focus.clone()
                        .filter(|focus| this.focus_handle.contains(focus, window))
                        .unwrap_or_else(|| this.query.focus_handle(cx));
                    focus.focus(window, cx);
                }
                if this.open_restart_confirmation(window, cx) && menu_origin {
                    this.root_menu_action_routed_focus = true;
                }
            }))
            .child(header);
        if docked_hash.is_some() {
            root_shell = root_shell.capture_any_mouse_down(cx.listener(|this, _, _, cx| {
                if this.cancel_docked_hash_preview() { cx.notify(); }
            }));
        }
        let mut root_scroll = div()
            .id("multi-launcher-root-scroll")
            .flex().flex_col().flex_1().min_h(px(0.0)).w_full()
            .gap_y(px(metrics.section_spacing))
            .overflow_y_scroll().items_center().justify_start()
            .track_scroll(&self.root_scroll)
            .when(docked_hash.is_some(), |this| this.items_start())
            .child(search_body);
        if let Some(settings_editor_panel) = settings_editor_panel {
            root_scroll = root_scroll.child(
                div()
                    .flex_shrink_0()
                    .child(settings_editor_panel),
            );
        }
        if let Some(toast_log_viewer_panel) = toast_log_viewer_panel {
            root_scroll = root_scroll.child(
                div()
                    .flex_shrink_0()
                    .child(toast_log_viewer_panel),
            );
        }
        if let Some(command_help_panel) = command_help_panel {
            root_scroll = root_scroll.child(
                div()
                    .flex_shrink_0()
                    .child(command_help_panel),
            );
        }
        if let Some(docked_hash_frame) = docked_hash_frame {
            root_scroll = root_scroll.child(docked_hash_frame);
        }
        if let Some(quick_help_panel) = quick_help_panel {
            root_scroll = root_scroll.child(
                div()
                    .flex_shrink_0()
                    .child(quick_help_panel),
            );
        }
        root_scroll = root_scroll.children(
            self.favorites_panel(root_controls_width, metrics, palette, cx)
                .map(|panel| div().flex_shrink_0().child(panel)),
        );
        if let Some(panel) = app_editor_panel {
            root_scroll = root_scroll.child(div().flex_shrink_0().child(panel));
        }
        if let Some(view) = self.file_search.as_ref() {
            let state = view.read(cx);
            if state.open && !state.popout {
                root_scroll = root_scroll.child(div().flex_shrink_0().w_full().child(view.clone()));
            } else if let Some(child) = self.root_window.and_then(|root| state.detached_inline_preview(root.window_id(), cx)) {
                let escape_view = view.downgrade();
                root_scroll = root_scroll.child(div().id("detached-file-search-preview").key_context("FileSearchPane")
                    .capture_action(cx.listener(move |root, _: &gpui_kit::base::input::Escape, window, cx| {
                        if root.pane_escape_latched { cx.stop_propagation(); return; }
                        if escape_view.update(cx, |view, cx| view.handle_escape(window, cx, None)).unwrap_or(false) {
                            root.pane_escape_latched = true; cx.stop_propagation();
                        }
                    }))
                    .flex_shrink_0().w_full().child(child));
            }
        }
        #[cfg(windows)]
        if let Some(view) = self.downloads_triage.as_ref().filter(|view| view.read(cx).open) {
            root_scroll = root_scroll.child(div().flex_shrink_0().w_full().child(view.clone()));
        }
        if let Some(view) = self.plugin_editor.as_ref().filter(|view| view.read(cx).open) {
            root_scroll = root_scroll.child(div().flex_shrink_0().w_full().child(view.clone()));
        }
        if let Some(view) = self.activity_view.as_ref().filter(|view| view.read(cx).open) {
            root_scroll = root_scroll.child(div().flex_shrink_0().w_full().child(view.clone()));
        }
        if let Some(view) = self.json_view.as_ref().filter(|view| view.read(cx).is_mounted_in(window.window_handle().window_id(), false)) {
            root_scroll = root_scroll.child(div().flex_shrink_0().w_full().child(view.clone()));
        }
        if let Some(view) = self.xml_view.as_ref().filter(|view| view.read(cx).is_mounted_in(window.window_handle().window_id(), false)) {
            root_scroll = root_scroll.child(div().flex_shrink_0().w_full().child(view.clone()));
        }
        if let Some(view) = self.regex_view.as_ref().filter(|view| view.read(cx).is_mounted_in(window.window_handle().window_id(), false)) {
            root_scroll = root_scroll.child(div().flex_shrink_0().w_full().child(view.clone()));
        }
        if let Some(view) = self.clipboard_history.as_ref().filter(|view| view.read(cx).is_mounted_in(window.window_handle().window_id(), false)) {
            root_scroll = root_scroll.child(div().flex_shrink_0().w_full().child(view.clone()));
        }
        if let Some(view) = self.clipboard_modify_view.as_ref().filter(|view| view.read(cx).is_mounted_in(window.window_handle().window_id(), false)) {
            root_scroll = root_scroll.child(div().flex_shrink_0().w_full().child(view.clone()));
        }
        if let Some(view) = self.snippets_view.as_ref().filter(|view| view.read(cx).open) {
            root_scroll = root_scroll.child(div().flex_shrink_0().w_full().child(view.clone()));
        }
        if let Some(view) = self.snippet_fill.as_ref().filter(|view| view.read(cx).open) {
            root_scroll = root_scroll.child(div().flex_shrink_0().w_full().child(view.clone()));
        }
        if let Some(view) = self.tempfile_view.as_ref().filter(|view| view.read(cx).open) {
            root_scroll = root_scroll.child(div().flex_shrink_0().w_full().child(view.clone()));
        }
        if let Some(view) = self.folder_size_view.as_ref().filter(|view| view.read(cx).is_mounted_in(window.window_handle().window_id(), false)) {
            root_scroll = root_scroll.child(div().flex_shrink_0().w_full().child(view.clone()));
        }
        if let Some(view) = self.file_lock_view.as_ref().filter(|view| view.read(cx).is_mounted_in(window.window_handle().window_id(), false)) {
            root_scroll = root_scroll.child(div().flex_shrink_0().w_full().child(view.clone()));
        }
        // ScrollHandle bounds update during prepaint. Re-render either tool
        // when that measured body viewport changes, including native resizes.
        if docked_hash.is_some() || self.app_editor.open {
            let scroll = self.root_scroll.clone();
            let owner = cx.entity().downgrade();
            root_scroll = root_scroll.child(canvas(
                move |_, window, app| {
                    let measured = root_scroll_viewport_size(scroll.bounds());
                    let owner = owner.clone();
                    window.defer(app, move |_, app| {
                        let _ = owner.update(app, |root, cx| {
                            if root.root_body_viewport != Some(measured) {
                                root.root_body_viewport = Some(measured);
                                cx.notify();
                            }
                        });
                    });
                },
                |_, _, _, _| {},
            ).absolute().size(px(0.0)));
        }
        // The absolute Search observer still has a layout slot, so keep it after the pane anchors.
        if let (Some(draft), Some(target_ids)) = (jump_geometry_draft, jump_target_ids) {
            root_scroll = root_scroll.child(jump_geometry_publisher(
                draft,
                jump_geometry_frame,
                cx.entity().downgrade(),
                jump_session_generation,
                target_ids,
                self.search_jump_mode.alphabet.clone(),
                metrics,
                |root, frame, window, cx, geometry_changed| {
                    let retired = root.reconcile_search_jump_geometry_frame(frame, window, cx);
                    let changed = geometry_changed
                        && root.search_jump_mode.is_active()
                        && frame.matches_active_session;
                    if retired || changed {
                        cx.notify();
                    }
                },
            ));
        }
        let popup = self.context_favorite_popup.as_ref().and_then(|popup| {
            let menu = popup.menu.as_ref()?;
            self.context_favorite_is_live(&popup.receipt, window, cx).then(|| {
                deferred(
                    anchored()
                        .position(popup.position)
                        .snap_to_window_with_margin(px(8.0))
                        .anchor(Anchor::TopLeft)
                        .child(menu.clone()),
                )
                .with_priority(gpui_kit::base::POPUP_PRIORITY)
            })
        });
        root_shell.child(root_scroll).children(popup)
    }
}

struct HashGeneratorWindow {
    jump: hash_jump::HashJump,
    source_popup: Rc<RefCell<hash_jump_select::PopupCollector>>,
    algorithm_popup: Rc<RefCell<hash_jump_select::PopupCollector>>,
    text: Entity<TextareaState>,
    expected: Entity<TextareaState>,
    source: Entity<SelectState<Vec<hash_jump_select::HashSelectItem<HashSourceChoice>>>>,
    algorithm: Entity<SelectState<Vec<hash_jump_select::HashSelectItem<HashAlgorithmChoice>>>>,
    focus_handle: FocusHandle,
    session: Arc<Mutex<HashSession>>,
    wake_signal: Arc<HashWakeSignal>,
    file_picker_pending: Arc<AtomicBool>,
    clipboard_pending: Arc<HashClipboardPendingState>,
    pinned: Arc<AtomicBool>,
    native_window: Option<NativeWindow>,
    native_geometry_slot: NativeGeometrySlot,
    native_geometry_stamp: NativeGeometryStamp,
    hash_tool_key: UtilityToolKey,
    native_geometry_sequence: Arc<AtomicU64>,
    native_geometry_binding: Option<NativeGeometryBinding>,
    native_geometry_error: Option<String>,
    size_write_error: Option<String>,
    factory_outer_size: Option<PreferredWindowSize>,
    restart_admission: Arc<RestartAdmission>,
    mount_generation: u64,
    session_epoch: u64,
    active_mount_generation: Arc<AtomicU64>,
    rehost_pending: Arc<AtomicBool>,
    rehost_revision: Arc<AtomicU64>,
    presentation_feedback: Arc<Mutex<Option<String>>>,
    root: WeakEntity<RootWindow>,
    mode: HashPresentationMode,
    file_picker_feedback: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl HashGeneratorWindow {
    fn attach_native_geometry(&mut self) -> Result<(), String> {
        if self.native_geometry_binding.is_some() || self.mode != HashPresentationMode::Popout {
            return Ok(());
        }
        if !self.is_current_mount() {
            return Err("the Hash Generator mount is no longer active".to_owned());
        }
        let Some(native_window) = self.native_window else {
            return if cfg!(windows) {
                Err("the Hash Generator popout has no checked HWND".to_owned())
            } else {
                Ok(())
            };
        };
        let target = checked_geometry_target(native_window)
            .ok_or_else(|| "the Hash Generator HWND could not be checked".to_owned())?;
        let wake_signal = Arc::clone(&self.wake_signal);
        let binding = NativeGeometryBinding::attach_checked(
            target,
            self.hash_tool_key,
            self.native_geometry_stamp,
            Arc::clone(&self.native_geometry_slot),
            1.0,
            Arc::clone(&self.native_geometry_sequence),
            Arc::new(move |_| wake_signal.notify()),
        )?;
        self.native_geometry_binding = Some(binding);
        Ok(())
    }

    fn take_native_geometry(
        &self,
    ) -> Option<(UtilityNativeGeometrySample, Option<PreferredWindowSize>)> {
        let sample = take_native_geometry(
            &self.native_geometry_slot,
            self.hash_tool_key,
            self.native_geometry_stamp,
        )?;
        let convert = |rect: NativeWindowRect| PhysicalWindowRect {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        };
        Some((
            UtilityNativeGeometrySample {
                position: convert(sample.position),
                position_scale: sample.position_scale,
                position_sequence: sample.position_sequence,
                size: sample.size.map(convert),
                size_sequence: sample.size_sequence,
            },
            self.factory_outer_size,
        ))
    }

    fn detach_native_geometry(&mut self) {
        self.native_geometry_binding.take();
    }

    fn is_current_mount(&self) -> bool {
        if self.active_mount_generation.load(Ordering::Acquire) != self.mount_generation
            || self.rehost_pending.load(Ordering::Acquire)
            || self.restart_admission.is_fenced()
        {
            return false;
        }
        let session = self
            .session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        hash_mount_identity_matches(
            self.active_mount_generation.load(Ordering::Acquire),
            self.mount_generation,
            session.epoch(),
            self.session_epoch,
        )
    }

    fn draft(&self, cx: &App) -> HashDraft {
        let text = self.text.read(cx).value().to_string();
        let expected = self.expected.read(cx).value().to_string();
        let mode = self
            .source
            .read(cx)
            .selected_value()
            .copied()
            .map(HashSourceChoice::mode);
        let algorithm = self
            .algorithm
            .read(cx)
            .selected_value()
            .map(|choice| choice.0);
        let session = self
            .session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        HashDraft {
            text,
            expected,
            mode: mode.unwrap_or_else(|| session.mode()),
            algorithm: algorithm.unwrap_or_else(|| session.algorithm()),
            file_path: session.file_path().map(PathBuf::from),
            clipboard_snapshot_revision: session.clipboard_snapshot_revision(),
        }
    }

    fn sync_session_draft(&self, cx: &App) -> Option<HashDraft> {
        let draft = self.draft(cx);
        let mut session = self
            .session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if session.epoch() != self.session_epoch {
            return None;
        }
        if session.file_path() != draft.file_path.as_deref() {
            session.set_file_path(draft.file_path.clone());
        }
        session.set_text(draft.text.clone());
        session.set_expected(draft.expected.clone());
        session.set_mode(draft.mode);
        session.set_algorithm(draft.algorithm);
        Some(draft)
    }

    fn set_editors_readonly(&mut self, readonly: bool, cx: &mut Context<Self>) {
        self.text
            .update(cx, |state, cx| state.set_readonly(readonly, cx));
        self.expected
            .update(cx, |state, cx| state.set_readonly(readonly, cx));
    }

    fn editable_focus_is_exact(&self, window: &Window, cx: &App) -> bool {
        let mode = self.session.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .mode();
        match mode {
            HashInputMode::Text => self.text.focus_handle(cx).is_focused(window),
            _ => self.source_popup.borrow().trigger.as_ref()
                .is_some_and(|trigger| trigger.is_focused(window)),
        }
    }

    fn focus_editable(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = self
            .session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .mode();
        if mode == HashInputMode::Text {
            self.text.focus_handle(cx).focus(window, cx);
        } else {
            self.source.update(cx, |state, cx| state.focus(window, cx));
        }
    }

    fn apply_file_selection(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.session.lock().unwrap_or_else(std::sync::PoisonError::into_inner).set_file_path(Some(path));
        self.source.update(cx, |state, cx| state.set_selected_value(&HashSourceChoice::File, window, cx));
        self.file_picker_feedback = None;
        cx.notify();
    }

    fn select_external_file(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Result<(), String> {
        if !self.is_current_mount() || self.file_picker_pending.load(Ordering::Acquire) || self.clipboard_pending.is_pending() {
            return Err("Hash Generator is busy with another operation.".to_owned());
        }
        self.apply_file_selection(path, window, cx);
        Ok(())
    }

    fn picker_request_is_current(&self, request: HashPickerRequest) -> bool {
        if !self.is_current_mount()
            || request.mount_generation != self.mount_generation
            || request.session_epoch != self.session_epoch
        {
            return false;
        }
        let session = self
            .session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        hash_picker_result_is_current(
            self.active_mount_generation.load(Ordering::Acquire),
            request.mount_generation,
            session.epoch(),
            request.session_epoch,
            session.generation(),
            request.source_generation,
        )
    }

    fn resolve_file_picker(
        &mut self,
        request: HashPickerRequest,
        outcome: HashPickerOutcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.picker_request_is_current(request) {
            return;
        }
        match outcome {
            HashPickerOutcome::Selected(path) => {
                self.apply_file_selection(path, window, cx);
            }
            HashPickerOutcome::Cancelled => {
                self.file_picker_feedback =
                    Some("File selection cancelled; the current source is unchanged.".to_owned());
            }
            HashPickerOutcome::Failed(error) => {
                let message = format!("Could not choose a file: {error}");
                self.session
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .set_error(message.clone());
                self.file_picker_feedback = Some(message);
            }
        }
        cx.notify();
    }

    fn calculate_hash(&mut self, cx: &mut Context<Self>) {
        if !self.is_current_mount() || self.clipboard_pending.is_copying() {
            return;
        }
        let Some(_operation) = self.restart_admission.admit() else {
            return;
        };
        if self.sync_session_draft(cx).is_none() {
            return;
        }
        let mut session = self
            .session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if session.has_job() {
            return;
        }
        let wake_signal = Arc::clone(&self.wake_signal);
        if let Err(error) = session.calculate(Arc::new(move || wake_signal.notify())) {
            session.set_error(error);
        }
        cx.notify();
    }

    fn cancel_hash(&mut self, cx: &mut Context<Self>) {
        if !self.is_current_mount() || self.clipboard_pending.is_copying() {
            return;
        }
        let Some(_operation) = self.restart_admission.admit() else {
            return;
        };
        let mut session = self
            .session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if session.epoch() == self.session_epoch {
            session.cancel();
        }
        cx.notify();
    }

    fn clipboard_request(&self) -> Option<HashClipboardRequest> {
        if !self.is_current_mount() {
            return None;
        }
        let session = self
            .session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (session.epoch() == self.session_epoch).then_some(HashClipboardRequest {
            mount_generation: self.mount_generation,
            session_epoch: self.session_epoch,
            source_generation: session.generation(),
        })
    }

    fn load_clipboard(&mut self, cx: &mut Context<Self>) {
        if !self.is_current_mount() || self.clipboard_pending.is_pending() {
            return;
        }
        let Some(operation) = self.restart_admission.admit() else {
            return;
        };
        if self.sync_session_draft(cx).is_none() {
            return;
        }
        let Some(request) = self.clipboard_request() else {
            return;
        };
        match self.root.update(cx, |root, cx| {
            root.start_hash_clipboard_operation(
                request,
                HashClipboardOperationKind::Load,
                operation,
                cx,
            )
        }) {
            Ok(Ok(())) => self.file_picker_feedback = None,
            Ok(Err(error)) => self
                .set_clipboard_start_error(format!("Could not start Clipboard Load: {error}"), cx),
            Err(error) => self
                .set_clipboard_start_error(format!("Could not start Clipboard Load: {error}"), cx),
        }
        cx.notify();
    }

    fn copy_digest(&mut self, cx: &mut Context<Self>) {
        if !self.is_current_mount()
            || !hash_clipboard_copy_admissible(
                self.clipboard_pending.kind(),
                self.file_picker_pending.load(Ordering::Acquire),
            )
        {
            return;
        }
        let Some(operation) = self.restart_admission.admit() else {
            return;
        };
        if self.sync_session_draft(cx).is_none() {
            return;
        }
        let (request, copy) = {
            let session = self
                .session
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if session.epoch() != self.session_epoch {
                return;
            }
            let Some(copy) = session.clipboard_copy_request() else {
                return;
            };
            (
                HashClipboardRequest {
                    mount_generation: self.mount_generation,
                    session_epoch: self.session_epoch,
                    source_generation: session.generation(),
                },
                copy,
            )
        };

        self.set_editors_readonly(true, cx);
        match self.root.update(cx, |root, cx| {
            root.start_hash_clipboard_operation(
                request,
                HashClipboardOperationKind::Copy(copy),
                operation,
                cx,
            )
        }) {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                self.set_editors_readonly(false, cx);
                self.set_clipboard_start_error(format!("Could not start Copy Digest: {error}"), cx);
            }
            Err(error) => {
                self.set_editors_readonly(false, cx);
                self.set_clipboard_start_error(format!("Could not start Copy Digest: {error}"), cx);
            }
        }
        cx.notify();
    }

    fn set_clipboard_start_error(&mut self, message: String, cx: &mut Context<Self>) {
        self.session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_error(message);
        cx.notify();
    }

    fn choose_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_current_mount()
            || !hash_file_picker_admissible(
                self.clipboard_pending.kind(),
                self.file_picker_pending.load(Ordering::Acquire),
            )
        {
            return;
        }
        if let Err(error) = checked_active_dialog_owner(window) {
            self.file_picker_feedback = Some(format!("Could not open the file picker: {error}"));
            cx.notify();
            return;
        }
        if self
            .file_picker_pending
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            self.file_picker_feedback = Some("A file picker is already open.".to_owned());
            cx.notify();
            return;
        }
        let Some(operation) = self.restart_admission.admit() else {
            self.file_picker_pending.store(false, Ordering::Release);
            return;
        };
        let request = {
            let session = self
                .session
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if session.epoch() != self.session_epoch {
                self.file_picker_pending.store(false, Ordering::Release);
                return;
            }
            HashPickerRequest {
                mount_generation: self.mount_generation,
                session_epoch: self.session_epoch,
                source_generation: session.generation(),
            }
        };
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose a file to hash".into()),
        });
        let pending = Arc::clone(&self.file_picker_pending);
        let root = self.root.clone();
        match root.update(cx, |root, cx| {
            root.start_hash_file_picker(request, picker, operation, cx);
        }) {
            Ok(()) => self.file_picker_feedback = None,
            Err(error) => {
                pending.store(false, Ordering::Release);
                self.file_picker_feedback =
                    Some(format!("Could not start the file picker: {error}"));
            }
        }
        cx.notify();
    }

    fn request_rehost(&mut self, target: HashPresentationMode, cx: &mut Context<Self>) {
        if target == self.mode
            || !self.is_current_mount()
            || self.clipboard_pending.is_copying()
            || self.file_picker_pending.load(Ordering::Acquire)
        {
            return;
        }
        if self
            .rehost_pending
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let Some(operation) = self.restart_admission.admit() else {
            self.rehost_pending.store(false, Ordering::Release);
            return;
        };
        let Some(_) = self.sync_session_draft(cx) else {
            self.rehost_pending.store(false, Ordering::Release);
            return;
        };
        let dock_gesture = if target == HashPresentationMode::Docked && self.mode == HashPresentationMode::Popout {
            self.native_window.and_then(|source| {
                self.root.upgrade().map(|root| hash_dock::Gesture {
                    source,
                    foreground_authorized: native_window_is_valid(source) && current_foreground_identity() == Some(source.identity),
                    visibility_revision: root.read(cx).root_visibility.request_revision.load(Ordering::Acquire),
                })
            })
        } else { None };
        self.set_editors_readonly(true, cx);
        let revision = self
            .rehost_revision
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1)
            .max(1);
        let root = self.root.clone();
        let mount_generation = self.mount_generation;
        let session_epoch = self.session_epoch;
        cx.spawn(async move |_, cx| {
            let _ = root.update(cx, |root, cx| {
                root.start_hash_rehost(
                    target,
                    mount_generation,
                    session_epoch,
                    revision,
                    dock_gesture,
                    None,
                    operation,
                    cx,
                );
            });
        })
        .detach();
        cx.notify();
    }

    fn request_close(&mut self, origin: hash_pin::CloseOrigin, cx: &mut Context<Self>) {
        if !hash_pin::allows_close(self.pinned.load(Ordering::Acquire), origin)
            || !self.is_current_mount() || self.clipboard_pending.is_pending()
            || self.file_picker_pending.load(Ordering::Acquire) {
            return;
        }
        if self
            .rehost_pending
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let Some(operation) = self.restart_admission.admit() else {
            self.rehost_pending.store(false, Ordering::Release);
            return;
        };
        self.set_editors_readonly(true, cx);
        let revision = self
            .rehost_revision
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1)
            .max(1);
        let root = self.root.clone();
        let mount_generation = self.mount_generation;
        let session_epoch = self.session_epoch;
        cx.spawn(async move |_, cx| {
            let _ = root.update(cx, |root, cx| {
                root.start_hash_close(mount_generation, session_epoch, revision, origin, operation, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn hash_control_eligibility(&self) -> hash_jump::HashEligibility {
        let current = self.is_current_mount();
        let session = self
            .session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let clipboard = self.clipboard_pending.kind();
        hash_jump::HashEligibility {
            current,
            mode: session.mode(),
            presentation: self.mode,
            has_job: session.has_job(),
            is_running: session.is_running(),
            picker_pending: self.file_picker_pending.load(Ordering::Acquire),
            clipboard_pending: clipboard != HashClipboardPendingKind::Idle,
            clipboard_copy_pending: clipboard == HashClipboardPendingKind::Copy,
            can_copy_digest: session.can_copy_digest() && session.result().is_some(),
        }
    }

    fn hash_jump_owns_input(&self, window: &Window, cx: &App) -> bool {
        window.is_window_active()
            && hash_escape_capture_should_close(
                &window.context_stack(),
                self.focus_handle.contains_focused(window, cx),
                self.file_picker_pending.load(Ordering::Acquire),
            )
    }

    fn handle_jump_keydown(
        &mut self,
        receipt: PhysicalKeydown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if receipt.virtual_key == 0x1b {
            self.jump.controller.release_fresh_key(receipt);
        }
        if let Some(consumed) = self.handle_popup_jump_keydown(receipt, window, cx) {
            return consumed;
        }
        if !self.is_current_mount() || !self.hash_jump_owns_input(window, cx) {
            if self.jump.retire() {
                cx.notify();
            }
            return false;
        }
        let frame = Rc::clone(&self.jump.frame);
        let (consumed, repaint, target) = self.jump.key(
            receipt,
            hash_jump::HashJumpScope::Main,
            frame.borrow().as_ref(),
            self.mount_generation,
        );
        if let Some(target) = target {
            let current = self.jump.frame.borrow().as_ref().is_some_and(|frame| {
                frame.generation == self.mount_generation && frame.has_target(&target)
            });
            if current && let SemanticTargetId::Named(name) = target.semantic() {
                if let Some(role) = hash_jump::HashControl::from_name(name)
                    && self.hash_control_eligibility().allows(role)
                {
                    match role {
                        hash_jump::HashControl::Source => {
                            self.source.update(cx, |state, cx| state.focus(window, cx))
                        }
                        hash_jump::HashControl::Algorithm => self
                            .algorithm
                            .update(cx, |state, cx| state.focus(window, cx)),
                        hash_jump::HashControl::Text => {
                            self.text.focus_handle(cx).focus(window, cx)
                        }
                        hash_jump::HashControl::Expected => {
                            self.expected.focus_handle(cx).focus(window, cx)
                        }
                        _ => {
                            if let Some(focus) = self.jump.button(role) {
                                focus.focus(window, cx);
                            }
                        }
                    }
                }
            }
        }
        if repaint {
            cx.notify();
        }
        consumed
    }

    fn handle_escape(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        if !self.is_current_mount()
            || !hash_escape_capture_should_close(
                &window.context_stack(),
                self.focus_handle.contains_focused(window, cx),
                self.file_picker_pending.load(Ordering::Acquire),
            )
        {
            return false;
        }
        if self.jump.controller.owns_key(0x1b) {
            return true;
        }
        self.request_close(hash_pin::CloseOrigin::Keyboard, cx);
        true
    }
}


struct SecondaryWindow {
    query: Entity<InputState>,
    native_window: Option<NativeWindow>,
    secondary_revision: Arc<AtomicU64>,
    restart_admission: Arc<RestartAdmission>,
}

impl Render for SecondaryWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .w(px(420.))
                    .child(Label::new("Retained window input"))
                    .child(
                        Input::new(&self.query)
                            .aria_label("Retained window input")
                            .readonly(self.restart_admission.is_fenced()),
                    )
                    .child(
                        Button::new("dismiss-retained-window")
                            .label("Dismiss window")
                            .on_click(cx.listener(|this, _, _, cx| {
                                let Some(operation) = this.restart_admission.admit() else {
                                    return;
                                };
                                if let Some(native_window) = this.native_window {
                                    let secondary_revision = Arc::clone(&this.secondary_revision);
                                    let revision =
                                        secondary_revision.fetch_add(1, Ordering::AcqRel) + 1;
                                    cx.spawn(async move |_, _cx| {
                                        let _operation = operation;
                                        if secondary_revision.load(Ordering::Acquire) != revision {
                                            return;
                                        }
                                        if let Err(error) =
                                            set_native_window_visible(native_window, false)
                                        {
                                            tracing::warn!(%error, "could not dismiss retained GPUI window");
                                        }
                                    })
                                    .detach();
                                }
                            })),
                    ),
            )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ForegroundIdentity {
    hwnd: usize,
    process_id: u32,
    thread_id: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NativeWindow {
    identity: ForegroundIdentity,
}

fn checked_geometry_target(window: NativeWindow) -> Option<CheckedNativeWindow> {
    Some(CheckedNativeWindow {
        hwnd: window.identity.hwnd,
        process_id: window.identity.process_id,
        thread_id: window.identity.thread_id,
    })
}

fn outer_size_in_points(metrics: NativeWindowMetrics) -> Option<PreferredWindowSize> {
    if !metrics.scale_factor.is_finite() || metrics.scale_factor <= 0.0 {
        return None;
    }
    PreferredWindowSize::new(
        metrics.outer.width() as f32 / metrics.scale_factor,
        metrics.outer.height() as f32 / metrics.scale_factor,
    )
    .ok()
}

#[cfg(windows)]
fn checked_native_window(window: &Window) -> Result<Option<NativeWindow>, String> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow},
    };

    let handle = HasWindowHandle::window_handle(window)
        .map_err(|error| format!("GPUI did not expose a native window handle: {error}"))?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return Err("GPUI returned a non-Windows native window handle".to_owned());
    };
    let hwnd = HWND(handle.hwnd.get() as *mut c_void);
    if hwnd.0.is_null() || !unsafe { IsWindow(hwnd) }.as_bool() {
        return Err("GPUI returned an invalid Windows window handle".to_owned());
    }

    let mut process_id = 0;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    if process_id != std::process::id() || thread_id == 0 {
        return Err("GPUI window handle is not owned by this process".to_owned());
    }

    Ok(Some(NativeWindow {
        identity: ForegroundIdentity {
            hwnd: hwnd.0 as usize,
            process_id,
            thread_id,
        },
    }))
}

#[cfg(not(windows))]
fn checked_native_window(_: &Window) -> Result<Option<NativeWindow>, String> {
    Ok(None)
}

fn checked_active_dialog_owner(window: &Window) -> Result<Option<NativeWindow>, String> {
    let native_window = checked_native_window(window)?;
    #[cfg(windows)]
    {
        use windows::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;

        let Some(native_window) = native_window else {
            return Err("The Hash Generator window has no checked Windows owner".to_owned());
        };
        let active = native_identity(unsafe { GetActiveWindow() });
        if active != Some(native_window.identity) {
            return Err(
                "Activate this Hash Generator window before opening its file picker".to_owned(),
            );
        }
    }
    Ok(native_window)
}

#[cfg(windows)]
fn current_foreground_identity() -> Option<ForegroundIdentity> {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    let hwnd = unsafe { GetForegroundWindow() };
    native_identity(hwnd)
}

#[cfg(not(windows))]
fn current_foreground_identity() -> Option<ForegroundIdentity> {
    None
}

#[cfg(windows)]
fn native_identity(hwnd: windows::Win32::Foundation::HWND) -> Option<ForegroundIdentity> {
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow};
    if hwnd.0.is_null() || !unsafe { IsWindow(hwnd) }.as_bool() {
        return None;
    }
    let mut process_id = 0;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    (process_id != 0 && thread_id != 0).then_some(ForegroundIdentity {
        hwnd: hwnd.0 as usize,
        process_id,
        thread_id,
    })
}

#[cfg(windows)]
fn capture_foreground_identity() -> Option<ForegroundIdentity> {
    let foreground = current_foreground_identity()?;
    if foreground.process_id == std::process::id() {
        return Some(foreground);
    }

    multi_launcher::active_window::capture_external_foreground_window()?;
    (current_foreground_identity() == Some(foreground)).then_some(foreground)
}

#[cfg(not(windows))]
fn capture_foreground_identity() -> Option<ForegroundIdentity> {
    None
}

// Capture the title only while the same external native window remains foreground.
fn capture_snippet_origin(expected: Option<ForegroundIdentity>) -> Option<multi_launcher::active_window::ExternalForegroundWindow> {
    let expected = expected?;
    if current_foreground_identity() != Some(expected) { return None; }
    let origin = multi_launcher::active_window::capture_external_foreground_window()?;
    (origin.matches_identity(expected.hwnd, expected.process_id, expected.thread_id)
        && current_foreground_identity() == Some(expected)).then_some(origin)
}

#[cfg(windows)]
fn native_window_is_valid(window: NativeWindow) -> bool {
    use windows::Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow},
    };
    let hwnd = HWND(window.identity.hwnd as *mut c_void);
    if hwnd.0.is_null() || !unsafe { IsWindow(hwnd) }.as_bool() {
        return false;
    }
    let mut process_id = 0;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    process_id == window.identity.process_id && thread_id == window.identity.thread_id
}

#[cfg(not(windows))]
fn native_window_is_valid(_: NativeWindow) -> bool {
    false
}

#[cfg(windows)]
fn native_window_is_visible(window: NativeWindow) -> Result<bool, String> {
    use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::IsWindowVisible};
    if !native_window_is_valid(window) {
        return Err("the GPUI window handle is stale".to_owned());
    }
    Ok(unsafe { IsWindowVisible(HWND(window.identity.hwnd as *mut c_void)) }.as_bool())
}

#[cfg(not(windows))]
fn native_window_is_visible(_: NativeWindow) -> Result<bool, String> {
    Err("native GPUI window visibility is only available on Windows".to_owned())
}

#[cfg(windows)]
fn set_native_window_visible(window: NativeWindow, visible: bool) -> Result<(), String> {
    use windows::Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{
            IsWindowVisible, SHOW_WINDOW_CMD, SW_HIDE, SW_SHOWNOACTIVATE, ShowWindow,
        },
    };
    if !native_window_is_valid(window) {
        return Err("the GPUI window handle is stale".to_owned());
    }
    let hwnd = HWND(window.identity.hwnd as *mut c_void);
    let command: SHOW_WINDOW_CMD = if visible { SW_SHOWNOACTIVATE } else { SW_HIDE };
    // SAFETY: the HWND was captured from GPUI and revalidated against this process and thread.
    unsafe {
        let _ = ShowWindow(hwnd, command);
    }
    if !native_window_is_valid(window) || unsafe { IsWindowVisible(hwnd) }.as_bool() != visible {
        return Err("Windows did not apply the requested GPUI visibility".to_owned());
    }
    Ok(())
}

#[cfg(not(windows))]
fn set_native_window_visible(_: NativeWindow, _: bool) -> Result<(), String> {
    Err("native GPUI window visibility is only available on Windows".to_owned())
}

#[cfg(windows)]
fn activate_native_window(window: NativeWindow) -> Result<(), String> {
    use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::SetForegroundWindow};
    if !native_window_is_valid(window) {
        return Err("the GPUI window handle is stale".to_owned());
    }
    // One guarded USER32 attempt avoids an asynchronous retry that could steal
    // focus after the foreground has changed.
    if unsafe { SetForegroundWindow(HWND(window.identity.hwnd as *mut c_void)) }.as_bool() {
        Ok(())
    } else {
        Err("Windows refused to activate the GPUI window".to_owned())
    }
}

#[cfg(not(windows))]
fn activate_native_window(_: NativeWindow) -> Result<(), String> {
    Err("native GPUI window activation is only available on Windows".to_owned())
}

#[derive(Clone)]
struct RootVisibility {
    visible: Arc<AtomicBool>,
    request_revision: Arc<AtomicU64>,
    active: Arc<AtomicBool>,
    retired: Arc<AtomicBool>,
    window_search_update: Arc<AtomicBool>,
    snippet_origin: Arc<Mutex<Option<(u64, multi_launcher::active_window::ExternalForegroundWindow)>>>,
    #[cfg(windows)]
    request_sender: Arc<Mutex<Option<Sender<()>>>>,
    #[cfg(windows)]
    requested_visible: Arc<Mutex<Option<RootVisibilityRequest>>>,
}

enum RootVisibilityRequest {
    DockHash(hash_dock::Reveal),
    DockUtility(utility_mount::UtilityKind, hash_dock::UtilityReveal),
    HideAfterExternalLaunch(u64),
    HideAfterWindowSwitch(u64, ForegroundIdentity, Arc<str>),
    HideAfterWindowClose(u64, ForegroundIdentity, ForegroundIdentity, Arc<str>),
    HideAfterClipboardUndo(u64, ForegroundIdentity, u64, Arc<str>, Arc<AtomicBool>, Option<FocusHandle>, Option<Arc<dyn Fn() -> bool + Send + Sync>>),
}

fn exclude_closed_return_target(return_target: Option<ForegroundIdentity>, closed: ForegroundIdentity) -> Option<ForegroundIdentity> {
    return_target.filter(|target| *target != closed)
}

fn window_switch_hide_is_current(
    expected_target: ForegroundIdentity,
    foreground: Option<ForegroundIdentity>,
    expected_query: Option<&str>,
    live_query: Option<&str>,
) -> bool {
    foreground == Some(expected_target) && expected_query.is_some() && expected_query == live_query
}

fn window_close_hide_is_current(expected_root: ForegroundIdentity, foreground: Option<ForegroundIdentity>, expected_query: &str, live_query: Option<&str>) -> bool {
    foreground == Some(expected_root) && live_query == Some(expected_query)
}
fn clipboard_undo_hide_is_current(expected_root: ForegroundIdentity, foreground: Option<ForegroundIdentity>, expected_generation: u64, generation: u64, expected_query: &str, query: &str, lease: bool) -> bool {
    lease && foreground == Some(expected_root) && generation == expected_generation && query == expected_query
}

fn utility_dock_focus_allowed(action_sheet: bool, dialog: bool, popup: bool) -> bool {
    !action_sheet && !dialog && !popup
}
fn utility_dock_ack_allowed(activated: bool, delivered: bool, current: bool) -> bool {
    activated && delivered && current
}

impl RootWindow {
    fn utility_dock_focus_available(&self, window: &mut Window, cx: &mut App) -> bool {
        let contexts = window.context_stack();
        utility_dock_focus_allowed(self.action_sheet.is_some(), window.has_active_dialog(cx),
            context_stack_has_primary(&contexts, "PopupMenu") || context_stack_has_primary(&contexts, "Dialog") || context_stack_has_primary(&contexts, "AlertDialog"))
    }
    fn utility_destination_current(&self, kind: utility_mount::UtilityKind, receipt: &utility_mount::MountReceipt, window: WindowId, cx: &App) -> bool {
        match kind {
            utility_mount::UtilityKind::FileLock => self.file_lock_view.as_ref().is_some_and(|view| {
                let view = view.read(cx);
                view.is_mounted_in(window, false) && receipt.matches(&view.mount) && receipt.is_current()
            }),
            utility_mount::UtilityKind::FolderSize => self.folder_size_view.as_ref().is_some_and(|view| {
                let view = view.read(cx);
                view.is_mounted_in(window, false) && receipt.matches(&view.mount) && receipt.is_current()
            }),
            utility_mount::UtilityKind::FileSearch => self.file_search.as_ref().is_some_and(|view| {
                let view = view.read(cx);
                view.is_mounted_in(window, false) && receipt.matches(&view.mount_receipt()) && receipt.is_current()
            }),
            utility_mount::UtilityKind::Json => self.json_view.as_ref().is_some_and(|view| {
                let view = view.read(cx);
                view.is_mounted_in(window, false) && receipt.matches(&view.mount_receipt()) && receipt.is_current()
            }),
            utility_mount::UtilityKind::Xml => self.xml_view.as_ref().is_some_and(|view| {
                let view = view.read(cx);
                view.is_mounted_in(window, false) && receipt.matches(&view.mount_receipt()) && receipt.is_current()
            }),
            utility_mount::UtilityKind::ClipboardHistory => self.clipboard_history.as_ref().is_some_and(|view| {
                let view = view.read(cx);
                view.is_mounted_in(window, false) && receipt.matches(&view.mount_receipt()) && receipt.is_current()
            }),
            utility_mount::UtilityKind::ClipboardModify => self.clipboard_modify_view.as_ref().is_some_and(|view| {
                let view = view.read(cx);
                view.is_mounted_in(window, false) && receipt.matches(&view.mount_receipt()) && receipt.is_current()
            }),
            utility_mount::UtilityKind::Regex => self.regex_view.as_ref().is_some_and(|view| {
                let view = view.read(cx);
                view.is_mounted_in(window, false) && receipt.matches(&view.mount_receipt()) && receipt.is_current()
            }),
        }
    }
}

impl RootVisibilityRequest {
    fn is_current(&self, revision: u64, visible: bool, newer_pending: bool) -> bool {
        match self {
            Self::DockHash(reveal) => reveal.gesture.visibility_revision == revision && !newer_pending,
            Self::DockUtility(_, reveal) => reveal.gesture.visibility_revision == revision && reveal.mount.is_current() && !newer_pending,
            Self::HideAfterExternalLaunch(expected) | Self::HideAfterWindowSwitch(expected, _, _)
                | Self::HideAfterWindowClose(expected, _, _, _)
                | Self::HideAfterClipboardUndo(expected, _, _, _, _, _, _) => {
                *expected == revision && visible && !newer_pending
            }
        }
    }

    fn superseded_by_hotkey(&self, take_hotkey: impl FnOnce() -> bool) -> bool {
        matches!(self, Self::HideAfterExternalLaunch(_) | Self::HideAfterWindowSwitch(_, _, _) | Self::HideAfterWindowClose(_, _, _, _) | Self::HideAfterClipboardUndo(_, _, _, _, _, _, _)) && take_hotkey()
    }
}

impl RootVisibility {
    fn new() -> Self {
        Self {
            visible: Arc::new(AtomicBool::new(true)),
            request_revision: Arc::new(AtomicU64::new(0)),
            active: Arc::new(AtomicBool::new(false)),
            retired: Arc::new(AtomicBool::new(false)),
            window_search_update: Arc::new(AtomicBool::new(false)),
            snippet_origin: Arc::new(Mutex::new(None)),
            #[cfg(windows)]
            request_sender: Arc::new(Mutex::new(None)),
            #[cfg(windows)]
            requested_visible: Arc::new(Mutex::new(None)),
        }
    }

    #[cfg(windows)]
    fn bind_request_sender(&self, sender: Sender<()>) {
        if let Ok(mut slot) = self.request_sender.lock()
            && !self.retired.load(Ordering::Acquire)
        {
            *slot = Some(sender);
        }
    }

    #[cfg(windows)]
    fn activate(&self) -> bool {
        let Ok(sender) = self.request_sender.lock() else {
            return false;
        };
        if self.retired.load(Ordering::Acquire) || sender.is_none() {
            return false;
        }
        self.active.store(true, Ordering::Release);
        true
    }

    /// Retire queued work and wake the blocking receiver exactly once.
    fn deactivate(&self) {
        #[cfg(windows)]
        {
            let mut sender = self
                .request_sender
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if self.retired.swap(true, Ordering::AcqRel) {
                return;
            }
            self.active.store(false, Ordering::Release);
            self.request_revision.fetch_add(1, Ordering::AcqRel);
            *self
                .requested_visible
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
            if let Some(sender) = sender.take() {
                let _ = sender.send(());
            }
        }
        #[cfg(not(windows))]
        {
            if !self.retired.swap(true, Ordering::AcqRel) {
                self.active.store(false, Ordering::Release);
                self.request_revision.fetch_add(1, Ordering::AcqRel);
            }
        }
    }

    fn notify_window_search(&self) {
        #[cfg(windows)]
        if self.active.load(Ordering::Acquire) && !self.retired.load(Ordering::Acquire) {
            self.window_search_update.store(true, Ordering::Release);
            if let Ok(sender) = self.request_sender.lock() {
                if let Some(sender) = sender.as_ref() { let _ = sender.send(()); }
            }
        }
    }

    fn request_window_switch_hide(&self, revision: u64, target: ForegroundIdentity, query: &str) -> bool {
        if current_foreground_identity() != Some(target) { return false; }
        self.queue_visible(RootVisibilityRequest::HideAfterWindowSwitch(revision, target, Arc::from(query)))
    }

    fn request_external_hide(&self, revision: u64) -> bool {
        self.queue_visible(RootVisibilityRequest::HideAfterExternalLaunch(revision))
    }

    fn request_clipboard_undo_hide(&self, revision: u64, root: ForegroundIdentity, generation: u64, query: &str, lease: Arc<AtomicBool>, focus: Option<FocusHandle>, catalog_current: Option<Arc<dyn Fn() -> bool + Send + Sync>>) -> bool {
        self.queue_visible(RootVisibilityRequest::HideAfterClipboardUndo(revision, root, generation, Arc::from(query), lease, focus, catalog_current))
    }

    fn request_window_close_hide(&self, revision: u64, root: ForegroundIdentity, target: ForegroundIdentity, query: &str) -> bool {
        self.queue_visible(RootVisibilityRequest::HideAfterWindowClose(revision, root, target, Arc::from(query)))
    }

    fn queue_visible(&self, request: RootVisibilityRequest) -> bool {
        #[cfg(windows)]
        {
            // Serialize request publication with retirement, including Hash focus.
            let Ok(sender) = self.request_sender.lock() else {
                return false;
            };
            if !self.active.load(Ordering::Acquire) || self.retired.load(Ordering::Acquire) {
                return false;
            }
            let Some(sender) = sender.as_ref() else {
                return false;
            };
            let Ok(mut pending) = self.requested_visible.lock() else {
                return false;
            };
            let supersedes_dock = matches!(pending.as_ref(), Some(RootVisibilityRequest::DockHash(_) | RootVisibilityRequest::DockUtility(_, _)))
                && matches!(&request, RootVisibilityRequest::HideAfterExternalLaunch(_)
                    | RootVisibilityRequest::HideAfterWindowSwitch(_, _, _)
                    | RootVisibilityRequest::HideAfterWindowClose(_, _, _, _));
            let dock_supersedes_undo = matches!(pending.as_ref(), Some(RootVisibilityRequest::HideAfterClipboardUndo(_, _, _, _, _, _, _)))
                && matches!(&request, RootVisibilityRequest::DockHash(_) | RootVisibilityRequest::DockUtility(_, _));
            if !request.is_current(
                self.request_revision.load(Ordering::Acquire),
                self.visible.load(Ordering::Acquire),
                pending.is_some() && !supersedes_dock && !dock_supersedes_undo,
            ) {
                return false;
            }
            *pending = Some(request);
            if sender.send(()).is_ok() {
                return true;
            }
            *pending = None;
            false
        }
        #[cfg(not(windows))]
        {
            let _ = request;
            false
        }
    }

    fn request_hash_visible(&self, gesture: hash_dock::Gesture, mount_generation: u64) -> Option<Arc<HashWakeSignal>> {
        let receiver = Arc::new(HashWakeSignal::default());
        let acknowledgement = hash_dock::Acknowledgement(Arc::clone(&receiver));
        self.queue_visible(RootVisibilityRequest::DockHash(hash_dock::Reveal { gesture, mount_generation, acknowledgement })).then_some(receiver)
    }
    fn request_utility_visible(&self, kind: utility_mount::UtilityKind, gesture: hash_dock::Gesture, mount: utility_mount::MountReceipt) -> Option<(Arc<HashWakeSignal>, Arc<AtomicU64>)> {
        let receiver = Arc::new(HashWakeSignal::default());
        let accepted = Arc::new(AtomicU64::new(0));
        let acknowledgement = hash_dock::UtilityAcknowledgement { signal: receiver.clone(), accepted_revision: accepted.clone() };
        let reveal = hash_dock::UtilityReveal { gesture, mount, acknowledgement };
        let request = RootVisibilityRequest::DockUtility(kind, reveal);
        self.queue_visible(request).then_some((receiver, accepted))
    }
    fn request_xml_visible(&self, gesture: hash_dock::Gesture, mount: utility_mount::MountReceipt) -> Option<(Arc<HashWakeSignal>, Arc<AtomicU64>)> {
        self.request_utility_visible(utility_mount::UtilityKind::Xml, gesture, mount)
    }

    #[cfg(windows)]
    fn take_requested_visible(&self) -> Option<RootVisibilityRequest> {
        self.requested_visible
            .lock()
            .ok()
            .and_then(|mut pending| pending.take())
    }

    #[cfg(windows)]
    fn accept_requested_visible(&self, request: &RootVisibilityRequest) -> Option<(u64, bool)> {
        let pending = self.requested_visible.lock().ok()?;
        if !self.active.load(Ordering::Acquire)
            || !request.is_current(
                self.request_revision.load(Ordering::Acquire),
                self.visible.load(Ordering::Acquire),
                pending.is_some(),
            )
        {
            return None;
        }
        let visible = match request {
            RootVisibilityRequest::DockHash(_) => true,
            RootVisibilityRequest::DockUtility(_, _) => true,
            RootVisibilityRequest::HideAfterExternalLaunch(_) => false,
            RootVisibilityRequest::HideAfterWindowClose(_, _, _, _) => false,
            RootVisibilityRequest::HideAfterClipboardUndo(_, root, _, _, lease, _, _) => {
                if !lease.load(Ordering::Acquire) || current_foreground_identity() != Some(*root) { return None; }
                false
            }
            RootVisibilityRequest::HideAfterWindowSwitch(_, target, _) => {
                if current_foreground_identity() != Some(*target) { return None; }
                false
            }
        };
        Some(self.set_visible(visible))
    }

    fn set_visible(&self, visible: bool) -> (u64, bool) {
        let revision = self
            .request_revision
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1)
            .max(1);
        self.visible.store(visible, Ordering::Release);
        (revision, visible)
    }

    fn next_toggle(&self) -> (u64, bool) {
        let visible = !self.visible.load(Ordering::Acquire);
        self.set_visible(visible)
    }

    fn is_current(&self, revision: u64, visible: bool) -> bool {
        self.active.load(Ordering::Acquire)
            && self.request_revision.load(Ordering::Acquire) == revision
            && self.visible.load(Ordering::Acquire) == visible
    }

    #[cfg(windows)]
    fn current_visible_revision(&self, native_window: NativeWindow) -> Option<u64> {
        use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::IsIconic};

        let revision = self.request_revision.load(Ordering::Acquire);
        let active = self.active.load(Ordering::Acquire);
        let visible = self.visible.load(Ordering::Acquire);
        if !active || !visible {
            return None;
        }
        // A minimized HWND retains WS_VISIBLE. Match the backend's displayed
        // window condition before accepting a Help press or its delivery.
        let native_window_visible = native_window_is_visible(native_window).unwrap_or(false)
            && !unsafe { IsIconic(HWND(native_window.identity.hwnd as *mut c_void)) }.as_bool();
        let current_revision = self.request_revision.load(Ordering::Acquire);
        quick_help_event_targets_visible_revision(
            active && self.active.load(Ordering::Acquire),
            visible && self.visible.load(Ordering::Acquire),
            native_window_visible,
            revision,
            current_revision,
        )
        .then_some(revision)
    }

    #[cfg(windows)]
    fn is_current_visible_revision(&self, revision: u64, native_window: NativeWindow) -> bool {
        self.current_visible_revision(native_window) == Some(revision)
    }

    fn revert_if_current(&self, revision: u64, visible: bool) {
        if self.request_revision.load(Ordering::Acquire) == revision
            && self.visible.load(Ordering::Acquire) == visible
        {
            self.visible.store(!visible, Ordering::Release);
            if !visible {
                if let Ok(mut saved) = self.snippet_origin.lock() {
                    if let Some((issued, _)) = saved.as_mut() { *issued = revision; }
                }
            }
        }
    }
}

#[cfg(any(test, windows))]
fn reveal_edge_is_current(revision_current: bool, newer_hotkey_pending: bool) -> bool {
    revision_current && !newer_hotkey_pending
}

#[cfg(windows)]
fn hotkey_edge_pending(trigger: &HotkeyTrigger) -> bool {
    trigger.open.lock().map(|pending| *pending).unwrap_or(true)
}

#[cfg(any(test, windows))]
fn rollback_reveal_unless_hotkey_pending(visibility: &RootVisibility, revision: u64, trigger: &multi_launcher::hotkey::runtime::HotkeyTrigger) -> bool {
    if let Ok(pending) = trigger.open.lock() {
        if !*pending && visibility.is_current(revision, true) {
            visibility.revert_if_current(revision, true);
            return true;
        }
    }
    false
}

#[cfg(windows)]
fn activate_for_root_request(
    visibility: &RootVisibility,
    revision: u64,
    desired_visible: bool,
    target: NativeWindow,
    expected_foreground: Option<ForegroundIdentity>,
    newer_hotkey: Option<&HotkeyTrigger>,
) -> bool {
    let current = || reveal_edge_is_current(visibility.is_current(revision, desired_visible),
        newer_hotkey.is_some_and(hotkey_edge_pending));
    if !current() || !native_window_is_valid(target) {
        return false;
    }
    let foreground = current_foreground_identity();
    if foreground == Some(target.identity) {
        return false;
    }
    let Some(expected_foreground) = expected_foreground else {
        return false;
    };
    if foreground != Some(expected_foreground) {
        return false;
    }
    if !current() {
        return false;
    }
    if let Err(error) = activate_native_window(target) {
        tracing::debug!(%error, "GPUI foreground activation was refused");
        return false;
    }
    current() && current_foreground_identity() == Some(target.identity)
}

#[cfg(windows)]
fn activate_for_hash_pin(
    revision_guard: &AtomicU64,
    revision: u64,
    window: NativeWindow,
    expected_foreground: Option<ForegroundIdentity>,
) -> bool {
    if revision_guard.load(Ordering::Acquire) != revision || !native_window_is_valid(window) {
        return false;
    }
    let foreground = current_foreground_identity();
    if foreground == Some(window.identity) { return true; }
    if expected_foreground.is_none() || foreground != expected_foreground
        || revision_guard.load(Ordering::Acquire) != revision { return false; }
    activate_native_window(window).is_ok()
}

#[cfg(not(windows))]
fn activate_for_hash_pin(_: &AtomicU64, _: u64, _: NativeWindow, _: Option<ForegroundIdentity>) -> bool {
    false
}

#[cfg(windows)]
fn activate_for_secondary_window(
    secondary_revision: &AtomicU64,
    revision: u64,
    window: NativeWindow,
    expected_foreground: Option<ForegroundIdentity>,
) {
    activate_for_secondary_window_if(secondary_revision, revision, window, expected_foreground, &|| true);
}

#[cfg(any(windows, test))]
fn secondary_activation_request_allowed(already_foreground: bool, expected_foreground: bool, current: bool) -> bool {
    !already_foreground && expected_foreground && current
}

#[cfg(windows)]
fn activate_for_secondary_window_if(
    secondary_revision: &AtomicU64,
    revision: u64,
    window: NativeWindow,
    expected_foreground: Option<ForegroundIdentity>,
    current: &dyn Fn() -> bool,
) -> bool {
    if secondary_revision.load(Ordering::Acquire) != revision || !native_window_is_valid(window) || !current() { return false; }
    let foreground = current_foreground_identity();
    let Some(expected_foreground) = expected_foreground else { return false; };
    if !secondary_activation_request_allowed(foreground == Some(window.identity), foreground == Some(expected_foreground),
        secondary_revision.load(Ordering::Acquire) == revision && current()) { return false; }
    match activate_native_window(window) {
        Ok(()) => true,
        Err(error) => { tracing::debug!(%error, "retained GPUI window activation was refused"); false }
    }
}

#[cfg(not(windows))]
fn activate_for_secondary_window(
    _: &AtomicU64,
    _: u64,
    _: NativeWindow,
    _: Option<ForegroundIdentity>,
) {
}

#[cfg(not(windows))]
fn activate_for_secondary_window_if(_: &AtomicU64, _: u64, _: NativeWindow, _: Option<ForegroundIdentity>, _: &dyn Fn() -> bool) -> bool { false }

pub(crate) fn run_foundation_window(
    hotkey: Hotkey,
    launcher_hotkey_display: String,
    configured_help_hotkey: Option<String>,
    saved_pinned_panels: Vec<Panel>,
    enabled_plugins: Option<HashSet<String>>,
    enabled_capabilities: Option<HashMap<String, Vec<String>>>,
    search_startup: LauncherSearchStartup,
    root_placement: root_placement::RootPlacement,
    file_search_settings: multi_launcher::file_search::settings::FileSearchSettings,
    #[cfg(windows)] app_data_root: multi_launcher::platform::app_data::AppDataRoot,
    external_launch_preferences: ExternalLaunchPreferences,
    coordinate_tool_preferences: CoordinateToolPreferences,
    theme_settings: multi_launcher::settings::ThemeSettings,
    settings_path: String,
    saved_utility_presentations: UtilityPresentations,
    saved_utility_sizes: UtilityDialogSizes,
    settings_save_workers: SettingsSaveWorkerRegistry,
) -> Result<Option<RestartPlan>, String> {
    let quick_help_shortcut_admission = admit_quick_help_shortcut(
        configured_help_hotkey.as_deref(),
        hotkey,
        search_startup.jump_mode_enabled,
        search_startup.jump_mode_activation_chord,
    );
    let quick_help_hotkey_lines = quick_help_hotkey_lines(
        &launcher_hotkey_display,
        quick_help_shortcut_admission.shortcut.as_ref(),
    );
    let quick_help_shortcut_diagnostic = quick_help_shortcut_admission.diagnostic.clone();
    let quick_help_shortcut_for_run = quick_help_shortcut_admission.shortcut.clone();
    let search_jump_mode_for_root = JumpModeController::new(
        search_startup.jump_mode_enabled,
        search_startup.jump_mode_activation_chord,
        search_startup.jump_mode_hint_alphabet.clone(),
    );
    let external_launch_preferences_for_root = external_launch_preferences;
    let launcher_search_catalog =
        LauncherSearchCatalog::new(search_startup, enabled_plugins.as_ref());
    let search_jump_geometry_frame_for_root =
        Rc::new(RefCell::new(None::<JumpGeometryFrame>));
    let hash_tool_key = UtilityToolKey::parse("hash-generator")
        .map_err(|error| format!("Hash Generator has no stable presentation key: {error}"))?;
    let utility_presentations = UtilityPresentationStore::new(
        settings_path.clone(),
        settings_save_workers.clone(),
        saved_utility_presentations,
    );
    let settings_path_for_root = settings_path.clone();
    let settings_save_workers_for_root = settings_save_workers.clone();
    let mut utility_sizes = UtilitySizeStore::new(
        settings_path,
        settings_save_workers.clone(),
        saved_utility_sizes,
    );
    utility_sizes.track_key(hash_tool_key);
    let hash_docked_preferred_size = match utility_sizes.preference(hash_tool_key) {
        multi_launcher::settings::UtilitySizePreference::Preferred(size) => size,
        _ => HASH_DOCK_FACTORY_SIZE,
    };
    let hash_docked_preferred_size_for_root = hash_docked_preferred_size;
    let utility_size_store = Arc::new(Mutex::new(utility_sizes));
    let utility_size_store_after_run = Arc::clone(&utility_size_store);
    let utility_size_polling = Arc::new(AtomicBool::new(false));
    let utility_size_polling_for_root = Arc::clone(&utility_size_polling);
    let hash_geometry_sequence = Arc::new(AtomicU64::new(0));
    let hash_geometry_sequence_for_root = Arc::clone(&hash_geometry_sequence);
    let restart_context = RestartContext::capture().ok();
    let restart_exit = Rc::new(RefCell::new(None));
    let restart_exit_after_run = Rc::clone(&restart_exit);
    let startup_error = Rc::new(RefCell::new(None));
    let startup_error_sink = Rc::clone(&startup_error);
    let subscriptions = Rc::new(RefCell::new(Vec::new()));
    let subscription_sink = Rc::clone(&subscriptions);
    let root_for_quit = Rc::new(RefCell::new(None::<WeakEntity<RootWindow>>));
    let root_for_quit_callback = Rc::clone(&root_for_quit);
    let listener_slot = Arc::new(Mutex::new(None::<HotkeyListener>));
    let listener_slot_for_run = Arc::clone(&listener_slot);
    let listener_slot_for_quit = Arc::clone(&listener_slot);
    let listener_slot_for_root = Arc::clone(&listener_slot);
    let restart_admission = Arc::new(RestartAdmission::default());
    let restart_admission_for_quit = Arc::clone(&restart_admission);
    let settings_save_workers_for_quit = settings_save_workers.clone();
    let restart_admission_for_root = Arc::clone(&restart_admission);
    let restart_admission_for_hotkey = Arc::clone(&restart_admission);
    let restart_admission_for_quick_help_capture = Arc::clone(&restart_admission);
    let visibility = RootVisibility::new();
    let visibility_after_run = visibility.clone();
    let visibility_for_quit = visibility.clone();
    let visibility_for_root = visibility.clone();
    let restart_retiring = Arc::new(AtomicBool::new(false));
    let restart_retiring_for_root = Arc::clone(&restart_retiring);
    let restart_retiring_for_root_close = Arc::clone(&restart_retiring);
    let restart_failure = Rc::new(RefCell::new(None));
    let restart_failure_after_run = Rc::clone(&restart_failure);
    let initial_return_target = capture_foreground_identity();
    let initial_snippet_origin = capture_snippet_origin(initial_return_target);
    let (activity_center, hash_session) = new_activity_hash_session();
    let hash_session_for_quit = Arc::clone(&hash_session);
    let hash_session_for_root = Arc::clone(&hash_session);
    let hash_window_revision = Arc::new(AtomicU64::new(0));
    let hash_window_revision_for_app = Arc::clone(&hash_window_revision);
    let hash_mount_generation = Arc::new(AtomicU64::new(0));
    let hash_mount_generation_for_quit = Arc::clone(&hash_mount_generation);
    let hash_mount_generation_for_root = Arc::clone(&hash_mount_generation);
    let hash_mount_generation_for_dock = Arc::clone(&hash_mount_generation);
    let hash_rehost_pending = Arc::new(AtomicBool::new(false));
    let hash_rehost_pending_for_quit = Arc::clone(&hash_rehost_pending);
    let hash_rehost_pending_for_root = Arc::clone(&hash_rehost_pending);
    let hash_rehost_revision = Arc::new(AtomicU64::new(0));
    let hash_rehost_revision_for_root = Arc::clone(&hash_rehost_revision);
    let hash_presentation_feedback = Arc::new(Mutex::new(None));
    let hash_presentation_feedback_for_root = Arc::clone(&hash_presentation_feedback);
    let hash_file_picker_pending = Arc::new(AtomicBool::new(false));
    let hash_file_picker_pending_for_quit = Arc::clone(&hash_file_picker_pending);
    let hash_file_picker_pending_for_root = Arc::clone(&hash_file_picker_pending);
    let hash_clipboard_pending = Arc::new(HashClipboardPendingState::default());
    let hash_clipboard_pending_for_root = Arc::clone(&hash_clipboard_pending);
    let hash_wake_signal = Arc::new(HashWakeSignal::default());
    let hash_wake_signal_for_quit = Arc::clone(&hash_wake_signal);
    let hash_wake_signal_for_root = Arc::clone(&hash_wake_signal);
    let hash_wake_signal_for_task = Arc::clone(&hash_wake_signal);
    let hash_wake_signal_for_root_close = Arc::clone(&hash_wake_signal);
    let coordinate_hud_probe_controller =
        Arc::new(Mutex::new(CoordinateToolController::new_with_preferences(
            Arc::new(NativeCoordinateRuntimeFactory),
            coordinate_tool_preferences,
        )));
    let coordinate_hud_probe_closing = Arc::new(AtomicBool::new(false));
    let coordinate_shutdown_complete = Arc::new(AtomicBool::new(false));
    let coordinate_shutdown_complete_for_root = Arc::clone(&coordinate_shutdown_complete);
    let controller_for_app = Arc::clone(&coordinate_hud_probe_controller);
    let closing_for_app = Arc::clone(&coordinate_hud_probe_closing);
    let utility_presentations_after_run = utility_presentations.clone();

    let startup_pin_order = hash_pin::startup_order(&saved_pinned_panels);
    let appearance_settings_for_run = theme_settings.clone();
    application().with_assets(assets::Assets).run(move |cx| {
        init(cx);
        let initial_appearance = appearance::system_appearance(cx.window_appearance());
        appearance::apply_saved_theme(
            &appearance_settings_for_run,
            initial_appearance,
            cx,
        );
        cx.bind_keys(gpui_key_bindings());
        let coordinate_hud_probe_closing_for_quit = Arc::clone(&closing_for_app);
        let hash_session_for_quit = Arc::clone(&hash_session_for_quit);
        let hash_wake_signal_for_quit = Arc::clone(&hash_wake_signal_for_quit);
        let hash_window_revision_for_quit = Arc::clone(&hash_window_revision_for_app);
        let restart_admission_for_quit = Arc::clone(&restart_admission_for_quit);
        let settings_save_workers_for_quit = settings_save_workers_for_quit.clone();
        let quit_subscription = cx.on_app_quit(move |app| {
            if let Some(root) = root_for_quit_callback.borrow().clone() {
                let _ = root.update(app, |root, cx| {
                    root.snippets_search.wake.close();
                    if let Some(token) = root.settings_search.as_ref().and_then(|view| view.read(cx).pending_token()) { token.request_cancel(); }
                    root.retire_search_jump_mode();
                    root.drain_file_search_geometry(cx, true);
                    root.drain_folder_size_geometry(cx, true);
                    root.drain_file_lock_geometry(cx, true);
                    root.drain_file_preview_geometry(cx, true);
                    if let Some(view) = root.file_search.clone() { view.update(cx, |view, cx| view.shutdown(cx)); }
                    if let Some(view) = root.folder_size_view.clone() { view.update(cx, |view, _| view.shutdown()); }
                    if let Some(view) = root.file_lock_view.clone() { view.update(cx, |view, _| view.shutdown()); }
                    root.retire_json_workspace(cx);
                    root.drain_regex_native_geometry(cx, true);
                    root.drain_clipboard_native_geometry(cx, true);
                    root.drain_clipboard_modify_geometry(cx, true);
                    if let Some(view) = &root.clipboard_history { view.update(cx, |view, _| view.shutdown()); }
                    if let Some(view) = &root.clipboard_modify_view { view.update(cx, |view, _| view.shutdown_host()); }
                    if let Some(view) = &root.regex_view { view.update(cx, |view, _| view.shutdown()); }
                    #[cfg(windows)]
                    if let Some(view) = root.downloads_triage.clone() { view.update(cx, |view, _| view.shutdown()); }
                    root.drain_hash_native_geometry(cx, true);
                });
            }
            settings_save_workers_for_quit.begin_shutdown();
            visibility_for_quit.deactivate();
            restart_admission_for_quit.fence();
            coordinate_hud_probe_closing_for_quit.store(true, Ordering::Release);
            hash_wake_signal_for_quit.close();
            hash_window_revision_for_quit.fetch_add(1, Ordering::AcqRel);
            hash_session_for_quit
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .shutdown();
            hash_mount_generation_for_quit.store(0, Ordering::Release);
            hash_rehost_pending_for_quit.store(false, Ordering::Release);
            hash_file_picker_pending_for_quit.store(false, Ordering::Release);
            if let Ok(mut listener) = listener_slot_for_quit.lock()
                && let Some(mut listener) = listener.take()
            {
                listener.stop();
            }
            async {}
        });
        subscription_sink.borrow_mut().push(quit_subscription);

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(root_placement.initial_client.0 as f32), px(root_placement.initial_client.1 as f32)), cx)),
            window_min_size: Some(size(px(320.0), px(160.0))),
            titlebar: Some(TitlebarOptions {
                title: Some("Multi Launcher GPUI Foundation".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let controller_for_root = Arc::clone(&controller_for_app);
        let closing_for_root = Arc::clone(&closing_for_app);
        let utility_sizes_for_root = Arc::clone(&utility_size_store);
        let utility_size_polling_for_root = Arc::clone(&utility_size_polling_for_root);
        let hash_geometry_sequence_for_root = Arc::clone(&hash_geometry_sequence_for_root);
        let session_for_root = Arc::clone(&hash_session_for_root);
        let wake_signal_for_root = Arc::clone(&hash_wake_signal_for_root);
        let picker_pending_for_root_close = Arc::clone(&hash_file_picker_pending_for_root);
        let clipboard_pending_for_root_close = Arc::clone(&hash_clipboard_pending_for_root);
        let window_revision_for_root = Arc::clone(&hash_window_revision_for_app);
        let restart_exit_for_root = Rc::clone(&restart_exit);
        let restart_failure_for_root = Rc::clone(&restart_failure);
        let restart_context_for_root = restart_context.clone();
        let theme_settings_for_root = theme_settings.clone();
        let appearance_subscription_sink = Rc::clone(&subscription_sink);
        let quick_help_hotkeys_for_root = quick_help_hotkey_lines.clone();
        let quick_help_diagnostic_for_root = quick_help_shortcut_diagnostic.clone();
        let (root_window, root_view) = match open_window(options, cx, move |window, cx| {
            if appearance::should_observe_system(&theme_settings_for_root) {
                let settings = theme_settings_for_root.clone();
                let appearance_subscription = window.observe_window_appearance(move |window, cx| {
                    appearance::apply_saved_theme(
                        &settings,
                        appearance::system_appearance(window.appearance()),
                        cx,
                    );
                });
                appearance_subscription_sink
                    .borrow_mut()
                    .push(appearance_subscription);
            }
            let query = cx.new(|cx| InputState::new(window, cx).placeholder("Type a query"));
            query.focus_handle(cx).focus(window, cx);
            let focus_handle = cx.focus_handle();
            focus_handle.focus(window, cx);
            let resize_focus_handle = cx.focus_handle();
            let search_catalog_for_root = launcher_search_catalog;
            let settings_editor_for_root =
                SettingsEditorState::default();
            let command_help_for_root =
                CommandHelpState::new(search_catalog_for_root.startup_show_examples);
            let command_help_filter =
                cx.new(|cx| InputState::new(window, cx).placeholder("Filter commands"));
            let quick_help_filter =
                cx.new(|cx| InputState::new(window, cx).placeholder("Filter Quick Help providers"));
            let settings_focus_handle = cx.focus_handle();
            let settings_owner_focus_handle = cx.focus_handle();
            let settings_menu_focus_handle = cx.focus_handle();
            let favorites_pin_focus_handle = cx.focus_handle();
            let hash_pin_focus_handle = cx.focus_handle();
            let json_pin_focus_handle = cx.focus_handle();
            let xml_pin_focus_handle = cx.focus_handle();
            let command_help_owner_focus_handle = cx.focus_handle();
            let command_help_close_focus_handle = cx.focus_handle();
            let quick_help_owner_focus_handle = cx.focus_handle();
            let quick_help_close_focus_handle = cx.focus_handle();
            let toast_log_focus_handle = cx.focus_handle();
            let toast_log_owner_focus_handle = cx.focus_handle();
            let initial_launcher_results =
                search_catalog_for_root.initial_results(enabled_plugins.as_ref());
            let root = cx.new(move |cx| RootWindow {
                root_window: None,
                listener_slot: Arc::clone(&listener_slot_for_root),
                root_visibility: visibility_for_root.clone(),
                query,
                enabled_plugins: enabled_plugins.clone(),
                enabled_capabilities: enabled_capabilities.clone(),
                search_catalog: search_catalog_for_root,
                launcher_results: initial_launcher_results,
                calculator_search: calculator_search::SearchOwner::default(),
                indexed_applications: indexed_applications::Owner::default(),
                window_search: window_search::Owner::default(),
                file_search_probe: file_search::ProbeOwner::default(),
                file_search_popup_intent: Arc::new(AtomicBool::new(false)),
                external_launch: external_launch::ExternalLaunch::default(),
                external_launch_preferences: external_launch_preferences_for_root.clone(),
                action_sheet: None,
                action_sheet_serial: 0,
                context_favorite_popup: None,
                context_favorite_serial: 0,
                result_favorites: ResultFavorites::new(saved_pinned_panels.contains(&Panel::FavDialog)),
                panel_stack: Vec::new(),
                hash_pin: hash_pin::HashPinState::new(&saved_pinned_panels),
                structured_pins: structured_pin::StructuredPins::new(&saved_pinned_panels),
                launcher_results_scroll: ScrollHandle::new(),
                query_history:
                    QueryHistoryNavigator::default(),
                fresh_search_enter: None,
                search_jump_mode: search_jump_mode_for_root,
                search_jump_geometry_frame: Rc::clone(&search_jump_geometry_frame_for_root),
                search_completion: SearchCompletion::default(),
                search_completion_focus_revision: None,
                settings_editor: settings_editor_for_root,
                settings_search: None,
                settings_navigation: None,
                settings_panel_scroll: ScrollHandle::new(),
                settings_actions: None,
                app_editor: add_action::AppEditorState::new(cx),
                plugin_editor: None,
                command_help: command_help_for_root,
                command_help_filter,
                quick_help: QuickHelpState::default(),
                quick_help_filter,
                shared_help_filter: SharedHelpFilter::default(),
                pending_help_filter_sync: None,
                quick_help_shortcut_diagnostic: quick_help_diagnostic_for_root,
                quick_help_return_focus: None,
                quick_help_return_scroll_offset: None,
                quick_help_owner_focus_handle,
                quick_help_close_focus_handle,
                quick_help_scroll: ScrollHandle::new(),
                quick_help_hotkeys: quick_help_hotkeys_for_root,
                quick_help_paste_sequence_summary:
                    "Playback shortcut status is unavailable in GPUI.".to_owned(),
                quick_help_sequence_starts: Vec::<String>::new().into(),
                settings_path: settings_path_for_root.clone(),
                settings_save_workers: settings_save_workers_for_root.clone(),
                command_help_save_permit: None,
                command_help_save_token: None,
                preference_close_after_terminal: false,
                preference_close_failed: false,
                file_menu_open: false,
                settings_menu_open: false,
                help_menu_open: false,
                root_menu_previous_focus: None,
                root_menu_action_routed_focus: false,
                toast_log_external_open_pending: false,
                toast_log_external_open_error: None,
                toast_log_viewer: ToastLogViewerState::default(),
                toast_log_return_focus: None,
                toast_log_return_scroll_offset: None,
                toast_log_focus_handle,
                toast_log_owner_focus_handle,
                toast_log_scroll: ScrollHandle::new(),
                settings_focus_handle,
                settings_owner_focus_handle,
                settings_menu_focus_handle,
                favorites_pin_focus_handle,
                hash_pin_focus_handle,
                json_pin_focus_handle,
                xml_pin_focus_handle,
                command_help_owner_focus_handle,
                command_help_close_focus_handle,
                command_help_scroll: ScrollHandle::new(),
                command_help_return_focus: None,
                command_help_return_scroll_offset: None,
                settings_return_focus: None,
                settings_return_scroll_offset: None,
                native_window: None,
                focus_handle,
                resize_focus_handle,
                root_scroll: ScrollHandle::new(),
                utility_presentations: utility_presentations.clone(),
                utility_size_store: Arc::clone(&utility_sizes_for_root),
                utility_size_polling: Arc::clone(&utility_size_polling_for_root),
                hash_docked_preferred_size: hash_docked_preferred_size_for_root,
                root_body_viewport: None,
                hash_docked_resize: Arc::new(Mutex::new(None)),
                hash_geometry_sequence: Arc::clone(&hash_geometry_sequence_for_root),
                hash_tool_key,
                hash_session: session_for_root,
                activity_center,
                activity_view: None,
                json_view: None,
                xml_view: None,
                regex_view: None,
                clipboard_history: None,
                snippets_view: None,
                tempfile_view: None,
                snippet_fill: None,
                pane_escape_latched: false,
                pane_enter_latched: false,
                snippets_search: snippets::SearchState::default(),
                clipboard_modify_view: None,
                clipboard_modify_startup: clipboard_modify_startup::Owner::default(),
                clipboard_popup: None,
                clipboard_retiring: Vec::new(),
                clipboard_modify_popup: None,
                clipboard_modify_retiring: Vec::new(),
                xml_popup: None,
                regex_popup: None,
                regex_retiring: Vec::new(),
                xml_retiring: Vec::new(),
                xml_terminal_quit: false,
                json_popup: None,
                json_retiring: Vec::new(),
                hash_wake_signal: wake_signal_for_root,
                hash_file_picker_pending: hash_file_picker_pending_for_root,
                hash_clipboard_pending: hash_clipboard_pending_for_root,
                hash_clipboard_operation: None,
                hash_clipboard_operation_counter: 0,
                file_search: None,
                folder_size_view: None,
                file_lock_view: None,
                file_lock_popup: None,
                file_lock_retiring: Vec::new(),
                folder_size_popup: None,
                folder_size_retiring: Vec::new(),
                file_search_popup: None,
                file_search_retiring: Vec::new(),
                #[cfg(windows)]
                downloads_triage: None,
                #[cfg(windows)]
                app_data_root: app_data_root.clone(),
                file_search_settings: file_search_settings.clone(),
                hash_window: HashWindowState::Closed,
                hash_return: None,
                hash_window_revision: window_revision_for_root,
                hash_mount_generation: hash_mount_generation_for_root,
                hash_mount_counter: 0,
                hash_rehost_pending: hash_rehost_pending_for_root,
                hash_rehost_revision: hash_rehost_revision_for_root,
                hash_window_close_subscriptions: Vec::new(),
                hash_retiring_windows: Vec::new(),
                hash_presentation_feedback: hash_presentation_feedback_for_root,
                launcher_search_error: None,
                hash_open_error: None,
                hash_presentation_error: None,
                hash_native_geometry_error: None,
                hash_size_write_error: None,
                secondary: SecondaryWindowState::Closed,
                secondary_revision: Arc::new(AtomicU64::new(0)),
                secondary_close_subscription: None,
                secondary_error: None,
                coordinate_hud_probe_controller: controller_for_root,
                coordinate_hud_probe_closing: closing_for_root,
                coordinate_hud_probe_pending: false,
                coordinate_hud_probe_status: "Coordinate HUD probe is idle.".to_owned(),
                coordinate_shutdown_complete: Arc::clone(&coordinate_shutdown_complete_for_root),
                restart_context: restart_context_for_root,
                restart_admission: Arc::clone(&restart_admission_for_root),
                restart_retiring: Arc::clone(&restart_retiring_for_root),
                restart_status: None,
                restart_exit: restart_exit_for_root,
                restart_failure: restart_failure_for_root,
            });
            let sheet_subscription = root.update(cx, |_, cx| {
                let root = root.downgrade();
                cx.intercept_keystrokes(move |event, window, cx| {
                    let _ = root.update(cx, |root, cx| {
                        if root.action_sheet_key(event, window, cx) {
                            cx.stop_propagation();
                        }
                    });
                })
            });
            appearance_subscription_sink.borrow_mut().push(sheet_subscription);
            root.update(cx, |root, cx| root.start_application_index(cx));
            #[cfg(windows)]
            {
                let keypad_subscription = root.update(cx, |_, cx| {
                    let root = root.downgrade();
                    cx.intercept_keystrokes(move |event, window, cx| {
                        let receipt = gpui_windows::take_current_native_keydown_receipt()
                            .map(PhysicalKeydown::from);
                        let Some(receipt) = receipt else {
                            return;
                        };
                        if receipt.virtual_key == 0x1b {
                            let suppress = root.update(cx, |root, _| {
                                if root.root_window.is_none_or(|host| host.window_id() != window.window_handle().window_id())
                                    || root.native_window.is_none_or(|native| native.identity.hwnd != receipt.hwnd) { return false; }
                                if !receipt.held { root.pane_escape_latched = false; }
                                root.pane_escape_latched && receipt.held
                            }).unwrap_or(false);
                            if suppress { window.prevent_default(); cx.stop_propagation(); return; }
                        }
                        if receipt.virtual_key == 0x0d {
                            let suppress = root.update(cx, |root, cx| {
                                if root.root_window.is_none_or(|host| host.window_id() != window.window_handle().window_id())
                                    || root.native_window.is_none_or(|native| native.identity.hwnd != receipt.hwnd
                                        || !native_window_is_valid(native) || current_foreground_identity() != Some(native.identity))
                                    || !root.root_visibility.active.load(Ordering::Acquire)
                                    || !root.root_visibility.visible.load(Ordering::Acquire) { return false; }
                                if !receipt.held {
                                    root.pane_enter_latched = false;
                                    if let Some(view) = root.settings_navigation.clone() { view.update(cx, |view, _| view.release_enter()); }
                                }
                                root.pane_enter_latched && receipt.held
                            }).unwrap_or(false);
                            if suppress { window.prevent_default(); cx.stop_propagation(); return; }
                        }
                        if !receipt.held || receipt.virtual_key == 0x09 {
                            let _ = root.update(cx, |root, _| {
                                if root.native_window.is_some_and(|native| native.identity.hwnd == receipt.hwnd)
                                    && root.root_window.is_some_and(|host| host.window_id() == window.window_handle().window_id()) {
                                    root.external_launch.retire_focus_bound_effects();
                                }
                            });
                        }
                        if scoped_jump::dispatch(&root, &event.context_stack, receipt, window, cx) {
                            window.prevent_default();
                            cx.stop_propagation();
                            return;
                        }
                        if intercept_root_pane_escape(&root, receipt, event, window, cx) {
                            cx.stop_propagation();
                            return;
                        }
                        let is_hash = context_stack_has_primary(&event.context_stack, "HashGeneratorWindow");
                        if is_hash || context_stack_has_primary(&event.context_stack, "List") {
                            let owned = root.update(cx, |root, cx| {
                                let (view, native, event_window_matches, visible) = match &root.hash_window {
                                    HashWindowState::Docked { view, .. } => (view.clone(), root.native_window,
                                        root.root_window.is_some_and(|root_window| root_window.window_id() == window.window_handle().window_id()),
                                        root.root_visibility.active.load(Ordering::Acquire) && root.root_visibility.visible.load(Ordering::Acquire)),
                                    HashWindowState::Popout { view, window: hash_window, .. } => (view.clone(), view.read(cx).native_window,
                                        hash_window.window_id() == window.window_handle().window_id(), true),
                                    HashWindowState::Closed => return false,
                                };
                                let Some(native) = native.filter(|native| native_window_is_valid(*native)) else { return false; };
                                if !visible || !event_window_matches || receipt.hwnd != native.identity.hwnd
                                    || current_foreground_identity() != Some(native.identity)
                                    || (!is_hash && view.read(cx).hash_popup_scope(window, cx).is_none()) {
                                    return false;
                                }
                                view.update(cx, |view, cx| {
                                    if view.handle_jump_keydown(receipt, window, cx) { cx.stop_propagation(); }
                                });
                                true
                            }).unwrap_or(false);
                            if is_hash || owned {
                                return;
                            }
                        }
                        if !context_stack_has_primary(&event.context_stack, "LauncherSearch") { return; }
                        let _ = root.update(cx, |root, cx| {
                            let Some(native_root) = root
                                .native_window
                                .filter(|native_root| native_window_is_valid(*native_root))
                            else {
                                return;
                            };
                            let event_window_is_root = root.root_window.is_some_and(|root_handle| {
                                root_handle.window_id() == window.window_handle().window_id()
                            });
                            if !event_window_is_root
                                || receipt.hwnd != native_root.identity.hwnd
                                || current_foreground_identity() != Some(native_root.identity)
                            {
                                return;
                            }
                            if root.handle_search_jump_keydown(receipt, window, cx) {
                                cx.stop_propagation();
                                return;
                            }
                            if physical_keypad_receipt_direction(receipt).is_none()
                                || event.keystroke.modifiers != Modifiers::default()
                            {
                                return;
                            }
                            let Some(movement) = physical_keypad_selection_move(
                                Some(receipt),
                                event.keystroke.modifiers,
                                &event.context_stack,
                                Some(native_root.identity),
                                current_foreground_identity(),
                                event_window_is_root,
                            ) else {
                                return;
                            };
                            if root.move_search_keypad_selection(movement, window, cx) {
                                root.fresh_search_enter = None;
                                cx.stop_propagation();
                            }
                        });
                    })
                });
                appearance_subscription_sink
                    .borrow_mut()
                    .push(keypad_subscription);
            }
            let search_subscription = root.update(cx, |root, cx| {
                let query = root.query.clone();
                cx.subscribe(
                    &query,
                    |root: &mut RootWindow,
                     query: Entity<InputState>,
                     event: &InputEvent,
                     cx: &mut Context<RootWindow>| {
                        match event {
                            InputEvent::Change => {
                                let query_text = query.read(cx).value().to_string();
                                root.launcher_search_error = None;
                                root.query_history.synchronize(&query_text);
                                root.search_completion
                                    .apply_change(&root.search_catalog, &query_text);
                                root.refresh_launcher_results(query_text, cx);
                                cx.notify();
                            }
                            InputEvent::PressEnter { secondary, shift } => {
                                let marker = root.fresh_search_enter.take();
                                if *secondary || *shift {
                                    return;
                                }
                                let query_text = query.read(cx).value().to_string();
                                let Some(marker) = marker.filter(|marker| {
                                    marker.generation == root.launcher_results.generation
                                        && marker.query == query_text
                                        && marker.jump_mode_revision
                                            == root.search_jump_mode.revision()
                                }) else {
                                    return;
                                };
                                let Some((generation, index)) =
                                    root.launcher_results.enter_target(&query_text)
                                else {
                                    return;
                                };
                                if generation != marker.generation {
                                    return;
                                }
                                let Some(root_window) = root.root_window else {
                                    return;
                                };
                                let jump_mode_revision = marker.jump_mode_revision;
                                let root_view = cx.entity();
                                cx.spawn(async move |_, cx| {
                                    let _ = cx.update(|app| {
                                        let _ = update_kit_window_content(
                                            root_window,
                                            &root_view,
                                            app,
                                            |root, window, cx| {
                                                let Some(_search_operation) =
                                                    root.search_input_operation(window, cx)
                                                else {
                                                    return;
                                                };
                                                let query_is_current = root
                                                    .query
                                                    .read(cx)
                                                    .value()
                                                    .as_ref()
                                                    == query_text;
                                                if query_is_current
                                                    && root.launcher_results.generation
                                                        == generation
                                                    && root.search_jump_mode.revision()
                                                        == jump_mode_revision
                                                {
                                                    root.activate_launcher_result(
                                                        generation,
                                                        index,
                                                        ActivationSource::Enter,
                                                        LauncherResultActivationOrigin::DeferredSearchEnter,
                                                        window,
                                                        cx,
                                                    );
                                                }
                                            },
                                        );
                                    });
                                })
                                .detach();
                            }
                            InputEvent::Blur => {
                                root.fresh_search_enter = None;
                                if root.retire_search_jump_mode() {
                                    cx.notify();
                                }
                                root.revoke_search_completion_focus();
                            }
                            _ => {}
                        }
                    },
                )
            });
            appearance_subscription_sink
                .borrow_mut()
                .push(search_subscription);
            let command_help_filter_subscription = root.update(cx, |root, cx| {
                let filter = root.command_help_filter.clone();
                cx.subscribe(
                    &filter,
                    |root: &mut RootWindow,
                     filter: Entity<InputState>,
                     event: &InputEvent,
                     cx: &mut Context<RootWindow>| {
                        if matches!(event, InputEvent::Change) {
                            root.pending_help_filter_sync = Some((
                                HelpFilterPeer::CommandHelp,
                                filter.read(cx).value().to_string(),
                            ));
                            cx.notify();
                        }
                    },
                )
            });
            appearance_subscription_sink
                .borrow_mut()
                .push(command_help_filter_subscription);
            let quick_help_filter_subscription = root.update(cx, |root, cx| {
                let filter = root.quick_help_filter.clone();
                cx.subscribe(
                    &filter,
                    |root: &mut RootWindow,
                     filter: Entity<InputState>,
                     event: &InputEvent,
                     cx: &mut Context<RootWindow>| {
                        if matches!(event, InputEvent::Change) {
                            root.pending_help_filter_sync = Some((
                                HelpFilterPeer::QuickHelp,
                                filter.read(cx).value().to_string(),
                            ));
                            cx.notify();
                        }
                    },
                )
            });
            appearance_subscription_sink
                .borrow_mut()
                .push(quick_help_filter_subscription);
            let tab_completion_subscription = root.update(cx, |_, cx| {
                cx.observe_keystrokes(|root, event, window, cx| {
                    if root_tab_traversed_to_search(
                        &event.keystroke.key,
                        event.keystroke.modifiers,
                        event.action.is_some(),
                        &event.context_stack,
                        root.search_input_is_live(window, cx),
                          root.file_menu_open || root.settings_menu_open || root.help_menu_open
                              || root.context_favorite_popup.is_some()
                              || root.action_sheet.is_some()
                              || root.result_favorites.owns_focus(window, cx),
                    ) {
                        root.accept_search_completion(false, window, cx);
                    }
                })
            });
            appearance_subscription_sink
                .borrow_mut()
                .push(tab_completion_subscription);
            let activation_subscription = root.update(cx, |_, cx| {
                cx.observe_window_activation(window, |root, window, cx| {
                    if window.is_window_active() {
                        if let Some(view) = root.clipboard_modify_view.clone().filter(|view| view.read(cx).is_mounted_in(window.window_handle().window_id(), false)) {
                            let receipt = view.read(cx).mount_receipt();
                            let weak = view.downgrade();
                            window.defer(cx, move |window, app| { let _ = weak.update(app, |view, cx| {
                                if receipt.is_current() && receipt.matches(&view.mount_receipt()) { view.wake_pending_read(window, cx); }
                            }); });
                        }
                    } else {
                        root.external_launch.retire_focus_bound_effects();
                        if let Some(view) = root.json_view.clone() {
                            if !view.read(cx).popout { view.update(cx, |view, cx| view.retire_jump(cx)); }
                        }
                        if let Some(view) = root.regex_view.clone() {
                            if !view.read(cx).popout { view.update(cx, |view, cx| view.retire_jump(cx)); }
                        }
                        if let Some(view) = root.xml_view.clone() {
                            if !view.read(cx).popout { view.update(cx, |view, cx| view.retire_jump(cx)); }
                        }
                        root.retire_action_sheet_now(window, cx, false);
                        root.retire_context_favorite();
                        root.retire_search_jump_mode();
                        root.revoke_search_completion_focus();
                        cx.notify();
                    }
                })
            });
            appearance_subscription_sink
                .borrow_mut()
                .push(activation_subscription);
            let subscription = root.update(cx, |root, cx| {
                let resize_focus_handle = root.resize_focus_handle.clone();
                cx.on_blur(&resize_focus_handle, window, |root, _, cx| {
                    if root.cancel_docked_hash_preview() {
                        cx.notify();
                    }
                })
            });
            appearance_subscription_sink
                .borrow_mut()
                .push(subscription);
            let root_for_window_close = root.downgrade();
            window.on_window_should_close(cx, move |_, app| {
                let clipboard_message =
                    hash_clipboard_close_message(clipboard_pending_for_root_close.kind());
                let picker_pending = picker_pending_for_root_close.load(Ordering::Acquire);
                let Ok(blocker) = root_for_window_close.update(app, |root, cx| {
                    root.retire_context_favorite();
                    root.current_root_close_blocker(clipboard_message, picker_pending, cx)
                }) else {
                    return false;
                };
                match blocker {
                    None => {
                        let deferred = root_for_window_close.update(app, |root, cx| {
                            if root.xml_view.is_some() || root.file_lock_popup.is_some() || !root.file_lock_retiring.is_empty() || root.folder_size_popup.is_some() || !root.folder_size_retiring.is_empty() || root.file_search_popup.is_some() || !root.file_search_retiring.is_empty() || root.regex_popup.is_some() || !root.regex_retiring.is_empty() || root.clipboard_popup.is_some() || !root.clipboard_retiring.is_empty() || root.clipboard_modify_popup.is_some() || !root.clipboard_modify_retiring.is_empty() { root.quit_after_xml_drain(cx); true } else { false }
                        }).unwrap_or(true);
                        if deferred { return false; }
                        let _ = root_for_window_close.update(app, |root, cx| {
                            if let Some(view) = &root.file_search { view.read(cx).retire_explorer(); }
                            root.retire_json_workspace(cx);
                            if let Some(view) = &root.regex_view { view.update(cx, |view, _| view.shutdown()); }
                            root.window_search.retire(); root.file_search_popup_intent.store(false, Ordering::Release); root.file_search_probe.retire();
                            root.indexed_applications.retire();
                        });
                        return true;
                    }
                    Some(RootCloseBlocker::CalculatorSearch) => {
                        let _ = root_for_window_close.update(app, |root, cx| {
                            root.restart_status = Some(calculator_search::SEARCH_PENDING_STATUS.to_owned());
                            cx.notify();
                        });
                        false
                    }
                    Some(RootCloseBlocker::PluginPreferenceSave) => {
                        let _ = root_for_window_close.update(app, |root, cx| {
                            root.restart_status = Some("Wait for the plugin Search capability save.".to_owned());
                            cx.notify();
                        });
                        false
                    }
                    Some(RootCloseBlocker::BooleanPreferenceSave) => {
                        let close_now = root_for_window_close
                            .update(app, |root, cx| {
                                root.prepare_boolean_preference_close(cx)
                            })
                            .unwrap_or(false);
                        if close_now {
                            let deferred = root_for_window_close.update(app, |root, cx| {
                                if root.xml_view.is_some() || root.file_lock_popup.is_some() || !root.file_lock_retiring.is_empty() || root.folder_size_popup.is_some() || !root.folder_size_retiring.is_empty() || root.file_search_popup.is_some() || !root.file_search_retiring.is_empty() || root.regex_popup.is_some() || !root.regex_retiring.is_empty() || root.clipboard_popup.is_some() || !root.clipboard_retiring.is_empty() || root.clipboard_modify_popup.is_some() || !root.clipboard_modify_retiring.is_empty() { root.quit_after_xml_drain(cx); true } else { false }
                            }).unwrap_or(true);
                            if deferred { return false; }
                            let _ = root_for_window_close.update(app, |root, cx| {
                            if let Some(view) = &root.file_search { view.read(cx).retire_explorer(); }
                            root.retire_json_workspace(cx);
                            if let Some(view) = &root.regex_view { view.update(cx, |view, _| view.shutdown()); }
                            root.window_search.retire(); root.file_search_popup_intent.store(false, Ordering::Release); root.file_search_probe.retire();
                            root.indexed_applications.retire();
                        });
                        }
                        return close_now;
                    }
                    Some(RootCloseBlocker::HashClipboard(message)) => {
                        let _ = root_for_window_close.update(app, |root, cx| {
                            root.restart_status = Some(message.to_owned());
                            cx.notify();
                        });
                        false
                    }
                    Some(RootCloseBlocker::HashFilePicker) => {
                        let _ = root_for_window_close.update(app, |root, cx| {
                            root.restart_status = Some(
                                "Finish or cancel the Hash Generator file picker before closing Multi Launcher."
                                    .to_owned(),
                            );
                            cx.notify();
                        });
                        false
                    }
                    Some(RootCloseBlocker::XmlFilePicker) => {
                        let _ = root_for_window_close.update(app, |root, cx| {
                            root.restart_status = Some("Finish the XML file operation or overwrite review, or wait for its window to finish docking, before closing Multi Launcher.".to_owned()); cx.notify();
                        });
                        false
                    }
                    Some(RootCloseBlocker::ToastLogOpen) => {
                        let _ = root_for_window_close.update(app, |root, cx| {
                            root.restart_status =
                                Some(TOAST_LOG_OPEN_PENDING_STATUS.to_owned());
                            cx.notify();
                        });
                        false
                    }
                    Some(RootCloseBlocker::AppWrite) => {
                        let _ = root_for_window_close.update(app, |root, cx| {
                            root.restart_status = Some(add_action::APP_WRITE_CLOSE_STATUS.to_owned());
                            cx.notify();
                        });
                        false
                    }
                    Some(RootCloseBlocker::ExternalLaunch) => {
                        let _ = root_for_window_close.update(app, |root, cx| {
                            root.restart_status = Some(external_launch::CLOSE_STATUS.to_owned());
                            cx.notify();
                        });
                        false
                    }
                    Some(RootCloseBlocker::FavoritesWrite) => {
                        let _ = root_for_window_close.update(app, |root, cx| {
                            root.restart_status = Some(FAVORITES_WRITE_PENDING.to_owned());
                            cx.notify();
                        });
                        false
                    }
                    Some(RootCloseBlocker::RegexHistory) => { let _ = root_for_window_close.update(app, |root, cx| { root.restart_status = Some(regex_history::WAIT_CLOSE.to_owned()); cx.notify(); }); false }
                    Some(RootCloseBlocker::ClipboardModifyBlocked) => { let _ = root_for_window_close.update(app, |root, cx| { root.restart_status = Some(clipboard_modify_view::BLOCK_WAIT.to_owned()); cx.notify(); }); false }
                    Some(RootCloseBlocker::SettingsActionsSave) => { let _ = root_for_window_close.update(app, |root, cx| { root.restart_status = Some(settings_actions::WAIT.to_owned()); cx.notify(); }); false }
                    Some(RootCloseBlocker::FileSearchEffect) => { let _ = root_for_window_close.update(app, |root, cx| { root.restart_status = Some(file_search_preview::WAIT.to_owned()); cx.notify(); }); false }
                    Some(RootCloseBlocker::FolderSize) => { let _ = root_for_window_close.update(app, |root, cx| { root.restart_status = Some(folder_size::WAIT.to_owned()); cx.notify(); }); false }
                    Some(RootCloseBlocker::FileLock) => { let _ = root_for_window_close.update(app, |root, cx| { root.restart_status = Some(file_lock::WAIT.to_owned()); cx.notify(); }); false }
                    Some(RootCloseBlocker::TempfileSave) => { let _ = root_for_window_close.update(app, |root, cx| { root.restart_status = Some(tempfile_view::WAIT.to_owned()); cx.notify(); }); false }
                    Some(RootCloseBlocker::SnippetsSave) => { let _ = root_for_window_close.update(app, |root, cx| { root.restart_status = Some(snippets::SAVE_WAIT.to_owned()); cx.notify(); }); false }
                    Some(RootCloseBlocker::ClipboardHistory) => { let _ = root_for_window_close.update(app, |root, cx| { root.restart_status = Some(clipboard_history::WAIT.to_owned()); cx.notify(); }); false }
                    Some(RootCloseBlocker::RegexPresets) => { let _ = root_for_window_close.update(app, |root, cx| { root.restart_status = Some(regex_presets::WAIT_CLOSE.to_owned()); cx.notify(); }); false }
                    Some(RootCloseBlocker::RegexClipboard) => { let _ = root_for_window_close.update(app, |root, cx| { root.restart_status = Some("Wait for the Regex Tester clipboard operation before closing.".to_owned()); cx.notify(); }); false }
                    Some(RootCloseBlocker::RegexRehost) => { let _ = root_for_window_close.update(app, |root, cx| { root.restart_status = Some("Wait for Regex Tester to finish changing windows before closing.".to_owned()); cx.notify(); }); false }
                    Some(RootCloseBlocker::StructuredPinWrite) => {
                        let _ = root_for_window_close.update(app, |root, cx| { root.restart_status = Some("Wait for the utility pin update to finish before closing Multi Launcher.".to_owned()); cx.notify(); });
                        false
                    }
                    Some(RootCloseBlocker::HashRehost) => { let _ = root_for_window_close.update(app, |root, cx| { root.restart_status = Some("Wait for the Hash Generator candidate window to close.".into()); cx.notify(); }); false }
                    Some(RootCloseBlocker::HashPinWrite) => {
                        let _ = root_for_window_close.update(app, |root, cx| {
                            root.restart_status = Some(hash_pin::UNPIN_PENDING.to_owned());
                            cx.notify();
                        });
                        false
                    }
                }
            });
            root
        }) {
            Ok((window, view)) => (window, view),
            Err(error) => {
                *startup_error_sink.borrow_mut() = Some(error.to_string());
                cx.quit();
                return;
            }
        };
        *root_for_quit.borrow_mut() = Some(root_view.downgrade());

        let native_window =
            match update_kit_window_content(root_window, &root_view, cx, |root, window, cx| {
                let native_window = checked_native_window(window)?;
                root.native_window = native_window;
                root.root_window = Some(root_window);
                root.query.focus_handle(cx).focus(window, cx);
                if !cfg!(windows) || native_window.is_some() {
                    for panel in startup_pin_order {
                        match panel {
                            Panel::FavDialog => root.mount_pinned_favorites(window, cx),
                            Panel::JsonUtilityDialog if root.structured_pins.saved(Panel::JsonUtilityDialog) => root.mount_pinned_json(window, cx),
                            Panel::XmlToolkitDialog if root.structured_pins.saved(Panel::XmlToolkitDialog) => root.mount_pinned_xml(window, cx),
                            Panel::HashGeneratorDialog if root.hash_pin.is_saved() => {
                                if let Some(operation) = root.restart_admission.admit() {
                                    if let Err(error) = root.open_hash_generator_with_mount_intent(
                                        HashOpenIntent::Default, operation, window,
                                        hash_pin::HashMountIntent::Passive, cx,
                                    ) {
                                        root.hash_open_error = Some(error);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Ok::<_, String>(native_window)
            }) {
                Ok(Ok(native_window)) if !cfg!(windows) || native_window.is_some() => native_window,
                Ok(Ok(_)) => {
                    *startup_error_sink.borrow_mut() = Some(
                        "GPUI did not expose a checked Windows window handle for the launcher."
                            .to_owned(),
                    );
                    cx.quit();
                    return;
                }
                Ok(Err(error)) => {
                    *startup_error_sink.borrow_mut() = Some(error);
                    cx.quit();
                    return;
                }
                Err(error) => {
                    *startup_error_sink.borrow_mut() = Some(format!(
                        "GPUI could not initialize its root window: {error}"
                    ));
                    cx.quit();
                    return;
                }
            };

        let root_id = root_window.window_id();
        let root_for_window_close = root_view.downgrade();
        let hash_window_revision_for_root_close = Arc::clone(&hash_window_revision_for_app);
        let hash_mount_generation_for_root_close = Arc::clone(&hash_mount_generation);
        let hash_rehost_pending_for_root_close = Arc::clone(&hash_rehost_pending);
        let hash_rehost_revision_for_root_close = Arc::clone(&hash_rehost_revision);
        subscription_sink
            .borrow_mut()
            .push(cx.on_window_closed(move |cx, closed| {
                if closed == root_id {
                    let _ = root_for_window_close.update(cx, |root, cx| {
                        root.drain_hash_native_geometry(cx, true);
                    });
                    hash_wake_signal_for_root_close.close();
                    hash_window_revision_for_root_close.fetch_add(1, Ordering::AcqRel);
                    hash_mount_generation_for_root_close.store(0, Ordering::Release);
                    hash_rehost_pending_for_root_close.store(false, Ordering::Release);
                    hash_rehost_revision_for_root_close.fetch_add(1, Ordering::AcqRel);
                    if !restart_retiring_for_root_close.load(Ordering::Acquire) {
                        cx.quit();
                    }
                }
            }));

        let hash_root_view = root_view.downgrade();
        cx.spawn(async move |cx| {
            while hash_wake_signal_for_task.wait().await {
                let _ = cx.update(|cx| {
                    let live_window = hash_root_view.update(cx, |root, cx| {
                        root.drain_hash_native_geometry(cx, false);
                        root.drain_json_native_geometry(cx, false);
                        root.drain_xml_native_geometry(cx, false);
                        root.drain_regex_native_geometry(cx, false);
                        root.drain_file_search_geometry(cx, false);
                        root.drain_folder_size_geometry(cx, false);
                        root.drain_file_lock_geometry(cx, false);
                        root.drain_file_preview_geometry(cx, false);
                        root.drain_clipboard_native_geometry(cx, false);
                        root.drain_clipboard_modify_geometry(cx, false);
                        root.hash_session
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .poll();
                        match &root.hash_window {
                            HashWindowState::Closed => None,
                            HashWindowState::Docked { view, .. } => Some((None, view.clone())),
                            HashWindowState::Popout { window, view, .. } => {
                                Some((Some(*window), view.clone()))
                            }
                        }
                    });
                    if let Ok(Some((window, view))) = live_window {
                        if let Some(window) = window {
                            let _ = update_kit_window_content(window, &view, cx, |_, _, cx| {
                                cx.notify();
                            });
                        } else {
                            view.update(cx, |_, cx| cx.notify());
                        }
                    }
                });
            }
        })
        .detach();

        #[cfg(windows)]
        {
            let Some(native_window) = native_window else {
                *startup_error_sink.borrow_mut() = Some(
                    "GPUI did not provide a Windows window handle for the launcher.".to_owned(),
                );
                cx.quit();
                return;
            };
            let (event_tx, event_rx) = channel();
            visibility.bind_request_sender(event_tx.clone());
            let trigger = Arc::new(HotkeyTrigger::new(hotkey));
            let quick_help_visibility = visibility.clone();
            let quick_help_admission = Arc::clone(&restart_admission_for_quick_help_capture);
            let quick_help_trigger = quick_help_shortcut_for_run.map(|shortcut| {
                Arc::new(HotkeyTrigger::new_scoped(shortcut.hotkey, move || {
                    if quick_help_admission.is_fenced() {
                        None
                    } else {
                        quick_help_visibility.current_visible_revision(native_window)
                    }
                }))
            });
            let mut triggers = vec![Arc::clone(&trigger)];
            triggers.extend(quick_help_trigger.iter().map(Arc::clone));
            let listener = match HotkeyTrigger::prepare_listener(
                triggers,
                "gpui-frontend",
                event_tx,
            ) {
                Ok(listener) => listener,
                Err(error) => {
                    *startup_error_sink.borrow_mut() = Some(format!(
                        "Could not start the configured launcher hotkey: {error}"
                    ));
                    cx.quit();
                    return;
                }
            };
            if let Ok(mut slot) = listener_slot_for_run.lock() {
                *slot = Some(listener);
            } else {
                *startup_error_sink.borrow_mut() =
                    Some("Could not initialize the GPUI hotkey owner.".to_owned());
                cx.quit();
                return;
            }
            if let Ok(slot) = listener_slot_for_run.lock()
                && let Some(listener) = slot.as_ref()
            {
                listener.activate();
            }
            if !visibility.activate() {
                *startup_error_sink.borrow_mut() =
                    Some("Could not activate the GPUI visibility owner.".to_owned());
                cx.quit();
                return;
            }
            if let Ok(mut origin) = visibility.snippet_origin.lock() {
                *origin = if current_foreground_identity() == Some(native_window.identity) {
                    initial_snippet_origin.map(|origin| (visibility.request_revision.load(Ordering::Acquire), origin))
                } else { None };
            }
            root_view.update(cx, |root, cx| { root.start_clipboard_modify_catalog(cx); root.start_snippets_search(cx); });

            let root_window = root_window;
            let root_view = root_view;
            let visibility = visibility.clone();
            let dock_mount_generation = Arc::clone(&hash_mount_generation_for_dock);
            let startup_revision = visibility.request_revision.load(Ordering::Acquire);
            cx.spawn(async move |cx| {
                let mut event_rx: Receiver<()> = event_rx;
                let mut return_target = initial_return_target;
                if let Err(error) = root_placement::apply_placement(root_placement, native_window, root_placement::PlacementEdge::Startup, || {
                    visibility.is_current(startup_revision, true) && !restart_admission_for_hotkey.is_fenced()
                }) {
                    tracing::warn!(%error, "could not apply saved launcher placement");
                }
                loop {
                    let (receiver, event) = cx
                        .background_executor()
                        .spawn(async move {
                            let event = event_rx.recv();
                            (event_rx, event)
                        })
                        .await;
                    event_rx = receiver;
                    if event.is_err() || !visibility.active.load(Ordering::Acquire) {
                        break;
                    }
                    let Some(_operation) = restart_admission_for_hotkey.admit() else {
                        if let Some(trigger) = quick_help_trigger.as_ref() {
                            trigger.clear_pending();
                        }
                        break;
                    };

                    if visibility.window_search_update.swap(false, Ordering::AcqRel) {
                        let _ = root_view.update(&mut *cx, |root, cx| {
                            let query = root.query.read(cx).value().to_string();
                            if root.root_visibility.visible.load(Ordering::Acquire)
                                && window_search::query_is_windows(&query)
                                && window_search::enabled(root.enabled_plugins.as_ref(), root.enabled_capabilities.as_ref())
                                && !root.restart_admission.is_fenced()
                            {
                                if root.external_launch.window_catalog_pending() {
                                    root.window_search.defer_update();
                                } else {
                                    root.refresh_launcher_results_with_viewport(query, false, cx);
                                    cx.notify();
                                }
                            }
                        });
                    }

                    let quick_help_stamp = quick_help_trigger
                        .as_ref()
                        .and_then(|trigger| trigger.take_scoped_stamp());
                    let mut requested_visible = visibility.take_requested_visible();
                    if let Some(Some(revision)) = quick_help_stamp
                        && visibility.is_current_visible_revision(revision, native_window)
                    {
                        // The polling edge captured this exact visible native
                        // root revision. Consume it only while that same window
                        // and revision remain current before applying queued
                        // show requests.
                        let _ = update_kit_window_content(
                            root_window,
                            &root_view,
                            &mut *cx,
                            |root, window, cx| {
                                let root_handle_matches = root
                                    .root_window
                                    .is_some_and(|handle| {
                                        handle.window_id() == root_window.window_id()
                                    });
                                if root_handle_matches
                                    && root.native_window == Some(native_window)
                                    && root
                                        .root_visibility
                                        .is_current_visible_revision(revision, native_window)
                                    && !root.restart_admission.is_fenced()
                                {
                                    let take_focus =
                                        root.quick_help_focus_change_allowed(window, cx);
                                    root.toggle_quick_help_from_shortcut(
                                        window, cx, take_focus,
                                    );
                                }
                            },
                        );
                    }

                    let was_visible = if matches!(requested_visible.as_ref(), Some(RootVisibilityRequest::DockUtility(_, _))) {
                        visibility.current_visible_revision(native_window).is_some()
                    } else { visibility.visible.load(Ordering::Acquire) };
                    let hotkey_for_conditional = requested_visible.as_ref()
                        .is_some_and(|request| request.superseded_by_hotkey(|| trigger.take()));
                    let switch_hide = requested_visible.as_ref().and_then(|request| match request {
                        RootVisibilityRequest::HideAfterWindowSwitch(_, target, query) => Some((*target, Arc::clone(query))),
                        _ => None,
                    });
                    let switch_query_current = switch_hide.as_ref().is_none_or(|(_, expected)| {
                        update_kit_window_content(root_window, &root_view, &mut *cx,
                            |root, _, cx| root.query.read(cx).value().as_str() == expected.as_ref(),
                        ).unwrap_or(false)
                    });
                    let close_current = requested_visible.as_ref().is_none_or(|request| {
                        let RootVisibilityRequest::HideAfterWindowClose(_, root, _, expected) = request else { return true; };
                        let current_query = update_kit_window_content(root_window, &root_view, &mut *cx,
                            |root, _, cx| root.query.read(cx).value().to_string(),
                        ).ok();
                        native_window.identity == *root
                            && window_close_hide_is_current(*root, current_foreground_identity(), expected, current_query.as_deref())
                    });
                    let undo_current = requested_visible.as_ref().is_none_or(|request| {
                        let RootVisibilityRequest::HideAfterClipboardUndo(_, expected_root, generation, expected_query, lease, focus, catalog_current) = request else { return true; };
                        native_window.identity == *expected_root && update_kit_window_content(root_window, &root_view, &mut *cx, |root, window, cx| {
                            root.root_window.is_some_and(|host| host.window_id() == window.window_handle().window_id())
                                && root.native_window == Some(native_window)
                                && !window.has_active_dialog(cx)
                                && root.external_owner_is_current(focus.as_ref(), window, cx)
                                && catalog_current.as_ref().is_none_or(|current| current())
                                && clipboard_undo_hide_is_current(*expected_root, current_foreground_identity(), *generation, root.launcher_results.generation, expected_query, root.query.read(cx).value().as_str(), lease.load(Ordering::Acquire))
                        }).unwrap_or(false)
                    });
                    let dock_reveal = requested_visible.as_ref().and_then(|request| match request {
                        RootVisibilityRequest::DockHash(reveal) => Some((reveal.gesture, Some(reveal.mount_generation), None)),
                        RootVisibilityRequest::DockUtility(kind, reveal) => Some((reveal.gesture, None, Some((reveal.mount.clone(), *kind)))),
                        _ => None,
                    });
                    let utility_destination_current = dock_reveal.as_ref().and_then(|(_, _, utility)| utility.as_ref()).is_none_or(|(receipt, kind)| {
                        update_kit_window_content(root_window, &root_view, &mut *cx, |root, window, app| {
                            root.native_window == Some(native_window)
                                && root.utility_dock_focus_available(window, app)
                                && root.utility_destination_current(*kind, receipt, window.window_handle().window_id(), app)
                        }).unwrap_or(false)
                    });
                    let dock_current = dock_reveal.as_ref().is_none_or(|(gesture, generation, utility_mount_receipt)| {
                        generation.is_none_or(|generation| dock_mount_generation.load(Ordering::Acquire) == generation)
                            && utility_mount_receipt.as_ref().is_none_or(|(mount, _)| mount.is_current())
                            && (utility_mount_receipt.is_none() || (gesture.foreground_authorized && current_foreground_identity() == Some(gesture.source.identity)))
                            && native_window_is_valid(gesture.source)
                            && !restart_admission_for_hotkey.is_fenced()
                    });
                    let accepted_visible = if hotkey_for_conditional || !switch_query_current || !close_current || !undo_current || !dock_current || !utility_destination_current {
                        None
                    } else {
                        requested_visible.as_ref().and_then(|request| visibility.accept_requested_visible(request))
                    };
                    let dock_reveal = accepted_visible.is_some().then_some(dock_reveal).flatten();
                    let switch_hide = accepted_visible.is_some().then_some(switch_hide).flatten();
                    let close_hide = if accepted_visible.is_some() {
                        requested_visible.as_ref().and_then(|request| match request {
                            RootVisibilityRequest::HideAfterWindowClose(_, root, target, query) => Some((*root, *target, Arc::clone(query))),
                            _ => None,
                        })
                    } else { None };
                    let undo_hide = if accepted_visible.is_some() {
                        requested_visible.as_ref().and_then(|request| match request {
                            RootVisibilityRequest::HideAfterClipboardUndo(_, root, generation, query, lease, focus, catalog_current) => Some((*root, *generation, Arc::clone(query), Arc::clone(lease), focus.clone(), catalog_current.clone())),
                            _ => None,
                        })
                    } else { None };
                    let (revision, desired_visible, hash_focus_generation, hotkey_toggle) =
                        if let Some((revision, visible)) = accepted_visible {
                            let hash_focus_generation = dock_reveal.as_ref().and_then(|(_, generation, _)| *generation);
                            (revision, visible, hash_focus_generation, false)
                        } else if hotkey_for_conditional || trigger.take() {
                            let (revision, visible) = visibility.next_toggle();
                            (revision, visible, None, true)
                        } else {
                            continue;
                        };
                    let (foreground, snippet_origin) = if let Some((gesture, _, _)) = &dock_reveal {
                        (gesture.foreground_authorized.then_some(gesture.source.identity), None)
                    } else if desired_visible {
                        cx.background_executor().spawn(async {
                            let identity = capture_foreground_identity();
                            (identity, capture_snippet_origin(identity))
                        }).await
                    } else {
                        (None, None)
                    };
                    if !visibility.is_current(revision, desired_visible) {
                        continue;
                    }
                    let checked_hotkey_reveal = hotkey_toggle && desired_visible && !was_visible;
                    if checked_hotkey_reveal && hotkey_edge_pending(&trigger) { continue; }
                    if checked_hotkey_reveal {
                        if let Some(expected) = foreground.filter(|identity| *identity != native_window.identity) {
                            let worker_visibility = visibility.clone();
                            let worker_trigger = trigger.clone();
                            let worker_admission = restart_admission_for_hotkey.clone();
                            let request = multi_launcher::window_activation::CheckedRootDesktop {
                                identity: multi_launcher::window_catalog::WindowNativeIdentity {
                                    hwnd: native_window.identity.hwnd,
                                    pid: native_window.identity.process_id,
                                    tid: native_window.identity.thread_id,
                                },
                                is_current: Arc::new(move || {
                                    reveal_edge_is_current(worker_visibility.is_current(revision, true), hotkey_edge_pending(&worker_trigger))
                                        && !worker_admission.is_fenced() && native_window_is_valid(native_window)
                                        && native_window_is_valid(NativeWindow { identity: expected })
                                        && current_foreground_identity() == Some(expected)
                                }),
                            };
                            let result = cx.background_executor().spawn(async move {
                                multi_launcher::window_activation::prepare_checked_root_desktop(request)
                            }).await;
                            if let Err(error) = result {
                                // Keep the pending-edge observation and rollback ordered against
                                // listener publication. A second press remains queued for its own toggle.
                                let reverted = rollback_reveal_unless_hotkey_pending(&visibility, revision, &trigger);
                                if reverted {
                                    let _ = root_view.update(&mut *cx, |root, cx| {
                                        root.launcher_search_error = Some(format!("Could not show launcher on the current desktop: {error}"));
                                        cx.notify();
                                    });
                                }
                                continue;
                            }
                        }
                    }
                    if checked_hotkey_reveal && hotkey_edge_pending(&trigger) { continue; }
                    if root_placement::should_place_reveal(was_visible, desired_visible, visibility.is_current(revision, desired_visible)) {
                        if let Err(error) = root_placement::apply_placement(root_placement, native_window, root_placement::PlacementEdge::Reveal, || {
                            visibility.is_current(revision, true) && !restart_admission_for_hotkey.is_fenced()
                                && (!checked_hotkey_reveal || (!hotkey_edge_pending(&trigger)
                                    && foreground.is_none_or(|expected| current_foreground_identity() == Some(expected)
                                        && native_window_is_valid(NativeWindow { identity: expected }))))
                        }) {
                            tracing::warn!(%error, "could not apply saved launcher placement");
                        }
                        if !visibility.is_current(revision, true) || !native_window_is_valid(native_window) || restart_admission_for_hotkey.is_fenced() {
                            continue;
                        }
                    }
                    let reveal_current = || !checked_hotkey_reveal || (
                        reveal_edge_is_current(visibility.is_current(revision, true), hotkey_edge_pending(&trigger))
                            && !restart_admission_for_hotkey.is_fenced() && native_window_is_valid(native_window)
                            && foreground.is_none_or(|expected| current_foreground_identity() == Some(expected)
                                && native_window_is_valid(NativeWindow { identity: expected }))
                    );
                    if !reveal_current() {
                        rollback_reveal_unless_hotkey_pending(&visibility, revision, &trigger);
                        continue;
                    }
                    let switch_hide_accepted = switch_hide.is_some();
                    let guarded_hide = switch_hide_accepted || close_hide.is_some() || undo_hide.is_some();
                    let query_window = update_kit_window_content(
                        root_window,
                        &root_view,
                        &mut *cx,
                        |root, window, cx| {
                            if !reveal_current() { return (root.native_window, false, false); }
                            if !guarded_hide {
                                root.revoke_search_completion_focus();
                                if !desired_visible {
                                    root.retire_action_sheet_now(window, cx, true);
                                    root.retire_context_favorite();
                                    root.retire_search_jump_mode();
                                }
                            }
                            let mut search_focus_selected = false;
                            let mut utility_focus_delivered = false;
                            if desired_visible {
                                root.remount_pinned_hash_if_ready(window, cx);
                                if let Some(view) = root.activity_view.clone().filter(|view| view.read(cx).open) {
                                    view.update(cx, |view, cx| view.resume(cx));
                                }
                                let query = root.query.read(cx).value().to_string();
                                if (window_search::query_is_windows(&query)
                                    && window_search::enabled(root.enabled_plugins.as_ref(), root.enabled_capabilities.as_ref()))
                                    || (file_search::typed_query(&query)
                                        && file_search::enabled(root.enabled_plugins.as_ref(), root.enabled_capabilities.as_ref()))
                                {
                                    root.refresh_launcher_results_with_viewport(query, false, cx);
                                }
                            }
                            let utility_requested = dock_reveal.as_ref().is_some_and(|(_, _, utility)| utility.is_some());
                            let utility_focus_available = !utility_requested || root.utility_dock_focus_available(window, cx);
                            if desired_visible && root.action_sheet.is_none() && utility_focus_available
                                && visibility.is_current(revision, true)
                                && root.native_window == Some(native_window)
                                && !root.restart_admission.is_fenced()
                            {
                                let docked_mount_generation = match &root.hash_window {
                                    HashWindowState::Docked {
                                        mount_generation, ..
                                    } => Some(*mount_generation),
                                    _ => None,
                                };
                                let hash_view = if docked_hash_focus_is_current(
                                    hash_focus_generation,
                                    root.hash_mount_generation.load(Ordering::Acquire),
                                    docked_mount_generation,
                                ) {
                                    root.hash_window.view().cloned()
                                } else {
                                    None
                                };
                                let utility = dock_reveal.as_ref().and_then(|(_, _, mount)| mount.as_ref()).filter(|(mount, kind)| root.utility_destination_current(*kind, mount, window.window_handle().window_id(), cx));
                                let file_lock_view = utility.filter(|(_, kind)| *kind == utility_mount::UtilityKind::FileLock).and_then(|_| root.file_lock_view.as_ref()).cloned();
                                let folder_size_view = utility.filter(|(_, kind)| *kind == utility_mount::UtilityKind::FolderSize).and_then(|_| root.folder_size_view.as_ref()).cloned();
                                let file_search_view = utility.filter(|(_, kind)| *kind == utility_mount::UtilityKind::FileSearch).and_then(|_| root.file_search.as_ref()).cloned();
                                let json_view = utility.filter(|(_, kind)| *kind == utility_mount::UtilityKind::Json).and_then(|_| root.json_view.as_ref()).cloned();
                                let xml_view = utility.filter(|(_, kind)| *kind == utility_mount::UtilityKind::Xml).and_then(|_| root.xml_view.as_ref()).cloned();
                                let regex_view = utility.filter(|(_, kind)| *kind == utility_mount::UtilityKind::Regex).and_then(|_| root.regex_view.as_ref()).cloned();
                                let clipboard_view = utility.filter(|(_, kind)| *kind == utility_mount::UtilityKind::ClipboardHistory).and_then(|_| root.clipboard_history.as_ref()).cloned();
                                let clipboard_modify_view = utility.filter(|(_, kind)| *kind == utility_mount::UtilityKind::ClipboardModify).and_then(|_| root.clipboard_modify_view.as_ref()).cloned();
                                search_focus_selected = file_lock_view.is_none() && folder_size_view.is_none() && file_search_view.is_none() && hash_view.is_none() && json_view.is_none() && xml_view.is_none() && regex_view.is_none() && clipboard_view.is_none() && clipboard_modify_view.is_none();
                                if let Some(view) = file_lock_view {
                                    root.reveal_file_lock(window, cx);
                                    utility_focus_delivered = view.update(cx, |tool, cx| { tool.focus_host(window, cx); tool.choose_focus.is_focused(window) });
                                } else if let Some(view) = folder_size_view {
                                    root.reveal_folder_size(window, cx);
                                    utility_focus_delivered = view.update(cx, |tool, cx| { tool.focus_host(window, cx); tool.choose_focus.is_focused(window) });
                                } else if let Some(view) = file_search_view {
                                    root.root_scroll.scroll_to_top_of_item(root.file_search_scroll_item());
                                    utility_focus_delivered = view.update(cx, |tool, cx| { tool.focus_host(window, cx); tool.host_focus_is_exact(window, cx) });
                                } else if let Some(view) = clipboard_modify_view {
                                    let host = window.window_handle().window_id();
                                    let history = root.clipboard_history.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(host, false));
                                    root.root_scroll.scroll_to_top_of_item(root.clipboard_scroll_item(host, cx) + usize::from(history));
                                    utility_focus_delivered = view.update(cx, |tool, cx| { tool.focus_host(window, cx); tool.contains_focus(window, cx) });
                                } else if let Some(view) = clipboard_view {
                                    root.scroll_to_clipboard(window.window_handle().window_id(), cx);
                                    utility_focus_delivered = view.update(cx, |tool, cx| { tool.focus_editor(window, cx); tool.contains_focus(window, cx) });
                                } else if let Some(view) = regex_view {
                                    root.scroll_to_regex(cx);
                                    utility_focus_delivered = view.update(cx, |tool, cx| { tool.focus_editor(window, cx); tool.contains_focus(window, cx) });
                                } else if let Some(view) = xml_view {
                                    root.scroll_to_xml(cx);
                                    utility_focus_delivered = view.update(cx, |tool, cx| { tool.focus_editor(window, cx); tool.contains_focus(window, cx) });
                                } else if let Some(view) = json_view {
                                    root.scroll_to_json(cx);
                                    utility_focus_delivered = view.update(cx, |tool, cx| { tool.focus_editor(window, cx); tool.contains_focus(window, cx) });
                                } else if let Some(view) = hash_view {
                                    view.update(cx, |tool, cx| {
                                        tool.focus_editable(window, cx);
                                    });
                                } else {
                                    root.query.focus_handle(cx).focus(window, cx);
                                }
                            } else if !desired_visible && !guarded_hide {
                                root.cancel_docked_hash_preview();
                            }
                            (root.native_window, search_focus_selected, utility_focus_delivered)
                        },
                    );
                    let (search_focus_selected, utility_focus_delivered) = match query_window {
                        Ok((Some(current), search_focus_selected, utility_focus_delivered)) if current == native_window => {
                            (search_focus_selected, utility_focus_delivered)
                        }
                        _ => {
                            tracing::warn!("GPUI root window closed before hotkey presentation");
                            cx.update(|cx| cx.quit());
                            break;
                        }
                    };
                    if !reveal_current() {
                        rollback_reveal_unless_hotkey_pending(&visibility, revision, &trigger);
                        continue;
                    }
                    if !visibility.is_current(revision, desired_visible) {
                        continue;
                    }

                    let foreground_before_transition = current_foreground_identity();
                    let should_restore = !desired_visible
                        && foreground_before_transition == Some(native_window.identity);
                    let show_request_is_current = desired_visible
                        && foreground
                            .is_some_and(|captured| Some(captured) == foreground_before_transition);
                    if let Some((target, ref expected_query)) = switch_hide {
                        let current_query = update_kit_window_content(
                            root_window, &root_view, &mut *cx,
                            |root, _, cx| root.query.read(cx).value().to_string(),
                        ).ok();
                        if !window_switch_hide_is_current(
                            target, current_foreground_identity(), Some(expected_query.as_ref()), current_query.as_deref(),
                        ) {
                            visibility.revert_if_current(revision, false);
                            continue;
                        }
                    }
                    if let Some((root, _, expected_query)) = &close_hide {
                        let current_query = update_kit_window_content(
                            root_window, &root_view, &mut *cx,
                            |root, _, cx| root.query.read(cx).value().to_string(),
                        ).ok();
                        if !visibility.is_current(revision, false) || native_window.identity != *root
                            || !window_close_hide_is_current(*root, current_foreground_identity(), expected_query, current_query.as_deref())
                        {
                            visibility.revert_if_current(revision, false);
                            continue;
                        }
                    }
                    if let Some((expected_root, generation, expected_query, lease, focus, catalog_current)) = &undo_hide {
                        let current = update_kit_window_content(root_window, &root_view, &mut *cx, |root, window, cx| {
                            root.native_window == Some(native_window)
                                && root.root_window.is_some_and(|host| host.window_id() == window.window_handle().window_id())
                                && !window.has_active_dialog(cx)
                                && root.external_owner_is_current(focus.as_ref(), window, cx)
                                && catalog_current.as_ref().is_none_or(|current| current())
                                && clipboard_undo_hide_is_current(*expected_root, current_foreground_identity(), *generation, root.launcher_results.generation, expected_query, root.query.read(cx).value().as_str(), lease.load(Ordering::Acquire))
                        }).unwrap_or(false);
                        if !visibility.is_current(revision, false) || native_window.identity != *expected_root || !current {
                            visibility.revert_if_current(revision, false);
                            continue;
                        }
                    }
                    if !reveal_current() {
                        rollback_reveal_unless_hotkey_pending(&visibility, revision, &trigger);
                        continue;
                    }
                    if checked_hotkey_reveal && hotkey_edge_pending(&trigger) { continue; }
                    if let Err(error) = set_native_window_visible(native_window, desired_visible) {
                        if checked_hotkey_reveal {
                            rollback_reveal_unless_hotkey_pending(&visibility, revision, &trigger);
                        } else {
                            visibility.revert_if_current(revision, desired_visible);
                        }
                        tracing::error!(%error, "could not change GPUI root window visibility");
                        continue;
                    }
                    if checked_hotkey_reveal && hotkey_edge_pending(&trigger) { continue; }
                    if desired_visible && !(was_visible && dock_reveal.as_ref().is_some_and(|(_, _, mount)| mount.is_some())) {
                        return_target = foreground
                            .filter(|captured| Some(*captured) == foreground_before_transition)
                            .filter(|captured| captured.hwnd != native_window.identity.hwnd)
                            .filter(|captured| dock_reveal.as_ref().is_none_or(|(gesture, _, _)| *captured != gesture.source.identity));
                    } else if !desired_visible && !should_restore && switch_hide.is_none() {
                        return_target = None;
                    }
                    if !desired_visible && visibility.is_current(revision, false) {
                        if let Ok(mut saved) = visibility.snippet_origin.lock() { *saved = None; }
                    }
                    if !desired_visible {
                        if let Some((_, closed, _)) = close_hide {
                            return_target = exclude_closed_return_target(return_target, closed);
                        }
                    }
                    if !desired_visible {
                        if guarded_hide {
                            if switch_hide_accepted { return_target = None; }
                            let _ = update_kit_window_content(
                                root_window, &root_view, &mut *cx, |root, window, cx| {
                                    root.revoke_search_completion_focus();
                                    root.retire_action_sheet_now(window, cx, true);
                                    root.retire_context_favorite();
                                    root.retire_search_jump_mode();
                                    root.cancel_docked_hash_preview();
                                    root.window_search.retire(); root.file_search_popup_intent.store(false, Ordering::Release); root.file_search_probe.retire();
                                    if let Some(view) = root.file_search.clone() { view.update(cx, |view, cx| { view.retire_inline_preview(cx); }); }
                                },
                            );
                        } else {
                            let _ = root_view.update(&mut *cx, |root, cx| {
                                root.window_search.retire();
                                root.file_search_popup_intent.store(false, Ordering::Release); root.file_search_probe.retire();
                                if let Some(view) = root.file_search.clone() { view.update(cx, |view, cx| { view.retire_inline_preview(cx); }); }
                            });
                        }
                    }
                    if checked_hotkey_reveal && hotkey_edge_pending(&trigger) { continue; }
                    let foreground_after_transition = current_foreground_identity();
                    let show_request_is_current = show_request_is_current
                        && foreground_after_transition == foreground_before_transition;

                    let target = if desired_visible {
                        show_request_is_current.then_some(native_window)
                    } else if should_restore
                        && foreground_after_transition == Some(native_window.identity)
                    {
                        return_target.map(|identity| NativeWindow { identity })
                    } else {
                        None
                    };
                    let mut root_activation_accepted = false;
                    if let Some((gesture, generation, utility_mount_receipt)) = dock_reveal {
                        let current = visibility.is_current(revision, true)
                            && generation.is_none_or(|generation| dock_mount_generation.load(Ordering::Acquire) == generation)
                            && utility_mount_receipt.as_ref().is_none_or(|(mount, _)| mount.is_current())
                            && (utility_mount_receipt.is_none() || utility_focus_delivered)
                            && !restart_admission_for_hotkey.is_fenced()
                            && native_window_is_valid(native_window)
                            && native_window_is_valid(gesture.source);
                        let mut activated = false;
                        if hash_dock::may_activate(gesture.foreground_authorized, gesture.source.identity, current_foreground_identity(), current) {
                            if let Err(error) = activate_native_window(native_window) {
                                tracing::debug!(%error, "GPUI dock foreground activation was refused");
                            } else {
                                activated = true;
                            }
                        }
                        root_activation_accepted = activated
                            && current_foreground_identity() == Some(native_window.identity);
                        let destination_current = utility_mount_receipt.as_ref().is_none_or(|(receipt, kind)| {
                            update_kit_window_content(root_window, &root_view, &mut *cx, |root, window, app| {
                                root.native_window == Some(native_window)
                                    && root.utility_destination_current(*kind, receipt, window.window_handle().window_id(), app)
                            }).unwrap_or(false)
                        });
                        match requested_visible.take() {
                            Some(RootVisibilityRequest::DockHash(reveal)) => drop(reveal.acknowledgement),
                            Some(RootVisibilityRequest::DockUtility(_, reveal)) => {
                                if utility_dock_ack_allowed(activated, utility_focus_delivered, destination_current) && reveal.mount.is_current() && visibility.is_current(revision, true)
                                    && native_window_is_valid(gesture.source)
                                    && current_foreground_identity() == Some(native_window.identity)
                                    && !restart_admission_for_hotkey.is_fenced() {
                                    reveal.acknowledgement.accept(revision);
                                }
                                drop(reveal.acknowledgement);
                            }
                            _ => {}
                        }
                    } else if let Some(target) = target {
                        let expected_foreground = foreground_after_transition;
                        let visibility = visibility.clone();
                        let activation_trigger = checked_hotkey_reveal.then(|| trigger.clone());
                        root_activation_accepted = cx.background_executor()
                            .spawn(async move {
                                activate_for_root_request(
                                    &visibility,
                                    revision,
                                    desired_visible,
                                    target,
                                    expected_foreground,
                                    activation_trigger.as_deref(),
                                )
                            })
                            .await;
                    }
                    if desired_visible && visibility.is_current(revision, true) {
                        if let Ok(mut saved) = visibility.snippet_origin.lock() {
                            if was_visible {
                                if let Some((issued, _)) = saved.as_mut() { *issued = revision; }
                            } else if root_activation_accepted
                                && current_foreground_identity() == Some(native_window.identity)
                            {
                                    *saved = snippet_origin
                                        .filter(|_| foreground == foreground_before_transition && return_target == foreground && show_request_is_current)
                                        .map(|origin| (revision, origin));
                            } else {
                                *saved = None;
                            }
                        }
                    }
                    if hotkey_toggle
                        && desired_visible
                        && search_focus_selected
                        && show_request_is_current
                        && visibility.is_current(revision, true)
                    {
                        let foreground = cx
                            .background_executor()
                            .spawn(async { current_foreground_identity() })
                            .await;
                        if foreground == Some(native_window.identity)
                            && visibility.is_current(revision, true)
                        {
                            let _ = cx.update(|app| {
                                let _ = update_kit_window_content(
                                    root_window,
                                    &root_view,
                                    app,
                                    |root, window, cx| {
                                        if root.native_window == Some(native_window)
                                            && visibility.is_current(revision, true)
                                            && current_foreground_identity()
                                                == Some(native_window.identity)
                                        {
                                            root.grant_search_completion_focus_after_show(
                                                revision, window, cx,
                                            );
                                        }
                                    },
                                );
                            });
                        }
                    }
                }
            })
            .detach();
        }

        #[cfg(not(windows))]
        {
            root_view.update(cx, |root, cx| { root.start_clipboard_modify_catalog(cx); root.start_snippets_search(cx); });
            let _ = (native_window, hotkey, initial_return_target);
        }
    });

    visibility_after_run.deactivate();
    let restart_exit = restart_exit_after_run.borrow_mut().take();

    hash_wake_signal.close();
    hash_window_revision.fetch_add(1, Ordering::AcqRel);
    hash_session
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .shutdown();

    coordinate_hud_probe_closing.store(true, Ordering::Release);
    let coordinate_shutdown_result = if coordinate_shutdown_complete.load(Ordering::Acquire) {
        Ok(())
    } else {
        coordinate_hud_probe_controller
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .shutdown()
    };
    if let Err(error) = &coordinate_shutdown_result {
        if restart_exit.is_none() {
            tracing::warn!(%error, "could not fully shut down the GPUI Coordinate HUD probe");
        }
    }

    if let Ok(mut listener) = listener_slot.lock()
        && let Some(mut listener) = listener.take()
    {
        listener.stop();
    }

    let settings_save_receipt = settings_save_workers.join_all_checked();
    let (utility_size_failures, utility_size_pending) = {
        let mut store = utility_size_store_after_run
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = store.poll();
        (
            store.take_new_write_failures(),
            store.has_pending_acknowledgement(),
        )
    };
    for failure in utility_size_failures {
        tracing::error!(tool = failure.key.as_str(), error = %failure.message, "could not save utility window geometry during GPUI shutdown");
    }
    for (key, error) in utility_presentations_after_run.poll() {
        tracing::error!(tool = key.as_str(), %error, "could not save utility presentation preference during GPUI shutdown");
    }

    if let Some(error) = startup_error.borrow_mut().take() {
        return Err(format!(
            "GPUI could not open its window: {error}. Restart with {} to use the fallback frontend.",
            Frontend::Egui.selector_argument()
        ));
    }
    if let Some(error) = restart_failure_after_run.borrow_mut().take() {
        return Err(error);
    }

    if restart_exit.is_some() {
        let status = utility_size_store_after_run
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .status(hash_tool_key);
        if utility_size_pending || status.pending_revision.is_some() {
            return Err(
                "A utility geometry save did not settle before restart. Reopen GPUI and retry after the save finishes."
                    .to_owned(),
            );
        }
        if let Some((_, error)) = status.failure {
            return Err(format!(
                "The Hash Generator geometry save failed before restart. No egui process was started: {error}"
            ));
        }
        if let Err(error) = settings_save_receipt.ensure_clean() {
            return Err(format!(
                "The final Settings drain did not complete: {error}. No egui process was started. Resolve the reported persistence problem and rerun Multi Launcher from the same working directory."
            ));
        }
    }

    let Some((context, native_windows)) = restart_exit else {
        return Ok(None);
    };
    let coordinate_shutdown_result = coordinate_shutdown_result.map_err(|error| {
        format!(
            "The Coordinate HUD did not shut down cleanly; no egui process was started: {error}"
        )
    });
    #[cfg(windows)]
    let windows_retired = !native_windows
        .iter()
        .any(|window| native_window_is_valid(*window));
    #[cfg(not(windows))]
    let windows_retired = {
        let _ = native_windows;
        true
    };

    crate::gpui_restart::plan_after_teardown(
        context,
        Frontend::Egui,
        coordinate_shutdown_result,
        windows_retired,
    )
    .map(Some)
}

#[cfg(test)]
mod tests {
    #[test]
    fn app_activity_tracks_hash_across_fresh_sessions_without_private_inputs() {
        use multi_launcher::activity::{ActivityLifecycle, ActivityOperation, ActivityOutcome, ActivityOwner};
        use std::sync::Arc;
        use std::time::{Duration, Instant};

        let (center, session) = super::new_activity_hash_session();
        let secret = "private hash draft café";
        let directory = tempfile::tempdir().unwrap();
        let private_path = directory.path().join("private-hash-input.txt");
        std::fs::write(&private_path, secret).unwrap();
        for expected_rows in 1..=2 {
            {
                let mut session = session.lock().unwrap();
                session.begin((expected_rows == 2).then(|| private_path.clone()));
                if expected_rows == 1 { session.set_text(secret.to_owned()); }
                session.calculate(Arc::new(|| {})).unwrap();
            }
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                let mut session = session.lock().unwrap();
                session.poll();
                if !session.has_job() { break; }
                drop(session);
                assert!(Instant::now() < deadline, "hash worker did not finish");
                std::thread::yield_now();
            }
            let snapshot = center.snapshot();
            assert_eq!(snapshot.rows.len(), expected_rows);
            assert!(snapshot.rows.iter().all(|row| row.key.owner() == ActivityOwner::Hash
                && row.operation == ActivityOperation::CalculateHash
                && row.lifecycle == ActivityLifecycle::Terminal(ActivityOutcome::Completed)));
            let visible = format!("{snapshot:?}");
            let digest = session.lock().unwrap().result().unwrap().digest.clone();
            assert!(!visible.contains(secret));
            assert!(!visible.contains("private-hash-input.txt"));
            assert!(!visible.contains(&digest));
            session.lock().unwrap().end();
        }
        let snapshot = center.snapshot();
        assert_ne!(snapshot.rows[0].key, snapshot.rows[1].key);
    }

    use super::{
        ActivationSource, AppliedWindowSize, CommandOutcome, DockedHashResize,
        DockedHashResizeOrigin, ForegroundIdentity, Frontend, FrontendSelection, FutureContext,
        HashClipboardPendingKind, HashClipboardPendingState, HashClipboardRequest, HashDraft,
        HashPickerOutcome, HashPresentationMode, HashWakeSignal, InstalledSearchInvocation,
        InstalledSearchRoute, JumpModeChord, Hint, HintAlphabet, CommandHelpState,
        LauncherResultActivationOrigin,
        LauncherSearchCatalog, LauncherSearchResults, LauncherSearchStartup, Modifiers,
        PhysicalKeydown, Poll, JumpGeometryDraft, JumpGeometryFrame,
        JumpModeController,
        PreferredWindowSize, SearchCompletion, classify_hash_picker_result, dispatch_hash_open,
        root_scroll_viewport_size, docked_hash_fit, docked_hash_focus_is_current,
        docked_hash_resize_accessibility_label, docked_hash_resize_identity_matches,
        docked_resize_commit_size, duplicate_instance_message, fresh_search_enter_marker,
        hash_clipboard_copy_admissible, hash_clipboard_request_is_current,
        admit_quick_help_shortcut,
        hash_escape_capture_should_close, hash_file_picker_admissible, hash_mount_identity_matches,
        hash_picker_result_is_current,
        bounds_are_contained_by, clipped_jump_geometry, is_fresh_plain_search_enter,
        keyboard_docked_resize_preview,
        launcher_column_width, launcher_result_activation_is_current, launcher_result_grid_geometry,
        launcher_result_grid_row_id, launcher_result_row_id,
        parse_installed_search_invocation,
        physical_keypad_selection_move, pointer_docked_resize_preview, query_set_outcome_text,
        result_click_source, root_menu_close_should_restore_focus,
        root_search_focus_is_current, root_tab_traversed_to_search,
        jump_badge_placement, search_jump_result_target_id, search_jump_search_target_id, select_search_result_state,
        search_completion_session_is_current, search_input_is_current, selection_from_args,
        RootCloseBlocker, ToastLogViewerState,
        clear_stale_settings_restart_status, clear_toast_log_open_pending_status,
        command_help_escape_capture_should_close,
        docked_hash_root_scroll_item_index,
        quick_help_escape_capture_should_close, quick_help_event_targets_visible_revision,
        quick_help_focus_change_is_allowed, quick_help_viewport_body_height,
        QuickHelpFocusBlockers, GPUI_BOUND_KEYS,
        gpui_key_bindings, quick_help_hotkey_lines, quick_help_root_scroll_item_index,
        command_help_viewport_body_height,
        settings_menu_close_needs_trigger_focus, settings_return_focus_is_still_owned,
        root_close_blocker,
        SETTINGS_DIRTY_RESTART_STATUS,
        TOAST_LOG_OPEN_PENDING_STATUS,
        single_line_history_queries,
        supported_launcher_search_actions,
        toast_log_viewport_body_height,
    };
    use gpui_kit::{Bounds, KeyContext, Pixels, point, px};
    use multi_launcher::{
        commands::{Command, DialogCommand},
        hash_generator::{HashAlgorithm, parse_expected_digest},
        launcher::{
            jump_mode::{HintMap, TargetGeneration, TargetId, TargetKind, TargetMetadata},
            query_results_layout::{QuerySelectionMove, ResolvedQueryResultsLayout},
        },
        plugin::Plugin as _,
        settings::{QueryResultsLayoutSettings, RememberedPresentation},
    };
    use std::collections::{HashMap, HashSet};
    use std::ffi::OsString;
    use std::future::Future;
    use std::path::PathBuf;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::task::{Wake, Waker};

    #[derive(Default)]
    struct WakeCounter(AtomicUsize);

    impl Wake for WakeCounter {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn args(args: &[&str]) -> Vec<OsString> {
        args.iter().map(|arg| OsString::from(*arg)).collect()
    }

    fn hash_action() -> multi_launcher::actions::Action {
        let hash_generator = multi_launcher::plugins::hash_generator::HashGeneratorPlugin;
        multi_launcher::plugin::search_filtered_plugins(
            [&hash_generator as &dyn multi_launcher::plugin::Plugin],
            "hash",
            None,
            None,
        )
        .into_iter()
        .next()
        .expect("the built-in Hash Generator route is installed")
    }

    fn physical_keydown(scan_code: u8, virtual_key: u16) -> PhysicalKeydown {
        PhysicalKeydown {
            hwnd: 17,
            virtual_key,
            scan_code,
            extended: false,
            modifiers: Modifiers::default(),
            held: false,
        }
    }

    fn test_bounds(left: f32, top: f32, width: f32, height: f32) -> Bounds<Pixels> {
        Bounds::from_corners(
            point(px(left), px(top)),
            point(px(left + width), px(top + height)),
        )
    }

    fn jump_test_theme_metrics() -> multi_launcher::gui::theme::ThemeMetrics {
        multi_launcher::gui::theme::ThemeMetrics {
            body_text_size: 14.0,
            heading_text_size: 18.0,
            small_text_size: 12.0,
            line_height: 18.0,
            item_spacing_x: 6.0,
            item_spacing_y: 4.0,
            section_spacing: 8.0,
            window_padding: 8.0,
            panel_padding: 6.0,
            button_padding_x: 8.0,
            button_padding_y: 4.0,
            corner_radius: 4.0,
            stroke_width: 1.0,
            shadow_blur: 8.0,
            icon_size: 16.0,
            row_min_height: 28.0,
        }
    }

    fn jump_test_frame(
        generation: u64,
        visible_rows: &[usize],
        layout: ResolvedQueryResultsLayout,
    ) -> JumpGeometryFrame {
        let mut draft = JumpGeometryDraft::new(generation);
        let clip = test_bounds(0.0, 0.0, 500.0, 600.0);
        let search_id = search_jump_search_target_id(generation);
        let search = TargetMetadata::new(search_id, "Search", TargetKind::TextEntry, 0);
        draft.targets.push(
            clipped_jump_geometry(
                search,
                test_bounds(0.0, 0.0, 460.0, 32.0),
                clip.clone(),
            )
            .expect("the Search fixture is visible"),
        );
        for &index in visible_rows {
            let (left, top, width, height) = if layout.grid {
                let column = index % layout.columns;
                let row = index / layout.columns;
                (column as f32 * 165.0, 42.0 + row as f32 * 64.0, 160.0, 60.0)
            } else {
                (0.0, 42.0 + index as f32 * 48.0, 460.0, 44.0)
            };
            let metadata = TargetMetadata::new(
                search_jump_result_target_id(generation, index),
                format!("action-{index}"),
                TargetKind::Action,
                index.saturating_add(1) as u64,
            );
            draft.targets.push(
                clipped_jump_geometry(
                    metadata,
                    test_bounds(left, top, width, height),
                    clip.clone(),
                )
                .expect("each fixture result is clipped into view"),
            );
        }
        draft.finish(
            None,
            &[],
            &HintAlphabet::default(),
            jump_test_theme_metrics(),
        )
    }

    fn jump_test_actions(count: usize) -> Vec<multi_launcher::actions::Action> {
        (0..count)
            .map(|index| {
                let mut action = hash_action();
                action.label = format!("Fixture action {index}");
                action.args = Some(format!("full-action-payload-{index}"));
                action
            })
            .collect()
    }

    fn jump_leader(chord: JumpModeChord) -> PhysicalKeydown {
        let virtual_key = multi_launcher::hotkey::launcher_invocation::vk_from_key(
            chord.hotkey_key(),
        )
        .expect("the configured Jump Mode key has a native virtual key") as u16;
        let mut receipt = physical_keydown(0, virtual_key);
        receipt.modifiers = Modifiers {
            control: chord.ctrl(),
            alt: chord.alt(),
            shift: chord.shift(),
            ..Modifiers::default()
        };
        receipt
    }

    fn enter_jump_hint(
        controller: &mut JumpModeController,
        frame: &JumpGeometryFrame,
        generation: u64,
        hint: &str,
    ) -> Option<TargetId> {
        let mut completed = None;
        for character in hint.chars() {
            let receipt = physical_keydown(0, character as u16);
            let outcome = controller.process_key_down(receipt, Some(frame), generation);
            if outcome.selected_target.is_some() {
                completed = outcome.selected_target;
            }
        }
        completed
    }

    fn launcher_search_contexts() -> Vec<KeyContext> {
        ["RootWindow", "LauncherSearch"]
            .into_iter()
            .map(|name| KeyContext::parse(name).expect("valid launcher key context"))
            .collect()
    }

    #[test]
    fn jump_mode_root_result_hints_select_without_activation_in_list_and_grid_then_fresh_enter_activates_full_action() {
        let chord = JumpModeChord::parse("Ctrl+J").expect("valid configured leader");
        for layout in [
            ResolvedQueryResultsLayout::new(false, 1),
            ResolvedQueryResultsLayout::new(true, 3),
        ] {
            let mut results = LauncherSearchResults {
                generation: 19,
                query: "hash".to_owned(),
                actions: jump_test_actions(7)
                    .into_iter()
                    .map(|action| super::LauncherSearchResult {
                        action,
                        origin: super::LauncherResultOrigin::Saved,
                        display_label: None,
                        hide_description: false,
                    })
                    .collect(),
                selected: None,
                layout,
                page_jump: 3,
            };
            let visible_rows = (0..7).collect::<Vec<_>>();
            let frame = jump_test_frame(19, &visible_rows, layout);
            let mut jump = JumpModeController::new(
                true,
                chord,
                HintAlphabet::default(),
            );
            let started = jump.process_key_down(jump_leader(chord), Some(&frame), 19);
            assert!(started.consumed);
            assert!(started.session_transition);
            assert!(jump.is_active());
            assert_eq!(results.enter_target("hash"), None);

            let target = search_jump_result_target_id(19, 6);
            let hint = jump
                .hint_for(&target)
                .expect("visible result has a frozen hint")
                .as_str()
                .to_owned();
            let completed = enter_jump_hint(&mut jump, &frame, 19, &hint);
            assert_eq!(completed, Some(target.clone()));
            assert!(!jump.is_active());
            assert_eq!(results.selected, None);
            assert_eq!(results.enter_target("hash"), None);

            let (changed, reveal_row) = select_search_result_state(
                &mut results,
                "hash",
                19,
                6,
            )
            .expect("the exact current result remains selectable");
            assert!(changed);
            assert_eq!(reveal_row, Some(layout.scroll_child_for(6)));
            assert_eq!(results.query, "hash");
            assert_eq!(results.enter_target("hash"), Some((19, 6)));
            assert_eq!(
                select_search_result_state(&mut results, "hash", 19, 6),
                Some((false, None)),
                "an accepted hint does not repeat the reveal when already selected",
            );

            let marker = fresh_search_enter_marker(
                true,
                "enter",
                Modifiers::default(),
                false,
                19,
                "hash".to_owned(),
                jump.revision(),
            )
            .expect("only a separate fresh Enter can activate the selected row");
            assert_eq!(marker.jump_mode_revision, jump.revision());
            let (generation, index) = results.enter_target(&marker.query).unwrap();
            let action = results
                .resolve_action(generation, index, &marker.query)
                .expect("fresh Enter resolves the current full action");
            assert_eq!(action.label, "Fixture action 6");
            assert_eq!(action.args.as_deref(), Some("full-action-payload-6"));
        }
    }

    #[test]
    fn hash_jump_rejects_roles_when_live_eligibility_changes() {
        use super::hash_jump::{HashControl, HashEligibility};
        let mut state = HashEligibility {
            current: true, mode: super::HashInputMode::Text,
            presentation: super::HashPresentationMode::Docked, has_job: false,
            is_running: false, picker_pending: false, clipboard_pending: false,
            clipboard_copy_pending: false, can_copy_digest: true,
        };
        assert!(state.allows(HashControl::Text));
        assert!(state.allows(HashControl::Calculate));
        assert!(state.allows(HashControl::Copy));
        state.mode = super::HashInputMode::File;
        state.has_job = true;
        state.can_copy_digest = false;
        assert!(!state.allows(HashControl::Text));
        assert!(!state.allows(HashControl::Calculate));
        assert!(!state.allows(HashControl::Copy));
        assert!(state.allows(HashControl::File));
        state.picker_pending = true;
        assert!(!state.allows(HashControl::File));
        state.current = false;
        assert!(!state.allows(HashControl::Expected));
    }

    #[test]
    fn hash_popup_cursor_direction_handles_unselected_and_current_rows() {
        use super::hash_jump_select::{CursorMove, cursor_move};
        assert_eq!(cursor_move(None, 0), Some(CursorMove::Down));
        let mut cursor = Some(0);
        assert_eq!(cursor_move(cursor, 2), Some(CursorMove::Down));
        cursor = Some(1);
        assert_eq!(cursor_move(cursor, 2), Some(CursorMove::Down));
        cursor = Some(2);
        assert_eq!(cursor_move(cursor, 2), None);
        assert_eq!(cursor_move(cursor, 0), Some(CursorMove::Up));
    }

    #[test]
    fn hash_select_popup_targets_are_scoped_by_opening_and_selector() {
        use super::hash_jump_select::{HashPopupKind, PopupCollector};
        let source = PopupCollector::new(HashPopupKind::Source, 4, true);
        let algorithm = PopupCollector::new(HashPopupKind::Algorithm, 4, true);
        let first = source.borrow().target(0);
        assert_ne!(first, algorithm.borrow().target(0));
        source.borrow_mut().dismiss();
        assert_ne!(first, source.borrow().target(0));
        assert_ne!(source.borrow().target(0), source.borrow().target(1));
    }

    #[test]
    fn hash_jump_target_roles_are_mount_scoped() {
        use super::hash_jump::{HashControl, HashJump};
        let target = HashJump::id(14, HashControl::Calculate);
        assert_eq!(target.generation(), TargetGeneration(14));
        assert_ne!(target, HashJump::id(15, HashControl::Calculate));
        let multi_launcher::launcher::jump_mode::SemanticTargetId::Named(name) = target.semantic()
        else {
            panic!("Hash controls use named targets")
        };
        assert_eq!(HashControl::from_name(name), Some(HashControl::Calculate));
        assert_eq!(HashControl::from_name("uninstalled"), None);
    }

    #[test]
    fn hash_jump_escape_ownership_survives_cancel_until_fresh_key() {
        let chord = JumpModeChord::default();
        let frame = jump_test_frame(51, &[0], ResolvedQueryResultsLayout::default());
        let mut jump = JumpModeController::new(true, chord, HintAlphabet::default());
        assert!(
            jump.process_key_down(jump_leader(chord), Some(&frame), 51)
                .session_transition
        );
        let escape = physical_keydown(0, 0x1b);
        assert!(jump.process_key_down(escape, Some(&frame), 51).consumed);
        assert!(!jump.is_active());
        assert!(jump.owns_key(0x1b));
        let mut held = escape;
        held.held = true;
        assert!(jump.process_key_down(held, Some(&frame), 51).consumed);
        assert!(jump.owns_key(0x1b));
        assert!(!jump.process_key_down(escape, Some(&frame), 51).consumed);
        assert!(!jump.owns_key(0x1b));
    }

    #[test]
    fn jump_named_target_population_uses_canonical_ids_and_visual_hint_order() {
        use multi_launcher::launcher::jump_mode::{SemanticTargetId, SurfaceId};

        let generation = 41;
        let first = TargetId::new(
            SurfaceId::LauncherResults,
            SemanticTargetId::named("z-first"),
            TargetGeneration(generation),
        );
        let second = TargetId::new(
            SurfaceId::LauncherResults,
            SemanticTargetId::named("a-second"),
            TargetGeneration(generation),
        );
        let mut draft = JumpGeometryDraft::new(generation);
        for (order, id) in [first.clone(), second.clone()].into_iter().enumerate() {
            let meta = TargetMetadata::new(id, "fixture", TargetKind::Action, order as u64);
            draft.targets.push(
                clipped_jump_geometry(
                    meta,
                    test_bounds(0.0, order as f32 * 50.0, 200.0, 40.0),
                    test_bounds(0.0, 0.0, 300.0, 200.0),
                )
                .unwrap(),
            );
        }
        let alphabet = HintAlphabet::default();
        let frame = draft.finish(None, &[], &alphabet, jump_test_theme_metrics());
        assert_eq!(frame.target_ids(), &[second, first.clone()]);
        let mut jump = JumpModeController::new(true, JumpModeChord::default(), alphabet.clone());
        assert!(jump.start(&frame));
        let map = HintMap::assign_with_alphabet(
            frame.targets.iter().map(|target| target.metadata.clone()),
            &alphabet,
        )
        .unwrap();
        assert_eq!(map.assignments()[0].target().id(), &first);
        assert_eq!(jump.hint_for(&first), Some(map.assignments()[0].hint()));
        assert_eq!(jump.target_ids(), frame.target_ids());
        let next = draft.finish(
            Some(generation),
            jump.target_ids(),
            &alphabet,
            jump_test_theme_metrics(),
        );
        assert!(next.matches_active_session);
        assert!(!jump.reconcile_frame(Some(&next)));
        assert!(jump.is_active());
    }

    #[test]
    fn jump_mode_root_result_targets_follow_visible_clipped_rows_and_cancel_on_generation_change() {
        let cell = test_bounds(0.0, 0.0, 100.0, 40.0);
        let root_clip = test_bounds(0.0, 0.0, 80.0, 40.0);
        let results_clip = test_bounds(0.0, 0.0, 60.0, 25.0);
        let nested_clip = root_clip.intersect(&results_clip);
        let metadata = TargetMetadata::new(
            TargetId::root_result(TargetGeneration(23), 3),
            "Partial grid cell",
            TargetKind::Action,
            4,
        );
        let partial = clipped_jump_geometry(metadata.clone(), cell.clone(), nested_clip)
            .expect("a partially clipped cell remains a visible target");
        assert_eq!(partial.visible_bounds.size.width.as_f32(), 60.0);
        assert_eq!(partial.visible_bounds.size.height.as_f32(), 25.0);
        let metrics = jump_test_theme_metrics();
        let badge_fits_visible_bounds = |target: &super::JumpTargetGeometry, hint: &Hint| {
            let placement = jump_badge_placement(target, hint, metrics)
                .expect("the fixture clip can contain the hint badge");
            let badge_bounds = test_bounds(
                target.bounds.origin.x.as_f32() + placement.left,
                target.bounds.origin.y.as_f32() + placement.top,
                placement.width,
                placement.height,
            );
            bounds_are_contained_by(&badge_bounds, &target.visible_bounds)
        };
        let partial_hint_map = HintMap::assign([partial.metadata.clone()]).unwrap();
        assert!(badge_fits_visible_bounds(
            &partial,
            partial_hint_map.assignments()[0].hint(),
        ));

        let scrolled_list = clipped_jump_geometry(
            TargetMetadata::new(
                TargetId::root_result(TargetGeneration(23), 5),
                "Scrolled List row",
                TargetKind::Action,
                6,
            ),
            test_bounds(0.0, -20.0, 220.0, 44.0),
            test_bounds(0.0, 0.0, 200.0, 24.0),
        )
        .expect("a partially scrolled List row remains visible");
        let list_hint_map = HintMap::assign([scrolled_list.metadata.clone()]).unwrap();
        assert!(badge_fits_visible_bounds(
            &scrolled_list,
            list_hint_map.assignments()[0].hint(),
        ));

        let mut tiny_clip_draft = JumpGeometryDraft::new(25);
        let tiny_target = TargetMetadata::new(
            TargetId::root_result(TargetGeneration(25), 0),
            "Tiny clipped row",
            TargetKind::Action,
            1,
        );
        tiny_clip_draft.targets.push(
            clipped_jump_geometry(
                tiny_target,
                test_bounds(0.0, 0.0, 100.0, 40.0),
                test_bounds(0.0, 0.0, 8.0, 8.0),
            )
            .expect("the row has a positive but too-small visible clip"),
        );
        let tiny_clip_frame = tiny_clip_draft.finish(
            None,
            &[],
            &HintAlphabet::default(),
            metrics,
        );
        assert!(tiny_clip_frame.targets.is_empty());
        let mut empty_population_jump = JumpModeController::new(
            true,
            JumpModeChord::default(),
            HintAlphabet::default(),
        );
        assert!(empty_population_jump
            .process_key_down(
                jump_leader(JumpModeChord::default()),
                Some(&tiny_clip_frame),
                25,
            )
            .consumed);
        assert!(!empty_population_jump.is_active());
        assert!(
            !empty_population_jump
                .process_key_down(physical_keydown(0, b'A' as u16), Some(&tiny_clip_frame), 25,)
                .consumed,
            "an empty eligible population does not silently own later input",
        );

        assert!(
            clipped_jump_geometry(
                metadata,
                test_bounds(70.0, 0.0, 20.0, 20.0),
                nested_clip,
            )
            .is_none(),
            "a fully clipped cell is not registered",
        );

        let layout = ResolvedQueryResultsLayout::new(true, 3);
        let visible_rows = (0..7).collect::<Vec<_>>();
        let original = jump_test_frame(23, &visible_rows, layout);
        let mut jump = JumpModeController::new(
            true,
            JumpModeChord::default(),
            HintAlphabet::default(),
        );
        let leader = jump_leader(JumpModeChord::default());
        assert!(jump.process_key_down(leader, Some(&original), 23).session_transition);
        let target = search_jump_result_target_id(23, 4);
        let original_hint = jump.hint_for(&target).unwrap().as_str().to_owned();

        let mut moved = original.clone();
        let target_geometry = moved
            .targets
            .iter_mut()
            .find(|geometry| geometry.metadata.id() == &target)
            .unwrap();
        target_geometry.bounds.origin.x = px(7.0);
        target_geometry.visible_bounds.origin.x = px(7.0);
        assert!(!jump.reconcile_frame(Some(&moved)));
        assert_eq!(jump.hint_for(&target).unwrap().as_str(), original_hint);

        let clipped_population = jump_test_frame(23, &[0, 1, 2, 3], layout);
        assert!(jump.reconcile_frame(Some(&clipped_population)));
        assert!(!jump.is_active());

        assert!(jump.start(&original));
        let new_generation = jump_test_frame(24, &visible_rows, layout);
        assert!(jump.reconcile_frame(Some(&new_generation)));
        assert!(!jump.is_active());
    }

    #[test]
    fn jump_mode_root_input_deduplicates_text_repeats_and_cancels_without_leaks() {
        let chord = JumpModeChord::parse("Shift+2").expect("baseline configured chord");
        let alphabet = HintAlphabet::parse("ZXQ").expect("valid frozen custom alphabet");
        let layout = ResolvedQueryResultsLayout::new(false, 1);
        let rows = (0..7).collect::<Vec<_>>();
        let frame = jump_test_frame(31, &rows, layout);
        let mut jump = JumpModeController::new(true, chord, alphabet);
        assert!(jump.should_process_key_down(jump_leader(chord)));
        assert!(jump.process_key_down(jump_leader(chord), Some(&frame), 31).consumed);
        let idle_key = physical_keydown(0, b'A' as u16);
        assert!(!JumpModeController::new(true, chord, HintAlphabet::default())
            .should_process_key_down(idle_key));
        assert!(jump.should_process_key_down(idle_key));

        let mut unrelated_shortcut = physical_keydown(0, b'B' as u16);
        unrelated_shortcut.modifiers.control = true;
        assert!(!jump.should_process_key_down(unrelated_shortcut));
        assert!(!jump
            .process_key_down(unrelated_shortcut, Some(&frame), 31)
            .consumed);
        unrelated_shortcut.held = true;
        assert!(!jump.should_process_key_down(unrelated_shortcut));
        assert!(!jump
            .process_key_down(unrelated_shortcut, Some(&frame), 31)
            .consumed);
        assert!(!jump.held_owned_vkeys.contains(&u16::from(b'B')));

        let multi_character_hint = jump
            .target_ids()
            .iter()
            .find_map(|target| {
                jump.hint_for(target)
                    .filter(|hint| hint.as_str().len() > 1)
                    .map(|hint| (target.clone(), hint.as_str().to_owned()))
            })
            .expect("the constrained alphabet requires a multi-character code");
        let first_character = multi_character_hint.1.as_bytes()[0];
        let first = jump.process_key_down(
            physical_keydown(0, u16::from(first_character)),
            Some(&frame),
            31,
        );
        assert!(first.consumed);
        let expected_prefix = char::from(first_character).to_string();
        assert_eq!(jump.session.as_ref().unwrap().prefix(), expected_prefix);
        let mut repeated_character = physical_keydown(0, u16::from(first_character));
        repeated_character.held = true;
        assert!(jump.should_process_key_down(repeated_character));
        assert!(jump
            .process_key_down(repeated_character, Some(&frame), 31)
            .consumed);
        assert_eq!(jump.session.as_ref().unwrap().prefix(), expected_prefix);

        let mut modified_enter = physical_keydown(0, 0x0D);
        modified_enter.modifiers.control = true;
        assert!(jump.should_process_key_down(modified_enter));
        assert!(jump
            .process_key_down(modified_enter, Some(&frame), 31)
            .consumed);
        let mut held_modified_enter = modified_enter;
        held_modified_enter.held = true;
        assert!(jump.should_process_key_down(held_modified_enter));
        assert!(jump
            .process_key_down(held_modified_enter, Some(&frame), 31)
            .consumed);
        let mut alt_enter = physical_keydown(0, 0x0D);
        alt_enter.modifiers.alt = true;
        assert!(jump.should_process_key_down(alt_enter));
        assert!(jump
            .process_key_down(alt_enter, Some(&frame), 31)
            .consumed);
        let mut held_alt_enter = alt_enter;
        held_alt_enter.held = true;
        assert!(jump
            .process_key_down(held_alt_enter, Some(&frame), 31)
            .consumed);
        let mut control_shift_enter = physical_keydown(0, 0x0D);
        control_shift_enter.modifiers.control = true;
        control_shift_enter.modifiers.shift = true;
        assert!(jump.should_process_key_down(control_shift_enter));
        assert!(jump
            .process_key_down(control_shift_enter, Some(&frame), 31)
            .consumed);
        let mut control_space = physical_keydown(0, 0x20);
        control_space.modifiers.control = true;
        assert!(jump.should_process_key_down(control_space));
        assert!(jump
            .process_key_down(control_space, Some(&frame), 31)
            .consumed);
        let mut held_control_space = control_space;
        held_control_space.held = true;
        assert!(jump.should_process_key_down(held_control_space));
        assert!(jump
            .process_key_down(held_control_space, Some(&frame), 31)
            .consumed);
        let mut modified_space = physical_keydown(0, 0x20);
        modified_space.modifiers.alt = true;
        assert!(!jump.should_process_key_down(modified_space));
        assert!(!jump
            .process_key_down(modified_space, Some(&frame), 31)
            .consumed);
        let mut held_modified_space = modified_space;
        held_modified_space.held = true;
        assert!(!jump
            .process_key_down(held_modified_space, Some(&frame), 31)
            .consumed);
        let mut alt_shift_space = physical_keydown(0, 0x20);
        alt_shift_space.modifiers.alt = true;
        alt_shift_space.modifiers.shift = true;
        assert!(jump.should_process_key_down(alt_shift_space));
        assert!(jump
            .process_key_down(alt_shift_space, Some(&frame), 31)
            .consumed);
        assert!(jump.is_active());
        assert_eq!(jump.session.as_ref().unwrap().prefix(), expected_prefix);

        let alt_space_chord = JumpModeChord::parse("Alt+Space").unwrap();
        let mut native_leader_jump =
            JumpModeController::new(true, alt_space_chord, HintAlphabet::default());
        assert!(native_leader_jump.should_process_key_down(jump_leader(alt_space_chord)));
        let native_start = native_leader_jump.process_key_down(
            jump_leader(alt_space_chord),
            Some(&frame),
            31,
        );
        assert!(!native_start.consumed);
        assert!(native_start.session_transition && native_leader_jump.is_active());
        let native_toggle = native_leader_jump.process_key_down(
            jump_leader(alt_space_chord),
            Some(&frame),
            31,
        );
        assert!(!native_toggle.consumed);
        assert!(native_toggle.session_transition && !native_leader_jump.is_active());

        assert!(jump
            .process_key_down(physical_keydown(0, 0x08), Some(&frame), 31)
            .consumed);
        assert_eq!(jump.session.as_ref().unwrap().prefix(), "");
        let mut held_backspace = physical_keydown(0, 0x08);
        held_backspace.held = true;
        assert!(jump
            .process_key_down(held_backspace, Some(&frame), 31)
            .consumed);
        assert_eq!(jump.session.as_ref().unwrap().prefix(), "");

        let cancelled = jump.process_key_down(
            physical_keydown(0, 0x1B),
            Some(&frame),
            31,
        );
        assert!(cancelled.consumed && cancelled.session_transition);
        assert!(!jump.is_active());
        let mut held_escape = physical_keydown(0, 0x1B);
        held_escape.held = true;
        assert!(jump
            .process_key_down(held_escape, Some(&frame), 31)
            .consumed);
        assert!(!jump
            .process_key_down(physical_keydown(0, b'C' as u16), Some(&frame), 31)
            .consumed);

        assert!(jump.process_key_down(jump_leader(chord), Some(&frame), 31).consumed);
        let toggled_off = jump.process_key_down(jump_leader(chord), Some(&frame), 31);
        assert!(toggled_off.consumed && toggled_off.session_transition);
        assert!(!jump.is_active());
        assert_eq!(multi_character_hint.0.generation(), TargetGeneration(31));
    }

    pub(super) fn completion_catalog(
        actions: Vec<multi_launcher::actions::Action>,
        query_autocomplete: bool,
        enabled_plugins: Option<&HashSet<String>>,
    ) -> LauncherSearchCatalog {
        LauncherSearchCatalog::new(
            LauncherSearchStartup {
                branding: super::launcher_header::Branding::default(),
                verified_folders: Vec::new(),
                index_config: multi_launcher::indexer::coordinator::IndexConfig::new(Vec::new(), None),
                actions,
                calculator_settings: None,
                match_exact: true,
                fuzzy_weight: 0.75,
                usage_weight: 0.0,
                query_autocomplete,
                show_examples: false,
                page_jump: 2,
                query_results_layout: QueryResultsLayoutSettings::default(),
                jump_mode_enabled: true,
                jump_mode_activation_chord: JumpModeChord::default(),
                jump_mode_hint_alphabet: HintAlphabet::default(),
                usage: HashMap::new(),
                folder_aliases_lc: HashMap::new(),
                bookmark_aliases_lc: HashMap::new(),
            },
            enabled_plugins,
        )
    }

    #[test]
    fn exact_scoring_keeps_only_the_stock_typed_settings_row() {
        use multi_launcher::plugin::Plugin;
        let query = "settings toast duration";
        let action = multi_launcher::plugins::settings::SettingsPlugin.search(query).remove(0);
        let catalog = completion_catalog(Vec::new(), true, None);
        let rows = catalog.compose(query, vec![("settings".into(), action.clone())], false);
        assert!(rows.iter().any(|row| row.action == action && matches!(&row.origin, super::LauncherResultOrigin::Provider(name) if name == "settings")));
    }

    #[test]
    fn exact_scoring_keeps_the_typed_content_stock_row() {
        use multi_launcher::file_search::{model::SearchKind, query::SearchRequestDraft};
        let query = r#"fs content "café needle" "C:\Owned Corpus""#;
        let draft = SearchRequestDraft { kind: SearchKind::Content, root: Some(std::path::PathBuf::from(r"C:\Owned Corpus")), search_text: "café needle".into() };
        let action = multi_launcher::plugins::file_search::start_action(draft);
        let catalog = completion_catalog(Vec::new(), true, None);
        let rows = catalog.compose(query, vec![("file_search".into(), action.clone())], false);
        assert!(rows.iter().any(|row| row.action == action && matches!(&row.origin, super::LauncherResultOrigin::Provider(name) if name == "file_search")));
    }

    #[test]
    fn folder_description_redecoration_updates_only_folder_rows() {
        let folder = multi_launcher::actions::Action { label: "Alias".into(), desc: "C:/owned".into(), action: "C:/owned".into(), args: None };
        let other = multi_launcher::actions::Action { label: "Other".into(), desc: "Text".into(), action: "other".into(), args: None };
        let mut rows = vec![super::LauncherSearchResult { action: folder, origin: super::LauncherResultOrigin::Provider("folders".into()), display_label: None, hide_description: false },
            super::LauncherSearchResult { action: other, origin: super::LauncherResultOrigin::Saved, display_label: None, hide_description: true }];
        let aliases = HashMap::from([("C:/owned".to_owned(), HashSet::from(["Alias".to_owned()]))]);
        super::decorate_folder_descriptions(&mut rows, false, false, &aliases);
        assert!(rows[0].hide_description && rows[1].hide_description);
        super::decorate_folder_descriptions(&mut rows, true, false, &HashMap::new());
        assert!(!rows[0].hide_description && rows[1].hide_description);
        super::decorate_folder_descriptions(&mut rows, false, true, &aliases);
        assert!(!rows[0].hide_description && rows[1].hide_description);
    }

    #[test]
    fn window_close_preparation_uses_existing_provider_scoring() {
        let mut catalog = completion_catalog(Vec::new(), true, None);
        let mut switch = saved_action("Switch to Editor", "window:switch:42", None); switch.desc = "Windows".into();
        let mut close = saved_action("Close Editor", "window:close:42", None); close.desc = "Windows".into();
        let mut other = saved_action("Close Browser", "window:close:43", None); other.desc = "Windows".into();
        let rows = || vec![("windows".to_owned(), switch.clone()), ("windows".to_owned(), close.clone()), ("windows".to_owned(), other.clone())];
        let exact = catalog.compose("win Editor", rows(), false);
        assert!(exact.iter().any(|row| row.action == switch && matches!(&row.origin, super::LauncherResultOrigin::Provider(name) if name == "windows")));
        assert!(exact.iter().any(|row| row.action == close && matches!(&row.origin, super::LauncherResultOrigin::Provider(name) if name == "windows")));
        assert!(!exact.iter().any(|row| row.action == other));
        catalog.match_exact = false;
        assert!(catalog.compose("win Editor", rows(), false).iter().any(|row| row.action == close));
    }

    #[test]
    fn calculator_history_exact_composition_keeps_original_filtered_index() {
        use multi_launcher::plugins::calc_history::CalcHistoryEntry;
        let entries = [
            CalcHistoryEntry { expr: "first".into(), result: "1".into() },
            CalcHistoryEntry { expr: "second".into(), result: "2".into() },
        ];
        let action = multi_launcher::plugins_builtin::calculator_history_row(1, &entries[1]);
        let catalog = completion_catalog(Vec::new(), true, None);
        let rows = catalog.compose(
            "id:calc:history:1 = history",
            vec![("calculator".into(), action.clone())],
            false,
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].action, action);
        assert_eq!(rows[0].action.action, "calc:history:1");
        assert_eq!(rows[0].origin, super::LauncherResultOrigin::Provider("calculator".into()));
        let filtered = catalog.compose(
            "id:calc:history:1 = list",
            vec![("calculator".into(), action)],
            false,
        );
        assert!(filtered.is_empty());
    }

    #[test]
    fn indexed_completion_merges_sorted_unique_bounded_suggestions() {
        let mut catalog = completion_catalog(
            vec![saved_action("Alpha", "hash:open", None), saved_action("Echo", "hash:open", None)],
            true, None,
        );
        catalog.commands.clear();
        catalog.completion_index = multi_launcher::completion::build_index(&[], catalog.saved_actions());
        let indexed = ["Alpha", "Bravo", "Charlie", "Delta", "Foxtrot", "Golf"]
            .map(|label| saved_action(label, "hash:open", None));
        catalog.indexed_completion_index = Some(multi_launcher::completion::build_index(&[], &indexed));
        let mut completion = super::SearchCompletion::default();
        completion.reset_for_query(&catalog, "app ");
        assert_eq!(completion.suggestions, ["app alpha", "app bravo", "app charlie", "app delta", "app echo"]);
        catalog.replace_saved_actions(vec![saved_action("Zulu", "hash:open", None)], Vec::new());
        completion.reset_for_query(&catalog, "app ");
        assert_eq!(completion.suggestions, ["app alpha", "app bravo", "app charlie", "app delta", "app foxtrot"]);
    }

    #[test]
    fn indexed_and_saved_identical_rows_keep_origin_after_search_and_replacement() {
        let action = saved_action("Same", "hash:open", None);
        let mut catalog = completion_catalog(vec![action.clone()], true, None);
        let config = multi_launcher::indexer::coordinator::IndexConfig::new(Vec::new(), None);
        let raw_actions = Arc::new(vec![action.clone(), saved_action("Raw only", "C:/data.md", None)]);
        let snapshot = super::indexed_applications::Snapshot {
            config, generation: 7, roots: Vec::new(), actions: vec![action.clone()],
            raw_actions: raw_actions.clone(),
            cached: vec![multi_launcher::actions::search::CachedSearchEntry::from_action(&action)],
            metadata: vec![multi_launcher::common::query::ActionFilterMetadata::from_action(&action)],
            completion_index: multi_launcher::completion::build_index(&[], std::slice::from_ref(&action)),
        };
        let identity = catalog.install_index(snapshot);
        assert!(Arc::ptr_eq(&identity.raw_actions, &raw_actions));
        assert_eq!(identity.raw_actions.len(), 2);
        assert_eq!(catalog.actions.len(), 2);
        for query in ["", "app Same"] {
            let rows = catalog.compose(query, Vec::new(), true);
            let origins = rows.iter().filter(|row| row.action == action).map(|row| row.origin.clone()).collect::<Vec<_>>();
            assert_eq!(origins, vec![super::LauncherResultOrigin::Saved, super::LauncherResultOrigin::Indexed { generation: 7, index: 0 }]);
        }
        let resolver = multi_launcher::universal_actions::ActionTargetResolver;
        let aliases = HashMap::new();
        let saved_context = multi_launcher::universal_actions::ActionTargetResolverContext::new(
            &aliases, &aliases, catalog.saved_actions(),
        );
        let indexed_context = multi_launcher::universal_actions::ActionTargetResolverContext::new(
            &aliases, &aliases, &[],
        );
        assert_eq!(resolver.resolve(&action, &saved_context).custom_action_index, Some(0));
        assert_eq!(resolver.resolve(&action, &indexed_context).custom_action_index, None);
        assert!(catalog.replace_saved_actions(Vec::new(), Vec::new()));
        let rows = catalog.compose("app Same", Vec::new(), true);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].origin, super::LauncherResultOrigin::Indexed { generation: 7, index: 0 });
    }

    #[test]
    fn blank_browse_preserves_command_order_canonical_rows_and_generation() {
        let saved = saved_action("Folder", r"C:\Data\Project", None);
        let unsupported = saved_action("Unsupported", "unsupported:bad", None);
        let mut catalog = completion_catalog(Vec::new(), true, None);
        catalog.commands = vec![
            saved_action("a command", "query:a", None),
            saved_action("b command", "query:b", None),
        ];
        catalog.command_cached_entries = catalog.commands.iter().map(multi_launcher::actions::search::CachedSearchEntry::from_action).collect();
        assert!(catalog.replace_saved_actions(
            vec![saved.clone(), unsupported, saved.clone()], vec![saved.clone()],
        ));
        let initial = catalog.initial_results(None);
        assert_eq!(initial.query, "");
        assert_ne!(initial.generation, 0);
        assert_eq!(initial.actions.len(), 4);
        assert_eq!(initial.actions[0].label(), "a command");
        assert_eq!(initial.actions[1].label(), "b command");
        assert_eq!(initial.actions[2].label(), "app Folder");
        assert_eq!(initial.actions[2].action, saved);
        assert_eq!(initial.actions[3].action, saved);
        assert_eq!(initial.actions[2].origin, super::LauncherResultOrigin::Saved);
        assert!(catalog.verified_folders.contains(&initial.actions[2].action));
        assert!(initial.enter_target("").is_none());
        let whitespace = catalog.compose(" \t", vec![("other".into(), saved.clone())], false);
        assert_eq!(whitespace, initial.actions);
        let mut current = initial;
        let generation = current.generation;
        current.replace("app Folder".into(), catalog.compose("app Folder", Vec::new(), true),
            ResolvedQueryResultsLayout::default(), 2);
        assert!(current.resolve_result(generation, 2, "").is_none());
        assert!(current.actions.iter().all(|row| row.display_label.is_none()));
    }

    #[test]
    fn folder_bookmark_post_filter_preserves_origin_and_raw_provider_precedence() {
        use super::LauncherResultOrigin;
        let catalog = completion_catalog(Vec::new(), true, None);
        let folder = saved_action("Alpha", r"C:\alpha", None);
        let missing = saved_action("Beta", r"C:\beta", None);
        let rows = catalog.compose(
            "f list alpha",
            vec![
                ("folders".into(), folder.clone()),
                ("folders".into(), missing),
            ],
            false,
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].action, folder);
        assert_eq!(
            rows[0].origin,
            LauncherResultOrigin::Provider("folders".into())
        );
        let bookmark = saved_action("Alpha", "https://example.com", None);
        let rows = catalog.compose(
            "bm list alpha",
            vec![("bookmarks".into(), bookmark.clone())],
            false,
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].action, bookmark);
        assert_eq!(
            rows[0].origin,
            LauncherResultOrigin::Provider("bookmarks".into())
        );
        let with_commands = completion_catalog(Vec::new(), true, None);
        assert!(!with_commands.compose("ha", Vec::new(), true).is_empty());
        assert!(with_commands.compose("ha", Vec::new(), false).is_empty());
    }

    #[test]
    fn web_provider_projection_uses_stock_postfilter_and_preserves_literal_url() {
        let enabled = HashSet::from(["reddit".to_owned()]);
        let catalog = completion_catalog(Vec::new(), true, Some(&enabled));
        let query = "ReD Café+Tea";
        let mut raw = multi_launcher::plugin::search_filtered_plugins_with_origin(
            catalog.providers.plugins(), query, Some(&enabled), None,
        );
        assert_eq!(raw.len(), 1);
        assert_eq!(raw[0].1.action, "https://www.reddit.com/search/?q=Caf%C3%A9%2BTea");
        let exact = raw[0].1.clone();
        let mut unmatched = exact.clone();
        unmatched.label = "unrelated".into();
        unmatched.desc = "unrelated".into();
        unmatched.action = "https://example.org/unrelated".into();
        raw.push(("reddit".into(), unmatched));
        let rows = catalog.compose(query, raw, false);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].action, exact);
        assert_eq!(rows[0].origin, super::LauncherResultOrigin::Provider("reddit".into()));
    }

    #[test]
    fn web_provider_filtered_query_obeys_exact_and_fuzzy_postfilter() {
        let enabled = HashSet::from(["wikipedia".to_owned()]);
        let mut catalog = completion_catalog(Vec::new(), true, Some(&enabled));
        let query = "wiki cat kind:https";
        let raw = multi_launcher::plugin::search_filtered_plugins_with_origin(
            catalog.providers.plugins(), query, Some(&enabled), None,
        );
        assert_eq!(raw.len(), 1);
        assert_eq!(raw[0].1.action, "https://en.wikipedia.org/wiki/Special:Search?search=cat");
        assert!(catalog.compose(query, raw.clone(), false).is_empty());
        catalog.match_exact = false;
        let rows = catalog.compose(query, raw.clone(), false);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].action, raw[0].1);
    }

    #[test]
    fn google_route_preserves_bare_cached_command_and_exclusive_projection() {
        let enabled = HashSet::from(["web_search".to_owned()]);
        let mut catalog = completion_catalog(Vec::new(), true, Some(&enabled));
        let cached = catalog.compose("g", Vec::new(), true);
        assert!(cached.iter().any(|row| row.action.action == "query:g "));
        let disabled_catalog = completion_catalog(Vec::new(), true, Some(&HashSet::new()));
        assert!(
            !disabled_catalog
                .compose("g", Vec::new(), true)
                .iter()
                .any(|row| row.action.action == "query:g ")
        );
        let query = "g Café+Tea";
        let rows = super::search_providers::search_routed(
            catalog.providers.plugins(),
            query,
            Some(&HashSet::new()),
            None,
        );
        assert_eq!(rows.len(), 1);
        let composed = catalog.compose(query, rows.clone(), false);
        assert_eq!(composed.len(), 1);
        assert_eq!(composed[0].action, rows[0].1);

        let fallback = saved_action("G Café+Tea", "query:g Café+Tea", None);
        catalog
            .command_cached_entries
            .push(multi_launcher::actions::search::CachedSearchEntry::from_action(&fallback));
        catalog.commands.push(fallback.clone());
        assert!(
            catalog
                .compose("Café+Tea", Vec::new(), true)
                .iter()
                .any(|row| row.action == fallback)
        );
        assert!(catalog.compose("G Café+Tea", Vec::new(), true).is_empty());
    }

    #[test]
    fn unit_convert_shared_route_preserves_scoring_and_base_precedence() {
        let enabled = HashSet::from(["unit_convert".to_owned(), "base_convert".to_owned()]);
        let mut catalog = completion_catalog(Vec::new(), true, Some(&enabled));
        for query in ["CONVERT 1 MB to Mb", "conv 1 month to day", "kind:clipboard conv 1 MB to Mb"] {
            let rows = super::search_providers::search_routed(catalog.providers.plugins(), query, Some(&enabled), None);
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].0, "unit_convert");
            assert!(super::unit_convert_search::is_current(query, &rows[0].1, Some(&enabled), None));
            let projected = catalog.compose(query, rows.clone(), false);
            assert_eq!(projected.len(), 1);
            let mut results = LauncherSearchResults::default();
            results.replace(query.into(), projected, ResolvedQueryResultsLayout::default(), 2);
            let generation = results.generation;
            assert!(results.resolve_result(generation, 0, query).is_some());
            assert!(results.resolve_result(generation, 0, "changed").is_none());
            results.replace("changed".into(), Vec::new(), ResolvedQueryResultsLayout::default(), 2);
            assert!(results.resolve_result(generation, 0, query).is_none());
        }
        let base = super::search_providers::search_routed(catalog.providers.plugins(), "conv 255 dec to hex", Some(&enabled), None);
        assert_eq!(base.len(), 1);
        assert_eq!(base[0].0, "base_convert");
        let no_unit = HashSet::from(["base_convert".to_owned()]);
        assert!(super::search_providers::search_routed(catalog.providers.plugins(), "conv 1 month to day", Some(&no_unit), None).is_empty());
        let query = "conv 3 km to kg";
        let diagnostic = super::search_providers::search_routed(catalog.providers.plugins(), query, Some(&enabled), None);
        assert_eq!(diagnostic.len(), 1);
        assert!(catalog.compose(query, diagnostic.clone(), false).is_empty());
        catalog.match_exact = false;
        let projected = catalog.compose(query, diagnostic.clone(), false);
        assert_eq!(projected.len(), 1);
        assert_eq!(projected[0].action, diagnostic[0].1);
        let disabled = HashMap::from([("unit_convert".to_owned(), Vec::new())]);
        assert!(!super::unit_convert_search::is_current(query, &diagnostic[0].1, Some(&enabled), Some(&disabled)));
    }

    #[test]
    fn base_convert_search_uses_stock_scoring_gates_and_stale_receipts() {
        let enabled = HashSet::from(["base_convert".to_owned()]);
        let catalog = completion_catalog(Vec::new(), true, Some(&enabled));
        for query in ["conv 255 dec to hex", "CONVERT 20436166c3a920 hex to text"] {
            let rows = super::search_providers::search_routed(
                catalog.providers.plugins(), query, Some(&enabled), None,
            );
            assert_eq!(rows.len(), 1);
            assert!(super::base_convert_search::is_current(
                query, &rows[0].1, Some(&enabled), None,
            ));
            let projected = catalog.compose(query, rows.clone(), false);
            assert_eq!(projected.len(), 1);
            assert_eq!(projected[0].action, rows[0].1);
            let mut results = LauncherSearchResults::default();
            results.replace(query.into(), projected, ResolvedQueryResultsLayout::default(), 2);
            let generation = results.generation;
            assert!(results.resolve_result(generation, 0, query).is_some());
            assert!(results.resolve_result(generation, 0, "changed").is_none());
            results.replace("changed".into(), Vec::new(), ResolvedQueryResultsLayout::default(), 2);
            assert!(results.resolve_result(generation, 0, query).is_none());
            let no_search = HashMap::from([("base_convert".to_owned(), Vec::new())]);
            assert!(!super::base_convert_search::is_current(
                query, &rows[0].1, Some(&enabled), Some(&no_search),
            ));
            assert!(!super::base_convert_search::is_current(
                query, &rows[0].1, Some(&HashSet::new()), None,
            ));
        }
        let cached = catalog.compose("conv", Vec::new(), true);
        assert!(cached.iter().any(|row| row.action.action == "query:conv "));
        let cached = catalog.compose("convert", Vec::new(), true);
        assert!(cached.iter().any(|row| row.action.action == "query:convert "));
    }

    #[test]
    fn date_provider_search_projection_obeys_gate_and_exact_bypass() {
        let enabled = HashSet::from(["date_arithmetic".to_owned()]);
        let catalog = completion_catalog(Vec::new(), true, Some(&enabled));
        let query = "date days between 2026-10-05 and 2026-12-25";
        let rows = super::search_providers::search_routed(catalog.providers.plugins(), query, Some(&enabled), None);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1.action, "clipboard:81 days");
        let composed = catalog.compose(query, rows.clone(), false);
        assert_eq!(composed.len(), 1);
        assert_eq!(composed[0].action, rows[0].1);
        let filtered_query = "kind:clipboard date days between 2026-10-05 and 2026-12-25";
        let filtered = super::search_providers::search_routed(catalog.providers.plugins(), filtered_query, Some(&enabled), None);
        assert_eq!(filtered.len(), 1);
        assert!(super::date_search::classify(&filtered[0].0, &filtered[0].1).is_some());
        assert!(!multi_launcher::actions::search::date_resolved_postfilter_bypass(filtered_query, &filtered[0].1.action));
        let projected = catalog.compose(filtered_query, filtered.clone(), false);
        assert_eq!(projected.len(), 1);
        assert_eq!(projected[0].action, filtered[0].1);
        let disabled = HashMap::from([("date_arithmetic".to_owned(), Vec::new())]);
        assert!(super::search_providers::search_routed(catalog.providers.plugins(), query, Some(&enabled), Some(&disabled)).is_empty());
        assert!(super::search_providers::search_routed(catalog.providers.plugins(), query, Some(&HashSet::new()), None).is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn visibility_deactivation_wakes_idle_and_retires_pending_requests() {
        use super::RootVisibility;
        use std::sync::{
            atomic::Ordering,
            mpsc::{TryRecvError, channel},
        };

        let idle = RootVisibility::new();
        let (tx, rx) = channel();
        idle.bind_request_sender(tx);
        assert!(idle.activate());
        let revision = idle.request_revision.load(Ordering::Acquire);
        idle.deactivate();
        assert_eq!(rx.try_recv(), Ok(()));
        idle.deactivate();
        assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
        assert!(!idle.is_current(revision, true));
        assert!(!idle.request_external_hide(revision));
        let gesture = super::hash_dock::Gesture {
            source: super::NativeWindow { identity: super::ForegroundIdentity { hwnd: 1, process_id: 2, thread_id: 3 } },
            foreground_authorized: true,
            visibility_revision: 0,
        };
        assert!(idle.request_hash_visible(gesture, 7).is_none());
        let (late_tx, late_rx) = channel();
        idle.bind_request_sender(late_tx);
        assert!(!idle.activate());
        assert_eq!(late_rx.try_recv(), Err(TryRecvError::Disconnected));

        let pending = RootVisibility::new();
        let (tx, rx) = channel();
        pending.bind_request_sender(tx);
        assert!(pending.activate());
        let acknowledgement = pending.request_hash_visible(gesture, 9).unwrap();
        assert!(!acknowledgement.state.lock().unwrap().closed);
        assert_eq!(rx.try_recv(), Ok(()));
        pending.deactivate();
        assert_eq!(rx.try_recv(), Ok(()));
        assert!(pending.take_requested_visible().is_none());
        assert!(acknowledgement.state.lock().unwrap().closed);
        assert!(!pending.request_external_hide(0));
    }

    #[test]
    fn utility_dock_refuses_modal_ownership_and_requires_delivered_focus() {
        assert!(super::utility_dock_focus_allowed(false, false, false));
        assert!(!super::utility_dock_focus_allowed(true, false, false));
        assert!(!super::utility_dock_focus_allowed(false, true, false));
        assert!(!super::utility_dock_focus_allowed(false, false, true));
        assert!(super::utility_dock_ack_allowed(true, true, true));
        assert!(!super::utility_dock_ack_allowed(true, false, true));
        assert!(!super::utility_dock_ack_allowed(true, true, false));
        assert!(!super::utility_dock_ack_allowed(false, true, true));
    }

    #[cfg(windows)]
    #[test]
    fn xml_dock_reveal_rejects_retired_mount_and_releases_pending_ack() {
        let visibility = super::RootVisibility::new();
        let (tx, _rx) = std::sync::mpsc::channel();
        visibility.bind_request_sender(tx);
        assert!(visibility.activate());
        let gesture = super::hash_dock::Gesture {
            source: super::NativeWindow { identity: super::ForegroundIdentity { hwnd: 1, process_id: 2, thread_id: 3 } },
            foreground_authorized: true,
            visibility_revision: 0,
        };
        let owner = super::utility_mount::MountOwner::default();
        let old = owner.attach();
        let (ack, accepted) = visibility.request_xml_visible(gesture, old.clone()).unwrap();
        let current = owner.attach();
        let request = visibility.take_requested_visible().unwrap();
        assert!(!request.is_current(0, true, false));
        drop(request);
        assert!(ack.state.lock().unwrap().closed);
        assert_eq!(accepted.load(std::sync::atomic::Ordering::Acquire), 0);
        assert!(visibility.request_xml_visible(gesture, old).is_none());
        assert!(visibility.request_xml_visible(gesture, current).is_some());
    }

    #[cfg(windows)]
    #[test]
    fn dock_reveal_queue_acknowledges_handling_and_hide_supersession() {
        let visibility = super::RootVisibility::new();
        let (tx, _rx) = std::sync::mpsc::channel();
        visibility.bind_request_sender(tx);
        assert!(visibility.activate());
        let gesture = super::hash_dock::Gesture {
            source: super::NativeWindow { identity: super::ForegroundIdentity { hwnd: 1, process_id: 2, thread_id: 3 } },
            foreground_authorized: true,
            visibility_revision: 0,
        };
        let acknowledged = visibility.request_hash_visible(gesture, 1).unwrap();
        assert!(!acknowledged.state.lock().unwrap().closed);
        let request = visibility.take_requested_visible().unwrap();
        assert!(!acknowledged.state.lock().unwrap().closed);
        drop(request);
        assert!(acknowledged.state.lock().unwrap().closed);

        let superseded = visibility.request_hash_visible(gesture, 2).unwrap();
        assert!(!superseded.state.lock().unwrap().closed);
        assert!(visibility.request_external_hide(0));
        assert!(superseded.state.lock().unwrap().closed);
        visibility.deactivate();
    }

    #[cfg(windows)]
    #[test]
    fn saved_shortcut_projection_requires_saved_origin_and_blank_args() {
        use super::LauncherResultOrigin;
        let action = saved_action("Shortcut", r"C:\Links\Tool.lnk", Some("  "));
        let catalog = completion_catalog(vec![action.clone()], true, None);
        let rows = catalog.compose(
            "app Shortcut",
            vec![("provider".into(), action.clone())],
            false,
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].origin, LauncherResultOrigin::Saved);
        assert!(rows[0].saved_shortcut().is_some());
        assert!(rows[1].saved_shortcut().is_none());
        let mut invalid = action;
        invalid.args = Some("extra".into());
        assert!(super::external_launch::saved_shortcut(&invalid).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn saved_folder_projection_receipt_tracks_exact_row_and_generation() {
        let action = saved_action("Folder", r"C:\Data\Project", None);
        let mut catalog = completion_catalog(Vec::new(), true, None);
        assert!(catalog.replace_saved_actions(vec![action.clone()], vec![action.clone()]));
        let rows = catalog.compose("app Folder", vec![("other".into(), action.clone())], false);
        let saved_index = rows.iter().position(|row| row.origin == super::LauncherResultOrigin::Saved)
            .expect("saved projection is present");
        assert_eq!(rows[saved_index].action, action);
        let mut results = LauncherSearchResults::default();
        results.replace("app Folder".into(), rows, ResolvedQueryResultsLayout::default(), 2);
        let generation = results.generation;
        assert!(results.resolve_result(generation, saved_index, "app Folder").is_some());
        assert!(results.resolve_result(generation, saved_index, "changed").is_none());
        assert!(catalog.replace_saved_actions(vec![action], Vec::new()));
        results.replace("app Folder".into(), Vec::new(), ResolvedQueryResultsLayout::default(), 2);
        assert!(results.resolve_result(generation, saved_index, "app Folder").is_none());
    }

    #[test]
    fn saved_url_projection_requires_saved_origin_and_current_result() {
        use super::LauncherResultOrigin;
        let action = saved_action("Remote", "https://example.com/a.exe?X=%2f", Some(" "));
        let catalog = completion_catalog(vec![action.clone()], true, None);
        let rows = catalog.compose("app Remote", vec![("provider".into(), action.clone())], false);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].origin, LauncherResultOrigin::Saved);
        assert!(rows[0].saved_url().is_some());
        assert!(rows[0].saved_external().is_none());
        assert!(rows[1].saved_url().is_none());
        let mut results = LauncherSearchResults::default();
        results.replace("app Remote".into(), rows, ResolvedQueryResultsLayout::default(), 2);
        let generation = results.generation;
        assert!(results.resolve_result(generation, 0, "app Remote").unwrap().saved_url().is_some());
        assert!(results.resolve_result(generation, 0, "changed").is_none());
        results.replace("other".into(), Vec::new(), ResolvedQueryResultsLayout::default(), 2);
        assert!(results.resolve_result(generation, 0, "app Remote").is_none());
    }

    #[test]
    fn saved_document_projection_keeps_exact_origin_and_rejects_stale_rows() {
        use super::LauncherResultOrigin;
        let action = saved_action("Manual", r"C:\Docs\Manual.pdf", Some(" "));
        let catalog = completion_catalog(vec![action.clone()], true, None);
        let rows = catalog.compose(
            "app Manual",
            vec![("provider".into(), action.clone())],
            false,
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].origin, LauncherResultOrigin::Saved);
        assert!(rows[0].saved_document().is_some());
        assert!(rows[1].saved_document().is_none());
        let mut results = LauncherSearchResults::default();
        results.replace(
            "app Manual".into(),
            rows,
            ResolvedQueryResultsLayout::default(),
            2,
        );
        let generation = results.generation;
        assert!(
            results
                .resolve_result(generation, 0, "app Manual")
                .unwrap()
                .saved_document()
                .is_some()
        );
        assert!(results.resolve_result(generation, 0, "changed").is_none());
    }

    #[test]
    fn executable_authority_tracks_result_origin_and_rejects_stale_rows() {
        use super::{LauncherResultOrigin, LauncherSearchResult};
        let action = saved_action("Executable", r"C:\Tools\example.exe", None);
        let catalog = completion_catalog(vec![action.clone()], true, None);
        let rows = catalog.compose(
            "app Executable",
            vec![("provider".into(), action.clone())],
            false,
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0],
            LauncherSearchResult {
                action: action.clone(),
                origin: LauncherResultOrigin::Saved,
                display_label: None,
                hide_description: false
            }
        );
        assert_eq!(
            rows[1],
            LauncherSearchResult {
                action: action.clone(),
                origin: LauncherResultOrigin::Provider("provider".into()),
                display_label: None,
                hide_description: false
            }
        );
        let mut results = LauncherSearchResults::default();
        results.replace("app Executable".into(), rows, ResolvedQueryResultsLayout::default(), 2);
        let generation = results.generation;
        assert!(results.resolve_result(generation, 0, "app Executable").unwrap().saved_external().is_some());
        assert!(results.resolve_result(generation, 1, "app Executable").unwrap().saved_external().is_none());
        assert!(results.resolve_result(generation, 0, "changed").is_none());
        results.replace(
            "app Executable".into(),
            vec![LauncherSearchResult {
                action,
                origin: LauncherResultOrigin::Provider("provider".into()),
                display_label: None,
                hide_description: false,
            }],
            ResolvedQueryResultsLayout::default(),
            2,
        );
        assert!(
            results
                .resolve_result(generation, 0, "app Executable")
                .is_none()
        );
    }

    #[test]
    fn saved_action_projection_changes_only_for_supported_ordered_records() {
        let action = |label: &str, command: &str| multi_launcher::actions::Action {
            label: label.into(),
            desc: String::new(),
            action: command.into(),
            args: None,
        };
        let first = action("Saved Alpha", "hash:open");
        let second = action("Saved Beta", "hash:open");
        let unsupported = action("Ordinary App", "https://example.com");
        let mut catalog = completion_catalog(vec![first.clone()], true, None);
        let query = "app saved";
        let mut completion = SearchCompletion::default();
        completion.reset_for_query(&catalog, query);
        assert!(!catalog.replace_saved_actions(vec![unsupported.clone(), first.clone()], Vec::new()));
        assert!(catalog.replace_saved_actions(vec![second.clone(), unsupported, first.clone()], Vec::new()));
        assert_eq!(catalog.actions, vec![second.clone(), first.clone()]);
        let mut results = LauncherSearchResults::default();
        results.replace_test_actions(
            query.into(),
            catalog.compose_actions(query, Vec::new()),
            ResolvedQueryResultsLayout::default(),
            catalog.page_jump,
        );
        assert_eq!(results.actions.len(), 2);
        assert_eq!(results.query, query);
        completion.reset_for_query(&catalog, query);
        assert!(
            completion
                .suggestions
                .iter()
                .any(|value| value.contains("saved beta"))
        );
        assert!(!catalog.replace_saved_actions(vec![second, first], Vec::new()));
    }

    #[cfg(windows)]
    #[test]
    fn downloads_provider_aliases_obey_search_gates() {
        let providers = super::InstalledLauncherProviders::test_without_persisted_providers();
        let enabled = HashSet::from(["downloads_triage".to_owned()]);
        let search = HashMap::from([("downloads_triage".to_owned(), vec!["search".to_owned()])]);
        for query in ["downloads", " DOWNLOADS TRIAGE ", "Downloads Triage Center"] {
            let rows = multi_launcher::plugin::search_filtered_plugins_with_origin(providers.plugins(), query, Some(&enabled), Some(&search));
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].0, "downloads_triage");
            assert!(super::is_downloads_triage_opener(&rows[0].1));
        }
        assert!(multi_launcher::plugin::search_filtered_plugins_with_origin(providers.plugins(), "downloads", Some(&HashSet::new()), Some(&search)).is_empty());
        let no_search = HashMap::from([("downloads_triage".to_owned(), Vec::new())]);
        assert!(multi_launcher::plugin::search_filtered_plugins_with_origin(providers.plugins(), "downloads", Some(&enabled), Some(&no_search)).is_empty());
        let mut action = saved_action("Downloads", "downloads_triage:open", None);
        action.args = Some("unexpected".to_owned());
        assert!(!super::is_downloads_triage_opener(&action));
    }

    #[test]
    fn file_search_provider_admits_only_enabled_bare_opener() {
        let providers = super::InstalledLauncherProviders::test_without_persisted_providers();
        let enabled = HashSet::from(["file_search".to_owned()]);
        let search = HashMap::from([("file_search".to_owned(), vec!["search".to_owned()])]);
        let rows = multi_launcher::plugin::search_filtered_plugins_with_origin(providers.plugins(), "FS", Some(&enabled), Some(&search));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "file_search");
        assert!(super::is_file_search_opener("FS", &rows[0].1));
        assert!(multi_launcher::plugin::search_filtered_plugins_with_origin(providers.plugins(), "fs file", Some(&enabled), Some(&search)).is_empty());
        assert!(multi_launcher::plugin::search_filtered_plugins_with_origin(providers.plugins(), "fs", Some(&HashSet::new()), Some(&search)).is_empty());
        let disabled_caps = HashMap::from([("file_search".to_owned(), Vec::new())]);
        assert!(multi_launcher::plugin::search_filtered_plugins_with_origin(providers.plugins(), "fs", Some(&enabled), Some(&disabled_caps)).is_empty());
    }

    #[test]
    fn direct_inherited_hash_commit_requires_all_receipts_and_activation() {
        use super::direct_inherited_hash_commit_current as current;
        assert!(current(true, true, true, true, true, true, true));
        assert!(super::hash_popup_visibility_eligible(true, false));
        assert!(!super::hash_popup_visibility_eligible(true, true));
        assert!(!super::hash_popup_visibility_eligible(false, false));
        assert!(!current(false, true, true, true, true, true, true));
        assert!(!current(true, false, true, true, true, true, true));
        assert!(!current(true, true, false, true, true, true, true));
        assert!(!current(true, true, true, false, true, true, true));
        assert!(!current(true, true, true, true, false, true, true));
        assert!(!current(true, true, true, true, true, false, true));
        assert!(!current(true, true, true, true, true, true, false));
    }

    #[test]
    fn inherited_hash_restore_checks_origin_before_and_after_show() {
        use super::prepare_direct_hash_popup as prepare;
        use super::restore_direct_hash_popup as restore;
        use std::cell::{Cell, RefCell};
        let calls = RefCell::new(Vec::new());
        let authorized = Cell::new(true);
        assert!(prepare(false, Some((true, true)),
            || { calls.borrow_mut().push("current"); authorized.get() },
            || { calls.borrow_mut().push("restore"); true },
            || { calls.borrow_mut().push("visible"); true }));
        assert_eq!(&*calls.borrow(), &["current", "restore", "visible", "current"]);
        calls.borrow_mut().clear();
        assert!(!prepare(false, Some((false, true)),
            || { calls.borrow_mut().push("current"); true },
            || { calls.borrow_mut().push("restore"); true }, || true));
        assert!(calls.borrow().is_empty());
        assert!(!prepare(false, Some((true, true)), || true, || false, || true));
        let count = Cell::new(0);
        assert!(!prepare(false, Some((true, true)),
            || { count.set(count.get() + 1); count.get() == 1 },
            || true, || true));
        assert_eq!(count.get(), 2);
        let called = Cell::new(false);
        assert!(prepare(false, Some((true, false)), || true, || { called.set(true); true }, || true));
        assert!(!called.get(), "a visible, unminimized window does not need restoration");
        assert!(restore(false, Some((true, false)), || { called.set(true); true }));
        assert!(!called.get(), "a window restored since sampling is not shown again");
        assert!(!restore(false, Some((false, true)), || { called.set(true); true }));
        assert!(!called.get(), "a hidden window is never restored");
    }

    #[test]
    fn inherited_hash_commit_keeps_source_on_refusal_and_same_path() {
        assert!(super::inherited_hash_parent_return_allowed(false, false));
        assert!(!super::inherited_hash_parent_return_allowed(true, false));
        assert!(!super::inherited_hash_parent_return_allowed(false, true));
        let first = PathBuf::from(r"C:\owned\first.txt");
        let second = PathBuf::from(r"C:\owned\second.txt");
        let mut session = super::HashSession::new();
        session.begin(Some(first.clone()));
        let generation = session.generation();
        assert!(!super::commit_inherited_hash_file(&mut session, &second, false));
        assert_eq!(session.file_path(), Some(first.as_path()));
        assert_eq!(session.generation(), generation);
        assert!(super::commit_inherited_hash_file(&mut session, &first, true));
        assert_eq!(session.generation(), generation);
        assert!(super::commit_inherited_hash_file(&mut session, &second, true));
        assert_eq!(session.file_path(), Some(second.as_path()));
        assert_ne!(session.generation(), generation);
        assert!(!session.has_job());
    }

    #[test]
    fn file_handoff_preserves_same_hash_file_and_invalidates_changed_file() {
        let first = PathBuf::from(r"C:\first\same.txt");
        let second = PathBuf::from(r"C:\second\same.txt");
        let mut session = super::HashSession::new();
        session.begin(Some(first.clone()));
        assert_eq!(session.mode(), super::HashInputMode::File);
        let generation = session.generation();
        session.set_file_path(Some(first.clone()));
        assert_eq!(session.generation(), generation);
        assert_eq!(session.file_path(), Some(first.as_path()));
        session.set_file_path(Some(second.clone()));
        assert_ne!(session.generation(), generation);
        assert_eq!(session.file_path(), Some(second.as_path()));
        assert!(!session.has_job());
    }

    #[test]
    fn file_search_hash_requires_exact_file_row_and_current_page() {
        use super::file_search::{current_row, eligible_file_row};
        use multi_launcher::file_search::model::{FileKind, FilenameRank, SearchId};
        let path = PathBuf::from(r"C:\data\first\same.txt");
        let mut row = multi_launcher::file_search::model::FilenameResult {
            path: path.clone(), file_name: "same.txt".into(), parent_directory: None,
            kind: FileKind::File, size: None, modified: None, rank: FilenameRank::ExactFilename,
            match_quality: FilenameRank::ExactFilename, filename_match_ranges: Vec::new(),
            path_match_ranges: Vec::new(), arrival_index: 0,
        };
        assert!(eligible_file_row(&row, &path));
        assert!(!eligible_file_row(&row, &PathBuf::from(r"C:\data\second\same.txt")));
        assert!(current_row(true, 3, Some(SearchId(7)), 3, Some(SearchId(7)), 1, 0..2));
        assert!(!current_row(true, 4, Some(SearchId(7)), 3, Some(SearchId(7)), 1, 0..2));
        assert!(!current_row(true, 3, Some(SearchId(8)), 3, Some(SearchId(7)), 1, 0..2));
        assert!(!current_row(true, 3, Some(SearchId(7)), 3, Some(SearchId(7)), 1, 2..4));
        row.kind = FileKind::Directory;
        assert!(!eligible_file_row(&row, &path));
    }

    #[test]
    fn file_search_opener_is_exact_and_current_rows_are_generation_bound() {
        use super::{file_search::current_row, is_file_search_opener};
        use multi_launcher::file_search::model::SearchId;
        let action = saved_action("Open file search", "file_search:open", None);
        assert!(is_file_search_opener(" FS ", &action));
        assert!(!is_file_search_opener("fs file", &action));
        let mut with_args = action.clone(); with_args.args = Some(String::new());
        assert!(!is_file_search_opener("fs", &with_args));
        assert!(current_row(true, 4, Some(SearchId(7)), 4, Some(SearchId(7)), 1, 0..2));
        assert!(!current_row(true, 5, Some(SearchId(7)), 4, Some(SearchId(7)), 1, 0..2));
        assert!(!current_row(true, 4, Some(SearchId(8)), 4, Some(SearchId(7)), 1, 0..2));
        assert!(!current_row(false, 4, Some(SearchId(7)), 4, Some(SearchId(7)), 1, 0..2));
        assert!(!current_row(true, 4, Some(SearchId(7)), 4, Some(SearchId(7)), 1, 100..102));
    }

    #[test]
    fn saved_executables_are_admitted_without_opening_the_provider_gate() {
        let action = multi_launcher::actions::Action {
            label: "Saved Tool".into(),
            desc: String::new(),
            action: r"C:\Tools\tool.exe".into(),
            args: Some("--exact".into()),
        };
        let catalog = completion_catalog(vec![action.clone()], true, None);
        assert_eq!(catalog.actions, vec![action.clone()]);
        assert!(supported_launcher_search_actions(vec![action]).is_empty());
    }

    #[test]
    fn gpui_grid_policy_uses_startup_settings_and_installed_hash_metadata() {
        let enabled = HashSet::from(["hash_generator".to_owned()]);
        let catalog = LauncherSearchCatalog::new(
            LauncherSearchStartup {
                branding: super::launcher_header::Branding::default(),
                verified_folders: Vec::new(),
                index_config: multi_launcher::indexer::coordinator::IndexConfig::new(Vec::new(), None),
                actions: Vec::new(),
                calculator_settings: None,
                match_exact: true,
                fuzzy_weight: 0.75,
                usage_weight: 0.0,
                query_autocomplete: false,
                show_examples: false,
                page_jump: 2,
                query_results_layout: QueryResultsLayoutSettings {
                    enabled: true,
                    cols: 3,
                    ..QueryResultsLayoutSettings::default()
                },
                jump_mode_enabled: true,
                jump_mode_activation_chord: JumpModeChord::default(),
                jump_mode_hint_alphabet: HintAlphabet::default(),
                usage: HashMap::new(),
                folder_aliases_lc: HashMap::new(),
                bookmark_aliases_lc: HashMap::new(),
            },
            Some(&enabled),
        );
        assert!(!catalog.resolve_result_layout("hash", Some(&enabled)).grid);
        assert!(catalog.resolve_result_layout("app QMarker", Some(&enabled)).grid);
        assert_eq!(
            catalog
                .resolve_result_layout("app QMarker", Some(&enabled))
                .columns,
            3
        );
        assert_eq!(catalog.page_jump, 2);
        assert!(
            catalog
                .resolve_result_layout("hash", Some(&HashSet::new()))
                .grid
        );

        let globally_gridded = LauncherSearchCatalog::new(
            LauncherSearchStartup {
                branding: super::launcher_header::Branding::default(),
                verified_folders: Vec::new(),
                index_config: multi_launcher::indexer::coordinator::IndexConfig::new(Vec::new(), None),
                actions: Vec::new(),
                calculator_settings: None,
                match_exact: true,
                fuzzy_weight: 0.75,
                usage_weight: 0.0,
                query_autocomplete: false,
                show_examples: false,
                page_jump: 4,
                query_results_layout: QueryResultsLayoutSettings {
                    enabled: true,
                    cols: 2,
                    respect_plugin_capability: false,
                    ..QueryResultsLayoutSettings::default()
                },
                jump_mode_enabled: true,
                jump_mode_activation_chord: JumpModeChord::default(),
                jump_mode_hint_alphabet: HintAlphabet::default(),
                usage: HashMap::new(),
                folder_aliases_lc: HashMap::new(),
                bookmark_aliases_lc: HashMap::new(),
            },
            Some(&enabled),
        );
        assert!(
            globally_gridded
                .resolve_result_layout("hash", Some(&enabled))
                .grid
        );
        assert_eq!(globally_gridded.page_jump, 4);
        assert!(globally_gridded.resolve_result_layout("", Some(&enabled)).grid);
    }

    fn saved_hash_action(label: &str) -> multi_launcher::actions::Action {
        multi_launcher::actions::Action {
            label: label.into(),
            desc: "Hash generator fixture".into(),
            action: "hash:open".into(),
            args: None,
        }
    }

    fn saved_action(
        label: &str,
        action: &str,
        args: Option<&str>,
    ) -> multi_launcher::actions::Action {
        multi_launcher::actions::Action {
            label: label.into(),
            desc: "Saved launcher fixture".into(),
            action: action.into(),
            args: args.map(str::to_owned),
        }
    }

    #[test]
    fn installed_search_routes_keep_exact_handler_query_set_output() {
        let cases = [
            (
                saved_action("Literal", "query:  literal  東京  ", None),
                "  literal  東京  ",
            ),
            (
                saved_action(
                    "Argument",
                    "query:g05 outcome",
                    Some(r#"{"query":"  second β  "}"#),
                ),
                "g05 outcome   second β  ",
            ),
            (saved_action("Empty", "query:", None), ""),
            (
                saved_action("Empty argument", "query:", Some(r#"{"query":""}"#)),
                " ",
            ),
            (
                saved_action(
                    "Invalid JSON fallback",
                    "query:plain text",
                    Some("not JSON"),
                ),
                "plain text",
            ),
        ];

        for (action, expected_query) in cases {
            for source in [ActivationSource::Enter, ActivationSource::Click] {
                let InstalledSearchInvocation { invocation, route } =
                    parse_installed_search_invocation(action.clone(), source)
                        .expect("the shared Query Set handler outcome is representable");
                assert_eq!(invocation.original_action, action);
                assert_eq!(invocation.query_override, None);
                assert_eq!(invocation.source, source);
                let InstalledSearchRoute::QuerySet(outcome) = route else {
                    panic!("saved Query Set action must retain the Query route");
                };
                assert_eq!(query_set_outcome_text(&outcome), Some(expected_query));
            }
        }

        for action in [
            saved_action("Raw LF", "query:line\nbreak", None),
            saved_action("Raw CR", "query:line\rbreak", None),
            saved_action(
                "Decoded LF",
                "query:prefix",
                Some(r#"{"query":"line\nbreak"}"#),
            ),
            saved_action("Execute First", "queryexec:hash", None),
            saved_action("Uninstalled tool", "mouse:settings", None),
            saved_action("External", "launch-external", None),
        ] {
            assert!(
                parse_installed_search_invocation(action, ActivationSource::Click).is_err(),
                "unsupported or multiline action must not enter the installed catalog"
            );
        }

        let hash = parse_installed_search_invocation(
            saved_hash_action("Hash Open"),
            ActivationSource::Click,
        )
        .expect("Hash Open remains installed");
        assert!(matches!(hash.route, InstalledSearchRoute::HashOpen));
    }

    #[test]
    fn bare_settings_route_and_completion_fallback_follow_installed_search_capability() {
        let plugin = multi_launcher::plugins::settings::SettingsPlugin;
        let provider_action = plugin
            .search("settings")
            .into_iter()
            .next()
            .expect("the installed Settings search provider returns its bare route");
        let invocation = parse_installed_search_invocation(
            provider_action.clone(),
            ActivationSource::Enter,
        )
        .expect("the bare typed command is an installed route");
        assert_eq!(
            invocation.invocation.command,
            Command::Dialog(DialogCommand::Settings)
        );
        assert!(matches!(invocation.route, InstalledSearchRoute::Settings));
        let suffix_action = plugin.search("settings font").remove(0);
        assert!(
            parse_installed_search_invocation(suffix_action, ActivationSource::Enter).is_err(),
            "the later Settings search suffix route is not admitted in this checkpoint"
        );

        let settings_action = saved_action("Open Settings", "settings:dialog", None);
        let startup = || LauncherSearchStartup {
            branding: super::launcher_header::Branding::default(),
                verified_folders: Vec::new(),
                index_config: multi_launcher::indexer::coordinator::IndexConfig::new(Vec::new(), None),
            calculator_settings: None,
            actions: vec![settings_action.clone()],
            match_exact: true,
            fuzzy_weight: 0.25,
            usage_weight: 0.0,
            query_autocomplete: true,
            show_examples: false,
            page_jump: 2,
            query_results_layout: QueryResultsLayoutSettings::default(),
            jump_mode_enabled: false,
            jump_mode_activation_chord: JumpModeChord::default(),
            jump_mode_hint_alphabet: HintAlphabet::default(),
            usage: HashMap::new(),
            folder_aliases_lc: HashMap::new(),
            bookmark_aliases_lc: HashMap::new(),
        };
        let enabled = HashSet::from(["settings".to_owned()]);
        let search_enabled = HashMap::from([("settings".to_owned(), vec!["search".to_owned()])]);
        let enabled_catalog = LauncherSearchCatalog::new(startup(), Some(&enabled));
        assert_eq!(enabled_catalog.actions, vec![settings_action.clone()]);
        assert!(enabled_catalog.commands.iter().any(|action| {
            parse_installed_search_invocation(action.clone(), ActivationSource::Enter)
                .is_ok_and(|invocation| matches!(invocation.route, InstalledSearchRoute::Settings))
        }));
        let composed = enabled_catalog.compose_actions("settings", Vec::new());
        assert_eq!(composed.len(), 1);
        assert!(matches!(
            parse_installed_search_invocation(composed[0].clone(), ActivationSource::Enter)
                .expect("the admitted Settings command composes into Search")
                .route,
            InstalledSearchRoute::Settings
        ));

        let provider = multi_launcher::plugin::search_filtered_plugins(
            [&plugin as &dyn multi_launcher::plugin::Plugin],
            "settings",
            Some(&enabled),
            Some(&search_enabled),
        );
        assert_eq!(provider, vec![provider_action]);

        let mut fallback_startup = startup();
        fallback_startup.actions.clear();
        let fallback_catalog = LauncherSearchCatalog::new(fallback_startup, Some(&enabled));
        let fallback = fallback_catalog.compose_actions("settings", Vec::new());
        assert_eq!(fallback.len(), 1, "the admitted bare command is a fallback");
        assert!(matches!(
            parse_installed_search_invocation(fallback[0].clone(), ActivationSource::Enter)
                .expect("fallback retains the typed Settings route")
                .route,
            InstalledSearchRoute::Settings
        ));

        let suffix_provider = supported_launcher_search_actions(
            multi_launcher::plugin::search_filtered_plugins(
                [&plugin as &dyn multi_launcher::plugin::Plugin],
                "settings font",
                Some(&enabled),
                Some(&search_enabled),
            ),
        );
        assert!(suffix_provider.is_empty());
        assert!(
            enabled_catalog
                .compose_actions("settings font", suffix_provider)
                .is_empty(),
            "unsupported typed Settings search does not leak through command fallback"
        );

        let capability_disabled =
            HashMap::from([("settings".to_owned(), Vec::<String>::new())]);
        let capability_disabled_catalog =
            LauncherSearchCatalog::new(startup(), Some(&enabled));
        assert_eq!(
            capability_disabled_catalog.actions,
            vec![settings_action.clone()],
            "explicit saved actions use local-action admission independent of Search capability"
        );
        assert!(capability_disabled_catalog.commands.iter().any(|action| {
            parse_installed_search_invocation(action.clone(), ActivationSource::Enter)
                .is_ok_and(|invocation| matches!(invocation.route, InstalledSearchRoute::Settings))
        }));
        let provider = multi_launcher::plugin::search_filtered_plugins(
            [&plugin as &dyn multi_launcher::plugin::Plugin],
            "settings",
            Some(&enabled),
            Some(&capability_disabled),
        );
        assert!(provider.is_empty(), "Search capability gates provider rows");
        let capability_fallback = capability_disabled_catalog.compose_actions("settings", Vec::new());
        assert_eq!(capability_fallback.len(), 1);
        assert!(matches!(
            parse_installed_search_invocation(
                capability_fallback[0].clone(),
                ActivationSource::Enter
            )
            .expect("enabled plugin command remains in fallback")
            .route,
            InstalledSearchRoute::Settings
        ));

        let disabled = HashSet::new();
        let disabled_catalog = LauncherSearchCatalog::new(startup(), Some(&disabled));
        assert_eq!(
            disabled_catalog.actions,
            vec![settings_action],
            "explicit saved typed actions remain available through shared local Search"
        );
        assert!(disabled_catalog.commands.is_empty());
        assert!(disabled_catalog.compose_actions("settings", Vec::new()).is_empty());
        let saved = disabled_catalog.compose_actions("app settings", Vec::new());
        assert_eq!(saved.len(), 1);
        assert!(matches!(
            parse_installed_search_invocation(saved[0].clone(), ActivationSource::Enter)
                .expect("explicit saved action remains an installed route")
                .route,
            InstalledSearchRoute::Settings
        ));
    }

    #[test]
    fn gpui_favorites_browse_admission_and_provider_gates() {
        let providers = super::InstalledLauncherProviders::test_without_persisted_providers();
        let enabled = HashSet::from(["favorites".to_owned()]);
        let search = HashMap::from([("favorites".to_owned(), vec!["search".to_owned()])]);
        for query in ["fav", " FAV ADD "] {
            let rows = multi_launcher::plugin::search_filtered_plugins(
                providers.plugins(), query, Some(&enabled), Some(&search),
            );
            assert_eq!(rows.len(), 1);
            assert!(matches!(
                parse_installed_search_invocation(rows[0].clone(), ActivationSource::Enter)
                    .unwrap().route,
                InstalledSearchRoute::FavoritesBrowse
            ));
        }
        let named = multi_launcher::plugin::search_filtered_plugins(
            providers.plugins(), "FAV ADD Mixed Case", Some(&enabled), Some(&search),
        );
        assert_eq!(named.len(), 1);
        assert!(matches!(
            parse_installed_search_invocation(named[0].clone(), ActivationSource::Enter).unwrap().route,
            InstalledSearchRoute::FavoritesNamed(ref label) if label == "Mixed Case"
        ));
        assert!(multi_launcher::plugin::search_filtered_plugins(
            providers.plugins(), "fav addx", Some(&enabled), Some(&search),
        ).is_empty());
        assert!(multi_launcher::plugin::search_filtered_plugins(
            providers.plugins(), "fav", Some(&HashSet::new()), Some(&search),
        ).is_empty());
        let no_search = HashMap::from([("favorites".to_owned(), Vec::<String>::new())]);
        assert!(multi_launcher::plugin::search_filtered_plugins(
            providers.plugins(), "fav", Some(&enabled), Some(&no_search),
        ).is_empty());
        let commands = providers.cached_commands(Some(&enabled));
        assert_eq!(commands.iter().map(|command| command.action.as_str()).collect::<Vec<_>>(),
            vec!["query:fav ", "query:fav add ", "query:fav list"]);
        assert!(providers.cached_commands(Some(&HashSet::new())).is_empty());
        for (command, admitted) in [("fav:dialog:", true), ("fav:dialog:name", true)] {
            let action = saved_action("saved favorite", command, None);
            assert_eq!(
                parse_installed_search_invocation(action, ActivationSource::Enter).is_ok(),
                admitted
            );
        }
    }

    #[test]
    fn gpui_provider_collection_drives_search_cache_and_help_without_search_gates() {
        let providers = super::InstalledLauncherProviders::test_without_persisted_providers();
        let enabled = HashSet::from([
            "hash_generator".to_owned(),
            "settings".to_owned(),
            "help".to_owned(),
        ]);
        let capabilities = enabled
            .iter()
            .map(|name| (name.clone(), vec!["search".to_owned()]))
            .collect::<HashMap<_, _>>();
        let help_results = multi_launcher::plugin::search_filtered_plugins(
            providers.plugins(),
            "help",
            Some(&enabled),
            Some(&capabilities),
        );
        assert_eq!(help_results.len(), 1);
        assert!(matches!(
            parse_installed_search_invocation(help_results[0].clone(), ActivationSource::Enter)
                .expect("the installed Help provider uses typed activation")
                .route,
            InstalledSearchRoute::Help
        ));

        let help_capability_disabled = HashMap::from([
            ("hash_generator".to_owned(), vec!["search".to_owned()]),
            ("settings".to_owned(), vec!["search".to_owned()]),
            ("help".to_owned(), Vec::<String>::new()),
        ]);
        assert!(multi_launcher::plugin::search_filtered_plugins(
            providers.plugins(),
            "help",
            Some(&enabled),
            Some(&help_capability_disabled),
        )
        .is_empty());

        let catalog = LauncherSearchCatalog::new(
            LauncherSearchStartup {
                branding: super::launcher_header::Branding::default(),
                verified_folders: Vec::new(),
                index_config: multi_launcher::indexer::coordinator::IndexConfig::new(Vec::new(), None),
                actions: Vec::new(),
                calculator_settings: None,
                match_exact: true,
                fuzzy_weight: 0.75,
                usage_weight: 0.0,
                query_autocomplete: false,
                show_examples: true,
                page_jump: 2,
                query_results_layout: QueryResultsLayoutSettings::default(),
                jump_mode_enabled: false,
                jump_mode_activation_chord: JumpModeChord::default(),
                jump_mode_hint_alphabet: HintAlphabet::default(),
                usage: HashMap::new(),
                folder_aliases_lc: HashMap::new(),
                bookmark_aliases_lc: HashMap::new(),
            },
            Some(&enabled),
        );
        assert!(catalog.startup_show_examples);
        let cached_actions = catalog
            .commands
            .iter()
            .map(|action| action.action.as_str())
            .collect::<HashSet<_>>();
        assert!(cached_actions.contains("query:hash"));
        assert!(cached_actions.contains("settings:dialog"));
        assert!(cached_actions.contains("query:help"));
        assert!(!cached_actions.contains("dashboard:settings"));
        assert!(!cached_actions.contains("theme:dialog"));

        let help_names = catalog
            .command_help_providers
            .iter()
            .map(|provider| provider.name.as_str())
            .collect::<Vec<_>>();
        #[cfg(not(windows))]
        assert_eq!(help_names, ["activity", "base_convert", "calculator", "date_arithmetic", "favorites", "file_search", "folder_size", "hash_generator", "help", "json_utility", "reddit", "regex_tester", "runescape_search", "settings", "unit_convert", "weather", "web_search", "wikipedia", "xml_toolkit", "youtube"]);
        #[cfg(windows)]
        assert_eq!(help_names, ["activity", "base_convert", "calculator", "date_arithmetic", "downloads_triage", "favorites", "file_lock", "file_search", "folder_size", "hash_generator", "help", "json_utility", "reddit", "regex_tester", "runescape_search", "settings", "unit_convert", "weather", "web_search", "wikipedia", "windows", "xml_toolkit", "youtube"]);
        let settings_help = catalog
            .command_help_providers
            .iter()
            .find(|provider| provider.name == "settings")
            .expect("installed Settings appears in Help regardless of search gating");
        assert_eq!(settings_help.commands.len(), 1);
        assert_eq!(settings_help.commands[0].action, "settings:dialog");

        let disabled_catalog = LauncherSearchCatalog::new(
            LauncherSearchStartup {
                branding: super::launcher_header::Branding::default(),
                verified_folders: Vec::new(),
                index_config: multi_launcher::indexer::coordinator::IndexConfig::new(Vec::new(), None),
                calculator_settings: None,
                actions: vec![saved_action("Saved Help", "help:show", None)],
                match_exact: true,
                fuzzy_weight: 0.75,
                usage_weight: 0.0,
                query_autocomplete: false,
                show_examples: false,
                page_jump: 2,
                query_results_layout: QueryResultsLayoutSettings::default(),
                jump_mode_enabled: false,
                jump_mode_activation_chord: JumpModeChord::default(),
                jump_mode_hint_alphabet: HintAlphabet::default(),
                usage: HashMap::new(),
                folder_aliases_lc: HashMap::new(),
                bookmark_aliases_lc: HashMap::new(),
            },
            Some(&HashSet::new()),
        );
        assert!(disabled_catalog.commands.is_empty());
        assert_eq!(disabled_catalog.command_help_providers.len(), if cfg!(windows) { 20 } else { 17 });
        assert_eq!(disabled_catalog.actions.len(), 1);
        assert!(matches!(
            parse_installed_search_invocation(
                disabled_catalog.actions[0].clone(),
                ActivationSource::Enter
            )
            .expect("saved typed Help follows local-action admission")
            .route,
            InstalledSearchRoute::Help
        ));
    }

    #[test]
    fn command_help_show_examples_publishes_only_acknowledged_values_and_allows_retry() {
        let mut help = CommandHelpState::new(false);
        assert!(help.begin_save(true).is_none(), "a closed panel cannot write");
        help.open = true;
        let failed_attempt = help.begin_save(true).expect("open panel starts a preference save");
        assert!(!help.show_examples, "the draft checkbox does not publish before the receipt");
        assert!(!help.cancel_idle(), "a pending write retains the panel");
        assert_eq!(
            help.finish_save(
                failed_attempt.id + 1,
                Err(multi_launcher::settings::SettingsBooleanSaveError::Store(
                    "stale receipt".to_owned()
                )),
            ),
            None,
            "an obsolete acknowledgement cannot release the active save"
        );
        assert!(help.pending.is_some());
        assert_eq!(
            help.finish_save(
                failed_attempt.id,
                Err(multi_launcher::settings::SettingsBooleanSaveError::Store(
                    "write failed".to_owned()
                )),
            ),
            None
        );
        assert!(!help.show_examples);
        assert!(help.pending.is_none());
        assert!(help.open);
        assert!(help.status.as_deref().unwrap().contains("write failed"));

        let retry = help.begin_save(true).expect("a failed value can be retried");
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        std::fs::write(&path, br#"{"show_examples":false}"#).unwrap();
        let acknowledgement = multi_launcher::settings::Settings::save_boolean_preference_checked(
            &path,
            multi_launcher::settings::SettingsBooleanSaveRequest::new(
                multi_launcher::settings::SettingsBooleanPreference::ShowExamples,
                retry.expected_baseline,
                retry.desired,
            ),
        )
        .unwrap();
        assert_eq!(help.finish_save(retry.id, Ok(acknowledgement)), Some(true));
        assert!(help.show_examples);
        assert_eq!(help.status.as_deref(), Some("Show examples saved."));
        assert!(help.pending.is_none());
        assert!(help.cancel_idle());
        assert!(!help.open);
    }

    #[test]
    fn command_help_body_height_stays_within_the_available_viewport() {
        assert_eq!(command_help_viewport_body_height(100.0, 140.0), 0.0);
        assert_eq!(command_help_viewport_body_height(260.0, 140.0), 120.0);
        assert_eq!(command_help_viewport_body_height(900.0, 140.0), 280.0);
    }

    #[test]
    fn command_help_rejects_acknowledgement_for_query_autocomplete() {
        let mut help = CommandHelpState::new(false);
        help.open = true;
        let attempt = help.begin_save(true).expect("open panel admits the save");
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        std::fs::write(&path, br#"{"query_autocomplete":false}"#).unwrap();
        let acknowledgement = multi_launcher::settings::Settings::save_boolean_preference_checked(
            &path,
            multi_launcher::settings::SettingsBooleanSaveRequest::new(
                multi_launcher::settings::SettingsBooleanPreference::QueryAutocomplete,
                false,
                true,
            ),
        )
        .unwrap();

        assert_eq!(help.finish_save(attempt.id, Ok(acknowledgement)), None);
        assert!(!help.show_examples);
        assert!(help.open);
        assert!(help.pending.is_none());
        assert!(help.status.as_deref().unwrap().contains("QueryAutocomplete"));
    }

    #[test]
    fn linking_guide_keeps_the_egui_filter_seed() {
        assert_eq!(super::LINKING_GUIDE_FILTER, "todo note cal @note: @todo:");
    }

    #[test]
    fn context_favorite_receipt_follows_unselected_row_in_list_and_grid() {
        use super::{ActionSurface, ContextFavoriteReceipt, UniversalActionInvocationContext};

        for layout in [ResolvedQueryResultsLayout::new(false, 1), ResolvedQueryResultsLayout::new(true, 3)] {
            let first = hash_action();
            let mut clicked = first.clone();
            clicked.label = "Right-clicked fixture".into();
            clicked.args = Some("complete action".into());
            let mut results = LauncherSearchResults::default();
            results.replace_test_actions("hash".into(), vec![first.clone(), clicked.clone()], layout, 2);
            results.selected = Some(0);
            let receipt = ContextFavoriteReceipt {
                serial: 1,
                generation: results.generation,
                row: 1,
                invocation: UniversalActionInvocationContext {
                    history_query: "hash".into(),
                    ..UniversalActionInvocationContext::legacy(ActionSurface::ContextMenu, ActivationSource::Click)
                },
                action: clicked.clone(),
                visibility_revision: 4,
                previous_focus: None,
            };
            assert!(receipt.matches(&results, "hash", 4));
            assert_eq!(results.selected, Some(0));
            assert!(!receipt.matches(&results, "other", 4));
            assert!(!receipt.matches(&results, "hash", 5));
            results.actions[1].action.args = Some("changed".into());
            assert!(!receipt.matches(&results, "hash", 4));
            results.replace_test_actions("hash".into(), vec![first, clicked], layout, 2);
            assert!(!receipt.matches(&results, "hash", 4));
        }
    }

    #[test]
    fn launcher_results_follow_egui_enter_selection_and_reject_stale_rows() {
        let first = hash_action();
        let second = multi_launcher::actions::Action {
            label: "Second installed route".into(),
            desc: "Fixture row".into(),
            action: "hash:open".into(),
            args: Some("beta argument".into()),
        };
        let mut results = LauncherSearchResults::default();
        results.replace_test_actions(
            "hash".into(),
            vec![first.clone()],
            ResolvedQueryResultsLayout::default(),
            2,
        );
        let sole_generation = results.generation;
        assert_eq!(results.enter_target("hash"), Some((sole_generation, 0)));

        results.replace_test_actions(
            "hash".into(),
            vec![first.clone(), second.clone()],
            ResolvedQueryResultsLayout::default(),
            2,
        );
        let rendered_generation = results.generation;
        assert_eq!(results.enter_target("hash"), None);
        assert!(results.move_selection(QuerySelectionMove::Down));
        assert!(results.move_selection(QuerySelectionMove::Down));
        assert_eq!(results.enter_target("hash"), Some((rendered_generation, 1)));
        assert_eq!(
            results.resolve_action(rendered_generation, 1, "hash"),
            Some(second.clone())
        );
        let selected = results
            .resolve_action(rendered_generation, 1, "hash")
            .expect("the selected absolute row resolves in its current generation");
        let InstalledSearchInvocation { invocation, route } =
            parse_installed_search_invocation(selected, ActivationSource::Enter)
                .expect("the persisted row resolves through the installed typed route");
        assert!(matches!(route, InstalledSearchRoute::HashOpen));
        assert_eq!(invocation.original_action, second);
        assert_eq!(invocation.source, ActivationSource::Enter);

        results.replace_test_actions(
            "hash changed".into(),
            vec![first],
            ResolvedQueryResultsLayout::default(),
            2,
        );
        assert_eq!(results.selected, None);
        assert_eq!(
            results.resolve_action(rendered_generation, 0, "hash changed"),
            None,
            "a row rendered for an older result generation must not activate"
        );
        assert_eq!(results.enter_target("hash"), None);

        let stale_generation = results.generation;
        results.replace_test_actions(
            "hash changed".into(),
            vec![hash_action()],
            ResolvedQueryResultsLayout::default(),
            2,
        );
        assert_eq!(
            results.resolve_action(stale_generation, 0, "hash changed"),
            None,
            "replacement at the same query and index still rejects the old row generation"
        );
    }

    #[test]
    fn launcher_search_arrows_navigate_and_invalidate_equal_query_rows() {
        let first = hash_action();
        let second = multi_launcher::actions::Action {
            label: "Second installed route".into(),
            desc: "Fixture row".into(),
            action: "hash:open".into(),
            args: Some("second argument".into()),
        };
        let actions = vec![first, second];
        let mut results = LauncherSearchResults::default();
        results.replace_test_actions(
            "app QMarker".into(),
            actions.clone(),
            ResolvedQueryResultsLayout::default(),
            2,
        );

        assert!(!results.move_selection(QuerySelectionMove::Up));
        assert!(results.move_selection(QuerySelectionMove::Down));
        assert_eq!(results.selected, Some(0));
        assert!(results.move_selection(QuerySelectionMove::Down));
        assert_eq!(results.selected, Some(1));
        assert!(!results.move_selection(QuerySelectionMove::Down));
        assert_eq!(results.selected, Some(1));
        assert!(results.move_selection(QuerySelectionMove::Up));
        assert_eq!(results.selected, Some(0));
        assert!(results.move_selection(QuerySelectionMove::Up));
        assert_eq!(results.selected, None);
        assert_eq!(results.enter_target("app QMarker"), None);

        let previous_generation = results.generation;
        assert!(results.move_selection(QuerySelectionMove::Down));
        assert_eq!(results.selected, Some(0));
        results.replace_test_actions(
            "app QMarker".into(),
            actions,
            ResolvedQueryResultsLayout::default(),
            2,
        );
        assert_eq!(results.generation, previous_generation + 1);
        assert_eq!(results.selected, None);
        assert_eq!(results.enter_target("app QMarker"), None);
    }

    #[test]
    fn launcher_search_grid_moves_page_jumps_and_partial_row_identity_are_absolute() {
        let actions = (0..7)
            .map(|index| multi_launcher::actions::Action {
                label: format!("Grid row {index}"),
                desc: "Grid navigation fixture".into(),
                action: "hash:open".into(),
                args: Some(format!("row {index}")),
            })
            .collect::<Vec<_>>();
        let layout = ResolvedQueryResultsLayout::new(true, 3);
        let mut results = LauncherSearchResults::default();
        results.replace_test_actions("app GridFixture".into(), actions.clone(), layout, 2);

        assert_eq!(results.layout.row_count(actions.len()), 3);
        assert_eq!(results.selected_scroll_child(), None);
        assert!(!results.move_selection(QuerySelectionMove::Up));
        assert!(results.move_selection(QuerySelectionMove::Down));
        assert_eq!(results.selected, Some(0));
        assert_eq!(results.selected_scroll_child(), Some(0));
        assert!(results.move_selection(QuerySelectionMove::Right));
        assert_eq!(results.selected, Some(1));
        assert!(results.move_selection(QuerySelectionMove::Down));
        assert_eq!(results.selected, Some(4));
        assert_eq!(results.selected_scroll_child(), Some(1));
        assert!(results.move_selection(QuerySelectionMove::PageDown));
        assert_eq!(results.selected, Some(6));
        assert_eq!(results.selected_scroll_child(), Some(2));
        let (generation, index) = results
            .enter_target("app GridFixture")
            .expect("page navigation selects an exact current result");
        assert_eq!(index, 6);
        assert_eq!(
            results.resolve_action(generation, index, "app GridFixture"),
            Some(actions[6].clone())
        );
        assert!(!results.move_selection(QuerySelectionMove::PageDown));
        assert!(results.move_selection(QuerySelectionMove::PageUp));
        assert_eq!(results.selected, Some(4));
        assert!(results.move_selection(QuerySelectionMove::Up));
        assert_eq!(results.selected, Some(1));
        assert!(results.move_selection(QuerySelectionMove::Left));
        assert_eq!(results.selected, Some(0));

        assert_eq!(
            launcher_result_grid_row_id(generation, 6),
            "launcher-search-grid-row-1-6"
        );
        assert_ne!(
            launcher_result_grid_row_id(generation, 6),
            launcher_result_grid_row_id(generation + 1, 6)
        );
    }

    #[test]
    fn grid_claims_empty_lateral_moves_while_list_leaves_them_to_search() {
        let mut results = LauncherSearchResults::default();
        results.replace_test_actions(
            "app Empty".into(),
            Vec::new(),
            ResolvedQueryResultsLayout::new(true, 3),
            2,
        );
        assert!(results.can_capture_lateral_move());
        assert!(!results.move_selection(QuerySelectionMove::Right));

        results.replace_test_actions(
            "hash".into(),
            vec![hash_action()],
            ResolvedQueryResultsLayout::new(false, 3),
            2,
        );
        assert!(!results.can_capture_lateral_move());
        assert_eq!(results.selected, None);
    }

    #[test]
    fn launcher_search_keypad_classifier_maps_all_scan_and_virtual_key_forms() {
        use multi_launcher::launcher::query_results_layout::QuerySelectionMove;

        let root = ForegroundIdentity {
            hwnd: 17,
            process_id: 19,
            thread_id: 23,
        };
        let contexts = launcher_search_contexts();
        for (scan, virtual_key, expected) in [
            (0x50, 0x62, QuerySelectionMove::Down),
            (0x50, 0x28, QuerySelectionMove::Down),
            (0, 0x62, QuerySelectionMove::Down),
            (0x4b, 0x64, QuerySelectionMove::Left),
            (0x4b, 0x25, QuerySelectionMove::Left),
            (0, 0x64, QuerySelectionMove::Left),
            (0x4d, 0x66, QuerySelectionMove::Right),
            (0x4d, 0x27, QuerySelectionMove::Right),
            (0, 0x66, QuerySelectionMove::Right),
            (0x48, 0x68, QuerySelectionMove::Up),
            (0x48, 0x26, QuerySelectionMove::Up),
            (0, 0x68, QuerySelectionMove::Up),
        ] {
            assert_eq!(
                physical_keypad_selection_move(
                    Some(physical_keydown(scan, virtual_key)),
                    Modifiers::default(),
                    &contexts,
                    Some(root),
                    Some(root),
                    true,
                ),
                Some(expected),
                "scan {scan:#04x}, virtual key {virtual_key:#04x}"
            );
        }
    }

    #[test]
    fn launcher_search_keypad_classifier_rejects_nonphysical_or_foreign_events() {
        let root = ForegroundIdentity {
            hwnd: 17,
            process_id: 19,
            thread_id: 23,
        };
        let contexts = launcher_search_contexts();
        let accepted = |receipt, event_modifiers, contexts: &[KeyContext], foreground, is_root| {
            physical_keypad_selection_move(
                receipt,
                event_modifiers,
                contexts,
                Some(root),
                foreground,
                is_root,
            )
        };

        assert_eq!(
            accepted(
                Some(physical_keydown(0x50, 0x62)),
                Modifiers::default(),
                &contexts,
                Some(root),
                true,
            ),
            Some(multi_launcher::launcher::query_results_layout::QuerySelectionMove::Down)
        );
        for receipt in [
            None,
            Some(PhysicalKeydown {
                extended: true,
                ..physical_keydown(0x50, 0x62)
            }),
            Some(PhysicalKeydown {
                extended: true,
                ..physical_keydown(0, 0x62)
            }),
            Some(physical_keydown(0x03, 0x32)),
            Some(physical_keydown(0, 0x32)),
            Some(physical_keydown(0x50, 0x32)),
            Some(physical_keydown(0, 0x28)),
            Some(physical_keydown(0, 0x25)),
            Some(physical_keydown(0, 0x27)),
            Some(physical_keydown(0, 0x26)),
            Some(physical_keydown(0x51, 0x62)),
            Some(PhysicalKeydown {
                modifiers: Modifiers {
                    control: true,
                    ..Modifiers::default()
                },
                ..physical_keydown(0x50, 0x62)
            }),
        ] {
            assert_eq!(
                accepted(receipt, Modifiers::default(), &contexts, Some(root), true),
                None
            );
        }

        let modified_event = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        assert_eq!(
            accepted(
                Some(physical_keydown(0x50, 0x62)),
                modified_event,
                &contexts,
                Some(root),
                true,
            ),
            None
        );
        assert_eq!(
            accepted(
                Some(physical_keydown(0x50, 0x62)),
                Modifiers::default(),
                &contexts,
                Some(ForegroundIdentity {
                    process_id: root.process_id + 1,
                    ..root
                }),
                true,
            ),
            None
        );
        assert_eq!(
            accepted(
                Some(PhysicalKeydown {
                    hwnd: root.hwnd + 1,
                    ..physical_keydown(0x50, 0x62)
                }),
                Modifiers::default(),
                &contexts,
                Some(root),
                true,
            ),
            None
        );
        assert_eq!(
            accepted(
                Some(physical_keydown(0x50, 0x62)),
                Modifiers::default(),
                &contexts,
                Some(root),
                false,
            ),
            None
        );
        assert_eq!(
            physical_keypad_selection_move(
                Some(physical_keydown(0x50, 0x62)),
                Modifiers::default(),
                &[],
                Some(root),
                Some(root),
                true,
            ),
            None
        );
        let hash_contexts = [
            KeyContext::parse("RootWindow").expect("valid root key context"),
            KeyContext::parse("LauncherSearch").expect("valid search key context"),
            KeyContext::parse("HashGeneratorWindow").expect("valid Hash key context"),
        ];
        assert_eq!(
            physical_keypad_selection_move(
                Some(physical_keydown(0x50, 0x62)),
                Modifiers::default(),
                &hash_contexts,
                Some(root),
                Some(root),
                true,
            ),
            None
        );
    }

    #[test]
    fn launcher_search_keypad_list_lateral_moves_preserve_and_clear_exact_rows() {
        use multi_launcher::launcher::query_results_layout::QuerySelectionMove;

        let actions = ["Alpha", "Beta"]
            .into_iter()
            .map(|label| multi_launcher::actions::Action {
                label: label.to_owned(),
                desc: "Inert keypad fixture".to_owned(),
                action: "query:".to_owned(),
                args: Some(label.to_ascii_lowercase()),
            })
            .collect::<Vec<_>>();
        let mut results = LauncherSearchResults::default();
        results.replace_test_actions(
            "app KMarker".to_owned(),
            actions.clone(),
            ResolvedQueryResultsLayout::new(false, 3),
            2,
        );

        assert!(results.move_keypad_selection(QuerySelectionMove::Right));
        assert_eq!(results.selected, Some(0));
        assert!(!results.move_keypad_selection(QuerySelectionMove::Right));
        assert_eq!(results.selected, Some(0));
        assert!(results.move_keypad_selection(QuerySelectionMove::Down));
        assert_eq!(results.selected, Some(1));
        assert!(!results.move_keypad_selection(QuerySelectionMove::Down));
        assert_eq!(results.selected, Some(1));
        assert!(results.move_keypad_selection(QuerySelectionMove::Left));
        assert_eq!(results.selected, None);
        assert!(!results.move_keypad_selection(QuerySelectionMove::Left));
        assert_eq!(results.enter_target("app KMarker"), None);
        assert_eq!(results.query, "app KMarker");

        assert!(results.move_keypad_selection(QuerySelectionMove::Right));
        let (generation, index) = results
            .enter_target("app KMarker")
            .expect("keypad Right selects the first available result");
        assert_eq!(index, 0);
        assert_eq!(
            results.resolve_action(generation, index, "app KMarker"),
            Some(actions[0].clone())
        );
    }

    #[test]
    fn launcher_search_keypad_grid_reuses_shared_direction_and_reveal_mapping() {
        use multi_launcher::launcher::query_results_layout::QuerySelectionMove;

        let actions = (0..7)
            .map(|index| multi_launcher::actions::Action {
                label: format!("KMarker Grid row {index}"),
                desc: "Grid navigation fixture".into(),
                action: "query:".into(),
                args: Some(index.to_string()),
            })
            .collect::<Vec<_>>();
        let mut results = LauncherSearchResults::default();
        results.replace_test_actions(
            "app GridFixture".into(),
            actions,
            ResolvedQueryResultsLayout::new(true, 3),
            2,
        );

        assert!(results.move_keypad_selection(QuerySelectionMove::Down));
        assert_eq!(results.selected, Some(0));
        assert!(results.move_keypad_selection(QuerySelectionMove::Right));
        assert_eq!(results.selected, Some(1));
        assert!(results.move_keypad_selection(QuerySelectionMove::Down));
        assert_eq!(results.selected, Some(4));
        assert!(results.move_keypad_selection(QuerySelectionMove::Down));
        assert_eq!(results.selected, Some(6));
        assert_eq!(results.selected_scroll_child(), Some(2));
        assert!(!results.move_keypad_selection(QuerySelectionMove::Down));
        assert_eq!(results.selected, Some(6));
    }

    #[test]
    fn gpui_history_snapshot_preserves_single_line_unicode_and_filters_newlines() {
        assert_eq!(
            single_line_history_queries([
                " app QMarker 東京 ",
                "query\nwith line break",
                "query\rwith carriage return",
                "hash",
            ]),
            vec![" app QMarker 東京 ", "hash"]
        );
    }

    #[test]
    fn search_history_actions_require_the_visible_live_focused_root() {
        assert!(search_input_is_current(true, true, true, true, true));
        assert!(!search_input_is_current(false, true, true, true, true));
        assert!(!search_input_is_current(true, false, true, true, true));
        assert!(!search_input_is_current(true, true, false, true, true));
        assert!(!search_input_is_current(true, true, true, false, true));
        assert!(!search_input_is_current(true, true, true, true, false));
    }

    #[test]
    fn result_activation_requires_a_live_root_and_search_focus_for_enter() {
        assert!(launcher_result_activation_is_current(
            true,
            true,
            true,
            false,
            true,
            LauncherResultActivationOrigin::ResultWidget,
        ));
        assert!(!launcher_result_activation_is_current(
            true,
            true,
            true,
            false,
            true,
            LauncherResultActivationOrigin::DeferredSearchEnter,
        ));
        assert!(launcher_result_activation_is_current(
            true,
            true,
            true,
            true,
            true,
            LauncherResultActivationOrigin::DeferredSearchEnter,
        ));
        assert!(!launcher_result_activation_is_current(
            false,
            true,
            true,
            true,
            true,
            LauncherResultActivationOrigin::ResultWidget,
        ));
        assert!(!launcher_result_activation_is_current(
            true,
            false,
            true,
            true,
            true,
            LauncherResultActivationOrigin::ResultWidget,
        ));
        assert!(!launcher_result_activation_is_current(
            true,
            true,
            false,
            true,
            true,
            LauncherResultActivationOrigin::ResultWidget,
        ));
        assert!(!launcher_result_activation_is_current(
            true,
            true,
            true,
            true,
            false,
            LauncherResultActivationOrigin::ResultWidget,
        ));
    }

    #[test]
    fn gpui_app_catalog_admits_query_set_and_keeps_distinct_hash_arguments() {
        let alpha = multi_launcher::actions::Action {
            label: "Alpha QMarker".into(),
            desc: "Hash generator".into(),
            action: "hash:open".into(),
            args: Some("alpha argument".into()),
        };
        let beta = multi_launcher::actions::Action {
            label: "Beta QMarker".into(),
            desc: "Hash generator".into(),
            action: "hash:open".into(),
            args: Some("beta argument".into()),
        };
        let query_set = saved_action(
            "Query Set QMarker",
            "query:g05 outcome",
            Some(r#"{"query":"  second β  "}"#),
        );
        let unsupported = multi_launcher::actions::Action {
            label: "Unsupported QMarker".into(),
            desc: "Other feature".into(),
            action: "queryexec:hash".into(),
            args: None,
        };
        let catalog = LauncherSearchCatalog::new(
            LauncherSearchStartup {
                branding: super::launcher_header::Branding::default(),
                verified_folders: Vec::new(),
                index_config: multi_launcher::indexer::coordinator::IndexConfig::new(Vec::new(), None),
                calculator_settings: None,
                actions: vec![
                    alpha.clone(),
                    beta.clone(),
                    query_set.clone(),
                    unsupported.clone(),
                ],
                match_exact: true,
                fuzzy_weight: 0.75,
                usage_weight: 0.0,
                query_autocomplete: true,
                show_examples: false,
                page_jump: 2,
                query_results_layout: QueryResultsLayoutSettings::default(),
                jump_mode_enabled: true,
                jump_mode_activation_chord: JumpModeChord::default(),
                jump_mode_hint_alphabet: HintAlphabet::default(),
                usage: HashMap::new(),
                folder_aliases_lc: HashMap::new(),
                bookmark_aliases_lc: HashMap::new(),
            },
            None,
        );

        assert_eq!(
            catalog.actions,
            vec![alpha.clone(), beta.clone(), query_set.clone()]
        );
        let composed = catalog.compose_actions("app QMarker", Vec::new());
        assert_eq!(
            composed,
            vec![alpha.clone(), beta.clone(), query_set.clone()],
            "same command strings do not collapse actions with distinct arguments"
        );
        let mut results = LauncherSearchResults::default();
        results.replace_test_actions(
            "app QMarker".into(),
            composed,
            ResolvedQueryResultsLayout::default(),
            2,
        );
        assert_eq!(results.enter_target("app QMarker"), None);
        assert!(results.move_selection(QuerySelectionMove::Down));
        assert!(results.move_selection(QuerySelectionMove::Down));
        let (generation, index) = results
            .enter_target("app QMarker")
            .expect("Down selects the second persisted row for fresh Enter");
        assert_eq!(index, 1);
        let action = results
            .resolve_action(generation, index, "app QMarker")
            .expect("the current query, generation and absolute position resolve Beta");
        let InstalledSearchInvocation { invocation, route } =
            parse_installed_search_invocation(action, ActivationSource::Enter)
                .expect("Beta resolves through the installed typed Hash Open route");
        assert!(matches!(route, InstalledSearchRoute::HashOpen));
        assert_eq!(invocation.original_action, beta);
        assert_eq!(invocation.source, ActivationSource::Enter);
        assert!(catalog.compose("", Vec::new(), true).iter().any(|row|
            row.origin == super::LauncherResultOrigin::Saved && row.action == alpha && row.label() == "app Alpha QMarker"
        ));
        assert_eq!(
            supported_launcher_search_actions(vec![unsupported]),
            Vec::new()
        );
    }

    #[test]
    fn gpui_cached_hash_command_fallback_respects_enablement_and_provider_precedence() {
        let enabled = HashSet::from(["hash_generator".to_owned()]);
        let startup = |match_exact| LauncherSearchStartup {
            branding: super::launcher_header::Branding::default(),
                verified_folders: Vec::new(),
                index_config: multi_launcher::indexer::coordinator::IndexConfig::new(Vec::new(), None),
            calculator_settings: None,
            actions: Vec::new(),
            match_exact,
            fuzzy_weight: 0.25,
            usage_weight: 0.0,
            query_autocomplete: true,
            show_examples: false,
            page_jump: 2,
            query_results_layout: QueryResultsLayoutSettings::default(),
            jump_mode_enabled: true,
            jump_mode_activation_chord: JumpModeChord::default(),
            jump_mode_hint_alphabet: HintAlphabet::default(),
            usage: HashMap::new(),
            folder_aliases_lc: HashMap::new(),
            bookmark_aliases_lc: HashMap::new(),
        };

        let enabled_catalog = LauncherSearchCatalog::new(startup(true), Some(&enabled));
        let hash_command = multi_launcher::plugins::hash_generator::HashGeneratorPlugin
            .commands()
            .into_iter()
            .next()
            .expect("Hash installs one cached Search command");
        assert_eq!(enabled_catalog.commands, vec![hash_command.clone()]);
        assert_eq!(enabled_catalog.command_cached_entries.len(), 1);
        assert_eq!(enabled_catalog.compose_actions("ha", Vec::new()), vec![hash_command.clone()]);

        let route = parse_installed_search_invocation(hash_command.clone(), ActivationSource::Enter)
            .expect("the fallback command uses the installed Query Set route");
        assert!(matches!(route.route, InstalledSearchRoute::QuerySet(_)));
        assert_eq!(route.invocation.original_action, hash_command);

        let provider = multi_launcher::actions::Action {
            label: "Provider owns this query".into(),
            desc: "Installed provider row".into(),
            action: "provider:owned".into(),
            args: Some("full provider arguments".into()),
        };
        assert_eq!(
            enabled_catalog.compose_actions("ha", vec![provider.clone()]),
            vec![provider],
            "an incoming provider row suppresses command fallback"
        );
        assert!(
            enabled_catalog.compose_actions("g ha", Vec::new()).is_empty(),
            "global search never offers a cached command fallback"
        );

        let disabled_catalog = LauncherSearchCatalog::new(startup(true), Some(&HashSet::new()));
        assert!(disabled_catalog.commands.is_empty());
        assert!(disabled_catalog.compose_actions("ha", Vec::new()).is_empty());

        let fuzzy_catalog = LauncherSearchCatalog::new(startup(false), Some(&enabled));
        assert_eq!(
            fuzzy_catalog.compose_actions("hsh", Vec::new()),
            vec![enabled_catalog.commands[0].clone()],
            "non-exact installed fallback reuses cached fuzzy command scoring"
        );

        let hash_plugin = multi_launcher::plugins::hash_generator::HashGeneratorPlugin;
        let disabled_plugin_provider = multi_launcher::plugin::search_filtered_plugins(
            [&hash_plugin as &dyn multi_launcher::plugin::Plugin],
            "hash",
            Some(&HashSet::new()),
            None,
        );
        assert!(disabled_plugin_provider.is_empty());

        let capability_disabled_provider = multi_launcher::plugin::search_filtered_plugins(
            [&hash_plugin as &dyn multi_launcher::plugin::Plugin],
            "hash",
            Some(&enabled),
            Some(&HashMap::from([("hash_generator".into(), Vec::new())])),
        );
        assert!(capability_disabled_provider.is_empty());
        assert_eq!(
            enabled_catalog.compose_actions("hash", capability_disabled_provider),
            vec![enabled_catalog.commands[0].clone()],
            "plugin enablement admits its command label while Search capability filters provider results"
        );
    }

    #[test]
    fn gpui_completion_uses_admitted_actions_and_enabled_hash_commands() {
        let alpha = saved_hash_action("G05 Comp 東 Alpha");
        let beta = saved_hash_action("G05 Comp 東 Beta");
        let unsupported = multi_launcher::actions::Action {
            label: "G05 Comp 東 Unsupported".into(),
            desc: "Other route".into(),
            action: "queryexec:hash".into(),
            args: None,
        };
        let catalog =
            completion_catalog(vec![alpha.clone(), beta.clone(), unsupported], true, None);

        assert_eq!(
            multi_launcher::completion::suggestions(
                &catalog.completion_index,
                "app g05 comp 東",
                5,
            ),
            vec!["app g05 comp 東 alpha", "app g05 comp 東 beta"]
        );
        assert_eq!(
            multi_launcher::completion::suggestions(&catalog.completion_index, "h", 5),
            vec!["hash"],
            "the enabled Hash command label remains in the completion index"
        );

        let disabled = HashSet::new();
        let disabled_catalog = completion_catalog(vec![alpha, beta], true, Some(&disabled));
        assert!(multi_launcher::completion::suggestions(
            &disabled_catalog.completion_index,
            "h",
            5,
        )
        .is_empty());
    }

    #[test]
    fn gpui_completion_preserves_its_cycle_edit_and_history_reset_rules() {
        let catalog = completion_catalog(
            vec![
                saved_hash_action("G05 Comp 東 Alpha"),
                saved_hash_action("G05 Comp 東 Beta"),
                multi_launcher::actions::Action {
                    label: "G05 Comp 東 Multiline\r\nLabel".into(),
                    desc: "Not representable by the pinned Search editor".into(),
                    action: "hash:open".into(),
                    args: None,
                },
            ],
            true,
            None,
        );
        let draft = "app g05 comp 東";
        let alpha = "app g05 comp 東 alpha";
        let beta = "app g05 comp 東 beta";
        let mut completion = SearchCompletion::default();
        completion.reset_for_query(&catalog, draft);

        assert_eq!(
            completion.suggestions,
            vec![alpha.to_owned(), beta.to_owned()],
            "the GPUI picker excludes a CR/LF candidate it cannot represent exactly"
        );
        assert_eq!(completion.begin_replacement(draft), Some(alpha.into()));
        completion.apply_change(&catalog, alpha);
        assert_eq!(completion.next_index, 1);
        assert_eq!(completion.begin_replacement(alpha), Some(beta.into()));
        completion.apply_change(&catalog, beta);
        assert_eq!(completion.next_index, 0, "the sequence wraps after Beta");

        // The ordinary Change event for an equal-text history recall has no
        // completion receipt, so it starts from the newly recalled query.
        completion.apply_change(&catalog, beta);
        assert!(completion.suggestions.is_empty());
        assert_eq!(completion.next_index, 0, "equal-text history recall resets the cycle");
        assert!(completion.begin_replacement("app g05 comp missing").is_none());
        assert!(completion.pending_change.is_none());

        completion.reset_for_query(&catalog, draft);
        assert_eq!(completion.begin_replacement(draft), Some(alpha.into()));
        completion.apply_change(&catalog, alpha);
        assert_eq!(completion.begin_replacement(alpha), Some(beta.into()));
        completion.apply_change(&catalog, beta);
        assert_eq!(completion.begin_replacement(beta), Some(alpha.into()));
        completion.apply_change(&catalog, alpha);
        let accepted = alpha;
        completion.apply_change(&catalog, &format!("{accepted}β"));
        assert!(completion.suggestions.is_empty());
        assert_eq!(completion.next_index, 0, "a user edit clears the accepted sequence");
    }

    #[test]
    fn query_set_change_cannot_consume_a_pending_completion_receipt() {
        let catalog = completion_catalog(
            vec![
                saved_hash_action("G05 Comp 東 Alpha"),
                saved_hash_action("G05 Comp 東 Beta"),
            ],
            true,
            None,
        );
        let mut completion = SearchCompletion::default();
        completion.reset_for_query(&catalog, "app g05 comp 東");
        assert_eq!(completion.suggestions.len(), 2);
        let query = completion
            .begin_replacement("app g05 comp 東")
            .expect("the first suggestion creates a completion receipt");
        assert_eq!(query, "app g05 comp 東 alpha");
        assert!(completion.pending_change.is_some());

        completion.clear_for_query_replacement();
        completion.apply_change(&catalog, &query);
        assert!(completion.suggestions.is_empty());
        assert_eq!(completion.next_index, 0);
        assert!(completion.pending_change.is_none());
    }

    #[test]
    fn gpui_completion_respects_the_setting_and_declines_without_candidates() {
        let catalog = completion_catalog(Vec::new(), false, None);
        let mut completion = SearchCompletion::default();
        completion.reset_for_query(&catalog, "h");
        assert!(completion.suggestions.is_empty());
        assert!(completion.begin_replacement("h").is_none());
        assert!(completion.pending_change.is_none());
        assert!(search_completion_session_is_current(Some(8), 8));
        assert!(!search_completion_session_is_current(Some(8), 9));
        assert!(!search_completion_session_is_current(None, 8));
    }

    #[test]
    fn hash_escape_defers_to_nested_owners_and_native_picker() {
        let contexts = |nested: Option<&str>| {
            let mut contexts = vec![KeyContext::parse("HashGeneratorWindow").unwrap()];
            if let Some(nested) = nested {
                contexts.push(KeyContext::parse(nested).unwrap());
            }
            contexts
        };
        assert!(hash_escape_capture_should_close(&contexts(Some("Input")), true, false));
        assert!(hash_escape_capture_should_close(&contexts(Some("Select")), true, false));
        for owner in ["List", "PopupMenu", "AlertDialog", "Dialog"] {
            assert!(!hash_escape_capture_should_close(&contexts(Some(owner)), true, false));
        }
        assert!(!hash_escape_capture_should_close(&contexts(None), false, false));
        assert!(!hash_escape_capture_should_close(&contexts(None), true, true));
    }

    #[test]
    fn post_dispatch_completion_requires_plain_root_tab_into_search() {
        let root = [KeyContext::parse("RootWindow").expect("valid root key context")];
        let search = [
            KeyContext::parse("RootWindow").expect("valid root key context"),
            KeyContext::parse("LauncherSearch").expect("valid search key context"),
        ];
        let utility = [
            KeyContext::parse("RootWindow").expect("valid root key context"),
            KeyContext::parse("HashGeneratorWindow").expect("valid Hash key context"),
        ];
        let settings = [
            KeyContext::parse("RootWindow").expect("valid root key context"),
            KeyContext::parse("SettingsEditor").expect("valid Settings editor context"),
        ];
        let popup = [
            KeyContext::parse("RootWindow").expect("valid root key context"),
            KeyContext::parse("PopupMenu").expect("valid popup menu context"),
        ];
        let toast_log = [
            KeyContext::parse("RootWindow").expect("valid root key context"),
            KeyContext::parse("ToastLogViewer").expect("valid Toast Log viewer context"),
        ];
        let command_help = [
            KeyContext::parse("RootWindow").expect("valid root key context"),
            KeyContext::parse("CommandHelp").expect("valid Command Help context"),
        ];
        let input = [
            KeyContext::parse("RootWindow").expect("valid root key context"),
            KeyContext::parse("CommandHelp").expect("valid Command Help context"),
            KeyContext::parse("Input").expect("valid input context"),
        ];
        assert!(root_tab_traversed_to_search(
            "tab",
            Modifiers::default(),
            true,
            &root,
            true,
            false,
        ));
        assert!(!root_tab_traversed_to_search(
            "tab",
            Modifiers::shift(),
            true,
            &root,
            true,
            false,
        ));
        assert!(!root_tab_traversed_to_search(
            "tab",
            Modifiers::default(),
            true,
            &command_help,
            true,
            false,
        ));
        let plugin_editor = [
            KeyContext::parse("RootWindow").expect("valid root key context"),
            KeyContext::parse("PluginEditorPane").expect("valid plugin editor context"),
        ];
        assert!(!root_tab_traversed_to_search(
            "tab", Modifiers::default(), true, &plugin_editor, true, false,
        ));
        assert!(command_help_escape_capture_should_close(&command_help, true));
        assert!(!command_help_escape_capture_should_close(&command_help, false));
        assert!(
            !command_help_escape_capture_should_close(&input, true),
            "a more-specific input receives Escape before the Command Help owner"
        );
        assert!(
            !command_help_escape_capture_should_close(&popup, true),
            "a more-specific popup receives Escape before the Command Help owner"
        );
        assert!(!root_tab_traversed_to_search(
            "tab",
            Modifiers::default(),
            true,
            &search,
            true,
            false,
        ));
        assert!(!root_tab_traversed_to_search(
            "tab",
            Modifiers::default(),
            true,
            &utility,
            true,
            false,
        ));
        assert!(!root_tab_traversed_to_search(
            "tab",
            Modifiers::default(),
            true,
            &settings,
            true,
            false,
        ));
        assert!(!root_tab_traversed_to_search(
            "tab",
            Modifiers::default(),
            true,
            &popup,
            true,
            false,
        ));
        assert!(!root_tab_traversed_to_search(
            "tab",
            Modifiers::default(),
            true,
            &toast_log,
            true,
            false,
        ));
        assert!(root_tab_traversed_to_search(
            "tab",
            Modifiers::default(),
            true,
            &root,
            true,
            false,
        ));
        assert!(!root_tab_traversed_to_search(
            "tab",
            Modifiers::default(),
            true,
            &root,
            true,
            true,
        ));
        assert!(!root_tab_traversed_to_search(
            "tab",
            Modifiers::default(),
            false,
            &root,
            true,
            false,
        ));
        assert!(!root_tab_traversed_to_search(
            "tab",
            Modifiers::default(),
            true,
            &root,
            false,
            false,
        ));
    }

    #[test]
    fn quick_help_escape_is_scoped_to_its_panel_and_filter() {
        let quick_help = [
            KeyContext::parse("RootWindow").expect("valid root key context"),
            KeyContext::parse("QuickHelp").expect("valid Quick Help key context"),
        ];
        let quick_help_input = [
            KeyContext::parse("RootWindow").expect("valid root key context"),
            KeyContext::parse("QuickHelp").expect("valid Quick Help key context"),
            KeyContext::parse("Input").expect("valid input context"),
        ];
        let quick_help_popup = [
            KeyContext::parse("RootWindow").expect("valid root key context"),
            KeyContext::parse("QuickHelp").expect("valid Quick Help key context"),
            KeyContext::parse("PopupMenu").expect("valid popup menu context"),
        ];
        let quick_help_alert = [
            KeyContext::parse("RootWindow").expect("valid root key context"),
            KeyContext::parse("QuickHelp").expect("valid Quick Help key context"),
            KeyContext::parse("AlertDialog").expect("valid alert dialog key context"),
        ];
        let quick_help_dialog = [
            KeyContext::parse("RootWindow").expect("valid root key context"),
            KeyContext::parse("QuickHelp").expect("valid Quick Help key context"),
            KeyContext::parse("Dialog").expect("valid dialog key context"),
        ];
        assert!(quick_help_escape_capture_should_close(&quick_help, true));
        assert!(quick_help_escape_capture_should_close(&quick_help_input, true));
        assert!(!quick_help_escape_capture_should_close(&quick_help_input, false));
        assert!(!quick_help_escape_capture_should_close(&quick_help_popup, true));
        assert!(!quick_help_escape_capture_should_close(&quick_help_alert, true));
        assert!(!quick_help_escape_capture_should_close(&quick_help_dialog, true));
    }

    #[test]
    fn quick_help_shortcut_admission_checks_launcher_jump_and_actual_gpui_bindings() {
        let launcher = multi_launcher::hotkey::parse_hotkey("F2").unwrap();
        let default_jump = JumpModeChord::parse("Shift+Space").unwrap();
        let disabled = admit_quick_help_shortcut(None, launcher, false, default_jump);
        assert!(disabled.shortcut.is_none());
        assert!(disabled.diagnostic.is_none());

        let invalid = admit_quick_help_shortcut(Some("NotAKey"), launcher, false, default_jump);
        assert!(invalid.shortcut.is_none());
        assert!(invalid
            .diagnostic
            .as_deref()
            .is_some_and(|message| message.contains("is invalid")));

        let launcher_conflict = admit_quick_help_shortcut(Some("F2"), launcher, false, default_jump);
        assert!(launcher_conflict.shortcut.is_none());
        assert!(launcher_conflict
            .diagnostic
            .as_deref()
            .is_some_and(|message| message.contains("launcher shortcut")));

        let f8 = admit_quick_help_shortcut(Some("F8"), launcher, false, default_jump);
        assert_eq!(f8.shortcut.as_ref().map(|shortcut| shortcut.display.as_str()), Some("F8"));
        assert!(f8.diagnostic.is_none());

        let jump_conflict = admit_quick_help_shortcut(
            Some("F8"),
            launcher,
            true,
            JumpModeChord::parse("F8").unwrap(),
        );
        assert!(jump_conflict.shortcut.is_none());
        assert!(jump_conflict
            .diagnostic
            .as_deref()
            .is_some_and(|message| message.contains("Search Jump shortcut")));

        let local_conflict = admit_quick_help_shortcut(Some("F6"), launcher, false, default_jump);
        assert!(local_conflict.shortcut.is_none());
        assert!(local_conflict
            .diagnostic
            .as_deref()
            .is_some_and(|message| message.contains("Coordinate HUD")));

        let extra_required_modifier =
            admit_quick_help_shortcut(Some("Ctrl+Shift+G"), launcher, false, default_jump);
        assert!(extra_required_modifier.shortcut.is_some());
        assert!(extra_required_modifier.diagnostic.is_none());

        for polling_chord in ["G", "Ctrl+G"] {
            let conflict =
                admit_quick_help_shortcut(Some(polling_chord), launcher, false, default_jump);
            assert!(conflict.shortcut.is_none(), "{polling_chord} should conflict");
            assert!(conflict
                .diagnostic
                .as_deref()
                .is_some_and(|message| message.contains("Open Hash Generator")));
        }

        let modified_local_conflict =
            admit_quick_help_shortcut(Some("Ctrl+Shift+E"), launcher, false, default_jump);
        assert!(modified_local_conflict.shortcut.is_none());
        assert!(modified_local_conflict
            .diagnostic
            .as_deref()
            .is_some_and(|message| message.contains("Restart in egui")));

        let hotkeys = quick_help_hotkey_lines("F2", f8.shortcut.as_ref());
        assert!(hotkeys
            .iter()
            .any(|line| line.label == "Toggle launcher" && line.chord == "F2"));
        assert!(hotkeys
            .iter()
            .any(|line| line.label == "Help overlay" && line.chord == "F8"));
        assert!(hotkeys.iter().any(|line| {
            line.label == "Restart in egui" && line.chord == "Ctrl+Shift+E"
        }));

        let unsupported = admit_quick_help_shortcut(Some("F13"), launcher, false, default_jump);
        assert!(unsupported.shortcut.is_none());
        assert!(unsupported
            .diagnostic
            .as_deref()
            .is_some_and(|message| message.contains("unsupported by the Windows GPUI listener")));
        assert_eq!(gpui_key_bindings().len(), GPUI_BOUND_KEYS.len());
    }

    #[test]
    fn quick_help_event_requires_the_current_visible_root_and_focus_never_activates_it() {
        assert!(quick_help_event_targets_visible_revision(true, true, true, 7, 7));
        assert!(!quick_help_event_targets_visible_revision(false, true, true, 7, 7));
        assert!(!quick_help_event_targets_visible_revision(true, false, true, 7, 7));
        assert!(!quick_help_event_targets_visible_revision(true, true, false, 7, 7));
        assert!(!quick_help_event_targets_visible_revision(true, true, true, 6, 7));

        assert!(quick_help_focus_change_is_allowed(
            true,
            true,
            QuickHelpFocusBlockers::default(),
            false
        ));
        assert!(!quick_help_focus_change_is_allowed(
            false,
            false,
            QuickHelpFocusBlockers::default(),
            false
        ));
        let blockers = [
            QuickHelpFocusBlockers {
                menu_open: true,
                ..QuickHelpFocusBlockers::default()
            },
            QuickHelpFocusBlockers {
                popup_or_alert_owns_focus: true,
                ..QuickHelpFocusBlockers::default()
            },
            QuickHelpFocusBlockers {
                settings_owner_has_focus: true,
                ..QuickHelpFocusBlockers::default()
            },
            QuickHelpFocusBlockers {
                toast_log_owner_has_focus: true,
                ..QuickHelpFocusBlockers::default()
            },
            QuickHelpFocusBlockers {
                command_help_owner_has_focus: true,
                ..QuickHelpFocusBlockers::default()
            },
            QuickHelpFocusBlockers {
                docked_hash_owner_has_focus: true,
                ..QuickHelpFocusBlockers::default()
            },
            QuickHelpFocusBlockers {
                hash_resize_owner_has_focus: true,
                ..QuickHelpFocusBlockers::default()
            },
        ];
        for blocker in blockers {
            assert!(!quick_help_focus_change_is_allowed(true, true, blocker, false));
        }
        assert!(!quick_help_focus_change_is_allowed(
            true,
            true,
            QuickHelpFocusBlockers::default(),
            true
        ));
        assert_eq!(quick_help_viewport_body_height(500.0, 160.0), 280.0);
        assert_eq!(quick_help_viewport_body_height(220.0, 160.0), 60.0);
    }

    #[test]
    fn quick_help_scroll_anchor_follows_optional_docked_hash_and_existing_panels() {
        assert_eq!(docked_hash_root_scroll_item_index(false, false, false), 1);
        assert_eq!(quick_help_root_scroll_item_index(false, false, false, false), 1);
        assert_eq!(quick_help_root_scroll_item_index(false, false, false, true), 2);
        assert_eq!(docked_hash_root_scroll_item_index(true, true, true), 4);
        assert_eq!(quick_help_root_scroll_item_index(true, true, true, true), 5);
    }

    #[test]
    fn root_menu_close_restores_only_after_the_final_menu_loses_live_focus() {
        assert!(root_menu_close_should_restore_focus(
            false, false, false, true
        ));
        assert!(
            !root_menu_close_should_restore_focus(false, true, false, true),
            "Kit focus inside Settings or Toast routing remains authoritative"
        );
        assert!(
            !root_menu_close_should_restore_focus(true, false, false, true),
            "closing one popup cannot steal focus while another root menu stays open"
        );
        assert!(
            !root_menu_close_should_restore_focus(false, false, true, true),
            "accepted Settings or Toast routing remains authoritative"
        );
        assert!(
            !root_menu_close_should_restore_focus(false, false, false, false),
            "a menu dismissal cannot reactivate an unfocused launcher window"
        );
    }

    #[test]
    fn toast_log_body_height_reserves_footer_and_caps_at_240_pixels() {
        assert_eq!(toast_log_viewport_body_height(171.0, 112.0), 59.0);
        assert_eq!(toast_log_viewport_body_height(500.0, 112.0), 240.0);
        assert_eq!(toast_log_viewport_body_height(100.0, 112.0), 0.0);
    }

    #[test]
    fn toast_log_viewer_preserves_rows_and_fences_stale_reads_by_session_and_request() {
        let mut viewer = ToastLogViewerState::default();
        viewer.rows = vec!["existing row".to_owned()];
        let open_receipt = viewer.open().expect("opening starts a read");
        assert!(viewer.rows.is_empty(), "a fresh session clears prior rows");

        let refresh_receipt = viewer.begin_read().expect("Refresh starts a newer request");
        assert!(viewer.rows.is_empty());
        assert!(
            !viewer.finish_read(open_receipt, vec!["stale open row".to_owned()]),
            "an older request in the same session cannot replace the current Refresh"
        );
        assert!(viewer.rows.is_empty());
        assert!(viewer.finish_read(refresh_receipt, vec!["refreshed row".to_owned()]));
        assert_eq!(viewer.rows, vec!["refreshed row".to_owned()]);

        let pending_close = viewer.begin_read().expect("a second refresh is admitted");
        assert_eq!(
            viewer.rows,
            vec!["refreshed row".to_owned()],
            "Refresh retains the current rows until its receipt"
        );
        assert!(viewer.close());
        assert_eq!(viewer.rows, vec!["refreshed row".to_owned()]);
        assert!(
            !viewer.finish_read(pending_close, vec!["closed session row".to_owned()]),
            "closing the viewer invalidates an outstanding read"
        );
        let reopened_receipt = viewer
            .open()
            .expect("reopening starts a fresh session read");
        assert_ne!(reopened_receipt.session_id, open_receipt.session_id);
        assert!(
            viewer.rows.is_empty(),
            "reopening clears rows from the old session"
        );
        assert!(
            !viewer.finish_read(pending_close, vec!["old session row".to_owned()]),
            "a completion from the prior session cannot replace retained rows"
        );
        assert!(viewer.rows.is_empty());
        assert!(viewer.finish_read(reopened_receipt, vec!["new session row".to_owned()]));
        assert_eq!(viewer.rows, vec!["new session row".to_owned()]);
    }

    #[test]
    fn root_close_paths_wait_for_toast_log_open_and_clear_only_its_status() {
        assert_eq!(
            root_close_blocker(None, false, true, false, false),
            Some(RootCloseBlocker::ToastLogOpen),
            "the external OS handoff remains a close blocker until its receipt"
        );
        assert_eq!(
            root_close_blocker(None, false, true, true, true),
            Some(RootCloseBlocker::ToastLogOpen),
            "the same pending predicate applies before a Settings save can claim close"
        );
        assert_eq!(
            root_close_blocker(None, false, false, true, false),
            Some(RootCloseBlocker::BooleanPreferenceSave)
        );
        assert_eq!(
            root_close_blocker(None, false, false, false, true),
            Some(RootCloseBlocker::BooleanPreferenceSave)
        );
        assert_eq!(
            root_close_blocker(None, false, false, false, false),
            None,
            "read-only viewer requests do not delay close"
        );

        let mut status = Some(TOAST_LOG_OPEN_PENDING_STATUS.to_owned());
        assert!(clear_toast_log_open_pending_status(&mut status));
        assert_eq!(status, None);

        let mut unrelated = Some("A Hash Generator operation is still pending.".to_owned());
        assert!(!clear_toast_log_open_pending_status(&mut unrelated));
        assert_eq!(
            unrelated.as_deref(),
            Some("A Hash Generator operation is still pending.")
        );
    }

    #[test]
    fn settings_cleaning_clears_only_the_stale_dirty_restart_message() {
        let mut status = Some(SETTINGS_DIRTY_RESTART_STATUS.to_owned());
        assert!(!clear_stale_settings_restart_status(&mut status, false));
        assert!(clear_stale_settings_restart_status(&mut status, true));
        let mut unrelated = Some("A Hash Generator operation is still pending.".to_owned());
        assert!(!clear_stale_settings_restart_status(&mut unrelated, true));
    }

    #[test]
    fn settings_cancel_restores_origin_only_while_settings_still_owns_focus() {
        assert!(settings_return_focus_is_still_owned(true, false));
        assert!(
            !settings_return_focus_is_still_owned(false, false),
            "a newer docked Hash focus owner must keep its focus"
        );
        assert!(
            !settings_return_focus_is_still_owned(true, true),
            "focus restoration cannot cross the restart fence"
        );
        assert!(settings_menu_close_needs_trigger_focus(true, false, false));
        assert!(settings_menu_close_needs_trigger_focus(false, true, true));
        assert!(
            !settings_menu_close_needs_trigger_focus(false, true, false),
            "closing Settings from a popup opened over Search or Hash leaves that live origin alone"
        );
        assert!(!settings_menu_close_needs_trigger_focus(false, false, true));
    }

    #[test]
    fn launcher_row_ids_include_generation_and_absolute_position() {
        assert_ne!(launcher_result_row_id(9, 0), launcher_result_row_id(9, 1));
        assert_ne!(launcher_result_row_id(9, 1), launcher_result_row_id(10, 1));
    }

    #[test]
    fn search_enter_accepts_only_fresh_plain_keydown() {
        assert!(is_fresh_plain_search_enter(
            true,
            "enter",
            Modifiers::default(),
            false,
        ));
        assert!(!is_fresh_plain_search_enter(
            true,
            "enter",
            Modifiers::default(),
            true,
        ));
        assert!(!is_fresh_plain_search_enter(
            true,
            "enter",
            Modifiers::control(),
            false,
        ));
        assert!(!is_fresh_plain_search_enter(
            false,
            "enter",
            Modifiers::default(),
            false,
        ));
        assert!(!is_fresh_plain_search_enter(
            true,
            "down",
            Modifiers::default(),
            false,
        ));

        let marker =
            fresh_search_enter_marker(
                true,
                "enter",
                Modifiers::default(),
                false,
                7,
                "hash".into(),
                11,
            )
                .unwrap();
        assert_eq!(marker.generation, 7);
        assert_eq!(marker.query, "hash");
        assert_eq!(marker.jump_mode_revision, 11);
    }

    #[test]
    fn search_activation_preserves_source_and_dispatches_installed_routes_once() {
        let mut action = hash_action();
        action.args = Some("retained action arguments".into());
        assert_eq!(result_click_source(true), ActivationSource::Enter);
        assert_eq!(result_click_source(false), ActivationSource::Click);
        for source in [ActivationSource::Enter, ActivationSource::Click] {
            let InstalledSearchInvocation { invocation, route } =
                parse_installed_search_invocation(action.clone(), source).unwrap();
            assert!(matches!(route, InstalledSearchRoute::HashOpen));
            assert_eq!(invocation.original_action, action);
            assert_eq!(invocation.query_override, None);
            assert_eq!(invocation.source, source);

            let mut opened = 0;
            let outcome = dispatch_hash_open(invocation, || {
                opened += 1;
                Ok(())
            })
            .unwrap();
            assert_eq!(opened, 1);
            assert_eq!(outcome, CommandOutcome::default());
        }

        let query_action = saved_action(
            "Saved query",
            "query:g05 outcome",
            Some(r#"{"query":"  second β  "}"#),
        );
        for source in [ActivationSource::Enter, ActivationSource::Click] {
            let InstalledSearchInvocation { invocation, route } =
                parse_installed_search_invocation(query_action.clone(), source).unwrap();
            assert_eq!(invocation.original_action, query_action);
            assert_eq!(invocation.query_override, None);
            assert_eq!(invocation.source, source);
            let InstalledSearchRoute::QuerySet(outcome) = route else {
                panic!("saved query must use the shared Query Set handler");
            };
            assert_eq!(
                query_set_outcome_text(&outcome),
                Some("g05 outcome   second β  ")
            );
        }

        let invalid = multi_launcher::actions::Action {
            label: "Not Hash".into(),
            desc: "Other route".into(),
            action: "queryexec:hash".into(),
            args: None,
        };
        assert!(parse_installed_search_invocation(invalid, ActivationSource::Click).is_err());
    }

    #[test]
    fn defaults_to_egui_and_preserves_other_arguments() {
        assert_eq!(
            selection_from_args(args(&["--open", "settings"])),
            Ok(FrontendSelection::DefaultEgui)
        );
    }

    #[test]
    fn selects_each_explicit_frontend() {
        assert_eq!(
            selection_from_args(args(&["--ui=egui"])),
            Ok(FrontendSelection::Explicit(Frontend::Egui))
        );
        assert_eq!(
            selection_from_args(args(&["--ui=gpui"])),
            Ok(FrontendSelection::Explicit(Frontend::Gpui))
        );
    }

    #[test]
    fn rejects_invalid_and_repeated_frontend_selectors() {
        assert!(selection_from_args(args(&["--ui=unknown"])).is_err());
        assert!(selection_from_args(args(&["--ui"])).is_err());
        assert!(selection_from_args(args(&["--ui=egui", "--ui=gpui"])).is_err());
        assert!(selection_from_args(args(&["--ui=egui", "--ui=egui"])).is_err());
    }

    #[test]
    fn duplicate_launch_diagnostic_is_silent_only_for_default_selection() {
        assert_eq!(
            duplicate_instance_message(FrontendSelection::DefaultEgui),
            None
        );
        assert_eq!(
            duplicate_instance_message(FrontendSelection::Explicit(Frontend::Egui)).as_deref(),
            Some(
                "The data directory is already owned by another Multi Launcher instance. Close that instance, then rerun with --ui=egui from the same working directory."
            )
        );
        assert_eq!(
            duplicate_instance_message(FrontendSelection::Explicit(Frontend::Gpui)).as_deref(),
            Some(
                "The data directory is already owned by another Multi Launcher instance. Close that instance, then rerun with --ui=gpui from the same working directory."
            )
        );
    }

    #[test]
    fn hash_mount_defaults_docked_and_restores_saved_popout() {
        assert_eq!(
            HashPresentationMode::from_saved(None),
            HashPresentationMode::Docked
        );
        assert_eq!(
            HashPresentationMode::from_saved(Some(RememberedPresentation::Popout)),
            HashPresentationMode::Popout
        );
        assert_eq!(
            HashPresentationMode::from_saved(Some(RememberedPresentation::Docked)),
            HashPresentationMode::Docked
        );
    }

    #[test]
    fn result_wheel_hands_off_only_at_scroll_boundaries() {
        assert!(super::inner_results_consumed_scroll(0.0, 100.0, -10.0));
        assert!(super::inner_results_consumed_scroll(-95.0, 100.0, -10.0));
        assert!(!super::inner_results_consumed_scroll(-100.0, 100.0, -10.0));
        assert!(super::inner_results_consumed_scroll(-50.0, 100.0, 10.0));
        assert!(!super::inner_results_consumed_scroll(0.0, 100.0, 10.0));
    }

    #[test]
    fn hash_menu_retains_focus_only_for_owned_docked_mount() {
        assert!(super::hash_menu_focus_is_owned(true, true, true));
        assert!(!super::hash_menu_focus_is_owned(false, true, true));
        assert!(!super::hash_menu_focus_is_owned(true, false, true));
        assert!(!super::hash_menu_focus_is_owned(true, true, false));
    }

    #[test]
    fn launcher_result_grid_geometry_bounds_gaps_and_equal_cells() {
        assert_eq!(launcher_result_grid_geometry(359.0, 3, 6.0, 1.0), (6.0, 115.0));
        assert_eq!(launcher_result_grid_geometry(10.0, 3, 8.0, 1.0), (5.0, 0.0));
        assert_eq!(launcher_result_grid_geometry(0.0, 3, 8.0, 1.0), (0.0, 0.0));
        assert_eq!(launcher_result_grid_geometry(10.0, 1, 8.0, 1.0), (0.0, 10.0));
        assert_eq!(launcher_result_grid_geometry(3.0, 3, 8.0, 1.5), (2.0 / 1.5, 0.0));
        assert_eq!(super::launcher_snapped_length(7.5, 1.0), 7.0);
    }

    #[test]
    fn launcher_column_width_fits_viewport_padding() {
        assert_eq!(launcher_column_width(800.0, 12.0, 1.0), 520.0);
        assert_eq!(launcher_column_width(544.0, 12.0, 1.0), 520.0);
        assert_eq!(launcher_column_width(400.0, 12.0, 1.0), 376.0);
        assert_eq!(launcher_column_width(20.0, 12.0, 1.0), 1.0);
        assert_eq!(launcher_column_width(400.0, 8.0, 1.2), 460.0 / 1.2);
        assert_eq!(launcher_column_width(800.0, 8.0, 1.2), 520.0);
        assert_eq!(launcher_column_width(10.0, 8.0, 1.2), 1.0 / 1.2);
        assert_eq!(super::launcher_search_width(800.0, 12.0, 1.0), 776.0);
        assert_eq!(super::launcher_search_width(400.0, 12.0, 1.0), 376.0);
    }

    #[test]
    fn docked_hash_fit_is_transient_and_handles_viewports_below_settings_limits() {
        let preferred = PreferredWindowSize::new(620.0, 440.0).unwrap();
        let available = root_scroll_viewport_size(Bounds::new(point(px(0.0), px(0.0)), gpui_kit::size(px(16.0), px(6.0))));
        assert_eq!(
            available,
            AppliedWindowSize {
                width: 16.0,
                height: 6.0
            }
        );
        assert_eq!(
            docked_hash_fit(preferred, available),
            AppliedWindowSize {
                width: 16.0,
                height: 6.0
            }
        );
        assert_eq!(preferred.width, 620.0);
        assert_eq!(preferred.height, 440.0);
    }

    #[test]
    fn docked_hash_resize_callbacks_require_the_active_docked_mount_and_session() {
        assert!(docked_hash_resize_identity_matches(7, 7, 3, 3, true));
        assert!(!docked_hash_resize_identity_matches(8, 7, 3, 3, true));
        assert!(!docked_hash_resize_identity_matches(7, 7, 4, 3, true));
        assert!(!docked_hash_resize_identity_matches(7, 7, 3, 3, false));
        assert!(!docked_hash_resize_identity_matches(0, 0, 3, 3, true));
    }

    #[test]
    fn docked_pointer_resize_commits_one_changed_axis_and_respects_applied_fit() {
        let preferred = PreferredWindowSize::new(620.0, 440.0).unwrap();
        let visible = AppliedWindowSize {
            width: 430.0,
            height: 300.0,
        };
        let (preview, changed_width, changed_height) =
            pointer_docked_resize_preview(preferred, visible, 2.0, 0.0);
        assert_eq!(preview, PreferredWindowSize::new(432.0, 440.0).unwrap());
        assert!(changed_width);
        assert!(!changed_height);

        let constrained = DockedHashResize {
            mount_generation: 7,
            session_epoch: 3,
            start_size: preferred,
            available_at_start: visible,
            preview_size: preview,
            changed_width,
            changed_height,
            drag_started: true,
            origin: DockedHashResizeOrigin::Pointer {
                position: point(px(0.0), px(0.0)),
                visible_size: visible,
            },
        };
        assert_eq!(docked_resize_commit_size(constrained), None);

        let click_without_drag = DockedHashResize {
            mount_generation: 7,
            session_epoch: 3,
            start_size: preferred,
            available_at_start: visible,
            preview_size: preferred,
            changed_width: false,
            changed_height: false,
            drag_started: false,
            origin: DockedHashResizeOrigin::Pointer {
                position: point(px(0.0), px(0.0)),
                visible_size: visible,
            },
        };
        assert_eq!(docked_resize_commit_size(click_without_drag), None);

        let room = AppliedWindowSize {
            width: 900.0,
            height: 700.0,
        };
        let visible = docked_hash_fit(preferred, room);
        let (preview, changed_width, changed_height) =
            pointer_docked_resize_preview(preferred, visible, 32.0, 0.0);
        let committed = docked_resize_commit_size(DockedHashResize {
            mount_generation: 7,
            session_epoch: 3,
            start_size: preferred,
            available_at_start: room,
            preview_size: preview,
            changed_width,
            changed_height,
            drag_started: true,
            origin: DockedHashResizeOrigin::Pointer {
                position: point(px(0.0), px(0.0)),
                visible_size: visible,
            },
        });
        assert_eq!(
            committed,
            Some(PreferredWindowSize::new(652.0, 440.0).unwrap())
        );
    }

    #[test]
    fn docked_keyboard_resize_keeps_the_untouched_legacy_axis() {
        let preferred = PreferredWindowSize::new(400.0, 440.0).unwrap();
        let preview = keyboard_docked_resize_preview(preferred, 0.0, 16.0);
        assert_eq!(preview, PreferredWindowSize::new(400.0, 456.0).unwrap());
        let available = AppliedWindowSize {
            width: 8192.0,
            height: 8192.0,
        };
        assert_eq!(
            docked_resize_commit_size(DockedHashResize {
                mount_generation: 9,
                session_epoch: 2,
                start_size: preferred,
                available_at_start: available,
                preview_size: preview,
                changed_width: false,
                changed_height: true,
                drag_started: false,
                origin: DockedHashResizeOrigin::Keyboard,
            }),
            Some(PreferredWindowSize::new(400.0, 456.0).unwrap())
        );

        let clipped = AppliedWindowSize {
            width: 430.0,
            height: 300.0,
        };
        let clipped_preference = PreferredWindowSize::new(620.0, 440.0).unwrap();
        assert_eq!(
            docked_resize_commit_size(DockedHashResize {
                mount_generation: 9,
                session_epoch: 2,
                start_size: clipped_preference,
                available_at_start: clipped,
                preview_size: keyboard_docked_resize_preview(clipped_preference, 16.0, 0.0),
                changed_width: true,
                changed_height: false,
                drag_started: false,
                origin: DockedHashResizeOrigin::Keyboard,
            }),
            None,
            "a keyboard preview that remains clamped to the same root-fit width must not persist"
        );
    }

    #[test]
    fn docked_resize_accessibility_label_reports_current_and_preview_dimensions() {
        let label = docked_hash_resize_accessibility_label(
            PreferredWindowSize::new(620.0, 440.0).unwrap(),
            AppliedWindowSize {
                width: 430.0,
                height: 300.0,
            },
            PreferredWindowSize::new(636.0, 456.0).unwrap(),
            AppliedWindowSize {
                width: 430.0,
                height: 300.0,
            },
            true,
        );
        assert!(label.contains("Current preferred size: 620 by 440 logical points"));
        assert!(label.contains("displayed 430 by 300"));
        assert!(label.contains("Preview size: 636 by 456 logical points"));
        assert!(label.contains("Enter to save, Escape to discard"));
    }

    #[test]
    fn hash_mount_callbacks_require_current_mount_and_session() {
        assert!(hash_mount_identity_matches(4, 4, 7, 7));
        assert!(!hash_mount_identity_matches(5, 4, 7, 7));
        assert!(!hash_mount_identity_matches(4, 4, 8, 7));
        assert!(!hash_mount_identity_matches(0, 0, 7, 7));
    }

    #[test]
    fn hash_file_picker_completion_requires_mount_session_and_source_generation() {
        assert!(hash_picker_result_is_current(4, 4, 7, 7, 11, 11));
        assert!(!hash_picker_result_is_current(5, 4, 7, 7, 11, 11));
        assert!(!hash_picker_result_is_current(4, 4, 8, 7, 11, 11));
        assert!(!hash_picker_result_is_current(4, 4, 7, 7, 12, 11));
    }

    #[test]
    fn hash_clipboard_completion_requires_mount_session_and_source_generation() {
        let request = HashClipboardRequest {
            mount_generation: 4,
            session_epoch: 7,
            source_generation: 11,
        };
        assert!(hash_clipboard_request_is_current(4, 7, 11, request, false));
        assert!(!hash_clipboard_request_is_current(5, 7, 11, request, false));
        assert!(!hash_clipboard_request_is_current(4, 8, 11, request, false));
        assert!(!hash_clipboard_request_is_current(4, 7, 12, request, false));
        assert!(!hash_clipboard_request_is_current(4, 7, 11, request, true));
    }

    #[test]
    fn hash_clipboard_slot_coalesces_and_copy_excludes_picker_in_both_orders() {
        let pending = HashClipboardPendingState::default();
        assert!(pending.begin(HashClipboardPendingKind::Load));
        assert!(!pending.begin(HashClipboardPendingKind::Copy));
        pending.finish();
        assert!(pending.begin(HashClipboardPendingKind::Copy));
        assert!(!pending.begin(HashClipboardPendingKind::Load));
        pending.finish();

        assert!(hash_clipboard_copy_admissible(
            HashClipboardPendingKind::Idle,
            false,
        ));
        assert!(!hash_clipboard_copy_admissible(
            HashClipboardPendingKind::Idle,
            true,
        ));
        assert!(!hash_clipboard_copy_admissible(
            HashClipboardPendingKind::Load,
            false,
        ));
        assert!(!hash_clipboard_copy_admissible(
            HashClipboardPendingKind::Copy,
            false,
        ));
        assert!(hash_file_picker_admissible(
            HashClipboardPendingKind::Idle,
            false,
        ));
        assert!(!hash_file_picker_admissible(
            HashClipboardPendingKind::Copy,
            false,
        ));
        assert!(!hash_file_picker_admissible(
            HashClipboardPendingKind::Idle,
            true,
        ));
    }

    #[test]
    fn restart_draft_keeps_only_opaque_clipboard_snapshot_identity() {
        let mut session = multi_launcher::hash_generator::HashSession::new();
        session.apply_clipboard_load(Ok("secret".to_owned()));
        let first = HashDraft::from_session(&session);
        session.apply_clipboard_load(Ok("public".to_owned()));
        let replacement = HashDraft::from_session(&session);

        assert_ne!(first, replacement);
        assert_ne!(
            first.clipboard_snapshot_revision,
            replacement.clipboard_snapshot_revision
        );
        let serialized_for_diagnostics = format!("{first:?}");
        assert!(!serialized_for_diagnostics.contains("secret"));
        assert!(!serialized_for_diagnostics.contains("public"));
    }

    #[test]
    fn hash_file_picker_completion_classifies_selection_cancel_and_failure() {
        let selected_path = PathBuf::from("sample.txt");
        assert!(matches!(
            classify_hash_picker_result::<std::io::Error>(Ok(Ok(Some(vec![
                selected_path.clone()
            ])))),
            HashPickerOutcome::Selected(path) if path == selected_path
        ));
        assert!(matches!(
            classify_hash_picker_result::<std::io::Error>(Ok(Ok(None))),
            HashPickerOutcome::Cancelled
        ));
        assert!(matches!(
            classify_hash_picker_result::<std::io::Error>(Ok(Ok(Some(Vec::new())))),
            HashPickerOutcome::Failed(_)
        ));
        assert!(matches!(
            classify_hash_picker_result::<std::io::Error>(Ok(Err(anyhow::anyhow!(
                "native dialog failed"
            )))),
            HashPickerOutcome::Failed(message) if message.contains("native dialog failed")
        ));
        assert!(matches!(
            classify_hash_picker_result::<std::io::Error>(Err(std::io::Error::other(
                "dialog task failed"
            ))),
            HashPickerOutcome::Failed(message) if message.contains("did not return")
        ));
    }

    #[test]
    fn expected_digest_newlines_remain_invalid_internal_separators() {
        let digest = "a".repeat(64);
        let split = format!("{}\n{}", &digest[..32], &digest[32..]);
        assert!(parse_expected_digest(HashAlgorithm::Sha256, &split).is_err());
        assert!(
            parse_expected_digest(HashAlgorithm::Sha256, &format!("\r\n{digest}\t"))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn dock_reveal_focus_is_fenced_to_the_current_mount() {
        assert!(docked_hash_focus_is_current(Some(8), 8, Some(8)));
        assert!(!docked_hash_focus_is_current(Some(8), 9, Some(8)));
        assert!(!docked_hash_focus_is_current(Some(8), 8, Some(7)));
        assert!(!docked_hash_focus_is_current(Some(8), 8, None));
        assert!(!docked_hash_focus_is_current(None, 8, Some(8)));
    }

    #[test]
    fn hidden_reveal_yields_to_pending_second_toggle_without_losing_parity() {
        let visibility = super::RootVisibility::new();
        visibility.active.store(true, std::sync::atomic::Ordering::Release);
        visibility.visible.store(false, std::sync::atomic::Ordering::Release);
        let (first, visible) = visibility.next_toggle();
        assert!(visible);
        assert!(super::reveal_edge_is_current(visibility.is_current(first, true), false));
        assert!(!super::reveal_edge_is_current(visibility.is_current(first, true), true));
        // Leave the accepted first bit intact until the queued edge is drained.
        let trigger = multi_launcher::hotkey::runtime::HotkeyTrigger::new(multi_launcher::hotkey::parse_hotkey("F2").unwrap());
        *trigger.open.lock().unwrap() = true;
        assert!(!super::rollback_reveal_unless_hotkey_pending(&visibility, first, &trigger));
        assert!(trigger.take());
        let (_, visible) = visibility.next_toggle();
        assert!(!visible);
        assert!(!super::reveal_edge_is_current(visibility.is_current(first, true), false));
        let (fresh, _) = visibility.next_toggle();
        assert!(super::rollback_reveal_unless_hotkey_pending(&visibility, fresh, &trigger));
        let (_, visible) = visibility.next_toggle();
        assert!(visible);
    }

    #[test]
    fn queued_window_hide_requires_exact_target_and_query_and_yields_to_hotkey() {
        let target = super::ForegroundIdentity { hwnd: 42, process_id: 2, thread_id: 3 };
        let other = super::ForegroundIdentity { hwnd: 43, ..target };
        assert!(super::window_switch_hide_is_current(target, Some(target), Some(""), Some("")));
        assert!(!super::window_switch_hide_is_current(target, Some(other), Some(""), Some("")));
        assert!(!super::window_switch_hide_is_current(target, Some(target), Some(""), Some("new")));
        let request = super::RootVisibilityRequest::HideAfterWindowSwitch(5, target, Arc::from(""));
        assert!(request.is_current(5, true, false));
        assert!(!request.is_current(6, true, false));
        assert!(request.superseded_by_hotkey(|| true));
    }

    #[test]
    fn clipboard_undo_hide_requires_exact_live_receipt_and_yields_to_hotkey() {
        let root = super::ForegroundIdentity { hwnd: 52, process_id: 2, thread_id: 3 };
        let other = super::ForegroundIdentity { hwnd: 53, ..root };
        let check = super::clipboard_undo_hide_is_current;
        assert!(check(root, Some(root), 7, 7, "cm undo", "cm undo", true));
        assert!(!check(root, Some(other), 7, 7, "cm undo", "cm undo", true));
        assert!(!check(root, Some(root), 7, 8, "cm undo", "cm undo", true));
        assert!(!check(root, Some(root), 7, 7, "cm undo", "cm", true));
        assert!(!check(root, Some(root), 7, 7, "cm undo", "cm undo", false));
        let request = super::RootVisibilityRequest::HideAfterClipboardUndo(5, root, 7, Arc::from("cm undo"), Arc::new(std::sync::atomic::AtomicBool::new(true)), None, None);
        assert!(request.is_current(5, true, false));
        assert!(request.superseded_by_hotkey(|| true));
        assert!(!request.is_current(6, true, false));
    }

    #[cfg(windows)]
    #[test]
    fn clipboard_undo_hide_yields_to_explicit_dock_in_both_orders() {
        let visibility = super::RootVisibility::new();
        let (tx, _rx) = std::sync::mpsc::channel();
        visibility.bind_request_sender(tx);
        assert!(visibility.activate());
        let root = super::ForegroundIdentity { hwnd: 1, process_id: 2, thread_id: 3 };
        let gesture = super::hash_dock::Gesture { source: super::NativeWindow { identity: root }, foreground_authorized: true, visibility_revision: 0 };
        let lease = || Arc::new(std::sync::atomic::AtomicBool::new(true));
        let first = visibility.request_hash_visible(gesture, 1).unwrap();
        assert!(!visibility.request_clipboard_undo_hide(0, root, 1, "cm undo", lease(), None, None));
        assert!(!first.state.lock().unwrap().closed);
        assert!(matches!(visibility.take_requested_visible(), Some(super::RootVisibilityRequest::DockHash(_))));
        assert!(visibility.request_clipboard_undo_hide(0, root, 1, "cm undo", lease(), None, None));
        let second = visibility.request_hash_visible(gesture, 1).unwrap();
        assert!(matches!(visibility.take_requested_visible(), Some(super::RootVisibilityRequest::DockHash(_))));
        assert!(second.state.lock().unwrap().closed);
        assert!(visibility.request_clipboard_undo_hide(0, root, 1, "cm undo", lease(), None, None));
        let owner = super::utility_mount::MountOwner::default();
        let (utility, _) = visibility.request_xml_visible(gesture, owner.attach()).unwrap();
        assert!(matches!(visibility.take_requested_visible(), Some(super::RootVisibilityRequest::DockUtility(_, _))));
        assert!(utility.state.lock().unwrap().closed);
    }

    #[test]
    fn accepted_close_hide_excludes_only_the_exact_closed_return_target() {
        let target = super::ForegroundIdentity { hwnd: 42, process_id: 2, thread_id: 3 };
        let other_process = super::ForegroundIdentity { process_id: 4, ..target };
        let other_thread = super::ForegroundIdentity { thread_id: 5, ..target };
        assert_eq!(super::exclude_closed_return_target(Some(target), target), None);
        assert_eq!(super::exclude_closed_return_target(Some(other_process), target), Some(other_process));
        assert_eq!(super::exclude_closed_return_target(Some(other_thread), target), Some(other_thread));
        assert_eq!(super::exclude_closed_return_target(None, target), None);
        let request = super::RootVisibilityRequest::HideAfterWindowClose(5, target, target, Arc::from("query"));
        assert!(request.is_current(5, true, false));
        assert!(!request.is_current(6, true, false));
        assert!(request.superseded_by_hotkey(|| true));
        assert!(super::window_close_hide_is_current(target, Some(target), "query", Some("query")));
        assert!(!super::window_close_hide_is_current(target, Some(other_process), "query", Some("query")));
        assert!(!super::window_close_hide_is_current(target, Some(target), "query", Some("changed")));
    }

    #[test]
    fn root_search_focus_restoration_respects_mount_and_restart_fences() {
        assert!(root_search_focus_is_current(
            Some(HashPresentationMode::Popout),
            Some(9),
            9,
            Some(HashPresentationMode::Popout),
            9,
            false,
        ));
        assert!(!root_search_focus_is_current(
            Some(HashPresentationMode::Popout),
            Some(10),
            10,
            Some(HashPresentationMode::Popout),
            9,
            false,
        ));
        assert!(root_search_focus_is_current(None, None, 0, None, 0, false));
        assert!(!root_search_focus_is_current(None, None, 0, None, 0, true,));
        assert!(!root_search_focus_is_current(
            Some(HashPresentationMode::Docked),
            Some(9),
            9,
            None,
            0,
            false,
        ));
    }

    #[test]
    fn hash_wake_coalesces_notifications_and_closes_waiter() {
        let signal = Arc::new(HashWakeSignal::default());
        let wake_counter = Arc::new(WakeCounter::default());
        let waker = Waker::from(Arc::clone(&wake_counter));
        let mut wait = Box::pin(signal.wait());
        let mut cx = FutureContext::from_waker(&waker);

        assert!(matches!(wait.as_mut().poll(&mut cx), Poll::Pending));
        signal.notify();
        signal.notify();
        assert_eq!(wake_counter.0.load(Ordering::Relaxed), 1);
        assert!(matches!(wait.as_mut().poll(&mut cx), Poll::Ready(true)));
        assert!(matches!(wait.as_mut().poll(&mut cx), Poll::Pending));
        signal.close();
        assert_eq!(wake_counter.0.load(Ordering::Relaxed), 2);
        assert!(matches!(wait.as_mut().poll(&mut cx), Poll::Ready(false)));
    }
}
