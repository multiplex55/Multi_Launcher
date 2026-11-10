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

Goal section F supplies the authoritative objective, ownership, scope, invariants, tests and done criteria. Follow the bounded order refinement recorded below: shared presentation precedes companion hosting, projections precede UI, remaining global adapters finish before M9. Use configured planner/implementer/reviewer roles, sequential implementation packets and one cumulative independent review. Compile separately from targeted fast-dev Nextest execution; each execution batch uses the owned-process runner with a 100-second deadline. No full-suite campaign.

| Checkpoint | Outcomes / deliverable | State |
| --- | --- | --- |
| M0-A | Exact-parent baseline / push complete; source inventory ongoing | complete Git gate |
| M1-A | L1 compact descriptions / bounded cards | complete; automated verified, native pending |
| M1-B | B1 Dashboard slot markers | complete; automated verified, native pending |
| M2-A | FS1 cancellation-safe Clear results | complete; automated verified, native pending |
| M2-B | FS2 literal leaf-name matching | complete; automated verified, native pending |
| M3-A | H2 owned presentation preference / generic host | complete foundation; automated verified, custom coverage pending M3-B |
| M3-B | H1/H2 custom hosts / accepted child origins | existing hosts and B2a already-hosted edges verified; ordinary B2b/c adapters pending |
| M4-A | D1/D2 Mermaid Colors / menu strips | complete; automated verified, native pending |
| M4-B | D4 toolbar / pinned inspector | complete; automated verified, native pending |
| M4-C | D3 owned-text endpoint captions | complete; automated verified, native pending |
| M4-D | D5 actual connector creation / reconnection | complete; automated verified, native pending |
| M5-A | I1 bounded label-file exchange | complete; automated verified, native pending |
| M6-A | R1/R2 explicit duration / encoder timeline | complete foundation; UI pending M6-B |
| M6-B | R1/R2 exact typing / 25-second steps | complete; automated verified, native pending |
| M7-A | F1 retained-tree treemap model | complete foundation; linked UI pending M7-B |
| M7-B | F1/F2 linked navigation / visibility | complete; automated verified, native pending |
| M8-A | C1 cached stable dependency snapshot | complete foundation; companion pending M8-B |
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
| Diagram Studio / Viewer / Regex | M3-B1c3a implemented / automated verified |
| Markdown / Diff / MkMacro main | M3-B1c3b implemented / automated verified |
| Dashboard Editor | Missing B1 hooks found/fixed in B2a2; lifecycle/origin automated verified |
| File Lock / Duplicate Finder / Folder Size / Image Compare / Recording / Table / XML / ZIP | M3-B1c4 implemented; policy verified, three existing-test limits below |
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
| Note editor todo reference / Todo View | Editable Todo View; Todo View's Open note and Open at anchor → note editor | B2b adapter + transitive origin |
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

M3-B1c2 checkpoint `98c08e7831e9c8a0aa01c3cfd431558170cccc94` pushed successfully; clean tree before document-host M3-B1c3. Implementer owns existing Diagram owner/viewer, Markdown, Diff, Regex and MkMacro main preference integration; nested children remain B2. Remaining analysis/output hosts follow. Parent ledger/Git; single source/Cargo lane.

Final NoteGraph caller clarification: production opens via `note graph` launcher command (`commands/handlers/note_link.rs` → command host) or root panel focus/pin; radial command handoff is an excluded overlay origin. No Quick Notes/NotePanel/Markdown/other ordinary-tool Graph open was found. Dashboard graph widget opens notes, not the Graph dialog. Retain root-only/no existing Dock exclusion for Graph parent; Graph→Note editor resolves the editor's remembered preference. Existing command-route test: `note_graph_dialog_action_opens_dialog`. This supersedes the pending classification note above; no adapter is invented.

## M3-B1c3a partial acceptance: Default-Popout document hosts

Diagram Studio, Diagram Viewer and Regex restore saved presentation once per real open while retaining Popout when no preference exists. Accepted transitions and successful final owner/viewer closes remember effective presentation; Diagram dirty decision cancellation retains its choice and draft. Root open/focus paths prepare early and show root only for actual Docked presentation. Existing document, viewer and pass-generation fences retained. Markdown/Diff/MkMacro main form the separate remaining document group.

Separate fast-dev lib no-run compile passed. Owned 100-second `--no-fail-fast` batches:

- `m3-b1c3-diagram`: 4/4 passed, run `c1470625-20fb-43ba-89e1-895132232adc`, runner 2.475s: independent owner/viewer preferences, dirty native-close decision, discarded Dock and viewer close/reopen generation scope.
- `m3-b1c3-regex-focus`: 3/3 passed, run `9ee911d4-483a-4a83-b6bd-fcaea7640cd2`, runner 9.586s: Regex saved-Docked/live-Popout policy, retained revision/copy host moves and full existing detached-query coexistence matrix after the Diagram change.

Exact selectors/output/cleanup receipts: `target/utility-usability-and-insights/evidence/m3-b1c3-{diagram,regex-focus}/`; cleanup/drain verified. Parent inspected source/open/close diff and whitespace; lanes returned. Other document/analysis hosts and B2 remain pending; native pending M9.

M3-B1c3a checkpoint `7d3d1bffc3219906cf9e925eb275645753dce025` pushed successfully; clean tree before Markdown/Diff/MkMacro main M3-B1c3b. Implementer owns those existing parent hosts only; nested editors/help/pickers and typed child origins remain B2. Analysis/output families follow; parent ledger/Git, one source/Cargo lane.

## M3-B1c3b partial acceptance: Docked-default document hosts

Markdown Studio, Diff and MkMacro main use shared saved presentation at real open, preserving Docked default and live session state. Accepted transitions/final successful closes record before retirement; dirty decisions stay with document/workspace/macro owners. Direct universal-action MkMacro and activity-driven Diff routes now prepare through the normal open/focus boundary. Nested ordinary children and typed origin work remain B2, not claimed verified here.

Separate fast-dev lib no-run compile passed after one activity-opener Context borrow correction. Owned 100-second `--no-fail-fast` batches:

- `m3-b1c3b-markdown`: 3/3 passed, run `216bba8a-b2af-4525-bf3f-53e2d0a01868`, runner 2.776s: three-key/live-session policy, same-document host moves and cancelled-close no-repeat.
- `m3-b1c3b-diff-mkmacro`: 5/5 passed, run `b774979e-aa64-425d-9f3e-0fe62b82f11e`, runner 2.778s: Diff dirty-close/undo preservation, busy hidden-root workspace, MkMacro retained action edit and same-macro Run/host move.

Exact selectors/output/cleanup receipts: `target/utility-usability-and-insights/evidence/m3-b1c3b-{markdown,diff-mkmacro}/`; cleanup/drain verified. Parent inspected source/caller/close diff and whitespace; lanes returned. Eight analysis/output existing hosts and B2 remain pending; native pending M9.

M3-B1c3b checkpoint `18cab48d9559a23db72daae8b08c513be7677b27` pushed successfully; clean tree before final eight existing analysis/output hosts M3-B1c4. Implementer owns preference lifecycle only, explicitly preserving scans/workers/recording/output and dirty-decision behavior; recording duration/treemap features remain later milestones. B2 typed origins/new ordinary adapters still pending. Parent ledger/Git; one source/Cargo lane.

## M3-B1c4 partial acceptance: Final eight existing hosts

All eight existing families use per-open preference preparation and accepted transition/final-close writes: file-lock, duplicate-finder, folder-size, image-comparison, screen-recording, table-utility, xml-toolkit and zip-extractor. Existing workers, scan/recording/output and dirty-decision behavior retained. XML's actual absent-preference Popout default preserved. Image Compare uses the registered image-comparison key, not its navigation label.

Separate final fast-dev lib no-run compile passed. Owned 100-second `--no-fail-fast` batches and exact individual attribution checks:

- `m3-b1c4-policy`: 3/3 passed, run `5f0e6a20-79c4-45e1-a152-936036d3f564`, runner 2.773s: eight keys/live-session policy, discarded File Lock actions and Folder Size explicit-scan-only.
- `m3-b1c4-workers`: File Lock held inspection and retained Recording controller passed; Folder Size selection and Recorder preview fixture failed and reproduced individually.
- `m3-b1c4-analysis-hosts`: Table Utility held file/selection and Image Compare in-flight pair/camera passed; XML retained close and ZIP inspection failed.
- XML old fixture expected Popout after explicit Dock/reopen. This conflicts with intentional H2; it now asserts remembered Docked, explicitly pops out, then retains all original discarded/retained native-close assertions. A smaller Recorder accepted-host/close/reopen test covers H2 independently of the preview fixture.
- `m3-b1c4-corrected`: both new/migrated cases passed 2/2, run `666f5214-b817-4abd-b5b4-504c445d47fa`, runner 2.724s.

**Existing-test limits, not passes:** `folder_size_host_preserves_scan_and_drill_through_hidden_root` loses only selected_entry Some((1,1)) to None after Dock while the same scan/drilled directory remain; `recorder_setup_hosts_preserve_real_controls_popup_and_stale_region` cannot find enabled fully clipped preview-label; `zip_host_preview_confirm_survives_moves_and_rejects_discarded_actions` times out waiting for inspection (about 11s, not runner deadline). Each fails individually and in available pre-task binary `target/fast-dev/deps/multi_launcher-057b516185de8672.exe`, last-write UTC 2026-11-08T21:54:39, SHA256 `A37114B09F4743A1C4A838B9709FD59D32FA50571AE123992762EC92CF63847E`. This is pre-task evidence, **not an exact-parent source attribution**. Assertions remain intact; no engine/fixture campaign added. Folder Size selection is directly relevant to upcoming M7 and must be considered there. Recorder/ZIP limits remain separately reported unless later changed behavior makes them relevant.

Exact selectors/output/attribution receipts: `target/utility-usability-and-insights/evidence/m3-b1c4-*` (policy, workers, analysis-hosts, individual retests, old-binary/old-recording/old-xml/old-zip, corrected). All receipts show no runner deadline timeout and verified cleanup/output drain. Parent inspected final scoped diff and whitespace; lanes returned. All existing UtilityHostState families are now integrated; global H1/H2 remain pending B2 ordinary child origins/adapters. Native pending M9.

M3-B1c4 checkpoint `ff490a9094840daf13b5edb56328e4b7e1ee9bb2` pushed successfully; clean tree before B2a1. Implementer owns shared typed-open precedence/authority and safe generic promotion plus Pad→Inspector/reference and Notes→editors. Explicit accepted root opens use remembered preference; observing/rendering a live session never reapplies it, and a live Popout is never demoted. Actual parent host/session/document authority must survive/revalidate delayed dispatch; no focused-viewport inference. Other already-hosted edges and all ordinary adapters remain later B2 slices. Parent ledger/Git; one source/Cargo lane.

B2a current-source finding: Pad dispatch occurs while Inspector may still own a retained pending frame. Promoting by dropping that frame could lose accepted child edits. B2a0 therefore first establishes a typed accepted parent receipt and common precedence model; generic fresh opens consume it. B2a1 then coordinates normal target completion before actual promotion and migrates Pad/Notes edges. Acceptance proof is captured once; delayed dispatch validates current host/session/document, not an indefinitely stale exact-pass counter. This foundational checkpoint is not H1 edge completion.

## M3-B2a0 foundation acceptance

`AcceptedParentOpen<S>` has private fields and can be constructed only after retained root/owner passes. Its current-host/session check is separate from the one-time retention proof, allowing valid delayed dispatch while rejecting changed authority. `ChildOpenOrigin<S>` and `resolve_child_presentation` encode popped-parent override, remembered/default fallback, live Docked promotion and live Popout non-demotion. Generic fresh-host preparation consumes the resolver. Actual live promotion and Pad/Notes edges remain pending, so H1 is not complete.

Separate fast-dev lib no-run compile passed. Owned `m3-b2a1-policy` (receipt name predates the foundation split), 100-second `--no-fail-fast`, passed 3/3: policy/receipt, host-generation and real generic Inspector regression. Run `eab71aaa-8a57-44bb-9dda-1c947ef07eed`, runner 3.423s. Exact selectors/output/cleanup receipt: `target/utility-usability-and-insights/evidence/m3-b2a1-policy/result.json`; cleanup/drain verified, no timeout. Parent inspected final two-source-file diff and whitespace; lanes returned. No actual child-open path is claimed migrated by this foundation.

M3-B2a0 checkpoint `96f7d27b38a596d5e67aeee947a2fe0a2b93504f` pushed successfully; clean tree before actual B2a1 Pad/Notes migration. Implementer owns safe generic promotion after normal pending-target completion and real accepted Pad/Notes origin routing. Parent-session/host/modal/source guards must be revalidated at dispatch; root visibility follows resolved child presentation. No additional adapters/edges until later B2 slices. Parent ledger/Git; one source/Cargo lane.

## M3-B2a1a acceptance: Pad → Inspector/reference

Pad emits `PadOpenReceipt` binding retained source host to document incarnation/ID. Dispatch revalidates current open/host/document and refuses pending close, chooser, Save As or dirty transition. Inspector opens/promotes only after its normal pending frame completes; reference promotion similarly follows retained reference completion. Existing session/private bits survive. Root visibility uses the final resolved host. Generic deferred opens are explicit and re-resolve after pending actions, preserving an accepted Popout/edit rather than applying an earlier Docked result. Root restore/focus is read-only; inherited Popout records accepted intent, while internal retirement remains separate.

Separate final fast-dev lib no-run compile passed. Focused runs exposed/fixed a premature default-preference write and deferred visibility/choice ordering. New fixtures were corrected for actual created viewport ownership, private draft setup after source publication, relative bit value and fresh source focus/clip observation; substantive value/epoch/input/hidden-root assertions retained. Required missing fresh/stale-owner cases were completed before acceptance.

Final owned 100-second `--no-fail-fast` evidence:

- `m3-b2a1-pad-owned-reference`: rendered existing-child promotion/reference inheritance 1/1, run `2ec901df-b6a6-4061-8fe1-9968b2356903`, runner 2.710s. Private value/epoch retained, promoted native control enabled, Reference popped out, root hidden.
- `m3-b2a1-pad-final-regressions`: 6/6, run `c7cbe5ab-9d1e-4468-b7bd-8ce1242490ea`, runner 3.573s: pending Popout+bit edit, accepted host/ShowRoot behavior, retained-origin invalidation, existing reference independence, real Pad selection/insert and generic discarded-preference guard.
- `m3-b2a1-pad-finish`: 3/3, run `d6643321-7067-4fdf-bf46-ff5d54892e54`, runner 2.633s: fresh native Inspector over explicit saved Docked preference, actual parent close/document replacement/modal-state rejection and retained native Reference origin.

Receipts/output/selectors under `target/utility-usability-and-insights/evidence/m3-b2a1-pad-*`; final three receipts exit 0/no timeout/cleanup and drain verified. Parent inspected final source diff and whitespace; lanes returned. These are automated viewport tests, not physical native observation.

Goal test-anchor wording clarified from actual source: `calculation_reference_is_ordinary_root_context_when_pad_is_native` opens Reference **before** moving Pad native. Its Docked Reference expectation legitimately protects already-open child independence and remains intact; new rendered tests cover the requested open-from-already-popped-parent contract. No retroactive child movement is introduced. Notes and all other B2 edges/adapters remain pending; global H1/H2 are not complete.

M3-B2a1a checkpoint `91c87fb7b3662549529ef9e867012866709c9117` pushed successfully; clean tree before B2a1b Notes→note editors. Implementer owns qualified list/editor source receipts and fresh/reused note-editor promotion after normal target completion; no ordinary companion adapters yet. Preserve private dirty state/same-slug identity and use actual resolved host for root visibility. Parent ledger/Git; one source/Cargo lane.

Additional actual link inventory: `MarkdownLinkAction::TodoReference` → `open_todo_reference` opens editable `TodoViewDialog` (`todo_view_dialog.rs`), not main launcher navigation. It requires a B2b ordinary adapter with a distinct stable kind (existing `todos` size key belongs separate root-only TodoDialog). Its Open note / Open at anchor handlers also need actual accepted Todo View origin once hosted. Keep existing live edits when focusing/promoting rather than unconditionally reloading via open_edit. Separate TodoDialog has no Dock or ordinary parent edge found and remains scoped root-only; no TodoDialog→TodoView edge is invented. Current B2a1b only covers note-editor link targets, not this pending Todo View adapter.

## M3-B2a1b acceptance: Notes list/editor → note editors

Accepted list opens carry host/open-epoch authority; editor wiki/link opens carry host/prompt/slug/path authority. Dispatch follows normal target frame completion and revalidates live source/modal state. Fresh and live same-slug targets share the presentation resolver, preserve dirty contents/history/identity, and show root only for final Docked targets. Root/command opens that meet a staged target defer within NotePanel until normal completion; an accepted Popout/edit cannot be overwritten. Query activation retains the existing one-shot handoff. Closing/reopening via app ensure-open advances list lifetime, while closed editor entries awaiting cleanup cannot resurrect discarded drafts.

Separate fast-dev lib no-run compile passed. Final owned 100-second `--no-fail-fast` batch `m3-b2a1b-lifecycle-final` passed 6/6 in 2.726s: actual rendered native list→fresh editor→existing dirty wiki target, retained target edits/deferred Root/source Dock and discard/actual app close-reopen, two authority tests, same-slug reuse and query handoff. Earlier scoped 6/6 and deferred-Root 3/3 receipts remain under `target/utility-usability-and-insights/evidence/m3-b2a1b-*`. Final receipt exit 0, no timeout, cleanup/drain verified. Only touched-hunk formatting followed tests; diff/whitespace inspected, source/Cargo returned. These are automated viewport interactions; physical native observation remains M9. Global H1/H2 remain pending other already-hosted edges and ordinary adapters.

M3-B2a1b checkpoint `6095d87e1996f5034534daf9cb3712b55922e19f` pushed successfully; clean tree before consolidated M3-B2a2. Implementer owns all remaining already-hosted child edges in one checkpoint: FileSearch→FolderSize/FileLock; Calendar/DateTime children; Note/Markdown→Diagram→Viewer; Settings→existing hosted targets. Planner delivered current owner/receipt/pending-frame guards and narrow tests. New ordinary adapters remain B2b/c. Parent ledger/Git; one source/Cargo lane.

Planned order refinement after B2a2: M4 Diagram work next for current-owner locality, then M5–M8 independent outcomes, then finish the inventoried B2b/c adapters before M9. Shared M3 preference/origin foundation is already available for new companion integration. This changes order only: every global host/child row and all 17 outcomes remain required. Planner has prepared adapter ownership packets; no parallel source/Cargo work.

B2a2 current-source correction: DashboardEditor's custom host was omitted from the earlier preference integrations despite blanket inventory wording. Its Settings origin work exposed missing prepare/remember hooks. H2 Dashboard lifecycle is included in this checkpoint at the actual owner, with focused restore/close/origin coverage and M1 draft/token behavior preserved. Earlier “all existing families integrated” statements are superseded by this concrete finding; final inventory reconciliation remains required.

Planner's bounded reconciliation checked all existing UtilityHostState owner modules, custom Dock producers, shared-host paths and stable-key prepare/remember calls. No additional existing-family omissions found beyond Dashboard. This is inventory/key-wiring evidence only, not deep lifecycle correctness or test execution; pending ordinary B2b/c adapters were deliberately not counted as covered.

B2a2 implementation/verification still in progress: FileSearch receipts bind host instance/search ID/result revision and dispatch after normal FolderSize/FileLock completion; accepted same-path reuse always requests focus. Calendar/DateTime receipts bind host/session; live children retain drafts. Note/Markdown source-copy snapshots carry host authority validated before Diagram accepts ownership of its copied snapshot/dirty decision; no new post-decision protocol. Settings carries accepted origin through direct actions/navigation. Target-specific host visibility is resolved after queued promotions (FolderSize/FileLock/Dashboard/Mouse/Gesture/Clipboard), preserving independent root requests. Parent review caught Details Edit intentionally retiring its source; authority now validates before the accepted handoff, not retroactively afterward. Final compile/tests pending; initial focused batches passed FileSearch/Calendar 6/6 and Settings/Dashboard/query 5/5, Diagram 4/5 with an obsolete absent-preference assertion being migrated while preserving discarded-close/generation checks. No checkpoint acceptance yet.

## M3-B2a2 acceptance: Remaining already-hosted child edges

The preceding WIP findings are resolved. FileSearch→FolderSize/FileLock, Calendar/DateTime children, Note/Markdown→Diagram→Viewer and Settings→Dashboard/Mouse/Gesture/Clipboard use accepted source authority and common presentation precedence. Completion order preserves staged target actions and final target-specific visibility. Details same-occurrence focus preserves epoch; explicit different occurrence or changed occurrence metadata updates its read-only context. Editor drafts remain unchanged by promotion. Dashboard's missing H2 hooks now restore per actual open and remember accepted transitions/final close before retirement. New source opens use current source checks; a Diagram accepted copied snapshot and dirty decision remain target-owned.

Final separate `cargo nextest run -p multi_launcher --cargo-profile fast-dev --lib --no-run` passed with six pre-existing test warnings only. Targeted owned 100-second `--no-fail-fast` receipts under `target/utility-usability-and-insights/evidence/`:

- `m3-b2a2-file-calendar`: 6/6, 2.944s; retained/discarded actual FileSearch→FileLock menus, source search/host authority and target reuse, Calendar draft promotion, DateTime root prepare guards.
- `m3-b2a2-settings`: 5/5, 9.810s; Settings target selection, Dashboard close/reopen and real host controls, native-viewport Settings draft and detached-query matrix.
- `m3-b2a2-diagram`: 4/5, 2.405s; dirty copied-source decision, viewer override, Note/Markdown stale-source handoff passed. Old viewer test absent-preference assertion conflicted with accepted inherited Popout persistence; migrated only preference expectation, retained close/generation assertions.
- `m3-b2a2-corrections`: 5/5, 3.410s; staged target Dock→parent promotion keeps path/session and hidden root; FolderSize retained/discarded prefills without scans; real native-viewport Details Edit→editor; migrated viewer generation.
- `m3-b2a2-calendar-occurrences`: 2/2, same/different Details context and real Edit handoff.
- `m3-b2a2-final-owner`: 4/4 after final cleanup compile; FileSearch authority, staged visibility, Calendar draft/occurrence behavior and actual Details Edit flow.

All successful receipts exit 0/no timeout/cleanup and drain verified. Initial compile's two test accessor errors corrected; unrelated formatting churn removed before final verification. Parent inspected scoped diff/whitespace and status; task-created clipboard fixture artifact removed. Lanes returned. New FileSearch/Settings origin tests inject retained receipts; existing actual menu/native-viewport regressions provide separate integration evidence. No physical native observation is claimed. Global H1/H2 remain incomplete until B2b/c adapters; proceed with recorded M4 locality order.

M3-B2a2 checkpoint `4e378c8d07c49077fb52a3800831dd2fc65eeaab` pushed successfully; clean tree before M4-A. Implementer owns Diagram D1/D2 only: source-pane Colors placement, menu/direct-control strips, bounded narrow/native layout and directly affected accepted color/focus/history tests. M4-B/C/D follow separately; B2b/c adapters remain pending as recorded. Parent ledger/Git; one source/Cargo lane.

## M4-A acceptance: Diagram source Colors and menu strips

Mermaid Source and both Split branches render the existing Colors editor immediately after Suggestions in their shared source pane; source-hidden Visual retains one reachable Colors placement before its preview. The old post-workspace editor is removed. Expanded Colors has an independent 180-point bounded scroll area plus normal bottom spacing so Apply can scroll fully into view. Mermaid and Visual Document headers separate top-level menus from direct viewer/mode/zoom controls, using existing popup ownership/MenuFocusScope and one color draft/accepted transaction path. No color/source/history semantics changed.

Separate final fast-dev lib no-run compile passed. Owned 100-second `--no-fail-fast` evidence under `target/utility-usability-and-insights/evidence/`:

- `m4-a-layout-first`: 6/6, 2.321s; source/normal+narrow Split placement, short footer, distinct menu/direct strips/stable IDs, source position/status stability, Tab/undo and retained keyboard/Escape focus.
- `m4-a-controls-first`: 2/3, 2.798s; Mermaid color history/save and visual template decisions passed. Mindmap exposed scroll-input routing to the old parent area, then a real 0.2-point footer clipping at maximum scroll. Helper uses observed inner Colors scroll rectangle; content bottom inset fixes reachability without weakening full-visibility/transaction/stale assertions.
- `m4-a-controls-final`: 2/2, 3.752s; exact Mindmap regression and short footer passed after the concrete cause correction.
- `m4-a-font-layout`: 2/2, actual 1.35× text-style sizes at fixed logical widths; enlarged Source/Split/menu layouts pass original assertions. DPI scaling alone was rejected as insufficient text-layout evidence. Initial compile used unsupported Context style API; corrected to this egui version's `all_styles_mut` before successful final compile.

Successful receipts exit 0/no timeout/cleanup and output drain verified. Parent inspected scoped diff/whitespace/status; only Diagram source and ledger changed. Source/Cargo returned. These are rendered tests, not physical native observation; M9 remains pending. M4-B D4 layout follows; C/D and remaining approved features/adapters unchanged in scope.

M4-A checkpoint `901891321ad539f7aa75bdbbcd8ec62da61aeebe` pushed successfully; clean tree before M4-B. Implementer owns VisualDocument arrangement strip/right inspector only, preserving current property draft/accepted transactions/camera and view-only collapse/resize invariants. Narrow real-font/mapping/Apply coverage plus immediate editing regressions; C/D remain subsequent. Parent ledger/Git; one source/Cargo lane.

## M4-B acceptance: Visual arrangement and pinned inspector

VisualDocument's single existing Align/Distribute implementation now precedes the canvas. Its single existing property/action renderer lives in a right-hand independently scrolling inspector with reachable collapse/reopen and a bounded draggable splitter. Fixed body rectangles use the actual visible clip rather than egui's potentially expanded available width; narrow forms wrap/bound controls and permit horizontal scrolling for intrinsically wider content. Accepted view effects own collapse/ratio updates under existing host/input fences. Layout changes preserve source, scene, revision/dirty/history, valid property drafts and camera; existing mapping-change cancellation remains authoritative.

Separate final fast-dev lib no-run compile passed. Initial focused tests exposed real inspector overflow and an Undo helper timing defect. Investigation confirmed parent available width 642.6 versus visible clip 620 and vertical-only ScrollArea content expansion; actual-clip bounds/fixed regions/explicit scroll sizing resolve this. Observed Undo was disabled during its popup sizing pass and enabled after settling; the helper now clicks settled observed geometry. Accepted VisualHistory and the original revision assertions pass. No transaction expectations weakened; temporary diagnostics removed.

Final owned 100-second `--no-fail-fast` evidence under `target/utility-usability-and-insights/evidence/`:

- `m4-b-splitter`: 5/5, 2.235s; normal+narrow actual 1.35× fonts, arrangement/sidebar bounds, accepted collapse and splitter resize, full-visible enabled Apply after independent scroll, preserved selected-object draft through collapse/host change, current mapping after resize, align/distribute and property edit single-undo/cache invariants.
- `m4-b-canvas`: 2/2, 6.050s; existing inline-label commit/reedit/cancel/stale and accepted middle-click fit regressions.
- `m4-b-undo-observed`: exact diagnostic confirmation passed; initial/correction/observed receipts remain `m4-b-*`, including failures truthfully retained for cause evidence.

Final receipts exit 0/no timeout/cleanup and drain verified. Parent inspected scoped diff/whitespace/status; no duplicate lower arrangement implementation or diagnostics. Large unchanged canvas/property bodies retain baseline formatting to avoid about 1,800 lines of unrelated indentation churn; new boundary/tests formatted. Source/Cargo returned. Physical native observation remains M9. M4-C endpoint captions and M4-D actual connector gestures remain pending.

M4-B checkpoint `e20d7f7ff7a9f34162cab2c106c21a144dae9127` pushed successfully; clean tree before M4-C. Implementer owns owned-text/ID endpoint display and full tooltip only, preserving eligibility, ID/anchor storage and Apply behavior. M4-D actual creation/reconnection follows. Parent ledger/Git; one source/Cargo lane.

## M4-C acceptance: Owned-text endpoint choices

The existing shared object-caption owner uses each primitive's owned label, Text content or container/lane label, normalizes whitespace into a Unicode-safe bounded excerpt and appends stable IDs/type fallback. Current From/To and choice rows use the same caption; full original text remains in tooltips. Missing targets remain truthful. A streaming formatter avoids constructing the entire normalized label in hot object-list paths. Eligibility and typed object-ID/anchor storage remain unchanged; no proximity inference or reconnect occurs during a rename.

Separate final fast-dev lib no-run compile passed. Owned 100-second `--no-fail-fast` batches `m4-c-first` (3/3, new formatter/combo plus existing connector Apply regression) and `m4-c-streaming` (2/2, formatter+actual combo after bounded-allocation correction) passed. Fixtures cover duplicate IDs, labelled rectangle/ellipse, Text/container/lane/empty/missing/multiline Unicode/long labels, actual combo selection and committed rename without anchor/endpoint change. Receipts/selectors/output under `target/utility-usability-and-insights/evidence/m4-c-*`; exit 0/no timeout/cleanup and drain verified.

Parent inspected final scoped diff/whitespace/status; old truncation paths removed, only Diagram source/ledger changed. Source/Cargo returned; physical native pending M9. M4-D actual endpoint reassignment and interactive creation remains required; neither route-point editing nor existing attached-object movement counts as its proof.

M4-C checkpoint `1972bed93ac303cf0509fa833193f6bad98453fd` pushed successfully; clean tree before M4-D. Implementer owns bounded actual connector tool/From-To handles and local preview→accepted atomic transaction, pure hit/anchor support and directly affected tests. Exact endpoint.object_id changes, route/style preservation, cancellation/noop/single undo and host/document/mapping fences required. Parent ledger/Git; one source/Cargo lane.

M4-D source clarification: actual existing Add connector uses the first two eligible scene objects, despite Goal/planner wording describing a two-selected shortcut. The requested outcome governs: quick creation will use the actual two eligible selected objects; other selection states arm explicit source/destination picking. Relevant fixtures migrate intentionally while preserving transactional assertions. Target eligibility must preserve valid locked-object attachment without moving/unlocking targets.

## M4-D acceptance: Actual connector creation and endpoint gestures

Add connector uses two eligible selected objects, otherwise arms an accepted source→destination tool with a stable hint row and projected target/anchor feedback. Typed From/To attachment handles are distinct from manual-route controls; direct drag and connector double-click focus coexist with primitive inline editing. Preview changes only the chosen endpoint; accepted release performs one existing ReplaceObject transaction, preserving connector ID/label/style/direction/manual points and target geometry. Invalid/Escape/noop do not edit/history. Coincident self-loop projected endpoints get independently hittable labelled affordances around the unchanged attachment/anchors. Same-batch pointer handling uses qualified raw events, not delays or synthetic input.

HostIdentity now participates in gesture authority alongside document/revision/owner/viewport/start acceptance/mapping. Tool state and delayed endpoint-editor focus use the same current-owner boundaries; discarded or retired source and changed mappings/documents cannot authorize stale edits. A narrow shared domain attachment query reuses existing projected anchor math. Current VisualObject/Layer model has no lock mechanism: no lock feature or fake fixture was invented; visible painted-target eligibility and nonmutation of targets are covered.

Final separate fast-dev lib no-run compile passed. The initial compile's route-point conversion and unreachable arm were corrected. Final owned 100-second `--no-fail-fast` evidence:

- `m4-d-final-authority`: 6/6; actual canvas creation/Escape, From and To endpoint.object_id reassignment to different objects, exact connector data/manual-route preservation/undo-redo, batched input, no-op/invalid/Escape/discarded start, stale document/host/source, rotated/clip/painted-prefix targeting, actual connector double-click focus, and explicit coincident self-loop projection with both handle hits.
- `m4-d-regressions`: 5/5; existing connector controls, manual route controls, attached-object movement (distinct from new proof), inline-label behavior and child-discard transactions.
- `m4-d-root-discard`: 1/1 exact root-discard regression.

Receipts/selectors/output: `target/utility-usability-and-insights/evidence/m4-d-*`; final receipts exit 0/no timeout/cleanup and drain verified. Parent inspected scoped diff/whitespace/status; no diagnostics or duplicate implementation. Source/Cargo returned; only new-block formatting followed final successful tests. Physical native creation/reconnect remains pending M9; automated state/input tests are not native observations. M5–M8 and B2b/c adapters remain required.

M4-D checkpoint `3c4e687d19a2ca3e512332861d2334f3b49d8c88` pushed successfully; clean tree before M5-A. Implementer owns bounded labels-only JSON codec/atomic replacement and accepted Inspector file/confirmation flow, with native owner/session/draft guards and nonlabel/Pad invariants. Parent ledger/Git; one source/Cargo lane. M6–M8, global ordinary B2b/c adapters and M9 remain pending.

## M5-A acceptance: Bounded Inspector label exchange

Version 1 UTF-8 JSON exchanges only ordered `{label,start,end_exclusive}` definitions with an optional source-width hint. The regular-file read is bounded to 128 KiB plus growth detection; universal 0≤start<end≤128 validation limits 128 labels/128 UTF-8 bytes each, rejects duplicate exact ranges and malformed/unsupported input, and preserves overlapping distinct ranges/order/empty labels. Atomic domain replacement retains out-of-current-width definitions as invalid without changing numeric mode. Export uses captured labels and existing durable `save_atomic`.

Import/Export buttons live inside Bit labels. Typed file requests carry the rendered host and Inspector epoch; accepted same-frame draft edits complete first, then picker/confirmation guards bind resulting generation to the original owner. A host promoted/retired before dispatch, pending close, changed epoch/draft or rejected pass cannot open/apply stale file work. Existing nonempty changed labels (including replacement by empty) require explicit count confirmation; cancel/failure leaves model state. Numeric bits/mode, pending input, Pad source/binding/history/dirty and populated comparison state are preserved.

The initial compiled binary stalled in test discovery before any test ran. Owned `m5-a-codec` timed out at 100 seconds with cleanup/drain verified. Local dependency/PE inspection established the first Windows rfd MessageDialog reference imported `TaskDialogIndirect` under existing common-controls-v6, while resources have no activation manifest. The confirmation boundary now uses HWND-owned `MessageBoxW` with No default and explicit API error handling; global features/manifests unchanged. Pre/post imports and timeout evidence are retained in `m5-a-codec`; rebuilt discovery completed normally, TaskDialogIndirect absent/MessageBoxW present. This is a resolved feature startup defect, not a claimed failed codec test or physical native observation.

Final separate fast-dev lib no-run compile passed. Owned 100-second `--no-fail-fast` successful batches under `target/utility-usability-and-insights/evidence/`: `m5-a-codec-fixed` 4/4 (bounds/roundtrip/atomic domain plus existing exact Inspector behavior); `m5-a-inspector` 7/7 (accepted controls/file flow, stale host/session/draft, same-frame ordering, confirmation, narrow/native body/private draft regressions); pending-close guards 3/3; actual export replacement I/O failure/sentinel 1/1; final `m5-a-pad-invariants` 1/1 uses real Pad source/binding and nonempty comparison result. Exact selectors remain in each receipt. All final receipts exit 0/no timeout/cleanup and drain verified.

Parent inspected scoped diff/whitespace/status; only label domain/codec, Inspector/generic-host/app dispatch and ledger changed. New codec is intentional; no loader workarounds, diagnostics or task processes remain. Source/Cargo returned. Physical native file-picker/import/export observation pending M9; M6–M8 and remaining global ordinary adapters continue.

M5-A checkpoint `dfadbdb04fa858ae2727282cebd8746754b25052` pushed successfully; clean tree before M6-A. Implementer owns typed timed/Unlimited recording semantics through all model/session/idle/terminal paths and wide GIF/true MP4 timestamps, preserving bounded streaming/cancellation/output. Injected clocks/tiny encoder fixtures; visible exact duration controls remain M6-B. Parent ledger/Git; one source/Cargo lane.

## M6-A acceptance: Explicit recording limits and encoder timing

`RecordingDurationLimit` explicitly models Unlimited or validated Timed 1–1,000 seconds; default remains Timed 30. Shared reached/clamp semantics cover timeline admission, idle/no-frame observation, manual stop and terminal claims. Unlimited retains real elapsed time and no DurationLimit termination; existing target/error/empty-clip/cancel/resource safeguards remain. Immediate controller/Windows/GUI request callers migrate to the type. The visible 1–120 control is intentionally still M6-B work, so R1/R2 UI outcomes are not yet complete.

GIF uses checked u64 absolute centisecond positions and streams long holds in ≤u16 per-frame delay chunks from one quantized pending image, checking session/generation between chunks and before finalization. Counts and representational overflow are explicit; cadence/zero-tail/rounding and staged output lifecycle preserved. MP4 removes product-duration guards and validates signed 100-nanosecond timestamp/sample-end bounds, positive sample durations and checked counters before native submission. No sentinel Unlimited duration or whole-clip buffering.

Separate final fast-dev lib no-run compile passed without introduced warnings. Owned 100-second `--no-fail-fast` successful receipts under `target/utility-usability-and-insights/evidence/`:

- `m6-a-session`: 10/10; fake-clock idle Unlimited remains Recording beyond 121/655.35/1,001 seconds before explicit Stop, held-frame Stop/target loss, finite 17/1,000 boundaries, default/options/timeline, empty-source/clock/error and mailbox/stop invariants.
- `m6-a-encoders`: 8/8; decoded long GIF delay chunks sum exactly beyond 65,535cs/1,000s with cancellation/write/trailer/true overflow coverage, original cadence/zero-tail/error behavior, MP4 real signed timestamp and sample-end boundaries/backwards validation.
- `m6-a-terminal`: 1/1 slow-source terminal claim freezes time before cleanup.

Final receipts exit 0/no timeout/cleanup and drain verified; selectors/output recorded there. Parent inspected scoped diff/whitespace/stale-cap search: no product 120-second cap remains in recording domain/encoders; remaining 120-second fixtures are legitimate, visible control remains explicitly deferred. Test-only GIF wrapper is retained for its existing trailer-error caller. Source/Cargo returned. No real long recording or physical native smoke claimed; M6-B exact input/steps/frozen status follows.

M6-A checkpoint `1ef6715ff6712770843b261c9a5cdcf40e1f3dee` pushed successfully; clean tree before M6-B. Implementer owns exact integer 0–1,000 UI/25-second owned raw-wheel and step controls, invalid-input admission, frozen active request display and current README. Domain/encoder semantics remain M6-A; parent ledger/Git, one source/Cargo lane. M7/M8 and global B2b/c adapters follow before M9.

## M6-B acceptance: Exact duration controls and frozen status

Displayed duration text is the single selection value validated for a request: whole seconds 0–1,000, 0 = Unlimited, default 30. Buttons/arrows/owned raw-wheel events step by 25 from the current value with clamping; typed 17 remains 17. Invalid text cannot hide a prior valid number and is rejected both visually and at retained ChoosePath/request admission. Wheel handling uses this egui's actual event/smooth-scroll boundary, not DragValue speed or nonexistent raw-scroll fields; outside/blocked/discarded input cannot change duration and the parent does not also scroll. Active controller status derives timed/Unlimited and elapsed from the frozen session request, independently of later mutable setup selection. Countdown/Stop/capture guards and other defaults remain; README updated.

Separate final fast-dev lib no-run compile passed without introduced warnings. Owned 100-second `--no-fail-fast` receipts under `target/utility-usability-and-insights/evidence/`: `m6-b-first` 3/3 (actual typing/blur, button/arrow/raw wheel, 0/1,000/invalid, retained same-pass frozen request and status); `m6-b-scroll-parent` 1/1 (genuinely scrollable parent at nonzero offset unchanged by owned 17→42 wheel step); `m6-b-regressions` 3/3 (cancelled path/frozen 17, reciprocal capture guards, hidden retained Stop). All exit 0/no timeout/cleanup and drain verified.

Parent inspected scoped diff/whitespace/status; only Recording UI, current README and ledger changed. Source/Cargo returned. R1/R2 implementation and automated evidence complete; physical short timed GIF/MP4/Unlimited recording and playback remain M9. No real 1,000-second wait performed or required. M7/M8/global adapters continue.

M6-B checkpoint `a2bb87de4bd1f376fbdfd476b21d17779301eb5b` pushed successfully; clean tree before M7-A. Implementer owns deterministic pure immediate-child treemap measurement/layout and accepted-publication/geometry cache seam, with exact u128 accounting and no-scan proof. M7-B linked UI/visibility and the directly relevant known selection-after-Dock failure follow; A is not full F1/F2 completion. Parent ledger/Git; one source/Cargo lane.

## M7-A acceptance: Retained-child treemap projection

Pure local-coordinate area subdivision uses original row IDs, measured directory subtree totals/direct-file lengths, checked u128 displayed totals and deterministic byte/ID ordering. Immediate children only prevent ancestor/descendant double counting. Zero, unknown, omitted and provisional entries remain distinct metadata without invented area. Geometry rejects invalid/nonfinite dimensions and stays finite/bounded/non-overlapping; tiny positive measurements retain identity and exact totals even when floating-point area rounds away. No path/filesystem/scanner dependency.

The GUI-owned on-demand cache maps accepted projection rows into narrow inputs and keys by generation/publication/result, directory, filter and dimensions independently of table sort. No renderer/hidden-frame work is added in A. Transitional dormant-seam allowances are explicitly removed when B wires the UI.

Separate final fast-dev lib no-run compile passed after correcting one test-only function-name shadowing error. Owned 100-second `--no-fail-fast` receipts `m7-a-treemap` passed 5/5 in 1.888s (exact bounds/ties/counts, cache reuse and same-count publication/result/directory/filter/size invalidation with held-scan counter, existing projection/zero-total/cancel regressions); `m7-a-tiny` passed 1/1 (one u64::MAX child plus 128 one-byte children, exact u128 sum/all identities/finite bounds/non-overlap/honest zero and unknown counts). Evidence under `target/utility-usability-and-insights/evidence/`; exit 0/no timeout/cleanup and drain verified.

Parent inspected scoped diff/whitespace/status; only pure module/export, cache seam/tests and ledger changed. Source/Cargo returned. B still owns all visible chart/linked selection/toggles and real UI no-scan evidence. Planner isolated existing selection loss: Drill selects the entered directory, then passive child-cache rebuild clears it because it is not its own child; settling frames expose this before Dock. Fix that owner boundary in B, preserve original regression and Reveal guards; no OS/focus workaround.

M7-A checkpoint `0d74806fd63c23fb47219376653990bf21854143` pushed successfully; clean tree before M7-B. Implementer owns linked table/treemap renderer, accepted view/press/navigation state, current-directory selection fix and full path/publication fences; existing host/session/scan reused. Require actual UI no-extra-scan proof and original Dock regression, remove dormant allowances. Parent ledger/Git; one source/Cargo lane. M8 and global ordinary adapters remain after this checkpoint, then M9.

## M7-B acceptance: Linked retained-data table and treemap

One Folder Size dialog offers Table (initial), Treemap and Both, with adaptive split/stack layout and accepted view state preserved across Dock/Pop Out. Both views use the same row selection, directory drill/Up and guarded context actions. File double-click is inert. Tiles show name/size/displayed-total percentage, text/outline selection and lazy full-path/completeness tooltips. Exact u128 totals and tiny/zero/unknown/skipped/provisional counts remain explicit. Unavailable retained children explain the explicit recursive Refresh or new scan-root remedy; drawing/navigation never looks up filesystem contents or submits a scan.

Primary press authority binds publication, original row, directory/filter, host/epoch and geometry; discarded/stale releases reject, including same-generation replacement. Fresh same-batch primary clicks can replace an old stamp without trailing cancellation erasing them. Table and chart share selection/drill ownership. The existing current-directory selection loss is fixed at passive row-cache rebuilding, preserving original Dock regression assertions and separate Reveal guards. Cache and draw use one actual-clip-bounded size; transitional dormant allowances removed.

Separate final fast-dev lib no-run compile passed; six existing lib-test warnings remain. Owned 100-second `--no-fail-fast` evidence under `target/utility-usability-and-insights/evidence/m7-b-*`: `m7-b-final` passed original hidden-root scan/drill/Dock, sorted/filtered original-row Reveal and held-scan authority tests (3/4); the remaining enlarged-font test exposed real chart overflow. Actual visible-clip sizing fixed the owner; final `m7-b-clip` passed both new tests 2/2 in 1.777s. Final receipts have no timeout and verified cleanup/output drain. Earlier failures remain recorded: observed egui triple-click classification required separating intended double-click sequences in the test clock; the enlarged-font frame also needed existing texture-delta disposal. Navigation assertions were preserved; temporary diagnostics removed.

Actual pointer fixtures prove linked selection, two-level drill/Up, inert file double-click, view restoration/native-host moves, nonrecursive unavailable-child wording and full narrow/enlarged-font chart containment. Held-scan counter proves draw/toggle/filter/select and stale-publication work adds no scan; separate real-scan navigation/resize fixture preserves generation. This is automated input/state evidence, not physical native Explorer or cross-drive observation. Parent inspected final scoped diff/whitespace/status and receipts; only intended source/tests and ledger changed. Source/Cargo returned. F1/F2 automated completion does not finish global H1/H2 adapters, C1 or M9.

M7-B checkpoint `0ad010836c1eee4a94871d785eaa2ba5b728151c` pushed successfully; clean tree before M8-A. Implementer owns cached dependency snapshot/current stable-row resolver only, keyed by document/incarnation/revision with no second evaluation. Relevant session metadata already contains authoritative physical dependency indices and precise outcomes. M8-B companion UI/navigation follows its committed API; global ordinary adapters still remain before M9. Parent owns ledger/Git; one source/Cargo lane.

## M8-A acceptance: Cached stable dependency snapshots

ActiveDocument owns an on-demand immutable Arc snapshot keyed by document/incarnation/revision. It reads only a matching existing SessionEvaluation; missing/stale evaluation returns unavailable without invoking the evaluator. Authoritative physical dependency indices become deterministic stable-LineId direct/reverse edges; iterative visited traversal returns deduplicated physical-order upstream/downstream sets safely through self/cyclic references. Original source ranges/excerpts, assignment names, precise LineOutcome and document diagnostic remain available. No parser/value inference or Inspector integer-only projection is used.

`CalculationPadSession::dependency_snapshot` reuses the Arc until a different current revision is requested. `resolve_dependency_line` checks document/incarnation and current cached evaluation, resolving surviving LineIds to latest UTF-8 ranges after normal evaluation; deleted/reopened identities reject. Browsing/resolution preserves source/revision/line IDs/dirty/history/evaluation count. The new bounded module/export and session cache/resolver are the only source changes; no UI yet.

Separate `cargo nextest run -p multi_launcher --cargo-profile fast-dev --lib --no-run` passed in 2m57s. Owned 100-second `--no-fail-fast` batch `m8-a-model` passed 7/7, including three new graph/cache/identity fixtures and four directly affected cached-evaluation/stable-line/undo/error-order regressions. Unicode/braced names, physical blank/comment/rN rows, chain/fanout/disconnected/vector/error outcomes, synthetic duplicate/self/cycle/global diagnostic, Arc reuse/build count and insertion/deletion/reopen are covered. Exact selectors/output/result: `target/utility-usability-and-insights/evidence/m8-a-model/`; exit 0/no timeout/cleanup and drain verified. Parent inspected source/diff/whitespace/status and receipts. Source/Cargo returned; native/companion acceptance remains pending M8-B/M9.
