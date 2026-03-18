use super::*;

pub(super) fn watch_file(
    path: &Path,
    tx: Sender<WatchEvent>,
    event: WatchEvent,
) -> notify::Result<RecommendedWatcher> {
    let mut watcher = RecommendedWatcher::new(
        move |res: notify::Result<notify::Event>| match res {
            Ok(ev) => {
                if matches!(
                    ev.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                ) {
                    let _ = tx.send(event.clone());
                }
            }
            Err(e) => tracing::error!("watch error: {:?}", e),
        },
        Config::default(),
    )?;
    watcher
        .watch(path, RecursiveMode::NonRecursive)
        .or_else(|_| {
            let parent = path.parent().unwrap_or_else(|| Path::new("."));
            watcher.watch(parent, RecursiveMode::NonRecursive)
        })?;
    Ok(watcher)
}

impl LauncherApp {
    pub fn process_watch_events(&mut self) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                WatchEvent::Actions => {
                    if let Ok(mut acts) = load_actions(&self.actions_path) {
                        let custom_len = acts.len();
                        self.custom_len = custom_len;
                        if let Some(paths) = &self.index_paths {
                            let options =
                                indexer::IndexOptions::with_max_items(self.max_indexed_items);
                            for batch in indexer::index_paths_batched(paths, options) {
                                match batch {
                                    Ok(idx) => {
                                        acts.extend(idx);
                                        self.actions = Arc::new(acts.clone());
                                        self.update_action_cache();
                                        self.search();
                                    }
                                    Err(e) => {
                                        tracing::error!(error = %e, "failed to index paths");
                                        self.report_error_message(
                                            "launcher",
                                            format!("Failed to index paths: {e}"),
                                        );
                                        break;
                                    }
                                }
                            }
                        }
                        self.actions = Arc::new(acts);
                        self.update_action_cache();
                        self.search();
                        crate::actions::bump_actions_version();
                        tracing::info!("actions reloaded");
                    }
                }
                WatchEvent::Folders => {
                    let (aliases, aliases_lc) = Self::folder_alias_maps();
                    self.folder_aliases = aliases;
                    self.folder_aliases_lc = aliases_lc;
                    self.search();
                }
                WatchEvent::Bookmarks => {
                    let (aliases, aliases_lc) = Self::bookmark_alias_maps();
                    self.bookmark_aliases = aliases;
                    self.bookmark_aliases_lc = aliases_lc;
                    self.search();
                }
                WatchEvent::Clipboard => {
                    self.dashboard_data_cache.refresh_clipboard();
                }
                WatchEvent::Snippets => {
                    self.dashboard_data_cache.refresh_snippets();
                }
                WatchEvent::Notes => {
                    self.dashboard_data_cache.refresh_notes();
                }
                WatchEvent::Todos => {
                    self.dashboard_data_cache.refresh_todos();
                }
                WatchEvent::Favorites => {
                    self.dashboard_data_cache.refresh_favorites();
                }
                WatchEvent::Gestures => {
                    self.dashboard_data_cache.refresh_gestures();
                }
                WatchEvent::Dashboard(_) => {
                    self.dashboard.reload();
                    for warn in &self.dashboard.warnings {
                        tracing::warn!("dashboard: {}", warn);
                        if self.enable_toasts {
                            push_toast(
                                &mut self.toasts,
                                Toast {
                                    text: warn.clone().into(),
                                    kind: ToastKind::Warning,
                                    options: ToastOptions::default()
                                        .duration_in_seconds(self.toast_duration as f64),
                                },
                            );
                        }
                    }
                }
                WatchEvent::Recycle(res) => match res {
                    Ok(()) => {
                        if self.enable_toasts {
                            push_toast(
                                &mut self.toasts,
                                Toast {
                                    text: "Emptied Recycle Bin".into(),
                                    kind: ToastKind::Success,
                                    options: ToastOptions::default()
                                        .duration_in_seconds(self.toast_duration as f64),
                                },
                            );
                        }
                    }
                    Err(e) => {
                        let msg = format!("Failed to empty recycle bin: {e}");
                        self.report_error_message("recycle.empty", msg);
                    }
                },
                WatchEvent::ExecuteAction(action) => {
                    self.activate_action(action, None, ActivationSource::Gesture);
                }
            }
        }
        self.maybe_rebuild_completion_index(Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{plugin::PluginManager, settings::Settings};
    use eframe::egui;
    use std::sync::{atomic::AtomicBool, mpsc::channel, Arc};
    use tempfile::tempdir;

    fn new_app(ctx: &egui::Context) -> LauncherApp {
        LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new(),
            "actions.json".into(),
            "settings.json".into(),
            Settings::default(),
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    #[test]
    fn burst_watch_events_coalesce_completion_rebuild_until_debounce_window() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);

        app.actions = Arc::new(vec![Action {
            label: "Before".into(),
            desc: "demo".into(),
            action: "before:app".into(),
            args: None,
        }]);
        app.update_action_cache();
        let first_due = app.completion_rebuild_after.expect("first due");

        app.actions = Arc::new(vec![Action {
            label: "After".into(),
            desc: "demo".into(),
            action: "after:app".into(),
            args: None,
        }]);
        app.update_action_cache();
        let second_due = app.completion_rebuild_after.expect("second due");

        app.maybe_rebuild_completion_index(first_due);
        assert!(app.completion_index.is_none());
        app.maybe_rebuild_completion_index(second_due + Duration::from_millis(1));
        assert!(app.completion_index.is_some());
    }

    #[test]
    fn folder_and_bookmark_watch_updates_refresh_alias_caches() {
        let dir = tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        std::fs::write(
            crate::plugins::folders::FOLDERS_FILE,
            serde_json::to_string_pretty(&serde_json::json!([
                {"path": "C:/Docs", "alias": "Docs Alias"}
            ]))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            crate::plugins::bookmarks::BOOKMARKS_FILE,
            serde_json::to_string_pretty(&serde_json::json!([
                {"url": "https://example.com", "alias": "Example Alias"}
            ]))
            .unwrap(),
        )
        .unwrap();

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.tx.send(WatchEvent::Folders).unwrap();
        app.tx.send(WatchEvent::Bookmarks).unwrap();
        app.process_watch_events();

        assert_eq!(
            app.folder_aliases.get("C:/Docs"),
            Some(&Some("Docs Alias".into()))
        );
        assert_eq!(
            app.folder_aliases_lc.get("C:/Docs"),
            Some(&Some("docs alias".into()))
        );
        assert_eq!(
            app.bookmark_aliases.get("https://example.com"),
            Some(&Some("Example Alias".into()))
        );
        assert_eq!(
            app.bookmark_aliases_lc.get("https://example.com"),
            Some(&Some("example alias".into()))
        );

        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn test_watch_event_adapters_preserve_expected_public_parity() {
        let action = Action {
            label: "Run".into(),
            desc: "demo".into(),
            action: "demo:run".into(),
            args: None,
        };
        assert_eq!(
            TestWatchEvent::from(WatchEvent::Actions),
            TestWatchEvent::Actions
        );
        assert_eq!(
            TestWatchEvent::from(WatchEvent::Folders),
            TestWatchEvent::Folders
        );
        assert_eq!(
            TestWatchEvent::from(WatchEvent::Bookmarks),
            TestWatchEvent::Bookmarks
        );
        assert_eq!(
            TestWatchEvent::from(WatchEvent::ExecuteAction(action.clone())),
            TestWatchEvent::Actions
        );
        assert_eq!(
            TestWatchEvent::from(WatchEvent::Dashboard(
                crate::dashboard::DashboardEvent::Reloaded
            )),
            TestWatchEvent::Actions
        );

        let (tx, rx) = channel();
        tx.send(WatchEvent::Clipboard).unwrap();
        tx.send(WatchEvent::Folders).unwrap();
        assert_eq!(recv_test_event(&rx), Some(TestWatchEvent::Folders));

        let (tx, rx) = channel();
        tx.send(WatchEvent::ExecuteAction(action)).unwrap();
        tx.send(WatchEvent::Bookmarks).unwrap();
        assert_eq!(recv_test_event(&rx), Some(TestWatchEvent::Bookmarks));
    }
}
