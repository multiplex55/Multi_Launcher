# G08/M1 — Visible history with bounded reader ownership

2026-10-30. READY_FOR_ONE_WRITER after parent commits M0-A. This single coherent source checkpoint combines original M1-A UI and directly necessary M1-B tests; do not split it into tiny interface-only handoffs. Binding source/privacy/accounting contract: [HISTORY_CONTRACT.md](HISTORY_CONTRACT.md). Active [Goal](../../../../../../reliability-feature-closeout-20261029/goals/G08-mkmacro-run-history/GOAL.md) and [Kickstart](../../../../../../reliability-feature-closeout-20261029/goals/G08-mkmacro-run-history/KICKSTART.md).

## Objective and scope

Expose existing metadata-only terminal summaries in MkMacro's actual dialog through a short-held, budgeted frame projection, while preserving canonical history limits and execution behavior. The sole writer owns these source changes; the parent owns documentation/ledger/Git. Do not revert other contributors' changes.

Primary files:

- `src/mkmacro/runtime/summary.rs`: projection, accounting/lease and completed-only clear; retain no-Clone records and existing accumulator/publish semantics.
- `src/mkmacro/runtime.rs`: narrow view/clear gateway and typed runtime-generation RunKey; reuse `MacroRuntime::generation`, `RUNTIME_GENERATIONS` and existing summary lock. Do not rewrite admission or worker execution.
- `src/gui/mkmacro_dialog/history.rs` (new focused presentation module): allowlisted render formatting, bounded selection/filter/page state and tests.
- `src/gui/mkmacro_dialog/mod.rs`: module/state initialization, visible render and close integration.
- `src/gui/mkmacro_dialog/toolbar.rs`: a small History/Editor affordance using existing control/modal conventions. Register real response identities through the existing Jump target helper when adding a target, without changing Jump controller behavior.
- `src/mkmacro/mod.rs` only if a necessary type re-export is needed. Do not edit invocation/store/schema, runtime inspector's existing behavior, recipes/templates, recorder, settings or unrelated GUI.

Current relevant APIs are `SummaryHistory::{begin,reserve_active,evict_oldest,publish,with_completed}`, `MacroRuntime::with_run_summaries`, global `runtime::with_run_summaries`, `MkMacroDialog::{ui_with_jump_mode_synced,show_contents_with_table_viewport,close_children}`, `toolbar::show_with_jump_mode` and safe enum label methods. Existing global read helper holds locks through callbacks; do not render inside it. `runtime_inspector_snapshot` is payload-bearing and must not supply history.

## Ordered implementation

1. Establish `RunKey` using the existing runtime generation plus admitted run ID. Implement the narrow frame-view request/result/lease inside the history/runtime owner under the exact contract: 20 headers plus at most one selected header, 64 detail rows, 256-byte names, one active view, actual allocation/inline charge included in existing total bytes. No read-triggered eviction. Return a typed unavailable state if charge/slot cannot be reserved. Keep view bytes separate from `active_bytes` so a reader cannot make `begin` think a run is already active.
2. Project only allowlisted metadata under a short lock; release history and RUNTIME registry locks before invoking the renderer. The result must not expose a persistable full record, Arc record or complete per-run aggregate collection. Scope it to a callback/render lifetime and release its original-owner charge on completion/Drop/unwind. Preserve safe worker publication while the view is active. Reuse the existing revision as the point-in-time view marker.
3. Add completed-only clear with captured-generation checking. It advances history revision, preserves active accumulator and frame lease charges, and never stops a run or mutates saved macros. UI queues clear, releases the view, then applies it to the same generation. Keep the existing borrowed API for its valid current tests/callers; the GUI uses the new bounded owner path rather than a second hand-written copier.
4. Implement the actual visible History presentation. Keep the existing toolbar's execution controls unchanged and use a bounded History body/section that fits existing dialog sizing. Preserve editor selection/draft when switching. Respect action-editor/recording/package modal ownership; do not create a bypass around active modal controls. History panel acquisition happens only in its visible render path, never hidden synchronization. Store only keys/finite filters/page state between frames.
5. Display captured metadata, safe labels, empty/runtime-unavailable/budget-unavailable states, newest-first page, stable selected detail and clear. Use plain bounded display text for names; no arbitrary diagnostic formatting, action descriptions, raw debug snapshot, current live macro-name reconstruction or rerun controls. Missing selected records clear their detail state. No polling timer or ongoing repaint request.
6. Add direct ownership/UI tests and inspect the cumulative diff for escape paths, raw diagnostics and hidden history reads. Keep one coherent checkpoint; suggested parent commit subject `feat(mkmacro): [G08/M1] expose bounded metadata-only run history`.

Helper naming, exact Rust lifetime/RAII arrangement, layout details and equivalent type placement are implementer choices. Do not implement the recipe continuation in this checkpoint.

## Focused tests

Use new names prefixed `g08_history_`; group equivalent cases rather than creating a large matrix.

- **Accounting/ownership:** small-limit fixture proves view acquisition never evicts to fund a read; max page/detail/name bound; concurrent second view unavailable; view charge included in total; acquisition/allocation failure releases reservation; normal and unwind Drop release exactly once. Clear and terminal publication while a frame view exists preserve queue/active/view accounting. No full record/Arc/vector escapes. The no-Clone contract remains visible in source; do not create a clone-equivalent snapshot type.
- **Unlocked concurrency:** use the existing inert runtime and deterministic barriers. The view render callback must permit a separate thread to publish a terminal summary or clear without waiting for render completion. Avoid sleeps/timing thresholds as evidence. On callback return, accounting remains bounded. Do not obtain another shared lock in inverse order or block the worker behind UI work.
- **Stable identity/lifecycle:** two runtimes reuse run ID 1 but differ in generation; stale selection/clear cannot target the new runtime. New publication preserves a selected RunKey, eviction/clear invalidates details, and close/reopen reads fresh history without retained projection/runtime data. Busy admission produces no invented row; a running paused run is absent until terminal and then shows its recorded pause duration. Missing/deleted macro still shows the captured name/fixed fallback.
- **Actual presentation boundary:** inert summaries covering Completed/Stopped/Failed and enum labels, zero durations/visits, truncated names/details and invalid display-control characters. Seed diagnostic/action/variable payload sentinels and assert projection/render data excludes them. No history action changes document bytes/draft revision or calls an execution command. Exercise actual History open/filter/page/select/clear/close routes, not only string helpers.
- **Visible-only work:** count acquisitions through a narrow test seam. Closed dialog and visible Editor with History inactive make zero history acquisitions; visible History gets a bounded request. Closing releases the frame before return. No new hidden timer/repaint loop. Existing Run/Stop/Pause enablement remains unchanged.

Directly relevant existing selections:

```text
mkmacro::runtime::summary::tests::row_and_history_caps_preserve_totals_and_evict_oldest_completed
mkmacro::runtime::summary::tests::active_byte_pressure_evicts_old_rows_before_growth_and_keeps_current_terminal
mkmacro::runtime::summary::tests::failure_summary_keeps_only_safe_location_not_diagnostic_payload
mkmacro::runtime::tests::admitted_macro_summary_records_source_subset_and_only_safe_visit_metadata
mkmacro::runtime::tests::stop_after_execution_uses_one_terminal_classification_for_snapshot_and_summary
gui::mkmacro_dialog::toolbar::tests::running_and_paused_controls_are_deterministic
```

Confirm actual module paths from Nextest inventory; the function names are source-observed. If layout integration alters the existing main-body path, also select `minimum_size_main_layout_keeps_toolbar_and_steps_reachable_without_mutation` from the actual dialog test module. Do not add unrelated debug/recorder/template suites. App fixtures, if needed, disable Paste Sequences next/cancel shortcuts before constructing the app; prefer the existing dialog/local-runtime harness without constructing LauncherApp.

## Verification, review and done

One nonzero selected preparation and one matching focused run:

```text
cargo nextest list -p multi_launcher --lib --cargo-profile fast-dev --offline --locked -E '<actual g08_history_ names plus selected exact existing tests>'
cargo nextest run -p multi_launcher --lib --cargo-profile fast-dev --offline --locked -E '<same nonzero verified selection>'
```

Fill the filter from actual introduced names. Preparation cap 1200s, run 180s, aggregate actual-error correction 600s and 1–2 localized reruns. No full suite/domain/build/install/broad lint, native campaign or manual-user test. Record exact commands/source/count/exit/time/cleanup and first meaningful failure. Native NOT_RUN is honest and does not block acceptance of this state-tested ordinary GUI feature.

Done means the actual dialog exposes useful history, correct frame ownership/accounting is proved under concurrent publication/clear/replacement, no secret payload or retention escape exists, hidden polling is absent, directly relevant execution invariants/tests pass and reviewer accepts the stable diff. Inspect source to ensure every history display uses the owner gateway and no metadata caches or execution shortcuts were added. Parent commits/pushes before the recipe decision/closure.

The only real design obligation needing implementation confirmation is that its chosen projection storage/lease accounts all retained bytes and releases the correct generation without altering active-run admission. Do not replace that obligation with a generic snapshot framework. If a concrete contradiction appears, report the exact boundary before broadening; do not defer merely because Rust ownership needs adjustment.

M2-A is separate after accepted history. [RECIPES_DECISION.md](RECIPES_DECISION.md) records current template versus invocation authority and supplies a complete conditional continuation; it does not authorize recipe source now.
