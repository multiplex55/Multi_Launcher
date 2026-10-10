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
                WatchEvent::RadialResolveDeferred(envelope) => {
                    self.resolve_deferred_radial(envelope)
                }
                WatchEvent::RadialDeferredSearchReady { envelope, result } => {
                    self.complete_deferred_radial_search(envelope, result)
                }
                WatchEvent::RadialAuthoringSearchReady { request, result } => {
                    self.complete_authoring_provider_search(request, result)
                }
                WatchEvent::RadialAuthoringSearchFailed { request, reason } => {
                    self.fail_authoring_provider_search_from_provider(request, reason)
                }
                WatchEvent::AuthoringProviderCapacityAvailable => {
                    if !self.radial_provider_search_capacity.is_occupied() {
                        self.retire_capacity_deferred_authoring_catalog();
                        self.resume_capacity_deferred_search();
                    }
                    self.start_next_authoring_provider_search();
                }
                WatchEvent::RadialInvalidate => {
                    if let Ok(mut editor) = self.radial_editor.lock() {
                        editor.invalidate_appearance_resources();
                    }
                    self.radial_expected_diagnostics.clear();
                    self.invalidate_radial_leases();
                }
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
                WatchEvent::RadialDiagnostic(diagnostic) => {
                    if diagnostic.is_expected_layout() {
                        if !self
                            .radial_expected_diagnostics
                            .iter()
                            .any(|retained| retained.fingerprint == diagnostic.fingerprint)
                        {
                            if self.radial_expected_diagnostics.len()
                                == crate::radial::diagnostics::MAX_EXPECTED_LAYOUT_DIAGNOSTICS
                            {
                                self.radial_expected_diagnostics.pop_front();
                            }
                            self.radial_expected_diagnostics.push_back(diagnostic);
                        }
                        self.egui_ctx.request_repaint();
                    } else if diagnostic.severity
                        != crate::radial::diagnostics::RadialDiagnosticSeverity::Info
                    {
                        self.report_error_message("radial.resource", diagnostic.message);
                    } else {
                        tracing::info!(message = %diagnostic.message, "radial diagnostic");
                    }
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
                    let mut reload_timer = crate::performance::MetricTimer::start(
                        crate::performance::Metric::ActionsReload,
                    );
                    reload_timer.set_work_units(0);
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
                    reload_timer.set_work_units(custom.len() as u64);
                    let current_custom_len = self.custom_len.min(self.actions.len());
                    if self.actions[..current_custom_len] == custom {
                        self.actions_persistence_diagnostic = None;
                        tracing::debug!("ignored unchanged actions reload notification");
                        continue;
                    }

                    let indexed_len = self.actions.len().saturating_sub(current_custom_len);
                    reload_timer.set_work_units(custom.len().saturating_add(indexed_len) as u64);
                    self.publish_custom_actions_with_indexed_tail(custom);
                    self.actions_persistence_diagnostic = None;
                    crate::actions::bump_actions_version();
                    tracing::info!("actions reloaded");
                }
                WatchEvent::IndexReady => self.process_index_ready(),
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
                        self.invalidate_root_list_display_geometry();
                        self.request_background_query_refresh();
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
                        self.request_background_query_refresh();
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
                WatchEvent::ScreenDrawRecover(intent) => {
                    self.recover_screen_draw(intent);
                }
                WatchEvent::ScreenDrawEmergency(intent) => {
                    self.recover_screen_draw(intent);
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
                            self.apply_command_outcome_with_root_policy(
                                completion.completion_outcome,
                                &completion.invocation,
                                Some(&completion.history_query),
                                completion.root_policy,
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

    fn action_reload_payload(
        seed: u64,
        count: usize,
        variant: &str,
    ) -> (
        crate::performance::workloads::Fixture<crate::actions::Action>,
        Vec<u8>,
    ) {
        let mut fixture = crate::performance::workloads::action_fixture(seed, count);
        fixture.values[0].desc.push_str(variant);
        fixture.summary.estimated_bytes += variant.len() as u64;
        let mut signature = crate::performance::workloads::StableSignature::new(
            seed,
            "actions-reload-payload",
            count,
        );
        signature.number(fixture.summary.signature);
        signature.bytes(variant.as_bytes());
        fixture.summary.signature = signature.finish();
        let serialized = serde_json::to_vec(&fixture.values)
            .expect("serialize synthetic action reload payload outside timed work");
        (fixture, serialized)
    }

    fn indexed_actions(root: &Path, max_items: usize) -> Vec<crate::actions::Action> {
        let roots = vec![root.to_string_lossy().into_owned()];
        let mut actions = Vec::new();
        for batch in crate::indexer::index_paths_batched(
            &roots,
            crate::indexer::IndexOptions {
                batch_size: 128,
                max_items,
            },
        ) {
            actions.extend(batch.expect("synthetic indexed tree traversal succeeds"));
        }
        actions
    }

    fn action_reload_output_signatures(
        root: &Path,
        actions: &[crate::actions::Action],
        custom_len: usize,
    ) -> (u64, u64) {
        let tail = &actions[custom_len.min(actions.len())..];
        let mut prefix_membership = crate::performance::workloads::StableSignature::new(
            0,
            "actions-reload-custom-prefix",
            custom_len,
        );
        let mut prefix_order = crate::performance::workloads::StableSignature::new(
            0,
            "actions-reload-custom-order",
            custom_len,
        );
        for action in actions.iter().take(custom_len) {
            prefix_membership.bytes(action.action.as_bytes());
            prefix_membership.bytes(action.args.as_deref().unwrap_or_default().as_bytes());
            prefix_membership.bytes(action.label.as_bytes());
            prefix_membership.bytes(action.desc.as_bytes());
            prefix_order.bytes(action.action.as_bytes());
            prefix_order.bytes(action.args.as_deref().unwrap_or_default().as_bytes());
            prefix_order.bytes(action.label.as_bytes());
            prefix_order.bytes(action.desc.as_bytes());
        }
        let mut membership = crate::performance::workloads::StableSignature::new(
            0,
            "actions-reload-output-membership",
            custom_len + tail.len(),
        );
        membership.number(prefix_membership.finish());
        membership.number(crate::performance::workloads::index_actions_signature(
            root, tail,
        ));
        let mut order = crate::performance::workloads::StableSignature::new(
            0,
            "actions-reload-output-order",
            custom_len + tail.len(),
        );
        order.number(prefix_order.finish());
        order.number(crate::performance::workloads::index_actions_order_signature(root, tail));
        (membership.finish(), order.finish())
    }

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

    #[test]
    #[ignore = "opt-in Track A workload benchmark; set MULTI_LAUNCHER_PERF=1 before the process"]
    fn track_a_benchmark_actions_reload_event_owner() {
        use crate::performance::{Metric, workloads};

        let workspace = workloads::IsolatedWorkspace::new();
        for indexed_count in workloads::selected_sizes(&[16, 1_000, 10_000]) {
            let custom_count = if indexed_count == 16 {
                100
            } else {
                indexed_count
            };
            let root = workspace
                .root()
                .join(format!("actions-reload-{indexed_count}"));
            let index_root = root.join("indexed");
            let index_fixture =
                workloads::create_index_tree(&index_root, 0x4143_5449_4f4e_535f, indexed_count);
            let (variant_a, bytes_a) =
                action_reload_payload(0x4143_5449_4f4e_5341, custom_count, " synthetic variant A");
            let (variant_b, bytes_b) =
                action_reload_payload(0x4143_5449_4f4e_5342, custom_count, " synthetic variant B");
            let (initial_variant, initial_bytes) = action_reload_payload(
                0x4143_5449_4f4e_5343,
                custom_count,
                " synthetic initial variant",
            );
            let actions_path = root.join("actions.json");
            let settings_path = root.join("settings.json");
            let index_root_string = index_root.to_string_lossy().into_owned();
            let index_paths = vec![index_root_string];
            let initial_tail = indexed_actions(&index_root, indexed_count);
            assert_eq!(initial_tail.len(), indexed_count);
            std::fs::write(&actions_path, &initial_bytes)
                .expect("write prepared initial action payload");

            let mut initial_actions = initial_variant.values.clone();
            initial_actions.extend(initial_tail.iter().cloned());
            let context = egui::Context::default();
            let mut settings = Settings::default();
            settings.enable_toasts = false;
            settings.show_inline_errors = false;
            settings.show_error_toasts = false;
            settings.dashboard.enabled = false;
            settings.hotkey = None;
            settings.quit_hotkey = None;
            settings.help_hotkey = None;
            settings.enabled_plugins = Some(std::collections::HashSet::new());
            settings.max_indexed_items = Some(indexed_count);
            let mut app = LauncherApp::new(
                &context,
                Arc::new(initial_actions),
                custom_count,
                PluginManager::new_inert_for_test(),
                actions_path.to_string_lossy().into_owned(),
                settings_path.to_string_lossy().into_owned(),
                settings,
                None,
                Some(index_paths),
                Some(std::collections::HashSet::new()),
                None,
                Arc::new(AtomicBool::new(false)),
                Arc::new(AtomicBool::new(false)),
                Arc::new(AtomicBool::new(false)),
            );
            // File events are enqueued explicitly below; no watcher may observe
            // setup writes or outlive the isolated workspace.
            app.watchers.clear();
            app.process_watch_events();
            assert!(app.rx.try_recv().is_err(), "initial events were drained");
            assert_eq!(app.custom_len, custom_count);
            assert_eq!(app.actions.len(), custom_count + indexed_count);
            let initial_tail_signature =
                workloads::index_actions_signature(&index_root, &app.actions[custom_count..]);
            assert_eq!(
                initial_tail_signature,
                workloads::index_actions_signature(&index_root, &initial_tail)
            );

            let summary = workloads::FixtureSummary {
                count: custom_count + indexed_count,
                estimated_bytes: variant_a
                    .summary
                    .estimated_bytes
                    .saturating_add(index_fixture.estimated_bytes),
                signature: {
                    let mut signature = workloads::StableSignature::new(
                        0x4143_5449_4f4e_535f,
                        "actions-reload-workload",
                        custom_count + indexed_count,
                    );
                    signature.number(variant_a.summary.signature);
                    signature.number(variant_b.summary.signature);
                    signature.number(initial_variant.summary.signature);
                    signature.number(index_fixture.signature);
                    signature.finish()
                },
                output_signature: None,
                output_order_signature: None,
            };

            let changed_payloads = [bytes_a.clone(), bytes_b.clone()];
            let (changed_timing, ()) = workloads::measure_with_state(
                &mut app,
                workloads::UI_WARMUPS,
                |app, warmup, iteration| {
                    let payload_index = if warmup {
                        iteration % changed_payloads.len()
                    } else {
                        (iteration + 1) % changed_payloads.len()
                    };
                    std::fs::write(&actions_path, &changed_payloads[payload_index])
                        .expect("write prepared changed payload outside timer");
                    app.event_tx
                        .send(WatchEvent::Actions)
                        .expect("enqueue one synthetic actions event");
                },
                |app| {
                    app.process_watch_events();
                },
            );
            let changed_metrics =
                workloads::metrics_for(&[Metric::ActionsReload, Metric::IndexScan]);
            assert_eq!(changed_metrics.len(), 2);
            let reload = &changed_metrics[0];
            let scan = &changed_metrics[1];
            assert_eq!(reload.metric, Metric::ActionsReload);
            assert_eq!(reload.calls, workloads::SAMPLE_COUNT as u64);
            assert_eq!(
                reload.work_units,
                (custom_count + indexed_count) as u64 * workloads::SAMPLE_COUNT as u64
            );
            assert_eq!(scan.metric, Metric::IndexScan);
            assert_eq!(scan.calls, 0);
            assert_eq!(scan.work_units, 0);
            assert_eq!(scan.completed, 0);
            assert_eq!(scan.errors, 0);
            assert_eq!(scan.abandoned, 0);
            assert_eq!(app.custom_len, custom_count);
            assert_eq!(&app.actions[..custom_count], variant_a.values.as_slice());
            assert_eq!(app.actions.len(), custom_count + indexed_count);
            let changed_output =
                action_reload_output_signatures(&index_root, app.actions.as_slice(), custom_count);
            assert_eq!(
                app.actions[custom_count..].len(),
                initial_tail.len(),
                "changed actions are published with the actual indexed tail"
            );
            assert_eq!(
                &app.actions[custom_count..],
                initial_tail.as_slice(),
                "changed custom actions retain the exact indexed tail"
            );
            assert_eq!(
                changed_output.0,
                action_reload_output_signatures(
                    &index_root,
                    &variant_a
                        .values
                        .iter()
                        .cloned()
                        .chain(initial_tail.iter().cloned())
                        .collect::<Vec<_>>(),
                    custom_count,
                )
                .0,
                "custom prefix and indexed tail membership remain stable"
            );
            workloads::emit_summary(
                &format!("actions-reload-{custom_count}-indexed-{indexed_count}-changed"),
                "synthetic WatchEvent::Actions process_watch_events event-drain; timed phase includes typed file read, retained indexed-tail publication, cache publication and synchronous query refresh",
                summary.with_output_signatures(Some(changed_output.0), Some(changed_output.1)),
                changed_timing,
                &changed_metrics,
            );

            let retained_actions = Arc::clone(&app.actions);
            let retained_version = crate::actions::actions_version();
            let unchanged_payload = bytes_a.clone();
            let (unchanged_timing, ()) = workloads::measure_with_state(
                &mut app,
                workloads::UI_WARMUPS,
                |app, _, _| {
                    std::fs::write(&actions_path, &unchanged_payload)
                        .expect("write prepared unchanged payload outside timer");
                    app.event_tx
                        .send(WatchEvent::Actions)
                        .expect("enqueue one unchanged synthetic actions event");
                },
                |app| {
                    app.process_watch_events();
                },
            );
            let unchanged_metrics =
                workloads::metrics_for(&[Metric::ActionsReload, Metric::IndexScan]);
            assert_eq!(unchanged_metrics.len(), 2);
            assert_eq!(unchanged_metrics[0].metric, Metric::ActionsReload);
            assert_eq!(unchanged_metrics[0].calls, workloads::SAMPLE_COUNT as u64);
            assert_eq!(
                unchanged_metrics[0].work_units,
                custom_count as u64 * workloads::SAMPLE_COUNT as u64
            );
            assert_eq!(unchanged_metrics[1].metric, Metric::IndexScan);
            assert_eq!(unchanged_metrics[1].calls, 0);
            assert_eq!(unchanged_metrics[1].work_units, 0);
            assert_eq!(unchanged_metrics[1].completed, 0);
            assert_eq!(unchanged_metrics[1].errors, 0);
            assert_eq!(unchanged_metrics[1].abandoned, 0);
            assert!(Arc::ptr_eq(&app.actions, &retained_actions));
            assert_eq!(crate::actions::actions_version(), retained_version);
            let unchanged_output =
                action_reload_output_signatures(&index_root, app.actions.as_slice(), custom_count);
            workloads::emit_summary(
                &format!("actions-reload-{custom_count}-indexed-{indexed_count}-unchanged"),
                "synthetic unchanged WatchEvent::Actions process_watch_events event-drain; typed file read timed; no indexed traversal or publication",
                summary.with_output_signatures(Some(unchanged_output.0), Some(unchanged_output.1)),
                unchanged_timing,
                &unchanged_metrics,
            );
            drop(app);
        }
        drop(workspace);
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
        app.pending_query = Some("pending root".into());
        app.results = vec![Action {
            label: "Existing result".into(),
            desc: "Existing result".into(),
            action: "help:show".into(),
            args: None,
        }];
        app.selected = Some(0);
        app.last_search_query = "old search".into();
        app.last_results_valid = true;
        app.restore_flag.store(false, Ordering::SeqCst);
        app.focus_query = false;
        app.move_cursor_end = false;
        app.visible_flag.store(true, Ordering::SeqCst);
        app.event_tx
            .send(WatchEvent::VirtualDesktop(VirtualDesktopGuiCompletion {
                invocation,
                completion_outcome: crate::commands::CommandOutcome {
                    query: crate::commands::QueryPolicy::Set("changed".into()),
                    pending_query: crate::commands::PendingQueryPolicy::Set("changed".into()),
                    search: true,
                    invalidate_results: true,
                    results: crate::commands::ResultsPolicy::Replace(Vec::new()),
                    visibility: crate::commands::VisibilityPolicy::Hide,
                    restore: true,
                    focus: true,
                    move_cursor_end: true,
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
        assert_eq!(app.pending_query.as_deref(), Some("pending root"));
        assert_eq!(app.results.len(), 1);
        assert_eq!(app.selected, Some(0));
        assert_eq!(app.last_search_query, "old search");
        assert!(app.last_results_valid);
        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert!(!app.restore_flag.load(Ordering::SeqCst));
        assert!(!app.focus_query);
        assert!(!app.move_cursor_end);
        assert_eq!(app.test_recorded_history_queries, ["captured radial query"]);
        assert_eq!(app.usage.get("vd:create"), Some(&1));
    }

    #[test]
    fn delayed_preserved_completion_cannot_overwrite_a_newer_launcher_visibility_request() {
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
            source: ActivationSource::RadialRelease,
        };
        app.virtual_desktop_interaction_token = 3;
        app.query = "newer hotkey query".into();
        app.visible_flag.store(false, Ordering::SeqCst);
        app.restore_flag.store(false, Ordering::SeqCst);

        // Model a newer hotkey decision committed while the virtual desktop
        // operation was still running.
        app.request_launcher_state(Some(true), Some(true));
        let newer_revision = app.visibility_revision.current();
        app.event_tx
            .send(WatchEvent::VirtualDesktop(VirtualDesktopGuiCompletion {
                invocation,
                completion_outcome: crate::commands::CommandOutcome {
                    query: crate::commands::QueryPolicy::Set("stale completion".into()),
                    visibility: crate::commands::VisibilityPolicy::Hide,
                    restore: false,
                    history: crate::commands::HistoryPolicy::Record,
                    ..crate::commands::CommandOutcome::default()
                },
                history_query: "captured radial query".into(),
                interaction_token: 3,
                expected_query: "old query".into(),
                expected_visible: true,
                root_policy: crate::universal_actions::RootLauncherPolicy::PreserveOrdinaryState,
                result: Ok(()),
            }))
            .unwrap();

        app.process_watch_events();

        assert_eq!(app.visibility_revision.current(), newer_revision);
        assert_eq!(app.query, "newer hotkey query");
        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert!(app.restore_flag.load(Ordering::SeqCst));
        assert_eq!(app.usage.get("vd:create"), Some(&1));
        assert_eq!(app.test_recorded_history_queries, ["captured radial query"]);
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
    fn tooltip_view_limit_routes_as_actionable_radial_diagnostic() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.show_inline_errors = true;
        let diagnostic = crate::radial::diagnostics::RadialDiagnostic::from_font(
            &crate::radial::model::MenuId::new("menu"),
            &crate::radial::model::CellId::new("cell"),
            "a long tooltip",
            (13_000_u32, 240_000_u32),
            &crate::radial::font_cache::FontDiagnostic::TooltipViewLimited,
        );
        assert!(!diagnostic.is_expected_layout());
        app.event_tx
            .send(WatchEvent::RadialDiagnostic(diagnostic.clone()))
            .unwrap();
        app.process_watch_events();
        assert_eq!(app.error.as_deref(), Some(diagnostic.message.as_str()));
        assert!(app.radial_expected_diagnostics.is_empty());
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
    fn placement_designer_action_opens_editor_without_showing_launcher() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.visible_flag.store(false, Ordering::SeqCst);
        app.restore_flag.store(false, Ordering::SeqCst);
        let notice = placement_notice();
        app.radial_placement_viewport.request(notice.clone());

        assert!(app.open_radial_designer_from_placement(&notice, &ctx));

        assert!(!app.visible_flag.load(Ordering::SeqCst));
        assert!(!app.restore_flag.load(Ordering::SeqCst));
        assert!(app.is_panel_open(crate::gui::Panel::RadialEditor));
        assert_eq!(
            app.panel_stack.last(),
            Some(&crate::gui::Panel::RadialEditor)
        );
        assert!(!app.radial_placement_viewport.present_requested());
        assert!(app.radial_placement_viewport.notice().is_none());
        assert!(app.test_activation_trace.is_empty());
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
        let bridge = crate::screen_draw::ScreenDrawRecoveryBridge::default();
        bridge.stage_start();
        let recover = bridge
            .admit(
                crate::screen_draw::ScreenDrawRecoveryKind::LauncherToggle,
                None,
            )
            .unwrap();
        let emergency = bridge
            .admit(crate::screen_draw::ScreenDrawRecoveryKind::Emergency, None)
            .unwrap();
        assert_eq!(
            TestWatchEvent::from(WatchEvent::ScreenDrawRecover(recover)),
            TestWatchEvent::ScreenDrawRecover(recover)
        );
        assert_eq!(
            TestWatchEvent::from(WatchEvent::ScreenDrawEmergency(emergency)),
            TestWatchEvent::ScreenDrawEmergency(emergency)
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
