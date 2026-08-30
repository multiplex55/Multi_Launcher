//! Polling macro-hotkey service and authoring diagnostics.
use super::{
    ExecutionDiagnostic, MkHotkey, MkHotkeyScope, MkKey, MkMacroDocument, MkMacroStore,
    MkWindowMatcher, WindowCandidate, candidate_matches,
};
use std::{
    collections::{BTreeMap, BTreeSet},
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

/// Error returned when the foreground-window snapshot cannot be obtained.
pub type ActiveWindowError = ExecutionDiagnostic;

/// The deliberately small boundary between hotkey polling and foreground-window discovery.
pub trait ActiveWindowBackend: Send + Sync {
    fn active_window(&self) -> Result<Option<WindowCandidate>, ActiveWindowError>;
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

#[derive(Default)]
pub(crate) struct SystemActiveWindowBackend;

impl ActiveWindowBackend for SystemActiveWindowBackend {
    fn active_window(&self) -> Result<Option<WindowCandidate>, ActiveWindowError> {
        Ok(crate::multi_manager::win::active_window().map(WindowCandidate::from))
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

/// Whether a hotkey configuration is invalid or merely risks contextual overlap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyDiagnosticSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyDiagnostic {
    pub severity: HotkeyDiagnosticSeverity,
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

fn compiled_canonical_hotkey(h: &MkHotkey) -> Option<String> {
    let (modifiers, primary) = compile_hotkey(h)?;
    Some(canonical_hotkey(&normalized_hotkey(&modifiers, &primary)))
}

fn reserved_modifier(token: &str) -> Option<MkKey> {
    match token {
        "CTRL" | "CONTROL" | "LCTRL" | "LEFTCTRL" | "LEFTCONTROL" | "RCTRL" | "RIGHTCTRL"
        | "RIGHTCONTROL" => Some(MkKey::Control),
        "ALT" | "LALT" | "LEFTALT" | "RALT" | "RIGHTALT" => Some(MkKey::Alt),
        "SHIFT" | "LSHIFT" | "LEFTSHIFT" | "RSHIFT" | "RIGHTSHIFT" => Some(MkKey::Shift),
        "WIN" | "WINDOWS" | "META" | "SUPER" | "CMD" | "COMMAND" | "LWIN" | "LEFTWIN"
        | "LEFTWINDOWS" | "LMETA" | "LEFTMETA" | "RWIN" | "RIGHTWIN" | "RIGHTWINDOWS" | "RMETA"
        | "RIGHTMETA" => Some(MkKey::Meta),
        _ => None,
    }
}

fn reserved_primary(token: &str) -> Option<MkKey> {
    let primary = match token {
        "ENTER" | "RETURN" => MkKey::Enter,
        "TAB" => MkKey::Tab,
        "ESC" | "ESCAPE" => MkKey::Escape,
        "SPACE" => MkKey::Space,
        "BACKSPACE" => MkKey::Backspace,
        "DELETE" | "DEL" => MkKey::Delete,
        "UP" | "UPARROW" => MkKey::Up,
        "DOWN" | "DOWNARROW" => MkKey::Down,
        "LEFT" | "LEFTARROW" => MkKey::Left,
        "RIGHT" | "RIGHTARROW" => MkKey::Right,
        "HOME" => MkKey::Home,
        "END" => MkKey::End,
        "PAGEUP" => MkKey::PageUp,
        "PAGEDOWN" => MkKey::PageDown,
        _ if token.starts_with('F') => MkKey::Function(token[1..].parse::<u8>().ok()?),
        _ if token.len() == 1 && token.as_bytes()[0].is_ascii_alphanumeric() => {
            MkKey::Character(token.to_string())
        }
        _ => return None,
    };
    Some(primary)
}

/// Canonicalizes a caller-provided chord through the same `MkHotkey` compiler
/// used by macro hotkeys. Whitespace, case, modifier aliases, and modifier
/// ordering therefore cannot change conflict identity.
fn canonical_reserved_chord(chord: &str) -> Option<String> {
    let mut modifiers = Vec::new();
    let mut primary = None;
    for token in chord.split('+') {
        let token = token.trim().to_ascii_uppercase();
        if token.is_empty() {
            continue;
        }
        if let Some(modifier) = reserved_modifier(&token) {
            modifiers.push(modifier);
        } else if primary.replace(reserved_primary(&token)?).is_some() {
            return None;
        }
    }
    let key = primary?;
    compiled_canonical_hotkey(&MkHotkey { key, modifiers })
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct NormalizedReservedChords {
    by_chord: BTreeMap<String, String>,
}

fn normalize_reserved_chords(
    doc: &MkMacroDocument,
    reserved: impl IntoIterator<Item = (String, String)>,
) -> NormalizedReservedChords {
    let mut normalized = NormalizedReservedChords::default();

    if let Some(chord) = compiled_canonical_hotkey(&doc.settings.record_toggle_hotkey) {
        normalized
            .by_chord
            .insert(chord, "the recording toggle".into());
    }

    for (name, chord) in reserved {
        let Some(chord) = canonical_reserved_chord(&chord) else {
            continue;
        };
        normalized.by_chord.entry(chord).or_insert(name);
    }
    normalized
}

pub fn validate_hotkeys(doc: &MkMacroDocument, reserved: &[(&str, &str)]) -> Vec<HotkeyDiagnostic> {
    let reserved = normalize_reserved_chords(
        doc,
        reserved
            .iter()
            .map(|(name, chord)| ((*name).to_string(), (*chord).to_string())),
    );
    let mut out = Vec::new();
    let mut groups = BTreeMap::<_, Vec<_>>::new();
    for m in doc.macros.iter().filter(|m| m.enabled) {
        let Some(h) = &m.hotkey else { continue };
        if !valid_hotkey_scope(&m.hotkey_scope) {
            out.push(HotkeyDiagnostic {
                severity: HotkeyDiagnosticSeverity::Error,
                macro_id: m.id,
                message: "Invalid active-window hotkey matcher: provide at least one nonempty constraint and a valid title regex. This hotkey will not run.".into(),
            });
        }
        let Some(c) = compiled_canonical_hotkey(h) else {
            out.push(HotkeyDiagnostic {
                severity: HotkeyDiagnosticSeverity::Error,
                macro_id: m.id,
                message: "Malformed hotkey: use a supported primary key and only modifiers in the modifier list.".into(),
            });
            continue;
        };
        if let Some(name) = reserved.by_chord.get(&c) {
            out.push(HotkeyDiagnostic {
                severity: HotkeyDiagnosticSeverity::Error,
                macro_id: m.id,
                message: format!("hotkey conflicts with {name}"),
            });
        }
        // Reserved chords still participate in duplicate analysis: these are
        // independent problems.
        groups.entry(c).or_default().push(m);
    }
    for (chord, candidates) in groups {
        let unrestricted = candidates
            .iter()
            .filter(|m| matches!(m.hotkey_scope, MkHotkeyScope::AnyWindow))
            .collect::<Vec<_>>();
        if unrestricted.len() > 1 {
            for m in unrestricted {
                out.push(HotkeyDiagnostic {
                    severity: HotkeyDiagnosticSeverity::Error,
                    macro_id: m.id,
                    message: format!(
                        "Multiple unrestricted macros use {}. Only one global fallback is allowed.",
                        diagnostic_chord(&chord)
                    ),
                });
            }
        }
        let contextual = candidates
            .iter()
            .filter(|m| valid_hotkey_scope(&m.hotkey_scope))
            .filter_map(|m| match &m.hotkey_scope {
                MkHotkeyScope::ActiveWindow(matcher) => Some((m.id, normalized_matcher(matcher))),
                MkHotkeyScope::AnyWindow => None,
            })
            .collect::<Vec<_>>();
        for (index, (macro_id, matcher)) in contextual.iter().enumerate() {
            if contextual
                .iter()
                .enumerate()
                .any(|(other_index, (_, other))| index != other_index && matcher == other)
            {
                out.push(HotkeyDiagnostic {
                    severity: HotkeyDiagnosticSeverity::Error,
                    macro_id: *macro_id,
                    message: "Window-specific macros sharing this hotkey have identical matchers and are always ambiguous when they match. Document order does not resolve the ambiguity; none will run.".into(),
                });
            }
            // Distinct structures can still overlap. Do not infer exclusivity
            // from process, title substring, regex, or class constraints.
            if contextual.iter().any(|(_, other)| matcher != other) {
                out.push(HotkeyDiagnostic {
                    severity: HotkeyDiagnosticSeverity::Warning,
                    macro_id: *macro_id,
                    message: "Multiple window-specific macros share this hotkey. If more than one matcher matches the active window, none will run.".into(),
                });
            }
        }
    }
    out
}

fn normalized_matcher(matcher: &MkWindowMatcher) -> MkWindowMatcher {
    let nonempty = |value: &Option<String>| value.clone().filter(|value| !value.trim().is_empty());
    MkWindowMatcher {
        title: nonempty(&matcher.title),
        title_regex: nonempty(&matcher.title_regex),
        process: nonempty(&matcher.process),
        class: nonempty(&matcher.class),
    }
}
fn diagnostic_chord(canonical_chord: &str) -> String {
    canonical_chord
        .split('+')
        .map(|part| match part {
            "CONTROL" => "Ctrl",
            "SHIFT" => "Shift",
            "ALT" => "Alt",
            "META" => "Meta",
            part => part,
        })
        .collect::<Vec<_>>()
        .join("+")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HotkeyCandidate {
    macro_id: u64,
    display_name: String,
    scope: MkHotkeyScope,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CandidateSummary {
    pub(crate) display_name: String,
    pub(crate) macro_id: u64,
}

impl CandidateSummary {
    fn from_candidate(candidate: &HotkeyCandidate) -> Self {
        Self {
            display_name: candidate.display_name.clone(),
            macro_id: candidate.macro_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HotkeyGroup {
    canonical_chord: String,
    modifiers: BTreeSet<String>,
    primary: MkKey,
    contextual_candidates: Vec<HotkeyCandidate>,
    unrestricted_candidates: Vec<HotkeyCandidate>,
    unarmed_candidates: Vec<HotkeyCandidate>,
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
/// Contextual candidates and unrestricted fallback candidates are compiled
/// into separate tiers. Multiple unrestricted candidates remain attached to
/// the group for diagnostics, but they do not form a fallback tier and are
/// never dispatched.
fn compile_hotkey_groups_with_reserved(
    doc: &MkMacroDocument,
    reserved: &NormalizedReservedChords,
) -> Vec<HotkeyGroup> {
    let mut groups = BTreeMap::<String, HotkeyGroup>::new();

    for m in doc.macros.iter().filter(|m| m.enabled) {
        let Some(hotkey) = m.hotkey.as_ref() else {
            continue;
        };
        let Some((modifiers, primary)) = compile_hotkey(hotkey) else {
            continue;
        };
        let canonical_chord = canonical_hotkey(&normalized_hotkey(&modifiers, &primary));
        if reserved.by_chord.contains_key(&canonical_chord) {
            continue;
        }

        let group = groups
            .entry(canonical_chord.clone())
            .or_insert_with(|| HotkeyGroup {
                canonical_chord,
                modifiers,
                primary,
                contextual_candidates: Vec::new(),
                unrestricted_candidates: Vec::new(),
                unarmed_candidates: Vec::new(),
                triggered: false,
            });
        let candidate = HotkeyCandidate {
            macro_id: m.id,
            display_name: m.name.clone(),
            scope: m.hotkey_scope.clone(),
        };
        if !valid_hotkey_scope(&m.hotkey_scope) {
            group.unarmed_candidates.push(candidate);
        } else if matches!(m.hotkey_scope, MkHotkeyScope::AnyWindow) {
            group.unrestricted_candidates.push(candidate);
        } else {
            group.contextual_candidates.push(candidate);
        }
    }

    let mut groups = groups.into_values().collect::<Vec<_>>();
    for group in &mut groups {
        group
            .contextual_candidates
            .sort_by_key(|candidate| candidate.macro_id);
        group
            .unrestricted_candidates
            .sort_by_key(|candidate| candidate.macro_id);
        group
            .unarmed_candidates
            .sort_by_key(|candidate| candidate.macro_id);
    }
    groups
}

fn compile_hotkey_groups(doc: &MkMacroDocument) -> Vec<HotkeyGroup> {
    let reserved = normalize_reserved_chords(doc, std::iter::empty());
    compile_hotkey_groups_with_reserved(doc, &reserved)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HotkeyResolution {
    Run(u64),
    NoMatch,
    Ambiguous(Vec<CandidateSummary>),
    BackendFailure(ExecutionDiagnostic),
}

/// Resolves one already-compiled chord against one foreground-window snapshot.
///
/// This function deliberately has no backend or logging dependency. Contextual
/// candidates are resolved as a separate tier from unrestricted candidates, so
/// a global fallback can never steal a chord from a matching contextual macro.
fn resolve_hotkey_group(
    group: &HotkeyGroup,
    foreground: Option<&WindowCandidate>,
) -> HotkeyResolution {
    let mut contextual_matches = Vec::new();
    let mut matcher_failure = None;

    for candidate in &group.contextual_candidates {
        let MkHotkeyScope::ActiveWindow(matcher) = &candidate.scope else {
            continue;
        };
        let Some(foreground) = foreground else {
            continue;
        };
        match candidate_matches(matcher, foreground) {
            Ok(true) => contextual_matches.push(candidate),
            Ok(false) => {}
            Err(error) => {
                // Continue evaluating the remaining contextual candidates so
                // every matcher gets the same snapshot, but never fall back
                // after a matcher failure.
                matcher_failure
                    .get_or_insert(error.context("macro_id", candidate.macro_id.to_string()));
            }
        }
    }

    if let Some(error) = matcher_failure {
        return HotkeyResolution::BackendFailure(error);
    }

    if let [candidate] = contextual_matches.as_slice() {
        return HotkeyResolution::Run(candidate.macro_id);
    }
    if !contextual_matches.is_empty() {
        return HotkeyResolution::Ambiguous(candidate_summaries(contextual_matches));
    }

    match group.unrestricted_candidates.as_slice() {
        [candidate] => HotkeyResolution::Run(candidate.macro_id),
        _ => HotkeyResolution::NoMatch,
    }
}

fn candidate_summaries<'a>(
    candidates: impl IntoIterator<Item = &'a HotkeyCandidate>,
) -> Vec<CandidateSummary> {
    let mut summaries = candidates
        .into_iter()
        .map(CandidateSummary::from_candidate)
        .collect::<Vec<_>>();
    summaries.sort_by_key(|candidate| candidate.macro_id);
    summaries
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
    reserved: Vec<(String, String)>,
}
type Trigger = dyn Fn(u64) + Send + Sync;

pub struct MkMacroHotkeyService {
    store: Arc<MkMacroStore>,
    stop: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
    state: Arc<Mutex<PollState>>,
    key_state_backend: Arc<dyn KeyStateBackend>,
    active_window_backend: Arc<dyn ActiveWindowBackend>,
    trigger: Arc<Trigger>,
}
impl MkMacroHotkeyService {
    pub fn new(store: Arc<MkMacroStore>) -> Self {
        Self::new_with_reserved(store, &[])
    }
    pub fn new_with_reserved(store: Arc<MkMacroStore>, reserved: &[(&str, &str)]) -> Self {
        Self::with_backends_and_reserved(
            store,
            Arc::new(SystemKeyStateBackend),
            Arc::new(SystemActiveWindowBackend),
            reserved,
        )
    }
    pub fn with_backend(store: Arc<MkMacroStore>, backend: Arc<dyn KeyStateBackend>) -> Self {
        Self::with_backend_and_reserved(store, backend, &[])
    }
    pub fn with_backend_and_reserved(
        store: Arc<MkMacroStore>,
        backend: Arc<dyn KeyStateBackend>,
        reserved: &[(&str, &str)],
    ) -> Self {
        Self::with_backends_and_reserved(
            store,
            backend,
            Arc::new(SystemActiveWindowBackend),
            reserved,
        )
    }
    pub fn with_backends(
        store: Arc<MkMacroStore>,
        key_state_backend: Arc<dyn KeyStateBackend>,
        active_window_backend: Arc<dyn ActiveWindowBackend>,
    ) -> Self {
        Self::with_backends_and_reserved(store, key_state_backend, active_window_backend, &[])
    }
    pub fn with_backends_and_reserved(
        store: Arc<MkMacroStore>,
        key_state_backend: Arc<dyn KeyStateBackend>,
        active_window_backend: Arc<dyn ActiveWindowBackend>,
        reserved: &[(&str, &str)],
    ) -> Self {
        let reserved = reserved
            .iter()
            .map(|(name, chord)| ((*name).to_string(), (*chord).to_string()))
            .collect();
        Self::start(
            store,
            key_state_backend,
            active_window_backend,
            reserved,
            Arc::new(|id| {
                let _ = crate::mkmacro::runtime::run(id);
            }),
        )
    }
    fn start(
        store: Arc<MkMacroStore>,
        key_state_backend: Arc<dyn KeyStateBackend>,
        active_window_backend: Arc<dyn ActiveWindowBackend>,
        reserved: Vec<(String, String)>,
        trigger: Arc<Trigger>,
    ) -> Self {
        let snapshot = store.snapshot();
        let normalized_reserved = normalize_reserved_chords(&snapshot, reserved.iter().cloned());
        let state = Arc::new(Mutex::new(PollState {
            groups: compile_hotkey_groups_with_reserved(&snapshot, &normalized_reserved),
            snapshot,
            reserved,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let (
            worker_store,
            worker_stop,
            worker_state,
            worker_key_state_backend,
            worker_active_window_backend,
            worker_trigger,
        ) = (
            store.clone(),
            stop.clone(),
            state.clone(),
            key_state_backend.clone(),
            active_window_backend.clone(),
            trigger.clone(),
        );
        let worker = thread::Builder::new()
            .name("mkmacro-hotkeys".into())
            .spawn(move || {
                while !worker_stop.load(Ordering::SeqCst) {
                    tick(
                        &worker_store,
                        &worker_state,
                        worker_key_state_backend.as_ref(),
                        worker_active_window_backend.as_ref(),
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
            key_state_backend,
            active_window_backend,
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
    key_state_backend: &dyn KeyStateBackend,
    active_window_backend: &dyn ActiveWindowBackend,
    trigger: &F,
) where
    F: Fn(u64) + ?Sized,
{
    let snapshot = store.snapshot();
    let edge_groups = {
        let mut state = state.lock().unwrap();
        if !Arc::ptr_eq(&snapshot, &state.snapshot) {
            let normalized_reserved =
                normalize_reserved_chords(&snapshot, state.reserved.iter().cloned());
            let previously_triggered = state
                .groups
                .iter()
                .filter(|group| group.triggered)
                .map(|group| group.canonical_chord.clone())
                .collect::<BTreeSet<_>>();
            state.groups = compile_hotkey_groups_with_reserved(&snapshot, &normalized_reserved);
            for group in &mut state.groups {
                group.triggered = previously_triggered.contains(&group.canonical_chord);
            }
            state.snapshot = snapshot;
        }
        let ctrl = key_state_backend.is_down(&MkKey::Control);
        let shift = key_state_backend.is_down(&MkKey::Shift);
        let alt = key_state_backend.is_down(&MkKey::Alt);
        let meta = key_state_backend.is_down(&MkKey::Meta);
        let mut primary_states: Vec<(MkKey, bool)> = Vec::new();
        for group in &state.groups {
            if !primary_states.iter().any(|(key, _)| key == &group.primary) {
                primary_states.push((
                    group.primary.clone(),
                    key_state_backend.is_down(&group.primary),
                ));
            }
        }
        let mut edge_groups = Vec::new();
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
            edge_groups.push(group.clone());
        }
        edge_groups
    };

    let resolutions = edge_groups
        .into_iter()
        .map(|group| {
            let resolution = if group
                .contextual_candidates
                .iter()
                .any(|candidate| matches!(candidate.scope, MkHotkeyScope::ActiveWindow(_)))
            {
                match active_window_backend.active_window() {
                    Ok(foreground) => resolve_hotkey_group(&group, foreground.as_ref()),
                    Err(error) => HotkeyResolution::BackendFailure(error),
                }
            } else {
                resolve_hotkey_group(&group, None)
            };
            (group.canonical_chord, resolution)
        })
        .collect::<Vec<_>>();

    // The state lock is released before resolving external effects, logging,
    // or admitting a macro to the runtime. Each of these operations may
    // re-enter the service or store.
    for (chord, resolution) in resolutions {
        match resolution {
            HotkeyResolution::Run(macro_id) => trigger(macro_id),
            HotkeyResolution::NoMatch => {}
            HotkeyResolution::Ambiguous(candidates) => {
                report_ambiguity(&chord, &candidates);
            }
            HotkeyResolution::BackendFailure(error) => {
                report_resolution_failure(&chord, &error);
            }
        }
    }
}

fn report_ambiguity(chord: &str, candidates: &[CandidateSummary]) {
    tracing::warn!(chord = %chord, "Ambiguous mkmacro hotkey {chord}");
    for candidate in candidates {
        tracing::warn!(
            chord = %chord,
            macro_id = candidate.macro_id,
            macro_name = %candidate.display_name,
            "  candidate {} ({})",
            candidate.display_name,
            candidate.macro_id
        );
    }
}

fn report_resolution_failure(chord: &str, error: &ExecutionDiagnostic) {
    tracing::error!(
        chord = %chord,
        error = %error,
        "mkmacro hotkey resolution failed; no macro triggered"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{DiagnosticKind, MkMacro, MkPlayback};
    use std::sync::{
        Arc, RwLock,
        atomic::{AtomicUsize, Ordering},
    };
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
    fn process_mac(id: u64, process: &str) -> MkMacro {
        let mut m = mac(id, true);
        m.hotkey_scope = MkHotkeyScope::ActiveWindow(MkWindowMatcher {
            process: Some(process.into()),
            ..Default::default()
        });
        m
    }

    fn validation_document(macros: Vec<MkMacro>) -> MkMacroDocument {
        MkMacroDocument {
            macros,
            ..Default::default()
        }
    }

    fn assert_overlap_warnings(doc: &MkMacroDocument, ids: &[u64]) {
        let diagnostics = validate_hotkeys(doc, &[]);
        assert_eq!(diagnostics.len(), ids.len(), "{diagnostics:?}");
        for (diagnostic, id) in diagnostics.iter().zip(ids) {
            assert_eq!(diagnostic.macro_id, *id);
            assert_eq!(diagnostic.severity, HotkeyDiagnosticSeverity::Warning);
            assert_eq!(
                diagnostic.message,
                "Multiple window-specific macros share this hotkey. If more than one matcher matches the active window, none will run."
            );
        }
    }

    #[test]
    fn validation_firefox_and_visual_studio_warn_without_errors() {
        let mut studio = process_mac(2, "devenv.exe");
        // Physical aliases and character case must still form the same group.
        studio.hotkey.as_mut().unwrap().modifiers = vec![MkKey::RightControl];
        studio.hotkey.as_mut().unwrap().key = MkKey::Character("k".into());
        assert_overlap_warnings(
            &validation_document(vec![process_mac(1, "firefox.exe"), studio]),
            &[1, 2],
        );
    }

    #[test]
    fn validation_firefox_and_one_global_fallback_have_no_duplicate_diagnostic() {
        let doc = validation_document(vec![process_mac(1, "firefox.exe"), mac(2, true)]);
        assert!(validate_hotkeys(&doc, &[]).is_empty());
    }

    #[test]
    fn validation_two_globals_have_errors_on_both() {
        let doc = validation_document(vec![mac(1, true), mac(2, true)]);
        let diagnostics = validate_hotkeys(&doc, &[]);
        assert_eq!(diagnostics.len(), 2);
        for (diagnostic, id) in diagnostics.iter().zip([1, 2]) {
            assert_eq!(diagnostic.macro_id, id);
            assert_eq!(diagnostic.severity, HotkeyDiagnosticSeverity::Error);
            assert!(
                diagnostic
                    .message
                    .contains("Only one global fallback is allowed")
            );
        }
    }

    #[test]
    fn validation_identical_firefox_matchers_are_errors_regardless_of_order() {
        let mut doc = validation_document(vec![
            process_mac(1, "firefox.exe"),
            process_mac(2, "firefox.exe"),
        ]);
        for ids in [[1, 2], [2, 1]] {
            let diagnostics = validate_hotkeys(&doc, &[]);
            assert_eq!(diagnostics.len(), 2);
            for (diagnostic, id) in diagnostics.iter().zip(ids) {
                assert_eq!(diagnostic.macro_id, id);
                assert_eq!(diagnostic.severity, HotkeyDiagnosticSeverity::Error);
                assert!(diagnostic.message.contains("identical matchers"));
                assert!(diagnostic.message.contains("always ambiguous"));
                assert!(
                    diagnostic
                        .message
                        .contains("Document order does not resolve")
                );
            }
            doc.macros.reverse();
        }
    }

    #[test]
    fn validation_normalizes_empty_matcher_fields_without_changing_document() {
        for field in ["title", "title_regex", "process", "class"] {
            for empty in ["", " \t\n\u{2003}"] {
                let mut first = process_mac(1, "firefox.exe");
                if field == "process" {
                    first = scoped_mac(1, "Firefox");
                }
                let mut second = first.clone();
                second.id = 2;
                let MkHotkeyScope::ActiveWindow(matcher) = &mut second.hotkey_scope else {
                    unreachable!()
                };
                let value = match field {
                    "title" => &mut matcher.title,
                    "title_regex" => &mut matcher.title_regex,
                    "process" => &mut matcher.process,
                    "class" => &mut matcher.class,
                    _ => unreachable!(),
                };
                *value = Some(empty.into());
                let doc = validation_document(vec![first, second]);
                let before = doc.clone();
                let diagnostics = validate_hotkeys(&doc, &[]);
                assert_eq!(diagnostics.len(), 2, "{field}: {empty:?}");
                assert!(diagnostics.iter().all(|d| {
                    d.severity == HotkeyDiagnosticSeverity::Error
                        && d.message.contains("identical matchers")
                }));
                assert_eq!(doc, before);
            }
        }
    }

    #[test]
    fn validation_process_firefox_and_title_youtube_warn_about_potential_overlap() {
        assert_overlap_warnings(
            &validation_document(vec![
                process_mac(1, "firefox.exe"),
                scoped_mac(2, "YouTube"),
            ]),
            &[1, 2],
        );
    }

    #[test]
    fn validation_distinct_regex_class_and_nonempty_title_constraints_only_warn() {
        let mut regex = mac(3, true);
        regex.hotkey_scope = MkHotkeyScope::ActiveWindow(MkWindowMatcher {
            title_regex: Some("Firefox.*".into()),
            ..Default::default()
        });
        let mut class = mac(4, true);
        class.hotkey_scope = MkHotkeyScope::ActiveWindow(MkWindowMatcher {
            class: Some("MozillaWindowClass".into()),
            ..Default::default()
        });
        assert_overlap_warnings(
            &validation_document(vec![
                scoped_mac(1, "Firefox"),
                scoped_mac(2, " Firefox "),
                regex,
                class,
            ]),
            &[1, 2, 3, 4],
        );
    }

    #[test]
    fn validation_disabled_macros_do_not_affect_diagnostics() {
        let mut doc = validation_document(vec![
            process_mac(1, "firefox.exe"),
            mac(2, true),
            process_mac(3, "firefox.exe"),
            process_mac(4, "devenv.exe"),
            mac(5, true),
            mac(6, true),
            mac(7, true),
        ]);
        doc.macros[5].hotkey.as_mut().unwrap().key = MkKey::Function(9);
        doc.macros[6].hotkey.as_mut().unwrap().key = MkKey::Control;
        for m in &mut doc.macros[2..] {
            m.enabled = false;
        }
        assert!(validate_hotkeys(&doc, &[]).is_empty());
    }

    #[test]
    fn validation_invalid_contextual_matchers_do_not_cause_overlap_warnings() {
        let mut empty = mac(2, true);
        empty.hotkey_scope = MkHotkeyScope::ActiveWindow(MkWindowMatcher::default());
        let mut invalid = process_mac(3, "firefox.exe");
        let MkHotkeyScope::ActiveWindow(matcher) = &mut invalid.hotkey_scope else {
            unreachable!()
        };
        matcher.title_regex = Some("[".into());
        let doc = validation_document(vec![process_mac(1, "firefox.exe"), empty, invalid]);
        let diagnostics = validate_hotkeys(&doc, &[]);
        assert_eq!(diagnostics.len(), 2);
        for (diagnostic, id) in diagnostics.iter().zip([2, 3]) {
            assert_eq!(diagnostic.macro_id, id);
            assert_eq!(diagnostic.severity, HotkeyDiagnosticSeverity::Error);
            assert!(
                diagnostic
                    .message
                    .starts_with("Invalid active-window hotkey matcher")
            );
        }
    }

    #[test]
    fn validation_empty_whitespace_and_invalid_regex_matchers_are_errors() {
        for matcher in [
            MkWindowMatcher::default(),
            MkWindowMatcher {
                title: Some(" \t\n".into()),
                title_regex: Some(" ".into()),
                process: Some("\u{2003}".into()),
                class: Some(String::new()),
            },
            MkWindowMatcher {
                title_regex: Some("[".into()),
                ..Default::default()
            },
        ] {
            let mut m = mac(1, true);
            m.hotkey_scope = MkHotkeyScope::ActiveWindow(matcher);
            let mut doc = validation_document(vec![m]);
            let diagnostics = validate_hotkeys(&doc, &[]);
            assert_eq!(diagnostics.len(), 1);
            assert_eq!(diagnostics[0].severity, HotkeyDiagnosticSeverity::Error);
            assert_eq!(diagnostics[0].macro_id, 1);
            assert!(
                diagnostics[0]
                    .message
                    .starts_with("Invalid active-window hotkey matcher")
            );

            doc.macros[0].enabled = false;
            assert!(validate_hotkeys(&doc, &[]).is_empty());
            doc.macros[0].enabled = true;
            doc.macros[0].hotkey = None;
            assert!(validate_hotkeys(&doc, &[]).is_empty());
        }
    }

    #[test]
    fn validation_reports_invalid_matcher_alongside_malformed_chord() {
        let mut m = mac(1, true);
        m.hotkey_scope = MkHotkeyScope::ActiveWindow(MkWindowMatcher::default());
        m.hotkey.as_mut().unwrap().key = MkKey::Control;
        let diagnostics = validate_hotkeys(&validation_document(vec![m]), &[]);
        assert_eq!(diagnostics.len(), 2);
        assert!(
            diagnostics
                .iter()
                .all(|d| d.severity == HotkeyDiagnosticSeverity::Error)
        );
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.starts_with("Invalid active-window"))
        );
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.starts_with("Malformed hotkey"))
        );
    }
    #[test]
    fn validation_duplicate_errors_and_distinct_warnings_coexist_without_affecting_fallback() {
        let doc = validation_document(vec![
            process_mac(1, "firefox.exe"),
            process_mac(2, "firefox.exe"),
            process_mac(3, "devenv.exe"),
            mac(4, true),
        ]);
        let diagnostics = validate_hotkeys(&doc, &[]);
        assert_eq!(diagnostics.len(), 5);
        for id in [1, 2] {
            assert!(
                diagnostics
                    .iter()
                    .any(|d| d.macro_id == id && d.severity == HotkeyDiagnosticSeverity::Error)
            );
        }
        for id in [1, 2, 3] {
            assert!(
                diagnostics
                    .iter()
                    .any(|d| d.macro_id == id && d.severity == HotkeyDiagnosticSeverity::Warning)
            );
        }
        assert!(diagnostics.iter().all(|d| d.macro_id != 4));
    }

    #[test]
    fn validation_preserves_reserved_and_malformed_errors_alongside_duplicates() {
        let mut malformed = mac(5, true);
        malformed
            .hotkey
            .as_mut()
            .unwrap()
            .modifiers
            .push(MkKey::Character("X".into()));
        let doc = validation_document(vec![
            mac(1, true),
            mac(2, true),
            process_mac(3, "firefox.exe"),
            process_mac(4, "firefox.exe"),
            malformed,
        ]);
        let diagnostics = validate_hotkeys(&doc, &[("launcher", "Ctrl+K")]);
        assert_eq!(diagnostics.len(), 9, "{diagnostics:?}");
        assert!(
            diagnostics
                .iter()
                .all(|d| d.severity == HotkeyDiagnosticSeverity::Error)
        );
        for id in 1..=4 {
            assert_eq!(diagnostics.iter().filter(|d| d.macro_id == id).count(), 2);
            assert!(
                diagnostics
                    .iter()
                    .any(|d| d.macro_id == id && d.message == "hotkey conflicts with launcher")
            );
        }
        assert!(
            diagnostics
                .iter()
                .any(|d| d.macro_id == 5 && d.message.starts_with("Malformed hotkey"))
        );
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
                .unrestricted_candidates
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
        assert_eq!(groups[0].unrestricted_candidates.len(), 2);
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
                .unrestricted_candidates
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
        assert_eq!(groups[0].contextual_candidates[0].macro_id, 4);
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
                severity: HotkeyDiagnosticSeverity::Error,
                macro_id: 7,
                message: "hotkey conflicts with the recording toggle".into(),
            }]
        );
        assert!(compile_hotkey_groups(&d).is_empty());
    }
    #[test]
    fn recorder_toggle_conflict_is_not_registrable_for_a_contextual_macro() {
        let mut m = mac(8, true);
        m.hotkey = Some(MkHotkey {
            key: MkKey::Function(9),
            modifiers: vec![],
        });
        m.hotkey_scope = MkHotkeyScope::ActiveWindow(MkWindowMatcher {
            title: Some("Editor".into()),
            ..Default::default()
        });
        let d = MkMacroDocument {
            settings: Default::default(),
            schema_version: 1,
            folders: vec![],
            macros: vec![m],
        };
        assert!(compile_hotkey_groups(&d).is_empty());
        assert_eq!(validate_hotkeys(&d, &[]).len(), 1);
    }
    #[test]
    fn reserved_contextual_group_is_blocked_and_all_candidates_are_diagnosed() {
        let mut first = mac(11, true);
        first.hotkey_scope = MkHotkeyScope::ActiveWindow(MkWindowMatcher {
            title: Some("Editor".into()),
            ..Default::default()
        });
        let mut second = mac(12, true);
        second.hotkey_scope = MkHotkeyScope::ActiveWindow(MkWindowMatcher {
            title: Some("Terminal".into()),
            ..Default::default()
        });
        let d = MkMacroDocument {
            settings: Default::default(),
            schema_version: 1,
            folders: vec![],
            macros: vec![first, second],
        };
        let reserved =
            normalize_reserved_chords(&d, [("launcher".to_string(), " k + control ".to_string())]);
        assert!(compile_hotkey_groups_with_reserved(&d, &reserved).is_empty());
        let diagnostics = validate_hotkeys(&d, &[("launcher", " k + control ")]);
        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.message == "hotkey conflicts with launcher")
                .map(|diagnostic| diagnostic.macro_id)
                .collect::<Vec<_>>(),
            vec![11, 12]
        );
    }
    #[test]
    fn reserved_group_does_not_block_an_unrelated_group() {
        let mut unrelated = mac(13, true);
        unrelated.hotkey.as_mut().unwrap().key = MkKey::Character("J".into());
        let d = MkMacroDocument {
            settings: Default::default(),
            schema_version: 1,
            folders: vec![],
            macros: vec![mac(14, true), unrelated],
        };
        let reserved =
            normalize_reserved_chords(&d, [("launcher".to_string(), "Ctrl+K".to_string())]);
        let groups = compile_hotkey_groups_with_reserved(&d, &reserved);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].canonical_chord, "CONTROL+J");
    }
    #[test]
    fn service_constructor_applies_caller_reserved_chords() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        store
            .save(MkMacroDocument {
                settings: Default::default(),
                schema_version: 1,
                folders: vec![],
                macros: vec![mac(16, true)],
            })
            .unwrap();
        let service = MkMacroHotkeyService::with_backend_and_reserved(
            Arc::new(store),
            Arc::new(Fake(RwLock::new(Vec::new()))),
            &[("launcher", "control + k")],
        );
        assert!(service.state.lock().unwrap().groups.is_empty());
        service.shutdown();
    }
    #[test]
    fn duplicate_recorder_reservation_has_one_diagnostic() {
        let mut m = mac(15, true);
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
            validate_hotkeys(&d, &[("launcher", " f9 ")]),
            vec![HotkeyDiagnostic {
                severity: HotkeyDiagnosticSeverity::Error,
                macro_id: 15,
                message: "hotkey conflicts with the recording toggle".into(),
            }]
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
    struct ActiveWindowFake {
        snapshot: Mutex<Result<Option<WindowCandidate>, ActiveWindowError>>,
        query_count: AtomicUsize,
    }
    impl ActiveWindowFake {
        fn with_title(title: &str) -> Self {
            Self {
                snapshot: Mutex::new(Ok(Some(WindowCandidate {
                    handle: 1,
                    title: title.into(),
                    executable: "editor.exe".into(),
                    process_path: r"C:\Apps\editor.exe".into(),
                    class_name: "EditorClass".into(),
                }))),
                query_count: AtomicUsize::new(0),
            }
        }
        fn with_result(result: Result<Option<WindowCandidate>, ActiveWindowError>) -> Self {
            Self {
                snapshot: Mutex::new(result),
                query_count: AtomicUsize::new(0),
            }
        }
    }
    impl ActiveWindowBackend for ActiveWindowFake {
        fn active_window(&self) -> Result<Option<WindowCandidate>, ActiveWindowError> {
            self.query_count.fetch_add(1, Ordering::SeqCst);
            self.snapshot.lock().unwrap().clone()
        }
    }

    fn window(title: &str) -> WindowCandidate {
        WindowCandidate {
            handle: 1,
            title: title.into(),
            executable: "editor.exe".into(),
            process_path: r"C:\Apps\editor.exe".into(),
            class_name: "EditorClass".into(),
        }
    }

    fn scoped_mac(id: u64, title: &str) -> MkMacro {
        let mut m = mac(id, true);
        m.hotkey_scope = MkHotkeyScope::ActiveWindow(MkWindowMatcher {
            title: Some(title.into()),
            ..Default::default()
        });
        m
    }

    #[test]
    fn pure_resolver_matches_contextual_candidate_regardless_of_order() {
        let d = MkMacroDocument {
            settings: Default::default(),
            schema_version: 1,
            folders: vec![],
            macros: vec![scoped_mac(9, "Editor"), scoped_mac(2, "Terminal")],
        };
        let groups = compile_hotkey_groups(&d);
        assert_eq!(groups.len(), 1);
        let foreground = window("Editor");
        assert_eq!(
            resolve_hotkey_group(&groups[0], Some(&foreground)),
            HotkeyResolution::Run(9)
        );

        let mut reversed = groups[0].clone();
        reversed.contextual_candidates.reverse();
        assert_eq!(
            resolve_hotkey_group(&reversed, Some(&foreground)),
            HotkeyResolution::Run(9)
        );
    }

    #[test]
    fn contextual_match_has_precedence_over_unrestricted_fallback() {
        let d = MkMacroDocument {
            settings: Default::default(),
            schema_version: 1,
            folders: vec![],
            macros: vec![mac(2, true), scoped_mac(9, "Editor")],
        };
        let groups = compile_hotkey_groups(&d);
        let foreground = window("Editor");
        assert_eq!(
            resolve_hotkey_group(&groups[0], Some(&foreground)),
            HotkeyResolution::Run(9)
        );

        let mut reversed = groups[0].clone();
        reversed.contextual_candidates.reverse();
        assert_eq!(
            resolve_hotkey_group(&reversed, Some(&foreground)),
            HotkeyResolution::Run(9)
        );
    }

    #[test]
    fn multiple_contextual_matches_are_ambiguous_and_order_independent() {
        let d = MkMacroDocument {
            settings: Default::default(),
            schema_version: 1,
            folders: vec![],
            macros: vec![
                scoped_mac(9, "Editor"),
                scoped_mac(2, "Editor"),
                mac(4, true),
            ],
        };
        let groups = compile_hotkey_groups(&d);
        assert_eq!(validate_hotkeys(&d, &[]).len(), 2);

        let foreground = window("Editor");
        let expected = HotkeyResolution::Ambiguous(vec![
            CandidateSummary {
                display_name: "2".into(),
                macro_id: 2,
            },
            CandidateSummary {
                display_name: "9".into(),
                macro_id: 9,
            },
        ]);
        assert_eq!(
            resolve_hotkey_group(&groups[0], Some(&foreground)),
            expected
        );

        let mut reversed = groups[0].clone();
        reversed.contextual_candidates.reverse();
        assert_eq!(resolve_hotkey_group(&reversed, Some(&foreground)), expected);
    }

    #[test]
    fn one_unrestricted_candidate_is_the_fallback_when_context_does_not_match() {
        let d = MkMacroDocument {
            settings: Default::default(),
            schema_version: 1,
            folders: vec![],
            macros: vec![scoped_mac(9, "Editor"), mac(2, true)],
        };
        let groups = compile_hotkey_groups(&d);
        let foreground = window("Terminal");
        assert_eq!(
            resolve_hotkey_group(&groups[0], Some(&foreground)),
            HotkeyResolution::Run(2)
        );

        let mut reversed = groups[0].clone();
        reversed.contextual_candidates.reverse();
        assert_eq!(
            resolve_hotkey_group(&reversed, Some(&foreground)),
            HotkeyResolution::Run(2)
        );
    }

    #[test]
    fn no_contextual_or_unrestricted_match_is_no_match() {
        let d = MkMacroDocument {
            settings: Default::default(),
            schema_version: 1,
            folders: vec![],
            macros: vec![scoped_mac(9, "Editor")],
        };
        let groups = compile_hotkey_groups(&d);
        let foreground = window("Terminal");
        assert_eq!(
            resolve_hotkey_group(&groups[0], Some(&foreground)),
            HotkeyResolution::NoMatch
        );
        assert_eq!(
            resolve_hotkey_group(&groups[0], None),
            HotkeyResolution::NoMatch
        );
    }

    #[test]
    fn duplicate_unrestricted_candidates_have_no_fallback_when_context_does_not_match() {
        let d = MkMacroDocument {
            settings: Default::default(),
            schema_version: 1,
            folders: vec![],
            macros: vec![mac(9, true), mac(2, true), scoped_mac(4, "Editor")],
        };
        let groups = compile_hotkey_groups(&d);
        assert_eq!(
            resolve_hotkey_group(&groups[0], Some(&window("Terminal"))),
            HotkeyResolution::NoMatch
        );
        assert_eq!(
            validate_hotkeys(&d, &[]),
            vec![
                HotkeyDiagnostic {
                    severity: HotkeyDiagnosticSeverity::Error,
                    macro_id: 9,
                    message: "Multiple unrestricted macros use Ctrl+K. Only one global fallback is allowed.".into(),
                },
                HotkeyDiagnostic {
                    severity: HotkeyDiagnosticSeverity::Error,
                    macro_id: 2,
                    message: "Multiple unrestricted macros use Ctrl+K. Only one global fallback is allowed.".into(),
                },
            ]
        );
    }
    #[test]
    fn duplicate_unrestricted_candidates_do_not_block_matching_contextual_candidate() {
        let d = MkMacroDocument {
            settings: Default::default(),
            schema_version: 1,
            folders: vec![],
            macros: vec![mac(2, true), mac(4, true), scoped_mac(9, "Editor")],
        };
        let groups = compile_hotkey_groups(&d);
        assert_eq!(
            groups[0]
                .contextual_candidates
                .iter()
                .map(|candidate| candidate.macro_id)
                .collect::<Vec<_>>(),
            vec![9]
        );
        assert_eq!(
            groups[0]
                .unrestricted_candidates
                .iter()
                .map(|candidate| candidate.macro_id)
                .collect::<Vec<_>>(),
            vec![2, 4]
        );
        assert_eq!(
            resolve_hotkey_group(&groups[0], Some(&window("Editor"))),
            HotkeyResolution::Run(9)
        );
    }

    #[test]
    fn matcher_failure_blocks_fallback() {
        let group = HotkeyGroup {
            canonical_chord: "CONTROL+K".into(),
            modifiers: ["CONTROL".into()].into_iter().collect(),
            primary: MkKey::Character("K".into()),
            contextual_candidates: vec![HotkeyCandidate {
                macro_id: 9,
                display_name: "broken".into(),
                scope: MkHotkeyScope::ActiveWindow(MkWindowMatcher {
                    title_regex: Some("[".into()),
                    ..Default::default()
                }),
            }],
            unrestricted_candidates: vec![HotkeyCandidate {
                macro_id: 2,
                display_name: "fallback".into(),
                scope: MkHotkeyScope::AnyWindow,
            }],
            unarmed_candidates: vec![],
            triggered: false,
        };
        assert!(matches!(
            resolve_hotkey_group(&group, Some(&window("Editor"))),
            HotkeyResolution::BackendFailure(error)
                if error.kind == DiagnosticKind::InvalidTarget
                    && error.context.get("macro_id") == Some(&"9".to_string())
        ));
    }
    #[test]
    fn contextual_candidates_share_one_active_window_snapshot_per_group() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
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
        store
            .save(MkMacroDocument {
                settings: Default::default(),
                schema_version: 1,
                folders: vec![],
                macros: vec![editor, terminal],
            })
            .unwrap();
        let store = Arc::new(store);
        let snapshot = store.snapshot();
        let state = Mutex::new(PollState {
            groups: compile_hotkey_groups(&snapshot),
            snapshot,
            reserved: Vec::new(),
        });
        let keys = Fake(RwLock::new(vec![
            MkKey::Control,
            MkKey::Character("K".into()),
        ]));
        let active_window = ActiveWindowFake::with_title("Editor");
        let fired = Mutex::new(Vec::new());
        tick(&store, &state, &keys, &active_window, &|id| {
            fired.lock().unwrap().push(id)
        });
        assert_eq!(active_window.query_count.load(Ordering::SeqCst), 1);
        assert_eq!(*fired.lock().unwrap(), vec![9]);
    }
    #[test]
    fn active_window_backend_error_blocks_contextual_and_fallback_candidates() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        let mut contextual = mac(9, true);
        contextual.hotkey_scope = MkHotkeyScope::ActiveWindow(MkWindowMatcher {
            title: Some("Editor".into()),
            ..Default::default()
        });
        let fallback = mac(2, true);
        store
            .save(MkMacroDocument {
                settings: Default::default(),
                schema_version: 1,
                folders: vec![],
                macros: vec![contextual, fallback],
            })
            .unwrap();
        let store = Arc::new(store);
        let snapshot = store.snapshot();
        let state = Mutex::new(PollState {
            groups: compile_hotkey_groups(&snapshot),
            snapshot,
            reserved: Vec::new(),
        });
        let keys = Fake(RwLock::new(vec![
            MkKey::Control,
            MkKey::Character("K".into()),
        ]));
        let active_window = ActiveWindowFake::with_result(Err(ExecutionDiagnostic::new(
            DiagnosticKind::Backend,
            "foreground query failed",
        )));
        let fired = Mutex::new(Vec::new());
        tick(&store, &state, &keys, &active_window, &|id| {
            fired.lock().unwrap().push(id)
        });
        assert!(fired.lock().unwrap().is_empty());
        assert_eq!(active_window.query_count.load(Ordering::SeqCst), 1);

        keys.0.write().unwrap().clear();
        tick(&store, &state, &keys, &active_window, &|id| {
            fired.lock().unwrap().push(id)
        });
        *active_window.snapshot.lock().unwrap() = Ok(None);
        keys.0
            .write()
            .unwrap()
            .extend([MkKey::Control, MkKey::Character("K".into())]);
        tick(&store, &state, &keys, &active_window, &|id| {
            fired.lock().unwrap().push(id)
        });
        assert_eq!(*fired.lock().unwrap(), vec![2]);
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
            reserved: Vec::new(),
        });
        let fake = Fake(RwLock::new(vec![
            MkKey::Control,
            MkKey::Character("K".into()),
        ]));
        let fired = Mutex::new(vec![]);
        let cb = |id| fired.lock().unwrap().push(id);
        tick(&store, &state, &fake, &SystemActiveWindowBackend, &cb);
        tick(&store, &state, &fake, &SystemActiveWindowBackend, &cb);
        assert_eq!(*fired.lock().unwrap(), vec![1]);
        fake.0.write().unwrap().clear();
        tick(&store, &state, &fake, &SystemActiveWindowBackend, &cb);
        fake.0
            .write()
            .unwrap()
            .extend([MkKey::Control, MkKey::Character("K".into())]);
        tick(&store, &state, &fake, &SystemActiveWindowBackend, &cb);
        assert_eq!(*fired.lock().unwrap(), vec![1, 1]);
        store
            .save(MkMacroDocument {
                settings: Default::default(),
                schema_version: 1,
                folders: vec![],
                macros: vec![],
            })
            .unwrap();
        tick(&store, &state, &fake, &SystemActiveWindowBackend, &cb);
        assert_eq!(*fired.lock().unwrap(), vec![1, 1]);
    }
    #[test]
    fn disabling_one_duplicate_unrestricted_restores_remaining_fallback_after_snapshot_rebuild() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        store
            .save(MkMacroDocument {
                settings: Default::default(),
                schema_version: 1,
                folders: vec![],
                macros: vec![mac(1, true), mac(2, true)],
            })
            .unwrap();
        let store = Arc::new(store);
        let snapshot = store.snapshot();
        let state = Mutex::new(PollState {
            groups: compile_hotkey_groups(&snapshot),
            snapshot,
            reserved: Vec::new(),
        });
        let keys = Fake(RwLock::new(vec![
            MkKey::Control,
            MkKey::Character("K".into()),
        ]));
        let fired = Mutex::new(Vec::new());
        let cb = |id| fired.lock().unwrap().push(id);

        tick(&store, &state, &keys, &SystemActiveWindowBackend, &cb);
        assert!(fired.lock().unwrap().is_empty());

        keys.0.write().unwrap().clear();
        tick(&store, &state, &keys, &SystemActiveWindowBackend, &cb);
        let mut updated = (*store.snapshot()).clone();
        updated
            .macros
            .iter_mut()
            .find(|macro_| macro_.id == 2)
            .unwrap()
            .enabled = false;
        assert!(validate_hotkeys(&updated, &[]).is_empty());
        store.save(updated).unwrap();
        tick(&store, &state, &keys, &SystemActiveWindowBackend, &cb);

        keys.0
            .write()
            .unwrap()
            .extend([MkKey::Control, MkKey::Character("K".into())]);
        tick(&store, &state, &keys, &SystemActiveWindowBackend, &cb);
        assert_eq!(*fired.lock().unwrap(), vec![1]);
    }
}
