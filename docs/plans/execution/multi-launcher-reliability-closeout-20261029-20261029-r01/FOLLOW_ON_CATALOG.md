# Follow-on catalog

Only source-supported unresolved findings receive complete Goal/Kickstart pairs.

| Finding | Priority / confidence | Source / role | Disposition / dependency | Canonical future pair |
|---|---|---|---|---|
| File Search sender fixture | P1 / high | G01 implementer Astra/xhigh requested; baseline8d183f66 | PROPOSED_NOT_EXECUTED; independent test-only migration, no G01 runtime dependency | [Goal](goals/G01/follow-on/file-search-sender-fixture/GOAL.md) / [Kickstart](goals/G01/follow-on/file-search-sender-fixture/KICKSTART.md) |
| G01-R1 mutation gate | P1 / high | G01 reviewer Astra/max requested | RESOLVED_IN_G01 at6c6812f0, confirmed independently; do not schedule | [Closed contingency](goals/G01/roles/reviewer/follow-on/folder-rescan-operation-gate/GOAL.md) |
| G02 native parent admission proof | P1 / observed failure high; root cause unknown | G02 implementer; reviewer agrees documentation-only deferral | PROPOSED_NOT_EXECUTED; G02 deferred, G03 blocked; donor source unaccepted | [Goal](goals/G02/follow-on/native-parent-admission-proof/GOAL.md) / [Kickstart](goals/G02/follow-on/native-parent-admission-proof/KICKSTART.md) |
| G03 conditional full-file operations | P2 / hard dependency established | G03 planner/implementer/reviewer sequential | DEFERRED_DEPENDENCY; require all G02 accepted reviewed source, fresh G03 accepted base/M0; existing full contract retained, no duplicate pair | [Goal](../../reliability-feature-closeout-20261029/goals/G03-table-full-file-operations/GOAL.md) / [Kickstart](../../reliability-feature-closeout-20261029/goals/G03-table-full-file-operations/KICKSTART.md) |
| G04 native gesture/output proof | P1 / API gap high; no defect inferred | G04 planner, parent capability receipt, reviewer corroborates protocol | NATIVE_PROOF_DEFERRED / PROPOSED_NOT_EXECUTED; require supported secondary drag, exact binary, isolation, actual Save/Copy pixels and cleanup | [Goal](goals/G04/follow-on/native-gesture-output-proof/GOAL.md) / [Kickstart](goals/G04/follow-on/native-gesture-output-proof/KICKSTART.md) |
