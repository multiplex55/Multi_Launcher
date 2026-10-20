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
| [Query hint](00_query_hints.png) | M05 native leader, map-derived focus handoff, normal typing and zero action activations PASS; parent viewed identical SHA-256 pixels | `jump_m05_query_probe_20261020T000051Z_b5d58182` |

The M05 query screenshot is 900×560, 178133 bytes, SHA-256
`50b14f02a5d4165af7f5fc110dcde35d65a976ad0408eb2dec31bb6579c9c816`.
Its receipt binds the active map to retained ROOT pass 9. The trace independently
proves hint A completed to actual query focus, unchanged empty query/count 0,
then native X produced query x/count 0. PID 40556 exited 0 gracefully; owned tree gone.
Both Paste Sequence shortcuts are disabled in this profile. Query-only scope:
dashboard/grid controls are not yet registered in this screenshot.
