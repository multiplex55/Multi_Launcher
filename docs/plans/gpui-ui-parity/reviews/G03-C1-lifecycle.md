# G03-C1 controlled restart review

The independent review covered consent, in-flight work, service and window retirement, and the canonical guard handoff. The production fixes preserve the existing private helper and no-selector behavior.

| Area | Disposition |
| --- | --- |
| Consent and drafts | The first draft left editors writable during asynchronous teardown. The corrected path reads all three actual retained inputs, checks them again on confirmation, and immediately makes them read-only. Escape returns to the same open windows with text preserved. UI Automation exposes the discard explanation. |
| Admission | The one-shot gate rejects active or retiring Hash work, pending native operations, and stale or duplicate confirmation. A failed service precondition leaves the current windows and drafts available. |
| Services and native windows | Checked hotkey and Coordinate HUD shutdown precede utility retirement. GPUI keeps its event loop alive through native HWND retirement. Ordinary root quit remains available when teardown fails before the root close. The checked teardown and native retirement results gate the restart disposition. |
| Accessibility | The first full native run exposed a late COM geometry callback after an HWND retired. The exact-version local AccessKit fix propagates the stale element result while preserving valid coordinates and UI Automation support. The patched artifact completed the same scenario without a panic. |
| Relaunch | `main` drops logging and the canonical data guard before the one-shot same-executable spawn. The supplied original working directory and sole `--ui=egui` selector preserve identity and avoid replay. The new egui window held the original root in the Windows test. |

See `G03-C1-UIA-RETIRED-HWND` and `G03-C1-MULTIWINDOW-RESTART` in the [validation log](../VALIDATION_LOG.md). G03 still needs a reverse switch through egui's authoritative editors and jobs, and cross-frontend persistence evidence.
