# Goal AUD-F002 — Decide Diff folder mutation authority and focused identity proof

## 0. Identity, provenance and readiness

- Package: **AUD-F002**, linked finding **AUD-002**, decision **D02** in Goal 01 Static Architecture Audit.
- Type: **bounded research spike**. State: **PROPOSED_NOT_AUTHORIZED**. Readiness: **NEEDS_RESEARCH_SPIKE**. Creating this contract does not execute it.
- Canonical directory: `docs/research/multi-launcher-research-20261028-r01/01-static-audit/follow-on/AUD-F002-diff-mutation-authority/`; promoted from the preserved implementer-role proposal in M3-A.
- Priority: second accepted audit package, after the smaller ready AUD-F001; independent dependency-wise. User value: clarify whether a confirmed Diff copy/recycle remains authorized when another process replaces a selected file object.
- Severity: **conditional medium consequence**, not reproduced native data loss. Confidence: high for the inspected metadata/path distinction; native feasibility/likelihood and intended concurrent-writer contract unverified.
- Complexity: moderate reasoning/contract work, low code execution burden. Regression risk: none to production within this spike; any later repair may have meaningful Windows/overwrite/trash compatibility risk. Validation burden: bounded source/abstract proof plus explicit native proof obligations, no application campaign.
- Historical evidence SHA: `59156d96adcc73939c7171487a0cb582e3286587`; revalidate actual current owners, do not reset future work to this SHA.
- Original author: `/root/audit_author`, research implementer/author, 2026-10-28. Requested/accepted dispatch `worker`, `gpt-6.1-sol`, `xhigh`; actual backend identity/effort unverified. Planner supplied M2-A/M2-B framework; parent supplied reserved package ID and sequential publication. Separate research review: completed; the parent records final disposition in [review and limits](../../REVIEW_AND_LIMITATIONS.md). No production implementation or native proof is accepted.
- Evidence: [AUD-002](../../findings/AUD-002-diff-mutation-identity.md), [D02 and alternatives](../../DECISION_RECORDS.md#d02--decide-diff-concurrent-writer-authority-before-choosing-a-repair), [file-safety trace](../../PERSISTENCE_AND_FILE_SAFETY.md#diff-folder-mutations-have-a-different-guarantee), [change cost](../../BOUNDARIES_AND_CHANGE_COST.md). Competing raw package: none accepted; planner did not prescribe a Diff repair. Search handoff is a later Goal 01 artifact, not a dependency of this spike.
- Companion directive: [KICKSTART.md](KICKSTART.md).
- Canonical promotion: `/root/audit_author`, Goal01/M3-A, 2026-10-28; preserved raw author [Goal](../../roles/implementer/follow-on/AUD-F002-diff-mutation-authority/GOAL.md)/[Kickstart](../../roles/implementer/follow-on/AUD-F002-diff-mutation-authority/KICKSTART.md). Changes are directory/provenance links and canonical status only; 100-state/30s-run/60s-aggregate limits and PROPOSED_NOT_AUTHORIZED state remain intact. No native proof, production repair or independent review is claimed by promotion.

## 1. Objective and outcome

Produce a source-grounded decision about the **required authority at Diff folder copy/overwrite/recycle mutation time**. Compare two contracts: best-effort metadata/path freshness under external writers, and binding approval to a specific source/destination object or an explicit no-replace/overwrite policy. Deliver a proof/acceptance specification at actual owners; recommend no change/documentation or a narrowly scoped later correction with clear prerequisites. Do not implement that correction during this spike.

Example: a user previews `left/a.txt → right/a.txt`. Another process replaces the source or destination with a different object while preserving kind/length/modification time, or swaps the pathname after freshness validation. Current Windows `FileIdentity` cannot distinguish same-metadata objects, and mutation uses paths after validation. Decide whether the accepted contract tolerates that interference, whether it must reject, and which boundary can actually enforce the chosen rule.

The proven fact is a source authority distinction. The opportunity is **speculative until the contract/proof decision**; no runtime occurrence, exploitability, percentage improvement or general filesystem insecurity is asserted. A successful result can be “retain best-effort behavior with explicit limitation; do not implement a new native layer.”

## 2. Current architecture and evidence

| Owner / path / symbol | Current responsibility and counterguards | Spike task / potential later boundary | Callers/contracts |
|---|---|---|---|
| `src/diff/file_ops.rs::{FileIdentity, CapturedRoot, CopyPlan, DeletePlan}` | Captured path/root, selected source/destination metadata, overwrite intent and generation. Windows comparison has kind/length/modified; Unix additionally device/inode. | Define what preview authorizes; distinguish metadata equality, object identity and destination absence/overwrite consent. | `plan_copy`, `plan_delete`, confirmation preview. [S1] |
| `fresh`, `same_root_identity`, `execute_copy` | Recheck containment/path meaning/root, expected source/destination metadata and dirty target/descendant, then path-based `fs::copy`. | Map validation-to-use interleave and determine smallest owner that could bind source and destination consistently. | GUI consumes its captured confirmed plan and current dirty paths. [S2], [S4] |
| `execute_delete`, `spawn_recycle_delete`, `TrashBackend` | Validate selected target, dirty path and metadata; GUI spawn refuses Permanent; trash failure does not choose permanent fallback. | Decide what recycle API can prove without inventing identity or atomicity guarantees. Preserve recycle-only behavior. | GUI plans only Recycle, uses SystemTrash. [S3], [S4] |
| `src/gui/diff_dialog/mod.rs::show_operation_preview` and folder runtime | Capture dirty paths, consume exact preview, retain operation handle/generation and prevent overlapping mutation through UI state. | Revalidate caller/confirmation/close/cancel semantics; UI is not a new filesystem policy owner. | CopyLeft/Right and DeleteLeft/Right; model reload/result acceptance. [S4] |
| `src/file_operations.rs::{GuardedRenameFile, prepare_guarded_rename_batch_native, rename_open_file_no_replace}` | Existing native source/parent proof/held handles/no-replace operation, tailored to Rename/Replace semantics. | Assess reusable primitive only; do not import Rename's same-parent/journal/undo assumptions into Diff automatically. | Batch Rename/Replace transaction owners, different policies. [S5] |
| `tests/domain_cases/diff_file_ops.rs` via `tests/suites/domain.rs` | Ordinary copy/overwrite, normalization/escape/dirty/refusal and recycle no-fallback source assertions. | Inventory coverage and specify missing deterministic interleave cases; run none in this spike. | Cargo grouped `domain` target; discover current names before any later authorized implementation run. [S6], [S7] |

Existing Rename/Replace machinery is counterevidence against “missing filesystem architecture.” The package concerns **Diff's separate policy and enforcement**, not historical Goal 09/12 remediation reports. It neither reopens those plans nor treats their old failures as present on the current base.

## 3. Scope, non-goals and dependencies

Allowed writes are only the new task's documentation and, optionally, one pure in-memory proof model in its task documentation directory. Current source/tests/manifests/native wrappers are read-only. No application build, Cargo/nextest/rustc/Clippy, native GUI/input test, native file mutation, application benchmark, dependency installation, personal/global configuration, live user-data access or whole-tree scan experiment.

Allowed source follow-ups: the named Diff owner/callers/tests, their actual containment/trash/platform wrappers, and selected native identity primitives only where needed to decide a concrete alternative. Stop after the boundary and proof obligation are known. Do not audit every subsystem that imports file operations.

Independent base: **then-current verified actual master**, with the user's real new commits included; no dependency on AUD-F001, Goal 02, historical research branch or an unaccepted remediation patch. If a future accepted dependency is explicitly selected, record its accepted commit/capability and ancestry; branch name alone is insufficient. If ownership has changed, update mapping and evaluate the current delta rather than reproducing obsolete code.

Non-goals: repair implementation; universal filesystem framework; replacing trash with permanent deletion; introducing new journals/undo for all Diff operations; algorithm/performance optimization; target-repair/settings/persistence refactor; broad native acceptance or test-fixture rewrite; executing any other follow-on package.

## 4. Competing hypotheses, evidence and decision design

### 4.1 Contract and owner map

Enumerate copy to absent destination, user-approved overwrite, directory creation/recursive selection, recycle and cancellation/partial completion. For each, record exact preview authority, validation owner, path/object used at mutation, postcondition and outcome reporting. Distinguish root, ancestor, source, destination and scratch ownership; copy without scratch is not renamed into an atomic transaction by prose.

Candidate contracts:

- **H0 — best-effort external-writer semantics:** retain current metadata/path checks. State exactly which replacements are tolerated, what user confirmation means and what the product can report after partial failure. A documented limitation can be sufficient if no stronger requirement exists.
- **H1 — local stronger admission:** keep Diff plans/policy; bind source/destination identity and expected absent/overwrite state at the first responsible mutation boundary using appropriate existing primitives. Assess whether this enforces the selected contract through use, rather than adding one more stat before a path API.
- **H2 — narrow native primitive reuse/extraction:** only if H1 needs a shared enforceable primitive, define inputs/outputs/handle lifetime/errors and migrate the relevant callers in a later goal. Do not move whole transactions or claim source handles alone pin a destination.

Reject a general framework unless the bounded inspection demonstrates a necessary shared behavior and names every required caller migration; the default result remains H0/H1/H2.

### 4.2 Deterministic authority proof

Write a small transition table demonstrating exactly what the current comparison can and cannot distinguish. State `Approved(path, object A, metadata m)`, `Validate(path, m)`, `ExternalReplace(path, object B, metadata m)`, `Mutate(path)` and `MutateHeld(A)` separately. Include same-metadata replacement **before** validation and pathname replacement **after** validation. The result is an abstract logic witness, not an executed Windows race.

Optionally implement that table as `proof/identity_model.py`: Python standard library only, no subprocess/network/environment/filesystem API and no source imports; no more than 100 enumerated states and two swap scenarios plus guard counterexamples. Assert that metadata equality permits distinct IDs and that path use can differ from the approved object; retain cases where changed metadata/dirty target is refused. At most one run and one localized repair rerun, **30 seconds wall-clock per run and 60 seconds aggregate**, enforced by the runner. Timeout stops that optional run and preserves failure evidence; no native fallback or renewed allowance. If Python is unavailable, the hand-reviewed table is sufficient; mark the model NOT_RUN and continue. Model success cannot prove a Windows API's actual sharing, rename, reparse or trash contract.

### 4.3 Native proof obligations without a native campaign

For each selected H1/H2 primitive, identify its exact current source and consult current official primary documentation for load-bearing API semantics: handle identity, sharing/delete rights, parent/destination binding, overwrite/no-replace and trash target semantics. Cite API version/platform/access date. A source comment or fixture report is not an independently reproduced guarantee. If documentation is unavailable, qualify that obligation; do not repeatedly search or block unrelated decision work.

This spike executes **no native mutation**. Where source/docs cannot establish a required guarantee, deliver one minimal disposable-fixture protocol for a later separately authorized proof: synthetic source/destination in one isolated temporary directory, exact two interleaves, expected object IDs/outcomes, no real user tree, retained owned handles, guaranteed release/cleanup, and refusal to delete unexpected identities. State why that observation changes H0/H1/H2. Native lack of proof must be visible in the final decision confidence rather than converted into a pass.

### 4.4 Persistence, UI, concurrency, resources and migration

No schema/default/wire-format change in this spike. Any later correction design must name whether CopyPlan/DeletePlan carry opaque evidence, whether serialization exists, and how old callers migrate. Preserve confirmation meaning, explicit overwrite consent, dirty-document protection, selection/viewport behavior, recycle-only/no-permanent-fallback, cancellation between items and accurate partial outcomes. Do not add delays, synthetic input or new UI confirmation flows merely to mask file authority.

A later native owner must retain exactly the handles/evidence needed until effect/cleanup, define thread/callback restrictions and distinguish cancellation from completion/retirement. Do not introduce whole-file hashing by default; reason about required content-versus-object identity and per-file/operation bounds. No throughput/RSS benefit is claimed or measured by this spike.

Output an ordered **proposal**, if warranted: revalidate and add narrow owner tests; introduce the selected primitive without competing authority; migrate copy/recycle caller(s) and directly affected tests; preserve behavior/errors; search stale paths; remove temporary adapters when no normal caller bypasses the intended boundary. Reversal is documentation-only until a separately authorized production goal; no production rollback is needed for this spike.

## 5. Alternatives, tradeoffs and rejection conditions

H0 has the lowest compatibility cost and may accurately represent the product contract; reject it only when a stronger requirement is established or concrete evidence shows an unmet guarantee. More metadata checks improve ordinary stale detection but do not prove object identity across path replacement; reject them as a complete H1 solution if they leave that gap.

Existing held-file guards can avoid duplicate native code but may require incompatible same-parent/journal semantics or still leave destination/trash authority unresolved. Reject importing them wholesale. A narrow extraction is justified only by a specific reusable primitive with named callers and a stronger enforceable guarantee. A universal native mutation framework has the highest migration/validation cost and is rejected absent concrete necessity.

If current master already closes the gap, record source-backed no-op and remaining test/proof limits. If intended best-effort contract is chosen, publish that limitation and close the recommendation without inventing a repair. If native evidence is insufficient, produce a precise bounded proof obligation and conditional decision; do not pretend a production fix is ready.

## 6. Invariants and acceptance behavior

1. Decision identifies H0/H1/H2, required contract, actual enforcing owner, competing option and rejection reason; uncertainty is explicit.
2. Every safety claim maps to current source or dated primary API documentation, labeled static/model/native as applicable. No native run occurred in this spike.
3. Metadata equality, object identity, content identity, path containment and destination overwrite authority remain separate concepts.
4. Existing confirmation, dirty descendants, containment, recycle-only/no-fallback, cancellation/partial outcomes and source/destination role checks are preserved in all proposed changes.
5. The abstract witness includes pre-validation same-metadata replacement and post-validation swap, plus guard counterexamples; it is not relabeled application proof.
6. No production code/configuration/test manifest or user data is changed. Optional model has no file/native effects and stays within its run/state budget.
7. Any implementation recommendation is a bounded **future proposal**, with exact ownership/migration/tests; it is not executed by this goal. No accepted claim requires the user to rediscover missing API ownership.
8. A no-change/no-op decision is acceptable when supported; a missing required native guarantee is recorded as an incomplete proof, not passing implementation acceptance.

## 7. Fresh branch and checkpoints

For a new authorized run, inspect Git state/operations/worktree ownership and applicable instructions; resolve/fetch actual current master without overwriting dirty work. Use one fresh branch stem **`codex/research-diff-mutation-authority-r01`**, increment only the run suffix if collision. Use corresponding task docs root **`docs/research/diff-mutation-authority-r01/`**, with the same run suffix. Record exact master/base/HEAD and source status. A confirmed interrupted instance resumes its own branch/docs after identity/ancestry checks; no reset/rebase/force-push/discard/stash of others.

| Checkpoint | Objective / actual owners | Required outputs and focused acceptance | Commit subject |
|---|---|---|---|
| M0-A | Safe bootstrap; Git and task docs only | `README.md`, `GOAL.md` execution copy/progress, branch/base and role record; no production edit. Base/ownership/authorization verified. | `docs(diff): [AUD-F002/M0-A] initialize mutation authority spike` |
| M1-A | Contract and proof at Diff/native boundaries named in section 2 | `CONTRACT_AND_EVIDENCE.md`, `IDENTITY_PROOF.md`, optional pure model/report, actual API references and existing-test inventory. Both swaps/counterguards and authority owner mapped; native unknowns labeled. | `docs(diff): [AUD-F002/M1-A] trace mutation authority and proof` |
| M2-A | Choose bounded outcome and review | `DECISION.md`, `REVIEW.md`, `LIMITATIONS.md`, final README/progress; no-change or precise later correction/proof proposal. One critical review/correction pass; links/scope checked. | `docs(diff): [AUD-F002/M2-A] close mutation authority decision` |

**M1-A handoff:** objective is the contract/proof map, owner remains Diff plan/executor plus existing native/trash wrappers. Revalidate current facts, inspect named callers/counterguards, compare H0/H1/H2, construct transition witness and capture primary API obligations. Invariants in section 6; no production/native execution. Depends only on M0-A current baseline. Tests are static inventory; optional pure-model run has its separate bound. Done when another engineer can identify every authority handoff and distinguish known proof from missing native evidence without reopening the architecture survey.

**M2-A handoff:** compare all options against the required contract; select or explicitly condition the decision, specify migration and focused future acceptance if warranted. A separate reviewer challenges destination/trash claims, misleading model proof and scope creep. One substantive correction pass; no new experiment matrix. Done when all concerns are decided/no-action/explicitly unproven with a bounded next proof, required documents agree, and production tree is unchanged.

Commit coherent checkpoints, then attempt one normal push per commit; use configured upstream or verified intended origin. Preserve local commits on network/auth/rejection failure and continue. Do not pull/rebase/force-push to repair publication or auto-merge master.

## 8. Future verification matrix — not executed during Goal 01 research

| Behavior/risk | Owner/case and setup | Expected evidence | Level / bound / cleanup |
|---|---|---|---|
| Current metadata cannot distinguish same-ID fields/different object | Transition table; optional pure in-memory model, synthetic IDs/bytes only | Equal compared metadata, different object identity; explicit authority mismatch, not Windows reproduction | Mandatory static witness; optional model ≤100 states, one run + one repair rerun, 30s/run and 60s aggregate; no external resources or native fallback |
| Check/use race | Place abstract replacement after Validate and before path Mutate | Effect identity may differ while validation was true; held-identity alternative's assumptions enumerated | Same witness; no destructive fixture |
| Existing guard counterevidence | Current `fresh`, `execute_delete`, GUI/TrashBackend and domain test bodies | Changed metadata, dirty descendants/escape and permanent-mode GUI refusal remain refused; no trash failure fallback | Source/test inspection only; no Cargo/native required |
| Destination/overwrite/trash enforcement | Exact current wrapper plus official API reference | Separate absent-destination, approved overwrite and recycle guarantees; unresolved semantics marked unproven | Primary docs/source, bounded to selected option; no API execution |
| Need for native proof | Only if selected contract cannot be decided from source/docs | Minimal later two-interleave synthetic protocol and expected IDs/outcomes, including owned cleanup/refusal | Protocol document, **NOT_RUN** in spike; separately authorized later execution only |
| Proposed later production correction | Exact test locations: `tests/domain_cases/diff_file_ops.rs`, GUI inline preview tests, selected native owner tests; revalidate current target/names | Add deterministic injected interleave/identity refusal; retain ordinary copy/overwrite/dirty/recycle cases and partial outcome behavior | Not implemented/run here. Later goal separately specifies compile/targeted Nextest/first-party lint/native bound; verify nonzero inventory before claiming pass |
| Artifact correctness and scope | Task docs/source references/progress; Git diff/status | Links/IDs/ranges resolve; alternatives/limits agree; no unauthorized source/config change | One cheap static pass plus one localized correction; no test suite |

For any later app fixture, disable Paste `next_hotkey` and `cancel_hotkey` before construction, preserving production defaults. No build/native/Clippy gate is required for this documentation/research spike. Optional model success cannot replace native acceptance, and missing optional tooling does not block the decision report.

## 9. Failure, rollback and bounded continuation

Localized documentation/model corrections remain within one substantive review pass and the model's one repair rerun. Preserve first failed output and exact changed assumption. Do not renew the run allowance or substitute a native experiment when the pure model is inconclusive.

Unavailable optional Python/API documentation/reviewer gets one supported recovery, then truthful source/table/parent-review fallback. Continue independent source/decision work. A genuinely missing required contract may yield a conditional decision and one precise product/native proof question; it must not turn into repeated permission prompts or an open-ended investigation. Never fabricate native proof or accept implementation from a static witness alone.

Changes are docs/optional pure model only; rollback means preserve evidence and correct/revert only this task's own artifacts on its branch. No user-data restoration or production reset is applicable. If Git conflict/index safety is unresolved, preserve state, write stall report and do not commit through it.

## 10. Models and role folders

Recommend planner `gpt-6-astra` max, research author `gpt-6-astra` xhigh, reviewer `gpt-6-astra` max for competing Windows authority/trash semantics; parent Astra xhigh if available. This is a bounded reasoning problem, not permission to enlarge scope. A deliberate supported fallback is Sol xhigh authorship with Astra planner/reviewer, or parent source review explicitly labeled self-review if separate role unavailable.

Inspect actual role configuration/dispatch support once; supported recovery at most once, then record unavailable/actual accepted request/backend unknown. Do not modify global role TOML, permissions or model defaults. One active child role and one writer at a time. Read-only planner/reviewer return content to writer. Use `roles/planner/{ROLE_RECORD,ANALYSIS}.md`, `roles/implementer/{ROLE_RECORD,ANALYSIS}.md`, `roles/reviewer/{ROLE_RECORD,REVIEW}.md` under the new task docs root; record requested/accepted/resolved-or-unverified identity, source, actual role ID, authorship/persistence and review independence.

## 11. Progress and stalls

Execution copy `GOAL.md` under the resolved task docs root has the mutable section 13 below. Update at M0/M1/M2 state transitions with artifact links, exact last commit/push status, current issue and next action. Parent Git owner publishes; no repeated status-only commits.

For genuine blocked/paused/deferred boundary use `stalls/` in that root, filename containing checkpoint and actual UTC timestamp. Record branch/worktree, base/HEAD, diagnostic/expected-versus-actual, preserved docs/model state, no-native/no-user-data scope, attempted recovery and remaining budget, evidence and smallest safe resume instruction. Do not place private paths/payloads/secrets in evidence. A clearly labeled partial research commit is useful and does not imply production acceptance.

## 12. Final delivery and authorization boundary

Deliver README, execution Goal/progress, contract/evidence, identity proof/optional model result, decision, review/provenance and limitations. Report actual base/branch/checkpoint/final commit/push status, selected H0/H1/H2, what remains unproven and whether a later focused proposal is warranted. Distinguish model NOT_RUN/run results, source reasoning and native NOT_RUN. All production code/config/tests remain unchanged.

Do not implement the recommended correction, execute another generated goal, merge master, open/publish unrelated artifacts or run destructive user-data operations. Authorization to execute this spike authorizes only its bounded documentation/pure optional model scope; native/application implementation requires an appropriate separately selected goal.

## 13. Execution progress — mutable

State: **PROPOSED_NOT_AUTHORIZED**. No execution branch/base, tests, model/native run or checkpoint commit exists for this package. This role proposal was authored during Goal 01 research; that is not M0 completion.

| Checkpoint | State | Evidence / next action |
|---|---|---|
| M0-A | PENDING | User selects/authorizes AUD-F002, then revalidate current source/base and bootstrap safely. |
| M1-A | PENDING | Contract/evidence and bounded identity witness; optional pure model only. |
| M2-A | PENDING | Decision, review and limits; no production repair. |

## Pinned research sources

[S1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/diff/file_ops.rs#L30-L64
[S2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/diff/file_ops.rs#L507-L631
[S3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/diff/file_ops.rs#L804-L898
[S4]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/diff_dialog/mod.rs#L1052-L1178
[S5]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/file_operations.rs#L1149-L1268
[S6]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/tests/domain_cases/diff_file_ops.rs#L7-L179
[S7]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/tests/suites/domain.rs#L13-L32
