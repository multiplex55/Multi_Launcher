# Productivity and iteration execution

Contract: `CODEX_GOAL_PRODUCTIVITY_AND_ITERATION.md` and its companion Kickstart.
Started 2026-10-31 in the single existing `G:\Repos\rust\Multi_Launcher` checkout.

## Category chain

| Category | Branch | Parent branch / tip | Baseline | Status |
| --- | --- | --- | --- | --- |
| C01 | `perf/productivity-iteration-20261031` | `master` / `d6541edd5f89c7dc04de8c68b65e2b9247978163` | `a8a6aa8e62012b83f1e99098c442654cb8ef6778` | M0-A complete; M0-B in_progress |
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

M0-B and M1-A–M1-C pending. C02–C07 implementation and final review pending. No feature, speedup, test, or native acceptance claimed by this baseline.

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
