# Track A checkpoint ledger

## Provenance and execution contract

- Branch: `performance-optimization`.
- Base: `02399cf0848de2b429dfd9fae80a141ce50d972d` (clean `performance` checkout).
- The user explicitly confirmed current HEAD as authoritative and waived the referenced ZIP comparison on October 9, 2026.
- Both companion plans in `docs/plans/` and repository/agent instructions were read. Cited source mechanisms correspond to this checkout; shifted native/Quick Notes line hints are nonmaterial.
- Remote `origin/performance` was confirmed at the base SHA using `git ls-remote`. Feature branch did not exist locally or remotely. Push write access will be confirmed by the first checkpoint push, not inferred from read access.
- Track B is excluded. Implementation writers run sequentially. Parent owns commits/pushes and this ledger.
- Results use PASS / FAIL / NOT RUN / NOT MEASURED / SKIPPED. A pending checkpoint is not complete.

| Checkpoint | Objective | State | Tests / evidence | Review | Commit | Push |
| --- | --- | --- | --- | --- | --- | --- |
| M0-A | Bounded opt-in owner metrics | complete | PASS: 3 performance library tests; changed-file rustfmt and diff checks | planner + parent diff review; corrections resolved | stage commit pending | pending |
| M0-B | Deterministic isolated fixtures | pending | — | — | — | — |
| M0-C | Freeze pre-change baseline | pending | — | — | — | — |
| M1-A | Consistent note revision publication | pending | — | — | — | — |
| M1-B | Cheap note refresh gate | pending | — | — | — | — |
| M1-C | Notes regression and comparison | pending | — | — | — | — |
| M2-A | Shared command resolution context | pending | — | — | — | — |
| M2-B | Bounded matching history candidates | pending | — | — | — | — |
| M2-C | History parity and comparison | pending | — | — | — | — |
| M3-A | Visible launcher list rows | pending | — | — | — | — |
| M3-B | Complete visible grid rows | pending | — | — | — | — |
| M3-C | Lightweight Quick Notes projection | pending | — | — | — | — |
| M3-D | Variable-height Quick Notes virtualization | pending | — | — | — | — |
| M4-A | Actions reload reuses indexed tail | pending | — | — | — | — |
| M4-B | Bounded scan worker | pending | — | — | — | — |
| M4-C | Generation/config guarded publication | pending | — | — | — | — |
| M4-D | Watcher race and stall evidence | pending | — | — | — | — |
| M5-A | Tick contract characterization | pending | — | — | — | — |
| M5-B | Current-source presentation ordering | pending | — | — | — | — |
| M5-C | Native reliability/review/comparison | pending | — | — | — | — |
| M6-A | Sampling/HUD measurement decision | pending | — | — | — | — |
| M6-B | Conditional display metadata reuse | pending | measurement gate | — | — | — |
| M6-C | Conditional retained GDI resources | pending | measurement gate | — | — | — |
| M7-A | Bounded cross-surface regression | pending | — | — | — | — |
| M7-B | Final comparable report | pending | — | — | — | — |

