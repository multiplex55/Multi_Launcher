use super::SequencePlaybackError;
use crate::mkmacro::input::{InputSink, Win32InputBackend};
use crate::mkmacro::{InputBackend, MkKey};

#[derive(Default)]
pub(super) struct PasteModifiers {
    pub control: bool,
    pub shift: bool,
    pub alt: bool,
    pub windows: bool,
}

/// Preserve foreign-held Control. Releasing it would break the user's next
/// ordinary Ctrl+V after the sequence completes. The existing backend emits
/// tagged key events; this boundary balances only its accepted keys on failure.
pub(super) fn paste_chord<S: InputSink>(
    input: &Win32InputBackend<S>,
    modifiers: PasteModifiers,
) -> Result<(), SequencePlaybackError> {
    if modifiers.shift || modifiers.alt || modifiers.windows {
        return Err(SequencePlaybackError::InputRejected);
    }
    struct OwnedKeys<'a, S: InputSink> {
        input: &'a Win32InputBackend<S>,
        keys: Vec<MkKey>,
    }
    impl<S: InputSink> Drop for OwnedKeys<'_, S> {
        fn drop(&mut self) {
            for key in self.keys.iter().rev() {
                let _ = self.input.key_up(key);
            }
        }
    }
    let mut owned = OwnedKeys {
        input,
        keys: Vec::with_capacity(2),
    };
    let v = MkKey::Character("V".into());
    if !modifiers.control {
        input
            .key_down(&MkKey::Control)
            .map_err(|_| SequencePlaybackError::InputRejected)?;
        owned.keys.push(MkKey::Control);
    }
    input
        .key_down(&v)
        .map_err(|_| SequencePlaybackError::InputRejected)?;
    owned.keys.push(v);
    while let Some(key) = owned.keys.last() {
        input
            .key_up(key)
            .map_err(|_| SequencePlaybackError::InputRejected)?;
        owned.keys.pop();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::input::{KEYEVENTF_KEYUP_, MKMACRO_EXTRA_INFO, RawInputEvent};
    use std::sync::Mutex;

    #[derive(Default)]
    struct Sink {
        events: Mutex<Vec<RawInputEvent>>,
        calls: Mutex<usize>,
        fail: Option<usize>,
    }
    impl InputSink for &Sink {
        fn send(&self, events: &[RawInputEvent]) -> Result<usize, String> {
            self.events.lock().unwrap().extend_from_slice(events);
            let mut calls = self.calls.lock().unwrap();
            *calls += 1;
            if self.fail == Some(*calls) {
                Err("fake input rejection".into())
            } else {
                Ok(events.len())
            }
        }
    }

    #[test]
    fn paste_input_preserves_held_control_and_balances_only_owned_keys() {
        let sink = Sink::default();
        let input = Win32InputBackend::with_sink(&sink);
        for _ in 0..3 {
            paste_chord(
                &input,
                PasteModifiers {
                    control: true,
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let events = sink.events.lock().unwrap();
        assert_eq!(events.len(), 6);
        for (index, event) in events.iter().enumerate() {
            assert!(
                matches!(event, RawInputEvent::Keyboard { vk: 0x56, flags, extra, .. }
                if (*flags & KEYEVENTF_KEYUP_ != 0) == (index % 2 == 1) && *extra == MKMACRO_EXTRA_INFO)
            );
        }
        drop(events);
        // A fourth physical OS paste still has the user's Control down: no
        // injected event in the three sequence pastes released either Control.
        let empty = Sink::default();
        paste_chord(
            &Win32InputBackend::with_sink(&empty),
            PasteModifiers::default(),
        )
        .unwrap();
        assert_eq!(empty.events.lock().unwrap().len(), 4);
        let failing = Sink {
            fail: Some(2),
            ..Default::default()
        };
        assert!(
            paste_chord(
                &Win32InputBackend::with_sink(&failing),
                PasteModifiers::default()
            )
            .is_err()
        );
        let events = failing.events.lock().unwrap();
        assert!(
            matches!(events.last(), Some(RawInputEvent::Keyboard { vk: 0x11, flags, .. }) if flags & KEYEVENTF_KEYUP_ != 0)
        );
        let failed_up = Sink {
            fail: Some(2),
            ..Default::default()
        };
        assert!(
            paste_chord(
                &Win32InputBackend::with_sink(&failed_up),
                PasteModifiers {
                    control: true,
                    ..Default::default()
                }
            )
            .is_err()
        );
        let events = failed_up.events.lock().unwrap();
        assert_eq!(events.len(), 3);
        assert!(
            events
                .iter()
                .all(|event| matches!(event, RawInputEvent::Keyboard { vk: 0x56, .. }))
        );
        assert!(
            matches!(events.last(), Some(RawInputEvent::Keyboard { flags, .. }) if flags & KEYEVENTF_KEYUP_ != 0)
        );
        for modifiers in [
            PasteModifiers {
                shift: true,
                ..Default::default()
            },
            PasteModifiers {
                alt: true,
                ..Default::default()
            },
            PasteModifiers {
                windows: true,
                ..Default::default()
            },
        ] {
            let blocked = Sink::default();
            assert!(paste_chord(&Win32InputBackend::with_sink(&blocked), modifiers).is_err());
            assert!(blocked.events.lock().unwrap().is_empty());
        }
    }
}
