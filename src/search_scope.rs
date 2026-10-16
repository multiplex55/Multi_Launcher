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

/// The session-local category shown by the launcher result list.
///
/// This selection is presentation state. It is not part of an [`Action`] or
/// persisted in launcher settings.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum SearchScope {
    #[default]
    All,
    Applications,
    Files,
    Notes,
    Commands,
}

impl SearchScope {
    pub const ALL: [Self; 5] = [
        Self::All,
        Self::Applications,
        Self::Files,
        Self::Notes,
        Self::Commands,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Applications => "Applications",
            Self::Files => "Files",
            Self::Notes => "Notes",
            Self::Commands => "Commands",
        }
    }

    pub const fn short_label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Applications => "Apps",
            Self::Files => "Files",
            Self::Notes => "Notes",
            Self::Commands => "Commands",
        }
    }

    pub const fn shortcut_number(self) -> u8 {
        match self {
            Self::All => 1,
            Self::Applications => 2,
            Self::Files => 3,
            Self::Notes => 4,
            Self::Commands => 5,
        }
    }

    pub const fn from_shortcut_number(number: u8) -> Option<Self> {
        match number {
            1 => Some(Self::All),
            2 => Some(Self::Applications),
            3 => Some(Self::Files),
            4 => Some(Self::Notes),
            5 => Some(Self::Commands),
            _ => None,
        }
    }

    pub const fn includes(self, category: SearchResultCategory) -> bool {
        match self {
            Self::All => true,
            Self::Applications => matches!(category, SearchResultCategory::Applications),
            Self::Files => matches!(category, SearchResultCategory::Files),
            Self::Notes => matches!(category, SearchResultCategory::Notes),
            Self::Commands => matches!(category, SearchResultCategory::Commands),
        }
    }
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
        Some(crate::commands::Command::External(_)) => SearchResultCategory::Applications,
        None if is_executable_or_shortcut(&action.action) => SearchResultCategory::Applications,
        None => SearchResultCategory::Applications,
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
    use super::{SearchResultCategory, SearchScope, catalog_action_category};
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

    #[test]
    fn launcher_scope_categories_and_shortcuts_are_stable() {
        assert_eq!(SearchScope::default(), SearchScope::All);
        assert_eq!(SearchScope::ALL[0], SearchScope::All);
        assert_eq!(SearchScope::ALL[4], SearchScope::Commands);
        for (number, scope) in SearchScope::ALL.into_iter().enumerate() {
            assert_eq!(scope.shortcut_number(), number as u8 + 1);
            assert_eq!(
                SearchScope::from_shortcut_number(number as u8 + 1),
                Some(scope)
            );
        }
        assert_eq!(SearchScope::from_shortcut_number(0), None);
        assert_eq!(SearchScope::from_shortcut_number(6), None);
        assert!(SearchScope::Notes.includes(SearchResultCategory::Notes));
        assert!(!SearchScope::Notes.includes(SearchResultCategory::Commands));
        assert!(SearchScope::All.includes(SearchResultCategory::Commands));
    }
}
