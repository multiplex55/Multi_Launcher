use super::{push_toast, LauncherApp};
use crate::actions::load_actions;
use crate::indexer;
use egui_toast::{Toast, ToastKind, ToastOptions};
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Instant;

use super::WatchEvent;

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
