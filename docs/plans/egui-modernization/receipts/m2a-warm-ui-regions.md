# M2-A archive-only warm UI regions

Status: v3 release artifact, exact no-body listing and single diagnostic evidence independently accepted. Live application source remains79653eca; production Full ROOT performance remains FAIL.

Plan checkpoint:f34d10db. Fresh ignored candidate: `target/egui-modernization/m2a/warm-ui-regions-001/root`, copied from the accepted frozen79653eca root. Only archived `src/gui/render.rs` differs. Manifests, lockfile, vendors and font policy remain unchanged.

The source adds a local test-only carrier and shared timestamps for visible prelude/menu, entire CentralPanel call and post-panel auxiliary tail. Existing outer lifecycle clocks and LauncherRowsBuilt remain in both modes; the row metric is nested within central. Hidden and OCR early returns and QA-finish order are preserved. Both modes take the same render path; disabled mode creates no new Instants. The derived owner is `gui::render::tests::track_c_diagnostic_root_list_warm_ui_regions`, with the approved counterbalanced14observations,2warmups/5samples per mode, fixed100actions/960x640/nine-pass preparation and accepted fixture/identity/work contracts.

Initial source SHA `52c3684a71072ca2c3dab857317d17db41cdae18ddfd71e8cc6edae749c2db90`; baseline frozen render SHA `83d05c04b9233bd90f2480a1f77c50394f32cd97c736571fd9fd8d7837e3b24c`. Initial patch `receipts/render-ui-regions.patch` SHA `0f80e91ebc89b636c1a0205cc365471e0b34b6ca1f5711e3f2349e7d64d7d285`; initial provenance `receipts/source-preparation.json` SHA `a4f3edde7b0e16419b1cef07de5993af3fb9907d1f4c016b318aa7710228c026`. Parent independently matched initial source/base/patch and five unchanged manifest/vendor bindings. Scoped rustfmt check passed; compiler/type verification remains for the separately gated canonical no-run build.

Independent review found two P2 diagnostic-contract gaps: disabled mode computed negative central-minus-rows although central time was unavailable; metric work validation admitted any1–64 rather than requiring the accepted25painted rows. Corrections will make subtraction enabled-only with a documented zero off sentinel, and add exact metric-work/paint equality while preserving raw delta fields and existing bounds. Initial patch/provenance remain as history; corrected source receives fresh v2 bindings. No body was consumed by these findings.

The standard patch helper could not write through the ignored target reparse point; the writer used explicitly scoped Python writes to this candidate. No live source, dependency, build or body change.

A separate narrow parent inspection of `font_revision::active_revision` confirms warm geometry reads a stored scalar through ctx.data; font-definition comparisons occur only in installation. No per-frame font-data hashing was found there. No source change follows from this check.

Next gates remain source re-review, one canonical release no-run artifact/exact owner listing review, then one off/on diagnostic invocation. Base commit, instrumented render digest and probe executable must stay distinct. This experiment cannot establish original phase parity, global observer overhead or production performance acceptance. No optimizer/source adoption, extra owner, per-widget cascade, original/Full repetition or profiler route is authorized. Both mission tracks remain required; checkpoints stay local.


## Corrected source gate accepted

Final archived render.rs SHA `4d722c7afb15ac242f65433335004323ce8ccf346c7b3481a2db00401932a47a`,1,028,848bytes. V2 patch `receipts/render-ui-regions-v2.patch` SHA `b0cb0349082a053e997e9e9fbf1949bfe3973a460d238bc6715d54c12cf0c91b`; v2 provenance `receipts/source-preparation-v2.json` SHA `a3887e2506f122d21cae714b928be00e779408254494ca1b7489ee44827d2c63`. Disabled central-minus-rows is now documented0/valid=false; subtraction and containment apply only when enabled. Metric work equals25painted rows, retaining prior bounds and raw before/after fields. Both P2 findings are resolved. Parent rehashed final source/v2patch/five unchanged bindings and v1history; independent re-review accepted source4D722/patchB0CB/receiptA388. Scopedformat0,24,105CRLF/zero loneLF; no build or body.

The forthcoming instrumented artifact must identify base commit79653eca separately from probe render digest4D722 and its executable. For the derived owner, the protocol source field may use that instrumented-render SHA256 as a file marker, with base commit recorded separately; it is not an unmodified production Git-source claim. Canonical artifact/profile/executable/listing review remains required before a single off/on body.


Corrected archive source gate saved locallyda8a02ff. Solewriter activated canonical INSTRUMENTED no-run artifact/listing only for warm-ui-regions-001: frozenrender4D722/v2patchB0CB/provenanceA388, base796 separate, freshisolatedrelease target/defaultfeatures/stableMSVC/directnativeCargoJSON. Source/compiler/exe/profile/exactderivedowner listing review precedes any off/on body. No live source/private11/Full/originalrepeat or productionadoption. Compilerfeedback, if any, must preservefailures and refreshsourcebindings before execution. Actual build start/result pending; no body. Bothmissiontracks/performanceFAIL unchanged.


## Canonical instrumented build started

The single no-run release build started at 2026-10-14T14:01:15.551742Z, Cargo PID 35328, writer-owned session 6919. Its input receipt is `target/egui-modernization/m2a/production-warm-ui-regions-001/receipts/canonical-instrumented-no-run-001.input.json`, SHA256 `0ac1256df8f41a9a5fbccb2821e230baf40160b46d8156fb4dbc6883a69f2e13`. The command uses locked/offline release, default features, lib/no-run and a fresh isolated target. Parent independently matched the reviewed render, patch and preparation hashes plus all five unchanged base/candidate bindings. Independent launch/input review accepted the clean child environment and command intent without findings. Completion, actual Cargo-selected executable, profile and exact diagnostic listing remain pending. No body has run.

A separate read-only cold-frame investigation established that both original and current preparation evict unused offscreen full galleys before the timed cold frame. Both prepare nine real frames, construct the eager oracle in a separate Context and invalidate only display geometry. Both font caches retain current-generation entries and advance their generation at frame/pass start. Original offscreen full layouts from the first preparation frame therefore expire; current ordinary measurement misses never insert them. Visible widgets refresh normal full entries. This rules out retained original offscreen galleys as an explanation, but attributes no timing cost and justifies no cache change.


## First compiler feedback

The no-run build ended native101 after384.751s. Raw Cargo stdout547,359bytes SHA `12fbd5d6099d473a67ae71e9873d524406305dbe30bd5265a78dc1a5f2708074`; stderr13,207bytes SHA `13610c465a9ac3b68e35b359308c2bb9fa36a37049cdbff7a79e64189f6267d0`. Parent independently read two E0308 errors in test-only containment assertions: region/lifecycle nanoseconds are u128 and metric nanoseconds are u64. The scoped correction widens only comparison operands with u128::from, preserving stored fields and timing semantics. Source v3 review must precede the smallest same-target incremental no-run recheck. No executable selection, listing or body occurred.

Cold cache references: original f45 source uses track_c_prepare_root_list_state/invalidate_root_list_display_geometry; registry epaint0.27.2 text/fonts.rs begin_frame396/flush414/flush_cache722 and egui context.rs555. Current immutable796 source render.rs preparation7285/invalidation2154/oracle5554; vendor epaint text/fonts.rs begin_pass728/flush742/measure1178/flush_cache1322 and egui context.rs596. These establish lifetime semantics only.


V3 source correction independently accepted without findings: render SHA `63da1f29c9002094a44ad3d7d33ac176be4f47f9f92b23366cbe58f8a4020f83`; full patch `16d6b9a420ec99be78f655e13e49c92678dda2197a47f46b9412c7e086f98f6f`; provenance `d2a9a73ce7e5741fb7ca4dca6b619db5b640b886fade44cafa7468ac1c2a1145`. Only the two lossless comparison conversions differ from v2. Scoped format passed. Same-target incremental no-run recheck and, on success, one exact no-body owner listing are activated. Artifact review remains required before any measurement. The final protocol marker must identify63DA instrumented render, with base796 separate.


## Incremental release artifact completed

Attempt002 reused the same target and completed native0 in418.6588628s. Its input SHA is `58d2ea8cbd778997a0a2b661638e286d07965765bd3b5ddadb191297d322859b`; result `08cd9bc651c172dc4d258634e5565d305721111e26b7cb9c6fa74c1b912cdddc7`. Raw stdout540,097bytes SHA `07797855d0b82ac7645354716214549dab65ac1acfdae114324644f22c31c919`; stderr184bytes SHA `5cf182b362e28fd7aba7dccc3109e72a46ade20672eec5cd53f8e3fd47e56b0b`. Parent parsed523CargoJSONmessages and exactly one newly compiled libtest: opt3/debug0/assertfalse/overflowfalse/testtrue/features[].

The preserved instrumented executable is `receipts/current-release-multi_launcher-libtest.exe`,87,162,880bytes, SHA `c60c18585d02b154e80375bfbd5a4ec3fab7327b0c092f34f55adeeac5e14c04`. Exactly one no-body listing completed native0 with the expected ignored owner once; stdout93bytes SHA `6ad8c4d164944be386f86f451536722408d39e96c74e89920b7b1e58c065fb5b`, empty stderr. Listing result `9d3bc4eda8ae9bda17091c1b447f1f566a81e419b6d9cfb32d86e92269bb11bf`; artifact gate `9039ede4d0e7628163d292bbf432179aa4057dbbc65506c964f47fe1d2e5e39c`. Parent matched executable/raw logs and eleven bound receipt hashes. Independent completed-artifact review is pending. No diagnostic body has run.


Independent completed-artifact review accepted without findings: source63DA, gate9039, actual release executableC60C, single exact no-body listing and preserved first failure. The next action is the separately activated single counterbalanced diagnostic body under the accepted packet; no production performance acceptance follows from artifact success.


## Single diagnostic result

The single authorized body ran from `production-warm-ui-regions-001/body-runs/warm-ui-regions-body-002`, native0/0.8050994s, one passed owner. It retained14records: four warmups and ten measured observations in the reviewed counterbalanced order. Instrumented render63DA and executableC60C matched before/after. Preparation attempt001 stopped before invocation on a pre-created output directory; its failure remains preserved and consumed no body.

Evidence hashes: input `cc301e2e94c5a62ccbc363eb0eeccc89a2082d166c06650f9f3c65088fec76be`; native result `92e13548c737ca772db0dba0576d389ad657710a2dff61cb2c5bca2c69bb73e4`; stdout `a0aee59d133d78427d4237260c9995e7543a388508a33524167209efe8d8e5bf`; stderr `383b456263df6482653d3359a839747a8d754764224e7e39be0e9f93db96be92`; parsed `cc68a7ae24e585be5d90867f442b1840e367591a181f84f832787bd16d33ac36`; validation `191e7d47ebb3c761bc9e72d13380734f31e2cc4b0c4d20384983673812298fe5`; sibling runner `d9f1e6fe3934ead71ac96a733d67ce0498457bb05da8ffe8e59d005ad0eb263e`. Parent independently parsed raw14records, exact order, IDs/fixture,0rebuild/0measure/25paint/onepass, metric1call25work, before/after deltas, phase/residual and region/gap arithmetic, enabled containment and disabled unavailable sentinel. All pass; independent actual-result review pending.

Ordered five-sample arrays in nanoseconds:

| Interval | Clocks disabled | Clocks enabled | Enabled p50/p95 |
|---|---|---|---|
| Owner |434400,293500,275400,254400,270100|423900,282300,476100,329900,279600|329900/476100|
| UI |369900,255500,243600,218200,229900|376900,242200,401300,279000,233700|279000/401300|
| Prelude |0,0,0,0,0|36500,36700,71700,46100,36900|36900/71700|
| Central panel |0,0,0,0,0|245200,138400,252500,168400,140400|168400/252500|
| Auxiliary tail |0,0,0,0,0|94600,66700,76100,64100,55800|66700/94600|
| Central minus nested rows |unavailable zero sentinel|37900,27800,46700,38700,30600|37900/46700|
| Nested rows |205200,109700,107500,108000,111400|207300,110600,205800,129700,109800|129700/207300|

The tail is a material current UI responsibility for bounded source inspection. These are descriptive values with all outliers retained; do not add independent medians, double-count nested rows, claim global observer overhead or original phase parity, or accept production performance. Live source796 and Full ROOT FAIL remain unchanged.


Independent actual-result review accepted without findings: all14ordered records/every raw field, env/bindings, work/IDs and arithmetic pass. Enabled median prelude36.9us/central168.4us/tail66.7us/central-minus-rows37.9us are descriptive partitions, never additive medians. Variation prevents precise observer-overhead attribution. Planner activated only bounded read-only post-panel closed-dialog/overlay lifecycle inspection, including original corresponding tail; no repair or new measurement is yet authorized.
