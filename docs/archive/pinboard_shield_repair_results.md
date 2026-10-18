# Screenshot Pin shield repair results

Branch: `pinboard-repair`. Approved plan: `multi_launcher_pinboard_shield_repair_plan.md`.

## Root-cause evidence and implemented boundary

The reported initial acknowledgement failure was **not reproduced**. The owned production startup probe passed on the available monitor: requested/observed `(0, 0, 2560, 1440)`, visible=true, extended styles `0x08080088`, window DPI=96, matching thread/window awareness contexts (24592), no API failure. This does not establish mixed-DPI hardware behavior or the original failure's cause. No DPI policy or coordinate tolerance was changed.

Source-proven coverage defects corrected: monitor-info failure could silently omit a monitor, and topology repair skipped visible HWNDs whose actual geometry/styles were invalid. Monitor enumeration now fails closed; live typed probes decide replacements and verify both startup and display-change coverage. Verification accepts a valid replacement even when an older same-bounds shield is invalid. Detailed failures preserve immediate API/last-error context, requested/actual bounds, visibility, styles, operation/monitor identity and DPI metadata. Monitor API last-error values are labelled context, following the documented boolean-failure contracts for [GetMonitorInfoW](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getmonitorinfow) and [EnumDisplayMonitors](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enumdisplaymonitors).

All shields must pass before startup input is acknowledged. Alpha 1, hit-testability, nonactivation, complete physical bounds and the existing Down/Move/Up/release/cancel state machine remain intact. No hooks, sleeps, relaxed coverage or pin-only bypass were introduced.

## Verification

| Check | Actual result |
| --- | --- |
| M1-A: `cargo nextest run --lib -E 'test(rectangle_shield_acknowledgement)'` | 1 passed, 5764 skipped |
| M1-B: `cargo run --features dev-smoke-binaries --bin passive_overlay_smoke -- --rectangle-shield-probe` | Startup/cleanup passed; original failure unobserved |
| M2-A/B: `cargo nextest run --lib -E 'test(rectangle_shield_) or test(rectangle_guard_) or test(rectangle_startup_baseline) or test(shield_plan_preserves)'` | 17 passed, 5749 skipped |
| Single corrected no-input probe, same probe command | Deliberate 2559/2560 width mismatch rejected as `RectMismatch`; topology repair verified a replacement, retained 2 shields until cleanup, and destroyed every owned shield/visual/tooltip HWND |
| M3-A: `cargo run --features dev-smoke-binaries --bin passive_overlay_smoke -- --rectangle-input-isolation` | **not_run_environment** before injection: initial `GetCursorPos` denied (`0x80070005`), also denied in the existing fixture's built-in fresh-thread attempt. No more native attempts |
| M3-B: `cargo nextest run --lib -E 'test(pinboard) or test(crop_screenshot_uses_shared) or test(general_ocr_rectangle_isolated) or test(screen_draw_rectangle_events) or test(commands::handlers::screenshot::tests::)'` | 24 passed, 5742 skipped |
| M4-A: `cargo nextest run --workspace`, source checkpoint `70e574e7`, build jobs=2 | **Failed**: 3021 passed, 1 failed, 45 skipped; 3367 tests not run after fail-fast. See unrelated fixture issue below. No full rerun |
| M4-B: `cargo nextest run --lib -E 'test(rectangle_shield_acknowledgement) or test(rectangle_shield_monitor_query_failure)'` | **2 passed**, 5764 skipped |

M2 coverage includes held/rapid input, capture loss, Escape, replacement, shutdown, failed-start cleanup/retry and signed bounds. M3-B covers actual query/parser/typed GUI-host routes, successful image-pin commit, rejection without capture, pin management/root visibility and shared crop/OCR/Screen Draw selectors. These are deterministic state/GUI tests, **not** a physical mouse-isolation pass. M4-B extends the existing acknowledgement test for exact signed geometry at 96/120/144 DPI metadata, one-column gaps and incorrectly logical-sized bounds.

Full-suite issue: `hotfolder::capability::tests::capability_marks_filesystem_risk_and_macro_staleness_without_dispatch` fails at `src/hotfolder/capability.rs:748`. Its fixture registers only the watched root in `root_availability`, then expects an unregistered copy destination to be supported; production correctly returns `Unavailable { reason: "action destination could not be inspected" }`. The owner is unchanged from the starting checkout, with no dependency on overlay coverage. Left outside this focused repair. Full output: ignored `target/pinboard-shield-nextest.log`.

The first M1 filtered build omitted `--lib`; its owned process tree was canceled before tests ran to avoid unrelated linking. No full suite was executed during M0-M3. An untracked `clipboard_modifiers.json` appeared during the full suite; it was preserved as ignored `target/pinboard-full-test-generated-clipboard_modifiers.json` rather than included in commits.

## Checkpoints and final gate

- `5161d5ce`: M1-A diagnostics, rejection/retry test and opt-in no-input probe; pushed normally.
- `70e574e7`: M2-A strict topology repair, terminal monitor enumeration and lifecycle/probe evidence; pushed normally.
- M4-B final diagnostic cleanup, DPI cases and evidence checkpoint: focused tests, formatting and diff checks passed; final commit hash and push status are reported in chat.
- M4-C runs `cargo clippy --workspace --all-targets -- -D warnings` **last**, after ordinary edits/commits. Its final result and final HEAD are reported in chat; tracked files must not be updated afterward. Vendored lints are outside scope.

## Limitations

The original startup failure remains unobserved. Native Screenshot Pin capture, physical Down/Move/Up isolation, next ordinary click, and mixed-DPI hardware behavior remain unverified because cursor access is denied. No fixture screenshots are available: initialization stopped before the owned EDIT/BUTTON controls were created. No user testing or approval was requested.
