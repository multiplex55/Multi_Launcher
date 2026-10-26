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
