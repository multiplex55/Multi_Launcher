# Hash Generator

G04 owns `hash-generator`. Source confirmed at the execution parent. Native egui evidence covers the keyboard route, independent popout, Text calculation, comparison edits, all algorithm options, SHA-1 warnings and Unicode multiline comparison, plus native picker dismissal and empty-path validation. Selecting a file, clipboard actions and remaining native baseline checks are pending. See the [validation log](../VALIDATION_LOG.md).

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

## Kit 0.10.0 implementation mapping

- Retain a `TextareaState` for Text and an `InputState` for Expected digest. Subscribe to `InputEvent::Change` and pass the value unchanged to the session. Recreate window-bound control entities on rehosting and seed them from the same session. Use `set_value` for a genuine reset or bind rather than a routine refresh, because it resets caret and undo state.
- Use retained `SelectState` with typed source and algorithm values, stable control IDs and accessible labels. Apply `SelectEvent::Confirm` through the session setters. Reflect source changes from clipboard load and accepted file input. Render the existing SHA-1 notice.
- Attach Calculate, Cancel and Copy to the session capability checks. Kit progress accepts 0–100. Show bytes and an indeterminate state for an unknown total. Remain capable of draining a retired worker while its utility is closed. Preserve digest visibility and separate feedback after clipboard errors.
- Invoke the built-in asynchronous picker on the accepted Choose File event after checking that the originating HWND is the active owner on that thread. Use single-file selection, distinguish cancellation and error, and reject completion from a retired host or session. Read and copy clipboard content through the existing backend and preserve prior snapshots on failure.
- Preserve live popout and accepted parent-popout precedence, followed by the remembered or default docked mode. Rehosting retains the session and job. Actual close ends the session. Reuse the utility presentation revision and save-worker policy with the canonical `hash-generator` key, preserving unsupported fields and separate docked size/native geometry records. Keep settings side effects bound to an accepted presentation action.

## First hosting checkpoint

Move the preference state and write acknowledgement handling out of `UtilityPresentationManager` while preserving its egui context binding and pending-only repaint. Both selected runtimes use `SettingsSaveWorkerRegistry` and canonical settings. GPUI drains its writer after its event loop and before the data guard releases. A failed save reports once and preserves the accepted in-session choice. Unknown settings payloads remain intact.

Use closed, docked and popout host states with a stable mount generation separate from the reactivation revision. Create the destination before retiring the source. Seed its controls from the retained session and reject callbacks from a retired mount. If Dock returns to a hidden root, request show through the root visibility owner. Persist only the accepted mode change. Preserve saved geometry fields in this first round trip. A later geometry adapter needs Windows outer coordinates and scale because the Kit window bounds describe the client area.

Use the already-borrowed Kit root `Window` for a docked view and the independent window/content handle for a popout. Defer child rehost effects until its current update returns. Replace the mount before closing the old HWND so its close observer cannot end the retained session. Keep accepted rehost work in the G03 restart admission gate until the old HWND retires and the current save revision is acknowledged. Report an unresolved write and retain any unretired HWND in restart preflight.

The first native round trip exercised Dock and Pop Out with the root visible and retained the draft and digest across both mounts. The confirmed GPUI-to-egui restart reopened the saved Popout presentation from the same root. See `G04-C2-INITIAL-HOSTING` and `G04-C2-FOCUS-RESTART`. The G01 global hotkey injection remains unverified. Hidden-root behavior requires a separate successful visibility action or physical input.

## Pending Clipboard checkpoint

Use `ArboardClipboardBackend` for explicit Load and Copy. The read reaches the existing protected reader and its timeout policy. Run the accepted operations outside the UI thread and `HashSession` lock, with one root-owned in-flight operation. Put load and copy outcome application in `HashSession` so egui and GPUI share snapshot, error and digest feedback. Load keeps a prior snapshot on failure and invalidates the earlier result only on a changed source. An empty string is a valid loaded snapshot. GPUI shows the byte count and current feedback.

Fence a late Load by mount, session and source generation. Bind an accepted Copy to the current result request and hold its admission until the OS write and feedback settle. Prevent a conflicting result change, host move or close during that short accepted write and keep Quit/restart blocked with feedback. Include opaque clipboard snapshot presence and revision in restart draft checks without carrying contents in the restart, settings or diagnostics. Keep the adapter's control entities and rendering in a focused Hash view module if the move leaves root mount and effect ownership clear.

Exercise the existing session and clipboard-backend tests, exact UTF-8 snapshot handling, private refusal, stale load and accepted write/restart fencing. Native Load/Copy verification requires a disposable Windows session with an isolated clipboard. A separate application settings directory shares the desktop clipboard.

## Pending geometry checkpoint

Reuse the canonical `Settings.utility_dialog_sizes` component merge and `SettingsSaveWorkerRegistry`. Keep docked logical `size` and popped-out `native` outer placement independent. Extract the existing acknowledgement and accepted-intent ownership from `UtilityDialogSizeManager`. Keep egui viewport and widget discovery in its adapter. Load and place the popout on initial open and rehost through the existing nearest monitor/work-area policy. GPUI's reported bounds and resize operation describe the client area, so sample actual checked outer bounds and monitor DPI for the saved placement.

Share the Windows gesture observer in `utility_window_geometry.rs` after separating egui title-based lookup from direct checked-HWND binding. An ordinary move/resize notification can also come from maximize, restore or a DPI change. `WM_ENTERSIZEMOVE`, `WM_SIZING` and `WM_EXITSIZEMOVE` distinguish the accepted user gesture. Stamp its result with the current mount and session, drain a completed gesture before retiring or replacing its mount, and submit only the affected geometry components. Use a fitted, scrollable docked surface and Kit's resize handle with one durable intent per completed drag. A temporary constraint from root layout must preserve the stored preference.

Target `utility_dialog_sizes_keep_docked_and_native_geometry_independent`, `native_geometry_intent_keeps_unacknowledged_resize_across_later_move` and `utility_restore_uses_current_child_scale_for_cross_monitor_commands` where the extraction touches them. Add a focused case for stale mount and immediate close if the shared observer changes. Windows proof should seed distinct mode sizes, move and resize the popout, reopen and compare its outer rectangle and DPI, then perform a docked resize and read both components in egui. The Windows system menu may provide keyboard Move and Size. Mixed-DPI, docked grip and screenshot comparison need a native environment that can supply the required observations.

## Verification

Use the existing Hash Generator unit tests for exact byte vectors, request lifecycle and errors, and the typed dispatch and file-origin tests for the entry paths. Select the `multi_launcher` library target before the `hash_generator` filter. Compare both actual frontend adapters with synthetic text and a disposable file. Run a scoped native test with isolated clipboard data for snapshot/read/write feedback. Exercise mutation while a job is active, popout with hidden root, cancellation and reopen, and compare docked and narrow popout presentation, focus and accessible control names. Establish result and lifecycle parity before accepting any row.
