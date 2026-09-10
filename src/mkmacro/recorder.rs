//! Worker-side, pure recording normalization. No OS, UI automation, or persistence is used here.
use super::{
    HookEvent, KeyTransition, MkAction, MkCoordinateTarget, MkErrorPolicy, MkKey, MkMouseButton,
    MkMouseDragPayload, MkMouseMovePayload, MkMousePayload, MkMouseScrollAxis, MkPoint,
    MkRecorderSettings, MkStep, MkWindowMatcher, MkWindowPayload, MouseButton, MouseMessage,
    MovementMode, mk_key_from_windows_event, should_record,
};

#[derive(Debug, Clone)]
pub struct NormalizationConfig {
    pub record_keyboard: bool,
    pub record_mouse_buttons: bool,
    pub record_mouse_wheel: bool,
    pub movement_mode: MovementMode,
    pub movement_distance_px: i32,
    pub movement_interval_ms: u64,
    pub click_max_ms: u64,
    pub click_distance_px: i32,
    pub multi_click_ms: u64,
    pub record_injected_input: bool,
    /// Capture target-window metadata and author activation/client-relative actions.
    pub record_window_context: bool,
    pub minimum_idle_delay_ms: u64,
    pub delay_rounding_ms: u64,
    pub key_tap_max_ms: u64,
    pub text_run_gap_ms: u64,
    pub smart_keyboard_cleanup: bool,
    pub smart_mouse_cleanup: bool,
    pub smart_window_cleanup: bool,
    pub smart_repeated_click_cleanup: bool,
    pub detect_application_launches: bool,
    pub inspect_clicked_controls: bool,
    pub capture_text_paste_for_freeze_suggestion: bool,
    pub repeated_click_minimum: u32,
    pub repeated_click_interval_tolerance_ms: u64,
}

impl From<&MkRecorderSettings> for NormalizationConfig {
    fn from(settings: &MkRecorderSettings) -> Self {
        Self {
            record_keyboard: settings.record_keyboard,
            record_mouse_buttons: settings.record_mouse_buttons,
            record_mouse_wheel: settings.record_mouse_wheel,
            movement_mode: settings.movement_mode,
            movement_distance_px: settings.movement_distance_px,
            movement_interval_ms: settings.movement_interval_ms,
            click_max_ms: settings.click_max_ms,
            click_distance_px: settings.click_distance_px,
            multi_click_ms: settings.multi_click_ms,
            record_injected_input: settings.record_injected_input,
            record_window_context: settings.record_window_context,
            minimum_idle_delay_ms: settings.minimum_idle_delay_ms,
            delay_rounding_ms: settings.delay_rounding_ms,
            key_tap_max_ms: settings.key_tap_max_ms,
            text_run_gap_ms: settings.text_run_gap_ms,
            smart_keyboard_cleanup: settings.smart_keyboard_cleanup,
            smart_mouse_cleanup: settings.smart_mouse_cleanup,
            smart_window_cleanup: settings.smart_window_cleanup,
            smart_repeated_click_cleanup: settings.smart_repeated_click_cleanup,
            detect_application_launches: settings.detect_application_launches,
            inspect_clicked_controls: settings.inspect_clicked_controls,
            capture_text_paste_for_freeze_suggestion: settings
                .capture_text_paste_for_freeze_suggestion,
            repeated_click_minimum: settings.repeated_click_minimum,
            repeated_click_interval_tolerance_ms: settings.repeated_click_interval_tolerance_ms,
        }
    }
}

impl NormalizationConfig {
    pub fn semantic_settings(&self) -> MkRecorderSettings {
        let mut settings = MkRecorderSettings::default();
        settings.record_window_context = self.record_window_context;
        settings.minimum_idle_delay_ms = self.minimum_idle_delay_ms;
        settings.delay_rounding_ms = self.delay_rounding_ms;
        settings.key_tap_max_ms = self.key_tap_max_ms;
        settings.text_run_gap_ms = self.text_run_gap_ms;
        settings.smart_keyboard_cleanup = self.smart_keyboard_cleanup;
        settings.smart_mouse_cleanup = self.smart_mouse_cleanup;
        settings.smart_window_cleanup = self.smart_window_cleanup;
        settings.smart_repeated_click_cleanup = self.smart_repeated_click_cleanup;
        settings.detect_application_launches = self.detect_application_launches;
        settings.inspect_clicked_controls = self.inspect_clicked_controls;
        settings.capture_text_paste_for_freeze_suggestion =
            self.capture_text_paste_for_freeze_suggestion;
        settings.repeated_click_minimum = self.repeated_click_minimum;
        settings.repeated_click_interval_tolerance_ms = self.repeated_click_interval_tolerance_ms;
        settings
    }
}
impl Default for NormalizationConfig {
    fn default() -> Self {
        Self::from(&MkRecorderSettings::default())
    }
}

/// Editable-macro sampling defaults: coarse enough to avoid noisy recordings while
/// still leaving useful waypoints for hand editing.
pub const DEFAULT_MOVEMENT_DISTANCE_PX: i32 = 16;
pub const DEFAULT_MOVEMENT_INTERVAL_MS: u64 = 80;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WindowContext {
    pub executable: String,
    pub process_path: String,
    pub title: String,
    pub class: String,
    pub rect: Option<(i32, i32, i32, i32)>,
    pub client_origin: Option<MkPoint>,
    /// Stable only within the capture session; never persisted.
    pub native_root_id: Option<usize>,
    /// Transient owning process/thread identity used only for recording correlation.
    pub process_id: Option<u32>,
    pub thread_id: Option<u32>,
    /// OS process start time (seconds since boot/epoch as supplied by the observer).
    /// Combined with PID this protects launch inference from PID reuse.
    pub process_started_at: Option<u64>,
}
impl WindowContext {
    /// Produces a non-empty, normalized matcher, preferring executable + title.
    pub fn matcher(&self) -> Option<MkWindowMatcher> {
        let value = |s: &str| (!s.trim().is_empty()).then(|| s.trim().to_owned());
        let title = value(&self.title);
        let process = value(&self.executable).or_else(|| value(&self.process_path));
        let class = (title.is_none() || process.is_none())
            .then(|| value(&self.class))
            .flatten();
        if title.is_none() && process.is_none() && class.is_none() {
            return None;
        }
        Some(MkWindowMatcher {
            title,
            title_regex: None,
            process,
            class,
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventContext {
    pub foreground: WindowContext,
    pub window_under_point: Option<WindowContext>,
    /// Input layout of the foreground window's owning thread at event time.
    pub keyboard_layout: Option<isize>,
}
pub trait EventEnricher: Send {
    fn enrich(&mut self, event: &HookEvent) -> Option<EventContext>;
    /// Lightweight keyboard-only path used when window-context authoring is disabled.
    fn enrich_keyboard_layout(&mut self) -> Option<EventContext> {
        None
    }
    fn context_for_root(&mut self, _root: usize) -> Option<WindowContext> {
        None
    }
    /// Whether a native handle is a visible, non-tool top-level window. Owned
    /// top-level windows remain eligible because they commonly represent dialogs.
    /// The default keeps synthetic enrichers platform independent.
    fn is_recordable_top_level(&self, _root: usize) -> bool {
        true
    }
    fn invalidate_root(&mut self, _root: usize) {}
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordingBoundary {
    Event(HookEvent, Option<EventContext>),
    Pause { timestamp_us: u64 },
    Resume { timestamp_us: u64 },
    Marker { timestamp_us: u64 },
    Annotation { timestamp_us: u64, text: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordedAction {
    Key {
        down: bool,
        vk: u32,
        scan_code: u32,
        extended: bool,
        flags: u32,
        extra_info: usize,
    },
    Move {
        x: i32,
        y: i32,
        /// Travel time into this retained waypoint. Microsecond differences are
        /// truncated to whole milliseconds; sub-millisecond remainders are not
        /// accumulated or rounded up.
        duration_ms: u64,
    },
    Down {
        button: MouseButton,
        x: i32,
        y: i32,
    },
    Up {
        button: MouseButton,
        x: i32,
        y: i32,
    },
    Click {
        button: MouseButton,
        x: i32,
        y: i32,
        count: u32,
    },
    Drag {
        button: MouseButton,
        from: (i32, i32),
        to: (i32, i32),
        down_timestamp_us: u64,
        up_timestamp_us: u64,
    },
    Wheel {
        delta: i32,
        horizontal: bool,
        x: i32,
        y: i32,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedStep {
    pub timestamp_us: u64,
    pub delay_after_ms: u64,
    pub action: RecordedAction,
    pub context: Option<EventContext>,
}

fn distance(a: (i32, i32), b: (i32, i32)) -> i64 {
    (i64::from(a.0) - i64::from(b.0))
        .abs()
        .max((i64::from(a.1) - i64::from(b.1)).abs())
}

fn point(step: &RecordedStep) -> Option<(i32, i32)> {
    match step.action {
        RecordedAction::Move { x, y, .. } => Some((x, y)),
        _ => None,
    }
}

fn perpendicular_distance_squared(p: (i32, i32), a: (i32, i32), b: (i32, i32)) -> f64 {
    let (px, py) = (p.0 as f64, p.1 as f64);
    let (ax, ay) = (a.0 as f64, a.1 as f64);
    let (dx, dy) = ((b.0 - a.0) as f64, (b.1 - a.1) as f64);
    let length_squared = dx * dx + dy * dy;
    if length_squared == 0.0 {
        return (px - ax).powi(2) + (py - ay).powi(2);
    }
    let cross = dx * (ay - py) - (ax - px) * dy;
    cross * cross / length_squared
}

/// Pure Ramer-Douglas-Peucker simplification. Selected steps are cloned so their
/// exact timestamp, integer coordinates, and event context are retained.
fn simplify_move_run(run: &[RecordedStep], tolerance_px: f64) -> Vec<RecordedStep> {
    if run.len() <= 2 {
        return run.to_vec();
    }
    let mut keep = vec![false; run.len()];
    keep[0] = true;
    keep[run.len() - 1] = true;
    let mut pending = vec![(0, run.len() - 1)];
    while let Some((start, end)) = pending.pop() {
        let (a, b) = (point(&run[start]).unwrap(), point(&run[end]).unwrap());
        let mut best = None;
        for i in start + 1..end {
            let deviation = perpendicular_distance_squared(point(&run[i]).unwrap(), a, b);
            if best.is_none_or(|(_, value)| deviation > value) {
                best = Some((i, deviation));
            }
        }
        if let Some((i, deviation)) = best
            && deviation > tolerance_px * tolerance_px
        {
            keep[i] = true;
            pending.push((i, end));
            pending.push((start, i));
        }
    }
    run.iter()
        .zip(keep)
        .filter_map(|(step, keep)| keep.then(|| step.clone()))
        .collect()
}

fn simplify_sampled_runs(steps: Vec<RecordedStep>, cfg: &NormalizationConfig) -> Vec<RecordedStep> {
    if cfg.movement_mode != MovementMode::SampledMovement {
        return steps;
    }
    // Half the raw sampling distance is deliberately less aggressive than stage one.
    let tolerance = (cfg.movement_distance_px.max(1) as f64) / 2.0;
    let mut result = Vec::new();
    let mut start = 0;
    while start < steps.len() {
        if point(&steps[start]).is_none() {
            result.push(steps[start].clone());
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < steps.len() && point(&steps[end]).is_some() {
            end += 1;
        }
        result.extend(simplify_move_run(&steps[start..end], tolerance));
        start = end;
    }
    result
}

fn sample_move_runs(steps: Vec<RecordedStep>, cfg: &NormalizationConfig) -> Vec<RecordedStep> {
    match cfg.movement_mode {
        MovementMode::Off | MovementMode::ClicksOnly => {
            return steps.into_iter().filter(|s| point(s).is_none()).collect();
        }
        MovementMode::DetailedMovement => return steps,
        MovementMode::SampledMovement => {}
    }
    let mut result = Vec::new();
    let mut start = 0;
    while start < steps.len() {
        if point(&steps[start]).is_none() {
            result.push(steps[start].clone());
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < steps.len() && point(&steps[end]).is_some() {
            end += 1;
        }
        let run = &steps[start..end];
        result.push(run[0].clone());
        let mut last = 0;
        for i in 1..run.len().saturating_sub(1) {
            if distance(point(&run[last]).unwrap(), point(&run[i]).unwrap())
                >= i64::from(cfg.movement_distance_px)
                || run[i].timestamp_us.saturating_sub(run[last].timestamp_us)
                    >= cfg.movement_interval_ms * 1000
            {
                result.push(run[i].clone());
                last = i;
            }
        }
        if run.len() > 1 {
            result.push(run[run.len() - 1].clone());
        }
        start = end;
    }
    result
}
pub fn normalize(
    input: &[RecordingBoundary],
    cfg: &NormalizationConfig,
    mut enricher: Option<&mut dyn EventEnricher>,
) -> Vec<RecordedStep> {
    let mut raw = Vec::new();
    let mut paused = false;
    let mut pause_at = 0;
    let mut excluded = 0;
    for item in input {
        match item {
            RecordingBoundary::Pause { timestamp_us } => {
                paused = true;
                pause_at = *timestamp_us;
            }
            RecordingBoundary::Resume { timestamp_us } => {
                if paused {
                    excluded += timestamp_us.saturating_sub(pause_at);
                    paused = false;
                }
            }
            RecordingBoundary::Event(e, captured_context)
                if !paused && should_record(e, cfg.record_injected_input) =>
            {
                raw.push((
                    *e,
                    e.timestamp_us().saturating_sub(excluded),
                    captured_context.clone(),
                ))
            }
            _ => {}
        }
    }
    let mut out: Vec<RecordedStep> = vec![];
    // Phase 2/3: retain distinct raw positions, then recognize clicks and drags.
    let mut down: Option<(MouseButton, (i32, i32), u64, Option<EventContext>, usize)> = None;
    let mut last_move: Option<((i32, i32), u64)> = None;
    for (e, t, captured_context) in raw {
        let enabled = match e {
            HookEvent::Key { .. } => cfg.record_keyboard,
            HookEvent::Mouse {
                message: MouseMessage::Move,
                ..
            } => true,
            HookEvent::Mouse {
                message: MouseMessage::Down(_) | MouseMessage::Up(_),
                ..
            } => cfg.record_mouse_buttons,
            HookEvent::Mouse {
                message: MouseMessage::Wheel(_) | MouseMessage::HorizontalWheel(_),
                ..
            } => cfg.record_mouse_wheel,
        };
        if !enabled {
            continue;
        }
        let context = if cfg.record_window_context || matches!(e, HookEvent::Key { .. }) {
            captured_context.or_else(|| enricher.as_deref_mut().and_then(|x| x.enrich(&e)))
        } else {
            None
        };
        if !matches!(
            e,
            HookEvent::Mouse {
                message: MouseMessage::Move,
                ..
            }
        ) {
            last_move = None;
        }
        let action = match e {
            HookEvent::Key {
                transition,
                vk,
                scan_code,
                flags,
                extra_info,
                ..
            } => Some(RecordedAction::Key {
                down: transition == KeyTransition::Down,
                vk,
                scan_code,
                extended: flags & super::LLKHF_EXTENDED != 0,
                flags,
                extra_info,
            }),
            HookEvent::Mouse {
                message: MouseMessage::Move,
                x,
                y,
                ..
            } => {
                let keep = match cfg.movement_mode {
                    MovementMode::Off | MovementMode::ClicksOnly => false,
                    MovementMode::DetailedMovement => last_move.is_none_or(|(p, _)| p != (x, y)),
                    MovementMode::SampledMovement => last_move.is_none_or(|(p, _)| p != (x, y)),
                };
                if keep {
                    last_move = Some(((x, y), t));
                    Some(RecordedAction::Move {
                        x,
                        y,
                        duration_ms: 0,
                    })
                } else {
                    None
                }
            }
            HookEvent::Mouse {
                message: MouseMessage::Down(b),
                x,
                y,
                ..
            } => {
                down = Some((b, (x, y), t, context.clone(), out.len()));
                None
            }
            HookEvent::Mouse {
                message: MouseMessage::Up(b),
                x,
                y,
                ..
            } => {
                if let Some((db, p, dt, dc, down_index)) = down.take() {
                    // MouseDrag is an endpoint-only model: discard standalone in-drag
                    // movement, while leaving movement before down and after up intact.
                    out.truncate(down_index);
                    if db == b
                        && distance(p, (x, y)) <= i64::from(cfg.click_distance_px)
                        && t.saturating_sub(dt) <= cfg.click_max_ms * 1000
                    {
                        Some(RecordedAction::Click {
                            button: b,
                            x,
                            y,
                            count: 1,
                        })
                    } else if db == b && distance(p, (x, y)) > i64::from(cfg.click_distance_px) {
                        Some(RecordedAction::Drag {
                            button: b,
                            from: p,
                            to: (x, y),
                            down_timestamp_us: dt,
                            up_timestamp_us: t,
                        })
                    } else {
                        out.push(RecordedStep {
                            timestamp_us: dt,
                            delay_after_ms: 0,
                            action: RecordedAction::Down {
                                button: db,
                                x: p.0,
                                y: p.1,
                            },
                            context: dc,
                        });
                        Some(RecordedAction::Up { button: b, x, y })
                    }
                } else {
                    Some(RecordedAction::Up { button: b, x, y })
                }
            }
            HookEvent::Mouse {
                message: MouseMessage::Wheel(delta),
                x,
                y,
                ..
            } => Some(RecordedAction::Wheel {
                delta,
                horizontal: false,
                x,
                y,
            }),
            HookEvent::Mouse {
                message: MouseMessage::HorizontalWheel(delta),
                x,
                y,
                ..
            } => Some(RecordedAction::Wheel {
                delta,
                horizontal: true,
                x,
                y,
            }),
        };
        if let Some(action) = action {
            if let (
                Some(prev),
                RecordedAction::Click {
                    button,
                    x,
                    y,
                    count,
                },
            ) = (out.last_mut(), &action)
                && let RecordedAction::Click {
                    button: pb,
                    x: px,
                    y: py,
                    count: pc,
                } = &mut prev.action
                && pb == button
                && distance((*px, *py), (*x, *y)) <= i64::from(cfg.click_distance_px)
                && t.saturating_sub(prev.timestamp_us) <= cfg.multi_click_ms * 1000
            {
                *pc += *count;
                prev.timestamp_us = t;
                prev.context = context;
                continue;
            }
            out.push(RecordedStep {
                timestamp_us: t,
                delay_after_ms: 0,
                action,
                context,
            });
        }
    }
    if let Some((b, p, t, c, _)) = down {
        out.push(RecordedStep {
            timestamp_us: t,
            delay_after_ms: 0,
            action: RecordedAction::Down {
                button: b,
                x: p.0,
                y: p.1,
            },
            context: c,
        });
    }
    // Phase 4: stage-one sampling followed by pure geometric simplification.
    out = simplify_sampled_runs(sample_move_runs(out, cfg), cfg);
    // Phase 5: finalize chronology. A move timestamp is its arrival time. Each
    // contiguous run's anchor is instantaneous, and later waypoints own all
    // elapsed travel since the preceding retained waypoint (including time
    // across points removed by either simplifier). Saturating subtraction means
    // equal/reversed timestamps never fabricate movement time. Conversion from
    // microseconds truncates each interval independently to whole milliseconds.
    out.sort_by_key(|x| x.timestamp_us);
    for i in 1..out.len() {
        let elapsed_ms = out[i].timestamp_us.saturating_sub(out[i - 1].timestamp_us) / 1000;
        if matches!(out[i - 1].action, RecordedAction::Move { .. })
            && let RecordedAction::Move { duration_ms, .. } = &mut out[i].action
        {
            *duration_ms = elapsed_ms;
        }
    }
    // Delays contain idle time only. Timed actions begin before their arrival/end,
    // so their owned interval is subtracted by comparing this action's end with
    // the next action's start.
    for i in 0..out.len().saturating_sub(1) {
        let next_start_us = match out[i + 1].action {
            RecordedAction::Move { duration_ms, .. } => out[i + 1]
                .timestamp_us
                .saturating_sub(duration_ms.saturating_mul(1000)),
            RecordedAction::Drag {
                down_timestamp_us, ..
            } => down_timestamp_us,
            _ => out[i + 1].timestamp_us,
        };
        out[i].delay_after_ms = next_start_us.saturating_sub(out[i].timestamp_us) / 1000;
    }
    out
}

/// Converts literal normalized output through the explicit compatibility plan.
/// New recorder results expose their semantic `RecordingPlan` directly.
pub fn to_macro_steps(
    items: &[RecordedStep],
    next_id: u64,
    record_window_context: bool,
) -> Vec<MkStep> {
    super::materialize_plan(
        &super::build_literal_recording_plan(items, record_window_context),
        next_id,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::recorder_hooks::LLKHF_EXTENDED;
    fn mouse(t: u64, m: MouseMessage, x: i32, y: i32) -> RecordingBoundary {
        RecordingBoundary::Event(
            HookEvent::Mouse {
                timestamp_us: t,
                message: m,
                x,
                y,
                flags: 0,
                extra_info: 0,
            },
            None,
        )
    }
    fn move_step(t: u64, x: i32, y: i32) -> RecordedStep {
        RecordedStep {
            timestamp_us: t,
            delay_after_ms: 0,
            action: RecordedAction::Move {
                x,
                y,
                duration_ms: 0,
            },
            context: None,
        }
    }

    #[test]
    fn editable_sampling_defaults_are_explicit() {
        let cfg = NormalizationConfig::default();
        assert_eq!(cfg.movement_mode, MovementMode::SampledMovement);
        assert_eq!(cfg.movement_distance_px, 16);
        assert_eq!(cfg.movement_interval_ms, 80);
    }

    #[test]
    fn pure_simplifier_handles_lines_corners_and_degenerate_endpoints() {
        let line = [
            move_step(0, -1_000_000, 5),
            move_step(1, 0, 5),
            move_step(2, 1_000_000, 5),
        ];
        assert_eq!(
            simplify_move_run(&line, 2.0),
            vec![line[0].clone(), line[2].clone()]
        );
        let corner = [
            move_step(0, 0, 0),
            move_step(1, 20, 30),
            move_step(2, 40, 0),
        ];
        assert_eq!(simplify_move_run(&corner, 5.0), corner);
        let repeated = [move_step(0, 7, 7), move_step(1, 20, 7), move_step(2, 7, 7)];
        assert_eq!(simplify_move_run(&repeated, 2.0), repeated);
        assert_eq!(simplify_move_run(&line[..1], 2.0), line[..1]);
        assert_eq!(simplify_move_run(&line[..2], 2.0), line[..2]);
    }

    #[test]
    fn modes_and_action_boundaries_preserve_expected_moves() {
        let events = [
            mouse(0, MouseMessage::Move, 0, 0),
            mouse(1_000, MouseMessage::Move, 1, 0),
            mouse(2_000, MouseMessage::Move, 30, 0),
            mouse(3_000, MouseMessage::Wheel(120), 30, 0),
            mouse(4_000, MouseMessage::Move, 31, 0),
        ];
        for mode in [MovementMode::Off, MovementMode::ClicksOnly] {
            let mut cfg = NormalizationConfig::default();
            cfg.movement_mode = mode;
            assert!(
                normalize(&events, &cfg, None)
                    .iter()
                    .all(|s| point(s).is_none())
            );
        }
        let mut cfg = NormalizationConfig::default();
        cfg.movement_mode = MovementMode::DetailedMovement;
        assert_eq!(
            normalize(&events, &cfg, None)
                .iter()
                .filter(|s| point(s).is_some())
                .count(),
            4
        );
        cfg.movement_mode = MovementMode::SampledMovement;
        let sampled = normalize(&events, &cfg, None);
        let positions: Vec<_> = sampled.iter().filter_map(point).collect();
        assert_eq!(positions, vec![(0, 0), (30, 0), (31, 0)]);
        assert_eq!(sampled[1].delay_after_ms, 1);
    }

    #[test]
    fn detailed_and_sampled_share_a_long_timed_pointer_fixture() {
        // Exact 10 ms spacing makes the expected timeline independent of
        // fractional rounding while 101 points provide meaningful compression.
        let events: Vec<_> = (0..=100)
            .map(|i| mouse(i * 10_000, MouseMessage::Move, -500 + i as i32 * 10, -75))
            .collect();
        let mut cfg = NormalizationConfig::default();
        cfg.movement_distance_px = 40;
        cfg.movement_interval_ms = 1_000;
        cfg.movement_mode = MovementMode::DetailedMovement;
        let detailed = normalize(&events, &cfg, None);
        cfg.movement_mode = MovementMode::SampledMovement;
        let sampled = normalize(&events, &cfg, None);

        let detailed_moves: Vec<_> = detailed.iter().filter_map(point).collect();
        let sampled_moves: Vec<_> = sampled.iter().filter_map(point).collect();
        assert!(detailed_moves.len() > sampled_moves.len());
        assert!(
            sampled_moves.len() * 10 <= detailed_moves.len(),
            "straight-line sampling should remove at least 90%: detailed={}, sampled={}",
            detailed_moves.len(),
            sampled_moves.len()
        );
        assert_eq!(sampled_moves.first(), Some(&(-500, -75)));
        assert_eq!(sampled_moves.last(), Some(&(500, -75)));

        let elapsed = |steps: &[RecordedStep]| -> u64 {
            steps
                .iter()
                .map(|step| match step.action {
                    RecordedAction::Move { duration_ms, .. } => duration_ms,
                    _ => 0,
                })
                .sum::<u64>()
                + steps.iter().map(|step| step.delay_after_ms).sum::<u64>()
        };
        assert_eq!(elapsed(&detailed), 1_000);
        assert_eq!(elapsed(&sampled), 1_000);
    }

    #[test]
    fn sampling_preserves_click_drag_and_button_transition_anchors() {
        let events = [
            mouse(0, MouseMessage::Move, -100, -50),
            mouse(500, MouseMessage::Move, -99, -50),
            mouse(1_000, MouseMessage::Down(MouseButton::Left), -90, -40),
            mouse(1_500, MouseMessage::Up(MouseButton::Left), -90, -40),
            mouse(2_000, MouseMessage::Down(MouseButton::Left), -90, -40),
            mouse(2_500, MouseMessage::Up(MouseButton::Left), -90, -40),
            mouse(3_000, MouseMessage::Move, -80, -30),
            mouse(3_100, MouseMessage::Move, -79, -29),
            mouse(3_200, MouseMessage::Down(MouseButton::Right), -70, -20),
            mouse(3_300, MouseMessage::Move, -10, 10),
            mouse(3_400, MouseMessage::Up(MouseButton::Right), 40, 25),
            mouse(3_500, MouseMessage::Move, 41, 26),
            mouse(3_600, MouseMessage::Move, 42, 27),
        ];
        let mut cfg = NormalizationConfig::default();
        cfg.movement_distance_px = 1_000;
        cfg.movement_interval_ms = 1_000;
        let normalized = normalize(&events, &cfg, None);

        assert!(matches!(
            normalized[0].action,
            RecordedAction::Move {
                x: -100,
                y: -50,
                ..
            }
        ));
        assert!(matches!(
            normalized[1].action,
            RecordedAction::Move { x: -99, y: -50, .. }
        ));
        assert!(matches!(
            normalized[2].action,
            RecordedAction::Click {
                button: MouseButton::Left,
                x: -90,
                y: -40,
                count: 2
            }
        ));
        assert!(matches!(
            normalized[3].action,
            RecordedAction::Move { x: -80, y: -30, .. }
        ));
        assert!(matches!(
            normalized[4].action,
            RecordedAction::Move { x: -79, y: -29, .. }
        ));
        assert!(matches!(
            normalized[5].action,
            RecordedAction::Drag {
                button: MouseButton::Right,
                from: (-70, -20),
                to: (40, 25),
                down_timestamp_us: 3_200,
                up_timestamp_us: 3_400
            }
        ));
        assert!(matches!(
            normalized[6].action,
            RecordedAction::Move { x: 41, y: 26, .. }
        ));
        assert!(matches!(
            normalized[7].action,
            RecordedAction::Move { x: 42, y: 27, .. }
        ));
        assert_eq!(
            normalized.len(),
            8,
            "in-drag movement must not escape as a waypoint"
        );
    }
    #[test]
    fn clicks_repeats_drag_wheels_and_delay() {
        let c = NormalizationConfig::default();
        let v = normalize(
            &[
                mouse(0, MouseMessage::Down(MouseButton::Left), 0, 0),
                mouse(10_000, MouseMessage::Up(MouseButton::Left), 0, 0),
                mouse(20_000, MouseMessage::Down(MouseButton::Left), 0, 0),
                mouse(30_000, MouseMessage::Up(MouseButton::Left), 0, 0),
                mouse(50_000, MouseMessage::Down(MouseButton::Left), 0, 0),
                mouse(60_000, MouseMessage::Move, 20, 0),
                mouse(70_000, MouseMessage::Up(MouseButton::Left), 20, 0),
                mouse(80_000, MouseMessage::Wheel(120), 20, 0),
                mouse(90_000, MouseMessage::HorizontalWheel(-120), 20, 0),
            ],
            &c,
            None,
        );
        assert!(matches!(
            v[0].action,
            RecordedAction::Click { count: 2, .. }
        ));
        assert!(
            v.iter()
                .any(|x| matches!(x.action, RecordedAction::Drag { .. }))
        );
        assert!(v.iter().any(|x| matches!(
            x.action,
            RecordedAction::Wheel {
                horizontal: true,
                ..
            }
        )));
        assert!(v[0].delay_after_ms > 0);
    }
    #[test]
    fn wheel_axes_and_signed_deltas_survive_conversion_without_coalescing() {
        let events = [
            mouse(0, MouseMessage::Wheel(120), 1, 2),
            mouse(1, MouseMessage::Wheel(-73), 1, 2),
            mouse(2, MouseMessage::HorizontalWheel(240), 1, 2),
            mouse(3, MouseMessage::HorizontalWheel(-41), 1, 2),
        ];
        let normalized = normalize(&events, &NormalizationConfig::default(), None);
        assert_eq!(
            normalized.len(),
            4,
            "adjacent wheel axes/deltas stay distinct"
        );
        let actions: Vec<_> = to_macro_steps(&normalized, 0, false)
            .into_iter()
            .map(|step| step.action)
            .collect();
        assert_eq!(
            actions,
            vec![
                MkAction::MouseScroll {
                    axis: MkMouseScrollAxis::Vertical,
                    i32_delta: 120
                },
                MkAction::MouseScroll {
                    axis: MkMouseScrollAxis::Vertical,
                    i32_delta: -73
                },
                MkAction::MouseScroll {
                    axis: MkMouseScrollAxis::Horizontal,
                    i32_delta: 240
                },
                MkAction::MouseScroll {
                    axis: MkMouseScrollAxis::Horizontal,
                    i32_delta: -41
                },
            ]
        );
    }

    #[test]
    fn sampling_pause_and_key_fidelity() {
        let mut c = NormalizationConfig::default();
        c.movement_distance_px = 10;
        let k = HookEvent::Key {
            timestamp_us: 200_000,
            transition: KeyTransition::Up,
            vk: 65,
            scan_code: 30,
            flags: LLKHF_EXTENDED,
            extra_info: 7,
        };
        let v = normalize(
            &[
                mouse(0, MouseMessage::Move, 0, 0),
                mouse(1_000, MouseMessage::Move, 2, 2),
                RecordingBoundary::Pause {
                    timestamp_us: 2_000,
                },
                mouse(4_000, MouseMessage::Move, 30, 30),
                RecordingBoundary::Resume {
                    timestamp_us: 102_000,
                },
                RecordingBoundary::Event(k, None),
            ],
            &c,
            None,
        );
        // Although the second position is below the normal sampling threshold,
        // it is the final waypoint before the key action and therefore forms the
        // required end of that contiguous movement run.
        assert_eq!(v.len(), 3);
        assert!(matches!(
            v[0].action,
            RecordedAction::Move { x: 0, y: 0, .. }
        ));
        assert!(matches!(
            v[1].action,
            RecordedAction::Move { x: 2, y: 2, .. }
        ));
        assert!(matches!(
            v[2].action,
            RecordedAction::Key {
                down: false,
                scan_code: 30,
                extended: true,
                ..
            }
        ));
        assert!(matches!(
            v[1].action,
            RecordedAction::Move { duration_ms: 1, .. }
        ));
        // The millisecond between retained waypoints is travel owned by the
        // destination move, not idle time duplicated on the anchor.
        assert_eq!(v[0].delay_after_ms, 0);
        assert_eq!(v[1].delay_after_ms, 99);
    }

    #[test]
    fn normalized_drag_becomes_one_drag_step() {
        let recorded = RecordedStep {
            timestamp_us: 0,
            action: RecordedAction::Drag {
                button: MouseButton::Right,
                from: (3, 4),
                to: (30, 40),
                down_timestamp_us: 12_000,
                up_timestamp_us: 45_000,
            },
            delay_after_ms: 77,
            context: None,
        };
        let steps = to_macro_steps(&[recorded], 10, false);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].delay_after_ms, 77);
        let MkAction::MouseDrag(payload) = &steps[0].action else {
            panic!()
        };
        assert_eq!(payload.button, MkMouseButton::Right);
        assert_eq!(payload.duration_ms, 33);
        assert_eq!(
            payload.from,
            MkCoordinateTarget::Screen {
                point: MkPoint { x: 3, y: 4 }
            }
        );
        assert_eq!(
            payload.to,
            MkCoordinateTarget::Screen {
                point: MkPoint { x: 30, y: 40 }
            }
        );
    }

    #[test]
    fn retained_waypoints_own_independent_travel_without_duplicate_delays() {
        let mut c = NormalizationConfig::default();
        c.movement_mode = MovementMode::DetailedMovement;
        let normalized = normalize(
            &[
                mouse(1_000, MouseMessage::Move, 0, 0),
                mouse(11_500, MouseMessage::Move, 10, 0),
                mouse(31_900, MouseMessage::Move, 20, 10),
                mouse(46_900, MouseMessage::Wheel(120), 20, 10),
            ],
            &c,
            None,
        );
        let steps = to_macro_steps(&normalized, 40, false);
        let durations: Vec<_> = steps
            .iter()
            .filter_map(|step| match step.action {
                MkAction::MouseMove(ref payload) => Some(payload.duration_ms),
                _ => None,
            })
            .collect();
        assert_eq!(durations, [0, 10, 20]);
        assert_eq!(
            steps.iter().map(|step| step.id).collect::<Vec<_>>(),
            [41, 42, 43, 44]
        );
        assert_eq!(steps[0].delay_after_ms, 0);
        assert_eq!(steps[1].delay_after_ms, 0);
        assert_eq!(steps[2].delay_after_ms, 15);
    }

    #[test]
    fn simplification_rolls_removed_time_into_destination_and_equal_time_stays_zero() {
        let mut c = NormalizationConfig::default();
        c.movement_distance_px = 100;
        c.movement_interval_ms = 100;
        let normalized = normalize(
            &[
                mouse(5_000, MouseMessage::Move, 0, 0),
                mouse(10_000, MouseMessage::Move, 1, 0),
                mouse(25_000, MouseMessage::Move, 2, 0),
            ],
            &c,
            None,
        );
        assert_eq!(normalized.len(), 2);
        assert!(matches!(
            normalized[1].action,
            RecordedAction::Move {
                duration_ms: 20,
                ..
            }
        ));

        c.movement_mode = MovementMode::DetailedMovement;
        let equal = normalize(
            &[
                mouse(9_000, MouseMessage::Move, 0, 0),
                mouse(9_000, MouseMessage::Move, 1, 0),
            ],
            &c,
            None,
        );
        assert!(matches!(
            equal[1].action,
            RecordedAction::Move { duration_ms: 0, .. }
        ));
    }

    #[test]
    fn key_boundary_and_pause_prevent_travel_time_leaking_between_runs() {
        let mut c = NormalizationConfig::default();
        c.movement_mode = MovementMode::DetailedMovement;
        let key = |t| {
            RecordingBoundary::Event(
                HookEvent::Key {
                    timestamp_us: t,
                    transition: KeyTransition::Down,
                    vk: 65,
                    scan_code: 30,
                    flags: 0,
                    extra_info: 0,
                },
                None,
            )
        };
        let normalized = normalize(
            &[
                mouse(0, MouseMessage::Move, 0, 0),
                mouse(10_000, MouseMessage::Move, 10, 0),
                key(15_000),
                mouse(20_000, MouseMessage::Move, 20, 0),
                RecordingBoundary::Pause {
                    timestamp_us: 22_000,
                },
                RecordingBoundary::Resume {
                    timestamp_us: 122_000,
                },
                mouse(130_000, MouseMessage::Move, 30, 0),
            ],
            &c,
            None,
        );
        assert!(matches!(
            normalized[1].action,
            RecordedAction::Move {
                duration_ms: 10,
                ..
            }
        ));
        assert_eq!(normalized[1].delay_after_ms, 5);
        assert!(matches!(
            normalized[3].action,
            RecordedAction::Move { duration_ms: 0, .. }
        ));
        assert!(matches!(
            normalized[4].action,
            RecordedAction::Move {
                duration_ms: 10,
                ..
            }
        ));
    }

    #[test]
    fn mismatched_and_unmatched_buttons_remain_fallback_transitions() {
        let normalized = normalize(
            &[
                mouse(0, MouseMessage::Down(MouseButton::Left), 1, 2),
                mouse(20_000, MouseMessage::Up(MouseButton::Right), 30, 40),
                mouse(30_000, MouseMessage::Up(MouseButton::Middle), 30, 40),
            ],
            &NormalizationConfig::default(),
            None,
        );
        assert_eq!(normalized.len(), 3);
        assert!(matches!(
            normalized[0].action,
            RecordedAction::Down {
                button: MouseButton::Left,
                ..
            }
        ));
        assert!(matches!(
            normalized[1].action,
            RecordedAction::Up {
                button: MouseButton::Right,
                ..
            }
        ));
        assert!(matches!(
            normalized[2].action,
            RecordedAction::Up {
                button: MouseButton::Middle,
                ..
            }
        ));
    }

    #[test]
    fn click_splits_move_runs_without_absorbing_click_time() {
        let mut c = NormalizationConfig::default();
        c.movement_mode = MovementMode::DetailedMovement;
        let normalized = normalize(
            &[
                mouse(0, MouseMessage::Move, 0, 0),
                mouse(10_000, MouseMessage::Move, 10, 0),
                mouse(20_000, MouseMessage::Down(MouseButton::Left), 10, 0),
                mouse(25_000, MouseMessage::Up(MouseButton::Left), 10, 0),
                mouse(35_000, MouseMessage::Move, 20, 0),
                mouse(45_000, MouseMessage::Move, 30, 0),
            ],
            &c,
            None,
        );
        assert!(matches!(
            normalized[1].action,
            RecordedAction::Move {
                duration_ms: 10,
                ..
            }
        ));
        assert_eq!(normalized[1].delay_after_ms, 15);
        assert!(matches!(normalized[2].action, RecordedAction::Click { .. }));
        assert_eq!(normalized[2].delay_after_ms, 10);
        assert!(matches!(
            normalized[3].action,
            RecordedAction::Move { duration_ms: 0, .. }
        ));
        assert!(matches!(
            normalized[4].action,
            RecordedAction::Move {
                duration_ms: 10,
                ..
            }
        ));
    }
}
