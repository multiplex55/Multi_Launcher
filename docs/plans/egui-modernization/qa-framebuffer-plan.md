# Opt-in framebuffer evidence probe — implementation packet

Planner packet,2026-10-13. Execute sequentially after M1-D source ownership releases. This is a bounded QA prerequisite for actual screenshot evidence after repeated Windows.Graphics.Capture transport failures, not a rendering redesign. User authorizes app launch/use/screenshots. Current implementation is in progress; no successful native capture has occurred.

## Objective and ownership

Add one feature-gated `src/gui/qa_framebuffer.rs` owner using existing image0.24,serde_json,sha2 dependencies. Use supported egui0.36.2 `ViewportCommand::Screenshot(UserData)` and matching `Event::Screenshot { viewport_id, user_data, image }`. Vendored Glow reads the painted visible viewport framebuffer and returns pixels on subsequent input. No vendor or OS automation changes.

Ordinary builds exclude the probe through an empty `qa-framebuffer-capture` feature. Main::spawn_gui reads one request-manifest environment variable before app construction and installs an optional probe; test/benchmark constructors remain disabled. Reject performance instrumentation or ML_TRACK_C_BENCH_MODE. Benchmark scripts omit the feature and unset its request variable.

## Configuration and provenance

Manifest carries run/request ID, ROOT target, trigger, fresh output directory, fixture-manifest hash and build-receipt identity. Validate once. Exclusive output creation prevents evidence overwrite. Parent launch receipt binds source commit, probe patch hash, Cargo feature/profile, executable SHA256, PID/start time and actual fixture hashes. Save actual executable identity alongside supplied provenance; unchecked labels do not prove source identity.

## Integration

State: disabled -> armed -> awaiting pixels -> saving -> complete/failed; no retry or automatic exit.

1. Startup trigger becomes eligible after the first completed visible ROOT UI frame, after existing on_ui_frame_complete receipt finalization. Request in the following ordinary visible ROOT UI pass. Optionally support isolated-build local unmodified F11 after accessibility interaction; consume before ordinary root routing, only ROOT focused and no modal/key-capture/tool owner. One shot, no global hooks.
2. At existing render_root_ui first-pass guard on normal visible paint path, send one ROOT Screenshot command with pre-created typed UserData token; transition once and request one follow-up repaint. Commands accumulate across discarded passes, so do not duplicate. Exclude hidden bootstrap/OCR-only paths.
3. At guarded begin_root_frame_logic start before other input owners, match ROOT and typed token; remove only matching screenshot event. Preserve foreign events and input. Ignore stale/duplicate replies after transition.
4. Transfer returned Arc<ColorImage> to one bounded worker. Convert Color32 with supported unmultiplied sRGBA conversion, preserve upstream orientation, PNG encode via existing image0.24; save one completion receipt. Encoding/hash/IO stay outside frame path. Worker completion requests repaint; enabled owner checks channel. No per-frame filesystem polling or repaint loop. External launch deadline records missing event/process exit as failure.

Receipt distinguishes issuance, receipt, save; includes run/request/viewport, frame/pass identity, dimensions, pixels-per-point, query/result generation where applicable, visibility ownership, times, PNG SHA256, executable identity and manifest hash. CPU frame completion is not display/GPU completion.

## Verification and done

Focused state tests: disabled path, benchmark rejection, one request across discarded passes, matching/foreign/duplicate events, save failure/no retry/no overwrite. Scoped compilation including feature. One real isolated fixture launch yields readable PNG and matching receipt. No pixel assertions/full suite. Inspect source diff and obtain bounded review. Parent owns launch/native observations/Git/docs. Do not widen scope to screenshot automation frameworks.

## Original baseline follow-up

A separate instrumented original0.27 build preserves original binaries/benchmarks unchanged. Original command is unit Screenshot; event has viewport/image without UserData. One outstanding ROOT request locally associated with run ID, reject unsolicited events. Integrate through original App::update only; separately attribute patch/build/executable identity.

Framebuffer evidence proves application-rendered pixels/layout, not OS composition/occlusion/layering/focus/click-through/physical releases/monitor placement/display latency. Native observations remain separate.

## Essential review correction: native Glow scheduling

Independent source review found Glow paints and drains previously queued actions_requested, then processes THIS frame viewport_output only after swap. A command issued in ROOT UI frame N therefore reads pixels during frame N+1; the reply is serviced on later input N+2. Screenshot enqueue does not itself request another repaint. The probe must request exactly one follow-up at issuance for the capture frame and a bounded second ROOT repaint after that post-issuance retained UI frame for reply delivery. Track these frame transitions explicitly; no awaiting-state repaint polling or retries.

Receipt must distinguish issuance frame/state, expected capture frame/state, actual event receipt frame and successful save. Do not label issue-frame query/generation as the photographed content. Use the existing final retained ROOT UI completion hook to identify the next capture-frame CPU receipt and request the second repaint; ensure repeated/discarded passes cannot advance this state twice. The hook remains before GPU presentation and must be labelled accordingly. A process deadline records any missing capture/reply as failure.

## Essential review correction: visibility and input-owner changes

While armed/awaiting pixels, revalidate logical visibility and ROOT/OCR/tool ownership before issue and at subsequent lifecycle boundaries. A hide/OCR/tool transition terminally fails/cancels this one-shot request; never leave it eligible until resume. Hidden native logic discards raw UI events, and a queued screenshot could otherwise execute after a later visible frame. begin_root_frame_logic cannot consume reply in logic_only. If issue preceded a later same-frame hide, mark the request terminal and ignore delayed matching replies; do not save them as the requested visible state. Tests must cover these bounded transitions without native polling/retry mechanisms. Reviewer otherwise accepts feature-off/benchmark exclusion, constructor default, worker/token/orientation and separate original attribution boundaries.

## Retained-pass marker clarification

Existing finish_ui_frame_receipt clears root_frame_logic, and pending receipt is normally None with performance/query instrumentation disabled. Do not enable Track C or depend on that optional candidate. Stage a tiny feature-gated eligible-surface marker owned by the probe on every UI pass, including hidden/OCR early returns; overwrite it per pass so retained hidden/OCR state replaces discarded visible state. Read the final marker after existing completion finalization. This is the probe's own bounded QA state, not another production frame owner or performance receipt path.

## Original0.27 archived-source backport handoff

Read-only planner confirmed baseline72a01bd integration: main::spawn_gui900 creates app/bridges then returns Box; render_root_frame2085 captures native handle and routes snippet keyboard before launcher input; App::update3786 delegates to it. Context::run executes once (no discard-loop contract). Original Glow paints, reads previously armed screenshot, swaps, then processes current viewport commands; unit Screenshot and ROOT event have no UserData.

After current-probe source/native validation, parent creates a fresh ignored source archive of72a01bd and assigns one sequential writer there. Keep currentcheckout and preserved binaries/performance receipts untouched. Minimal archived changes only: empty QA feature, optional owner/module/disabled constructor field, one-time main manifest init, pre/post App::update seams, and normal-tail render eligibility marker. Do not backport current lifecycle/vendor/Track C machinery.

Update start resets marker and processes screenshot/F11 before ordinary routing; existing render_root_frame order/signature stays intact; normal tail stages visible/root ownership/query-generation/frame/PPP metadata (OCR early-return remains ineligible); update tail finalizes once and revalidates before issue/accept/save. For issueN request capture repaint; after expected captureN+1 tail record CPU state and request reply repaint; acceptROOTreplyN+2 only in expected outstanding state. Terminally cancel on hidden/OCR/tool/owner/incompatible transitions. No resume/retry/polling loop. Preserve foreignviewportevents; local correlation is weaker without UserData and must be labelled.

Reuse compatible current manifest/worker/exclusive-write/provenance/benchmark-exclusion logic only. Startup after first eligible completed update, issue following update; local unmodified F11 only explicitly enabled/focused/no inputowner conflict. No synthetic/global hooks. Tests use original Context::run for scheduling, unsolicited/foreign replies, cancellation/F11ownership/save failure; no discard tests. Parent builds archived manifest with direct Cargo, lockedofflinefast-dev QAfeature, separate CARGO_TARGET_DIR, and separately hashed probe patch/executable/fixture/request/process receipts. Actual readable PNG required; label instrumented original0.27 framebuffer, never preserved unmodified output. No source/build/run occurred in planning.
