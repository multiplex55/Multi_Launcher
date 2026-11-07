use crate::actions::Action;
use crate::explorer_tabs::FolderListStore;
use crate::platform::app_data::AppDataRoot;
use crate::plugin::Plugin;
use std::sync::{Arc, RwLock};

pub(crate) type ExplorerTabsRoot = Arc<RwLock<Option<AppDataRoot>>>;

pub(crate) struct ExplorerTabsPlugin {
    root: ExplorerTabsRoot,
}
impl ExplorerTabsPlugin {
    pub(crate) fn new(root: ExplorerTabsRoot) -> Self {
        Self { root }
    }
    fn actions(&self) -> Vec<Action> {
        let mut actions = vec![Action {
            label: "Open Explorer Tabs".into(),
            desc: "Manage saved folder lists and review run progress".into(),
            action: "explorertabs:open".into(),
            args: None,
        }];
        let root = self.root.read().ok().and_then(|root| root.clone());
        match root {
            None => {
                actions[0].desc =
                    "Storage is not configured; open Explorer Tabs to manage lists".into()
            }
            Some(root) => match FolderListStore::open(&root) {
                Ok(store) => actions.extend(store.lists().iter().map(|list| Action {
                    label: format!("Open {} in Explorer Tabs", list.name()),
                    desc: "Open the saved ordered folders in a new Explorer window".into(),
                    action: format!("explorertabs:run:{}", list.id().get()),
                    args: None,
                })),
                Err(error) => {
                    actions[0].desc = format!(
                        "Folder-list storage unavailable: {}",
                        error.to_string().chars().take(200).collect::<String>()
                    )
                }
            },
        }
        actions
    }
}
impl Plugin for ExplorerTabsPlugin {
    fn name(&self) -> &str {
        "explorer_tabs"
    }
    fn description(&self) -> &str {
        "Open saved ordered folders, including duplicates, in a new Explorer window; review verified and uncertain results (query: `explorertabs`)"
    }
    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
    fn query_prefixes(&self) -> &[&str] {
        &["explorertabs"]
    }
    fn search(&self, query: &str) -> Vec<Action> {
        if query.trim().eq_ignore_ascii_case("explorertabs") {
            self.actions()
        } else {
            Vec::new()
        }
    }
    fn commands(&self) -> Vec<Action> {
        self.actions()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explorer_tabs::FolderPath;
    #[test]
    fn explorer_tabs_plugin_uses_configured_root_and_stable_saved_ids_without_writes() {
        let dir = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_settings_path(dir.path().join("settings.json")).unwrap();
        let shared = Arc::new(RwLock::new(None));
        let plugin = ExplorerTabsPlugin::new(Arc::clone(&shared));
        assert_eq!(plugin.search("explorertabs").len(), 1);
        *shared.write().unwrap() = Some(root.clone());
        assert_eq!(plugin.commands().len(), 1);
        assert!(
            !root
                .path()
                .join(crate::explorer_tabs::FOLDER_LIST_FILE_NAME)
                .exists()
        );
        let mut store = FolderListStore::open(&root).unwrap();
        let id = store
            .create("Work", vec![FolderPath::parse(r"C:\Work").unwrap()])
            .unwrap();
        let rows = plugin.search("explorertabs");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].action, format!("explorertabs:run:{}", id.get()));
        assert!(!rows[1].action.contains("Work"));
        assert_eq!(plugin.commands()[1].action, rows[1].action);
    }
}
