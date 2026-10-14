# Historical QA plan — revised policy2026-10-14

Authority: [PERFORMANCE_ACCEPTANCE_ADDENDUM.md](PERFORMANCE_ACCEPTANCE_ADDENDUM.md). This earlier child-framebuffer plan is historical reference, not a new gate or authorization to repeat native attempts. Preserve its completed evidence; unavailable current coverage is a manual QA gap.

## Historical record — unchanged, not operative

# M1-D native QR framebuffer prerequisite

Planner handoff,2026-10-13. This bounded follow-up supports M1-D integration smoke and later M2-B evidence; it does not start the broader M2-B milestone. Execute only after original archived-source writer releases ownership. One current-checkout source writer; parent owns docs/Git/build attribution/native launches. ROOT capture already passed real native PNG gates. Current QR window opened, but desktop capture failed twice and its accessibility exposes only chrome.

## Verified owner and limits

Both original0.27 and current Glow initialize AccessKit only for ROOT. Chrome-only QR accessibility is not evidence that QR body failed. Do not add child AccessKit or redesign QR. Current `vendor/eframe/src/native/glow_integration.rs::render_immediate_viewport` paints/swaps but never services screenshot actions, unlike normal Glow rendering. Native redraw of immediate children redirects to the parent. Targeting QR without fixing this owner cannot produce pixels.

`src/gui/qr_dialog.rs::QrDialogState::show` uses `show_viewport_immediate` and the stable `qr_generator_viewport` ID. The callback receives actual class. **Do not inspect Context ViewportState.class as a native guard:** immediate rendering never assigns that field; its default remains Root. Use callback-provided class plus actual child context viewport ID and rendered/open state.

## Objective and scope

Extend the existing feature-gated probe with exactly one selected QR target and service its screenshot at the existing native immediate renderer. Keep one request/state machine/worker/deadline/provenance owner in LauncherApp. Scope: vendor Glow screenshot owner/provenance, `qa_framebuffer.rs`, QR viewport ID/observation seam, existing `render_root_ui` QR call site, directly affected tests and request docs. No new dependency, target registry, styling, screenshot framework, clipboard servicing, focus workaround, automatic QR opening or original-archive change.

## Required implementation

1. After immediate child painting and before swap, fulfill previously queued Screenshot actions from that child's current framebuffer, preserving actual viewport ID/UserData and queueing replies into its `egui_winit` input. Share the narrow fulfillment with normal Glow where practical. Preserve unrelated queued actions and behavior; update vendor patch provenance/removal conditions.
2. Add explicit manifest target QR, mapped through a small shared `qr_dialog::viewport_id()` helper. QR supports Startup only; reject QR/F11 explicitly. ROOT/F11 and existing ROOT receipt semantics remain unchanged. Startup waits for parent/user to open QR; it must not open/focus it automatically.
3. Add a tiny feature-gated QR callback observation without another probe owner/state in QR/context memory. Capture actual callback class/child ID, callback frame/pass/current-pass index, raw visibility/minimized/occluded/focus, body-rendered/still-open state and CPU observation time after body rendering. Overwrite per callback. After immediate rendering returns, attach completed child frame/pass counters. The return boundary follows child paint/swap; callback CPU observations are not GPU timestamps. Feature-off production must avoid probe allocation/metadata work.
4. At the existing QR caller, pass observation to the LauncherApp probe. Only after confirmed target rendering may target input/counter APIs be used: nonexistent counter lookup can panic and input lookup can manufacture default state. Remove only selected target/token screenshot events through `input_mut_for`; preserve ROOT/foreign events. QR focus is valid even when ROOT is unfocused.
5. Maintain target-specific eligibility: actual Immediate callback and matching child ID, rendered body/still open, no explicit hidden/minimized/occluded report, existing logical ROOT/fixture/tool safety. Preserve raw visibility None with Glow's effective true fallback. Unopened is waiting; after acquisition, close, embedded fallback, takeover or ineligibility terminally cancels without resume/retry. Buffer pixels until final retained ROOT completion revalidates all relevant ownership before Save.

## Essential dispatch timing correction

Issue exactly once **after qr.show**, recording API-issue child observation and requesting ROOT repaint. ROOT final end_pass extracts commands for all child states; native handling follows ROOT run_ui and paint/swap. Child renders during discarded parent passes can advance BEFORE that queued command reaches native handling. Therefore, do not call the first arbitrary child advance after API issuance the capture.

At final retained ROOT completion for the issue parent frame, revalidate and retain the latest completed child frame/pass observation as the dispatch-boundary baseline. This is **CPU completion before native dispatch**, not confirmed native dispatch. Ignore child advances earlier within that same issue parent frame.

The first confirmed child render in a later parent frame with completed child counter greater than this baseline is expected capture. Preserve that first observation; request the bounded second ROOT repaint. A further child invocation may deliver the reply within that same later parent frame due to repeated passes. Match target/token and child progression; never require parent N+2. Keep distinct API issue, dispatch-boundary baseline, expected capture and actual reply observations in receipt. Final retained ROOT cancellation must prevent saving even when reply was already buffered. No idle polling, repeated issuance or extra repaint sequence.

## Bounded deadline

For QR only, submit the existing worker wait once at installation so an unopened target also terminates within the deadline. Do not submit another wait at capture; Save/Failure completes the existing wait. Timeout receipt states selected QR capture did not complete within deadline without inventing which unobserved stage failed. Keep the two-slot nonblocking queue protocol and ROOT pre-trigger behavior. No per-frame filesystem polling/repaint loop.

Native run007 demonstrated that the existing30-second ROOT reply timeout is insufficient for QR's installation-to-user-open interval through supported UI automation: the request timed out before the owned action opened a native child. Parent-approved correction2026-10-13: use a fixed120-second QR startup capture deadline, retain ROOT's30-second reply wait, record selected duration in receipt, reuse the same worker/injected short test timeout. No manifest option, automatic QR open, second wait or polling. This remains a bounded QA-only wait; it does not change application timing or performance runs.

## Verification and done

Retain existing ROOT group. Add meaningful tests for actual callback class/identity despite stale Context class, embedded rejection, unopened timeout/acquired close, target/token isolation, issue-frame discarded child advances before dispatch, capture/reply child progression within a later parent frame, final-parent cancellation of buffered pixels, bounded repaint count and one wait/save sequence. Use a controlled native renderer adapter, not synthetic visible flags as proof of native class. Add narrow vendor dispatch coverage with controlled framebuffer reader proving child ID/UserData routing and unrelated-action preservation; no pixel assertions.

Use direct stable MSVC toolchain and locked/offline Cargo. Run feature-enabled all-target check and focused Nextest library probe group, relevant existing QR viewport case, and new vendor case through established vendor test invocation. Since vendor production owner changes, also check default all-target compilation; do not run a full application suite at this prerequisite. Formatting/diff inspection and independent bounded review are required.

Done requires an actual isolated **native QR child** readable PNG and matching child-specific receipt, plus preserved ROOT capture regression evidence. Embedded/headless QR pixels cannot satisfy this gate. OS composition/focus/physical input/secondary accessibility remain separate; do not claim them from framebuffer pixels. Parent owns native tests, artifact hashes and local-only checkpoint.
