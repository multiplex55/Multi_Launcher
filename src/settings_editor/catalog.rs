use crate::plugin::{PluginCatalogOrigin, PluginCatalogSnapshot};
use std::collections::BTreeSet;

/// Persistent Settings section identities shared by rendering and search metadata.
pub mod ids {
    pub const SECTION_HOTKEYS: &str = "settings_section_hotkeys";
    pub const SECTION_PASTE_SEQUENCES: &str = "settings_section_paste_sequences";
    pub const SECTION_LAUNCHER_WINDOW: &str = "settings_section_launcher_window";
    pub const SECTION_SEARCH_RESULTS: &str = "settings_section_search_results";
    pub const SECTION_ACTIONS_FEEDBACK: &str = "settings_section_actions_feedback";
    pub const SECTION_DASHBOARD: &str = "settings_section_dashboard";
    pub const SECTION_RADIAL: &str = "settings_section_radial";
    pub const SECTION_PLUGINS: &str = "settings_section_plugins";
    pub const NOTES: &str = "plugin_notes";

    pub const HOTKEY_LAUNCHER: &str = "hotkeys.launcher";
    pub const HOTKEY_QUIT_ENABLED: &str = "hotkeys.quit.enabled";
    pub const HOTKEY_QUIT_VALUE: &str = "hotkeys.quit.value";
    pub const HOTKEY_HELP_ENABLED: &str = "hotkeys.help.enabled";
    pub const HOTKEY_HELP_VALUE: &str = "hotkeys.help.value";
    pub const PASTE_NEXT_ENABLED: &str = "paste.next.enabled";
    pub const PASTE_NEXT_HOTKEY: &str = "paste.next.hotkey";
    pub const PASTE_CANCEL_ENABLED: &str = "paste.cancel.enabled";
    pub const PASTE_CANCEL_HOTKEY: &str = "paste.cancel.hotkey";
    pub const PASTE_ESCAPE_CANCEL: &str = "paste.escape_cancel";
    pub const PASTE_KEEP_VISIBLE: &str = "paste.keep_visible";
    pub const LAUNCHER_ALWAYS_ON_TOP: &str = "launcher.always_on_top";
    pub const LAUNCHER_SHOW_ICON: &str = "launcher.show_icon";
    pub const LAUNCHER_SHOW_TITLE: &str = "launcher.show_title";
    pub const LAUNCHER_JUMP_MODE: &str = "launcher.experimental_jump_mode";
    pub const LAUNCHER_FOLLOW_MOUSE: &str = "launcher.follow_mouse";
    pub const LAUNCHER_STATIC_POSITION: &str = "launcher.static_position";
    pub const LAUNCHER_QUERY_SCALE: &str = "launcher.query_scale";
    pub const LAUNCHER_QUERY_SCALE_VALUE: &str = "launcher.query_scale/value";
    pub const LAUNCHER_LIST_SCALE: &str = "launcher.list_scale";
    pub const LAUNCHER_LIST_SCALE_VALUE: &str = "launcher.list_scale/value";
    pub const LAUNCHER_THEME_SETTINGS: &str = "launcher.open_theme_settings";
    pub const SEARCH_AUTOCOMPLETE: &str = "search.autocomplete";
    pub const SEARCH_FUZZY_WEIGHT: &str = "search.fuzzy_weight";
    pub const SEARCH_FUZZY_WEIGHT_VALUE: &str = "search.fuzzy_weight/value";
    pub const SEARCH_USAGE_WEIGHT: &str = "search.usage_weight";
    pub const SEARCH_USAGE_WEIGHT_VALUE: &str = "search.usage_weight/value";
    pub const SEARCH_MATCH_EXACT: &str = "search.match_exact";
    pub const SEARCH_GRID_LAYOUT: &str = "search.grid_layout";
    pub const SEARCH_RESPECT_PLUGIN_CAPABILITY: &str = "search.respect_plugin_capability";
    pub const SEARCH_PLUGIN_OPT_OUT: &str = "search.plugin_opt_out";
    pub const ACTIONS_HIDE_AFTER_RUN: &str = "actions.hide_after_run";
    pub const ACTIONS_PRESERVE_COMMAND: &str = "actions.preserve_command";
    pub const ACTIONS_CLEAR_QUERY: &str = "actions.clear_query_after_run";
    pub const ACTIONS_CONFIRM_DESTRUCTIVE: &str = "actions.confirm_destructive";
    pub const FEEDBACK_TOASTS: &str = "feedback.toasts";
    pub const FEEDBACK_TOAST_DURATION: &str = "feedback.toast_duration";
    pub const FEEDBACK_TOAST_DURATION_VALUE: &str = "feedback.toast_duration/value";
    pub const FEEDBACK_INLINE_ERRORS: &str = "feedback.inline_errors";
    pub const FEEDBACK_ERROR_TOASTS: &str = "feedback.error_toasts";
    pub const FEEDBACK_DEBUG_LOGGING: &str = "feedback.debug_logging";
    pub const FEEDBACK_DISABLE_TIMER_UPDATES: &str = "feedback.disable_timer_updates";
    pub const DASHBOARD_ENABLED: &str = "dashboard.enabled";
    pub const DASHBOARD_PATH: &str = "dashboard.path";
    pub const DASHBOARD_DEFAULT_LOCATION: &str = "dashboard.default_location";
    pub const DASHBOARD_SHOW_WHEN_EMPTY: &str = "dashboard.show_when_empty";
    pub const DASHBOARD_REDUCE_WORK: &str = "dashboard.reduce_work_unfocused";
    pub const DASHBOARD_DIAGNOSTICS: &str = "dashboard.diagnostics";
    pub const DASHBOARD_CUSTOMIZE: &str = "dashboard.customize";
    pub const RADIAL_ENABLED: &str = "radial.enabled";
    pub const RADIAL_SHARED_TAP_HOLD: &str = "radial.shared_tap_hold";
    pub const RADIAL_EXPECTED_LAYOUT_DIAGNOSTICS: &str = "radial.expected_layout_diagnostics";
    pub const RADIAL_GLOBAL_ITEM_INPUTS: &str = "radial.global_item_inputs";
    pub const RADIAL_EDIT_MENUS: &str = "radial.edit_menus";
    pub const RADIAL_EDIT_SKINS: &str = "radial.edit_skins";
    pub const RADIAL_RESTORE_SUBMENU: &str = "radial.restore_submenu_presentation";
    pub const PLUGINS_TOGGLE_SECTIONS: &str = "plugins.toggle_sections";
    pub const SECTIONS_EXPAND_ALL: &str = "sections.expand_all";
    pub const SECTIONS_COLLAPSE_ALL: &str = "sections.collapse_all";
    pub const FOOTER_SAVE: &str = "footer.save";
    pub const FOOTER_CANCEL: &str = "footer.cancel";
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MainSettingsSection {
    Hotkeys,
    PasteSequences,
    LauncherWindow,
    SearchResults,
    ActionsFeedback,
    Dashboard,
    Radial,
    Plugins,
}

impl MainSettingsSection {
    pub const ALL: [Self; 8] = [
        Self::Hotkeys,
        Self::PasteSequences,
        Self::LauncherWindow,
        Self::SearchResults,
        Self::ActionsFeedback,
        Self::Dashboard,
        Self::Radial,
        Self::Plugins,
    ];

    pub const fn persistent_id(self) -> &'static str {
        match self {
            Self::Hotkeys => ids::SECTION_HOTKEYS,
            Self::PasteSequences => ids::SECTION_PASTE_SEQUENCES,
            Self::LauncherWindow => ids::SECTION_LAUNCHER_WINDOW,
            Self::SearchResults => ids::SECTION_SEARCH_RESULTS,
            Self::ActionsFeedback => ids::SECTION_ACTIONS_FEEDBACK,
            Self::Dashboard => ids::SECTION_DASHBOARD,
            Self::Radial => ids::SECTION_RADIAL,
            Self::Plugins => ids::SECTION_PLUGINS,
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::Hotkeys => "Hotkeys",
            Self::PasteSequences => "Paste Sequences",
            Self::LauncherWindow => "Launcher Window & Appearance",
            Self::SearchResults => "Search & Results",
            Self::ActionsFeedback => "Actions, Safety & Feedback",
            Self::Dashboard => "Dashboard",
            Self::Radial => "Radial Menus",
            Self::Plugins => "Plugin Settings",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Hotkeys => "Set the launcher shortcut and optional quit/help shortcuts.",
            Self::PasteSequences => {
                "Configure playback controls. Saved sequence start shortcuts are edited in Paste Sequences."
            }
            Self::LauncherWindow => "Choose window placement behavior and query/list text scales.",
            Self::SearchResults => "Tune matching and the way search results are presented.",
            Self::ActionsFeedback => "Control action completion, safety prompts, and feedback.",
            Self::Dashboard => "Configure the empty-query dashboard and its refresh behavior.",
            Self::Radial => "Set radial timing, input scope, safety, and menu defaults.",
            Self::Plugins => "Configure settings for enabled plugins.",
        }
    }

    pub const fn default_open(self) -> bool {
        matches!(
            self,
            Self::Hotkeys | Self::LauncherWindow | Self::SearchResults
        )
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SettingsControlId(String);

impl SettingsControlId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum SettingsOwner {
    Application,
    Plugin {
        name: String,
        origin: PluginCatalogOrigin,
        epoch: Option<u64>,
    },
    Theme,
    Mouse,
    BrightnessDialog,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum SettingsSection {
    Main(MainSettingsSection),
    Notes,
    Plugin {
        name: String,
        origin: PluginCatalogOrigin,
        epoch: Option<u64>,
    },
    Theme,
    Mouse,
    MouseGestures,
    PluginEditor,
    Brightness,
}

impl SettingsSection {
    pub fn persistent_id(&self) -> String {
        match self {
            Self::Main(section) => section.persistent_id().to_owned(),
            Self::Notes => ids::NOTES.to_owned(),
            Self::Plugin { name, .. } => format!("plugin_{name}"),
            Self::Theme => "theme_settings".to_owned(),
            Self::Mouse => "mouse_settings".to_owned(),
            Self::MouseGestures => "mouse_gesture_settings".to_owned(),
            Self::PluginEditor => "plugin_editor".to_owned(),
            Self::Brightness => "brightness".to_owned(),
        }
    }

    pub fn title(&self) -> String {
        match self {
            Self::Main(section) => section.title().to_owned(),
            Self::Notes => "Notes / Markdown".to_owned(),
            Self::Plugin { name, .. } => format!("{name} settings"),
            Self::Theme => "Theme Settings".to_owned(),
            Self::Mouse => "Mouse Settings".to_owned(),
            Self::MouseGestures => "Mouse Gesture Settings".to_owned(),
            Self::PluginEditor => "Plugin Settings".to_owned(),
            Self::Brightness => "Brightness".to_owned(),
        }
    }

    pub fn breadcrumb(&self) -> Vec<String> {
        match self {
            Self::Main(section) => vec![section.title().to_owned()],
            Self::Notes => vec![
                MainSettingsSection::Plugins.title().to_owned(),
                "Notes / Markdown".to_owned(),
            ],
            Self::Plugin { name, .. } => vec![
                MainSettingsSection::Plugins.title().to_owned(),
                format!("{name} settings"),
            ],
            Self::Theme => vec!["Theme Settings".to_owned()],
            Self::Mouse => vec!["Mouse Settings".to_owned()],
            Self::MouseGestures => vec!["Mouse Gesture Settings".to_owned()],
            Self::PluginEditor => vec!["Plugin Settings".to_owned()],
            Self::Brightness => vec!["Brightness".to_owned()],
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SettingsAvailability {
    Available,
    Conditional { prerequisite: String },
    Unavailable { prerequisite: String },
}

impl SettingsAvailability {
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsCatalogEntry {
    pub id: SettingsControlId,
    pub owner: SettingsOwner,
    pub section: SettingsSection,
    pub label: String,
    pub help: String,
    pub synonyms: Vec<String>,
    pub breadcrumb: Vec<String>,
    /// Human-readable dependency for controls which appear only after a user
    /// enables a related setting. No current setting values are stored here.
    pub availability: SettingsAvailability,
    normalized_search: String,
}

#[derive(Clone, Debug)]
pub struct SettingsCatalog {
    plugin_revision: u64,
    entries: Vec<SettingsCatalogEntry>,
}

impl SettingsCatalog {
    pub(crate) fn from_plugin_snapshot(snapshot: &PluginCatalogSnapshot) -> Self {
        let mut seen_plugin_names = BTreeSet::new();
        let mut ambiguous_plugin_names = BTreeSet::new();
        for plugin in &snapshot.plugins {
            if !seen_plugin_names.insert(plugin.name.as_str()) {
                ambiguous_plugin_names.insert(plugin.name.as_str());
            }
        }

        let mut entries = Vec::with_capacity(SEEDS.len() + snapshot.plugins.len() * 3);
        for seed in SEEDS {
            let (owner, section, availability) = seed.resolve(snapshot, &ambiguous_plugin_names);
            entries.push(build_entry(
                seed.id,
                owner,
                section,
                seed.label,
                seed.help,
                seed.synonyms,
                availability,
            ));
        }

        for plugin in &snapshot.plugins {
            if plugin.origin != PluginCatalogOrigin::Builtin
                && plugin.has_settings
                && plugin.name != "notes"
                && plugin.name != "mouse_gestures"
            {
                let owner = SettingsOwner::Plugin {
                    name: plugin.name.clone(),
                    origin: plugin.origin,
                    epoch: Some(plugin.epoch),
                };
                let section = SettingsSection::Plugin {
                    name: plugin.name.clone(),
                    origin: plugin.origin,
                    epoch: Some(plugin.epoch),
                };
                let availability = if ambiguous_plugin_names.contains(plugin.name.as_str()) {
                    ambiguous_plugin_availability(&plugin.name)
                } else if plugin.enabled {
                    SettingsAvailability::Available
                } else {
                    SettingsAvailability::Unavailable {
                        prerequisite: format!("Enable {} in Plugin Settings first", plugin.name),
                    }
                };
                let stable_origin = match plugin.origin {
                    PluginCatalogOrigin::Builtin => "builtin",
                    PluginCatalogOrigin::DynamicLibrary => "dynamic",
                    PluginCatalogOrigin::Registered => "registered",
                };
                entries.push(build_entry(
                    &format!(
                        "plugin.{stable_origin}.{}.{}.settings",
                        plugin.name, plugin.epoch
                    ),
                    owner,
                    section,
                    &format!("{} settings", plugin.name),
                    "Open the settings section provided by this registered plugin.",
                    &["plugin preferences", "configuration", "options"],
                    availability,
                ));
            }

            let owner = SettingsOwner::Plugin {
                name: plugin.name.clone(),
                origin: plugin.origin,
                epoch: Some(plugin.epoch),
            };
            let plugin_editor_section = SettingsSection::PluginEditor;
            let origin = match plugin.origin {
                PluginCatalogOrigin::Builtin => "builtin",
                PluginCatalogOrigin::DynamicLibrary => "dynamic",
                PluginCatalogOrigin::Registered => "registered",
            };
            let editor_id = format!(
                "plugin_editor.{origin}.{}.{}.enabled",
                plugin.name, plugin.epoch
            );
            entries.push(build_entry(
                &editor_id,
                owner.clone(),
                plugin_editor_section.clone(),
                &format!("Enable {} plugin", plugin.name),
                "Plugin enablement in Plugin Settings. Search only reveals this control; it never changes enablement.",
                &["plugin", "enabled", "disable", "turn on", "turn off"],
                if ambiguous_plugin_names.contains(plugin.name.as_str()) {
                    ambiguous_plugin_availability(&plugin.name)
                } else {
                    SettingsAvailability::Available
                },
            ));
            for capability in &plugin.capabilities {
                let capability_words = capability.replace('_', " ");
                let capability_id = format!(
                    "plugin_editor.{origin}.{}.{}.capability.{}",
                    plugin.name, plugin.epoch, capability
                );
                let availability = if ambiguous_plugin_names.contains(plugin.name.as_str()) {
                    ambiguous_plugin_availability(&plugin.name)
                } else if plugin.enabled {
                    SettingsAvailability::Available
                } else {
                    SettingsAvailability::Unavailable {
                        prerequisite: format!("Enable {} in Plugin Settings first", plugin.name),
                    }
                };
                entries.push(build_entry(
                    &capability_id,
                    owner.clone(),
                    plugin_editor_section.clone(),
                    &format!("{} capability: {capability}", plugin.name),
                    "Capability preference for this plugin. It is not changed by searching.",
                    &[
                        capability.as_str(),
                        capability_words.as_str(),
                        "plugin capability",
                        "permission",
                        "support",
                    ],
                    availability,
                ));
            }
        }

        entries.sort_by(|left, right| left.id.cmp(&right.id));
        Self {
            plugin_revision: snapshot.revision,
            entries,
        }
    }

    pub fn entries(&self) -> &[SettingsCatalogEntry] {
        &self.entries
    }

    pub fn entry(&self, id: &str) -> Option<&SettingsCatalogEntry> {
        self.entries.iter().find(|entry| entry.id.as_str() == id)
    }

    pub fn plugin_revision(&self) -> u64 {
        self.plugin_revision
    }

    fn is_current_for(&self, snapshot: &PluginCatalogSnapshot) -> bool {
        self.plugin_revision == snapshot.revision
    }

    /// Resolve availability against the latest registration snapshot. A
    /// plugin-owned destination from an older catalog revision is fail-closed
    /// until its metadata is refreshed.
    pub(crate) fn availability_for(
        &self,
        entry: &SettingsCatalogEntry,
        snapshot: &PluginCatalogSnapshot,
    ) -> SettingsAvailability {
        let SettingsOwner::Plugin {
            name,
            origin,
            epoch,
        } = &entry.owner
        else {
            return entry.availability.clone();
        };

        let Some(epoch) = epoch else {
            return entry.availability.clone();
        };
        let registered = snapshot.plugins.iter().find(|plugin| {
            plugin.name == *name && plugin.origin == *origin && plugin.epoch == *epoch
        });
        if registered.is_none() || !self.is_current_for(snapshot) {
            return SettingsAvailability::Unavailable {
                prerequisite: "Plugin registration changed; refresh the Settings catalog.".into(),
            };
        }
        entry.availability.clone()
    }

    /// Match normalized query words against cached, non-sensitive catalog
    /// metadata. Search does not access PluginManager or provider code.
    pub fn search(&self, query: &str) -> Vec<&SettingsCatalogEntry> {
        let normalized_query = normalize_query(query);
        if normalized_query.is_empty() {
            return Vec::new();
        }
        let terms = normalized_query.split_whitespace().collect::<Vec<_>>();
        self.entries
            .iter()
            .filter(|entry| {
                terms
                    .iter()
                    .all(|term| entry.normalized_search.contains(term))
            })
            .collect()
    }

    pub fn has_unique_control_ids(&self) -> bool {
        let mut ids = BTreeSet::new();
        self.entries
            .iter()
            .all(|entry| ids.insert(entry.id.as_str()))
    }
}

impl Default for SettingsCatalog {
    fn default() -> Self {
        Self::from_plugin_snapshot(&PluginCatalogSnapshot::default())
    }
}

fn build_entry(
    id: &str,
    owner: SettingsOwner,
    section: SettingsSection,
    label: &str,
    help: &str,
    synonyms: &[&str],
    availability: SettingsAvailability,
) -> SettingsCatalogEntry {
    let section_title = section.title();
    let breadcrumb = section.breadcrumb();
    let normalized_search = normalize_query(&format!(
        "{} {section_title} {label} {help} {}",
        breadcrumb.join(" "),
        synonyms.join(" ")
    ));
    SettingsCatalogEntry {
        id: SettingsControlId(id.to_owned()),
        owner,
        section,
        label: label.to_owned(),
        help: help.to_owned(),
        synonyms: synonyms
            .iter()
            .map(|synonym| (*synonym).to_owned())
            .collect(),
        breadcrumb,
        availability,
        normalized_search,
    }
}

fn normalize_query(value: &str) -> String {
    value
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Clone, Copy)]
enum SeedOwner {
    Application,
    BuiltinPlugin(&'static str),
    Theme,
    Mouse,
    Brightness,
}

#[derive(Clone, Copy)]
enum SeedSection {
    Main(MainSettingsSection),
    Notes,
    Plugin,
    Theme,
    Mouse,
    MouseGestures,
    Brightness,
}

#[derive(Clone, Copy)]
struct Seed {
    id: &'static str,
    owner: SeedOwner,
    section: SeedSection,
    label: &'static str,
    help: &'static str,
    synonyms: &'static [&'static str],
    prerequisite: Option<&'static str>,
}

impl Seed {
    fn resolve(
        &self,
        snapshot: &PluginCatalogSnapshot,
        ambiguous_plugin_names: &BTreeSet<&str>,
    ) -> (SettingsOwner, SettingsSection, SettingsAvailability) {
        let section = match self.section {
            SeedSection::Main(section) => SettingsSection::Main(section),
            SeedSection::Notes => SettingsSection::Notes,
            SeedSection::Theme => SettingsSection::Theme,
            SeedSection::Mouse => SettingsSection::Mouse,
            SeedSection::MouseGestures => SettingsSection::MouseGestures,
            SeedSection::Brightness => SettingsSection::Brightness,
            SeedSection::Plugin => match self.owner {
                SeedOwner::BuiltinPlugin(name) => {
                    let item = snapshot.plugins.iter().find(|plugin| {
                        plugin.name == name && plugin.origin == PluginCatalogOrigin::Builtin
                    });
                    SettingsSection::Plugin {
                        name: name.to_owned(),
                        origin: PluginCatalogOrigin::Builtin,
                        epoch: item.map(|item| item.epoch),
                    }
                }
                _ => unreachable!("plugin section requires a plugin owner"),
            },
        };

        let (owner, availability) = match self.owner {
            SeedOwner::Application => (SettingsOwner::Application, condition(self.prerequisite)),
            SeedOwner::Theme => (SettingsOwner::Theme, condition(self.prerequisite)),
            SeedOwner::Mouse => (SettingsOwner::Mouse, condition(self.prerequisite)),
            SeedOwner::Brightness => (
                SettingsOwner::BrightnessDialog,
                condition(self.prerequisite),
            ),
            SeedOwner::BuiltinPlugin(name) => {
                let item = snapshot.plugins.iter().find(|plugin| {
                    plugin.name == name && plugin.origin == PluginCatalogOrigin::Builtin
                });
                let owner = SettingsOwner::Plugin {
                    name: name.to_owned(),
                    origin: PluginCatalogOrigin::Builtin,
                    epoch: item.map(|item| item.epoch),
                };
                let availability = match item {
                    None => SettingsAvailability::Unavailable {
                        prerequisite: format!("Builtin plugin {name} is not loaded."),
                    },
                    Some(_) if ambiguous_plugin_names.contains(name) => {
                        ambiguous_plugin_availability(name)
                    }
                    Some(item) if !item.has_settings => SettingsAvailability::Unavailable {
                        prerequisite: format!("Builtin plugin {name} has no settings section."),
                    },
                    Some(item) if !item.enabled => SettingsAvailability::Unavailable {
                        prerequisite: format!("Enable {name} in Plugin Settings first."),
                    },
                    Some(_) => condition(self.prerequisite),
                };
                (owner, availability)
            }
        };
        (owner, section, availability)
    }
}

fn ambiguous_plugin_availability(name: &str) -> SettingsAvailability {
    SettingsAvailability::Unavailable {
        prerequisite: format!(
            "Multiple registered plugins share the name {name}; Settings resolves plugin controls by name."
        ),
    }
}

fn condition(prerequisite: Option<&str>) -> SettingsAvailability {
    prerequisite.map_or(SettingsAvailability::Available, |prerequisite| {
        SettingsAvailability::Conditional {
            prerequisite: prerequisite.to_owned(),
        }
    })
}

macro_rules! seed {
    ($id:expr, BuiltinPlugin($name:expr), $section:expr, $label:expr, $help:expr, $synonyms:expr) => {
        Seed {
            id: $id,
            owner: SeedOwner::BuiltinPlugin($name),
            section: $section,
            label: $label,
            help: $help,
            synonyms: $synonyms,
            prerequisite: None,
        }
    };
    ($id:expr, BuiltinPlugin($name:expr), $section:expr, $label:expr, $help:expr, $synonyms:expr, $requires:expr) => {
        Seed {
            id: $id,
            owner: SeedOwner::BuiltinPlugin($name),
            section: $section,
            label: $label,
            help: $help,
            synonyms: $synonyms,
            prerequisite: Some($requires),
        }
    };
    ($id:expr, $owner:ident, $section:expr, $label:expr, $help:expr, $synonyms:expr) => {
        Seed {
            id: $id,
            owner: SeedOwner::$owner,
            section: $section,
            label: $label,
            help: $help,
            synonyms: $synonyms,
            prerequisite: None,
        }
    };
    ($id:expr, $owner:ident, $section:expr, $label:expr, $help:expr, $synonyms:expr, $requires:expr) => {
        Seed {
            id: $id,
            owner: SeedOwner::$owner,
            section: $section,
            label: $label,
            help: $help,
            synonyms: $synonyms,
            prerequisite: Some($requires),
        }
    };
}

use MainSettingsSection as M;
use SeedSection as S;

const SEEDS: &[Seed] = &[
    // Main Settings: launcher and optional shortcuts.
    seed!(
        ids::HOTKEY_LAUNCHER,
        Application,
        S::Main(M::Hotkeys),
        "Launcher hotkey",
        "Shortcut that opens the launcher.",
        &["global shortcut", "keyboard chord", "launch key"]
    ),
    seed!(
        ids::HOTKEY_QUIT_ENABLED,
        Application,
        S::Main(M::Hotkeys),
        "Enable quit hotkey",
        "Optional shortcut to quit the launcher.",
        &["exit shortcut", "quit key"]
    ),
    seed!(
        ids::HOTKEY_QUIT_VALUE,
        Application,
        S::Main(M::Hotkeys),
        "Quit hotkey",
        "Keyboard shortcut used to quit the launcher.",
        &["exit shortcut", "quit chord"],
        "Enable quit hotkey first"
    ),
    seed!(
        ids::HOTKEY_HELP_ENABLED,
        Application,
        S::Main(M::Hotkeys),
        "Enable help hotkey",
        "Optional shortcut to show launcher help.",
        &["help shortcut"]
    ),
    seed!(
        ids::HOTKEY_HELP_VALUE,
        Application,
        S::Main(M::Hotkeys),
        "Help hotkey",
        "Keyboard shortcut used to show launcher help.",
        &["help chord"],
        "Enable help hotkey first"
    ),
    seed!(
        ids::PASTE_NEXT_ENABLED,
        Application,
        S::Main(M::PasteSequences),
        "Enable Next shortcut",
        "Enable the Paste Sequences next shortcut.",
        &["paste next", "playback next"]
    ),
    seed!(
        ids::PASTE_NEXT_HOTKEY,
        Application,
        S::Main(M::PasteSequences),
        "Next shortcut",
        "Keyboard shortcut that advances Paste Sequences playback.",
        &["paste next key", "playback next chord"],
        "Enable the Next shortcut first"
    ),
    seed!(
        ids::PASTE_CANCEL_ENABLED,
        Application,
        S::Main(M::PasteSequences),
        "Enable Cancel shortcut",
        "Enable the Paste Sequences cancel shortcut.",
        &["paste cancel"]
    ),
    seed!(
        ids::PASTE_CANCEL_HOTKEY,
        Application,
        S::Main(M::PasteSequences),
        "Cancel shortcut",
        "Keyboard shortcut that cancels Paste Sequences playback.",
        &["paste cancel key"],
        "Enable the Cancel shortcut first"
    ),
    seed!(
        ids::PASTE_ESCAPE_CANCEL,
        Application,
        S::Main(M::PasteSequences),
        "Escape cancels playback",
        "Allow Escape to cancel active Paste Sequences playback.",
        &["escape key", "cancel playback"]
    ),
    seed!(
        ids::PASTE_KEEP_VISIBLE,
        Application,
        S::Main(M::PasteSequences),
        "Keep Paste Sequences dialog visible",
        "Keep the dialog open during playback started from it.",
        &["paste window", "stay open"]
    ),
    // Main Settings: launcher placement and appearance.
    seed!(
        ids::LAUNCHER_ALWAYS_ON_TOP,
        Application,
        S::Main(M::LauncherWindow),
        "Always on top",
        "Keep the launcher above other windows.",
        &["topmost", "z order"]
    ),
    seed!(
        ids::LAUNCHER_SHOW_ICON,
        Application,
        S::Main(M::LauncherWindow),
        "Show launcher icon",
        "Show the launcher icon in its title area.",
        &["branding", "window icon"]
    ),
    seed!(
        ids::LAUNCHER_SHOW_TITLE,
        Application,
        S::Main(M::LauncherWindow),
        "Show launcher title",
        "Show the launcher title in its window.",
        &["branding", "window title"]
    ),
    seed!(
        ids::LAUNCHER_JUMP_MODE,
        Application,
        S::Main(M::LauncherWindow),
        "Experimental Jump Mode",
        "Enable hint-based navigation for supported screens.",
        &["keyboard navigation", "jump hints"]
    ),
    seed!(
        "launcher.offscreen_x",
        Application,
        S::Main(M::LauncherWindow),
        "Off-screen position X",
        "Horizontal position used by launcher positioning.",
        &["window position", "coordinate"]
    ),
    seed!(
        "launcher.offscreen_y",
        Application,
        S::Main(M::LauncherWindow),
        "Off-screen position Y",
        "Vertical position used by launcher positioning.",
        &["window position", "coordinate"]
    ),
    seed!(
        ids::LAUNCHER_FOLLOW_MOUSE,
        Application,
        S::Main(M::LauncherWindow),
        "Follow mouse",
        "Place the launcher relative to the pointer.",
        &["cursor position", "follow cursor"]
    ),
    seed!(
        ids::LAUNCHER_STATIC_POSITION,
        Application,
        S::Main(M::LauncherWindow),
        "Use static position",
        "Use the saved launcher position and size instead of following the pointer.",
        &["fixed location", "window placement"],
        "Turn off Follow mouse first"
    ),
    seed!(
        "launcher.static_x",
        Application,
        S::Main(M::LauncherWindow),
        "Static position X",
        "Horizontal coordinate for the fixed launcher location.",
        &["window coordinate", "left"],
        "Turn off Follow mouse and enable Use static position first"
    ),
    seed!(
        "launcher.static_y",
        Application,
        S::Main(M::LauncherWindow),
        "Static position Y",
        "Vertical coordinate for the fixed launcher location.",
        &["window coordinate", "top"],
        "Turn off Follow mouse and enable Use static position first"
    ),
    seed!(
        "launcher.static_width",
        Application,
        S::Main(M::LauncherWindow),
        "Static window width",
        "Width used for the fixed launcher size.",
        &["window dimensions", "size"],
        "Turn off Follow mouse and enable Use static position first"
    ),
    seed!(
        "launcher.static_height",
        Application,
        S::Main(M::LauncherWindow),
        "Static window height",
        "Height used for the fixed launcher size.",
        &["window dimensions", "size"],
        "Turn off Follow mouse and enable Use static position first"
    ),
    seed!(
        ids::LAUNCHER_QUERY_SCALE,
        Application,
        S::Main(M::LauncherWindow),
        "Query text scale",
        "Scale of text in the launcher query field.",
        &["font size", "text size", "search text"]
    ),
    seed!(
        ids::LAUNCHER_QUERY_SCALE_VALUE,
        Application,
        S::Main(M::LauncherWindow),
        "Query text scale value",
        "Numeric value for query text scale.",
        &["font size", "text size"]
    ),
    seed!(
        ids::LAUNCHER_LIST_SCALE,
        Application,
        S::Main(M::LauncherWindow),
        "Result text scale",
        "Scale of text in the launcher result list.",
        &["font size", "text size", "list font"]
    ),
    seed!(
        ids::LAUNCHER_LIST_SCALE_VALUE,
        Application,
        S::Main(M::LauncherWindow),
        "Result text scale value",
        "Numeric value for result text scale.",
        &["font size", "text size"]
    ),
    seed!(
        ids::LAUNCHER_THEME_SETTINGS,
        Application,
        S::Main(M::LauncherWindow),
        "Open Theme Settings",
        "Open the separate Theme Settings editor.",
        &["appearance", "palette", "colors"]
    ),
    // Main Settings: query and result behavior.
    seed!(
        ids::SEARCH_AUTOCOMPLETE,
        Application,
        S::Main(M::SearchResults),
        "Query autocomplete",
        "Complete the query from matching results.",
        &["suggestions", "tab completion"]
    ),
    seed!(
        ids::SEARCH_FUZZY_WEIGHT,
        Application,
        S::Main(M::SearchResults),
        "Fuzzy match weight",
        "Weight used for fuzzy text matching.",
        &["search ranking", "match weight"]
    ),
    seed!(
        ids::SEARCH_FUZZY_WEIGHT_VALUE,
        Application,
        S::Main(M::SearchResults),
        "Fuzzy match weight value",
        "Numeric fuzzy matching weight.",
        &["search ranking"]
    ),
    seed!(
        ids::SEARCH_USAGE_WEIGHT,
        Application,
        S::Main(M::SearchResults),
        "Usage weight",
        "Weight given to frequently selected results.",
        &["ranking", "result relevance"]
    ),
    seed!(
        ids::SEARCH_USAGE_WEIGHT_VALUE,
        Application,
        S::Main(M::SearchResults),
        "Usage weight value",
        "Numeric weight for result usage history.",
        &["ranking"]
    ),
    seed!(
        ids::SEARCH_MATCH_EXACT,
        Application,
        S::Main(M::SearchResults),
        "Match exact",
        "Prefer exact query matches.",
        &["exact search"]
    ),
    seed!(
        "search.page_jump",
        Application,
        S::Main(M::SearchResults),
        "Page jump",
        "Number of results to move when paging through results.",
        &["page size", "keyboard navigation"]
    ),
    seed!(
        ids::SEARCH_GRID_LAYOUT,
        Application,
        S::Main(M::SearchResults),
        "Grid result layout",
        "Display search results in a grid.",
        &["columns", "rows", "tiles"]
    ),
    seed!(
        "search.grid_rows",
        Application,
        S::Main(M::SearchResults),
        "Grid rows",
        "Number of rows in the result grid.",
        &["layout", "result tiles"],
        "Turn on Grid result layout first"
    ),
    seed!(
        "search.grid_columns",
        Application,
        S::Main(M::SearchResults),
        "Grid columns",
        "Number of columns in the result grid.",
        &["layout", "result tiles"],
        "Turn on Grid result layout first"
    ),
    seed!(
        ids::SEARCH_RESPECT_PLUGIN_CAPABILITY,
        Application,
        S::Main(M::SearchResults),
        "Respect plugin result capability",
        "Use each plugin's declared support for grid results.",
        &["plugin list grid", "layout capability"],
        "Turn on Grid result layout first"
    ),
    seed!(
        ids::SEARCH_PLUGIN_OPT_OUT,
        Application,
        S::Main(M::SearchResults),
        "Force list for plugins",
        "Comma-separated plugin names that should always use list results.",
        &["list layout", "plugin opt out"],
        "Turn on Grid result layout first"
    ),
    // Main Settings: action completion and feedback.
    seed!(
        ids::ACTIONS_HIDE_AFTER_RUN,
        Application,
        S::Main(M::ActionsFeedback),
        "Hide window after running action",
        "Hide the launcher after an action completes.",
        &["close launcher", "completion"]
    ),
    seed!(
        ids::ACTIONS_PRESERVE_COMMAND,
        Application,
        S::Main(M::ActionsFeedback),
        "Preserve command after run",
        "Keep the entered command after an action completes.",
        &["keep query", "completion"]
    ),
    seed!(
        ids::ACTIONS_CLEAR_QUERY,
        Application,
        S::Main(M::ActionsFeedback),
        "Clear query after run",
        "Clear the query after an action completes.",
        &["remove search text", "completion"]
    ),
    seed!(
        ids::ACTIONS_CONFIRM_DESTRUCTIVE,
        Application,
        S::Main(M::ActionsFeedback),
        "Confirm destructive actions",
        "Ask before running destructive actions.",
        &["safety", "confirmation"]
    ),
    seed!(
        ids::FEEDBACK_TOASTS,
        Application,
        S::Main(M::ActionsFeedback),
        "Toast notifications",
        "Show transient notifications.",
        &["notification popups"]
    ),
    seed!(
        ids::FEEDBACK_TOAST_DURATION,
        Application,
        S::Main(M::ActionsFeedback),
        "Toast duration",
        "How long toast notifications stay visible.",
        &["notification timeout"],
        "Enable Toast notifications first"
    ),
    seed!(
        ids::FEEDBACK_TOAST_DURATION_VALUE,
        Application,
        S::Main(M::ActionsFeedback),
        "Toast duration value",
        "Numeric notification display duration.",
        &["notification timeout"],
        "Enable Toast notifications first"
    ),
    seed!(
        ids::FEEDBACK_INLINE_ERRORS,
        Application,
        S::Main(M::ActionsFeedback),
        "Inline errors",
        "Show error details next to the relevant interface.",
        &["error messages"]
    ),
    seed!(
        ids::FEEDBACK_ERROR_TOASTS,
        Application,
        S::Main(M::ActionsFeedback),
        "Error toasts",
        "Show errors as transient notifications.",
        &["error notifications"]
    ),
    seed!(
        ids::FEEDBACK_DEBUG_LOGGING,
        Application,
        S::Main(M::ActionsFeedback),
        "Debug logging",
        "Choose whether diagnostic logging is enabled.",
        &["logs", "diagnostics"]
    ),
    seed!(
        ids::FEEDBACK_DISABLE_TIMER_UPDATES,
        Application,
        S::Main(M::ActionsFeedback),
        "Disable timer refresh",
        "Stop periodic updates for timer results.",
        &["refresh", "timer performance"]
    ),
    seed!(
        "feedback.timer_refresh",
        Application,
        S::Main(M::ActionsFeedback),
        "Timer refresh rate",
        "Interval for refreshing timer results.",
        &["timer interval", "update frequency"],
        "Enable timer updates first"
    ),
    // Main Settings: dashboard, radial, and notes.
    seed!(
        ids::DASHBOARD_ENABLED,
        Application,
        S::Main(M::Dashboard),
        "Enable dashboard",
        "Show the dashboard when the query is empty.",
        &["empty search", "home screen"]
    ),
    seed!(
        ids::DASHBOARD_PATH,
        Application,
        S::Main(M::Dashboard),
        "Dashboard configuration path",
        "Location of the dashboard configuration file.",
        &["dashboard file", "config path"]
    ),
    seed!(
        ids::DASHBOARD_DEFAULT_LOCATION,
        Application,
        S::Main(M::Dashboard),
        "Dashboard default location",
        "Default location used when opening the dashboard.",
        &["startup location"]
    ),
    seed!(
        ids::DASHBOARD_SHOW_WHEN_EMPTY,
        Application,
        S::Main(M::Dashboard),
        "Show dashboard when query is blank",
        "Show dashboard content for an empty search.",
        &["empty query", "home screen"]
    ),
    seed!(
        ids::DASHBOARD_REDUCE_WORK,
        Application,
        S::Main(M::Dashboard),
        "Reduce background dashboard work",
        "Reduce dashboard updates while the launcher is unfocused.",
        &["performance", "background work"]
    ),
    seed!(
        ids::DASHBOARD_DIAGNOSTICS,
        Application,
        S::Main(M::Dashboard),
        "Dashboard diagnostics",
        "Show development diagnostics in the dashboard.",
        &["debug", "developer"],
        "Available in development builds or with debug logging"
    ),
    seed!(
        ids::DASHBOARD_CUSTOMIZE,
        Application,
        S::Main(M::Dashboard),
        "Customize dashboard",
        "Open the dashboard editor.",
        &["dashboard layout", "widgets"]
    ),
    seed!(
        ids::RADIAL_ENABLED,
        Application,
        S::Main(M::Radial),
        "Enable radial menus",
        "Enable radial menu interactions.",
        &["pie menu", "circular menu"]
    ),
    seed!(
        ids::RADIAL_SHARED_TAP_HOLD,
        Application,
        S::Main(M::Radial),
        "Share launcher hotkey between tap and hold",
        "Choose whether the launcher shortcut also opens radial menus on hold.",
        &["hold threshold", "radial shortcut"],
        "Enable radial menus first"
    ),
    seed!(
        "radial.hold_threshold",
        Application,
        S::Main(M::Radial),
        "Radial hold threshold",
        "Time threshold for distinguishing a tap from a hold.",
        &["milliseconds", "timing"],
        "Enable radial menus first"
    ),
    seed!(
        "radial.tooltip_scope",
        Application,
        S::Main(M::Radial),
        "Radial tooltip scope",
        "Choose which radial item labels receive full-label tooltips.",
        &["hover labels", "tooltip"],
        "Enable radial menus first"
    ),
    seed!(
        "radial.tooltip_delay",
        Application,
        S::Main(M::Radial),
        "Radial tooltip delay",
        "Delay before showing a radial item tooltip.",
        &["milliseconds", "hover timing"],
        "Enable radial menus first"
    ),
    seed!(
        ids::RADIAL_EXPECTED_LAYOUT_DIAGNOSTICS,
        Application,
        S::Main(M::Radial),
        "Radial layout diagnostics",
        "Show expected radial label-layout diagnostics.",
        &["developer", "debug"],
        "Enable radial menus first"
    ),
    seed!(
        "radial.default_menu",
        Application,
        S::Main(M::Radial),
        "Default radial menu",
        "Menu selected when a new radial interaction starts.",
        &["menu selection"],
        "Enable radial menus first"
    ),
    seed!(
        "radial.default_interaction",
        Application,
        S::Main(M::Radial),
        "New menu interaction mode",
        "Default click and release behavior for new radial menus.",
        &["sticky click", "release to select"],
        "Enable radial menus first"
    ),
    seed!(
        "radial.default_submenu_presentation",
        Application,
        S::Main(M::Radial),
        "New submenu presentation",
        "Default visual presentation for new radial submenus.",
        &["cascade", "same center"],
        "Enable radial menus first"
    ),
    seed!(
        "radial.safety_policy",
        Application,
        S::Main(M::Radial),
        "Radial destructive action safety",
        "Choose how radial destructive actions use confirmation.",
        &["confirm", "safety"],
        "Enable radial menus first"
    ),
    seed!(
        ids::RADIAL_GLOBAL_ITEM_INPUTS,
        Application,
        S::Main(M::Radial),
        "Process-wide radial item shortcuts",
        "Allow radial item shortcuts and hotstrings to work process-wide.",
        &["global hotkeys", "hotstrings"],
        "Enable radial menus first"
    ),
    seed!(
        "radial.default_item_input_scope",
        Application,
        S::Main(M::Radial),
        "New radial item input scope",
        "Default scope for shortcuts on newly created radial items.",
        &["menu local", "global shortcut"],
        "Allow process-wide radial item shortcuts first"
    ),
    seed!(
        ids::RADIAL_EDIT_MENUS,
        Application,
        S::Main(M::Radial),
        "Edit radial menus",
        "Open the radial menu designer.",
        &["designer", "menu authoring"]
    ),
    seed!(
        ids::RADIAL_EDIT_SKINS,
        Application,
        S::Main(M::Radial),
        "Edit radial skins",
        "Open radial appearance editing.",
        &["designer", "theme"]
    ),
    seed!(
        ids::RADIAL_RESTORE_SUBMENU,
        Application,
        S::Main(M::Radial),
        "Restore submenu presentation",
        "Restore the previous radial submenu presentation after migration.",
        &["recovery", "migration"],
        "Restore is available only after a radial submenu migration was applied"
    ),
    // The Notes section retains its established `plugin_notes` persistent ID.
    seed!(
        "notes.markdown",
        Application,
        S::Notes,
        "Rich Markdown",
        "Enable Markdown formatting in notes.",
        &["formatting", "preview"]
    ),
    seed!(
        "notes.task_lists",
        Application,
        S::Notes,
        "Task lists",
        "Enable task-list syntax in notes.",
        &["checkboxes", "todo"]
    ),
    seed!(
        "notes.interactive_checkboxes",
        Application,
        S::Notes,
        "Interactive note checkboxes",
        "Allow task checkboxes to be toggled in rendered notes.",
        &["task completion"]
    ),
    seed!(
        "notes.collapsible_sections",
        Application,
        S::Notes,
        "Collapsible note sections",
        "Enable collapsible Markdown sections.",
        &["fold headings"]
    ),
    seed!(
        "notes.outline_sidebar",
        Application,
        S::Notes,
        "Note outline sidebar",
        "Show the note outline sidebar.",
        &["table of contents", "headings"]
    ),
    seed!(
        "notes.outline_default_open",
        Application,
        S::Notes,
        "Open note outline by default",
        "Start the outline sidebar open.",
        &["table of contents"],
        "Enable the note outline sidebar first"
    ),
    seed!(
        "notes.split_view",
        Application,
        S::Notes,
        "Note split view",
        "Allow editing and preview panes side by side.",
        &["editor preview"]
    ),
    seed!(
        "notes.default_view_mode",
        Application,
        S::Notes,
        "Default note view",
        "Choose Edit, Preview, or Split as the default note view.",
        &["editor", "preview"]
    ),
    seed!(
        "notes.callouts",
        Application,
        S::Notes,
        "Note callouts",
        "Enable callout blocks in Markdown notes.",
        &["admonitions"]
    ),
    seed!(
        "notes.backlinks",
        Application,
        S::Notes,
        "Note backlinks",
        "Enable backlinks between notes.",
        &["linked notes"]
    ),
    seed!(
        "notes.aliases",
        Application,
        S::Notes,
        "Note aliases",
        "Enable alternate names for notes.",
        &["alternate titles"]
    ),
    seed!(
        "notes.templates",
        Application,
        S::Notes,
        "Note templates",
        "Enable note templates.",
        &["new note defaults"]
    ),
    seed!(
        "notes.collapsed_sections_persist",
        Application,
        S::Notes,
        "Remember collapsed note sections",
        "Persist collapsed Markdown section state.",
        &["fold state"]
    ),
    seed!(
        "notes.max_outline_depth",
        Application,
        S::Notes,
        "Maximum note outline depth",
        "Maximum heading depth shown in the outline.",
        &["table of contents", "heading levels"]
    ),
    seed!(
        "notes.save_on_close",
        Application,
        S::Notes,
        "Save note on close",
        "Save note changes when closing the panel with Escape.",
        &["draft", "close behavior"]
    ),
    seed!(
        "notes.confirm_discard",
        Application,
        S::Notes,
        "Confirm before discarding note changes",
        "Ask before discarding an unsaved note draft.",
        &["unsaved changes", "close confirmation"]
    ),
    seed!(
        "notes.always_overwrite",
        Application,
        S::Notes,
        "Always overwrite existing notes",
        "Choose whether external note changes are overwritten automatically.",
        &["conflicts", "save behavior"]
    ),
    seed!(
        "notes.images_as_links",
        Application,
        S::Notes,
        "Display note images as links",
        "Show image references as links instead of inline images.",
        &["image rendering"]
    ),
    seed!(
        "notes.show_details",
        Application,
        S::Notes,
        "Show note details by default",
        "Show note metadata details when opening a note.",
        &["metadata"]
    ),
    seed!(
        "notes.preview_limit",
        Application,
        S::Notes,
        "Tag and link preview limit",
        "Maximum number of tag or link previews shown in notes.",
        &["preview", "more limit"]
    ),
    seed!(
        "notes.panel_width",
        Application,
        S::Notes,
        "Default note panel width",
        "Default width of the note panel.",
        &["window size"]
    ),
    seed!(
        "notes.panel_height",
        Application,
        S::Notes,
        "Default note panel height",
        "Default height of the note panel.",
        &["window size"]
    ),
    seed!(
        "plugin.notes.external_open",
        Application,
        S::Notes,
        "Open notes externally",
        "Choose the external editor used for note actions.",
        &["Powershell", "Notepad", "WezTerm", "external editor"]
    ),
    // Builtin plugin settings. Values, file paths, and user-authored rules are never indexed.
    seed!(
        "plugin.browser_tabs.recalculate_each_query",
        BuiltinPlugin("browser_tabs"),
        S::Plugin,
        "Recalculate browser tab cache on each query",
        "Refresh the browser tab cache for every search.",
        &["query cache", "browser tabs"]
    ),
    seed!(
        "plugin.clipboard.max_entries",
        BuiltinPlugin("clipboard"),
        S::Plugin,
        "Clipboard history limit",
        "Maximum number of clipboard history entries to keep.",
        &["clipboard limit", "history size"]
    ),
    seed!(
        "plugin.clipboard_modify.hide_after_apply",
        BuiltinPlugin("clipboard_modify"),
        S::Plugin,
        "Hide launcher after applying clipboard changes",
        "Whether Clipboard Modify hides the launcher after applying a template or pipeline.",
        &["clipboard modify", "completion"]
    ),
    seed!(
        "plugin.clipboard_modify.dialog_width",
        BuiltinPlugin("clipboard_modify"),
        S::Plugin,
        "Clipboard Modify dialog width",
        "Width of the Clipboard Modify dialog.",
        &["window dimensions"]
    ),
    seed!(
        "plugin.clipboard_modify.dialog_height",
        BuiltinPlugin("clipboard_modify"),
        S::Plugin,
        "Clipboard Modify dialog height",
        "Height of the Clipboard Modify dialog.",
        &["window dimensions"]
    ),
    seed!(
        "plugin.clipboard_modify.navigation_width",
        BuiltinPlugin("clipboard_modify"),
        S::Plugin,
        "Clipboard Modify navigation width",
        "Width of Clipboard Modify's navigation column.",
        &["dialog layout"]
    ),
    seed!(
        "plugin.clipboard_modify.source_preview_split",
        BuiltinPlugin("clipboard_modify"),
        S::Plugin,
        "Clipboard Modify source and preview split",
        "Relative width of the source and preview panes.",
        &["split ratio", "dialog layout"]
    ),
    seed!(
        "plugin.clipboard_modify.template_filter",
        BuiltinPlugin("clipboard_modify"),
        S::Plugin,
        "Clipboard template filter",
        "Filter text for the template list.",
        &["templates", "search filter"]
    ),
    seed!(
        "plugin.clipboard_modify.pipeline_filter",
        BuiltinPlugin("clipboard_modify"),
        S::Plugin,
        "Clipboard pipeline filter",
        "Filter text for saved pipelines.",
        &["pipelines", "search filter"]
    ),
    seed!(
        "plugin.clipboard_modify.management_sort",
        BuiltinPlugin("clipboard_modify"),
        S::Plugin,
        "Clipboard management sort field",
        "Field used to sort template and pipeline management lists.",
        &["sort order"]
    ),
    seed!(
        "plugin.clipboard_modify.management_sort_ascending",
        BuiltinPlugin("clipboard_modify"),
        S::Plugin,
        "Clipboard management sort direction",
        "Choose ascending or descending management sort order.",
        &["sort ascending descending"]
    ),
    seed!(
        "plugin.color_picker.color",
        BuiltinPlugin("color_picker"),
        S::Plugin,
        "Color picker selected color",
        "Color value used by the color picker plugin.",
        &["picker color", "RGBA"]
    ),
    seed!(
        "plugin.diff.wrap_text",
        BuiltinPlugin("diff"),
        S::Plugin,
        "Wrap diff text",
        "Wrap long lines in the diff viewer.",
        &["line wrapping"]
    ),
    seed!(
        "plugin.diff.syntax_highlighting",
        BuiltinPlugin("diff"),
        S::Plugin,
        "Diff syntax highlighting",
        "Highlight source syntax in diff panes.",
        &["code colors"]
    ),
    seed!(
        "plugin.diff.ignore_whitespace",
        BuiltinPlugin("diff"),
        S::Plugin,
        "Ignore whitespace in diffs",
        "Ignore whitespace changes by default.",
        &["comparison"]
    ),
    seed!(
        "plugin.diff.case_sensitive",
        BuiltinPlugin("diff"),
        S::Plugin,
        "Case-sensitive diff matching",
        "Compare text with case sensitivity enabled.",
        &["comparison"]
    ),
    seed!(
        "plugin.diff.pane_split",
        BuiltinPlugin("diff"),
        S::Plugin,
        "Diff pane split",
        "Relative size of the left and right diff panes.",
        &["layout", "split ratio"]
    ),
    seed!(
        "plugin.file_search.global_roots",
        BuiltinPlugin("file_search"),
        S::Plugin,
        "Global file search roots",
        "Directories used as global filename and content-search roots.",
        &["file search", "folders"]
    ),
    seed!(
        "plugin.file_search.excluded_directory_names",
        BuiltinPlugin("file_search"),
        S::Plugin,
        "Excluded directory names",
        "Directory names omitted from file search.",
        &["exclude folders"]
    ),
    seed!(
        "plugin.file_search.max_results",
        BuiltinPlugin("file_search"),
        S::Plugin,
        "Maximum file search results",
        "Limit for returned file search results.",
        &["search limit"]
    ),
    seed!(
        "plugin.file_search.max_matches",
        BuiltinPlugin("file_search"),
        S::Plugin,
        "Maximum content matches per file",
        "Limit for content matches from one file.",
        &["search limit"]
    ),
    seed!(
        "plugin.file_search.max_content_file_size",
        BuiltinPlugin("file_search"),
        S::Plugin,
        "Maximum content-search file size",
        "Largest file size searched for text content.",
        &["bytes", "search limit"]
    ),
    seed!(
        "plugin.file_search.max_full_preview_size",
        BuiltinPlugin("file_search"),
        S::Plugin,
        "Maximum full-file preview size",
        "Limit for full-file previews.",
        &["MiB", "preview limit"]
    ),
    seed!(
        "plugin.file_search.include_hidden",
        BuiltinPlugin("file_search"),
        S::Plugin,
        "Include hidden files",
        "Include hidden files in search results.",
        &["dotfiles"]
    ),
    seed!(
        "plugin.file_search.case_sensitive",
        BuiltinPlugin("file_search"),
        S::Plugin,
        "Case-sensitive file search",
        "Use case-sensitive matching by default.",
        &["filename matching"]
    ),
    seed!(
        "plugin.file_search.everything_enabled",
        BuiltinPlugin("file_search"),
        S::Plugin,
        "Use Everything for global file search",
        "Use the Everything command-line search provider when available.",
        &["ES CLI", "filename search"]
    ),
    seed!(
        "plugin.file_search.everything_executable",
        BuiltinPlugin("file_search"),
        S::Plugin,
        "Everything ES executable",
        "Configure the Everything command-line executable location. The configured path is not indexed.",
        &["Everything path", "es.exe"]
    ),
    seed!(
        "plugin.file_search.ripgrep",
        BuiltinPlugin("file_search"),
        S::Plugin,
        "Ripgrep search provider",
        "Configure the ripgrep executable used for file content searches.",
        &["rg", "text search"]
    ),
    seed!(
        "plugin.file_search.ripgrep_executable",
        BuiltinPlugin("file_search"),
        S::Plugin,
        "Ripgrep executable",
        "Configure the ripgrep executable location. The configured path is not indexed.",
        &["rg path", "text search"]
    ),
    seed!(
        "plugin.file_search.preferred_editor_command",
        BuiltinPlugin("file_search"),
        S::Plugin,
        "Preferred editor command",
        "Command used to open a selected file in an editor.",
        &["open file", "external editor"]
    ),
    seed!(
        "plugin.file_search.preferred_editor_arguments",
        BuiltinPlugin("file_search"),
        S::Plugin,
        "Preferred editor arguments",
        "Arguments passed to the preferred editor command.",
        &["open file"]
    ),
    seed!(
        "plugin.file_search.preferred_terminal_command",
        BuiltinPlugin("file_search"),
        S::Plugin,
        "Preferred terminal command",
        "Command used to open a selected directory in a terminal.",
        &["open folder", "shell"]
    ),
    seed!(
        "plugin.file_search.preferred_terminal_arguments",
        BuiltinPlugin("file_search"),
        S::Plugin,
        "Preferred terminal arguments",
        "Arguments passed to the preferred terminal command.",
        &["open folder"]
    ),
    seed!(
        "plugin.history.max_entries",
        BuiltinPlugin("history"),
        S::Plugin,
        "Search history limit",
        "Maximum number of launcher history entries.",
        &["history size"]
    ),
    seed!(
        "plugin.mkmacro.enabled",
        BuiltinPlugin("mkmacro"),
        S::Plugin,
        "Enable mouse and keyboard macros",
        "Enable the mouse and keyboard macro plugin.",
        &["macro playback", "recording"]
    ),
    seed!(
        "plugin.network.refresh_rate",
        BuiltinPlugin("network"),
        S::Plugin,
        "Network refresh rate",
        "Interval for refreshing network statistics.",
        &["network monitor", "update frequency"]
    ),
    seed!(
        "plugin.network.units",
        BuiltinPlugin("network"),
        S::Plugin,
        "Network speed units",
        "Choose the units used for network transfer rates.",
        &["B/s", "kB/s", "MB/s"]
    ),
    seed!(
        "plugin.omni_search.included_categories",
        BuiltinPlugin("omni_search"),
        S::Plugin,
        "Omni Search included categories",
        "Choose which app, note, todo, calendar, folder, and bookmark categories Omni Search queries.",
        &["provider categories", "global search"]
    ),
    seed!(
        "plugin.screenshot.directory",
        BuiltinPlugin("screenshot"),
        S::Plugin,
        "Screenshot save directory",
        "Directory used by screenshot save actions. The configured path is not part of search metadata.",
        &["folder", "screenshot location"]
    ),
    seed!(
        "plugin.screenshot.save_file",
        BuiltinPlugin("screenshot"),
        S::Plugin,
        "Save screenshot file when copying",
        "Save a file when a screenshot is copied.",
        &["copy screenshot"]
    ),
    seed!(
        "plugin.screenshot.use_editor",
        BuiltinPlugin("screenshot"),
        S::Plugin,
        "Enable screenshot editor",
        "Open the screenshot editor after capture.",
        &["image editor"]
    ),
    seed!(
        "plugin.screenshot.auto_save",
        BuiltinPlugin("screenshot"),
        S::Plugin,
        "Auto-save edited screenshots",
        "Save automatically after editing a screenshot.",
        &["image editor"]
    ),
    seed!(
        "plugin.shell.open_in_wezterm",
        BuiltinPlugin("shell"),
        S::Plugin,
        "Open shell commands in WezTerm",
        "Use WezTerm for shell commands when enabled.",
        &["terminal"]
    ),
    seed!(
        "plugin.stopwatch.precision",
        BuiltinPlugin("stopwatch"),
        S::Plugin,
        "Stopwatch precision",
        "Number of fractional digits shown by the stopwatch.",
        &["decimal places"]
    ),
    seed!(
        "plugin.stopwatch.refresh_rate",
        BuiltinPlugin("stopwatch"),
        S::Plugin,
        "Stopwatch refresh rate",
        "Interval for stopwatch display refresh.",
        &["update frequency"]
    ),
    seed!(
        "plugin.virtual_desktop.launch_timeout",
        BuiltinPlugin("virtual_desktop"),
        S::Plugin,
        "Virtual desktop launch discovery timeout",
        "Time allowed to discover a newly launched window.",
        &["milliseconds", "launch timeout"]
    ),
    seed!(
        "plugin.virtual_desktop.rule.enabled",
        BuiltinPlugin("virtual_desktop"),
        S::Plugin,
        "Enable application auto-switch rule",
        "Turn an application-to-desktop auto-switch rule on or off. Rule values are not indexed.",
        &["process rule", "window matching"]
    ),
    seed!(
        "plugin.virtual_desktop.rule.executable",
        BuiltinPlugin("virtual_desktop"),
        S::Plugin,
        "Application or process match",
        "Match a process executable name. The configured name is not indexed.",
        &["process name", "application match"]
    ),
    seed!(
        "plugin.virtual_desktop.rule.process_path",
        BuiltinPlugin("virtual_desktop"),
        S::Plugin,
        "Process path match",
        "Optionally match a process path. The configured path is not indexed.",
        &["executable path", "application match"]
    ),
    seed!(
        "plugin.virtual_desktop.rule.window_title",
        BuiltinPlugin("virtual_desktop"),
        S::Plugin,
        "Window title match",
        "Optionally match text in a window title. The configured text is not indexed.",
        &["title filter", "window matching"]
    ),
    seed!(
        "plugin.virtual_desktop.rule.window_class",
        BuiltinPlugin("virtual_desktop"),
        S::Plugin,
        "Window class match",
        "Optionally match a window class. The configured class is not indexed.",
        &["class filter", "window matching"]
    ),
    seed!(
        "plugin.virtual_desktop.rule.target_desktop",
        BuiltinPlugin("virtual_desktop"),
        S::Plugin,
        "Target desktop",
        "Choose the virtual desktop assigned to a matching application. User desktop names are not indexed.",
        &["desktop assignment", "workspace destination"]
    ),
    // Dedicated settings owners. These entries describe controls, never drafts or values.
    seed!(
        "theme.mode",
        Theme,
        S::Theme,
        "Theme mode",
        "Choose system, dark, light, or custom theme mode.",
        &["appearance", "color scheme"]
    ),
    seed!(
        "theme.saved_palette",
        Theme,
        S::Theme,
        "Saved palette",
        "Choose a saved theme palette.",
        &["preset"]
    ),
    seed!(
        "theme.window_background",
        Theme,
        S::Theme,
        "Window background",
        "Edit the window background color.",
        &["surface color", "palette"]
    ),
    seed!(
        "theme.panel_background",
        Theme,
        S::Theme,
        "Panel background",
        "Edit the panel background color.",
        &["surface color", "palette"]
    ),
    seed!(
        "theme.text_color",
        Theme,
        S::Theme,
        "Text",
        "Edit the theme text color.",
        &["font color", "palette"]
    ),
    seed!(
        "theme.hyperlink_color",
        Theme,
        S::Theme,
        "Hyperlink",
        "Edit the hyperlink color.",
        &["link color", "palette"]
    ),
    seed!(
        "theme.widget.inactive_fill",
        Theme,
        S::Theme,
        "Inactive fill",
        "Edit the fill color for inactive widgets.",
        &["button color", "palette"]
    ),
    seed!(
        "theme.widget.inactive_stroke",
        Theme,
        S::Theme,
        "Inactive stroke",
        "Edit the stroke color for inactive widgets.",
        &["button outline", "palette"]
    ),
    seed!(
        "theme.widget.hovered_fill",
        Theme,
        S::Theme,
        "Hovered fill",
        "Edit the fill color for hovered widgets.",
        &["button color", "palette"]
    ),
    seed!(
        "theme.widget.hovered_stroke",
        Theme,
        S::Theme,
        "Hovered stroke",
        "Edit the stroke color for hovered widgets.",
        &["button outline", "palette"]
    ),
    seed!(
        "theme.widget.active_fill",
        Theme,
        S::Theme,
        "Active fill",
        "Edit the fill color for active widgets.",
        &["button color", "palette"]
    ),
    seed!(
        "theme.widget.active_stroke",
        Theme,
        S::Theme,
        "Active stroke",
        "Edit the stroke color for active widgets.",
        &["button outline", "palette"]
    ),
    seed!(
        "theme.selection_background",
        Theme,
        S::Theme,
        "Selection background",
        "Edit the selection highlight background color.",
        &["selected text", "highlight"]
    ),
    seed!(
        "theme.selection_stroke",
        Theme,
        S::Theme,
        "Selection stroke",
        "Edit the outline for selected and highlighted content.",
        &["selected text outline", "highlight"]
    ),
    seed!(
        "theme.status.success",
        Theme,
        S::Theme,
        "Success",
        "Edit the success status color.",
        &["feedback color", "positive status"]
    ),
    seed!(
        "theme.status.warning",
        Theme,
        S::Theme,
        "Warning",
        "Edit the warning status color.",
        &["feedback color", "caution status"]
    ),
    seed!(
        "theme.status.error",
        Theme,
        S::Theme,
        "Error",
        "Edit the error status color.",
        &["feedback color", "failure status"]
    ),
    seed!(
        "theme.accent",
        Theme,
        S::Theme,
        "Override accent",
        "Enable an explicit theme accent color.",
        &["accent setting", "highlight color"]
    ),
    seed!(
        "theme.accent.color",
        Theme,
        S::Theme,
        "Accent color",
        "Choose the explicit theme accent color.",
        &["highlight color"],
        "Enable Override accent first"
    ),
    seed!(
        "theme.typography_scale",
        Theme,
        S::Theme,
        "Theme text size",
        "Scale text and typography in the interface.",
        &[
            "font size",
            "text size",
            "overall text size",
            "accessibility"
        ]
    ),
    seed!(
        "mouse.coordinate_space",
        Mouse,
        S::Mouse,
        "Mouse coordinate space",
        "Choose screen or desktop coordinates for the mouse inspector.",
        &["cursor coordinates"]
    ),
    seed!(
        "mouse.hud_detail",
        Mouse,
        S::Mouse,
        "Mouse HUD detail",
        "Choose the amount of coordinate detail in the HUD.",
        &["inspector"]
    ),
    seed!(
        "mouse.cursor_offset.x",
        Mouse,
        S::Mouse,
        "HUD cursor offset X",
        "Set the horizontal cursor-relative HUD offset in physical pixels.",
        &["mouse position", "inspector offset"]
    ),
    seed!(
        "mouse.cursor_offset.y",
        Mouse,
        S::Mouse,
        "HUD cursor offset Y",
        "Set the vertical cursor-relative HUD offset in physical pixels.",
        &["mouse position", "inspector offset"]
    ),
    seed!(
        "mouse.click.style",
        Mouse,
        S::Mouse,
        "Click effect style",
        "Choose the persisted visual style for click feedback.",
        &["ripple", "circle", "flash"]
    ),
    seed!(
        "mouse.click.color",
        Mouse,
        S::Mouse,
        "Click effect color",
        "Choose the persisted color for click feedback.",
        &["mouse color"]
    ),
    seed!(
        "mouse.click.size",
        Mouse,
        S::Mouse,
        "Click effect size",
        "Set the click feedback size in physical pixels.",
        &["mouse effect"]
    ),
    seed!(
        "mouse.click.opacity",
        Mouse,
        S::Mouse,
        "Click effect opacity",
        "Set click feedback opacity.",
        &["transparency"]
    ),
    seed!(
        "mouse.click.duration",
        Mouse,
        S::Mouse,
        "Click effect duration",
        "Set how long click feedback remains visible.",
        &["milliseconds", "timing"]
    ),
    seed!(
        "mouse.crosshair.color",
        Mouse,
        S::Mouse,
        "Crosshair color",
        "Choose the cursor crosshair color.",
        &["mouse overlay"]
    ),
    seed!(
        "mouse.crosshair.thickness",
        Mouse,
        S::Mouse,
        "Crosshair thickness",
        "Set the crosshair line thickness.",
        &["mouse overlay"]
    ),
    seed!(
        "mouse.crosshair.arm_length",
        Mouse,
        S::Mouse,
        "Crosshair arm length",
        "Set the length of each crosshair arm.",
        &["mouse overlay"]
    ),
    seed!(
        "mouse.crosshair.center_gap",
        Mouse,
        S::Mouse,
        "Crosshair center gap",
        "Set the gap at the center of the crosshair.",
        &["mouse overlay"]
    ),
    seed!(
        "mouse.crosshair.opacity",
        Mouse,
        S::Mouse,
        "Crosshair opacity",
        "Set crosshair transparency.",
        &["mouse overlay"]
    ),
    seed!(
        "mouse.crosshair.virtual_desktop_guides",
        Mouse,
        S::Mouse,
        "Virtual desktop crosshair guides",
        "Show virtual desktop guides on the crosshair.",
        &["overlay"]
    ),
    seed!(
        "mouse.crosshair.high_contrast_outline",
        Mouse,
        S::Mouse,
        "High contrast crosshair outline",
        "Add an outline to improve crosshair contrast.",
        &["overlay"]
    ),
    seed!(
        "mouse.halo.radius",
        Mouse,
        S::Mouse,
        "Cursor halo radius",
        "Set the cursor halo radius in physical pixels.",
        &["inversion effect"]
    ),
    seed!(
        "mouse.halo.inversion_strength",
        Mouse,
        S::Mouse,
        "Cursor halo inversion strength",
        "Set the screen inversion strength inside the cursor halo.",
        &["screen effect"]
    ),
    seed!(
        "mouse.halo.outline_enabled",
        Mouse,
        S::Mouse,
        "Cursor halo outline",
        "Show an outline around the halo.",
        &["screen effect"]
    ),
    seed!(
        "mouse.halo.outline_color",
        Mouse,
        S::Mouse,
        "Cursor halo outline color",
        "Choose the halo outline color.",
        &["screen effect"]
    ),
    seed!(
        "mouse.halo.outline_thickness",
        Mouse,
        S::Mouse,
        "Cursor halo outline thickness",
        "Set the halo outline thickness.",
        &["screen effect"]
    ),
    seed!(
        "mouse.magnifier.mode",
        Mouse,
        S::Mouse,
        "Magnifier destination mode",
        "Choose offset or centered magnifier placement.",
        &["zoom mode"]
    ),
    seed!(
        "mouse.magnifier.zoom_factor",
        Mouse,
        S::Mouse,
        "Magnifier zoom factor",
        "Set the cursor magnification level.",
        &["zoom"]
    ),
    seed!(
        "mouse.magnifier.diameter",
        Mouse,
        S::Mouse,
        "Magnifier lens diameter",
        "Set the magnifier lens diameter in physical pixels.",
        &["zoom size"]
    ),
    seed!(
        "mouse.magnifier.offset_x",
        Mouse,
        S::Mouse,
        "Magnifier destination X offset",
        "Set horizontal displacement of the magnifier lens.",
        &["zoom position"]
    ),
    seed!(
        "mouse.magnifier.offset_y",
        Mouse,
        S::Mouse,
        "Magnifier destination Y offset",
        "Set vertical displacement of the magnifier lens.",
        &["zoom position"]
    ),
    seed!(
        "mouse.magnifier.outline_enabled",
        Mouse,
        S::Mouse,
        "Magnifier outline",
        "Show an outline around the magnifier lens.",
        &["zoom"]
    ),
    seed!(
        "mouse.magnifier.outline_color",
        Mouse,
        S::Mouse,
        "Magnifier outline color",
        "Choose the magnifier outline color.",
        &["zoom"]
    ),
    seed!(
        "mouse.magnifier.outline_thickness",
        Mouse,
        S::Mouse,
        "Magnifier outline thickness",
        "Set the magnifier outline thickness.",
        &["zoom"]
    ),
    seed!(
        "mouse.click.session_enabled",
        Mouse,
        S::Mouse,
        "Click feedback enabled for this session",
        "Start or stop click feedback immediately; this session switch is not saved.",
        &["click effect", "temporary enablement"]
    ),
    seed!(
        "mouse.crosshair.session_enabled",
        Mouse,
        S::Mouse,
        "Crosshair enabled for this session",
        "Start or stop the crosshair immediately; this session switch is not saved.",
        &["cursor crosshair", "temporary enablement"]
    ),
    seed!(
        "mouse.halo.session_enabled",
        Mouse,
        S::Mouse,
        "Halo enabled for this session",
        "Start or stop the cursor halo immediately; this session switch is not saved.",
        &["cursor halo", "temporary enablement"]
    ),
    seed!(
        "mouse.magnifier.session_enabled",
        Mouse,
        S::Mouse,
        "Magnifier enabled for this session",
        "Start or stop the magnifier immediately; this session switch is not saved.",
        &["zoom", "temporary enablement"]
    ),
    seed!(
        "mouse.hud.session_enabled",
        Mouse,
        S::Mouse,
        "HUD enabled for this session",
        "Show or hide the coordinate HUD immediately; this session switch is not saved.",
        &["coordinate inspector", "temporary enablement"]
    ),
    seed!(
        "mouse_gestures.enabled",
        BuiltinPlugin("mouse_gestures"),
        S::MouseGestures,
        "Enable mouse gestures",
        "Enable the mouse gesture recognizer.",
        &["gesture plugin"]
    ),
    seed!(
        "mouse_gestures.debug_logging",
        BuiltinPlugin("mouse_gestures"),
        S::MouseGestures,
        "Mouse gesture debug logging",
        "Enable diagnostic logging for mouse gestures.",
        &["debug"]
    ),
    seed!(
        "mouse_gestures.show_trail",
        BuiltinPlugin("mouse_gestures"),
        S::MouseGestures,
        "Show mouse gesture trail",
        "Show the gesture trail overlay.",
        &["overlay"]
    ),
    seed!(
        "mouse_gestures.trail_color",
        BuiltinPlugin("mouse_gestures"),
        S::MouseGestures,
        "Mouse gesture trail color",
        "Choose the gesture trail color.",
        &["overlay"]
    ),
    seed!(
        "mouse_gestures.trail_width",
        BuiltinPlugin("mouse_gestures"),
        S::MouseGestures,
        "Mouse gesture trail width",
        "Set the gesture trail width.",
        &["overlay"]
    ),
    seed!(
        "mouse_gestures.trail_interval",
        BuiltinPlugin("mouse_gestures"),
        S::MouseGestures,
        "Mouse gesture trail refresh interval",
        "Interval for updating the visible mouse gesture trail.",
        &["refresh", "milliseconds"]
    ),
    seed!(
        "mouse_gestures.recognition_interval",
        BuiltinPlugin("mouse_gestures"),
        S::MouseGestures,
        "Mouse gesture recognition interval",
        "Interval for recognizing a mouse gesture.",
        &["refresh", "milliseconds"]
    ),
    seed!(
        "mouse_gestures.show_hint",
        BuiltinPlugin("mouse_gestures"),
        S::MouseGestures,
        "Show mouse gesture hints",
        "Show the gesture hint overlay.",
        &["overlay"]
    ),
    seed!(
        "mouse_gestures.hint_offset_x",
        BuiltinPlugin("mouse_gestures"),
        S::MouseGestures,
        "Mouse gesture hint horizontal offset",
        "Set the horizontal offset for gesture hints.",
        &["overlay position"]
    ),
    seed!(
        "mouse_gestures.hint_offset_y",
        BuiltinPlugin("mouse_gestures"),
        S::MouseGestures,
        "Mouse gesture hint vertical offset",
        "Set the vertical offset for gesture hints.",
        &["overlay position"]
    ),
    seed!(
        "mouse_gestures.wheel_cycle_gate",
        BuiltinPlugin("mouse_gestures"),
        S::MouseGestures,
        "Mouse gesture wheel cycle gate",
        "Choose how wheel input cycles gestures.",
        &["gesture behavior"]
    ),
    seed!(
        "mouse_gestures.cancel_behavior",
        BuiltinPlugin("mouse_gestures"),
        S::MouseGestures,
        "Mouse gesture cancel behavior",
        "Choose what happens when a gesture is cancelled.",
        &["gesture behavior"]
    ),
    seed!(
        "mouse_gestures.no_match_behavior",
        BuiltinPlugin("mouse_gestures"),
        S::MouseGestures,
        "Mouse gesture no-match behavior",
        "Choose what happens when no gesture matches.",
        &["gesture behavior"]
    ),
    seed!(
        "mouse_gestures.ignore_window_titles",
        BuiltinPlugin("mouse_gestures"),
        S::MouseGestures,
        "Mouse gesture ignored window titles",
        "Manage the ignore-list control. Window title values are not search metadata.",
        &["ignore list", "excluded windows"]
    ),
    seed!(
        "brightness.level",
        Brightness,
        S::Brightness,
        "Brightness level",
        "Current brightness dialog level; it is applied only when Set is pressed and is not a persisted global preference.",
        &["display brightness", "monitor level"],
        "Open the Brightness dialog; an adjustable display must be available"
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin(
        name: &str,
        origin: PluginCatalogOrigin,
        epoch: u64,
        has_settings: bool,
        enabled: bool,
        capabilities: &[&str],
    ) -> crate::plugin::PluginCatalogEntry {
        crate::plugin::PluginCatalogEntry {
            name: name.to_owned(),
            origin,
            epoch,
            has_settings,
            enabled,
            capabilities: capabilities
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
        }
    }

    fn snapshot(
        revision: u64,
        plugins: Vec<crate::plugin::PluginCatalogEntry>,
    ) -> PluginCatalogSnapshot {
        PluginCatalogSnapshot { revision, plugins }
    }

    #[test]
    fn settings_search_catalog_contains_the_rendered_main_settings_field_inventory() {
        let catalog = SettingsCatalog::default();
        let expected_ids = [
            "hotkeys.launcher",
            "hotkeys.quit.enabled",
            "hotkeys.quit.value",
            "hotkeys.help.enabled",
            "hotkeys.help.value",
            "paste.next.enabled",
            "paste.next.hotkey",
            "paste.cancel.enabled",
            "paste.cancel.hotkey",
            "paste.escape_cancel",
            "paste.keep_visible",
            "launcher.always_on_top",
            "launcher.show_icon",
            "launcher.show_title",
            "launcher.experimental_jump_mode",
            "launcher.offscreen_x",
            "launcher.offscreen_y",
            "launcher.follow_mouse",
            "launcher.static_position",
            "launcher.static_x",
            "launcher.static_y",
            "launcher.static_width",
            "launcher.static_height",
            "launcher.query_scale",
            "launcher.query_scale/value",
            "launcher.list_scale",
            "launcher.list_scale/value",
            "launcher.open_theme_settings",
            "search.autocomplete",
            "search.fuzzy_weight",
            "search.fuzzy_weight/value",
            "search.usage_weight",
            "search.usage_weight/value",
            "search.match_exact",
            "search.page_jump",
            "search.grid_layout",
            "search.grid_rows",
            "search.grid_columns",
            "search.respect_plugin_capability",
            "search.plugin_opt_out",
            "actions.hide_after_run",
            "actions.preserve_command",
            "actions.clear_query_after_run",
            "actions.confirm_destructive",
            "feedback.toasts",
            "feedback.toast_duration",
            "feedback.toast_duration/value",
            "feedback.inline_errors",
            "feedback.error_toasts",
            "feedback.debug_logging",
            "feedback.disable_timer_updates",
            "feedback.timer_refresh",
            "dashboard.enabled",
            "dashboard.path",
            "dashboard.default_location",
            "dashboard.show_when_empty",
            "dashboard.reduce_work_unfocused",
            "dashboard.diagnostics",
            "dashboard.customize",
            "radial.enabled",
            "radial.shared_tap_hold",
            "radial.hold_threshold",
            "radial.tooltip_scope",
            "radial.tooltip_delay",
            "radial.expected_layout_diagnostics",
            "radial.default_menu",
            "radial.default_interaction",
            "radial.default_submenu_presentation",
            "radial.safety_policy",
            "radial.global_item_inputs",
            "radial.default_item_input_scope",
            "radial.edit_menus",
            "radial.edit_skins",
            "radial.restore_submenu_presentation",
            "notes.markdown",
            "notes.task_lists",
            "notes.interactive_checkboxes",
            "notes.collapsible_sections",
            "notes.outline_sidebar",
            "notes.outline_default_open",
            "notes.split_view",
            "notes.default_view_mode",
            "notes.callouts",
            "notes.backlinks",
            "notes.aliases",
            "notes.templates",
            "notes.collapsed_sections_persist",
            "notes.max_outline_depth",
            "notes.save_on_close",
            "notes.confirm_discard",
            "notes.always_overwrite",
            "notes.images_as_links",
            "notes.show_details",
            "notes.preview_limit",
            "notes.panel_width",
            "notes.panel_height",
            "plugin.notes.external_open",
        ];
        for id in expected_ids {
            assert!(catalog.entry(id).is_some(), "missing Settings field {id}");
        }

        let builtin_owners = SEEDS
            .iter()
            .filter_map(|seed| match seed.owner {
                SeedOwner::BuiltinPlugin(name) => Some(name),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let expected_builtin_owners = [
            "browser_tabs",
            "clipboard",
            "clipboard_modify",
            "color_picker",
            "diff",
            "file_search",
            "history",
            "mkmacro",
            "mouse_gestures",
            "network",
            "omni_search",
            "screenshot",
            "shell",
            "stopwatch",
            "virtual_desktop",
        ]
        .into_iter()
        .collect::<BTreeSet<_>>();
        assert_eq!(builtin_owners, expected_builtin_owners);
        assert!(catalog.has_unique_control_ids());
    }

    #[test]
    fn settings_search_catalog_covers_builtin_and_dedicated_control_inventory() {
        let catalog = SettingsCatalog::default();
        let expected_ids = [
            "plugin.browser_tabs.recalculate_each_query",
            "plugin.clipboard.max_entries",
            "plugin.clipboard_modify.hide_after_apply",
            "plugin.clipboard_modify.dialog_width",
            "plugin.clipboard_modify.dialog_height",
            "plugin.clipboard_modify.navigation_width",
            "plugin.clipboard_modify.source_preview_split",
            "plugin.clipboard_modify.template_filter",
            "plugin.clipboard_modify.pipeline_filter",
            "plugin.clipboard_modify.management_sort",
            "plugin.clipboard_modify.management_sort_ascending",
            "plugin.color_picker.color",
            "plugin.diff.wrap_text",
            "plugin.diff.syntax_highlighting",
            "plugin.diff.ignore_whitespace",
            "plugin.diff.case_sensitive",
            "plugin.diff.pane_split",
            "plugin.file_search.global_roots",
            "plugin.file_search.excluded_directory_names",
            "plugin.file_search.max_results",
            "plugin.file_search.max_matches",
            "plugin.file_search.max_content_file_size",
            "plugin.file_search.max_full_preview_size",
            "plugin.file_search.include_hidden",
            "plugin.file_search.case_sensitive",
            "plugin.file_search.everything_enabled",
            "plugin.file_search.everything_executable",
            "plugin.file_search.ripgrep",
            "plugin.file_search.ripgrep_executable",
            "plugin.file_search.preferred_editor_command",
            "plugin.file_search.preferred_editor_arguments",
            "plugin.file_search.preferred_terminal_command",
            "plugin.file_search.preferred_terminal_arguments",
            "plugin.history.max_entries",
            "plugin.mkmacro.enabled",
            "plugin.network.refresh_rate",
            "plugin.network.units",
            "plugin.omni_search.included_categories",
            "plugin.screenshot.directory",
            "plugin.screenshot.save_file",
            "plugin.screenshot.use_editor",
            "plugin.screenshot.auto_save",
            "plugin.shell.open_in_wezterm",
            "plugin.stopwatch.precision",
            "plugin.stopwatch.refresh_rate",
            "plugin.virtual_desktop.launch_timeout",
            "plugin.virtual_desktop.rule.enabled",
            "plugin.virtual_desktop.rule.executable",
            "plugin.virtual_desktop.rule.process_path",
            "plugin.virtual_desktop.rule.window_title",
            "plugin.virtual_desktop.rule.window_class",
            "plugin.virtual_desktop.rule.target_desktop",
            "plugin.notes.external_open",
            "mouse_gestures.enabled",
            "mouse_gestures.debug_logging",
            "mouse_gestures.show_trail",
            "mouse_gestures.trail_color",
            "mouse_gestures.trail_width",
            "mouse_gestures.trail_interval",
            "mouse_gestures.recognition_interval",
            "mouse_gestures.show_hint",
            "mouse_gestures.hint_offset_x",
            "mouse_gestures.hint_offset_y",
            "mouse_gestures.wheel_cycle_gate",
            "mouse_gestures.cancel_behavior",
            "mouse_gestures.no_match_behavior",
            "mouse_gestures.ignore_window_titles",
            "theme.mode",
            "theme.saved_palette",
            "theme.window_background",
            "theme.panel_background",
            "theme.text_color",
            "theme.hyperlink_color",
            "theme.widget.inactive_fill",
            "theme.widget.inactive_stroke",
            "theme.widget.hovered_fill",
            "theme.widget.hovered_stroke",
            "theme.widget.active_fill",
            "theme.widget.active_stroke",
            "theme.selection_background",
            "theme.selection_stroke",
            "theme.status.success",
            "theme.status.warning",
            "theme.status.error",
            "theme.accent",
            "theme.accent.color",
            "theme.typography_scale",
            "mouse.coordinate_space",
            "mouse.hud_detail",
            "mouse.cursor_offset.x",
            "mouse.cursor_offset.y",
            "mouse.click.style",
            "mouse.click.color",
            "mouse.click.size",
            "mouse.click.opacity",
            "mouse.click.duration",
            "mouse.crosshair.color",
            "mouse.crosshair.thickness",
            "mouse.crosshair.arm_length",
            "mouse.crosshair.center_gap",
            "mouse.crosshair.opacity",
            "mouse.crosshair.virtual_desktop_guides",
            "mouse.crosshair.high_contrast_outline",
            "mouse.halo.radius",
            "mouse.halo.inversion_strength",
            "mouse.halo.outline_enabled",
            "mouse.halo.outline_color",
            "mouse.halo.outline_thickness",
            "mouse.magnifier.mode",
            "mouse.magnifier.zoom_factor",
            "mouse.magnifier.diameter",
            "mouse.magnifier.offset_x",
            "mouse.magnifier.offset_y",
            "mouse.magnifier.outline_enabled",
            "mouse.magnifier.outline_color",
            "mouse.magnifier.outline_thickness",
            "mouse.click.session_enabled",
            "mouse.crosshair.session_enabled",
            "mouse.halo.session_enabled",
            "mouse.magnifier.session_enabled",
            "mouse.hud.session_enabled",
            "brightness.level",
        ];
        for id in expected_ids {
            assert!(catalog.entry(id).is_some(), "missing catalog field {id}");
        }
        for phantom_id in [
            "mouse_gestures.trail_start_move",
            "mouse_gestures.practice_mode",
            "theme.base_palette_colors",
            "theme.core_surface_colors",
            "theme.text_link_colors",
            "theme.widget_state_colors",
            "theme.selection_colors",
            "theme.status_accent_colors",
            "theme.density",
            "theme.active_preset",
            "mouse.cursor_offset",
            "mouse.session_effects",
        ] {
            assert!(
                catalog.entry(phantom_id).is_none(),
                "phantom control {phantom_id}"
            );
        }
        for (id, expected_label) in [
            ("theme.window_background", "Window background"),
            ("theme.panel_background", "Panel background"),
            ("theme.text_color", "Text"),
            ("theme.hyperlink_color", "Hyperlink"),
            ("theme.widget.inactive_fill", "Inactive fill"),
            ("theme.widget.inactive_stroke", "Inactive stroke"),
            ("theme.widget.hovered_fill", "Hovered fill"),
            ("theme.widget.hovered_stroke", "Hovered stroke"),
            ("theme.widget.active_fill", "Active fill"),
            ("theme.widget.active_stroke", "Active stroke"),
            ("theme.selection_background", "Selection background"),
            ("theme.selection_stroke", "Selection stroke"),
            ("theme.status.success", "Success"),
            ("theme.status.warning", "Warning"),
            ("theme.status.error", "Error"),
            ("theme.accent", "Override accent"),
            ("theme.accent.color", "Accent color"),
            ("mouse.cursor_offset.x", "HUD cursor offset X"),
            ("mouse.cursor_offset.y", "HUD cursor offset Y"),
            (
                "mouse.click.session_enabled",
                "Click feedback enabled for this session",
            ),
            (
                "mouse.crosshair.session_enabled",
                "Crosshair enabled for this session",
            ),
            (
                "mouse.halo.session_enabled",
                "Halo enabled for this session",
            ),
            (
                "mouse.magnifier.session_enabled",
                "Magnifier enabled for this session",
            ),
            ("mouse.hud.session_enabled", "HUD enabled for this session"),
        ] {
            assert_eq!(catalog.entry(id).unwrap().label, expected_label, "{id}");
        }
        let external_notes = catalog.entry("plugin.notes.external_open").unwrap();
        assert_eq!(external_notes.owner, SettingsOwner::Application);
        assert_eq!(external_notes.section, SettingsSection::Notes);
        assert!(external_notes.availability.is_available());
        assert_eq!(
            catalog
                .entry("notes.markdown")
                .unwrap()
                .breadcrumb
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["Plugin Settings", "Notes / Markdown"]
        );
        assert_eq!(
            catalog
                .entry("plugin.file_search.global_roots")
                .unwrap()
                .breadcrumb
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["Plugin Settings", "file_search settings"]
        );
        assert_eq!(
            catalog
                .entry("brightness.level")
                .unwrap()
                .breadcrumb
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["Brightness"]
        );
    }

    #[test]
    fn settings_search_catalog_normalizes_unicode_whitespace_and_matches_synonyms() {
        let catalog = SettingsCatalog::default();
        let results = catalog.search("  QUERY\u{2003} AUTOCOMPLETE  ");
        assert_eq!(
            results
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            ["search.autocomplete"]
        );
        let repeated_results = catalog.search("  QUERY\u{2003} AUTOCOMPLETE  ");
        assert_eq!(
            results
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            repeated_results
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>()
        );

        let weav = catalog.search("  WEZTERM ");
        assert!(
            weav.iter()
                .any(|entry| entry.id.as_str() == "plugin.shell.open_in_wezterm")
        );
        assert!(
            weav.iter()
                .any(|entry| entry.id.as_str() == "plugin.notes.external_open")
        );
        let grid = catalog.search("  result\tLAYOUT  ");
        assert!(
            grid.iter()
                .any(|entry| entry.id.as_str() == "search.grid_layout")
        );
        assert!(
            catalog
                .search("clipboard history limit")
                .iter()
                .any(|entry| entry.id.as_str() == "plugin.clipboard.max_entries")
        );
    }

    #[test]
    fn settings_search_catalog_keeps_owner_identity_and_unavailable_prerequisites_truthful() {
        let current = snapshot(
            5,
            vec![
                plugin("history", PluginCatalogOrigin::Builtin, 10, true, true, &[]),
                plugin(
                    "history",
                    PluginCatalogOrigin::DynamicLibrary,
                    11,
                    true,
                    true,
                    &["grid_results_compatible", "Foo", "foo"],
                ),
                plugin(
                    "external_tools",
                    PluginCatalogOrigin::Registered,
                    12,
                    true,
                    false,
                    &["show_full_path"],
                ),
                plugin(
                    "notes",
                    PluginCatalogOrigin::Builtin,
                    14,
                    true,
                    true,
                    &["search"],
                ),
            ],
        );
        let catalog = SettingsCatalog::from_plugin_snapshot(&current);
        assert!(catalog.has_unique_control_ids());
        assert!(catalog.is_current_for(&current));

        let builtin = catalog.entry("plugin.history.max_entries").unwrap();
        assert_eq!(
            builtin.owner,
            SettingsOwner::Plugin {
                name: "history".into(),
                origin: PluginCatalogOrigin::Builtin,
                epoch: Some(10),
            }
        );
        assert!(matches!(
            catalog.availability_for(builtin, &current),
            SettingsAvailability::Unavailable { prerequisite }
                if prerequisite.contains("share the name history")
        ));
        let notes_plugin = catalog
            .entry("plugin_editor.builtin.notes.14.enabled")
            .expect("the registered Notes provider keeps its actual registered name");
        assert_eq!(
            notes_plugin.owner,
            SettingsOwner::Plugin {
                name: "notes".into(),
                origin: PluginCatalogOrigin::Builtin,
                epoch: Some(14),
            }
        );
        assert!(catalog.entry("plugin.builtin.notes.14.settings").is_none());

        let dynamic = catalog
            .entry("plugin.dynamic.history.11.settings")
            .expect("the colliding DLL keeps a distinct metadata identity");
        assert_eq!(
            dynamic.owner,
            SettingsOwner::Plugin {
                name: "history".into(),
                origin: PluginCatalogOrigin::DynamicLibrary,
                epoch: Some(11),
            }
        );
        assert!(matches!(
            &dynamic.availability,
            SettingsAvailability::Unavailable { prerequisite }
                if prerequisite.contains("share the name history")
        ));
        assert_eq!(
            dynamic.section,
            SettingsSection::Plugin {
                name: "history".into(),
                origin: PluginCatalogOrigin::DynamicLibrary,
                epoch: Some(11),
            }
        );
        let dynamic_capability = catalog
            .entry("plugin_editor.dynamic.history.11.capability.grid_results_compatible")
            .expect("PluginEditor capability controls retain registered identity");
        assert!(matches!(
            &dynamic_capability.availability,
            SettingsAvailability::Unavailable { prerequisite }
                if prerequisite.contains("share the name history")
        ));
        assert!(
            catalog
                .search("grid results capability")
                .iter()
                .any(|entry| entry.id.as_str()
                    == "plugin_editor.dynamic.history.11.capability.grid_results_compatible")
        );
        assert_eq!(dynamic_capability.section, SettingsSection::PluginEditor);
        for id in [
            "plugin_editor.dynamic.history.11.enabled",
            "plugin_editor.builtin.history.10.enabled",
        ] {
            assert!(matches!(
                &catalog.entry(id).unwrap().availability,
                SettingsAvailability::Unavailable { prerequisite }
                    if prerequisite.contains("share the name history")
            ));
        }
        let uppercase_capability = catalog
            .entry("plugin_editor.dynamic.history.11.capability.Foo")
            .unwrap();
        let lowercase_capability = catalog
            .entry("plugin_editor.dynamic.history.11.capability.foo")
            .unwrap();
        assert_ne!(uppercase_capability.id, lowercase_capability.id);
        let case_capability_matches = catalog.search("foo capability");
        assert!(case_capability_matches.iter().any(|entry| {
            entry.id.as_str() == "plugin_editor.dynamic.history.11.capability.Foo"
        }));
        assert!(case_capability_matches.iter().any(|entry| {
            entry.id.as_str() == "plugin_editor.dynamic.history.11.capability.foo"
        }));
        let disabled_plugin_target = catalog
            .entry("plugin.registered.external_tools.12.settings")
            .expect("registered settings destinations are cataloged");
        assert!(matches!(
            &disabled_plugin_target.availability,
            SettingsAvailability::Unavailable { .. }
        ));
        let disabled_capability = catalog
            .entry("plugin_editor.registered.external_tools.12.capability.show_full_path")
            .expect("disabled plugin capabilities retain a target with a prerequisite");
        assert!(matches!(
            &disabled_capability.availability,
            SettingsAvailability::Unavailable { .. }
        ));
        let repeated_labels = catalog
            .entries()
            .iter()
            .filter(|entry| entry.label == "Enable history plugin")
            .collect::<Vec<_>>();
        assert_eq!(repeated_labels.len(), 2);
        assert_ne!(repeated_labels[0].id, repeated_labels[1].id);
        assert!(matches!(
            catalog.availability_for(
                catalog.entry("plugin.file_search.global_roots").unwrap(),
                &current,
            ),
            SettingsAvailability::Unavailable { .. }
        ));

        let stale = snapshot(
            6,
            vec![plugin(
                "history",
                PluginCatalogOrigin::Builtin,
                13,
                true,
                true,
                &[],
            )],
        );
        assert!(!catalog.is_current_for(&stale));
        assert!(matches!(
            catalog.availability_for(builtin, &stale),
            SettingsAvailability::Unavailable { prerequisite } if prerequisite.contains("registration changed")
        ));
        let refreshed = SettingsCatalog::from_plugin_snapshot(&stale);
        let current_builtin = refreshed.entry("plugin.history.max_entries").unwrap();
        assert_eq!(
            current_builtin.owner,
            SettingsOwner::Plugin {
                name: "history".into(),
                origin: PluginCatalogOrigin::Builtin,
                epoch: Some(13),
            }
        );
        assert!(current_builtin.availability.is_available());
    }

    #[test]
    fn settings_search_catalog_reports_conditional_fields_without_setting_values() {
        let catalog = SettingsCatalog::default();
        let rows = catalog.entry("search.grid_rows").unwrap();
        assert!(matches!(
            &rows.availability,
            SettingsAvailability::Conditional { prerequisite }
                if prerequisite.contains("Grid result layout")
        ));
        assert!(!rows.availability.is_available());
        for (id, expected_prerequisite) in [
            ("launcher.static_x", "Follow mouse"),
            ("hotkeys.quit.value", "quit hotkey"),
            ("paste.cancel.hotkey", "Cancel shortcut"),
            ("radial.default_menu", "radial menus"),
            ("radial.restore_submenu_presentation", "migration"),
            ("theme.accent.color", "Override accent"),
            ("brightness.level", "Brightness dialog"),
        ] {
            let entry = catalog.entry(id).unwrap();
            assert!(
                matches!(
                    &entry.availability,
                    SettingsAvailability::Conditional { prerequisite }
                        if prerequisite.to_lowercase().contains(&expected_prerequisite.to_lowercase())
                ),
                "{id} must explain its conditional availability"
            );
            assert!(!entry.availability.is_available());
        }
        assert!(catalog.search("user-config-secret-9876").is_empty());
        assert!(
            catalog
                .entries()
                .iter()
                .all(|entry| !entry.help.contains("user-config-secret-9876"))
        );
    }

    #[test]
    fn settings_search_catalog_refreshes_after_enablement_and_registration_revision_changes() {
        let disabled = snapshot(
            12,
            vec![plugin(
                "history",
                PluginCatalogOrigin::Builtin,
                20,
                true,
                false,
                &[],
            )],
        );
        let old_catalog = SettingsCatalog::from_plugin_snapshot(&disabled);
        let enabled = snapshot(
            13,
            vec![plugin(
                "history",
                PluginCatalogOrigin::Builtin,
                20,
                true,
                true,
                &[],
            )],
        );
        assert!(!old_catalog.is_current_for(&enabled));
        assert!(matches!(
            old_catalog.availability_for(
                old_catalog.entry("plugin.history.max_entries").unwrap(),
                &enabled
            ),
            SettingsAvailability::Unavailable { .. }
        ));
        let new_catalog = SettingsCatalog::from_plugin_snapshot(&enabled);
        assert!(
            new_catalog
                .entry("plugin.history.max_entries")
                .unwrap()
                .availability
                .is_available()
        );
    }
}
