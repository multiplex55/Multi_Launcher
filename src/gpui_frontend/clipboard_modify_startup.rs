//! One launcher-owned, off-UI Clipboard Modify catalog initialization.
use super::*;
use multi_launcher::clipboard_modify::runtime::ClipboardModifyRuntime;

pub(super) fn is_clipboard_modify_search(query: &str) -> bool {
    let (effective, _) = multi_launcher::common::query::split_action_filters(query);
    effective.split_whitespace().next().is_some_and(|head| head.eq_ignore_ascii_case("cm"))
}

#[derive(Default)]
pub(super) struct Owner {
    pub pending: bool,
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
