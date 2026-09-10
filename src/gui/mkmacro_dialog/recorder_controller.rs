//! Authoritative recorder-control availability shared by present and future
//! recorder surfaces.
use crate::mkmacro::{RecorderRuntimeState, RuntimeState};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RecordingControlState {
    pub record: bool,
    pub pause: bool,
    pub resume: bool,
    pub stop: bool,
    pub marker: bool,
    pub annotate: bool,
}

/// One authoritative derivation shared by the docked toolbar and an optional
/// floating controller. It reflects modal ownership and runtime admission
/// before a command reaches either worker.
pub fn decide_recording_controls(
    playback: RuntimeState,
    recorder: RecorderRuntimeState,
    has_target: bool,
    has_review: bool,
    modal_open: bool,
) -> RecordingControlState {
    let playback_idle = !matches!(
        playback,
        RuntimeState::Running | RuntimeState::Paused | RuntimeState::Stopping
    );
    let recording = recorder == RecorderRuntimeState::Recording;
    let paused = recorder == RecorderRuntimeState::Paused;
    RecordingControlState {
        record: playback_idle
            && recorder == RecorderRuntimeState::Idle
            && has_target
            && !has_review
            && !modal_open,
        pause: recording && !modal_open,
        resume: paused && !modal_open,
        stop: recording || paused,
        marker: (recording || paused) && !modal_open,
        annotate: (recording || paused) && !modal_open,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_derive_exclusively_from_runtime_recorder_and_modal_state() {
        let idle = decide_recording_controls(
            RuntimeState::Idle,
            RecorderRuntimeState::Idle,
            true,
            false,
            false,
        );
        assert!(idle.record);
        assert!(!idle.pause && !idle.stop);

        let recording = decide_recording_controls(
            RuntimeState::Idle,
            RecorderRuntimeState::Recording,
            true,
            false,
            false,
        );
        assert!(recording.pause && recording.stop && recording.marker && recording.annotate);
        assert!(!recording.record && !recording.resume);

        let modal = decide_recording_controls(
            RuntimeState::Idle,
            RecorderRuntimeState::Paused,
            true,
            false,
            true,
        );
        assert!(modal.stop);
        assert!(!modal.resume && !modal.marker && !modal.annotate);

        assert!(
            !decide_recording_controls(
                RuntimeState::Running,
                RecorderRuntimeState::Idle,
                true,
                false,
                false,
            )
            .record
        );
    }
}
