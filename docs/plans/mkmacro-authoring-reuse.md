# MkMacro authoring and reusable macros execution ledger

## Authority and baseline

- Original specification: `C:\Users\Jay\.codex\attachments\2e98f628-a059-487a-8797-e277719ea22a\pasted-text-1.txt`. Read completely during planning. Its 48 numbered items and final definitions of done remain acceptance requirements; this ledger groups them into coherent implementation commits.
- Repository policy: root `AGENTS.md`; current source takes precedence over historical architectural descriptions.
- Branch: `mkmacro-editor-enhancements`.
- Clean baseline: `2dc9204d6a797884381646cd89ad9883b6e3f506` (verified by planner; parent independently confirmed).
- Baseline `cargo check`: passed, reported by orchestrator (27.37 seconds). Planner did not execute tests.
- Resume note: during the user pause, `.codex/config.toml` and `.codex/agents/{implementer,planner,reviewer}.toml` were edited from high to xhigh reasoning. User explicitly authorized committing these settings separately at the end. An external commit `74f06216 update config` subsequently committed exactly those four files; preserve that commit and do not create a duplicate settings commit. M02 resumed from its existing implementation after interruption; no source work discarded.
- Nextest installed: 0.9.135, reported by orchestrator. No repository Clippy requirement discovered.
- Planning agent changed only this ledger. Parent owns plan acceptance, source-writer sequencing, verification, commits, and status updates.

## Execution rules

Exactly one implementation writer runs at a time. Each milestone is inspected and committed before its dependent milestone starts. Implementers own only their assigned scope; accommodate other intentional changes and never revert others' work. Reuse agents where practical. Read-only reviewers may run independently.

The user's later, specific testing preference supersedes the generic instruction to run targeted Nextest after every construction milestone: implement and migrate required compile-time test constructors up front; use formatting, diff review and strategic compiler checks during construction; concentrate behavioral test expansion and expensive execution in M13/M14. A construction milestone is complete when its stated construction gates pass, not a claim that its late behavioral gates already passed. Every deferred gate is explicitly owned by M13/M14 and must pass before overall completion. Add/run a focused unit test early when needed to resolve a risky invariant, especially runtime cleanup or transactional import; do not repeatedly execute slow binaries.

Every commit: inspect `git status --short`, actual diff, `git diff --check`, and staged diff; stage only intended files. Do not run broad cleanup/format churn. Record construction verification, deferred coverage, and commit in this ledger. Record commit hashes in the following commit or a final ledger commit to avoid self-referential commit hashes.

## Pipeline status

| ID | Milestone | Depends on | Status | Commit / verification |
| --- | --- | --- | --- | --- |
| M01 | Persisted model and schema 12 | baseline | complete | 89fdb753; construction gates passed, behavioral execution tracked in M13/M14 |
| M02 | Shared identity/mutation operations and analysis invalidation | M01 | complete | 875466b8; construction gates passed; behavioral execution M13 |
| M03 | Clipboard, drag/drop, folding and step annotations | M02 | complete | 401933ca; construction gates passed; behavioral execution M13 |
| M04 | Typed field visitors and navigation/search/replace/outline | M03 | complete | cd9969db; construction gates passed; behavioral execution M13 |
| M05 | Central reusable/static validation and program compilation | M01, M04 | complete | construction gates passed; behavioral execution M13/M14; commit recorded next milestone |
| M06 | Executor frame ownership with existing behavior parity | M05 | pending | |
| M07 | Nested calls, typed invocation and Return execution | M06 | pending | |
| M08 | Frame-aware debug events and Runtime Inspector | M07 | pending | |
| M09 | Shared direct invocation preparation and parameter prompts | M07, M08 | pending | |
| M10 | Signature, Call and Return authoring | M05, M09 | pending | |
| M11 | Versioned package export and transactional import | M02, M05, M10 | pending | |
| M12 | Library/template workflows and user help | M11 | pending | |
| M13 | Model/editor/analysis/package regression coverage | M12 | pending | |
| M14 | Runtime/debug/invocation regression coverage and focused verification | M13 | pending | |
| M15 | Independent review, remediation and authoritative full verification | M14 | pending | |

## Verified baseline architecture and required migrations

- `src/mkmacro/model.rs`: schema 11; `MkMacro` and `MkStep` have no signature/metadata grouping. Values already use tagged serde in `variables.rs`; `Null` remains runtime-only. Model action enum matches are widespread in executor, validation, recorder, action catalog/editor and tests.
- `store.rs`: `read_document` applies migrations through `migrate_v10_to_v11`, then serde defaults, version update and `repair_ids`; `probe_document` is a separate read-only health path. `repair_ids` enforces document-local macro IDs and **per-macro** step uniqueness; folder IDs form a separate namespace. Its allocation scans maxima and handles overflow. It currently repairs duplicate IDs without graph context, which is unsafe to reuse blindly for new signature bindings.
- `structure.rs`: tolerant `analyze_structure`, `StructureAnalysis::block_for_marker`, `containing_block`, `StructuralBlock`, `delete_block`, and `unwrap_block`. No shared clipboard/move normalization yet.
- `gui/mkmacro_dialog/step_table.rs`: `expanded_move_ids`, `delete_selection`, `move_selection_structurally` privately duplicate structural ownership. `move_steps` swaps raw rows; structural movement preserves block parent identity. `duplicate_steps_with_ids` copies only raw selected rows, builds a synthetic document and repairs zero IDs. Public duplicate/move functions are reexported from dialog `mod.rs` and used by tests. M02 migrates these paths to a single engine; thin compatibility forwards are permissible only for actual public callers, never a second implementation.
- `Selection` stores stable selected IDs but an index anchor and no explicit stable primary row. Dragging/folding require a stable primary/anchor contract. `MkMacroDialog` stores the entire draft and baseline; `mark_dirty` currently sets only a bool. Save, reload, external sync, recorder append, property edits, and modal apply all need analysis invalidation.
- `step_table::show` currently enumerates monitors and invokes full document validation with asset root every frame. Do not add call graph/variable/asset work to this path. Replace it with revision-based cached diagnostics, structure, navigation, and explicit environment refresh. Existing `action_editor` variable inventory also rebuilds each frame; migrate new shared data to revision/consumer-position keys.
- `validation.rs`: already has `DiagnosticSeverity::{Warning,Fatal}`, stable diagnostic code, macro ID, optional step ID, and `can_run` blocking Fatal only. Preserve this public severity naming; no gratuitous Error/Fatal rename. Store publishes cached validation on document publication, while editor currently recomputes separately.
- `compiler.rs`: `compile(&MkMacro)` validates a synthetic singleton document, constructs `MkExecutionPlan` with playback, `Arc<MkStep>` instructions, jumps and step lookup. Extract validated plan lowering before document-aware compilation; calling `compile(callee)` as-is would incorrectly reject external Call targets. Keep single-macro API for ordinary tests.
- `executor.rs`: `Executor::execute` owns locals, PC, loop state, retries/repetition, transitions, one `InputCleanupGuard`, one `RunActivityGuard`, debug safe snapshots, and action dispatch. Existing `ExecutionEvent` variants have step identity alone. `MAX_CONTROL_TRANSITIONS`, cancellation checks, debug-only variable publication, retry timing and one-before-repetitions breakpoints are behavior to preserve.
- `runtime.rs`: one worker, command enum, admission/control, immutable runtime snapshots. `run_one` gets one store snapshot, compiles root, then slices root instructions for Run From/Selection. Structural slices are rejected. `macro_id` is root identity; step maps and failure keys currently assume one macro. `MacroRuntime::command` and global `run/debug_run/run_from/run_selection` APIs have many callers/tests.
- Direct entry points: dialog `prepare_execution*` and `run/debug*` helpers; `commands/headless.rs` `MacroCommand::MkRun`; `mkmacro/hotkeys.rs` dispatch closure calling global `runtime::run`. Plugin only produces ID-based launcher actions. Hotkey scope is dispatch-only and must not become callee execution authorization.
- Existing GUI request pattern: `mkmacro/prompt.rs`, `launcher_command.rs`, `launcher_query.rs` brokers call registered repaint callbacks and are drained in `gui/render.rs`. Existing PromptInput broker synchronously waits on the active runtime; do not use its blocking wait for pre-run parameters. Share a typed nonblocking invocation-request boundary and one reusable GUI dialog.
- `store.rs` owns safe direct-child PNG paths, decoding, collision handling, `asset_authoring` and document `transaction` locks; `common/atomic_file.rs::save_atomic` publishes atomically. Package code must call store-owned operations and hold consistent lock order. Store has an existing JSON watcher; this initiative adds no watcher.
- Existing test targets include `mkmacro_authoring`, `mkmacro_compiler`, `mkmacro_runtime`, `mkmacro_store`, `mkmacro_visual`, `mkmacro_recorder`, `mkmacro_plugin`, `mkmacro_launcher_integration`, plus many module tests and aggregated suites. Extend these; add no new integration binary.

## Cross-cutting decisions and invariants

1. **Schema:** one bump, 11 -> 12. Defaults preserve every historical action, breakpoint, image filename, folder, hotkey and playback field. Existing migration tests stay. Group authoring fields under metadata; fixed snake_case accent palette only. No extra MkValue variants, arbitrary colors, expression syntax, globals, recursive macros or debugger stepping features.
2. **Identity:** stable u64 macro IDs; stable step IDs scoped to macro; one per-macro signature ID namespace for parameters plus outputs. Allocation is checked/deterministic and shared, not name/index-based. Signature rename/reorder retains ID. Dangling and duplicate nonzero signature IDs produce diagnostics; never guess a new binding. Fresh authoring definitions get IDs explicitly. Templates/import allocate fresh IDs and remap all relationships; steps copied within/cross macros get fresh destination IDs. Pixel `search_id` is a separate semantic identity, not automatically a step ID: inspect and preserve/remap its producer-consumer relationships deliberately, never mechanically rewrite all numbers.
3. **Runtime identity:** preserve `RuntimeSnapshot.macro_id` as root for compatibility and add explicit active macro identity. Use `MacroStepKey { macro_id, step_id }` for cross-macro status/outcome/failure data (failure key also run ID); stack frames carry invocation/depth/caller step information. A repeated call is a fresh frame; no simultaneous recursive instance is permitted. Existing root-only maps may be retained as explicit root projections if necessary for public API compatibility, but new UI must use authoritative compound data. No ambiguity or competing updates.
4. **Invocation:** root arguments keyed by parameter ID; defaults applied centrally; type checking uses one `MkValueType` helper. Variable sources may reference built-ins through a read-reference validator, whereas assignments must use `validate_variable_name` and reject built-ins. String literals use existing interpolation once at caller resolution; defaults do not gain caller local access. Callee locals are initialized only with parameters and callee built-ins.
5. **Execution:** immutable root dependency closure built once from one document snapshot, O(1) Call lookup, no store reads at Call, no ordinary-step macro lookup. One worker/control/input guard and root run ID. Callee playback remains its own. Per-call repetitions and retries create fresh locals and start from first callee instruction. Output mapping is prepared/type-checked completely before any caller variable write. Cancellation always unwinds root, irrespective of Continue/Retry. Guard cleanup happens at root termination; callee Return/failure does not release caller-owned input.
6. **Validation:** warnings never block ordinary macros. Document diagnostics show unrelated invalid macros, but root compilation blocks only relevant closure failures and global identity ambiguity. Disabled/dangling targets and cycles on included enabled call edges are fatal; disabled ordinary actions do not execute. Explicitly preserve current validation treatment of authored disabled rows unless new behavior requires stricter call safety; graph/closure and runtime must agree on which calls are executable. Return with outputs requires complete compatible output sets at every successful exit. Branch/loop analysis is conservative: definite violations fatal where execution cannot safely succeed, uncertain variable facts warning. Full reachability must not claim steps after a conditional Return are all unreachable.
7. **Editor:** normalized structural units are the only Copy/Cut/Paste/Duplicate/Delete/Move/Drag mutation boundary. Atomic candidate validation compares structure and parent/marker identities, not merely count of diagnostics. Malformed selected marker errors leave state untouched. Moving preserves IDs; cloning replaces IDs. Clipboard/folds/search/outline state is session-only and never dirties document. TextEdit owns its shortcuts. A single stable-ID navigation helper expands ancestors/selects/scrolls/focuses.
8. **Replacement:** exhaustive typed editable-field visitor, never serde JSON search/replace. It traverses nested conditions, coordinate/window matchers and new value sources. Replacing an image filename constructs a validated `MkImageRef`. Editing a migrated launcher query clears `legacy_resolved_action`, matching existing action-editor semantics; compatibility payload is not independently rewritten. Transaction preview records revision and field identity; stale preview must rebuild/reject. Candidate errors caused by replacement reject application atomically; pre-existing unrelated draft diagnostics must not make harmless label edits impossible.
9. **Caching:** draft revision increments once per actual mutation; presentation changes do not. Document-wide semantic diagnostics/call graph and selected macro search/outline are invalidated by revision, including external replacement. Environment checks refresh on open/explicit refresh/asset authoring/save-run, never filesystem or monitor scan in idle frames. Provide a clear refresh path for externally changed assets.
10. **Packages:** use a versioned `.mkmacro` JSON envelope with typed manifest and base64 PNG bytes using existing serde_json/base64 dependencies. This is the specification's permitted simpler encoding; no ZIP dependency. Manifest has roots, macros, relevant folders, dependencies, asset filenames and format version. Enforce input-size/count/decoded-byte limits and PNG validation before writes. Deterministic traversal/order, no absolute path metadata, all image references direct filenames. Libraries are multi-root packages copied into ordinary local macros. Templates are versioned records containing this package in `mkmacro_templates.json`, independent copies, no watcher/live link.
11. **Import transaction:** parse/validate/plan without mutations; remap fresh macro/step/signature/folder IDs and references, deterministic unique names, same-name identical images reused and differing images safely renamed (including Windows case collisions). At apply revalidate expected document/asset state under store transaction/asset locks; stage new assets, publish document atomically, publish snapshot last. Roll back only newly created owned files on failure; never delete reused/user assets. Account for store watcher publication and concurrent save/import; injected failure must prove no partial state. Template creation uses the same package/identity machinery, not a second serializer.

## M01 — Persisted model and schema 12

**Goal / spec:** numbered items 1-5; establish all persisted types atomically before UI/runtime. Suggested commit `feat(mkmacro): add reusable macro authoring model`.

**Ownership/files:** `mkmacro/model.rs`, `variables.rs`, `store.rs`, `mod.rs`, `persistence/catalog.rs`; exhaustive match sites in executor, validation, GUI catalog/editor, recorder and test constructors only as required for compilation.

**Changes:** add `MkStepMetadata`, palette enum, signature/parameter/output types, `MkValueType`, `MkValueSource`, Call argument/output and Return bindings. Serde default metadata/signature; stable snake_case variants. Central compatibility helpers and checked signature allocation. Schema migration preserves image v11 work; `read_document`/`probe_document` recognize v12 accurately. Update common test literals with explicit defaults. Calls/Returns are persisted but **not yet exposed in catalog**. Central validation reports temporary `unsupported_reusable_action` Fatal and executor returns structured unsupported diagnostics; no silent no-op. Remove this temporary guard only when M07 delivers execution.

**Acceptance:** old document loads and saves v12 deterministically; fresh types round-trip by contract; existing parameterless behavior unchanged; deleted signature binding cannot be retargeted by repair. Compilation includes all variants.

**Construction gate:** touched formatting, `cargo check`, `cargo check --tests` once if constructor changes are extensive, `git diff --check`.

**Late tests / migrations:** M13 covers spec 38; retain prior schema fixture chains and persistence health tests, add v11 fixture including breakpoints/folders/images/playback. Update catalog completeness expectations without advertising unsupported variants. Risk: broad MkStep/MkMacro literals, old schema health probe accepting malformed v11, duplicate-ID repair.

## M02 — Shared identity/mutation operations and analysis invalidation

**Goal / spec:** 6-7 and foundational parts of 9/12/37/performance. Suggested commit `refactor(mkmacro): centralize structured editor mutations`.

**Ownership/files:** extend `structure.rs`; add focused `mkmacro/editor_mutation.rs` and reusable identity helper if needed; dialog `step_table.rs`, `mod.rs`, mutation call sites, analysis cache module.

**Changes:** typed normalized selection with ordered non-overlapping units, stable-ID insertion anchors/boundaries, typed mutation errors/results, canonical fragment clone (steps/remap/inserted IDs), delete and structural move legality. Migrate Duplicate, Move Up/Down, Delete, block deletion/unwrap to shared analysis without separate nesting rules. Keep existing valid movement behavior including noncontiguous order and parent preservation. Replace synthetic-document clone path. Introduce stable primary/selection anchor and dialog revision/cache ownership; migrate `mark_dirty`, save/reload/external sync and direct mutations to invalidate once. Cache existing validation so subsequent full analyses are never added to per-frame path.

**Acceptance:** selecting any complete marker includes entire block once; nested overlap deduplicates; invalid marker produces typed error and no mutation; moved IDs preserved, cloned IDs fresh even after overflow/high IDs; no stale selection anchor after reorder. Existing deletion fallback selection remains deterministic. No full document validation/asset scan/monitor enumeration in idle table rendering.

**Construction gate:** formatting, `cargo check`, diff review of all old helper callers; focused pure mutation test only if resolving ambiguous movement behavior.

**Late tests / migrations:** M13 spec 39-40; move step_table private-helper tests to new owner or forward behavioral assertions; preserve `movement_*`, `malformed_movement_is_transactional_and_does_not_dirty_the_macro`, Delete/unwrap tests. Add cache generation tests including external sync, save/repair, no-op and metadata mutation. Risk: repeated mark_dirty calls, public draft mutation test helpers, malformed drafts with equal diagnostic counts.

## M03 — Clipboard, drag/drop, folding and annotations

**Goal / spec:** 8-11. Suggested commit `feat(mkmacro): add structured clipboard and step organization`.

**Ownership/files:** focused dialog `editor_operations.rs`/`folding.rs` as useful, `step_table.rs`, `action_editor.rs`, `mod.rs`, toolbar/context menu.

**Changes:** process-local fragment clipboard shared across macro selection; Copy, Cut (prepare then remove), Paste, Ctrl+D on canonical clone; keyboard ownership guards integrate existing modal/text-input/pointer gates. Paste after normalized primary unit or end, select all inserted with first primary and scroll request. Drag stores selected IDs (or selects unselected dragged unit), computes current valid boundaries without mutation, insertion indicator, reject or unambiguously snap invalid drop. Preserve multi-selection and primary. Fold complete opener IDs, retain child folds, derive visible rows, keep hidden selection IDs and visible primary. Add label/comment/accent/bookmark controls and compact indicators/tooltips via existing transactional action editor.

**Acceptance:** Windows clipboard untouched; TextEdit Ctrl+C/X/V/D unaffected; Cut errors leave source and dirty unchanged; cross-macro Paste succeeds with fresh IDs; drag does not mutate until drop and cannot orphan structure; nested If includes Else under root fold; folds/session state never serialize/dirty; metadata persists and remains execution-neutral.

**Construction gate:** touched formatting and `cargo check`; inspect shortcut/modal gates and ID anchoring; no GUI automation claim without actual interaction.

**Late tests / migrations:** M13 remaining 39-41 clipboard/fold cases, keyboard routing pure helper tests, existing viewport/selection/context-menu tests. Risk: egui Copy/Cut/Paste events differ from raw key events, table virtualization changes visual vs source indices, hidden primary row.

## M04 — Typed fields and Find/Replace/Jump/Outline

**Goal / spec:** 12-16; build reusable field/data analysis for later diagnostics. Suggested commit `feat(mkmacro): add searchable outline and safe replacement`.

**Ownership/files:** `mkmacro/authoring_fields.rs` (or equivalent), focused dialog `navigation.rs`, `search.rs`, `outline.rs`, `step_table.rs`, `mod.rs`, catalog detail reuse. Existing variable inventory may move domain facts out of GUI here or M05.

**Changes:** explicit typed field identifiers/visitor cover legitimate editable strings across all current actions/nested conditions and metadata, variable reads/writes, Call/Return sources, image references. Derive searchable rows including action type/details, stable ID/source row, label/comment/bookmark and dynamically resolved Call name. One navigation request expands ancestors/selects/scrolls/focuses. Ctrl+F current macro, count/next/previous and F3/Enter; Ctrl+G searchable palette; Ctrl+H typed field preview with old/new, field, row and macro, Replace Current/All/Cancel. Candidate validation/apply is one transaction. Right collapsible Outline preserves left macro/folder sidebar and shows structure/Else/calls/returns/bookmarked/labeled rows with hierarchy, optional all rows.

**Acceptance:** no serde text searching/replacing; IDs/enums/numerics/legacy metadata unchanged; valid image replacements constructed through type boundary; changed launcher query clears legacy compatibility payload; stale preview safe. Find/Jump/Outline navigate hidden targets and never dirty; Replace dirties once only on changed candidate; no expensive rebuild with unchanged revision/query.

**Construction gate:** `cargo check`, formatting, exhaustive action visitor review. Search no-match/empty-query behavior explicit (empty replacement query rejected).

**Late tests / migrations:** M13 spec 41; visitor coverage for text, window/process/path/UIA/image/notification/prompt/query, nested conditions, interpolation references and unsupported fields; preview failure/staleness; outline hierarchy and cache invalidation. Existing exact `action_details` tests retained. Risk: derived text search must not imply every summary word is replaceable; replacement variable identifiers must obey built-in assignment rules.

## M05 — Validation, dependency graph and compiled program

**Goal / spec:** 17-22, 37. Suggested commit `feat(mkmacro): validate reusable signatures and compile dependencies`.

**Ownership/files:** `validation.rs`, focused `call_graph.rs`/`authoring_analysis.rs`, `compiler.rs`, `variables.rs`/`interpolation.rs` shared read parsing, GUI `variable_catalog.rs`, cached diagnostic UI/navigation.

**Changes:** preserve Fatal/Warning; central signature uniqueness/names/default types, Return completeness/type and call bindings/target enabled checks. Share type and variable facts with authoring suggestions instead of copying current GUI `VariableValueType` matching. Explicit iterative DFS/colors or bounded graph algorithm with readable cycle paths and deterministic closure. Conservative control-flow facts for unused locals, reads-before-definitions, unreachable after Return/Break/Continue and duplicate labels. Build immutable `MkCompiledProgram` root ID + O(1) plans/signatures/name metadata from reachable macros only; extract single-macro validated lowering and preserve `compile`. No flattening and no per-call disk work. Keep M01 temporary runtime support guard until M07; semantic graph/closure/lowering is separately testable, root-only compile_program works, reusable program admission may remain guarded during construction. Diagnostic panel and row markers navigate through M04; macro-wide findings visible.

**Acceptance:** renamed/reordered signature binds by ID; deleted IDs remain visible errors; required defaults accepted; no duplicate/missing/dangling mapping silently accepted; built-in reads allowed and assignments rejected. A->B->C cycle path useful; unreferenced invalid macro does not unnecessarily block valid root program, while ambiguous macro IDs do. Warnings alone runnable. Branch analysis does not fabricate definite execution facts. All cached diagnostics refresh after dependency edit.

**Construction gate:** formatting, `cargo check`, review compile synthetic-singleton removal from document lowering; focused graph/CFG tests when needed, broad tests deferred.

**Late tests / migrations:** M13 spec 42-43 and existing compiler flow tests; keep single macro tests simple. Add root/closure/excluded unrelated plan, own playback/breakpoint metadata, deleted/disabled target, all binding validation and variable-flow cases. Risk: current is_structural semantics include Break/Continue; Return/Call must not accidentally inherit marker retry/selection restrictions. Preserve existing disabled-row validation semantics and document executable-edge decisions.

**Read-only M05 handoff after M03:** current validation checks authored rows even when disabled; retain payload/binding diagnostics, but executable dependency/cycle edges come from enabled Call rows. Disabled block openers skip only the opener, not the body (`disabled_compiled_structural_openers_skip_without_breaking_or_evaluating`); disabled Return and failure-with-Continue cannot prove termination. Extract validated lowering from singleton `compile` instead of calling it per callee or calling compilation from validation. Root admission must include closure findings and global macro-ID ambiguity; migrate GUI `playback_block_reason` as well as compiler admission, retaining document-wide diagnostics display. Move producer/type/shadow facts from `variable_catalog` into the domain, including real parameter sources, Unset, disabled writes, UI reads and document-aware Call outputs. Keep interpolation reads distinct from editable fields: SetVariable strings are literal, metadata has no reads, `$${name}` is escaped, substitutions are nonrecursive, and historical exact Unicode read keys remain supported. Keep cache diagnostic order/content deterministic for semantic/environment subtraction. Planning agent performed read-only investigation; no additional tests ran.

Diagnostic provenance: zero/duplicate macro IDs must form explicit document-global admission failures, without relying on a magic macro ID or message matching. Other signature/action findings retain owner macro/step identity. Missing/disabled targets are findings on the caller's Call step with the target identity retained; filtering only by target ownership would incorrectly omit a failing caller edge.

## M06 — Executor frame ownership with behavior parity

**Goal / spec:** foundational 23/26/27. Suggested commit `refactor(mkmacro): isolate executor frame and root run state`.

**Ownership/files:** `executor.rs`, optional private `executor/frame.rs`/execution helper module, runtime options only as necessary.

**Changes:** separate root execution resources from per-frame PC, locals, loop/repeat state and safe-boundary snapshots. One root session owns input guard, control, transition counter, waiter/backends and activity lifecycle. Frame continuation carries instruction repetition/attempt and eventual child-return state needed by M07. Reuse one instruction/error/pacing path rather than retaining original loop beside new implementation. Existing `execute(plan, options, observer)` runs one frame through shared engine; new program entry can be prepared but calls remain unsupported until M07. Minimize ordinary step overhead and variable clones beyond existing behavior; no hash lookup per noncall action.

**Acceptance:** ordinary macros preserve action order, loop jumps, disabled skips, retry delay unscaled, per-step pacing, breakpoint once before repetitions, debug-only snapshot boundaries, stop wakeups and exact owned-input cleanup. Control transition limit belongs to root session and cannot reset across later calls. No child control or cleanup guard.

**Construction gate:** `cargo check`; strategically execute narrow existing executor tests for sequential input cleanup, unowned input and compiled nested control if refactor behavior is uncertain. Record any deferred runtime suite explicitly.

**Late tests / migrations:** M14 existing executor notification/screenshot/interpolation/window/input tests and debugger ordering; preserve behavioral assertions and avoid broad event changes before M08. Risk: borrow-driven duplicated loops, guard dropped during frame pop, condition failures bypassing existing policy, incorrect disabled-marker behavior.

**Read-only M06 handoff after M03:** move plan reference, PC, locals, loop counters, safe variables/step and resumable instruction repetition/attempt/phase into the frame. Keep input/activity guards, control, transition budget, observer/options/backends/waiter/samplers root-owned. Existing `Executor::action` already takes frame macro ID/playback/variables plus the input guard; retain that effect boundary. Preserve pause-before-observer ordering (observer may immediately resume/stop), one breakpoint per instruction before repetitions, disabled rows counting transitions but never updating safe state, unscaled retry backoff, successful-repeat pacing, and StepOutcome before StepFinished. StepFinished currently precedes If/While condition evaluation; publish the safe boundary only after condition/jump success. Fatal/cancelled errors retain prior safe state; Continue errors publish settled failed-step variables. No additional variable clones for Normal runs. Preserve input-guard-before-activity teardown even if moved into fields. Existing `runtime::run_one` lacks a panic boundary and manually releases admission/operation ownership after the result: M07/M14 must cover the requested panic cleanup and worker lifecycle; M06 remains a behavior-preserving extraction. Planner confirmed milestone ordering is viable; no tests ran for this investigation.

## M07 — Typed invocation, nested Call and Return

**Goal / spec:** 23-27 plus runtime integration of 22. Suggested commit `feat(mkmacro): execute reusable macros in isolated call frames`.

**Ownership/files:** `mkmacro/invocation.rs`, `executor.rs` and frame module, `runtime.rs`, `compiler.rs`, remove temporary validation support guard; model exports. Do not expose new catalog until M10.

**Changes:** typed root invocation with stable-ID arguments and mode/subset selection options, empty-argument compatibility commands. Runtime takes one document snapshot, resolves dependency program, applies root-only subset behavior while retaining callee full plans. Preserve Run From/Selected rejection for structured roots exactly; do not expand this feature to arbitrary structural slice execution. Full callee never inherits root subset. Root signature/default validation applies to all invocation modes. Stack state machine pushes fresh callee per Call attempt/repetition, resolves literal/variable sources with existing interpolation and centralized types, initializes current macro.id/name, applies callee playback, shares control. Return prepares complete typed output set, exits only current frame, root Return succeeds; procedure fallthrough succeeds, output fallthrough errors. Child failure becomes caller Call failure honoring Stop/Continue/Retry; fresh retry restarts target, partial outputs discarded. Structured depth error at >64 frames even for unvalidated input. Root stop unwinds all frames; output-map commit only after every mapping validated.

**Acceptance:** A-before/B/A-after and A->B->C correct; locals isolated, explicit outputs only; default parameters and required/type errors defended at runtime; target disabled/missing handled; call-step repeat fresh; callee playback, cancellation, root Return and natural end correct. No nested worker/global runtime calls or new RunControl. Ordinary facade callers remain functional. M01 unsupported guard removed only here when all cases executable.

**Construction gate:** `cargo check`, formatting, focused fake-backend call/locals/retry/cleanup tests if necessary; review every continue/retry/cancel branch. Do not wait until late suite to knowingly accept broken runtime.

**Late tests / migrations:** M14 spec 44 and invocation primitives from 46; regression Run/Debug From/Selection command pairs and save/remapped selection preparation. Test separate macros using same step ID. Risk: retry child repeats leaked state, partial outputs on failure, output-producing subset fallthrough, child cancellation swallowed by caller Continue, current builtin identity restoration.

## M08 — Debugger and Runtime Inspector call identity

**Goal / spec:** 28-29. Suggested commit `feat(mkmacro): publish frame-aware debug snapshots`.

**Ownership/files:** executor events, runtime snapshot/state/diagnostic keys, dialog `runtime_inspector.rs`, `step_table.rs`, `toolbar.rs`, `mod.rs` snapshot observation.

**Changes:** add explicit immutable frame stack snapshots with macro ID/name, caller step, active step/depth and root/active identity. Compound macro-step status/outcome/failure keys; preserve documented legacy root identity/projections where needed. Breakpoint/safe-boundary events identify current frame; active variables only, no variable history. Frame push/pop restores correct active state and caller pending Call identity. Runtime Inspector shows Call Stack for depth >1 with active row; step status/highlight for currently displayed macro must not show same-numbered row in wrong macro. Auto-open breakpoint occurrence key includes macro/frame identity. Normal runs ignore breakpoints and do not allocate debug variable snapshots.

**Acceptance:** root/direct/nested callee breakpoint pauses one root run, publishes callee locals and stack, resume same frame, stop entire stack. Existing root debug events and current-run vs retained-snapshot behavior preserved. No mutable frame references exposed and no runtime state serialization.

**Construction gate:** `cargo check`, formatting, inspect event observer and map update completeness.

**Late tests / migrations:** M14 spec 45 plus existing `breakpoint_*`, `debug_variable_*`, Runtime Inspector lifecycle and revision tests. Update event shape/compound-key assertions, preserving ordering/safe snapshot assertions. Risk: ambiguous root macro_id use in toolbar/editor, colliding step IDs, repeated breakpoint suppression from old `(run,step)` key, normal-run debug data leakage.

## M09 — Direct invocation preparation and parameter prompts

**Goal / spec:** 30. Suggested commit `feat(mkmacro): prompt for direct invocation parameters`.

**Ownership/files:** invocation preparation/broker, `runtime.rs` global run facades, `gui/mkmacro_dialog/parameter_prompt.rs`, `gui/render.rs`/`mod.rs`, dialog execution helpers, hotkey and `commands/headless.rs` integration; retain plugin action strings.

**Changes:** one preparation result applies defaults, validates supplied stable IDs/types and reports missing required definitions. All six editor modes, launcher MkRun and hotkey use it. Complete/default-only invocation submits immediately; missing parameters enqueue nonblocking typed UI request with mode/root subset context and repaint callback. One reusable Number/String/Boolean/Point dialog shows name/type/description/default; validate Run, Cancel submits nothing. Preserve requested Debug/selection/from semantics through prompt. Pending requests must not silently overwrite; report busy/duplicate or bounded queue explicitly. Revalidate target/signature and active runtime on confirmation; reject stale removed/type-changed definitions without rebinding by name. Do not reserve an executing run/hold hotkey thread while user considers input; error without GUI availability rather than executing Nulls.

**Acceptance:** all entry points behave identically for defaults/missing/types; no undocumented argument string syntax; cancelling/closing prompt causes no runtime request. Hotkey scope still governs dispatch only, callee calls bypass scope appropriately. Worker does not block on egui pre-run prompt. Existing PromptInput action behavior unchanged.

**Construction gate:** `cargo check`, formatting, read every direct `runtime::run` caller; no broad launcher parser refactor.

**Late tests / migrations:** M14 spec 46 plus hotkey/launcher integration and admission tests; fake prompt request/submit/cancel/stale/busy/unavailable UI paths. Risk: command facade success means queued prompt (document result semantics), hidden GUI viewport, runtime starts between preparation and submit, draft save changes selection IDs.

## M10 — Signature, Call and Return authoring

**Goal / spec:** 31 and finish 37. Suggested commit `feat(mkmacro): add reusable signature and binding editors`.

**Ownership/files:** `macro_properties.rs`, action catalog/editor, focused `call_editor.rs`/signature widgets, `variable_catalog.rs`, shared typed-value UI used by parameter prompt, navigation and diagnostics.

**Changes:** Parameters/Outputs add/delete/move/rename/type/description/default controls preserve IDs; type change never silently coerces incompatible existing data. Call/Return visible in existing appropriate category with runtime capability audit updated. Searchable ID-based macro target selector with name/folder/description and self disabled; argument typed literal/variable with context suggestions (including root parameters, earlier call outputs); optional output-to-caller-variable mapping. Render obsolete argument/output bindings explicitly and require explicit removal. Target Open action navigates without losing draft. Return edits current signature output ID values; no-output Return explanatory text. Signature editing and binding validation use central diagnostics.

**Acceptance:** full reusable macro authoring possible; rename/reorder preserves bindings; definition deletion shows repairable obsolete section; disabled/missing target displayed with diagnostics; variable suggestions respect current caller scope. Call target display refreshes on rename via revision; no name-based authority. Modal cancel unchanged document.

**Construction gate:** formatting, `cargo check`, action capability/catalog exhaustive review.

**Late tests / migrations:** M13/M14 catalog visible/name/detail/EditorKind audits, transactional action editor tests, stable signature evolution, Return no-output and stale binding UI model. Risk: modal lacks current macro/document context and borrows stale cloned signature, default placeholder action not compilable until edited (existing catalog tests need behavior-specific setup rather than weaker assertions).

## M11 — Package export and transactional import

**Goal / spec:** 33-35 and core 36. Suggested commit `feat(mkmacro): add dependency-aware macro packages`.

**Ownership/files:** new `mkmacro/package.rs`, store transaction helpers, typed image traversal shared with model visitors, `mod.rs`, persistence catalog only where required to recognize later template file.

**Changes:** typed versioned JSON+base64 package, one/multi-root export, exact transitive macro closure, relevant folders, exact PNG references (nested conditions/coordinate image targets included); deterministic order/dedup and clean missing asset error. Parse bounded data and reject unsupported versions/malformed manifests/duplicate IDs/traversal; validate before state changes. Import plan holds expected baseline, all macro/step/signature/folder maps, renamed names and assets, rewrite every Call argument/output and Return binding using owner-target maps. Fresh imports never overwrite local macros. Same-name identical assets reused, differing bytes renamed with safe deterministic suffixes and references rewritten. Stage/commit/rollback in store owner with no new background scans. Plan summary carries added/renamed/dependency/image counts/warnings. Save atomic document only after assets available; publish snapshot only on success.

**Acceptance:** root A exports only A/B/C and required PNGs; library multi-root uses same format. Import into conflicting local IDs/names/assets remains internally correct, no path escape/silent overwrite, no partial document/assets on injected failure. Existing store watcher cannot publish stale preimport state; expected-state mismatch replans/rejects instead of overwriting edits. Lock order documented. Case-insensitive Windows collisions and symlink safety reused.

**Construction gate:** `cargo check`, touched formatting, manually review transactional failure branches; strategically run injected rollback test if needed before accepting data-safety boundary.

**Late tests / migrations:** M13 spec 47 in existing store target/module tests, unchanged v10/11 asset migration tests, atomic failure injection; no new binary/dependency. Risk: save repairs IDs after package remap (avoid double remapping), lock recursion, store save failure after asset publication, oversized base64 allocation, asset paths in dormant nested fields.

**Read-only M11 handoff after M04:** store `Inner::transaction` serializes save/reload/publication and `asset_authoring` serializes PNG writes; batch import belongs inside the store with documented transaction-then-asset lock order and private lock-held helpers (public save/write calls would re-lock). `publish` computes asset-aware diagnostics before replacing snapshot, so publish assets first and snapshot last; existing watcher remains sufficient. Validate original PNG bytes once through store containment/decode helpers; `validate_image_ref` returns decoded pixels, while `image_refs()` enumerates unrelated filesystem assets and is not dependency discovery. Reuse M04 Image-kind field traversal for exact discovery/remapping. Use existing case-insensitive migration naming and byte-identical reuse logic, but retain ownership/rollback through document persistence: migration's current rollback ends too early. Add stale document/disk/asset checks under locks, verify no post-remap ID repair changes, and an injected fallible checkpoint after earlier asset publication/during document persistence. Existing `before_publication` hook is after persistence and cannot simulate those failures. Preserve watcher transaction, root-containment, PNG-limit, migration rollback and atomic-file failure tests. Templates need their own catalog/probe/reset entry in M12; embedded assets need not join the existing document/assets recovery group.

Parent verified the locked existing `tempfile` 3.21.0 implementation provides `NamedTempFile::persist_noclobber`; Windows publication omits `MOVEFILE_REPLACE_EXISTING` for this path. This can supply staged create-only publication without a dependency or new raw Windows API. Export/template closure must include every authored Call target, including disabled rows, because import remaps every persisted relationship; keep that traversal policy distinct from executable enabled-edge closure and reject cycles in exported authored graphs.

## M12 — Libraries, user templates and help

**Goal / spec:** 32, 36 and documentation phase 18. Suggested commit `feat(mkmacro): add macro library and template workflows`.

**Ownership/files:** `mkmacro/templates.rs` using package/store, focused dialog `package_ui.rs`/template UI, toolbar/macro list/properties as appropriate, `persistence/catalog.rs` for user-authored template backup/recovery registration, `README.md` and actual help content owner (`help_window.rs` or MkMacro dialog help).

**Changes:** export selected macro or explicit multi-root library via existing rfd file UI; import read/plan preview then explicit apply with summary, preserve dirty/conflict draft using existing save/close conventions (do not discard). Save Selected Macro as Template creates versioned independent package record with name/description/ID; New from Template reuses import remap including dependency macros, new signature/step IDs and unique names. Template persistence atomic in same data directory, no watcher/live links. Template-created hotkeys should be cleared, matching current macro duplication safety, so copied library dependencies do not silently install conflicting hotkeys; imported authored hotkeys retained only with surfaced conflict diagnostics/explicit policy in preview. Document policy consistently. Add help for C/X/V/D/F/H/G, drag/drop, folds/metadata/bookmark, parameters/calls/returns, explicit local scope and mapping, rename stability, recursion prohibition, package/library/template copy semantics.

**Acceptance:** all package/template domain paths reachable from user UI; templates survive reload and remain isolated from source/instances; dependencies remap deterministically; import plan cancellation does not save/mutate anything unexpectedly. User sees conflict/rename/asset summary before applying. Data catalog includes new user-authored template storage if catalog is the repository backup boundary. No duplicated format, live links or new workers.

**Construction gate:** formatting, `cargo check`, review help vs actual UI shortcut semantics and persistence catalog entry.

**Late tests / migrations:** M13 spec 48, package UI preparation/cancel models, template storage/probe recovery tests, existing macro duplication clears-only-hotkey invariant. Risk: template save uses unsaved root but persisted stale dependency, toolbar operations silently overwrite dirty draft, path rooted to wrong data directory.

Parent documentation inventory after M04: no existing MkMacro-specific Markdown guide or static section in `help_window.rs` was found. General help currently renders plugin descriptions/commands; `plugins/mkmacro.rs` exposes command descriptions, and the dialog toolbar supplies action tooltips. Add focused user help at the actual chosen UI/documentation owner and link it from README rather than assuming an existing detailed MkMacro guide.

## M13 — Model/editor/validation/package coverage

**Goal / spec:** complete 38-43, 47-48 and editor/persistence integration tests; authoritative late coverage for M01-M05/M10-M12. Suggested commit `test(mkmacro): cover authoring and package compatibility`.

**Ownership/files:** existing module tests, `tests/mkmacro_authoring.rs`, `mkmacro_compiler.rs`, `mkmacro_store.rs`, fixtures, aggregated persistence coverage. Implementer may fix source defects revealed in these scopes; report meaningful changes, do not weaken tests.

**Required matrix:**

- Migration v11 ->12 all fields preserved, defaults, metadata/signatures/Call/Return roundtrips, stable IDs/types; earlier migrations retained.
- Copy ordinary/If opener/Else/EndIf/Repeat and While boundaries/nested/parent+child/noncontiguous/malformed. Copy/Cut/Paste same/cross macro middle/end; fresh IDs and metadata; movement keeps IDs/order; invalid move/drop atomic. Overflow and destination conflicts.
- Folding nested visibility and remembered child states; navigation expands ancestors; TextEdit/modal clipboard gates; primary selection behavior; no dirty from presentation; no expensive idle cache rebuild.
- Find each field family and Call target/bookmark; replace supported/unsupported/current/all/empty query/stale preview/failure; launcher legacy clearing; typed image filenames; outline hierarchy/right-panel state.
- Signature duplicate ID/name/type/default/built-in assignments, all call/return invalid bindings and missing targets, direct/indirect cycle and readable path. Conservative unused/read-before/after-control warnings and output-return fallthrough; warning-only run allowed.
- Program root-only/transitive/exclusion/lookup/preserved Call instruction/playback/breakpoints/invalid closure. Same-ID steps across macros retained.
- Export exact roots/closure/assets; missing image; malformed/future/traversal/case collision/duplicate identity/limits; import all ID/binding/folder/image remaps, name suffix, identical reuse/differing rename, source unchanged and injected partial failure rollback. Template save/instantiate/fresh identities/dependencies and bidirectional independence.

**Acceptance/gates:** `cargo fmt --all --check`, `cargo check`, `cargo check --tests` if not already proven since API changes, `git diff --check`; then focused commands using actual discovered names:

```text
cargo nextest run --lib -E 'test(mkmacro::model::) | test(mkmacro::variables::) | test(mkmacro::interpolation::) | test(mkmacro::structure::) | test(mkmacro::editor_mutation::) | test(mkmacro::authoring_fields::) | test(mkmacro::authoring_analysis::) | test(mkmacro::call_graph::) | test(mkmacro::reusable_validation::) | test(mkmacro::validation::) | test(mkmacro::compiler::)'
cargo nextest run --lib -E 'test(gui::mkmacro_dialog::) | test(mkmacro::store::) | test(mkmacro::package::) | test(mkmacro::templates::)'
cargo nextest run --test mkmacro_authoring --test mkmacro_compiler --test mkmacro_store
```

Add new module names to filters if implementation naming differs; use `cargo nextest list` before relying on a filter to prove new tests ran. Keep heavy runtime-containing GUI tests grouped once. Capture exact pass/fail/skip counts, fix grouped failures then rerun affected test groups only. No new binary. Do not mark complete if a required targeted group remains failing.

## M14 — Runtime/debug/invocation tests and focused integration

**Goal / spec:** 44-46, preserve complete ordinary-runtime behavior and all invocation entry points. Suggested commit `test(mkmacro): verify reusable runtime and debug lifecycle`.

**Ownership/files:** executor/runtime/invocation module tests, existing `tests/mkmacro_runtime.rs`, `mkmacro_launcher_integration.rs`, `mkmacro_plugin.rs`, hotkey/dialog tests and existing fake backend only where necessary. Fix defects in assigned runtime integration boundaries.

**Required matrix:** A/B ordering; literal/variable/default/missing/incompatible values and builtin source references; isolation including parameters with same local name; complete output/discard/mapping atomicity; nested A/B/C; early/root Return; procedure natural end; output fallthrough; frame playback using deterministic waiter/random hooks; step repeat fresh frame; Call Stop/Continue/Retry (fresh locals, restart first instruction, no partial output); missing/disabled target and 64-depth malformed program guard; shared root transition limit; root-owned input across successful Return, child failure/retry/stop and panic unwind. Breakpoints root/direct/nested, safe active locals, correct stack/active/root ID, same step IDs in different macros, resume exact frame, stop full stack, normal ignores breakpoints and emits no debug vars. Parameter prepare no/default/required/supplied/type; prompt cancel/stale/busy and hotkey launcher routes; all six Run/Debug subset paths preserve dependencies and existing structural rejection. Compile immutable program then change callee document and prove active execution still uses original plan. No filesystem calls at child dispatch.

**Acceptance/gates:** focused Nextest groups discovered with actual test names, after source fix grouping:

```text
cargo nextest run --lib -E 'test(mkmacro::executor::) | test(mkmacro::runtime::) | test(mkmacro::invocation::) | test(mkmacro::hotkeys::)'
cargo nextest run --test mkmacro_runtime --test mkmacro_launcher_integration --test mkmacro_plugin --test mkmacro_recorder --test mkmacro_visual
```

Retain existing debugger race/admission/cleanup/recording/launcher compatibility tests. If UI smoke interaction is available, execute original spec clipboard, drag/drop, folding, annotation save/reload, reusable/nested calls, signature evolution and package smoke scripts. Native CUA availability is uncertain; record unavailable interaction honestly and compensate with pure/UI model tests, never claim manual passes. Do not automate dangerous real macro input merely to exercise fake-backend behavior.

## M15 — Independent review, remediation and full verification

**Goal:** satisfy all final definitions of done. No feature is complete until this gate passes.

**Ownership:** independent read-only reviewer (not the primary implementer) examines original spec, this ledger, baseline-to-HEAD cumulative diff, surrounding architecture and tests. Parent classifies findings; exactly one implementation writer remediates at a time and commits coherent fixes. Review architecture/performance as well as passing tests.

**Review checklist:** single mutation/identity source, no raw JSON replace, all string/image/call visitors exhaustive; no old private movement/duplicate implementation bypass; semantic/capability guard removed; document-aware compile at runtime; immutable closure and root-only slices; isolated call frame locals and current builtins; no nested worker/control; Call retries and output transactionality; guard lifetime and cancellation/panic cleanup; active/root snapshot identity and compound keys; dangling bindings retained; package remap/path/lock/rollback safety; template copy independence; revision-gated analysis/environment refresh and no extra polling/workers. Verify catalog/tests/UI all expose correct runtime capability. Check no dead adapters or source paths remain.

**Final commands (after review remediation and targeted reruns):**

```text
cargo fmt --all --check
cargo check
git diff --check
cargo nextest run --no-fail-fast
git status --short
```

If full suite fails: capture exact test and cause, group fixes, run narrow failing tests, commit remediation, rerun the **complete** `cargo nextest run --no-fail-fast`; final full pass required. No invented Clippy requirement. If review changes source, rerun affected focused groups before full suite; no need to repeatedly full-run unchanged code. Record actual output counts and command results. Review cumulative diff and stale-symbol search after remediation, ensure every intended source/test/help/ledger change committed and working tree clean.

**Completion report:** concise user-requested Implemented, Architecture, Schema Migration, Tests, Verification, Performance, Review, Commits, Remaining Issues; report actual verification only, all milestone statuses/commits available here. Only state “No known issues remain within the implemented scope.” if there are no substantive unresolved findings. Do not stop at plan/editor/targeted-suite completion.

## Specification coverage map

| Original numbered items | Pipeline owner | Final proof |
| --- | --- | --- |
| 1 metadata, 2 signature, 3 Call model, 4 Return model, 5 migration | M01 | M13 migration/model |
| 6 selection normalization, 7 cloning | M02 | M13 structural/ID |
| 8 clipboard and Duplicate | M03 | M13 clipboard/keyboard |
| 9 drag/drop | M02-M03 | M13 pure move/drop |
| 10 folding, 11 annotations | M03 | M13 fold/persistence |
| 12 representation, 13 Find, 14 Replace, 15 Jump, 16 Outline | M04 | M13 fields/navigation |
| 17 severity, 18 signatures, 19 calls, 20 graph, 21 static facts | M05 | M13 validation |
| 22 compiled program | M05, M07 runtime wiring | M13 compiler, M14 immutable run |
| 23 invocation/frame, 24 Call, 25 Return, 26 error policy, 27 control | M06-M07 | M14 executor/runtime |
| 28 callee breakpoints, 29 stack/Inspector | M08 | M14 debug lifecycle |
| 30 direct invocation | M09 | M14 facade/prompt/hotkey |
| 31 catalog/signature/Call/Return editors | M10 | M13/M14 authoring |
| 32 templates | M12 | M13 templates |
| 33 package, 34 closure/assets, 35 import | M11 | M13 store/package |
| 36 libraries | M11-M12 | M13 multi-root/copy semantics |
| 37 diagnostics UX | M04-M05, M10 | M13 navigation/severity |
| 38-43 model/editor/validation/compiler tests | M13 | focused recorded runs |
| 44-46 runtime/debug/invocation tests | M14 | focused recorded runs |
| 47-48 package/template tests | M13 | focused recorded runs |
| Unnumbered performance/help/manual review/full verification/commits | all, M12, M14-M15 | cache tests, inspection, actual smoke where possible, full suite, clean Git |

## Verification and review record

Construction/targeted/full verification and review findings are appended by the orchestrator. Nothing below is assumed passed until recorded with actual results.

- Baseline: clean branch/hash above; parent reported `cargo check` passed.
- Plan review: accepted by orchestrator. Dependencies, intermediate unsupported-action gates, complete specification coverage, late verification ownership, and compatibility boundaries reviewed. M01 started.
- Targeted tests: pending.
- Independent review: pending.
- Authoritative full Nextest: pending.
- Final clean Git status: pending.
- GUI smoke capability: parent read computer-use SKILL.md and its guidance/API/confirmation docs; initialized `@oai/sky` through available `mcp__node_repl__js`. `sky.list_windows()` succeeded; no Multi Launcher window currently open. Native smoke testing is available in principle after the final build; no manual test has run yet.
- Smoke isolation: `main.rs` derives `AppDataRoot` from relative `settings.json`; `platform/app_data.rs` preserves process current-directory resolution and the single-instance guard is data-root-specific. Launch the final executable with an explicit task scratch working directory to isolate smoke data, then select its actual returned window through `sky.list_windows()`; do not use the repository/user data directory.

### M01 construction verification

- Implemented grouped step metadata, schema 12 signatures and transparent numeric `MkSignatureId`, typed value sources/bindings, checked ID allocation reserving dangling references, and explicit unsupported Call/Return boundaries.
- Schema 11 migration preserves historical fields; persistence health probing still fully deserializes schema 11. Existing constructors and schema expectations migrated; no old migration fixture removed.
- Implementer ran `cargo check` (passed), `cargo check --tests` (passed; final run 43.11 seconds), `cargo fmt --all --check` (passed), and `git diff --check` (passed).
- Parent inspected model/type/migration/health/catalog/executor changes and test expectations; two minor test naming/assertion corrections resolved. No unresolved M01 finding.
- Focused tests added for metadata/signature/action roundtrips, type compatibility, signature identity preservation/dangling allocation/overflow, schema 11 preservation, and health probes. Execution deferred to M13/M14 per user preference.
- M07 must remove temporary `unsupported_reusable_action` diagnostics, capability gates and associated temporary test assertion when Call/Return runtime support is delivered.
- Commit: `89fdb753 feat(mkmacro): add reusable macro authoring model`. Working tree was clean immediately after commit; M02 started after commit success.

### M02 construction verification

- Centralized structural normalization, fragment cloning/insertion, ID-preserving move/drop, deletion and unwrap in `editor_mutation`; GUI/public legacy functions now forward to one owner. Stable selection primary/anchor survives moves and resets on explicit replacement selection.
- Added revision-cached semantic diagnostics/structure, variable inventory, and shared image inventories through nested coordinate/condition/image widgets. Explicit/open/asset-authoring/save/run refresh handles environment checks; changed references visibly await refresh. No new workers or polling.
- Canonical cloning uses checked fresh step IDs while retaining pixel producer/consumer result-slot semantics. Recorder append and macro duplicate use the same identity helper. Direct public draft mutations must call `mark_dirty` before cached reads.
- Implementer final commands: `cargo check --tests` passed (17.17 seconds), `cargo check` passed (8.10 seconds), `cargo fmt --all --check` passed, `git diff --check -- src` passed. Parent `git diff --check` passed and inspected final mutation/cache/selection/widget-context diffs.
- Nine focused tests added for structural normalization/atomic failures, overflow metadata cloning, pixel references, complete duplication, drop legality, stable selection, cache reuse and revision/save/reload/external-sync lifecycle. Behavioral execution remains explicitly deferred to M13; existing movement/deletion tests retained.
- Parent review found no unresolved substantive M02 defect. No production dependency added. User reasoning-setting edits excluded from this milestone commit.
- Commit: `875466b8 refactor(mkmacro): centralize structured editor mutations`. Only the four authorized-for-final-commit user `.codex` changes remained immediately afterward. M03 started after commit success.

### M03 construction verification

- Added process-local structured Copy/Cut/Paste/Duplicate through the canonical mutation owner. Cut prepares both clipboard and deletion before publishing; invalid mutations preserve source, selection, clipboard and revision. Macro switches reset selection/drag/scroll while retaining the clipboard and macro-scoped folds.
- Added stable-ID drag previews and insertion feedback with atomic validated drop; session-only nested folding retains hidden selection and a visible primary. Labels, comments, bookmarks and a fixed accent palette use the existing transactional action editor and compact row indicators.
- Keyboard routing honors text/modal/popup/pointer ownership, consumes egui clipboard events without writing the Windows clipboard, and accounts for the eframe 0.27 empty-clipboard V-release event. Removed per-frame selection reconciliation; drag preview is revision/anchor cached.
- Parent final `cargo check --tests` passed (16.83 seconds), `cargo check` passed (16.28 seconds), `cargo fmt --all --check` passed after formatting corrections, and `git diff --check` passed. Parent took over final gates after interrupting the unresumed implementer (`pending_init`); no source changes were discarded.
- Added tests for cross-macro clipboard/metadata, invalid Cut atomicity, macro-switch state, Duplicate, drag preview/failure/primary preservation, nested folding/selection, exact-marker annotation Apply/Cancel, and clipboard event/release routing. Existing context-menu expectations migrated. Behavioral execution remains deferred to M13.
- Parent inspected the final source/mutation/input gates/tests. No unresolved substantive M03 finding; no production dependency or worker added. Native GUI smoke remains pending final integration.
- Commit: `401933ca feat(mkmacro): add structured clipboard and step organization`. Working tree clean immediately after commit. M04 started after commit success.

### M04 construction verification

- Added exhaustive typed author-editable step-field traversal, separating plain text/templates/variable reads/assignments/images/colors/character keys. IDs, numeric values, enum tags and legacy payloads are excluded. Call/Return String sources are templates; SetVariable and condition String values retain literal semantics. Existing exact-key Unicode and escaped interpolation syntax remains compatible.
- Added transactional replacement previews with readable field paths, original row/macro captions, old/new values, selected-field/all/cancel operations, draft/macro staleness checks, typed image construction, launcher compatibility payload clearing, and rejection of introduced Fatal diagnostics while allowing unrelated existing errors.
- Added one revision-cached searchable row representation for Find/Jump/Outline and one stable-ID fold/select/scroll/focus navigation owner. Ctrl+F/H/G, F3/Enter navigation, no-match/wrap behavior and text/modal input ownership are integrated. Outline is a separate collapsible right panel (initially collapsed for existing 920px layout); filtered rows/results are cached and virtualized. Search children close with the parent dialog.
- Implementer final `cargo check --tests` passed (17.52 seconds), `cargo check` passed (7.97 seconds), `cargo fmt --all --check` passed, and `git diff --check` passed. Parent inspected final domain/visitor/payload/UI/focus/cache/lifecycle diffs and independently ran `git diff --check` successfully.
- Twelve focused tests added: seven typed-field/replacement tests, two navigation/cache/outline tests, and three search/replacement-apply/keyboard tests. Behavioral execution remains deferred to M13; native GUI smoke remains pending integration. Parent review found no unresolved substantive M04 defect.
- No new dependency, worker, polling loop, filesystem scan or ordinary-frame graph rebuild. Call/Return capability gates remain intentionally in place until M07.
- Commit: `cd9969db feat(mkmacro): add searchable outline and safe replacement`. Working tree clean immediately after commit; M05 started after commit success.

### M05 construction verification

- Added central signature, stable-ID Call/Return binding/type validation and explicit document-global identity diagnostics. Deterministic iterative dependency analysis separates enabled runtime edges from all authored package edges; closure admission retains caller-owned missing/disabled/ambiguous target failures while unrelated invalid macros remain visible without blocking a valid root.
- Extracted immutable compiled programs with O(1) plan/signature/name lookup and validated lowering that preserves individual macro instructions, jumps, playback and breakpoints. Existing singleton compile and temporary reusable runtime capability guards remain until M07.
- Moved variable catalog facts/tests into shared domain authoring analysis, including real parameter sources, Call output types, Unset/disabled/UI-read behavior and conservative control-flow warnings. Shared streaming interpolation scanner preserves exact-key Unicode reads, escaped/nonrecursive syntax and left-to-right failure order. Safe literal Returns terminate under Continue; potentially failing sources retain fallthrough.
- Added revision-cached root admission and modal-safe diagnostic navigation. Parent reviewed domain/compiler/cache/UI changes and resolved eager-parser error precedence, ambiguous signature inference, Continue-Return flow and action-editor navigation ownership findings. Row markers prioritize Fatal over warnings. No unresolved substantive M05 finding.
- Implementer final `cargo check --tests` passed (15.76 seconds), `cargo check` passed (7.53 seconds), `cargo fmt --all --check` passed, and `git diff --check` passed. Parent independently inspected the final diff and ran `git diff --check` successfully.
- Fourteen focused tests added for contracts/graph/program/flow/catalog/cache/navigation/interpolation, with existing catalog tests and legitimate warning expectations migrated. Behavioral execution explicitly deferred to M13/M14; M13 includes the new shared-domain test module. No production dependency, worker or polling added.
