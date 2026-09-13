use std::collections::HashSet;

use crate::actions::Action;
use crate::commands::{ClipboardCommand, Command};

use super::*;

fn legacy(command: &str) -> Action {
    Action {
        label: "Selected".into(),
        desc: "Test result".into(),
        action: command.into(),
        args: Some("--kept".into()),
    }
}

fn resolved(target: ActionTarget) -> ResolvedActionTarget {
    ResolvedActionTarget {
        target,
        selected_action: legacy("plugin:execute"),
        custom_action_index: None,
    }
}

fn context(surface: ActionSurface) -> ActionResolutionContext<'static> {
    ActionResolutionContext::new(surface, "current query")
}

fn ids(actions: &[UniversalAction]) -> Vec<&str> {
    actions.iter().map(|action| action.id.as_str()).collect()
}

#[test]
fn each_target_gets_primary_and_current_specialized_capabilities() {
    let cases = [
        (
            ActionTarget::Folder {
                path: "C:/work".into(),
            },
            vec!["result.execute", "folder.set_alias", "folder.remove"],
        ),
        (
            ActionTarget::Bookmark {
                url: "https://example.test".into(),
            },
            vec!["result.execute", "bookmark.set_alias", "bookmark.remove"],
        ),
        (
            ActionTarget::Timer { id: 1 },
            vec![
                "result.execute",
                "timer.pause",
                "timer.resume",
                "timer.cancel",
            ],
        ),
        (
            ActionTarget::Stopwatch { id: 2 },
            vec![
                "result.execute",
                "stopwatch.pause",
                "stopwatch.resume",
                "stopwatch.copy_time",
                "stopwatch.stop",
            ],
        ),
        (
            ActionTarget::Snippet {
                alias: "sig".into(),
            },
            vec!["result.execute", "snippet.edit", "snippet.remove"],
        ),
        (
            ActionTarget::Tempfile {
                path: "C:/tmp/a".into(),
            },
            vec!["result.execute", "tempfile.set_alias", "tempfile.delete"],
        ),
        (
            ActionTarget::Note {
                slug: "daily".into(),
            },
            vec![
                "result.execute",
                "note.edit",
                "note.open_notepad",
                "note.open_neovim",
                "note.remove",
            ],
        ),
        (
            ActionTarget::ClipboardEntry { index: 3 },
            vec!["result.execute", "clipboard.edit", "clipboard.remove"],
        ),
        (
            ActionTarget::Todo { index: 4 },
            vec!["result.execute", "todo.edit"],
        ),
        (
            ActionTarget::Window { hwnd: 5 },
            vec!["result.execute", "window.activate", "window.close"],
        ),
        (
            ActionTarget::MkMacro { id: 6 },
            vec!["result.execute", "mkmacro.run", "mkmacro.edit"],
        ),
        (
            ActionTarget::BrowserTab {
                runtime_id: vec![7, 8],
                url: Some("https://example.test".into()),
            },
            vec![
                "result.execute",
                "browser_tab.activate",
                "browser_tab.copy_url",
            ],
        ),
    ];

    for (target, expected) in cases {
        let actions = UniversalActionRegistry
            .resolve(&resolved(target), &context(ActionSurface::ActionSheet));
        assert_eq!(ids(&actions), expected);
    }
}

#[test]
fn timer_and_stopwatch_pause_resume_availability_uses_open_time_snapshot() {
    let timer = resolved(ActionTarget::Timer { id: 7 });
    let mut timer_context = context(ActionSurface::ActionSheet);
    timer_context.timer_paused = Some(false);
    let actions = UniversalActionRegistry.resolve(&timer, &timer_context);
    assert!(
        actions
            .iter()
            .find(|action| action.id == action_ids::TIMER_PAUSE)
            .unwrap()
            .is_available()
    );
    assert!(
        !actions
            .iter()
            .find(|action| action.id == action_ids::TIMER_RESUME)
            .unwrap()
            .is_available()
    );

    timer_context.timer_paused = Some(true);
    let actions = UniversalActionRegistry.resolve(&timer, &timer_context);
    assert!(
        !actions
            .iter()
            .find(|action| action.id == action_ids::TIMER_PAUSE)
            .unwrap()
            .is_available()
    );
    assert!(
        actions
            .iter()
            .find(|action| action.id == action_ids::TIMER_RESUME)
            .unwrap()
            .is_available()
    );

    let stopwatch = resolved(ActionTarget::Stopwatch { id: 8 });
    let mut stopwatch_context = context(ActionSurface::ContextMenu);
    stopwatch_context.stopwatch_paused = None;
    let actions = UniversalActionRegistry.resolve(&stopwatch, &stopwatch_context);
    for id in [
        action_ids::STOPWATCH_PAUSE,
        action_ids::STOPWATCH_RESUME,
        action_ids::STOPWATCH_COPY_TIME,
        action_ids::STOPWATCH_STOP,
    ] {
        assert_eq!(
            actions
                .iter()
                .find(|action| action.id == id)
                .and_then(|action| action.availability.disabled_reason()),
            Some("Stopwatch is no longer available")
        );
    }
}

#[test]
fn generic_dynamic_fallback_supports_edit_pins_and_favorite_prefill() {
    let selected = legacy("dynamic-plugin:future");
    let resolved = ResolvedActionTarget {
        target: ActionTarget::Generic {
            action: selected.clone(),
        },
        selected_action: selected.clone(),
        custom_action_index: Some(9),
    };
    let mut context = context(ActionSurface::ActionSheet);
    context.pin = PinCapability::Writable { is_pinned: false };
    context.can_add_favorite = true;

    let actions = UniversalActionRegistry.resolve(&resolved, &context);
    assert_eq!(
        ids(&actions),
        [
            "result.execute",
            "custom_action.edit",
            "result.pin",
            "result.favorite",
            "result.recompute_pins",
        ]
    );
    assert!(matches!(
        actions
            .iter()
            .find(|action| action.id == action_ids::RESULT_FAVORITE)
            .map(|action| &action.operation),
        Some(UniversalActionOperation::UiIntent(
            UniversalUiIntent::AddFavorite { action }
        )) if action == &selected
    ));
}

#[test]
fn pinned_and_read_only_pin_states_match_existing_menu_choices() {
    let resolved = resolved(ActionTarget::Generic {
        action: legacy("x"),
    });
    let mut writable = context(ActionSurface::ContextMenu);
    writable.pin = PinCapability::Writable { is_pinned: true };
    let actions = UniversalActionRegistry.resolve(&resolved, &writable);
    assert!(ids(&actions).contains(&"result.unpin"));
    assert!(ids(&actions).contains(&"result.replace_pin"));
    assert!(!ids(&actions).contains(&"result.pin"));
    assert_eq!(
        actions
            .iter()
            .find(|action| action.id == action_ids::RESULT_UNPIN)
            .unwrap()
            .effective_presentation(ActionSurface::ContextMenu)
            .label,
        "Unpin result"
    );

    writable.pin = PinCapability::ReadOnly {
        is_pinned: true,
        reason: "pins unavailable".into(),
    };
    let actions = UniversalActionRegistry.resolve(&resolved, &writable);
    for id in [
        action_ids::RESULT_UNPIN,
        action_ids::RESULT_REPLACE_PIN,
        action_ids::RESULT_RECOMPUTE_PINS,
    ] {
        assert_eq!(
            actions
                .iter()
                .find(|action| action.id == id)
                .and_then(|action| action.availability.disabled_reason()),
            Some("pins unavailable")
        );
    }
}

#[test]
fn primary_visibility_is_surface_specific() {
    let resolved = resolved(ActionTarget::Generic {
        action: legacy("x"),
    });
    for surface in [ActionSurface::ActionSheet, ActionSurface::RadialMenu] {
        let actions = UniversalActionRegistry.resolve(&resolved, &context(surface));
        assert!(actions[0].effective_presentation(surface).visible);
    }
    let actions = UniversalActionRegistry.resolve(&resolved, &context(ActionSurface::ContextMenu));
    assert!(
        !actions[0]
            .effective_presentation(ActionSurface::ContextMenu)
            .visible
    );
}

#[test]
fn radial_actions_have_short_labels_icons_groups_and_priorities() {
    let resolved = resolved(ActionTarget::Note {
        slug: "daily".into(),
    });
    let actions = UniversalActionRegistry.resolve(&resolved, &context(ActionSurface::RadialMenu));
    assert!(actions.iter().all(|action| {
        let presentation = action.effective_presentation(ActionSurface::RadialMenu);
        presentation.short_label.is_some()
            && presentation.icon.is_some()
            && presentation.group != ActionGroup::Other
    }));
}

#[test]
fn browser_copy_url_is_disabled_without_cached_url() {
    let resolved = resolved(ActionTarget::BrowserTab {
        runtime_id: vec![1],
        url: None,
    });
    let actions = UniversalActionRegistry.resolve(&resolved, &context(ActionSurface::ActionSheet));
    let copy = actions
        .iter()
        .find(|action| action.id == action_ids::BROWSER_TAB_COPY_URL)
        .unwrap();
    assert!(!copy.is_available());
    assert!(copy.availability.disabled_reason().unwrap().contains("URL"));
    assert!(matches!(
        copy.operation,
        UniversalActionOperation::Command {
            command: Command::Clipboard(ClipboardCommand::SetText { ref text }),
            ..
        } if text.is_empty()
    ));
}

#[test]
fn removals_deletes_cancel_stop_and_close_are_destructive_and_ordered_last() {
    let targets = [
        ActionTarget::Folder { path: "a".into() },
        ActionTarget::Bookmark { url: "b".into() },
        ActionTarget::Timer { id: 1 },
        ActionTarget::Stopwatch { id: 2 },
        ActionTarget::Snippet { alias: "c".into() },
        ActionTarget::Tempfile { path: "d".into() },
        ActionTarget::Note { slug: "e".into() },
        ActionTarget::ClipboardEntry { index: 3 },
        ActionTarget::Window { hwnd: 4 },
    ];
    let destructive_ids = [
        "folder.remove",
        "bookmark.remove",
        "timer.cancel",
        "stopwatch.stop",
        "snippet.remove",
        "tempfile.delete",
        "note.remove",
        "clipboard.remove",
        "window.close",
    ];
    for target in targets {
        let actions = UniversalActionRegistry
            .resolve(&resolved(target), &context(ActionSurface::ActionSheet));
        let destructive = actions
            .iter()
            .find(|action| destructive_ids.contains(&action.id.as_str()))
            .unwrap();
        assert_eq!(destructive.safety, ActionSafety::Destructive);
        assert_eq!(destructive.presentation.group, ActionGroup::Destructive);
        assert_eq!(actions.last().unwrap().id, destructive.id);
    }
}

#[test]
fn ordering_is_deterministic_deduped_and_list_grid_equivalent() {
    let resolved = resolved(ActionTarget::Window { hwnd: 10 });
    let list = UniversalActionRegistry.resolve(&resolved, &context(ActionSurface::LauncherList));
    let list_again =
        UniversalActionRegistry.resolve(&resolved, &context(ActionSurface::LauncherList));
    let grid = UniversalActionRegistry.resolve(&resolved, &context(ActionSurface::LauncherGrid));
    assert_eq!(ids(&list), ids(&list_again));
    assert_eq!(ids(&list), ids(&grid));
    assert_eq!(
        list.iter()
            .map(|action| &action.id)
            .collect::<HashSet<_>>()
            .len(),
        list.len()
    );
    assert_eq!(list[0].id, action_ids::RESULT_EXECUTE);
    assert_eq!(list.last().unwrap().id, action_ids::WINDOW_CLOSE);
}
