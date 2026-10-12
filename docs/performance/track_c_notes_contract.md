# Track C C1-A note relationship contract

Status: PASS. Parent-owned contract; source, Full attribution and original-owner continuity independently reviewed CLEAR. Accepted source is `b47c1646d99c653b53e96bafa6b242259eabef17`. Frozen G0 remains `071243db068835617a054a74fb5734a6dd9f128d`; its numeric values and eager oracle are immutable. LOCAL ONLY.

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

On the reviewed uncommitted C1-A source based on `bf105215c918656a5cdd18e64eef677f10cd9186`, the exact 14-test fast-dev selection genuinely ran: 13 PASS, the new caller harness FAIL at its initial complete row comparison (actual RelatedNotes tab empty while the independent eager reference expected six note rows). Nextest run `069003db-1bca-4609-b631-b1b1fd318d43`; build 2m02s, test execution 0.899s. Preserved raw log: ignored `target/performance/track-c/c1-a/nextest-ordinary.log`. This is not an accepted measurement or a reason to weaken the oracle. Sole source writer is correcting the setup boundary; only the failed new test will be rerun initially. The thirteen existing behavioral/publication tests need no redundant rerun unless the correction affects their behavior. C1-A remains IN_PROGRESS; frozen G0 unchanged; LOCAL ONLY.

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

### Exact-source Full collection

Reviewed source committed locally as `b47c1646d99c653b53e96bafa6b242259eabef17` (`test(notes): [C1-A] characterize relationship-refresh contract`); tree clean immediately after commit. Parent Full runner verified source HEAD, clean source and no active Cargo before launch, saves/restores telemetry/source/profile/mode variables in finally, refuses existing raw logs, and rechecks HEAD/clean source afterward. Full attribution run `a8b4eaed-5132-4d5e-8c3d-b4eba7ec53f3` is active in session51023, raw ignored `nextest-full-attribution.log`; source writers paused. Five required scenarios,100 retained samples, no exclusions. Original note-owner rerun follows serially on the same source. Results and independent review remain pending; C1-A is not closed.

Full supplemental terminal PASS1/1 (build0.84s, execution282.635s,5166skipped); all five scenarios collected by the committed strict collector. Source `b47c1646d99c653b53e96bafa6b242259eabef17`, fast-dev,Full,PERF1,observer enabled,5warmups/20samples each,0exclusions. Candidate/raw remain ignored until review.

| Transition / notes | Frame p50 / p95 / max (ms) | Snapshot wrapper p50 / p95 (ms) | Heavy refresh p50 / p95 (ms) | Actual note+todo visits / fence transforms |
| --- | --- | --- | --- | --- |
| Draft /100 |24.9107 /26.4808 /26.6228 |0.2531 /0.2834 |22.2972 /23.2908 |336 /300 |
| Draft /1000 |233.9967 /243.0205 /243.2449 |2.1270 /2.6344 |231.1644 /240.3031 |3036 /3000 |
| Draft /5000 |1148.3915 /1215.0013 /1246.0377 |11.5232 /13.7048 |1145.7011 /1210.8903 |15036 /15000 |
| External note /1000 |234.3999 /245.5493 /264.6342 |2.3465 /2.7735 |231.7930 /242.6472 |3036 /3000 |
| Persisted todo /1000 |231.1382 /248.4943 /264.0510 |2.2306 /2.5697 |228.3185 /245.5075 |3036 /3000 |

Snapshot clone-byte estimates for local100/1000/5000 are559340/5709977/28606683 bytes. Selected fenced-string construction lengths total1487457/15215796/76249743 bytes per local refresh; these are observed helper construction lengths, not allocator traffic or retained memory. Full per-pass distributions remain in the appendix; do not subtract overlapping timers. At5000 notes each of the three row passes has a median in the366–395ms range, versus11.5232ms for the snapshot wrapper. This supports bounded C1-B fusion before indexed state; it does not prove future speedup. Original owner rerun is in progress on the same source, parent session52960; independent Full numeric review pending.

Independent Full attribution review CLEAR: all250 raw arrays exactly match five collected records, twenty entries each; independent nearest-rank timing quantiles, provenance, counter joins, timer containment and privacy passed. Across100 samples snapshot-wrapper/refresh ratios span approximately0.83–1.68%, and per-sample sum-of-three-pass/refresh spans95.80–98.67%. Quantiles of the actual per-sample pass sums (not sums of quantiles) have local100/1000/5000 p50 values21.6067/227.4301/1128.3059ms. Nested/observer overhead limits remain applicable.

Preserve contrary/variable receipts: the external1000 scenario has two structural hashes across twenty samples; the other four scenarios have one each. Filesystem minute-formatted updated strings are retained exactly and can change while fixtures are written. Every individual sample passed full frozen row equality, so do not normalize away this variation or require false across-sample identity. Complete state IDs retain monotonic revision variation. Original-owner run/review and final closure remain pending.

### Original owner continuity rerun

Original note owner PASS1/1, Nextest run `9210b828-3287-4144-9fd3-c2520d7e3bee`, build0.84s, execution120.677s,5166skipped; raw `nextest-full-original-owner.log` and strict three-record collection retained. Exact same committed source/profile as Full supplemental; observer is not armed for this original owner, although disabled test hooks remain compiled in. Fixture sizes100/1000/5000,5warmups/20retained samples each,0exclusions. Frozen G0 remains unchanged.

| Notes | Frozen G0 p50 / p95 / max (ms) | C1-A original owner p50 / p95 / max (ms) |
| --- | --- | --- |
|100 |22.3415 /23.4536 /26.5596 |23.0671 /33.1873 /35.7915 |
|1000 |229.0207 /254.2780 /385.6323 |232.4852 /254.9681 /286.6619 |
|5000 |1153.8690 /1182.8377 /1255.7501 |1163.7188 /1186.3802 /1216.1710 |

Retain the higher median/p95 observations and differing maxima; no improvement, statistical equivalence or whole-application claim follows. This original owner differs from the actual UI/observer-enabled supplemental series, so do not mix their quantiles. Inert constructor correction is a separate disclosed test-setup change; production behavior is unchanged. Independent original-owner raw/identity/work review is pending. Native display/input and allocator traffic remain NOT MEASURED; no cleanup operation was executed by these owner tests.

### Reproduction

Use the exact source above and `fast-dev`; do not overwrite frozen G0 or existing raw logs. Serialize Cargo, save/restore these process variables in `finally`, and use fresh owned raw-log names. The original Full invocations were:

```powershell
$env:MULTI_LAUNCHER_PERF='1'
$env:ML_SKIP_CLIPBOARD_SYNC='1'
$env:ML_TRACK_C_PROFILE='fast-dev'
$env:ML_TRACK_C_BENCH_MODE='full'
$env:ML_TRACK_C_SOURCE_SHA='b47c1646d99c653b53e96bafa6b242259eabef17'
cargo nextest run --lib --cargo-profile fast-dev --test-threads 1 --no-fail-fast --success-output immediate --run-ignored only -E 'test(track_c_c1a_benchmark_open_note_panel_refresh_attribution)'
cargo nextest run --lib --cargo-profile fast-dev --test-threads 1 --no-fail-fast --success-output immediate --run-ignored only -E 'test(track_c_benchmark_note_panel_heavy_refresh_owner)'
```

Capture each run separately; parse supplemental output using `track_c_collect_note_attribution.ps1` with `-ExpectedSource` the exact SHA, `-ExpectedMode Full`, `-LogPath` its raw log and a fresh `-OutputPath`. Parse original-owner output using `track_c_collect_owner_results.ps1` and its `-LogPaths` argument. Source SHA, profile and protocols must match actual execution; never relabel an uncommitted run as committed evidence. Original guarded runners additionally required clean source before/after, no competing Cargo, no log overwrite and restored every variable.

### C1-A closure

Independent original-owner review CLEAR: all21 raw arrays across three records match JSON; every fixture/protocol/full output-state ID/structural ID/revision receipt/viewport/work-unit/work-counter array exactly matches frozen G0, apart from source and elapsed values. The100-note p95 was41.50% higher in this rerun; retain that unfavorable observation without inferring a causal production regression or equivalence from one observer-inactive test rerun. The complete supplemental/continuity evidence is saved in `track_c_c1a_evidence.json`. Source, method, full semantics, counts, quantiles and privacy reviews are CLEAR; C1-A PASS. No optimization was implemented. Next checkpoint C1-B should fuse the triplicate passes against the immutable eager oracle and this matched supplemental series, with failed-snapshot/debounce/revision/unsaved-order behavior preserved. Indexing remains gated until fusion has been measured.
