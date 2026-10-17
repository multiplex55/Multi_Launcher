# Refinement execution ledger

Approved contract: `multi_launcher_refinement_codex_master_plan.md` and attached kickoff.
Execution branch: `big-change-cleanup-2`; initial HEAD `d0853d79`; initial worktree clean.
One writer; sequential checkpoint commits; best-effort normal push per commit.
Evidence/fixtures: ignored `target/refinement-evidence/` (owned fixtures only).

| Milestone | Status | Checkpoints / evidence |
| --- | --- | --- |
| M0 | complete | M0-A: source/manifest/status inspection; Nextest 0.9.135; rustc 1.99.0, x86_64-pc-windows-msvc; commit `513e89a4ca0292017b348735ce33cb896f25cd12` |
| M1 | in_progress | M1-A implemented in `src/gui/render.rs`: removed launcher scope strip, Ctrl+1–5 routing, scoped empty-state message and new-session scope reset; query/results layout now follows visible query geometry. `cargo nextest run --lib launcher_query_frame_has_no_scope_strip_or_ctrl_number_scope_changes`: 1 passed, 5,687 skipped. `rustfmt --edition 2024 src/gui/render.rs` and `git diff --check` passed. M1-A commit `178fb3b6`. M1-B source and scoped verification complete: unified Action-only provider/search flow, retired scope/category state and module removed; 11 launcher/provider tests and 1 radial ordering test passed (details below). M1-C help/compatibility cleanup remains pending. |
| M2 | pending | Shared rectangle-picker input shield |
| M3 | pending | Ordered-step layout |
| M4 | pending | Playback settings / atomic hotkey routes |
| M5 | pending | Cross-app playback / physical paste / tooltip |
| M6 | pending | Search and Replace layout |
| M7 | pending | Cargo binary target selection |
| M8 | pending | Integration, one full Nextest, ledger closure, Clippy last |

## Owner / direct test map

- Search: `src/gui/{render,search,mod}.rs`, Action-only plugin result boundary, Omni Search and radial actions; inline search/render tests. Retired `src/search_scope.rs` removed by M1-B.
- Capture: `src/gui/pinboard*.rs`, `src/gui/mkmacro_dialog/{visual_capture_workflow,visual_overlay,visual_overlay_windows}.rs`; inline overlay tests.
- Paste: `src/paste_sequence.rs`, `src/paste_sequence/playback{,/windows}.rs`, `src/gui/paste_sequence_dialog.rs`; inline persistence/playback/dialog tests. Existing schema v1 uses snake_case Automatic / ManualAdvance.
- Settings/routes: `src/settings/model.rs`, `src/settings_editor/`, `src/hotkey/{validation,reconfiguration,launcher_invocation}.rs`, `src/main.rs`, shared `src/mkmacro/input/`; staged settings and prepared reconfiguration own rollback.
- Replace: `src/gui/search_replace_dialog.rs`, domain/executor owners unchanged; inline dialog and disposable file tests.
- Cargo: root manifest currently autodiscovers binaries, no default feature list; explicit benches and grouped tests. Patched vendor dependencies are not assumed workspace members.

## Verification policy

No baseline build, full Nextest or Clippy. M0 used read-only inspection only.
M0–M7: directly affected focused tests and minimum compile checks.
M8-B: one full `cargo nextest run --workspace`, at most one repaired rerun.
M8-D only: maintained-target Clippy with `-D warnings`; final successful lint is the last automated verification.

## Commit index

- M0-A: `513e89a4ca0292017b348735ce33cb896f25cd12`.
- M1-A: `178fb3b6` (local).
- M1-B: source and verification complete; parent checkpoint commit pending.

## Limitations / unresolved issues

Native GUI evidence has not yet been attempted. No implementation blockers identified.
Normal push after M0-A was rejected by automatic approval review because the remote was untrusted; successful checkpoint commits remain local. No workaround or repeated escalation was attempted.

## M1-B scoped evidence

Removed launcher `SearchScope`, effective-scope/filter/cache identity, result categories and category-only plugin APIs. Plugins return `Action`; scoring uses `(Action, f32)`. Omni keeps source order, first-source deduplication and independent include-provider settings. Provider pending/revision/catalog-version checks, literal query policy, enabled prefix matching and `fs` Global/Directory ownership remain intact. Main/settings reload callers use the existing Action-only reload API. Scope-only tests were removed; eager search/watch oracles and radial fixtures were migrated. M1-A's frame test now proves query/results/Ctrl+1-5/layout behavior without retired state.

- Launcher/provider command: `cargo nextest run --lib -E 'test(unified_launcher_and_read_only_snapshot_keep_order) | test(launcher_query_frame_has_no_scope_strip_or_ctrl_number_scope_changes) | test(read_only_search_uses_launcher_boundary_without_mutating_root_state) | test(deferred_snapshot_matches_ordinary_g_prefix_provider_selection) | test(track_c2a_oracle_covers_provider_precedence_and_snapshot_outcomes) | test(track_c3a_search_contract_matches_independent_fst_and_linear_references) | test(omni_results_keep_catalog_order_and_first_source_dedup) | test(provider_actions_match_on_synchronous_and_snapshot_paths) | test(query_prefix_respects_enabled_plugins_and_search_capability) | test(literal_qr_payload_survives_exact_sync_and_snapshot_search) | test(cs_prefix_uses_isolated_snippets_provider)'`: **11 passed, 5,672 skipped**; run `fb45a624-3249-4968-a564-edd3c8852e25`.
- Adjacent radial command: `cargo nextest run --lib deferred_execute_first_uses_the_current_shared_search_order`: **1 passed, 5,682 skipped**; run `2bca522b-5aec-4dec-9985-638fc1fbe0da`.
- Initial focused compile failed on duplicate ticketed helper and stale snippets/watch fixture API calls (E0592, E0599, E0061); corrected before the successful rerun above. No failing test assertions. Final compilation emitted existing vendor warnings only (`epaint`, `egui`, `image`); no vendor source changed.
- Touched Rust files formatted with `rustfmt --edition 2024 --config skip_children=true`; touched/cumulative diff inspected and `git diff --check` passed. Source/tests/benches search found no retired launcher scope/category/API references; unrelated radial-editor local `search_scope` and file-search domain scopes remain. No full suite, Clippy, extra check/build or native desktop test was run for this checkpoint.
