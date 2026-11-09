# Utility Usability and Visual Insights execution

Contract: `docs/plans/CODEX_GOAL_UTILITY_USABILITY_AND_INSIGHTS.md` and its companion Kickstart. One checkout, one source writer and one Cargo/native lane. Parent owns Git, acceptance and reporting.

## M0-A bootstrap

- Preparation: clean master `10210e0b2f45cf7481e5637257f05fd69cadf2d3`, matching origin/master.
- Verified parent_branch: `feat/studio-and-utility-polish`.
- parent_tip_sha: `9a7fb137126d316e351e5ee8e6a04ec20fb7630d`; local and origin refs agree. Its STATUS records terminal LIMITED delivery. No later category handoff exists in the inspected refs/records.
- Read-only planner audit confirmed master differs only by 235 byte-identical historical documentation archival renames and this Goal/Kickstart pair. Source, Cargo, AGENTS, .codex and dev compare equal. Master and its archival commit remain preserved; this task retains the predecessor's historical documentation locations, as the preceding task did. Only the authorized new task packet is carried into this baseline.
- Task branch: `feat/utility-usability-and-insights`, created directly at parent_tip_sha; initial HEAD equality verified. Initial sandbox index-lock denial was resolved through approved escalation.
- No pre-existing dirty/untracked work; no worktrees, helper clones, auxiliary/checkpoint branches or intermediate integration operations.
- baseline_sha: `e5dcd694c93cc34f425317b9a476218bf1fb2ea3`; `git rev-parse HEAD^` verified exact parent_tip_sha. Initial ordinary push succeeded; upstream is `origin/feat/utility-usability-and-insights`.
- Staged whitespace inspection identified only three original Markdown hard-break lines in the carried Goal; the authorized packet was preserved verbatim. No application changes at M0-A.

## Checkpoint plan

Goal section F supplies the authoritative objective, ownership, scope, invariants, tests and done criteria. Execute M1 through M9 in order; shared presentation precedes companion hosting, projections precede UI. Use configured planner/implementer/reviewer roles, sequential implementation packets and one cumulative independent review. Compile separately from targeted fast-dev Nextest execution; each execution batch uses the owned-process runner with a 100-second deadline. No full-suite campaign.

| Checkpoint | Outcomes / deliverable | State |
| --- | --- | --- |
| M0-A | Exact-parent baseline / push complete; source inventory ongoing | complete Git gate |
| M1-A | L1 compact descriptions / bounded cards | complete; automated verified, native pending |
| M1-B | B1 Dashboard slot markers | complete; automated verified, native pending |
| M2-A | FS1 cancellation-safe Clear results | complete; automated verified, native pending |
| M2-B | FS2 literal leaf-name matching | complete; automated verified, native pending |
| M3-A | H2 owned presentation preference / generic host | complete foundation; automated verified, custom coverage pending M3-B |
| M3-B | H1/H2 custom hosts / accepted child origins | in_progress B1 existing custom hosts; B2 pending |
| M4-A | D1/D2 Mermaid Colors / menu strips | pending |
| M4-B | D4 toolbar / pinned inspector | pending |
| M4-C | D3 owned-text endpoint captions | pending |
| M4-D | D5 actual connector creation / reconnection | pending |
| M5-A | I1 bounded label-file exchange | pending |
| M6-A | R1/R2 explicit duration / encoder timeline | pending |
| M6-B | R1/R2 exact typing / 25-second steps | pending |
| M7-A | F1 retained-tree treemap model | pending |
| M7-B | F1/F2 linked navigation / visibility | pending |
| M8-A | C1 cached stable dependency snapshot | pending |
| M8-B | C1 companion explorer / navigation | pending |
| M9-A | Cumulative review, scoped lint/native, handoff | pending |

No implementation builds, tests or native interactions have run at baseline preparation. The complete 17-outcome acceptance matrix and actual receipts will be maintained here as work proceeds.

## M1 planning and inventory

Planner validated `RootResultTextPresentation`, `root_list_result_text`, `root_grid_result_text`, `RootListGeometryCache::ensure_grid` and `root_grid_cell_response` as L1's shared owner. Existing minimum-width, Extend and galley-expansion paths must migrate together with the old spill-oriented geometry tests. Allowed provider changes are mouse, hotfolder and search_replace with legitimate detailed help preserved. Implementer received M1-A only, including exact acceptance/invariants and focused rendered geometry/provider tests. Parent has read the raw ArgumentList owned runner interface; separate compilation and 100-second execution batches required.

Initial ordinary-host source inventory (integration pending M3): generic Batch Rename, Search & Replace, Clipboard History/Modify, Paste Sequences, Snippets, JSON and Programmer Inspector; custom Calc Pad/reference, notes list/editor, File Lock, Diff, Duplicate Finder, XML, settings, mouse forms, Date & Time Studio, ZIP, Markdown Studio, Dashboard Editor, Image Comparison, Recording, Regex, Folder Size, File Search, Timer, Diagram Studio, Calendar, Table Utility, MkMacro and Explorer Tabs. Actual accepted caller/child edges and exclusions will be established in M3, not inferred from the generic wrapper. Calc Pad currently emits bare OpenInspector; notes use existing open routes. No host is yet claimed covered by new policy.

## M1-A acceptance

Shared whitespace/Unicode-safe seven-word projection leaves underlying Action data intact. Grid title/description are independently pixel-fitted; cache, row allocation, clipping, hit rectangles and keyboard reveal use bounded geometry. Registered-folder list full paths and full tooltips remain; snippet privacy retained. Mouse summaries are command-specific; Hot Folder and Search & Replace search summaries are compact while existing detailed plugin/commands help remains.

Separate compilation: `cargo nextest run -p multi_launcher --cargo-profile fast-dev --lib --no-run` succeeded on final source (last compile 1m15s; existing vendor/test warnings only). Execution used `dev/run_owned_process.ps1` raw ArgumentList, TimeoutSeconds 100 and Nextest `--no-fail-fast` for each focused batch:

- `m1-a-focused`: 5/7 initially passed, including new real-provider/synthetic narrow-width/enlarged-font hit-rect assertions, list geometry, explicit text color roles, mouse summaries and Search & Replace detailed help. Two obsolete geometry expectations failed: eager Grid centered tall cells in a 44-point allocation; row-eviction assertion lay inside existing overscan. Tests migrated to full-row allocation and a viewport beyond overscan, without weakening actual hit/clip assertions.
- `m1-a-geometry-retest`: 2/2 passed after first focused remediation, run `5d769cb6-fc3f-4c22-b78e-c3b3d1dfd7fa`, runner 2.463s.
- `m1-a-navigation`: 3/3 passed for keyboard activation, Jump Mode full action and 10k-result last-row click/virtualization, run `26d872ff-d390-47f0-83fe-0a1041434b7f`, runner 5.232s.

Exact selectors, output and result/cleanup receipts: `target/utility-usability-and-insights/evidence/m1-a-{focused,geometry-retest,navigation}/`. All cleanup/output drains verified. Parent inspected resulting diff and whitespace; only the four intended source files and this ledger changed. Native checks are deferred to M9, not claimed passed. Implementer explicitly returned source/Cargo lane before commit.

Planner correction to inventory: Explorer Tabs is generic (`UtilityWindowHost<ExplorerTabsDialog>` alias/trait implementation live in its own module). M3 will also account for ordinary File Search preview/hash and notes templates/assets children, retaining native picker/modal/overlay exclusions.

M1-A checkpoint `5363fa3b428f5a587ad0db29036336156f05d0da` pushed successfully; clean tree before M1-B handoff. Planner's M1-B packet assigns Dashboard preview/list token identity and presentation only, preserving placement/resize/occupancy semantics. Implementer now owns M1-B source and Cargo lane; parent retains ledger/Git.

## M1-B acceptance

Dashboard preview/list share existing unique session-token S-markers, clipped slot text, full label/widget/position/span tooltips and linked hover outlines. Exact selected index avoids highlighting multiple duplicate IDs. Tiny/large-font cells retain markers with the list as full-name legend; overlapping markers stack. Selection/conflict use text and borders as well as color. No saved layout schema or live-widget rendering changes.

Separate final lib compilation with the same fast-dev Nextest no-run command passed in 1m34s. An introduced test-only unused-mut warning was fixed; six existing test warnings remain. Owned 100-second `--no-fail-fast` executions:

- `m1-b-focused`, run `a7a6b1b0-aeaf-447c-89d0-de807e11e598`, 2/4 passed: clipped duplicate/tiny marker paint and token swap/removal identity. Drag fixture exposed an existing press-origin defect (expected two-cell span, got one because drag threshold position became anchor); fixed narrowly in preview's input owner. Linked-hover assertions passed before draft equality exposed initial weather widget-default normalization; fixture now snapshots settled state and retains direct-preview no-mutation assertion.
- `m1-b-remediation`, run `a9b75c22-2e92-4723-9156-0d4d9e9af014`, 2/2 passed after first focused remediation; runner 1.976s.

Exact selectors/output/cleanup receipts are under `target/utility-usability-and-insights/evidence/m1-b-{focused,remediation}/`; cleanup/drain verified. Parent inspected complete source diff and whitespace. Only Dashboard dialog and this ledger changed. Source/Cargo lane explicitly returned; native observation pending M9.

M1-B checkpoint `2c8135e71fbac81a7c239c526245f2cf655d5b32` pushed successfully; clean tree before M2-A handoff. Planner identified the active GUI search identity as FS1's first responsible boundary; Clear must retire it through accepted host actions, unlike Cancel which preserves partial results. Implementer received M2-A only; parent retains ledger/Git.

## M2-A acceptance

Explicit Clear results beside Search/Cancel has a stable Jump target. Its accepted captured search identity/result revision takes priority over same-pass result row/copy/export effects. Matching coordinator cancellation/retirement is immediate and nonblocking; no synchronous unbounded event-queue drain. Late events are stale; worker-owned Activity reaches terminal independently. Rows/refinement/selection/preview/diagnostics/request/backend/sort-pending state clear to Ready without changing form/preferences. Preview effects complete after parent actions and reject the cleared context. Ordinary Cancel still preserves partial results.

Separate fast-dev lib Nextest no-run compile passed in 2m00s (six existing test warnings). Owned execution `m2-a-focused`, 100-second deadline and `--no-fail-fast`, passed 9/9; run `38ceef29-9c15-4a5f-8e15-b2cd6f59e2c8`, runner 2.713s. Cases cover delayed event kinds, worker cancellation/activity, rapid restart, form preservation, stale Clear/row/copy/export and retained/discarded/detached Clear UI. Exact selectors/output/cleanup receipt: `target/utility-usability-and-insights/evidence/m2-a-focused/result.json`; cleanup/drain verified. Post-test source changes only whitespace. Parent inspected scoped diff/whitespace; source/Cargo lane returned. Native observation pending M9.

Unrelated pre-existing caveat, left unchanged: unused public `SearchCoordinator::drain_events_including_stale` also applies stale events to coordinator status. There are no current callers; ordinary GUI drain rejects them. This is not an FS1 active-path defect and did not justify another rebuild or broader coordinator cleanup.

M2-A checkpoint `a92e9915f292948286967f9f4a016923dfdeeb9d` pushed successfully; clean tree before M2-B handoff. FS2 planner packet assigns the distinct literal leaf-name matcher before WalkDir result-budget admission, retains existing modes/defaults, and deliberately uses existing qualified WalkDir routing rather than an unqualified Everything/UI filter. Implementer owns M2-B source/Cargo only; parent ledger/Git.

## M2-B acceptance

Additional `FilenameContains` / `filename_contains` mode uses shared exact/prefix/substring filename ranking without path fallback or fuzzy semantics. Case-fold expansions now highlight complete original UTF-8 characters. Existing Ranked/Fuzzy defaults/serialization retained. Qualified routing uses WalkDir in settings-aware and no-settings selectors; Everything remains Ranked-only. WalkDir's pre-existing omission of Type admission was corrected before result-budget accounting, with traversal/exclusions unchanged. README documents the literal mode and routing.

`SearchRequest` has no serialization contract in this codebase; preference serde roundtrip plus actual UI selection to built request/backend is the verified boundary. Focused fixtures include path-only/fuzzy negatives, gcc/libgcc positives, punctuation/extensions/case/Unicode and capped file/directory/exclusion scans.

Separate final fast-dev lib Nextest no-run compile passed in 1m30s (six existing warnings); first compile's fixture-only CentralPanel Ui/Context mismatch was corrected. Owned 100-second `--no-fail-fast` batches:

- `m2-b-focused`, run `2e22458f-f559-4566-adff-0451b3b54bdf`, 10/11 passed; closed Filters UI fixture could not find the new option.
- `m2-b-remediation`, run `a88cd2b7-a664-4e87-9f29-544e15de65f5`: strengthened same-global-roots backend assertion passed, forced egui header ID fixture still failed.
- `m2-b-ui-retest`, run `135aad4b-670b-45c5-9729-4fbf5396ca8a`: second focused fixture correction clicks actual rendered Filters header and option; passed 1/1, runner 1.874s. No assertion weakened and production UI unchanged by fixture corrections.

Exact selectors/output/cleanup receipts: `target/utility-usability-and-insights/evidence/m2-b-{focused,remediation,ui-retest}/`; cleanup/drain verified. Parent inspected scoped source/docs diff and whitespace; source/Cargo lane returned. Native observation pending M9.

M2-B checkpoint `0eabaaca45107c13a09bf83abdc3a276f47c475a` pushed successfully; clean tree before M3-A handoff. Planner's verified M3-A packet assigns a separate settings presentation map, independently owned writes, shared context capability and generic host integration (including Explorer Tabs). Internal retirement, cancelled/discarded closes and failed bootstrap cannot record fallback Docked. Custom hosts and accepted child-origin routing remain M3-B; no global H1/H2 claim until inventory integration completes. Implementer owns M3-A source/Cargo only; parent ledger/Git.

M3-B will use two sequential coherent subcheckpoints under the existing milestone: M3-B1 migrates existing custom ordinary hosts to shared remembered preferences; M3-B2 carries accepted child origins and adds the narrowly required ordinary companion adapters. Both remain on this branch, with no new feature scope or parallel writer. H1/H2 global acceptance stays pending until both and their inventory checks pass.

## M3-A acceptance

New settings-level versioned bounded presentation map retains unsupported JSON opaquely and fails closed for writes. Independently owned per-key worker revisions coalesce writes and reject superseded acknowledgements; general Settings apply/ack/rollback does not own this field. Settings editor's explicit full-value construction copies the current field for compatibility. App-owned context manager preserves accepted in-session choices on write failure and reports once; registry rebind clears obsolete waits and profile changes reload saved preferences instead of leaking another profile's local choices.

All nine generic hosts restore once per real open session before root visibility/modal decisions and their own hidden-root guard. Open/focus preparation also avoids opening-frame root focus grabs. Accepted explicit changes and successful closes remember the effective presentation before retirement; retirement itself remains unchanged. No startup auto-open. Custom families/child origins remain unimplemented until M3-B.

Separate fast-dev lib no-run compilation passed. Initial integration compile omissions (Settings editor construction and app rebind ownership) were corrected at their owners. Owned 100-second `--no-fail-fast` executions:

- `m3-a-settings`: 5/5 passed, run `212c1e80-59c2-40a1-a2d5-30f8327eb144`, runner 1.883s: schema/opaque data, failed-save shutdown report, stale general apply/ack/rollback, local failed choice/report-once and newer-write/old-ack/shutdown.
- `m3-a-host`: existing host-generation regression passed; combined Inspector fixture aborted with Windows stack overflow before assertions because two large LauncherApp values occupied one frame.
- `m3-a-host-retest`: after separate non-inlined app-lifetime helpers and a local fixture key binding correction, Inspector integration passed 1/1, run `180ce89c-8676-4ec4-9de9-6338dee676f4`, runner 3.452s. Actual retained/discarded pointer transitions, Popout restart restore, Docked close/reopen and absence of phantom root layer verified.

Exact selectors/output/cleanup receipts: `target/utility-usability-and-insights/evidence/m3-a-{settings,host,host-retest}/`; cleanup/drain verified. Parent inspected final model/manager/writer/generic source changes and whitespace. Final changes only format newly added blocks; no redundant tests after whitespace-only finishing. Source/Cargo lanes returned. Native observation pending M9.

M3-A checkpoint `fa1a0f340b9363a67da4f79a86d949fa50eeb862` pushed successfully; clean tree before M3-B1. Implementer owns existing-custom-host H2 integration only, with shared policy helpers, pre-modal preparation, accepted close/transition writes and preserved absent defaults/dirty-close decisions. Typed child origins and new ordinary companion adapters remain B2. Parent owns ledger/Git and will record per-family actual coverage at acceptance.

## M3-B1a partial acceptance: Calculation Pad/reference

First coherent custom-family checkpoint adds shared preference helpers and Pad/reference restoration before focus/root ownership. Accepted transitions and successful final closes persist the effective choice; cancelled dirty close retains draft/presentation. Absent preference remains Docked. Other custom families and all new child-origin routing remain pending, so global H1/H2 are not complete.

Separate final fast-dev lib no-run compile passed. Owned 100-second `--no-fail-fast` executions:

- `m3-b1-pad`: 5/5 passed, run `660db660-1142-4f3b-86cf-df198800b770`, runner 1.920s. New preference/restart/cancel test plus retained reference, explicit host request, dirty write-conflict and native-host state regressions.
- Existing detached-query matrix initially aborted `0xc00000fd` during app fixture construction. Two diagnostic runs localized overflow after context/workspace creation and entering `root_list_app_at`, before app/UI frames. Test-only non-inlined boxed fixture removes multi-layer by-value stack pressure; all original panels/Jump/query assertions retained and diagnostics removed.
- `m3-b1-focus-fixture-retest`: full original query/focus matrix passed 1/1, run `3aa7e5d4-4986-4fb6-891b-69f52f2e3ecc`, runner 9.670s. No stack-size override or production constructor change.

Exact output/selectors/cleanup receipts: `target/utility-usability-and-insights/evidence/m3-b1-{pad,focus,focus-diagnostic,focus-diagnostic-2,focus-fixture-retest}/`; cleanup/drain verified. Parent inspected scoped diff/whitespace; no temporary diagnostics remain. Source/Cargo returned before partial-family commit; native observation pending M9.

| Host family inventory | Current policy coverage |
| --- | --- |
| Nine generic hosts including Explorer Tabs / Inspector | M3-A implemented, representative integration verified |
| Calculation Pad and reference | M3-B1a implemented / automated verified |
| Notes list and note editors | M3-B1b implemented / automated verified |
| File Search / Settings / Mouse Settings / Gesture Settings / Mouse Gestures | M3-B1c1 implemented / automated verified |
| Calendar / event details / event editor / DateTime Studio / Timer | M3-B1c2 implemented / automated verified |
| Other existing custom families enumerated above | pending remaining B1c groups |
| Accepted child-origin routes / ordinary companion adapters | pending B2 |
| Native pickers, modal confirmations, menus, capture/radial/recording overlays | retain existing ownership; excluded from ordinary detached-tool policy |

M3-B1a checkpoint `b343e7b0c6b48f4c74245349834ee55f90cc6e96` pushed successfully; clean tree before Notes M3-B1b. Implementer owns notes list/editor restoration, accepted final-close ordering and dirty-close fences only. Remaining custom families form B1c; child origins and ordinary companion adapters remain B2. Source/Cargo single lane; parent ledger/Git.

## M3-B1b partial acceptance: Notes list/editors

Notes list and fresh editors restore shared stable-tool preferences before focus/modal decisions; live editor focus never reapplies another instance's preference. Accepted transitions, app-level closes and successful editor Save/Discard close paths remember effective presentation; Cancel keeps the dirty draft. Two editors share one note-editor preference, with last accepted successful close winning. New editor stack insertion and the separate remove-note/reopen-list route prepare early; no per-note preference records or child-origin inference.

Separate final fast-dev lib no-run compile passed (six existing test warnings). Owned 100-second `--no-fail-fast` batches:

- `m3-b1b-editor`: 3/3 passed, run `27375df6-583a-49f1-9576-01cd36143365`, runner 2.799s: two-editor preference/Cancel/close ordering, rendered editor rejected-pass/draft preservation and same-slug reuse.
- `m3-b1b-notes`: 2/2 passed, run `051208a9-4629-4f24-9f14-d0b9714d7216`, runner 2.675s: rendered Notes host rejected-pass/draft preservation and close/restart preference restoration.

Exact selectors/output/cleanup receipts: `target/utility-usability-and-insights/evidence/m3-b1b-{editor,notes}/`; cleanup/drain verified. Existing rendered editor fixture now uses a disposable settings path because accepted presentation can write. Test-created untracked `clipboard_modifiers.json` was confirmed absent before execution and removed; no user file discarded. Parent inspected source diff/whitespace; lanes returned. Native pending M9; other custom hosts and B2 remain pending.

M3-B1b checkpoint `422e60489ae6719c97f6dcf08216b696126cbb85` pushed successfully; clean tree before M3-B1c. Implementer owns remaining existing custom hosts (enumerated in the source inventory), batched by lifecycle mechanism with representative tests. Stable defaults, pre-modal restoration, accepted-final-close fences and hidden-root Docked access are explicit invariants; unhosted ordinary child adapters and origin precedence remain B2. Parent ledger/Git.

Bounded B2 planner inventory is complete. Execution will use B2a accepted origins for already hosted edges; B2b ordinary companions outside MkMacro; B2c MkMacro's nested ordinary editors/help/pickers. Natural family commits are permitted under these substantial boundaries. Reuse `UtilityWindowHost`/`HostedUtilityDialog` (or a narrow extraction of its existing receipt/chrome mechanics), not automatic detachment in `show_utility` size wrappers. Move child draft ownership deliberately, retain one content renderer and parent-owned typed transactions; child/root receipts and parent session/document/draft freshness must qualify effects, never current global focus. Inventory includes actual transitive edges, with no invented ImagePanel→Crop or NotePanel→Graph edge. Actual covered/excluded rows will be recorded at acceptance.

## M3-B1c1 partial acceptance: Search/settings/mouse hosts

File Search, Settings and the three Mouse forms restore once per open before visibility/focus decisions and remember accepted changes/successful final closes. Settings/Mouse final decisions remain with existing owners. App/command direct-open routes prepare before deciding whether to show root. Forced final-close paths capture preference before retirement; helper retirement does not persist.

Separate fast-dev lib no-run compile passed. Owned 100-second `--no-fail-fast` batches:

- `m3-b1c-search`: 3/3 passed, run `1a2f682c-4982-4fd7-8167-b806f061d202`, runner 2.706s: five stable-kind preferences/once-per-open policy, live hidden-root search session and retained/discarded/popped Clear regression.
- `m3-b1c-settings-mouse`: 4/4 passed, run `2ecf2789-7ec2-46ff-8a34-86aa60e71fa7`, runner 2.722s: live Settings/Mouse/Gesture forms and Jump Mode Cancel-draft preservation.

Exact selectors/output/cleanup receipts: `target/utility-usability-and-insights/evidence/m3-b1c-{search,settings-mouse}/`; cleanup/drain verified. Parent inspected owner/command/open/close diff and whitespace; lanes returned. This is five existing hosts only; remaining custom families and all B2 work remain pending. Native pending M9.

M3-B1c1 checkpoint `0dcb66034f8fbff259c303dd80345854146d8893` pushed successfully; clean tree before time-host M3-B1c2. Implementer owns Calendar/details/editor, DateTime Studio and Timer preference lifecycle, retaining current accepted child routing until B2a. Remaining document and analysis/output families stay pending. Parent ledger/Git; single source/Cargo lane.

## B2 planned child inventory (not implementation evidence)

The planner traced these actual ordinary edges. Every row remains pending until accepted integration/tests; Dependency Explorer is added later at M8-B using the same policy.

| Parent / owner | Actual ordinary child targets | Planned checkpoint |
| --- | --- | --- |
| Calc Pad | Inspector; reference | B2a existing hosts |
| Notes list / wiki / note links | New or existing note editor | B2a existing host |
| File Search | Folder Size; File Lock | B2a existing hosts |
| Calendar / DateTime Studio | Event details/editor | B2a existing hosts |
| Note editor / Markdown Studio / Diagram | Diagram Studio; Diagram Viewer | B2a existing hosts |
| Settings accepted navigation/actions | Dashboard; Mouse/Gesture Settings; Clipboard Modify | B2a existing hosts |
| File Search | File Preview; Hash Generator | B2b adapters |
| Notes list | Template manager; Unused Assets | B2b adapters |
| Note editor | Image viewer; custom Markdown table; Insert Link / Link new note forms | B2b adapters |
| Diff | Comparison Rules; Folder Rules; memory New/Edit Inputs | B2b adapters |
| Settings | Theme Settings; Plugin Editor; Hotkey Directory | B2b adapters |
| Mouse Gestures / Gesture Settings | Binding editor; app-owned window-candidate picker | B2b adapters |
| MkMacro | Action Catalog; Action Editor; Recording Review; Find/Replace Steps | B2c adapters |
| MkMacro package UI | Help; Templates browser; Save Template; Export Library choices | B2c adapters |
| MkMacro Action Editor / hotkey / review | Launcher Action Picker; window-candidate editor; reference Crop; nested Action Editor | B2c adapters with transitive origin |
| MkMacro recording toolbar | Recording annotation text editor | B2c ordinary text editor; preserve pause/resume transaction |

Evidence-backed exclusions: Package Import Preview is an immutable summary with Apply/Cancel (confirmation); macro invocation/runtime parameter prompts are broker-owned fixed native execution prompts; signature editor, image-asset browser, ZIP Preview and Regex Explanation are inline. Capture polling and screen/radial/recording control/outline overlays, timer-finished notification, native Open/Save/Folder pickers and actual overwrite/delete/dirty confirmations retain their existing ownership. The Recording annotation editor is a normal editable parent-owned Window without capture geometry/topmost overlay behavior, so it is included. Crop's loading and ready content must use one contextual tool kind rather than competing reference-crop/image-crop preferences. Image viewer has no Crop child; templates Open uses external viewer, Unused Assets has no ordinary child, and Diff navigation opens workspace views rather than new windows. NoteGraph parent/caller classification is receiving a final targeted caller check; its actual Note-editor opens use remembered preference while it remains root-only.

## M3-B1c2 partial acceptance: Time hosts

Stable keys calendar, calendar-event-details, calendar-event-editor, date-time-studio and timer now restore per real open and remember accepted changes/successful owner-completed closes. Calendar's existing popped-parent request stays authoritative; Docked/root opening resolves saved preference. Root visibility/focus uses actual presentation rather than an embedded fallback layer. Timer/Alarm share the existing authoring key; finished notification and timer runtime semantics remain unchanged. Full child-origin promotion is still B2a.

Separate final fast-dev lib no-run compile passed; final post-test edit was indentation only. Owned 100-second `--no-fail-fast` batches:

- `m3-b1c2-date`: 3/3 passed, run `6517273e-c2e5-4aca-973c-770bfcc0d5b3`, runner 3.183s: five-key/live-session policy, retained DateTime→Calendar preparation and DateTime draft/reference/popup host moves.
- `m3-b1c2-calendar-timer`: 3/3 passed, run `445763ae-6127-4b4a-bab3-a7097664079f`, runner 3.244s: retained Calendar save/delete occurrence, Timer submission failure/success and Timer/Alarm authoring host moves.

Exact selectors/output/cleanup receipts: `target/utility-usability-and-insights/evidence/m3-b1c2-{date,calendar-timer}/`; cleanup/drain verified. Parent inspected source/open/close/command diff and whitespace; lanes returned. Other document and analysis/output custom hosts and B2 remain pending; native pending M9.
