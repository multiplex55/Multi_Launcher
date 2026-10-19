# Jump Mode fixture screenshots

These PNGs contain isolated generated fixture data only. Raw runs remain local
under `dev/jump_mode/evidence/`; this curated set has an 8 MiB total budget.

| Screenshot | Acceptance | Source run |
|---|---|---|
| [Normal dashboard](00_baseline_dashboard.png) | M01 populated baseline accepted by parent visual review; production fixture checks and owned-process cleanup passed | `jump_m01_20261019T232227Z_fc7bc04b` |

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
