# Visual Productivity Studios acceptance index

Canonical execution ledger: [VISUAL_PRODUCTIVITY_STUDIOS.md](../../VISUAL_PRODUCTIVITY_STUDIOS.md).

| Category | Branch | Source checkpoint | Acceptance |
| --- | --- | --- | --- |
| C01 | feat/visual-studios-01-quick-tools | 00395c036183724393b168ab53f9e4e9beb44fef (pushed) | COMPLETE; later native evidence accepted on C02 without changing C01 source or ancestry |
| C02 | feat/visual-studios-02-calculation-pad | M2-A, M2-B and M2-C; exact SHAs in canonical ledger | COMPLETE at the M2-C checkpoint; focused tests and native checks accepted |
| C03 | feat/visual-studios-03-date-time-studio | M3-A and M3-B; exact SHAs in canonical ledger | COMPLETE; original acceptance preserved; cumulative root-effect/focus corrections pass targeted checks and native Prepare visibility at900x651 (ROOT capture013) |
| C04 | feat/visual-studios-04-diagram-mermaid | M4-A, M4-B and M4-C; exact SHAs and final handoff in canonical ledger | COMPLETE; independent native editing, persistence/restart, lossless unsupported import, templates/layout/undo, SVG/PNG/Mermaid exports and minimum-width controls accepted |
| C05 | feat/visual-studios-05-regex-visualization | M5-A and M5-B; exact SHAs and native records in canonical ledger | COMPLETE; existing tester syntax visualization, exact source correspondence, constructors/undo, constrained layout and corrected native glyph/system-menu checks accepted |
| C06 | feat/visual-studios-06-markdown-studio | M6-A ffb2ea86; M6-B 442d0617/9a255e1a; M6-C 1114600a/42b7c0b7; qualification c68bf3d8; feedback f2e6859d; mode-input fix4ddf03f2 (pushed) | COMPLETE; implementation, scoped checks and native Notes accepted. User confirmed all five exported-HTML browser checks pass on November 3, 2026. [Final handoff](FINAL_HANDOFF.md); previous [policy stop](stalls/20261103T204514Z.md) is resolved |

[Historical C01 native stop report](stalls/20261102T000436Z.md) remains truthful for its preparation time. Later source-bound native evidence resolved those acceptance gaps; see the canonical ledger. C01's historical final handoff remains `54160cf65f137943240d03e43b688fdb74d1ed00` and is C02's exact parent. This index supplements the canonical ledger, rather than serving as a second operational ledger. C03's baseline directly descends from C02's final committed tip; exact ancestry is in the canonical ledger.

All six categories are accepted. The HTML visual gate was closed by explicit user browser inspection of the unchanged2108243-byte export, SHA256 `4CFE12F47604D894ABDA69BC9B3A8944E942C7F4099A7140752F572A5B72B317`; receipt `target/visual-studios/c06-html-user-visual-acceptance.json`. Historical stop reports remain unchanged. No PR or merge was created; the final reporting tip is recorded in `target/visual-studios/program-final-handoff.json` and the completion chat response.
