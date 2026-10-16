# Workspace Clippy cleanup — October 16, 2026

## Contract and baseline

- User requests all reported workspace Clippy warnings resolved idiomatically, preserving functionality, performance and public APIs wherever possible; structural warnings need deliberate ownership choices.
- Explicit exclusion: do not handle or modify any vendor crate. Vendor dependency warnings are reported separately, not silenced through configuration or source edits.
- Branch: `codex/clippy-cleanup-20261016` from `4b5f96bf7f42fedf305a341995a0ecd184392f6a`.
- Pre-existing user work: untracked root `clippy_output.txt`,731200bytes, SHA256 `345b82956b6e7f7a1533eea0f937c901d3371edff5bfc5af5230a52a2877f6e9`. Preserve; never stage/edit/delete.
- Supplied log is UTF16. Inventory records542 distinct located owned diagnostics, including170collapsible_if,97too_many_arguments and17large_enum_variant. Vendor image summary9 is excluded; no vendor primary diagnostic blocks are in the supplied log. Inventory: ignored `target/clippy-cleanup-20261016/input-inventory.json`.
- Cargo metadata: sole workspace member is `multi_launcher`; vendor paths are patched dependencies, not workspace members. Features: `unstable_grab`, `qa-framebuffer-capture`.
- Required final gates: `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo nextest run` (full suite explicitly authorized for this task). Adjust only for demonstrated repository configuration constraints and document why.
- No pushes, history rewrites, discarded user work, blanket lint suppressions, or unrelated changes. Narrow documented exemptions only if demonstrably necessary to preserve an established API/performance contract.
- Existing Planner/Implementer/Reviewer reused; one source writer at a time. Root owns this ledger, Git checkpoints and final reporting. Earlier UI-polish original-recording blocker is separate and remains unresolved.

## Execution plan

| Checkpoint | State | Scope / evidence |
|---|---|---|
| CL1-A | complete | Eight equivalent Atomic::fetch_update→try_update substitutions in six owners; preserve orderings/callback/Result semantics; four existing targeted tests |
| CL1-B | complete | Core/native idioms; preserve pixel remainders, cfg branches, side effects and test instrumentation |
| CL2-A | complete | Preserve eight exported enum representations and exact authoring reply error contract with individually documented necessary exceptions; additive Default/is_empty |
| CL2-B | complete | Private GUI intent/error payload ownership |
| CL2-C1 | pending | Allocation-free native effects cache/views plus deferred coordinate render idiom |
| CL2-C2 | pending | Allocation-free synchronous trace publication views |
| CL3-A | pending | MkMacro domain/runtime idioms: 29 sites / 14 files |
| CL3-B | pending | MkMacro GUI/editor/overlay idioms: 63 sites / 19 files |
| CL3-C1 | pending | Radial runtime/resources/evidence idioms: 49 sites / 12 files |
| CL3-C2 | pending | Radial Designer/editor idioms: 45 sites / 6 files |
| CL3-D1 | pending | Remaining presentation/compiler idioms: 52 sites / 26 files |
| CL3-D2 | pending | Remaining routing/plugins/integration idioms: 29 sites / 11 files |
| CL4 | pending | Acceptance/smoke idioms followed by argument ownership at existing native-click, trace-observation and suite-run boundaries |
| CL-QA | pending | Final formatting, strict all-workspace/all-target/all-feature Clippy, full Nextest, cumulative review and warning reconciliation |

### CL1-A handoff

Files: `src/performance/track_c.rs`, `src/screen_draw/recovery.rs`, `src/gui/screen_draw_toolbar.rs`, `src/gui/event_channel.rs`, `src/gui/mod.rs`, `src/thread_reaper.rs`.

Replace only the eight deprecated atomic method names. Preserve both memory orderings, callbacks, saturation/overflow behavior and returned-result handling. No new tests or payload/layout/cfg changes. Verify existing event depth/race/reaper cases, formatting and diff; commit a coherent checkpoint before the next source batch.

Record actual commands, test IDs/counts, lint residuals, structural decisions and commits below. No checkpoint is complete merely because code was written.
### CL1-A verification

- Exact eight method-name substitutions; both orderings, callback bodies, return handling and saturation/overflow behavior unchanged. Root inspected the full six-file diff.
- `cargo fmt --all -- --check` and `git diff --check`: pass.
- `cargo clippy -p multi_launcher --lib --message-format=json`: exit 0; 354 existing warnings, zero primary spans on changed lines. Diagnostic JSON: `target/ui-polish-evidence/cl1a-clippy-20261016.jsonl` (historical evidence directory used by Implementer for this first packet only).
- Targeted library Nextest union: four event-depth/race/reaper cases pass; 5367 skipped. Run ID `5748ccdf-3e21-4f40-9cfe-91a6a29e7d9a`.
- Commit subject: `refactor(core): [CL1-A] migrate deprecated atomic updates`. Hash recorded in next ledger update to avoid self-reference.
CL1-A commit: `6e1224ee`.

### CL1-B handoff

Own 18 files / 60 original warning sites: actions_editor; commands/parser; unit_conversion; platform/pixels; color_pick/native; coordinate_tool capture/controller/native/native_effects/render; persistence backup/recovery; screen_draw controller/model/native_canvas/raster/recovery/window_layers. Only listed idioms (conditional/return/borrow simplifications, numeric/byte literal formatting, allocation-free fixed pixel chunk views, exact derived defaults, private conversion-helper naming) are authorized. Native effects excludes payload layout. Preserve partial pixels, channel/premultiplication arithmetic, cfg returns, cleanup ordering, guard lifetimes and serialized defaults.

Targeted Nextest union covers premultiplication, stable serde defaults, GDI subpath breaks, capture cancellation, stationary effect refresh, halo fallback, toolbar layering, partial backup/cancel recovery, unit conversion and command parser tests. Run formatting and all-feature library diagnostic Clippy; reconcile warning sites. Full strict gates remain final.

### Structural pre-review decisions

Reviewer confirmed RecordingBoundary, ObservationResult, DataServiceResult, DocumentMutation, AuthoringRequest, ControllerEvent, DeferredResolutionResult and DispatchIntent are externally reachable through library exports. Integration tests directly construct these variants. Their nested payloads also expose fields; boxing changes public construction and owned pattern APIs. Preserve these contracts with individually justified documented lint exemptions only where a source-compatible structural fix is unavailable. This is an explicit necessary exception, not blanket lint suppression or an assertion of structural remediation. RecordingBoundary also retains enriched context per accepted hook event (default on); boxing would add event and normalization-clone allocations.

Private payloads and disconnected-send errors remain candidates for genuine layout/error-path fixes while preserving unsent payload recovery. Native EffectLiveSource cache is on the per-sample path; avoid per-refresh Box/Arc allocations. Planner and Reviewer will assess typed owned storage with borrowed source views against a narrow documented inline-layout exemption before implementation. Public AuthoringReplySender::send preserves its exact SendError contract. Vendor is never changed.
### CL2 executable sequence (Planner, independently pre-reviewed)

- **CL2-A**: exact-item documented large-enum exceptions for the eight exported enums above; exact-function large-error exception for AuthoringReplySender::send. Add WindowsNotificationBackend::Default delegating to new, and cache is_empty methods matching len. Verify public test constructors, radial cache/font-cache tests and registered authoring owner wakeup. No private enum exceptions in this packet.
- **CL2-B**: gui state/event_channel/render/radial_actions and radial_editor mod/action_editor. One boxed private Test payload moves from DesignerUiIntent into AuthoringProviderSearchPurpose; preserve identity separately. Box only rejected queue/search and failed event-send returns, retaining exact unsent payload and depth/disarm/metadata rollback. Box deepest ring proposal first and remeasure outer intent; context-menu Execute boxes only on click. Verify queue cancellation/returned identity, stale-scope rejection, context-menu parity and send/drop accounting; extend existing queue-full assertions if needed.
- **CL2-C1**: native_effects/native typed owned halo/zoom cache with short-lived borrowed view; preserve cached stationary refresh, failure clearing/topology/shutdown, fake receipt lifetime and no sample allocations. Verify existing stationary/topology/failed-sample cases.
- **CL2-C2**: private BudgetTraceEntry borrows synchronous event view; preserve fence ordering, exhaustion marker uniqueness, terminal reserves, sequence/serialized output. Tests retaining receipts copy owned evidence explicitly. Verify hard event budget and ordered exhaustion/terminal-boundary cases.

No later source writer overlaps an active packet. Each needs formatting, focused verification, diagnostic Clippy, reconciled warnings, reviewed diff and its own commit. If a private borrowed-view migration becomes disproportionate lint-only complexity, require concrete performance rationale and review before a narrow exception; do not add heap traffic silently.
CL1-B actual-scope reconciliation: 18 source files / 61 original sites. The two gui/render ScreenDraw restore/repark condition sites were included and reviewed; coordinate_tool/render.rs:667 remains deferred to CL2-C1. This replaces the initial 18-file/60-site packet's render path. Root and Reviewer found no substantive source concerns: complete-pixel/remainder semantics, cfg returns, defaults, cleanup ordering and conditional side effects unchanged. Capture boolean-condition RefCell temporary drops before driver.pump under edition-2024 operand scopes. Formatting/diff checks pass; all-feature library diagnostic Clippy exits 0. Targeted tests pending.
### CL3-A pending executable packet — MkMacro domain

14 files / 29 original sites under src/mkmacro: authoring_analysis, authoring_fields, executor/frame, image_search, package, recorder, recorder_observer, recorder_processor, recorder_semantics, reusable_validation, runtime, screen, store, validation. Local condition/match/return idioms, boolean filter_map, typed checked-session/matcher borrows, unit backend construction and exact Desktop default only; no GUI/public API/schema/actor redesign.

Recorder worker preserves commands > backlog > fresh events, one-event publication, 20ms receive timeout and session borrow release before command mutation. Boolean filtering preserves ordering and avoids cloning discarded steps. Store schema-10 vacant-entry boundary preserves directory creation -> save_atomic -> created-path record -> planned insertion, dedup/mappings, cleanup of only successfully created paths and schema advance only after success.

Focused library tests: pause_resume_excludes_prompt_key_and_balances_pre_pause_held_key; markers_and_annotations_keep_order_when_authored_during_pause; held_modifier_shortcuts_use_each_primary_keys_window_context; own_process_context_never_authors_window_activation; simplification_rolls_removed_time_into_destination_and_equal_time_stays_zero; schema_ten_migration_flattens_all_reference_forms_and_keeps_legacy_sources; schema_ten_missing_reference_is_explicit_without_blocking_other_assets; watcher_reload_cannot_publish_across_newer_save_transaction; import_remaps_all_relationships_and_resolves_case_insensitive_conflicts; injected_failure_rolls_back_only_new_assets_and_never_publishes_document; decode_cache_reloads_after_same_filename_overwrite; early_return_and_procedure_fallthrough_resume_the_exact_caller_position; replacement_is_atomic_and_current_can_edit_around_unrelated_errors; document_catalog_has_real_parameters_calls_ui_reads_unset_and_disabled_writes. External mkmacro_recorder target: contextual_sequence_has_exact_activation_coordinates_and_semantic_key_presses and context_opt_out_preserves_screen_targets_delays_and_matching_primary_keys. Format, diagnostic Clippy, reconcile sites/diff, commit before GUI packet.
### CL1-B terminal verification

- `cargo fmt --all`, `git diff --check`: pass; no new tests needed for idiom-only changes.
- `cargo clippy -p multi_launcher --lib --all-features --message-format=json`: exit 0; evidence `target/clippy-cleanup-20261016/cl1b-clippy.jsonl`. 293 warning diagnostics overall; only three primary spans in modified files, all later-owned existing warnings (native EffectLiveSource and GUI ContextMenu large variants, GUI clone-on-Copy). No CL1-B warning sites remain.
- Exact planned library Nextest union: 65 passed, 5306 skipped; run ID `1db4fec3-03db-479b-8bbc-8cc46b47091c`, exit 0.
- Independent Reviewer: no substantive findings; Root inspected initial pixel/conversion/parser/default and capture/recovery/GUI restore hunks, confirmed scoped diff. Source is 18 files / 61 reconciled original warning sites.
- Commit subject: `refactor(core): [CL1-B] clean native and persistence idioms`; hash in next update.
CL1-B commit: `f94b5700`.

CL2-A source review: exact eight enum exemptions plus one sender method exemption; no broad/duplicate attributes. Exported variants/payloads and send signature unchanged. Default delegates existing initialization; is_empty uses same underlying map as len. Actual font-cache owner is FontLayoutService (Planner's RadialFontCache label was stale). Existing cache tests assert empty/nonempty states. Reviewer requested precise reason text: DataServiceResult large payload is SnapshotResult/inline manifest, not RecoveryStaged; sender explanation must state exact public SendError return type. Implementer correcting documentation. All-feature library diagnostic Clippy exits 0 with 281 warnings, remaining seven large enum and three large error warnings later-owned, no len_without_is_empty; tests pending.
Remaining executable Planner artifact: ignored target/clippy-cleanup-20261016/planner-remaining-packets.md. All 542 original owned sites assigned once: CL1-A 8; CL1-B 61; CL2-A 12; CL2-B 8; CL2-C1 2; CL2-C2 1; CL3-A 29; CL3-B 63; CL3-C1 49; CL3-C2 45; CL3-D1 52; CL3-D2 29; CL4 183. Exact C/D lint/line/column tables verified against 175 inventory entries after initial artifact formatting correction. Original locations are locators, not post-edit line numbers. CL4 includes all binary paths, including ../smoke, 97 argument-count sites, private binary enums/type complexity and remaining compiler/mechanical diagnostics. New warnings from final required command remain in scope.
### CL2-A terminal verification

- Corrected reason wording inspected by Root: exact public SendError contract and snapshot completion payload; one recorder attribute with allocation rationale. Nine source files / 32 insertions; no representation/signature changes, eight enum + one function exemptions only. Independent functional/API review accepted; reason corrections resolved.
- `cargo clippy -p multi_launcher --lib --all-features --message-format=json`: exit 0; evidence `target/clippy-cleanup-20261016/cl2a-clippy.jsonl`, 281 later-owned diagnostics. Zero len_without_is_empty. Seven large-enum / three large-error diagnostics remain outside this checkpoint.
- Three targeted library cases passed (run `d3a13d7c-0d6e-436e-aedc-532b4f062305`); recorder direct-public-constructor integration case passed (run `6f2584da-ca6c-4500-a33c-bd2b2c847fda`); authoring constructor case passed (run `b75232f2-3736-447e-aede-4ae8ec4a9f1d`). Five tests total.
- Final `cargo fmt --all -- --check` and `git diff --check`: pass. No Cargo process remains.
- Commit subject: `refactor(api): [CL2-A] preserve exported payload contracts`; hash in next ledger update.
CL2-A commit: `3db4078b`. CL2-B sole-writer packet authorized; no later source work.

CL2-A exact Nextest evidence: library union `test(cache_identity_includes_version_and_dpi_and_hits_are_shared) | test(cache_key_separates_purpose_wrap_constraints_width_and_dpi) | test(authoring_replies_wake_the_registered_designer_owner)` (3 pass / 5368 skip); `--test mkmacro_recorder -E 'test(ordinary_physical_input_remains_recordable)'` (1 pass / 6 skip); `--test mkmacro_authoring -E 'test(complete_authoring_recording_and_playback_workflow_uses_typed_intents)'` (1 pass / 8 skip). All commands use `cargo nextest run -p multi_launcher`; library uses `--lib`. No more constructor-only integration reruns warranted after these passed.
CL2-B source review: no substantive findings. Same Test box moves across identity checks, rejected queue request retains exact identity/query/token, eight existing queued entries/order/cancellation state intact. Failed event return retains original variant after accounting/disarm; click-only action box and deepest proposal box preserve deferred/stale-scope semantics. Necessary caller reexport in gui/mod.rs included.

Formatting correction: CL2-A's reported final format check was not consistent with the subsequent actual workspace check. CL2-B cargo fmt --all required wrapping seven allow attributes in four earlier API files (persistence/data_service and radial authoring/controller/handoff). Root inspected these as formatting-only, accepted them, and retained them rather than claiming a scoped check satisfies the workspace gate. Actual `cargo fmt --all -- --check` now passes, captured at target/clippy-cleanup-20261016/cl2b-fmt-check.txt. Future verification records terminal outputs/exits. No public signature/payload changes in these four files.
### CL4 pending executable sequence

Ignored Planner artifact target/clippy-cleanup-20261016/planner-cl4-packets.md assigns 183/183 binary sites and 97/97 argument owners: CL4-A 83 local/compiler idioms; CL4-B 9 borrowed run/environment sites; CL4-C 44 candidate/pointer interactions (natural subcommits pointer, candidate/query/hotkey, Designer/authoring); CL4-D 30 wait/selection/private Pin enum sites; CL4-E 17 report/artifact/parsed evidence sites. Exact owner names and scope tables verified against inventory/current source. Preserve serialized UNQUALIFIED spelling if renaming Rust variant; smoke reader stops/reports first read error rather than flatten-looping. Keep borrowed desktop/child/guard lifetimes, fresh pre-down validation, absolute deadlines, generic injectable callbacks and Some(empty) precleanup-attempt semantics. Focused binary tests only; no live desktop campaign solely for argument cleanup.
CL2-B iteration: compiler feedback identified three missed no-result fallback binding accesses, corrected to payload.binding with same stale/current/fallback checks (Root reviewed). First diagnostic Clippy exposed AddToRadial as the next large ContextMenuChoice variant after Execute boxing. Minimum necessary follow-up boxes its ActionBinding only at SaveLiveQuery click and row-selection helper whose sole production caller follows clicked/available admission; handoff moves the original binding out once. Reviewer accepted the delta, no idle/per-row allocation. Seven focused tests passed (run 27d989e5-193e-432f-9716-2e83fb1d16f3); narrow context-menu parity rerun plus final diagnostic lint pending. Full output target/clippy-cleanup-20261016/cl2b-nextest.txt.
New diagnostic reconciliation CL2-B: shrinking AuthoringProviderSearchRequest exposes large_enum_variant on gui/state.rs::WatchEvent (absent from supplied inventory; observed variants RadialDispatch 1024 B and RadialDeferredSearchReady 632 B). Reviewer verified public gui reexport, public send_event, direct external integration constructors/matches and nested public payload fields. Exact source-compatible layout change unavailable without altering public contract. Root authorized one individually documented item exemption under the same necessary API-preservation standard as CL2-A. This is the tenth public-contract exception, not an ignored inherited warning. No additional private suppression. Eight original CL2-B sites plus this new diagnostic must be reconciled before commit.

Test-generated clipboard_modifiers.json (baseline absent) matched exact known fixture SHA256 e8a60851b2043c2dc68ec51768bdb93f105949a938a762d0beb93244a3a53c65, 3666 bytes. After Cargo terminal, Root backed it up to ignored target/clippy-cleanup-20261016/cl2b-generated-clipboard_modifiers.json and removed only that duplicate. User clippy_output remains preserved.
### CL2-B terminal verification

- Eight original sites and newly exposed WatchEvent diagnostic reconciled. Ten narrow exported-contract exemptions cumulatively, each reviewed/documented. Private layout/error changes reviewed without findings; no successful-send error allocation or idle/per-row menu allocation.
- `cargo nextest run -p multi_launcher --lib` exact seven-filter union in target/clippy-cleanup-20261016/cl2b-verification.txt: 7 pass / 5365 skip, run 27d989e5-193e-432f-9716-2e83fb1d16f3. New full_authoring_search_queue_returns_rejected_request_without_mutating_pending_entries and strengthened event-failure payload assertions meaningfully protect recovery. After AddToRadial correction, context_menu_resolves_semantic_actions_with_list_grid_parity: 1 pass / 5371 skip, run 2f04fa1c-917e-46f4-a099-b627fb74a60d.
- Final `cargo clippy -p multi_launcher --lib --all-features --message-format=json --no-deps`: exit 0, 273 later-owned warnings; no assigned size/error or WatchEvent warning. Evidence cl2b-clippy-final-with-watch-event.jsonl / .stderr.
- Final `cargo fmt --all -- --check`: exit 0, evidence cl2b-final-fmt-check.txt. `git diff --check`: pass. No repeat tests for final attribute-only addition. All Cargo terminal.
- Commit subject: `refactor(gui): [CL2-B] right-size private intent payloads`; hash in next update.