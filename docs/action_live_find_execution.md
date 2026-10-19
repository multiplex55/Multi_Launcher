# Action parity, live pins, and note Find execution ledger

Active scope: the user-provided 2026-10-18 goal. Only action execution parity,
opt-in window-attached live cropped pins, and transient in-note Find.

Baseline: clean `live-pinboard` branch at `96d4926e`. Preserve static pins,
radial presentation policy, explicit action settings, and note persistence.

## Checkpoints

| Stage | Objective / owner | State | Acceptance and verification |
| --- | --- | --- | --- |
| M0 | Reconcile checkout, types, callers and tests | complete | Read-only ownership inspection; no broad build |
| M1-A/B | Measure action effects separately from presentation; fix demonstrated gaps in existing dispatch owners | complete | Compact parity cases, directly affected paste/radial tests; combine tests/fix if needed |
| M2-A | Typed live intent through existing selector/parking lifecycle; unambiguous external window binding | complete | Parser/handler and selection/restoration tests; static path unchanged |
| M2-B | Window-content capture and bounded latest-frame sessions owned by pinboard | complete | Fake stream/budget/pause/identity/cancellation tests; Windows compile |
| M2-C/D | Child texture updates, status, Pause/Resume, latest-frame copy/save and cleanup | pending | Presenter/model/lifecycle tests, mixed static/live limits and shutdown |
| M3-A | Per-panel Unicode-safe source matching and navigation | pending | Literal case-insensitive range/navigation tests; no content changes |
| M3-B | Find strip, scoped keys, Edit/Preview/Split navigation | pending | Targeted note-panel interactions, honest source-only fallback |
| M4 | Independent focused review and user documentation | pending | Final targeted integration, fmt, one Clippy attempt; report native smoke limitations |

Implementation checkpoints execute sequentially. Each coherent verified checkpoint
is committed using `<type>(<scope>): [M#-X] ...`; one normal push is attempted
after each commit. Push failure does not block continuation. No history rewriting.

Verification budget: one focused pass per materially completed subsystem plus
one final targeted integration pass and one final Clippy attempt. At most two
bounded diagnosis/remediation/recheck cycles per distinct feature failure.
No full suite, all-targets/release builds, benchmarks or unrelated cleanup.

## Evidence and decisions

- M0: attached source findings match the existing pinboard model/lifecycle and
  independent viewport presenter. Existing Windows crate is 0.58; WGC/D3D
  features are not enabled. Existing regex dependency can support note matching.
- Tests, commits, push outcomes and concrete limitations will be recorded here
  at their checkpoint boundaries.

### M1 compact effect / presentation matrix

| Case | Shared effect contract | Deliberate presentation / existing coverage |
| --- | --- | --- |
| External action | Same original action and arguments, once; retain activation source | List/grid/gesture/macro share `activate_action`; radial uses prepared dispatch |
| Launcher dialog | Same reachable panel and focus intent | Root restoration must not cover independent native editors |
| Saved paste sequence | Same prepared sequence and captured external target | Effective keep-dialog-visible setting governs playback |
| Clipboard modification | Same operation and captured history query | Ordinary hide preference; radial root preservation |
| Destructive action | Cancel zero, confirm once with retained invocation | Additional radial confirmation remains valid |
| Exclusive capture | One input owner after overlay teardown | Radial KeepOpen incompatibility remains explicit |
| Stale / disabled action | No operation or false successful history | Existing prepared-lease revalidation rejects stale targets |

M1 inspection found a concrete capture prerequisite mismatch: the shared
`PinboardCaptureLifecycle::begin` rejects hidden roots, while eligible radial
capture preserves a hidden root. Repair belongs to M2-A's parking/restoration
boundary; M1 does not add a per-surface visibility workaround.


M1-A: six-source primary contract regression added; radial Help asserts reachable
panel and one attributed dispatch. Five selected Nextest tests passed twice after
one borrow-check correction (owned history query). Command: `cargo nextest run
--lib -E 'test(action_parity_) | test(primary_universal_action_delegates_to_exact_activation_path) | test(ordinary_radial_execution_preserves_root_launcher_state) | test(saved_paste_to_advance_run_repeats_three_times_through_app_and_physical_admission) | test(saved_run_prompt_continuation_opens_closed_grid_route_and_keeps_root_visible)'`.
Touched-file rustfmt and diff whitespace checks passed. `cargo fmt --check`
fails on preexisting formatting in `src/bin/radial_acceptance/native.rs` and
other untouched files; no unrelated formatting edits made. Test-generated
`clipboard_modifiers.json` removed after provenance confirmed. M1-B has no
production change; hidden-root capture gap proceeds to M2-A.

M1 checkpoint: f5793450 test(action): [M1-A] define surface-aware action parity cases — normal push succeeded to origin/live-pinboard.


Native capture references: Microsoft documents window-item creation via
[CreateForWindow](https://learn.microsoft.com/en-us/windows/win32/api/windows.graphics.capture.interop/nf-windows-graphics-capture-interop-igraphicscaptureiteminterop-createforwindow)
(Windows 10 1903+) and internal-worker frame events via
[CreateFreeThreaded](https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.direct3d11captureframepool.createfreethreaded).
The [capture guide](https://learn.microsoft.com/en-us/windows/apps/develop/media-authoring-processing/screen-capture)
requires respecting each frame ContentSize; pixels outside it are undefined.
WGC/D3D features can be activated on the existing Windows 0.58 dependency;
no convenience capture crate is planned.

M2-A integration decisions: preserve the original hidden-root visibility and
restore intent inside the shared capture lifecycle rather than showing the root
from radial dispatch. Selection uses physical DWM frame bounds and whole-rect
z-order ownership after overlay teardown. Hidden former root/pin bounds and
lower windows do not cause rejection. Minimized, cloaked, and empty/non-rendering
windows are excluded; true higher visible occluders remain a rejection.
Live initialization must never publish a desktop screenshot as live content.

M2-A verified: additive typed Live command and source resolver, exact initial
hidden-root visibility/restore preservation, no desktop screenshot fallback.
Enabled existing Windows crate feature Win32_Graphics_Dwm for physical frame
bounds/cloaking. Initial headless test expectation corrected to retain shared
GUI-only error; no headless production behavior changed. Unknown source bounds
fail closed above a candidate and cannot disqualify a higher containing source.

Passed (13 tests): `cargo nextest run --lib -E 'test(pinboard_live_) | test(live_source_) | test(pinboard_namespace_is_exact_typed_and_payload_free) | test(pinboard_typed_actions_reach_the_gui_host_without_generic_visibility_or_focus_policy) | test(pinboard_is_explicitly_gui_only_and_never_uses_external_fallback) | test(pinboard_commands_dispatch_to_the_typed_gui_host) | test(exact_pinboard_queries_expose_capture_and_recovery_actions) | test(plugin_help_discloses_all_pinboard_paths) | test(capture_safety_accepts_hidden_or_visible_non_iconic_offscreen_window)'`.
Passed (2 tests): `cargo nextest run --lib -E 'test(fake_capture_waits_for_both_native_acknowledgements_and_restores_before_commit) | test(capture_reservations_bound_pending_and_aggregate_bytes_with_overflow_checks)'`.
Passed after final source correction (5 tests): `cargo nextest run --lib -E 'test(live_source_)'`.
Touched-file rustfmt and diff whitespace checks passed. Whole-repo formatting
still reports preexisting drift in native.rs, lib.rs and paste_sequence/playback.rs.
Live frame backend is deliberately not implemented at this checkpoint; live
creation restores and reports unavailable rather than creating a misleading pin.

M2-A checkpoint: 1877b0c9 feat(pinboard): [M2-A] add live capture intent and source binding — normal push succeeded to origin/live-pinboard.


M2-B design guardrails: one authoritative completed live-frame slot, immutable
leases for consumers, and no permanent retained initial seed after replacement.
Admission must charge pending static reservations and in-flight live conversion
before allocation, in addition to current/retained action images. Capture items
are opened once and source closure is terminal; no recycled-HWND rebinding.
M2-B and following verification use fast-dev to bound test-symbol/link cost.

Native smoke capability check: `[Environment]::UserInteractive` is true;
current process and Explorer both use session 1. M2-D should attempt one bounded
owned-source-window WGC smoke after C. Browser-only CUA cannot drive arbitrary
native app UI; native capture can instead be exercised through a narrowly owned
fixture/test, with exact child-process cleanup. This is not evidence that WGC
frames or note-panel native interaction have already passed.

M2-D native smoke handoff (prepared read-only, execute only after C): no existing
changing-window fixture is reusable. Use one ignored Windows unit test adjacent
to live.rs plus tests/fixtures/pinboard_live_source.ps1 (owned PowerShell -STA,
hidden console, Add-Type WinForms source/opaque occluder). No new binary or
historical native harness. Fixed nonce/PID/HWND READY handshake, command IDs and
UI-paint acknowledgements; exact Child guard sends QUIT then kills/reaps only its
owned process on bounded failure. Show without activation; manipulate no user
windows. Keep ApplicationContext alive after source HWND closes.

Exercise real resolver -> reserve_live_capture -> reserve_selected_rect ->
NativePinLiveCaptureBackend.start -> first frame -> stage_live_capture -> commit.
Use nonzero crop and asymmetric known pixels to establish source-relative offset.
Fixed script: baseline A; move source, cover it with opaque magenta, paint B and
verify B (never desktop/occluder); Pause, paint C and observe frozen sequence;
Resume and verify C; close source while helper remains alive and verify terminal
SourceClosed/frozen frame and rejected Resume. Close pin, join workers, drop all
leases and assert budget zero. Save initial/covered-updated PNGs under
`target/pinboard-live-smoke/<nonce>/`. One bounded ~30-second session, only actual
feature failure gets at most two scoped fixes. Report unsupported OS/session/WGC
as failure/unverified, never success. Actual native pin controls, radial and note
UI are not proven by this backend smoke; focused UI tests remain separate.

M2-B final-review blocker (being fixed before commit): Capturing handled Live,
then unconditionally ran static capture_job handling; a pending first live frame
fell through the static no-job branch to Restoring. Immediate fake delivery
masked this. Split typed intent branches and gate fake first-frame delivery;
prove repeated pending polls do not restore/commit, release leads to normal
restoration+commit, and cancellation drains a blocked first-frame worker.

Future M3 implementation notes from planner: use existing escaped Unicode
case-insensitive regex on original Markdown (never lowercase-string offsets),
store source byte + egui char ranges and content/query revisions per NotePanel.
mark_content_changed and external mutation seams invalidate match results;
pending_selection is cached actual user selection, not an authoritative Find
navigation queue. Exact editor IDs are (note_text,slug)/(note_split_text,slug).
Root Escape in gui/render.rs occurs before note panels render, so consume Find
Esc early only for the active note; don't change close_front_dialog globally
(Ctrl+W and explicit close retain semantics). Ctrl+F must target focused note
layer, never the first panel rendered or another dialog. Preview must map original
source ranges to actual rendered semantic block/fragment rectangles, scroll and
outline the active containing segment; source-only syntax/destinations get honest
status and explicit Reveal in Edit. No automatic mode switches, unrelated
collapse expansion or Markdown mutation for highlighting. Existing CommonMark
viewer exposes whole-fragment response, no public source offset hook; direct
already-locked pulldown-cmark is justified only if semantic range metadata is
needed, preserving whole-block Markdown/reference-link semantics.

M2-B verified implementation: WGC CreateForWindow item retained once per source;
free-threaded two-buffer pool, coalesced events, completed conversions capped at
100 ms, crop-only staging/readback with row-pitch BGRA->RGBA conversion. Native
COM/D3D/event/session resources have RAII cleanup. Pause and source invisibility
stop capture; resume uses the same retained item. SourceClosed/Error are terminal.
Sequence/generation/publication-epoch fences reject stale or paused publication.
Shared latest frame owns no permanent initial seed; static snapshots remain
immutable. One atomic RAII budget charges pending reservations, in-flight images,
completed frames and retained action leases; separate GPU/session bounds apply.
Active/pending/retired workers remain domain-owned and are drained on shutdown or
domain destruction. Child notifier seam publishes without root repaint.

Windows check `cargo check --lib --profile fast-dev` passed after two bounded
native-API correction cycles (event token/type inference, then borrowed event
handlers). Focused live pass (8 tests): `cargo nextest run --cargo-profile fast-dev --lib -E 'test(live_frame_) | test(live_gpu_) | test(frame_cadence_) | test(latest_frame_slot_) | test(fake_live_session_) | test(active_live_capture_worker_) | test(pinboard_live_selection_resolves_source_then_restores_hidden_root_without_snapshot)'`.
Model pass (3 tests): `cargo nextest run --cargo-profile fast-dev --lib -E 'test(pins_keep_independent_session_state_and_release_bounded_image_memory) | test(capture_reservations_bound_pending_and_aggregate_bytes_with_overflow_checks) | test(live_capture_worker_is_cancelled_and_joined_when_pinboard_drops)'`.
Final slot/notifier/all-owner teardown pass (2 tests): `cargo nextest run --cargo-profile fast-dev --lib -E 'test(latest_frame_slot_rejects_stale_publications_and_charges_retained_leases) | test(live_capture_worker_is_cancelled_and_joined_when_pinboard_drops)'`.
Owner-drop test capacity assertion was corrected when its test budget increased;
no production limit was weakened. A test-only operation_id shadow was also fixed.

The final-review delayed-first-frame fall-through was fixed before commit.
Live/Static Capturing paths are mutually exclusive. A cancellable fake first-frame
gate proves repeated pending polls neither restore nor commit, release permits
normal restore/commit, and cancellation drains blocked initialization. This and
static acknowledgement passed 2/2: `cargo nextest run --cargo-profile fast-dev --lib -E 'test(pinboard_live_selection_resolves_source_then_restores_hidden_root_without_snapshot) | test(fake_capture_waits_for_both_native_acknowledgements_and_restores_before_commit)'`.
Touched Rust formatting and whitespace checks passed; global fmt still reports
only preexisting drift. Native WGC smoke remains pending M2-D; no UI/native proof
is claimed from these fakes. C must remove presenter's initial live image lease,
wire child notifier/texture refresh/current-frame actions, and show hidden-root
creation failures through reachable independent UI after safe restoration.


Final delayed-restore variant passed the same 2-test filter: no live pin commits
while native pin restoration acknowledgement is pending. The expected restore
poll count was updated from 1 to 2 for that intentionally delayed test fixture.
Touched Rust formatting passes; ledger EOF whitespace repaired by parent.
