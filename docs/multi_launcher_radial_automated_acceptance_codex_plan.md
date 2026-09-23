# Multi Launcher — Automated Radial Acceptance and Recovery
## Approved Codex implementation/remediation brief

**Status:** Approved. The user selected every recommended option and said **Proceed**. Do not repeat the requirements questionnaire.  
**Current source inspected:** `launcher.zip`, SHA-256 `b5e1df91f2dcab48618f0f9e29ae827c4048eb3404c5892fce92008e2e085f5a`.  
**Implementation authority:** The actual current checkout. Preserve legitimate changes newer than the inspected archive.  
**Current branch/ledger context:** the archive's `docs/plans/radial-input-recovery.md` records the prior input-recovery packages as implemented, final automated Nextest as 4,635 passed / 8 skipped, and the real Windows interaction matrix as still unverified.  
**Primary goal:** stop using manual user key presses/click descriptions as the correctness oracle. Build an automated acceptance harness that drives the production input/UI paths, then use it to repair the focused-launcher and dead-Designer failures until the complete basic radial authoring workflow passes automatically.

This is a code-first implementation plan. It is not a request for another broad radial redesign, another exhaustive audit, or another unit-test-only completion report.

---

# 0. Non-negotiable execution rules

Read `AGENTS.md`, `docs/plans/radial-input-recovery.md`, `docs/plans/radial-stabilization.md`, the current source, and the companion source notes before editing. Inspect the live checkout to account for changes after this archive.

Preserve the existing immutable radial feature baseline. Record a separate:

```text
AUTOMATED_ACCEPTANCE_START_HEAD
initial branch
initial staged/unstaged/untracked state
current input-recovery ledger state
```

in a compact new execution ledger, preferably:

```text
docs/plans/radial-automated-acceptance.md
```

Do **not** rewrite the old stabilization/input-recovery ledgers as if their reported tests never happened. They are historical evidence. Equally, do not use their green counts to mark the new native acceptance goal complete.

## 0.1 Code first; very light intermediate test execution

The user explicitly wants more implementation up front and much less repetitive testing.

Use this cadence:

1. Inspect the narrow current owners required by the milestone.
2. Implement a coherent architecture/work package.
3. Run `cargo check` at meaningful integration points.
4. Continue implementing the next coherent package.
5. Once the automation architecture is integrated, run **one small combined headless/focused batch if it will materially reduce risk**.
6. Build the actual launcher plus native acceptance runner.
7. Run the automated native acceptance workflow. Use its failures to remediate the production code automatically rather than asking the user to test it.
8. Near completion, run the complete required `cargo nextest run --no-fail-fast` once for the final candidate. Relevant final remediation invalidates that full-suite result and requires a replacement final gate.

Normal compile checkpoint:

```text
cargo check --lib --bin multi_launcher --bin radial_acceptance
```

If the exact target names differ in the current manifest, discover them first and adapt. Do not add `--all-targets`, `--all-features`, benches, or unrelated targets reflexively.

`cargo check` is the development compile/type gate. It is **not** proof that a current linked executable exists, that native UI works, or that tests pass.

Build the source-matched native candidates when the acceptance harness is ready:

```text
cargo build --bin multi_launcher --bin radial_acceptance
```

Do not run Nextest after each helper, UI control, or native adapter correction. Write the relevant tests alongside code, but execute them in coherent batches later.

## 0.2 Slow-machine observation schedule

Only one Cargo/build/Nextest process tree may use the shared checkout/target directory at once. Read-only reviewers must not launch independent builds.

For a still-running job:

| Observation | Wait from launch/previous scheduled observation |
|---|---:|
| First | about 10 minutes / 600 seconds |
| Second | about 15 minutes / 900 seconds |
| Later | about 20 minutes / 1,200 seconds |

Actual completion/failure notifications take precedence: process them immediately. These intervals are **observation cadence**, not kill deadlines, not application timers, and not unit-test sleeps.

Before a long job, record command, cwd, source SHA/diff identity including untracked task source, target/profile/features, start time, persistent session/PID identity, durable stdout/stderr log, exit-code record, and next observation time. Reattach to the same job. Do not duplicate a quiet job, run `cargo clean`, alter toolchains/profiles, or use repeated short process/log polls.

Use `--no-fail-fast` for the final Nextest gate and for any deliberately combined focused batch where supported. Do not use `--no-capture` merely for progress; keep durable logs instead.

---

# 1. Accepted product and testing contract

The next Codex goal is:

> **Create an automated acceptance framework that exercises Multi Launcher from deterministic tap/hold classification through actual native ROOT visibility and actual Designer widget interaction. Use it to identify and repair the unresolved focused-hotkey and noninteractive-Designer failures, then automatically validate the basic radial authoring workflow against an isolated deterministic profile and, when supplied, a copied user/test profile. Manual key pressing and manual clicking are not required for functional sign-off.**

## 1.1 Why this is the correct next boundary

The current archive already contains:

- the dedicated shared tap/hold reducer and low-level Windows hook service;
- deterministic chord/release tests, including admitted externally injected input;
- explicit ROOT viewport targeting;
- an independent deferred Designer viewport;
- bounded acceptance tracing for input, Designer body/widget edges, requests, ROOT commands, native window snapshots and pointer events;
- AccessKit output/tests in the Designer;
- Win32 UI Automation support elsewhere in the repository (`windows` already enables `Win32_UI_Accessibility`);
- a visual-first Designer with New Menu / Add Ring / Ring / Slots and geometry proposal code;
- prior full automated suite evidence.

The unresolved gap is that the tests do not prove this complete production chain:

```text
native key/mouse input
    -> low-level hook or winit/eframe
    -> egui input
    -> production widget/tap-hold response
    -> main/authoring mutation
    -> actual root/Designer/native window result
```

Do not respond by adding more pure reducer cases while leaving the missing boundary untested.

## 1.2 Accepted three-layer automation model

### Layer A — deterministic exact-chord tests

Drive the exact configured user chord semantics without global OS input:

```text
Shift down
Alt down
Win down
End down
End up
Win up
Alt up
Shift up
```

with a fake monotonic clock around the configured hold threshold.

Cover tap, hold, repeats, full release, active runtime radial, no co-fire, lifecycle cancellation, and emergency/Screen Draw precedence. The existing adapter tests already cover much of this. Consolidate/extend rather than duplicate.

This layer proves the exact `Shift+Alt+Win+End` state-machine contract safely and deterministically. It does **not** prove Windows hook delivery or root-window behavior.

### Layer B — headless production Designer interaction

Run the **actual Designer UI path** using an egui `Context` and real `egui::RawInput` / pointer / keyboard events. Do not build a fake second Designer model.

The harness must be able to:

1. render the production Designer for a frame at a controlled client size/DPI;
2. enable AccessKit output;
3. find a production widget semantically (accessible name/role) and obtain its current bounds;
4. inject pointer move/down/up at that widget's center through subsequent frames;
5. inject text/key/Tab events;
6. assert the actual `RadialEditorState`/authoring session and next rendered frame changed;
7. exercise bootstrap/readiness rather than relying only on `open_test_snapshot()` shortcuts for every interaction test.

This layer proves the egui/authoring path while remaining fast and deterministic. It does not prove the native deferred HWND receives Windows client input.

### Layer C — opt-in native Windows acceptance runner

Add a separate binary/driver, expected target name:

```text
radial_acceptance
```

It launches a **real source-matched `multi_launcher.exe`** in an isolated profile and drives the real Windows/input/UI boundaries automatically.

It is explicit/opt-in and is **not** a normal unit-test/Nextest test. It may focus/move/click only its own isolated test windows and should restore prior cursor/focus when practical.

It uses:

- Win32 `SendInput` for a safe isolated-profile native chord and at least one actual Designer pointer/Tab path;
- Windows UI Automation (through the repository's already-enabled `Win32_UI_Accessibility`) for semantic control discovery and invocation where available;
- process/HWND inspection for actual root/Designer/native window state;
- the existing bounded acceptance trace as diagnostic evidence, extended only where an actual required edge cannot otherwise be observed;
- automatic failure screenshots and a machine-readable report.

Do not add a new external UI automation dependency unless current Windows bindings are demonstrably insufficient. There are already UIA patterns in `mkmacro::uia`, browser-tab enumeration, and system actions to use as implementation reference. Avoid coupling the acceptance domain to MkMacro-specific selectors if a small shared/driver-local wrapper is clearer.

---

# 2. Safety and isolation rules for native automation

## 2.1 Never mutate the user's real profile

Default native acceptance creates a temporary profile directory containing deterministic settings and starter radial data.

The current application derives its data root from the **current working directory's `settings.json`**. Therefore the runner can isolate a child without changing ordinary production startup semantics:

```text
TempDir /
    settings.json
    radial.json
    radial_assets/ (only if needed)
    acceptance.log
```

Start `multi_launcher.exe` with its child process current directory set to that temporary directory.

This also gives the child a distinct single-instance identity because the application mutex is rooted in the normalized data directory. Do not disable the single-instance mechanism globally.

### Deterministic profile

Create the profile using current serialized Rust types/fixtures rather than hand-maintaining an obsolete JSON blob. At minimum:

- `Settings::default()` as the starting schema;
- safe native acceptance hotkey such as `F12` (unless it conflicts with the live current defaults; detect rather than assume);
- `help_hotkey = None` or another non-conflicting choice;
- radial enabled;
- shared tap/hold enabled;
- valid hold threshold (normally preserve the current default, about 350 ms; no need to reduce it for speed);
- debug/file logging configured inside the temp directory;
- predictable static/follow-mouse settings appropriate for window assertions;
- `RadialDocument::starter()` serialized using the current schema.

Validate the created settings/radial document through current validators before launching. Do not invent a relaxed “acceptance config” parser.

### Optional copied-profile pass

Support an explicit argument such as:

```text
--profile-copy <directory>
```

or equivalent.

Copy the supplied source directory to a new temporary directory first. Never run acceptance with its CWD pointing at the source profile. Reject/handle symlinks/reparse points safely according to existing project path conventions. Record hashes/identities of copied `settings.json` and `radial.json` before launch.

The deterministic fixture is mandatory. Copied-profile compatibility is required when the caller supplies a valid profile path.

## 2.2 Safe native chord policy

Do **not** globally synthesize the user's real `Shift+Alt+Win+End` chord by default.

Use two complementary tests:

1. exact real chord at the adapter/reducer layer using fake time;
2. a harmless profile-specific chord (recommended `F12` or another validated non-shell chord) through `SendInput` in native acceptance.

The current `InvocationConfig` sets `accept_external_injected: true`, so ordinary `SendInput` events with no Multi Launcher self-injection tag should be classified as externally injected and traverse the production hook. Verify this in the current checkout instead of introducing a special “accept acceptance-runner input” bypass.

Do not tag acceptance input with the application's self-injection tag; self-injected input is intentionally excluded from invocation ownership.

Before native injection, check current key state and ensure the runner is not accidentally holding modifiers. Microsoft documents that `SendInput` does not reset already-held keyboard state and can be blocked by UIPI when integrity levels differ. Treat returned event count/failure as test evidence, not as a silent retry loop. [W1]

## 2.3 Mouse/focus containment

The native runner may temporarily focus its own test root/Designer and move/click the cursor within those windows. It must:

- capture prior cursor position and foreground HWND;
- not click arbitrary coordinates without first resolving the target window/control and validating its process ID;
- bring the isolated test window to foreground only as required by the case;
- restore cursor and prior foreground window when practical after the run;
- never dispatch a real Designer leaf action;
- terminate/close the isolated child and remove temp data on success;
- retain failure artifacts in the output directory while cleaning the temporary application profile unless an explicit `--keep-profile-on-failure` option is requested.

No acceptance case should type into whichever unrelated application happens to be foreground.

---

# 3. Work Package A — automation primitives and deterministic Designer driver

## Objective

Create the reusable test/runner primitives first so every later production correction can be reproduced automatically. Do not spend this package debugging the actual focused/root failure manually.

## A1. Establish an acceptance domain without duplicating production behavior

Recommended organization (adapt to current ownership; exact helper filenames are implementation choices):

```text
src/bin/radial_acceptance.rs          native orchestrator entry point
src/radial/... or src/platform/...    narrow shared data/report/native helpers only where reusable
src/gui/radial_editor/mod.rs tests    headless production Designer driver/tests
```

Do not expose the whole `LauncherApp` or `RadialEditorState` publicly just to satisfy the binary. Keep production state private where possible.

The native binary should orchestrate through public/OS-visible boundaries:

- start process;
- discover windows;
- use UIA/SendInput;
- inspect persistence files after clean shutdown;
- parse the child's trace/log output;
- write reports.

The headless Designer driver belongs next to the Designer implementation, where it can exercise private production methods under `#[cfg(test)]` without broadening the public library API.

If one small read-only acceptance API is genuinely necessary, expose the narrowest typed boundary and document why it does not bypass the behavior being tested.

## A2. Headless production Designer driver

Use egui 0.27's real `Context::run`/`RawInput` frame loop. The current repository already uses `ctx.run`, `begin_frame`, AccessKit updates, and keyboard events in tests. Build on those patterns. [W2]

Create a test helper that retains one `egui::Context` across frames. Do not create a fresh Context for every event because input/focus/widget memory is frame-to-frame state.

A typical semantic click sequence should be:

```text
Frame N:
  render actual viewport_ui with AccessKit enabled
  locate node by expected accessible name + role
  save node bounds

Frame N+1:
  RawInput: PointerMoved(center)
  render actual viewport_ui

Frame N+2:
  RawInput: PointerButton(primary, pressed=true, pos=center)
  render

Frame N+3:
  RawInput: PointerButton(primary, pressed=false, pos=center)
  render
  assert actual authoring/UI state changed

Frame N+4:
  render idle
  assert mutation is visibly represented / accessible state changed
```

Use the actual egui event shape for 0.27 after inspecting pinned signatures. Ensure screen/client coordinates and `pixels_per_point` are consistent. Do not add private magic coordinates for each control.

### Semantic lookup

Prefer AccessKit node role/name/bounds from the actual `FullOutput.platform_output.accesskit_update`. The project already exposes names for many Designer controls.

If a required control has no useful accessible name, **fix its production accessibility label** as a legitimate product improvement, then drive it semantically. Do not teach the harness opaque widget IDs that a user/assistive technology cannot observe.

Maintain a small map of required semantic controls, not a general-purpose browser automation framework.

### Keyboard focus

Inject `Tab` through `egui::Event::Key` and verify the actual accessibility focus changes to another eligible Designer control. Also test Shift+Tab if straightforward. Do not infer keyboard support merely because a `TextEdit` can be directly mutated in a unit test.

### Bootstrap/readiness

Existing direct snapshot tests remain useful, but add at least one production-like headless sequence where an initial snapshot request is pending, the matching reply is accepted through the real editor poll/authoring boundary, the body transitions from `InitialSnapshot` to `Enabled`, and then an injected click is accepted.

Do not remove the `InitialSnapshot`/conflict protection to make the test pass.

## A3. Deterministic exact chord regression

Consolidate existing adapter coverage into a clear table-driven test for the user's exact chord:

```text
Shift+Alt+Win+End
```

Use external-injected and physical provenance where appropriate. Verify:

- tap release at threshold-1 -> exactly one grid toggle and deadline cancellation;
- deadline at threshold while primary remains held -> exactly one radial toggle/open/close according to active state;
- release after hold -> no grid toggle;
- repeated primary down -> no duplicate deadline/action;
- all modifier releases drain normally;
- a fresh cycle works immediately after full release;
- active radial + short tap affects grid only;
- active radial + hold affects radial only;
- cancellation/recovery does not create a delayed tap;
- Screen Draw/exclusive owner retains priority.

Do not put Windows sleeps in these tests; use the reducer's timestamp/deadline boundary.

## A4. Machine-readable acceptance result schema

Define a bounded serializable report owned by the native runner, e.g. conceptually:

```text
AcceptanceReport {
  schema_version,
  run_id,
  candidate,
  environment,
  profile,
  cases: Vec<AcceptanceCaseResult>,
  artifacts,
  cleanup,
}
```

Each case should carry:

```text
id
status: passed | failed | skipped | unsupported
started/elapsed
expected summary
observed summary
failure_stage (typed/small enum where possible)
artifact references
```

Do not put note contents, clipboard contents, arbitrary window titles, or secrets into the report. Reuse the privacy principles of `radial::acceptance_trace`.

Record candidate/environment facts:

- launcher executable absolute path and SHA-256;
- runner executable SHA-256 if practical;
- current source/commit identity supplied by orchestration/ledger;
- Windows version;
- process ID;
- temp data root;
- settings/radial file SHA-256;
- configured acceptance chord/threshold;
- monitor count/work areas/DPI or scale values relevant to the run;
- acceptance log/report/screenshot paths.

## A5. Compile checkpoint

After the headless driver, report schema, and runner skeleton compile coherently:

```text
cargo check --lib --bin multi_launcher --bin radial_acceptance
```

Do not run the full suite yet.

**Package A done:** the test infrastructure can drive the actual Designer in-process and compile an opt-in native runner, but no claim about the current native bug is required yet.

---

# 4. Work Package B — native acceptance runner through the real Windows stack

## Objective

Launch a real isolated Multi Launcher, drive real Windows input/UIA, observe actual HWND state and production trace events, and produce useful failure localization automatically.

## B1. Candidate discovery and startup

The runner should accept explicit candidate path when provided:

```text
radial_acceptance --launcher <path-to-multi_launcher.exe>
```

Default may be the sibling `multi_launcher.exe` beside the runner when unambiguous. Never silently run an older PATH-installed copy.

Before launch:

1. verify the launcher exists and hash it;
2. create/validate the isolated profile;
3. configure its log path to an absolute path under the run output/temp area;
4. set `MULTI_LAUNCHER_RADIAL_ACCEPTANCE_TRACE=1` in the child environment;
5. preserve ordinary `RUST_LOG` behavior unless a narrow trace filter is needed; do not disable existing logs;
6. set child `current_dir` to the isolated profile;
7. spawn the child and record PID/start time.

Do not modify production startup to add a second data-root owner unless CWD isolation proves insufficient. Current startup intentionally derives `AppDataRoot` from `settings.json` in CWD.

## B2. Find and verify ROOT

Find top-level windows belonging to the child PID using Win32 enumeration/UIA. Identify the root by the known production title `Multi Lnchr` and process ownership, not title alone.

Wait using a bounded condition/deadline until ROOT exists and is ready. Avoid a giant fixed startup sleep. Record timed-out stage precisely.

Verify:

- HWND belongs to child PID;
- root has sane nonzero client/window bounds;
- UIA root/descendants are queryable if UIA is expected;
- root can be focused for the focused case;
- no other process's window can be selected as a fallback.

## B3. Drive the safe native tap through SendInput

Configure the temp profile's launcher chord to the accepted safe chord (recommended F12 unless validation/conflict rules reject it). Use Win32 `SendInput`, leaving `dwExtraInfo` as ordinary external injected input rather than Multi Launcher's self-injection tag.

For a tap:

```text
primary key down
short deterministic dwell well below hold threshold
primary key up
```

The native runner may use short real timing here because it is explicitly testing the OS timer/hook behavior; unit/headless tests must remain fake-time based. Use margins generous enough to distinguish tap from hold without making the suite slow.

Assert both internal and external evidence when possible:

- child acceptance log contains the configured-primary/short-tap chain;
- desired visibility changes once;
- a ROOT command is issued;
- actual root HWND reaches the expected visible/hidden state/bounds within a bounded deadline.

The current hide path may combine parking with `Visible(false)`; assert the actual product contract using current native state, not only an AtomicBool.

### Required tap cases

- ROOT focused -> tap hides.
- ROOT hidden -> next tap shows.
- ROOT visible but another acceptance-owned window focused -> tap hides.
- Designer focused/open -> tap toggles ROOT only and leaves Designer open/interactable.

The original user requirement is that ordinary main-window focus must not suppress the short tap. The safe native chord tests the same production hook/adapter/ROOT path without shell side effects.

## B4. Drive a real hold

Send safe primary down, keep it held past the configured threshold plus a small scheduling margin, then release.

Prove:

- no grid tap occurs;
- runtime radial enters the expected open/toggle state;
- release does not co-fire grid visibility;
- a second full hold toggles/closes runtime according to the approved contract;
- Designer/root remain intentionally unaffected.

Use an observable runtime-window/session boundary. Prefer an existing controller/native lifecycle signal. If the current acceptance trace lacks any way to prove runtime open/closed, add one **narrow typed lifecycle event** at the authoritative controller/native boundary. Do not infer success solely from “there was no ShortTap.” Do not build a new telemetry framework.

## B5. Open Designer semantically

Use production UI entry points, not direct mutation of `RadialEditorState` from the runner.

Preferred native flow:

1. focus ROOT;
2. locate `File` / `Apps` / `Edit Radial Menus` through UIA and invoke them; or use another visible production semantic entry if current UIA exposes it more robustly;
3. verify a separate top-level `Radial Designer` HWND owned by the child appears;
4. wait for body readiness through UIA/trace, not an arbitrary long sleep.

The runner may use pointer fallback for a menu item if UIA cannot invoke an egui popup reliably, but it must first resolve the target element/bounds to the child process. Do not add a hidden “open Designer for acceptance” state mutation unless all production-semantic avenues are shown impossible. If a CLI/acceptance start action is eventually necessary, route it through the existing typed `RadialCommand::Edit/Skins` command host and separately retain a test of the normal UI entry point.

Repeat for `Edit Radial Skins` or open the same Designer and switch to Skins through its real mode control, matching current product behavior.

## B6. Native semantic UIA wrapper

Implement only the small UIA subset needed by acceptance:

- initialize COM on runner thread;
- get root element/from HWND;
- query descendants by process ID/name/control type;
- read name/control type/bounds/enabled/focus state;
- invoke `InvokePattern` where available;
- use `SelectionItem`, `Toggle`, `Value`, or `SetFocus` only for required controls;
- bounded waits/retries around window UI publication;
- useful error mapping.

The repo already contains working `IUIAutomation`/pattern examples. Reuse conventions and `windows` 0.58 signatures. Do not refactor the full MkMacro UIA subsystem merely to share 50 lines with acceptance.

If an egui control lacks a semantic UIA node/pattern required for automation, first check its AccessKit role/name. Correct the production accessibility metadata where appropriate. Accessibility quality is part of the user-facing UI, not test-only scaffolding.

## B7. Native pointer proof of Designer client input

UIA Invoke alone is not sufficient because the reported failure is specifically that mouse/keyboard interaction does not register.

Choose one harmless real control with a visible state transition and safe semantics—e.g. `Tree`, `Inspector`, Menus/Skins mode, or another non-destructive toggle.

Sequence:

1. UIA locates the target and returns current screen bounds.
2. Assert center lies inside the Designer window/client area and target process.
3. Save cursor position.
4. move cursor to target center;
5. use `SendInput` mouse down/up;
6. wait for production evidence:
   - Designer callback/pointer edge;
   - body is `Enabled` rather than loading/conflict;
   - expected widget response accepted;
   - next UIA state/visibility reflects the toggle.
7. restore cursor.

This case localizes the currently dead Designer client area automatically:

```text
No native click delivered / wrong target
    -> Win32 targeting/input failure
Designer window receives native click but no DesignerPointer
    -> winit/eframe/deferred viewport input boundary
DesignerPointer exists, body blocked
    -> readiness/conflict/request state
body enabled, no widget response
    -> egui layout/layer/hit target
widget accepted, no mutation/state change
    -> handler/authoring bridge
mutation exists, next UI state unchanged
    -> repaint/publication/render state
```

The runner should encode this as a failure-stage classification where evidence supports it. It must not ask the user which one happened.

## B8. Tab/focus proof

Bring Designer foreground. Resolve currently focused UIA element if any. Send a real Tab key down/up through `SendInput`. Assert within a deadline that focus moves to another eligible UIA/AccessKit-backed child control in the Designer process.

If Tab cannot be observed because the native UIA provider does not publish keyboard focus for a specific widget class, use the existing trace/accessibility focus evidence plus at least one text/edit keyboard case. Document the exact limitation; do not silently mark Tab passed because a headless test passed.

## B9. Automatic failure artifacts

For every failed native case, automatically retain:

- `report.json` and concise `report.txt`;
- child's acceptance/log output;
- runner log;
- current process/window inventory limited to test child/runner identities;
- relevant acceptance-trace excerpt around the case;
- screenshot of the target test window/monitor area;
- current test `settings.json`/`radial.json` hashes, but do not copy potentially sensitive copied-profile contents into a public report.

Use the already-declared `screenshots` dependency or existing capture helpers where practical. Crop to the test window/work area using verified HWND/monitor coordinates. Pixel-perfect image comparison is not required.

On success, report case timings and cleanup status but avoid keeping unnecessary screenshots/temp user-data copies by default.

## B10. Cleanup

Ask the child to close through a normal production close operation when possible. Wait for process exit. If it refuses after a bounded timeout, record cleanup failure, then terminate the isolated process so automation does not strand it.

Close COM/resources, restore pointer/focus where practical, and delete temporary deterministic/copied profile. Preserve reports/logs outside the profile.

## B11. Compile/build checkpoint

After native runner integration:

```text
cargo check --lib --bin multi_launcher --bin radial_acceptance
cargo build --bin multi_launcher --bin radial_acceptance
```

Do not run the full Nextest suite yet.

Then run the native acceptance runner on the deterministic profile. This is the first authoritative automated test of the currently failing Windows boundaries.

**Package B done:** the runner can automatically prove or localize focused ROOT tap, native hold, real Designer pointer input, Tab/focus, and clean close on an isolated real process.

---

# 5. Work Package C — use the harness to repair the actual failures

## Objective

Do not stop after building diagnostics. Run the harness, identify the first missing boundary, correct the production owner, and rerun until the required cases pass.

## C1. Focused launcher remediation loop

For a failed focused tap, use the automated evidence in this order:

1. Did `SendInput` insert the event count requested?
2. Did the low-level hook record externally injected configured primary down/up?
3. Did modifier/primary matching admit the cycle?
4. Did a `ShortTap` intent occur exactly once?
5. Did desired ROOT visibility change exactly once?
6. Did `RootViewportCtx` issue the intended ROOT commands?
7. Did the actual root HWND change visible/hidden state?
8. Did a later restore/activation command reverse it?

Correct the **first missing/incorrect owner** only.

Do not:

- add a second focused-only hotkey listener;
- bypass the shared tap/hold adapter while ROOT has focus;
- synthesize Escape/unfocus before hiding;
- permanently repaint or poll;
- delete legitimate restore logic globally;
- disable keyboard input in ROOT simply to make the hook win.

If an input consumed by egui prevents low-level hook delivery, prove that from the hook trace before changing keyboard routing. If the hook gets the event but native root stays visible, do not rewrite the reducer.

Repeat cases with Designer closed and focused/open because root focus must not be a special failure condition and Designer must not steal root-target commands.

## C2. Designer remediation loop

Use the native click case's first absent edge:

### A. Native/UIA target absent or disabled

Audit deferred viewport creation, window styles, enabled state, modal ownership, transparent/no-activate flags, overlay/input HWND ownership, and whether another process/window is covering the client area. Do not change all styles speculatively.

### B. Native click target is Designer but `DesignerPointer` is absent

Audit winit/eframe deferred viewport input routing and whether another local/native input host covers or intercepts the client area. Compare title-bar/border operation (which Windows owns) with client-area routing. Validate actual HWND/class/process/region at the click point.

### C. `DesignerPointer` present but `DesignerBody` is `InitialSnapshot`/`Conflict`

Repair the request/readiness lifecycle that is holding the body read-only. Keep the protection. Ensure the actual error/retry state is visible and automatable. Failed/late requests cannot leave the body permanently blocked.

### D. body `Enabled`, no `DesignerWidget`

Audit actual pane/canvas layer order, clipping, sense/hit rectangles, disabled `Ui`, transparent overlays, modal Areas/Windows, and coordinate transforms. Use the headless semantic test to reproduce the same widget and size.

### E. widget accepted, no mutation

Fix the real click handler/post-render/authoring mutation. Preserve stable IDs/undo/errors.

### F. mutation accepted, next UIA/visual state unchanged

Fix repaint/wake/publication/state selection. Do not apply the mutation twice.

After each coherent production correction, use `cargo check`. Do not run a full test suite between each branch of this investigation. Rebuild once a new native candidate is ready and rerun the automated acceptance cases.

## C3. Close behavior

The runner should also expose whether clean Designer X/close is delayed by a pending disposable/durable request. Use the existing typed close intent rather than replacing it.

Automate:

- clean idle close;
- dirty close prompt via UIA (Keep Editing, then later safe Save/Discard as fixture permits);
- close during disposable preview preparation;
- no late reopen.

Do not intentionally interrupt a durable save halfway through to make close look instant. If a real durable transaction is pending, the UI/runner should observe its state and safe terminal outcome.

**Package C done:** the currently reported root-focus and Designer-input failures are no longer reproducible by the automated native runner in the deterministic isolated profile.

---

# 6. Work Package D — automate the complete basic authoring workflow

## Objective

Expand the now-working native harness to prove the actual user workflow, not only one toggle/click.

Use semantic UIA first and pointer/keyboard where the interaction itself is the behavior being tested.

## D1. Required workflow

Run this on a fresh deterministic profile:

```text
Open Radial Designer in Menus mode
-> New Menu
-> Add Ring
-> select Ring
-> change Slots
-> generate geometry proposal
-> verify preview/proposal state
-> Apply proposal
-> choose an empty cell
-> open compact properties
-> search Universal Actions
-> select an action beyond the historical unfiltered first-50 boundary
-> Open in Inspector and verify it actually appears/selects the cell
-> change one harmless skin/style setting
-> switch/use Skins mode as appropriate
-> start safe embedded/native preview as supported
-> verify Design clicks did not execute the action
-> Save
-> close Designer
-> reopen
-> verify menu/ring/slot/action/skin values persisted
-> perform one edit then Undo/Redo and verify state
```

Choose a harmless assignable action fixture that has no external side effect if it were accidentally invoked, while still asserting that Design mode dispatch count/history remains zero.

Do not create a destructive action merely to prove confirmation.

## D2. Semantic assertions

For each meaningful step, prove both:

1. user-facing semantic state changed (UIA/AccessKit name/value/selection/bounds where applicable);
2. durable/model state changed only when the operation is supposed to commit.

For Save/reopen, stop inspecting in-memory state and parse/reload the temporary profile's actual persisted `radial.json` using current model/store decoding. Verify stable IDs and selected values.

Do not compare entire JSON files byte-for-byte when nondeterministic/unrelated metadata may change. Assert the intended typed fields and graph relationships.

## D3. Geometry proposal

Exercise at least:

- new outer ring proposal;
- slot grow proposal;
- populated shrink that requires explicit resolution and is cancelled once before an accepted resolution;
- stale proposal protection if cheaply automatable.

Do not require dozens of ring-count cases in native acceptance. The geometry math remains covered in fast pure tests. Native acceptance proves the UI workflow is reachable and correctly applies one representative safe candidate.

## D4. Menus and Skins entry points

Prove both user-facing entries:

- Edit Radial Menus opens interactive Designer in expected mode;
- Edit Radial Skins opens/focuses the same authoritative Designer backend in Skins/resources mode without creating a competing draft/window.

Switching modes must not lose the current draft or make client input stop responding.

## D5. ROOT/Designer coexistence

With Designer open/focused:

- safe native tap hides ROOT only;
- next tap shows ROOT;
- Designer remains open and interactive;
- no Designer focus loop continuously steals focus after explicit ROOT targeting;
- hiding ROOT does not stop Designer service/reply progress.

This directly covers the user's two unresolved issues in the same real process.

---

# 7. Copied-profile compatibility pass

When the user/caller provides a path to the real failing **test** profile, run the same acceptance binary with `--profile-copy`.

Required behavior:

1. canonicalize/read the source profile without modifying it;
2. copy to a new temporary directory;
3. hash source/copy settings/radial inputs and verify copy identity before launching;
4. use the copied directory as child CWD;
5. run the high-value subset first: focused tap, Designer pointer, Tab, Menus/Skins, clean close;
6. if those pass, run the complete authoring workflow on the copy or a derived safe menu so existing user definitions are not destructively rewritten;
7. delete copy after success; retain report/logs, not private content.

The acceptance report should label deterministic fixture versus copied-profile run distinctly.

Do not infer that a clean-fixture pass means a profile-specific failure is fixed. Conversely, a copied-profile failure must not mutate/repair the source profile automatically.

---

# 8. Visual/structural validation and screenshots

Do not introduce strict full-window pixel golden tests across arbitrary Windows fonts, themes, GPUs, and DPI.

Prefer structural assertions:

- root/Designer window nonzero sane bounds;
- Designer target control inside client/work area;
- canvas visible with nontrivial size;
- optional panes don't completely cover/overlap the board;
- selected ring/cell/control exposed in accessibility tree;
- proposal preview has valid geometry;
- state/value/selection changes on next frame;
- no screen-sized or absurd bounds for ordinary controls/tooltips if those surfaces are touched by the workflow.

On failure, screenshot is diagnostic. Save the target window/monitor crop and report bounds/scale. Manual aesthetic review may remain optional, but functional layout/input pass is automated.

---

# 9. Minimal test execution plan

The goal is **stronger coverage with fewer expensive executions**, not more test cycles.

## 9.1 Add tests while coding, defer most execution

Expected new/updated automated groups:

1. table-driven exact `Shift+Alt+Win+End` adapter semantics;
2. headless production Designer semantic click and state mutation;
3. headless Tab/focus and initial-snapshot -> enabled transition;
4. report/profile-builder/native helper pure tests (no real desktop input);
5. any small regression that directly captures the actual root cause discovered by native acceptance.

Do not replicate the entire native case matrix in normal unit tests.

## 9.2 Optional one combined focused batch

After Packages A–D are integrated and before final native remediation, run at most one small focused batch if useful. Discover actual test names; an example shape is:

```text
cargo nextest run --no-fail-fast -E 'test(/launcher_invocation/) | test(/radial_editor/) | test(/radial.*acceptance/)'
```

Do not blindly use this expression if it selects hundreds of unrelated historical tests. Inspect selected count first if the installed Nextest supports it, or use exact current module/name filters.

If the full final Nextest is imminent and `cargo check --tests` plus native acceptance already give enough feedback, skip this optional focused run.

## 9.3 Native acceptance is the primary remediation loop

Build the source-matched executables and run:

```text
radial_acceptance --launcher <source-matched multi_launcher.exe> --output <run-dir>
```

Then, when available:

```text
radial_acceptance --launcher <...> --profile-copy <test-profile-dir> --output <run-dir>
```

Exact CLI syntax is an implementation detail; document the actual final syntax in the ledger/README.

Every native failure should identify a case and stage. Fix root causes in coherent batches; `cargo check`; rebuild; rerun native acceptance. Do not ask the user to perform the failing action unless the runner proves a specific OS boundary cannot be driven automatically.

## 9.4 Final automated gates

For the final candidate:

```text
cargo fmt --all --check
cargo check --lib --bin multi_launcher --bin radial_acceptance
git diff --check
cargo build --bin multi_launcher --bin radial_acceptance
# automated deterministic native acceptance, plus copied-profile pass when supplied
cargo nextest run --no-fail-fast
```

Retain durable logs and true exit codes. After relevant source remediation following full Nextest, rerun the affected acceptance and the final full suite. Documentation-only ledger updates do not require rebuilding unchanged code.

---

# 10. Native acceptance case matrix

The native runner should have stable IDs and report each independently.

## HOTKEY

| ID | Case | Pass condition |
|---|---|---|
| H0 | ROOT focused, tap | ROOT becomes hidden; exactly one short-tap path; no radial open |
| H1 | ROOT hidden, tap | ROOT becomes shown/focused as current product requires |
| H2 | ROOT visible but runner/other acceptance window focused | global tap still toggles ROOT |
| H3 | Designer focused/open, tap | ROOT toggles only; Designer remains open |
| H4 | hold from ROOT visible | runtime radial toggles/open; ROOT visibility unchanged |
| H5 | release after hold | no grid co-fire |
| H6 | second full hold | runtime radial closes/toggles according to contract; release is inert |
| H7 | emergency/Screen Draw fixture path | established higher-priority ownership wins; only if safely automatable in isolated fixture |

The exact Win chord remains in deterministic Layer A rather than default global injection.

## DESIGNER INPUT/LIFECYCLE

| ID | Case | Pass condition |
|---|---|---|
| D0 | Edit Radial Menus entry | one `Radial Designer` appears with ready/enabled body |
| D1 | native client click on harmless toggle | native target -> DesignerPointer -> enabled body -> accepted widget -> observable state change |
| D2 | Tab | focus moves among eligible Designer controls |
| D3 | ROOT hidden while Designer open | Designer continues accepting input/replies |
| D4 | Edit Radial Skins entry | same authoritative Designer backend reaches Skins/resources mode and remains interactive |
| D5 | clean close | Designer closes promptly; child process/root remains |
| D6 | dirty close | existing safe prompt can be automated and Keep Editing preserves draft |
| D7 | disposable pending close | no permanent blocked close or late reopen |

## BASIC AUTHORING

| ID | Case | Pass condition |
|---|---|---|
| A0 | New Menu | new stable menu exists/selected |
| A1 | Add Ring | proposal/review reachable, no silent mutation before Apply |
| A2 | Slots grow | validated proposal, Apply changes count and keeps old cell IDs |
| A3 | Action search | query reaches target beyond unfiltered first-50 boundary and assigns it |
| A4 | Open in Inspector | Inspector visibly opens/selects same cell without silently losing popup edit |
| A5 | Skin/style change | harmless style value updates preview/draft |
| A6 | Save/reopen | persisted radial data contains intended menu/ring/action/style |
| A7 | Undo/redo | one representative accepted edit reverses/reapplies coherently |
| A8 | Design safety | zero real leaf execution/history side effect from ordinary Designer interactions |

## GEOMETRY/ROBUSTNESS

| ID | Case | Pass condition |
|---|---|---|
| G0 | outer ring proposal | valid candidate preview and explicit Apply |
| G1 | populated shrink cancel/resolution | no silent data loss; explicit decision path works |
| G2 | compact Designer bounds | required controls/canvas within sane client geometry |

## CLEANUP/REPORTING

| ID | Case | Pass condition |
|---|---|---|
| R0 | report | valid JSON + concise text; candidate/profile hashes recorded |
| R1 | failure artifact test | controlled harness-only failure records trace/log/screenshot without corrupting profile; may be unit/helper test rather than deliberately failing final run |
| R2 | cleanup | test child exits, no owned acceptance child HWND/process remains, temp profile removed on success |

Do not inflate the native suite with historical radial rendering/import cases that existing tests already cover.

---

# 11. Failure classification and remediation evidence

A major purpose of the runner is to tell Codex **where** to fix the code.

Use a small failure-stage taxonomy, for example:

```text
Environment
CandidateStartup
WindowDiscovery
InputInjection
HookAdmission
GestureDecision
RootCommand
NativeRootState
DesignerEntry
DesignerNativeTarget
DesignerFrameworkInput
DesignerReadiness
DesignerWidget
DesignerMutation
DesignerPresentation
Persistence
Cleanup
```

Names can differ, but the stages must distinguish the actual ownership boundaries.

The report should include the first absent/contradictory expected edge and nearby trace facts, not dump all logs into one opaque error string.

Examples:

```text
H0 failed at NativeRootState:
  SendInput=2/2
  configured_primary Press/Release seen
  short_tap seen
  desired_visibility=false
  ROOT Visible(false) command seen
  HWND remained visible after deadline
```

versus:

```text
D1 failed at DesignerFrameworkInput:
  UIA target bounds valid
  WindowFromPoint belongs to Designer PID
  SendInput mouse events inserted
  no DesignerPointer trace edge
```

These are actionable without user narration.

---

# 12. Performance, determinism, and flake control

Native GUI automation can become flaky if implemented as sleeps and title searches. Avoid that.

- Use per-condition deadlines and short bounded polling/event waits; don't hardcode multi-second sleeps between every action.
- The only intentional long-ish gesture wait is the hold threshold plus margin.
- Resolve HWNDs by child PID plus semantic title/class/owner, not title globally.
- Resolve controls semantically each time after structural UI changes; do not retain screen coordinates across menu/ring changes unless the control is intentionally stable.
- Wait for an expected next state rather than “sleep 500 ms then assume.”
- Treat UIA element invalidation after frame rebuild as normal; reacquire by semantic identity.
- Keep one child process per run unless the case specifically verifies restart/persistence.
- Run native cases serially inside that child to avoid foreground/input races.
- Headless/pure tests remain parallelizable under Nextest.
- Do not make the production app depend on the acceptance runner or vice versa at runtime.

If UIA provider behavior is inconsistent for a particular egui widget, document the specific control/pattern and use the approved pointer fallback with semantic bounds. Do not abandon automation wholesale.

---

# 13. Definition of done

This task is complete only when all of the following are true for the final candidate:

## Architecture

- [ ] `radial_acceptance` is an explicit Windows-only native acceptance runner, not part of normal app startup.
- [ ] deterministic temp profile isolates persistence/single-instance state.
- [ ] optional profile-copy mode never modifies the source profile.
- [ ] exact real chord semantics are deterministically tested without global Win-key injection.
- [ ] safe injected native chord traverses the real low-level hook and actual ROOT window path.
- [ ] headless Designer driver sends real egui RawInput through the production Designer UI.
- [ ] native Designer automation uses real UIA/Windows input and at least one real pointer and keyboard path.
- [ ] no “test mode = directly set success state” shortcuts exist.

## User failures

- [ ] focused ROOT short tap hides automatically.
- [ ] hidden ROOT tap shows automatically.
- [ ] Designer-focused tap toggles ROOT only.
- [ ] hold toggles runtime radial only and release cannot co-fire grid.
- [ ] Edit Radial Menus client controls accept native pointer and keyboard input.
- [ ] Edit Radial Skins client controls accept native pointer and keyboard input.
- [ ] hiding/showing ROOT does not disable Designer.
- [ ] clean close is terminal and responsive; dirty close preserves existing safe semantics.

## Authoring

- [ ] native automated New Menu -> Add Ring -> Slots -> geometry proposal -> Apply works.
- [ ] action search/assignment reaches targets beyond the historical first-50 subset.
- [ ] Open in Inspector actually opens/selects the item and handles popup edits safely.
- [ ] skin/style edit previews and saves.
- [ ] Save -> close -> reopen proves persisted typed state.
- [ ] undo/redo works for representative edit.
- [ ] Design-mode automation executes no real leaf action.

## Evidence

- [ ] deterministic profile native acceptance report is PASS.
- [ ] copied-profile high-value pass is PASS when a path was provided; otherwise clearly `not_run` rather than implied pass.
- [ ] every native failure during development automatically had a report/log/trace and screenshot where possible.
- [ ] final `cargo fmt --all --check` passes.
- [ ] final `cargo check --lib --bin multi_launcher --bin radial_acceptance` passes.
- [ ] final `git diff --check` passes.
- [ ] source-matched launcher/runner build passes and hashes are recorded.
- [ ] final `cargo nextest run --no-fail-fast` passes, with actual counts and exit code recorded.
- [ ] independent review focuses on bypasses, automation validity, input safety, profile isolation, and actual root-cause corrections.
- [ ] working tree contains only intended task changes/ledger updates and is clean after commits.

**Functional sign-off must not require the user to manually press the launcher chord or click the Designer.** Manual review may remain useful only for subjective aesthetics. If one exact OS boundary truly cannot be automated, record the technical reason and the smallest remaining manual observation; do not casually revert to user-driven testing.

---

# 14. Independent review questions

Before final completion, a read-only reviewer should answer:

1. Does native hotkey acceptance actually enter through `SendInput` -> WH_KEYBOARD_LL -> `LauncherInvocationAdapter`, or does the runner bypass the hook?
2. Is acceptance input classified as ordinary external injection rather than the application's own self-injection tag?
3. Does exact `Shift+Alt+Win+End` remain covered by deterministic production adapter tests?
4. Does root visibility pass use real HWND/viewport outcome rather than only a desired boolean?
5. Does Designer pointer acceptance use a real client pointer path and prove the production widget mutation?
6. Does headless Designer testing call the actual `viewport_ui`/production controls, or a reimplemented test widget?
7. Are AccessKit/UIA labels stable/user-meaningful rather than test-only IDs?
8. Could the runner accidentally inject into another foreground process/window?
9. Can copied-profile mode mutate the original through symlinks/junctions/shared asset paths?
10. Are failure traces/reports free of note/clipboard/user payload?
11. Does any acceptance-only code branch make production behavior succeed without traversing the normal owner?
12. Are the actual current focused-hotkey and dead-Designer root causes fixed at their ownership boundary rather than concealed by retry/sleep/focus hacks?
13. Does final native PASS correspond to the same source-matched executable whose hash is reported?
14. Did final source changes after full Nextest invalidate/re-require the final gate?

Resolve substantive findings in coherent batches. Do not expand into unrelated radial features or aesthetic redesign.

---

# Appendix A — likely current code seams

Revalidate these paths/symbols in the live checkout; they are supported by the inspected archive:

| Area | Current seam |
|---|---|
| low-level shared input | `src/hotkey/launcher_invocation.rs` — `LauncherInvocationAdapter`, `LauncherInvocationService`, WH_KEYBOARD_LL hook, `classify_provenance`, `accept_external_injected` |
| shared invocation config | `src/main.rs::radial_invocation_config` |
| root visibility | `src/visibility.rs` — `RootViewportCtx`, `apply_visibility_*`, toggle batch |
| root UI entry | `src/gui/render.rs` — File -> Apps -> `Edit Radial Menus` |
| Designer viewport/input | `src/gui/radial_editor/mod.rs` — `show_deferred`, `viewport_ui`, `trace_pointer_release_response`, `basic_authoring_toolbar` |
| Designer preview | `src/gui/radial_editor/preview.rs` |
| authoring | `src/radial/authoring.rs` and `src/radial/authoring/menu.rs` |
| acceptance trace | `src/radial/acceptance_trace.rs` |
| UIA references | `src/mkmacro/uia.rs`, `src/plugins/browser_tabs.rs`, `src/actions/system.rs` |
| app data root | `src/platform/app_data.rs` |
| single instance | `src/platform/single_instance.rs` |
| settings startup | `src/main.rs`, `src/settings/io.rs`, `src/settings/model.rs` |
| existing native smoke style | `src/bin/passive_overlay_smoke.rs` |

The current manifest already enables `windows` `Win32_UI_Accessibility` and input/window features and already depends on `screenshots`. Prefer existing dependencies.

---

# Appendix B — suggested commit boundaries

Use actual coherent results; do not force this exact count if production remediation naturally combines two items.

```text
test(radial): add deterministic designer and native acceptance harness
fix(radial): resolve native focused toggle and designer input ownership
feat(radial): automate basic authoring acceptance workflow
test(radial): close native acceptance and regression evidence
```

If the harness immediately identifies two distinct fixes, separate the production fixes into clear commits. Do not create a commit after every compiler correction.

---

# Appendix C — primary API references consulted

Use the pinned versions/current Windows bindings in the repository.

```text
[W1] Microsoft SendInput
https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput

Important constraints used here:
- inserts keyboard/mouse INPUT events serially;
- does not reset already-held keyboard state;
- can be blocked by UIPI when injecting into higher-integrity applications.

[W2] egui 0.27.2 Context
https://docs.rs/egui/0.27.2/egui/struct.Context.html

Important boundaries used here:
- Context::run / RawInput drives a complete UI frame;
- Context retains input/memory across frames;
- AccessKit output and viewport-specific input/wakeup APIs are available in the pinned version.
```

External documentation explains API constraints; it does not prove any Multi Launcher bug is fixed.
