# G10 documentation reconciliation requirements

This is the implementation checklist for proposed content. Parent alone applies authoritative global updates and owns their Git lifecycle. Preserve frozen role/test receipts and append-only historical events; make current status clear above them.

## Global proposals the writer must prepare

| Parent-owned destination | Required current content / correction |
|---|---|
| `PROGRAM_STATUS.md` | All eleven current dispositions; distinguish latest integration docs tip, newest test source `fc9e5c7c`, last production `b9f5fa4b`, fetched master and baseline. At writing G10 is in progress, not final accepted. Replace stale next-G10-baseline actions only with the actual next review/import/PR action. |
| `BRANCH_MAP.md` | Current exact branch/ref table from the parent receipt, with G10 accepted-integration ancestry and G03 docs-only bootstrap exception. Move old staged/pending statements under clearly dated historical events, not unqualified current facts. Preserve original branches and donor history. |
| `ACCEPTED_CHECKPOINTS.md` | Keep established provenance, corrected cumulative acceptance and rejected intermediate exclusions. Add G10 only after its actual review/import. Explicitly separate documentation acceptance from source acceptance. |
| `DEPENDENCY_MAP.md` (currently absent) | Materialize the logical gate map from Planner RESIDUAL, identify canonical pairs and current status; distinguish logical edges from historical sequential dispatch. Original queue namespaces must be explicit. |
| `FOLLOW_ON_CATALOG.md` | Normalize existing table/bullets without changing priority or reopening resolved findings. Include pair paths, source role, confidence, gate, accepted base requirement and exact next action. Do not duplicate G03's complete conditional pair. |
| `INTEGRATION_MANIFEST.md` (new) | Exact accepted 20-path source/test map, source/import/closing refs, blob identities and manifests, 11 excluded-path proof, current branch topology, evidence reuse rationale and no-cross-owner-Cargo decision. Link parent receipts and per-goal verification. |
| `PR_SUMMARY.md` | Replace stale opening “no accepted production changes yet.” Describe G01 error preservation/feedback; gated G05 shared metadata/literal foundation; G06 Jump editor/validated transaction; G09 tests only. State G02/G03/G04/G07/G08 exclusions and G05/G06 incompleteness. List final checkpoint tests without summing them into a new suite. No fabricated CI/native/merge success. |
| `FINAL_HANDOFF.md` (new) | Actual reviewed/published refs and PR link when known, all dispositions, excluded source/default gates, canonical next actions, role attribution, cleanup and a link to the one final manual-review checklist. Until host facts exist use explicit pending fields, never invented refs/URLs. |
| `MANUAL_REVIEW_CHECKLIST.md` (new, single checklist) | The short final user review checklist specified below; link it from FINAL_HANDOFF instead of maintaining multiple copies or asking for mid-goal tests. |
| `RISKS_AND_DECISIONS.md` | Current unresolved gates and truthful source/no-change/limited-native boundaries, with historical entries preserved. Record missing old local reports and remote historical evidence separately. |
| `MEGA_GOAL.md` | Preserve its authored contract and dated events, label historical source baseline as such. Reconcile current status header/goal matrix and section 10 active state: it currently still says NOT_STARTED/G00/all pending despite G00–G09 dispositions. Link the authoritative run. Do not describe all features as complete. |
| Original G00/G03 Goal mutable sections | G00 accepted documentation/continuation cleanup; G03 dependency-deferred and original conditional contract intact. Reconcile current fields/milestone rows against their final receipts; do not modify frozen requirements or imply G03 work ran. |
| G10 Goal mutable section and G10 progress | Parent records actual current documentation/review/import/PR phase and closing receipts. Keep implementation acceptance distinct from completion of reconciliation. |
| `docs/plans/continue.md` | Its two live links are already correct. Verify they resolve and that resumed work reads the reconciled status first; do not resurrect historical queue instructions. Only change if a concrete final-pointer correction is necessary. |

Other prior per-goal progress files contain author-stage statements followed by parent closure receipts. The writer must not bulk-rewrite those dated events. The authoritative current table and clear historical labels resolve ambiguity; identify a genuinely misleading unqualified current field for parent correction only when necessary.

## Static checks and evidence rules

One bounded static pass, no Cargo/CI/native campaign:

1. Parse the two parent baseline JSON receipts and every JSON receipt directly cited by newly prepared proposals using `ConvertFrom-Json` or an existing standard-library equivalent. Record files/count, errors and required fields. Parsing is not test acceptance; preserve null exits, timed-out flags, late exit 0 and missing test-run states exactly.
2. Check the 11-goal table is complete and unique; its branch/ref joins match `BASELINE_BRANCH_RECEIPT.json`. Check the 20 source paths cover G01/G05/G06/G09 exactly once and consume the exact donor/current blob values from `BASELINE_SOURCE_PARITY.json`. Assert unchanged manifests and the 11 excluded paths using that same receipt. Do not infer an entire source tree is accepted from a closing donor ref containing rejected prototypes.
3. Validate local Markdown links in new/changed program documents and complete canonical Goal/Kickstart pairs. Resolve paths from each file's directory; distinguish local links, fragments, external pinned historical URLs, code examples and intentionally absent historical reports. Log actual broken local links, not blanket “all links valid” from a grep. A known absent old report stays a documented absence, not a fabricated replacement.
4. Search only current-facing status fields/proposals for contradictory claims: all goals pending, no accepted production, stale next-goal action, G02/G07/G08 source accepted, G05 conditions enabled, G06 native passed, G08 corrected tests passed, G09 universal retirement. Classify dated events/frozen contracts separately; do not delete history to make a negative text search pass.
5. Run `git diff --check` and a scoped changed-path manifest for the documentation checkpoint. Parent reuses baseline parity for an unchanged source tree; after promotion it performs one source/no-config-drift check against the accepted baseline. Record any actual mismatch instead of silently importing or fixing source.

An optional compact JSON summary may be emitted by the implementation/static tooling if parent authorizes that role path. It must contain only safe paths/identifiers/counts/statuses, not application payloads. The planner writes Markdown only. No fresh source test is justified by editing status, links or manifest documents.

## Single final user review checklist and PR handoff

Prepare one short checklist in the final handoff, not separate mid-goal requests. It should ask the user to review: (1) exact included changes and excluded prototypes; (2) accepted Diff/Jump behavior versus gated Hot Folder foundations; (3) remaining native limits and which canonical future work, if any, they want to select; (4) PR head/base and host-reported checks/review before any merge. If a concrete deferred native manual flow is offered, link its existing isolated synthetic protocol and mark NOT_RUN; do not turn this list into a new full application test matrix or waiver.

Parent performs the single independent cumulative review after the docs packet. Parent creates/updates one reviewable integration PR to then-current master using the connector, attaches it to the task, records actual URL/head/base/check/review state and attempts only normal authorized pushes. Do not merge, delete branches, force-push or retry indefinitely. If publication fails, preserve exact local/remote refs and one precise manual publication step; no claim of CI or master change. PR description must be rewritten around the final accepted scope rather than accumulated chronological progress.
