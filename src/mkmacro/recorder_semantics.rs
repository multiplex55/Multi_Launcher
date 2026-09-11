//! Pure semantic cleanup over literal recorder output.
//!
//! Plans use session-local IDs and source spans. Persisted step IDs are allocated
//! only when a reviewed plan is applied.

use super::{
    EventContext, KeyTranslation, KeyboardTranslationRequest, KeyboardTranslator, MkAction,
    MkCoordinateTarget, MkErrorPolicy, MkKey, MkMouseButton, MkMouseDragPayload,
    MkMouseMovePayload, MkMousePayload, MkMouseScrollAxis, MkPoint, MkRecorderSettings, MkStep,
    MkTextMode, MkTextPayload, MkWindowMatcher, MkWindowPayload, RecordedAction, RecordedStep,
    WindowContext, is_modifier, mk_key_from_windows_event,
};
use std::{
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReviewStepId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordingSourceSpan {
    pub first: usize,
    pub last: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RecordingProvenance {
    Literal,
    KeyTap,
    KeyHold,
    AutoRepeat,
    Chord,
    TextRun,
    MouseCleanup,
    WindowContext,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlannedStep {
    pub review_id: ReviewStepId,
    pub source: RecordingSourceSpan,
    /// Raw source event that semantically owns this action. This stays narrow
    /// even when `source` claims a broader modifier/provenance interval.
    pub association_source: usize,
    pub provenance: RecordingProvenance,
    pub action: MkAction,
    pub enabled: bool,
    pub breakpoint: bool,
    pub delay_after_ms: u64,
    pub repeat: u32,
    pub on_error: MkErrorPolicy,
    pub metadata: super::MkStepMetadata,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RecordingPlan {
    pub steps: Vec<PlannedStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrichedRecordedStep {
    pub source_index: usize,
    pub literal: RecordedStep,
    pub key: Option<MkKey>,
    pub translation: KeyTranslation,
}

pub fn enrich_keyboard(
    literal: &[RecordedStep],
    translator: &mut dyn KeyboardTranslator,
) -> Vec<EnrichedRecordedStep> {
    let state = translator.initial_key_state();
    enrich_keyboard_with_state(literal, translator, state)
}

pub fn enrich_keyboard_with_state(
    literal: &[RecordedStep],
    translator: &mut dyn KeyboardTranslator,
    mut state: [u8; 256],
) -> Vec<EnrichedRecordedStep> {
    let mut dead_key_pending = false;
    literal
        .iter()
        .enumerate()
        .map(|(source_index, step)| {
            let mut key = None;
            let mut translation = KeyTranslation::None;
            if let RecordedAction::Key {
                down,
                vk,
                scan_code,
                extended,
                ..
            } = step.action
            {
                let mapped = mk_key_from_windows_event(vk, scan_code, extended);
                key = Some(mapped.clone());
                let index = (vk & 0xff) as usize;
                if down {
                    let was_down = state[index] & 0x80 != 0;
                    state[index] |= 0x80;
                    if !was_down && matches!(vk, 0x14 | 0x90 | 0x91) {
                        state[index] ^= 1;
                    }
                    mirror_generic_modifier(&mut state, vk, true);
                    let translated = translator.translate(&KeyboardTranslationRequest {
                        key: mapped.clone(),
                        vk,
                        scan_code,
                        extended,
                        key_state: state,
                        keyboard_layout: step.context.as_ref().and_then(|c| c.keyboard_layout),
                    });
                    if !is_modifier(&mapped) {
                        translation = if dead_key_pending {
                            dead_key_pending = matches!(translated, KeyTranslation::DeadKey);
                            KeyTranslation::None
                        } else {
                            dead_key_pending = matches!(translated, KeyTranslation::DeadKey);
                            translated
                        };
                    }
                } else {
                    state[index] &= 0x7f;
                    mirror_generic_modifier(&mut state, vk, false);
                }
            }
            EnrichedRecordedStep {
                source_index,
                literal: step.clone(),
                key,
                translation,
            }
        })
        .collect()
}

fn mirror_generic_modifier(state: &mut [u8; 256], vk: u32, down: bool) {
    let generic = match vk {
        0xA0 | 0xA1 => Some(0x10),
        0xA2 | 0xA3 => Some(0x11),
        0xA4 | 0xA5 => Some(0x12),
        _ => None,
    };
    if let Some(generic) = generic {
        let any = match generic {
            0x10 => [0xA0, 0xA1],
            0x11 => [0xA2, 0xA3],
            _ => [0xA4, 0xA5],
        }
        .into_iter()
        .any(|i| state[i] & 0x80 != 0);
        state[generic] = if down || any { 0x80 } else { 0 };
    }
}

#[derive(Clone)]
struct KeyInterval {
    down: usize,
    repeat_downs: Vec<usize>,
    up: usize,
    key: MkKey,
    repeats: u32,
    translation: KeyTranslation,
}

pub fn build_recording_plan(
    enriched: &[EnrichedRecordedStep],
    settings: &MkRecorderSettings,
) -> RecordingPlan {
    let intervals = key_intervals(enriched);
    let mut interval_at = HashMap::new();
    let mut consumed = HashSet::new();
    for (n, interval) in intervals.iter().enumerate() {
        interval_at.insert(interval.down, n);
    }
    let collapsible_modifiers: HashSet<usize> = intervals
        .iter()
        .filter(|modifier| is_modifier(&modifier.key))
        .filter_map(|modifier| {
            let nested = intervals
                .iter()
                .filter(|interval| {
                    !is_modifier(&interval.key)
                        && interval.down > modifier.down
                        && interval.up < modifier.up
                        && interval.repeats == 1
                        && enriched[interval.up]
                            .literal
                            .timestamp_us
                            .saturating_sub(enriched[interval.down].literal.timestamp_us)
                            / 1000
                            <= settings.key_tap_max_ms
                })
                .collect::<Vec<_>>();
            let crosses_non_key_action = enriched[modifier.down + 1..modifier.up]
                .iter()
                .any(|item| !matches!(item.literal.action, RecordedAction::Key { .. }));
            let held_ms = enriched[modifier.up]
                .literal
                .timestamp_us
                .saturating_sub(enriched[modifier.down].literal.timestamp_us)
                / 1000;
            (!nested.is_empty() && !crosses_non_key_action && held_ms <= settings.key_tap_max_ms)
                .then_some(modifier.down)
        })
        .collect();

    let mut planned = Vec::new();
    for (index, item) in enriched.iter().enumerate() {
        if consumed.contains(&index) {
            continue;
        }
        if let Some(&number) = interval_at.get(&index) {
            let interval = &intervals[number];
            consumed.insert(interval.up);
            consumed.extend(interval.repeat_downs.iter().copied());
            if interval.repeats > 1 {
                let held_ms = enriched[interval.up]
                    .literal
                    .timestamp_us
                    .saturating_sub(enriched[interval.down].literal.timestamp_us)
                    / 1000;
                planned.push(make_step(
                    interval.down,
                    interval.down,
                    RecordingProvenance::AutoRepeat,
                    MkAction::KeyDown(interval.key.clone()),
                    held_ms,
                    1,
                ));
                planned.push(make_step(
                    interval.up,
                    interval.up,
                    RecordingProvenance::AutoRepeat,
                    MkAction::KeyUp(interval.key.clone()),
                    enriched[interval.up].literal.delay_after_ms,
                    1,
                ));
                continue;
            }
            if !is_modifier(&interval.key) {
                let mut compound: Vec<_> = intervals
                    .iter()
                    .filter(|other| {
                        !is_modifier(&other.key)
                            && other.down >= interval.down
                            && other.down < interval.up
                            && other.up > interval.down
                            && other.repeats == 1
                    })
                    .collect();
                compound.sort_by_key(|other| other.down);
                let all_down_before_any_up =
                    compound.iter().map(|i| i.down).max() < compound.iter().map(|i| i.up).min();
                if compound.len() > 1 && all_down_before_any_up {
                    let first_down = compound[0].down;
                    let last_up = compound.iter().map(|i| i.up).max().unwrap_or(interval.up);
                    let chord_ms = enriched[last_up]
                        .literal
                        .timestamp_us
                        .saturating_sub(enriched[first_down].literal.timestamp_us)
                        / 1000;
                    if chord_ms <= settings.key_tap_max_ms {
                        let trigger_down = compound
                            .iter()
                            .map(|interval| interval.down)
                            .max()
                            .unwrap_or(first_down);
                        let modifiers: Vec<_> = intervals
                            .iter()
                            .filter(|m| {
                                is_modifier(&m.key)
                                    && collapsible_modifiers.contains(&m.down)
                                    && m.down < first_down
                                    && m.up > last_up
                            })
                            .collect();
                        let mut keys: Vec<_> = modifiers.iter().map(|m| m.key.clone()).collect();
                        keys.extend(compound.iter().map(|i| i.key.clone()));
                        let first = modifiers.iter().map(|m| m.down).min().unwrap_or(first_down);
                        let last = modifiers.iter().map(|m| m.up).max().unwrap_or(last_up);
                        for part in compound {
                            consumed.insert(part.down);
                            consumed.insert(part.up);
                        }
                        for modifier in modifiers {
                            consumed.insert(modifier.down);
                            consumed.insert(modifier.up);
                        }
                        let mut step = make_step(
                            first,
                            last,
                            RecordingProvenance::Chord,
                            MkAction::Hotkey(keys),
                            enriched[last_up].literal.delay_after_ms,
                            1,
                        );
                        step.association_source = trigger_down;
                        planned.push(step);
                        continue;
                    }
                }
            }
            let modifiers: Vec<_> = intervals
                .iter()
                .filter(|m| {
                    is_modifier(&m.key)
                        && collapsible_modifiers.contains(&m.down)
                        && m.down < interval.down
                        && m.up > interval.up
                })
                .collect();
            let held_ms = enriched[interval.up]
                .literal
                .timestamp_us
                .saturating_sub(item.literal.timestamp_us)
                / 1000;
            if !is_modifier(&interval.key)
                && !modifiers.is_empty()
                && held_ms <= settings.key_tap_max_ms
                && interval.repeats == 1
            {
                let mut keys: Vec<MkKey> = modifiers.iter().map(|m| m.key.clone()).collect();
                keys.push(interval.key.clone());
                let first = modifiers
                    .iter()
                    .map(|m| m.down)
                    .min()
                    .unwrap_or(interval.down);
                let last = modifiers.iter().map(|m| m.up).max().unwrap_or(interval.up);
                for modifier in modifiers {
                    consumed.insert(modifier.down);
                    consumed.insert(modifier.up);
                }
                let mut step = make_step(
                    first,
                    last,
                    RecordingProvenance::Chord,
                    MkAction::Hotkey(keys),
                    enriched[interval.up].literal.delay_after_ms,
                    interval.repeats,
                );
                step.association_source = interval.down;
                planned.push(step);
            } else if is_modifier(&interval.key) && collapsible_modifiers.contains(&interval.down) {
                consumed.insert(interval.up);
            } else {
                let modifier_encloses_activity =
                    is_modifier(&interval.key) && interval.up > interval.down.saturating_add(1);
                if held_ms <= settings.key_tap_max_ms && !modifier_encloses_activity {
                    let provenance = if interval.repeats > 1 {
                        RecordingProvenance::AutoRepeat
                    } else {
                        RecordingProvenance::KeyTap
                    };
                    let mut step = make_step(
                        interval.down,
                        interval.up,
                        provenance,
                        MkAction::KeyPress(interval.key.clone()),
                        enriched[interval.up].literal.delay_after_ms,
                        interval.repeats,
                    );
                    step.association_source = interval.down;
                    planned.push(step);
                } else {
                    planned.push(make_step(
                        interval.down,
                        interval.down,
                        RecordingProvenance::KeyHold,
                        MkAction::KeyDown(interval.key.clone()),
                        item.literal.delay_after_ms,
                        1,
                    ));
                    planned.push(make_step(
                        interval.up,
                        interval.up,
                        RecordingProvenance::KeyHold,
                        MkAction::KeyUp(interval.key.clone()),
                        enriched[interval.up].literal.delay_after_ms,
                        1,
                    ));
                }
            }
            continue;
        }
        planned.extend(literal_action(item, settings.record_window_context));
    }
    planned.sort_by_key(|step| (step.source.first, step.review_id.0));
    if settings.smart_keyboard_cleanup {
        fold_text_runs(
            &mut planned,
            enriched,
            settings.text_run_gap_ms,
            &collapsible_modifiers,
        );
    }
    cleanup_delays(
        &mut planned,
        settings.minimum_idle_delay_ms,
        settings.delay_rounding_ms,
    );
    if settings.smart_mouse_cleanup {
        cleanup_mouse(&mut planned);
    }
    if settings.smart_window_cleanup && settings.record_window_context {
        author_window_context(&mut planned, enriched);
    }
    assign_stable_review_ids(&mut planned);
    RecordingPlan { steps: planned }
}

fn key_intervals(items: &[EnrichedRecordedStep]) -> Vec<KeyInterval> {
    let mut down: HashMap<MkKey, (usize, Vec<usize>, KeyTranslation)> = HashMap::new();
    let mut result = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let RecordedAction::Key { down: is_down, .. } = item.literal.action else {
            continue;
        };
        let Some(key) = item.key.clone() else {
            continue;
        };
        if is_down {
            down.entry(key)
                .and_modify(|entry| entry.1.push(index))
                .or_insert((index, Vec::new(), item.translation.clone()));
        } else if let Some((start, repeat_downs, translation)) = down.remove(&key) {
            result.push(KeyInterval {
                down: start,
                repeats: repeat_downs.len() as u32 + 1,
                repeat_downs,
                up: index,
                key,
                translation,
            });
        }
    }
    result.sort_by_key(|i| i.down);
    result
}

fn make_step(
    first: usize,
    last: usize,
    provenance: RecordingProvenance,
    action: MkAction,
    delay_after_ms: u64,
    repeat: u32,
) -> PlannedStep {
    PlannedStep {
        review_id: ReviewStepId(first as u64 + 1),
        source: RecordingSourceSpan { first, last },
        association_source: last,
        provenance,
        action,
        enabled: true,
        breakpoint: false,
        delay_after_ms,
        repeat: repeat.max(1),
        on_error: MkErrorPolicy::Stop,
        metadata: Default::default(),
    }
}

fn literal_action(item: &EnrichedRecordedStep, windows: bool) -> Vec<PlannedStep> {
    let s = &item.literal;
    let window = if windows {
        match s.action {
            RecordedAction::Key { .. } => s.context.as_ref().map(|c| &c.foreground),
            _ => s
                .context
                .as_ref()
                .and_then(|c| c.window_under_point.as_ref().or(Some(&c.foreground))),
        }
    } else {
        None
    };
    let target = |x, y| coordinate_target(x, y, window);
    let action = match s.action {
        RecordedAction::Key {
            down,
            vk,
            scan_code,
            extended,
            ..
        } => {
            if down {
                MkAction::KeyDown(mk_key_from_windows_event(vk, scan_code, extended))
            } else {
                MkAction::KeyUp(mk_key_from_windows_event(vk, scan_code, extended))
            }
        }
        RecordedAction::Move { x, y, duration_ms } => MkAction::MouseMove(MkMouseMovePayload {
            target: target(x, y),
            duration_ms,
        }),
        RecordedAction::Click {
            button,
            x,
            y,
            count,
        } => MkAction::MouseClick(MkMousePayload {
            target: target(x, y),
            button: mouse_button(button),
            clicks: count,
        }),
        RecordedAction::Down { button, .. } => MkAction::MouseDown(mouse_button(button)),
        RecordedAction::Up { button, .. } => MkAction::MouseUp(mouse_button(button)),
        RecordedAction::Drag {
            button,
            from,
            to,
            down_timestamp_us,
            up_timestamp_us,
        } => MkAction::MouseDrag(MkMouseDragPayload {
            from: target(from.0, from.1),
            to: target(to.0, to.1),
            button: mouse_button(button),
            duration_ms: up_timestamp_us.saturating_sub(down_timestamp_us) / 1000,
        }),
        RecordedAction::Wheel {
            delta, horizontal, ..
        } => MkAction::MouseScroll {
            axis: if horizontal {
                MkMouseScrollAxis::Horizontal
            } else {
                MkMouseScrollAxis::Vertical
            },
            i32_delta: delta,
        },
    };
    vec![make_step(
        item.source_index,
        item.source_index,
        RecordingProvenance::Literal,
        action,
        s.delay_after_ms,
        1,
    )]
}

fn coordinate_target(x: i32, y: i32, window: Option<&WindowContext>) -> MkCoordinateTarget {
    if let Some((window, matcher)) = window.zip(window.and_then(WindowContext::matcher))
        && let Some(origin) = window.client_origin
    {
        MkCoordinateTarget::WindowClient {
            matcher,
            point: MkPoint {
                x: x - origin.x,
                y: y - origin.y,
            },
        }
    } else {
        MkCoordinateTarget::Screen {
            point: MkPoint { x, y },
        }
    }
}
fn mouse_button(button: super::MouseButton) -> MkMouseButton {
    match button {
        super::MouseButton::Left => MkMouseButton::Left,
        super::MouseButton::Right => MkMouseButton::Right,
        super::MouseButton::Middle => MkMouseButton::Middle,
        super::MouseButton::X1 => MkMouseButton::X1,
        super::MouseButton::X2 => MkMouseButton::X2,
    }
}

fn fold_text_runs(
    planned: &mut Vec<PlannedStep>,
    enriched: &[EnrichedRecordedStep],
    gap_ms: u64,
    collapsible_modifiers: &HashSet<usize>,
) {
    let intervals = key_intervals(enriched);
    let mut output = Vec::new();
    let mut i = 0;
    while i < planned.len() {
        let mut end = i;
        let mut text = String::new();
        while end < planned.len() {
            let step = &planned[end];
            let Some(value) = translation_for_step(step, &intervals, collapsible_modifiers) else {
                break;
            };
            let text_capable = match &step.action {
                MkAction::KeyPress(_) => true,
                MkAction::Hotkey(keys) => {
                    keys.last().is_some_and(|key| !is_modifier(key))
                        && (keys[..keys.len().saturating_sub(1)].iter().all(|key| {
                            matches!(key, MkKey::Shift | MkKey::LeftShift | MkKey::RightShift)
                        }) || is_alt_gr_chord(keys))
                }
                _ => false,
            };
            if !text_capable || (end > i && planned[end - 1].delay_after_ms > gap_ms) {
                break;
            }
            let same_target = same_text_target(
                enriched,
                representative_source(step, enriched),
                representative_source(&planned[i], enriched),
            );
            if !same_target {
                break;
            }
            text.push_str(&value);
            end += 1;
        }
        if end - i >= 2 && text.chars().count() >= 2 {
            let last = &planned[end - 1];
            let mut first = planned[i].source.first;
            let mut last_source = last.source.last;
            for interval in &intervals {
                if is_modifier(&interval.key) && interval.down < first && interval.up > last_source
                {
                    first = first.min(interval.down);
                    last_source = last_source.max(interval.up);
                }
            }
            let mut step = make_step(
                first,
                last_source,
                RecordingProvenance::TextRun,
                MkAction::Text(MkTextPayload {
                    text,
                    mode: MkTextMode::Type,
                }),
                last.delay_after_ms,
                1,
            );
            step.association_source = planned[i].association_source;
            output.push(step);
            i = end;
        } else {
            output.push(planned[i].clone());
            i += 1;
        }
    }
    *planned = output;
}

fn translation_for_step(
    step: &PlannedStep,
    intervals: &[KeyInterval],
    collapsible_modifiers: &HashSet<usize>,
) -> Option<String> {
    if intervals.iter().any(|interval| {
        is_modifier(&interval.key)
            && !collapsible_modifiers.contains(&interval.down)
            && interval.down < step.source.first
            && interval.up > step.source.last
    }) {
        return None;
    }
    let primary = match &step.action {
        MkAction::KeyPress(key) => key,
        MkAction::Hotkey(keys) => keys.last()?,
        _ => return None,
    };
    intervals
        .iter()
        .find(|interval| {
            interval.key == *primary
                && interval.repeats == 1
                && interval.down >= step.source.first
                && interval.up <= step.source.last
        })
        .and_then(|interval| match &interval.translation {
            KeyTranslation::Text(text) if text.chars().all(|c| !c.is_control()) => {
                Some(text.clone())
            }
            _ => None,
        })
}

fn is_alt_gr_chord(keys: &[MkKey]) -> bool {
    let modifiers = &keys[..keys.len().saturating_sub(1)];
    modifiers.len() == 2
        && modifiers.iter().any(|key| {
            matches!(
                key,
                MkKey::Control | MkKey::LeftControl | MkKey::RightControl
            )
        })
        && modifiers.iter().any(|key| matches!(key, MkKey::RightAlt))
}

fn same_text_target(enriched: &[EnrichedRecordedStep], a: usize, b: usize) -> bool {
    let a = enriched.get(a).and_then(|e| e.literal.context.as_ref());
    let b = enriched.get(b).and_then(|e| e.literal.context.as_ref());
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => window_identity(&a.foreground)
            .zip(window_identity(&b.foreground))
            .is_some_and(|(a, b)| a == b),
        _ => false,
    }
}

/// Chooses the input event that owns an action's window context independently
/// from the broader provenance span used by review suggestions. Modifier-held
/// chords deliberately claim the modifier's full interval, but their target is
/// the window receiving the primary key rather than the window where the
/// modifier was eventually released.
fn representative_source(step: &PlannedStep, enriched: &[EnrichedRecordedStep]) -> usize {
    enriched
        .get(step.association_source)
        .map(|_| step.association_source)
        .unwrap_or(step.source.last)
}

#[derive(Clone, PartialEq)]
enum WindowIdentity {
    Native(usize),
    Matcher(MkWindowMatcher),
}

fn window_identity(window: &WindowContext) -> Option<WindowIdentity> {
    window
        .native_root_id
        .map(WindowIdentity::Native)
        .or_else(|| window.matcher().map(WindowIdentity::Matcher))
}

fn cleanup_delays(steps: &mut [PlannedStep], threshold: u64, quantum: u64) {
    let quantum = quantum.max(1);
    for step in steps {
        if step.provenance == RecordingProvenance::AutoRepeat
            || (matches!(step.action, MkAction::KeyDown(_))
                && step.provenance == RecordingProvenance::KeyHold)
        {
            continue;
        }
        let rounded =
            (step.delay_after_ms.saturating_add(quantum / 2) / quantum).saturating_mul(quantum);
        step.delay_after_ms = if rounded < threshold { 0 } else { rounded };
    }
}

fn cleanup_mouse(steps: &mut Vec<PlannedStep>) {
    let mut output: Vec<PlannedStep> = Vec::new();
    for mut step in steps.drain(..) {
        if let MkAction::MouseMove(_) = &step.action {
            if let Some(previous) = output.last_mut()
                && let (MkAction::MouseMove(a), MkAction::MouseMove(b)) =
                    (&previous.action, &step.action)
                && a.target == b.target
            {
                previous.delay_after_ms = step.delay_after_ms;
                previous.source.last = step.source.last;
                previous.association_source = step.association_source;
                previous.provenance = RecordingProvenance::MouseCleanup;
                continue;
            }
        }
        let redundant_pre_click =
            output
                .last()
                .and_then(|previous| match (&previous.action, &step.action) {
                    (MkAction::MouseMove(m), MkAction::MouseClick(c))
                        if m.duration_ms == 0
                            && previous.delay_after_ms == 0
                            && m.target == c.target =>
                    {
                        Some(previous.source.first)
                    }
                    _ => None,
                });
        if let Some(first) = redundant_pre_click {
            output.pop();
            step.source.first = first;
            step.provenance = RecordingProvenance::MouseCleanup;
        }
        output.push(step);
    }
    *steps = output;
}

fn author_window_context(steps: &mut Vec<PlannedStep>, enriched: &[EnrichedRecordedStep]) {
    let mut output = Vec::new();
    let mut active: Option<WindowIdentity> = None;
    for step in steps.drain(..) {
        let relevant = !matches!(step.action, MkAction::MouseMove(_));
        let context_source = representative_source(&step, enriched);
        let window = enriched
            .get(context_source)
            .and_then(|e| e.literal.context.as_ref())
            .and_then(|c| {
                if matches!(
                    step.action,
                    MkAction::KeyDown(_)
                        | MkAction::KeyUp(_)
                        | MkAction::KeyPress(_)
                        | MkAction::Hotkey(_)
                        | MkAction::Text(_)
                ) {
                    Some(&c.foreground)
                } else {
                    c.window_under_point.as_ref().or(Some(&c.foreground))
                }
            })
            .filter(|window| window.process_id != Some(std::process::id()));
        let matcher = window.and_then(WindowContext::matcher);
        let identity = window.and_then(window_identity);
        if relevant && matcher.is_some() && identity != active {
            let matcher = matcher.unwrap();
            output.push(make_step(
                context_source,
                context_source,
                RecordingProvenance::WindowContext,
                MkAction::WindowActivate(MkWindowPayload {
                    matcher,
                    wait: None,
                }),
                0,
                1,
            ));
            active = identity;
        }
        output.push(step);
    }
    *steps = output;
}

fn assign_stable_review_ids(steps: &mut [PlannedStep]) {
    let mut used = HashSet::new();
    for step in steps {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        step.source.first.hash(&mut hasher);
        step.source.last.hash(&mut hasher);
        step.provenance.hash(&mut hasher);
        let mut id = hasher.finish().max(1);
        while !used.insert(id) {
            id = id.wrapping_add(1).max(1);
        }
        step.review_id = ReviewStepId(id);
    }
}

pub fn materialize_plan(plan: &RecordingPlan, mut next_id: u64) -> Vec<MkStep> {
    plan.steps
        .iter()
        .map(|step| {
            next_id += 1;
            MkStep {
                id: next_id,
                enabled: step.enabled,
                breakpoint: step.breakpoint,
                repeat: step.repeat,
                delay_after_ms: step.delay_after_ms,
                on_error: step.on_error.clone(),
                metadata: step.metadata.clone(),
                action: step.action.clone(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeTranslator(HashMap<u32, KeyTranslation>);
    impl KeyboardTranslator for FakeTranslator {
        fn translate(&mut self, request: &KeyboardTranslationRequest) -> KeyTranslation {
            self.0
                .get(&request.vk)
                .cloned()
                .unwrap_or(KeyTranslation::None)
        }
    }
    fn key(index: u64, down: bool, vk: u32) -> RecordedStep {
        RecordedStep {
            timestamp_us: index * 100_000,
            delay_after_ms: 100,
            action: RecordedAction::Key {
                down,
                vk,
                scan_code: 0,
                extended: matches!(vk, 0xA3 | 0xA5),
                flags: 0,
                extra_info: 0,
            },
            context: None,
        }
    }
    fn plan(input: Vec<RecordedStep>, translations: &[(u32, &str)]) -> RecordingPlan {
        let mut translator = FakeTranslator(
            translations
                .iter()
                .map(|(vk, text)| (*vk, KeyTranslation::Text((*text).into())))
                .collect(),
        );
        let enriched = enrich_keyboard(&input, &mut translator);
        build_recording_plan(&enriched, &MkRecorderSettings::default())
    }

    #[test]
    fn malformed_transitions_remain_literal_and_never_panic() {
        let result = plan(vec![key(1, false, 0x41), key(2, true, 0x42)], &[]);
        assert!(matches!(result.steps[0].action, MkAction::KeyUp(_)));
        assert!(matches!(result.steps[1].action, MkAction::KeyDown(_)));
    }

    #[test]
    fn tap_repeat_and_long_hold_are_distinct() {
        let mut input = vec![
            key(1, true, 0x41),
            key(2, true, 0x41),
            key(3, false, 0x41),
            key(4, true, 0x42),
            key(11, false, 0x42),
        ];
        input[3].delay_after_ms = 700;
        let result = plan(input, &[]);
        assert!(matches!(result.steps[0].action, MkAction::KeyDown(_)));
        assert_eq!(result.steps[0].delay_after_ms, 200);
        assert!(matches!(result.steps[1].action, MkAction::KeyUp(_)));
        assert!(matches!(result.steps[2].action, MkAction::KeyDown(_)));
        assert_eq!(result.steps[2].delay_after_ms, 700);
        assert!(matches!(result.steps[3].action, MkAction::KeyUp(_)));
    }

    #[test]
    fn clear_arbitrary_chord_uses_release_delay() {
        let mut input = vec![
            key(1, true, 0x11),
            key(2, true, 0x4b),
            key(3, true, 0x43),
            key(4, false, 0x43),
            key(5, false, 0x4b),
            key(6, false, 0x11),
        ];
        input[4].delay_after_ms = 340;
        let result = plan(input, &[]);
        assert_eq!(result.steps.len(), 1);
        assert!(matches!(&result.steps[0].action, MkAction::Hotkey(keys) if keys.len() == 3));
        assert_eq!(result.steps[0].delay_after_ms, 340);
        assert_eq!(
            result.steps[0].source,
            RecordingSourceSpan { first: 0, last: 5 }
        );
    }

    #[test]
    fn long_modified_hold_stays_explicit() {
        let mut input = vec![
            key(0, true, 0x11),
            key(1, true, 0x41),
            key(8, false, 0x41),
            key(9, false, 0x11),
        ];
        input[0].delay_after_ms = 100;
        input[1].delay_after_ms = 700;
        let result = plan(input, &[]);
        assert!(
            result
                .steps
                .iter()
                .any(|s| matches!(s.action, MkAction::KeyDown(MkKey::LeftControl)))
        );
        assert!(
            result
                .steps
                .iter()
                .any(|s| matches!(s.action, MkAction::KeyDown(MkKey::Character(_))))
        );
        assert!(
            !result
                .steps
                .iter()
                .any(|s| matches!(s.action, MkAction::Hotkey(_)))
        );
    }

    #[test]
    fn printable_run_folds_but_control_character_does_not() {
        let text = plan(
            vec![
                key(1, true, 0x41),
                key(2, false, 0x41),
                key(3, true, 0x42),
                key(4, false, 0x42),
            ],
            &[(0x41, "a"), (0x42, "b")],
        );
        assert!(matches!(&text.steps[0].action, MkAction::Text(payload) if payload.text == "ab"));
        let control = plan(
            vec![
                key(1, true, 0x41),
                key(2, false, 0x41),
                key(3, true, 0x42),
                key(4, false, 0x42),
            ],
            &[(0x41, "\n"), (0x42, "b")],
        );
        assert!(
            !control
                .steps
                .iter()
                .any(|s| matches!(s.action, MkAction::Text(_)))
        );
    }

    #[test]
    fn delay_rounding_saturates() {
        let mut steps = vec![make_step(
            0,
            0,
            RecordingProvenance::Literal,
            MkAction::KeyPress(MkKey::Enter),
            u64::MAX,
            1,
        )];
        cleanup_delays(&mut steps, 1, 10);
        assert_eq!(steps[0].delay_after_ms, (u64::MAX / 10) * 10);
        cleanup_delays(&mut steps, 1, 0);
    }

    #[test]
    fn delay_cleanup_rounds_before_applying_the_noise_threshold() {
        let mut steps = [37, 42, 96, 104, 147]
            .into_iter()
            .enumerate()
            .map(|(index, delay)| {
                make_step(
                    index,
                    index,
                    RecordingProvenance::Literal,
                    MkAction::KeyPress(MkKey::Enter),
                    delay,
                    1,
                )
            })
            .collect::<Vec<_>>();
        cleanup_delays(&mut steps, 100, 10);
        assert_eq!(
            steps
                .iter()
                .map(|step| step.delay_after_ms)
                .collect::<Vec<_>>(),
            vec![0, 0, 100, 100, 150]
        );

        let mut zero_threshold = vec![make_step(
            0,
            0,
            RecordingProvenance::Literal,
            MkAction::KeyPress(MkKey::Enter),
            4,
            1,
        )];
        cleanup_delays(&mut zero_threshold, 0, 10);
        assert_eq!(zero_threshold[0].delay_after_ms, 0);
    }

    #[test]
    fn dead_key_blocks_the_following_composed_text_candidate() {
        let input = vec![
            key(1, true, 0xDE),
            key(2, false, 0xDE),
            key(3, true, 0x41),
            key(4, false, 0x41),
            key(5, true, 0x42),
            key(6, false, 0x42),
        ];
        struct DeadThenText;
        impl KeyboardTranslator for DeadThenText {
            fn translate(&mut self, request: &KeyboardTranslationRequest) -> KeyTranslation {
                match request.vk {
                    0xDE => KeyTranslation::DeadKey,
                    0x41 => KeyTranslation::Text("á".into()),
                    0x42 => KeyTranslation::Text("b".into()),
                    _ => KeyTranslation::None,
                }
            }
        }
        let mut translator = DeadThenText;
        let enriched = enrich_keyboard(&input, &mut translator);
        let result = build_recording_plan(&enriched, &MkRecorderSettings::default());
        assert!(
            !result
                .steps
                .iter()
                .any(|step| matches!(step.action, MkAction::Text(_)))
        );
    }

    #[test]
    fn matcher_identity_is_used_when_native_window_id_is_absent() {
        let context = |title: &str| EventContext {
            foreground: WindowContext {
                title: title.into(),
                ..Default::default()
            },
            window_under_point: None,
            keyboard_layout: None,
        };
        let mut a = key(1, true, 0x41);
        let mut b = key(2, true, 0x42);
        a.context = Some(context("Editor"));
        b.context = Some(context("Editor"));
        let enriched = vec![
            EnrichedRecordedStep {
                source_index: 0,
                literal: a,
                key: None,
                translation: KeyTranslation::None,
            },
            EnrichedRecordedStep {
                source_index: 1,
                literal: b,
                key: None,
                translation: KeyTranslation::None,
            },
        ];
        assert!(same_text_target(&enriched, 0, 1));
        let mut different = enriched.clone();
        different[1].literal.context = Some(context("Other"));
        assert!(!same_text_target(&different, 0, 1));
    }

    #[test]
    fn short_held_modifier_across_keyboard_taps_becomes_separate_hotkeys() {
        let mut input = vec![
            key(1, true, 0xA2),
            key(2, true, 0x41),
            key(3, false, 0x41),
            key(4, true, 0x42),
            key(5, false, 0x42),
            key(6, false, 0xA2),
        ];
        for (index, step) in input.iter_mut().enumerate() {
            step.timestamp_us = index as u64 * 50_000;
        }
        let result = plan(input, &[]);
        assert_eq!(result.steps.len(), 2);
        assert!(
            matches!(&result.steps[0].action, MkAction::Hotkey(keys) if keys == &vec![MkKey::LeftControl, MkKey::Character("A".into())])
        );
        assert!(
            matches!(&result.steps[1].action, MkAction::Hotkey(keys) if keys == &vec![MkKey::LeftControl, MkKey::Character("B".into())])
        );
    }

    #[test]
    fn long_modifier_hold_preserves_explicit_lead_and_trailing_state() {
        let mut input = vec![
            key(0, true, 0xA2),
            key(7, true, 0x43),
            key(8, false, 0x43),
            key(9, true, 0x56),
            key(10, false, 0x56),
            key(12, false, 0xA2),
        ];
        input[0].timestamp_us = 0;
        input[1].timestamp_us = 700_000;
        input[2].timestamp_us = 750_000;
        input[3].timestamp_us = 800_000;
        input[4].timestamp_us = 850_000;
        input[5].timestamp_us = 1_200_000;
        let result = plan(input, &[]);
        assert_eq!(result.steps.len(), 4);
        assert!(matches!(
            &result.steps[0].action,
            MkAction::KeyDown(MkKey::LeftControl)
        ));
        assert!(matches!(
            &result.steps[1].action,
            MkAction::KeyPress(MkKey::Character(value)) if value == "C"
        ));
        assert!(matches!(
            &result.steps[2].action,
            MkAction::KeyPress(MkKey::Character(value)) if value == "V"
        ));
        assert!(matches!(
            &result.steps[3].action,
            MkAction::KeyUp(MkKey::LeftControl)
        ));
    }

    #[test]
    fn modifier_spanning_keys_and_mouse_click_remains_explicit() {
        let mut input = vec![
            key(0, true, 0xA2),
            key(1, true, 0x43),
            key(2, false, 0x43),
            key(3, true, 0x56),
            key(4, false, 0x56),
            RecordedStep {
                timestamp_us: 500_000,
                delay_after_ms: 100,
                action: RecordedAction::Click {
                    button: super::super::MouseButton::Left,
                    x: 10,
                    y: 20,
                    count: 1,
                },
                context: None,
            },
            key(6, false, 0xA2),
        ];
        for (index, step) in input.iter_mut().enumerate() {
            step.timestamp_us = index as u64 * 50_000;
        }
        let result = plan(input, &[]);
        assert_eq!(result.steps.len(), 5);
        assert!(matches!(
            result.steps[0].action,
            MkAction::KeyDown(MkKey::LeftControl)
        ));
        assert!(matches!(
            result.steps[1].action,
            MkAction::KeyPress(MkKey::Character(ref value)) if value == "C"
        ));
        assert!(matches!(
            result.steps[2].action,
            MkAction::KeyPress(MkKey::Character(ref value)) if value == "V"
        ));
        assert!(matches!(result.steps[3].action, MkAction::MouseClick(_)));
        assert!(matches!(
            result.steps[4].action,
            MkAction::KeyUp(MkKey::LeftControl)
        ));
    }

    #[test]
    fn held_modifier_shortcuts_use_each_primary_keys_window_context() {
        let context = |process: &str, root| EventContext {
            foreground: WindowContext {
                executable: process.into(),
                native_root_id: Some(root),
                ..Default::default()
            },
            window_under_point: None,
            keyboard_layout: None,
        };
        let mut input = vec![
            key(0, true, 0xA2),
            key(1, true, 0x43),
            key(2, false, 0x43),
            key(3, true, 0x56),
            key(4, false, 0x56),
            key(5, false, 0xA2),
        ];
        for step in &mut input[..3] {
            step.context = Some(context("editor-a.exe", 1));
        }
        for step in &mut input[3..] {
            step.context = Some(context("editor-b.exe", 2));
        }

        let result = plan(input, &[]);
        assert_eq!(result.steps.len(), 4);
        let activation_process = |step: &PlannedStep| match &step.action {
            MkAction::WindowActivate(payload) => payload.matcher.process.clone(),
            _ => None,
        };
        assert_eq!(
            activation_process(&result.steps[0]).as_deref(),
            Some("editor-a.exe")
        );
        assert!(
            matches!(&result.steps[1].action, MkAction::Hotkey(keys) if keys.last() == Some(&MkKey::Character("C".into())))
        );
        assert_eq!(
            activation_process(&result.steps[2]).as_deref(),
            Some("editor-b.exe")
        );
        assert!(
            matches!(&result.steps[3].action, MkAction::Hotkey(keys) if keys.last() == Some(&MkKey::Character("V".into())))
        );
    }

    #[test]
    fn held_shift_text_does_not_fold_across_primary_key_window_contexts() {
        let context = |process: &str, root| EventContext {
            foreground: WindowContext {
                executable: process.into(),
                native_root_id: Some(root),
                ..Default::default()
            },
            window_under_point: None,
            keyboard_layout: None,
        };
        let mut input = vec![
            key(0, true, 0xA0),
            key(1, true, 0x41),
            key(2, false, 0x41),
            key(3, true, 0x42),
            key(4, false, 0x42),
            key(5, false, 0xA0),
        ];
        for step in &mut input[..3] {
            step.context = Some(context("editor-a.exe", 1));
        }
        for step in &mut input[3..] {
            step.context = Some(context("editor-b.exe", 2));
        }

        let result = plan(input, &[(0x41, "A"), (0x42, "B")]);
        let actions = result
            .steps
            .iter()
            .map(|step| &step.action)
            .collect::<Vec<_>>();
        assert_eq!(actions.len(), 4);
        assert!(matches!(
            actions[0],
            MkAction::WindowActivate(payload)
                if payload.matcher.process.as_deref() == Some("editor-a.exe")
        ));
        assert!(matches!(actions[1], MkAction::Hotkey(_)));
        assert!(matches!(
            actions[2],
            MkAction::WindowActivate(payload)
                if payload.matcher.process.as_deref() == Some("editor-b.exe")
        ));
        assert!(matches!(actions[3], MkAction::Hotkey(_)));
    }

    #[test]
    fn translated_contiguous_taps_fold_to_text_but_singletons_do_not() {
        let result = plan(
            vec![
                key(1, true, 0x41),
                key(2, false, 0x41),
                key(3, true, 0x42),
                key(4, false, 0x42),
            ],
            &[(0x41, "a"), (0x42, "b")],
        );
        assert!(matches!(&result.steps[0].action, MkAction::Text(payload) if payload.text == "ab"));
        let one = plan(
            vec![key(1, true, 0x41), key(2, false, 0x41)],
            &[(0x41, "a")],
        );
        assert!(matches!(one.steps[0].action, MkAction::KeyPress(_)));
    }

    #[test]
    fn shift_and_altgr_text_fold_while_navigation_splits_runs() {
        struct StatefulText;
        impl KeyboardTranslator for StatefulText {
            fn translate(&mut self, request: &KeyboardTranslationRequest) -> KeyTranslation {
                match request.vk {
                    0x41 => KeyTranslation::Text(
                        if request.key_state[0x10] & 0x80 != 0 {
                            "A"
                        } else {
                            "a"
                        }
                        .into(),
                    ),
                    0x45 if request.key_state[0xA5] & 0x80 != 0 => KeyTranslation::Text("€".into()),
                    0x42 => KeyTranslation::Text("b".into()),
                    0x43 => KeyTranslation::Text("c".into()),
                    _ => KeyTranslation::None,
                }
            }
        }
        let input = vec![
            key(0, true, 0xA0),
            key(1, true, 0x41),
            key(2, false, 0x41),
            key(3, false, 0xA0),
            key(4, true, 0x42),
            key(5, false, 0x42),
            key(6, true, 0x08),
            key(7, false, 0x08),
            key(8, true, 0xA2),
            key(9, true, 0xA5),
            key(10, true, 0x45),
            key(11, false, 0x45),
            key(12, false, 0xA5),
            key(13, false, 0xA2),
            key(14, true, 0x43),
            key(15, false, 0x43),
        ];
        let mut translator = StatefulText;
        let result = build_recording_plan(
            &enrich_keyboard(&input, &mut translator),
            &MkRecorderSettings::default(),
        );
        assert!(matches!(
            &result.steps[0].action,
            MkAction::Text(payload) if payload.text == "Ab"
        ));
        assert!(matches!(
            result.steps[1].action,
            MkAction::KeyPress(MkKey::Backspace)
        ));
        assert!(matches!(
            &result.steps[2].action,
            MkAction::Text(payload) if payload.text == "€c"
        ));
    }

    #[test]
    fn text_runs_split_on_gap_and_foreground_identity() {
        let context = |root| EventContext {
            foreground: WindowContext {
                executable: "editor.exe".into(),
                native_root_id: Some(root),
                ..Default::default()
            },
            window_under_point: None,
            keyboard_layout: None,
        };
        let mut input = vec![
            key(0, true, 0x41),
            key(1, false, 0x41),
            key(2, true, 0x42),
            key(3, false, 0x42),
            key(4, true, 0x41),
            key(5, false, 0x41),
            key(6, true, 0x42),
            key(7, false, 0x42),
        ];
        for step in &mut input[..4] {
            step.context = Some(context(1));
        }
        for step in &mut input[4..] {
            step.context = Some(context(2));
        }
        input[1].delay_after_ms = 2_000;
        let result = plan(input, &[(0x41, "a"), (0x42, "b")]);
        let texts = result
            .steps
            .iter()
            .filter_map(|step| match &step.action {
                MkAction::Text(payload) => Some(payload.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(texts, ["ab"]);
        assert_eq!(
            result
                .steps
                .iter()
                .filter(|step| matches!(step.action, MkAction::KeyPress(_)))
                .count(),
            2
        );
    }

    #[test]
    fn own_process_context_never_authors_window_activation() {
        let mut input = vec![key(0, true, 0x41), key(1, false, 0x41)];
        for step in &mut input {
            step.context = Some(EventContext {
                foreground: WindowContext {
                    executable: "multi_launcher.exe".into(),
                    title: "Recorder".into(),
                    process_id: Some(std::process::id()),
                    native_root_id: Some(77),
                    ..Default::default()
                },
                window_under_point: None,
                keyboard_layout: None,
            });
        }
        let result = plan(input, &[]);
        assert!(
            !result
                .steps
                .iter()
                .any(|step| matches!(step.action, MkAction::WindowActivate(_)))
        );
    }

    #[test]
    fn mouse_cleanup_removes_only_an_exact_instantaneous_pre_click_move() {
        let target = MkCoordinateTarget::Screen {
            point: MkPoint { x: 10, y: 20 },
        };
        let move_step = |source, duration_ms, delay_after_ms, target| {
            make_step(
                source,
                source,
                RecordingProvenance::Literal,
                MkAction::MouseMove(MkMouseMovePayload {
                    target,
                    duration_ms,
                }),
                delay_after_ms,
                1,
            )
        };
        let click_step = |source| {
            make_step(
                source,
                source,
                RecordingProvenance::Literal,
                MkAction::MouseClick(MkMousePayload {
                    target: target.clone(),
                    button: MkMouseButton::Left,
                    clicks: 1,
                }),
                0,
                1,
            )
        };

        let mut exact = vec![move_step(0, 0, 0, target.clone()), click_step(1)];
        cleanup_mouse(&mut exact);
        assert_eq!(exact.len(), 1);
        assert_eq!(exact[0].source, RecordingSourceSpan { first: 0, last: 1 });

        for movement in [
            move_step(0, 1, 0, target.clone()),
            move_step(0, 0, 1, target.clone()),
            move_step(
                0,
                0,
                0,
                MkCoordinateTarget::Screen {
                    point: MkPoint { x: 11, y: 20 },
                },
            ),
        ] {
            let mut plan = vec![movement, click_step(1)];
            cleanup_mouse(&mut plan);
            assert_eq!(plan.len(), 2);
        }
    }

    #[test]
    fn materialization_is_the_only_permanent_id_allocator() {
        let result = plan(vec![key(1, true, 0x41), key(2, false, 0x41)], &[]);
        let rebuilt = plan(vec![key(1, true, 0x41), key(2, false, 0x41)], &[]);
        assert_ne!(result.steps[0].review_id, ReviewStepId(0));
        assert_eq!(result.steps[0].review_id, rebuilt.steps[0].review_id);
        assert_eq!(materialize_plan(&result, 40)[0].id, 41);
    }
}
