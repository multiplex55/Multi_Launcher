# Codex Kickstart: complete Multi Launcher GPUI UI parity

You are executing the complete GPUI UI parity program for `multiplex55/Multi_Launcher`. Read `docs/CODEX_GOAL_GPUI_UI_PARITY.md` and its linked goal package in full before broad implementation. These files define the approved scope: a complete native GPUI Kit frontend, GUI-by-GUI parity, one shared engine, and the complete egui fallback. Continue across all G00–G26 goals and any necessary discovered child work.

## 1. Establish source, authority, and the dedicated branch

1. Read current `AGENTS.md` and applicable nested instructions, `.codex/config.toml`, and actual planner/implementer/reviewer definitions. Use the configured roles/settings; do not invent old model choices or change agent instructions.
2. Inspect `git status -sb`, active branch, HEAD, remotes/upstream, index, unstaged and untracked work, and any existing migration continuation record. This is one existing checkout with no worktrees, auxiliary clones, or per-agent branches.
3. For a genuinely new run, fetch `origin/master` normally and resolve its exact SHA. The package was prepared against `f7bc610a86bcad1c14b69dce65b1d804df1b7934`; this is a planning reference, not permission to reset to it. Inspect subsequent commits affecting scope, startup, commands, core, UI, persistence, tests, or instructions.
4. Create a fresh unique branch such as `feat/gpui-ui-parity` directly from the fetched master SHA. Preserve all existing work. If the transition would overwrite or wrongly incorporate user work, stop only that unsafe transition and report the precise blocker. Do not hard reset, auto-stash, force-checkout, or substitute a stale base.
5. If this is a resumed migration, reconcile the recorded branch/HEAD and unfinished changes, then continue that branch. Do not create a fresh branch from master or lose prior GUI acceptance.
6. Materialize this package’s `docs/` files on the migration branch if they were attached instead of already present. Preserve existing repository files; do not overwrite unrelated plans. Inspect the package as task input, then update its execution fields from actual observations.
7. Complete `G00-A`: record parent SHA, task branch, current environment and tool versions, source deltas, and initial status; commit a small coherent baseline documentation checkpoint. Record the baseline commit separately from its parent.
8. Attempt one ordinary non-force upstream push. If it fails, preserve the local commit, record the failure, and continue. Do not enter a push-repair/rewrite loop.

Do not implement on master. All goals and agents stay on this same migration branch. A goal boundary does not authorize a branch change.

## 2. Apply the controlling defaults and invariants

The actual binary is `multi_launcher.exe`. Required selections:

~~~powershell
.\multi_launcher.exe                 # egui for an ordinary launch
.\multi_launcher.exe --ui=egui       # explicit fallback
.\multi_launcher.exe --ui=gpui       # explicit GPUI selection
~~~

For this program no selector always resolves to egui, even when the UI remembers that the user previously chose GPUI. A switch action may persist its remembered choice and relaunch with an explicit selector. Do not promote GPUI to the ordinary-launch default, including at completion.

Select before either GUI framework initializes. Keep exactly one frontend, one engine, and intentional service ownership. Preserve existing helper modes, data locks, startup recovery, command arguments and foreground capture. Explicit egui selection while a GPUI instance already owns the data directory must receive a controlled restart or clear actionable response; it must not silently do nothing.

No egui utility windows, hidden egui host, or egui textures in GPUI. No Tauri/webview, JavaScript extension runtime, WebAssembly product, new daemon, or universal widget abstraction. Reuse existing independent Windows facilities where they meet the selected frontend’s contract.

## 3. Finish discovery without stalling in planning

Complete G00’s source/runtime inventory. Inspect all actual construction and routes, including code outside `src/gui/`, plugin `settings_ui` paths, dashboard widgets, context menus, child dialogs, optional features and custom/native overlays. Expand the source candidate list into stable GUI IDs with modes, controls, operations, dependencies and accountable goals.

Create substantive `gui/<id>.md` records and a parity matrix. Mark source-only discoveries and runtime-unconfirmed behavior accurately. The 27 initial goals are coverage families, not a license to omit newly discovered interfaces.

Use bounded parallel read-only investigation when useful. Stop investigating a checkpoint once its owner, scope, invariants and next action are clear. Do not run the complete test suite to “establish a baseline.” Do not convert preparation into months of speculative architecture design.

## 4. Prove the risky foundation early

Execute G01 before bulk GUI ports. Pin official GPUI Kit and matching GPUI dependencies; use current primary documentation. Build the actual application binary with both frontends, establish the correct selected event-loop/thread ownership, and verify both startup alternatives.

Probe actual Windows hotkey/focus/show-hide, secondary windows with hidden root, foreground restoration, representative overlays, and DPI/monitor behavior. Record a concrete adaptation or blocker for unsupported behavior. A separate hello-world demo cannot accept this goal.

Then execute G02–G04: incremental shared-engine boundaries, selection/restart/data identity, shared presentation components, and one complete real Hash Generator GUI. Preserve the working egui path while extracting behavior. Do not require a wholesale engine rewrite before the pilot, and do not stop when the pilot is done.

## 5. Execute the full product conversion

Use `GOAL_REGISTER.md` and the per-goal packets. Default order is G00–G26; reorder independent feature work only with recorded dependencies and GUI ownership. Investigate native risks early even when the corresponding complete feature port is later.

For each checkpoint:

1. Planner confirms the actual owner, source/callers, invariants, concrete changes, narrow tests, native/visual needs, and done criteria.
2. One implementer performs the coherent changes, extracting shared operations instead of duplicating them.
3. Verify the meaningful affected behavior and inspect the diff.
4. Review the completed scope when the architecture/risk or goal requires it; resolve concrete findings.
5. Update the GUI/goal/evidence records, commit a coherent checkpoint, and attempt one ordinary push.
6. Continue to the next eligible checkpoint without asking for routine permission.

Include all ordinary, nested, complex and optional GUIs: launcher/Jump Mode, settings/authoring/recovery, file tools, Notes/Markdown/export/print, Clipboard Modify/snippets, JSON/XML/Regex, Calculation Pad and every companion, tables/converters, Diagram Studio, image tools, MkMacro/recording, radial interfaces, automation, native overlays, dashboard/organizers, and system/window tools. All other discovered baseline interfaces are also in scope.

## 6. Protect core behavior and user data

Use the existing typed command infrastructure and real domain owners. A trait implementation with default no-op methods is not command coverage. A backend callable without its controls is not GUI coverage.

Preserve command parsing, search/ranking/navigation, calculations, plugin operations, file safeguards, protected clipboard authority, stale-result rejection, exactly-once side effects, cancellation, visibility revisions, persistence, and recovery. Recent Clipboard Modify and test-runner fixes are part of the inspected baseline and must survive extraction.

Preserve shared canonical data identity and data locations. Separate framework widget memory. Test cross-frontend save/reopen/edit round trips on representative disposable data. Do not use production data or clipboard contents as casual fixtures.

A frontend switch is a controlled restart: resolve unsaved edits and active jobs through the existing save/finish/cancel semantics. Keep fallback usable if GPUI fails before opening a window. Do not promise arbitrary live jobs survive restart.

## 7. Validate proportionately

Read `VALIDATION_STRATEGY.md` before commands. Select the affected Cargo package/target first, then meaningful Nextest filters. Reuse existing tests and fixtures, confirm nonzero intended selection, and record actual commands/results.

Keep the current Nextest wrapper/stack policy and disable Paste Sequence Next/Cancel shortcuts in app fixtures before app construction, while preserving production defaults. Use the inspected profiles/helpers only where applicable and revalidate configuration against the execution baseline.

Separate compilation/linking from individual test execution. Do not put a blanket 100-second timeout around Cargo. Use supported runner/test-specific limits and diagnose actual hangs.

No full-suite nextest after every GUI, checkpoint, plan edit, or routine finalization. Broaden only to resolve a named concrete risk, selected shared boundary, or real collateral failure. Native tests and visual inspection are scoped by the GUI’s behavior. Kit headless tests do not prove Windows clipboard, IME, foreground focus, or release packaging.

Before a GPUI route exists, core+egui checks may accept a coherent extraction checkpoint; its GPUI parity remains pending. Compare behavior to the baseline, not merely to the other newly changed frontend.

When later shared changes affect accepted GUIs, reopen only relevant criteria and preserve still-applicable evidence. Do not make every edit trigger a complete regression campaign.

## 8. Maintain honest progress and resumability

Track the stages Not started → Specified → In implementation → Implemented → Validation pending → Accepted; blocked status is separate. Report actual inventory, implemented and accepted counts, parity gaps, native checks pending, core regressions, current goal/checkpoint and tested commit.

A window that opens is not automatically implemented; an implemented GUI is not automatically accepted. No placeholders, Unsupported buttons, TODO routes, suppressed errors or automatic egui handoffs may be counted as complete GPUI parity.

Keep the linked documents authoritative. At each context/session boundary write the exact branch, HEAD, working-tree state, active checkpoint, partial changes, applicable evidence, unrun checks, blockers, and next action. On resume read that record before repeating work.

Use one source writer, one shared heavy build/test process, and one native desktop controller. Planner/reviewer work may be parallel only when it is read-only and snapshot-safe.

## 9. Handle real blockers and finish correctly

Fix ordinary compiler/test/UI failures. If a repeated failure supplies no new evidence, stop identical attempts, record diagnosis and the smallest next capability/decision, and continue independent work. Push failures do not block local implementation.

If required native access is unavailable, preserve its verification gap. Do not claim Windows acceptance from a Linux build, headless test, screenshot, or intent to test later. When all independent progress is exhausted, write the current AGENTS-required factual stall report and state exactly what is blocked.

At G25–G26 reconcile every GUI and requirement against the delivered candidate. Run the appropriate focused union and canonical ordinary release build, check both startup choices and fallback, inspect actual assets/window behavior, compare measured performance, and obtain bounded cumulative review. Resolve relevant findings and refresh only affected evidence.

Report full completion only when all mandatory original criteria pass. Explicit user exceptions remain visible as changed scope/coverage. Deliver actual commit/branch/push state, build/run instructions, completed coverage, evidence and limitations. Do not merge to master or publish a release as part of this prompt.

**Begin with G00-A now and continue the approved program until complete or a genuine documented blocker prevents further independent progress.**
