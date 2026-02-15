use std::collections::{HashMap, VecDeque};
use std::time::Instant;

pub const DRAW_PERF_DEBUG_ENV: &str = "ML_DRAW_PERF_DEBUG";
const DEFAULT_WINDOW_SIZE: usize = 120;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawPerfSnapshot {
    pub enabled: bool,
    pub avg_ms: f64,
    pub worst_ms: f64,
    pub p95_ms: f64,
    pub raster_ms: f64,
    pub input_ingest_ms: f64,
    pub invalidate_to_paint_ms: f64,
    pub points_per_second: f64,
    pub effective_present_hz: f64,
    pub input_to_present_ms: f64,
    pub coalesced_moves: u64,
    pub dirty_pixels: u64,
    pub estimated_pixels_touched: u64,
    pub misuse_count: u64,
    pub frame_samples: usize,
}

impl Default for DrawPerfSnapshot {
    fn default() -> Self {
        Self {
            enabled: false,
            avg_ms: 0.0,
            worst_ms: 0.0,
            p95_ms: 0.0,
            raster_ms: 0.0,
            input_ingest_ms: 0.0,
            invalidate_to_paint_ms: 0.0,
            points_per_second: 0.0,
            effective_present_hz: 0.0,
            input_to_present_ms: 0.0,
            coalesced_moves: 0,
            dirty_pixels: 0,
            estimated_pixels_touched: 0,
            misuse_count: 0,
            frame_samples: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DrawPerfStats {
    enabled: bool,
    window_size: usize,
    started_at: Instant,
    next_span_id: u64,
    frame_ms_window: VecDeque<f64>,
    raster_ms_window: VecDeque<f64>,
    input_ms_window: VecDeque<f64>,
    invalidate_ms_window: VecDeque<f64>,
    pending_input_spans: HashMap<u64, u64>,
    pending_raster_spans: HashMap<u64, u64>,
    pending_invalidate: VecDeque<u64>,
    points_window: VecDeque<(u64, u32)>,
    present_window: VecDeque<u64>,
    input_to_present_ms_window: VecDeque<f64>,
    coalesced_moves_total: u64,
    last_input_micros: Option<u64>,
    dirty_pixels_last: u64,
    pixels_touched_last: u64,
    misuse_count: u64,
}

impl DrawPerfStats {
    pub fn new(enabled: bool, rolling_window: usize) -> Self {
        Self {
            enabled,
            window_size: rolling_window.max(1),
            started_at: Instant::now(),
            next_span_id: 1,
            frame_ms_window: VecDeque::with_capacity(rolling_window.max(1)),
            raster_ms_window: VecDeque::with_capacity(rolling_window.max(1)),
            input_ms_window: VecDeque::with_capacity(rolling_window.max(1)),
            invalidate_ms_window: VecDeque::with_capacity(rolling_window.max(1)),
            pending_input_spans: HashMap::new(),
            pending_raster_spans: HashMap::new(),
            pending_invalidate: VecDeque::new(),
            points_window: VecDeque::new(),
            present_window: VecDeque::new(),
            input_to_present_ms_window: VecDeque::with_capacity(rolling_window.max(1)),
            coalesced_moves_total: 0,
            last_input_micros: None,
            dirty_pixels_last: 0,
            pixels_touched_last: 0,
            misuse_count: 0,
        }
    }

    pub fn disabled() -> Self {
        Self::new(false, DEFAULT_WINDOW_SIZE)
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled == enabled {
            return;
        }
        self.enabled = enabled;
        self.reset();
    }

    pub fn reset(&mut self) {
        self.started_at = Instant::now();
        self.next_span_id = 1;
        self.frame_ms_window.clear();
        self.raster_ms_window.clear();
        self.input_ms_window.clear();
        self.invalidate_ms_window.clear();
        self.pending_input_spans.clear();
        self.pending_raster_spans.clear();
        self.pending_invalidate.clear();
        self.points_window.clear();
        self.present_window.clear();
        self.input_to_present_ms_window.clear();
        self.coalesced_moves_total = 0;
        self.last_input_micros = None;
        self.dirty_pixels_last = 0;
        self.pixels_touched_last = 0;
        self.misuse_count = 0;
    }

    pub fn begin_input_ingestion(&mut self) -> Option<u64> {
        self.begin_span(SpanKind::Input)
    }

    pub fn end_input_ingestion(&mut self, span_id: Option<u64>, points_ingested: u32) {
        if !self.enabled {
            return;
        }
        let Some(span_id) = span_id else {
            return;
        };
        if let Some(ms) = self.end_span(SpanKind::Input, span_id) {
            Self::push_window(&mut self.input_ms_window, ms, self.window_size);
        }
        if points_ingested > 0 {
            self.last_input_micros = Some(self.now_micros());
        }
        self.record_points(points_ingested);
    }

    pub fn begin_raster(&mut self) -> Option<u64> {
        self.begin_span(SpanKind::Raster)
    }

    pub fn end_raster(&mut self, span_id: Option<u64>, estimated_pixels_touched: u64) {
        if !self.enabled {
            return;
        }
        let Some(span_id) = span_id else {
            return;
        };
        if let Some(ms) = self.end_span(SpanKind::Raster, span_id) {
            Self::push_window(&mut self.raster_ms_window, ms, self.window_size);
        }
        self.pixels_touched_last = estimated_pixels_touched;
    }

    pub fn mark_invalidate_requested(&mut self) {
        if !self.enabled {
            return;
        }
        self.pending_invalidate.push_back(self.now_micros());
    }

    pub fn mark_paint_completed(&mut self) {
        if !self.enabled {
            return;
        }
        let Some(started) = self.pending_invalidate.pop_front() else {
            self.misuse_count = self.misuse_count.saturating_add(1);
            return;
        };
        let ms = (self.now_micros().saturating_sub(started)) as f64 / 1000.0;
        Self::push_window(&mut self.invalidate_ms_window, ms, self.window_size);
    }

    pub fn finish_frame(&mut self, frame_ms: f64, dirty_pixels: u64) {
        if !self.enabled {
            return;
        }
        let now = self.now_micros();
        self.dirty_pixels_last = dirty_pixels;
        Self::push_window(&mut self.frame_ms_window, frame_ms, self.window_size);
        if let Some(last_input) = self.last_input_micros {
            let latency_ms = (now.saturating_sub(last_input)) as f64 / 1000.0;
            Self::push_window(
                &mut self.input_to_present_ms_window,
                latency_ms,
                self.window_size,
            );
        }
        self.present_window.push_back(now);
        self.prune_points_window();
        self.prune_present_window();
    }

    pub fn mark_coalesced_moves(&mut self, count: u64) {
        if !self.enabled {
            return;
        }
        self.coalesced_moves_total = self.coalesced_moves_total.saturating_add(count);
    }

    pub fn snapshot(&self) -> DrawPerfSnapshot {
        if !self.enabled {
            return DrawPerfSnapshot::default();
        }

        DrawPerfSnapshot {
            enabled: true,
            avg_ms: avg(&self.frame_ms_window),
            worst_ms: max(&self.frame_ms_window),
            p95_ms: p95(&self.frame_ms_window),
            raster_ms: avg(&self.raster_ms_window),
            input_ingest_ms: avg(&self.input_ms_window),
            invalidate_to_paint_ms: avg(&self.invalidate_ms_window),
            points_per_second: self.points_per_second(),
            effective_present_hz: self.effective_present_hz(),
            input_to_present_ms: avg(&self.input_to_present_ms_window),
            coalesced_moves: self.coalesced_moves_total,
            dirty_pixels: self.dirty_pixels_last,
            estimated_pixels_touched: self.pixels_touched_last,
            misuse_count: self.misuse_count,
            frame_samples: self.frame_ms_window.len(),
        }
    }

    fn begin_span(&mut self, kind: SpanKind) -> Option<u64> {
        if !self.enabled {
            return None;
        }
        let span_id = self.next_span_id;
        self.next_span_id = self.next_span_id.saturating_add(1);
        let now = self.now_micros();
        match kind {
            SpanKind::Input => {
                self.pending_input_spans.insert(span_id, now);
            }
            SpanKind::Raster => {
                self.pending_raster_spans.insert(span_id, now);
            }
        }
        Some(span_id)
    }

    fn end_span(&mut self, kind: SpanKind, span_id: u64) -> Option<f64> {
        let now = self.now_micros();
        let started = match kind {
            SpanKind::Input => self.pending_input_spans.remove(&span_id),
            SpanKind::Raster => self.pending_raster_spans.remove(&span_id),
        };
        let Some(started) = started else {
            self.misuse_count = self.misuse_count.saturating_add(1);
            return None;
        };
        Some((now.saturating_sub(started)) as f64 / 1000.0)
    }

    fn record_points(&mut self, points: u32) {
        if points == 0 {
            return;
        }
        self.points_window.push_back((self.now_micros(), points));
        self.prune_points_window();
    }

    fn points_per_second(&self) -> f64 {
        let total: u64 = self
            .points_window
            .iter()
            .map(|(_, points)| *points as u64)
            .sum();
        total as f64
    }

    fn prune_points_window(&mut self) {
        let now = self.now_micros();
        while let Some((stamp, _)) = self.points_window.front().copied() {
            if now.saturating_sub(stamp) <= 1_000_000 {
                break;
            }
            let _ = self.points_window.pop_front();
        }
    }

    fn effective_present_hz(&self) -> f64 {
        self.present_window.len() as f64
    }

    fn prune_present_window(&mut self) {
        let now = self.now_micros();
        while let Some(stamp) = self.present_window.front().copied() {
            if now.saturating_sub(stamp) <= 1_000_000 {
                break;
            }
            let _ = self.present_window.pop_front();
        }
    }

    fn push_window(window: &mut VecDeque<f64>, sample: f64, window_size: usize) {
        window.push_back(sample);
        while window.len() > window_size {
            let _ = window.pop_front();
        }
    }

    fn now_micros(&self) -> u64 {
        self.started_at.elapsed().as_micros() as u64
    }
}

#[derive(Debug, Clone, Copy)]
enum SpanKind {
    Input,
    Raster,
}

pub fn draw_perf_runtime_enabled(setting_enabled: bool) -> bool {
    if cfg!(debug_assertions) || setting_enabled {
        return true;
    }
    match std::env::var(DRAW_PERF_DEBUG_ENV) {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => false,
    }
}

fn avg(window: &VecDeque<f64>) -> f64 {
    if window.is_empty() {
        return 0.0;
    }
    window.iter().sum::<f64>() / window.len() as f64
}

fn max(window: &VecDeque<f64>) -> f64 {
    window.iter().copied().fold(0.0, f64::max)
}

fn p95(window: &VecDeque<f64>) -> f64 {
    if window.is_empty() {
        return 0.0;
    }
    let mut values: Vec<f64> = window.iter().copied().collect();
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((values.len() as f64 * 0.95).ceil() as usize).saturating_sub(1);
    values[idx.min(values.len().saturating_sub(1))]
}

#[cfg(test)]
mod tests {
    use super::{DrawPerfStats, DRAW_PERF_DEBUG_ENV};
    use once_cell::sync::Lazy;
    use std::sync::Mutex;
    use std::thread;
    use std::time::Duration;

    static ENV_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    #[test]
    fn rolling_window_avg_max_and_p95_respect_window_bounds() {
        let mut stats = DrawPerfStats::new(true, 3);
        stats.finish_frame(10.0, 100);
        stats.finish_frame(20.0, 100);
        stats.finish_frame(30.0, 100);
        stats.finish_frame(40.0, 100);
        let snap = stats.snapshot();
        assert_eq!(snap.frame_samples, 3);
        assert!((snap.avg_ms - 30.0).abs() < 0.001);
        assert_eq!(snap.worst_ms, 40.0);
        assert_eq!(snap.p95_ms, 40.0);
    }

    #[test]
    fn reset_clears_metrics_and_pending_spans() {
        let mut stats = DrawPerfStats::new(true, 8);
        let input = stats.begin_input_ingestion();
        thread::sleep(Duration::from_millis(1));
        stats.end_input_ingestion(input, 4);
        stats.mark_invalidate_requested();
        stats.mark_paint_completed();
        stats.finish_frame(6.0, 22);

        assert!(stats.snapshot().frame_samples > 0);
        stats.reset();
        let snap = stats.snapshot();
        assert_eq!(snap.frame_samples, 0);
        assert_eq!(snap.avg_ms, 0.0);
        assert_eq!(snap.points_per_second, 0.0);
    }

    #[test]
    fn disabled_mode_does_not_mutate_counters() {
        let mut stats = DrawPerfStats::disabled();
        let start = stats.begin_input_ingestion();
        stats.end_input_ingestion(start, 10);
        stats.mark_invalidate_requested();
        stats.mark_paint_completed();
        stats.finish_frame(15.0, 200);
        assert_eq!(stats.snapshot().frame_samples, 0);
        assert_eq!(stats.snapshot().misuse_count, 0);
    }

    #[test]
    fn span_pairing_misuse_increments_misuse_counter_without_panicking() {
        let mut stats = DrawPerfStats::new(true, 16);
        stats.end_input_ingestion(Some(999), 1);
        stats.end_raster(Some(888), 10);
        stats.mark_paint_completed();
        let snap = stats.snapshot();
        assert_eq!(snap.misuse_count, 3);
    }

    #[test]
    fn move_to_present_latency_and_present_rate_are_tracked() {
        let mut stats = DrawPerfStats::new(true, 16);
        let span = stats.begin_input_ingestion();
        std::thread::sleep(Duration::from_millis(1));
        stats.end_input_ingestion(span, 2);
        stats.mark_coalesced_moves(3);
        stats.finish_frame(4.0, 50);
        let snap = stats.snapshot();
        assert!(snap.input_to_present_ms >= 0.0);
        assert!(snap.effective_present_hz >= 1.0);
        assert_eq!(snap.coalesced_moves, 3);
    }

    #[test]
    fn runtime_gate_accepts_env_override() {
        let _guard = ENV_MUTEX.lock().expect("env mutex");
        std::env::set_var(DRAW_PERF_DEBUG_ENV, "1");
        assert!(super::draw_perf_runtime_enabled(false));
        std::env::remove_var(DRAW_PERF_DEBUG_ENV);
        if !cfg!(debug_assertions) {
            assert!(!super::draw_perf_runtime_enabled(false));
        }
    }
}
