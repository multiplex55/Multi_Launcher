# Goal 02 execution record

Queue: `multi-launcher-queue-20261025-eedc19d2`  
Goal: `CODEX_GOAL_02_TEST_EFFICIENCY.md`  
Goal SHA256: `4743A61B3EB97E5A89025A0DA5582DD9A3E36038ADE40DE37B92EB8BABDCBCC9`  
Kickstart SHA256: `66B4395523B9229AD12E08B255582CBD0D9B4E9717C0B5CBDC393C1ED031C0D2`  
Queue Kickstart SHA256: `EEDC19D2F26FA22DD3A8B9370C7909C398623312B66632D686F7091F1FB29F3B`  
Task branch: `codex/perf-isolated-test-fixtures-queue-20261025`  
M0 baseline: `0ce995589be62db8c2bfeec79e7827ff653f1778`

## M1-A — explicit isolated app fixtures and small-test migration

### Implementation

Added a private shared `LauncherApp` initializer with a test-only isolated
policy. Ordinary `LauncherApp::new` keeps production service construction and
defaults. The isolated entry point disables Paste Sequences next/cancel
shortcuts before construction, takes explicit profile paths, and uses an inert
plugin manager with a snapshot-backed, worker-free window catalog. It leaves
the real watcher integration test on its real filesystem event path. Unrelated
watchers, runtime workers, shortcut pollers and process-global macro services
are dormant only under the isolated test policy; tests can still exercise real
services through their existing production paths.

The shared fixture owns its app and workspace lifetime. It scopes current
directory and legacy notes/template environment paths to each profile, drops
the app before restoring process state or deleting the profile, and exposes
fallible `finish()` cleanup. Its unwind fallback restores state and removes
temporary data; the lifecycle test verifies a fresh fixture can be acquired
after unwinding. Two profiles are constructed sequentially under one process
state guard and prove persisted settings, dashboard configuration, and note
data remain distinct.

The two dashboard app cases moved from `tests/dashboard_config.rs` into
`gui::tests` in `src/gui/mod.rs`; the generic render consumer in
`src/gui/render.rs` also uses the fixture. `tests/watchers.rs` keeps the real
watcher case and scopes its app construction. The integrated test catalog
contains workflow selections, not these case identities, so no catalog entry
needed migration.

### Identity mapping

| Before | After | Result |
| --- | --- | --- |
| `dashboard_config::should_show_dashboard_when_empty_query` | `gui::tests::should_show_dashboard_when_empty_query` | Same behavioral assertion; moved from integration target to library unit target. |
| `dashboard_config::activation_applies_query_override_first` | `gui::tests::activation_applies_query_override_first` | Same behavioral assertion; moved from integration target to library unit target. |
| `gui::render::tests::launcher_initialization_restores_macro_property_preferences` | same identity | Representative generic render consumer now uses the isolated fixture. |
| Four `gui::render::tests::track_c*oracle*` cases | same identities | Existing independent rendering oracles retained. |
| `watchers::actions_watcher_sends_event` | same identity | Real watcher delivery retained. |

### Bounded verification

Baseline and candidate used default features, Cargo profile `fast-dev`, and
targets `--lib --test dashboard_config --test watchers`. Nextest prepared
binaries-only JSON separately from selected execution. Preparation was bounded
at 1800 seconds, inventory at 300 seconds, and each selected run at 600
seconds. Both preparations and inventories had nonzero expected test counts.

The baseline inventory selected 8 cases. Its selected run passed 8/8 in
18.431175 seconds. Baseline source HEAD was `0ce995589be62db8c2bfeec79e7827ff653f1778`
with premeasurement hygiene in `src/gui/render.rs`,
`tests/dashboard_config.rs`, `tests/watchers.rs`, and `tests/common/mod.rs`;
the normalized source fingerprint was
`eafe0c7f3c9899ce804fed66387a7c44e3a5a77df06d684a8c376495c0d17a10`.

The final candidate inventory selected the exact 10-case roster below. One
prepared-metadata run passed 10/10 in 17.888760 seconds. The source HEAD was
unchanged from baseline; the candidate dirty map is in its receipt and its
source fingerprint is
`c58e3e26e01a68a64c3ec27f7d54ee2dde1227192b4b6c84661b32aa94225297`.
This is not a whole-batch speedup comparison: the candidate adds the profile
isolation and unwind cases, and the two moved cases have new test identities
and fixture costs. Nextest reports per-case elapsed wall time, not test-body
time. Passing-test telemetry did not retain separate fixture setup/body/drop
durations, so no such breakdown or zero-cost fixture claim is made.

Common unchanged-identity per-case observations (baseline → candidate) were:

| Case | Baseline | Candidate |
| --- | ---: | ---: |
| Generic macro-property render consumer | 0.598 s | 0.480 s |
| Query render oracle | 0.999 s | 0.988 s |
| List render oracle | 1.763 s | 1.811 s |
| Grid render oracle | 8.091 s | 8.020 s |
| Query-sequence render oracle | 17.653 s | 17.100 s |
| Real watcher delivery | 0.682 s | 0.572 s |

The migrated dashboard cases were 0.620 s and 0.681 s under their old
integration identities; their new unit identities took 0.417 s and 0.543 s.
These are descriptive observations only: target and fixture costs changed,
and candidate concurrency included two additional cases.

The first candidate preparation failed on a test-only dormant-service
reference from an ordinary build. The shared initializer was corrected with a
cfg-gated test factory boundary; the original receipt remains preserved. A
subsequent inventory filter matched only 8 cases because it did not qualify
the moved names. It launched no tests. After correcting only that filter, the
already-prepared metadata was hash-verified and reused; the exact 10-case
inventory and final run succeeded. No source changed between preparation,
the corrected inventory, and selected execution.

The successful selected roster was:

1. `gui::render::tests::launcher_initialization_restores_macro_property_preferences`
2. `gui::render::tests::track_c2g_oracle_query_sequences_match_usable_cpu_frames`
3. `gui::render::tests::track_c_oracle_root_grid_frame_matches_frozen_eager_geometry`
4. `gui::render::tests::track_c_oracle_root_list_frame_matches_frozen_eager_geometry`
5. `gui::render::tests::track_c_oracle_root_query_reaches_current_usable_cpu_frame`
6. `gui::tests::activation_applies_query_override_first`
7. `gui::tests::isolated_fixture_drop_restores_process_state_during_unwind`
8. `gui::tests::isolated_profiles_load_persisted_settings_without_cross_contamination`
9. `gui::tests::should_show_dashboard_when_empty_query`
10. `actions_watcher_sends_event`

### Evidence

External evidence root:
`C:\Users\Jay\.codex\visualizations\2026\10\25\01a17319-6f7d-7052-b54e-1bb4a92c5fd0\multi-launcher-queue-20261025-eedc19d2`

- Baseline selected inventory/run: `g02-m1a-baseline\baseline-summary.json`
  and `g02-m1a-baseline\g02-m1a-baseline-run_20261025T183823710Z_0ddca96b4cb542d2a241081f4e4e3efa\measurement.json`.
- Final candidate inventory/run: `g02-m1a-candidate-exact\candidate-summary.json`,
  `g02-m1a-candidate-exact\candidate-inventory-summary.json`, and
  `g02-m1a-candidate-exact\g02-m1a-candidate-exact-run_20261025T190100191Z_d46c82f09b6d486e847faa55dfc3b05d\measurement.json`.
- Final preparation receipt and binaries metadata are in
  `g02-m1a-candidate-corrected`; metadata SHA256 is
  `999b5570b9d9fedd2f8ffeafeb0e70ebfe9687fbd490f22db3f93ccf64121b4f`.
- The failed first preparation and incomplete-filter inventory receipts remain
  in their original evidence directories.

`git diff --check` passed after formatting. The configured reviewer completed
a read-only review of the source correction and found no remaining source
findings. No full suite, early Clippy run, or Goal 01 acceptance campaign was
run. The parent owns the final Goal Clippy decision and Git checkpoint.
