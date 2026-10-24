use super::ImageCompareError;
use super::budget::{ImageBudgetLease, ImageBudgetSnapshot, ImageCompareBudget};
use super::compare::{ImageComparison, compare_inputs};
use super::input::{
    ImageInput, ImageInputSource, ImageSide, LoadedInput, load_clipboard, load_file,
};
use crate::thread_reaper::{self, ReapPermit};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ImageLoadStatus {
    Empty,
    Loading { revision: u64 },
    Ready { revision: u64 },
    Failed { revision: u64, message: String },
    Cancelled { revision: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ImageSessionError {
    Closed,
    RetiringWorker,
    RevisionExhausted,
    RequestSequenceExhausted,
    WorkerStart(String),
}

impl std::fmt::Display for ImageSessionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => write!(formatter, "image comparison session is closed"),
            Self::RetiringWorker => write!(
                formatter,
                "the previous image comparison worker is still retiring"
            ),
            Self::RevisionExhausted => write!(formatter, "image input revision space is exhausted"),
            Self::RequestSequenceExhausted => {
                write!(formatter, "image request sequence space is exhausted")
            }
            Self::WorkerStart(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for ImageSessionError {}

pub(crate) struct ImageCompareSnapshot {
    pub(crate) input_a: Option<ImageInput>,
    pub(crate) input_b: Option<ImageInput>,
    pub(crate) comparison: Option<ImageComparison>,
    pub(crate) status_a: ImageLoadStatus,
    pub(crate) status_b: ImageLoadStatus,
    pub(crate) revisions: [u64; 2],
    pub(crate) closed: bool,
}

impl ImageCompareSnapshot {
    pub(crate) fn input(&self, side: ImageSide) -> Option<&ImageInput> {
        match side {
            ImageSide::A => self.input_a.as_ref(),
            ImageSide::B => self.input_b.as_ref(),
        }
    }

    pub(crate) fn status(&self, side: ImageSide) -> &ImageLoadStatus {
        match side {
            ImageSide::A => &self.status_a,
            ImageSide::B => &self.status_b,
        }
    }
}

#[derive(Clone)]
enum LoadSource {
    File(PathBuf),
    Clipboard,
    #[cfg(test)]
    RawRgba {
        width: u32,
        height: u32,
        pixels: Vec<u8>,
        label: String,
    },
}

struct LoadRequest {
    side: ImageSide,
    revision: u64,
    sequence: u64,
    source: LoadSource,
}

struct ActiveRequest {
    side: ImageSide,
    revision: u64,
    cancelled: Arc<AtomicBool>,
}

struct SessionState {
    closed: bool,
    revisions: [u64; 2],
    next_sequence: u64,
    pending: [Option<LoadRequest>; 2],
    active: Option<ActiveRequest>,
    accepted: [Option<ImageInput>; 2],
    comparison: Option<ImageComparison>,
    statuses: [ImageLoadStatus; 2],
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            closed: false,
            revisions: [0, 0],
            next_sequence: 1,
            pending: [None, None],
            active: None,
            accepted: [None, None],
            comparison: None,
            statuses: [ImageLoadStatus::Empty, ImageLoadStatus::Empty],
        }
    }
}

struct SessionShared {
    state: Mutex<SessionState>,
    changed: Condvar,
    wake: Arc<dyn Fn() + Send + Sync + 'static>,
}

struct WorkerOwner {
    join: JoinHandle<()>,
    reap_permit: ReapPermit,
    retired: Arc<AtomicBool>,
}

struct WorkerRetirementSignal(Arc<AtomicBool>);

impl Drop for WorkerRetirementSignal {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

trait ImageInputBackend: Send + Sync + 'static {
    fn load(
        &self,
        source: LoadSource,
        budget: &ImageCompareBudget,
        cancelled: &AtomicBool,
    ) -> Result<LoadedInput, ImageCompareError>;
}

struct PlatformImageInputBackend;

impl ImageInputBackend for PlatformImageInputBackend {
    fn load(
        &self,
        source: LoadSource,
        budget: &ImageCompareBudget,
        cancelled: &AtomicBool,
    ) -> Result<LoadedInput, ImageCompareError> {
        match source {
            LoadSource::File(path) => load_file(&path, budget, cancelled),
            LoadSource::Clipboard => load_clipboard(budget, cancelled),
            #[cfg(test)]
            LoadSource::RawRgba {
                width,
                height,
                pixels,
                label,
            } => {
                if cancelled.load(Ordering::Acquire) {
                    return Err(ImageCompareError::Cancelled);
                }
                let rgba_len = super::input::checked_rgba_len(width, height)?;
                let reservation = budget.reserve(rgba_len)?;
                LoadedInput::from_rgba(
                    width,
                    height,
                    pixels,
                    ImageInputSource::Test {
                        display_name: label,
                    },
                    reservation,
                )
            }
        }
    }
}

/// One lazily-created worker with at most one pending request per input side.
/// A newer request replaces only that side's queued request and cancels an
/// active load for the same side; the accepted pair remains available until a
/// replacement and its matching mask are both ready.
pub(crate) struct ImageCompareSession {
    budget: ImageCompareBudget,
    shared: Arc<SessionShared>,
    backend: Arc<dyn ImageInputBackend>,
    worker: Mutex<Option<WorkerOwner>>,
    retired_worker: Mutex<Option<Arc<AtomicBool>>>,
    #[cfg(test)]
    close_transition_hook: Mutex<Option<Arc<dyn Fn() + Send + Sync + 'static>>>,
}

impl Default for ImageCompareSession {
    fn default() -> Self {
        Self::new()
    }
}

impl ImageCompareSession {
    pub(crate) fn new() -> Self {
        Self::with_budget_and_wake(
            ImageCompareBudget::default(),
            Arc::new(PlatformImageInputBackend),
            Arc::new(|| {}),
        )
    }

    pub(crate) fn with_wake(wake: Arc<dyn Fn() + Send + Sync + 'static>) -> Self {
        Self::with_budget_and_wake(
            ImageCompareBudget::default(),
            Arc::new(PlatformImageInputBackend),
            wake,
        )
    }

    fn with_budget_and_wake(
        budget: ImageCompareBudget,
        backend: Arc<dyn ImageInputBackend>,
        wake: Arc<dyn Fn() + Send + Sync + 'static>,
    ) -> Self {
        Self {
            budget,
            shared: Arc::new(SessionShared {
                state: Mutex::new(SessionState::default()),
                changed: Condvar::new(),
                wake,
            }),
            backend,
            worker: Mutex::new(None),
            retired_worker: Mutex::new(None),
            #[cfg(test)]
            close_transition_hook: Mutex::new(None),
        }
    }

    pub(crate) fn replace_file(
        &self,
        side: ImageSide,
        path: PathBuf,
    ) -> Result<u64, ImageSessionError> {
        self.submit(side, LoadSource::File(path))
    }

    pub(crate) fn paste_image(&self, side: ImageSide) -> Result<u64, ImageSessionError> {
        self.submit(side, LoadSource::Clipboard)
    }

    fn submit(&self, side: ImageSide, source: LoadSource) -> Result<u64, ImageSessionError> {
        let revision = {
            let mut state = self.shared.state.lock().unwrap();
            if state.closed {
                return Err(ImageSessionError::Closed);
            }
            let index = side.index();
            let revision = state.revisions[index]
                .checked_add(1)
                .ok_or(ImageSessionError::RevisionExhausted)?;
            let sequence = state.next_sequence;
            state.next_sequence = sequence
                .checked_add(1)
                .ok_or(ImageSessionError::RequestSequenceExhausted)?;
            state.revisions[index] = revision;
            state.pending[index] = Some(LoadRequest {
                side,
                revision,
                sequence,
                source,
            });
            state.statuses[index] = ImageLoadStatus::Loading { revision };
            if let Some(active) = state.active.as_ref()
                && active.side == side
            {
                active.cancelled.store(true, Ordering::Release);
            }
            self.shared.changed.notify_all();
            revision
        };
        if let Err(error) = self.ensure_worker() {
            let mut state = self.shared.state.lock().unwrap();
            let index = side.index();
            if state.revisions[index] == revision {
                state.pending[index] = None;
                state.statuses[index] = ImageLoadStatus::Failed {
                    revision,
                    message: error.to_string(),
                };
            }
            drop(state);
            (self.shared.wake)();
            return Err(error);
        }
        (self.shared.wake)();
        Ok(revision)
    }

    fn ensure_worker(&self) -> Result<(), ImageSessionError> {
        let mut owner = self.worker.lock().unwrap();
        if owner.is_some() {
            return Ok(());
        }
        {
            let state = self.shared.state.lock().unwrap();
            if state.closed {
                return Err(ImageSessionError::Closed);
            }
            if state.pending.iter().all(Option::is_none) {
                return Ok(());
            }
        }
        let reap_permit = thread_reaper::reserve().map_err(|error| {
            ImageSessionError::WorkerStart(format!("image worker capacity is unavailable: {error}"))
        })?;
        let completion = reap_permit.completion_notifier();
        let shared = Arc::clone(&self.shared);
        let budget = self.budget.clone();
        let backend = Arc::clone(&self.backend);
        let retired = Arc::new(AtomicBool::new(false));
        let retirement_signal = Arc::clone(&retired);
        let join = thread::Builder::new()
            .name("image-compare-session".into())
            .spawn(move || {
                // This guard remains until source buffers, decoder scratch, and
                // result leases owned by the loop have all been dropped.
                let completion = completion;
                let _retirement = WorkerRetirementSignal(retirement_signal);
                worker_loop(shared, budget, backend);
                drop(completion);
            })
            .map_err(|error| ImageSessionError::WorkerStart(error.to_string()))?;
        *owner = Some(WorkerOwner {
            join,
            reap_permit,
            retired,
        });
        Ok(())
    }

    /// Reopen this session after close once its prior worker has returned and
    /// dropped all processing-local buffers. The budget and revision namespace
    /// remain attached to this owner, so a closed session cannot be replaced
    /// with a fresh allowance while old snapshots or uploads remain retained.
    pub(crate) fn reopen(&self) -> Result<(), ImageSessionError> {
        let worker = self.worker.lock().unwrap();
        let mut retired_worker = self.retired_worker.lock().unwrap();
        if let Some(retired) = retired_worker.as_ref() {
            if !retired.load(Ordering::Acquire) {
                return Err(ImageSessionError::RetiringWorker);
            }
            *retired_worker = None;
        }
        let mut state = self.shared.state.lock().unwrap();
        if !state.closed {
            return Ok(());
        }
        if worker.is_some() || state.active.is_some() || state.pending.iter().any(Option::is_some) {
            return Err(ImageSessionError::RetiringWorker);
        }
        state.closed = false;
        state.accepted = [None, None];
        state.comparison = None;
        state.statuses = [ImageLoadStatus::Empty, ImageLoadStatus::Empty];
        self.shared.changed.notify_all();
        drop(state);
        drop(retired_worker);
        drop(worker);
        (self.shared.wake)();
        Ok(())
    }

    pub(crate) fn cancel_input(&self, side: ImageSide) -> Result<bool, ImageSessionError> {
        let cancelled = {
            let mut state = self.shared.state.lock().unwrap();
            if state.closed {
                return Err(ImageSessionError::Closed);
            }
            let index = side.index();
            let had_request = state.pending[index].take().is_some()
                || state
                    .active
                    .as_ref()
                    .is_some_and(|active| active.side == side);
            if !had_request {
                return Ok(false);
            }
            let revision = state.revisions[index]
                .checked_add(1)
                .ok_or(ImageSessionError::RevisionExhausted)?;
            state.revisions[index] = revision;
            if let Some(active) = state.active.as_ref()
                && active.side == side
            {
                active.cancelled.store(true, Ordering::Release);
            }
            state.statuses[index] = ImageLoadStatus::Cancelled { revision };
            self.shared.changed.notify_all();
            true
        };
        (self.shared.wake)();
        Ok(cancelled)
    }

    pub(crate) fn snapshot(&self) -> ImageCompareSnapshot {
        let state = self.shared.state.lock().unwrap();
        ImageCompareSnapshot {
            input_a: state.accepted[0].clone(),
            input_b: state.accepted[1].clone(),
            comparison: state.comparison.clone(),
            status_a: state.statuses[0].clone(),
            status_b: state.statuses[1].clone(),
            revisions: state.revisions,
            closed: state.closed,
        }
    }

    pub(crate) fn budget_snapshot(&self) -> ImageBudgetSnapshot {
        self.budget.snapshot()
    }

    pub(crate) fn reserve_retained_representation(
        &self,
        bytes: usize,
    ) -> Result<ImageBudgetLease, super::budget::BudgetExceeded> {
        self.budget.reserve_retained_representation(bytes)
    }

    pub(crate) fn is_busy(&self) -> bool {
        let state = self.shared.state.lock().unwrap();
        state.active.is_some() || state.pending.iter().any(Option::is_some)
    }

    pub(crate) fn close(&self) {
        // Linearize close with ensure_worker/reopen under the worker slot.
        // submit releases the state lock before it needs this slot, so this
        // worker -> state order has no reverse nested acquisition.
        let mut worker_slot = self.worker.lock().unwrap();
        {
            let mut state = self.shared.state.lock().unwrap();
            if !state.closed {
                state.closed = true;
                for pending in &mut state.pending {
                    *pending = None;
                }
                if let Some(active) = state.active.as_ref() {
                    active.cancelled.store(true, Ordering::Release);
                }
                state.accepted = [None, None];
                state.comparison = None;
                state.statuses = [ImageLoadStatus::Empty, ImageLoadStatus::Empty];
                self.shared.changed.notify_all();
            }
        }
        #[cfg(test)]
        if let Some(hook) = self.close_transition_hook.lock().unwrap().clone() {
            hook();
        }
        if let Some(WorkerOwner {
            join,
            reap_permit,
            retired,
        }) = worker_slot.take()
        {
            *self.retired_worker.lock().unwrap() = Some(retired);
            if let Err(error) = reap_permit.reap(join) {
                tracing::error!(%error, "failed to transfer image comparison worker to join reaper");
            }
        }
        drop(worker_slot);
        (self.shared.wake)();
    }

    #[cfg(test)]
    fn with_test_backend(budget: ImageCompareBudget, backend: Arc<dyn ImageInputBackend>) -> Self {
        Self::with_budget_and_wake(budget, backend, Arc::new(|| {}))
    }

    #[cfg(test)]
    fn set_close_transition_hook_for_test(&self, hook: Arc<dyn Fn() + Send + Sync + 'static>) {
        *self.close_transition_hook.lock().unwrap() = Some(hook);
    }

    #[cfg(test)]
    fn submit_raw_for_test(
        &self,
        side: ImageSide,
        width: u32,
        height: u32,
        pixels: Vec<u8>,
        label: &str,
    ) -> Result<u64, ImageSessionError> {
        self.submit(
            side,
            LoadSource::RawRgba {
                width,
                height,
                pixels,
                label: label.to_owned(),
            },
        )
    }

    #[cfg(test)]
    fn reaper_probe_for_test(&self) -> Option<crate::thread_reaper::ReapCompletionProbe> {
        self.worker
            .lock()
            .unwrap()
            .as_ref()
            .map(|worker| worker.reap_permit.reaped_probe())
    }
}

impl Drop for ImageCompareSession {
    fn drop(&mut self) {
        self.close();
    }
}

fn worker_loop(
    shared: Arc<SessionShared>,
    budget: ImageCompareBudget,
    backend: Arc<dyn ImageInputBackend>,
) {
    loop {
        let (request, cancelled) = {
            let mut state = shared.state.lock().unwrap();
            loop {
                if state.closed {
                    return;
                }
                let selected = state
                    .pending
                    .iter()
                    .enumerate()
                    .filter_map(|(index, request)| {
                        request.as_ref().map(|request| (index, request.sequence))
                    })
                    .min_by_key(|(_, sequence)| *sequence)
                    .map(|(index, _)| index);
                if let Some(index) = selected {
                    let request = state.pending[index]
                        .take()
                        .expect("selected request is present");
                    let cancelled = Arc::new(AtomicBool::new(false));
                    state.active = Some(ActiveRequest {
                        side: request.side,
                        revision: request.revision,
                        cancelled: Arc::clone(&cancelled),
                    });
                    break (request, cancelled);
                }
                state = shared.changed.wait(state).unwrap();
            }
        };

        let result = process_request(&shared, &budget, backend.as_ref(), &request, &cancelled);
        {
            let mut state = shared.state.lock().unwrap();
            if state.active.as_ref().is_some_and(|active| {
                active.side == request.side && active.revision == request.revision
            }) {
                state.active = None;
            }
            let index = request.side.index();
            if !state.closed && state.revisions[index] == request.revision {
                match result {
                    Ok((input, comparison)) => {
                        state.accepted[index] = Some(input);
                        state.comparison = comparison;
                        state.statuses[index] = ImageLoadStatus::Ready {
                            revision: request.revision,
                        };
                    }
                    Err(ImageCompareError::Cancelled) => {
                        state.statuses[index] = ImageLoadStatus::Cancelled {
                            revision: request.revision,
                        };
                    }
                    Err(error) => {
                        state.statuses[index] = ImageLoadStatus::Failed {
                            revision: request.revision,
                            message: error.to_string(),
                        };
                    }
                }
            }
            shared.changed.notify_all();
        }
        (shared.wake)();
    }
}

fn process_request(
    shared: &SessionShared,
    budget: &ImageCompareBudget,
    backend: &dyn ImageInputBackend,
    request: &LoadRequest,
    cancelled: &AtomicBool,
) -> Result<(ImageInput, Option<ImageComparison>), ImageCompareError> {
    let loaded = backend.load(request.source.clone(), budget, cancelled)?;
    if cancelled.load(Ordering::Acquire) {
        return Err(ImageCompareError::Cancelled);
    }
    let input = ImageInput::from_loaded(loaded, request.revision)?;
    let other = {
        let state = shared.state.lock().unwrap();
        if state.closed || state.revisions[request.side.index()] != request.revision {
            return Err(ImageCompareError::Cancelled);
        }
        state.accepted[request.side.other().index()].clone()
    };
    let comparison = other
        .as_ref()
        .map(|other| match request.side {
            ImageSide::A => {
                compare_inputs(&input, other, budget, || cancelled.load(Ordering::Acquire))
            }
            ImageSide::B => {
                compare_inputs(other, &input, budget, || cancelled.load(Ordering::Acquire))
            }
        })
        .transpose()?;
    if cancelled.load(Ordering::Acquire) {
        return Err(ImageCompareError::Cancelled);
    }
    Ok((input, comparison))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    struct GateBackend {
        first_label: String,
        entered: mpsc::SyncSender<()>,
        release: Mutex<mpsc::Receiver<()>>,
        base: PlatformImageInputBackend,
        loads: std::sync::atomic::AtomicUsize,
    }

    impl ImageInputBackend for GateBackend {
        fn load(
            &self,
            source: LoadSource,
            budget: &ImageCompareBudget,
            cancelled: &AtomicBool,
        ) -> Result<LoadedInput, ImageCompareError> {
            self.loads.fetch_add(1, Ordering::AcqRel);
            if matches!(&source, LoadSource::RawRgba { label, .. } if label == &self.first_label) {
                let _ = self.entered.send(());
                let _ = self.release.lock().unwrap().recv();
                if cancelled.load(Ordering::Acquire) {
                    return Err(ImageCompareError::Cancelled);
                }
            }
            self.base.load(source, budget, cancelled)
        }
    }

    fn wait_for(session: &ImageCompareSession, predicate: impl Fn(&SessionState) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut state = session.shared.state.lock().unwrap();
        while !predicate(&state) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "timed out waiting for image session state"
            );
            let (next, timeout) = session
                .shared
                .changed
                .wait_timeout(state, remaining)
                .unwrap();
            state = next;
            assert!(
                !timeout.timed_out() || predicate(&state),
                "timed out waiting for image session state"
            );
        }
    }

    fn wait_ready(session: &ImageCompareSession, side: ImageSide, revision: u64) {
        wait_for(session, |state| {
            state.statuses[side.index()] == ImageLoadStatus::Ready { revision }
        });
    }

    #[test]
    fn image_compare_session_failed_replacement_keeps_last_good_pair_and_result() {
        let temp = tempfile::tempdir().unwrap();
        let a_path = temp.path().join("a.png");
        let b_path = temp.path().join("b.png");
        image::RgbaImage::from_pixel(2, 1, image::Rgba([1, 2, 3, 4]))
            .save(&a_path)
            .unwrap();
        image::RgbaImage::from_pixel(2, 1, image::Rgba([1, 2, 3, 5]))
            .save(&b_path)
            .unwrap();
        let session = ImageCompareSession::new();
        let a_revision = session.replace_file(ImageSide::A, a_path).unwrap();
        wait_ready(&session, ImageSide::A, a_revision);
        let b_revision = session.replace_file(ImageSide::B, b_path).unwrap();
        wait_ready(&session, ImageSide::B, b_revision);
        let before = session.snapshot();
        let old_ids = (
            before.input_a.as_ref().unwrap().id(),
            before.input_b.as_ref().unwrap().id(),
        );
        let old_result = before.comparison.unwrap();

        let failed_revision = session
            .replace_file(ImageSide::A, temp.path().join("missing.png"))
            .unwrap();
        wait_for(
            &session,
            |state| matches!(state.statuses[0], ImageLoadStatus::Failed { revision, .. } if revision == failed_revision),
        );
        let after = session.snapshot();
        assert_eq!(after.input_a.as_ref().unwrap().id(), old_ids.0);
        assert_eq!(after.input_b.as_ref().unwrap().id(), old_ids.1);
        assert_eq!(
            after.comparison.as_ref().unwrap().input_ids(),
            old_result.input_ids()
        );
        assert_eq!(
            after.comparison.as_ref().unwrap().classes(),
            old_result.classes()
        );
        session.close();
    }

    #[test]
    fn image_compare_session_coalesces_same_side_and_keeps_independent_revisions() {
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let backend = Arc::new(GateBackend {
            first_label: "blocked".into(),
            entered: entered_tx,
            release: Mutex::new(release_rx),
            base: PlatformImageInputBackend,
            loads: std::sync::atomic::AtomicUsize::new(0),
        });
        let session =
            ImageCompareSession::with_test_backend(ImageCompareBudget::default(), backend.clone());
        let first = session
            .submit_raw_for_test(ImageSide::A, 1, 1, vec![1, 1, 1, 255], "blocked")
            .unwrap();
        entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        let superseded = session
            .submit_raw_for_test(ImageSide::A, 1, 1, vec![2, 2, 2, 255], "superseded")
            .unwrap();
        let latest = session
            .submit_raw_for_test(ImageSide::A, 1, 1, vec![3, 3, 3, 255], "latest")
            .unwrap();
        assert!(first < superseded && superseded < latest);
        release_tx.send(()).unwrap();
        wait_ready(&session, ImageSide::A, latest);
        let snapshot = session.snapshot();
        assert_eq!(snapshot.input_a.as_ref().unwrap().pixels(), &[3, 3, 3, 255]);
        assert_eq!(snapshot.input_a.as_ref().unwrap().revision(), latest);
        assert_eq!(backend.loads.load(Ordering::Acquire), 2);

        let b = session
            .submit_raw_for_test(ImageSide::B, 1, 1, vec![3, 3, 3, 255], "B")
            .unwrap();
        wait_ready(&session, ImageSide::B, b);
        let paired = session.snapshot();
        assert_eq!(paired.revisions, [latest, b]);
        assert_eq!(paired.comparison.unwrap().input_revisions(), (latest, b));
        session.close();
    }

    #[test]
    fn image_compare_session_cancelled_replacement_discards_late_pixels_and_keeps_accepted_input() {
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let backend = Arc::new(GateBackend {
            first_label: "slow replacement".into(),
            entered: entered_tx,
            release: Mutex::new(release_rx),
            base: PlatformImageInputBackend,
            loads: std::sync::atomic::AtomicUsize::new(0),
        });
        let session =
            ImageCompareSession::with_test_backend(ImageCompareBudget::default(), backend);
        let accepted_revision = session
            .submit_raw_for_test(ImageSide::A, 1, 1, vec![1, 2, 3, 4], "accepted")
            .unwrap();
        wait_ready(&session, ImageSide::A, accepted_revision);
        let accepted_id = session.snapshot().input_a.unwrap().id();

        let stale_revision = session
            .submit_raw_for_test(ImageSide::A, 1, 1, vec![9, 8, 7, 6], "slow replacement")
            .unwrap();
        entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(session.cancel_input(ImageSide::A).unwrap());
        let cancelled_revision = session.snapshot().revisions[0];
        assert!(cancelled_revision > stale_revision);
        release_tx.send(()).unwrap();
        wait_for(&session, |state| {
            state.active.is_none()
                && state.statuses[0]
                    == ImageLoadStatus::Cancelled {
                        revision: cancelled_revision,
                    }
        });
        let after = session.snapshot();
        assert_eq!(after.input_a.unwrap().id(), accepted_id);
        assert!(after.comparison.is_none());
        session.close();
    }

    #[test]
    fn image_compare_session_close_reopen_reaps_old_worker_and_releases_its_lease() {
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let backend = Arc::new(GateBackend {
            first_label: "blocked-close".into(),
            entered: entered_tx,
            release: Mutex::new(release_rx),
            base: PlatformImageInputBackend,
            loads: std::sync::atomic::AtomicUsize::new(0),
        });
        let budget = ImageCompareBudget::with_limit(1024);
        let session = ImageCompareSession::with_test_backend(budget.clone(), backend);
        let accepted = session
            .submit_raw_for_test(ImageSide::A, 2, 2, vec![1; 16], "retained-A")
            .unwrap();
        wait_ready(&session, ImageSide::A, accepted);
        let retained_snapshot = session.snapshot();
        let blocked = session
            .submit_raw_for_test(ImageSide::B, 2, 2, vec![2; 16], "blocked-close")
            .unwrap();
        entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        let reaped = session.reaper_probe_for_test().unwrap();
        session.close();
        assert!(session.snapshot().closed);
        assert_eq!(session.snapshot().revisions[1], blocked);
        assert_eq!(budget.snapshot().used_bytes, 16);
        assert!(!reaped.is_reaped(), "noninterruptible loader remains owned");
        assert_eq!(session.reopen(), Err(ImageSessionError::RetiringWorker));
        assert_eq!(
            session.submit_raw_for_test(ImageSide::B, 1, 1, vec![9; 4], "too early"),
            Err(ImageSessionError::Closed)
        );

        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !reaped.is_reaped() {
            assert!(Instant::now() < deadline, "old image worker was not reaped");
            thread::yield_now();
        }
        assert!(session.reopen().is_ok());
        let revision = session
            .submit_raw_for_test(
                ImageSide::B,
                1,
                1,
                vec![9, 8, 7, 6],
                "new same-owner request",
            )
            .unwrap();
        assert!(
            revision > blocked,
            "reopen preserves the revision namespace"
        );
        wait_ready(&session, ImageSide::B, revision);
        assert_eq!(
            session.snapshot().input_b.as_ref().unwrap().pixels(),
            &[9, 8, 7, 6]
        );
        assert_eq!(budget.snapshot().used_bytes, 20);
        drop(retained_snapshot);
        assert_eq!(budget.snapshot().used_bytes, 4);
        let current_worker_reaped = session.reaper_probe_for_test().unwrap();
        session.close();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !current_worker_reaped.is_reaped() {
            assert!(
                Instant::now() < deadline,
                "reopened image worker was not reaped"
            );
            thread::yield_now();
        }
        assert_eq!(budget.snapshot().used_bytes, 0);
    }

    #[test]
    fn image_compare_session_close_holds_reopen_gate_across_owner_transition() {
        let session = Arc::new(ImageCompareSession::with_test_backend(
            ImageCompareBudget::default(),
            Arc::new(PlatformImageInputBackend),
        ));
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let release_rx = Mutex::new(release_rx);
        session.set_close_transition_hook_for_test(Arc::new(move || {
            let _ = entered_tx.send(());
            let _ = release_rx.lock().unwrap().recv();
        }));

        let closing = Arc::clone(&session);
        let close_thread = thread::spawn(move || closing.close());
        entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(matches!(
            session.worker.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ));
        release_tx.send(()).unwrap();
        close_thread.join().unwrap();
        assert!(session.snapshot().closed);
        session.reopen().unwrap();
        assert!(!session.snapshot().closed);
    }

    #[test]
    fn image_compare_session_replacement_peak_refusal_preserves_last_good_and_retained_upload() {
        let budget = ImageCompareBudget::with_limit(64);
        let session = ImageCompareSession::with_test_backend(
            budget.clone(),
            Arc::new(PlatformImageInputBackend),
        );
        let a_revision = session
            .submit_raw_for_test(ImageSide::A, 2, 2, vec![1; 16], "A")
            .unwrap();
        wait_ready(&session, ImageSide::A, a_revision);
        let b_revision = session
            .submit_raw_for_test(ImageSide::B, 2, 2, vec![2; 16], "B")
            .unwrap();
        wait_ready(&session, ImageSide::B, b_revision);
        let before = session.snapshot();
        let old_ids = (before.input_a.unwrap().id(), before.input_b.unwrap().id());
        let old_result_ids = before.comparison.unwrap().input_ids();
        let retained_upload = session.reserve_retained_representation(12).unwrap();

        let replacement_revision = session
            .submit_raw_for_test(ImageSide::A, 2, 2, vec![3; 16], "replacement")
            .unwrap();
        wait_for(
            &session,
            |state| matches!(state.statuses[0], ImageLoadStatus::Failed { revision, .. } if revision == replacement_revision),
        );
        let mut after = session.snapshot();
        assert_eq!(after.input_a.as_ref().unwrap().id(), old_ids.0);
        assert_eq!(after.input_b.as_ref().unwrap().id(), old_ids.1);
        assert_eq!(
            after.comparison.as_ref().unwrap().input_ids(),
            old_result_ids
        );
        assert_eq!(budget.snapshot().used_bytes, 48);
        drop(retained_upload);
        assert_eq!(budget.snapshot().used_bytes, 36);
        drop(after.comparison.take());
        let worker_reaped = session.reaper_probe_for_test().unwrap();
        session.close();
        assert_eq!(
            budget.snapshot().used_bytes,
            32,
            "the retained snapshot continues to own both accepted pixel buffers"
        );
        drop(after);
        let deadline = Instant::now() + Duration::from_secs(3);
        while !worker_reaped.is_reaped() {
            assert!(
                Instant::now() < deadline,
                "the failed replacement worker was not reaped"
            );
            thread::yield_now();
        }
        assert_eq!(budget.snapshot().used_bytes, 0);
    }
}
