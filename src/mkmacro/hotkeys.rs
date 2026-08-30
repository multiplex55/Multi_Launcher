//! Polling macro-hotkey service and authoring diagnostics.
use super::{MkHotkey, MkHotkeyScope, MkKey, MkMacroDocument, MkMacroStore, MkWindowMatcher};
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
    fn active_window_matches(&self, _matcher: &MkWindowMatcher) -> bool {
        false
    }
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
    fn active_window_matches(&self, matcher: &MkWindowMatcher) -> bool {
        use super::executor::WindowBackend;
        use super::windows::Win32WindowBackend;
        Win32WindowBackend.is_active(matcher).unwrap_or(false)
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
    let recorder = canonical_hotkey(&doc.settings.record_toggle_hotkey);
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
        if c == recorder {
            out.push(HotkeyDiagnostic {
                macro_id: m.id,
                message: "hotkey conflicts with the recording toggle".into(),
            });
        }
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct HotkeyCandidate {
    macro_id: u64,
    display_name: String,
    scope: MkHotkeyScope,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HotkeyGroup {
    canonical_chord: String,
    modifiers: BTreeSet<String>,
    primary: MkKey,
    candidates: Vec<HotkeyCandidate>,
    triggered: bool,
}

fn normalized_hotkey(modifiers: &BTreeSet<String>, primary: &MkKey) -> MkHotkey {
    let modifiers = modifiers
        .iter()
        .map(|modifier| match modifier.as_str() {
            "CONTROL" => MkKey::Control,
            "SHIFT" => MkKey::Shift,
            "ALT" => MkKey::Alt,
            "META" => MkKey::Meta,
            _ => unreachable!("compile_hotkey produced an unknown modifier"),
        })
        .collect();
    MkHotkey {
        key: primary.clone(),
        modifiers,
    }
}

fn valid_hotkey_scope(scope: &MkHotkeyScope) -> bool {
    match scope {
        MkHotkeyScope::AnyWindow => true,
        MkHotkeyScope::ActiveWindow(matcher) => {
            let usable = |value: &Option<String>| {
                value.as_ref().is_some_and(|value| !value.trim().is_empty())
            };
            (usable(&matcher.title)
                || usable(&matcher.title_regex)
                || usable(&matcher.process)
                || usable(&matcher.class))
                && matcher
                    .title_regex
                    .as_ref()
                    .is_none_or(|regex| regex::Regex::new(regex).is_ok())
        }
    }
}

/// Compiles one polling group for each usable physical chord.
///
/// Duplicate chords are intentionally retained as contextual candidates. The
/// recorder toggle is a hard conflict because it has a separate owner and
/// must never be dispatched as a macro hotkey.
fn compile_hotkey_groups(doc: &MkMacroDocument) -> Vec<HotkeyGroup> {
    let recorder = compile_hotkey(&doc.settings.record_toggle_hotkey)
        .map(|(modifiers, primary)| canonical_hotkey(&normalized_hotkey(&modifiers, &primary)));
    let mut groups = BTreeMap::<String, HotkeyGroup>::new();

    for m in doc.macros.iter().filter(|m| m.enabled) {
        let Some(hotkey) = m.hotkey.as_ref() else {
            continue;
        };
        let Some((modifiers, primary)) = compile_hotkey(hotkey) else {
            continue;
        };
        let canonical_chord = canonical_hotkey(&normalized_hotkey(&modifiers, &primary));
        if recorder.as_deref() == Some(canonical_chord.as_str())
            || !valid_hotkey_scope(&m.hotkey_scope)
        {
            continue;
        }

        let group = groups
            .entry(canonical_chord.clone())
            .or_insert_with(|| HotkeyGroup {
                canonical_chord,
                modifiers,
                primary,
                candidates: Vec::new(),
                triggered: false,
            });
        group.candidates.push(HotkeyCandidate {
            macro_id: m.id,
            display_name: m.name.clone(),
            scope: m.hotkey_scope.clone(),
        });
    }

    let mut groups = groups.into_values().collect::<Vec<_>>();
    for group in &mut groups {
        group.candidates.sort_by_key(|candidate| candidate.macro_id);
    }
    groups
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
    groups: Vec<HotkeyGroup>,
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
            groups: compile_hotkey_groups(&snapshot),
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
            state.groups = compile_hotkey_groups(&snapshot);
            state.snapshot = snapshot;
        }
        let ctrl = backend.is_down(&MkKey::Control);
        let shift = backend.is_down(&MkKey::Shift);
        let alt = backend.is_down(&MkKey::Alt);
        let meta = backend.is_down(&MkKey::Meta);
        let mut primary_states: Vec<(MkKey, bool)> = Vec::new();
        for group in &state.groups {
            if !primary_states.iter().any(|(key, _)| key == &group.primary) {
                primary_states.push((group.primary.clone(), backend.is_down(&group.primary)));
            }
        }
        for group in &mut state.groups {
            // Like the Launcher listener, required modifiers are inclusive: extra modifiers are allowed.
            let primary_down = primary_states
                .iter()
                .find(|(key, _)| key == &group.primary)
                .map(|(_, down)| *down)
                .unwrap_or(false);
            let down = primary_down
                && (!group.modifiers.contains("CONTROL") || ctrl)
                && (!group.modifiers.contains("SHIFT") || shift)
                && (!group.modifiers.contains("ALT") || alt)
                && (!group.modifiers.contains("META") || meta);
            if !down {
                group.triggered = false;
                continue;
            }
            if group.triggered {
                continue;
            }
            group.triggered = true;
            match resolve_candidate(group, backend) {
                CandidateResolution::None => {}
                CandidateResolution::Unique(candidate) => fire.push(candidate.macro_id),
                CandidateResolution::Ambiguous(candidates) => {
                    let candidates = candidates
                        .iter()
                        .map(|candidate| {
                            format!("{} ({})", candidate.display_name, candidate.macro_id)
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    tracing::warn!(
                        chord = %group.canonical_chord,
                        candidates = %candidates,
                        "macro hotkey is ambiguous; no macro triggered"
                    );
                }
            }
        }
    }
    // Never hold the service/store state lock across runtime admission.
    for id in fire {
        trigger(id);
    }
}

enum CandidateResolution<'a> {
    None,
    Unique(&'a HotkeyCandidate),
    Ambiguous(Vec<&'a HotkeyCandidate>),
}

fn resolve_candidate<'a>(
    group: &'a HotkeyGroup,
    backend: &dyn KeyStateBackend,
) -> CandidateResolution<'a> {
    let matches = group
        .candidates
        .iter()
        .filter(|candidate| match &candidate.scope {
            MkHotkeyScope::AnyWindow => true,
            MkHotkeyScope::ActiveWindow(matcher) => backend.active_window_matches(matcher),
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => CandidateResolution::None,
        [candidate] => CandidateResolution::Unique(candidate),
        _ => CandidateResolution::Ambiguous(matches),
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
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: MkPlayback::default(),
            steps: vec![],
            image_assets: vec![],
        }
    }
    #[test]
    fn three_shared_macros_compile_into_one_group_with_one_edge_flag() {
        let d = MkMacroDocument {
            settings: Default::default(),
            schema_version: 1,
            folders: vec![],
            macros: vec![mac(9, true), mac(2, true), mac(1, true)],
        };
        assert_eq!(validate_hotkeys(&d, &[]).len(), 3);
        let groups = compile_hotkey_groups(&d);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].canonical_chord, "CONTROL+K");
        assert_eq!(groups[0].primary, MkKey::Character("K".into()));
        assert_eq!(
            groups[0]
                .candidates
                .iter()
                .map(|candidate| candidate.macro_id)
                .collect::<Vec<_>>(),
            vec![1, 2, 9]
        );
        assert!(!groups[0].triggered);
    }
    #[test]
    fn different_chords_form_different_groups() {
        let d = MkMacroDocument {
            settings: Default::default(),
            schema_version: 1,
            folders: vec![],
            macros: vec![mac(2, true), {
                let mut m = mac(1, true);
                m.hotkey.as_mut().unwrap().key = MkKey::Character("J".into());
                m
            }],
        };
        let groups = compile_hotkey_groups(&d);
        assert_eq!(groups.len(), 2);
        assert_eq!(
            groups
                .iter()
                .map(|group| group.canonical_chord.as_str())
                .collect::<Vec<_>>(),
            vec!["CONTROL+J", "CONTROL+K"]
        );
    }
    #[test]
    fn left_and_right_modifier_aliases_share_a_group() {
        let mut a = mac(1, true);
        a.hotkey.as_mut().unwrap().modifiers = vec![MkKey::RightControl];
        let mut b = mac(2, true);
        b.hotkey.as_mut().unwrap().modifiers = vec![MkKey::LeftControl];
        let d = MkMacroDocument {
            settings: Default::default(),
            schema_version: 1,
            folders: vec![],
            macros: vec![a, b],
        };
        let groups = compile_hotkey_groups(&d);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].canonical_chord, "CONTROL+K");
        assert_eq!(groups[0].candidates.len(), 2);
    }
    #[test]
    fn disabled_missing_and_malformed_hotkeys_are_not_candidates() {
        let mut bad = mac(3, true);
        bad.hotkey
            .as_mut()
            .unwrap()
            .modifiers
            .push(MkKey::Character("X".into()));
        let mut missing = mac(4, true);
        missing.hotkey = None;
        let d = MkMacroDocument {
            settings: Default::default(),
            schema_version: 1,
            folders: vec![],
            macros: vec![mac(2, true), mac(1, false), bad, missing],
        };
        let groups = compile_hotkey_groups(&d);
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0]
                .candidates
                .iter()
                .map(|candidate| candidate.macro_id)
                .collect::<Vec<_>>(),
            vec![2]
        );
    }

    #[test]
    fn invalid_contextual_scopes_are_not_candidates() {
        let mut empty = mac(2, true);
        empty.hotkey_scope = MkHotkeyScope::ActiveWindow(MkWindowMatcher::default());
        let mut malformed_regex = mac(3, true);
        malformed_regex.hotkey_scope = MkHotkeyScope::ActiveWindow(MkWindowMatcher {
            title_regex: Some("[".into()),
            ..Default::default()
        });
        let mut valid = mac(4, true);
        valid.hotkey_scope = MkHotkeyScope::ActiveWindow(MkWindowMatcher {
            title: Some("Editor".into()),
            ..Default::default()
        });
        let d = MkMacroDocument {
            settings: Default::default(),
            schema_version: 1,
            folders: vec![],
            macros: vec![empty, malformed_regex, valid],
        };
        let groups = compile_hotkey_groups(&d);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].candidates[0].macro_id, 4);
    }

    #[test]
    fn reserved_conflict() {
        assert_eq!(
            validate_hotkeys(
                &MkMacroDocument {
                    settings: Default::default(),
                    schema_version: 1,
                    folders: vec![],
                    macros: vec![mac(1, true)]
                },
                &[("emergency stop", "CONTROL+K")]
            )
            .len(),
            1
        );
    }
    #[test]
    fn recorder_toggle_conflict_is_diagnosed_and_not_registrable() {
        let mut m = mac(7, true);
        m.hotkey = Some(MkHotkey {
            key: MkKey::Function(9),
            modifiers: vec![],
        });
        let d = MkMacroDocument {
            settings: Default::default(),
            schema_version: 1,
            folders: vec![],
            macros: vec![m],
        };
        assert_eq!(
            validate_hotkeys(&d, &[]),
            vec![HotkeyDiagnostic {
                macro_id: 7,
                message: "hotkey conflicts with the recording toggle".into(),
            }]
        );
        assert!(compile_hotkey_groups(&d).is_empty());
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
    struct ContextFake {
        active_title: String,
    }
    impl KeyStateBackend for ContextFake {
        fn is_down(&self, _: &MkKey) -> bool {
            false
        }
        fn active_window_matches(&self, matcher: &MkWindowMatcher) -> bool {
            matcher.title.as_deref() == Some(self.active_title.as_str())
        }
    }
    #[test]
    fn candidate_order_does_not_choose_between_contextual_candidates() {
        let mut editor = mac(9, true);
        editor.hotkey_scope = MkHotkeyScope::ActiveWindow(MkWindowMatcher {
            title: Some("Editor".into()),
            ..Default::default()
        });
        let mut terminal = mac(2, true);
        terminal.hotkey_scope = MkHotkeyScope::ActiveWindow(MkWindowMatcher {
            title: Some("Terminal".into()),
            ..Default::default()
        });
        let d = MkMacroDocument {
            settings: Default::default(),
            schema_version: 1,
            folders: vec![],
            macros: vec![editor, terminal],
        };
        let groups = compile_hotkey_groups(&d);
        assert_eq!(groups.len(), 1);
        assert!(matches!(
            resolve_candidate(
                &groups[0],
                &ContextFake {
                    active_title: "Editor".into()
                }
            ),
            CandidateResolution::Unique(candidate) if candidate.macro_id == 9
        ));

        let mut reversed = groups[0].clone();
        reversed.candidates.reverse();
        assert!(matches!(
            resolve_candidate(
                &reversed,
                &ContextFake {
                    active_title: "Editor".into()
                }
            ),
            CandidateResolution::Unique(candidate) if candidate.macro_id == 9
        ));
    }
    #[test]
    fn tick_has_edges_and_rebuilds() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        store
            .save(MkMacroDocument {
                settings: Default::default(),
                schema_version: 1,
                folders: vec![],
                macros: vec![mac(1, true)],
            })
            .unwrap();
        let store = Arc::new(store);
        let snap = store.snapshot();
        let state = Mutex::new(PollState {
            groups: compile_hotkey_groups(&snap),
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
                folders: vec![],
                macros: vec![],
            })
            .unwrap();
        tick(&store, &state, &fake, &cb);
        assert_eq!(*fired.lock().unwrap(), vec![1, 1]);
    }
}
