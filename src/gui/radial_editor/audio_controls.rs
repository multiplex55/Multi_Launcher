//! Explicit sound audition; preview rendering never calls this module.

use crate::radial::audio::{
    PreparedRadialSounds, RadialAudioSession, RadialCue, SystemRadialAudioOutput,
};
use crate::radial::model::SessionId;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_AUDITION: AtomicU64 = AtomicU64::new(1);

pub(super) fn audition(wav: Arc<[u8]>) -> bool {
    let generation = NEXT_AUDITION.fetch_add(1, Ordering::Relaxed);
    let id = SessionId::new(format!("radial-editor-audition-{generation}"));
    let mut session = RadialAudioSession::new(
        id.clone(),
        generation,
        PreparedRadialSounds {
            open: Some(wav),
            ..Default::default()
        },
        SystemRadialAudioOutput,
    );
    session.cue(&id, generation, RadialCue::Open, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radial::audio::RadialAudioOutput;
    use std::sync::atomic::AtomicUsize;

    struct CountOutput(Arc<AtomicUsize>);
    impl RadialAudioOutput for CountOutput {
        fn play(&mut self, _: &SessionId, _: u64, _: Arc<[u8]>) -> bool {
            self.0.fetch_add(1, Ordering::Relaxed);
            true
        }
        fn stop(&mut self, _: &SessionId, _: u64) -> bool {
            true
        }
    }

    #[test]
    fn one_explicit_audition_request_plays_once() {
        let count = Arc::new(AtomicUsize::new(0));
        let id = SessionId::new("audition-test");
        let mut session = RadialAudioSession::new(
            id.clone(),
            1,
            PreparedRadialSounds {
                open: Some(Arc::from(&b"wav"[..])),
                ..Default::default()
            },
            CountOutput(Arc::clone(&count)),
        );
        assert!(session.cue(&id, 1, RadialCue::Open, 0));
        assert!(!session.cue(&id, 1, RadialCue::Open, 1));
        assert_eq!(count.load(Ordering::Relaxed), 1);
    }
}
