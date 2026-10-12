# C2-G query-to-usable CPU viewport plan

State: protocol frozen; v6 source/ordinary/check/Small independently cleared.
Source committed d161e8da; Full01 failed at large viewport menu fixture.
Bounded clipped-hit fixture correction and new-source verification pending. LOCAL ONLY. Accepted sourcef246958ced8d3be8b712e222461311824880e48c.
C2-F is closed SKIPPED;17/29 checkpoints closed. C2-G is one bounded observation
milestone, with no query/search/scheduling/layout optimization.

## Objective and owner

Observe accepted GUI query mutation through the first complete usable egui CPU
viewport. Include provider/debounce waiting and intervening frames; exclude physical
input delivery, GPU submission/composition and native presentation. One app-local
field is backed by private `src/gui/track_c_query_observer.rs` lifecycle/types.
Keep at most the latest pending intent and bounded acceptance/paint receipts, with
fixed origin/outcome/cause enums; no raw query logging, registry or unbounded trace.
Disabled observation performs no timer reads, metadata copies or collector updates.

One source writer owns that new module plus `src/gui/{mod,render,search,command_host}.rs`
and `src/performance/track_c.rs`, including only directly required test fixtures and
contracts. Parent owns documentation, strict collection, evidence, Cargo and Git.
Preserve historical metric/phase numbering and meaning; append any distinct query
phase. Preserve C0 hotkey/composite and Track A LauncherRowsBuilt work-unit meaning.

## Required integration

1. Start before accepted TextEdit mutation/search/debounce, history recall, pending
   query consumption and normalized command QueryPolicy::Set. Avoid double starts
   through shared handlers. Macro query changes invalidate/supersede older GUI intent.
2. New intent abandons the previous sample; A→B→A has distinct sequences. Hide,
   disposal or replacement root ownership prevents stale completion. Background,
   provider, capacity, selection, resize and paint never restart intent.
3. Stamp cached, empty and ordinary search acceptance, including C2-C equal-result
   generation retention. Read-only/radial searches cannot accept ordinary GUI intent.
   Empty browse need not set existing validity/search-query fields.
4. Distinguish SearchAccepted and CommandReplacementAccepted telemetry receipts.
   The latter captures provider/catalog before and after normalized direct result
   installation. Later invalidation/search/recursive activation or another install
   invalidates/supersedes it; actual search acceptance takes precedence. Never invent
   LauncherSearchOutcome or modify production last_search_* revisions for telemetry.
5. Capture the accepted receipt actually painted. Complete once at root frame end
   after late source checks. Require current sequence/receipt/result generation,
   visible root ownership, no pending query/background/debounce/capacity or excluded
   modal/OCR/color/coordinate/screen-draw ownership. Search receipts require existing
   pending/deferral/stable-current sources; direct replacements require their own
   unchanged installation baseline, not stale earlier search state.
6. Keep the complete historical hotkey predicate unchanged when extracting common
   root checks. Valid empty output can complete. Do not rebaseline old receipts after
   provider/catalog change. Observe actual geometry rebuild sites with bounded
   counters/reasons for query, source changes, display/resize and navigation; expose
   multiple rebuilds instead of inferring them from invalidation counts.

Direct replacement provenance means command-owned output stable from installation
through CPU paint, not coherent search-snapshot construction. Overlapping lifecycle
intervals are not additive CPU phases; cancelled durations are not completed latency.

## Controlled workload inventory

Small: four records/80 sequences at100 actions. Full: twelve records/240 sequences
at100/1,000/10,000 actions. Each record has five warmups,20 retained samples and zero
exclusions. Freeze concrete provider prefix/payload, exact event/frame steps, finite
settling limit and emitter/collector fields before execution; exceeding the frame
limit fails a sample. No real providers, network, native input or sleep-driven tests.

| Record | Sequence and expected intent outcomes |
|---|---|
| typed_suffix_sequence | Settled `app ` seed; actual suffix TextEdit events `0`,`00`,`000`,`00009` yield four completed intents. |
| app_provider_supersession | Settled `app` seed, append-space edit completes; controlled provider becomes capacity-deferred; release/mutate args/notify once, then beta edit supersedes before old output paints; beta empty output completes, then provider completes. Four intents: completed/superseded/completed/completed. |
| equivalent_filter_sequence | Actual edits alternate two absent exclusion filters preserving complete catalog; two completed intents, unchanged raw-query invalidation semantics. |
| viewport_controls | Settled catalog,960→480→960 at640 height; first→middle→last selection; an actually visible row context-menu open/close. Seven controls, no new intent. |

Fixture preparation/canonical publication/initial settling and independent references
stay outside the sequence clock. Pending release/supersession and returned egui
FullOutput retirement are inside. Record whole-sequence elapsed separately from
fixed per-step completed/abandoned outcomes, validity masks, latency/control times,
sequence/source/install/paint receipts, frames and actual rebuild/row counts.
Validate full ordered Actions including args, source/search state, selection,
eager geometry, visible widget identities, extents and popup ownership. Reuse
unchanged eager/reference bodies with separate fixture/full-output/state/viewport
signature domains. This is controlled headless mechanism coverage, not production
typing frequency or native display latency.

## Fixture safety exception

Read-only audit found root render queues dashboard All on first frame even with
dashboard.enabled=false. Its production backend includes recycle-bin status and
host system/process queries. No tests/probes were executed in this audit.
Independent reviewer cleared the smallest fixture-only correction: set existing
private dashboard_initial_refresh_queued=true in owned inert root_list_app_at
immediately after construction and before any frame. Worker starts dormant and
existing wait_for_refresh safely returns with target0. No global constructor policy,
production API or duplicate fixture is needed.

This is a deliberate exception to the historical C2-E protected fixture-helper body;
all eager/reference/oracle/benchmark bodies and frozen G0 data remain unchanged.
Disclose suppressed background status work in future comparisons; never attribute
every G0 timing difference to runtime optimization. Other necessary generic test
constructors must migrate to owned inert fixtures while retaining their assertions.
Do not execute them beforehand. Never perform recycle cleanup, inspection or recovery.

## Verification and done criteria

Preserve existing safe fallback/debounce/capacity and deferred-activation contracts.
After reviewed fixture correction, run exact root-query/current-source frame tests;
migrate only required screen-draw/history/keyboard/radial root-policy fixtures. Add
one compact lifecycle contract covering supersession/A→B→A, stale/duplicate receipt,
old debounce paint, equal-generation acceptance, empty output, direct-replacement
precedence and disabled instrumentation. No full suite/native/startup campaign.

Source handoff → protected-helper/fixture/source-method review → exact serial
fast-dev Nextest and non-test check → frozen strict collector/protocol → reviewed
Small → clean local source commit → exact-source Full → independent evidence review.
Resolve concrete failures at their owner and preserve failed logs. Done means
exactly-once current usable completion, honest cancellation/source/rebuild accounting,
unchanged complete output/interaction semantics, inert disabled observation and
strict exact-source evidence. Parent then closes C2-G and proceeds to C2-H.


## Frozen collector and exact protocol

The source/method review cleared the four actual-egui recipes and fixed schema.
`track_c_query_observation/protocol_freeze.json` binds the pre-measurement source
and collector hashes; the adjacent independent files bind twelve fixture recipes
and51 full ordered Action/count/logical UTF8 step outputs. Eight parser and20 nested
synthetic controls test rejection behavior only; they are not Rust or timing evidence.
The collector requires committed source for Full, exact inventory/sample counts,
current accepted/painted generation and sources, correct superseded disposition,
per-recipe actual rebuild work, and bounded whole/frame/intent timing. It preserves
all20 samples and separates completed latency from abandoned diagnostic elapsed.

Whole sequence elapsed includes interstep validation. Each actual frame duration
includes Context::run and FullOutput retirement; completed intent terminal time
must fit inside its terminal frame. These overlapping durations are not additive
CPU phases. No native input delivery, GPU composition or production typing-frequency
claim follows from this controlled headless protocol.

After an owned serial benchmark run, collect from repository root with:

```text
python docs/performance/track_c_query_observation/collect_query.py --source <exact-source-SHA-or-UNCOMMITTED_SOURCE-for-Small> --mode <small-or-full> --output <new-output.json> <raw-log>
```

Raw logs remain ignored under target. Only sanitized controlled receipts and
summaries are committed. Existing eager/reference/oracle bodies remain frozen;
root_list_app_at has only the reviewed dashboard-initial-queue suppression exception.

### Precommit command/pending handoff correction

Final source review found a no-doublebegin gap when one normalized outcome explicitly
supplies matching QueryPolicy::Set and PendingQueryPolicy::Set. Current todo handlers
publish pending refreshes with QueryPolicy::Keep; the four measured Small recipes use
TextEdit and did not expose this composition. Use one observer-local sequence token
for the paired outcome, consumed once only while that same intent/query remains
active. Unrelated pending publication or supersession clears it. Independent same-text
pending requests remain distinct; existing pending refreshes still begin at consumption.
An owned actual command/pending root-frame contract and corrected-source Small rerun
are required before source commit. Historical v5 Small remains preserved.

V6 corrected-source evidence:23distinct ordinary tests PASS (earlier unchanged cases
reused plus exact new handoff/lifecycle execution), non-test library check PASS, Small
owner/strict collector/independent review PASS4records80sequences200intents340frames.
Small has180completed20superseded/0cancelled; all40 timing triplets independently
recalculated. The viewport sequence maximum192.3688ms is retained. Full is pending
exact clean committed source; this checkpoint is not closed by Small alone.

### Historical Full01 clipped-click correction (superseded by schema2)

Exact d161 Full01 failed the1k viewport real-menu-open assertion after seven emitted
records. No partial subset is accepted as Full evidence. Planner and reviewer found
a concrete fixture defect: intersecting response rectangles were admitted, then
their full centers used even when outside the actual clickable clip. Choose a
positive-area clipped intersection interior, retain real menu/eager assertions, and
verify owned large viewport cases. No new frames, sleep, animation or production
change is justified. The seven-step generic visible-row protocol and signature
bindings remain; actual owner/viewport receipts may change consistently with the
corrected hit. Preserve failed Full01 and disclose new source before rerunning.

The first clipped-hit oracle retry opened the real menu but failed a newly added
current-rectangle containment assertion. Pinned egui hit-tests previous-frame
widgets, then attaches interaction by ID to current response rectangles; scrolling
can move those rectangles. Correct the new assertion to bind the clicked prior ID
to the current popup owner/index/action, while retaining prior clipped-point
containment, real menu checks and current eager geometry parity. No settling or
production change follows. Preserve oracle_attempt_07_failure.json.

### Adopted schema2 controlled viewport protocol

Oracle09 confirmed N1000: prior row0 ID was clicked, but current rows15..39
omitted that response, so no menu could attach. Preserve production animation.
The revised viewport recipe adds two explicit CPU frames after last selection: first
advances simulated input19ticks (19/60s, beyond pinned0.3s maximum), second paints
the updated offset. Both execute full eager/action/paint validation and remain
inside whole-owner timing with receipts. No sleep, retry or untallied frame.

Viewport input ticks are9,10,11,12,13,32,33,34,35; query recipes remain contiguous
from9. Schema2 emits and strictly validates input_tick. Viewport steps now total9,
popup-open index7/close8. Small380/Full1140 actual frames; intent counts and5warm/20
retained/zero exclusions are unchanged. The8-frame limit remains per query intent.
Only three viewport recipe signatures change; independent outputs grow51to57.
The nine-frame controls are NOT COMPARABLE with historical seven-frame controls,
and simulated animation time is not wall waiting or native interaction latency.
V1 collector/bindings/committed measurements remain immutable; companion
collect_query_v2.py and v2 bindings require independent review/freeze before execution.

Schema2 source, exact collector and independent bindings now reviewed CLEAR.
protocol_schema2_freeze.json freezes all six source hashes, collector, binding
hashes and 25 passing synthetic controls before focused oracle10 execution.
All42 protected helpers and byte-identical Notes confirmed. Schema2 oracle and
Small/Full acceptance remain pending.

Schema2 oracle10 PASS (four N100 recipes plus1k/10k viewport). Small03 owner PASS
13.931s, strict collector4records80sequences200intents380frames,180completed/20
superseded. Whole p50/p95/max ms: typed62.1107/68.5586/70.3971; provider43.2483/
44.4446/45.0983; filters34.4561/38.7398/43.4390; nine-frame controls151.5174/
247.8101/248.6301. No historical viewport comparison or improvement claim.
Independent Small evidence review and exact committed Full remain pending.

Independent schema2 Small review CLEAR: six live source hashes, collector/bindings,
raw equality and all42 timing triplets reproduced. Reviewed source+Small may commit;
Full acceptance requires the exact clean new commit.

C2-G CLOSED PASS: exact clean c6c0f64a Full02 owner481.069s/Nextest481.131s,
strict12records240sequences600intents1140frames;540completed60superseded0cancelled.
Independent acceptance CLEAR126triplets57bindings/raw equality/source/paint/work/
privacy. Native/GPU remain unmeasured. Full timing table and distinctions are in
query_observation/schema2_results.md.18/29checkpoints closed, nextC2-H. LOCAL ONLY.
