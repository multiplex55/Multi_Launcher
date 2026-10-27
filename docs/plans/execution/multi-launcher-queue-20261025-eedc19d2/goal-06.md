# Goal 06 — Activity and cancellation execution record

Queue: `multi-launcher-queue-20261025-eedc19d2`.

Branch: `codex/activity-cancellation-queue-20261026`.
Base (explicit user skip-base override): `ceff1f17fb09545d8b9a6010aae9eca7892a7ea7`.
M0-A: `86d4693f74c76fa7fd7dc6d755175938adee1727`, own empty baseline, pushed.

Goal 04 has accepted M1-A/M1-B only; editor M2-A/M2-B are deferred. Goal 05 is deferred with mandatory native acceptance unresolved, FINAL05 unavailable. Both original dirty worktrees and evidence remain preserved. No partial code is inherited. The October 26 recovery amendment at the original absolute plan root governs continuation.

M1-A: **complete / accepted under authorized parent-review fallback (NOT independent)**. No production execution policy is changed.
Configured Planner completed the owner audit. Implementer and Reviewer dispatch each failed once with agent thread limit reached; parent implementation and review fallback are authorized. Parent review is **not independent**.

## Source-derived lifecycle and retention map

These are source findings, not measured latency or memory claims. Paths are relative to this Goal's execution worktree. Cancellation acknowledgment, terminal publication, job-resource release, native cleanup and actual thread join are distinct events.

| Owner | Start / progress / cancellation boundary | Terminal / retirement / retention boundary |
| --- | --- | --- |
| Indexing — `src/indexer/coordinator.rs`, `src/gui/indexing.rs` | `IndexCoordinator::submit` generation, active token and one latest pending request; result acceptance fences current generation/configuration. | `worker_termination` differs from accepted result; shutdown transfers live handle to reserved reaper. Installed actions remain separately retained GUI cache. |
| File search — `src/file_search/coordinator.rs` | `start_search` per-request thread, `SearchId` fencing; `cancel_active` signals and immediately synthesizes Cancelled. | Coordinator lacks retained join receipt; backend completion does not prove coordinator thread exit. Ripgrep/Everything children/readers have separate backend owners; no bounded worker capacity is claimed. |
| Table/CSV — `src/table_utility_session.rs` | `open`, `request_parse`, `request_file_read`, `schedule` and `submit`; one active, one replaceable pending, one completion slot; `JobKey` lifetime/source/view/selection/export identities. | `close` invalidates lifetime/cancels; retains source, last-good document/projection/selections and idle worker for reopen. Active-token clearance follows `job.run`; final `shutdown` joins synchronously. Close is not retirement. Pure engine remains in `crates/multi_launcher_table_core`. |
| Duplicate discovery — `src/duplicate_finder/session.rs` | `start_scan`, coalesced progress, `view`, `cancel`; one active and one pending; cancelled publication advances generation immediately while restart is blocked. | Active clearance means job drained; `close` drops payload and sends handle to reaper; post-join receipt admits reopen. Limits: 32 roots, 250000 files, 350000 entries, 64 MiB path storage, 64 diagnostics. |
| Hashing — `src/hash_generator.rs`, `src/gui/hash_generator_dialog.rs` | `HashJob` request identity, coalesced HashProgress, cancellation and a one-item terminal channel. | Spawn discards JoinHandle; terminal receipt cannot prove actual thread retirement. Dialog close/reopen and hidden polling retain lifecycle ownership; no invented join or worker capacity claim. |
| Rename — `src/file_operations.rs` | `RenameScanCoordinator::submit`, ticket cancel; shared worker owns apply/undo/cancel/runtime snapshot; one active and one pending. | Mutation stops only at native atomic-operation boundaries. Journal/result receipts authoritative; `request_shutdown` and `try_reap` prove exit. Dialog close cannot discard transaction ownership. |
| Text search/replace — `src/search_replace.rs`, `src/gui/search_replace_dialog.rs`, `src/replace_executor.rs` | Search generations/progress/ticket cancellation; preview one pending/result slot with no active cooperative Cancel; replacement admission rejects active or unconsumed terminal result. | Search/replace shutdown/reap distinct from request. Preserve recovery receipts and atomic commit. Preview cancellation capability must not be invented. |
| XML — `src/xml_toolkit_session.rs`, `src/gui/xml_toolkit_dialog.rs` | COM owner plus bridge; separate source/format/detail/XPath/schema pending slots; immutable shared snapshot and source/request identities. | No safe interruption of active COM call. Shutdown clears publication; Closing retains owner until `try_reap`/`poll_closed_background`. File worker one in-flight operation, no invented mid-save Cancel. |
| Regex — `src/regex_tester/session.rs` | `tick` / `tick_test_cases` debounce and evaluate synchronously. | No asynchronous owner; do not invent queued worker, progress or cancellation. |
| Folder comparison — `src/diff/folder_runtime.rs`, `folder_scan.rs`, `file_ops.rs` | Scan/mutation cancellation, `prepare_rescan` generation; refinement MAX_IN_FLIGHT=2. | Clearing tracked work is not a join receipt. Comparison model retained separately from ephemeral runtime. |
| Hot Folder — `src/hotfolder/runtime.rs` | Lazy activation, reconcile/resume/pause/ready leases. Existing `snapshot()` sends request and waits on reply `recv()`. | The root reads the app driver's short-lock cached HotFolderDriverSnapshot in src/gui/hotfolder_runtime.rs; its driver worker calls the watcher service. The watcher method remains synchronous. M1-B must reuse existing driver publication and inspect/fix the authoritative watcher boundary only as needed, without a second UI cache. Shutdown joins. Ready queue/leases/diagnostics bounded at owner. |
| Screenshot — `src/gui/screenshot_capture.rs` | Request identity, Waiting/AwaitingPresentation/Capturing/Draining; one worker and reserved reaper permit. | Cancel retains Draining slot/discards late image; shutdown transfers handle to reaper. |
| OCR — `src/ocr/job.rs` | Generation-tagged Captured/Finished events. | Spawn drops JoinHandle; Finished precedes actual return. Cancel or Finished cannot establish retirement. Static pin `CaptureJob` in `src/gui/pinboard.rs` has a distinct join/identity boundary. |
| Live pins — `src/pinboard/live.rs`, `src/pinboard.rs` | Generation/epoch, latest frame slot, image leases, GPU budget. Cancel invalidates slot/signals worker. | Cancelled handles remain charged against actor capacity until reaped. `shutdown_live_sessions_and_join` proves worker exit. External image leases intentionally retain accounted bytes after close/join. |
| Macro playback — `src/mkmacro/runtime.rs` | Runtime generation plus run_id, `RuntimeSnapshot`, Stop/Stopping; permanent worker differs from admitted run. | Run completion/held-input cleanup differ from worker lifetime; shutdown joins. Never copy variables, macro bodies or clipboard into activity history. |

Shared `src/thread_reaper.rs` supervisor has capacity **16**. CompletionNotifier wakes supervision; `reap_with_callback` callback runs after actual join. Notification alone is not retirement.

## Activity-facts contract for M1-B

- Key: typed owner kind, session incarnation and immutable operation sequence/receipt. Result source revision is a validity field; advancing cancellation generation must not rename the cancelled operation.
- Lifecycle: queued, running, cancelling, terminal outcome (completed/cancelled/failed). Independently report retirement pending/confirmed/unknown. A retained idle worker is capacity, not a forever-running job.
- Immutable/revisioned small facts: operation kind, monotonic timestamps, meaningful progress count/optional total/unit, current supported control, bounded redacted diagnostic. No raw payload-bearing snapshots in history.
- Resource facts carry unit/provenance: owner-accounted, estimate, process total, unavailable. Display process working set/private bytes separately. Unknown is not zero; no per-feature CPU percentage inferred from elapsed time; shared leases never summed twice.
- Owners retain admission/execution/cancellation/cleanup authority and existing bounds. Observer retains latest bounded owner-slot facts plus at most **100** terminal session-only rows. Clear belongs to observer/history.
- Owner transitions/completion publish event-driven; progress coalesces. Visible activity refresh at most **4 Hz**, closed view produces no refresh demand; preserve child-pin-only wake behavior.
- Cancel routes only through current owner identity and existing safe cancellation capability; unsupported rows offer owner navigation/control. No scheduler or new thread termination mechanism.

## Bounded baseline protocol and evidence

Four exact library cases, default feature graph, fast-dev; no app is constructed and no production shortcut defaults change:

1. `table_utility_session::tests::activity_baseline_table_cancel_and_warm_reopen`: warm small parse/close/reopen; 40000-row generated source within existing limits; observe real active token before close; record method ACK, active-job clearance and final join separately, retained last-good document/source.
2. `duplicate_finder::tests::activity_baseline_native_file_scan_cancel_and_warm_reopen`: 8 identical 128 KiB local files, actual NativeDiscoveryJob and first-read checkpoint; cancel while native reader owns work, immediately release controlled gate, drain, close/post-join reopen, then complete warm scan. Controlled-checkpoint timing is not worst-case blocked native I/O.
3. `pinboard::tests::activity_baseline_synthetic_live_pin_cancel_and_warm_reopen`: two fake-backend open/frame/cancel/join cycles, 2x2 pixels; explicit external 16-byte lease remains charged after join, falls to zero only after lease drop. Headless/synthetic, not native capture.
4. `pinboard::live::tests::native_smoke::activity_baseline_native_live_pin_cancel_and_warm_cycle`: separately selected ignored Windows WGC test; reuse owned PowerShell source HWND/PID, paint/move/pause/resume and frame assertions; cancel pin while source HWND remains live, join, then release retained frame leases and finish owned source. Two sessions supply initial/warm sample. Existing source-closed smoke remains separately available with its original assertions.

`ACTIVITY_BASELINE` JSON lines contain operation/fixture identity, acknowledgment/observed drain/join timings, measured accounting and cleanup. Checked owner records executable/argv/source revision/profile, process identities, streams, deadline and cleanup. Source hashes bind local test-only additions; baseline commit alone does not identify dirty test code.

Compile ceiling 1800 seconds, selected test execution 600 seconds, native execution separate 120 seconds, owner cleanup 5 seconds. Internal small observation deadlines apply. Method ACK is not next usable UI frame. No speedup or M2-B 30-cycle claim. Source-level native support does not establish interactive-session/WGC availability before execution.

Results and exact limitations are recorded below; headless tests do not replace native evidence. M1-B begins only after this accepted checkpoint is committed. Final Clippy belongs at the end-of-Goal boundary, not this checkpoint.

### Initial measured results (not final acceptance)

Evidence root: `C:/Users/Jay/.codex/visualizations/2026/10/25/01a17319-6f7d-7052-b54e-1bb4a92c5fd0/multi-launcher-queue-20261025-eedc19d2/g06`.

Library preparation1: Cargo exit0, 6m42s. Original checked-owner result remains FAILED cleanup verification: 585 absent, two reused, one unknown identity; captured bytes match (stdout1927, stderr19911), drain/auxiliary-lease cleanup verified. One bounded native CIM reconciliation established that exact unknown PID34852/birth22:20:50.0700643Z was reused by a later22:23:35.421912Z identity; no unresolved owned identity remains. Original receipt is preserved unchanged; this is separate retirement evidence, not a rewritten pass.

Inventory confirmed exactly three ordinary cases plus one explicitly ignored native case. Selected ordinary run1: three PASS (1.504s / 0.045s / 0.024s), checked-owner exit0/cleanup/drain true. No unrelated case ran; Nextest's JSON stream includes ignored-case bookkeeping, which is not execution evidence.

| Sample | Method ACK (microseconds) | Observed job clearance / final join (microseconds) | Retained facts |
| --- | ---: | --- | --- |
| Table, 40000 rows / 840006 bytes | 0 at whole-microsecond reporting resolution | active-job clear23; join229 | source840006 bytes and last-good document intentionally retained |
| Native file discovery, 8x128KiB | 86 | job drain250; post-join reopen536 | controlled first-read gate released after Cancel; complete warm scan then second post-join reopen |
| Synthetic pin initial | 145 | join20992 | external image lease16 bytes after join, zero after lease drop |
| Synthetic pin warm | 8 | join117 | same intentional lease behavior |

A reported ACK of zero microseconds reflects timer/reporting resolution; it is not a claim of zero work or UI latency.

Native run1: **FAIL**, access violation0xc0000005 during repeated WGC scenario after first active-cancel receipt. Partial sample: ACK68us, join28952us,49152 external-lease CPU bytes retained after join, zero CPU/GPU accounting after final lease drop, owned source finished. Initial/updated64x64 PNGs visually inspected and hashed. This receipt alone is not native acceptance or proof of no native leak. Final owner containment: all13recorded process identities absent, cleanup/drain true; root Nextest exit100 with nonzero-descendant-retirement reason preserved.

Focused native correction1of2 is TEST-ONLY: keep a balanced explicit host MTA apartment across both sessions while worker initialization, cancellation, joins and resource assertions remain unchanged. Factory lifetime is a hypothesis, not a diagnosed defect. A pass establishes this stated host-apartment fixture only; standalone last-MTA teardown/restart remains an unverified boundary. Rebuild only the affected test library; preserve all three ordinary passes without rerunning them. No production policy correction in M1-A.

### M1-A acceptance

Preparation2 passed (Cargo1m28s; owner94.576683s), exit0/cleanup/drain true. Only the native test fixture changed; file hashes confirm the three passing ordinary-case files remained unchanged, so those cases were not repeated.

Native run2 passed exactly one selected test containing both owned-source sessions (test2.708s; owner4.465452s), exit0/cleanup/drain true, all16recorded identities absent. Captured bytes match exactly (stdout2064190, stderr6394). Session1 ACK72us/join11464us; session2 ACK18us/join14981us. Each retained49152 externally leased CPU bytes after join, then zero CPU/GPU accounted reservations after lease release. Four64x64 generated initial/updated PNGs visually inspected and hashed. The explicit test-host MTA lifetime is retained as a reproducibility precondition. The original no-host repeated-session crash is unresolved; this checkpoint does not fix or establish standalone last-MTA teardown/restart behavior. M2-A must evaluate that measured boundary without silently promoting this fixture pass into a production fix.

Final parent review (NOT independent): owner coverage and small-facts contract present; four representative baseline/warm receipts present; cancellation/terminal/job-clear/join/lease retention remain distinct; actual native capture/owned source cleanup verified for the disclosed fixture; original failures remain immutable. Diff confined to this execution record and four cfg(test) measurement seams. Original source-close native test and its assertions preserved. No production policy/API/dependency/profile changes, no unrelated tests or full suite, no final Clippy yet. Touched-file rustfmt and diff checks pass.

Evidence: `m1a-source-before-prepare*.json`, `m1a-selection.json`, `m1a-baseline-receipts.json`, original `m1a-prepare-1-result.json` plus separate `m1a-prepare-1-retirement-audit.json`, passing `m1a-prepare-2-result.json` / `m1a-selected-1-result.json` / `m1a-native-2-result.json`, preserved failed `m1a-native-1-result.json`, both image manifests and `M1A_PARENT_REVIEW.md` under the evidence root. Parent owns checkpoint Git/push; resulting hash is recorded externally without recursive commits.

### M1-B implementation handoff

**State: source implemented; scoped compile/test verification and parent review pending.** No M1-B build, test, Clippy, native launch, or Git command was run by the source writer; those launches and the checkpoint commit remain parent-owned.

The shared activity registry retains at most 256 active facts and 100 terminal rows, reports overflow by owner, distinguishes late-observed unknown elapsed from a known start, freezes terminal elapsed, and allows already-admitted work to publish its terminal result after Clear. Owner/incarnation/operation identity keeps overlapping instances distinct; superseded operations can still publish their own terminal and retirement fact. Cancellation handlers capture only narrow existing owner capabilities.

Adapters publish at owner/worker boundaries for indexing, file search, table work, duplicate discovery, hashing, rename, search/replace, XML, folder comparison, Hot Folder execution, screenshot capture, OCR, live pins, static pin capture, and macro playback. Regex remains synchronous. Worker completion does not claim thread retirement unless the existing owner actually joins: live-pin and static pin-capture join receipts update the same retained row, while detached workers and per-run work on persistent workers report unavailable retirement. Macro cancellation routes to the immutable run ID through its existing Stop boundary. No payload snapshots, image leases, macro variables/bodies, or clipboard contents enter activity facts.

Hot Folder's watcher `snapshot()` now reads a short-lock cached core fact snapshot combined with existing bounded queue/lease/diagnostic state. The existing app driver remains the only UI-facing cache; snapshot no longer sends a request to or waits for the watcher worker.

The Activity dialog is registered through the typed command/plugin and menu paths, filters safe labels/status/progress/resources, copies only sanitized metadata, and requests repaint/refresh no more than once per 250 ms while its root is visible. Closed or hidden root state performs no refresh demand. Owner navigation routes to existing panels or the existing root/pin-list surface; cancellation remains unavailable for owners without a safe exact capability.

Focused tests added or extended include:

- `activity::tests::{immediate_completion_is_retained_and_retirement_updates_the_same_row,replaced_slot_keeps_the_older_operation_until_its_terminal_update,concurrent_instances_remain_visible_without_inventing_start_times,repeated_superseded_incarnations_have_bounded_owner_overflow_summaries,clear_preserves_a_live_operation_that_finishes_later,terminal_elapsed_time_is_frozen_across_late_retirement,late_attached_observation_keeps_unknown_terminal_elapsed}`
- `gui::activity_dialog::tests::visible_refresh_is_throttled_and_closed_refresh_is_inert` (now also covers hidden-root refresh gating) and safe-filter coverage.
- `pinboard::live::tests::live_pin_activity_cancels_exact_session_and_confirms_join`.
- `gui::pinboard::tests::static_pin_capture_activity_is_published_by_worker_before_join_receipt`.
- `mkmacro::runtime::run_mode_tests::macro_activity_routes_cancel_to_exact_run_and_reports_terminal_progress`.
- Existing plugin/parser/headless, Hot Folder cached-snapshot, and other owner focused tests remain in the parent verification roster.

No result is claimed yet for this M1-B test roster. The parent must verify compilation, the bounded focused tests (including Hot Folder cached-read/ack consistency and dialog navigation/refresh), inspect the coherent source/docs diff, and record exact commands/results here before marking M1-B accepted. M1-A's immutable failures, limitations, receipts, and acceptance remain unchanged.

### M1-B acceptance

**Complete / accepted under the authorized parent-review fallback (NOT independent).** Configured Implementer supplied the source; configured Reviewer dispatch failed once with agent thread limit reached. Parent reviewed the scoped owner/publication/command/panel/Jump/cache diff and the actual test receipts. No retry loop or independent-review claim.

Activity retains at most256active facts and100terminal rows with attributable overflow counts; fixed diagnostic enums and numeric facts carry no source payloads. Clear preserves live jobs and fences old terminal updates. Terminal elapsed freezes; queued/running work observed after admission has unknown start. Exact owner tokens/run IDs route cancellation. Live-pin controls use weak references rather than retaining frame slots/leases. Worker terminal publication is independent of Activity visibility; unavailable joins stay unavailable. Persistent idle services are not fabricated jobs. FileSearch terminal events preserve actual failures; existing file-scan and hash-byte progress publish without GUI drain. Current-process working set/private bytes are sampled for this PID only and displayed separately from global pinboard-accounted image reservations; other unavailable attribution stays unknown. The visible250ms gate and closed/hidden paths are covered directly.

HotFolder watcher snapshots use short-lock cached authoritative facts; the existing app-driver cache remains the GUI boundary. Reconcile acknowledgment follows publication. Worker-held snapshot reads and acknowledgment consistency passed. Activity has its typed payload-free command/plugin/menu, panel-stack open/close tracking, owner navigation and existing Jump window identity. App construction in the extended typed-dialog fixture disables both Paste Sequence test shortcuts before construction.

Scoped root-library fast-dev preparation1 failed with30 compiler errors (largely cascading import/fixture/owner integration issues), Cargo101; cleanup/drain/byte capture verified. One focused integration correction resolved that boundary. Preparation2 passed3m17s. Preparation3 passed1m32s after necessary numeric-progress additions; preparation4 passed1m26s after panel/late-observation corrections. Original failures remain preserved; no core rebuild/profile/dependency changes or full suite.

Exactly46ordinary cases were inventoried. Run4 passed17, then aborted in the new render fixture because strict vendored egui texture deltas were dropped without explicit disposal;28cases did not run under fail-fast. The recorded0xc0000409 abort was accompanied by the fixture's texture-delta destructor panic, not evidence of a diagnosed production buffer overrun. All20recorded identities were absent afterward; capture/drains verified. Focused fixture correction1of2 explicitly disposes all three headless outputs while retaining assertions; only that cfg(test) file changed. Preparation5 passed; run5 selected exactly the failed case plus28unexecuted cases and passed29/29. The17prior passes were preserved without repetition, giving46unique passing cases. All26run5identities were absent and stdout/stderr read/recorded byte counts match. Original compile/render failure receipts remain immutable.

Coverage includes bounded history/order/Clear/late updates/exact cancellation, file/hash worker-side progress and failure with GUI undrained, actual headless open/filter/close rendering and process-sampler gating, HotFolder cached-read/ACK consistency, typed GUI/headless/parser privacy behavior, Jump layer inventory, existing source/generation/reopen/shutdown invariants, screenshot/static-pin/live-pin join reporting and exact macro-run controls. The previously unrun hash-verified M1-A binary idle comparator and candidate both pass the same hidden-root behavioral/counter check; this is not a CPU/power measurement or native presentation claim. M1-A cancellation/native baselines were not rerun here.

Evidence under external g06: m1b-verification-summary.json (all46unique passing cases and both actual runs), m1b-selection-4/5.json, m1b-source-before-prepare-1..5.json, m1b-prepare-1..5-result.json, m1b-selected-4/5-result.json, m1b-fixture-boundary.json, m1b-idle-baseline-summary.json, M1B_PARENT_REVIEW.md. Final touched-file formatting, diff whitespace, source bindings and scoped review passed. No final Clippy yet. Native running/cancelling/retired screenshots and30warm cycles remain mandatory M2-B work; original unguarded WGC boundary remains for M2-A. No FINAL06 or CARRY advancement is claimed. Parent owns this checkpoint commit and one normal push; resulting SHA is recorded externally.
