//! Explicit, bounded native sampling and HUD profile driven by the production
//! coordinate worker. This is intentionally separate from the fast smoke path.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use multi_launcher::coordinate_tool::{
    CoordinateSpace, CoordinateToolController, CoordinateToolPreferences, CoordinateUnavailable,
    CursorEffectStatus, HudDetail, PhysicalPoint, format_coordinate,
};
use multi_launcher::performance::{self, Metric, coordinate_profile::ProfileSession};
use serde::Serialize;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::System::Threading::{GR_GDIOBJECTS, GetCurrentProcess, GetGuiResources};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowRect, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos,
};

use super::{
    CountingFactory, Counts, DesktopRestore, DpiScope, Receiver, Target, compare_geometry,
    passive_windows, wait_sample,
};

const WARMUP: Duration = Duration::from_secs(2);
const MOVE_SECONDS: Duration = Duration::from_secs(60);
const PROBE_SECONDS: Duration = Duration::from_secs(5);
const DRIVE_INTERVAL: Duration = Duration::from_millis(4);

#[derive(Serialize)]
struct PhaseRun {
    name: &'static str,
    requested_seconds: f64,
    actual_seconds: f64,
    samples: usize,
    full_renders: usize,
    stationary_source_refreshes: usize,
    coordinate_sample_metric: MetricWindow,
    hud_gdi_create_metric: MetricWindow,
}

#[derive(Serialize)]
struct MetricWindow {
    calls_delta: u64,
    work_units_delta: u64,
    elapsed_nanos_total_delta: u64,
    cumulative_elapsed_nanos_max_at_end: u64,
}

#[derive(Serialize)]
struct ResourceSample {
    elapsed_ms: u64,
    phase: &'static str,
    process_gdi_objects: u32,
}

pub(super) fn run() -> Result<(), String> {
    if !performance::enabled() {
        return Err(
            "--coordinate-profile requires MULTI_LAUNCHER_PERF=1 before process start".into(),
        );
    }

    let _dpi = DpiScope::enter()?;
    let prior_desktop = DesktopRestore::save_prior()?;
    let mut receiver = Receiver::start()?;
    let mut restore = DesktopRestore::new(receiver.target, prior_desktop);
    super::require_foreground(receiver.target)?;

    println!(
        "coordinate-tool-smoke: PROFILE starting bounded native profile; keep receiver foreground, do not use mouse/keyboard; focus changes abort safely"
    );

    let counts = Arc::new(Counts::default());
    let gdi_baseline = process_gdi_count();
    let passive_before = passive_windows(std::process::id());
    if passive_before != (0, 0) {
        return Err(format!(
            "profile requires no passive HWNDs before activation, found {passive_before:?}"
        ));
    }
    // Both collectors are reset/activated before the coordinate worker exists.
    // The warm configuration work is retained in the native profile summary.
    performance::reset_metrics();
    let mut profile_session = ProfileSession::start()?;
    let profile_started = Instant::now();
    let mut resources = Vec::with_capacity(128);
    resources.push(ResourceSample {
        elapsed_ms: 0,
        phase: "profile_start_before_worker",
        process_gdi_objects: gdi_baseline,
    });
    let profile_worker = profile_session.attach_worker()?;
    let mut controller =
        CoordinateToolController::new(Arc::new(CountingFactory(Arc::clone(&counts))));
    let mut preferences = CoordinateToolPreferences::default();
    preferences.hud_detail = HudDetail::Detailed;
    controller.set_preferences(preferences)?;

    let idle_copy_rejected = controller.sample_for_copy().is_err();
    thread::sleep(Duration::from_millis(80));
    if controller.is_running()
        || !idle_copy_rejected
        || counts.sampler_creations.load(Ordering::Acquire) != 0
        || counts.backend_creations.load(Ordering::Acquire) != 0
        || counts.samples.load(Ordering::Acquire) != 0
        || passive_windows(std::process::id()) != (0, 0)
    {
        return Err("all-off controller created native work before activation".into());
    }

    let initial_points = client_motion_points(receiver.target.hwnd)?;
    let warm_point = initial_points[0];
    restore.move_to(warm_point)?;
    controller.set_hud_enabled(true)?;
    controller.set_crosshair_enabled(true)?;
    controller.set_halo_enabled(true)?;
    controller.set_zoom_enabled(true)?;
    let initial_sample = wait_sample(&controller, warm_point)?;
    let initial_geometry = compare_geometry(&initial_sample, receiver.target, warm_point)?;

    let warm_started = Instant::now();
    while warm_started.elapsed() < WARMUP {
        super::require_foreground(receiver.target)?;
        thread::sleep(DRIVE_INTERVAL);
    }
    let warm_seconds = warm_started.elapsed().as_secs_f64();
    let gdi_post_warmup = process_gdi_count();
    let warm_effect_status = controller.effects_status();
    resources.push(ResourceSample {
        elapsed_ms: profile_started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        phase: "post_active_warmup",
        process_gdi_objects: gdi_post_warmup,
    });

    let mut move_resize = MoveResizeEvidence::new(receiver.target)?;
    move_resize.move_and_resize()?;
    let points = client_motion_points(receiver.target.hwnd)?;
    let resized_point = points[0];
    restore.move_to(resized_point)?;
    let resized_sample = wait_sample(&controller, resized_point)?;
    let resized_geometry = compare_geometry(&resized_sample, receiver.target, resized_point)?;
    if initial_geometry.client.bounds == resized_geometry.client.bounds {
        return Err("receiver client bounds did not change after the move/resize probe".into());
    }
    let expected_client_point = PhysicalPoint::new(
        resized_point.x - resized_geometry.client.origin.x,
        resized_point.y - resized_geometry.client.origin.y,
    );
    let client_copy = format_coordinate(&resized_sample, CoordinateSpace::ForegroundClient)
        .map_err(|error: CoordinateUnavailable| format!("foreground-client copy: {error:?}"))?;
    if client_copy.point != expected_client_point {
        return Err(format!(
            "client-relative copy disagreed with the sampled client origin: {client_copy:?} vs {expected_client_point:?}"
        ));
    }

    // No collector reset occurs while the worker is live.
    let mut phases = Vec::with_capacity(6);

    let run_result: Result<(PhysicalPoint, PhysicalPoint, PhysicalPoint), String> = (|| {
        controller.disable_effects()?;
        restore.move_to(resized_point)?;
        wait_sample(&controller, resized_point)?;
        phases.push(run_phase(
            "moving_hud_only",
            MOVE_SECONDS,
            true,
            &mut controller,
            &mut restore,
            receiver.target,
            &points,
            &counts,
            profile_started,
            &mut resources,
        )?);

        phases.push(run_phase(
            "stationary_hud",
            PROBE_SECONDS,
            false,
            &mut controller,
            &mut restore,
            receiver.target,
            &points,
            &counts,
            profile_started,
            &mut resources,
        )?);

        controller.set_crosshair_enabled(true)?;
        controller.set_hud_enabled(false)?;
        phases.push(run_phase(
            "moving_crosshair",
            PROBE_SECONDS,
            true,
            &mut controller,
            &mut restore,
            receiver.target,
            &points,
            &counts,
            profile_started,
            &mut resources,
        )?);

        controller.set_halo_enabled(true)?;
        controller.set_crosshair_enabled(false)?;
        phases.push(run_phase(
            "moving_halo",
            PROBE_SECONDS,
            true,
            &mut controller,
            &mut restore,
            receiver.target,
            &points,
            &counts,
            profile_started,
            &mut resources,
        )?);

        controller.set_zoom_enabled(true)?;
        controller.set_halo_enabled(false)?;
        phases.push(run_phase(
            "moving_zoom",
            PROBE_SECONDS,
            true,
            &mut controller,
            &mut restore,
            receiver.target,
            &points,
            &counts,
            profile_started,
            &mut resources,
        )?);

        controller.set_hud_enabled(true)?;
        controller.set_zoom_enabled(true)?;
        restore.move_to(points[0])?;
        let before_freeze = wait_sample(&controller, points[0])?;
        controller.freeze();
        let frozen_sample = controller.sample_for_copy()?;
        if frozen_sample != before_freeze {
            return Err("freeze did not retain the sample shown before movement".into());
        }
        let sample_count_before_frozen_move = counts.samples.load(Ordering::Acquire);
        phases.push(run_phase(
            "moving_frozen_hud_live_zoom",
            PROBE_SECONDS,
            true,
            &mut controller,
            &mut restore,
            receiver.target,
            &points,
            &counts,
            profile_started,
            &mut resources,
        )?);
        if controller.sample_for_copy()? != frozen_sample {
            return Err("frozen HUD/copy sample changed while live zoom continued moving".into());
        }
        if counts.samples.load(Ordering::Acquire) <= sample_count_before_frozen_move {
            return Err("the worker did not continue sampling while the HUD was frozen".into());
        }
        let latest_cursor = points[1];
        restore.move_to(latest_cursor)?;
        controller.unfreeze();
        let live_after_unfreeze = wait_sample(&controller, latest_cursor)?;
        if live_after_unfreeze.desktop_point == frozen_sample.desktop_point {
            return Err("unfreeze did not expose the later live sample".into());
        }
        let live_geometry = compare_geometry(&live_after_unfreeze, receiver.target, latest_cursor)?;
        let delayed_client_copy =
            format_coordinate(&live_after_unfreeze, CoordinateSpace::ForegroundClient).map_err(
                |error: CoordinateUnavailable| format!("delayed client copy: {error:?}"),
            )?;
        let delayed_expected = PhysicalPoint::new(
            latest_cursor.x - live_geometry.client.origin.x,
            latest_cursor.y - live_geometry.client.origin.y,
        );
        if delayed_client_copy.point != delayed_expected {
            return Err("delayed copy used stale foreground-client geometry".into());
        }
        Ok((
            frozen_sample.desktop_point,
            live_after_unfreeze.desktop_point,
            delayed_client_copy.point,
        ))
    })();

    // Join the one real coordinate worker before closing its profile session or
    // collecting either summary. This is the only explicit native teardown.
    let profiled_effect_status = controller.effects_status();
    let shutdown = controller.shutdown();
    drop(profile_worker);
    let profile_summary = profile_session.finish();
    let gdi_after_shutdown = process_gdi_count();
    let passive_after_shutdown = passive_windows(std::process::id());
    let sample_count_after_shutdown = counts.samples.load(Ordering::Acquire);
    thread::sleep(Duration::from_millis(80));
    let samples_stayed_off = counts.samples.load(Ordering::Acquire) == sample_count_after_shutdown;
    let all_off_rejected_copy = controller.sample_for_copy().is_err();
    let ordinary = performance::snapshot_metrics();

    let (frozen_point, live_after_unfreeze_point, delayed_client_point) = run_result?;
    shutdown?;
    if !samples_stayed_off || !all_off_rejected_copy || passive_after_shutdown != (0, 0) {
        return Err(format!(
            "all-off cleanup failed: samples_stayed_off={samples_stayed_off}, copy_rejected={all_off_rejected_copy}, passive_hwnds={passive_after_shutdown:?}"
        ));
    }
    let profile_summary = profile_summary?;
    let total_profile_seconds = profile_started.elapsed().as_secs_f64();
    resources.push(ResourceSample {
        elapsed_ms: (total_profile_seconds * 1000.0) as u64,
        phase: "after_worker_shutdown",
        process_gdi_objects: gdi_after_shutdown,
    });

    let output = serde_json::json!({
        "schema_version": 1,
        "profile": "coordinate_native_sampling_and_hud",
        "activation": {
            "command": "coordinate_tool_smoke --coordinate-profile",
            "required_environment": "MULTI_LAUNCHER_PERF=1 before process start",
            "capacity_per_phase": profile_summary.capacity_per_phase,
        },
        "environment": {
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "receiver_hwnd": format!("{:#x}", receiver.target.hwnd.0 as usize),
            "same_receiver_moved_and_resized": true,
            "client_before_resize": {
                "origin": point_json(initial_geometry.client.origin),
                "bounds": initial_geometry.client.bounds.map(rect_json),
            },
            "client_after_resize": {
                "origin": point_json(resized_geometry.client.origin),
                "bounds": resized_geometry.client.bounds.map(rect_json),
            },
            "sampled_monitor": {
                "id": resized_geometry.monitor.id.0,
                "bounds": rect_json(resized_geometry.monitor.bounds),
                "work_area": rect_json(resized_geometry.monitor.work_area),
                "effective_dpi": resized_geometry.monitor.effective_dpi,
            },
        },
        "protocol": {
            "active_configuration_warmup_seconds": warm_seconds,
            "warmup_in_native_distributions": true,
            "profiled_seconds": total_profile_seconds,
            "ordinary_metrics_collection_scope": "starts before worker creation and includes all-off idle, active warmup, geometry/copy validation, named phases, and shutdown drain; per-phase metric deltas cover only each named run_phase interval",
            "ordinary_metric_delta_semantics": "per-phase deltas are adjacent boundary snapshots and can straddle an in-flight call; elapsed_nanos_max is cumulative through the interval end, not an interval-only maximum",
            "phases": phases,
            "resources_are_process_total_gdi_objects": true,
            "process_gdi_objects": {
                "baseline_before_native_surfaces": gdi_baseline,
                "post_warmup": gdi_post_warmup,
                "samples": resources,
                "after_worker_shutdown": gdi_after_shutdown,
            },
        },
        "native_profile": profile_summary,
        "existing_opt_in_metrics": {
            "coordinate_sample_includes_passive_and_click_owners": metric_json(ordinary[Metric::CoordinateSample as usize]),
            "hud_gdi_create_attempt_metric_preserved": metric_json(ordinary[Metric::HudGdiCreate as usize]),
        },
        "worker": {
            "sampler_creations": counts.sampler_creations.load(Ordering::Acquire),
            "backend_creations": counts.backend_creations.load(Ordering::Acquire),
            "samples": counts.samples.load(Ordering::Acquire),
            "full_renders": counts.renders.load(Ordering::Acquire),
            "stationary_source_refreshes": counts.stationary_refreshes.load(Ordering::Acquire),
            "backend_shutdowns": counts.shutdowns.load(Ordering::Acquire),
            "warm_configuration_status": {
                "halo": status_name(warm_effect_status.halo()),
                "zoom": status_name(warm_effect_status.zoom()),
            },
            "profile_end_status": {
                "halo": status_name(profiled_effect_status.halo()),
                "zoom": status_name(profiled_effect_status.zoom()),
            },
        },
        "copy_and_lifecycle_checks": {
            "initial_client_relative_copy": point_json(client_copy.point),
            "frozen_sample_point": point_json(frozen_point),
            "live_point_after_unfreeze": point_json(live_after_unfreeze_point),
            "delayed_client_copy": point_json(delayed_client_point),
            "all_off_rejects_copy": all_off_rejected_copy,
            "no_samples_after_worker_shutdown": samples_stayed_off,
            "passive_hwnds_before": passive_before,
            "passive_hwnds_after": passive_after_shutdown,
        },
        "measurement_limits": [
            "native phase timings measure API work, not visible display latency",
            "process GDI counts include unrelated objects in this process",
            "no desktop readback was performed during the resource profile",
            "mixed-DPI topology and other physical monitors are reported only if encountered",
        ],
    });
    println!(
        "COORDINATE_PROFILE_JSON {}",
        serde_json::to_string(&output).map_err(|e| e.to_string())?
    );
    drop(move_resize);
    drop(restore);
    receiver.close()?;
    Ok(())
}

fn run_phase(
    name: &'static str,
    requested: Duration,
    move_cursor: bool,
    controller: &mut CoordinateToolController,
    restore: &mut DesktopRestore,
    target: Target,
    points: &[PhysicalPoint],
    counts: &Counts,
    profile_started: Instant,
    resources: &mut Vec<ResourceSample>,
) -> Result<PhaseRun, String> {
    if !controller.is_running() {
        return Err(format!("coordinate worker stopped before phase {name}"));
    }
    let samples_before = counts.samples.load(Ordering::Acquire);
    let renders_before = counts.renders.load(Ordering::Acquire);
    let refreshes_before = counts.stationary_refreshes.load(Ordering::Acquire);
    let metric_before = performance::snapshot_metrics();
    let started = Instant::now();
    let mut cursor_index = 0_usize;
    let mut next_resource_sample = started + Duration::from_secs(1);
    while started.elapsed() < requested {
        super::require_foreground(target)?;
        if move_cursor {
            restore.move_to(points[cursor_index % points.len()])?;
            cursor_index = cursor_index.wrapping_add(1);
        }
        let now = Instant::now();
        if now >= next_resource_sample {
            resources.push(ResourceSample {
                elapsed_ms: profile_started.elapsed().as_millis().min(u64::MAX as u128) as u64,
                phase: name,
                process_gdi_objects: process_gdi_count(),
            });
            next_resource_sample = now + Duration::from_secs(1);
        }
        thread::sleep(DRIVE_INTERVAL);
    }
    let metric_after = performance::snapshot_metrics();
    Ok(PhaseRun {
        name,
        requested_seconds: requested.as_secs_f64(),
        actual_seconds: started.elapsed().as_secs_f64(),
        samples: counts.samples.load(Ordering::Acquire) - samples_before,
        full_renders: counts.renders.load(Ordering::Acquire) - renders_before,
        stationary_source_refreshes: counts.stationary_refreshes.load(Ordering::Acquire)
            - refreshes_before,
        coordinate_sample_metric: metric_window(
            metric_before[Metric::CoordinateSample as usize],
            metric_after[Metric::CoordinateSample as usize],
        ),
        hud_gdi_create_metric: metric_window(
            metric_before[Metric::HudGdiCreate as usize],
            metric_after[Metric::HudGdiCreate as usize],
        ),
    })
}

fn metric_window(
    before: multi_launcher::performance::MetricSnapshot,
    after: multi_launcher::performance::MetricSnapshot,
) -> MetricWindow {
    MetricWindow {
        calls_delta: after.calls.saturating_sub(before.calls),
        work_units_delta: after.work_units.saturating_sub(before.work_units),
        elapsed_nanos_total_delta: after
            .elapsed_nanos_total
            .saturating_sub(before.elapsed_nanos_total),
        cumulative_elapsed_nanos_max_at_end: after.elapsed_nanos_max,
    }
}

struct MoveResizeEvidence {
    target: Target,
    original: RECT,
}

impl MoveResizeEvidence {
    fn new(target: Target) -> Result<Self, String> {
        let mut original = RECT::default();
        unsafe { GetWindowRect(target.hwnd, &mut original) }
            .map_err(|error| format!("GetWindowRect before resize: {error}"))?;
        Ok(Self { target, original })
    }

    fn move_and_resize(&mut self) -> Result<(), String> {
        let left = unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
                windows::Win32::UI::WindowsAndMessaging::SM_XVIRTUALSCREEN,
            )
        };
        let top = unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
                windows::Win32::UI::WindowsAndMessaging::SM_YVIRTUALSCREEN,
            )
        };
        let desktop_width = unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
                windows::Win32::UI::WindowsAndMessaging::SM_CXVIRTUALSCREEN,
            )
        };
        let desktop_height = unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
                windows::Win32::UI::WindowsAndMessaging::SM_CYVIRTUALSCREEN,
            )
        };
        let width = 640.min(desktop_width - 48).max(320);
        let height = 480.min(desktop_height - 48).max(240);
        unsafe {
            SetWindowPos(
                self.target.hwnd,
                HWND::default(),
                left + 32,
                top + 32,
                width,
                height,
                SWP_NOACTIVATE | SWP_NOZORDER,
            )
        }
        .map_err(|error| format!("move/resize the same receiver HWND: {error}"))
    }
}

impl Drop for MoveResizeEvidence {
    fn drop(&mut self) {
        if !unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindow(self.target.hwnd) }.as_bool()
        {
            return;
        }
        let _ = unsafe {
            SetWindowPos(
                self.target.hwnd,
                HWND::default(),
                self.original.left,
                self.original.top,
                self.original.right.saturating_sub(self.original.left),
                self.original.bottom.saturating_sub(self.original.top),
                SWP_NOACTIVATE | SWP_NOZORDER,
            )
        };
    }
}

fn client_motion_points(hwnd: HWND) -> Result<Vec<PhysicalPoint>, String> {
    let mut client = RECT::default();
    unsafe { windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut client) }
        .map_err(|error| format!("GetClientRect motion path: {error}"))?;
    let mut origin = windows::Win32::Foundation::POINT {
        x: client.left,
        y: client.top,
    };
    let mut bottom_right = windows::Win32::Foundation::POINT {
        x: client.right,
        y: client.bottom,
    };
    if !unsafe { windows::Win32::Graphics::Gdi::ClientToScreen(hwnd, &mut origin) }.as_bool()
        || !unsafe { windows::Win32::Graphics::Gdi::ClientToScreen(hwnd, &mut bottom_right) }
            .as_bool()
    {
        return Err("ClientToScreen failed for profile motion path".into());
    }
    let margin = 24;
    let width = bottom_right.x - origin.x;
    let height = bottom_right.y - origin.y;
    if width <= margin * 2 || height <= margin * 2 {
        return Err(format!(
            "receiver client area is too small: {width}x{height}"
        ));
    }
    let range_x = width - margin * 2;
    let range_y = height - margin * 2;
    let mut points = Vec::with_capacity(128);
    for i in 0..128_i32 {
        let x_step = (i * 73 + i * i * 7) % range_x;
        let y_step = (i * 41 + i * i * 13) % range_y;
        points.push(PhysicalPoint::new(
            origin.x + margin + x_step,
            origin.y + margin + y_step,
        ));
    }
    Ok(points)
}

fn process_gdi_count() -> u32 {
    unsafe { GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS) }
}

fn status_name(status: &CursorEffectStatus) -> &'static str {
    match status {
        CursorEffectStatus::Disabled => "disabled",
        CursorEffectStatus::Prepared => "prepared",
        CursorEffectStatus::Active => "active",
        CursorEffectStatus::Fallback(_) => "fallback",
        CursorEffectStatus::Paused => "paused",
        CursorEffectStatus::GeometryPaused(_) => "geometry_paused",
        CursorEffectStatus::Unavailable(_) => "unavailable",
    }
}

fn rect_json(rect: multi_launcher::coordinate_tool::PhysicalRect) -> serde_json::Value {
    serde_json::json!({
        "left": rect.left(),
        "top": rect.top(),
        "right": rect.right(),
        "bottom": rect.bottom(),
    })
}

fn point_json(point: PhysicalPoint) -> serde_json::Value {
    serde_json::json!({ "x": point.x, "y": point.y })
}

fn metric_json(snapshot: multi_launcher::performance::MetricSnapshot) -> serde_json::Value {
    serde_json::json!({
        "name": snapshot.metric.name(),
        "calls": snapshot.calls,
        "work_units": snapshot.work_units,
        "elapsed_nanos_total": snapshot.elapsed_nanos_total,
        "elapsed_nanos_max": snapshot.elapsed_nanos_max,
        "completed": snapshot.completed,
        "errors": snapshot.errors,
        "abandoned": snapshot.abandoned,
    })
}
