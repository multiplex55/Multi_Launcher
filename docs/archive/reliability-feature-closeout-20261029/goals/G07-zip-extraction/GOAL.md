# G07 GOAL — G14 ZIP: Bounded Extraction Writer and Safe UI Integration

> **Program:** `multi-launcher-reliability-closeout-20261029`  
> **Disposition at plan creation:** `NOT_STARTED / NOT_EXECUTED`  
> **Priority:** P2 / security-sensitive utility  
> **Lane:** gated extraction implementation  
> **Prerequisite:** A safe writer and exact output contract must precede extraction UI enablement  
> **Companion:** [KICKSTART.md](KICKSTART.md)  
> **Historical planning-time master:** `56223fdc123396f4c41c48e0542506d912c6c634` (October 29, 2026; verify current upstream at runtime; **never reset to this SHA**).

## 1. Goal and user-visible benefit

Complete the accepted ZIP catalog/planning foundation with a bounded, traversal-safe extraction writer and, only after writer proof, a user-facing selection/extraction workflow with truthful progress and cancellation.

**Scope of user authorization when this package is launched:** execute only the milestones and bounded continuation rules in this Goal; this isn't permission to import unaccepted old queue branches, broaden into refactors, run a full test campaign, or merge into `master` automatically. This is a new action program, not the prior Astra research-only program.

## 2. Source-grounded initial observations

- Current source contains strong bounded ZIP catalog/plan validation and captures physical entries. `src/archive/mod.rs` exports `read_catalog`/`build_extraction_plan`, not an extraction implementation.
- ZIP filenames, duplicate archive entries, symlinks/reparse points, legacy encodings, unsupported compression/encryption, output collisions and bombs are safety-sensitive. A direct `zip::extract`/unzip convenience helper is not an acceptable shortcut.
- The earlier M1-A project did not authorize a writer or native output side effects.

The above is a **planning-time source observation**, not proof of runtime behavior or an assertion that all functions remain identical after this program starts. The worker must re-read the named owners at its actual task base and classify any drift before editing. Important links are pinned to the merged master for provenance:

- [`src/archive/catalog.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/archive/catalog.rs) — `read_catalog` bounded ZIP metadata and revision
- [`src/archive/model.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/archive/model.rs) — entries, encoding/compression, limits, safe relative paths
- [`src/archive/plan.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/archive/plan.rs) — `build_extraction_plan`, physical entry ID, destination policy
- [`src/archive/mod.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/archive/mod.rs) — currently exports catalog and plan, not a writer
- [PR #1455](https://github.com/multiplex55/Multi_Launcher/pull/1455) — G14 read-only archive catalog/planning accepted; extraction UI/writer absent

## 3. Exact ownership, permitted changes and exclusions

- `src/archive/` owns physical entry lookup, exact plan validation, bounded stream decompression and output receipt; extend with a module only when necessary.
- Existing archive command/GUI owner discovered by current source inspection; after writer accepted, add small UI for source/destination selection, preview and progress.
- Task-owned temporary destinations only for test; no user folder mutation or network archive downloads in automated acceptance.

**Non-goals / changes to refuse:**

- No archive creation, multipart/encrypted archive support, password manager, arbitrary extraction plugins or network ZIP download.
- No recursive extraction of nested archives or dynamic child process.

New files may be created under the goal-specific execution directory `docs/plans/execution/multi-launcher-reliability-closeout-20261029/goals/G07/` and in tightly scoped production owners above only. Do not edit `.codex` role/configuration files, historical research reports, unrelated worktrees, vendored crates, snapshots or independent user content. A previously uncommitted or blocked candidate is **evidence only**, never an implementation source to cherry-pick wholesale.

## 4. Required behavior and preserved invariants

- All source entry IDs, path validation, quotas and cancellation stay bound to the accepted immutable catalog/plan.
- No symlink traversal, reparse escape, overwrite without the chosen consent, memory bomb, unbounded decompression, unsupported encryption or partial file published as success.
- The existing read-only archive catalog remains usable even if extraction is deferred.
- No privileged shell extraction invocation, process spawning or broad parser rewrite.

If a required invariant has a concrete counterexample that the stated milestone cannot fix without substantial adjacent work, **stop that checkpoint**, preserve its evidence and author an explicit follow-on Goal/Kickstart. Continue the next independent program goal. An accepted subset may be integrated only if default behavior is safe, the subset is independent and the acceptance scope is truthful.

## 5. Milestones and bounded handoffs

| Checkpoint | Owner and outcome | Acceptance / intended commit |
|---|---|---|
| `M0-A` | Security and format decision record | Scoped work + tangible artifact; `docs(closeout): [G07/M0-A] ...` or `fix/feat` when source changed |
| `M1-A` | Physical-entry validation and output reservation | Scoped work + tangible artifact; `docs(closeout): [G07/M1-A] ...` or `fix/feat` when source changed |
| `M1-B` | Quota-bound streaming writer | Scoped work + tangible artifact; `docs(closeout): [G07/M1-B] ...` or `fix/feat` when source changed |
| `M2-A` | Focused adversarial archive proof | Scoped work + tangible artifact; `docs(closeout): [G07/M2-A] ...` or `fix/feat` when source changed |
| `M2-B` | Minimal UI gated by writer acceptance | Scoped work + tangible artifact; `docs(closeout): [G07/M2-B] ...` or `fix/feat` when source changed |
| `M3-A` | Close or defer | Scoped work + tangible artifact; `docs(closeout): [G07/M3-A] ...` or `fix/feat` when source changed |

### M0-A — Security and format decision record

**Objective and detailed handoff.** Inspect accepted plan/catalog and real command/UI reachability. Explicitly define v1 support for stored/Deflate only if decoded safely, allowed entry kind, names, output limits, ratio/window/size quotas, external attributes, symlink handling, duplicate targets, encrypted/unsupported modes. Record fail-closed unsupported cases.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M1-A — Physical-entry validation and output reservation

**Objective and detailed handoff.** Bind each selected `ArchiveEntryId` to the same immutable source/revision and local header/offset, not just its textual path. Recheck captured output policy/occupancy. Refuse traversal, absolute/root/drive paths, mixed-separator normalization escapes, symlink/reparse/link attacks and duplicate normalized output. Reserve safely under destination using directory-handle/identity-aware primitives; check exact Windows effect semantics before claiming no-clobber.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M1-B — Quota-bound streaming writer

**Objective and detailed handoff.** Decompress into task-owned staging with checked size/ratio/resource counters, cancellation, CRC/size/EOF validation and mismatch reporting. Publish only fully verified files under an explicit overwrite policy; on failure clean staging without deleting pre-existing targets. Unrecoverable cleanup/partial publication is recorded, never silently retried or called atomic batch.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M2-A — Focused adversarial archive proof

**Objective and detailed handoff.** Test benign stored/Deflate, ZIP64/unsupported variants, duplicate normalized path, traversal, symlink, files > declared size, CRC failure, encrypted flags, existing target, source changed between catalog and apply, quota/cancel, malicious local header offset. Tests use synthetic archives and owned temp roots; no full external corpus.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M2-B — Minimal UI gated by writer acceptance

**Objective and detailed handoff.** Only if M1 writer and adversarial tests pass, expose selection, destination, preview blockers, explicit overwrite choice, progress, cancellation and per-file result. Retain catalog behavior for unsupported archives and never allow an invalid or stale plan to start writes.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M3-A — Close or defer

**Objective and detailed handoff.** If safe physical extraction/native publishing cannot be established in bounded proof, preserve read-only catalog/planner, mark writer/UI deferred and generate one exact follow-on safety Goal/Kickstart; do not silently enable a best-effort unzip path.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### Reviewer handoff rule

For each completed production milestone, the parent must provide a reviewer: actual diff from the correct base, original objective, named callers and existing defenses, new tests and selection inventory, first meaningful failure if any, exact run/receipt, stop/accept criteria and non-goals. The reviewer should return **severity-ranked substantive findings only** and identify stale callers, duplicate state, false positive test passes, unsafe activation, migrations or ownership violations. One substantive correction pass is allowed before a recorded stop/spike; do not loop indefinitely.

## 6. Focused acceptance / negative regression tests

- Synthetic harmless ZIP with exact expected payload/CRC and supported methods; same decoded bytes in output.
- Traversal/absolute/path collision/symlink/reparse rejection; source changed while plan exists; local header mismatch.
- Decompression bomb/quota/cancellation mid-file, existing output conflict, injected write failure and cleanup/receipt truthfulness.
- Minimal UI source/selection/destination/preview/error with a focused test; native output proof only if effect-time Windows safety assertions require it.

Test execution is **not automatic simply because it is listed here**; first identify the actual Cargo package, present test names and test-target support. Select a nonzero meaningful subset. Prefer `cargo nextest list` and `cargo nextest run` scoped to named modules or exact test cases; if a test command selects zero, that is not PASS. Default allowance for one changed source checkpoint is one selected preparation, one selected run and at most two correction reruns **only when required**. The program-wide budgets in [VERIFICATION_POLICY](../../VERIFICATION_POLICY.md) control timings, native proof and documentation-only checks. Never weaken tests, delete failures or claim native acceptance from pure raster/source tests.

## 7. Decision gates: block only unsafe scope, not the whole queue

- If host facilities cannot demonstrate safe Windows path containment/publication, `WRITER_DEFERRED` and no UI affordance for writing.
- Any unsupported compression/name/attributes fail closed; cannot weaken catalog checks to raise success rate.

Classify each checkpoint as `PENDING`, `IN_PROGRESS`, `ACCEPTED_COMPLETE`, `PARTIAL_ACCEPTED`, `DEFERRED_DEPENDENCY`, `DEFERRED_WITH_DESIGN`, `NATIVE_PROOF_DEFERRED`, `FAILED_VERIFICATION`, or `NO_CHANGE_REVALIDATED`. The label must match the documented facts; a `PARTIAL_ACCEPTED` status is not feature-wide completion. On a true stop, create a durable per-goal `stalls/<checkpoint>-UTC.md` with actual source/ref, blocker, attempted one-shot fallback, unrun commands, ownership/cleanup, accepted files and exact next step; then return control to the parent to schedule the next independent goal.

## 8. Branch, commit, push, integration and model rules

1. Parent verifies actual `origin/master` and the program's accepted integration ancestry. Each **new** Goal receives its own branch `codex/closeout-g07-zip-extraction-<RUN_ID>` from the pinned program-start branch (contains only this plan) unless an explicitly accepted predecessor must be present. Dependent goals start from the prior **accepted integration commit** and record this exception. Never base on a rejected/dirty historical implementation tip.
2. Keep one active writer per goal; parent owns Git and the authoritative program ledger. Create the goal's execution directory and a `M0-A` docs baseline commit before editing production source. Accepted milestones each get meaningful Conventional Commit-style subjects including `G07/<milestone>` and one normal non-force push after commit. If push fails, preserve local commits and proceed; remote success must be recorded truthfully.
3. Parent integrates only reviewed, acceptance-gated production/documentation checkpoints into `codex/closeout-integration-<RUN_ID>` in dependency order (normally `git cherry-pick -x`). Do not import failed/unaccepted source. A rejected goal may contribute its **documentation-only** stop report/complete future pair, not an unsafe patch.
4. Role defaults verified from current `.codex`: orchestrator `gpt-6-astra/xhigh`; planner/reviewer `gpt-6-astra/max`; implementer `gpt-6-astra/xhigh`. These are configured role preferences, not proof of actual backend. If supported and selected for a narrow bounded implementation, `gpt-6.1-sol/xhigh` may be used deliberately, with honest attribution. No silent `.codex` configuration edits.
5. Roles contribute `roles/planner/`, `roles/implementer/`, `roles/reviewer/` and `roles/orchestrator/` Markdown under this goal's execution directory. New actionable recommendations get complete role-authored `roles/<role>/follow-on/<id>/GOAL.md` and `KICKSTART.md`; parent deduplicates into a canonical `follow-on/<id>/` library. Do not fabricate findings to fill quotas.
6. Do not ask the user to manually test between milestones; use supported app/native automation if available. If unavailable, log the gap and reserve a **single final manual-review checklist** for the program handoff.

## 9. Completed outcome definition

A safe, bounded extraction subset is explicitly proven and only that subset enabled; otherwise a complete security design and precise blocked proof are published with read-only behavior preserved.

**Required closure record:** real branch/base, all milestone dispositions, exact changed files, accepted and rejected interfaces, focused test counts and command receipts, actual native proof or `NOT_RUN`, model/role provenance, reviewer disposition, commits/pushes, output/follow-on paths, source safety constraints, and exact next action. No PR merge/CI success may be claimed without host-confirmed evidence.

## 10. Execution progress — mutable (Codex updates here)

- **Current state:** `DEFERRED_WITH_DESIGN / FAILED_VERIFICATION / WRITER_DEFERRED`; all M1-A source UNACCEPTED.
- **Run ID and branch:** `20261029-r01`, `codex/closeout-g07-zip-extraction-20261029-r01`.
- **Actual master/source base:** master `4a1283e6`, independent bootstrap `02d0ac64`; accepted integration `92c7076a` remains separate.
- **Current milestone:** M1-A stopped after corrected10/12 result with two native gate failures; no later milestone started.
- **Active role and requested/configured/resolved model:** independent documentation reviewer requested Astra/max, configured Sol/high, actual backend UNVERIFIED; no config change.
- **Last known commit / push:** parent preservation donor `e4e74ec8675c070e165849477fd33d9f490b63d2` normally pushed; all source UNACCEPTED, no source integration.
- **Next action:** independent documentation closure review then G08; canonical remaining pair in execution `goals/G07/follow-on/native-guard-publication-compatibility/`.
- **Blocker and scope:** guarded no-replace publication OS32; one source/alias deletion assertion failed with exact spelling UNKNOWN. No guard relaxation, path fallback or unsafe writer/UI enabled.

| Milestone | State | Real artifact, source commit, verification and owner | Next exact action |
|---|---|---|---|
| `M0-A` | COMMITTED_HANDOFF | Parent security/format decision and explicit packet at3f6408d6 | Preserve original binding contract |
| `M1-A` | FAILED_VERIFICATION / UNACCEPTED | Final10/12; two mandatory native failures; complete source/receipt/stop package | Independent disposition, then exact remaining native proof |
| `M1-B` | NOT_STARTED | No streaming writer | Requires accepted M1-A and fresh packet |
| `M2-A` | NOT_STARTED | No accepted writer proof | Actual CRC/EOF/quota/cancel/cleanup/partial receipts |
| `M2-B` | NOT_STARTED / WRITER_GATED | No UI/source changes | Requires accepted writer |
| `M3-A` | DEFERRED_WITH_DESIGN | Implementer stop and complete future pair authored | Parent reviewer/canonical closure |

### Append-only execution events

- Planning package created 2026-10-29; no goal code, test, branch, PR or merge was executed by generating these documents.
- Runtime2026-10-30 M0-A IN_PROGRESS: branch codex/closeout-g07-zip-extraction-20261029-r01 frombootstrap02d0ac64, fetched master4a1283e6unchanged, integration92c7076a authoritative. Planner security/format/firstwriterpacket next; no code/test/nativeZIPyet.
- 2026-10-30 M1-A bounded stop: initial8/12, corrected10/12; all source UNACCEPTED. Native ordinary guarded publicationOS32 and successful removal in source/alias loop (spellingUNKNOWN) remain. Initial preparation170.815s; aggregate correction411.306/600s; runs3.378s/3.492s. Material-gate stop with188.694s unused, no further source/probe. Exact owned fixture root empty and processes retired. Complete continuation retains all original native/writer/UI gates; parent owns review, canonicalization, Git and any docs-only integration.
