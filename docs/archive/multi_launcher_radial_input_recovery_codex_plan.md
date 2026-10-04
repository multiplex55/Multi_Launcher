# Multi Launcher — Radial Input Recovery and Basic Ring Authoring
## Approved, code-first Codex goal

**Status:** The user approved the options and said **Proceed**. Do not repeat the questionnaire.  
**Source inspected:** `multi_launcher(20260922-180803).zip`. Implement against the current checkout while preserving legitimate newer changes.  
**References:** Current project `docs/references/` and supplied RM4 material, read as reference data, never executed.  
**Primary deliverable:** Working focused launcher toggling, an interactive Designer, and a simple complete menu/ring/action/skin editing workflow. A diagnostic-only build or another planning document is not completion.  
**Development policy:** Substantial implementation first; `cargo check` at meaningful code checkpoints; very light targeted test execution; one complete required Nextest run near the end.  
**Job observation:** 10 minutes initially, then 15 minutes, then every 20 minutes, unless actual completion/failure is delivered sooner.

This handoff is an implementation specification. It is not a claim that source was modified, a build was executed, or Windows behavior was reproduced during its preparation. Companion source notes identify inspected evidence and its limits.

---

# 1. Concrete end goal

> On the user's actual Windows profile, a short tap of the configured launcher chord reliably shows/hides **Multi Lnchr**, including while its own main window has focus. Both Edit Radial Menus and Edit Radial Skins accept mouse and keyboard input independently of grid visibility. The ordinary visual workspace directly supports New Menu → Add Ring → Slots → Assign Action → Change Skin → Preview → Save → Close → Reopen, without manual JSON editing, silent failures, lost changes, or mandatory advanced-pane discovery.

Keep the existing native renderer, Universal Actions, authoring backend, stable identities, persistence, and undo. Repair their actual boundaries; do not restart the original radial or stabilization initiatives.

## 1.1 Latest observations override earlier diagnostic assumptions

Record these verbatim in meaning:

- The main **Multi Lnchr grid/list** fails to toggle on a short tap when that main window is active/focused. It should toggle on every eligible short tap regardless of normal application focus. The latest answer identifies **main-window focus** as the failure condition; do not continue asserting that an open Designer is required for failure. Exercise both closed/open Designer cases without fabricating a result for either.
- Menus and Skins show ordinary-looking controls, but no client interaction registers, whether the grid is hidden or visible.
- The Designer **can be moved/resized using its title bar/window border**.
- **Tab does not move a focus outline among controls**.
- Native desktop preview's actual state is uncertain. Do not assume it is stopped or active.
- The executable runs from a test directory. Exact executable and data-directory paths were not supplied. Discover actual process/startup paths; do not hardcode a developer's `G:\...` location or treat an answer of “yes” as a path.
- The unchanged chord is **Shift+Alt+Win+End**, mapped to one key action; all constituent keys are released between invocations. The producer is unknown: do not invent firmware, AHK, Talon, or raw-keyboard provenance.
- No acceptance trace was supplied in these answers. The environment variable and bounded trace already exist; use them rather than proposing a new telemetry subsystem.

**Interpretation limit:** Working title-bar/border interaction proves a visible top-level window responds to some native operations. It does not prove the application receives/uses client mouse or keyboard events. No Tab outline is also not, by itself, proof that Windows disabled the HWND. Separate native routing, framework input, UI readiness, interaction layers, and model updates before choosing a fix.

## 1.2 Retained behavior

| Input/action | Required outcome |
|---|---|
| Short tap before current threshold | Toggle grid/list only, immediately on release; no wait for the rest of the threshold |
| Hold through current threshold | Toggle runtime radial only |
| Release the opening hold | Sticky radial remains open |
| Root focused/unfocused/hidden | Does not by itself disable the shared trigger |
| Menus/Skins open | Neither blocks ordinary root toggling nor requires showing the grid to accept input |
| Clean Designer X | Close only the Designer promptly; release its owned preview work |
| Dirty/durable-operation close | Preserve current safe save/discard/Cancel semantics and show the actual pending operation |

Keep configured threshold and chord. The existing default remains about 350 ms; do not reset the user's settings. Preserve explicit key-capture mode, secure/unsupported OS contexts, emergency/quit controls, and Screen Draw/exclusive-tool priority. “Always” here must not remove higher-priority safety or synthesize forbidden OS input. Keep SameCenter/Cascade choices and current tooltip/skin behavior except for a proven interaction regression.

## 1.3 Basic authoring contract

The default Design workspace exposes, without opening the tree or Inspector:

```text
New Menu    Add Ring    Ring: [selected ring]    Slots: [number]
```

The visual board remains primary. Optional tree/advanced Inspector stay user-controlled. Select a cell by click; compact properties by right-click; edit a child by double-click; breadcrumb/Back returns along the visited path. Center-plus/empty slot creates with explicit placement. Drag previews a destination and never silently overwrites data.

The existing searchable Universal Action catalog is the assignment mechanism. Items beyond a fixed initial first-50 list must be reachable through search and/or supported result paging. Open in Inspector must actually open the selected cell and preserve or explicitly resolve uncommitted popup edits.

New rings/changed slot counts receive a proposed valid geometry using the **current layout and effective style**. Preview the candidate before Apply. Keep manual radius/cell-size/gap/rotation controls in Advanced. Existing other rings are not silently moved or rescaled. If fitting requires other changes, display the exact proposal and require acceptance; if impossible within supported limits, explain alternatives. Ring shrink stays reference-safe and undoable.

Design mode never runs real leaf actions. Dynamic projected results retain their source. Existing IDs, saved data, imports, confirmations, and Save/Apply/Cancel semantics are preserved.

---

# 2. Workflow: implement first, keep testing light

Read applicable project `AGENTS.md`, source, and current agent configuration. Preserve the existing immutable branch baseline, and record a separate `INPUT_RECOVERY_START_HEAD` plus initial dirty/untracked state in a short linked section of the current radial ledger or `docs/plans/radial-input-recovery.md`.

Use **one bounded implementation milestone with the work packages in section 4**, followed by a final acceptance/sign-off gate. Work packages are not separate mandatory Nextest/review/commit gates. This avoids the old cycle of a full focused suite after each small correction while preserving the repository rule that a milestone is not complete until its required verification passes.

## 2.1 Compilation and test execution policy

1. Inspect enough to identify owners and input flow, then implement coherent changes. Do not produce another exhaustive source inventory before writing code, and do not pre-run the original S0–S5 battery.
2. Write/refactor a small set of high-value regressions alongside the implementation, but defer execution until integration. No blanket test-first cycle or new integration binary for every behavior.
3. Use the narrow application check at meaningful checkpoints, normally after input/lifecycle changes and after the combined authoring/geometry changes:

   ```text
   cargo check --lib --bin multi_launcher
   ```

   Adapt target names if the current manifest differs. Do not run it after every edit or automatically add `--all-targets`, `--all-features`, a new profile, or an unnecessary target triple. Preserve existing incremental caches and environment.
4. `cargo check` is more than a syntax parser, but **it does not produce the new linked executable or execute tests**, and some code-generation diagnostics can only appear during a real build. Do not report a successful check as a successful runtime build or a passing Nextest run. [W1]
5. Ordinary checks do not cover all `#[cfg(test)]` code. Near integration, optionally use `cargo check --tests` if it will prevent another known test-compilation cycle. It is not mandatory repeated work before Nextest, which will compile its actual test targets. Do not check unrelated benches/examples by reflex.
6. Run one small combined targeted test batch near the end only when useful to isolate the changed paths; the final full suite may cover those same cases directly if that avoids an extra expensive invocation. Prefer `--no-fail-fast` so one run exposes all independent failures.
7. The user retained **one full required `cargo nextest run --no-fail-fast` after corrections**. Keep this final gate. “Very light testing” reduces repetition and unnecessary scope during development; it does not authorize deleting valid tests or falsely claiming full regression success.
8. For an actual interactive candidate, reuse a known source-matched executable if it exists; otherwise build the application target with `cargo build --bin multi_launcher` when ready for the walkthrough. A previous binary beside fresh `cargo check` metadata is not a current candidate. One early native smoke build after input repair is justified if it prevents completing UI improvements atop a still-dead client area; it is not a full regression run.
9. Retain actual source/executable identities and evidence. Do not combine a check from one snapshot, a native run from another, and an older test count into a single “passed” claim.
10. One read-only review after coherent implementation is sufficient before closing the gate unless a concrete issue requires remediation. Batch findings; no endless nested “final closure review” cycles. Follow applicable AGENTS final-review sequencing without expanding the requested scope. Relevant post-verification source changes require renewed final evidence; documentation-only updates do not.
11. Do not mark work packages tested or complete merely because `cargo check` passed. They may be `implemented / check-passed / acceptance-pending`. Preserve coherent commits once the integrated changes are actually verified; do not create compile-broken intermediate commits to mimic old milestone boundaries.

## 2.2 Slow-job observation

| Check of the same still-running job | Wait interval |
|---|---:|
| First | 10 minutes / 600 seconds after launch |
| Second | 15 minutes / 900 seconds after the first check |
| Later | 20 minutes / 1,200 seconds apart |

Prefer completion notifications or supported blocking waits returning early. A completion/failure event can be handled immediately; short finished commands require no artificial delay. These are not process timeouts, application delays, or unit-test sleeps.

One writer and **one Cargo/check/build/Nextest process tree** may use the shared checkout/target directory at a time. Reviewers do not launch builds. Record command, cwd, source/diff identity including new files, target/profile, session/PID and start identity, durable log, true exit code, and next observation time. Reattach to that job; do not relaunch because output is quiet. Keep build inputs stable during its run.

Respect actual tool wait limits using an available persistent session/job mechanism. Do not invent tools/callbacks, kill a healthy job when a wrapper yields, or replace the long intervals with a rapid series of status/log queries. No `cargo clean`, profile/toolchain changes, copied source trees rebuilding dependencies, or shortened test timeouts merely because compilation is slow. Retain logs so missing terminal output does not force another build.

---

# 3. Current source facts and limits

The accompanying source notes and earlier reviewed excerpts identify these concrete current seams. Revalidate paths in the checkout before edits; do not overwrite newer fixes.

| Area | Present implementation / consequence |
|---|---|
| `src/visibility.rs` | `RootViewportCtx` already sends ROOT-targeted commands and repaint requests. Adding this same adapter again is not the goal. |
| `src/gui/radial_editor/mod.rs::show_deferred` | Live focus flag is consumed using `std::mem::take`; `ensure_open` is idempotent. Earlier focus-replay repairs are present. |
| `src/radial/authoring.rs` | Draft-generation changes already retire disposable work and obsolete preview requests; pending stop/generation handling was repaired. Preserve it. |
| `src/gui/radial_editor/mod.rs::viewport_ui` | Main document controls are gated by initial snapshot and conflict state. Menus/Skins are the same Designer/session. A visible but disabled UI is possible; do not assume all failed clicks are absent native events. |
| `src/radial/acceptance_trace.rs` | Existing `MULTI_LAUNCHER_RADIAL_ACCEPTANCE_TRACE` has a 256-event budget, explicit exhaustion, root-command/actual-window, authoring, Designer pointer/widget, and preview-window evidence. Use it. |
| `show_deferred` catalog preparation | The action-catalog snapshot is constructed before the closed-Designer early return. Move unnecessary work behind the actual need and reuse revisions if appropriate; this is not proof of the dead input cause. |
| Basic controls | New Menu/Add Ring live in the optional tree; Slots/Cells is in the Inspector. Expose shared commands on the basic toolbar, not another authoring engine. |
| Compact properties | Open in Inspector currently follows the cancel path; the action rows use an empty filter then `.take(50)`. Correct those actual behaviors. |
| Ring operations | `add_ring` seeds eight spacers at `92 + ordinal*64`; resize callers discard some failures. Current custom dimensions/styles require a real proposal and visible errors. |
| Geometry and limits | Circular capacity/center/inter-ring checks already exist. Current model limits: 16 rings/menu, 128 authored cells/ring, 8,192 total authored cells. Preserve actual current limits and dynamic paging semantics. |
| Existing ledger | The previous corrective pass is explicitly diagnostic and did not establish the user's actual profile; its developer F2 and old executable hash are not evidence for this run. |

**No new proven native root cause is asserted by this brief.** Two symptom paths remain open: focused root toggling and dead Designer client input. Implement against evidence and current owners, not a guessed common cause.

---

# 4. Ordered implementation work packages

## Package A — Restore real input, focused toggles, readiness, and close

### Objective

Make the existing windows usable before improving authoring controls. Correct the first broken production boundary; do not stop at adding more tracing or changing another unrelated focus flag.

### A1. Identify the actual running profile once

Use the actual process/launch context to record executable path/hash, working directory, resolved `AppDataRoot`, loaded settings path, log destination, configured chord/threshold, and relevant preview state. Do not print private notes, clipboard text, arbitrary keystrokes, or full configuration into shared logs.

The user's directory is a test directory, not a known developer path. Startup uses the settings/data-root and single-instance boundary. Ensure the intended candidate is the process being inspected; a new launch that exits because another instance owns the same directory does not replace it. Do not kill all launcher instances, switch to factory settings, or mutate/reset the user's data to manufacture a clean reproduction. Close normally or use an authorized isolated **copy** of the relevant profile, retaining the original and identifying any differences.

No additional source rebuild is required just to enable the trace already in the current candidate. Use `MULTI_LAUNCHER_RADIAL_ACCEPTANCE_TRACE=1` and the existing durable logging mechanism. Derive the real log path from the loaded settings; do not hardcode the ledger's path. Trace absence is meaningful only if activation, capture, and budget are known. If the 256-event budget is exhausted before the gesture, use a fresh short trace session or narrowly fix event prioritization—not unbounded logging or many new diagnostics modules.

### A2. Isolate the Designer's dead client area

Begin with one ordinary click, one Tab press, one title-bar drag, and X. Repeat with grid hidden and visible. Preview state must be read from actual session/window ownership rather than inferred from an unresponsive button. Record Designer-only/preview involvement as observed, without retaining the superseded claim that grid focus failure requires Designer open.

Follow this decision chain using current tracing plus only a missing narrow boundary probe:

```text
Native window / input owner
    -> framework callback and per-viewport input
    -> correct UI layer / hit area / enabled state
    -> widget response
    -> authoring or UI-only mutation
    -> new frame/presentation
```

Investigate the first absent transition:

1. **No native client delivery:** Inspect the actual Designer HWND's enabled/activation/hit-test behavior, owner/modal relationships, capture ownership, and overlapping preview/input-shield windows. Inspect who owns any shield and when it should be released. Do not force `EnableWindow(true)` globally, make every window click-through, or destroy a live capture to conceal ownership. A title bar working does not establish the whole client path. Windows' enabled state controls input eligibility, but that is a hypothesis to check, not a conclusion. [W3]
2. **Native events but no usable egui input:** Verify the current callback's viewport ID and current Context, integration event forwarding, event consumption, and lock/reentrancy/early-return behavior. Do not feed old/synthetic global input to pretend the window works. Access current child input in the active callback; retained Context clones/callback lifetimes must respect the pinned integration. Keep locks short and avoid recursive Context access. [W2]
3. **egui sees click/Tab but controls do not:** Inspect effective `Ui::is_enabled`, the authoritative-snapshot/conflict block, top interaction layer, full-canvas overlays, clip/transform, stable widget identity, keyboard focus eligibility, and any root or menu handler consuming the child's events. A painted cell overlay must not intercept every unrelated control. No Tab outline may reflect disabled controls or stolen/consumed keyboard input; demonstrate which.
4. **Widget responds but state does not change:** Follow real result/error and request IDs. Preserve proper authoring revision/generation checks. Fix incorrect binding/selection and surface failures; do not ignore `Result` or silently replace an invalid draft.
5. **State changes but display is stale:** Wake the correct viewport after publication and retain the response until consumed. Fix event delivery rather than add a permanent render timer or force root visibility.

The same ordinary window/session backs Menus and Skins. Fix their shared path, then verify both modes. Do not create a second Skins window to avoid it.

### A3. Readiness must be safe and recoverable

Retain read-only/blocked behavior before a valid authoritative snapshot or while a genuine conflict is unresolved. Never edit the starter placeholder as though it were the user's loaded data.

Make loading/conflict/request-failure state visible in an independently usable status/recovery region. Retry/Close/Copy diagnostic status may be available outside the document-edit gate where safe. Recovery controls must not themselves be disabled by the condition they resolve. Do not mistake a returned error for an initial request still running forever.

Distinguish initial snapshot, session-stable catalog, disposable preview, and durable commit. Existing stale-disposable corrections must remain. On delivery failure, retire only the exact unsent/failed request; on a stale reply, reject its payload without indefinitely occupying an obsolete slot or clearing a newer request. A durable accepted write needs safe completion/reconciliation, not cancellation by timeout. Do not call every pending request a “save.”

While closing, latch the requested close and do not restart catalogs/previews on the next repaint. Preserve the existing X versus Save/Apply/Cancel semantics. A clean X should not wait for optional preview resources unnecessarily; a durable operation should show what is pending and close after its terminal acknowledgement. Do not join a worker while holding a GUI/state lock it requires. Late preview readiness must not recreate a closed surface.

### A4. Restore the focused root toggle

Trace one short mapped chord cycle while the actual root is focused and compare with it unfocused. Test this with Designer closed and open; root focus is the reported axis, not assumed preview dependency.

At each stage establish: physical primary press/release and accepted provenance, modifier state, tap/hold decision, emitted root toggle, desired visibility edge, explicit ROOT viewport command, actual root HWND/bounds, and later activation/restore completion.

Correct whichever boundary fails:

- a hook/focus-dependent capture path consumes the accepted chord;
- a shared-event consumer drains the wrong target's event;
- an effect is sent to the wrong actual native window despite the ROOT enum;
- a hide is overwritten by a stale logical restore or previously queued native foreground job;
- the native effect is never delivered while root is focused.

These are candidates, not asserted diagnoses. `RootViewportCtx`, idempotent ensure-open, and one-shot focus consumption already exist; preserve them. If a queued activation is proven to restore after a newer hide, make that specific operation respect current visibility/intent generation rather than disabling all legitimate editor/launcher restoration.

Only the admitted chord's main-key release before the threshold produces the tap outcome; do not wait another threshold, require unfocus first, add a second focused-window hotkey, or perform speculative toggle-on-down plus undo-on-hold. Repeat/down/up drainage and accepted external mapping behavior remain intact. Do not hardcode `Shift+Alt+Win+End`; use it as the actual-profile test case. Preserve currently supported emergency/exclusive priority rather than routing through the radial delay.

Designer visibility must not be tied to root visibility. Native preview may have its own resource-exclusion policy, but it must not impersonate runtime state or permanently intercept Designer client input. Intentional preview stop/teardown must complete on its owner thread with stale-generation protection.

### A5. Exit of this package

Implement the demonstrated ownership/readiness/lifecycle correction, preserve working earlier fixes, and run a meaningful `cargo check --lib --bin multi_launcher` checkpoint. Add only the directly relevant regression source now. Do not launch the full suite.

If an appropriate Windows host is available, run a short click/Tab/focused-toggle/X smoke check using a source-matched executable before trusting the repaired input foundation. This is a short behavior check, not a new broad acceptance matrix. If native observation is unavailable, continue Packages B/C where unblocked but keep the input-recovery outcome explicitly **native-unverified**. Do not announce success because code compiles, and do not make the whole assignment another diagnostics-only stop.

---

## Package B — Complete the ordinary visual authoring workflow

### Objective / owners

Make basic menu/ring/action/skin editing discoverable and dependable using existing `RadialAuthoringSession`, `radial::authoring::menu`, compact properties, and `UniversalActionAuthoringCatalog`. Likely edits are in `src/gui/radial_editor/mod.rs`, `preview.rs`, `src/radial/authoring/menu.rs`, and the current catalog helper. Do not add a parallel command graph or mutable launcher query as a search workaround.

### B1. Always-accessible basics

Add a bounded toolbar in Design/Menu mode:

```text
New Menu | Add Ring | Ring [selector] | Slots [draft number] | Apply proposal
```

An Apply proposal control may appear only when a change is pending. Keep controls readable at about 900×650 and smaller supported windows by deliberate wrapping/scrolling—not character-per-line text or forcing a larger root launcher. Tree/Inspector remain optional; toolbar state and selection must stay synchronized with them through one command path.

- New Menu creates/selects a valid menu with current defaults using the existing factory. No IDs from user-facing labels or menu indexes.
- Add Ring selects the new ring after its proposal is accepted. Do not require manual tree navigation afterward.
- Ring selector lists readable labels/ordinal and authored slot count. Preserve stable IDs internally and dynamic source information.
- Slots is the authored ring slot count (`cells.len()`), not the number of currently projected dynamic entries. Clearly distinguish dynamic source result caps/page capacity. Do not turn a dynamic list into static cells merely because the toolbar displays a number.
- Edit the number in a local typed draft. Typing the intermediate `5` while entering `50` must not delete/create cells or rewrite the document. Commit on explicit Apply/Enter once the complete proposal is ready; cancel leaves state unchanged.
- Add/remove/resize operations use one existing authoring transaction and undo entry per accepted operation, not per keystroke/frame.

### B2. Remove silent failure paths

Route current New Menu/Add Ring/Add Spacer/resize/Apply failures to the existing bounded error/status UI. Preserve enough domain error detail to distinguish missing target, invalid geometry, content at risk, stale proposal, and write/conflict failure. Existing `.map_err(|_| MissingEntity)` or `let _ = ...` patterns must not erase the relevant validation explanation for these edited paths. Do not mechanically rewrite every ignored result in unrelated code.

No visible button may pretend success without changing state. A pending/disabled control should have a concise reason, but must not expand the whole window or repeat a toast on every repaint. An expected configuration validation message is not an application crash.

### B3. Searchable action assignment

Replace the compact popup's empty-filter `.take(50)` behavior with a real search field feeding the existing catalog before rendering limits are applied. Reuse a prepared, revisioned snapshot; do not rebuild/scan the entire application catalog or execute plugin queries during every pointer event. Do not change the root launcher query, history, focus, or selection to perform picker search.

Keep result rendering bounded through the existing scroll/virtualization/paging mechanism. Show a result count/limit where useful; every valid matching action must be reachable by search or paging, including one beyond the old first 50. Preserve stable target/action identity, supported action availability, persistence rules, and contextual target semantics. Choosing a row assigns data only; Test is explicit.

Do not silently clear a prior action if `assignment()` fails. Explain why the candidate cannot be assigned and preserve the existing draft. Enforce the same availability/handoff/confirmation rules at execution later. No arbitrary shell-string fallback.

### B4. Real popup-to-Inspector handoff

Replace `Open in Inspector -> cancel = true` with an explicit handoff intent. It reveals the Inspector, selects the same cell/ring/menu, and focuses the appropriate property area without changing the main grid's focus/visibility.

If popup edits are clean, open immediately. For dirty popup edits, either transfer the same draft object safely or present a small Apply-and-open / Discard-and-open / Keep-editing decision using current authoring semantics. Invalid edits must remain visible for correction. Never silently drop edits or bypass the existing validation/undo path. Ordinary Cancel still cancels; it must not be overloaded as “open Inspector.”

Keep all skin/image/font/behavior options available in the existing Inspector/Skins view. This pass exposes basics, not a reduction in customization. Changing a skin field should visibly update the safe preview after the established preparation reply, without changing runtime configuration before the current Apply/Save boundary.

### B5. Direct-editing integration

Verify the existing single-click, right-click, double-click, center-plus, empty-slot, drag/drop, breadcrumb/Back, and undo routes all use the same selected entity and pending draft. Repair any specific miswired route encountered in this complete workflow; do not rewrite working gesture geometry.

Render and hit testing use the same canvas transform at zoom/pan/DPI. Editor-only slot outlines and center-plus are not exported actions. Generated dynamic cells remain source views. Existing Create-and-link Submenu should remain one validated undoable graph operation with cycle/ref protection. No action may execute during design clicks or selecting an action-picker row.

Keep pane visibility/size, manual section state, zoom/pan, and window geometry as UI preferences. No auto-expansion/fit on every edit, and no `radial.json` mutation from selecting another ring. Reset Designer layout affects UI only, including when a dirty document is open.

---

## Package C — Safe automatic ring/slot geometry proposals

### Objective

Users can ask for a ring or slot count and see a feasible proposed layout instead of choosing radii blindly. The current runtime geometry and validator remain the authority. Adding a proposal mechanism must not redefine ring rendering or break existing saved layouts.

### C1. Pure proposal boundary

Add a focused, pure helper near `radial::authoring::menu`/the existing geometry policy. A representative, **proposed** API shape is:

```text
propose_ring_edit(document, selected_menu, ring_or_new_ring,
                  requested_slots, current_style, preview_context)
    -> validated candidate + explicit changes + warnings/resolution needs
```

Use existing domain types, not necessarily those names. The proposal carries menu/ring identities and base document revision/draft generation so Apply cannot reuse it after unrelated edits. It does not save, register hotkeys, modify live window position, allocate a native host, or publish the document. No new standalone layout framework.

Preview the candidate with the same effective-style/layout/render code used for actual editing. Store requested slot count and proposed dimensions separately from the authoritative draft until acceptance. Coalesce/recompute only when proposal inputs change, not on idle frames or every mouse movement.

### C2. Geometry rules

Inspect actual ordered rings by their current radii/extents, not only by array position. The existing starter formula `92 + ordinal*64` is not sufficient for customized neighboring rings.

For equal circular cells at unscaled radius `r`, count `n >= 2`, cell radius `c`, and requested gap `g`, the geometric spacing constraint is:

```text
2*r*sin(pi/n) >= 2*c + g
r >= (2*c + g) / (2*sin(pi/n))
```

This is a geometric starting bound, **not a duplicate replacement validator**. Resolve actual style scales/overrides first and apply the existing center/inter-ring/shape validity checks. Use the production geometry's current interpretation of item size/radius scale/menu scale; inspect it rather than assuming a ring/cell override changes dimensions if the renderer does not use it. Do not invent anisotropic cell shapes or general packing algorithms.

Special cases: zero slots if currently supported, one slot, high counts, invalid/non-finite input, style-driven item sizes, rotation, wedge versus circular layout, and minimum interactive size. Wedges require the existing wedge policy, not the circular sine formula.

For a new outer ring, find a valid radius outside the actual outermost occupied extent while respecting center and neighboring separation. Preserve all existing ring geometries by default. For resizing an inner ring, detect whether enlarging its radius or changing target ring size would collide with neighbors. Propose only validated alternatives; do not shuffle other rings silently.

If safe results require moving other rings, show exactly which dimensions will change and why, in a separate explicit whole-change preview. The user must approve it. Prefer the smaller scoped option or offer a valid new outer ring rather than automatically relaying out every menu. Keep manual controls as a legitimate alternative.

If the configuration fits mathematically but not the current monitor/work area, explain that separately. Use the actual preview/runtime fit and usable-scale policy. Do not imply that all 128 model slots can always be displayed comfortably on one monitor. No silent cell dropping, unbounded rings, or undetectable shrinking below the current interaction minimum. A 50-slot proposal must either preview a valid achievable arrangement or clearly explain why the chosen size/work area cannot support it.

### C3. Counts, shrink, and atomic apply

Keep current limits (currently 16 rings/menu, 128 authored cells/ring, 8,192 total authored cells) and the actual current validation ranges. Do not increase limits just to make a failed proposal pass. Distinguish authored spacer slots from generated runtime cells/pagination controls.

Growing a static ring preserves every retained cell's ID/content/action/style and adds only fresh spacer IDs. Shrinking uses the existing `ResizePlan` content-at-risk information. Offer explicit relocation/valid overflow-ring/discard choices; no populated cell disappears because a text field temporarily held a smaller number. Any created overflow ring must use the same safe geometry helper, not the older blind radius-plus-64 formula.

Apply rechecks the proposal's base generation and full candidate validation, then commits geometry and contents together through the existing `replace_document_atomic`/authoring transaction. Undo restores the entire accepted change, references, and selection coherently. If stale/invalid, leave the prior draft usable and show a recalculation/conflict explanation. Save remains the existing durable transaction and single-owner persistence flow.

### C4. Compile checkpoint

After Packages B/C are integrated, use `cargo check --lib --bin multi_launcher` once to collect compiler errors for the combined implementation. Fix root errors in batches. Do not execute a test for every ring count or popup control during coding.

At this point all requested code should be implemented or a specific native cause/evidence gap identified. Proceed to the one final verification phase; do not expand into typography, skin import, another Cascade redesign, or new dynamic source plugins.

---

# 5. Minimal but meaningful verification

Testing stays deliberately small during development. Do not generate a 60-case new milestone matrix or repeat broad historical radial filters after every package. Reuse existing coverage and add parameterized cases only for the changed failure mechanisms.

## 5.1 Approximately six focused regression groups

1. **The repaired input/visibility boundary:** The exact production path that was shown to fail, covering focused/unfocused ROOT and Designer state without duplicate tap/hold or stale restore effects. Do not only assert that `RootViewportCtx` contains the ROOT enum again.
2. **Designer readiness/close:** The actual repaired client-input/readiness/request case; mouse and Tab routing where implicated; exact stale/failure reconciliation and prompt/terminal close without data loss. Use the production callback/authoring bridge rather than only `open_test_snapshot` bypassing bootstrap.
3. **Basic toolbar operations/errors:** New Menu/Add Ring/Slots reachable with tree/Inspector hidden; an accepted operation updates state; invalid operation gives its true reason and preserves content.
4. **Action picker and Inspector handoff:** A valid action beyond the old first-50 list is reachable by search; assignment is non-executing; dirty popup edits survive or are explicitly resolved; Inspector is actually revealed.
5. **Geometry proposals and application:** Representative small/large counts including 50; customized neighboring rings/effective style; valid fit or honest refusal; stale plan and cancellation leave the draft unchanged. Test both supported layout kinds using existing constraints where relevant.
6. **Data preservation:** Growing/shrinking with populated cells, explicit relocation/overflow, stable IDs, one-step undo/redo, and existing save/reopen behavior. No silent dynamic-to-static conversion.

These are coverage groups, not a hard cap forbidding one additional necessary regression. Avoid global hooks, real SendInput, real user files, long sleeps, and extra integration binaries in ordinary tests. Fake clocks/adapters are appropriate, but do not mislabel them native acceptance.

Existing tests requiring intentionally changed UI assumptions—basic controls only in the tree, popup Cancel standing in for Inspector navigation, or a fixed starter radius—must be rewritten around the approved new behavior. Existing safety, persistence, real stable-ID, command, and no-execution tests are not weakened/deleted/ignored.

## 5.2 Short real Windows walkthrough

Use a source-matched executable/profile and preserve the user's test directory. A pristine profile is useful only as a labeled comparison, not a substitute for the failing settings. A human can press the actual Windows-key chord when the automation environment cannot synthesize it; that does not authorize tool-policy violations or fabricated events.

**First prove interaction:**

```text
Grid focused -> short tap -> hidden.
Short tap -> visible; repeat with Designer closed and open.
Open Menus -> click a simple UI control -> visible response.
Click a text field and type; Tab moves focus between eligible controls.
Open Skins -> change a harmless draft field -> visible response.
Hide the grid -> Designer remains interactive.
Close clean Designer -> promptly gone; main launcher remains usable.
```

Record actual preview state rather than trust an unresponsive toggle. Preserve correct runtime hold/sticky behavior, prior SameCenter/Cascade defaults, and emergency/recovery.

**Then one authoring workflow:**

```text
New Menu -> Add Ring -> choose Slots -> inspect geometry proposal -> Apply
-> assign an action using search -> open selected cell in Inspector
-> change a skin value -> safe preview -> Save -> close -> reopen
-> verify contents, IDs and selected settings retained.
```

Use a harmless non-executed action and separate disposable fixture menu/skin. Do not run user macros or delete real data merely to verify editing. Exercise one shrink-with-content prompt and undo; cancel a proposal to prove non-mutation. This is a compact acceptance session, not another full native test framework project.

Native action/title-bar success is not sufficient: client clicks, keyboard focus, root focused toggle, and saved result are the actual gates. Record latency only if measured; do not promise zero delay or infer frame speed from `cargo check`.

If no suitable Windows interaction environment exists, finish all unblocked code/checks, deliver the actual linked candidate and minimal trace instructions, and list the precise remaining native gate. Do not claim the goal complete; also do not spend hours adding more instrumentation without a concrete missing event boundary.

## 5.3 Final command set

After substantive implementation and one bounded review, with the normal project target/features/profile:

```text
cargo fmt --all --check
cargo check --lib --bin multi_launcher
git diff --check
cargo nextest run --no-fail-fast
```

Reuse an already-valid check for unchanged inputs rather than run it redundantly by ceremony. A necessary linked executable uses `cargo build --bin multi_launcher` as described above. Optional `cargo check --tests` catches test-only compile issues without running tests, but do not automatically repeat it and a broad `--all-targets` run.

Use any explicitly required repository checks, not new broad lint/benchmark/feature-combination policies. Validate that the full Nextest run uses the established complete scope; a huge skip count from an accidental filter is not full success. Existing intentionally skipped tests must be disclosed. If failures occur, collect all from the retained log, classify, and repair together. Rerun affected diagnostics and the full suite only when relevant remediation invalidates the candidate.

No claims that `cargo check` executed unit tests, linked a new executable, or proved Windows input. No historical test totals reused as new results. [W1]

---

# 6. Definition of done and final report

The assignment is **implemented and natively accepted** only when:

- Short tap toggles the actual focused Multi Lnchr and ordinary unfocused/hidden cases correctly; hold remains radial-only.
- Both Designer modes respond to client mouse and keyboard, including with grid hidden; loading/conflict/failure is visible and recoverable.
- Clean close is prompt and durable/dirty cases preserve existing safety and confirmed semantics.
- New Menu/Add Ring/Ring/Slots are visible in the default workspace, not trapped in optional panes.
- Geometry proposal visibly fits or honestly explains its limits; no silent neighbor moves, discarded edits, hidden losses, or invalid saved arrangement.
- The action search finds beyond the previous first 50; Open in Inspector actually hands off without losing popup edits.
- Basic and direct editing use existing authoring/undo/identity/action boundaries; no Design-mode execution or dynamic-source conversion.
- The complete short Save/reopen workflow succeeds.
- Meaningful checks and retained full required Nextest pass on the actual final code; native evidence is separately recorded.
- Intended coherent commits exist and unrelated user changes are preserved.

Do not substitute “no source-review findings” for a working editor, a green suite for a native click, or another diagnostic binary for the user's requested result. A missing native host is a specific acceptance limitation, not proof that code failed or passed.

Final report should be short enough to use:

1. **Fixed and observed:** each blocker, confirmed cause, actual native evidence versus remaining uncertainty.
2. **Authoring changes:** where basic controls are, action search, Inspector handoff, geometry proposal and shrink behavior.
3. **Preserved:** data/IDs, settings, safety, existing semantics and unrelated functions.
4. **Verification:** exact check/build/test commands, candidate identity, real exit codes/counts, and native walkthrough result. Distinguish check-only, built, tested, and native-accepted.
5. **Commits / remaining issues:** actual hashes/subjects, no fabricated completion percentage. Note any unavailable native evidence plainly.

Use a compact ledger status block for package progress and the currently active job, not another thousand-line duplicated initiative. On resume, read it, reattach to the existing job if any, honor the next 10/15/20-minute observation time, then continue code-first work. No routine request to approve already-selected options.

## Primary technical references — supplemental, not repository evidence

The attached source is the basis for implementation. These references clarify tool/API behavior only; they do not diagnose the user's unresponsive window.

```text
[W1] Cargo Book — cargo check, target selection and code-generation limits
https://doc.rust-lang.org/cargo/commands/cargo-check.html

[W2] egui 0.27.2 Context — input, locks, deferred viewports, explicit targeting
https://docs.rs/egui/0.27.2/egui/struct.Context.html

[W3] Microsoft — EnableWindow / native input eligibility
https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enablewindow
```

Use the current pinned versions and API ownership in the actual checkout. Do not upgrade eframe/winit/Windows crates or bypass operating-system input rules to avoid tracing the current boundaries.
