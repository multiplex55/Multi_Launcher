use super::MkMacroStore;
use anyhow::{Result, bail};
use once_cell::sync::Lazy;
use std::sync::{Arc, RwLock};

static STORE: Lazy<RwLock<Option<Arc<MkMacroStore>>>> = Lazy::new(|| RwLock::new(None));
pub fn set_shared_store(store: Arc<MkMacroStore>) {
    *STORE.write().unwrap() = Some(store);
}
fn store() -> Result<Arc<MkMacroStore>> {
    STORE
        .read()
        .unwrap()
        .clone()
        .ok_or_else(|| anyhow::anyhow!("macro runtime is not initialized"))
}
pub fn run(id: u64) -> Result<()> {
    let s = store()?;
    let doc = s.snapshot();
    let m = doc
        .macros
        .iter()
        .find(|m| m.id == id)
        .ok_or_else(|| anyhow::anyhow!("unknown macro id {id}"))?;
    if !m.enabled {
        bail!("macro '{}' is disabled", m.name);
    }
    if !s.can_run() {
        bail!("macro document has fatal validation errors");
    }
    let _ = crate::mkmacro::compile(m)
        .map_err(|_| anyhow::anyhow!("macro has fatal validation errors"))?;
    Ok(())
}
pub fn pause() -> Result<()> {
    store().map(|_| ())
}
pub fn resume() -> Result<()> {
    store().map(|_| ())
}
pub fn stop() -> Result<()> {
    store().map(|_| ())
}
pub fn record() -> Result<()> {
    store().map(|_| ())
}
pub fn record_stop() -> Result<()> {
    store().map(|_| ())
}
