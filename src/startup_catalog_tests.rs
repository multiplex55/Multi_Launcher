use crate::actions::Action;
use crate::indexer::coordinator::{CoordinatorError, IndexConfig, IndexCoordinator};
use crate::startup_catalog::{self, StartupIndexingTransfer};
use std::cell::RefCell;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

const STARTUP_CALL_TIMEOUT: Duration = Duration::from_secs(10);

thread_local! {
    static STARTUP_COORDINATOR_OVERRIDE: RefCell<Option<IndexCoordinator>> = const { RefCell::new(None) };
}

pub(super) fn take_startup_coordinator_override() -> Option<IndexCoordinator> {
    STARTUP_COORDINATOR_OVERRIDE.with(|slot| slot.borrow_mut().take())
}

struct StartupCoordinatorOverrideGuard;

impl StartupCoordinatorOverrideGuard {
    fn install(coordinator: IndexCoordinator) -> Self {
        STARTUP_COORDINATOR_OVERRIDE.with(|slot| {
            assert!(
                slot.borrow().is_none(),
                "startup coordinator overrides do not nest"
            );
            *slot.borrow_mut() = Some(coordinator);
        });
        Self
    }
}

impl Drop for StartupCoordinatorOverrideGuard {
    fn drop(&mut self) {
        STARTUP_COORDINATOR_OVERRIDE.with(|slot| {
            drop(slot.borrow_mut().take());
        });
    }
}

/// Owns release-before-join behavior so a panic in the test cannot strand the
/// startup caller behind the scanner gate while its workspace is being torn
/// down.
struct StartupCallerGuard {
    release: Option<SyncSender<()>>,
    caller: Option<JoinHandle<()>>,
}

impl StartupCallerGuard {
    fn new(release: SyncSender<()>, caller: JoinHandle<()>) -> Self {
        Self {
            release: Some(release),
            caller: Some(caller),
        }
    }

    fn release_and_join(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
        if let Some(caller) = self.caller.take() {
            let _ = caller.join();
        }
    }
}

impl Drop for StartupCallerGuard {
    fn drop(&mut self) {
        self.release_and_join();
    }
}

fn receive_with_deadline<T>(receiver: &Receiver<T>, description: &str) -> T {
    receiver
        .recv_timeout(STARTUP_CALL_TIMEOUT)
        .unwrap_or_else(|error| {
            panic!("{description} did not arrive before the bounded deadline: {error}")
        })
}

fn owned_action(label: &str, action: &str, args: Option<String>) -> Action {
    Action {
        label: label.to_owned(),
        desc: format!("{label} description"),
        action: action.to_owned(),
        args,
    }
}

#[test]
fn startup_action_catalog_waits_for_blocked_initial_scan_before_publication() {
    use crate::performance::track_c_workloads::TrackCWorkspace;

    // Workspace ownership comes first and is consequently dropped last.
    let workspace = TrackCWorkspace::new();
    let root = workspace.root().join("blocked-startup-index");
    std::fs::create_dir_all(&root).expect("owned startup index root is created");
    let config = IndexConfig::new(vec![root.to_string_lossy().into_owned()], Some(2));

    let custom = owned_action("Custom first", "custom:first", None);
    let indexed = vec![
        owned_action("Indexed empty args", "indexed:empty", Some(String::new())),
        owned_action(
            "Indexed values",
            "indexed:values",
            Some("alpha beta".into()),
        ),
    ];
    let scanner_config = config.clone();
    let scanner_actions = indexed.clone();
    let (entered_sender, entered_receiver) = mpsc::sync_channel(1);
    let (release_sender, release_receiver) = mpsc::sync_channel(1);
    let release_receiver = Arc::new(Mutex::new(release_receiver));
    let coordinator = IndexCoordinator::with_test_scanner(move |actual_config| {
        if actual_config != &scanner_config {
            return Err("startup scanner received a different config".into());
        }
        entered_sender
            .send(())
            .map_err(|_| "startup caller left before scan entry".to_owned())?;
        release_receiver
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .recv_timeout(STARTUP_CALL_TIMEOUT)
            .map_err(|error| format!("startup scan release deadline elapsed: {error}"))?;
        Ok(scanner_actions.clone())
    })
    .expect("channel-gated startup coordinator starts");

    let (result_sender, result_receiver) = mpsc::sync_channel(1);
    let caller_config = config.clone();
    let caller = std::thread::spawn(move || {
        let _override_guard = StartupCoordinatorOverrideGuard::install(coordinator);
        let result = startup_catalog::startup_action_catalog(vec![custom], caller_config);
        let _ = result_sender.send(result);
    });
    let mut caller_guard = StartupCallerGuard::new(release_sender, caller);

    receive_with_deadline(&entered_receiver, "blocked scanner entry");
    assert!(
        matches!(result_receiver.try_recv(), Err(TryRecvError::Empty)),
        "the startup catalog cannot be published while its actual scanner is blocked"
    );

    caller_guard.release_and_join();
    let result = receive_with_deadline(&result_receiver, "startup catalog result");
    let (catalog, custom_len, transfer): (
        std::sync::Arc<Vec<Action>>,
        usize,
        Option<StartupIndexingTransfer>,
    ) = result.expect("blocked startup scan completes and publishes a catalog");

    assert_eq!(custom_len, 1);
    let mut expected_catalog = vec![owned_action("Custom first", "custom:first", None)];
    expected_catalog.extend(indexed);
    assert_eq!(catalog.as_slice(), expected_catalog.as_slice());

    let transfer = transfer.expect("nonempty roots transfer their acknowledged coordinator");
    assert_eq!(transfer.config, config);
    assert_eq!(transfer.generation, 1);
    transfer
        .coordinator
        .validate_acknowledged_result(transfer.generation, &transfer.config)
        .expect("the returned coordinator retains the exact acknowledged identity");
    assert!(matches!(
        transfer
            .coordinator
            .validate_acknowledged_result(transfer.generation + 1, &transfer.config),
        Err(CoordinatorError::Superseded { .. })
    ));
    assert!(
        transfer.coordinator.take_result().is_none(),
        "startup consumes and acknowledges the completion exactly once"
    );
    drop(transfer);
}
