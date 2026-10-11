# Track C C1-A note relationship contract

Status: IN_PROGRESS. Parent-owned contract; source implementation and measurement review are pending. Initial C1-A source is `bf105215c918656a5cdd18e64eef677f10cd9186`. Frozen G0 remains `071243db068835617a054a74fb5734a6dd9f128d`; its numeric values and eager oracle are immutable. LOCAL ONLY.

## Ownership and refresh routes

`NotePanel::refresh_heavy_derived` owns applying relationship rows, todo labels and the captured persisted revisions. `maybe_refresh_heavy_derived` owns its frame-time cheap gate, edit debounce and retry scheduling. `plugins::note` owns canonical note-cache publication and paired note/revision snapshots; the todo persistence owner supplies the current or last-good todo collection.

| Route | Existing contract |
| --- | --- |
| Open/reopen | The shared panel constructor performs fast derivation and a forced heavy refresh using the configured backlink setting. Bringing an already-open panel forward reuses it. |
| Ordinary open-panel frame | `ui_with_note_close_observation` updates dirty fast data and invokes `maybe_refresh_heavy_derived`. An unchanged frame skips full snapshots and relationship work. |
| Persisted note revision | A changed `note_version()` bypasses draft debounce. The applied revision comes from the successful paired snapshot, not a separate later version read. |
| Persisted todo revision | A changed `todo_version()` bypasses debounce. The revision is captured before loading; a mutation during loading can trigger a safe follow-up refresh. |
| Backlink setting | Setting changes bypass debounce. Disabling records the applied keys and clears request/retry state while retaining existing vectors; reenabling recomputes. |
| Draft/programmatic edit | `mark_content_changed` invalidates markdown data, marks fast/heavy data dirty, records egui edit time and clears retry. Heavy work waits 250ms. Pending frames request repaint after the remaining delay. Unchanged programmatic replacement is a no-op. |
| External invalidation | Explicit external invalidation requests heavy work without assigning a new edit time. Programmatic external replacement uses the established time-zero path. |
| Alias edit/save | Alias synchronization marks fast/heavy data dirty and clears retry. Successful alias persistence publishes a revision consumed through the normal refresh gate. |
| Save | Save mutates draft metadata/request state before persistence. Success forces refresh, marks saved, completes save and invalidates link-menu caches. Failure preserves committed cache but does not undo all draft mutations. |
| Overwrite / Save as New | Both successful overwrite-prompt branches force refresh. Failure follows the existing error path without successful-refresh behavior. |
| Snapshot retry | A failed note snapshot preserves derived rows, todo labels and applied revisions/settings. Matching inputs wait one second; changed note/todo/settings inputs bypass cooldown. A new draft edit clears retry and resumes ordinary debounce. |
| Link menu | Its own lightweight versioned target snapshot and result cache do not invoke heavy relationship refresh. Failure retains targets/results and their captured version until recovery. |

## Snapshot and failure distinctions

`note_cache_snapshot_with_version` clones canonical notes and captures their revision under the same lock, without disk I/O. A poisoned lock returns an error. The compatibility `note_cache_snapshot` wrapper instead returns an empty vector on failure and must not replace the fallible paired API in this owner.

`refresh_cache` loads disk state before publication. Unreadable reload retains the committed cache and revision. A missing notes directory is different: the loader can create it and successfully publish an empty collection. A successful empty snapshot is valid data, not a failed snapshot.

`load_todos_or_last_good` is infallible to its caller: a read error returns committed `TODO_DATA`; poisoned fallback access yields an empty vector. This differs from note-snapshot retry behavior. The underlying todo reader can persist a format migration, so even nominally read-only tests need an owned working directory.

## Frozen output oracle

C1-A reuses `track_c_eager_note_rows` without changing its algorithm or the existing C0 emitter. Every scenario compares complete row title, badge, updated string, snippet, reason, note slug and todo ID, all three ordered tabs, the todo-label map and applied persisted revisions. Complete state identity, normalized relationship structure and revision receipts remain distinct. Setup/reference calculation and validation are outside owner timers.

Canonical resolved wiki links take precedence over typed note references and fenced-prose matching. `NoteCache::from_notes` resolves raw wiki references, including those inside fenced text; those canonical links can still produce RelatedNotes. Excluding fenced prose must not erase that precedence.

`NoteCache.links` alone does not represent todo linkage, typed entity references, outgoing references from an unsaved draft, textual title/slug/primary-alias needles, category/reason precedence or exact snippets. Preserve category and input order, all strings and unsaved state. The separately reproduced UTF-8 snippet-window panic is unchanged and outside this checkpoint.

## Attribution boundaries

The scoped observer is `cfg(test)` only and stores fixed-size numeric state in thread-local storage. Its scope restores the prior observer on normal return or unwind; disabled timers do not read a clock. Normal application builds contain no observer code. Independent foundation, execution-hook, caller harness and inert-discovery correction source reviews are CLEAR. Execution evidence below retains failed attempts; final measurements remain pending.

| Observation | Meaning |
| --- | --- |
| Refresh calls / successes / failures | Actual invocation attempts, completed recomputes and failed note snapshots; refresh duration includes invocation gates. |
| Snapshot calls / wrapper duration | Complete fallible note snapshot call. Existing `Metric::NoteSnapshot` separately supplies estimated clone bytes, lock wait and cache-hold measurements. Enabled hold time includes estimating bytes and cloning. Successful calls historically leave its completed outcome field at zero; the new observer separately records snapshot success. |
| Todo loading | Actual reader duration, distinct from snapshot and row passes. Persistence and external publication occur outside the measured refresh. |
| Label map | Actual inspected todo entries and map-preparation duration, separate from relationship-loop visits. |
| Three tab passes | Actual pass calls, every todo/note loop entry including skipped self-note entries, and each pass duration. |
| Fence transformations | Actual helper calls and input/output byte lengths. |
| Selected String constructions | Observed count, `len()` and capacity at construction points in needle/reason, fenced-content and snippet helpers. These are not allocator calls, allocator traffic, all temporary clones, peak memory or final retained size. |
| Derived installation | The measured assignment/publication portion of applying prepared derived data. |
| Enclosing CPU frame | Actual egui frame invoking `NotePanel::ui`, reported separately from nested refresh attribution. It is not native input-to-display latency. |

Snippet construction observations can precede a final ellipsis push, which can grow or reallocate the string. Matched-needle and output-row clones outside the instrumented helpers are not covered. Capacity snapshots must not be summed into an allocation or retained-memory claim.

Nested timers overlap and include test bookkeeping. Do not infer row computation by subtracting overlapping totals. Disabled test hooks can also add harness overhead to a C0-owner rerun; that rerun establishes output/algorithm continuity, not automatic latency equivalence with the pre-hook frozen G0. This supplemental observer-enabled series requires its own matched comparison in C1-B.

For the current three-pass implementation with N notes and T todos, expected actual counts are three passes, 3N note-loop entries, 3T todo-loop entries, T label-map visits and 3+3*(N-1)=3N fence transformations. Accepted note fixtures contain the stated N documents plus twelve todos. An unchanged next frame must record zero snapshots, passes, visits and transforms.

## Bounded caller scenarios and evidence

Use an owned `TrackCWorkspace`, an inert plugin manager, empty enabled plugins, owned paths and disabled hotkeys/dashboard/toasts/errors. Construct workspace before the note-environment lock, matching the workspace-to-note lock order of isolated legacy tests. Keep all owners and workers inside the workspace lifetime. No native action or cleanup is invoked.

Each sample opens and settles the actual panel, changes its draft through `replace_content_from_mutation` at deterministic egui time, and validates the predeadline no-refresh control. The measured frame calls actual `NotePanel::ui`. Local draft refresh runs after 250ms; external note reload and persisted todo change run before that deadline and must bypass it. Full ordered output, todo labels, applied revisions and retained unsaved draft are checked after every warmup and measured sample. A subsequent unchanged frame proves the idle control.

Full scenarios: local draft at100/1000/5000 notes, external note reload at1000, persisted todo transition at1000. Small smoke uses100 for all three transitions. External reload writes owned Markdown and calls actual `refresh_cache`; todo transition uses actual `replace_todos`. Local-draft/todo cases can publish the accepted canonical cache directly. Fixture creation, publication, expected output and validation stay outside owner timing.

Use five warmups, twenty retained measurements, nearest-rank quantiles and zero exclusions. Preserve every raw numeric observation and both favorable and unfavorable timings. Measurements are pending. Ordinary checks and Small smoke precede a coherent source commit; Full supplemental measurements and the original note owner run serially on that precise commit. The separate C1-A evidence appendix will record source/profile/fixture/observer/telemetry identities and actual commands. No release, repository-wide regression or native campaign is implied.

C1-A does not fuse passes, build an index, replace snapshot ownership, optimize alias lookup, change saved data or fix unrelated defects. The next smallest hypothesis is C1-B pass fusion, gated on accepted work attribution and full output parity.
### C1-A first ordinary execution (pending correction)

On the reviewed uncommitted C1-A source based on `bf105215c918656a5cdd18e64eef677f10cd9186`, the exact 14-test fast-dev selection genuinely ran: 13 PASS, the new caller harness FAIL at its initial complete row comparison (actual Mentioned tab empty while the independent eager reference expected six note rows). Nextest run `069003db-1bca-4609-b631-b1b1fd318d43`; build 2m02s, test execution 0.899s. Preserved raw log: ignored `target/performance/track-c/c1-a/nextest-ordinary.log`. This is not an accepted measurement or a reason to weaken the oracle. Sole source writer is correcting the setup boundary; only the failed new test will be rerun initially. The thirteen existing behavioral/publication tests need no redundant rerun unless the correction affects their behavior. C1-A remains IN_PROGRESS; frozen G0 unchanged; LOCAL ONLY.

### C1-A constructor isolation finding (before retry)

The source review localized the first failure: `LauncherApp::new` calls `PluginEditor::new`; metadata discovery constructs a temporary production plugin manager and default NotePlugin, which synchronously reloads notes from the owned disk. Publishing the fixture after app construction fixes that replacement boundary. The same discovery path also constructs default clipboard/system-data/macro services despite the supplied inert manager and empty enablement. Empty enablement alone was therefore insufficient evidence for the earlier no-plugin-startup claim. No cleanup command is called by this chain; the first run's outcome is still exactly13 PASS/1 FAIL, with no accepted benchmark.

Before retry, a bounded `cfg(test)` correction will make the explicitly inert manager path skip editor plugin discovery while retaining the normal production path. This corrects test isolation, not runtime performance. The frozen C0 baseline and raw measurements remain immutable; previous constructor-isolation wording is qualified by this finding, and future comparisons must disclose the test-setup correction. Parent owns documentation and runners; one writer owns the necessary source changes. LOCAL ONLY.

Constructor correction source review CLEAR: `PluginManager` carries an explicit `cfg(test)` inert marker set only by `new_inert_for_test`; `LauncherApp::new` uses supplied-manager editor metadata only for this marked test path. Normal managers and production retain ordinary discovery. `PluginEditor::with_available` centralizes the original sorted metadata/settings derivation. No production algorithm change. The failed caller test alone is now rerunning in parent session93086, raw `nextest-ordinary-retry1.log`; all source writers paused. The collector's reviewed missing snapshot wait/hold containment guard has been added; actual-data and negative-copy validation still pending.

Narrow retry1 compiled no tests: E0063 exposed a second `PluginManager` struct initializer (`with_clipboard_modifier_catalog`) missing the new test-only marker. Original retry log preserved. This normal constructor must keep marker false; sole writer correcting that initializer before the same focused test is retried. No test outcome is claimed for this compile failure.

Narrow retry2 genuinely executed the new test after a1m11s build, then failed at the historical `Metric::NoteSnapshot.completed` assertion (0 actual,1 expected), after full row/cache/revision and actual observer work assertions succeeded. Nextest run `a0f00624-0170-4ac6-ad5e-c39a5df4be2d`, execution0.227s; raw retry2 preserved. Successful note snapshots historically call `record_metric_sample` without recording a Completed outcome; failure records Error. Preserve that metric meaning: raw completed stays0; the new actual snapshot-success counter is the explicit success evidence. Sole writer is correcting the new harness expectation; parent parser now expects this legacy outcome field0, rather than modifying Track A telemetry to satisfy the new test.

Narrow retry3 genuinely ran the caller test (build43.53s, execution0.516s; run `aed8f71a-c0d2-48f0-8e8c-c1e2eb8c7a77`) and passed the local-draft owner validation/idle path before failing external-transition setup. The new helper incorrectly assumed fixture note1 begins with a Markdown heading; the accepted C0 fixture intentionally prepends a wiki link and Unicode snippet content. Correct only the external-transition fixture: prepend a deterministic ASCII heading to the complete existing content and match its actual loaded title. Keep the frozen C0 fixture/oracle unchanged. Retry3 raw log retained; no accepted benchmark yet.

Corrected ordinary caller test PASS: Nextest run `d9a236a9-4dbe-43e6-9940-67f89a913e14`, fast-dev build44.16s, execution0.837s,1passed/5166skipped. All three transitions completed full rows/labels/revisions/unsaved-state, predeadline control and unchanged-idle assertions. Together with the first run's13 existing PASS, the exact selected behavioral inventory is covered without repeating unrelated tests. Raw `nextest-ordinary-retry4.log`. Small observer-enabled attribution now running, honest UNCOMMITTED_SOURCE, raw `nextest-small-smoke.log`, parent session10351; no source writes while Cargo active.

### Small attribution smoke (uncommitted source)

Nextest run `79ab7c2d-8a2f-40da-bc2a-5dff0d02d8da` PASS1/1, build0.88s, execution17.891s; raw `nextest-small-smoke.log` retained. Protocol fast-dev/Small/PERF1/observer enabled/UNCOMMITTED_SOURCE, five warmups and twenty retained samples for each of three100-note transitions, zero exclusions. Full row/label/revision/draft parity and no-work controls passed for every sample. Each measured refresh inspected300 note-loop entries and36 todo-loop entries (work336), performed300 fence transformations, loaded todos once and took one successful note snapshot. Selected construction byte observations remain estimates at named points, not allocator totals.

| Transition | CPU frame p50 / p95 / max (ms) | Snapshot wrapper p50 (ms) | Heavy refresh p50 (ms) |
| --- | --- | --- | --- |
| Draft after debounce |24.8581 /26.1724 /26.2946 |0.2529 |22.1230 |
| External note reload |25.1530 /26.9383 /28.3046 |0.2414 |22.5179 |
| Persisted todo change |25.5499 /31.2364 /36.9360 |0.2219 |22.8561 |

Collector accepted all three actual records and preserved twenty numeric/identity receipts per record. Four copied-log negative cases rejected: altered frame quantile, missing refresh array, incorrect work counter and snapshot wait/hold exceeding wrapper. Original raw data was never rewritten. This is supplemental characterization smoke, not an optimization, committed-source Full evidence or a native display measurement. Independent numerical review is pending; C1-A remains IN_PROGRESS.

Independent Small evidence review CLEAR: all150 raw arrays across three records match collected JSON exactly, twenty entries each; every timing distribution was independently recomputed, work/containment/revision joins passed, and privacy scan found no personal paths/contact data. Each scenario has one structural identity and twenty distinct complete-state IDs explained by monotonic revision receipts. Reviewed smoke saved as `track_c_c1a_small.json`, retaining honest UNCOMMITTED_SOURCE. Collector saved as `track_c_collect_note_attribution.ps1`. Source/contract commit may proceed with C1-A explicitly IN_PROGRESS; exact committed-source Full and original owner evidence remain required before closure.
