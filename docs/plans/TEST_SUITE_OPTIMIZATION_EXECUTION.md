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
| M2-A | pending | Independent C2G oracle optimization; retain all recipes and 10k rows; exact and negative verification |
| M2-B | pending | At most five measured additional candidates; retain only justified improvements |
| M3-A | pending | One reversible pure-target grouping A/B; map full roster; retain only measured benefit |
| M3-B | pending | Optional one test-only compile hotspot; skip without evidence |
| M4-A/B | pending | Only observed isolation/wait costs; at most three wait substitutions |
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
