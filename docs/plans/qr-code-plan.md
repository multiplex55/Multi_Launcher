\# MULTI LAUNCHER — LOCAL QR CODE GENERATOR



\## Source of Truth



Use the current checked-out repository corresponding to:



`multi\_launcher(20261005-221056).zip`



The current repository is authoritative.



Before modifying code, re-inspect the relevant source because the plan describes the intended architecture and behavior, but the checked-out implementation remains the source of truth for exact type names, ownership, and call sites.



Follow `AGENTS.md`.



\---



\# 1. Goal



Add a polished, completely local QR Code Generator to Multi Launcher.



The feature should behave like a first-class built-in utility rather than an external tool or web shortcut.



Primary flows:



```text

qr

```



Opens an empty QR Generator dialog.



```text

qr https://example.com

```



Opens the same dialog with the supplied text already populated and the QR preview generated.



The user must then be able to:



\- edit the source text;

\- see the QR update live;

\- paste text from the Windows clipboard explicitly;

\- copy the encoded source text explicitly;

\- copy the generated QR image to the Windows clipboard explicitly;

\- save the QR as a PNG;

\- inspect payload character/byte count;

\- see useful capacity/error feedback;

\- optionally change QR error-correction level from an Advanced control.



All QR generation must occur locally in-process.



\---



\# 2. Approved Requirements



\## 2.1 Local-only behavior



Required:



\- QR encoding happens entirely locally.

\- No QR generation website.

\- No HTTP request.

\- No cloud API.

\- No remote service.

\- No external QR executable.

\- No runtime dependency on an internet connection.

\- Do not transmit QR payloads through analytics or telemetry.

\- Do not log raw QR payload contents.



A normal Rust QR encoding dependency is acceptable and preferred.



The dependency itself may be obtained normally through Cargo during development/build, but generating a QR at runtime must require no network activity.



\---



\## 2.2 Supported source content



Initial version must support arbitrary textual payloads, including:



\- plain text;

\- URLs;

\- multiline text entered/pasted in the dialog;

\- UTF-8 / Unicode text.



Do not add dedicated structured editors for:



\- Wi-Fi credentials;

\- vCards;

\- email QR schemas;

\- telephone QR schemas;

\- SMS QR schemas.



Those formats can already be represented as ordinary text if the user supplies the encoded payload manually and can be considered future enhancements.



\---



\## 2.3 Invocation behavior



Support:



```text

qr

```



and:



```text

qr <text>

```



Requirements:



\- `qr` opens a fresh empty QR Generator.

\- `qr <text>` opens it populated with `<text>`.

\- The QR preview is generated automatically from non-empty valid input.

\- The input remains editable after invocation.

\- Changing input regenerates the preview live.

\- Multiline input is supported inside the dialog.



Do not automatically read the clipboard when `qr` is invoked.



Instead, expose an obvious \*\*Paste\*\* / \*\*Use Clipboard Text\*\* control.



\---



\## 2.4 Clipboard behavior



Clipboard interaction must always be explicit.



Provide:



\- \*\*Paste\*\* — read current clipboard text into the QR source field.

\- \*\*Copy Text\*\* — copy the exact source payload.

\- \*\*Copy QR\*\* — copy the rendered QR image.



Do not:



\- automatically replace clipboard contents when opening the dialog;

\- automatically copy the generated QR;

\- automatically read clipboard contents at startup;

\- silently mutate clipboard state during ordinary live regeneration.



If clipboard text cannot be read, leave the current QR payload unchanged and report the problem through the normal Multi Launcher error/UI mechanism.



\---



\## 2.5 QR preview



The QR must appear directly inside Multi Launcher.



Requirements:



\- standard black modules on a white background;

\- standards-compliant quiet zone;

\- crisp module boundaries;

\- no blurry interpolation;

\- sensible resizing as the utility window changes size;

\- sufficiently large default display for a smartphone camera to scan from a monitor.



Do not add QR theming in this goal.



No:



\- custom module colors;

\- custom backgrounds;

\- embedded logos;

\- styled dots;

\- decorative corners;

\- gradients;

\- custom quiet-zone controls.



The surrounding dialog should continue using the user's normal Multi Launcher theme.



\---



\## 2.6 Capacity handling



Never truncate source data.



For non-empty payloads:



\- generate the smallest appropriate QR representation supported by the chosen encoder;

\- report when a payload cannot fit at the selected error-correction level;

\- clear/replace stale preview state when the current source cannot be encoded;

\- show the user an actionable capacity message.



Display useful payload metadata, at minimum:



\- character count;

\- UTF-8 byte count.



When generation succeeds, it is useful to additionally expose lightweight QR metadata available naturally from the encoder, such as matrix dimensions/version, but do not turn this into a technical QR inspection tool.



Do not implement multiple chained QR codes for oversized payloads.



One displayed QR must correspond to one complete payload.



\---



\## 2.7 Error correction



Default to QR error-correction level \*\*M / Medium\*\*.



Provide an \*\*Advanced\*\* section that allows the user to choose:



\- L / Low;

\- M / Medium;

\- Q / Quartile;

\- H / High.



Medium remains the default whenever a fresh generator session begins.



The UI should briefly explain that:



\- higher error correction improves recovery/reliability;

\- higher error correction also reduces maximum payload capacity.



This selection is transient for the first version.



Do not add new persisted QR settings merely to remember the last error-correction choice.



\---



\## 2.8 File output



Provide:



\*\*Save PNG\*\*



Requirements:



\- use the application's existing native file-dialog approach;

\- expose PNG as the supported output;

\- produce a standards-compliant crisp raster QR;

\- ensure an appropriate `.png` extension;

\- do not derive filenames from QR payload contents;

\- do not automatically save QR files.



A neutral default filename such as:



`multi\_launcher\_qr.png`



is appropriate.



SVG export is out of scope.



\---



\## 2.9 Window behavior



The QR Generator should be a focused utility dialog/window, consistent with other Multi Launcher utilities.



Required:



\- editable multiline source area;

\- visible QR preview;

\- explicit actions near the preview/input;

\- keyboard usable;

\- sensible initial focus;

\- normal close button behavior;

\- Escape closes according to existing Multi Launcher panel/dialog conventions;

\- no separate task-management/settings-style interface.



Opening and closing the QR Generator must not alter unrelated launcher state.



\---



\# 3. Explicit Non-Goals



Do \*\*not\*\* expand this goal into any of the following:



\- QR code decoding;

\- camera scanning;

\- screen-region QR recognition;

\- QR history;

\- persistent payload library;

\- favorites;

\- automatic payload persistence;

\- Wi-Fi QR form;

\- vCard editor;

\- email/SMS/phone QR builders;

\- SVG export;

\- PDF export;

\- animated QR;

\- multi-QR file transfer;

\- QR sequence/chunk protocol;

\- logo embedding;

\- custom QR colors;

\- QR styling/theme designer;

\- broad Universal Action support;

\- Clipboard History → QR actions;

\- Note → QR actions;

\- Browser Tab → QR actions;

\- File/Folder → QR actions;

\- unrelated clipboard changes;

\- unrelated launcher redesign;

\- unrelated plugin refactors.



If one of these appears easy while implementing the feature, report it as a future enhancement rather than expanding scope.



\---



\# 4. Architectural Direction From Current Source



Current source inspection establishes several useful existing patterns.



Multi Launcher already has:



\- a built-in `Plugin` abstraction;

\- centralized built-in plugin registration in `PluginManager`;

\- the typed command bus;

\- typed interactive-dialog routing;

\- `LauncherApp` ownership of dialog state;

\- centralized panel/open/close tracking;

\- per-frame egui dialog rendering;

\- `arboard` for text/image clipboard access;

\- `image` for image generation/PNG encoding;

\- `rfd` for native save dialogs;

\- existing nearest-neighbor texture handling patterns;

\- existing error/toast reporting paths.



The QR implementation should use those systems.



Do \*\*not\*\* create:



\- a second command dispatcher;

\- global QR state;

\- a QR worker service;

\- a background runtime;

\- a parallel clipboard abstraction solely for QR;

\- a separate native executable.



QR encoding is bounded and small enough to remain ordinary local application work.



\---



\# 5. Intended Ownership



Keep the feature split into three conceptual layers.



\## QR domain/core



Own:



\- error-correction abstraction;

\- QR encoding;

\- payload validation/capacity errors;

\- module/matrix representation;

\- deterministic raster rendering;

\- payload metadata required by UI.



This layer must not depend on `LauncherApp`.



It should be directly unit-testable.



Likely location:



```text

src/qr/

```



or an equivalently appropriate domain module after inspecting current repository conventions.



\---



\## QR plugin / command entry



Own:



\- recognizing `qr`;

\- recognizing `qr <text>`;

\- exposing command metadata;

\- constructing the typed action/command used to open the utility.



Likely location:



```text

src/plugins/qr.rs

```



Register it with existing built-in plugins.



\---



\## QR dialog UI



Own:



\- transient source text;

\- preview texture/cache;

\- Advanced section state;

\- selected error-correction level;

\- Paste;

\- Copy Text;

\- Copy QR;

\- Save PNG;

\- user-facing generation/status messages;

\- keyboard/focus behavior.



Likely location:



```text

src/gui/qr\_dialog.rs

```



`LauncherApp` should own the dialog state just like neighboring first-class utility dialogs.



\---



\# 6. Active Checkpoint Commit Cadence



\*\*Use the project’s active checkpoint commit cadence and define the task-specific commit boundaries in the plan.\*\*



Use an active, milestone-based commit cadence throughout this task.



Break larger milestones into coherent implementation checkpoints such as

`M1-A`, `M1-B`, `M2-A`, etc., and commit after each meaningful subsection is

complete rather than waiting for an entire large milestone or feature to finish.



I want Git history to show visible progress and make it easy to understand what

was implemented at each stage.



Use judgment on commit size:



\- Do NOT commit every tiny edit or individual line.

\- Do NOT create meaningless WIP/checkpoint commits.

\- Do NOT let several substantial, independently understandable changes

&#x20; accumulate into one very large commit.

\- Before beginning a materially different subsection, prefer committing the

&#x20; previous coherent subsection.



Use descriptive commit messages with the plan-stage identifier:



`<type>(<scope>): \[M#-X] <clear description>`



For example:



`refactor(settings): \[M2-B] organize launcher window and appearance controls`



When useful, include a short commit body explaining what changed, why, and what

behavior was intentionally preserved.



Do not run expensive full verification before every commit. Use small/local

checks where useful, commit coherent checkpoints, and perform the plan's

substantive targeted verification at the appropriate verification milestone.



If later testing or review finds a defect, prefer a clearly described follow-up

remediation commit rather than silently folding unrelated fixes into an earlier

checkpoint.



Do not squash or rewrite the checkpoint history unless I explicitly request it.



When creating the implementation plan, explicitly identify natural commit

boundaries and suggested stage IDs/commit subjects.



\---



\# 7. Milestone 1 — Establish the Local QR Core



\## Objective



Create a UI-independent QR generation and rasterization layer capable of converting an arbitrary UTF-8 text payload into a standards-compliant black-and-white QR image.



Do not wire it into the launcher UI yet.



\---



\## M1-A — Introduce QR domain types and encoder



\### Required work



1\. Inspect available Rust QR libraries compatible with the current crate/toolchain.



2\. Prefer a mature, in-process QR encoder such as the Rust `qrcode` ecosystem unless current compatibility inspection identifies a concrete reason to use another local encoder.



3\. Add only the dependency required for QR generation.



4\. Introduce a QR-specific error-correction type rather than allowing raw library enums to leak through the application.



Conceptually:



```rust

enum QrErrorCorrection {

&#x20;   Low,

&#x20;   Medium,

&#x20;   Quartile,

&#x20;   High,

}

```



Default:



```text

Medium

```



5\. Add a pure generation entry point.



It should accept:



\- source string;

\- error-correction level.



It should return either:



\- successfully generated QR information; or

\- a typed failure.



6\. Represent failures clearly enough to distinguish at least:



\- empty/no payload where applicable;

\- data too large/capacity failure;

\- unexpected encoder failure.



An empty source should normally be treated by the UI as "nothing to generate" rather than a scary error.



7\. Encode the exact UTF-8 payload.



Do not normalize, trim, case-fold, or otherwise mutate user contents before encoding.



If the payload is:



```text

&#x20;hello

```



the leading space belongs to the QR.



8\. Do not emit payload data into tracing/error logs.



User-facing errors may describe size/capacity but should not echo potentially sensitive source text unnecessarily.



\### Tests



Add focused unit tests covering:



\- ASCII payload;

\- URL payload;

\- multiline payload;

\- non-ASCII/Unicode payload;

\- all four error-correction choices;

\- source bytes are not mutated;

\- payload too large produces a controlled error rather than panic;

\- default error correction is Medium.



\### Invariants



\- no GUI dependency;

\- no network operation;

\- no clipboard operation;

\- no filesystem operation;

\- no application-global state.



\### Natural commit boundary



\*\*M1-A\*\*



Suggested commit:



```text

feat(qr): \[M1-A] add local QR encoding domain

```



\---



\## M1-B — Add deterministic crisp QR raster rendering



\### Required work



Create a rendering path capable of converting the QR matrix/modules into an `image::RgbaImage` or equivalent reusable raster representation.



Requirements:



1\. Fixed black QR modules.

2\. Fixed white background.

3\. Standards-compliant quiet zone.

4\. Integer module scaling.

5\. No anti-aliasing.

6\. No interpolation that blurs module boundaries.

7\. Square output.

8\. Output suitable for both:

&#x20;  - egui preview;

&#x20;  - clipboard image copy;

&#x20;  - PNG save.



Centralize raster sizing behavior instead of duplicating separate "preview QR" and "saved QR" renderers with different mathematical assumptions.



A dedicated raster scale constant or small render-options structure is appropriate.



The UI may display the raster at different visual dimensions, but the actual generated modules must remain crisp.



\### Metadata



Expose enough metadata for the UI to show:



\- character count;

\- UTF-8 byte count;

\- QR matrix/module dimensions when available naturally.



Do not create an extensive QR diagnostics model.



\### Tests



Test:



\- raster is square;

\- quiet-zone pixels are white;

\- QR module pixels are only expected black/white values;

\- image dimensions correspond to module count + quiet zone × integer scale;

\- deterministic input produces deterministic output dimensions;

\- changing error correction can legitimately alter QR dimensions/capacity;

\- no fractional scaling occurs in exported raster creation.



\### Natural commit boundary



\*\*M1-B\*\*



Suggested commit:



```text

feat(qr): \[M1-B] add crisp QR raster rendering and metadata

```



Before moving to command/UI integration, inspect the M1 diff and ensure the QR core has no launcher/UI ownership.



\---



\# 8. Milestone 2 — Add the QR Plugin and Typed Invocation



\## Objective



Make `qr` and `qr <text>` recognizable first-class launcher commands and route them through the existing typed command architecture.



Do not bypass the command bus with direct GUI mutation from plugin search.



\---



\## M2-A — Add QR built-in plugin



\### Required behavior



Add a built-in QR plugin.



It should:



\- have a stable plugin name;

\- have a concise user-facing description;

\- participate in existing plugin enablement;

\- expose the `qr` command;

\- use `qr` as its query prefix where appropriate.



Query behavior:



\### Exact



```text

qr

```



produces an action equivalent to:



```text

Open QR Generator

```



with no initial payload.



\### Payload



```text

qr hello world

```



produces an action equivalent to:



```text

Generate QR for supplied text

```



with:



```text

hello world

```



as initial input.



Do not put the full arbitrary payload into the action wire identifier.



Use the existing action argument/payload capability or another typed mechanism consistent with the repository.



Avoid inventing delimiter escaping for arbitrary text when the application already has structured argument transport.



\### Command discovery



Make `qr` discoverable through normal plugin command listings/autocomplete without removing or changing existing commands.



\### Register plugin



Wire it into `PluginManager::reload\_from\_dirs` alongside other built-in utility plugins.



Add the module export under `src/plugins/mod.rs`.



\### Tests



Add plugin-level tests covering:



\- exact `qr`;

\- `qr hello`;

\- spaces;

\- Unicode;

\- query-prefix routing;

\- correct plugin name;

\- command discoverability;

\- plugin registration in the default manager;

\- unrelated commands remain present.



\### Natural commit boundary



\*\*M2-A\*\*



Suggested commit:



```text

feat(qr): \[M2-A] register QR launcher plugin and query commands

```



\---



\## M2-B — Add typed command routing



\### Architectural requirement



Use the existing typed command bus.



Because QR invocation carries optional initial source text, do not route it as an opaque external command.



Use the smallest typed-command extension consistent with the current architecture.



A QR-aware dialog command is acceptable if that remains the natural owner in the checked-out source.



The important requirement is:



```text

Action

&#x20; -> typed parser

&#x20; -> typed Command

&#x20; -> CommandBus

&#x20; -> LauncherApp host method

&#x20; -> QR dialog state

```



Not:



```text

Action

&#x20; -> string matching scattered through GUI code

```



\### Required work



Extend the relevant command model/parser.



The typed representation must carry:



```text

initial\_text: Option<String>

```



or equivalent.



Extend the appropriate host trait with a method such as conceptually:



```rust

open\_qr\_dialog(initial\_text: Option<\&str>)

```



or an ownership-equivalent signature.



Route through the normal command handler.



The interactive open command should:



\- avoid generic headless execution;

\- use existing interactive-dialog focus policy;

\- skip command-history recording unless current architecture requires otherwise for an explicit reason.



This matters because QR payloads may contain private text.



Do not intentionally create a QR payload history.



\### Parser tests



Cover:



\- no payload;

\- text payload;

\- Unicode payload;

\- typed command domain/kind identification;

\- malformed/future QR wire commands do not accidentally launch external processes.



\### Handler tests



Use the existing fake-host style.



Verify:



\- QR open routes exactly once;

\- optional initial text is preserved exactly;

\- opening is claimed by the typed handler;

\- the generic headless executor does not run;

\- history behavior is appropriate for an interactive utility;

\- normal interactive focus policy is preserved.



\### Natural commit boundary



\*\*M2-B\*\*



Suggested commit:



```text

feat(commands): \[M2-B] route QR generator through typed command bus

```



\---



\# 9. Milestone 3 — Build the QR Generator Dialog



\## Objective



Create the actual focused QR utility UI and integrate it with `LauncherApp`/panel lifecycle.



\---



\## M3-A — Add transient dialog state and lifecycle



\### State



Add a dedicated QR dialog state.



Conceptually it will need:



```text

open

source text

selected error correction

advanced section open/closed

current generated QR result

current generation error

preview texture/cache

input focus request

optional transient status

```



Exact names should fit current code conventions.



\### Opening



Support:



```text

open(None)

```



Behavior:



\- open dialog;

\- reset source to empty;

\- default error correction to Medium;

\- clear old preview/error/status state;

\- request focus in source editor.



Support:



```text

open(Some(text))

```



Behavior:



\- open dialog;

\- replace source with exact supplied text;

\- default error correction to Medium;

\- generate QR;

\- request focus appropriately.



Do not retain payload from a previous closed session when starting a fresh `qr` invocation.



\### Closing



Closing:



\- does not save payload history;

\- does not automatically copy anything;

\- does not create a file;

\- leaves unrelated app state unchanged.



\### `LauncherApp`



Add the dialog state to `LauncherApp`.



Initialize it in the normal constructor/default setup.



Integrate with whatever panel lifecycle is currently responsible for:



\- is-open checks;

\- panel stack;

\- focus ordering;

\- forced close;

\- Escape handling.



If this requires adding a `Panel::QrDialog` variant, do so consistently in every relevant panel-state mapping.



Do not add QR-specific Escape hacks outside the centralized panel behavior.



\### Natural commit boundary



\*\*M3-A\*\*



Suggested commit:



```text

feat(qr): \[M3-A] integrate QR dialog lifecycle with launcher panels

```



\---



\## M3-B — Implement editable source and live preview



\### Layout



Build a focused utility layout approximately around:



```text

QR Generator



Text

┌──────────────────────────────────────────┐

│ editable multiline source               │

│                                          │

└──────────────────────────────────────────┘



\[Paste] \[Copy Text]



Characters: ...

UTF-8 bytes: ...



&#x20;                 ┌──────────────────┐

&#x20;                 │                  │

&#x20;                 │     QR CODE      │

&#x20;                 │                  │

&#x20;                 └──────────────────┘



\[Copy QR] \[Save PNG]



Advanced ▸

```



Do not treat this sketch as a strict pixel specification.



Use existing application UI conventions.



\### Source editor



Use a multiline editor.



It must:



\- accept normal typing;

\- accept paste;

\- accept Unicode;

\- preserve whitespace/newlines;

\- remain editable with preview visible.



\### Live generation



When source or error-correction selection changes:



\- regenerate the QR;

\- update metadata;

\- refresh preview texture.



Do not regenerate the same QR every egui frame when nothing changed.



Track dirty/revision state or otherwise cache the result.



\### Texture



Create/update the egui preview texture only when generated image contents change.



Use nearest-neighbor texture sampling or an equivalent crisp presentation path.



The displayed preview should:



\- maintain square aspect ratio;

\- scale within available space;

\- not become visually smeared.



\### Empty state



With no text:



\- do not display an error;

\- display an instructional placeholder such as:

&#x20; - "Enter or paste text to generate a QR code."



Disable output actions requiring a generated QR.



\### Capacity failure



When the payload does not fit:



\- clear the current valid preview so stale QR content cannot be mistaken for the new input;

\- show a clear inline error;

\- retain the user's full text;

\- continue allowing editing;

\- disable Copy QR / Save PNG until generation succeeds.



Never truncate.



\### Natural commit boundary



\*\*M3-B\*\*



Suggested commit:



```text

feat(qr): \[M3-B] add live QR source editor and preview

```



\---



\## M3-C — Add advanced error-correction controls



Add a collapsible \*\*Advanced\*\* area.



Allow:



\- Low / L;

\- Medium / M;

\- Quartile / Q;

\- High / H.



Default:



```text

Medium / M

```



Include brief explanatory copy.



For example:



```text

Higher error correction improves recovery from obstruction or image damage,

but reduces the amount of data a QR code can hold.

```



Changing the level must immediately regenerate the current payload.



If changing from M to H causes previously valid data to exceed capacity:



\- show the capacity error;

\- do not mutate the source;

\- do not silently revert the user's choice.



No QR settings persistence is needed.



\### Tests



Where practical, test state transitions separately from egui rendering:



\- fresh dialog defaults to Medium;

\- open-with-text sets exact source;

\- fresh plain open clears previous source;

\- changing source invalidates/rebuilds generated state;

\- changing error correction invalidates/rebuilds generated state;

\- capacity error removes stale output;

\- empty input is not treated as an encoding failure.



\### Natural commit boundary



\*\*M3-C\*\*



Suggested commit:



```text

feat(qr): \[M3-C] add QR error correction controls and capacity feedback

```



\---



\# 10. Milestone 4 — Add Explicit Clipboard and PNG Actions



\## Objective



Complete the useful output/input actions without introducing automatic side effects.



\---



\## M4-A — Paste and Copy Text



\### Paste



Add explicit:



```text

Paste

```



Behavior:



1\. Attempt to read text from clipboard.

2\. On success:

&#x20;  - replace source with clipboard text;

&#x20;  - trigger normal live generation.

3\. On failure:

&#x20;  - retain existing source;

&#x20;  - report the failure clearly.



Do not support clipboard image decoding in this goal.



\### Copy Text



Add:



```text

Copy Text

```



Behavior:



\- copy source exactly as entered;

\- no normalization;

\- do nothing destructive to QR state.



Disable or sensibly handle the action when source is empty.



Use existing error/toast conventions rather than bespoke message popups.



\### Natural commit boundary



\*\*M4-A\*\*



Suggested commit:



```text

feat(qr): \[M4-A] add explicit QR text clipboard actions

```



\---



\## M4-B — Copy QR image



Add:



```text

Copy QR

```



Reuse existing `arboard` image clipboard patterns already present in the application.



Requirements:



\- use the same standards-compliant raster represented by current QR state;

\- do not rebuild a differently styled QR for clipboard output;

\- copy full white quiet zone;

\- use RGBA bytes in the format expected by `arboard`;

\- only enable when generation is valid.



On success:



\- use the normal lightweight success/toast convention.



On failure:



\- keep the dialog open;

\- retain source/generated state;

\- report error normally.



Do not automatically close after copying.



\### Tests



Keep platform clipboard calls behind the narrowest practical function boundary so transformation/raster logic remains testable without relying on the live Windows clipboard.



Do not build an oversized new clipboard abstraction for this feature.



\### Natural commit boundary



This can be committed with PNG saving if the diff remains coherent. If it becomes independently substantial, commit it separately.



Preferred identifier:



\*\*M4-B\*\*



Suggested commit:



```text

feat(qr): \[M4-B] add QR image clipboard export

```



\---



\## M4-C — PNG saving



Add:



```text

Save PNG

```



Use the existing `rfd::FileDialog` style.



Recommended default filename:



```text

multi\_launcher\_qr.png

```



Requirements:



\- PNG filter;

\- appropriate extension handling;

\- do not use payload contents as filename;

\- write crisp QR raster;

\- preserve quiet zone;

\- no automatic save;

\- only enabled for a valid generated QR.



On cancellation:



\- do nothing;

\- do not show an error.



On failure:



\- retain dialog state;

\- report the failure.



On success:



\- optional normal success toast with resulting path is appropriate.



\### Tests



Unit-test PNG encoding where practical using a temporary file or memory buffer.



Verify:



\- resulting file is valid PNG;

\- decoded dimensions match expected export dimensions;

\- no source payload is required in filename.



\### Natural commit boundary



\*\*M4-C\*\*



Suggested commit:



```text

feat(qr): \[M4-C] add local PNG export for generated QR codes

```



\---



\# 11. Milestone 5 — Integration Hardening and Targeted Verification



\## Objective



Validate the complete feature against the approved behavior without turning this into an unrelated application-wide regression campaign.



This milestone does not require a commit simply for "running tests."



Only make a checkpoint commit if verification produces a coherent code/test/documentation change.



\---



\## M5-A — Complete QR-focused integration coverage



Create/extend tests sufficient to prove the complete path:



```text

query

&#x20;-> QR plugin

&#x20;-> Action

&#x20;-> typed parser

&#x20;-> CommandBus

&#x20;-> QR host

&#x20;-> dialog open state

```



Cover:



\### Plugin behavior



\- built-in QR plugin registered;

\- `qr` discoverable;

\- exact query opens empty generator;

\- text query preserves source;

\- Unicode preserved;

\- existing plugin commands remain present.



\### Command behavior



\- QR action parses to typed command;

\- payload transported exactly;

\- dialog command bypasses generic external/headless execution;

\- interactive-history policy does not create QR-specific payload history.



\### Dialog/domain behavior



\- empty initial state;

\- prefilled initial state;

\- fresh invocation resets stale text;

\- Medium default;

\- L/M/Q/H generation;

\- live update state;

\- multiline;

\- Unicode;

\- capacity failure;

\- stale preview removed on invalid source;

\- valid state restored after reducing/editing payload.



\### Export behavior



\- PNG raster valid;

\- quiet zone retained;

\- image output deterministic for same payload/config;

\- explicit clipboard functions operate only when invoked.



\### Privacy invariants



Review code to ensure:



\- payload is not emitted through `tracing!`;

\- payload is not written to QR-specific persistent storage;

\- payload is not automatically saved;

\- QR plugin performs no network calls.



\### Natural commit boundary



If substantive tests were added here:



```text

test(qr): \[M5-A] cover QR command and dialog integration

```



Do not create a meaningless commit if these tests were already added alongside their respective milestones.



\---



\## M5-B — Targeted verification



Run the narrowest useful checks first.



Examples should include the actual test targets added by the implementation, such as:



```text

cargo test qr

```



and relevant existing integration suites such as:



```text

cargo test --test plugin\_commands

cargo test --test plugin\_routing

```



Use `cargo nextest` equivalents where appropriate for this repository's normal workflow.



Also perform a compile-level check after command/gui integration:



```text

cargo check

```



Do not run the most expensive full repository test matrix after every checkpoint.



At final verification, run the QR-specific tests plus directly affected command/plugin integration tests.



Run broader verification only if:



\- changes touched a shared abstraction in a way that makes broader fallout plausible;

\- targeted testing discovers cross-feature regressions;

\- compiler failures indicate an unexpectedly wider dependency.



Document the concrete reason before broadening verification.



\---



\# 12. Manual Acceptance Checks



Perform a short native smoke check after automated verification if a Windows GUI session is available.



This is not an automated UI-test campaign.



\## Invocation



Verify:



```text

qr

```



opens empty QR Generator.



Verify:



```text

qr https://example.com

```



opens populated generator.



\---



\## Editing



Verify:



\- typing updates QR;

\- multiline text works;

\- Unicode works;

\- emptying the source removes preview without alarming error.



\---



\## Smartphone scan



Using an ordinary phone camera, verify at least representative:



\- URL;

\- short plain text;

\- multiline text if phone decoder displays it reasonably.



The generated URL QR should open/offer the encoded URL.



Do not block automated implementation solely because Codex itself cannot physically perform this phone-camera step. Mark it as a human smoke check if necessary.



\---



\## Clipboard



Verify:



\- Paste reads text only when clicked;

\- opening QR does not change clipboard;

\- Copy Text copies exact text;

\- Copy QR produces an image that can be pasted into a normal image-capable app.



\---



\## PNG



Verify:



\- Save PNG opens native save dialog;

\- cancellation is silent;

\- resulting PNG opens normally;

\- saved QR can be scanned;

\- no automatic file is created merely by opening the generator.



\---



\## Error correction / capacity



Verify:



\- default is Medium;

\- L/M/Q/H are selectable;

\- switching error correction updates QR;

\- excessive data produces understandable failure;

\- source data remains intact;

\- returning to a valid payload regenerates preview.



\---



\# 13. Compatibility Invariants



Throughout the task preserve:



\- existing plugin command behavior;

\- existing plugin enable/disable behavior;

\- existing launcher search behavior;

\- existing typed command dispatch behavior;

\- existing panel stack/Escape behavior;

\- existing screenshot/image clipboard behavior;

\- existing PNG save behavior in unrelated tools;

\- existing configuration formats;

\- existing radial behavior;

\- existing MkMacro behavior;

\- existing clipboard history behavior.



Do not refactor common systems simply because QR uses them.



Reuse them at their existing seams.



\---



\# 14. Expected Files / Areas



Exact files remain subject to source inspection, but likely touched areas include:



```text

Cargo.toml

Cargo.lock



src/lib.rs

src/qr.rs

or

src/qr/mod.rs



src/plugins/mod.rs

src/plugins/qr.rs

src/plugin.rs



src/commands/model.rs

src/commands/parser.rs

src/commands/host.rs

src/commands/bus.rs

src/commands/handlers/...



src/gui/mod.rs

src/gui/command\_host.rs

src/gui/render.rs

src/gui/qr\_dialog.rs



tests/plugin\_commands.rs

tests/plugin\_routing.rs

possibly a focused QR integration test target

```



Do not touch every listed file mechanically.



Only modify what the current architecture actually requires.



\---



\# 15. Recommended Commit Map



The intended checkpoint history is:



```text

M1-A  QR encoding domain

M1-B  QR raster rendering and metadata



M2-A  QR plugin and launcher queries

M2-B  Typed QR command routing



M3-A  QR dialog and launcher panel lifecycle

M3-B  Editable input and live preview

M3-C  Error-correction controls and capacity UX



M4-A  Explicit text clipboard actions

M4-B  QR image clipboard copy

M4-C  PNG export



M5-A  Any remaining coherent integration-test coverage

```



Suggested commits:



```text

feat(qr): \[M1-A] add local QR encoding domain

feat(qr): \[M1-B] add crisp QR raster rendering and metadata



feat(qr): \[M2-A] register QR launcher plugin and query commands

feat(commands): \[M2-B] route QR generator through typed command bus



feat(qr): \[M3-A] integrate QR dialog lifecycle with launcher panels

feat(qr): \[M3-B] add live QR source editor and preview

feat(qr): \[M3-C] add QR error correction controls and capacity feedback



feat(qr): \[M4-A] add explicit QR text clipboard actions

feat(qr): \[M4-B] add QR image clipboard export

feat(qr): \[M4-C] add local PNG export for generated QR codes



test(qr): \[M5-A] cover QR command and dialog integration

```



These are suggested boundaries, not a requirement to manufacture exactly ten commits.



If two neighboring checkpoints are genuinely one small coherent change, use judgment.



Do not let unrelated substantial work accumulate into a giant commit merely to reduce commit count.



\---



\# 16. Definition of Done



Goal D is complete when all of the following are true:



\- \[ ] `qr` is a discoverable built-in command.

\- \[ ] `qr` opens a fresh empty QR Generator.

\- \[ ] `qr <text>` opens pre-populated.

\- \[ ] All generation is local/offline.

\- \[ ] Arbitrary text works.

\- \[ ] URLs work.

\- \[ ] Multiline input works.

\- \[ ] UTF-8/Unicode works.

\- \[ ] Live preview updates when input changes.

\- \[ ] Preview remains crisp.

\- \[ ] Required quiet zone is present.

\- \[ ] Character count is shown.

\- \[ ] UTF-8 byte count is shown.

\- \[ ] Payloads are never silently truncated.

\- \[ ] Oversized payloads show useful feedback.

\- \[ ] Default error correction is Medium.

\- \[ ] L/M/Q/H are available under Advanced.

\- \[ ] Paste is explicit.

\- \[ ] Copy Text is explicit.

\- \[ ] Copy QR is explicit.

\- \[ ] Save PNG is explicit.

\- \[ ] PNG export remains entirely local.

\- \[ ] No SVG functionality was added.

\- \[ ] No QR decoding was added.

\- \[ ] No QR history was added.

\- \[ ] No cloud/network generation was added.

\- \[ ] No QR styling system was added.

\- \[ ] No broad Universal Action work was added.

\- \[ ] QR payloads are not intentionally logged.

\- \[ ] Existing launcher/plugin/dialog behavior remains intact.

\- \[ ] QR-focused automated tests pass.

\- \[ ] Directly affected plugin/command tests pass.

\- \[ ] Diff has been reviewed for scope creep.

\- \[ ] Active checkpoint commits clearly show implementation progress.



\---



\# 17. Stop Conditions / Scope Control



Stop and investigate before proceeding if implementation appears to require:



\- replacing the typed command bus;

\- rewriting panel management;

\- changing clipboard architecture application-wide;

\- introducing a networking dependency for runtime generation;

\- persistent payload storage;

\- a major plugin-system refactor;

\- changing unrelated utility behavior.



Those are signs that the implementation has moved away from the intended feature boundary.



Resolve the architectural cause or report the conflict rather than broadening the feature silently.

