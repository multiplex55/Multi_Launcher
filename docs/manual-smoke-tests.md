# Manual smoke tests

This checklist covers behavior that depends on a real Windows desktop session, Win32 window handles, UI Automation, global hooks, or foreground-window focus. Keep these checks manual instead of forcing them into CI.

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
