# Local patch

Based on the exact `accesskit_windows` 0.34.0 crates.io source and VCS
revision `c88605b96d04431f9c3c792464a0f2f253480e94`.

`ClientToScreen` can fail after Windows destroys an HWND while UI Automation
still retains a provider for that window. The upstream helper unwrapped this
failure, which aborted the process from the COM callback. The local change
returns `UIA_E_ELEMENTNOTAVAILABLE` for `ERROR_INVALID_WINDOW_HANDLE` and
propagates geometry errors through the existing UIA result paths.

Remove this patch when the pinned upstream dependency provides the same
destroyed-window behavior. GPUI's current Windows integration keeps this
adapter private, so the application cannot retire retained providers itself.

The empty workspace marker keeps this vendored package independently testable
without adding it to the application's workspace membership.
