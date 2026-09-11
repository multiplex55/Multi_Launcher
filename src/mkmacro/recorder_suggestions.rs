//! Pure discovery and deterministic application of reviewable recorder suggestions.

use super::{
    ClipboardObservation, MkAction, MkKey, MkProcessPayload, MkRecorderSettings, MkTextMode,
    MkTextPayload, MkWaitOptions, MkWindowPayload, ObservationBaseline, PlannedStep,
    ProcessIdentity, REPEATED_CLICK_MINIMUM_MIN, RecordingPlan, RecordingProvenance,
    RecordingSourceSpan, ReviewStepId, WindowObservation, WindowObservationKind,
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

/// Opaque transient suggestion payload. Its `Debug` representation never exposes
/// action contents because freeze-paste replacements may contain clipboard text.
#[derive(Clone, PartialEq)]
pub struct SuggestionReplacement {
    steps: Vec<PlannedStep>,
    frozen_clipboard_text: Option<super::SensitiveClipboardText>,
}
impl From<Vec<PlannedStep>> for SuggestionReplacement {
    fn from(value: Vec<PlannedStep>) -> Self {
        Self {
            steps: value,
            frozen_clipboard_text: None,
        }
    }
}
impl SuggestionReplacement {
    fn freeze_paste(step: PlannedStep, text: super::SensitiveClipboardText) -> Self {
        Self {
            steps: vec![step],
            frozen_clipboard_text: Some(text),
        }
    }

    fn materialize(&self) -> Vec<PlannedStep> {
        let mut steps = self.steps.clone();
        if let (Some(step), Some(text)) = (steps.first_mut(), &self.frozen_clipboard_text) {
            step.action = MkAction::Text(MkTextPayload {
                text: text.expose().into(),
                mode: MkTextMode::Paste,
            });
        }
        steps
    }

    pub(crate) fn clear_sensitive(&mut self) {
        self.frozen_clipboard_text = None;
    }
}
impl fmt::Debug for SuggestionReplacement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SuggestionReplacement")
            .field("step_count", &self.steps.len())
            .field("steps", &"<redacted transient actions>")
            .finish()
    }
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
    pub replacement: SuggestionReplacement,
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
        let minimum = settings
            .repeated_click_minimum
            .max(REPEATED_CLICK_MINIMUM_MIN) as usize;
        if count >= minimum && intervals.len() + 1 == count {
            let min = *intervals.iter().min().unwrap_or(&0);
            let max = *intervals.iter().max().unwrap_or(&0);
            if max.saturating_sub(min) <= settings.repeated_click_interval_tolerance_ms {
                let average = intervals.iter().sum::<u64>() / intervals.len() as u64;
                let span = RecordingSourceSpan {
                    first: plan.steps[i].association_source,
                    last: plan.steps[end - 1].association_source,
                };
                let mut repeated = plan.steps[i].clone();
                repeated.source = RecordingSourceSpan {
                    first: plan.steps[i].source.first,
                    last: plan.steps[end - 2].source.last,
                };
                repeated.provenance = RecordingProvenance::MouseCleanup;
                repeated.repeat = count as u32 - 1;
                repeated.delay_after_ms = average;
                let mut tail = plan.steps[end - 1].clone();
                tail.provenance = RecordingProvenance::MouseCleanup;
                out.push(make_suggestion(
                    RecordingSuggestionKind::RepeatedClick,
                    SuggestionConfidence::Medium,
                    span,
                    format!("Repeat {count} identical clicks"),
                    format!("{count} clicks at the same target with ~{average} ms spacing"),
                    false,
                    vec![repeated, tail],
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
            association_source: source,
            provenance: RecordingProvenance::WindowContext,
            action: MkAction::WindowWait(MkWindowPayload {
                matcher: matcher.clone(),
                wait: Some(MkWaitOptions {
                    timeout_ms: 10_000,
                    poll_interval_ms: 50,
                }),
            }),
            enabled: true,
            breakpoint: false,
            delay_after_ms: 0,
            repeat: 1,
            on_error: crate::mkmacro::MkErrorPolicy::Stop,
            metadata: Default::default(),
        };
        let activate = PlannedStep {
            review_id: ReviewStepId(0),
            source: span,
            association_source: source,
            provenance: RecordingProvenance::WindowContext,
            action: MkAction::WindowActivate(MkWindowPayload {
                matcher,
                wait: None,
            }),
            enabled: true,
            breakpoint: false,
            delay_after_ms: 0,
            repeat: 1,
            on_error: crate::mkmacro::MkErrorPolicy::Stop,
            metadata: Default::default(),
        };
        if existing && !existing_window {
            let preserved: Vec<_> = plan
                .steps
                .iter()
                .filter(|step| {
                    span.first <= step.association_source
                        && step.association_source <= span.last
                        && step.provenance != RecordingProvenance::WindowContext
                })
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
        } else if !existing && !shown.window.process_path.trim().is_empty() {
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
                association_source: source,
                provenance: RecordingProvenance::WindowContext,
                action: MkAction::Process(MkProcessPayload {
                    program: shown.window.process_path.clone(),
                    arguments: Vec::new(),
                    working_directory: None,
                    wait: false,
                }),
                enabled: true,
                breakpoint: false,
                delay_after_ms: 0,
                repeat: 1,
                on_error: crate::mkmacro::MkErrorPolicy::Stop,
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
        .min_by_key(|(_, step)| step.association_source.abs_diff(source))?
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
        first: plan.steps[launcher_index].association_source,
        last: enter.association_source,
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
        .min_by_key(|step| step.association_source.abs_diff(source))
        .map(|step| RecordingSourceSpan {
            first: step.association_source,
            last: step.association_source,
        })
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
        let step = plan.steps.iter().filter(|step| is_paste(&step.action))
            .min_by_key(|step| step.association_source.abs_diff(source_index))?;
        if !seen.insert(step.review_id) { return None; }
        let association_span = RecordingSourceSpan {
            first: step.association_source,
            last: step.association_source,
        };
        let mut suggestion = make_suggestion(
            RecordingSuggestionKind::FreezePaste, SuggestionConfidence::High, association_span,
            "Freeze pasted clipboard text".into(),
            format!("Captured {} clipboard characters near Ctrl+V; playback remains dynamic unless enabled", snapshot.text.len()),
            false, vec![step.clone()],
        );
        suggestion.replacement = SuggestionReplacement::freeze_paste(step.clone(), snapshot.text.clone());
        Some(suggestion)
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
        replacement: replacement.into(),
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
    let mut accepted: Vec<&RecordingSuggestion> = Vec::new();
    for suggestion in selected {
        if suggestion.source_span.first > suggestion.source_span.last
            || !plan
                .steps
                .iter()
                .any(|step| suggestion_claims_step(suggestion, step))
        {
            return Err(SuggestionApplicationError::InvalidSpan(suggestion.id));
        }
        if let Some(prior) = accepted.iter().find(|prior| {
            plan.steps.iter().any(|step| {
                suggestion_claims_step(prior, step) && suggestion_claims_step(suggestion, step)
            })
        }) {
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
            .find(|(_, s)| suggestion_claims_step(s, step))
        {
            if emitted.insert(index) {
                let mut replacement = suggestion.replacement.materialize();
                let separately_preserved: HashSet<_> =
                    if suggestion.kind == RecordingSuggestionKind::RepeatedClick {
                        replacement
                            .iter()
                            .skip(1)
                            .map(|step| step.association_source)
                            .collect()
                    } else {
                        HashSet::new()
                    };
                if let Some(target) = replacement.first_mut() {
                    let source_metadata: Vec<_> = plan
                        .steps
                        .iter()
                        .filter(|source| suggestion_claims_step(suggestion, source))
                        .filter(|source| !separately_preserved.contains(&source.association_source))
                        .map(|source| &source.metadata)
                        .collect();
                    target.metadata.bookmarked |=
                        source_metadata.iter().any(|metadata| metadata.bookmarked);
                    let mut existing_consumed = false;
                    for metadata in source_metadata {
                        let comment = metadata.comment.trim();
                        if comment.is_empty() {
                            continue;
                        }
                        if !existing_consumed && target.metadata.comment.trim() == comment {
                            existing_consumed = true;
                            continue;
                        }
                        if !target.metadata.comment.is_empty() {
                            target.metadata.comment.push('\n');
                        }
                        target.metadata.comment.push_str(comment);
                    }
                }
                steps.extend(replacement);
            }
        } else {
            steps.push(step.clone());
        }
    }
    Ok(RecordingPlan { steps })
}
fn suggestion_claims_step(suggestion: &RecordingSuggestion, step: &PlannedStep) -> bool {
    if suggestion.kind == RecordingSuggestionKind::FreezePaste
        && suggestion.replacement.frozen_clipboard_text.is_some()
    {
        suggestion
            .replacement
            .steps
            .iter()
            .any(|target| target.review_id == step.review_id)
    } else if suggestion.kind == RecordingSuggestionKind::RepeatedClick {
        suggestion.source_span.first <= step.association_source
            && step.association_source <= suggestion.source_span.last
            && matches!(step.action, MkAction::MouseClick(_))
    } else {
        suggestion.source_span.first <= step.association_source
            && step.association_source <= suggestion.source_span.last
    }
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
        let Some(step) = plan
            .steps
            .iter_mut()
            .min_by_key(|step| step.association_source.abs_diff(source))
        else {
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
    use super::super::executor::{RecordingWaiter, fake::FakeBackend};
    use super::super::{
        EventContext, ExecutionOptions, Executor, KeyTranslation, KeyboardTranslationRequest,
        KeyboardTranslator, MkCoordinateTarget, MkMacro, MkMouseButton, MkMousePayload, MkPlayback,
        MkPoint, RecordedAction, RecordedStep, RunControl, WindowContext, build_recording_plan,
        compile, enrich_keyboard, materialize_plan,
    };
    use super::*;
    use std::sync::Arc;
    fn click(source: usize, delay: u64) -> PlannedStep {
        PlannedStep {
            review_id: ReviewStepId(source as u64 + 1),
            source: RecordingSourceSpan {
                first: source,
                last: source,
            },
            association_source: source,
            provenance: RecordingProvenance::Literal,
            action: MkAction::MouseClick(MkMousePayload {
                target: MkCoordinateTarget::Screen {
                    point: MkPoint { x: 4, y: 5 },
                },
                button: MkMouseButton::Left,
                clicks: 1,
            }),
            enabled: true,
            breakpoint: false,
            delay_after_ms: delay,
            repeat: 1,
            on_error: crate::mkmacro::MkErrorPolicy::Stop,
            metadata: Default::default(),
        }
    }
    fn action(source: usize, action: MkAction) -> PlannedStep {
        let mut step = click(source, 0);
        step.action = action;
        step
    }

    #[test]
    fn replacement_preserves_bookmarks_and_ordered_comments_from_claimed_span() {
        let mut first = click(0, 100);
        first.metadata.comment = "first".into();
        let mut second = click(1, 0);
        second.metadata.comment = "second".into();
        second.metadata.bookmarked = true;
        let plan = RecordingPlan {
            steps: vec![first, second],
        };
        let suggestion = RecordingSuggestion {
            id: RecordingSuggestionId(77),
            kind: RecordingSuggestionKind::RepeatedClick,
            confidence: SuggestionConfidence::High,
            source_span: RecordingSourceSpan { first: 0, last: 1 },
            description: "combine".into(),
            rationale: "test".into(),
            enabled_by_default: true,
            replacement: vec![click(0, 0)].into(),
        };
        let applied = apply_suggestions(
            &plan,
            &[suggestion],
            &HashSet::from([RecordingSuggestionId(77)]),
        )
        .unwrap();
        assert_eq!(applied.steps.len(), 1);
        assert!(applied.steps[0].metadata.bookmarked);
        assert_eq!(applied.steps[0].metadata.comment, "first\nsecond");
    }

    #[test]
    fn suggestion_replacement_preserves_markers_and_ordered_annotations() {
        let mut first = click(0, 100);
        first.metadata.comment = "first".into();
        let mut second = click(1, 0);
        second.metadata.bookmarked = true;
        second.metadata.comment = "second".into();
        let plan = RecordingPlan {
            steps: vec![first, second],
        };
        let mut replacement = click(0, 100);
        replacement.metadata.comment = "generated".into();
        let suggestion = RecordingSuggestion {
            id: RecordingSuggestionId(77),
            kind: RecordingSuggestionKind::RepeatedClick,
            confidence: SuggestionConfidence::High,
            source_span: RecordingSourceSpan { first: 0, last: 1 },
            description: "replace".into(),
            rationale: "test".into(),
            enabled_by_default: true,
            replacement: vec![replacement].into(),
        };
        let applied = apply_suggestions(
            &plan,
            std::slice::from_ref(&suggestion),
            &HashSet::from([suggestion.id]),
        )
        .unwrap();
        assert!(applied.steps[0].metadata.bookmarked);
        assert_eq!(
            applied.steps[0].metadata.comment,
            "generated\nfirst\nsecond"
        );
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
                extended: matches!(vk, 0x5B | 0x5C),
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
        assert_eq!(applied.steps.len(), 2);
        assert_eq!(applied.steps[0].repeat, 3);
        assert_eq!(applied.steps[0].delay_after_ms, 500);
        assert_eq!(applied.steps[1].repeat, 1);
        assert_eq!(applied.steps[1].delay_after_ms, 0);
    }

    #[test]
    fn repeated_click_cleanup_default_contract_requires_three_clicks() {
        let settings = MkRecorderSettings::default();

        assert!(
            repeated_click_suggestions(
                &RecordingPlan {
                    steps: vec![click(0, 500), click(1, 0)],
                },
                &settings,
            )
            .is_empty()
        );
        assert_eq!(
            repeated_click_suggestions(
                &RecordingPlan {
                    steps: vec![click(0, 500), click(1, 500), click(2, 0)],
                },
                &settings,
            )
            .len(),
            1
        );
    }

    #[test]
    fn repeated_click_metadata_is_preserved_once_at_prefix_and_tail() {
        let mut first = click(0, 500);
        first.metadata.comment = "first".into();
        let mut middle = click(1, 500);
        middle.metadata.comment = "middle".into();
        middle.metadata.bookmarked = true;
        let mut last = click(2, 17);
        last.metadata.comment = "last".into();
        last.metadata.bookmarked = true;
        let plan = RecordingPlan {
            steps: vec![first, middle, last],
        };
        let suggestions = repeated_click_suggestions(&plan, &MkRecorderSettings::default());

        let applied =
            apply_suggestions(&plan, &suggestions, &HashSet::from([suggestions[0].id])).unwrap();

        assert_eq!(applied.steps.len(), 2);
        assert_eq!(applied.steps[0].metadata.comment, "first\nmiddle");
        assert!(applied.steps[0].metadata.bookmarked);
        assert_eq!(applied.steps[1].metadata.comment, "last");
        assert!(applied.steps[1].metadata.bookmarked);
        assert_eq!(
            applied
                .steps
                .iter()
                .flat_map(|step| step.metadata.comment.lines())
                .collect::<Vec<_>>(),
            ["first", "middle", "last"]
        );
    }

    #[test]
    fn repeat_inside_explicit_modifier_hold_preserves_modifier_boundaries() {
        let plan = RecordingPlan {
            steps: vec![
                action(0, MkAction::KeyDown(MkKey::LeftControl)),
                click(1, 500),
                click(2, 500),
                click(3, 0),
                action(4, MkAction::KeyUp(MkKey::LeftControl)),
            ],
        };
        let suggestions = repeated_click_suggestions(&plan, &MkRecorderSettings::default());
        assert_eq!(suggestions.len(), 1);
        let applied =
            apply_suggestions(&plan, &suggestions, &HashSet::from([suggestions[0].id])).unwrap();
        assert_eq!(applied.steps.len(), 4);
        assert!(matches!(
            applied.steps[0].action,
            MkAction::KeyDown(MkKey::LeftControl)
        ));
        assert!(matches!(applied.steps[1].action, MkAction::MouseClick(_)));
        assert_eq!(applied.steps[1].repeat, 2);
        assert!(matches!(applied.steps[2].action, MkAction::MouseClick(_)));
        assert_eq!(applied.steps[2].repeat, 1);
        assert!(matches!(
            applied.steps[3].action,
            MkAction::KeyUp(MkKey::LeftControl)
        ));
    }

    #[test]
    fn repeated_click_replacement_executes_exact_pacing_while_modifier_is_held() {
        let plan = RecordingPlan {
            steps: vec![
                action(0, MkAction::KeyDown(MkKey::LeftControl)),
                click(1, 500),
                click(2, 500),
                click(3, 37),
                action(4, MkAction::KeyUp(MkKey::LeftControl)),
            ],
        };
        let suggestions = repeated_click_suggestions(&plan, &MkRecorderSettings::default());
        let applied =
            apply_suggestions(&plan, &suggestions, &HashSet::from([suggestions[0].id])).unwrap();
        let macro_plan = compile(&MkMacro {
            signature: Default::default(),
            id: 1,
            name: "recorded clicks".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: MkPlayback::default(),
            steps: materialize_plan(&applied, 10),
        })
        .unwrap();
        let fake = Arc::new(FakeBackend::default());
        let waiter = Arc::new(RecordingWaiter::default());
        let control = Arc::new(RunControl::default());
        control.reset();

        Executor::with_waiter(fake.clone().backends(), control, waiter.clone())
            .execute(&macro_plan, ExecutionOptions::normal(), &|_| {})
            .unwrap();

        assert_eq!(
            waiter.sleeps(),
            [
                std::time::Duration::from_millis(500),
                std::time::Duration::from_millis(500),
                std::time::Duration::from_millis(37),
            ]
        );
        let events = fake.events();
        assert_eq!(
            events.first().map(String::as_str),
            Some("key_down:LeftControl")
        );
        assert_eq!(
            events.last().map(String::as_str),
            Some("key_up:LeftControl")
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| event.as_str() == "button_down:Left")
                .count(),
            3
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| event.as_str() == "button_up:Left")
                .count(),
            3
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| event.starts_with("key_up:"))
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["key_up:LeftControl"]
        );
    }

    #[test]
    fn repeat_preserves_the_window_activation_associated_with_first_click() {
        let mut activate = action(
            0,
            MkAction::WindowActivate(MkWindowPayload {
                matcher: super::super::MkWindowMatcher {
                    process: Some("app.exe".into()),
                    ..Default::default()
                },
                wait: None,
            }),
        );
        activate.provenance = RecordingProvenance::WindowContext;
        let plan = RecordingPlan {
            steps: vec![activate, click(0, 500), click(1, 500), click(2, 0)],
        };
        let suggestions = repeated_click_suggestions(&plan, &MkRecorderSettings::default());
        assert_eq!(suggestions.len(), 1);
        let applied =
            apply_suggestions(&plan, &suggestions, &HashSet::from([suggestions[0].id])).unwrap();
        assert_eq!(applied.steps.len(), 3);
        assert!(matches!(
            applied.steps[0].action,
            MkAction::WindowActivate(_)
        ));
        assert!(matches!(applied.steps[1].action, MkAction::MouseClick(_)));
        assert_eq!(applied.steps[1].repeat, 2);
        assert!(matches!(applied.steps[2].action, MkAction::MouseClick(_)));
        assert_eq!(applied.steps[2].repeat, 1);
    }

    #[test]
    fn repeat_suggestion_rejects_short_irregular_zero_gap_and_changed_targets() {
        let settings = MkRecorderSettings::default();
        let cases = [
            RecordingPlan {
                steps: vec![click(0, 500), click(1, 0)],
            },
            RecordingPlan {
                steps: vec![click(0, 500), click(1, 900), click(2, 0)],
            },
            RecordingPlan {
                steps: vec![click(0, 0), click(1, 500), click(2, 0)],
            },
            RecordingPlan {
                steps: {
                    let mut changed = click(2, 0);
                    let MkAction::MouseClick(payload) = &mut changed.action else {
                        unreachable!()
                    };
                    payload.target = MkCoordinateTarget::Screen {
                        point: MkPoint { x: 99, y: 5 },
                    };
                    vec![click(0, 500), click(1, 500), changed]
                },
            },
        ];
        for plan in cases {
            assert!(repeated_click_suggestions(&plan, &settings).is_empty());
        }
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
        let MkAction::Process(process) = &found[0].replacement.steps[0].action else {
            panic!()
        };
        assert!(process.arguments.is_empty());
        let MkAction::WindowWait(wait) = &found[0].replacement.steps[1].action else {
            panic!()
        };
        assert_eq!(wait.wait.as_ref().unwrap().timeout_ms, 10_000);
        let identity = ProcessIdentity {
            pid: 44,
            started_at: 7,
        };
        let existing = ObservationBaseline {
            processes: HashSet::from([identity]),
            top_level_windows: HashSet::from([(9, identity)]),
        };
        assert!(
            window_suggestions(
                &plan,
                &existing,
                &observations,
                &[(0, 100), (1, 200), (2, 300), (3, 400), (4, 500), (5, 600)],
            )
            .is_empty()
        );
        assert!(
            window_suggestions(
                &plan,
                &ObservationBaseline::default(),
                &observations[..1],
                &[(0, 100), (1, 200), (2, 300), (3, 400), (4, 500), (5, 600)],
            )
            .is_empty()
        );
        let mut late_window = observations.clone();
        late_window[0].timestamp_us = 4_000_000;
        late_window[1].timestamp_us = 4_100_000;
        assert!(
            window_suggestions(
                &plan,
                &ObservationBaseline::default(),
                &late_window,
                &[(0, 100), (1, 200), (2, 300), (3, 400), (4, 500), (5, 600)],
            )
            .is_empty(),
            "a process shown outside the launch gesture window is not inferred as a launch"
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
            found[0].replacement.steps[0].action,
            MkAction::WindowWait(_)
        ));
        assert!(matches!(
            found[0].replacement.steps[1].action,
            MkAction::WindowActivate(_)
        ));
        assert!(matches!(
            found[0].replacement.steps[2].action,
            MkAction::MouseClick(_)
        ));
    }

    #[test]
    fn dialog_suggestion_claims_only_the_associated_held_modifier_action() {
        let broad = RecordingSourceSpan { first: 0, last: 5 };
        let mut copy = action(
            1,
            MkAction::Hotkey(vec![MkKey::LeftControl, MkKey::Character("C".into())]),
        );
        copy.source = broad;
        let mut paste = action(
            3,
            MkAction::Hotkey(vec![MkKey::LeftControl, MkKey::Character("V".into())]),
        );
        paste.source = broad;
        let mut authored_activation = action(
            3,
            MkAction::WindowActivate(MkWindowPayload {
                matcher: super::super::MkWindowMatcher {
                    process: Some("note.exe".into()),
                    ..Default::default()
                },
                wait: None,
            }),
        );
        authored_activation.source = broad;
        authored_activation.provenance = RecordingProvenance::WindowContext;
        let plan = RecordingPlan {
            steps: vec![copy, authored_activation, paste],
        };
        let process = ProcessIdentity {
            pid: 44,
            started_at: 7,
        };
        let window = WindowContext {
            executable: "note.exe".into(),
            title: "Paste".into(),
            class: "Dialog".into(),
            native_root_id: Some(9),
            process_id: Some(process.pid),
            process_started_at: Some(process.started_at),
            ..Default::default()
        };
        let observations = vec![
            WindowObservation {
                timestamp_us: 300,
                source_hint: None,
                kind: WindowObservationKind::Shown,
                window: window.clone(),
                visible_top_level: true,
            },
            WindowObservation {
                timestamp_us: 310,
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
        let suggestions =
            window_suggestions(&plan, &baseline, &observations, &[(1, 100), (3, 305)]);
        assert_eq!(suggestions.len(), 1);
        assert_eq!(
            suggestions[0].source_span,
            RecordingSourceSpan { first: 3, last: 3 }
        );
        let applied =
            apply_suggestions(&plan, &suggestions, &HashSet::from([suggestions[0].id])).unwrap();
        assert_eq!(applied.steps.len(), 4);
        assert!(matches!(
            &applied.steps[0].action,
            MkAction::Hotkey(keys) if keys.last() == Some(&MkKey::Character("C".into()))
        ));
        assert!(matches!(applied.steps[1].action, MkAction::WindowWait(_)));
        assert!(matches!(
            applied.steps[2].action,
            MkAction::WindowActivate(_)
        ));
        assert!(matches!(
            &applied.steps[3].action,
            MkAction::Hotkey(keys) if keys.last() == Some(&MkKey::Character("V".into()))
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
        let MkAction::WindowWait(wait) = &found[0].replacement.steps[0].action else {
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
        assert!(!format!("{:?}", found[0].replacement).contains("top secret"));
        assert!(matches!(plan.steps[0].action, MkAction::Hotkey(_)));
        let mut cleared = found[0].replacement.clone();
        cleared.clear_sensitive();
        assert!(matches!(
            cleared.materialize()[0].action,
            MkAction::Hotkey(_)
        ));
    }

    #[test]
    fn freeze_paste_claims_only_the_paste_when_a_held_modifier_spans_copy_and_paste() {
        let literal = vec![
            literal_key(0, 0xA2, true, 1),
            literal_key(1, 0x43, true, 1),
            literal_key(2, 0x43, false, 1),
            literal_key(3, 0x56, true, 1),
            literal_key(4, 0x56, false, 1),
            literal_key(5, 0xA2, false, 1),
        ];
        let mut settings = MkRecorderSettings::default();
        settings.record_window_context = false;
        let plan = build_recording_plan(&enrich_keyboard(&literal, &mut TextTranslator), &settings);
        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.steps[0].source, plan.steps[1].source);
        let observations = [ClipboardObservation {
            timestamp_us: 400_000,
            text: super::super::SensitiveClipboardText::new("frozen value".into()),
        }];
        let source_times = literal
            .iter()
            .enumerate()
            .map(|(index, step)| (index, step.timestamp_us))
            .collect::<Vec<_>>();
        let suggestions = freeze_paste_suggestions(&plan, &observations, &source_times);
        assert_eq!(suggestions.len(), 1);

        let dynamic = apply_suggestions(&plan, &suggestions, &HashSet::new()).unwrap();
        assert_eq!(dynamic, plan);
        let frozen =
            apply_suggestions(&plan, &suggestions, &HashSet::from([suggestions[0].id])).unwrap();
        assert_eq!(frozen.steps.len(), 2);
        assert!(matches!(
            &frozen.steps[0].action,
            MkAction::Hotkey(keys)
                if keys.last() == Some(&MkKey::Character("C".into()))
        ));
        assert!(matches!(
            &frozen.steps[1].action,
            MkAction::Text(payload)
                if payload.text == "frozen value" && payload.mode == MkTextMode::Paste
        ));
    }

    #[test]
    fn repeated_pastes_under_one_modifier_have_distinct_suggestions() {
        let literal = vec![
            literal_key(0, 0xA2, true, 1),
            literal_key(1, 0x56, true, 1),
            literal_key(2, 0x56, false, 1),
            literal_key(3, 0x56, true, 1),
            literal_key(4, 0x56, false, 1),
            literal_key(5, 0xA2, false, 1),
        ];
        let mut settings = MkRecorderSettings::default();
        settings.record_window_context = false;
        let plan = build_recording_plan(&enrich_keyboard(&literal, &mut TextTranslator), &settings);
        assert_eq!(plan.steps.len(), 2);
        assert_ne!(
            plan.steps[0].association_source,
            plan.steps[1].association_source
        );
        let observations = [
            ClipboardObservation {
                timestamp_us: 100_000,
                text: super::super::SensitiveClipboardText::new("first".into()),
            },
            ClipboardObservation {
                timestamp_us: 300_000,
                text: super::super::SensitiveClipboardText::new("second".into()),
            },
        ];
        let source_times = literal
            .iter()
            .enumerate()
            .map(|(index, step)| (index, step.timestamp_us))
            .collect::<Vec<_>>();
        let suggestions = freeze_paste_suggestions(&plan, &observations, &source_times);
        assert_eq!(suggestions.len(), 2);
        assert_ne!(suggestions[0].id, suggestions[1].id);

        let applied =
            apply_suggestions(&plan, &suggestions, &HashSet::from([suggestions[1].id])).unwrap();
        assert!(matches!(applied.steps[0].action, MkAction::Hotkey(_)));
        assert!(matches!(
            &applied.steps[1].action,
            MkAction::Text(payload) if payload.text == "second"
        ));
    }

    #[test]
    fn notes_under_one_held_modifier_follow_action_associations() {
        let literal = vec![
            literal_key(0, 0xA2, true, 1),
            literal_key(1, 0x43, true, 1),
            literal_key(2, 0x43, false, 1),
            literal_key(3, 0x56, true, 1),
            literal_key(4, 0x56, false, 1),
            literal_key(5, 0xA2, false, 1),
        ];
        let mut settings = MkRecorderSettings::default();
        settings.record_window_context = false;
        let mut plan =
            build_recording_plan(&enrich_keyboard(&literal, &mut TextTranslator), &settings);
        let source_times = literal
            .iter()
            .enumerate()
            .map(|(index, step)| (index, step.timestamp_us))
            .collect::<Vec<_>>();
        apply_recording_notes(
            &mut plan,
            &[
                RecordingNote::Marker {
                    timestamp_us: 100_000,
                },
                RecordingNote::Annotation {
                    timestamp_us: 300_000,
                    text: "paste here".into(),
                },
            ],
            &source_times,
        );
        assert!(plan.steps[0].metadata.bookmarked);
        assert!(plan.steps[0].metadata.comment.is_empty());
        assert_eq!(plan.steps[1].metadata.comment, "paste here");
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
