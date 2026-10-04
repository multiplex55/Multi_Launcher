# Multi Launcher — Approved Radial Stabilization Requirements

**Status:** All choices below were approved; the user has said **Proceed**. No further routine product approval is needed.  
**Inspected application snapshot:** `multi_launcher(20260918-003954).zip`.  
**References:** The supplied RM4 archive/screenshots and corresponding repository-local material under `docs/references/`.  
**Scope:** Stabilize the current implementation and make its existing capabilities usable. Do not restart the original radial initiative or add unrelated features.

## 1. User observations — evidence, not guesses

- The configured launcher chord is `Shift+Alt+Win+End`, bound to a single key action. The user releases **all keys** between invocations. The precise firmware/software source of that mapping was not supplied; do not assume AutoHotkey, Talon, or a particular keyboard firmware is producing it.
- The user answered **No** to “Does tap-to-hide still fail with both Designer and native desktop preview completely closed?” Thus the ordinary baseline works with both closed. Which of Designer-only, preview-only, or their combination causes the regression remains to be isolated.
- The observed problematic closing operations are **Radial Designer title-bar X** and **holding the launcher chord to close the runtime radial**. Unchecked operations were not established as broken.
- The main launcher remains responsive. The report is delayed/ineffective Designer closing, not a confirmed whole-application crash or deadlock.
- The user does not think a preview, save/import, or unsaved edit was active, but the broken UI makes this uncertain. Test genuinely idle closing and pending-request states separately.
- Supplied screenshots show the Designer tree spreading horizontally with character-wrapped labels and the canvas offscreen; the RM4 references show a visual slot board and overlapping complete submenu discs.
- Reported tooltips are monitor-height, appear near the top, and hover produces transient phantom drawing. Source supports a tooltip units defect and suspicious native presentation sequencing; a real Windows trace is still required to explain the complete flicker and hotkey/close symptoms.

## 2. Retained independent invocation contract

| Input | Normal grid/list launcher | Runtime radial |
|---|---|---|
| Short tap released before configured threshold | Toggle immediately on release, including when focused | Unchanged |
| Hold through threshold | Unchanged | Toggle |
| Release the original opening hold | Unchanged | Remain open in sticky mode |
| Esc owned by active radial | No duplicate cancellation below it | Close according to existing radial cancellation scope |

Keep the user's current chord and configured threshold. Existing defaults remain approximately 350 ms; do not forcibly reset settings or hardcode the example chord. A tap does not wait out the remaining threshold. No mouse wiggle, focus-away click, or visible-grid repaint should be necessary.

Preserve the working both-closed case and disabled-shared-mode legacy behavior. Preserve Screen Draw recovery, quit, emergency, and exclusive-tool priority. A Designer is not itself an exclusive desktop-capture tool.

## 3. RM4-style visual Designer

Retain the independent Designer window and its current authoring backend. Do not create a second editor process or rewrite the document/undo/persistence system.

The visual workspace defaults to:

- One selected menu's radial board as the main content.
- Compact menu selector, breadcrumbs/Back, and compact toolbar.
- Optional tree and advanced inspector hidden initially in this visual workspace; explicit toggles and remembered user choices thereafter.
- Compact right-click properties for label, icon, and Action/Submenu/Dynamic content, with advanced settings collapsed but available.
- Readable user labels. Internal IDs belong in advanced details or unambiguous diagnostic/context text, not as long primary tree captions.

Fix actual pane layout, sizing, clipping, and coordinate transforms. Do not hide defects by forcing a larger window. A typical compact window must expose its essential controls and usable canvas. Pane sizes/visibility, manual section expansion, zoom, and pan remain user-controlled. Long content scrolls internally; no selection, hover, diagnostic, or refresh forces expansion.

Provide **Reset Designer layout** for UI preferences only. It must not change `radial.json`, skins, actions, ring contents, bindings, or unsaved authoring state. Keep the current default window size/geometry unless a narrow correctness adjustment is necessary; 900 × 650 logical units is a useful acceptance viewport, not a forced resize of existing user windows.

## 4. Direct editing

| Gesture | Behavior |
|---|---|
| Single-click cell | Select |
| Right-click | Compact properties |
| Double-click submenu | Edit child |
| Breadcrumb/Back | Navigate to the actual visited parent |
| Center “+” or empty slot | Create with explicit placement |
| Drag/drop | Show destination; no silent overwrite |

Center creation and actual runtime center navigation remain distinct. Design mode must never execute real actions. Explicit Test continues through existing Universal Actions and safety/confirmation rules. Dynamic result cells retain their source; no accidental static conversion. Preserve undo/redo, stable IDs, duplication/move/reference checks, and existing Save/Apply/Cancel semantics.

## 5. SameCenter and overlapping Cascade

SameCenter remains the agreed default. **Do not mass-convert existing submenu choices again.** Existing explicit Cascade settings remain valid.

In Cascade:

- Render each visible ancestor's complete menu background, rim, skin, cells, and relevant static decoration back-to-front.
- Use a small, consistent diagonal overlap rather than dispersing wheels across the desktop.
- Make the active child clearly frontmost.
- Keep the stack together on the session's monitor; Back must not drift.
- Clicking an exposed parent navigates directly back to that parent **only**. That press/release must not execute the exposed cell, fall through to another app, or trigger a new submenu after navigation.
- No new opening/closing animation while stabilizing static rendering.

The supplied blue-disc screenshot defines visual intent. A specific fixed offset was not supplied by the user and is not asserted to be the original RM4 algorithm. Choose a modest, tested native offset using actual geometry/work area, documenting the default without creating a new broad configuration subsystem.

## 6. Tooltips and clean hover

Keep configured tooltip behavior and delay; default approximately 300 ms. Tooltip delay is unrelated to the launcher hold threshold.

Tooltips show the complete original label, and configured description where applicable. Their width follows actual text subject to a sensible wrapping maximum. **Height is measured text plus padding**, not monitor height or a fixed maximum. Position beside the hovered cell and flip/clamp at edges without moving the menu itself.

Only the current hovered item's tooltip is visible. Leaving, navigation, paging, closing, or superseding a session invalidates stale tooltip work. No phantom menu copies, repeated input-window hide/show, focus stealing, tooltip interception, or arbitrary delay workaround. Expected label truncation stays quiet; actual asset/config/save/runtime failures remain visible.

## 7. Closing and responsiveness

Clean Designer X closes only the Designer promptly. Dirty closing asks once through the current save/discard/keep-editing behavior. Preserve the existing, confirmed semantics of Apply, Save, Cancel, and any Cancel-after-Apply undo transaction; this is not authorization to redefine them.

Cancel nonessential preview/catalog work. Do not silently ignore X because a request is pending. If a durable save/import must finish safely, display its status, remember the close request, and close after safe completion. Delivery failure or a failed operation must not leave controls permanently disabled. Never abort a transaction halfway through or claim its outcome without evidence.

Treat runtime radial hold-to-close as a separate input/session lifecycle. Closing either surface must not strand input capture, suppression guards, windows, pending generations, or key-release ownership. Late replies must not reopen a closed editor/menu.

## 8. Engineering boundaries

Reuse Universal Actions, current native renderer/host, stable IDs, shared layout/hit-testing, authoring transactions, asset management, persistence, and undo. Existing architecture may receive narrow ownership corrections where the reported defects require them. No additional feature family, full renderer replacement, blanket framework upgrade, or generic subsystem redesign.

Inspect current code and preserve later legitimate work. Keep the existing immutable feature-baseline record; record a separate stabilization-start HEAD/diff identity. Old green test counts and implementation percentages are not acceptance evidence for these reported bugs.

## 9. Slow-machine verification policy

Implement coherent batches with tests added/refactored alongside them. Use focused gates collecting failures together (`--no-fail-fast` where supported) and one full required Nextest run near completion. Fix invalidated verification after relevant code changes.

Keep one Cargo/build/Nextest job at a time, including subagents. Retain durable logs, true exit code, source snapshot, session/PID, and next observation time. Reattach to the existing job; never duplicate a quiet build.

| Still-running job observation | Wait interval |
|---|---:|
| First check after launch | 10 minutes / 600 seconds |
| Second check | 15 minutes / 900 seconds later |
| Subsequent checks | 20 minutes / 1,200 seconds apart |

Prefer completion notifications or blocking waits that return early on completion. An actual completion/failure event may be handled immediately. Tool wait limits do not justify rapid polling. Use a supported persistent job/wait mechanism and record real limitations; do not invent one.

These are observation intervals, not kill deadlines or delays to put in application/unit-test code. Do not alter legitimate test timeouts, run `cargo clean`, or serialize tests with `--no-capture` just to obtain output. No rebuild is needed solely because a transient terminal lost a summary: durable logs must retain it.

## 10. Acceptance

Require tests of the production layout/render/event/close paths, not only serialization, helper enums, or AccessKit label presence. Include real Windows reproduction of the user's Designer/preview combinations, actual chord, tooltip placement, phantom drawing, closing, direct editing, and Cascade overlap/Back.

A native test that was not run remains unverified. A mocked event sequence or screenshot of a synthetic helper is not proof of a fixed deployed window. Continue all unblocked work, record genuine unavailable host evidence, and never mark the native release gate passed without observing it.
