# Radial input recovery execution ledger

Approved specification: [radial input recovery plan](../multi_launcher_radial_input_recovery_codex_plan.md). This is one bounded implementation milestone followed by integrated verification and native acceptance.

## Start state

- `INPUT_RECOVERY_START_HEAD`: `fdb27f91beecd7e57f0d9aaebb6299282b4bbb3d` (`radial-menu-2`)
- Initial working tree: clean; no staged, unstaged, or untracked files in `git status --short --branch`.
- Immutable feature baseline remains `0d0acaf471a52f49a7ebdff61879416eefa9fc9b` (recorded in `radial-stabilization.md`).

## Milestone

| Work package | State | Evidence |
| --- | --- | --- |
| A. Designer input, focused root toggle, readiness, close | implemented; native acceptance pending | User-triggered ROOT hide sends a native visibility edge while Screen Draw and startup parking remain intact. Failed bootstrap stays read-only with Retry and Close available. The reported Designer client-input failure still needs an actual Windows trace and interaction. |
| B. Basic ring authoring, action search, Inspector handoff | implemented; native acceptance pending | The ordinary Design toolbar exposes New Menu, Add Ring, Ring and Slots; action search runs before result limiting, Inspector handoff preserves popup edits, and errors are visible. |
| C. Previewable validated geometry proposals | implemented; native acceptance pending | Candidate geometry, stale-generation protection, explicit Apply, shrink resolution and undo are implemented. Candidate preparation follows the normal service path. Apply waits for its exact prepared frame; proposal-instance tokens prevent cache reuse. |
| Integrated verification and native acceptance | tests passed; native acceptance pending | Final `cargo nextest run --no-fail-fast`: 4,635 passed, eight skipped, exit 0. Final `cargo build --bin multi_launcher`: exit 0. The actual failing Windows profile and live GUI gestures remain unobserved. |

## Final verified source and candidate

- Final test source diff SHA-256: `00461A0B51C2B1A2E709906FEBCFE02ED798797A4668E5DC3F7DCBB12997D063` relative to `INPUT_RECOVERY_START_HEAD` (the untracked ledger is excluded).
- Final Nextest: `target/input-recovery/final-nextest-verified.log` and `.meta`; 4,635 passed, eight skipped, exit 0 at 2026-09-22 21:55:38 UTC.
- Final application build: `target/input-recovery/final-build.log` and `.meta`; exit 0 at 2026-09-22 21:59:13 UTC.
- Source-matched executable: `target/debug/multi_launcher.exe`, SHA-256 `FFAB543E1F9018B9A7112B3CAAAD6132A5213BF578B841F8984251B904DCC3E7`.
- The final source passed `cargo fmt --all --check`, `cargo check --lib --bin multi_launcher`, and `git diff --check`. The test fixture-only adjustment after the last check was compiled and exercised by final Nextest.

## Native acceptance remaining

There was no running Multi Launcher instance to inspect, no path to the user's failing test profile or source-matched launch context, and no native desktop input control available in this task. Consequently the first broken Designer client-input boundary has not been observed, and focused short-tap, Menus/Skins mouse and Tab, close, and Save/reopen cannot be claimed as natively accepted. Use the built candidate with the actual profile and `MULTI_LAUNCHER_RADIAL_ACCEPTANCE_TRACE=1`; resolve the log destination from its loaded settings. Capture a short session before the 256-event trace budget is exhausted. Compare `NativePointer`, `DesignerPointer`, `DesignerBody` and the root desired/command/actual-window events at the first absent transition, then run the plan's complete short Windows walkthrough. Preserve the original profile and record executable hash, working directory, loaded settings/data root, chord and threshold before interpreting the trace.
