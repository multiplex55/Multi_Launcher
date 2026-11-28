//! One launcher-owned, off-UI Clipboard Modify catalog initialization.
use super::*;
use multi_launcher::clipboard_modify::runtime::ClipboardModifyRuntime;

pub(super) fn is_clipboard_modify_search(query: &str) -> bool {
    let (effective, _) = multi_launcher::common::query::split_action_filters(query);
    effective.split_whitespace().next().is_some_and(|head| head.eq_ignore_ascii_case("cm"))
}

pub(super) fn catalog_is_current(runtime: &ClipboardModifyRuntime, catalog: &std::sync::Arc<multi_launcher::clipboard_modify::model::ClipboardModifierCatalog>) -> bool {
    std::sync::Arc::ptr_eq(&runtime.catalog_snapshot(), catalog)
}

fn check_configuration_file(path: &std::path::Path) -> anyhow::Result<()> {
    #[cfg(windows)]
    { multi_launcher::actions::exec::checked_local_file(path) }
    #[cfg(not(windows))]
    {
        anyhow::ensure!(path.is_absolute(), "Configuration path is not absolute");
        for part in path.ancestors() {
            let metadata = std::fs::symlink_metadata(part)?;
            anyhow::ensure!(!metadata.file_type().is_symlink(), "Configuration path contains a link");
            if part == path { anyhow::ensure!(metadata.is_file(), "Configuration is not a file"); }
        }
        Ok(())
    }
}

pub(super) fn open_configuration_with(
    path: &std::path::Path,
    current: &dyn Fn() -> bool,
    launch: impl FnOnce(&std::path::Path, &dyn Fn() -> bool) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let absolute = std::path::absolute(path)?;
    anyhow::ensure!(current(), "Configuration open was superseded");
    check_configuration_file(&absolute)?;
    anyhow::ensure!(current(), "Configuration open was superseded");
    launch(&absolute, &|| check_configuration_file(&absolute).is_ok() && current())
}

#[derive(Default)]
pub(super) struct Owner {
    pub pending: bool,
    pub refresh_pending: bool,
    pub runtime: Option<ClipboardModifyRuntime>,
    pub diagnostic: Option<String>,
    pub projection: Option<(String, String, std::sync::Arc<multi_launcher::clipboard_modify::model::ClipboardModifierCatalog>)>,
    pub published: Option<(String, String, u64, std::sync::Arc<multi_launcher::clipboard_modify::model::ClipboardModifierCatalog>)>,
}

#[cfg(test)]
mod tests {
    #[test]
    fn refresh_matches_the_effective_clipboard_modify_query_head() {
        assert!(super::is_clipboard_modify_search("-id:unrelated cm uppercase"));
        assert!(super::is_clipboard_modify_search(" CM template"));
        assert!(!super::is_clipboard_modify_search("cmother uppercase"));
        assert!(!super::is_clipboard_modify_search("-id:unrelated other cm"));
    }

    #[test]
    fn open_configuration_passes_exact_path_and_fences_late_revoke() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clipboard_modifiers.json");
        std::fs::write(&path, "{}").unwrap();
        let live = AtomicBool::new(true);
        let opened = AtomicBool::new(false);
        super::open_configuration_with(&path, &|| live.load(Ordering::Acquire), |actual, current| {
            assert_eq!(actual, std::path::absolute(&path).unwrap());
            assert!(current()); opened.store(true, Ordering::Release); Ok(())
        }).unwrap();
        assert!(opened.load(Ordering::Acquire));
        opened.store(false, Ordering::Release);
        super::open_configuration_with(&path, &|| live.load(Ordering::Acquire), |_, current| {
            live.store(false, Ordering::Release);
            if current() { opened.store(true, Ordering::Release); }
            Ok(())
        }).unwrap();
        assert!(!opened.load(Ordering::Acquire));
        assert!(super::open_configuration_with(&dir.path().join("missing"), &|| true, |_, _| panic!("missing path launched")).is_err());
    }

    #[test]
    fn reload_adoption_requires_the_exact_published_catalog() {
        use multi_launcher::clipboard_modify::{defaults::default_catalog, runtime::ClipboardModifyRuntime};
        use std::sync::{Arc, RwLock};
        let directory = tempfile::tempdir().unwrap();
        let shared = Arc::new(RwLock::new(Arc::new(default_catalog())));
        let (runtime, _) = ClipboardModifyRuntime::new(&directory.path().join("settings.json"), shared.clone());
        let old = runtime.catalog_snapshot();
        assert!(super::catalog_is_current(&runtime, &old));
        *shared.write().unwrap() = Arc::new(default_catalog());
        assert!(!super::catalog_is_current(&runtime, &old));
        assert!(super::catalog_is_current(&runtime, &runtime.catalog_snapshot()));
    }

    #[test]
    fn owned_startup_installs_one_shared_catalog_for_the_runtime() {
        use multi_launcher::clipboard_modify::{
            defaults::default_catalog, runtime::ClipboardModifyRuntime,
        };
        use std::sync::{Arc, RwLock};
        let directory = tempfile::tempdir().unwrap();
        let settings = directory.path().join("settings.json");
        let shared = Arc::new(RwLock::new(Arc::new(default_catalog())));
        let (runtime, loaded) = ClipboardModifyRuntime::new(&settings, shared.clone());
        assert_eq!(
            loaded.path,
            directory.path().join("clipboard_modifiers.json")
        );
        assert!(loaded.path.exists());
        assert!(Arc::ptr_eq(&shared, &runtime.store.catalog));
        assert!(Arc::ptr_eq(
            &runtime.catalog_snapshot(),
            &shared.read().unwrap()
        ));
    }
}

impl RootWindow {
    pub(super) fn start_clipboard_modify_catalog(&mut self, cx: &mut Context<Self>) {
        if self.clipboard_modify_startup.pending || self.clipboard_modify_startup.runtime.is_some()
        {
            return;
        }
        if self.root_window.is_none() {
            return;
        }
        let Some(permit) = self.restart_admission.admit() else {
            return;
        };
        self.clipboard_modify_startup.pending = true;
        let settings = self.settings_path.clone();
        let catalog = self.search_catalog.providers.clipboard_modify_catalog();
        let task = cx.background_executor().spawn(async move {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                ClipboardModifyRuntime::new(std::path::Path::new(&settings), catalog)
            }))
        });
        cx.spawn(async move |weak, cx| {
            let _permit = permit;
            let result = task.await;
            let mut ready = None;
            let mut failed = None;
            let _ = weak.update(cx, |root, cx| {
                if !root.clipboard_modify_startup.pending { return; }
                match result {
                    Ok((runtime, loaded)) => {
                        let diagnostic = loaded.state.startup_diagnostic().map(|_| "Clipboard Modify settings could not be loaded; defaults are in use.".to_owned());
                        root.clipboard_modify_startup.runtime = Some(runtime.clone());
                        root.search_catalog.providers.clipboard_modify_ready();
                        root.clipboard_modify_startup.diagnostic = diagnostic.clone();
                        ready = root.clipboard_modify_view.clone().map(|view| (view, runtime, diagnostic));
                    }
                    Err(_) => { root.clipboard_modify_startup.diagnostic = Some("Clipboard Modify could not be initialized.".into()); failed = root.clipboard_modify_view.clone(); },
                }
                cx.notify();
            });
            if let Some((view, runtime, diagnostic)) = ready {
                let current_host = cx.update(|app| view.read(app).host);
                if update_kit_window_content(current_host, &view, cx, |view, window, cx| view.install_startup(runtime, diagnostic, window, cx)).is_err() {
                    let _ = view.update(cx, |view, cx| view.startup_delivery_failed(cx));
                }
            }
            if let Some(view) = failed { let _ = view.update(cx, |view, cx| view.startup_failed(cx)); }
            let _ = weak.update(cx, |root, cx| {
                if root.clipboard_modify_startup.pending {
                    root.clipboard_modify_startup.pending = false;
                    if root.restart_status.as_deref() == Some(super::clipboard_modify_view::BLOCK_WAIT) { root.restart_status = None; }
                    let query = root.query.read(cx).value().to_string();
                    if is_clipboard_modify_search(&query) {
                        root.refresh_launcher_results_with_viewport(query, false, cx);
                    }
                    cx.notify();
                }
            });
        }).detach();
        cx.notify();
    }
}
