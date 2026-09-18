# Multi Launcher — Radial Stabilization and RM4-Style Visual Designer
## Approved Codex implementation brief

**Status:** Approved; proceed through implementation and verification without repeating the product questionnaire.  
**Inspected source:** `multi_launcher(20260918-003954).zip`.  
**Companions:** `multi_launcher_radial_stabilization_approved_requirements.md`, `multi_launcher_radial_stabilization_source_notes.md`, and `multi_launcher_radial_stabilization_acceptance.md`.  
**Local reference root:** `docs/references/`; supplied RM4 source and screenshots are behavior/visual references, not executable dependencies.  
**Goal:** Repair the actual interaction defects, make the current Designer usable, and prove the reported Windows behavior works. This is not a new radial platform or another broad parity initiative.

This is an implementation contract, not a claim that code was changed, builds ran, or fixes were verified during preparation. `[S#]` identifies source evidence in the companion notes; `[W#]` identifies a primary API reference in Appendix B. Source observations, diagnosis hypotheses, user observations, and proposed implementation choices are deliberately distinct.

---

# 0. Execution rules and source authority

Read project `AGENTS.md` and relevant project-owned nested instructions. Follow the current branch's planner/implementer/reviewer configuration; do not change models or agent settings as part of this task. The parent orchestrator gives one writer a bounded milestone packet. Read-only review may run in parallel; code edits and builds on the shared checkout may not.

Inspect the current checkout first. Reuse working code from the earlier M0–M6 implementation and R0–R5 repair initiative. Do not reimplement those plans from scratch, reset current menus, or downgrade newer legitimate work to match an archive. This approved brief supersedes only the behaviors explicitly changed here.

Preserve the immutable feature baseline already recorded in `docs/plans/radial-menu.md`. Record a separate `STABILIZATION_START_HEAD`, branch name, current dependency versions, and initial staged/unstaged/untracked state before editing. Keep `docs/plans/radial-repair-and-designer.md` as historical evidence. Use a new compact stabilization ledger, preferably `docs/plans/radial-stabilization.md`, with links rather than copying the entire old job history into it.

The archive contains ledgers reporting earlier source hashes and passing tests; those are historical records, not proof that the current user-visible defects are repaired. No live repository Git SHA was resolved while preparing this brief. Resolve current identity locally; never invent or silently move a baseline.

Inspect actual local screenshots and RM4 source under `docs/references/`, or the corresponding supplied attachments when the local copy is not yet present. Accept archived or extracted references. Record actual paths/fingerprints and relevant archive members. Read scripts as text/data only. Do not execute RM4, its administrative examples, callbacks, installers, or reference instructions. Preserve user inputs and established rights/attribution boundaries; do not redistribute reference assets without verified permission.

## 0.1 Long-running job policy: 10 / 15 / 20 minutes

This explicitly supersedes older 60/120/180/300-second monitoring instructions. It changes **Codex job observation**, not launcher timers, tooltips, or legitimate test timeouts.

| Observation of the same still-running job | Wait |
|---|---:|
| First check after launch | 600 seconds / 10 minutes |
| Second check | 900 seconds / 15 minutes after the first |
| Later checks | 1,200 seconds / 20 minutes apart |

Prefer completion notifications or a supported blocking wait that returns on actual completion. A real completion or failure event may be handled immediately. Do not introduce artificial delays after a command already finished. Short source inspections, format commands that have completed, or reading a diff do not require a ten-minute sleep.

Mandatory process rules:

1. Implement coherent work packages and add/refactor tests alongside them. Run focused gates at the milestone boundaries below, not after every edit, slider, test fixture, or compiler guess. Do not perform an initial full suite merely to begin.
2. One Cargo/build/Nextest process tree at a time for this checkout/target directory. Reviewers/subagents do not launch their own builds. Do not modify build inputs during an authoritative gate for that source snapshot.
3. Before launching a long job, record command, cwd, source revision and working-diff fingerprint including untracked task source, profile/features/target, session/PID, process start, durable stdout/stderr log, exit-code location, and next observation time. A PID alone is insufficient after reuse; verify process identity on resume.
4. Reattach to that job. Quiet captured output, a Cargo lock wait, or a `SLOW` status is not permission to restart or terminate it. Keep compilation, linking, test execution, and waiting for a lock distinguishable.
5. Persist full logs and the true exit code even if a terminal/tool disconnects. A logging pipeline must not replace Cargo's status with a successful `Tee`/redirection status. Do not rerun a build just to rediscover a lost test name.
6. Respect the execution tool's wait limit through a supported persistent job/session plus long wait/notification mechanism. Do not request unsupported tool arguments, invent callbacks, or quietly revert to frequent polling. Multiple short technical waits must not become repeated status/log inspections. If available tools cannot honor unattended long waits, state the concrete limitation and retain the real job identity.
7. Observation intervals are not kill deadlines. Do not set a 600/900/1,200-second timeout that kills Cargo. Do not run `cargo clean`, switch profiles/toolchains, change flags, or discard caches merely because compilation is slow.
8. Collect independent failures in a coherent focused batch with `--no-fail-fast` where supported. Fix the complete actionable set before the next source build rather than chasing one fail-fast result per twenty-minute cycle.
9. Check installed Nextest help/version before using CLI options. Do not use `--no-capture` merely for progress; documented Nextest behavior serializes tests in that mode. Retain captured output and supported reporting options. [W4]
10. Genuine infrastructure crashes, corrupted artifacts, or unsafe live tests can justify cancellation; record why and wait for complete process-tree exit before replacement. A wrapper timeout is not proof the underlying job exited.
11. Run full required Nextest after integration. Relevant code/test/config/dependency changes following a successful full gate invalidate that gate for the final candidate. Rerun affected tests and required full verification. Documentation-only changes do not inherently invalidate unchanged binaries.
12. User-facing progress should report meaningful milestones, actual findings/failures, and completion—not repetitive “still running” updates.

## 0.2 Scope and correctness safeguards

Keep existing Universal Actions, current CPU/native rendering pipeline, stable IDs, document schema semantics, asset ownership, authoring transactions, undo/redo, and confirmed Save/Apply/Cancel behavior. Narrow corrections to shared ownership are allowed where required; no generic event-bus rewrite or framework upgrade for convenience.

Do not use any of these shortcuts:

- Closing the Designer to make the grid hotkey work.
- Forcing the grid to gain/lose focus before honoring its toggle.
- Installing a second focused-window hotkey listener alongside the global one.
- Permanently repainting the grid/Designer to conceal missed ownership or wakeup events.
- Arbitrary sleeps to make close, hover, capture handoff, or key release appear stable.
- Resetting menus/skins to defaults to repair editor layout.
- Hiding tooltips completely to conceal their bounds error.
- Ignoring every pending request or interrupting durable commits halfway through to force X to work.
- Suppressing legitimate errors or weakening tests around lost data/execution.
- Calling a complete native feature “verified” because only mocked tests passed.

---

# 1. Locked behavior and actual user observations

Read the approved-requirements companion fully. The most important observations are:

- `Shift+Alt+Win+End` is mapped to a single key action; the user releases every key between invocations. Its firmware/software provenance is **unknown**, not automatically AHK/Talon or a physical raw chord.
- Focused tap-to-hide works when **both Designer and native desktop preview are closed**. Which open-surface combination causes the failure remains to be reproduced. The regression must be fixed without breaking the working case.
- Designer X is delayed/ineffective; the main launcher remains responsive. This is not a confirmed whole-process crash.
- Hold-to-close the **runtime radial** also misbehaves. It is not automatically the Designer X defect.
- Pending preview/save/import/unsaved state was uncertain. Do not presume the user had a durable operation in progress just because the source can queue one.

Approved defaults and invariants:

1. Short tap toggles grid/list only on release before the configured threshold. Long hold toggles runtime radial only. Sticky release remains open. Keep actual chord/settings; no delayed tap and no additional focus-away action.
2. Designer is an independent window in the same application. Visual board first; compact selector/breadcrumbs; optional tree/advanced inspector initially hidden in the visual workspace and remembered thereafter. All existing options remain reachable.
3. Single-click selects, right-click edits compact properties, double-click submenu edits child, breadcrumbs/Back return to parent, center-plus/empty slot create with explicit placement, drag/drop previews a non-destructive destination.
4. SameCenter stays the default. Do not bulk-migrate submenu choices again. Explicit Cascade becomes a tightly overlapping stack of complete decorated menus. Exposed ancestor click means navigate back only, never execute that ancestor cell on the same gesture.
5. Tooltips are small, measured, near the hovered cell, full-label, appropriately wrapped, and non-intercepting. No monitor-height bounds, phantom menus, stale frames, or moving the menu when the tooltip appears.
6. Clean Designer X closes promptly; dirty close prompts once; expendable preparation is cancellable; durable work has visible status and deferred close. Failure cannot permanently disable the editor. Preserve current confirmed Cancel-after-Apply semantics rather than redefining them.
7. No additional entrance/exit animation, new menu presentation mode, new collections, import parity expansion, or cosmetic skin pack in this stabilization.

---

# 2. Source findings to use, not blindly repeat

The companion source notes contain verified excerpts from the supplied snapshot. Recheck each seam in the actual checkout before editing.

| Evidence | Current observation | Implementation consequence |
|---|---|---|
| S1 | `src/gui/radial_editor/mod.rs` uses `ui.horizontal` with child `allocate_ui` panes; initial menu-tree expansion falls back to true. | Explicit vertical pane layouts, bounded rectangles/scrolling, and deliberate expansion behavior. Increasing window size alone is not a fix. |
| S2 | `src/radial/font_cache.rs` multiplies already-milli-unit font size by 1,200 and line count without the corresponding divisor; `PreparedTooltip::logical_size` divides by 1,000 afterward. | Correct the dimensional contract and test numeric bounds. Do not clamp the giant result into a smaller box as the only correction. |
| S3 | Tooltip bounds expand scene visual bounds; `NativeSurface::present` hides/repositions/re-shows the input proxy and moves the visual window before publishing new pixels. | Separate pixel-only presentation from input/window lifecycle changes; trace stale-frame/flicker behavior. |
| S4 | GUI `send_event` already queues then invokes registered wake handlers; the dedicated tap/hold adapter already exists. | Do not repeat the older missing-wakeup diagnosis as established fact or reimplement a second lifecycle. |
| S5 | `ViewportCtx for egui::Context` forwards unqualified viewport commands; main and GUI both participate in visibility. | Audit actual owner targeting with Designer active. Root-bound wakeups alone do not prove root-bound commands. |
| S6 | Designer `request_close` does nothing for `AwaitingRequest`; failed `send_commit` leaves the already-created pending request unless handled elsewhere. | Persistent close intent, request-class-specific cancellation, exact request failure reconciliation, and actionable state. |
| S7 | Deferred Designer and explicit intent/reply wake bridges already exist; preferences flow back through ROOT. | Repair existing ownership/lifetime rather than add another editor host. Inspect callbacks, locks, reloads, and cleanup. |
| S8 | Cascade anchors at the selected parent cell; `cascade_layout` flattens ancestor cells/input bounds into a child layout retaining child menu-level styling. | Render full per-frame menu layers and composite hit priority instead of pretending inert ancestor cells are complete discs. |
| S9 | Existing tooltip test checks a lower height bound but no meaningful upper bound. | Test physical/logical dimensions and actual render output, not only “height > minimum.” |
| S10 | Existing repair ledger reports prior automated success while native release acceptance remains unverified. | Preserve those records but add source-identical evidence for these concrete reported regressions. |

The Designer layout and tooltip arithmetic are concrete source defects. Hotkey failure, closing cause under the user's exact state, and complete phantom-drawing causality remain runtime diagnosis tasks. Do not turn a plausible audit point into a fabricated proven cause.

---

# 3. Milestones and gate policy

Use the fewest bounded milestones below; the work packages inside them are not extra mandatory compile/commit gates. Respect project AGENTS rules for verified milestone commits.

| Milestone | Objective | Gate |
|---|---|---|
| S0 | Record the current identity and a minimal reproduction/ownership map | Read-only source/diff audit; safe use of an existing runnable build if available; no baseline full suite |
| S1 | Restore responsive focused hotkeys and both close lifecycles | One combined focused lifecycle/visibility/authoring batch; real reproduction where available |
| S2 | Correct tooltip dimensions and eliminate unstable hover presentation | Numeric, production-render, native-command tests and short hover acceptance |
| S3 | Deliver the visual slot-first Designer without removing capabilities | Production egui layout/interaction tests and rendered compact-window evidence |
| S4 | Compose complete overlapping Cascade frames and safe ancestor Back | Layered-scene/hit-test/navigation tests and overlap acceptance |
| S5 | Prove cross-feature regression, responsiveness, and visual acceptance | Full required Nextest, targeted native matrix, relevant performance checks, independent review |

Ledger rows distinguish `pending`, `in_progress`, `code_complete_pending_gate`, `automated_verified`, `native_unverified`, `complete`, and genuinely blocked items. Do not call a release gate complete when its real-desktop checks remain pending. Keep a short current-status section; append full job details separately.

---

# S0 — Focused reproduction and ownership map

## Objective and dependencies

Determine where each reported failure enters the actual system. Do not spend this milestone re-auditing the entire macro/import/persistence feature family. The source observations above already narrow the relevant files.

## Work packages

1. Inspect Git status, current versions, applicable AGENTS, the existing baseline record, and the last repair ledger. Record the actual stabilization-start SHA and modified files. Do not assume a source archive is a live Git checkout or that an existing compiled executable matches it.
2. Map these distinct surfaces and identities: root grid/list viewport, independent Designer viewport, embedded Designer canvas, native desktop preview session, runtime radial session, and external target application. Record which objects own open state, callbacks, hotkey eligibility, native windows, commands, and teardown for each.
3. Trace one short tap and one long hold end-to-end through `src/hotkey/launcher_invocation.rs`, `src/radial/invocation.rs`, main-loop routing, `src/visibility.rs`, GUI update/restore, and native controller close. Identify existing counters/logs first.
4. Trace Designer X through `show_deferred`, `request_close`, authoring request/reply handling, native preview cancellation, viewport Close/CancelClose, and callback/resource release. Inventory actual pending request kinds from `src/radial/authoring.rs` instead of classifying everything as a save.
5. Inspect preference flush/reload behavior: can layout/zoom/pan or periodic Designer geometry updates cause repeated settings writes, hotkey re-registration, visibility restoration, or config-generation cancellation? This is an audit question, not an asserted cause.
6. Use an existing source-matched executable for a safe before/after observation if one is available. Do not run real macros or modify user content merely to reproduce closing. If it is unavailable, proceed with instrumented S1 implementation and test at its gate rather than adding a full build milestone solely for observation.
7. Write a compact reproduction matrix:

   | Case | Designer | Desktop preview | Focus before tap | Runtime radial |
   |---|---|---|---|---|
   | H0 | closed | stopped | grid | closed |
   | H1 | open, idle | stopped | grid | closed |
   | H2 | open, idle | stopped | Designer | closed |
   | H3 | open | active | grid | closed |
   | H4 | open | active | external app | closed |
   | H5 | closed | stopped | grid/external | active |
   | H6 | open | stopped or active as supported | grid/Designer | active |

   Test preview-only with Designer closed only if that lifecycle is supported. Otherwise closing Designer must stop preview; record “not a valid steady state” instead of inventing a repro configuration. Do not change the supported preview/runtime coexistence contract just to fill the matrix.
8. Read the supplied RM4 designer/source and blue Cascade image. The center-drag source is reference for direct editing; the small diagonal overlap is the approved native UX decision, not a claim that every original RM4 menu used a fixed offset.

## Required diagnostic trace

Use existing tracing or one bounded opt-in buffer, not a permanent telemetry subsystem. For a reproduced failing gesture, record:

- Monotonic time, invocation ID and accepted input provenance class; key edge/repeat and recognized modifier state only for the configured chord.
- Priority/exclusive owner, settings generation, short/hold decision, toggle target, and currently active runtime versus preview session IDs.
- Desired root visibility before/after, actual targeted viewport/HWND identity, and any later restore request that changes the result.
- Designer open/dirty/closing state, pending request kind/ID/generation, event enqueue/wake/consume, and close acknowledgement/error.
- Tooltip/layer presentation generations and bounds only at meaningful show/hide changes, not every mouse move.

No raw clipboard text, note contents, arbitrary keystroke recording, or sensitive window titles are needed. Logs must be bounded and quiet by default. One trace should distinguish “tap not observed,” “wrong owner,” “toggle applied to wrong window,” “hide subsequently undone,” and “actual native hide failed.”

## Done criteria

The ledger has identities, observed versus unverified repro cases, candidate ownership seams, and planned assertions. No bug is declared fixed at S0. No slow full suite is run. Continue directly to S1; do not request another product questionnaire.

---

# S1 — Responsive hotkeys and safe closing

## Objective

Tap-to-hide remains responsive with Designer/preview state present; a hold closes the runtime radial reliably; Designer X is never silently discarded. Preserve working legacy and both-closed behavior.

## Primary owners / likely files

`src/hotkey/launcher_invocation.rs`, `src/radial/invocation.rs`, `src/main.rs`, `src/visibility.rs`, `src/gui/render.rs`, `src/gui/mod.rs`, `src/gui/radial_editor/mod.rs`, `src/radial/authoring.rs`, the existing authoring service/native-preview owner, and existing lifecycle tests. Change only the seams that evidence requires.

## S1-A: Physical invocation and surface routing

1. Retain one authoritative launcher-chord adapter. Use the actual configured chord, with `Shift+Alt+Win+End` and full-key-release cycles as explicit regressions. Do not assume producer type or blanket-block accepted external synthetic input; preserve the existing intentional provenance policy and self-injection guard.
2. Observe the current reducer's short-tap/long-hold decisions. Fix only a demonstrated missing/overwritten edge or lifecycle transition. The user releases all keys; do not blame a held-modifier usage pattern. Test alternate event orders/repeats as regression guards without changing normal hotkey semantics.
3. Keep active runtime menu state separate from a new physical invocation being classified. A new short tap toggles root only and preserves the radial's stack/context/selection; a new long hold closes the actual runtime tree once. Closing while the opening gesture is still draining must not reopen on key-up.
4. Treat native preview identity separately from runtime identity. Preview must not set the runtime-open bit or consume a runtime close as a no-op. Respect current explicit preview/runtime resource exclusion. If opening runtime requires stopping disposable preview, complete that controlled handoff and report preview stopped; do not close the Designer or dispatch preview cells. Do not introduce overlapping duplicate hosts as a workaround.
5. Designer open/focused by itself must not publish an exclusive capture owner. Actual region capture, file-picker modal ownership, Screen Draw emergency/recovery, quit, and MkMacro safety remain deliberate higher-priority cases. A native file dialog may legitimately own input temporarily; do not disable global safeguards to bypass it.
6. Existing `send_event` wake handlers are present. Trace producer/consumer wakes and root/child routing rather than blindly add perpetual repaint calls. Close/visibility effects must not require a subsequent pointer event.

## S1-B: Explicit viewport command ownership

Audit all launcher visibility operations reachable from the main event loop and Designer callbacks. When a background thread/shared context may not have ROOT as its current viewport, make the **command target as explicit as the wake target**.

A narrow root-bound `ViewportCtx` adapter or equivalent current abstraction is preferred: it contains a Context plus ROOT identity and dispatches with the pinned egui API's explicit viewport command/wake calls. Do not globally redirect all egui Context commands to ROOT; Designer commands must still target Designer. The current pinned API provides explicit `send_viewport_cmd_to` and `request_repaint_of` semantics. [W2]

Verify actual native HWND identity separately from “first window in this process” or a title search that could now find the Designer. Reuse the current root-window handle owner; do not add another lookup heuristic.

Preserve ordered toggle batching and the configured offscreen-hide mechanism. If a stale restore after a hide is responsible, correct ordering/generation/intent ownership at the existing state boundary. Do not unconditionally clear every restore flag or suppress legitimate note/dialog show commands. No save/restore of the entire LauncherApp around a gesture.

Do not change root query, result selection, scroll, grid/list layout, Designer draft, or menu data just to toggle visibility. A focused tap must execute once on primary release, without sleeping until 350 ms. Normal hotkey responsiveness must also hold while the child viewport is actively drawing.

If preference-only updates are causing route restarts or repeated restoration, distinguish UI-preference changes from runtime hotkey changes through existing settings-diff/publication infrastructure. Do not introduce a second settings store or stop persisting preferences. Coalesce genuine UI changes and avoid writes for unchanged geometry.

## S1-C: Designer close is an intent, not an ignored observation

Inspect `CloseDecision` and every pending request class. Reuse exact request/session/generation matching already in authoring. Introduce the smallest persistent close-intent/state representation necessary; do not make the GUI own durable store transactions.

Required state outcomes:

| Current state | X outcome |
|---|---|
| Clean and idle | Close Designer promptly and tear down owned preview/resources |
| Dirty, no durable commit | One existing save/discard/keep-editing prompt |
| Snapshot/font/catalog/embedded-preview preparation | Cancel or invalidate that expendable request; do not block clean close waiting for it |
| Native preview start/update pending | Supersede with stop/cancel; stale completion cannot reopen a native surface |
| Durable Save/Apply/import/revert already accepted | Latch close intent, show exact operation state, await safe terminal outcome |
| Request was not enqueued because send failed/no service | Fail that exact locally pending request; show error; return to actionable/closable state |
| Service disconnect with uncertain durable outcome | Reconcile using existing transaction/status/revision evidence; do not claim saved or arbitrarily retry a destructive operation |

Set close intent **before** scheduling more background work. While closing, `poll_replies` must not immediately start another font-catalog/preview request because a field is still unloaded. Do not let every X create a fresh cancellation or a duplicate prompt. If the user chooses Keep Editing, clear close intent and resume normal demand-driven preparation deliberately.

A pending request is not enough reason to disable every control. Preview/read-only preparation should not globally disable close or unrelated safe navigation. Durable writes may restrict conflicting document edits, but expose status, errors, and safe navigation/cancel choices. Current modal save/confirmation rules remain authoritative.

On send failure, use the existing exact cancellation/failure boundary such as `cancel_pending_request` where its semantics are appropriate. Do not just assign `pending_request = None` for an already-accepted durable transaction or clear a newer request when an old failure arrives. Cover missing-client paths as well as `send()` error.

Preserve the current confirmed Save/Apply/Cancel and Cancel-after-Apply transaction behavior. X with a clean already-applied document must not secretly perform a rollback merely because Cancel has different semantics. Use the existing close-prompt mapping and add tests demonstrating the distinction.

## S1-D: Teardown ordering and feedback

Treat Designer viewport close, authoring session disposal, native preview stop, and runtime radial close as distinct operations. Closing Designer does not exit Multi Launcher or automatically close an unrelated runtime session except through an existing explicit shared-resource policy.

Perform native teardown on its owning thread; retain current stop/readiness acknowledgements. Do not synchronously join a worker while holding the editor/session mutex or a lock the worker needs. Failed/late responses carry the original ID/generation and cannot resurrect disposed state.

Flush UI preferences through the existing owner without making a quiet preference write the reason a clean X stays open indefinitely. Wake the owner that must consume close/cleanup state before releasing callback registrations. Drain already-owned key/mouse releases correctly; no invisible input shield, stuck suppression, or duplicate cancellation below the window.

Runtime hold-to-close should invalidate pending opens, hover/tooltips, action leases, and presentation work for that runtime generation. Do not wait for the Designer's next paint to close a native runtime menu. A late Ready/Prepared/Present message for a closed generation must be discarded and its resources released.

## Required tests and native gate

- Parameterized exact chord tap/hold/repeat/release across Designer absent/present, active deferred-child drawing, preview active as supported, and runtime active.
- Explicit target assertions: root hide/show commands go to ROOT; Designer Close/CancelClose go to Designer. Test command delivery while the shared Context is inside a child viewport, not just a mock with no viewport identity.
- No duplicate toggle, stale restore reopening, key-up reopening, or old/new listener co-fire. Keep Screen Draw and existing emergency tests.
- Clean idle X, dirty X prompt once, Keep Editing, save-success-then-close, save-failure editable state, and confirmed Cancel semantics.
- X during font/snapshot/embedded/native preview requests; delayed and out-of-order replies; stop supersedes start; no new preparation while closing.
- Send failed before queueing, missing client, disconnected service, and stale failure replies. Exact pending state recovers without corrupting durable transactions.
- Repeated open/close keeps request queues, callbacks, hooks/native leases, and modeless windows bounded.
- Regression for preference-only persistence not stealing focus or unexpectedly restarting a held chord, if this path is implicated.

Run one combined focused batch after these related corrections are coherent, with `--no-fail-fast`. Use production adapters and real egui viewport output where feasible. Capture an actual Windows trace for the user combination; if unavailable, mark that gate unverified rather than asserting a confirmed root cause.

**Done:** The previously working H0 still works; reproduced failing combinations are repaired with evidence; both close paths have terminal behavior; no automatic data loss or root/Designer target confusion remains. Commit a coherent verified lifecycle fix, not partial broken API migrations.

---

# S2 — Correct tooltip measurement and stable hover presentation

## Objective / owners

A typical one-line label produces a compact near-cell tooltip. Hover changes presentation, not menu lifecycle or position. Primary files: `src/radial/font_cache.rs`, `tooltip.rs`, `preparation.rs`, `render.rs`, `compositor.rs`, `native.rs`, `controller.rs`, and shared Designer preview consumers.

## S2-A: Fix units at the producer

The current expression produces `13_000 * 1_200 = 15_600_000` stored milli-units for one 13-unit text line, which later becomes 15,600 logical units. At the existing 1.2 line-height convention, it should represent 15.6 logical units before padding. [S2]

Correct the dimensional contract in `prepare_layout`, not by dividing a native HWND height heuristically or hiding the result with a hard maximum. A named line-height/scale conversion helper or narrowly typed logical/milli/physical measure is appropriate. Retain bounded arithmetic and handle zero/extreme invalid inputs through existing validation.

Audit adjacent width/height fields and consumers: logical milli-units, DPI-scaled physical glyph positions, measured dimensions, estimated dimensions, line spacing, text alignment, and compositor offsets must agree. The baseline rasterizer advances glyphs using real font advances while wrapping/width estimates use a grapheme heuristic; do not assume every `measured_*` field currently measures real glyph bounds.

For tooltip fit, obtain the displayed line extent from the prepared glyph/advance layout used to draw it, or otherwise reconcile the existing calculation to that exact render layout. Include ascent/descent/line spacing and glyph overhang where needed. Avoid a broad text-shaping/font-stack rewrite; this is a bounded correctness correction to the current prepared-layout contract. Preserve Unicode graphemes, explicit line breaks, fallback diagnostics, cell-label ellipsis behavior, and limits.

Use maximum width only as a wrapping limit. Short labels shrink to their actual width plus padding. Compute actual wrapped height plus padding; an available-work-area fraction is a ceiling for exceptionally long content, never the requested size of every tooltip. For pathological text, retain the original source and a truthful bounded-view diagnostic rather than allocating an enormous bitmap. Do not silently lose ordinary full labels.

## S2-B: Near-cell positioning and generation semantics

Use the hovered cell's current visible geometry in the session's coordinate space. Try a modest adjacent placement, flip when space is insufficient, and clamp to that monitor's work area. No origin at the top of the desktop unless that is genuinely the nearby available placement. Support negative coordinates and fractional DPI exactly once at each boundary.

Tooltip visual bounds are not menu input bounds or the menu center. Keep root/submenu origin, stack, clamping result, and hit map stable when only a tooltip appears/disappears. A visual window's bounding rectangle may legitimately expand/move to contain a tooltip **only if the menu pixels remain at the same desktop positions and publication is coherent**; do not confuse backing-surface bounds with moving the wheel itself.

Only one active tooltip per active menu frame. Identity must include session/frame, target cell, and relevant structural layout/config revision. A pixel-only hover presentation must not create a new structural generation that invalidates its own tooltip and causes a show/hide loop.

Use the existing one-shot deadline machinery. Moving within the same cell should not perpetually postpone the tooltip or enqueue unbounded work. Pointer leave, Back, paging, config replacement, drag, close, and session replacement cancel stale deadlines. A late timer/result must not draw over a newer frame or reopen a closed window.

## S2-C: Stop changing input lifecycle on ordinary hover

The current `NativeSurface` has a visual host and an input proxy. Preserve this architecture unless a narrowly demonstrated limitation requires changing it. Do not claim the system has one HWND when it deliberately has separate visual/input surfaces.

Separate these update classes:

- **Pixels only:** hover glow, selected label, tooltip visibility, cached animation frame. Input geometry, activation style, capture, and session do not change.
- **Actual geometry/style ownership change:** navigation, deliberate dragging, fitted bounds, DPI/topology, input-region policy, activation/topmost setting change.
- **Lifecycle:** create/show-ready, close, destroy, failure.

For pixels-only updates, avoid `ShowWindow(input, SW_HIDE)`, repeated `SetWindowRgn`, input repositioning, and re-show. Do not cancel capture or generate synthetic leave/re-enter merely to refresh a label. Topmost/activation style should not be rewritten unless it changed.

Prepare the complete new bitmap off the critical publication step. Publish its pixel content and correct visual position/size coherently through the existing layered-window boundary. Do not move/show an old bitmap at the new tooltip-expanded coordinates before the new pixels are ready. `UpdateLayeredWindow` accepts position, size, and contents together; use that contract rather than an observable move-then-paint sequence. [W3]

For real geometry changes involving both input and visual hosts, use the established owner-thread ordering and generation validation so neither half advertises a stale coordinate map. There is no requirement for an impossible atomic transaction across separate HWNDs, but transitions must be bounded, fail-closed, and not route clicks to a mismatched scene. Do not inject/replay user mouse clicks as a workaround.

Clear old tooltip pixels by presenting a complete correct frame or a proven dirty-region strategy. Retain premultiplied alpha conventions, static-layer caches, diagnostic handling, and current resource budgets. Cache keys must distinguish menu structural layers from transient tooltip content without keeping stale full frames at old origins.

No font scanning, media decoding, source searching, or disk IO in hover/native paint callbacks. If actual frames are expensive, measure and fix the specific repeated work; do not add a permanent animation/repaint loop.

## Required tests and evidence

1. One-line 13-logical-unit example, multi-line examples, and explicit upper **and** lower bounds. An exact 1.2 convention test expects 15.6 logical text height (15,600 milli-units), not 15,600 logical units; real-font tests may allow documented metric overhang. Padding is counted once.
2. Same logical text at 100%, 125%, 150%, and 200% DPI: logical size stays consistent, physical pixels scale once. Test long strings, CJK/emoji/combining marks, explicit line breaks, and missing-font fallback.
3. Short tooltip shrink-to-content; wrapping maximum; text/glyph bounds fit its box within a documented rounding margin; extreme text stays bounded.
4. Edge/corner/negative-monitor placement near the selected cell without changing menu origin or input geometry.
5. Hover timer identity, same-cell movement, leave before deadline, rapid cell changes, Back/page, drag, close/reopen, and late stale results.
6. Native call-plan/adapter tests for same-layout hover: **no input Hide/Show/region replacement**, no new host/session, no focus change. Actual geometry transitions still update necessary resources.
7. Render a frame with tooltip and then without; old tooltip pixels are gone. Menu pixels remain at equal desktop coordinates even if visual backing bounds differ. Include the exact oversized-height regression.
8. Real Windows hover across several cells with a stationary menu: compact boxes, no phantom ring, no transient jumps, no busy-pointer regression, no input interception.

**Done:** The units defect is corrected, production-render assertions would fail against the prior bug, hover does not disturb input ownership, and the native result is either observed clean or explicitly still unverified. Do not claim that a pointer trace alone proves visual flicker is gone.

---

# S3 — Visual slot-first Designer with bounded layout

## Objective / boundaries

Repair actual egui layout and make the existing features accessible through a compact visual workflow. Retain the independent deferred viewport, current authoring backend, properties draft/bridge, renderer preview, import/export, conflict handling, and undo. No second UI framework, dock framework, second authoring store, or separate application process.

Primary files: `src/gui/radial_editor/mod.rs`, `preview.rs`, existing work-area/Designer helpers, `src/settings/*` Designer preferences, `src/radial/authoring/*`, and existing headless UI tests.

## S3-A: Correct pane layout before adding polish

The inspected `ui.horizontal` parent feeds `allocate_ui` children without explicit vertical layouts. In pinned egui, the child inherits its parent's layout, and desired dimensions are not strict clipping limits. [S1, W1]

Use explicit top-down child layouts, real bounded pane rectangles, consistent clip/interact regions, and independent internal scrolling where needed. Built-in SidePanel/CentralPanel or a narrow splitter abstraction are both acceptable; select the simplest pinned-version solution matching manual bounds. Do not merely replace one min-width constant or make the default window enormous.

Account for all toolbar/padding/separator widths before allocating content. Do not apply `.max(320)`/multiple minimums in a way that makes the sum exceed a smaller real viewport. At too-small sizes, keep essential actions and a usable canvas accessible through an explicit compact layout/scrolling or a reasonable documented window minimum; never silently push the board offscreen.

Headings, breadcrumbs/toolbars, tree content, preview canvas, and inspector content each need intentional layout direction. Text labels should not be forced into one-character columns. Clip long names sensibly with full-name access; no hidden IDs forcing columns wider than the window.

## S3-B: Default visual workspace and preference preservation

Make the visible default one selected menu board, a compact menu selector and visited breadcrumbs, primary edit controls, and a small status area. Group optional advanced preview/resource controls behind explicit affordances rather than several permanently dense toolbar rows.

The visual workspace starts with tree/advanced inspector hidden when the user has not made an explicit choice for that workspace. Remember subsequent toggles, section states, split widths, zoom/pan, and window geometry. Do not reset those on every open/selection/repaint.

Existing preferences may represent the old layout rather than an explicit choice for the new workspace. Use the smallest versioned UI-preference transition/preset needed; preserve old values for deliberate restoration where practical and never infer permission to reset content. Expose **Reset Designer layout** that resets only Designer UI preferences to the new visual defaults. It must work while a draft is dirty without altering that draft or its undo/redo position. Do not run the previous SameCenter menu-data migration again.

When optional tree is shown, it is one readable vertical hierarchy with stable IDs; only the selected menu/relevant ancestors need be visible through deliberate navigation. Default untouched advanced sections collapsed; selection/search/warnings do not force unrelated expansion. If a user explicitly reveals/expands a path, persist it. Do not rely on all tree rows being open for accessibility tests.

Use readable menu names and `Ring 1`, `Ring 2`, or actual user-assigned ring labels. Keep exact IDs available for diagnostics, duplicate-name disambiguation, or advanced details, without changing persistent identities. Breadcrumbs follow the **visited path**, not a guessed unique parent in a reusable submenu graph.

## S3-C: Canvas bounds and transforms

Canvas zoom/pan are view state, separate from radial scale/geometry. Use one transform for drawing, hit tests, slot overlays, drag previews, and compact popup placement; invert it for pointer-to-model mapping. Include DPI, canvas origin, scale, and pan exactly once. Test the current screenshot's offscreen canvas case.

Provide explicit Fit and Reset view commands. Initial view may be fitted once when no saved view exists, but do not automatically refit or expand controls on each selection, warning, parameter edit, or runtime catalog refresh. Preserve valid saved view; if changed display topology makes it unusable, provide visible recovery rather than silently changing radial geometry.

The board shows slots, contents, submenu indicators, and selection clearly. Empty-slot outlines and center “+” are **Design-mode overlays**, not cells accidentally added to exported/runtime definitions. Runtime preview/test must retain real content without designer-only executable controls.

## S3-D: Make existing direct-manipulation paths work end-to-end

The current code already includes placement drafts, stable drag payloads, pending-drop choices, properties drafts, breadcrumbs, and a session reducer. Repair and unify these paths rather than add a second canvas model.

- Single-click selects a stable cell/ring; no real action runs.
- Right-click anchors a compact popup next to the selected cell, constrained to Designer work area. Label, icon, and content type are immediate; advanced inputs/styles remain collapsed. Property drafts survive multiple frames and validate stale selection/config revisions before Apply.
- Double-click a submenu navigates to it for editing without executing leaf actions or also starting a drag. Use a single pointer gesture classification with clear movement threshold and release ownership.
- Center-plus or empty slot starts an explicit create flow. Nothing persistent is inserted until a valid placement is accepted. Esc/cancel leaves the document unchanged.
- Drag a stable source to a highlighted destination. Cross-ring movement retains IDs. Occupied target shows the existing explicit swap/move/replace choice or refuses it; never erase silently. Dropping outside cancels safely.
- “Create submenu” creates a child and links the source in one undoable transaction; reused submenu navigation retains the visited parent frame. Do not create half-linked nodes on cancellation.
- Ring count/size edits retain guarded shrink, overflow/relocation choices, and one atomic undo entry rather than one per hover frame.
- Dynamic projected cells are views of a source. Show source management or explicit supported operations rather than silently freezing them into authored static actions.
- Explicit Test remains isolated from design clicks and uses the existing Universal Action safety/confirmation/external-target boundary. Do not add test dispatch to ordinary preview hit testing.

No generic control should display as actionable when it cannot work. Give disabled controls a concise reason. Do not globally disable editing because a harmless tooltip/font preview is loading; classify work using S1 rules. Expensive package/catalog preparation stays outside immediate GUI rendering locks. Do not repeatedly clone/search the entire application catalog on each pointer event if the current snapshot revision has not changed.

## Required production-UI tests

Existing pure tests and AccessKit names are useful but did not expose the screenshot failure. Add tests invoking the **real Designer render path** with controlled snapshots/transport and inspect actual widget/layout/clip rectangles, painter output, and events.

- Test at approximately 900×650, a smaller supported size such as 720×520, the supplied screenshot dimensions, and at least one high-DPI scale. These are logical/client constraints where appropriate, not hardcoded OS screen coordinates.
- Default visual workspace: one selected board visible, essential toolbar/menu selector usable, optional panes hidden, no spontaneous document mutation.
- Optional tree/inspector enabled together: each fits its bounds, tree entries stacked vertically, essential canvas area still present, internal scrolling works, no one-letter columns.
- User changes pane sizes/expansion, closes/reopens, edits a cell, triggers a warning, changes selection: preferences persist and unrelated panes do not expand.
- Very long user labels/IDs, many menus/rings/cells, empty collections, and missing icons remain navigable.
- Draw/hit/drag coordinates agree after zoom and pan, including near canvas edges. Popup stays near its cell within the Designer and does not move the runtime wheel.
- Actual click, right-click, double-click, create/cancel, drag occupied/empty/outside, Back, undo/redo, and save through the production UI handlers; no leaf dispatch in Design mode.
- Reset layout changes only UI preferences, even with a dirty document and asset edits pending.
- Accessibility tests explicitly open optional advanced areas when verifying their roles/names. Do not force all sections open in production to satisfy an old test. Preserve named controls and usable keyboard focus.

Use approved plain/original fixtures and fonts with clear rights. Do not ship RM4 assets or system font files just to take a screenshot. A headless rendered-layout test can assert bounds without pixel-perfect OS-font equality. Real Windows screenshots of the compact Designer remain required to judge the final layout.

**Done:** The supplied broken layout is not reproducible, ordinary slot editing is obvious and working, every previous advanced capability remains reachable, and the independent viewport/close/hotkey repairs remain intact. Commit after the combined S3 gate.

---

# S4 — Complete overlapping Cascade scenes and ancestor navigation

## Objective / current gap

SameCenter remains working and default. Explicit Cascade renders full overlapping decorated menus rather than ancestor cell fragments. The current flat `cascade_layout` cannot preserve separate menu-level backgrounds/styles by simply appending cells. [S8]

Primary owners: `radial::controller`, `geometry`, `session`, `render`, `compositor`/caches, and both embedded/native authoring preview paths. Do not duplicate the renderer or command executor and do not require an HWND for every ancestor.

## S4-A: Preserve per-frame presentation

Use the existing menu frame IDs/navigation stack and prepared-resource ownership. Retain immutable per-visible-frame presentation: menu/frame identity, layout, complete effective style/background/rim/cells, cached prepared assets/text, origin/scale, and clipping/occlusion information. This may be a narrow scene-stack wrapper around the existing builder; it does not require a new engine.

Compose visible frames oldest-to-newest, active child last. Every ancestor keeps its own skin and menu-level layers; child styles must not repaint the parent's cells with the wrong colors/backgrounds. Inactive ancestors do not emit live hover sounds/tooltips or dispatch actions. Preserve actual viewport input protections in gaps; decorative shadows/tooltips need not gain broad input ownership.

Calculate unioned visual and input bounds separately. Avoid double-applying desktop origins or DPI while combining frames. Cache stable ancestor scenes by their actual contents/geometry/resources, not by transient active-child hover generation. Cull fully occluded work only when it cannot change visible appearance or hit ownership. Keep depth/resource limits and retained snapshot rules.

Both Designer previews must use the same scene-stack composition and navigation semantics as runtime. No “nice-looking” preview-only cascade while native runtime still flattens cells.

## S4-B: Small stable overlap

Choose a modest diagonal offset based on actual menu extents, logical units, and available work area. The approved goal is a small crescent of the parent visible behind the child, not selected-cell-distance displacement. A bounded default around a few tens of logical units may be appropriate; the exact value is an implementation choice to validate visually, not a new user question or a claimed RM4 constant.

Keep direction stable for a navigation chain. Fit the stack within its session monitor. At an edge, choose a consistent inward orientation or the existing safe SameCenter fallback rather than scattering children, cursor-warping, or translating the stack on every hover/Back. Different-sized children must have explicit fit/overlap behavior, with a small visible ancestor affordance when feasible.

Deliberate whole-stack drag may translate visible frames coherently; Back uses saved actual visible frame origins and does not accumulate a clamping/offset delta. Preserve SameCenter's already-established actual-visible-center behavior and current fallback prompts. No re-running bulk submenu presentation migration and no automatic change of explicit user choices.

## S4-C: Exposed parent click means navigation only

Build composite hit testing with front-to-back frame priority. An active child's owned region wins even over a parent's cell beneath it; a gap that the child owns is not a fall-through action region. Tooltip/decorative pixels do not become targetable menu entries.

For an exposed ancestor owned region, return a typed navigation target containing that **frame identity**, not a bare cell index/label or a currently resolved Universal Action. On a valid deliberate press/release, navigate directly to that ancestor, dispose intervening active presentations, and restore its captured dynamic/page/selection state.

Consume the complete ancestor-navigation gesture. The same mouse-up, double-click tail, hold release, or stale hover dwell cannot now execute the newly exposed cell. Require fresh deliberate activation after the structural transition. Exposed ancestors remain non-executable even though they are now navigation targets; this intentionally supersedes earlier “completely inert ancestor” tests only for that back-navigation role.

Support reused menu IDs at different frame positions with identity that disambiguates instances. Clear stale tooltip/action/hover candidates for popped frames. Cancel pending presentations/teardown safely so old children cannot flash back over the restored parent.

## Required tests / gate

- Parent and child with visibly distinct simple skins/backgrounds/rims: real composite pixels contain both complete intended layers, active child wins overlap, not only copied cells.
- Three-level cascade and mixed Cascade/SameCenter navigation; Back returns exact saved locations and expected visible ancestors.
- Child hit wins over overlapping ancestor action; owned gaps do not click through; exposed ancestor click selects that frame only; counters prove zero action dispatch/history and no click-through to external app.
- Same gesture cannot execute after Back; stale input/timer/frame reply ignored. Right-click background behavior remains intentional and not accidentally a second Back/action.
- Root near every work-area edge, negative coordinates, fractional DPI, unequal child sizes, and whole-stack drag preserve safe overlap and stable anchors.
- Native/embedded preview scene parity; inactive ancestor assets/text come from the right frame and never acquire duplicate executors/hotkeys.
- Repeated child open/Back has bounded handles, frame caches, native windows, and prepared resources.
- Real Windows view resembles the overlapping-disc intent, not multiple displaced wheels. No animation added to hide bad intermediate frames.

**Done:** Cascade is visually complete and predictable, ancestor Back is safe, and SameCenter/data/paging/action behavior is preserved. Do not claim pixel-identical RM4 skins; the accepted target is functionality and visual overlap using the user's selected native skin.

---

# S5 — Integrated acceptance, regression, and release evidence

## Objective

Prove this repair resolves the actual reported failures without regressing the working launcher. Avoid another endless review/feature-expansion loop. New findings must be tied to requested behavior, data/input safety, or a demonstrated regression; unrelated polish goes to a later backlog.

## 5.1 Verification sequence

1. Review the cumulative diff and named acceptance cases before launching the final full suite. Check that each bug is covered by a production-path test, not only a new helper.
2. Run current formatting/check/diff verification and the complete required Nextest suite with captured logs and `--no-fail-fast` under the repository's normal target/profile/features.
3. Perform independent review per AGENTS; batch substantive findings. Use read-only reviewers, no duplicate build jobs. Relevant remediation gets targeted and final full verification again. Do not rerun solely because prose changed.
4. Complete real Windows acceptance on the same source/binary identity, including user chord and broken screenshot cases. An available native host can be exercised earlier at S1/S2/S3 gates to avoid postponing all important risk to the end.
5. Measure affected response/render paths and resource lifetime. Preserve actual trace/frame/screenshot/log evidence in a compact location and keep user-sensitive content out of it.
6. Report automated versus native evidence separately. Finish unblocked work even if a suitable desktop is unavailable, but do not label the whole release gate complete or promise the native failures are fixed without that evidence.

## 5.2 Commands — adapt to actual current targets

Inspect installed Nextest help and current test inventory; avoid empty filters and mistaken test-binary names. Tests may be grouped inside `domain`, library, or other existing binaries. Do not create a new top-level integration-test binary for every case.

Representative focused gate shape (verify exact names):

```text
cargo nextest run --no-fail-fast -E 'test(/radial/) | test(/launcher_invocation/) | test(/visibility/)'
```

Use narrower, meaningful S1/S2/S3/S4 filters after discovering actual module names. Include changed GUI render/authoring/main routing tests even when their names do not contain `radial`. A filter selecting zero cases is not success.

Final verification, using the repository's established variants:

```text
cargo fmt --all --check
cargo check
git diff --check
cargo nextest run --no-fail-fast
```

Run standard required lint/doctest/additional checks where the repository requires them. Do not invent a stricter lint policy or upgrade dependencies to finish this stabilization. Check that configured default test filters have not excluded affected cases.

If supported by installed Nextest, quiet reporting can use `--status-level slow`, `--final-status-level fail`, `--success-output never`, and `--failure-output final`; output verbosity must not change the selected test scope. Verify flags rather than copy untested syntax. Retain summary, failed test output, and true exit code. [W4]

Slower observation remains **600 → 900 → 1,200 seconds**. It is unrelated to the 350 ms gesture or 300 ms tooltip defaults and must never become a unit-test sleep.

## 5.3 Mandatory regression areas

- Working grid-only tap while focused, hidden/offscreen launcher show, static/follow-mouse placement, query/history/selection preservation.
- Designer/preview combinations and correct root versus child viewport commands.
- Runtime hold open/close, sticky release, no co-fire, injected self-event guard, actual configured chord, emergency and Screen Draw recovery.
- Designer X, pending requests, rejected send/disconnect, close prompts, dirty drafts, existing Apply/Save/Cancel/revert semantics, native-preview cancellation.
- Window/input cleanup, late messages, capture loss, normal cursor, resource counts.
- Actual egui bounds/layout and pointer actions, optional-pane preferences, readable labels, advanced accessibility controls.
- Tooltip unit/measurement/render/placement and stable hover input geometry.
- SameCenter continuity plus layered Cascade and navigation-only exposed ancestors.
- Action confirmations/handoffs, macro/capture paths, dynamic frozen content/pagination, no design-preview dispatch.
- Existing persistence/import/export/settings reload behavior around changed preferences and state, not a rewrite of those systems.

Refactor tests only where accepted behavior changes an obsolete assertion: e.g., “all tree sections start expanded” or “ancestor never accepts any interaction” becomes the approved explicit-navigation semantics. A valid safety test still failing means fix implementation, not remove the test.

## 5.4 Responsiveness and performance

Measure these intervals separately in a safe fixture session:

- Physical primary release -> toggle intent -> root viewport command -> actual hidden/shown state.
- Hold threshold crossing -> runtime close intent -> hidden/input released -> teardown acknowledgement.
- Clean Designer X -> close intent -> actual window gone, versus durable-save close waiting.
- Hover deadline -> measured tooltip/layout -> compositor -> layered presentation.
- Designer selection/right-click/drag edit -> next visible UI state; read-only preparation must not unnecessarily block these.
- Cascade open/Back -> prepared composed layers -> actual presented frame.

Use relative before/after evidence and report actual latencies. Do not invent “0 ms” or hard CPU-independent test limits. Structural targets are no additional tap threshold wait, no polling dependence, no needless synchronous work on event/GUI/native callbacks, no growing queues/handle counts, and no menu reposition on tooltip-only events.

Use representative small and dense existing fixtures, normal plain skin plus one lawful image-heavy skin, and repeated open/close/hover/Back cycles. Stop adding benchmark scenarios once the affected paths and resource risks are covered. No full baseline clean-build benchmark project is required for a tooltip/layout fix. Preserve baseline SHA; compare stabilization-start behavior where source-matched measurements are practical, clearly labeled.

Inspect root idle/search behavior with Designer closed and open-but-idle. No permanent repaint/polling workaround and no per-frame settings save, catalog rebuild, font scan, or image decode. Existing memory/native context caches may remain warm if bounded and intentionally owned; no need to destroy/recreate everything after each hover to claim no leak.

## 5.5 Review checklist

An independent reviewer checks specific defects and evidence, not style-driven redesign:

1. Has the hotkey failure been localized, and does the fix target the correct owner without breaking H0?
2. Can root commands still hit a child viewport, or stale restore/preview events override a hide/close?
3. Does X leave pending state forever, restart read-only preparation, lose dirty data, or abort an uncertain durable transaction?
4. Are tooltip units consistent at every producer/consumer, with real upper bounds and renderer agreement?
5. Does hover still hide/re-show input, move old pixels, reset structural generations, or display a stale tooltip?
6. Does the actual Designer render fit a compact window with optional panes both on and off? Do all important controls still work?
7. Are Cascade parent backgrounds/styles complete, hit order correct, and parent navigation gestures consumed without action dispatch?
8. Are explicit user choices, profiles, stable IDs, original menus/skins, and confirmed save semantics preserved?
9. Are asserted passes tied to the final source identity and actual native evidence, not a historical ledger?
10. Were new background loops, waits, broad settings changes, duplicate executors, or large unnecessary abstractions introduced?

Resolve substantive findings in coherent batches. Do not restart completed work to pursue speculative unrelated compatibility or cosmetic improvements.

---

# 6. Definition of done

## Runtime

- [ ] Grid-only focused tap remains responsive with Designer/preview closed.
- [ ] Supported Designer/preview-open combinations also honor tap without focus-away or mouse wiggle.
- [ ] The exact configured chord lifecycle is handled once, with all-key release as reported.
- [ ] Hold closes the runtime radial while leaving grid/Designer states intentionally unchanged.
- [ ] Screen Draw recovery/emergencies and disabled-shared-mode behavior are preserved.
- [ ] Root/Designer/native session command and wake ownership is explicit and correct.
- [ ] Clean Designer X closes promptly; dirty and durable-request cases terminate safely with clear feedback.
- [ ] Failed/disconnected/stale requests cannot leave permanent disabled state or resurrect closed windows.

## Visual experience

- [ ] Short tooltips have short measured bounds beside the actual hovered cell.
- [ ] Long labels wrap safely; source text is preserved and ordinary truncation does not spam errors.
- [ ] Hover does not flash or relocate menu pixels, input surfaces, focus, or cursor incorrectly.
- [ ] Designer default visual board is visible and usable at compact size.
- [ ] Optional panes are readable, bounded, manually controlled, and remember state.
- [ ] Direct editing is demonstrably functional and never executes ordinary Design-mode leaf clicks.
- [ ] Reset layout affects UI only; existing content/undo/skins/actions are preserved.
- [ ] Cascade shows complete overlapping parent/child layers and safe ancestor-only Back.
- [ ] SameCenter remains the default and existing explicit choices are not mass-converted.

## Engineering/evidence

- [ ] Production-path tests cover the prior layout/measurement gaps and lifecycle combinations.
- [ ] Existing legitimate regression tests are preserved or intentionally migrated with explanations.
- [ ] Focused gates and complete required Nextest/check/format/diff checks pass on the final candidate.
- [ ] Native checklist is observed on an appropriate Windows host, or its remaining evidence gap is explicitly marked unverified rather than claiming release completion.
- [ ] Relevant responsiveness/resource measurements and visual captures exist without fabricated timing claims.
- [ ] Independent review is complete and substantive findings are resolved.
- [ ] Coherent task commits exist and unrelated user files/work are preserved.
- [ ] No additional feature scope or old migration is silently reintroduced.

Do not call this done merely because the broad suite passes. The previous suite did not prevent the user's current screenshot or tooltip defect. Conversely, a screenshot that looks good does not replace lifecycle and data-safety tests. Both are required for the release claim.

---

# 7. Completion report

Return an evidence-based engineering report with these sections:

1. **Implemented behavior:** Focused hotkeys, runtime close, Designer X, compact visual Designer, tooltips/hover, and layered Cascade. Include where controls are found.
2. **Diagnosed causes:** For each reported symptom, distinguish confirmed cause, contributory issue, and any remaining hypothesis. Do not say “fixed repaint” without identifying the missing or misdirected effect.
3. **Architecture:** Actual command/wake/close owners, request cancellation policy, render/input update distinction, Designer layout, and per-frame Cascade composition. Explain reused components, not a catalog of every file.
4. **Compatibility:** Explicit choices preserved, UI-only preference changes, no repeated SameCenter conversion, stable IDs/actions and current Save/Apply/Cancel semantics retained.
5. **Tests and verification:** Actual commands, selected cases, pass/fail/skip counts/reasons, source identity, durable logs, and real exit codes. Do not invent dropped summaries.
6. **Native acceptance:** Completed checklist rows, actual chord/producer evidence if known, focus/Designer/preview combinations, monitor/DPI coverage, screenshots/clips, and unrun cases.
7. **Responsiveness/resources:** Actual measurements and any limitations; no unsupported no-regression claim.
8. **Review and commits:** Concrete findings/fixes and real commit subjects/hashes.
9. **Remaining issues:** Genuine unresolved bugs/evidence gaps only. Missing native evidence is not an implemented-feature percentage estimate.

---

# Appendix A — Suggested milestone subjects and resume rules

Suggested commit subjects, adapted to the actual verified diff:

```text
fix(radial): restore focused toggles and reliable designer close
fix(radial): correct tooltip metrics and stable hover presentation
fix(radial): make designer layout visual and directly editable
fix(radial): compose overlapping cascade frames with safe back navigation
test(radial): cover designer and native stabilization regressions
```

Do not commit extracted reference source, generated binaries, private user settings, or raw sensitive traces. Preserve intentionally tracked references; do not blanket-delete or ignore `docs/references/`. Small appropriately sanitized acceptance artifacts can be committed where the project expects them.

On resume:

```text
Read AGENTS.md and the stabilization ledger.
Reuse immutable baseline; record current candidate without moving it.
Check the active-job record before launching Cargo.
Reattach to the existing session/PID and obey the next observation time.
Inspect durable logs only at the scheduled check or completion notification.
Do not edit active verification inputs or start a duplicate build.
After completion capture true exit status and all actionable failures.
Continue the assigned milestone; commit only verified task-owned changes.
Do not re-ask already-approved questions or restart the original initiative.
```

# Appendix B — Primary API references

These references were checked when drafting this brief. They supplement the supplied source; they do not replace its architecture or prove native reproduction. Use the actual pinned library version in the checkout.

```text
[W1] egui 0.27.2 — Ui layout, allocation, child UI, clipping
https://docs.rs/egui/0.27.2/egui/struct.Ui.html

[W2] egui 0.27.2 — Context, explicit viewport commands/wakeups, deferred viewport
https://docs.rs/egui/0.27.2/egui/struct.Context.html

[W3] Microsoft — UpdateLayeredWindow position/size/content contract
https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-updatelayeredwindow

[W4] Nextest — Reporting, capture and no-capture behavior
https://nexte.st/docs/reporting/
```

The companion source notes identify the actual application/RM4 excerpts, archive hashes, observations, and limits. Do not cite unrelated earlier ledgers as current evidence or assume every code landmark still has the same line number after edits.
