use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread::{self, JoinHandle};

const REAPER_CAPACITY: usize = 16;
const RUNNING_UNARMED: u8 = 0;
const ARMED: u8 = 1;
const COMPLETED: u8 = 2;
const DETACHED: u8 = 3;
type SupervisorSpawner = Arc<dyn Fn(Arc<JoinReaper>) -> Result<(), String> + Send + Sync + 'static>;

struct PendingJoin {
    join: JoinHandle<()>,
    completion: Arc<AtomicU8>,
}

#[derive(Default)]
struct ReaperState {
    pending: Vec<PendingJoin>,
    supervisor_active: bool,
    completions: usize,
    #[cfg(test)]
    waits: usize,
}

struct JoinReaper {
    outstanding: AtomicUsize,
    state: Mutex<ReaperState>,
    completed: Condvar,
    spawn_supervisor: SupervisorSpawner,
    #[cfg(test)]
    registration_hook: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

impl JoinReaper {
    fn new(spawn_supervisor: SupervisorSpawner) -> Arc<Self> {
        Arc::new(Self {
            outstanding: AtomicUsize::new(0),
            state: Mutex::new(ReaperState::default()),
            completed: Condvar::new(),
            spawn_supervisor,
            #[cfg(test)]
            registration_hook: Mutex::new(None),
        })
    }

    fn reserve(self: &Arc<Self>) -> Option<ReapPermit> {
        self.outstanding
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                (current < REAPER_CAPACITY).then_some(current + 1)
            })
            .ok()
            .map(|_| ReapPermit {
                owner: Arc::clone(self),
                completion: Arc::new(AtomicU8::new(RUNNING_UNARMED)),
                active: true,
            })
    }

    fn latch_completion(&self, completion: &AtomicU8) {
        let mut state = self.state.lock().unwrap();
        let notify = match completion.load(Ordering::Acquire) {
            RUNNING_UNARMED => {
                completion.store(COMPLETED, Ordering::Release);
                false
            }
            ARMED => {
                completion.store(COMPLETED, Ordering::Release);
                state.completions = state.completions.saturating_add(1);
                true
            }
            DETACHED => {
                drop(state);
                self.outstanding.fetch_sub(1, Ordering::AcqRel);
                return;
            }
            COMPLETED => false,
            value => unreachable!("invalid completion state {value}"),
        };
        drop(state);
        if notify {
            self.completed.notify_one();
        }
    }

    fn submit(
        self: &Arc<Self>,
        join: JoinHandle<()>,
        completion: Arc<AtomicU8>,
    ) -> Result<(), ReapError> {
        let mut state = self.state.lock().unwrap();
        debug_assert!(state.pending.len() < REAPER_CAPACITY);
        state.pending.push(PendingJoin {
            join,
            completion: Arc::clone(&completion),
        });
        #[cfg(test)]
        if let Some(hook) = self.registration_hook.lock().unwrap().clone() {
            hook();
        }
        let completed_before_arm = match completion.compare_exchange(
            RUNNING_UNARMED,
            ARMED,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => false,
            Err(COMPLETED) => {
                state.completions = state.completions.saturating_add(1);
                true
            }
            Err(value) => unreachable!("invalid completion state during registration: {value}"),
        };
        let spawn = !state.supervisor_active;
        state.supervisor_active = true;
        drop(state);
        if completed_before_arm {
            self.completed.notify_one();
        }
        if !spawn {
            return Ok(());
        }

        if let Err(message) = (self.spawn_supervisor)(Arc::clone(self)) {
            let mut state = self.state.lock().unwrap();
            state.supervisor_active = false;
            state.completions = 0;
            let mut completed = 0usize;
            for pending in &state.pending {
                match pending.completion.compare_exchange(
                    ARMED,
                    DETACHED,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => {}
                    Err(COMPLETED) => completed += 1,
                    Err(value) => debug_assert_eq!(value, DETACHED),
                }
            }
            let detached = std::mem::take(&mut state.pending);
            drop(state);
            self.outstanding.fetch_sub(completed, Ordering::AcqRel);
            for pending in detached {
                drop(pending.join);
            }
            return Err(ReapError::SupervisorSpawn(message));
        }
        Ok(())
    }

    fn supervise(self: Arc<Self>) {
        loop {
            let finished = {
                let mut state = self.state.lock().unwrap();
                while state.completions == 0 {
                    if state.pending.is_empty() {
                        state.supervisor_active = false;
                        return;
                    }
                    #[cfg(test)]
                    {
                        state.waits += 1;
                    }
                    state = self.completed.wait(state).unwrap();
                }
                state.completions -= 1;
                // CompletionNotifier is the last worker-owned guard. Its
                // latched state proves user work returned even if the OS has
                // not yet marked JoinHandle::is_finished during the epilogue.
                let index = state
                    .pending
                    .iter()
                    .position(|pending| pending.completion.load(Ordering::Acquire) == COMPLETED)
                    .expect("completion count must name a registered handle");
                state.pending.swap_remove(index)
            };
            let _ = finished.join.join();
            self.outstanding.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

fn global_reaper() -> &'static Arc<JoinReaper> {
    static REAPER: OnceLock<Arc<JoinReaper>> = OnceLock::new();
    REAPER.get_or_init(|| {
        JoinReaper::new(Arc::new(|owner| {
            thread::Builder::new()
                .name("multi-launcher-join-reaper".into())
                .spawn(move || owner.supervise())
                .map(drop)
                .map_err(|error| error.to_string())
        }))
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReapError {
    SupervisorSpawn(String),
}

impl std::fmt::Display for ReapError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SupervisorSpawn(message) => {
                write!(
                    formatter,
                    "failed to spawn bounded join supervisor: {message}"
                )
            }
        }
    }
}

/// A slot reserved before worker creation. Normal joins release it on drop;
/// faulted workers transfer it with their handle to the completion supervisor.
pub(crate) struct ReapPermit {
    owner: Arc<JoinReaper>,
    completion: Arc<AtomicU8>,
    active: bool,
}

impl ReapPermit {
    pub(crate) fn completion_notifier(&self) -> CompletionNotifier {
        CompletionNotifier {
            owner: Arc::clone(&self.owner),
            completion: Arc::clone(&self.completion),
        }
    }

    pub(crate) fn reap(mut self, join: JoinHandle<()>) -> Result<(), ReapError> {
        let result = self.owner.submit(join, Arc::clone(&self.completion));
        self.active = false;
        result
    }
}

/// Must be moved into the worker wrapper after all other locals. Its drop is
/// the sole wakeup used by the faulted-handle supervisor.
pub(crate) struct CompletionNotifier {
    owner: Arc<JoinReaper>,
    completion: Arc<AtomicU8>,
}

impl Drop for CompletionNotifier {
    fn drop(&mut self) {
        self.owner.latch_completion(&self.completion);
    }
}

impl Drop for ReapPermit {
    fn drop(&mut self) {
        if self.active {
            self.owner.outstanding.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

pub(crate) fn reserve() -> Result<ReapPermit, &'static str> {
    global_reaper()
        .reserve()
        .ok_or("bounded worker join capacity exhausted")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn test_reaper() -> Arc<JoinReaper> {
        JoinReaper::new(Arc::new(|owner| {
            thread::Builder::new()
                .spawn(move || owner.supervise())
                .map(drop)
                .map_err(|error| error.to_string())
        }))
    }

    fn wait_until_reclaimed(reaper: &JoinReaper) {
        while reaper.outstanding.load(Ordering::Acquire) != 0
            || reaper.state.lock().unwrap().supervisor_active
        {
            thread::yield_now();
        }
    }

    #[test]
    fn hung_handle_does_not_block_completed_reclamation_and_capacity_is_bounded() {
        let reaper = test_reaper();
        let (release_hung_tx, release_hung_rx) = mpsc::sync_channel(1);
        let hung_permit = reaper.reserve().unwrap();
        let hung_completed = hung_permit.completion_notifier();
        let hung = thread::spawn(move || {
            let _ = release_hung_rx.recv();
            drop(hung_completed);
        });
        hung_permit.reap(hung).unwrap();

        let mut held = Vec::new();
        for _ in 1..REAPER_CAPACITY {
            held.push(reaper.reserve().unwrap());
        }
        assert!(
            reaper.reserve().is_none(),
            "capacity must fail before another worker is created"
        );

        drop(held.pop());
        let completed_permit = reaper.reserve().unwrap();
        let completed = completed_permit.completion_notifier();
        completed_permit
            .reap(thread::spawn(move || drop(completed)))
            .unwrap();
        while reaper.outstanding.load(Ordering::Acquire) == REAPER_CAPACITY {
            thread::yield_now();
        }
        assert_eq!(
            reaper.outstanding.load(Ordering::Acquire),
            REAPER_CAPACITY - 1
        );
        while reaper.state.lock().unwrap().waits < 2 {
            thread::yield_now();
        }
        let state = reaper.state.lock().unwrap();
        assert!(state.supervisor_active);
        let waits_with_hung_worker = state.waits;
        drop(state);
        for _ in 0..100 {
            thread::yield_now();
        }
        assert_eq!(reaper.state.lock().unwrap().waits, waits_with_hung_worker);

        drop(held);
        release_hung_tx.send(()).unwrap();
        while reaper.outstanding.load(Ordering::Acquire) != 0 {
            thread::yield_now();
        }
    }

    #[test]
    fn supervisor_spawn_failure_detaches_without_panicking_and_retains_capacity_until_exit() {
        let spawn_attempts = Arc::new(AtomicUsize::new(0));
        let attempts = Arc::clone(&spawn_attempts);
        let reaper = JoinReaper::new(Arc::new(move |_| {
            attempts.fetch_add(1, Ordering::AcqRel);
            Err("injected supervisor failure".into())
        }));
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let permit = reaper.reserve().unwrap();
        let completed = permit.completion_notifier();
        let join = thread::spawn(move || {
            let _ = release_rx.recv();
            drop(completed);
        });

        assert_eq!(
            permit.reap(join),
            Err(ReapError::SupervisorSpawn(
                "injected supervisor failure".into()
            ))
        );
        assert_eq!(spawn_attempts.load(Ordering::Acquire), 1);
        assert_eq!(reaper.outstanding.load(Ordering::Acquire), 1);
        let mut held = Vec::new();
        for _ in 1..REAPER_CAPACITY {
            held.push(reaper.reserve().unwrap());
        }
        assert!(reaper.reserve().is_none());
        drop(held);
        release_tx.send(()).unwrap();
        while reaper.outstanding.load(Ordering::Acquire) != 0 {
            thread::yield_now();
        }
    }

    #[test]
    fn completion_before_reap_is_latched_even_while_join_is_not_finished() {
        let reaper = test_reaper();
        let permit = reaper.reserve().unwrap();
        let notifier = permit.completion_notifier();
        let (latched_tx, latched_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let join = thread::spawn(move || {
            drop(notifier);
            latched_tx.send(()).unwrap();
            let _ = release_rx.recv();
        });
        latched_rx.recv().unwrap();
        assert!(!join.is_finished());

        permit.reap(join).unwrap();
        release_tx.send(()).unwrap();
        wait_until_reclaimed(&reaper);
    }

    #[test]
    fn completion_during_registration_cannot_be_lost_before_arm() {
        let reaper = test_reaper();
        let (inserted_tx, inserted_rx) = mpsc::sync_channel(1);
        let (release_registration_tx, release_registration_rx) = mpsc::sync_channel(1);
        let release_registration_rx = Arc::new(Mutex::new(release_registration_rx));
        *reaper.registration_hook.lock().unwrap() = Some(Arc::new(move || {
            inserted_tx.send(()).unwrap();
            let _ = release_registration_rx.lock().unwrap().recv();
        }));

        let permit = reaper.reserve().unwrap();
        let notifier = permit.completion_notifier();
        let (drop_now_tx, drop_now_rx) = mpsc::sync_channel(1);
        let (dropping_tx, dropping_rx) = mpsc::sync_channel(1);
        let join = thread::spawn(move || {
            let _ = drop_now_rx.recv();
            dropping_tx.send(()).unwrap();
            drop(notifier);
        });
        let (reaped_tx, reaped_rx) = mpsc::sync_channel(1);
        thread::spawn(move || reaped_tx.send(permit.reap(join)).unwrap());

        inserted_rx.recv().unwrap();
        drop_now_tx.send(()).unwrap();
        dropping_rx.recv().unwrap();
        assert!(reaped_rx.try_recv().is_err());
        release_registration_tx.send(()).unwrap();
        reaped_rx.recv().unwrap().unwrap();
        wait_until_reclaimed(&reaper);
    }

    #[test]
    fn completion_after_arm_notifies_once_and_reclaims_without_polling() {
        let reaper = test_reaper();
        let permit = reaper.reserve().unwrap();
        let notifier = permit.completion_notifier();
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let join = thread::spawn(move || {
            let _ = release_rx.recv();
            drop(notifier);
        });
        permit.reap(join).unwrap();
        while reaper.state.lock().unwrap().waits == 0 {
            thread::yield_now();
        }
        release_tx.send(()).unwrap();
        wait_until_reclaimed(&reaper);
    }
}
