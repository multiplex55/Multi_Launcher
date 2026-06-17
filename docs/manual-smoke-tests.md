# Manual smoke tests

This checklist covers behavior that depends on a real Windows desktop session, Win32 window handles, UI Automation, global hooks, or foreground-window focus. Keep these checks manual instead of forcing them into CI.

## MultiManager Win32 workflow

1. Start Multi Launcher on Windows with at least two normal desktop applications open, such as Notepad and Windows Terminal.
2. Run `mm` and create a temporary workspace.
3. Use **Capture Active Window** and press **Enter** while the target app is foregrounded.
4. Confirm the captured row shows the expected title and executable metadata.
5. Set home and target rectangles, then verify **Send Home** and **Move Target** move the real window.
6. Close and reopen one captured app, then run `mm reconnect` and confirm stale handles are refreshed when a matching window exists.
7. Run `mm recapture all`, use **S** to skip at least one candidate, and use **Escape** to cancel a remaining recapture flow.
8. Use **Save HWND Snapshot** and **Restore HWND Snapshot** after moving/reopening windows to confirm bindings round-trip.
9. Keep `ignore_launcher_window_on_capture` enabled and verify capture does not save the launcher window when the launcher had focus immediately before capture.

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
