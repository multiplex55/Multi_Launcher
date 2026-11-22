# G26 — Verify the final distribution and deliver full parity

**Status: Not started.** This is an executable work packet, not a record of implementation or verification.

Read the [master Goal](../../../CODEX_GOAL_GPUI_UI_PARITY.md), [requirements](../REQUIREMENTS.md), [goal register](../GOAL_REGISTER.md) and [validation strategy](../VALIDATION_STRATEGY.md). All work stays on the same dedicated migration branch.


## Objective

Finish the program with a reproducible ordinary Windows distribution, applicable acceptance evidence, preserved egui fallback, resolved migration regressions and truthful user-facing documentation. This goal delivers the implementation; it does not merge/publish a release automatically.

Dependencies: G00–G25 applicable criteria accepted. If mandatory original scope remains unresolved, report partial/blocked rather than relabeling it complete.

## G26-A — Candidate and evidence scope

Select the final candidate tree and record source/dirty identity. Reconcile every GUI, control/mode/workflow and core/default/fallback criterion against FINAL_ACCEPTANCE. Identify only evidence invalidated by changes since it ran.

Select the focused final union needed for remaining shared risks; reuse valid prior results. Confirm current dependencies/lockfile, profiles/features, frontend initialization paths and production resource handling. Read current repo instruction gates, without resurrecting historical campaigns.

**Accept when:** the candidate and exact outstanding verification are defined, no known regression is hidden, and all accepted totals can be traced to applicable evidence.

## G26-B — Ordinary production build and actual launch

Run the canonical ordinary cargo build --release workflow and verify it produces the intended multi_launcher distribution with both choices. A developer egui-only feature build or standalone GPUI test app does not satisfy this criterion.

From the actual produced artifact, verify bare launch and explicit egui, explicit GPUI, exit/reopen and explicit fallback/recovery. Keep no-selector egui at completion even if GPUI is polished. Verify same canonical data identity, existing-instance behavior and selected-only runtime ownership.

Inspect fonts/icons/application resources and intended launch paths outside the source tree where supported. Record exact artifact/hash/source/profile and real native result. Reuse targeted native scenarios if still applicable; do not claim Windows proof from cross-compilation.

**Accept when:** the actual deliverable supports the specified choices and required native behavior, with usable instructions for switching and fallback.

## G26-C — Performance, visual/accessibility and regression closure

Compare the PERFORMANCE metrics relevant to changed boundaries with equivalent baseline conditions. Resolve material startup/invocation/typing/idle/lifecycle/memory regressions. Record measured tradeoffs honestly; GPU rendering is not a performance guarantee.

Inspect representative shared layout/theme/text-size/DPI behavior and any specific changed/problematic GUI. Confirm custom control semantics, keyboard usability and required Windows accessibility observations. Feature evidence may be reused when current.

Run focused tests/lint/checks justified by remaining changes and repository gates. A full suite is allowed only with a concrete current reason; finishing a large branch alone does not mandate it. Classify old failures with evidence and resolve all known migration-caused core regressions.

**Accept when:** performance/usability requirements and required validation are satisfied, with no mandatory gap relabeled as pass.

## G26-D — Final independent review, records and handoff

Have the configured reviewer inspect cumulative scope and actual receipts. Resolve material findings in coherent commits and rerun only affected checks. Reconcile final evidence freshness after corrections.

Update final acceptance, progress, run/build/default/switch/recovery instructions, dependency/toolchain record, known limitations and exact source/artifact identity. Clean up temporary migration probes/stale duplicate paths only when safe; preserve full egui code and valid developer tooling.

Commit coherent final changes and attempt one ordinary push. Report actual branch, parent/baseline, checkpoint/final tip, push status and remaining working-tree changes. Do not repeatedly amend to embed the final commit's own hash inside itself.

**Accept when:** all mandatory original criteria have evidence, review findings are resolved, documentation corresponds to the delivered artifact and the user can operate the complete GPUI or egui application.

## Final report and stop conditions

Report complete only for full original scope. If a genuine external blocker remains, identify exact affected criteria, validated subset, preserved work and next action; continue independent work until exhausted. If the user explicitly accepts reduced scope/verification, record the exception and use a correspondingly qualified outcome.

Provide concise coverage counts with the final denominator, key architectural decisions, commands/results, native/visual/performance evidence, actual run examples, fallback instructions and source/Git state. Do not claim absolute proof of bug freedom.

Once the approved program and required scoped verification are complete, stop optional testing or cleanup. Do not merge to master, force-push, create an unsolicited release, or begin unrelated features.
