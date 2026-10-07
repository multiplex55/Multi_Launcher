\# Multi Launcher — General-Purpose Screen Region OCR



\## Goal



Add a first-class, general-purpose Screen Region OCR utility to Multi Launcher.



The intended workflow is:



&#x20;   invoke `ocr`

&#x20;       ↓

&#x20;   temporarily hide/park the launcher

&#x20;       ↓

&#x20;   select a rectangular screen region

&#x20;       ↓

&#x20;   release mouse to confirm

&#x20;       ↓

&#x20;   capture selected pixels locally

&#x20;       ↓

&#x20;   recognize English text using the local Windows OCR backend

&#x20;       ↓

&#x20;   return to Multi Launcher

&#x20;       ↓

&#x20;   show editable/selectable multiline OCR result

&#x20;       ↓

&#x20;   Copy All / Re-capture / Close



This feature must remain entirely local/offline.



Do not use:



\- cloud OCR;

\- websites;

\- remote APIs;

\- online fallback services.



The existing repository is the source of truth.



Before implementation, inspect the current checked-out code rather than assuming

that file names, abstractions, or behavior exactly match this plan.



\---



\# Source-of-Truth Context



The source snapshot used when producing this plan was:



&#x20;   multi\_launcher(20261005-221056).zip



Relevant current architecture observed in that snapshot includes:



\- `src/mkmacro/ocr.rs`

&#x20; - `OcrBackend`

&#x20; - `WindowsOcrBackend`

&#x20; - `OcrDocument`

&#x20; - `OcrLine`

&#x20; - `OcrWord`

&#x20; - `OcrRegionDocument`

&#x20; - `recognize\_region`

&#x20; - OCR tiling and tiled-document reconstruction

&#x20; - local Windows OCR language support

\- `src/mkmacro/screen.rs`

&#x20; - `ScreenCaptureBackend`

&#x20; - `WindowsScreenCaptureBackend`

&#x20; - `SearchRegion`

&#x20; - `ScreenRect`

&#x20; - signed virtual-desktop handling

&#x20; - multi-monitor compositing

\- `src/gui/mkmacro\_dialog/visual\_overlay.rs`

&#x20; - native rectangle-selection infrastructure

&#x20; - `RectanglePurpose`

&#x20; - operation IDs

&#x20; - rectangle-confirm/cancel/error events

\- `src/gui/mkmacro\_dialog/visual\_capture\_workflow.rs`

&#x20; - reusable overlay-controller behavior

&#x20; - rectangle-pick dispatch/polling

&#x20; - stale-operation protection

\- `src/plugin.rs`

&#x20; - built-in plugin registration

\- `src/gui/mod.rs`

&#x20; - launcher panel/dialog ownership

&#x20; - panel lifecycle and launcher state



These are architectural clues, not instructions to mechanically put the new

feature inside MkMacro.



General OCR is a launcher utility in its own right.



\---



\# Approved Product Requirements



\## Invocation



Provide a first-class launcher command:



&#x20;   ocr



Executing it begins screen-region selection.



The OCR action should also be usable through existing launcher action-assignment

surfaces where ordinary launcher commands/actions are already supported, such as

radial cells.



Do not add an independent OCR-specific global hotkey subsystem.



\---



\## Selection UX



When OCR starts:



1\. hide or park the normal launcher;

2\. show the existing/reused rectangle-selection overlay;

3\. allow click-drag rectangular selection;

4\. mouse release confirms the rectangle;

5\. `Esc` cancels;

6\. support the Windows virtual desktop, including multi-monitor layouts and

&#x20;  negative coordinates;

7\. avoid requiring a second confirmation click.



The workflow should feel closer to a screenshot-region picker than to opening a

new editor.



\---



\## Capture



The pixels recognized must predictably correspond to the selected region.



The implementation should reuse the existing local screen-capture infrastructure

where practical.



OCR captures are transient.



Do not:



\- save the selected image to disk;

\- add it to Screenshot History;

\- create an OCR image gallery;

\- retain the source image after the OCR workflow no longer requires it.



\---



\## OCR Engine



Use local Windows OCR.



Reuse/generalize the existing OCR backend already used by MkMacro.



Do not introduce a second OCR implementation merely because general-purpose OCR

is a new feature.



In particular, do not add Tesseract or another heavyweight bundled OCR engine

unless the existing Windows OCR implementation is proven incapable of satisfying

the approved requirements.



There must never be an online fallback.



If OCR is unavailable, report that clearly.



\---



\# English-Only Policy



Goal E is English-only.



Do not build an OCR-language selector.



Do not build language-management settings.



Use the local Windows OCR language facilities to resolve a suitable installed

English recognizer.



The exact language-resolution implementation should fit the current Windows OCR

backend, but the behavioral rule is:



1\. choose an installed/supported English recognizer deterministically;

2\. where multiple English variants exist, prefer a sensible user-profile or

&#x20;  common English variant rather than arbitrary ordering;

3\. never resolve to a non-English recognizer for this general OCR command;

4\. if no supported English OCR language is available, show an actionable local

&#x20;  error;

5\. do not change MkMacro's existing configurable/automatic OCR-language behavior.



Do not convert MkMacro itself to English-only.



The English-only rule applies to the new general-purpose OCR feature.



\---



\# OCR Result UX



After successful recognition, return to Multi Launcher and show a compact OCR

result surface.



The surface must:



\- display multiline text;

\- preserve meaningful OCR-derived line breaks;

\- support long text through scrolling;

\- allow the user to select text;

\- permit lightweight manual correction;

\- make `Copy All` obvious.



This is not intended to become a full note editor or text-processing IDE.



The recognized text should begin as the text returned from the OCR document with

only safe/basic normalization.



Do not automatically:



\- rewrite grammar;

\- modify punctuation heuristically;

\- merge everything into prose paragraphs;

\- summarize content;

\- run AI cleanup.



\---



\# Clipboard Behavior



Successful OCR must NOT automatically replace the clipboard.



The user explicitly chooses:



&#x20;   Copy All



or manually selects/copies text.



A no-text result must never copy an empty string over the clipboard.



\---



\# Initial OCR Result Actions



The initial result surface should intentionally remain small.



Required actions:



\- Copy All

\- Re-capture / OCR Another Region

\- Close / Done



Do not add the following to Goal E:



\- Save as Note

\- Create TODO

\- Search Web

\- Clipboard Modify

\- Regex Tester integration

\- JSON Formatter integration

\- translation

\- large Universal Action menus for OCR text



Those can be future additions once the core OCR workflow is proven reliable.



\---



\# No-Text Behavior



If OCR completes successfully but returns no meaningful text:



\- clearly say that no text was recognized;

\- expose Re-capture;

\- expose Close;

\- leave the clipboard unchanged;

\- do not treat an empty result as successful copyable content.



\---



\# Responsiveness



Rectangle selection must remain responsive.



OCR execution must not freeze the primary egui UI thread.



If recognition lasts long enough for latency to be visible, show a lightweight:



&#x20;   Recognizing text...



state.



Do not implement:



\- continuous OCR;

\- background OCR polling;

\- automatic screen watching;

\- permanent capture threads unrelated to an active OCR operation.



\---



\# Cancellation and Restoration



Cancellation is a first-class workflow outcome.



Pressing `Esc` during region selection must:



\- cancel only the active OCR region operation;

\- remove native overlay resources;

\- avoid starting OCR;

\- restore the launcher to the appropriate previous state;

\- avoid generating an error toast for normal user cancellation;

\- leave the clipboard unchanged.



Successful OCR should intentionally transition to the OCR result surface rather

than restoring an unrelated previous search as though nothing happened.



Avoid stale completion events reopening OCR UI after the user has cancelled,

closed, or started another OCR operation.



\---



\# MkMacro Compatibility



General OCR is NOT an MkMacro feature exposed through a different button.



It is a normal Multi Launcher utility that happens to share infrastructure with

MkMacro.



Required invariants:



\- existing MkMacro OCR actions continue working;

\- existing MkMacro conditions continue working;

\- existing MkMacro OCR language behavior continues working;

\- existing MkMacro serialized macro formats remain compatible;

\- existing MkMacro capture/search regions remain compatible;

\- existing MkMacro visual OCR/testing workflows remain functional;

\- MkMacro must not be forced through the new general OCR result UI.



Share infrastructure.



Do not share user workflow.



\---



\# Persistence and Privacy



Do not persist general OCR results by default.



Do not create:



\- OCR text history;

\- OCR image history;

\- OCR database entries;

\- automatically saved screenshot files.



Once the transient result/workflow is discarded, the OCR utility itself should

not retain the recognized text or source image.



Normal OS clipboard behavior after an explicit Copy action is obviously outside

that rule.



\---



\# Settings Scope



Keep settings minimal.



Because this version is English-only, a normal OCR-language setting is not

required.



Do not add settings for:



\- confidence thresholds;

\- image preprocessing;

\- tile sizes;

\- OCR debug modes;

\- engine internals.



The existing Plugins enable/disable mechanism may be used as appropriate.



Do not create a large OCR Settings page.



\---



\# Explicit Non-Goals



The following are OUT OF SCOPE:



\- PDF OCR

\- full-document OCR

\- camera/webcam OCR

\- handwriting-specific functionality

\- translation

\- table reconstruction

\- rich document-layout reconstruction

\- QR recognition

\- barcode recognition

\- OCR screenshot history

\- OCR text history

\- screenshot annotation

\- screenshot editing

\- general image editing

\- AI cleanup

\- AI summarization

\- live OCR

\- continuous OCR

\- cloud OCR

\- website integrations

\- OCR-specific global hotkeys

\- broad MkMacro redesign

\- broad screenshot-system redesign



If one of these becomes technically tempting while implementing the feature,

leave it out and report it as potential future work.



\---



\# Architectural Principles



\## 1. Establish correct ownership without an unnecessary mega-refactor



The existing OCR implementation currently lives largely under `mkmacro`.



General-purpose OCR should not be architecturally owned by the MkMacro UI.



However, do not migrate large unrelated parts of the repository purely to obtain

perfect module names.



Establish the smallest clean shared boundary necessary.



Acceptable strategies include:



\- extracting genuinely generic OCR types/backend/service code into a shared OCR

&#x20; module;

\- introducing a shared OCR service/facade outside MkMacro that composes the

&#x20; proven existing backend;

\- retaining compatibility re-exports/adapters for MkMacro where migration is

&#x20; necessary.



The implementer must inspect actual callers before choosing the exact migration.



Do NOT create a duplicate:



\- Windows OCR backend;

\- screen capture implementation;

\- tiling implementation;

\- OCR text reconstruction implementation.



\---



\## 2. Separate workflow state from OCR mechanics



The general OCR workflow should have explicit state.



Conceptually it will need states equivalent to:



&#x20;   Idle

&#x20;   SelectingRegion

&#x20;   Recognizing

&#x20;   Result

&#x20;   NoText

&#x20;   Error



The exact Rust representation may differ.



Do not encode the workflow as a scattered collection of unrelated booleans.



Operations that can complete asynchronously must carry enough identity/generation

information to reject stale completion.



\---



\## 3. UI should not own recognition logic



The egui result surface should display and edit result state.



It should not implement:



\- Windows OCR;

\- image capture;

\- tile planning;

\- language discovery;

\- screen-coordinate calculations.



Keep platform/domain behavior behind appropriate services.



\---



\## 4. Use existing typed command/action architecture



The new launcher command should fit the current typed command/plugin execution

path.



Do not create a special string parser or side channel only for OCR.



If the current plugin architecture naturally produces a launcher action which

dispatches a typed command, follow that architecture.



\---



\# Active Checkpoint Commit Cadence



Use the project’s active checkpoint commit cadence and define the task-specific commit boundaries in the plan.



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



\# Milestone 0 — Reconfirm Current Architecture



\## Objective



Before editing, confirm that the current checkout still matches the architectural

assumptions in this plan.



This is an investigation gate, not a broad research project.



\## Inspect



At minimum inspect:



\- current OCR backend and OCR document model;

\- all direct callers of `recognize\_region`;

\- Windows OCR implementation;

\- Windows screen-capture backend;

\- region/rectangle selector;

\- overlay operation ownership;

\- launcher hide/restore or parking mechanisms used by similar features;

\- plugin registration;

\- typed command dispatch;

\- panel/dialog ownership;

\- relevant tests.



Likely files include:



&#x20;   src/mkmacro/ocr.rs

&#x20;   src/mkmacro/screen.rs

&#x20;   src/mkmacro/executor.rs

&#x20;   src/gui/mkmacro\_dialog/visual\_overlay.rs

&#x20;   src/gui/mkmacro\_dialog/visual\_capture\_workflow.rs

&#x20;   src/plugin.rs

&#x20;   src/gui/mod.rs

&#x20;   src/commands/\*

&#x20;   src/plugins/\*



Also inspect Screen Draw / Screenshot launcher-parking behavior if it provides a

better precedent for temporarily hiding and restoring the launcher.



\## Questions to resolve from code



Determine:



1\. whether generic OCR code should be extracted from `mkmacro::ocr` or wrapped

&#x20;  by a shared service;

2\. whether rectangle selection can be consumed cleanly through the existing

&#x20;  visual-overlay controller or needs a narrow general-purpose adapter;

3\. which existing launcher parking/restore mechanism is safest to reuse;

4\. how a first-class `ocr` action should enter the typed command system;

5\. where transient OCR workflow state should live;

6\. what existing worker/background-task pattern should be reused for recognition.



\## Boundaries



Do not:



\- refactor unrelated MkMacro authoring code;

\- redesign the screenshot system;

\- redesign the command bus;

\- migrate unrelated visual overlays.



\## Verification



Read-only investigation is normally sufficient.



Run a targeted existing test only if needed to settle a concrete ambiguity.



\## Commit



No commit is required for Milestone 0 unless the repository explicitly stores

plan/investigation documentation.



Do not create a meaningless investigation-only source commit.



\## Done when



The implementer can clearly state:



\- shared OCR ownership;

\- capture ownership;

\- workflow-state owner;

\- async execution approach;

\- launcher restoration approach;

\- command/plugin entry path.



\---



\# Milestone 1 — Establish Shared General OCR Boundary



\## M1-A — Shared OCR service/facade



\### Objective



Make the proven OCR implementation consumable by a general launcher feature

without duplicating the MkMacro OCR stack.



\### Architectural intent



Create a reusable OCR boundary outside the general OCR UI.



The boundary should support, at minimum:



\- recognizing a selected rectangular screen region;

\- obtaining reconstructed plain text;

\- local backend errors;

\- cancellation;

\- English language resolution for the new workflow.



Do not let the new UI call raw WinRT APIs.



\### Required behavior



Reuse:



\- existing `OcrBackend`;

\- existing `WindowsOcrBackend`;

\- existing OCR tiling;

\- existing `OcrDocument`;

\- existing text reconstruction;

\- existing screen capture backend.



If moving generic code to a shared module is clean and proportionate, do so.



If moving all screen primitives would create a large unrelated migration, prefer

a narrow shared adapter/facade and leave deeper ownership refactoring for later.



The key acceptance condition is:



> There is one OCR engine/pipeline implementation, not an MkMacro implementation

> plus a second launcher implementation.



\### Compatibility



MkMacro's current callers must continue to compile and retain their existing

semantics.



\### Tests



Use fake capture/OCR backends where practical to prove:



\- capture is requested for the supplied rectangle;

\- reconstructed text is returned;

\- cancellation is propagated;

\- backend failures remain distinguishable;

\- no persistent file output is introduced.



\### Suggested commit



&#x20;   refactor(ocr): \[M1-A] establish shared local OCR service boundary



\---



\## M1-B — English-only general OCR policy



\### Objective



Implement a deterministic English recognizer policy for general OCR without

changing MkMacro language behavior.



\### Required behavior



General OCR:



\- chooses only an installed/supported English OCR language;

\- never silently recognizes using a non-English engine;

\- exposes a useful error when English OCR is unavailable;

\- has no language selector.



Prefer a sensible English variant using the local/profile-supported language

information available from the Windows backend.



Do not hard-code assumptions that make machines with another installed English

variant unusable if a supported English recognizer is available.



\### Preserve



MkMacro must retain:



\- `Auto`, where currently supported;

\- explicit language tags;

\- existing macro serialization.



\### Tests



Add deterministic tests around language selection independent of the actual

developer machine's installed language packs.



Cover:



\- one English language;

\- several English variants;

\- English plus non-English;

\- no English language;

\- empty language list;

\- backend language-query failure.



\### Suggested commit



&#x20;   feat(ocr): \[M1-B] add English-only general OCR language policy



\---



\# Milestone 2 — Generalize Region Selection for OCR



\## M2-A — Add a general OCR rectangle-pick purpose



\### Objective



Allow the existing native region-selection system to represent a

general-purpose OCR capture.



\### Required behavior



Add a distinct semantic purpose equivalent to:



&#x20;   GeneralOcrCapture



to the existing rectangle-pick system if that is the cleanest fit after Milestone

0 inspection.



It must inherit the existing correct behaviors for:



\- initial mouse-release handling;

\- drag start/end;

\- signed virtual-desktop coordinates;

\- Esc cancellation;

\- overlay cleanup;

\- operation IDs;

\- stale-event rejection.



Do not clone the native overlay into an OCR-specific overlay implementation.



\### Tests



Extend existing overlay/unit tests to verify:



\- new purpose round-trips through begin/confirmed events;

\- cancellation is preserved;

\- operation IDs are preserved;

\- the new purpose uses ordinary rectangular behavior;

\- negative virtual-desktop coordinates are retained.



\### Suggested commit



&#x20;   feat(ocr): \[M2-A] add general OCR rectangle capture purpose



\---



\## M2-B — Add OCR selection ownership and launcher parking



\### Objective



Create the launcher-owned lifecycle that begins OCR selection and temporarily

gets Multi Launcher itself out of the captured region.



\### Required behavior



Starting OCR must:



1\. remember the relevant pre-invocation launcher/UI state;

2\. park/hide the main launcher using an existing proven mechanism where

&#x20;  practical;

3\. obtain the current virtual-desktop bounds;

4\. begin the OCR rectangle picker;

5\. retain the operation ID;

6\. process only events belonging to that operation.



Normal cancellation must:



\- terminate the selection transaction;

\- restore the launcher appropriately;

\- not begin OCR;

\- not produce a scary error state.



Overlay failure must:



\- clean up selection ownership;

\- restore the launcher;

\- expose a useful error.



\### Important



Do not allow an old rectangle-confirmation event to start OCR after:



\- cancellation;

\- a second OCR invocation;

\- workflow close;

\- operation replacement.



\### Tests



Test the state transitions around:



\- begin;

\- confirm;

\- cancel;

\- overlay error;

\- stale event;

\- second invocation replacing/rejecting the first as appropriate.



Prefer testing workflow logic without actual GUI automation.



\### Suggested commit



&#x20;   feat(ocr): \[M2-B] integrate OCR region selection with launcher lifecycle



\---



\# Milestone 3 — First-Class Launcher Command



\## M3-A — Add and register the OCR launcher utility



\### Objective



Make OCR discoverable and invokable as a normal Multi Launcher feature.



\### Required behavior



Add a built-in plugin/action entry for:



&#x20;   ocr



The result label/description should make its purpose obvious, for example:



&#x20;   OCR Screen Region

&#x20;   Select a screen region and recognize English text locally



Exact wording may fit existing launcher conventions.



Register it through the normal built-in plugin system.



It should respect ordinary plugin enabled/disabled behavior.



\### Command architecture



Follow the existing typed command/activation architecture.



Do not:



\- parse an arbitrary magic action string directly in GUI code;

\- bypass normal activation;

\- create a new OCR-only message bus.



The resulting action should be assignable wherever normal launcher actions can

already be assigned, including radial cells when supported by the current action

model.



\### Tests



Cover:



\- plugin registration;

\- search result generation for `ocr`;

\- action activation dispatch;

\- disabled plugin behavior if applicable;

\- typed command parsing/dispatch if a new typed command variant is required.



\### Suggested commit



&#x20;   feat(ocr): \[M3-A] register first-class screen OCR launcher command



\---



\## M3-B — Connect command execution to OCR workflow state



\### Objective



Have executing the OCR action start exactly one clean OCR workflow.



\### Required behavior



Invocation should:



\- not open MkMacro;

\- not open Screenshot Editor;

\- not begin OCR until a region is selected;

\- not create persistent OCR state;

\- correctly transition into the selection state from Milestone 2.



Repeated invocation must have defined behavior consistent with other transient

launcher operations.



Avoid duplicate active selectors.



\### Tests



Test:



\- ordinary launcher activation;

\- action launched from a non-primary activation surface where practical;

\- duplicate activation behavior;

\- no accidental MkMacro UI opening;

\- no OCR before rectangle confirmation.



\### Suggested commit



&#x20;   feat(ocr): \[M3-B] route OCR activation into region workflow



\---



\# Milestone 4 — Asynchronous Capture and Recognition



\## M4-A — Perform selected-region capture exactly once



\### Objective



After rectangle confirmation, capture the selected pixels through the shared

capture/OCR service.



\### Required behavior



Use the rectangle returned by the selector.



Preserve:



\- signed desktop origin;

\- full selected width/height;

\- multi-monitor intersections.



The selected screen state should be captured exactly once for recognition.



Do not recapture individual OCR tiles from the desktop.



If tiling is needed, tile the immutable captured image using the existing OCR

pipeline.



This preserves predictable correspondence between selection and recognition.



\### Error cases



Handle:



\- empty rectangle;

\- invalid rectangle;

\- capture outside current desktop;

\- monitor/capture backend failure;

\- cancellation.



\### Tests



Use fake screen backends to cover:



\- negative X/Y origin;

\- cross-monitor-style rectangles;

\- dimensions;

\- one capture per OCR operation;

\- capture errors.



\### Suggested commit



&#x20;   feat(ocr): \[M4-A] capture selected OCR region through shared backend



\---



\## M4-B — Run recognition without blocking the main UI



\### Objective



Move potentially noticeable OCR work off the egui/UI thread.



\### Required behavior



After capture confirmation:



&#x20;   SelectingRegion

&#x20;       ↓

&#x20;   Recognizing

&#x20;       ↓

&#x20;   Result / NoText / Error



The main UI must remain responsive.



Use the repository's existing background-work pattern where appropriate.



Carry an operation/generation ID into asynchronous work so completion from an

obsolete request can be ignored.



\### Recognition progress



While a current operation is running, expose lightweight status:



&#x20;   Recognizing text...



Do not add fake percentage progress if the Windows backend does not provide real

progress.



\### Cancellation/close



If the workflow is abandoned while recognition is outstanding:



\- mark/invalidate the operation;

\- ignore late result publication;

\- allow owned temporary images/results to drop;

\- do not unexpectedly reopen the OCR result UI.



\### Resource lifetime



Once recognition text/document information required by the UI is extracted,

allow the captured image to be released.



Do not store it in `LauncherApp` longer than necessary.



\### Tests



Test workflow-state logic for:



\- successful completion;

\- OCR failure;

\- cancellation/invalidation;

\- stale completion;

\- consecutive OCR attempts;

\- no UI-state resurrection from old worker results.



\### Suggested commit



&#x20;   feat(ocr): \[M4-B] run OCR recognition asynchronously with stale-result guards



\---



\# Milestone 5 — OCR Result Surface



\## M5-A — Add compact OCR result UI



\### Objective



Present successful recognition in a simple, useful Multi Launcher surface.



\### Required UI



Show:



\- a clear OCR result title;

\- editable/selectable multiline recognized text;

\- vertical scrolling for long output;

\- Copy All;

\- Re-capture;

\- Close / Done.



Use the existing egui visual language and dialog/panel conventions.



Do not create a visually unrelated application inside the launcher.



\### Text behavior



Initialize the editor from the existing reconstructed OCR text.



Preserve meaningful line breaks.



Allow lightweight user correction.



The edited text becomes the source used by Copy All.



\### Clipboard



Opening the result surface must not touch the clipboard.



`Copy All` explicitly writes the current edited text to the clipboard.



\### Tests



Extract view-independent state/actions where useful so the following can be

tested without GUI automation:



\- initial recognized text;

\- edits update copy source;

\- Copy All invokes the intended clipboard path;

\- result can close;

\- result can request re-capture.



\### Suggested commit



&#x20;   feat(ocr): \[M5-A] add editable multiline OCR result surface



\---



\## M5-B — No-text and error result states



\### Objective



Make unsuccessful recognition understandable without making ordinary cases feel

like crashes.



\### No-text state



Show something equivalent to:



&#x20;   No text was recognized in the selected region.



Actions:



\- Re-capture

\- Close



Do not offer Copy All for an empty result.



Do not overwrite the clipboard.



\### Error state



Present a useful distinction between cases such as:



\- OCR unavailable;

\- no English OCR recognizer installed;

\- screen capture failed;

\- OCR backend failed.



Do not expose enormous raw diagnostic dumps as the primary UX.



Detailed diagnostic context may still be logged through existing diagnostics.



\### Suggested commit



&#x20;   feat(ocr): \[M5-B] add OCR empty-result and failure states



\---



\## M5-C — Re-capture and close lifecycle



\### Objective



Make repeated OCR usage fast and predictable.



\### Re-capture



From either Result or NoText:



&#x20;   Re-capture

&#x20;       ↓

&#x20;   clear transient prior result

&#x20;       ↓

&#x20;   park launcher

&#x20;       ↓

&#x20;   start fresh rectangle selector



Do not recursively create result windows.



Do not retain old captured images.



\### Close



Close/Done should:



\- discard transient OCR text owned by this workflow;

\- cancel/invalidate outstanding recognition if applicable;

\- release operation ownership;

\- return Multi Launcher to its normal idle behavior.



\### Suggested commit



&#x20;   feat(ocr): \[M5-C] complete OCR recapture and close lifecycle



\---



\# Milestone 6 — Integration Hardening



\## M6-A — Multi-monitor and DPI-sensitive behavior



\### Objective



Verify that the general workflow correctly reuses the repository's signed

virtual-desktop capture semantics.



\### Validate



At minimum:



\- primary monitor;

\- secondary monitor;

\- monitor positioned left of primary;

\- monitor positioned above primary if available/testable;

\- selection entirely on one monitor;

\- selection crossing monitor boundaries;

\- negative coordinates;

\- non-100% Windows display scaling where practical.



Avoid introducing new coordinate conversions in the OCR UI.



The rectangle selector and capture backend should agree on physical screen

coordinates through their existing contract.



\### Automated tests



Prefer deterministic geometry tests where native hardware configuration cannot be

reliably automated.



\### Suggested commit



&#x20;   test(ocr): \[M6-A] cover virtual desktop and capture geometry



\---



\## M6-B — Cancellation, restoration, and operation ownership



\### Objective



Aggressively verify lifecycle edge cases.



\### Cover



\- Esc before drag;

\- Esc during selection;

\- selection followed by close during recognition;

\- two OCR invocations near each other;

\- old worker result arriving after re-capture;

\- overlay error;

\- capture error;

\- OCR error;

\- launcher restore after cancellation;

\- result close;

\- no leaked overlay ownership.



No automated mouse-driving test framework is required.



Test state machines/controller seams instead.



\### Suggested commit



&#x20;   test(ocr): \[M6-B] harden OCR cancellation and restoration behavior



\---



\# Milestone 7 — Preserve MkMacro OCR



\## M7-A — Targeted MkMacro regression verification



\### Objective



Ensure shared OCR changes did not alter MkMacro behavior.



\### Verify existing behavior around



\- OCR Find Text;

\- OCR Click Text;

\- OCR Read Text;

\- OCR conditions / Wait Until OCR;

\- existing language configuration;

\- OCR region capture;

\- tiling;

\- reconstructed text;

\- relevant visual OCR authoring/debug behavior.



Do not broaden this into a complete MkMacro regression campaign.



Use the existing relevant unit/integration tests.



\### Compatibility requirements



No intentional change to:



\- macro JSON schema;

\- serialized OCR action payloads;

\- configured language tags;

\- variable output behavior;

\- OCR match modes;

\- OCR occurrence selection.



\### Suggested commit



Only create a commit here if test additions or compatibility fixes are needed.



Possible:



&#x20;   test(mkmacro): \[M7-A] protect shared OCR compatibility



If no code/test changes are needed, record verification in the implementation

report and do not manufacture an empty commit.



\---



\# Milestone 8 — User-Facing Polish and Documentation



\## M8-A — Help/plugin metadata



\### Objective



Make the new feature understandable without adding a large documentation burden.



Update the relevant launcher/help/plugin metadata so users can discover:



&#x20;   ocr



Describe it as local screen-region OCR.



Make clear through wording where appropriate that it:



\- recognizes English text;

\- works locally;

\- starts a region picker.



Do not market unsupported functionality such as PDF OCR or translation.



\### Suggested commit



&#x20;   docs(ocr): \[M8-A] document local screen region OCR workflow



\---



\# Milestone 9 — Verification and Review Gate



This milestone is a verification gate, not an excuse for broad refactoring.



\## M9-A — Targeted verification



Run the narrowest meaningful set of tests covering changed modules.



At minimum, include relevant tests for:



\- OCR domain/service;

\- English language resolution;

\- capture geometry;

\- rectangle selection;

\- OCR workflow state;

\- command/plugin activation;

\- result behavior;

\- MkMacro OCR compatibility.



Also run:



&#x20;   cargo fmt --check



Run an appropriate targeted compile/check for the affected crate.



Prefer targeted `cargo nextest` filters while iterating.



\---



\## M9-B — Broader verification



After the feature is coherent and targeted verification passes, run the

project's substantive broader verification appropriate for this repository.



Use the repository's existing preferred commands and AGENTS.md guidance.



A full relevant:



&#x20;   cargo nextest run



and/or normal build/check may be appropriate at this final gate.



Do not run the expensive full suite after every earlier checkpoint.



If repository build time is unusually expensive, perform broad verification once

at this gate rather than repeatedly.



\---



\## M9-C — Reviewer pass



Have a reviewer inspect:



\- architecture;

\- duplicate implementation risk;

\- MkMacro preservation;

\- async/stale-result safety;

\- overlay ownership;

\- launcher restoration;

\- clipboard behavior;

\- accidental persistence;

\- scope creep.



Specifically ask the reviewer to look for:



1\. a second OCR implementation accidentally created beside MkMacro's;

2\. a general OCR UI that depends directly on MkMacro editor state;

3\. UI-thread blocking;

4\. old worker results reopening closed UI;

5\. clipboard writes before explicit Copy;

6\. capture images retained unnecessarily;

7\. language policy accidentally changing MkMacro;

8\. non-English recognizer fallback;

9\. region coordinates converted inconsistently;

10\. OCR captures being persisted.



\---



\# Manual Smoke Verification



No elaborate automated GUI-driving harness is required.



Perform a focused manual smoke pass on Windows.



\## Happy path



1\. Open Multi Launcher.

2\. Search `ocr`.

3\. Execute OCR.

4\. Confirm launcher is not present in the captured screen content.

5\. Drag a rectangle around visible English text.

6\. Release.

7\. Observe `Recognizing text...` if processing is noticeable.

8\. Confirm OCR result opens.

9\. Confirm line breaks are reasonable.

10\. Edit a word.

11\. Press Copy All.

12\. Paste elsewhere and confirm the edited text was copied.



\## Clipboard preservation



1\. Put a known value on the clipboard.

2\. Run OCR successfully.

3\. Do NOT press Copy.

4\. Confirm the previous clipboard value remains.

5\. Press Copy All.

6\. Confirm clipboard then changes to OCR text.



\## Cancellation



1\. Invoke OCR.

2\. Press Esc.

3\. Confirm selector disappears.

4\. Confirm launcher restores appropriately.

5\. Confirm no OCR result opens.

6\. Confirm clipboard is unchanged.



\## No text



1\. Select a blank/non-text region.

2\. Confirm a no-text result appears.

3\. Confirm Re-capture is available.

4\. Confirm empty text is not copied.



\## Re-capture



1\. Complete OCR.

2\. Choose Re-capture.

3\. Select another region.

4\. Confirm only the new result is shown.



\## Multi-monitor



If hardware permits:



\- select text on secondary monitor;

\- select text on a monitor with negative desktop coordinates;

\- select a region crossing two monitors.



\## Scaling



If practical, test on a display with non-100% DPI scaling.



\## MkMacro



Run a focused existing OCR macro operation and confirm its normal behavior still

works.



\---



\# Task-Specific Commit Map



The following are intended natural commit boundaries.



They are guidance, not an instruction to make empty commits when a checkpoint

does not produce a meaningful diff.



| Stage | Suggested commit |

|---|---|

| M1-A | `refactor(ocr): \[M1-A] establish shared local OCR service boundary` |

| M1-B | `feat(ocr): \[M1-B] add English-only general OCR language policy` |

| M2-A | `feat(ocr): \[M2-A] add general OCR rectangle capture purpose` |

| M2-B | `feat(ocr): \[M2-B] integrate OCR region selection with launcher lifecycle` |

| M3-A | `feat(ocr): \[M3-A] register first-class screen OCR launcher command` |

| M3-B | `feat(ocr): \[M3-B] route OCR activation into region workflow` |

| M4-A | `feat(ocr): \[M4-A] capture selected OCR region through shared backend` |

| M4-B | `feat(ocr): \[M4-B] run OCR recognition asynchronously with stale-result guards` |

| M5-A | `feat(ocr): \[M5-A] add editable multiline OCR result surface` |

| M5-B | `feat(ocr): \[M5-B] add OCR empty-result and failure states` |

| M5-C | `feat(ocr): \[M5-C] complete OCR recapture and close lifecycle` |

| M6-A | `test(ocr): \[M6-A] cover virtual desktop and capture geometry` |

| M6-B | `test(ocr): \[M6-B] harden OCR cancellation and restoration behavior` |

| M7-A | `test(mkmacro): \[M7-A] protect shared OCR compatibility` if needed |

| M8-A | `docs(ocr): \[M8-A] document local screen region OCR workflow` |



If testing/review identifies a later defect, add a clear remediation commit such

as:



&#x20;   fix(ocr): \[M9-R1] reject stale recognition completion after recapture



Do not silently rewrite an earlier checkpoint merely to make the history look

perfect.



\---



\# Acceptance Criteria



Goal E is complete when all of the following are true.



\## Invocation



\- \[ ] `ocr` is discoverable as a built-in launcher utility.

\- \[ ] Executing it starts region selection.

\- \[ ] It can be assigned through ordinary launcher action surfaces where

&#x20;     supported.

\- \[ ] No OCR-specific global hotkey subsystem was added.



\## Capture



\- \[ ] Launcher is appropriately hidden/parked during selection.

\- \[ ] Mouse drag selects a rectangle.

\- \[ ] Mouse release confirms.

\- \[ ] Esc cancels.

\- \[ ] Multi-monitor signed coordinates work.

\- \[ ] Selected pixels are captured locally.

\- \[ ] Source image is not automatically saved.



\## OCR



\- \[ ] Recognition is fully local/offline.

\- \[ ] Existing Windows OCR backend/pipeline is reused.

\- \[ ] No cloud fallback exists.

\- \[ ] General OCR resolves English only.

\- \[ ] Missing English OCR support produces a clear error.

\- \[ ] Main UI remains responsive during OCR.

\- \[ ] Existing tiled recognition remains usable for large captures.



\## Result



\- \[ ] Multiline recognized text is displayed.

\- \[ ] Meaningful line breaks are preserved.

\- \[ ] Long results scroll.

\- \[ ] Text can be selected.

\- \[ ] Text can be lightly edited.

\- \[ ] Copy All copies the edited result.

\- \[ ] OCR success does not automatically touch clipboard.

\- \[ ] Re-capture starts a new clean capture.

\- \[ ] Close discards transient result state.



\## Empty/Error



\- \[ ] Empty recognition produces a useful no-text state.

\- \[ ] Empty recognition does not clear clipboard.

\- \[ ] Re-capture is available after no-text.

\- \[ ] Backend/capture failures are understandable.

\- \[ ] Normal Esc cancellation is not treated as a scary failure.



\## Lifecycle



\- \[ ] Stale overlay events are ignored.

\- \[ ] Stale worker results are ignored.

\- \[ ] Closing during OCR does not reopen the result later.

\- \[ ] Cancellation cleans up overlay ownership.

\- \[ ] Re-capture does not retain previous images/results.



\## Compatibility



\- \[ ] MkMacro OCR behavior remains intact.

\- \[ ] MkMacro serialization remains compatible.

\- \[ ] MkMacro language behavior is unchanged.

\- \[ ] Screenshot/other rectangle workflows remain intact.



\## Persistence



\- \[ ] No OCR text history added.

\- \[ ] No OCR image history added.

\- \[ ] No automatic screenshot-history entry added.

\- \[ ] No cloud/network dependency added.



\## Verification



\- \[ ] Focused automated tests pass.

\- \[ ] Relevant MkMacro OCR tests pass.

\- \[ ] `cargo fmt --check` passes.

\- \[ ] Appropriate targeted compile/check passes.

\- \[ ] Final broader verification is performed at the designated gate.

\- \[ ] Focused Windows manual smoke verification is completed.

\- \[ ] Reviewer finds no unresolved critical/high-confidence regression.



\---



\# Final Implementation Report



At task completion, provide a concise report containing:



1\. milestone/checkpoint status;

2\. commit hashes and subjects;

3\. important architectural decisions;

4\. shared OCR components created or moved;

5\. tests added/changed;

6\. verification commands and outcomes;

7\. manual smoke results if performed;

8\. any known limitations;

9\. explicitly deferred follow-up ideas.



Do not describe out-of-scope future ideas as though they were implemented.

