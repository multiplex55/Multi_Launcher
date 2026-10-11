use super::event_channel::EventSender;
use super::*;
use crate::performance::track_c::EventOrigin;
use std::path::{Path, PathBuf};

pub(super) fn watch_file(
    path: &Path,
    tx: EventSender,
    event: WatchEvent,
    repaint: egui::Context,
) -> notify::Result<RecommendedWatcher> {
    watch_file_with_wake(path, tx, event, ViewportWake::root(&repaint))
}

pub(super) fn watch_file_with_wake(
    path: &Path,
    tx: EventSender,
    event: WatchEvent,
    wake: ViewportWake,
) -> notify::Result<RecommendedWatcher> {
    let tx = tx.with_origin(EventOrigin::FileWatcher);
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
        let mut timer = crate::performance::track_c::Timer::start(
            crate::performance::track_c::Phase::EventDrain,
        );
        let mut drained = 0_usize;
        while let Ok(delivery) = self.rx.try_recv_for_dispatch() {
            drained = drained.saturating_add(1);
            self.event_sink.event_consumed();
            match delivery.event {
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
                    #[cfg(test)]
                    let _track_c_actions_event = super::track_c_action_observer::scope(
                        super::track_c_action_observer::Phase::ActionsEvent,
                    );
                    let mut reload_timer = crate::performance::MetricTimer::start(
                        crate::performance::Metric::ActionsReload,
                    );
                    reload_timer.set_work_units(0);
                    let _transaction = crate::actions::transaction_guard();
                    let typed_load = {
                        #[cfg(test)]
                        let _track_c_typed_load = super::track_c_action_observer::scope(
                            super::track_c_action_observer::Phase::TypedLoad,
                        );
                        load_actions_typed(&self.actions_path)
                    };
                    let custom = match typed_load {
                        Ok(crate::common::persistence::LoadState::Missing) => {
                            #[cfg(test)]
                            super::track_c_action_observer::set_value(
                                super::track_c_action_observer::Counter::TypedLoadOutcome,
                                3,
                            );
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
                        Ok(crate::common::persistence::LoadState::Empty) => {
                            #[cfg(test)]
                            super::track_c_action_observer::set_value(
                                super::track_c_action_observer::Counter::TypedLoadOutcome,
                                1,
                            );
                            Vec::new()
                        }
                        Ok(crate::common::persistence::LoadState::Loaded(actions)) => {
                            #[cfg(test)]
                            {
                                super::track_c_action_observer::set_value(
                                    super::track_c_action_observer::Counter::TypedLoadOutcome,
                                    1,
                                );
                                super::track_c_action_observer::add(
                                    super::track_c_action_observer::Counter::TypedActionsLoaded,
                                    actions.len(),
                                );
                            }
                            actions
                        }
                        Err(error) => {
                            #[cfg(test)]
                            super::track_c_action_observer::set_value(
                                super::track_c_action_observer::Counter::TypedLoadOutcome,
                                2,
                            );
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
        timer.set_work_units(drained);
        timer.finish(crate::performance::track_c::Outcome::Completed);
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
    static TRACK_C_EVENT_REGISTRY_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
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

    fn indexed_actions_for_roots(
        roots: &[PathBuf],
        max_items: usize,
    ) -> Vec<crate::actions::Action> {
        let roots = roots
            .iter()
            .map(|root| root.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        crate::indexer::index_paths_batched(
            &roots,
            crate::indexer::IndexOptions {
                batch_size: 512,
                max_items,
            },
        )
        .flat_map(Result::unwrap)
        .collect()
    }

    fn multi_root_output_signatures(
        root_a: &Path,
        root_b: &Path,
        actions: &[crate::actions::Action],
    ) -> (u64, u64) {
        let root_a = std::fs::canonicalize(root_a).expect("canonicalize first index root");
        let root_b = std::fs::canonicalize(root_b).expect("canonicalize second index root");
        let relative_identities = actions
            .iter()
            .map(|action| {
                let path = Path::new(&action.action);
                if let Ok(relative) = path.strip_prefix(&root_a) {
                    format!("a/{}", relative.to_string_lossy().replace('\\', "/"))
                } else if let Ok(relative) = path.strip_prefix(&root_b) {
                    format!("b/{}", relative.to_string_lossy().replace('\\', "/"))
                } else {
                    panic!("indexed action is outside the configured roots");
                }
            })
            .collect::<Vec<_>>();
        let mut members = relative_identities.clone();
        members.sort_unstable();

        let mut membership = crate::performance::workloads::StableSignature::new(
            0,
            "multi-root-index-membership",
            members.len(),
        );
        for relative in members {
            membership.bytes(relative.as_bytes());
        }
        let mut order = crate::performance::workloads::StableSignature::new(
            0,
            "multi-root-index-order",
            relative_identities.len(),
        );
        for relative in relative_identities {
            order.bytes(relative.as_bytes());
        }
        (membership.finish(), order.finish())
    }

    fn timing_summary(
        mut samples: [u64; crate::performance::workloads::SAMPLE_COUNT],
        warmups: usize,
    ) -> crate::performance::workloads::TimingSummary {
        samples.sort_unstable();
        let percentile = |percent: usize| {
            let rank = (samples.len() * percent).div_ceil(100).max(1);
            samples[rank - 1]
        };
        crate::performance::workloads::TimingSummary {
            warmups,
            samples: samples.len(),
            p50_nanos: percentile(50),
            p95_nanos: percentile(95),
            max_nanos: samples[samples.len() - 1],
        }
    }

    fn run_index_config_sample(
        app: &mut LauncherApp,
        config: &crate::indexer::coordinator::IndexConfig,
        custom: &[crate::actions::Action],
        expected_tail: &[crate::actions::Action],
    ) -> [u64; 3] {
        let request_started = Instant::now();
        app.request_index_config(config.clone());
        let request_nanos = request_started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        let generation = app
            .indexing
            .expected_generation
            .expect("the changed config is accepted by the coordinator");

        let completion = app
            .wait_for_index_completion_for_test(generation)
            .expect("the accepted request completes");
        let completion_nanos = request_started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        assert_eq!(completion.config(), config);
        assert_eq!(
            completion
                .outcome()
                .as_ref()
                .expect("synthetic index traversal succeeds")
                .as_slice(),
            expected_tail
        );

        // Wait for and consume the real app-scoped notification outside the
        // publication timer, then dispatch that exact event through the GUI
        // event reducer below.
        let ready = app
            .rx
            .recv_timeout(Duration::from_secs(30))
            .expect("worker completion sends the app-scoped ready event");
        assert!(matches!(ready, WatchEvent::IndexReady));
        app.event_tx
            .send(ready)
            .expect("the app event receiver remains connected");

        let publication_started = Instant::now();
        app.process_watch_events();
        let publication_nanos = publication_started
            .elapsed()
            .as_nanos()
            .min(u64::MAX as u128) as u64;
        assert!(app.rx.try_recv().is_err(), "ready event was consumed");
        assert_eq!(app.indexing.desired, *config);
        assert_eq!(app.indexing.expected_generation, Some(generation));
        assert_eq!(app.custom_len, custom.len());
        assert_eq!(&app.actions[..custom.len()], custom);
        assert_eq!(&app.actions[custom.len()..], expected_tail);
        assert_eq!(app.last_search_query, app.query);
        assert!(app.last_results_valid);
        [request_nanos, completion_nanos, publication_nanos]
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
        let (event_tx, event_rx) = super::event_channel::channel();
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

    fn track_c_watch_signature_string(
        signature: &mut crate::performance::track_c_workloads::StableSignature,
        value: &str,
    ) {
        signature.number(value.len() as u64);
        signature.bytes(value.as_bytes());
    }

    fn track_c_watch_normalize_indexed_path(root: &Path, value: &str) -> String {
        let root = root
            .to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_lowercase();
        let value = value.replace('\\', "/");
        let folded = value.to_lowercase();
        if folded == root {
            return "@owned-index".into();
        }
        let Some(relative) = folded
            .strip_prefix(&root)
            .filter(|suffix| suffix.starts_with('/'))
        else {
            return value;
        };
        format!("@owned-index{relative}")
    }

    fn track_c_watch_signature_path_field(
        signature: &mut crate::performance::track_c_workloads::StableSignature,
        root: &Path,
        value: &str,
    ) {
        track_c_watch_signature_string(
            signature,
            &track_c_watch_normalize_indexed_path(root, value),
        );
    }

    fn track_c_expected_cached_search_entry(action: &crate::actions::Action) -> CachedSearchEntry {
        CachedSearchEntry {
            label_lc: action.label.to_lowercase(),
            desc_lc: action.desc.to_lowercase(),
            action_lc: action.action.to_lowercase(),
        }
    }

    fn track_c_expected_filter_metadata(
        action: &crate::actions::Action,
    ) -> crate::common::query::ActionFilterMetadata {
        let mut normalized_kind_candidates = Vec::new();
        if !action.desc.trim().is_empty() {
            normalized_kind_candidates.push(action.desc.trim().to_lowercase());
        }
        if let Some(prefix) = action.action.split(':').next()
            && !prefix.trim().is_empty()
        {
            normalized_kind_candidates.push(prefix.trim().to_lowercase());
        }
        normalized_kind_candidates.sort_unstable();
        normalized_kind_candidates.dedup();
        crate::common::query::ActionFilterMetadata {
            normalized_id: action.action.to_lowercase(),
            normalized_kind_candidates,
        }
    }

    fn track_c_assert_action_lookup_matches(
        lookup: &crate::actions::ActionIdLookup,
        expected_actions: &[crate::actions::Action],
    ) {
        let actual = lookup.iter().collect::<std::collections::HashMap<_, _>>();
        let expected = expected_actions
            .iter()
            .map(|action| (action.action.as_str(), action))
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            actual, expected,
            "resolved ID lookup preserves full Actions"
        );
    }

    fn track_c_expected_actions_by_id(
        actions: &[crate::actions::Action],
    ) -> std::collections::HashMap<String, crate::actions::Action> {
        actions
            .iter()
            .cloned()
            .map(|action| (action.action.clone(), action))
            .collect()
    }

    fn track_c_watch_action_signature(root: &Path, actions: &[crate::actions::Action]) -> u64 {
        use crate::performance::track_c_workloads::StableSignature;

        let mut signature = StableSignature::new(0, "track-c-watch-actions", actions.len());
        for action in actions {
            track_c_watch_signature_string(&mut signature, &action.label);
            track_c_watch_signature_path_field(&mut signature, root, &action.desc);
            track_c_watch_signature_path_field(&mut signature, root, &action.action);
            match action.args.as_deref() {
                Some(args) => {
                    signature.number(1);
                    track_c_watch_signature_string(&mut signature, args);
                }
                None => signature.number(0),
            }
        }
        signature.finish()
    }

    fn track_c_watch_projection_signature(app: &LauncherApp, root: &Path) -> u64 {
        use crate::performance::track_c_workloads::StableSignature;

        let mut signature = StableSignature::new(0, "track-c-watch-projections", app.actions.len());
        signature.number(track_c_watch_action_signature(root, app.actions.as_slice()));
        signature.number(app.custom_len as u64);
        signature.number(app.action_cache.len() as u64);
        for cached in &app.action_cache {
            track_c_watch_signature_string(&mut signature, &cached.label_lc);
            track_c_watch_signature_path_field(&mut signature, root, &cached.desc_lc);
            track_c_watch_signature_path_field(&mut signature, root, &cached.action_lc);
        }
        signature.number(app.action_filter_metadata.len() as u64);
        for metadata in &app.action_filter_metadata {
            track_c_watch_signature_path_field(&mut signature, root, &metadata.normalized_id);
            signature.number(metadata.normalized_kind_candidates.len() as u64);
            for candidate in &metadata.normalized_kind_candidates {
                track_c_watch_signature_path_field(&mut signature, root, candidate);
            }
        }
        let mut by_id = app.actions_by_id.iter().collect::<Vec<_>>();
        by_id.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
        signature.number(by_id.len() as u64);
        for (id, action) in by_id {
            track_c_watch_signature_path_field(&mut signature, root, id);
            signature.number(track_c_watch_action_signature(
                root,
                std::slice::from_ref(action),
            ));
        }
        signature.number(track_c_watch_action_signature(root, &app.results));
        track_c_watch_signature_string(&mut signature, &app.query);
        track_c_watch_signature_string(&mut signature, &app.last_search_query);
        signature.number(app.last_results_valid as u64);
        signature.number(app.last_search_pending as u64);
        signature.number(app.background_query_refresh_pending as u64);
        signature.number(app.last_search_result_catalog_versions_stable as u64);
        match app.selected {
            Some(selected) => {
                signature.number(1);
                signature.number(selected as u64);
            }
            None => signature.number(0),
        }
        signature.number(app.suggestions.len() as u64);
        for suggestion in &app.suggestions {
            track_c_watch_signature_string(&mut signature, suggestion);
        }
        signature.finish()
    }

    struct TrackCActionsOwnerState {
        app: LauncherApp,
        expected_actions: Vec<crate::actions::Action>,
        indexed_tail: Vec<crate::actions::Action>,
        index_root: PathBuf,
        actions_path: PathBuf,
        custom_len: usize,
        actions_version_before: u64,
    }

    struct TrackCWatchQueueOwnerState {
        // Field order ensures the app and its coordinator/registration drop
        // before the global registry serialization guard is released.
        owner: TrackCActionsOwnerState,
        age_recorder: super::event_channel::EventAgeRecorder,
        recorder_capacity: usize,
        expected_actions_events: usize,
        expected_index_ready_events: usize,
        expected_recycle_events: usize,
        expected_event_count: usize,
        expected_metadata: Vec<(
            crate::performance::track_c::EventClass,
            crate::performance::track_c::EventOrigin,
        )>,
        _registry_guard: std::sync::MutexGuard<'static, ()>,
    }

    struct TrackCStaleIndexQueueOwnerState {
        queue: TrackCWatchQueueOwnerState,
        release_blocked_scan: Option<std::sync::mpsc::Sender<()>>,
        blocked_generation: u64,
    }

    impl Drop for TrackCStaleIndexQueueOwnerState {
        fn drop(&mut self) {
            if let Some(release) = self.release_blocked_scan.take() {
                let _ = release.send(());
            }
        }
    }

    struct TrackCWatchQueueSampleReceipt {
        delivered: super::event_channel::EventAgeSnapshot,
        drain: crate::performance::track_c::Snapshot,
        handlers: Vec<crate::performance::track_c::EventSnapshot>,
    }

    struct TrackCActionsFixture {
        // The owner apps are local to each sample and drop before this fixture
        // drops its CWD/temp-directory guard.
        _workspace: crate::performance::track_c_workloads::TrackCWorkspace,
        index_root: PathBuf,
        actions_path: PathBuf,
        settings_path: PathBuf,
        initial_custom: Vec<crate::actions::Action>,
        replacement_custom: Vec<crate::actions::Action>,
        expected_actions: Vec<crate::actions::Action>,
        indexed_tail: Vec<crate::actions::Action>,
        custom_len: usize,
        fixture_signature: u64,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum TrackC1ECase {
        CustomChanged,
        CustomEqual,
        CustomMalformed,
        CustomMissing,
        IndexAcceptedChanged,
    }

    impl TrackC1ECase {
        fn token(self) -> &'static str {
            match self {
                Self::CustomChanged => "custom_changed",
                Self::CustomEqual => "custom_equal",
                Self::CustomMalformed => "custom_malformed",
                Self::CustomMissing => "custom_missing",
                Self::IndexAcceptedChanged => "index_accepted_changed",
            }
        }

        fn typed_load_outcome(self) -> u64 {
            match self {
                Self::CustomChanged | Self::CustomEqual => 1,
                Self::CustomMalformed => 2,
                Self::CustomMissing => 3,
                Self::IndexAcceptedChanged => 0,
            }
        }

        fn version_delta(self) -> u64 {
            match self {
                Self::CustomChanged | Self::IndexAcceptedChanged => 1,
                Self::CustomEqual | Self::CustomMalformed | Self::CustomMissing => 0,
            }
        }

        fn publishes(self) -> bool {
            self.version_delta() != 0
        }
    }

    struct TrackC1EFixture {
        base: TrackCActionsFixture,
        case: TrackC1ECase,
        expected_custom: Vec<crate::actions::Action>,
        expected_tail: Vec<crate::actions::Action>,
        expected_actions: Vec<crate::actions::Action>,
        fixture_signature: u64,
    }

    struct TrackC1EOwnerState {
        // The app owns any test coordinator. It must drop before the observer,
        // registry lock, and the fixture's workspace.
        owner: TrackCActionsOwnerState,
        _age_recorder: Option<super::event_channel::EventAgeRecorder>,
        setup_observer: super::track_c_action_observer::Snapshot,
        observer: Option<super::track_c_action_observer::Guard>,
        _registry_guard: std::sync::MutexGuard<'static, ()>,
        case: TrackC1ECase,
        accepted_index_config: Option<crate::indexer::coordinator::IndexConfig>,
        accepted_index_generation: Option<u64>,
        expected_custom: Vec<crate::actions::Action>,
        expected_tail: Vec<crate::actions::Action>,
        expected_actions: Vec<crate::actions::Action>,
        fixture_signature: u64,
        initial_actions_ptr: usize,
        initial_radial_ptr: usize,
        initial_radial_builds: usize,
        initial_root_generation: u64,
        initial_history_digest: u64,
        initial_completion_keys: Vec<String>,
        initial_suggestions: Vec<String>,
        pins: Vec<crate::history::HistoryPin>,
        tail_selected_clone_bytes: u64,
        index_custom_selected_clone_bytes: u64,
        index_tail_selected_clone_bytes: u64,
        id_key_selected_clone_bytes: u64,
    }

    struct TrackC1EValidatedSample {
        observer: super::track_c_action_observer::Snapshot,
        observation: crate::performance::track_c_workloads::OwnerObservation,
    }

    fn track_c_actions_fixture(total_count: usize) -> TrackCActionsFixture {
        use crate::performance::track_c_workloads;

        assert!(total_count >= 4);
        let workspace = track_c_workloads::TrackCWorkspace::new();
        let owner_root = workspace.root().join("actions-owner");
        std::fs::create_dir_all(&owner_root).expect("create owned action fixture directory");
        let index_root = owner_root.join("indexed");
        let custom_len = total_count / 2;
        let indexed_count = total_count - custom_len;
        let index_fixture = track_c_workloads::index_tree(&index_root, indexed_count);
        let indexed_tail = indexed_actions(&index_root, indexed_count);
        assert_eq!(indexed_tail.len(), indexed_count);

        let custom_fixture = track_c_workloads::action_fixture(custom_len);
        let custom_fixture_signature =
            track_c_workloads::action_fixture_identity(&custom_fixture.values);
        let mut initial_custom = custom_fixture.values.clone();
        let mut replacement_custom = custom_fixture.values;
        // Deliberately collide one custom id with the first indexed id. The
        // catalog remains custom-first and the independent id-only projection
        // must therefore retain the later indexed winner.
        initial_custom[0].action = indexed_tail[0].action.clone();
        replacement_custom[0].action = indexed_tail[0].action.clone();
        replacement_custom[0]
            .desc
            .push_str("; accepted external replacement");

        let mut expected_actions = replacement_custom.clone();
        expected_actions.extend(indexed_tail.iter().cloned());
        let actions_path = owner_root.join("actions.json");
        let settings_path = workspace.settings_path();
        let canonical_index_root =
            std::fs::canonicalize(&index_root).expect("canonicalize owned index fixture root");

        let mut fixture =
            track_c_workloads::StableSignature::new(0, "track-c-actions-watch-owner", total_count);
        fixture.number(custom_fixture_signature);
        fixture.number(index_fixture.signature);
        fixture.number(track_c_watch_action_signature(
            &canonical_index_root,
            &initial_custom,
        ));
        fixture.number(track_c_watch_action_signature(
            &canonical_index_root,
            &replacement_custom,
        ));
        fixture.number(track_c_watch_action_signature(
            &canonical_index_root,
            &indexed_tail,
        ));
        fixture.number(custom_len as u64);
        fixture.number(indexed_count as u64);
        fixture.bytes(b"custom-first-index-tail-duplicate-id-fixed-json-payload");
        fixture.bytes(b"query=app Synthetic action 00009;match_exact=true;plugins=inert");
        fixture.bytes(
            b"enabled-plugins=empty;folder-bookmark-aliases=empty;usage=empty;usage-weight=0;fuzzy-weight=1;toasts=off;dashboard=off;hotkeys=off",
        );
        let fixture_signature = fixture.finish();

        TrackCActionsFixture {
            _workspace: workspace,
            index_root: canonical_index_root,
            actions_path,
            settings_path,
            initial_custom,
            replacement_custom,
            expected_actions,
            indexed_tail,
            custom_len,
            fixture_signature,
        }
    }

    impl Drop for TrackC1EOwnerState {
        fn drop(&mut self) {
            // Disable the thread-local observer before LauncherApp and any
            // coordinator fields are dropped during unwinding or teardown.
            self.observer.take();
        }
    }

    fn track_c1e_merge_observer_snapshots(
        mut owner: super::track_c_action_observer::Snapshot,
        setup: &super::track_c_action_observer::Snapshot,
    ) -> super::track_c_action_observer::Snapshot {
        for (value, setup_value) in owner.phases_ns.iter_mut().zip(setup.phases_ns) {
            *value = value.saturating_add(setup_value);
        }
        for (value, setup_value) in owner.phase_calls.iter_mut().zip(setup.phase_calls) {
            *value = value.saturating_add(setup_value);
        }
        for (value, setup_value) in owner.counters.iter_mut().zip(setup.counters) {
            *value = value.saturating_add(setup_value);
        }
        owner
    }

    fn track_c1e_action_payload_bytes(action: &crate::actions::Action) -> u64 {
        action.label.len() as u64
            + action.desc.len() as u64
            + action.action.len() as u64
            + action.args.as_ref().map_or(0, |args| args.len() as u64)
    }

    fn track_c1e_selected_payload_bytes(actions: &[crate::actions::Action]) -> u64 {
        actions.iter().map(track_c1e_action_payload_bytes).sum()
    }

    fn track_c1f_id_key_bytes(actions: &[crate::actions::Action]) -> u64 {
        actions
            .iter()
            .map(|action| action.action.len() as u64)
            .sum()
    }

    fn track_c1e_fixture(total_count: usize, case: TrackC1ECase) -> TrackC1EFixture {
        use crate::performance::track_c_workloads::StableSignature;

        let base = track_c_actions_fixture(total_count);
        let mut expected_custom = base.initial_custom.clone();
        let mut expected_tail = base.indexed_tail.clone();
        match case {
            TrackC1ECase::CustomChanged => expected_custom = base.replacement_custom.clone(),
            TrackC1ECase::IndexAcceptedChanged => {
                expected_tail[0]
                    .desc
                    .push_str("; accepted synthetic index generation");
            }
            TrackC1ECase::CustomEqual
            | TrackC1ECase::CustomMalformed
            | TrackC1ECase::CustomMissing => {}
        }
        let mut expected_actions = expected_custom.clone();
        expected_actions.extend(expected_tail.iter().cloned());

        let mut signature =
            StableSignature::new(0, "track-c1e-action-publication-fixture", total_count);
        signature.number(base.fixture_signature);
        signature.bytes(case.token().as_bytes());
        if case == TrackC1ECase::IndexAcceptedChanged {
            signature.bytes(
                b"index-config-root=track-c1e-accepted-generation;max-items=expected-tail-count",
            );
        }
        signature.number(base.custom_len as u64);
        signature.number(base.indexed_tail.len() as u64);
        signature.number(track_c_watch_action_signature(
            &base.index_root,
            &base.initial_custom,
        ));
        signature.number(track_c_watch_action_signature(
            &base.index_root,
            &expected_custom,
        ));
        signature.number(track_c_watch_action_signature(
            &base.index_root,
            &expected_tail,
        ));
        signature.bytes(b"query=app Synthetic action 00009;exact=true;usage=empty");
        signature.bytes(b"plugins=inert;aliases=empty;dashboard=off;toasts=off;hotkeys=off");
        signature.bytes(b"action-option-args-preserved;duplicate-id-last-catalog-occurrence");
        let fixture_signature = signature.finish();

        TrackC1EFixture {
            base,
            case,
            expected_custom,
            expected_tail,
            expected_actions,
            fixture_signature,
        }
    }

    fn track_c1e_completion_keys(app: &LauncherApp) -> Vec<String> {
        use fst::{IntoStreamer, Streamer};

        let Some(index) = app.completion_index.as_ref() else {
            return Vec::new();
        };
        let mut stream = index.stream().into_stream();
        let mut keys = Vec::new();
        while let Some((key, _)) = stream.next() {
            keys.push(
                std::str::from_utf8(key)
                    .expect("completion keys are UTF-8")
                    .to_owned(),
            );
        }
        keys
    }

    fn track_c1e_expected_completion_keys(
        app: &LauncherApp,
        actions: &[crate::actions::Action],
    ) -> Vec<String> {
        let mut keys = app
            .command_cache
            .iter()
            .map(|action| action.label.to_lowercase())
            .chain(
                actions
                    .iter()
                    .map(|action| format!("app {}", action.label.to_lowercase())),
            )
            .collect::<Vec<_>>();
        keys.sort();
        keys.dedup();
        keys
    }

    fn track_c1e_expected_suggestions(keys: &[String], query: &str, limit: usize) -> Vec<String> {
        let query = query.to_lowercase();
        keys.iter()
            .filter(|key| key.starts_with(&query) && key.as_str() != query)
            .take(limit)
            .cloned()
            .collect()
    }

    fn track_c1e_fixture_pin(
        action_id: &str,
        args: Option<String>,
        timestamp: i64,
    ) -> crate::history::HistoryPin {
        crate::history::HistoryPin {
            action_id: action_id.to_owned(),
            label: "Saved synthetic pin".into(),
            desc: "Saved synthetic description".into(),
            args,
            query: "app Synthetic action".into(),
            timestamp,
        }
    }

    fn make_track_c1e_owner_state(fixture: &TrackC1EFixture) -> TrackC1EOwnerState {
        use crate::indexer::coordinator::{IndexConfig, IndexCoordinator};

        let registry_guard = TRACK_C_EVENT_REGISTRY_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut owner = make_track_c_actions_owner_state(&fixture.base);
        let mut age_recorder = None;
        let mut setup_observer = super::track_c_action_observer::Snapshot::default();
        let mut accepted_index_config = None;
        let mut accepted_index_generation = None;
        if fixture.case == TrackC1ECase::IndexAcceptedChanged {
            // Consume only the setup helper's original equal Actions envelope.
            // The measured event below is the coordinator's original queued
            // IndexReady envelope; it is never received and re-enqueued.
            std::fs::write(
                &owner.actions_path,
                serde_json::to_vec(&fixture.base.initial_custom)
                    .expect("serialize initial custom state outside owner"),
            )
            .expect("restore initial custom action payload before index setup");
            let initial_pointer = Arc::as_ptr(&owner.app.actions) as usize;
            owner.app.process_watch_events();
            assert_eq!(Arc::as_ptr(&owner.app.actions) as usize, initial_pointer);
            assert!(owner.app.rx.try_recv().is_err());
            owner.expected_actions = fixture.base.initial_custom.clone();

            age_recorder = Some(install_track_c_observed_channel(&mut owner.app, 2));
            let accepted_tail = fixture.expected_tail.clone();
            let coordinator =
                IndexCoordinator::with_test_scanner(move |_| Ok(accepted_tail.clone()))
                    .expect("construct inert accepted-index coordinator");
            owner.app.install_test_index_coordinator(coordinator);
            let setup_guard = super::track_c_action_observer::Guard::install();
            let config = IndexConfig::new(
                vec!["track-c1e-accepted-generation".into()],
                Some(fixture.expected_tail.len()),
            );
            accepted_index_config = Some(config.clone());
            {
                let _request = super::track_c_action_observer::scope(
                    super::track_c_action_observer::Phase::IndexRequest,
                );
                owner.app.request_index_config(config.clone());
            }
            let generation = owner
                .app
                .indexing
                .expected_generation
                .expect("accepted synthetic index generation");
            accepted_index_generation = Some(generation);
            {
                let _wait = super::track_c_action_observer::scope(
                    super::track_c_action_observer::Phase::ScannerWait,
                );
                let completion = owner
                    .app
                    .wait_for_index_completion_for_test(generation)
                    .expect("accepted synthetic index scan completes");
                assert_eq!(completion.config(), &config);
                assert_eq!(
                    completion
                        .outcome()
                        .as_ref()
                        .expect("scan succeeds")
                        .as_slice(),
                    fixture.expected_tail.as_slice()
                );
                wait_for_track_c_successful_enqueue(
                    &owner.app,
                    age_recorder.as_ref().expect("observed index sender"),
                    1,
                    1,
                );
            }
            owner.indexed_tail = fixture.expected_tail.clone();
            owner.expected_actions = fixture.expected_actions.clone();
            owner.app.completion_rebuild_after =
                Some(Instant::now() + super::COMPLETION_REBUILD_DEBOUNCE);
            setup_observer = setup_guard.snapshot();
            drop(setup_guard);
        } else {
            match fixture.case {
                TrackC1ECase::CustomChanged => {}
                TrackC1ECase::CustomEqual => {
                    std::fs::write(
                        &owner.actions_path,
                        serde_json::to_vec(&fixture.base.initial_custom)
                            .expect("serialize unchanged custom state outside owner"),
                    )
                    .expect("write equal custom action payload");
                    owner.expected_actions = fixture.expected_actions.clone();
                }
                TrackC1ECase::CustomMalformed => {
                    std::fs::write(&owner.actions_path, b"{")
                        .expect("write malformed action payload");
                    owner.expected_actions = fixture.expected_actions.clone();
                }
                TrackC1ECase::CustomMissing => {
                    std::fs::remove_file(&owner.actions_path)
                        .expect("remove action payload before missing-file owner");
                    owner.expected_actions = fixture.expected_actions.clone();
                }
                TrackC1ECase::IndexAcceptedChanged => unreachable!(),
            }
        }

        let initial_actions_ptr = Arc::as_ptr(&owner.app.actions) as usize;
        let radial_before = owner.app.cached_universal_action_catalog_snapshot();
        assert_eq!(
            radial_before
                .entries
                .iter()
                .take(owner.app.actions.len())
                .map(|entry| entry.selected_action.clone())
                .collect::<Vec<_>>(),
            owner.app.actions.as_ref().clone()
        );
        let initial_radial_ptr = Arc::as_ptr(&radial_before) as usize;
        drop(radial_before);
        let initial_radial_builds = owner
            .app
            .authoring_catalog_build_count
            .load(Ordering::SeqCst);
        let initial_root_generation = owner.app.root_list_geometry.result_generation_for_test();
        let initial_history_digest = owner.app.query_history.acceptance_digest();
        let initial_completion_keys = track_c1e_completion_keys(&owner.app);
        assert!(!initial_completion_keys.is_empty());
        let initial_suggestions = owner.app.suggestions.clone();
        assert_eq!(owner.app.actions.len(), fixture.expected_actions.len());

        let duplicate_id = fixture.base.indexed_tail[0].action.clone();
        let argument_id = fixture.base.initial_custom[3].action.clone();
        assert!(fixture.base.initial_custom[3].args.is_some());
        let pins = vec![
            track_c1e_fixture_pin(&duplicate_id, Some("saved args differ".into()), 2),
            track_c1e_fixture_pin(&argument_id, None, 1),
        ];
        let full_catalog = &fixture.base.initial_custom;
        let full_indexed = &fixture.base.indexed_tail;
        let tail_selected_clone_bytes = track_c1e_selected_payload_bytes(full_indexed);
        let index_custom_selected_clone_bytes = track_c1e_selected_payload_bytes(full_catalog);
        let index_tail_selected_clone_bytes =
            track_c1e_selected_payload_bytes(&fixture.expected_tail);
        let id_key_selected_clone_bytes = track_c1f_id_key_bytes(&fixture.expected_actions);

        crate::performance::track_c::reset();
        TrackC1EOwnerState {
            owner,
            _age_recorder: age_recorder,
            setup_observer,
            observer: Some(super::track_c_action_observer::Guard::install()),
            _registry_guard: registry_guard,
            case: fixture.case,
            accepted_index_config,
            accepted_index_generation,
            expected_custom: fixture.expected_custom.clone(),
            expected_tail: fixture.expected_tail.clone(),
            expected_actions: fixture.expected_actions.clone(),
            fixture_signature: fixture.fixture_signature,
            initial_actions_ptr,
            initial_radial_ptr,
            initial_radial_builds,
            initial_root_generation,
            initial_history_digest,
            initial_completion_keys,
            initial_suggestions,
            pins,
            tail_selected_clone_bytes,
            index_custom_selected_clone_bytes,
            index_tail_selected_clone_bytes,
            id_key_selected_clone_bytes,
        }
    }

    fn track_c1e_phase_values(
        samples: &[TrackC1EValidatedSample],
        phase: super::track_c_action_observer::Phase,
    ) -> Vec<u64> {
        samples
            .iter()
            .map(|sample| sample.observer.phase_ns(phase))
            .collect()
    }

    fn track_c1e_phase_call_values(
        samples: &[TrackC1EValidatedSample],
        phase: super::track_c_action_observer::Phase,
    ) -> Vec<u64> {
        samples
            .iter()
            .map(|sample| sample.observer.phase_calls(phase))
            .collect()
    }

    fn track_c1e_counter_values(
        samples: &[TrackC1EValidatedSample],
        counter: super::track_c_action_observer::Counter,
    ) -> Vec<u64> {
        samples
            .iter()
            .map(|sample| sample.observer.counter(counter))
            .collect()
    }

    fn validate_track_c1e_owner(state: &mut TrackC1EOwnerState) -> TrackC1EValidatedSample {
        use super::track_c_action_observer::{Counter, Phase};
        use crate::performance::track_c_workloads::{OwnerObservation, StableSignature};

        // Freeze owner receipts before any untimed completion-index rebuild or
        // consumer verification can add work to the measured observer.
        let observer_guard = state
            .observer
            .take()
            .expect("the Track C1E observer is scoped to one owner invocation");
        let observer =
            track_c1e_merge_observer_snapshots(observer_guard.snapshot(), &state.setup_observer);
        drop(observer_guard);
        let expected_by_id = track_c_expected_actions_by_id(&state.expected_actions);

        let published = state.case.publishes();
        let expected_id_key_attempts = if published {
            state.expected_actions.len() as u64
        } else {
            0
        };
        assert_eq!(
            (
                observer.counter(Counter::IdKeysCloned),
                observer.counter(Counter::IdActionsProcessed),
            ),
            (expected_id_key_attempts, expected_id_key_attempts),
            "key-clone and processed-action receipts match actual lookup construction"
        );
        let expected_version = state
            .owner
            .actions_version_before
            .saturating_add(state.case.version_delta());
        let version_after = crate::actions::actions_version();
        assert_eq!(version_after, expected_version);
        assert_eq!(
            observer.counter(Counter::TypedLoadOutcome),
            state.case.typed_load_outcome(),
            "typed loader outcome comes from the actual owner branch"
        );
        let expected_loaded = match state.case {
            TrackC1ECase::CustomChanged | TrackC1ECase::CustomEqual => {
                state.expected_custom.len() as u64
            }
            TrackC1ECase::CustomMalformed
            | TrackC1ECase::CustomMissing
            | TrackC1ECase::IndexAcceptedChanged => 0,
        };
        assert_eq!(
            observer.counter(Counter::TypedActionsLoaded),
            expected_loaded
        );
        assert_eq!(
            observer.phase_calls(Phase::ActionsEvent),
            (state.case != TrackC1ECase::IndexAcceptedChanged) as u64
        );
        assert_eq!(
            observer.phase_calls(Phase::IndexReadyOwner),
            (state.case == TrackC1ECase::IndexAcceptedChanged) as u64
        );
        assert_eq!(
            observer.counter(Counter::IndexAccepted),
            (state.case == TrackC1ECase::IndexAcceptedChanged) as u64
        );
        assert_eq!(observer.counter(Counter::IndexRejected), 0);

        let expected_actions_version = expected_version;
        if published {
            assert_track_c_actions_owner_state(
                &state.owner,
                &state.expected_actions,
                expected_actions_version,
            );
        } else {
            let app = &state.owner.app;
            assert_eq!(app.actions.as_slice(), state.expected_actions.as_slice());
            assert_eq!(
                crate::actions::actions_version(),
                expected_actions_version,
                "equal or failed load retains the current catalog generation"
            );
            assert_eq!(app.query, "app Synthetic action 00009");
            assert_eq!(app.last_search_query, app.query);
            assert_eq!(app.results, vec![state.expected_actions[9].clone()]);
            assert!(app.last_results_valid);
            assert!(!app.last_search_pending);
            assert!(!app.background_query_refresh_pending);
            assert_eq!(app.selected, Some(0), "no-op reload retains selection");
        }
        let app = &mut state.owner.app;
        if let (Some(config), Some(generation)) = (
            state.accepted_index_config.as_ref(),
            state.accepted_index_generation,
        ) {
            assert_eq!(&app.indexing.desired, config);
            assert_eq!(app.indexing.expected_generation, Some(generation));
        } else {
            assert!(state.accepted_index_config.is_none());
            assert!(state.accepted_index_generation.is_none());
        }
        assert_eq!(app.custom_len, state.expected_custom.len());
        assert_eq!(
            &app.actions[..app.custom_len],
            state.expected_custom.as_slice(),
            "custom-first prefix preserves every Action field and Option args"
        );
        assert_eq!(
            &app.actions[app.custom_len..],
            state.expected_tail.as_slice(),
            "indexed-tail membership and order match the accepted generation"
        );
        let expected_cache = state
            .expected_actions
            .iter()
            .map(track_c_expected_cached_search_entry)
            .collect::<Vec<_>>();
        assert_eq!(app.action_cache, expected_cache);
        let expected_filter = state
            .expected_actions
            .iter()
            .map(track_c_expected_filter_metadata)
            .collect::<Vec<_>>();
        assert_eq!(app.action_filter_metadata, expected_filter);
        track_c_assert_action_lookup_matches(&app.actions_by_id, &state.expected_actions);
        assert_eq!(app.results, vec![state.expected_actions[9].clone()]);
        assert_eq!(app.query, "app Synthetic action 00009");
        assert_eq!(app.last_search_query, app.query);
        assert!(app.last_results_valid);
        assert!(!app.last_search_pending);
        assert!(!app.background_query_refresh_pending);
        assert_eq!(app.selected, if published { None } else { Some(0) });
        assert_eq!(
            app.query_history.acceptance_digest(),
            state.initial_history_digest,
            "publication refresh does not alter query-history navigation state"
        );
        assert_eq!(
            app.root_list_geometry.result_generation_for_test(),
            state.initial_root_generation + published as u64,
            "only a changed catalog/search refresh invalidates root result geometry"
        );

        let diagnostic_present = app.actions_persistence_diagnostic.is_some();
        assert_eq!(
            diagnostic_present,
            matches!(
                state.case,
                TrackC1ECase::CustomMalformed | TrackC1ECase::CustomMissing
            ),
            "malformed and missing files retain last-good state with a diagnostic"
        );
        if !diagnostic_present {
            assert!(app.error.is_none());
        } else {
            assert!(app.actions_persistence_diagnostic.is_some());
        }

        let expected_pin_actions = state
            .pins
            .iter()
            .map(|pin| expected_by_id.get(&pin.action_id).cloned())
            .collect::<Vec<_>>();
        let resolved_pin_actions = state
            .pins
            .iter()
            .map(|pin| app.resolve_pin_action(pin))
            .collect::<Vec<_>>();
        assert_eq!(resolved_pin_actions, expected_pin_actions);

        let completion_keys_before_rebuild = track_c1e_completion_keys(app);
        if published {
            assert!(app.completion_index.is_none());
            assert!(app.action_completion_dirty);
            assert!(app.suggestions.is_empty());
            app.rebuild_completion_index_now();
        } else {
            assert!(app.completion_index.is_some());
            assert!(!app.action_completion_dirty);
            assert_eq!(
                completion_keys_before_rebuild,
                state.initial_completion_keys
            );
            assert_eq!(app.suggestions, state.initial_suggestions);
        }
        let expected_completion_keys =
            track_c1e_expected_completion_keys(app, &state.expected_actions);
        let completion_keys = track_c1e_completion_keys(app);
        assert_eq!(completion_keys, expected_completion_keys);
        assert!(!app.action_completion_dirty);
        let expected_suggestions =
            track_c1e_expected_suggestions(&expected_completion_keys, &app.query, 5);
        assert_eq!(app.suggestions, expected_suggestions);

        let radial = app.cached_universal_action_catalog_snapshot();
        let radial_actions = radial
            .entries
            .iter()
            .take(state.expected_actions.len())
            .map(|entry| entry.selected_action.clone())
            .collect::<Vec<_>>();
        assert_eq!(radial_actions, state.expected_actions);
        let radial_pointer = Arc::as_ptr(&radial) as usize;
        let radial_builds = app.authoring_catalog_build_count.load(Ordering::SeqCst);
        assert_eq!(
            radial_builds,
            state.initial_radial_builds + published as usize,
            "the radial catalog rebuilds only when the action generation changes"
        );
        assert_eq!(radial_pointer == state.initial_radial_ptr, !published);
        drop(radial);
        let actions_pointer = Arc::as_ptr(&app.actions) as usize;
        let actions_arc_retained = actions_pointer == state.initial_actions_ptr;
        assert_eq!(actions_arc_retained, !published);

        if state.case == TrackC1ECase::IndexAcceptedChanged {
            assert_eq!(app.rx.queued_depth(), Some(0));
            let delivered = state
                ._age_recorder
                .as_ref()
                .expect("accepted index uses its original observed channel")
                .snapshot();
            assert_eq!(delivered.delivered.len(), 1);
            assert_eq!(delivered.overflow, 0);
            assert_eq!(delivered.abandoned, 0);
            assert_eq!(
                (delivered.delivered[0].class, delivered.delivered[0].origin),
                (
                    crate::performance::track_c::EventClass::IndexCompletion,
                    EventOrigin::IndexCoordinator
                )
            );
        } else {
            assert_eq!(app.rx.queued_depth(), Some(0));
        }

        let root_generation = app.root_list_geometry.result_generation_for_test();
        let history_digest = app.query_history.acceptance_digest();
        let radial_build_delta = radial_builds.saturating_sub(state.initial_radial_builds);
        let mut complete = StableSignature::new(
            state.fixture_signature,
            "track-c1e-complete-consumer-state",
            state.expected_actions.len(),
        );
        complete.number(track_c_watch_projection_signature(
            app,
            &state.owner.index_root,
        ));
        complete.number(state.case.version_delta());
        if let (Some(config), Some(generation)) = (
            state.accepted_index_config.as_ref(),
            state.accepted_index_generation,
        ) {
            complete.bytes(b"accepted-index-config");
            complete.number(generation);
            complete.number(config.roots().len() as u64);
            for root in config.roots() {
                track_c_watch_signature_string(&mut complete, root);
            }
            match config.max_items() {
                Some(max_items) => {
                    complete.number(1);
                    complete.number(max_items as u64);
                }
                None => complete.number(0),
            }
        } else {
            complete.bytes(b"no-index-config");
        }
        complete.number(root_generation.saturating_sub(state.initial_root_generation));
        complete.number(radial_build_delta as u64);
        complete.number(actions_arc_retained as u64);
        complete.number(history_digest);
        complete.number(app.action_completion_dirty as u64);
        complete.number(app.command_completion_dirty as u64);
        complete.number(completion_keys.len() as u64);
        for key in &completion_keys {
            track_c_watch_signature_string(&mut complete, key);
        }
        complete.number(resolved_pin_actions.len() as u64);
        for action in &resolved_pin_actions {
            if let Some(action) = action {
                complete.bytes(b"resolved");
                complete.number(track_c_watch_action_signature(
                    &state.owner.index_root,
                    std::slice::from_ref(action),
                ));
            } else {
                complete.bytes(b"fallback");
            }
        }
        complete.bytes(state.case.token().as_bytes());
        complete.bytes(if diagnostic_present {
            b"diagnostic-retained"
        } else {
            b"diagnostic-none"
        });
        let output_identity = complete.finish();
        let structural_signature =
            track_c_watch_action_signature(&state.owner.index_root, &app.actions);
        let mut revisions = vec![
            state.owner.actions_version_before,
            version_after,
            state.initial_root_generation,
            root_generation,
            state.initial_radial_builds as u64,
            radial_builds as u64,
            state.initial_history_digest,
            history_digest,
            app.last_search_provider_revision,
        ];
        if let Some(versions) = app.last_search_result_catalog_versions {
            revisions.extend([versions.clipboard, versions.todo, versions.notes]);
        } else {
            revisions.extend([u64::MAX, u64::MAX, u64::MAX]);
        }
        let work_units = state.expected_actions.len() as u64;
        let work_counters = vec![
            state.expected_custom.len() as u64,
            state.expected_tail.len() as u64,
            app.action_cache.len() as u64,
            app.action_filter_metadata.len() as u64,
            app.actions_by_id.len() as u64,
            app.results.len() as u64,
            completion_keys.len() as u64,
            app.suggestions.len() as u64,
            state.case.version_delta(),
            actions_arc_retained as u64,
            if state.case == TrackC1ECase::CustomChanged {
                state.tail_selected_clone_bytes
            } else {
                0
            },
            if state.case == TrackC1ECase::IndexAcceptedChanged {
                state.index_custom_selected_clone_bytes
            } else {
                0
            },
            if state.case == TrackC1ECase::IndexAcceptedChanged {
                state.index_tail_selected_clone_bytes
            } else {
                0
            },
            if published {
                state.id_key_selected_clone_bytes
            } else {
                0
            },
        ];
        let observation = OwnerObservation {
            output_identity,
            structural_signature,
            revision_receipts: revisions,
            viewport_receipt: None,
            work_units,
            work_counters,
        };
        TrackC1EValidatedSample {
            observer,
            observation,
        }
    }

    fn emit_track_c1f_action_results(
        mode: crate::performance::track_c_workloads::BenchmarkMode,
        fixture: &TrackC1EFixture,
        samples: &[crate::performance::track_c_workloads::SampleRecord],
        receipts: &[TrackC1EValidatedSample],
    ) {
        use super::track_c_action_observer::{Counter, Phase};
        use crate::performance::track_c_workloads::{MEASURED_SAMPLES, WARMUPS};

        assert_eq!(samples.len(), MEASURED_SAMPLES);
        assert_eq!(receipts.len(), MEASURED_SAMPLES);
        assert!(samples.iter().all(|sample| {
            sample.output_identity == samples[0].output_identity
                && sample.structural_signature == samples[0].structural_signature
                && sample.work_units == fixture.expected_actions.len() as u64
                && sample.work_counters == samples[0].work_counters
        }));
        let source =
            std::env::var("ML_TRACK_C_SOURCE_SHA").unwrap_or_else(|_| "UNCOMMITTED_SOURCE".into());
        let profile =
            std::env::var("ML_TRACK_C_PROFILE").unwrap_or_else(|_| "unspecified-profile".into());
        let telemetry_enabled = crate::performance::enabled();
        assert!(
            telemetry_enabled,
            "C1E benchmark requires Track C telemetry"
        );
        assert_eq!(
            crate::performance::track_c_workloads::BenchmarkMode::from_process_env(),
            Ok(mode),
            "reported mode matches the benchmark process"
        );
        let custom_count = fixture.expected_custom.len();
        let indexed_count = fixture.expected_tail.len();
        let count = custom_count + indexed_count;
        let mut arrays: Vec<(&str, Vec<u64>)> = vec![
            (
                "actions_event_ns",
                track_c1e_phase_values(receipts, Phase::ActionsEvent),
            ),
            (
                "typed_load_ns",
                track_c1e_phase_values(receipts, Phase::TypedLoad),
            ),
            (
                "tail_copy_ns",
                track_c1e_phase_values(receipts, Phase::TailCopy),
            ),
            (
                "catalog_assembly_arc_ns",
                track_c1e_phase_values(receipts, Phase::CatalogAssemblyArc),
            ),
            (
                "search_entries_ns",
                track_c1e_phase_values(receipts, Phase::SearchEntries),
            ),
            (
                "filter_metadata_ns",
                track_c1e_phase_values(receipts, Phase::FilterMetadata),
            ),
            ("id_map_ns", track_c1e_phase_values(receipts, Phase::IdMap)),
            (
                "completion_invalidation_ns",
                track_c1e_phase_values(receipts, Phase::CompletionInvalidation),
            ),
            (
                "same_query_refresh_ns",
                track_c1e_phase_values(receipts, Phase::SameQueryRefresh),
            ),
            (
                "search_total_ns",
                track_c1e_phase_values(receipts, Phase::SearchTotal),
            ),
            (
                "index_ready_owner_ns",
                track_c1e_phase_values(receipts, Phase::IndexReadyOwner),
            ),
            (
                "index_request_ns",
                track_c1e_phase_values(receipts, Phase::IndexRequest),
            ),
            (
                "scanner_wait_ns",
                track_c1e_phase_values(receipts, Phase::ScannerWait),
            ),
            (
                "index_custom_prefix_copy_ns",
                track_c1e_phase_values(receipts, Phase::IndexCustomPrefixCopy),
            ),
            (
                "index_tail_clone_ns",
                track_c1e_phase_values(receipts, Phase::IndexTailClone),
            ),
            (
                "typed_load_calls",
                track_c1e_phase_call_values(receipts, Phase::TypedLoad),
            ),
            (
                "tail_copy_calls",
                track_c1e_phase_call_values(receipts, Phase::TailCopy),
            ),
            (
                "catalog_assembly_arc_calls",
                track_c1e_phase_call_values(receipts, Phase::CatalogAssemblyArc),
            ),
            (
                "search_entries_calls",
                track_c1e_phase_call_values(receipts, Phase::SearchEntries),
            ),
            (
                "filter_metadata_calls",
                track_c1e_phase_call_values(receipts, Phase::FilterMetadata),
            ),
            (
                "id_map_calls",
                track_c1e_phase_call_values(receipts, Phase::IdMap),
            ),
            (
                "completion_invalidation_calls",
                track_c1e_phase_call_values(receipts, Phase::CompletionInvalidation),
            ),
            (
                "same_query_refresh_calls",
                track_c1e_phase_call_values(receipts, Phase::SameQueryRefresh),
            ),
            (
                "typed_load_outcome",
                track_c1e_counter_values(receipts, Counter::TypedLoadOutcome),
            ),
            (
                "typed_actions_loaded",
                track_c1e_counter_values(receipts, Counter::TypedActionsLoaded),
            ),
            (
                "tail_actions_cloned",
                track_c1e_counter_values(receipts, Counter::TailActionsCloned),
            ),
            (
                "index_custom_actions_cloned",
                track_c1e_counter_values(receipts, Counter::IndexCustomActionsCloned),
            ),
            (
                "index_tail_actions_cloned",
                track_c1e_counter_values(receipts, Counter::IndexTailActionsCloned),
            ),
            (
                "catalog_assembly_actions",
                track_c1e_counter_values(receipts, Counter::CatalogAssemblyActions),
            ),
            (
                "search_entries_processed",
                track_c1e_counter_values(receipts, Counter::SearchEntriesProcessed),
            ),
            (
                "filter_entries_processed",
                track_c1e_counter_values(receipts, Counter::FilterEntriesProcessed),
            ),
            (
                "id_actions_processed",
                track_c1e_counter_values(receipts, Counter::IdActionsProcessed),
            ),
        ];
        arrays.extend([
            (
                "cache_entries",
                samples
                    .iter()
                    .map(|sample| sample.work_counters[2])
                    .collect(),
            ),
            (
                "filter_entries",
                samples
                    .iter()
                    .map(|sample| sample.work_counters[3])
                    .collect(),
            ),
            (
                "id_entries",
                samples
                    .iter()
                    .map(|sample| sample.work_counters[4])
                    .collect(),
            ),
            (
                "query_results",
                samples
                    .iter()
                    .map(|sample| sample.work_counters[5])
                    .collect(),
            ),
            (
                "completion_keys",
                samples
                    .iter()
                    .map(|sample| sample.work_counters[6])
                    .collect(),
            ),
            (
                "suggestions",
                samples
                    .iter()
                    .map(|sample| sample.work_counters[7])
                    .collect(),
            ),
            (
                "actions_version_delta",
                samples
                    .iter()
                    .map(|sample| sample.work_counters[8])
                    .collect(),
            ),
            (
                "arc_retained",
                samples
                    .iter()
                    .map(|sample| sample.work_counters[9])
                    .collect(),
            ),
            (
                "tail_selected_clone_bytes",
                samples
                    .iter()
                    .map(|sample| sample.work_counters[10])
                    .collect(),
            ),
            (
                "index_custom_selected_clone_bytes",
                samples
                    .iter()
                    .map(|sample| sample.work_counters[11])
                    .collect(),
            ),
            (
                "index_tail_selected_clone_bytes",
                samples
                    .iter()
                    .map(|sample| sample.work_counters[12])
                    .collect(),
            ),
            // ActionIdLookup::build clones only action.action Strings and
            // retains the original catalog Arc; this is a structural zero,
            // not an intercepted Action::clone counter.
            ("id_full_actions_cloned", vec![0; samples.len()]),
            (
                "id_keys_cloned",
                track_c1e_counter_values(receipts, Counter::IdKeysCloned),
            ),
            (
                "id_key_selected_clone_bytes",
                samples
                    .iter()
                    .map(|sample| sample.work_counters[13])
                    .collect(),
            ),
        ]);
        assert_eq!(arrays.len(), 46);
        assert!(
            arrays
                .iter()
                .all(|(_, values)| values.len() == MEASURED_SAMPLES)
        );
        let mut array_text = String::new();
        for (name, values) in arrays {
            let encoded = values
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(",");
            use std::fmt::Write as _;
            write!(&mut array_text, " {name}=[{encoded}]").expect("format result field");
        }
        let is_index = fixture.case == TrackC1ECase::IndexAcceptedChanged;
        eprintln!(
            "C1F_ACTIONS_RESULT schema=1 stage=id_position_projection owner={} source={} profile={} mode={} telemetry_enabled={} observer_enabled=true fixture=custom_first_indexed_tail fixture_signature={:016x} output_signature={:016x} case={} count={} custom_count={} indexed_count={} warmups={} samples={} exclusions=0 scope=owner_includes_sync_publication overlap=nested_durations_not_subtracted outcome_scope=typed_load_0_not_called_1_valid_2_malformed_3_missing payload_scope=precomputed_logical_utf8_not_allocator_or_rss index_tail_scope=cloned_iterator_consumption{}",
            if is_index {
                "process_index_ready_accepted"
            } else {
                "process_watch_events_actions"
            },
            source,
            profile,
            if mode == crate::performance::track_c_workloads::BenchmarkMode::Small {
                "small"
            } else {
                "full"
            },
            telemetry_enabled,
            fixture.fixture_signature,
            samples[0].output_identity,
            fixture.case.token(),
            count,
            custom_count,
            indexed_count,
            WARMUPS,
            MEASURED_SAMPLES,
            array_text,
        );
    }

    #[test]
    fn track_c_oracle_action_publication_preserves_complete_consumers() {
        for case in [
            TrackC1ECase::CustomChanged,
            TrackC1ECase::CustomEqual,
            TrackC1ECase::CustomMalformed,
            TrackC1ECase::CustomMissing,
            TrackC1ECase::IndexAcceptedChanged,
        ] {
            let fixture = track_c1e_fixture(100, case);
            let mut state = make_track_c1e_owner_state(&fixture);
            // Setup completion is acknowledged before this owner call; an
            // accepted index remains the coordinator's original envelope.
            assert_eq!(state.owner.app.rx.queued_depth(), Some(1));
            state.owner.app.process_watch_events();
            let sample = validate_track_c1e_owner(&mut state);
            assert_eq!(sample.observation.work_units, 100);
            let current_indexed = &state.expected_tail[0];
            let current_pin =
                track_c1e_fixture_pin(&current_indexed.action, current_indexed.args.clone(), 20);
            let missing_pin =
                track_c1e_fixture_pin("synthetic:removed-history-action", Some(String::new()), 10);
            let history_rows =
                crate::dashboard::widgets::CommandHistoryWidget::prepare_pinned_entries_for_test(
                    vec![current_pin, missing_pin.clone()],
                    &state.owner.app.plugins,
                    state.owner.app.enabled_plugins.as_ref(),
                    &state.owner.app.actions_by_id,
                    &state.owner.app.dashboard_data_cache,
                );
            assert_eq!(
                history_rows,
                vec![
                    (current_indexed.clone(), false),
                    (
                        crate::actions::Action {
                            label: missing_pin.label.clone(),
                            desc: missing_pin.desc.clone(),
                            action: missing_pin.action_id.clone(),
                            args: missing_pin.args.clone(),
                        },
                        true,
                    ),
                ],
                "history resolves a current indexed id and preserves a missing pin as fallback"
            );
            drop(state);
            drop(fixture);
        }
    }

    fn assert_track_c1e_transition_consumers(
        app: &mut LauncherApp,
        expected_actions: &[crate::actions::Action],
        custom_len: usize,
        query: &str,
        expected_results: &[crate::actions::Action],
        expected_version: u64,
    ) {
        let expected_by_id = track_c_expected_actions_by_id(expected_actions);
        assert_eq!(app.actions.as_slice(), expected_actions);
        assert_eq!(app.custom_len, custom_len);
        assert_eq!(&app.actions[..custom_len], &expected_actions[..custom_len]);
        assert_eq!(&app.actions[custom_len..], &expected_actions[custom_len..]);
        assert_eq!(
            app.action_cache,
            expected_actions
                .iter()
                .map(track_c_expected_cached_search_entry)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            app.action_filter_metadata,
            expected_actions
                .iter()
                .map(track_c_expected_filter_metadata)
                .collect::<Vec<_>>()
        );
        track_c_assert_action_lookup_matches(&app.actions_by_id, expected_actions);
        assert_eq!(app.query, query);
        assert_eq!(app.last_search_query, query);
        assert_eq!(app.results, expected_results);
        assert!(app.last_results_valid);
        assert!(!app.last_search_pending);
        assert!(!app.background_query_refresh_pending);
        assert_eq!(app.selected, None);
        assert_eq!(crate::actions::actions_version(), expected_version);

        if app.completion_index.is_none() {
            assert!(app.action_completion_dirty);
            assert!(app.suggestions.is_empty());
            app.rebuild_completion_index_now();
        }
        let expected_keys = track_c1e_expected_completion_keys(app, expected_actions);
        assert_eq!(track_c1e_completion_keys(app), expected_keys);
        assert!(!app.action_completion_dirty);
        assert_eq!(
            app.suggestions,
            track_c1e_expected_suggestions(&expected_keys, query, 5)
        );

        let radial = app.cached_universal_action_catalog_snapshot();
        assert_eq!(
            radial
                .entries
                .iter()
                .take(expected_actions.len())
                .map(|entry| entry.selected_action.clone())
                .collect::<Vec<_>>(),
            expected_actions
        );
        drop(radial);

        let mut pins = Vec::new();
        if let Some(indexed) = expected_actions.get(custom_len) {
            pins.push(track_c1e_fixture_pin(
                &indexed.action,
                indexed.args.clone(),
                30,
            ));
        }
        if custom_len > 3 {
            let custom = &expected_actions[3];
            pins.push(track_c1e_fixture_pin(
                &custom.action,
                custom.args.clone(),
                20,
            ));
        }
        pins.push(track_c1e_fixture_pin(
            "synthetic:removed-history-action",
            Some(String::new()),
            10,
        ));
        let history_rows =
            crate::dashboard::widgets::CommandHistoryWidget::prepare_pinned_entries_for_test(
                pins.clone(),
                &app.plugins,
                app.enabled_plugins.as_ref(),
                &app.actions_by_id,
                &app.dashboard_data_cache,
            );
        let expected_history_rows = pins
            .iter()
            .map(|pin| {
                if let Some(action) = expected_by_id.get(&pin.action_id) {
                    (action.clone(), false)
                } else {
                    (
                        crate::actions::Action {
                            label: pin.label.clone(),
                            desc: pin.desc.clone(),
                            action: pin.action_id.clone(),
                            args: pin.args.clone(),
                        },
                        true,
                    )
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(history_rows, expected_history_rows);
    }

    #[test]
    fn track_c_oracle_local_equal_empty_prefix_and_recovery_publication() {
        let fixture = track_c1e_fixture(100, TrackC1ECase::CustomEqual);
        let mut state = make_track_c1e_owner_state(&fixture);

        // The queued watcher notification is equal and must retain the app's
        // original catalog Arc and generation.
        assert_eq!(state.owner.app.rx.queued_depth(), Some(1));
        state.owner.app.process_watch_events();
        let equal_watcher = validate_track_c1e_owner(&mut state);
        assert_eq!(equal_watcher.observation.work_units, 100);
        assert_eq!(state.owner.app.rx.queued_depth(), Some(0));

        // A local durable save deliberately publishes even when its custom
        // payload is identical; the actions transaction owns the version bump.
        let watcher_equal_pointer = Arc::as_ptr(&state.owner.app.actions) as usize;
        let root_generation_before_local_equal = state
            .owner
            .app
            .root_list_geometry
            .result_generation_for_test();
        let version_before_local_equal = crate::actions::actions_version();
        state
            .owner
            .app
            .update_custom_actions(|actions| {
                assert_eq!(actions.as_slice(), fixture.expected_custom.as_slice());
                Ok(())
            })
            .expect("equal local custom save succeeds in the owned workspace");
        let version_after_local_equal = crate::actions::actions_version();
        assert_eq!(version_after_local_equal, version_before_local_equal + 1);
        assert_eq!(
            state
                .owner
                .app
                .root_list_geometry
                .result_generation_for_test(),
            root_generation_before_local_equal + 1
        );
        assert_ne!(
            Arc::as_ptr(&state.owner.app.actions) as usize,
            watcher_equal_pointer,
            "a durable equal local save still republishes the catalog"
        );
        assert_track_c_actions_owner_state(
            &state.owner,
            &fixture.expected_actions,
            version_after_local_equal,
        );
        assert_track_c1e_transition_consumers(
            &mut state.owner.app,
            &fixture.expected_actions,
            fixture.expected_custom.len(),
            "app Synthetic action 00009",
            &fixture.expected_actions[9..10],
            version_after_local_equal,
        );
        assert!(matches!(
            crate::actions::load_actions_typed(&state.owner.actions_path)
                .expect("read the owned persisted custom prefix"),
            crate::common::persistence::LoadState::Loaded(actions)
                if actions == fixture.expected_custom
        ));

        // Once that save is on disk, an equal watcher notification must keep
        // the newly published Arc and the version produced by the local save.
        let local_equal_pointer = Arc::as_ptr(&state.owner.app.actions) as usize;
        state
            .owner
            .app
            .event_tx
            .with_origin(EventOrigin::FileWatcher)
            .send(WatchEvent::Actions)
            .expect("queue the equal watcher notification");
        state.owner.app.process_watch_events();
        assert_eq!(crate::actions::actions_version(), version_after_local_equal);
        assert_eq!(
            Arc::as_ptr(&state.owner.app.actions) as usize,
            local_equal_pointer,
            "an equal watcher reload retains the local-save catalog Arc"
        );
        assert_eq!(
            state
                .owner
                .app
                .root_list_geometry
                .result_generation_for_test(),
            root_generation_before_local_equal + 1
        );
        assert_track_c_actions_owner_state(
            &state.owner,
            &fixture.expected_actions,
            version_after_local_equal,
        );
        assert_track_c1e_transition_consumers(
            &mut state.owner.app,
            &fixture.expected_actions,
            fixture.expected_custom.len(),
            "app Synthetic action 00009",
            &fixture.expected_actions[9..10],
            version_after_local_equal,
        );

        // Empty custom files retain the indexed tail, and a later valid local
        // save restores the custom prefix through the same publication owner.
        let empty_query = "app __track_c1e_no_such_action__";
        state.owner.app.query = empty_query.into();
        let root_generation_before_empty = state
            .owner
            .app
            .root_list_geometry
            .result_generation_for_test();
        let before_empty_pointer = Arc::as_ptr(&state.owner.app.actions) as usize;
        let version_before_empty = crate::actions::actions_version();
        state
            .owner
            .app
            .update_custom_actions(|actions| {
                actions.clear();
                Ok(())
            })
            .expect("empty custom prefix persists in the owned workspace");
        let version_after_empty = crate::actions::actions_version();
        assert_eq!(version_after_empty, version_before_empty + 1);
        assert_ne!(
            Arc::as_ptr(&state.owner.app.actions) as usize,
            before_empty_pointer
        );
        assert_eq!(
            state
                .owner
                .app
                .root_list_geometry
                .result_generation_for_test(),
            root_generation_before_empty + 1
        );
        assert!(matches!(
            crate::actions::load_actions_typed(&state.owner.actions_path)
                .expect("read the owned empty custom prefix"),
            crate::common::persistence::LoadState::Loaded(actions) if actions.is_empty()
        ));
        assert_track_c1e_transition_consumers(
            &mut state.owner.app,
            &fixture.expected_tail,
            0,
            empty_query,
            &[],
            version_after_empty,
        );

        state.owner.app.query = "app Synthetic action 00009".into();
        let root_generation_before_recovery = state
            .owner
            .app
            .root_list_geometry
            .result_generation_for_test();
        let app = &mut state.owner.app;
        let before_recovery_pointer = Arc::as_ptr(&app.actions) as usize;
        let version_before_recovery = crate::actions::actions_version();
        app.update_custom_actions(|actions| {
            *actions = fixture.expected_custom.clone();
            Ok(())
        })
        .expect("valid custom prefix recovers in the owned workspace");
        let version_after_recovery = crate::actions::actions_version();
        assert_eq!(version_after_recovery, version_before_recovery + 1);
        assert_eq!(
            app.root_list_geometry.result_generation_for_test(),
            root_generation_before_recovery + 1
        );
        assert_ne!(Arc::as_ptr(&app.actions) as usize, before_recovery_pointer);
        assert_track_c_actions_owner_state(
            &state.owner,
            &fixture.expected_actions,
            version_after_recovery,
        );
        assert_track_c1e_transition_consumers(
            &mut state.owner.app,
            &fixture.expected_actions,
            fixture.expected_custom.len(),
            "app Synthetic action 00009",
            &fixture.expected_actions[9..10],
            version_after_recovery,
        );
        assert!(matches!(
            crate::actions::load_actions_typed(&state.owner.actions_path)
                .expect("read the recovered owned custom prefix"),
            crate::common::persistence::LoadState::Loaded(actions)
                if actions == fixture.expected_custom
        ));
        drop(state);
        drop(fixture);
    }

    #[test]
    #[ignore = "opt-in C1-F action ID-position characterization; parent runs isolated fast-dev Small/Full processes"]
    fn track_c_c1f_benchmark_action_publication_attribution() {
        use crate::performance::track_c_workloads::{
            BenchmarkMode, MEASURED_SAMPLES, WARMUPS, measure_owner,
        };

        assert!(crate::performance::enabled());
        let mode = BenchmarkMode::from_process_env().expect("valid Track C benchmark mode");
        let total_counts: &[usize] = match mode {
            BenchmarkMode::Small => &[100],
            BenchmarkMode::Full => &[100, 1_000, 10_000, 20_000],
        };
        for total_count in total_counts {
            for case in [
                TrackC1ECase::CustomChanged,
                TrackC1ECase::CustomEqual,
                TrackC1ECase::CustomMalformed,
                TrackC1ECase::CustomMissing,
                TrackC1ECase::IndexAcceptedChanged,
            ] {
                let fixture = track_c1e_fixture(*total_count, case);
                let mut validations = Vec::with_capacity(WARMUPS + MEASURED_SAMPLES);
                let samples = measure_owner(
                    || make_track_c1e_owner_state(&fixture),
                    |state| {
                        state.owner.app.process_watch_events();
                        crate::actions::actions_version()
                    },
                    |state, _version_after| {
                        let validated = validate_track_c1e_owner(state);
                        let observation = validated.observation.clone();
                        validations.push(validated);
                        observation
                    },
                );
                assert_eq!(samples.len(), MEASURED_SAMPLES);
                assert_eq!(validations.len(), WARMUPS + MEASURED_SAMPLES);
                let measured = validations.into_iter().skip(WARMUPS).collect::<Vec<_>>();
                emit_track_c1f_action_results(mode, &fixture, &samples, &measured);
            }
        }
    }

    fn make_track_c_actions_owner_state(fixture: &TrackCActionsFixture) -> TrackCActionsOwnerState {
        make_track_c_actions_owner_state_inner(fixture, None).0
    }

    fn make_track_c_actions_owner_state_with_recorder(
        fixture: &TrackCActionsFixture,
        recorder_capacity: usize,
    ) -> (
        TrackCActionsOwnerState,
        super::event_channel::EventAgeRecorder,
        std::sync::MutexGuard<'static, ()>,
    ) {
        let registry_guard = TRACK_C_EVENT_REGISTRY_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (state, recorder) =
            make_track_c_actions_owner_state_inner(fixture, Some(recorder_capacity));
        (
            state,
            recorder.expect("observed queue setup creates its age recorder"),
            registry_guard,
        )
    }

    fn install_track_c_observed_channel(
        app: &mut LauncherApp,
        recorder_capacity: usize,
    ) -> super::event_channel::EventAgeRecorder {
        let (sender, receiver, recorder) =
            super::event_channel::channel_with_test_age_recorder(recorder_capacity);
        let registration = crate::gui::register_observed_event_sender(
            sender.clone(),
            ViewportWake::root(&app.egui_ctx),
        );
        let previous_registration = std::mem::replace(&mut app.event_sink, registration);
        drop(previous_registration);
        let previous_sender = std::mem::replace(&mut app.event_tx, sender);
        let previous_receiver = std::mem::replace(&mut app.rx, receiver);
        drop(previous_sender);
        drop(previous_receiver);
        recorder
    }

    #[test]
    fn track_c_observed_channel_replaces_only_its_own_registry_sink() {
        let _registry_guard = TRACK_C_EVENT_REGISTRY_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let fixture = track_c_actions_fixture(24);
        let mut state = make_track_c_actions_owner_state(&fixture);
        let (unrelated_tx, unrelated_rx) = channel();
        let unrelated_registration = crate::gui::register_event_sender(unrelated_tx);
        let old_owner_registration_id = state.app.event_sink.id;

        let registry_snapshot = || {
            let registry = crate::gui::APP_EVENT_REGISTRY
                .lock()
                .expect("lock app event registry for assertion");
            let ids = registry
                .sinks
                .iter()
                .map(|sink| sink.id)
                .collect::<Vec<_>>();
            (
                ids,
                registry.pending_before_owner.len(),
                registry.owner_registered,
            )
        };
        let (before_ids, before_pending, before_owner_registered) = registry_snapshot();
        let mut expected_ids = before_ids
            .into_iter()
            .filter(|id| *id != old_owner_registration_id)
            .collect::<std::collections::HashSet<_>>();
        assert!(expected_ids.contains(&unrelated_registration.id));

        let _recorder = install_track_c_observed_channel(&mut state.app, 4);
        let new_owner_registration_id = state.app.event_sink.id;
        assert_ne!(new_owner_registration_id, old_owner_registration_id);
        expected_ids.insert(new_owner_registration_id);
        let (after_ids, after_pending, after_owner_registered) = registry_snapshot();
        assert_eq!(
            after_ids
                .into_iter()
                .collect::<std::collections::HashSet<_>>(),
            expected_ids,
            "only the app's old registration is replaced"
        );
        assert_eq!(
            after_pending, before_pending,
            "pre-owner events are retained"
        );
        assert_eq!(
            after_owner_registered, before_owner_registered,
            "replacing a registered app does not reset owner history"
        );

        crate::gui::send_event(WatchEvent::IndexReady);
        assert!(matches!(
            unrelated_rx.try_recv(),
            Ok(WatchEvent::IndexReady)
        ));
        state.app.process_watch_events();
        assert!(state.app.rx.try_recv().is_err());
        drop(unrelated_registration);
        drop(state);
        drop(fixture);
    }

    fn wait_for_track_c_successful_enqueue(
        app: &LauncherApp,
        recorder: &super::event_channel::EventAgeRecorder,
        successful_sends: usize,
        expected_depth: usize,
    ) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if recorder.successful_sends() >= successful_sends {
                assert_eq!(
                    app.rx.queued_depth(),
                    Some(expected_depth),
                    "the successful-send acknowledgment precedes queue-depth observation"
                );
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "event send did not commit number {successful_sends}"
            );
            std::thread::yield_now();
        }
    }

    fn make_track_c_watch_queue_owner_state(
        fixture: &TrackCActionsFixture,
        recorder_capacity: usize,
        extra_actions_events: usize,
        recycle_events: usize,
    ) -> TrackCWatchQueueOwnerState {
        let (owner, age_recorder, registry_guard) =
            make_track_c_actions_owner_state_with_recorder(fixture, recorder_capacity);
        let event_tx = owner.app.event_tx.clone();
        for _ in 0..extra_actions_events {
            event_tx
                .with_origin(EventOrigin::FileWatcher)
                .send(WatchEvent::Actions)
                .expect("enqueue finite Actions burst");
        }
        for _ in 0..recycle_events {
            event_tx
                .with_origin(EventOrigin::Registry)
                .send(WatchEvent::Recycle(Ok(())))
                .expect("enqueue recycle completion receipt without invoking cleanup");
        }

        let expected_actions_events = extra_actions_events + 1;
        let expected_event_count = expected_actions_events + recycle_events;
        assert!(recorder_capacity >= expected_event_count);
        assert_eq!(owner.app.rx.queued_depth(), Some(expected_event_count));
        crate::performance::track_c::reset();

        let mut expected_metadata = vec![
            (
                crate::performance::track_c::EventClass::FileReload,
                EventOrigin::FileWatcher,
            );
            expected_actions_events
        ];
        expected_metadata.extend(std::iter::repeat_n(
            (
                crate::performance::track_c::EventClass::Clipboard,
                EventOrigin::Registry,
            ),
            recycle_events,
        ));

        TrackCWatchQueueOwnerState {
            owner,
            age_recorder,
            recorder_capacity,
            expected_actions_events,
            expected_index_ready_events: 0,
            expected_recycle_events: recycle_events,
            expected_event_count,
            expected_metadata,
            _registry_guard: registry_guard,
        }
    }

    fn expected_track_c_watch_queue_metadata(
        state: &TrackCWatchQueueOwnerState,
    ) -> Vec<(
        crate::performance::track_c::EventClass,
        crate::performance::track_c::EventOrigin,
    )> {
        state.expected_metadata.clone()
    }

    fn make_track_c_stale_index_watch_queue_owner_state(
        fixture: &TrackCActionsFixture,
        recorder_capacity: usize,
        extra_actions_events: usize,
        recycle_events: usize,
    ) -> TrackCStaleIndexQueueOwnerState {
        use crate::indexer::coordinator::{IndexConfig, IndexCoordinator};

        let mut queue = make_track_c_watch_queue_owner_state(fixture, recorder_capacity, 0, 0);
        let (candidate_finished_tx, candidate_finished_rx) = channel();
        let (replacement_started_tx, replacement_started_rx) = channel();
        let (release_replacement_tx, release_replacement_rx) = channel();
        let release_replacement_rx = Arc::new(Mutex::new(release_replacement_rx));
        let unchanged_tail = queue.owner.indexed_tail.clone();
        let mut stale_candidate = unchanged_tail.clone();
        stale_candidate[0]
            .desc
            .push_str("; stale queue candidate must not publish");
        let candidate_tail = stale_candidate;
        let coordinator = IndexCoordinator::with_test_scanner(move |config| {
            match config.roots().first().map(String::as_str) {
                Some("queue-candidate") => {
                    candidate_finished_tx.send(()).unwrap();
                    Ok(candidate_tail.clone())
                }
                Some("queue-blocked-replacement") => {
                    replacement_started_tx.send(()).unwrap();
                    release_replacement_rx
                        .lock()
                        .unwrap()
                        .recv_timeout(std::time::Duration::from_secs(5))
                        .map_err(|error| format!("queue replacement release failed: {error}"))?;
                    Ok(unchanged_tail.clone())
                }
                other => Err(format!("unexpected Track C queue scan config: {other:?}")),
            }
        })
        .expect("construct gated queue coordinator");
        queue.owner.app.install_test_index_coordinator(coordinator);

        let indexed_count = queue.owner.indexed_tail.len();
        let config = |root: &str| IndexConfig::new(vec![root.to_owned()], Some(indexed_count));
        queue
            .owner
            .app
            .request_index_config(config("queue-candidate"));
        let candidate_generation = queue.owner.app.indexing.expected_generation.unwrap();
        candidate_finished_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("coordinator completed the original candidate scan");
        queue
            .owner
            .app
            .wait_for_index_completion_for_test(candidate_generation)
            .expect("candidate scan completion is available");
        wait_for_track_c_successful_enqueue(&queue.owner.app, &queue.age_recorder, 2, 2);

        // The candidate's original IndexReady envelope stays in the app queue.
        // Submitting B makes that real envelope stale while B is held at its
        // scanner gate; no receive/re-enqueue step changes its timestamp.
        queue
            .owner
            .app
            .request_index_config(config("queue-blocked-replacement"));
        let blocked_generation = queue.owner.app.indexing.expected_generation.unwrap();
        replacement_started_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("replacement scan reached its gate");

        for _ in 0..extra_actions_events {
            queue
                .owner
                .app
                .event_tx
                .with_origin(EventOrigin::FileWatcher)
                .send(WatchEvent::Actions)
                .expect("enqueue finite Actions duplicate after original index envelope");
        }
        for _ in 0..recycle_events {
            queue
                .owner
                .app
                .event_tx
                .with_origin(EventOrigin::Registry)
                .send(WatchEvent::Recycle(Ok(())))
                .expect("enqueue toast-disabled completion after stale index envelope");
        }

        let recorder_capacity_required = 1 + 1 + extra_actions_events + recycle_events + 1; // B's completion is drained outside the measured receipt.
        assert!(recorder_capacity >= recorder_capacity_required);
        queue.expected_actions_events = 1 + extra_actions_events;
        queue.expected_index_ready_events = 1;
        queue.expected_recycle_events = recycle_events;
        queue.expected_event_count =
            queue.expected_actions_events + queue.expected_index_ready_events + recycle_events;
        queue.expected_metadata = vec![
            (
                crate::performance::track_c::EventClass::FileReload,
                EventOrigin::FileWatcher,
            ),
            (
                crate::performance::track_c::EventClass::IndexCompletion,
                EventOrigin::IndexCoordinator,
            ),
        ];
        queue.expected_metadata.extend(std::iter::repeat_n(
            (
                crate::performance::track_c::EventClass::FileReload,
                EventOrigin::FileWatcher,
            ),
            extra_actions_events,
        ));
        queue.expected_metadata.extend(std::iter::repeat_n(
            (
                crate::performance::track_c::EventClass::Clipboard,
                EventOrigin::Registry,
            ),
            recycle_events,
        ));
        assert_eq!(queue.expected_metadata.len(), queue.expected_event_count);
        wait_for_track_c_successful_enqueue(
            &queue.owner.app,
            &queue.age_recorder,
            queue.expected_event_count,
            queue.expected_event_count,
        );
        assert_eq!(
            queue.owner.app.indexing.expected_generation,
            Some(blocked_generation),
            "the queued candidate completion is stale under the blocked generation"
        );
        crate::performance::track_c::reset();

        TrackCStaleIndexQueueOwnerState {
            queue,
            release_blocked_scan: Some(release_replacement_tx),
            blocked_generation,
        }
    }

    fn finish_track_c_stale_index_watch_queue_owner(state: &mut TrackCStaleIndexQueueOwnerState) {
        let queue = &mut state.queue;
        let release = state
            .release_blocked_scan
            .take()
            .expect("blocked scan release is consumed exactly once");
        release
            .send(())
            .expect("blocked replacement scanner remains available");
        queue
            .owner
            .app
            .wait_for_index_completion_for_test(state.blocked_generation)
            .expect("replacement scan completes after owner timing");
        wait_for_track_c_successful_enqueue(
            &queue.owner.app,
            &queue.age_recorder,
            queue.expected_event_count + 1,
            1,
        );

        let actions_before_equal_completion = Arc::clone(&queue.owner.app.actions);
        let version_before_equal_completion = crate::actions::actions_version();
        queue.owner.app.process_watch_events();
        assert!(Arc::ptr_eq(
            &queue.owner.app.actions,
            &actions_before_equal_completion
        ));
        assert_eq!(
            crate::actions::actions_version(),
            version_before_equal_completion,
            "equal replacement completion preserves the accepted catalog"
        );
        assert_eq!(queue.owner.app.rx.queued_depth(), Some(0));
        assert_track_c_actions_owner_state(
            &queue.owner,
            &queue.owner.expected_actions,
            queue.owner.actions_version_before + 1,
        );
        let all_delivered = queue.age_recorder.snapshot();
        assert_eq!(
            all_delivered.delivered.len(),
            queue.expected_event_count + 1
        );
        assert_eq!(all_delivered.overflow, 0);
        assert_eq!(all_delivered.abandoned, 0);
        let mut expected_all_metadata = expected_track_c_watch_queue_metadata(queue);
        expected_all_metadata.push((
            crate::performance::track_c::EventClass::IndexCompletion,
            EventOrigin::IndexCoordinator,
        ));
        assert_eq!(
            all_delivered
                .delivered
                .iter()
                .map(|event| (event.class, event.origin))
                .collect::<Vec<_>>(),
            expected_all_metadata,
            "cleanup consumes B's original coordinator notification outside the timed receipt"
        );
    }

    fn validate_track_c_watch_queue_owner(
        state: &TrackCWatchQueueOwnerState,
    ) -> (
        crate::performance::track_c_workloads::OwnerObservation,
        TrackCWatchQueueSampleReceipt,
    ) {
        use crate::performance::track_c::{self, EventClass, EventOrigin, Phase};

        assert_eq!(state.owner.app.rx.queued_depth(), Some(0));
        let actions_version_after = crate::actions::actions_version();
        let mut observation = validate_track_c_actions_owner(&state.owner, actions_version_after);
        let delivered = state.age_recorder.snapshot();
        assert!(delivered.delivered.len() <= state.recorder_capacity);
        assert_eq!(delivered.delivered.len(), state.expected_event_count);
        assert_eq!(delivered.overflow, 0, "event age recorder did not overflow");
        assert_eq!(
            delivered.abandoned, 0,
            "the bounded queue sample has no abandoned event envelopes"
        );
        let expected_metadata = expected_track_c_watch_queue_metadata(state);
        assert_eq!(
            delivered
                .delivered
                .iter()
                .map(|sample| (sample.class, sample.origin))
                .collect::<Vec<_>>(),
            expected_metadata,
            "one producer preserves the event sequence and static attribution"
        );

        let drain = track_c::snapshot()[Phase::EventDrain as usize].1;
        let mut handlers = Vec::new();
        if crate::performance::enabled() {
            assert!(drain.calls > 0, "the real event reducer was measured");
            assert_eq!(drain.completed, drain.calls);
            assert_eq!(drain.errors, 0);
            assert_eq!(drain.abandoned, 0);
            assert_eq!(drain.work_units, state.expected_event_count as u64);

            let event_snapshots = track_c::snapshot_events();
            for (class, origin) in [
                (EventClass::FileReload, EventOrigin::FileWatcher),
                (EventClass::IndexCompletion, EventOrigin::IndexCoordinator),
                (EventClass::Clipboard, EventOrigin::Registry),
            ] {
                let count = expected_metadata
                    .iter()
                    .filter(|pair| **pair == (class, origin))
                    .count() as u64;
                if count == 0 {
                    continue;
                }
                let event = event_snapshots
                    .iter()
                    .find(|snapshot| snapshot.class == class && snapshot.origin == origin)
                    .copied()
                    .expect("event class/origin pair exists in fixed collector");
                assert_eq!(event.dequeued, count);
                assert_eq!(event.send_failed, 0);
                assert_eq!(event.abandoned, 0);
                assert_eq!(event.handler_calls, count);
                handlers.push(event);
            }
            assert_eq!(
                event_snapshots
                    .iter()
                    .filter(|event| event.dequeued != 0 || event.handler_calls != 0)
                    .count(),
                handlers.len(),
                "no other event class/origin was handled in this owner"
            );
        }

        observation.work_units = state.expected_event_count as u64;
        observation.work_counters = vec![
            state.expected_event_count as u64,
            state.expected_actions_events as u64,
            state.expected_index_ready_events as u64,
            state.expected_recycle_events as u64,
            state.owner.custom_len as u64,
            state.owner.indexed_tail.len() as u64,
            state.owner.app.actions.len() as u64,
            state.owner.app.action_cache.len() as u64,
            state.owner.app.action_filter_metadata.len() as u64,
            state.owner.app.actions_by_id.len() as u64,
            state.owner.app.results.len() as u64,
        ];
        (
            observation,
            TrackCWatchQueueSampleReceipt {
                delivered,
                drain,
                handlers,
            },
        )
    }

    fn set_track_c_concurrent_queue_counts(
        state: &mut TrackCWatchQueueOwnerState,
        producer_actions: usize,
        producer_recycle: usize,
    ) {
        state.expected_actions_events = 1 + producer_actions;
        state.expected_index_ready_events = 0;
        state.expected_recycle_events = producer_recycle;
        state.expected_event_count = state.expected_actions_events + producer_recycle;
        state.expected_metadata = vec![
            (
                crate::performance::track_c::EventClass::FileReload,
                EventOrigin::FileWatcher,
            );
            state.expected_actions_events
        ];
        state.expected_metadata.extend(std::iter::repeat_n(
            (
                crate::performance::track_c::EventClass::Clipboard,
                EventOrigin::Registry,
            ),
            producer_recycle,
        ));
        assert!(state.expected_event_count <= state.recorder_capacity);
    }

    fn run_track_c_concurrent_queue_owner(
        state: &mut TrackCWatchQueueOwnerState,
        producer_actions: usize,
        producer_recycle: usize,
    ) {
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let producer_barrier = Arc::clone(&barrier);
        let producer_tx = state.owner.app.event_tx.clone();
        std::thread::scope(|scope| {
            scope.spawn(move || {
                producer_barrier.wait();
                for _ in 0..producer_actions {
                    producer_tx
                        .with_origin(EventOrigin::FileWatcher)
                        .send(WatchEvent::Actions)
                        .expect("finite concurrent Actions send succeeds");
                }
                for _ in 0..producer_recycle {
                    producer_tx
                        .with_origin(EventOrigin::Registry)
                        .send(WatchEvent::Recycle(Ok(())))
                        .expect("finite concurrent completion receipt succeeds");
                }
            });
            barrier.wait();
            state.owner.app.process_watch_events();
        });
        if state.owner.app.rx.queued_depth().unwrap_or_default() > 0 {
            state.owner.app.process_watch_events();
        }
        assert_eq!(state.owner.app.rx.queued_depth(), Some(0));
        assert_eq!(
            state.age_recorder.successful_sends(),
            state.expected_event_count,
            "all offered events committed before final validation"
        );
    }

    fn track_c_watch_queue_fixture_signature(
        fixture: &TrackCActionsFixture,
        recorder_capacity: usize,
        actions_events: usize,
        index_ready_events: usize,
        recycle_events: usize,
        concurrent_producer: bool,
    ) -> u64 {
        let mut signature = crate::performance::track_c_workloads::StableSignature::new(
            0,
            "track-c-watch-queue-fixture",
            fixture.expected_actions.len(),
        );
        signature.number(fixture.fixture_signature);
        signature.number(recorder_capacity as u64);
        signature.number(actions_events as u64);
        signature.number(index_ready_events as u64);
        signature.number(recycle_events as u64);
        signature.number(concurrent_producer as u64);
        signature.bytes(b"unbounded-app-mpsc;fixed-capacity-test-age-recorder");
        if index_ready_events == 0 {
            signature.bytes(b"FileWatcher:Actions;Registry:RecycleOk;no-index-ready");
        } else {
            signature.bytes(
                b"FileWatcher:Actions;IndexCoordinator:original-stale-IndexReady(candidate-tail-differs);Registry:RecycleOk;blocked-B-unchanged-tail-cleanup-outside-interval",
            );
        }
        signature.finish()
    }

    fn emit_track_c_watch_event_age_rows(
        metadata: crate::performance::track_c_workloads::ReportMetadata,
        recorder_capacity: usize,
        samples: &[crate::performance::track_c_workloads::SampleRecord],
        receipts: &[TrackCWatchQueueSampleReceipt],
    ) {
        use crate::performance::track_c_workloads::{MEASURED_SAMPLES, nearest_rank};

        assert_eq!(samples.len(), MEASURED_SAMPLES);
        assert_eq!(receipts.len(), MEASURED_SAMPLES);
        let source =
            std::env::var("ML_TRACK_C_SOURCE_SHA").unwrap_or_else(|_| "UNCOMMITTED_SOURCE".into());
        let profile =
            std::env::var("ML_TRACK_C_PROFILE").unwrap_or_else(|_| "unspecified-profile".into());
        let telemetry_enabled = crate::performance::enabled();
        for (sample_index, (sample, receipt)) in samples.iter().zip(receipts).enumerate() {
            let ages = receipt
                .delivered
                .delivered
                .iter()
                .map(|event| event.age_nanos)
                .collect::<Vec<_>>();
            let age_text = ages.iter().map(u64::to_string).collect::<Vec<_>>();
            let classes = receipt
                .delivered
                .delivered
                .iter()
                .map(|event| format!("{:?}", event.class))
                .collect::<Vec<_>>();
            let origins = receipt
                .delivered
                .delivered
                .iter()
                .map(|event| format!("{:?}", event.origin))
                .collect::<Vec<_>>();
            let handler_receipts = receipt
                .handlers
                .iter()
                .map(|event| {
                    format!(
                        "{:?}.{:?}:{}:{}:{}",
                        event.class,
                        event.origin,
                        event.handler_calls,
                        event.handler_nanos_total,
                        event.handler_nanos_max
                    )
                })
                .collect::<Vec<_>>();
            eprintln!(
                "TRACK_C_EVENT_AGES schema=1 owner={} source={} profile={} mode={:?} telemetry_enabled={} fixture={} fixture_signature={:016x} count={} cold_type={} recorder_capacity={} sample={} events={} age_ns=[{}] age_p50_ns={} age_p95_ns={} age_max_ns={} classes=[{}] origins=[{}] complete_output_state_id={:016x} structural_output_id={:016x} drain_calls={} drain_ns={} handler_receipts=[{}] overflow={} abandoned={}",
                metadata.owner,
                source,
                profile,
                metadata.mode,
                telemetry_enabled,
                metadata.fixture_name,
                metadata.fixture_signature,
                metadata.item_count,
                metadata.cold_type,
                recorder_capacity,
                sample_index,
                ages.len(),
                age_text.join(","),
                nearest_rank(&ages, 50),
                nearest_rank(&ages, 95),
                ages.iter().copied().max().unwrap_or_default(),
                classes.join(","),
                origins.join(","),
                sample.output_identity,
                sample.structural_signature,
                receipt.drain.calls,
                receipt.drain.elapsed_nanos_total,
                handler_receipts.join(","),
                receipt.delivered.overflow,
                receipt.delivered.abandoned,
            );
        }
    }

    #[test]
    fn track_c_oracle_original_index_ready_merges_latest_prefix_and_ignores_stale_wake() {
        use crate::indexer::coordinator::{IndexConfig, IndexCoordinator};

        let fixture = track_c_actions_fixture(24);
        let (mut state, age_recorder, _registry_guard) =
            make_track_c_actions_owner_state_with_recorder(&fixture, 16);
        let (latest_started_tx, latest_started_rx) = channel();
        let (release_latest_tx, release_latest_rx) = channel();
        let release_latest_rx = Arc::new(Mutex::new(release_latest_rx));
        let (replacement_started_tx, replacement_started_rx) = channel();
        let (release_replacement_tx, release_replacement_rx) = channel();
        let release_replacement_rx = Arc::new(Mutex::new(release_replacement_rx));

        struct ReleaseGates(Vec<std::sync::mpsc::Sender<()>>);
        impl Drop for ReleaseGates {
            fn drop(&mut self) {
                for sender in &self.0 {
                    let _ = sender.send(());
                }
            }
        }
        let _release_gates = ReleaseGates(vec![
            release_latest_tx.clone(),
            release_replacement_tx.clone(),
        ]);

        let mut latest_tail = state.indexed_tail.clone();
        latest_tail[0].desc.push_str("; latest blocked scan");
        let mut stale_tail = state.indexed_tail.clone();
        stale_tail[0].desc.push_str("; stale completed scan");
        let mut replacement_tail = state.indexed_tail.clone();
        replacement_tail[0]
            .desc
            .push_str("; replacement completed scan");

        let latest_for_scanner = latest_tail.clone();
        let stale_for_scanner = stale_tail.clone();
        let replacement_for_scanner = replacement_tail.clone();
        let coordinator = IndexCoordinator::with_test_scanner(move |config| {
            match config.roots().first().map(String::as_str) {
                Some("latest-blocked") => {
                    latest_started_tx.send(()).unwrap();
                    release_latest_rx
                        .lock()
                        .unwrap()
                        .recv_timeout(std::time::Duration::from_secs(5))
                        .map_err(|error| format!("latest scan release failed: {error}"))?;
                    Ok(latest_for_scanner.clone())
                }
                Some("stale-complete") => Ok(stale_for_scanner.clone()),
                Some("replacement-blocked") => {
                    replacement_started_tx.send(()).unwrap();
                    release_replacement_rx
                        .lock()
                        .unwrap()
                        .recv_timeout(std::time::Duration::from_secs(5))
                        .map_err(|error| format!("replacement scan release failed: {error}"))?;
                    Ok(replacement_for_scanner.clone())
                }
                other => Err(format!("unexpected Track C scan config: {other:?}")),
            }
        })
        .expect("construct gated test coordinator");
        state.app.install_test_index_coordinator(coordinator);

        let indexed_count = state.indexed_tail.len();
        let config = |root: &str| IndexConfig::new(vec![root.to_owned()], Some(indexed_count));
        let version_before = state.actions_version_before;

        // The first scan is held while the actual Actions event publishes the
        // latest custom prefix. Its later original IndexReady envelope must
        // merge the scan result with that prefix.
        state.app.request_index_config(config("latest-blocked"));
        let latest_generation = state.app.indexing.expected_generation.unwrap();
        latest_started_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("latest-prefix scan reached its gate");
        state.app.process_watch_events();
        state.expected_actions = fixture.replacement_custom.clone();
        state
            .expected_actions
            .extend(state.indexed_tail.iter().cloned());
        assert_track_c_actions_owner_state(&state, &state.expected_actions, version_before + 1);
        assert_eq!(state.app.rx.queued_depth(), Some(0));

        release_latest_tx
            .send(())
            .expect("release latest-prefix scan");
        state
            .app
            .wait_for_index_completion_for_test(latest_generation)
            .expect("latest-prefix scan completes");
        wait_for_track_c_successful_enqueue(&state.app, &age_recorder, 2, 1);
        assert_eq!(
            state.app.rx.queued_depth(),
            Some(1),
            "the coordinator's original notification remains queued"
        );
        state.app.process_watch_events();
        state.indexed_tail = latest_tail;
        state.expected_actions = fixture.replacement_custom.clone();
        state
            .expected_actions
            .extend(state.indexed_tail.iter().cloned());
        assert_track_c_actions_owner_state(&state, &state.expected_actions, version_before + 2);
        assert_eq!(state.app.rx.queued_depth(), Some(0));

        // Queue A's real notification, then submit B before reducing A. The
        // stale A wake is consumed through the ordinary event reducer while B
        // remains gated and cannot publish.
        state.app.request_index_config(config("stale-complete"));
        let stale_generation = state.app.indexing.expected_generation.unwrap();
        state
            .app
            .wait_for_index_completion_for_test(stale_generation)
            .expect("stale candidate scan completes");
        wait_for_track_c_successful_enqueue(&state.app, &age_recorder, 3, 1);
        state
            .app
            .request_index_config(config("replacement-blocked"));
        let replacement_generation = state.app.indexing.expected_generation.unwrap();
        replacement_started_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("replacement scan reached its gate");
        let retained_actions = Arc::clone(&state.app.actions);
        let retained_version = crate::actions::actions_version();
        state.app.process_watch_events();
        assert!(Arc::ptr_eq(&state.app.actions, &retained_actions));
        assert_track_c_actions_owner_state(&state, &state.expected_actions, retained_version);
        assert_eq!(retained_version, version_before + 2);
        assert_eq!(
            state.app.indexing.expected_generation,
            Some(replacement_generation)
        );
        assert_eq!(state.app.rx.queued_depth(), Some(0));

        release_replacement_tx
            .send(())
            .expect("release replacement scan");
        state
            .app
            .wait_for_index_completion_for_test(replacement_generation)
            .expect("replacement scan completes");
        wait_for_track_c_successful_enqueue(&state.app, &age_recorder, 4, 1);
        state.app.process_watch_events();
        state.indexed_tail = replacement_tail;
        state.expected_actions = fixture.replacement_custom.clone();
        state
            .expected_actions
            .extend(state.indexed_tail.iter().cloned());
        assert_track_c_actions_owner_state(&state, &state.expected_actions, version_before + 3);
        assert_eq!(state.app.rx.queued_depth(), Some(0));

        let delivered = age_recorder.snapshot();
        assert_eq!(delivered.overflow, 0);
        assert_eq!(delivered.abandoned, 0);
        assert_eq!(delivered.delivered.len(), 4);
        assert_eq!(
            delivered
                .delivered
                .iter()
                .map(|sample| (sample.class, sample.origin))
                .collect::<Vec<_>>(),
            [
                (
                    crate::performance::track_c::EventClass::FileReload,
                    EventOrigin::FileWatcher
                ),
                (
                    crate::performance::track_c::EventClass::IndexCompletion,
                    EventOrigin::IndexCoordinator,
                ),
                (
                    crate::performance::track_c::EventClass::IndexCompletion,
                    EventOrigin::IndexCoordinator,
                ),
                (
                    crate::performance::track_c::EventClass::IndexCompletion,
                    EventOrigin::IndexCoordinator,
                ),
            ]
        );
        assert_eq!(state.app.rx.queued_depth(), Some(0));
    }

    fn make_track_c_actions_owner_state_inner(
        fixture: &TrackCActionsFixture,
        recorder_capacity: Option<usize>,
    ) -> (
        TrackCActionsOwnerState,
        Option<super::event_channel::EventAgeRecorder>,
    ) {
        use crate::performance::track_c_workloads;

        let initial_payload = serde_json::to_vec(&fixture.initial_custom)
            .expect("serialize initial custom actions outside owner interval");
        let replacement_payload = serde_json::to_vec(&fixture.replacement_custom)
            .expect("serialize replacement custom actions outside owner interval");
        std::fs::write(&fixture.actions_path, initial_payload)
            .expect("write initial action fixture before app construction");

        let context = egui::Context::default();
        let mut settings = Settings::default();
        settings.enable_toasts = false;
        settings.show_inline_errors = false;
        settings.show_error_toasts = false;
        settings.dashboard.enabled = false;
        settings.hotkey = None;
        settings.quit_hotkey = None;
        settings.help_hotkey = None;
        settings.match_exact = true;
        settings.usage_weight = 0.0;
        settings.enabled_plugins = Some(std::collections::HashSet::new());
        settings.max_indexed_items = Some(fixture.indexed_tail.len());
        let mut initial_actions = fixture.initial_custom.clone();
        initial_actions.extend(fixture.indexed_tail.iter().cloned());
        let mut app = LauncherApp::new(
            &context,
            Arc::new(initial_actions),
            fixture.custom_len,
            PluginManager::new_inert_for_test(),
            fixture.actions_path.to_string_lossy().into_owned(),
            fixture.settings_path.to_string_lossy().into_owned(),
            settings,
            None,
            None,
            Some(std::collections::HashSet::new()),
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        app.watchers.clear();
        app.process_watch_events();
        assert!(app.rx.try_recv().is_err(), "startup events are quiescent");
        app.folder_aliases.clear();
        app.folder_aliases_lc.clear();
        app.bookmark_aliases.clear();
        app.bookmark_aliases_lc.clear();
        app.usage.clear();
        app.usage_weight = 0.0;
        app.fuzzy_weight = 1.0;
        app.query = "app Synthetic action 00009".into();
        app.match_exact = true;
        app.search();
        assert_eq!(app.results, vec![app.actions[9].clone()]);
        app.selected = Some(0);

        let age_recorder =
            recorder_capacity.map(|capacity| install_track_c_observed_channel(&mut app, capacity));
        if age_recorder.is_some() {
            crate::performance::track_c::reset();
        }

        std::fs::write(&fixture.actions_path, replacement_payload)
            .expect("write replacement payload before owner interval");
        app.event_tx
            .with_origin(EventOrigin::FileWatcher)
            .send(WatchEvent::Actions)
            .expect("enqueue one synthetic file-reload event");
        let actions_version_before = crate::actions::actions_version();

        (
            TrackCActionsOwnerState {
                app,
                expected_actions: fixture.expected_actions.clone(),
                indexed_tail: fixture.indexed_tail.clone(),
                index_root: fixture.index_root.clone(),
                actions_path: fixture.actions_path.clone(),
                custom_len: fixture.custom_len,
                actions_version_before,
            },
            age_recorder,
        )
    }

    fn assert_track_c_actions_owner_state(
        state: &TrackCActionsOwnerState,
        expected_actions: &[crate::actions::Action],
        expected_actions_version: u64,
    ) {
        let app = &state.app;
        assert_eq!(app.actions.as_slice(), expected_actions);
        assert_eq!(app.custom_len, state.custom_len);
        assert_eq!(
            &app.actions[state.custom_len..],
            state.indexed_tail.as_slice(),
            "custom reload retains the exact indexed tail"
        );
        let expected_cache = expected_actions
            .iter()
            .map(track_c_expected_cached_search_entry)
            .collect::<Vec<_>>();
        assert_eq!(app.action_cache, expected_cache);
        let expected_filter_metadata = expected_actions
            .iter()
            .map(track_c_expected_filter_metadata)
            .collect::<Vec<_>>();
        assert_eq!(app.action_filter_metadata, expected_filter_metadata);
        track_c_assert_action_lookup_matches(&app.actions_by_id, expected_actions);
        assert_eq!(
            app.actions_by_id
                .get(&state.indexed_tail[0].action)
                .expect("duplicate action id is projected"),
            &state.indexed_tail[0],
            "id-only map keeps the last custom-first/indexed-tail duplicate"
        );
        assert_eq!(app.query, "app Synthetic action 00009");
        assert_eq!(app.last_search_query, app.query);
        assert_eq!(app.results, vec![expected_actions[9].clone()]);
        assert!(app.last_results_valid);
        assert!(!app.last_search_pending);
        assert!(!app.background_query_refresh_pending);
        assert_eq!(app.selected, None, "catalog refresh clears stale selection");
        assert_eq!(
            crate::actions::actions_version(),
            expected_actions_version,
            "catalog version remains at the expected publication revision"
        );
    }

    fn validate_track_c_actions_owner(
        state: &TrackCActionsOwnerState,
        actions_version_after: u64,
    ) -> crate::performance::track_c_workloads::OwnerObservation {
        use crate::performance::track_c_workloads::OwnerObservation;

        let app = &state.app;
        assert_track_c_actions_owner_state(
            state,
            &state.expected_actions,
            state.actions_version_before + 1,
        );
        assert_eq!(
            actions_version_after,
            state.actions_version_before + 1,
            "one changed custom-prefix publication advances the version once"
        );

        let structural_signature =
            track_c_watch_action_signature(&state.index_root, app.actions.as_slice());
        let output_identity = track_c_watch_projection_signature(app, &state.index_root);
        OwnerObservation {
            output_identity,
            structural_signature,
            revision_receipts: {
                let mut receipts = vec![actions_version_after, app.last_search_provider_revision];
                if let Some(versions) = app.last_search_result_catalog_versions {
                    receipts.extend([versions.clipboard, versions.todo, versions.notes]);
                } else {
                    receipts.extend([u64::MAX, u64::MAX, u64::MAX]);
                }
                receipts
            },
            viewport_receipt: None,
            work_units: app.actions.len() as u64,
            work_counters: vec![
                app.custom_len as u64,
                app.actions.len().saturating_sub(app.custom_len) as u64,
                app.action_cache.len() as u64,
                app.action_filter_metadata.len() as u64,
                app.actions_by_id.len() as u64,
                app.results.len() as u64,
            ],
        }
    }

    fn enqueue_track_c_actions_event(app: &LauncherApp) {
        app.event_tx
            .with_origin(EventOrigin::FileWatcher)
            .send(WatchEvent::Actions)
            .expect("enqueue synthetic file-reload event");
    }

    #[test]
    fn track_c_oracle_actions_reload_preserves_catalog_and_last_good_state() {
        let fixture = track_c_actions_fixture(24);
        let mut state = make_track_c_actions_owner_state(&fixture);
        let expected_file_payload = serde_json::to_vec(&state.expected_actions[..state.custom_len])
            .expect("serialize expected custom prefix for unchanged notification");
        state.app.process_watch_events();
        validate_track_c_actions_owner(&state, crate::actions::actions_version());

        let retained = Arc::clone(&state.app.actions);
        let retained_version = crate::actions::actions_version();
        std::fs::write(&state.actions_path, &expected_file_payload)
            .expect("write identical custom prefix");
        enqueue_track_c_actions_event(&state.app);
        state.app.process_watch_events();
        assert!(Arc::ptr_eq(&state.app.actions, &retained));
        assert_eq!(crate::actions::actions_version(), retained_version);
        assert!(state.app.actions_persistence_diagnostic.is_none());
        assert_track_c_actions_owner_state(&state, &state.expected_actions, retained_version);

        std::fs::write(&state.actions_path, b"{").expect("write invalid JSON fixture");
        enqueue_track_c_actions_event(&state.app);
        state.app.process_watch_events();
        assert!(Arc::ptr_eq(&state.app.actions, &retained));
        assert_eq!(crate::actions::actions_version(), retained_version);
        assert!(state.app.actions_persistence_diagnostic.is_some());
        assert_track_c_actions_owner_state(&state, &state.expected_actions, retained_version);

        std::fs::remove_file(&state.actions_path).expect("remove actions fixture");
        enqueue_track_c_actions_event(&state.app);
        state.app.process_watch_events();
        assert!(Arc::ptr_eq(&state.app.actions, &retained));
        assert_eq!(crate::actions::actions_version(), retained_version);
        assert!(state.app.actions_persistence_diagnostic.is_some());
        assert_track_c_actions_owner_state(&state, &state.expected_actions, retained_version);

        let mut recovered_custom = state.expected_actions[..state.custom_len].to_vec();
        recovered_custom[1]
            .desc
            .push_str("; recovered after reload error");
        let mut recovered = recovered_custom.clone();
        recovered.extend(state.indexed_tail.iter().cloned());
        std::fs::write(
            &state.actions_path,
            serde_json::to_vec(&recovered_custom).expect("serialize recovery fixture"),
        )
        .expect("write valid recovery fixture");
        enqueue_track_c_actions_event(&state.app);
        let before_recovery = crate::actions::actions_version();
        state.app.process_watch_events();
        assert_eq!(state.app.actions.as_slice(), recovered.as_slice());
        assert_eq!(state.app.custom_len, state.custom_len);
        assert!(state.app.actions_persistence_diagnostic.is_none());
        assert_eq!(crate::actions::actions_version(), before_recovery + 1);
        assert_track_c_actions_owner_state(&state, &recovered, before_recovery + 1);
        assert_eq!(
            state.app.actions_by_id.get(&state.indexed_tail[0].action),
            Some(&state.indexed_tail[0])
        );
    }

    #[test]
    #[ignore = "opt-in Track C benchmark; use an owned isolated test process and small mode for smoke"]
    fn track_c_benchmark_actions_reload_owner() {
        use crate::performance::track_c_workloads;

        assert!(crate::performance::enabled());
        let mode = track_c_workloads::BenchmarkMode::from_process_env()
            .expect("valid Track C benchmark mode");
        for total_count in mode.combined_action_sizes() {
            let fixture = track_c_actions_fixture(*total_count);
            crate::performance::track_c::reset();
            let samples = track_c_workloads::measure_owner(
                || {
                    let state = make_track_c_actions_owner_state(&fixture);
                    crate::performance::track_c::reset();
                    // The event was queued by the state factory. Reset setup
                    // counters after the envelope has its original timestamp.
                    state
                },
                |state| {
                    state.app.process_watch_events();
                    crate::actions::actions_version()
                },
                |state, version_after| validate_track_c_actions_owner(state, *version_after),
            );
            assert_eq!(samples.len(), track_c_workloads::MEASURED_SAMPLES);
            assert!(
                samples
                    .iter()
                    .all(|sample| sample.output_identity == samples[0].output_identity)
            );
            assert!(samples.iter().all(|sample| {
                sample.structural_signature == samples[0].structural_signature
                    && sample.work_units == *total_count as u64
                    && sample.work_counters == samples[0].work_counters
            }));
            let phase_snapshot = crate::performance::track_c::snapshot()
                [crate::performance::track_c::Phase::EventDrain as usize]
                .1;
            // `measure_owner` includes five warmups and 20 measured calls.
            // Its state factory drops any previous app and then resets counters,
            // so the final sample leaves exactly one production drain receipt.
            assert_eq!(phase_snapshot.calls, 1);
            assert_eq!(phase_snapshot.completed, 1);
            assert_eq!(phase_snapshot.work_units, 1);
            let metadata = track_c_workloads::ReportMetadata {
                owner: "process_watch_events_actions_reload",
                fixture_name: "custom_prefix_retained_indexed_tail",
                fixture_signature: fixture.fixture_signature,
                item_count: *total_count,
                viewport: "none",
                scale_milli: 1_000,
                font_state: "not_applicable",
                settings: "inert_plugins_toasts_off",
                cold_type: "typed_file_read_and_catalog_publish",
                mode,
            };
            track_c_workloads::emit_samples(metadata, &samples);
        }
    }

    #[test]
    fn track_c_oracle_finite_watch_burst_preserves_fifo_and_state() {
        let fixture = track_c_actions_fixture(24);
        let mut state = make_track_c_stale_index_watch_queue_owner_state(&fixture, 16, 2, 1);
        state.queue.owner.app.process_watch_events();
        let (observation, receipt) = validate_track_c_watch_queue_owner(&state.queue);
        assert_eq!(observation.work_units, 5);
        assert_eq!(receipt.delivered.delivered.len(), 5);
        assert_eq!(
            receipt
                .delivered
                .delivered
                .iter()
                .map(|sample| (sample.class, sample.origin))
                .collect::<Vec<_>>(),
            [
                (
                    crate::performance::track_c::EventClass::FileReload,
                    EventOrigin::FileWatcher,
                ),
                (
                    crate::performance::track_c::EventClass::IndexCompletion,
                    EventOrigin::IndexCoordinator,
                ),
                (
                    crate::performance::track_c::EventClass::FileReload,
                    EventOrigin::FileWatcher,
                ),
                (
                    crate::performance::track_c::EventClass::FileReload,
                    EventOrigin::FileWatcher,
                ),
                (
                    crate::performance::track_c::EventClass::Clipboard,
                    EventOrigin::Registry,
                ),
            ]
        );
        finish_track_c_stale_index_watch_queue_owner(&mut state);
        assert_eq!(state.queue.owner.app.rx.queued_depth(), Some(0));
    }

    #[test]
    fn track_c_oracle_concurrent_watch_producer_drains_finite_offered_load() {
        const PRODUCER_ACTIONS: usize = 12;
        const PRODUCER_RECYCLE: usize = 1;
        const EVENT_COUNT: usize = 1 + PRODUCER_ACTIONS + PRODUCER_RECYCLE;

        let fixture = track_c_actions_fixture(24);
        let mut state = make_track_c_watch_queue_owner_state(&fixture, 32, 0, 0);
        set_track_c_concurrent_queue_counts(&mut state, PRODUCER_ACTIONS, PRODUCER_RECYCLE);
        run_track_c_concurrent_queue_owner(&mut state, PRODUCER_ACTIONS, PRODUCER_RECYCLE);

        assert_eq!(state.expected_event_count, EVENT_COUNT);
        let (observation, receipt) = validate_track_c_watch_queue_owner(&state);
        assert_eq!(observation.work_units, EVENT_COUNT as u64);
        assert_eq!(receipt.delivered.delivered.len(), EVENT_COUNT);
        assert_eq!(state.owner.app.rx.queued_depth(), Some(0));
    }

    #[test]
    #[ignore = "opt-in Track C benchmark; use an owned isolated test process and small mode for smoke"]
    fn track_c_benchmark_watch_event_finite_burst_owner() {
        use crate::performance::track_c_workloads::{self, ReportMetadata};

        assert!(crate::performance::enabled());
        let mode = track_c_workloads::BenchmarkMode::from_process_env()
            .expect("valid Track C benchmark mode");
        for total_count in mode.combined_action_sizes() {
            const EXTRA_ACTIONS: usize = 5;
            const RECYCLE: usize = 1;
            const RECORDER_CAPACITY: usize = 16;
            let fixture = track_c_actions_fixture(*total_count);
            let fixture_signature = track_c_watch_queue_fixture_signature(
                &fixture,
                RECORDER_CAPACITY,
                EXTRA_ACTIONS + 1,
                1,
                RECYCLE,
                false,
            );
            let mut queue_receipts = Vec::with_capacity(
                track_c_workloads::WARMUPS + track_c_workloads::MEASURED_SAMPLES,
            );
            let samples = track_c_workloads::measure_owner(
                || {
                    make_track_c_stale_index_watch_queue_owner_state(
                        &fixture,
                        RECORDER_CAPACITY,
                        EXTRA_ACTIONS,
                        RECYCLE,
                    )
                },
                |state| state.queue.owner.app.process_watch_events(),
                |state, ()| {
                    let (observation, receipt) = validate_track_c_watch_queue_owner(&state.queue);
                    queue_receipts.push(receipt);
                    // Preserve the primary drain's complete observation and
                    // age receipt before consuming B's cleanup notification.
                    finish_track_c_stale_index_watch_queue_owner(state);
                    observation
                },
            );
            assert_eq!(samples.len(), track_c_workloads::MEASURED_SAMPLES);
            assert!(samples.iter().all(|sample| {
                sample.output_identity == samples[0].output_identity
                    && sample.structural_signature == samples[0].structural_signature
                    && sample.work_units == 8
                    && sample.work_counters == samples[0].work_counters
            }));
            assert_eq!(
                queue_receipts.len(),
                track_c_workloads::WARMUPS + track_c_workloads::MEASURED_SAMPLES
            );
            let metadata = ReportMetadata {
                owner: "process_watch_events_finite_burst",
                fixture_name: "actions_original_stale_indexready_recycle",
                fixture_signature,
                item_count: *total_count,
                viewport: "none",
                scale_milli: 1_000,
                font_state: "not_applicable",
                settings: "inert_plugins_toasts_off",
                cold_type: "stale_index_gate_finite_burst",
                mode,
            };
            track_c_workloads::emit_samples(metadata, &samples);
            emit_track_c_watch_event_age_rows(
                metadata,
                RECORDER_CAPACITY,
                &samples,
                &queue_receipts[track_c_workloads::WARMUPS..],
            );
        }
    }

    #[test]
    #[ignore = "opt-in Track C benchmark; use an owned isolated test process and small mode for smoke"]
    fn track_c_benchmark_watch_event_concurrent_offered_load_owner() {
        use crate::performance::track_c_workloads::{
            self, OwnerObservation, ReportMetadata, SampleRecord,
        };

        assert!(crate::performance::enabled());
        let mode = track_c_workloads::BenchmarkMode::from_process_env()
            .expect("valid Track C benchmark mode");
        for total_count in mode.combined_action_sizes() {
            const PRODUCER_ACTIONS: usize = 12;
            const PRODUCER_RECYCLE: usize = 1;
            const RECORDER_CAPACITY: usize = 32;
            let fixture = track_c_actions_fixture(*total_count);
            let fixture_signature = track_c_watch_queue_fixture_signature(
                &fixture,
                RECORDER_CAPACITY,
                PRODUCER_ACTIONS + 1,
                0,
                PRODUCER_RECYCLE,
                true,
            );
            let make_state = || {
                let mut state =
                    make_track_c_watch_queue_owner_state(&fixture, RECORDER_CAPACITY, 0, 0);
                set_track_c_concurrent_queue_counts(&mut state, PRODUCER_ACTIONS, PRODUCER_RECYCLE);
                state
            };
            for _ in 0..track_c_workloads::WARMUPS {
                let mut state = make_state();
                run_track_c_concurrent_queue_owner(&mut state, PRODUCER_ACTIONS, PRODUCER_RECYCLE);
                let _ = validate_track_c_watch_queue_owner(&state);
            }

            let mut samples = Vec::with_capacity(track_c_workloads::MEASURED_SAMPLES);
            let mut receipts = Vec::with_capacity(track_c_workloads::MEASURED_SAMPLES);
            for _ in 0..track_c_workloads::MEASURED_SAMPLES {
                let mut state = make_state();
                run_track_c_concurrent_queue_owner(&mut state, PRODUCER_ACTIONS, PRODUCER_RECYCLE);
                let (observation, receipt): (OwnerObservation, TrackCWatchQueueSampleReceipt) =
                    validate_track_c_watch_queue_owner(&state);
                samples.push(SampleRecord {
                    // The result uses actual reducer timers; the scoped producer
                    // spawn, barrier, and join remain outside those intervals.
                    elapsed_nanos: receipt.drain.elapsed_nanos_total,
                    output_identity: observation.output_identity,
                    structural_signature: observation.structural_signature,
                    revision_receipts: observation.revision_receipts,
                    viewport_receipt: observation.viewport_receipt,
                    work_units: observation.work_units,
                    work_counters: observation.work_counters,
                });
                receipts.push(receipt);
            }
            assert_eq!(samples.len(), track_c_workloads::MEASURED_SAMPLES);
            assert!(samples.iter().all(|sample| {
                sample.output_identity == samples[0].output_identity
                    && sample.structural_signature == samples[0].structural_signature
                    && sample.work_units == 14
                    && sample.work_counters == samples[0].work_counters
            }));
            let metadata = ReportMetadata {
                owner: "process_watch_events_concurrent_offered_load",
                fixture_name: "actions_recycle_finite_producer",
                fixture_signature,
                item_count: *total_count,
                viewport: "none",
                scale_milli: 1_000,
                font_state: "not_applicable",
                settings: "inert_plugins_toasts_off",
                cold_type: "concurrent_finite_offered_load",
                mode,
            };
            track_c_workloads::emit_samples(metadata, &samples);
            emit_track_c_watch_event_age_rows(metadata, RECORDER_CAPACITY, &samples, &receipts);
        }
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

    #[test]
    #[ignore = "opt-in Track A workload benchmark; set MULTI_LAUNCHER_PERF=1 before the process"]
    fn track_a_benchmark_index_config_completion_owner() {
        use crate::performance::{Metric, workloads};

        assert!(crate::performance::enabled());
        let workspace = workloads::IsolatedWorkspace::new();
        for indexed_count in workloads::selected_sizes(&[1_000, 10_000]) {
            let fixture_root = workspace
                .root()
                .join(format!("index-config-{indexed_count}"));
            let root_a = fixture_root.join("root-a");
            let root_b = fixture_root.join("root-b");
            let a_count = indexed_count.div_ceil(2);
            let b_count = indexed_count - a_count;
            let fixture_a = workloads::create_index_tree(&root_a, 0x494e_4445_5841, a_count);
            let fixture_b = workloads::create_index_tree(&root_b, 0x494e_4445_5842, b_count);
            let roots_a = vec![root_a.clone(), root_b.clone()];
            let roots_b = vec![root_b.clone(), root_a.clone()];
            let config_a = crate::indexer::coordinator::IndexConfig::new(
                roots_a
                    .iter()
                    .map(|root| root.to_string_lossy().into_owned())
                    .collect(),
                Some(indexed_count),
            );
            let config_b = crate::indexer::coordinator::IndexConfig::new(
                roots_b
                    .iter()
                    .map(|root| root.to_string_lossy().into_owned())
                    .collect(),
                Some(indexed_count),
            );
            assert_ne!(config_a, config_b);
            let expected_a = indexed_actions_for_roots(&roots_a, indexed_count);
            let expected_b = indexed_actions_for_roots(&roots_b, indexed_count);
            assert_eq!(expected_a.len(), indexed_count);
            assert_eq!(expected_b.len(), indexed_count);
            let signatures_a = multi_root_output_signatures(&root_a, &root_b, &expected_a);
            let signatures_b = multi_root_output_signatures(&root_a, &root_b, &expected_b);
            assert_ne!(signatures_a.1, signatures_b.1);

            let custom_fixture = workloads::action_fixture(0x494e_4445_5843, 8);
            let custom = custom_fixture.values;
            let mut initial_actions = custom.clone();
            initial_actions.extend(expected_a.iter().cloned());
            let actions_path = fixture_root.join("actions.json");
            let settings_path = fixture_root.join("settings.json");
            let mut settings = Settings::default();
            settings.index_paths = Some(config_a.roots().iter().map(Clone::clone).collect());
            settings.max_indexed_items = Some(indexed_count);
            settings.hotkey = None;
            settings.quit_hotkey = None;
            settings.help_hotkey = None;
            settings.enabled_plugins = Some(std::collections::HashSet::new());
            settings.enable_toasts = false;
            settings.show_inline_errors = false;
            settings.show_error_toasts = false;
            settings.dashboard.enabled = false;
            let context = egui::Context::default();
            let mut app = LauncherApp::new(
                &context,
                Arc::new(initial_actions),
                custom.len(),
                PluginManager::new_inert_for_test(),
                actions_path.to_string_lossy().into_owned(),
                settings_path.to_string_lossy().into_owned(),
                settings,
                None,
                Some(config_a.roots().to_vec()),
                Some(std::collections::HashSet::new()),
                None,
                Arc::new(AtomicBool::new(false)),
                Arc::new(AtomicBool::new(false)),
                Arc::new(AtomicBool::new(false)),
            );
            app.watchers.clear();
            app.process_watch_events();
            assert!(app.rx.try_recv().is_err(), "initial watcher events drained");
            app.update_action_cache();
            app.query = "file".into();
            app.search();
            assert!(app.last_results_valid);

            let coordinator = crate::indexer::coordinator::IndexCoordinator::new()
                .expect("start one persistent indexing worker");
            app.install_test_index_coordinator(coordinator);

            // Bootstrap the persistent worker outside the five measured-protocol
            // warmups. Its scan is discarded when metrics reset below.
            run_index_config_sample(&mut app, &config_b, &custom, &expected_b);
            for iteration in 0..workloads::UI_WARMUPS {
                let (config, expected) = if iteration % 2 == 0 {
                    (&config_a, &expected_a)
                } else {
                    (&config_b, &expected_b)
                };
                run_index_config_sample(&mut app, config, &custom, expected);
            }
            crate::performance::reset_metrics();

            let mut request_samples = [0_u64; workloads::SAMPLE_COUNT];
            let mut completion_samples = [0_u64; workloads::SAMPLE_COUNT];
            let mut publication_samples = [0_u64; workloads::SAMPLE_COUNT];
            for iteration in 0..workloads::SAMPLE_COUNT {
                let (config, expected) = if iteration % 2 == 0 {
                    (&config_b, &expected_b)
                } else {
                    (&config_a, &expected_a)
                };
                let elapsed = run_index_config_sample(&mut app, config, &custom, expected);
                request_samples[iteration] = elapsed[0];
                completion_samples[iteration] = elapsed[1];
                publication_samples[iteration] = elapsed[2];
            }

            let scan_metrics = workloads::metrics_for(&[Metric::IndexScan]);
            assert_eq!(scan_metrics.len(), 1);
            let scan = scan_metrics[0];
            assert_eq!(scan.metric, Metric::IndexScan);
            assert!(scan.calls > 0);
            assert_eq!(
                scan.work_units,
                indexed_count as u64 * workloads::SAMPLE_COUNT as u64
            );
            assert_eq!(scan.completed, workloads::SAMPLE_COUNT as u64);
            assert_eq!(scan.errors, 0);
            assert_eq!(scan.abandoned, 0);

            let mut fixture_signature = workloads::StableSignature::new(
                0x494e_4445_5844,
                "index-config-completion-workload",
                custom.len() + indexed_count,
            );
            fixture_signature.number(custom_fixture.summary.signature);
            fixture_signature.number(fixture_a.signature);
            fixture_signature.number(fixture_b.signature);
            fixture_signature.number(signatures_a.1);
            fixture_signature.number(signatures_b.1);
            let mut output_membership = workloads::StableSignature::new(
                0x494e_4445_584d,
                "index-config-completion-membership",
                custom.len() + indexed_count,
            );
            output_membership.number(signatures_a.0);
            output_membership.number(signatures_b.0);
            let mut output_order = workloads::StableSignature::new(
                0x494e_4445_584f,
                "index-config-completion-order",
                custom.len() + indexed_count,
            );
            output_order.number(signatures_a.1);
            output_order.number(signatures_b.1);
            let summary = workloads::FixtureSummary {
                count: custom.len() + indexed_count,
                estimated_bytes: custom_fixture
                    .summary
                    .estimated_bytes
                    .saturating_add(fixture_a.estimated_bytes)
                    .saturating_add(fixture_b.estimated_bytes),
                signature: fixture_signature.finish(),
                output_signature: Some(output_membership.finish()),
                output_order_signature: Some(output_order.finish()),
            };

            workloads::emit_summary(
                &format!("index-config-{indexed_count}-request"),
                "LauncherApp::request_index_config; synchronous config acceptance/submission only, with worker traversal and waiting excluded",
                summary,
                timing_summary(request_samples, workloads::UI_WARMUPS),
                &[],
            );
            workloads::emit_summary(
                &format!("index-config-{indexed_count}-completion"),
                "request entry through IndexCoordinator completion, including synchronous submission and background traversal/wait; GUI event publication excluded; one worker-bootstrap scan is outside the five warmups and discarded with setup metrics",
                summary,
                timing_summary(completion_samples, workloads::UI_WARMUPS),
                &scan_metrics,
            );
            workloads::emit_summary(
                &format!("index-config-{indexed_count}-publication"),
                "real app-scoped IndexReady notification through LauncherApp::process_watch_events; includes action/cache/query publication, event dequeue wait excluded",
                summary,
                timing_summary(publication_samples, workloads::UI_WARMUPS),
                &[],
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

        let (tx, rx) = super::event_channel::channel();
        tx.send(WatchEvent::Clipboard).unwrap();
        tx.send(WatchEvent::Folders).unwrap();
        assert_eq!(recv_test_event(&rx), Some(TestWatchEvent::Folders));

        let (tx, rx) = super::event_channel::channel();
        tx.send(WatchEvent::ExecuteAction(action)).unwrap();
        tx.send(WatchEvent::Bookmarks).unwrap();
        assert_eq!(recv_test_event(&rx), Some(TestWatchEvent::Bookmarks));
    }
}
