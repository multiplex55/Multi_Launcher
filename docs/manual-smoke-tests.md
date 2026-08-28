# Manual smoke tests

This checklist covers behavior that depends on a real Windows desktop session, Win32 window handles, UI Automation, global hooks, or foreground-window focus. Keep these checks manual instead of forcing them into CI.

## Diff acceptance matrix (25 criteria)

The supported and excluded product boundary is recorded in
[`diff-scope.md`](diff-scope.md). Use two temporary roots containing text, binary, Unicode, read-only, missing,
and deliberately large files. Repeat filesystem cases on Windows and Linux.

| # | Scenario | Verify |
|---:|---|---|
| 1 | Run `diff`, `diff <left>`, and `diff <left> <right>`. | Start focus and visible path validation are correct; invalid/mixed paths show recoverable errors. |
| 2 | Press Ctrl+O and choose another pair. | The replacement invalidates the old scan/compare immediately and receives focus. |
| 3 | Compare equal and changed text. | Raw/rules equality, aligned rows, status, and progress are correct. |
| 4 | Use F7 and F8. | Previous/next differences wrap and focus the active pane. |
| 5 | Use both copy-direction actions, then undo. | The selected hunk copies in the requested direction, recomputes, and undo restores it. |
| 6 | Press Ctrl+S and Ctrl+Shift+S. | Active/all dirty sides save; failures remain dirty with a visible error. |
| 7 | Press Alt+Left from a folder child. | The exact parent selection, expansion, filter, sort, and scroll state return. |
| 8 | Use focus-left and focus-right commands. | Keyboard editing moves to the requested pane after every invocation. |
| 9 | Open a changed file from a folder and use Previous/Next Different File. | Navigation skips identical/filter-hidden files without losing the folder parent. |
| 10 | Change status/display filters repeatedly. | Retained scan data is filtered without starting another scan. |
| 11 | Compare roots with repeated directory and filenames. | Expansion, selection, scrolling, and row widgets never share state. |
| 12 | Scroll very large text and folder results. | Only visible rows plus overscan are created; navigation still spans all retained data. |
| 13 | Edit while comparison is running. | An old text result never replaces the new document revisions. |
| 14 | Change include/exclude rules during a scan. | Old batches and content refinements never enter the new root/filter generation. |
| 15 | Change comparison rules. | Text/content caches invalidate and results report the active rules revision. |
| 16 | Resize panes, toggle wrapping, font, or theme. | Wrapped/syntax caches update without changing comparison meaning. |
| 17 | Externally edit a clean file. | The watcher reload is generation checked and comparison refreshes. |
| 18 | Externally edit a dirty file. | A conflict prompt preserves edits; reload/overwrite/cancel each behaves as labelled. |
| 19 | Delete or rename an open file externally. | Missing state is visible, recoverable, and stale reloads are rejected. |
| 20 | Copy one/many folder items in both directions. | Confirmation, containment, per-item cancellation/errors, cache invalidation, and rescan are correct. |
| 21 | Delete to Recycle Bin and permanently. | The chosen mode is explicit; failures do not claim success; Windows Recycle Bin receives recycled files. |
| 22 | Open a normal, large, and extremely large text file. | Status explains the active tier; large reduces expensive decoration; extreme is bounded/read-only and never advertises hex editing. |
| 23 | Compare equal/different binary files. | Byte equality remains available and no text/hex editor is implied. |
| 24 | Close Diff during scan/read/hash/mutation, then reopen a session. | Senders/watchers release promptly, no old event updates the new view, and focus is restored. |
| 25 | Exercise permission errors, locked destinations, invalid regex, malformed UTF-8, UTF-16 save, symlink/reparse escape, and disk-full simulation. | Each failure is scoped and actionable; data is retained, atomic overwrite is used, and unrelated launcher plugins/routing still work. |

## MultiManager Win32 workflow

1. Start Multi Launcher on Windows with at least two normal desktop applications open, such as Notepad and Windows Terminal.
2. Run `mm` and create a temporary workspace.
3. Use **Capture Active Window** and press **Enter** while the target app is foregrounded.
4. Confirm the captured row shows the expected title and executable metadata.
5. Set home and target rectangles, then verify **Send Home** and **Move Target** move the real window.
6. Close one tracked application and leave Multi Launcher running for several minutes. Confirm the GUI remains responsive and the closed application does not automatically reconnect while the launcher continues running.
7. Reopen the closed application and confirm it remains disconnected until a manual reconnect is requested.
8. Run `mm reconnect` or click **Reconnect Windows**. Confirm the UI shows **Reconnecting…**, remains responsive during the operation, and reconnects the reopened application when a matching window exists.
9. Force or observe a failed manual reconnect, then fix the matching condition or reopen the application and confirm a later manual reconnect attempt can succeed.
10. In a workspace with three tracked windows, close one window and confirm **Send Home**, **Move Target**, toggle, and rotate still move the other two tracked windows.
11. Create two candidate windows with the same exact title for one disconnected entry, then run manual reconnect and confirm the result is reported as `Ambiguous`.
12. Create an exact-title candidate whose stable metadata is incompatible with the disconnected entry, then run manual reconnect and confirm the result is reported as `MetadataMismatch`.
13. Start reconnect, recapture the same entry before stale reconnect results can apply, and confirm the recaptured HWND is not overwritten by stale reconnect results.
14. Run `mm recapture all`, use **S** to skip at least one candidate, and use **Escape** to cancel a remaining recapture flow.
15. Use **Save HWND Snapshot** and **Restore HWND Snapshot** after moving/reopening windows to confirm bindings round-trip.
16. Keep `ignore_launcher_window_on_capture` enabled and verify capture does not save the launcher window when the launcher had focus immediately before capture.

## Clipboard Modify layout matrix

Run these checks in a real desktop session because they depend on launcher focus, dialog sizing, clipboard behavior, and native text-control scrolling.

| Scenario | Setup | Verify |
|---|---|---|
| Large Clipboard | Copy text containing at least 1,000 short lines, one extremely long line, Unicode characters, and blank lines. Open Clipboard Modify with `cm`. | Window remains bounded; source field displays ten rows; source scrolls internally; preview displays ten rows; wrapping defaults to enabled; applying a transformation produces complete output, not only the visible preview. |
| Tab Growth | Create enough templates and saved pipelines to exceed window height, then open template and pipeline management. | Every tab scrolls; add/filter controls remain visible; Save remains visible; tab navigation remains fixed. |
| Help | Open `cm help`, clear the Help filter, and scroll the command list. | Complete command list is reachable; window does not grow; filtering still works while scrolled. |
| Narrow Dialog | Open Clipboard Modify and reduce the dialog width until the tab bar no longer fits. | Tab row scrolls horizontally; tab labels do not wrap; no tab becomes unreachable. |
| Session Resize | Resize the dialog, close it, reopen it, then restart the application and open it again. | Size is retained after close/reopen within the same session; after restart, the configured or default startup size is used. |
| Launcher Completions | Type `cm` in the launcher. | Open Clipboard Modify is selected first; navigation commands are listed; canonical transformations are listed; pipelines are listed; templates are listed; undo and aliases are not listed; selecting rows fills the query and keeps focus active; pressing Enter a second time opens or executes the completed command. |

### Clipboard Modify scenario details

#### Large Clipboard

1. Copy a large sample that includes:
   - At least 1,000 short lines.
   - One extremely long line.
   - Unicode characters.
   - Blank lines.
2. Type `cm` and open **Open Clipboard Modify**.
3. Confirm the window remains bounded instead of growing to fit the whole clipboard.
4. Confirm the Source field displays ten rows and scrolls internally.
5. Run a previewable transformation and confirm the Preview field displays ten rows.
6. Confirm wrapping defaults to enabled.
7. Apply the transformation and paste the clipboard into an editor. Confirm the output contains the complete transformed clipboard, not only the visible preview text.

#### Tab Growth

1. Create enough templates and saved pipelines to exceed the dialog height.
2. Open template management and saved-pipeline management.
3. Confirm every tab can scroll through its full content.
4. Confirm add/filter controls remain visible while content scrolls.
5. Confirm Save remains visible.
6. Confirm tab navigation remains fixed while tab content scrolls.

#### Help

1. Open `cm help`.
2. Clear the Help filter.
3. Scroll through the Help list.
4. Confirm the complete command list is reachable.
5. Confirm the window does not grow to fit the list.
6. Confirm filtering still works after scrolling.

#### Narrow Dialog

1. Open Clipboard Modify.
2. Reduce the width until the tab bar no longer fits.
3. Confirm the tab row scrolls horizontally.
4. Confirm tab labels do not wrap.
5. Confirm every tab remains reachable.

#### Session Resize

1. Open Clipboard Modify and resize the dialog.
2. Close the dialog.
3. Reopen Clipboard Modify and confirm the resized dimensions are retained.
4. Restart Multi Launcher.
5. Open Clipboard Modify and confirm the configured or default startup size is used instead of the previous runtime resize.

#### Launcher Completions

1. Type `cm` in the launcher.
2. Confirm **Open Clipboard Modify** is selected first.
3. Confirm navigation commands are listed.
4. Confirm canonical transformations are listed.
5. Confirm saved pipelines are listed.
6. Confirm templates are listed.
7. Confirm `cm undo` and aliases are not listed.
8. Select completion rows and confirm the first **Enter** fills the query while keeping focus active.
9. Press **Enter** again and confirm the completed command opens or executes.

## Browser tabs and UI Automation

1. Open a Chromium-based browser with several tabs.
2. Run `tab cache`.
3. Search with `tab <title fragment>` and select a result.
4. Confirm the browser tab activates, allowing for the documented click-simulation fallback.

## Mouse gestures

1. Enable mouse gestures in `mg settings`.
2. Bind a simple gesture to a harmless command such as opening help.
3. Hold the right mouse button, draw the gesture, and release.
4. Confirm the expected command runs and no unexpected elevated-permission prompt appears.

## `mkmacro` Windows acceptance checklist

These checks are intentionally manual and destructive. They require a real interactive Windows desktop; ordinary automated tests **must use fake input, window, screen, UIA, and launcher backends** and must never opt into `LiveInputOptIn`. Record the Windows version, Multi Launcher commit, monitor layout/DPI, integrity level, and tester/date beside every applicable result.

### Prerequisites and recovery

- [ ] Save work, close sensitive applications, disconnect remote-control software, and use a disposable standard-user Windows account or VM with a restore snapshot.
- [ ] Keep Task Manager available and know how to terminate Multi Launcher without using its hotkeys. Ensure a physical keyboard and mouse remain available.
- [ ] Create a disposable Notepad document, a harmless test window, known exact/tolerant reference images, and pixels of known colors. Use a second monitor with its left/top edge at a negative virtual-desktop coordinate when available.
- [ ] Back up `mkmacros.json` and `mkmacro_assets`; verify the legacy `macros.json` backup separately.
- [ ] After each failure, stop playback/recording, release any apparently held keys/buttons physically, close test applications, restore the files/snapshot, and restart Multi Launcher. Never continue if normal keyboard or mouse input is impaired.

### Keyboard, mouse, and destructive emergency-stop checks

- [ ] **Notepad text/keys:** type Unicode containing an accented character and a non-BMP emoji; run Ctrl+A and Ctrl+C; run individual key-down/key-up rows. **Expected:** exact text and selection/clipboard results, no replacement characters, balanced down/up events, and `Completed` rows.
- [ ] **Stop with modifier held:** hold Ctrl in a macro, stop during a delay, then type normally. **Expected:** Ctrl releases immediately, status is `Stopped`, no later row runs, and subsequent input works normally.
- [ ] **Pointer coverage:** verify move, single/double/right click, X1/X2 buttons where hardware supports them, vertical/horizontal wheel, drag, active-window/variable/image-relative targets, and multi-monitor negative coordinates. **Expected:** the intended target receives each exact operation and every button is released.
- [ ] **DESTRUCTIVE—key Emergency Stop:** run **key down → 10-second delay → Emergency Stop**. **Expected (all required):** immediate key release, `Stopped` status, no following action, and successful subsequent normal keyboard input.
- [ ] **DESTRUCTIVE—mouse Emergency Stop:** run **mouse button down → 10-second delay → Emergency Stop**. **Expected (all required):** immediate button release, `Stopped` status, no following action, and successful subsequent normal mouse input.

### Windows and recording

- [ ] **Notepad window operations:** open a disposable Notepad document and use the Window Picker to capture its process, title, and class. Exercise activate, wait, move-only, resize-only, combined move+resize, minimize, maximize, restore, and finally close. Record whether activation succeeds or Windows foreground-activation policy refuses it. **Expected:** each accepted operation changes only Notepad; a policy refusal is a visible explicit failure (not silent success), but is recorded as an OS-policy outcome rather than an automated-test failure. Close is used only on the disposable document.
- [ ] **File Explorer window operations:** open a disposable File Explorer window and use the Window Picker to capture its process, title, and class. Exercise activate, wait, move-only, resize-only, combined move+resize, minimize, maximize, restore, and finally close. Record whether activation succeeds or Windows foreground-activation policy refuses it. **Expected:** each accepted operation changes only that Explorer window; a policy refusal is a visible explicit failure (not silent success), but is recorded as an OS-policy outcome rather than an automated-test failure. Close is used only on the disposable window.
- [ ] **Missing and ambiguous window safety:** repeat both application checklists with a missing target, then with two windows matching the same picker criteria. **Expected:** diagnostics retain enough process/title/class context to identify the missing or ambiguous matcher, and no candidate or unrelated window is activated, moved, resized, state-changed, or closed.
- [ ] **Recorder:** record typing, clicks, wheel, and drag; use the recorder controls; play the result while recording is armed. **Expected:** physical actions become sensible steps, while injected playback, Emergency Stop, and record/controller interactions are excluded and playback is not re-recorded.
- [ ] **Injected exclusion:** generate tagged playback and unrelated injected input with exclusion enabled. **Expected:** neither becomes a recorded step; physical input still does.

### Control flow, launcher, and hotkeys

- [ ] **Structured plan:** exercise nested If/Else, repeat, while, break, continue, timeout, retry, and continue-on-error. **Expected:** row order and per-row Success/Skipped/Failed states match the branches; bounded loops/timeouts terminate; retry delay remains stoppable.
- [ ] **Launcher coexistence:** run an `mkmacro` launcher command and an existing legacy `macro` invocation. Disable each plugin in turn. **Expected:** prefixes route independently and disabling one never disables or rewrites the other.
- [ ] **Hotkeys:** invoke a per-macro hotkey and configure a collision with Emergency Stop, launcher, or another macro. **Expected:** the unique hotkey runs once; every conflict is presented clearly and is not silently registered.

### UI Automation, vision, and cancellation

- [ ] **UIA patterns:** against disposable controls, test invoke, set/read value, toggle, select, and focus. Request an unsupported pattern with fallback explicitly disabled. **Expected:** supported operations affect the selected element; unsupported/no-fallback fails explicitly without mouse/keyboard synthesis.
- [ ] **Visual operations:** test exact and tolerant image search, pixel search, found-image click, and cancellation during a long visual wait. **Expected:** confidence/tolerance distinguish fixtures, click uses the returned point, cancellation wakes promptly, status becomes `Stopped`, and no later action runs.
- [ ] **Wait safety:** repeat Stop/Emergency Stop during a long delay, paused delay, held key, held mouse button, smooth move/drag, window wait, image/pixel wait, and retry delay. **Expected:** prompt wakeup, final `Stopped` status, no following row, and cleanup releases every input owned by playback.

### Higher-integrity/UIPI behavior

- [ ] From a normal, non-elevated Multi Launcher, target an elevated disposable application (for example, an administrator-launched Notepad) where policy permits. **Expected:** UIPI-blocked activation/input/UIA fails visibly as a rejected operation; it never reports success, every owned key/button is released, later rows do not run under Stop policy, and normal input still works. Do not weaken UAC/UIPI policy merely to make this pass.

### Native visual overlay renderer

This checklist requires a **real, interactive Windows desktop**. The native layered-window renderer cannot be visually validated in headless CI, so these checks must not be automated there. For every item, record pass/fail evidence (screenshots or video where useful) plus:

#### Direct passive-preview harness

Before debugging this rendering path through **Wait for Visual Change**, run the
manual harness from a Windows terminal:

```powershell
cargo run --bin passive_overlay_smoke
```

1. Confirm four bright yellow edges appear at `(100,100)`, outlining a
   `500×300` rectangle.
2. Confirm all four edges remain visible for roughly 2.5 seconds.
3. Confirm every edge disappears afterward, and that the terminal prints the
   launch, active, and cleanup diagnostics.

This harness must pass before debugging the same path through **Wait for Visual
Change**. It is manual-only: ordinary cross-platform unit tests never execute it
or open native windows.

- Windows version and build;
- Multi Launcher commit;
- GPU and graphics configuration;
- each monitor's resolution, scale/DPI, orientation, and virtual-desktop placement;
- whether any monitor has a negative virtual-desktop origin;
- tester name and date.

| Check | Setup | Action | Expected result | Cleanup |
|---|---|---|---|---|
| **Highlight Selected** | Open the monitor editor and select Monitor 0. | Click **Highlight Selected**. | A bright, clearly visible outline and the number `0` appear on the correct physical monitor for approximately 2.5 seconds. The overlay remains click-through and disappears without residue. | Wait for expiry and verify no overlay window or marking remains. |
| **Identify All** | Configure at least three displays, preferably with mixed DPI and one negative-origin display, and compare the editor's ordered descriptors with the physical layout. | Click **Identify All Monitors**. | Every physical monitor receives exactly one visible mkmacro index; indices agree with the editor descriptors, labels remain legible under mixed DPI, and all overlays disappear together. | Wait for expiry and verify every display is clear. |
| **Highlight Window** | Open a disposable File Explorer window and place it on a secondary or negative-origin monitor. | Select the Explorer target and click **Highlight Window**. | The outline matches the entire visible Explorer bounds, including its placement on the non-primary monitor. | Wait for expiry, then close the disposable Explorer window. |
| **Highlight Client Area** | Reopen or retain the same disposable Explorer window in the same position. | Highlight that window's client area. | The outline matches only the client area; the title bar, resize borders, and non-client frame are excluded. | Wait for expiry and verify no residue; close Explorer if still open. |
| **Pick Region** | Arrange windows across two monitors, including a negative-origin monitor when available. | Begin **Pick Region**; drag in multiple directions and across monitor boundaries; also exercise release/Enter confirmation and Escape cancellation. | The live layered-window outline follows the pointer continuously, crosses monitor boundaries correctly, normalizes the final rectangle, confirms on release/Enter as designed, and disappears on Escape. | Press Escape, close the picker, and verify no overlay windows remain on any display. |

**Release-blocking failures:** invisible or black overlays, wrong z-order, DPI-offset bounds, incorrect numbering, captured mouse clicks, stale overlay windows, overlays left after Escape, or a duration materially different from `PASSIVE_OVERLAY_DURATION` block release.

### Completion and release gate

- [ ] Restore/compare `macros.json`, remove disposable `mkmacros.json` entries/assets, close targets, verify no hooks/hotkeys remain registered, restart, and confirm ordinary keyboard/mouse and the legacy macro plugin still work.
- [ ] Attach the completed checklist (including explicit Not Applicable reasons), environment metadata, failures, and recovery performed to the release record.
- [ ] **Release acceptance:** the complete fake-backed `mkmacro` suite must pass, and every applicable Windows smoke-test entry above must have a recorded result, **before enabling `mkmacro` by default**. Any safety, cleanup, false-success, or UIPI result blocks default enablement.
