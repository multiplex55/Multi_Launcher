use once_cell::sync::Lazy;
use std::collections::{BTreeMap, VecDeque};
use std::io::Cursor;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

pub static SOUND_NAMES: &[&str] = &[
    "None",
    "Alarm.wav",
    "Alarm02.wav",
    "Alarm03.wav",
    "Alarm04.wav",
    "Alarm05.wav",
    "Alarm06.wav",
    "Alarm07.wav",
    "Alarm08.wav",
    "Alarm09.wav",
    "AlarmNag.wav",
    "ReminderDelete.wav",
    "ReminderHold.wav",
    "ReminderStart.wav",
    "StartUp.wav",
];

static SOUNDS: Lazy<Vec<(&'static str, &'static [u8])>> = Lazy::new(|| {
    vec![
        ("Alarm.wav", include_bytes!("../Resources/sounds/Alarm.wav")),
        (
            "Alarm02.wav",
            include_bytes!("../Resources/sounds/Alarm02.wav"),
        ),
        (
            "Alarm03.wav",
            include_bytes!("../Resources/sounds/Alarm03.wav"),
        ),
        (
            "Alarm04.wav",
            include_bytes!("../Resources/sounds/Alarm04.wav"),
        ),
        (
            "Alarm05.wav",
            include_bytes!("../Resources/sounds/Alarm05.wav"),
        ),
        (
            "Alarm06.wav",
            include_bytes!("../Resources/sounds/Alarm06.wav"),
        ),
        (
            "Alarm07.wav",
            include_bytes!("../Resources/sounds/Alarm07.wav"),
        ),
        (
            "Alarm08.wav",
            include_bytes!("../Resources/sounds/Alarm08.wav"),
        ),
        (
            "Alarm09.wav",
            include_bytes!("../Resources/sounds/Alarm09.wav"),
        ),
        (
            "AlarmNag.wav",
            include_bytes!("../Resources/sounds/AlarmNag.wav"),
        ),
        (
            "ReminderDelete.wav",
            include_bytes!("../Resources/sounds/ReminderDelete.wav"),
        ),
        (
            "ReminderHold.wav",
            include_bytes!("../Resources/sounds/ReminderHold.wav"),
        ),
        (
            "ReminderStart.wav",
            include_bytes!("../Resources/sounds/ReminderStart.wav"),
        ),
        (
            "StartUp.wav",
            include_bytes!("../Resources/sounds/StartUp.wav"),
        ),
    ]
});

const AUDIO_QUEUE_CAPACITY: usize = 32;
const MAX_ACTIVE_SINKS: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PlaybackScope {
    Global,
    Radial { session: String, generation: u64 },
}

enum AudioCommand {
    Play {
        scope: PlaybackScope,
        wav: Arc<[u8]>,
    },
    Stop(PlaybackScope),
    /// Cancel stale sounds for a scope, then play one terminal cue. The scope
    /// is retired after admission so later stale commands cannot revive it.
    Finish {
        scope: PlaybackScope,
        wav: Arc<[u8]>,
    },
}

struct AudioService {
    mailbox: Arc<AudioMailbox>,
}

struct AudioMailbox {
    queue: Mutex<VecDeque<AudioCommand>>,
    terminal_scopes: Mutex<BTreeMap<PlaybackScope, usize>>,
    ready: Condvar,
    capacity: usize,
}

impl AudioMailbox {
    fn new(capacity: usize) -> Self {
        Self {
            queue: Mutex::new(VecDeque::with_capacity(capacity)),
            terminal_scopes: Mutex::new(BTreeMap::new()),
            ready: Condvar::new(),
            capacity: capacity.max(1),
        }
    }

    fn play(&self, command: AudioCommand) -> bool {
        let mut queue = self.queue.lock().unwrap_or_else(|error| error.into_inner());
        let plays = queue
            .iter()
            .filter(|command| matches!(command, AudioCommand::Play { .. }))
            .count();
        if plays >= self.capacity || queue.len() >= self.capacity.saturating_add(MAX_ACTIVE_SINKS) {
            return false;
        }
        queue.push_back(command);
        self.ready.notify_one();
        true
    }

    fn stop(&self, scope: PlaybackScope) -> bool {
        let mut queue = self.queue.lock().unwrap_or_else(|error| error.into_inner());
        let removed_finishes = queue
            .iter()
            .filter(|command| {
                matches!(command, AudioCommand::Finish { scope: queued, .. } if queued == &scope)
            })
            .count();
        queue.retain(|command| {
            !matches!(command, AudioCommand::Play { scope: queued, .. } if queued == &scope)
                && !matches!(command, AudioCommand::Stop(queued) if queued == &scope)
                && !matches!(command, AudioCommand::Finish { scope: queued, .. } if queued == &scope)
        });
        if !make_terminal_room(&mut queue, self.capacity) {
            return false;
        }
        self.release_terminal_tokens(&scope, removed_finishes);
        queue.push_back(AudioCommand::Stop(scope));
        self.ready.notify_one();
        true
    }

    fn finish(&self, scope: PlaybackScope, wav: Arc<[u8]>) -> bool {
        let mut queue = self.queue.lock().unwrap_or_else(|error| error.into_inner());
        let replaced_queued = queue
            .iter()
            .filter(|command| {
                matches!(command, AudioCommand::Finish { scope: queued, .. } if queued == &scope)
            })
            .count();
        queue.retain(|command| {
            !matches!(command, AudioCommand::Play { scope: queued, .. } if queued == &scope)
                && !matches!(command, AudioCommand::Finish { scope: queued, .. } if queued == &scope)
                && !matches!(command, AudioCommand::Stop(queued) if queued == &scope)
        });
        // Terminal retirement is control-plane work. It must not be rejected
        // behind a saturated queue of nonterminal selection/hover cues.
        let mut terminal_scopes = self
            .terminal_scopes
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let existing_tokens = terminal_scopes.get(&scope).copied().unwrap_or(0);
        let retained_tokens = existing_tokens.saturating_sub(replaced_queued);
        let is_new_scope = !terminal_scopes.contains_key(&scope);
        if is_new_scope && terminal_scopes.len() >= MAX_ACTIVE_SINKS {
            return false;
        }
        if !make_terminal_room(&mut queue, self.capacity) {
            return false;
        }
        terminal_scopes.insert(scope.clone(), retained_tokens.saturating_add(1));
        queue.push_back(AudioCommand::Finish { scope, wav });
        self.ready.notify_one();
        true
    }

    fn recv(&self) -> AudioCommand {
        let mut queue = self.queue.lock().unwrap_or_else(|error| error.into_inner());
        loop {
            if let Some(command) = queue.pop_front() {
                return command;
            }
            queue = self
                .ready
                .wait(queue)
                .unwrap_or_else(|error| error.into_inner());
        }
    }

    fn recv_timeout(&self, timeout: Duration) -> Option<AudioCommand> {
        let mut queue = self.queue.lock().unwrap_or_else(|error| error.into_inner());
        if queue.is_empty() {
            let (next, _) = self
                .ready
                .wait_timeout(queue, timeout)
                .unwrap_or_else(|error| error.into_inner());
            queue = next;
        }
        queue.pop_front()
    }

    fn release_terminal_tokens(&self, scope: &PlaybackScope, count: usize) {
        if count == 0 {
            return;
        }
        let mut scopes = self
            .terminal_scopes
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(tokens) = scopes.get_mut(scope) {
            *tokens = tokens.saturating_sub(count);
            if *tokens == 0 {
                scopes.remove(scope);
            }
        }
    }
}

fn make_terminal_room(queue: &mut VecDeque<AudioCommand>, capacity: usize) -> bool {
    let terminal_count = queue
        .iter()
        .filter(|command| !matches!(command, AudioCommand::Play { .. }))
        .count();
    if terminal_count >= MAX_ACTIVE_SINKS
        || queue.len() >= capacity.saturating_add(MAX_ACTIVE_SINKS)
    {
        return false;
    }
    if queue.len() < capacity {
        return true;
    }
    if let Some(index) = queue
        .iter()
        .position(|command| matches!(command, AudioCommand::Play { .. }))
    {
        queue.remove(index);
        true
    } else {
        // Reserve enough bounded capacity for every sink which can be active.
        // Previously accepted terminal commands are never displaced.
        true
    }
}

impl AudioService {
    fn spawn() -> Self {
        let mailbox = Arc::new(AudioMailbox::new(AUDIO_QUEUE_CAPACITY));
        let worker_mailbox = Arc::clone(&mailbox);
        let _ = std::thread::Builder::new()
            .name("multi-launcher-audio".into())
            .spawn(move || audio_worker(worker_mailbox));
        Self { mailbox }
    }

    fn play(&self, scope: PlaybackScope, wav: Arc<[u8]>) -> bool {
        self.mailbox.play(AudioCommand::Play { scope, wav })
    }

    fn stop(&self, scope: PlaybackScope) -> bool {
        self.mailbox.stop(scope)
    }

    fn finish(&self, scope: PlaybackScope, wav: Arc<[u8]>) -> bool {
        self.mailbox.finish(scope, wav)
    }
}

static AUDIO: Lazy<AudioService> = Lazy::new(AudioService::spawn);

fn audio_worker(mailbox: Arc<AudioMailbox>) {
    let Ok((_stream, handle)) = rodio::OutputStream::try_default() else {
        loop {
            let command = mailbox.recv();
            if let AudioCommand::Finish { scope, .. } = command {
                mailbox.release_terminal_tokens(&scope, 1);
            }
        }
    };
    let mut active = BTreeMap::<PlaybackScope, Vec<rodio::Sink>>::new();
    let mut terminal_active = BTreeMap::<PlaybackScope, usize>::new();
    let mut stopped_radial = VecDeque::<PlaybackScope>::new();
    loop {
        let command = mailbox.recv_timeout(Duration::from_millis(50));
        for sinks in active.values_mut() {
            sinks.retain(|sink| !sink.empty());
        }
        active.retain(|_, sinks| !sinks.is_empty());
        let completed = terminal_active
            .iter()
            .filter(|(scope, _)| !active.contains_key(*scope))
            .map(|(scope, tokens)| (scope.clone(), *tokens))
            .collect::<Vec<_>>();
        for (scope, tokens) in completed {
            terminal_active.remove(&scope);
            mailbox.release_terminal_tokens(&scope, tokens);
        }
        let Some(command) = command else { continue };
        match command {
            AudioCommand::Play { scope, wav } => {
                if stopped_radial.contains(&scope) {
                    continue;
                }
                if active.values().map(Vec::len).sum::<usize>() >= MAX_ACTIVE_SINKS {
                    continue;
                }
                if let Ok(source) = rodio::Decoder::new(Cursor::new(wav))
                    && let Ok(sink) = rodio::Sink::try_new(&handle)
                {
                    sink.append(source);
                    active.entry(scope).or_default().push(sink);
                }
            }
            AudioCommand::Stop(scope) => {
                if let Some(sinks) = active.remove(&scope) {
                    for sink in sinks {
                        sink.stop();
                    }
                }
                if let Some(tokens) = terminal_active.remove(&scope) {
                    mailbox.release_terminal_tokens(&scope, tokens);
                }
                if matches!(scope, PlaybackScope::Radial { .. }) {
                    stopped_radial.push_back(scope);
                    while stopped_radial.len() > 64 {
                        stopped_radial.pop_front();
                    }
                }
            }
            AudioCommand::Finish { scope, wav } => {
                if let Some(sinks) = active.remove(&scope) {
                    for sink in sinks {
                        sink.stop();
                    }
                }
                if let Some(tokens) = terminal_active.remove(&scope) {
                    mailbox.release_terminal_tokens(&scope, tokens);
                }
                // Accepted terminal cues own a separate bounded reserve. They
                // are never skipped merely because ordinary playback filled
                // its sink budget.
                let mut admitted = false;
                if can_admit_finish_sink(active.values().map(Vec::len).sum::<usize>())
                    && let Ok(source) = rodio::Decoder::new(Cursor::new(wav))
                    && let Ok(sink) = rodio::Sink::try_new(&handle)
                {
                    sink.append(source);
                    active.entry(scope.clone()).or_default().push(sink);
                    terminal_active.insert(scope.clone(), 1);
                    admitted = true;
                }
                if !admitted {
                    mailbox.release_terminal_tokens(&scope, 1);
                }
                if matches!(scope, PlaybackScope::Radial { .. }) {
                    stopped_radial.push_back(scope);
                    while stopped_radial.len() > 64 {
                        stopped_radial.pop_front();
                    }
                }
            }
        }
    }
}

fn can_admit_finish_sink(active_sink_count: usize) -> bool {
    active_sink_count < MAX_ACTIVE_SINKS.saturating_mul(2)
}

pub fn play_sound(name: &str) {
    if name == "None" {
        return;
    }
    if let Some(bytes) = SOUNDS
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, data)| *data)
    {
        let _ = AUDIO.play(PlaybackScope::Global, Arc::from(bytes));
    }
}

pub fn play_wav(scope: PlaybackScope, wav: Arc<[u8]>) -> bool {
    AUDIO.play(scope, wav)
}

pub fn stop(scope: PlaybackScope) -> bool {
    AUDIO.stop(scope)
}

pub fn finish_wav(scope: PlaybackScope, wav: Arc<[u8]>) -> bool {
    AUDIO.finish(scope, wav)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_mailbox_rejects_a_storm_without_spawning_workers() {
        let service = AudioService {
            mailbox: Arc::new(AudioMailbox::new(2)),
        };
        assert!(service.play(PlaybackScope::Global, Arc::from(&b"one"[..])));
        assert!(service.play(PlaybackScope::Global, Arc::from(&b"two"[..])));
        assert!(!service.play(PlaybackScope::Global, Arc::from(&b"three"[..])));
        service.stop(PlaybackScope::Radial {
            session: "closed".into(),
            generation: 1,
        });
        assert_eq!(service.mailbox.queue.lock().unwrap().len(), 2);
    }

    #[test]
    fn terminal_sound_replaces_stale_scope_queue_without_being_deleted() {
        let mailbox = AudioMailbox::new(4);
        let scope = PlaybackScope::Radial {
            session: "one".into(),
            generation: 9,
        };
        assert!(mailbox.play(AudioCommand::Play {
            scope: scope.clone(),
            wav: Arc::from(&b"select"[..])
        }));
        assert!(mailbox.finish(scope.clone(), Arc::from(&b"close"[..])));
        let queue = mailbox.queue.lock().unwrap();
        assert_eq!(queue.len(), 1);
        assert!(
            matches!(&queue[0], AudioCommand::Finish { scope: queued, wav } if queued == &scope && &**wav == b"close")
        );
    }

    #[test]
    fn terminal_finish_evicts_nonterminal_work_when_mailbox_is_full() {
        let mailbox = AudioMailbox::new(3);
        let closing = PlaybackScope::Radial {
            session: "closing".into(),
            generation: 4,
        };
        assert!(mailbox.play(AudioCommand::Play {
            scope: closing.clone(),
            wav: Arc::from(&b"select"[..]),
        }));
        for session in ["other-a", "other-b"] {
            assert!(mailbox.play(AudioCommand::Play {
                scope: PlaybackScope::Radial {
                    session: session.into(),
                    generation: 1,
                },
                wav: Arc::from(&b"hover"[..]),
            }));
        }
        assert!(mailbox.finish(closing.clone(), Arc::from(&b"close"[..])));
        let queue = mailbox.queue.lock().unwrap();
        assert_eq!(queue.len(), 3);
        assert!(!queue.iter().any(
            |command| matches!(command, AudioCommand::Play { scope, .. } if scope == &closing)
        ));
        assert!(queue.iter().any(
            |command| matches!(command, AudioCommand::Finish { scope, wav } if scope == &closing && &**wav == b"close")
        ));
    }

    #[test]
    fn reserved_terminal_capacity_never_evicts_an_accepted_closed_scope() {
        let mailbox = AudioMailbox::new(2);
        let scopes = (0..4)
            .map(|generation| PlaybackScope::Radial {
                session: format!("terminal-{generation}"),
                generation,
            })
            .collect::<Vec<_>>();
        assert!(mailbox.stop(scopes[0].clone()));
        assert!(mailbox.finish(scopes[1].clone(), Arc::from(&b"close-1"[..])));
        // The ordinary queue is now entirely saturated by terminals. Reserved
        // capacity admits later terminal work without evicting either one.
        assert!(mailbox.stop(scopes[2].clone()));
        assert!(mailbox.finish(scopes[3].clone(), Arc::from(&b"close-3"[..])));
        // Same-scope replacement is the only terminal coalescing allowed.
        assert!(mailbox.finish(scopes[2].clone(), Arc::from(&b"close-2"[..])));
        let queue = mailbox.queue.lock().unwrap();
        assert_eq!(queue.len(), scopes.len());
        for scope in &scopes {
            assert!(queue.iter().any(|command| match command {
                AudioCommand::Stop(queued) => queued == scope,
                AudioCommand::Finish { scope: queued, .. } => queued == scope,
                AudioCommand::Play { .. } => false,
            }));
        }
        assert!(queue.iter().any(|command| {
            matches!(command, AudioCommand::Finish { scope, wav } if scope == &scopes[2] && &**wav == b"close-2")
        }));
    }

    #[test]
    fn mailbox_has_an_absolute_bound_and_terminal_scope_limit() {
        let mailbox = AudioMailbox::new(2);
        for generation in 0..MAX_ACTIVE_SINKS {
            assert!(mailbox.stop(PlaybackScope::Radial {
                session: format!("closed-{generation}"),
                generation: generation as u64,
            }));
        }
        assert!(!mailbox.stop(PlaybackScope::Radial {
            session: "beyond-terminal-reserve".into(),
            generation: 99,
        }));
        assert!(mailbox.play(AudioCommand::Play {
            scope: PlaybackScope::Global,
            wav: Arc::from(&b"one"[..]),
        }));
        assert!(mailbox.play(AudioCommand::Play {
            scope: PlaybackScope::Global,
            wav: Arc::from(&b"two"[..]),
        }));
        assert!(!mailbox.play(AudioCommand::Play {
            scope: PlaybackScope::Global,
            wav: Arc::from(&b"overflow"[..]),
        }));
        assert_eq!(mailbox.queue.lock().unwrap().len(), MAX_ACTIVE_SINKS + 2);
    }

    #[test]
    fn finish_has_reserved_sink_capacity_when_all_ordinary_sinks_are_active() {
        assert!(can_admit_finish_sink(MAX_ACTIVE_SINKS));
        assert!(can_admit_finish_sink(MAX_ACTIVE_SINKS * 2 - 1));
        assert!(!can_admit_finish_sink(MAX_ACTIVE_SINKS * 2));
    }

    #[test]
    fn queued_and_active_finish_scopes_share_one_reservation_limit() {
        let mailbox = AudioMailbox::new(2);
        let scopes = (0..MAX_ACTIVE_SINKS)
            .map(|generation| PlaybackScope::Radial {
                session: format!("finish-{generation}"),
                generation: generation as u64,
            })
            .collect::<Vec<_>>();
        for scope in &scopes {
            assert!(mailbox.finish(scope.clone(), Arc::from(&b"close"[..])));
        }
        let later = PlaybackScope::Radial {
            session: "finish-later".into(),
            generation: 99,
        };
        assert!(!mailbox.finish(later.clone(), Arc::from(&b"later"[..])));
        let AudioCommand::Finish {
            scope: completed, ..
        } = mailbox.recv()
        else {
            panic!("expected queued finish")
        };
        mailbox.release_terminal_tokens(&completed, 1);
        assert!(mailbox.finish(later, Arc::from(&b"later"[..])));
        assert_eq!(
            mailbox.terminal_scopes.lock().unwrap().len(),
            MAX_ACTIVE_SINKS
        );
    }

    #[test]
    fn active_finish_and_queued_replacement_own_independent_tokens() {
        let mailbox = AudioMailbox::new(2);
        let scope = PlaybackScope::Radial {
            session: "replacement".into(),
            generation: 7,
        };
        assert!(mailbox.finish(scope.clone(), Arc::from(&b"old"[..])));
        let AudioCommand::Finish { .. } = mailbox.recv() else {
            panic!("expected old finish to transition to active")
        };
        assert!(mailbox.finish(scope.clone(), Arc::from(&b"replacement"[..])));
        assert_eq!(mailbox.terminal_scopes.lock().unwrap()[&scope], 2);
        // Natural completion of the old sink releases only its active token.
        mailbox.release_terminal_tokens(&scope, 1);
        assert_eq!(mailbox.terminal_scopes.lock().unwrap()[&scope], 1);
        let AudioCommand::Finish { scope: queued, .. } = mailbox.recv() else {
            panic!("expected queued replacement")
        };
        assert_eq!(queued, scope);
        mailbox.release_terminal_tokens(&scope, 1);
        assert!(mailbox.terminal_scopes.lock().unwrap().is_empty());
    }
}
