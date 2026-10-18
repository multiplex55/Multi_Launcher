# Bounded workload and evidence contract

All fixtures are synthetic. No launcher startup, settings load, clipboard,
hotkey, desktop inventory or external action is part of a CPU probe.

| Scenario | Owner / intended measurement | Current disposition |
|---|---|---|
| hotfolder_disabled | `gui::hotfolder_runtime::driver_main` idle worker | supported real driver with fresh fixture; run through owned Job runner |
| hotfolder_enabled_zero_rules | same real driver, empty isolated store | supported real driver; no watched roots or actions |
| radial_8 | `radial::geometry::layout_menu` + `radial::render::build_scene` | verified direct CPU, 20 samples |
| radial_32 | same, 32 cells | verified direct CPU, 20 samples |
| radial_128 | same, 128 cells | verified direct CPU, 20 samples |
| dashboard_minimal | real dashboard/widget egui construction | verified CPU: 1 safe synthetic widget / 20 samples |
| dashboard_typical | same, synthetic bounded widgets | verified CPU: 4 widgets / 20 samples; pinned placeholder |
| dashboard_dense | same, synthetic bounded widgets | verified CPU: 9 widgets / 20 samples; pinned placeholders |

Radial uses the starter document's fixed default skin, one ring with stable
synthetic cell IDs/labels, scale 1, anchor (0,0), work area +/-10000 and
minimum scale 0.25, matching the existing radial benchmark's layout inputs. Validate cell
identities/count and nonempty scene output. Geometry/scene timing excludes
fixture allocation and records no font rasterization or native presentation.
`build_scene` uses empty prepared resources, matching the existing benchmark;
the result covers geometry and unprepared scene construction, not full rendering.

Default 3 warmups, 20 recorded samples; hard maximum 20 samples, 3 warmups,
90 seconds. One smoke batch per scenario. Timing is monotonic wall duration
of real CPU computation, not process CPU percentage or native responsiveness.
Native presentation remains null: Job containment alone does not prove desktop
access or a visible frame. Dashboard uses fixed 1280x720 points, scale 1, synthetic
time increments and no input. Safe widget types are quick_tools, pinned_commands
(empty placeholder), and frequent_commands; other widget paths are excluded.

One shared feature-only recorder owns unique ignored evidence directories,
schema/versioned events and atomic summary publication. Strict CLI parsing
precedes fixture mutation. Fixed statuses: success, error, timeout, unsupported,
blocked_environment, blocked_desktop, truncated. Missing metrics use null and
reasons. Raw logs <=32 MiB/run and aggregate <=100 MiB; no raw logs in Git.
Panic/error completion is best effort; a runner watchdog owns forced-terminal
reporting when a child cannot flush. No shell evaluation by the native runner.

Only opt-in `perf-harness` targets may be added; production startup, existing
smoke gates, profiles and algorithms remain unchanged. H0–H4 checkpoint order
and exact limits are defined by the approved master plan.
