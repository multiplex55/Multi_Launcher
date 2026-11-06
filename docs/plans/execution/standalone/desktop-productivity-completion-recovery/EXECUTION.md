# Desktop Productivity Completion and Acceptance Recovery

## R01/M0-A — baseline

The sole checkout is G:/Repos/rust/Multi_Launcher. The clean incoming branch was feat/silent-screen-recorder at 05153ac1af7cdd015f72c1d95dd4a8bcfed0d043, whose only change after M17-B was adding the recovery Goal and Kickstart. The exact authorized source snapshot is 1852f264dae69056f79bc0c3b593c70331fa620f. The upstream reflog, original push log and current original receipt agree that this was the pushed M17-B tip. The origin is multiplex55/Multi_Launcher. The index was empty and there was no ongoing Git operation. R01 was created directly from that snapshot.

Development host reports Windows build 19045. Rust 1.99.1 and Nextest 0.9.135 are present. Original M17 verification reported 444 distinct affected cases passing across the first run and one exact correction, with no full N1–N12 observations. This is inherited evidence only. Exact original case names and results are in ignored target/desktop-productivity-expansion/m17-accepted-test-names.txt and m17-focused-run.log. Narrow original native results remain distinct from the full gates below.

All four preserved-source archives match the SHA-256 checksums in the [original stop reports](../desktop-productivity-expansion/stalls). The C01 archive is a failed candidate and does not apply cleanly to M17-B. Recover relevant parts against current accepted source.

| Category | Recovery checkpoints | State at baseline |
| --- | --- | --- |
| R01 | M0-A, M1-A Diagram click paths, M1-B shared geometry and focus | M0-A |
| R02 | M2-A document/macro, M2-B settings/mouse, M2-C file/clipboard/developer/time, M2-D references and new utilities | Pending |
| R03 | M3-A bit model, M3-B inspector/Copy/Insert, M3-C comparison/reference | Pending |
| R04 | M4-A groups/swimlanes, M4-B starters, M4-C PNG/SVG, M4-D grouped draw.io | Pending |
| R05 | M5-A safe Explorer operation, M5-B list editor/launch, M5-C utility/recorder integration | Pending |
| R06 | M6-A focused verification, M6-B manual packet, M6-C final receipt | Pending |

R01 starts with the original two exact failing test names. Their bodies were in the inactive archived patch. The initial attempt will distinguish unavailable cases from behavioral failures. Reconstruct the fixture and necessary production wiring, diagnose the click at the enabled/visible retained-pass boundary, then check ordinary Dock return and retained session identity. Use independent review and the focused correction budget in the recovery Goal. Full native checks are deferred to the final packet.

## Inherited category and original 50-ID status
### Category results

| Category | Retained implementation and review | Mandatory gaps |
| --- | --- | --- |
| C01 — Utility hosts | Reference Calculation Pad session/accepted-pass wiring and shared geometry storage; 12 + 22 focused, reviewed | M1-B.2 archived after 52/54 and two unresolved docked Diagram clicks. Remaining ordinary hosts, focus/return/picker/geometry wiring, M2–M4, IDs 13–23; partial 24–26; N1–N4 |
| C02 — Calculation | Exact integers, legacy scalar source, unit algebra and executable 30-example catalog; 18/46/69/3 checkpoint tests, reviewed; M17 scalar/minute overlap fixture passed | Separate inspector/bitfields/copy/insert/comparison, IDs 5–6, 8, 11; partial 12; N5 |
| C03 — Diagram | Legacy Mermaid and bounded visual authority; basic transforms, snapping, anchors, assets/presets, recents/copy; reviewed ungrouped draw.io and internal rich PNG/SVG encoder; M17 direct/pending late Save As collision fixture passed | Archived group/container/swimlane, visual starter and rich Studio PNG/SVG publication increments; grouped conversion/interchange, independent reader, common hosts, N6/N7. Partial selected groups traced in ID audit |
| C04 — Explorer tabs | Ordered operation and guarded read-only Windows observations, saved-list store; 21 + 9 focused, reviewed | Windows 10 build 19045 lacks supported tab observation. No verified mutation, editor/launcher/progress/cancel/typed route or common host; ID 61 partial and N8 unverified |
| C05 — Folder sizes | Read-only, bounded logical per-entry metrics, docked controls, File Search handoff and exact Activity; 15/10/9 focused and disposable-tree/hard-link native evidence, reviewed | Shared detach/return/focus/geometry, ID 88 partial and full N9 unverified |
| C06 — File locks | Read-only Restart Manager and exact process identity, docked controls, safe File Search prefill; 15/14 focused including exact held/released owned helper native evidence, reviewed | Shared detach/return/focus/geometry, ID 89 partial and full N10 unverified |
| C07 — Recorder | Exact recorder session and WGC source; independently decoded owned-window GIF and video-only H.264 MP4; docked setup/retained compact controls, actual shortcut/admission and lifecycle tests, verified repeated active discard and exact resource cleanup; configured review | Full N11/N12: actual launcher hidden-root stop, global chord and pointer/still capture/live Pinboard coexistence, monitor/DPI; ordinary setup common host remains docked |

Primary owners: existing launcher/utility host state; calculation_pad exact scalar/programmer/quantity boundary; diagram document/persistence/visual projection and draw.io; explorer_tabs read-only operation and store; folder_size/file_lock adapters and their docked GUI actions; recording session/controller/encoders and existing hotkey runtime.

### Selected requirements

Scope is 50 IDs from Goal E.4. Native N1–N12 remain separate from automated checks and isolated adapter/artifact evidence.

| Category / IDs | Retained implementation and prior evidence | Incomplete or unverified |
| --- | --- | --- |
| C01: 13, 14, 15 | Existing owners traced; no new M2 source | Notes/each editor, Markdown Studio, MkMacro common host migration and native N2 |
| C01: 16, 17, 18 | Existing owners traced; no new M3 source | Main/Dashboard and Mouse/Gesture Settings/common host, native N3 |
| C01: 19, 20, 21, 22, 23 | Each existing utility owner traced; no new M4 source | All named file, clipboard, developer, time and file/image migrations; N4 |
| C01: 24, 25, 26 | M1-A accepted reference lifecycle and shared provenance, 12/12; M1-B.1 geometry model and persistence, 22/22 | M1-B.2 52/54 with two unresolved docked Diagram click paths, full increment archived/inactive; shared wiring, geometry/focus/return, parentage, N1 |
| C02: 1, 2, 3, 4, 7, 10 | M5-A/B exact integer model, grammar, operations and scalar compatibility 18/18 and 46/46 | Native N5 complete end-to-end path unavailable |
| C02: 5, 6, 8, 11 | None | M6 separate inspector, working-copy bitfields, labels/copy, revision-fenced insert/undo, comparison absent due C01 dependency |
| C02: 9 | Unit algebra M7-A all69 distinct; M17 verified legacy scalar minimum and signed operands coexist with minutes and dimension diagnostics | No claim of broad native workflow |
| C02: 12 | M7-B retained reference and executable 30-example catalog; three exact tests | Missing M6 tools and corresponding interactive/reference completion |
| C03: 27, 28, 34, 35 | M8 retained dual authoring, legacy v1/v2 and rich bounded v3, styles/layers/projected editor and viewer, 38/38/16 distinct; M17 verified late-collision-safe native Save As | N6/N7, common host transitions |
| C03: 29, 30, 31, 32, 33, 36, 37, 39, 40, 41 | M9 retained transforms, snapping, navigation/minimap, anchored connectors, local embedded raster/presets (19/11/20/18 distinct) | Group/container/swimlane membership and descendant transform M9-B.2 inactive at 15/16; grouped conversion and interchange unavailable; full N7 |
| C03: 38, 42, 43, 45, 46 | M10 retained Mermaid assistance, ungrouped visual-copy, exact duplicate, recents, internal visual PNG/SVG encoder (11/17/7 distinct) | Visual template UI 14/15 and Studio visual PNG/SVG controls/publication 15/16 inactive; native N6/N7 and grouped conversion |
| C03: 44 | M11 bounded uncompressed single-page ungrouped draw.io, reviewable isolated import/native copy and current committed scene export, no clobber; 9/8/6 across adapter/Studio/docs | Grouped interchange depends on M9-B.2; compressed optional not attempted; independent external reader unavailable |
| C04: 61 | Ordered operation, guarded read-only Explorer Shell observations and saved-list store, 21/9 distinct | Windows 10 lacks supported Explorer tabs; no positive tab/path attribution, native mutation, editor, launcher route, progress/cancel or ordinary host; N8 |
| C05: 88 | Bounded metadata-only scan, docked UI, sorted rows/File Search and exact activity, 15/10/9 distinct; disposable Windows tree accounting | Shared detach/return unavailable, full N9 unverified |
| C06: 89 | Read-only Restart Manager, exact process identity and docked actions, 15/14 distinct incl exact owned held-file adapter | Shared detach/return unavailable, full N10 unverified |
| C07: 119 | M15 bounded WGC and silent GIF/H.264 MP4; M16 typed discovery, exact setup/controller, user result, capture interlocks and optional Stop; 15 + 15 launcher/shortcut focused cases, owned-window GIF and MP4 independently decoded and repeated active discard/retirement proven | Full N11/N12 (interactive launcher, actual global chord/pointer/still capture/live Pinboard, monitor/DPI); ordinary setup common detach/return absent under C.2 |

Optional: 128-bit arithmetic and compressed draw.io import were not claimed or counted against required behavior. Audio is outside scope.


## Original 38 milestone inventory

See the inherited category results above for baseline disposition. The [original ledger](../desktop-productivity-expansion/EXECUTION.md) retains checkpoint-specific evidence. These entries are the original milestone inventory and do not imply recovery acceptance.

| Milestone | Original scope |
| --- | --- |
| M0-B | Bounded baseline and host inventory |
| M1-A | Reusable single-session host lifecycle |
| M1-B | Geometry, focus and host controls |
| M2-A | Notes and Markdown hosting |
| M2-B | MkMacro editor hosting |
| M3-A | Main Settings and Dashboard Settings hosting |
| M3-B | Mouse Settings and Gesture hosting |
| M4-A | File workflow dialogs |
| M4-B | Clipboard workflow dialogs |
| M4-C | Developer utility dialogs |
| M4-D | Time and image/file utility dialogs; host coverage closure |
| M5-A | Exact integer domain and checked operations |
| M5-B | Grammar, references and compatibility |
| M6-A | Separate programmer result inspector |
| M6-B | Side-by-side calculation comparison |
| M7-A | Dimensional arithmetic |
| M7-B | Examples, reference and calculator acceptance |
| M8-A | Authoring modes and backward-compatible document format |
| M8-B | Rich scene objects, styles and layers |
| M9-A | Direct manipulation and navigation |
| M9-B | Connectors, containers and reusable assets |
| M10-A | Mermaid assistance, templates, conversion and library |
| M10-B | Current-scene exports and viewer parity |
| M11-A | Bounded draw.io XML interchange |
| M11-B | Diagram usability acceptance and documentation |
| M12-A | Targeted Explorer tab operation |
| M12-B | Saved folder lists and command integration |
| M13-A | Bounded progressive scan and accounting |
| M13-B | Folder-size interface and launcher actions |
| M14-A | Restart Manager inspection boundary |
| M14-B | File lock inspector UI and actions |
| M15-A | Recording target and bounded capture lifecycle |
| M15-B | Streaming animated GIF output |
| M15-C | Video-only MP4 output |
| M16-A | Recorder controls and command integration |
| M16-B | Recording interoperability and cleanup acceptance |
| M17-A | Scoped cumulative verification and final review |
| M17-B | Final delivery, status and next-tip handoff |

### Common host acceptance audit

Scope: Goal C.2. Existing owners and routes are listed in the baseline inventory in [original execution ledger](../desktop-productivity-expansion/EXECUTION.md). M1-B.1 verified the shared geometry storage and recovery behavior in isolation; it did not wire native geometry and Dock/Pop Out controls into each surface. Baseline behavior and focused domain tests are not counted as common-host migration evidence.

| Surface | Implementation | Host state test | Native status |
| --- | --- | --- | --- |
| Quick Notes | Existing host; common migration absent | Unrun; no migration wiring | Unverified N2 |
| Each note editor | Existing host; common migration absent | Unrun; no migration wiring | Unverified N2 |
| Markdown Studio | Existing host; common migration absent | Unrun; no migration wiring | Unverified N2 |
| MkMacro | Existing host; common migration absent | Unrun; no migration wiring | Unverified N2 |
| Main Settings | Existing host; common migration absent | Unrun; no migration wiring | Unverified N3 |
| Dashboard Settings | Existing host; common migration absent | Unrun; no migration wiring | Unverified N3 |
| Mouse Settings | Existing host; common migration absent | Unrun; no migration wiring | Unverified N3 |
| Mouse Gestures | Existing host; common migration absent | Unrun; no migration wiring | Unverified N3 |
| Gesture Settings | Existing host; common migration absent | Unrun; no migration wiring | Unverified N3 |
| File Search | Existing host; common migration absent | Unrun; no migration wiring | Unverified N4 |
| Diff | Existing host; common migration absent | Unrun; no migration wiring | Unverified N4 |
| Batch Rename | Existing host; common migration absent | Unrun; no migration wiring | Unverified N4 |
| Search & Replace | Existing host; common migration absent | Unrun; no migration wiring | Unverified N4 |
| Clipboard History | Existing host; common migration absent | Unrun; no migration wiring | Unverified N4 |
| Snippets | Existing host; common migration absent | Unrun; no migration wiring | Unverified N4 |
| Clipboard Modify | Existing host; common migration absent | Unrun; no migration wiring | Unverified N4 |
| Paste Sequences | Existing host; common migration absent | Unrun; no migration wiring | Unverified N4 |
| JSON | Existing host; common migration absent | Unrun; no migration wiring | Unverified N4 |
| XML | Existing host; common migration absent | Unrun; no migration wiring | Unverified N4 |
| Regex | Existing host; common migration absent | Unrun; no migration wiring | Unverified N4 |
| Table | Existing host; common migration absent | Unrun; no migration wiring | Unverified N4 |
| Timer/alarm authoring | Existing host; common migration absent | Unrun; no migration wiring | Unverified N4 |
| Calendar root | Existing host; common migration absent | Unrun; no migration wiring | Unverified N4 |
| Calendar event editor | Existing host; common migration absent | Unrun; no migration wiring | Unverified N4 |
| Calendar event details | Existing host; common migration absent | Unrun; no migration wiring | Unverified N4 |
| Date & Time Studio | Existing host; common migration absent | Unrun; no migration wiring | Unverified N4 |
| Image Compare | Existing host; common migration absent | Unrun; no migration wiring | Unverified N4 |
| Duplicate Finder | Existing host; common migration absent | Unrun; no migration wiring | Unverified N4 |
| ZIP Extractor | Existing host; common migration absent | Unrun; no migration wiring | Unverified N4 |
| Calculation Pad | Reference migration retains the existing dock/pop-out session via M1-A; shared geometry/focus integration from B.2 is inactive | M1-A accepted-pass, identity, dirty/history and pending decision coverage; full B.2 wiring unverified | Unverified N1 |
| Diagram Studio | Existing native editor; shares M1-A pass provenance; B.2 docked click paths, ordinary detachment and picker parentage unaccepted | M1-A exact provenance passed; B.2 failures retained in stop report, inactive | Unverified N6/N7 |
| Diagram viewer | Existing separate native/embedded view of the same owner; M1-A pass provenance only | M1-A close/reopen generation and embedded fallback passed; no common Dock/Pop Out wiring | Unverified N6/N7 |
| Programmer inspector | M6 owner/UI absent | Unrun; no implementation | Unverified N5 |
| Explorer saved-list editor/launcher | Ordered operation and saved-list store only; editor/launcher absent | Domain tests passed; no UI host state | Unverified N8 |
| Folder Size Analyzer | Docked owner/actions delivered; common detach/return absent | Docked generation/picker/selection/cancellation state tested; no migration test | Unverified full N9; narrower disposable Windows tree metadata passed |
| File Lock Inspector | Docked owner/actions delivered; common detach/return absent | Docked generation/prefill/action safety tested; no migration test | Unverified full N10; narrower held-file adapter with owned PID/creation time passed |
| Silent recorder setup / compact controller | Docked setup and dedicated retained compact controller, including hidden root, live Stop/Discard, and retained output result; ordinary setup detach/return absent | Exact hidden and stale Watch/retained Stop, reciprocal capture guards, and exit wait for exact retirement passed; optional shortcut both routing modes tested | Full N11/N12 unverified; owned HWND WGC→GIF and WGC→MP4 independently decoded, repeated active discard and exact owned cleanup passed |

File Search is still docked; retained folder-size and file-lock prefill/reveal interactions are separate, covered workflows. A recorder rectangle selector is a specialized overlay excluded from ordinary Dock/Pop Out under C.2.

## Original native gates

All gates below are NOT_RUN as full end-to-end flows at the recovery baseline. The recovery brief authorizes delivering safe implementation before final user verification. The original narrower observations are recorded in the category and host matrices.

| Gate | Required observation |
| --- | --- |
| N1 — Reference host | Dirty Calculation Pad → pop out → hide launcher → edit/undo/copy → dock → cancel close. Same source/history remains, focus/Jump Mode belongs to the correct surface. |
| N2 — Document and macro hosts | Notes and Markdown editable detached sessions; MkMacro harmless fixture retaining selection and exactly-once execution. Owned child prompts are reachable. |
| N3 — Settings/mouse hosts | Main and Dashboard Settings save/cancel, Mouse/Gesture settings edit/record/cancel; no draft auto-commit or residual input suppression. |
| N4 — Remaining utility families | Representative file workflow, clipboard workflow including second sequence run, structured editor/table, timer/calendar and image/ZIP inspection. Every requested surface still needs an automated wiring check even when native evidence is sampled by family. |
| N5 — Programmer tools | Large exact result and shifted mask → independent inspector → bit toggle → copy chosen format → explicit insert/undo → comparison. Ordinary scalar pad remains text-first. |
| N6 — Mermaid legacy | Existing `.mld`, supported source editing, invalid/stale preview, viewer and current export; exact source/dirty decisions survive. |
| N7 — Rich diagram | Add shapes/text/image, connect, resize/rotate, multi-select/align, use container/layer, undo, save/reopen, PNG/SVG export and supported draw.io interchange. |
| N8 — Explorer tabs | Five ordered test folders in one identified new window with another Explorer window already open. Verify paths, tab order and unrelated window preservation; cancel/foreground-loss test stops safely. |
| N9 — Folder sizes | Known temporary tree → progressive sorted results → cancel → refresh → detach/dock; totals and incomplete status match expected accounting. |
| N10 — File users | Hold a disposable file in a known helper → inspect → release → refresh. Confirm identity and truthful empty/failed presentation. |
| N11 — Recording | Record moving controlled content to GIF and MP4; hide root and stop; independently inspect/play artifacts, duration/dimensions and absence of audio. Include non-primary/DPI geometry where available. |
| N12 — Cleanup | Cancel selection/countdown/recording, close target, repeat start/stop, exit task app; normal mouse/hotkeys/still screenshot/live Pinboard remain usable. Inspect only task-owned resources/processes. |

## R01/M0-A delivery receipt

Baseline commit b430934723e4838cfaf35419b9bf2d762148b8e2 has exactly one parent, 1852f264dae69056f79bc0c3b593c70331fa620f. The normal upstream push succeeded. The commit contains only the two recovery ledger files.

## R01/M1-A — Diagram host click recovery in progress

Scope is the Diagram Studio owner and its existing read-only Viewer, original IDs 24–26 and C01/M1-B.2. The first product increment changes src/gui/diagram_studio.rs and the associated LauncherApp completion and input routing in src/gui/render.rs. Retain the current initial native presentation and keep document identity separate from host identity. Stage host changes in the existing retained pass. Gate Studio movement while a viewport-bound picker, import or dirty decision owns the surface. Use one shared render path per body and keep native/embedded fallback routes. M1-B will wire native geometry and picker ownership.

- Baseline exact Nextest list exited 0 with zero matching names because both historical tests were archived. No runtime result at baseline.
- Initial reconstructed list exited 0 with two exact matches. Initial run exited 100 with 0/2. Both fixtures reached Dock, then hit egui's root Background layer instead of the newly mounted Diagram Studio window. This happens before the tested click dispatch. The first window pass is an invisible egui sizing pass, which requests another frame before pointer input can target the window. The correction renders the next visible pass and requires a visible enabled response, full clipping containment and the expected hit layer before actual pointer press and release.
- Independent review also identified a nested Context lock, launcher/Diagram keyboard ownership leakage, dropped docked viewer focus, root title-bar gating, and admission of viewport-bound work between draw and completion. Corrections are in the second focused candidate. No full native or user acceptance evidence.
- Logs are under ignored target/desktop-productivity-completion-recovery. Exact initial filter was test(/::diagram_owner_dock_and_pop_out_keep_the_same_document_generation$/) | test(/::focused_docked_viewer_owns_root_close_shortcuts_from_docked_owner$/). Both list and run used --locked --offline -p multi_launcher --lib --cargo-profile fast-dev, with --no-fail-fast on run.

### R01/M1-A acceptance

- The first correction list initially exited 101 because two pre-existing direct renderer fixtures needed the new keyboard authorization argument. It executed no tests. The corrected exact list matched 3 and all 3 passed on the first runtime correction.
- Review recheck found docked Viewer ownership could be lost when focus moved to another control on its layer, and egui's collapsible window could omit the body callback needed for Close. The final candidate tracks live docked focus by layer and keeps the host expanded. The Viewer test also exercises a click away and a real Pop Out click with unchanged generation and camera.
- Final list and run each exited 0. The list matched exactly 13 and the run passed 13/13, with 7411 skipped. Final filter: test(/::(diagram_owner_dock_and_pop_out_keep_the_same_document_generation|focused_docked_viewer_owns_root_close_shortcuts_from_docked_owner|diagram_visual_host_discards_unretained_dock_click|diagram_studio_visual_canvas_escape_cancels_owned_gesture_before_close|diagram_studio_root_discard_does_not_publish_child_focus_intent|diagram_studio_embedded_fallback_uses_parent_surface_without_claiming_child_identity|diagram_studio_same_frame_source_edit_survives_ctrl_w_close|diagram_studio_os_viewport_close_requests_cancel_until_dirty_decision|studio_usability_diagram_viewer_uses_a_distinct_callback_and_independent_camera|studio_usability_diagram_viewer_discard_close_and_reopen_are_generation_scoped|studio_usability_diagram_viewer_owner_dirty_decisions_keep_or_retire_viewer|studio_usability_diagram_viewer_bootstraps_when_root_input_is_hidden|studio_usability_diagram_viewer_embedded_fallback_keeps_root_host_identity)$/). Same flags as above. Run SHA-256: F5B09A124CC6997A1BB30F4FE5F4C34CF0A243FB692D23C93BE8FDB2A54232F7.
- Production cargo check --locked --offline -p multi_launcher --lib --bin multi_launcher --profile fast-dev exited 0. Check SHA-256: 8AA684C93A789853759B5F6EC8EC4089E8878EB5668793416FA146E60CFE97EA. Warnings are outside the changed paths, including existing table and vendored dependency warnings. Scoped Rust formatting and git diff --check passed. Independent review findings were addressed. Scope is Diagram Studio, Viewer and Launcher routing.
- M1-A automated host interaction is verified. Real native N6/N7 remain unverified. M1-B still owns geometry, host-qualified native pickers and the rest of the focus/presentation contract.

## R01/M1-B — shared geometry and focus in progress

R01/M1-A source and reporting commit 9057106f13492004bc25a57e8900d916d27aa290 was pushed, upstream verified equal, before M1-B source work. M1-B covers original C01/M1-B and IDs 24–26 for the reference hosts only.

The shared utility size manager owns acknowledged geometry writes and per-viewport gesture observations. Native HWND attachment matches current root process and UI thread, unique window title and retained physical viewport bounds, and installs a property token checked at use. Diagram pickers use the accepted owner identity and report an unavailable parent instead of opening under another window. Native position is issued first, then a retained destination sample is used for size. Bounded fallback clamps to the actual monitor dimensions. Detached Pad and Diagram surfaces have hidden-root rendering routes, and docked Diagram content is skipped while hidden. Taskbar interaction for reset/fallback and mixed-DPI end-to-end behavior still need real native validation.

- First production check was compile-only, exit 101. Six picker functions borrowed path references before acquiring the parent-bound picker. Acquire the picker first.
- Initial exact Nextest list matched 13. Initial run exited 100, 12/13 passed. The reset-then-move fixture used a 2.0 child sample for physical geometry recorded at 1.0. The correction models consistent viewport scale and a retained placement before resize.
- Independent review also identified command-order DPI risk, missing monitor clamp on reset, and exhausted attachment retries after transient readiness failure. The second candidate stages native move/resize across retained samples, clamps reset, and renews bounded attempts on material retained geometry change. Read-only review recheck reported no remaining blocking findings in those corrections.
- The correction had two compile-only failures from partial formatter application and one missing qualification. The rendering callback ranges were reconstructed from accepted M1-A with the intended M1-B changes and parsed successfully. No tests ran in the failed lists.

- Final exact list matched 14. Final run exited 0, 14/14 passed, 7424 skipped. Both used --locked --offline -p multi_launcher --lib --cargo-profile fast-dev with the following exact filter: test(/::(native_restore_placement_waits_for_a_new_destination_scale_sample|utility_restore_uses_current_child_scale_for_cross_monitor_commands|move_across_dpi_keeps_preferred_physical_size_and_updates_position_scale|coalesced_resize_then_clamped_move_keeps_explicit_size_and_latest_position|same_tool_viewports_keep_latest_position_and_latest_explicit_size|retired_and_stale_generation_observations_are_discarded|retiring_one_viewport_keeps_same_tool_peer_registration_live|utility_size_integration_native_geometry_reset_is_not_reapplied_by_later_ack|utility_size_integration_reset_then_move_before_ack_reopens_at_factory_size|explicit_examples_and_dock_requests_emit_show_root_only_after_host_acceptance|native_popout_child_discard_and_os_close_keep_the_edited_session|diagram_hidden_root_skips_docked_owner_but_keeps_native_viewer|diagram_owner_dock_and_pop_out_keep_the_same_document_generation|focused_docked_viewer_owns_root_close_shortcuts_from_docked_owner)$/). Run also used --no-fail-fast. Run SHA-256: 138B18FF81EC5C38F0D0FEAEA701D72D01A86F62B6AA54B5F1019FE311294D85.

- Production cargo check --locked --offline -p multi_launcher --lib --bin multi_launcher --profile fast-dev exited 0. Check SHA-256: DAD6653AFC68B046D7A7428EE12E4C61CDA712D3D75448F3D9A3AA58132EC3B4. Scoped Rust formatting, parser checks and git diff --check passed. Existing table and dependency warnings remain.
- Review complete for R01. The full Windows flows remain unverified. No successful real picker, mixed-DPI or end-to-end native result was inferred from compiled bindings or harness tests.

| Reference surface | Implemented host behavior | Verification | Native status |
| --- | --- | --- | --- |
| Calculation Pad | Retained Pad session, shared labels and docked size, native geometry, hidden-root route | Accepted ShowRoot, discarded child, dirty close, and shared geometry tests | IMPLEMENTED_USER_VALIDATION_PENDING (N1) |
| Diagram Studio | Same document and revision across Dock/Pop Out, root/native input separation, retained native geometry and owner-qualified pickers | Both historical interaction names recovered; production picker branch compiled; hidden-root and retained-pass coverage | IMPLEMENTED_USER_VALIDATION_PENDING (host subset of N6/N7) |
| Diagram Viewer | Same owner and independent camera, Dock/Pop Out, focused layer, native geometry | Real harness click, focus, return and camera checks; native and embedded fallback coverage | IMPLEMENTED_USER_VALIDATION_PENDING (host subset of N6/N7) |

R01 closes after this checkpoint and its normal push attempt. R02 starts from the actual committed R01 tip. Remaining shared behavior for individual utility surfaces is tracked under R02 and later categories. The exact committed tip and push are recorded in the ignored receipt.

## R02/M0-A — named utility host migrations

R02 starts from the clean, pushed R01 final tip b8c3f259cbd55d483fb1346b262026ed42d5d820. The branch is feat/productivity-host-migrations. The existing source snapshot includes the verified Diagram click paths, shared reference geometry and focus routing, and accepted-host picker binding. R01's final tip and upstream were identical at transition. No new worktree or intermediate merge was used.

- M2-A starts with Markdown Studio, the smallest document consumer, then Quick Notes and independent note editors, then MkMacro. Reuse retained pass and presentation identity with one live owner per logical instance. Preserve source-before-intent ordering, focus and Unicode selection, note history and prompts, and exactly-once macro execution.
- M2-B follows for Main, Dashboard, Mouse, Gestures and Gesture Settings. Preserve drafts and recording ownership. M2-C follows in small file, clipboard, developer and time/image slices. M2-D closes reference and new utility host wiring. Surface-level requirements are in the baseline matrix above.
- Host moves are presentation changes. Each surface retains its own document/worker owner and explicit close or save decisions. Test state and actual visible actions per host and keep native evidence separate.

### R02 baseline and exact brief restoration

R02/M0-A 9fb5099a6cc1d4d8f9bf384be0afa038e97a9a42 has direct parent b8c3f259cbd55d483fb1346b262026ed42d5d820 and was pushed. At the user's request, the two recovery briefs absent from R02 were restored in docs-only commit 2db5aae61e35026e73b3da34c2c32d94508c12d1 and pushed. Both committed Git blobs match 05153ac1af7cdd015f72c1d95dd4a8bcfed0d043 exactly. Markdown source work stayed unstaged and separate.

### R02/M2-A.1 — Markdown Studio in verification

Markdown Studio retains one document across root and native hosts, with stable body and TextEdit IDs. It stages edits and intents behind both the originating and root pass, applies source before intents, and reuses the accepted HWND capability for owner-qualified pickers. A detached Studio has its own input and focus route. The launcher close route skips that child when it selects an underlying root dialog. Idle retained child frames drive native geometry and focus.

Initial Nextest list matched 13, and 13/13 passed. The selected set includes new actual host clicks and root close routing, plus existing document, Unicode selection, dirty close, export and Diagram handoff tests. Independent review identified a final root ownership gap and nested egui lock; both were corrected. The final focused run and Windows production check are pending.

After root ownership correction, 12/13 passed. The strengthened focused Unicode fixture exposed egui collapsing the cursor on the host toolbar press while the TextEdit still held focus. Keep the logical source range on outside pointer activation and let actual edits or selection drags rebase it. The production check exposed one export picker borrow-order error. Acquiring the owner-qualified picker before borrowing the filename corrected it. The exact Unicode correction subset then passed 3/3 and production check exited 0. The new launcher close fixture initially used a shared app-data root and generated a default clipboard-modifier config; the generated file was preserved under ignored evidence and the fixture now uses the isolated app workspace. A final focused subset rechecks that isolation and the simultaneous edit guard.

Final exact list matched 4 and the affected rerun passed 4/4, 7436 skipped. The unaffected initial cases retained their prior pass evidence. The commands used --locked --offline -p multi_launcher --lib --cargo-profile fast-dev and exact filter test(/::(markdown_studio_visible_host_buttons_pop_out_and_dock_the_same_document|root_close_skips_detached_markdown_and_closes_underlying_dialog|markdown_studio_keeps_logical_unicode_selection_when_toolbar_takes_focus|markdown_studio_format_toolbar_preserves_selected_unicode_source_range)$/), plus --no-fail-fast for run. Run SHA-256: C822692002B9C7603F1D7624733A6516064C71C8A03DE26CA6A6A1E280B39C88. Final production cargo check --locked --offline -p multi_launcher --lib --bin multi_launcher --profile fast-dev exited 0, SHA-256: 141D1B65591BC0A58ECA33ACEADAA2FEA9F7990126F2757849627C6548548588. Scoped Rust formatting and git diff --check passed. Independent review cleared both final root shortcut findings. Picker ownership compiled and native GUI validation remains pending.

| Surface | Implemented behavior | Verification | Native status |
| --- | --- | --- | --- |
| Markdown Studio | One dirty document, selection and undo history across Dock and Pop Out, retained host effects, root/child focus and owned native pickers | Real harness pointer clicks, isolated launcher close routing, dirty close, Unicode, save, export and Diagram handoff regression checks | IMPLEMENTED_USER_VALIDATION_PENDING (N2) |

M2-A proceeds next through Quick Notes, independent note editors, and MkMacro in the same R02 branch.

### R02/M2-A.2a — Quick Notes in progress

Source starts at pushed Markdown checkpoint 6f611c95c960ebb3d03fcc100339dfdde47b2c5d. This slice covers Quick Notes only, within original ID13/C01 M2-A. Independent note editors are the following slice. Keep the Notes draft and template owner, use one retained body in docked or Immediate presentation, preserve the root-only query bridge and native child Jump Mode, and deliver exact retained save, close and open effects after the owner is restored to LauncherApp. Test real visible moves, discard, hidden-root input and existing draft/row popup routing.

The initial working review found template owner input and persistence, fallback window duplication, capture/activation ordering, and discarded draft mutations needing corrections. Candidate is unaccepted while these are being addressed. No Quick Notes tests or production check have run on the candidate.

The two initial list attempts failed at compile time, first from an extra borrowed Jump Mode scope and stale bootstrap array length, then from captured draw closure borrows. Neither executed tests. The corrected list matched exactly six. Initial runtime passed 5/6, including the actual Pop Out/Dock/discard case, row popup ownership and draft/save cases. The query-hint regression failed on clicking back from the launcher query and typing into the Notes draft. The docked and embedded body had inherited a keyboard-focus eligibility gate, making the previous frame's pointer hit disabled. The correction keeps the body eligible for egui hit testing while retaining separate keyboard and Jump Mode ownership gates. The exact query case and the two affected click/popup cases are in the correction run.

The initial run filter was test(/::(quick_notes_popup_owner_stays_with_identity_offscreen_then_closes_on_removal|quick_notes_publishes_a_successful_edit_after_the_editor_closes|notes_query_hint_focuses_actual_query_and_returns_to_quick_notes_draft|quick_notes_long_edit_keeps_footer_reachable_without_mutating_draft|quick_notes_defers_drafts_and_keeps_last_good_candidate_until_recovery|quick_notes_visible_host_buttons_move_the_same_draft_and_discard_stale_host_intent)$/). The correction list matched three, and the run passed 3/3, including exact query return, actual host buttons/discard, and popup ownership. Correction filter: test(/::(notes_query_hint_focuses_actual_query_and_returns_to_quick_notes_draft|quick_notes_visible_host_buttons_move_the_same_draft_and_discard_stale_host_intent|quick_notes_popup_owner_stays_with_identity_offscreen_then_closes_on_removal)$/). Both used cargo nextest with --locked --offline -p multi_launcher --lib --cargo-profile fast-dev, plus --no-fail-fast for run. Correction run SHA-256: 6BBBB35F07C175DEE04D263E3C44AFE071E657CC0F74B515A0A9C58E03A3293F. Production cargo check --locked --offline -p multi_launcher --lib --bin multi_launcher --profile fast-dev exited 0. Check SHA-256: 4D53C22106896675ECD9261CF1B1CF72D778132268227BB9A1D1F6AA341CE7D2. Rustfmt parser checks and diff whitespace checks passed. Whole-file formatting was omitted to avoid unrelated churn. Independent review cleared the corrected host and input paths.

| Surface | Implemented behavior | Verification | Native status |
| --- | --- | --- | --- |
| Quick Notes | Single draft across Dock/Pop Out; retained save, template and external intents; owner-specific keyboard and Jump Mode; launcher query return; hidden-root host route | Real host buttons with discard, draft/save, popup and query-return checks. Initial 5/6 and exact correction 3/3 | IMPLEMENTED_USER_VALIDATION_PENDING (N2) |

Next is M2-A.2b independent note editors on the same R02 branch, followed by MkMacro.

### R02/M2-A.2b — Independent note editors

Source starts at pushed Quick Notes checkpoint aff2d8fab2112ee0f50140850d82aa4fe71ad2a4. This slice covers independent note editors within original ID13/C01 M2-A. Each editor retains its draft, undo history, child prompts and logical identity across Dock and Pop Out. Root shortcuts select an eligible docked note or underlying dialog. The detached editor owns its Find and close shortcuts, stays live with the launcher hidden, and uses the accepted host for attachments and export. Note and image links, saves, and existing Diagram handoffs use the originating editor's retained pass. Launcher note links raise the resolved docked editor, including alias and new draft targets.

The first list attempt exposed compile errors and executed zero tests. After correction, the list matched 10. The initial run passed 9/10. The new launcher test failed because its fixture had opened the underlying QR dialog without publishing its panel stack state. The fixture now initializes the order before inserting notes. The six-case rerun passed 6/6 and the production check exited 0. Initial and rerun SHA-256 values are 328B8B8027322123FB3844D8FA4FBDB150CEE82B569C258BBEA57121EADAC2FA and 31D3EB8EA569D59B304E0BB3083DB1996541E72B773ECE040703689FE9BD36BD. A test-generated default clipboard modifiers file was preserved under ignored evidence.

Independent review cleared the host identity, dirty draft rollback, prompt focus, and root and detached close boundaries. It also found that embedded Attach/Export needed the verified root owner, link activation needed to use the resolved target, and native Find needed to handle Ctrl+F and Escape before editor close. The re-review cleared all three corrections.

For the final run, `$f = (Get-Content target/desktop-productivity-completion-recovery/r02-m2-a2b-final-filter.txt -Raw).Trim()`. `cargo nextest list --locked --offline -p multi_launcher --lib --cargo-profile fast-dev -E $f` matched exactly 14. `cargo nextest run --locked --offline -p multi_launcher --lib --cargo-profile fast-dev --no-fail-fast -E $f` passed 14/14, with 7430 skipped. Run SHA-256: 5E9ADD441D0C57DB79493B028A4D7756E6B1905AAB156E8BBB2307CF1F67FF92. `cargo check --locked --offline -p multi_launcher --lib --bin multi_launcher --profile fast-dev` exited 0. Check SHA-256: D7C7A8480BF8F22309890A2971FC53B9FC161C00ECEE0682C01FFA24B92473B0. Rustfmt parsed the three touched Rust files and `git diff --check` passed. Source scope is `src/gui/mod.rs`, `src/gui/note_panel.rs`, and `src/gui/render.rs`.

| Surface | Implemented behavior | Verification | Native status |
| --- | --- | --- | --- |
| Independent note editor | Distinct live editor per logical instance, visible Dock and Pop Out, dirty draft and undo state, owner-qualified prompts and pickers, independent input and hidden-root operation | Actual harness host buttons and child Find keys, root close with same-title peers, rejected edit rollback, resolved note links, save/conflict, export, and Diagram history checks | IMPLEMENTED_USER_VALIDATION_PENDING (N2) |

Next is M2-A.3 MkMacro Editor on the same R02 branch.

### R02/M2-A.3 — MkMacro Editor

Source starts at pushed independent note editor checkpoint 7862c746d95f2e3630c8f5a2ac61ad53c1fadfc2. This slice covers the MkMacro authoring host within original IDs 13–15/C01 M2-B. The editor retains its document, selection, children and Review while changing presentation. Playback, recorder, overlay, and independent runtime and parameter prompt owners keep their existing lifetimes. Save, Run, recorder controls, and native picker requests go through accepted host completion. Dock raises the launcher, while a detached editor can run with the launcher hidden.

The first Nextest list failed to compile on a close decision type and the nested egui window response. It executed zero tests. Log SHA-256 D15DF33EE4B4C293D44DB2DC9BE135208BDD9761EDAF446F7A24D7A2093B35C8. After those corrections and review fixes for native Jump Mode and detached activation, the exact list matched eight and the run passed 8/8, with 7439 skipped. This included real Pop Out, discarded and accepted Run, Dock, root close isolation, native navigation binding, and independent runtime prompt coverage. Run SHA-256 023BAE8D095FA1FC992DFE989FB5AA0390A18D4FBFDE84608B7D1E6E35CF306F.

The expanded list matched ten and the run passed 9/10. The new detached Action Editor click test reproduced the ownership mismatch found in review. The host used a plain title string to derive the layer ID, while egui hashes its optional title. The accepted Apply click left the editor open. Run SHA-256 DE5156470530254F02043038A199E196C52C9478B1C680DDE94B4A98CFFD01ED.

The correction matched the actual layers of the Action Editor and the other title-derived child windows. Independent review confirmed the owner and generation checks. Apply, Cancel and Open Call Target use the existing action editor behavior after retained completion. For verification, `$p = 'target/desktop-productivity-completion-recovery'` and `$f = (Get-Content $p/r02-m2-a3-filter-04.txt -Raw).Trim()`. `cargo nextest list --locked --offline -p multi_launcher --lib --cargo-profile fast-dev -E $f` matched seven. `cargo nextest run --locked --offline -p multi_launcher --lib --cargo-profile fast-dev --no-fail-fast -E $f` passed 7/7, with 7441 skipped. The initial and expanded runs used the same commands with `r02-m2-a3-filter-02.txt` and `r02-m2-a3-filter-03.txt`. Final list and run SHA-256 values are 4F612609BECC434F975DE2F8B904DA1AE5B67689CF2309214A3E23A01724118E and D7F2916D9BAA5579AC1A86F9BE2065274FDE3D3968699FE2D7819D768C66FCA0. `cargo check --locked --offline -p multi_launcher --lib --bin multi_launcher --profile fast-dev` exited 0. Production log SHA-256 0ED9343A60B7A973EA3C66DBA29C46AA42EE56E351BB293542A2FED149A452EB. Rustfmt corrections were limited to changed lines and the cumulative diff passed `git diff --check`. Source scope is `src/gui/mkmacro_dialog/`, `src/gui/mod.rs`, `src/gui/render.rs`, and `src/gui/jump_mode/ui.rs`.

| Surface | Implemented behavior | Verification | Native status |
| --- | --- | --- | --- |
| MkMacro Editor | One editor across Dock and Pop Out, current-host focus, owned authoring children and pickers, preserved dirty draft and selection, and retained playback and recorder requests | Real host, Run, Apply and Cancel clicks, rejected pass checks, fake playback exactly once, Jump Mode, package import cancellation, production build | IMPLEMENTED_USER_VALIDATION_PENDING (N2) |

M2-A is complete within R02. M2-B begins with Dashboard Editor on the same branch.

### R02/M2-B.1 — Dashboard Editor

Dashboard Editor covers the Dashboard portion of original IDs 16–18/C01 M3-A. It retains one config draft, selection, and slot identities across Dock and Pop Out. The launcher and Settings command share the opener. Host and viewport provenance fence Save, Close, and presentation writes alongside the existing dashboard path and target generation checks. The detached editor stays usable while the launcher is hidden. Native confirmation and combo input use the current editor's layers, and Dock returns to the launcher without publishing the draft.

The administrative pause preserved the Dashboard worktree above `069e556ad1149684567521a2211183581b040d5f`. The first exact list exited 101 for three Settings-only `NavigationWindows` fixtures missing `dashboard_editor_layer` and ran no tests. Log SHA-256 0AA6E790D35D4B0031FE2D54BBF04012D47A678DFA527EAC6A955622627CDCE5. The user updated the implementer configuration in `9686ea0c6cafaddb6321d74ad3de0cd12459bb15`. The resumed implementer supplied `None` in those three fixtures and kept their owner assertions.

The corrected exact list matched seven and the initial runtime passed 6/7, with 7444 skipped. The failed real-click test recorded target generation before the first draw initialized the fallback draft's slot identities. The fixture now records the generation after initialization and still requires exact preservation through Pop Out, failed Save, and Dock. Run SHA-256 4CDCF432CCBD74250B046E72659B47A820AC335BAEAF697BAADD546CFECA11D0. Independent review also found a focused combo option could be rejected by the embedded host. The correction accepts the recognized popup focus and leaves Escape to the popup. Re-review cleared the correction and the fixture baseline.

The exact selectors are in `r02-m2-b1-filter-01.txt` and `r02-m2-b1-filter-03.txt` under the ignored recovery evidence directory. With `$f` set to the trimmed contents of each selector, `cargo nextest list --locked --offline -p multi_launcher --lib --cargo-profile fast-dev -E $f` matched seven initially and three for the final affected set. `cargo nextest run --locked --offline -p multi_launcher --lib --cargo-profile fast-dev --no-fail-fast -E $f` passed the final set 3/3 with 7448 skipped. Final run SHA-256 33151AC805372FBB28191C17E533831E16A31A1E333A925F6C36D53A64D7B4EA. Formatting corrections were scoped to changed lines. `git diff --check` passed.

`cargo check --locked --offline -p multi_launcher --lib --bin multi_launcher --profile fast-dev` exited 0. Log SHA-256 B2B84EB410C96C071AF0F9C99089B975B90178F6343D2CAE1FFA2603EFA4D979. Source scope is `src/gui/dashboard_editor_dialog.rs`, `src/gui/render.rs`, `src/gui/command_host.rs`, `src/gui/jump_mode/ui.rs`, and `src/settings_editor/render.rs`.

| Surface | Implemented behavior | Verification | Native status |
| --- | --- | --- | --- |
| Dashboard Editor | One draft, owned confirmations and current combo focus across hosts, retained save and close, hidden launcher support, Dock restores launcher | Real host buttons, failed Save preserving the draft, hidden-launcher routing, exact confirmation binding, retained root effects, and production build | IMPLEMENTED_USER_VALIDATION_PENDING (N3) |

Next is R02/M2-B.2 Main Settings on the same branch.

### R02/M2-B.2 — Main Settings

The Main Settings portion of original IDs 16–18/C01 M3-A starts from pushed Dashboard checkpoint `a7139ea15f694c2149492e4cb10d2af2c254f11b`. The launcher owns presentation and viewport lifecycle outside the replaceable editor draft. The existing settings save worker remains app-owned. Docked, Immediate, and embedded presentations share the editor body. Save, Cancel, Close, host moves, and cross-owner activation publish only after the current ROOT and owner passes are retained.

The initial exact list matched five and the initial run passed 5/5, with 7448 skipped. It covered real host buttons and hidden launcher routing, staged Cancel, search Escape, and settings worker completion after hiding. Run SHA-256 7CC4D3332DF274FD02E51125158FFFAA5014922B4F8588AC3D64AC4AA83EB698. Review then identified native Jump Mode configuration, ROOT Escape, child-pass rollback, pinned focus, OCR continuity, and shortcut ownership corrections. The first correction list stopped at compile time after partial formatter alignment moved a test button assignment and removed the host module declaration. Log SHA-256 42382687F8F1FFAACEB7962C2ABC456C6348640DA5DB4A2589C6781F610515B3. Both splices were corrected. The three-case correction selector is `r02-m2-b2-filter-02.txt` in the ignored recovery evidence directory.

The corrected three-case run passed 3/3, with 7450 skipped. It includes real Escape input on discarded ROOT and Immediate passes. Run SHA-256 D2D3F3D8CB71223CC9F7D362B9CA5A79C07E6E9C8384327181FF72EBCAE19077. The production check exited 0. A final review found a raised Settings window could still accept a launcher shortcut when another Middle layer retained keyboard focus. The owner guard now requires the raised layer and compatible focus. The child replay test also checks that a subsequent accepted Escape clears the search while keeping Settings open. Final two-case verification uses `r02-m2-b2-filter-04.txt`.

For verification, `$f` is the trimmed contents of the exact selector in the ignored recovery evidence directory. `cargo nextest list --locked --offline -p multi_launcher --lib --cargo-profile fast-dev -E $f` matched five initially, three for the correction, and two for the final owner change. `cargo nextest run --locked --offline -p multi_launcher --lib --cargo-profile fast-dev --no-fail-fast -E $f` passed the final set 2/2, with 7451 skipped. Final run SHA-256 701BA4091CDD23D3E3D6AE7C60075478A19123FD8E147E7A148A9517C1E3A96C. Independent review confirmed the final owner guard and the accepted child Escape control.

`cargo check --locked --offline -p multi_launcher --lib --bin multi_launcher --profile fast-dev` exited 0 on the final source. Log SHA-256 B128108F67C9C135F2551F9AF854597FAC561A01119F3AD1835A59F6726EE7A4. Scoped formatting and `git diff --check` passed. Source scope is `src/gui/{settings_window_host,mod,render}.rs`, `src/gui/jump_mode/ui.rs`, and `src/settings_editor/{mapping,render,state}.rs`.

| Surface | Implemented behavior | Verification | Native status |
| --- | --- | --- | --- |
| Main Settings | One draft and save worker across Dock and Pop Out, current-owner navigation, hidden-launcher operation, retained Save and Cancel | Visible host and Cancel clicks, discarded and accepted launcher and child Escape, hidden save-worker completion, production build | IMPLEMENTED_USER_VALIDATION_PENDING (N3) |

Next is R02/M2-B.3 Mouse Settings and Gesture Settings on the same branch.

### R02/M2-B.3 — Mouse Settings and Gesture Settings

The Mouse Settings and Gesture Settings portion of original IDs 16–18/C01 M3-A starts from pushed Main Settings checkpoint `7b1c0f03214288e09fb80c2cdf337230293c7573`. Both forms retain their existing draft across Dock and Pop Out, use the same body in ROOT and detached presentations, and stay live while the launcher is hidden. Mouse appearance Apply and the five runtime switches retain their separate semantics. Gesture Save keeps its persisted plugin, Main Settings, and runtime synchronization. Its ignore-window picker stays in the owning viewport and prevents parent shortcut and host-move takeover.

Independent review identified ROOT close bypass, detached popup input loss, Settings navigation mutation on discarded passes, and an embedded close shortcut bypass. The corrections use exact owner checks, retain recognized native combo ownership, and commit Settings search observations only with current retained ROOT and child passes. Re-review cleared those findings. Tests use actual enabled host controls and a detached combo selection, preserve an unsaved Gesture edit, exercise hidden launcher rendering and Dock, and check discarded Escape and picker ownership.

The initial exact list matched eight. A preliminary direct harness diagnostic passed six selected cases. The final exact selector is `r02-m2-b3-filter.txt` under the ignored recovery evidence directory. With `$f` set to its trimmed contents, `cargo nextest list --locked --offline -p multi_launcher --lib --cargo-profile fast-dev -E $f` matched nine (log SHA-256 B74B789A79F491653AB7B98C02BA6A65CD5B500658DE5DD11B263C72A9FCBE7A). `cargo nextest run --locked --offline -p multi_launcher --lib --cargo-profile fast-dev --no-fail-fast -E $f` passed 9/9 with 7448 skipped (run SHA-256 A3157AE3D086408AFEFE2A2DDDB50473386A57D600BE1B104EFE7955B343331D). `cargo check --locked --offline -p multi_launcher --lib --bin multi_launcher --profile fast-dev` exited 0 (log SHA-256 F4555492B1A941F201D839ADA5F747B62FF80C90767B07D4155F064287ABAEBE). Scoped formatting and `git diff --check` passed.

Source scope is `src/gui/{mouse_form_host,mouse_settings_dialog,mouse_gesture_settings_dialog,mod,render}.rs`, `src/gui/jump_mode/ui.rs`, and `src/settings_editor/render.rs`. Native N3 remains `IMPLEMENTED_USER_VALIDATION_PENDING`.

Next is R02/M2-B.4 Mouse Gestures editor on the same branch.

### R02/M2-B.4 — Mouse Gestures editor

The editor completes the Mouse Gestures portion of original IDs 16–18/C01 M3-A from pushed checkpoint `18fe3416a5b1a3701f2299536a72338fb3eb6cf4`. The editor retains its database, gesture and binding identities, selection, and recorder across Dock and Pop Out. Binding and delete confirmation remain separate exact Jump Mode owners within the active viewport. Save, Copy, and Close are published after retained ROOT and owner passes. A discarded pass restores a value checkpoint, including when Jump Mode already consumed the original key event. A recorder stroke uses its own viewport's pointer and focus, disables host movement during capture, and leaves the existing save transaction in charge of persistence and publication.

The initial exact list matched 11 and the run passed 10/11. The new detached fixture tried to locate Confirm before the child had finished sizing and Jump Mode owner admission (run SHA-256 A377417B7BDCF23BA23BB232A55B001E310BA519D8F4586C8AA230A0C63AFB84). The five-case correction proved ROOT selection rollback after a real consumed hint, the docked recorder lifecycle, and both earlier Mouse settings host round trips. Its detached case proved a discarded Confirm preserved disk and child, then hit disabled replay geometry on the next press (run SHA-256 CED9E65421E9E85EE23689870544375EB81AC682FC08E229B036DFBD24138F1F). Requesting a repaint on rejection and reacquiring the current Confirm let the next run prove accepted deletion. The recorder immediately after child close hit the same retained-layer handoff (run SHA-256 3B10604D77F247FDBF70B63BFD0BF4FBE4C8A595F10749BE7ED9EC9A016D8DC7). The final source requests repaint when the exact native owner changes, and the fixture drives the real enabled recorder response after parent admission. Independent review confirmed the snapshot and replay corrections.

For verification, `$f` is the trimmed contents of `r02-m2-b4-filter.txt`, `r02-m2-b4-filter-2.txt`, or `r02-m2-b4-filter-3.txt` in the ignored evidence directory. `cargo nextest list --locked --offline -p multi_launcher --lib --cargo-profile fast-dev -E $f` listed 11, five and one respectively. `cargo nextest run --locked --offline -p multi_launcher --lib --cargo-profile fast-dev --no-fail-fast -E $f` passed the final detached case 1/1 with 7458 skipped (run SHA-256 D51BC8B15F3A3C6490F54C2AA87F89538317A8165B26CF708B63FAAC0C64FD1D). The test clicks both host buttons, hides ROOT, rejects a discarded delete click, commits a fresh delete, records and persists a real pointer stroke, and Docks the same editor. `cargo check --locked --offline -p multi_launcher --lib --bin multi_launcher --profile fast-dev` exited 0 on final source (SHA-256 3198F1E21D46EF7F5B78A723516C3332DAD2668C45188A70F969601BBE562F7E). Scoped formatting and `git diff --check` passed.

Source scope is `src/gui/{command_host,mod,mouse_form_host,mouse_gestures_dialog,render}.rs`, `src/gui/jump_mode/ui.rs`, and `src/mouse_gestures/engine.rs`. Native N3 remains `IMPLEMENTED_USER_VALIDATION_PENDING`.

M2-B is complete within R02. Next is R02/M2-C.1 File Search on the same branch.

### R02/M2-C.1 — File Search

File Search now hosts the same search coordinator and preview receiver in docked, embedded fallback, and native Immediate presentations. The host snapshots editable form and selection values without cloning workers. Search, cancel, result actions, export, picker actions, preference publication, and Preview actions require the current retained ROOT and owner passes. Deferred row actions revalidate the search and row identity. File Lock and Folder Size receive the accepted ROOT provenance when a result action crosses viewports. A view preference save updates the view and persisted settings without rebuilding the executor or cancelling the current search.

The staging selection passed 4/4 (SHA-256 CFB467933A4CBCF5CFC5682FA4D476949DC45F6F1BB01978004C882ABDB6B118). The initial integrated list matched eight and the run passed 7/8. Its held-worker fixture disconnected before any host interaction because preference publication had reapplied full search settings (SHA-256 5875C4EE02A8F5FF1BE34C59E9659F8C4336A99D1A050C98A16985E0D1B1C4AF). Review also identified ownership leaks in row selection, sort, and title-bar close, stale focus, and Preview completion. The correction fences those paths, raises ROOT after an accepted Dock or ROOT tool handoff, and publishes preferences without resetting the worker. The first correction list exited 101 at a missing deserialization type annotation, before any test ran (SHA-256 F9B901121E34C11E215CE5F1FE3FCFA04EA7F4BDF68F3F187ED0D33E77B3AC1A). The corrected four-case run passed three. Its new round-trip fixture retained the same search ID and accepted a detached filter edit, then failed because the held executor had never sent the Started event counted by diagnostics (SHA-256 65A358310FC29F89DB2D379BCCB1B0C40002E260FD7E8C072C945A5FF50CC726). The final fixture sends that event and checks that no second worker starts.

For the exact final selector, set `$f` to the trimmed contents of `r02-m2-c1-filter-4.txt` in the ignored evidence directory. `cargo nextest list --locked --offline -p multi_launcher --lib --cargo-profile fast-dev -E $f` matched one and `cargo nextest run --locked --offline -p multi_launcher --lib --cargo-profile fast-dev --no-fail-fast -E $f` passed 1/1 with 7460 skipped (SHA-256 7FE6299E4F91B5B8726FDC278766328B349B63C96C3209393153666131641A99). The test uses enabled Pop Out and Dock buttons, a held search executor, and real detached filter input while ROOT is hidden. `cargo check --locked --offline -p multi_launcher --lib --bin multi_launcher --profile fast-dev` exited 0 (SHA-256 35B34ADB90574288AC1396DFB871E32949AE9DE8FD1266D7440ADAFCD5E31B37). Scoped formatting and `git diff --check` passed.

Source scope is `src/gui/{file_search_dialog,file_search_window_host,file_search_preview_dialog,command_host,mod,render,file_lock_dialog,folder_size_dialog}.rs`, `src/gui/file_search_dialog/keyboard.rs`, and `src/gui/jump_mode/ui.rs`. Native N4 remains `IMPLEMENTED_USER_VALIDATION_PENDING`. Next is R02/M2-C.2 Diff on the same branch.

### R02/M2-C.2 — Diff

Diff moves one workspace between docked, embedded fallback, and native Immediate presentations. The active text comparison, folder operation, and watcher handles stay with that workspace. The retained ROOT and owner passes authorize picker, save, export, rule, navigation, and operation intents. Each child window checks its input owner. Worker polling precedes the UI checkpoint so a discarded frame restores editor and navigation values while preserving autonomous results. Text source and undo storage are shared across the checkpoint with copy-on-write edits. Native Close routes dirty comparisons through Save, Discard, or Cancel.

The first list attempt found a test-only attribute and a mismatched action return. Later compile attempts found stale Rules fixture arguments, an embedded renderer argument, and the pane setting reference moved with Binary polling. Those attempts ran no tests. The first runtime subset passed 11/12, and the expanded run passed 13/14 (SHA-256 8E0780A874CED45BA9B342A5AF4D5A711AEAAE3706E1B0D8F0EF13EB266F2076). The failing round-trip fixture installed its held operation before binding scan roots, so the initial poll cancelled it. The corrected fixture binds both roots, clicks the enabled Pop Out and Dock controls, and proves that the same uncancelled operation receives its terminal event while the launcher is hidden. Independent review cleared the final owner and rollback corrections.

Set `$f` to the trimmed contents of `r02-m2-c2-filter-8.txt` in the ignored recovery evidence directory. `cargo nextest list --locked --offline -p multi_launcher --lib --cargo-profile fast-dev -E $f` matched three (SHA-256 35207F2842D1413CEFF5AD4C4B68C0EAF52D85673E270F358841DF272239DDC6). `cargo nextest run --locked --offline -p multi_launcher --lib --cargo-profile fast-dev --no-fail-fast -E $f` passed 3/3 with 7460 skipped (SHA-256 4A71EDB994737D23C9FBFEEEFC3E8AF3AACDEA52DF5E98F21EB93BE94608FEE5). `cargo check --locked --offline -p multi_launcher --lib --bin multi_launcher --profile fast-dev` exited 0 (SHA-256 1B94C94C99B4850C86AD9997FB0BF35D76A30423DD020DCE8177B1246B754972). Scoped Rustfmt and `git diff --check` passed.

Source scope is `src/gui/diff_dialog/`, `src/diff/{model,text_file}.rs`, and `src/gui/{command_host,mod,render}.rs`. Native N4 remains `IMPLEMENTED_USER_VALIDATION_PENDING`. Next is R02/M2-C.3 Batch Rename and Search & Replace on the same branch.

### R02/M2-C.3a — Batch Rename

The Batch Renamer retains its scan ticket, verified snapshot, selected entries, rule draft, preview, frozen confirmation and coordinator across docked, embedded fallback, and native Immediate presentations. A retained owner and ROOT pass authorize scan, Apply, Undo, cancellation, picker, and close commands. Interaction rollback shares the verified preview and row caches and leaves the live ticket with the dialog. The native picker uses the verified active HWND. The launcher continues polling the coordinator while hidden. The exact confirmation layer owns input and Jump Mode in its viewport, and keyboard close respects panel pinning.

The first list exposed a viewport bootstrap array length mismatch (SHA-256 1B23CC882315A0C048F2EE1C97EC674E0AFB96800082D1D01212FE58BF0EF2C6) and ran no tests. After correction, nine tests passed. The final list matched ten and all ten passed with 7457 skipped. The real-click round trip rejects a discarded Pop Out, detaches while the launcher is hidden, and docks the same verified preview. The detached confirmation test opens Apply, rejects a Confirm release on a discarded ROOT pass, checks the same pending confirmation and untouched files, then cancels from its native owner. The existing guarded transaction test still applies and undoes through the launcher. Independent review cleared the final close and confirmation ownership corrections.

Set `$f` to the trimmed contents of `r02-m2-c3a-filter-3.txt` in the ignored evidence directory. `cargo nextest list --locked --offline -p multi_launcher --lib --cargo-profile fast-dev -E $f` matched ten (SHA-256 392586BA9F9C92967A395CADD53857AAB5C29418B7A42358ED60207DFAD0ECEC). `cargo nextest run --locked --offline -p multi_launcher --lib --cargo-profile fast-dev --no-fail-fast -E $f` passed 10/10 (SHA-256 E2F36FAD7AE3EDBA198D125F878A634204ED2DB2FB932FAE00AEDF48DC92AEC1). `cargo check --locked --offline -p multi_launcher --lib --bin multi_launcher --profile fast-dev` exited 0 (SHA-256 93DFC688CCA190832F60EAA9D56C7BA8764C3E2BD47E75AE433B35D201F63953). Scoped Rustfmt and `git diff --check` passed.

Source scope is `src/gui/{batch_rename_dialog,batch_rename_window_host,mod,render}.rs` and `src/gui/jump_mode/ui.rs`. Native N4 remains `IMPLEMENTED_USER_VALIDATION_PENDING`. Next is R02/M2-C.3b Search & Replace on the same branch.
