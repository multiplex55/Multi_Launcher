//! Polling macro-hotkey service and authoring diagnostics.
use super::validation::{MatcherValidationError, validate_window_matcher};
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
        if let Err(error) = validate_hotkey_scope(&m.hotkey_scope) {
            out.push(HotkeyDiagnostic {
                severity: HotkeyDiagnosticSeverity::Error,
                macro_id: m.id,
                message: format!(
                    "Invalid active-window hotkey matcher: {} This hotkey will not run.",
                    error.message
                ),
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
            .filter(|m| validate_hotkey_scope(&m.hotkey_scope).is_ok())
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

fn validate_hotkey_scope(scope: &MkHotkeyScope) -> Result<(), MatcherValidationError> {
    match scope {
        MkHotkeyScope::AnyWindow => Ok(()),
        MkHotkeyScope::ActiveWindow(matcher) => validate_window_matcher(matcher),
    }
}

/// Compiles one polling group for each usable physical chord.
///
/// Contextual candidates and unrestricted fallback candidates are compiled
/// into separate tiers. Multiple unrestricted candidates remain attached to
/// the group for ambiguity diagnostics and are never dispatched.
/// Statically invalid contextual matchers are unarmed individually. They cannot
/// match or block valid contextual peers or the single unrestricted fallback.
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
        if validate_hotkey_scope(&m.hotkey_scope).is_err() {
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
        [] => HotkeyResolution::NoMatch,
        candidates => HotkeyResolution::Ambiguous(candidate_summaries(candidates)),
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
    use crate::mkmacro::{
        DiagnosticKind, MkAction, MkMacro, MkMacroDocument, MkPlayback, MkStep, MkTextMode,
        MkTextPayload, RuntimeRunMode, RuntimeState, SCHEMA_VERSION, StepState,
        executor::fake::FakeBackend, runtime,
    };
    use serial_test::serial;
    use std::sync::{
        Arc, RwLock,
        atomic::{AtomicUsize, Ordering},
    };
    use std::{
        thread,
        time::{Duration, Instant},
    };
    fn mac(id: u64, on: bool) -> MkMacro {
        MkMacro {
            id,
            name: id.to_string(),
            description: String::new(),
            enabled: on,
            hotkey: Some(ctrl_k()),
            hotkey_scope: any_window(),
            folder_id: None,
            playback: MkPlayback::default(),
            steps: vec![],
            image_assets: vec![],
        }
    }
    fn ctrl_k() -> MkHotkey {
        MkHotkey {
            key: MkKey::Character("K".into()),
            modifiers: vec![MkKey::Control],
        }
    }

    fn any_window() -> MkHotkeyScope {
        MkHotkeyScope::AnyWindow
    }

    fn contextual_mac(id: u64, matcher: MkWindowMatcher) -> MkMacro {
        let mut m = mac(id, true);
        m.hotkey_scope = MkHotkeyScope::ActiveWindow(matcher);
        m
    }

    fn process_mac(id: u64, process: &str) -> MkMacro {
        contextual_mac(id, process_matcher(process))
    }

    fn process_matcher(process: &str) -> MkWindowMatcher {
        MkWindowMatcher {
            process: Some(process.into()),
            ..Default::default()
        }
    }

    fn title_matcher(title: &str) -> MkWindowMatcher {
        MkWindowMatcher {
            title: Some(title.into()),
            ..Default::default()
        }
    }

    fn regex_matcher(regex: &str) -> MkWindowMatcher {
        MkWindowMatcher {
            title_regex: Some(regex.into()),
            ..Default::default()
        }
    }

    fn class_matcher(class: &str) -> MkWindowMatcher {
        MkWindowMatcher {
            class: Some(class.into()),
            ..Default::default()
        }
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
    fn invalid_matchers_are_diagnosed_and_unarmed_without_blocking_peers_or_fallback() {
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
            let expected_detail = if matcher.title_regex.as_deref() == Some("[") {
                "unclosed character class"
            } else {
                "Enter at least one non-whitespace window criterion"
            };
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

            assert!(diagnostics[0].message.contains(expected_detail));
            let groups = compile_hotkey_groups(&doc);
            assert_eq!(groups.len(), 1);
            assert!(groups[0].contextual_candidates.is_empty());
            assert_eq!(groups[0].unarmed_candidates.len(), 1);
            assert_eq!(groups[0].unarmed_candidates[0].macro_id, 1);
            assert_eq!(
                resolve_hotkey_group(&groups[0], Some(&window("Editor"))),
                HotkeyResolution::NoMatch
            );

            // A statically invalid matcher cannot win over or suppress a fallback,
            // even when there is no foreground window.
            let mut with_fallback = doc.clone();
            with_fallback.macros.push(mac(2, true));
            let groups = compile_hotkey_groups(&with_fallback);
            for foreground in [None, Some(window("Editor"))] {
                assert_eq!(
                    resolve_hotkey_group(&groups[0], foreground.as_ref()),
                    HotkeyResolution::Run(2)
                );
            }

            with_fallback.macros.push(scoped_mac(3, "Editor"));
            for _ in 0..2 {
                let groups = compile_hotkey_groups(&with_fallback);
                assert_eq!(groups[0].contextual_candidates.len(), 1);
                assert_eq!(groups[0].contextual_candidates[0].macro_id, 3);
                assert_eq!(validate_hotkeys(&with_fallback, &[]), diagnostics);
                assert_eq!(
                    resolve_hotkey_group(&groups[0], Some(&window("Editor"))),
                    HotkeyResolution::Run(3)
                );
                assert_eq!(
                    resolve_hotkey_group(&groups[0], Some(&window("Terminal"))),
                    HotkeyResolution::Run(2)
                );
                with_fallback.macros.reverse();
            }

            doc.macros[0].enabled = false;
            assert!(validate_hotkeys(&doc, &[]).is_empty());
            doc.macros[0].enabled = true;
            doc.macros[0].hotkey = None;
            assert!(validate_hotkeys(&doc, &[]).is_empty());
        }
    }

    #[test]
    fn each_individual_matcher_criterion_is_armed_with_existing_matching_semantics() {
        for (field, value, nonmatch) in [
            ("process", "EDITOR.EXE", "other.exe"),
            ("process", "c:/apps/EDITOR.EXE", "C:/Other/editor.exe"),
            ("title", "dit", "editor"),
            ("title_regex", "^E.*r$", "^Terminal$"),
            ("class", "editorclass", "Editor"),
        ] {
            let set = |value: &str| {
                let mut matcher = MkWindowMatcher::default();
                let slot = match field {
                    "process" => &mut matcher.process,
                    "title" => &mut matcher.title,
                    "title_regex" => &mut matcher.title_regex,
                    "class" => &mut matcher.class,
                    _ => unreachable!(),
                };
                *slot = Some(value.into());
                matcher
            };
            for (value, expected) in [
                (value, HotkeyResolution::Run(1)),
                (nonmatch, HotkeyResolution::NoMatch),
            ] {
                let mut m = mac(1, true);
                m.hotkey_scope = MkHotkeyScope::ActiveWindow(set(value));
                let doc = validation_document(vec![m]);
                let before = doc.clone();
                assert!(validate_hotkeys(&doc, &[]).is_empty(), "{field}: {value}");
                let groups = compile_hotkey_groups(&doc);
                assert_eq!(doc, before);
                assert_eq!(groups[0].contextual_candidates.len(), 1);
                assert!(groups[0].unarmed_candidates.is_empty());
                assert_eq!(
                    resolve_hotkey_group(&groups[0], Some(&window("Editor"))),
                    expected,
                    "{field}: {value}"
                );
            }
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
            Arc::new(FakeKeyStateBackend::default()),
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
    #[derive(Default)]
    struct FakeKeyStateBackend(RwLock<Vec<MkKey>>);

    impl FakeKeyStateBackend {
        fn set_pressed(&self, keys: Vec<MkKey>) {
            *self.0.write().unwrap() = keys;
        }

        fn press_ctrl_k(&self) {
            self.set_pressed(vec![MkKey::Control, MkKey::Character("K".into())]);
        }

        fn release_primary(&self) {
            // Keep Ctrl down so this tests primary-key rearming specifically.
            self.set_pressed(vec![MkKey::Control]);
        }
    }

    impl KeyStateBackend for FakeKeyStateBackend {
        fn is_down(&self, key: &MkKey) -> bool {
            self.0.read().unwrap().contains(key)
        }
    }

    struct FakeActiveWindowBackend {
        snapshot: Mutex<Result<Option<WindowCandidate>, ActiveWindowError>>,
        query_count: AtomicUsize,
    }

    impl FakeActiveWindowBackend {
        fn with_result(result: Result<Option<WindowCandidate>, ActiveWindowError>) -> Self {
            Self {
                snapshot: Mutex::new(result),
                query_count: AtomicUsize::new(0),
            }
        }

        fn set_result(&self, result: Result<Option<WindowCandidate>, ActiveWindowError>) {
            *self.snapshot.lock().unwrap() = result;
        }

        fn queries(&self) -> usize {
            self.query_count.load(Ordering::SeqCst)
        }
    }

    impl ActiveWindowBackend for FakeActiveWindowBackend {
        fn active_window(&self) -> Result<Option<WindowCandidate>, ActiveWindowError> {
            self.query_count.fetch_add(1, Ordering::SeqCst);
            self.snapshot.lock().unwrap().clone()
        }
    }

    #[derive(Default)]
    struct TriggerRecorder(Mutex<Vec<u64>>);

    impl TriggerRecorder {
        fn record(&self, macro_id: u64) {
            self.0.lock().unwrap().push(macro_id);
        }

        fn ids(&self) -> Vec<u64> {
            self.0.lock().unwrap().clone()
        }
    }

    // Exercise the worker's tick function with persistent state and a real store,
    // but no polling thread or wall-clock waits.
    struct TickHarness {
        store: MkMacroStore,
        state: Mutex<PollState>,
        keys: FakeKeyStateBackend,
        foreground: FakeActiveWindowBackend,
        fired: TriggerRecorder,
        _directory: tempfile::TempDir,
    }

    impl TickHarness {
        fn new(macros: Vec<MkMacro>, foreground: Option<WindowCandidate>) -> Self {
            Self::with_reserved(validation_document(macros), foreground, Vec::new())
        }

        fn with_reserved(
            doc: MkMacroDocument,
            foreground: Option<WindowCandidate>,
            reserved: Vec<(String, String)>,
        ) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let (store, _) = MkMacroStore::open(directory.path()).unwrap();
            store.save(doc).unwrap();
            let snapshot = store.snapshot();
            let normalized = normalize_reserved_chords(&snapshot, reserved.iter().cloned());
            let state = Mutex::new(PollState {
                groups: compile_hotkey_groups_with_reserved(&snapshot, &normalized),
                snapshot,
                reserved,
            });
            Self {
                store,
                state,
                keys: FakeKeyStateBackend::default(),
                foreground: FakeActiveWindowBackend::with_result(Ok(foreground)),
                fired: TriggerRecorder::default(),
                _directory: directory,
            }
        }

        fn tick(&self) {
            tick(
                &self.store,
                &self.state,
                &self.keys,
                &self.foreground,
                &|id| self.fired.record(id),
            );
        }

        fn press(&self) {
            self.keys.press_ctrl_k();
            self.tick();
        }

        fn release(&self) {
            self.keys.release_primary();
            self.tick();
        }

        fn groups(&self) -> Vec<HotkeyGroup> {
            self.state.lock().unwrap().groups.clone()
        }
    }

    #[derive(Clone, Default)]
    struct CapturedLog(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for CapturedLog {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    // Use the production compiler, matcher/resolver, and worker dispatch for every
    // row. Only OS input and the runtime admission callback are replaced.
    fn assert_dispatch_table(
        document: &MkMacroDocument,
        cases: &[(&str, WindowCandidate, HotkeyResolution)],
    ) {
        for (name, foreground, expected) in cases {
            let h = TickHarness::with_reserved(document.clone(), Some(foreground.clone()), vec![]);
            let groups = h.groups();
            // A repeated chord is registered once, not rejected by frequency == 1.
            assert_eq!(groups.len(), 1, "{name}: physical chord registration");
            let group = &groups[0];
            assert_eq!(group.canonical_chord, "CONTROL+K", "{name}");
            assert_eq!(
                resolve_hotkey_group(group, Some(foreground)),
                *expected,
                "{name}: resolver"
            );

            let log = CapturedLog::default();
            let writer = log.clone();
            let subscriber = tracing_subscriber::fmt()
                .without_time()
                .with_ansi(false)
                .with_max_level(tracing::Level::WARN)
                .with_writer(move || writer.clone())
                .finish();
            tracing::subscriber::with_default(subscriber, || {
                h.press();
                h.tick(); // Holding the chord must not dispatch or log twice.
            });
            let output = String::from_utf8(log.0.lock().unwrap().clone()).unwrap();
            let expected_ids = match expected {
                HotkeyResolution::Run(id) => vec![*id],
                _ => vec![],
            };
            assert_eq!(h.fired.ids(), expected_ids, "{name}: dispatch");
            assert_eq!(
                h.foreground.queries(),
                usize::from(!group.contextual_candidates.is_empty()),
                "{name}: one foreground snapshot per chord, not per candidate"
            );
            if let HotkeyResolution::Ambiguous(candidates) = expected {
                assert_eq!(
                    output.matches("Ambiguous mkmacro hotkey CONTROL+K").count(),
                    1,
                    "{name}: missing or repeated ambiguity: {output}"
                );
                let identities = output
                    .lines()
                    .filter(|line| line.contains("macro_id="))
                    .collect::<Vec<_>>();
                assert_eq!(identities.len(), candidates.len(), "{name}: {output}");
                for candidate in candidates {
                    assert!(
                        identities
                            .iter()
                            .any(|line| line.contains("chord=CONTROL+K")
                                && line.contains(&format!("macro_id={}", candidate.macro_id))
                                && line
                                    .contains(&format!("macro_name={}", candidate.display_name))),
                        "{name}: missing {candidate:?}: {output}"
                    );
                }
            } else {
                assert!(output.is_empty(), "{name}: unexpected diagnostic: {output}");
            }
        }
    }

    fn shared_chord_cases() -> Vec<(&'static str, WindowCandidate, HotkeyResolution)> {
        vec![
            ("Firefox", firefox(), HotkeyResolution::Run(91)),
            ("Visual Studio", visual_studio(), HotkeyResolution::Run(23)),
            (
                "Notepad global fallback",
                notepad(),
                HotkeyResolution::Run(47),
            ),
        ]
    }
    fn firefox() -> WindowCandidate {
        WindowCandidate {
            handle: 1,
            title: "YouTube - Mozilla Firefox".into(),
            executable: "firefox.exe".into(),
            process_path: r"C:\Apps\Firefox\firefox.exe".into(),
            class_name: "MozillaWindowClass".into(),
        }
    }

    fn visual_studio() -> WindowCandidate {
        WindowCandidate {
            handle: 2,
            title: "Multi_Launcher - Microsoft Visual Studio".into(),
            executable: "devenv.exe".into(),
            process_path: r"C:\Apps\Visual Studio\devenv.exe".into(),
            class_name: "HwndWrapper".into(),
        }
    }

    fn notepad() -> WindowCandidate {
        WindowCandidate {
            handle: 3,
            title: "Untitled - Notepad".into(),
            executable: "notepad.exe".into(),
            process_path: r"C:\Windows\System32\notepad.exe".into(),
            class_name: "Notepad".into(),
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
        contextual_mac(id, title_matcher(title))
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
            HotkeyResolution::Ambiguous(vec![
                CandidateSummary {
                    macro_id: 2,
                    display_name: "2".into()
                },
                CandidateSummary {
                    macro_id: 9,
                    display_name: "9".into()
                },
            ])
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
    fn unexpected_matcher_failure_in_manually_built_group_blocks_fallback() {
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
    fn scope_blocks_hotkey_dispatch_but_not_direct_execution() {
        use crate::mkmacro::{
            CommandResult, MacroRuntime, MkAction, MkStep, MkTextMode, MkTextPayload,
            RuntimeCommand, RuntimeState, StepState, executor::fake::FakeBackend,
        };
        use std::time::Instant;

        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        let mut macro_ = process_mac(9, "firefox.exe");
        macro_.steps = vec![MkStep {
            id: 1,
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            action: MkAction::Text(MkTextPayload {
                text: "scoped macro executed".into(),
                mode: MkTextMode::Type,
            }),
        }];
        store
            .save(MkMacroDocument {
                schema_version: crate::mkmacro::SCHEMA_VERSION,
                settings: Default::default(),
                folders: vec![],
                macros: vec![macro_],
            })
            .unwrap();
        let store = Arc::new(store);
        let active_window = FakeActiveWindowBackend::with_result(Ok(None));
        let keys = FakeKeyStateBackend::default();
        let snapshot = store.snapshot();
        let state = Mutex::new(PollState {
            groups: compile_hotkey_groups(&snapshot),
            snapshot,
            reserved: Vec::new(),
        });
        let fired = TriggerRecorder::default();
        let trigger = |id| fired.record(id);
        for foreground in [Some(notepad()), None] {
            active_window.set_result(Ok(foreground));
            let effects = Arc::new(FakeBackend::default());
            let runtime = MacroRuntime::new(store.clone(), effects.clone().backends());
            assert_eq!(
                runtime.command(RuntimeCommand::Run(9)),
                CommandResult::Accepted
            );
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                let snapshot = runtime.snapshot();
                if snapshot.state == RuntimeState::Completed {
                    assert_eq!(snapshot.steps[&1], StepState::Success);
                    assert!(snapshot.latest_failure.is_none());
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "runtime did not complete: {snapshot:?}"
                );
                thread::sleep(Duration::from_millis(2));
            }
            assert_eq!(effects.events(), ["text:scoped macro executed"]);
            assert!(effects.window_calls.lock().unwrap().is_empty());

            keys.press_ctrl_k();
            tick(&store, &state, &keys, &active_window, &trigger);
            assert!(fired.ids().is_empty());
            keys.release_primary();
            tick(&store, &state, &keys, &active_window, &trigger);
        }
        assert_eq!(active_window.queries(), 2);

        // Positive control: the same enabled macro and chord fire in Firefox.
        active_window.set_result(Ok(Some(firefox())));
        keys.press_ctrl_k();
        tick(&store, &state, &keys, &active_window, &trigger);
        assert_eq!(fired.ids(), vec![9]);
        assert_eq!(active_window.queries(), 3);
    }

    #[test]
    fn folder_metadata_does_not_change_hotkey_registration_or_dispatch() {
        for macro_ in [mac(91, true), process_mac(91, "firefox.exe")] {
            let mut document = validation_document(vec![macro_]);
            document.folders = vec![
                crate::mkmacro::MkMacroFolder {
                    id: 42,
                    name: "Utilities".into(),
                },
                crate::mkmacro::MkMacroFolder {
                    id: 43,
                    name: "Work".into(),
                },
            ];
            let expected = compile_hotkey_groups(&document);
            assert_eq!(expected.len(), 1);
            for (folder_id, name) in [
                (None, "Utilities"),
                (Some(42), "Utilities"),
                (Some(42), "Renamed folder"),
                (Some(43), "Utilities"),
            ] {
                document.macros[0].folder_id = folder_id;
                document.folders[0].name = name.into();
                assert_eq!(compile_hotkey_groups(&document), expected);
                let h = TickHarness::with_reserved(document.clone(), Some(firefox()), vec![]);
                assert_eq!(h.groups(), expected);
                h.press();
                h.tick();
                assert_eq!(h.fired.ids(), vec![91]);
                h.release();
                h.press();
                assert_eq!(h.fired.ids(), vec![91, 91]);
            }
        }
    }

    #[test]
    fn moving_folders_preserves_hotkey_identity_and_held_edge() {
        let h = TickHarness::new(vec![process_mac(91, "firefox.exe")], Some(firefox()));
        let expected = h.groups();
        h.press();
        for destination in [Some(42), Some(43), None] {
            let mut document = (*h.store.snapshot()).clone();
            document.folders = vec![
                crate::mkmacro::MkMacroFolder {
                    id: 42,
                    name: "Utilities".into(),
                },
                crate::mkmacro::MkMacroFolder {
                    id: 43,
                    name: "Work".into(),
                },
            ];
            document.macros[0].folder_id = destination;
            h.store.save(document).unwrap();
            h.tick();
            assert_eq!(h.fired.ids(), vec![91]);
            assert!(h.groups()[0].triggered);
        }
        h.release();
        assert_eq!(h.groups(), expected);
        h.press();
        assert_eq!(h.fired.ids(), vec![91, 91]);
    }

    #[test]
    fn shared_chord_dispatch_table_covers_process_title_regex_and_class() {
        for (firefox_matcher, vs_matcher) in [
            (
                process_matcher("firefox.exe"),
                process_matcher("devenv.exe"),
            ),
            (
                title_matcher("Mozilla Firefox"),
                title_matcher("Visual Studio"),
            ),
            (regex_matcher("Firefox$"), regex_matcher("Visual Studio$")),
            (
                class_matcher("MozillaWindowClass"),
                class_matcher("HwndWrapper"),
            ),
        ] {
            let mut document = validation_document(vec![
                mac(47, true),
                contextual_mac(91, firefox_matcher),
                contextual_mac(23, vs_matcher),
            ]);
            for reverse in [false, true] {
                if reverse {
                    document.macros.reverse();
                }
                let groups = compile_hotkey_groups(&document);
                assert_eq!(groups.len(), 1);
                assert_eq!(groups[0].contextual_candidates.len(), 2);
                assert_eq!(groups[0].unrestricted_candidates.len(), 1);
                assert_dispatch_table(&document, &shared_chord_cases());
            }
        }
    }

    #[test]
    fn overlapping_scopes_report_chord_and_competing_identities_without_fallback() {
        for matcher in [
            title_matcher("YouTube"),
            regex_matcher("^YouTube.*Firefox$"),
            class_matcher("MozillaWindowClass"),
        ] {
            let mut browser = process_mac(91, "firefox.exe");
            browser.name = "Firefox controls".into();
            let mut video = contextual_mac(23, matcher);
            video.name = "YouTube controls".into();
            let mut document = validation_document(vec![browser, mac(47, true), video]);
            let cases = [
                (
                    "overlap suppresses global fallback",
                    firefox(),
                    HotkeyResolution::Ambiguous(vec![
                        CandidateSummary {
                            macro_id: 23,
                            display_name: "YouTube controls".into(),
                        },
                        CandidateSummary {
                            macro_id: 91,
                            display_name: "Firefox controls".into(),
                        },
                    ]),
                ),
                (
                    "fallback remains eligible outside overlap",
                    notepad(),
                    HotkeyResolution::Run(47),
                ),
            ];
            assert_dispatch_table(&document, &cases);
            document.macros.reverse();
            assert_dispatch_table(&document, &cases);
        }
    }

    #[test]
    fn multiple_any_window_candidates_are_ambiguous_on_dispatch() {
        for include_context in [false, true] {
            let mut first = mac(23, true);
            first.name = "Global first".into();
            let mut second = mac(47, true);
            second.name = "Global second".into();
            let mut document = validation_document(vec![second, first]);
            let mut cases = vec![(
                "multiple global matches",
                notepad(),
                HotkeyResolution::Ambiguous(vec![
                    CandidateSummary {
                        macro_id: 23,
                        display_name: "Global first".into(),
                    },
                    CandidateSummary {
                        macro_id: 47,
                        display_name: "Global second".into(),
                    },
                ]),
            )];
            if include_context {
                document.macros.push(process_mac(91, "firefox.exe"));
                cases.push((
                    "scoped match still takes precedence",
                    firefox(),
                    HotkeyResolution::Run(91),
                ));
            }
            assert_dispatch_table(&document, &cases);
            document.macros.reverse();
            assert_dispatch_table(&document, &cases);
        }
    }

    #[test]
    fn folder_assignment_does_not_change_shared_chord_grouping_or_precedence() {
        let mut document = validation_document(vec![
            mac(47, true),
            process_mac(91, "firefox.exe"),
            process_mac(23, "devenv.exe"),
        ]);
        document.folders = vec![
            crate::mkmacro::MkMacroFolder {
                id: 42,
                name: "Browser".into(),
            },
            crate::mkmacro::MkMacroFolder {
                id: 43,
                name: "Work".into(),
            },
        ];
        let expected = compile_hotkey_groups(&document);
        for assignments in [
            [None, None, None],
            [Some(42), Some(42), Some(42)],
            [None, Some(42), Some(43)],
            [Some(43), Some(42), None],
        ] {
            for (macro_, folder_id) in document.macros.iter_mut().zip(assignments) {
                macro_.folder_id = folder_id;
            }
            assert_eq!(compile_hotkey_groups(&document), expected);
            assert_dispatch_table(&document, &shared_chord_cases());
        }
    }
    #[test]
    fn unmatched_context_without_fallback_never_fires() {
        let h = TickHarness::new(
            vec![
                process_mac(91, "firefox.exe"),
                process_mac(23, "devenv.exe"),
            ],
            Some(notepad()),
        );
        h.press();
        h.tick();
        assert!(h.fired.ids().is_empty());
        assert_eq!(h.foreground.queries(), 1);
    }

    #[test]
    fn held_chord_fires_once_and_primary_release_rearms_it() {
        let h = TickHarness::new(vec![process_mac(91, "firefox.exe")], Some(firefox()));
        h.tick();
        assert_eq!(h.foreground.queries(), 0);
        h.press();
        for _ in 0..5 {
            h.tick();
        }
        assert_eq!(h.fired.ids(), vec![91]);
        assert_eq!(h.foreground.queries(), 1);
        h.release();
        assert!(!h.groups()[0].triggered);
        assert_eq!(h.foreground.queries(), 1);
        h.press();
        assert_eq!(h.fired.ids(), vec![91, 91]);
        assert_eq!(h.foreground.queries(), 2);
    }

    #[test]
    fn foreground_change_while_held_waits_for_release_and_repress() {
        let h = TickHarness::new(
            vec![
                process_mac(91, "firefox.exe"),
                process_mac(23, "devenv.exe"),
                mac(47, true),
            ],
            Some(firefox()),
        );
        h.press();
        assert_eq!(h.fired.ids(), vec![91]);
        h.foreground.set_result(Ok(Some(visual_studio())));
        for _ in 0..5 {
            h.tick();
        }
        assert_eq!(h.fired.ids(), vec![91]);
        // One foreground query per pressed edge, not per candidate or held tick.
        assert_eq!(h.foreground.queries(), 1);
        h.release();
        h.press();
        assert_eq!(h.fired.ids(), vec![91, 23]);
        assert_eq!(h.foreground.queries(), 2);
    }

    #[test]
    fn no_match_edge_is_consumed_until_primary_release() {
        let h = TickHarness::new(vec![process_mac(91, "firefox.exe")], Some(notepad()));
        h.press();
        h.foreground.set_result(Ok(Some(firefox())));
        h.tick();
        assert!(h.fired.ids().is_empty());
        assert_eq!(h.foreground.queries(), 1);
        h.release();
        h.press();
        assert_eq!(h.fired.ids(), vec![91]);
        assert_eq!(h.foreground.queries(), 2);
    }

    #[test]
    fn disabled_contextual_and_global_macros_are_absent_and_never_fire() {
        let mut disabled_context = process_mac(23, "firefox.exe");
        disabled_context.enabled = false;
        let h = TickHarness::new(
            vec![
                disabled_context,
                mac(47, false),
                process_mac(91, "firefox.exe"),
            ],
            Some(firefox()),
        );
        let groups = h.groups();
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0]
                .contextual_candidates
                .iter()
                .map(|c| c.macro_id)
                .collect::<Vec<_>>(),
            vec![91]
        );
        assert!(groups[0].unrestricted_candidates.is_empty());
        assert!(groups[0].unarmed_candidates.is_empty());
        h.press();
        h.release();
        h.foreground.set_result(Ok(Some(notepad())));
        h.press();
        assert_eq!(h.fired.ids(), vec![91]);
    }

    #[test]
    fn saved_matcher_refreshes_snapshot_and_preserves_held_edge_without_reconstruction() {
        let h = TickHarness::new(vec![process_mac(91, "firefox.exe")], Some(firefox()));
        h.press();
        assert_eq!(h.fired.ids(), vec![91]);
        let before = h.state.lock().unwrap().snapshot.clone();
        let mut updated = (*before).clone();
        updated.macros[0].hotkey_scope = MkHotkeyScope::ActiveWindow(process_matcher("devenv.exe"));
        h.store.save(updated).unwrap();
        let published = h.store.snapshot();
        assert!(!Arc::ptr_eq(&before, &published));
        assert!(Arc::ptr_eq(&before, &h.state.lock().unwrap().snapshot));
        h.tick();
        let observed = h.state.lock().unwrap().snapshot.clone();
        // The real file watcher may republish equivalent contents in another Arc.
        assert!(!Arc::ptr_eq(&before, &observed));
        assert_eq!(*observed, *published);
        assert!(h.groups()[0].triggered);
        assert_eq!(h.fired.ids(), vec![91]);
        assert_eq!(h.foreground.queries(), 1);
        // The old Firefox matcher must no longer dispatch after the refresh.
        h.release();
        h.press();
        assert_eq!(h.fired.ids(), vec![91]);
        h.release();
        h.foreground.set_result(Ok(Some(visual_studio())));
        h.press();
        assert_eq!(h.fired.ids(), vec![91, 91]);
        assert_eq!(h.foreground.queries(), 3);
    }

    #[test]
    fn duplicate_contextual_matchers_never_dispatch_or_fall_back() {
        let h = TickHarness::new(
            vec![
                process_mac(91, "firefox.exe"),
                process_mac(23, "firefox.exe"),
                mac(47, true),
            ],
            Some(firefox()),
        );
        assert_eq!(
            resolve_hotkey_group(&h.groups()[0], Some(&firefox())),
            HotkeyResolution::Ambiguous(vec![
                CandidateSummary {
                    macro_id: 23,
                    display_name: "23".into()
                },
                CandidateSummary {
                    macro_id: 91,
                    display_name: "91".into()
                },
            ])
        );
        h.press();
        h.tick();
        assert!(h.fired.ids().is_empty());
        assert_eq!(h.foreground.queries(), 1);
    }

    #[test]
    fn disabling_duplicate_fallback_refreshes_candidates_without_reconstruction() {
        let h = TickHarness::new(
            vec![mac(23, true), mac(47, true), process_mac(91, "firefox.exe")],
            Some(notepad()),
        );
        h.press();
        assert!(h.fired.ids().is_empty());
        h.release();
        h.foreground.set_result(Ok(Some(firefox())));
        h.press();
        assert_eq!(h.fired.ids(), vec![91]);
        h.release();
        let mut updated = (*h.store.snapshot()).clone();
        updated.macros[0].enabled = false;
        h.store.save(updated).unwrap();
        h.tick();
        assert_eq!(
            h.groups()[0]
                .unrestricted_candidates
                .iter()
                .map(|c| c.macro_id)
                .collect::<Vec<_>>(),
            vec![47]
        );
        h.foreground.set_result(Ok(Some(notepad())));
        h.press();
        assert_eq!(h.fired.ids(), vec![91, 47]);
    }

    #[test]
    fn invalid_matchers_are_unarmed_on_tick_without_blocking_valid_candidates() {
        let h = TickHarness::new(
            vec![
                contextual_mac(11, MkWindowMatcher::default()),
                contextual_mac(23, regex_matcher("[")),
                process_mac(91, "firefox.exe"),
                mac(47, true),
            ],
            Some(firefox()),
        );
        let group = h.groups().remove(0);
        assert_eq!(
            group
                .contextual_candidates
                .iter()
                .map(|c| c.macro_id)
                .collect::<Vec<_>>(),
            vec![91]
        );
        assert_eq!(
            group
                .unarmed_candidates
                .iter()
                .map(|c| c.macro_id)
                .collect::<Vec<_>>(),
            vec![11, 23]
        );
        h.press();
        h.release();
        h.foreground.set_result(Ok(Some(notepad())));
        h.press();
        assert_eq!(h.fired.ids(), vec![91, 47]);
    }

    #[test]
    fn regex_and_class_matchers_dispatch_against_foreground_candidates() {
        for matcher in [
            regex_matcher("^YouTube.*Firefox$"),
            class_matcher("MozillaWindowClass"),
        ] {
            let h = TickHarness::new(vec![contextual_mac(91, matcher)], Some(firefox()));
            h.press();
            h.release();
            h.foreground.set_result(Ok(Some(notepad())));
            h.press();
            assert_eq!(h.fired.ids(), vec![91]);
        }
    }

    #[test]
    fn recorder_and_caller_reserved_groups_never_poll_foreground_or_dispatch() {
        for recorder in [true, false] {
            let mut doc = validation_document(vec![process_mac(91, "firefox.exe"), mac(47, true)]);
            let reserved = if recorder {
                doc.settings.record_toggle_hotkey = ctrl_k();
                Vec::new()
            } else {
                vec![("launcher".into(), " k + RightControl ".into())]
            };
            let h = TickHarness::with_reserved(doc, Some(firefox()), reserved);
            assert!(h.groups().is_empty());
            h.press();
            h.tick();
            // Reservations must also survive snapshot recompilation.
            h.store.save((*h.store.snapshot()).clone()).unwrap();
            h.tick();
            h.release();
            h.press();
            assert!(h.groups().is_empty());
            assert!(h.fired.ids().is_empty());
            assert_eq!(h.foreground.queries(), 0);
        }
    }

    #[test]
    fn absent_foreground_uses_only_a_single_global_fallback() {
        for fallback in [false, true] {
            let mut macros = vec![process_mac(91, "firefox.exe")];
            if fallback {
                macros.push(mac(47, true));
            }
            let h = TickHarness::new(macros, None);
            h.press();
            h.tick();
            assert_eq!(h.fired.ids(), if fallback { vec![47] } else { vec![] });
            assert_eq!(h.foreground.queries(), 1);
        }
    }

    #[test]
    fn active_window_backend_error_blocks_context_and_fallback_until_new_edge() {
        let h = TickHarness::new(
            vec![process_mac(91, "firefox.exe"), mac(47, true)],
            Some(firefox()),
        );
        h.foreground.set_result(Err(ExecutionDiagnostic::new(
            DiagnosticKind::Backend,
            "foreground query failed",
        )));
        h.press();
        h.tick();
        assert!(h.fired.ids().is_empty());
        assert_eq!(h.foreground.queries(), 1);
        h.foreground.set_result(Ok(Some(firefox())));
        h.tick();
        assert!(h.fired.ids().is_empty());
        assert_eq!(h.foreground.queries(), 1);
        h.release();
        h.press();
        assert_eq!(h.fired.ids(), vec![91]);
        h.release();
        h.foreground.set_result(Ok(None));
        h.press();
        assert_eq!(h.fired.ids(), vec![91, 47]);
        assert_eq!(h.foreground.queries(), 3);
    }

    #[test]
    fn removing_all_candidates_through_store_refresh_removes_the_group() {
        let h = TickHarness::new(vec![process_mac(91, "firefox.exe")], Some(firefox()));
        h.press();
        let mut updated = (*h.store.snapshot()).clone();
        updated.macros.clear();
        h.store.save(updated).unwrap();
        h.tick();
        assert!(h.groups().is_empty());
        h.release();
        h.press();
        assert_eq!(h.fired.ids(), vec![91]);
        assert_eq!(h.foreground.queries(), 1);
    }
    #[test]
    fn global_only_group_does_not_query_foreground_backend() {
        let h = TickHarness::new(vec![mac(47, true)], None);
        h.foreground.set_result(Err(ExecutionDiagnostic::new(
            DiagnosticKind::Backend,
            "foreground must not be queried",
        )));
        h.press();
        h.tick();
        assert_eq!(h.fired.ids(), vec![47]);
        assert_eq!(h.foreground.queries(), 0);
    }

    fn breakpoint_macro(id: u64) -> MkMacro {
        let mut macro_ = mac(id, true);
        macro_.steps = vec![MkStep {
            id: 1,
            enabled: true,
            breakpoint: true,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            action: MkAction::Text(MkTextPayload {
                text: "hotkey breakpoint effect".into(),
                mode: MkTextMode::Type,
            }),
        }];
        macro_
    }

    fn wait_for_global_state(wanted: RuntimeState) -> Arc<crate::mkmacro::RuntimeSnapshot> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let snapshot = runtime::snapshot().expect("shared macro runtime is installed");
            if snapshot.state == wanted {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "runtime did not reach {wanted:?}: {snapshot:?}"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    fn wait_for_hotkey_completion() -> Arc<crate::mkmacro::RuntimeSnapshot> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let snapshot = runtime::snapshot().expect("shared macro runtime is installed");
            assert_ne!(
                snapshot.state,
                RuntimeState::Paused,
                "hotkey dispatch unexpectedly entered Debug mode"
            );
            if snapshot.state == RuntimeState::Completed {
                return snapshot;
            }
            assert!(
                !matches!(snapshot.state, RuntimeState::Failed | RuntimeState::Stopped),
                "hotkey dispatch failed: {snapshot:?}"
            );
            assert!(
                Instant::now() < deadline,
                "hotkey dispatch did not complete"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    #[serial]
    fn hotkey_dispatch_runs_breakpoint_step_in_normal_mode_exactly_once() {
        let directory = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(directory.path()).unwrap();
        store
            .save(MkMacroDocument {
                schema_version: SCHEMA_VERSION,
                macros: vec![breakpoint_macro(7401)],
                ..Default::default()
            })
            .unwrap();
        let store = Arc::new(store);
        let effects = Arc::new(FakeBackend::default());
        runtime::set_shared_store_with_backends(store.clone(), effects.clone().backends());

        let keys = Arc::new(FakeKeyStateBackend::default());
        let service = MkMacroHotkeyService::with_backend(store, keys.clone());
        keys.press_ctrl_k();
        let snapshot = wait_for_hotkey_completion();
        keys.release_primary();
        service.shutdown();

        assert_eq!(snapshot.macro_id, Some(7401));
        assert_eq!(snapshot.run_mode, RuntimeRunMode::Normal);
        assert_eq!(snapshot.steps[&1], StepState::Success);
        assert_eq!(effects.events(), vec!["text:hotkey breakpoint effect"]);
    }

    #[test]
    #[serial]
    fn direct_debug_dispatch_pauses_before_the_same_breakpoint_effect() {
        let directory = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(directory.path()).unwrap();
        store
            .save(MkMacroDocument {
                schema_version: SCHEMA_VERSION,
                macros: vec![breakpoint_macro(7402)],
                ..Default::default()
            })
            .unwrap();
        let effects = Arc::new(FakeBackend::default());
        runtime::set_shared_store_with_backends(Arc::new(store), effects.clone().backends());

        runtime::debug_run(7402).unwrap();
        let paused = wait_for_global_state(RuntimeState::Paused);
        assert_eq!(paused.macro_id, Some(7402));
        assert_eq!(paused.run_mode, RuntimeRunMode::Debug);
        assert!(effects.events().is_empty());

        runtime::resume().unwrap();
        let completed = wait_for_global_state(RuntimeState::Completed);
        assert_eq!(completed.run_mode, RuntimeRunMode::Debug);
        assert_eq!(effects.events(), vec!["text:hotkey breakpoint effect"]);
    }
}
