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
| M3-A | H2 owned presentation preference / generic host | pending |
| M3-B | H1/H2 custom hosts / accepted child origins | pending |
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
