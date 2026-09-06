# Persistence Reliability Execution Ledger

This document is the durable execution ledger for the persistence reliability initiative. The checked-out repository is authoritative; update the status, files changed, verification results, and commit field as each milestone is completed. A milestone is complete only after every acceptance criterion and its required verification pass.

Status values: `pending`, `in_progress`, `complete`, `blocked`.

## Initiative invariants

- One Multi Launcher process owns an application data directory; protect threads within that process, but add no cross-process file locks, lock files, database, or merge protocol.
- Missing may initialize defaults. Existing empty data keeps store-specific compatibility. Existing malformed or unreadable data must not silently become defaults and must not be overwritten without explicit recovery/reset.
- A mutation is ordered `load/validate -> prepare -> persist -> publish`; caches and generations describe committed state only.
- Important user-authored JSON uses the existing `common::atomic_file` replacement primitive. Serialization and supported legacy formats remain compatible.
- Watchers retain the last-known-good snapshot on invalid/unreadable events and recover after a later valid event.
- Live data is authoritative. Backups are bounded snapshots; recovery is a staged startup instruction and runs before normal loads.
- Private/high-frequency state is excluded from backup by default and is not mechanically changed to synchronous durable writes.
- No new polling, unbounded workers, startup health scan, UI-thread backup/recovery copy, or persistent idle work.

## Milestone 0 baseline

- **Status:** `complete`
- **Dependency:** none
- **Objective:** establish exact persistence ownership, failure semantics, test coverage, and baseline results before source changes.
- **Files changed:** `docs/persistence-reliability-plan.md`
- **Results:** inventory and baseline recorded; documentation diff inspected; `git diff --check` passed.
- **Commit:** `4fc7edd docs(storage): inventory persistence ownership and failure semantics`

### Repository and path baseline

- Branch: `reliability`; baseline commit: `e0697f9`; working tree was clean before this document was created.
- Production startup passes literal relative paths (`settings.json`, `actions.json`, and most store constants). There is no startup `set_current_dir`; therefore these paths resolve against the process current working directory (CWD). This is existing compatibility behavior and must be preserved.
- `settings::set_settings_path` records the settings path. `common::config_files::resolve_config_path` resolves registered relative config paths against the settings file's parent (or `.`). Today this governs `layouts.json`, `clipboard_modifiers.json`, and `note_ui_state.json` path derivation.
- The future `AppDataRoot` is an identity/ownership boundary around these semantics, not authorization to relocate existing files. Default CWD-owned stores and settings-relative stores belong to it.
- Dashboard uses `settings.dashboard.config_path` or `dashboard.json`; `DashboardConfig::path_for` appends `dashboard.json` only when the supplied value is an existing directory. A configured location outside `AppDataRoot` is external.
- MultiManager defaults to settings-relative `multi_manager_workspaces.json` and `multi_manager_bindings.json`; `multi_manager::state` resolves configured relative paths against the settings parent. Configured absolute/outside paths are external.
- Notes use `ML_NOTES_DIR` when present, otherwise `<current executable directory>/notes`; note assets live below `notes/assets`. Only a notes tree inside `AppDataRoot` is application-owned for default backup. An environment-selected external tree is excluded.
- Scratchpad defaults to CWD-relative `scratchpad.json`; widget `storage_path` may point elsewhere. An outside path is external and excluded by default.
- External Dashboard, MultiManager, Notes, and Scratchpad paths may be health-reported as external but must never be recursively copied, reset, or restored by default.

### Existing atomic and reference implementations

- `src/common/atomic_file.rs`: `save_atomic` creates the parent, uses a collision-resistant create-new temporary file in the destination directory, writes all bytes, flushes and `sync_all`s, then replaces the destination. Windows uses `MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH)` with bounded 5/10/20/40/80/160 ms retry for access-denied/sharing-violation errors. Failures preserve the destination and clean the temporary file. `backup_file` creates collision-safe hard-link/copy `.bak` files. It does not yet expose typed JSON load states or a selectable non-durable policy.
- `src/mkmacro/store.rs`: `MkMacroStore` is the strongest ownership reference. It distinguishes `Missing`, `Empty`, `Loaded`, and `NeedsUserRecovery`; validates/version-migrates; serializes read/write/publication with a transaction mutex; atomically persists before publishing an `Arc` snapshot; retains last-good data and a diagnostic on invalid watcher reload; and manages direct-child PNG assets in `mkmacro_assets`.
- `src/clipboard_modify/config.rs`, `store.rs`, and `watch.rs`: settings-relative, versioned, size-limited, validated config. Missing creates atomic defaults; invalid/future/oversized input remains untouched while explicit load states/diagnostics are published. Saves are atomic and the store publishes only after success. Explicit reset/recovery first creates a collision-safe backup. Watcher errors retain the valid catalog.
- `src/diff/persistence.rs`: typed `Option` missing state, explicit I/O/malformed/unsupported-version errors, validation and atomic saves; runtime worker/cancellation/buffer state is deliberately not serialized.
- `src/plugins/note.rs`: Markdown note saves use `save_atomic`, including rename safety; the note and asset tree has domain-specific validation and watcher/cache handling. The external `ML_NOTES_DIR` policy must remain explicit.

### Critical and user-authored stores

The compact entries below record all required inventory dimensions: purpose/path/class; reader and missing/malformed/unreadable behavior; writer and atomicity; mutation/cache/watcher/legacy behavior; and backup/restore/frequency policy.

1. **settings — `settings.json` (CWD), critical.** `Settings::load` in `settings/model.rs` uses `read_to_string(...).unwrap_or_default()`: missing and every read error become default, empty becomes default, malformed JSON errors. `main` then additionally applies `unwrap_or_default`, runs Clipboard Modify enablement migration twice across startup paths, and may direct-write defaults over malformed input. `Settings::save`, settings editor, GUI actions, hotkey restart, and plugin settings are mutation writers; no transaction lock/cache watcher; serde defaults provide schema compatibility. Direct non-atomic write. Backup eligible; whole-file validate-before-startup restore; low-frequency/on edit. Highest-risk migration target.
2. **actions — `actions.json` (CWD), critical.** `actions::load_actions` reports missing/unreadable/malformed; startup converts all errors to empty. `save_actions` direct-writes and then bumps `ACTIONS_VERSION`. GUI/action editor mutations operate on shared application state, and `gui::watch` reloads actions/settings on notify events; no store transaction. No alternate format. Backup/whole-file restore; low-frequency.
3. **bookmarks — `bookmarks.json` (CWD), critical.** `plugins::bookmarks::load_bookmarks` treats missing/unreadable as empty because of `unwrap_or_default`, accepts empty, then current `Vec<BookmarkEntry>` or legacy `Vec<String>`. Add/remove/alias use load-or-empty then direct save, so corrupt input can be replaced. Plugin keeps data and LRU search cache. Its direct `notify` watcher publishes only successful parses (last-good on malformed/remove because load returns empty on missing and therefore removal can publish empty); cache is invalidated after save/reload. Backup/whole-file restore; user edits, low-frequency.
4. **folders — `folders.json` (CWD), critical.** Reader maps missing/unreadable/empty to generated common-folder defaults; malformed returns error, but mutations and startup fall back to defaults. Add/remove/alias direct-write. Plugin data cache has a direct watcher that unconditionally substitutes defaults on any reload failure, so invalid events overwrite last-good memory. No legacy schema. Backup/whole-file restore; low-frequency.
5. **snippets — `snippets.json` (CWD), critical/private content.** Reader maps missing/unreadable/empty to empty; mutation load errors become empty; direct writer bumps version after success. Plugin data cache plus watcher retains last-good on parse error but missing is read as empty; search behavior uses the cache. No legacy format. Backup eligible despite private content because it is user-authored configuration; validated whole-file restore; low-frequency.
6. **favorites — `fav.json` (CWD), critical.** Same missing/unreadable/empty collapse and mutation hazard as snippets; direct write then version bump. Plugin cache/watch reload only publishes `Ok`, though removal reads as empty. No legacy format. Backup/whole-file restore; low-frequency.
7. **todos — `todo.json` (CWD), critical/private content.** Reader maps missing/unreadable/empty to empty and assigns missing IDs; that load-time migration best-effort direct-saves. Every add/remove/done/priority/tag/clear mutation uses load-or-empty and direct save. `TODO_DATA`, search/index caches, and version publish after a successful save in primary paths; watcher reload publishes only `Ok`, but missing maps to empty. No transaction, so concurrent RMW can lose updates. Backup/whole-file restore; moderate event-driven writes.
8. **shell commands — `shell_cmds.json` (CWD), critical.** Reader maps missing/unreadable/empty to empty; append/remove use load-or-empty and direct write; version is bumped after successful save. No watcher-backed data cache in `ShellPlugin`; no legacy format. Backup/whole-file restore; low-frequency.
9. **legacy macros — `macros.json` (CWD), critical legacy store.** Reader maps missing/unreadable/empty to empty; direct writer. Runtime and plugin startup use load-or-empty. Plugin data has a `JsonWatcher` that publishes only valid parses, but missing maps to empty; command execution also reloads. It remains a supported legacy command format alongside, not a replacement for, MkMacro. Backup/whole-file restore; low-frequency.
10. **history pins — `history_pins.json` (CWD), critical user-authored selection.** `history::{load,save,toggle,upsert,remove,recompute}_pins`; reader maps missing/unreadable/empty to empty, and every RMW mutation uses load-or-empty before direct save. No watcher/cache/version. Backup/whole-file restore; low-frequency. Distinct from replaceable query history.
11. **calendar events — `calendar/events.json` (CWD), critical/private content.** Reader maps missing/unreadable/empty to empty; add/snooze and UI mutations clone `CALENDAR_DATA`, direct-write (creating `calendar/`), then publish cache/version/index. Persistence failure does not publish, but no transaction means concurrent clones can lose updates. `watch_calendar_events` publishes only `Ok`, while deletion maps to empty. No legacy alternate schema. Backup/whole-file restore; moderate event-driven writes.
12. **layouts — settings-relative `layouts.json`, critical.** Path comes from `LAYOUTS_CONFIG` and settings parent. Reader maps missing/unreadable/empty to `LayoutStore::default` and normalizes version 0 in memory. Dashboard/launcher layout UI mutates a loaded store; writer creates parent, direct-writes, then bumps version. No store watcher/cache; dashboard consumers observe generation. Preserve schema/version and aliases. Backup/whole-file restore; low-frequency.
13. **dashboard config — default/configured `dashboard.json`, critical when AppDataRoot-owned, external otherwise.** `DashboardConfig::load` maps missing/unreadable/empty to default, parses then sanitizes/migrates widget configuration in memory. Editor/direct save is non-atomic. Dashboard owns cached config and a watcher/background update path; invalid reload reports rather than intentionally replacing healthy state, but typed state is absent. Backup/restore only if owned; low-frequency.
14. **gesture definitions — `mouse_gestures.json` (CWD), critical.** Reader maps missing/unreadable/empty to defaults and supports schema v2 plus v1 conversion. Save direct-writes normalized v2. Gesture service/UI owns shared runtime DB; reload/mutation paths need transaction and last-good audit. Backup/whole-file restore; low-frequency.
15. **MkMacro — `mkmacros.json` and `mkmacro_assets/*.png` beside it (CWD by default), critical.** Owned by `MkMacroStore`; explicit load disposition, versioned migration, validation, atomic document and asset writes, transaction locks, persist-before-publish cache, last-known-good watcher with error. Legacy import from `macros.json` is supported without changing the legacy store contract. Backup document plus validated direct-child assets; coordinated restore; edits/captures are event-driven. Already compliant reference.
16. **Clipboard Modify — settings-relative `clipboard_modifiers.json`, critical.** Owned by `ClipboardModifierStore/config/watch`; explicit missing/invalid/future/oversized states, schema migration, validation, 5 MiB cap, atomic writes, backup-before-explicit-reset/recovery, persist-before-publish shared catalog, last-good watcher. Backup/whole-file restore; low-frequency. Already compliant reference.
17. **MultiManager workspaces — configured path, default settings-relative `multi_manager_workspaces.json`, critical if owned/external otherwise.** `multi_manager::store` distinguishes read/parse errors but `load_or_default` and some startup paths collapse them. Save uses a fixed sibling `.tmp`, write/flush/`sync_all`, then `fs::rename`; this is not the canonical collision-safe Windows replacement and can strand temp files/collide. State/runtime/UI mutate shared workspaces, with auto-save and exit save; no file watcher. Legacy tuple rectangles/missing IDs normalize on load and Old Manager import remains supported. Backup/restore only when owned; moderate/bursty writes.
18. **notes and note assets — `ML_NOTES_DIR` or `<exe>/notes`, critical if owned/external otherwise.** `plugins::note` and GUI note mutation/panel own Markdown, templates, and `assets/`. Note writes use the canonical atomic primitive and watcher/cache refresh keeps domain data. Missing notes are normal; text is not JSON; read errors are surfaced in mutation paths. Preserve Markdown/link/template formats. Backup directory only when within AppDataRoot; coordinated tree restore; debounced/user-edit frequency. Modern compliant reference.
19. **scratchpad — configured path or CWD `scratchpad.json`, critical if owned/external otherwise.** Background widget worker loads missing/empty as blank and reports unreadable/malformed while returning blank; direct save of `{content}` after debounce. Widget uses generations/results, but error handling can display blank rather than a typed retained state. No watcher/legacy format. Backup/restore only when owned; debounced moderate frequency.

### Replaceable, private, high-frequency, and runtime stores

These stores require an explicit Milestone 11 policy. They are excluded from default backups unless a later product decision explicitly opts them in.

1. **query history — `history.json` (CWD), replaceable/private/high-frequency.** Global `HISTORY` loads missing/unreadable as empty and malformed as logged empty; append/clear publish memory before a direct save, so failed persistence leaves memory advanced. No watcher; bounded by settings history limit. Do not make every append fully durable without measurement.
2. **clipboard history — `clipboard_history.json` (CWD), replaceable/highly private/high-frequency.** Plugin reader collapses missing/unreadable to empty, direct writer bumps version, watcher publishes valid parses, and clipboard polling mutates cached history before best-effort save. Exclude from backups and diagnostics content.
3. **calculator history — `calc_history.json` (CWD), replaceable/private.** Reader collapses missing/unreadable/empty; append/remove/clear RMW and direct-write; bounded to 20 normally; no watcher/cache owner. Exclude by default.
4. **usage — `usage.json` (CWD), replaceable/high-frequency.** `usage::{load_usage,save_usage}` maps missing/unreadable/empty to empty and direct-writes counters; launcher holds the working map. No watcher. Exclude by default.
5. **calendar view state — `calendar/state.json` (CWD), replaceable/session.** Missing/unreadable/empty becomes default; direct writer creates parent. No watcher/cache. Exclude by default.
6. **gesture usage — `mouse_gestures_usage.json` (CWD), replaceable/high-frequency.** Reader collapses all parse/read failures to default; usage recording direct-writes best-effort and logs errors. Exclude by default.
7. **gesture runtime state — `mouse_gestures_state.json` (CWD), replaceable/session.** Reader collapses read/parse failures to default; service direct-writes best-effort. Exclude by default.
8. **note UI state — settings-relative `note_ui_state.json`, replaceable/session.** Reader explicitly defaults only when path does not exist and otherwise propagates read/parse errors; writer creates parent then direct-writes. GUI panel RMW handles errors. Exclude by default.
9. **MultiManager bindings — configured path, default settings-relative `multi_manager_bindings.json`, replaceable/runtime and private window metadata; external when outside root.** `load_bindings_if_exists` correctly distinguishes missing from malformed/unreadable. Save snapshots only valid HWND bindings via fixed sibling temp, flush/sync, rename; auto-save is debounced and exit-flushed. Exclude from default backup; restore/reconnect is already a domain command, not persistence disaster recovery.
10. **alarms — `alarms.json` (CWD), runtime/session.** Timer manager serializes persistent alarms while holding its timer lock and direct-writes best-effort after timer changes; startup loader restores saved alarms. Missing/corrupt handling is permissive runtime recovery. Exclude by default unless later classified as user-critical.
11. **logs/toasts/runtime/session — `launcher.log`, `toast.log`, in-memory worker progress, selections, caches, and other UI state.** Diagnostic or ephemeral, not recovery sources; existing append/rotation/runtime ownership varies. Never include by default or expose content in health diagnostics.

### Failure pattern and watcher conclusions

- Dangerous RMW sites confirmed: bookmarks, folders, snippets, favorites, shell commands, todos, history pins, calculator history, and similar helpers load with `unwrap_or_default`/default fallback before save. Settings has the startup migration overwrite chain. These must be migrated by their owning store, not by global dynamic path locks.
- `JsonWatcher` coalesces one native watcher per normalized path and invokes callbacks on create/modify/remove. It provides notification only; the domain callback owns parsing, last-good publication, diagnostic state, and transaction ordering.
- Good current last-good callbacks include MkMacro and Clipboard Modify (explicit errors), plus several older stores that publish only `Ok`. Folders is unsafe because callback failure becomes defaults. All callbacks must treat deletion/missing according to an explicit policy and serialize with local saves.
- Several cached stores already persist before cache publication (todo primary paths, calendar events, MkMacro, Clipboard Modify), but generation/cache semantics and concurrency still require store-by-store tests. History and clipboard mutate memory before best-effort persistence.

### Baseline tests and migration map

- Existing focused suites: `tests/bookmarks_plugin.rs`, `folders_plugin.rs`, `snippets_plugin.rs`, `fav_plugin.rs`, `todo_plugin.rs`, `dashboard_config.rs`, `mouse_gestures_db.rs`, `mouse_gestures_service.rs`, `mkmacro_store.rs`, `mkmacro_*`, `multi_manager_plugin.rs`, `multi_manager_launcher_actions.rs`, `settings_editor.rs`, `settings_plugin.rs`, `watchers.rs`, `watcher_failures.rs`, `clipboard_persistence.rs`, grouped `tests/domain_cases/clipboard_modify_{config,runtime}.rs`, and relevant inline unit tests in settings, calendar, layouts, dashboard, common atomic/watch, MultiManager, notes, and note UI state.
- Tests likely to migrate: store tests that currently assert missing and malformed both produce empty/default; mutation tests that assume direct-write behavior; settings startup/migration tests; folders watcher fallback tests; cache/version tests; fixed `.tmp` replacement-failure tests in MultiManager; watcher timing/failure tests; serialization snapshot tests where ownership APIs change. Preserve their user-visible contracts and supported JSON fixtures.
- Add focused corruption coverage for settings; every critical catalog mutation; atomic replacement/temp cleanup/parent creation; cache publication and generation on failure; invalid watcher then valid recovery; two concurrent logical mutations; catalog health; backup inclusion/privacy/retention; staged restore/reset; and named-mutex isolation. Prefer existing grouped Nextest suites over many new binaries.

### Recorded baseline verification

- `cargo check`: **PASS**, cold run `2m20s` at `e0697f9`.
- `cargo nextest run --no-fail-fast`: **BASELINE FAIL** — `3014 passed`, `8 failed`, `7 skipped`, `70 binaries`.
- Exact pre-existing failures, all in `gui::file_search_dialog::tests`: `cancelling_search_updates_status`; `enter_in_file_search_field_starts_exactly_one_search`; `escape_while_running_cancels_and_leaves_dialog_open`; `global_filename_search_uses_walkdir_backend_when_everything_is_disabled`; `second_escape_after_cancellation_closes_now_idle_dialog`; `starting_a_search_clears_selection`; `starting_search_sets_active_state`; `starting_search_sets_immediate_repaint_request`.
- A targeted rerun reproduced all eight. Environmental root cause: `FileSearchDialogState::build_search_request` rejects the default global scope because `default_global_search_roots()` obtains no usable home directory in this execution environment, so `start_search` returns `None`. This is unrelated to persistence and must not be attributed to this initiative.

### Milestone 0 acceptance

- [x] Exact store inventory and critical/replaceable/external classifications recorded.
- [x] Readers, writers, mutation paths, watchers, caches, legacy formats, missing/malformed/unreadable behavior, atomicity, backup/restore policy, and write frequency recorded.
- [x] Current path resolution and modern reference implementations verified in source.
- [x] Existing tests and expected migration areas recorded.
- [x] Baseline commands, counts, exact failures, and root cause recorded.
- [x] Documentation diff inspected and `git diff --check` passes.

## Ordered implementation milestones

Unless a milestone states otherwise, verification means: focused unit/integration tests, `cargo fmt --all -- --check`, `cargo check`, an appropriate `cargo nextest run` subset, `git diff --check`, and manual diff inspection for format/API/serialization drift. Record actual commands and results in that milestone before marking it complete.

### 1. AppDataRoot identity and Windows single instance

- **Status:** `complete`; **depends on:** 0.
- **Objective:** introduce a canonical data-root identity without relocating files and acquire a Windows named mutex before persistence/startup initialization.
- **Repository finding:** startup currently has no data-root type or single-instance guard and begins loading `settings.json` immediately from CWD.
- **Likely files:** `src/platform/single_instance.rs`, `src/platform/mod.rs`, `src/main.rs`, path/config helpers, focused tests.
- **Design/invariants:** stable mutex name derived from normalized root identity; guard lives through shutdown; duplicate exits cleanly with concise feedback; unique test names; no lock file/cross-process file locking. Preserve CWD/settings-relative paths.
- **Acceptance/tests:** first acquisition succeeds, duplicate reports already running, independent roots/names do not conflict, guard release permits reacquisition; test names isolated under Nextest; startup acquires before settings/recovery. Run targeted tests, format check, `cargo check`, Nextest subset, diff check.
- **Risks:** path normalization identity collisions, abandoned mutex semantics, test interference, changing startup path behavior.
- **Files/results/commit:** `Cargo.toml`, `src/main.rs`, `src/platform/mod.rs`, `src/platform/app_data.rs`, `src/platform/single_instance.rs`, this ledger / `cargo test platform::single_instance --lib` 6 passed; `cargo test platform::app_data --lib` 2 passed; `cargo nextest run -E 'test(/platform::(single_instance|app_data)/)'` 8 passed; `cargo fmt --all -- --check` passed; `cargo check` passed; `git diff --check` passed; startup diff and path/API compatibility inspected / `cb9b71b feat(startup): enforce single-instance data ownership`.

### 2. Typed atomic JSON foundation

- **Status:** `complete`; **depends on:** 1.
- **Objective:** build a small typed JSON load/save API on `common::atomic_file` that distinguishes missing, empty, valid, malformed, unreadable, invalid, and unsupported states.
- **Repository finding:** the atomic byte primitive is strong, but load semantics and domain context are duplicated; older stores direct-write JSON.
- **Likely files:** `src/common/persistence.rs` or `json_file.rs`, `src/common/atomic_file.rs`, `src/common/mod.rs`, tests.
- **Design/invariants:** preserve `save_atomic`; contextual operation/store/path errors; decoding/validation separated from migration writes; explicit durable policy; no ORM/global path-lock manager/schema change.
- **Acceptance/tests:** missing/empty/valid/malformed/unreadable are distinct; atomic successful output parses; replacement failure preserves destination and cleans temp; expected parents created; existing atomic retry/backup behavior retained.
- **Risks:** generic API complexity, accidental serialization differences, Windows replacement edge cases, unnecessary `sync_all` on hot paths.
- **Files/results/commit:** `src/common/persistence.rs`, `src/common/mod.rs`, this ledger / `cargo test common::persistence --lib` 8 passed; `cargo test common::atomic_file --lib` 2 passed; `cargo nextest run -E 'test(/common::(persistence|atomic_file)/)'` 10 passed; `cargo fmt --all -- --check` passed; `cargo check` passed; `git diff --check` passed; API/serialization diff inspected and existing atomic failure seam retained unchanged / `be9ab29 refactor(storage): add typed atomic JSON persistence primitives`.

### 3. Settings startup corruption safety and transactions

- **Status:** `complete`; **depends on:** 2.
- **Objective:** make settings the first typed transactional store and eliminate malformed-settings overwrite during startup migration.
- **Repository finding:** `Settings::load` collapses read errors; `main` also defaults load errors and then may persist the Clipboard Modify migration.
- **Likely files:** `src/settings/*`, `src/main.rs`, settings editor/GUI save paths, startup tests.
- **Design/invariants:** explicit missing/empty/loaded/invalid startup state; malformed/unreadable bytes retained; a settings-owned transaction serializes read/modify/write; migrations run only after valid/default-missing load and persist before publish/restart.
- **Acceptance/tests:** missing and intended empty compatibility initialize; valid loads; malformed returns diagnostic and remains byte-for-byte unchanged through startup; valid Clipboard Modify migration persists; failed save does not publish; existing serde contract preserved.
- **Risks:** startup behavior and hotkey/plugin settings regression, duplicate migration ownership, editor callers bypassing store.
- **Files/results/commit:** `src/settings/store.rs`, `src/settings/model.rs`, `src/settings/mod.rs`, `src/startup.rs`, `src/lib.rs`, `src/main.rs`, `src/gui/{mod,render,theme_settings_dialog,mouse_gesture_settings_dialog,note_graph_dialog,note_panel}.rs`, `src/settings_editor/save.rs`, `src/plugin_editor.rs`, `src/help_window.rs`, `src/multi_manager/settings.rs`, `tests/domain_cases/theme_settings_dialog.rs`, this ledger / `cargo test settings::store --lib` 6 passed; `cargo test startup::tests --lib` 7 passed; `cargo test settings --lib` 95 passed; theme failed-save publication test and file-search failed-preferences-save runtime rollback test passed; targeted Nextest settings/startup/migration/GUI/MultiManager subset 140 passed across 70 binaries; `cargo check` passed; `cargo fmt --all -- --check` and `git diff --check` passed; settings load/save/migration callers and serialization/API diff inspected. Broad `cargo test clipboard_modify --lib` had 163 pass and one parallel shared-history assertion failure; its exact isolated rerun passed. / _pending orchestrator commit_.

### 4A. Actions

- **Status:** `pending`; **depends on:** 2 (and root identity from 1).
- **Objective:** give `actions.json` typed load semantics, atomic writes, serialized mutations, and committed-only version publication.
- **Repository finding:** action loading reports errors but startup converts them to empty; saving direct-writes and only then bumps a global version.
- **Likely files:** `src/actions/mod.rs`, action editor/GUI callers, watcher/startup tests.
- **Acceptance/tests:** missing remains valid initial state; malformed/unreadable mutation fails without byte change; concurrent mutations do not lose updates; failed save does not bump version/publish; format and command behavior unchanged.
- **Risks:** startup currently accepts missing as empty; action list `Arc` ownership and watcher reload integration.
- **Files/results/commit:** _pending / pending / pending_.

### 4B. Bookmarks and folders

- **Status:** `pending`; **depends on:** 2.
- **Objective:** replace load-or-default RMW with domain transactions and atomic persistence for both catalogs.
- **Repository finding:** both stores have unsafe default fallbacks; bookmarks preserves a legacy string list, while the folders watcher publishes defaults on reload failure.
- **Likely files:** `src/plugins/bookmarks.rs`, `folders.rs`, action/GUI callers, existing plugin/watcher tests.
- **Design/invariants:** retain legacy bookmark string-list import and generated folder defaults only for legitimate missing/empty policy; persist before cache/version publication; last-good watchers.
- **Acceptance/tests:** malformed add/remove/alias rejected unchanged; bookmark legacy schema readable; folders corruption never publishes defaults; two logical mutations survive; existing search/command behavior preserved.
- **Risks:** default folders compatibility, watcher deletion semantics, bookmark LRU invalidation.
- **Files/results/commit:** _pending / pending / pending_.

### 4C. Snippets and favorites

- **Status:** `pending`; **depends on:** 2.
- **Objective:** transactional atomic mutations with committed-only cache/version changes.
- **Repository finding:** both readers collapse read failures and mutations load-or-empty before direct writes; watchers publish only successful parses but deletion appears empty.
- **Likely files:** `src/plugins/snippets.rs`, `fav.rs`, action/GUI callers and tests.
- **Acceptance/tests:** malformed mutation rejected unchanged; missing initializes; failed write retains cache/version; watcher invalid then valid recovery; command formats unchanged.
- **Risks:** private content diagnostics, watcher remove behavior, global cache/version test isolation.
- **Files/results/commit:** _pending / pending / pending_.

### 4D. Shell commands, legacy macros, and history pins

- **Status:** `pending`; **depends on:** 2.
- **Objective:** harden remaining simple critical CWD catalogs without conflating legacy macros with MkMacro or replaceable query history.
- **Repository finding:** each critical catalog uses direct writes and load-or-empty RMW; `macros.json` remains a supported legacy feature and pins share a module with replaceable history.
- **Likely files:** `src/plugins/shell.rs`, `src/plugins/macros.rs`, `src/history.rs`, GUI/action callers/tests.
- **Design/invariants:** keep `macros.json` schema/commands readable; treat `history_pins.json` as critical while `history.json` remains Milestone 11; transaction per store.
- **Acceptance/tests:** malformed mutation/recompute fails unchanged; atomic save; macro watcher retains last-good; failed saves do not publish/bump; legacy behavior preserved.
- **Risks:** macros runtime reloads settings/actions, history naming confusion, indirect callers.
- **Files/results/commit:** _pending / pending / pending_.

### 5A. Todo

- **Status:** `pending`; **depends on:** 2.
- **Objective:** transactional todo persistence including safe ID migration and cache/index publication.
- **Repository finding:** todo load may write migrated IDs, mutation loads collapse errors, and global data/search/link caches are published after primary saves but lack transaction serialization.
- **Likely files:** `src/plugins/todo.rs`, todo GUI/command handlers, tests.
- **Acceptance/tests:** malformed mutations fail unchanged; missing-ID migration never overwrites invalid data; cache/index/version update only after commit; concurrent mutations retained; watcher last-good; schema/links behavior compatible.
- **Risks:** load-time side-effect migration, multiple GUI mutation entry points, notes link-index coupling.
- **Files/results/commit:** _pending / pending / pending_.

### 5B. Calendar events

- **Status:** `pending`; **depends on:** 2.
- **Objective:** serialize event RMW and atomically persist before publishing calendar cache/index/version; leave calendar view state for Milestone 11.
- **Repository finding:** event saves publish after direct write, but mutations clone a global snapshot without a transaction and missing/unreadable reads collapse to empty.
- **Likely files:** `src/plugins/calendar.rs`, calendar handlers/UI/tests.
- **Acceptance/tests:** malformed event DB rejects add/edit/snooze unchanged; concurrent changes retained; failed write retains cache/version/index; watcher last-good then recovers; parent creation and JSON compatibility preserved.
- **Risks:** recurrence/date serde, global state test isolation, event IDs and UI clones.
- **Files/results/commit:** _pending / pending / pending_.

### 5C. Layouts and dashboard config

- **Status:** `pending`; **depends on:** 2.
- **Objective:** apply typed/atomic semantics to owned layouts and dashboard configuration while respecting external paths.
- **Repository finding:** both loaders collapse read failures; both writers are direct; dashboard paths may be configured outside the root and loading performs sanitization/migration.
- **Likely files:** `src/plugins/layouts_storage.rs`, `src/dashboard/config.rs`, dashboard/editor/widgets, config path tests.
- **Acceptance/tests:** malformed mutation/save workflow does not replace data; version/sanitize migrations remain compatible; cache/generation publishes after commit; external paths classified, not backed up/reset; watcher invalid retains last-good.
- **Risks:** sanitization currently mutates during load, directory-vs-file dashboard path semantics, imported layouts.
- **Files/results/commit:** _pending / pending / pending_.

### 5D. Mouse gesture definitions

- **Status:** `pending`; **depends on:** 2.
- **Objective:** transactional atomic schema-v2 gesture definition storage; usage/state remain Milestone 11.
- **Repository finding:** definitions use direct writes and permissive defaults while supporting v1 input; the service shares runtime state across threads.
- **Likely files:** `src/mouse_gestures/db.rs`, service/UI/settings integration and tests.
- **Acceptance/tests:** missing defaults preserved; malformed mutation fails unchanged; v1 loads/migrates safely; failed persistence does not publish runtime DB; watcher retains last-good; gesture behavior unchanged.
- **Risks:** runtime service concurrency, schema migration writes, global service tests.
- **Files/results/commit:** _pending / pending / pending_.

### 5E. MultiManager workspaces and modern-store conformance

- **Status:** `pending`; **depends on:** 2, 3.
- **Objective:** migrate owned workspaces to canonical atomic transactions and verify MkMacro, Clipboard Modify, Diff, Notes, and replaceable bindings conform rather than rewriting them.
- **Repository finding:** MultiManager uses fixed `.tmp` plus `rename` and retains a defaulting loader; the named modern stores already implement most target guarantees.
- **Likely files:** `src/multi_manager/store.rs`, `state.rs`, runtime/UI callers, focused conformance tests; reference stores only if a proven gap exists.
- **Acceptance/tests:** malformed workspaces do not default-and-save; canonical collision-safe replace; auto/exit saves serialized; supported legacy rectangles/IDs import; external workspace policy enforced; modern references keep persist-before-publish and last-good guarantees.
- **Risks:** configured paths, autosave bursts, fixed-temp compatibility tests, unnecessary churn in already-correct stores.
- **Files/results/commit:** _pending / pending / pending_.

### 6. Watcher last-known-good standardization

- **Status:** `pending`; **depends on:** 3, 4A-4D, 5A-5E.
- **Objective:** audit every watched critical JSON store and standardize typed last-known-good/error/recovery behavior.
- **Repository finding:** `JsonWatcher` is only a notifier; domain callbacks differ, notably unsafe folder fallback versus explicit MkMacro/Clipboard Modify diagnostics.
- **Likely files:** `src/common/json_watch.rs`, domain watcher callbacks, `src/gui/watch.rs`, `tests/watchers.rs`, `tests/watcher_failures.rs`.
- **Design/invariants:** notification utility does not own domain parsing; transaction lock orders reload with local save; malformed/unreadable events retain data; later valid events publish; no sleeps/polling as a correctness mechanism.
- **Acceptance/tests:** invalid event never publishes empty/default; error observable; valid follow-up recovers and clears/updates diagnostic; remove policy explicit; no watcher leak/duplicate callback regression.
- **Risks:** notify event bursts, self-write races, tests under Nextest, Windows sharing.
- **Files/results/commit:** _pending / pending / pending_.

### 7. Canonical catalog and read-only health

- **Status:** `pending`; **depends on:** 1-6.
- **Objective:** define one typed store catalog used by health, backup, and recovery, and provide on-demand non-mutating health inspection.
- **Repository finding:** no canonical catalog exists; path/classification knowledge is currently spread across constants, settings, GUI construction, and environment logic.
- **Likely files:** `src/persistence/{mod,catalog,health}.rs` (or clearly named equivalent), store descriptors/tests.
- **Design/invariants:** descriptors cover ID/path ownership/class/privacy/validation/backup/restore; external and missing distinguished; health never migrates, creates, resets, or writes; no startup scan/polling/content disclosure.
- **Acceptance/tests:** every inventoried store classified exactly once or explicitly excluded; healthy/missing/empty/malformed/unreadable/external states; catalog drives later consumers; diagnostics contain metadata/errors, never private contents.
- **Risks:** duplicated store lists, validator side effects, symlink/path containment.
- **Files/results/commit:** _pending / pending / pending_.

### 8A. Backup engine

- **Status:** `pending`; **depends on:** 7.
- **Objective:** create validated application-owned snapshots with a manifest and safe bounded retention.
- **Repository finding:** `backup_file` handles one adjacent backup, but there is no application snapshot manifest, inclusion policy, or bounded snapshot retention.
- **Likely files:** `src/persistence/backup.rs`, catalog integration/tests.
- **Design/invariants:** root such as `<AppDataRoot>/backups/<recognized snapshot>`; include critical owned files/directories and MkMacro assets/owned notes; exclude private/high-frequency/external data; copy to staging then finalize; never prune unknown directories; keep newest five recognized snapshots.
- **Acceptance/tests:** valid manifest and copied healthy files; optional missing recorded; external/private excluded; asset trees copied safely; partial copy explicit; retention only removes recognized old snapshots.
- **Risks:** recursive traversal/symlinks, partial snapshots, unsafe pruning, large notes/assets.
- **Files/results/commit:** _pending / pending / pending_.

### 8B. Bounded data-service worker

- **Status:** `pending`; **depends on:** 7, 8A.
- **Objective:** run backup/health work off egui using existing bounded worker/lifecycle conventions.
- **Repository finding:** dashboard and other optimized subsystems provide bounded background-worker patterns; no persistence data-service worker exists.
- **Likely files:** persistence/data service, GUI worker integration, shutdown tests.
- **Design/invariants:** bounded queue/worker count; event-driven/manual only; cancellation/result delivery and shutdown owned explicitly; no detached/unbounded threads or idle polling.
- **Acceptance/tests:** UI call returns promptly; busy/coalescing policy deterministic; success/failure delivered; worker shuts down; no persistent repaint/idle work.
- **Risks:** shutdown leaks, stale generation results, UI-thread file enumeration.
- **Files/results/commit:** _pending / pending / pending_.

### 9. Staged startup recovery and reset

- **Status:** `pending`; **depends on:** 1, 2, 7, 8A.
- **Objective:** validate and stage explicit recovery/reset, then apply it atomically before normal runtime loads on next launch.
- **Repository finding:** Clipboard Modify has local explicit recovery helpers, but the application has no general pending-recovery descriptor or pre-load recovery phase.
- **Likely files:** `src/persistence/recovery.rs`, startup/main integration, descriptor format/tests.
- **Design/invariants:** pending descriptor is short-lived instruction, not source of truth; validate source/store before staging; at startup back up current destination, atomically replace or preserve-corrupt-on-reset, clear descriptor only after success; restart required; no live overwrite under stale memory.
- **Acceptance/tests:** valid restore/reset; invalid backup cannot harm destination; destination backup/preserved corrupt original; failure leaves descriptor/actionable state; processing precedes settings/plugins/watchers.
- **Risks:** path traversal/store-ID spoofing, crash mid-recovery, stale descriptor loop, ordering before logging/settings.
- **Files/results/commit:** _pending / pending / pending_.

### 10A. Typed Data command and plugin

- **Status:** `pending`; **depends on:** 7-9 and existing Typed Command Bus.
- **Objective:** expose data health/backup/recovery intents through typed commands without a new string dispatcher.
- **Repository finding:** the Typed Command Bus already owns application commands, but there is no Data plugin/request family.
- **Likely files:** `src/commands/model.rs`, parser/bus/handlers, `src/plugins/data.rs`, plugin registry/tests.
- **Design/invariants:** parser maps user syntax to typed request; handler delegates to data service; command outcome drives UI/restart. Preserve existing command formats/plugin ABI.
- **Acceptance/tests:** command discovery/parsing/routing for health, backup, recovery/reset/open UI; invalid args typed errors; architecture tests prove no stringly bypass.
- **Risks:** command namespace collisions, UI-only operations in headless host, plugin capability registration.
- **Files/results/commit:** _pending / pending / pending_.

### 10B. Data & Recovery UI and diagnostics

- **Status:** `pending`; **depends on:** 8B, 9, 10A.
- **Objective:** provide on-demand health, backup status, and explicit recovery/reset staging UI with concise diagnostics and restart guidance.
- **Repository finding:** existing dialogs and command outcomes provide lifecycle/error patterns; there is no unified data-health/recovery interface.
- **Likely files:** `src/gui/data_recovery_dialog.rs`, `src/gui/{mod,render,command_host}.rs`, help/README, tests.
- **Design/invariants:** potentially destructive choices require explicit confirmation; operations are async; show store/path/state/error summary but never file contents; external/excluded status clear; recovery says restart required.
- **Acceptance/tests:** open/close lifecycle, busy/result/error states, confirmation, backup action, staged action and restart prompt, diagnostic privacy, no idle repaint regression.
- **Risks:** accidental immediate replacement, UI blocking, stale results, exposing private content.
- **Files/results/commit:** _pending / pending / pending_.

### 11. High-frequency and replaceable policy

- **Status:** `pending`; **depends on:** 2, 7; should follow critical-store migration.
- **Objective:** audit history, clipboard/calc history, usage, calendar state, gesture usage/state, note UI state, MultiManager bindings, alarms, and other runtime/session data separately.
- **Repository finding:** these stores have materially different frequency/privacy/durability needs; several mutate memory before best-effort direct writes.
- **Likely files:** inventoried store modules and this ledger; code only when measurement/correctness justifies it.
- **Design/invariants:** critical data is atomic+durable; replaceable data uses a named measured policy (possibly atomic without full durability), coalescing/debounce/best effort as appropriate; private data excluded from backup. Do not change code for uniformity alone.
- **Acceptance/tests:** each store has a documented policy; failure/cache semantics are intentional; no unnecessary hot-path sync; no major latency/idle regression.
- **Risks:** write amplification, UI stalls, privacy, weakening explicitly durable runtime state.
- **Files/results/commit:** _pending / pending / pending (no commit if audit needs no code; ledger update belongs with documentation milestone)_.

### 12. Comprehensive regression matrix and documentation

- **Status:** `pending`; **depends on:** 1-11.
- **Objective:** make persistence failures first-class regression behavior and finalize user/developer guarantees without expanding test binary count casually.
- **Repository finding:** broad behavior coverage exists across 70 binaries and grouped suites, but corruption/recovery/concurrency cases are incomplete and the prior initiative intentionally reduced binary count.
- **Likely files:** existing grouped suites (`tests/suites/domain.rs` or appropriate group), focused module tests, README/help, this ledger.
- **Acceptance/tests:** settings missing/empty/valid/malformed/startup migration; every critical catalog malformed mutation; atomic preservation/cleanup/parse/parents; committed-only caches/generations; watcher retain/recover; concurrent mutations; backup manifest/inclusion/privacy/retention; recovery validation/order/preservation; isolated named mutex. Existing behavioral tests remain represented.
- **Verification:** targeted subsets followed by `cargo nextest run --no-fail-fast`; format, check, diff check. Plain `cargo test` is insufficient.
- **Risks:** Nextest global-state races, excessive new binaries, weakening old assertions, environmental file-search baseline.
- **Files/results/commit:** _pending / pending / pending_.

### 13. Performance regression verification

- **Status:** `pending`; **depends on:** 1-12.
- **Objective:** prove reliability work preserves the prior startup, first-frame, dashboard idle, cached-search, and test-performance gains.
- **Repository finding:** existing `performance::Timer` labels and the performance ledger provide comparable startup/first-frame mechanisms; persistence health currently adds no idle scan.
- **Likely files:** existing performance ledger/instrumentation and this ledger; code only to remediate measured regression.
- **Design/invariants:** startup does only mutex, pending-recovery descriptor, required settings, and normal existing loads; no automatic health/backup scan. Move nonessential work off startup/UI without weakening corruption protection.
- **Acceptance/tests:** comparable measurements for `LauncherApp` construction, first usable frame, idle/static dashboard, and common persistence-involved commands; inspect added synchronous settings work; no meaningful regression; record before/after actuals.
- **Risks:** incomparable measurements, cold-cache noise, hiding regression by weakening durability.
- **Files/results/commit:** _pending / pending / pending_.

### 14. Independent review and remediation

- **Status:** `pending`; **depends on:** 1-13.
- **Objective:** independent high-reasoning, repository-wide review followed by resolution of all material findings.
- **Repository finding:** this cross-cutting migration can leave bypasses outside touched files, so repository-wide searches and an independent reviewer are required.
- **Likely files:** read-only review initially; focused remediation files/tests as findings require.
- **Review checklist:** remaining critical `unwrap_or_default` RMW/direct writes; publish-before-persist; startup overwrite; automatic reset; watcher empty publication; duplicate migration paths; late recovery; unsafe pruning/private backup; UI blocking/unbounded workers/shutdown; Nextest races; cross-process complexity; format/ABI/command/performance regressions.
- **Acceptance/tests:** findings classified Critical/High/Medium/Low/false-positive; all Critical/High and in-scope Medium resolved; affected tests rerun; repeat review until no unresolved material issue.
- **Risks:** review limited to touched files, undocumented intentional exceptions, broad remediation commits.
- **Files/results/commit:** _pending / pending / pending_.

### Final integration gate

- **Status:** `pending`; **depends on:** all milestones complete and review clear.
- **Objective:** prove the cumulative feature meets the original definition of done and leave the feature branch clean.
- **Repository finding:** the baseline full Nextest run has eight unrelated environmental file-search failures that must be distinguished from feature regressions and resolved for the final all-green gate.
- **Acceptance:** inspect cumulative diff and stale references; all migrations complete; no duplicate/bypass path; milestone ledger/commits complete; working tree clean.
- **Required verification:** `cargo fmt --all -- --check`; `cargo check`; `cargo nextest run --no-fail-fast`; `cargo build --release`; `git diff --check`; existing performance commands including `cargo bench --bench search` when still standard. Record actual counts/measurements; no estimated PASS.
- **Compatibility/performance gate:** JSON schemas, legacy formats, Typed Command Bus, plugin ABI, commands, hotkeys/visibility, watcher behavior, startup/first-frame/idle performance remain compatible except explicitly documented changes.
- **Files/results/commit:** _pending / pending / pending_.
