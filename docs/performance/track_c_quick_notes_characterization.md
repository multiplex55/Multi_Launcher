# C2-F Quick Notes characterization contract

State: IN_PROGRESS; characterization only. No production reuse candidate authorized.
Accepted starting HEAD: `e0564d601141240ba1a06b56f3967acb0897f9e2`.
Quick Notes source blob: `5ea37d521afbe4280ae0e445de85c89511cef084`.
LOCAL ONLY. C2-E is closed as a measured skip; accepted C2-C remains intact.

## Objective and ownership

Attribute complete Quick Notes cold refresh and variable-height layout cost before
deciding whether one narrow reuse candidate earns implementation. Sole source
writer owns `src/gui/notes_dialog.rs`; changes are cfg(test) observations,
independent references and owned synthetic fixtures. Parent owns collection,
protocol, evidence, Cargo execution and commits. Independent review precedes
collection and any optimization decision.

`maybe_refresh_derived` owns coherent snapshot/index/metadata preparation and
atomic candidate installation. `update_filtered_projection` owns ordered original
indices. `ensure_notes_geometry` owns invalidation and replacement; the UI owns
retired geometry disposal and anchor remapping. `measure_notes_geometry` includes
key/atlas/allocation/row layout, so its timer must not be described as a pure
font loop. Existing NoteSnapshot metric meanings remain unchanged.

## Bounded inventory and clocks

Small: N=100, ten records, 200 retained samples. Full: N=100/1000/5000,
thirty records, 600 samples. Five warmups then twenty retained samples per
record; zero exclusions. At screen height 640 use eight recipes: canonical
revision with final-note content edit; metadata-only settings; sparse search;
case-only equal normalized filter; explicit cold geometry with settled fonts;
unchanged warm repeat; early wide Unicode header; actual 960-to-300 screen-width
transition. Height 180 adds only explicit cold geometry and warm repeat.
Concrete recipes are frozen below; screen dimensions are not actual ScrollArea widths.

Prepare canonical cache publication, owned fixture files, app/dialog, fonts,
background quiescence and eager references outside the owner clock. Inside it
perform the recipe trigger and actual Context::run/dialog.ui, including retired
derived values and geometry disposal. Publish a changed canonical revision before
the clock and let timed UI detect it naturally. The width-transition frame belongs
inside the clock. Record actual row-key width and ScrollArea dimensions.

This supplemental owner also drops the returned egui FullOutput inside timing.
Its scope differs from C0; do not compare those owner distributions as identical.

Use a local opt-in cfg(test) observer only during the measured operation, with
constant phase clocks and actual work counters, no per-note clocks. Distinguish
snapshot, search-index, metadata, shared projection, ensure geometry, nested whole
measurement function, and complete owner. Installation/disposal/anchor/widget work
belongs to full-owner residual; do not overlap it with projection phase. Warm and
case-only helper calls can occur with zero scanned entries. Report residuals and
all p50/p95/max distributions, including contrary cases.

## Frozen recipe constants

Start with track_c_workloads::note_fixture(N), canonical publication after inert
app construction. Both backlinks and task lists explicitly on; default context
fonts/style, wrap=None, pixels-per-point=1; screen width 960 unless transitioning.
No editor, closed popup, scroll offset zero, empty search unless specified.
Settle initial dialog/fonts/background outside timing. Sign initial and final
canonical inputs, settings/query and dimensions. Ordinary contract must resolve
and freeze any deferred egui width constraint before benchmark collection.

| Recipe | Setup and timed transition |
|---|---|
| revision_final_preview | Prepend `C2F changed final preview 東京 🙂\r\n` to final note content; publish outside timing; timed UI naturally detects revision. Assert preview changed and full derived rebuild. |
| metadata_settings_only | Inside timing set backlinks=false, tasks stays true. Assert removed backlink text; zero snapshot/index work. |
| search_sparse | Inside timing set search=`track-a-note-00050`; complete ordered projection must be `[50]`. Zero snapshot/index/metadata work. |
| search_case_only | Settle with lowercase sparse query; inside timing use `TRACK-A-NOTE-00050`. Preserve normalized filter/projection generation/geometry with zero scanned entries. |
| geometry_cold | Inside timing take/drop installed notes_geometry then actual UI. Disposal stays timed; settled fonts; no derived preparation. |
| warm_repeat | Identical settled state, no trigger, actual UI; zero full derived/geometry work and bounded paint. |
| early_wide_unicode | Special initial note0 has no aliases and no Alias/Aliases body lines; short display title. Publish title/heading replaced by `東京🙂W`.repeat(256) outside timing. Timed UI detects revision; assert early header/content width and later width_before increase. |
| width_960_to_300 | Genuinely settle at960, run first actual300-width transition inside timer. Assert actual ScrollArea/key width change and complete eager narrow parity. No derived note preparation. |

Independent method review CLEAR for planning: observe shared projection function
at both callers; include retired geometry disposal/anchor in full owner; reference
published canonical inputs rather than raw fixtures. Concrete recipes supplied
by planner against accepted HEAD and parent checked marker formatting in source.

Parent independently reconstructed the frozen synthetic inputs before execution.
Seed is `0e95fb4ed4c02a56`; N100/1000/5000 canonical alias counts are
300/3000/15000, canonical link counts 8/69/344, index UTF-8 bytes
508775/5205217/26083431, and sparse indices `[50]`. Raw fixture Note identities
are `2562218e371ed95b` / `b36d68ebb9af6615` / `fdf17b415d1e03d0`;
canonical identities with the original relative synthetic paths are
`cb61ebc0c9c5a9f3` / `dc0a76a6abbeef47` / `8796bffb35e72bb4`.
Independent reconstruction review CLEAR for these fixtures, not a general wiki
resolver. All thirty emitter-specific fixture/canonical/index/metadata/projection
bindings are frozen in the collector before execution.

## Semantic references and safety

Validate complete canonical Note equality before owned-root path normalization;
full search-index strings; all metadata strings; ordered original indices; every
geometry row/map entry and content extent; actual eager widget header, preview,
separator and bounded viewport receipts. Freeze separate fixture, canonical-note,
index, metadata, projection, geometry and viewport domains. Keep revisions and
generations separate from normalized semantic identities.

The existing production-backed expected metadata is not an independent oracle.
Add immutable cfg(test) references that do not call production index/metadata/
preview/checkbox/backlink functions. Independently count distinct canonical source
slugs whose published Note.links contain the target. Preserve exact field order,
separators, Unicode, mixed CRLF, checkbox prefixes, 120-character preview and
ellipsis. Freeze existing actual eager widget oracle bodies; label supplementary
production measurement estimates honestly.

Create TrackCWorkspace before new_isolated_app, publish only synthetic cache
after app construction, retain guards through validation, and drop owners/guards
before workspace cleanup. No generic new_app, default plugins, clipboard,
external action or cleanup execution. Partial retained-capacity evidence must
state included containers and actual old/new overlap; it is not allocator/RSS.

Preserve last-good failure/race/retry, edit freeze and unsaved sentinel, stable
original indices, identity/intra-row anchors and nearest survivor, horizontal
offset, bottom reachability, offscreen popup ownership, actual atlas/style/font/
scale/wrap invalidation and width_before-to-prefix_width recurrence.

## Verification and completion

Retain nine exact existing ordinary tests: the seven named in approved C2-F,
plus track_c_oracle_quick_notes_dialog_matches_frozen_eager_geometry and
track_c_oracle_quick_notes_horizontal_width_transition_matches_frozen_eager_geometry.
Add one compact attribution/recipe contract and one ignored benchmark owner.
Parent audits isolation and independent references, freezes emitter/schema/work/
fixture bindings and helper bodies, then runs serial fast-dev exact ordinary
tests, reviewed Small, and clean committed-source Full. No broad suite, native
campaign, release build, dependency or runtime algorithm change.
One non-test fast-dev library check will verify the cfg(test) boundary, since
ordinary test compilation alone cannot catch an unguarded observer reference.

Characterization is complete only with reviewed complete inventory, independent
identity/work/numeric/privacy validation, meaningful scoped tests and an explicit
candidate-or-skip decision. An optimization requires a separate reviewed bounded
candidate contract and frozen acceptance limits before candidate results.

## Frozen collection protocol

`track_c_collect_quick_notes.ps1` accepts exactly nineteen scalar fields and
forty-nine twenty-sample arrays per record. Eight timing arrays preserve five
disjoint phases, nested whole measurement and exact owner residual. Twenty-five
work/state arrays separate helper invocations from scanned/materialized entries;
eight identity arrays separate complete semantic domains from viewport receipts.
Revision, settings, viewport, width, visible-original-index and capacity receipts
retain all samples. No samples are excluded. Quantiles use sorted positions
9/18/19 (zero based) for p50/p95/max.

The collector freezes all thirty independent input bindings, exact recipe and
work inventories, positive finite actual widths, revision/generation relations,
snapshot wait/hold nesting, and actual geometry replacement pairs. Ordinary
per-sample assertions establish eager full extent, exact two-row overscan and
actual header/title/meta/preview/separator rectangles before receipts are emitted.
The five new reference helpers have final reviewed fingerprints; thirteen C0
oracle/fixture/benchmark helpers remain byte-identical after newline normalization.

Each endpoint capacity receipt has twelve components: Note vector storage,
Note-owned string capacities, nested Note vector storage, search-index vector
storage and string capacities, metadata vector storage and three-string capacities,
filtered-index vector storage, geometry-row vector storage and identity-string
capacities, geometry-map key-string capacities, and geometry-map entry count.
The last component is a count, not bytes. Conservative container lower bounds on
the 64-bit target use 216/24/80/80 bytes per Note/String/metadata/geometry-row;
they do not assert exact ABI sizes. Actual receipts use Rust size_of and capacity
(Small observed Note vector bytes224*N). Nonempty payload
components are checked at both endpoints. Eight overlap components describe only
the actual old/new geometry pair; explicit take/drop cold geometry has no old
pair. Other derived-value overlap, HashMap buckets/control blocks, font galleys,
harness allocations, allocator peaks and process RAM are excluded. These receipts
are partial storage observations, never a whole-owner peak estimate.

Before Cargo execution, source/method and the focused widget/atlas corrections
received independent CLEAR review. Collector synthetic checks accepted ten Small
and thirty Full records and rejected sixteen malformed cases plus ten timing/capacity
guard cases. Synthetic data is parser validation only, not performance evidence.

## Verification history

- Ordinary attempt01 stopped at compile101 with zero tests: a receipt loop used
  the enclosing rectangle struct instead of its optional rectangle element.
  Corrected the binding; no runtime behavior changed.
- Ordinary attempt02 compiled and executed ten tests: nine existing tests passed;
  the new contract failed because observation began before untimed atlas oracle
  setup. Kept the expected work count and moved metric reset/observer activation
  to immediately before the actual owner timer. Focused lifecycle review CLEAR.
- Contract attempt03 passed 1/1, including actual width transition, atlas replacement,
  full extent, overscan and widget rectangles. Together with attempt02 this covers
  all ten distinct ordinary tests. No broad suite was run.
- Non-test `cargo check --lib --profile fast-dev` passed (30.45s), verifying the
  cfg(test) boundary. Committed-source Full results remain pending.

Small attempt01 executed PASS (31.594s), ten records and 200 retained samples.
The first strict collection attempt rejected fixture framing: the independent
parent binding builder omitted the initial screen-width number included by source.
Source-derived v2 inserts initial960 before final width; original bindings and raw
log are preserved. Independent focused review confirmed the correction, with no
runtime change or owner rerun. Strict collection and independent Small review CLEAR:
490 arrays/9,800 entries, 190 scalars and 100 independently recomputed timing
triplets. All 200 samples satisfy work, identity, timing, revision/settings/width/
viewport/capacity/overlap and privacy checks. No exclusions or reruns.

Small N100 owner p50/p95/max (ms), diagnostic uncommitted source only:

| Recipe / height | p50 | p95 | max |
|---|---:|---:|---:|
| revision_final_preview /640 | 16.6087 | 17.6571 | 17.7070 |
| metadata_settings_only /640 | 13.2316 | 14.1785 | 14.9905 |
| search_sparse /640 | 2.3591 | 2.5020 | 2.8650 |
| search_case_only /640 | 0.4158 | 0.4955 | 0.7255 |
| geometry_cold /640 | 11.0850 | 14.7043 | 14.9797 |
| geometry_cold /180 | 11.0831 | 11.9608 | 14.3993 |
| warm_repeat /640 | 0.6628 | 0.7028 | 0.7352 |
| warm_repeat /180 | 0.5439 | 0.6808 | 0.8204 |
| early_wide_unicode /640 | 17.1623 | 25.5128 | 27.3834 |
| width_960_to_300 /640 | 11.2285 | 11.7031 | 11.7501 |

These are characterization values, not gains. No input prevalence, native display
or whole-owner peak memory conclusion follows. Full must run on this exact clean
committed source before a candidate or measured-skip decision.
