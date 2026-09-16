//! Shared policy, hover deadline state, and passive tooltip placement.

use super::font_cache::PreparedTextLayout;
use super::geometry::{LogicalPoint, LogicalRect, PhysicalRect, ScaleFactor};
use super::model::{CellId, RadialFeatureSettings, SessionId, TooltipScope};
use super::session::FrameId;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const DEFAULT_TOOLTIP_DELAY_MS: u64 = 300;
pub const MAX_TOOLTIP_WIDTH_LOGICAL: f32 = 420.0;
pub const MAX_TOOLTIP_HEIGHT_FRACTION: f32 = 0.60;
pub const TOOLTIP_PADDING_LOGICAL: f32 = 8.0;
const TOOLTIP_GAP_LOGICAL: f32 = 10.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TooltipPreferences {
    pub scope: TooltipScope,
    pub delay_ms: u64,
    pub show_expected_diagnostics: bool,
}

impl Default for TooltipPreferences {
    fn default() -> Self {
        Self {
            scope: TooltipScope::AllCells,
            delay_ms: DEFAULT_TOOLTIP_DELAY_MS,
            show_expected_diagnostics: false,
        }
    }
}

impl From<&RadialFeatureSettings> for TooltipPreferences {
    fn from(settings: &RadialFeatureSettings) -> Self {
        Self {
            scope: settings.tooltip_scope,
            delay_ms: settings.tooltip_delay_ms,
            show_expected_diagnostics: settings.show_expected_layout_diagnostics,
        }
    }
}

impl TooltipPreferences {
    pub fn label_is_eligible(self, truncated: bool) -> bool {
        match self.scope {
            TooltipScope::Off => false,
            TooltipScope::TruncatedOnly => truncated,
            TooltipScope::AllCells => true,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PreparedTooltip {
    /// The complete authored/projected label; never replaced by ellipsized text.
    pub full_label: Arc<str>,
    /// An optional custom description retained separately from the label.
    pub description: Option<Arc<str>>,
    /// Bounded, wrapped display layout in label-then-description order.
    pub layout: Arc<PreparedTextLayout>,
    pub font_size: f32,
    pub show_label: bool,
    pub label_was_truncated: bool,
}

impl PreparedTooltip {
    pub fn combined_source(&self) -> String {
        let label = self.show_label.then_some(self.full_label.as_ref());
        match (label, self.description.as_deref()) {
            (Some(label), Some(description)) => format!("{label}\n{description}"),
            (Some(label), None) => label.to_owned(),
            (None, Some(description)) => description.to_owned(),
            (None, None) => String::new(),
        }
    }

    pub fn logical_size(&self) -> (f32, f32) {
        let width = self.layout.measured_width_milli as f32 / 1_000.0;
        let height = self.layout.measured_height_milli as f32 / 1_000.0;
        (
            width + TOOLTIP_PADDING_LOGICAL * 2.0,
            height + TOOLTIP_PADDING_LOGICAL * 2.0,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TooltipIdentity {
    pub session_id: SessionId,
    pub frame_id: FrameId,
    pub layout_generation: u64,
    pub cell_id: CellId,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TooltipHoverState {
    candidate: Option<TooltipCandidate>,
    visible: Option<TooltipIdentity>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TooltipCandidate {
    identity: TooltipIdentity,
    deadline_ms: u64,
}

impl TooltipHoverState {
    /// Starts a deadline only when the eligible identity changes. Repeated
    /// pointer movement within one cell does not postpone a pending tooltip.
    pub fn observe(
        &mut self,
        identity: Option<TooltipIdentity>,
        eligible: bool,
        now_ms: u64,
        delay_ms: u64,
    ) -> Option<u64> {
        let identity = identity.filter(|_| eligible);
        if identity.is_none() {
            self.cancel();
            return None;
        }
        let identity = identity.expect("checked above");
        if self
            .candidate
            .as_ref()
            .is_some_and(|candidate| candidate.identity == identity)
        {
            return self
                .candidate
                .as_ref()
                .map(|candidate| candidate.deadline_ms);
        }
        if self.visible.as_ref() == Some(&identity) {
            return None;
        }
        let deadline_ms = now_ms.saturating_add(delay_ms);
        self.visible = None;
        self.candidate = Some(TooltipCandidate {
            identity,
            deadline_ms,
        });
        Some(deadline_ms)
    }

    /// Reveals only the still-current candidate after its one-shot deadline.
    pub fn expire(&mut self, identity: &TooltipIdentity, now_ms: u64) -> bool {
        let Some(candidate) = self.candidate.as_ref() else {
            return false;
        };
        if &candidate.identity != identity || candidate.deadline_ms > now_ms {
            return false;
        }
        self.visible = Some(identity.clone());
        self.candidate = None;
        true
    }

    pub fn cancel(&mut self) -> bool {
        let changed = self.candidate.is_some() || self.visible.is_some();
        self.candidate = None;
        self.visible = None;
        changed
    }

    pub fn visible(&self) -> Option<&TooltipIdentity> {
        self.visible.as_ref()
    }

    pub fn candidate(&self) -> Option<(&TooltipIdentity, u64)> {
        self.candidate
            .as_ref()
            .map(|candidate| (&candidate.identity, candidate.deadline_ms))
    }
}

enum TooltipDeadlineCommand {
    Arm(u64),
    Cancel,
    Stop,
}

/// Retained one-shot wake source for preview hosts that do not already own a
/// keyed deadline scheduler. It sends one host wake at the current deadline;
/// the receiver validates the generation-tagged candidate before revealing.
pub struct TooltipDeadlineScheduler {
    tx: mpsc::Sender<TooltipDeadlineCommand>,
    join: Option<JoinHandle<()>>,
    armed_deadline: std::sync::Mutex<Option<u64>>,
}

impl TooltipDeadlineScheduler {
    pub fn spawn(wake: mpsc::Sender<()>) -> Result<Self, String> {
        let (tx, rx) = mpsc::channel();
        let join = std::thread::Builder::new()
            .name("radial-tooltip-deadline".into())
            .spawn(move || {
                let mut deadline: Option<u64> = None;
                loop {
                    let command = if let Some(at) = deadline {
                        let remaining = at.saturating_sub(monotonic_ms());
                        match rx.recv_timeout(Duration::from_millis(remaining.max(1))) {
                            Ok(command) => Some(command),
                            Err(mpsc::RecvTimeoutError::Timeout) => {
                                deadline = None;
                                let _ = wake.send(());
                                None
                            }
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        }
                    } else {
                        match rx.recv() {
                            Ok(command) => Some(command),
                            Err(_) => break,
                        }
                    };
                    match command {
                        Some(TooltipDeadlineCommand::Arm(at)) => deadline = Some(at),
                        Some(TooltipDeadlineCommand::Cancel) => deadline = None,
                        Some(TooltipDeadlineCommand::Stop) => break,
                        None => {}
                    }
                }
            })
            .map_err(|error| format!("failed to start radial tooltip scheduler: {error}"))?;
        Ok(Self {
            tx,
            join: Some(join),
            armed_deadline: std::sync::Mutex::new(None),
        })
    }

    pub fn arm(&self, deadline_ms: u64) {
        if let Ok(mut armed_deadline) = self.armed_deadline.lock() {
            if *armed_deadline == Some(deadline_ms) {
                return;
            }
            *armed_deadline = Some(deadline_ms);
        }
        let _ = self.tx.send(TooltipDeadlineCommand::Arm(deadline_ms));
    }

    pub fn cancel(&self) {
        if let Ok(mut armed_deadline) = self.armed_deadline.lock() {
            if armed_deadline.take().is_none() {
                return;
            }
        }
        let _ = self.tx.send(TooltipDeadlineCommand::Cancel);
    }
}

impl Drop for TooltipDeadlineScheduler {
    fn drop(&mut self) {
        let _ = self.tx.send(TooltipDeadlineCommand::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

pub fn monotonic_ms() -> u64 {
    static EPOCH: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    EPOCH
        .get_or_init(Instant::now)
        .elapsed()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

/// Places a tooltip beside its hovered cell in the frozen monitor work area.
/// This returns visual geometry only; callers must not feed it into menu
/// fitting, pointer ownership, or the session's spatial anchor.
pub fn place_tooltip(
    anchor: LogicalRect,
    size: (f32, f32),
    work_area: PhysicalRect,
    scale: ScaleFactor,
) -> LogicalRect {
    let min = scale.physical_to_logical(work_area.min);
    let max = scale.physical_to_logical(work_area.max);
    let area_width = (max.x - min.x).max(1.0);
    let area_height = (max.y - min.y).max(1.0);
    let width = size.0.max(1.0).min(area_width);
    let height = size.1.max(1.0).min(area_height);

    let mut x = anchor.max.x + TOOLTIP_GAP_LOGICAL;
    if x + width > max.x {
        x = anchor.min.x - TOOLTIP_GAP_LOGICAL - width;
    }
    let mut y = anchor.min.y + (anchor.max.y - anchor.min.y - height) * 0.5;
    if y + height > max.y {
        y = anchor.min.y - TOOLTIP_GAP_LOGICAL - height;
    }
    if y < min.y {
        y = anchor.max.y + TOOLTIP_GAP_LOGICAL;
    }

    x = x.clamp(min.x, (max.x - width).max(min.x));
    y = y.clamp(min.y, (max.y - height).max(min.y));
    LogicalRect {
        min: LogicalPoint { x, y },
        max: LogicalPoint {
            x: x + width,
            y: y + height,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work_area(min: (f64, f64), max: (f64, f64)) -> PhysicalRect {
        PhysicalRect {
            min: super::super::geometry::PhysicalPoint { x: min.0, y: min.1 },
            max: super::super::geometry::PhysicalPoint { x: max.0, y: max.1 },
        }
    }

    #[test]
    fn hover_deadline_is_stationary_one_shot_and_same_cell_motion_does_not_postpone() {
        let id = TooltipIdentity {
            session_id: SessionId::new("session"),
            frame_id: FrameId(4),
            layout_generation: 9,
            cell_id: CellId::new("cell"),
        };
        let mut state = TooltipHoverState::default();
        assert_eq!(state.observe(Some(id.clone()), true, 100, 300), Some(400));
        assert_eq!(state.observe(Some(id.clone()), true, 250, 300), Some(400));
        assert!(!state.expire(&id, 399));
        assert!(state.expire(&id, 400));
        assert_eq!(state.visible(), Some(&id));
        assert!(!state.expire(&id, 500));
    }

    #[test]
    fn moving_inside_a_visible_cell_keeps_the_tooltip_visible_without_restarting() {
        let id = TooltipIdentity {
            session_id: SessionId::new("session"),
            frame_id: FrameId(4),
            layout_generation: 9,
            cell_id: CellId::new("cell"),
        };
        let mut state = TooltipHoverState::default();
        state.observe(Some(id.clone()), true, 100, 300);
        assert!(state.expire(&id, 400));
        assert_eq!(state.observe(Some(id.clone()), true, 450, 300), None);
        assert_eq!(state.visible(), Some(&id));
        assert!(state.candidate().is_none());
    }

    #[test]
    fn retained_deadline_sends_one_wake_without_pointer_activity() {
        let (tx, rx) = mpsc::channel();
        let scheduler = TooltipDeadlineScheduler::spawn(tx).unwrap();
        scheduler.arm(monotonic_ms().saturating_add(5));
        rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn different_identity_and_ineligible_hover_cancel_stale_deadline() {
        let mut state = TooltipHoverState::default();
        let mut first = TooltipIdentity {
            session_id: SessionId::new("session"),
            frame_id: FrameId(1),
            layout_generation: 3,
            cell_id: CellId::new("first"),
        };
        state.observe(Some(first.clone()), true, 1, 300);
        first.cell_id = CellId::new("second");
        assert_eq!(state.observe(Some(first.clone()), true, 5, 300), Some(305));
        let stale_generation = TooltipIdentity {
            layout_generation: first.layout_generation - 1,
            ..first.clone()
        };
        assert!(!state.expire(
            &TooltipIdentity {
                cell_id: CellId::new("first"),
                ..first.clone()
            },
            500,
        ));
        assert!(!state.expire(&stale_generation, 500));
        assert_eq!(state.observe(Some(first), false, 6, 300), None);
        assert!(state.candidate().is_none());
    }

    #[test]
    fn placement_flips_and_clamps_to_all_work_area_edges_including_negative_origin() {
        let work = work_area((-400.0, -250.0), (800.0, 650.0));
        let scale = ScaleFactor::new(1.0).unwrap();
        for anchor in [
            LogicalRect {
                min: LogicalPoint { x: 720.0, y: 300.0 },
                max: LogicalPoint { x: 770.0, y: 350.0 },
            },
            LogicalRect {
                min: LogicalPoint {
                    x: -380.0,
                    y: 280.0,
                },
                max: LogicalPoint {
                    x: -340.0,
                    y: 320.0,
                },
            },
            LogicalRect {
                min: LogicalPoint {
                    x: 100.0,
                    y: -240.0,
                },
                max: LogicalPoint {
                    x: 140.0,
                    y: -210.0,
                },
            },
            LogicalRect {
                min: LogicalPoint { x: 100.0, y: 600.0 },
                max: LogicalPoint { x: 140.0, y: 630.0 },
            },
        ] {
            let placed = place_tooltip(anchor, (240.0, 120.0), work, scale);
            assert!(placed.min.x >= -400.0 && placed.max.x <= 800.0);
            assert!(placed.min.y >= -250.0 && placed.max.y <= 650.0);
            assert_eq!(placed.max.x - placed.min.x, 240.0);
            assert_eq!(placed.max.y - placed.min.y, 120.0);
        }
    }
}
