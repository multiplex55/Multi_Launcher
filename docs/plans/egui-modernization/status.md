# egui modernization checkpoint ledger

Source contract: [`MULTI_LAUNCHER_EGUI_UPGRADE_AND_THEME_PLAN.md`](../MULTI_LAUNCHER_EGUI_UPGRADE_AND_THEME_PLAN.md). Statuses here distinguish recorded preflight evidence from acceptance gates that have actually passed.

| Checkpoint | Status | Current evidence / exit gap |
|---|---|---|
| M0-A — repository and toolchain preflight | `complete` | The Windows MSVC `cargo check --locked --all-targets --offline` passed. Original Nextest diagnostics completed with three classified stale fixture failures; a separate focused test-only correction passed 4/4. Current HEAD is the objective's working baseline; comparison with the separately named archive remains unresolved. No remote builder was configured or supplied. |
| M0-B — golden interaction and render inventory | `in_progress` | Source inventory and isolated fixture prepared; full root-list CPU owner passed, serial remaining owners active. Native capture failed twice; screenshots, display/GPU timing and native interactions remain unproven. |
| M0-C — dependency/API breakage worklist | `in_progress` | Planner source-backed worklist written; independent review pending. No dependency changes. |
| M1-A — align Rust toolchain and egui dependency graph | `pending` | Not started. |
| M1-B — app lifecycle migration | `pending` | Not started. |
| M1-C — mechanical egui API compatibility | `pending` | Not started. |
| M1-D — rich text, toasts, image and third-party compatibility | `pending` | Not started. |
| M2-A — geometry/cache migration | `pending` | Not started. |
| M2-B — Windows windowing, renderer and event integration | `pending` | Not started. |
| M2-C — renderer performance comparison and decision | `pending` | Not started. |
| M2-D — Track A final parity gate | `pending` | Not started. |
| M3-A — semantic theme model | `pending` | Not started. |
| M3-B — runtime theme application | `pending` | Not started. |
| M3-C — presets and transactional live theme preview | `pending` | Not started. |
| M3-D — typography, icons and lightweight rendering assets | `pending` | Not started. |
| M4-A — root launcher search field polish | `pending` | Not started. |
| M4-B — result list and grid visual polish | `pending` | Not started. |
| M4-C — UI primitives and standardized surfaces | `pending` | Not started. |
| M5-A — global settings shell and theme editor redesign | `pending` | Not started. |
| M5-B1..n — high-usage plugin dialogs | `pending` | Not started. |
| M5-C1..n — remaining dialogs, messages and secondary windows | `pending` | Not started. |
| M6-A — optional advanced effects as constrained opt-in | `pending` | Not started. |
| M6-B — final visual, accessibility and performance acceptance | `pending` | Not started. |

## M0-A checklist

- [x] Read repository instructions, the full execution contract, the attached goal objective, and the available historical benchmark references.
- [x] Record current branch/SHA, working-tree diff inventory, `Cargo.lock` digest, Rust/Cargo versions, and locked offline dependency-tree outputs.
- [x] Inventory the current egui family, direct `winit` and `raw-window-handle` dependencies/usages, active eframe feature flags, and Cargo patch/git dependency declarations.
- [ ] Reconcile HEAD with `multi_launcher(20261012-203406).zip`; the archive was not present in the attachment's recursive file inventory or repository search, so the comparison is unresolved. The mission objective's current-HEAD baseline is recorded and parent requested archive clarification asynchronously.
- [x] Record original Windows compile diagnostics: `cargo check --locked --all-targets --offline` passed, exit 0, dev profile, 4m11s; compiler warnings are summarized in [baseline.md](baseline.md).
- [x] Complete and classify original test diagnostics: the full no-fail-fast run completed with 6,442 run, 6,439 passed, 3 failed and 41 skipped. The focused fixture-correction run passed 4/4 without changing the original receipt. See [`nextest-status.txt`](receipts/nextest-status.txt) and [`regressions.md`](regressions.md).
- [x] Confirm remote builder applicability: no remote Windows builder is configured or supplied; the local `x86_64-pc-windows-msvc` host is the migration host.
- [x] Save local M0-A checkpoints: inventory `050a922f`, reviewed fixture correction `f45e3773` (four focused tests passed). Original failed full-suite receipts are retained. User directed local-only checkpoints. Archive comparison remains an explicit evidence limitation; current HEAD is the objective-authorized baseline.

See [baseline.md](baseline.md), [dependency-audit.md](dependency-audit.md), [migration-decisions.md](migration-decisions.md), and raw command outputs under `receipts/`.
