# Multi-goal execution queue

Authority: user attachment `23318cbb-b81f-4b99-b261-eb0b1ea1b68a/pasted-text-1.txt` (master Kickstart), the three Goal contracts here, AGENTS.md and actual configured runners.

Execute G1 → G2 → G3. Each gets one fresh branch and distinct baseline. G2 bases on verified G1 final local SHA; G3 bases on verified G2 final local SHA. Never merge/rebase/cherry-pick/reset or modify master/main. A blocked predecessor is not a valid cumulative handoff.

| Goal | Contract | Branch | Base | Baseline | Current checkpoint | Status | Verification / push |
|---|---|---|---|---|---|---|---|
| G1 | CODEX_GOAL_LAUNCHER_QUERY_FOCUS.md | fix/launcher-query-focus-jump-notes | bfc3ae52a8826fdf223cbc14c8c21dbb580215c2 | 8e3c740671e3e60a3ad3b5bca5913a928cc8d816 | COMPLETE | COMPLETE | Final d7cfd5f1508979aab8085eb93e8c9f60b27aba68 pushed; final slices18/18+16/16; final Clippy PASS; native optional not attempted |
| G2 | CODEX_GOAL_CAPTURE_AND_PRODUCTIVITY_EXPANSION.md | feat/capture-productivity-expansion | d7cfd5f1508979aab8085eb93e8c9f60b27aba68 | 25c47106d642a49a03ec1c6c4e050fc1128f7250 | M3-C pending | IN_PROGRESS (through M3-B verified; native M1 limitation) | M3-B direct7 unique PASS + adjacent4/4, commit pending; M3-A 80036867 pushed,direct8uniquePASS+adjacent3/3; M2 34070ea6 pushed,1/1focusedPASS; M1 native not_run_environment/noinput; one full Nextest near end,Clippy last |
| G3 | CODEX_GOAL_PRODUCTIVITY_TOOLS.md | not created | verified G2 final required | — | M0-A | NOT_STARTED | Selected tests only; Clippy last |

Per-goal details: `launcher_query_focus_execution.md`, then the required capture/productivity ledgers when their branch bootstraps begin. Preserve `table` defaults when G3 adds `csv`; share one engine. Reuse/extend Settings search, Paste source pickers and native pin opacity rather than duplicating owners. Preserve original `diff` grammar with separate `imgdiff`.

Starting checkout `big-start` at `1a19489239cb83c3553512f7f49dccf250ad3f17` was clean. Fetched origin; G1 branches at integration `bfc3ae52a8826fdf223cbc14c8c21dbb580215c2`, which has identical application source and all three contracts. Original branch is one queue-document-only commit ahead. No original work discarded; no integration branch moved.
