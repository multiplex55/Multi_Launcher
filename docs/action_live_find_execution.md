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
| M2-C | Child texture updates, status, Pause/Resume and latest-frame copy/save | complete | Focused presenter/lifecycle tests; exact native ownership and hidden-root error feedback |
| M2-D | Native source smoke and remaining direct lifetime/resource gate | complete | One owned-window WGC smoke; reuse B/C coverage without duplicate tests |
| M3-A | Per-panel Unicode-safe source matching and navigation | complete | Literal case-insensitive range/navigation tests; no content changes |
| M3-B | Find strip, scoped keys, Edit/Preview/Split navigation | complete | Targeted note-panel interactions, honest source-only fallback |
| M4 | Independent focused review and user documentation | in_progress | Final targeted integration, fmt, one Clippy attempt; report native smoke limitations |

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

M2-B checkpoint: 8dad8ebc feat(pinboard): [M2-B] stream bounded source-window frames — normal push succeeded to origin/live-pinboard.

M2-C implementation review: presentation conversion uses a coalescing per-pin
worker and shared image-budget reservations. Joined workers must terminate
independently of renderer-held upload Arcs; retained upload reservations stay
owned after worker exit until the renderer releases those pixels. A focused
retained-upload cancellation test is required for this concrete shutdown risk.

M2-C implementation: explicit Static vs Live presentation sources remove the
permanent live seed lease. A coalescing per-pin worker prepares ColorImage off
UI; sequence changes update the same TextureHandle. Prepared and bounded retired
upload buffers hold shared image-budget reservations. Cancellation joins workers
without waiting for renderer-held Arcs; presenter retains charged upload buffers
until release. Source/status notifications target the exact child viewport.
Child/list Pause/Resume preserve native identity. Copy/Save admit one immutable
latest-completed-frame lease. Hidden-root startup errors wait for cleanup and
native restoration, then remain reachable in a dedicated non-modal viewport.

M2-C verification: initial combined fast-dev Nextest batch compiled after fixing
new Condvar/Arc/texture-delta/test setup errors. Five of six tests passed; the
presenter repaint recorder failed because egui coalesces pending repaint requests.
The exact list test was initially omitted by a filter typo; it passed separately:
`cargo nextest run --cargo-profile fast-dev --lib -E 'test(pin_list_is_keyboard_focusable_and_stale_rows_cannot_reopen_closed_pins)'`.
The presenter test also required borrowed ViewportId arguments, and setup repaint
isolation: egui requires two passes; rendering no-op UI can create more work.
A bounded Context::run_logic drain establishes a clean baseline, without changing
production behavior or weakening child-only assertions. Final presenter recheck
passed (1/1): `cargo nextest run --cargo-profile fast-dev --lib -E 'test(pinboard_live_presenter_reuses_texture_and_charges_retained_uploads)'`.
Initial five passing cases were feedback persistence, exact native controls,
retained child texture/zoom, command recovery, and delayed live restoration/error.
All seven distinct selected cases have passed across these runs. No full suite
was run. Actual native pin UI remains unverified; M2-D provides backend evidence.

M2-C checkpoint: 3776f20b feat(pinboard): [M2-C] display and control live pins — normal push succeeded to origin/live-pinboard.
M2-D assigned to sole implementation writer: one ignored owned-window native
WGC smoke with fixed movement/occlusion/pause/source-death/cleanup script.

M2-D native gate evidence so far (not complete): ignored test and owned WinForms
fixture use actual resolver, WGC backend and domain commit. First compile needed
three missing test imports. First execution was blocked by PowerShell script
policy before windows; process-scoped -ExecutionPolicy Bypass fixes fixture launch
without changing user/machine policy. Restricted execution then failed at WGC
IsSupported with 0x8007000E. One normal-permissions run passed that boundary and
failed CreateForWindow 0x80070057. Fixture-only source style correction (ordinary
app/taskbar window, no forced NOACTIVATE; ShowWithoutActivation retained) passed
binding. No production window-style workaround was added.

First-frame timeout exposed a real production defect: wait_for_frame_or_control
cleared frame_arrived before constructing its returned snapshot, so the native
loop discarded every frame event. Fixed snapshot-before-clear and activation's
pending-drain signal (needed when the bounded pool fills during restoration).
Two deterministic regressions passed: `cargo nextest run --cargo-profile fast-dev
--lib -E 'test(worker_signal_)'`. Touched rustfmt/diff checks pass.

Same fixed native command, run with normal permissions:
`cargo nextest run --cargo-profile fast-dev --lib --run-ignored ignored-only -E 'test(native_live_pin_owned_window_smoke)' --no-capture`.
It now verified initial source-relative asymmetric A pixels and moved/covered B
pixels (never magenta desktop occluder), then paused frame stability. Evidence
saved and visually inspected at target/pinboard-live-smoke/18dfcce490d22a8c/initial.png
and covered-updated.png. Resume exposed a second production boundary: creating a
new capture session from the already-used frame pool after Close returned
0x8000FFFF. Implementation is correcting pool/session restart while retaining the
same source item; source closure and full cleanup assertions remain pending.

M2-D restart correction: NativeCaptureResources retains one item/Closed
subscription but closes and replaces pool/session/event token on pause/recovery.
GPU reservation/staging release while stopped; reserve before restart allocation.
Windows `cargo check --profile fast-dev --lib`, touched fmt/diff passed. The next
same escalated smoke reached valid resumed C pixels, proving restart, then exposed
a source-closure race: a zero-filled teardown frame replaced C before terminal
status. The final frozen C assertion correctly failed. Implementation is fencing
terminal status in the actual item Closed callback and native publication boundary;
no pixel heuristics or source HWND reopening. New A/B artifacts from that run:
target/pinboard-live-smoke/18dfcd37c9bbb75c/{initial,covered-updated}.png.

M2-D COMPLETE: item Closed callback terminalizes the exact slot before waking
worker; native identity/visibility/control checks fence GPU/CPU readback and
publication. Existing terminal late-publication test passed (1/1):
`cargo nextest run --cargo-profile fast-dev --lib -E 'test(latest_frame_slot_rejects_stale_publications_and_charges_retained_leases)'`.
The escalated same fixed smoke then passed all assertions in 1.73s. Final diff
review also repaired initial-reservation transfer if a first candidate is rejected
after readback; same reservation remains pending, no activation-before-first-frame
stall. Windows library check/fmt/diff passed and same native smoke passed again
(1/1, 1.730s total). Final artifacts:
`target/pinboard-live-smoke/18dfcd900e990e80/initial.png` and
`target/pinboard-live-smoke/18dfcd900e990e80/covered-updated.png`.

This establishes real source-relative WGC pixels, moved/covered changing source,
Pause/Resume, frozen terminal source closure with Resume rejected, and zero CPU
image/GPU reservations after all leases/workers close. Owned helper acknowledges
QUIT and exits; timeout/unwind guards target only its own Child. No HWND reopen,
pixel heuristic, screen polling, synthetic input, or unrelated user window control.
Native pin UI controls, radial triggering and note UI were not manually operated;
focused deterministic UI tests provide separate evidence. No broader smoke suite.

M2-D checkpoint: e067a612 fix(pinboard): [M2-D] verify native frame lifetime and cleanup — normal push succeeded to origin/live-pinboard.
M3-A assigned to sole implementation writer: per-panel transient original-source
Find model and invalidation, no Find UI/preview/key routing until M3-B.

M3-A complete: new private gui/note_find.rs owns per-panel transient open/query,
escaped Unicode-insensitive regex, query/content revisions, original byte+char
ranges, current result and wrapping navigation. One monotonic char_indices scan
maps all matches. Empty/no-query results are empty; conventional nonoverlap is
explicit. Unchanged revisions reuse compiled regex/results. NotePanel mutation,
alias save, external invalidation/replacement and slug-switch owners invalidate
or reset it; no note content/save/dirty/mode changes come from Find.
Scoped `cargo nextest run --cargo-profile fast-dev --lib -E 'test(note_find_)'`
passed 6/6. Covers literal punctuation/nonoverlap, Unicode/CRLF original ranges,
empty/no matches, wraps/revisions/cache reuse, and panel isolation/same-length
replacement/unchanged persistence state. Touched rustfmt passes (mod.rs with
skip_children=true); git diff --check passes. No UI/preview/root-key changes yet.

M3-A checkpoint: d4b702a8 feat(notes): [M3-A] add transient per-note find navigation — normal push succeeded to origin/live-pinboard.
M3-B assigned to sole writer: active-note Find controls/keys/selection and honest
original-source to rendered preview navigation; early root Esc only for Find.

M3-B preview decision (implementation pending): current show_markdown_fragment
can span entire heading-free documents, so Find-open preview uses actual semantic
block response boundaries, caching source metadata. Direct already-locked
pulldown-cmark is justified for original-source block/visibility spans; normal
closed-Find rendering remains unchanged. Ordinary reference definition spans are
carried into rendering units. A bounded read-only planner check confirmed existing
footnote support and no public CommonMarkViewer/backend event or shared-footnote
rendering API. Do not fork/edit vendor or invent approximate line-to-pixel offsets.
Footnote-dependent blocks may coalesce into a containing semantic rendering unit;
unrelated body blocks remain separate. A large linked group has a coarser unit
locator, honestly identified with explicit Reveal in Edit for exact source
selection. This uses the approved containing-segment locator policy, never a false
claim that visible body text is source-only. Tests must establish ordinary long
body navigation, hidden source targets, and preserved reference/footnote context.

M3-B first test batch stopped at compilation (no tests ran): public modal-layer
lookup belongs to Memory, and root routing needed borrowed Context arguments.
Those API corrections are in progress alongside the required long-preview,
keyboard-owner and collapsed-source interaction cases. Ctrl+F/Ctrl+W matching
uses each key event's modifiers rather than the frame's last modifier state.
README documentation is prepared independently; no source-write overlap.

M4 independent reviewer is inspecting baseline 96d4926e through the coherent
M3-B source boundary (including the untracked preview helper), read-only and
without builds/tests. M3-B focused test binary is compiling; no tests have
reported yet. No broad regression or native UI campaign is being started.

M3-B next compile exposed an E0502 borrowing cached task analysis alongside
note content in find_preview_visibility; no tests ran (exit 101). The writer
is correcting that specific ownership boundary before the intended test batch.
Independent review accepted concrete fixes: preserve partial text beside custom
image segments; Ctrl+F must refocus an already-open query; unrelated callouts
must not consume preview navigation. These stay in M3-B. A separate M4 fix will
make initially-hidden Static capture errors independently visible, matching the
already-admitted hidden-root capture path without changing root visibility.

Independent cumulative review completed read-only with five P2 findings:
partial custom-preview text preservation, callout locator ownership, hidden
Static error reachability, reopening Find focus, and event-local key modifiers.
All are accepted for correction. No additional actionable capture lifetime or
resource ownership defect was found. Reviewer ran no tests/builds. Final approval
is pending bounded inspection of these fixes and scoped verification, not a
second general audit. The Static fix remains sequential after M3-B commit.

M3-B acceptance batch compiled and ran 15/27: 12 passed, 3 failed, remaining
12 cancelled/not executed (5778 outside selection). Concrete failures are helper
resolved-id source visibility, later-body navigation after an unrelated callout,
and collapsed-heading locator visibility. Writer is diagnosing these owners;
failed and unexecuted cases must pass. The recheck uses --no-fail-fast to avoid
counting cancelled cases as evidence. Four note review findings are resolved by
independent source inspection, with execution evidence still pending.

Resolved-id visibility diagnosis: new preview helper's TODO_TOKEN_RE had no
capture group while token construction required captures.get(1), silently
discarding every todo token. Parent identified the direct mismatch and sent
it to the sole writer; expected hidden resolved-ID behavior is preserved.
Scroll tests now disable animation only in their test Contexts to inspect the
completed target deterministically; production animation remains unchanged.

Animation diagnosis correction: egui Ui::scroll_to_rect reads
style.scroll_animation, independently of style.animation_time. The first
test-only animation_time adjustment did not disable scrolling. The verified
public test control is egui::style::ScrollAnimation::none(); writer was sent
this exact API. Production scrolling and visibility assertions stay intact.

Full no-fail-fast batch: 20/27 passed, 7 failed. Todo/wiki visibility now passes.
Remaining actual failures: three offscreen locator cases (long body, callout,
collapsed heading), all-mode editor-state initialization, query initial focus,
and two root shortcuts. Parent/reviewer confirmed plain CTRL has command=false
in egui; matches_logically(COMMAND) requires command=true. Route/consume actual
Ctrl-or-command event modifiers, retaining legitimate unrelated-input exclusion.
Reviewer also found Find focus request cleared in disabled Window sizing pass;
retain until the enabled TextEdit accepts focus. No broad note-open rewrite.

M4 actual-route inspection confirmed one directly requested integration gap:
macro/gesture/radial note Open/New can retain launcher query focus, so a new
Preview note rejects immediate Ctrl+F. Enter/click routes happen to surrender it.
The shared open_note_panel owner only pushes/reorders panels, and outcome/root
restoration does not clear stale egui query ownership. M4 will add a one-time
activation handoff at the exact note render boundary (new/reactivated panels),
clearing only superseded root-query ownership/pending focus and raising that
note layer. Later deliberate query/dialog focus must continue excluding Find;
Split initial-editor policy/default modes stay unchanged. This is the explicit
note-open/Find integration criterion, not a general focus subsystem change.

M3-B behavioral gate passed across bounded runs: 20 distinct cases from the
complete 27-case batch, four previously failing cases in the seven-case recheck,
then the final three cases passed 3/3 (5802 outside selection). All 27 selected
cases have now executed successfully. No cancelled test is counted as passing.
Final three-case command:
`cargo nextest run --cargo-profile fast-dev --lib --no-fail-fast -E 'test(note_find_navigation_renders_original_ranges_in_all_view_modes) | test(note_find_collapsed_match_reveals_exact_original_unicode_selection) | test(note_find_shortcut_uses_focused_or_top_note_and_preserves_ctrl_w_close)'`.

Initial complete selection was `test(note_find_)` plus the four semantic-preview
helper tests and eight existing regressions: focused_split_editor_routes_keyboard_image_paste,
clipboard_lookup_is_excluded_by_each_non_editor_focus_or_view_mode,
markdown_fragments_keep_distinct_stable_ids_and_prepare_shared_cache,
split_mode_does_not_request_initial_editor_focus,
preview_preprocess_preserves_markdown_blocks_and_raw_html,
programmatic_content_replacement_invalidates_cached_markdown_analysis,
persistence_baseline_tracks_exact_content, discard_prompt_actions_have_explicit_transitions.
The direct pulldown-cmark dependency uses already-locked 0.13.4 without default
features, for original-source semantic ranges; no vendor/new parser renderer.

Actual fixes preserve source text around custom images, restrict callout locator
ownership, refocus repeated Ctrl+F, use event-local modifiers, defer disabled
sizing-pass focus, and preserve exact active Find selection after unfocused
TextEdit normalization without overriding a user-focused caret. Scroll tests
use ScrollAnimation::none only in their Context; production animations stay.
One test-only accessor exposes the actual rendered editor ID to the root fixture.
M4 activation-time root-query handoff and hidden Static error feedback remain
pending as explicitly scoped integration work.

M3-B touched Rust formatter checks and git diff --check passed. The generated
default clipboard_modifiers.json (absent baseline, created during this run's
new_app startup) was inspected and removed by the writer. Only intended source
and parent README/ledger changes remain. Writer stopped before checkpoint Git.
