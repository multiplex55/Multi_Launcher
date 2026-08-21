//! Global recorder-control polling, deliberately separate from macro playback bindings.
use super::{
    MkKey, MkMacroDocument, MkMacroStore,
    hotkeys::{KeyStateBackend, compile_hotkey},
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

type Toggle = dyn Fn() + Send + Sync;
struct State {
    snapshot: Arc<MkMacroDocument>,
    modifiers: std::collections::BTreeSet<String>,
    primary: Option<MkKey>,
    triggered: bool,
}
pub struct RecorderHotkeyService {
    stop: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
}
impl RecorderHotkeyService {
    pub fn new(
        store: Arc<MkMacroStore>,
        backend: Arc<dyn KeyStateBackend>,
        toggle: Arc<Toggle>,
    ) -> Self {
        let snapshot = store.snapshot();
        let compiled = compile_hotkey(&snapshot.settings.record_toggle_hotkey);
        let state = Arc::new(Mutex::new(State {
            snapshot,
            modifiers: compiled.as_ref().map(|x| x.0.clone()).unwrap_or_default(),
            primary: compiled.map(|x| x.1),
            triggered: false,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker = thread::Builder::new()
            .name("mkmacro-recorder-hotkey".into())
            .spawn(move || {
                while !worker_stop.load(Ordering::SeqCst) {
                    tick(&store, &state, backend.as_ref(), toggle.as_ref());
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
        Self::new(
            store,
            Arc::new(super::hotkeys::SystemKeyStateBackend),
            Arc::new(super::runtime::toggle_recording),
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

fn tick(
    store: &MkMacroStore,
    state: &Mutex<State>,
    backend: &dyn KeyStateBackend,
    toggle: &Toggle,
) {
    let snapshot = store.snapshot();
    let fire = {
        let mut s = state.lock().unwrap();
        if !Arc::ptr_eq(&snapshot, &s.snapshot) {
            let compiled = compile_hotkey(&snapshot.settings.record_toggle_hotkey);
            s.modifiers = compiled.as_ref().map(|x| x.0.clone()).unwrap_or_default();
            s.primary = compiled.map(|x| x.1);
            // A chord held while configuration changes must first be released.
            s.triggered = s
                .primary
                .as_ref()
                .is_some_and(|key| chord_down(key, &s.modifiers, backend));
            s.snapshot = snapshot;
        }
        let down = s
            .primary
            .as_ref()
            .is_some_and(|key| chord_down(key, &s.modifiers, backend));
        let fire = down && !s.triggered;
        s.triggered = down;
        fire
    };
    if fire {
        toggle();
    }
}
fn chord_down(
    primary: &MkKey,
    modifiers: &std::collections::BTreeSet<String>,
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
    use crate::mkmacro::{MkHotkey, MkMacroDocument};
    use std::sync::RwLock;

    struct Fake(RwLock<Vec<MkKey>>);
    impl KeyStateBackend for Fake {
        fn is_down(&self, key: &MkKey) -> bool {
            self.0.read().unwrap().contains(key)
        }
    }
    #[test]
    fn held_key_fires_once_then_release_and_repress_fires_again() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        let snapshot = store.snapshot();
        let (mods, primary) = compile_hotkey(&snapshot.settings.record_toggle_hotkey).unwrap();
        let state = Mutex::new(State {
            snapshot,
            modifiers: mods,
            primary: Some(primary),
            triggered: false,
        });
        let fake = Fake(RwLock::new(vec![MkKey::Function(9)]));
        let count = Mutex::new(0);
        let callback = || *count.lock().unwrap() += 1;
        tick(&store, &state, &fake, &callback);
        tick(&store, &state, &fake, &callback);
        assert_eq!(*count.lock().unwrap(), 1);
        fake.0.write().unwrap().clear();
        tick(&store, &state, &fake, &callback);
        fake.0.write().unwrap().push(MkKey::Function(9));
        tick(&store, &state, &fake, &callback);
        assert_eq!(*count.lock().unwrap(), 2);
    }
    #[test]
    fn refresh_to_held_modifier_chord_waits_for_release() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        let snapshot = store.snapshot();
        let (mods, primary) = compile_hotkey(&snapshot.settings.record_toggle_hotkey).unwrap();
        let state = Mutex::new(State {
            snapshot,
            modifiers: mods,
            primary: Some(primary),
            triggered: false,
        });
        let fake = Fake(RwLock::new(vec![
            MkKey::Control,
            MkKey::Character("K".into()),
        ]));
        let count = Mutex::new(0);
        let callback = || *count.lock().unwrap() += 1;
        let mut doc = MkMacroDocument::default();
        doc.settings.record_toggle_hotkey = MkHotkey {
            key: MkKey::Character("K".into()),
            modifiers: vec![MkKey::Control],
        };
        store.save(doc).unwrap();
        tick(&store, &state, &fake, &callback);
        assert_eq!(*count.lock().unwrap(), 0);
        fake.0.write().unwrap().clear();
        tick(&store, &state, &fake, &callback);
        fake.0
            .write()
            .unwrap()
            .extend([MkKey::Control, MkKey::Character("K".into())]);
        tick(&store, &state, &fake, &callback);
        assert_eq!(*count.lock().unwrap(), 1);
    }
}
