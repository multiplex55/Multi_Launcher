//! One global polling worker for all recorder controls.
use super::{
    MkHotkey, MkKey, MkMacroDocument, MkMacroStore,
    hotkeys::{KeyStateBackend, compile_hotkey},
};
use std::{
    collections::BTreeSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecorderControlAction {
    Toggle,
    PauseResume,
    Marker,
}
type Callback<'a> = dyn Fn(RecorderControlAction) + Send + Sync + 'a;
#[derive(Clone, PartialEq, Eq)]
struct Binding {
    modifiers: BTreeSet<String>,
    primary: MkKey,
    triggered: bool,
}
impl Binding {
    fn compile(hotkey: &MkHotkey, backend: &dyn KeyStateBackend) -> Option<Self> {
        let (modifiers, primary) = compile_hotkey(hotkey)?;
        let triggered = chord_down(&primary, &modifiers, backend);
        Some(Self {
            modifiers,
            primary,
            triggered,
        })
    }
}
struct State {
    snapshot: Arc<MkMacroDocument>,
    bindings: [Option<Binding>; 3],
}
pub struct RecorderHotkeyService {
    stop: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
}
impl RecorderHotkeyService {
    pub fn new(
        store: Arc<MkMacroStore>,
        backend: Arc<dyn KeyStateBackend>,
        toggle: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        Self::with_controls(
            store,
            backend,
            Arc::new(move |action| {
                if action == RecorderControlAction::Toggle {
                    toggle()
                }
            }),
        )
    }
    pub fn with_controls(
        store: Arc<MkMacroStore>,
        backend: Arc<dyn KeyStateBackend>,
        callback: Arc<Callback<'static>>,
    ) -> Self {
        let snapshot = store.snapshot();
        let bindings = compile_bindings(&snapshot, backend.as_ref());
        let state = Arc::new(Mutex::new(State { snapshot, bindings }));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker = thread::Builder::new()
            .name("mkmacro-recorder-hotkeys".into())
            .spawn(move || {
                while !worker_stop.load(Ordering::SeqCst) {
                    tick(&store, &state, backend.as_ref(), callback.as_ref());
                    thread::sleep(Duration::from_millis(20));
                }
            })
            .expect("spawn recorder hotkey service");
        Self {
            stop,
            worker: Mutex::new(Some(worker)),
        }
    }
    pub fn system(store: Arc<MkMacroStore>) -> Self {
        Self::with_controls(
            store,
            Arc::new(super::hotkeys::SystemKeyStateBackend),
            Arc::new(super::runtime::recorder_control),
        )
    }
    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.lock().unwrap().take() {
            let _ = worker.join();
        }
    }
}
impl Drop for RecorderHotkeyService {
    fn drop(&mut self) {
        self.shutdown();
    }
}
fn compile_bindings(doc: &MkMacroDocument, backend: &dyn KeyStateBackend) -> [Option<Binding>; 3] {
    let toggle_name = super::hotkeys::canonical_hotkey(&doc.settings.record_toggle_hotkey);
    let pause_name = doc
        .settings
        .recorder
        .pause_resume_hotkey
        .as_ref()
        .map(super::hotkeys::canonical_hotkey);
    let marker_name = doc
        .settings
        .recorder
        .marker_hotkey
        .as_ref()
        .map(super::hotkeys::canonical_hotkey);
    let pause_conflict = pause_name.as_ref().is_some_and(|name| *name == toggle_name);
    let marker_conflict = marker_name.as_ref().is_some_and(|name| {
        *name == toggle_name || pause_name.as_ref().is_some_and(|pause| pause == name)
    });
    [
        Binding::compile(&doc.settings.record_toggle_hotkey, backend),
        (!pause_conflict)
            .then_some(doc.settings.recorder.pause_resume_hotkey.as_ref())
            .flatten()
            .and_then(|h| Binding::compile(h, backend)),
        (!marker_conflict)
            .then_some(doc.settings.recorder.marker_hotkey.as_ref())
            .flatten()
            .and_then(|h| Binding::compile(h, backend)),
    ]
}
fn tick(
    store: &MkMacroStore,
    state: &Mutex<State>,
    backend: &dyn KeyStateBackend,
    callback: &Callback<'_>,
) {
    let snapshot = store.snapshot();
    let mut fired = Vec::new();
    {
        let mut s = state.lock().unwrap();
        if !Arc::ptr_eq(&snapshot, &s.snapshot) {
            let next = compile_bindings(&snapshot, backend);
            for (index, new) in next.into_iter().enumerate() {
                let changed = s.bindings[index]
                    .as_ref()
                    .map(|b| (&b.modifiers, &b.primary))
                    != new.as_ref().map(|b| (&b.modifiers, &b.primary));
                if changed {
                    s.bindings[index] = new;
                }
            }
            s.snapshot = snapshot;
        }
        for (index, binding) in s.bindings.iter_mut().enumerate() {
            if let Some(binding) = binding {
                let down = chord_down(&binding.primary, &binding.modifiers, backend);
                if down && !binding.triggered {
                    fired.push(match index {
                        0 => RecorderControlAction::Toggle,
                        1 => RecorderControlAction::PauseResume,
                        _ => RecorderControlAction::Marker,
                    });
                }
                binding.triggered = down;
            }
        }
    }
    for action in fired {
        callback(action)
    }
}
fn chord_down(
    primary: &MkKey,
    modifiers: &BTreeSet<String>,
    backend: &dyn KeyStateBackend,
) -> bool {
    backend.is_down(primary)
        && (!modifiers.contains("CONTROL") || backend.is_down(&MkKey::Control))
        && (!modifiers.contains("SHIFT") || backend.is_down(&MkKey::Shift))
        && (!modifiers.contains("ALT") || backend.is_down(&MkKey::Alt))
        && (!modifiers.contains("META") || backend.is_down(&MkKey::Meta))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::RwLock;
    struct Fake(RwLock<Vec<MkKey>>);
    impl KeyStateBackend for Fake {
        fn is_down(&self, key: &MkKey) -> bool {
            self.0.read().unwrap().contains(key)
        }
    }
    #[test]
    fn controls_have_independent_rising_edges() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        let mut doc = (*store.snapshot()).clone();
        doc.settings.recorder.pause_resume_hotkey = Some(MkHotkey {
            key: MkKey::Function(8),
            modifiers: vec![],
        });
        store.save(doc).unwrap();
        let fake = Fake(RwLock::new(vec![]));
        let snapshot = store.snapshot();
        let state = Mutex::new(State {
            bindings: compile_bindings(&snapshot, &fake),
            snapshot,
        });
        let seen = Mutex::new(Vec::new());
        fake.0.write().unwrap().push(MkKey::Function(9));
        tick(&store, &state, &fake, &|a| seen.lock().unwrap().push(a));
        fake.0.write().unwrap().push(MkKey::Function(8));
        tick(&store, &state, &fake, &|a| seen.lock().unwrap().push(a));
        assert_eq!(
            *seen.lock().unwrap(),
            vec![
                RecorderControlAction::Toggle,
                RecorderControlAction::PauseResume
            ]
        );
    }

    #[test]
    fn duplicate_recorder_controls_are_not_armed_twice() {
        let mut doc = MkMacroDocument::default();
        doc.settings.recorder.pause_resume_hotkey = Some(doc.settings.record_toggle_hotkey.clone());
        doc.settings.recorder.marker_hotkey = Some(doc.settings.record_toggle_hotkey.clone());
        let fake = Fake(RwLock::new(vec![MkKey::Function(9)]));
        let bindings = compile_bindings(&doc, &fake);
        assert!(bindings[0].is_some());
        assert!(bindings[1].is_none());
        assert!(bindings[2].is_none());
        assert!(
            crate::mkmacro::validate_document(&doc, None)
                .iter()
                .any(|diagnostic| diagnostic.code == "duplicate_recorder_control_hotkey")
        );
    }

    #[test]
    fn refreshed_binding_held_during_configuration_waits_for_a_new_rising_edge() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        let fake = Fake(RwLock::new(vec![]));
        let snapshot = store.snapshot();
        let state = Mutex::new(State {
            bindings: compile_bindings(&snapshot, &fake),
            snapshot,
        });
        let seen = Mutex::new(Vec::new());

        fake.0.write().unwrap().push(MkKey::Function(10));
        let mut doc = (*store.snapshot()).clone();
        doc.settings.record_toggle_hotkey = MkHotkey {
            key: MkKey::Function(10),
            modifiers: vec![],
        };
        store.save(doc).unwrap();
        tick(&store, &state, &fake, &|action| {
            seen.lock().unwrap().push(action)
        });
        assert!(seen.lock().unwrap().is_empty());

        fake.0.write().unwrap().clear();
        tick(&store, &state, &fake, &|action| {
            seen.lock().unwrap().push(action)
        });
        fake.0.write().unwrap().push(MkKey::Function(10));
        tick(&store, &state, &fake, &|action| {
            seen.lock().unwrap().push(action)
        });
        tick(&store, &state, &fake, &|action| {
            seen.lock().unwrap().push(action)
        });
        assert_eq!(*seen.lock().unwrap(), vec![RecorderControlAction::Toggle]);
    }
}
