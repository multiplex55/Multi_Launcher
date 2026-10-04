\# Multi Launcher — Radial Mouse Gesture Invocation + Reliable Launcher Hotkey Toggle



\## Status



\*\*State:\*\* Ready for implementation



\*\*Source of truth:\*\* Current repository checkout based on:



`multi\_launcher(20261004-165103).zip`



This plan is authoritative for this initiative.



Historical radial plans, candidate reports, M5/M6/M7 procedures, acceptance matrices, native validation campaigns, and previous remediation methodologies are reference material only.



They must \*\*not\*\* be treated as active requirements unless this plan explicitly references them.



\---



\# 1. Goal



Implement two tightly scoped improvements related to the Multi Launcher radial menu and launcher invocation system.



\## Goal A — Mouse Gesture → Radial Menu



Allow the existing Mouse Gestures feature to invoke the Radial Menu through the existing launcher action system.



The desired architecture is:



```text

Right-click gesture

&#x20;       ↓

existing Mouse Gesture recognizer

&#x20;       ↓

existing gesture binding

&#x20;       ↓

existing Action execution path

&#x20;       ↓

existing radial command/action

&#x20;       ↓

existing radial controller

&#x20;       ↓

radial menu appears

```



Do \*\*not\*\* create another mouse gesture implementation or another right-mouse-button hook.



\---



\## Goal B — Reliable Repeated Launcher Hotkey Toggle



Fix the current defect where the launcher hotkey can display Multi Launcher but cannot reliably hide it again while Multi Launcher itself remains focused.



Required behavior:



```text

Launcher hidden

&#x20;   ↓

short launcher hotkey

&#x20;   ↓

Launcher visible



Launcher visible + focused

&#x20;   ↓

same short launcher hotkey

&#x20;   ↓

Launcher hidden



Launcher hidden

&#x20;   ↓

same short launcher hotkey

&#x20;   ↓

Launcher visible

```



This must continue working through repeated complete press/release cycles.



The user must \*\*not\*\* need to:



\- press Escape;

\- click another application;

\- move focus;

\- insert artificial delays.



The existing Dygma/Vial keyboard-macro use case must continue to work.



\---



\# 2. Engineering Philosophy for This Plan



This is intentionally a \*\*surgical implementation\*\*.



Do enough investigation to identify the correct owner.



Do enough implementation to fix the requested behavior correctly.



Do enough testing to establish that the touched behavior works.



Then stop.



This is \*\*not\*\*:



\- a radial reliability initiative;

\- an application-wide regression initiative;

\- a new input architecture;

\- a rewrite of Mouse Gestures;

\- a rewrite of the radial menu;

\- a revival of previous M5/M6/M7 work;

\- a request to validate every historical radial scenario.



Do not broaden the scope merely because additional cleanup or testing is possible.



\---



\# 3. Agent Orchestration



The parent Codex agent is the \*\*orchestrator\*\*.



Use the available specialized agents where useful:



\- planner;

\- implementer;

\- reviewer.



Do not create excessive agent loops.



The expected flow is:



```text

Orchestrator

&#x20;   ↓

Focused planner inspection

&#x20;   ↓

Implementation

&#x20;   ↓

Targeted verification

&#x20;   ↓

Focused reviewer

&#x20;   ↓

Remediation only if reviewer finds a concrete issue

&#x20;   ↓

Done

```



Do not create:



\- candidate 1 / candidate 2 / candidate 3 cycles;

\- repeated qualification rounds;

\- repeated independent reviewers;

\- broad validation campaigns.



\---



\# 4. Global Constraints



\## 4.1 Preserve existing architecture where possible



Before adding anything new, determine whether an existing abstraction already supports the requirement.



Prefer reuse over parallel architecture.



\---



\## 4.2 Fix root causes



For the launcher toggle defect, find the first ownership boundary where the expected event/state transition fails.



Fix the defect at that boundary.



Do not compensate farther downstream if the upstream owner is known to be incorrect.



\---



\## 4.3 No unrelated refactoring



Do not refactor neighboring systems merely because they could be improved.



Only make adjacent changes that are genuinely necessary for:



\- correctness;

\- compilation;

\- integration;

\- maintainability of the requested change;

\- targeted testability.



\---



\## 4.4 Existing behavior stays intact



Unless this plan explicitly changes behavior, preserve it.



Especially preserve:



\- launcher grid behavior;

\- radial hold behavior;

\- configured hold threshold;

\- mouse gesture recognition semantics;

\- existing gesture bindings;

\- persisted radial configuration;

\- persisted gesture configuration;

\- injected/macro keyboard input support;

\- independent Radial Designer state.



\---



\# 5. Non-Goals



The following are explicitly outside this plan.



Do not:



\- redesign the radial menu;

\- redesign the Mouse Gestures plugin;

\- create a second mouse hook;

\- create a second keyboard hook;

\- create another global input service;

\- replace the existing action model;

\- redesign radial menu authoring;

\- redesign radial skins;

\- redesign the Radial Designer;

\- add new radial animations;

\- change radial ring layout;

\- fix unrelated radial bugs;

\- refactor Multi Manager;

\- refactor MkMacro;

\- refactor launcher search;

\- perform general performance work;

\- perform repository-wide test cleanup;

\- revive old radial candidate qualification infrastructure;

\- automate every native input behavior;

\- perform pixel-level GUI testing.



\---



\# 6. Expected Existing Architecture



The following paths were identified from the source-of-truth repository and should be confirmed before editing.



Do not blindly assume symbol names remain identical if the current checkout has changed.



\---



\## 6.1 Mouse Gesture execution path



The existing Mouse Gestures system appears to already execute normal launcher actions.



Expected conceptual path:



```text

Mouse gesture recognized

&#x20;       ↓

gesture BindingEntry

&#x20;       ↓

stored Action

&#x20;       ↓

WatchEvent::ExecuteAction

&#x20;       ↓

normal action activation

&#x20;       ↓

launcher command/action handler

```



Relevant areas are expected to include:



```text

src/mouse\_gestures/db.rs

src/mouse\_gestures/service.rs

src/gui/mouse\_gestures\_dialog.rs

src/gui/watch.rs

```



\---



\## 6.2 Existing radial action path



The Radial plugin already appears to expose normal launcher commands/actions such as:



```text

radial

radial show <menu>

radial close

radial edit

radial skins

```



Relevant areas are expected to include:



```text

src/plugins/radial.rs

src/commands/parser.rs

src/commands/handlers/radial.rs

```



The implementation should reuse these paths.



\---



\## 6.3 Launcher invocation path



The launcher hotkey already appears to use the established launcher invocation/input architecture.



Expected conceptual path:



```text

Windows keyboard hook

&#x20;       ↓

launcher invocation state/adapter

&#x20;       ↓

ToggleLegacyLauncher

&#x20;       ↓

controller/main event handling

&#x20;       ↓

ordered visibility request

&#x20;       ↓

root viewport visibility

```



Relevant areas may include:



```text

src/hotkey/launcher\_invocation.rs

src/main.rs

src/visibility.rs

```



Other directly connected modules may also be relevant.



Inspect before editing.



\---



\# 7. Milestone 0 — Confirm Active Repository Policy



\## Objective



Ensure current repository instructions match the scoped methodology required by this plan.



\---



\## Required actions



1\. Read the repository `AGENTS.md`.

2\. Confirm that the active version:

&#x20;  - uses scope-proportionate verification;

&#x20;  - does not mandate the old M5–M7 radial workflow;

&#x20;  - does not require a full `cargo nextest run` after every substantial change;

&#x20;  - treats historical plans as non-authoritative unless adopted by the active task.

3\. If the repository still contains the obsolete version of `AGENTS.md`, update it to the newer scoped policy before implementation proceeds.



Do not delete historical plan files merely because they contain old methodologies.



\---



\## Done criteria



\- The active agent policy supports targeted verification.

\- No old initiative-specific policy can silently force this task into candidate cycles or exhaustive testing.



\---



\# 8. Milestone 1 — Focused Architecture Reconnaissance



\## Objective



Confirm the exact current ownership paths before implementation.



This milestone is read-only unless an obvious stale documentation mismatch requires correction.



\---



\## Planner assignment



The planner should inspect only enough code to answer the following questions.



\### Mouse Gesture questions



1\. How does a recognized mouse gesture resolve its configured action?

2\. Does a gesture already support the generic `Action` representation?

3\. Where does `WatchEvent::ExecuteAction` or its current equivalent get consumed?

4\. Can a radial command already travel through that path unchanged?

5\. Does the Mouse Gesture action picker already expose registered plugin commands?

6\. Is the radial plugin already visible there?

7\. If not visible, is the problem:

&#x20;  - filtering;

&#x20;  - labeling;

&#x20;  - registration;

&#x20;  - action enumeration;

&#x20;  - another small UI issue?



\### Launcher hotkey questions



Trace one complete short launcher invocation.



Identify:



1\. where low-level key input enters;

2\. where the launcher chord is recognized;

3\. where tap-vs-hold is classified;

4\. where a short tap becomes the root-launcher toggle request;

5\. where that request changes root viewport visibility;

6\. how the state is reset after all keys release.



Then identify the most likely owner of:



> second complete short invocation being ignored while the launcher has focus.



\---



\## Deliverable



The planner should provide the implementer with a concise packet containing:



\- exact owner of mouse gesture action dispatch;

\- exact radial action/command route;

\- whether production changes are actually needed for gesture execution;

\- exact suspected or confirmed owner of the repeated-toggle failure;

\- files/symbols that need modification;

\- files inspected but determined unnecessary;

\- exact targeted tests to reuse or modify;

\- implementation non-goals.



\---



\## Constraints



Do not:



\- run the full test suite;

\- start historical native acceptance tooling;

\- inspect every radial module;

\- inspect unrelated plugins;

\- pre-implement the solution during planning.



\---



\## Done criteria



The implementer can explain both required paths end-to-end without guessing.



\---



\# 9. Milestone 2 — Mouse Gesture → Radial Integration



\## Objective



Allow Mouse Gestures to invoke radial menus through the existing generic action architecture.



\---



\## Preferred implementation



Reuse:



```text

gesture

→ generic Action

→ normal action execution

→ radial command

→ radial controller

```



A radial invocation should \*\*not\*\* become a special low-level gesture concept.



\---



\## Required behavior



The Mouse Gesture configuration UI should support selecting an action equivalent to:



```text

Radial

└── Show default radial menu

```



Where naturally supported by the existing plugin/action UI, also expose specific radial menus:



```text

Radial

├── Show default radial menu

├── Show Bookmarks

├── Show Windows

├── Show Notes

└── ...

```



Exact labels should follow existing UI conventions.



Do not invent an entirely new picker UI for this task.



\---



\## Gesture timing



Default required behavior:



```text

RButton down

&#x20;   ↓

movement

&#x20;   ↓

gesture recognized/tracked

&#x20;   ↓

RButton up

&#x20;   ↓

gesture action executes

&#x20;   ↓

radial appears

```



The radial should therefore open after the gesture completes.



Do not change this to an early-open/while-RButton-held interaction model as part of this task.



\---



\## Preserve existing gesture semantics



Do not change:



\- recognition thresholds;

\- direction classification;

\- capture behavior;

\- RButton lifecycle;

\- matched gesture swallowing;

\- unmatched right-click passthrough;

\- gesture database schema;



unless inspection proves a minimal related change is necessary.



\---



\## Architecture requirements



Do not add:



```text

RadialGestureService

RadialMouseHook

RadialRightClickListener

RadialGestureRecognizer

```



or equivalent parallel architecture.



Do not add a second right-click hook.



Do not duplicate radial controller logic in Mouse Gestures.



\---



\## If it already works



If the source proves:



```text

mouse gesture binding

→ radial command

```



already functions correctly end-to-end, do \*\*not\*\* rewrite it.



Make only whatever minimal discoverability/UI integration is genuinely missing.



If no production change at all is needed, document that finding and do not manufacture code changes.



\---



\## Targeted tests



Use existing generic gesture-action tests wherever possible.



Add or change a test only if new production behavior is introduced.



Potential narrow invariant:



```text

gesture binding configured with radial action

→ recognized gesture

→ expected Action/ExecuteAction emitted

```



Do not build an automated native RButton → actual radial-window acceptance harness for this feature.



\---



\## Done criteria



\- A normal Mouse Gesture binding can invoke the default radial menu.

\- Specific radial menus can be selected where the existing radial command/action enumeration naturally permits it.

\- Existing generic action dispatch is reused.

\- No new mouse input architecture exists.



\---



\# 10. Milestone 3 — Reproduce the Focused Launcher Toggle Failure



\## Objective



Identify exactly where the second launcher invocation is lost.



Do this before redesigning anything.



\---



\## Exact reproduction



Use the current configured launcher hotkey semantics.



The known user configuration is typically a macro emitting:



```text

Shift + Alt + Win + End

```



with all keys subsequently released.



The implementation must remain generic; do not hard-code this chord.



Reproduce conceptually:



```text

1\. Launcher begins hidden.



2\. Complete short launcher chord:

&#x20;  key down sequence

&#x20;  ...

&#x20;  key release sequence



3\. Launcher becomes visible.



4\. Do not press Escape.



5\. Do not click another window.



6\. Leave the root launcher focused.



7\. Complete the same launcher chord again.



EXPECTED:

Launcher becomes hidden.



CURRENT FAILURE:

Launcher may remain visible.



8\. Complete the chord again.



EXPECTED:

Launcher becomes visible.



9\. Continue several cycles.



EXPECTED:

Every complete short cycle alternates root visibility.

```



\---



\# 11. Milestone 4 — Trace the Launcher Invocation Boundary



\## Objective



Determine the first incorrect boundary in the second focused invocation.



\---



\## Diagnostic boundary A — Low-level input



Determine whether the existing global keyboard input mechanism receives the second complete chord while Multi Launcher is focused.



\### If NO



Inspect:



\- hook lifetime;

\- hook ownership;

\- event filtering;

\- focused-process filtering;

\- injected-event filtering;

\- modifier state;

\- release processing.



Do not immediately replace the input mechanism.



\### If YES



Continue downstream.



\---



\## Diagnostic boundary B — Gesture-cycle classification



Determine whether the invocation state machine recognizes the second complete press/release cycle.



Check:



\- all relevant keys transition back to released;

\- state resets after a completed invocation;

\- short-tap readiness is restored;

\- hold state from the previous invocation is cleared;

\- macro/injected event provenance does not leave stale ownership.



\### If this boundary fails



Fix it here.



Do not compensate in visibility handling.



\---



\## Diagnostic boundary C — Toggle event generation



Determine whether the second short invocation produces the expected toggle event/request.



Conceptually:



```text

InvocationIntent::ToggleLegacyLauncher

```



or its current equivalent.



\### If classification succeeds but toggle generation fails



Fix the event handoff at this boundary.



\---



\## Diagnostic boundary D — Visibility request



Determine whether the toggle event becomes the expected ordered root visibility change.



Inspect relevant revision/order/state ownership.



\### If the toggle arrives but no visibility transition is queued



Fix visibility request ownership.



\---



\## Diagnostic boundary E — Root viewport application



Determine whether the root viewport receives the hide request but remains visible because it currently owns focus.



\### If so



Fix the root visibility application/focus interaction.



Do not alter hotkey classification to compensate.



\---



\# 12. Milestone 5 — Implement the Hotkey Root-Cause Fix



\## Objective



Make each complete short launcher invocation independently toggle root launcher visibility regardless of whether the launcher currently owns focus.



\---



\## Required behavior



```text

hidden

→ tap

→ visible



visible + focused

→ tap

→ hidden



hidden

→ tap

→ visible



visible + focused

→ tap

→ hidden

```



This must remain stable across rapid but complete macro cycles.



\---



\## Input provenance



Firmware/macro-generated input must continue to work.



The existing code intentionally supports externally injected input.



Do not add special handling for:



```text

Dygma

Vial

Sofle

specific keyboard models

specific macro software

```



The launcher input layer should simply recognize valid input events.



\---



\## Explicitly prohibited fixes



Do not solve the bug using:



\- a second low-level keyboard hook;

\- `RegisterHotKey` in parallel with the existing mechanism;

\- a focused-window-only keyboard listener;

\- synthetic Escape;

\- simulated mouse clicks;

\- forced focus transfer;

\- arbitrary debounce sleeps;

\- arbitrary timers;

\- polling root focus;

\- duplicate launcher visibility state;

\- special Dygma code;

\- special Vial code.



Do not implement:



```text

if launcher\_is\_focused {

&#x20;   force\_hide();

}

```



at an unrelated downstream location unless root viewport visibility itself is proven to be the actual defective owner.



\---



\## State-cycle invariant



After a complete invocation:



```text

all keys released

\+

tap/hold decision resolved

\+

requested action dispatched

```



the launcher invocation subsystem must be ready for a new independent cycle.



No stale state from the previous invocation may make the next invocation dependent on:



\- focus changing;

\- Escape;

\- another unrelated key;

\- another mouse event.



\---



\# 13. Milestone 6 — Preserve Tap/Hold Radial Semantics



\## Objective



Ensure the hotkey fix does not regress the existing shared launcher/radial invocation behavior.



\---



\## Short press



A complete short invocation before the configured threshold must:



```text

toggle root launcher

```



\---



\## Hold



Crossing the existing configured hold threshold must preserve the current radial behavior.



Do not replace the configured threshold with a hard-coded value.



\---



\## Exclusivity



One completed physical invocation must not execute both:



```text

launcher toggle

```



and:



```text

radial hold action

```



unless existing intentionally configured behavior explicitly requires it.



The normal invariant for this plan is one resolved path per invocation.



\---



\## Designer behavior



An independently open Radial Designer should remain independent from root launcher visibility.



Root launcher toggling should not:



\- destroy the Designer;

\- discard an unsaved Designer draft;

\- implicitly close the Designer.



Do not broaden this task into Designer lifecycle remediation.



\---



\# 14. Milestone 7 — Targeted Automated Verification



\## Objective



Prove only the behavior that this implementation touched.



\---



\# Verification Budget



This is a hard scope constraint.



Do \*\*not\*\* automatically run:



```text

cargo nextest run

```



for the entire repository.



Do \*\*not\*\* run:



\- all-workspace tests;

\- every plugin test;

\- every radial test;

\- old native radial acceptance suites;

\- historical H-series cases;

\- M5/M6/M7 qualification;

\- candidate-cycle validation;

\- pixel-level GUI validation;

\- Multi Manager test suites;

\- MkMacro test suites;

\- unrelated launcher search tests.



Use filtered tests.



\---



\## Test A — Repeated launcher invocation



Use or modify the test closest to the actual defect owner.



Prove conceptually:



```text

initially hidden



cycle 1 short tap

→ visible



cycle 2 short tap while launcher state is visible/focused

→ hidden



cycle 3 short tap

→ visible

```



The exact automated abstraction may represent visibility state rather than native OS focus if the defect is below the native boundary.



Test the layer containing the bug.



Do not duplicate the same invariant through five layers.



\---



\## Test B — Invocation resets between complete cycles



If the root cause involves launcher invocation state, prove that:



```text

complete press/release

→ action resolved

→ invocation state reset

→ next complete press/release accepted

```



If existing tests already prove this after the fix, reuse them.



\---



\## Test C — Tap/hold exclusivity



Only if launcher invocation classification was changed, run or update the existing test proving:



```text

short invocation

→ launcher path



hold past threshold

→ radial path

```



and that one invocation does not incorrectly resolve both paths.



If this code was untouched and an existing relevant test already exists, simply run that test.



\---



\## Test D — Mouse Gesture radial action



If production Mouse Gesture dispatch code changed, prove the narrow new invariant.



For example:



```text

recognized configured gesture

→ radial Action

→ ExecuteAction emitted

```



If the generic action dispatch path was already correct and only UI labeling changed, test only the changed UI/action enumeration logic if such a test adds meaningful value.



Do not create redundant tests just to satisfy a test-count expectation.



\---



\# 15. Milestone 8 — Minimal Manual Smoke Validation



Some real global-input behavior is better checked once manually than recreated through a large native automation framework.



Keep this validation tiny.



\---



\## 15.1 Launcher smoke test



Start with Multi Launcher hidden.



Perform:



```text

launcher hotkey

→ launcher visible



launcher hotkey

→ launcher hidden



launcher hotkey

→ launcher visible



launcher hotkey

→ launcher hidden



launcher hotkey

→ launcher visible

```



Between those presses:



\- do not press Escape;

\- do not click another application;

\- do not intentionally move focus away.



The launcher must toggle every cycle.



\---



\## 15.2 Hold smoke test



Perform one launcher hotkey hold beyond the configured radial threshold.



Confirm:



```text

radial behavior occurs

```



and the invocation does not incorrectly run both tap and hold outcomes.



\---



\## 15.3 Mouse Gesture smoke test



Configure one simple existing gesture, for example:



```text

RButton + drag down

```



to:



```text

Radial → Show default radial menu

```



Perform:



```text

RButton down

→ drag

→ RButton release

```



Confirm:



```text

radial appears once

```



through the normal radial invocation path.



No larger gesture matrix is required.



\---



\# 16. Milestone 9 — Focused Review



\## Objective



Perform one independent review of the completed diff.



The reviewer is not being asked to re-plan the project.



\---



\## Reviewer questions



The reviewer should answer:



\### Architecture



1\. Did Mouse Gestures reuse the existing generic Action system?

2\. Did Radial invocation reuse the existing radial command/controller path?

3\. Was another mouse hook avoided?

4\. Was another keyboard hook avoided?

5\. Was duplicate visibility/input state avoided?



\### Hotkey fix



6\. Was the repeated-toggle defect fixed at the first incorrect owner?

7\. Is the next complete invocation independent of focus changes and Escape?

8\. Is externally injected/macro input still supported?



\### Tap/hold



9\. Are short-tap and hold paths still resolved correctly?

10\. Can one physical invocation accidentally execute both paths?



\### Scope



11\. Is there any unrelated refactoring?

12\. Was any historical validation methodology unnecessarily resurrected?

13\. Were tests kept inside the specified verification budget?



\---



\## Review behavior



If the reviewer finds a concrete correctness issue:



1\. report the exact finding;

2\. return it to the implementer;

3\. fix it;

4\. rerun only directly affected targeted verification;

5\. recheck the relevant review finding.



Do not restart the whole implementation process.



Do not automatically request another complete review cycle for trivial remediation.



\---



\# 17. Milestone 10 — Final Diff and Cleanup



Before completion:



```text

git status

git diff

```



Inspect the complete task diff.



Confirm:



\- no debugging output remains;

\- no temporary diagnostic code remains;

\- no arbitrary sleeps remain;

\- no duplicate input mechanisms were introduced;

\- no unrelated formatting churn exists;

\- no unrelated files were accidentally modified;

\- no old implementation was commented out instead of properly handled;

\- no temporary compatibility path remains unnecessarily.



If commits are part of the current workflow, create coherent commits.



Possible commit structure:



```text

fix(hotkey): allow focused launcher to toggle repeatedly

```



and, if sufficiently distinct:



```text

feat(radial): expose radial invocation to mouse gestures

```



Do not force separate commits if the actual implementation is too tightly coupled for that structure.



\---



\# 18. Completion Criteria



This plan is complete when all of the following are true.



\## Mouse Gesture



\- \[ ] Mouse Gestures can invoke the default radial menu.

\- \[ ] The existing generic Action path is used.

\- \[ ] Specific radial menus can be selected where naturally supported by existing radial action enumeration.

\- \[ ] No second mouse hook was introduced.

\- \[ ] Existing right-click gesture semantics remain intact.



\## Launcher toggle



\- \[ ] Short launcher hotkey from hidden state shows the launcher.

\- \[ ] Same short launcher hotkey while launcher is visible and focused hides it.

\- \[ ] Repeated complete press/release cycles alternate visibility reliably.

\- \[ ] Escape is not required.

\- \[ ] Clicking another application is not required.

\- \[ ] Keyboard macro/injected input continues working.

\- \[ ] No second keyboard listener/hook was introduced.



\## Radial tap/hold



\- \[ ] Existing hold-to-radial behavior remains intact.

\- \[ ] Existing configured threshold remains authoritative.

\- \[ ] One invocation does not accidentally execute both paths.



\## Scope and verification



\- \[ ] Targeted affected tests pass.

\- \[ ] Minimal manual launcher smoke test passes.

\- \[ ] Minimal radial mouse gesture smoke test passes.

\- \[ ] One focused reviewer finds no unresolved substantive issue.

\- \[ ] No unrelated refactor was introduced.

\- \[ ] No historical exhaustive radial validation campaign was run unnecessarily.



\---



\# 19. Stop Condition



Once the completion criteria above are satisfied:



\*\*STOP.\*\*



Do not continue into:



\- optional architecture cleanup;

\- broader radial reliability investigation;

\- application-wide regression testing;

\- full Nextest merely for ceremony;

\- historical candidate validation;

\- additional mouse gesture features;

\- radial Designer redesign;

\- skin work;

\- unrelated input improvements.



Record any worthwhile unrelated observations as optional future work rather than implementing them now.



\---



\# 20. Final Codex Report



At completion, report:



\## Implemented



What changed for:



\- Mouse Gesture → Radial;

\- repeated focused launcher hotkey toggle.



\## Root Cause



Explain the actual cause of the focused-toggle defect and which owner was corrected.



Do not merely describe symptoms.



\## Files Changed



List the meaningful files modified.



\## Tests



List the exact targeted automated tests that were run and their result.



\## Manual Verification



State whether the minimal smoke checks were performed and their results.



If a real-user smoke check is still required, say so clearly.



\## Review



Summarize any substantive reviewer findings and remediation.



\## Explicitly Not Run



State that broad/full regression suites were intentionally not required by this plan, if applicable.



\## Remaining Issues



List only genuine known remaining issues within this scope.



If none exist, say:



> No known remaining issues within the scope of this plan.

