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

## Accepted M1-B record — bounded macro broker waits

M1-B replaced the two selected broker tests' polling and unbounded yielding
with finite completion notifications; it also added a focused helper case for
prearrival, timeout and sender-disconnection behavior. Production broker
cancellation, debounce behavior and OS watcher delivery were unchanged. The
candidate inventory selected exactly these three library identities, and all
three passed:

- `gui::render::tests::macro_launcher_poll_consumes_one_request_and_mutates_query_on_gui_dispatch`
- `gui::render::tests::disconnected_macro_launcher_waiter_does_not_panic_gui_dispatch`
- `gui::render::tests::completion_wait_handles_prearrival_timeout_and_disconnection`

Candidate run evidence is `g02-m1b-candidate/g02-m1b-candidate-three-case-run_20261025T191846781Z_98764f545d084cd8954c4e6ff5d56322/measurement.json` under the external queue evidence root. The run passed 3/3 in 0.363 seconds of Nextest time and 1.102606 seconds of outer process wall time. The removed polling interval was below useful timing resolution, and fixture construction dominated the candidate's observed time; this is coverage and responsiveness evidence, not a speedup claim. The complete preparation, inventory, run and retained failures are recorded in external `g02-m1b-execution-record.md`.

The configured Reviewer could not be dispatched once because the agent thread
limit had been reached. Under the user's explicit fallback, the parent made a
structured review and accepted the checkpoint. That review is not described as
independent.

## M2-B — real GUI watcher lifetime and resource policy

The M1-A section above records the earlier checkpoint, when the Actions watcher
case still lived in `tests/watchers.rs`. M2-B moved the three real file-delivery
cases into the owning library module, deleted that integration target and its
now-unused `tests/common/mod.rs` helper, and kept their event path unchanged:
the existing save function writes a file, the shared production `watch_file`
callback observes it, and the app queue receives the expected event within the
existing three-second deadline.

`LauncherInitPolicy::IsolatedWatcherTest` is private and `cfg(test)`. It enables
only the existing Actions, Folders and Bookmarks GUI watcher registrations.
The same constructor uses `PluginManager::new_inert_for_test_at`, explicit
workspace paths, empty enabled-plugin/capability snapshots, and disables the
test-owned launcher and Paste Sequences shortcuts before construction. Macro
search configuration, mouse gesture synchronization, dashboard/configuration
watchers, Hot Folder startup, window-catalog workers and the remaining
production services stay behind the production-only policy. The fixture proves
that policy directly; it does not reset or assume that process-global
macro/gesture `OnceCell` state was empty before a test process started. The
existing isolated-test assertion remains strict and still requires an empty
watcher list.

The migrated identities and the new unwind guard case are:

| Prior integration identity | Final library identity | Proof |
| --- | --- | --- |
| `multi_launcher::watchers / actions_watcher_sends_event` | `gui::watch::tests::actions_watcher_sends_event` | Real action-file save produces the Actions event. |
| `multi_launcher::watchers / folders_watcher_sends_event` | `gui::watch::tests::folders_watcher_sends_event` | Real folders-file save produces the Folders event. |
| `multi_launcher::watchers / bookmarks_watcher_sends_event` | `gui::watch::tests::bookmarks_watcher_sends_event` | Real bookmarks-file save produces the Bookmarks event. |
| — | `gui::watch::tests::real_watcher_fixture_unwind_releases_callbacks_before_workspace_cleanup` | A caught panic while the app is live still releases callbacks before restoring process state and removing the root. |

`HandlerOwnedWorkspace` owns the CWD/environment/root before the app is
constructed. The release probe starts before watcher registration. Cleanup
drops the app first, waits at most five seconds for the captured callback
closures, checks the exact nonzero Actions/Folders/Bookmarks attempts,
registrations and releases with no overflow, then drops the workspace and
asserts restored CWD/environment and root removal. The fixture's unwind guard
retains the whole workspace if release is incomplete. The Windows `notify`
backend uses detached threads: these receipts establish callback-closure
release, not backend-thread joins or process retirement.

The default-feature library preparation used Cargo profile `fast-dev` and the
checked `dev/track_b/measure.ps1` driver with unknown cache classification,
an 1,800-second deadline and verified MSVC VCTIP policy. It succeeded in
184.136024 seconds. Prepared metadata SHA256 was
`f4985b1447d3a0920b59c50724c04d8d2201b7521b56c381ea52216aae187a2e`. The
metadata-only inventory selected exactly the four non-ignored identities above
from the library target in 0.915965 seconds. It used Nextest's default profile
and did not pass Cargo manifest, package, target or profile selectors alongside
`--binaries-metadata`.

The bounded preparation, inventory and run used these argument shapes (the
measurement driver supplied the 1,800/300/600-second deadlines and owned
process verification):

```text
cargo nextest list --list-type binaries-only --message-format json --manifest-path G:\Repos\rust\Multi_Launcher\Cargo.toml -p multi_launcher --lib --cargo-profile fast-dev
cargo nextest list --binaries-metadata <metadata> --message-format json -E 'test(=gui::watch::tests::actions_watcher_sends_event) | test(=gui::watch::tests::folders_watcher_sends_event) | test(=gui::watch::tests::bookmarks_watcher_sends_event) | test(=gui::watch::tests::real_watcher_fixture_unwind_releases_callbacks_before_workspace_cleanup)'
cargo nextest run --binaries-metadata <metadata> --no-tests=fail -E 'test(=gui::watch::tests::actions_watcher_sends_event) | test(=gui::watch::tests::folders_watcher_sends_event) | test(=gui::watch::tests::bookmarks_watcher_sends_event) | test(=gui::watch::tests::real_watcher_fixture_unwind_releases_callbacks_before_workspace_cleanup)' --success-output immediate
```

The selected command used ordinary Nextest concurrency and
`--success-output immediate`; it passed 4/4 (6,344 skipped) in 0.390 seconds
of Nextest time and 1.146422 seconds of measured process wall time. Each test
reported `attempted=[GuiActions, GuiBookmarks, GuiFolders]`, the same three
registered and released, `complete=true`, and `overflowed=false`. The caught
intentional panic appears in the retained stderr output, followed by a passing
test and a complete release receipt. Owned-process cleanup and output-drain
verification passed. These are descriptive costs for the moved tests and new
unwind proof, not an equivalent before/after timing comparison or a speedup
claim.

Exact M2-B receipts are under the external queue evidence root in `g02-m2b/`:

- Preparation: `g02-m2b-candidate-compile-prep_20261025T195235380Z_bb37b1bd23c54b7bafda7bffa56ec9e5/measurement.json` and `stdout.bin`.
- Inventory: `g02-m2b-candidate-inventory_20261025T195610007Z_09d16a90bfe44c00933af746893d40e3/measurement.json` and `stdout.bin`; the exact roster is also in `candidate-inventory-summary.json`.
- Selected run and callback telemetry: `g02-m2b-candidate-four-case-run_20261025T195612717Z_7485f910e8f842daa7a2a127330f7fa2/measurement.json` and `stderr.bin`; `candidate-summary.json` records metadata, profile, identity and source-hash details.

The four source hashes captured during preparation and the selected run were
`src/gui/mod.rs=e53171424414d0a36e22bb8ec8a62d3ea359a91c37a1942b97bdaeb35080d242`,
`src/gui/watch.rs=28d2dae0bbecd0b535627322cd5b9e1158adc389e10f3829da76a273e2e3392c`,
`src/performance/track_c_watcher_lifecycle.rs=84c59ff472f5fea980df1ebc1c72aa97b25c978bb0d83f353761d11fa3987660`,
and `src/performance/track_c_workloads.rs=e1b76147d1960c723913f9076278d0c81be5cc6b8d12e5b392687b4c201c7a87`.
After the selected run, the sole source correction changed the equivalent root
absence assertion's syntax; the final `src/gui/watch.rs` hash is
`70099d69f26a1b76486f1683ed9e66cd4980a0c89bf6e36dc2fc80500d17b33b`. The
other three hashes remain unchanged. The measured predicate already passed;
there was no repeat selected run for this syntax-only correction.

No catalog selector names these watcher cases, so `dev/workflows/catalog.json`
is unchanged. The selected tests use unique temporary files and do not compete
for a shared exclusive native resource. Ordinary Nextest concurrency is
retained; no `.config/nextest.toml` or fabricated resource group was added. The
existing process-local workspace lock protects CWD/environment mutation but is
not presented as a cross-process resource lock. The M2-B candidate binary had
6,348 default-discovered library cases after migration, with the selected four
run and 6,344 skipped. No full suite, native personal-input test or broad GUI
watcher suite was run.

After the passing selected run, one assertion was rewritten from boolean
`assert_eq!(... , false)` to equivalent `assert!(!...)` style. This does not
change the tested predicate; the measured run proved the root was absent after
cleanup. No repeat selected run was made for that syntax-only edit. The parent
will lint the final source tree before freezing it.

## Final Goal 02 roster and evidence status

The unique accepted behavioral roster is 16 cases: M1-A's ten cases, with its
old integration watcher case superseded by M2-B's three moved delivery cases
and unwind case; M1-B's three broker cases; and M2-A's four CPU oracles, which
overlap the original M1-A roster. The accepted M2-A run remains the only
selected timing pair supporting a speed claim: 4/4, 18.305994 to 16.683376
seconds, a one-pair 1.622618-second / 8.863862% aggregate reduction. Its
preparation telemetry was mixed, and no isolated startup/drop attribution was
measured. M1-B and M2-B observations above make no speedup claim. Prior
checkpoint runs and the current four-case run are not repeated to produce a
new union timing.

The configured M2-B Reviewer follow-up failed once at the agent-thread limit.
The authorized fallback is a structured parent review, not an independent
review. The parent's source/evidence verification passed for all three measured
phases and 28 unique owned process identities, with zero live or unknown
identities and cleanup/output draining verified; its receipt is
`g02-m2b/parent-review-verification.json`. That review also identified the
README recipe-count placement ambiguity, which was corrected before this
record was finalized. The parent's final source/evidence/documentation review
found no remaining correctness findings. Final staged-tree lint is still
pending:

```text
cargo clippy -p multi_launcher --profile fast-dev --lib --tests --no-deps
```

`--tests` statically checks the `cfg(test)` unit-fixture code; it does not run
tests. The parent will record the final lint result under external
`g02-m2b/final-clippy/` and close Goal 02 with its next-goal handoff in external `QUEUE_STATE.md`.
The parent retains Git staging, commit and push ownership. No final lint result
or commit SHA is claimed here before that freeze.

The first final-lint attempt ran against staged tree
`9ee487f7d9b11ad0349d8b0ff69d571f65cc250a` and returned exit code 0 in
143.282268 seconds, but was not accepted as the final lint because it reported
three Goal 02 warnings: an unnecessary `mut` and redundant local binding in
`src/gui/mod.rs`, plus `io_other_error` in `src/performance/workloads.rs`.
Those three equivalent forms were corrected without changing behavior. No
tests or Clippy rerun was performed by this source writer; the final Clippy
rerun remains the parent's gate. The retained first-attempt stderr is under
`g02-m2b/final-clippy/g02-final-clippy_20261025T200026800Z_5c7cbdf315dc479e8b57ca96c98edff1/stderr.bin`.
