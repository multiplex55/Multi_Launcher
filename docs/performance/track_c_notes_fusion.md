# Track C C1-B note relationship fusion

Status: IN_PROGRESS. Base `0a7f0efde29efc78b5c0a9bd5d2bea3187d88ee6`; C1-A source `b47c1646d99c653b53e96bafa6b242259eabef17`. Closed checkpoints remain5of29. LOCAL ONLY.

## Bounded plan and ownership

One milestone replaces the three tab-specific relationship traversals in `src/gui/note_panel.rs` with one private prepared result containing three ordered vectors. One source writer owns that file plus minimal owned-CWD guards for four selected cases in `tests/notes_plugin.rs`. Parent owns this document, ledger, runners, collectors, evidence and Git. Independent reviewer is read-only. No Cargo until source is coherent and reviewed; never concurrent source writers or Cargo processes.

Compute needles and draft fenced projection once. Traverse todos once followed by notes once; transform each non-self note once. A matching todo produces the same row in LinkedTodos and Mentions, preserving source order and duplicate source entries; clone its prepared row for the second vector. Preserve canonical wiki→typed reference→text precedence and exact reasons, category classification and snippets. Mentions contains matching todos before matching notes. Label-map preparation and final installation remain in the existing refresh owner. Remove the obsolete production tab helper and migrate its direct behavioral tests; leave the independent frozen eager oracle untouched.

Preserve disabled-setting behavior, full row/map/revision application,250ms draft debounce,1s matching-input retry, changed persisted-input bypass, last-good failure state, unsaved draft, save/close behavior and canonical raw fenced-wiki precedence. Do not change helper parsing/snippet behavior, persistence, note cache, todo ownership, aliases, dependencies, profiles or native application behavior. No index or UTF-8-window fix in this milestone.

## Actual work and protocol

Replace test observer `passes[3]` with a named fused traversal. Count actual N note visits and T todo visits, including skipped self entries; accepted fixtures have N fence transforms, generally1+non-self-note count. Needle construction occurs once; selected String observations continue to exclude row clones/allocator traffic/final retained memory.

Keep the fixed phase registry. `NoteMentionsScan` now measures the shared fused classification traversal, including work for all categories, and is no longer isolated mentions-only timing. Both this phase and enclosing relationship-refresh phase report N+T actual loop entries. Update current Rust owner validators deliberately; never manufacture old3x work.

Current supplemental benchmark/emitter migrates to `C1B_NOTE_RESULT schema=1 stage=fused`, with `fused_calls`, `fused_todo_visits`, `fused_note_visits`, `fused_ns` replacing three pass arrays. Existing fixture/signature/output domains and actual UI timing boundaries remain matched. Prior C1-A source is reproducible at its source commit; immutable artifacts and the schema-1 three-pass collector remain unchanged. Parent draft separate C1-B collector is ignored until actual-data validation/review. Compare new fused interval to quantiles of old per-sample pass sums, never sums of quantiles.

## Meaningful verification budget

Add a compact differential full-seven-field/three-vector test for0/1 notes and semantic fixtures: ambiguous aliases and primary/secondary transitions through canonical publication, broken/competing references, fenced raw wiki vs fenced prose, nested markdown, duplicate refs/source ordering, valid Unicode, empty todo IDs, persisted todo references and unsaved outgoing references. Reuse existing100/1k/5k actual owner scenarios without a size-by-semantics crossproduct. Preserve original owner full state and work receipts, changing only actual work expectations.

Selected existing units: original oracle, actual open-panel caller, direct grouping/fence helper tests, settings transition, idle non-cloning, persisted note/todo debounce bypass, persisted todo text/reference, unsaved external aliases, retry-input changes, failed snapshot last-good and new draft after failure. Add owned-CWD guard before the selected settings-transition notes guard to keep WORKLOAD→NOTES lock order. Add one focused inert save-on-close unit covering persistence and resulting derived output using the existing explicit inert app helper; do not call legacy generic app constructors.

Safe integration filter in `notes_plugin`: `note_link_dedupes_backlinks`, `note_alias_supports_open_rm_and_labels`, `extract_tags_skips_code_fences`, `note_meta_wrap_links_integration_parses_aliases_and_preserves_links_regression`. Add minimal file-local owned-CWD guard after the existing test mutex and before setup for only these cases; plugins/watchers drop before workspace. These cases start an owned-directory NotePlugin filesystem watcher/coordinator and execute no returned native launch/remove actions.

`note_panel_auto_save` sole legacy integration test constructs production plugin discovery and cannot use the library's cfg(test) inert path. Record that target NOT RUN under isolation constraints; prove affected save-on-close behavior through the new inert unit rather than adding a production testing feature. App-dependent integration callers and legacy generic-app unit equivalents are excluded for the same reason.

Parent runs exact fast-dev unit filters and the four-case integration filter, then Small actual-UI smoke, strict collector positive/negative checks and independent review. Commit accepted source before Full measurements; on that exact SHA run5actualUI scenarios and original owner serially,5warmups/20retained samples/0exclusions. Preserve G0/C1-A evidence and contrary results. No full repository suite, release build or native campaign.

## Acceptance gate

Full eager parity, correct lifecycle/ownership and removal of stale production helper are mandatory. Demonstrate one real traversal and reduced normalization work with repeatable benefit in matched samples. If common cases worsen without convincing net value, revert fusion and document the measured gate decision. Source, numerical/privacy review and final cumulative diff must clear before local checkpoint closure.

## Source review progress

Production/observer subset independently CLEAR: one prepared traversal retains TODO-first source order, duplicate source entries, exact TODO overlap, wiki→typed→sorted textual precedence, self exclusion and unchanged snippet/fence/updated helpers. Label map, final keys, disabled setting, snapshot failure, retry and debounce remain at the original boundary. Actual named fused work/timer scope is honest; historical Track A owners unchanged. Completed tests/emitter/isolation review is in progress; execution not yet authorized. Ordinary new names are `prepared_backlink_rows_match_frozen_eager_categories_and_order` and `track_c_c1b_note_save_on_close_persists_and_refreshes_backlinks`. Existing actual-UI ordinary name retained; ignored owner renamed `track_c_c1b_benchmark_open_note_panel_refresh_attribution`. Sole writer paused, rustfmt check PASS for touched source/tests; no Cargo/tests/benchmarks yet.

Source packet independently CLEAR after adding explicit duplicate-ID and repeated-source TODO rows to the compact differential fixture. Full seven-field eager comparison and explicit ordering/overlap/duplicate assertions remain; no production defect found. Save-on-close actual owner, inert construction, guard/drop/lock order, emitter and new collector source reviews CLEAR. Parent exact15-unit fast-dev run active in session42744, raw ignored `target/performance/track-c/c1-b/nextest-ordinary.log`; no concurrent Cargo/source writes. Integration and Small/Full performance evidence still NOT RUN.

## Targeted unit execution

First C1-B unit run genuinely PASS15/15,5154skipped. Nextest run `2baa5009-db87-465d-8574-30a1555e6aef`, fast-dev build1m28s, execution1.417s, PERF1/Small/UNCOMMITTED_SOURCE. This includes compact differential duplicate/reference/category parity, real inert save-on-close, actual UI three transitions and existing lifecycle/failure/idle tests. Raw ignored `c1-b/nextest-ordinary.log`. No failed test or compiler attempt in this packet. Four safe notes-plugin integration cases now building/running serially in parent session13717, raw `c1-b/nextest-notes-plugin.log`; source writers paused. Legacy auto-save integration remains deliberately NOT RUN, covered affected behavior by passing inert unit. Benchmark evidence still pending.

## Safe integration and Small smoke

Safe `notes_plugin` filter genuinely PASS4/4,19skipped. Run `173df3d0-e329-4bd0-b475-7c5b8b9a66cc`, fast-dev build1m11s, execution0.176s; raw ignored `c1-b/nextest-notes-plugin.log`. Audited filesystem watcher/coordinator remained within owned note paths. Sole legacy auto-save integration remains NOT RUN; inert close-save unit passed.

Small fused attribution PASS1/1,5168skipped, run `08b0e537-c732-494b-9116-544342b5b6d1`, build0.88s, execution15.346s. Honest UNCOMMITTED_SOURCE/fast-dev/Small/PERF1/observer enabled,5warmups/20samples/0exclusions for each of three100-note transitions. Raw ignored `nextest-small-smoke.log`; new strict collector accepted3records60samples. Five copied negative cases (wrong stage, frame quantile, missing array, work counter, snapshot containment) all rejected with original raw unchanged. Independent numerical review pending.

|100-note transition | C1-A Small frame p50 / p95 / max (ms) | C1-B Small frame p50 / p95 / max (ms) |
| --- | --- | --- |
| Draft debounce |24.8581 /26.1724 /26.2946 |11.0648 /13.4482 /16.8119 |
| External note |25.1530 /26.9383 /28.3046 |11.2665 /15.7581 /18.3039 |
| Persisted todo |25.5499 /31.2364 /36.9360 |10.8646 /11.8700 /13.4802 |

Each refresh reports one fused traversal,100note visits/12todo visits, separate12label-map visits,100fence transforms and one successful snapshot; previous relationship work336/fences300 becomes112/100. Full oracle parity/predeadline/idle controls remain active for every sample. Selected construction lengths/capacities continue to omit allocator traffic, row clones and final retained memory. This smoke is not accepted committed-source Full evidence or an application-wide improvement claim.

Independent Small numerical review CLEAR: all126 raw arrays match three collected records exactly, twenty samples each; scalar protocol, every timing quantile, actual fused/snapshot/fence/string receipts, nested containment and privacy passed. Fixture signatures and revision arrays match C1-A exactly; local/todo full and structural output arrays match exactly. External retains two structural hashes from timestamp text, with full eager parity each sample, so no forced cross-run equality. Needle construction count30→10; fenced construction lengths are exactly one-third. Do not reinterpret these as allocator/RAM totals. Reviewed honest uncommitted smoke saved in `track_c_c1b_small.json`, separate collector `track_c_collect_fused_note_attribution.ps1`. Source commit can proceed with C1-B IN_PROGRESS; Full comparison/acceptance remains pending.
