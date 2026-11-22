# Pin Capture Feedback

G22 owns `pin-capture-feedback`, the independently presented deferred native failure window. The pin presenter receives a current capture failure after the staged root and pin restoration.

| Criterion | Source-backed behavior |
| --- | --- |
| PIN-FEEDBACK-01 | Report the applicable failure in the current topmost nonactivating feedback window, including with the launcher hidden. Dismiss and native close retire the exact owner. Probe with a synthetic failure and presenter fixture. |

Select `pinboard_presenter_capture_feedback_re_registers_until_dismissed`. Native and GPUI remain pending.
