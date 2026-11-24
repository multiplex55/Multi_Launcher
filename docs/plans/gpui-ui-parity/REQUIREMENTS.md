# Requirements authority

## Current execution clarifications

The text below is the user's supplied scope, preserved as the governing product brief. The master Goal and individual packets operationalize it. The following explicit interpretations apply consistently across the package:

1. **Ordinary-launch default:** the user’s new parenthetical “Egui will be the default for now, if the user just activates the exe” controls. For this program, absent `--ui` selects egui. A remembered frontend setting may populate a chooser or explicit restart action, but does not implicitly override a bare launch. This deliberately specializes the older generic “CLI, then saved preference” paragraph below; there is no hidden ambiguity for the implementer.
2. **Actual binary:** the inspected Cargo manifest names the executable `multi_launcher`. Examples must use `multi_launcher.exe` on Windows; preserve future intentional naming changes at kickoff.
3. **Source baseline:** refresh latest `origin/master` when starting the actual branch. This package’s source reference is `f7bc610a86bcad1c14b69dce65b1d804df1b7934`. Do not silently update the behavioral baseline after work starts.
4. **Program ownership:** all initial work goals remain on one migration branch. Old standalone-goal templates that create a new branch per goal are not adopted.
5. **Validation budget:** no automatic full Nextest run at initial discovery, each GUI, each goal, or final closure. A broader run requires an explicit concrete reason. Compilation/linking time is separate from individual test limits.
6. **Framework-independent native facilities:** the selected frontend may call existing native file dialogs, capture/overlay backends, and Windows services where they do not depend on an egui host. Every egui-defined control/presentation still needs a GPUI counterpart; native reuse is not an omission.
7. **Planning status:** the documents are ready-to-execute prompts. They are not evidence of code migration, runtime inventory completeness, tests or native acceptance.

The program invariants PAR-01 through PAR-10 and the detailed per-GUI records provide traceability; no requirement in the supplied brief may be dropped because it lacks a separate identifier.

---


# Multi Launcher UI Conversion

## 1. Primary objective

Create a second, complete native frontend for **Multi Launcher**, implemented in **GPUI using GPUI Kit**.

Recreate the existing egui interface **one GUI at a time**, preserving all existing functionality, controls, information, interactions, workflows, and supported window modes.

Both frontends must use the same underlying Multi Launcher engine. The user must be able to choose which frontend to run when launching the application:

```powershell
MultiLauncher.exe --ui=gpui
MultiLauncher.exe --ui=egui
```

(Egui will be the default for now, if the user just activates the exe)

These commands illustrate the required selection behavior; preserve the project’s actual executable naming conventions.

Exactly one frontend runs during a session. The egui frontend remains a complete, working fallback throughout development and after the GPUI frontend is finished.

**The GPUI frontend must ultimately provide the entire application.** A polished main launcher, a collection of representative dialogs, or an interface that requires egui for unfinished features does not satisfy the final objective.

---

## 2. Define precisely what “everything the same” means

The migration must preserve the following:

| Area | Required equivalence |
|---|---|
| Features | Every existing operation remains available. |
| Controls | Every meaningful button, field, toggle, selector, menu item, toolbar, tab, panel, preview, and configuration section has an equivalent. |
| Information | The same relevant information, labels, statuses, results, and errors are available. |
| Behavior | Inputs, defaults, validation, limits, enabled/disabled conditions, and side effects retain their meaning. |
| Keyboard interaction | Shortcuts, focus movement, arrows, numpad navigation, Tab, Enter, Escape, and Jump Mode retain their established behavior. |
| Pointer interaction | Clicking, selection, scrolling, dragging, resizing, context menus, and tooltips continue to work as expected. |
| Workflows | Existing entry points and sequences of actions remain available. |
| Window modes | Embedded, docked, popped-out, independent, modal, floating, and overlay presentations are preserved where supported. |
| Data | Both frontends operate on the same application data and configuration. |
| Appearance | Styling improves while the interface remains recognizable and functionally equivalent. |

### Visual improvements are permitted and expected

The GPUI version can have cleaner typography, better spacing, refined borders, consistent icons, improved colors, and more polished hover, focus, and selection states.

However, that does not authorize removing controls, changing command prefixes, combining unrelated dialogs, reorganizing workflows, or hiding advanced settings because a different arrangement is easier to implement.

**The target is behavioral and structural parity with a visual refresh.** Reproducing egui’s exact pixels would defeat the reason for introducing GPUI.

Use matching 1516 × 937 pixel windows, display scaling, and application states for the primary parity reference. Compare information density, control positions, spacing, alignment, and workspace. Compact and minimum sizes supplement this reference. Keep Search top-centered and visible above the results and docked content, with its position stable during content changes and scrolling and its sizing responsive to window resizing as in egui.

Existing defects need separate treatment. Record proven baseline bugs and distinguish them from intended behavior. Do not deliberately reproduce a known defect, but do not silently redesign uncertain behavior during the port.

---

## 3. Use the official GPUI Kit as the frontend foundation

Use the toolkit at:

- [GPUI Kit](https://gpui-kit.com/)
- [Official source repository: longbridge/gpui-kit](https://github.com/longbridge/gpui-kit)

The official installation documentation currently presents the `gpui-kit` application dependency and coordinates its matching GPUI packages. That is the dependency family the project should evaluate and pin. Earlier suggestions involving other repositories should not override this selection. [GPUI Kit](https://gpui-kit.com/docs/installation/)

Requirements:

- Record the selected Kit release, matching GPUI packages, Rust toolchain, enabled features, and lockfile.
- Use documentation matching the selected release.
- Avoid mixing independently selected, incompatible GPUI dependencies.
- Use Kit’s styled components and reusable interaction infrastructure as the normal foundation.
- Map recurring Multi Launcher patterns to reusable GPUI components.
- Compose or extend primitives when a stock control does not preserve the required behavior.
- Record significant custom controls, adaptations, and upstream limitations.

Adding `gpui-kit` as a dependency while independently rebuilding every ordinary control would not meet the intended adoption.

### Optional capabilities do not automatically expand the project

This remains a **native Windows Rust application**.

WebAssembly output, JavaScript extensions, a webview frontend, and a new plugin execution language are outside this migration’s scope. Likewise, optional alternative rendering backends should not be introduced as an unrelated experiment.

Accessibility and UI testing should be used where they support the application. Custom controls need meaningful accessible names, states, actions, keyboard operation, and visible focus—not merely an AccessKit dependency. GPUI Kit’s accessibility documentation makes these separate responsibilities explicit. [GPUI Kit](https://gpui-kit.com/docs/accessibility/)

---

## 4. Make frontend selection reliable

### Select before initializing the GUI framework

The application must choose its frontend before starting either framework’s application/window lifecycle.

A GPUI session must not initialize an invisible egui application to provide services, render controls into textures, or open egui utility windows.

Ordinary Windows facilities such as native file pickers remain appropriate.

### Support a saved preference and an external override

The selection behavior should be:

1. An explicit launch argument takes precedence.
2. Otherwise, use the saved preference.
3. Preserve a documented way to start egui when GPUI cannot open.

A setting located only inside GPUI is insufficient as a recovery mechanism.

The interaction with single-instance enforcement must also be defined. If GPUI is running and the user launches with `--ui=egui`, the application must not silently ignore that choice and reactivate GPUI. It should use the controlled restart flow or clearly explain that the current instance must close first.

### Prefer one executable and the existing build workflow

The preferred distribution is one executable containing both frontend choices, built through the normal:

```powershell
cargo build --release
```

Establish early that the selected dependencies can build, link, and launch both alternatives on Windows. Treat this as a compatibility check, rather than assuming that the proposed packaging already works.

An egui-only development build may help iteration, but it must not replace verification of the actual distribution containing both choices.

### Switch by restarting

A frontend change should use a controlled restart/relaunch. A persistent daemon, IPC architecture, or live replacement of event loops is not required.

The switch must account for unsaved documents, active file operations, recording, automation, and other ongoing work. Preserve existing save, finish, cancel, and shutdown semantics.

Manual startup into egui is mandatory. Automatic recovery from every native GPUI crash is not a promise this project needs to make.

---

## 5. Establish one shared engine without duplicating behavior

The shared-engine boundary should make both frontends use the same authoritative application operations.

| Shared application responsibilities | Frontend responsibilities |
|---|---|
| Query interpretation and search | Search-field and result-list presentation |
| Commands and action execution | Buttons, menus, shortcuts, and event translation |
| Plugin operations | Plugin dialogs and editors |
| Validation and calculations | Displaying results and validation feedback |
| Persistence and canonical documents | Framework-specific widget state |
| Background jobs and operation status | Progress presentation and UI updates |
| Shared service ownership | Native windows, focus, layout, and rendering |

When an operation currently lives inside an egui handler, extract the operation into shared code and make both frontends call it.

Do not implement the same file operation, calculation, command, or persistence rule independently in GPUI.

### Keep framework objects out of shared contracts

Shared public interfaces should not require egui widget/context objects, viewport identifiers, texture handles, GPUI views/entities, or framework-specific input events.

The engine can express application intent, such as:

- Execute this action.
- Publish these search results.
- Open this tool.
- Focus the launcher query.
- Save this document.
- Report this operation’s progress.

Each frontend implements the corresponding presentation and interaction.

This does not require a universal widget abstraction. A common `draw_button` or `draw_panel` interface would constrain both frameworks while doing little to preserve actual application behavior.

### Preserve asynchronous behavior and ownership

Retain correct behavior under rapid typing, stale search results, repeated invocation, background completion, cancellation, and close/reopen cycles.

One user action must still cause one intended operation. Closing a window must not let an obsolete task modify a replacement window or execute a duplicate side effect.

Global hotkeys, clipboard listeners, indexing, file watchers, timers, and automation must retain deliberate ownership. The inactive frontend must not start another copy.

### Inspect plugin UI contracts

A plugin backend and its GUI are separate migration obligations.

Any existing callback that expects `egui::Ui` needs an appropriate GPUI presentation or a documented compatible contract. Having a callable plugin function does not establish UI parity.

---

## 6. Inventory the complete application before claiming coverage

The implementation AI must derive the inventory from the actual source baseline and reachable application behavior.

It must inspect window creation, viewport usage, dialogs, menus, command routes, plugin entry points, configuration gates, and custom drawing.

**One Rust file is not necessarily one GUI.** A large file can contain many surfaces, and a single interface can span multiple modules.

The inventory must include:

- Main windows and utility windows.
- Embedded and popped-out variants.
- Child dialogs, confirmations, popovers, menus, and tooltips.
- Floating toolbars, overlays, radial interfaces, and custom canvases.
- Settings pages and nested configuration sections.
- Loading, empty, selected, disabled, error, cancellation, and recovery states.
- Optional and dynamically generated plugin interfaces.
- Transitions between tools.

### Starting coverage families

The following is a starting list, not a boundary on the final scope:

| Family | Examples to inventory |
|---|---|
| Launcher/navigation | Main query, results, invocation, history, command routes, keyboard/numpad navigation, Jump Mode, universal actions |
| Settings/authoring | General settings, themes/fonts, bookmarks, actions, groups, favorites, hotkeys, plugin configuration, help |
| Search/files | File Search, previews, Diff, Batch Rename, Search & Replace, duplicate finding, hashing, archives, file locks and operations |
| Notes/text | Notes, Markdown Studio, previews, outlines, export/printing, clipboard tools, snippets, JSON/XML/regex utilities |
| Calculation/data | Calculation Pad, configuration, dependencies, programmer inspector, vectors, charts, sensitivities, tables, date/time and units |
| Visual editors | Diagram Studio, visual/Mermaid interfaces, canvases, inspectors, crop/compare, QR and color tools |
| Automation/integration | MkMacro, recording, Paste Sequence, gestures, Hot Folder, process/window tools and Explorer integrations |
| Specialized Windows surfaces | Radial runtime/designer, OCR selection, Screen Draw, Pinboard, coordinate tools and click visualization |
| Other tools | Dashboard, calendars, tasks, timers, activity, notifications and every other discovered utility |

Verify the actual feature names and availability at the selected baseline. No interface may be omitted because it is difficult, unusual, or absent from this example list.

---

## 7. Give every GUI its own specification and acceptance record

Create a dedicated Markdown record for each GUI with a stable identifier.

Each record should contain:

1. **Identity and routes:** name, source modules, parent/child surfaces, launch routes, owner goal, and dependencies.
2. **Controls:** every field, button, menu, panel, column, option, default, limit, and availability condition.
3. **States:** relevant normal, empty, populated, loading, error, disabled, selected, expanded, and recovery states.
4. **Input behavior:** pointer actions, text editing, shortcuts, focus movement, Jump Mode, scrolling, selection, and dragging.
5. **Window behavior:** ownership, modality, supported pop-outs, placement, resize, close/hide/reopen, and remembered state.
6. **Data behavior:** shared operations, save/apply/cancel semantics, persistence, and active-job handling.
7. **GPUI mapping:** Kit components, custom elements, reusable patterns, and any behavioral adaptations.
8. **Visual evidence:** comparable application screenshots and recorded intentional styling changes.
9. **Verification:** scenario IDs, tests, native checks, tested binary/commit, outcomes, and reviewer findings.
10. **Remaining work:** gaps, regressions, blockers, and the exact next action.

A GUI is not accepted because it opens or resembles the old interface.

For example, accepting the main launcher requires evidence for invocation, typing, results, navigation between query and list, action execution, hiding/reopening, and interaction while utility windows are open.

Complete coherent GUI slices. Empty shells for many dialogs are scaffolding, not broad migration progress.

---

## 8. Treat Windows interaction as part of the product contract

For Multi Launcher, window and input behavior are central features.

Explicitly preserve and verify:

- Configured global hotkeys and tap/hold behavior.
- Toggle behavior when the launcher is already focused.
- Main query editability while utilities are open.
- Navigation from query to results and back.
- Arrow, numpad, Tab, Enter, Escape, and Jump Mode behavior.
- Appropriate shortcut scoping inside text editors.
- Foreground-window capture and restoration where intended.
- Independent windows, ownership, modality, Alt-Tab, and taskbar behavior.
- Operation while the main/root window is hidden.
- Always-on-top, opacity, transparency, and hit testing.
- Click-through prevention and intentional click-through behavior.
- Pointer capture and input restoration after cancellation or failure.
- DPI scaling, monitor placement, remembered geometry, and monitor changes.
- Actual clipboard/paste behavior against other applications.

Use the current configured shortcuts and thresholds. Do not hardcode values remembered from a previous conversation.

A toolkit’s local focus model passing a test does not prove that Windows activates the correct foreground window.

---

## 9. Preserve data compatibility between the frontends

Both frontends must use the same canonical application identity, data locations, settings, documents, history, bookmarks, actions, and plugin state.

Keep framework-specific presentation memory separate where needed. egui widget-memory serialization does not become GPUI state automatically.

Shared semantic preferences—such as text sizing, theme intent, tool settings, and meaningful geometry—must retain their effect.

### Require cross-frontend round trips

For affected persisted features, verify workflows such as:

1. Create or modify data in egui.
2. Open it in GPUI.
3. Modify and save it in GPUI.
4. Reopen it in egui.
5. Confirm that content, options, and interpretation remain correct.

Use representative fixtures and isolated copies.

The fallback is only useful if switching to GPUI has not made the application’s data unreadable or incomplete in egui.

Also preserve save/apply/cancel and unsaved-change behavior. A frontend restart must not silently discard edits or pretend that active jobs automatically survive.

---

## 10. Use targeted validation instead of repeated full nextest runs

This needs to be an explicit project rule.

**Do not run the entire repository’s `cargo nextest` suite after every GUI, checkpoint, or documentation update.**

Use a documented impact map to select the checks relevant to each change:

| Change | Appropriate verification |
|---|---|
| Styling within one GUI | Inspect affected states and build/check as appropriate |
| Control behavior | Focused production-view tests and relevant interactions |
| Shared command or validation | Relevant core tests and representative use through both frontends |
| Persistence | Compatibility and round-trip checks |
| Startup/frontend selection | Launch both choices and verify selection/fallback |
| Hotkeys/window lifecycle | Actual Windows interaction checks |
| Cross-cutting engine extraction | A broader, named set of affected suites and frontend regression checks |
| Final integration | Consolidated acceptance evidence, with broader testing where justified |

Before a GPUI path exists, core tests and the working egui path can validate an extraction checkpoint. Its GPUI parity checks remain pending until that interface exists.

### Preserve meaningful testing discipline

Requirements:

- Reuse existing tests and fixtures before adding new ones.
- Add tests for meaningful behavior and regressions, not tests that merely repeat implementation details.
- Confirm filters selected the intended tests; zero selected tests is not a successful verification.
- Record exact commands, selected tests, pass/fail/timeout counts, durations, and tested commits.
- Reproduce failures individually or in the smallest relevant group.
- Distinguish established baseline failures from introduced regressions using evidence.
- Do not disable failing tests to make the migration appear complete.

### Separate build time from test time

Do not apply a blanket 100-second limit to an entire Cargo command.

Compilation, dependency rebuilding, and release linking have different costs from executing an individual test. Record those separately and use appropriate runner settings and test-specific timeouts.

### Use Kit’s tests alongside native verification

GPUI Kit documents production-view interaction testing, but explicitly distinguishes it from pixel checks and native application/platform testing. Its element snapshots do not launch the packaged application or inspect rendered pixels. [GPUI Kit](https://gpui-kit.com/docs/test/)

Use those facilities for ordinary control interactions and application outcomes. Retain actual Windows verification for global hotkeys, foreground activation, clipboard delivery, overlays, IME, and related platform behavior.

### Revalidate only what later changes affect

If a later shared component, engine contract, focus adapter, persistence change, or toolkit update affects an already accepted GUI, reopen its relevant checks.

Retain evidence that remains applicable. This avoids both stale acceptance and unnecessary full-suite reruns.

A deliberately scheduled full-suite run may be appropriate near final integration or after a broad change. It should resolve a concrete risk or satisfy an applicable gate, rather than become an automatic loop.

---

## 11. Preserve performance and responsiveness

Visual polish must preserve the application’s usefulness as a fast launcher.

Track relevant measurements:

- Startup to first usable frame.
- Hotkey to visible interface.
- Typing and query responsiveness.
- Large list/editor responsiveness.
- Radial and overlay transitions.
- Idle CPU and background wakeups.
- Repeated open/close stability.
- Material memory changes.
- Executable size and build/link iteration cost.

Compare equivalent runs on the same machine, build profile, configuration, and data.

Set practical regression budgets from the measured baseline. Do not invent a performance promise merely because GPUI uses the GPU.

The inactive frontend must not create windows, graphics devices, render loops, or duplicate listeners. Missing notifications must not be “fixed” by permanently repainting or polling.

---

## 12. Use a new, dedicated migration branch

When implementation begins:

- Create a new branch for the program.
- Use the latest fetched `origin/master` unless the user specifies another starting point.
- Record the exact baseline SHA.
- Preserve existing local work.
- Use a single linear migration branch by default.
- Continue successive goals on that branch.
- Follow the established no-worktree workflow.
- Make small, coherent checkpoint commits.
- Push checkpoints through the project’s established workflow.
- Record a push failure without discarding local progress.

Do not silently update the behavioral reference whenever master changes. If later integration is necessary, record the decision, reconcile the inventory, and verify affected areas.

Keep the egui frontend buildable and runnable at accepted checkpoints.

The implementing AI must read the current repository `AGENTS.md` and configured agent definitions, then use the project’s planner, implementer, and reviewer pattern. It must not invent model assignments or change agent instructions to simplify the task.

On the single Windows machine, coordinate heavy builds and allow only one agent to control the live desktop at a time.

---

## 13. Create a substantial Markdown tracking system

Use a dedicated documentation root, such as:

`docs/plans/gpui-ui-parity/`

The eventual path should follow existing repository conventions.

These records should be substantive and linked, with one authoritative location for each fact:

| Document | Purpose |
|---|---|
| `README.md` | Navigation, current goal, latest checkpoint, overall status |
| `REQUIREMENTS.md` | Accepted requirements and scope changes |
| `SOURCE_BASELINE.md` | Source SHA, environment, dependencies, baseline behavior and failures |
| `GUI_INVENTORY.md` | Complete surface inventory, routes, modes, dependencies and ownership |
| `PARITY_MATRIX.md` | GUI-by-GUI control, behavior and acceptance coverage |
| `CORE_CONTRACTS.md` | Shared operations, state and frontend boundaries |
| `FRONTEND_SELECTION.md` | Selection, preference, restart, single-instance and fallback behavior |
| `DESIGN_SYSTEM.md` | Styling, density, theme mappings and reusable visual rules |
| `COMPONENT_MAPPING.md` | Existing patterns mapped to Kit components and custom elements |
| `GOAL_REGISTER.md` | Goal order, dependencies, assigned GUIs and exit criteria |
| `PROGRESS.md` | Implemented/accepted counts and remaining work |
| `VALIDATION_STRATEGY.md` | Impact map, test groups, native checks and broader-run triggers |
| `VALIDATION_LOG.md` | Commands, results, timings, commits and evidence |
| `PERFORMANCE.md` | Baselines, comparisons, budgets and regressions |
| `KNOWN_ISSUES.md` | Existing failures, migration regressions and parity discrepancies |
| `DECISIONS.md` | Architectural choices, dependency decisions and explicit exceptions |
| `CHECKPOINTS.md` | Commit-linked progress and continuation records |
| `BLOCKERS.md` | Blockers, attempts, impact and recovery conditions |
| `FINAL_ACCEPTANCE.md` | Final reconciliation and actual outcome |
| `gui/<id>.md` | Detailed specification and acceptance for each GUI |
| `goals/<id>.md` | Scope and acceptance for each implementation goal |
| `reviews/<id>.md` | Independent review findings and resolution |
| `evidence/<id>.md` | Scenario, fixture, binary, commit and evidence references |

Create files as their contents become known. Avoid hundreds of empty placeholders or copying the entire project checklist into every document.

Documentation must make the work recoverable and measurable; it must not become a substitute for implementation.

---

## 14. Report progress in a way that cannot hide unfinished work

Use clear stages:

**Not started → Specified → In implementation → Implemented → Validation pending → Accepted**

Record blockers separately so the last achieved stage remains visible.

At meaningful checkpoints, report:

- GUIs inventoried.
- GUIs implemented.
- GUIs accepted.
- Shared engine requirements accepted.
- Remaining parity gaps.
- Open migration regressions.
- Native checks pending or blocked.
- Previously accepted checks requiring revalidation.
- Current goal, next work, and tested commit.

A percentage must identify its denominator and meaning. Files changed, lines written, or windows that open are not measures of accepted parity.

Also distinguish coverage from effort. A simple confirmation dialog and Diagram Studio can each be one inventory entry while requiring very different amounts of work.

Newly discovered GUIs must expand the inventory openly. Do not conceal them to preserve an earlier progress percentage.

---

## 15. Divide the program into as many goals as needed

The final goals should follow the completed inventory. A reasonable initial grouping is:

| Goal family | Required outcome |
|---|---|
| A. Baseline and scope | Verified source, GUI inventory, existing failures and documentation system |
| B. Shared behavior | Incremental engine separation with egui behavior preserved |
| C. Frontend hosting | Both launch choices, restart/fallback, Kit foundation and a representative complete GUI |
| D. Main launcher | Query, results, commands, invocation, keyboard/numpad and Jump Mode parity |
| E. Settings/common interfaces | Configuration, themes, authoring, menus, help and common dialogs |
| F. Files/utilities | Search, preview, diff, rename, replace, archives and related tools |
| G. Notes/text/editing | Notes, Markdown, clipboard interfaces, previews and exports |
| H. Data/visual studios | Calculation Pad, charts, tables, Diagram Studio and associated editors |
| I. Automation/native surfaces | Macros, paste, recording, gestures, radial, pinboard, drawing, capture and overlays |
| J. Integration | Cross-feature workflows, data round trips, lifecycle cleanup and remaining gaps |
| K. Final acceptance | Final release verification, regression closure, fallback and evidence reconciliation |

Large families can become several goals. Each GUI needs one accountable owner goal, with dependencies and shared work linked.

Investigate difficult native feasibility questions early. Do not defer all overlay, focus, and multi-window questions until most of the ordinary dialogs have been recreated.

---

## 16. Continue until the complete program is finished

After accepting a milestone or goal, update the records and continue to the next eligible work without asking for routine permission.

A completed prototype, main launcher, or subset of studios is not the end of the task.

At a session or context boundary, leave a durable continuation record containing:

- Branch and exact commit.
- Working-tree status.
- Active goal.
- Accepted GUI IDs.
- Partially completed changes.
- Latest validation and its coverage.
- Blockers.
- The exact next action.

Resume from that record. Do not restart accepted work or lose pending native verification when a different agent or session takes over.

For genuine blockers, record attempts and continue independent work. Once independent progress is exhausted, identify the specific missing input or environment capability. Do not loop indefinitely on the same failing check or declare blocked work complete.

---

## 17. Final definition of complete

The full migration is complete only when:

1. The inventory covers the actual application, including nested, optional and specialized interfaces.
2. Every required GUI, mode, control, interaction and workflow has an accepted GPUI counterpart.
3. GPUI Kit provides a coherent frontend foundation.
4. Both startup selections work in the actual distribution.
5. Only the selected frontend initializes.
6. GPUI does not depend on egui windows or rendering for missing features.
7. The complete egui fallback remains usable.
8. Both frontends use shared authoritative application behavior.
9. Data and configuration round trips preserve meaning and content.
10. Required native Windows behavior has appropriate evidence.
11. Visual quality, resizing, density, text sizing and keyboard usability are verified.
12. Material performance regressions are resolved or explicitly addressed.
13. No known migration-caused core regression or mandatory verification gap remains unresolved.
14. Final documentation identifies the delivered commit, build/run instructions, validation performed and coverage limits.

If the user explicitly accepts a verification limitation or reduced scope, document that exception accurately. It does not turn an unperformed check into a passing result or establish full original parity.

**The governing acceptance rule should be: the user can choose GPUI and use the whole of Multi Launcher as before, with a cleaner presentation, while retaining a dependable egui launch option.**
