use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::actions::Action;
use crate::commands::Command;

use super::ActionTarget;

/// Stable, non-display identity of a semantic action.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActionId(Cow<'static, str>);

impl ActionId {
    pub const fn from_static(value: &'static str) -> Self {
        Self(Cow::Borrowed(value))
    }

    pub fn new(value: impl Into<String>) -> Self {
        Self(Cow::Owned(value.into()))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl fmt::Display for ActionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl From<&'static str> for ActionId {
    fn from(value: &'static str) -> Self {
        Self::from_static(value)
    }
}

impl From<String> for ActionId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// Canonical action IDs shared by providers and consumers.
pub mod action_ids {
    use super::ActionId;

    macro_rules! action_ids {
        ($($name:ident => $value:literal),+ $(,)?) => {
            $(pub const $name: ActionId = ActionId::from_static($value);)+
        };
    }

    action_ids!(
        RESULT_EXECUTE => "result.execute",
        RESULT_PIN => "result.pin",
        RESULT_UNPIN => "result.unpin",
        RESULT_REPLACE_PIN => "result.replace_pin",
        RESULT_RECOMPUTE_PINS => "result.recompute_pins",
        RESULT_FAVORITE => "result.favorite",
        CUSTOM_ACTION_EDIT => "custom_action.edit",
        FOLDER_SET_ALIAS => "folder.set_alias",
        FOLDER_REMOVE => "folder.remove",
        BOOKMARK_SET_ALIAS => "bookmark.set_alias",
        BOOKMARK_REMOVE => "bookmark.remove",
        TIMER_PAUSE => "timer.pause",
        TIMER_RESUME => "timer.resume",
        TIMER_CANCEL => "timer.cancel",
        STOPWATCH_PAUSE => "stopwatch.pause",
        STOPWATCH_RESUME => "stopwatch.resume",
        STOPWATCH_STOP => "stopwatch.stop",
        STOPWATCH_COPY_TIME => "stopwatch.copy_time",
        SNIPPET_EDIT => "snippet.edit",
        SNIPPET_REMOVE => "snippet.remove",
        TEMPFILE_SET_ALIAS => "tempfile.set_alias",
        TEMPFILE_DELETE => "tempfile.delete",
        NOTE_OPEN => "note.open",
        NOTE_EDIT => "note.edit",
        NOTE_OPEN_NOTEPAD => "note.open_notepad",
        NOTE_OPEN_NEOVIM => "note.open_neovim",
        NOTE_COPY_LINK => "note.copy_link",
        NOTE_REMOVE => "note.remove",
        CLIPBOARD_COPY => "clipboard.copy",
        CLIPBOARD_EDIT => "clipboard.edit",
        CLIPBOARD_REMOVE => "clipboard.remove",
        TODO_EDIT => "todo.edit",
        WINDOW_ACTIVATE => "window.activate",
        WINDOW_CLOSE => "window.close",
        WINDOW_MOVE_DESKTOP => "window.move_desktop",
        MKMACRO_RUN => "mkmacro.run",
        MKMACRO_EDIT => "mkmacro.edit",
        BROWSER_TAB_ACTIVATE => "browser_tab.activate",
        BROWSER_TAB_COPY_URL => "browser_tab.copy_url",
    );
}

/// The UI surface presenting actions. This is deliberately distinct from
/// `commands::ActivationSource`, which describes the triggering input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ActionSurface {
    LauncherList,
    LauncherGrid,
    ActionSheet,
    ContextMenu,
    Dashboard,
    RadialMenu,
    Gesture,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootLauncherPolicy {
    Legacy,
    PreserveOrdinaryState,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UniversalActionInvocationContext {
    pub surface: ActionSurface,
    pub source: crate::commands::ActivationSource,
    pub stable_request: Option<super::PersistedUniversalActionRef>,
    pub history_query: String,
    pub root_policy: RootLauncherPolicy,
}

impl UniversalActionInvocationContext {
    pub fn legacy(surface: ActionSurface, source: crate::commands::ActivationSource) -> Self {
        Self {
            surface,
            source,
            stable_request: None,
            history_query: String::new(),
            root_policy: RootLauncherPolicy::Legacy,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ActionGroup {
    Primary,
    OpenEdit,
    Navigation,
    Window,
    CopyShare,
    Organization,
    Automation,
    Destructive,
    Other,
}

impl Default for ActionGroup {
    fn default() -> Self {
        Self::Other
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ActionIconKey {
    Open,
    Edit,
    Run,
    Copy,
    Delete,
    Pin,
    Unpin,
    Favorite,
    Window,
    Monitor,
    Desktop,
    Macro,
    Terminal,
    Folder,
    File,
    Note,
    Clipboard,
    Timer,
    Stopwatch,
    Browser,
    Search,
    Settings,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ActionPriority {
    High,
    #[default]
    Normal,
    Low,
}

impl ActionPriority {
    /// Ascending sort rank, with the most prominent priority first.
    pub const fn sort_rank(self) -> u8 {
        match self {
            Self::High => 0,
            Self::Normal => 1,
            Self::Low => 2,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionAvailability {
    Available,
    Disabled { reason: String },
}

impl ActionAvailability {
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available)
    }

    pub fn disabled_reason(&self) -> Option<&str> {
        match self {
            Self::Available => None,
            Self::Disabled { reason } => Some(reason),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ActionSafety {
    #[default]
    Normal,
    Destructive,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActionPresentationOverride {
    pub label: Option<String>,
    pub short_label: Option<String>,
    pub description: Option<String>,
    pub icon: Option<ActionIconKey>,
    pub group: Option<ActionGroup>,
    pub priority: Option<ActionPriority>,
    pub visible: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionPresentation {
    pub label: String,
    pub short_label: Option<String>,
    pub description: Option<String>,
    pub icon: Option<ActionIconKey>,
    pub group: ActionGroup,
    pub priority: ActionPriority,
    pub visible: bool,
    pub surface_overrides: BTreeMap<ActionSurface, ActionPresentationOverride>,
}

impl ActionPresentation {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            short_label: None,
            description: None,
            icon: None,
            group: ActionGroup::Other,
            priority: ActionPriority::Normal,
            visible: true,
            surface_overrides: BTreeMap::new(),
        }
    }

    pub fn effective(&self, surface: ActionSurface) -> EffectiveActionPresentation {
        let mut effective = EffectiveActionPresentation {
            label: self.label.clone(),
            short_label: self.short_label.clone(),
            description: self.description.clone(),
            icon: self.icon,
            group: self.group,
            priority: self.priority,
            visible: self.visible,
        };

        if let Some(override_) = self.surface_overrides.get(&surface) {
            if let Some(value) = &override_.label {
                effective.label.clone_from(value);
            }
            if let Some(value) = &override_.short_label {
                effective.short_label = Some(value.clone());
            }
            if let Some(value) = &override_.description {
                effective.description = Some(value.clone());
            }
            if let Some(value) = override_.icon {
                effective.icon = Some(value);
            }
            if let Some(value) = override_.group {
                effective.group = value;
            }
            if let Some(value) = override_.priority {
                effective.priority = value;
            }
            if let Some(value) = override_.visible {
                effective.visible = value;
            }
        }

        effective
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectiveActionPresentation {
    pub label: String,
    pub short_label: Option<String>,
    pub description: Option<String>,
    pub icon: Option<ActionIconKey>,
    pub group: ActionGroup,
    pub priority: ActionPriority,
    pub visible: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteExternalEditor {
    Notepad,
    Neovim,
}

/// GUI-owned operations represented as data so action descriptors remain free
/// of egui objects and callbacks.
#[derive(Clone, Debug, PartialEq)]
pub enum UniversalUiIntent {
    EditCustomAction {
        index: usize,
    },
    OpenFolderAlias {
        path: String,
    },
    OpenBookmarkAlias {
        url: String,
    },
    EditSnippet {
        alias: String,
    },
    OpenTempfileAlias {
        path: String,
    },
    EditNote {
        slug: String,
    },
    OpenNoteExternal {
        slug: String,
        editor: NoteExternalEditor,
    },
    EditClipboardEntry {
        index: usize,
    },
    RemoveClipboardEntry {
        index: usize,
        label: String,
    },
    EditTodo {
        index: usize,
    },
    AddFavorite {
        action: Action,
    },
    PinResult {
        action: Action,
        query: String,
    },
    UnpinResult {
        action: Action,
    },
    ReplacePin {
        action: Action,
        query: String,
    },
    RecomputePins,
    CopyStopwatchTime {
        id: u64,
    },
    OpenMkMacro {
        id: u64,
    },
}

/// Typed operation executed after a surface selects a Universal Action.
#[derive(Clone, Debug, PartialEq)]
pub enum UniversalActionOperation {
    InvokePrimary(Action),
    Command {
        command: Command,
        original_action: Action,
    },
    UiIntent(UniversalUiIntent),
}

#[derive(Clone, Debug, PartialEq)]
pub struct UniversalAction {
    pub id: ActionId,
    pub target: ActionTarget,
    pub presentation: ActionPresentation,
    pub availability: ActionAvailability,
    pub safety: ActionSafety,
    pub operation: UniversalActionOperation,
}

impl UniversalAction {
    pub fn effective_presentation(&self, surface: ActionSurface) -> EffectiveActionPresentation {
        self.presentation.effective(surface)
    }

    pub fn is_available(&self) -> bool {
        self.availability.is_available()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy_action(label: &str) -> Action {
        Action {
            label: label.into(),
            desc: "Example".into(),
            action: "example:run".into(),
            args: None,
        }
    }

    fn universal_action(label: &str) -> UniversalAction {
        let action = legacy_action(label);
        UniversalAction {
            id: action_ids::RESULT_EXECUTE,
            target: ActionTarget::Generic {
                action: action.clone(),
            },
            presentation: ActionPresentation::new(label),
            availability: ActionAvailability::Available,
            safety: ActionSafety::Normal,
            operation: UniversalActionOperation::InvokePrimary(action),
        }
    }

    #[test]
    fn semantic_identity_does_not_depend_on_display_label() {
        let default = universal_action("Open Example");
        let translated = universal_action("Ouvrir l’exemple");

        assert_eq!(default.id, translated.id);
        assert_ne!(default.presentation.label, translated.presentation.label);
    }

    #[test]
    fn effective_presentation_applies_only_requested_surface_override() {
        let mut presentation = ActionPresentation::new("Move to Virtual Desktop");
        presentation.short_label = Some("Move Desktop".into());
        presentation.icon = Some(ActionIconKey::Window);
        presentation.group = ActionGroup::Window;
        presentation.priority = ActionPriority::High;
        presentation.surface_overrides.insert(
            ActionSurface::ContextMenu,
            ActionPresentationOverride {
                label: Some("Move to Desktop".into()),
                ..Default::default()
            },
        );
        presentation.surface_overrides.insert(
            ActionSurface::RadialMenu,
            ActionPresentationOverride {
                short_label: Some("Desktop".into()),
                icon: Some(ActionIconKey::Desktop),
                ..Default::default()
            },
        );

        let default = presentation.effective(ActionSurface::ActionSheet);
        assert_eq!(default.label, "Move to Virtual Desktop");
        assert_eq!(default.short_label.as_deref(), Some("Move Desktop"));
        assert_eq!(default.icon, Some(ActionIconKey::Window));

        let context = presentation.effective(ActionSurface::ContextMenu);
        assert_eq!(context.label, "Move to Desktop");
        assert_eq!(context.icon, Some(ActionIconKey::Window));

        let radial = presentation.effective(ActionSurface::RadialMenu);
        assert_eq!(radial.label, "Move to Virtual Desktop");
        assert_eq!(radial.short_label.as_deref(), Some("Desktop"));
        assert_eq!(radial.icon, Some(ActionIconKey::Desktop));
        assert_eq!(radial.group, ActionGroup::Window);
        assert_eq!(radial.priority, ActionPriority::High);
    }

    #[test]
    fn availability_exposes_disabled_reason() {
        let available = ActionAvailability::Available;
        let disabled = ActionAvailability::Disabled {
            reason: "Virtual desktop service is unavailable".into(),
        };

        assert!(available.is_available());
        assert_eq!(available.disabled_reason(), None);
        assert!(!disabled.is_available());
        assert_eq!(
            disabled.disabled_reason(),
            Some("Virtual desktop service is unavailable")
        );
    }

    #[test]
    fn priority_has_deterministic_prominence_order() {
        assert!(ActionPriority::High.sort_rank() < ActionPriority::Normal.sort_rank());
        assert!(ActionPriority::Normal.sort_rank() < ActionPriority::Low.sort_rank());
    }
}
