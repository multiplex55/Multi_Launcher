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
| UP-HK | in_progress | A `474e6377`, B1 `17ad560c`; B2 tests/review and native acceptance in progress |
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
- UP-HK-B1 committed: `17ad560c` — `fix(hotkey): [UP-HK-B1] make runtime route transitions transactional` (`Checkpoint: UP-HK-B1`). Clean working tree immediately after commit.
- B2 delegated to the same Implementer; root retains ledger/Git ownership. Planner refreshed exact Save/Plugin Editor handoff. No UP-MK or later source work started.

### UP-HK-B2 — in progress

- Immutable editor intent and one LauncherApp-owned Save job; shared worker coordinates prepare, guarded owned-field persistence, Apply and terminal result. Plugin checkbox autosaves use the same coordinator with only enablement/capability ownership.
- Reviewer narrow lifecycle inspection: LauncherApp survives ordinary hiding/dialog close and hidden run_runtime_tick can drain completion. Actual root destruction reaches on_exit; existing main quit path stops services/joins GUI while retaining an unserviced runtime receiver. B2 must mark/resolve owner unavailability before GUI join (or keep servicing), and worker cleanup must not depend on the GUI result receiver. Applied receipts already won must remain authoritative.
- Implementer is mapping exact field/key ownership and integrating these bounded shutdown requirements. No external technical blocker.
- B2 source implementation has started (`src/settings_save.rs` shared job/intent boundary, editors and LauncherApp integration). Worker registry is shared with main for shutdown reconciliation; pending job survives dialog replacement.
- Publication must preserve unrelated post-persistence/pre-terminal writes, while retaining only acknowledged route values; latest disk reads alone cannot prove independently changed hotkeys were activated. Registry must close admission before shutdown snapshot/join to prevent escaped workers.
- B2 draft architecture review found three concrete defects before acceptance: cancellation timeout/disconnection cannot authorize rollback or claim restored routing; latest disk reread must not publish unacknowledged route values; atomic persistence needs the expected invocation baseline conflict guard. Root also required skipping unchanged before/written values during rollback to avoid false conflicts. Writer will correct these within the existing coordinator/bridge boundaries and add focused behavioral coverage.
- Draft-review resolution design: extend existing bridge with typed terminal disposition (Applied, owner-confirmed non-application/restoration, restoration unavailable); transport timeout/disconnection stay distinct. Retain first authoritative outcome and reconciliation ownership until resolved. Add typed invocation baseline guard, safe latest non-owned merge after Applied, and completed editor baseline advancement without losing newer draft edits. These are B2 corrections to the existing path, not a second runtime owner.
- B2 correction source now distinguishes authoritative terminal disposition from transport failures, retains reconciliation ownership after timeout, merges acknowledged owned values with latest non-owned settings, advances editor baseline and bounds owner-loss detection during shutdown.
- Target-limited `cargo check -p multi_launcher --lib --bin multi_launcher` is in compile-fix iteration (initial errors confined to touched integration/rollback code). No B2 acceptance tests or final review claimed yet; no B2 commit and no later milestone implementation.
- B2 target-limited check now PASS: `cargo check --lib --bin multi_launcher`, exit 0; 29 inherited unrelated GUI/deprecation warnings reported. Real-ticket coordinator, hidden/reopened GUI and two-save baseline tests are being added. Final targeted test/review/lint/native gates remain.
- New B2 coordinator/bridge test command is compiling in confirmed live Nextest session `78103`: `cargo nextest run -p multi_launcher --lib --bin multi_launcher -E 'test(settings_save::tests::) | test(reconfiguration::tests::)'`. Writer owns that handle; root does not start overlapping Cargo.
- Narrow review confirmed failed Apply enqueue is provably not accepted, distinct from accepted/wake-failed or lost terminal. Writer adds typed NotEnqueued submission evidence and guarded rollback regression preserving concurrent writes.
- Root prepared an ignored isolated native profile at `target/ui-polish-evidence/native-hk-20261015/settings.json` (UTF-8 without BOM, indexing/plugin directories empty, Ctrl+F2 initial legacy route). Normal repository settings remain untouched; task executable not launched yet.
- B2 focused coordinator/bridge Nextest PASS: `cargo nextest run -p multi_launcher --lib --bin multi_launcher -E 'test(settings_save::tests::) | test(reconfiguration::tests::)'`; 19/19 across two binaries, 1m04s compile / 0.518s execution, exit 0. Initial fixture/timing corrections are superseded by this result (details pending). Reviewer acceptance refresh underway; existing relevant route/editor tests, final lint and executable/native gates remain.
- Initial B2 test failures were diagnostic casing (`apply_never_enqueued...`) and a fixture waiting 1s for a cancellation notice configured for 2s (`apply_timeout...`). Corrected user-facing expected summary and only test cancellation timeout (100ms); behavioral rollback/Applied assertions retained.
- Reviewer confirms tests use production worker/app publication meaningfully and prior baseline/terminal/NotEnqueued/unchanged-field/second-save findings resolved. Remaining corrections: publication must freeze all acknowledged invocation dependencies for every Save source (including unowned Screen Draw chords), make failed-enqueue fixture ordering deterministic, and add one focused pending-app shutdown/registry/deferred-preference regression. Writer is addressing these before acceptance.
- Source-independent publication correction now preserves all acknowledged launcher/quit/help/radial/Screen Draw chord dependencies while retaining latest non-route preferences. Added General unchanged-ScreenDraw and PluginSelection later-hotkey scenarios, plus preparation byte preservation, persistence cancellation and rollback conflict/storage-error cases. Consolidated run encountered one test-only serde trait import issue; writer corrects with existing from_value pattern.
- Reviewer verified lossy terminal wrappers are feature-new and test-only (wait_terminal unused). They are being removed and assertions migrated to explicit typed dispositions, so actual owner tests distinguish confirmed restoration from unavailable restoration. No historical public compatibility path depends on those wrappers.
- Consolidated B2 Nextest now includes pending-app shutdown/worker admission/deferred preferences, typed bridge, main owner/handoff cases and the three approved relevant integration targets. Test-fixture serde conversion and two Option<window_size> type corrections were made before rerun; current writer-owned confirmed live session `98035` (functions cell 854). Exact literal command/result pending completion.
- Root native inventory confirms Multi Launcher and Notepad are not running; no task executable or neutral test window launched yet.
- Reviewer final B2 source review ACCEPTED, conditional on expanded tests/lint/native smoke. All remaining publication, deterministic enqueue, shutdown, and typed-terminal findings resolved; no further substantive defect found. Reviewer diff check passed; no builds/source edits from Reviewer.
- Expanded Nextest compiled selected targets in 5m33s and started 43 tests: 20 passed, 2 failed; remaining cases canceled by fail-fast. Failed cases: `failed_rollback_reports_reconciliation_when_settings_storage_disappears` and `persistence_failure_cancels_prepared_route_before_returning`. Writer is investigating actual error/ordering before changing expectations; this is not an acceptance pass.
- Root requires one declarative General Settings ownership list driving copy/name/rollback helpers in the new coordinator to avoid multiple ownership sources as later branding fields are added. Writer will incorporate this bounded consistency change with corrections and rerun affected checks.
- Corrected filesystem failure fixtures now use a regular file at the expected directory path, so persistence cannot recreate the directory. The two failure tests pass (2/2, 1m18s compile / 0.058s tests).
- Consolidated acceptance-aligned union PASS:43/43 across5 binaries,5354 skipped,2m43s compilation /0.966s execution. Includes shutdown, typed bridge, main owner and relevant settings_editor/hotkey_events/trigger_visibility integration targets. Full literal command pending writer readiness report.
- One General Settings ownership declaration now generates copy, field-name and rollback helpers; parent diff check passed.
- Strict Clippy before final fixes failed on inherited debt and one introduced worker large-enum warning; boxing fixed the new warning. Non-strict JSON audit then exited0,376 warnings; parent independently found no new-file warnings but one changed-hunk spawn_gui14-argument diagnostic (13 at B1). Writer will replace that touched startup signature with a typed input; no suppression.
- Final expanded acceptance union passed 43/43 across five selected targets (5,354 skipped); 2m43s compile / 0.966s execution. Exact filter/result retained in agent evidence; no full-suite run.
- Additional actual main-owner gate: `cargo nextest run -p multi_launcher --bin multi_launcher -E 'test(main_hotkey_apply_publishes_runtime_before_applied_receipt) | test(shared_hotkey_owner_restores_old_route_before_reporting_candidate_failure) | test(shared_hook_failure_installs_legacy_controls_before_terminal_error)'`: PASS 3/3 after correcting successful legacy fallback to RuntimeTransition rather than Rollback disposition (9.86s compile / 0.068s execution).
- Typed GuiStartupServices replaced touched expanded spawn_gui argument list. Final non-strict Clippy JSON audit exited 0, 375 inherited diagnostics, zero changed-line/new-file warnings after parent normalized Windows paths. Log: `target/ui-polish-evidence/hk-b2-clippy-final.jsonl`. Scoped rustfmt and diff check passed. Latest source debug build passed (11.70s).
- A default clipboard_modifiers.json generated by the existing trigger_visibility fixture was hash-backed up under ignored target/ui-polish-evidence and removed from the source tree; original preflight tree had no such user file. No unrelated fixture refactor.
- Native profile launched from isolated target/ui-polish-evidence/native-hk-20261015 directory. Initial sandbox process had desktop access denied and was identity-checked/stopped. Automatically approved normal-desktop launch PID 12616, start 2026-10-15T18:37:10.9198979Z, remains active; normal profile untouched. User confirmed readiness to approve Computer Use access.
- Native capture now fails with `window capture timed out: timed out waiting on channel`; accessibility-only capture succeeds. Click on observed Settings control fails `coordinate input geometry is unavailable`. Keyboard attempts did not change the observed focused search control. User asked to confirm visible isolated window/open Settings manually; native acceptance not claimed.
- Independent narrow review found unused startup-only GUI RegisterHotKey reservations survive Save, despite no WM_HOTKEY consumer. Implementer resumed B2 to retire the competing reservation path at its ownership boundary; source verification/review and final native gate remain pending. No later milestone source changes.

B2 consolidated targeted command (PASS 43/43; underscores verified against the actual invocation):
```text
cargo nextest run -p multi_launcher --lib --bin multi_launcher --test settings_editor --test hotkey_events --test trigger_visibility --no-fail-fast -E 'test(settings_save::tests::) | test(reconfiguration::tests::) | test(radial_route_plan_coherently_hands_off_enable_and_disable) | test(route_start_failures_disable_control_and_restore_legacy_plan) | test(settings_reload_replaces_screen_draw_triggers_without_carrying_edges) | test(shared_disabled_route_still_registers_stable_direct_trigger_identity_only) | test(global_item_shortcuts_require_opt_in_before_reserving_mkmacro_ownership) | test(route_handoff_waits_for_old_release_and_new_admission_is_unambiguous) | test(route_handoff_ack_is_withheld_until_claimed_cycle_drains) | test(new_handles_none_default_hotkey) | test(always_on_top_persists) | test(omni_plugin_settings_round_trip_preserves_all_fields) | test(omni_settings_missing_fields_fall_back_to_defaults) | test(omni_settings_from_editor_are_persisted_and_applied_on_reload) | test(launcher_and_quit_hotkeys_toggle_flags) | test(zero_key_events_toggle_visibility) | test(shift_capslock_does_not_trigger) | test(win_modifier_triggers) | test(visibility_toggle_immediate_when_context_present) | test(launcher_toggle_action_sets_visibility_and_restore)'
```
- Removed only the in-app startup launcher/quit RegisterHotKey calls. Retained public helper/map/cleanup compatibility. Reviewer accepts correction and reports no remaining B2 source findings. Three actual main-owner tests pass after removal (46.9s compile / 0.066s execution); Rust 2024 scoped formatting and diff check pass.
- Final retirement audit: `cargo clippy -p multi_launcher --lib --bin multi_launcher --message-format=json` exited 0; 375 warnings. Parent independently intersected normalized primary spans against all modified Rust hunks and new files: zero hits. Log `target/ui-polish-evidence/hk-b2-hotkey-retirement-clippy.jsonl`.
- Final ordinary executable build PASS: `cargo build -p multi_launcher --bin multi_launcher`, 48.36s; target/debug/multi_launcher.exe, 118,444,544 bytes, 2026-10-15T19:38:29Z. Not yet copied over running isolated process. Source frozen and independently accepted.
- B2 is a coherent source checkpoint: immutable coordinated Save, guarded persistence/reconciliation, acknowledged publication and shutdown lifecycle. Full UP-HK remains in_progress pending meaningful Windows Save/input smoke; UP-MK has not started.
