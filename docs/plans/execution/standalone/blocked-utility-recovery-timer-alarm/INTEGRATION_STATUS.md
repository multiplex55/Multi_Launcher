# Utility recovery integration status

Human index only. The [execution ledger](../../../BLOCKED_FEATURE_RECOVERY_TIMER_ALARM_EXECUTION.md) is authoritative.

| Category | Branch / last accepted source | State | Report |
| --- | --- | --- | --- |
| C01 sizing | fix/utility-sizing-20261101-r01 / 3c3d732c07f9fb7ec6fccd8c511c9bf189313311 | PARTIAL: embedded sizes/Reset accepted; native restore-only, automatic capture absent; desktop NOT_RUN | [Native capture boundary](stalls/20261101T145606Z.md) |
| C02 Unicode | feat/unicode-inspector-ui-20261101-r01 / 67117fdfce9e8da20578039fbfea860f8ba71dda | PARTIAL/BLOCKED: row gate exhausted; UI/command preserved and withdrawn; pure domain unchanged | [Unicode final-row stop](stalls/20261101T161624Z.md) |
| C03 ZIP | feat/zip-extraction-20261101-r01 / 5b978524a35d002748ebac6436a6ea94a01332c1 | COMPLETE source/native-file gates:39/39 Windows final matrix, explicit UI and app build accepted; desktop NOT_RUN | — |
| C04 Timer/Alarm | feat/timer-alarm-picker-20261101-r01 / 6989a20c72ab397171c336105dbc57f9a9f1e8c0 | COMPLETE source/test gates:31 owner/route and13 GUI cases across runs, app build/review accepted; desktop NOT_RUN | — |
| C05 closeout | chore/native-acceptance-closeout-20261101-r01 / source f3364722ba0c8e04be9b70fdf48eb7a2805e485e | PARTIAL overall: local test/build/lint/review complete; desktop NATIVE_BLOCKED after one attempt plus one recovery, zero accepted screenshots; owned PID retired. Exact closing tip/draft PR measured after closing commit | [Current capture stop](stalls/20261101T220138Z.md) |

The final cumulative branch supplies the only draft PR. No merge or follow-on Goal is authorized by this index. Final source/file validation passed; desktop observations remain unverified. The closing commit precedes final-tip capture and the single draft PR publication; the post-commit delivery receipt and final response record those actual results.
