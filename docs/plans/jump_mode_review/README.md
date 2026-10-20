# Jump Mode fixture screenshots

These actual ROOT framebuffer PNGs contain isolated generated fixture data only.
Raw runs and their receipts, events, manifests and cleanup records remain local
under `dev/jump_mode/evidence/`. The curated set totals 1319518 bytes, below its
8 MiB budget. Capture provenance is CPU UI completion before GPU presentation;
these images do not prove a completed GPU presentation.

| Screenshot | Accepted coverage | Source run |
|---|---|---|
| [Baseline dashboard](00_baseline_dashboard.png) | M01 populated fixture dashboard; parent visual review | `jump_m01_20261019T232227Z_fc7bc04b` |
| [Query hint](00_query_hints.png) | M05 native hint focus, normal typing, no action | `jump_m05_query_probe_20261020T000051Z_b5d58182` |
| [Dashboard hints](02_dashboard_hints_active.png) | M09 Refresh labels readable; five-capture native focus/Enter smoke | `jump_m08_phase1_enabled_20261020T021959Z_abcd5152_dashboard` |
| [Grid hints](07_note_grid_hints_active.png) | M08 32 visible results/33 prefix-free hints; exact duplicate-note selection/open, stale generation and ownership | `jump_m08_phase1_enabled_20261020T020713Z_48084286_grid` |

All source runs have graceful exit 0 and verified owned-tree cleanup. The M01
baseline predates the instruction to disable Paste Sequence Next/Cancel; its
original evidence remains unchanged. Every later profile disables both.
Current fixtures keep normal/error notifications enabled. The note ownership
capture includes the normal note-open information toast.

The M05 query image is 900x560, 178133 bytes, SHA256
`50b14f02a5d4165af7f5fc110dcde35d65a976ad0408eb2dec31bb6579c9c816`.
Its trace proves observed A grants actual query focus with empty query/count 0,
then X edits the query/count 0. Its old feature scope does not include dashboard
or grid target registration.

The dashboard image is 1800x1000, 568604 bytes, SHA256
`f6f455043e592a57de8c24626ba363d0286d559f51e1a79caed5fb6e4b6504af`.
Parent viewed the actual post-fix pixels: Refresh labels are intact and badges
sit beside the buttons. Hint YA selected Note 006/count 0; a distinct Enter
invoked the exact Dashboard FillQuery action once. PID 23372 exited 0 gracefully.
The earlier M08 dashboard image remains in its raw run and checkpoint history.

The grid image is 1800x1000, 401213 bytes, SHA256
`84595d5fe6a7c72ffc9af2dc012d01a0a57a33a4e968534c88a1c5d4601405f2`.
Parent viewed active/selected/ownership states. The trace proves duplicate-title
slug 001 selection before action, safe query-generation invalidation and a
processed Enter with no action, then exact 001 open once after restoration and
reselection. PID 20588 exited 0 gracefully. Background hints are suppressed while
the fixture note panel is open. See JUMP_MODE_RESULTS.md for all 15 accepted
Phase 1 capture mappings, including the separate disabled-gate run.
