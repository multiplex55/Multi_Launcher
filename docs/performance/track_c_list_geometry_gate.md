# C2-D â€” exact cold list geometry gate

Status: IN_PROGRESS, reviewed eager baseline; one bounded runtime candidate experiment approved after this evidence commit. Accepted base `43c90ca759b4c9ec3fd92747bd8f28c933805c80`; C2-C accepted runtime `da838c6d59cc5a4172c3a2471398a673dd0188f9`. Fourteen of29 checkpoints closed. LOCAL ONLY. The planner inspected the actual list owner and callers; one writer owns src/gui/render.rs for the bounded candidate. Parent owns this plan/protocol/collector/evidence/Cargo/Git. Parent GO authorizes only the Phase 2 experiment below; acceptance requires measured full-owner savings and exact semantics.

## Ownership and existing constraints

RootListGeometryCache::ensure returns before display formatting when its complete key and retained font-atlas Arc match. A cold pass measures all N rows and constructs six geometry vectors and full extents. Width and x offsets accumulate: an early wide/no-wrap row changes later row layout widths. Alias presence as Some, rather than alias text, controls description suppression. Result generation also contributes widget IDs, and popup cleanup remains required. C2-C's equal-result retention does not justify reusing changed presentation or previously invalidated geometry.

## Phase 1 â€” bounded test-only characterization

Reuse owned inert root_list_app_at/run_root_list_frame and existing private eager helpers. Small100, Full100/1k/10k; five warmups and20retained samples per recipe, zero exclusions. Twelve recipes: fresh-context first-open CPU frame; replacement preserving5% or20% ordered presentation prefix; new generation with identical presentation (changed args is still a full Action replacement); last-row display edit; first-row width expansion with wrapping disabled; full-path toggle; wrap change; verified actual font-atlas replacement; pixels-per-point change; viewport-width change; warm control. Prepare complete incoming Actions outside clock. Measured installation/disposal/invalidation and actual root frame stay inside owner consistently. This layout-owner stage excludes search scoring/provider work and is not query-to-usable/native latency.

First-open must use a genuinely fresh egui context and no prior root layout; report actual first CPU frame separately from settled-font invalidations. Old C0 cold recipe warms nine frames first and remains unchanged. Environment changes must exercise real egui state and demonstrate required key/atlas transition; pointer addresses never enter emitted metrics. Do not weaken assertions if fresh context/font replacement needs a fixture correction.

Construct workspace before apps; use inert manager, clear constructor-derived alias maps, disable plugins/hotkeys/toasts/dashboard/pins, settle completion and any dashboard work outside timing; drop app/workers before workspace. No default providers/native events/user data. Original frozen eager geometry/search oracles and accepted C2-C tests remain unchanged.

Every sample checks complete ordered Actions, six geometry vectors/total height/content width/x offset, ScrollArea extents, absolute visible indices/widget IDs/rectangles and generation/state receipts. Eager oracle executes outside measured context/timer. Empty layout needs explicit assertions because existing eager validator unwraps the final row. Distinguish full output/projection identity from viewport receipts. Agree distinct versioned emitter and parent-independent fixture bindings before execution; strict collection rejects missing/extra/duplicate fields/cases, wrong source/profile/bindings and incomplete samples.

Use existing actual test observation for rebuild count/measured rows/cells/geometry time; geometry_ns=0 on warm control, not stale prior time. Capture environment/atlas change receipts and exact presentation-prefix length from independently constructed old/new fixture data outside clock. Partial logical geometry vector capacities may be reported explicitly; they are not allocator/RSS or total retained-memory measurements. No invented baseline comparison timing. Additional actual-site test observer requires identified missing metric and parent approval.

## Phase 2 â€” conditional runtime candidate (experiment GO; acceptance pending)

Only after reviewed committed baseline, consider one bounded previous list presentation projection, exact ordered display prefix and cumulative geometry checkpoint. Require exact environment and retained atlas Arc. Recompute every affected suffix row and final extent; matching later text alone cannot prove geometry. Preserve result generation/widget IDs/popup cleanup for complete-Action replacement even when display matches. Display/mode/environment invalidation rejects or purges retained state, preserving accepted alias reload semantics. No cloned full Actions, multigeneration cache, grid algorithm or background galley measurement.

Gate: lower cold p95 with full owner cost for realistic5â€“20% shared prefixes and no material warm regression, exact eager responses/selection/menu semantics and bounded reported logical retention/old-plus-new peak. Retain adverse first-open/earlywidth/environment cases. Formatting/comparison/storage and egui galley caching may erase value; measured SKIP is valid. Synthetic overlap does not prove production frequency or app-wide savings.

## Verification and done criteria

Source/fixture/protocol review before exact serial fast-dev execution. New bounded ordinary contract + existing root_list_geometry_matches_eager_selectable_label_layout, root_list_viewport_builds_bounded_rows_with_absolute_ids_and_selection, root_list_geometry_rebuilds_only_when_layout_or_display_inputs_change, root_list_context_menu_owner_stays_with_its_absolute_row_offscreen and frozen list oracle as relevant. Small, independent review, local harness commit, exact-source Full36records720samples, independent gate before any runtime candidate. Candidate additionally covers zero/single/multiline/giant labels, early width, args-only presentation parity, first/middle/last selection and actual click/navigation/menu lifecycle. Shared cache storage change warrants existing grid eager smoke, not C2-E benchmark matrix. No fullsuite/release/native campaign. C2-D remains open until its accepted or measured-skip decision and scoped evidence are committed.

## Early independent method review

Method/scope review is sound, with two required boundaries now explicit. Prepare complete incoming Actions, Style and FontDefinitions outside timing, but execute set_fonts/set_style/set_pixels_per_point, full-path flag changes, result assignment/disposal/invalidation and actual viewport frame inside the owner. Deferred font realization remains timed. Baseline and any later candidate must match this boundary.

The existing eager helper receives only style/scale/width; it does not copy pixels-per-point or font definitions. Configure the independent oracle Context explicitly with the measured frame's effective ppp and FontDefinitions outside the clock. Obtain actual effective receipts postclock; never render or settle the measured fresh context to construct its oracle. Require actual old/new atlas allocation identity inequality for font replacement and actual key-bit changes for ppp/width/wrap, rather than setter calls alone. No addresses serialized.

Five/twenty-percent prefix is exact formatted presentation, including description suppression; the row at the declared cut must differ. Args-only full Action change belongs to identical-presentation replacement. Full-path toggle requires a deterministic Some alias and changed display text. First-wide no-wrap requires actual first-row response-width growth and next-row incoming-width/downstream accumulated-offset changes; an existing later long row may retain the global maximum. No production observer or cache change justified before characterization.

## Frozen characterization receipt inventory

Version 1 prefix C2D_LIST_RESULT, stage cold_list_geometry_characterization, installation eager_list_geometry. Nine numeric arrays: elapsed_ns, geometry_rebuilds, geometry_measured_rows, geometry_measured_cells, geometry_ns, painted_rows, presentation_prefix_len, geometry_capacity_before_bytes, geometry_capacity_after_bytes. Five hexadecimal identity arrays: full_output_state_ids, action_output_ids, presentation_ids, geometry_projection_ids, viewport_receipts. Each array contains exactly20 retained entries.

transition_receipts contains20 colon-separated tuples of24 decimal u64 values, in this order: old/new complete Action IDs; old/new result generations; old-key-present/new-key-present/key-changed; old-atlas-present/new-atlas-present/atlas-changed; old/new available-width bits; old/new ppp bits; old/new button-font signature IDs; old/new wrap flags; old/new full-path flags; old/new screen-width bits; old/new screen-height bits. No pointer addresses. Fresh first-open has absent old key/atlas, zero old key fields, and changed flags on absent-to-present transitions; its reusable presentation prefix is zero.

Default screen960x640, ppp1.0, wrap true, full paths false, aliases empty. First-wide recipe has wrap false both before and after. Its first description gains24 copies of the exact string ` early-wide-segment/`. Shared-prefix recipes leave floor(N*5/100) or floor(N*20/100) leading rows unchanged and append ` [C2-D display edit]` to every later label. Args-only replaces every args with Some(c2d-args-{index:05}). Last-row description gains ` [C2-D last-row edit]`. Only full-path recipe installs Some aliases every17th row and toggles full paths false to true. Separate wrap trueâ†’false, font replacement, ppp1.0â†’1.25 and viewport960â†’720 transitions retain all Actions. Font fixture clones Ubuntu-Light under an unused name and preserves family mapping; actual atlas replacement must be asserted. Identity framing is reviewed before execution, never inferred from emitted evidence.

Independent pre-execution method/reconstruction review: CLEAR. Python selects only allowlisted pure Signature/catalog/identity/payload AST definitions and executes no module-level native/path code. All36 bindings were generated before emitted evidence. Fresh recipe controls retain default old ppp/wrap/screen values for input hashing, while absent-old-key receipt fields width/ppp/font/wrap/fullpath are zero; collector must distinguish these domains. Actual font/oracle/key checks remain pending stable source review and execution.

Collector preflight: final protocol synthetic-v4 accepted1 fabricated positive and rejected21 negative controls (inventory, duplicates, provenance, fixture/Action/presentation binding, overflow, timing, work, prefix, capacity, sample count, atlas and finite-positive width validity). These are explicitly fabricated protocol controls, never timing evidence. Initial synthetic-v1 did not execute because child PowerShell script policy blocked loading; v2 onward used process-only ExecutionPolicy Bypass. Reviewer collector findings were corrected: settled old capacity >=24*N, warm capacity unchanged, atlas unchanged unless first-open/font/ppp recipe, actual-width bits finite positive f32 within u32. Final fixed metadata also includes fixture=synthetic_root_list_geometry and measurement_scope=installation+egui_Context_run+LauncherApp::render_root_frame.

Stable source review requires four bounded harness corrections before execution: first-row width recurrence instead of assuming final global maximum grows; explicit overlapping absolute widget-ID preservation/change and uniqueness; explicit fixed query/selection/valid/pending state assertions before semantic hashing; and non-owning old-atlas observation. A strong old-atlas Arc lease would defer potential destruction beyond the measured owner. A retained Weak allocation identity supports actual old/new comparison without suppressing payload disposal inside the owner; its control-block lease is test-only observation, no addresses emitted. This corrects the earlier retained-strong-Arc interpretation while preserving actual atlas transition proof. All fixture strings and pre-evidence bindings remain unchanged. No production source change authorized.
Pre-execution source/fixture/safety review CLEAR after all four fixes. Source is one additive test-only render.rs hunk; frozen helper/oracle bodies remain unchanged. Parent audited inert root_list_app_at, owned workspace lifecycle, disabled hotkeys/dashboard/plugins and actual measure_owner timing. Six exact fast-dev ordinary tests dispatched serially. First launcher attempt under Windows PowerShell5 stopped on Cargo's informational stderr before tests; no test/measurement result or raw file was produced. Retried under bundled PowerShell7 with the same six filters and source guard; compilation/test result pending. Source writes and Git commits remain paused throughout execution.

First actual PowerShell7 ordinary invocation stopped at compilation with two E0599 errors: pinned egui0.27 Fonts has no definitions() getter. Zero tests executed. Raw ordinary-v2.log retained. Fix is test-only explicit owned FontDefinitions configuration installed on measured Context before any frame and copied to oracle after measurement; replacement is prepared from that controlled definition and installed inside owner. Actual postframe atlas/ppp/key checks remain required. No unsupported introspection, dependency change or weaker oracle acceptance. Parent source search found no LauncherApp font installation overriding the fixture configuration. Ordinary verification will rerun after focused review, not broaden scope.

Focused explicit-font correction review CLEAR. Reviewer inspected pinned egui0.27.2 set_fonts/update_fonts_mut locally: set_fonts queues memory.new_font_definitions; font-cache clear, Fonts::new and preload execute in next frame. Explicit default setup therefore preserves genuinely cold first root frame, and atlas replacement realization remains inside owner. Independent oracle receives the same controlled effective definitions postclock. The same six ordinary tests were redispatched under PowerShell7 as ordinary-v3; no result yet.

Ordinary verification PASS: Nextest43f7c9e1-c6a8-4db6-85c5-6fe4bb8e1cb2, fast-dev build2m05s, execution10.471s, six exact tests executed and passed (5187 unrelated skipped). New ordinary recipe contract executed all12 recipes and three independent N100 literal binding triples; existing eager layout, bounded viewport/absolute selection, invalidation, offscreen menu ownership and frozen root-list oracle passed. Three pre-existing render warnings retained. Small12cases/240retained samples dispatched under guarded uncommitted source; no benchmark result yet. Source and HEAD remain frozen throughout runs.

## Small eager baseline â€” independent review pending

Nextest a15b4a66-fb09-4449-8cba-e1456b99fb6b; build0.88s, test128.573s, Nextest execution128.595s; 1/1 owner PASS,12records240retained samples,5warmups/20samples/0exclusions per case. Source truthfully UNCOMMITTED_SOURCE, fast-dev. Strict collection and parent independent semantic/quantile reconstruction CLEAR; independent reviewer disposition pending. No candidate comparison or optimization acceptance.

All timings below are nearest-rank p50/p95/max in milliseconds. Geometry is contained within the full owner; do not subtract quantiles to infer remaining-phase timings.

| Case (N100) | Full owner | Geometry | Painted rows |
|---|---|---|---|
| fresh_context_first_open | 15.5779/18.2804/19.5174 | 6.0514/6.6987/7.1436 | 26 |
| shared_prefix_5_percent | 7.1499/9.3997/9.4244 | 6.0521/7.7431/7.9654 | 25 |
| shared_prefix_20_percent | 5.9108/7.7884/8.5883 | 4.8182/6.4040/7.0510 | 26 |
| identical_presentation_new_generation | 5.1317/5.7852/5.9922 | 4.0394/4.6018/4.7773 | 26 |
| last_row_text_edit | 5.2252/5.7688/6.0649 | 4.1254/4.6112/5.0225 | 26 |
| first_row_width_expansion_no_wrap | 5.0000/6.7809/8.3325 | 3.8415/5.3345/6.3823 | 29 |
| full_paths_toggle | 5.3129/6.5138/6.8425 | 4.1389/5.2913/5.5197 | 26 |
| wrap_change | 6.0767/6.8156/7.3380 | 4.8160/5.6491/5.9446 | 29 |
| font_atlas_replacement | 13.6318/16.2087/17.2450 | 5.3803/6.5584/6.9334 | 26 |
| pixels_per_point_change | 13.6461/16.4679/16.8085 | 5.3129/6.3178/7.7375 | 21 |
| viewport_width_change | 6.3275/7.4472/9.2531 | 5.3130/6.2733/7.8136 | 24 |
| warm_repeat | 0.9975/1.1616/1.2760 | 0.0000/0.0000/0.0000 | 26 |

Every cold recipe rebuilds1/measures100rows/0gridcells; warm0/0/0 and geometry_ns0. All samples pass actual eager vectors/extents/visible absolute indices/rectangles, fixed semantic state and generation-dependent widget-ID assertions. First-open has no prior geometry capacity; every after capacity is2400bytes, six f32 list-vector capacities only, not allocator/RSS or total memory. Retained old capacity2400bytes for settled cases.

Collector correction retained in history: first Small collection rejected a width assumption, not a measurement failure. Pinned egui0.27.2 set_pixels_per_point delegates zoom; begin_frame_mut rescales previous screen and overrides incoming RawInput.screen_rect on that first transition. At1.0â†’1.25, prior960x640 becomes effective768x512; CentralPanel8pt margins yield available width752. Normal width944 and viewport720 width704 derive from the same source/margins. The ppp recipe is a composite first zoom/environment-transition CPU frame, not isolated steady/native DPI latency. Actual key/ppp/atlas receipts and raw requested screen controls remain recorded separately. No source/fixture/hash/measurement changed or benchmark rerun; final collector synthetic-v5 1positive21negative controls PASS.

Independent Small evidence disposition CLEAR for local harness commit and exact-source Full, not runtime acceptance. Reviewer compared raw log/JSON all180arrays3600entries/204scalars/24triplets and independently recomputed240semantic hashes with separate FNV implementation; all fixture/control/work/transition/privacy bindings PASS. Executed eager assertions cover geometry/viewport, not separate font-metric regeneration. Source remains one1191-line test-module addition, runtime and original helpers unchanged. C2-D remains IN_PROGRESS; Full36records720samples required before candidate decision. LOCAL ONLY.

Independent Small evidence disposition CLEAR for local harness commit and exact-source Full, not runtime acceptance. Reviewer compared raw log/JSON all180arrays3600entries/204scalars/24triplets and independently recomputed240semantic hashes with separate FNV implementation; all fixture/control/work/transition/privacy bindings PASS. Executed eager assertions cover geometry/viewport, not separate font-metric regeneration. Source remains one1191-line test-module addition, runtime and original helpers unchanged. C2-D remains IN_PROGRESS; Full36records720samples required before candidate decision. LOCAL ONLY.

Harness/Small source commit d57dab096711f32c7dc33cc874ebf2d940e6dd45 created locally after review and clean staged diff; original G0JSON SHA256 remains EE6C71106D08D46FCFAE06C1F7ACEEBD9E6131C4F27E54DF7AAC3106C5825599. Exact-source Full36records720samples dispatched as runca0b8d64-0cd9-4479-8ed1-2d1a57731e95, build0.87s; result pending. Source/HEAD remain frozen during execution. No runtime candidate authorized.

## Conditional candidate handoff refinement — read-only, Full gate pending

Planner refined one milestone on exact d57dab096711f32c7dc33cc874ebf2d940e6dd45 while Full runs; no source prototype or implementation authorized. RootListGeometryCache owns one previous successful list presentation/checkpoint and its actual geometry/environment/atlas validity. Keep warm exact-key/atlas return before all formatting/scans. Result invalidation preserves each existing generation increment and popup cleanup; repeated pre-frame invalidations retain at most one useful previously measured projection, never an empty/unmeasured replacement. Explicit display and list/grid mode invalidation purge/reject reuse, including conservative alias reload semantics.

Cold ensure requires precise environment/atlas equality, compares complete ordered formatted text only until first mismatch, reuses the proven prefix and resumes original f32 spacing/y/width/x recurrence for every affected suffix. Result count/generation may change; args-only may have identical presentation but still requires new generation/widget IDs/popup handling. No cloned full Actions, multigeneration lookup, later matching-row reuse, grid algorithm change or force-eager behavior switch. Initial storage creation and all timed installation/invalidation/format/compare/storage/disposal costs count inside full owner. Actual galley count is suffix length; commit count stays distinct and can be1 with0 measured rows. Partial logical storage reporting includes old/new overlap, string lengths/capacities and geometry/checkpoint capacities, not allocator/RSS.

Required stage migration: new candidate emitter/collector/ordinary work expectations; all baseline artifacts/collector remain immutable. C2-D cold N assertions intentionally become N-prefix only for eligible replacements, otherwise N. C2-C track_c2c_validate_owner changed-provider recipe changes args only: candidate may reuse every presentation row while preserving full Action/provider/generation/eager assertions; its historical N+1 measurement expectation must migrate explicitly, never silently relabel historical timing. Original C0 display-invalidation fixture still requires N and original C0/C2-A eager oracle bodies remain unchanged. Focus candidate tests on exact existing recipes plus bounded empty/single/stress/selection/click/navigation/popup/repeated-invalidation behavior and one existing grid eager smoke for shared storage. Candidate scope/GO awaits independently reviewed Full baseline and explicit parent decision.

## Exact-source Full eager baseline — independently CLEAR

Source d57dab096711f32c7dc33cc874ebf2d940e6dd45; runca0b8d64-0cd9-4479-8ed1-2d1a57731e95; build0.87s/test1016.438s/Nextest execution1016.469s. 1/1 ownerPASS,36records720retained samples,5warmups/20samples/0exclusions per recipe. Full test wall time includes untimed fixture setup/eager validation; owner elapsed_ns includes only installation/environment mutation plus actual CPU frame. Strict collection and parent independent720semantic hashes/72nearest-rank triplets PASS. Small12-case non-timing overlaps all156arrays3120entries/180scalars match, excluding only elapsed_ns/geometry_ns/statistics/source/mode. Independent review CLEAR: 540 arrays/10800 entries,612 scalars,72 timing triplets and720 separately recomputed semantic hashes; Small overlaps match exactly. No runtime optimization acceptance.

Full owner and geometry p50/p95/max below are milliseconds. All adverse first-open/environment/tail cases retained.

| N | Case | Full owner | Geometry | Prefix | Painted |
|---|---|---|---|---|---|
| 100 | fresh_context_first_open | 15.0359/21.3780/22.6060 | 5.8329/8.1726/8.3729 | 0 | 26 |
| 100 | shared_prefix_5_percent | 6.9356/7.9522/8.4348 | 5.8842/6.7016/7.1090 | 5 | 25 |
| 100 | shared_prefix_20_percent | 6.0069/8.1545/8.2827 | 4.8884/6.6901/6.8674 | 20 | 26 |
| 100 | identical_presentation_new_generation | 5.3144/7.5893/8.0235 | 4.1860/6.0984/6.4411 | 100 | 26 |
| 100 | last_row_text_edit | 5.2871/7.3917/8.5755 | 4.1909/6.0954/6.9834 | 99 | 26 |
| 100 | first_row_width_expansion_no_wrap | 5.2027/6.0846/6.3408 | 4.0430/4.7434/4.8284 | 0 | 29 |
| 100 | full_paths_toggle | 5.3826/6.1948/6.3772 | 4.2117/4.8432/5.1554 | 0 | 26 |
| 100 | wrap_change | 6.1489/6.8629/9.4290 | 4.9676/5.6571/7.7835 | 100 | 29 |
| 100 | font_atlas_replacement | 13.7910/18.1080/18.9938 | 5.4571/8.2472/8.3906 | 100 | 26 |
| 100 | pixels_per_point_change | 14.3893/17.4319/18.9928 | 5.5404/6.6376/7.0972 | 100 | 21 |
| 100 | viewport_width_change | 6.3110/7.6116/8.0185 | 5.3231/6.4386/6.7132 | 100 | 24 |
| 100 | warm_repeat | 1.0446/1.2410/1.2816 | 0.0000/0.0000/0.0000 | 100 | 26 |
| 1000 | fresh_context_first_open | 65.8638/77.0927/92.7477 | 55.2815/61.9538/77.3898 | 0 | 26 |
| 1000 | shared_prefix_5_percent | 61.7244/69.2664/69.4899 | 60.1979/67.8458/68.0328 | 50 | 26 |
| 1000 | shared_prefix_20_percent | 60.1792/66.7475/72.9068 | 58.7584/65.3174/71.4041 | 200 | 26 |
| 1000 | identical_presentation_new_generation | 53.7778/59.5942/62.9444 | 52.3175/58.2049/61.4420 | 1000 | 26 |
| 1000 | last_row_text_edit | 53.6438/63.1035/64.3137 | 52.2891/61.5824/62.7767 | 999 | 26 |
| 1000 | first_row_width_expansion_no_wrap | 49.3637/50.8247/53.7297 | 47.7269/49.2322/51.9948 | 0 | 29 |
| 1000 | full_paths_toggle | 54.8761/58.6501/61.7303 | 53.8165/57.4987/60.6181 | 0 | 26 |
| 1000 | wrap_change | 50.8487/58.7502/59.1593 | 49.5755/57.0168/57.8493 | 1000 | 29 |
| 1000 | font_atlas_replacement | 61.8384/68.6881/79.2295 | 53.4295/60.7604/68.2087 | 1000 | 26 |
| 1000 | pixels_per_point_change | 62.2897/70.4497/86.0640 | 53.4124/58.5900/71.1502 | 1000 | 21 |
| 1000 | viewport_width_change | 54.0588/60.5037/65.2137 | 52.8890/59.3536/64.0094 | 1000 | 24 |
| 1000 | warm_repeat | 1.0027/1.3132/1.3140 | 0.0000/0.0000/0.0000 | 1000 | 26 |
| 10000 | fresh_context_first_open | 604.8952/729.1262/820.5416 | 561.7572/683.1177/732.0431 | 0 | 26 |
| 10000 | shared_prefix_5_percent | 628.1134/651.4424/765.8397 | 623.5481/646.8816/760.4302 | 500 | 26 |
| 10000 | shared_prefix_20_percent | 621.6263/677.3523/704.4039 | 617.0762/671.9601/699.7216 | 2000 | 26 |
| 10000 | identical_presentation_new_generation | 551.6320/598.8862/616.5687 | 547.2865/594.4309/611.8311 | 10000 | 26 |
| 10000 | last_row_text_edit | 557.1789/598.7135/619.2397 | 552.7768/594.2242/614.7955 | 9999 | 26 |
| 10000 | first_row_width_expansion_no_wrap | 499.4731/573.6285/579.9358 | 494.6728/569.1347/574.8740 | 0 | 29 |
| 10000 | full_paths_toggle | 561.3349/609.7567/621.3020 | 560.2186/608.3862/620.1280 | 0 | 26 |
| 10000 | wrap_change | 498.9457/518.8426/530.3989 | 497.7993/517.6845/529.2479 | 10000 | 29 |
| 10000 | font_atlas_replacement | 558.9813/600.6273/718.2097 | 550.5030/592.3912/666.5011 | 10000 | 26 |
| 10000 | pixels_per_point_change | 560.0683/576.9729/583.1194 | 550.9472/568.3305/573.9492 | 10000 | 21 |
| 10000 | viewport_width_change | 551.8365/617.4324/630.6484 | 550.7143/616.2937/629.0716 | 10000 | 24 |
| 10000 | warm_repeat | 1.0519/1.5997/1.7189 | 0.0000/0.0000/0.0000 | 10000 | 26 |

Every cold sample rebuilds1/measuresN list rows/0gridcells; warm0/0/0 with geometry_ns0. Actual all24transition fields and complete Action/presentation/state bindings pass; first-open before capacity0, every settled before and every after capacity24*N bytes (2400/24000/240000), six geometry f32 vectors only. Environment-only recipes may have full presentation prefix but cannot reuse geometry across changed key/atlas. Source-confirmed composite ppp transition and actual effective widths remain explicit. No independent font-metric regeneration, production overlap frequency, application-wide speedup, native composition or allocator/RSS claim.

## Parent experiment decision

Independent reviewer and parent GO for ONE bounded previous-presentation/prefix-checkpoint candidate after this evidence commit. Actual 10k prefixes500/2000 and dominant geometry costs justify measuring the hypothesis. The refined packet above is adopted with exact environment/atlas matching, unchanged warm early return, complete f32 recurrence and generation/popup behavior. Candidate formatting/comparison/initial storage/disposal and old/new logical overlap must be measured inside the full owner. Baseline source d57dab096711f32c7dc33cc874ebf2d940e6dd45, collector and evidence stay immutable. Use distinct candidate protocol; migrate affected C2-C args-only work assertions explicitly. KEEP requires exact eager geometry/state/interaction parity, lower full-owner cold p95 for 5–20% prefixes, no material warm regression and bounded disclosed retention. Otherwise retain evidence and close as measured SKIP. All Cargo terminal;14/29 closed, C2-D still IN_PROGRESS. LOCAL ONLY.
