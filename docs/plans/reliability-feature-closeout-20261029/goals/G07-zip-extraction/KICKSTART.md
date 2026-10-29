# G07 KICKSTART — G14 ZIP: Bounded Extraction Writer and Safe UI Integration

**Binding task contract:** [GOAL.md](GOAL.md), overarching [MEGA_GOAL.md](../../MEGA_GOAL.md), [ORCHESTRATOR_KICKSTART](../../ORCHESTRATOR_KICKSTART.md), and [VERIFICATION_POLICY](../../VERIFICATION_POLICY.md). If run under the orchestrator, **do not create a new independent program or ask for this already supplied task scope**.

## Dispatch packet

**Desired result:** Complete the accepted ZIP catalog/planning foundation with a bounded, traversal-safe extraction writer and, only after writer proof, a user-facing selection/extraction workflow with truthful progress and cancellation.

**Hard gate:** A safe writer and exact output contract must precede extraction UI enablement. Before doing any source work, read the current Goal mutable progress and the authoritative `PROGRAM_STATUS.md` and check this gate. If unmet, write `DEFERRED_DEPENDENCY` with exact missing proof; do not do the blocked implementation, and return to the parent so the next independent goal proceeds.

**Current-state references:**

- [`src/archive/catalog.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/archive/catalog.rs) — `read_catalog` bounded ZIP metadata and revision
- [`src/archive/model.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/archive/model.rs) — entries, encoding/compression, limits, safe relative paths
- [`src/archive/plan.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/archive/plan.rs) — `build_extraction_plan`, physical entry ID, destination policy
- [`src/archive/mod.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/archive/mod.rs) — currently exports catalog and plan, not a writer
- [PR #1455](https://github.com/multiplex55/Multi_Launcher/pull/1455) — G14 read-only archive catalog/planning accepted; extraction UI/writer absent

## Execute sequentially

1. **Safe start:** discover current repository/worktree/AGENTS instructions; fetch and verify `origin/master`; check user changes, other worktrees and active Git operations. Resume an existing matching task branch only when its program identity and dependency ancestry are valid. Otherwise let parent create fresh named branch from program-start base or accepted dependency. Historical SHA `56223fdc123396f4c41c48e0542506d912c6c634` is planning evidence, never a reset instruction.
2. **Role assignment:** parent orchestrator owns Git, progress and conflicts. Have planner (configured Astra/max) inspect real owners and produce one approved bounded milestone handoff. Have exactly one implementer/author (configured Astra/xhigh, optionally Sol/xhigh if explicitly supported and attributed) execute that handoff. Reviewer (configured Astra/max) examines the stable diff and focused proof. One active child at a time; do not require a second worker to start another goal.
3. **Create durable task state:** write `docs/plans/execution/multi-launcher-reliability-closeout-20261029/goals/G07/` with `SOURCE_REVALIDATION.md`, `PROGRESS.md`, `VERIFICATION.md`, `RISKS.md`, role folders and `follow-on/`. Copy or link this exact Goal/Kickstart. Initialize the mutable progress in GOAL.md; docs baseline commit/push before source work.
4. **Execute checkpoints in order.** Do not enter a subsequent checkpoint until the previous relevant source evidence is accepted or a truthful defer/no-change boundary has been documented:

   - **M0-A — Security and format decision record:** Inspect accepted plan/catalog and real command/UI reachability. Explicitly define v1 support for stored/Deflate only if decoded safely, allowed entry kind, names, output limits, ratio/window/size quotas, external attributes, symlink handling, duplicate targets, encrypted/unsupported modes. Record fail-closed unsupported cases.
   - **M1-A — Physical-entry validation and output reservation:** Bind each selected `ArchiveEntryId` to the same immutable source/revision and local header/offset, not just its textual path. Recheck captured output policy/occupancy. Refuse traversal, absolute/root/drive paths, mixed-separator normalization escapes, symlink/reparse/link attacks and duplicate normalized output. Reserve safely under destination using directory-handle/identity-aware primitives; check exact Windows effect semantics before claiming no-clobber.
   - **M1-B — Quota-bound streaming writer:** Decompress into task-owned staging with checked size/ratio/resource counters, cancellation, CRC/size/EOF validation and mismatch reporting. Publish only fully verified files under an explicit overwrite policy; on failure clean staging without deleting pre-existing targets. Unrecoverable cleanup/partial publication is recorded, never silently retried or called atomic batch.
   - **M2-A — Focused adversarial archive proof:** Test benign stored/Deflate, ZIP64/unsupported variants, duplicate normalized path, traversal, symlink, files > declared size, CRC failure, encrypted flags, existing target, source changed between catalog and apply, quota/cancel, malicious local header offset. Tests use synthetic archives and owned temp roots; no full external corpus.
   - **M2-B — Minimal UI gated by writer acceptance:** Only if M1 writer and adversarial tests pass, expose selection, destination, preview blockers, explicit overwrite choice, progress, cancellation and per-file result. Retain catalog behavior for unsupported archives and never allow an invalid or stale plan to start writes.
   - **M3-A — Close or defer:** If safe physical extraction/native publishing cannot be established in bounded proof, preserve read-only catalog/planner, mark writer/UI deferred and generate one exact follow-on safety Goal/Kickstart; do not silently enable a best-effort unzip path.

5. **Verification:** choose actual nonzero targeted tests matched to changed modules; respect per-checkpoint compilation/execution caps in VERIFICATION_POLICY. No default full nextest/Clippy/native campaign. Source-only claims require inspection, not invented passing tests. Native-proof-specific user flows need supported automation, real effects in owned fixtures and actual results; otherwise mark that piece `NATIVE_PROOF_DEFERRED`.
6. **Review and publication:** review the exact cumulative source diff and changed callers. Apply at most one substantive correction cycle, rerun only the affected checks, and preserve failures. Commit coherent accepted checkpoints; attempt one normal push after each. Report real task/parent/head/upstream and push outcome. Parent may import **accepted** changes only; do not independently merge to master or start another Goal.
7. **Follow-on package duty:** for each new *source-supported* actionable issue, write a fully scoped Goal/Kickstart pair in the discovering role's folder and parent canonical library. Include confidence, counterevidence, chosen existing owner, exact scope/callers, migrations/defaults, stage boundaries, focused tests/native proof, dependencies, model assignments, small commits, rollbacks, and defer criteria. Do not execute generated follow-ons or invent an unbounded queue.
8. **Stop handling:** if hard proof/tool/permission/native scope fails, save the source/condition, actual command/error, exact resume step and a Markdown report. Return a `DEFERRED_*` state and let the parent proceed to the next **independent** goal. Never claim future background work or hand off an unfinished source mutation as accepted.

## Must-preserve checklist

- All source entry IDs, path validation, quotas and cancellation stay bound to the accepted immutable catalog/plan.
- No symlink traversal, reparse escape, overwrite without the chosen consent, memory bomb, unbounded decompression, unsupported encryption or partial file published as success.
- The existing read-only archive catalog remains usable even if extraction is deferred.
- No privileged shell extraction invocation, process spawning or broad parser rewrite.

## Exact test intent

- Synthetic harmless ZIP with exact expected payload/CRC and supported methods; same decoded bytes in output.
- Traversal/absolute/path collision/symlink/reparse rejection; source changed while plan exists; local header mismatch.
- Decompression bomb/quota/cancellation mid-file, existing output conflict, injected write failure and cleanup/receipt truthfulness.
- Minimal UI source/selection/destination/preview/error with a focused test; native output proof only if effect-time Windows safety assertions require it.

## Done report to orchestrator

Include `G07` outcome (`ACCEPTED_COMPLETE`, `PARTIAL_ACCEPTED`, `NO_CHANGE_REVALIDATED` or precise defer), source/master/branch/closing SHA/push, diff and owners, tests actually run + nonzero counts, native evidence vs `NOT_RUN`, reviewer and model receipts, unresolved issues, created follow-on Goal/Kickstart paths and integration eligibility. **Never claim the presence of a Kickstart itself proves implementation acceptance.**
