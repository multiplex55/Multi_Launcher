# Multi Launcher — Two Remaining Native Interaction Blockers

## Assignment and scope

Continue on `multiplex55/Multi_Launcher`, branch `radial-menu-2`. The live branch was inspected at `fc9278e15f43d315c91e46ff3f11923308624b41`. Preserve legitimate newer work and the existing immutable feature baseline. Read AGENTS.md and `docs/plans/radial-stabilization.md`.

The user still reports (1) Edit Radial Menus and Edit Radial Skins do not respond to interaction and (2) the focused normal launcher will not hide with its configured hotkey until focus moves elsewhere. Reopen these specific acceptance items. Retain the historical successful automated runs; do not mark the current native failures resolved because 4,599 tests previously passed.

This handoff is a read-only source review and corrective assignment, NOT a patch or native reproduction report. No files were pushed, no application was run, and no Cargo tests were executed during preparation. Do not restart S0–S5, extend skin/import/Cascade features, add another action engine, or request another broad requirements questionnaire.

## 1. Establish which binary and profile are failing before rebuilding

Record the actual running process ID, executable path, start time, SHA-256, and application's effective data/settings directory. Checking only `target/debug` does not establish which process the user is interacting with. The application enforces single-instance ownership per data directory, so launching another executable is not proof it replaced the existing process.

The user's report identifies automated source candidate `c865cf37d2ff1cb98be706450b8bb24b36cff984` and one debug artifact hash:

```text
4AF12CF54F6FA251E24871561E0A720E0C062452C73CE339B204923388169EA0
```

A different hash is not automatically an obsolete binary: a release build or legitimate rebuild can differ. Reconcile its source/build identity and preserve the user's data. Do not delete settings, force-kill unsaved work, or demand another full build before checking identity.

Keep the user's actual `Shift+Alt+Win+End` mapping and all-key-release behavior. Its producer is unknown. No global Windows-key synthesis is necessary for a user-operated diagnostic build: the user can reproduce the physical mapped-key gesture while the application records its own bounded event trace.

## 2. Source issue A — repeated focus requests in a retained deferred callback

In `src/gui/radial_editor/mod.rs::show_deferred`, current code snapshots `viewport_focus_pending`, clears the live field before registration, and captures the old boolean in the `move` deferred callback. That callback sends `ViewportCommand::Focus` whenever its captured boolean is true.

Pinned egui 0.27.2 explicitly permits multiple callback executions while the parent sleeps. The copied boolean therefore does not become false when the live field is cleared. The same retained callback can send Focus repeatedly until replaced. This is a source-level one-shot-delivery problem; its contribution to the user's exact symptoms still needs observation.

Corrective direction:
- Consume the live pending-focus request exactly once inside the actual child callback after confirming the window should remain open and is a supported native viewport.
- Remove the stale captured decision/pre-clear arrangement; preserve intentional reopen/focus requests and wake the correct owner when one arrives.
- Do not constantly focus the Designer or use focus theft as a responsiveness fix.
- Audit `LauncherApp::enforce_pinned -> ensure_open(Panel::RadialEditor) -> editor.open()`. Current `open()` re-arms focus when already open. If Designer is pinned, per-frame ensure-open can become per-frame refocus. Separate idempotent keep-open maintenance from explicit user focus requests. Do not assume this user has pinned it; inspect configuration.

Required regression: run the real retained deferred callback twice without another parent update. One focus request must produce exactly one Focus command. Add repeated pinned-maintenance and a second explicit user-focus case. A test invoking only `viewport_ui` bypasses the defect.

## 3. Source issue B — stale disposable replies can strand the pending slot

Inspect the real production sequence across:
- `radial::authoring::RadialAuthoringSession::mutate` and `accept_reply`;
- `gui::radial_editor::poll_replies`;
- `gui::radial_editor::preview::EmbeddedPreview::sync_preparation`.

In the inspected code, mutation increments draft generation and only prohibits mutation while the initial snapshot is pending. A non-initial disposable request can remain pending during that edit. `accept_reply` returns false if the reply generation differs from the current draft generation, before clearing the matching pending slot. `sync_preparation` returns early whenever that slot is occupied; `request_commit` also rejects pending work.

Write a deterministic production-state regression BEFORE speculative broad fixes:

```text
Loaded authoritative session, generation G
  -> request FontCatalog or embedded preview at generation G
  -> valid document edit increments generation to G+1
  -> receive the old, exactly correlated reply for G
  -> stale payload must not overwrite the new draft
  -> the old completed/cancelled request must not remain pending forever
  -> a fresh preview/save must be able to proceed
```

The inspected UI polling does not independently retire this mismatch; verify current code has not changed. This is a concrete liveness sequence to test, not proof it alone explains a window that never receives clicks.

Corrective direction:
- Preserve separate request/session correlation, result freshness, and operation terminality.
- Cancel/invalidate obsolete disposable work at mutation or retire a matching terminal stale response safely; never clear an unrelated newer request.
- A session-stable font catalog may deserve different freshness semantics from draft-specific preview data. Follow existing ownership rather than accepting every stale reply indiscriminately.
- Do not apply this cancellation rule to already-accepted durable saves/imports. Preserve revision checks and current Save/Apply/Cancel semantics.
- Test two rapid edits, undo/redo, stale reply after close/reopen, mismatched request ID, and delivery failure.

## 4. Classify the real failed click and focused tap

Both menu and skin editing use the same Designer viewport/session path. Diagnose that common path before changing two property panels independently.

The Designer intentionally disables its body while the initial authoritative Snapshot is pending or a conflict is active. Tests using `open_test_snapshot()` skip that production bootstrap. Do not simply remove these safety gates. Record the gate's reason, pending kind/age/session/generation, and where its initial request/reply stops.

Add only bounded opt-in diagnostics sufficient to answer:

### Failed Designer click

```text
Native window under pointer and owner identity
 -> Designer receives pointer down/up?
 -> deferred callback is entered/completes?
 -> widget enabled or blocked (exact reason)?
 -> widget response clicked/changed?
 -> authoring mutation accepted/rejected?
 -> draft generation/state changed and painted?
```

Distinguish missing input, invisible preview/input-host interception, intentional disabled state, blocked/long frame or lock, and a mutation whose result is overwritten/not presented. Observe simple non-document controls such as Tree/Inspector/Zoom as well as a safe rename on an isolated fixture. Do not log note contents, arbitrary keyboard text, or clipboard data.

### Failed focused tap

```text
Configured primary press/release received + provenance/modifiers
 -> one short-tap intent
 -> one desired visibility change
 -> RootViewportCtx command to ROOT
 -> native root HWND actual bounds
 -> any later show/restore/focus operation
```

`RootViewportCtx` is already implemented and used by main and GUI visibility. Do not “add” it a second time or claim its absence is the cause. Offscreen hiding currently moves the root instead of using native Hide: inspect actual bounds, not only IsWindowVisible.

The asynchronous `restore_launcher_to_current_desktop` worker carries no visibility-generation cancellation in the inspected code. Check for a late restore only if the trace shows a hide being undone; do not rewrite the shared activation service based on that possibility alone.

Compare grid focused/unfocused, Designer closed/open, and native preview stopped/active where supported. Preserve Screen Draw emergency/recovery and the working baseline. Do not assume both symptoms share one cause, and do not bypass native security/input policy.

## 5. Bounded delivery, tests, and stop condition

First deliver the two deterministic source regressions and small corrections plus ONE diagnostically useful candidate. Reuse existing tracing where sufficient. Do not add a new general diagnostic platform or run repeated full suites while the actual failure mechanism is still unknown.

If a permitted interactive Windows backend cannot operate the real app, return that candidate with exact launch/profile/log locations and a short manual sequence the user can perform. Finish unblocked deterministic work, but do not spend another unbounded run producing hypothetical native fixes and “cleared” source reviews. Do not describe absent reproduction as proof of success.

Once the two real blockers are reproduced and corrected, run focused regression tests, then the complete required suite on the final source. Completion requires:
- Menu and skin editors accept ordinary clicks/edits and display results.
- Initial snapshot settles or fails visibly with recovery, never silently disables forever.
- Focus is delivered once per explicit request.
- Editing during disposable preparation does not strand pending work.
- The configured mapped tap hides/shows the focused grid without clicking another app.
- No duplicate toggle, stale release, destructive cancellation, or preview/input-host leftovers.

Keep one Cargo/build/Nextest job, durable logs/true exit status, and the existing incremental target. Observe running jobs after 10 minutes, then 15 minutes, then every 20 minutes. Handle actual completion notifications immediately. These are observation intervals, not kill deadlines or application delays. Use coherent focused batches with `--no-fail-fast`; no new top-level test binary per regression and no `cargo clean` workaround.

Final report must distinguish: source defects repaired, exact native failure trace, running binary identity, automated results, user/host observed behavior, and any unresolved evidence. Never convert a historical successful HWND probe into proof that the Designer receives input.

## Evidence locations

All project observations above were inspected through the GitHub connector at:

```text
Repository: multiplex55/Multi_Launcher
Branch: radial-menu-2
Pinned inspected HEAD: fc9278e15f43d315c91e46ff3f11923308624b41

src/gui/radial_editor/mod.rs                 show_deferred, open, poll_replies, viewport_ui
src/gui/radial_editor/preview.rs             sync_preparation
src/radial/authoring.rs                     mutate, request_commit, accept_reply
src/gui/mod.rs                             ensure_open, enforce_pinned
src/visibility.rs                          RootViewportCtx, apply_visibility
src/gui/render.rs                          root restore/update, Designer registration
src/window_manager.rs                      restore_launcher_to_current_desktop
src/window_activation.rs                    activation/foreground retries
src/main.rs                                root context installation, single-instance startup

egui pinned callback contract:
https://docs.rs/egui/0.27.2/egui/struct.Context.html#method.show_viewport_deferred
```

No current native symptom was reproduced during this source review. The issues above are targeted code-level findings and investigative boundaries, not a claim that the exact user failures have already been fixed.
