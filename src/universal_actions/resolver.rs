use std::collections::HashMap;

use crate::actions::Action;
use crate::commands::{
    BrowserTabCommand, ClipboardCommand, Command, MacroCommand, NoteCommand, SystemCommand,
    TimerCommand, TodoCommand, parse_action,
};

use super::ActionTarget;

/// Borrowed launcher catalogs used only to identify a selected legacy result.
///
/// Resolution is deliberately limited to already-loaded memory. Constructing
/// this context performs no cloning, filesystem access, or platform discovery.
pub struct ActionTargetResolverContext<'a> {
    folder_aliases: &'a HashMap<String, Option<String>>,
    bookmark_aliases: &'a HashMap<String, Option<String>>,
    custom_actions: &'a [Action],
}

impl<'a> ActionTargetResolverContext<'a> {
    pub fn new(
        folder_aliases: &'a HashMap<String, Option<String>>,
        bookmark_aliases: &'a HashMap<String, Option<String>>,
        custom_actions: &'a [Action],
    ) -> Self {
        Self {
            folder_aliases,
            bookmark_aliases,
            custom_actions,
        }
    }

    fn custom_action_index(&self, selected: &Action) -> Option<usize> {
        // Preserve the launcher's established identity rule. Description and
        // args are not part of custom-action matching today.
        self.custom_actions.iter().position(|candidate| {
            candidate.action == selected.action && candidate.label == selected.label
        })
    }
}

/// A typed target together with the exact legacy result selected by the user.
///
/// The selected action remains the source of truth for future primary-action
/// execution. In particular, two results that identify the same target (such
/// as window activate and window close) retain distinct primary semantics.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedActionTarget {
    pub target: ActionTarget,
    pub selected_action: Action,
    pub custom_action_index: Option<usize>,
}

/// Side-effect-free mapping from legacy search results to Universal targets.
#[derive(Clone, Copy, Debug, Default)]
pub struct ActionTargetResolver;

impl ActionTargetResolver {
    pub fn resolve(
        &self,
        selected: &Action,
        context: &ActionTargetResolverContext<'_>,
    ) -> ResolvedActionTarget {
        let custom_action_index = context.custom_action_index(selected);
        let target = self.resolve_target(selected, context, custom_action_index);

        ResolvedActionTarget {
            target,
            selected_action: selected.clone(),
            custom_action_index,
        }
    }

    fn resolve_target(
        &self,
        selected: &Action,
        context: &ActionTargetResolverContext<'_>,
        custom_action_index: Option<usize>,
    ) -> ActionTarget {
        // These aliases are legacy object identities rather than command
        // protocols, so their cached membership must take precedence.
        if context.folder_aliases.contains_key(&selected.action)
            && !selected.action.starts_with("folder:")
        {
            return ActionTarget::Folder {
                path: selected.action.clone(),
            };
        }
        if context.bookmark_aliases.contains_key(&selected.action) {
            return ActionTarget::Bookmark {
                url: selected.action.clone(),
            };
        }

        // `timer:show` intentionally remains an external command in the
        // existing parser, but is the stable identity of active timer rows.
        if selected.desc == "Timer"
            && let Some(id) = exact_unsigned_suffix(&selected.action, "timer:show:")
        {
            return ActionTarget::Timer { id };
        }

        if let Ok(command) = parse_action(selected) {
            match command {
                Command::Timer(TimerCommand::StopwatchShow(id)) if selected.desc == "Stopwatch" => {
                    return ActionTarget::Stopwatch { id };
                }
                Command::Note(NoteCommand::Open { slug })
                    if selected.desc == "Note" && !slug.is_empty() =>
                {
                    return ActionTarget::Note { slug };
                }
                Command::Clipboard(ClipboardCommand::Copy { index })
                    if selected.desc == "Clipboard" =>
                {
                    return ActionTarget::ClipboardEntry { index };
                }
                Command::Todo(TodoCommand::Done { index }) if selected.desc == "Todo" => {
                    return ActionTarget::Todo { index };
                }
                Command::System(SystemCommand::WindowSwitch(hwnd))
                | Command::System(SystemCommand::WindowClose(hwnd)) => {
                    return ActionTarget::Window { hwnd };
                }
                Command::Macro(MacroCommand::MkRun(id)) => {
                    return ActionTarget::MkMacro { id };
                }
                Command::BrowserTab(BrowserTabCommand::Switch(runtime_id))
                    if browser_runtime_id_is_exact(&selected.action, &runtime_id) =>
                {
                    let description = selected.desc.trim();
                    let url = (!description.is_empty()
                        && !description.eq_ignore_ascii_case("Browser Tab")
                        && !description.eq_ignore_ascii_case("Browser tabs"))
                    .then(|| selected.desc.clone());
                    return ActionTarget::BrowserTab { runtime_id, url };
                }
                _ => {}
            }
        }

        // These current result rows carry identity in presentation/legacy
        // fields rather than in a dedicated typed command.
        if selected.desc == "Snippet" {
            return ActionTarget::Snippet {
                alias: selected.label.clone(),
            };
        }
        if selected.desc == "Tempfile" && !selected.action.starts_with("tempfile:") {
            return ActionTarget::Tempfile {
                path: selected.action.clone(),
            };
        }

        match custom_action_index {
            Some(index) => ActionTarget::CustomAction {
                index,
                action: selected.clone(),
            },
            None => ActionTarget::Generic {
                action: selected.clone(),
            },
        }
    }
}

fn exact_unsigned_suffix(value: &str, prefix: &str) -> Option<u64> {
    let suffix = value.strip_prefix(prefix)?;
    (!suffix.is_empty()).then(|| suffix.parse().ok()).flatten()
}

fn browser_runtime_id_is_exact(raw: &str, parsed: &[i32]) -> bool {
    let Some(suffix) = raw.strip_prefix("tab:switch:") else {
        return false;
    };
    let components = suffix.split('_').collect::<Vec<_>>();
    components.len() == parsed.len()
        && components
            .iter()
            .zip(parsed)
            .all(|(raw, parsed)| raw.parse::<i32>().ok() == Some(*parsed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::universal_actions::PersistableActionTargetRef;

    fn action(label: &str, desc: &str, command: &str) -> Action {
        Action {
            label: label.into(),
            desc: desc.into(),
            action: command.into(),
            args: None,
        }
    }

    fn resolve(
        selected: &Action,
        folder_aliases: &HashMap<String, Option<String>>,
        bookmark_aliases: &HashMap<String, Option<String>>,
        custom_actions: &[Action],
    ) -> ResolvedActionTarget {
        ActionTargetResolver.resolve(
            selected,
            &ActionTargetResolverContext::new(folder_aliases, bookmark_aliases, custom_actions),
        )
    }

    #[test]
    fn resolves_every_legacy_context_menu_target_class() {
        let folder = action("Projects", "C:/work", "C:/work");
        let bookmark = action("Rust", "Bookmark", "https://www.rust-lang.org");
        let mut folders = HashMap::new();
        folders.insert(folder.action.clone(), Some("work".into()));
        let mut bookmarks = HashMap::new();
        bookmarks.insert(bookmark.action.clone(), Some("rust".into()));
        let empty = HashMap::new();

        let cases = [
            (
                folder.clone(),
                ActionTarget::Folder {
                    path: folder.action.clone(),
                },
            ),
            (
                bookmark.clone(),
                ActionTarget::Bookmark {
                    url: bookmark.action.clone(),
                },
            ),
            (
                action("Tea", "Timer", "timer:show:41"),
                ActionTarget::Timer { id: 41 },
            ),
            (
                action("Lap", "Stopwatch", "stopwatch:show:42"),
                ActionTarget::Stopwatch { id: 42 },
            ),
            (
                action("sig", "Snippet", "clipboard:Regards"),
                ActionTarget::Snippet {
                    alias: "sig".into(),
                },
            ),
            (
                action("scratch.txt", "Tempfile", "C:/tmp/scratch.txt"),
                ActionTarget::Tempfile {
                    path: "C:/tmp/scratch.txt".into(),
                },
            ),
            (
                action("Daily", "Note", "note:open:daily"),
                ActionTarget::Note {
                    slug: "daily".into(),
                },
            ),
            (
                action("Copied", "Clipboard", "clipboard:copy:43"),
                ActionTarget::ClipboardEntry { index: 43 },
            ),
            (
                action("Task", "Todo", "todo:done:44"),
                ActionTarget::Todo { index: 44 },
            ),
        ];

        for (selected, expected) in cases {
            let (folder_catalog, bookmark_catalog) = if selected == folder {
                (&folders, &empty)
            } else if selected == bookmark {
                (&empty, &bookmarks)
            } else {
                (&empty, &empty)
            };
            assert_eq!(
                resolve(&selected, folder_catalog, bookmark_catalog, &[]).target,
                expected
            );
        }

        let generic = action("Unknown", "Plugin value", "plugin:future");
        assert_eq!(
            resolve(&generic, &empty, &empty, &[]).target,
            ActionTarget::Generic {
                action: generic.clone()
            }
        );
    }

    #[test]
    fn resolves_typed_window_macro_and_browser_tab_targets() {
        let empty = HashMap::new();
        let cases = [
            (
                action("Activate", "Windows", "window:switch:-27"),
                ActionTarget::Window { hwnd: -27 },
            ),
            (
                action("Close", "Windows", "window:close:28"),
                ActionTarget::Window { hwnd: 28 },
            ),
            (
                action("Build", "Macro", "mkmacro:run:29"),
                ActionTarget::MkMacro { id: 29 },
            ),
            (
                action("Docs", "https://example.test/docs", "tab:switch:30_-31"),
                ActionTarget::BrowserTab {
                    runtime_id: vec![30, -31],
                    url: Some("https://example.test/docs".into()),
                },
            ),
            (
                action("Blank tab", "Browser Tab", "tab:switch:32"),
                ActionTarget::BrowserTab {
                    runtime_id: vec![32],
                    url: None,
                },
            ),
        ];

        for (selected, expected) in cases {
            assert_eq!(resolve(&selected, &empty, &empty, &[]).target, expected);
        }
    }

    #[test]
    fn malformed_target_ids_fall_back_without_partial_browser_parsing() {
        let empty = HashMap::new();
        for malformed in [
            action("Timer", "Timer", "timer:show:not-a-number"),
            action("Stopwatch", "Stopwatch", "stopwatch:show:nope"),
            action("Note", "Note", "note:open:"),
            action("Clipboard", "Clipboard", "clipboard:copy:nope"),
            action("Todo", "Todo", "todo:done:nope"),
            action("Window", "Windows", "window:close:nope"),
            action("Macro", "Macro", "mkmacro:run:0"),
            action("Tab", "Browser Tab", "tab:switch:1_bad_2"),
            action("Tab", "Browser Tab", "tab:switch:"),
        ] {
            assert!(matches!(
                resolve(&malformed, &empty, &empty, &[]).target,
                ActionTarget::Generic { .. }
            ));
        }
    }

    #[test]
    fn alias_membership_is_exact_and_legacy_folder_commands_are_not_targets() {
        let selected = action("Projects", "Folder", "C:/Projects");
        let folder_command = action("Remove", "Folder", "folder:remove:C:/Projects");
        let mut folders = HashMap::new();
        folders.insert(selected.action.clone(), None);
        folders.insert(folder_command.action.clone(), None);
        let empty = HashMap::new();

        assert!(matches!(
            resolve(&selected, &folders, &empty, &[]).target,
            ActionTarget::Folder { .. }
        ));
        assert!(matches!(
            resolve(
                &action("Projects", "Folder", "c:/projects"),
                &folders,
                &empty,
                &[]
            )
            .target,
            ActionTarget::Generic { .. }
        ));
        assert!(matches!(
            resolve(&folder_command, &folders, &empty, &[]).target,
            ActionTarget::Generic { .. }
        ));
    }

    #[test]
    fn custom_matching_uses_existing_label_and_command_identity() {
        let empty = HashMap::new();
        let persisted = Action {
            label: "Editor".into(),
            desc: "Original description".into(),
            action: "editor.exe".into(),
            args: Some("--original".into()),
        };
        let selected = Action {
            label: persisted.label.clone(),
            desc: "Search description".into(),
            action: persisted.action.clone(),
            args: Some("--search-override".into()),
        };

        let resolved = resolve(&selected, &empty, &empty, &[persisted]);
        assert_eq!(resolved.custom_action_index, Some(0));
        assert_eq!(resolved.selected_action, selected);
        assert!(matches!(
            resolved.target,
            ActionTarget::CustomAction { index: 0, .. }
        ));
    }

    #[test]
    fn specialized_custom_results_keep_both_identities() {
        let empty = HashMap::new();
        let selected = action("Daily", "Note", "note:open:daily");
        let resolved = resolve(&selected, &empty, &empty, std::slice::from_ref(&selected));

        assert_eq!(resolved.custom_action_index, Some(0));
        assert_eq!(
            resolved.target,
            ActionTarget::Note {
                slug: "daily".into()
            }
        );
    }

    #[test]
    fn persistent_and_ephemeral_resolution_remain_distinct() {
        let empty = HashMap::new();
        let note = action("Daily", "Note", "note:open:daily");
        let window = action("Activate", "Windows", "window:switch:50");

        assert_eq!(
            resolve(&note, &empty, &empty, &[]).target.persistent_ref(),
            Some(PersistableActionTargetRef::Note {
                slug: "daily".into()
            })
        );
        assert_eq!(
            resolve(&window, &empty, &empty, &[])
                .target
                .persistent_ref(),
            None
        );
    }

    #[test]
    fn window_close_retains_its_exact_selected_primary_action() {
        let empty = HashMap::new();
        let close = Action {
            label: "Close Calculator".into(),
            desc: "Windows".into(),
            action: "window:close:75".into(),
            args: Some("unchanged".into()),
        };

        let resolved = resolve(&close, &empty, &empty, &[]);
        assert_eq!(resolved.target, ActionTarget::Window { hwnd: 75 });
        assert_eq!(resolved.selected_action, close);
        assert_eq!(resolved.selected_action.action, "window:close:75");
    }
}
