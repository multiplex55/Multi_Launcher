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
- Evidence location: `target/productivity_expansion/` (already Git-ignored; generated safe fixtures/screenshots only). Computer-use `@oai/sky` initialization and native window discovery succeeded; capture is assessed against the owned dev window.
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
| M1-A | complete | this checkpoint commit | `cargo check --lib` passed; `cargo test --lib launcher_scope`: 3 passed; `cargo nextest run --test plugin_routing`: 17 passed. Reviewer reload-boundary and typed-command precedence findings fixed; diff checked. Native evidence belongs to M1-C. |
| M1-B/C | pending | — | Bounded planner handoffs ready for session, cache, controls and prefix compatibility |
| M2-A/B/C/D | pending | — | — |
| M3-A/B/C/D/E | pending | — | — |
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
