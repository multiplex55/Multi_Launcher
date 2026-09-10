//! Ordered recorder event processing. OS enrichment and normalization live on this
//! blocking worker, never on a GUI snapshot path or low-level hook callback.
use super::{
    ClickInspection, ClipboardObservation, EventEnricher, HookEvent, KeyTransition,
    KeyboardTranslator, MkPoint, MouseMessage, NormalizationConfig, ObservationBaseline,
    ObservationResult, RecordedStep, RecorderObserverSession, RecordingBoundary, RecordingNote,
    RecordingPlan, RecordingSuggestion, SequencedHookEvent, SystemKeyboardTranslator,
    WindowObservation, WindowsEventEnricher, apply_recording_notes, build_recording_plan,
    capture_process_baseline, discover_suggestions, enrich_keyboard_with_state, normalize,
};
use anyhow::{Result, anyhow};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, RwLock, mpsc},
    thread::{self, JoinHandle},
    time::Duration,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordingTarget {
    pub macro_id: u64,
    pub insertion_anchor_step_id: Option<u64>,
    /// Process-local step-instance generation captured by the authoring dialog.
    /// It distinguishes a surviving row from a delete/recreate that reuses its ID.
    pub insertion_anchor_generation: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct ProcessorSnapshot {
    pub raw_event_count: u64,
    pub estimated_action_count: usize,
}

#[derive(Debug)]
pub struct ProcessorResult {
    pub target: RecordingTarget,
    pub literal_steps: Vec<RecordedStep>,
    pub plan: RecordingPlan,
    pub suggestions: Vec<RecordingSuggestion>,
    pub clipboard_observations: Vec<ClipboardObservation>,
    pub click_inspections: Vec<ClickInspection>,
    pub window_observations: Vec<WindowObservation>,
    pub notes: Vec<RecordingNote>,
    pub raw_event_count: u64,
}

enum Command {
    Begin {
        target: RecordingTarget,
        config: NormalizationConfig,
        floor: u64,
        held_keys: Vec<u32>,
        reply: mpsc::SyncSender<Result<()>>,
    },
    Pause {
        timestamp_us: u64,
        fence: u64,
        occurrence: Vec<u32>,
        reply: mpsc::SyncSender<Result<()>>,
    },
    Resume {
        timestamp_us: u64,
        held_keys: Vec<u32>,
        reply: mpsc::SyncSender<Result<()>>,
    },
    Control {
        fence: u64,
        occurrence: Vec<u32>,
        reply: mpsc::SyncSender<Result<()>>,
    },
    Marker {
        timestamp_us: u64,
        reply: mpsc::SyncSender<Result<()>>,
    },
    Annotation {
        timestamp_us: u64,
        text: String,
        reply: mpsc::SyncSender<Result<()>>,
    },
    Finish {
        fence: u64,
        occurrence: Vec<u32>,
        reply: mpsc::SyncSender<Result<ProcessorResult>>,
    },
    Shutdown {
        reply: mpsc::SyncSender<()>,
    },
    #[cfg(test)]
    PanicForTest {
        _reply: mpsc::SyncSender<()>,
    },
}

struct Session {
    target: RecordingTarget,
    config: NormalizationConfig,
    raw: Vec<RecordingBoundary>,
    pressed: HashMap<u32, DownRun>,
    /// Keys whose down transition was already recorded when capture paused.
    /// A matching up after resume belongs to the recording, unlike keys first
    /// pressed by a pause-owned annotation prompt.
    held_before_pause: HashMap<u32, DownRun>,
    suppressed_until_up: HashSet<u32>,
    suppressed: HashSet<usize>,
    mouse_pressed: HashMap<super::MouseButton, usize>,
    suppressed_mouse_until_up: HashSet<super::MouseButton>,
    backlog: VecDeque<SequencedHookEvent>,
    floor: u64,
    initial_key_state: [u8; 256],
    live_key_state: [bool; 256],
    observations: RecorderObserverSession,
}
struct DownRun {
    index: usize,
    ordinary_key_downs: Vec<usize>,
}

pub struct RecorderProcessor {
    commands: mpsc::Sender<Command>,
    snapshot: Arc<RwLock<Arc<ProcessorSnapshot>>>,
    worker: std::sync::Mutex<Option<JoinHandle<()>>>,
}

impl RecorderProcessor {
    pub fn new(events: mpsc::Receiver<SequencedHookEvent>) -> Self {
        Self::with_translator(events, Box::new(SystemKeyboardTranslator))
    }
    pub fn with_translator(
        events: mpsc::Receiver<SequencedHookEvent>,
        translator: Box<dyn KeyboardTranslator>,
    ) -> Self {
        Self::with_components(
            events,
            translator,
            Arc::new(RecorderObserverSession::production),
        )
    }
    pub fn with_components(
        events: mpsc::Receiver<SequencedHookEvent>,
        translator: Box<dyn KeyboardTranslator>,
        observer_factory: Arc<dyn Fn() -> RecorderObserverSession + Send + Sync>,
    ) -> Self {
        Self::with_all_components(
            events,
            translator,
            observer_factory,
            Box::new(WindowsEventEnricher::default()),
        )
    }
    pub(crate) fn with_all_components(
        events: mpsc::Receiver<SequencedHookEvent>,
        translator: Box<dyn KeyboardTranslator>,
        observer_factory: Arc<dyn Fn() -> RecorderObserverSession + Send + Sync>,
        enricher: Box<dyn EventEnricher>,
    ) -> Self {
        let (commands, command_rx) = mpsc::channel();
        let snapshot = Arc::new(RwLock::new(Arc::new(ProcessorSnapshot::default())));
        let worker_snapshot = snapshot.clone();
        let worker = thread::Builder::new()
            .name("mkmacro-recorder-processor".into())
            .spawn(move || {
                let mut translator = translator;
                let mut enricher = enricher;
                loop {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        worker_loop(
                            &command_rx,
                            &events,
                            &worker_snapshot,
                            translator.as_mut(),
                            &observer_factory,
                            enricher.as_mut(),
                        )
                    }));
                    if result.is_ok() {
                        break;
                    }
                    publish(&worker_snapshot, None);
                }
            })
            .expect("spawn recorder processor");
        Self {
            commands,
            snapshot,
            worker: std::sync::Mutex::new(Some(worker)),
        }
    }

    fn request<T>(&self, build: impl FnOnce(mpsc::SyncSender<T>) -> Command) -> Result<T> {
        let (reply, response) = mpsc::sync_channel(0);
        self.commands
            .send(build(reply))
            .map_err(|_| anyhow!("recorder processor stopped"))?;
        response
            .recv()
            .map_err(|_| anyhow!("recorder processor stopped"))
    }

    pub fn begin(
        &self,
        target: RecordingTarget,
        config: NormalizationConfig,
        floor: u64,
        held_keys: Vec<u32>,
    ) -> Result<()> {
        self.request(|reply| Command::Begin {
            target,
            config,
            floor,
            held_keys,
            reply,
        })?
    }
    pub fn pause(&self, timestamp_us: u64, fence: u64, occurrence: Vec<u32>) -> Result<()> {
        self.request(|reply| Command::Pause {
            timestamp_us,
            fence,
            occurrence,
            reply,
        })?
    }
    pub fn resume(&self, timestamp_us: u64, held_keys: Vec<u32>) -> Result<()> {
        self.request(|reply| Command::Resume {
            timestamp_us,
            held_keys,
            reply,
        })?
    }
    pub fn control_occurrence(&self, fence: u64, occurrence: Vec<u32>) -> Result<()> {
        self.request(|reply| Command::Control {
            fence,
            occurrence,
            reply,
        })?
    }
    pub fn marker(&self, timestamp_us: u64) -> Result<()> {
        self.request(|reply| Command::Marker {
            timestamp_us,
            reply,
        })?
    }
    pub fn annotation(&self, timestamp_us: u64, text: String) -> Result<()> {
        self.request(|reply| Command::Annotation {
            timestamp_us,
            text,
            reply,
        })?
    }
    pub fn finish(&self, fence: u64, occurrence: Vec<u32>) -> Result<ProcessorResult> {
        self.request(|reply| Command::Finish {
            fence,
            occurrence,
            reply,
        })?
    }
    pub fn snapshot(&self) -> Arc<ProcessorSnapshot> {
        self.snapshot.read().unwrap().clone()
    }
    pub fn shutdown(&self) {
        let (reply, response) = mpsc::sync_channel(0);
        if self.commands.send(Command::Shutdown { reply }).is_ok() {
            let _ = response.recv();
        }
        if let Some(worker) = self.worker.lock().unwrap().take() {
            let _ = worker.join();
        }
    }
    #[cfg(test)]
    pub(crate) fn panic_worker_for_test(&self) {
        let (reply, response) = mpsc::sync_channel(0);
        let _ = self.commands.send(Command::PanicForTest { _reply: reply });
        let _ = response.recv();
    }
}
impl Drop for RecorderProcessor {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn publish(snapshot: &RwLock<Arc<ProcessorSnapshot>>, session: Option<&Session>) {
    let count = session.map_or(0, |s| {
        s.raw
            .iter()
            .filter(|e| matches!(e, RecordingBoundary::Event(..)))
            .count()
    });
    *snapshot.write().unwrap() = Arc::new(ProcessorSnapshot {
        raw_event_count: count as u64,
        estimated_action_count: count,
    });
}

fn accept(session: &mut Session, sequenced: SequencedHookEvent, enricher: &mut dyn EventEnricher) {
    if sequenced.sequence < session.floor {
        return;
    }
    if enricher.is_own_process_input(&sequenced.event) {
        if let HookEvent::Key { vk, transition, .. } = sequenced.event {
            if transition == KeyTransition::Down {
                update_live_key_state(&mut session.live_key_state, vk, true);
                session.suppressed_until_up.extend(seed_suppression([vk]));
            } else {
                track_key(session, &sequenced.event);
                session
                    .raw
                    .push(RecordingBoundary::Event(sequenced.event, None));
            }
        }
        if let HookEvent::Mouse { message, .. } = sequenced.event {
            match message {
                MouseMessage::Down(button) => {
                    session.suppressed_mouse_until_up.insert(button);
                }
                MouseMessage::Up(button) => {
                    if !session.suppressed_mouse_until_up.remove(&button)
                        && let Some(index) = session.mouse_pressed.remove(&button)
                    {
                        session.suppressed.insert(index);
                    }
                }
                _ => {}
            }
        }
        return;
    }
    if let HookEvent::Mouse { message, .. } = sequenced.event {
        match message {
            MouseMessage::Down(button) if session.suppressed_mouse_until_up.contains(&button) => {
                return;
            }
            MouseMessage::Down(button) => {
                session.mouse_pressed.insert(button, session.raw.len());
            }
            MouseMessage::Up(button) if session.suppressed_mouse_until_up.remove(&button) => {
                return;
            }
            MouseMessage::Up(button) => {
                session.mouse_pressed.remove(&button);
            }
            _ => {}
        }
    }
    track_key(session, &sequenced.event);
    let context = if session.config.record_window_context {
        enricher.enrich(&sequenced.event)
    } else if matches!(sequenced.event, HookEvent::Key { .. }) {
        enricher.enrich_keyboard_layout()
    } else {
        None
    };
    session.observations.drain_native(enricher);
    let (control_down, alt_down) = paste_modifier_state(&session.live_key_state);
    session.observations.observe(
        &sequenced.event,
        context.as_ref(),
        session.config.inspect_clicked_controls,
        session.config.record_mouse_buttons,
        session.config.capture_text_paste_for_freeze_suggestion,
        control_down,
        alt_down,
        session.config.click_max_ms,
        session.config.click_distance_px,
    );
    session
        .raw
        .push(RecordingBoundary::Event(sequenced.event, context));
}

fn paste_modifier_state(live_key_state: &[bool; 256]) -> (bool, bool) {
    let down = |keys: &[usize]| keys.iter().any(|key| live_key_state[*key]);
    (down(&[0x11, 0xa2, 0xa3]), down(&[0x12, 0xa4, 0xa5]))
}

fn live_state_from_initial(initial: &[u8; 256]) -> [bool; 256] {
    std::array::from_fn(|index| initial[index] & 0x80 != 0)
}

fn is_modifier_vk(vk: u32) -> bool {
    matches!(vk, 0x10 | 0x11 | 0x12 | 0x5b | 0x5c | 0xa0..=0xa5)
}

fn reconcile_resume_ownership(
    session: &mut Session,
    physical_state: &[u8; 256],
    control_keys: &[u32],
    timestamp_us: u64,
) -> [u8; 256] {
    let controls = seed_suppression(control_keys.iter().copied());
    let mut restored_state = [0u8; 256];
    let mut owned = std::mem::take(&mut session.held_before_pause)
        .into_iter()
        .collect::<Vec<_>>();
    // Release in reverse authored-down order, matching normal chord cleanup,
    // while keeping synthetic order and downstream IDs deterministic.
    owned.sort_by_key(|(_, run)| std::cmp::Reverse(run.index));
    for (vk, run) in owned {
        let still_down = physical_state[(vk & 0xff) as usize] & 0x80 != 0;
        if still_down && !controls.contains(&vk) {
            restored_state[(vk & 0xff) as usize] = 0x80;
            session.pressed.insert(vk, run);
        } else {
            // The physical up occurred while callbacks were paused. Close the
            // already-recorded down immediately after Resume so normalization
            // compresses the paused duration while preserving a balanced pair.
            let (scan_code, flags, extra_info) = match session.raw.get(run.index) {
                Some(RecordingBoundary::Event(
                    HookEvent::Key {
                        scan_code,
                        flags,
                        extra_info,
                        ..
                    },
                    _,
                )) => (*scan_code, *flags, *extra_info),
                _ => (0, 0, 0),
            };
            let suppress_synthetic = session.suppressed.contains(&run.index)
                || controls.contains(&vk)
                || (!still_down && take_suppressed_key_up(&mut session.suppressed_until_up, vk));
            let synthetic_index = session.raw.len();
            session.raw.push(RecordingBoundary::Event(
                HookEvent::Key {
                    timestamp_us,
                    transition: KeyTransition::Up,
                    vk,
                    scan_code,
                    flags,
                    extra_info,
                },
                None,
            ));
            if suppress_synthetic {
                session.suppressed.insert(synthetic_index);
            }
        }
    }
    session.suppressed_until_up.extend(
        physical_state
            .iter()
            .enumerate()
            .filter(|(vk, value)| **value & 0x80 != 0 && restored_state[*vk] & 0x80 == 0)
            .map(|(vk, _)| vk as u32),
    );
    session.suppressed_until_up.extend(controls);
    restored_state
}
fn down_key_for(requested: u32, pressed: &HashMap<u32, DownRun>) -> Option<u32> {
    match requested {
        0x10 => [0x10, 0xa0, 0xa1]
            .into_iter()
            .find(|key| pressed.contains_key(key)),
        0x11 => [0x11, 0xa2, 0xa3]
            .into_iter()
            .find(|key| pressed.contains_key(key)),
        0x12 => [0x12, 0xa4, 0xa5]
            .into_iter()
            .find(|key| pressed.contains_key(key)),
        _ => pressed.contains_key(&requested).then_some(requested),
    }
}
fn seed_suppression(keys: impl IntoIterator<Item = u32>) -> HashSet<u32> {
    let mut out = HashSet::new();
    for key in keys {
        match key {
            0x10 => out.extend([0x10, 0xa0, 0xa1]),
            0x11 => out.extend([0x11, 0xa2, 0xa3]),
            0x12 => out.extend([0x12, 0xa4, 0xa5]),
            _ => {
                out.insert(key);
            }
        }
    }
    out
}
fn take_suppressed_key_up(suppressed: &mut HashSet<u32>, vk: u32) -> bool {
    if !suppressed.contains(&vk) {
        return false;
    }
    match vk {
        0x10 | 0xa0 | 0xa1 => suppressed.retain(|key| !matches!(*key, 0x10 | 0xa0 | 0xa1)),
        0x11 | 0xa2 | 0xa3 => suppressed.retain(|key| !matches!(*key, 0x11 | 0xa2 | 0xa3)),
        0x12 | 0xa4 | 0xa5 => suppressed.retain(|key| !matches!(*key, 0x12 | 0xa4 | 0xa5)),
        _ => {
            suppressed.remove(&vk);
        }
    }
    true
}
fn initial_key_state(
    translator: &mut dyn super::KeyboardTranslator,
    held_keys: &[u32],
) -> [u8; 256] {
    cleared_control_state(translator.initial_key_state(), held_keys)
}

fn cleared_control_state(mut state: [u8; 256], held_keys: &[u32]) -> [u8; 256] {
    for &key in held_keys {
        state[(key & 0xff) as usize] &= 1;
        match key {
            0x10 => {
                state[0xa0] &= 1;
                state[0xa1] &= 1;
            }
            0x11 => {
                state[0xa2] &= 1;
                state[0xa3] &= 1;
            }
            0x12 => {
                state[0xa4] &= 1;
                state[0xa5] &= 1;
            }
            0xA0 | 0xA1 => state[0x10] &= 1,
            0xA2 | 0xA3 => state[0x11] &= 1,
            0xA4 | 0xA5 => state[0x12] &= 1,
            _ => {}
        }
    }
    state
}

fn reconcile_suppression_with_physical_state(
    suppressed: &mut HashSet<u32>,
    physical_state: &[u8; 256],
) {
    suppressed.retain(|vk| physical_state[(vk & 0xff) as usize] & 0x80 != 0);
}
fn track_key(session: &mut Session, event: &HookEvent) {
    let HookEvent::Key { transition, vk, .. } = event else {
        return;
    };
    let index = session.raw.len();
    update_live_key_state(
        &mut session.live_key_state,
        *vk,
        *transition == KeyTransition::Down,
    );
    match transition {
        KeyTransition::Down if session.suppressed_until_up.contains(vk) => {
            session.suppressed.insert(index);
        }
        KeyTransition::Down => {
            if !is_modifier_vk(*vk) {
                for (key, run) in &mut session.pressed {
                    if is_modifier_vk(*key) {
                        run.ordinary_key_downs.push(index);
                    }
                }
            }
            session.pressed.entry(*vk).or_insert(DownRun {
                index,
                ordinary_key_downs: Vec::new(),
            });
        }
        KeyTransition::Up if take_suppressed_key_up(&mut session.suppressed_until_up, *vk) => {
            session.suppressed.insert(index);
            session.pressed.remove(vk);
        }
        KeyTransition::Up => {
            session.pressed.remove(vk);
        }
    }
}

fn update_live_key_state(state: &mut [bool; 256], vk: u32, down: bool) {
    state[(vk & 0xff) as usize] = down;
    match vk {
        0xa0 | 0xa1 => state[0x10] = state[0xa0] || state[0xa1],
        0xa2 | 0xa3 => state[0x11] = state[0xa2] || state[0xa3],
        0xa4 | 0xa5 => state[0x12] = state[0xa4] || state[0xa5],
        _ => {}
    }
}
fn suppress_occurrence(session: &mut Session, keys: &[u32]) {
    let Some((&requested_primary, modifiers)) = keys.split_last() else {
        return;
    };
    let Some(primary) = down_key_for(requested_primary, &session.pressed) else {
        return;
    };
    let primary_down = session.pressed.get(&primary).map(|run| run.index);
    if let Some(primary_down) = primary_down {
        session.suppressed.insert(primary_down);
        session.suppressed_until_up.insert(primary);
    }
    for requested in modifiers {
        let Some(key) = down_key_for(*requested, &session.pressed) else {
            continue;
        };
        if let Some(run) = session.pressed.get(&key) {
            if run
                .ordinary_key_downs
                .iter()
                .all(|index| Some(*index) == primary_down)
            {
                session.suppressed.insert(run.index);
                session.suppressed_until_up.insert(key);
            }
        }
    }
}

fn drain_before(
    events: &mpsc::Receiver<SequencedHookEvent>,
    session: &mut Session,
    fence: u64,
    enricher: &mut dyn EventEnricher,
) {
    while let Some(event) = session.backlog.pop_front() {
        if event.sequence >= fence {
            session.backlog.push_front(event);
            return;
        }
        accept(session, event, enricher);
    }
    while let Ok(event) = events.try_recv() {
        if event.sequence < fence {
            accept(session, event, enricher);
        } else {
            session.backlog.push_back(event);
            break;
        }
    }
}

#[derive(Debug, Clone, Default)]
struct RecordingTimeline {
    paused_ranges: Vec<(u64, u64)>,
    open_pause: Option<u64>,
}
impl RecordingTimeline {
    fn from_boundaries(boundaries: &[RecordingBoundary]) -> Self {
        let mut timeline = Self::default();
        for boundary in boundaries {
            match boundary {
                RecordingBoundary::Pause { timestamp_us } => {
                    if timeline.open_pause.is_none() {
                        timeline.open_pause = Some(*timestamp_us);
                    }
                }
                RecordingBoundary::Resume { timestamp_us } => {
                    if let Some(start) = timeline.open_pause.take() {
                        timeline.paused_ranges.push((start, *timestamp_us));
                    }
                }
                _ => {}
            }
        }
        timeline
    }

    fn normalize(&self, timestamp_us: u64) -> u64 {
        let mut excluded = 0u64;
        for &(start, end) in &self.paused_ranges {
            if timestamp_us <= start {
                break;
            }
            excluded = excluded.saturating_add(timestamp_us.min(end).saturating_sub(start));
            if timestamp_us < end {
                return timestamp_us.saturating_sub(excluded);
            }
        }
        if let Some(start) = self.open_pause
            && timestamp_us > start
        {
            excluded = excluded.saturating_add(timestamp_us.saturating_sub(start));
        }
        timestamp_us.saturating_sub(excluded)
    }

    fn is_paused(&self, timestamp_us: u64) -> bool {
        self.paused_ranges
            .iter()
            .any(|(start, end)| timestamp_us >= *start && timestamp_us < *end)
            || self.open_pause.is_some_and(|start| timestamp_us >= start)
    }
}

fn worker_loop(
    commands: &mpsc::Receiver<Command>,
    events: &mpsc::Receiver<SequencedHookEvent>,
    snapshot: &RwLock<Arc<ProcessorSnapshot>>,
    translator: &mut dyn KeyboardTranslator,
    observer_factory: &Arc<dyn Fn() -> RecorderObserverSession + Send + Sync>,
    enricher: &mut dyn EventEnricher,
) {
    let mut session: Option<Session> = None;
    loop {
        let command = if session.is_none() {
            match commands.recv() {
                Ok(command) => Some(command),
                Err(_) => break,
            }
        } else {
            match commands.try_recv() {
                Ok(command) => Some(command),
                Err(mpsc::TryRecvError::Disconnected) => break,
                Err(mpsc::TryRecvError::Empty) => {
                    if let Some(event) = session.as_mut().unwrap().backlog.pop_front() {
                        accept(session.as_mut().unwrap(), event, enricher);
                        publish(snapshot, session.as_ref());
                        continue;
                    }
                    match events.recv_timeout(Duration::from_millis(20)) {
                        Ok(event) => {
                            accept(session.as_mut().unwrap(), event, enricher);
                            publish(snapshot, session.as_ref());
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    None
                }
            }
        };
        let Some(command) = command else { continue };
        match command {
            Command::Begin {
                target,
                config,
                floor,
                held_keys,
                reply,
            } => {
                let result = if session.is_some() {
                    Err(anyhow!("recorder processor is already active"))
                } else {
                    let initial_key_state = initial_key_state(translator, &held_keys);
                    let live_key_state = live_state_from_initial(&initial_key_state);
                    session = Some(Session {
                        target,
                        config,
                        raw: Vec::new(),
                        pressed: HashMap::new(),
                        held_before_pause: HashMap::new(),
                        suppressed_until_up: seed_suppression(held_keys),
                        suppressed: HashSet::new(),
                        mouse_pressed: HashMap::new(),
                        suppressed_mouse_until_up: HashSet::new(),
                        backlog: VecDeque::new(),
                        floor,
                        initial_key_state,
                        live_key_state,
                        observations: observer_factory(),
                    });
                    publish(snapshot, session.as_ref());
                    Ok(())
                };
                let _ = reply.send(result);
            }
            Command::Pause {
                timestamp_us,
                fence,
                occurrence,
                reply,
            } => {
                let result = session
                    .as_mut()
                    .ok_or_else(|| anyhow!("recorder processor is idle"))
                    .map(|s| {
                        drain_before(events, s, fence, enricher);
                        suppress_occurrence(s, &occurrence);
                        s.raw.push(RecordingBoundary::Pause { timestamp_us });
                        s.held_before_pause = std::mem::take(&mut s.pressed);
                        s.live_key_state = [false; 256];
                        publish(snapshot, Some(s));
                    });
                let _ = reply.send(result);
            }
            Command::Resume {
                timestamp_us,
                held_keys,
                reply,
            } => {
                let result = session
                    .as_mut()
                    .ok_or_else(|| anyhow!("recorder processor is idle"))
                    .map(|s| {
                        s.raw.push(RecordingBoundary::Resume { timestamp_us });
                        let physical_state = translator.initial_key_state();
                        reconcile_suppression_with_physical_state(
                            &mut s.suppressed_until_up,
                            &physical_state,
                        );
                        let resumed_state = reconcile_resume_ownership(
                            s,
                            &physical_state,
                            &held_keys,
                            timestamp_us,
                        );
                        s.live_key_state = live_state_from_initial(&resumed_state);
                    });
                let _ = reply.send(result);
            }
            Command::Control {
                fence,
                occurrence,
                reply,
            } => {
                let result = session
                    .as_mut()
                    .ok_or_else(|| anyhow!("recorder processor is idle"))
                    .map(|s| {
                        drain_before(events, s, fence, enricher);
                        suppress_occurrence(s, &occurrence);
                    });
                let _ = reply.send(result);
            }
            Command::Marker {
                timestamp_us,
                reply,
            } => {
                let result = session
                    .as_mut()
                    .ok_or_else(|| anyhow!("recorder processor is idle"))
                    .map(|s| {
                        s.raw.push(RecordingBoundary::Marker { timestamp_us });
                    });
                let _ = reply.send(result);
            }
            Command::Annotation {
                timestamp_us,
                text,
                reply,
            } => {
                let result = session
                    .as_mut()
                    .ok_or_else(|| anyhow!("recorder processor is idle"))
                    .map(|s| {
                        s.raw
                            .push(RecordingBoundary::Annotation { timestamp_us, text });
                    });
                let _ = reply.send(result);
            }
            Command::Finish {
                fence,
                occurrence,
                reply,
            } => {
                let result = session
                    .take()
                    .ok_or_else(|| anyhow!("recorder processor is idle"))
                    .map(|mut s| {
                        drain_before(events, &mut s, fence, enricher);
                        suppress_occurrence(&mut s, &occurrence);
                        let mut index = 0;
                        s.raw.retain(|_| {
                            let keep = !s.suppressed.contains(&index);
                            index += 1;
                            keep
                        });
                        let timeline = RecordingTimeline::from_boundaries(&s.raw);
                        let notes: Vec<_> = s
                            .raw
                            .iter()
                            .filter_map(|boundary| match boundary {
                                RecordingBoundary::Marker { timestamp_us } => {
                                    Some(RecordingNote::Marker {
                                        timestamp_us: timeline.normalize(*timestamp_us),
                                    })
                                }
                                RecordingBoundary::Annotation { timestamp_us, text } => {
                                    Some(RecordingNote::Annotation {
                                        timestamp_us: timeline.normalize(*timestamp_us),
                                        text: text.clone(),
                                    })
                                }
                                _ => None,
                            })
                            .collect();
                        let literal_steps = normalize(&s.raw, &s.config, None);
                        let enriched = enrich_keyboard_with_state(
                            &literal_steps,
                            translator,
                            s.initial_key_state,
                        );
                        let source_times: Vec<_> = literal_steps
                            .iter()
                            .enumerate()
                            .map(|(index, step)| (index, step.timestamp_us))
                            .collect();
                        let mut plan =
                            build_recording_plan(&enriched, &s.config.semantic_settings());
                        s.observations.finish(enricher);
                        s.observations
                            .retain_active(|timestamp| !timeline.is_paused(timestamp));
                        s.observations
                            .retime(|timestamp| timeline.normalize(timestamp));
                        apply_recording_notes(&mut plan, &notes, &source_times);
                        let suggestions = discover_suggestions(
                            &plan,
                            &s.config.semantic_settings(),
                            &s.observations.baseline,
                            &s.observations.windows,
                            &s.observations.clipboards,
                            &source_times,
                        );
                        publish(snapshot, None);
                        ProcessorResult {
                            target: s.target,
                            literal_steps,
                            plan,
                            suggestions,
                            clipboard_observations: s.observations.clipboards,
                            click_inspections: s.observations.inspections,
                            window_observations: s.observations.windows,
                            notes,
                            raw_event_count: s
                                .raw
                                .iter()
                                .filter(|boundary| matches!(boundary, RecordingBoundary::Event(..)))
                                .count() as u64,
                        }
                    });
                let _ = reply.send(result);
            }
            Command::Shutdown { reply } => {
                let _ = reply.send(());
                break;
            }
            #[cfg(test)]
            Command::PanicForTest { .. } => panic!("injected recorder processor panic"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        KeyTranslation, KeyboardTranslationRequest, MkAction, MkKey, RecordedAction,
    };
    use super::*;
    use std::sync::Mutex;

    struct SurfaceEnricher {
        own: bool,
    }
    impl EventEnricher for SurfaceEnricher {
        fn enrich(&mut self, _event: &HookEvent) -> Option<super::super::EventContext> {
            None
        }
        fn is_own_process_input(&self, _event: &HookEvent) -> bool {
            self.own
        }
    }
    struct StatefulTranslator(Arc<Mutex<[u8; 256]>>);
    impl KeyboardTranslator for StatefulTranslator {
        fn initial_key_state(&mut self) -> [u8; 256] {
            *self.0.lock().unwrap()
        }
        fn translate(&mut self, _: &KeyboardTranslationRequest) -> KeyTranslation {
            KeyTranslation::None
        }
    }

    fn synthetic_processor(
        physical: Arc<Mutex<[u8; 256]>>,
        receiver: mpsc::Receiver<SequencedHookEvent>,
    ) -> RecorderProcessor {
        RecorderProcessor::with_all_components(
            receiver,
            Box::new(StatefulTranslator(physical)),
            Arc::new(|| {
                RecorderObserverSession::with_parts(
                    ObservationBaseline::default(),
                    super::super::AuxiliaryObservationWorker::spawn(None, None),
                )
            }),
            Box::new(SurfaceEnricher { own: false }),
        )
    }

    fn key(vk: u32, transition: KeyTransition, timestamp_us: u64) -> HookEvent {
        HookEvent::Key {
            timestamp_us,
            transition,
            vk,
            scan_code: 0,
            flags: 0,
            extra_info: 0,
        }
    }

    fn raw_key(
        vk: u32,
        transition: KeyTransition,
        timestamp_us: u64,
        scan_code: u32,
        flags: u32,
        extra_info: usize,
    ) -> HookEvent {
        HookEvent::Key {
            timestamp_us,
            transition,
            vk,
            scan_code,
            flags,
            extra_info,
        }
    }

    fn begin_synthetic(processor: &RecorderProcessor) {
        let mut config = NormalizationConfig::default();
        config.record_window_context = false;
        processor
            .begin(
                RecordingTarget {
                    macro_id: 1,
                    insertion_anchor_step_id: None,
                    insertion_anchor_generation: None,
                },
                config,
                0,
                Vec::new(),
            )
            .unwrap();
    }

    fn mouse(button: super::super::MouseButton, down: bool, timestamp_us: u64) -> HookEvent {
        HookEvent::Mouse {
            timestamp_us,
            message: if down {
                MouseMessage::Down(button)
            } else {
                MouseMessage::Up(button)
            },
            x: 10,
            y: 20,
            flags: 0,
            extra_info: 0,
        }
    }

    #[test]
    fn own_process_key_boundaries_preserve_balance_without_recording_ui_input() {
        let mut s = session();
        let mut external = SurfaceEnricher { own: false };
        let mut own = SurfaceEnricher { own: true };
        accept(
            &mut s,
            SequencedHookEvent {
                sequence: 0,
                event: key(0xa2, KeyTransition::Down, 1),
            },
            &mut external,
        );
        accept(
            &mut s,
            SequencedHookEvent {
                sequence: 1,
                event: key(0xa2, KeyTransition::Up, 2),
            },
            &mut own,
        );
        assert_eq!(retained_vks(&s), vec![0xa2, 0xa2]);
        assert!(!s.live_key_state[0xa2]);

        let mut s = session();
        accept(
            &mut s,
            SequencedHookEvent {
                sequence: 0,
                event: key(0x11, KeyTransition::Down, 1),
            },
            &mut own,
        );
        accept(
            &mut s,
            SequencedHookEvent {
                sequence: 1,
                event: key(0xa2, KeyTransition::Up, 2),
            },
            &mut external,
        );
        assert!(retained_vks(&s).is_empty());
        assert!(!s.live_key_state[0xa2]);
    }

    #[test]
    fn own_process_mouse_boundaries_never_leave_an_unmatched_button() {
        let button = super::super::MouseButton::Left;
        let mut external = SurfaceEnricher { own: false };
        let mut own = SurfaceEnricher { own: true };
        let mut s = session();
        accept(
            &mut s,
            SequencedHookEvent {
                sequence: 0,
                event: mouse(button, true, 1),
            },
            &mut external,
        );
        accept(
            &mut s,
            SequencedHookEvent {
                sequence: 1,
                event: mouse(button, false, 2),
            },
            &mut own,
        );
        assert!(s.mouse_pressed.is_empty());
        assert!(
            s.raw
                .iter()
                .enumerate()
                .filter(|(index, _)| !s.suppressed.contains(index))
                .all(|(_, boundary)| !matches!(
                    boundary,
                    RecordingBoundary::Event(HookEvent::Mouse { .. }, _)
                ))
        );

        let mut s = session();
        accept(
            &mut s,
            SequencedHookEvent {
                sequence: 0,
                event: mouse(button, true, 1),
            },
            &mut own,
        );
        accept(
            &mut s,
            SequencedHookEvent {
                sequence: 1,
                event: mouse(button, false, 2),
            },
            &mut external,
        );
        assert!(s.mouse_pressed.is_empty());
        assert!(s.raw.is_empty());
    }
    #[test]
    fn paste_modifiers_include_preheld_control_and_reject_altgr() {
        let mut initial = [0u8; 256];
        initial[0xa2] = 0x80;
        let live = live_state_from_initial(&initial);
        assert_eq!(paste_modifier_state(&live), (true, false));
        initial[0xa5] = 0x80;
        let altgr = live_state_from_initial(&initial);
        assert_eq!(paste_modifier_state(&altgr), (true, true));
    }

    #[test]
    fn pause_resume_excludes_prompt_key_and_balances_pre_pause_held_key() {
        let physical = Arc::new(Mutex::new([0u8; 256]));
        let (events, receiver) = mpsc::sync_channel(8);
        let processor = synthetic_processor(physical.clone(), receiver);
        begin_synthetic(&processor);

        events
            .send(SequencedHookEvent {
                sequence: 0,
                event: key(0x57, KeyTransition::Down, 10),
            })
            .unwrap();
        processor.pause(100, 1, Vec::new()).unwrap();
        let mut resumed = [0u8; 256];
        resumed[0x57] = 0x80; // W belongs to the recording from before Pause.
        resumed[0x0d] = 0x80; // Enter was first pressed by the paused annotation prompt.
        *physical.lock().unwrap() = resumed;
        processor.resume(200, Vec::new()).unwrap();
        events
            .send(SequencedHookEvent {
                sequence: 1,
                event: key(0x57, KeyTransition::Up, 300),
            })
            .unwrap();
        events
            .send(SequencedHookEvent {
                sequence: 2,
                event: key(0x0d, KeyTransition::Up, 310),
            })
            .unwrap();

        let result = processor.finish(3, Vec::new()).unwrap();
        assert_eq!(result.raw_event_count, 2);
        assert_eq!(result.literal_steps.len(), 2);
        assert!(matches!(
            result.literal_steps[0].action,
            RecordedAction::Key {
                down: true,
                vk: 0x57,
                ..
            }
        ));
        assert!(matches!(
            result.literal_steps[1].action,
            RecordedAction::Key {
                down: false,
                vk: 0x57,
                ..
            }
        ));
    }

    #[test]
    fn modifier_held_across_pause_is_not_duplicated_and_owns_its_later_up() {
        let physical = Arc::new(Mutex::new([0u8; 256]));
        let (events, receiver) = mpsc::sync_channel(8);
        let processor = synthetic_processor(physical.clone(), receiver);
        begin_synthetic(&processor);
        events
            .send(SequencedHookEvent {
                sequence: 0,
                event: key(0xa2, KeyTransition::Down, 10),
            })
            .unwrap();
        processor.pause(100, 1, Vec::new()).unwrap();
        physical.lock().unwrap()[0xa2] = 0x80;
        processor.resume(200, Vec::new()).unwrap();
        for (sequence, event) in [
            key(0x43, KeyTransition::Down, 210),
            key(0x43, KeyTransition::Up, 220),
            key(0xa2, KeyTransition::Up, 230),
        ]
        .into_iter()
        .enumerate()
        {
            events
                .send(SequencedHookEvent {
                    sequence: sequence as u64 + 1,
                    event,
                })
                .unwrap();
        }
        let result = processor.finish(4, Vec::new()).unwrap();
        let keys = result
            .literal_steps
            .iter()
            .filter_map(|step| match step.action {
                RecordedAction::Key { down, vk, .. } => Some((down, vk)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            vec![(true, 0xa2), (true, 0x43), (false, 0x43), (false, 0xa2)]
        );
    }

    #[test]
    fn key_released_during_pause_gets_a_deterministic_metadata_preserving_up() {
        let physical = Arc::new(Mutex::new([0u8; 256]));
        let (events, receiver) = mpsc::sync_channel(8);
        let processor = synthetic_processor(physical, receiver);
        begin_synthetic(&processor);
        events
            .send(SequencedHookEvent {
                sequence: 0,
                event: raw_key(0xe8, KeyTransition::Down, 10, 0x45, 1, 77),
            })
            .unwrap();
        processor.pause(100, 1, Vec::new()).unwrap();
        processor.resume(200, Vec::new()).unwrap();
        let result = processor.finish(1, Vec::new()).unwrap();
        let keys = result
            .literal_steps
            .iter()
            .filter_map(|step| match step.action {
                RecordedAction::Key {
                    down,
                    vk,
                    scan_code,
                    extended,
                    flags,
                    extra_info,
                } => Some((down, vk, scan_code, extended, flags, extra_info)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            vec![
                (true, 0xe8, 0x45, true, 1, 77),
                (false, 0xe8, 0x45, true, 1, 77),
            ]
        );
    }

    #[test]
    fn chord_released_during_pause_closes_in_reverse_order_and_remains_a_hotkey() {
        let physical = Arc::new(Mutex::new([0u8; 256]));
        let (events, receiver) = mpsc::sync_channel(8);
        let processor = synthetic_processor(physical, receiver);
        begin_synthetic(&processor);
        events
            .send(SequencedHookEvent {
                sequence: 0,
                event: key(0xa2, KeyTransition::Down, 10),
            })
            .unwrap();
        events
            .send(SequencedHookEvent {
                sequence: 1,
                event: key(0x53, KeyTransition::Down, 20),
            })
            .unwrap();
        processor.pause(100, 2, Vec::new()).unwrap();
        processor.resume(200, Vec::new()).unwrap();
        let result = processor.finish(2, Vec::new()).unwrap();
        let keys = result
            .literal_steps
            .iter()
            .filter_map(|step| match step.action {
                RecordedAction::Key { down, vk, .. } => Some((down, vk)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            vec![(true, 0xa2), (true, 0x53), (false, 0x53), (false, 0xa2)]
        );
        assert!(
            matches!(
                result.plan.steps.as_slice(),
                [step]
                    if matches!(
                        &step.action,
                        MkAction::Hotkey(keys)
                            if keys == &vec![MkKey::LeftControl, MkKey::Character("S".into())]
                    )
            ),
            "unexpected plan: {:?}",
            result.plan.steps
        );
    }

    #[test]
    fn pause_and_resume_controls_leave_no_strays_and_later_control_is_recorded() {
        let physical = Arc::new(Mutex::new([0u8; 256]));
        let (events, receiver) = mpsc::sync_channel(16);
        let processor = synthetic_processor(physical.clone(), receiver);
        begin_synthetic(&processor);
        events
            .send(SequencedHookEvent {
                sequence: 0,
                event: key(0xa2, KeyTransition::Down, 10),
            })
            .unwrap();
        events
            .send(SequencedHookEvent {
                sequence: 1,
                event: key(0x77, KeyTransition::Down, 20),
            })
            .unwrap();
        processor.pause(100, 2, vec![0x11, 0x77]).unwrap();
        {
            let mut state = physical.lock().unwrap();
            state[0xa2] = 0x80;
            state[0x77] = 0x80;
        }
        processor.resume(200, vec![0x11, 0x77]).unwrap();
        for (sequence, event) in [
            key(0x77, KeyTransition::Up, 210),
            key(0xa2, KeyTransition::Up, 220),
            key(0xa2, KeyTransition::Down, 300),
            key(0x43, KeyTransition::Down, 310),
            key(0x43, KeyTransition::Up, 320),
            key(0xa2, KeyTransition::Up, 330),
        ]
        .into_iter()
        .enumerate()
        {
            events
                .send(SequencedHookEvent {
                    sequence: sequence as u64 + 2,
                    event,
                })
                .unwrap();
        }
        let result = processor.finish(8, Vec::new()).unwrap();
        let keys = result
            .literal_steps
            .iter()
            .filter_map(|step| match step.action {
                RecordedAction::Key { down, vk, .. } => Some((down, vk)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            vec![(true, 0xa2), (true, 0x43), (false, 0x43), (false, 0xa2)]
        );
    }

    #[test]
    fn resume_discards_stale_suppression_for_keys_released_while_paused() {
        let mut suppressed = seed_suppression([0x11, 0x78]);
        let mut physical = [0u8; 256];
        physical[0xa2] = 0x80;
        reconcile_suppression_with_physical_state(&mut suppressed, &physical);
        assert_eq!(suppressed, HashSet::from([0xa2]));
        assert!(take_suppressed_key_up(&mut suppressed, 0xa2));
        assert!(suppressed.is_empty());
    }

    #[test]
    fn generic_resume_modifier_clears_generic_and_sided_physical_state() {
        let mut physical = [0u8; 256];
        physical[0x11] = 0x80;
        physical[0xa2] = 0x80;
        physical[0xa3] = 0x80;
        let cleared = cleared_control_state(physical, &[0x11, 0x78]);
        assert_eq!(cleared[0x11] & 0x80, 0);
        assert_eq!(cleared[0xa2] & 0x80, 0);
        assert_eq!(cleared[0xa3] & 0x80, 0);
    }
    #[test]
    fn pause_timeline_excludes_paused_observations_and_compresses_all_later_times() {
        let timeline = RecordingTimeline::from_boundaries(&[
            RecordingBoundary::Pause { timestamp_us: 100 },
            RecordingBoundary::Resume {
                timestamp_us: 1_100,
            },
        ]);
        assert!(!timeline.is_paused(99));
        assert!(timeline.is_paused(100));
        assert!(timeline.is_paused(900));
        assert!(!timeline.is_paused(1_100));
        assert_eq!(timeline.normalize(50), 50);
        assert_eq!(timeline.normalize(1_500), 500);
    }
    fn push(session: &mut Session, event: HookEvent) {
        track_key(session, &event);
        session.raw.push(RecordingBoundary::Event(event, None));
    }
    fn session() -> Session {
        Session {
            target: RecordingTarget {
                macro_id: 1,
                insertion_anchor_step_id: Some(9),
                insertion_anchor_generation: None,
            },
            config: NormalizationConfig::default(),
            raw: Vec::new(),
            pressed: HashMap::new(),
            held_before_pause: HashMap::new(),
            suppressed_until_up: HashSet::new(),
            suppressed: HashSet::new(),
            mouse_pressed: HashMap::new(),
            suppressed_mouse_until_up: HashSet::new(),
            backlog: VecDeque::new(),
            floor: 0,
            initial_key_state: [0; 256],
            live_key_state: [false; 256],
            observations: RecorderObserverSession::with_parts(
                ObservationBaseline::default(),
                super::super::AuxiliaryObservationWorker::spawn(None, None),
            ),
        }
    }
    fn retained_vks(session: &Session) -> Vec<u32> {
        session
            .raw
            .iter()
            .enumerate()
            .filter(|(i, _)| !session.suppressed.contains(i))
            .filter_map(|(_, b)| match b {
                RecordingBoundary::Event(HookEvent::Key { vk, .. }, _) => Some(*vk),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn exact_control_occurrence_removes_primary_and_unused_modifier_interval() {
        let mut s = session();
        push(&mut s, key(0xa3, KeyTransition::Down, 1));
        push(&mut s, key(0x78, KeyTransition::Down, 2));
        suppress_occurrence(&mut s, &[0x11, 0x78]);
        push(&mut s, key(0x78, KeyTransition::Up, 3));
        push(&mut s, key(0xa3, KeyTransition::Up, 4));
        assert!(retained_vks(&s).is_empty());
    }

    #[test]
    fn modifier_serving_ordinary_key_is_preserved() {
        let mut s = session();
        push(&mut s, key(0xa2, KeyTransition::Down, 1));
        push(&mut s, key(0x43, KeyTransition::Down, 2));
        push(&mut s, key(0x43, KeyTransition::Up, 3));
        push(&mut s, key(0x78, KeyTransition::Down, 4));
        suppress_occurrence(&mut s, &[0x11, 0x78]);
        push(&mut s, key(0x78, KeyTransition::Up, 5));
        push(&mut s, key(0xa2, KeyTransition::Up, 6));
        assert_eq!(retained_vks(&s), vec![0xa2, 0x43, 0x43, 0xa2]);
    }
}
