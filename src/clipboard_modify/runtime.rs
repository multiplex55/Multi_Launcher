pub use super::store::{
    ClipboardModifierStore, SharedClipboardModifierCatalog, shared_default_catalog,
};

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock};

use serde::Deserialize;

use super::actions::{ClipboardModifyActionPayload, decode_action_payload};
use super::config::LoadedConfig;
use super::model::ClipboardModifierCatalog;

use super::clipboard::{ClipboardError, ProductionClipboardService, production_clipboard_service};
use super::executor::Cancellation;
use super::parser::ClipboardModifyIntent;
use super::pipeline::find_pipeline;

static SERVICE: OnceLock<Arc<ProductionClipboardService>> = OnceLock::new();

#[derive(Clone)]
pub struct ClipboardModifyRuntime {
    pub store: ClipboardModifierStore,
    pub diagnostic: Arc<std::sync::RwLock<Option<String>>>,
}

impl ClipboardModifyRuntime {
    pub fn new(
        settings_path: &Path,
        shared_catalog: SharedClipboardModifierCatalog,
    ) -> (Self, LoadedConfig) {
        let loaded = super::config::load_startup(settings_path);
        *shared_catalog.write().unwrap() = Arc::new(loaded.catalog.clone());
        let store = ClipboardModifierStore {
            path: loaded.path.clone(),
            catalog: shared_catalog,
            diagnostic: Arc::new(std::sync::RwLock::new(None)),
        };
        (
            Self {
                diagnostic: Arc::clone(&store.diagnostic),
                store,
            },
            loaded,
        )
    }

    pub fn replace_catalog(&self, catalog: ClipboardModifierCatalog) {
        self.store.replace_valid(catalog);
    }

    pub fn catalog_snapshot(&self) -> Arc<ClipboardModifierCatalog> {
        self.store.catalog.read().unwrap().clone()
    }

    pub fn reload_now(&self) -> anyhow::Result<ClipboardModifierCatalog> {
        self.store.reload_now()
    }

    pub fn reset_to_factory_defaults(&self) -> anyhow::Result<ClipboardModifierCatalog> {
        self.store.reset_to_factory_defaults()
    }
}

pub fn clipboard_service() -> Arc<ProductionClipboardService> {
    SERVICE
        .get_or_init(|| Arc::new(production_clipboard_service()))
        .clone()
}

#[derive(Debug, Deserialize)]
#[serde(tag = "intent", rename_all = "kebab-case")]
enum LegacyExecutePayload {
    Stages {
        stages: ClipboardModifyActionPayload,
    },
    Template {
        payload: ClipboardModifyActionPayload,
    },
    SavedPipeline {
        payload: ClipboardModifyActionPayload,
    },
}

pub fn decode_execute_payload_for_gui(
    encoded_or_json: &str,
) -> Result<ClipboardModifyActionPayload, String> {
    if let Ok(payload) = decode_action_payload(encoded_or_json) {
        return Ok(payload);
    }
    match serde_json::from_str::<LegacyExecutePayload>(encoded_or_json)
        .map_err(|err| format!("invalid clipboard modify action: {err}"))?
    {
        LegacyExecutePayload::Stages { stages }
        | LegacyExecutePayload::Template { payload: stages }
        | LegacyExecutePayload::SavedPipeline { payload: stages } => Ok(stages),
    }
}

pub fn execute_action_args(
    args: Option<&str>,
    catalog: &SharedClipboardModifierCatalog,
) -> Result<(), ClipboardError> {
    let encoded = args.unwrap_or("");
    let payload = decode_execute_payload_for_gui(encoded).map_err(ClipboardError::Config)?;
    let snapshot = catalog.read().unwrap().clone();
    let cancel = AtomicBool::new(false);
    match payload {
        ClipboardModifyActionPayload::ExecuteAdHocStages { stages } => {
            clipboard_service().apply_stages(&stages, snapshot.as_ref(), "cm execute", &cancel)?;
        }
        ClipboardModifyActionPayload::ExecuteTemplate { name } => {
            let stages = vec![super::model::StageSpec {
                operation: super::model::OperationId::Template,
                arguments: super::model::StageArguments {
                    name: Some(name.clone()),
                    ..Default::default()
                },
            }];
            clipboard_service().apply_stages(
                &stages,
                snapshot.as_ref(),
                &format!("cm template {name}"),
                &cancel,
            )?;
        }
        ClipboardModifyActionPayload::ExecuteSavedPipeline { name } => {
            if find_pipeline(snapshot.as_ref(), &name).is_none() {
                return Err(ClipboardError::Config(format!("unknown pipeline {name}")));
            }
            clipboard_service().apply_pipeline(
                &name,
                snapshot.as_ref(),
                &format!("cm apply {name}"),
                &cancel,
            )?;
        }
        ClipboardModifyActionPayload::Undo => {
            clipboard_service().undo()?;
        }
        ClipboardModifyActionPayload::OpenDialogSection { .. } => {
            return Err(ClipboardError::Config(
                "open-dialog payload cannot be executed".into(),
            ));
        }
    }
    Ok(())
}

pub fn execute_intent<C: Cancellation + ?Sized>(
    intent: ClipboardModifyIntent,
    catalog: &SharedClipboardModifierCatalog,
    cancellation: &C,
) -> Result<(), ClipboardError> {
    let snapshot = catalog.read().unwrap().clone();
    match intent {
        ClipboardModifyIntent::Stages(stages) => {
            clipboard_service().apply_stages(
                &stages,
                snapshot.as_ref(),
                "cm execute",
                cancellation,
            )?;
        }
        ClipboardModifyIntent::ApplyTemplate { name } => {
            let stages = vec![super::model::StageSpec {
                operation: super::model::OperationId::Template,
                arguments: super::model::StageArguments {
                    name: Some(name.clone()),
                    ..Default::default()
                },
            }];
            clipboard_service().apply_stages(
                &stages,
                snapshot.as_ref(),
                &format!("cm template {name}"),
                cancellation,
            )?;
        }
        ClipboardModifyIntent::ApplySavedPipeline { name } => {
            clipboard_service().apply_pipeline(
                &name,
                snapshot.as_ref(),
                &format!("cm apply {name}"),
                cancellation,
            )?;
        }
        ClipboardModifyIntent::Undo => {
            clipboard_service().undo()?;
        }
    }
    Ok(())
}

pub fn undo() -> Result<(), ClipboardError> {
    clipboard_service().undo()?;
    Ok(())
}
