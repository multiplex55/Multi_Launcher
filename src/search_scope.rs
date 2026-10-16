use crate::actions::Action;
use std::path::Path;

/// The source category for a transient launcher search result.
///
/// This is search-time provenance only and is never persisted with [`Action`].
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum SearchResultCategory {
    Applications,
    Files,
    Notes,
    #[default]
    Commands,
}

/// An action together with the provider-owned category used by launcher scopes.
#[derive(Clone, Debug, PartialEq)]
pub struct LauncherSearchResult {
    pub action: Action,
    pub category: SearchResultCategory,
}

impl LauncherSearchResult {
    pub fn new(action: Action, category: SearchResultCategory) -> Self {
        Self { action, category }
    }

    pub fn command(action: Action) -> Self {
        Self::new(action, SearchResultCategory::Commands)
    }

    pub fn into_action(self) -> Action {
        self.action
    }
}

/// Classify an action from the launcher catalog without consulting the filesystem.
///
/// The custom prefix is legacy user-authored launcher content and defaults to
/// Applications. The indexed tail is Files except for recognized launch targets.
pub fn catalog_action_category(action: &Action, indexed_tail: bool) -> SearchResultCategory {
    if indexed_tail {
        return if is_executable_or_shortcut(&action.action) {
            SearchResultCategory::Applications
        } else {
            SearchResultCategory::Files
        };
    }

    match crate::commands::parse_action(action).ok() {
        Some(crate::commands::Command::Note(_)) => SearchResultCategory::Notes,
        Some(crate::commands::Command::Storage(
            crate::commands::StorageCommand::FolderAdd(_)
            | crate::commands::StorageCommand::FolderRemove(_),
        )) => SearchResultCategory::Files,
        Some(crate::commands::Command::External(_)) => {
            SearchResultCategory::Applications
        }
        None if is_executable_or_shortcut(&action.action) => SearchResultCategory::Applications,
        None => {
            SearchResultCategory::Applications
        }
        Some(_) => SearchResultCategory::Commands,
    }
}

fn is_executable_or_shortcut(target: &str) -> bool {
    Path::new(target)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "exe" | "com" | "bat" | "cmd" | "lnk" | "url" | "appref-ms"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::{SearchResultCategory, catalog_action_category};
    use crate::actions::Action;

    fn action(target: &str) -> Action {
        Action {
            label: target.into(),
            desc: String::new(),
            action: target.into(),
            args: None,
        }
    }

    #[test]
    fn launcher_scope_catalog_categories_follow_custom_and_indexed_ownership() {
        assert_eq!(
            catalog_action_category(&action("legacy:custom-action"), false),
            SearchResultCategory::Applications
        );
        assert_eq!(
            catalog_action_category(
                &Action {
                    desc: "Note".into(),
                    ..action("note:open:meeting-notes")
                },
                false,
            ),
            SearchResultCategory::Notes
        );
        assert_eq!(
            catalog_action_category(
                &Action {
                    desc: "Note".into(),
                    ..action("note:open:release.exe")
                },
                false,
            ),
            SearchResultCategory::Notes
        );
        assert_eq!(
            catalog_action_category(
                &Action {
                    desc: "Folder".into(),
                    ..action(r"folder:add:C:\Tools\tool.exe")
                },
                false,
            ),
            SearchResultCategory::Files
        );
        assert_eq!(
            catalog_action_category(&action("query:regex"), false),
            SearchResultCategory::Commands
        );
        assert_eq!(
            catalog_action_category(&action(r"C:\Apps\editor.exe"), true),
            SearchResultCategory::Applications
        );
        assert_eq!(
            catalog_action_category(&action(r"C:\Apps\editor.lnk"), true),
            SearchResultCategory::Applications
        );
        assert_eq!(
            catalog_action_category(&action(r"C:\Docs\notes.txt"), true),
            SearchResultCategory::Files
        );
    }
}
