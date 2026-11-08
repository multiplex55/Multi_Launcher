# Automated independent A-C peer review

This is an automated independent peer review by the configured reviewer agent, not a human GitHub approval. It is the requested interim visibility checkpoint for the full Launcher and Utility Fixes Goal. **D-H remain active and proceed immediately after publication, on the same branch.**

## Exact review boundary

- Baseline: `c10da8ddd90d3ebc7cf477409deb7921921a10d8`.
- Initial A-C code tip: `2c01ab4fa871521b93065c6bf9622e4fe06ca55a`.
- Corrective commit and final `abc_reviewed_tip_sha`: **`8f4db7e10004e68bfc547832614adbc49fcee125`**.
- Initial review inspected baseline through the initial tip; follow-up inspected the correction and relevant context consumers. No source edits or Cargo commands were performed by the reviewer.

## Scope and outcome

A: actual root-versus-detached presentation, query/Jump ownership, retained pointer transfer, native/visibility generations and protected owners. B: root-results-local fade with unchanged widget identity, virtualization and navigation. C: release-profile inheritance, unwinding, bounded feature review, idle capability work and executor/watch-service lifecycle.

The review found one substantive issue. It is resolved; follow-up found no additional concrete findings.

| Severity | Finding | Disposition |
| --- | --- | --- |
| P2 | With no tool open, plain ROOT returned no ordinary focus context. A query click could not stage active Jump cancellation; the original regression's detached Markdown masked this case. | Fixed in `8f4db7e10004e68bfc547832614adbc49fcee125`: stable empty-layer context persists across cancellation. Regression now covers bare ROOT and detached Markdown, rejected discarded press, a fresh real click, Unicode and later focus. Independent follow-up confirmed resolution without weakened ownership checks. |

The reviewer confirmed local fade restoration, effective iteration/development settings, retained unwind behavior and fresh Hot Folder admission/lease validation. The idle hint does not authorize execution or move filesystem probes into the watcher lease.

## Verification actually performed

All Nextest selections used package `multi_launcher`, `--cargo-profile fast-dev --lib --no-fail-fast`, with `RUST_MIN_STACK=16777216`. Compilation completed separately from bounded 100-second pure-test execution. Exact selectors and receipt paths are in [STATUS.md](STATUS.md).

| Check | Actual result |
| --- | --- |
| M1-A focus/query selection | 15/15 passed; execution 9.363s; run `6d2c6a33-45bf-4a96-8ffa-30f237ad2bc2` |
| M2-A existing list/grid viewport interaction tests | 2/2 passed; execution 2.962s; run `12b46689-66d7-4b5d-a08c-17e96e77db1d` |
| M3-B idle callback plus execution/lifecycle cases | 4/4 passed; execution 6.359s; run `48347404-bd13-4c2f-8d48-0e362ac74fc3` |
| Review correction plus protected/stale owner regressions | 8/8 passed; execution 8.485s; run `5f6bbcba-8152-4463-8e63-2ffc52ec255a` |
| Owned test-process receipts | Exit 0; cleanup and output drain verified |
| Manifest/profile inspection and Cargo feature graphs | Passed; no safe dependency removal identified; lock unchanged |
| Ordinary `cargo build --release` | Passed once in 13m10s on Windows, at the initial A-C code tip |
| Diff inspection / `git diff --check` | Passed |

The release artifact and native smoke predate the later query correction. They establish C's release configuration/build evidence, and are not represented as a binary containing that correction. Corrected root logic has focused fast-dev evidence. Profile/Hot Folder behavior was unchanged by the correction; no second release build was run.

Release settings are optimization level 3, Thin LTO, 16 codegen units, symbol stripping and unwind panics. Iteration explicitly preserves unstripped symbols. Thin LTO can increase build/link work; no benchmark or measured idle CPU claim is made. Existing warnings in unchanged Note Panel/Table Utility and vendored code were recorded without unrelated cleanup.

## Native evidence and limitations

The exact release executable was launched with disposable settings, actions and note storage. Native keyboard navigation reached the actual query; text entry, Notes search, selection and Return opened Quick Notes with the disposable note. Alt+F4 exited normally; owned cleanup/output drain were verified. The fixture note remained unchanged, and no task launcher remained in process/helper inventories.

Screenshot capture timed out twice, including a refreshed/re-activated target retry. Indexed query click failed because coordinate input geometry was unavailable. Accessibility and keyboard worked, but a Notes accessibility edit had no observed effect and is not counted as success. No alternate capture helper, guessed coordinates or security/privacy changes were used.

**Still native-unverified:** A's genuine mouse-click handoff with multiple detached editors and B's temporal fade/flicker, wheel and context-menu visual checks. State/egui tests do not replace native proof. A functioning capture/input-geometry session is required for those checks. Host Windows 10 build 19045 also limits later supported Explorer tab verification; it is not treated as evidence for H.

## Continuation

The remaining approved checkpoints are D remembered scratch close; E viewer drag; F top pending decision; G sortable/filterable bounded Folder Size results, reveal and capacity; H actual Shell baseline correction; then final focused review/Clippy and exact-tip handoff. Publication creates no approval, CI or merge wait. The parent continues immediately on `codex/launcher-utility-fixes`.
