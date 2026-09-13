use std::collections::BTreeMap;

use crate::commands::{
    BrowserTabCommand, ClipboardCommand, Command, MacroCommand, NoteCommand, StorageCommand,
    SystemCommand, TimerCommand, TodoCommand,
};

use super::action_ids;
use super::provider::{ActionResolutionContext, PinCapability, UniversalActionProvider};
use super::{
    ActionAvailability, ActionGroup, ActionIconKey, ActionId, ActionPresentation,
    ActionPresentationOverride, ActionPriority, ActionSafety, ActionSurface, ActionTarget,
    NoteExternalEditor, ResolvedActionTarget, UniversalAction, UniversalActionOperation,
    UniversalUiIntent,
};

fn presentation(
    label: &str,
    short_label: &str,
    icon: ActionIconKey,
    group: ActionGroup,
    priority: ActionPriority,
) -> ActionPresentation {
    ActionPresentation {
        label: label.into(),
        short_label: Some(short_label.into()),
        description: None,
        icon: Some(icon),
        group,
        priority,
        visible: true,
        surface_overrides: BTreeMap::new(),
    }
}

fn action(
    id: ActionId,
    target: &ActionTarget,
    presentation: ActionPresentation,
    operation: UniversalActionOperation,
) -> UniversalAction {
    UniversalAction {
        id,
        target: target.clone(),
        presentation,
        availability: ActionAvailability::Available,
        safety: ActionSafety::Normal,
        operation,
    }
}

fn command_action(
    id: ActionId,
    target: &ActionTarget,
    label: &str,
    short_label: &str,
    icon: ActionIconKey,
    group: ActionGroup,
    priority: ActionPriority,
    command: Command,
    original_action: &crate::actions::Action,
) -> UniversalAction {
    action(
        id,
        target,
        presentation(label, short_label, icon, group, priority),
        UniversalActionOperation::Command {
            command,
            original_action: original_action.clone(),
        },
    )
}

fn ui_action(
    id: ActionId,
    target: &ActionTarget,
    label: &str,
    short_label: &str,
    icon: ActionIconKey,
    group: ActionGroup,
    priority: ActionPriority,
    intent: UniversalUiIntent,
) -> UniversalAction {
    action(
        id,
        target,
        presentation(label, short_label, icon, group, priority),
        UniversalActionOperation::UiIntent(intent),
    )
}

fn destructive(mut action: UniversalAction) -> UniversalAction {
    action.safety = ActionSafety::Destructive;
    action.presentation.group = ActionGroup::Destructive;
    action.presentation.priority = ActionPriority::Low;
    action
}

pub(super) struct GenericProvider;

impl UniversalActionProvider for GenericProvider {
    fn actions(
        &self,
        resolved: &ResolvedActionTarget,
        context: &ActionResolutionContext<'_>,
    ) -> Vec<UniversalAction> {
        let target = &resolved.target;
        let mut primary = action(
            action_ids::RESULT_EXECUTE,
            target,
            presentation(
                "Execute",
                "Open",
                ActionIconKey::Open,
                ActionGroup::Primary,
                ActionPriority::High,
            ),
            UniversalActionOperation::InvokePrimary(resolved.selected_action.clone()),
        );
        primary.presentation.surface_overrides.insert(
            ActionSurface::ContextMenu,
            ActionPresentationOverride {
                visible: Some(false),
                ..Default::default()
            },
        );

        let mut actions = vec![primary];
        if let Some(index) = resolved.custom_action_index {
            actions.push(ui_action(
                action_ids::CUSTOM_ACTION_EDIT,
                target,
                "Edit App",
                "Edit",
                ActionIconKey::Edit,
                ActionGroup::OpenEdit,
                ActionPriority::Normal,
                UniversalUiIntent::EditCustomAction { index },
            ));
        }

        match &context.pin {
            PinCapability::Unsupported => {}
            PinCapability::Writable { is_pinned } | PinCapability::ReadOnly { is_pinned, .. } => {
                let availability = match &context.pin {
                    PinCapability::ReadOnly { reason, .. } => ActionAvailability::Disabled {
                        reason: reason.clone(),
                    },
                    _ => ActionAvailability::Available,
                };
                if *is_pinned {
                    let mut unpin = ui_action(
                        action_ids::RESULT_UNPIN,
                        target,
                        "Unpin Result",
                        "Unpin",
                        ActionIconKey::Unpin,
                        ActionGroup::Organization,
                        ActionPriority::Normal,
                        UniversalUiIntent::UnpinResult {
                            action: resolved.selected_action.clone(),
                        },
                    );
                    unpin.availability = availability.clone();
                    actions.push(unpin);

                    let mut replace = ui_action(
                        action_ids::RESULT_REPLACE_PIN,
                        target,
                        "Replace Pin with Current Result",
                        "Replace Pin",
                        ActionIconKey::Pin,
                        ActionGroup::Organization,
                        ActionPriority::Low,
                        UniversalUiIntent::ReplacePin {
                            action: resolved.selected_action.clone(),
                            query: context.query.to_string(),
                        },
                    );
                    replace.availability = availability.clone();
                    actions.push(replace);
                } else {
                    let mut pin = ui_action(
                        action_ids::RESULT_PIN,
                        target,
                        "Pin Current Query Result",
                        "Pin",
                        ActionIconKey::Pin,
                        ActionGroup::Organization,
                        ActionPriority::Normal,
                        UniversalUiIntent::PinResult {
                            action: resolved.selected_action.clone(),
                            query: context.query.to_string(),
                        },
                    );
                    pin.availability = availability.clone();
                    actions.push(pin);
                }

                let mut recompute = ui_action(
                    action_ids::RESULT_RECOMPUTE_PINS,
                    target,
                    "Recompute Pinned Results",
                    "Recompute",
                    ActionIconKey::Pin,
                    ActionGroup::Organization,
                    ActionPriority::Low,
                    UniversalUiIntent::RecomputePins,
                );
                recompute.availability = availability;
                actions.push(recompute);
            }
        }

        if context.can_add_favorite {
            actions.push(ui_action(
                action_ids::RESULT_FAVORITE,
                target,
                "Add to Favorites",
                "Favorite",
                ActionIconKey::Favorite,
                ActionGroup::Organization,
                ActionPriority::Normal,
                UniversalUiIntent::AddFavorite {
                    action: resolved.selected_action.clone(),
                },
            ));
        }
        actions
    }
}

macro_rules! provider {
    ($name:ident, $resolved:ident, $pattern:pat => $body:expr) => {
        pub(super) struct $name;
        impl UniversalActionProvider for $name {
            fn actions(
                &self,
                $resolved: &ResolvedActionTarget,
                _context: &ActionResolutionContext<'_>,
            ) -> Vec<UniversalAction> {
                match &$resolved.target {
                    $pattern => $body,
                    _ => Vec::new(),
                }
            }
        }
    };
}

provider!(FolderProvider, resolved, ActionTarget::Folder { path } => vec![
    ui_action(action_ids::FOLDER_SET_ALIAS, &resolved.target, "Set Alias", "Alias", ActionIconKey::Edit, ActionGroup::OpenEdit, ActionPriority::High, UniversalUiIntent::OpenFolderAlias { path: path.clone() }),
    destructive(command_action(action_ids::FOLDER_REMOVE, &resolved.target, "Remove Folder", "Remove", ActionIconKey::Delete, ActionGroup::Destructive, ActionPriority::Low, Command::Storage(StorageCommand::FolderRemove(path.clone())), &resolved.selected_action)),
]);

provider!(BookmarkProvider, resolved, ActionTarget::Bookmark { url } => vec![
    ui_action(action_ids::BOOKMARK_SET_ALIAS, &resolved.target, "Set Alias", "Alias", ActionIconKey::Edit, ActionGroup::OpenEdit, ActionPriority::High, UniversalUiIntent::OpenBookmarkAlias { url: url.clone() }),
    destructive(command_action(action_ids::BOOKMARK_REMOVE, &resolved.target, "Remove Bookmark", "Remove", ActionIconKey::Delete, ActionGroup::Destructive, ActionPriority::Low, Command::Storage(StorageCommand::BookmarkRemove(url.clone())), &resolved.selected_action)),
]);

provider!(TimerProvider, resolved, ActionTarget::Timer { id } => vec![
    command_action(action_ids::TIMER_PAUSE, &resolved.target, "Pause Timer", "Pause", ActionIconKey::Timer, ActionGroup::Automation, ActionPriority::High, Command::Timer(TimerCommand::Pause(*id)), &resolved.selected_action),
    destructive(command_action(action_ids::TIMER_CANCEL, &resolved.target, "Remove Timer", "Remove", ActionIconKey::Delete, ActionGroup::Destructive, ActionPriority::Low, Command::Timer(TimerCommand::Cancel(*id)), &resolved.selected_action)),
]);

provider!(StopwatchProvider, resolved, ActionTarget::Stopwatch { id } => vec![
    command_action(action_ids::STOPWATCH_PAUSE, &resolved.target, "Pause Stopwatch", "Pause", ActionIconKey::Stopwatch, ActionGroup::Automation, ActionPriority::High, Command::Timer(TimerCommand::StopwatchPause(*id)), &resolved.selected_action),
    command_action(action_ids::STOPWATCH_RESUME, &resolved.target, "Resume Stopwatch", "Resume", ActionIconKey::Stopwatch, ActionGroup::Automation, ActionPriority::High, Command::Timer(TimerCommand::StopwatchResume(*id)), &resolved.selected_action),
    destructive(command_action(action_ids::STOPWATCH_STOP, &resolved.target, "Stop Stopwatch", "Stop", ActionIconKey::Delete, ActionGroup::Destructive, ActionPriority::Low, Command::Timer(TimerCommand::StopwatchStop(*id)), &resolved.selected_action)),
    ui_action(action_ids::STOPWATCH_COPY_TIME, &resolved.target, "Copy Time", "Copy", ActionIconKey::Copy, ActionGroup::CopyShare, ActionPriority::High, UniversalUiIntent::CopyStopwatchTime { id: *id }),
]);

provider!(SnippetProvider, resolved, ActionTarget::Snippet { alias } => vec![
    ui_action(action_ids::SNIPPET_EDIT, &resolved.target, "Edit Snippet", "Edit", ActionIconKey::Edit, ActionGroup::OpenEdit, ActionPriority::High, UniversalUiIntent::EditSnippet { alias: alias.clone() }),
    destructive(command_action(action_ids::SNIPPET_REMOVE, &resolved.target, "Remove Snippet", "Remove", ActionIconKey::Delete, ActionGroup::Destructive, ActionPriority::Low, Command::Storage(StorageCommand::SnippetRemove(alias.clone())), &resolved.selected_action)),
]);

provider!(TempfileProvider, resolved, ActionTarget::Tempfile { path } => vec![
    ui_action(action_ids::TEMPFILE_SET_ALIAS, &resolved.target, "Set Alias", "Alias", ActionIconKey::Edit, ActionGroup::OpenEdit, ActionPriority::High, UniversalUiIntent::OpenTempfileAlias { path: path.clone() }),
    destructive(command_action(action_ids::TEMPFILE_DELETE, &resolved.target, "Delete File", "Delete", ActionIconKey::Delete, ActionGroup::Destructive, ActionPriority::Low, Command::Storage(StorageCommand::TempfileRemove(path.clone())), &resolved.selected_action)),
]);

provider!(NoteProvider, resolved, ActionTarget::Note { slug } => vec![
    ui_action(action_ids::NOTE_EDIT, &resolved.target, "Edit Note", "Edit", ActionIconKey::Edit, ActionGroup::OpenEdit, ActionPriority::High, UniversalUiIntent::EditNote { slug: slug.clone() }),
    ui_action(action_ids::NOTE_OPEN_NOTEPAD, &resolved.target, "Open in Notepad", "Notepad", ActionIconKey::Note, ActionGroup::OpenEdit, ActionPriority::High, UniversalUiIntent::OpenNoteExternal { slug: slug.clone(), editor: NoteExternalEditor::Notepad }),
    ui_action(action_ids::NOTE_OPEN_NEOVIM, &resolved.target, "Open in Neovim", "Neovim", ActionIconKey::Terminal, ActionGroup::OpenEdit, ActionPriority::High, UniversalUiIntent::OpenNoteExternal { slug: slug.clone(), editor: NoteExternalEditor::Neovim }),
    destructive(command_action(action_ids::NOTE_REMOVE, &resolved.target, "Remove Note", "Remove", ActionIconKey::Delete, ActionGroup::Destructive, ActionPriority::Low, Command::Note(NoteCommand::Remove { slug: slug.clone() }), &resolved.selected_action)),
]);

provider!(ClipboardProvider, resolved, ActionTarget::ClipboardEntry { index } => vec![
    ui_action(action_ids::CLIPBOARD_EDIT, &resolved.target, "Edit Entry", "Edit", ActionIconKey::Edit, ActionGroup::OpenEdit, ActionPriority::High, UniversalUiIntent::EditClipboardEntry { index: *index }),
    destructive(ui_action(action_ids::CLIPBOARD_REMOVE, &resolved.target, "Remove Entry", "Remove", ActionIconKey::Delete, ActionGroup::Destructive, ActionPriority::Low, UniversalUiIntent::RemoveClipboardEntry { index: *index, label: resolved.selected_action.label.clone() })),
]);

provider!(TodoProvider, resolved, ActionTarget::Todo { index } => vec![
    command_action(action_ids::TODO_EDIT, &resolved.target, "Edit Todo", "Edit", ActionIconKey::Edit, ActionGroup::OpenEdit, ActionPriority::High, Command::Todo(TodoCommand::Edit { index: *index }), &resolved.selected_action),
]);

provider!(WindowProvider, resolved, ActionTarget::Window { hwnd } => vec![
    command_action(action_ids::WINDOW_ACTIVATE, &resolved.target, "Activate Window", "Activate", ActionIconKey::Window, ActionGroup::Window, ActionPriority::High, Command::System(SystemCommand::WindowSwitch(*hwnd)), &resolved.selected_action),
    destructive(command_action(action_ids::WINDOW_CLOSE, &resolved.target, "Close Window", "Close", ActionIconKey::Delete, ActionGroup::Destructive, ActionPriority::Low, Command::System(SystemCommand::WindowClose(*hwnd)), &resolved.selected_action)),
]);

provider!(MkMacroProvider, resolved, ActionTarget::MkMacro { id } => vec![
    command_action(action_ids::MKMACRO_RUN, &resolved.target, "Run Macro", "Run", ActionIconKey::Run, ActionGroup::Automation, ActionPriority::High, Command::Macro(MacroCommand::MkRun(*id)), &resolved.selected_action),
    ui_action(action_ids::MKMACRO_EDIT, &resolved.target, "Edit in MkMacro", "Edit", ActionIconKey::Macro, ActionGroup::OpenEdit, ActionPriority::Normal, UniversalUiIntent::OpenMkMacro { id: *id }),
]);

provider!(BrowserTabProvider, resolved, ActionTarget::BrowserTab { runtime_id, url } => {
    let mut copy = command_action(action_ids::BROWSER_TAB_COPY_URL, &resolved.target, "Copy URL", "Copy URL", ActionIconKey::Copy, ActionGroup::CopyShare, ActionPriority::High, Command::Clipboard(ClipboardCommand::SetText { text: url.clone().unwrap_or_default() }), &resolved.selected_action);
    if url.is_none() {
        copy.availability = ActionAvailability::Disabled { reason: "This browser tab did not expose a URL".into() };
    }
    vec![
        command_action(action_ids::BROWSER_TAB_ACTIVATE, &resolved.target, "Activate Tab", "Activate", ActionIconKey::Browser, ActionGroup::Navigation, ActionPriority::High, Command::BrowserTab(BrowserTabCommand::Switch(runtime_id.clone())), &resolved.selected_action),
        copy,
    ]
});

pub(super) static GENERIC: GenericProvider = GenericProvider;
pub(super) static FOLDER: FolderProvider = FolderProvider;
pub(super) static BOOKMARK: BookmarkProvider = BookmarkProvider;
pub(super) static TIMER: TimerProvider = TimerProvider;
pub(super) static STOPWATCH: StopwatchProvider = StopwatchProvider;
pub(super) static SNIPPET: SnippetProvider = SnippetProvider;
pub(super) static TEMPFILE: TempfileProvider = TempfileProvider;
pub(super) static NOTE: NoteProvider = NoteProvider;
pub(super) static CLIPBOARD: ClipboardProvider = ClipboardProvider;
pub(super) static TODO: TodoProvider = TodoProvider;
pub(super) static WINDOW: WindowProvider = WindowProvider;
pub(super) static MKMACRO: MkMacroProvider = MkMacroProvider;
pub(super) static BROWSER_TAB: BrowserTabProvider = BrowserTabProvider;
