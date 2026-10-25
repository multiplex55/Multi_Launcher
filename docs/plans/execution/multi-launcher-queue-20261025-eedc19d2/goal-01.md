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
