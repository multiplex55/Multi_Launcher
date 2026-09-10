//! Pure discovery and deterministic application of reviewable recorder suggestions.

use super::{
    ClipboardObservation, MkAction, MkKey, MkProcessPayload, MkRecorderSettings, MkTextMode,
    MkTextPayload, MkWaitOptions, MkWindowPayload, ObservationBaseline, PlannedStep,
    ProcessIdentity, RecordingPlan, RecordingProvenance, RecordingSourceSpan, ReviewStepId,
    WindowObservation, WindowObservationKind,
};
use std::{
    collections::{HashMap, HashSet},
    fmt,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecordingSuggestionId(pub u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionConfidence {
    High,
    Medium,
    Low,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecordingSuggestionKind {
    RepeatedClick,
    ApplicationLaunch,
    NewDialog,
    FreezePaste,
}

#[derive(Clone, PartialEq)]
pub struct RecordingSuggestion {
    pub id: RecordingSuggestionId,
    pub kind: RecordingSuggestionKind,
    pub confidence: SuggestionConfidence,
    pub source_span: RecordingSourceSpan,
    pub description: String,
    pub rationale: String,
    pub enabled_by_default: bool,
    pub replacement: Vec<PlannedStep>,
}
impl fmt::Debug for RecordingSuggestion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_struct("RecordingSuggestion");
        debug
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("confidence", &self.confidence)
            .field("source_span", &self.source_span)
            .field("description", &self.description)
            .field("rationale", &self.rationale)
            .field("enabled_by_default", &self.enabled_by_default);
        if self.kind == RecordingSuggestionKind::FreezePaste {
            debug.field("replacement", &"<redacted clipboard replacement>");
        } else {
            debug.field("replacement", &self.replacement);
        }
        debug.finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordingNote {
    Marker { timestamp_us: u64 },
    Annotation { timestamp_us: u64, text: String },
}

pub fn discover_suggestions(
    plan: &RecordingPlan,
    settings: &MkRecorderSettings,
    baseline: &ObservationBaseline,
    windows: &[WindowObservation],
    clipboards: &[ClipboardObservation],
    source_times: &[(usize, u64)],
) -> Vec<RecordingSuggestion> {
    let mut out = Vec::new();
    if settings.smart_repeated_click_cleanup {
        out.extend(repeated_click_suggestions(plan, settings));
    }
    if settings.detect_application_launches {
        out.extend(window_suggestions(plan, baseline, windows, source_times));
    }
    if settings.capture_text_paste_for_freeze_suggestion {
        out.extend(freeze_paste_suggestions(plan, clipboards, source_times));
    }
    out.sort_by_key(|s| (s.source_span.first, s.source_span.last, s.id.0));
    out
}

fn repeated_click_suggestions(
    plan: &RecordingPlan,
    settings: &MkRecorderSettings,
) -> Vec<RecordingSuggestion> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < plan.steps.len() {
        let MkAction::MouseClick(first_click) = &plan.steps[i].action else {
            i += 1;
            continue;
        };
        let mut end = i + 1;
        let mut intervals = Vec::new();
        while end < plan.steps.len() {
            let previous = &plan.steps[end - 1];
            let MkAction::MouseClick(click) = &plan.steps[end].action else {
                break;
            };
            if click != first_click || previous.delay_after_ms == 0 {
                break;
            }
            intervals.push(previous.delay_after_ms);
            end += 1;
        }
        let count = end - i;
        if count >= settings.repeated_click_minimum as usize && intervals.len() + 1 == count {
            let min = *intervals.iter().min().unwrap_or(&0);
            let max = *intervals.iter().max().unwrap_or(&0);
            if max.saturating_sub(min) <= settings.repeated_click_interval_tolerance_ms {
                let average = intervals.iter().sum::<u64>() / intervals.len() as u64;
                let span = RecordingSourceSpan {
                    first: plan.steps[i].source.first,
                    last: plan.steps[end - 1].source.last,
                };
                let mut replacement = plan.steps[i].clone();
                replacement.source = span;
                replacement.provenance = RecordingProvenance::MouseCleanup;
                replacement.repeat = count as u32;
                replacement.delay_after_ms = average;
                out.push(make_suggestion(
                    RecordingSuggestionKind::RepeatedClick,
                    SuggestionConfidence::Medium,
                    span,
                    format!("Repeat {count} identical clicks"),
                    format!("{count} clicks at the same target with ~{average} ms spacing"),
                    false,
                    vec![replacement],
                ));
            }
        }
        i = end.max(i + 1);
    }
    out
}

fn window_suggestions(
    plan: &RecordingPlan,
    baseline: &ObservationBaseline,
    observations: &[WindowObservation],
    source_times: &[(usize, u64)],
) -> Vec<RecordingSuggestion> {
    let mut grouped: HashMap<(usize, u32, u64), Vec<&WindowObservation>> = HashMap::new();
    for observation in observations {
        if let Some(root) = observation.window.native_root_id {
            grouped
                .entry((
                    root,
                    observation.window.process_id.unwrap_or(0),
                    observation.window.process_started_at.unwrap_or(0),
                ))
                .or_default()
                .push(observation);
        }
    }
    let mut shown_epochs = Vec::new();
    for mut group in grouped.into_values() {
        group.sort_by_key(|observation| observation.timestamp_us);
        for (index, shown) in group
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, observation)| {
                observation.kind == WindowObservationKind::Shown && observation.visible_top_level
            })
        {
            let next_show = group[index + 1..]
                .iter()
                .find(|observation| observation.kind == WindowObservationKind::Shown)
                .map(|observation| observation.timestamp_us)
                .unwrap_or(u64::MAX);
            let promptly_foregrounded = group[index + 1..].iter().any(|observation| {
                observation.kind == WindowObservationKind::Foreground
                    && observation.timestamp_us < next_show
                    && observation.timestamp_us.saturating_sub(shown.timestamp_us) <= 2_000_000
            });
            if promptly_foregrounded {
                shown_epochs.push((shown, next_show));
            }
        }
    }
    let mut out = Vec::new();
    for (shown, next_show) in shown_epochs {
        let Some(matcher) = shown.window.matcher() else {
            continue;
        };
        let Some(pid) = shown.window.process_id else {
            continue;
        };
        if pid == std::process::id() {
            continue;
        }
        let identity = ProcessIdentity {
            pid,
            started_at: shown.window.process_started_at.unwrap_or(0),
        };
        let existing = baseline.processes.contains(&identity)
            || (identity.started_at == 0 && baseline.processes.iter().any(|p| p.pid == pid));
        let existing_window = shown
            .window
            .native_root_id
            .is_some_and(|root| baseline.top_level_windows.contains(&(root, identity)));
        let hint = if existing && !existing_window {
            source_times
                .iter()
                .filter(|(_, timestamp)| *timestamp >= shown.timestamp_us && *timestamp < next_show)
                .min_by_key(|(_, timestamp)| timestamp.saturating_sub(shown.timestamp_us))
                .filter(|(_, timestamp)| timestamp.saturating_sub(shown.timestamp_us) <= 5_000_000)
                .map(|(source, _)| *source)
        } else {
            source_times
                .iter()
                .filter(|(_, timestamp)| *timestamp <= shown.timestamp_us)
                .min_by_key(|(_, timestamp)| shown.timestamp_us.saturating_sub(*timestamp))
                .map(|(source, _)| *source)
        };
        let Some(source) = hint else { continue };
        let span = source_span_near(plan, source);
        let wait = PlannedStep {
            review_id: ReviewStepId(0),
            source: span,
            provenance: RecordingProvenance::WindowContext,
            action: MkAction::WindowWait(MkWindowPayload {
                matcher: matcher.clone(),
                wait: Some(MkWaitOptions {
                    timeout_ms: 10_000,
                    poll_interval_ms: 50,
                }),
            }),
            delay_after_ms: 0,
            repeat: 1,
            metadata: Default::default(),
        };
        let activate = PlannedStep {
            review_id: ReviewStepId(0),
            source: span,
            provenance: RecordingProvenance::WindowContext,
            action: MkAction::WindowActivate(MkWindowPayload {
                matcher,
                wait: None,
            }),
            delay_after_ms: 0,
            repeat: 1,
            metadata: Default::default(),
        };
        if existing && !existing_window {
            let preserved: Vec<_> = plan
                .steps
                .iter()
                .filter(|step| spans_overlap(step.source, span))
                .cloned()
                .collect();
            let mut replacement = vec![wait, activate];
            replacement.extend(preserved);
            out.push(make_suggestion(
                RecordingSuggestionKind::NewDialog,
                SuggestionConfidence::Medium,
                span,
                "Wait for newly shown dialog".into(),
                "A new visible window appeared in an application that existed when recording began"
                    .into(),
                false,
                replacement,
            ));
        } else if !shown.window.process_path.trim().is_empty() {
            let Some(gesture_span) = launch_gesture_span(plan, source, &shown.window) else {
                continue;
            };
            let Some(gesture_time) = source_times
                .iter()
                .find(|(gesture_source, _)| *gesture_source == gesture_span.last)
                .map(|(_, timestamp)| *timestamp)
            else {
                continue;
            };
            if gesture_time > shown.timestamp_us
                || shown.timestamp_us.saturating_sub(gesture_time) > 3_000_000
            {
                continue;
            }
            let mut wait = wait;
            wait.source = gesture_span;
            let mut activate = activate;
            activate.source = gesture_span;
            let process = PlannedStep {
                review_id: ReviewStepId(0),
                source: gesture_span,
                provenance: RecordingProvenance::WindowContext,
                action: MkAction::Process(MkProcessPayload {
                    program: shown.window.process_path.clone(),
                    arguments: Vec::new(),
                    working_directory: None,
                    wait: false,
                }),
                delay_after_ms: 0,
                repeat: 1,
                metadata: Default::default(),
            };
            out.push(make_suggestion(
                RecordingSuggestionKind::ApplicationLaunch,
                SuggestionConfidence::High,
                gesture_span,
                format!("Replace launch gesture with {}", shown.window.executable),
                "A matching Run/Start gesture created a new process whose visible window became foreground"
                    .into(),
                true,
                vec![process, wait, activate],
            ));
        }
    }
    out
}

fn launch_gesture_span(
    plan: &RecordingPlan,
    source: usize,
    window: &super::WindowContext,
) -> Option<RecordingSourceSpan> {
    let nearest = plan
        .steps
        .iter()
        .enumerate()
        .min_by_key(|(_, step)| {
            if source < step.source.first {
                step.source.first - source
            } else {
                source.saturating_sub(step.source.last)
            }
        })?
        .0;
    let enter_index = (0..=nearest)
        .rev()
        .find(|index| matches!(plan.steps[*index].action, MkAction::KeyPress(MkKey::Enter)))?;
    let enter = &plan.steps[enter_index];
    let text_index = previous_non_window_step(plan, enter_index)?;
    let text = match &plan.steps[text_index].action {
        MkAction::Text(payload) => payload.text.trim().trim_matches('"'),
        _ => return None,
    };
    let executable = window.executable.trim();
    let stem = std::path::Path::new(executable)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(executable);
    let path = window.process_path.trim().trim_matches('"');
    if !text.eq_ignore_ascii_case(executable)
        && !text.eq_ignore_ascii_case(stem)
        && !text.eq_ignore_ascii_case(path)
    {
        return None;
    }
    let launcher_index = previous_non_window_step(plan, text_index)?;
    let opens_launcher = match &plan.steps[launcher_index].action {
        MkAction::KeyPress(MkKey::Meta | MkKey::LeftMeta | MkKey::RightMeta) => true,
        MkAction::Hotkey(keys) => {
            keys.iter()
                .any(|key| matches!(key, MkKey::Meta | MkKey::LeftMeta | MkKey::RightMeta))
                && keys.iter().any(
                    |key| matches!(key, MkKey::Character(value) if value.eq_ignore_ascii_case("r")),
                )
        }
        _ => false,
    };
    opens_launcher.then_some(RecordingSourceSpan {
        first: plan.steps[launcher_index].source.first,
        last: enter.source.last,
    })
}

fn previous_non_window_step(plan: &RecordingPlan, before: usize) -> Option<usize> {
    (0..before).rev().find(|index| {
        !matches!(
            plan.steps[*index].action,
            MkAction::WindowActivate(_) | MkAction::WindowWait(_)
        )
    })
}

fn source_span_near(plan: &RecordingPlan, source: usize) -> RecordingSourceSpan {
    plan.steps
        .iter()
        .min_by_key(|s| {
            if source < s.source.first {
                s.source.first - source
            } else {
                source.saturating_sub(s.source.last)
            }
        })
        .map(|s| s.source)
        .unwrap_or(RecordingSourceSpan {
            first: source,
            last: source,
        })
}

fn freeze_paste_suggestions(
    plan: &RecordingPlan,
    observations: &[ClipboardObservation],
    source_times: &[(usize, u64)],
) -> Vec<RecordingSuggestion> {
    let mut seen = HashSet::new();
    observations.iter().filter_map(|snapshot| {
        let source_index = source_times.iter().min_by_key(|(_, time)| time.abs_diff(snapshot.timestamp_us)).map(|(source, _)| *source)?;
        let step = plan.steps.iter().find(|step| step.source.first <= source_index
            && step.source.last >= source_index && is_paste(&step.action))?;
        if !seen.insert(step.review_id) { return None; }
        let mut replacement = step.clone();
        replacement.action = MkAction::Text(MkTextPayload { text: snapshot.text.expose().into(), mode: MkTextMode::Paste });
        Some(make_suggestion(
            RecordingSuggestionKind::FreezePaste, SuggestionConfidence::High, step.source,
            "Freeze pasted clipboard text".into(),
            format!("Captured {} clipboard characters near Ctrl+V; playback remains dynamic unless enabled", snapshot.text.len()),
            false, vec![replacement],
        ))
    }).collect()
}

fn is_paste(action: &MkAction) -> bool {
    let MkAction::Hotkey(keys) = action else {
        return false;
    };
    keys.last() == Some(&MkKey::Character("V".into()))
        && keys[..keys.len().saturating_sub(1)].iter().any(|key| {
            matches!(
                key,
                MkKey::Control | MkKey::LeftControl | MkKey::RightControl
            )
        })
}

fn make_suggestion(
    kind: RecordingSuggestionKind,
    confidence: SuggestionConfidence,
    span: RecordingSourceSpan,
    description: String,
    rationale: String,
    enabled_by_default: bool,
    mut replacement: Vec<PlannedStep>,
) -> RecordingSuggestion {
    let id = stable_id(kind, span, &description);
    for (offset, step) in replacement.iter_mut().enumerate() {
        step.review_id = ReviewStepId(id.0.wrapping_add(offset as u64).max(1));
    }
    RecordingSuggestion {
        id,
        kind,
        confidence,
        source_span: span,
        description,
        rationale,
        enabled_by_default,
        replacement,
    }
}

fn stable_id(
    kind: RecordingSuggestionKind,
    span: RecordingSourceSpan,
    description: &str,
) -> RecordingSuggestionId {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in [kind as u8]
        .into_iter()
        .chain(span.first.to_le_bytes())
        .chain(span.last.to_le_bytes())
        .chain(description.bytes())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    RecordingSuggestionId(hash.max(1))
}

/// Applies selected transformations in source order. Once a span is claimed,
/// later overlapping suggestions are ignored, making the result deterministic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuggestionApplicationError {
    ConflictingSpans(RecordingSuggestionId, RecordingSuggestionId),
    InvalidSpan(RecordingSuggestionId),
}

pub fn apply_suggestions(
    plan: &RecordingPlan,
    suggestions: &[RecordingSuggestion],
    enabled: &HashSet<RecordingSuggestionId>,
) -> Result<RecordingPlan, SuggestionApplicationError> {
    let mut selected: Vec<_> = suggestions
        .iter()
        .filter(|s| enabled.contains(&s.id))
        .collect();
    selected.sort_by_key(|s| (s.source_span.first, s.source_span.last, s.id.0));
    let mut accepted = Vec::new();
    for suggestion in selected {
        if suggestion.source_span.first > suggestion.source_span.last
            || !plan
                .steps
                .iter()
                .any(|step| spans_overlap(step.source, suggestion.source_span))
        {
            return Err(SuggestionApplicationError::InvalidSpan(suggestion.id));
        }
        if let Some(prior) = accepted
            .iter()
            .copied()
            .find(|prior: &&RecordingSuggestion| {
                spans_overlap(prior.source_span, suggestion.source_span)
            })
        {
            return Err(SuggestionApplicationError::ConflictingSpans(
                prior.id,
                suggestion.id,
            ));
        }
        accepted.push(suggestion);
    }
    let mut steps = Vec::new();
    let mut emitted = HashSet::new();
    for step in &plan.steps {
        if let Some((index, suggestion)) = accepted
            .iter()
            .enumerate()
            .find(|(_, s)| spans_overlap(step.source, s.source_span))
        {
            if emitted.insert(index) {
                steps.extend(suggestion.replacement.clone());
            }
        } else {
            steps.push(step.clone());
        }
    }
    Ok(RecordingPlan { steps })
}
fn spans_overlap(a: RecordingSourceSpan, b: RecordingSourceSpan) -> bool {
    a.first <= b.last && b.first <= a.last
}

pub fn apply_recording_notes(
    plan: &mut RecordingPlan,
    notes: &[RecordingNote],
    source_times: &[(usize, u64)],
) {
    for note in notes {
        let timestamp = match note {
            RecordingNote::Marker { timestamp_us }
            | RecordingNote::Annotation { timestamp_us, .. } => *timestamp_us,
        };
        let Some(source) = source_times
            .iter()
            .min_by_key(|(_, time)| time.abs_diff(timestamp))
            .map(|(source, _)| *source)
        else {
            continue;
        };
        let Some(step) = plan.steps.iter_mut().min_by_key(|step| {
            if source < step.source.first {
                step.source.first - source
            } else {
                source.saturating_sub(step.source.last)
            }
        }) else {
            continue;
        };
        match note {
            RecordingNote::Marker { .. } => step.metadata.bookmarked = true,
            RecordingNote::Annotation { text, .. } if !text.trim().is_empty() => {
                if !step.metadata.comment.is_empty() {
                    step.metadata.comment.push('\n');
                }
                step.metadata.comment.push_str(text.trim());
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        EventContext, KeyTranslation, KeyboardTranslationRequest, KeyboardTranslator,
        MkCoordinateTarget, MkMouseButton, MkMousePayload, MkPoint, RecordedAction, RecordedStep,
        WindowContext, build_recording_plan, enrich_keyboard,
    };
    use super::*;
    fn click(source: usize, delay: u64) -> PlannedStep {
        PlannedStep {
            review_id: ReviewStepId(source as u64 + 1),
            source: RecordingSourceSpan {
                first: source,
                last: source,
            },
            provenance: RecordingProvenance::Literal,
            action: MkAction::MouseClick(MkMousePayload {
                target: MkCoordinateTarget::Screen {
                    point: MkPoint { x: 4, y: 5 },
                },
                button: MkMouseButton::Left,
                clicks: 1,
            }),
            delay_after_ms: delay,
            repeat: 1,
            metadata: Default::default(),
        }
    }
    fn action(source: usize, action: MkAction) -> PlannedStep {
        let mut step = click(source, 0);
        step.action = action;
        step
    }
    struct TextTranslator;
    impl KeyboardTranslator for TextTranslator {
        fn translate(&mut self, request: &KeyboardTranslationRequest) -> KeyTranslation {
            match request.vk {
                0x4e => KeyTranslation::Text("n".into()),
                0x4f => KeyTranslation::Text("o".into()),
                0x54 => KeyTranslation::Text("t".into()),
                0x45 => KeyTranslation::Text("e".into()),
                _ => KeyTranslation::None,
            }
        }
    }
    fn literal_key(index: usize, vk: u32, down: bool, root: usize) -> RecordedStep {
        RecordedStep {
            timestamp_us: index as u64 * 100_000,
            delay_after_ms: 100,
            action: RecordedAction::Key {
                down,
                vk,
                scan_code: 0,
                extended: false,
                flags: 0,
                extra_info: 0,
            },
            context: Some(EventContext {
                foreground: WindowContext {
                    executable: "explorer.exe".into(),
                    title: format!("shell {root}"),
                    native_root_id: Some(root),
                    ..Default::default()
                },
                window_under_point: None,
                keyboard_layout: None,
            }),
        }
    }
    #[test]
    fn repeat_suggestion_is_stable_and_applies_repeat() {
        let plan = RecordingPlan {
            steps: vec![click(0, 500), click(1, 510), click(2, 490), click(3, 0)],
        };
        let settings = MkRecorderSettings::default();
        let a = repeated_click_suggestions(&plan, &settings);
        let b = repeated_click_suggestions(&plan, &settings);
        assert_eq!(a[0].id, b[0].id);
        let applied = apply_suggestions(&plan, &a, &HashSet::from([a[0].id])).unwrap();
        assert_eq!(applied.steps.len(), 1);
        assert_eq!(applied.steps[0].repeat, 4);
        assert_eq!(applied.steps[0].delay_after_ms, 500);
    }
    #[test]
    fn overlapping_suggestions_are_reported() {
        let plan = RecordingPlan {
            steps: vec![click(0, 0), click(1, 0)],
        };
        let a = make_suggestion(
            RecordingSuggestionKind::RepeatedClick,
            SuggestionConfidence::High,
            RecordingSourceSpan { first: 0, last: 1 },
            "a".into(),
            "a".into(),
            true,
            vec![click(0, 0)],
        );
        let b = make_suggestion(
            RecordingSuggestionKind::FreezePaste,
            SuggestionConfidence::High,
            RecordingSourceSpan { first: 1, last: 1 },
            "b".into(),
            "b".into(),
            true,
            vec![],
        );
        let result =
            apply_suggestions(&plan, &[b.clone(), a.clone()], &HashSet::from([a.id, b.id]));
        assert!(matches!(
            result,
            Err(SuggestionApplicationError::ConflictingSpans(_, _))
        ));
    }

    #[test]
    fn launch_suggestion_has_finite_wait_and_never_invents_arguments() {
        let context_action = || {
            MkAction::WindowActivate(MkWindowPayload {
                matcher: super::super::MkWindowMatcher {
                    title: Some("Run".into()),
                    title_regex: None,
                    process: Some("explorer.exe".into()),
                    class: None,
                },
                wait: None,
            })
        };
        let plan = RecordingPlan {
            steps: vec![
                action(0, context_action()),
                action(
                    1,
                    MkAction::Hotkey(vec![MkKey::LeftMeta, MkKey::Character("R".into())]),
                ),
                action(2, context_action()),
                action(
                    3,
                    MkAction::Text(MkTextPayload {
                        text: "note".into(),
                        mode: MkTextMode::Type,
                    }),
                ),
                action(4, context_action()),
                action(5, MkAction::KeyPress(MkKey::Enter)),
            ],
        };
        let window = super::super::WindowContext {
            executable: "note.exe".into(),
            process_path: r"C:\Tools\note.exe".into(),
            title: "Note".into(),
            class: "Editor".into(),
            native_root_id: Some(9),
            process_id: Some(44),
            process_started_at: Some(7),
            ..Default::default()
        };
        let observations = vec![
            WindowObservation {
                timestamp_us: 700,
                source_hint: None,
                kind: WindowObservationKind::Shown,
                window: window.clone(),
                visible_top_level: true,
            },
            WindowObservation {
                timestamp_us: 800,
                source_hint: None,
                kind: WindowObservationKind::Foreground,
                window: window.clone(),
                visible_top_level: true,
            },
        ];
        let found = window_suggestions(
            &plan,
            &ObservationBaseline::default(),
            &observations,
            &[(0, 100), (1, 200), (2, 300), (3, 400), (4, 500), (5, 600)],
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].confidence, SuggestionConfidence::High);
        assert!(found[0].enabled_by_default);
        assert_eq!(
            found[0].source_span,
            RecordingSourceSpan { first: 1, last: 5 }
        );
        let MkAction::Process(process) = &found[0].replacement[0].action else {
            panic!()
        };
        assert!(process.arguments.is_empty());
        let MkAction::WindowWait(wait) = &found[0].replacement[1].action else {
            panic!()
        };
        assert_eq!(wait.wait.as_ref().unwrap().timeout_ms, 10_000);
        assert!(
            window_suggestions(
                &plan,
                &ObservationBaseline::default(),
                &observations[..1],
                &[(0, 100), (1, 200), (2, 300), (3, 400), (4, 500), (5, 600)],
            )
            .is_empty()
        );
        let mut missing_path = observations.clone();
        for observation in &mut missing_path {
            observation.window.process_path.clear();
        }
        assert!(
            window_suggestions(
                &plan,
                &ObservationBaseline::default(),
                &missing_path,
                &[(0, 100), (1, 200), (2, 300), (3, 400), (4, 500), (5, 600)],
            )
            .is_empty()
        );
    }

    #[test]
    fn launch_gesture_survives_default_semantic_window_context_rows() {
        let mut literal = vec![
            literal_key(0, 0x5b, true, 1),
            literal_key(1, 0x5b, false, 1),
        ];
        for vk in [0x4e, 0x4f, 0x54, 0x45] {
            let index = literal.len();
            literal.push(literal_key(index, vk, true, 2));
            literal.push(literal_key(index + 1, vk, false, 2));
        }
        let index = literal.len();
        literal.push(literal_key(index, 0x0d, true, 2));
        literal.push(literal_key(index + 1, 0x0d, false, 2));
        let enriched = enrich_keyboard(&literal, &mut TextTranslator);
        let plan = build_recording_plan(&enriched, &MkRecorderSettings::default());
        assert!(
            plan.steps
                .iter()
                .any(|step| matches!(step.action, MkAction::WindowActivate(_)))
        );
        let target = WindowContext {
            executable: "note.exe".into(),
            process_path: r"C:\Tools\note.exe".into(),
            ..Default::default()
        };
        assert!(launch_gesture_span(&plan, index + 1, &target).is_some());
    }

    #[test]
    fn dialog_wait_and_activation_precede_preserved_interaction() {
        let plan = RecordingPlan {
            steps: vec![click(3, 0)],
        };
        let process = ProcessIdentity {
            pid: 44,
            started_at: 7,
        };
        let window = super::super::WindowContext {
            executable: "note.exe".into(),
            process_path: r"C:\Tools\note.exe".into(),
            title: "Open".into(),
            class: "Dialog".into(),
            native_root_id: Some(9),
            process_id: Some(process.pid),
            process_started_at: Some(process.started_at),
            ..Default::default()
        };
        let observations = vec![
            WindowObservation {
                timestamp_us: 100,
                source_hint: None,
                kind: WindowObservationKind::Shown,
                window: window.clone(),
                visible_top_level: true,
            },
            WindowObservation {
                timestamp_us: 110,
                source_hint: None,
                kind: WindowObservationKind::Foreground,
                window,
                visible_top_level: true,
            },
        ];
        let baseline = ObservationBaseline {
            processes: HashSet::from([process]),
            top_level_windows: HashSet::new(),
        };
        let found = window_suggestions(&plan, &baseline, &observations, &[(3, 105)]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, RecordingSuggestionKind::NewDialog);
        assert!(matches!(
            found[0].replacement[0].action,
            MkAction::WindowWait(_)
        ));
        assert!(matches!(
            found[0].replacement[1].action,
            MkAction::WindowActivate(_)
        ));
        assert!(matches!(
            found[0].replacement[2].action,
            MkAction::MouseClick(_)
        ));
    }

    #[test]
    fn reused_handle_epochs_do_not_both_claim_the_later_interaction() {
        let plan = RecordingPlan {
            steps: vec![click(3, 0)],
        };
        let process = ProcessIdentity {
            pid: 44,
            started_at: 7,
        };
        let window = |title: &str| super::super::WindowContext {
            executable: "note.exe".into(),
            process_path: r"C:\Tools\note.exe".into(),
            title: title.into(),
            class: "Dialog".into(),
            native_root_id: Some(9),
            process_id: Some(process.pid),
            process_started_at: Some(process.started_at),
            ..Default::default()
        };
        let observations = vec![
            WindowObservation {
                timestamp_us: 100,
                source_hint: None,
                kind: WindowObservationKind::Shown,
                window: window("Old"),
                visible_top_level: true,
            },
            WindowObservation {
                timestamp_us: 200,
                source_hint: None,
                kind: WindowObservationKind::Foreground,
                window: window("Old"),
                visible_top_level: true,
            },
            WindowObservation {
                timestamp_us: 10_000_000,
                source_hint: None,
                kind: WindowObservationKind::Shown,
                window: window("New"),
                visible_top_level: true,
            },
            WindowObservation {
                timestamp_us: 10_100_000,
                source_hint: None,
                kind: WindowObservationKind::Foreground,
                window: window("New"),
                visible_top_level: true,
            },
        ];
        let baseline = ObservationBaseline {
            processes: HashSet::from([process]),
            top_level_windows: HashSet::new(),
        };
        let found = window_suggestions(&plan, &baseline, &observations, &[(3, 10_050_000)]);
        assert_eq!(found.len(), 1);
        let MkAction::WindowWait(wait) = &found[0].replacement[0].action else {
            panic!()
        };
        assert_eq!(wait.matcher.title.as_deref(), Some("New"));
    }

    #[test]
    fn freeze_suggestion_is_opt_in_and_debug_redacts_text() {
        let mut step = click(2, 0);
        step.action = MkAction::Hotkey(vec![MkKey::LeftControl, MkKey::Character("V".into())]);
        let plan = RecordingPlan { steps: vec![step] };
        let snapshots = vec![ClipboardObservation {
            timestamp_us: 55,
            text: super::super::SensitiveClipboardText::new("top secret".into()),
        }];
        let found = freeze_paste_suggestions(&plan, &snapshots, &[(2, 55)]);
        assert_eq!(found.len(), 1);
        assert!(!found[0].enabled_by_default);
        assert!(!format!("{:?}", found[0]).contains("top secret"));
        assert!(matches!(plan.steps[0].action, MkAction::Hotkey(_)));
    }

    #[test]
    fn notes_append_in_order_without_overwriting_metadata() {
        let mut step = click(0, 0);
        step.metadata.comment = "existing".into();
        let mut plan = RecordingPlan { steps: vec![step] };
        apply_recording_notes(
            &mut plan,
            &[
                RecordingNote::Marker { timestamp_us: 10 },
                RecordingNote::Annotation {
                    timestamp_us: 11,
                    text: "first".into(),
                },
                RecordingNote::Annotation {
                    timestamp_us: 12,
                    text: "second".into(),
                },
            ],
            &[(0, 10)],
        );
        assert!(plan.steps[0].metadata.bookmarked);
        assert_eq!(plan.steps[0].metadata.comment, "existing\nfirst\nsecond");
    }
}
