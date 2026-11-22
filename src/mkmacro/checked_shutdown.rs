use std::sync::{Condvar, Mutex};

#[derive(Default)]
struct State {
    running: bool,
    result: Option<Result<(), String>>,
}

/// Serializes one real owner retirement and publishes its exact terminal
/// outcome to every caller, including Drop after an explicit checked join.
#[derive(Default)]
pub(crate) struct CheckedShutdown {
    state: Mutex<State>,
    changed: Condvar,
}

impl CheckedShutdown {
    pub(crate) fn run(
        &self,
        panic_message: &'static str,
        retire: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            loop {
                if let Some(result) = state.result.as_ref() {
                    return result.clone();
                }
                if !state.running {
                    state.running = true;
                    break;
                }
                state = self
                    .changed
                    .wait(state)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        }

        let (result, panic_payload) = match
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(retire))
        {
            Ok(result) => (result, None),
            Err(payload) => (Err(panic_message.to_owned()), Some(payload)),
        };
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.running = false;
        state.result = Some(result.clone());
        self.changed.notify_all();
        drop(state);
        // A panic payload can run arbitrary destructor code, including a
        // reentrant shutdown call. Publish the terminal receipt first.
        drop(panic_payload);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::CheckedShutdown;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn repeated_shutdown_returns_the_original_terminal_receipt() {
        let shutdown = CheckedShutdown::default();
        let retirements = AtomicUsize::new(0);

        let first = shutdown.run("retirement panicked", || {
            retirements.fetch_add(1, Ordering::SeqCst);
            Err("worker join failed".to_owned())
        });
        let second = shutdown.run("retirement panicked", || {
            retirements.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });

        assert_eq!(first, Err("worker join failed".to_owned()));
        assert_eq!(second, first);
        assert_eq!(retirements.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn panic_payload_can_reenter_after_terminal_receipt_is_published() {
        use std::sync::{Arc, mpsc};
        use std::time::Duration;

        struct ReenterOnDrop {
            shutdown: Arc<CheckedShutdown>,
            observed: mpsc::Sender<Result<(), String>>,
        }
        impl Drop for ReenterOnDrop {
            fn drop(&mut self) {
                let result = self.shutdown.run("unexpected second panic", || {
                    panic!("a terminal owner must not be retired twice")
                });
                let _ = self.observed.send(result);
            }
        }

        let shutdown = Arc::new(CheckedShutdown::default());
        let reentrant_shutdown = Arc::clone(&shutdown);
        let (observed, receiver) = mpsc::channel();
        let result = shutdown.run("owner retirement panicked", move || {
            std::panic::panic_any(ReenterOnDrop {
                shutdown: reentrant_shutdown,
                observed,
            })
        });

        assert_eq!(result, Err("owner retirement panicked".to_owned()));
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(1)).unwrap(),
            result
        );
    }
}
