# Proposed goal — preserve Diff mutation failure information

Status: **PROPOSED_NOT_AUTHORIZED**. Authored 2026-10-29 by `/root/contract_author` as a bounded output of AUD-F002 M2-A. **This proposal is not an accepted catalog package or an instruction to execute.** It has no assigned catalog ID. User authorization for the existing catalog does not authorize it; require separate explicit selection. Companion: [KICKSTART](KICKSTART.md).

## Objective and reason

Preserve ordinary failure information at two existing Diff boundaries: distinguish an actually absent copy destination from a failed destination identity read, and show the user a delivered current-generation operation report's failed/cancelled/partial outcome before rescanning hides the operation state.

The current source proves both information losses without requiring a native race: `src/diff/file_ops.rs::fresh` uses `FileIdentity::read(&dst).ok()`, and `src/gui/diff_dialog/mod.rs::poll_folder_runtime` ignores `report.items`/`report.cancelled`. The observed base is `59156d96adcc73939c7171487a0cb582e3286587`; M1 evidence is published at `ab5088f94f4f0d25783b05f4a2cdc3148d453270`. Revalidate then-current source before implementation; do not import an obsolete patch or reset a newer master to this base. [Evidence](../../CONTRACT_AND_EVIDENCE.md) and [decision](../../DECISION.md) explain the distinct native limits.

Success means an unreadable destination cannot be admitted merely because its read failed, and a delivered failed/cancelled report remains visible in the correct folder view with its actual partial results. It does **not** mean object-bound mutation, guaranteed recycling, atomic copy or a stronger concurrent-writer policy.

## Ownership and concrete scope

| Owner | Required responsibility | Intended files/seams |
|---|---|---|
| Diff mutation domain | Destination observation classification and per-item failure result; preserve existing plan/effect flow | `src/diff/file_ops.rs::{fresh, FileIdentity::read, execute_copy, ExecutionReport, ItemOutcome}` |
| Diff controller/runtime presentation | Receive matching-generation completed report before rescan; retain only transient view-associated outcome; render bounded feedback using existing GUI patterns | `src/gui/diff_dialog/mod.rs::{poll_folder_runtime, refresh_after_mutation}`, its current call sites, and `src/diff/folder_runtime.rs` only if it is the established transient state owner |
| Focused tests | Admission and GUI receipt behavior plus unchanged direct invariants | `tests/domain_cases/diff_file_ops.rs`, private tests beside a necessary narrow destination-read classifier/injection seam, and existing inline Diff controller tests |

Do not put filesystem classification in UI code. Do not create a second operation executor or parallel result channel. Keep `CopyPlan`/`DeletePlan` as their current nonserialized execution plans; no persistent schema, session/default/setting or wire-format change. The GUI's transient result must not be serialized into saved comparisons. Ordinary private helper naming and ownership layout belong to the implementation agent after current-source revalidation.

## One bounded implementation milestone

**Objective:** preserve destination-read errors through execution admission and preserve delivered operation errors/cancellation through GUI completion, with focused deterministic tests.

**Dependencies:** separate explicit authorization; verified then-current master/branch ownership; revalidated relevant functions and tests. AUD-F002 research is evidence, not a code dependency. No dependency on AUD-F001, SEARCH-F001/002, Rename transactions or a future native proof.

**Required changes, in order:**

1. Inspect the current four owner seams above and existing test bodies. If the behavior has already been corrected, record a source-backed no-op for that boundary rather than duplicating it. Identify the current matching-generation completion caller and transient view-state owner before editing.
2. In `fresh`, classify destination identity reads explicitly: success is `Some(identity)`; only `io::ErrorKind::NotFound` is `None`; every other error produces an informative per-item validation failure before `fs::copy`. Preserve current expected-None/Some comparison, source/root/containment/dirty checks and the original error context. A small private classifier or injection seam is permitted for deterministic error cases; no general filesystem abstraction or production dependency.
3. Before accepting a delivered `Completed` report, verify both its operation generation and its owning comparison identity: the view ID and ordered left/right roots captured for that operation must still describe the receiving comparison. Perform this check before consuming the report into feedback or initiating its completion rescan; the current controller checks completion before its later root-reconciliation block. A retained view ID or matching scan generation alone is insufficient after Swap Sides or a retained-parent change. Transfer an accepted report to the existing Diff controller before clearing its active operation/preparing rescan, retaining minimal nonpersistent outcome information. Derive failure entries from `ItemOutcome::Failed`, cancellation from the report flag and explicit cancelled entries without double counting. Preserve successful entries that precede failure/cancellation. Do not fabricate results for unattempted items or treat `affected_subtree` as a complete mutation receipt.
4. Use existing bounded message/detail UI patterns to show failed relative paths and their messages, plus cancellation/partial-completion status. A fully successful delivered report must not be labeled an error; do not relabel delete's reused `Copied` outcome as a successful file copy. Keep accepted feedback through automatic rescan of the same ordered root pair, including the rescan's generation increment, and through preview open/cancel or navigation back to the unchanged retained comparison. Clear/replace it when a new operation is actually dispatched, its owning view is discarded, or that comparison's ordered roots/sides change—even when the view ID is retained or its parent changes while a child is active. Do not reinterpret old feedback under new roots, overwrite unrelated workspace errors or leak another operation/view's report.
5. Keep normal preview dispatch/worker channels and rescan/selection behavior. If the controller signature changes, migrate all actual call sites and directly affected inline tests together; remove temporary duplicate consumption/formatting paths before completion. No new confirmation flow, delay, polling subsystem or native callback ownership.
6. Add focused deterministic tests below, run their targeted Nextest selection, inspect the cumulative diff/stale call sites, and obtain one scoped review of admission and GUI result ownership. Commit/push only under the then-active repository workflow and Git owner.

**Invariants:** preserve exact preview-plan reuse, left/right direction, explicit overwrite observation, nested-selection normalization, source/root/containment/dirty-descendant guards, GUI Recycle-only mode, no application Permanent fallback, between-item cancellation, generation filtering, current rescan and retained selection/viewport behavior. No native API or sharing policy changes. A destination error newly refused is the intended behavior change; postvalidation arrival can still escape the existing path-copy check.

## Distinct acceptance and focused future tests

| Boundary | Deterministic case | Acceptance |
|---|---|---|
| Admission | Expected absent destination, injected NotFound | Existing absent-destination plan can reach the current copy effect when other guards pass |
| Admission | Expected absent destination, injected PermissionDenied and a non-NotFound identity error such as InvalidInput | Per-item Failed retains error context; effect is not invoked; no silent conversion to absence |
| Admission | Expected existing destination, missing/read-failed/changed observation | Refuse rather than overwriting under stale authority; matching ordinary overwrite still works |
| GUI | Matching-generation delivered report containing successful entries followed by Failed | Feedback retains actual relative-path/error information through rescan and identifies partial completion; successful effects are not called rolled back |
| GUI | Matching-generation cancelled report, with and without earlier successes/explicit Cancelled item | Cancellation visible, no double counting, no invented unattempted results or rollback claim |
| GUI | Fully successful report; report for wrong generation/view | Success is not an error; stale/wrong-view outcome is not displayed or substituted for the current operation |
| GUI lifetime | Automatic rescan with generation increment; preview open/cancel; unchanged retained-view navigation; next dispatch; view discard; same-ID root/side swap, including a retained parent | Accepted feedback survives the unchanged-comparison cases; new dispatch, discard or ordered-root/side change clears/replaces it without relabeling old outcomes |
| GUI identity | An old completion remains queued while its retained folder parent is swapped from an open child; return to the parent before runtime root reconciliation | Reject the report for the changed ordered roots before presenting feedback or initiating completion rescan, even if its generation still matches the old runtime |

Use a narrow injected metadata-read result for deterministic admission errors; do not depend on real Windows ACL manipulation, sleeps, external writers or filesystem timing. Use injected `ExecutionReport` values for GUI-controller behavior; do not launch an app or trash real files. Exact helper/test names are chosen after revalidation, not invented here.

Run targeted Nextest for the new admission/controller tests and the directly affected existing Diff tests, including ordinary overwrite/copy, recursive selection, escape/dirty refusal, recycle no-fallback, Permanent spawn refusal, captured preview direction and controller rescan retention. Confirm nonzero expected test selection. Targeted Nextest supplies compilation for those targets; add another compile/lint command only if current repository rules or a concrete uncovered target require it. No full suite, native/manual/pixel campaign, benchmarks or unrelated plugin tests. Rerun only affected failures after corrections. Any later application fixture must set Paste `next_hotkey=None` and `cancel_hotkey=None` before construction without changing production defaults.

**Done criteria:** both admission and GUI acceptance boundaries hold; no normal caller bypasses the corrected classification/result-consumption path; required focused tests pass; one scoped review has no unresolved correctness issue; only intended source/test changes remain; exact commit/push and test receipts are recorded by the assigned Git owner. This proposal itself supplies no passing result or native acceptance.

## Non-goals, uncertainty and stopping conditions

No native object/content binding, absent-target exclusive publication, scratch files, ancestor-lock framework, hashing policy, journals/undo, atomic batches, trash flag/backend replacement, Permanent policy change or persisted schema change. Do not repair worker panic/disconnection retirement or general runtime supervision as an adjacent improvement. This goal handles reports that are actually delivered through the existing completion channel.

Current GUI state layout may require a small controller/runtime field or return-type migration; choose the current established owner after inspection. Native interleaving feasibility and universal recycling remain unproven and are not dependencies of these ordinary failure-information fixes. If completion delivery itself becomes the blocking defect rather than delivered-report consumption, stop and report the concrete scope mismatch; do not silently absorb a worker-lifecycle project.

No implementation, fixture, model, test run, branch or dependency installation is performed by generating this proposal. Reversal before separate authorization is documentation-only.
