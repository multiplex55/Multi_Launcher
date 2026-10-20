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

M13 selected-state additions (actual ROOT/F11 pixels, parent viewed; each receipt PID/fixture/frame/pass/PNG hash checked):

| Image | State | Source run |
|---|---|---|
| [Settings numeric focus](17_settings_numeric_selected.png) | Inactive overlay; actual Fuzzy weight numeric focus before fresh native Up; later Cancel preserves runtime | jump_m13_phase2_settings_20261020T093421Z_20ea511d |
| [Notes content focus](17_notes_content_selected.png) | Inactive overlay; actual empty multiline editor focus before ordinary typing/newline; later Cancel preserves32 notes | jump_m13_phase2_notes_20261020T093507Z_665061ad |

Both are1800x1000 at pixels-per-point1.0, graceful exit0/ownedtreegone. Settings328310bytes, SHA2564af2cb39cfb3aaf029f76a1c15113ffa4120a8d314e81752230fe46685071196; Notes381682bytes, SHA2561cb4b880073f2e3d3349fbd2b4dbeaee82d3ec1eaf1eb8ecd74440ccd7725be5. All8curatedPNGs total2744955bytes. Settings active-header placement is corrected and accepted in the final M15 screenshots below. No pre-repair active Settings PNG is curated. All six Phase2 workflows and final representative smoke are accepted.
| Image | State | Source run |
|---|---|---|
| [Paste Sequence loaded](17_paste_sequence_loaded.png) | Manual fixture row loaded after hint focus + fresh Enter; context present/clipboard-text absent; no playback | jump_m13_phase2_paste_20261020T102403Z_e086058e |
| [MkMacro step selection](17_mkmacro_step_selected.png) | Disabled macro1/Delay step1 selected with actual native table focus, unchanged revision and idle runtime/recorder before later safe editor Enter | jump_m13_phase2_mkmacro_20261020T102454Z_5f0e4761 |

Parent viewed both actual selected images and checked receipt PID/fixture/frame/pass/PNG hashes; graceful exit0/ownedtreegone. Paste385024bytes/SHA256cf659e49e2cf4486fd9892e83f049a6449f37c5aa196a6a9be137c18e40fdcf0; MkMacro330421bytes/SHA2569cf81dca75a9da309f1830cb6db4641f11508e982f2712f2782c31c3c631ed13. These are accepted fixture-only captures, not the earlier failed Paste context run. All eight PNGs total2744955bytes (<8MiB).
Final M15 additions, accepted after the compact-header repair:

| Image | Actual retained state | Source run |
|---|---|---|
| [Settings active hints](16_settings_hints_final.png) | Visible section badges after the actual readable heading; active Settings owner; checkbox draft false/runtime true and fuzzy draft1.1 | jump_m13_phase2_settings_20261020T114315Z_12e41b61,capture16 |
| [Resized exact note opened](09_resized_note_opened_final.png) | Native hint selection then distinct Enter opens only jump-qa-note-001; action count2 after earlier dashboard FillQuery; smaller owned client1527×844 | jump_m15_final_smoke_20261020T113712Z_4c58c867,capture09 |

Parent checked each receipt PID/fixture/frame/pass/PNG hash and actual pixels. Settings340691bytes/SHA25681ce7295d280c920f5757ede56a4496cc7a8ab0ae85f74d004e84da924416308; resized note340935bytes/SHA2567e4acff17f20ad3887302a7c13069eb441cd21cea84ff58aea85fdd725dfdb0b. Source copies are byte-identical. Both owned processes exited gracefully0 with verified owned-tree cleanup. Ten curated PNGs now total3426581bytes, below8MiB. Earlier subtotal paragraphs describe their historical checkpoint. Captures are ROOT CPU UI completion before GPU presentation; native96DPI/1.0scale only. M00–M15 complete; see the final Results for exact checks, baseline lint/format exceptions, coverage and optional review.
