//! Opt-in C07 headless startup measurement using the owned synthetic profile.

use super::LauncherApp;
use crate::performance::startup_profile::{ProfileScale, StartupProfile, materialize};
use crate::performance::track_c_workloads::TrackCWorkspace;
use eframe::egui;
use std::sync::Arc;
use std::time::Instant;

const SAMPLE_COUNT: usize = 3;
const FIRST_ACTION_ID: &str = "synthetic:action:00000";
const FIRST_ACTION_QUERY: &str = "Synthetic action 00000";

struct StartupSample {
    index: usize,
    settings_load_ns: u64,
    action_load_ns: u64,
    note_load_ns: u64,
    note_vec_clone_ns: u64,
    note_cache_build_ns: u64,
    snippet_load_ns: u64,
    history_load_ns: u64,
    action_id_lookup_pair_ns: u64,
    action_id_lookup_pair_key_clones: usize,
    indexed_catalog_ready_ns: u64,
    catalog_assembly_ns: u64,
    indexed_action_count: usize,
    catalog_action_count: usize,
    catalog_signature: u64,
    headless_app_constructor_ns: u64,
    startup_transfer_install_ns: u64,
    first_query_ns: u64,
    first_query_result_count: usize,
    first_query_result_signature: u64,
}

#[test]
#[ignore = "opt-in C07 measurement; set ML_C07_STARTUP_PROFILE=small or large"]
fn c07_startup_profile_measurement() {
    let scale = std::env::var("ML_C07_STARTUP_PROFILE")
        .map_or(Ok(ProfileScale::Small), |value| ProfileScale::parse(&value))
        .expect("ML_C07_STARTUP_PROFILE is either small or large");
    let mut workspace = TrackCWorkspace::new();
    let profile = materialize(&mut workspace, scale).expect("owned startup profile materializes");
    println!("C07_STARTUP_PROFILE {}", profile.receipt().json_line());

    let samples = (0..SAMPLE_COUNT)
        .map(|index| measure_sample(index, &profile))
        .collect::<Vec<_>>();
    for sample in &samples {
        println!(
            "C07_STARTUP_SAMPLE {}",
            serde_json::json!({
                "profile": scale.name(),
                "sample": sample.index,
                "cache_condition": "same persisted profile; sequential OS cache uncontrolled",
                "measurement_boundary": "isolated library harness; production binary wrapper and native UI readiness are separate",
                "headless_app_scope": "inert plugin manager, empty provider lists, action-query path only",
                "note_clone_attribution": "standalone Vec<Note> clone proxy; full NoteCache build includes the resolver's map cloning",
                "phase_ns": {
                    "settings_load_with_startup_migration_check": sample.settings_load_ns,
                    "actions_load": sample.action_load_ns,
                    "notes_load": sample.note_load_ns,
                    "note_vec_clone_proxy": sample.note_vec_clone_ns,
                    "note_cache_candidate_full_build": sample.note_cache_build_ns,
                    "snippets_load": sample.snippet_load_ns,
                    "history_load": sample.history_load_ns,
                    "two_action_id_lookup_builds_candidate": sample.action_id_lookup_pair_ns,
                    "index_coordinator_scan_wait_ack": sample.indexed_catalog_ready_ns,
                    "custom_first_catalog_materialization": sample.catalog_assembly_ns,
                    "isolated_headless_app_constructor_with_empty_plugins": sample.headless_app_constructor_ns,
                    "startup_index_transfer_install": sample.startup_transfer_install_ns,
                    "first_action_query": sample.first_query_ns,
                },
                "action_id_lookup_pair_key_clone_attempts": sample.action_id_lookup_pair_key_clones,
                "indexed_actions": sample.indexed_action_count,
                "catalog_actions": sample.catalog_action_count,
                "catalog_signature_fnv64": format!("{:016x}", sample.catalog_signature),
                "first_query_results": sample.first_query_result_count,
                "first_query_result_signature_fnv64": format!("{:016x}", sample.first_query_result_signature),
                "native_ui_ready": "unavailable in headless unit-test process; measure separately on supported Windows launch",
                "candidate_status": "attribution only; no optimization applied",
            })
        );
    }
    assert_eq!(samples.len(), SAMPLE_COUNT);
    assert!(samples.iter().all(|sample| {
        sample.indexed_action_count == profile.receipt().counts.indexed_files
            && sample.catalog_action_count
                == profile.receipt().counts.actions + profile.receipt().counts.indexed_files
            && sample.catalog_signature == samples[0].catalog_signature
            && sample.first_query_result_signature == samples[0].first_query_result_signature
    }));

    workspace
        .finish()
        .expect("owned startup profile restores process state and removes root");
}

fn measure_sample(index: usize, profile: &StartupProfile) -> StartupSample {
    let started = Instant::now();
    let startup_settings = crate::startup::load_startup_settings(profile.settings_path());
    assert!(startup_settings.diagnostic.is_none());
    let settings_load_ns = elapsed_ns(started);
    let mut settings = startup_settings.settings;
    // Keep the isolated app's construction free of global Paste Sequence hooks.
    settings.paste_sequence.next_hotkey = None;
    settings.paste_sequence.cancel_hotkey = None;
    assert!(settings.index_paths.as_ref().is_some_and(|roots| {
        roots.len() == 1 && std::path::Path::new(&roots[0]) == profile.index_root()
    }));

    let started = Instant::now();
    let startup_actions = crate::actions::load_startup_actions(profile.actions_path());
    assert!(startup_actions.diagnostic.is_none());
    let action_load_ns = elapsed_ns(started);
    assert_eq!(
        startup_actions.actions.len(),
        profile.receipt().counts.actions
    );
    assert_eq!(
        crate::performance::startup_profile::safe_hash(
            startup_actions
                .actions
                .last()
                .expect("loaded actions are nonempty")
                .action
                .as_bytes()
        ),
        profile.receipt().signatures_fnv64.action_last_sentinel
    );
    let custom_actions = Arc::new(startup_actions.actions);

    let started = Instant::now();
    let notes = crate::plugins::note::load_notes().expect("persisted profile notes load");
    let note_load_ns = elapsed_ns(started);
    assert_eq!(notes.len(), profile.receipt().counts.notes);
    assert!(notes.iter().any(|note| {
        crate::performance::startup_profile::safe_hash(note.slug.as_bytes())
            == profile.receipt().signatures_fnv64.note_last_sentinel
    }));

    let started = Instant::now();
    let temporary_resolver_notes = std::hint::black_box(notes.clone());
    let note_vec_clone_ns = elapsed_ns(started);
    drop(temporary_resolver_notes);

    let started = Instant::now();
    let note_cache = crate::plugins::note::build_note_cache_candidate_for_test(notes);
    let note_cache_build_ns = elapsed_ns(started);
    assert_eq!(note_cache.notes.len(), profile.receipt().counts.notes);

    let started = Instant::now();
    let snippets = crate::plugins::snippets::load_snippets_typed(profile.snippets_path())
        .expect("persisted profile snippets load");
    let snippet_load_ns = elapsed_ns(started);
    let crate::common::persistence::LoadState::Loaded(snippets) = snippets else {
        panic!("persisted profile snippets are present");
    };
    assert_eq!(snippets.len(), profile.receipt().counts.snippets);
    assert!(snippets.iter().any(|snippet| {
        crate::performance::startup_profile::safe_hash(snippet.alias.as_bytes())
            == profile.receipt().signatures_fnv64.snippet_last_sentinel
    }));

    let started = Instant::now();
    let history = crate::history::load_startup_profile_history(profile.history_path())
        .expect("persisted profile history loads");
    let history_load_ns = elapsed_ns(started);
    assert_eq!(history.len(), profile.receipt().counts.history);
    assert_eq!(
        crate::performance::startup_profile::safe_hash(
            history
                .back()
                .expect("persisted history is nonempty")
                .query
                .as_bytes()
        ),
        profile.receipt().signatures_fnv64.history_last_sentinel
    );

    let config = crate::indexer::coordinator::IndexConfig::new(
        settings.index_paths.clone().unwrap_or_default(),
        settings.max_indexed_items,
    );
    let started = Instant::now();
    let coordinator = crate::indexer::coordinator::IndexCoordinator::new()
        .expect("startup coordinator constructs");
    let generation = coordinator
        .submit(config.clone())
        .expect("startup index request submits");
    let completion = coordinator
        .wait_for_completion(generation)
        .expect("startup index request completes");
    assert_eq!(completion.generation(), generation);
    assert_eq!(completion.config(), &config);
    let indexed_actions = completion
        .outcome()
        .as_ref()
        .map(Arc::clone)
        .expect("synthetic index scan succeeds");
    let acknowledged = coordinator
        .take_result()
        .expect("startup result remains retained for acknowledgement");
    assert_eq!(acknowledged.generation(), generation);
    coordinator
        .validate_acknowledged_result(generation, &config)
        .expect("startup result validates before ownership transfer");
    let indexed_catalog_ready_ns = elapsed_ns(started);
    assert_eq!(
        indexed_actions.len(),
        profile.receipt().counts.indexed_files
    );
    let last_index_path =
        profile
            .index_root()
            .join(crate::performance::startup_profile::index_relative_path(
                profile.receipt().counts.indexed_files - 1,
            ));
    assert!(
        indexed_actions
            .iter()
            .any(|action| std::path::Path::new(&action.action) == last_index_path)
    );

    let started = Instant::now();
    let custom_len = custom_actions.len();
    let mut catalog_values = Arc::try_unwrap(custom_actions)
        .expect("temporary action ID candidates were dropped before catalog assembly");
    catalog_values.extend(indexed_actions.iter().cloned());
    let catalog = Arc::new(catalog_values);
    let catalog_assembly_ns = elapsed_ns(started);
    assert_eq!(catalog.len(), custom_len + indexed_actions.len());
    let catalog_signature = signature_ids(catalog.iter().map(|action| action.action.as_str()));

    // The current isolated app constructor builds one ID map and its first
    // action-cache initialization builds another. Time the equivalent pair
    // against the complete custom-first/indexed-tail catalog as attribution,
    // without changing either owner.
    let (action_id_lookup_pair_ns, action_id_lookup_pair_key_clones) = {
        let started = Instant::now();
        let (first, first_receipt) =
            crate::actions::ActionIdLookup::new_with_test_receipt(Arc::clone(&catalog));
        let (second, second_receipt) =
            crate::actions::ActionIdLookup::new_with_test_receipt(Arc::clone(&catalog));
        let elapsed = elapsed_ns(started);
        assert!(first.get(FIRST_ACTION_ID).is_some());
        assert!(second.get(FIRST_ACTION_ID).is_some());
        let key_clones = first_receipt
            .key_clone_attempts
            .saturating_add(second_receipt.key_clone_attempts);
        drop((first, second));
        (elapsed, key_clones)
    };

    let context = egui::Context::default();
    let started = Instant::now();
    let mut app = LauncherApp::new_isolated_for_startup_measurement_at(
        &context,
        profile.root(),
        catalog,
        custom_len,
        settings,
        config.clone(),
    );
    let headless_app_constructor_ns = elapsed_ns(started);

    let started = Instant::now();
    app.install_startup_indexing(coordinator, config, generation)
        .expect("acknowledged startup index transfers to the app");
    let startup_transfer_install_ns = elapsed_ns(started);
    assert_eq!(app.custom_len, custom_len);
    assert_eq!(app.actions.len(), custom_len + indexed_actions.len());
    assert_eq!(
        signature_ids(app.actions.iter().map(|action| action.action.as_str())),
        catalog_signature,
        "startup transfer preserves the complete custom-first/indexed-tail catalog"
    );
    assert!(
        app.actions[custom_len..]
            .iter()
            .any(|action| std::path::Path::new(&action.action) == last_index_path)
    );

    app.query = format!("{} {FIRST_ACTION_QUERY}", super::APP_PREFIX);
    let started = Instant::now();
    app.search();
    let first_query_ns = elapsed_ns(started);
    assert!(
        app.results
            .iter()
            .any(|action| action.action == FIRST_ACTION_ID),
        "first query returns the deterministic action sentinel"
    );
    let first_query_result_signature =
        app.results
            .iter()
            .fold(0xcbf2_9ce4_8422_2325_u64, |mut signature, action| {
                for byte in action.action.as_bytes() {
                    signature ^= u64::from(*byte);
                    signature = signature.wrapping_mul(0x100_0000_01b3);
                }
                signature
            });
    let first_query_result_count = app.results.len();

    StartupSample {
        index,
        settings_load_ns,
        action_load_ns,
        note_load_ns,
        note_vec_clone_ns,
        note_cache_build_ns,
        snippet_load_ns,
        history_load_ns,
        action_id_lookup_pair_ns,
        action_id_lookup_pair_key_clones,
        indexed_catalog_ready_ns,
        catalog_assembly_ns,
        indexed_action_count: indexed_actions.len(),
        catalog_action_count: custom_len + indexed_actions.len(),
        catalog_signature,
        headless_app_constructor_ns,
        startup_transfer_install_ns,
        first_query_ns,
        first_query_result_count,
        first_query_result_signature,
    }
}

fn signature_ids<'a>(ids: impl Iterator<Item = &'a str>) -> u64 {
    ids.fold(0xcbf2_9ce4_8422_2325_u64, |mut signature, id| {
        for byte in id.as_bytes() {
            signature ^= u64::from(*byte);
            signature = signature.wrapping_mul(0x100_0000_01b3);
        }
        signature
    })
}

fn elapsed_ns(started: Instant) -> u64 {
    started.elapsed().as_nanos().min(u64::MAX as u128) as u64
}
