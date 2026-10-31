# Productivity and iteration execution

Contract: `CODEX_GOAL_PRODUCTIVITY_AND_ITERATION.md` and its companion Kickstart.
Started 2026-10-31 in the single existing `G:\Repos\rust\Multi_Launcher` checkout.

## Category chain

| Category | Branch | Parent branch / tip | Baseline | Status |
| --- | --- | --- | --- | --- |
| C01 | `perf/productivity-iteration-20261031` | `master` / `d6541edd5f89c7dc04de8c68b65e2b9247978163` | `a8a6aa8e62012b83f1e99098c442654cb8ef6778` | M0-A/M0-B/M1-A complete; M1-B/M1-C assessed_no_accepted_optimization |
| C02–C07 | Not started | Must use immediately preceding actual final tip | pending | pending |

## C01/M0-A

- Clean tracked/untracked state before bootstrap; only local branch was `master`, tracking `origin/master`, both at the parent above. Origin is `https://github.com/multiplex55/Multi_Launcher.git`; cached default is `master`.
- This is a new chain. Previous queue/program material is archived; no current execution ledger or active predecessor exists. Parent includes the planning reference `261651ed` plus the committed task-document/archive relocation update `d6541edd`.
- Safe first-task current-HEAD exception: `git ls-remote --symref origin HEAD` failed (GitHub port 443 inaccessible); ordinary sandboxed fetch also could not write `.git/FETCH_HEAD`. Remote freshness is unverified. Preserve the newer clean local integration tip rather than rewind or infer remote ancestry.
- Created child directly from the exact parent with `git switch -c`; initial child HEAD verified before baseline. No source edits/builds/tests preceded bootstrap.
- Only this task-owned ledger is staged for baseline. Existing Goal/Kickstart remain unchanged. No worktrees, auxiliary checkouts, clones, merges, rebases, or cherry-picks.
- `git rev-parse a8a6aa8e^` equals the parent above. Initial `git push -u origin HEAD` succeeded; upstream now tracks this category. Follow-up approved `git ls-remote --symref origin HEAD` succeeded and confirmed actual remote default `master` still equals the selected parent. Initial sandbox failures were access limitations, not evidence of a remote problem.

## C01/M0-B capability observations

- `rustc -Vv`: 1.99.1 (`58cf8f9c3`, 2026-10-13), x86_64-pc-windows-msvc, LLVM 23.1.1. `cargo nextest --version`: 0.9.135. Existing `target/fast-dev` cache retained; no clean performed.
- `fast-dev` inherits dev checks with line-tables-only debug info; `iteration` inherits release with opt-level 2, 64 codegen units, incremental and no LTO. No root `.config/nextest.toml` exists.
- Supported process ownership: `dev/track_b/measure.ps1` wraps the checked owned-process runner with raw argument arrays, timeout, source fingerprints, stdout/stderr and cleanup receipts. Use 1800-second first preparation; 1200-second subsequent preparation; separate prepared Nextest execution with a finite runner/test bound. `dev/test_perf/profile.ps1` has a 900-second parameter cap, so do not use it to impose an accidental short first-preparation budget.
- Computer-use skill and its full guidance/API/confirmation documentation read. `@oai/sky` import and read-only `list_apps()` succeeded (40 apps, installed launcher with no open window). No arbitrary user window was targeted. Launch a freshly built isolated executable later; this capability observation is not a native app pass.
- Planner is read-only; parent remains sole ledger/Git owner. No role configuration changes.

## Execution rules

One source writer and one Git owner; configured planner/implementer/reviewer roles, unchanged. Repository role files specify planner/reviewer gpt-6.1-sol high and implementer gpt-6-luna max; these are configuration observations, not backend-resolution claims.

Use Goal H bounds: first necessary preparation 30 minutes; subsequent preparation 20 minutes (one justified adjustment to 30); execution 100 seconds/test and 10 minutes/run; justified native case 180 seconds; UI scenario 10 minutes with one recovery; initial attempt plus two focused corrections. No competing Cargo jobs, full-suite default, cargo clean, or process-name termination. Final first-party Clippy only at closeout.

## Checkpoints

M0-B complete (`010cb970a01f2b81a705f61b9870b29b2e8ef1ed`, pushed). M1-A complete (`46a1afe827a421e7cc4a1fb16c99c5ef2d4c6ef6`, pushed); M1-B assessed_no_accepted_optimization (`8d964fddca802648ad6f8d97f6da9cb84edf5725`, pushed); M1-C assessed_no_accepted_optimization. C02–C07 implementation and cumulative final review pending. No feature or speedup claimed by C01.

### M0-B owner map and command selection (complete)

Read-only planner intake completed against baseline `a8a6aa8e`; no application changes or builds. Root Cargo package is `multi_launcher`, explicit bin `multi_launcher`. Library owns `diff::*`, `mkmacro::*`, `hotfolder::capability::*`, `gui::jump_mode::*`, `dashboard::widgets::quick_tools::tests::*`, `archive::*`. Integration `domain` owns Diff engine/file/session and theme cases; `pure` owns Macro compiler. Enumerate exact names/counts before execution.

Macro schema v14/model, executor/validation, `authoring_fields.rs`, `editor_mutation.rs`, `action_catalog`, GUI action editor and `package_ui.rs`, `package.rs`, and `store.rs` are the reuse/migration boundary. Geometry already exists for Diff (`DiffPersistenceV1`), Clipboard Modify settings/runtime, main launcher and Multi Manager; no general utility-size store found. Extend existing persistence infrastructure in C04. Startup recovery/settings owner is `startup.rs`; catalog/resolver tests can be binary-owned (`--bin multi_launcher`), deterministic scanner tests also exist in the library. Notes/Snippets loaders are their plugin modules; History is lazy-loaded in `history.rs`.

Initial measurement selections (runtime inventory still required): library `history::tests::pin_roundtrip_and_toggle`; `gui::tests::{activation_applies_query_override_first,should_show_dashboard_when_empty_query,isolated_profiles_load_persisted_settings_without_cross_contamination,isolated_fixture_drop_restores_process_state_during_unwind}`; `gui::render::tests::track_c2g_oracle_query_sequences_match_usable_cpu_frames`. First four GUI fixture cases emit construction/drop/cleanup timing and include deliberate unwind. Track C2G uses its owned workspace and preserves interaction/viewport assertions. Do not run generic render family: some older helpers still construct production app against relative paths.

| Purpose | Current command route |
| --- | --- |
| Incremental check | `cargo check --locked --profile fast-dev -p multi_launcher --lib` |
| Test preparation | `cargo nextest list --locked --cargo-profile fast-dev -p multi_launcher --lib --list-type binaries-only --message-format json` |
| Exact inventory | `cargo nextest list --binaries-metadata <prepared-json> -E <resolved-expression>` |
| Prepared execution | `cargo nextest run --binaries-metadata <prepared-json> -E <resolved-expression> --no-tests fail` with finite runner/test limits |
| Native/UI | Build current app `cargo build --locked --profile fast-dev -p multi_launcher --bin multi_launcher`, then isolated profile via explicit CWD/settings/data-root conventions and note/template overrides as demonstrated in `dev/jump_mode`; target owned process/window only |

Existing `IsolatedAppWorkspace`/`IsolatedWorkspace` own unique roots and app-before-environment teardown; isolated constructor disables both Paste Sequence shortcuts before construction. Do not invoke historical `developer_loop.ps1` unchanged (hardcoded old SHA/protocol). M1-A uses byte-saved/restored comment edits to `src/history.rs` and `src/gui/mod.rs`, one at a time, no optimization yet. Preparation/execution/timing and cleanup outcomes will be recorded separately. M0-B is intake only, not an app/test/native pass.

### M1-A measurements (complete)

Evidence root: `C:\Users\Jay\AppData\Local\Temp\Multi_Launcher\productivity-iteration-20261031`. Each scenario directory contains `measurement.json`, command/source fingerprint, stdout/stderr `.bin` text, and owned-process retirement/output-drain details. Source `010cb970`; unchanged application source inherited from `d6541edd`. Windows MSVC toolchain/profile above, existing warm cache, normal Nextest concurrency. One sample so far per condition; no speedup claimed.

| Scenario prefix | Result | Wall seconds / observations |
| --- | --- | --- |
| `c01-m1a-lib-preparation_20261031T113621818Z` | Cargo exit 0; overall runner failed | Cargo 4m38s; runner 339.582325s, owned descendants survived root (observed vctip). Cleanup verified, output drain verified, every recorded owned identity absent. Not relabeled a pass. Reuse successfully prepared binary metadata. Subsequent compile measurements use existing verified-MSVC-vctip policy, not a name-based kill or security change. |
| `c01-m1a-selected-inventory_20261031T114227646Z` | pass | 1.752764s; 6 exact selected tests / 1 library binary, nonempty asserted; 6492 excluded. |
| `c01-m1a-selected-execution_20261031T114240550Z` | pass | 16.536532s runner / 15.747s Nextest; 6 passed, 0 failed/timeouts. Custom local Nextest profile `productivity` sets supported slow-timeout period 100s/terminate-after 1; outer owned deadline 600s. |
| `c01-m1a-lib-noop_20261031T114308989Z` | pass | 1.702950s; warm unchanged test preparation, not edited incremental. |

Selected runtime breakdown: History pin test .106s; activation .474s; dashboard predicate .474s; unwind .477s; profile isolation .542s; GUI oracle 15.724s. Fixture construction activation366.7705ms/dashboard357.5732ms, drops44.1437/45.1189ms, root cleanup4.0042/3.0438ms; profile A/B construction354.2219/16.3717ms, combined drop74.7594ms. Root removal/CWD restoration asserted; deliberate unwind and subsequent fixture reuse passed. Oracle retains 100, 1,000 and 10,000-row cases and interaction assertions.

Further baseline samples (same evidence root, exact command in receipt): `c01-m1a-check-preparation_20261031T114320016Z` passed121.410246s (Cargo1m51s, preparation included missing check artifacts); `c01-m1a-check-noop_20261031T114542426Z` passed0.908375s; `c01-m1a-check-leaf-edit_20261031T114544294Z` passed17.240388s; `c01-m1a-gui-edit-check_20261031T114644451Z` passed14.305147s. One sample each. Edited selected-test preparation `c01-m1a-gui-edit-test-preparation_20261031T114701081Z` passed58.896280s (Cargo47.97s). Byte-restoration verified History SHA256 `2B14DC9C9F0E6F06ED61F0BBDC00E8A2A06C0391079E4C29E907280BE4FBDDEE` and GUI SHA256 `2411C1D13DC60AC3BCFE75162C927D8BB6BB50AB218A22EED7BF7B9C94DBD88B`. These check samples are not application link/build or executed tests.

Three unchanged six-test execution samples:16.536532/17.516866/16.242799s runner (median16.536532, range16.242799–17.516866); Nextest15.747/15.766/15.396s; oracle15.724/15.753/15.379s. All18 case executions passed, no failures/timeouts, same selectors/workload/profile/concurrency. Samples2/3 used the semantically identical comment-only rebuilt binary; original bytes restored before execution, no behavior change. Additional receipts `c01-m1a-selected-execution-2_20261031T114857545Z` and `-3_20261031T114915758Z`. Expensive oracle width-change validation (~2.72/2.63s for10k rows) and repeated full-fixture construction (~.35s) identified; no unsupported optimization claim. All owned processes retired with output drained; only intentional ledger diff remains. M1-A acceptance met.

### Bounded C01 remaining plan

M1-B disposition: **assessed_no_accepted_optimization**. Controlled root checks14.31–17.24s and edited selected-test preparation58.90s establish cost, not proof of an actionable narrow compile boundary. Existing fast-dev reduced symbols, grouped explicit targets, gated developer bins, table-core boundary, and reusable metadata already cover the obvious low-risk options. A further profile would change comparability; a new crate needs evidence of cohesive production ownership and dominant saved work beyond this bounded investigation. No speculative production extraction, dependency change, or linker switch retained. Zero candidate source changes; no new tests needed for this no-change assessment. Baseline passing checks/tests reused unchanged and labeled above. This delivers baseline/command guidance, **not faster compilation or preparation**; that optimization request remains partially fulfilled. Reliable selected command is target-first library preparation and prepared metadata execution in the M0-B table, with the exact nonempty M1-A selectors; name filtering alone is not compile reduction. M1-B checkpoint closes the assessment without asserting a numerical gain.

M1-C candidate1 (planner handoff): retain a dedicated independent default-font/DPI `egui::Context` inside the test-only reusable eager-oracle cache in `src/gui/render.rs`; reuse its text caches on key misses while recomputing all expected rows. Keep production `state.ctx` independent, full cache key (actions/aliases/path mode/width/style/scale), FreshPerFrame historical mode, all100/1k/10k scenarios, viewport ticks, corruption detection, and complete assertions. Extend existing eager-cache invalidation test to compare reused-context geometry against fresh-context results across changed inputs. No global cache, source ownership migration, worker, dependency, or production shortcut. Relevant tests: existing oracle, `track_c2g_eager_layout_cache_invalidates_changed_inputs`, `track_c2g_validator_rejects_offscreen_geometry_popup_and_painted_row_corruption`, plus four isolated GUI fixtures above. Measure families individually and together at normal concurrency; retain only a gain beyond variation (up to3 samples/condition). Preserve preconstruction shortcut disabling and app-first teardown. Do not extract the small dashboard predicate for marginal fixture savings without a meaningful domain boundary.

### M1-C outcome: assessed_no_accepted_optimization

Implementer completed candidate1 in test-only `src/gui/render.rs`; formatting and diff checks passed. Actual candidate preparation passed181.254730s (Cargo2m50s), receipt `c01-m1c-candidate1-preparation_20261031T115209607Z`. Correctness inventory selected exactly2 library tests; `c01-m1c-correctness-execution_20261031T115606767Z` passed2/2 in1.699890s (Nextest.898s): full reused/fresh geometry comparisons on width/scale/content/style/alias/path changes, and existing intentional-corruption detection. Expected caught-panic diagnostics belong to the passing corruption test, not an ignored failure.

Comparable same-six-test candidate execution receipts: `c01-m1c-candidate1-execution-1_20261031T115618089Z`, `-2_20261031T115635002Z`, `-3_20261031T115651696Z`. Runner16.239412/16.148991/16.110800s (median16.148991, range16.110800–16.239412); oracle15.383/15.271/15.227s. All18 executions passed, no failures/timeouts; unique-root lifecycle/unwind assertions remained. Cleanup/output drain verified. These are **unaccepted candidate measurements**, not delivered runtime gains.

Planner independently assessed measurements: runner median difference.387541s is below baseline1.274067s spread; oracle median difference.453s is below combined observed spreads.530s, with overlapping samples. Gain beyond variation is inconclusive. One candidate, no fourth sample or unjustified second candidate. Existing fixture setup/lifecycle is already isolated and constructor-dominated; no additional safe fixture improvement accepted. Extra standalone-family runs were unnecessary after rejection; candidate is not promoted on correctness alone.

Preserved candidate patch at evidence root `candidate1-unaccepted.diff` (candidate blob `73c608e70f60c51cdb52298d53905877fcb03d13`), then restored **only** this task's candidate file. Verified working and HEAD render blobs both `178f7b20c2f7a49d2bcbae2ee68ee7c6ee83391b`; no application/test source changes remain. Baseline six-test behavior evidence is reused for unchanged accepted source, not represented as a rerun. No application/native build required for this rejected test-only candidate. C01 closes as a bounded assessment with useful baselines, preserved isolation, and **no delivered compile/fixture/GUI speedup**. Overall optimization fulfillment remains partial. Category independent review and actual closing tip are recorded at successor bootstrap; current intended diff is ledger only.
