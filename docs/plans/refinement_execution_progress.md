# Refinement execution ledger

Approved contract: `multi_launcher_refinement_codex_master_plan.md` and attached kickoff.
Execution branch: `big-change-cleanup-2`; initial HEAD `d0853d79`; initial worktree clean.
One writer; sequential checkpoint commits; best-effort normal push per commit.
Evidence/fixtures: ignored `target/refinement-evidence/` (owned fixtures only).

| Milestone | Status | Checkpoints / evidence |
| --- | --- | --- |
| M0 | complete | M0-A: source/manifest/status inspection; Nextest 0.9.135; rustc 1.99.0, x86_64-pc-windows-msvc; commit `513e89a4ca0292017b348735ce33cb896f25cd12` |
| M1 | in_progress | M1-A implemented in `src/gui/render.rs`: removed launcher scope strip, Ctrl+1–5 routing, scoped empty-state message and new-session scope reset; query/results layout now follows visible query geometry. `cargo nextest run --lib launcher_query_frame_has_no_scope_strip_or_ctrl_number_scope_changes`: 1 passed, 5,687 skipped. `rustfmt --edition 2024 src/gui/render.rs` and `git diff --check` passed. M1-B state/filter work remains pending. |
| M2 | pending | Shared rectangle-picker input shield |
| M3 | pending | Ordered-step layout |
| M4 | pending | Playback settings / atomic hotkey routes |
| M5 | pending | Cross-app playback / physical paste / tooltip |
| M6 | pending | Search and Replace layout |
| M7 | pending | Cargo binary target selection |
| M8 | pending | Integration, one full Nextest, ledger closure, Clippy last |

## Owner / direct test map

- Search: `src/gui/{render,search,mod}.rs`, `src/search_scope.rs`, plugin result boundary, Omni Search and radial actions; inline search/render tests.
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
- M1-A: source and verification complete; parent checkpoint commit pending.

## Limitations / unresolved issues

Native GUI evidence has not yet been attempted. No implementation blockers identified.
