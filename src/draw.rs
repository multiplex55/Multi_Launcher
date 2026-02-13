use once_cell::sync::Lazy;
use std::sync::Mutex;

#[derive(Default)]
pub struct DrawRuntime;

static DRAW_RUNTIME: Lazy<DrawRuntime> = Lazy::new(DrawRuntime::default);

static DRAW_START_HOOK: Lazy<Mutex<Option<Box<dyn Fn() -> anyhow::Result<()> + Send + Sync>>>> =
    Lazy::new(|| Mutex::new(None));

pub fn runtime() -> &'static DrawRuntime {
    &DRAW_RUNTIME
}

pub fn set_runtime_start_hook(hook: Option<Box<dyn Fn() -> anyhow::Result<()> + Send + Sync>>) {
    if let Ok(mut guard) = DRAW_START_HOOK.lock() {
        *guard = hook;
    }
}

impl DrawRuntime {
    pub fn start(&self) -> anyhow::Result<()> {
        if let Ok(guard) = DRAW_START_HOOK.lock() {
            if let Some(ref hook) = *guard {
                return hook();
            }
        }
        Ok(())
    }
}
