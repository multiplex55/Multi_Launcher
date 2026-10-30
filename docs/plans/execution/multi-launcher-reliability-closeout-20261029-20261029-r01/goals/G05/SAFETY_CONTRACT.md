# G05 condition-evaluation safety contract

DESIGN_READY_FOR_M1_A; production conditions remain disabled. Baseline d845e0aab65f49913996791e26e8951d69a0637c; branch codex/closeout-g05-hotfolder-condition-evaluator-20261029-r01; authoritative integration5873cb8a73dfa771d59431afe29986cda51b6c93. Read-only SOURCE_OBSERVED; Cargo/native NOT_RUN. Parent persists planner contract.

## Current ownership

model.rs existing InclusiveU64Range/HotFolderConditions/LiteralContentCondition/FileMetadata/MetadataConditionResult/evaluate_metadata own semantics. evaluate_rule_conditions currently refuses configured conditions before event/path filters. runtime.rs HotFolderWatchService owns bounded ingress, settling, epoch and unique HotFolderReadyLease; ready candidate captures rule/fingerprint/store revision/epoch/path/file evidence. execution.rs run_next_ready owns admission/ExecutionControlLease/current rule/action checks/fingerprint/durable intent/completion; TransferToConsumer validates lease but is not success. file_operations.rs NativeHotFolderInput/fingerprint_hotfolder_file/prepare_hotfolder_source/execute_hotfolder_native_action own native admission/built-in effects. capability.rs owns action identity/eligibility/explicit enablement and condition-bearing refusal. store.rs v1/v2/revision/sanitized quarantine. gui/hotfolder_dialog.rs RuleEditor/SampleTestJob/evaluate_sample_test/TestRule; gui/hotfolder_runtime.rs lifecycle/control gates/capability/sample bridge; gui/hotfolder_dispatch.rs actual launcher invocation/MkMacro start/current action checks/completion authority.

## Binding and precedence

Predicate result is descriptive evidence, never authorization. One production evaluation uses same candidate/rule/fingerprint/store revision/epoch/settled root-parent-file identities/length/mtime/captured wall time/control cancellation generation/action authorization. Capture wall time once at admission; monotonic work limits. Validate configuration before inspection, revalidate after inspection/lease transfer and retain required binding through actual effect. Previous match cannot authorize new candidate/rule/epoch/action.

Configured conditions require exact captured store revision even if another row changed it; preserve unconditioned legacy behavior. Existing control gate/lease/native checks/journal/loop limits/completion owner remain authoritative.

Outcome precedence: Invalid > Changed(source/rule/authorization) > Unavailable(evidence/resource) > NoMatch > Match. Nonpersisted typed outcomes/sanitized reasons. Validate every configured field first. Metadata no-match cannot hide configured content admissibility/completeness. Invalid includes malformed rule/range/literal/evaluation input; Changed includes revision/epoch/lease/fingerprint/native mismatch; Unavailable includes missing metadata/invalid clock/unreadable/encoding/resource/cancel/budget/unimplemented evaluator; NoMatch only complete admissible negative; Match all predicates true and required evidence valid. Cancel/budget never match.

## Metadata

Reuse evaluate_metadata numeric logic. Inclusive byte/age bounds, at least one endpoint. Full-precision Duration versus seconds, no fractional truncation. Required mtime missing/future relative captured_now => complete evaluator Unavailable. No age predicate requires no optional mtime. Preserve existing helper API behavior (currently missing/future NoMatch) if practical; full evaluator classifies admissibility before delegation. Configured content stays Unavailable/NotEvaluated in M1-A.

## Later literal contract

Existing persisted enum/4096byte literal limit; nonempty validUTF8/noNUL, exact case-sensitive bytes, no regex/normalization/folding/BOM removal. Complete stream validUTF8/noNUL; nonNUL ASCII controls alone not binary, BOM ordinary. Invalid/incomplete finalUTF8/NUL => Unavailable, no lossy decoding. Initial8MiB admitted length/read budget,64KiB max chunk,2s monotonic deadline below30s lease, bounded overlap/UTF8 carry/no whole buffer. Candidate-wide budget cannot silently multiply across rules; fresh packet specifies bounded selection reuse.

Contains AND DoesNotContain require complete admissible input, even early hit cannot ignore later error. Empty admissible input Containsfalse/DoesNotContaintrue. EOF before settled length or changed final identity/metadata=>Changed. Truncation/read error/cancel/unsupported reader=>Unavailable. Negative match requires complete verifiedEOF/valid encoding/final binding. Between-read deadline checks do not interrupt blocked synchronous native I/O: prove cancellation/retirement before enablement, no detached unowned reader/harddeadline claim. Never log literals/contents/newfullpaths or leak them through Debug/receipts.

## Native and queued effects

fingerprint_hotfolder_file currently prepares guarded source, returns Copy NativeHotFolderInput evidence and drops guard. Built-ins reopen, verify identity/metadata/SHA256 and hold native guard through effect. open_native_replace_file denies write/delete sharing, so holding extra guard beside conflicting reopen is invalid. Later M1-B must hand ownership through existing native owner, no parallel path reader/FFI/sharing weakening. Content/effect same admitted source or equivalently verified transfer.

External requests currently carry evidence values. Actual launcher/macro dispatch validates target config but does not itself establish conditioned-source freshness after queue. Retain/revalidate condition evidence at actual launcher invocation/macro activation using existing cancel/completion; no action authority redesign.

Dated scope clarification(session2026-10-29): parent authorizes necessary narrow existing file_operations HotFolder ownership adapter and actual GUI dispatch boundary for later milestone only. No duplicate reader/FFI/conflicting guard reopen/framework. Native proof uses owned repository fixtures following existing m10c_native_copy_move_rename_collision_and_stream_refusal_stay_in_fixture, not assumptions from separate G02OS5.

## Selection, preview, compatibility and gate

Later first_matching_rule selects first fully matching admissible rule, not first filename match. Unknown/changed never permits falling through to another action. Preserve current unconditioned order. TestRule same pure predicate/separate sample admission; never production lease/action/journal/live watcher. Editor identity/draft revision result authority; obsolete bounded reads cancel/retire. Preserve v1 read-only load/intentionalv2upgrade/defaultdisabled/untouchedrules/existingv2fields/sanitized quarantine. No schema, idle polling, recursive backfill, extra production worker/global config.

Keep FILE_CONDITIONS_NOT_EVALUATED in model/capability/runtime/execution until evaluator/reader/editor/TestRule/native through-effect proof and independent review complete. M1-A foundation cannot authorize conditioned effects.
## 2026-10-29 — ordered M1-B foundation

M1-B is split at its existing ownership boundary. B1 implements pure streaming literal evidence/composition under one bounded literal set, shared8 MiB/64 KiB/two-second monotonic limits. Both modes require complete valid UTF-8/no NUL/genuine EOF/terminal checks; provisional hit never bypasses later validation.

The stream owns no filesystem reader and grants no authority. Its cancellation/deadline checks prove bounded cooperative parser processing only, not blocked native-read interruption/retirement. Current ScanCancellation AtomicBool and synchronous hash_reader checks leave native proof required. Next checkpoint adapts existing Hot Folder owner, never duplicates a reader or treats cooperative checks as sufficient.

Next checkpoint also must hold/verify transfer through effect, avoid guard/reopen sharing conflict and bind queued launcher/MkMacro at actual start with existing authorization/cancellation/completion owners. All FILE_CONDITIONS_NOT_EVALUATED gates remain until M1-C/M2/M3/full independent acceptance. B1 is not native readiness or M1-B/G05 completion. Remaining work stays active G05 scope, not a separate deferral.
