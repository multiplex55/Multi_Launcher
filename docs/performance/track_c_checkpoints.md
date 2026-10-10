# Track C checkpoint ledger — LOCAL ONLY

Track C has 29 checkpoints in the approved dependency order from `docs/plans/multi_launcher_track_c_codex_plan.md`. This ledger is the live progress record. The companion [`track_c_baseline.md`](track_c_baseline.md) records provenance and toolchain details; it is not a frozen G0 measurement report yet.

## Status and evidence conventions

- State values follow the approved vocabulary: `PASS`, `SKIPPED (measured gate)`, `FAIL`, `IN_PROGRESS`, and `PENDING`. Explicit `NOT RUN` and `NOT MEASURED` labels describe evidence, not checkpoint state.
- Every checkpoint is **LOCAL ONLY**. No checkpoint authorizes push, fetch/pull, merge, rebase, reset, or remote PR activity.
- For every `PENDING` row, the source SHA is **NOT STARTED** and its commit is **PENDING**; profiles and fixtures are **NOT RUN**; test count is **0** and actual test result is **NOT RUN**; p50/p95 and work-unit change are **NOT MEASURED**; output identity/parity is **NOT CAPTURED**; reviewer is **PENDING**; risk is **UNASSESSED**. Each pending row is LOCAL ONLY.
- For `C0-A`, source is the approved starting commit `bcfd24d226ffb46c85f9f7ce7bdd919466191070`; the documentation commit is **PENDING parent review/commit**. No profile or fixture was used; 0 tests/builds were run; actual test result is **NOT RUN**; p50/p95 and work-unit change are **NOT MEASURED**; runtime output parity is **NOT APPLICABLE (documentation-only)**; reviewer sign-off is **parent/planner PASS (source inventory)**; remaining risk is unmeasured runtime performance; LOCAL ONLY.
- Checkpoint owners below identify the planned component boundary from the approved plan. They do not assign future implementation agents. The detailed C0-A source/caller/test inventory is recorded after read-only planner reconnaissance; parent/planner source inventory sign-off is PASS.

## Checkpoints

| ID | Objective / planned owner | State | Source SHA / local commit | Evidence: profiles, fixtures, tests, metrics, output parity, reviewer, risk | LOCAL ONLY |
|---|---|---|---|---|---|
| C0-A | Establish provenance, owners, invariants / parent orchestrator | PASS | Source `bcfd24d226ffb46c85f9f7ce7bdd919466191070`; docs commit PENDING parent review/commit | No profile or fixture; tests/builds 0, actual result NOT RUN; p50/p95/work delta NOT MEASURED; output parity N/A (docs-only); reviewer parent/planner PASS; risk: G0 not frozen, runtime unmeasured. | YES |
| C0-B | Add bounded opt-in responsiveness telemetry / performance owners | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C0-C | Add deterministic fixtures and semantic oracles / performance workload and owner tests | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C0-D | Freeze current-source G0 and native recipe / parent and reviewer | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C1-A | Characterize full note relationship semantics / note panel and note cache | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C1-B | Fuse note and todo relationship traversals / note panel | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C1-C | Gate a versioned reverse relationship projection / note cache | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C1-D | Gate lightweight alias-collision lookups / note alias display | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C1-E | Characterize action publication and consumers / GUI action publication and search cache | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C1-F | Gate action catalog publication improvements / action publication | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C1-G | Verify note/action semantics and measurements / notes and actions | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C2-A | Capture complete ordered search oracle / search and result-generation owners | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C2-B | Delay Action materialization until stable sorting / search | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C2-C | Gate safe candidate screening and no-op invalidation / search | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C2-D | Gate exact cold list-geometry reuse / root list geometry | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C2-E | Gate exact cold grid-geometry reuse / root grid geometry | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C2-F | Gate Quick Notes cold layout and metadata reuse / Quick Notes geometry | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C2-G | Measure query through first usable viewport / search and root rendering | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C2-H | Independently check search and geometry parity / reviewer | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C3-A | Characterize complete-catalog startup readiness / startup | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C3-B | Gate safe startup critical-path changes / startup | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C3-C | Measure event queue age, bursts and repaint fairness / GUI event queue | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C3-D | Gate coalescing for proven idempotent events / event delivery | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C3-E | Gate bounded event-drain fairness / GUI event drain | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C3-F | Verify startup/event races and native behavior / startup and event owners | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C4-A | Run focused cross-surface regression checks / affected owners | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C4-B | Run or explicitly mark native responsiveness acceptance / native recipe owner | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C4-C | Publish matched G0/G1 results and limits / parent and reviewer | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
| C4-D | Close all 29 records and hand off locally / parent orchestrator | PENDING | NOT STARTED / PENDING | Per pending evidence defaults above. | YES |
