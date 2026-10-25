# Test-suite optimization execution

Contract: `CODEX_GOAL_TEST_SUITE_OPTIMIZATION.md` and the supplied Kickstart.
No application behavior or default profile changes are authorized.

## Provenance

- Original branch: `master`; initial dirty inventory: empty.
- Default integration branch: `origin/master`, successfully fetched.
- Original base: `c5e10bdc2a86e54363950a441dc8d105bc869267`.
- Fresh task branch: `codex/test-suite-optimization-20261025`.
- Own M0-A baseline: `916bd05d9daf461129d5fa64307ab1f2b90e1bed`.
- Initial upstream push: succeeded; tracks the fresh origin branch.

## Bounded execution plan

| Checkpoint | State | Acceptance / verification |
|---|---|---|
| M0-A | complete | Fresh branch, own baseline commit, normal initial push; clean checkout |
| M0-B | complete | Bounded owned profiling and structured receipts; synthetic success/failure/timeout and real exact smoke passed |
| M0-C | complete | Current-source inventory and full run, standalone oracle, three warm no-op/edit samples; hashes restored |
| M1-A/B | complete | Native progress helper and exact selections passed; matched no-op profiles effectively tied |
| M2-A | complete | Independent last-layout cache; original recipes and all row/state assertions retained; exact and mixed proofs passed |
| M2-B | complete | One retained measured dialog fixture improvement; Quick Notes already caches geometry and was left unchanged |
| M3-A | complete | One two-file grouping retained; 24.67% controlled-edit build saving, exact roster mapping and six selected tests passed |
| M3-B | skipped | No small measured extraction/deduplication justified; avoid broad GUI test-owner restructuring |
| M4-A | complete | Measured replacement-observer teardown race fixed with existing completion seam; 20 exact repeats and mixed group passed |
| M4-B | skipped | No measured arbitrary sleep justified a substitution; no wait campaign |
| M5-A | pending | Matched scorecard, coverage parity; at most one final full execution |
| M5-B | pending | Independent review; scoped format and one first-party Clippy; commit and push |

One source writer per checkpoint; planning and final review are read-only.
Each coherent retained checkpoint is committed and receives one normal push attempt.
Raw logs and receipts stay outside the repository under the Windows temporary
`Multi_Launcher/test-perf` directory. Tracked results omit private host paths.

## Environment observations

- Windows native toolchain: Cargo 1.99.1, rustc 1.99.1, Nextest 0.9.135.
- Initial sandboxed contained library listing stopped with CIM access denied;
  root was absent after cleanup. This is a containment/environment failure,
  not a test failure or a baseline duration. Retried once with process inventory
  permission before any source change.
- Build, execution, no-op and edited-build measurements are distinct. Historic
  263.841 s is context only and cannot serve as this branch's baseline.

## M0-C baseline and M1-B profile comparison

All application/test source remained identical to M0-A during these samples.
Only tooling and this ledger were uncommitted. Host: Windows x64, Intel i7-7700K,
8 logical processors; default features, Nextest default profile, no job/thread
overrides, no repository Nextest config. No concurrent Cargo compilations.

| Measurement | Seconds | Samples / status |
|---|---:|---|
| fast-dev library preparation | 350 (Cargo report) | Descriptive cache refresh; excluded from warm comparison |
| fast-dev workspace preparation | 310 (Cargo report) | Descriptive; excluded from execution |
| fast-dev library warm no-op | 2.377 median | 2.447 / 2.377 / 2.279; all success |
| fast-dev two-target warm no-op | 2.347 median | 2.347 / 2.265 / 2.454; all success |
| fast-dev two-target controlled edit | 8.445 median | 10.333 / 8.445 / 7.976; all success |
| dev two-target preparation | 371.36 | Separate cache preparation; excluded from paired result |
| dev two-target warm no-op | 2.360274 median | 2.419993 / 2.360274 / 1.994045; all success |
| C2G standalone execution | 42.664474 | One passed; Nextest runner summary 42.717 |
| Full execution | 249.316 | One run; 6966 pass / 5 fail / 0 timeout / 48 skipped |
| C2G in full execution | 74.883421 | Passed, reported slow; timeout/parallelism unchanged |

The selected integration pair is `query_autocomplete` and `mkmacro_compiler`.
Each controlled edit appends the same sample-specific comment to both files;
original bytes are restored after each sample and verified by SHA-256 in a
finally block. Restored-source preparation is excluded from measured edits.
The profiles' no-op medians are effectively tied; retain existing opt-in
`fast-dev` guidance and do not change defaults or claim edited-build profile gains.

Inventory: 7019 cases / 71 binaries / 48 ignored. Full baseline failures:

- `file_operations::tests::direct_scan_keeps_child_directory_blocker_when_listing_is_denied`
- `gui::radial_editor::action_editor::tests::live_ranked_editor_search_scrolls_and_assigns_rows_beyond_fifty`
- `gui::table_utility_dialog::tests::csv_workflow_opens_sorts_filters_selects_and_copies_the_current_profile`
- `gui::universal_action_catalog::tests::acceptance_fixture_queries_reach_real_ranked_actions_and_authoring_rows`
- `mouse_gestures::service::tests::deferred_click_release_cannot_remove_a_replacement_observer`

The last failure is a test-side teardown race: replacement release may be deferred
while the old worker still holds the mutex; the test asserts uninstall before
waiting for replacement cleanup. M4-A will use its existing completion seam.
The other failures remain outside this task; isolated passes will not erase them.

Raw evidence names beneath the external test-perf root: `bootstrap-lib-*`,
`baseline-inventory`, `baseline-oracle`, `baseline-full`, `baseline-context.json`,
`baseline-top20.json`, `build-samples`, `profile-choice`, and their containment
records. Ranked top costs start with C2G (74.883s), tracked-panel coverage
(23.668s), Quick Notes recipe (16.892s), root-list recipe (14.527s), Quick Notes
geometry (13.839s), and cold-grid recipe (12.997s). The top 20 remain in JSON.

The baseline generated `clipboard_modifiers.json` in the initially clean root;
it was moved intact to external evidence, not staged. No pre-existing work existed.

## M0-B / M1-A verification

- `dev/test_perf/test_profile.ps1`: synthetic argument quoting, JSON-plus suite
  metadata, success, exit 7, owned parent/child timeout cleanup and startup-failure
  receipt passed. All scripts parsed successfully.
- `profile.ps1 -Mode SelectedBuild -TargetKind Library -CargoProfile fast-dev`:
  success; unknown case counts stay null because no case inventory is collected.
- `profile.ps1 -Mode SelectedRun -TargetKind Library -CargoProfile fast-dev`
  with exact history pin-roundtrip and typed-simple-dialog selectors: 2 passed;
  typed dialog baseline 4.552922s, history 0.031055s.
- `profile.ps1 -Mode SelectedRun -TargetKind Integration -Target query_autocomplete
  -CargoProfile fast-dev`: 1 passed, execution separated from build/listing.
- `run.ps1` on that integration target: native progress and exit 0; exact absent
  selector: no execution and outer `pwsh` exit 4.
- `summarize.ps1 -ReceiptPath <library-receipt>`: JSON summary validated.
- Initial real smoke exposed Nextest's prohibition on combining binary metadata
  with build selectors. One bounded repaired smoke group passed after selectors
  were restricted to prebuild; inventory/execution now use only its metadata.
  The unsuccessful receipts remain external. No real failing test was hidden.
- Profiling phase wall includes CIM/event/cleanup overhead; pure test durations
  are from Nextest. Interactive runs avoid this optional profiling overhead.
- M0-B commit `7325caee`: pushed successfully.

## M2-A independent oracle

Only `src/gui/render.rs` test helpers changed. One state-local immutable eager
snapshot is keyed by complete actions, aliases, style, path-display, width and
scale. Each miss uses a fresh default-font/default-DPI egui context, independent
of production geometry. Every frame retains action equality, every row geometry,
visible/overscan/popup/provider/selection/scroll and state/receipt assertions.
An explicit mode preserves fresh validation for the ignored benchmark, including
its sequence timing boundaries; ordinary oracle runs reuse equivalent inputs.

- Standalone: 42.6644737s before / 17.8393458s after (one sample each), **58.19%**
  lower per-test wall time. Same host/default features/fast-dev/default Nextest
  parallelism, with compilation excluded and owned execution bounded at 180s.
- Mixed group: 4 passed, no failures/timeouts; oracle 17.979s. Other exact cases:
  `root_list_geometry_matches_eager_selectable_label_layout`,
  `track_c2g_eager_layout_cache_invalidates_changed_inputs`, and
  `track_c2g_validator_rejects_offscreen_geometry_popup_and_painted_row_corruption`.
- Invalidation proof covers equal-content reuse and width/scale/content/style/
  alias/path-display changes. Negative fixture catches deliberately corrupt
  offscreen geometry, omitted popup owner and painted rectangle. Its caught
  panic diagnostics are expected evidence, not test failures.
- Original four 100-action recipes, 1k/10k viewport cases, nine viewport steps,
  ticks, action cardinalities and independent full-row comparisons are unchanged.
  Two additional proof tests account for the library roster increase 6337→6339.
- Existing progress markers in the mixed run report the 10k case: preparation
  2.530s, rendered frames 4.080s, validation 6.072s. Timings nested inside those
  phases are not additive. The baseline JSON reporter omitted successful stdout,
  so current-baseline subphase attribution, separate hash/serialization and
  teardown costs are unavailable; no historical subphase is substituted.
- Scoped rustfmt and diff checks passed. First prebuild caught a tuple-field
  mistake in the new negative fixture; the corrected prebuild passed. Both
  receipts are retained. No failed build duration is counted as an improvement.
- Evidence: `m2a-build*.log`, corresponding SelectedBuild receipts,
  `m2a-oracle.log` / SelectedRun receipt, `m2a-mixed-proofs`, and
  `m2a-phase-summary.json` under the external evidence root.

## M2-B measured fixture

`src/gui/actions.rs` changes only `typed_simple_dialogs_preserve_interactive_lifecycle`:
each of its 17 independent apps now has a temporary root, inert plugin manager,
isolated actions/settings paths and disabled launcher/help/Paste Sequence
shortcuts before construction. These commands dispatch directly to dialog hosts;
plugin search and a global macro runtime are unrelated fixture work. The shared
`new_app` helper, all 17 command/panel pairs, Dashboard activation and every
query/restore/visibility/history assertion remain unchanged.

Matched two-case selection (typed-dialog case plus history pin-roundtrip),
fast-dev/default features/default parallelism, one sample per side:
typed dialog **4.5529221 → 2.4971859s**, 45.15% lower. Both cases passed; history
was 0.031055 / 0.0325031s. Compilation was separate (candidate preparation 77s,
not a scored build improvement). Scoped rustfmt/diff checks passed; owned
cleanup completed. Evidence: `m2b-build.log`, `m2b-selected.log` and their receipts.

Quick Notes geometry was inspected as a second candidate: it already computes
eager geometry once per independent cold/warm fixture. Its 12 fixtures and seven
settling frames have meaningful lifecycle coverage; no speculative reduction was
retained. No other additional slow-test optimizations were manufactured.

## M4-A deterministic teardown

Only `src/mouse_gestures/service.rs`'s existing replacement-observer test changed.
It now bounds the old completion receive and explicitly releases/awaits the
replacement token using the existing completion method before asserting exactly
one uninstall. Both local handshakes use two-second bounds; no sleeps, retries,
production changes, global timeouts or parallelism changes were introduced.
Stale-token rejection, actual replacement click/position/generation and duplicate
event assertions remain. Completion proves token cleanup/uninstall, not a thread join.

The original failure was observed at M0-A source, line 1944 (0 vs 1 uninstall).
After the fix, `cargo nextest run --lib --cargo-profile fast-dev --offline -E
'test(=mouse_gestures::service::tests::deferred_click_release_cannot_remove_a_replacement_observer)'
--stress-count 20 --no-fail-fast` passed all 20 iterations (0.823s runner total).
One normal-parallelism group with the adjacent passive-lease and guard-drop
cases passed all three (0.047s total). Both owned invocations completed with
recorded cleanup; evidence: `m4a-build.log`, `m4a-repeat`, `m4a-mixed`.
The failed baseline duration is not counted as a speed improvement.

There is no measured unnecessary wall-clock sleep in these selected owners.
Quick Notes' settling frames test lifecycle behavior; no M4-B edits were made.

## M3-A one target-grouping experiment

The unchanged bodies of `tests/query_autocomplete.rs` (one test) and
`tests/mkmacro_compiler.rs` (four tests) moved under `tests/pure_cases/`.
`tests/suites/pure.rs` includes them as private modules; `Cargo.toml` declares
that single target. Autodiscovery and all existing profiles/features remain.
Both files are pure in-memory tests: no app, native services, environment,
temporary fixtures, binary-local global state or subprocess-name assumptions.

| fast-dev selected build | Standalone pair | Grouped `pure` | Change |
|---|---:|---:|---:|
| Warm no-op median, 3 samples | 2.346657s | 2.100373s | 10.50% lower |
| Controlled edit median, 3 samples | 8.444534s | 6.361472s | **24.67% lower** |
| Controlled edit range | 7.975874–10.333270s | 6.289574–6.383503s | No overlap |

Grouped preparation (7.556874s) and restored-source preparation (7.197831s)
are excluded. Exact edit bytes/comments match the baseline fixture on both
files; final original SHA-256s match. This is a two-file edit/link workload,
not a claim about arbitrary single-file edits or the entire suite's build.
One experiment only; no alternative group was tried.
Combined executable size fell from 10,168,320 to 9,052,672 bytes (PDBs excluded).

Full M3 inventories: **7021 cases, 48 ignored**, targets **71→70**; canonical
name/ignored mapping has zero differences or duplicates. The earlier M0-A full
roster had 7019 cases: its only additions are the two passing C2G proof tests.
Mapping: `query_autocomplete/default_query_autocomplete_enabled` becomes
`pure/query_autocomplete::default_query_autocomplete_enabled`; all four
`mkmacro_compiler/<name>` cases become `pure/mkmacro_compiler::<name>`.
The complete five-name mapping and checksum are in `m3-roster-parity.json`.

`cargo nextest run --test pure --test plugin_queries --cargo-profile fast-dev
--offline -E 'binary(=pure)|test(=help_plugin::search_returns_help_action)'
--no-fail-fast`: **6 passed**, 135 unselected/ignored cases skipped. Existing
owned runner records show completed cleanup. Retain the grouping on these
measurements and parity proofs. Evidence: `m3-before-inventory`,
`m3-after-inventory`, `m3-mapped-tests`, `m3-grouped-build-containment`,
`build-samples`, `standalone-target-sizes.json`, and `m3-roster-parity.json`.

No optional M3-B change was made: the monolithic library-test preparation is
expensive (327s for M2-A), but a small proven duplication/monomorphization fix
was not identified. Large GUI test-module restructuring and compiler tuning
would exceed this bounded follow-on.

## M5-B review remediation

The independent reviewer found two tooling defects: the native helper returned
zero after a failed target prebuild, and the JSON parser attributed interleaved
case completions to the most recent suite announcement. Both are repaired.
Terminal case identities now resolve against the actual inventory; ambiguous
or unknown protocol identities remain explicitly unresolved. Pure fixtures
cover interleaving, ambiguity, unknown identity and started announcements.
Ignored counts come from inventory. Generic failed events cannot reliably
distinguish per-case timeouts on this reporter version, so that total stays
nullable; explicit timeout-event counts and raw output remain available.

All PowerShell scripts parse; `test_profile.ps1 -ParserOnly` passes. Bounded
native helper checks returned **101** for a deliberately nonexistent target,
**0** for all five `pure` cases, and **4** for an unmatched exact filter without
starting execution. The containing check exited 0; all recorded owned
identities are absent afterward (`m5-helper-check`). The same reviewer
confirmed both repairs and found no new substantive issue in the narrow
follow-up. No production code changed.

Final matched library no-op samples at `15d9e12c` are 2.140753, 2.159011 and
2.101898 seconds (median **2.140753s**, 9.94% below baseline 2.376990s).
Preparation 2.203940s is excluded. These small warm command differences include
startup/freshness work and do not measure compiler or linker time separately.
Evidence: `final-lib-build-containment`, `build-samples/candidate-lib-*`.
