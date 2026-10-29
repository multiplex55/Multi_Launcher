# M1-B — query PR publication and merge gate

Actual PR: [#1456](https://github.com/multiplex55/Multi_Launcher/pull/1456), open from `codex/integrate-query-escape-20261029-r01` to `master`. Published M1-A head is `0035d85d39d0fa27583547764969fdf362ea1abc`; base `59156d96adcc73939c7171487a0cb582e3286587`. M0-A and M1-A pushes succeeded. The PR is attached to this chat.

[Host publication receipt](receipts/pr1-publication.json) records clean mergeability, all 23 changed paths, zero reported commit statuses/check runs, and no GitHub review submissions. This means **no checks reported**, not CI passed. The separate [integration review](roles/reviewer/PR1_REVIEW.md) is ACCEPTED; it is not represented as a GitHub approval by another account. The actual three Rust blobs and baseline match the reused historical 11/11 Nextest/scoped Clippy proof.

Visible master ruleset 6667862 protects deletion and non-fast-forward updates only. Legacy branch metadata reports protection disabled/no required contexts, while detailed legacy protection read returns 403 to the connector. No settings, access or bypass behavior is changed. Normal merge-commit strategy is enabled. GitHub reports `mergeable=true`, `mergeable_state=clean` for the observed head.

This M1-B checkpoint adds only publication/progress receipts. After its normal push, the parent checks the resulting actual head and host gate, then calls normal merge with that exact expected head. A successful merge SHA cannot be written into its own preceding commit; the next M2-A receipt/final host report records the observed result. Until then this file accurately reports **OPEN, normal merge pending**, not MERGED.

If the normal host merge is blocked, preserve this published PR and proceed with PR2 stacked on its published integration tip, recording the actual gate. No admin bypass, forced merge, branch deletion or routine mid-plan user confirmation.

## Subsequent merge disposition

Normal merge of the final pre-report head `8bda3294636afb62a8782cbcc062504d663cc49b` was rejected by automatic approval review, citing earlier no-merge authorization. No bypass/retry followed. Host re-read confirmed OPEN/merged=false and unchanged master. [Full report](stalls/G03-M1-B-20261029T212244Z.md) was published as `ed25f9dcc32f9bb74d2277d25d6fb62e292636c6`, the actual PR1 tip used as PR2's stacked base. No merge SHA exists; final direct approval is deferred until both PRs are review-ready. This new note is on the documentation branch, preserving the original pre-merge receipt.
