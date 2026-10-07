# QR generator execution ledger

Authority: `qr-code-plan.md` and the current QR kickstart request. Source baseline: `9e16cfe5` on the selected branch; working tree initially clean.

| Checkpoint | Scope | State |
| --- | --- | --- |
| M1-A | Local encoding domain, dependency, focused tests | complete |
| M1-B | Shared crisp raster and metadata | complete |
| M2-A | Built-in plugin, exact/payload queries, discovery | complete |
| M2-B | Typed parser/bus/host routing, privacy/history | complete |
| M3-A | Transient dialog and panel lifecycle | complete |
| M3-B | Editor, cached live preview | complete |
| M3-C | Advanced correction and capacity feedback | complete |
| M4-A | Explicit Paste and Copy Text | complete |
| M4-B | Explicit image copy | complete |
| M4-C | Explicit PNG save | complete |
| M5 | Remaining integration coverage, targeted verification, review | complete |

Commit boundaries follow these stages; neighboring small coherent checkpoints may be combined as permitted by the approved plan. Each source checkpoint is inspected and committed before materially different work. Only one implementation writer is active at a time.

Verification: QR unit/state tests, directly affected plugin_commands/plugin_routing, typed commands, cargo check. Prefer scoped Nextest targets. No full-suite campaign. Review locality, privacy, exact payload transport, quiet zone/integer raster, stale-output clearing, explicit side effects, panel/Escape lifecycle, and scope.

Native phone scan remains a human smoke check. Native GUI checks depend on available computer-control capabilities.

M1-A: scoped Nextest passed 7/7. Independent review identified automatic Kanji-mode reinterpretation of UTF-8; corrected before checkpoint using ECI 26 + Byte mode for non-ASCII, with ascending version selection. ASCII retains encoder optimization. No source/payload storage or side effects.

M1-B: scoped Nextest passed 13/13; independent raster review had no substantive findings. Shared raster uses four quiet modules, eight pixels per module, opaque black/white, and independent source counts.

M2-A: 24/24 selected Nextest tests passed across lib/plugin_commands/plugin_routing. Added literal provider query policy shared by synchronous/background routing, exact payload args, built-in registration/discovery and QR exact-search bypass. Review found unsupported filter syntax in new tests; corrected to real kind:/id: and negative tokens before checkpoint. Formatting/diff checks passed.

M2-B/M3-A combined to keep real host/dialog integration coherent: exact plain Action.args through typed DialogCommand; reserved QR namespace, headless rejection, no query override or execution history. All normal panel mappings and shared close cleanup added. Independent review had no substantive findings; cargo check passed. Focused parser/bus/headless/state/panel tests added, execution pending M5 by verification budget.

M3-B: editable exact multiline source, independent counts, cached generation/raster/nearest texture, immediate prefilled preview, non-error empty state, capacity clearing and recovery. Native utility viewport follows existing Regex pattern (620x700) to fit editor and scan-sized preview without changing launcher geometry; bounded embedded fallback. Both cache/state and viewport reviews passed; cargo check passed after refinement. State/viewport tests added; execution pending M5.

M3-C: Advanced L/M/Q/H transient controls regenerate before preview; source/selection retained on capacity failure, fresh Medium/Advanced closed. Correction/capacity/cache tests added pending M5. Independent review, cargo check, formatting/diff checks passed.

M4-A: explicit text clipboard actions through existing backend; exact text, failure preservation, payload-free typed inline status, empty Copy disabled/oversized text copyable. Mock call-count/error/empty/capacity tests added pending M5. Independent review, cargo check and format/diff checks passed.

M4-B/M4-C combined coherent output checkpoint: same cached raster to explicit guarded image clipboard/PNG actions. Native chooser PNG default/neutral filename, confirmed destination unchanged with PNG suffix validation, explicit PNG encoding, silent cancel, state retained on error. Image byte/PNG roundtrip/guard/path/cancel/failure tests added pending M5. Review, cargo check, formatting/diff checks passed.
M5-A: added one full searched-action activation test, preserving literal payload/query and proving no history/usage recording plus fresh bare invocation reset. All tests previously recorded as pending M5 have now passed.

## Final verification

- `cargo nextest run --lib --test plugin_commands --test plugin_routing --test domain -E 'test(qr) | test(commands::) | binary(plugin_commands) | binary(plugin_routing) | test(command_bus_architecture)'`: **202 passed, 4,709 skipped**, across four selected binaries. No full repository run.
- `cargo check`: **passed** on final source (41.40s).
- `rustfmt --edition 2024 --check --config skip_children=true` on all changed Rust files: **passed**.
- `git diff --check`: **passed**. Cumulative diff inspected; only QR and directly needed integration/tests/ledger files changed.
- Independent milestone and cumulative reviews: no remaining substantive findings. Initial Unicode/Kanji optimizer defect and unsupported filter fixtures corrected before their commits; no separate remediation commit was needed.
- Test-generated root `clipboard_modifiers.json` removed (absent in initial clean tree). No test/GUI process left running.

## Native smoke limitations

The newly compiled executable was launched in isolated `target/qr-native-smoke`, without touching the user's normal data. The Windows helper returned no targetable launcher window, including after visible relaunch; approval for a temporary blank native test surface timed out. Both agent-owned launcher processes were terminated and no UI smoke result is claimed. Native invocation/editing, clipboard image paste, save-dialog default extension/cancel, and phone-camera scanning remain human smoke checks. The isolated directory contains only normal settings/modifier catalog, no QR export or payload history.

## Architecture and dependency

`qrcode 0.14.1`, default features disabled, adds local encoding without optional image/SVG renderers or additional dependency packages. ASCII retains smallest optimized version selection; non-ASCII uses UTF-8 ECI 26 and Byte mode to avoid Shift-JIS Kanji reinterpretation. Application-owned correction/matrix/error types isolate the encoder. A single opaque raster with four quiet modules and eight pixels per module serves preview, image clipboard and PNG. Dialog state, source, correction and feedback stay transient; generation/texture are cached until edits. Plugin discovery carries exact text in structured action arguments through the typed interactive command bus with history skipped.

No new adjacent future enhancement was discovered. Scanning/history/structured forms/SVG/styling/cloud generation/Universal Actions remain deferred as specified by the approved plan.

## Checkpoint history

| Stage | Commit |
| --- | --- |
| Execution ledger | `50a13b8f` |
| M1-A local encoding | `41477b8d` |
| M1-B shared raster/metadata | `62fd68bf` |
| M2-A plugin/query routing | `9f68b762` |
| M2-B / M3-A typed invocation/lifecycle | `5cf43d79` |
| M3-B live native preview | `bbd5f72b` |
| M3-C correction/capacity controls | `d8fa8213` |
| M4-A text clipboard | `c7cb3893` |
| M4-B / M4-C image clipboard/PNG | `bb257e1e` |
| M5-A integration coverage/final report | This commit |
