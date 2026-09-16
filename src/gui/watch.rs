use super::*;
use std::path::{Path, PathBuf};

pub(super) fn watch_file(
    path: &Path,
    tx: Sender<WatchEvent>,
    event: WatchEvent,
    repaint: egui::Context,
) -> notify::Result<RecommendedWatcher> {
    watch_file_with_wake(path, tx, event, ViewportWake::root(&repaint))
}

pub(super) fn watch_file_with_wake(
    path: &Path,
    tx: Sender<WatchEvent>,
    event: WatchEvent,
    wake: ViewportWake,
) -> notify::Result<RecommendedWatcher> {
    let target = path.to_path_buf();
    let target_is_directory = path.is_dir();
    let mut watcher = RecommendedWatcher::new(
        move |res: notify::Result<notify::Event>| match res {
            Ok(ev) => {
                if matches!(
                    ev.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                ) && crate::common::json_watch::event_targets_path(
                    &ev,
                    &target,
                    target_is_directory,
                ) {
                    if tx.send(event.clone()).is_ok() {
                        wake.wake();
                    }
                }
            }
            Err(e) => tracing::error!("watch error: {:?}", e),
        },
        Config::default(),
    )?;
    let watch_root = if target_is_directory {
        path
    } else {
        path.parent().unwrap_or_else(|| Path::new("."))
    };
    watcher.watch(watch_root, RecursiveMode::NonRecursive)?;
    Ok(watcher)
}

impl LauncherApp {
    pub fn process_watch_events(&mut self) {
        while let Ok(ev) = self.rx.try_recv() {
            self.event_sink.event_consumed();
            match ev {
                WatchEvent::RadialDispatch(request) => self.execute_radial_dispatch(request),
                WatchEvent::RadialPrepare(envelope) => self.prepare_radial(envelope),
                WatchEvent::RadialInvalidate => self.invalidate_radial_leases(),
                WatchEvent::RadialConfigDiagnostic(diagnostic) => {
                    if let Some(diagnostic) = diagnostic {
                        self.report_error_message(
                            "radial.reload",
                            format!("Radial menu configuration was not reloaded: {diagnostic}"),
                        );
                    }
                }
                WatchEvent::RadialRuntimeDiagnostic(diagnostic) => {
                    self.report_error_message("radial.runtime", diagnostic);
                }
                WatchEvent::RadialSubmenuPlacementFailure(notice) => {
                    self.radial_placement_viewport.request(notice);
                    self.egui_ctx.request_repaint();
                    self.egui_ctx
                        .request_repaint_of(super::radial_placement_viewport_id());
                }
                WatchEvent::RadialPlacementActionResult {
                    session_id,
                    parent_frame_id,
                    result,
                } => {
                    if self.radial_placement_viewport.apply_action_result(
                        &session_id,
                        parent_frame_id,
                        result,
                    ) {
                        self.egui_ctx.request_repaint();
                        self.egui_ctx
                            .request_repaint_of(super::radial_placement_viewport_id());
                    }
                }
                WatchEvent::RadialMigrationNotice(notice) => {
                    self.add_toast(Toast {
                        text: notice.into(),
                        kind: ToastKind::Info,
                        options: ToastOptions::default()
                            .duration_in_seconds(self.toast_duration as f64),
                    });
                }
                WatchEvent::RadialMigrationState {
                    receipt,
                    default_submenu_presentation,
                } => {
                    self.radial_migration_receipt = receipt;
                    self.radial_feature_settings.default_submenu_presentation =
                        default_submenu_presentation;
                    self.settings_editor.radial_default_submenu_presentation =
                        default_submenu_presentation;
                }
                WatchEvent::Actions => {
                    let _transaction = crate::actions::transaction_guard();
                    let custom = match load_actions_typed(&self.actions_path) {
                        Ok(crate::common::persistence::LoadState::Missing) => {
                            let error = crate::common::persistence::PersistenceError::Read {
                                path: PathBuf::from(&self.actions_path),
                                source: std::io::Error::new(
                                    std::io::ErrorKind::NotFound,
                                    "actions file was removed; retaining last-good state",
                                ),
                            };
                            self.report_error_message(
                                "actions.reload",
                                format!("Failed to reload actions: {error}"),
                            );
                            self.actions_persistence_diagnostic = Some(error);
                            continue;
                        }
                        Ok(crate::common::persistence::LoadState::Empty) => Vec::new(),
                        Ok(crate::common::persistence::LoadState::Loaded(actions)) => actions,
                        Err(error) => {
                            self.report_error_message(
                                "actions.reload",
                                format!("Failed to reload actions: {error}"),
                            );
                            self.actions_persistence_diagnostic = Some(error);
                            continue;
                        }
                    };
                    let current_custom_len = self.custom_len.min(self.actions.len());
                    if self.actions[..current_custom_len] == custom {
                        self.actions_persistence_diagnostic = None;
                        tracing::debug!("ignored unchanged actions reload notification");
                        continue;
                    }

                    let mut indexed = Vec::new();
                    if let Some(paths) = &self.index_paths {
                        let options = indexer::IndexOptions::with_max_items(self.max_indexed_items);
                        for batch in indexer::index_paths_batched(paths, options) {
                            match batch {
                                Ok(actions) => indexed.extend(actions),
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
                    self.publish_actions(custom, indexed);
                    self.actions_persistence_diagnostic = None;
                    crate::actions::bump_actions_version();
                    tracing::info!("actions reloaded");
                }
                WatchEvent::Folders
                    if !Path::new(crate::plugins::folders::FOLDERS_FILE).exists() =>
                {
                    self.report_error_message(
                        "folders.reload",
                        "Folders file was removed; retaining last-good aliases",
                    );
                }
                WatchEvent::Folders => match Self::try_folder_alias_maps() {
                    Ok((aliases, aliases_lc)) => {
                        self.folder_aliases = aliases;
                        self.folder_aliases_lc = aliases_lc;
                        self.search();
                    }
                    Err(error) => self.report_error_message(
                        "folders.reload",
                        format!("Failed to reload folder aliases: {error}"),
                    ),
                },
                WatchEvent::Bookmarks
                    if !Path::new(crate::plugins::bookmarks::BOOKMARKS_FILE).exists() =>
                {
                    self.report_error_message(
                        "bookmarks.reload",
                        "Bookmarks file was removed; retaining last-good aliases",
                    );
                }
                WatchEvent::Bookmarks => match Self::try_bookmark_alias_maps() {
                    Ok((aliases, aliases_lc)) => {
                        self.bookmark_aliases = aliases;
                        self.bookmark_aliases_lc = aliases_lc;
                        self.search();
                    }
                    Err(error) => self.report_error_message(
                        "bookmarks.reload",
                        format!("Failed to reload bookmark aliases: {error}"),
                    ),
                },
                WatchEvent::Clipboard => {
                    self.dashboard_data_cache
                        .request_refresh(DashboardRefreshRequest::Clipboard);
                }
                WatchEvent::Snippets => {
                    self.dashboard_data_cache
                        .request_refresh(DashboardRefreshRequest::Snippets);
                }
                WatchEvent::Notes => {
                    self.dashboard_data_cache
                        .request_refresh(DashboardRefreshRequest::Notes);
                }
                WatchEvent::Todos => {
                    self.dashboard_data_cache
                        .request_refresh(DashboardRefreshRequest::Todos);
                }
                WatchEvent::Favorites => {
                    self.dashboard_data_cache
                        .request_refresh(DashboardRefreshRequest::Favorites);
                }
                WatchEvent::Gestures => {
                    self.dashboard_data_cache
                        .request_refresh(DashboardRefreshRequest::Gestures);
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
                WatchEvent::ScreenDrawStart => {
                    if let Err(error) = self.start_or_focus_screen_draw() {
                        self.report_error_message("screen_draw.start", error);
                    }
                    self.screen_draw_controller.reconcile_recovery_publication();
                }
                WatchEvent::ScreenDrawRecover => {
                    self.recover_screen_draw(super::ScreenDrawRecoveryRequest::LauncherToggle);
                }
                WatchEvent::ScreenDrawEmergency => {
                    self.recover_screen_draw(super::ScreenDrawRecoveryRequest::Emergency);
                }
                WatchEvent::ClipboardModify(ev) => {
                    self.handle_clipboard_modify_gui_event(ev);
                }
                WatchEvent::VirtualDesktop(mut completion) => {
                    let interaction_is_current = self.virtual_desktop_interaction_token
                        == completion.interaction_token
                        && (completion.root_policy
                            == crate::universal_actions::RootLauncherPolicy::PreserveOrdinaryState
                            || (self.query == completion.expected_query
                                && self.visible_flag.load(Ordering::SeqCst)
                                    == completion.expected_visible));
                    let preserved_root = (completion.root_policy
                        == crate::universal_actions::RootLauncherPolicy::PreserveOrdinaryState)
                        .then(|| super::universal_action_executor::RadialRootState::capture(self));
                    match completion.result {
                        Ok(()) => {
                            if !interaction_is_current {
                                completion.completion_outcome.toasts.clear();
                            }
                            self.apply_command_outcome_with_history_query(
                                completion.completion_outcome,
                                &completion.invocation,
                                Some(&completion.history_query),
                            );
                        }
                        Err(error) if interaction_is_current => {
                            self.report_error_message("virtual_desktop", format!("Failed: {error}"))
                        }
                        Err(error) => tracing::error!(
                            context = "virtual_desktop",
                            error,
                            "suppressed stale virtual desktop completion error"
                        ),
                    }
                    if let Some(root) = preserved_root {
                        root.restore(self);
                    }
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
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::channel,
    };
    use tempfile::tempdir;

    #[test]
    fn watcher_enqueues_then_requests_repaint_for_external_change() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("actions.json");
        std::fs::write(&path, "[]").unwrap();
        let (event_tx, event_rx) = channel();
        let (repaint_tx, repaint_rx) = channel();
        let ctx = egui::Context::default();
        ctx.set_request_repaint_callback(move |_| {
            let _ = repaint_tx.send(());
        });
        let _watcher = watch_file(&path, event_tx, WatchEvent::Actions, ctx).unwrap();

        crate::common::persistence::save_json_atomic(&path, &serde_json::json!([])).unwrap();

        assert!(matches!(
            event_rx.recv_timeout(Duration::from_secs(3)).unwrap(),
            WatchEvent::Actions
        ));
        repaint_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("watch callback should wake idle egui after enqueue");
    }

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

    fn placement_notice() -> RadialPlacementFailureNotice {
        RadialPlacementFailureNotice {
            session_id: crate::radial::model::SessionId::new("designer-session"),
            parent_frame_id: crate::radial::session::FrameId(11),
            parent_menu_id: crate::radial::model::MenuId::new("parent"),
            child_menu_id: crate::radial::model::MenuId::new("child"),
            parent_presentation: crate::radial::model::SubmenuPresentation::SameCenter,
            message: "fixed center does not fit".into(),
        }
    }

    #[test]
    fn repeated_pre_capture_screen_draw_launch_event_is_idempotent_without_toolbar() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.event_tx.send(WatchEvent::ScreenDrawStart).unwrap();
        app.process_watch_events();
        let generation = app.screen_draw_controller.state().generation().unwrap();
        assert!(matches!(
            app.screen_draw_controller.state(),
            crate::screen_draw::ScreenDrawState::AwaitingLauncherParking { .. }
        ));

        ctx.begin_frame(egui::RawInput::default());
        app.event_tx.send(WatchEvent::ScreenDrawStart).unwrap();
        app.process_watch_events();
        app.show_screen_draw_toolbar(&ctx);
        let _ = ctx.end_frame();
        assert_eq!(
            app.screen_draw_controller.state().generation(),
            Some(generation)
        );
        assert!(!app.screen_draw_controller.toolbar_open());

        ctx.begin_frame(egui::RawInput::default());
        app.event_tx.send(WatchEvent::ScreenDrawStart).unwrap();
        app.process_watch_events();
        let _ = ctx.end_frame();
        assert!(!app.screen_draw_toolbar.was_open);
        assert_eq!(app.screen_draw_toolbar.focus_request_count, 0);
    }

    #[test]
    fn virtual_desktop_completion_error_is_surfaced_on_gui_event_reduction() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let invocation = crate::commands::CommandInvocation {
            command: crate::commands::Command::VirtualDesktop(
                crate::commands::VirtualDesktopCommand::Create,
            ),
            original_action: Action {
                label: "Create Virtual Desktop".into(),
                desc: "Virtual Desktop".into(),
                action: "vd:create".into(),
                args: None,
            },
            query_override: None,
            source: ActivationSource::Enter,
        };
        app.event_tx
            .send(WatchEvent::VirtualDesktop(VirtualDesktopGuiCompletion {
                invocation,
                completion_outcome: crate::commands::CommandOutcome::default(),
                history_query: "vd create".into(),
                interaction_token: 0,
                expected_query: String::new(),
                expected_visible: false,
                root_policy: crate::universal_actions::RootLauncherPolicy::Legacy,
                result: Err("injected desktop failure".into()),
            }))
            .unwrap();
        app.process_watch_events();
        assert!(
            app.error
                .as_deref()
                .is_some_and(|error| { error.contains("Failed: injected desktop failure") })
        );
    }

    #[test]
    fn delayed_virtual_desktop_success_preserves_new_interaction_and_records_captured_query() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let action = Action {
            label: "Create Virtual Desktop".into(),
            desc: "Virtual Desktop".into(),
            action: "vd:create".into(),
            args: None,
        };
        let invocation = crate::commands::CommandInvocation {
            command: crate::commands::Command::VirtualDesktop(
                crate::commands::VirtualDesktopCommand::Create,
            ),
            original_action: action.clone(),
            query_override: None,
            source: ActivationSource::Enter,
        };
        app.virtual_desktop_interaction_token = 1;
        app.query = "new interaction".into();
        app.visible_flag.store(true, Ordering::SeqCst);
        app.event_tx
            .send(WatchEvent::VirtualDesktop(VirtualDesktopGuiCompletion {
                invocation,
                completion_outcome: crate::commands::CommandOutcome {
                    history: crate::commands::HistoryPolicy::Record,
                    toasts: vec![crate::commands::ToastPolicy::Launched(action.label.clone())],
                    ..crate::commands::CommandOutcome::default()
                },
                history_query: "vd create".into(),
                interaction_token: 1,
                expected_query: String::new(),
                expected_visible: false,
                root_policy: crate::universal_actions::RootLauncherPolicy::Legacy,
                result: Ok(()),
            }))
            .unwrap();
        app.process_watch_events();
        assert_eq!(app.query, "new interaction");
        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert_eq!(app.usage.get("vd:create"), Some(&1));
        assert_eq!(app.test_recorded_history_queries, ["vd create"]);
        assert!(app.test_toast_messages.is_empty());
    }

    #[test]
    fn radial_async_completion_applies_history_once_without_mutating_root_state() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let action = Action {
            label: "Create Virtual Desktop".into(),
            desc: "Virtual Desktop".into(),
            action: "vd:create".into(),
            args: None,
        };
        let invocation = crate::commands::CommandInvocation {
            command: crate::commands::Command::VirtualDesktop(
                crate::commands::VirtualDesktopCommand::Create,
            ),
            original_action: action,
            query_override: None,
            source: ActivationSource::RadialRelease,
        };
        app.virtual_desktop_interaction_token = 7;
        app.query = "untouched root".into();
        app.last_search_query = "old search".into();
        app.last_results_valid = true;
        app.visible_flag.store(true, Ordering::SeqCst);
        app.event_tx
            .send(WatchEvent::VirtualDesktop(VirtualDesktopGuiCompletion {
                invocation,
                completion_outcome: crate::commands::CommandOutcome {
                    query: crate::commands::QueryPolicy::Set("changed".into()),
                    visibility: crate::commands::VisibilityPolicy::Hide,
                    history: crate::commands::HistoryPolicy::Record,
                    ..crate::commands::CommandOutcome::default()
                },
                history_query: "captured radial query".into(),
                interaction_token: 7,
                expected_query: "transient command state".into(),
                expected_visible: false,
                root_policy: crate::universal_actions::RootLauncherPolicy::PreserveOrdinaryState,
                result: Ok(()),
            }))
            .unwrap();
        app.process_watch_events();
        assert_eq!(app.query, "untouched root");
        assert_eq!(app.last_search_query, "old search");
        assert!(app.last_results_valid);
        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert_eq!(app.test_recorded_history_queries, ["captured radial query"]);
        assert_eq!(app.usage.get("vd:create"), Some(&1));
    }

    #[test]
    fn clipboard_modify_watch_events_keep_their_content_free_type_for_tests() {
        assert_eq!(
            TestWatchEvent::from(WatchEvent::ClipboardModify(
                ClipboardModifyGuiEvent::ImmediateOperationComplete
            )),
            TestWatchEvent::ClipboardModify(ClipboardModifyGuiEvent::ImmediateOperationComplete)
        );
        assert_eq!(
            TestWatchEvent::from(WatchEvent::ClipboardModify(
                ClipboardModifyGuiEvent::ConfigurationReloadFailure("bad".into())
            )),
            TestWatchEvent::ClipboardModify(ClipboardModifyGuiEvent::ConfigurationReloadFailure(
                "bad".into()
            ))
        );
    }

    #[test]
    fn radial_runtime_diagnostic_is_visible_without_mutating_launcher_state() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.query = "keep query".into();
        app.selected = Some(3);
        app.show_inline_errors = true;
        app.event_tx
            .send(WatchEvent::RadialRuntimeDiagnostic(
                "missing managed radial asset".into(),
            ))
            .unwrap();
        app.process_watch_events();
        assert_eq!(app.query, "keep query");
        assert_eq!(app.selected, Some(3));
        assert_eq!(app.error.as_deref(), Some("missing managed radial asset"));
    }

    #[test]
    fn hidden_launcher_radial_notice_requests_one_stable_viewport_and_correlated_actions() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.visible_flag.store(false, Ordering::SeqCst);
        app.restore_flag.store(false, Ordering::SeqCst);
        let notice = RadialPlacementFailureNotice {
            session_id: crate::radial::model::SessionId::new("radial-session"),
            parent_frame_id: crate::radial::session::FrameId(3),
            parent_menu_id: crate::radial::model::MenuId::new("favorites"),
            child_menu_id: crate::radial::model::MenuId::new("applications"),
            parent_presentation: crate::radial::model::SubmenuPresentation::SameCenter,
            message: "fixed center placement does not fit".into(),
        };
        assert!(notice.can_switch_parent_to_cascade());
        let mut already_cascade = notice.clone();
        already_cascade.parent_presentation = crate::radial::model::SubmenuPresentation::Cascade;
        assert!(!already_cascade.can_switch_parent_to_cascade());

        app.event_tx
            .send(WatchEvent::RadialSubmenuPlacementFailure(notice.clone()))
            .unwrap();
        app.process_watch_events();
        assert_eq!(app.radial_placement_viewport.notice(), Some(&notice));
        assert!(app.radial_placement_viewport.present_requested());
        let recovery_viewport = super::super::radial_placement_viewport_id();
        assert!(!app.visible_flag.load(Ordering::SeqCst));
        assert!(!app.restore_flag.load(Ordering::SeqCst));
        assert!(app.radial_placement_viewport.take_focus_request());
        assert!(!app.radial_placement_viewport.take_focus_request());

        app.event_tx
            .send(WatchEvent::RadialSubmenuPlacementFailure(notice.clone()))
            .unwrap();
        app.process_watch_events();
        assert_eq!(
            super::super::radial_placement_viewport_id(),
            recovery_viewport
        );
        assert!(app.radial_placement_viewport.take_focus_request());
        assert!(!app.visible_flag.load(Ordering::SeqCst));
        assert!(!app.restore_flag.load(Ordering::SeqCst));

        app.event_tx
            .send(WatchEvent::RadialPlacementActionResult {
                session_id: crate::radial::model::SessionId::new("stale-session"),
                parent_frame_id: notice.parent_frame_id,
                result: Ok(()),
            })
            .unwrap();
        app.process_watch_events();
        assert_eq!(app.radial_placement_viewport.notice(), Some(&notice));

        app.event_tx
            .send(WatchEvent::RadialPlacementActionResult {
                session_id: notice.session_id.clone(),
                parent_frame_id: notice.parent_frame_id,
                result: Err("revision conflict".into()),
            })
            .unwrap();
        app.process_watch_events();
        assert!(
            app.radial_placement_viewport
                .notice()
                .is_some_and(|active| {
                    active.session_id == notice.session_id
                        && active.parent_frame_id == notice.parent_frame_id
                        && active.message.contains("revision conflict")
                })
        );
        assert!(app.radial_placement_viewport.present_requested());

        app.event_tx
            .send(WatchEvent::RadialPlacementActionResult {
                session_id: notice.session_id.clone(),
                parent_frame_id: notice.parent_frame_id,
                result: Ok(()),
            })
            .unwrap();
        app.process_watch_events();
        assert!(app.radial_placement_viewport.notice().is_none());
        assert!(app.radial_placement_viewport.present_requested());
        app.radial_placement_viewport.mark_viewport_closed();
        assert!(!app.radial_placement_viewport.present_requested());
        assert!(!app.visible_flag.load(Ordering::SeqCst));
        assert!(!app.restore_flag.load(Ordering::SeqCst));
    }

    #[test]
    fn placement_designer_action_uses_launcher_show_command_then_opens_editor() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.visible_flag.store(false, Ordering::SeqCst);
        app.restore_flag.store(false, Ordering::SeqCst);
        let notice = placement_notice();
        app.radial_placement_viewport.request(notice.clone());

        assert!(app.open_radial_designer_from_placement(&notice, &ctx));

        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert!(app.restore_flag.load(Ordering::SeqCst));
        assert!(app.is_panel_open(crate::gui::Panel::RadialEditor));
        assert_eq!(
            app.panel_stack.last(),
            Some(&crate::gui::Panel::RadialEditor)
        );
        assert!(!app.radial_placement_viewport.present_requested());
        assert!(app.radial_placement_viewport.notice().is_none());
        assert_eq!(app.test_activation_trace.len(), 1);
        assert_eq!(app.test_activation_trace[0].0.action, "launcher:show");
        assert_eq!(
            app.test_activation_trace[0].1,
            crate::commands::ActivationSource::Click
        );
    }

    #[test]
    fn placement_dismiss_and_cascade_leave_hidden_launcher_unchanged() {
        let ctx = egui::Context::default();
        let notice = placement_notice();
        let mut dismissed_app = new_app(&ctx);
        dismissed_app.visible_flag.store(false, Ordering::SeqCst);
        dismissed_app.restore_flag.store(false, Ordering::SeqCst);
        dismissed_app
            .radial_placement_viewport
            .request(notice.clone());

        assert!(dismissed_app.dismiss_radial_placement_notice(&notice));
        assert!(!dismissed_app.visible_flag.load(Ordering::SeqCst));
        assert!(!dismissed_app.restore_flag.load(Ordering::SeqCst));
        assert!(!dismissed_app.radial_placement_viewport.present_requested());
        assert!(dismissed_app.test_activation_trace.is_empty());

        let mut cascade_app = new_app(&ctx);
        cascade_app.visible_flag.store(false, Ordering::SeqCst);
        cascade_app.restore_flag.store(false, Ordering::SeqCst);
        cascade_app
            .radial_placement_viewport
            .request(notice.clone());
        let (client, endpoint) =
            crate::radial::control::radial_control_service_with_wake(true, None);

        assert!(
            cascade_app
                .request_radial_placement_cascade(&notice, &client)
                .unwrap()
        );
        assert_eq!(
            endpoint.request_rx.try_recv().unwrap(),
            crate::radial::control::RadialControlRequest::SetActiveParentSubmenuCascade {
                session_id: notice.session_id,
                parent_frame_id: notice.parent_frame_id,
                parent_menu_id: notice.parent_menu_id,
            }
        );
        assert!(!cascade_app.visible_flag.load(Ordering::SeqCst));
        assert!(!cascade_app.restore_flag.load(Ordering::SeqCst));
        assert!(cascade_app.radial_placement_viewport.present_requested());
        assert!(cascade_app.test_activation_trace.is_empty());
    }

    #[test]
    fn stale_placement_designer_action_cannot_show_launcher_or_change_panel() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.visible_flag.store(false, Ordering::SeqCst);
        app.restore_flag.store(false, Ordering::SeqCst);
        let active_notice = placement_notice();
        app.radial_placement_viewport.request(active_notice.clone());
        let mut stale_notice = active_notice;
        stale_notice.parent_frame_id = crate::radial::session::FrameId(12);

        assert!(!app.open_radial_designer_from_placement(&stale_notice, &ctx));

        assert!(!app.visible_flag.load(Ordering::SeqCst));
        assert!(!app.restore_flag.load(Ordering::SeqCst));
        assert!(!app.is_panel_open(crate::gui::Panel::RadialEditor));
        assert!(app.radial_placement_viewport.present_requested());
        assert!(app.test_activation_trace.is_empty());
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
                {"label": "Docs", "path": "C:/Docs", "alias": "Docs Alias"}
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
        assert_eq!(
            app.folder_aliases.get("C:/Docs"),
            Some(&Some("Docs Alias".into()))
        );
        assert_eq!(
            app.bookmark_aliases.get("https://example.com"),
            Some(&Some("Example Alias".into()))
        );

        std::fs::write(
            crate::plugins::folders::FOLDERS_FILE,
            serde_json::to_string_pretty(&serde_json::json!([
                {"label": "Docs", "path": "C:/Docs", "alias": "Updated Docs Alias"}
            ]))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            crate::plugins::bookmarks::BOOKMARKS_FILE,
            serde_json::to_string_pretty(&serde_json::json!([
                {"url": "https://example.com", "alias": "Updated Example Alias"}
            ]))
            .unwrap(),
        )
        .unwrap();

        send_event(WatchEvent::Folders);
        send_event(WatchEvent::Bookmarks);
        app.process_watch_events();

        assert_eq!(
            app.folder_aliases.get("C:/Docs"),
            Some(&Some("Updated Docs Alias".into()))
        );
        assert_eq!(
            app.folder_aliases_lc.get("C:/Docs"),
            Some(&Some("updated docs alias".into()))
        );
        assert_eq!(
            app.bookmark_aliases.get("https://example.com"),
            Some(&Some("Updated Example Alias".into()))
        );
        assert_eq!(
            app.bookmark_aliases_lc.get("https://example.com"),
            Some(&Some("updated example alias".into()))
        );

        std::fs::remove_file(crate::plugins::folders::FOLDERS_FILE).unwrap();
        std::fs::remove_file(crate::plugins::bookmarks::BOOKMARKS_FILE).unwrap();
        send_event(WatchEvent::Folders);
        send_event(WatchEvent::Bookmarks);
        app.process_watch_events();
        assert_eq!(
            app.folder_aliases.get("C:/Docs"),
            Some(&Some("Updated Docs Alias".into()))
        );
        assert_eq!(
            app.bookmark_aliases.get("https://example.com"),
            Some(&Some("Updated Example Alias".into()))
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
        assert_eq!(
            TestWatchEvent::from(WatchEvent::ScreenDrawRecover),
            TestWatchEvent::ScreenDrawRecover
        );
        assert_eq!(
            TestWatchEvent::from(WatchEvent::ScreenDrawEmergency),
            TestWatchEvent::ScreenDrawEmergency
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
