//! Polling macro-hotkey service and authoring diagnostics.
use super::{MkHotkey, MkKey, MkMacroDocument, MkMacroStore};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

/// The deliberately small boundary between hotkey polling and the operating system.
pub trait KeyStateBackend: Send + Sync {
    fn is_down(&self, key: &MkKey) -> bool;
}

#[derive(Default)]
pub(crate) struct SystemKeyStateBackend;

#[cfg(windows)]
impl KeyStateBackend for SystemKeyStateBackend {
    fn is_down(&self, key: &MkKey) -> bool {
        use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
        fn down(vk: i32) -> bool {
            unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 }
        }
        let one = |a, b| down(a) || down(b);
        match key {
            MkKey::Control => one(0xA2, 0xA3),
            MkKey::Shift => one(0xA0, 0xA1),
            MkKey::Alt => one(0xA4, 0xA5),
            MkKey::Meta => one(0x5B, 0x5C),
            MkKey::LeftControl => down(0xA2),
            MkKey::RightControl => down(0xA3),
            MkKey::LeftShift => down(0xA0),
            MkKey::RightShift => down(0xA1),
            MkKey::LeftAlt => down(0xA4),
            MkKey::RightAlt => down(0xA5),
            MkKey::LeftMeta => down(0x5B),
            MkKey::RightMeta => down(0x5C),
            _ => vk_from_primary(key).is_some_and(down),
        }
    }
}

#[cfg(not(windows))]
impl KeyStateBackend for SystemKeyStateBackend {
    fn is_down(&self, _key: &MkKey) -> bool {
        false
    }
}

#[cfg(windows)]
fn vk_from_primary(key: &MkKey) -> Option<i32> {
    Some(match key {
        MkKey::Character(s) if s.len() == 1 && s.is_ascii() => {
            let c = s.as_bytes()[0].to_ascii_uppercase();
            if !c.is_ascii_alphanumeric() {
                return None;
            }
            c as i32
        }
        MkKey::Enter => 0x0D,
        MkKey::Tab => 0x09,
        MkKey::Escape => 0x1B,
        MkKey::Space => 0x20,
        MkKey::Backspace => 0x08,
        MkKey::Delete => 0x2E,
        MkKey::Up => 0x26,
        MkKey::Down => 0x28,
        MkKey::Left => 0x25,
        MkKey::Right => 0x27,
        MkKey::Home => 0x24,
        MkKey::End => 0x23,
        MkKey::PageUp => 0x21,
        MkKey::PageDown => 0x22,
        MkKey::Function(n @ 1..=12) => 0x6F + *n as i32,
        _ => return None,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyDiagnostic {
    pub macro_id: u64,
    pub message: String,
}

fn modifier(key: &MkKey) -> Option<MkKey> {
    match key {
        MkKey::Control | MkKey::LeftControl | MkKey::RightControl => Some(MkKey::Control),
        MkKey::Alt | MkKey::LeftAlt | MkKey::RightAlt => Some(MkKey::Alt),
        MkKey::Shift | MkKey::LeftShift | MkKey::RightShift => Some(MkKey::Shift),
        MkKey::Meta | MkKey::LeftMeta | MkKey::RightMeta => Some(MkKey::Meta),
        _ => None,
    }
}
fn key_name(k: &MkKey) -> String {
    match modifier(k) {
        Some(MkKey::Control) => "CONTROL".into(),
        Some(MkKey::Alt) => "ALT".into(),
        Some(MkKey::Shift) => "SHIFT".into(),
        Some(MkKey::Meta) => "META".into(),
        _ => match k {
            MkKey::Character(s) => s.to_ascii_uppercase(),
            x => format!("{x:?}").to_ascii_uppercase(),
        },
    }
}
pub fn canonical_hotkey(h: &MkHotkey) -> String {
    let mut mods = h.modifiers.iter().map(key_name).collect::<Vec<_>>();
    mods.sort();
    mods.dedup();
    mods.push(key_name(&h.key));
    mods.join("+")
}

pub(crate) fn compile_hotkey(h: &MkHotkey) -> Option<(BTreeSet<String>, MkKey)> {
    // A modifier in the primary slot, or any non-modifier in the modifier list, is malformed.
    if modifier(&h.key).is_some() || !usable_primary(&h.key) {
        return None;
    }
    let mut mods = BTreeSet::new();
    for key in &h.modifiers {
        mods.insert(key_name(&modifier(key)?));
    }
    Some((mods, normalize_primary(&h.key)?))
}
fn normalize_primary(key: &MkKey) -> Option<MkKey> {
    match key {
        MkKey::Character(s)
            if s.chars().count() == 1
                && s.is_ascii()
                && s.as_bytes()[0].is_ascii_alphanumeric() =>
        {
            Some(MkKey::Character(s.to_ascii_uppercase()))
        }
        _ if usable_primary(key) => Some(key.clone()),
        _ => None,
    }
}
fn usable_primary(key: &MkKey) -> bool {
    match key {
        MkKey::Character(s) => {
            s.chars().count() == 1 && s.is_ascii() && s.as_bytes()[0].is_ascii_alphanumeric()
        }
        MkKey::Function(n) => (1..=12).contains(n),
        _ => modifier(key).is_none(),
    }
}

pub fn validate_hotkeys(doc: &MkMacroDocument, reserved: &[(&str, &str)]) -> Vec<HotkeyDiagnostic> {
    let mut frequencies = HashMap::new();
    for h in doc
        .macros
        .iter()
        .filter(|m| m.enabled)
        .filter_map(|m| m.hotkey.as_ref())
    {
        *frequencies.entry(canonical_hotkey(h)).or_insert(0usize) += 1;
    }
    let reserved = reserved
        .iter()
        .map(|(n, h)| (h.to_ascii_uppercase().replace(' ', ""), *n))
        .collect::<HashMap<_, _>>();
    let mut out = Vec::new();
    for m in doc.macros.iter().filter(|m| m.enabled) {
        let Some(h) = &m.hotkey else { continue };
        let c = canonical_hotkey(h);
        if frequencies[&c] > 1 {
            out.push(HotkeyDiagnostic {
                macro_id: m.id,
                message: "hotkey is duplicated by another enabled macro".into(),
            });
        }
        if let Some(name) = reserved.get(&c) {
            out.push(HotkeyDiagnostic {
                macro_id: m.id,
                message: format!("hotkey conflicts with {name}"),
            });
        }
    }
    out
}

/// Legacy stable ID mapping retained for callers; duplicate bindings are omitted.
pub fn rebuild_hotkey_map(doc: &MkMacroDocument, first_registration_id: i32) -> BTreeMap<i32, u64> {
    let bindings = compile_bindings(doc);
    bindings
        .into_iter()
        .enumerate()
        .map(|(i, b)| (first_registration_id + i as i32, b.macro_id))
        .collect()
}

struct Binding {
    macro_id: u64,
    modifiers: BTreeSet<String>,
    primary: MkKey,
    triggered: bool,
}
fn compile_bindings(doc: &MkMacroDocument) -> Vec<Binding> {
    let recorder = canonical_hotkey(&doc.settings.record_toggle_hotkey);
    let mut frequency = HashMap::new();
    for m in doc.macros.iter().filter(|m| m.enabled) {
        if let Some(h) = &m.hotkey {
            *frequency.entry(canonical_hotkey(h)).or_insert(0usize) += 1;
        }
    }
    let mut out = doc
        .macros
        .iter()
        .filter(|m| m.enabled)
        .filter_map(|m| {
            let h = m.hotkey.as_ref()?;
            if canonical_hotkey(h) == recorder {
                return None;
            }
            if frequency[&canonical_hotkey(h)] != 1 {
                return None;
            }
            let (modifiers, primary) = compile_hotkey(h)?;
            Some(Binding {
                macro_id: m.id,
                modifiers,
                primary,
                triggered: false,
            })
        })
        .collect::<Vec<_>>();
    out.sort_by_key(|b| b.macro_id);
    out
}

/// Windows virtual-key representation used for recorder-control suppression.
pub(crate) fn primary_virtual_key(key: &MkKey) -> Option<u32> {
    Some(match key {
        MkKey::Character(s) if s.len() == 1 && s.as_bytes()[0].is_ascii_alphanumeric() => {
            s.as_bytes()[0].to_ascii_uppercase() as u32
        }
        MkKey::Enter => 0x0D,
        MkKey::Tab => 0x09,
        MkKey::Escape => 0x1B,
        MkKey::Space => 0x20,
        MkKey::Backspace => 0x08,
        MkKey::Delete => 0x2E,
        MkKey::Up => 0x26,
        MkKey::Down => 0x28,
        MkKey::Left => 0x25,
        MkKey::Right => 0x27,
        MkKey::Home => 0x24,
        MkKey::End => 0x23,
        MkKey::PageUp => 0x21,
        MkKey::PageDown => 0x22,
        MkKey::Function(n @ 1..=12) => 0x6F + *n as u32,
        _ => return None,
    })
}

struct PollState {
    snapshot: Arc<MkMacroDocument>,
    bindings: Vec<Binding>,
}
type Trigger = dyn Fn(u64) + Send + Sync;

pub struct MkMacroHotkeyService {
    store: Arc<MkMacroStore>,
    stop: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
    state: Arc<Mutex<PollState>>,
    backend: Arc<dyn KeyStateBackend>,
    trigger: Arc<Trigger>,
}
impl MkMacroHotkeyService {
    pub fn new(store: Arc<MkMacroStore>) -> Self {
        Self::with_backend(store, Arc::new(SystemKeyStateBackend))
    }
    pub fn with_backend(store: Arc<MkMacroStore>, backend: Arc<dyn KeyStateBackend>) -> Self {
        Self::start(
            store,
            backend,
            Arc::new(|id| {
                let _ = crate::mkmacro::runtime::run(id);
            }),
        )
    }
    fn start(
        store: Arc<MkMacroStore>,
        backend: Arc<dyn KeyStateBackend>,
        trigger: Arc<Trigger>,
    ) -> Self {
        let snapshot = store.snapshot();
        let state = Arc::new(Mutex::new(PollState {
            bindings: compile_bindings(&snapshot),
            snapshot,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let (worker_store, worker_stop, worker_state, worker_backend, worker_trigger) = (
            store.clone(),
            stop.clone(),
            state.clone(),
            backend.clone(),
            trigger.clone(),
        );
        let worker = thread::Builder::new()
            .name("mkmacro-hotkeys".into())
            .spawn(move || {
                while !worker_stop.load(Ordering::SeqCst) {
                    tick(
                        &worker_store,
                        &worker_state,
                        worker_backend.as_ref(),
                        worker_trigger.as_ref(),
                    );
                    thread::sleep(Duration::from_millis(20));
                }
            })
            .expect("spawn macro hotkey service");
        Self {
            store,
            stop,
            worker: Mutex::new(Some(worker)),
            state,
            backend,
            trigger,
        }
    }
    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.worker.lock().unwrap().take() {
            let _ = h.join();
        }
    }
}
impl Drop for MkMacroHotkeyService {
    fn drop(&mut self) {
        self.shutdown()
    }
}

fn tick<F>(
    store: &MkMacroStore,
    state: &Mutex<PollState>,
    backend: &dyn KeyStateBackend,
    trigger: &F,
) where
    F: Fn(u64) + ?Sized,
{
    let snapshot = store.snapshot();
    let mut fire = Vec::new();
    {
        let mut state = state.lock().unwrap();
        if !Arc::ptr_eq(&snapshot, &state.snapshot) {
            state.bindings = compile_bindings(&snapshot);
            state.snapshot = snapshot;
        }
        let ctrl = backend.is_down(&MkKey::Control);
        let shift = backend.is_down(&MkKey::Shift);
        let alt = backend.is_down(&MkKey::Alt);
        let meta = backend.is_down(&MkKey::Meta);
        let mut keys: Vec<(MkKey, bool)> = Vec::new();
        for b in &state.bindings {
            if !keys.iter().any(|(k, _)| k == &b.primary) {
                keys.push((b.primary.clone(), backend.is_down(&b.primary)));
            }
        }
        for b in &mut state.bindings {
            // Like the Launcher listener, required modifiers are inclusive: extra modifiers are allowed.
            let down = keys.iter().find(|(k, _)| k == &b.primary).unwrap().1
                && (!b.modifiers.contains("CONTROL") || ctrl)
                && (!b.modifiers.contains("SHIFT") || shift)
                && (!b.modifiers.contains("ALT") || alt)
                && (!b.modifiers.contains("META") || meta);
            if down && !b.triggered {
                b.triggered = true;
                fire.push(b.macro_id);
            } else if !down {
                b.triggered = false;
            }
        }
    }
    // Never hold the service/store state lock across runtime admission.
    for id in fire {
        trigger(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{MkMacro, MkPlayback};
    use std::sync::RwLock;
    fn mac(id: u64, on: bool) -> MkMacro {
        MkMacro {
            id,
            name: id.to_string(),
            description: String::new(),
            enabled: on,
            hotkey: Some(MkHotkey {
                key: MkKey::Character("K".into()),
                modifiers: vec![MkKey::Control],
            }),
            playback: MkPlayback::default(),
            steps: vec![],
        }
    }
    #[test]
    fn every_duplicate_is_diagnosed_and_unarmed() {
        let d = MkMacroDocument {
            settings: Default::default(),
            schema_version: 1,
            macros: vec![mac(9, true), mac(2, true), mac(1, true)],
        };
        assert_eq!(validate_hotkeys(&d, &[]).len(), 3);
        assert!(compile_bindings(&d).is_empty());
    }
    #[test]
    fn disabled_duplicate_does_not_conflict() {
        let d = MkMacroDocument {
            settings: Default::default(),
            schema_version: 1,
            macros: vec![mac(2, true), mac(1, false)],
        };
        assert_eq!(compile_bindings(&d).len(), 1);
    }
    #[test]
    fn aliases_and_malformed_are_handled() {
        let mut a = mac(1, true);
        a.hotkey.as_mut().unwrap().modifiers = vec![MkKey::RightControl];
        let mut b = mac(2, true);
        b.hotkey.as_mut().unwrap().modifiers = vec![MkKey::LeftControl];
        let d = MkMacroDocument {
            settings: Default::default(),
            schema_version: 1,
            macros: vec![a, b],
        };
        assert!(compile_bindings(&d).is_empty());
        let mut bad = mac(3, true);
        bad.hotkey
            .as_mut()
            .unwrap()
            .modifiers
            .push(MkKey::Character("X".into()));
        assert!(
            compile_bindings(&MkMacroDocument {
                settings: Default::default(),
                schema_version: 1,
                macros: vec![bad]
            })
            .is_empty()
        );
    }
    #[test]
    fn deterministic_rebuild() {
        let d = MkMacroDocument {
            settings: Default::default(),
            schema_version: 1,
            macros: vec![mac(9, true), {
                let mut m = mac(2, true);
                m.hotkey.as_mut().unwrap().key = MkKey::Character("J".into());
                m
            }],
        };
        assert_eq!(
            rebuild_hotkey_map(&d, 100)
                .into_values()
                .collect::<Vec<_>>(),
            vec![2, 9]
        );
    }
    #[test]
    fn reserved_conflict() {
        assert_eq!(
            validate_hotkeys(
                &MkMacroDocument {
                    settings: Default::default(),
                    schema_version: 1,
                    macros: vec![mac(1, true)]
                },
                &[("emergency stop", "CONTROL+K")]
            )
            .len(),
            1
        );
    }
    #[test]
    fn primary_virtual_keys_accept_only_supported_ascii_characters() {
        assert_eq!(
            primary_virtual_key(&MkKey::Character("a".into())),
            Some(0x41)
        );
        assert_eq!(
            primary_virtual_key(&MkKey::Character("7".into())),
            Some(0x37)
        );
        assert_eq!(primary_virtual_key(&MkKey::Character("é".into())), None);
        assert_eq!(primary_virtual_key(&MkKey::Character("AB".into())), None);
    }
    struct Fake(RwLock<Vec<MkKey>>);
    impl KeyStateBackend for Fake {
        fn is_down(&self, k: &MkKey) -> bool {
            self.0.read().unwrap().contains(k)
        }
    }
    #[test]
    fn tick_has_edges_and_rebuilds() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        store
            .save(MkMacroDocument {
                settings: Default::default(),
                schema_version: 1,
                macros: vec![mac(1, true)],
            })
            .unwrap();
        let store = Arc::new(store);
        let snap = store.snapshot();
        let state = Mutex::new(PollState {
            bindings: compile_bindings(&snap),
            snapshot: snap,
        });
        let fake = Fake(RwLock::new(vec![
            MkKey::Control,
            MkKey::Character("K".into()),
        ]));
        let fired = Mutex::new(vec![]);
        let cb = |id| fired.lock().unwrap().push(id);
        tick(&store, &state, &fake, &cb);
        tick(&store, &state, &fake, &cb);
        assert_eq!(*fired.lock().unwrap(), vec![1]);
        fake.0.write().unwrap().clear();
        tick(&store, &state, &fake, &cb);
        fake.0
            .write()
            .unwrap()
            .extend([MkKey::Control, MkKey::Character("K".into())]);
        tick(&store, &state, &fake, &cb);
        assert_eq!(*fired.lock().unwrap(), vec![1, 1]);
        store
            .save(MkMacroDocument {
                settings: Default::default(),
                schema_version: 1,
                macros: vec![],
            })
            .unwrap();
        tick(&store, &state, &fake, &cb);
        assert_eq!(*fired.lock().unwrap(), vec![1, 1]);
    }
}
