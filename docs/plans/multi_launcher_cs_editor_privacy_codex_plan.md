# Multi Launcher — Plan A: Clipboard Snippets Editor & Privacy

**Status:** Approved requirements; ready for Codex implementation  
**Source-of-truth reference:** `multi_launcher(20261007-185532).zip` (October 7, 2026)  
**Scope:** Existing `cs` / Clipboard Snippets plugin, its editor, and existing snippet-body preview surfaces  
**Artifacts:** This plan and `multi_launcher_cs_editor_privacy_codex_start.md`  
**Implementation language/UI:** Rust, egui/eframe, existing Multi Launcher architecture

> **Mandatory orchestration directive:** **Use the project’s active checkpoint commit cadence and define the task-specific commit boundaries in the plan.**

## 1. Outcome and success definition

Improve the existing Clipboard Snippets (`cs`) experience without creating a new snippets plugin or modifying how snippets fundamentally execute.

At completion:

1. A snippet with a very long alias or text **cannot push Edit or Remove out of view** in the management list at ordinary supported editor sizes. The default list uses one-line, whitespace-normalized, truncated previews; the controls remain accessible without horizontal scrolling of the entire row.
2. Each snippet has a persisted **Hide contents** setting, set from its Add/Edit form, defaulting to `false` for pre-existing and newly created snippets.
3. A masked snippet is displayed as `<alias>: ******` (optionally plus a small hidden/eye-off indicator) in the editor list and every existing snippet-body preview in scope. Its plaintext is never exposed through that row's hover text, tooltip, or automatically expanded preview.
4. Editing a previously saved masked snippet **starts concealed**; an explicit **Reveal to Edit** action is required to display/edit the actual body. Closing, canceling, saving, switching entries, and reopening reset the reveal state. Alias and privacy-flag edits do not require revealing the body.
5. A hidden snippet still searches by **alias and body**, still copies its **exact original body**, and still participates in all existing `cs` commands. The user-entered search query remains visible; this is display masking, not encryption.
6. The editor gains **Clear Filter**, **N of M match count**, **inline Remove → Confirm/Cancel**, **exact duplicate-alias prevention for GUI create/rename**, and improved resizing/minimum usable width.
7. Existing `snippets.json` content continues loading unchanged and is never silently discarded, reset, or overwritten on malformed input or a failed save.
8. The existing Dashboard clipboard/snippet widget respects the per-snippet flag **without altering clipboard-history handling**. Other preview consumers are audited and changed only if they actually display snippet bodies.
9. No functional changes to unrelated plugins or unrelated launcher, hotkey, radial, note, or MkMacro functionality.

## 2. Approved decisions (fixed; do not re-ask)

| ID | Approved choice | Required behavior |
|---|---|---|
| R01 | Responsive row design | Keep Edit and Remove permanently visible; clip/truncate preview, rather than scrolling whole row horizontally. |
| R02 | Ordinary previews | Show alias plus single-line text preview; replace line breaks visually and truncate to available width. |
| R03 | Where to configure privacy | `Hide contents` checkbox inside Add/Edit form; small hidden-state indicator in row. No checkbox clutter on each row. |
| R04 | Mask appearance | `mySnippet: ******`; alias visible; optional eye-off icon. No masked-text tooltip/hover disclosure. |
| R05 | Editing masked snippets | Start hidden; explicit `Reveal to Edit`; re-conceal when editing session ends. |
| R06 | Defaults/persistence | Old/new entries default unmasked; choice saved per snippet; renames and text updates preserve its flag. |
| R07 | Searching | Alias **and body** still searchable, even if body is masked. Search must not print matching hidden text as a UI preview. |
| R08 | Masking coverage | All **existing CS snippet-body preview surfaces** in scope, including the Dashboard snippet rows. |
| R09 | Clipboard behavior | Clipboard writes and existing clipboard-history behavior remain **unchanged**; mask is presentation-only. |
| R10 | Approved small QoL | Clear Filter; matching count; reject exact duplicate GUI aliases; inline delete confirmation; resize-friendly layout. |
| R11 | Removal UX | Inline `Remove → Confirm / Cancel`, not a modal, for the Snippets **editor list**. |

**Explicitly not selected:** extra Copy button on rows, alphabetical sorting, Ctrl+S shortcut, row right-click menu, full-preview toggle, Undo Remove, row-level temporary reveal, protecting or modifying clipboard history, encrypting saved data, prompted fields, global hotstrings. Do not implement these under Plan A.

**Interpretation:** "Exact duplicate alias" means an alias equal to a **different entry** under the current storage semantics (currently case-sensitive equality). Do not silently make `cs` aliases case-insensitive, trim/change valid existing alias strings, normalize old records, or make duplicate detection a breaking change for command-based updates. Existing duplicate rows, if any, must remain loadable and manageable without automatic deletion.

## 3. Grounded repository inventory

These are verified points of entry in the source archive, not reasons to refactor unrelated code. Check the actual current checkout before edits.

| File / area | Observed responsibility | Why it matters |
|---|---|---|
| `src/plugins/snippets.rs` | `SnippetEntry { alias, text }`; Serde JSON load/save, `append_snippet`, `remove_snippet`, transactional `update_snippets`, atomic persistence, watcher/live snapshots, `SnippetsPlugin::search` | Owner of the new saved flag and copy/search compatibility. |
| `src/gui/snippet_dialog.rs` | `SnippetDialog`, list filter, Add/Edit form, list rows, `open`, `open_edit`, `commit_entries`, `ui` | Main editor, long-row layout bug, privacy form/reveal state, inline confirmation, duplicate checks. |
| `src/dashboard/widgets/clipboard_snippets.rs` | Dashboard combined clipboard history + snippet rows; snippet label and `.on_hover_text(&snippet.text)` | Existing plaintext preview and tooltip disclosure to fix for masked snippets. |
| `src/dashboard/data_cache.rs` | Dashboard cached `Vec<SnippetEntry>` snapshots | Existing flag should flow through same cache; avoid a second privacy store. |
| `src/gui/command_host.rs` / `src/commands/handlers/dialog_crop.rs` | Routes `snippet:dialog` and `snippet:edit:` into the same editor | Ensure direct `cs edit alias` starts concealed for a masked existing entry. |
| `src/gui/universal_action_catalog.rs` | Offers snippet actions with alias label and an underlying `clipboard:<text>` action ID | Action execution must remain intact. Review visible descriptions without redesigning the action transport. |
| `src/radial/dynamic.rs` and related GUI consumers | Consume snippet/action candidates through existing provider mechanisms | Audit displayed labels/tooltips only; not an invitation to rewrite the radial menu. |
| `src/dashboard/widgets/command_history.rs` | Resolves snippet clipboard actions back to alias labels | Preserve history behavior; audit whether any visible fallback leaks text. |
| `src/actions/snippets.rs`, `src/commands/parser.rs`, `src/commands/headless.rs` | Existing command-add/edit/remove and clipboard execution paths | Keep the command contract and command-based update behavior unchanged. |
| `tests/snippets_plugin.rs` | Save/load, search, `cs list`, `cs rm`, `cs add`, `cs edit`, multiline copy | Primary integration regression coverage. |
| `tests/plugin_exact_match.rs` | Includes snippet search-related fixture/behavior | Existing fixtures need adjustment when adding a struct field. |
| Unit tests in `src/plugins/snippets.rs`, `src/gui/snippet_dialog.rs`, `src/dashboard/data_cache.rs` | Last-good state, malformed JSON, atomic failures, filter semantics | Extend; do not weaken or delete safeguards. |

### Current pitfalls to address

- `SnippetDialog::ui` currently renders `entry.alias` and **all** `entry.text` inside an unconstrained horizontal row before Edit/Remove. A vertical scroll area does not constrain this width.
- `SnippetDialog::open_edit` currently preloads `self.text` and immediately displays a multiline text editor; this is inappropriate for a masked entry until revealed.
- The editor currently removes an entry immediately after one click and commits by replacing a cloned vector. With new privacy flags, avoid a stale editor snapshot silently overwriting an intervening change.
- `append_snippet` currently changes only `entry.text` on an existing exact-match alias. **Keep the new flag on those updates**; new command-created entries should be unmasked by default.
- The Dashboard snippet button currently displays an excerpt and binds the full plaintext as hover help. This must be flag-aware.
- The Dashboard preview shortening helper slices by byte index, which may split multibyte UTF-8. While touching this preview, use safe character/grapheme/egui truncation as appropriate without changing the original stored text.
- Several source/tests construct `SnippetEntry` with struct literals; adding a required Rust field means all relevant literals must be updated to compile, without weakening any test.

## 4. Scope boundaries and invariants

### Required preservation

- No new `cs` command namespace or separate plugin; do not rewrite command parsing or action payloads.
- Keep `cs`, `cs list`, `cs edit`, `cs add`, `cs rm`, and ordinary `cs <query>` semantics, result matching, action behavior, and multiline copy fidelity.
- Do not change the clipboard monitor, clipboard-history retention, clipboard-history widget, or clipboard write semantics. A masked snippet **still enters the OS clipboard** when invoked and can still appear in history.
- Preserve the single persistent `snippets.json` and its current data location; no data wipe, forced migration, extra encrypted file, or competing privacy metadata store.
- Old JSON entries lacking the flag load as unmasked. Prefer `#[serde(default)]` and, where appropriate, omitting `false` on serialization to keep old files readable and avoid gratuitous JSON churn. Keep the wire field name stable and unambiguous (suggestion: `hide_contents`).
- Keep malformed/invalid source file safeguards, atomic save behavior, watchers, last-good in-memory snapshots, and error handling.
- Save failure must not turn a masked snippet unmasked, leak text, or discard unsaved body/alias changes silently.
- Preserve snippet ordering unless user explicitly changes it; do not alphabetically resort.
- Preserve exact alias identity and case sensitivity in command-based add/update; GUI duplicate validation is incremental and local to GUI create/rename.
- Do not change the global theme or shared egui design system to solve one row's layout.
- Keep app launch, hotkeys, focus/close flow, dashboard quick actions, radial actions, and command history functional.

### Privacy threat model (be precise)

This feature is intended to prevent **casual shoulder-surfing of snippet previews**. It is **not a vault/security boundary**.

- The source text remains readable in `snippets.json`, the underlying `clipboard:<text>` action payload, the OS clipboard after execution, and existing clipboard history. No claims of encryption or secret-safe storage.
- Never present the body of a **masked** snippet in an ordinary list row, snippet preview, hover help, tooltip, auto-expanded preview, or action label derived from a snippet preview.
- The snippet alias is always visible. Filtering still searches hidden body contents; when a user types sensitive content into the filter field, **their typed query is not masked**.
- Deliberate **Reveal to Edit** is the only sanctioned on-screen reveal inside a masked edit session. That action is transient and resets reliably.
- Do not build a global redaction/logging architecture. Avoid adding new diagnostics, toasts, or debug text that repeat masked bodies.

### Quality gates

Do not consider a checkpoint complete if it breaks compilation, knowingly regresses a nearby feature, modifies user data unexpectedly, or leaves the listed acceptance criteria unsatisfied. Keep verification proportional: low-cost checks at each commit, substantive targeted tests at designated gates, and one final relevant manual Windows UI inspection when available.

## 5. Milestone and checkpoint overview

**Four milestones; seven natural implementation commits plus a conditional remediation commit.** The IDs are stable reporting/commit labels; don't produce meaningless commits just to satisfy a count.

| Stage | Main ownership | Proposed commit subject | Gate |
|---|---|---|---|
| **M1-A** | Serialized hide flag and legacy compatibility | `feat(snippets): [M1-A] persist optional per-snippet content masking` | JSON compatibility/compilation path understood |
| **M1-B** | Command mutation preservation + shared display rules | `fix(snippets): [M1-B] preserve masking through updates and centralize previews` | Domain tests reviewed; targeted compilation deferred to planned gate if expensive |
| **M2-A** | Responsive management list | `fix(snippets-ui): [M2-A] keep row actions visible and improve filtering` | Long/wide/Unicode and filter behavior |
| **M2-B** | Inline removal + GUI alias validation | `feat(snippets-ui): [M2-B] confirm removal and prevent duplicate aliases` | Narrow functional tests, transactional safety |
| **M3-A** | Masking checkbox and concealed row/form state | `feat(snippets-ui): [M3-A] add masked snippet editor presentation` | UI state tests |
| **M3-B** | Reveal-to-edit lifecycle and mutation integration | `feat(snippets-ui): [M3-B] require deliberate reveal for masked editing` | **Primary targeted test gate** |
| **M4-A** | Dashboard/other CS preview coverage | `fix(dashboard): [M4-A] honor hidden snippet previews and tooltips` | Cross-surface regression gate |
| **M4-B** | Final regression evidence and documentation | No commit required for test execution alone. If substantive docs/tests change, use `test(snippets): [M4-B] cover editor privacy regressions` or `docs(snippets): [M4-B] document display-only masking` | **Final verification/review** |

If an implementation step naturally contains enough coherent changes to warrant an additional subcheckpoint, adjust within the same milestone; report the change, use another descriptive suffix, and never create dummy commits.

---

## 6. M1 — Snippet schema and shared invariants

### M1-A — Persist an optional per-snippet hide flag

**Objective:** Make privacy a property of the existing snippet, not a separate UI-only state or metadata file.

**Ownership:** `src/plugins/snippets.rs` and only compile-required test/call-site adjustments.

**Steps:**

1. Inspect the current `SnippetEntry`, `load_snippets_typed`, `save_snippets`/`update_snippets`, watcher path, and all `SnippetEntry` constructors. Confirm whether a hide flag already exists on the active branch; do not add a second one.
2. Add one clearly named boolean flag, preferably `hide_contents: bool`, using Serde backward-compatible defaulting. Choose a stable false-omitting serialization rule if consistent with current persisted conventions.
3. Audit all explicit Rust struct-literal constructors (including source-internal tests, `tests/snippets_plugin.rs`, `tests/plugin_exact_match.rs`, and Dashboard data-cache tests). Supply explicit default `false` or a straightforward constructor/default helper when appropriate; avoid a sweeping API redesign.
4. Keep the saved file a JSON array of snippet objects containing `alias`, `text`, and the optional new boolean. Do not add a schema wrapper, version migration runner, or separate metadata document.
5. Ensure both `false` (or absent) and `true` round-trip through the real load/save API. The plaintext `text` and original alias must remain byte-for-byte equivalent after deserialization/serialization apart from ordinary JSON escaping.
6. Use an isolated temp directory for fixture tests. Do not edit the user's live `snippets.json` or reinterpret malformed/empty/missing-state semantics.
7. Inspect the diff before committing. Run formatting/diff checks; if the changed constructors are numerous, consider one compile pass only at a planned checkpoint gate rather than repeated full builds.

**Focused tests:**

- Legacy `[ { "alias": "a", "text": "first" } ]` deserializes with masking disabled.
- New masked entry deserializes/serializes with flag `true`.
- Unmasked entry and existing fixtures round-trip with old semantics.
- Malformed file remains an error; no content is replaced.
- Existing save/reload/watch tests remain present with adapted constructors.

**Acceptance:** New field loads old data and persists correctly, with no user-visible behavior change yet.

**Commit:** `feat(snippets): [M1-A] persist optional per-snippet content masking`.

### M1-B — Preserve masking across command updates; centralize display policy

**Objective:** Keep all pre-existing execution paths intact while creating one lightweight, testable display policy shared by consumers.

**Ownership:** `src/plugins/snippets.rs`, with one appropriate shared presentation helper if justified by multiple consumers.

**Steps:**

1. Review `append_snippet`: update an existing entry's `text` **without touching its hide flag**, as the current mutation structure already suggests. New CLI-created snippets start with flag `false`.
2. Preserve exact command-based alias matches and upsert behavior. Do not introduce duplicate-alias errors for `cs add` / inline `cs edit` commands, and do not turn a masked update into an unmasked update.
3. Preserve `remove_snippet`, typed command dispatch, fuzzy search by body, and exact copy actions (`clipboard:<original text>`). Do not route execution through masked display strings.
4. Define a pure, side-effect-free preview policy usable by the editor and Dashboard. It should return only a safe display fragment for a masked entry, including a constant mask such as `******`; it must not accidentally reveal content in hover help. Keep it small and near the existing domain or presentation modules rather than inventing an application-wide secret-management framework.
5. For an unmasked entry, create a single-line display projection that substitutes line breaks/tabs/control characters for visual whitespace; **do not modify saved text**. Truncation must be Unicode-safe and ideally width-driven by egui rather than arbitrary byte indexing.
6. Ensure privacy status propagates through the existing watcher and Dashboard `SnippetEntry` snapshots without creating parallel flags or violating current save/version semantics.
7. Extend focused tests: masked command add/edit/upsert remains masked; new `cs add` creates unmasked; search masked body returns alias-labeled action whose payload is still the exact real text; no accidental mask string copied.
8. Only change related consumers where the display policy is needed; broad replacement of all `Action` payloads is forbidden.

**Acceptance:** Flag continuity, normal search/copy/removal, and a safe preview helper are proven without changing command grammar.

**Commit:** `fix(snippets): [M1-B] preserve masking through updates and centralize previews`.

---

## 7. M2 — Responsive editor and high-value controls

### M2-A — List layout, clipping, filtering, sizing

**Objective:** Eliminate inaccessible row buttons and improve basic list usability.

**Ownership:** `src/gui/snippet_dialog.rs` and focused helper tests.

**Steps:**

1. Inspect the current egui window sizing, list `ScrollArea::vertical`, `ui.horizontal` row, row ordering, and state change side effects. Keep the same dialog entry paths (`cs` and `cs edit`).
2. Replace unconstrained full-width snippet labels with a bounded text-preview region that uses the **available remaining row width**, after reserving fixed-width Edit/Remove controls. Use the idiomatic egui 0.27 layout primitives present in this repo; do not assume a newer egui API exists.
3. The default row displays `<alias>: <single-line snippet excerpt>` and truncates with an ellipsis when necessary. Account for a very long **alias** as well as long body text; neither may displace the actions. Keep the alias distinguishable from the text at reasonable widths.
4. Ensure that normal list previews have consistent line height and don't expand to multiple text lines; preserve list scrolling for many snippets. Don't introduce whole-row horizontal scrolling as the primary solution.
5. Establish a practical window minimum width and resizable dimensions without making the dialog massive or trapping actions offscreen at high DPI. At extreme narrow widths, prefer a controlled responsive layout (e.g., preview clipped above a fixed action row) over hiding controls.
6. Give the Add/Edit multiline field meaningful usable dimensions and expansion behavior when the window grows; do not alter actual snippet text on resize.
7. Add **Clear Filter** next to the filter input, enabled/visible when appropriate. Clearing resets only the filter, not editing state or saved entries.
8. Add a visible **`N of M snippets`** match count (including `0 of M`) based on the same filtering predicate already used to display rows. Keep the existing empty-state message.
9. Preserve case-insensitive substring matching across alias and body; this includes masked bodies later. No sort/reorder/fuzzy rewrite in this stage.
10. Avoid auto-showing the full unmasked text in a tooltip from a layout helper if masked-state behavior hasn't landed yet; share the flag-aware formatter from M1.

**Tests / review:**

- Pure filtering/count tests for empty filter, case-insensitive alias match, body-only match, and no matches.
- Pure preview tests for newline/tab normalization and multibyte text.
- Manual Windows UI spot-check at ordinary and narrow widths, long aliases, very long text, and several rows; Edit/Remove always reachable.

**Acceptance:** No horizontal offscreen Edit/Remove; predictable clipped one-line previews; Clear Filter and count work; the Add/Edit field resizes usefully.

**Commit:** `fix(snippets-ui): [M2-A] keep row actions visible and improve filtering`.

### M2-B — Safe inline removal and editor-only duplicate validation

**Objective:** Add confirmation and guard against accidental duplicate aliases without changing command semantics or breaking data safety.

**Ownership:** `src/gui/snippet_dialog.rs`, plus a minimal transactional helper in `src/plugins/snippets.rs` **only if needed** for correctness.

**Steps:**

1. Replace immediate row removal with ephemeral state marking a single pending target. First click: show inline Confirm and Cancel for that row; no save/mutation at this point.
2. Confirm removes only the specifically selected entry. Cancel leaves persistence, list, filter, and row order untouched. Keep button visibility guaranteed during confirmation.
3. Reset the pending confirmation when the filter changes, list/dialog closes, an editing flow begins, or a different row is targeted. Do not leave a stale confirmation affecting a later row.
4. Because old files might already have duplicate aliases and lists can change via commands/watchers, do **not** rely on alias alone to identify a row. Use a guarded original index plus expected entry snapshot, or a similarly simple stale-state check. If the target changed externally, reject/refresh rather than deleting the wrong entry.
5. Evaluate current whole-list `replace_snippets` from the GUI. A stale editor snapshot can overwrite an intervening command update and potentially reset privacy flags. Prefer a transactionally safe **targeted update** through existing `update_snippets` where practical; if a guarded snapshot approach is chosen instead, explicitly test conflicts. Do not broaden into a generalized database migration.
6. GUI Add validation: if **another** entry already has the proposed exact alias, present a useful inline error and do not write. GUI Rename validation: allow an unchanged alias for the same entry, but block naming it exactly like a different entry. Use current case-sensitive equality; do not silently normalize.
7. Do not automatically delete/merge pre-existing duplicate aliases. Make existing records accessible for rename/edit/removal. Avoid rejecting a benign edit solely because the file already contains historical duplicates elsewhere.
8. Leave CLI `cs add` / `cs edit` exact-match overwrite semantics and existing `cs rm` removal invocation as they are. The inline confirmation belongs to the management UI, not command dispatch.
9. On a failed transaction/invalid file, leave the edit draft usable, show existing error feedback, and retain the last-good in-memory state. Do not show success or clear state prematurely.
10. Refresh launcher search/snapshot once after successful modifications using existing mechanisms; avoid extra watcher loops, parallel caches, or repeated writes on egui repaint.

**Tests / review:**

- First Remove click does not modify disk; Cancel doesn't mutate; Confirm removes exactly once.
- Confirmation invalidates when filter changes or target is stale; no accidental deletion of another row.
- GUI new/renamed duplicate alias rejected; current row's unchanged alias accepted; `Foo` and `foo` behavior follows existing exact equality.
- Prior command-based upsert and `cs rm` tests remain valid.
- External/intervening modification between editor open and commit cannot be silently replaced with an old version.

**Acceptance:** Confirmation and duplicate checks work while preserving atomic writes, existing malformed-file safeguards, and command semantics.

**Commit:** `feat(snippets-ui): [M2-B] confirm removal and prevent duplicate aliases`.

---

## 8. M3 — Per-snippet masking and reveal-to-edit

### M3-A — Privacy controls and concealed presentation

**Objective:** Let users configure per-snippet display masking without adding any new plugin or altering execution.

**Ownership:** `src/gui/snippet_dialog.rs` plus the shared flag-aware preview policy from M1.

**Steps:**

1. Add `Hide contents` to the existing Add/Edit form, within reach of the alias/body controls. Don't add a list-wide mask toggle, a row-level checkbox, or a second global privacy setting.
2. Initialize new GUI entries as unmasked; initialize an existing entry's checkbox from its persisted flag. Toggling must affect **only that snippet** when successfully saved.
3. In the list, render the alias followed by `******` for flagged snippets, plus a small eye-off/hidden indicator with accessible explanatory text. The indicator must not include the body as hover help or accessible label.
4. Unmasked rows continue to show their one-line truncated preview from M2. Do not weaken the permanently visible Edit/Remove layout for the sake of the icon.
5. Keep the filter predicate independent from the masked display preview: masked text still participates in case-insensitive body search and count totals. When a hidden text match occurs, show only the alias, mask and normal action controls, not a highlighted body excerpt.
6. Preserve the checkbox on the same snippet after an alias edit, body edit, command-based edit, save/reload, or Dashboard cache refresh.
7. Make the edit form's unrevealed state explicit: render a concise `Contents hidden` placeholder with a **Reveal to Edit** control, not a `TextEdit` filled with actual plaintext until the user opts to reveal.
8. Handle the create flow intentionally: a newly created draft can be edited normally even if its Hide Contents checkbox is checked during creation; its saved/reopened representation must start concealed. If toggling Hide Contents on an *existing* visible draft, immediately conceal its body or require another explicit reveal, while preserving the underlying draft.
9. Ensure save paths do not convert `******` or placeholder text into persisted snippet contents. Alias/privacy-only updates must preserve the original plaintext. Don't use placeholder strings as model data.
10. Verify that existing load error/read-only mode still prevents edit/delete/add and does not accidentally reveal hidden text.

**Tests / review:**

- Flagged list entry never contains real body in constructed display text or hover text.
- Unflagged entry preview still shows expected single-line content.
- Filter matching on masked body yields a masked visible row.
- Adding a new masked snippet preserves original entered body on save.
- Mask-only edit leaves body unchanged.

**Acceptance:** Per-entry privacy choice saves and affects list/form presentation with no change to copy/query behavior.

**Commit:** `feat(snippets-ui): [M3-A] add masked snippet editor presentation`.

### M3-B — Explicit reveal, lifecycle reset, and safe saving

**Objective:** Make reveal intentional and temporary through every opening and dismissal route.

**Ownership:** `SnippetDialog` state transitions and relevant command/UI routing integration.

**Steps:**

1. Introduce a narrowly scoped **ephemeral per-edit-session reveal state**. It is **not persisted**, not global, and not shared between snippets.
2. Opening a saved masked entry from **list Edit**, `cs edit <alias>`, or an existing Universal Action Edit route must start concealed. Include the case where a dialog was previously used to edit a different unmasked/revealed entry.
3. Only the explicit **Reveal to Edit** interaction may make the actual saved body visible in the edit text area. Avoid rendering any hidden draft as plaintext in tooltips, widget help, hover regions or accidental expansion prior to reveal.
4. The edit state must separately track (a) persisted/original text, (b) currently edited draft when exposed, and (c) whether a deliberate reveal occurred. Avoid using asterisks as the editable value.
5. A masked entry may be renamed or have Hide Contents unchecked/checked without a reveal; this must preserve its exact existing text. Merely unchecking the saved hide flag should not automatically reveal the body in the current session.
6. If a user edits body after revealing, Save uses the actual edited body; if they do not reveal, Save uses the original body. Preserve multiline content and Unicode exactly.
7. Reset reveal state on **Cancel**, successful Save, dialog Close button, egui title-bar **X**, opening another entry, switching from list to Add, failed-load reset, and reopen. Verify a canceled edit never mutates disk.
8. On save failure, preserve sensible state: no data loss, no success toast, and no leak due to fallback rendering. If draft contents remain visible after a deliberate reveal, keep them only while the same active edit session remains open; closing must conceal them.
9. Keep alias/flag changes coherent with M2's guarded mutation strategy. A simultaneous `cs add` / watcher update must not silently overwrite someone else's privacy flag or body.
10. Preserve the launcher's existing editor close/focus/refresh behaviors. Avoid new app-level focus hacks or global hotkey changes.
11. Add headless/pure state-machine tests where feasible for `open`/`open_edit`/begin edit/reveal/cancel/save/close. A small state model or helper methods are preferable to tests tied to pixel-perfect egui rendering.
12. Document the visual-privacy limitation with a short piece of UI help text (e.g. next to the setting): masking hides previews, but snippets remain plaintext on disk and on the clipboard after use. Keep it succinct and non-alarmist.

**Required cases:**

- Open hidden via row Edit → concealed; reveal → exact body; close/reopen → concealed.
- Open hidden via `cs edit alias` → concealed; reveal/close/reset work.
- Hidden → edit alias without reveal → save → full original body persists and flag unchanged.
- Hidden → uncheck flag without reveal → save → original body persists and previews become unmasked.
- Reveal → edit body → save → exact new body persists; if flag stays on, next open is concealed.
- Reveal → cancel → no changes on disk; next open concealed.
- Open one hidden entry, reveal, then open another hidden entry → second starts concealed.
- Save failure/invalid JSON doesn't clear/rewrite persisted data or falsely confirm success.

**Primary targeted test gate after this checkpoint:** On the actual Windows development/build machine, run the narrowest available `snippets` domain/dialog tests and one meaningful compile/type-check or filtered nextest invocation that covers the affected modules. It is acceptable to perform the long build **once here**, rather than between every small commit. Use failures to drive bounded remediation, not a broad app rewrite. Do not silently replace tests with snapshots that no longer prove behavior.

**Acceptance:** No accidental reveal via ordinary edit entry/exit and no content corruption or command regression.

**Commit:** `feat(snippets-ui): [M3-B] require deliberate reveal for masked editing`.

---

## 9. M4 — Cross-surface coverage, regression verification, and documentation

### M4-A — Respect masking in Dashboard and other existing CS previews

**Objective:** No existing snippet-preview surface visibly discloses a masked body during normal browsing.

**Ownership:** `src/dashboard/widgets/clipboard_snippets.rs` plus any other *confirmed* CS snippet-body preview consumer.

**Steps:**

1. Replace raw `snippet.text` preview generation in the Dashboard's snippet row with the shared mask-aware presentation policy. Keep the visible alias.
2. For masked snippets, **remove or replace** `.on_hover_text(&snippet.text)` with innocuous privacy help (or no tooltip). Do not show the body via hover, clipboard snippet widget title, or auto expansion.
3. For unmasked snippets, preserve their existing truncated preview/hover behavior as closely as possible. Fix the byte-slicing/UTF-8 hazard in the local shortening utility if not already solved by the shared formatter.
4. The Dashboard snippet click must still issue an action that copies the **actual** stored content. Do not copy the mask string; do not change `query_override` or unrelated widget refresh behavior.
5. Keep the **Clipboard history** section of the same widget untouched: existing history text, copy action, hover behavior and recording rules are independent of snippet masking.
6. Audit other consumers using `SnippetEntry` or snippet-derived `Action`: launcher results, `cs list`, `cs rm`, edit actions, Universal Action Catalog/Action Sheet, history, radial dynamic snippets, and any snippet display utility. Change only those that truly place snippet **body previews** in the UI. Where labels are already alias-only, preserve them.
7. Be alert for raw `clipboard:<text>` action payloads exposed in a visible metadata panel. Do not change action execution or saved action IDs to solve a UI preview; instead mask the presentation of a known masked snippet where applicable. Any general-purpose developer/debug view that is outside this narrow scope should be documented rather than triggering a broad security redesign.
8. Check that all consumers receive the privacy flag on data refresh through the existing JSON/watcher/cache pipeline; avoid manual invalidation hacks if current mechanisms suffice.
9. Add focused tests for Dashboard label/tooltip generation (extract tiny pure presentation helper if needed). Ensure an emoji, CJK, accented letter or other multibyte snippet cannot panic from truncation.

**Acceptance:** Dashboard and other confirmed CS preview surfaces show `******`/hidden indicator with no plaintext hover, while clicking copies the original and clipboard-history behavior remains unchanged.

**Commit:** `fix(dashboard): [M4-A] honor hidden snippet previews and tooltips`.

### M4-B — Targeted verification, user-facing documentation, and review

**Objective:** Produce evidence that Plan A works without running an unrelated repository-wide test campaign.

**Steps:**

1. Add/adjust the smallest number of focused tests that meaningfully prove the scenarios in section 10. Reuse existing tests rather than duplicating suites just to increase test count.
2. Update `README.md` only where the `cs` usage/behavior documentation belongs: editor truncation, Hide Contents, Reveal to Edit, search/copy unchanged, display-only disclaimer, and inline deletion confirmation. No lengthy generic security documentation required.
3. Run formatting and whitespace validation and the relevant filtered snippets-related tests. A suggested starting point on the user's Windows build environment is `cargo nextest run --test snippets_plugin`; also cover relevant in-module `SnippetDialog` and Dashboard tests using the precise filters available in the actual test binary. If the existing suite/configuration differs, inspect help/list and adapt.
4. Run one `cargo check` or appropriate build check if a successful compile has not already been established by the M3 gate. Avoid duplicate expensive rebuilds. Full `cargo nextest run` is **not a default requirement**.
5. Execute a single **targeted Windows manual acceptance pass** focused on resizing, long aliases/bodies, normal/hidden previews, filter, save/cancel/close/reopen, Dashboard tooltip/copy, and malformed-file rejection. Use disposable snippet data or backup/restore; do not test on irreplaceable live user content.
6. Request independent **reviewer** inspection of the actual diff against this plan. Prioritize leaked snippet body in UI, corrupted content, incorrect defaulting, stale-save overwrites, broken `cs` semantics, and inadequate tests. Do not turn review into an unrelated architecture cleanup.
7. Fix substantive review/test issues in narrowly scoped follow-up commits, tagged to the stage that owns the defect where practical (example: `fix(snippets-ui): [M3-B] reset reveal state on window close`). Do not squash or rewrite prior checkpoints.
8. Final report: stages completed, exact commits/hashes, file list, relevant tests run/results, manual UI checks performed/not performed, known limitations, and explicitly deferred items. Distinguish verified from unverified results.

**Acceptance:** All Plan A criteria satisfied or any remaining limitation precisely documented; no invented test-pass claims. Do not create an empty M4-B commit if the only work is testing/review.

---

## 10. Explicit acceptance and regression matrix

Use focused unit/integration tests where feasible plus one targeted manual UI pass for geometry. **P0** means must pass before Plan A is complete.

| ID | Priority | Setup / action | Expected result |
|---|---|---|---|
| T01 | P0 | Load old JSON with only `alias` and `text` | Loads successfully; `hide_contents=false`. |
| T02 | P0 | Save/reload `hide_contents=true` | Flag persists; alias/body unchanged. |
| T03 | P0 | Save/reload unmasked entry | Behavior and JSON compatibility retained. |
| T04 | P0 | Malformed `snippets.json`, try Add/Edit/Delete | Read-only error; original bytes preserved. |
| T05 | P0 | Existing masked alias updated using `cs add` or inline `cs edit` | Text changes; masking flag stays `true`. |
| T06 | P0 | `cs list`/`cs query` matches masked snippet body | Alias-labeled result appears, with no visible body preview. |
| T07 | P0 | Execute masked snippet | Exact original bytes/text copied, **not** `******`. |
| T08 | P0 | Multiline and Unicode masked snippet copied | Exact line breaks/Unicode preserved. |
| T09 | P0 | Show 10,000-character snippet row | One line, truncated; Edit/Remove visible. |
| T10 | P0 | Show extremely long alias | Actions still reachable; no runaway horizontal width. |
| T11 | P0 | Narrow/resized window | No clipped/overlapping action controls; editor usable. |
| T12 | P0 | Normal snippet with newlines/tabs | Preview single-line; stored body unchanged. |
| T13 | P0 | Hidden snippet on list + hover | `******`; no content tooltip or hidden-text excerpt. |
| T14 | P0 | Open hidden via row Edit | Body concealed until explicit reveal. |
| T15 | P0 | Open hidden via `cs edit <alias>` | Body concealed until explicit reveal. |
| T16 | P0 | Reveal then Cancel/Close/X and reopen | Concealed again; canceled text not saved. |
| T17 | P0 | Hidden entry alias/checkbox only edit, never reveal | Original body survives save. |
| T18 | P0 | Reveal, edit body, save/reopen | New exact body saved; if still masked, starts concealed. |
| T19 | P0 | Edit one hidden entry then another | Reveal state not inherited by second. |
| T20 | P0 | Filter matches only masked body | Row remains masked; match count accurate. |
| T21 | P1 | Clear Filter | All rows restored; `N of M` updated. |
| T22 | P0 | GUI create/rename to exact alias of another row | Save rejected without disk mutation. |
| T23 | P0 | GUI edit with unchanged alias; CLI exact-match upsert | Both remain allowed. |
| T24 | P0 | Click Remove once | Only inline Confirm/Cancel appears; disk unchanged. |
| T25 | P0 | Cancel vs Confirm remove | Cancel no mutation; Confirm exactly one entry. |
| T26 | P0 | Change filter or target while confirmation open | Stale confirmation cannot delete other entry. |
| T27 | P0 | External change while editor has stale draft | No silent overwrite/loss of new text/flag. |
| T28 | P0 | Dashboard hidden snippet row + hover | Only alias/mask shown; never hidden plaintext. |
| T29 | P0 | Click hidden Dashboard snippet | Copies full original text. |
| T30 | P0 | Dashboard ordinary clipboard-history section | Completely unchanged by snippet flag. |
| T31 | P1 | Unmasked emoji/CJK snippet displayed | No panic or invalid UTF-8 truncation. |
| T32 | P0 | Reopen after app restart/watcher refresh | Flag and concealment survive; command functionality intact. |
| T33 | P1 | Invalid save/permission error | No false saved toast; last-good/disk contents preserved. |
| T34 | P1 | Existing duplicates in old file | They load; no implicit merge/deletion; single-row edits/removes remain safe. |

**Manual acceptance setup suggestion:** Create throwaway aliases `short`, `long-alias`, `long-body`, `hidden-secret`, `unicode`, and `multiline`, preferably with a temporary data directory or controlled backup. The exact displayed payload of `hidden-secret` should never appear in ordinary CS/Dashboard previews before explicit editor reveal.

## 11. Verification budget and handling slow Windows builds

The user's Rust build/test environment can be slow; reduce redundant waits without claiming success on unexecuted checks.

- **At each meaningful checkpoint:** `git status --short`, `git diff --check` and formatting for touched code (e.g. `cargo fmt --all -- --check` when practical), inspect the diff, then commit. Do **not** run the full suite before each commit.
- **M1–M2:** Prefer quick unit/pure-helper checks, source inspection, test additions, and a single targeted compile if needed; preserve coherent working checkpoints.
- **After M3-B:** Execute **one primary focused nextest/build gate** that exercises plugin and dialog behavior. Compile failures are fixed before declaring the milestone done. Follow-up verification should reuse the narrowest applicable target/filter.
- **After M4-A / during M4-B:** Execute only the additional targeted checks needed for Dashboard and cross-surface coverage, then one manual native Windows UI pass where accessible. Avoid rerunning an already successful expensive full build without changed code affecting it.
- Suggested command patterns (adapt to actual repo/test listing):

```powershell
# Cheap checks; can be frequent.
git diff --check
cargo fmt --all -- --check

# More expensive; batch at major gates on the Windows build host.
cargo nextest run --test snippets_plugin
cargo nextest run --lib -E 'test(snippet)'
# If present, use additional exact-name tests for dashboard preview helpers.
# One compile/type-check only if coverage above did not already establish it.
cargo check
```

- Do not demand repeated broad builds, full `cargo nextest run`, performance benchmarking, unrelated radial tests, UI automation campaigns, or synthetic legacy harnesses unless a directly attributable defect requires them.
- If a long-running build/test is executing, **do not spam short polling loops**. Use the orchestration environment's standard wait/return behavior; if periodic observation is required and supported, check at a relaxed **10–20 minute cadence**, not every few seconds. Never treat silence as proof of success.
- Never commit compiled artifacts, user JSON data, secrets, or unrelated logs.

## 12. Active checkpoint commit cadence (binding)

**Use the project’s active checkpoint commit cadence and define the task-specific commit boundaries in the plan.**

Use an active, milestone-based commit cadence throughout this task. Break larger milestones into coherent implementation checkpoints such as `M1-A`, `M1-B`, `M2-A`, etc., and commit after each **meaningful complete subsection**, rather than waiting until the feature is finished.

Rules:

1. Do **not** commit every tiny edit, individual line, or meaningless WIP state.
2. Do **not** accumulate multiple substantial independent changes into one giant commit.
3. Before a materially different subsection, prefer committing the previous coherent subsection.
4. Check repo status and stage only this task's files. Respect pre-existing user changes and branches; never overwrite, revert, or silently include unrelated changes.
5. Use descriptive subjects: `<type>(<scope>): [M#-X] <clear description>`.
6. Add brief commit bodies for non-obvious migrations/invariants, e.g. explaining display-only masking and preserved command copy semantics.
7. Full expensive verification is **not required before every checkpoint**. Check syntax/format/diff and logically relevant tests as practical; reserve costly targeted verification for planned gates.
8. If testing/review uncovers a defect, make a transparent remediation commit (stage-tagged), not a rewritten history.
9. Do **not** squash, amend, or rewrite checkpoints unless the user explicitly requests it.
10. A stage with only checking/reporting doesn't warrant an empty commit.

### Suggested commit subjects — quick reference

```text
feat(snippets): [M1-A] persist optional per-snippet content masking
fix(snippets): [M1-B] preserve masking through updates and centralize previews
fix(snippets-ui): [M2-A] keep row actions visible and improve filtering
feat(snippets-ui): [M2-B] confirm removal and prevent duplicate aliases
feat(snippets-ui): [M3-A] add masked snippet editor presentation
feat(snippets-ui): [M3-B] require deliberate reveal for masked editing
fix(dashboard): [M4-A] honor hidden snippet previews and tooltips
# Conditional only when a meaningful diff exists:
test(snippets): [M4-B] cover editor privacy regressions
```

## 13. Codex orchestrator contract

1. **You are the orchestrator**, not merely the implementer. Coordinate the repository-configured **planner**, **implementer**, and **reviewer** agents from `.codex/agents/` while following `AGENTS.md`.
2. Inspect the current working tree, branch, this approved plan, and the relevant source. Compare the checked-out code with the source-of-truth archive only as needed to identify differences; do not silently overwrite a newer working tree with an archive.
3. Ask the **planner** for a bounded execution handoff based on this approved plan: architecture ownership, dependencies, tests, non-goals, and natural checkpoints. Do not ask it to invent new products or run unrelated verification.
4. Assign **one explicit checkpoint or milestone handoff at a time** to an **implementer**. Give it the agreed acceptance criteria, files/flows to inspect, tests relevant to the work, and exclusions. Do not have multiple implementers concurrently editing `src/gui/snippet_dialog.rs` or `src/plugins/snippets.rs`.
5. As orchestrator, verify each completed subsection is coherent, examine diffs/status, and ensure the associated stage-tagged checkpoint commit exists before moving to materially different work.
6. Involve the **reviewer** at substantive checkpoints (after privacy editing behavior and at final integrated scope) to look for correctness, compatibility, UI leakage, stale-data destruction and unmet tests. Avoid repetitive, open-ended review cycles.
7. Use controlled remediation for review findings and only request user intervention for a genuine blocker that cannot be safely resolved from the current code/approved requirements.
8. Don't modify existing Codex agent config, global instructions, or unrelated architecture as part of this task.
9. Continue through the milestones autonomously; produce a concise final report with commits, tested results, omissions and remaining limitations. No claim of completion without checks.

## 14. Definition of done

The task is complete only when:

- [ ] The CS editor is resizable and row Edit/Remove controls remain accessible with long snippet bodies and aliases.
- [ ] Normal rows show a single-line truncated preview; Clear Filter and `N of M` work.
- [ ] `Hide contents` is persisted per snippet; old/new snippets default unmasked.
- [ ] Masked list/Dashboard rows show alias + `******`, with no hidden body in hover help or other in-scope snippet previews.
- [ ] Masked editing begins concealed and only explicit Reveal to Edit exposes text; lifecycle reset is verified.
- [ ] Save/rename/checkbox-only edits preserve exact actual body; command-based `cs` behavior, search and copy remain unchanged.
- [ ] Duplicate alias GUI validation and inline removal confirmation behave safely.
- [ ] Malformed file, failed write, watcher refresh and concurrent update safeguards remain intact.
- [ ] Relevant tests and manual checks have honest results recorded; unperformed checks are flagged as such.
- [ ] Reviewer findings addressed, scope stays narrow, and coherent stage-tagged commit history exists.
- [ ] The final Codex report states that masking is **display-only**, not clipboard protection or encryption.

**End of approved Plan A.** Prompted fields belong to Plan B and must not be started under this plan.
