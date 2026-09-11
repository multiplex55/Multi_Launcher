use super::model::MmWorkspace;
use std::sync::{Arc, Mutex, RwLock, Weak};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceDescriptor {
    pub id: String,
    pub name: String,
}

/// Instance-scoped view of the workspaces owned by one Multi Manager state.
///
/// Plugin discovery only needs stable IDs and display names, so the catalog
/// holds a weak reference rather than extending the state's lifetime.
#[derive(Default)]
pub struct WorkspaceCatalog {
    source: RwLock<Weak<Mutex<Vec<MmWorkspace>>>>,
}

impl WorkspaceCatalog {
    pub fn attach(&self, workspaces: &Arc<Mutex<Vec<MmWorkspace>>>) {
        if let Ok(mut source) = self.source.write() {
            *source = Arc::downgrade(workspaces);
        }
    }

    pub fn snapshot(&self) -> Vec<WorkspaceDescriptor> {
        let source = self.source.read().ok().and_then(|source| source.upgrade());
        source
            .and_then(|workspaces| {
                workspaces.lock().ok().map(|workspaces| {
                    workspaces
                        .iter()
                        .map(|workspace| WorkspaceDescriptor {
                            id: workspace.id.clone(),
                            name: workspace.name.clone(),
                        })
                        .collect()
                })
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspaces(id: &str, name: &str) -> Arc<Mutex<Vec<MmWorkspace>>> {
        Arc::new(Mutex::new(vec![MmWorkspace {
            id: id.into(),
            name: name.into(),
            ..MmWorkspace::default()
        }]))
    }

    #[test]
    fn catalogs_are_isolated_between_state_instances() {
        let first_source = workspaces("workspace-first", "First");
        let second_source = workspaces("workspace-second", "Second");
        let first = WorkspaceCatalog::default();
        let second = WorkspaceCatalog::default();
        first.attach(&first_source);
        second.attach(&second_source);

        assert_eq!(
            first.snapshot(),
            vec![WorkspaceDescriptor {
                id: "workspace-first".into(),
                name: "First".into(),
            }]
        );
        assert_eq!(
            second.snapshot(),
            vec![WorkspaceDescriptor {
                id: "workspace-second".into(),
                name: "Second".into(),
            }]
        );

        first_source.lock().unwrap()[0].name = "First renamed".into();
        assert_eq!(first.snapshot()[0].name, "First renamed");
        assert_eq!(second.snapshot()[0].name, "Second");

        drop(first_source);
        assert!(first.snapshot().is_empty());
        assert_eq!(second.snapshot()[0].id, "workspace-second");
    }
}
