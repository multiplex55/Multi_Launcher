# Multi Launcher — Plan B: Prompted Snippet Fields

**Status:** M1-A through M6-A implementation and scoped automated evidence complete; M6-B P2 remediation is focused-verified with parent commit and reviewer reread pending; native smoke remains environment-blocked.\
**Deliverable:** Extend the existing Clipboard Snippets (`cs`) plugin to support opt-in, user-filled template fields, without creating a new plugin.  
**Execution baseline:** `prompt-fields` at `888a55fb793cdcf2c44202ed56f2752abe8c9da1`; the user approved the current branch HEAD in place of the ZIP requirement on 2026-10-07.\
**Original reference snapshot:** `multi_launcher(20261007-185532).zip` (SHA-256 `0d1deb806acd61857693a0559d2cd5cc5bf7f4336e3259969e9749ce9504e7ec`); retained as historical context, not an execution prerequisite.\
**Companion kickoff:** `multi_launcher_cs_prompted_fields_codex_start.md`  
**Authority:** The agreed Plan B questionnaire, this plan, the checked-out repository's `AGENTS.md`, and the user's execution-baseline approval recorded above. The checked-out current branch is authoritative for implementation.

**Execution ledger:**

- M1-A — complete; commit `37d3c3704c9278c78df7911fab6ed4e50925c745`.
- M1-A verification: `cargo fmt --all` and `git diff --check` passed; `cargo nextest run --lib -E 'test(plugins::snippets::persistence_tests)'` passed 13/13; `cargo nextest run --test snippets_plugin --test plugin_exact_match -E 'test(load_save_roundtrip) or test(snippet_edit_command_unfiltered)'` passed 2/2 before the final label-fallback-only correction.
- M1-B — complete; commit `f08b95faf76d3f0400eeaf651662ba25e7fd051f`.
- M1-B verification: `cargo fmt --all` and `git diff --check` passed; `cargo nextest run --lib -E 'test(plugins::snippets::persistence_tests) or test(gui::snippet_dialog::tests::prompted_editor_save_preserves_configuration_on_alias_and_text_changes) or test(gui::snippet_dialog::tests::prompted_editor_cancel_discards_alias_and_text_draft_without_writing) or test(gui::snippet_dialog::tests::prompted_editor_no_op_save_preserves_bytes_and_version)'` passed 19/19; `cargo nextest run --test snippets_plugin -E 'test(launch_action_add_saves_snippet) or test(command_add_and_inline_edit_preserve_prompt_metadata_and_hidden_flag)'` passed 2/2.
- M2-A — complete; commit `7c630598b49093b7da9ae989097ff4a086f5cd88`.
- M2-A verification: `cargo fmt --all` and `git diff --check` passed; `cargo nextest run --lib -E 'test(plugins::snippet_template::tests) or test(plugins::snippets::persistence_tests) or test(gui::snippet_dialog::tests::prompted_editor_reconciles_fields_in_candidate_and_commits_them_only_on_save) or test(gui::snippet_dialog::tests::prompted_editor_invalid_text_keeps_inline_error_draft_and_snapshot)'` passed 28/28.
- M2-A adaptation: Activated the M1-B-deferred validation only for changed prompted text in append and editor Save; load/open/watcher remain tolerant. Duplicate configured keys are rejected before key reconciliation so malformed metadata is not silently discarded.
- M2-B — complete; commit `6d4d5f64`.
- M2-B verification: `cargo fmt --all` and `git diff --check` passed; `cargo nextest run --lib -E 'test(plugins::snippet_template::tests) or test(plugins::snippets::persistence_tests::runtime_preparation_normalizes_discovered_fields_without_mutating_config) or test(plugins::snippets::persistence_tests::runtime_preparation_rejects_duplicate_config_before_reconciliation)'` passed 14/14.
- M2-B scope note: Renderer-side validation covers definitions, completeness and required values; runtime preparation supplies effective discovered fields without persisting those transient changes.
- M3-A — complete; commit `bf34b287`.
- M3-A verification: `cargo fmt --all` and `git diff --check` passed; `cargo nextest run --lib --test snippets_plugin -E 'test(plugins::snippets::persistence_tests) or test(commands::parser::tests::exact_manager_storage_timer_system_and_layout_variants_are_owned) or test(commands::handlers::snippet_run::tests) or test(commands::headless::snippet_headless_tests) or test(commands::bus::tests::invalid_snippet_run_uses_the_typed_handler_without_headless_fallback) or test(universal_actions::resolver::tests) or test(search_returns_typed_snippet_run_action_without_body_payload) or test(search_run_action_keeps_newline_body_out_of_wire_payload) or test(hidden_body_search_and_list_keep_alias_identity_without_leaking_body)'` passed 44/44 selected tests. After restoring legacy Universal Action snippet identity, `cargo nextest run --lib -E 'test(universal_actions::resolver::tests)'` passed 9/9.
- M3-A/M5-A compatibility note: `SnippetRunMode` centralizes plain-versus-prompted preparation; GUI/headless adapters share it, and M5-A migrates the live Universal Action catalog to alias-based actions.
- M3-B — complete; commit `aa4ac8a4`.
- M3-B verification: `cargo fmt --all` and `git diff --check` passed; `cargo nextest run --lib -E 'test(gui::snippet_prompt_dialog::tests) or test(gui::command_host::tests::prompt_request_opens_and_focuses_one_panel_from_hidden_or_visible_root) or test(gui::command_host::tests::prompt_completion_uses_captured_history_and_root_policy_once) or test(commands::handlers::snippet_run::tests)'` passed 17/17; after centralizing cancel cleanup and adding panel-state assertions, `cargo nextest run --lib -E 'test(gui::command_host::tests::prompt_request_opens_and_focuses_one_panel_from_hidden_or_visible_root)'` passed 1/1.
- M3-B scope note: execute and preview state share one transient owner; preview cancel returns the exact unsaved entry while the existing editor instance remains the authoring-state owner. Prompt completion uses the captured history query and root policy; no dialog layout or keyboard controls are included.
- M3-C — complete; commit `f178e246`.
- M3-C verification: `cargo fmt --all` and `git diff --check` passed; `cargo nextest run --lib -E 'test(gui::snippet_prompt_dialog::tests) or test(gui::render::tests::snippet_prompt_focus_tab_multiline_and_escape_are_owned_by_the_dialog) or test(gui::render::tests::snippet_prompt_ctrl_enter_submits_once_and_preview_consumes_without_copy)'` passed 14/14 before and after render-test fixture isolation; the post-isolation invocation completed 14/14. The post-isolation `cargo nextest run --lib -E 'test(gui::render::tests::snippet_prompt_focus_tab_multiline_and_escape_are_owned_by_the_dialog) or test(gui::render::tests::snippet_prompt_ctrl_enter_submits_once_and_preview_consumes_without_copy)'` run passed 2/2.
- M3-C scope note: the fill/preview dialog owns keyboard input before launcher routes, uses fresh-generation widget identities, and submits/cancels through the M3-B session owner. `cargo build --locked --bin multi_launcher` passed (2m 52s). Native retry: sandbox launch had no discoverable window; the approved interactive-desktop isolated launch exposed Multi Lnchr, but screenshot capture/recovery timed out (`FrameArrived timed out: timed out waiting on channel` / `window capture timed out`), accessibility reads worked while click input failed (`coordinate input geometry is unavailable`), and Tab produced no editable focus. No text was typed and no clipboard write or native smoke assertion was made; the process was closed. This is an automation-environment limitation, not evidence for a source change.
- M4-A — complete; commit `844043c1`.
- M4-A verification: `cargo fmt --all` and `git diff --check` passed. `cargo nextest run --lib -E 'test(gui::snippet_dialog)'` reached 35/36 after the fixture correction; its only failure was the field-control paint assertion below. The first run's `editor_initializes_saved_privacy_and_new_drafts_safely` fixture was still a plain snippet despite asserting the opted-in flag; the fixture now uses a prompted entry. Session 35496 failed with `assertion failed: painted.contains("PRIVATE LABEL")`: the field grid was below the editor's outer-scroll viewport. The test retains the full-dialog hidden-value assertions and renders the same field-settings helper in a dedicated panel; after warming egui's Grid sizing pass once, `cargo nextest run --lib -E 'test(gui::snippet_dialog::tests::prompted_field_controls_follow_opt_in_and_masking_state)'` passed 1/1. This preserves the active/hidden assertions and accommodates the grid's initial layout pass rather than weakening them.
- M4-A scope note: prompt state and field definitions are detached editor draft state; active fields follow parser discovery order, removed settings remain private in the draft for re-addition, and shared preparation prunes only a successful prompted Save. Prompt-off edits bypass parsing and preserve literal text and authored metadata.
- M4-B — complete; commit `f15ebfe0`. `cargo fmt --all` and `git diff --check` passed. `cargo nextest run --lib -E 'test(gui::snippet_dialog)'` passed 38/38 (4,838 skipped). The first invocation caught one test-only nested mutable borrow; the fix stores the detached candidate before opening the preview, and the final scoped invocation passed. Build emitted existing unused-variable/unused-`Context::run` warnings in `src/gui/render.rs`.
- M4-B scope note: the detached editor candidate opens the shared PreviewOnly form only after the editor is restored; active preview suppresses editor rendering, and cancel/close returns focus to the same editor session without applying trial values.
- M5-A — complete; commit `14d2217e`. Dashboard Snippets and synthetic Universal Action catalog rows use the canonical alias route, while clipboard-history targets retain indexed literal-copy identity.
- M5-A verification: `cargo fmt --check` and `git diff --check` passed. The initial focused invocation `cargo nextest run --lib -E 'test(dashboard::widgets::clipboard_snippets::tests) or test(gui::universal_action_catalog::tests::dashboard_snippet_catalog_uses_alias_route_and_keeps_clipboard_history_literal) or test(gui::universal_action_catalog::tests::preview_catalog_browsing_never_executes_an_action) or test(gui::universal_action_catalog::tests::unchanged_designer_reuses_action_catalog_until_results_change) or test(universal_actions::resolver::tests::legacy_snippet_rows_keep_secondary_identity_and_primary_literal_action)'` passed 9/9. After isolating the new catalog fixture under a retained temporary directory and narrowing the clipboard helper to its index, `cargo nextest run --lib -E 'test(gui::universal_action_catalog::tests::dashboard_snippet_catalog_uses_alias_route_and_keeps_clipboard_history_literal) or test(dashboard::widgets::clipboard_snippets::tests::clipboard_history_action_preserves_index_and_clipboard_identity)'` passed 2/2.
- M5-B — complete; commit `1fc314d4`. Saved pins resolve only canonical aliases from the current dashboard snapshot, GUI history replay recognizes only typed snippet actions and delegates through the shared run handler, and radial execution derives UI ownership from current snippet mode. Opaque literal clipboard actions retain their existing generic path. Its scoped command passed 21/21 selected tests (4,864 skipped):

  ```text
  cargo nextest run --lib -E 'test(gui::tests::saved_pin_rebinds_only_by_canonical_alias_not_literal_body) or test(dashboard::widgets::command_history::tests::history_pins_keep_opaque_clipboard_literals_and_resolve_snippets_by_alias) or test(gui::radial_actions::tests::saved_radial_snippet_identity_uses_current_prompt_mode) or test(commands::handlers::headless_gui::tests::history_snippet_replay_uses_current_run_mode_and_captured_activation_context) or test(commands::handlers::headless_gui::tests::invalid_or_opaque_history_actions_never_infer_snippet_from_description) or test(plugins::snippets::persistence_tests::saved_run_identity_resolves_only_a_unique_current_alias) or test(universal_actions::resolver::tests) or test(commands::headless::snippet_headless_tests) or test(commands::handlers::snippet_run::tests)'
  ```

- M6-A — implementation and scoped automated evidence complete; documentation/evidence commit `9aa1f7a7ddafa67c211e18672845262292687e38`. README covers opt-in fields, grammar and escaping, defaults/validation, keyboard behavior, preview and privacy boundaries. The production dialog now subtracts egui's title bar and frame chrome from the available content size, and queues focused-field scrolling after the nested preview so the outer form owns the scroll target. Added a small-root production prompt/router test for many long-label fields, retained safe feedback, scroll content, keyboard traversal, and Copy/Cancel bounds. Existing radial tests prove current-mode preflight; native radial flash/focus remains unverified.
- M6-A remediation cadence: the sizing and scroll-ownership correction plus its A27 geometry evidence is committed separately as `ef35ea1cecffcc462c2b9c4165dbf56c00a83abd` — `fix(cs): [M6-A] keep prompted snippet controls within small roots`. The final documentation/evidence checkpoint is `9aa1f7a7ddafa67c211e18672845262292687e38`.
- M6-A combined verification: `cargo fmt --all`, `cargo fmt --all -- --check`, and `git diff --check` passed. The exact combined invocation selected 124 tests: 123 passed; only the new A27 geometry test failed and triggered the focused diagnosis/correction below. It was not rerun as a combined suite after the scoped correction.

  ```text
  cargo nextest run --lib --test snippets_plugin -E 'test(plugins::snippet_template) or test(plugins::snippets::persistence_tests) or test(gui::snippet_dialog::tests) or test(gui::snippet_prompt_dialog::tests) or test(gui::render::tests::snippet_prompt_focus_tab_multiline_and_escape_are_owned_by_the_dialog) or test(gui::render::tests::snippet_prompt_ctrl_enter_submits_once_and_preview_consumes_without_copy) or test(gui::render::tests::many_prompt_fields_and_feedback_keep_keyboard_and_footer_reachable_on_small_root) or test(gui::command_host::tests::prompt_request_opens_and_focuses_one_panel_from_hidden_or_visible_root) or test(gui::command_host::tests::prompt_completion_uses_captured_history_and_root_policy_once) or test(commands::handlers::snippet_run::tests) or test(commands::headless::snippet_headless_tests) or test(commands::bus::tests::invalid_snippet_run_uses_the_typed_handler_without_headless_fallback) or test(commands::handlers::headless_gui::tests::history_snippet_replay_uses_current_run_mode_and_captured_activation_context) or test(commands::handlers::headless_gui::tests::invalid_or_opaque_history_actions_never_infer_snippet_from_description) or test(dashboard::widgets::clipboard_snippets::tests) or test(dashboard::widgets::command_history::tests::history_pins_keep_opaque_clipboard_literals_and_resolve_snippets_by_alias) or test(gui::universal_action_catalog::tests::dashboard_snippet_catalog_uses_alias_route_and_keeps_clipboard_history_literal) or test(gui::universal_action_catalog::tests::preview_catalog_browsing_never_executes_an_action) or test(gui::universal_action_catalog::tests::unchanged_designer_reuses_action_catalog_until_results_change) or test(universal_actions::resolver::tests::opaque_literal_actions_do_not_gain_snippet_identity_from_description) or test(gui::tests::saved_pin_rebinds_only_by_canonical_alias_not_literal_body) or test(gui::radial_actions::tests::saved_radial_snippet_identity_uses_current_prompt_mode) or test(load_save_roundtrip) or test(search_returns_typed_snippet_run_action_without_body_payload) or test(list_command_returns_entries) or test(rm_command_returns_remove_actions) or test(search_run_action_keeps_newline_body_out_of_wire_payload) or test(search_add_returns_action) or test(launch_action_add_saves_snippet) or test(command_add_and_inline_edit_preserve_prompt_metadata_and_hidden_flag) or test(hidden_body_search_and_list_keep_alias_identity_without_leaking_body) or test(search_edit_returns_actions) or test(search_edit_inline_returns_add_action)'
  ```

- M6-A A27 diagnostics: the initial failing check used `Window::show`'s content response as the window bounds, yielding a zero-size rect; the test instrumentation now reads the actual egui area rect and observes a settled second frame. The settled 400×220 case exposed actual bounds `[-1, 0]–[400, 233]`, 13 px below the root; subtracting title/frame chrome corrected that. The next focused run showed 766.3 px of content in a 98.3 px body viewport, field 11 focused, but scroll offset remained zero even after advancing egui's animation clock. The nested preview ScrollArea had consumed the shared pending scroll target; deferring the target until after preview rendering fixed the owner. The final focused command passed 1/1 (4,885 skipped):

  ```text
  cargo nextest run --lib -E 'test(gui::render::tests::many_prompt_fields_and_feedback_keep_keyboard_and_footer_reachable_on_small_root)'
  ```

- M6-A final executable: `cargo build --locked --bin multi_launcher` passed (exit 0, 2m 53s). The fresh executable was launched on the interactive desktop with an isolated `target/prompt-fields-smoke` profile containing plain and prompted fixtures (two required fields, one optional multiline field, and a default). Computer Use returned the Multi Lnchr window, but capture failed with `window capture timed out: timed out waiting on channel`; refreshing the unique window and activating it before the one supported retry produced the same failure. Accessibility-only fallback returned `null`. No app input, typing, clipboard write, or native keyboard/focus/position/radial assertion was made. Only the verified smoke process was closed. Native smoke remains environment-blocked; the focused human script in §6 is the remaining verification path.
- M6-A fixture cleanup: the combined test run generated an untracked repository-root `clipboard_modifiers.json` containing built-in defaults; it was absent at the baseline and removed after provenance inspection. The new A27 fixture uses a retained temporary directory and absolute settings/actions paths. No unrelated fixtures were changed.
- M6-B P2 reviewer finding/remediation — egui's in-memory `TextEditState` can retain prompted text in widget undo history after the form's values are dropped. Prompt lifecycle now receives the app's existing context and clears only the active generation's field and preview edit states on replacement, successful copy, cancel, and shutdown; it surrenders focus only for IDs owned by that generation. Failed submit and invalid preview retain the active state. A retained-editor test checks the actual rendered authoring TextEdit cursor survives preview return. This is in-memory widget cleanup; no file persistence or output-history claim is made.
- M6-B P2 verification — `cargo fmt --all` and `git diff --check` passed. The planned combined 19-test command compiled after two test-only assertion fixes and passed 18/19; its sole failure was the retained-editor test's guessed, unsalted TextEdit ID. Capturing the actual response ID under `#[cfg(test)]` corrected the instrumentation; the exact retained-editor test then passed 1/1. The other 18 selected tests, including rendered cancel/replacement/copy/shutdown cleanup, failed-submit/invalid-preview retention, keyboard routing and A27, passed in the combined run. Existing unused-variable/unused-`Context::run` warnings in `src/gui/render.rs` remain.

  ```text
  cargo nextest run --lib -E 'test(gui::snippet_prompt_dialog::tests::) | test(gui::command_host::tests::prompt_) | test(gui::snippet_dialog::tests::preview_uses_unsaved_draft_without_mutating_editor_or_persisted_state) | test(gui::render::tests::snippet_prompt_) | test(gui::render::tests::many_prompt_fields_and_feedback_keep_keyboard_and_footer_reachable_on_small_root)'
  cargo nextest run --lib -E 'test(gui::snippet_dialog::tests::preview_uses_unsaved_draft_without_mutating_editor_or_persisted_state)'
  ```

- M6-B P2 status — source and focused evidence complete; parent commit and read-only reviewer reread pending. Native smoke remains explicitly environment-blocked.

> **Required directive:** Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.

---

## 1. Goal and user experience

Enable a user to create a normal `cs` snippet such as:

```text
Alias: ticketreply
Prompt for fields: ON

Hello {{name}},

Ticket {{ticket}} has been updated.
The next review is scheduled for {{date}}.

Thank you,
{{signature}}
```

Selecting `cs ticketreply` or `cs list` → `ticketreply` should show one compact **Fill Snippet** dialog containing inputs for `name`, `ticket`, `date`, and `signature`. Its preview updates as the user types. A valid **Copy** / **Ctrl+Enter** copies **only the fully resolved text** to the Windows clipboard and closes the form. **Escape** or **Cancel** closes without changing the clipboard.

An existing snippet **without** `Prompt for fields` enabled continues copying its literal text immediately, with no extra dialog, paste operation, or other workflow change—even if its literal text happens to contain `{{braces}}`.

This is a local, keyboard-first extension to the existing `cs` plugin. It is **not** a global hotstring or text-expansion service.

### 1.1 User-visible success criteria

1. Users enable prompting per snippet inside the existing `cs` editor; the setting is **off by default** for every existing snippet and every new plain snippet.
2. When prompting is enabled, `{{field_name}}` placeholders produce a single input per **distinct** name, ordered by first occurrence; repeated placeholders share one entered value.
3. Each field may have a custom label, default text, a required/optional flag, and a single-line or multiline input type.
4. A single dialog displays all fields with a live, plain-text rendered preview; Tab and Shift+Tab navigate; Ctrl+Enter submits; Escape cancels.
5. Submission replaces placeholders once, without recursive evaluation, and copies the result **only after successful validation**. The form stays open if validation or clipboard write fails.
6. No auto-paste, no globally registered hotstrings, no computed date/clipboard/system variables, and no conditional template language.
7. All identifiable snippet invocations—including launcher search, `cs list`, dashboard snippets, Universal Action catalog, and radial snippet sources—use the correct prompting behavior. Favorites/pins/history are covered under the explicit legacy-provenance rule in §5.6.
8. `snippets.json` remains compatible with old `[ { "alias": "...", "text": "..." } ]` content, existing `cs` add/edit/remove commands, atomic saves, external watcher reloads, and last-good-data safety.
9. Values supplied to a fill form are never saved as **form state**, action arguments, search text, favorites, ordinary action-history payloads, debug logs, or crash diagnostics. The final clipboard value **may** enter Windows / Multi Launcher clipboard history as requested.
10. The changes from **Plan A** (editor horizontal scrolling and hidden snippet previews) remain separate. No Plan A dependency is assumed.

### 1.2 Explicit non-goals

- Separate `cs` replacement plugin or duplicate snippet store.
- Global hotstrings, system-wide text expansion, typing into other apps, or automatic paste.
- Date picker, dropdown, checkbox, numeric-only field, per-field regex validation, or remembering the last filled values.
- Automatic `{{date}}`, `{{clipboard}}`, user-name variables, pipelines, conditionals, loops, nesting, expressions, or arbitrary scripting.
- Building a visual template designer, reorganizing snippet folders/categories, editor scrolling/hide-content controls (Plan A), new radial-menu editor infrastructure, broad Universal Actions expansion, or generic command-bus refactoring.
- Encryption or secret storage. Hiding content in Plan A would be visual masking only and is not part of this plan.

---

## 2. Source-backed architecture map

These are findings from the **provided ZIP**. Verify the equivalent code in the actual working branch before modifying it.

| Area | Existing owner / source location | Facts and consequence for Plan B |
|---|---|---|
| Snippet model and store | `src/plugins/snippets.rs` | `SnippetEntry` has `alias` and `text`; `load_snippets`, `append_snippet`, `replace_snippets`, `update_snippets`, and a `JsonWatcher` own atomic persisted mutations and last-good reload behavior. Extend, don't replace. |
| Query results | `src/plugins/snippets.rs`, `impl Plugin for SnippetsPlugin` | `cs` opens the existing editor; `cs list` and `cs <query>` return actions containing `clipboard:{snippet.text}`. A naive UI-only prompt would therefore miss the command path and other sources. |
| Editor | `src/gui/snippet_dialog.rs` | `SnippetDialog` already owns listing, add/edit, filter, validation and save; editor currently writes `SnippetEntry` directly. Keep authoring here and use separate transient fill state. |
| Command routing | `src/commands/model.rs`, `parser.rs`, `bus.rs`, `handlers/dialog_crop.rs`, `handlers/headless_gui.rs`, `headless.rs`, `host.rs` | A typed command bus distinguishes GUI-owned dialogs and headless work. Use a typed snippet invocation and consistent completion semantics, not a magic string intercepted ad hoc in one UI widget. |
| Launcher host and panel lifecycle | `src/gui/command_host.rs`, `src/gui/mod.rs`, `src/gui/render.rs`, `src/gui/actions.rs` | `LauncherApp` owns dialogs, focus, visibility, `activate_action`, and history/usage recording. New prompt state must enter this established lifecycle without racing launcher auto-hide or stealing focus. |
| Copy path / clipboard history | `src/actions/clipboard.rs`, `src/plugins/clipboard.rs` | Clipboard writes already use `arboard` and the clipboard plugin may later synchronize clipboard text into `clipboard_history.json`. Reuse copy behavior; do not add a second clipboard history subsystem. |
| Dashboard snippet widget | `src/dashboard/widgets/clipboard_snippets.rs` | Currently constructs `clipboard:{snippet.text}` directly, so the widget must switch to the canonical snippet-aware action. |
| Universal Action catalog | `src/gui/universal_action_catalog.rs`, `src/universal_actions/resolver.rs`, `providers.rs` | Catalog synthesizes snippet actions; resolver detects snippet targets by their `Snippet` description and label. Maintain primary and secondary behavior and do not accidentally classify generic clipboard actions as snippets. |
| Favorites and pins | `src/plugins/fav.rs`, `src/dashboard/widgets/snippets_favorites.rs`, `src/gui/mod.rs`, `src/dashboard/widgets/command_history.rs` | Saved favorites contain label/action/args and some old pins/history identify snippets only through literal `clipboard:` content. See §5.6 for safe compatibility. |
| Radial dynamic entries | `src/gui/radial_actions.rs`, `src/radial/dynamic.rs`, `src/radial/handoff.rs` | Snippets are a dynamic radial source; its action resolution must open a GUI prompt when appropriate and must not cause a radial/grid flash, lost dialog, or a mistaken deferred/headless copy. |
| Headless legacy action facade | `src/launcher/exec.rs` | `launch_action` parses a typed command and executes it headlessly; a prompted snippet cannot silently fall back to copying unresolved placeholders. |
| Repository process | `AGENTS.md`, `.codex/config.toml`, `.codex/agents/{planner,implementer,reviewer}.toml` | Existing roles and single-writer discipline apply. This plan uses targeted tests and checkpoint commits, not historical radial acceptance suites. |

**Compile-impact watch:** Search for all `SnippetEntry { ... }` literals, including `src/dashboard/data_cache.rs`, `src/gui/snippet_dialog.rs`, snippet tests, and any newer working-tree occurrences. New persistent fields require updating these construction sites without weakening their tests.

### 2.1 Execution ownership principle

There must be **one domain-level decision** for a snippet invocation: resolve the alias against the current snippet model, then either **copy immediately** (non-prompted) or **open a prompt session** (prompted). The template parser/renderer must be pure Rust and shared between editor diagnostics, fill preview, and final submission. GUI widgets should not independently implement or reimplement placeholder substitution.

For new snippet-origin commands, favor an alias-based identity such as `snippet:run:<alias>` mapped to a **typed command variant** (e.g., `StorageCommand::SnippetRun { alias }`, or a narrower snippet command if the current code warrants it). The implementer may choose the existing project's most idiomatic variant, but may **not** turn the action string into rendered snippet text, put filled values in command arguments, or bypass the command bus to show a dialog.

If alias values can contain command delimiters, command parsing must use a lossless encoding or an unambiguous suffix; do not introduce a lossy split or fail to parse previously valid aliases. Treat aliases as data, not executable source.

### 2.2 Scope-safe treatment of earlier action identities

The ordinary historical `clipboard:<literal text>` command must still mean **copy that literal text**, because other features rely on it. An old saved favorite or pin that contains only that string may have **lost its snippet provenance**. Do not globally reinterpret all `clipboard:<text>` actions as prompted snippets merely because their text resembles a template.

For **newly created** snippet-origin actions, favorites, pins, or radial bindings, use the stable alias-aware invocation so prompted/plain transitions resolve correctly. For clearly identifiable legacy snippet-origin records, provide a deterministic, non-destructive rebind/migration if their identity is provable; otherwise leave them unchanged and **document how to re-save/rebind the item**. An opaque literal-copy favorite is not proof of snippet identity. Never silently rewrite or hijack unrelated clipboard commands.

---

## 3. Detailed behavior contract

### 3.1 Template grammar

Define one strict, documented initial syntax for prompted snippets:

- Placeholder: `{{identifier}}`, where `identifier` is an ASCII letter or underscore followed by ASCII letters, digits, or underscores; identifiers are **case-sensitive** (`{{Name}}` and `{{name}}` are different fields). Custom display labels permit human-friendly spaces/punctuation without complicating the placeholder grammar.
- Recommended literal escape convention: `\{{name}}` renders **literally** as `{{name}}` in prompted mode; the escape backslash is consumed for that opening marker. A standalone normal backslash is not an escape and is preserved. Document exact escaping semantics with tests, including two consecutive backslashes and literal `\{{` sequences. Do not apply escape processing at all to unprompted snippets.
- Field scanning is left-to-right through the original UTF-8 string. Field discovery preserves first-seen order. Duplicate names produce one configured field and one form input.
- Reject in *prompted mode* missing closing `}}`, empty `{{}}`, invalid identifiers, unexpected bare `}}`, and similarly malformed placeholder-like constructs with a useful position/field-specific error. Never attempt best-effort partial substitution.
- `{{date}}` is **not** computed; it prompts for literal text. Other unknown/unsupported dynamic variable conventions are treated as ordinary literal text unless they constitute malformed `{{...}}` syntax.
- Values inserted by a user are **opaque strings**: don't recursively expand `{{...}}` inside submitted text, automatically trim the output, normalize line endings, or reinterpret escapes inside values.
- Preserve multiline text, UTF-8, and existing literal text unchanged apart from defined placeholder/escape replacement. For required-field checks, trimming may be used **only for validation**, not to rewrite the user's submitted value.
- Empty optional input renders as an empty string; an explicitly empty required input (including whitespace-only) blocks submission. Defaults prepopulate fields and can be edited or cleared.
- For a prompted snippet with **zero real fields**, the editor should offer a clear validation message and refuse to enable/save it as prompted; invalid externally edited data must remain loadable (valid JSON) but should fail safely when invoked, never silently copy an unresolved template.
- Prevent UI hangs from pathological inputs: choose modest, documented limits for field count and length only as needed for practical safety, with friendly errors; do not add speculative heavy parsing infrastructure.

Suggested pure API *shape* (illustrative, not mandatory names):

```rust
parse_template(text: &str) -> Result<ParsedTemplate, TemplateError>
discover_fields(parsed: &ParsedTemplate) -> Vec<FieldKey>
render_template(parsed: &ParsedTemplate, filled: &FieldValues) -> Result<String, FieldErrors>
```

### 3.2 Persisted field configuration

Each `SnippetEntry` must retain `alias` and `text` and may add opt-in metadata resembling:

```json
{
  "alias": "ticketreply",
  "text": "Hello {{name}}, ticket {{ticket}} is updated.",
  "prompt_for_fields": true,
  "fields": [
    { "name": "name", "label": "Name", "default": "", "required": true, "input_kind": "single_line" },
    { "name": "ticket", "label": "Ticket number", "default": "INC-", "required": true, "input_kind": "single_line" }
  ]
}
```

The JSON is **illustrative**; exact field names/types are implementer-owned, subject to the invariants:

1. Missing opt-in flag deserializes as false; missing fields deserialize as empty. Legacy JSON remains readable, including missing or empty files.
2. Saved simple snippets may continue serializing with just `alias` and `text` if compatible with existing project patterns. Never mass-rewrite user files on mere read.
3. Field declarations identify the placeholder key, hold label/default/required/input-kind, and persist only those **configured** defaults; they do **not** store values entered during a fill dialog.
4. Discovery order in the snippet text is authoritative initially; existing metadata is reconciled by **key**, preserving labels/defaults/required/input-kind for fields that remain, adding new fields with sensible defaults, and removing orphan metadata only on an explicit committed edit.
5. New fields default to **required** and **single-line**; label defaults to a readable version of the key; default text begins empty. Optional fields may have an empty default. Existing metadata is never discarded during an ordinary text update merely because the caller only supplies alias/text.
   A missing persisted label may deserialize as empty; reconciliation and display use the readable key fallback so partial metadata never produces a blank field label.
6. Respect existing atomic transaction, no-op-save behavior, watchers, version counters, snapshot update semantics, last-good-data on malformed JSON, and failure rollback. Do not introduce a second JSON file.
7. A normal `cs add` creates a non-prompted snippet. A normal `cs edit <alias> <text>` on an existing prompted snippet preserves prompting and compatible field settings, reconciles newly valid fields, and rejects malformed prompted templates without corrupting persistent data. Do not silently disable prompting or delete field configuration to make old commands work.
8. Editing via UI changes a **draft**, not the persisted entry, until Save. Cancel discards draft changes, even after a failed validation.

### 3.3 Prompt UI and keyboard behavior

- Open one dialog entitled `Fill Snippet — <alias>`, with all detected fields in logical order; no one-field-at-a-time wizard.
- Single-line fields use a single-line editor; multiline fields use a bounded multiline editor. Dialog stays within the visible monitor and scrolls the form/preview as needed for many fields or small screens.
- Initial focus goes to the first **required** field (if none, first available input); Tab/Shift+Tab move between inputs and buttons in predictable order.
- Ctrl+Enter submits regardless of focused input; plain Enter inside a multiline input inserts a newline rather than unexpectedly submitting. Escape cancels. Visible Copy/Cancel controls and window X mirror keyboard semantics.
- Live preview re-renders from the **same pure parser/renderer** as final output, with responsive local updates. Show errors inline near offending inputs and optionally a concise form-level error; errors do not replace entered values.
- On valid Copy, write through the existing clipboard service; if the write succeeds, close/clear the transient session and optionally issue a generic success toast. If it fails, keep the session open, show a safe error, and retain entered values.
- Do not auto-type into the formerly active app, synthesize paste keystrokes, or accidentally treat a second hotkey gesture as a second submission. No duplicate writes from a single gesture.
- Preserve established launcher-window position, panel stack, restore/focus policy, action sheet behavior and radial handoff. Opening a prompt from a hidden/root-dismissed state must present a usable, focused prompt with no launcher flicker.
- **Snapshot consistency:** capture an immutable snippet template/config when the prompt opens. At submit, verify the snippet has not been deleted/replaced since opening (compare relevant revision/content or a suitable snapshot identity). If changed, show a safe stale-template notice and do not copy stale content; keep the draft until the user cancels/reopens.

### 3.4 Preview-only workflow

From the **existing CS editor**, users should be able to choose **Test / Preview** for their current unsaved draft. Reuse the fill form and renderer in a **preview-only** mode, but do not write the clipboard, record launcher history/usage, or save snippet metadata. Let the user see the rendered output, change test inputs, then close preview and return to the exact unsaved editor draft. Explicitly label preview mode and provide no ambiguous “Copy” action in that mode.

### 3.5 History, privacy, and error conditions

- Prompted **invocation** and **successful completion** are distinct events. Cancel and validation failures must not be recorded as successful executions, success toasts, or clipboard changes. If the command framework historically records the opening action, refine the policy for this command only and attribute exactly one successful completion after the clipboard write.
- Store only safe action identity (alias/command) and non-sensitive source/query in action history; never save completed output in the `Action.action`, `Action.args`, favorites, pin definitions, radial persisted definitions, diagnostics, or tracing fields. Ensure the user-entered values are **not** included in an error string.
- **Exception explicitly approved:** the completed output may be observed by the existing clipboard plugin and stored in `clipboard_history.json` because normal clipboard history behavior is preserved. Do not claim that copied text is secret or never stored.
- No persistent recent-values cache. Clear temporary form values on completion, explicit cancel, closing the window, and terminal application cleanup. Configured editor defaults are intentionally persistent.
- Missing alias, duplicate/ambiguous alias identities, malformed externally edited template, stale session, field validation failure, and clipboard failure produce readable feedback without copying raw `{{...}}` tokens.
- Don't block unrelated snippets due to a single invalid prompted template in otherwise valid JSON; still retain the existing last-good-file behavior for structurally invalid JSON.

---

## 4. Milestones and commit map

The project explicitly prefers **active checkpoint commits**, not one giant end-of-feature commit. The stage labels below are proposed **coherent implementation checkpoints**. Merge an extremely small adjacent checkpoint only with a written explanation; do not drop meaningful boundaries simply to shorten Git history.

| Stage | Coherent unit of work | Suggested commit subject |
|---|---|---|
| **M1-A** | Backward-compatible snippet persistence model | `feat(cs): [M1-A] add opt-in prompted snippet metadata` |
| **M1-B** | Metadata-preserving mutations and reload tests | `fix(cs): [M1-B] preserve prompt configuration across snippet updates` |
| **M2-A** | Pure strict template parser / field discovery | `feat(cs): [M2-A] parse and discover prompted fields` |
| **M2-B** | Pure rendering, validation, error tests | `feat(cs): [M2-B] render validated snippet templates` |
| **M3-A** | Canonical alias-aware action and typed command route | `feat(cs): [M3-A] route snippet invocation through typed commands` |
| **M3-B** | Transient execution/preview session state and lifecycle | `feat(cs): [M3-B] manage transient prompted snippet sessions` |
| **M3-C** | Keyboard-first fill dialog and successful copy | `feat(cs): [M3-C] add prompted snippet fill dialog` |
| **M4-A** | Existing editor's opt-in field metadata controls | `feat(cs): [M4-A] configure prompted fields in snippets editor` |
| **M4-B** | Unsaved-draft test/preview, editor error UX | `feat(cs): [M4-B] preview prompted snippets without copying` |
| **M5-A** | Dashboard and Universal Action snippet entrypoints | `feat(cs): [M5-A] unify dashboard and catalog snippet activation` |
| **M5-B** | Favorites, pins/history, radial and non-GUI compatibility | `fix(cs): [M5-B] preserve snippet identity across saved action surfaces` |
| **M6-A** | Focused cross-surface regression tests + docs | `test(cs): [M6-A] verify prompted snippet flows and legacy behavior` |

**M6-B is a review/closeout gate**, not a forced content-free commit. If it uncovers real bugs, use one or more descriptive remediation commits such as `fix(cs): [M6-B] correct prompt dialog cancellation and focus`. Do not create an empty WIP/checkpoint commit purely to satisfy an ID.

> **Sequencing:** M1 → M2 → M3 → M4 → M5 → M6. Implementation edits to shared files are sequential (one writer). Planner/reviewer may inspect read-only in parallel where useful.

---

## 5. Detailed executable milestone handoffs

Each stage describes **objective; ownership; required work; tests/evidence; invariants; done criteria**. The implementer owns idiomatic Rust details that do not affect the behavioral contract.

### M1 — Persisted snippet configuration and safe mutations

#### M1-A — Add compatible data model

**Owner and likely files:** `src/plugins/snippets.rs`; small mechanical fixture fixes in `src/dashboard/data_cache.rs` and directly affected tests.

**Required tasks**

1. Introduce an opt-in prompted flag and a list of per-field definitions on `SnippetEntry` using additive Serde defaults. Use typed input kind rather than arbitrary strings inside runtime logic; ensure serde defaults for new values are explicit and tested.
2. Preserve the old public `alias` and `text` fields. Keep existing plain records usable without a conversion command, file migration, or startup rewrite.
3. Define an idiomatic field key, label, default text, required flag, and `single_line`/`multiline` input mode. Ensure equality semantics consider metadata so versioned snapshots notice meaningful configuration changes.
4. Update all compile-affected struct literals/test helpers intentionally; do not hide missing fields through broad test changes that remove legitimate assertions.
5. Add minimal persisted JSON fixtures (legacy, explicit opt-out, prompted, optional metadata, unexpected unknown fields as currently supported). Verify de/serialization and that defaults do not turn an existing snippet into a prompt.

**Evidence:** Focused model/persistence tests demonstrating legacy deserialization and new model roundtrips; no user file rewritten as a side effect of read.  
**Invariant:** An existing simple snippet still copies the exact same literal value.  
**Done:** New metadata types exist, legacy records load, tests compile semantically, no behavior route has changed yet.  
**Commit:** `feat(cs): [M1-A] add opt-in prompted snippet metadata`

#### M1-B — Preserve metadata and atomic reload behavior

**Owner and likely files:** `src/plugins/snippets.rs`, relevant `src/gui/snippet_dialog.rs` mutation seams, focused store/watch tests.

**Required tasks**

1. Audit `append_snippet`, `replace_snippets`, `update_snippets`, UI add/edit, and `src/actions/snippets.rs` wrappers. An alias/text-only update must not accidentally reset `prompt_for_fields` or wipe field definitions on an existing entry.
2. Introduce a narrowly owned normalization/reconciliation function for metadata keyed by placeholder name; it will be used by the editor and command-path changes as M2 becomes available. Avoid implementing a second unsynchronized source of truth in GUI state.
3. Ensure new `cs add` creates an unprompted snippet; updates to an existing prompted alias leave opted-in status intact. Once the parser is ready in M2, reject invalid changed prompted templates atomically; until then, prevent information loss rather than inventing placeholder inference in this stage.
4. Preserve atomic save and watcher invariants: successful save publishes one coherent snapshot, watcher sees metadata changes, failed save preserves last good model/file, malformed external JSON remains read-only in editor and does not overwrite user data.
5. Add tests for same-content no-op, metadata-only update, successful watcher reload, failed save, external edits and concurrent mutations. Keep the current single-data-directory ownership rule untouched.

**Evidence:** Focused store tests, version bumps, unchanged malformed-file behavior.  
**Invariant:** Metadata cannot disappear because an older command supplies only alias/text.  
**Done:** Store mutation boundaries are safe and ready to consume M2 validation.  
**Commit:** `fix(cs): [M1-B] preserve prompt configuration across snippet updates`

### M2 — Shared template parser and renderer

#### M2-A — Strict parsing and field discovery

**Owner and likely files:** new small module under `src/plugins/` or a focused `src/snippets/` domain module; export only useful APIs to `src/plugins/snippets.rs` and GUI.

**Required tasks**

1. Implement the specified `{{key}}` parser over UTF-8 safely without slicing at invalid indices or leaking parser diagnostics containing arbitrary template content.
2. Support the exact escape convention and literal segments; decide and document handling of consecutive backslashes consistently, then cover it with tests.
3. Reject malformed `{{...}}` constructs with useful span/field information. Do not accept zero-length field names, unclosed braces, nested constructs, invalid characters, or ambiguous adjacent tokens.
4. Discover unique keys in first-occurrence order and retain reusable parsed segments for rendering. Avoid regex replacements that reparse text or accidentally substitute inside user-provided values.
5. Include a reconcile-by-key operation or interface shape to preserve existing authored field settings as text is edited. Do not persist the derived field list until editor Save.
6. Keep search/hot-path cheap: parse on authoring/opening a prompt, not for every global query or every frame that renders the main launcher.

**Tests:** Literal text; one and many fields; repeated keys; adjacent placeholders; name/Name distinction; escaped syntax; literal backslashes; ASCII identifiers; UTF-8 around tokens; malformed starts/ends; zero fields; first-appearance order.  
**Done:** Deterministic parser behavior specified by tests; pure operation with no UI/clipboard/filesystem dependencies.  
**Commit:** `feat(cs): [M2-A] parse and discover prompted fields`

#### M2-B — Validated rendering and safe failures

**Owner and likely files:** M2 parser module, pure tests.

**Required tasks**

1. Render template segments from an input map keyed by placeholder name. Insert each field value exactly as entered and treat it as literal content even if it contains `{{...}}`.
2. For required fields, reject empty or whitespace-only values; for optional fields insert empty text when blank. Prepopulate configured defaults at form creation (not in pure substitution).
3. Require all fields referenced by parsed template to have a known value after validation; prevent partially substituted strings.
4. Return structured per-field and template errors instead of using panics, substring-based heuristics, or raw text in log messages.
5. Confirm escaping, multiline, Unicode, repeated-key replacement and no auto-date expansion. Invalid/missing values never cause clipboard writes because the engine has no clipboard dependency.
6. Validate the design with incremental live-preview use in mind (parse once per draft revision, render from transient field values).

**Tests:** Default provided and overridden; optional blank; required blank/whitespace; missing key; duplicate fields; multiline; Unicode; value containing `{{key}}`; malformed template; full output equality; no recursion.  
**Done:** One pure, testable renderer is authoritative for both preview and Copy.  
**Commit:** `feat(cs): [M2-B] render validated snippet templates`

### M3 — Typed dispatch and native prompted fill UI

#### M3-A — Canonical snippet invocation route

**Owner and likely files:** `src/plugins/snippets.rs`, `src/commands/{model,parser,bus,headless,host}.rs`, relevant `src/commands/handlers/*.rs`, targeted test-host implementations.

**Required tasks**

1. Establish a canonical alias-based snippet execution action for new snippet-origin results and route it through a typed command (e.g. `snippet:run:<alias>` → `StorageCommand::SnippetRun`). Keep legacy `clipboard:<literal>` command working as before.
2. Have one execution owner resolve the **current** entry from the store/snapshot by alias, rather than relying on cached snippet text supplied in the action payload. Missing or ambiguous alias produces a safe, explicit error.
3. When an entry is unprompted, immediately copy literal text through the established clipboard path—**no form or extra UI**—with standard success/failure semantics. If a prompted entry is malformed, do **not** copy raw placeholder text.
4. When prompted, return a GUI-owned open-prompt intent/host call, not a headless fake-success. Preserve `CommandOutcome` policy: opening is not a completed successful copy. An execution arriving from a headless-only caller must return an understandable “requires launcher UI” error without copying raw template text.
5. Extend existing command parser, matching tests, host fakes, dispatch tests, command-kind telemetry if required, and Universal Action compatibility without sprawling into unrelated command refactors.
6. Capture safe invocation metadata (alias, source, original query/action identifier) for attribution after successful Copy. Do not carry values inside action IDs or histories.
7. Preserve behavior of `cs`, `cs add`, `cs edit`, `cs rm`, `cs list`; `cs` by itself still launches the Snippets editor.

**Tests:** Exact parse/dispatch; selected alias; missing alias; plain fast path; prompted route; invalid prompted rejection; headless outcome; legacy clipboard command unaffected; no bogus history on opening.  
**Done:** The typed pipeline knows whether it must copy or prompt and does so without leakage.  
**Commit:** `feat(cs): [M3-A] route snippet invocation through typed commands`

#### M3-B — Session state, lifecycle and completion semantics

**Owner and likely files:** `src/gui/mod.rs`, `src/gui/command_host.rs`, potentially a new `src/gui/snippet_prompt_dialog.rs`, `src/gui/actions.rs` and established render/panel hooks.

**Required tasks**

1. Create an ephemeral prompt session with snapshot of template/config, alias, parsed fields, mode (`Execute` vs `PreviewOnly`), transient field values, validation state, and minimal non-sensitive invocation context.
2. Integrate it with `LauncherApp` and the panel/window state model. Opening from a hidden launcher, grid, dashboard, favorites, action sheet, or radial should result in a usable focused prompt; root launcher visibility and position must follow established policy.
3. Explicitly handle initial focus, closing via Escape, Cancel, X, panel close, reopening, app exit, and repeated invocation while an existing prompt is active; no stale input values may carry into a new session.
4. Implement a **single final-submit path**: validate → check session/template freshness → write clipboard → close and emit safe history/success metadata only on success. For a clipboard failure, keep session and fields; for stale template, block and explain; for validation failure, keep draft and show inline errors.
5. Ensure exactly one clipboard write/history record per completion. Cancel, preview-only, failed validation and failed clipboard writes do not create a successful execution history entry.
6. Do not leak field values in logs, `Action`, queries, usage keys, temporary files, or panic/test traces. Existing clipboard history synchronization remains enabled.
7. Add lifecycle state tests around transitions and proof that a filled value is not retained after successful completion/cancel.

**Tests:** Opening from hidden/visible root; initial snapshot; second opening; cancel; submit once; validation failure; clipboard backend failure; stale-file revision; history/usage policy and privacy.  
**Done:** All state transitions are deterministic and safe; no UI layout work is necessary to prove the core session logic.  
**Commit:** `feat(cs): [M3-B] manage transient prompted snippet sessions`

#### M3-C — Compact fill dialog and keyboard shortcuts

**Owner and likely files:** new `src/gui/snippet_prompt_dialog.rs` (if appropriate), `src/gui/render.rs`, `src/gui/mod.rs`, focus/panel tests.

**Required tasks**

1. Render a small `egui` dialog showing alias, all inputs in discovery order, labels, required indicators and single-line/multiline editors; support vertical scrolling and reasonable maximum size.
2. Initialize inputs from configured defaults, focus first required field (otherwise first field), and support Tab/Shift+Tab consistently with normal egui conventions.
3. Add a rendered plain-text live preview using M2. Preview should be scrollable/read-only, should preserve newlines, and should show concise field validation issues without silently hiding them.
4. Implement Ctrl+Enter submission from any input; plain Enter in a multiline field must remain a newline; Escape, Cancel, and X all cancel with no clipboard write.
5. On success, close and return focus/visibility to existing app conventions. On error, preserve entered text and focus; display error text that does not reproduce sensitive input.
6. Integrate with `restore_for_new_launcher_interaction`, panel stack and radial handoff safely; do not change the existing launcher hotkey tap/hold or forced-root-show logic except a narrowly proven correctness fix.
7. Check small display sizes, high DPI, long field labels, long multiline values and many inputs. Prefer simple UI rather than a new designer or complex animation.

**Tests/evidence:** Focused keyboard interaction tests where available, small UI state tests, manual on-Windows check for Ctrl+Enter/Escape/focus and no launcher flash.  
**Done:** End-to-end prompted `cs <alias>` flow is usable with keyboard and mouse; ordinary snippets remain immediate.  
**Commit:** `feat(cs): [M3-C] add prompted snippet fill dialog`

### M4 — Edit prompted snippets in the existing CS editor

#### M4-A — Opt-in and per-field configuration

**Owner and likely files:** `src/gui/snippet_dialog.rs` and shared field-reconciliation model.

**Required tasks**

1. Extend the existing Add/Edit view with `Prompt for fields` checkbox (unchecked by default on new/legacy snippets). This is the **only** template opt-in mechanism; don't implicitly prompt when a plain snippet contains braces.
2. Display a clear syntax hint and at least one example, including `\{{...}}` literal escaping and a note that `{{date}}` is a prompted input rather than a computed date.
3. Discover fields from unsaved text and display a compact editable table/list: placeholder key (read-only identifier), editable label, editable default, required/optional checkbox, single-line/multiline selection. Keep first-occurrence ordering.
4. On typing changes, reconcile by field key so labels, defaults and optional choices persist for unchanged keys. If keys disappear, do not discard saved settings until Save; show the draft state accurately and avoid silently mutating an existing persisted entry.
5. On Save, validate enabled templates and metadata; block malformed/empty templates, show precise inline feedback, and keep the unsaved draft intact. When prompting is off, preserve historic validation and literal text behavior; do not enforce placeholder grammar.
6. Preserve original alias editing, add/edit/remove commands and atomically persisted updates. Save should call existing mutation boundary rather than writing JSON directly from UI.
7. Keep Plan A's horizontal scrolling/hide-content features outside scope. If Plan A is already implemented on the active branch, integrate without reverting it; otherwise do not use this feature to refactor unrelated listing UI.

**Tests:** Off by default; legacy load/edit/save; discovery; field type/default/required edits; rename/remove/add field; malformed template; editing ordinary snippets; cancelled draft; save failure retains original; metadata preserved on `cs edit` command.  
**Done:** Users can fully author prompted snippets in the current editor without new dialogs unrelated to field filling.  
**Commit:** `feat(cs): [M4-A] configure prompted fields in snippets editor`

#### M4-B — Preview unsaved drafts safely

**Owner and likely files:** `src/gui/snippet_dialog.rs`, session model and fill preview UI.

**Required tasks**

1. Add a `Test / Preview` control while editing a prompted snippet; validate current draft and open the existing fill UI in **PreviewOnly** mode using an immutable copy of that draft.
2. Permit trial values and show resolved output. Preview action must not copy, paste, change histories/usage, commit file changes, or replace the current editor text.
3. Return from preview to exactly the same unsaved editor draft, with its field settings and cursor/scroll position where feasible; cancellation from preview never closes or resets the editor.
4. Show parse/field errors in the authoring editor if preview cannot start; provide clear distinction between unprompted literal text and prompted templates.
5. Keep preview-only mode separate from execution so no clipboard-write branch is reachable through a button/shortcut in preview mode.

**Tests:** Preview output accuracy; unsaved draft preserved; no file/clipboard/history mutations; cancel and re-open; correct edit state after a preview error.  
**Done:** Editor test preview is safe, clear and non-destructive.  
**Commit:** `feat(cs): [M4-B] preview prompted snippets without copying`

### M5 — All relevant snippet entry points and compatibility

#### M5-A — Dashboard and Universal Action catalog

**Owner and likely files:** `src/dashboard/widgets/clipboard_snippets.rs`, `src/gui/universal_action_catalog.rs`, `src/universal_actions/{resolver,providers}.rs` where needed.

**Required tasks**

1. Replace direct `clipboard:{snippet.text}` generation on active snippet-aware surfaces with the canonical alias-aware action; preserve labels, context menus/Action Sheet secondary Edit/Remove actions, filtering, shortcuts and read-only listing behavior.
2. Ensure the dashboard clipboard **history** section remains literal-copy history; only its **Snippets** section should use snippet invocation. Avoid treating a clipboard-history value that happens to contain `{{...}}` as a prompted snippet.
3. Ensure Universal Action resolution still identifies `ActionTarget::Snippet { alias }` and secondary actions for snippet entries; don't change non-snippet action identification. Catalog actions should not embed snippet template or filled output.
4. Audit search result collections, launcher action cache, and read-only search used by the Radial Designer; avoid reparsing prompts during every frame, prevent long-running blocking UI work.
5. Add tests for normal and prompted snippets via both widgets/catalog and for generic clipboard content that resembles a snippet.

**Done:** Every **live, identifiable** snippet in these surfaces executes through the same canonical command and correct UI/copy branch.  
**Commit:** `feat(cs): [M5-A] unify dashboard and catalog snippet activation`

#### M5-B — Favorites, history, pinned actions, radial and headless protection

**Owner and likely files:** `src/plugins/fav.rs`, `src/dashboard/widgets/{snippets_favorites,command_history}.rs`, `src/gui/mod.rs`, `src/gui/radial_actions.rs`, applicable `src/radial/*` handoff code, `src/actions/history.rs`, `src/launcher/exec.rs`, and focused tests only where genuine changes are required.

**Required tasks**

1. Trace all creation/activation of snippet-origin favorites, history pins, generic pin entries, radial dynamic Snippets sources and exact-action bindings. Newly saved snippet-origin actions must persist the **alias-aware** command instead of a template-content copy command.
2. Confirm prompted activation through `cs list`, dashboard, favorites created from the new canonical action, radial dynamic Snippets and Action Sheet does **not** copy raw `{{...}}` template text or inadvertently dismiss the pending GUI.
3. When a snippet previously plain becomes prompted, **new alias-aware actions already saved before that change** must resolve current opt-in configuration at execution and open the form. When it becomes plain again, the same alias-aware action must return to immediate copy.
4. Address pre-existing `clipboard:<literal>` favorites/pins/history **safely**: identify which records have provable snippet provenance, if any, and offer a narrowly scoped deterministic conversion where that information exists. Do **not** rewrite ambiguous clipboard-only actions using fuzzy content equality. Leave opaque literal-copy commands functioning as explicit literal copies and document a simple rebind/re-save path for old shortcuts. Avoid silent or lossy migration of `fav.json`/pin/history data.
5. Audit direct headless `launch_action` and `run_fav` paths, old `history:<index>` replay and MkMacro-executed actions. New prompted command in headless context must fail gracefully with an explicit GUI-needed result; **never** substitute unresolved text, start an invisible prompt, or claim successful copy. Plain snippet and old clipboard headless behavior must continue working.
6. Verify radial/pinned selection uses current snippet alias and does not bind the wrong item after a reload or removal. If radial intentionally freezes an invocation identity, preserve its dispatch identity and defer only the UI interaction through approved radial/handoff pathways.
7. Ensure history and usage records do not store filled values; record only successful prompted completion once and retain existing plain-snippet expected policy. Unresolved `{{...}}` text must never reach clipboard via a true prompted snippet action.
8. Document the exact boundary between identifiable snippet-origin entry points (supported) and opaque legacy raw-text actions (literal-copy by definition), with concrete instructions for re-saving a favorite/pin/radial cell as an alias-aware snippet action.

**Tests:** New snippet favorite; convert plain↔prompted after saving favorite; dashboard favorite; persisted pin; history replay; radial dynamic source; generic clipboard favorite unaffected; multiple snippets with identical literal content; renamed/deleted alias; headless prompted action failure; no duplicate history or launcher flash.  
**Done:** No active snippet-aware entry point silently bypasses prompting; ambiguous old raw actions are not hijacked and migration limitations are explained.  
**Commit:** `fix(cs): [M5-B] preserve snippet identity across saved action surfaces`

### M6 — Targeted verification, documentation and review

#### M6-A — Verify and document the completed behavior

**Owner and likely files:** directly related tests, `README.md` snippet command/help section and focused in-product hints.

**Required tasks**

1. Review changed modules and add/update **targeted** end-to-end dispatch tests for prompted versus plain, editor Save/Cancel, clipboard success/failure, safe history, persisted data compatibility and relevant cross-surface cases. Reuse tests from earlier checkpoints; do not duplicate them under new names without added coverage.
2. Confirm latest source remains functional with Windows-only GUI constraints. Prefer local unit tests for parser/store/state and small integration tests for command bus. Run one meaningful targeted final verification pass on Windows, not every repository test by default.
3. Update user-facing docs: how to enable prompt in `cs`, example `{{name}}`, required/optional/default/multiline behavior, `Ctrl+Enter`, `Escape`, copy-only behavior, privacy clipboard history exception, escaped literal placeholders, test-preview mode, and legacy favorite rebind note.
4. Check that normal `cs`, `cs add`, `cs edit`, `cs rm`, `cs list`, file reload, plain snippet search and action-sheet edit/remove haven't regressed.
5. Compare result to the full requirements and explicit non-goals. Inspect `git diff --check` and changed file list; report exact commands/results (do not claim tests passed if not run).

**Done:** Acceptance checklist §6 is supported by focused evidence; docs reflect actual behavior and limitations.  
**Commit:** `test(cs): [M6-A] verify prompted snippet flows and legacy behavior`

#### M6-B — Read-only reviewer gate and remediation only if needed

1. Request a **read-only reviewer** agent to inspect the actual diff versus the approved requirements and the baseline. Review parser correctness, serialization, privacy/logging, command sources, modal ownership, keyboard focus and history semantics.
2. Classify findings by severity. Any defect is corrected by a **single write-owning implementer** in a separate bounded follow-up; use `fix(cs): [M6-B] <specific correction>` (and subsequent descriptive stage suffix if more than one independently meaningful fix).
3. Rerun only the tests invalidated by each change; for materially changed core behavior, rerun the final targeted acceptance slice. Don't automatically restart a broad suite after each correction.
4. Mark only proven milestones complete and provide final report with commit hashes, tests, scope, explicit caveats, and no silent squashing/rebasing.

**Done:** No unresolved high-severity contract violations; all supported requirements implemented; incomplete or environment-blocked checks reported explicitly. **Do not create an empty M6-B commit.**

---

## 6. Focused acceptance checklist

Acceptance IDs let the orchestrator and reviewer refer to observable behavior rather than internal helper names.

| ID | Test / scenario | Required observation |
|---|---|---|
| **A01** | Load legacy `snippets.json` with only alias/text | Loads unchanged; all prompting disabled by default |
| **A02** | Execute a legacy snippet with literal `{{name}}` | Copies literal braces immediately, **no dialog** |
| **A03** | Save prompted `Hi {{name}}` | `cs name` selection opens one dialog with Name field |
| **A04** | Template `{{name}}-{{name}}` | One input, both occurrences identical in output |
| **A05** | Template `{{z}} {{a}} {{z}}` | Inputs in `z`, `a` order |
| **A06** | Field label/default/required/type configured | Roundtrips JSON and appears correctly in editor/fill UI |
| **A07** | Multiline typed value + Unicode characters | Exact bytes/newlines represented in preview and copied output |
| **A08** | Escaped `\{{name}}` | Displays literal `{{name}}` without prompting for it |
| **A09** | Required field blank or spaces | Submit blocked; inline error; clipboard unchanged |
| **A10** | Optional field blank | Substitution is empty, other content preserved |
| **A11** | Malformed `{{` / `{{}}` / invalid field | Editor Save and execution show error, no partial clipboard copy |
| **A12** | Ctrl+Enter from single-/multiline input | Exactly one successful submission; plain Enter inserts newline in multiline |
| **A13** | Escape, Cancel, title X | No clipboard changes, no successful execution history; values discarded |
| **A14** | Clipboard write fails | Dialog remains open with values; safe error; no success record |
| **A15** | Snippet modified externally while fill open | Stale form blocks copying; last-good reload not corrupted |
| **A16** | Editor Test/Preview on unsaved draft | Correct rendered output; no clipboard/save/history mutation |
| **A17** | Use `cs`, `cs add`, `cs edit`, `cs rm`, `cs list` | Existing command semantics preserved; prompted metadata retained on text edit |
| **A18** | Dashboard Snippets vs clipboard-history rows | Snippets prompt where opted-in; clipboard history always copies literal entry |
| **A19** | Snippet via Action Sheet/catalog | Primary action prompts correctly; Edit/Remove still work |
| **A20** | Newly saved favorite/pin/radial snippet action | Alias-aware action resolves current enabled/disabled state |
| **A21** | Existing ambiguous literal clipboard favorite | Still performs literal copy; not hijacked or silently migrated |
| **A22** | Headless invocation of prompted alias | Clear UI-required failure, never raw template copy; plain headless still works |
| **A23** | Search/history/favorites/log review | No filled values serialized into action identity, args, event logs, success messages or ordinary action history |
| **A24** | Clipboard history after successful Copy | Normal existing clipboard-history capture remains possible (intentional exception) |
| **A25** | Save failure / corrupted external JSON | No user data overwritten; last-good protections preserved |
| **A26** | Reopen form, reopen editor, repeated hotkey | No old values, duplicate submissions, launcher position regression or incorrect focus |
| **A27** | Many fields / long labels / small screen | All fields/controls remain usable by keyboard and scrolling |

**Manual Windows smoke script:** (1) create plain snippet; (2) create prompted snippet with two required fields, one multiline optional, and a default; (3) run via `cs` and `cs list`; (4) preview and cancel; (5) copy completed text and inspect clipboard; (6) run via dashboard and an alias-aware favorite/radial action; (7) change opt-in and repeat; (8) test malformed template, external JSON reload, missing alias and a clipboard write failure if safely simulatable; (9) close/reopen root and confirm hotkey/position behavior. This focused script is enough if automated tests prove the core model/command behavior; do **not** invent a broad radial acceptance campaign.

---

## 7. Efficient build and test policy (slow Windows development machines)

**Verification budget:** Do **not** run expensive whole-repository builds or the entire `cargo nextest run` suite after each checkpoint. Many unrelated integration test targets make that unnecessarily costly. Plan for cached incremental compilation and proportional proof.

- **During M1–M2:** format/check changed files; run relevant model/parser unit tests only after a coherent group is ready. Pure tests should cover most cases without a GUI harness.
- **During M3–M5:** run a small filtered test set when command dispatch or UI state boundaries change, and manual Windows focus/clipboard smoke as soon as the dialog works. Use isolated mocks/fake clipboard services rather than relying on real clipboard state in unit tests.
- **At M6-A:** one substantive targeted verification pass containing the necessary snippets, command-parser/dispatcher, GUI prompt state, dashboard action and persistence tests. Do not automatically include unrelated MkMacro/radial acceptance binaries.
- **Example commands** (adapt test filters after discovering exact names; do not assume an example filter matches until checked):

```powershell
cargo fmt --all -- --check
cargo nextest run -E 'test(snippets)'
cargo nextest run -E 'test(snippet_dialog) or test(snippet_prompt)'
cargo nextest run -E 'test(commands::parser) or test(commands::bus)'
cargo check --locked
```

- An initial cold compile may take substantial time. **Do not repeatedly terminate/restart a running test or launch overlapping Cargo compiles.** If polling a known long-running build remotely, use an infrequent check (approximately 10–20 minutes rather than rapid wake-ups), or await its completion when tooling permits. Never claim it has passed until exit status/output is available.
- A broad or full suite is **exceptional** and requires a concrete technical reason (e.g., an actual shared-core regression), not the mere presence of a changed shared command type. If needed, document the reason before expanding verification.
- If the execution environment cannot build the Windows/eframe project, report the constraint and static/focused evidence accurately; do not pretend static inspection substitutes for the missing Windows smoke check.
- Preserve existing valid tests. Fix newly broken test fixtures appropriately; don't delete assertions, ignore failures, or edit expected output solely to get green results.

---

## 8. Active Checkpoint Commit Cadence — mandatory

> **Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.**

Use an **active, milestone-based commit cadence** throughout this task.

Break larger milestones into coherent implementation checkpoints such as `M1-A`, `M1-B`, `M2-A`, etc., and commit after each meaningful subsection is complete rather than waiting for an entire large milestone or feature to finish. The concrete stage boundaries and suggested commit messages are in §4–§5.

I want Git history to show visible progress and make it easy to understand what was implemented at each stage.

Use judgment on commit size:

- **Do NOT** commit every tiny edit or individual line.
- **Do NOT** create meaningless WIP/checkpoint commits.
- **Do NOT** let several substantial, independently understandable changes accumulate into one very large commit.
- Before beginning a materially different subsection, prefer committing the previous coherent subsection.

Use descriptive commit messages with the plan-stage identifier:

```text
<type>(<scope>): [M#-X] <clear description>
```

For example:

```text
feat(cs): [M3-C] add prompted snippet fill dialog
```

When useful, include a short commit body explaining what changed, why, and what behavior was intentionally preserved.

Do **not** run expensive full verification before every commit. Use small/local checks where useful, commit coherent checkpoints, and perform the plan's substantive targeted verification at the appropriate verification milestone.

If later testing or review finds a defect, prefer a clearly described follow-up remediation commit rather than silently folding unrelated fixes into an earlier checkpoint.

**Do not squash or rewrite the checkpoint history unless I explicitly request it.**

The orchestrator should keep a small, durable stage ledger in the plan or an adjacent task report (planned/in-progress/complete/blocked, commit hash, verification evidence) when helpful, but avoid creating noisy WIP commits only to track progress. Keep already completed checkpoints stable; a reviewer finding should result in a visible remediation commit instead of destructive rewriting.

---

## 9. Orchestrator, agent, and handoff rules

The existing `.codex` configuration defines **planner**, **implementer**, and **reviewer** agents. Reuse them rather than inventing new roles.

- **Orchestrator:** Owns task scope, stage order, Git state, one-writer scheduling, milestone ledger, verification budget, acceptance evidence, and final summary. Before any edits, confirm checked-out branch, `git status`, `HEAD`, `AGENTS.md`, plan and the October 7 ZIP's relevant behavior. No detached-head/silent branch reset/rebase.
- **Planner (read-only):** Refines each upcoming stage into an implementer-ready handoff, verifying source owners and identifying only genuine divergences from the baseline. Must not write code or conduct unrelated test campaigns.
- **Implementer (write):** Executes **one** scoped checkpoint or approved related microgroup at a time, with idiomatic Rust, focused tests, diff review and coherent commit. Only one implementer may modify this repository at a time.
- **Reviewer (read-only):** Reviews changes for behavioral requirements and regression risks; can review key boundaries incrementally and the final complete diff. Must report concrete, severity-ranked defects, not general style preferences.

**Each handoff** should state objective, files/current-state facts, required modifications, invariants, dependencies, non-goals, tests, targeted verification, specific done criteria, and suggested checkpoint commit. If a genuine source divergence makes a stage impossible, stop only the affected stage, explain the precise mismatch and adapt without broadening product scope.

**Decision discipline:** The approved behavior is fixed. Do not ask the user to re-decide each checkbox. Normal Rust/design details belong to the implementer. Escalate only a real incompatible requirement or potentially destructive data-migration choice. Prefer safe additive persistence and deterministic behavior.

**Integration discipline:** Capture testable facts and one canonical action route, not a set of surface-specific hacks. No duplicated parsing, no keyword interception that bypasses typed commands, no generic clipboard hijacking, no application-wide refactor.

---

## 10. Final completion report contract

When all stages are complete, provide:

1. **What was implemented**, user-facing behavior and anything intentionally deferred.
2. **Checkpoint summary:** `M#-X`, short description, commit hash; note any remediation commits without rewriting history.
3. **Proof:** exact focused test/build commands and pass/fail/unrun reasons; manual Windows behaviors tested, with machine/environment qualifier.
4. **Compatibility:** old `snippets.json`, normal `cs`, file watch, clipboard semantics, favorites/pins/radial, and explicit opaque legacy-action limitation.
5. **Security/privacy:** confirmation that typed values were not logged/persisted as form data, plus honest clipboard-history exception.
6. **Unresolved findings:** severity, reproduction and bounded next action (if any).
7. **Changed files**, relevant documentation, and clean/uncommitted working-tree state.

**Final standard:** An ordinary snippet is unchanged; an opted-in snippet opens a clear, fast form from any identifiable snippet invocation, copies a validated final value once, and otherwise fails safely without destroying user data or breaking unrelated launcher behavior.
