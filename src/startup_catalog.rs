use super::startup_runtime as runtime;
use anyhow::Context;
use runtime::actions::Action;
use runtime::indexer::coordinator::{IndexConfig, IndexCoordinator};
use std::sync::Arc;

pub(super) struct StartupIndexingTransfer {
    pub(super) coordinator: IndexCoordinator,
    pub(super) config: IndexConfig,
    pub(super) generation: u64,
}

#[cfg(test)]
pub(crate) mod track_c_startup_phase_observer {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Instant;

    const PHASE_COUNT: usize = 7;

    #[derive(Clone, Copy)]
    pub(crate) enum Phase {
        CoordinatorConstruct = 0,
        Submit = 1,
        Wait = 2,
        TakeResult = 3,
        ValidateTransfer = 4,
        CloneIndexedActions = 5,
        WrapCatalogArc = 6,
    }

    thread_local! {
        static ACTIVE: RefCell<Option<Rc<RefCell<[u64; PHASE_COUNT]>>>> = const { RefCell::new(None) };
    }

    pub(crate) struct Observation {
        values: Rc<RefCell<[u64; PHASE_COUNT]>>,
    }

    impl Observation {
        pub(crate) fn begin() -> Self {
            let values = Rc::new(RefCell::new([0; PHASE_COUNT]));
            ACTIVE.with(|active| {
                assert!(
                    active.borrow().is_none(),
                    "startup phase observers do not nest"
                );
                *active.borrow_mut() = Some(Rc::clone(&values));
            });
            Self { values }
        }

        pub(crate) fn finish(self) -> [u64; PHASE_COUNT] {
            ACTIVE.with(|active| {
                let current = active.borrow_mut().take();
                assert!(current.is_some_and(|current| Rc::ptr_eq(&current, &self.values)));
            });
            *self.values.borrow()
        }
    }

    impl Drop for Observation {
        fn drop(&mut self) {
            ACTIVE.with(|active| {
                let mut active = active.borrow_mut();
                if active
                    .as_ref()
                    .is_some_and(|current| Rc::ptr_eq(current, &self.values))
                {
                    *active = None;
                }
            });
        }
    }

    pub(crate) struct PhaseTimer {
        phase: Phase,
        started: Instant,
        values: Rc<RefCell<[u64; PHASE_COUNT]>>,
    }

    pub(crate) fn start(phase: Phase) -> Option<PhaseTimer> {
        ACTIVE.with(|active| {
            active.borrow().as_ref().map(|values| PhaseTimer {
                phase,
                started: Instant::now(),
                values: Rc::clone(values),
            })
        })
    }

    impl Drop for PhaseTimer {
        fn drop(&mut self) {
            let elapsed = self.started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
            let slot = &mut self.values.borrow_mut()[self.phase as usize];
            *slot = slot.saturating_add(elapsed);
        }
    }
}

/// Complete the initial index on the main thread before plugin registration.
/// The returned coordinator has acknowledged its startup result and is ready
/// to own later configuration changes after transfer to the GUI app.
pub(super) fn startup_indexed_actions(
    config: IndexConfig,
) -> anyhow::Result<(Arc<Vec<Action>>, Option<StartupIndexingTransfer>)> {
    if config.roots().is_empty() {
        return Ok((Arc::new(Vec::new()), None));
    }

    #[cfg(test)]
    let phase = track_c_startup_phase_observer::start(
        track_c_startup_phase_observer::Phase::CoordinatorConstruct,
    );
    let coordinator = super::startup_index_coordinator_for_owner()
        .context("failed to create startup index coordinator")?;
    #[cfg(test)]
    drop(phase);

    #[cfg(test)]
    let phase =
        track_c_startup_phase_observer::start(track_c_startup_phase_observer::Phase::Submit);
    let generation = coordinator
        .submit(config.clone())
        .context("failed to submit startup index request")?;
    #[cfg(test)]
    drop(phase);

    #[cfg(test)]
    let phase = track_c_startup_phase_observer::start(track_c_startup_phase_observer::Phase::Wait);
    let waited = coordinator
        .wait_for_completion(generation)
        .context("startup index request did not complete")?;
    #[cfg(test)]
    drop(phase);
    anyhow::ensure!(
        waited.generation() == generation && waited.config() == &config,
        "startup index completion did not match its submitted identity"
    );

    #[cfg(test)]
    let phase =
        track_c_startup_phase_observer::start(track_c_startup_phase_observer::Phase::TakeResult);
    let completion = coordinator
        .take_result()
        .context("startup index completion was not retained for acknowledgement")?;
    #[cfg(test)]
    drop(phase);
    anyhow::ensure!(
        completion.generation() == generation && completion.config() == &config,
        "startup index result changed before acknowledgement"
    );
    let indexed_actions = completion
        .outcome()
        .as_ref()
        .map(Arc::clone)
        .map_err(|error| anyhow::anyhow!("startup index scan failed: {error}"))?;
    #[cfg(test)]
    let phase = track_c_startup_phase_observer::start(
        track_c_startup_phase_observer::Phase::ValidateTransfer,
    );
    coordinator
        .validate_acknowledged_result(generation, &config)
        .context("startup index result failed transfer validation")?;
    #[cfg(test)]
    drop(phase);

    Ok((
        indexed_actions,
        Some(StartupIndexingTransfer {
            coordinator,
            config,
            generation,
        }),
    ))
}

/// Build the exact catalog supplied to startup plugin consumers and the GUI.
/// Keeping assembly here makes the custom-first/indexed-tail boundary shared
/// by startup and its consumer-parity test.
pub(super) fn startup_action_catalog(
    mut custom_actions: Vec<Action>,
    config: IndexConfig,
) -> anyhow::Result<(Arc<Vec<Action>>, usize, Option<StartupIndexingTransfer>)> {
    let mut timer = runtime::performance::track_c::Timer::start(
        runtime::performance::track_c::Phase::StartupCatalogReady,
    );
    let custom_len = custom_actions.len();
    let (indexed_actions, transfer) = match startup_indexed_actions(config) {
        Ok(result) => result,
        Err(error) => {
            timer.finish(runtime::performance::track_c::Outcome::Error);
            return Err(error);
        }
    };
    #[cfg(test)]
    let phase = track_c_startup_phase_observer::start(
        track_c_startup_phase_observer::Phase::CloneIndexedActions,
    );
    custom_actions.extend(indexed_actions.iter().cloned());
    #[cfg(test)]
    drop(phase);
    let work_units = custom_actions.len();
    #[cfg(test)]
    let phase = track_c_startup_phase_observer::start(
        track_c_startup_phase_observer::Phase::WrapCatalogArc,
    );
    let catalog = Arc::new(custom_actions);
    #[cfg(test)]
    drop(phase);
    timer.set_work_units(work_units);
    timer.finish(runtime::performance::track_c::Outcome::Completed);
    Ok((catalog, custom_len, transfer))
}
