\# Multi Launcher — JSON Utilities and Screen Eyedropper



\## Implementation Plan



\*\*Source of truth:\*\* the current checked-out repository corresponding to `multi\_launcher(20261005-221056).zip`.



\*\*Goal:\*\* add two focused, fully local productivity utilities:



1\. A first-class JSON formatter/minifier.

2\. A screen color eyedropper integrated with the existing `color` plugin.



This is one bounded Codex goal. Do not expand it into the other proposed desktop utilities.



\---



\# 1. Goal Summary



Multi Launcher already contains pieces of both feature areas:



\- Clipboard Modify already implements `json-pretty` and `json-minify`.

\- The existing `color` plugin already accepts colors such as `color #ff0000` and returns HEX/RGB/HSL representations.

\- The repository already contains substantial signed virtual-desktop capture and launcher-parking infrastructure used by Screen Draw and MkMacro.



The implementation should therefore \*\*promote and reuse existing capabilities\*\*, not create parallel JSON or screen-capture systems.



The finished user-facing behavior should be:



```text

json

json format

json pretty

json minify

```



→ opens a compact first-class JSON utility.



And:



```text

color pick

```



→ hides Multi Launcher, presents a frozen-desktop eyedropper with magnification, allows one pixel to be selected, then returns that color through the existing `color` workflow.



Everything remains local to the Windows machine.



\---



\# 2. Locked Requirements



\## 2.1 JSON



The JSON utility must:



\- be a first-class launcher feature;

\- reuse the same underlying JSON transformation behavior as Clipboard Modify;

\- preserve existing `cm json-pretty` and `cm json-minify` behavior;

\- support strict JSON only;

\- support objects, arrays, strings, numbers, booleans, and `null`;

\- pretty-print using conventional 2-space indentation;

\- minify valid JSON;

\- preserve input object ordering where reasonably possible without creating a dangerous application-wide serialization behavior change;

\- keep malformed input intact when parsing fails;

\- show a useful parse error;

\- expose line/column information when the parser provides it;

\- explicitly copy output only when the user requests Copy;

\- initialize from clipboard text only when that clipboard text is valid JSON;

\- otherwise open empty;

\- allow the user to reload/paste from the clipboard;

\- remain a compact utility rather than becoming a full JSON IDE.



Out of scope:



\- JSON5;

\- comments;

\- trailing commas;

\- automatic JSON repair;

\- schema validation;

\- JSONPath;

\- JMESPath;

\- tree editing;

\- structured object editing;

\- opening/saving JSON files;

\- configurable indentation;

\- general clipboard redesign.



\---



\## 2.2 Screen Eyedropper



The eyedropper must:



\- extend the existing `color` feature;

\- preserve existing `color <hex>` behavior;

\- use `color pick` as the canonical screen-picking command;

\- remain discoverable through the normal `color` command family;

\- hide/park the launcher before desktop capture;

\- capture a clean frozen desktop image;

\- sample the frozen image, not a continuously changing live desktop;

\- work across the Windows virtual desktop;

\- support monitors with negative X/Y coordinates;

\- support mixed monitor dimensions and layouts;

\- account correctly for DPI/scaling;

\- show a magnified area around the pointer;

\- clearly indicate the exact sampled pixel;

\- accept the current color with left-click;

\- cancel with Escape;

\- cleanly recover launcher state after cancellation or error;

\- return the selected color through the existing color workflow;

\- expose the existing HEX/RGB/HSL outputs;

\- not silently alter the clipboard when the user merely selects a pixel.



Out of scope:



\- HSV/HSB;

\- persistent recent-color history;

\- favorite colors;

\- palette management;

\- CSS/Rust-specific output expansion;

\- screenshot editing;

\- OCR;

\- region selection;

\- image editing;

\- general Screen Draw changes.



\---



\# 3. Architectural Guardrails



Follow `AGENTS.md`.



In particular:



\- inspect current implementation before editing;

\- maintain one owner for each piece of domain behavior;

\- do not duplicate JSON formatting logic;

\- do not create another Windows desktop compositor solely for the eyedropper;

\- do not route new behavior through arbitrary UI-side string parsing if the typed command bus provides the correct ownership boundary;

\- keep platform behavior outside presentation code;

\- make testable state/coordinate behavior independent from native window creation where practical;

\- preserve unrelated plugin semantics;

\- do not perform opportunistic cleanup.



The existing implementation already provides useful architectural anchors:



\- `src/clipboard\_modify/executor.rs`

&#x20; - owns the current JSON pretty/minify execution behavior.

\- `src/clipboard\_modify/model.rs`

&#x20; - already models `JsonPretty` and `JsonMinify`.

\- `src/clipboard\_modify/catalog.rs`

&#x20; - exposes those operations through Clipboard Modify.

\- `src/plugins/color\_picker.rs`

&#x20; - owns the existing `color` search behavior and HEX/RGB/HSL conversion.

\- `src/mkmacro/screen.rs`

&#x20; - contains shared virtual-desktop capture concepts including signed origins.

\- `src/screen\_draw/capture.rs`

&#x20; - deliberately reuses the shared MkMacro capture backend rather than owning another compositor.

\- `src/screen\_draw/launcher\_parking.rs`

&#x20; - contains existing capture-safe launcher parking/restoration behavior.

\- `src/commands/model.rs`

&#x20; - owns typed commands.

\- `src/commands/parser.rs`

&#x20; - translates persisted/action strings into typed commands.

\- `src/gui/command\_host.rs`

&#x20; - connects typed command handling to `LauncherApp`.



Exact new module/type names may be chosen during implementation after confirming current ownership. Do not invent a parallel architecture merely to match names suggested by this plan.



\---



\# 4. Active Checkpoint Commit Cadence



\*\*Use the project’s active checkpoint commit cadence and define the task-specific commit boundaries in the plan.\*\*



Use an active, milestone-based commit cadence throughout this task.



Break larger milestones into coherent implementation checkpoints such as `M1-A`, `M1-B`, `M2-A`, etc., and commit after each meaningful subsection is complete rather than waiting for an entire large milestone or feature to finish.



Git history should show visible progress and make it easy to understand what was implemented at each stage.



Use judgment on commit size:



\- Do NOT commit every tiny edit or individual line.

\- Do NOT create meaningless WIP/checkpoint commits.

\- Do NOT let several substantial, independently understandable changes accumulate into one very large commit.

\- Before beginning a materially different subsection, prefer committing the previous coherent subsection.



Use descriptive commit messages with the plan-stage identifier:



```text

<type>(<scope>): \[M#-X] <clear description>

```



When useful, include a short commit body explaining:



\- what changed;

\- why;

\- important architecture decisions;

\- behavior intentionally preserved.



Do not run expensive full verification before every commit.



Use small/local checks where useful, commit coherent checkpoints, and perform substantive targeted verification at the appropriate verification milestone.



If later testing or review finds a defect, prefer a clearly described follow-up remediation commit rather than silently folding unrelated fixes into an earlier checkpoint.



Do not squash or rewrite checkpoint history unless explicitly requested.



\---



\# 5. Milestone Overview



| Stage | Objective | Natural commit |

|---|---|---|

| M1-A | Establish shared JSON transformation ownership | Yes |

| M1-B | Add first-class JSON command/plugin routing | Yes |

| M1-C | Add compact JSON utility UI and behavior | Yes |

| M2-A | Separate/reuse color conversion domain behavior | Yes |

| M2-B | Build frozen-desktop eyedropper runtime | Yes |

| M2-C | Integrate `color pick` into launcher lifecycle | Yes |

| M3-A | Cross-feature qualification, docs, focused regression coverage | Conditional/coherent |

| M3-R\* | Reviewer/test remediation if findings exist | Only when needed |



The intent is approximately \*\*6 meaningful implementation commits\*\*, not one commit per tiny code edit and not one giant commit for the entire goal.



\---



\# 6. M1 — First-Class JSON Utility



\## M1-A — Establish Shared JSON Transformation Ownership



\### Objective



Ensure Clipboard Modify and the new JSON utility use the same JSON formatting/minification implementation.



There must be exactly one domain-level source of truth for:



\- strict JSON parsing;

\- pretty formatting;

\- minification;

\- structured error information needed by callers.



\### Relevant Current State



Clipboard Modify currently performs JSON work directly in `src/clipboard\_modify/executor.rs`:



\- `OperationId::JsonPretty`

\- `OperationId::JsonMinify`



Both parse through `serde\_json`.



The existing pipeline behavior must remain stable.



\### Required Work



1\. Inspect the current Clipboard Modify executor and tests around both JSON operations.



2\. Identify the narrowest reusable JSON transformation boundary.



3\. Extract or introduce shared JSON utility behavior outside the Clipboard Modify UI/plugin-specific layer.



4\. Make Clipboard Modify call that shared behavior.



5\. Preserve Clipboard Modify's existing error wrapping and stage diagnostics.



6\. Expose enough structured failure information for the future JSON UI to display useful parse errors without requiring the UI to parse error strings.



7\. Verify strict JSON behavior for:

&#x20;  - object;

&#x20;  - array;

&#x20;  - string;

&#x20;  - number;

&#x20;  - boolean;

&#x20;  - null;

&#x20;  - malformed JSON;

&#x20;  - valid JSON followed by unexpected trailing data.



8\. Address key ordering carefully.



\### Key-Ordering Constraint



Do \*\*not\*\* enable a global `serde\_json` behavior change merely to preserve JSON object order unless repository inspection establishes that it is safe.



If the current `serde\_json::Value` path reorders object keys, prefer a local JSON transformation strategy that preserves source ordering without changing unrelated application serialization semantics.



The user requirement is:



> Preserve object key ordering from the input where reasonably possible.



A local implementation is preferable to a global dependency feature that changes unrelated persistence behavior.



\### Tests



Add or migrate focused tests covering:



\- pretty object formatting;

\- pretty arrays;

\- primitive values;

\- minification;

\- escaped strings;

\- nested values;

\- malformed input;

\- line/column reporting where supported;

\- key-order behavior;

\- existing Clipboard Modify pipeline compatibility.



\### Invariants



\- `cm json-pretty` remains available.

\- `cm json-minify` remains available.

\- saved Clipboard Modify pipelines continue resolving the existing operation IDs.

\- serialization names such as `json-pretty` / `json-minify` do not change.

\- unrelated Clipboard Modify operations are not refactored.



\### Done Criteria



\- shared JSON transformation owner exists;

\- Clipboard Modify uses it;

\- existing JSON operation semantics remain compatible;

\- focused JSON tests pass.



\### Commit Boundary



Commit before beginning launcher/UI work.



Suggested commit:



```text

refactor(json): \[M1-A] centralize JSON transformation behavior

```



\---



\## M1-B — Add First-Class JSON Commands and Plugin Routing



\### Objective



Make JSON formatting/minification directly discoverable from the launcher without requiring knowledge of Clipboard Modify syntax.



\### Required User-Facing Queries



At minimum:



```text

json

json format

json pretty

json minify

```



Behavior:



\- `json` → open general JSON utility;

\- `json format` → open the same utility with Format as the intended workflow;

\- `json pretty` → alias of Format;

\- `json minify` → open the same utility with Minify as the intended workflow.



Do not make large inline JSON query strings the primary interface.



\### Required Work



1\. Inspect current plugin search/command registration conventions.



2\. Add a focused JSON plugin/command surface.



3\. Integrate it with the typed command bus.



4\. Avoid encoding JSON UI semantics through ad-hoc special-case handling in general GUI code.



5\. Give the typed command model enough information to express:

&#x20;  - open JSON utility;

&#x20;  - open preferring Format;

&#x20;  - open preferring Minify.



6\. Add parser and handler routing consistent with the repository's current command architecture.



7\. Register the JSON plugin alongside built-in plugins.



8\. Ensure normal command completion/discovery exposes the new command family.



\### Invariants



\- `cm ...` remains separate and unchanged.

\- no Clipboard Modify command is removed.

\- no existing plugin prefix is repurposed.

\- the JSON plugin should not write the clipboard simply because its command was executed.



\### Tests



Cover:



\- plugin command discovery;

\- `json` query routing;

\- `json format`;

\- `json pretty`;

\- `json minify`;

\- typed command parsing;

\- appropriate handler dispatch;

\- no collisions with unrelated plugin queries.



\### Done Criteria



The launcher can discover and route all JSON utility commands to a typed JSON utility intent.



The full UI does not have to be polished until M1-C.



\### Commit Boundary



Suggested commit:



```text

feat(json): \[M1-B] add first-class JSON launcher commands

```



\---



\## M1-C — Implement the JSON Utility UI



\### Objective



Provide the compact user-facing formatter/minifier.



\### Required UX



The utility should contain:



\- editable JSON text;

\- Format / Pretty action;

\- Minify action;

\- Copy Result;

\- Reload/Paste from Clipboard;

\- Clear/reset;

\- visible parse/validation error state.



Keep the design compact and consistent with existing egui dialogs.



Do not build a JSON IDE.



\### Opening Behavior



On each intentional open:



1\. inspect clipboard text;

2\. if the clipboard contains valid strict JSON:

&#x20;  - populate the editor with it;

3\. otherwise:

&#x20;  - start with an empty editor.



Do not fill the editor with arbitrary invalid clipboard text by default.



The user must also have an explicit way to load current clipboard contents while the utility is already open.



\### Format / Minify Behavior



On success:



\- transform the utility's editable buffer;

\- show the resulting JSON in the utility;

\- clear stale parse errors;

\- do \*\*not\*\* copy automatically.



On failure:



\- leave the exact input intact;

\- display a clear parser error;

\- include line/column when available;

\- do not alter clipboard contents.



\### Copy Behavior



`Copy Result` is explicit.



Copy the current transformed/valid editor contents only as a direct user action.



Provide normal success feedback using existing application patterns if appropriate, without adding a new notification framework.



\### Focus / Keyboard Expectations



Use established dialog behavior where possible.



At minimum ensure:



\- the text editor can be used normally;

\- Escape follows existing dialog-close conventions;

\- buttons are keyboard reachable;

\- opening the dialog does not unexpectedly mutate the main launcher query or clipboard.



Do not create a new application-wide shortcut standard during this goal.



\### Tests



Separate pure UI state behavior from rendering where practical.



Cover:



\- valid clipboard initialization;

\- invalid clipboard → empty state;

\- format success;

\- minify success;

\- parse failure preserves buffer;

\- parse failure exposes useful location data;

\- clear;

\- reload clipboard;

\- explicit copy behavior;

\- switching between format and minify in one session.



\### Done Criteria



A user can invoke:



```text

json

```



paste/edit JSON, format or minify it, see parse failures, and explicitly copy the result.



Existing Clipboard Modify JSON operations remain functional.



\### Commit Boundary



Suggested commit:



```text

feat(json): \[M1-C] add JSON formatter and minifier utility

```



\---



\# 7. M2 — Screen Color Eyedropper



\## M2-A — Make Existing Color Conversion Reusable



\### Objective



Prepare the existing `color` plugin so a color selected from the screen can enter exactly the same HEX/RGB/HSL result path as a manually typed color.



\### Relevant Current State



`src/plugins/color\_picker.rs` currently owns:



\- HEX parsing;

\- HEX output;

\- RGB output;

\- HSL conversion;

\- `color` result generation.



The screen picker should not duplicate these calculations.



\### Required Work



1\. Inspect existing color parsing/conversion tests and plugin behavior.



2\. Separate pure color parsing/representation/output behavior from launcher-query-specific behavior where necessary.



3\. Preserve:



```text

color

color #ff0000

```



and any existing settings behavior.



4\. Ensure an RGB color selected by the future picker can be converted to the canonical color representation expected by the existing plugin.



5\. Ensure one shared path generates:

&#x20;  - HEX;

&#x20;  - RGB;

&#x20;  - HSL.



6\. Add `color pick` discoverability, but do not launch native picker behavior until the typed command integration is ready.



\### Tests



Cover:



\- existing HEX parsing;

\- canonical HEX formatting;

\- RGB output;

\- HSL output;

\- representative edge colors;

\- existing `color #...` query behavior;

\- `color pick` being treated as a command rather than rejected as an invalid HEX color.



\### Invariants



\- existing color conversion output remains compatible;

\- do not broaden formats;

\- do not add color history/settings.



\### Done Criteria



The color plugin has one reusable representation/conversion path suitable for both typed colors and picked pixels.



\### Commit Boundary



Suggested commit:



```text

refactor(color): \[M2-A] share color conversion behavior with screen picking

```



\---



\## M2-B — Implement the Frozen-Desktop Eyedropper Runtime



\### Objective



Create the actual native screen-picking interaction.



This milestone owns desktop capture, coordinate mapping, picker interaction, magnification, accept/cancel, and lifecycle events.



It does \*\*not\*\* yet need to own final launcher query restoration; that integration is M2-C.



\### Existing Infrastructure to Reuse



Inspect before adding anything new:



\- `crate::mkmacro::screen::WindowsScreenCaptureBackend`

\- `ScreenCaptureBackend`

\- `CapturedRegion`

\- `ScreenRect`

\- signed virtual-desktop coordinate handling

\- Screen Draw's `ScreenDrawCaptureBackend`

\- Screen Draw launcher parking/restoration behavior

\- existing native overlay/window infrastructure where applicable



Important existing architectural precedent:



> Screen Draw intentionally reuses the shared MkMacro virtual-desktop compositor rather than owning another monitor capture implementation.



The eyedropper should follow the same principle.



\### Capture Requirements



The sequence must conceptually be:



```text

request picker

&#x20;   ↓

make launcher capture-safe

&#x20;   ↓

capture complete virtual desktop

&#x20;   ↓

show picker using frozen pixels

&#x20;   ↓

sample frozen capture only

```



The picker must not sample its own UI from the live desktop.



\### Launcher Capture Safety



Do not simply hide the egui contents and immediately assume the launcher is absent from the captured pixels.



Reuse or generalize the repository's established capture-safe parking/visibility mechanism.



If shared functionality must be extracted from Screen Draw:



\- make the extraction narrowly reusable;

\- preserve Screen Draw semantics;

\- add regression tests around the extracted boundary;

\- do not broadly refactor Screen Draw.



\### Coordinate Model



The captured image needs:



\- signed virtual-desktop origin;

\- dimensions;

\- deterministic conversion from desktop point → capture-local pixel.



The conversion must work for:



```text

primary monitor:       x >= 0

monitor to the left:   x < 0

monitor above:         y < 0

```



Never cast signed desktop coordinates to unsigned values before applying the capture origin.



\### Picker Visuals



The picker should provide:



\- desktop image as frozen background;

\- cursor/crosshair or equivalent target marker;

\- magnified sample near the pointer;

\- clear center pixel indicator;

\- currently selected color representation if useful.



The magnifier should remain usable near desktop edges rather than reading outside the captured image.



Keep visuals minimal.



Do not turn the picker into Screen Draw.



\### Input



Required:



\- mouse movement updates hovered pixel;

\- left-click accepts;

\- Escape cancels.



Ignore unrelated clicks/keys unless required by the native overlay implementation.



\### Result Model



The runtime should produce a clear outcome such as:



```text

Picked(color)

Cancelled

Failed(error)

```



Do not make GUI code infer state by inspecting native windows.



\### DPI / Monitor Requirements



Use the application's existing Windows DPI-awareness assumptions.



Validate coordinate conversion against actual physical capture pixels.



Do not assume:



```text

egui logical point == desktop capture pixel

```



Mixed-DPI behavior must be deliberately handled at the native boundary.



\### Automated Tests



Push as much correctness as possible below the native window layer.



Test:



\- signed virtual desktop;

\- origin translation;

\- pixel lookup;

\- negative coordinates;

\- top-left and bottom-right edges;

\- out-of-bounds rejection/clamping policy;

\- magnifier extraction at center;

\- magnifier extraction at edges;

\- exact chosen pixel;

\- accept transition;

\- cancel transition;

\- error transition;

\- capture ownership/recovery state.



Use fake capture/native boundaries where appropriate.



\### Done Criteria



A testable eyedropper runtime exists that can:



\- consume a frozen signed-desktop capture;

\- track the pointer;

\- render magnification;

\- select the exact pixel;

\- return Picked/Cancelled/Failed.



\### Commit Boundary



Suggested commit:



```text

feat(color): \[M2-B] add frozen-desktop screen eyedropper runtime

```



\---



\## M2-C — Integrate `color pick` with Launcher Lifecycle



\### Objective



Connect the native picker to the existing `color` launcher workflow.



\### Desired Happy Path



```text

color pick

&#x20;   ↓

launcher enters capture-safe state

&#x20;   ↓

desktop captured

&#x20;   ↓

picker opens

&#x20;   ↓

left-click selects pixel

&#x20;   ↓

picker closes

&#x20;   ↓

launcher is restored

&#x20;   ↓

selected color flows into existing `color` results

&#x20;   ↓

HEX / RGB / HSL are available to choose/copy

```



The preferred integration is to return through the existing color query/results path rather than creating a second result UI.



For example, if consistent with current launcher ownership, the selected RGB value may be normalized into the equivalent:



```text

color #RRGGBB

```



query after restoration.



The implementation may use an equivalent typed/state-based handoff if that fits the current architecture better.



The key invariant is:



> A screen-picked color and a manually entered color must ultimately use the same result-generation behavior.



\### Cancellation



Escape must:



\- close the picker;

\- not copy anything;

\- not change the selected color permanently;

\- not leave native overlay windows behind;

\- restore launcher state safely.



Where practical, restore the launcher/query state that existed before the picker started.



\### Failure



Capture or native-window failures must:



\- terminate the picker session;

\- restore launcher visibility/ownership;

\- leave the clipboard unchanged;

\- report an actionable error through established UI/error mechanisms;

\- not strand the application offscreen.



\### Reentrancy



Ensure repeated `color pick` activation does not create concurrent picker sessions.



Define deterministic behavior if a second request arrives while a picker is active:



\- ignore/reject;

\- or focus the active picker;



whichever best matches existing application conventions.



Do not permit two capture/parking owners to fight over launcher restoration.



\### Typed Command Integration



Add a proper typed command path for the picker.



Do not implement the feature solely as a special string comparison buried in the main UI render loop.



Update:



\- command model;

\- command parser;

\- command dispatch/host ownership;

\- plugin action generation;



as required by the current command architecture.



\### Tests



Cover:



\- `color pick` command parsing;

\- plugin discovery;

\- correct command dispatch;

\- successful result → existing color query/results;

\- cancellation;

\- failure recovery;

\- duplicate activation handling;

\- clipboard remains unchanged until the user selects a HEX/RGB/HSL result.



\### Done Criteria



`color pick` behaves as a complete launcher command and returns selected colors through the established color results.



\### Commit Boundary



Suggested commit:



```text

feat(color): \[M2-C] integrate screen picking with the color command

```



\---



\# 8. M3 — Qualification, Documentation, and Review



\## M3-A — Focused Qualification



\### Objective



Verify Goal A as a cohesive feature without converting this task into a repository-wide regression campaign.



\### JSON Verification



At minimum verify:



\- `json` command discoverability;

\- `json format`;

\- `json pretty`;

\- `json minify`;

\- valid clipboard initialization;

\- invalid clipboard behavior;

\- pretty formatting;

\- minification;

\- primitive JSON;

\- nested JSON;

\- malformed JSON;

\- error line/column;

\- explicit clipboard copy;

\- existing `cm json-pretty`;

\- existing `cm json-minify`.



\### Color Verification



At minimum verify:



\- `color #ff0000` still works;

\- `color pick` is discoverable;

\- launcher does not appear in the frozen capture;

\- left-click chooses the intended pixel;

\- returned results include HEX/RGB/HSL;

\- no automatic clipboard mutation on pixel selection;

\- Escape cancels;

\- picker can be opened again after cancellation;

\- picker can be opened again after successful completion;

\- negative virtual-desktop coordinate logic is covered automatically;

\- mixed-monitor geometry is covered automatically.



\### Native Smoke Verification



If the current environment provides a usable Windows desktop, perform a small native sanity pass for:



1\. ordinary single-monitor selection;

2\. Escape cancellation;

3\. repeat open/close;

4\. multi-monitor selection when multiple monitors are available.



Do not build a large manual acceptance matrix merely for this goal.



Native UI behavior should primarily be protected through testable state/geometry layers.



\### Test Command Philosophy



Prefer targeted verification.



Examples may include:



```text

cargo nextest run <json-related filter>

cargo nextest run <color/eyedropper-related filter>

cargo nextest run <clipboard-modify-related filter>

cargo test <specific target/filter>

cargo check

```



Use the actual available test names discovered in the repository.



Do not blindly copy placeholder filters.



A full:



```text

cargo nextest run

```



is not automatically required.



Run broader verification only if:



\- shared infrastructure changes justify it;

\- targeted results indicate possible broader breakage;

\- or the active repository instructions require it.



\### Documentation



Update README/help/command documentation where the project currently documents built-in commands.



Document at least:



```text

json

json format

json minify

color pick

```



Keep documentation proportional.



\### Commit Behavior



If qualification reveals no source/document changes, do not create an empty checkpoint commit.



If documentation and final focused tests form a meaningful final change, use:



```text

docs(utilities): \[M3-A] document JSON and screen color utilities

```



or an equivalent accurate subject.



\---



\# 9. Independent Review



After implementation and targeted verification, use a reviewer agent.



The reviewer should focus on:



\- duplicated JSON ownership;

\- accidental Clipboard Modify behavior change;

\- unexpected serde/serialization behavior change;

\- command-routing consistency;

\- launcher lifecycle recovery;

\- capture ownership;

\- signed coordinate correctness;

\- DPI assumptions;

\- native resource cleanup;

\- clipboard side effects;

\- scope creep.



The reviewer should \*\*not\*\* redesign the feature or demand unrelated cleanup.



\## Review Remediation



If the reviewer identifies a concrete issue:



1\. assign it to the appropriate implementer;

2\. fix it at the correct ownership boundary;

3\. run the smallest meaningful verification;

4\. create a follow-up checkpoint commit.



Use IDs such as:



```text

M3-R1

M3-R2

```



Examples:



```text

fix(color): \[M3-R1] restore launcher after picker capture failure

```



```text

fix(json): \[M3-R2] preserve source key ordering during formatting

```



Do not amend or silently rewrite earlier checkpoint commits merely to make the history look cleaner.



\---



\# 10. Explicit Non-Goals



Do not add any of the following:



\- OCR;

\- selected-text actions;

\- regex tester;

\- QR generator;

\- date arithmetic;

\- unit conversion work;

\- smart clipboard detection;

\- clipboard power actions;

\- color history;

\- favorite colors;

\- JSON history;

\- JSON file editing;

\- JSON schemas;

\- JSON tree view;

\- JSON repair;

\- screen ruler;

\- screenshot editor changes;

\- Screen Draw feature work;

\- MkMacro feature work;

\- action-sheet redesign;

\- general keyboard-shortcut standardization;

\- workspaces.



Do not broaden Goal A even if one of these appears easy while touching adjacent code.



Record useful future ideas separately.



\---



\# 11. Expected Checkpoint History



The exact subjects may change slightly to match the final implementation, but the intended shape is:



```text

refactor(json): \[M1-A] centralize JSON transformation behavior



feat(json): \[M1-B] add first-class JSON launcher commands



feat(json): \[M1-C] add JSON formatter and minifier utility



refactor(color): \[M2-A] share color conversion behavior with screen picking



feat(color): \[M2-B] add frozen-desktop screen eyedropper runtime



feat(color): \[M2-C] integrate screen picking with the color command



docs(utilities): \[M3-A] document JSON and screen color utilities

```



Conditional review/remediation commits follow afterward as `M3-R#`.



Do not force exactly seven commits if two adjacent tasks prove inseparable or a listed checkpoint produces no meaningful diff.



The intent is \*\*coherent visible progress\*\*, not an arbitrary commit count.



\---



\# 12. Completion Definition



Goal A is complete when all of the following are true.



\## JSON



\- \[ ] `json` is discoverable.

\- \[ ] `json format` works.

\- \[ ] `json pretty` works.

\- \[ ] `json minify` works.

\- \[ ] A compact JSON utility exists.

\- \[ ] Valid clipboard JSON can initialize it.

\- \[ ] Invalid clipboard text does not automatically populate it.

\- \[ ] Invalid JSON remains editable after failure.

\- \[ ] Useful parse location is displayed.

\- \[ ] Copy is explicit.

\- \[ ] Existing Clipboard Modify JSON behavior remains operational.

\- \[ ] One shared JSON transformation implementation owns the behavior.



\## Screen Eyedropper



\- \[ ] Existing `color <hex>` behavior remains operational.

\- \[ ] `color pick` is discoverable.

\- \[ ] Launcher pixels do not contaminate the frozen capture.

\- \[ ] Full signed virtual-desktop capture is supported.

\- \[ ] Negative monitor coordinates are supported.

\- \[ ] Magnified cursor preview works.

\- \[ ] Exact selected pixel is visually identifiable.

\- \[ ] Left-click accepts.

\- \[ ] Escape cancels.

\- \[ ] Native resources are cleaned up after success/cancel/error.

\- \[ ] Picked colors return through the existing HEX/RGB/HSL workflow.

\- \[ ] Pixel selection alone does not overwrite clipboard contents.



\## Engineering / Workflow



\- \[ ] Goal remains within scope.

\- \[ ] Targeted automated verification passes.

\- \[ ] Appropriate Windows/native smoke testing succeeds where available.

\- \[ ] Reviewer has inspected the completed work.

\- \[ ] Concrete review findings have been remediated.

\- \[ ] README/help is updated appropriately.

\- \[ ] Active checkpoint commits remain visible and unsquashed.

