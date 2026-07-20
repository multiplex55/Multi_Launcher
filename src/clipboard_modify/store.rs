use super::config::{self, LoadedConfig, VersionedClipboardModifiersFile};
use super::model::{ClipboardModifierCatalog, ClipboardTemplate};
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
}

pub fn shared_default_catalog() -> SharedClipboardModifierCatalog {
    static SHARED: OnceLock<SharedClipboardModifierCatalog> = OnceLock::new();
    SHARED
        .get_or_init(|| Arc::new(RwLock::new(Arc::new(super::defaults::default_catalog()))))
        .clone()
}
