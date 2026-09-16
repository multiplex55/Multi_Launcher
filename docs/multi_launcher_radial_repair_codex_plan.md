# Multi Launcher — Radial Runtime Repairs and Compact Visual Designer
## Approved Codex implementation brief

**Status:** Approved. Implement this repair initiative; do not ask the user to approve the same choices again.  
**Inspected snapshot:** `multi_launcher(20260916-004931).zip`.  
**Implementation authority:** The current checkout, preserving newer legitimate work and the already-recorded immutable branch baseline.  
**Reference material:** Repository-local `docs/references/`, the reported screenshots, and the existing RM4 designer reference.  
**Primary goals:** Repair the reported behavior; make editing compact and direct; preserve existing functionality, performance, and data.  
**New job-monitoring policy:** Check long-running jobs at approximately **10 minutes, then 15 minutes, then every 20 minutes**, unless a completion/failure notification arrives sooner. This replaces all earlier 60–300-second observation schedules.

This document is an implementation specification, not evidence that the application has been fixed or tested. Source findings are summarized in `multi_launcher_radial_repair_source_notes.md`; external API references are separated in Appendix C.

---

# 0. Assignment, authority, and scope

Read the project's `AGENTS.md`, applicable project-owned instructions, and the existing radial implementation ledger before editing. Work on the currently checked-out branch. Preserve unrelated staged/unstaged/untracked user changes. Do not reset to the attached snapshot or the historical baseline.

Keep the previously recorded branch-first-commit baseline unchanged. Record a separate `REPAIR_START_HEAD`, initial working-tree identity, and the current repair scope in `docs/plans/radial-repair-and-designer.md` or an equivalent clearly linked repair section. This is a follow-up to the existing radial feature, not a restart of its M0–M6 initiative. Historical “complete” labels do not prove the newly reported native bugs are fixed.

Current source wins over historical path names. The approved behavior below intentionally replaces earlier behavior only where explicitly stated. Do not reintroduce immediate-press dismissal or Cascade defaults merely because older tests/plans require them. Add a short supersession note to the applicable existing plan/help; retain the historical record instead of rewriting it as if the earlier requirements never existed.

The user has accepted all preselected repair choices. Decide routine APIs, small layout constants, and test organization autonomously. Ask only about a genuinely unresolvable destructive/data conflict. Do not turn technical implementation decisions into another questionnaire.

## 0.1 New slow-machine policy — supersedes earlier monitoring intervals

The machine is slow at builds and tests. Implement substantial coherent batches, write/refactor tests alongside the code, and execute focused gates at milestone boundaries. Do not run a full suite before starting or after every small edit.

**Observation cadence for a still-running Cargo/build/Nextest job:**

| Check | Wait since launch or the preceding scheduled check |
|---|---:|
| First status check | About **10 minutes / 600 seconds** |
| Second, if still running | About **15 minutes / 900 seconds** |
| Subsequent checks | About **20 minutes / 1,200 seconds** |

These are observation intervals, not timeouts or permission to kill the job. A completion/failure notification should be handled immediately; do not deliberately wait 20 more minutes once the exit is known. Fast synchronous commands such as `git diff --check` need no artificial wait.

Mandatory execution rules:

1. At most one Cargo/build/test job uses the shared checkout/target directory. Read-only review agents must not start independent builds. Baseline/candidate measurements, when needed, are also serialized.
2. Record command, cwd, source revision/diff identity including new files, environment/profile, start time, session/PID, log path, expected next observation time, and true exit code. Verify PID identity before reattaching; PID alone can be reused.
3. Prefer completion-driven or blocking tool waits that can return early. Respect the actual tool's maximum wait. If a wrapper yields sooner, keep the same job/session and the scheduled observation time; do not replace long waits with a rapid loop of status checks. Use an available persistent wait/job mechanism, not an invented tool capability.
4. Do not build a new scheduling daemon just to implement slower observation. Do not promise an unsupported future callback. When a process already runs, reattach rather than relaunching it.
5. Do not run `Get-Process`, repeated log tails, or “wake up and check” prompts every few seconds. Do not retain the older 60/120/180/300-second schedule in the active repair runner or resume prompt.
6. Silence, a `SLOW` marker, or a short tool-call timeout is not a hang. Keep compilation, linking, test execution, and Cargo-lock waiting distinct. Check actual process state only at the scheduled observation or for a concrete failure/cancellation reason.
7. Do not run `cargo clean`, alter profiles/toolchains, reduce required coverage, add fake-passing timeouts, or kill a healthy build to save waiting. Do not mutate its build inputs while it is producing evidence for a source snapshot.
8. Retain stdout/stderr to a durable log and capture the real Cargo/Nextest exit code; a logging pipeline must not mask it. The earlier ledger lost test summaries in PTY transcripts: do not repeat that problem and then rebuild merely to rediscover the failed test's name.
9. Use `--no-fail-fast` for each coherent focused gate as well as the final suite where supported by the current command. Collect failures in one pass, classify them, and repair the coherent set before the next build. Do not deliberately manufacture a one-failure-per-20-minute-build workflow. Do not weaken tests to do this.
10. Do not use `--no-capture` merely for progress visibility; Nextest documents that it serializes execution. Retain captured logs/status instead. Verify flags against the installed runner. [W4]
11. Run targeted verification once the work package is integrated, not after each inspector control or changed match arm. Static source/type-signature inspection is useful between gates, but rustfmt is not a type checker and a successful editor diagnostic scan is not a substitute for Cargo.
12. Keep one meaningful review pass per repair milestone and a final independent review. Address real findings, but do not request unlimited hypothetical “closure reviews” after a clean gate. New unrelated enhancements go to a future backlog.
13. A successful run remains evidence for unchanged code/build inputs. Do not repeat a full suite solely because a ledger/status paragraph changed. Relevant post-run code/test/config changes require new verification; prior success cannot be relabeled as covering them.

**This timing policy affects Codex's observation only.** Do not change the 350 ms hold default, 300 ms tooltip delay, UI scheduling, input timers, or test clocks to minutes. No unit test should sleep for 10–20 minutes.

## 0.2 Non-goals and preservation requirements

Do not replace the native radial renderer, create a second launcher application/backend, add an entirely new radial action system, rewrite all global hotkeys, or redesign unrelated plugin UIs. No new radial feature family, third submenu display mode, GPU migration, or additional import compatibility project is requested.

Preserve existing menu/skin fields and controls, stable IDs, action bindings, collections, pagination, import/export, revision checks, transaction safety, preview safety, undo/redo, and action confirmations. Preserve the existing meaning of Save/Apply/Cancel, including its current checked Cancel-after-Apply behavior; relocating the UI is not permission to silently change that contract.

Use original/native UI implementation informed by RM4's interaction patterns. Treat `docs/references/` scripts as text/data. Do not run them or copy reference artwork/code into the product without applicable rights. The requested center-click Add flow is an approved product choice; the source-reviewed RM4 center-drag flow is reference evidence, not proof that every requested gesture is identical upstream.

---

# 1. Approved behavior: the contract to implement

## 1.1 Independent launcher and radial toggles

For the configured shared launcher chord, after higher-priority emergency/exclusive ownership is handled:

- Release the primary trigger before the configured threshold: toggle only the normal grid/list launcher, immediately on release.
- Hold through the threshold: toggle only the radial. Do not briefly show the grid first.
- In default sticky mode, releasing the original opening hold leaves the radial open.
- An open radial does **not** make the next chord press an immediate dismissal. It still waits to distinguish tap from hold.
- A long hold used to close a radial is consumed once. Its release must not reopen it or toggle the grid.
- A short tap while the radial is open toggles the grid without closing/recreating/reordering the radial or changing its captured context.
- The rule works with the grid hidden/offscreen, visible but unfocused, and focused. It also works while the new Designer is focused, except explicit hotkey-capture UI may retain its established scoped capture behavior.
- Keep the user's chord and threshold; 350 ms remains the existing default, not a forced replacement.
- Esc remains immediate radial cancellation while a radial owns that cancellation route. Do not deliver the same Esc to an underlying editor. Screen Draw recovery, emergency stop, quit, and other existing priority owners retain their established priority.
- Preserve existing direct named-menu triggers, `radial show/close`, release-to-select, and hold-and-click behavior outside the approved shared-trigger change. The initiating release for those alternative modes retains its existing mode semantics.

Use this truth table as a regression oracle for sticky mode:

| Grid | Radial | Short tap result | Long hold result |
|---|---|---|---|
| Hidden | Closed | Grid visible; radial closed | Grid hidden; radial open |
| Visible | Closed | Grid hidden; radial closed | Grid visible; radial open |
| Hidden | Open | Grid visible; radial unchanged | Grid hidden; radial closed |
| Visible | Open | Grid hidden; radial unchanged | Grid visible; radial closed |

Repeat visible-grid cases with grid focused and unfocused. No mouse movement or external refresh is allowed to complete the test scenario.

## 1.2 Spatial navigation

Default to SameCenter at every depth. Only the active wheel is shown in that mode. Entering a child replaces the wheel at the current **visible physical center**, not an old requested position. Center-click in a runtime child goes Back one level; root-center drag remains supported.

Preserve the session's monitor/work area/DPI context during ordinary navigation. Moving the pointer must not choose another monitor for a submenu. A deliberate supported drag updates the session anchor coherently; Back does not revert to a stale pre-drag origin.

For a larger child near an edge, preserve the center. Fit within usable limits and paginate dynamic content using existing mechanisms. Never hide static cells, silently discard actions, shrink buttons beyond the established usable minimum, or move to a different desktop location. When safe fitting is impossible, keep the current menu usable and explain the problem with an explicit user-controlled alternative; do not secretly reposition.

Keep Cascade as an explicit alternative. Correct its coordinate transforms and position children close to or partly overlapping the current wheel, with an obvious active level and protective inactive ancestors. Do not accumulate desktop-sized offsets.

The user authorizes a **one-time, presentation-only conversion of existing submenu settings to SameCenter**, with backup and an undo path. Preserve IDs/content/actions/skins. Make the conversion visible once, not a recurring prompt or per-frame rewrite. A later explicit Cascade choice must remain in effect.

## 1.3 Tooltip, diagnostic, and pointer behavior

Full-label hover tooltips are enabled by default, with a configurable 300 ms delay independent of the hold threshold. They show the original complete label plus optional custom descriptive text and wrap at a sensible monitor-aware width. They work in runtime, embedded Designer preview, and native preview.

Expected `LabelTruncated` diagnostics are hidden from normal preview content/toasts by default. Keep deduplicated optional details in a collapsed Diagnostics section and a developer visibility setting. Actual asset, configuration, save, invalid-action, and runtime faults remain actionable. Do not globally suppress font/error logging or classify warnings by brittle string prefixes.

Ordinary radial hover uses the normal Windows arrow pointer. Preserve appropriate drag/resize cursor feedback. Fix class/message handling where necessary, and investigate genuine stalls instead of merely hiding a busy indicator.

## 1.4 Designer behavior

Deliver one ordinary separate desktop window, **Radial Designer**, in the existing process. Menus and Skins are tabs/modes of the same editor/backend. Suggested initial size is approximately 900 × 650 logical units, clamped to the work area; remember user geometry thereafter. It is not always-on-top by default.

The grid can stay hidden while the Designer renders, receives service replies, saves, and edits. Grid hotkeys affect the grid, not the Designer. Closing the Designer closes only that window, with existing dirty/transaction protection and native-preview cleanup; it must not quit Multi Launcher.

Use a narrow optional tree, bounded canvas, and hideable/manually resizable inspector. No equal-width three-column allocation. All advanced fields remain available in collapsible groups/tabs. Basic label/icon/content/action controls remain easy to reach.

Remember pane widths, visibility, open sections/tree nodes, zoom and pan. After initial defaults, only explicit user actions change these preferences. Selection, diagnostics, refresh, search, or new content must not open sections, expand the window, resize pane preferences, or repeatedly fit the preview. Long content scrolls within its bounded region. Fit is an explicit action. Preview zoom is not menu scale.

Design mode is non-executing and shows empty slot outlines. Center '+' starts adding a cell to an explicit ring/slot. Empty-slot click starts editing that slot; existing-cell single-click selects; right-click opens compact properties; double-click a submenu opens it for editing. Breadcrumb/Back navigates the editing path. Runtime center Back and Designer center Add are intentionally different.

Dragging previews the destination and cannot silently overwrite an occupied cell. Creating and linking a submenu is one undoable operation. Shrinking a ring cannot silently lose populated cells. Selecting generated dynamic results cannot convert them into static definitions. Test Action/native runtime-style preview remain explicit and safe.

---

# 2. Current-source map and bounded milestones

The attached source supports these observations; it does not prove the corresponding real Windows diagnosis. Revalidate moved paths in the actual checkout. Refer to companion notes [S1–S8].

| Current seam | Source finding / repair ownership |
|---|---|
| `src/gui/mod.rs::send_event`, `register_event_sender` | Common event publication currently enqueues without a repaint wake in that function. Own queue-and-wake delivery together. |
| `src/main.rs`, `src/gui/watch.rs`, `src/gui/radial_actions.rs` | `RadialPrepare` crosses into GUI processing; the reply wakes main. Preparation/dispatch must progress with the root hidden. |
| `src/radial/invocation.rs`, `src/hotkey/launcher_invocation.rs` | The reducer currently closes an active radial on a new `ChordPressed`. Change the actual lifecycle, not just the threshold constant. |
| `src/visibility.rs`, `src/gui/render.rs` | The existing visibility toggle has no focused-window exemption; trace input consumption/event delivery/restore ordering rather than adding unfocus workarounds. |
| `src/radial/controller.rs::open_submenu`, `shape_center`, `desktop_geometry`; `geometry.rs` | Child presentation is selected, SameCenter uses requested anchor, current cursor geometry is sampled, and cascade center conversion appears to add an extra origin. Repair one shared spatial contract. |
| `src/radial/font_cache.rs`, `preparation.rs`, `render.rs` | Tooltip layout reuses narrow label preparation and small drawing bounds. Preserve original text and give tooltip layout its own constraints. |
| `src/gui/radial_editor/preview.rs`, runtime diagnostic routing | Preview emits its diagnostics inline. Classify/deduplicate at a shared radial diagnostic boundary, not only in one widget. |
| `src/radial/native.rs` | Class creation leaves the cursor unset in the inspected path; no explicit cursor handlers were found. Repair native cursor policy without global changes. |
| `src/gui/radial_editor/mod.rs` | Embedded `egui::Window`, 1100 × 720 default, equal columns, fixed preview scale, tree `default_open(true)`. Separate native presentation and bounded layout from authoring. |
| `src/radial/authoring.rs`, `authoring/menu.rs`, `authoring/native_preview.rs` | Existing typed drafts, transactions, stable-ID graph edits, preview leases, and undo are the reusable backend, not replacement targets. |

Use these milestone gates. Subtasks inside a milestone are not separate mandatory Cargo runs.

| Milestone | Deliverable | Gate |
|---|---|---|
| R0 | Source/ownership audit and repair ledger | Read-only source/diff review; no initial full suite |
| R1 | Independent input cycles and reliable event wakeups | Combined input/event/visibility tests; targeted real no-mouse check |
| R2 | SameCenter, close Cascade, Back/drag transforms, one-time migration | Geometry/session/migration tests; multi-level live check |
| R3 | Normal cursor, complete tooltips, quiet truncation diagnostics | Focused font/tooltip/native/preview tests; hover smoke check |
| R4 | Independent compact Designer plus safe direct manipulation | One integrated editor/authoring/UI-lifecycle batch and Designer smoke check |
| R5 | Cross-feature regression, resource/performance check, final review | Complete required Nextest suite and consolidated native acceptance |

Commit verified milestones, not compile-breaking partial migrations. One writer per checkout. Do not create a new integration-test binary per milestone when module tests and existing suites suffice.

---

# R0 — Confirm ownership and prepare the repair ledger

## Objective

Start from the real current implementation and the reported failures, without rebuilding the entire original plan.

## Steps

1. Read `AGENTS.md`, Git status, previous radial ledger/baseline, and current repair inputs. Record repair-start revision and existing uncommitted work. Keep `docs/references/` originals unchanged.
2. Read the listed source seams and adjacent tests. Identify which symptoms share a cause and which remain hypotheses. In particular, do not claim that adding one repaint call automatically fixes focused tap-to-hide.
3. Trace the full route for physical chord down/up -> invocation reducer -> main loop -> visibility or preparation -> GUI processing -> native presentation. Trace restore flags and Screen Draw priority in the same route.
4. Identify GUI event sink lifetime/registration and the application's existing eframe context/event-loop proxy. Identify which service replies the editor polls and what wakes their receiving viewport.
5. Trace all submenu paths: enter, Back, paging, drag, prepared-frame reload, embedded preview, native preview, and topology changes. List coordinate spaces and units explicitly.
6. Inspect current editor UI/panel integration, action catalog client, authoring session, close/dirty flow, and current persisted UI-state conventions. Locate every Open Menus/Open Skins entry point.
7. Inspect the local RM4 designer's item/center drag source and the supplied designer screenshot for the direct-manipulation interaction reference. Preserve licensing boundaries; use native authoring operations.
8. Establish actual test module/binary names and installed Nextest options without running a baseline full build. Record the long-job command/log/wait protocol once.

## Gate

The ledger maps each reported issue to a bounded repair task, source evidence, acceptance scenario, and intended test. Record uncertainties for the implementer to investigate, not a new user requirements survey. Start R1 once the ownership map is sufficient; do not spend another milestone exhaustively auditing unrelated features.

---

# R1 — Independent hotkey cycles and reliable GUI/main wakeups

## Objective

A tap toggles the grid and a hold toggles the radial in every ordinary visibility/focus state. Runtime progress must not wait for mouse-induced egui activity.

## R1-A: Fix event delivery at its owner

1. Make successful event enqueue and waking the correct consumer one coherent operation. Prefer extending the existing event-sink registration with an explicit wake handle/viewport destination rather than scattering `request_repaint()` at individual plugin calls.
2. Enqueue **before** requesting the wake. Clone/extract wake handles and release registry locks before invoking callbacks; do not call into egui while holding a global sender list or while recursively holding an egui context lock. Coalesce naturally at the event-loop integration; do not lose the last event during an empty/draining race.
3. Use the actual owner viewport explicitly: root-owned preparation/dispatch wakes `ViewportId::ROOT`, not whichever viewport was last current. Designer-owned replies introduced in R4 wake its stable viewport ID. Root repaint requests must not include Show/Focus/OuterPosition commands.
4. Register the sink/wake during established app/context initialization, before the first relevant request can be lost. Requests queued before attachment need a safe first wake when attachment completes. Remove dead sinks and dispose subscriptions correctly; tests creating many app instances must not accumulate live contexts.
5. Keep the GUI-owned preparation/action safety boundary. Independent display does not require cloning the entire search/action backend or running GUI-owned state unsafely on the hook thread.
6. Process pending service requests independently of `visible_flag`, focus, hovered state, or whether the radial editor is open. Audit early returns and hidden/minimized paths. If the integration suppresses rendering for an occluded/hidden viewport, service work still needs an event-loop update path; do not “fix” it by making the grid visible.
7. Replies, asynchronous preparation completion, authoring notifications, and invalidations need symmetric wakeup rules. Do not leave the reverse path waiting for a later mouse move.
8. Preserve generation/request IDs and stale-reply rejection. Repeated wakeups may cause more than one update but never execute an action or consume a command twice.
9. Use finite event-driven redraws only. No 16 ms/100 ms permanent root repaint loop, synthetic mouse motion, fake keypress, busy-wait, or worker blocked waiting for a GUI frame.

The pinned egui Context API supports waking through a repaint request from another thread; use the existing eframe callback integration rather than replacing it casually. The specified viewport matters with a separate Designer. [W1]

## R1-B: Separate active radial lifetime from the next physical chord

The current `InvocationState::RadialActive` includes the original invocation identity and immediately dismisses on a new press. That representation needs a deliberate update so an active menu can coexist with a new pending tap/hold cycle.

Use the smallest typed representation that independently tracks:

- the currently active/opening/closing radial session and its generation;
- the current physical chord cycle, primary key, deadline, provenance, and release ownership;
- whether that cycle has emitted its one tap/hold outcome;
- a held-cycle action: open if closed at admission, close/cancel the existing session if open/opening at admission.

Do not blindly read “radial visible right now” at release and invert it. If Esc closes the radial while a closing hold is pending, that hold must not reopen a new menu. If opening/preparation is pending, a new closing hold cancels that pending open and invalidates its late reply.

Required transition behavior:

1. New shared-chord press does not change either surface immediately. Start one deadline while preserving the existing radial session.
2. Release before deadline emits exactly one `ToggleLegacyLauncher`, cancels that cycle's deadline, and leaves the radial's current tree/context/hover state intact.
3. Deadline while held emits the admitted radial-only open/close intent exactly once. The release drains that cycle and never becomes a tap.
4. A delayed release whose event timestamp crossed the deadline is a hold, not a tap. Preserve safe cancellation when release-to-select had no presented/armed target.
5. The release of the cycle that originally opened a release-sensitive menu retains that mode's behavior. A later short tap while a sticky menu remains open is not forwarded as a release-selection event to the old session.
6. Modifier releases/repeats do not create extra cycles. Invocation modifiers still do not become unintended Ctrl/Shift/Alt-click actions. Physical/external injected/self-generated provenance rules remain intact.
7. Session feedback about the old opening invocation cannot erase a newer pending chord. Match the specific lifecycle/request/generation before changing state.
8. Esc, exclusive-tool acquisition, reload, hook failure, shutdown, and focus-independent cancellation drain owned keys/releases without creating delayed toggle side effects.
9. Shared trigger matching has correct priority over menu-local accelerators. A real physical launcher chord is not ignored merely because the foreground window belongs to Multi Launcher. Preserve explicit binding-capture safeguards rather than treating all own-process focus as capture.
10. When the grid is toggled while the radial stays open, update normal keyboard ownership appropriately without resampling the radial's frozen action context. Typing in the newly focused grid must not also activate radial-local hotstrings/letters.

Retain existing direct-menu trigger semantics and fail-closed legacy restoration when shared mode is disabled. No second listener for the same shared chord.

## R1-C: Trace focused hide and visibility ordering

The existing normal toggle already inverts visibility without a focus exemption. Audit what happens before and after it: hook consumption, keyboard co-fire, GUI restore requests, focus helpers, `last_visible`, and queued stale Show events.

A short tap while the grid has focus must hide/offscreen it and leave it hidden. Do not first unfocus it, send synthetic Escape, or set a “visible=false” flag without delivering the viewport command. Nor may a stale `restore_flag` immediately undo the hide.

Preserve static/follow-mouse placement on genuine grid show and geometry preservation on internal restore. Radial open/close must not invoke grid placement. If a command intentionally opens a real launcher editor, retain that command's explicit existing behavior.

## Tests and gate

Add/migrate tests for the full truth table; repeat key/down/up; timestamps immediately below/at/above threshold; delayed preparation; Esc during pending open/close; old session close versus new pending chord; reload; Screen Draw immediate recovery; direct-only mode; own-window focus; and invocation-modifier safety.

Test queue-and-wake ordering, burst delivery, register-after-queue, closed sink removal, stale replies, and correct owner viewport. A test that calls `prepare_radial()` directly does not establish that the real queued request wakes its consumer; cover the actual sender/receiver boundary with a controllable fake wake adapter.

Run one coherent focused input/event/visibility gate. Use a safe real Windows check with **no pointer movement**: hold from hidden grid, tap-to-hide from focused grid, both surfaces open with independent tap/hold. Record native evidence separately. If no interactive Windows environment is available, mark those cases unverified rather than passed.

**Done:** Correct independent outcomes and event-driven progress; no repaint loop, duplicate listener, stale restore, or regression to emergency ownership.

---

# R2 — Stable submenu centers, close cascades, and safe preference conversion

## Objective

Root -> child -> grandchild -> Back stays in one predictable place in SameCenter. The menu setting has an obvious, consistently implemented scope.

## R2-A: One coordinate and anchor contract

1. Document/encode physical desktop points, desktop-logical points, local surface/image points, work-area bounds, and DPI. Reuse existing types, adding narrow wrappers only where they remove ambiguity.
2. Capture session monitor/work area/DPI during root placement. Compute the actual displayed physical center **after** root fitting/clamping. Keep requested root anchor separately for diagnostics only; never substitute it for visible center during navigation.
3. Add/reuse a session spatial state with the current visible anchor and topology generation. Ordinary navigation does not call cursor-based `desktop_geometry()` again. A genuine display-topology event may pause/revalidate through the existing safe lifecycle, not silently jump the menu.
4. Make one pure placement function serve runtime, embedded preview, and native preview. Preview uses its explicit synthetic/viewport coordinate context, not the physical cursor.
5. SameCenter places the child's menu center at the same physical point. Child native bitmap/top-left may differ because dimensions differ; that is not drift if the actual center remains fixed. Separate surface bounds from logical menu position.
6. Fix `shape_center` so desktop origin is applied exactly once. State its input/output coordinate spaces and test nonzero origins. Positive/negative monitor offsets must not amplify with submenu depth.
7. Back and paging use the same live session anchor and frozen frame data. Do not restore stale positions or resample collections merely because the user navigated back.
8. Deliberate supported dragging updates spatial state, hit testing, rendering, and relevant saved navigation frames atomically. A drag is not a Back click. Keep return navigation feasible when parent menus are larger; use the visited stack's fitting constraints where needed rather than discovering an inaccessible parent afterward.
9. Reset activation arming after navigation/reflow. The click/release entering a child cannot select a child item underneath the cursor.

## R2-B: Make the setting's scope visible

Use one documented rule, not a root control that is silently defeated by a child's serialized default.

Recommended bounded implementation with the existing menu field:

> The **currently displayed menu's** `submenu_presentation` controls how its children open.

Label it **“Open child menus: Same center / Cascade”**. A child's own setting controls the next level below it. Global/default settings seed new menus and must be labeled as creation defaults unless the current schema explicitly supports inheritance.

Move the runtime decision from the hidden child's default to that documented current-menu rule. Update preview/import documentation and tests consistently. Do not introduce a general new inheritance framework for this repair. If the current checkout already has explicit inheritable presentation, reuse that model and expose the effective value/source; the visible parent setting still must have an honest meaning.

## R2-C: Fixed-center fit and Cascade

At a fixed center, derive available extent from all four sides of the captured work area, accounting for visual rim/shadow/labels. Apply the current bounded scaling policy only while preserving usable targets. Reuse dynamic pagination; do not paginate static definitions by dropping authored cells or increasing undocumented limits.

If a child cannot safely fit, leave the previous frame and navigation stack unchanged and display a bounded, clear diagnostic with an explicit action to edit size/layout or deliberately switch presentation. An error path must not push a child frame before successful layout. Tooltips are separate overlays and must not influence menu-fitting calculations.

For Cascade, calculate a small local child offset from the visible parent/cell using the corrected units. Keep the child near/partly overlapping its parent; clamp only within the session monitor and use a same-center/error fallback explicitly when proximity cannot be maintained. Do not allow a cumulative `origin + already-desktop-point` offset. Preserve inactive-parent hit protection and Back semantics.

## R2-D: One-time conversion, not an endless default reset

The user approved conversion of existing submenu presentation to SameCenter. Implement it through existing versioned persistence/migration/transaction boundaries, not direct writes from a render callback.

1. Identify the existing app settings default and per-menu presentation fields. New settings/menu definitions default to SameCenter.
2. Before changing existing persisted values, create and verify a backup using established store facilities. Record a migration ID/receipt with previous presentation values and target revision.
3. Change presentation only. Preserve IDs, ordering, cell contents, actions, skins, other settings, and unrelated metadata. Validate and publish a complete revision. Use existing cross-store mechanisms if app settings and radial document both change; avoid half-migrated startup state.
4. Apply once, with a concise “submenu presentation updated” notice and an accessible undo/restore route. Do not ask the already-answered product question again.
5. Subsequent startup, reload, editing, or import must not repeatedly overwrite deliberate Cascade choices. A marker may not be cleared simply because the user undoes the conversion.
6. Undo restores the old presentation choices transactionally without overwriting intervening unrelated edits. Detect presentation conflicts and offer the existing safe resolution, not whole-document rollback that discards newer work.
7. Malformed/newer config, backup failure, or a conflicting concurrent edit must leave original bytes and runtime state valid. Do not regenerate starter menus as a “repair.”

## Tests and gate

Cover at least three SameCenter levels, different child radii/bitmap bounds, root clamping, negative monitor origins, mixed DPI, moving pointer to another monitor during navigation, drag then child/Back, and repeated transitions without drift. Test fixed-center impossible fit leaves parent state unchanged. Test close Cascade distance/transform and render-hit-test agreement.

Cover current-menu setting scope, new defaults, one-time conversion, preserved unrelated fields/IDs, rollback/backup failure, undo after unrelated edits, and preserved explicit Cascade after migration.

Run one focused geometry/session/migration batch. In a live check, compare the visible center through enter/Back before and after dragging, including near an edge. Do not count equality of two stale requested-anchor values as proof of correct visible placement.

**Done:** The menu appears to turn into its next menu at the same position; Back restores the previous level there; Cascade is local; migration is safe and not repeated.

---

# R3 — Normal cursor, complete tooltips, and quiet expected diagnostics

## Objective

Ordinary hover feels normal and reveals complete labels without spamming the UI.

## R3-A: Native cursor ownership

Initialize the radial window class with the shared system arrow cursor and implement appropriate `WM_SETCURSOR` handling for its owned client area. Respect default/nonclient resize behavior and explicit dragging; do not force the arrow over unrelated windows. Account for capture paths where ordinary cursor messages differ. [W3]

Use the pinned windows crate's actual signatures/namespaces. Shared system cursors are not owned custom resources to destroy. Do not call `SetSystemCursor`, globally hide/show the cursor repeatedly, or run a timer that constantly overwrites it.

Inspect hook/native/GUI stalls separately. A cursor assignment is not proof that decoding, locking, or render work is fast. If hover blocks on expensive work, fix that narrow source rather than cosmetically masking a real busy state.

## R3-B: Separate complete text from cell-display text

Preserve an immutable full label for every authored and generated/dynamic cell. Cell-layout ellipsis must not overwrite the source string or stored user label.

Build a distinct tooltip-layout request with its own width, wrapping policy, DPI, typography, and bounds. Do not clone a 40–60-pixel cell-label request and only change its text. Cache keys must distinguish label versus tooltip purpose and relevant constraints. Reuse the existing font discovery/glyph cache; no per-hover font scans.

Tooltip content order: full label, then distinct nonempty custom description. Do not replace the readable label with a generated internal ID unless the item genuinely has no label. Keep strings/caching bounded using existing validation limits. For pathological labels exceeding a documented safety/view limit, preserve the complete source in the properties/details view and clearly indicate any tooltip omission; do not claim clipped text is complete.

Add/reuse a visible feature preference for full-label tooltips and delay. Default All cells, 300 ms. Make the setting's relationship to existing per-skin/custom-tooltip controls clear so a legacy auto-tooltip default cannot silently defeat the new accepted default. Retain explicit Off/other supported mode controls. New fields deserialize compatibly; do not repurpose hold delay or silently rewrite custom descriptions.

## R3-C: Delayed display without polling or displacement

Use a generation-tagged one-shot hover deadline. Moving to another cell, entering a child, closing, dragging, or losing appropriate hover ownership cancels it. Remaining stationary must still display the tooltip at the deadline; do not require a second mouse event.

The tooltip uses the full-label layout and monitor-aware width/height. Place/flip/clamp the tooltip independently, leaving menu center/layout/input regions unchanged. It must not acquire keyboard focus, intercept the cell click, cover Back with an input shield, or trigger a submenu.

For native presentation, extending a visual bitmap or using an existing passive tooltip layer is acceptable if it preserves the wheel's physical center and input region. Do not expand the navigation layout to fit a tooltip and thereby move the wheel. For egui preview, use the same content/policy but the Designer's bounds and event-driven repaint deadline.

Avoid updating all tooltip layouts on every pixel of pointer motion. Preparation may cache bounded layouts by content/style/constraints; hover selects the relevant prepared tooltip and schedules only necessary redraws.

## R3-D: Typed diagnostic filtering

Preserve diagnostic kinds from `font_cache`/preparation to the runtime and preview sinks. Treat expected `LabelTruncated` as informational layout data, not a user-action error.

Default policy:

- No repeated inline truncation list below the canvas.
- No truncation toast or warning-level log for ordinary hover/preview.
- A collapsed, bounded Diagnostics section can show deduplicated details when explicitly opened/enabled.
- Deduplication uses stable cell/source identity, diagnostic kind, and meaningful text/style/layout fingerprint—not each redraw generation, which would make every repeated warning appear new.
- Clearing/hiding details works; unchanged warnings do not immediately refill a visible banner.
- Missing assets, missing glyph coverage, invalid menu geometry, failed saves/imports, and failed actions remain classified and actionable. Do not suppress them merely because they originate from font/resource preparation.

Route genuine messages to bounded status/error UI rather than allowing an unbounded list to increase window size. Do not classify diagnostics by checking whether an English message contains “truncated.”

## Tests and gate

Test label preservation for authored and dynamic cells; full tooltip label/description; long Unicode/wrapped text; purpose-specific layout/cache keys; delay/cancellation/stationary hover; viewport-edge bounds; tooltip visibility without menu movement; click-through/nonfocus. Test informational suppression and deduplication while real errors still reach their appropriate sink.

Test class/client cursor policy and teardown with native adapters; perform real hover/capture smoke checks without changing global cursor state. Verify runtime, embedded preview, and native preview have the same full-text content and no repeated truncation banners.

**Done:** Short cells remain readable through full tooltips; ordinary truncation is quiet; the pointer is normal; genuine failures remain discoverable.

---

# R4 — Independent compact Designer and direct manipulation

## Objective

Make editing possible in a modest standalone window without enlarging the launcher, losing advanced controls, or executing actions unintentionally.

## R4-A: Separate native presentation, reuse the authoring backend

Prefer one stable **deferred egui viewport** using the existing eframe integration. Pinned egui supports a deferred callback that can repaint independently; immediate viewports couple parent/child repaint work. Verify the current pinned API before using it. [W2]

The inspected editor function borrows `&mut LauncherApp`; do not force that borrow into a `'static` deferred callback using unsafe pointers. Separate:

- Designer-owned UI state, draft/session client, selection/navigation, viewport/panes, and preview cache;
- immutable action/catalog/settings snapshots;
- typed requests/replies to existing GUI/main-owned execution and persistence services.

Use a small owned shared-state holder only where the viewport callback contract requires it. Do not wrap the entire `LauncherApp` in a new mutex or hold a lock across a service wait/native dialog. Apply queued UI intents after rendering where required to avoid mutation/borrow reentrancy. One owner consumes authoring replies; do not let root and Designer race to drain the same receiver.

Register/retain the Designer viewport while open independently of the grid `visible_flag` and `panel_stack` display decisions. Native support is required on the target Windows configuration; do not silently fall back to a giant embedded window and call it complete. Do not start another `eframe::run_native`, launcher process, or application data owner.

Use explicit `ViewportId::ROOT` versus Designer IDs for Show/Hide/Focus/Close/position commands. Existing generic code that sends to the “current viewport” must not accidentally hide the Designer when the intent is grid toggle. Never use a root `Close` command for the Designer X button.

Open Menus/Open Skins reuses/focuses this one window, selects the intended mode, and preserves its draft. Opening from a hidden launcher must not unhide the grid merely to display the Designer. Audit `Panel::RadialEditor`, `focus_panel`, interaction snapshots, command outcomes, File menu, Settings buttons, and `radial edit`/`radial skins` routes. Migrate all callers to one open intent; remove the embedded rendering path once replaced.

Service reply/preparation completion wakes the Designer through the queue-and-wake pattern from R1. Required root/main service work can wake without showing the grid. Keep the existing production renderer and safe native preview lease.

Close lifecycle: OS close -> existing dirty/pending-operation decision -> cancel/finish appropriate preview lease -> dispose Designer state/viewport. Closing a clean Designer does not close an independently opened runtime radial. Apply/config publication may still close/invalidate runtime sessions under the existing store contract. Closing during pending preview startup must not leave a late invisible input-blocking window.

## R4-B: Bounded pane layout and local UI preferences

Initial Designer size: about 900 × 650 logical units, work-area-clamped. Initial pane sizes can start near 180 logical units for tree and 290–310 for inspector; the canvas receives the remainder. These are one-time defaults, not commands applied every frame.

Use:

- compact fixed-height toolbar with Menus/Skins, Save/Apply/Cancel, undo/redo, pane toggles, and explicit Fit;
- narrow optional tree with its own scrolling;
- clipped canvas with independent zoom/pan;
- hideable/resizable inspector with bounded internal scrolling;
- collapsed Diagnostics rather than free-growing warning content.

Do not use `ui.columns(3)` for equal sizing. Do not use an auto-sized enclosing window or a 360×360-times-zoom image allocation that forces ancestors to grow. Allocate available canvas space first; paint the preview inside its clip rectangle using a transform. Hit testing uses the inverse of that exact transform, including DPI, image origin, canvas origin, pan, and zoom.

Resizing the top-level window may change available realized canvas space, but must not overwrite the user's preferred pane widths. At small sizes clamp safely or allow internal scroll; keep explicit pane toggles accessible. Do not automatically open/hide panes or reset zoom because selection changes. Do not continuously recenter/fit after every preview generation.

Store local presentation preferences through existing UI/settings persistence conventions: Designer geometry, monitor-aware restored position, pane widths/visibility, active mode, expansion states, zoom/pan. These are local UI state, not portable menu/skin definitions or authoring undo entries. Persist on meaningful interaction/close or existing debounced boundaries, not every frame. Bound retained per-entity preference maps and tolerate deleted IDs/topology changes.

Restoring geometry offscreen may be safely clamped on reopen; content-driven auto-expansion is still prohibited. Preserve the last usable geometry and protect against non-finite/unreasonable persisted values.

## R4-C: All controls, better grouping

Create a control inventory from the existing editor before moving it. Every existing field/picker/import/export operation must remain reachable. Reorganize rather than deleting advanced capabilities to achieve compactness.

Suggested groups:

- Basic: label, icon, content type, primary action/submenu/source.
- Layout: ring/slot count, radius, rotation, size, spacing, child-presentation policy.
- Actions/input: alternate bindings, hotkeys/hotstrings, after-action policy.
- Appearance: skin selection and explicit override/inheritance controls.
- Images/layers; Text; Effects/tooltips; Window behavior; Audio.
- Assets/import/export, reference guards, and diagnostics.

Basic controls remain convenient; advanced groups start collapsed. Use stable IDs and preserve user expansion by section/scope. Remove unconditional `default_open(true)`/forced-open selection logic where it defeats that choice. Filtering can show matches without forcibly expanding trees; provide an explicit Reveal/Expand action when needed.

Long labels/paths wrap or scroll inside their controls and show full tooltips. Genuine error panels have bounded height; selecting an invalid cell must not grow the desktop window. Menus/Skins modes do not render every resource editor simultaneously below the canvas.

Preserve revision/SHA checks, external-change conflict decisions, managed asset reference guards, safe package preview, and exact Save/Apply/Cancel semantics. Preview navigation, pane resizing, and zooming do not dirty the document or consume authoring undo history.

## R4-D: Design-mode canvas interactions

Add a clear `Design` versus `Preview/Test` mode separation. Default editor mode is Design. Reuse production layout/render output for visual fidelity, but Design pointer events generate **authoring commands**, not runtime session Dispatch intents.

Keep active editing-menu path separate from selected entity. Selecting a cell or changing a property does not navigate unexpectedly or expand the tree. Double-clicking a submenu or explicit Open Submenu navigates. Because menus may be reused, breadcrumb is the actual visited path, not an invented globally unique parent pointer. Back from editing goes to that path's parent.

Required operations:

1. **Empty slots:** Render subdued outlines over actual `Spacer`/available authored positions. These overlays are editor-only and do not create runtime actions or modify the document merely by being shown.
2. **Center '+':** Start a cancellable placement draft. Use the active ring when unambiguous; otherwise show a compact explicit ring/slot choice. Dragging from '+' to a valid empty slot may use the same pending payload. Cancel/outside drop leaves the document unchanged. Do not silently add rings/slots merely because space looks empty.
3. **Empty-slot click:** Open compact cell properties for that slot. Commit one valid authored cell using existing stable-ID/validation operations.
4. **Single-click occupied cell:** Select it for the inspector only; no action execution, submenu entry, or forced tree expansion.
5. **Right-click:** Show a bounded compact properties popup for label/icon/type and action/submenu/dynamic-source choice, with advanced groups collapsed. Reuse the real action picker and handoff-policy validation. Popup edits participate in the same authoring session; its Cancel must not leave a half-created cell.
6. **Double-click submenu:** Navigate the canvas to its child, preserving appropriate view state. A single-click preceding the double-click may select but must not execute a leaf or create duplicate navigation.
7. **Create submenu:** Allocate fresh stable menu/ring/cell IDs through the existing factory/graph operations, create a valid child with SameCenter defaults and Back behavior, link the chosen cell, and commit as one undoable transaction. Reuse an existing submenu through a separate clear choice; prevent cycles.
8. **Drag between slots/rings:** Carry IDs and source generation, not borrowed pointers or labels. Paint a drag ghost and exact destination highlight using the canvas transform. Empty destination moves/swaps with the spacer so there is no silent slot-count/layout change. Occupied destination offers explicit **Swap or Cancel** unless the current editor already has a clearer equivalent non-destructive choice. Preserve both bindings and their identities. No overwrite-on-drop.
9. **Add/remove rings/slots:** Use existing resize plans and reference guards. Show populated cells at risk; require explicit relocation/overflow/removal choice. Default adding a ring should choose valid starter geometry, not create an invalid document that fails only on Save.
10. **Dynamic results:** Distinguish generated cells from authored source slots. Selecting a generated result shows its full label and source metadata, and offers editing its source definition. Do not persist runtime IDs or make dragging a generated note implicitly replace a static cell. An unsupported drag is visibly unavailable, not silently destructive.

Each completed gesture is one bounded undo step; don't append a whole-document snapshot per mouse-move frame. Sliders/text edits keep existing coalescing. Undo/redo after navigation/deletion chooses a valid existing selection/path without forcing unrelated UI sections open.

The safe native desktop preview still uses real rendering/navigation but discards execution unless the existing explicit Test Action is chosen. The designer's '+' gesture must never leak into runtime preview or execute a live menu-center action. No new Universal Action executor.

## R4-E: Designer acceptance and tests

Add pure tests for canvas transforms/hit targeting at zoom/pan/DPI, add-cell cancellation, create-and-link submenu undo, selection versus navigation, spacer-preserving move, occupied Swap/Cancel, stale drag generation, graph cycles, dynamic-cell protections, and no execution in Design mode.

Add state/layout tests for one viewport identity, duplicate open reuse, Menus/Skins switching without draft loss, root visibility independence, replies waking the proper viewport, dirty close, pending native-preview cancellation, restored geometry, pane preference persistence, collapsed sections unchanged on selection/diagnostics/refresh, and no document dirtying from UI-only actions.

At approximately 900×650, the tree, canvas, inspector, and toolbar must be usable without enlarging the launcher. Also check a smaller window by hiding panes/using internal scrolling and a larger window without giant empty equal-width columns. Verify every pre-existing inspector control remains reachable.

**Done:** Real separate Designer usable while grid hidden; compact manually controlled layout; intuitive slot-first editing; unchanged data/safety semantics and no accidental execution.

---

# R5 — Consolidated regression, native acceptance, and completion

## R5-A: Final automated verification

Inspect the cumulative diff and all changed call sites. Verify source tests cover production routes rather than only direct helper calls. Preserve legitimate existing tests for the grid, visibility, command outcomes, Screen Draw, MkMacro, gestures, settings, authoring, persistence, import/export, and Universal Actions.

Classify failing tests before editing them:

- Valid existing behavior regressed: fix implementation.
- Old immediate-press-dismiss/Cascade/default-expansion assumption intentionally replaced: migrate it to the approved new contract.
- Wrong/new test fixture: repair the fixture with an explanation.
- Environmental/unrelated failure: record evidence; do not silently ignore it or falsely mark the full suite passed.

Confirm actual binary/module names and installed Nextest syntax. Example filters only after those names are verified:

```text
cargo nextest run --lib --no-fail-fast -E 'test(/radial/) | test(/launcher_invocation/)'
```

That library filter is not enough for `src/main.rs` binary tests or all existing integrations. Include the relevant existing targets in the R1/R4 gates and the complete suite at the end. Avoid compiling every integration binary unnecessarily for an entirely pure-library gate, but never exclude real affected paths merely to get a pass.

Final commands, adapted to the established project/workspace/feature/profile configuration:

```text
cargo fmt --all --check
cargo check
git diff --check
cargo nextest run --no-fail-fast
```

Run existing required lint/additional checks if `AGENTS.md` requires them; do not invent a new lint regime or upgrade dependencies. A zero-test filter is not a valid gate. Keep durable logs and verify pass/fail/skip totals and relevant default-filter behavior.

Use section 0.1's **10/15/20-minute observation cadence**. Do not run a second command while the first is compiling quietly. After a complete successful run, documentation-only status updates do not require another full suite; code/test/build-input remediation does.

## R5-B: One explicit live acceptance matrix

Record each case as passed, failed, or unverified with the tested build identity. The user reported real behavior; a mocked viewport-command list is not sufficient evidence for these cases.

| Case | Required result |
|---|---|
| H1 | Start with grid hidden/offscreen and stationary mouse; hold opens the radial without showing grid or moving mouse. |
| H2 | Grid focused; short tap hides it. Another short tap shows it. No unfocus workaround. |
| H3 | Both surfaces visible; tap toggles only grid, hold toggles only radial; opening/closing release does not create another action. |
| H4 | Designer focused; shared chord still routes correctly outside explicit binding-capture mode. |
| H5 | Screen Draw active; its established recovery/emergency is immediate and no delayed radial/grid appears. |
| N1 | Root -> child -> grandchild -> Back -> Back maintains the same visible center with different-sized menus. |
| N2 | Repeat after root edge-clamping, deliberate dragging, and moving pointer to a different monitor. |
| N3 | Larger child that cannot safely fit has an explicit safe fallback, not a distant window or lost cells. |
| N4 | Explicit Cascade stays close, with an obvious active wheel and safe Back. |
| V1 | Normal arrow over cells, appropriate drag feedback, no busy pointer left behind. |
| V2 | Long authored and dynamic labels display complete wrapped tooltips after the configured delay without moving the wheel or intercepting clicks. |
| V3 | Repeated preview selection has no truncation-warning flood; a genuine bad asset/save still has an actionable diagnostic. |
| D1 | One independent Designer at about 900×650; grid can remain hidden and all controls/authoring replies work. |
| D2 | Resize/hide panes, collapse sections, change selection/menu/skin, refresh, reopen: preferences persist and nothing auto-expands. |
| D3 | Add a cell from '+', edit an empty slot, right-click properties, drag/swap between rings, create/open a child, breadcrumb Back, undo/redo. |
| D4 | No Design operation executes a real macro/application/action; runtime/native preview remain clearly distinct. |
| D5 | Dirty close, Apply/Cancel, external conflict, native preview startup followed by close, reopen: no lost changes or stranded native windows. |
| P1 | One-time presentation conversion has backup/undo; later explicit Cascade remains after restart. |
| P2 | Menu/skin definitions and imported content remain usable; stable IDs/action refs preserved; existing grid static/follow behavior unchanged. |

Use benign actions/test profiles, not user automation with side effects. Do not automate native tests using real clicks/SendInput against an uncontrolled desktop. When native interaction cannot be performed in the implementation environment, provide a short exact manual checklist and mark the corresponding gate unverified. Do not claim complete native acceptance from unit tests.

## R5-C: Performance and lifecycle checks

Compare repair-start/candidate behavior using the same practical build/profile conditions. A historical branch baseline can remain background context, but it is not a substitute for the immediate pre-repair comparison.

Measure/inspect only the affected costs: idle repaint/wakeup activity with grid hidden, hold-threshold-to-present latency excluding the intentional hold, submenu transition/Back, stationary tooltip timing, Designer responsiveness, and repeated open/close resource counts.

Required architecture: event-driven wakeups; no polling fix; no geometry re-layout because tooltip appeared; no per-hover asset/font IO; no rebuilding whole action catalogs/drafts for every pan pixel; no window recreation per cell/selection; no accumulating subscriptions, HWNDs, preview leases, or undo entries from continuous drag frames.

A deferred viewport, extra cached tooltip layouts, and meaningful event wakes may have some cost. Report actual measurements and remaining uncertainty rather than claiming literal zero overhead from source inspection. Do not launch a broad renderer/benchmark framework rewrite for a repair.

## R5-D: Final review and finish

Use a read-only reviewer for the cumulative repair if available. Provide actual code, tests, new contract, migration, and native evidence. Ask specifically about lost wakeups, focused-hide races, overlapping invocation identities, session-coordinate drift, conversion reversibility, child viewport lifetime, input/close ownership, tooltip overflow, diagnostic classification, and destructive drag/undo behavior.

Resolve substantive findings. Keep unrelated features in a separate backlog. Do not repeatedly reopen completed milestones for imagined enhancements without a concrete unmet requirement or defect. Preserve a concise current-status section and archive detailed old job history below it rather than burying the next action.

After required fixes and verification, commit coherent milestones and report actual hashes. Do not stage user reference archives, temporary extraction trees, local configuration/backups, test logs, or unrelated work merely because they are present.

---

# 3. Definition of done and reporting

This repair is complete only when all approved runtime/editor outcomes have implementation and relevant verification evidence. Code-written is not native-validated.

Checklist:

- Independent shared tap/hold contract works; focused grid hides; hidden grid does not block radial requests.
- Real request/reply delivery wakes the correct service/viewport without polling, showing the grid, or requiring pointer motion.
- Higher-priority Screen Draw/emergency behavior and existing alternate interaction modes remain valid.
- SameCenter uses actual visible/session center across enter/Back/drag; Cascade remains close and opt-in.
- One-time presentation migration is validated, backed up, reversible, and does not overwrite later choices or unrelated data.
- Normal pointer; full readable tooltips; expected truncation quiet by default; real errors actionable.
- Independent compact Designer with one authoring backend; all prior controls available; persistent manual pane/section/view choices.
- Safe slot-first add/edit/drag/submenu authoring; design does not execute or silently overwrite/convert dynamic results.
- Existing Save/Apply/Cancel, undo, references, imports, persistence, and confirmations are preserved.
- Focused gates and complete required Cargo Nextest suite pass, with durable logs/real counts/true exit codes.
- Native acceptance and affected performance/resource checks are truthfully recorded; unverified gates remain labeled as such.
- Review findings addressed, intended commits created, unrelated user work preserved.

Final response sections:

1. **Fixed behavior:** Map each user complaint to the resulting observable behavior.
2. **Architecture changes:** Name actual owners/functions changed and why; distinguish confirmed root causes from remaining hypotheses.
3. **Configuration/migration:** Fields/defaults changed, existing-menu conversion, backup location/undo procedure, and preserved compatibility.
4. **Designer use:** How to open Menus/Skins, add/edit/drag cells, create a child, return, adjust panes, and manage warnings/tooltips.
5. **Tests and native acceptance:** Actual commands/results/counts, build identity, checked scenarios, unrun gates and exact manual steps.
6. **Performance/resources:** Actual affected measurements, not estimates presented as results.
7. **Commits/review:** Real hashes, scope, and substantive findings/remediation.
8. **Remaining issues:** Genuine known issues only. Do not claim no regressions simply because one focused filter passed.

---

# Appendix A — Suggested commit boundaries

```text
fix(radial): decouple invocation from grid activity
fix(radial): preserve visible submenu center and migrate defaults
fix(radial): add full-label tooltips and quiet truncation diagnostics
feat(radial): add compact independent visual designer
fix(radial): close repair acceptance and regression gaps
```

Adapt to actual coherent changes. The last commit is needed only if hardening produced code; do not manufacture work to match the list. A source-audit ledger commit is optional and must preserve the existing historical baseline.

# Appendix B — Resume prompt and job record

Use a compact live status block in the repair ledger:

```text
repair_start_head: <actual SHA + initial dirty state>
current_milestone: <R0..R5>
implementation_status: <in_progress | pending_gate | complete | blocked>
last_verified_source: <actual SHA/diff identity>
current_native_evidence_gaps: <only outstanding scenarios>
job:
  command: <exact>
  cwd: <actual>
  source_snapshot: <commit + tracked/new source identities>
  session_or_process_identity: <actual>
  started_at: <actual local time with timezone>
  log_path: <durable file>
  next_observation_not_before: <10/15/20-minute cadence>
  exit_code: <pending or actual>
next_action: <one concrete step>
```

On resume: read the ledger, reattach to a recorded active job, and respect its next observation time. Do not run a duplicate build, recalculate the original branch baseline, or infer completion from silence. If completion has already been delivered, read the saved result immediately. Preserve the long cadence without changing application timers.

# Appendix C — Source evidence versus API references

**Repository evidence:** `multi_launcher_radial_repair_source_notes.md` identifies the inspected archive and relevant excerpts. The earlier `radial_repair_source_review.md` can be retained as the longer source-audit record. Revalidate current implementations before editing.

**Primary external API references, consulted for integration constraints:**

```text
[W1] egui 0.27.2 Context — request_repaint, request_repaint_of, locking
https://docs.rs/egui/0.27.2/egui/struct.Context.html

[W2] egui 0.27.2 Context — show_viewport_deferred versus immediate viewports,
     callback ownership, stable viewport identity and close handling
https://docs.rs/egui/0.27.2/egui/struct.Context.html#method.show_viewport_deferred

[W3] Microsoft — Using Cursors; WM_SETCURSOR
https://learn.microsoft.com/en-us/windows/win32/menurc/using-cursors
https://learn.microsoft.com/en-us/windows/win32/menurc/wm-setcursor

[W4] Nextest — Reporting test results
https://nexte.st/docs/reporting/
```

These references describe API behavior, not observed fixes in this project. Use the checkout's actual pinned versions. The reported screenshots and local RM4 designer source establish the requested UX reference; they do not prove native event delivery or performance in the new implementation.
