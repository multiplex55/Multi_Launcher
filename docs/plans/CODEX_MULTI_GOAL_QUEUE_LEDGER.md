# Multi-goal execution queue

Authority: user attachment `23318cbb-b81f-4b99-b261-eb0b1ea1b68a/pasted-text-1.txt` (master Kickstart), the three Goal contracts here, AGENTS.md and actual configured runners.

Execute G1 → G2 → G3. Each gets one fresh branch and distinct baseline. G2 bases on verified G1 final local SHA; G3 bases on verified G2 final local SHA. Never merge/rebase/cherry-pick/reset or modify master/main. A blocked predecessor is not a valid cumulative handoff.

| Goal | Contract | Branch | Base | Baseline | Current checkpoint | Status | Verification / push |
|---|---|---|---|---|---|---|---|
| G1 | CODEX_GOAL_LAUNCHER_QUERY_FOCUS.md | fix/launcher-query-focus-jump-notes | bfc3ae52a8826fdf223cbc14c8c21dbb580215c2 | pending B0 commit | B0 | in_progress | No tests/build yet; push follows baseline |
| G2 | CODEX_GOAL_CAPTURE_AND_PRODUCTIVITY_EXPANSION.md | not created | verified G1 final required | — | B0 | NOT_STARTED | One full Nextest required near end; Clippy last |
| G3 | CODEX_GOAL_PRODUCTIVITY_TOOLS.md | not created | verified G2 final required | — | M0-A | NOT_STARTED | Selected tests only; Clippy last |

Per-goal details: `launcher_query_focus_execution.md`, then the required capture/productivity ledgers when their branch bootstraps begin. Preserve `table` defaults when G3 adds `csv`; share one engine. Reuse/extend Settings search, Paste source pickers and native pin opacity rather than duplicating owners. Preserve original `diff` grammar with separate `imgdiff`.

Starting checkout `big-start` at `1a19489239cb83c3553512f7f49dccf250ad3f17` was clean. Fetched origin; G1 branches at integration `bfc3ae52a8826fdf223cbc14c8c21dbb580215c2`, which has identical application source and all three contracts. Original branch is one queue-document-only commit ahead. No original work discarded; no integration branch moved.
