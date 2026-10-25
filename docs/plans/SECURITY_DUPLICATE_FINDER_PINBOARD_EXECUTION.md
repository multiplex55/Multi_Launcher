# Security, Duplicate Finder and Pinboard execution

Contract: `CODEX_GOAL_SECURITY_DUPLICATE_FINDER_PINBOARD.md` and its Kickstart.
Only these corrections and their focused evidence are in scope.

## Git baseline

- Clean original/default checkout: `master`, `2275ae1ef85446b49405610d77632ea8e4ac616e` (also cached `origin/master`). Remote tip verification unavailable: GitHub connection blocked.
- Fresh branch: `fix/rustls-duplicate-finder-pinboard`.
- M0-A baseline: `abc8ced6dfd644d6190684482bf3b12abb96ceff`.
- Initial push: attempted once on the new branch; failed to connect to github.com:443. Continue locally.
- Later local Git inspection showed upstream configured and `origin/fix/rustls-duplicate-finder-pinboard` at `afde8db4` with reflog `update by push`, although this chat's push calls returned connection errors. No additional push was performed to resolve this discrepancy; server state is unverified.

## Checkpoints

| Stage | State | Evidence / commit / push |
| --- | --- | --- |
| M0-A | complete | Baseline above; no pre-existing modifications. |
| M1-A | complete | `afde8db4`; rustls 0.23.40 → 0.23.45, required webpki 0.103.13 → 0.103.16 only. Locked reverse graph: reqwest 0.12.28, hyper-rustls 0.27.9, tokio-rustls 0.26.4. Manifest features unchanged. Push: connection failed. Compile deferred to integration. |
| M2-A | partial / verified related fix | Zero-height results viewport corrected and focused geometry/scroll/resize regression passes; reported creep remains unreproduced. Unchanged response-flow gate timeout remains. |
| M3-A | pending | Measure static/live/paused native paths; fix only confirmed bottleneck while retaining exact window ownership and lifetime guards. |
| M4-A | pending | Bounded ~34 ms active capture cadence, deterministic production cadence tests plus distinct generated prepare/display evidence; preserve budgets/coalescing/idle/cancel. |
| M4-B | pending | One existing guarded Glow/native WinRT acceptance pass where supported; record environmental gaps truthfully. |
| M5-A | pending | Changed-file formatting, one first-party compile, focused tests and optional end-only Clippy, independent final diff review, clean committed handoff. |

## Native environment probe

The pre-existing `target/debug/pin_opacity_smoke.exe` (October 22 build) opened
owned Glow windows in session 1, then exited with `GAP: owned generated-region
pixel could not be sampled`. Its source label predates the current generated-live
fixture, so this is only an environment probe, not current-source acceptance or
drag evidence. It left no running `pin_opacity_smoke` process.

## Evidence distinctions

- Generated sessions do not use the native capture throttle: generated throughput proves display preparation/upload only.
- Deterministic FrameCadence tests prove production scheduling policy; actual WinRT FPS requires the owned native source test.
- Native position changes alone do not prove pointer drag latency.
- No full regression suite, release rebuild, unrelated warning cleanup, or new settings.

## Measured Duplicate Finder geometry

Before any production layout change, retained actual window rectangles were
sampled over 64 frames for idle and crowded snapshots, expanded diagnostics and
32 selected roots. At 1180×860 and 1180×1600, frame 0, frame 8 and frame 63 all
remained `[[16,16]-[1056,716]]` (1040×700). At 620×420 they remained
`[[0,0]-[620,420]]`. Height/top/bottom drift was 0.000 points.

The actual application theme was tested, including Comfortable density,
typography scale 1.5 and 1.25 pixels per point. A bottom-right pointer resize to
1040×780 persisted for 48 frames; a second resize to 1040×720 persisted for
another 48 frames. These measurements establish stability for the tested
conditions; they do not establish a fix for the reported symptom. The missing
trigger remains a feature-related reproduction gap.

The first new-test run failed an overly strong assertion that the results pane
must already be visible above the fold in the small enlarged-theme viewport.
Footer containment remained intact. The bounded follow-up proved a real related
defect: at Comfortable/1.5 typography in 620×420 the results inner rect was
`[[11,340.5]-[609,340.5]]` (zero height), and outer scrolling could not reveal it.
The nested results area explicitly sets minimum scrolled height to zero; egui
limits its outer size to exhausted available space, and the zero minimum defeats
its nested-scroll collapse guard. A focused correction will reserve the existing
finite body-derived results budget, leaving the outer body/window bounded.

The serial `dff_dialog_` group also failed the unchanged existing
response-flow test at its 10-second first-read checkpoint timeout, before its
geometry assertions. No production layout/worker changes have been made;
that timeout is recorded as an unresolved existing-test/environment failure,
not silently accepted or hidden.

Correction: `show_results` now uses `.min_scrolled_height(max_height)` alongside
the existing `.max_height(max_height)`, preventing collapse while keeping the
finite body-derived allocation. Focused command:
`cargo nextest run --locked -p multi_launcher --lib -E 'test(dff_dialog_window_geometry_stays_stable_before_and_after_resizing)' -j 1`
passed (1 passed, 6,335 skipped). It proves results can be revealed with bounded
outer scrolling while footer and geometry remain stable. Changed-file rustfmt
and `git diff --check` passed. The earlier group had 2 passed / 2 failed
(new zero-height regression plus unchanged first-read timeout); serial group
rerun stopped after 1 passed / first-read timeout. No more group retries.
