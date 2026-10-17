//! Payload-free playback presentation and its ephemeral passive surface.
use super::{
    PlaybackMode, SequencePlaybackStatus, SequenceRoutePresentation, SequenceShortcutPresentation,
};
use std::time::Duration;

const CURSOR_POLL: Duration = Duration::from_millis(50);

/// This boundary cannot carry step text, clipboard data, prompt values or titles.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PlaybackProgress<'a> {
    pub(crate) generation: u64,
    pub(crate) name: &'a str,
    pub(crate) mode: PlaybackMode,
    pub(crate) step: usize,
    pub(crate) total: usize,
    pub(crate) status: SequencePlaybackStatus,
}

fn safe_label(value: &str, limit: usize) -> String {
    let mut chars = value.chars().filter(|c| {
        !c.is_control() && !matches!(*c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
    });
    let mut result: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        result.push('…');
    }
    result
}

fn active(status: SequencePlaybackStatus) -> bool {
    matches!(
        status,
        SequencePlaybackStatus::Running
            | SequencePlaybackStatus::AwaitAdvance
            | SequencePlaybackStatus::AwaitTarget
            | SequencePlaybackStatus::AwaitPaste
    )
}

fn text(
    progress: PlaybackProgress<'_>,
    routes: &SequenceRoutePresentation,
    escape: bool,
) -> String {
    let action = match progress.status {
        SequencePlaybackStatus::AwaitTarget => {
            "Release shortcut keys; focus an eligible app".into()
        }
        SequencePlaybackStatus::Running if progress.mode == PlaybackMode::PasteToAdvance => {
            "Pasting; wait before the next Ctrl+V".into()
        }
        _ => match progress.mode {
            PlaybackMode::Automatic => "Automatic · focus the destination app".into(),
            PlaybackMode::ManualAdvance => match &routes.next {
                SequenceShortcutPresentation::Active(chord) => {
                    format!("{} or Next button", safe_label(chord, 48))
                }
                _ => "Next button".into(),
            },
            PlaybackMode::PasteToAdvance => "Ctrl+V to paste".into(),
        },
    };
    let cancel = match &routes.cancel {
        SequenceShortcutPresentation::Active(chord) => safe_label(chord, 48),
        _ => "Cancel Run button".into(),
    };
    let cancel = if escape {
        format!("Esc or {cancel}")
    } else {
        cancel
    };
    format!(
        "{} · step {} of {}\n{}\nCancel: {}",
        safe_label(progress.name, 48),
        progress.step,
        progress.total,
        action,
        cancel
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CursorArea {
    pub(crate) cursor: (i32, i32),
    pub(crate) work: (i32, i32, i32, i32),
    pub(crate) dpi: u32,
}

/// Geometry only. Reuse radial's existing visual placement without any radial
/// input/session owner, then reject/clamp positions that could cover the pointer.
pub(crate) fn place(area: CursorArea, size: (i32, i32)) -> Option<(i32, i32)> {
    use crate::radial::geometry::{
        LogicalPoint, LogicalRect, PhysicalPoint, PhysicalRect, ScaleFactor,
    };
    let (left, top, right, bottom) = area.work;
    let (width, height) = size;
    if width <= 0
        || height <= 0
        || !(48..=768).contains(&area.dpi)
        || i64::from(right) - i64::from(left) < i64::from(width)
        || i64::from(bottom) - i64::from(top) < i64::from(height)
    {
        return None;
    }
    let scale = ScaleFactor::new(f64::from(area.dpi) / 96.0)?;
    let point = scale.physical_to_logical(PhysicalPoint {
        x: f64::from(area.cursor.0),
        y: f64::from(area.cursor.1),
    });
    let placed = crate::radial::tooltip::place_tooltip(
        LogicalRect {
            min: LogicalPoint {
                x: point.x - 2.0,
                y: point.y - 2.0,
            },
            max: LogicalPoint {
                x: point.x + 2.0,
                y: point.y + 2.0,
            },
        },
        (
            width as f32 / scale.get() as f32,
            height as f32 / scale.get() as f32,
        ),
        PhysicalRect {
            min: PhysicalPoint {
                x: f64::from(left),
                y: f64::from(top),
            },
            max: PhysicalPoint {
                x: f64::from(right),
                y: f64::from(bottom),
            },
        },
        scale,
    );
    let margin = (8.0 * scale.get()).ceil() as i64;
    let (cx, cy) = (i64::from(area.cursor.0), i64::from(area.cursor.1));
    let candidate = (
        (f64::from(placed.min.x) * scale.get()).round() as i64,
        (f64::from(placed.min.y) * scale.get()).round() as i64,
    );
    let candidates = [
        candidate,
        (cx + margin, cy + margin),
        (cx - margin - i64::from(width), cy + margin),
        (cx + margin, cy - margin - i64::from(height)),
        (
            cx - margin - i64::from(width),
            cy - margin - i64::from(height),
        ),
    ];
    for (x, y) in candidates {
        let x = x.clamp(i64::from(left), i64::from(right) - i64::from(width));
        let y = y.clamp(i64::from(top), i64::from(bottom) - i64::from(height));
        let overlaps_pointer = x < cx + margin
            && x + i64::from(width) > cx - margin
            && y < cy + margin
            && y + i64::from(height) > cy - margin;
        if !overlaps_pointer {
            return Some((i32::try_from(x).ok()?, i32::try_from(y).ok()?));
        }
    }
    None
}

pub(crate) trait ProgressPresenter {
    fn sample(&mut self) -> Option<CursorArea>;
    fn measure(&mut self, text: &str, dpi: u32) -> Option<(i32, i32)>;
    fn show_at(&mut self, position: (i32, i32)) -> Option<()>;
    fn clear(&mut self);
}

#[derive(Default)]
pub(crate) struct NativeProgressPresenter {
    #[cfg(windows)]
    window: Option<crate::platform::passive_tooltip::PassiveTooltip>,
}
impl ProgressPresenter for NativeProgressPresenter {
    fn sample(&mut self) -> Option<CursorArea> {
        // Legacy app/unit fixtures must never probe the interactive desktop.
        // Presentation tests inject their own environment and presenter below.
        #[cfg(all(windows, not(test)))]
        {
            use windows::Win32::{
                Foundation::POINT,
                Graphics::Gdi::{
                    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
                },
                UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
            };
            let cursor = crate::platform::windows_api::current_mouse_position()?;
            if !cursor.0.is_finite() || !cursor.1.is_finite() {
                return None;
            }
            let point = POINT {
                x: cursor.0.round() as i32,
                y: cursor.1.round() as i32,
            };
            let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
            if monitor.0.is_null() {
                return None;
            }
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
                return None;
            }
            let (mut x, mut y) = (0, 0);
            unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y) }.ok()?;
            if x == 0 || y == 0 {
                return None;
            }
            Some(CursorArea {
                cursor: (point.x, point.y),
                work: (
                    info.rcWork.left,
                    info.rcWork.top,
                    info.rcWork.right,
                    info.rcWork.bottom,
                ),
                dpi: x,
            })
        }
        #[cfg(any(not(windows), test))]
        {
            None
        }
    }
    fn measure(&mut self, text: &str, dpi: u32) -> Option<(i32, i32)> {
        #[cfg(windows)]
        {
            if self.window.is_none() {
                self.window = crate::platform::passive_tooltip::PassiveTooltip::new();
            }
            let window = self.window.as_mut()?;
            window.set_dpi(dpi)?;
            window.set_text(text);
            window.size()
        }
        #[cfg(not(windows))]
        {
            let _ = (text, dpi);
            None
        }
    }
    fn show_at(&mut self, position: (i32, i32)) -> Option<()> {
        #[cfg(windows)]
        {
            self.window.as_mut()?.show_at(position)
        }
        #[cfg(not(windows))]
        {
            let _ = position;
            None
        }
    }
    fn clear(&mut self) {
        #[cfg(windows)]
        {
            self.window = None;
        }
    }
}

pub(crate) struct SequenceProgressTooltip<P = NativeProgressPresenter> {
    presenter: P,
    generation: Option<u64>,
    failed: bool,
    last: Option<(String, CursorArea)>,
}
impl<P: Default> Default for SequenceProgressTooltip<P> {
    fn default() -> Self {
        Self {
            presenter: P::default(),
            generation: None,
            failed: false,
            last: None,
        }
    }
}
impl<P: ProgressPresenter> SequenceProgressTooltip<P> {
    pub(crate) fn clear(&mut self) {
        if self.last.take().is_some() {
            self.presenter.clear();
        }
    }
    /// Returns a scoped cursor wake and one presentation-failure notification.
    pub(crate) fn update(
        &mut self,
        progress: Option<PlaybackProgress<'_>>,
        routes: Option<&SequenceRoutePresentation>,
        escape: bool,
        enabled: bool,
    ) -> (Option<Duration>, bool) {
        let progress = progress.filter(|progress| active(progress.status));
        let generation = progress.map(|progress| progress.generation);
        if self.generation != generation {
            self.clear();
            self.failed = false;
            self.generation = generation;
        }
        let Some((progress, routes)) = progress
            .zip(routes)
            .filter(|(_, routes)| enabled && routes.tooltip_enabled)
        else {
            self.clear();
            return (None, false);
        };
        if self.failed {
            return (None, false);
        }
        let value = text(progress, routes, escape);
        let result = (|| {
            let area = self.presenter.sample()?;
            if self
                .last
                .as_ref()
                .is_some_and(|(old, prior_area)| old == &value && *prior_area == area)
            {
                return Some(());
            }
            let size = self.presenter.measure(&value, area.dpi)?;
            let position = place(area, size)?;
            self.presenter.show_at(position)?;
            self.last = Some((value, area));
            Some(())
        })();
        if result.is_none() {
            self.presenter.clear();
            self.last = None;
            self.failed = true;
            return (None, true);
        }
        (Some(CURSOR_POLL), false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paste_sequence::{SequencePlaybackError, SequenceShortcutPresentation as Shortcut};

    fn routes() -> SequenceRoutePresentation {
        SequenceRoutePresentation {
            generation: 7,
            next: Shortcut::Active("Ctrl+F8".into()),
            cancel: Shortcut::Active("Ctrl+F9".into()),
            starts: Vec::new(),
            tooltip_enabled: true,
            escape_cancel_enabled: true,
        }
    }
    fn progress(mode: PlaybackMode) -> PlaybackProgress<'static> {
        PlaybackProgress {
            generation: 1,
            name: "Safe sequence",
            mode,
            step: 2,
            total: 3,
            status: match mode {
                PlaybackMode::Automatic => SequencePlaybackStatus::Running,
                PlaybackMode::ManualAdvance => SequencePlaybackStatus::AwaitAdvance,
                PlaybackMode::PasteToAdvance => SequencePlaybackStatus::AwaitPaste,
            },
        }
    }
    #[derive(Default)]
    struct FakePresenter {
        samples: usize,
        measures: Vec<(String, u32)>,
        positions: Vec<(i32, i32)>,
        clears: usize,
        area: Option<CursorArea>,
        fail_show: bool,
    }
    impl ProgressPresenter for FakePresenter {
        fn sample(&mut self) -> Option<CursorArea> {
            self.samples += 1;
            self.area
        }
        fn measure(&mut self, text: &str, dpi: u32) -> Option<(i32, i32)> {
            self.measures.push((text.into(), dpi));
            Some((220 * dpi as i32 / 96, 70 * dpi as i32 / 96))
        }
        fn show_at(&mut self, position: (i32, i32)) -> Option<()> {
            self.positions.push(position);
            (!self.fail_show).then_some(())
        }
        fn clear(&mut self) {
            self.clears += 1;
        }
    }
    fn tooltip() -> SequenceProgressTooltip<FakePresenter> {
        SequenceProgressTooltip {
            presenter: FakePresenter {
                area: Some(CursorArea {
                    cursor: (100, 100),
                    work: (0, 0, 1920, 1040),
                    dpi: 96,
                }),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn sequence_progress_metadata_uses_actual_actions_and_sanitizes_names() {
        let mut routes = routes();
        for (mode, action) in [
            (PlaybackMode::Automatic, "Automatic"),
            (PlaybackMode::ManualAdvance, "Ctrl+F8"),
            (PlaybackMode::PasteToAdvance, "Ctrl+V to paste"),
        ] {
            let value = text(progress(mode), &routes, false);
            assert!(value.contains("Safe sequence · step 2 of 3"));
            assert!(value.contains(action));
            assert!(value.contains("Ctrl+F9"));
            assert!(!value.contains("Esc"));
        }
        let mut state = progress(PlaybackMode::PasteToAdvance);
        state.status = SequencePlaybackStatus::Running;
        assert!(text(state, &routes, true).contains("Pasting; wait"));
        assert!(text(state, &routes, true).contains("Esc or Ctrl+F9"));
        state.status = SequencePlaybackStatus::AwaitTarget;
        assert!(text(state, &routes, false).contains("eligible app"));
        routes.next = Shortcut::Inactive {
            configured: "unavailable next".into(),
            reason: "conflict".into(),
        };
        routes.cancel = Shortcut::Disabled;
        let value = text(progress(PlaybackMode::ManualAdvance), &routes, false);
        assert!(value.contains("Next button"));
        assert!(value.contains("Cancel Run button"));
        assert!(!value.contains("unavailable next"));
        let name = format!("\0\n\u{202e}{}", "界".repeat(100));
        let mut state = progress(PlaybackMode::Automatic);
        state.name = &name;
        let value = text(state, &routes, false);
        assert!(!value.contains('\0'));
        assert!(!value.contains('\u{202e}'));
        assert_eq!(safe_label(&name, 48).chars().count(), 49);
    }

    #[test]
    fn sequence_progress_geometry_preserves_signed_coordinates_dpi_and_pointer_exclusion() {
        for (work, cursor) in [
            ((0, 0, 1920, 1040), (1919, 1039)),
            ((-7680, -2160, -3840, 0), (-7679, -2159)),
            ((40000, -100, 46000, 3000), (45999, 2999)),
            ((-70000, -50000, -60000, -40000), (-65000, -45000)),
        ] {
            for dpi in [96, 144, 192] {
                let area = CursorArea { work, cursor, dpi };
                let size = (220 * dpi as i32 / 96, 70 * dpi as i32 / 96);
                let (x, y) = place(area, size).unwrap();
                assert!(x >= work.0 && y >= work.1 && x + size.0 <= work.2 && y + size.1 <= work.3);
                let margin = 8 * dpi as i32 / 96;
                assert!(
                    x + size.0 <= cursor.0 - margin
                        || x >= cursor.0 + margin
                        || y + size.1 <= cursor.1 - margin
                        || y >= cursor.1 + margin
                );
            }
        }
        assert!(
            place(
                CursorArea {
                    cursor: (10, 10),
                    work: (0, 0, 20, 20),
                    dpi: 96
                },
                (220, 70)
            )
            .is_none()
        );
        let first = CursorArea {
            cursor: (100, 100),
            work: (0, 0, 1920, 1040),
            dpi: 96,
        };
        assert_ne!(
            place(first, (220, 70)),
            place(CursorArea { dpi: 192, ..first }, (440, 140))
        );
    }

    #[test]
    fn sequence_progress_surface_follows_cursor_and_live_preferences_without_idle_wakes() {
        let mut tooltip = tooltip();
        let mut routes = routes();
        assert_eq!(
            tooltip.update(None, Some(&routes), false, true),
            (None, false)
        );
        assert_eq!(tooltip.presenter.samples, 0);
        for mode in [
            PlaybackMode::Automatic,
            PlaybackMode::ManualAdvance,
            PlaybackMode::PasteToAdvance,
        ] {
            let state = progress(mode);
            assert_eq!(
                tooltip.update(Some(state), Some(&routes), false, true),
                (Some(CURSOR_POLL), false)
            );
            let count = tooltip.presenter.positions.len();
            tooltip.update(Some(state), Some(&routes), false, true);
            assert_eq!(
                tooltip.presenter.positions.len(),
                count,
                "unchanged state does not update the native surface"
            );
            tooltip.presenter.area.as_mut().unwrap().cursor.0 += 20;
            tooltip.update(Some(state), Some(&routes), false, true);
            assert_eq!(tooltip.presenter.positions.len(), count + 1);
            tooltip.presenter.area.as_mut().unwrap().dpi = 144;
            tooltip.update(Some(state), Some(&routes), true, true);
            assert_eq!(tooltip.presenter.measures.last().unwrap().1, 144);
            assert!(tooltip.presenter.measures.last().unwrap().0.contains("Esc"));
            routes.tooltip_enabled = false;
            let samples = tooltip.presenter.samples;
            assert_eq!(
                tooltip.update(Some(state), Some(&routes), true, true),
                (None, false)
            );
            assert_eq!(tooltip.presenter.samples, samples);
            routes.tooltip_enabled = true;
            tooltip.update(Some(state), Some(&routes), false, true);
            assert!(tooltip.last.is_some());
            tooltip.update(Some(state), Some(&routes), false, false);
            assert!(tooltip.last.is_none(), "plugin-off releases the surface");
        }
        let samples = tooltip.presenter.samples;
        tooltip.update(None, Some(&routes), false, true);
        assert_eq!(tooltip.presenter.samples, samples);
    }

    #[test]
    fn sequence_progress_terminal_cleanup_and_failure_latch_are_per_run() {
        let mut tooltip = tooltip();
        let routes = routes();
        let state = progress(PlaybackMode::PasteToAdvance);
        tooltip.presenter.fail_show = true;
        assert_eq!(
            tooltip.update(Some(state), Some(&routes), false, true),
            (None, true)
        );
        assert_eq!(tooltip.presenter.clears, 1);
        let samples = tooltip.presenter.samples;
        assert_eq!(
            tooltip.update(Some(state), Some(&routes), false, true),
            (None, false)
        );
        assert_eq!(tooltip.presenter.samples, samples);
        tooltip.presenter.fail_show = false;
        for terminal in [
            SequencePlaybackStatus::Completed,
            SequencePlaybackStatus::Cancelled,
            SequencePlaybackStatus::Failed(SequencePlaybackError::ClipboardChanged),
        ] {
            let state = PlaybackProgress {
                generation: terminal_generation(terminal),
                ..state
            };
            tooltip.update(Some(state), Some(&routes), false, true);
            assert!(tooltip.last.is_some());
            let clears = tooltip.presenter.clears;
            assert_eq!(
                tooltip.update(
                    Some(PlaybackProgress {
                        status: terminal,
                        ..state
                    }),
                    Some(&routes),
                    false,
                    true
                ),
                (None, false)
            );
            assert_eq!(tooltip.presenter.clears, clears + 1);
            let samples = tooltip.presenter.samples;
            tooltip.update(None, Some(&routes), false, true);
            assert_eq!(tooltip.presenter.samples, samples);
        }
    }
    fn terminal_generation(status: SequencePlaybackStatus) -> u64 {
        match status {
            SequencePlaybackStatus::Completed => 2,
            SequencePlaybackStatus::Cancelled => 3,
            _ => 4,
        }
    }
}
