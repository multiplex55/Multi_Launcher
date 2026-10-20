# Jump Mode fixture screenshots

These PNGs contain isolated generated fixture data only. Raw runs remain local
under `dev/jump_mode/evidence/`; this curated set has an 8 MiB total budget.

| Screenshot | Acceptance | Source run |
|---|---|---|
| [Normal dashboard](00_baseline_dashboard.png) | M01 populated baseline accepted by parent visual review; production fixture checks and owned-process cleanup passed | `jump_m01_20261019T232227Z_fc7bc04b` |
| [Query hint](00_query_hints.png) | M05 native leader, map-derived focus handoff, normal typing and zero action activations PASS; parent viewed identical SHA-256 pixels | `jump_m05_query_probe_20261020T000051Z_b5d58182` |

The baseline shows populated note list/search cards and four underscore tag
buckets, with no warning overlay. It is a 900×560 ROOT framebuffer capture of
the actual application. Its provenance is CPU UI completion before GPU
presentation; it is not evidence of a completed GPU presentation.

The source run retains `baseline_visual_review.json`, capture receipt,
fixture semantic logs, and `process_lifecycle.json` (graceful exit 0, owned
tree gone). No Jump keyboard scenario is claimed by this baseline image.

This baseline predates the instruction to disable Paste Sequence Next/Cancel
shortcuts in test profiles. All subsequent generated profiles enforce that
instruction; existing evidence is preserved unchanged.

The M05 query screenshot is 900×560, 178133 bytes, SHA-256
`50b14f02a5d4165af7f5fc110dcde35d65a976ad0408eb2dec31bb6579c9c816`.
Its receipt binds the active map to retained ROOT pass 9. The trace independently
proves hint A completed to actual query focus, unchanged empty query/count 0,
then native X produced query x/count 0. PID 40556 exited 0 gracefully; owned tree gone.
Both Paste Sequence shortcuts are disabled in this profile. Query-only scope:
dashboard/grid controls are not yet registered in this screenshot.
M08 Dashboard filter `jump_m08_phase1_enabled_20261020T014934Z_c5af8d06_dashboard`
passed catalog 01–05 with retained ROOT/F11 receipts and native observed YA
selection of Jump QA Note 006, unchanged query/count 0 before fresh Enter, then
the exact Dashboard FillQuery action/query override and count 1. PID 28440 exited 0
gracefully, with its owned tree gone. Parent viewed active hints and selected-state PNGs;
selected outline is clear. [Dashboard hints](02_dashboard_hints_active.png)
is a fixture-only 1800x1000 image, 567548 bytes. Small Refresh header badges
obscure the final label letters; this concrete placement issue is reserved
for bounded M09 repair. No theme redesign is planned.
[Grid hints](07_note_grid_hints_active.png) comes from the passing nine-capture
Grid smoke `jump_m08_phase1_enabled_20261020T020713Z_48084286_grid`.
It is 1800x1000, 401213 bytes, SHA-256
`84595d5fe6a7c72ffc9af2dc012d01a0a57a33a4e968534c88a1c5d4601405f2`.
Parent viewed this image and the note-panel ownership capture: all 32 result
cards are visible, the 33 hints (including query) are unique and prefix-free,
and the open fixture note suppresses background hints. The normal note-open
information toast remains visible; error notifications are enabled.
The trace proves exact duplicate-title slug 001 selection before activation,
stale-generation rejection after a safe zero-result query edit, a processed
Enter with no activation, and one exact note open after restoring/reselecting.
PID 20588 exited 0 gracefully; its owned tree was verified gone. Curated PNGs
currently total 1318462 bytes, below the 8 MiB budget.
