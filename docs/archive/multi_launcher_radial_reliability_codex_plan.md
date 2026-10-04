# Multi Launcher — Codex implementation plan: radial reliability and action authoring

**Status:** approved scope; implementation not started by this planning packet.  
**Prepared:** 24 September 2026.  
**Starting source:** `multi_launcher(20260924-210943).zip`  
**SHA-256:** `f21ca0ee92d1b7572f779b936bb3ce1de7e685314f685ae8711d1734952d2544`

Read this together with:

- `multi_launcher_radial_reliability_approved_requirements.md` — authoritative behavior.
- `multi_launcher_radial_reliability_source_notes.md` — inspected owners, exact source references, and limits of the review.
- `multi_launcher_radial_reliability_acceptance_matrix.md` — executable acceptance obligations and evidence rules.
- `plans/radial-reliability-and-action-authoring.md` — mutable execution ledger.

The source archive is authoritative for the starting implementation. The live checkout must be compared with it before work. Subsequent intentional changes belong to the feature branch, not to a silently substituted older baseline. Historical documents in the archive describe earlier checkpoints, not fresh validation of this candidate.

## Outcome to build

A short release of the configured launcher chord toggles the main grid immediately, including when it is focused and during uninterrupted repeated taps. A short tap also dismisses an open runtime radial. A hold toggles only the runtime radial. The Designer remains a separate window with its draft intact.

Users can type a launcher query when assigning a cell, see recognizable result targets, pin a specific target/action or save a live query, and enable Auto Submit to run the query's current first result. Read-only previews do not execute. Commands reuse the existing parsing, search, action registry, safety, and dispatch boundaries. Noninteractive radial actions do not flash the main grid.

After core acceptance, improve multi-selection, bulk editing, navigation, and skin authoring on top of the existing system. Preserve existing menus, settings, packages, custom skins, actions, and unrelated launcher functionality.

## Architectural decisions and invariants

**One gesture owner.** Keep `LauncherInvocationService`/the current invocation reducer as the tap/hold authority. Preserve its physical timestamps, owned-release state, timer cancellation, input provenance, and exclusive-owner priorities. Do not add a local egui shortcut or second global listener as a competing fallback.

**One ordered visibility boundary.** Reuse `VisibilityToggleBatch`, `RootViewportCtx`, and the ROOT wake/apply paths. Desired state, pending commands, placement, and asynchronous restoration must have coherent ordering. A stale show/focus/restore cannot override a later hide. Extend the existing owner rather than building a second boolean state machine beside it.

**Independent owners, coordinated transitions.** Grid, runtime radial, Designer, and native authoring preview are distinct. Short-tap runtime dismissal must invalidate the exact runtime presentation/preparation leases and preserve Designer work. Hold dismissal must not generate a selection. Do not use a global close-all operation for a launcher tap.

**Pinned identity is not a live query.** Keep existing `Persisted` and `Contextual` bindings. Add explicit single-cell query and exact-command representations; do not masquerade as `DynamicSource::LauncherQuery`, store display indexes, or persist a live HWND. Reuse existing supported target IDs; a slug change is not a reason to introduce a new global note-ID system here.

**Same search, separate presentation.** Main-grid search, read-only authoring preview, and Auto Submit should share result production and ranking, with GUI updates kept outside that boundary. Do not implement a radial-only fuzzy matcher or use the authoring catalog's alphabetical action-verb ordering as query ranking.

**Execution is deliberate and scoped.** Resolve a saved query at cell activation, not merely at menu opening. Resolve, select the first result, freeze/revalidate its identity, classify interaction needs, and dispatch once. Use existing dispatch identities, input-release/native-close acknowledgments, Universal Actions, confirmations, and command outcomes. Explicit visibility-changing commands must still do what they say.

**No side effects from browsing.** Selection, search, preview, pinning, assignment, navigation, skin thumbnails, and package inspection do not run actions or add execution history/usage. Cache warming that is part of ordinary provider search is not action execution, but inspect provider behavior before claiming arbitrary searches are pure.

**Compatibility first.** Migrate schema through the existing decoder/store, preserve unknown-newer-version refusal and last-known-good behavior, and keep atomic publication/conflict checks. Do not overwrite existing skins with a new default on load.

**Bound work and memory.** No provider scanning while holding the Designer mutex, no repeated closed-Designer catalog construction, no unbounded workers/tombstones/trace logs, and no periodic full-application repaint as a readiness workaround.

## Milestone map

| Milestone | Concrete deliverable | Dependencies | Acceptance gate |
|---|---|---|---|
| M0 | Verified starting state, fixture/test map, initialized ledger | None | Baseline recorded; no invented Git history |
| M1 | Reliable tap/hold/grid/radial transitions plus exact-chord native proof | M0 | H — hotkey reliability |
| M2 | Typed saved-query/exact-command bindings and compatible persistence | M1 | P — persistence/model |
| M3 | Shared query resolution, safe Auto Submit, correct UI handoff | M2 | Q — query/execution |
| M4 | Shared query-first action editor and Add to radial | M3 | C — core feature complete |
| M5 | Multi-select, transactional bulk edits, consistent navigation | M4 | D — Designer usability |
| M6 | Curated skins/gallery and simple controls using existing renderer | M5 | S — appearance/compatibility |
| M7 | Full regression, native integration, independent review/remediation | M6 | R — final release evidence |

Implement one milestone at a time with one writer. Tests are written with each milestone but execution is batched at useful boundaries. M0 is short read-only reconciliation, not a second planning project. M1 must not expand into general hotkey, desktop, or GUI infrastructure replacement.

A milestone may be code-complete but validation-blocked; record the difference. A known failed H gate stops dependent feature expansion until remedied. An unavailable Windows environment can permit bounded non-native development, but it cannot make H green or justify spending the project on optional polish while the primary failure is unverified.

---

# M0 — Establish the candidate and execution ledger

## Objective and ownership

Make the working baseline, accepted behavior, and relevant verification commands unambiguous before modifying code. The planner/coordinator owns this reconciliation. Do not run a full build or test suite merely to write a plan.

## Relevant current state

`AGENTS.md` already specifies milestone handoffs, sequential writers, Nextest, independent review, compatibility, and honest completion. The archive has `.codex` roles and prior radial ledgers. It has no `.git` history. `Cargo.toml` uses Rust edition 2024 and the existing egui/eframe stack; no dependency upgrade is required for this scope.

## Tasks

**M0.1 — Reconcile the checkout.** Read `AGENTS.md` and the four packet documents. Record branch, HEAD, dirty status, toolchain, target/profile, and source archive checksum. Compare relevant code with the archive, especially invocation, visibility, radial model/handoff, action catalog, editor, and acceptance runner. Do not reset user changes, replace source wholesale from the ZIP, switch to an older branch baseline, or edit unrelated files. If the checkout has intervening work, record the concrete differences and retain them when compatible.

**M0.2 — Separate three baselines.** Record the archive identity, implementation-start Git revision/diff, and validation candidate binary/hash. A first commit on the feature branch or branch-point may be recorded for historical comparison only when actual Git history establishes it. Do not infer a commit from document names or timestamps. Build provenance must eventually include dirty patch/manifest identity when the working tree is not clean; a string such as `HEAD+dirty` alone does not identify the exact source contents.

**M0.3 — Map tests and fixtures without broad builds.** Locate inline tests and existing integration targets identified in source notes. Identify existing source-matched binaries/reports if available. Record missing native capabilities, exact configured chord, settings/profile isolation, and copied-profile availability. Reuse existing deterministic acceptance fixture generation and native process ownership safeguards. The current fixture's `radial_acceptance_harmless_*` command strings are labels for authoring tests, not evidence of a successful harmless external action; positive execution tests need a real safe executable/marker fixture or an instrumented executor at the proper test layer.

**M0.4 — Initialize the ledger.** All milestones/gates begin pending. Record intentionally superseded expectations: short taps used to preserve an active runtime radial in some tests; they now dismiss it. Keep historical plans intact and link this contract as the new authority. Preserve current `.codex` role/model configuration; this task does not authorize model-setting changes.

## Tests and verification

Metadata/file inspection only unless a concrete ambiguity requires more. `git status --short`, `git rev-parse HEAD`, `git diff --stat`, `rustc -Vv`, `cargo -V`, and `cargo nextest --version` are sufficient starting commands. Do not invent outputs when running outside the user's Windows environment.

## Done criteria

The ledger names the actual working source, archive identity, candidate availability, test groups, and unresolved environmental facts. No unknown question concerns already-approved product behavior. No application behavior changed in M0.

---

# M1 — Fix rapid tap/hold visibility and prove the exact chord natively

## Objective and architectural ownership

Restore the primary launcher workflow before touching appearance. Input classification remains in `src/hotkey/launcher_invocation.rs` and `src/radial/invocation.rs`; runtime coordination in `src/main.rs` and `src/radial/controller.rs`; ROOT visibility/activation in `src/visibility.rs`, `src/gui/render.rs`, `src/window_manager.rs`, and narrowly `src/window_activation.rs` where needed. Existing acceptance tracing and runner provide evidence.

## Relevant current state

Short release already emits `ToggleLegacyLauncher`. Main already batches ordered toggles and targets ROOT. Ordinary hide parks ROOT offscreen rather than clearing its native visible bit. `handle_legacy_grid_toggle` currently transfers radial keyboard ownership rather than dismissing the runtime radial. Show can launch asynchronous desktop/foreground restoration. `RadialRootState::restore` writes captured visibility flags, which also needs ordering review. These are source facts; the actual cause of the reported focused-window failure is not yet established.

## Tasks

**M1.1 — Trace the failing transition end to end.** Extend the existing bounded trace only where it lacks the needed correlation. For one physical gesture relate input admission, ownership decision, invocation ID, physical edge times, threshold deadline, release/hold decision, grid visibility revision, emitted ROOT commands, native HWND/PID/geometry observations, and restoration completion/cancellation. Keep per-gesture identity through the main/controller event boundary; current unit-like `ControllerEvent::ToggleLegacyLauncher` loses invocation metadata. Do not log private query/note content or dump events every frame. If a source-matched pre-fix binary exists, run the focused short-tap case and retain the evidence. Otherwise add the behavioral regression and collect the earliest useful candidate trace without doing a redundant full baseline suite. State hypotheses as hypotheses until evidence isolates a cause.

**M1.2 — Enforce one admitted decision per physical cycle.** Preserve the reducer's below-deadline release branch and hold-at/exceeding-deadline rule. Retain exact physical timestamps even when processing is delayed. Test early timer rearming, stale timer delivery, duplicate up/down, auto-repeat, and owned release drain. Repeated mapped chords release End and all modifiers; a held modifier variant must not corrupt subsequent fully released cycles. Do not reset input state merely because focus moves between ROOT and the Designer. Preserve suppression by capture/emergency owners and the existing mouse-gesture hook stability fix. Do not uninstall/reinstall hooks as a routine focus transition or add cooldowns that drop valid taps.

**M1.3 — Implement the approved short-tap transaction.** For one admitted short tap, compute exactly one grid toggle from the ordered desired state and request runtime radial dismissal. Cancel pending/opening runtime presentations as well as active runtime surfaces; invalidate their session/preparation identities so late readiness replies cannot reopen them. Use the normal close lifecycle and feedback to the invocation reducer. Clean up runtime keyboard ownership, hover state, pending timers, submenu surfaces, and leases without a synthetic hover selection. Preserve Designer/native-authoring-preview owners and unsaved draft. Apply the rule both through the native invocation service and the supported legacy launcher-trigger route so disabling the hold feature does not leave a direct radial menu immune to launcher-tap dismissal. Audit `handle_visibility_trigger_with_owner` and every `handle_legacy_grid_toggle` call. Keep ordinary non-hotkey launcher commands' existing semantics unless this coordination is explicitly appropriate to their action.

**M1.4 — Make ROOT transitions newest-request-wins.** Extend the existing visibility boundary with a monotonically ordered request/revision or equivalent single-owner serialization. Reuse existing flags as compatibility state where necessary; do not create two writable authorities. Every asynchronous ROOT restore carries the originating request identity, HWND identity/lifetime, and desired presentation state. Check ownership immediately before each desktop/restore/foreground side effect and after bounded waits. A check only at thread creation or only on completion is insufficient. Serialize final native side effects with newer visibility work or reconcile unavoidable in-flight native calls so a stale worker cannot leave a newer hidden ROOT onscreen or focused. Do not hold GUI/editor locks across waits or Windows calls. Coalesce/cancel stale restores rather than spawning an unbounded worker per tap. Keep non-launcher external window activation unchanged.

Also audit restoration of captured ROOT state during radial execution. A snapshot taken before a concurrent newer hotkey decision cannot later write its old `visible`/`restore` flags over that decision. UI snapshots must be revision-scoped or their ordinary-state restoration must exclude state now owned by a newer transition. Do not mask explicit `launcher:show`, `launcher:hide`, or `launcher:toggle` commands with a preserve-state wrapper. Retain configured/follow-mouse placement and current-geometry restoration: opening a note editor or mkmacro dialog must not reset a user-moved window to a stale static location.

**M1.5 — Keep the UI wake path responsive.** Retain explicit ROOT wake/commands while ROOT is parked. In `RadialEditor::show_deferred`, inspect the minimal open/close-pending state before constructing a catalog. Closed/idle Designer frames must not enumerate providers. A pending close still needs its lifecycle command/acknowledgment but not a fresh full action catalog. Capture only immutable minimal editor state under its mutex, release it before provider work, and recheck session/revision before using results. Do not move the entire `LauncherApp` into a worker or assume it is `Send`. Introduce a small revision/demand-driven snapshot path here; M4 can extend it for editor queries. Measure catalog-build counts/frame impact rather than claim an unmeasured latency improvement. No full-render polling workaround.

**M1.6 — Generalize and extend native acceptance.** Extend `src/bin/radial_acceptance.rs`, `native.rs`, and `suite.rs`; do not create another hotkey test application. Parameterize the configured chord throughout fixture generation, registration preflight, observer key selection, injection/release guards, profile metadata, and report labels. Retain F11 as a regression control; add the exact `Shift+Alt+Win+End` suite. The runner's low-level observer currently singles out F11/F24 and must observe the actual chord keys. Correctly encode extended End-key input and modifier up/down, preserve production injected-input admission policy, and release only input the runner owns on every exit path.

Add a filtered hotkey suite so this gate does not require rerunning every Designer case. Proposed additive CLI: `--suite hotkey|core|designer|all` and `--hotkey <chord>`; until implemented these are not existing flags. Preserve old invocation/output arguments. Include focused and unfocused ROOT, dirty Designer, preview open/closed, active/opening radial, and direct-trigger radial combinations. Establish starting focus once; then inject uninterrupted short-tap bursts with fixed physical spacing, no waits for UI outcomes between taps, no pointer move, and no external-anchor refocus. Observe after the burst. Check odd/even final parity AND one admitted decision per gesture; final parity alone would miss dropped pairs. A readable slower sequence also checks individual native transitions. The suite must keep unrelated mouse-gesture behavior enabled; disabled-diagnostic runs are not release proof.

Parameterize bounded case/report capacities: current runner has 31 IDs and a 32-case cap. Keep the final integrity and cleanup records reserved, report saturation as failure, and preserve explicit schema/versioning of reports. Do not silently truncate new cases. Input-desktop/integrity/focus preflight failures are environmental blockers, not passes.

**M1.7 — Migrate obsolete tests and close Gate H.** Rewrite tests that assert runtime radial survival after a launcher tap, including `legacy_grid_toggle_round_trip_restores_radial_keyboard_without_mouse` and the corresponding legacy-trigger scenario. They should prove dismissal, no selection, correct grid parity, stale-open rejection, and next-gesture recovery. Retain separate tests for keyboard-owner transfer caused by non-dismissal events; do not delete the mechanism just because its old hotkey scenario changed. Retain direct radial trigger, sticky/release/hold-click, capture, native-preview, Designer close, settings reload, and ordinary grid tests.

## Required tests

Use H01–H18 and L01–L03 in the acceptance matrix. Deterministic tests use a fake clock/recording backend, not wall-clock sleeps. Include show(A) → hide(B) → delayed A completion; show(A) → hide(B) → show(C) with A delayed; even and odd batches; shutdown/reload during owned release; pending radial open cancelled by a tap; and a radial selection lease distinct from a dismissal. Test newest-state ordering in ROOT snapshot restoration as well as native workers.

## Verification and done criteria

Run the focused library tests plus runner-unit tests once the coherent change compiles. Build candidate and runner together, then run the native hotkey suite against their recorded hashes with the exact chord and F11 control. Gate H passes only when native evidence shows repeated focused hide/show without outside input, runtime dismissal without action dispatch, unchanged grid state for holds, preserved Designer draft, no late reopen, and successful cleanup. No native environment means H is blocked, not complete. Keep a concise root-cause note tied to evidence; do not call an unverified hypothesis the confirmed cause.

## Non-goals and genuine uncertainties

No new hotkey framework, force-focus loop, blanket `Visible(false)` conversion, renderer upgrade, or general desktop/window-manager rewrite. The exact native failure mechanism and timing must be established from the source-matched runner; they are implementation investigations, not new product questions.

---

# M2 — Add explicit saved-query and exact-command bindings safely

## Objective and ownership

Make cell assignment semantics typed and persistable before building the UI. Primary owners: `src/radial/model.rs`, `migration.rs`, `validation.rs`, `store.rs`, `package.rs`, `bindings.rs`, `dynamic.rs`, `preparation.rs`, and relevant authoring operations. Inspect every exhaustive match over action/cell/frozen binding types and every package-reference traversal.

## Relevant current state

`ActionBinding` has `Persisted` and `Contextual`. `CellContent::Dynamic` plus `DynamicSource::LauncherQuery` creates result menus. The document schema is v2. `decode_document` currently performs its v1 migration by assigning `CURRENT_SCHEMA_VERSION`; adding another version requires deliberate migration sequencing rather than assuming v1 already knows future fields.

## Tasks

**M2.1 — Introduce a small typed model.** Retain all existing binding variants. Add an explicit saved launcher-query variant and an advanced exact-command variant, preferably in the existing action-binding boundary so alternate clicks and existing `CellContent::Action` composition remain usable. Suggested design, not an existing API:

```rust
// Illustrative shape; adapt names and derives to the existing codebase.
enum QueryRunMode { OpenLauncher, ExecuteFirst }
// Add to the existing ActionBinding, preserving old variants/tags:
// LauncherQuery { query: String, mode: QueryRunMode }
// ExactCommand { command: String, args: Option<String> }
```

The UI Auto Submit switch maps to the enum; its default is `OpenLauncher`. Keep raw command and structured arguments separate. Store authored query text, not a result snapshot or index. Do not serialize runtime `Command`/`UniversalAction` objects just because they happen to be in memory; use an explicit stable authored representation and parse at validation/execution boundaries. Preserve old label/icon/after-action/alternate bindings and contextual selectors.

**M2.2 — Define validation and readiness.** Reject empty/whitespace-only new query/command assignments with an actionable field error. Preserve meaningful interior query text and do not inject an `app` prefix or shell interpretation. Bound text/argument lengths consistently with existing model limits and document the chosen finite limit. Distinguish structural validity from current target availability. A provider temporarily having no results is not an invalid saved query, and an imported missing pinned target should be diagnosable without destroying the rest of the document. Exact parsing can legitimately fall back to `Command::External`; show that interpretation rather than claiming every parse success is a recognized internal command. Retain current command/safety validation; no command execution during validation.

**M2.3 — Migrate and round-trip.** Introduce the next document schema version (v3 on the inspected source) because the new serialized variants are not understood by old readers. Implement explicit v1 → v2 → v3 and v2 → v3 behavior or an equivalently clear validated sequence. Do not let `migrate_v1` blindly stamp v3 and skip required transforms. Default only absent new fields; do not reset configured thresholds, labels, styles, triggers, or existing binding modes. Decode/migration is read-only; saving remains an explicit atomic store transaction. Preserve future-version refusal, disk hash/revision conflict checks, backup/rollback policy, and last-known-good runtime state on invalid reload.

**M2.4 — Propagate through runtime and interchange.** Update resolver/preparation availability to represent a deferred query without pretending that an unknown eventual action requires no interaction. Pending/unknown query requirements need an explicit state or separate resolution phase; do not label them `None` simply to get through an existing match. Keep per-invocation dynamic menu snapshots distinct from live-query activation. Ensure primary/alternate cell bindings, copy/duplicate, undo/redo, defaults, importer compatibility, store references, and package export/import all preserve the new forms. Use the shared decoder for embedded document migration where appropriate. The outer package version changes only if its actual format needs it; do not bump unrelated versions automatically. Imported queries/commands are data and never auto-run on import or preview.

**M2.5 — Add compatibility fixtures.** Add minimal v1/v2/v3 fixtures alongside existing fixture patterns, with persisted notes, contextual windows, alternate actions, dynamic query menus, skin inheritance, assets, and the new assignments. Assert semantic preservation, stable IDs, intentional new defaults, round-trip behavior, and no disk write on load. Fail an unsupported-future fixture without publishing it. Reopen/save/reload a query cell with Auto Submit ON and OFF and a command with nonempty args.

## Verification and done criteria

Gate P requires focused model/migration/validation/store/package/binding tests; no native suite is needed for this data-only milestone. Existing v1/v2 inputs load without losing functionality, v3 round-trips all new types, invalid candidates preserve last-known-good state, and no provider/action execution happens during persistence. M2 is a coherent compiling vertical model change, not a collection of unimplemented `todo!()` match arms.

## Non-goals

No wholesale JSON rewrite, automatic profile migration-on-open, new global note UUIDs, lossy conversion of pinned actions into queries, or reimplementation of package safety.

---
# M3 — Implement shared query resolution and safe, no-flash execution

## Objective and ownership

Run saved queries like the main launcher while preserving radial execution lifecycle and intentional UI behavior. Owners: `src/gui/search.rs`, `universal_action_catalog.rs`, `radial_actions.rs`, `universal_action_executor.rs`, `command_host.rs`, `src/commands/handlers/launcher_query.rs`, the command model/outcomes/parser, and `src/radial/handoff.rs`/controller/bindings. Prefer one shared internal query service/module when it removes duplicated behavior; do not make the GUI frame a search engine.

## Relevant current state

Quick Tools uses `query:` versus `queryexec:`. `CommandOutcome::query` requests search, Show, restore, and focus. `apply_command_outcome_with_history_query` activates the first result and subsequently applies outer visibility policy; a naive wrapper can therefore re-show ROOT after the inner action. `search_read_only` exists, but `search()` separately assembles similar results and manages GUI caches. The radial executor already rejects duplicate/stale dispatch identities, revalidates targets, and selects `RootLauncherPolicy`. All query commands currently classify as `LauncherUi`. These boundaries should be reconciled, not bypassed.

## Tasks

**M3.1 — Share result production and ranking.** Extract/reuse the established query-to-results implementation under both `search()` and `search_read_only`. Keep caching, selected-row updates, layout, suggestion/history navigation, and query widgets outside the read-only result function. Preserve prefixes, aliases, case handling, exact/fuzzy settings, usage weights, enabled plugins, explicit plugin-command outputs, and empty-query behavior. Equal-score ordering must match the grid; do not introduce a separate alphabetical tie-breaker only in radial authoring. Tests compare results from both entry points under the same provider/usage snapshot, invalidating the main cache correctly. Do not compare a fresh query preview to deliberately stale cached grid state and call the difference a ranking bug.

**M3.2 — Define request ownership and live resolution.** Use a typed request/result envelope with a request ID, authored binding/query revision, owner (runtime dispatch or Designer session), configuration/provider revision, captured invocation context, and cancellation/expiry policy. Reuse existing `InvocationId`, preparation generation, editor session/draft generation, and dispatch token types instead of inventing parallel identity systems. Queries resolve when the user activates the cell. Changing the top result after menu opening but before selection must affect a live query, not a pinned action.

Use immutable provider/catalog snapshots where available. Run heavy provider work outside egui and outside the editor lock using existing services or a bounded worker, not a thread per keystroke. Do not move non-thread-safe app state to background threads. If a provider cannot offer an asynchronous snapshot, give it a bounded, visible resolution state through its established interface; do not guess that an empty temporary cache proves no results. An initial first phase can support existing synchronous snapshot providers while exposing explicit `Pending/Unavailable` for others, but the claimed supported query surface must match the main launcher's behavior, with documented blockers rather than silent unsupported commands. A superseded preview is discarded. A cancelled runtime dispatch cannot later execute or pop open ROOT.

**M3.3 — Select and freeze the intended action.** For Auto Submit OFF, dispatch the typed equivalent of opening a query through the normal grid path; show the grid, set the query, focus input, move caret appropriately, and preserve geometry. For Auto Submit ON, resolve fresh results, select exactly the first current result, and resolve its primary activation through the same action/command semantics as Enter in the grid. A pinned secondary action remains that secondary action; a saved query uses the first result's primary action.

Freeze the selected target/command and provider revision at this point. Revalidate its availability/identity before actual dispatch; do not re-rank after a close/release wait or confirmation and silently switch to a different result. If the selected target disappears, fall back to the saved query with a clear message rather than selecting row two. Ephemeral results can execute from a currently validated runtime identity but cannot become a persisted pin. Preserve command args and source/history attribution. Reuse ordinary result activation adaptation; no new per-plugin switch statement enumerating every launcher capability.

**M3.4 — Resolve interaction needs before handoff.** Extend the existing dispatch state machine for a query-resolution phase before it commits to `None`, `LauncherUi`, `ExternalInput`, or `ExclusiveCapture`. Keep the original dispatch identity reserved across resolution; consume it exactly once at the execution boundary. Revalidate a request's concrete interaction requirement rather than weakening the current requirement-equality guard.

A noninteractive action preserves ordinary grid state without issuing gratuitous Show/Focus/Restore. External-input and capture actions retain native radial-close/trigger-release/target-focus safeguards even though they do not need the grid. UI actions and destructive confirmation open the necessary launcher interface deliberately. Explicit `launcher:*` or `query:*` actions retain their intended UI semantics and must not be undone by `RadialRootState` preservation. A saved query can resolve to a navigation result such as `query:note list`; perform that navigation, not recursive implicit auto-submission. Explicit nested `queryexec:` results need a bounded depth/visited-request guard and diagnostic fallback so a cycle cannot recurse forever. This is a recursion safety limit, not approval to add action chains.

Validate `KeepOpen`/after-action policy against the resolved interaction requirement. For unresolved live queries, expose a conservative incompatible/unknown state or a typed effective-policy fallback with a visible explanation; do not enable an unsafe KeepOpen policy by claiming no interaction is needed. Close-for-handoff is an expected lifecycle step and must not cancel the action it is intentionally handing off. Distinguish that from user dismissal/session supersession.

**M3.5 — Unify query policy without changing unrelated callers.** Route radial saved queries through the shared resolution core and typed invocation context. Quick Tools and existing `query:`/`queryexec:` parser aliases must retain their established user-visible behavior. They may use the same resolver with legacy visibility policy while radial invocations request resolved-action-aware UI policy. Audit all `Command::Query` callers and preserve macro, favorite, history, and argument override behavior. Do not fix grid flash by globally removing Show from `CommandOutcome::query`, which would break manual queries. Remove outer post-activation visibility resurrection in the new radial policy path without changing explicit command outcomes elsewhere.

**M3.6 — Handle failures and reentrancy explicitly.** Empty authored queries are caught at authoring validation; no-result or unavailable first results at runtime open the grid with the original query and reason. Provider errors/timeouts include a readable failure message. A genuinely cancelled/superseded request is silent or diagnostic only; it must not use the no-result fallback to undo a newer user hide. Confirmations use the existing destructive-action policy, retain the selected identity, and do not dispatch on Cancel. Account for settings reload, feature disable, closing/reopening a menu, a new hotkey gesture, and a query resolving to a radial/launcher command. Prevent double dispatch on repeated pointer input, repeated worker reply, or retry delivery. Once an action has committed, record that fact; do not pretend cancellation undoes an external process or clipboard change.

**M3.7 — Prove no execution during preview and no grid flash.** Add spy-executor unit tests proving zero dispatch, zero execution-history entries, and zero usage increments for preview/assignment. Execute a safe fixture action and assert one effect, not merely an attempted dispatch. Compare full ROOT ordinary state for non-UI actions when initially hidden and initially visible; allow changes genuinely made by the action. For native no-flash acceptance, combine production ROOT Show/Focus/Restore trace absence with native presentation/focus observations during resolution and handoff. A final hidden-state screenshot alone cannot prove there was no flash. Run a UI-required note-editor/query/confirmation example to prove required UI was not suppressed in the opposite direction.

## Tests and done criteria

Use Q01–Q20, L04–L06, and core native cases from the matrix. Gate Q requires matching first-result behavior, default manual-query UI, changed-rank live resolution, stable pins, no-result fallback, no unintended ROOT flash, correct UI-required behavior, confirmations, runtime identity revalidation, recursion guard, and duplicate/stale-reply rejection. All supported launcher query families route through the existing behavior rather than a small hard-coded allowlist. Provider-specific limitations must be recorded as incomplete behavior, not silently treated as successful empty results.

## Non-goals

No new command syntax to replace the launcher, arbitrary shell evaluation of query text, global plugin redesign, or persistent live target IDs. Do not alter usage ranking merely to make one acceptance fixture first.

---

# M4 — Ship one query-first editor and Add to radial

## Objective and ownership

Make the new model usable from both current editing surfaces and the main grid, with recognizable targets and safe previews. Owners: `src/gui/radial_editor/mod.rs`, a small shared editor submodule, `src/gui/universal_action_catalog.rs`, `src/gui/render.rs`, `src/radial/authoring.rs`/`authoring/menu.rs`, and any necessary shared UI-intent boundary. Use existing deferred-viewport/session/intent bridges; do not borrow `LauncherApp` across a deferred child callback.

## Tasks

**M4.1 — Define a shared editor state and intent API.** The component consumes an immutable authoring/search snapshot plus the current draft binding. It emits typed intents such as Search, Pin, SaveQuery, SetCommand, and explicit Test; it does not execute commands or save files itself. Both Inspector and Cell Properties instantiate the same component with stable egui IDs scoped by surface, editor session, and entity ID. Shared behavior does not mean they share an unscoped text buffer or widget ID. Preserve unsaved text per edit session and reject stale replies when a cell is changed, deleted, or the Designer is closed/reopened.

Suggested layout: a primary launcher query field; a visible result list grouped by target with a primary-action default and action selector; `Pin this result/action`; `Save query` with Auto Submit; read-only `Would execute`; and a collapsed Advanced exact-command section. The persisted binding type must remain obvious in the assigned-cell summary. A pinned note can still show its last-known title with an Unavailable marker if removed. Do not rename every cell automatically when its target title changes; keep user-authored labels separate from suggested labels.

**M4.2 — Fix target presentation and filtering once.** Extend the picker row presentation with target title, type, disambiguator, and action verb rather than relying on `presentation.label` alone. For two notes show, for example, `Daily Log — Edit Note` / `Note · daily-log`, and `Project Ideas — Edit Note` / `Note · project-ideas`. If titles collide, show the slug or another existing stable identifier. Give long text usable wrapping/ellipsis and accessible full information; tooltips are optional extras. Display availability and disabled reason directly when selection would fail.

Remove the Inspector's second whole-string label/command filter. Search the same result set as the shared query entry, and use the action catalog to enumerate actions for selected targets. Search/filter before pagination or display caps; show a result count and an explicit load-more/scroll/virtualized view rather than making entries beyond 50 unreachable. Query matching and action selection are different stages: do not replace main-grid result ranking with a flat alphabetized list of every action verb. Retain contextual action browsing for CapturedForeground/UnderPointer/LastExternal and explain why an ephemeral result cannot be pinned.

**M4.3 — Implement read-only preview and explicit assignment.** Display `Would execute: <target> — <primary action>` from the current request, with Pending/Unavailable/No results states. Label live preview as current rather than a guaranteed future target. `Pin result` converts only through the supported `assignment()`/persistent-ref boundary. `Save query` stores query text and mode; Auto Submit starts OFF for new assignments and retains an edited cell's saved choice. Changing preview selection alone does not change the cell. Applying the authoring edit creates one undoable mutation. Explicit Test uses current unsaved binding plus the normal test-action revalidation/confirmation path; it is visually distinct from preview and never runs on Enter while the query text field is merely being edited unless the existing explicit Test control is intentionally invoked.

**M4.4 — Add `Add to radial menu` through the main result context menu.** Extend the shared result context-menu path in `src/gui/render.rs` instead of implementing independent list/grid variants. Capture the selected result and desired action semantically, not the current display index. Open the existing Designer or reuse its current draft, then choose destination menu/ring/cell and show a summary. Default to the result's primary action; offer supported secondary actions. An occupied destination requires an explicit Replace choice; append/empty slot is preferred when valid. Preserve ring capacity/resize/submenu-graph constraints.

Do not overwrite or reload an already-dirty Designer document. Route the insertion to its current session as one mutation, leaving Save/Apply explicit. If the Designer is closed, open it through its ordinary lifecycle and enqueue the assignment only for the matching successful session/snapshot. For an ephemeral result, show `Pin unavailable` and offer an explicit contextual or live-query route rather than inventing a persistent identifier. Cancel leaves source action, radial document, grid query, and execution history unchanged. Choosing Add must never fall through to the source row's double-click/Enter activation.

**M4.5 — Finish demand-driven snapshots.** Build/search the action catalog on open, relevant provider/data revision, query change, or explicit refresh—not every frame. Preserve correct invalidation for notes/favorites/macros/custom actions/config reload. Keep snapshot construction outside editor locks and perform stale-session checks before publishing. Repaint only the affected viewport on a ready reply. Ensure close/dirty-discard/native-preview cancellation work while catalog/query work is pending. A closed Designer still completes required close acknowledgments without background provider scans. Add counter-based tests for no work during idle closed frames and no rebuild on an unchanged open frame.

**M4.6 — Native/headless authoring acceptance and old-test migration.** Use retained headless egui tests for text editing, mode switches, stable IDs, row labels, assignment, and per-surface equivalence. Extend existing native semantic-control evidence with query field/result/action/mode/assignment controls, scoped to editor session and revision; evidence must come from actual production widgets, not an acceptance-only mutation shortcut. Adapt old picker tests to new layout while keeping beyond-first-50 coverage. Native acceptance must type a query, distinguish two notes without a tooltip, pin one, save a second live query, toggle Auto Submit, Apply/Save, close/reopen, and inspect the persisted binding. Prove zero leaf dispatch during all browsing/editing and preserve the dirty draft while tapping the launcher chord.

## Done criteria and Gate C

Both surfaces behave identically through one component; labels identify targets; saved queries and pins remain visibly different; Advanced commands work through parser/dispatcher; Add to radial stages the correct binding without execution or lost draft; no expensive closed-state catalog work remains. Gate C reruns H and the core Q/D integration scenarios against this candidate. Core reliability/action/query changes can be delivered now, before M5/M6 cosmetics.

## Non-goals

Do not redesign the whole Designer layout, remove advanced controls, bypass the authoritative authoring store, or execute results from the preview for visual feedback.

---

# M5 — Add transactional multi-select, bulk editing, and consistent navigation

## Objective and ownership

Improve everyday menu maintenance using existing stable IDs and undo/history. Owners: `src/radial/authoring.rs`, `authoring/menu.rs`, `src/gui/radial_editor/mod.rs`, `canvas.rs`, and navigation/preview helpers. Keep domain mutation rules outside widgets.

## Tasks

**M5.1 — Model stable multi-selection.** Extend the existing stable selection concept with a set of authored cell identities and a primary selection/anchor. Do not use flat display indexes or labels. Plain click selects one; Ctrl-click toggles; Shift-click selects a range in the current deterministic tree/ring order. Text-edit controls retain their normal Ctrl/Shift behavior. Canvas hit-testing resolves to authored IDs; generated dynamic cells either map explicitly to their source cell with explanation or remain read-only, never become persisted synthetic targets. Initial scope may restrict a bulk selection to one menu, including multiple rings, if made explicit in UI; navigating to another menu clears or deliberately reconciles that selection.

Prune missing IDs after delete/reload/undo and preserve surviving IDs across rename/reorder. Keep selection state out of the persisted runtime document unless there is an established separate preferences field for it. Add keyboard/accessibility indication for selected cells without conflating selection and runtime hover/activation.

**M5.2 — Implement one transaction per bulk action.** Add typed bulk label/style/after-action operations that validate all selected targets and compatibility before applying. A failed validation leaves all selected cells unchanged and names the incompatible cells; do not partially edit and quietly skip failures. Use one existing history transaction/draft-generation advancement for a bulk operation, not one undo step per cell. Continuous sliders/text edits can coalesce within their existing edit gesture, then finalize on blur/commit. Use `Mixed` presentation for differing existing values. Editing one field changes only that field, preserving other values, inheritance, and intentional Clear versus Inherit semantics.

For labels, provide an explicit bulk operation such as Set label or Add prefix/suffix; no automatic renaming merely because cells are selected. An optional numbered pattern belongs only if small and bounded, not a new expression language. After-action edits use the same compatibility logic as individual editing, including unresolved queries. Undo restores exactly the previous labels/overrides/policies/selection; Redo reproduces the change. Preserve copy, duplicate, reorder, generated-ID allocation, and graph validation through existing helpers.

**M5.3 — Unify search and navigation.** Add a menu/cell filter over authored names/labels/IDs without mutating the document or runtime query. A result selects/reveals the stable target and expands its ancestor path; clearing the filter restores the tree without losing valid selection. Keep selection navigation separate from runtime submenu dispatch. Use a shared navigation model for tree selection, canvas reveal, and breadcrumbs. Shared submenus can have multiple parents: breadcrumbs follow the actual navigation path, not an arbitrarily inferred single parent. Back returns to the previous valid edit location; missing/deleted nodes are skipped safely; cycles/depth obey existing graph rules. Preserve unsaved property drafts by committing through explicit accepted behavior or showing the existing prompt, not silently discarding them. Escape in a text field/popup clears/closes that local interaction before becoming a window-close action.

**M5.4 — Validate usability and lifecycle.** Headless tests cover range selection, Ctrl toggles, mixed values, one-step undo/redo, invalid-policy atomic rejection, selection after ID-preserving edits, deleting selected nodes, dynamic-cell handling, and multi-parent breadcrumbs. Native smoke covers Ctrl/Shift selection, bulk change, Undo, search/reveal/back, then launcher short-tap toggling while the Designer holds a dirty draft. Do not change runtime ring order or hotkey behavior as a side effect of selecting cells in the editor.

## Done criteria and Gate D

Bulk edits are atomic and undoable, selection survives stable-ID operations, search/navigation cannot lose work, and no editing interaction executes an action. All selected usability operations work in compact layouts and do not regress Gate H/C. No new persistence semantics are introduced unnecessarily.

## Non-goals

No cross-document bulk editor, action chains, conditional cells, collaborative editing, or general-purpose label scripting.

---

# M6 — Curated skin gallery and approachable appearance controls

## Objective and ownership

Expose existing styling capabilities cleanly, preserving advanced customization and renderer consistency. Owners: `src/gui/radial_editor/skin_editor.rs`, the shared preview/editor modules, `src/radial/skin.rs`, `render.rs`/`preparation.rs`/`geometry.rs`, `assets.rs`, `package.rs`, and style validation. Avoid renderer replacement or extensive effects work.

## Tasks

**M6.1 — Add a small curated preset family.** Create a Modern Clean base with Compact and Comfortable density variants, a High Contrast preset, and one or two Classic-inspired variants. These are appearance/density options, not separate behavioral menu types. Use existing skin/style model fields for defaults. Keep built-in preset IDs stable and avoid collisions with user IDs. Changing the default affects new/starter configurations; existing users retain their selected skins and overrides. Apply a preset as an explicit undoable draft change, not an implicit file write or application restart.

Audit provenance before using image assets from reference archives. Prefer original procedural/vector-style assets or existing appropriately licensed assets. Do not redistribute bundled fonts without permission or extract/share system fonts. Missing optional media falls back visibly and safely; no silent replacement of user files.

**M6.2 — Build a truthful thumbnail gallery.** Render thumbnails from a fixed safe demonstration document through the same style compiler and rendering/preparation path used by preview/runtime, with fixed data and no live provider/action execution. Cache by effective skin/style/asset revision, scale, and thumbnail geometry. Bound memory, lazily prepare visible entries, invalidate only changed keys, and cancel/discard obsolete generation work. No native runtime window per thumbnail and no full thumbnail set rebuilt per egui frame. Show name, visual preview, built-in/custom origin, and current selection. Selecting a tile previews; Apply commits to the draft/history. Cancel restores the prior draft appearance, not a disk reload that destroys other changes.

**M6.3 — Add simple controls with honest advanced interaction.** Expose Accent, Menu scale, Spacing, Opacity, and Label size/visibility/readability. Map each control to documented existing style fields and a declared editing scope. A simple control may change several existing style fields atomically, but it cannot flatten the entire six-level inheritance tree or erase unrelated custom overrides. Show when lower-scope overrides mask a menu-level control, with an explicit opt-in reset-at-scope action. Keep `Inherit`, `Clear`, and explicit values distinct. Advanced Inspector edits are reflected in simple controls where representable; otherwise display Custom/Mixed instead of a false single value.

Ensure the same effective style determines drawing AND hit testing. Scaling or spacing must not create a clickable region that differs from the visible cell. Native and embedded preview use the same prepared style/geometry; keep current tooltip, submenu indicator, alternate-click, and paging behavior.

**M6.4 — Add readable density diagnostics and verify.** Evaluate effective label fit/overlap, target size, available monitor work area, and DPI/scale using current geometry. Warn before/after a dense layout becomes hard to use; offer existing remedies such as larger scale, wider rings, fewer visible slots, pagination, or shorter labels. Do not silently delete cells, cap a previously valid 50-cell ring to an arbitrary new limit, or shrink text to unreadable sizes to force fit. Preserve existing hard validation limits; distinguish warnings from invalid geometry. High Contrast must use visibly distinct hover/selection/focus states and legible text without relying only on a subtle color change. Do not claim formal accessibility-standard compliance without measuring it.

Add deterministic effective-style/geometry tests, cache-invalidation/no-idle-work tests, preset round-trips, package export/import, missing-asset fallback, and inheritance provenance. Use toleranced visual comparisons or screenshots as supplementary evidence, not a single pixel-perfect test that fails on every font/DPI environment. Native smoke covers at least the available normal/high-DPI contexts, a dense ring, and skin switching while verifying correct hit targets. Record untested monitor configurations explicitly.

## Done criteria and Gate S

Curated options are discoverable and consistent with runtime; controls do not destroy advanced overrides; existing user appearance is unchanged until explicitly edited; density problems are explained; caches stay bounded and event-driven. Recheck core hotkeys after appearance edits and native preview interactions. No extensive animation/effect pack or broad renderer rewrite is included.

---
# M7 — Final regression, native acceptance, and independent review

## Objective and ownership

Prove the integrated feature against a source-matched candidate and leave an auditable completion record. The implementer owns remediation and test execution; an independent read-only reviewer owns the final diff/architecture review. One writer remains active at a time.

## Tasks

**M7.1 — Audit the full migration and test inventory.** Search all binding variants, query callers, ROOT visibility/restore writes, close reasons, authoring edit surfaces, package traversal, and new enum matches. Remove transitional duplicate logic and unreachable compatibility scaffolding introduced by this work. Check that the shared editor is actually shared and the grid/query search code has not drifted into two independent implementations. Verify deliberate old-test replacements preserve their valid underlying invariants. Compare discovered test target counts and document skipped/ignored tests; do not hide new failures by changing defaults or filters.

**M7.2 — Run final Rust verification in an efficient batch.** Format/diff-check, run the complete required Cargo Nextest suite for the normal Windows configuration, and run relevant doctests separately. If the project has an established additional feature configuration used by the hotkey path, run its applicable coverage; do not indiscriminately require every feature/target combination without evidence it is supported. Use the same toolchain/profile/target directory across checks. Build the real application and native runner from the final source state. Avoid `cargo clean`, unnecessary debug/release duplication, and global dependency upgrades. Source changes after verification invalidate the affected evidence; final completion requires results attributable to the final tree.

**M7.3 — Execute full native acceptance.** Run the exact-chord and F11 suites, core action/query cases, Designer scenarios, and skin smoke against the final candidate. Capture process/window identity, source manifest/revision, executable and runner hashes, toolchain/profile, fixture/config hashes, monitor/DPI topology, injection provenance, per-case status, failure stage, input timings, and cleanup. Evaluate transient no-flash behavior through command/native evidence, not just final flags. Rerun repeated tap/hold sequences with the Designer and native preview in their tested combinations. Keep application subsystems normally enabled; diagnostic modes that disable mouse gestures or add between-tap quiescence are useful for diagnosis but not substitutes for normal-mode acceptance.

Use a disposable copied/sanitized user profile when an authorized consistent copy is available. Never start a second instance on the user's active data directory. Copy while the source is quiescent or through a consistent snapshot, preserve the source untouched, and block execution of real user commands during copied-profile authoring tests. Use isolated safe fixtures for positive execution. The current runner reports copied profile `not_run`; implement real supported copied-profile status/reporting only if that mode is actually exercised. If no suitable copy is available, record migration-fixture coverage and copied-profile acceptance as not run; do not falsely claim real-profile validation. This limits the verification claim rather than inventing a result.

**M7.4 — Independent review and focused remediation.** Give the reviewer the approved requirements, source/diff identity, completed ledger, and test artifacts. Ask for concrete findings ordered by severity: dropped/double gestures, obsolete ROOT restore, cross-window close, stale dispatch/preview, identity retargeting, confirmation bypass, parser fallback confusion, lossy schema/package migration, inaccurate search ranking, unbounded work, weak/no-op tests, native proof gaps, and hidden user-data changes. The reviewer does not change source. The single implementer fixes substantive findings, adds regressions, and reruns the affected checks. A final substantial source change requires refreshed full-suite/candidate evidence rather than attaching a stale green report.

**M7.5 — Publish a precise completion report.** Summarize implemented behaviors, intentional changes, architectural decisions, test names/groups added or migrated, exact commands, pass/fail/skip counts, exit codes, commit/diff identity, candidate/runner hashes, native report paths, review status, and known limitations. Include a brief user-facing explanation of Pin versus Save query and Auto Submit, plus how to use Add to radial, bulk edits, and presets. Do not say “no regressions” merely because it is a goal; state the coverage that passed and any environment not exercised. Every required gate must be passed or explicitly blocked; no blocked native case becomes a pass through wording.

## Done criteria and Gate R

All approved behavior is implemented; applicable full Nextest/doctest coverage passes; final native core and Designer cases pass on the exact candidate; no unresolved substantive review findings remain; compatibility and safe profile isolation are evidenced; and the ledger has no unsupported completion claims. Optional unavailable environment combinations and missing copied-profile evidence are explicitly separated from passed mandatory scenarios. A missing native core gate prevents final feature sign-off.

---

# Verification cookbook and build-cost discipline

## Commands are examples for the inspected package layout

Run commands from the actual Windows checkout root. Check local tool help/version once before using version-dependent options. Use PowerShell-native quoting for the examples below. This plan does not assert that these commands have been executed.

```powershell
# Cheap checks; do not start with a full build just to inspect the repository.
git status --short
git diff --check
cargo fmt --all -- --check
rustc -Vv
cargo -V
cargo nextest --version
```

After M1's code/test batch, a focused library scope can be:

```powershell
$filter = 'test(/^(hotkey::launcher_invocation::|radial::invocation::|radial::controller::|radial::handoff::|visibility::|window_activation::)/)'
cargo nextest list --locked -p multi_launcher --lib -E $filter
cargo nextest run --locked -p multi_launcher --lib -E $filter --no-tests=fail --no-fail-fast
cargo nextest run --locked -p multi_launcher --bin radial_acceptance --no-tests=fail --no-fail-fast
```

Discover and adjust filters if symbols move. Inspect listed test counts rather than trusting a filter string. Listing can itself compile test binaries; do it once when establishing/changing a target group, not as redundant work before every identical rerun. Prefer `--lib`, a named `--test`, or a named `--bin` where that contains the affected tests: a name filter alone does not necessarily prevent Cargo from compiling other selected test targets.

For M2/M3/M4, reuse existing inline module tests and relevant current integration targets; do not create one integration binary per tiny case. Current integration targets worth checking after visibility/command changes include `focus_visibility`, `gui_visibility`, `hotkey_events`, `hide_after_run`, `follow_mouse`, `plugin_commands`, `plugin_routing`, `preserve_command`, `notes_plugin`, and the existing `domain`/`plugin_queries` suites. Confirm names in the live checkout rather than assuming a filename not in the inventory exists. Add a newly necessary integration suite only where it buys a meaningful isolation boundary.

```powershell
# Final normal regression run, not a command to repeat after every small edit.
cargo nextest run --locked --no-tests=fail --no-fail-fast
cargo test --locked --doc

# Native application/runner binaries are distinct from test harness binaries.
cargo build --locked --bin multi_launcher --bin radial_acceptance
```

The archive has no `.config/nextest.toml`; a user's checkout/global config may. Verify that “full suite” is not silently restricted by a default filter. Use supported local options to remove that restriction when required and record ignored/skipped tests. Nextest does not automatically turn native GUI scenarios into automated tests; the opt-in runner remains separate.

Capture build-produced executable paths instead of assuming `target/debug` if `CARGO_TARGET_DIR`, target triples, or profile configuration differ. Where default paths do apply, the CURRENT runner accepts:

```powershell
$revision = (git rev-parse HEAD).Trim()
$runName = 'radial-reliability-' + [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss')
$reportDir = Join-Path (Join-Path (Get-Location) 'target') $runName
& '.\target\debug\radial_acceptance.exe' `
    --launcher '.\target\debug\multi_launcher.exe' `
    --output $reportDir `
    --source-revision $revision
$acceptanceExit = $LASTEXITCODE
# Preserve $acceptanceExit immediately in the durable job metadata/report.
```

`--output` accepts a new or existing empty run directory; prefer a unique new directory and never overwrite an old report. The current runner also supports `--report`, `--h6-repeat`, `--mouse-gestures`, and `--keep-profile-on-failure`; review `--help` before composing an invocation. M1's proposed `--suite` and `--hotkey` are NEW work and must only be used after implementation/help tests. Example intended after M1:

```powershell
# Proposed new runner options, not available in the starting archive.
& $runner --launcher $candidate --output $newRunDir `
    --source-revision $recordedSourceIdentity --suite hotkey `
    --hotkey 'Shift+Alt+Win+End'
```

A Git revision argument alone is not a cryptographic source-to-binary proof. Record the actual source manifest/dirty diff, build command, Cargo.lock/toolchain, artifact paths and SHA-256 values in the ledger. Run runner and candidate from the same finalized source tree.

## Logging, polling, and safety rules

Keep UTF-8/no-color logs using the local shell/tool-supported settings. Capture command, working directory, start/end, exit code, candidate identity, and log path in a small metadata file. In PowerShell, save `$LASTEXITCODE` immediately after the native process, before another native command or wrapper can overwrite it. A command reporting zero tests is not verification. A green process exit with saturated/missing acceptance cases is not acceptance.

Launch one expensive build/test job at a time and record its PID/job identity. Reuse incremental artifacts; avoid building check + test + application + release variants unnecessarily for the same small edit. When a known long-running job is quiet, check its actual state rather than rerunning it. Use completion signals where possible; otherwise use the user's 10–20 minute observation cadence for those long jobs. A readily available completion result can be processed immediately. Do not stretch native frame waits or input-release safety deadlines to 10–20 minutes.

Keep native tests opt-in and isolated. The runner owns its child process and test data directory, captures/restores foreground and cursor where possible, and releases its own injected keys during error/timeout cleanup. It must not kill unrelated launchers, change the user's hotkey configuration, temporarily disable OS safeguards, or require testing against the user's live profile. Equal-integrity/interactive-desktop limitations are environment facts to report, not excuses to bypass production input routing.

## Milestone handoff/report template

For each implementation handoff, copy the corresponding M section and add only verified live-checkout differences. Do not make the implementer re-plan the entire feature. Ordinary naming, borrow-checker adjustments, helper placement, and equivalent local Rust design belong to the implementer.

```text
Milestone:
Starting source/commit/diff identity:
Objective and approved requirement IDs:
Dependencies/gates:
Files/owners:
Ordered tasks:
Invariants/non-goals:
Tests to add/migrate:
Smallest useful verification commands:
Native requirements (if any):
Done criteria:
Actual results/artifacts:
Known blockers or deviations:
Commit/diff identity after work:
```

## Common incorrect implementations to reject

A second hotkey listener; a debounce that drops rapid taps; waiting until the hold threshold for short taps; closing the Designer with the grid; preserving the radial after the now-approved dismissal tap; executing the hovered item during dismissal; checking only the final parity of a burst; testing only F11; testing only with external-anchor refocus; asserting `IsWindowVisible` alone proves onscreen state; checking cancellation only after a stale native restore; treating a renamed/missing pin as a query fallback; sorting live query results by action verb; silently executing the next available result; executing previews; persisting HWNDs/list indexes; stripping all query Show semantics globally; recursively auto-running every query suggestion; changing user skins on load; unlimited thumbnail/query workers; hiding failures behind retry/diagnostic modes; claiming historical pass counts validate the final candidate.

## Primary external references used for verification guidance

The application's source defines its behavior. The following official references explain platform/tool constraints only, not the application's implementation. Consult installed tool help for version compatibility.

- Microsoft, `SendInput`: `https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput` — injected input and integrity-level constraints; existing key state and release handling matter.
- Microsoft, `SetForegroundWindow`: `https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow` — foreground activation is policy-constrained, not guaranteed by calling the API.
- Microsoft, `IsWindowVisible`: `https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-iswindowvisible` — a visible-style bit is not a presentation/occlusion oracle.
- Nextest, running/selecting/listing: `https://nexte.st/docs/running/` ; `https://nexte.st/docs/selecting/` ; `https://nexte.st/docs/listing/` — filtering, empty-selection behavior, and target selection.
- Cargo, `cargo test`: `https://doc.rust-lang.org/cargo/commands/cargo-test.html` — documentation-test verification with `--doc`.

These sources were checked while preparing the plan; no application build, unit test, or Windows native test was run here.
