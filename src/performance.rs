use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const PERF_ENV: &str = "MULTI_LAUNCHER_PERF";

static ENABLED: OnceLock<bool> = OnceLock::new();
static PROCESS_START: OnceLock<Instant> = OnceLock::new();
static FRAME_STATE: OnceLock<Mutex<FrameState>> = OnceLock::new();

#[derive(Debug)]
struct FrameState {
    first_frame_seen: bool,
    window_started: Instant,
    frames: u64,
    dashboard_repaint_requests: u64,
}

pub fn init_process_timer() {
    if enabled() {
        let _ = PROCESS_START.set(Instant::now());
    }
}

pub fn enabled() -> bool {
    *ENABLED.get_or_init(|| enabled_from_value(std::env::var(PERF_ENV).ok().as_deref()))
}

fn enabled_from_value(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

#[must_use]
pub struct Timer(Option<Instant>);

impl Timer {
    pub fn start() -> Self {
        Self(enabled().then(Instant::now))
    }

    pub fn start_if(enabled: bool) -> Self {
        Self(enabled.then(Instant::now))
    }

    pub fn finish(self, phase: &'static str) {
        if let Some(started) = self.0 {
            log_duration(phase, started.elapsed());
        }
    }

    pub fn finish_plugin(self, plugin: &str) {
        if let Some(started) = self.0 {
            log_plugin_duration(plugin, started.elapsed());
        }
    }
}

pub fn started() -> Option<Instant> {
    enabled().then(Instant::now)
}

pub fn started_if(enabled: bool) -> Option<Instant> {
    enabled.then(Instant::now)
}

pub fn log_elapsed(phase: &'static str, started: Option<Instant>) {
    if let Some(started) = started {
        log_duration(phase, started.elapsed());
    }
}

pub fn log_duration(phase: &'static str, duration: Duration) {
    if enabled() {
        tracing::info!(
            target: "multi_launcher::performance",
            phase,
            duration_ms = duration.as_secs_f64() * 1_000.0,
            "perf"
        );
    }
}

pub fn log_plugin_duration(plugin: &str, duration: Duration) {
    if enabled() {
        tracing::info!(
            target: "multi_launcher::performance",
            phase = "search.plugin",
            plugin,
            duration_ms = duration.as_secs_f64() * 1_000.0,
            "perf"
        );
    }
}

pub fn log_process_elapsed(phase: &'static str) {
    if enabled()
        && let Some(started) = PROCESS_START.get()
    {
        log_duration(phase, started.elapsed());
    }
}

pub fn record_dashboard_repaint_request() {
    if !enabled() {
        return;
    }
    if let Ok(mut state) = frame_state().lock() {
        state.dashboard_repaint_requests += 1;
    }
}

pub fn record_frame(visible: bool, focused: bool, dashboard: bool) {
    if !enabled() {
        return;
    }
    let Ok(mut state) = frame_state().lock() else {
        return;
    };
    state.frames += 1;
    if !state.first_frame_seen {
        state.first_frame_seen = true;
        drop(state);
        log_process_elapsed("startup.first_update");
        log_process_elapsed("startup.first_usable_frame");
        return;
    }
    let elapsed = state.window_started.elapsed();
    if elapsed >= Duration::from_secs(1) {
        tracing::info!(
            target: "multi_launcher::performance",
            phase = "runtime.frames",
            window_ms = elapsed.as_secs_f64() * 1_000.0,
            frames = state.frames,
            dashboard_repaint_requests = state.dashboard_repaint_requests,
            visible,
            focused,
            dashboard,
            "perf"
        );
        state.window_started = Instant::now();
        state.frames = 0;
        state.dashboard_repaint_requests = 0;
    }
}

fn frame_state() -> &'static Mutex<FrameState> {
    FRAME_STATE.get_or_init(|| {
        Mutex::new(FrameState {
            first_frame_seen: false,
            window_started: Instant::now(),
            frames: 0,
            dashboard_repaint_requests: 0,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::enabled_from_value;

    #[test]
    fn diagnostics_switch_accepts_only_explicit_truthy_values() {
        for value in ["1", "true", "TRUE", " yes ", "on"] {
            assert!(enabled_from_value(Some(value)), "{value}");
        }
        for value in [None, Some(""), Some("0"), Some("false"), Some("anything")] {
            assert!(!enabled_from_value(value));
        }
    }
}
