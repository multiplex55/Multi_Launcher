# Multi Launcher — Automated Radial Acceptance Source Notes

## Evidence scope

**Inspected archive:** `launcher.zip`  
**SHA-256:** `b5e1df91f2dcab48618f0f9e29ae827c4048eb3404c5892fce92008e2e085f5a`

This is a read-only source review supporting `multi_launcher_radial_automated_acceptance_codex_plan.md`. No Cargo build, Nextest run, native Windows interaction, RM4 execution, or source modification was performed while preparing this handoff.

The actual current checkout is the implementation authority. Line numbers below refer to the inspected archive only.

## S1 — The current ledger explicitly says implementation passed automated tests but native interaction was not observed

`docs/plans/radial-input-recovery.md` records Packages A/B/C as implemented and the integrated automated result as 4,635 passed / 8 skipped, while separately leaving the actual focused hotkey and Designer gestures native-unverified.

This is the core evidence for moving the next goal to automated native acceptance rather than another pure-unit stabilization pass.

```text
13: | Work package | State | Evidence |
15: | A. Designer input, focused root toggle, readiness, close | implemented; native acceptance pending | ... The reported Designer client-input failure still needs an actual Windows trace and interaction. |
16: | B. Basic ring authoring, action search, Inspector handoff | implemented; native acceptance pending | ... |
17: | C. Previewable validated geometry proposals | implemented; native acceptance pending | ... |
18: | Integrated verification and native acceptance | tests passed; native acceptance pending | Final `cargo nextest run --no-fail-fast`: 4,635 passed, eight skipped, exit 0. ... The actual failing Windows profile and live GUI gestures remain unobserved. |
```

The same ledger says there was no running instance/profile/native desktop input control in that task and recommends using `MULTI_LAUNCHER_RADIAL_ACCEPTANCE_TRACE=1` on the actual candidate. That manual gap is what the new runner should eliminate.

## S2 — The current acceptance trace already covers the useful internal boundaries

`src/radial/acceptance_trace.rs` defines a bounded, opt-in privacy-preserving trace:

```text
13: pub(crate) const ENVIRONMENT_VARIABLE: &str = "MULTI_LAUNCHER_RADIAL_ACCEPTANCE_TRACE";
14: pub(crate) const EVENT_BUDGET: usize = 256;
```

Its typed event schema includes:

- Designer callback/focus/pointer/body/widget/mutation;
- authoring request sent/enqueued/accepted/rejected/retired;
- configured primary press/release and short tap;
- desired visibility and ROOT viewport commands;
- restore/native activation;
- native window snapshots;
- native pointer input for radial hosts.

The module explicitly excludes arbitrary menu names, notes, clipboard contents, raw keys and other user payload from the trace. The new runner should parse/use this evidence rather than add a parallel unbounded logger.

A narrow runtime-radial lifecycle edge may be added only if no existing observable boundary can prove hold-open/close.

## S3 — The native low-level input path already admits externally injected input

`src/main.rs::radial_invocation_config` sets:

```text
accept_external_injected: true
```

The low-level hook in `src/hotkey/launcher_invocation.rs` reads `LLKHF_INJECTED`, classifies it through `classify_provenance`, and routes accepted external injection through the same adapter/service used for physical input.

The existing test `configured_chord_accepts_admitted_external_tap_with_full_release` already verifies an externally injected configured chord can produce `ToggleLegacyLauncher` and drain full release.

This makes a safe profile-specific `SendInput` chord suitable for native acceptance without a test-only bypass. The acceptance driver must **not** use the application's self-injection tag, which is intentionally classified separately.

## S4 — Current app startup is naturally isolatable by working directory

`src/main.rs` creates the application data root from:

```text
AppDataRoot::from_settings_path("settings.json")
```

and sets the runtime settings path to the same relative file. `AppDataRoot` resolves relative settings from the process current directory.

Therefore the native acceptance runner can launch a real child with `current_dir(temp_profile)` and obtain an isolated data root/single-instance identity without adding a global test data-dir override.

This is the recommended profile-isolation mechanism unless newer live code changes the contract.

## S5 — ROOT hiding has a real native-visible command path

`src/visibility.rs` updates desired visibility and applies placement to `RootViewportCtx`. On hide, the current owner path also sends:

```text
egui::ViewportCommand::Visible(false)
```

The lower `apply_visibility` path also emits the parking boundary and offscreen position. On show it sends Visible(true), Minimized(false), Focus, and configured placement.

Native acceptance should therefore assert the actual root HWND state/bounds plus trace command chain, not just an internal boolean.

## S6 — The Designer is a real deferred viewport with a known title

`src/gui/radial_editor/mod.rs` creates a deferred viewport with title:

```text
Radial Designer
```

minimum inner size 520 × 380, resizable, and visible while the editor is open.

The deferred callback already traces pointer edges and runs `viewport_ui`. `viewport_ui` itself records `DesignerPointer` and `DesignerBody` (`Enabled`, `InitialSnapshot`, or `Conflict`) before rendering the interactive body.

This gives the native runner a precise automatic localization chain:

```text
Window/point target -> DesignerPointer -> DesignerBody -> DesignerWidget -> DesignerMutation -> next frame/UIA state
```

## S7 — The Designer already has semantic/accessibility coverage but not full interaction coverage

The current tests enable AccessKit and inspect real output nodes for style controls and blank cell identities. Keyboard tests already feed `egui::RawInput`/`egui::Event::Key` into retained contexts.

However, those examples generally render isolated widgets/tree fragments or invoke editor shortcuts. They do not prove the complete production `viewport_ui` receives a native mouse click and mutates the session.

The new headless driver should reuse AccessKit names/bounds from the actual production Designer frame and inject pointer/key events across multiple retained frames.

## S8 — The repository already has Windows UI Automation support and examples

`Cargo.toml` already enables these Windows features, among others:

```text
Win32_UI_Input_KeyboardAndMouse
Win32_UI_WindowsAndMessaging
Win32_System_Com
Win32_UI_Accessibility
```

`src/mkmacro/uia.rs` already initializes `IUIAutomation`, creates cache requests, bounds provider calls, and reads ProcessId/ControlType/Name/AutomationId/ClassName/FrameworkId/BoundingRectangle. `src/plugins/browser_tabs.rs` and `src/actions/system.rs` use `FindAll`, `InvokePattern`, SelectionItem patterns, legacy accessibility and `SetFocus`.

Therefore a small acceptance-specific UIA wrapper can be implemented with the current `windows` dependency. A new automation crate is not required by the current source.

Do not force the acceptance domain to use MkMacro-specific selector types unless that is cleaner than a small driver-local wrapper.

## S9 — The current root UI exposes a production Designer entry

`src/gui/render.rs` contains File -> Apps -> `Edit Radial Menus`, which routes to the normal radial editor panel/Designer path. Typed commands also support `radial edit` / `radial skins` elsewhere in the codebase.

Native acceptance should open the Designer through a production semantic entry (UIA/pointer/typed command through the actual command host) rather than directly flipping editor state from the external driver.

## S10 — The basic authoring controls already exist in production code

`RadialEditorState::basic_authoring_toolbar` exposes:

- New Menu;
- Add Ring;
- Ring selector;
- Slots count/proposal flow.

The input-recovery ledger says action search-before-limit, Inspector handoff, and geometry proposals were implemented. The next task should **exercise these existing controls through automation and fix the first failing boundary**, not rewrite them from scratch.

## S11 — There is no current native radial acceptance binary

The inspected `src/bin` contains only:

```text
passive_overlay_smoke.rs
```

There is no `radial_acceptance.rs`. The existing smoke binary is useful as a precedent for an explicit Windows-only diagnostic/acceptance executable, but it does not cover the launcher/Designer workflow.

## S12 — A safe native runner can use current dependencies without changing production architecture

The current manifest already includes:

- `windows` with input, windowing, COM and accessibility features;
- `screenshots` for screen capture;
- `tempfile` for isolated temporary directories;
- `sha2`/`hex` for identities.

Prefer these current dependencies before adding another automation/reporting stack.

## S13 — External API constraints checked during planning

Microsoft documents that `SendInput` inserts keyboard/mouse events into the input stream, returns the number inserted, does not reset already-held key state, and can be blocked by UIPI when integrity levels differ. The runner must check/record insertion results and avoid assuming a zero/partial result is an application defect.

Pinned egui 0.27.2 documents that `Context::run(RawInput, ...)` runs one UI frame and that retained `Context` state is used for input/memory across frames. This supports the proposed deterministic headless Designer driver.

External API references explain constraints only; they do not prove current Multi Launcher behavior.

## Evidence limits

Static source supports the architecture above, but it does **not** identify the current first broken native boundary. The automated acceptance runner's purpose is to generate that evidence without requiring the user to manually press/click and narrate the result.
