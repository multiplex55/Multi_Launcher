# B5 History verification

State FAILED_VERIFICATION / source UNACCEPTED. Exact package/library/default-feature locked/offline fast-dev Nextest filter (8new+7controls) and source hashes are in [initial receipt](roles/implementer/N0_list.receipt.json), [corrected receipt](roles/implementer/N0_list_corrected.receipt.json), [final receipt](roles/implementer/N0_list_final.receipt.json). Commands selected the intended names by source inspection; no successful inventory or test execution was produced. Zero executed tests is NOT PASS.

| Phase | Observed result | Bound |
|---|---|---|
| Initial list PID23012 | test helper/type scope compiler failures; numeric exit UNKNOWN;233.5s through output-drain observation |1200s|
| First correction list PID30380 |121.0s, exit101; E0616 test-only private active_bytes access |original supervisor1200s mistakenly configured; known-PID watcher enforced600s|
| Final allowed correction list PID15308 |426.8s, exit-1, no JSON; deadline-triggered Kill(true) succeeded, output drained |stricter427s cap|
| Exact15case run |NOT_RUN; inventory NOT_PRODUCED|180s unused|

The [authoritative reconciliation](roles/implementer/N0_final_disposition.json) explains raw receipt fields without overwriting them. Final primary timed_out=false means the separate tree watcher killed two seconds before its loop deadline; effective disposition is TIMEOUT. Raw aggregate_prior_elapsed173.0 includes about52s idle editing; actual correction-process time121.0+426.8=547.8/600s,52.2s unused. Two localized actual-error correction preparations were consumed; there is no further attempt. Initial numeric exit remains UNKNOWN; later primary numeric exits were retained. Source stayed frozen through each command.

One substantive named review correction addressed tooltip diagnostic leakage, fixture/type errors, default-newest selection and clear accounting. Reviewer statically confirmed the final five hashes; that is not qualifying test/feature acceptance. New candidate private_interfaces warning retained in raw log, not mislabeled baseline. Native/app/full-suite/clippy/install NOT_RUN.

Known direct PIDs23012/30380/15308 absent; output drained. Supported process-tree kill returned success without denial, but complete descendant absence remains UNVERIFIED. No native/app/clipboard/file fixture created. Exact all-five raw sources and reversible patch preserved before scoped rollback; [precheck](PRESERVATION_AND_ROLLBACK_PRECHECK.json), [rollback](ROLLBACK_AND_CLEANUP.json). Production source now matches run-startf28355df and M0; candidate warning/code not carried.
