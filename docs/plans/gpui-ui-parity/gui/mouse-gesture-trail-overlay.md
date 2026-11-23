# Mouse Gesture Trail Overlay

G21 owns `mouse-gesture-trail-overlay`. `mouse_gestures/service.rs` creates the `TrailOverlay` and `HintOverlay` independently. The running gesture service admits the trail through the real gesture and Windows hook paths. `mouse-gesture-settings` controls whether the trail is shown, its start threshold, interval, color and width. The separate hint overlay keeps its own setting and lifecycle.

| Criterion | Source-backed behavior |
| --- | --- |
| GESTURE-TRAIL-01 | An eligible gesture resets the trail at its signed desktop start point. Drawing starts after the configured movement threshold. Keep the first segment anchored to the start, and use successive signed cursor points for later segments. Apply the configured interval, color and width. Clear active drawing and retained points on completion, cancellation or disable. |
| GESTURE-TRAIL-02 | `mouse_gestures/overlay.rs::TrailOverlaySurface` creates the existing Windows `MultiLauncherTrailOverlay` HWND and DIB across the virtual desktop. Preserve its layered, transparent, nonactivating, topmost tool-window behavior, the signed virtual origin and resource release on retirement. Reuse the service and its backend through either frontend. |

Verify threshold, signed coordinates and clearing with a fake `OverlayBackend` and the existing gesture service tests. Before accepting the Windows route, use an isolated receiver and configured test binding to observe the trail, unchanged pointer/focus ownership and teardown. Exercise a signed-origin monitor when available. Source specified. Native and GPUI verification pending.
