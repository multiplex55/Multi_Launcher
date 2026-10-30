# G05 M1-B1 — bounded literal evidence

Date: 2026-10-29
Status: READY_FOR_IMPLEMENTATION
Goal branch baseline: b14967e5a1eaddfad9290da2847ec90997e5bb53
Accepted M1-A source: 8b4d01a4cff48c2bdfaa30f760c03e91cbb6f9be
Authoritative integration receipt: 278467db
Dependency: accepted M1-A; commit this amendment before source work.

## Objective and current ownership

Implement one pure, bounded streaming literal evaluator and compose its descriptive evidence with accepted metadata/binding outcomes. No filesystem read or conditioned-rule enablement. B1 establishes the interface for the subsequent native owner; M1-B/G05 remain incomplete until native cancellation/retirement, source-to-effect ownership and actual external start binding pass.

model.rs owns LiteralContentCondition validation, HotFolderConditions, ConditionEvaluation, ConditionUnavailable, ConditionBinding, evaluate_rule_metadata and evaluate_rule_scope. Metadata currently returns Unavailable/Unevaluated for content before any scope/size no-match. execution.rs::CandidateConditionEvaluation already binds candidate, fingerprint, store revision, epoch, settled native evidence, captured wall time and control generation after fingerprinting/actual lease transfer.

file_operations.rs::ScanCancellation is AtomicBool; hash_reader checks between synchronous Read calls, not blocked-read interruption. prepare_hotfolder_source retains GuardedReplaceFile; fingerprint_hotfolder_file returns copied evidence/drops guard; execute_hotfolder_native_action_windows reopens/compares fingerprint. open_native_replace_file requests DELETE/read with FILE_SHARE_READ, so retaining a second guard beside reopen conflicts. gui/hotfolder_dispatch.rs owns PendingLauncherAction, complete_launcher_actions, invoke_registered_action_from_catalog and dispatch_mkmacro; copied evidence/target checks do not prove conditioned source freshness.

## Source scope

One writer owns private src/hotfolder/conditions.rs, its private mod.rs declaration and narrow model.rs composition/tests. No production execution/runtime/capability/store/native reader/GUI/editor/Test Rule changes. Existing execution tests may change only for necessary internal signature migration, preserving behavior. No dependency/schema/public API/framework/filesystem/FFI/action-authority expansion.

## Required changes

1. Reuse LiteralContentCondition validation, MAX_RULES and MAX_CONTENT_LITERAL_BYTES. Admit a bounded literal set from one snapshot; validate before scan, never allocate from unbounded caller count.
2. One scan feeds each chunk once for the set. No full-file restart or fresh budget per rule. Keep bounded matching state, UTF-8 carry, patterns and provisional hits; no whole-file retention.
3. Enforce admitted length/total content <=8 MiB, chunks <=64 KiB, one fixed two-second monotonic candidate deadline. Check cancellation/deadline during bounded processing and before final evidence. Deterministic clock/cancel tests; no deadline renewal, budget reset or revival of terminal failure. B1 limits parser work only.
4. Require complete valid UTF-8/no NUL; carry split sequences, refuse incomplete EOF/invalid UTF-8/NUL as Unavailable. Other valid ASCII controls allowed; BOM ordinary; exact case-sensitive matching, no normalization.
5. Provisional hits only until genuine EOF plus terminal checks. BOTH Contains and DoesNotContain require complete validation even after an early hit/disqualifying hit. Empty valid input Contains=false/DoesNotContain=true. Premature EOF/positive length or source change => Changed. Missing EOF/supplied read failure/resource/deadline/cancel/unsupported acquisition => Unavailable. Expected length or exhausted budget never imply EOF. Caller explicitly supplies end-of-stream; unit simulation is not native completion proof.
6. Completed evidence belongs to exact admitted literal set. No substitutable free-standing matched bool. Private construction through successful finalization only; descriptive evidence, never authorization.
7. Extend existing unavailable reasons only as necessary. Reuse ConditionEvaluation/ConditionBinding and common pure path: Invalid > Changed > Unavailable > NoMatch > Match. evaluate_rule_metadata keeps accepted behavior with no content evidence. Preserve validation/scope ownership; never clone/clear content as bypass. Known content/source Changed outranks missing metadata; unavailable content outranks size/scope/age no-match. No complete Match without all required stages.
8. Preserve literal-redacting Debug behavior. New diagnostics/Debug/test failure context contain no literal text, contents or full paths.

## Invariants

Public evaluate_rule_conditions still refuses every configured condition using FILE_CONDITIONS_NOT_EVALUATED. Capability/watcher/execution/editor/Test Rule gates remain closed. M1-A binding, actual lease transfer, captured wall time, exact configured revision/control generation intact. v1/v2 and unconditioned behavior unchanged. No action/journal/production lease/external launch/native read. Stream completion certifies neither native identity/retirement nor authorization.

## Meaningful tests

New prefix hotfolder_condition_literal_; compact table cases where useful:
- Cross-chunk literal/UTF-8 boundaries, exact case, BOM, empty, both modes, non-NUL controls.
- Early hit followed by bad UTF-8/NUL/supplied read failure/cancel/deadline/premature EOF: no usable prefix result for either mode.
- Exact bounds/one-over, several literals sharing one budget, no reset after hit, terminal failure cannot revive; fake clock, no sleep.
- Missing EOF/expected-length mismatch; evidence rejected for another literal set.
- Precedence: invalid plus stale, Changed plus unavailable content, unavailable content plus metadata/scope no-match.
- No sensitive diagnostics/Debug.
Reuse metadata/guard coverage, do not duplicate its matrix.

## Verification and done

Use cargo nextest list/run -p multi_launcher --lib --cargo-profile fast-dev --offline --locked, list --message-format json and run --retries 0 --no-fail-fast. Same nonzero filter:

test(hotfolder_condition_literal_) | test(hotfolder_condition_metadata_) | test(=hotfolder::model::tests::metadata_conditions_use_inclusive_bounds_and_one_captured_time) | test(=hotfolder::model::tests::modified_age_bounds_preserve_subsecond_precision) | test(=hotfolder::model::tests::file_conditions_validate_ranges_bound_utf8_and_defer_content) | test(=hotfolder::capability::tests::enabled_condition_rules_fail_closed_until_the_shared_evaluator_is_installed)

Initial preparation1200s, run180s, aggregate necessary incremental600s, only1–2 actual-error localized reruns. Record selected inventory/exact source/durations/results/process retirement. No domain/full suite/build/install/broad lint/native probe/app fixture. Inspect diff/retained gates; parent checkpoint/push before native migration.

Done: bounded stream/common composition pass; both modes need complete admissible terminal evidence; shared limits/fixed diagnostics tested; metadata/binding and all gates intact; only owned edits. Report B1 only, not M1-B/C/M2/M3/G05 complete.

## Required next packet — native acquisition/effect binding

Not authorized for B1 implementation. Connect B1 through existing file_operations native owner and execution HotFolderOperationPort:
- Admit/verify settled root/parent/file before read.
- Same candidate byte/deadline budget covers acquisition AND scan; preliminary hash pass not exempt.
- Prove in-flight read cancellation and retirement of every reader/I/O resource. Between-read checks, timeout-only returns, detached threads/dropped receivers insufficient.
- Hold source through effect or verify transfer in same owner; built-ins consume ownership without conflicting second DELETE reopen.
- Keep rule/revision/epoch/control/source binding through built-in effect AND actual external start.
- Launcher checks at complete_launcher_actions/invoke_registered_action_from_catalog immediately before dispatch.
- MkMacro checks at dispatch_mkmacro/start_hotfolder_macro/activate_if_not_cancelled; preserve stop_if_current and terminal completion authority.
- Preserve unconditioned paths, authorization, dedupe, rate, journal and completion.

Use existing file_operations::tests::m10c_native_copy_move_rename_collision_and_stream_refusal_stay_in_fixture pattern under target/productivity_expansion/hotfolder_native. Prove successful guarded scan/effect, denied incompatible sharing, source/parent substitution refusal, cancellation/deadline retirement/resource release. Hold queued external request, invalidate rule/control/source, require zero launcher/macro start and one correlated terminal, plus fresh success control. Actual Windows owner required; fake blocked parser/pre-cancelled request cannot prove native blocked-read cancellation. No impossibility established; concrete contradiction => exact continuation package, no weakened contract/G02 inference.