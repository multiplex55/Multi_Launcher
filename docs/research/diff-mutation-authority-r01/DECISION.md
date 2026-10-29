# AUD-F002 M2-A — retain best-effort mutation authority

**Selected outcome: H0 — retain current metadata/path authority under concurrent external writers, with explicit limits.** No object-bound native layer, native experiment or production change is authorized or executed by this decision. A separate, narrowly scoped failure-information improvement is [proposed only](proposals/diff-failure-information/GOAL.md); it does not strengthen or certify native identity/recycle guarantees.

Author decision date: 2026-10-29. Source/base `59156d96adcc73939c7171487a0cb582e3286587`; published M1-A evidence `ab5088f94f4f0d25783b05f4a2cdc3148d453270`. Branch `codex/research-diff-mutation-authority-r01`. **Independent M2 review and the parent's formal static artifact check remain pending at author handoff.** This is the author's bounded decision, not a claim those gates passed.

## 1. Contract and actual enforcing owner

For this research outcome, retain the current contract: the GUI confirms a captured selection, direction, paths and overwrite observations; Diff applies its existing admission/freshness/containment/dirty-state checks and performs path-based operations. These checks reject many stale or conflicting plans. They do not promise immutable source bytes, exact approved native-object mutation, atomic subtree operations, effect-time exclusive creation, rollback, or interruption of an in-flight native call. No stronger product requirement was established by the current request or inspected implementation. H0 is an engineering disposition to preserve behavior, not inferred user consent to an unspecified new data-loss or recycling policy.

| Operation | Retained authority and owner | Explicit limit |
|---|---|---|
| Copy to expected-absent destination | `plan_copy`/`add_file` capture `None`; `fresh` checks destination observation before `execute_copy` calls `fs::copy` | A readable file arriving before validation is refused; a later arrival is not excluded by the overwrite-capable effect. Execution also currently collapses destination read errors to `None`. |
| Approved overwrite | Same plan captures `Some(expected)` and overwrite intent; preview identifies replacement; per-file `fresh` compares observations | Windows same-compared-metadata substitutions can evade observation; postvalidation substitution remains outside retained authority. Holding a source alone would not authorize a replacement destination. |
| Directory/recursive selection | Plan normalizes selected paths; copy walks a snapshot; directory loop checks containment/path then `create_dir_all`; individual files later use `fresh` | Directory creation does not perform every per-file guard or undo prior effects. Recycle of a directory does not capture/revalidate every descendant. |
| Recycle | GUI plan and spawn boundary admit Recycle; shared executor validates selected items; SystemTrash invokes resolved trash 5.2.4 | Shared explicit Permanent executor still exists. Application no-fallback is established; universal native recycling and approved-object Shell targeting are not. |
| Cancellation/partial completion | Atomic flag checked between items; executor builds partial reports; current-generation GUI completion clears active operation and rescans | No rollback/join/in-flight API interruption. GUI presently does not display `report.items` or `report.cancelled`. Generation filtering does not make effects stop. |

Detailed source locators and dated primary references are in [CONTRACT_AND_EVIDENCE](CONTRACT_AND_EVIDENCE.md), [SOURCE_AND_TEST_INVENTORY](SOURCE_AND_TEST_INVENTORY.md), and [API_REFERENCES](API_REFERENCES.md). These remain the evidence owners; the decision does not redefine their observed behavior.

## 2. Why H0, and why the competing options are not selected

The hand-reviewed [W1/W2 witnesses](IDENTITY_PROOF.md) show two distinct limitations. Windows Diff comparison lacks native object identity, while Unix adds device/inode. Separately, a successful check followed by a path effect leaves a use interval even when the observation identifies an object correctly. The witnesses also preserve real refusal cases; they are not native reproductions or incidence/severity measurements.

| Option | Assessment | Disposition and compatibility reason |
|---|---|---|
| **H0: best effort** | Accurately describes the existing independent Diff owner without assuming an unrequested object/content contract | **Selected.** Preserve ordinary copy/overwrite, selection normalization, existing refusals, GUI Recycle-only admission, no application permanent fallback and between-item cancellation. Explicitly retain known limits; do not relabel incomplete evidence as a guarantee. |
| **H1: stronger local authority** | An additional metadata/native-ID check followed by the same path operation does not close W2. A real design must choose separate source, parent, absent destination and approved-overwrite semantics, plus recycle behavior | **Not selected.** No stronger required contract was established, and a complete effect-time design is not proven. Stronger sharing, exclusive creation or publication could reject previously admitted operations or change destination identity/metadata/partial outcomes. Adding it now would choose compatibility policy without necessity. |
| **H2: narrow native extraction/reuse** | Rename has useful native IDs and held source/directory guards, but its effect is same-parent no-replace rename with an absolute target and null RootDirectory | **Not selected.** It is not a completed copy/approved-overwrite/recycle boundary. Importing Rename's journal/undo/content policy would add unrelated ownership and still leave destination/Shell obligations. No concrete need for an extraction is established by H0. |
| Universal mutation framework | Broader migration and native proof burden with no bounded required consumer set here | **Rejected as out of scope and unjustified.** Neither code size nor the existence of a race model creates a product requirement. |

This outcome does not conclude that wrong-object effects are impossible, that current behavior has been accepted by every user, or that categorical Recycle Bin preview wording proves the backend's behavior. It concludes that this bounded spike has no sufficient requirement/evidence basis for imposing a new native authority contract. Its documented residual limits are part of the result.

## 3. Separate disposition of source-proven failure-information gaps

Two observations do not require a native interleave to establish their mechanism:

1. `fresh` maps **every** destination `FileIdentity::read` error through `.ok()` into `None`, whereas planning only admits absence on `NotFound`. Thus a read failure can be indistinguishable from absence at execution admission. This is source-proven information loss. Whether a particular Windows error then permits a damaging copy is unproven and is not needed to justify preserving the error distinction.
2. `poll_folder_runtime` consumes a matching `Completed` report and rescans without presenting its failed-item/cancelled fields. The executor has that information; the GUI completion path omits it. This is source-proven outcome-information loss, not an assertion that a particular user experienced an undisplayed failure.

These warrant one **PROPOSED_NOT_AUTHORIZED** follow-on, [Diff failure information — Goal](proposals/diff-failure-information/GOAL.md) and [Kickstart](proposals/diff-failure-information/KICKSTART.md). The two acceptance boundaries are distinct: classify actual absence versus read failure at Diff admission, and present current-generation partial/failure/cancellation results at the existing controller. They form a narrow error-information flow; neither is a substitute for native object binding.

The proposal has no new catalog package ID, no automatic queue promotion, and no inherited execution authority from the user-authorized catalog. It must be explicitly selected before production work. It does not request a native experiment, rewrite trash, alter Recycle/Permanent policy, create a concurrency framework, add a journal or change stored settings. The author has not started it.

The proposal also does not claim to solve worker panic/channel-disconnection reporting, generic external-writer semantics, or the truth of every native Recycle Bin outcome. Those remain distinct limits, without manufactured implementation packages in this decision.

## 4. Confidence and evidence classification

| Conclusion | Confidence / evidence |
|---|---|
| Current authority owner and comparison/path-effect distinction | **High, source-inspected.** Exact relevant source is identified by normalized content hashes and locators. |
| Both abstract substitutions and meaningful refusal counterexamples | **High under the stated table assumptions.** Hand-reviewed logical witnesses; executable model NOT_NEEDED. Unix fields are not erased. |
| Destination-error collapse and GUI result omission | **High, source-inspected.** Concrete control/data flow; no runtime reproduction claimed. |
| Native API documented semantics and resolved backend calls | **High for the cited contracts and inspected source.** API/OS/filesystem applicability remains as qualified in API_REFERENCES. |
| Actual swap feasibility, observed wrong-object effect, Shell recycle/partial behavior on the user's machine | **Unproven.** Native NOT_RUN; no likelihood or harm estimate. |
| H0 as the appropriate current engineering disposition | **Moderate.** Strong source basis and lower compatibility cost; bounded by the absence of a stated stronger product requirement and native incidence evidence. It is not irreversible product policy. |

Nine existing test bodies were read, not run. Their names and assertions cannot substitute for competing-writer or native recycling evidence. Production files, tests, manifests, dependency source and settings remain unchanged.

## 5. Reconsideration and proof threshold

Reconsider H0 only if a concrete requirement demands rejection of object substitution/effect-time arrival, or a relevant incident/fixture establishes an unmet guarantee worth changing. First state which property is required: source object, source bytes, existing destination object, absent destination, guarded ancestry, recyclable target, or reported outcome. Do not combine them into one undefined “safe” operation.

Then evaluate the exact responsible Diff/native boundary and its compatibility effects. If a load-bearing native claim cannot be resolved from source/docs, the existing [single future protocol](IDENTITY_PROOF.md#5-one-conditional-later-native-protocol--not_run) defines exactly two synthetic interleavings, identity/outcome evidence and owned cleanup/refusal. It remains **NOT_RUN** and is not required to close H0. No additional model or native campaign is justified merely to make this decision appear more conclusive.

M2 author work is complete. At handoff, the parent owned independent review, any bounded correction, the shared formal artifact check, progress/README and final commit/push receipts. [LIMITATIONS](LIMITATIONS.md) and [VERIFICATION](VERIFICATION.md) preserve the unresolved guarantees and actual execution budget.

## Parent closure — 2026-10-29

Separate review supports H0. Its single required proposal correction is resolved in the [review receipt](REVIEW.md): view plus ordered-root identity gates feedback before completion/rescan; side/root changes invalidate stale results without clearing accepted feedback on ordinary rescan. This changes the unexecuted proposal, not current production behavior or H0. The initial static artifact check passed 190 assertions in 0.650496s, exit 0; no recheck/model/native/application run. M2 research acceptance is complete. Actual closing publication is recorded by the subsequent queue receipt; no self-referential SHA is invented.
