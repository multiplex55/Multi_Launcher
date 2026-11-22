# G02-C/D Hash adapter review

The independent review covered the production changes to Hash worker publication, shared command admission, egui host behavior, and GPUI session and native window lifetime on the source base `d942d9b65`. The source corrections passed the scoped tests and native preview checks. See `G02-C-D-FOCUSED` and `G02-C-D-GPUI-HASH` in the [validation log](../VALIDATION_LOG.md).

| Area | Review evidence |
| --- | --- |
| Command admission | Both frontend entry points use the typed `HashGeneratorCommand::Open` handler. An unavailable host returns an error. The egui host continues to ensure and focus its real panel. |
| Session invalidation | The GPUI input change subscription sends the current text to `HashSession`. The authoritative generation rejects stale worker output and clears a digest when its input changes. |
| Publication | The worker updates its latest progress snapshot before wake. The terminal event or sender disconnection is observable when the callback runs. The callback-order tests inspect the job at notification time. |
| Receiver lifetime | A coalescing signal wakes the GPUI task without a blocking executor thread. The task holds a weak root and the signal closes during teardown. The root continues to poll a retiring job while its utility is closed. |
| Native activation | Reopening uses a checked HWND with the existing foreground and revision guards. Initial Kit creation succeeds when the window opens. A later attempt to activate a tool without a checked handle reports a distinct failure. Closing the Hash utility, the root or the app revokes queued Hash activation. The Windows smoke exercised retained reopen, fresh reopen and process exit. |

The typed Text/SHA-256 preview is the scope of this checkpoint. G04 owns full Hash controls, docked presentation, native file and clipboard behavior, Activity Center integration, and final GUI acceptance.
