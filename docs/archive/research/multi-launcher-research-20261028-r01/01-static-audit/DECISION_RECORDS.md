# Architecture decisions — Goal 01 / M2-B

Baseline `59156d96adcc73939c7171487a0cb582e3286587`; 2026-10-28. Static evidence and alternatives, no production changes. “Accepted” below means a proposed future package is worth review; all packages remain `PROPOSED_NOT_AUTHORIZED`. No generated goal is executed by this decision record.

## D01 — Correct raw-token spelling at its existing owner

**Decision:** retain AUD-001 as `READY_FOR_IMPLEMENTATION_REVIEW`, canonical ID AUD-F001; use the complete planner pair, no duplicate author package. EOF handling duplicates a raw backslash after it was already recorded. `split_action_filters` and tagged snippets forward raw spelling. Matching-prefix Literal limits manager-level impact; snippet-local no-tag preservation does not undo upstream manager reconstruction; terminal pairs do not take the same pending-escape branch. [AUD-001](findings/AUD-001-terminal-query-escape.md)

**Alternatives:** no change keeps a direct byte-preservation inconsistency; repairing callers duplicates work and misses another caller; parser/extraction rewrite expands compatibility risk. A local EOF correction plus focused pure/sync-snapshot/tagged-snippet regressions is the smallest complete change. Preserve decoded value, quotes, filters/order/normalization and literal payloads.

**Value / risk / reversal:** low severity, high source confidence, small implementation/validation scope, runtime frequency unknown. It ranks first because it is ready and cheap, not because measured user harm is high. If current source is already fixed when authorized, retain missing regression evidence or record no-op. No performance claim.

## D02 — Decide Diff concurrent-writer authority before choosing a repair

**Decision:** accept **AUD-F002 as a bounded research spike (`NEEDS_RESEARCH_SPIKE`)**, linked to AUD-002. Conditional medium consequence does not establish native incidence or a production defect. The spike must decide whether to retain/document best-effort metadata freshness or propose a narrow stronger identity/publication boundary. It must produce a precise proof/acceptance obligation; it must not implement the resulting production recommendation. [AUD-002](findings/AUD-002-diff-mutation-identity.md)

**Alternatives:** no change is viable if the explicit contract accepts concurrent external replacement; a small local metadata tweak cannot by itself close validation-to-path-use races; existing native guards may supply suitable identity primitives but Rename/Replace policy/journals are not automatically suitable for Diff. Narrow reuse belongs in a later accepted design, and a universal filesystem framework is rejected. The decision spike compares these choices at real Diff owners/callers and preserves confirmation, overwrite intent, dirty documents, containment, recycle-only/no-fallback behavior and partial outcomes.

**Value / risk / reversal:** file authority is consequential enough to resolve deliberately, but validation burden is higher than AUD-F001. Source distinction is high confidence; native feasibility/likelihood unverified. Documentation/abstract proof is bounded and harmless. If current source already has sufficient stronger ownership or the accepted contract explicitly tolerates the condition, the successful outcome is a source-backed no-op/limitation decision. A mandatory native observation that cannot be obtained is an explicit limited result, not invented proof. Complete raw [Goal](roles/implementer/follow-on/AUD-F002-diff-mutation-authority/GOAL.md) / [Kickstart](roles/implementer/follow-on/AUD-F002-diff-mutation-authority/KICKSTART.md); M3-A canonicalizes only.

## D03 — Preserve dashboard's explicit query grammar

**Decision:** **NO_ACTION** for a goal forcing PluginHome/Pinned Query parity with manager Literal policy. Both widget helpers explicitly split action filters, query a selected provider, then apply filters. Pinned Query additionally rejects a disabled configured engine and tracks provider ticket/cache state. Matching manager Literal routes deliberately preserve raw payload; QR is a concrete grammar difference, not proof the widget violates a shared contract. [Q1], [Q2], [Q3]

**Alternatives:** no change preserves existing saved widget grammar. Reusing manager policy blindly changes queries containing `kind:`/other recognized filter tokens, including literal QR data; a shared low-level helper with explicit caller policy is possible but adds an API/migration without an established requirement. Accepting cross-surface parity would require product semantics and saved-widget compatibility, not just deduplicating three lines. Neither source nor current task establishes that requirement. No parity package or silent persisted-query migration is accepted.

**Reopen condition:** a future explicit parity request or reproduced intended-widget behavior failure. Then inspect exact saved provider/query policy and define migration before implementation. Goal 02 must treat this difference as a compatibility constraint and not score literal/filter payload changes as a ranking improvement.

## D04 — Preserve Macro command handoff and exported compatibility route

**Decision:** **NO_ACTION** for broker unification or changing instantaneous-cardinality admission in this audit. The production Windows `ProductionLauncher` owns `LauncherCommandBroker`; `command` treats either Activated or PresentedForSelection as success. GUI sets current query, searches and uses current zero/one/many rows: no-results, activate exact one with Macro source, or show selection. It does not wait for every provider to become final. `last_search_pending` is tracked by search but not used by that handler. This is an explicit current contract/limit, not a claim that one current row is globally unique forever. [M1], [M2], [M3]

The older `LauncherQueryBroker` remains exported/polled, submits a raw query and activates the first row with Enter source. A bounded search of current tracked `src`/`tests` found its getter/definitions and GUI poll, but no production submit caller. That search is **not proof no external/dynamic consumer exists**. Presence alone is not an active bypass defect or permission to delete compatibility API. [M4]

**Alternatives:** preserve current handoff; have the newer broker wait on a bounded current-query stability state; or always present pending/ambiguous rows for human selection. The latter two change Macro timing/error/handoff behavior and need a stated finality requirement, cancellation/timeout/replacement ownership and provider-completion tests. No such product requirement is established here. Merging the legacy route would also change first-row/Enter behavior. A larger shared broker is not justified.

**Goal 02 constraint:** preserve current API behavior in experiments; do not equate `Activated` with effect completion or treat current row count as a stable selected identity. Any later new automatic execution must state its own eligibility/finality contract. Reopen only for an explicit contract change or concrete demonstrated failure, not hypothetical asynchronous providers alone. No orphan follow-on recommendation is left pending.

## D05 — Keep owner-specific cancellation; do not queue a generic admission refactor

**Decision:** **CONDITIONAL_WATCH, no accepted work item** for detached File Search/refinement accumulation. Source establishes stale rejection and cooperative cancellation, not a universal aggregate worker bound. Ripgrep kills/waits its child; retained-session/reaper/budget owners supply stronger retirement proof where needed. Current evidence establishes neither harmful accumulation nor required hard-stop latency. [Lifecycle report](LIFECYCLE_AND_CONCURRENCY.md)

**Alternatives:** no change preserves current responsiveness and backend compatibility; a local per-owner retained admission limit could prevent overlap but might block new work behind an uninterruptible call; a shared pool/scheduler would migrate unrelated lifetime contracts and can starve independent features. Without a demonstrated bound requirement and stalled-backend evidence, neither repair nor extraction is accepted. The [cost report](BOUNDARIES_AND_CHANGE_COST.md#performance-hypotheses-acceptance-before-optimization) specifies the smallest future evidence if a concrete complaint arises; it does not schedule it.

**Reversal trigger:** reproducible resource growth, a newly required maximum active-work count, or a shutdown guarantee the present backend cannot meet. Preserve result rejection versus retirement regardless. No missing runtime proof blocks Goal 01.

## D06 — Preserve existing architecture and test seams

**Decision:** **NO_ACTION** for a second typed bus, table-core extraction, universal save/filesystem transaction, broad input owner rewrite, crate-per-feature split or global fixture rewrite. Current owners/guards are real and useful; surveyed module size cannot establish missing boundaries or measured cost. Existing inert fixtures and grouped tests allow narrow future verification, with visibility/side-effect limits. [Boundaries/cost](BOUNDARIES_AND_CHANGE_COST.md)

Local-owner repair remains the default if a concrete future defect appears. A narrow extraction needs an actual shared behavior/caller migration plus evidence of value. Existing import/call/ownership relationships, patched native integration, serialized formats and defaults must survive. No cleanup or regression campaign is accepted by default.

## Finite result

Two proposed packages: **AUD-F001** narrow source correction and **AUD-F002** bounded contract/proof spike. Dashboard/Macro compatibility candidates are resolved NO_ACTION with explicit current semantics; worker-bound speculation is conditional watch, not a queued goal. No unresolved candidate is silently promoted into an implementation backlog. M3-A has finite consolidation and handoff work, not another open-ended architecture investigation.

## Pinned follow-up evidence

[Q1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/dashboard/widgets/plugin_home.rs#L47-L54
[Q2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/dashboard/widgets/pinned_query_results.rs#L384-L410
[Q3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L1360-L1390
[M1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/mkmacro/executor.rs#L567-L637
[M2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/render.rs#L3095-L3168
[M3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/search.rs#L817-L864
[M4]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/render.rs#L3171-L3191

Compatibility broker API: [submit/getter](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/mkmacro/launcher_query.rs#L20-L92); production factory: [new command broker](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/mkmacro/executor.rs#L641-L677).
