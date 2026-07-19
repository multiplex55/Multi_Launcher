pub use super::store::{
    ClipboardModifierStore, SharedClipboardModifierCatalog, shared_default_catalog,
};

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock};

use serde::Deserialize;

use super::clipboard::{ClipboardError, ProductionClipboardService, production_clipboard_service};
use super::executor::Cancellation;
use super::model::StageSpec;
use super::parser::ClipboardModifyIntent;
use super::pipeline::find_pipeline;

static SERVICE: OnceLock<Arc<ProductionClipboardService>> = OnceLock::new();

pub fn clipboard_service() -> Arc<ProductionClipboardService> {
    SERVICE
        .get_or_init(|| Arc::new(production_clipboard_service()))
        .clone()
}

#[derive(Debug, Deserialize)]
#[serde(tag = "intent", rename_all = "kebab-case")]
enum ExecutePayload {
    Stages { stages: Vec<StageSpec> },
    SavedPipeline { name: String },
}

pub fn execute_action_args(
    args: Option<&str>,
    catalog: &SharedClipboardModifierCatalog,
) -> Result<(), ClipboardError> {
    let payload: ExecutePayload = serde_json::from_str(args.unwrap_or(""))
        .map_err(|e| ClipboardError::Config(format!("invalid clipboard modify action: {e}")))?;
    let snapshot = catalog.read().unwrap().clone();
    let cancel = AtomicBool::new(false);
    match payload {
        ExecutePayload::Stages { stages } => {
            clipboard_service().apply_stages(&stages, snapshot.as_ref(), "cm execute", &cancel)?;
        }
        ExecutePayload::SavedPipeline { name } => {
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
        ClipboardModifyIntent::ApplySavedPipeline { name } => {
            clipboard_service().apply_pipeline(
                &name,
                snapshot.as_ref(),
                &format!("cm apply {name}"),
                cancellation,
            )?;
        }
    }
    Ok(())
}

pub fn undo() -> Result<(), ClipboardError> {
    clipboard_service().undo()?;
    Ok(())
}
