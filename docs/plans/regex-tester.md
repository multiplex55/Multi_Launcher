\# Multi Launcher — Goal B: Local Regex Tester



\## Source of Truth



Use the head of the current branch as the baseline. Do not rely on assumptions from older branches, plans, or previous versions when the current source disagrees with them.



The existing application already contains:



\- the `regex` Rust crate;

\- the normal `Plugin` / `PluginManager` architecture;

\- typed command parsing and command hosts;

\- dedicated utility/dialog patterns such as Diff and Convert;

\- shared JSON persistence helpers;

\- existing clipboard helpers;

\- existing egui UI conventions.



Reuse those existing architectural seams instead of creating an unrelated parallel command or window system.



\# Goal



Add a polished, completely local \*\*Regex Tester\*\* to Multi Launcher.



The experience should take strong UX inspiration from regex101:



\- prominent regular-expression input;

\- large editable test-text area;

\- live match highlighting;

\- obvious match count and navigation;

\- capture-group inspection;

\- helpful explanations;

\- searchable regex quick reference;

\- practical built-in examples;

\- substitution/replacement testing;

\- recent regex history;

\- user-saved regex presets.



Do NOT attempt to reproduce regex101 pixel-for-pixel.



The Regex Tester must visually and behaviorally belong to Multi Launcher and must operate entirely locally without websites, cloud APIs, remote regex services, or AI services.





\# Explicit Required Directive



\*\*Use the project’s active checkpoint commit cadence and define the task-specific commit boundaries in the plan.\*\*





\# Approved Product Requirements



\## Regex Flavor



Use one clearly identified \*\*Rust-compatible regex flavor\*\* for the first version.



Prefer the existing Rust `regex` crate already used by Multi Launcher.



The UI must make the supported engine/flavor clear.



Do not pretend to support PCRE2, JavaScript, Python, .NET, Java, or other engines.



Constructs unsupported by Rust regex, such as traditional backreferences or look-around constructs, must fail cleanly with useful inline validation rather than being silently interpreted differently.





\## Main Layout



Use a regex101-inspired desktop layout.



Conceptually:



```text

┌───────────────────────────────────────────────────────────────────────┐

│ Regular Expression                                          flags    │

│ / pattern here / imsux                                                │

├───────────────────────────────────────────┬───────────────────────────┤

│                                           │ Match Information         │

│                                           │                           │

│             TEST TEXT                     │ Capturing Groups          │

│                                           │                           │

│       highlighted matches inline          │ Explanation               │

│                                           │                           │

│                                           │ Quick Reference           │

│                                           │                           │

│                                           │ Examples                  │

├───────────────────────────────────────────┴───────────────────────────┤

│ 7 matches              ◀ Previous      Match 3 of 7       Next ▶     │

└───────────────────────────────────────────────────────────────────────┘

```



The exact visual arrangement may adapt to egui and the existing Multi Launcher style, but preserve the underlying information hierarchy.



Requirements:



\- Regex field prominently at the top.

\- Large multiline test-text editor.

\- Right-side information/reference area.

\- Right-side panel can be collapsed.

\- Resizable utility window.

\- Responsive layout at smaller sizes.

\- Prefer information density and usability over decorative UI.





\## Live Evaluation



Matching updates automatically when:



\- the regex changes;

\- flags change;

\- test text changes.



Use a small debounce so rapid editing does not needlessly reevaluate after every individual event.



Do not use blocking sleeps on the UI thread.



Invalid or temporarily incomplete patterns while the user types must produce an inline validation state.



Do not show disruptive error dialogs or repeated error toasts for normal regex syntax errors while typing.





\## Match Highlighting



Highlight all current matches directly in the test-text editor.



Visually distinguish:



\- ordinary matches;

\- the currently selected match.



Keep unmatched text fully readable.



Display:



\- total match count;

\- currently selected match;

\- previous/next controls.



Example:



```text

7 matches        ◀        Match 3 of 7        ▶

```



Zero-width matches are valid regex results and must not simply disappear from the match model.



Because a zero-width range cannot be painted like normal matched text, represent it with an appropriate visual marker or selection indicator rather than inventing a character span that did not actually match.





\## Match Navigation



Provide:



\- Previous Match;

\- Next Match;

\- wraparound navigation;

\- selection by clicking a match in Match Information.



Changing the selected match must synchronize:



\- active-match highlighting;

\- match-information display;

\- capture-group display.



Where practical using the egui version already in the project, navigation should bring the selected match into view.



Do not implement navigation by mutating the test text.





\## Match Information



Expose useful information for each match:



\- match number;

\- matched text;

\- source line;

\- source column or equivalent user-friendly location;

\- internal range/span;

\- numbered capture groups;

\- named capture groups;

\- captured values;

\- explicit indication when an optional capture group did not participate.



Rust regex ranges are byte offsets.



Do not incorrectly label byte offsets as Unicode character indexes.



Internally retain the exact byte ranges needed by `regex`, then derive safe user-facing location information such as line/column where appropriate.





\## Matching Lines



Make individual matches easy to scan.



The Match Information area should include a compact list resembling:



```text

Match 1    line 3     "user@example.com"

Match 2    line 8     "admin@example.com"

Match 3    line 14    "test@example.com"

```



Selecting a row activates that match.



Long values should be previewed safely without making the panel unusably wide; the full value should remain available in the detailed match view or copy action.





\## Regex Flags



Support the Rust-regex options appropriate to the selected engine:



\- case insensitive;

\- multiline;

\- dot matches newline;

\- Unicode;

\- ignore whitespace / extended mode where supported.



Use a compact control near the pattern.



Also provide a familiar visual expression such as:



```text

/pattern/imsux

```



Do not implement a misleading JavaScript/PCRE-style `g` engine flag.



The tester already evaluates all matches; if desired, describe that as an implicit "all matches" behavior in the UI rather than pretending Rust regex has a global flag.





\## Explanation



Provide a deterministic, local explanation system.



Examples:



```text

\\d          digit character class

\\s          whitespace character class

\[a-z]       character range

^           start anchor

$           end anchor

(...)       capturing group

(?:...)     non-capturing group

\*           zero or more

\+           one or more

?           optional / modifier depending on context

{2,5}       repetition range

a|b         alternation

```



Prefer using a reliable regex parser/AST facility compatible with the selected Rust regex syntax rather than attempting to write an entire regex parser manually.



Investigate `regex-syntax`, ideally using a version compatible with the current `regex` dependency.



Accuracy is more important than completeness.



If part of an expression cannot be confidently explained:



\- omit that explanation;

\- show a generic structural description;

\- or display the actual parser/compiler error.



Never fabricate an explanation.



This is NOT an AI explanation system.





\## Quick Reference



Provide a built-in searchable Quick Reference.



Recommended categories:



\- Character Classes

\- Anchors

\- Quantifiers

\- Groups

\- Alternation

\- Escaping

\- Unicode

\- Common Patterns



Each entry should include:



\- syntax;

\- short plain-language description;

\- optionally a tiny example.



Provide convenient actions such as:



\- Insert;

\- Copy.



Avoid a click accidentally replacing the user's entire expression unless that interaction is deliberately labeled.





\## Built-In Examples



Bundle useful examples directly with Multi Launcher.



No network access.



Initial examples should include at least:



\- email-like text;

\- IPv4 address;

\- URL;

\- UUID;

\- date;

\- time;

\- hex color;

\- integer;

\- decimal number;

\- whitespace cleanup;

\- file extension;

\- simple `key=value`.



Each example should contain:



\- name;

\- short description;

\- regex;

\- applicable flags;

\- sample input;

\- optionally a substitution example when useful.



Provide a clear "Load Example" action.



Loading an example may intentionally replace the current draft, but it must be explicit enough that the user understands what will happen.



Do not market simplified patterns as standards-complete validators when they are not. For example, call a practical simple email example "email-like text" rather than "RFC-compliant email validation."





\## Substitution Mode



Include substitution/replacement testing.



The user should be able to provide:



```text

Pattern

Replacement

Input Text

```



and see the local resulting text.



Use the actual replacement semantics of the Rust regex engine.



Support its capture replacement syntax correctly, including numbered/named captures where supported.



Provide:



\- replacement input;

\- output preview;

\- Copy Result.



Substitution testing must NEVER automatically:



\- modify a file;

\- modify another application;

\- replace clipboard contents merely because evaluation ran.



Clipboard modification occurs only through an explicit copy command.





\## Input Model



Use one main multiline test-text document.



Do not build multiple named test-input tabs in this goal.



Input sources:



\- typing;

\- paste;

\- explicit "Use Clipboard Text".



Do not add file loading or drag/drop file processing to the first version.





\## Clipboard and Copy Actions



Support explicit copying of:



\- regex pattern;

\- selected match;

\- all matches;

\- selected capture value;

\- substitution result;

\- readable match information.



Reuse existing Multi Launcher clipboard facilities where practical.



Do not add CSV/JSON export tooling in this goal.





\## Recent History



Maintain a bounded local recent-regex history.



History should primarily retain:



\- pattern;

\- flags;

\- useful timestamp/order metadata if needed.



Do NOT automatically persist arbitrary test-text buffers in recent history.



History should:



\- deduplicate repeated pattern/flag combinations;

\- promote reused entries;

\- have an explicit bounded size;

\- avoid unbounded persistence growth.





\## Saved Presets



Allow users to explicitly save regex presets.



A preset may contain:



\- user-defined name;

\- pattern;

\- flags;

\- optional sample text;

\- optional substitution expression.



Support:



\- create;

\- load;

\- rename;

\- update;

\- delete.



Saved sample text is allowed because saving a preset is an explicit user action.



Do not silently turn every regex history entry into a saved preset.





\## Persistence



Keep Regex Tester data local.



Use existing Multi Launcher typed persistence boundaries such as the shared JSON load/save helpers and atomic write behavior rather than raw ad-hoc writes.



Persist history and presets in domain-owned storage.



Malformed persistence must not silently overwrite the original malformed data with an empty/default file.



Treat persistence failures as recoverable UI diagnostics wherever practical.





\## Opening the Tester



Register a normal launcher command:



```text

regex

```



Executing it opens the dedicated Regex Tester.



Do not require inline regex syntax such as:



```text

regex \\d+ "abc 123"

```



for this goal.



The dedicated tester is the supported workflow.



The Regex Tester should use the existing typed command architecture rather than bypassing it with one-off action-string handling.





\## Window / Panel Behavior



Use the existing application's large utility/dialog patterns as guidance.



The existing Diff utility is a more appropriate architectural reference for a large resizable tool than the small Convert popup.



The Regex Tester must:



\- open reliably from `regex`;

\- be closable with normal Multi Launcher conventions;

\- retain unsaved in-memory draft state while appropriate;

\- not corrupt the main launcher query;

\- not change launcher hotkey semantics;

\- not change behavior of other dialogs;

\- not create duplicate egui IDs;

\- behave reasonably if opened repeatedly.





\## Large Input Policy



Optimize for normal interactive development/test/log text.



Do not attempt to become a huge-file regex processor.



Define explicit named limits rather than scattered magic numbers.



Determine a reasonable threshold through representative profiling/testing.



When input becomes too large for comfortable live evaluation:



\- remain responsive;

\- show a clear warning;

\- degrade or suspend expensive live behavior predictably.



Do not freeze the launcher.



Do not route giant-file processing into this feature; File Search remains the appropriate tool for that class of work.





\# Explicit Non-Goals



Do NOT expand the task to include:



\- multiple regex engines/flavors;

\- PCRE2 emulation;

\- JavaScript regex emulation;

\- Python regex emulation;

\- .NET regex emulation;

\- backreference emulation;

\- lookaround emulation;

\- online regex services;

\- external websites;

\- AI regex generation;

\- AI explanations;

\- cloud synchronization;

\- community sharing;

\- public regex libraries;

\- benchmark-comparison mode;

\- regex-language conversion;

\- a user-facing regex unit-test framework;

\- pipeline builders;

\- extraction/export studios;

\- CSV export;

\- JSON result export;

\- huge-file regex processing;

\- file modification;

\- direct modification of other applications;

\- multiple named input documents.



Do not add unrelated improvements discovered while working on this feature unless they are necessary for correctness.





\# Architecture Direction



Prefer a separation similar to:



```text

Regex Tester Domain

├── model

├── evaluator

├── explanation

├── built-in reference/examples

└── persistence



Plugin / Command Integration

└── "regex" opens tester



GUI

└── Regex Tester dialog

```



One reasonable file organization might be:



```text

src/

├── regex\_tester/

│   ├── mod.rs

│   ├── model.rs

│   ├── engine.rs

│   ├── explanation.rs

│   ├── reference.rs

│   └── persistence.rs

├── plugins/

│   └── regex\_tester.rs

└── gui/

&#x20;   └── regex\_tester\_dialog.rs

```



This is guidance, not an instruction to force unnecessary files.



Inspect existing conventions and adjust the exact split if the current architecture suggests a cleaner arrangement.



Critical rule:



\*\*The regex evaluation/domain layer must be testable without rendering egui.\*\*



Do not bury core matching, capture extraction, substitution, history management, or persistence semantics inside the UI rendering function.





\# Active Checkpoint Commit Cadence



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



When useful, include a short commit body explaining what changed, why, and what

behavior was intentionally preserved.



Do not run expensive full verification before every commit. Use small/local

checks where useful, commit coherent checkpoints, and perform the plan's

substantive targeted verification at the appropriate verification milestone.



If later testing or review finds a defect, prefer a clearly described follow-up

remediation commit rather than silently folding unrelated fixes into an earlier

checkpoint.



Do not squash or rewrite the checkpoint history unless explicitly requested.





\# Milestone 0 — Baseline Inspection and Integration Contract



\## M0-A — Confirm Current Architecture Before Editing



Inspect at minimum:



\- `Cargo.toml`

\- `src/plugin.rs`

\- `src/plugins/mod.rs`

\- representative utility plugins such as Diff/Convert

\- `src/commands/model.rs`

\- `src/commands/parser.rs`

\- `src/commands/host.rs`

\- dialog command handlers

\- `src/gui/mod.rs`

\- `src/gui/render.rs`

\- existing persistence helpers

\- existing clipboard helpers

\- current test conventions



Confirm specifically:



1\. the current `regex` crate version;

2\. current command routing for utility dialogs;

3\. current plugin registration conventions;

4\. current panel/dialog state conventions;

5\. current persistence/data-directory conventions;

6\. current copy-to-clipboard conventions;

7\. current mechanism for repainting/re-evaluating interactive dialogs.



Do not make speculative architectural changes before this inspection is complete.



This stage does not require a commit if it produces no code/document change. Do

not create an empty checkpoint merely to satisfy the stage ID.





\# Milestone 1 — Pure Regex Domain and Evaluation Engine



\## M1-A — Define Regex Tester Models



Create domain types for at least:



\- regex flags/options;

\- regex draft/input state where appropriate;

\- compiled/evaluation result;

\- individual match;

\- capture group;

\- named capture;

\- user-facing source location;

\- validation error;

\- substitution result;

\- selected-match identity.



Represent source ranges accurately.



Do not conflate:



\- UTF-8 byte offsets;

\- character offsets;

\- line/column display values.



Create helper functions for safe line/column derivation.



Model flags independently from egui checkbox state.



Suggested checkpoint commit:



`feat(regex): \[M1-A] add regex tester domain models and flag configuration`





\## M1-B — Implement Compilation and Match Evaluation



Build a pure evaluator around Rust `regex`.



It should:



1\. accept pattern + flags + test text;

2\. compile using `RegexBuilder` or equivalent;

3\. return structured validation errors;

4\. iterate all matches;

5\. gather full match spans;

6\. gather numbered captures;

7\. gather named captures;

8\. preserve optional unmatched captures;

9\. calculate safe display locations;

10\. handle empty/zero-width matches correctly.



The evaluator must not know about:



\- egui;

\- launcher windows;

\- clipboard;

\- persistence.



Add focused unit tests for:



\- literal matches;

\- multiple matches;

\- multiline matching;

\- case-insensitive matching;

\- dot-newline behavior;

\- Unicode enabled/disabled behavior where meaningful;

\- ignore-whitespace behavior;

\- numbered captures;

\- named captures;

\- optional unmatched captures;

\- invalid patterns;

\- zero-width matches;

\- Unicode strings;

\- CRLF and LF line calculations.



Suggested checkpoint commit:



`feat(regex): \[M1-B] implement local match and capture evaluation`





\## M1-C — Implement Substitution Engine



Add pure replacement evaluation.



Cover:



\- plain replacement;

\- replacing all matches;

\- numbered captures;

\- named captures;

\- literal replacement behavior;

\- no-match behavior;

\- invalid-pattern behavior.



Return structured result/error data suitable for rendering.



Do not copy anything to the clipboard from the domain layer.



Add focused tests.



Suggested checkpoint commit:



`feat(regex): \[M1-C] add local regex substitution evaluation`





\# Milestone 2 — Explanation, Reference, and Examples



\## M2-A — Add Deterministic Regex Explanation



Investigate and use the Rust regex syntax parser where practical.



Prefer `regex-syntax` or another parser consistent with the actual Rust regex

engine rather than inventing a competing grammar.



Build a structured explanation representation such as:



```text

token/span

kind

display label

description

```



Explain only constructs that can be identified accurately.



Cover common high-value syntax first:



\- literals;

\- escaped character classes;

\- character classes;

\- ranges;

\- anchors;

\- groups;

\- named groups;

\- non-capturing groups;

\- alternation;

\- quantifiers;

\- repetition ranges;

\- Unicode classes where available.



On unsupported or ambiguous structures:



\- do not guess;

\- preserve correct matching behavior;

\- show less information rather than wrong information.



Test representative expressions.



Suggested checkpoint commit:



`feat(regex): \[M2-A] add deterministic Rust regex explanations`





\## M2-B — Build Local Quick Reference Catalog



Create a static/local Quick Reference data model.



Include the approved categories:



\- Character Classes

\- Anchors

\- Quantifiers

\- Groups

\- Alternation

\- Escaping

\- Unicode

\- Common Patterns



Each entry should have:



\- title;

\- syntax;

\- description;

\- optional tiny example;

\- search terms if useful.



Keep data local and deterministic.



Add tests ensuring:



\- categories are non-empty;

\- required core entries exist;

\- duplicate identifiers do not exist.



Suggested checkpoint commit:



`feat(regex): \[M2-B] add searchable local regex quick reference data`





\## M2-C — Build Local Example Catalog



Create bundled examples for at least:



\- email-like text;

\- IPv4;

\- URL;

\- UUID;

\- date;

\- time;

\- hex color;

\- integer;

\- decimal;

\- whitespace cleanup;

\- file extension;

\- `key=value`.



Each example should include the approved data.



Add validation tests that compile every bundled example using the same engine

used by the tester.



Where an example promises expected matches, verify those expectations in tests.



This prevents the built-in learning material from silently rotting.



Suggested checkpoint commit:



`feat(regex): \[M2-C] add validated built-in regex examples`





\# Milestone 3 — Local History and Preset Persistence



\## M3-A — Implement Recent Regex History



Create a bounded history store.



Requirements:



\- pattern + flags;

\- deduplication;

\- most-recent ordering;

\- bounded count;

\- no automatic persistence of arbitrary test text;

\- loading malformed data must not destroy it;

\- persistence errors remain recoverable.



Use the project's shared persistence boundary and atomic JSON write behavior.



Tests should cover:



\- missing file;

\- empty file;

\- valid file;

\- malformed file;

\- deduplication;

\- cap enforcement;

\- repeated entry promotion.



Suggested checkpoint commit:



`feat(regex): \[M3-A] add bounded local regex history persistence`





\## M3-B — Implement Saved Presets



Create explicit preset persistence containing:



\- stable identifier if appropriate;

\- name;

\- pattern;

\- flags;

\- optional sample text;

\- optional replacement expression.



Support domain operations for:



\- create;

\- load;

\- rename;

\- update;

\- delete.



Protect against:



\- duplicate IDs/names according to chosen policy;

\- malformed storage;

\- accidental overwrite of malformed data.



Test all mutation paths.



Suggested checkpoint commit:



`feat(regex): \[M3-B] add saved regex preset persistence`





\# Milestone 4 — Plugin and Typed Command Integration



\## M4-A — Add Regex Tester Plugin



Create a normal built-in plugin using existing plugin conventions.



The user-facing launcher command is:



```text

regex

```



It should clearly describe itself as opening the local Regex Tester.



Register the plugin in:



\- the plugins module;

\- normal built-in plugin registration;

\- any required command/completion surfaces.



Do not add online search behavior.



Do not overload the command with inline regex parsing in this goal.



Add plugin search/command tests.



Suggested checkpoint commit:



`feat(regex): \[M4-A] register local regex tester launcher plugin`





\## M4-B — Add Typed Open Command



Extend the typed command system rather than special-casing the raw action string in UI code.



Use the existing dialog command/host architecture or a comparably appropriate typed domain determined during M0.



Required behavior:



```text

regex

&#x20;   ↓

typed action

&#x20;   ↓

typed command

&#x20;   ↓

host opens Regex Tester

```



Add parser tests and handler/host tests.



Ensure malformed or unrelated commands continue to route as before.



Suggested checkpoint commit:



`feat(regex): \[M4-B] route regex tester through typed dialog commands`





\# Milestone 5 — Regex Tester Dialog Shell and Layout



\## M5-A — Add Dialog State and Lifecycle



Create dedicated Regex Tester UI state.



Integrate it with:



\- `LauncherApp`;

\- panel/dialog state;

\- render cycle;

\- open/close lifecycle.



Repeatedly executing `regex` must not create duplicated logical state or duplicate egui widget IDs.



Opening/closing the tester must preserve unrelated launcher behavior.



Establish a sensible initial size for a desktop utility and allow resizing.



The tool should be substantially larger than the small Convert popup.



Suggested checkpoint commit:



`feat(regex-ui): \[M5-A] add regex tester dialog lifecycle and app integration`





\## M5-B — Implement the Core Three-Area Layout



Build:



1\. pattern area at top;

2\. large test-text editor;

3\. collapsible right information/reference area;

4\. bottom or nearby match status/navigation.



Provide compact flag controls near the pattern.



Show the Rust regex flavor clearly, for example:



```text

Engine: Rust regex

```



Show the pattern in the familiar conceptual form:



```text

/<pattern>/imsux

```



Keep raw pattern editing separate from decorative delimiters so `/` characters in

the expression are not unnecessarily escaped merely for display.



Suggested checkpoint commit:



`feat(regex-ui): \[M5-B] build regex101-inspired tester layout`





\# Milestone 6 — Live Evaluation and Validation UX



\## M6-A — Add Debounced Evaluation State



Track when pattern, flags, or text change.



Use a non-blocking debounce strategy.



Do NOT:



\- sleep the UI thread;

\- spawn unlimited work for every keystroke;

\- force continuous repaint when nothing changed.



Evaluation should happen after a small idle interval.



If the existing egui architecture makes synchronous evaluation of bounded text

simpler and profiling confirms it is sufficiently cheap, prefer that over

unnecessary worker complexity.



The pure evaluation layer from M1 must remain reusable either way.



Suggested checkpoint commit:



`feat(regex-ui): \[M6-A] add debounced live regex evaluation`





\## M6-B — Add Inline Syntax/Error Feedback



Render compiler/parser errors near the regex field.



Requirements:



\- readable error text;

\- no modal popup;

\- no repetitive error toast;

\- normal incomplete typing remains comfortable;

\- clearing/fixing the pattern clears the stale error.



If error spans/locations are available reliably from the parser, expose them.



Do not fake precise locations if the underlying library does not provide them.



Suggested checkpoint commit:



`feat(regex-ui): \[M6-B] add inline regex validation feedback`





\# Milestone 7 — Match Highlighting and Navigation



\## M7-A — Add Inline Match Highlighting



Integrate match ranges into the multiline editor rendering.



Use egui's supported text layout/edit APIs for the project's current version.



Preserve normal editing behavior:



\- cursor movement;

\- selection;

\- copy/paste;

\- insertion/deletion;

\- scrolling.



Do not implement highlighting by replacing/mutating the underlying source text.



Style:



\- normal matches clearly visible;

\- active match visually distinct;

\- text remains readable;

\- colors should derive sensibly from the current Multi Launcher theme rather than

&#x20; hard-coded assumptions that only look correct in one theme.



Handle:



\- adjacent matches;

\- Unicode;

\- multiline matches;

\- zero-width matches.



Suggested checkpoint commit:



`feat(regex-ui): \[M7-A] highlight live regex matches in test text`





\## M7-B — Add Match Selection and Previous/Next Navigation



Maintain an active match index.



Implement:



\- Previous;

\- Next;

\- wraparound;

\- `Match N of M`;

\- reset/clamp selection after reevaluation;

\- clicking a match-information row selects it.



If the selected match disappears when the expression changes, select a sensible

remaining match rather than retaining an invalid index.



Suggested checkpoint commit:



`feat(regex-ui): \[M7-B] add synchronized regex match navigation`





\## M7-C — Build Match Information and Capture Inspection



Render the match list requested by the product requirements.



For each selected match show:



\- full match;

\- line/location;

\- exact internal span where useful;

\- numbered captures;

\- named captures;

\- unmatched optional groups.



Add explicit copy actions for:



\- selected match;

\- selected capture.



Avoid enormous single-line values destroying panel layout.



Suggested checkpoint commit:



`feat(regex-ui): \[M7-C] add match and capture inspection panel`





\# Milestone 8 — Explanation, Quick Reference, and Examples UI



\## M8-A — Add Explanation Panel



Render structured explanations from M2.



Prefer a compact token-by-token list/table such as:



```text

\\d       Character class     Digit

\+        Quantifier          One or more

(...)    Capture group       Captures the contained match

```



Keep explanation read-only.



Invalid patterns should show the validation error rather than misleading partial

analysis unless the parser safely supports partial analysis.



Suggested checkpoint commit:



`feat(regex-ui): \[M8-A] expose deterministic regex explanations`





\## M8-B — Add Searchable Quick Reference



Add local search/filtering.



Allow browsing categories.



Each entry should provide clear actions:



\- Insert;

\- Copy.



Insertion should occur at the current pattern cursor if the egui editing API

supports it reliably; otherwise use the safest predictable insertion behavior

and make it explicit.



Do not silently erase the entire pattern merely because a reference entry was

clicked.



Suggested checkpoint commit:



`feat(regex-ui): \[M8-B] add searchable regex quick reference`





\## M8-C — Add Examples Browser



Provide searchable/browsable examples.



A selected example displays:



\- name;

\- description;

\- regex;

\- sample text.



Provide an explicit:



`Load Example`



action.



Loading it should populate the tester with its pattern, flags, sample text, and

replacement example where present.



Suggested checkpoint commit:



`feat(regex-ui): \[M8-C] add built-in regex examples browser`





\# Milestone 9 — History and Preset UI



\## M9-A — Add Recent Regex History UX



Expose recent patterns without overwhelming the main layout.



A small history section/dropdown/panel is appropriate.



Show enough information to distinguish entries:



\- pattern;

\- flag suffix;

\- recency.



Loading an entry restores:



\- pattern;

\- flags.



It does not restore arbitrary prior test text because history does not persist it.



Suggested checkpoint commit:



`feat(regex-ui): \[M9-A] add recent regex history browser`





\## M9-B — Add Saved Preset UX



Expose:



\- Save Preset;

\- Load;

\- Rename;

\- Update;

\- Delete.



When saving, allow the user to control whether current sample text and

replacement text become part of the explicit preset.



Do not make destructive preset operations ambiguous.



Suggested checkpoint commit:



`feat(regex-ui): \[M9-B] add regex preset authoring and management`





\# Milestone 10 — Substitution Mode and Copy Operations



\## M10-A — Add Substitution UI



Provide a clear mode or collapsible subsection for replacement testing.



Show:



\- replacement expression;

\- resulting text.



Keep test input editable.



Replacement preview updates through the same debounced evaluation model.



Clearly distinguish:



```text

Input

```



from:



```text

Replacement Result

```



The replacement result should be read-only.



Suggested checkpoint commit:



`feat(regex-ui): \[M10-A] add interactive regex substitution mode`





\## M10-B — Add Clipboard Input and Copy Actions



Add explicit:



\- Use Clipboard Text;

\- Copy Pattern;

\- Copy Selected Match;

\- Copy All Matches;

\- Copy Selected Capture;

\- Copy Replacement Result;

\- Copy Match Information.



Reuse established clipboard behavior in the current application.



Clipboard read failures should produce a localized, understandable error.



Do not add clipboard polling.



Suggested checkpoint commit:



`feat(regex-ui): \[M10-B] add regex clipboard and copy workflows`





\# Milestone 11 — Responsiveness, Edge Cases, and Keyboard UX



\## M11-A — Establish Large-Input Guardrails



Profile representative inputs.



Create named constants/policies for:



\- normal live-evaluation range;

\- warning threshold;

\- any hard guard if necessary.



The behavior must be explicit and testable.



A large input should never cause an accidental infinite repaint/evaluation loop.



Remember that Rust regex itself avoids traditional catastrophic backtracking,

but rendering and capture/result production can still become expensive with

large text or enormous match counts.



Consider limiting the number of match rows rendered/materialized for display

while still clearly reporting that additional matches exist, if measurements

show this is necessary.



Suggested checkpoint commit:



`perf(regex): \[M11-A] bound interactive regex workloads`





\## M11-B — Keyboard and Focus Polish



Ensure normal editing shortcuts remain normal.



Add only useful, non-conflicting navigation shortcuts consistent with the

existing project.



At minimum verify:



\- Tab/focus traversal is usable;

\- Esc closes/goes back according to Multi Launcher conventions;

\- keyboard navigation can reach match navigation and side-panel controls;

\- opening the tester gives focus to the regex field or another sensible initial

&#x20; target;

\- copying does not unexpectedly steal long-term focus.



Add concise shortcut hints where helpful.



Do not introduce a completely separate keyboard vocabulary for this one tool.



Suggested checkpoint commit:



`feat(regex-ui): \[M11-B] polish regex tester keyboard and focus behavior`





\## M11-C — Theme, Resize, and Layout Polish



Verify:



\- normal launcher theme;

\- dark/light or alternate theme behavior supported by the current application;

\- narrow window;

\- wide window;

\- collapsed side panel;

\- very long regex;

\- very long match text;

\- no results;

\- hundreds/thousands of matches;

\- substitution mode;

\- empty input.



Avoid fixed dimensions that clip badly under Windows display scaling.



Suggested checkpoint commit:



`fix(regex-ui): \[M11-C] harden regex tester responsive layout`





\# Milestone 12 — Targeted Regression and Contract Tests



\## M12-A — Complete Domain-Level Test Matrix



Ensure automated tests cover:



\### Compilation / flags

\- valid pattern;

\- invalid pattern;

\- `i`;

\- `m`;

\- `s`;

\- Unicode behavior;

\- extended/ignore-whitespace mode.



\### Matching

\- zero matches;

\- one match;

\- many matches;

\- adjacent matches;

\- zero-width matches;

\- multiline matches;

\- Unicode matches.



\### Captures

\- numbered captures;

\- named captures;

\- optional captures;

\- unmatched captures;

\- repeated groups.



\### Location

\- first line;

\- later lines;

\- LF;

\- CRLF;

\- Unicode.



\### Substitution

\- simple replacement;

\- replacement with captures;

\- named replacement;

\- no match;

\- multiple matches.



\### Explanation/reference/examples

\- expected common constructs;

\- every built-in example compiles;

\- example expected-match contracts.



Suggested checkpoint commit:



`test(regex): \[M12-A] complete regex domain coverage`





\## M12-B — Complete Integration and Persistence Tests



Cover:



\- `regex` plugin command appears;

\- plugin search routes correctly;

\- action parses into typed command;

\- typed handler opens tester;

\- repeated open behavior;

\- panel lifecycle;

\- history load/save;

\- history deduplication;

\- history limit;

\- preset CRUD;

\- malformed history/preset storage preservation;

\- copy action logic where testable without OS flakiness.



Suggested checkpoint commit:



`test(regex): \[M12-B] cover regex command and persistence integration`





\## M12-C — GUI-State Regression Tests



Prefer state/logic tests over brittle pixel automation.



Test separable GUI state such as:



\- active match index after reevaluation;

\- navigation wraparound;

\- panel collapse state if persisted/session-owned;

\- loading example;

\- loading history;

\- loading preset;

\- substitution mode state;

\- invalid regex transition;

\- zero-result transition;

\- selected match disappearing after edit;

\- large-input warning state.



Do NOT introduce a heavyweight automated Windows UI testing framework for this

feature.



Suggested checkpoint commit:



`test(regex-ui): \[M12-C] cover regex tester interaction state`





\# Milestone 13 — Documentation and Final Review



\## M13-A — Update User Documentation



Update README/plugin documentation to include:



```text

regex

```



Document:



\- local/offline behavior;

\- Rust regex flavor;

\- live matching;

\- supported flags;

\- capture inspection;

\- substitution;

\- Quick Reference;

\- examples;

\- history;

\- presets.



Explicitly mention major Rust-regex limitations that users coming from regex101

might otherwise mistake for bugs, especially unsupported constructs such as

look-around/backreferences.



Suggested checkpoint commit:



`docs(regex): \[M13-A] document local regex tester`





\## M13-B — Reviewer Pass



Have a reviewer inspect the completed feature against this plan.



Review specifically for:



\- accidental online/network behavior;

\- incorrect regex flavor claims;

\- misleading explanations;

\- byte-vs-character indexing errors;

\- Unicode bugs;

\- zero-width match handling;

\- egui ID collisions;

\- uncontrolled repaint loops;

\- unbounded history;

\- accidental persistence of test text;

\- preset data loss;

\- action parser regressions;

\- launcher hotkey regressions;

\- duplicated command architecture;

\- oversized files/functions that should be factored;

\- unnecessary complexity.



Any substantive defects found should receive explicit remediation commits such

as:



`fix(regex): \[M13-C] preserve zero-width matches during navigation`



Do not silently amend or squash earlier checkpoints merely to make history look

clean.





\# Verification Strategy



Do NOT repeatedly run the complete project test suite after every checkpoint.



Use inexpensive checks during implementation:



```text

cargo fmt --check

targeted cargo test filters

targeted module tests

cargo check where appropriate

```



Run tests closely associated with the current checkpoint before committing when

they are inexpensive enough.



At major integration points, run progressively broader verification.



Before declaring completion, perform the project's expected substantive

verification, including the relevant `cargo nextest` coverage according to the

project's established workflow.



Because this project can have long compile/test cycles:



\- batch meaningful code before expensive verification;

\- do not start redundant full-suite runs;

\- do not cancel a legitimate long build merely because it is quiet;

\- inspect actual process/test state when determining whether a run is stalled.



If final verification exposes defects, fix them in clearly labeled remediation

commits rather than rewriting previous checkpoint history.





\# Required Manual Acceptance Pass



Before considering Goal B complete, manually verify at least:



1\. Run `regex` from Multi Launcher.

2\. Regex Tester opens.

3\. Enter:



&#x20;  `\\b\\w+@\\w+\\.\\w+\\b`



4\. Paste several matching and non-matching lines.

5\. Matches highlight live.

6\. Match count is correct.

7\. Previous/Next navigation works.

8\. Selected match is visually distinct.

9\. Match Information displays correct text and location.

10\. Numbered captures work.

11\. Named captures work.

12\. Invalid regex such as `\[` shows an inline error and does not disrupt the app.

13\. Toggle each supported flag and verify behavior.

14\. Quick Reference can be searched.

15\. Reference syntax can be copied/inserted.

16\. Load several built-in examples.

17\. Explanation does not claim unsupported semantics.

18\. Save a preset.

19\. Close/reopen and load the preset.

20\. Recent history survives restart.

21\. Arbitrary unsaved test text is not unexpectedly persisted.

22\. Test substitution with numbered/named captures.

23\. Copy substitution result.

24\. Use Clipboard Text.

25\. Collapse and restore the information panel.

26\. Resize the window substantially smaller and larger.

27\. Test Unicode input.

28\. Test a zero-width expression such as an anchor.

29\. Test a large but reasonable log/text sample.

30\. Close the tester and confirm normal launcher hotkey behavior is unchanged.





\# Definition of Done



Goal B is complete only when:



\- `regex` opens a native Multi Launcher Regex Tester;

\- operation is fully local/offline;

\- Rust regex flavor is accurately identified;

\- live matching works;

\- validation is inline;

\- all normal matches are highlighted;

\- zero-width matches remain represented;

\- match navigation works;

\- match/capture information works;

\- numbered and named captures work;

\- supported flags work;

\- substitution works;

\- Quick Reference works;

\- built-in examples work;

\- explanations are accurate and deterministic;

\- history works;

\- explicit saved presets work;

\- clipboard import/copy workflows work;

\- the right-side panel is collapsible;

\- normal-sized input remains responsive;

\- large input degrades safely;

\- existing launcher behavior remains intact;

\- targeted and final verification pass;

\- documentation is updated;

\- checkpoint commits remain visible and unsquashed.





\# Task-Specific Commit Map



Natural expected commit boundaries:



\- `M1-A` — domain models/flags

\- `M1-B` — matching/capture engine

\- `M1-C` — substitution engine

\- `M2-A` — deterministic explanations

\- `M2-B` — Quick Reference data

\- `M2-C` — built-in examples

\- `M3-A` — history persistence

\- `M3-B` — preset persistence

\- `M4-A` — plugin registration

\- `M4-B` — typed command routing

\- `M5-A` — dialog lifecycle

\- `M5-B` — main layout

\- `M6-A` — debounced live evaluation

\- `M6-B` — inline validation

\- `M7-A` — match highlighting

\- `M7-B` — navigation

\- `M7-C` — match/capture information

\- `M8-A` — explanation UI

\- `M8-B` — Quick Reference UI

\- `M8-C` — examples UI

\- `M9-A` — history UI

\- `M9-B` — preset UI

\- `M10-A` — substitution UI

\- `M10-B` — clipboard/copy workflows

\- `M11-A` — large-input guardrails

\- `M11-B` — keyboard/focus polish

\- `M11-C` — responsive/theme polish

\- `M12-A` — domain test completion

\- `M12-B` — integration/persistence tests

\- `M12-C` — GUI-state tests

\- `M13-A` — documentation

\- `M13-B+` — review/remediation as required



These are expected natural boundaries, not an instruction to commit unchanged or

meaningless work. Combine adjacent IDs only when the actual implementation

proves they are inseparable; split an ID further when it grows into multiple

independently understandable changes.



The objective is visible, useful progress in Git history rather than an

arbitrary commit count.



