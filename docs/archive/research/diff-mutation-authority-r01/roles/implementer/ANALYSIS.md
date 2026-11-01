# AUD-F002 author analysis — M1-A evidence and M2-A decision

The planner's packet matched the current named owners. No production migration or broader architecture plan was needed. The evidence now supports a precise distinction between check-time freshness and effect-time authority while preserving useful current guards.

## Findings that determine the boundary

1. **Expected absence and overwrite diverge at approval but converge on the same effect.** `add_file` only plans absence on NotFound, captures existing destination metadata for overwrite, and skips equal source/destination identities. `fresh` rechecks expectations, but its destination `.ok()` collapses errors to absence. Both admitted cases still call overwrite-capable `fs::copy` by path. The tables use admissible unequal source/destination metadata and show prevalidation absence arrival is refused.
2. **Platform and operation distinctions matter.** Windows Diff metadata does not include native object identity; Unix does include device/inode. Prevalidation same-compared-metadata replacement is qualified accordingly. Postvalidation substitution is a separate check/use argument. Directory creation checks containment/path but does not run every per-file freshness/dirty/root guard. Recursive directory recycle does not revalidate every descendant.
3. **GUI lifecycle does not supply result presentation or filesystem exclusion.** The exact preview plan is moved to execution with an updated dirty snapshot. This is not an ongoing editor lock. Matching-generation completion clears the active operation and rescans, without displaying per-item failed/cancelled outcomes. Cancellation is between items, not rollback, in-flight API cancellation or joining the worker.
4. **Resolved backend behavior is narrower than the label.** Trash 5.2.4 resolves the parent, creates a Shell item by name, queues deletion and performs it with NO_UI/ALLOWUNDO/WANTNUKEWARNING, then checks abort after success. No application Permanent fallback is established. Guaranteed recycling or approved-object Shell binding is not established; the swap is placed before Shell parsing rather than guessing later internal behavior.
5. **Native reuse has a real but limited owner.** Rename holds a source with constrained share modes and retained/revalidated directory guards. The final native rename uses an absolute destination with null RootDirectory and no replacement. It is neither a relative-parent-handle implementation nor a completed Diff copy/overwrite/recycle protocol. Any later extraction must preserve rights, chain guards, identity/refusal and RAII lifetime without importing Rename journals/undo/transaction policy.

These are static findings from the pinned source and primary API contracts. They do not establish occurrence, severity by incident, actual Windows interleave feasibility or user harm.

## Alternatives prepared for M2

H0 remains viable if best-effort external-writer behavior is the required contract, with accurate limits for cancellation, reports and recycling. H1 would need a specifically chosen stronger contract and an actual local admission/effect boundary; another stat or native-ID comparison followed by the same pathname API does not suffice. H2 could reuse identification/open/guard primitives narrowly, but destination overwrite/absence and recycle remain unsolved by the inspected rename primitive. No option has been selected here.

Future scratch ownership is described only as an obligation if a later design chooses publication: unique object creation, guarded parent, recorded identity, explicit absent/overwrite semantics, held lifetime and identity-checked cleanup. No scratch behavior is attributed to current code.

## Proof and verification choice

The mandatory W1/W2 tables, explicit platform constraints and meaningful refusal examples answer the logical ambiguity. An executable pure model would restate the same assumptions, so it is **NOT_NEEDED** and has zero runs. Native behavior remains **NOT_RUN**; a single conditional two-interleave synthetic protocol defines identities, expected outcomes, handle lifetimes and refusal of unexpected cleanup identities. It is not a new goal or authorization to run it.

Current primary sources were bounded to actual load-bearing Rust/Win32/Shell/dependency calls. One incorrect API URL was corrected successfully; an unavailable remote crate source had one unsuccessful alternate URL, then the permitted exact local-cache source fallback. No further retrieval loop, installation or unofficial authority was substituted.

The existing-test inventory contains seven domain and two GUI test bodies, all source-inspected only. Production/source tests remain untouched. Source hash receipts identify inspected bytes. The formal single static artifact pass is reserved for M2, as authorized by the parent packet, so final decision/review files can be checked together without duplicating a whole checker.

## M1-A handoff receipt

M1-A evidence was returned before M2 and then committed/normally pushed by the parent as `ab5088f94f4f0d25783b05f4a2cdc3148d453270`. M1 did not select a production repair or start M2. The separate M2 author dispatch and resulting work are recorded below.

## M2-A decision and narrow proposal

Selected **H0**: preserve current metadata/path authority under concurrent external writers and document its limits. No stronger object/content/destination/recycle requirement was established; a native layer would impose new compatibility decisions with incomplete destination/Shell proof. H1 does not become complete through another stat or native-ID read; H2's held-source same-parent no-replace Rename pattern does not provide copy/overwrite/recycle semantics. Decision confidence is moderate; source/mechanism confidence is high; native occurrence/targeting remains unproven.

Two ordinary information losses warrant a distinct proposal without strengthening that concurrency policy. Destination identity read errors are collapsed to absence in `fresh`, unlike planning's NotFound admission. GUI completion discards the delivered report's failed/cancelled information. Both mechanisms are established by source, independent of whether a damaging native race can be reproduced. The parent confirmed keeping these as a narrow failure-information proposal with separate admission and GUI acceptance cases.

The proposed Goal/Kickstart defines one future implementation milestone: preserve non-NotFound read errors before copy, and retain/present delivered current-generation partial/failure/cancellation feedback at the existing controller. It preserves actual guards, captured plans, Recycle/no-fallback policy and between-item cancellation. Tests are deterministic injected destination-read results and reports plus directly affected existing Diff cases, with no native experiment or broad framework. Worker panic/disconnection supervision and universal recycle semantics are explicitly outside it.

The proposal is **PROPOSED_NOT_AUTHORIZED**, has no catalog ID or inherited authorization, and was not executed or dispatched as a new task. It is a complete reviewable proposal, not a production acceptance claim. No API fact needed renewed investigation; no model/runtime/test commands were added. The single formal static pass remains unused for the parent after separate review files are present.

M2 author outputs are complete for independent review: DECISION, LIMITATIONS, proposal Goal/Kickstart, and updated verification/provenance. Parent owns review/correction disposition, formal check, progress/publication and final receipt. No self-referential closing SHA or passing review/check result is claimed.
