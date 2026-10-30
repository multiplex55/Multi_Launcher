# Conditional G08 continuation — Explicit invocation recipe policy

State: PENDING_HISTORY_ACCEPTANCE_AND_POLICY; not a new initiative. Author: G08 planner, 2026-10-30. Source inspected at parent-supplied G08 `1df7c1531b5d46b7d8a94873fce722f6ce1d588f`; master `4a1283e6`, separate integration `ca2f6acb`. Historical refs are provenance, never reset targets. Parent must record actual accepted history/integration source before activation. Companion: [KICKSTART.md](KICKSTART.md).

## Outcome and evidence

Resolve the optional G08 recipe meaning and permit at most one explicit user-authored recipe workflow only when its current validation and authority can be represented through existing owners. A displayed historical run never authorizes execution. If the intended behavior is already covered by templates, document that exact existing workflow instead of duplicating it.

Observed owners: `src/mkmacro/templates.rs` stores user-authored independent package templates; `plan_template_instantiation` clears copied hotkeys. `src/gui/mkmacro_dialog/package_ui.rs` previews and explicitly applies template copies. `src/mkmacro/invocation.rs::{MkInvocation,prepare_parameters,compile_invocation_program,validate_parameter_assumptions}` models transient typed requests; `src/mkmacro/runtime.rs` owns current availability/admission and invocation source. Dialog `prepare_execution_checked`, `run_selected_macro`, `run_from_step`, `run_selected_steps` preserve current validation and MacroDialog attribution. `src/mkmacro/runtime/summary.rs` intentionally excludes executable invocation arguments and raw side effects.

Confidence: high that templates and transient invocation are distinct owners; high that summaries are insufficient replay authority. Counterevidence: an existing explicit template-copy workflow already supports safe authoring reuse, so no generic recipe infrastructure is justified. No native/permission failure has been demonstrated. The unresolved requirement is product/authority policy for a new saved invocation, not code difficulty or a missing UI button.

## Required policy before implementation

Record whether recipe means existing template authoring or a named current-macro invocation. For a new invocation, specify stable target identity, parameter input treatment (no inferred historical values or secret persistence), subset/mode, disabled/deleted/changed-target handling, explicit confirmation/start intent, permitted authoring container within existing storage, cancellation/completion ownership and original invocation attribution. Identify the exact existing validation and mutation seam; a new store/schema or permission model requires a new explicit scope decision and is outside this continuation's default authority.

No raw event replay, auto-trigger, automatic launch on display/copy/import, raw history logging, hidden polling, secret argument retention, new executor, generic framework, recorder redesign or cloud service. No requirement to reconstruct a historical Selected subset from its count. Existing macros/templates/defaults and history budgets/privacy remain unchanged.

## Bounded execution stages

1. **R0 decision:** read accepted G08 source and current owners. Write a concrete recipe policy and supported/refused cases. If only existing templates are intended, revalidate their explicit-preview/copy/no-auto-run path and close as accurately scoped no-change. If a genuinely non-inferable authority/storage decision remains, preserve a precise pending decision; no source/placeholder UI. Parent may ask one necessary product question rather than invent unsafe defaults.
2. **R1 one implementation checkpoint, only if R0 passes:** implement at most one explicit authoring/launch workflow through the chosen existing owner. Revalidate current target/signature/values and runtime admission at the actual launch boundary. History supplies at most a non-authorizing target hint; request fresh values and explicit intent as defined in R0. No parallel runner or saved history payload. Include directly necessary pure/UI/ownership tests in the same coherent checkpoint.
3. **R2 review/closure:** review actual authority, privacy, source attribution and stale-target handling; resolve at most one substantive correction pass. Record exactly shipped versus refused behavior and source/test/native limits.

## Verification and completion

No Cargo for R0 docs-only. Existing relevant source tests include `templates_survive_reload_and_capture_the_full_draft_closure`, `template_instances_are_fresh_independent_and_clear_every_hotkey`, package UI `import_preview_cancel_is_lossless_and_apply_adopts_exact_store_snapshot`, and runtime `admitted_macro_summary_records_source_subset_and_only_safe_visit_metadata`. Select only tests touched by the chosen behavior; do not run all template/invocation suites automatically.

For R1 use targeted library Nextest, `-p multi_launcher --lib --cargo-profile fast-dev --offline --locked`, one nonzero preparation capped1200s, run180s, incremental600s aggregate and 1–2 actual-error localized reruns. Test explicit intent/no-auto-run, changed/deleted/disabled target, missing/invalid/current parameters, refused unsafe combinations, admission conflict, correct source and no secret persistence. Ordinary state tests are sufficient unless R0 establishes a genuinely new native contract. No full suite/domain/build/install/broad lint or user manual request mid-goal. App fixtures disable Paste Sequence next/cancel hotkeys before construction.

Done is either an accurately evidenced unchanged existing-template decision or the accepted explicitly bounded workflow with privacy/authority proof. Recipe ambiguity remains a truthful defer; it must not downgrade already accepted history. On a concrete stop preserve code/receipts, exact unmet gate, cleanup/ownership and smallest restart; do not integrate unaccepted execution source.

Parent owns branch, ledger, Git and integration; use accepted current history as base, one writer at a time. Requested Planner/Reviewer Astra max, Implementer Astra xhigh unless parent explicitly selects otherwise; configured roles and resolved backend recorded separately. Never modify role configuration. Commit meaningful stage-tagged changes, attempt one normal push each, preserve local success if push fails, and do not merge master automatically.
