//! Worker-side, pure recording normalization. No OS, UI automation, or persistence is used here.
use super::{
    HookEvent, KeyTransition, MkAction, MkCoordinateTarget, MkErrorPolicy, MkKey, MkMouseButton,
    MkMousePayload, MkPoint, MkStep, MouseButton, MouseMessage, should_record,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovementMode {
    Off,
    ClicksOnly,
    SampledMovement,
    DetailedMovement,
}
#[derive(Debug, Clone)]
pub struct NormalizationConfig {
    pub movement_mode: MovementMode,
    pub movement_distance_px: i32,
    pub movement_interval_ms: u64,
    pub click_max_ms: u64,
    pub click_distance_px: i32,
    pub multi_click_ms: u64,
    pub record_injected_input: bool,
}
impl Default for NormalizationConfig {
    fn default() -> Self {
        Self {
            movement_mode: MovementMode::SampledMovement,
            movement_distance_px: 4,
            movement_interval_ms: 25,
            click_max_ms: 500,
            click_distance_px: 4,
            multi_click_ms: 500,
            record_injected_input: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WindowContext {
    pub executable: String,
    pub title: String,
    pub class: String,
    pub rect: Option<(i32, i32, i32, i32)>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventContext {
    pub foreground: WindowContext,
    pub window_under_point: Option<WindowContext>,
}
pub trait EventEnricher: Send {
    fn enrich(&mut self, event: &HookEvent) -> Option<EventContext>;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingBoundary {
    Event(HookEvent),
    Pause { timestamp_us: u64 },
    Resume { timestamp_us: u64 },
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

fn distance(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
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
        match *item {
            RecordingBoundary::Pause { timestamp_us } => {
                paused = true;
                pause_at = timestamp_us;
            }
            RecordingBoundary::Resume { timestamp_us } => {
                if paused {
                    excluded += timestamp_us.saturating_sub(pause_at);
                    paused = false;
                }
            }
            RecordingBoundary::Event(e)
                if !paused && should_record(&e, cfg.record_injected_input) =>
            {
                raw.push((e, e.timestamp_us().saturating_sub(excluded)))
            }
            _ => {}
        }
    }
    let mut out: Vec<RecordedStep> = vec![];
    let mut down: Option<(MouseButton, (i32, i32), u64, Option<EventContext>)> = None;
    let mut last_move: Option<((i32, i32), u64)> = None;
    for (e, t) in raw {
        let context = enricher.as_deref_mut().and_then(|x| x.enrich(&e));
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
                if let Some((_, start, _, _)) = down.as_mut() {
                    *start = (start.0, start.1);
                }
                let keep = match cfg.movement_mode {
                    MovementMode::Off | MovementMode::ClicksOnly => false,
                    MovementMode::DetailedMovement => last_move.map_or(true, |(p, _)| p != (x, y)),
                    MovementMode::SampledMovement => last_move.map_or(true, |(p, lt)| {
                        distance(p, (x, y)) >= cfg.movement_distance_px
                            || t.saturating_sub(lt) >= cfg.movement_interval_ms * 1000
                    }),
                };
                if keep {
                    last_move = Some(((x, y), t));
                    Some(RecordedAction::Move { x, y })
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
                down = Some((b, (x, y), t, context.clone()));
                None
            }
            HookEvent::Mouse {
                message: MouseMessage::Up(b),
                x,
                y,
                ..
            } => {
                if let Some((db, p, dt, dc)) = down.take() {
                    if db == b
                        && distance(p, (x, y)) <= cfg.click_distance_px
                        && t - dt <= cfg.click_max_ms * 1000
                    {
                        Some(RecordedAction::Click {
                            button: b,
                            x,
                            y,
                            count: 1,
                        })
                    } else if db == b && distance(p, (x, y)) > cfg.click_distance_px {
                        Some(RecordedAction::Drag {
                            button: b,
                            from: p,
                            to: (x, y),
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
            {
                if let RecordedAction::Click {
                    button: pb,
                    x: px,
                    y: py,
                    count: pc,
                } = &mut prev.action
                {
                    if pb == button
                        && distance((*px, *py), (*x, *y)) <= cfg.click_distance_px
                        && t.saturating_sub(prev.timestamp_us) <= cfg.multi_click_ms * 1000
                    {
                        *pc += *count;
                        prev.timestamp_us = t;
                        prev.context = context;
                        continue;
                    }
                }
            }
            out.push(RecordedStep {
                timestamp_us: t,
                delay_after_ms: 0,
                action,
                context,
            });
        }
    }
    if let Some((b, p, t, c)) = down {
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
    out.sort_by_key(|x| x.timestamp_us);
    for i in 0..out.len().saturating_sub(1) {
        out[i].delay_after_ms = out[i + 1].timestamp_us.saturating_sub(out[i].timestamp_us) / 1000;
    }
    out
}

fn key(vk: u32) -> MkKey {
    match vk {
        0x0D => MkKey::Enter,
        0x09 => MkKey::Tab,
        0x1B => MkKey::Escape,
        0x20 => MkKey::Space,
        0x25 => MkKey::Left,
        0x26 => MkKey::Up,
        0x27 => MkKey::Right,
        0x28 => MkKey::Down,
        0x70..=0x87 => MkKey::Function((vk - 0x6f) as u8),
        _ => MkKey::Character(char::from_u32(vk).unwrap_or('?').to_string()),
    }
}
fn button(b: MouseButton) -> MkMouseButton {
    match b {
        MouseButton::Left => MkMouseButton::Left,
        MouseButton::Right => MkMouseButton::Right,
        MouseButton::Middle => MkMouseButton::Middle,
        MouseButton::X1 => MkMouseButton::X1,
        MouseButton::X2 => MkMouseButton::X2,
    }
}
/// Converts a normalized batch to draft steps. IDs are allocated only at insertion time.
pub fn to_macro_steps(items: &[RecordedStep], mut next_id: u64) -> Vec<MkStep> {
    let mut result = Vec::new();
    for s in items {
        let point = |x, y| MkCoordinateTarget::Screen {
            point: MkPoint { x, y },
        };
        let actions: Vec<MkAction> = match s.action {
            RecordedAction::Key { down: true, vk, .. } => vec![MkAction::KeyDown(key(vk))],
            RecordedAction::Key {
                down: false, vk, ..
            } => vec![MkAction::KeyUp(key(vk))],
            RecordedAction::Move { x, y } => vec![MkAction::MouseMove(point(x, y))],
            RecordedAction::Click {
                button: b,
                x,
                y,
                count,
            } => vec![MkAction::MouseClick(MkMousePayload {
                target: point(x, y),
                button: button(b),
                clicks: count,
            })],
            RecordedAction::Down { button: b, .. } => vec![MkAction::MouseDown(button(b))],
            RecordedAction::Up { button: b, .. } => vec![MkAction::MouseUp(button(b))],
            RecordedAction::Drag {
                button: b,
                from,
                to,
            } => vec![
                MkAction::MouseMove(point(from.0, from.1)),
                MkAction::MouseDown(button(b)),
                MkAction::MouseMove(point(to.0, to.1)),
                MkAction::MouseUp(button(b)),
            ],
            RecordedAction::Wheel { delta, .. } => vec![MkAction::MouseScroll { i32_delta: delta }],
        };
        let action_count = actions.len();
        for (i, action) in actions.into_iter().enumerate() {
            next_id += 1;
            result.push(MkStep {
                id: next_id,
                enabled: true,
                repeat: 1,
                delay_after_ms: if i + 1 == action_count {
                    s.delay_after_ms
                } else {
                    0
                },
                on_error: MkErrorPolicy::Stop,
                action,
            });
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::recorder_hooks::LLKHF_EXTENDED;
    fn mouse(t: u64, m: MouseMessage, x: i32, y: i32) -> RecordingBoundary {
        RecordingBoundary::Event(HookEvent::Mouse {
            timestamp_us: t,
            message: m,
            x,
            y,
            flags: 0,
            extra_info: 0,
        })
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
                RecordingBoundary::Event(k),
            ],
            &c,
            None,
        );
        assert_eq!(v.len(), 2);
        assert!(matches!(
            v[1].action,
            RecordedAction::Key {
                down: false,
                scan_code: 30,
                extended: true,
                ..
            }
        ));
        assert_eq!(v[0].delay_after_ms, 100);
    }
}
