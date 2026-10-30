# G05 M1-A planner handoff

Objective shared metadata outcomes and execution-owned candidate binding, NO configured condition enablement. Scope src/hotfolder/model.rs, execution.rs, minimal mod.rs visibility only if necessary, relevant tests/own docs. No native adapter, GUI, content reader, store-format changes yet. Baseline d845e0aa. Safety contract binding.

1. Extract event/path/filename predicate from evaluate_rule_conditions into shared pure helper, preserve public condition guard/behavior. Never clone rule/clear conditions as bypass.
2. Small nonpersisted typed five-category outcome; existing types/validation/evaluate_metadata, no duplicate numeric implementation.
3. Complete-metadata entry validates first, required missing/future timestamp Unavailable, evaluates scope/ranges, content configured Unevaluated, proper precedence; predicate evidence usable later TestRule.
4. Execution capture wall time once beside cloned candidate/existing control lease; own expected evidence, no registry/persisted token.
5. Production-used private validation boundary around existing postfingerprint/posttransfer admission: current rule, native input root/parent/file/length/mtime versus settled evidence; condition-bearing exact store revision/control generation; preserve legacy/all existing refusal paths.
6. Deterministic fake port/clock/lease tests directly exercise boundary, no test-only gate bypass; condition-bearing run_next_ready still refused.
7. Document native lifetime/queued-through-effect remains M1-B; no through-effect acceptance.

New test prefix hotfolder_condition_metadata_: inclusive/subsecond, invalid precedence over simultaneous stale/unavailable/no-match, stale revision/fingerprint/epoch/control/every source evidence field refuses old match, missing/future age outranks size false, noage doesn't requiremtime, content remainsunevaluated, edited/disabled/stale transfer0effects, ordinary unconditioned same behavior.

Existing focused selection:
- hotfolder::model::tests::metadata_conditions_use_inclusive_bounds_and_one_captured_time
- hotfolder::model::tests::modified_age_bounds_preserve_subsecond_precision
- hotfolder::model::tests::file_conditions_validate_ranges_bound_utf8_and_defer_content
- hotfolder::model::tests::condition_bearing_v2_json_rejects_negative_and_overflow_unsigned_bounds
- hotfolder::capability::tests::enabled_condition_rules_fail_closed_until_the_shared_evaluator_is_installed
- hotfolder::store::tests::legacy_v1_load_is_read_only_and_intentional_save_upgrades_without_changing_untouched_rules
- hotfolder::store::tests::legacy_condition_rows_and_malformed_v2_literals_are_quarantined_without_diagnostics_leaking
- hotfolder::runtime::tests::settled_observation_leases_transfer_without_success_and_recheck_evidence
- hotfolder::execution::tests::m10c_fake_ready_execution_dedupes_partial_outputs_and_caps_runs
- hotfolder::execution::tests::external_terminal_is_correlated_and_finalized_once_by_c

One nonzero Nextest list/run -p multi_launcher --lib --cargo-profile fast-dev --offline --locked -E newprefix|namedtests. Initialprep1200s/builtrun180s/necessary aggregateincremental600s/1-2actualerror localizedreruns. No fulldomain/broad suite/build/install/lint. Exact cmd/version/HEAD+blobs/time/cap/exit/count/warnings/native-vs-deterministic/cleanup receipts; selected-only trackedJSON/raw unselected ignored+hashed. Scoped format/diff/guards checked. App profiles if constructed next/cancel hotkeysNone.

Done selected tests pass/shared semantics+binding covered/legacycompatibility/all configuredconditions refused; parent checkpoints before later packet. Native proof not claimed.

Later M1-B fresh interfaces: bounded literal/native handoff/fullcondition selection/built-in and actual queuedsource/action binding; owned-native source substitution/writer/cancel/read retirement/queued stale refusal. M1-C existing RuleEditor/render_editor gui/hotfolder_dialog.rs/evaluate_sample_test shared predicate/separate reader/draft admission+cancelretire; m10e_test_rule_reads_sample_metadata_without_live_runtime_effects and condition_edit_retires_a_previously_prepared_enable_confirmation. Never livequeue/lease. M2review/proof then M3 controlledgate removal only allpass. Don't preimplement now.