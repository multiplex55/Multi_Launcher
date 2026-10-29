# AUD-F002 M2-A independent review

Authored by `/root/research_reviewer` on 2026-10-29; persisted by `/root`.

## Disposition

**One bounded documentation correction required.** H0 and the source/abstract/native evidence distinctions are supported. The new failure-information proposal needs one comparison-lifetime amendment; no production change or additional investigation is requested.

Reviewed state is M1-A `ab5088f94f4f0d25783b05f4a2cdc3148d453270` plus the uncommitted M2 artifacts on `codex/research-diff-mutation-authority-r01`. Production source remains `59156d96adcc73939c7171487a0cb582e3286587`.

## Finding AUD-F002-REVIEW-01 — medium

**Bind feedback to the ordered comparison roots, not only a retained view ID and scan generation.**

Affected proposal: `proposals/diff-failure-information/GOAL.md`, required changes 3–4 and GUI lifetime acceptance; companion Kickstart step 3.

The proposal currently permits clearing feedback only on another dispatch or discarded view. Current `DiffDialogState::swap_sides` retains the view ID while changing its semantic endpoints (`src/gui/diff_dialog/mod.rs:165–184`). `DiffWorkspace::swap_sides` swaps folder roots and can also swap a retained folder parent while its child is active, without replacing that parent's ID (`src/diff/model.rs:353–391`).

There is an additional ordering concern: `poll_folder_runtime` consumes a generation-matching completion at `mod.rs:1255–1267` before reconciling changed roots at `1307–1320`. A queued parent report can therefore still match the old runtime generation when the retained comparison's ordered roots have changed.

An implementation following only the current lifetime rule could display old relative-path/direction feedback in a swapped comparison, or accept an old report before root reconciliation. Conversely, clearing feedback on every generation increment would violate the intended retention across automatic rescan.

This is a proposal-readiness defect, not a newly implemented regression. Amend the existing pair only. Comparison identity here means the existing logical ordered roots and owning view; it does not introduce native file-ID authority.

### Exact bounded amendments

Replace required change 3 in the proposed Goal with:

> 3. Before accepting a delivered `Completed` report, verify both its operation generation and its owning comparison identity: the view ID and ordered left/right roots captured for that operation must still describe the receiving comparison. Perform this check before consuming the report into feedback or initiating its completion rescan; the current controller checks completion before its later root-reconciliation block. A retained view ID or matching scan generation alone is insufficient after Swap Sides or a retained-parent change. Transfer an accepted report to the existing Diff controller before clearing its active operation/preparing rescan, retaining minimal nonpersistent outcome information. Derive failure entries from `ItemOutcome::Failed`, cancellation from the report flag and explicit cancelled entries without double counting. Preserve successful entries that precede failure/cancellation. Do not fabricate results for unattempted items or treat `affected_subtree` as a complete mutation receipt.

Replace required change 4 with:

> 4. Use existing bounded message/detail UI patterns to show failed relative paths and their messages, plus cancellation/partial-completion status. A fully successful delivered report must not be labeled an error; do not relabel delete's reused `Copied` outcome as a successful file copy. Keep accepted feedback through automatic rescan of the same ordered root pair, including the rescan's generation increment, and through preview open/cancel or navigation back to the unchanged retained comparison. Clear/replace it when a new operation is actually dispatched, its owning view is discarded, or that comparison's ordered roots/sides change—even when the view ID is retained or its parent changes while a child is active. Do not reinterpret old feedback under new roots, overwrite unrelated workspace errors or leak another operation/view's report.

Replace the GUI lifetime acceptance row and add the adjacent identity row:

| Boundary | Deterministic case | Acceptance |
|---|---|---|
| GUI lifetime | Automatic rescan with generation increment; preview open/cancel; unchanged retained-view navigation; next dispatch; view discard; same-ID root/side swap, including a retained parent | Accepted feedback survives the unchanged-comparison cases; new dispatch, discard or ordered-root/side change clears/replaces it without relabeling old outcomes |
| GUI identity | An old completion remains queued while its retained folder parent is swapped from an open child; return to the parent before runtime root reconciliation | Reject the report for the changed ordered roots before presenting feedback or initiating completion rescan, even if its generation still matches the old runtime |

Replace Kickstart step 3 with:

> 3. Before consuming a delivered report, verify its operation generation and owning view plus ordered left/right roots captured at dispatch. Do this before completion feedback/rescan; a same-ID Swap Sides or retained-parent change can precede the controller's root reconciliation. Retain accepted nonpersistent feedback through automatic rescan/generation advancement, preview open/cancel and unchanged retained-view navigation. Clear/replace it on actual new dispatch, view discard or ordered-root/side change, including a parent changed while its child is active. Show failures/cancellation/partial completion using bounded existing GUI patterns, without stale-comparison leakage, invented remaining-item results, double-counted cancellation, delete-as-copy labels, rollback claims or replacement of unrelated errors. Follow the Goal's focused lifetime and queued-report acceptance cases.

These amendments stay within the proposed controller/report ownership work. They require no worker-supervision expansion, native experiment, new proposal or production execution during this research.

## Supported conclusions

- `fresh` actually collapses all destination identity-read errors through `.ok()`, unlike planning's explicit NotFound admission. The proposed success/NotFound/other-error classification preserves that distinction at the correct owner. Its deterministic refusal tests are appropriate.
- The current GUI consumes delivered completion without retaining item failures or cancellation. The proposed information flow preserves partial successes, avoids inventing unattempted results and accounts for delete's reused `Copied` variant.
- H0 does not claim exact-object or immutable-byte mutation, effect-time exclusive creation, atomic subtree operations, rollback or interruption of an entered filesystem call.
- W1 correctly distinguishes Windows compared metadata from Unix device/inode checks. W2 is a separate validation/use witness. Admitted-plan and changed-metadata/dirty/absence counterguards remain present.
- The inspected trash backend uses the documented path-created Shell item, flags, perform and abort-query sequence. Application no-permanent-fallback is not presented as universal recycling or approved-object targeting.
- The inspected Rename primitive uses a held source, absolute destination, null RootDirectory and no replacement. It is not claimed to supply Diff copy/overwrite/recycle authority.
- The generated Goal/Kickstart remains PROPOSED_NOT_AUTHORIZED, with no catalog ID or inherited execution authority. Its ownership, migration, dependencies and focused tests otherwise fit the bounded findings.

## Limits and correction disposition

This review is static. It inspected selected current sources and the saved API evidence, without repeating the web survey, complete native investigation or formal artifact checker. Existing test bodies remain source evidence, not passing runs. Optional model remains NOT_NEEDED with zero execution; native/application execution remains NOT_RUN.

At handback, AUD-F002-REVIEW-01 is pending the parent's single documentation correction. After applying the exact amendments and recording resolution, the parent can perform the reserved initial static artifact pass, reconcile final progress and publish closure. No second review campaign or behavior experiment is requested.

The correction does not change H0 or authorize the proposed implementation.
