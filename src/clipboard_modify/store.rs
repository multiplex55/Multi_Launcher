use super::config::{self, LoadedConfig, VersionedClipboardModifiersFile};
use super::model::{ClipboardModifierCatalog, ClipboardTemplate, SavedPipeline};
use once_cell::sync::Lazy;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

pub type SharedClipboardModifierCatalog = Arc<RwLock<Arc<ClipboardModifierCatalog>>>;
static CONFIG_TRANSACTION: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

/// One collection's exact editor baseline and replacement, with its sibling
/// always drawn from the latest validated disk snapshot.
pub enum CheckedCatalogEdit<'a> {
    Templates {
        baseline: &'a [ClipboardTemplate],
        draft: Vec<ClipboardTemplate>,
    },
    Pipelines {
        baseline: &'a [SavedPipeline],
        draft: Vec<SavedPipeline>,
    },
}
#[derive(Debug)]
pub enum CheckedCatalogSave {
    Saved(Arc<ClipboardModifierCatalog>),
    Conflict,
    Superseded,
}
#[derive(Debug)]
pub enum CheckedCatalogError {
    Unavailable,
    InvalidCurrent,
    InvalidDraft,
    WriteFailed,
}

#[derive(Debug)]
pub enum CheckedCatalogRead {
    Loaded(Arc<ClipboardModifierCatalog>),
    Changed,
    Superseded,
}

#[derive(Clone)]
pub struct ClipboardModifierStore {
    pub path: PathBuf,
    pub catalog: SharedClipboardModifierCatalog,
    pub diagnostic: Arc<RwLock<Option<String>>>,
}

impl ClipboardModifierStore {
    /// Refresh the shared catalog from a validated, stable current file without
    /// creating, migrating, or writing it.
    pub fn refresh_checked(
        &self,
        current: &dyn Fn() -> bool,
    ) -> Result<CheckedCatalogRead, CheckedCatalogError> {
        if !current() {
            return Ok(CheckedCatalogRead::Superseded);
        }
        let _transaction = CONFIG_TRANSACTION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !current() {
            return Ok(CheckedCatalogRead::Superseded);
        }
        let observed = config::read_checked_snapshot(&self.path);
        if !current() {
            return Ok(CheckedCatalogRead::Superseded);
        }
        let bytes = observed.map_err(|_| CheckedCatalogError::Unavailable)?;
        let decoded = config::decode_current(&bytes);
        if !current() {
            return Ok(CheckedCatalogRead::Superseded);
        }
        let (_, catalog) = decoded.map_err(|_| CheckedCatalogError::InvalidCurrent)?;
        let reread = config::read_checked_snapshot(&self.path);
        if !current() {
            return Ok(CheckedCatalogRead::Superseded);
        }
        if reread.map_err(|_| CheckedCatalogError::Unavailable)? != bytes {
            return Ok(CheckedCatalogRead::Changed);
        }
        let catalog = Arc::new(catalog);
        let mut published = self.catalog.write().unwrap();
        if !current() {
            return Ok(CheckedCatalogRead::Superseded);
        }
        *published = catalog.clone();
        drop(published);
        *self.diagnostic.write().unwrap() = None;
        Ok(CheckedCatalogRead::Loaded(catalog))
    }
    pub fn save_checked(
        &self,
        edit: CheckedCatalogEdit<'_>,
        current: &dyn Fn() -> bool,
    ) -> Result<CheckedCatalogSave, CheckedCatalogError> {
        if !current() {
            return Ok(CheckedCatalogSave::Superseded);
        }
        let _transaction = CONFIG_TRANSACTION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !current() {
            return Ok(CheckedCatalogSave::Superseded);
        }
        let observed = config::read_checked_snapshot(&self.path);
        if !current() {
            return Ok(CheckedCatalogSave::Superseded);
        }
        let original = observed.map_err(|_| CheckedCatalogError::Unavailable)?;
        let decoded = config::decode_current(&original);
        if !current() {
            return Ok(CheckedCatalogSave::Superseded);
        }
        let (mut model, disk) = decoded.map_err(|_| CheckedCatalogError::InvalidCurrent)?;
        match edit {
            CheckedCatalogEdit::Templates { baseline, draft } => {
                if disk.templates != baseline {
                    return Ok(CheckedCatalogSave::Conflict);
                }
                model.templates = draft;
            }
            CheckedCatalogEdit::Pipelines { baseline, draft } => {
                if disk.pipelines != baseline {
                    return Ok(CheckedCatalogSave::Conflict);
                }
                model.pipelines = draft;
            }
        }
        let validated = config::validate_model(&model);
        if !current() {
            return Ok(CheckedCatalogSave::Superseded);
        }
        let catalog = Arc::new(validated.map_err(|_| CheckedCatalogError::InvalidDraft)?);
        let serialized = config::serialize_model(&model);
        if !current() {
            return Ok(CheckedCatalogSave::Superseded);
        }
        let bytes = serialized.map_err(|_| CheckedCatalogError::InvalidDraft)?;
        #[derive(Clone, Copy)]
        enum Refusal {
            Stale,
            Changed,
            Unreadable,
        }
        let refusal = std::cell::Cell::new(None);
        let result = crate::common::atomic_file::save_atomic_if(&self.path, &bytes, || {
            if !current() {
                refusal.set(Some(Refusal::Stale));
                return false;
            }
            let observed = config::read_checked_snapshot(&self.path);
            if !current() {
                refusal.set(Some(Refusal::Stale));
                return false;
            }
            match observed {
                Ok(bytes) if bytes == original => true,
                Ok(_) => {
                    refusal.set(Some(Refusal::Changed));
                    false
                }
                Err(_) => {
                    refusal.set(Some(Refusal::Unreadable));
                    false
                }
            }
        });
        if result.is_err() {
            return match refusal.get() {
                Some(Refusal::Stale) => Ok(CheckedCatalogSave::Superseded),
                Some(Refusal::Changed) => Ok(CheckedCatalogSave::Conflict),
                Some(Refusal::Unreadable) => Err(CheckedCatalogError::Unavailable),
                None => Err(CheckedCatalogError::WriteFailed),
            };
        }
        *self.catalog.write().unwrap() = catalog.clone();
        *self.diagnostic.write().unwrap() = None;
        Ok(CheckedCatalogSave::Saved(catalog))
    }
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
        let _transaction = CONFIG_TRANSACTION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.path.exists() {
            config::load_current_or_migrate(&self.path)
                .map_err(anyhow::Error::new)
                .map_err(|err| {
                    anyhow::anyhow!("refusing to replace invalid Clipboard Modify config: {err}")
                })?;
        }
        config::save_model_atomic(&self.path, model)
    }

    pub fn reload_now(&self) -> anyhow::Result<ClipboardModifierCatalog> {
        let _transaction = CONFIG_TRANSACTION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
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
        let _transaction = CONFIG_TRANSACTION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
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
        let _transaction = CONFIG_TRANSACTION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut model = self.latest_model_or(base)?;
        model.templates = templates;
        let catalog = config::validate_model(&model)?;
        config::save_model_atomic(&self.path, &model)?;
        self.replace_valid(catalog.clone());
        Ok(catalog)
    }

    pub fn save_pipelines(
        &self,
        base: &ClipboardModifierCatalog,
        pipelines: Vec<SavedPipeline>,
    ) -> anyhow::Result<ClipboardModifierCatalog> {
        let _transaction = CONFIG_TRANSACTION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut model = self.latest_model_or(base)?;
        model.pipelines = pipelines;
        let catalog = config::validate_model(&model)?;
        config::save_model_atomic(&self.path, &model)?;
        self.replace_valid(catalog.clone());
        Ok(catalog)
    }

    fn latest_model_or(
        &self,
        base: &ClipboardModifierCatalog,
    ) -> anyhow::Result<VersionedClipboardModifiersFile> {
        if self.path.exists() {
            config::load_current_or_migrate(&self.path)
                .map(|(model, _)| model)
                .map_err(anyhow::Error::new)
        } else {
            Ok(config::model_from_catalog(base))
        }
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

    #[test]
    fn checked_refresh_reads_current_without_rewriting_or_publishing_bad_data() {
        let dir = tempfile::tempdir().unwrap();
        let (store, base) = isolated_store(&dir.path().join("config.json"));
        let mut model = config::model_from_catalog(&base);
        model.templates[0].label = "External edit".into();
        config::save_model_atomic(&store.path, &model).unwrap();
        let bytes = std::fs::read(&store.path).unwrap();
        let CheckedCatalogRead::Loaded(catalog) = store.refresh_checked(&|| true).unwrap() else {
            panic!("expected refresh")
        };
        assert_eq!(catalog.templates[0].label, "External edit");
        assert_eq!(std::fs::read(&store.path).unwrap(), bytes);
        let published = store.catalog.read().unwrap().clone();
        std::fs::write(&store.path, b"{broken").unwrap();
        assert!(matches!(
            store.refresh_checked(&|| true),
            Err(CheckedCatalogError::InvalidCurrent)
        ));
        assert!(Arc::ptr_eq(&published, &store.catalog.read().unwrap()));
        assert_eq!(std::fs::read(&store.path).unwrap(), b"{broken");
        config::save_model_atomic(&store.path, &model).unwrap();
        let calls = std::cell::Cell::new(0);
        let revoked_after_lock = || {
            calls.set(calls.get() + 1);
            calls.get() < 6
        };
        assert!(matches!(
            store.refresh_checked(&revoked_after_lock),
            Ok(CheckedCatalogRead::Superseded)
        ));
        assert!(Arc::ptr_eq(&published, &store.catalog.read().unwrap()));
        let calls = std::cell::Cell::new(0);
        let revoked = || {
            calls.set(calls.get() + 1);
            calls.get() < 5
        };
        assert!(matches!(
            store.refresh_checked(&revoked),
            Ok(CheckedCatalogRead::Superseded)
        ));
        assert!(Arc::ptr_eq(&published, &store.catalog.read().unwrap()));
    }

    fn isolated_store(path: &Path) -> (ClipboardModifierStore, ClipboardModifierCatalog) {
        let model = config::default_model();
        config::save_model_atomic(path, &model).unwrap();
        let catalog = config::validate_model(&model).unwrap();
        (
            ClipboardModifierStore {
                path: path.to_owned(),
                catalog: Arc::new(RwLock::new(Arc::new(catalog.clone()))),
                diagnostic: Arc::new(RwLock::new(None)),
            },
            catalog,
        )
    }

    #[test]
    fn checked_save_rejects_same_half_and_merges_latest_sibling() {
        let dir = tempfile::tempdir().unwrap();
        let (store, base) = isolated_store(&dir.path().join("config.json"));
        let mut other = config::model_from_catalog(&base);
        other.pipelines[0].label = "Updated sibling".into();
        config::save_model_atomic(&store.path, &other).unwrap();
        let mut draft = base.templates.clone();
        draft[0].label = "Edited template".into();
        let saved = store
            .save_checked(
                CheckedCatalogEdit::Templates {
                    baseline: &base.templates,
                    draft: draft.clone(),
                },
                &|| true,
            )
            .unwrap();
        let CheckedCatalogSave::Saved(saved) = saved else {
            panic!("expected save")
        };
        assert_eq!(saved.templates[0].label, "Edited template");
        assert_eq!(saved.pipelines[0].label, "Updated sibling");
        let before = std::fs::read(&store.path).unwrap();
        let published = store.catalog.read().unwrap().clone();
        assert!(matches!(
            store.save_checked(
                CheckedCatalogEdit::Templates {
                    baseline: &base.templates,
                    draft
                },
                &|| true
            ),
            Ok(CheckedCatalogSave::Conflict)
        ));
        assert_eq!(std::fs::read(&store.path).unwrap(), before);
        assert!(Arc::ptr_eq(&published, &store.catalog.read().unwrap()));
        let mut external = config::model_from_catalog(&saved);
        external.templates[0].label = "New sibling template".into();
        config::save_model_atomic(&store.path, &external).unwrap();
        let mut pipeline_draft = saved.pipelines.clone();
        pipeline_draft[0].label = "Edited pipeline".into();
        let CheckedCatalogSave::Saved(merged) = store
            .save_checked(
                CheckedCatalogEdit::Pipelines {
                    baseline: &saved.pipelines,
                    draft: pipeline_draft,
                },
                &|| true,
            )
            .unwrap()
        else {
            panic!("expected pipeline save")
        };
        assert_eq!(merged.templates[0].label, "New sibling template");
        assert_eq!(merged.pipelines[0].label, "Edited pipeline");
    }

    #[test]
    fn checked_save_guards_final_authority_disk_and_reference_integrity() {
        use std::cell::Cell;
        let dir = tempfile::tempdir().unwrap();
        let (store, base) = isolated_store(&dir.path().join("config.json"));
        let original = std::fs::read(&store.path).unwrap();
        let published = store.catalog.read().unwrap().clone();
        let mut pipelines = base.pipelines.clone();
        pipelines[0].stages.push(super::super::model::StageSpec {
            operation: super::super::model::OperationId::Template,
            arguments: super::super::model::StageArguments {
                name: Some("not-present".into()),
                ..Default::default()
            },
        });
        assert!(matches!(
            store.save_checked(
                CheckedCatalogEdit::Pipelines {
                    baseline: &base.pipelines,
                    draft: pipelines
                },
                &|| true
            ),
            Err(CheckedCatalogError::InvalidDraft)
        ));
        let calls = Cell::new(0);
        let revoke = || {
            calls.set(calls.get() + 1);
            calls.get() < 8
        };
        assert!(matches!(
            store.save_checked(
                CheckedCatalogEdit::Templates {
                    baseline: &base.templates,
                    draft: base.templates.clone()
                },
                &revoke
            ),
            Ok(CheckedCatalogSave::Superseded)
        ));
        assert_eq!(std::fs::read(&store.path).unwrap(), original);
        assert!(Arc::ptr_eq(&published, &store.catalog.read().unwrap()));
        let calls = Cell::new(0);
        let external = b"external replacement";
        let replace = || {
            calls.set(calls.get() + 1);
            if calls.get() == 7 {
                std::fs::write(&store.path, external).unwrap();
            }
            true
        };
        assert!(matches!(
            store.save_checked(
                CheckedCatalogEdit::Templates {
                    baseline: &base.templates,
                    draft: base.templates.clone()
                },
                &replace
            ),
            Ok(CheckedCatalogSave::Conflict)
        ));
        assert_eq!(std::fs::read(&store.path).unwrap(), external);
        assert!(Arc::ptr_eq(&published, &store.catalog.read().unwrap()));
        assert!(matches!(
            store.save_checked(
                CheckedCatalogEdit::Templates {
                    baseline: &base.templates,
                    draft: base.templates.clone()
                },
                &|| true
            ),
            Err(CheckedCatalogError::InvalidCurrent)
        ));
        assert_eq!(std::fs::read(&store.path).unwrap(), external);
        assert!(Arc::ptr_eq(&published, &store.catalog.read().unwrap()));
    }

    #[test]
    fn checked_save_never_creates_or_migrates_missing_or_unsupported_file() {
        let dir = tempfile::tempdir().unwrap();
        let (store, base) = isolated_store(&dir.path().join("config.json"));
        let published = store.catalog.read().unwrap().clone();
        for raw in [
            b"{\"schema_version\":0,\"templates\":[],\"pipelines\":[]}".as_slice(),
            b"{\"schema_version\":999,\"templates\":[],\"pipelines\":[]}".as_slice(),
            b"{broken".as_slice(),
            b"{\"schema_version\":1,\"templates\":[],\"templates\":[],\"pipelines\":[]}".as_slice(),
            b"{\"schema_version\":1,\"templates\":[{\"id\":\"a\",\"id\":\"b\",\"label\":\"A\",\"template\":\"{{clipboard}}\"}],\"pipelines\":[]}".as_slice(),
        ] {
            std::fs::write(&store.path, raw).unwrap();
            assert!(matches!(
                store.save_checked(
                    CheckedCatalogEdit::Templates {
                        baseline: &base.templates,
                        draft: base.templates.clone()
                    },
                    &|| true
                ),
                Err(CheckedCatalogError::InvalidCurrent)
            ));
            assert_eq!(std::fs::read(&store.path).unwrap(), raw);
            assert!(Arc::ptr_eq(&published, &store.catalog.read().unwrap()));
        }
        std::fs::remove_file(&store.path).unwrap();
        assert!(matches!(
            store.save_checked(
                CheckedCatalogEdit::Templates {
                    baseline: &base.templates,
                    draft: base.templates.clone()
                },
                &|| true
            ),
            Err(CheckedCatalogError::Unavailable)
        ));
        assert!(!store.path.exists());
        assert!(Arc::ptr_eq(&published, &store.catalog.read().unwrap()));
    }

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

    #[test]
    fn concurrent_template_and_pipeline_editor_saves_preserve_both_changes() {
        let dir = tempfile::tempdir().unwrap();
        let settings = dir.path().join("settings.json");
        let (store, loaded) = ClipboardModifierStore::new(&settings);
        let base = loaded.catalog;
        let barrier = Arc::new(std::sync::Barrier::new(3));

        let template_thread = {
            let store = store.clone();
            let base = base.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                let mut templates = base.templates.clone();
                templates[0].label = "Concurrent Template".into();
                barrier.wait();
                store.save_templates(&base, templates).unwrap();
            })
        };
        let pipeline_thread = {
            let store = store.clone();
            let base = base.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                let mut pipelines = base.pipelines.clone();
                pipelines[0].label = "Concurrent Pipeline".into();
                barrier.wait();
                store.save_pipelines(&base, pipelines).unwrap();
            })
        };
        barrier.wait();
        template_thread.join().unwrap();
        pipeline_thread.join().unwrap();

        let (_, catalog) = config::load_current_or_migrate(&store.path).unwrap();
        assert_eq!(catalog.templates[0].label, "Concurrent Template");
        assert_eq!(catalog.pipelines[0].label, "Concurrent Pipeline");
    }
}
