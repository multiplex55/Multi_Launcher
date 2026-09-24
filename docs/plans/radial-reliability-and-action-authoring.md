# Radial reliability and action authoring — execution ledger

This is a mutable execution ledger. Statuses began pending; each change below records actual progress. Creating or updating this file is not application implementation or validation.

## Authority

Starting archive: `multi_launcher(20260924-210943).zip`

Archive SHA-256: `f21ca0ee92d1b7572f779b936bb3ce1de7e685314f685ae8711d1734952d2544`

Approved behavior: `../multi_launcher_radial_reliability_approved_requirements.md`

Implementation tasks: `../multi_launcher_radial_reliability_codex_plan.md`

Acceptance: `../multi_launcher_radial_reliability_acceptance_matrix.md`

Source evidence: `../multi_launcher_radial_reliability_source_notes.md`

Critical override of older plans: a short launcher tap toggles the grid AND dismisses the runtime radial. Designer and its draft remain independent.

## Baseline identity — fill from actual checkout/tools

| Field | Actual value |
|---|---|
| Branch / starting HEAD | `radial-menu-3` / `f6d0395f5b7cee987b09f1dd11f52361ba74fcf5`; upstream `origin/radial-menu-3`, ahead/behind 0/0 |
| Dirty diff / source manifest identity | Clean tracked and untracked tree before M0; empty staged/unstaged diff. M0 documentation move/update is recorded below and must not be confused with application source. |
| Archive comparison / intervening changes | Archive is identified by name and declared SHA-256 above but was unavailable in the supplied attachment directory and inspected repository/user locations. The source notes' 21 recorded fingerprints match only `AGENTS.md` and `src/dashboard/widgets/quick_tools.rs`; 19 files differ in bytes, including Cargo files and source/runner files. EOL differences may contribute; direct ZIP comparison and semantic attribution remain unavailable. Current checkout is the implementation source of truth. |
| Historical branch point / first feature commit (optional, verified only) | Verified `git merge-base HEAD origin/radial-menu-2` = `39cb72e251799a9c794e95f4e329f4dc88913c9b`; sole subsequent commit before M0 is `f6d0395f docs`, adding this requirements packet. Comparison metadata only. |
| Toolchain / Cargo / Nextest versions | `rustc 1.97.1 (8bab26f4f 2026-07-14)` x86_64-pc-windows-msvc; `cargo 1.97.1`; `cargo-nextest 0.9.135` |
| Target / profile / target directory | Single package `multi_launcher 0.1.0`, edition 2024; default Windows MSVC target; target directory `G:\Repos\rust\Multi_Launcher\target`. No build profile selected for M0. |
| Candidate application path / SHA-256 | No M0 source-matched candidate built. Existing `target\debug\multi_launcher.exe`: `3daa5c82d05315109ed22786dd56e2aa048963f2a3f45a9b5db872fec07c39ec`; existing release app: `d3ee35ea948d98f4a59cd7cd821a6da8fd6f3676706c94824f544608f83973f6`. Existing artifacts are not acceptance evidence. |
| Acceptance runner path / SHA-256 | No M0 source-matched runner built. Existing debug runner: `0d60b994dd40feefbd9c1c4438e99c27e8c5ecc187ed2eb596bdb22a9f49e2b4`; existing release runner: `e33a5f79001234e16b70074b132f60d08e0d26663d3e7e1c8e4de7454b08fafd`. |
| Build command and source-to-artifact record | None for M0; no existing acceptance report established as source matched. |
| Native environment / monitor-DPI / integrity | Windows 10.0.19045 Home 64-bit, interactive session reported. Monitor/DPI, process integrity, input desktop, and injection eligibility await native preflight; topology query was access denied. |
| Fixture / profile hashes | Pending M1 source-matched fixture and isolated profile. Current runner fixture configures F11. |
| Authorized consistent copied profile available | Not established; optional copied-profile coverage remains conditional. |

## Milestones

| ID | Scope | Code status | Verification status | Commit/diff | Evidence / blocker |
|---|---|---|---|---|---|
| M0 | Source baseline and fixture/test map | complete | complete (metadata inspection) | `f6d0395` + M0 ledger diff | Direct ZIP unavailable; byte fingerprint discrepancy and native preflight limits recorded below. |
| M1 | Hotkey/grid/runtime reliability and exact-chord runner | pending | pending | — | — |
| M2 | Typed bindings and persistence migration | pending | pending | — | — |
| M3 | Shared query resolution and execution/handoff | pending | pending | — | — |
| M4 | Shared authoring editor and Add to radial | pending | pending | — | — |
| M5 | Multi-select/bulk editing/navigation | pending | pending | — | — |
| M6 | Presets/gallery/simple controls/density | pending | pending | — | — |
| M7 | Full regression/native acceptance/review | pending | pending | — | — |

Use `pending`, `in_progress`, `code_complete`, `complete`, or `blocked` explicitly. A milestone is `complete` only when its required acceptance passed. Record a known failure separately from unavailable environment evidence.

## Gates

| Gate | Meaning | Status | Exact source/candidate identity | Report/exit/counts |
|---|---|---|---|---|
| H | Rapid exact-chord native hotkey reliability | pending | — | — |
| P | Model/migration/store/package compatibility | pending | — | — |
| Q | Same-ranked query resolution and correct execution | pending | — | — |
| C | Core editor and insertion, H/Q rerun | pending | — | — |
| D | Bulk/nav usability | pending | — | — |
| S | Skin/geometry/appearance compatibility | pending | — | — |
| R | Final full suite/native/review | pending | — | — |

## Long-running jobs

| Job ID/PID | Command + working directory | Source identity | Started | Log/metadata path | Last observed state | Exit code |
|---|---|---|---|---|---|---|
| None launched | — | — | — | — | — | — |

One expensive job at a time. Check actual process state before starting another. Use completion notification or the user's 10–20 minute observation cadence for long jobs. Keep native test timing independent.

## Milestone record template

### M0 — Baseline and fixture map (complete)

Objective/requirement IDs: Establish the current source, archive reference, toolchain, test/fixture map, and evidence limits before M1.

Starting and ending source/commit/diff: Clean `f6d0395f5b7cee987b09f1dd11f52361ba74fcf5` at start; documentation-only move/update of this ledger at end. No application source changes.

Changed files/architectural owners: This ledger moved from `docs/` to the plan's documented `docs/plans/` path and gained baseline facts.

Important decisions and intentional behavior changes: The checked-out implementation remains authoritative; no ZIP content was substituted. Older radial-survival assertions must be migrated in M1 to the approved short-tap dismissal contract. No behavior changed in M0.

Tasks completed: Checked Git state and verified branch comparison metadata; inspected archive fingerprint notes, Cargo targets, runner fixture/CLI, existing artifacts, toolchain, and native prerequisites.

Tests added/migrated and why: None; M0 is metadata inspection. Test map: library and binary inline tests; 67 integration targets including GUI/focus/trigger visibility, hotkey, command, query, history, settings, mouse and macro suites; `src/bin/radial_acceptance.rs` with `native.rs`, `suite.rs`, and `copied_profile.rs`. The runner presently uses F11 and has 31 case IDs against a 32-case cap; `--suite`/`--hotkey` are proposed, not implemented.

Commands, discovered counts, pass/fail/skip counts, exit codes: `git status --porcelain=v2 --branch`, `git diff --stat`, `git diff --cached --stat`, `git rev-parse HEAD`, `git merge-base HEAD origin/radial-menu-2`, `rustc -Vv`, `cargo -Vv`, `cargo nextest --version`, `cargo metadata --no-deps --format-version 1`, file/hash inspection; inspection commands succeeded except the unavailable archive and access-denied topology query. No tests or builds run, so no test count or pass claim.

Candidate/runner/profile identities: Existing binary hashes are recorded in Baseline identity; source-matched M1 candidates and isolated profile are pending.

Native cases and report path: None run. Existing reports are not credited toward Gate H.

Source-backed root-cause evidence versus remaining hypotheses: Focused ROOT failure cause is not established. Async restoration is a hypothesis to test, not a confirmed diagnosis.

Performance measurements, if any: None.

Unresolved blockers/limitations: Direct ZIP comparison was impossible because the named archive was unavailable; native monitor/DPI/integrity/input-desktop checks await runner preflight. These do not reopen approved product decisions.

Next bounded milestone: M1 hotkey/grid/runtime ordering, deterministic tests, and exact-chord native runner proof.

### Mx — title

Objective/requirement IDs:

Starting and ending source/commit/diff:

Changed files/architectural owners:

Important decisions and intentional behavior changes:

Tasks completed:

Tests added/migrated and why:

Commands, discovered counts, pass/fail/skip counts, exit codes:

Candidate/runner/profile identities:

Native cases and report path:

Source-backed root-cause evidence versus remaining hypotheses:

Performance measurements, if any:

Unresolved blockers/limitations:

Next bounded milestone:

## Review findings

| Finding | Severity | Source location | Required remediation | Status | Retest evidence |
|---|---|---|---|---|---|
| Review not run | — | — | — | pending | — |

## Final report checklist

Approved behavior implemented; intentional old-test changes explained; final Nextest results; applicable doctests; final candidate and runner hashes/source manifest; mandatory exact-chord/native cases; query/UI/no-flash/confirmation checks; Designer/persistence/skin checks; cleanup; copied-profile result or honest absence; independent review/remediation; remaining environment limitations; no unsupported “no regressions” claim.
