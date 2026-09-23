# Radial automated acceptance execution ledger

Approved specification: [automated acceptance plan](../multi_launcher_radial_automated_acceptance_codex_plan.md).

## Start state

- `AUTOMATED_ACCEPTANCE_START_HEAD`: `55120289c9ef9ec5c026d6348a0050388ff3e1fa`
- Initial branch: `radial-menu-2` (tracking `origin/radial-menu-2`).
- Initial working tree: clean; no staged, unstaged, or untracked files.
- Input-recovery ledger: Packages A/B/C implemented; final historical Nextest recorded 4,635 passed and eight skipped; native focused ROOT and Designer interaction still pending. Later diagnostic traces and candidate corrections are historical evidence, not a current native acceptance pass.
- Immutable radial feature baseline: `0d0acaf471a52f49a7ebdff61879416eefa9fc9b` as recorded in the stabilization ledger.

## Milestones

| Milestone | State | Evidence |
| --- | --- | --- |
| A. Deterministic input and headless Designer acceptance; native runner foundation | complete | Exact fake-time chord regression and retained `viewport_ui` driver passed the focused two-test gate. The opt-in runner currently reports preflight only; native acceptance is pending B. |
| B. Isolated native runner and failure evidence | complete | Source-matched Win32 run produced case reports, checked per-edge input evidence, trace/window artifacts and screenshots; the failed cases now identify hook admission and Designer semantic discovery rather than relying on manual interaction. |
| C. Focused ROOT and Designer root-cause remediation | pending | |
| D. Basic authoring and copied-profile acceptance | pending | |
| Final verification and independent review | pending | |

## Milestone A verification

- `cargo check --lib --bin multi_launcher --bin radial_acceptance`: passed.
- Focused selector preflight: selected exactly the two new tests.
- `cargo nextest run -p multi_launcher --no-fail-fast -E 'test(/^(hotkey::launcher_invocation::tests::exact_shift_alt_win_end_chord_uses_fake_time_for_tap_and_hold|gui::radial_editor::tests::retained_headless_viewport_accepts_semantic_click_after_snapshot_and_tabs)$/)'`: 2 passed, 4,647 skipped, exit 0. Durable record: `target/radial-acceptance-a-focused-final7.{meta,log,exit}`.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- Earlier focused attempts exposed test assumptions about AccessKit disabled/focus metadata; the final test instead verifies pending click inactivity, authoritative reply acceptance, semantic New Menu mutation, pointer-acquired TextInput focus, and Tab focus movement through production `viewport_ui`.
- Native Windows input and HWND acceptance: not run in this milestone.

## Milestone B verification

- `cargo fmt --all`: passed.
- `cargo check --lib --bin multi_launcher --bin radial_acceptance`: passed (eight driver dead-code warnings remain for later cases).
- `cargo build --bin multi_launcher --bin radial_acceptance`: passed.
- Source-matched native runner: produced `C:\Users\Jay\AppData\Local\Temp\multi-launcher-radial-acceptance-b-final2-a1b12086d47d4569a9878daebfa72e18\report.json` against revision `6229ba7a`. H0 failed at hook admission despite checked F11 down/up on the child ROOT HWND and input desktop. D0 and D5 passed. D1/D2 could not discover the semantic Tree control and D4 could not discover the Skins command. The run exited nonzero, as expected for the unresolved production cases. Child/profile/cursor/input-desktop cleanup passed; previous foreground restoration was attempted but Windows reported no foreground HWND. These failures are remediation input for C/D, not an acceptance pass.
