# UI polish execution ledger

Approved contract: `multi_launcher_ui_polish_codex_plan_20261015.md`.

- Branch: `codex/ui-polish-20261015`
- Base HEAD: `f92bbafb402b3f6d831585fc9d43eb2c5c589261`
- Initial working tree: clean (including staged changes).
- User waived reference ZIP comparison on October 15, 2026; current checkout is baseline.
- Project: multi_launcher 0.1.0; rustc 1.99.0 (b940084d7), x86_64-pc-windows-msvc; Nextest 0.9.135.
- Snapshot source observations confirmed: library hotkey no-op / binary restart channel split, six curated palettes, existing typography scale, unconditional branding.
- No push, reset, rebase, or discard authorized.

| Milestone | State | Commit / evidence |
|---|---|---|
| UP-HK | in_progress | Planner preparing bounded handoff |
| UP-MK | pending | |
| UP-BR | pending | |
| UP-PA | pending | |
| UP-TX | pending | |
| UP-FL | pending | Reference videos not located; diagnosis must precede fix |
| UP-QA | pending | |

## Verification and review

Record exact commands, outcomes, review resolutions, and native evidence at each checkpoint. No full test suite is an implicit gate. Modified Rust must remain Clippy-clean without new blanket suppressions.

- Baseline `cargo fmt --all -- --check`: FAIL (existing import order and assertion wrapping in `src/gui/theme_settings_dialog.rs`; no files changed by check). Address when that file is modified in UP-PA/UP-TX.
- Preflight `git diff --check`: PASS.

## Checkpoints

### UP-HK — in progress

- Planner confirmed current ownership and delivered UP-HK-A (checked bridge/validation/listener preparation) then UP-HK-B (transactional Save and runtime migration).
- Implementer is sole source writer and must stop after A for review and commit.
- Reviewer pre-review identifies release draining, stale generations, rollback field ownership, pending Save while dialog hidden, route symmetry, and actual Save behavior as required review points.
- Existing `Settings::update` lock must not span runtime acknowledgement; rollback must preserve unrelated concurrent settings writes.
- Existing route handoffs must be reused, including waiting for owned release. A is not completion of the full UP-HK behavior.
- UP-HK-A accepted for checkpoint commit: `refactor(hotkey): [UP-HK-A] introduce checked runtime reconfiguration requests`. Full UP-HK remains in progress; GUI Save activation belongs to B.
- A draft review found: radial self-reservation rejection (P1), strict preparation dropping valid startup bindings alongside unsupported saved keys (P1), and AltGr reservation conversion bypassing macro conflicts (P2). Implementer correcting all three with regressions before completion.
- A compile command: `cargo check -p multi_launcher --lib --bin multi_launcher`: PASS after correcting import, obsolete no-op caller, key mapping type, and listener closure ownership errors.
- Reviewer refresh confirms all three production findings resolved with focused regressions. Two reversed message assertions were also corrected. Focused Nextest verification is running; no test pass claimed yet.
- Native computer-use skill and its `@oai/sky` runtime initialized successfully; parent can perform scoped UI checks against an isolated profile after executable build.

### UP-HK-A verification and review result

- Typed library/main preparation requests carry identity/generation and explicit Prepared/error completion. Main validates proposals and owns dormant listener resources. Preparation does not activate a route or persist settings.
- Actual listener mapping is shared with validation. Strict replacement preparation rejects unsupported keys; legacy startup retains supported bindings alongside unsupported saved ones.
- Process/Screen Draw/radial/macro conflict checks are centralized; AltGr reservations are recognized. Removed isolated binary restart channel and library no-op/callers. B must integrate both Settings Editor and Plugin Editor (Screen Draw settings) with the transaction.
- Reviewer: three production findings resolved with regression coverage; no remaining substantive A finding. Parent inspected cumulative diff and complete Clippy output for changed-path warnings.
- `cargo check -p multi_launcher --lib --bin multi_launcher`: PASS (initial compiler errors corrected).
- First filtered Nextest run: 14/15 passed after 20m17s build; one fixture selected shared instead of legacy mode. Corrected explicit legacy fixture.
- Final target-limited Nextest: PASS, 15 passed / 5,344 skipped across 2 binaries, 2m12s build; exit 0. Exact command:

```text
cargo nextest run -p multi_launcher --lib --bin multi_launcher -E 'test(preparation_acknowledges_the_typed_request_identity_and_generation) | test(dropping_an_unhandled_request_is_an_explicit_error) | test(unavailable_library_owner_is_reported_without_accepting_a_request) | test(disconnected_receiver_is_not_reported_as_prepared) | test(selected_legacy_route_rejects_keys_the_polling_listener_cannot_map) | test(shared_route_accepts_f13_which_the_native_hook_can_map) | test(enabled_process_bindings_reject_invalid_values_and_cofire_conflicts) | test(bare_caps_lock_preserves_the_legacy_modified_chord_exception) | test(screen_draw_and_radial_direct_triggers_are_checked_against_process_routes) | test(nonconflicting_global_radial_trigger_and_item_shortcut_validate) | test(macro_reservations_use_the_existing_macro_hotkey_validator_without_mutation) | test(altgr_process_route_conflicts_with_unmodified_macro_hotkey) | test(disabling_radial_preserves_invalid_legacy_values_for_later_repair) | test(legacy_start_keeps_supported_saved_bindings_when_one_key_is_unsupported) | test(main_hotkey_preparation_is_dormant_and_does_not_write_settings_or_disk)'
rustfmt --edition 2024 --config skip_children=true src/hotkey.rs src/hotkey/runtime.rs src/hotkey/reconfiguration.rs src/hotkey/validation.rs src/lib.rs src/main.rs src/mkmacro/hotkeys.rs src/mkmacro/runtime.rs src/plugin_editor.rs src/settings_editor/save.rs
git diff --check
```

- Scoped rustfmt and diff check: PASS. Subsequent rustfmt covered corrected main/validation/reconfiguration files with the same options.
- `cargo clippy -p multi_launcher --lib --bin multi_launcher -- -D warnings`: FAIL. First pass found 357 inherited library warnings plus two introduced large-enum issues. Introduced issues fixed by boxing Settings payloads; no code lint suppressions added. Strict command was not rerun after correction.
- Final diagnostic audit: `cargo clippy -p multi_launcher --lib --bin multi_launcher --message-format=short`: exit 0 in 48.50s; 357 library + 13 binary warnings. None remain in new bridge/validator or changed listener/macro hotkey code; main/macro runtime diagnostics are unchanged legacy lines. This is a non-strict audit, not a strict-clean claim.
- Complete audit log: `C:\Users\Jay\AppData\Local\Temp\multi_launcher_up_hk_clippy_20261015.log`.
- Native activation smoke deferred to UP-HK-B, since A only prepares routes.
- Next: UP-HK-B asynchronous acknowledged Save, failure-safe activation and scoped persistence rollback.
