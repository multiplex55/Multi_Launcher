# Productivity expansion execution ledger

Plan: [approved master plan](multi_launcher_productivity_expansion_master_plan.md).
Started 2026-10-16 on `big-changes`, HEAD `677f7ba0cb49104dbf0d8397a8f4eaa3703019b0`; initial working tree clean.
Windows host, stable `x86_64-pc-windows-msvc`, rustc/cargo 1.99.0, Nextest 0.9.135 available.
`cargo metadata --no-deps --format-version 1` confirms only `multi_launcher` is a workspace member; no vendored workspace exclusions currently required.
Clippy is globally deferred to M13-D; no baseline or intermediate lint runs.

## Safety and evidence

- Single implementation writer; bounded planner and reviewer source passes per milestone.
- File mutations use only disposable `tempfile` roots named for productivity fixtures.
- Native smoke sessions are autonomous, owned-process-only, with isolated app data; no user-operated validation.
- Evidence location: `target/productivity_expansion/` (already Git-ignored; generated safe fixtures/screenshots only). Computer-use initialization/window discovery succeeded, but owned-app inspection timed out at app approval. One bounded retry found no window after containment timeout. Native control/capture is `not_run_environment`; do not repeat the unchanged approval blocker at subsequent milestones or ask the user to intervene.
- Per checkpoint: at most two focused test slices, minimal compilation, and bounded repairs per approved plan.
- Commit hashes are recorded at subsequent checkpoint boundaries to avoid self-referencing commits.

## Current source owners and relevant existing tests

| Concern | Owners | Existing focused coverage |
|---|---|---|
| Search/routing | `src/gui/search.rs`, `src/gui/mod.rs`, `src/plugin.rs`, `src/plugins/omni_search.rs`, `src/commands/` | `plugin_routing`, `query_autocomplete`, `gui_visibility`, internal search tests |
| Snippets/clipboard | `src/plugins/snippet_template.rs`, `src/plugins/snippets.rs`, snippet dialogs/handler, clipboard plugin/actions | `snippets_plugin`, `clipboard_persistence`, internal template tests |
| File operations | `src/file_search/`, file-search dialog, `src/diff/` | domain suite file-search cases |
| JSON/Regex | `src/json_transform.rs`, JSON dialog, `src/regex_tester/`, Regex dialog | internal domain/dialog/preset tests |
| Screenshots/mouse | screenshot plugin/actions/editor, `src/coordinate_tool/`, mouse plugin/settings, `src/mouse_gestures/` | `screenshot_cancel`, `screenshot_plugin`, mouse gesture suites |
| Native ownership | `dev/run_owned_process.ps1`, `dev/test_owned_process.ps1` | owned-process helper checks |

No baseline behavior is claimed tested by this inventory. Feature-specific inspection continues at each milestone.

## Checkpoints

| Checkpoint | Status | Commit | Verification/evidence/issues |
|---|---|---|---|
| M0-A | complete | baseline above | Clean checkout, toolchain and owner inventory inspected; no Clippy |
| M0-B | complete | `946a3006` | Plan read; fixture/evidence policy established |
| M1-A | complete | `263330a6` | `cargo check --lib` passed; `cargo test --lib launcher_scope`: 3 passed; `cargo nextest run --test plugin_routing`: 17 passed. Reviewer reload-boundary and typed-command precedence findings fixed; diff checked. Native evidence belongs to M1-C. |
| M1-B | complete | `49629e25` | Five session-local scopes, Ctrl+1–5, scope-aware cache/empty browse and fresh-session reset. `cargo nextest run --lib launcher_scope`: 11 passed. Reviewer confirmation ownership finding fixed; same slice rerun after final edits. |
| M1-C | complete | `6ae20e87` | Composite Nextest scope/read-only/literal-QR slice: 15 passed; `cargo build --bin multi_launcher` passed. README/Quick Help updated. Real `cs` prefix metadata fixed, legacy `csfoo` preserved; disposable scope fixtures produced no root artifact. Native app launched; no UI interaction/screenshot completed (app approval timeout). Owned PID 35224 removed on helper timeout, post-cleanup Absent; evidence `target/productivity_expansion/M1-native-process/result.json`. |
| M2-A | complete | `d544fbfc` | Typed tokens/context and pure contextual rendering; `cargo nextest run --lib snippet_template`: 19 passed. Reviewer parse-only chrono panic and UTF-8 expectation findings fixed; context/preview Debug redacted. Actual execution/capture follows in B. |
| M2-B | complete | `de5aa773` | Immutable invocation capture and plain/system-only/mixed execution; explicit reveal-gated editor preview. `cargo nextest run --lib snippet_run`: 11 passed; composite prompt/template/system/origin/radial slice: 41 passed; `cargo check --bin multi_launcher` passed. Reviewer startup/restoration origin and editor-preview findings resolved; diff checked. Native blocker unchanged. |
| M2-C | complete | `5ee03531` | Additive normalized tags through existing atomic store, watcher and search; scoped reviewer found no substantive defects. `cargo nextest run --test snippets_plugin`: 12 passed; `cargo nextest run --lib plugins::snippets`: 32 passed. One local error-context compile correction; touched core/test rustfmt and diff checks passed. Legacy JSON, invalid tag retention, hidden-body privacy and raw query/payload compatibility covered. |
| M2-D | complete | this checkpoint | Existing explicit Save/Cancel editor gains canonical tag drafts/selection, AND filters, variable help/preview and lifecycle/privacy coverage. Reviewer tag-only editor text-search gap fixed. `cargo nextest run --lib snippet_dialog`: 43 passed after grouped fixture corrections (duplicate alias and initial animated frame); no second slice needed. Diff checked; README/Quick Help updated. Native screenshot/reopen UI interaction not run due recorded approval timeout. M2 complete through Rust/domain/egui-state evidence. |
| M3-A/B/C/D/E | pending | — | Bounded planner handoffs ready: content references instead of rotating clipboard indexes, atomic preflight via M2 resolver, guarded target identity/conditional clipboard restore, existing main-owned hotkey routes. |
| M4-A/B/C | pending | — | — |
| M5-A/B/C/D | pending | — | — |
| M6-A/B/C/D | pending | — | — |
| M7-A/B/C/D/E/F | pending | — | — |
| M8-A/B/C/D/E | pending | — | — |
| M9-A/B/C/D/E | pending | — | — |
| M10-A/B/C/D/E/F | pending | — | — |
| M11-A/B/C/D | pending | — | — |
| M12-A/B/C | pending | — | — |
| M13-A/B/C | pending | — | — |
| M13-D | pending | — | Final maintained-crate Clippy only; actual outcome in final report |
