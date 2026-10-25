# Goal 01 execution record

Queue: multi-launcher-queue-20261025-eedc19d2
Goal: CODEX_GOAL_01_DEV_WORKFLOWS.md
Goal SHA256: 41491927E022047B297A5378B059520DFCF5AA09B8FCB5E4FBBBC7499DF1F1D6
Kickstart SHA256: F8597ADC2CB0FACF7393C505B87FABCA6323C8C2533F41BCBFFD06A0DC0707D9
Source branch: master / origin/master
Source base: bbc2ca1ef247c53eeae9f2ce7ba69f381483318d
Task branch: codex/developer-workflows-queue-20261025
Execution checkout: G:\Repos\rust\Multi_Launcher

M0-A establishes this branch baseline before application edits or verification.
Ordered checkpoints: M1-A bounded execution receipts; M1-B build modes and feature selection; M2-A selected-test summaries/replay; M2-B build comparisons; M3-A focused smoke integration.

External ledger owns resulting commit hashes, actual verification receipts, push results and next action:
C:\Users\Jay\.codex\visualizations\2026\10\25\01a17319-6f7d-7052-b54e-1bb4a92c5fd0\multi-launcher-queue-20261025-eedc19d2\QUEUE_STATE.md

Budgets: compilation 30 minutes; selected tests 10 minutes; native post-build 5 minutes and at most three captures.
No application verification has run at baseline creation.

## M1-A — bounded execution receipts

Baseline: 0504a87bcdf0f292ec1534e59158c601d6d74dd7.
Completed the checked shared Job Object owner, compatible legacy/raw wrapper,
and measurement integration. Optional Application events are not queried;
the legacy empty artifact and explicit NotCollected receipt remain available.

Verification: 11 controlled owner cases passed, including the event-provider
non-invocation regression, pre-resume deadline, argument fidelity, exit 7,
descendant retirement, ownership denial, and recorder failures. Measurement
and legacy test-performance compatibility receipts passed and are retained
in the external ledger. No application build or GUI validation belongs to
this checkpoint. All four reviewer findings are resolved; the configured
independent Reviewer accepted the final correction.

The user-authorized fallback permits the parent to write source when configured
Implementer capacity is exhausted, and to perform a labeled structured parent
review when Reviewer dispatch is unavailable. It applies to the full queue;
all functional, safety, verification, and Git requirements remain binding.
M1-A correction 2 used the parent as sole source writer and an independent
configured Reviewer. Original Goal/Kickstart inputs remain unchanged.

## M1-B — explicit build modes and test selections

M1-A commit: f86397762a210c20109628de55aabe80e0bb8af3 (pushed).
Added a declarative six-selection catalog and a single resolver for Preview,
Discover and Run. Existing Cargo presets remain compatible. The facade selects
explicit targets, separates compiler and runner profiles, validates nonempty
exact identities, rejects stale discovery, and uses the shared bounded owner.
Selections: tables, notes, paste, query-focus, settings and integration-query.

Verification: focused facade fixtures passed (10.522 seconds), including an
actual Discover-writer to validator to public-Preview round trip; legacy
performance parser fixtures passed. Final attempt-5 library and pure-target
discovery passed, both matching fingerprint
77633d885a587fcbb5293b54e5f3617c84795361a80336641e27c3077b8dabea.
Both public previews passed without executing a test. The exact table case ran:
1 passed, 6338 skipped, no failures. All final phases verified cleanup and drain.
Detailed receipts remain in external g01-m1b/verification/attempt-5.

Configured Reviewer dispatch was unavailable. The parent performed a separate
structured review under the user's override; this was not independent review.
Corrected optional CLI forwarding, receipt-property casing at the serialization
boundary, and quick-check target metadata. Focused regression coverage passes;
parent review accepts M1-B with no remaining blocking findings.

Earlier cold preparations exited Cargo successfully but failed owned-descendant
retirement checks. Original failures remain retained; the late descendant's
image is unknown. The bounded warm diagnostic and final cached preparations
passed. No owner/grace change, cold-build fix, or speedup is claimed. Native
mapping, result summaries/replay, and comparisons belong to later checkpoints.
Final Goal Clippy remains deferred until all Goal edits and validation finish.

## M2-A — historical interim status (superseded by the final update below)

Last accepted checkpoint: M1-B b63dc7353260aed791b3f15b747515319823aeeb,
normal push succeeded. M2-A changes remain uncommitted and unstaged.
Implemented structured selected-run reduction, distinct binary/case identities,
retry-aware slow cases, immutable original-receipt replay with exact failure
filter and native recording reference, fresh target validation and changed-source
disclosure. Native recording/export/replay interoperability remains unverified.

Focused synthetic evidence (external g01-m2a): reducer and facade passed under
the checked owner. Final review-correction1 facade passed in 14.141 seconds;
review-correction2 reducer passed in 0.786 seconds. Verified drain and cleanup.
The parent reviewed the diff after configured Reviewer dispatch returned
agent thread limit reached; this was parent review, not independent review.
Resolved guaranteed recording-state cleanup on exceptions, realistic suite
started/ok/failed lifecycle, and unknown timeout totals for partial or generic
failures. Original failures and all attempts remain retained.

Mandatory acceptance blocker: native-attempt5 target preparation produced the
fixture binary and Cargo exit 0, but the checked owner returned failure:
owned descendants survived the root process. The 60-second command ended in
6.01189 seconds; all 17 recorded identities were Absent after cleanup, and
drain/cleanup verified. No original test execution, recorded run, export or
replay followed. Short disposable workspace/state cleanup also verified.
No owner/grace changes or further native retries were made.

Resume at G01/M2-A native capture/replay acceptance boundary. First resolve or
attribute the owned-descendant retirement failure within an explicitly bounded
correction, preserving the original receipt and fail-closed semantics; then
obtain actual isolated failing-run and exact replay evidence and review before
any M2-A commit. Do not recreate branch/M0-A or repeat completed verification
without an affected-path reason. Current native evidence:
g01-m2a/native-attempt5/native-fixture-summary.json and its referenced measurement.
M2-B and M3-A remain pending because checkpoint acceptance is ordered. Goals
02-17 remain Not started under the strict cumulative handoff. No FINAL_01 is
claimed. Final Goal Clippy has not run because Goal 01 is incomplete.

## M2-A owned-survivor attribution — one diagnostic adjustment

Previous goal turn was progress. Current read-only reconciliation preserved
HEAD/upstream b63dc7353260aed791b3f15b747515319823aeeb and all partial files.
One bounded cold-preparation diagnostic reproduced the failure without changing
owner code, grace, command semantics, or acceptance policy. Creation-verified
Job snapshots identified VCTIP.EXE as the sole surviving member at root exit
and about 5.02 seconds later. Cargo exited 0; owner correctly rejected the run.
All recorded identities were Absent after cleanup; output drain and generated
temp-directory cleanup verified. No test execution or replay was attempted.
Evidence: g01-m2a/owner-attribution-once/diagnostic.json, including fixture and
owner hashes, exact argv, snapshots and cleanup. Instrumentation was local to
the diagnostic script; no repository source was edited.

The locally installed Microsoft MSVC 14.44 vctip.exe FileDescription identifies
a VC compiler/tools experience-improvement uploader. This contextualizes the
observed basename; the diagnostic did not retain full image path/signature.
Read-only installed-tool inspection found no verified child-scoped compiler
switch preventing this launch. VsDevCmd.bat VSCMD_SKIP_SENDTELEMETRY controls a
separate PowerShell telemetry path, so it was not applied as an unproven fix.
No registry/global preferences, linker choice, environment settings or owner
failure criteria were changed; no name-based exception or cleanup was added.

Same mandatory boundary remains blocked, now attributed rather than unknown.
Smallest next requirement: establish a supported invocation-scoped toolchain
control, or explicitly scope a safe compiler-helper lifecycle contract with
identity verification and negative coverage; never silently exempt the process
by basename or count cleanup-after-failure as acceptance. Existing native proof
and ordered queue handoff remain incomplete. This was the one evidence-backed
attribution adjustment, not permission for further unbounded retries.

## M2-A final evidence update — 2026-10-25

This update supersedes the interim blocked status and the attribution note above;
those entries remain as history of the preserved failures and diagnosis. The
verified MSVC helper lifecycle boundary and replay environment-restoration
boundary were corrected without changing the five-second owner grace or strict
default policy. Existing owner fixtures passed, and the focused structured-
result regression now prevents `Complete` and replay acceptance when recording,
cleanup or environment-restoration errors are present even if test output parses
completely.

Native capture/replay passed in the disposable library fixture at
`g01-m2a/vctip-lifecycle-revision/native-capture-replay-3b`. The exact original
case failed with exit 100 and a complete one-case structured result; the
12,016-byte Nextest recording (run ID
`e3fe9c18-fb77-4517-9296-e7204e6e2c3e`, SHA-256
`c9ba7cd1493443181c0daa24b9a8832d6312cd864ebaba588017812f4d243cd5`) was
replayed after a controlled source fix. Replay selected the same binary and
case with the same options and passed with exit 0 while disclosing changed
source. Original receipt/result/output/archive hashes remained identical;
private replay state and all 270 recorded identity observations were verified
clean. The generated workspace was removed; its supplemental receipt is
`supplemental-fixture-workspace-cleanup.json` in the same evidence directory.
This is bounded fixture proof, not application-suite or GUI acceptance.

The final targeted result-fixture command was run through
`dev/track_b/measure.ps1` with a 120-second test deadline and five-second
cleanup deadline; it exited 0 in 1.89 seconds with cleanup and output drain
verified. Focused owner, environment and auxiliary-lifecycle test receipts from
the earlier M2-A attempts are retained. The parent structured review accepted
the code after the reliability correction; M2-A changes await parent commit and
push. Review evidence is retained at
`g01-m2a/vctip-lifecycle-revision/parent-review.md`. M2-B and M3-A have not
started; final Goal Clippy remains deferred.

#### M2-B execution record (2026-10-25)

The measurement path now captures sanitized source, host, tool, selection,
target-directory, allowlisted environment and Cargo configuration provenance.
The workflow dispatches `quick-check` through the build owner and records the
actual `cargo check --lib` invocation. Comparisons classify incompatible,
missing, stale, failed, noisy and incomplete records without treating unknown
cache/work labels as evidence of an optimization. Evidence retention is a
bounded, report-only census; it does not delete evidence.

Focused PowerShell checks passed for the comparison helper, measurement
compatibility and workflow dispatch. The checked-owner receipts are under
`g01-m2b/verification` in the queue evidence directory. The first actual pair
is retained at
`g01-m2b/actual-pair-20261025-7f2c3a/comparison.json`; it remains classified
incomplete because the Rustc `--version --verbose` probe was passed as one
nested argument and selection fingerprint ordering was unstable. Neither
sample was retrospectively repaired or used for a claim. The probe now passes
the two arguments separately and selection fingerprints are canonicalized.

The corrected representative pair is retained at
`g01-m2b/actual-pair-correction1-20261025-a49d31/comparison.json`. Both samples
used the same public `quick-check` request and exact `cargo check --lib`
command, with no source edits between them. Both completed with the same
source and comparison fingerprints and complete provenance. Their wall times
were 0.895 and 0.867 seconds; work and cache remained unknown, so the result is
compatible but makes no improvement claim. Cargo timing-artifact freshness was
not configured, and these whole-command timings are not linker measurements.
The attached report-only census covered 15 files, 21 entries and 99,322 bytes
without truncation or deletion. See `comparison-command.json` beside the
comparison receipt for the exact comparison invocation and quota.
Review corrections verified the environment snapshot includes inherited `CARGO_BUILD_RUSTFLAGS`, target-specific Rust flags and linker selection, recording only hashes. Difference details now compare each sample with the first valid sample and identify sample index/path. Same-commit controlled source comparisons use actual between-sample file-map changes, so unchanged pre-existing dirty files do not invalidate a declared edit; an undeclared changed path still rejects the group. Focused regression checks passed under the checked owner.