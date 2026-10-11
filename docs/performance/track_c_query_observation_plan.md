# C2-G query-to-usable CPU viewport plan

State: implementation authorized; execution protocol still to be frozen before
tests/benchmarks. LOCAL ONLY. Accepted sourcef246958ced8d3be8b712e222461311824880e48c.
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
| typed_suffix_sequence | Settled app seed; actual TextEdit edits yield suffix lengths1/2/3/5; four completed intents. |
| app_provider_supersession | App completes; controlled provider becomes capacity-deferred; release and supersede before old output paints; app completes then provider completes. Four intents: completed/superseded/completed/completed. |
| equivalent_filter_sequence | Actual edits alternate two absent exclusion filters preserving complete catalog; two completed intents, unchanged raw-query invalidation semantics. |
| viewport_controls | Settled catalog,960→480→960 at640 height; first→middle→last selection; last context-menu open/close. Seven controls, no new intent. |

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
