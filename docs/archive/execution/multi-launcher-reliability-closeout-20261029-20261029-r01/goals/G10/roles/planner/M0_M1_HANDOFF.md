# G10 M0/M1 — One bounded reconciliation packet

## Objective

Prepare a complete documentation-only reconciliation of accepted G00–G09 work, residual gates and final review/PR readiness. Produce concrete parent-ready proposals, not new production work. This is one coherent writer checkpoint; subsequent independent review, authoritative promotion/import and publication belong to parent.

Read the complete active G10 [Goal](../../../../../../reliability-feature-closeout-20261029/goals/G10-residual-defect-triage-integration/GOAL.md)/[Kickstart](../../../../../../reliability-feature-closeout-20261029/goals/G10-residual-defect-triage-integration/KICKSTART.md), then [PROVENANCE.md](PROVENANCE.md), [RESIDUAL.md](RESIDUAL.md), and [DOCS_REQUIREMENTS.md](DOCS_REQUIREMENTS.md). These three bounded specifications are the packet; no fresh source audit is needed.

## Ownership and scope

You are the sole documentation writer and are not alone in the repository. Do not revert or overwrite parent or other-role work. Own only `goals/G10/roles/implementer/` artifacts under this execution run. Parent alone edits global ledgers, original mutable Goal/MEGA fields, Git refs, final PR and canonical final handoff. You may prepare exact replacement sections/files beneath your role folder, with their intended destination recorded, but do not promote them yourself.

Suggested coherent outputs (equivalent names are fine): `PROVENANCE_AUDIT.md`, `RESIDUAL_DISPOSITION.md`, `GLOBAL_RECONCILIATION_PROPOSAL.md`, `FINAL_HANDOFF_PROPOSAL.md`, `STATIC_CHECKS.md`, and your `ROLE_RECORD.md`. Include a complete proposed dependency map, integration manifest, final PR summary and one manual-review checklist in those artifacts; do not stop at telling parent what to write. Avoid copying every historical receipt into another store; link existing evidence.

## Current facts and ordered work

1. Consume parent `BASELINE_BRANCH_RECEIPT.json` and `BASELINE_SOURCE_PARITY.json`. The G10 baseline is `535993a4`, from accepted integration `ea3a7007`; master is `4a1283e6`. Newest accepted tests `fc9e5c7c`, last production `b9f5fa4b`. All 20 accepted paths are donor-identical; excluded G02/G07/G08 paths and manifests have their own explicit proof. Use the receipts rather than repeating Git enumeration.
2. Build one complete G00–G10 exact-ref table using Planner PROVENANCE and existing per-goal verification/acceptance links. Preserve corrected cumulative checkpoints and failed intermediate exclusions. The branch receipt is baseline/current at its timestamp, not final head/push proof.
3. Resolve residuals using Planner RESIDUAL: original queue04/05 user authority stops; original07/08/13 historical native/verification stops with current owner uncertainty; original10/17 upstream blocked. Distinguish original versus current numbering. Preserve the remote pinned matrix as historical evidence; do not reconstruct missing local reports. Existing research NO_ACTION/H0 and closed reviewer contingencies stay closed.
4. Author exact global reconciliation proposals per DOCS_REQUIREMENTS, especially missing `DEPENDENCY_MAP.md`, stale `MEGA_GOAL.md` current rows, G00/G03 mutable fields, `PROGRAM_STATUS.md`, `PR_SUMMARY.md`, new integration manifest and final handoff. Keep future PR/review/closing SHAs explicitly pending until parent obtains them. G10 document preparation is not already independently accepted.
5. Perform the single scoped static pass specified there: parse cited JSON, validate table/ref/blob completeness, resolve local Markdown and canonical pairs, check contradictory current claims and `git diff --check`. Do not modify source/config/manifests, build, run Cargo, launch an app, execute native probes or start CI. No full historical-doc link sweep.
6. Return concise readiness and exact artifact paths, static results and any actual discrepancy. Parent reviews/materializes proposals on the G10 branch, obtains independent cumulative review, then imports accepted docs and publishes the final integration PR. Do not pre-review all accepted source or independently execute the final parent phases.

## Invariants and non-goals

- No new source, feature, old candidate import, dependency, generic framework, benchmark, broad build or test campaign.
- G02/G07/G08 source remains excluded. G03 remains gated. G05 optional conditions remain refused. G06 native proof remains deferred. G09 native evidence remains only the accepted controlled file-handle scenario.
- Missing historical reports remain missing; remote historical narratives remain labeled. User-deferred original work is not silently scheduled.
- No new Cargo solely for ceremony. Different source owners and unchanged manifests provide evidence-reuse grounds. Only a concrete source overlap/conflict discovered by final review justifies a narrow extra check; stop for the parent scope decision rather than broadening.
- No source fix in G10. A distinct demonstrated defect gets a complete scoped role-authored future pair; existing incomplete goals and stale-doc reconciliation do not create extra initiatives. Current Planner disposition is NO_ACTION for distinct findings.

## Done criteria

One reviewable proposal packet contains all eleven current dispositions, exact branch/source/test/native evidence, correct original-goal authority/gate classifications, complete canonical follow-on and dependency references, concrete current-doc replacements, an accepted-only integration manifest, truthful PR summary and one final manual checklist. Static checks have actual outputs/counts and no unresolved newly introduced link/ref contradiction. No source or global-owner file changed. Parent can apply the approved content without rediscovering the decision or inventing missing facts.

Genuine remaining uncertainty is final publication/review state, not implementation ownership. Parent resolves actual final head, upstream, PR and check status after its operations. Any artifact that depends on those future facts must retain an explicit pending field until then. This packet does not promise successful publication or completion of deferred features.
