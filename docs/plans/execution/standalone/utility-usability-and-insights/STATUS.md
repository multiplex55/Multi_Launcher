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
| M1-B | B1 Dashboard slot markers | pending |
| M2-A | FS1 cancellation-safe Clear results | pending |
| M2-B | FS2 literal leaf-name matching | pending |
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
