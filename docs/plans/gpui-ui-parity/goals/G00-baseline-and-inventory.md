# G00 — Establish source, scope and durable execution records

**Status: Accepted for source discovery and representative baseline.** G00-A establishes the dedicated branch. G00-B and G00-D reconcile the reviewed source registrations and named GUI owners. G00-C records keyboard Search, an independently closed Hash popout, saved presentation, and later saved-action edits with reopen evidence. Native global invocation remains with G01. Feature-specific runtime baselines and expanded GUI acceptance stay with their owners through the G25 reconciliation. See [source review](../reviews/G00-source-discovery.md) and [checkpoints](../CHECKPOINTS.md) for evidence.

Read the [master Goal](../../../CODEX_GOAL_GPUI_UI_PARITY.md), [requirements](../REQUIREMENTS.md), [goal register](../GOAL_REGISTER.md) and [validation strategy](../VALIDATION_STRATEGY.md). All work stays on the same dedicated migration branch.


## Objective and ownership

Establish the exact current source baseline, safe migration branch, existing intended behavior and complete discovery system before broad application changes. The orchestrator owns Git and the ledger; read-only planner/research work can enumerate separate source families. No full-suite baseline is required.

The planning SHA is f7bc610a86bcad1c14b69dce65b1d804df1b7934. Execution must use safely fetched latest master. This is a new program, not a continuation of a historical standalone goal’s branch chain.

## Current entry points

Read actual AGENTS/configured agents, Cargo.toml/lock, src/main.rs, src/gui/mod.rs/render.rs, commands, plugin registry/editor, dashboard registry, utility hosts, settings/persistence, native overlays and current tests. SOURCE_BASELINE lists verified starting facts and retrieval limitations. Read the full local render source where research could not retrieve it.

## G00-A — Branch and source baseline

Inspect branch/HEAD/index/dirty/untracked state and remotes. Fetch current master safely, reconcile source changes since planning, create a fresh task branch from its exact SHA, and preserve user work. If resuming this program, verify its existing continuation instead.

Materialize the plan documents, record execution baseline/environment/instructions and the new branch’s parent. Create a small coherent initial documentation commit and attempt one normal push. No code/build/test is needed merely to establish this checkpoint.

**Accept when:** the branch is correct, parent and baseline are separately identified, unrelated work is preserved, task docs are available, and initial push status is factual. If unsafe branch selection is blocked, report it precisely instead of choosing a wrong base.

## G00-B — Complete source discovery and ownership

Expand GUI_INVENTORY candidates by reading actual constructors, route/command registries, state enums, modal/menu/overlay creation and settings/widget callback factories. Find UI outside src/gui, conditionally enabled paths and runtime-created children.

Assign stable GUI IDs with parent/child/mode relationships and a single owner goal. Record commands/entry routes, source modules, shared operations, controls, state conditions and special Windows contracts. Add missing goals/checkpoints as necessary. Do not equate one file or one Panel enum variant with a complete GUI.

**Accept when:** every source registration has an inventory disposition, discovered children/modes are represented, unknowns have concrete investigation actions, and goal ownership has no silent gaps. Source-complete discovery remains distinct from runtime-confirmed inventory.

## G00-C — Focused behavior and test baseline

Walk representative main and utility flows on Windows, then inspect each family before its port. Capture intended behavior with synthetic fixtures: launcher invocation/query/navigation, one multiwindow utility, a persisted edit, and relevant high-risk native behavior. G01 expands actual platform feasibility.

Discover current narrow test targets/filters and existing fixture helpers. Record known failures only with evidence. Reuse historical records as context, not as new passes. Note core/service ownership and event/outcome invariants likely affected by extraction.

**Accept when:** the first foundation/pilot checkpoints have usable expected outcomes and selected scoped verification; remaining per-family runtime observation is explicitly assigned to its pre-port specification. Do not block all implementation on capturing every screenshot before the first GUI.

## G00-D — Reconcile inventory and enable execution

Review source/runtime coverage with the planner/reviewer. Update the provisional goal register and documentation schemas. Ensure every candidate and discovered GUI has an accountable next action, including dynamically generated settings and expensive studios.

Create substantive per-GUI specs for the next active slices using the template; do not create hundreds of empty pages. Record discovery incompleteness honestly and schedule remaining runtime confirmations before each GUI is accepted.

**Accept when:** branch/source ownership, complete source discovery, all known surface ownership and the first runnable checkpoint packets are ready. Runtime inventory expands and is finally reconciled at G25; no false full-GUI denominator is published at G00.

## Invariants and non-goals

No migration source changes on master; no worktrees or competing writers. No tests run just to make a planning document look rigorous. No silent default switch, schema change, feature omission or wholesale engine rewrite.

## Verification and continuation

Git/source inspection and link/ledger consistency are the main evidence for G00. Native observations are actual observed baseline only. Targeted tests run only to resolve a concrete baseline question. Once G00-A and sufficient source scope exist, begin G01 immediately; independent source-family discovery may continue read-only.
