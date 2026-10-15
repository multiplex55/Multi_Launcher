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
| UP-HK | in_progress | UP-HK-A committed `474e6377`; UP-HK-B delegated |
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
- Implementer is sole source writer. A is reviewed and committed; B is in progress and must stop for review and commit before UP-MK.
- Reviewer pre-review identifies release draining, stale generations, rollback field ownership, pending Save while dialog hidden, route symmetry, and actual Save behavior as required review points.
- Existing `Settings::update` lock must not span runtime acknowledgement; rollback must preserve unrelated concurrent settings writes.
- Existing route handoffs must be reused, including waiting for owned release. A is not completion of the full UP-HK behavior.
- UP-HK-A committed: `refactor(hotkey): [UP-HK-A] introduce checked runtime reconfiguration requests`. Full UP-HK remains in progress; GUI Save activation belongs to B.
- A draft review found radial self-reservation rejection (P1), strict preparation dropping valid startup bindings alongside unsupported saved keys (P1), and AltGr reservation conversion bypassing macro conflicts (P2). All three were corrected with regressions before commit.
- A compile command: `cargo check -p multi_launcher --lib --bin multi_launcher`: PASS after correcting import, obsolete no-op caller, key mapping type, and listener closure ownership errors.
- Reviewer refresh confirmed all three production findings resolved with focused regressions. Two reversed message assertions were also corrected. Final A verification passed as recorded below.
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
- Complete audit log: `target/ui-polish-evidence/up-hk-a/clippy.log` (preserved from `C:\Users\Jay\AppData\Local\Temp\multi_launcher_up_hk_clippy_20261015.log`; local ignored evidence).
- Native activation smoke deferred to UP-HK-B, since A only prepares routes.
- Next: UP-HK-B asynchronous acknowledged Save, failure-safe activation and scoped persistence rollback.
- UP-HK-A result commit: `474e6377a439638068bc6de44878d693401c6488` — `refactor(hotkey): [UP-HK-A] introduce checked runtime reconfiguration requests`.
- UP-HK-B is delegated to the same sole Implementer. Settings Save and Plugin Editor must publish only after final acknowledgement; native smoke follows executable build. No later milestone implementation started.

### UP-HK-B — in progress

- Chosen sequence: immutable Save intent and lifecycle owned by LauncherApp; shared request identity progresses through Prepare, guarded persistence, Apply, and final Applied acknowledgement. Main retains old authoritative runtime while preparing dormant/admission-gated candidates.
- Reviewer architectural check: viable if terminal cancellation distinguishes never-applied from already-applied; all candidate input/control/timer admission is gated; old resources survive until successful swap; independent radial handoffs serialize; per-field rollback preserves concurrent writes and reports conflicts truthfully.
- Failure handling must also reconcile non-invocation settings persisted before an Apply failure, so disk and GUI/plugin/theme values cannot silently diverge.
- Deterministic tests must cover cancellation both before Apply and after swap, persistence/rollback failures, unrelated concurrent writes, hidden/reopened dialog lifecycle, and actual disk/effective/GUI outcomes.
- B bridge draft review found failed-wake Apply acceptance ambiguity (P1) and retained terminal sender hiding runtime disappearance (P2). Implementer must resolve both with ordering/disconnection regressions before acceptance.
- Bridge fixes add owner liveness, accepted-but-unwoken Apply semantics, and a terminal-channel recheck after observing owner death so queued Applied receipts win over disconnection. Final B1 review/verification remains pending.
- Interim bridge verification: `cargo nextest run -p multi_launcher --lib -E 'test(hotkey::reconfiguration::tests::)'`: 7 passed, 2m39s compile / 0.063s execution. This is not the final B or full UP-HK gate.
- B is split at a natural architectural boundary: B1 completes the compiling/tested main runtime transition owner; B2 integrates GUI Save/persistence coordination. Both remain within UP-HK and execute sequentially with review/commit between coherent checkpoints.
- Planner confirmed inert preparation uses disabled launcher, empty item inputs and empty related bindings; prepared service stays outside authoritative normal feedback/session/control routing. Existing service handoff needs an inert release-drain barrier before candidate activation. Rollback must retain/prepare old legacy resources and avoid duplicate held-key edges.
- For old shared-service drain, retain authoritative quit/help/Screen Draw emergency controls while suppressing launcher/direct/item admission; fresh staged service remains fully inert.
- Confirmed owner API gap: existing RouteHandoff acknowledges only after owned releases drain. B1 needs a correlated policy-installed acknowledgement for bounded restoration while retaining the matching release; drained completion still gates legacy activation.
- B1 draft review requirements pending resolution: competing radial publications must not overwrite transaction ownership; terminal failure follows restoration/cleanup; held legacy cycles must not re-fire after fallback; unrelated Settings writes must not cause whole-snapshot stale rejection; stale feedback/session commands must not repopulate quiescent service state.
- B1 binary check: `cargo check -p multi_launcher --bin multi_launcher` passed; introduced compiler warnings were corrected during iteration.
- Five supporting tests passed after migrating the old dropped-ticket cancellation fixture to explicit acknowledged cancellation. Command: `cargo nextest run -p multi_launcher --lib --bin multi_launcher -E 'test(reactivated_listener_suppresses_an_already_held_chord_until_release) | test(route_handoff_ack_is_withheld_until_claimed_cycle_drains) | test(shared_route_drain_keeps_old_process_controls_without_direct_radial_admission) | test(main_hotkey_preparation_is_dormant_and_does_not_write_settings_or_disk) | test(radial_route_plan_coherently_hands_off_enable_and_disable)'`. Final rerun: 5 passed, 0.095s execution / 10s incremental build.
- Those five tests are insufficient for B1 completion. Implementer is adding real production-owner Apply/Cancel/restore/terminal-order/publication-deferral tests, with only native handoff issue/poll effects injected. B2 persistence/UI tests remain later scope.
- Provisional Clippy audit: `cargo clippy -p multi_launcher --lib --bin multi_launcher --message-format=json` exited 0; `target/multi_launcher_hk_b1_clippy.jsonl`. Initial changed-path filtering missed Windows backslash paths. Parent normalized paths and found new main large-enum/argument-count/collapsible-if diagnostics; Implementer corrected those without suppressions. Final audit remains pending after owner seam/tests. Launcher invocation diagnostics were checked against unchanged source and classified inherited.
- Final B1 draft review found full-Settings Prepare/Apply equality still rejected compatible unrelated concurrent writes; required invocation-dependency comparison and publication of the latest merged candidate.
- Reviewer also found shared-hook failure could stop the old shared service with an empty legacy fallback while claiming restoration. Required a usable old legacy fallback or explicit truthful inability, plus a production-owner regression. B1 commit remains held until corrected and verified.
- Reviewer final B1 draft verdict: not ready. Additional corrections required: native handoff receipts must publish before their wake; preserve committed RadialInvalidate/tooltip diagnostic publication and existing preview semantics; fake handoff tests must assert actual issued candidate/restoration policies. Earlier competing-publication, old-priority-control, cleanup ordering, held-key and feedback-isolation findings were resolved.
- Consolidated B1 corrections are in place. Reviewer refresh confirms invocation dependency comparison/current-candidate publication, hook-loss legacy fallback, receipt-before-wake, recorded-policy assertions and RadialInvalidate/preview preservation corrected. One final parity detail remains: native-preview diagnostics must retain AuthoringReply::NativePreviewDiagnostics correlation to the active editor lease/session; writer is correcting it after the current build.

### UP-HK-B1 acceptance evidence

- Runtime-only checkpoint: typed Apply/Cancel/terminal receipts; one main-owned drain/activate/restore transaction; native policy-installed acknowledgement separate from owned-release drain; queued radial changes serialized behind the transaction.
- Compatible latest Settings merge is revalidated against actual route dependencies and published at commit. Unrecoverable shared-hook loss builds usable legacy process controls, or explicitly reports unavailable fallback for unsupported legacy keys. Normal shared rollback retains the shared route.
- Restored original radial invalidation and tooltip diagnostics, including lease/session-correlated authoring preview replies. Reviewer final bounded refresh: no remaining B1 findings.
- Final targeted Nextest: PASS, 8 passed / 5,361 skipped, 0.276s execution; run `c23a15e8-b163-4b89-bcb4-90edd842672a`. Exact command:

```text
cargo nextest run -p multi_launcher --lib --bin multi_launcher --test-threads=1 --no-fail-fast -E 'test(main_hotkey_preparation_is_dormant_and_does_not_write_settings_or_disk) | test(main_hotkey_apply_publishes_runtime_before_applied_receipt) | test(shared_hotkey_owner_restores_old_route_before_reporting_candidate_failure) | test(shared_hook_failure_installs_legacy_controls_before_terminal_error) | test(unsupported_legacy_control_does_not_reject_a_shared_route) | test(route_handoff_ack_is_withheld_until_claimed_cycle_drains) | test(shared_route_drain_keeps_old_process_controls_without_direct_radial_admission) | test(radial_route_plan_coherently_hands_off_enable_and_disable)'
```

- Owner test includes unrelated latest-field publication and rejected changed route dependency; shared tests assert issued candidate/restore policies, terminal ordering and effective legacy controls after hook loss. Two test-only generation type errors were corrected before the successful run.
- Scoped rustfmt and parent/Reviewer `git diff --check`: PASS. No new lint suppression, debug print or temporary stub found in added Rust lines. Final Clippy audit pending below.
- Full UP-HK remains incomplete: B2 must integrate immutable GUI Save, guarded persistence/rollback and native smoke before any UP-MK implementation.
- Final non-strict audit: `cargo clippy -p multi_launcher --lib --bin multi_launcher --message-format=json` exited 0; log `target/multi_launcher_hk_b1_clippy_20261015.jsonl`, 376 warning diagnostics. Parent independently normalized Windows paths and intersected primary warning spans with all changed Rust hunks: zero changed-line warnings. Remaining warnings in these files are unchanged legacy statements/functions; no new suppression. Strict audit failed on inherited library debt (357 errors), not a strict-clean result.
- Source frozen for checkpoint. Next: B2 immutable Settings/Plugin Editor Save and guarded persistence/rollback, with GUI completion while hidden and native smoke.
