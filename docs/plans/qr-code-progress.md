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
| M4-A | Explicit Paste and Copy Text | pending |
| M4-B | Explicit image copy | pending |
| M4-C | Explicit PNG save | pending |
| M5 | Remaining integration coverage, targeted verification, review | pending |

Commit boundaries follow these stages; neighboring small coherent checkpoints may be combined as permitted by the approved plan. Each source checkpoint is inspected and committed before materially different work. Only one implementation writer is active at a time.

Verification: QR unit/state tests, directly affected plugin_commands/plugin_routing, typed commands, cargo check. Prefer scoped Nextest targets. No full-suite campaign. Review locality, privacy, exact payload transport, quiet zone/integer raster, stale-output clearing, explicit side effects, panel/Escape lifecycle, and scope.

Native phone scan remains a human smoke check. Native GUI checks depend on available computer-control capabilities.

M1-A: scoped Nextest passed 7/7. Independent review identified automatic Kanji-mode reinterpretation of UTF-8; corrected before checkpoint using ECI 26 + Byte mode for non-ASCII, with ascending version selection. ASCII retains encoder optimization. No source/payload storage or side effects.

M1-B: scoped Nextest passed 13/13; independent raster review had no substantive findings. Shared raster uses four quiet modules, eight pixels per module, opaque black/white, and independent source counts.

M2-A: 24/24 selected Nextest tests passed across lib/plugin_commands/plugin_routing. Added literal provider query policy shared by synchronous/background routing, exact payload args, built-in registration/discovery and QR exact-search bypass. Review found unsupported filter syntax in new tests; corrected to real kind:/id: and negative tokens before checkpoint. Formatting/diff checks passed.

M2-B/M3-A combined to keep real host/dialog integration coherent: exact plain Action.args through typed DialogCommand; reserved QR namespace, headless rejection, no query override or execution history. All normal panel mappings and shared close cleanup added. Independent review had no substantive findings; cargo check passed. Focused parser/bus/headless/state/panel tests added, execution pending M5 by verification budget.

M3-B: editable exact multiline source, independent counts, cached generation/raster/nearest texture, immediate prefilled preview, non-error empty state, capacity clearing and recovery. Native utility viewport follows existing Regex pattern (620x700) to fit editor and scan-sized preview without changing launcher geometry; bounded embedded fallback. Both cache/state and viewport reviews passed; cargo check passed after refinement. State/viewport tests added; execution pending M5.

M3-C: Advanced L/M/Q/H transient controls regenerate before preview; source/selection retained on capacity failure, fresh Medium/Advanced closed. Correction/capacity/cache tests added pending M5. Independent review, cargo check, formatting/diff checks passed.
