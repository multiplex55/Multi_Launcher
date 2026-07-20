use super::config::{self, LoadedConfig, VersionedClipboardModifiersFile};
use super::model::{ClipboardModifierCatalog, ClipboardTemplate, SavedPipeline};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

pub type SharedClipboardModifierCatalog = Arc<RwLock<Arc<ClipboardModifierCatalog>>>;

#[derive(Clone)]
pub struct ClipboardModifierStore {
    pub path: PathBuf,
    pub catalog: SharedClipboardModifierCatalog,
    pub diagnostic: Arc<RwLock<Option<String>>>,
}

impl ClipboardModifierStore {
    pub fn new(settings_path: &Path) -> (Self, LoadedConfig) {
        let loaded = config::load_startup(settings_path);
        let shared = shared_default_catalog();
        *shared.write().unwrap() = Arc::new(loaded.catalog.clone());
        let store = Self {
            path: loaded.path.clone(),
            catalog: shared,
            diagnostic: Arc::new(RwLock::new(None)),
        };
        (store, loaded)
    }
    pub fn replace_valid(&self, catalog: ClipboardModifierCatalog) {
        *self.catalog.write().unwrap() = Arc::new(catalog);
        *self.diagnostic.write().unwrap() = None;
    }
    pub fn retain_with_error(&self, msg: String) {
        *self.diagnostic.write().unwrap() = Some(msg);
    }
    pub fn save(&self, model: &VersionedClipboardModifiersFile) -> anyhow::Result<()> {
        config::save_model_atomic(&self.path, model)
    }

    pub fn reload_now(&self) -> anyhow::Result<ClipboardModifierCatalog> {
        match config::load_current_or_migrate(&self.path) {
            Ok((_model, catalog)) => {
                self.replace_valid(catalog.clone());
                Ok(catalog)
            }
            Err(err) => {
                let msg = err.to_string();
                self.retain_with_error(msg.clone());
                anyhow::bail!(msg)
            }
        }
    }

    pub fn reset_to_factory_defaults(&self) -> anyhow::Result<ClipboardModifierCatalog> {
        let model = config::reset_to_defaults_with_backup(&self.path)?;
        let catalog = config::validate_model(&model)?;
        self.replace_valid(catalog.clone());
        Ok(catalog)
    }

    pub fn save_templates(
        &self,
        base: &ClipboardModifierCatalog,
        templates: Vec<ClipboardTemplate>,
    ) -> anyhow::Result<ClipboardModifierCatalog> {
        let mut model = config::model_from_catalog(base);
        model.templates = templates;
        let catalog = config::validate_model(&model)?;
        self.save(&model)?;
        self.replace_valid(catalog.clone());
        Ok(catalog)
    }

    pub fn save_pipelines(
        &self,
        base: &ClipboardModifierCatalog,
        pipelines: Vec<SavedPipeline>,
    ) -> anyhow::Result<ClipboardModifierCatalog> {
        let mut model = config::model_from_catalog(base);
        model.pipelines = pipelines;
        let catalog = config::validate_model(&model)?;
        self.save(&model)?;
        self.replace_valid(catalog.clone());
        Ok(catalog)
    }
}

pub fn shared_default_catalog() -> SharedClipboardModifierCatalog {
    static SHARED: OnceLock<SharedClipboardModifierCatalog> = OnceLock::new();
    SHARED
        .get_or_init(|| Arc::new(RwLock::new(Arc::new(super::defaults::default_catalog()))))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, s: &str) {
        std::fs::write(path, s).unwrap();
    }

    #[test]
    fn manual_reload_success_installs_new_catalog() {
        let d = tempfile::tempdir().unwrap();
        let settings = d.path().join("settings.json");
        let (store, _) = ClipboardModifierStore::new(&settings);
        let mut model = config::default_model();
        model.templates[0].label = "Changed Label".into();
        config::save_model_atomic(&store.path, &model).unwrap();
        let catalog = store.reload_now().unwrap();
        assert_eq!(catalog.templates[0].label, "Changed Label");
        assert!(store.diagnostic.read().unwrap().is_none());
    }

    #[test]
    fn manual_reload_failure_retains_last_valid_catalog() {
        let d = tempfile::tempdir().unwrap();
        let settings = d.path().join("settings.json");
        let (store, _) = ClipboardModifierStore::new(&settings);
        let before = store.catalog.read().unwrap().clone();
        write(&store.path, "{}");
        assert!(store.reload_now().is_err());
        let after = store.catalog.read().unwrap().clone();
        assert!(Arc::ptr_eq(&before, &after));
        assert!(store.diagnostic.read().unwrap().is_some());
    }

    #[test]
    fn factory_reset_creates_backup_and_clears_diagnostic() {
        let d = tempfile::tempdir().unwrap();
        let settings = d.path().join("settings.json");
        let (store, _) = ClipboardModifierStore::new(&settings);
        store.retain_with_error("bad config".into());
        let _ = store.reset_to_factory_defaults().unwrap();
        assert!(store.diagnostic.read().unwrap().is_none());
        assert!(std::fs::read_dir(d.path()).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .contains("factory-reset")
        }));
    }
}
