# Validation log

## Preparation status

No Multi_Launcher build, application test, native launch or performance measurement was run to create this planning package. Source/doc inspection is recorded in SOURCE_BASELINE; it does not constitute implementation acceptance.

Use the evidence template for actual runs, then index them here.

## Execution receipts

| Evidence | Checkpoint | Observation | Scope |
| --- | --- | --- | --- |
| G00-A-GIT | G00-A | Fetched `origin/master`, checked refs and clean checkout, recorded tool versions, committed and pushed `3e67f7396f127d6dcb7bec432e9bb1684e9e6ae3`. `git diff --check` passed for the checkpoint. | Branch and baseline only. |
| G00-C-DESKTOP-PROBE | G00-C | An existing `target/release/multi_launcher.exe` copy, SHA-256 `401A5899B65FDC958DF56CEECC9FA083057E889F322E6925063DAC30D6F9E468`, started bare in a disposable root with synthetic empty actions and both Paste Sequence shortcuts disabled. `Get-Process` reported session 1 and title `Multi Lnchr`. The Windows control surface listed live windows but did not expose the copied launcher or its title. Stopped only the verified owned fixture PID and confirmed exit. | Process startup observation only. Provenance was not established for the existing executable. No input, visual, clean shutdown, baseline acceptance or GPUI evidence. Recheck with an identified G01 binary. |

## Execution receipt fields

Record evidence ID; goal/checkpoint and GUI/criterion IDs; actual branch/commit or dirty source fingerprint; environment; package/target/profile/features; exact command/steps; selected test names/count; build/link and execution durations separately; pass/fail/timeout/zero-selection/skipped counts; observed result; artifact identity; diagnostics and cleanup; applicable native/visual limits; reviewer disposition and evidence freshness.

Never fill a result from a planned command or old historical ledger. Use a compact index with links to substantive evidence, avoiding copied raw logs. Keep private user content out of records.
