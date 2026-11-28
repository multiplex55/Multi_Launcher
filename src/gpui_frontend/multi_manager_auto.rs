//! One launcher-lifetime, read-only startup reconnect.
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use multi_launcher::multi_manager::checked::{self, ReconnectCatalogPlan};

use super::{HashWakeSignal, RestartAdmission};

pub(super) struct Owner {
    started: AtomicBool,
    current: AtomicBool,
    plan: Mutex<Option<Arc<ReconnectCatalogPlan>>>,
    error: Mutex<Option<String>>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
    pub wake: Arc<HashWakeSignal>,
}

impl Default for Owner {
    fn default() -> Self {
        Self {
            started: AtomicBool::new(false),
            current: AtomicBool::new(true),
            plan: Mutex::new(None),
            error: Mutex::new(None),
            worker: Mutex::new(None),
            wake: Arc::new(HashWakeSignal::default()),
        }
    }
}

impl Owner {
    pub fn start(
        self: &Arc<Self>,
        enabled: bool,
        path: std::path::PathBuf,
        admission: Arc<RestartAdmission>,
    ) {
        if self.started.swap(true, Ordering::AcqRel) || !enabled {
            return;
        }
        let Some(permit) = admission.admit() else {
            return;
        };
        let owner = self.clone();
        let worker = std::thread::Builder::new()
            .name("gpui-mm-auto-reconnect".into())
            .spawn(move || {
                let _permit = permit;
                let current = || owner.current.load(Ordering::Acquire) && !admission.is_fenced();
                let result = checked::reconnect_catalog_from_path_checked(
                    &path,
                    current,
                    &checked::NativeReconnect,
                );
                if current() {
                    match result {
                        Ok(plan) => {
                            *owner
                                .plan
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                                Some(Arc::new(plan))
                        }
                        Err(error) => {
                            *owner
                                .error
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                                Some(format!("Automatic reconnect failed: {error:#}"));
                        }
                    }
                    owner.wake.notify();
                }
            });
        let worker = match worker {
            Ok(worker) => worker,
            Err(error) => {
                *self
                    .error
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) =
                    Some(format!("Could not start automatic reconnect: {error}"));
                self.wake.notify();
                return;
            }
        };
        *self
            .worker
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(worker);
    }

    pub fn plan(&self) -> Option<Arc<ReconnectCatalogPlan>> {
        if !self.current.load(Ordering::Acquire) {
            return None;
        }
        self.plan.lock().ok()?.clone()
    }

    pub fn is_current(&self) -> bool {
        self.current.load(Ordering::Acquire)
    }

    pub fn error(&self) -> Option<String> {
        if !self.is_current() {
            return None;
        }
        self.error.lock().ok()?.clone()
    }

    pub fn retire(&self) {
        self.current.store(false, Ordering::Release);
        self.plan
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
    }

    pub fn shutdown(&self) {
        self.retire();
        self.wake.close();
        if let Some(worker) = self
            .worker
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_default_is_enabled_and_owner_runs_at_most_once() {
        assert!(multi_launcher::settings::MultiManagerSettings::default().auto_reconnect_on_load);
        let admission = Arc::new(RestartAdmission::default());
        let disabled = Arc::new(Owner::default());
        disabled.start(false, "unused.json".into(), admission.clone());
        assert!(disabled.worker.lock().unwrap().is_none());
        disabled.start(true, "unused.json".into(), admission.clone());
        assert!(disabled.worker.lock().unwrap().is_none());
        disabled.shutdown();

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workspaces.json");
        std::fs::write(&path, b"[]").unwrap();
        let owner = Arc::new(Owner::default());
        owner.start(true, path.clone(), admission.clone());
        owner.worker.lock().unwrap().take().unwrap().join().unwrap();
        assert!(owner.plan().is_some());
        owner.start(true, path, admission);
        assert!(owner.worker.lock().unwrap().is_none());
        owner.retire();
        assert!(owner.plan().is_none());
        owner.shutdown();
    }
}
