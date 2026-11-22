# Hash Generator

G04 owns `hash-generator`. Source confirmed at the execution parent. Native baseline and GPUI acceptance are pending.

## Routes and host

The `hash` plugin query emits `hash:open`, which dispatches `HashGeneratorCommand::Open` through the dialog host. File Search Calculate Hash and Downloads Triage supply a selected path. They must open in File mode with the original selected path. Reopening an active panel for the same path keeps its session, and supplying a different path invalidates the current calculation. See `src/plugins/hash_generator.rs`, `src/commands/handlers/hash_generator.rs`, `src/gui/command_host.rs`, `src/gui/mod.rs`, `src/gui/render.rs`, and `src/gui/downloads_triage_runtime.rs`.

`HashGeneratorWindowHost` uses utility key `hash-generator`. Docked and independent popout modes share a retained tool session. The popout has a 620 × 440 initial logical size, 420 × 320 minimum and scrolling. Host changes retain the draft and job. Close ends the session and invalidates the active request. The host owns its native file picker and Jump Mode targets, including the two dropdowns and their nested focus boundaries. See `src/gui/utility_window_host.rs` and `src/gui/hash_generator_dialog.rs`.

## Controls and outcomes

| Criterion | Source-backed baseline |
| --- | --- |
| HASH-01 | Source dropdown selects Text, Clipboard or File. Text starts selected and focuses a multiline editor. Hash the exact UTF-8 bytes. A new session clears prior text, clipboard snapshot, file path, expected digest and result. |
| HASH-02 | Algorithm dropdown selects SHA-256, SHA-512 or SHA-1. Default is SHA-256. Show the existing SHA-1 warning. |
| HASH-03 | Load Clipboard explicitly snapshots text, selects Clipboard mode and gives feedback. Show snapshot byte length. Report read failures and preserve the valid prior snapshot. |
| HASH-04 | File mode shows a read-only path and Choose File opens the originating window’s native picker. Picker dismissal preserves the draft. Report missing source or file errors. |
| HASH-05 | Expected digest is optional. An edit clears prior error and clipboard feedback and recomputes comparison without replacing the result or restarting a worker. Show match, mismatch or validation error. Respect existing hex length/case and ASCII whitespace rules. |
| HASH-06 | Calculate uses the shared `HashRequest` and `HashJob`, registers the existing Activity Center operation when available, and exposes byte progress. Cancel invalidates the result. While a cancelled worker drains, show stopping state and keep Calculate disabled. Edits that change the selected input, mode or algorithm invalidate earlier results. Close, reopen and late completion cannot publish into a replacement session. |
| HASH-07 | Show digest and Copy Digest after a successful current request. Clipboard error leaves the digest usable and displays feedback. There is no copy while a request is running. |
| HASH-08 | The Close and Dock/Pop Out buttons, native close and keyboard routes retain their respective behavior. Verify return to parent, hidden root and reopen. Respect the shared presentation preference and preserve Jump Mode and ordinary keyboard focus. Draft and results are session data. |

## Verification

Use the existing Hash Generator unit tests for exact byte vectors, request lifecycle and errors, and the typed dispatch and file-origin tests for the entry paths. Select the `multi_launcher` library target before the `hash_generator` filter. Compare both actual frontend adapters with synthetic text and a disposable file. Run a scoped native test with isolated clipboard data for snapshot/read/write feedback. Exercise mutation while a job is active, popout with hidden root, cancellation and reopen, and compare docked and narrow popout presentation, focus and accessible control names. Establish result and lifecycle parity before accepting any row.
