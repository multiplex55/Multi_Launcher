# Codex Goal: complete Multi Launcher GPUI UI parity

**Required startup and resumption.** Every Planner, Implementer, Reviewer and orchestrator follows the [Kickstart startup sequence](CODEX_KICKSTART_GPUI_UI_PARITY.md#mandatory-startup-and-resumption). [USER_PRIORITIES.md](plans/gpui-ui-parity/USER_PRIORITIES.md) governs user-defined scheduling. The [Execution Strategy](plans/gpui-ui-parity/EXECUTION_STRATEGY.md) governs batching and verification. This Goal defines scope and final acceptance. [AGENTS.md](../AGENTS.md) governs repository-wide engineering and agent conventions.

## Assignment

Create a complete second native frontend for Multi Launcher in Rust using **official GPUI Kit**. Recreate the current application **GUI by GUI**, preserving its controls, information, actions, input behavior, workflows, child interfaces, supported window modes, configuration, and data semantics. Improve the presentation through a coherent visual design while keeping the product recognizable.

Keep the complete egui frontend. Both frontends must use one authoritative application engine. An application session initializes exactly one frontend. A GPUI session must provide the entire application without opening egui windows, running a hidden egui host, or painting egui controls into textures.

The program is finished only when the user can choose GPUI and use all of Multi Launcher as before, with a cleaner presentation and a dependable egui fallback. A main-window prototype, partial feature catalog, mock implementation, and “use egui for the rest” are intermediate states.

This is one implementation program with **27 initial work goals, G00–G26**, on one dedicated branch. The goal packets are the initial executable breakdown, not a cap on scope. Split complex packets into additional recorded checkpoints or child goals when the actual inventory warrants it. Every discovered GUI and child flow must retain one accountable owner.

The interim [Daily-Driver Readiness milestone](plans/gpui-ui-parity/DAILY_DRIVER_READINESS.md) prioritizes ordinary use of P0 workflows. The [maturity register](plans/gpui-ui-parity/PLUGIN_MATURITY_REGISTER.md) records progress by feature. The original matrix and final acceptance remain the delivery authority.

## 1. Read these contracts first

Read this document, [the Kickstart](CODEX_KICKSTART_GPUI_UI_PARITY.md), current repository `AGENTS.md`, applicable nested instructions, current `.codex/config.toml` and `.codex/agents/*`, then:

- [Requirements](plans/gpui-ui-parity/REQUIREMENTS.md).
- [Source baseline](plans/gpui-ui-parity/SOURCE_BASELINE.md).
- [Frontend selection](plans/gpui-ui-parity/FRONTEND_SELECTION.md).
- [Core contracts](plans/gpui-ui-parity/CORE_CONTRACTS.md).
- [Goal register](plans/gpui-ui-parity/GOAL_REGISTER.md).
- [Validation strategy](plans/gpui-ui-parity/VALIDATION_STRATEGY.md).
- [Documentation protocol](plans/gpui-ui-parity/DOCUMENTATION_PROTOCOL.md).

Use the checked-out source as execution truth. The planning reference is `multiplex55/Multi_Launcher` master commit `f7bc610a86bcad1c14b69dce65b1d804df1b7934`, inspected November 21, 2026. At first kickoff, fetch current `origin/master` and record its exact SHA. Reconcile any new commits against this package before implementation. Once established, keep that behavioral baseline stable; later upstream integration requires a recorded decision and affected-scope reconciliation.

Historical plans may explain an existing subsystem. They do not silently impose old branch chains, model assignments, test campaigns, or acceptance cycles. This program explicitly uses one branch across its goals.

## 2. Product invariants

### PAR-01 — Full structural and behavioral parity

Port every GUI, subdialog, mode, meaningful control, status, configuration path, error path, and workflow reachable in the baseline. Include optional and dynamically generated plugin surfaces. Derive scope from both source and actual behavior.

Preserve command syntax and prefixes, defaults, validation, limits, enablement, selection, keyboard bindings, action results, save/apply/cancel behavior, remembered state, and external side effects. Keep the same information available at comparable density. Reuse names and labels unless a proven defect requires correction.

### PAR-02 — Visual refresh with recognizable structure

Improve typography, spacing, borders, icons, theme consistency, focus visibility, selection, and hover feedback. Preserve the organization and functional relationships the user relies on. Do not hide controls, remove advanced configuration, collapse unrelated windows, or rearrange workflows merely because a stock component is easier.

Pixel-identical egui rendering is not required. Record intentional presentation changes with comparable screenshots and inspect text sizing, small windows, long content, empty/error states, and supported DPI scales.

Use a 1516 × 937 pixel window in both frontends for primary visual and functional parity checks. Match display scaling and application state. Capture primary screenshots at that size and compare information density, control placement, spacing, alignment, and usable workspace. Compact and minimum sizes supplement this baseline. Application defaults, minimum sizes, and saved window geometry retain their existing ownership.

G05 must match the egui launcher’s top-centered query field. Keep it visible at a stable vertical position while results and docked utilities occupy and scroll through the space beneath it. Preserve focus, keyboard navigation, autocomplete, and Jump Mode through scrolling and resizing.

### PAR-03 — One engine, two complete frontend implementations

The same commands, validation, queries, persistence, document logic, jobs, and external actions must serve both frontends. Extract behavior from egui-owned handlers at its real architectural owner. Do not copy a subsystem into GPUI under different names.

Sharing an engine does not mean sharing a widget tree. Each frontend owns its framework state, drawing, layout, input translation, local focus, and window integration.

### PAR-04 — Exactly one selected frontend

Resolve frontend selection before framework/application/window/GPU initialization. The inactive frontend may be linked into the executable but must not start its graphics runtime, windows, listeners, background service copies, or render loop. Plain native dialogs and already framework-independent Win32 facilities may be reused where appropriate; they must not conceal egui dependencies.

### PAR-05 — egui remains the ordinary-launch default

Use the actual executable name:

~~~powershell
.\multi_launcher.exe
.\multi_launcher.exe --ui=egui
.\multi_launcher.exe --ui=gpui
~~~

For this program, no selector means egui. An explicit selector overrides any remembered choice. A settings action may remember the requested frontend and restart with its explicit argument; it may not silently change bare-executable startup to GPUI. Do not promote GPUI to the default at the final milestone. This resolves the newer user instruction about the no-argument default; see FRONTEND_SELECTION.md.

### PAR-06 — Data compatibility makes fallback real

Use the same application identity, data ownership, config paths, document formats, validation, and canonical stores in both frontends. Separate egui widget memory from GPUI presentation memory. Preserve semantic settings and compatible geometry. Verify meaningful cross-frontend round trips using isolated fixtures.

### PAR-07 — Core behavior remains correct

Preserve launch execution, command dispatch, query behavior, indexing, plugin logic, protected clipboard behavior, hotkeys, recording, file operations, validation, persistence, and recovery. Recent fixes on the selected master are part of the baseline. A newer UI must not restore a previously corrected race or operation semantics.

### PAR-08 — Native Windows behavior is acceptance work

Global invocation, foreground activation, hotkey toggle/tap/hold, utility focus, hidden-root lifetime, overlays, clipboard delivery, DPI, multi-monitor placement, and supported window modes require relevant native evidence. Headless tests and screenshots cannot establish all of these.

### PAR-09 — The ordinary distribution contains both choices

Preserve the normal `cargo build --release` workflow and the normal `multi_launcher` binary. Establish early that the actual dependency graph builds, links, and launches both frontends from the same Windows artifact. Developer-only egui builds may help iteration, but do not replace dual-frontend distribution verification.

### PAR-10 — Complete work remains distinguishable from scaffolding

A GUI becomes Accepted only after its applicable controls, interactions, outcomes, native checks, visual checks, and data behavior have evidence. A generic unavailable page, default no-op host method, callable backend without controls, or hidden restart into egui does not satisfy GPUI parity.

## 3. GPUI Kit implementation requirements

Use https://gpui-kit.com/ and its official source https://github.com/longbridge/gpui-kit. The inspected documentation identifies Kit 0.10.0; verify and pin the exact suitable release at G01, together with its aligned GPUI packages, compiler, Windows SDK/build tools, features, and Cargo.lock. This document does not claim that dependency compatibility has already been established.

Use Kit’s components, behavior primitives, themes, assets, and interaction facilities as the normal foundation. Compose or extend them for application-specific behavior. Use the selected release’s APIs and retain stateful inputs/entities across renders. Keep subscriptions and task cancellation associated with the correct view/window lifetime.

Use Kit’s matching development test support for production views where appropriate. Register application assets and inspect the packaged artifact. Default kit assets do not automatically cover all Multi Launcher resources.

Keep this a native Rust Windows application. Webviews, Tauri, JavaScript runtimes, WebAssembly distribution, a new plugin language, and optional alternative GPUI engines are outside scope. A needed upstream adaptation must be narrow, recorded, and verified against the required behavior. Never reduce scope to conceal a toolkit limitation.

## 4. Architectural ownership

Start with the existing typed command system, domain modules, settings/persistence boundaries, and native wrappers. Inspect `src/commands/`, `src/gui/command_host.rs`, `src/gui/mod.rs`, `src/main.rs`, `src/plugin.rs`, `src/platform/`, and the actual feature owners.

The current code is not assumed to be a finished headless engine. Existing host methods and plugin settings can still depend on egui or LauncherApp. Map each such dependency before extracting it. Existing no-op default trait methods must not make incomplete GPUI command handling look successful.

Required boundaries:

1. Typed application requests and outcomes contain domain meaning rather than UI objects.
2. Shared application state and stores have one authoritative owner per logical document/service.
3. UI intents describe opening/focusing/hiding/presenting tools without exposing egui viewport IDs or GPUI entities.
4. Framework adapters implement those intents in their own event/window lifecycle.
5. Jobs publish bounded, versioned results; stale results cannot update a replacement document/window or repeat a side effect.
6. Service ownership is deliberate: one hotkey/hook owner, one clipboard listener, one watcher/indexer/timer/automation service as appropriate.
7. Framework-specific state is not serialized as the other frontend’s model.

Do not build a universal draw-button/draw-panel API, a daemon, or a new IPC architecture to avoid thinking about these boundaries. The existing single-instance mechanism may need a narrow selection-aware response, but this is not authorization to redesign the application into a service.

Perform extraction incrementally. G02 establishes the minimum useful shared seams and invariant map. Later feature goals extract their remaining egui-owned behavior when they port the corresponding GUI. Do not delay useful GPUI work behind a rewrite of every module.

## 5. Native feasibility before bulk conversion

G01 must prove the selected Kit configuration inside the actual application binary and resolve the event-loop/threading arrangement. The current eframe host topology is evidence about egui, not a guarantee about GPUI.

Exercise representative native behaviors early: a real hotkey invocation, an editable query, repeated hide/show, a secondary window while the root is hidden, foreground restoration, supported no-activate/topmost behavior, coordinate/DPI conversion, and a representative overlay/capture path. If a capability belongs to an existing independent native subsystem, prove the GPUI adapter can drive it without an egui runtime.

Record which paths are proven, which need a narrow adaptation, and which remain blocked. Investigate difficult platform questions before dozens of ordinary dialogs depend on assumptions. Native checks may be developed with a minimal shell; they do not count as full launcher or overlay parity.

## 6. Startup, switching, and running work

Select the frontend before initializing either UI. Preserve existing noninteractive/helper startup modes, command forwarding, process identity, foreground capture, recovery-before-load ordering, and application data lock semantics.

An explicit `--ui=egui` invocation while GPUI owns the data directory must not silently disappear or reactivate GPUI. Implement a controlled restart or a clear actionable existing-instance response. Recovery instructions must still work when GPUI cannot show its settings. Automatic killing of a hung process and automatic recovery from arbitrary native crashes are not required.

Switching uses a restart. Account for unsaved documents, pending writes, active transfer/automation/recording, and user cancellation. Preserve current save/finish/cancel semantics and do not claim active work survives process replacement unless it already has a real supported recovery mechanism. Preserve working-directory/data-path identity during relaunch. Do not replay action arguments or completed jobs.

## 7. Work sequence and checkpoint discipline

Use the [goal register](plans/gpui-ui-parity/GOAL_REGISTER.md). G00 reconciles source and inventory; G01 resolves major feasibility; G02–G04 establish reusable ownership, selection, and one complete real GUI. G05–G24 cover the actual product. G25 reconciles cross-feature behavior. G26 closes the production acceptance.

Read the next goal and relevant GUI records. The planner creates a checkpoint packet from real source. Each packet names objective, owner, source paths/symbols, requested changes, invariants, dependencies, non-goals, targeted tests, evidence, and an objective exit condition. Hand that packet to one implementer.

Complete coherent vertical slices: retained state, all controls, domain operations, modes, errors/cancellation, persistence, and interaction verification. Do not spread empty shells across the application to inflate progress.

Use natural checkpoint commits such as `feat(gpui): [G07-B] preserve file search filters and result actions`. Commit coherent increments before materially different work. A checkpoint may be committed with required native evidence still pending when accurately recorded; that does not make its GUI Accepted or its goal complete. Preserve a buildable, working egui path at accepted checkpoints.

Follow the [Execution Strategy](plans/gpui-ui-parity/EXECUTION_STRATEGY.md) for the two functional and verification passes, immediate high-risk checks, and scheduling independent work. Apply the GUI records, requirements and validation contract to every criterion.

## 8. Branch and agent workflow

At first program initialization, create the dedicated branch from the latest safely fetched `origin/master`. Record the parent SHA, branch, initial baseline commit, and upstream. Preserve staged, unstaged, and untracked user work.

All G00–G26 work stays on this one branch. Resume the existing checkout from its preserved state. Use the single branch throughout ordinary implementation. For an authorized upstream merge, integrate master into the feature branch at a clean checkpoint, record both parent revisions and the result, and reconcile evidence only for the affected scope. Retain the historical behavioral baseline.

Use current configured planner/implementer/reviewer roles. Read their actual configuration; do not carry old model assignments forward. The parent coordinates source ownership, records, commits, and best-effort pushes. Parallelize bounded read-only investigation. Use one active source writer, one shared heavy build/test process, and one live-desktop controller. Do not allow read-only review to race an evolving diff.

After each coherent commit, attempt one ordinary non-force push when possible. If it fails, preserve local commits, record the result, and continue. Publishing the migration into master or publishing a release requires separate authorization.

## 9. Persistent planning and evidence

Maintain the dedicated directory `docs/plans/gpui-ui-parity/`. The package supplies initial contracts, goals, source seeds, tracking schemas, and templates. It deliberately does not claim a completed runtime inventory.

Update the authoritative records, not every document:

- GUI_INVENTORY identifies surfaces, modes, routes, dependencies, and accountable goals.
- PARITY_MATRIX links control/behavior requirements to implementation and evidence.
- Individual `gui/<stable-id>.md` records contain each GUI’s specification and acceptance.
- GOAL_REGISTER identifies order, dependencies, checkpoints, and status.
- PROGRESS summarizes implemented and accepted work with explicit denominators.
- VALIDATION_LOG records real commands, selected tests, outcomes, timings, binary/commit identity, and coverage.
- CHECKPOINTS preserves resumable next actions and Git state.
- KNOWN_ISSUES and BLOCKERS distinguish baseline defects, migration regressions, and environmental gaps.
- DECISIONS records genuine tradeoffs and explicit scope exceptions.
- FINAL_ACCEPTANCE reconciles the delivered artifact against the original scope.

Create substantive per-GUI records as discovery proceeds; do not generate hundreds of empty files. Seed candidates are leads, not counted accepted GUIs. New discoveries expand the denominator openly.

Statuses are Not started, Specified, In implementation, Implemented, Validation pending, Accepted. Track Blocked separately from the last achieved stage. Documentation preparation has accepted zero migration GUIs.

## 10. Focused validation contract

Follow the [Execution Strategy](plans/gpui-ui-parity/EXECUTION_STRATEGY.md) for verification cadence and the [Validation Strategy](plans/gpui-ui-parity/VALIDATION_STRATEGY.md) for evidence requirements. Select the relevant Cargo target before Nextest filters and reuse meaningful existing tests and fixtures.

For the inspected baseline, preserve the Windows Nextest wrapper and stack floor; apply the AGENTS fixture rule disabling Paste Sequence Next/Cancel shortcuts before app construction without changing production defaults. Confirm actual commands and runner configuration at kickoff.

Build time, linking, test execution, native checks, and visual checks are separate evidence. Do not apply a blanket 100-second deadline to an entire Cargo command. Filters selecting zero intended tests do not establish verification.

Use meaningful core tests plus working egui behavior for early extractions before a GPUI route exists; mark the GPUI checks pending. Once implemented, exercise both routes and compare to the baseline’s intended outcomes. Two new frontends agreeing with each other is not sufficient if both now share the same regression.

Kit headless tests should exercise production views and resulting operations. Native checks cover OS behavior. Screenshots cover visual appearance. Verify relevant accessible names, roles, states, actions, keyboard operation, and focus; dependency presence is not accessibility acceptance.

Use a named broader union only for a concrete changed shared boundary or evidence of collateral failures. A full suite may be justified but is not an automatic initial, per-goal, or final gate. After sufficient scoped evidence, stop optional re-testing and continue implementation.

Later changes must selectively invalidate affected earlier acceptance. Record why old evidence still applies or which scenario must be repeated. Do not blanket-reset all GUI acceptance for an unrelated change.

## 11. Continue to the full result

After completing a checkpoint or goal, update records, commit/push as appropriate, and continue to the next eligible task. Do not request routine permission at each screen or stop at the pilot.

At a session boundary, checkpoint branch/HEAD, working-tree state, active goal, GUI IDs, partial edits, tests, pending native checks, blockers, and the exact next action. Include the active Execution Strategy in the handoff and agent assignments. Resume from this state.

Normal compiler failures, local UI defects, and failing migration tests are work to resolve. For a persistent blocker with no new evidence, stop identical retries, record what was attempted and the smallest needed capability, and continue independent work. If all independent work is exhausted, create the current AGENTS-required stall report and report truthful partial status.

Do not promise unattended progress beyond the execution environment’s lifetime. Durable records make a future run resumable; they do not make a stopped run continue invisibly.

## 12. Final definition of complete

Full original completion requires all of the following:

1. Source and runtime inventory includes every baseline GUI, child surface, supported mode, optional route, and cross-feature workflow.
2. Every applicable control, information item, state, interaction, and operation has an accepted GPUI counterpart.
3. The selected GPUI Kit foundation is actually used coherently.
4. The ordinary release build produces the intended distribution with both frontend choices.
5. No-selector and explicit-egui startup use egui; explicit GPUI startup uses GPUI; only the selected frontend initializes.
6. Explicit fallback works independently of GPUI settings, including a defined existing-instance response.
7. The full egui frontend remains functional and its core behavior is preserved.
8. Shared authoritative operations prevent duplicated command, validation, persistence, and job logic.
9. Data/config round trips preserve meaning and content, including saved changes made by either frontend.
10. Required native-window/input/clipboard/overlay/DPI evidence exists, with no mandatory gap mislabeled as passed.
11. Visual quality, density, resizing, text sizing, keyboard operation, and applicable accessibility are verified.
12. Measured performance meets recorded practical budgets or a specifically accepted decision addresses a variance.
13. No unresolved known migration-caused core regression remains.
14. Final evidence is applicable to the delivered tree and artifact, review findings are resolved, and all remaining scope has been reconciled.
15. Build/run/switch/recovery instructions, exact delivered commit, branch, push state, and coverage limits are recorded.

An explicit user exception can change accepted scope, but it never transforms an unperformed check into a pass or proves full original parity. Report the revised outcome accurately.

## 13. Final report required from the implementing AI

Lead with actual outcome: complete, partial, or blocked. Include a goal summary, GUI counts and denominator, remaining gaps, architecture decisions, selected Kit/toolchain, targeted test commands/results, native and visual evidence, performance comparisons, exact release artifact/commit, egui fallback instructions, and known limitations.

Give the actual branch, baseline SHA, final tip SHA, commits/push status, and working-tree status. Do not attempt to make a tracked file contain its own commit hash through endless amend loops. State any deferred validation plainly. When complete, stop optional work; do not merge or publish a release without separate authorization.
