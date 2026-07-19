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
