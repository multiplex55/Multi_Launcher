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

## M2-A — isolate repeated GUI oracle setup

### Implementation

The four selected GUI oracle paths now construct their app through M1-A's
shared `LauncherApp::new_isolated_for_test_at` / `IsolatedTest` initializer.
That entry point accepts optional enabled-plugin and capability snapshots and
forwards them to the shared initializer. The oracle path passes
`Some(HashSet::new())` and `Some(HashMap::new())` during construction, retaining
the previous explicit-empty-provider semantics rather than replacing them
with `None` or mutating enabled state after initialization. The existing
shortcut settings, disabled toast/dashboard settings, synthetic results, and
independent eager geometry expectations remain in the shared root-list fixture
builder.

Initialization selection is explicit at the four oracle call sites. Existing
general root-list setup, benchmark setup, and the C2G FreshPerFrame validator
continue to use the normal constructor. This avoids changing unrelated GUI
tests or benchmark protocol. M2-B watcher-resource migration was not included.

### Coverage and invariants

The selected roster stayed exactly the same four tests. It retains list cold
and warm geometry; grid cold and warm geometry at columns 1, 2, 3, 5, and 6;
the 100-action root-query geometry/focus assertion; and all C2G cases at 100
actions plus viewport controls at 1,000 and 10,000 actions. C2G retains all
nine input steps and ticks 9–13 and 32–35, with query, geometry, focus,
navigation, selection, and paint assertions. The existing cache invalidation
and corruption tests were not weakened or altered. The C2G eager-input cache
continues to use its existing `ReuseEquivalentInputs` path in this oracle;
no mutable GUI or geometry state is carried between unrelated scenarios.

### Bounded comparable evidence

One unchanged-source baseline and one candidate run used default features,
Cargo `fast-dev`, Nextest defaults and concurrency, the exact four-test
filter, and `--success-output immediate`. Both runs used the checked
`dev/track_b/measure.ps1` owner with unknown cache/work classification and
passed cleanup and output-drain verification. Neither selected run added Cargo
build selectors or overrides. The baseline reused prepared M1-B metadata only
after checking the bound source and binary; candidate preparation was a
separate 161.393600-second checked compile stage. Metadata-only inventories
were separate, bounded provenance stages.

The unchanged-source baseline on HEAD
`064fcbc72841a0e087f75c2b7df2dd61d877ffc5` passed 4/4 in 18.305994 seconds
(2026-10-25 19:32:00.6846841–19:32:18.9906930 UTC). The candidate passed 4/4
in 16.683376 seconds (2026-10-25 19:39:53.4923353–19:40:10.1757274 UTC), a
single-pair 1.622618-second / 8.863862% reduction in aggregate selected-run
wall time. Nextest per-case observations were:

| Oracle | Baseline | Candidate |
| --- | ---: | ---: |
| Root query | 0.864 s | 0.542 s |
| Root list | 1.692 s | 1.132 s |
| Root grid | 7.809 s | 5.402 s |
| C2G query sequences | 17.526 s | 15.900 s |

The candidate also retains the same six C2G fixture signatures as baseline.
Its `prepare_complete` telemetry is not an initializer-only measure and was
mixed: for the six recorded scenarios it ranged from 0.359883 to 2.480445
seconds, while baseline ranged from 0.346709 to 2.367275 seconds. That phase
ends before app teardown. The source change removes production dashboard and
hotfolder runtime ownership from these oracle apps; those owners otherwise
start and join/reap workers. The aggregate selected-run reduction is measured,
but attributing that reduction to a particular startup or teardown subphase
is an inference; no isolated constructor/drop timing was collected. No claim
is made that the `prepare_complete` phase itself became faster. This is one
comparable timing pair, not a general benchmark claim.

### Commands and evidence

Preparation used the successful M1-B command shape with the same package,
library target, Cargo profile, and verified MSVC VCTIP policy:

```text
cargo nextest list --list-type binaries-only --message-format json --manifest-path G:\Repos\rust\Multi_Launcher\Cargo.toml -p multi_launcher --lib --cargo-profile fast-dev
```

Prepared metadata SHA256 was
`f4985b1447d3a0920b59c50724c04d8d2201b7521b56c381ea52216aae187a2e`.
Both metadata snapshots name
`target/fast-dev/deps/multi_launcher-b96e8e61417c6153.exe`; the executable was
SHA256 `569e86a61bb90e8ee56b2b71c107c788d1447bfd1def4491c0794b53411dfb7a`
for baseline and `a7a5a65e052e4b28163862bd44adfc6ec26a95d51036901e6a0bd27b61d43a3d`
for candidate. Candidate source fingerprint SHA256 was
`6887bc47ab75a3deada7e4fd15fb017ec107366fa170daf56157a0b7986f4e44`; its
three owned source hashes are recorded in the candidate preparation receipt.
The baseline source fingerprint was
`328a8425d1a9b47cc84b97dc89525a1fedd500be246fd10715ecc1eea93b063d`.

For each inventory and selected run, the prepared metadata path replaced
`<metadata>` below. The inventory had a 300-second deadline; the selected run
had a 600-second deadline. No package, target, feature, profile, or concurrency
selector was added to these metadata-only commands.

```text
cargo nextest list --binaries-metadata <metadata> --message-format json -E 'test(=gui::render::tests::track_c2g_oracle_query_sequences_match_usable_cpu_frames) | test(=gui::render::tests::track_c_oracle_root_grid_frame_matches_frozen_eager_geometry) | test(=gui::render::tests::track_c_oracle_root_list_frame_matches_frozen_eager_geometry) | test(=gui::render::tests::track_c_oracle_root_query_reaches_current_usable_cpu_frame)'
cargo nextest run --binaries-metadata <metadata> -E 'test(=gui::render::tests::track_c2g_oracle_query_sequences_match_usable_cpu_frames) | test(=gui::render::tests::track_c_oracle_root_grid_frame_matches_frozen_eager_geometry) | test(=gui::render::tests::track_c_oracle_root_list_frame_matches_frozen_eager_geometry) | test(=gui::render::tests::track_c_oracle_root_query_reaches_current_usable_cpu_frame)' --success-output immediate
```

The baseline inventory first preserved two metadata-inventory CLI failures:
Nextest rejected the unsupported `--format` option, then rejected Cargo build
selectors combined with `--binaries-metadata`. Neither launched tests. The
corrected metadata-only inventory succeeded and selected exactly the four
identities above. Candidate metadata-only inventory also succeeded and
selected those same four identities from 6,344 listed library cases.

External evidence root:
`C:\Users\Jay\.codex\visualizations\2026\10\25\01a17319-6f7d-7052-b54e-1bb4a92c5fd0\multi-launcher-queue-20261025-eedc19d2\g02-m2a\`

- Baseline inventory failures and successful inventory: `baseline-four-oracle\g02-m2a-baseline-four-oracle-inventory_20261025T193048307Z_88dac7ce84384967956088c843d7ab88\measurement.json`, `baseline-four-oracle\g02-m2a-baseline-four-oracle-inventory-corrected_20261025T193119035Z_23c0f0f8dfbd4b719031dde8dd949916\measurement.json`, and `baseline-four-oracle\g02-m2a-baseline-four-oracle-inventory-metadata-only_20261025T193138600Z_465a18600a704fb2a59c3716daae2e22\measurement.json`.
- Baseline run and retained passing telemetry: `baseline-four-oracle\g02-m2a-baseline-four-oracle_20261025T193200332Z_42cd9d2b3bdb451a9b2f6c8cf6e63a2a\measurement.json` and `stderr.bin`.
- Candidate compile metadata and binary provenance: `candidate-four-oracle\g02-m2a-candidate-compile-prep_20261025T193613519Z_93426c93c64e4196876c54c97ad630a2\measurement.json` and `stdout.bin`.
- Candidate metadata-only inventory: `candidate-four-oracle\g02-m2a-candidate-inventory-metadata-only_20261025T193929722Z_00e39fc9290e433f8a8c2a384078ddb7\measurement.json` and `stdout.bin`.
- Candidate run and retained passing telemetry: `candidate-four-oracle\g02-m2a-candidate-four-oracle_20261025T193953146Z_e68661776c2e4de1a6e5f49def9d5c34\measurement.json` and `stderr.bin`.

`cargo fmt --all` and `git diff --check` passed; incidental formatting churn
from the workspace-wide formatter was narrowly reverted. The checked candidate
preparation compiled the library tests, and the exact selected candidate run
passed 4/4. No full suite, early Clippy run, repeated timing campaign, or M2-B
resource migration was run. Parent retains Git, final review, and checkpoint
ownership.
