# Screen Region OCR execution ledger

Authority: [approved plan](general-purpose-screen-region-ocr.md), the attached kickstart, and `AGENTS.md`. This ledger refines implementation details against HEAD `112cb87f`; it does not replace the approved requirements. The approved plan was read completely. No production changes or builds/tests were performed during planning.

Update each checkpoint to `in_progress`, `complete`, or `blocked` as work proceeds. Record actual verification and commit hashes below. Completion requires acceptance evidence, not just written code. One source writer at a time; commit coherent checkpoints before materially different work. Preserve checkpoint history and use explicit remediation commits for later defects. Do not make empty commits.

## M0 findings — complete

- One proven OCR implementation exists in `src/mkmacro/ocr.rs`: `OcrBackend`, `WindowsOcrBackend`, `OcrDocument`, `recognize_region`, tiling and reconstruction. `recognized_text()` preserves multiline text with outer whitespace trimming. Direct production callers are `src/mkmacro/executor.rs` (Read Text and search) and `src/gui/mkmacro_dialog/ocr_test_job.rs`.
- Use a narrow shared facade in `src/ocr.rs`, exported from `src/lib.rs`, composing the existing engine and capture traits. Do not move all screen primitives or macro search/matching types. This is an explicitly permitted plan strategy and avoids unnecessary MkMacro migration.
- `WindowsOcrBackend::available_languages()` enumerates installed recognizers; explicit `MkOcrLanguage::LanguageTag` checks support. General OCR must not request `Auto`. Deterministic common preference `en-US`, then `en-GB`, then another supported English variant sorted by normalized tag is acceptable; handle tags case-insensitively and never use a non-English fallback. Keep MkMacro Auto/configured tags unchanged.
- Reuse `RectanglePurpose`, `VisualOverlayController`, native overlay service, and `SharedVisualOverlayController`. `confirm_picker` closes the native renderer before emitting confirmation. The picker already owns initial-release arming, Esc, operation IDs, physical signed coordinates, and normalized drag rectangles.
- Shared event delivery currently separates Screen Draw events from `editor_events`; other purposes share the editor queue. OCR needs isolated routing/retirement so MkMacro `poll()` cannot drain OCR confirmation/cancellation/error. Preserve synchronous worker-start error routing and service recovery. Do not reuse authoring `VisualCaptureWorkflow`, draft tokens, or asset persistence as the general OCR workflow.
- Reuse `LauncherParkingTransaction<Generation>` from `src/launcher_parking.rs`. `src/gui/color_pick.rs` is the focused one-shot precedent for exact rectangle restore, visibility revision ownership, root suppression, conflict rejection, and restore retry. Screen Draw's persistent recovery state remains Screen Draw-owned.
- Put transient general workflow state/generation in the OCR domain/controller and root/native coordination in a focused `src/gui/ocr.rs` lifecycle owner composed into `LauncherApp`. Add normal panel/dialog lifecycle integration in `src/gui/mod.rs` and `src/gui/render.rs`; result rendering must not own OCR/capture logic.
- Follow normal plugin registration and typed command dispatch (`src/plugins/`, `src/plugin.rs`, `src/commands/{model,parser,host,bus,headless}.rs`, handlers, `src/gui/command_host.rs`). No direct GUI string parser or OCR-specific hotkey. Normal actions remain assignable through existing radial/action surfaces.
- Use the `ocr_test_job.rs` thread + atomic cancellation + mpsc pattern with general operation identity, not macro draft identity. General completions need independent generation checks and repaint notification/nonblocking polling. Capture must finish while the launcher is parked; allow launcher restoration and Recognizing presentation once immutable pixels are available. If this requires splitting the existing pipeline, extract one captured-image recognition helper and have `recognize_region` delegate to it; never copy tiling/reconstruction.

Relevant existing test seams: `mkmacro::ocr::tests`, `mkmacro::executor::ocr_execution_tests`, `mkmacro::screen::tests`, `gui::mkmacro_dialog::{visual_overlay,visual_capture_workflow,ocr_test_job}::tests`, `launcher_parking`, `gui::color_pick::tests`, command parser/bus/handler/headless tests, and `tests/{plugin_commands,plugin_routing,plugin_exact_match}.rs`.

## Shared invariants and execution rules

All checkpoints preserve MkMacro OCR actions/conditions, Auto and explicit tags, match/occurrence/output behavior, macro schema, and authoring/debug workflows. Capture and result are transient; no writes to image/text history or screenshot files. No online service, OCR dependency, language selector, OCR settings page, extra action integrations, or native selector duplication. Clipboard writes require explicit Copy and must not copy an empty/no-text result. Cancel is a normal outcome, never an error toast.

Keep each checkpoint compiling and review its diff before commit. Add tests only for the changed behavior and directly affected invariants. Intermediate verification is targeted; do not run broad suites after every checkpoint. The verification commands below are suggested exact module filters; adapt to actual module names created, retaining scope. Use `cargo check --lib` when integration/export changes need compilation; check the application target when GUI wiring requires it. Formatting checks can be deferred to M9, but newly edited code should be formatted without unrelated churn.

## M1-A — shared service facade

Status: complete. Depends on M0. Commit: `refactor(ocr): [M1-A] establish shared local OCR service boundary`.

**Objective/ownership:** Give general launcher consumers one service outside MkMacro UI for selected-region recognition and reconstructed text, backed by the existing local implementation.

**Scope/current facts:** Add `src/ocr/mod.rs` (or equivalent narrow module), export in `src/lib.rs`; use `ScreenRect`, `SearchRegion::Rectangle`, `ScreenCaptureBackend`, `OcrBackend`, `MkOcrLanguage`, `ExecResult`/diagnostics, `recognize_region`, and `OcrDocument::recognized_text`. Existing executor and authoring callers remain on the same pipeline. The backend is Send + Sync and accepts a cancellation callback. `ScreenCaptureBackend::capture` validates desktop bounds/dimensions and preserves the signed origin.

**Required changes:** (1) Inspect these direct callers and existing tests before editing. (2) Introduce an injectable shared facade composing capture/OCR backends, with a selected `ScreenRect`, explicit language, and cancellation input. (3) Delegate recognition to `recognize_region`; return the reconstructed plain text and useful structured failure context without exposing WinRT to UI. (4) Ensure pixels/documents are released when no longer needed; the facade has no file/history/clipboard side effects. (5) Preserve a straightforward path for later capture-completion separation using the same pipeline; defer that split unless needed now.

**Invariants/non-goals:** Shared invariants above. No language policy yet (M1-B); no workflow, command, parking, UI, or OCR stack migration. Reusing existing MkMacro domain primitives at this adapter boundary is intentional and allowed; UI/editor-state dependence is forbidden.

**Tests/verification:** Fake backends prove exact signed supplied rectangle/dimensions, multiline text reconstruction, one capture, cancellation before/during work, distinguishable capture versus OCR failure, and no downstream recognition on failed capture. Reuse existing coverage rather than mirroring internals. Run `cargo nextest run --lib -E 'test(ocr::)'` and `cargo check --lib` as useful; report actual commands.

**Done:** Shared exported facade works with injected fakes; one existing pipeline is used; failure/cancellation context survives; source image is not persisted; scoped tests/check pass; diff contains only intentional files. **Genuine uncertainty:** none affecting product scope; implementer chooses the narrow public result/error representation consistent with current diagnostics.

## M1-B — English policy

Status: complete. Depends on M1-A. Commit: `feat(ocr): [M1-B] add English-only general OCR language policy`.

Owner/scope: shared OCR service language resolution, not the Windows backend's global policy. Enumerate supported languages on explicit work, select English deterministically using the M0 preference, pass an explicit tag, and provide actionable local missing-English/query-failure errors. Preserve all MkMacro language inputs. Test one/multiple English variants, mixed languages, case handling, reordered input, no English/empty lists, and enumeration failure using fakes. Verify the new language/service module with targeted Nextest. Done when general API cannot silently use Auto/non-English, every supported English variant can be used as fallback, and MkMacro semantics remain unchanged. No new selector/settings/profile API required.

## M2-A — selector purpose and isolated event ownership

Status: complete. Depends on M1. Commit: `feat(ocr): [M2-A] add general OCR rectangle capture purpose`.

Owner/scope: `src/gui/mkmacro_dialog/{visual_overlay,visual_capture_workflow}.rs` and exhaustive purpose matches/tests in `action_editor.rs`. Add `GeneralOcrCapture`; reuse existing native interaction with a narrow controller adapter. Isolate OCR completion/cancellation/error events from editor and Screen Draw drains, retain ID ownership through replacement and queued events, and cancel/retire only the requested operation. Preserve worker recovery and terminal shutdown. No native overlay fork or authoring asset workflow. Tests: purpose round-trip, signed coordinates, held-release/Esc behavior, editor polling isolation, startup failure, replacement/stale cancel. Verify `cargo nextest run --lib -E 'test(visual_overlay::) | test(visual_capture_workflow::)'`. Done when one service supplies OCR selection and all unrelated owners retain their own events.

## M2-B — transient selection lifecycle and parking

Status: complete. Depends on M2-A. Commit: `feat(ocr): [M2-B] integrate OCR region selection with launcher lifecycle`.

Owner/scope: OCR controller/state and `src/gui/ocr.rs`, `src/gui/{mod,render}.rs`; reuse `launcher_parking.rs` and visibility ordering. Capture pre-invocation presentation/query/selection as needed; obtain current desktop, park/verify exact native geometry, then start the selector and record generation + operation ID. Keep confirmed geometry staged until M4 consumes it. Cancellation/error retire selection and restore; errors are useful, ordinary cancel is silent. Newer visibility requests win over stale restore/result publication; failed restore retains ownership for retry. Coordinate conflict admission with Color Pick/Screen Draw and suppress root geometry changes while OCR owns parking. Repeated starts supersede the prior OCR generation with cleanup before fresh selection; never two selectors. Keep module/controller hooks available without user entry until M3. Tests use injected overlays/window APIs: begin, confirm, cancel, error, stale/replaced event, newer visibility and restore failure. Verify new OCR lifecycle tests plus directly changed parking tests and compilation. Done when selection can be exercised without GUI-driving, root ownership is released on all terminal paths, and no OCR/capture runs prematurely. No recognition/result rendering yet.

## M3-A — first-class command/plugin

Status: complete. Depends on M2. Commit: `feat(ocr): [M3-A] register first-class screen OCR launcher command`.

Owner/scope: new `src/plugins/ocr.rs`, `src/plugins/mod.rs`, `src/plugin.rs`, command model/parser/host/bus/handler exports and `headless.rs`. Emit one stable normal action for exact `ocr` query/prefix; register through normal enablement/inventory. Use a typed OCR Start command and narrow host method; no GUI magic string. Keep every command match compiling with an explicit GUI-only headless policy; never external-launch OCR. At this checkpoint typed handler can use a test host, but public action must have a coherent host adapter in M3-B before task completion. Test exact query boundaries/case, disabled routing, inventory, parser, bus/handler, headless behavior; refactor FakeHost implementations only as required. Verify command/module tests and `--test plugin_commands`, `--test plugin_routing`, `--test plugin_exact_match` with focused filters. No alternate hotkey or radial implementation.

## M3-B — activation to lifecycle

Status: complete. Depends on M3-A/M2-B. Commit: `feat(ocr): [M3-B] route OCR activation into region workflow`.

Owner/scope: `src/gui/command_host.rs`, OCR lifecycle admission, relevant activation tests. Connect typed host request to exactly one general selection operation. Use generic visibility Keep so the lifecycle owns parking; history records normal invocation metadata only, never captured text/pixels. Preserve activation-source support through the existing command bus, including a non-primary/radial source test. Confirm no MkMacro/Screenshot Editor opening and no recognition before confirmation. Verify targeted host/handler/activation tests and compilation. Done when normal plugin action actually starts the intended lifecycle and repeated invocation policy is covered; do not add OCR-specific action-assignment plumbing.

## M4-A — exact selected-region capture

Status: complete. Depends on M3/M1. Commit: `feat(ocr): [M4-A] capture selected OCR region through shared backend`.

Owner/scope: shared OCR service/controller and GUI capture lifecycle seam. On current confirmation (native overlay already closed), capture exactly once using `SearchRegion::Rectangle` and exact signed geometry while launcher remains capture-safe. Stage immutable pixels with operation identity; signal capture completion/failure so restoration can proceed before long recognition. Do not re-capture tiles or use screenshot save/history paths. If needed, extract captured-region recognition from existing `recognize_region`, leaving it as capture + shared helper for MkMacro. Test negative/cross-monitor rectangles/dimensions, one capture, empty/outside/overflow geometry, backend failures and cancellation. Verify service/controller and relevant `mkmacro::screen` tests. Done when selected pixels flow once to the shared pipeline with no persistence, and launcher restore cannot contaminate capture. No copied recognition implementation.

## M4-B — asynchronous recognition and publication

Status: in_progress. Depends on M4-A/M1-B. Commit: `feat(ocr): [M4-B] run OCR recognition asynchronously with stale-result guards`.

Owner/scope: OCR job/controller + focused GUI polling/repaint. Run capture/recognition away from egui as appropriate; local language discovery also must not block a frame. Use thread/mpsc/atomic cancellation precedent from `ocr_test_job.rs`, but carry general generation, not draft identity. Publish only current completion; restore launcher after capture and expose Recognizing status, then Result/NoText/Error. Closing/new invocation/re-capture invalidate outstanding jobs; completed workers never resurrect UI. Extract text and drop image/document before publishing a text-only result. Test worker-thread execution, success/failure, cancellation, two attempts, late completion after close/replacement, repaint, and resource release. Verify targeted OCR job/controller tests and compile. No permanent OCR polling/capture thread or fake percentage.

## M5-A — editable result and explicit copy

Status: pending. Depends on M4. Commit: `feat(ocr): [M5-A] add editable multiline OCR result surface`.

Owner/scope: focused OCR result UI and normal panel/lifecycle hooks in `src/gui/{mod,render}.rs`. Render compact title, vertically scrolling/selectable/editable multiline text, Copy All, Re-capture, Close, and lightweight Recognizing. Initialize from `recognized_text()` only; current edited text is sole copy source. Opening/success never writes clipboard. Use existing clipboard/error conventions and stable widget identity. Test state/actions independently: initial lines, edits update explicit copy, no implicit clipboard call, close/re-capture intent. Verify result/controller tests and GUI compile. No cleanup heuristics or additional integrations.

## M5-B — no-text and useful errors

Status: pending. Depends on M5-A. Commit: `feat(ocr): [M5-B] add OCR empty-result and failure states`.

Owner/scope: result/controller error presentation. Whitespace-only recognition is NoText with Re-capture/Close and no Copy; an edited empty result must not overwrite clipboard. Distinguish unavailable OCR, missing English, capture failure, recognition failure. Present concise actionable text and log useful diagnostics using existing infrastructure. Normal Esc remains cancellation. Test every state/copy guard and errors; verify result/controller filters. Done when errors/empty text are understandable and clipboard unchanged except explicit nonempty copy.

## M5-C — re-capture/close lifecycle

Status: pending. Depends on M5-B. Commit: `feat(ocr): [M5-C] complete OCR recapture and close lifecycle`.

Owner/scope: same OCR controller/lifecycle and panel close hooks. Re-capture clears old text/image, retires current identity, parks again, and starts one fresh selector; no nested surfaces. Close cancels/invalidate work, tears down owned overlay, restores as needed, discards text, releases lifecycle, and returns normal launcher idle behavior. Test consecutive captures, NoText retry, close in recognizing/selection/result, stale completion and repeat close. Verify targeted controller/GUI lifecycle tests. Done when all UI actions use controller transitions and resources cannot outlive the current workflow unnecessarily.

## M6-A / M6-B — focused hardening

Status: pending. Depends on M5. Commits: `test(ocr): [M6-A] cover virtual desktop and capture geometry`; `test(ocr): [M6-B] harden OCR cancellation and restoration behavior`.

M6-A owns additional deterministic coverage genuinely needed for left/above/secondary monitors, cross-monitor rectangles, negative origins, physical dimensions, gaps, and no UI DPI conversions. Reuse existing compositor/picker tests; add OCR adapter-specific assertions. Run relevant screen/overlay/OCR geometry filters.

M6-B owns remaining lifecycle defects/coverage: Esc before/during drag, close during recognition, near-simultaneous invocations, stale result after re-capture, overlay/capture/OCR errors, exact restoration, newer visibility request and no leaked native/exclusive ownership. Run the affected state/lifecycle tests; do not duplicate already adequate coverage. Hardware scaling/multi-monitor native behavior remains part of required manual smoke. Done only when concrete integration edges have evidence; do not use these stages for broad reliability redesign.

## M7-A — MkMacro compatibility gate

Status: pending. Depends on shared changes being coherent. Optional diff commit: `test(mkmacro): [M7-A] protect shared OCR compatibility`.

Verify existing Find/Click/Read Text, OCR conditions/Wait Until, configurable/Auto languages, region capture/tiling/reconstruction, output/match/occurrence semantics, schema and visual authoring/debug. Use `cargo nextest run --lib -E 'test(mkmacro::ocr::) | test(ocr_execution_tests::) | test(ocr_test_job::) | test(ocr_controls::)'`, plus directly affected serialization/debug tests if their code changed. No complete unrelated MkMacro campaign. Record results without empty commit when no test/code diff is needed.

## M8-A — discoverability/documentation

Status: pending. Depends on feature behavior being coherent. Commit: `docs(ocr): [M8-A] document local screen region OCR workflow`.

Update relevant help/plugin metadata and current user-facing docs to explain `ocr`, local English recognition, region selection, manual Copy, re-capture and close. Inspect existing help metadata conventions; do not add speculative unsupported features. Verify affected help/plugin tests and review wording. Done when documentation matches actual shipped behavior.

## M9 — final gates (all pending)

- **M9-A targeted:** collect passing coverage for service, English policy, capture geometry, selector ownership, workflow/jobs, command/plugin activation, result/copy actions, and MkMacro compatibility. Run `cargo fmt --check` and appropriate affected-crate compile/check. Search the cumulative diff for duplicated OCR/pipeline, stale bypasses, source/result persistence and unintended files. Report actual command results.
- **M9-B substantive broader:** after targeted success, perform broader repository verification once at this designated gate. `cargo nextest run` plus normal compile/check is the preferred default for this goal; if an environment constraint prevents it, record concrete evidence and limitation rather than claiming it passed or silently dropping the gate. Do not repeat broad runs without a new defect/change requiring them.
- **M9-C independent review:** use a reviewer for architecture, one engine/pipeline, MkMacro preservation, UI responsiveness, stale worker/overlay events, exact/newer-intent restore, clipboard, pixel/text lifetime, English-only general policy, coordinates, no persistence or scope creep. Resolve substantive findings and rerun affected checks; use `fix(ocr): [M9-R#] ...` commits when appropriate.
- **Required Windows manual smoke:** actual happy path (`ocr`, parked launcher, drag/release, Recognizing if visible, result/lines, edit, explicit Copy and paste); preserve preexisting clipboard before Copy; Esc cancellation and restore; blank region; re-capture shows only new result; focused existing MkMacro OCR operation. Test secondary/negative/cross-monitor selections and non-100% scaling when hardware/practicality permits, recording unavailable configurations. No GUI-driving framework required. Native computer APIs are unavailable to the current CUA setup, so human Windows evidence may be required; never mark manual smoke complete from deterministic tests alone.
- **Final acceptance:** every approved-plan checkbox needs evidence; preserve M9/manual gate status separately from implementation completion. Final report includes milestone status, subjects/hashes, changed modules/ownership, actual tests/checks/manual outcomes, limitations, and clearly unimplemented deferred ideas.

## Evidence and commits

| Stage | State | Commit | Verification/evidence |
|---|---|---|---|
| M0 | complete | none | Read-only current source/tests and complete approved plan inspected at 112cb87f; no build/test executed. |
| M1-A | complete | `24ef239a` | `cargo fmt --all`; `cargo test --lib ocr::tests`: 23 passed; diff inspected. |
| M1-B | complete | `911a6cc8` | Targeted Nextest `general_ocr_english_*`: 7 passed; installed profile preference/common variants/sorted fallback; diff inspected. |
| M1-R1 | complete | recorded in checkpoint history | Native repeated preference lookup remediation: tracked compile PID 17168 passed; direct test PID 16412 passed three successive fresh workers; PID 7700 passed seven English policy tests. Exit 0 and no matching Windows crash events; fmt/diff checks passed. |
| M2-A | complete | `cd54bc99` | Nextest overlay/capture-workflow modules: 66 passed; formatting and diff check passed; owner-aware OCR queue, cancellation ack retained. |
| M2-B | complete | `cd6dc40d` | Latest-source Nextest controller/GUI OCR/ColorPick: 23 passed; fmt/diff check passed. Typed selection, parking verification on separate polls, terminal ack, exact/newer-intent restore and retry. |
| M3-A | complete | `2c3e3ecd` | 6 command/plugin/host lib tests and 3 selected integration tests passed; fmt/diff checks passed. Normal typed action and disabled routing verified. |
| M3-B | complete | `6e8f77fc` | Latest-source 16 OCR lifecycle tests passed, including real Enter/Click/Dashboard/radial activation, hidden/no-flash cancel, duplicate/history and newer show; fmt/diff checks passed. |
| M4-A | complete | `a02d1260` | Nextest general/MkMacro OCR filter: 53 passed (includes substring-matched GUI OCR/handler tests); successful lib-test compile, fmt/diff checks passed. Shared borrowed frame pipeline, no GUI changes. |
| M4-B through M8-A | pending | — | Record each checkpoint individually as it completes. |
| M9-A targeted | pending | — | — |
| M9-B broader | pending | — | — |
| M9-C review | pending | — | — |
| Windows manual smoke | pending | — | — |


### M2 lifecycle refinement from focused planning

- Treat native terminal events as teardown acknowledgement: shared cancel_operation clears active_id immediately, so a missing active ID alone must never authorize restoration.
- Keep explicit cancel/close owner until terminal acknowledgement; invalidate successful confirmation first. A racing confirmation may acknowledge teardown but cannot stage capture.
- Stage parking and verify on separate frame polls after HWND discovery; reject active Color Pick/Screen Draw/unrelated overlay before mutation. Preserve exact geometry/newer visibility requests and restore-retry ownership.
- M2-B confirmation retains signed geometry and parking for M4 capture; capture/recognition/clipboard must remain unused at M2-B. Duplicate invocation while selection/restoration owns root may be a no-op, matching Color Pick.
- Root hooks: poll after HWND discovery; reconcile parking; suppress generic placement while OCR owns root; exit cancels only OCR and commits hidden without reopening. Color Pick/Screen Draw admission checks must reject OCR ownership.

M3-B integration reminder: actual radial/action invocation can occur while ROOT is hidden; current M2-B visible-root admission must be validated/adapted through real activation tests, preserving hidden state on cancellation and showing OCR result on success. Do not equate parsing/assignment tests with invocation evidence.

### M4 focused planning refinement

- M4-A extracts only existing post-capture recognize_region body into one borrowed CapturedRegion -> OcrDocument helper; preserve original recognize_region API/result and diagnostics, tiling/translation/reconstruction. General facade adds exact-region capture and captured-frame English recognition; no duplicate tiling.
- M4-B evolves existing selection controller into capturing/recognizing/result/empty/error, consumes Confirmed exactly once, uses generation-tagged ordered worker events: capture completed (no pixels), then text/error; failures before capture also end capture ownership.
- Restore only after capture stops; continuation differs: success restores/shows recognition surface, cancel restores prior state. Keep transaction/retry, stage completion while restore fails. Root parking ownership must end independently from active generation/intent guards after restore.
- Newer query/visibility during capture/restore/recognition invalidates publication; update saved owned revision for OCR's own restore. Close during capture waits capture terminal; during recognition cancels/invalidate and can return idle without awaiting WinRT, worker keeps/releases pixels.
- Handle thread-spawn/disconnected/panicked worker terminal failure. Tests block fake capture/OCR to prove parking boundary/background execution, capture once, staged completion on retry, stale close/new generation and error cleanup. M5 adds UI, no pixels in UI/completions.

M3-B implemented hidden-origin admission/cancel. M4 success must intentionally publish visible recognition/result; choose ApplyConfiguredPlacement when prior_visible=false, retaining exact PreserveCurrentGeometry for visible-origin restore. Cancel prior-hidden retains committed capture-safe parking to avoid flashing an onscreen snapshot when original hide is in flight.

### M4-B natural checkpoint split

- M4-B1: independently functioning one-shot async worker, ordered generation-tagged capture-complete/text/error events, cancellation/repaint and spawn/disconnect/panic handling, fake worker tests. No GUI/controller migration yet. Commit: feat(ocr): [M4-B1] add cancellable asynchronous OCR worker.
- M4-B2: evolve current controller and integrate GUI root capture/restoration boundary and stale completion guards, migrating directly affected lifecycle tests. Commit: feat(ocr): [M4-B2] integrate asynchronous OCR with launcher lifecycle. M4-B overall stays pending until both sections and scoped checks complete.

### Active native reliability remediation (user directive)

- Forward feature integration is paused until the reproduced native boundary is understood and stable. Every executable/test/preview/manual run must record exact spawned PID and owned descendants, enforce a bounded timeout, preserve stdout/stderr/exit status and available crash diagnostics, and terminate only that invocation's process tree on crash, hang, modal crash dialog, or timeout. Cleanup is a failed run; never kill by launcher process name.
- M4-B1 deterministic fake-worker tests passed (5). Native sequential profile lookup probe failed (session 17331, exit 1): first thread completed RoInitialize, GlobalizationPreferences::Languages, Size/GetAt and RoUninitialize; second completed RoInitialize then stalled inside GlobalizationPreferences::Languages. The bounded test timed out. This is native failure evidence, not successful verification.
- Retrospective elevated process inventories found no remaining matching runner/test or Windows crash reporter; no process was terminated. The earlier launch PID was not recorded, so it cannot be reconstructed or claimed. A recent Application log query returned no matching launcher crash event. The stall's lifetime cause remains under investigation; no access violation has been independently reproduced yet.
- Original probe output is preserved at `C:/Users/Jay/AppData/Local/Temp/ocr-native-probe-17331-evidence.txt`. Remove temporary tracing before checkpoint. Introduce owned-PID containment before further reproductions. Any fix to a previously committed boundary gets a separate remediation commit with reproduction, ownership/lifetime change, and focused verification evidence.
- Contained comparison succeeded: runner PID 20160, exit 0, run ID 9239cb03; two consecutive native preference workers completed in 20 ms after acquiring a fresh scoped activation factory instead of the generated process-static factory cache. Both completed WinRT teardown. This establishes the remedy for the reproduced lookup stall; it does not establish the cause of the user's reported access violations. Evidence: `$env:TEMP/ocr-fresh-factory-contained-20261006/` (stdout/stderr, process tree, result, Windows events). Final source tracing removal and focused regression verification remain pending before a separate M1-R1 remediation checkpoint.
- M1-R1 final source verified: factory and language-vector ownership are scoped within the initialized apartment and dropped before balanced RoUninitialize. The regression exercises three fresh worker lifetimes with bounded acknowledgement. Temporary tracing and B1 worker export/provider seam were excluded from the remediation. Direct executable runs record exact test PIDs; all eight selected tests passed with exit 0 and no matching Windows events. Detailed evidence directories: `C:/Users/Jay/AppData/Local/Temp/ocr-M1R1-compile-20261006`, `ocr-M1R1-native-20261006`, and `ocr-M1R1-policy-20261006` under the same Temp root. Containment wrapper: `C:/Users/Jay/AppData/Local/Temp/RunOwnedOcrTest.ps1`.
