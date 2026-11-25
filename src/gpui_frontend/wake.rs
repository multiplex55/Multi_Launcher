//! Coalesced completion notifications for retained GPUI owners.
use super::*;

/// Coalescing wake signal for the one retained HashSession job.
///
/// The receiver awaits this future instead of blocking a GPUI executor worker.
/// The signal is closed from app/root teardown so the stored task waker cannot
/// retain an abandoned receiver task.
#[derive(Default)]
pub(super) struct HashWakeSignal {
    pub(super) state: Mutex<HashWakeState>,
}

#[derive(Default)]
pub(super) struct HashWakeState {
    pending: bool,
    pub(super) closed: bool,
    waker: Option<Waker>,
}

impl HashWakeSignal {
    pub(super) fn notify(&self) {
        let waker = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.closed {
                return;
            }
            state.pending = true;
            state.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    pub(super) fn close(&self) {
        let waker = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.closed = true;
            state.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    pub(super) fn wait(self: &Arc<Self>) -> HashWakeFuture {
        HashWakeFuture {
            signal: Arc::clone(self),
        }
    }
}

pub(super) struct HashWakeFuture {
    signal: Arc<HashWakeSignal>,
}

impl Future for HashWakeFuture {
    type Output = bool;

    fn poll(self: Pin<&mut Self>, cx: &mut FutureContext<'_>) -> Poll<Self::Output> {
        let mut state = self
            .get_mut()
            .signal
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.closed {
            Poll::Ready(false)
        } else if state.pending {
            state.pending = false;
            Poll::Ready(true)
        } else {
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

impl Drop for HashWakeFuture {
    fn drop(&mut self) {
        self.signal
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .waker
            .take();
    }
}
