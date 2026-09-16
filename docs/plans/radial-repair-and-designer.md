# Radial runtime repair and compact Designer ledger

Status: approved follow-up to `docs/plans/radial-menu.md`

Canonical repair contract: `docs/multi_launcher_radial_repair_codex_plan.md`

Approved decisions: `docs/multi_launcher_radial_repair_approved_requirements.md`

Source-review notes: `docs/multi_launcher_radial_repair_source_notes.md`

This ledger tracks only the R0-R5 repair initiative. The historical M0-M6 ledger,
its immutable baseline, and its earlier verification evidence remain in
`docs/plans/radial-menu.md`. Historical evidence is not repair acceptance evidence.

## Live status

| Field | Value |
|---|---|
| repair start branch | `radial-menu-2` |
| repair start HEAD | `eda06f7667e481dd370da413fee9c5d92cfe0334` (`update`) |
| repair start worktree | clean: no staged, unstaged, or untracked changes |
| immutable feature baseline | `0d0acaf471a52f49a7ebdff61879416eefa9fc9b` (unchanged) |
| branch point | `f7c5f61ed2faa2288f5c19ddeaea66de6f760a2b` (unchanged) |
| current milestone | R1 |
| implementation status | pending implementation |
| last verified source | R0 read-only audit at repair-start HEAD; no Cargo/build/test run |
| current native evidence gaps | H1-H5, N1-N4, V1-V3, D1-D5, and P1-P2 are unverified for this repair |
| active Cargo/build/Nextest job | none |
| next action | implement R1 independent chord cycles and owner-viewport queue/wake delivery |

The repair-start Git diff was empty. Repository-local reference archives and images
were not modified. `docs/references/Radial menu v4.zip` was inspected in place as
text only; its center/item drag interactions are UX reference, not reusable source.

## Superseding behavior

The repair contract intentionally supersedes the earlier defaults in these areas:

- the shared chord always resolves tap to grid-only toggle and hold to radial-only
  toggle, including while a radial is opening or open;
- SameCenter is the default and uses the actual visible session center; Cascade is
  an explicit local alternative;
- full-label tooltips default on with a 300 ms delay, ordinary pointer behavior, and
  quiet typed truncation diagnostics;
- authoring moves from the embedded launcher panel to one independent compact
  Radial Designer viewport with safe nonexecuting direct manipulation.

All other historical radial architecture, safety, persistence, and compatibility
contracts remain in force unless the approved repair brief explicitly changes them.

## R0 ownership audit

| Issue | Confirmed current source ownership | Repair milestone and evidence gate |
|---|---|---|
| queued runtime preparation can wait for incidental GUI activity | `src/gui/mod.rs::{register_event_sender,send_event}` owns sender-only delivery; `src/gui/watch.rs` drains on update; GUI-to-main replies already wake main | R1: owner-viewport enqueue-then-wake tests and hidden-grid no-pointer live check |
| active radial is dismissed on the next chord press | `src/radial/invocation.rs::InvocationState` cannot represent an active lifecycle and a new physical cycle independently | R1: full four-state tap/hold truth table, timing boundaries, stale feedback, Screen Draw priority |
| a pending show restore can override a later hide | `src/visibility.rs` sets `restore_flag` on show but hide does not invalidate it; `src/gui/render.rs` consumes restore before visibility reconciliation | R1: show-then-hide-before-frame and focused/unfocused visibility ordering tests |
| submenu drift and distant Cascade | `src/radial/controller.rs::open_submenu` re-samples cursor geometry, reads the child policy, uses requested rather than visible anchor, and `shape_center` reapplies origin | R2: multi-level/negative-origin/drag/Back geometry and native placement checks |
| drag leaves session geometry stale | `src/radial/native.rs` updates HWND surface coordinates without a correlated controller/session relocation | R2: drag-to-child-to-Back session-anchor tests |
| Cascade defaults and one-time conversion | defaults live in radial/settings models and authoring creation; current migration is decode-only and has no presentation receipt/undo | R2: verified backup, receipt, conflict-safe restore, and later-Cascade persistence tests |
| inherited/busy pointer | radial class registration in `src/radial/native.rs` has no arrow cursor or owned-client `WM_SETCURSOR` policy | R3: native adapter plus real hover/capture smoke check |
| incomplete/immediate tooltips and warning flood | narrow label requests are reused in `font_cache.rs`/`preparation.rs`; controller has no hover deadline; typed diagnostics become strings and errors | R3: full-text wrapping/delay/parity/typed-diagnostic tests and V1-V3 smoke checks |
| embedded oversized editor | `src/gui/radial_editor/mod.rs` borrows `LauncherApp`, uses an embedded window/equal columns; `Panel::RadialEditor` participates in root visibility restoration | R4: stable deferred viewport, hidden-root operation, close lifecycle, and compact-layout checks |
| unsafe/missing direct manipulation | authoring transactions and stable IDs exist, but the canvas has runtime-like clicks, no slots/plus/breadcrumbs, and tree moves insert rather than safe swap | R4: pure transform and atomic add/link/move/swap/dynamic-protection tests plus D1-D5 |

Confirmed facts do not prove the reported real Windows behavior. Focused tap-to-hide,
busy-pointer causes beyond cursor ownership, hidden/offscreen eframe wake behavior,
visual center stability, and Designer independence require the native acceptance rows.

## Milestones

| Milestone | State | Deliverable | Gate |
|---|---|---|---|
| R0 | complete | source/ownership audit, repair start identity, and execution ledger | read-only inspection plus documentation diff check; no initial full suite |
| R1 | pending | independent chord cycles, reliable owner-viewport wakeups, authoritative hide ordering | combined invocation/event/visibility Nextest gate and H1-H3/H5 live checks |
| R2 | pending | frozen session centers, local Cascade, drag/Back transforms, reversible one-time conversion | geometry/session/migration gate and N1-N4/P1 live checks |
| R3 | pending | normal cursor, complete delayed tooltips, typed quiet diagnostics | font/preparation/native/preview gate and V1-V3 live checks |
| R4 | pending | one independent compact Designer and safe direct manipulation | editor/authoring/lifecycle gate and H4/D1-D5 live checks |
| R5 | pending | cumulative regression, performance/resources, native matrix, and independent review | complete required Nextest suite and final acceptance record |

Each write-heavy milestone has one writer, is verified and diff-reviewed before its
coherent commit, and completes before the next writer begins. Independent review is
read-only and may not start a Cargo/build/test job.

## Long-job record and observation contract

At most one Cargo/build/Nextest job may use the shared checkout/target directory.
Every long job record must include its exact command, cwd, commit plus dirty/new-file
identity, environment/profile, start time, persistent session/PID identity, durable
stdout/stderr log, next observation time, and true exit code.

For a still-running job, first observe at about 10 minutes, then 15 minutes later,
then every 20 minutes. Completion/failure notifications are handled immediately.
Silence is not a timeout; reattach to the recorded job and never launch a replacement
while it remains healthy. Small synchronous commands need no artificial delay.

Current job: none.

## R1 implementation packet

Objective: independently represent the active/opening radial lifecycle and the next
physical chord cycle so one cycle emits exactly one admitted grid or radial outcome.
Make GUI event enqueue and owner-viewport wake one disposable delivery boundary, and
make a hide transition invalidate stale root restoration.

Architectural owners and required scope:

- `src/radial/invocation.rs`: typed radial lifecycle plus physical-cycle reducer;
- `src/hotkey/launcher_invocation.rs`: physical ownership, provenance, repeats,
  modifiers, lifecycle feedback, Screen Draw and direct-trigger preservation;
- `src/gui/mod.rs`, `src/gui/watch.rs`, `src/gui/radial_actions.rs`: disposable
  sender/wake subscription, enqueue-before-wake, root viewport targeting, symmetric
  reply/invalidation wake behavior;
- `src/visibility.rs`, `src/gui/render.rs`, `src/main.rs`: authoritative hide and
  stale restore ordering without changing genuine show placement;
- existing reducer, adapter, controller, Screen Draw, visibility, and GUI event tests.

Required behavior and invariants:

1. Press changes neither surface; release before threshold toggles only the grid.
2. Deadline or a delayed release at/after threshold emits the admitted radial-only
   open/cancel-close action once, and its release drains without a tap.
3. Admission captures open versus close while preserving the existing session. Old
   lifecycle feedback cannot erase a newer pending chord, and close during preparation
   invalidates its late reply.
4. Only the original release-sensitive opening cycle may select on release. A later
   sticky tap release is not forwarded to the old menu.
5. Screen Draw/emergency, direct triggers, alternative modes, provenance, modifier,
   repeat, binding-capture, and shared-mode-disabled contracts remain intact.
6. Successful event enqueue is followed by a wake of its explicit viewport after all
   registry locks are released. Registration/attachment wakes queued work and dead
   subscriptions do not retain test/application contexts.
7. Root work wakes `ViewportId::ROOT` without Show/Focus/position commands. No polling,
   synthetic input, repaint loop, or duplicate listener is introduced.
8. Hiding clears/invalidates stale restore state and sends the established hide/offscreen
   command once. Genuine show retains configured placement; valid internal restore
   retains current geometry.

R1 tests must replace obsolete immediate-dismiss assertions and cover the full four-
state truth table, threshold boundaries, delayed preparation, Esc during pending
open/close, stale lifecycle feedback, reload/failure/shutdown, direct-only behavior,
Screen Draw recovery, modifiers/provenance, queue ordering/bursts/attachment/disposal,
and focused hide ordering. Direct preparation calls do not prove queue-and-wake.

R1 non-goals: submenu geometry, tooltip/cursor behavior, Designer construction, or
moving GUI-owned preparation/action execution to another thread.

R1 is done only after the focused source-identical gate passes, the diff contains no
legacy bypass or polling workaround, H1-H3/H5 are recorded as passed/failed/unverified,
and a coherent repair commit exists.
