# Multi Launcher — Productivity Expansion Master Implementation Plan

**Status:** Approved for autonomous Codex implementation  
**Product:** Multi Launcher, Rust / Windows / eframe-egui  
**Scope:** Four workstreams, **14 approved features** delivered in **12 feature milestones**, preceded by M0 and followed by M13.  
**Planning baseline:** Source reviewed October 16, 2026. **Codex's checked-out repository and `AGENTS.md` are authoritative at execution time.**  
**Companion document:** `multi_launcher_productivity_expansion_codex_kickoff.md`.

> **Execution contract:** Implement the entire approved scope without asking the user to run commands, manually verify a feature, provide screenshots, or approve checkpoints. Codex is authorized to build and run Multi Launcher, interact with its UI, use disposable test files, and capture screenshots on its own execution machine. The user will review the finished result only after implementation. Do not seek intermediate manual acceptance.

---

## 1. Approved scope at a glance

| Workstream | Milestone | Feature(s) | Delivery intent |
|---|---|---|---|
| Foundation | **M0** | Baseline / repository inventory | Identify owners, safeguard compatibility and establish a lightweight progress ledger. |
| Search & clipboard | **M1** | Search Scope Filters | Main launcher scopes: All, Applications, Files, Notes, Commands. |
| Search & clipboard | **M2** | Dynamic Snippet Variables; Snippet Categories / Tags | Extend existing `cs` without invalidating old snippets or prompts. |
| Search & clipboard | **M3** | Paste Sequence | Saved/one-off sequences using existing snippets, prompted fields, clipboard history and temporary text. |
| Developer tools | **M4** | Hash Generator | Text/file/clipboard hashing, compare, progress and cancellation. |
| Developer tools | **M5** | JSON Tree Explorer; JSON Key/Value Search | Integrated read-only tree and searchable navigation in existing JSON dialog. |
| Developer tools | **M6** | Regex Test Cases | Multiple expected-match/no-match cases saved with presets. |
| Developer tools | **M7** | XML Toolkit | Parse, format, XSD, XPath, tree, load/save/copy, secure local-first operation. |
| File operations | **M8** | Batch File Renamer | `fs`-style dialog, explicit directory, preview, conflict checks, execution and undo. |
| File operations | **M9** | Search-and-Replace with Preview | Scoped multi-file text replacement with per-match approval, diff and recovery. |
| File operations | **M10** | Hot Folder Automation | Persisted watched-folder rules, safe built-in actions and explicit launcher/macro actions. |
| Desktop utilities | **M11** | Screenshot Pinboard | Independent, temporary floating screenshots with multiple pins, zoom and click-through. |
| Desktop utilities | **M12** | Click Visualization | Lightweight configurable mouse click visual effects through the `mouse` feature family. |
| Integration | **M13** | Integration, cleanup and handoff | Documentation, compatibility tests, screenshots and final report; no feature creep. |

**Exactly 14 features:** M2 contains two; M5 contains two; the other ten feature milestones each contain one.

### Product principles

1. **Keyboard-first, snappy, Windows-native:** rapid invocation and responsive UI; no large synchronous file work or parsing in the per-frame egui path.
2. **Additive, not replacement:** preserve existing commands, `fs` searches, `cs` rendering/prompting, Regex Tester, JSON formatting, screenshot editor, mouse effects, macro hotkeys, radial menus and launcher hotkey behavior.
3. **No workspace functionality:** do not modify or expand workspace-related functionality as part of this effort. Existing unrelated code may remain untouched.
4. **Scoped side effects:** any file-writing feature must have an explicit target, a visible preview, and a clear user-initiated commit/enable step.
5. **One owner per concern:** domain rules belong in testable Rust modules; egui renders views and submits intentions; Windows specifics remain at narrow platform boundaries.
6. **No optional feature expansion:** do not add MD5, editable JSON tree cells, full-string regex expectations, keyboard press visualization, persistent pinboard restoration, generic macro redesign, or unrelated settings/theme revamps.
7. **Data safety:** no destructive test against real user data. Use temporary fixture roots and isolated application data whenever exercising write-capable actions.

---

## 2. Source-of-truth map and architectural boundaries

The following paths were present in the planning snapshot; **verify against the current checkout before changing them**, and locate changed equivalents instead of assuming this exact layout remains fixed.

| Area | Existing entry points / facts | Preserve |
|---|---|---|
| Launcher search | `src/gui/search.rs`, `src/gui/mod.rs`, `src/plugin.rs`, `src/plugins/omni_search.rs`; launcher search outcome is also used by radial resolution. | Query ranking/caching, prefix semantics, empty-query results, history, radial lookup, hotkey toggle. |
| Snippets | `src/plugins/snippets.rs` defines `SnippetEntry`, field metadata, atomic `snippets.json` persistence and live snapshots; `src/plugins/snippet_template.rs` has `{{name}}` parsing; `src/gui/snippet_dialog.rs` and `snippet_prompt_dialog.rs`. | Existing literal snippets, prompt fields, `snippet:run:` alias identity, hidden-content behavior, atomic saves, watchers and existing tests. |
| Clipboard | `src/plugins/clipboard.rs`, `src/actions/clipboard.rs`, `src/clipboard_modify/clipboard.rs`, `src/gui/clipboard_dialog.rs`, existing clipboard history. | Clipboard history/privacy and current clipboard action semantics. |
| File search | `src/file_search/{model,coordinator,executor,matching,path_identity,settings,...}.rs`, `src/gui/file_search_dialog.rs` and its submodules. | Existing `fs` filename/content search, cancellation, filters, result display, normal open/copy actions, and backend integrations. |
| File diff | `src/diff/` and `src/gui/diff_dialog/`. | Existing diff behavior; reuse relevant display primitives when practical rather than undertaking an unrelated redesign. |
| JSON | `src/json_transform.rs` and `src/gui/json_utility_dialog.rs`; strict document parse/format/minify behavior already exists. | Error semantics and original text-oriented transformations. |
| Regex | `src/regex_tester/`, `src/regex_tester/persistence/presets.rs`, `src/gui/regex_tester_dialog.rs` and submodules. Presets currently have a versioned persisted model. | Rust-regex dialect, pattern/flags, examples, match inspection, replacement, history, preset integrity. |
| Screenshot | `src/plugins/screenshot.rs`, `src/actions/screenshot.rs`, `src/gui/screenshot_editor.rs`, `src/gui/mod.rs` screenshot-editor ownership. | Region selection, clipboard/file capture, screenshot editor, multi-monitor coordinates. |
| Mouse | `src/plugins/mouse.rs`, `src/gui/mouse_settings_dialog.rs`, `src/coordinate_tool/`, `src/mouse_gestures/`. | `mouse` prefix, current cursor/gesture effects, session-only enabling and persisted appearance. |
| Plugin / command routing | `src/plugin.rs`, `src/plugins/mod.rs`, `src/commands/{model,parser,handlers,host,...}.rs`, `src/gui/mod.rs`, and existing help entries. | Typed command handling, central dispatch, plugin registration and existing command results. |
| Windows UI/launch | `src/active_window.rs`, `src/platform/windows_api.rs`, global hotkey and launcher activation code. | Hook ownership, HWND/focus lifetimes, graceful cleanup and non-interference with other automation. |
| Native lifecycle/testing | `dev/run_owned_process.ps1`, `dev/test_owned_process.ps1` and pertinent tests in `tests/`. | Safe ownership of any test-launched process; never terminate an unrelated user process. |

**Do not infer category from fragile action-string prefixes alone** when introducing search scopes. Where source provenance is needed, carry a small typed origin/category through the existing result boundary. Do not fork the entire search implementation or change radial search filtering.

**Do not create an unrelated generic automation framework.** Shared primitives should be introduced only where two or more selected features genuinely need them (e.g. path identity, operation preview, nonblocking file worker and cancel token).

---

## 3. Autonomous Codex execution protocol — mandatory

### 3.1 No user-operated verification, no approval gates

- Never tell the user to run `cargo`, launch Multi Launcher, test a hotkey, provide a screenshot, click through a dialog or sign off a checkpoint. Do not interrupt the job for manual validation.
- Codex may **build and launch Multi Launcher itself**, use automated UI controls when available, exercise feature commands and dialogs, create disposable test files and capture UI screenshots. This is express authorization to operate the development/test instance; it is **not** authorization to alter unrelated personal files or credentials.
- When Windows GUI automation is unavailable, use Rust domain tests, egui state tests or an available headless UI harness; record that native UI interaction was unavailable, **do not fabricate screenshots**, do not ask the user to supply them, and proceed through independent work.
- At the end, report changes, checkpoint commit hashes, validation actually performed, available screenshot paths and remaining genuine environmental/functional limitations. The user performs a **single final manual review** after Codex finishes.

### 3.2 Active checkpoint commit cadence

- Treat `M1-A`, `M1-B`, etc. as **small coherent commits** with working integration boundaries. Aim for **3–5 commits per normal milestone**; 5–6 for large file-automation/XML milestones. Subdivide further only if the code would otherwise become a large unreviewable diff.
- Commit after the checkpoint's relevant behavior is implemented, targeted tests pass and Clippy shows **no newly introduced warnings**. Where baseline Clippy is clean, keep it clean: preferred command `cargo clippy --all-targets -- -D warnings` on the Windows target/toolchain. Run it at **each completed checkpoint**, taking advantage of incremental compilation; do not postpone warnings until M13.
- Before each commit inspect `git status`, `git diff --check` and the touched diff. Do not stage pre-existing unrelated user changes. Commit messages: `feat(search): [M1-B] add scope chips and keyboard switching`, `feat(snippets): [M2-C] add tag filters`, `test(rename): [M8-D] cover collision and undo`, etc.
- An early failure during active editing is normal; fix it within the checkpoint. **Never claim a failing/blocked check passed.** Do not commit a knowingly broken feature merely to maintain cadence.
- Do not force-push, reset away someone else's work, change unrelated branches or simultaneously assign multiple agents overlapping write access. Write-heavy checkpoints execute **sequentially**. Planner/reviewer agents may inspect in parallel read-only.

### 3.3 Exact bounded verification policy — prevent spiraling

For **each checkpoint**, select:

1. **At most two focused automated test targets/slices** proving the changed behavior (one often suffices); no unrelated tests.
2. **One Clippy gate** after substantive code is ready. During repair, run only the narrowest needed compiler/test check, then repeat the gate **once after correction**. This does not excuse shipping new warnings.
3. **At most one native UI smoke session per completed *feature milestone***, generally with **1–3 screenshots** covering its principal visible states; do not repeat screenshot tours at every tiny commit.
4. Only M0 or M13 (or concrete shared-infrastructure regressions) may justify wider targeted integration checks; **do not automatically run full Nextest repeatedly** or initiate performance benchmarking campaigns.

**Retry limit:** For the *same failure and unchanged evidence*, perform **at most two corrective attempts**, followed by one focused recheck per correction. If still failing, capture the precise first root error, distinguish environmental from code-caused, and declare the checkpoint `blocked` rather than endlessly changing assertions, adding sleeps, or rerunning the same test. Continue **only independent, safely isolated** milestones; do not label the blocked feature complete. If an incomplete edit must be set aside, save its own diff/evidence and restore only Codex-owned changes without touching user work.

**Environment blockers:** One diagnostic pass and one retry for missing GUI capture, inaccessible Windows session, cargo toolchain/network setup, missing device/monitor or unavailable external schema dependency. Then log `not_run_environment` and move on where domain-level proof exists. A missing screenshot **does not block** a domain-complete feature; unverified behavior must be reported as such.

**Correctness priority:** These limits bound *repetitive verification*, not permission to ignore demonstrated file corruption or active regressions. If a mandatory safety invariant fails, **do not enable/ship** that part. Isolate and report it, continue only unrelated work.

### 3.4 UI smoke tests and screenshots

- When possible run an **owned development instance** with isolated test config/data. Reuse existing owned-process safety facilities if suitable; never kill arbitrary processes by name or PID guess.
- Generate test-owned fixtures: temporary text/XML/JSON files, snippets, watched folders, a harmless test paste target and programmatic screenshots. Prefer deterministic UI actions over cursor-coordinate scripts; avoid sleeps added solely to hide races.
- Capture a screenshot **after the UI reaches the intended state**: clear directory display/rename preview, selected replacement diff, populated XML diagnostics/tree, loaded JSON/regex test cases, paste editor, pinboard, click effect where technically possible. Screenshots are **visual evidence, not pixel-perfect acceptance tests**.
- Store redacted/generated screenshot evidence under a dedicated local `artifacts/productivity_expansion/` or equivalent; use Git ignore rules if appropriate to avoid committing giant or sensitive captures. Record screenshot filenames and observed states in the ledger/final report. Never capture real clipboard secrets or unrelated personal desktop content.
- For unsupported external displays, system hooks or focus conditions, use deterministic unit tests for coordinate/layout/state transitions and mark native coverage unavailable; **do not ask the user to take over**.

### 3.5 Progress and reviewer handoff

Create a small progress document such as `docs/plans/productivity_expansion_progress.md` at M0 (location may follow existing project convention). Track for each checkpoint: status (`pending`, `in_progress`, `complete`, `blocked`), commit hash, focused test name/result, Clippy result, screenshot path if any, and one-line issue. Update at checkpoint boundaries only; avoid verbose continuous logging.

For each milestone: planner (bounded source check) → **single implementer owning writes** → focused reviewer (correctness/scope/side effects) → implementer fixes substantive findings → checkpoint commit → next milestone. Do not repeatedly re-plan approved requirements or run successive review cycles after findings are resolved.

### 3.6 Cross-cutting protection

- Parse/validate before persisting; prefer existing atomic JSON persistence, config watchers, versioned models and clear error states. Old JSON files must load with defaults/migrations; do not silently rewrite or discard bad files.
- Implement cancellation/lifetime cleanup for background workers, monitors, clipboard playback and native overlays; workers must not outlive the intended feature/application, and UI updates should not happen from foreign threads directly.
- Bounded work per frame; expensive filesystem scans/hashes/XML/XPath run off the UI thread where necessary, with progress/cancel.
- Guard system hooks and overlays from capturing their own injected inputs or interfering with radial menus, MkMacro, native recording or user input.
- Use existing Windows-specific wrappers where appropriate. No new global input hook or polling loop without evidence the existing runtime cannot supply the events.
- Document new commands in in-app help and README at each respective milestone, rather than saving everything for the final sprint.
- Do not relax tests, disable lint gates or mask failures as the way to satisfy a checkpoint.

---

## 4. Common acceptance contracts

### 4.1 Main launcher integration

New features must appear through existing plugin registration, query prefixes and typed command dispatch; the user should not need an alternate application process. Preferred new names: `paste`, `rename`, `replace`, `hash`, `xml`, `hotfolder`; preserve `cs`, `json`, `regex`, `mouse` for enhancements. If a name conflicts with a verified existing command, choose a nonconflicting consistent spelling, document it, and test routing. For every command, include a searchable help entry and cancel/close behavior consistent with existing dialogs.

### 4.2 File mutation contract (M8–M10, plus M9)

- Filesystem roots/targets must be **explicit and visible**; preview never writes. All potentially mutating batch operations require a clear Apply/Confirm step (or explicit Enable for unattended rules). No automatic overwrite by default.
- For each candidate file record an identity and precondition (path, type, metadata and/or content fingerprint as appropriate); re-check before applying so stale previews do not silently mutate newer contents.
- Handle Windows reserved names, invalid characters, trailing spaces/dots, case-insensitive destination conflicts, symlinks/reparse points, read-only files, inaccessible locations and name/path-length errors. Do not traverse outside chosen roots by default or follow reparse points into unknown trees.
- Display partial successes and per-file failures; never present a whole batch as successful when some operations failed. A batch must be cancellable between files, though already-completed operations need an explicit undo path.
- Use backup/undo records with a target post-operation identity check; undo must not overwrite a file that another actor changed after the operation. Preserve original bytes and line endings for M9. Never use unconditional recursive delete.
- Test all file mutations only on temporary fixture roots (including simulated conflicts and changed-since-preview cases). No access to a real Downloads/Documents folder during Codex tests.

### 4.3 Persistent data contract

- New fields in existing `snippets.json` and Regex preset schemas must have migration/default behavior. Keep existing file locations where appropriate; store new unrelated collections in a separate documented JSON file rather than bloating `settings.json`.
- Persist only deliberate feature settings (e.g. saved paste sequences, hot folder rules, mouse click visual style). One-off snippets, pinned image content, active paste state and click animation frames are session-only.
- Redact hidden snippet content, clipboard text, target application contents and file-replacement preview contents from routine logs and screenshots.

### 4.4 Done means demonstrably complete

A milestone is complete only when its primary user flow, error/empty states, relevant cancellation or recovery, focused tests, plugin/dialog wiring, help/doc updates, and mandatory lint check are addressed. UI screenshots are opportunistic evidence where a supported GUI session exists. **“Code compiles” alone is not completion.** If environmental limitations prevent some verification, label that limitation explicitly rather than pretending it was performed.

---

## 5. Milestone execution order and dependencies

Implement in this order: **M0 → M1 → M2 → M3 → M4 → M5 → M6 → M7 → M8 → M9 → M10 → M11 → M12 → M13**. Earlier M2 is needed for M3; M8's file identity/preview conventions help M9/M10. Developer-tool milestones can proceed if an unrelated GUI-dependent feature is blocked, provided the working tree is isolated and clean.

Dependency edges **do not** authorize broad framework refactoring. If current source already supplies a relevant helper, reuse it and omit needless duplicate scaffolding.

---

## M0 — Repository baseline and compatibility map

**Objective:** Establish a reliable starting point without building an unnecessary baseline campaign. **Owner:** orchestrator/planner; no feature source changes required. **Depends on:** current selected branch.

### M0-A — Confirm current state and owners

- Record branch/HEAD, working-tree cleanliness, toolchain and Windows host availability. Preserve pre-existing modifications, including untracked files.
- Read `AGENTS.md`, trace current launcher search, commands and plugin registry; trace relevant snippets, file search, JSON, regex, screenshots, mouse and persistence paths using the checkout, not historical assumptions.
- List existing directly relevant test targets and note which individual baseline behaviors are already proven. Identify only material conflicts between this plan and repository state.
- Record the current `cargo clippy --all-targets -- -D warnings` outcome **once** (use the project's appropriate target/profile if needed). If baseline contains pre-existing warnings, record the exact set; perform only a bounded directly relevant cleanup before feature work or identify unrelated baseline issues separately. Do not spend unlimited cycles fixing unrelated old lints.

### M0-B — Create compact execution ledger and fixture policy

- Add the checkpoint status ledger, review/evidence location, temporary-fixture naming convention and plan pointer to the repository if appropriate.
- Record that UI smoke is autonomous and that file-changing tests are limited to disposable fixtures.
- No artificial M0 commit if no repository files changed. If the ledger is created, use `docs(plan): [M0-B] initialize expansion ledger`.

**Acceptance:** branch/checkout known, relevant owners identified, lint baseline honestly recorded, safety/evidence locations established; no feature or unrelated source churn.

---
## M1 — Search Scope Filters

**Feature contract:** The launcher shows compact **All / Applications / Files / Notes / Commands** scope selectors. Query stays unchanged when switching scopes; results update immediately. Keyboard switching works without clicking. Every *new* launcher query session begins in **All** (no persistence). `fs` itself is **not** replaced or given a second interpretation of its existing Global/Directory options.

**Owner/dependencies:** `src/gui/search.rs`, relevant result types and `src/gui/mod.rs` query UI; existing plugin and indexing providers remain authoritative. Depends on M0 only.

### M1-A — Source classification and result boundary

- Inventory where launcher results originate: indexed apps/files, notes, local commands and dynamic plugins. Define a small typed result scope/provenance attached at the appropriate search result aggregation boundary, **without modifying persisted Action semantics or reclassifying everything by parsing labels**.
- Classify overlapping results deterministically; document policy for provider results that do not have a precise type (treat generic plugin commands as Commands unless a strongly typed provider owner supplies Notes/Files/etc.).
- Preserve exact `All` search ordering and existing empty-query behavior. Do **not** change the unfiltered `search_read_only_outcome` used by radial/dynamic command resolution.
- Add focused classification tests (including aliases, note actions, regular commands, indexed files and unlabeled provider results).
- **Commit:** `feat(search): [M1-A] identify launcher result scopes`.

### M1-B — Scope state, cache invalidation and UI controls

- Add session-local active scope, default `All`; reset it when a new launcher query session begins, not on every render or keystroke.
- Add unobtrusive chips/tabs or an equivalent compact scope strip, visually indicating active scope. Preserve the existing search field position, result selection semantics, autocomplete and accessibility focus order.
- Install keyboard switching after inspecting existing launcher bindings; choose a nonconflicting shortcut (prefer number-based scope shortcuts or Ctrl+Tab only if not already occupied) and surface shortcut hints in UI/help.
- Filter results by the typed scope and include scope in the result-cache/validity identity so identical query text with a different scope cannot reuse stale filtered results. Recompute selection coherently, with empty-state feedback.
- **Commit:** `feat(search): [M1-B] add launcher scope controls`.

### M1-C — Explicit commands and compatibility checks

- Explicit plugin-prefixed queries (`fs`, `regex`, `cs`, `mouse`, etc.) must continue resolving according to existing behavior even when the user previously selected another scope. Define override/fallback only for deliberately typed plugin commands; do not surprise users by swallowing a command.
- Test: query unchanged across switches, scope reset on a fresh launch, `All` parity, prefix/command routing, no-result state, selection reset and hotkey show/hide consistency. Cover search cache invalidation and radial results independently.
- When GUI automation is available, launch Multi Launcher and capture one screen with scope controls and one scoped result state, using safe generated test data.
- **Commit:** `test(search): [M1-C] protect scoped and unscoped result behavior`.

**Done criteria:** five scopes filter correct results, keyboard interaction works, `All` remains behaviorally identical, existing plugin prefixes remain usable, no per-keystroke heavy rescans or new Clippy warnings.

**Non-goals:** new indexer, searching arbitrary file contents in the main search, provider-wide rewrite, persistent user-defined scopes, radial filtering, search ranking overhaul.

---

## M2 — Dynamic Snippet Variables and Snippet Categories / Tags

**Feature contracts:**

- A `cs` snippet can include automatic system values **`{{sys.date}}`, `{{sys.time}}`, `{{sys.clipboard}}`, `{{sys.window_title}}`**, alongside normal prompted placeholders. Automatic variables resolve at **execution** using an invocation-context snapshot; date/time allow explicit formatting. Existing `{{date}}` or any other historical placeholder **must stay a prompted field**.
- Each snippet may carry **zero or more tags**, searchable and filterable in the existing snippet editor and launcher search. Old snippets with no tags continue to load and run unchanged.

**Owner/dependencies:** `src/plugins/snippet_template.rs`, `src/plugins/snippets.rs`, `src/gui/snippet_dialog.rs`, `src/gui/snippet_prompt_dialog.rs`, `src/commands/handlers/snippet_run.rs`; existing persistence and watcher. Depends on M0; M3 consumes this work.

### M2-A — Parse and model explicit automatic variables

- Inspect the placeholder identifier grammar: the existing parser currently accepts ordinary field names but does not automatically interpret namespaced system fields. Extend grammar **only** as needed for supported `sys.*` keys, with explicit typed `PromptField` versus `SystemVariable` segments. Unrecognized `sys.*` forms must produce helpful validation instead of silently becoming user prompts or crashing.
- Preserve escaping and malformed-placeholder behavior, UTF-8 byte ranges, placeholder ordering, repeated-field handling and existing prompt reconciliation tests.
- Define a small `SnippetInvocationContext` (timestamp/time zone, clipboard text availability and foreground HWND/title) captured once at invocation before launcher focus changes. Avoid reading the launcher’s own title for `sys.window_title`.
- Define formatting for date/time using existing chrono facilities when practical: user-provided safe format strings, explicit invalid-format messages, documented defaults. No generic expression language or arbitrary evaluation.
- **Commit:** `feat(snippets): [M2-A] add namespaced dynamic variables`.

### M2-B — Rendering and prompt integration

- Resolve system variables when a plain snippet executes and when a prompted snippet is submitted; prefill/output preview should show effective values without retaining secret values in persisted metadata.
- Mixed templates must prompt **only** for user-input fields; a system-only template must execute without showing an empty prompt. Missing clipboard/foreground data receives a clear error or documented empty-value fallback; do not hang.
- No accidental double evaluation between preview/commit or after focus transitions; a single resolved payload is used by subsequent Paste Sequence preflight.
- Test old `{{date}}`, escaped braces, mixed prompts/system tokens, unavailable clipboard, repeated token, time formatting and editor preview.
- **Commit:** `feat(snippets): [M2-B] resolve variables in snippet execution`.

### M2-C — Tag model, persistence and search

- Extend `SnippetEntry` additively with e.g. `#[serde(default, skip_serializing_if = "Vec::is_empty")]` tags. Normalize whitespace, enforce modest length/count bounds, deduplicate case-insensitively while retaining chosen display capitalization, and preserve the stable `alias` identity.
- Make tag edits use existing atomic snippet update/snapshot/watch flow. No separate competing in-memory snippet store.
- Include tags in snippet query matching and provide explicit tag-filter behavior that still finds untagged items when no filter is active. Do not alter the `snippet:run:` identity or copy/prompt semantics.
- Test old `snippets.json` migration, round-trip, duplicate tags, hidden contents and watcher reload.
- **Commit:** `feat(snippets): [M2-C] persist searchable snippet tags`.

### M2-D — Editor, validation and interoperability

- Enhance the existing `cs` dialog with tag entry/selection and a multi-tag filter, avoiding a separate plugin editor. Expose automatic-variable help and preview in the existing prompt/editor surface; never display hidden snippet text in a passive list.
- Ensure tags and dynamic tokens work with both saved and edited snippets, including draft validation, auto-save semantics already present and empty-error states.
- Autonomously open the dialog with fixture snippets, capture a safe screenshot of tags and dynamic preview, and validate reopening/persistence.
- **Commit:** `feat(snippets): [M2-D] expose variables and tags in existing UI`.

**Done criteria:** old snippets/presets unaffected, `sys.*` resolves correctly, ordinary prompts unchanged, tags add/filter/search and survive reload, hidden content stays concealed, M3 has a reusable typed resolver.

**Non-goals:** user-authored code/eval, network variables, auto-typing expansion, multiple unrelated snippet libraries, rewriting `cs` storage, AI template generation.

---

## M3 — Paste Sequence

**Feature contract:** A user builds an ordered sequence of **saved `cs` snippets (including prompted fields)**, **clipboard-history entries**, and **one-off typed text**; can save/reuse it or run it only once; chooses **automatic** or **manual-advance** playback; can set delays, launch through `paste` or an assigned hotkey, preview the ordered content and cancel safely. Prompted fields must be resolved **before the first paste**.

**Owner/dependencies:** new testable sequence domain and small UI/plugin/command integration; reuse `src/plugins/snippets.rs`, snippet prompt resolution, clipboard history backend, existing hotkey registry and foreground-window activation. Depends on M2.

### M3-A — Sequence domain, formats and storage

- Create typed `Sequence`, `Step` variants (`SnippetRef`, `ClipboardSnapshotOrRef`, `LiteralText`), delay, playback mode, stable identifier, user label and optional hotkey. Define deterministic position/order, empty-sequence validation, duplicate naming and invalid reference behavior.
- Saved sequences go into a documented separate versioned JSON store using existing atomic persistence patterns; **unsaved one-off text/steps remain in memory**. The sequence stores references to saved snippets by stable existing alias, not a duplicate snippet document; resolve at invocation, detect missing/ambiguous aliases.
- For clipboard-history steps, define snapshot-versus-reference semantics explicitly. Recommended: **freeze the selected payload for the active run** so changes to rolling history cannot mutate a sequence mid-run. Avoid unintended persistence of sensitive clipboard bodies in saved sequences; if persistence of a chosen payload is necessary, make that explicit and label sensitive data handling in UI.
- Test save/reload/migration/errors and no-disk-write one-off sequences.
- **Commit:** `feat(paste): [M3-A] add typed sequence model and persistence`.

### M3-B — Preflight and snippet/clipboard resolution

- Before any paste, inspect the chosen saved sequence and gather required prompted values using the **existing** `cs` prompt semantics (do not fork the parser). Present a coherent prompt flow across steps and identify which step needs a value.
- Resolve `sys.*` using a captured **origin-target invocation context**, including true originating window title and clipboard snapshot captured before sequence preparation changes focus; pre-render all text steps before execution and show ordered preview, item count, delay and target application.
- Fail preflight atomically (paste **zero** steps) when a referenced snippet is missing, a required field is missing, a history item has vanished or the target window cannot be identified. Canceling prompts never results in partial paste.
- Do not expose `hide_contents` snippet plaintext in ordinary passive previews; explicit execution remains functional.
- **Commit:** `feat(paste): [M3-B] preflight and resolve sequence steps`.

### M3-C — Safe, cancellable playback

- Implement a nonblocking playback state machine (`Idle`/`Preparing`/`Ready`/`Running`/`AwaitAdvance`/`Cancelled`/`Completed`/`Failed`) and explicit Start/Next/Cancel operations. Avoid synchronous sleeps, spin loops or background threads that directly mutate egui state.
- Capture/restore the **intended foreground target** (the window active before launcher/sequence editor took focus), then verify identity before every paste. Never silently paste into Multi Launcher, a password field if reliably detectable, or a different newly focused application. On ambiguous focus, pause/fail clearly rather than guessing.
- Perform paste using the existing clipboard/Windows input mechanisms, preserving other hotkeys and mouse hooks. Use clipboard ownership/change checks: restore original clipboard only if Multi Launcher still owns the value and no external actor has replaced it; do not clobber a concurrent user's copy.
- Automatic mode advances after configured delay; manual mode pastes one prepared item per explicit Next action/shortcut. Cancellation stops pending future steps without retracting those already pasted. Reject concurrent sequence execution with clear feedback.
- Test state-machine order, cancellation, focus loss, missing target, clipboard ownership and no-paste-on-invalid-preflight through fakes; one harmless owned text-input target for native UI smoke where possible.
- **Commit:** `feat(paste): [M3-C] implement guarded sequence playback`.

### M3-D — Sequence editor, command and hotkey UX

- Provide a resizable keyboard-first `paste` dialog: saved sequence list, ordered editable step list, Add Snippet (`cs` picker), Add Clipboard Item, Add Temporary Text, Move Up/Down, Remove, delay, mode, preview, Save/Run Once/Run/Cancel/Next. Reuse existing visual theme rather than creating a separate styling system.
- A one-time sequence can be composed and run without ever entering the saved sequence store. A saved sequence can be invoked from launcher search and bound to an optional hotkey through established registration/conflict handling.
- Distinguish prompt/preflight, target window and playback errors; indicate current step number and completion/cancellation status; place stop/Next controls where keyboard users can reach them.
- Add in-app help and README examples.
- **Commit:** `feat(paste): [M3-D] add sequence editor and launcher commands`.

### M3-E — Integration and screenshot evidence

- Cover mixed `cs` prompted/system snippets + one-off text + clipboard step, both execution modes, saved sequence reloading, hotkey conflict rejection, abandoned prompt, focus change during playback and failed clipboard writes.
- Native: using **only a disposable destination/editor**, launch Multi Launcher, create a short mixed sequence, run it once, confirm expected literal contents, cancel another run mid-sequence, capture editor and progress screenshot(s).
- **Commit:** `test(paste): [M3-E] verify mixed steps and cancellation`.

**Done criteria:** no paste before prompts finish, correct order/delays, saved and one-off flows, safe target handling, no clipboard clobber, running state visible, cancellation works and input remains usable.

**Non-goals:** mouse-coordinate automation, scripting loops/conditions, cross-app text expansion, sending keystrokes into arbitrary protected controls, MKMacro rewrite, background clipboard exfiltration.

---

## M4 — Hash Generator

**Feature contract:** `hash` opens a responsive tool for **text, clipboard or selected local file**. Supports **SHA-256, SHA-512, SHA-1 (legacy)**; provides copyable digest, optional expected-hash comparison and **nonblocking progress/cancellation** for large files. **Do not add MD5**.

**Owner/dependencies:** new hash domain/worker + plugin/dialog routed via existing architecture. `sha2` is already in dependencies; confirm suitable SHA-1 crate only if needed. Depends on M0.

### M4-A — Pure hash primitives and validation

- Define typed algorithms and input modes, UTF-8 text hashing semantics, digest formatting (lowercase hex as default) and comparison normalization (accept whitespace/case differences only where unambiguous).
- Implement streaming file reads with bounded buffers, an independent cancellation token and progress snapshots; files should not be read fully into RAM.
- Treat SHA-1 as legacy checksum compatibility and display a concise security disclaimer; do not imply it is collision-resistant for modern security decisions.
- Test published known-answer vectors, empty input, non-ASCII UTF-8, large fixture files, invalid expected digests and cancellation.
- **Commit:** `feat(hash): [M4-A] add streaming hash and compare engine`.

### M4-B — GUI and file integration

- Add `hash` plugin/command with a resizable dialog, text editor, clipboard-load button, file picker, algorithm selector, progress/cancel state, copy digest and expected-hash Match/Mismatch result.
- Accept file invocation from appropriate existing file actions if a clean command integration is available; do not restructure `fs` merely to add a context action.
- Worker completion/errors must not freeze or corrupt an edited input session; clear stale digest when source/algorithm changes.
- **Commit:** `feat(hash): [M4-B] add hash generator dialog and actions`.

### M4-C — Focused tests and visual proof

- Test reruns after edits, missing/locked file, optional comparison, stop/start, and no accidental file writes.
- Capture a harmless UI screenshot showing a calculated hash and Match result when desktop automation is available.
- **Commit:** `test(hash): [M4-C] cover UI lifecycle and file errors`.

**Done criteria:** all three algorithms operate, digest copy and compare work, progress/cancel work, GUI remains responsive, filenames/text never logged unnecessarily.

**Non-goals:** digital signature validation, HMAC/key management, password hashing, whole-directory hashing, MD5, file editing.

---
## M5 — JSON Tree Explorer and JSON Key/Value Search

**Feature contract:** The existing JSON dialog gains a **read-only collapsible document tree** alongside its current editable JSON text; users can search **keys, values or both**, highlight matches, navigate Next/Previous and copy a selected value, subtree or path. Invalid JSON produces the existing meaningful parse error, not a stale tree. No inline tree editing.

**Owner/dependencies:** `src/json_transform.rs` (reuse authoritative parser), `src/gui/json_utility_dialog.rs`, existing `json` command. Depends on M0.

### M5-A — Typed tree projection with stable node identity

- Inspect `JsonDocument` parsing and value-order semantics, including large arrays, nested objects, duplicate-key behavior, source text and current transform errors. Reuse it; do not parse independently with inconsistent semantics.
- Define a read-only tree projection/node path representation (object keys, array indices, scalar leaf kinds) and copy formatting; choose a stable path notation, e.g. JSON Pointer with correct `~` and `/` escaping, and label it clearly in the UI.
- Build/projection refresh only on document changes, not every frame; use virtualized/limited visible rows and stable IDs to handle large documents.
- Test nested objects/arrays, root scalars, empty containers, Unicode/escaped property names and path-to-value resolution.
- **Commit:** `feat(json): [M5-A] add structured read-only JSON tree model`.

### M5-B — Collapsible tree UI

- Add a Tree tab or split view to existing JSON Utility, keeping original Text/Format/Minify flows intact. Each row shows key/index, value type and truncated preview; selected node details show complete value when appropriate.
- Provide expand/collapse node, expand/collapse branch, copy value, copy JSON subtree and copy full path. Preserve selection across harmless view toggles where possible; reset when edited source invalidates path.
- Maintain reasonable scroll/resize behavior on minimum window size; cap expensive display of very long scalar strings.
- **Commit:** `feat(json): [M5-B] add collapsible tree navigation`.

### M5-C — Key/value search and navigation

- Add search field with **Keys / Values / Both** selection, match count, highlighted matches and Previous/Next controls; searching values should handle primitive values and optionally text representations of scalars, with documented behavior for containers.
- Navigate matches in deterministic document order; auto-expand parents and reveal selected node in scroll area. Query changes invalidate stale match state; empty query shows no match highlighting, malformed JSON shows an error rather than leftover matches.
- Keep search computation derived from parsed snapshot, with sensible debouncing or bounded processing for large documents.
- **Commit:** `feat(json): [M5-C] add key and value matching`.

### M5-D — Compatibility and native dialog smoke

- Test original formatter/minifier and load-from-clipboard commands, subtree/path copying, type-sensitive search, malformed-JSON state and node navigation after source edits.
- Autonomously open a fixture JSON document and capture a screenshot with an expanded tree and highlighted result if supported.
- **Commit:** `test(json): [M5-D] protect tree, search and transform flows`.

**Done criteria:** both selected JSON enhancements work together inside existing dialog; transforms retain original semantics; invalid documents do not show a misleading tree; responsive browsing of representative large input.

**Non-goals:** JSON Schema validation, diffing, mutation/editing through tree, cross-file JSON indexing, alternate JSON dialects.

---

## M6 — Regex Test Cases

**Feature contract:** Extend existing Regex Tester with named/unnamed case rows containing test input and **Expected Match / Expected No Match**, using current Rust-regex pattern and flags. By default, Match means **any substring match**, not full-string match. Show per-case Pass/Fail/Error and a summary; save cases with existing presets and support temporary unsaved cases.

**Owner/dependencies:** `src/regex_tester/{engine,model,session,persistence/presets}.rs`, `src/gui/regex_tester_dialog.rs` and submodules. Depends on M0.

### M6-A — Domain evaluator and case models

- Define per-case identity, optional label, sample input and expected boolean; evaluate using exactly the same compilation/flags path as the existing Regex Tester rather than a separate engine or regex dialect.
- Produce a typed per-case state (Pass/Fail/PatternError) and total summary. Distinguish invalid pattern from a valid no-match result. Reject irrelevant stale evaluations when pattern/case changes.
- Make case ordering deterministic and handle blank text, Unicode, multiline input, anchors and flags consistent with existing engine.
- **Commit:** `feat(regex): [M6-A] add multi-case regex evaluation`.

### M6-B — Persist cases backward-compatibly with presets

- Extend `RegexPreset`, `PresetInput` and `PresetContent` in `src/regex_tester/persistence/presets.rs` and directly affected callers/tests. Legacy documents **must load with empty test cases**, retaining stable IDs, pattern/flags and saved sample/replacement.
- Existing preset storage has explicit versioning and `deny_unknown_fields`; choose a documented safe migration/version strategy. Do **not** silently reset corrupted/unsupported preset files or rewrite valid old data merely on read.
- Save, update, rename, delete and reload must preserve associated cases. Temporary unsaved case edits must not unexpectedly write storage.
- **Commit:** `feat(regex): [M6-B] persist test cases with regex presets`.

### M6-C — Case editor and results panel

- Add a collapsible Test Cases section to the current Regex Tester layout; Add/Remove/Duplicate/Reorder, expected-status toggle, multiline input editor, clear status/summary and failure inspection.
- Case edits should not reset the main example, highlighting or substitution results. Keep resizing, scrolling and keyboard interaction coherent with current dialog.
- Show useful empty state and pattern-error state. Provide a copyable summary if trivially compatible with current UI without adding a new reporting subsystem.
- **Commit:** `feat(regex): [M6-C] add test case editor and pass/fail display`.

### M6-D — Save/reopen and UI evidence

- Test cases load/save when switching presets, unsaved case persistence boundary, invalid regex reporting and mixed expected outcomes. Include a migration fixture for pre-feature preset JSON.
- Native screenshot: Regex Tester with at least one passing and one failing case, built from generated non-sensitive text.
- **Commit:** `test(regex): [M6-D] verify test cases and legacy presets`.

**Done criteria:** cases run consistently with current regex engine, expected match semantics clear, rich UI and persistence complete, no regression to existing Regex Tester.

**Non-goals:** arbitrary PCRE features, backreferences/lookbehind not supported by Rust regex, exact/full-string expectation mode, fuzz-testing framework, performance overhaul.

---

## M7 — XML Toolkit

**Feature contract:** `xml` opens a **resizable XML Toolkit** supporting pasted/file-loaded XML; well-formedness validation with diagnostic location, pretty-print/minify, local XSD validation, XPath 1.0 queries, collapsible document tree, attributes/text/fragment copy, selection navigation and copy/save of result. Work offline by default; external entity/network resolution disabled by default.

**Owner/dependencies:** new dedicated XML domain/worker and `src/gui/` dialog, plugin/command registration. No coupling to JSON parsing internals beyond consistent UI conventions. Depends on M0. **This is a deliberately bounded substantial milestone**; complete all selected capabilities, not only syntax highlighting.

### M7-A — Pick a single secure XML backend and prove the boundary

- Review one or two realistic options **briefly** against Windows build support, XPath 1.0, XSD, security controls, diagnostic line/column and UTF-8 input. On Windows, a tightly scoped MSXML 6 COM boundary may satisfy XPath/XSD; a well-maintained Rust parser+XSD/XPath combination is acceptable if it genuinely covers all requirements. **Do not adopt competing parallel parsers without a concrete need.**
- Create a tiny automated proof fixture exercising an XML parse, a local XSD constraint, and one XPath selection. Treat this as a **single architectural decision gate**; limit to a focused investigation and one corrected attempt, not days of dependency churn.
- Encapsulate XML engine in a typed interface with parse/format/schema/query errors and document/node identities; define threading/apartment ownership if COM-based, so all operations remain on correct workers without blocking egui.
- Set secure parsing defaults: DTD/external entities disabled, external schema/network retrieval disabled, bounded input, no automatic UNC/HTTP fetch or arbitrary external entity expansion. Only explicitly selected local XSD and necessary explicitly allowed local imports, within stated boundaries.
- If an engine cannot safely support selected XSD/XPath features in current environment, record **M7 as blocked** with one concrete failure; do not claim that syntax-only XML fulfills M7 or keep cycling libraries. Continue independent milestones.
- **Commit:** `feat(xml): [M7-A] establish secure XML engine boundary`.

### M7-B — Well-formedness and formatting

- Load/paste XML, parse once per meaningful source change, show useful line/column/description on malformed source when supported, and prevent stale success/error UI.
- Pretty-print with consistent indent and minify **without corrupting semantically significant text/mixed content**; document policy for XML declaration, namespaces, comments, CDATA, processing instructions and whitespace. Avoid claiming round-trip identity if library normalizes lexical representation.
- Do not overwrite source on Format/Minify until an explicit user action; show transformed output/copy and optional Apply to editor.
- Test empty file, malformed tag, entities, namespace prefixes, mixed content, comments and Unicode.
- **Commit:** `feat(xml): [M7-B] add syntax diagnostics and formatting`.

### M7-C — Structured XML tree and node inspection

- Build a read-only collapsible representation preserving element hierarchy, attributes, text and namespace display. Stable document/node selection must not silently point to a wrong node after source edits.
- Show selected node details with copy text, attribute, selected XML fragment and, when available, source line/selection jump. Give intuitive tree and source coordinate behavior in a resizable dialog.
- Avoid eagerly materializing every rendered row each frame for large trees; cap preview output and provide clear parse-empty/error states.
- Test nested namespaces, attributes with escaped values, empty elements, multiple siblings and selection invalidation.
- **Commit:** `feat(xml): [M7-C] add XML tree and node inspection`.

### M7-D — XPath 1.0 evaluator and highlighting

- Run user-entered XPath 1.0 over currently parsed document; show query syntax errors distinctly from zero matches. Match nodes/attributes where supported; display result count and selected value(s).
- Allow results list, Next/Previous navigation, and highlight/select the corresponding element/attribute in tree and/or source where reliable. If backend returns scalar, boolean or numeric XPath results, display those correctly, not only node sets.
- Handle namespace prefixes in queries through explicit in-document namespace context or a small prefix mapping UI as supported by the chosen engine; document behavior.
- Test basic paths, predicates, attributes, namespaces, zero matches and malformed expressions.
- **Commit:** `feat(xml): [M7-D] add XPath query and navigation`.

### M7-E — Local XSD validation

- Add optional local XSD selector/path display and Validate against Schema action. Distinguish XML syntax errors, XSD-loading errors, schema-validation failures and successful validation.
- Allow explicit local XSD dependencies under a limited/visible local resolution policy; reject uncontrolled external resources by default. If user XSD import cannot be satisfied offline, show a diagnostic, do not fetch it silently.
- Provide best available message and line/column for schema violations, with honest fallback if backend does not report a particular location.
- Test valid XML/XSD, required/missing element, wrong type, namespace mismatch, missing XSD, forbidden DTD/external fetch.
- **Commit:** `feat(xml): [M7-E] validate XML against local XSD`.

### M7-F — Combined UI, files, tests and native evidence

- Integrate split-pane or tabbed XML Source / Tree / Validation / XPath within resizable egui viewport. Add Open XML, choose XSD, copy selected value/attribute/fragment, copy formatted document, and Save As with explicit destination; no hidden autosave or overwrite.
- Keep document lifecycle coherent: opening new file invalidates old query/schema diagnostics as needed, edits invalidate stale query results, tabs preserve relevant selection, UI remains responsive and close/reopen is safe.
- Add `xml` command/help, targeted integrated fixtures, and autonomous screenshots of valid tree + XPath matches and an XSD failure; avoid reading personal XML documents.
- **Commit:** `feat(xml): [M7-F] complete XML toolkit UI and workflows`.

**Done criteria:** XML parse, format, tree, XPath, XSD and file operations all work together with explicit error states and secure local-first resource resolution; Clippy clean and representative native screenshot(s) if supported.

**Non-goals:** XML authoring IDE, XSLT, XML digital signatures, automatic remote schema downloads, HTML parsing, unlimited-size streaming XML editor, general WYSIWYG forms.

---
## M8 — Batch File Renamer

**Feature contract:** `rename` opens a **resizable, `fs`-style dialog** with the **exact target directory prominently visible**. User may filter candidates, optionally include subdirectories, configure prefix/suffix/sequential numbering/literal replacement/regex substitution/case conversion, compare Original → Proposed names, deselect rows, preview warnings, explicitly Apply, and Undo a batch when safe. **Extensions are protected by default.**

**Owner/dependencies:** new rename domain and transaction/undo module, `src/gui/` dialog; may reuse `src/file_search/path_identity.rs` and `src/gui/file_search_dialog.rs` UI patterns but **do not merge rename logic into the existing `fs` search engine**. Depends on M0, after M7 in execution order.

### M8-A — Rename request model and preview-only transformer

- Model explicit directory root, recursive toggle (**off by default**), inclusion/exclusion/selection of files, ordered list of naming operations and extension policy (**preserve by default**). Use immutable input snapshot with stable file identities and deterministic sort for numbering.
- Implement pure proposed-filename generation: prefix, suffix, sequence start/step/padding, literal find/replace, regex capture substitution and case conversion. Clearly distinguish stem versus extension and handle Unicode file names without lossy conversions.
- Validate empty/unchanged target, Windows forbidden/reserved names, case-only names, duplicate generated names and path-length limits. Do not write to disk during preview.
- Test numbered examples, extension protected/explicit editing, rename pattern composition, regex substitutions, Unicode, unchanged names and invalid rules.
- **Commit:** `feat(rename): [M8-A] model renames and compute dry-run preview`.

### M8-B — Root-bound scan and result selection

- Build a constrained enumerator over the **explicit selected root**. Display canonical/absolute path in a persistent header and require the user to intentionally choose it; no ambiguous “current folder” inference from focus.
- Optional recursion must be clearly indicated, bounded by the selected root, and avoid following reparse points/symlinks outside it. Show file count and selected count; allow filtering by visible name/extension and selecting/deselecting files. Preview should identify files that cannot be renamed.
- Preserve deterministic preview order independently of UI sorting so numbered names do not spontaneously change while a user scrolls or filters.
- Test nested folder scope, exclusions, symlink/reparse point exclusion, read-only/missing root and files changed after scan.
- **Commit:** `feat(rename): [M8-B] add scoped candidate enumeration`.

### M8-C — `fs`-style dialog and preview UX

- Add a resizable window with root picker/path banner, pattern controls, a results table (`Original filename`, `Proposed filename`, status), filtering, selection controls, warning summary, Preview, Apply, Undo and Close.
- **Apply is disabled** unless there are approved changes and no unhandled conflicts. Clear mode indication and confirmation summarizing root and selected files before the first write.
- Invalid operations show actionable per-row reasons; preview must not immediately rename files merely because settings changed. Keep large result tables virtualized/scrollable as `fs` does.
- **Commit:** `feat(rename): [M8-C] add batch rename preview interface`.

### M8-D — Safe commit, collisions, partial failure and undo

- Before Apply, revalidate source identities and destination occupancy; do not overwrite unrelated files. Case-only renames and cycles (`a` → `b`, `b` → `a`) should use a **documented collision-safe staged strategy** within the same filesystem when possible; cleanup leftover staging on error.
- Execute one planned transaction/batch with progress and between-file cancellation. On partial failure give a precise success/failure list, with a recoverable manifest for the successful changes; do not falsely report all-or-nothing success.
- Implement Undo using recorded original/after paths and identities, with conflict checking and no clobber of files modified after rename. Persist an undo manifest through a restart only if the design can do so safely; at minimum enable a safe session-local Undo consistent with the user's requirement and document its lifetime.
- Focused tests: collisions with an **unselected** destination, case-only rename, swap/cycle, mid-batch failure, cancel, destination changed before Apply and Undo refuses changed destination.
- **Commit:** `feat(rename): [M8-D] execute renames and safe undo`.

### M8-E — Integration, fixture-run and evidence

- Wire searchable `rename` command and help, preserve existing `fs` UI/behavior. Add a fake-root/temporary-fixture end-to-end test that previews a multi-file rename, applies it, compares actual filenames, undoes it, then verifies original names restored.
- Launch owned app, point dialog to **only a disposable generated directory**, capture initial root/preview/confirmation screenshot(s), run a harmless small batch and inspect the resulting fixture directory.
- **Commit:** `test(rename): [M8-E] validate preview, apply and undo`.

**Done criteria:** explicit directory, predictable controls, correct preview, protected extension, safe collisions and stale-file checks, working Undo, no destructive unconfirmed operations.

**Non-goals:** batch file content editing, mass moving files to new directories, auto-watching, arbitrary scripting expressions, changing default Windows Explorer associations, rewriting `fs`.

---

## M9 — Search-and-Replace with Preview

**Feature contract:** `replace` opens a **resizable, explicit-directory** dialog. Search **literal or Rust regex** within supported text files (recursive option, extension/exclusion filters); inspect per-file/per-match changes using before/after diff; check/uncheck files **and individual matches**; confirm changes; detect stale sources and skip them; produce a recoverable per-file backup/undo trail. Search/preview never mutate files.

**Owner/dependencies:** dedicated multi-file text transform and preview worker; may share selected path-identity and safe atomic-write helpers from M8, and present diff using relevant existing `src/diff` components without refactoring Diff plugin. Depends on M8 concepts, **not** its UI state.

### M9-A — Search request model and bounded discovery

- Define typed root(s) chosen explicitly, recursion, include extensions/globs, exclude paths/globs, content pattern, literal/regex mode, case-sensitivity and a cancelable search generation.
- Reuse `src/file_search/` only where its coordinator or discovery API *actually* fits; do not add mutation side effects to the normal `fs` action. The replace UI must never act on an implicit Global `fs` root.
- Skip/flag binary, unreadable, too-large and unsupported encoding files. **Minimum:** UTF-8 and UTF-8 with BOM, with untouched original line endings and byte sequences preserved outside replaced spans. Clearly report skipped files.
- Collect match byte ranges and lines; prevent unbounded repeated zero-length-match regex replacement or pathological preview explosion. Use worker and cancel for large searches.
- Test root scoping, extension filters, symlink/reparse skip, literal vs regex, Unicode, BOM, CRLF, zero-width handling, unreadable/binary files and cancellation.
- **Commit:** `feat(replace): [M9-A] add scoped text search and match model`.

### M9-B — Replacement computation and stable per-match selection

- Represent a **proposed replacement** independently of original scan match data. Support literal replacement and Rust-regex substitution with capture references; display clearly which mode is active and show invalid substitution diagnostics.
- Produce a deterministic preview of each file with original and transformed text/diff. Each match has a stable identity within the captured file snapshot; checking/unchecking one match recomputes exactly the output for the approved subset.
- Handle overlapping/adjacent match policy explicitly; never apply stale offsets to a modified text buffer. Preserve unchanged bytes/line endings; avoid silently converting UTF-8 BOM or CRLF.
- Recomputed summaries show affected file count, approved matches and estimated bytes changed. No write occurs in this phase.
- **Commit:** `feat(replace): [M9-B] compute selective replacement previews`.

### M9-C — Safe write and recovery

- At Apply, re-open files and re-check original content identity/fingerprint and relevant metadata; **skip** changed files with a reason, never silently recompute and apply an unreviewed proposal.
- Stage edited bytes safely in the same target filesystem, preserve relevant file properties/permissions where supported, and use a Windows-correct replacement strategy. Produce per-file recovery copy/manifest before changing each source; check backup-write errors rather than continuing destructively.
- Support cancel between files, mixed success/failure reporting and Undo/Restore of only files still matching the expected post-write identity. A failed file must not be presented as committed; preserve its original data on write failure where possible.
- Treat source/destination reparse points, insufficient permissions, read-only files, locks and concurrent edits as explicit errors. Never automatically delete backups while recovery might be needed; document retention controls.
- Focused fixture tests: unchanged source after preview, stale-file skip, subset selections, simulated mid-batch failure, BOM/line endings, rollback/restore and conflict refusal when target changed after Apply.
- **Commit:** `feat(replace): [M9-C] add verified write and restore operations`.

### M9-D — Rich preview UI and confirmation

- Add root banner, file filters, pattern/replacement editors, literal/regex switch, results list, per-file expansion and before/after diffs. Select/unselect at file and match granularity, Preview/Refresh, Apply and Undo/Restore.
- Clearly show old/new text, changed line indicators, selected count, stale file status, skipped file reasons, total pending changes and explicit confirmation of root/path list. Apply unavailable if preview is invalid/out of date.
- Ensure large result tables remain responsive and scrolling/selection stable. Use current theme/layout primitives, not a new application-wide UI framework.
- **Commit:** `feat(replace): [M9-D] add multi-file diff preview dialog`.

### M9-E — Integration and safe native validation

- Register `replace` command/help and verify `fs` normal search remains unchanged. Test a disposable project tree with several `.rs`/`.txt` files, one deselected match, one stale file and one binary file; compare actual disk bytes to approved preview.
- Capture a screenshot of explicit root, per-match selection and before/after diff. Execute only on the disposable fixture; test Restore and verify exact bytes.
- **Commit:** `test(replace): [M9-E] cover selective replacements and recovery`.

**Done criteria:** users can audit exactly what will change before Apply; only selected, unmodified fixture files are changed; backup/undo works, partial errors are truthful, no modifications happen during Preview.

**Non-goals:** AST refactoring, project-wide code renaming analysis, binary replacement, silently rewriting unsupported encodings, external source-control integration, watch-based live replacement.

---

## M10 — Hot Folder Automation

**Feature contract:** `hotfolder` opens a resizable rules manager. User chooses **explicit watched directory**, creates named rules that are **disabled until enabled**, chooses New File trigger by default (Modified optional), filters by filename/extension and associates **Move / Copy / Rename / explicitly selected existing Launcher Action / MkMacro**. Provides dry-run, pause/disable, success/failure history, protection from incomplete file writes, self-trigger loops and unattended destructive default behavior.

**Owner/dependencies:** dedicated persisted rules domain, `notify`-based watcher/service (already a dependency), application lifecycle boundary, typed command dispatcher bridge and MkMacro invocation bridge. May reuse M8 safe path/rename operations where appropriate; never reuse GUI view model as a service API. Depends on M8/M9 safe conventions but not necessarily their execution state.

### M10-A — Rule model, persistence and explicit action capability

- Define stable rule ID/name, exact watched root, enabled flag, event kind (New default, Modified optional), filename glob/extension filters, optional subfolder policy (**off by default**), action variant and destination/rule parameters. New rules start disabled and cannot perform side effects until user explicitly enables them.
- Save in a versioned, atomic store separate from `snippets.json` and unrelated settings. Validate invalid roots/action references, duplicate IDs and unsupported actions; preserve valid rules if a neighboring one is corrupt, using the existing repository's persistence-error conventions.
- For arbitrary existing launcher actions, use a **picker of registered supported actions** rather than arbitrary command string injection; persist stable identity and show the action that will run. MkMacro must resolve explicitly by stable current identity or report missing/stale reference.
- Decide and document restart behavior: **enabled rules are restored only after a user has deliberately enabled them**; do not retroactively process every file already present on restart unless a separate explicit manual scan is requested (outside current scope).
- **Commit:** `feat(hotfolder): [M10-A] add validated, disabled-by-default rules`.

### M10-B — Watch lifecycle, event classification and settling

- Reuse existing `notify` crate and watcher conventions; one managed watcher/service owns subscriptions, start/pause/disable and application shutdown. Avoid one thread/hook per rule or unrelated watcher rewrites.
- Distinguish created vs modified and rename events using supported platform event semantics; normalize paths against the watched root. Handle watcher overflow/errors as reportable states, not silent successes.
- Add **bounded debounce/file stability**: wait for file metadata/content to stop changing across a small configured observation window, then attempt an exclusive/open-read check where possible. Use bounded retries and report file-busy failures instead of waiting forever.
- Skip directories, temporary incomplete files where detectable (e.g. `.crdownload`, `.part`) unless rules specifically opt in, and symlinks/reparse paths leading outside the root.
- Test event batching, create/modify detection, pauses, enabling/disabling, app shutdown, incomplete file, inaccessible path and restart behavior using a temporary watched directory.
- **Commit:** `feat(hotfolder): [M10-B] add bounded watcher and file-settle service`.

### M10-C — Built-in file operations and loop suppression

- Execute only the selected matching rule's built-in Move/Copy/Rename actions with explicit destination preview and collision policy (**no overwrite by default**). Re-check source identity before action, respect permissions and report failures precisely.
- Track processed event/file identity and own destination writes to suppress repeats; recognize moves back into the watched tree and recurring patterns. Add a small bounded dedupe cache (not unbounded runtime history) and prevent simultaneous execution of the same rule/file pair.
- Provide a finite run limit/rate cap for an unexpectedly noisy watched directory, with visible throttled status. No automatic permanent file deletion in this milestone.
- Test copy/move/rename fixture, collision, source deletion during settle, duplicate events, nested watched folders and intentional self-trigger loop.
- **Commit:** `feat(hotfolder): [M10-C] run safe file actions without loops`.

### M10-D — Launcher/MkMacro action dispatch

- Bridge rule matches to established typed launcher command/MkMacro invocation at the correct executor boundary, not by scripting fake launcher keypresses or injecting a guessed serialized Action.
- Permit only actions that are **explicitly chosen and validated for unattended execution**; show a stronger confirmation on Enable for an action with potentially destructive/system side effects. Reject or disable actions requiring an interactive prompt/foreground context that cannot be supplied safely.
- Serialise or throttle invocations when necessary; avoid infinite recursion if a launched action creates files in the same watched root. Record action ID, outcome and sanitized error information, not sensitive file contents.
- Test valid known command, deleted/renamed action, unavailable macro, execution failure, recursion blocking and pause during pending action; no real dangerous commands in tests.
- **Commit:** `feat(hotfolder): [M10-D] connect validated launcher and macro actions`.

### M10-E — Rules editor, dry-run and history

- Provide resizable `hotfolder` dialog with rules list, Enabled/Paused state, watched root banner, trigger/filter form, action picker, optional destination, validation preview and Test Rule action.
- **Test Rule must not perform side effects:** show whether a chosen existing fixture path would match and what action would be selected. Display log/history with time, rule, file (respect privacy), action, success/failure and reason, plus Clear History.
- Rule edits while watcher active should publish a coherent new config without duplicate subscriptions, races or reprocessing existing inventory. UI empty states and errors must be actionable.
- **Commit:** `feat(hotfolder): [M10-E] add rule editor and execution history`.

### M10-F — Native fixture automation and lifecycle proof

- With an isolated watched directory, start owned Multi Launcher, add a disabled move rule, create a fixture file (verify no action), enable rule, create a second file (verify exactly one move), pause rule, create another (verify no action), restart the owned app if feasible (verify enabled-rule behavior). Never point tests to real Downloads, Documents or private folders.
- Capture screenshot(s) of a configured rule, watched path and execution history. Test shutdown, rapid event burst and no orphan worker/watch handles.
- **Commit:** `test(hotfolder): [M10-F] verify rules and watcher lifecycle`.

**Done criteria:** disabled rules truly inert, watcher handles stable, settle/dedupe effective, only selected actions run, no silent overwrite/destructive default, predictable restart, useful dry-run and history.

**Non-goals:** general-purpose workflow scripting language, system-service/background daemon that survives app exit, polling entire filesystem, auto-delete, automatic arbitrary code execution, mass backfill of preexisting files, integrating external cloud storage.

---
## M11 — Screenshot Pinboard

**Feature contract:** From an explicit **region selection**, create a floating reference image that can be moved, resized, zoomed, copied, saved, made not-topmost or made click-through. Support **multiple independent pins**, across monitors/DPI scales. Default **Always on Top on**, session-only pins (do not restore after restart). Preserve existing screenshot capture/editor and Screen Draw behavior.

**Owner/dependencies:** current screenshot capture/selection and egui viewport conventions, `src/gui/mod.rs` ownership and a narrow native-topmost/click-through boundary. Depends on M0; no forced reuse of Screenshot Editor's full editing model.

### M11-A — Capture model and independent pin lifecycle

- Reuse existing region selection/capture primitives; create an **in-memory image snapshot** for a new pin without forcing a disk save, external image editor or clipboard overwrite. If capture path currently goes through clipboard, isolate and restore prior clipboard when safe using ownership checks.
- Define independent pin IDs, captured pixel dimensions, display bounds, zoom, topmost and click-through states, plus close/dispose behavior. Use session-only storage and a modest configurable/resource-safe upper limit on simultaneous pins, with clear feedback at the limit.
- A cancelled region selection must create no pin and must not alter launcher visibility or capture/edit state unexpectedly. Never accidentally capture the pinboard overlay instead of intended region.
- Test pin creation, multiple IDs, cancellation, image data lifecycle and release of texture resources on close.
- **Commit:** `feat(pinboard): [M11-A] capture and manage floating screenshot pins`.

### M11-B — Floating window, zoom and image actions

- Render each pin in its own independent movable/resizable native egui viewport or existing supported layered-window equivalent. Do not reuse main grid window geometry or close/open hooks that reset its position.
- Show image at fit/actual size with scroll/zoom; retain aspect ratio unless explicit crop tools are later requested (out of scope). Offer Copy Image, Save As, Close, Always on Top and Click-Through controls in a small toolbar/context action.
- Saving asks for explicit path; copying writes an image to clipboard only when user clicks Copy. Pins continue displaying without depending on a backing temporary file.
- Ensure pin movement and resizing over multiple monitors account for negative virtual-desktop origins and DPI scaling; if hardware only has one monitor, cover coordinate conversion with fixture tests.
- **Commit:** `feat(pinboard): [M11-B] add movable pinned image windows`.

### M11-C — Native topmost and click-through behaviors

- Default pins to topmost but expose user toggle with immediate response and independent state per pin. Implement click-through **only on pin windows**, not on launcher grid; ensure there's a recoverable way to turn click-through off via launcher command/list even when direct mouse access is disabled.
- Keep window style manipulation inside a narrow Windows boundary with handle-validity checks; preserve DPI, focus and close semantics. Closing a pin must release window/texture and not affect other pins or overlays.
- Cover close while copying/saving, click-through toggle recovery, mixed topmost pins, multiple independent pin states and application shutdown.
- **Commit:** `feat(pinboard): [M11-C] add native topmost and click-through controls`.

### M11-D — Command integration and screenshot validation

- Add launcher command(s) such as `pinboard capture` and `pinboard list`, with discoverability and keyboard access. Ensure original screenshot commands still open the existing editor/capture flows exactly as before.
- Native fixture: capture a known, safe test window/region, create two pins, zoom/move one, toggle topmost/click-through, save/copy it, close one and verify the other remains.
- Capture screenshot(s) containing the owned fixture and pinned reference image(s), excluding unrelated desktop content. Where desktop capture is restricted, use an egui/natively renderable test fixture and record limitation.
- **Commit:** `test(pinboard): [M11-D] validate pin isolation and native lifecycle`.

**Done criteria:** several independent temporary floating images; correct zoom/topmost/click-through/copy/save; safe recovery from click-through; cancellation and shutdown cleanup; screenshot editor unaffected.

**Non-goals:** live-updating window crop, OCR editing, image annotation, persistence across restarts, desktop wallpaper management, cross-machine sync.

---

## M12 — Click Visualization

**Feature contract:** Existing `mouse` family gains user-configurable visual feedback for **left, right and middle click** using **Ripple / Circle / Flash** styles. Settings: color, size, opacity, duration; a `mouse`-prefixed toggle/on/off command and an Enabled control inside Mouse Settings. Appearance persists using current mouse preferences; **enablement is session-only**, consistent with related cursor effects. Overlay is noninteractive and must not steal input.

**Owner/dependencies:** `src/plugins/mouse.rs`, `src/gui/mouse_settings_dialog.rs`, `src/coordinate_tool/` rendering/settings/native integrations, current mouse gesture hook/event path. Depends on M0; no new unrelated keyboard visualization.

### M12-A — Settings, command and event ownership

- Extend existing coordinate/mouse settings with a small typed click-effect preference block and defaults; migrate old settings via serde defaults/established settings validation. Include duration bounds and normalized RGBA/size, plus style enumeration.
- Add `mouse click toggle|on|off` / `mouse click settings` or equivalent validated prefix commands and help. Ensure `mouse effects off` semantics remain coherent (include click visualization if currently defined as disabling all effects, and test that behavior).
- Reuse existing passive mouse event observation/gesture integration where possible; do not introduce a competing global low-level hook or block mouse clicks. Record only button-down edge/physical cursor position needed for effect.
- **Commit:** `feat(mouse): [M12-A] add click visualization state and commands`.

### M12-B — Nonintercepting native effect rendering and settings UI

- Render transient effects over left/right/middle presses with finite lifetime and no persistent repaint when idle. Effects should occur at correct global cursor coordinates, including secondary monitors and Windows DPI changes.
- Draw on a transparent, always-pass-through overlay with no keyboard focus; clicks must reach target applications and existing mouse gestures/MkMacro recording must remain unchanged. A burst of clicks should have bounded number of visual effects and memory use.
- Add Enable/Disable in Mouse Settings plus Ripple/Circle/Flash controls and color, size, opacity, duration, Apply/Cancel semantics aligned to the existing draft setting UI.
- Test render state for each button/style, expiration/time progression, invalid settings, coordinate conversion, multi-click throttling, and input pass-through under available native environment.
- **Commit:** `feat(mouse): [M12-B] render configurable click indicators`.

### M12-C — Interop regression and native visual evidence

- Test mouse gesture recognition and current halo/zoom/crosshair settings indirectly through their directly affected integration seams; no full mouse automation rewrite.
- Launch Multi Launcher, enable effect through `mouse`, click in a harmless owned fixture, capture one time-bounded screenshot/video frame if environment permits, confirm target received click, toggle off and confirm no new effects.
- On an automation platform unable to capture a short-lived ripple, assert visible effect render data deterministically, log native screenshot limitation and do **not** loop screenshot capture indefinitely.
- **Commit:** `test(mouse): [M12-C] protect existing input and effect behavior`.

**Done criteria:** click visuals configurable and discoverable, effects ephemeral and pass-through, no additional hotkey/hook conflicts, idle overhead negligible, other cursor/gesture features unaffected.

**Non-goals:** keyboard visualization, screen recorder, macro event injection, changing system cursor assets, complex shader/theme editor.

---

## M13 — Final integration, documentation and bounded cleanup

**Objective:** Consolidate **the selected 14 features** without adding new features or starting a repository-wide modernization. **Depends on:** all feature milestones that could be completed on this host. M13 cannot silently mark genuinely blocked requirements as complete.

### M13-A — Cross-feature regression checks

Execute **one focused integration pass** across the actual changed boundaries, selected from the following (do not re-run every earlier microtest as a giant matrix):

1. Launcher search scopes still allow `cs`, `json`, `regex`, `fs` and newly registered `hash`, `xml`, `rename`, `replace`, `paste`, `hotfolder` commands; existing launcher/radial hotkeys still route normally.
2. Prompted/dynamic/tagged snippets remain callable directly and from Paste Sequence; clipboard changes remain controlled; hidden snippet contents do not leak in passive UI.
3. JSON format/minify still works; Regex existing examples/substitution still work; new JSON tree/search and regex cases survive appropriate transitions.
4. Batch Renamer, Replace and Hot Folder only operate on **disposable** selected roots; preview/disabled state has no side effects, rechecking detects stale targets, undo/recovery refuses unsafe overwrite.
5. Pinboard and Click Visualization coexist with existing screenshot, cursor/gesture and application-close behavior; no orphan overlay/window/process remains after owned app exit.

Run targeted Nextest integration suites for shared changed logic. If changes materially affect a shared command dispatcher, add one scoped plugin-command routing suite; do not run unrelated full test targets for appearance of completeness.

### M13-B — Code cleanup, Clippy and final native screenshot collection

- Verify touched code is `cargo fmt` clean and **`cargo clippy --all-targets -- -D warnings`** passes with no warnings introduced by this effort. Resolve any *in-scope* lint and compiler errors. Do not start broad unrelated lint modernization.
- Inspect cumulative diff for temporary debug code, abandoned adapters, duplicated implementation, missing cancellations, accidental formatting churn, old code paths made obsolete by the new features and malformed data migrations.
- Capture (or confirm previously captured) a compact **feature UI gallery**: search scopes, snippet tags/variables, Paste Sequence, Hash, JSON explorer, Regex cases, XML validation/tree, rename preview, replace diff, hot folder rule history, pinboard, click visualization. **Reuse milestone screenshots rather than redoing native sessions unnecessarily.**
- If GUI screenshot capture is technically unavailable, link available test evidence and plainly state what was not observed; **do not ask the user to do it**.
- **Commit:** `chore(quality): [M13-B] complete scoped verification and cleanup` (only if actual files changed).

### M13-C — Documentation and final handoff

- Refresh README/in-app help for command discovery, current feature scope, safe file operations, XML local-only rules, paste focus safety, disabled-by-default Hot Folder rules and session-only pins.
- Close execution ledger with each checkpoint's actual status, focused tests, Clippy result, screenshot paths, commit hashes and known limitations.
- Provide a final concise report with: implemented features; noteworthy architectural decisions; exact verification commands and results; screenshot locations; commits; remaining genuine blocks/issues. **No “please run this” instructions and no intermediate approval request.**
- **Commit:** `docs(productivity): [M13-C] finalize feature documentation` (only when it adds a substantive change).

**Done criteria:** all unblocked features satisfy their acceptance criteria; final Clippy status truthfully reported; repository clean except intended files; no uncovered high-severity regression known; final user review is the **first** requested manual review.

---

## 6. Verification inventory — scoped, not an open-ended test campaign

This is a **menu of intended tests**, not a requirement to execute every row after every checkpoint. Milestone-specific tests run as code is written; M13 selects the few integration boundaries affected by multiple workstreams.

| Feature | Minimum convincing automated proof | Optional native GUI evidence |
|---|---|---|
| Search Scope Filters | `All` parity, category filtering, stale cache and explicit-prefix compatibility | Scope chips + scoped result screenshot |
| Dynamic Variables | Legacy `{{date}}` prompted, `{{sys.date}}` automatic, clipboard/window snapshot | Snippet edit/preview |
| Snippet Tags | Old JSON load, multi-tag save, query/filter and hidden-body preservation | Filtered `cs` dialog |
| Paste Sequence | Mixed steps/preflight, auto/manual ordering, focus loss, cancel, clipboard ownership | Editor + safe paste to owned text input |
| Hash | SHA vectors, file streaming, expected-hash comparison, cancel | Digest + Match screen |
| JSON Tree/Search | Parsed-node paths, expansion, key/value match navigation, existing formatting | JSON dialog tree + highlighted result |
| Regex Cases | Any-substring expectation, invalid pattern, preset v1 migration and save/reload | Cases with mixed pass/fail |
| XML | Well-formed/error, local XSD success/failure, XPath namespaces, secure DTD denial | XML tree/XPath + validation error |
| Batch Renamer | Preview no write, naming rules, collision, case-only swap, undo | Explicit root, before/after file table |
| Search/Replace | Per-match selection, byte fidelity, stale-file skip, recovery | Selected per-match diff preview |
| Hot Folder | Disabled inert, settle/dedupe, enable/pause, exactly-once fixture action | Rule form + execution history |
| Pinboard | Multi-pin lifetime, zoom/topmost/click-through recovery, cancel | Multiple pins over safe fixture |
| Click Visualization | Event-to-effect mapping, nonintercepting overlay and expiry | Effect around click if capture timing allows |

### Validation stop conditions

Stop a checkpoint's verification once the relevant invariants have evidence and the Clippy gate is satisfied. **No** mandatory screenshot pixel comparisons, benchmark parity mandates, mass repository test suites, repeated “just to be sure” reruns, unexplained waiting for hooks, speculative crash campaigns or manual user approval checkpoints.

If a checkpoint is genuinely blocked: identify the exact unsatisfied requirement and evidence, preserve useful work safely, mark it blocked, and proceed only with independent clean work. **Never erase a requirement from this plan just to report completion.**

---

## 7. End-of-project acceptance checklist

- [ ] M1: All five search scopes work; previous main/radial/prefix search paths are compatible.
- [ ] M2: Dynamic `sys.*` variables and multi-tag `cs` snippets work; legacy prompts and storage intact.
- [ ] M3: Paste Sequence supports saved snippets, prompted fields, clipboard history, one-offs, both playback modes, delay, hotkeys and cancel without unsafe focus/clipboard changes.
- [ ] M4: Hash Generator supports SHA-256/SHA-512/SHA-1, text/clipboard/file, copy/compare and cancel.
- [ ] M5: JSON tree and key/value search integrate with existing editor and copy paths.
- [ ] M6: Regex expected match/no-match cases persist with presets and report pass/fail.
- [ ] M7: XML Toolkit supports well-formedness/XSD, format/minify, XPath/tree, source/file/copy and secure parsing defaults.
- [ ] M8: Batch renaming has `fs`-style explicit-root UI, protected extensions, preview/conflict detection, apply and safe undo.
- [ ] M9: Multi-file replacement previews per-match diffs and applies only approved unchanged text with recovery.
- [ ] M10: Hot Folder rules start disabled, support declared conditions/actions, settle/dedupe, test mode, pause and execution history.
- [ ] M11: Session-only floating multi-pin screenshots can move, zoom, copy/save, topmost/click-through and close independently.
- [ ] M12: Left/right/middle click visualization supports three styles, settings, `mouse` command and input pass-through.
- [ ] Continuous checkpoint commits and honest Clippy reporting are documented.
- [ ] No manual testing/approval was requested of the user during the job.
- [ ] Native UI screenshots were captured when feasible, with limitations explicitly noted when not feasible.
- [ ] No destructive tests affected real user data or unrelated applications/processes.
- [ ] README/help updated and final implementation report includes checkpoint hashes, evidence and genuine remaining issues.

**Final instruction:** Once complete, stop implementing. Do not invent another optimization track, feature batch, redesign initiative, quality gate or manual acceptance phase. Return the finished result for the user's final review.
