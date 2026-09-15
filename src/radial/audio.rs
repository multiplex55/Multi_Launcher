use super::model::{CellId, MenuId, SessionId};
use std::collections::BTreeSet;
use std::sync::Arc;

pub const SELECTION_CUE_DEBOUNCE_MS: u64 = 40;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RadialCue {
    Open,
    Close,
    Select(CellId),
    SubmenuShow(MenuId),
    SubmenuClose(MenuId),
}

pub struct RadialCueCoordinator {
    session: SessionId,
    generation: u64,
    once: BTreeSet<RadialCue>,
    last_selection: Option<(CellId, u64)>,
    last_transition: Option<(RadialCue, u64)>,
    closed: bool,
}

#[derive(Clone, Debug, Default)]
pub struct PreparedRadialSounds {
    pub open: Option<Arc<[u8]>>,
    pub close: Option<Arc<[u8]>>,
    pub select: Option<Arc<[u8]>>,
    pub submenu_show: Option<Arc<[u8]>>,
    pub submenu_close: Option<Arc<[u8]>>,
}

pub trait RadialAudioOutput {
    fn play(&mut self, session: &SessionId, generation: u64, wav: Arc<[u8]>) -> bool;
    fn stop(&mut self, session: &SessionId, generation: u64) -> bool;
    fn finish(&mut self, session: &SessionId, generation: u64, wav: Arc<[u8]>) -> bool {
        if !self.stop(session, generation) {
            return false;
        }
        self.play(session, generation, wav)
    }
}

pub struct SystemRadialAudioOutput;
impl RadialAudioOutput for SystemRadialAudioOutput {
    fn play(&mut self, session: &SessionId, generation: u64, wav: Arc<[u8]>) -> bool {
        crate::sound::play_wav(
            crate::sound::PlaybackScope::Radial {
                session: session.as_str().to_owned(),
                generation,
            },
            wav,
        )
    }
    fn stop(&mut self, session: &SessionId, generation: u64) -> bool {
        crate::sound::stop(crate::sound::PlaybackScope::Radial {
            session: session.as_str().to_owned(),
            generation,
        })
    }
    fn finish(&mut self, session: &SessionId, generation: u64, wav: Arc<[u8]>) -> bool {
        crate::sound::finish_wav(
            crate::sound::PlaybackScope::Radial {
                session: session.as_str().to_owned(),
                generation,
            },
            wav,
        )
    }
}

pub struct RadialAudioSession<O> {
    coordinator: RadialCueCoordinator,
    sounds: PreparedRadialSounds,
    output: O,
}

/// Explicit one-shot audition used by the main-owned authoring service. The
/// caller must supply already validated WAV bytes; previews never call this.
pub fn audition_wav(session: SessionId, generation: u64, wav: Arc<[u8]>) -> bool {
    let mut audio = RadialAudioSession::new(
        session.clone(),
        generation,
        PreparedRadialSounds {
            open: Some(wav),
            ..Default::default()
        },
        SystemRadialAudioOutput,
    );
    audio.cue(&session, generation, RadialCue::Open, 0)
}

impl<O: RadialAudioOutput> RadialAudioSession<O> {
    pub fn new(
        session: SessionId,
        generation: u64,
        sounds: PreparedRadialSounds,
        output: O,
    ) -> Self {
        Self {
            coordinator: RadialCueCoordinator::new(session, generation),
            sounds,
            output,
        }
    }

    pub fn cue(
        &mut self,
        session: &SessionId,
        generation: u64,
        cue: RadialCue,
        at_ms: u64,
    ) -> bool {
        if !self
            .coordinator
            .admit(session, generation, cue.clone(), at_ms)
        {
            return false;
        }
        let sound = match cue {
            RadialCue::Open => self.sounds.open.clone(),
            RadialCue::Close => self.sounds.close.clone(),
            RadialCue::Select(_) => self.sounds.select.clone(),
            RadialCue::SubmenuShow(_) => self.sounds.submenu_show.clone(),
            RadialCue::SubmenuClose(_) => self.sounds.submenu_close.clone(),
        };
        sound.is_none_or(|wav| self.output.play(session, generation, wav))
    }

    pub fn stop(mut self, session: &SessionId, generation: u64) -> bool {
        self.output.stop(session, generation)
    }

    pub fn replace_sounds(&mut self, sounds: PreparedRadialSounds) {
        self.sounds = sounds;
    }

    /// Retire the scope while preserving the admitted close cue. Old queued
    /// selection/submenu sounds are removed atomically by the output mailbox.
    pub fn finish_close(mut self, session: &SessionId, generation: u64, at_ms: u64) -> bool {
        if !self
            .coordinator
            .admit(session, generation, RadialCue::Close, at_ms)
        {
            let _ = self.output.stop(session, generation);
            return false;
        }
        match self.sounds.close.take() {
            Some(wav) => self.output.finish(session, generation, wav),
            None => self.output.stop(session, generation),
        }
    }
}

impl RadialCueCoordinator {
    pub fn new(session: SessionId, generation: u64) -> Self {
        Self {
            session,
            generation,
            once: BTreeSet::new(),
            last_selection: None,
            last_transition: None,
            closed: false,
        }
    }

    pub fn admit(
        &mut self,
        session: &SessionId,
        generation: u64,
        cue: RadialCue,
        at_ms: u64,
    ) -> bool {
        if self.closed || session != &self.session || generation != self.generation {
            return false;
        }
        match &cue {
            RadialCue::Select(cell) => {
                if self.last_selection.as_ref().is_some_and(|(previous, at)| {
                    previous == cell && at_ms.saturating_sub(*at) < SELECTION_CUE_DEBOUNCE_MS
                }) {
                    return false;
                }
                self.last_selection = Some((cell.clone(), at_ms));
                true
            }
            RadialCue::Close => {
                self.closed = true;
                self.once.insert(cue)
            }
            RadialCue::Open => self.once.insert(cue),
            RadialCue::SubmenuShow(_) | RadialCue::SubmenuClose(_) => {
                if self.last_transition.as_ref() == Some(&(cue.clone(), at_ms)) {
                    false
                } else {
                    self.last_transition = Some((cue, at_ms));
                    true
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn lifecycle_cues_are_once_selection_is_debounced_and_stale_generation_is_rejected() {
        let id = SessionId::new("audio");
        let mut cues = RadialCueCoordinator::new(id.clone(), 7);
        assert!(cues.admit(&id, 7, RadialCue::Open, 0));
        assert!(!cues.admit(&id, 7, RadialCue::Open, 1));
        let cell = CellId::new("a");
        assert!(cues.admit(&id, 7, RadialCue::Select(cell.clone()), 10));
        assert!(!cues.admit(&id, 7, RadialCue::Select(cell.clone()), 20));
        assert!(cues.admit(&id, 7, RadialCue::Select(cell), 60));
        let submenu = MenuId::new("child");
        assert!(cues.admit(&id, 7, RadialCue::SubmenuShow(submenu.clone()), 61));
        assert!(!cues.admit(&id, 7, RadialCue::SubmenuShow(submenu.clone()), 61));
        assert!(cues.admit(&id, 7, RadialCue::SubmenuClose(submenu.clone()), 62));
        assert!(cues.admit(&id, 7, RadialCue::SubmenuShow(submenu), 63));
        assert!(!cues.admit(&id, 8, RadialCue::Close, 70));
        assert!(cues.admit(&id, 7, RadialCue::Close, 70));
        assert!(!cues.admit(&id, 7, RadialCue::Select(CellId::new("b")), 80));
    }

    #[derive(Default)]
    struct Output {
        played: usize,
        stopped: usize,
    }
    impl RadialAudioOutput for Output {
        fn play(&mut self, _: &SessionId, _: u64, _: Arc<[u8]>) -> bool {
            self.played += 1;
            true
        }
        fn stop(&mut self, _: &SessionId, _: u64) -> bool {
            self.stopped += 1;
            true
        }
    }

    #[test]
    fn prepared_lifecycle_cues_share_the_retained_generation_tagged_output() {
        let id = SessionId::new("prepared-audio");
        let sounds = PreparedRadialSounds {
            open: Some(Arc::from(&b"open"[..])),
            close: Some(Arc::from(&b"close"[..])),
            select: Some(Arc::from(&b"select"[..])),
            ..Default::default()
        };
        let mut session = RadialAudioSession::new(id.clone(), 2, sounds, Output::default());
        assert!(session.cue(&id, 2, RadialCue::Open, 0));
        assert!(!session.cue(&id, 2, RadialCue::Open, 1));
        assert!(session.cue(&id, 2, RadialCue::Select(CellId::new("a")), 50));
        assert!(!session.cue(&id, 3, RadialCue::Close, 60));
        assert!(session.cue(&id, 2, RadialCue::Close, 70));
        assert_eq!(session.output.played, 3);
        session.stop(&id, 2);
    }

    #[derive(Clone, Default)]
    struct MailboxOutput(Arc<Mutex<Vec<String>>>);
    impl RadialAudioOutput for MailboxOutput {
        fn play(&mut self, _: &SessionId, _: u64, wav: Arc<[u8]>) -> bool {
            self.0
                .lock()
                .unwrap()
                .push(format!("play:{}", String::from_utf8_lossy(&wav)));
            true
        }
        fn stop(&mut self, _: &SessionId, _: u64) -> bool {
            self.0.lock().unwrap().push("stop".into());
            true
        }
    }

    #[test]
    fn child_sound_set_replaces_parent_and_terminal_close_survives_scope_retirement() {
        let id = SessionId::new("mailbox-audio");
        let output = MailboxOutput::default();
        let log = Arc::clone(&output.0);
        let mut session = RadialAudioSession::new(
            id.clone(),
            4,
            PreparedRadialSounds {
                submenu_show: Some(Arc::from(&b"parent-sub"[..])),
                ..Default::default()
            },
            output,
        );
        session.replace_sounds(PreparedRadialSounds {
            submenu_show: Some(Arc::from(&b"child-sub"[..])),
            submenu_close: Some(Arc::from(&b"child-back"[..])),
            close: Some(Arc::from(&b"child-close"[..])),
            ..Default::default()
        });
        assert!(session.cue(&id, 4, RadialCue::SubmenuShow(MenuId::new("child")), 10));
        assert!(session.cue(&id, 4, RadialCue::SubmenuClose(MenuId::new("child")), 11));
        assert!(session.finish_close(&id, 4, 12));
        assert_eq!(
            &*log.lock().unwrap(),
            &[
                "play:child-sub",
                "play:child-back",
                "stop",
                "play:child-close"
            ]
        );
    }
}
