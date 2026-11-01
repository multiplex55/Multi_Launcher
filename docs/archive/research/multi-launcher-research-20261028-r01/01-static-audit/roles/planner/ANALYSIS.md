# Goal 01 — Planner analysis and research checkpoint handoffs

Authored 2026-10-28 by `/root/audit_planner`; requested/configured Astra max, resolved backend identity unverified. Source: `59156d96adcc73939c7171487a0cb582e3286587`. Parent persists this contribution. All checkpoint states below are **planned**, not completed audit coverage.

The research author should execute one packet at a time and return a coherent documentation increment. This plan identifies real owners and the questions worth tracing; it does not pre-author the full audit. No production implementation, application execution, or Goal 02 work is authorized by these packets.

## Established architectural facts worth preserving

1. The application already has a typed command boundary. `CommandInvocation` retains the original `Action`, query override and activation source; `CommandBus` dispatches typed domains through host traits. Ordinary GUI activation parses, applies destructive confirmation, then dispatches. A replacement command bus is not justified by the current architecture. [C1], [C2], [C3]
2. Launcher and radial query resolution already share `search_read_only_outcome`. Its result includes pending state, provider revision and mutable result-catalog versions. Reordered results are installed by `search`, which clears selection; a capacity-release refresh consults the current query rather than restoring a worker's captured query. Preserve these distinctions in any later search design. [S1], [S2]
3. `PluginQueryPolicy::Literal` is an explicit exception to action-filter parsing, and applies only when the original query head matches that plugin's prefix. Normal and snapshot searches share `plugin_query_plan`; enabled-plugin and capability checks occur in `plugin_matches_search`. Do not describe all plugin queries as interchangeable plain text. [P1], [P2]
4. Universal Action execution already separates `ActionSurface` from `ActivationSource`, rejects disabled availability before execution, and retains invocation context through confirmation. The inspected radial confirmation branch rechecks its lease and query/catalog revisions and may reopen a query instead of executing a stale selection. The remaining surfaces still need the planned audit trace. [U1]
5. Input authority is substantially modeled already. `TargetId` binds a semantic target to surface and generation; `NavigationViewportBinding::base_is_current` checks viewport, focus, modal and layer ownership. Root query-focus guards explicitly include action sheets, prompts, settings, capture owners and unsupported nested note owners. These guards are counterevidence to any blanket claim that focus is uncontrolled. [I1], [I2], [I3]
6. Settings updates already have a settings-owned transaction lock, committed-value publication convention, and a checked digest/readback path. Atomic save has intentionally different durable and replaceable-state policies. In-process serialization is not proof of exclusion against an external writer, and neither save helper's name proves complete crash durability. [D1], [D2]
7. Activity reporting explicitly distinguishes `ActivityLifecycle` from `ActivityRetirement`. `ReapPermit::reap_with_callback` documents that retirement callbacks follow an actual join. These are reuse candidates when comparing worker classes, not justification for installing one new framework everywhere. [L1], [L2]
8. Table processing already has a separate `multi_launcher_table_core` package with an application re-export facade. The audit must account for this existing boundary rather than proposing to extract it again. Root manifests also declare grouped integration targets while retaining auto-discovered top-level tests. [B1], [B2], [B3]

## Coverage plan, not an achieved coverage claim

The author owns the exhaustive mapping from tracked first-party modules to the final coverage matrix. Group related leaf plugins where their interaction is the same, but retain explicit coverage for their entry points. Assign one of `DEEP_TRACE`, `TARGETED_REVIEW`, `SURVEY_ONLY`, `OUT_OF_SCOPE`, or `UNAVAILABLE` to each meaningful subsystem **after** inspection. The table below gives the intended minimum depth and reason; it does not certify a subsystem safe.

| Family and real source roots | Intended depth | Reason and boundary |
|---|---|---|
| Bootstrap: `src/main.rs`, `startup.rs`, `startup_catalog.rs`, `gui/mod.rs`, `platform/app_data.rs`, `platform/single_instance.rs` | Deep trace of construction/transfer/shutdown; targeted platform read | Establish process/thread owners, initial catalog, services and persistence root. |
| Query and commands: `actions/`, `commands/`, `plugin.rs`, `plugins_builtin.rs`, `plugins/`, `gui/search.rs`, `completion.rs`, `common/query.rs`, `usage.rs`, `history.rs` | Deep shared contracts; targeted representative plugins; survey remaining leaf plugins | Highest-frequency routing and compatibility boundary; inspect both success and refusal. |
| Universal Actions and alternate surfaces: `universal_actions/`, `gui/universal_action_*`, `gui/radial_actions.rs`, `radial/`, `dashboard/`, macro/Hot Folder adapters | Deep admission/identity/activation traces; targeted authoring and widget cache lifecycle | Same action can arrive with different context, persistence and unattended authority. |
| Input: `gui/render.rs`, `gui/jump_mode*`, `gui/numpad_navigation.rs`, `gui/query_history.rs`, `hotkey*`, `visibility.rs`, `launcher_parking.rs`, `active_window.rs` | Deep representative owner transitions; targeted each separate native authority | Root, dialogs, viewports, native pickers and foreground ownership must not be conflated. |
| Settings and data: `settings/`, `settings_editor*`, `settings_save.rs`, `common/persistence.rs`, `common/atomic_file.rs`, `persistence/`, `gui/watch.rs`, `common/*watch.rs` | Deep transaction/load/recovery/live-apply trace | Publication, malformed-data handling and competing edits cross owners. |
| File mutation: `file_operations.rs`, `batch_rename*`, `search_replace.rs`, `replace_executor.rs`, `diff/file_ops.rs`, `archive/`, `target_repair.rs` | Deep one real apply/undo/recovery path; targeted remaining mutation paths | Confirmation is not file-identity authority; verify actual current native guards. |
| Worker/session infrastructure: `activity.rs`, `thread_reaper.rs`, `indexer*`, `file_search/`, `table_utility_session.rs`, `xml_toolkit_session.rs`, `duplicate_finder/`, `image_compare/`, `brightness_service*` | Deep lifecycle table for each major worker class; targeted algorithms | Distinguish stale-result fencing, cancellation, terminal publication, join and resource release. |
| Automation and clipboard: `mkmacro/`, `hotfolder/`, `gui/hotfolder_*`, `paste_sequence*`, `clipboard_modify*`, `plugins/clipboard.rs`, snippet owners | Deep invocation/admission/cancellation/publication; targeted authoring/store | Unattended eligibility, input leases, external changes and retained metadata. |
| Capture/overlays: `gui/screenshot_*`, `pinboard*`, `ocr*`, `screen_draw/`, `coordinate_tool/`, `color_pick/`, `image_crop.rs`, `annotation/` | Deep lifecycle edges for screenshot/pinboard/OCR; targeted other tools | Native resources, root parking, close/reopen, single-frame and output ownership. |
| Notes and structured user records: `plugins/note.rs`, `plugins/todo.rs`, `plugins/snippets.rs`, `notes_markdown/`, `note_todo_sync/`, `note_ui_state.rs`, `linking/`, `graph/`, corresponding dialogs | Targeted persistence, identity and input seams; survey pure transforms | Avoid overlooking file content, cross-record identities and UI authority while limiting document-editor scope. |
| Read-heavy utilities: File Search, table/CSV core, Duplicate Finder, Diff, hash, XML, JSON, regex, QR, image compare | Targeted each session/output boundary; survey pure algorithms unless a trace exposes risk | Cover every requested utility without assuming they need one generic engine. |
| Windows/window/workspace services: `window_*`, `windows_layout/`, `virtual_desktop/`, `multi_manager/`, `workspace.rs`, `mouse_gestures/`, `platform/`, `process.rs`, `global_hotkey.rs` | Targeted owner, identity and shutdown contracts | Existing workspace functionality is inventoried; no new workspace feature is in scope. |
| Pure helpers and presentation: `text_transform/`, conversion/date/color/QR helpers, theme/render helpers, sound/help/editors | Survey; escalate only a concrete cross-boundary dependency | Lower-risk local computation/presentation remains in the inventory. |
| Diagnostics/test/build support: logging/toasts/performance, `tests/`, `benches/`, `dev/`, `tools/cursor_effects_smoke/`, `build.rs`, manifests | Targeted provenance, fixture and dependency inspection; survey harness implementations | Source-only contracts; never execute historical harnesses. |
| `vendor/`, generated files, dependency code | Out of scope for bulk review; identify any directly relied-upon local patch seam | Root Cargo patches affect runtime semantics. A claim depending on a patched eframe/egui seam needs a targeted read or an explicit limit, not assumed upstream behavior. |

Use `src/lib.rs`, root/module declarations and tracked file inventory to catch omissions. The root workspace contains the app and table-core member; the standalone cursor-effects tool is separately inventoried. Module imports, runtime calls, ownership and message delivery must be labeled as different graph edges. Macro expansion, trait dispatch and `cfg` branches limit a static graph.

## M1-A — Source map and coverage inventory

**Objective:** produce `SOURCE_MAP.md`, `COVERAGE_MATRIX.md`, and the initial component/ownership portion of `ARCHITECTURE_AND_DATA_FLOWS.md`.

**Why / ownership:** this checkpoint establishes the research navigation and coverage contract; it does not issue conclusions for untraced code.

**Current-state facts:** the library exposes the broad owner inventory; the root manifest names binaries/features/test targets and the table-core member. `startup_indexed_actions` waits for and validates an exact generation/config completion before returning a coordinator for GUI ownership; `startup_action_catalog` preserves custom-first/indexed-tail assembly. [B1], [B2], [B3], [A1]

**Scope:** root and first-party manifests; `src/lib.rs`; `src/main.rs::main` and GUI worker construction; `src/startup.rs`; `src/startup_catalog.rs`; `src/gui/mod.rs::LauncherApp`; module-level inventory across the coverage families above. Inventory supporting `tests`, `dev`, tools and native adapters without running them.

**Required changes:**
1. Produce a tracked-file/module/package inventory against the fixed source SHA. State exclusions and method; do not interpret an `rg` hit as a resolved call edge.
2. Trace startup preload → initial catalog/coordinator → plugin/service construction → `LauncherApp` and main/GUI shutdown ownership. `main.rs` navigation points are GUI worker setup around 3993–4223, `main` at 4225, startup catalog call at 4303, and main-loop retirement at 4644–4665 and 6236–6251; read these bodies before making substantive claims.
3. Map stable facades and adapters, including command hosts, plugin snapshots, table-core re-export and persistence root. Give path, symbol, pinned source link and edge meaning for each load-bearing relationship.
4. Fill every subsystem coverage row with actual depth and reason; route deeper work to the later packets. Add the initial component diagram.

**Invariants:** pinned source; docs-only output; truthful achieved depth; no revival of the prior implementation queue.

**Non-goals:** source fixes, full Rust call-graph resolution, latency/build measurements, Goal 02 design.

**Dependencies:** accepted M0-A; fixed MASTER_START and output root.

**Tests:** inspect target declarations and representative fixture entry points only. No tests are changed or run.

**Verification:** resolve cited paths/symbols with `git show`/`rg`; check every `src/lib.rs` module and first-party manifest has a matrix disposition; inspect docs-only diff. Each text/document check stays within 60 seconds.

**Done criteria:** real source map and component flow exist; no significant inventory family is silently omitted; survey/unknown edges are explicit; citations are pinned. Parent checkpoint: `docs(audit): [G01/M1-A] map owners and define coverage`.

**Genuine uncertainties:** platform-conditional/native ownership and dynamic plugin dispatch require limited body inspection; inventory cannot prove native behavior or actual runtime load.

## M1-B — Command, search, and input contracts

**Objective:** complete command/query flows in `ARCHITECTURE_AND_DATA_FLOWS.md`, produce `INPUT_AND_UI_OWNERSHIP.md`, and record evidence-backed initial findings.

**Why / ownership:** equivalent-looking surface actions must preserve action identity, payload semantics and appropriate execution authority. Current command/search/input owners remain the reference boundaries.

**Current-state facts:** preserve C1–C3, S1–S2, P1–P2, U1 and I1–I3 above. GUI search clears result selection even on its cached return. Literal query handling is conditional on an explicit matching prefix. Universal Action confirmation has real radial revalidation; do not infer all surfaces have identical revalidation. [S2], [P2], [U1]

**Scope:** `commands/{model,parser,bus,outcome,host,headless}.rs`, `gui/{actions,command_host,search,universal_action_executor,universal_action_catalog,render,query_observation,action_sheet,confirmation_modal}.rs`, `universal_actions/`, `gui/jump_mode.rs`, `gui/jump_mode/ui.rs`, `hotkey/`, `gui/radial_actions.rs`, dashboard activation and macro/Hot Folder adapters.

**Required changes:**
1. Trace query intake → prefix/filter/literal handling → provider/local ranking → result installation → selected `Action` → parser/confirmation/bus/outcome. Document explicit `app`, `g`, empty browse, pending provider and autocomplete semantics as inspected, not generic assumptions.
2. Compare Enter, mouse, dashboard, radial, macro, hotkey/gesture and unattended Hot Folder routes. Distinguish dispatch source, presentation surface, captured query, saved identity, capability and confirmation policy. A refusal is a valid route outcome.
3. Trace refresh ticket admission/publication and provider/catalog revisions into current GUI/radial query acceptance. Explain where selection is cleared, preserved or re-resolved; require a concrete absent guard before calling something stale-executable.
4. Build an input transition table covering hidden/visible root, text editing, dashboard, modal, native picker, external foreground and child viewport. For Jump Mode trace leader capture, hint resolution, owner/generation retirement, release fencing and fresh Enter. Read root owner selection at `render.rs::jump_mode_navigation_owner_candidate`/`resolve_jump_mode_navigation_owner`, and `JumpModeUi` in `gui/jump_mode/ui.rs` rather than treating hint allocation as the whole feature.
5. Write AUD-001 below with its limits. Separately inspect dashboard `plugin_home::search_plugin_actions` and `pinned_query_results` filter paths: they directly split filters before provider calls. Determine their supported query contract before deciding whether divergence from `PluginQueryPolicy::Literal` is a defect. This is an investigation target, not an accepted second recommendation. [Q4]

**Invariants:** preserve legacy encoded Action compatibility and original arguments; do not equate display label with stable identity; current explicit prefixes and literal policy remain authoritative; no native-focus or race-freedom claims from static tests.

**Non-goals:** new command bus, replacement UI authority framework, new ranking algorithm, production repair.

**Dependencies:** M1-A owner map; no Goal 02 work.

**Tests:** inspect `tests/plugin_routing.rs`, `tests/plugin_exact_match.rs`, `tests/domain_cases/{command_bus_architecture,ranking,selection}.rs`, `src/plugin.rs` literal/snapshot tests, `gui/universal_action_executor.rs` parity/confirmation tests, and relevant Jump/query tests in `gui/render.rs`. Tests are contract evidence only.

**Verification:** source/caller/counterguard cross-check for each substantive claim; pin source ranges; give every initial finding a disposition. No application execution.

**Done criteria:** real cross-surface comparison and input transition table; at least representative rejection, replacement and cancellation paths; existing safeguards and unresolved questions separated; initial findings have evidence/limits. Parent checkpoint: `docs(audit): [G01/M1-B] trace command and input ownership`.

**Genuine uncertainties:** native timing/focus remains unverified; dashboard literal behavior needs product/source contract analysis; internal-versus-headless eligibility may intentionally differ.

## M2-A — Lifecycle, persistence, and file safety

**Objective:** produce `LIFECYCLE_AND_CONCURRENCY.md`, `PERSISTENCE_AND_FILE_SAFETY.md` and substantive finding records for justified concerns.

**Why / ownership:** workers and mutation owners must define their own acceptance, cancellation, authority, cleanup and publication contracts. Activity UI is an observer/cancel-capability consumer, not proof of resource retirement.

**Current-state facts:** activity and retirement are separate types; the reaper has an actual-join callback. File Search allocates a fresh SearchId/token, filters events by current ID, and emits a GUI cancellation event without waiting for worker return. OCR cancellation retains its terminal receiver; `Drop` requests cancellation. These facts alone do not prove a leak or dangerous race. Settings has an existing transaction owner. [L1], [L2], [L3], [L4], [D1]

**Scope / source entry points:**
- Worker comparison: `indexer/coordinator.rs`, `file_search/coordinator.rs`, `diff/{worker,folder_runtime,watch}.rs`, `table_utility_session.rs`, `xml_toolkit_session.rs`, `duplicate_finder/session.rs`, `image_compare/session.rs`, `brightness_service.rs`, `activity.rs`, `thread_reaper.rs`.
- Clipboard/automation: `clipboard_modify/{coordinator,runtime,clipboard}.rs`; `paste_sequence/admission.rs::PasteAdmissionBridge`, `paste_sequence/playback.rs::SequencePlaybackController`; `mkmacro/{runtime,invocation,executor,launcher_query,launcher_command}.rs` and `runtime/summary.rs`; `hotfolder/{runtime,capability,execution}.rs` and `gui/hotfolder_dispatch.rs`.
- Capture/output: `gui/{screenshot_capture,screenshot_countdown,pinboard,ocr}.rs`, `pinboard/live.rs`, `ocr/job.rs`, corresponding native owners.
- Persistence/mutation: `settings/{store,model,io}.rs`, `settings_save.rs`, `settings_editor/save.rs`, `gui/watch.rs`, `common/{persistence,atomic_file,json_watch,repair_watch}.rs`, `persistence/{catalog,backup,recovery,data_service}.rs`, `file_operations.rs`, `batch_rename_transaction.rs`, `replace_executor.rs`, `diff/file_ops.rs`.

**Required changes:**
1. For each major worker class, record owner; request/generation identity; admission limit; queued/running/cancelling/terminal state; cancellation signal; completion path; retirement evidence; cleanup/drop/shutdown owner. Write `not provided by this owner` or `not inspected` where appropriate. Do not infer stopped from hidden or cancelled.
2. Deep trace representative replacement, close/reopen and shutdown paths. Compare File Search's stale-ID rejection with its terminal/cancellation handling; inspect its executor/backend stop guarantees before recommending lifecycle changes. Contrast with retained-session workers and actual reaper joins.
3. Trace settings draft → validation → owned save intent → atomic commit → accepted state/live services → watcher reconciliation. Inspect `SettingsSaveIntent`, worker registry and `begin_settings_save`/`drain_settings_save`; do not recommend a global save coordinator without testing this existing boundary on paper.
4. Trace one real rename/replace apply, rejection, interruption and undo/recovery sequence through `file_operations` native identities and the transaction owner. Separate approved preview content, native source identity, destination identity, scratch ownership and journal authority. Determine whether relevant historical Goal 09/12 candidates are present on MASTER_START before using their reports.
5. Trace Macro/Hot Folder capability and unattended authorization, nested invocation, pause/stop and summary retention; Clipboard/Paste own-versus-external publication and input leases; capture/editor/output ownership and pin/OCR close/cancel. Identify privacy data classes from code only, without opening user data.
6. Record concrete adverse sequences, inspected counterguards and minimum future proof. Consult official API docs only for a load-bearing platform contract; if unavailable, qualify it. A runtime hypothesis stays an investigation/spike until supported.

**Invariants:** distinguish result rejection from retirement; no destructive experiments; no raw user payloads in evidence; preserve durable-versus-replaceable policy distinction; no historical blocked goal becomes a present defect by assertion.

**Non-goals:** generalized task scheduler, new filesystem framework, native acceptance campaigns, blanket redesign of all session owners.

**Dependencies:** M1-A inventory, M1-B activation/authority context. Unresolved native questions need not block unrelated documentation.

**Tests:** inspect inline lifecycle/source-identity tests and `tests/domain_cases/{file_search_state,file_search_e2e,diff_sessions,diff_file_ops}.rs`, `tests/{watcher_failures,clipboard_persistence,mkmacro_runtime,mkmacro_launcher_integration,xml_engine_gate}.rs`. For future app fixtures preserve `next_hotkey = None` and `cancel_hotkey = None` before construction; production defaults remain unchanged. Run nothing now.

**Verification:** source/caller/guard review and bounded path/link checks; any finding must state evidence type, assumptions and future proof. No build/native checks.

**Done criteria:** lifecycle table covers the major classes; save/recovery and real mutation flows are evidenced; every major concern has mechanism/counterevidence/confidence; no resource-retirement guarantee invented. Parent checkpoint: `docs(audit): [G01/M2-A] document lifecycle and persistence risks`.

**Genuine uncertainties:** native guarantees, scheduler interleavings and real resource cost remain unverified; transaction/native backend bodies and their existing counterguards require the author's scoped deep trace.

## M2-B — Change cost, alternatives, and prioritization

**Objective:** produce `BOUNDARIES_AND_CHANGE_COST.md`, `DECISION_RECORDS.md`, reconciled `FINDINGS.md`, and `RECOMMENDATIONS.md`.

**Why / ownership:** convert source observations into proportionate engineering choices; do not turn every coupling or large file into a refactor mandate.

**Current-state facts:** table-core extraction/facade already exists. Source inspection reveals grouped tests and explicit isolated app/plugin fixtures; `PluginManager::new` installs a shared macro store, while `new_inert_for_test_at` avoids that path and uses an empty store. This is a meaningful test seam to preserve. [B1], [B2], [B3], [T1], [T2]

**Scope:** manifests/features, `src/lib.rs`, table facade/core, representative owners and test fixtures already reached in M1/M2. Inspect diagnostics/history retention only where it supports a concrete hypothesis.

**Required changes:** compare no change, existing-owner repair and narrow extraction for each structural candidate. Document current coupling/call frequency/data size only when derivable. Separate source size, dependency exposure, test setup and measured compile/runtime cost. Use historical receipts only with original tree and applicability. State minimal later measurement and rejection condition for each performance hypothesis. Reconcile findings and prioritize separately by severity, confidence, breadth, user value and validation cost. Reject unsupported wish-list items explicitly; retain useful sound-pattern conclusions.

**Invariants:** no invented savings; no duplicate extraction of table core or typed bus; preserved APIs/defaults/serialization; no quota of findings.

**Non-goals:** crate-per-feature campaign, global test-fixture rewrite, historical benchmark execution, Goal 02 ranking evaluation.

**Dependencies:** M1-B and M2-A mechanisms and counterguards.

**Tests:** inspect fixtures and existing target structure only. Static case inventory is not a test result.

**Verification:** every recommended action traces to evidence and a proportional scope; every rejected/conditional item has rationale. Check source/pinned links and qualitative labels once.

**Done criteria:** alternatives and reversal conditions are recorded; no static performance assertion masquerades as measurement; proposed work is dependency-safe and ready for packaging. Parent checkpoint: `docs(audit): [G01/M2-B] prioritize evidence-backed architecture work`.

**Genuine uncertainties:** edited-target build/link and runtime cost require future measurement; no additional architectural extraction is accepted by this planner.

## M3-A — Follow-on library and search handoff

**Objective:** canonical complete Goal/Kickstart pairs, `FOLLOW_ON_INDEX.md`, and `SEARCH_HANDOFF.md`.

**Why / ownership:** findings become executable future contracts; the next research goal consumes validated owner constraints without replaying this audit.

**Current-state facts:** one planner package, AUD-F001, is supplied; all other questions above are unaccepted audit targets. The actual current dispatch/search/filter/input owners already constrain future work.

**Scope:** role proposals, canonical `follow-on/`, finding/decision/recommendation index and search handoff. No code changes and no Goal 02 experiment.

**Required changes:**
1. Consolidate proposals by mechanism/owner, retaining role attribution and objections. For each accepted actionable recommendation publish a substantive pair or an exact existing-plan delta. An unproven platform issue gets a bounded investigative decision goal, not an unsupported fix.
2. Review AUD-F001 against the raw-token finding below; retain its low severity/high source confidence and constrained impact. Promote only after checking package links and current source assumptions.
3. Reconcile findings → disposition → canonical pair → author contribution in both directions. No-action/duplicate items need clear reasons, not filler packages.
4. Give Goal 02 the existing search ordering/filter/prefix/literal rules; Action wire compatibility; usage identity; availability/confirmation/source-versus-surface rules; provider pending/revision/catalog fencing; UI selection/focus authority; saved-action identity and persistence contracts; privacy/retention limits. Distinguish validated constraints from source targets still requiring revalidation.
5. In particular preserve explicit `app`/`g` routes and the enabled/capability filter, the pending/result distinction, and exact selected `Action`/args. Do not silently replace selected action identity with a display label or ranking position. The author must verify usage/history retention and unattended Hot Folder eligibility before presenting those as settled constraints.

**Invariants:** every package remains `PROPOSED_NOT_AUTHORIZED`; future base is then-current accepted master/dependency, not the historical research SHA; all factual provenance stays pinned; no automatic follow-on execution.

**Non-goals:** implementing AUD-F001, creating Goal 02's branch, prototype/ranking work, speculative recursive package queues.

**Dependencies:** reconciled M2-B decisions; parent's canonical ID ownership.

**Tests:** packages specify future tests and expected observations; no tests run now. The documentation must not say an unexecuted case passed.

**Verification:** cheap link/path/ID checks, complete-pair check, bidirectional finding mapping, docs-only diff. Parent performs final M3-B review separately.

**Done criteria:** all accepted work has an implementer-ready pair; duplicates/unknowns are explicit; search handoff names actual owners and compatibility boundaries; no unresolved template instructions. Parent checkpoint: `docs(audit): [G01/M3-A] package follow-on goals and search constraints`.

**Genuine uncertainties:** new findings may emerge during the author's traces; they are added only with sufficient evidence and a complete package. This plan mandates no finding count.

## Planner finding AUD-001 — Terminal escape is duplicated in RawToken.raw

**State/category:** observed source inconsistency / query correctness. **Severity:** low. **Confidence:** high for the raw-token invariant; runtime impact/frequency unmeasured. **Disposition:** proposed `READY_FOR_IMPLEMENTATION_REVIEW`, via AUD-F001; not authorized to execute.

`tokenize_query_with_raw` appends a backslash to `raw` when seeing it and sets `escape = true`. If that escape is still pending at end of input, it appends another backslash to `raw` as well as the necessary literal backslash to decoded `value`. Thus input bytes `78 5C` (letter x followed by one backslash) yield raw bytes `78 5C 5C`. The raw spelling cannot be an exact copy of that input token. This is direct source reasoning, not a Rust reproduction run. [Q1]

`split_action_filters` forwards remaining `token.raw` values, so the extra character reaches ordinary filtered search input. Normal and snapshot plugin searches use this split path. [Q2], [P2], [P4] Snippet tag-filter queries also reconstruct from `raw`. The snippet parser's no-tag branch returns its original input; matching-prefix Literal plugins receive original query text rather than this reconstruction. A terminal *pair* of backslashes clears the escape flag before EOF and is not this defective branch. Do not claim every Windows path, every plugin or every snippet is corrupted. [P2], [Q3]

The existing `split_action_filters_preserves_query_tokens` test covers quotes and filter removal but not pending trailing escapes. [Q5] The existing QR sync/snapshot test is counterevidence showing an intentionally literal route; retain its behavior. [P3]

**Recommended existing-owner correction:** restore exact `RawToken.raw` spelling at EOF while preserving decoded `value`, quote/escape grammar, filter removal/order, normalized filter values and Literal query behavior. Add small pure regression cases plus one inert sync/snapshot provider test and the directly affected snippet-tag case. No new parser abstraction or global query-policy migration is needed. Expected benefit is a corrected byte-preservation invariant, not measured latency or a claim of reproduced native path failure.

**Alternatives/reversal:** no change retains a direct raw/value inconsistency; repairing callers duplicates the same normalization bug; a parser rewrite expands compatibility risk. If the current implementation is already corrected when authorized later, retain or add only missing regression proof and record a source-backed no-op.

## Source references

Every link pins `59156d96adcc73939c7171487a0cb582e3286587`. Navigation-only locations named in packets still require body inspection before the author publishes a claim. Parent applied the planner's explicitly returned L3 SHA correction and P4/Q5 citation additions before persistence.

[C1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/commands/model.rs#L13-L104
[C2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/commands/bus.rs#L14-L100
[C3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/actions.rs#L113-L187
[S1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/search.rs#L1395-L1448
[S2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/search.rs#L802-L985
[P1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L101-L158
[P2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L1290-L1437
[P3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L1693-L1709
[P4]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L535-L590
[U1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/universal_action_executor.rs#L84-L239
[I1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/jump_mode.rs#L21-L119
[I2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/jump_mode/ui.rs#L416-L485
[I3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/render.rs#L1889-L1964
[D1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/settings/store.rs#L8-L87
[D2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/common/atomic_file.rs#L7-L56
[L1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/activity.rs#L201-L240
[L2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/thread_reaper.rs#L231-L318
[L3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/file_search/coordinator.rs#L225-L398
[L4]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/ocr/job.rs#L250-L276
[B1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/Cargo.toml
[B2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/table_utility.rs#L1-L7
[B3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/crates/multi_launcher_table_core/Cargo.toml#L1-L9
[A1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/startup_catalog.rs#L7-L125
[T1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L601-L645
[T2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/test_fixtures.rs#L61-L109
[Q1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/common/query.rs#L113-L173
[Q2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/common/query.rs#L230-L263
[Q3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins/snippets.rs#L407-L445
[Q4]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/dashboard/widgets/plugin_home.rs#L47-L54
[Q5]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/common/query.rs#L417-L424
