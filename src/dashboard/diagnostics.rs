use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const DIAGNOSTICS_REFRESH_INTERVAL: Duration = Duration::from_millis(500);
pub const REFRESH_WARNING_THRESHOLD: Duration = Duration::from_millis(75);

#[derive(Clone, Debug)]
pub struct WidgetRefreshSnapshot {
    pub label: String,
    pub last_refresh_at: Instant,
    pub last_duration: Duration,
}

#[derive(Clone, Debug, Default)]
pub struct DashboardDiagnosticsSnapshot {
    pub fps: f32,
    pub frame_time: Duration,
    pub widget_refreshes: Vec<WidgetRefreshSnapshot>,
}

struct WidgetRefreshState {
    label: String,
    last_refresh_at: Instant,
    last_duration: Duration,
    last_sample: Instant,
}

pub struct DashboardDiagnostics {
    widget_states: HashMap<String, WidgetRefreshState>,
    fps: f32,
    frame_time: Duration,
    refresh_interval: Duration,
    warning_threshold: Duration,
    last_frame_sample: Instant,
}

impl DashboardDiagnostics {
    pub fn new() -> Self {
        Self::new_with_config(DIAGNOSTICS_REFRESH_INTERVAL, REFRESH_WARNING_THRESHOLD)
    }

    pub fn new_with_config(refresh_interval: Duration, warning_threshold: Duration) -> Self {
        let now = Instant::now();
        Self {
            widget_states: HashMap::new(),
            fps: 0.0,
            frame_time: Duration::from_millis(0),
            refresh_interval,
            warning_threshold,
            last_frame_sample: now - refresh_interval,
        }
    }

    pub fn update_frame_timing(&mut self, frame_time: Duration) {
        let now = Instant::now();
        if now.duration_since(self.last_frame_sample) < self.refresh_interval {
            return;
        }
        self.frame_time = frame_time;
        let secs = frame_time.as_secs_f32();
        self.fps = if secs > 0.0 { 1.0 / secs } else { 0.0 };
        self.last_frame_sample = now;
    }

    pub fn record_widget_refresh(&mut self, key: String, label: String, duration: Duration) {
        let now = Instant::now();
        let update_due = match self.widget_states.get(&key) {
            Some(state) => now.duration_since(state.last_sample) >= self.refresh_interval,
            None => true,
        };
        if update_due || duration >= self.warning_threshold {
            let entry = self.widget_states.entry(key).or_insert(WidgetRefreshState {
                label: label.clone(),
                last_refresh_at: now,
                last_duration: duration,
                last_sample: now,
            });
            entry.label = label;
            entry.last_refresh_at = now;
            entry.last_duration = duration;
            entry.last_sample = now;
        }
    }

    pub fn snapshot(&self) -> DashboardDiagnosticsSnapshot {
        let mut widget_refreshes: Vec<WidgetRefreshSnapshot> = self
            .widget_states
            .values()
            .map(|state| WidgetRefreshSnapshot {
                label: state.label.clone(),
                last_refresh_at: state.last_refresh_at,
                last_duration: state.last_duration,
            })
            .collect();
        widget_refreshes.sort_by(|a, b| a.label.cmp(&b.label));
        DashboardDiagnosticsSnapshot {
            fps: self.fps,
            frame_time: self.frame_time,
            widget_refreshes,
        }
    }

    pub fn warning_threshold(&self) -> Duration {
        self.warning_threshold
    }
}

#[cfg(test)]
impl DashboardDiagnostics {
    fn set_last_frame_sample_for_test(&mut self, instant: Instant) {
        self.last_frame_sample = instant;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_metrics_throttle_until_interval() {
        let mut diagnostics = DashboardDiagnostics::new_with_config(
            Duration::from_secs(10),
            REFRESH_WARNING_THRESHOLD,
        );
        diagnostics.update_frame_timing(Duration::from_millis(16));
        let first = diagnostics.snapshot();

        diagnostics.update_frame_timing(Duration::from_millis(33));
        let second = diagnostics.snapshot();
        assert_eq!(first.frame_time, second.frame_time);

        diagnostics.set_last_frame_sample_for_test(Instant::now() - Duration::from_secs(11));
        diagnostics.update_frame_timing(Duration::from_millis(33));
        let third = diagnostics.snapshot();
        assert_ne!(first.frame_time, third.frame_time);
    }

    #[test]
    fn widget_refresh_updates_on_threshold() {
        let mut diagnostics = DashboardDiagnostics::new_with_config(
            Duration::from_secs(10),
            Duration::from_millis(50),
        );
        diagnostics.record_widget_refresh(
            "widget-a".to_string(),
            "Widget A".to_string(),
            Duration::from_millis(10),
        );
        let first = diagnostics.snapshot();
        assert_eq!(first.widget_refreshes.len(), 1);
        assert_eq!(first.widget_refreshes[0].last_duration, Duration::from_millis(10));

        diagnostics.record_widget_refresh(
            "widget-a".to_string(),
            "Widget A".to_string(),
            Duration::from_millis(5),
        );
        let second = diagnostics.snapshot();
        assert_eq!(second.widget_refreshes[0].last_duration, Duration::from_millis(10));

        diagnostics.record_widget_refresh(
            "widget-a".to_string(),
            "Widget A".to_string(),
            Duration::from_millis(75),
        );
        let third = diagnostics.snapshot();
        assert_eq!(third.widget_refreshes[0].last_duration, Duration::from_millis(75));
    }
}
